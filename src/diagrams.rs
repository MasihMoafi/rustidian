//! Native Mermaid rendering, once per source/theme, away from the UI thread.
use eframe::egui;
use std::{
    collections::VecDeque,
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicU64, AtomicUsize, Ordering},
        mpsc::{self, SyncSender},
    },
    time::{Duration, Instant},
};

#[derive(Clone)]
enum State {
    Pending,
    Ready {
        uri: String,
        micros: u64,
        natural: egui::Vec2,
    },
    Failed(String),
}

struct Entry {
    source: String,
    dark: bool,
    state: State,
}
struct Job {
    source: String,
    dark: bool,
    uri: String,
    ctx: egui::Context,
}
struct Renderer {
    jobs: SyncSender<Job>,
    cache: Arc<Mutex<VecDeque<Entry>>>,
    renders: Arc<AtomicUsize>,
}

fn render(source: &str, dark: bool) -> Result<String, String> {
    let mut options = mermaid_rs_renderer::RenderOptions::mermaid_default();
    if dark {
        options.theme = mermaid_rs_renderer::Theme::dark();
    }
    options.theme.font_family = "Inter".into();
    mermaid_rs_renderer::render_with_options(source, options).map_err(|error| error.to_string())
}

fn svg_paths(svg: &str) -> Result<(String, egui::Vec2), String> {
    static FONTS: OnceLock<Arc<usvg::fontdb::Database>> = OnceLock::new();
    let fonts = FONTS.get_or_init(|| {
        let mut fonts = usvg::fontdb::Database::new();
        for bytes in [
            include_bytes!("../assets/fonts/Inter-Regular.otf").as_slice(),
            include_bytes!("../assets/fonts/Inter-Bold.otf").as_slice(),
            include_bytes!("../assets/fonts/NotoSansArabic-Regular.ttf").as_slice(),
        ] {
            fonts.load_font_data(bytes.to_vec());
        }
        fonts.set_sans_serif_family("Inter");
        Arc::new(fonts)
    });
    let options = usvg::Options {
        font_family: "Inter".into(),
        fontdb: Arc::clone(fonts),
        ..Default::default()
    };
    usvg::Tree::from_str(svg, &options)
        .map(|tree| {
            let size = tree.size();
            (
                tree.to_string(&usvg::WriteOptions::default()),
                egui::vec2(size.width(), size.height()),
            )
        })
        .map_err(|error| error.to_string())
}

impl Renderer {
    fn new() -> Self {
        let (jobs, incoming) = mpsc::sync_channel::<Job>(2);
        let cache = Arc::new(Mutex::new(VecDeque::<Entry>::new()));
        let rendered_cache = Arc::clone(&cache);
        let renders = Arc::new(AtomicUsize::new(0));
        let rendered_count = Arc::clone(&renders);
        std::thread::spawn(move || {
            while let Ok(job) = incoming.recv() {
                let start = Instant::now();
                let result = std::panic::catch_unwind(|| {
                    render(&job.source, job.dark).and_then(|svg| svg_paths(&svg))
                })
                .unwrap_or_else(|_| Err("Unable to render this diagram.".into()));
                rendered_count.fetch_add(1, Ordering::Relaxed);
                let mut cache = rendered_cache
                    .lock()
                    .unwrap_or_else(|error| error.into_inner());
                if let Some(entry) = cache
                    .iter_mut()
                    .find(|entry| entry.source == job.source && entry.dark == job.dark)
                {
                    entry.state = match result {
                        Ok((svg, natural)) => {
                            job.ctx.include_bytes(job.uri.clone(), svg.into_bytes());
                            State::Ready {
                                uri: job.uri,
                                micros: start.elapsed().as_micros() as u64,
                                natural,
                            }
                        }
                        Err(error) => State::Failed(error),
                    };
                }
                drop(cache);
                job.ctx.request_repaint();
            }
        });
        Self {
            jobs,
            cache,
            renders,
        }
    }

