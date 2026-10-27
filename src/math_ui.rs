//! Offline LaTeX math rendering, cached per UI context and performed off-thread.
use eframe::egui;
use std::{
    collections::VecDeque,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, AtomicUsize, Ordering},
        mpsc::{self, SyncSender, TrySendError},
    },
    time::Instant,
};

const MAX_SOURCE_BYTES: usize = 4096;
const MAX_SVG_BYTES: usize = 2 * 1024 * 1024;
const MAX_WIDTH: f32 = 8192.0;
const MAX_HEIGHT: f32 = 4096.0;
const MAX_PIXELS: f32 = 4_000_000.0;
const CACHE_ENTRIES: usize = 128;

pub struct Formula {
    pub uri: String,
    pub size: egui::Vec2,
}

#[derive(Clone, PartialEq, Eq)]
struct Key {
    source: String,
    display: bool,
    font_bits: u32,
    color: [u8; 4],
}

#[derive(Clone)]
enum State {
    Pending,
    Ready {
        uri: String,
        size: egui::Vec2,
        micros: u64,
    },
    Failed(String),
}

struct Entry {
    key: Key,
    uri: String,
    state: State,
}

struct Job {
    key: Key,
    uri: String,
    ctx: egui::Context,
}

struct Renderer {
    jobs: SyncSender<Job>,
    cache: Arc<Mutex<VecDeque<Entry>>>,
    renders: Arc<AtomicUsize>,
    startup_error: Option<String>,
}

fn check_size(width: f32, height: f32) -> Result<egui::Vec2, String> {
    if !width.is_finite() || !height.is_finite() || width <= 0.0 || height <= 0.0 {
        return Err("The formula has invalid or empty dimensions.".into());
    }
    if width > MAX_WIDTH || height > MAX_HEIGHT || width * height > MAX_PIXELS {
        return Err("The formula is too large to render safely.".into());
    }
    Ok(egui::vec2(width, height))
}

fn render(key: &Key) -> Result<(String, egui::Vec2), String> {
    use ratex_layout::{LayoutOptions, layout, to_display_list, validate_layout};
    use ratex_svg::{SvgColorSyntax, SvgOptions, render_to_svg_with_color_syntax};
    use ratex_types::{color::Color, math_style::MathStyle};

    if key.source.len() > MAX_SOURCE_BYTES {
        return Err("The formula exceeds the 4096-byte rendering limit.".into());
    }
    let font_size = f32::from_bits(key.font_bits);
    if !font_size.is_finite() || font_size <= 0.0 {
        return Err("The formula font size is invalid.".into());
    }
    let ast = ratex_parser::parse(&key.source).map_err(|error| error.to_string())?;
    let style = if key.display {
        MathStyle::Display
    } else {
        MathStyle::Text
    };
    let [r, g, b, a] = key.color.map(|channel| f32::from(channel) / 255.0);
    let options = LayoutOptions::default()
        .with_style(style)
        .with_color(Color::new(r, g, b, a));
    let boxed = layout(&ast, &options);
    validate_layout(&boxed)?;
    // The SVG backend writes point units; usvg converts those to CSS pixels.
    let svg_options = SvgOptions {
        font_size: f64::from(font_size) * 0.75,
        padding: 1.5,
        stroke_width: 0.75,
        embed_glyphs: true,
        font_dir: String::new(),
    };
    let padded_width = boxed.width * f64::from(font_size) + 4.0;
    let padded_height = (boxed.height + boxed.depth) * f64::from(font_size) + 4.0;
    check_size(padded_width as f32, padded_height as f32)?;
    let items = to_display_list(&boxed);
    let svg = render_to_svg_with_color_syntax(&items, &svg_options, SvgColorSyntax::Rgb);
    if svg.len() > MAX_SVG_BYTES {
        return Err("The formula exceeds the SVG rendering limit.".into());
    }
    // Missing outlines otherwise silently become text requiring external fonts.
    if svg.contains("<text") || svg.contains("<image") {
        return Err("This formula contains a glyph without a vector outline.".into());
    }
    let tree =
        usvg::Tree::from_str(&svg, &usvg::Options::default()).map_err(|error| error.to_string())?;
    let size = tree.size();
    let natural = check_size(size.width(), size.height())?;
    Ok((svg, natural))
}

impl Renderer {
    fn new() -> Self {
        let (jobs, incoming) = mpsc::sync_channel::<Job>(8);
        let cache = Arc::new(Mutex::new(VecDeque::<Entry>::new()));
        let worker_cache = Arc::clone(&cache);
        let renders = Arc::new(AtomicUsize::new(0));
        let worker_renders = Arc::clone(&renders);
        let startup_error = std::thread::Builder::new()
            .name("rustidian-math".into())
            .stack_size(8 * 1024 * 1024)
            .spawn(move || {
                while let Ok(job) = incoming.recv() {
                    let start = Instant::now();
                    let result = std::panic::catch_unwind(|| render(&job.key))
                        .unwrap_or_else(|_| Err("Unable to render this formula.".into()));
                    worker_renders.fetch_add(1, Ordering::Relaxed);
                    let mut cache = worker_cache
                        .lock()
                        .unwrap_or_else(|error| error.into_inner());
                    // URI identity prevents an evicted job from replacing a newer request.
                    if let Some(entry) = cache
                        .iter_mut()
                        .find(|entry| entry.key == job.key && entry.uri == job.uri)
                    {
                        entry.state = match result {
                            Ok((svg, size)) => {
                                job.ctx.include_bytes(job.uri.clone(), svg.into_bytes());
                                State::Ready {
                                    uri: job.uri,
                                    size,
                                    micros: start.elapsed().as_micros() as u64,
                                }
                            }
                            Err(error) => State::Failed(error),
                        };
                    }
                    drop(cache);
                    // Also wakes a caller whose submission met the full bounded queue.
                    job.ctx.request_repaint();
                }
            })
            .err()
            .map(|error| format!("Unable to start the math renderer: {error}"));
        Self {
            jobs,
            cache,
            renders,
            startup_error,
        }
    }