    fn request(&self, ctx: &egui::Context, source: &str, dark: bool) -> State {
        let mut cache = self.cache.lock().unwrap_or_else(|error| error.into_inner());
        if let Some(index) = cache
            .iter()
            .position(|entry| entry.source == source && entry.dark == dark)
        {
            let entry = cache.remove(index).unwrap();
            let state = entry.state.clone();
            cache.push_back(entry);
            return state;
        }
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let uri = format!(
            "bytes://mermaid/{}.svg",
            NEXT.fetch_add(1, Ordering::Relaxed)
        );
        let job = Job {
            source: source.into(),
            dark,
            uri,
            ctx: ctx.clone(),
        };
        if self.jobs.try_send(job).is_err() {
            ctx.request_repaint_after(Duration::from_millis(20));
            return State::Pending;
        }
        if cache.len() >= 16
            && let Some(Entry {
                state: State::Ready { uri, .. },
                ..
            }) = cache.pop_front()
        {
            ctx.forget_image(&uri);
        }
        cache.push_back(Entry {
            source: source.into(),
            dark,
            state: State::Pending,
        });
        State::Pending
    }
}

pub fn image(ui: &egui::Ui, source: &str) -> Result<Option<(String, egui::Vec2)>, String> {
    let renderer = ui.ctx().data_mut(|data| {
        Arc::clone(
            data.get_temp_mut_or_insert_with(egui::Id::unique("mermaid-renderer"), || {
                Arc::new(Renderer::new())
            }),
        )
    });
    let state = renderer.request(ui.ctx(), source, ui.visuals().dark_mode);
    crate::ui_probe::metric(
        ui.ctx(),
        "diagram-render-count",
        renderer.renders.load(Ordering::Relaxed) as f32,
    );
    match state {
        State::Pending => Ok(None),
        State::Failed(error) => Err(error),
        State::Ready {
            uri,
            micros,
            natural,
        } => {
            crate::ui_probe::metric(ui.ctx(), "diagram-render-us", micros as f32);
            let width = ui.available_width().min(natural.x).max(1.0);
            match ui
                .ctx()
                .try_load_image(&uri, egui::load::SizeHint::Width(width.ceil() as u32))
            {
                Ok(egui::load::ImagePoll::Ready { .. }) => {
                    let size = natural * (width / natural.x.max(1.0)).min(1.0);
                    Ok(Some((uri, size)))
                }
                Ok(egui::load::ImagePoll::Pending { .. }) => Ok(None),
                Err(error) => Err(error.to_string()),
            }
        }
    }
}

pub fn show(ui: &mut egui::Ui, source: &str, ordinal: usize) -> egui::Response {
    ui.vertical(|ui| match image(ui, source) {
        Ok(Some((uri, size))) => {
            let response = ui.add(egui::Image::new(uri).fit_to_exact_size(size));
            crate::ui_probe::record(ui.ctx(), "mermaid-diagram", response.rect);
        }
        Ok(None) => {
            ui.weak("Rendering diagram…");
        }
        Err(error) => {
            let response =
                ui.colored_label(ui.visuals().error_fg_color, format!("Mermaid: {error}"));
            crate::ui_probe::record(ui.ctx(), "mermaid-error", response.rect);
            crate::code_ui::reading(ui, source, "mermaid", ordinal);
        }
    })
    .response
}

#[cfg(test)]
mod tests {
    #[test]
    fn native_mermaid_renders_nodes_in_both_themes() {
        for dark in [false, true] {
            let svg = super::render("flowchart LR\n A[Write] --> B[Read]", dark).unwrap();
            assert!(svg.contains("<svg") && svg.contains("Write") && svg.contains("Read"));
            let (paths, size) = super::svg_paths(&svg).unwrap();
            assert!(paths.contains("<path") && !paths.contains("<text"));
            assert!(size.x > 100.0 && size.y > 30.0);
        }
        assert!(super::render("not a diagram", false).is_err());
    }
}