    fn request(&self, ctx: &egui::Context, key: Key) -> State {
        if let Some(error) = &self.startup_error {
            return State::Failed(error.clone());
        }
        let mut cache = self.cache.lock().unwrap_or_else(|error| error.into_inner());
        if let Some(index) = cache.iter().position(|entry| entry.key == key) {
            let entry = cache.remove(index).unwrap();
            let state = entry.state.clone();
            cache.push_back(entry);
            return state;
        }
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let uri = format!("bytes://math/{}.svg", NEXT.fetch_add(1, Ordering::Relaxed));
        let state = match self.jobs.try_send(Job {
            key: key.clone(),
            uri: uri.clone(),
            ctx: ctx.clone(),
        }) {
            Ok(()) => State::Pending,
            // A queued job's completion repaints; no timer or idle polling is needed.
            Err(TrySendError::Full(_)) => return State::Pending,
            Err(TrySendError::Disconnected(_)) => {
                State::Failed("The math renderer is unavailable.".into())
            }
        };
        if cache.len() >= CACHE_ENTRIES
            && let Some(Entry {
                state: State::Ready { uri, .. },
                ..
            }) = cache.pop_front()
        {
            ctx.forget_image(&uri);
        }
        cache.push_back(Entry {
            key,
            uri,
            state: state.clone(),
        });
        state
    }
}

pub fn image(
    ui: &egui::Ui,
    source: &str,
    display: bool,
    font_size: f32,
    color: egui::Color32,
) -> Result<Option<Formula>, String> {
    // Reject oversized input before copying it into a cache key or starting a worker.
    if source.len() > MAX_SOURCE_BYTES {
        return Err("The formula exceeds the 4096-byte rendering limit.".into());
    }
    if source.trim().is_empty() {
        return Err("The formula is empty.".into());
    }
    if !font_size.is_finite() || font_size <= 0.0 {
        return Err("The formula font size is invalid.".into());
    }
    let renderer = ui.ctx().data_mut(|data| {
        Arc::clone(
            data.get_temp_mut_or_insert_with(egui::Id::unique("math-renderer"), || {
                Arc::new(Renderer::new())
            }),
        )
    });
    let state = renderer.request(
        ui.ctx(),
        Key {
            source: source.into(),
            display,
            font_bits: font_size.to_bits(),
            color: color.to_srgba_unmultiplied(),
        },
    );
    crate::ui_probe::metric(
        ui.ctx(),
        "math-render-count",
        renderer.renders.load(Ordering::Relaxed) as f32,
    );
    match state {
        State::Pending => Ok(None),
        State::Failed(error) => Err(error),
        State::Ready { uri, size, micros } => {
            crate::ui_probe::metric(ui.ctx(), "math-render-us", micros as f32);
            Ok(Some(Formula { uri, size }))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(source: &str) -> Key {
        Key {
            source: source.into(),
            display: true,
            font_bits: 18.0_f32.to_bits(),
            color: [230, 230, 230, 255],
        }
    }

    #[test]
    fn common_math_has_self_contained_visible_outlines() {
        for source in [
            r"\frac{a+b}{2}",
            r"\sqrt{x^2+1}",
            r"\int_0^\infty e^{-x}\,dx",
            r"\begin{pmatrix}a&b\\c&d\end{pmatrix}",
            r"{\overline{x}}+\hat{y}+\vec{v}",
        ] {
            let (svg, size) = render(&key(source)).unwrap_or_else(|error| {
                panic!("{source}: {error}");
            });
            assert!(svg.contains("<path"), "{source} has no visible outlines");
            assert!(!svg.contains("<text"), "{source} depends on external fonts");
            assert!(size.x > 4.0 && size.y > 4.0, "{source} is empty");
        }
    }

    #[test]
    fn malformed_deep_and_oversized_math_are_rejected() {
        assert!(render(&key(r"\definitelyUnknownRustidianCommand{x}")).is_err());
        let deep = format!("{}x{}", "{".repeat(64), "}".repeat(64));
        assert!(render(&key(&deep)).is_err());
        assert!(
            render(&key(&"x".repeat(MAX_SOURCE_BYTES + 1)))
                .unwrap_err()
                .contains("4096-byte")
        );
        assert!(
            render(&key(r"\rule{500em}{2em}"))
                .unwrap_err()
                .contains("too large")
        );
        assert!(
            render(&key(r"\rule{200em}{200em}"))
                .unwrap_err()
                .contains("too large")
        );
    }
}
