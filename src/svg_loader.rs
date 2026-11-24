//! Web images that are SVG but whose address does not end in `.svg`, such as README badges
//! (`https://img.shields.io/badge/...`). egui's own SVG loader goes by the extension only, so
//! these failed with a warning sign; this loader goes by the server's type or the bytes.
use eframe::egui::{
    self,
    load::{BytesPoll, ImageLoadResult, ImageLoader, ImagePoll, LoadError, SizeHint},
};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex, OnceLock},
};

/// SVG options with Rustidian's own fonts, the sans-serif fallback for any family.
pub fn svg_options() -> usvg::Options<'static> {
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
    usvg::Options {
        font_family: "Inter".into(),
        fontdb: Arc::clone(fonts),
        ..Default::default()
    }
}

type Entry = Result<Arc<egui::ColorImage>, String>;
type Key = (String, SizeHint);
const WORKERS: usize = 2;
const QUEUED: usize = 8;
const CACHE_ENTRIES: usize = 128;
const CACHE_BYTES: usize = 32 * 1024 * 1024;
const SOURCE_BYTES: usize = 1024 * 1024;
const MAX_DIMENSION: f32 = 4096.0;
const MAX_PIXELS: f32 = 4_000_000.0;

enum State {
    Drawing(u64),
    Done(Entry),
}
struct Cached {
    state: State,
    touched: u64,
}
#[derive(Default)]
struct Cache {
    entries: HashMap<Key, Cached>,
    generation: u64,
    clock: u64,
}
impl Cache {
    fn bytes(&self) -> usize {
        self.entries
            .iter()
            .map(|((uri, _), entry)| {
                uri.len()
                    + match &entry.state {
                        State::Done(Ok(image)) => image.pixels.len() * 4,
                        State::Done(Err(error)) => error.len(),
                        _ => 0,
                    }
            })
            .sum()
    }
    fn trim(&mut self, incoming: usize) {
        while self.entries.len() >= CACHE_ENTRIES || self.bytes() + incoming > CACHE_BYTES {
            let oldest = self
                .entries
                .iter()
                .filter(|(_, entry)| matches!(entry.state, State::Done(_)))
                .min_by_key(|(_, entry)| entry.touched)
                .map(|(key, _)| key.clone());
            let Some(oldest) = oldest else { break };
            self.entries.remove(&oldest);
        }
    }
    fn publish(&mut self, key: &Key, generation: u64, result: Entry) {
        if !self.entries.get(key).is_some_and(
            |entry| matches!(entry.state, State::Drawing(token) if token == generation),
        ) {
            return;
        }
        let bytes = match &result {
            Ok(image) => image.pixels.len() * 4,
            Err(error) => error.len(),
        };
        self.trim(bytes);
        self.clock += 1;
        self.entries.insert(
            key.clone(),
            Cached {
                state: State::Done(result),
                touched: self.clock,
            },
        );
    }
}
struct Job {
    key: Key,
    generation: u64,
    bytes: egui::load::Bytes,
    ctx: egui::Context,
}

fn complete(cache: &Mutex<Cache>, job: Job) {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        render(&job.bytes, job.key.1)
    }))
    .unwrap_or_else(|_| Err("SVG rendering failed.".into()))
    .map(Arc::new);
    cache
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .publish(&job.key, job.generation, result);
    job.ctx.request_repaint();
}

/// Bounded off-thread SVG rendering, including extensionless README badges.
#[derive(Default)]
pub struct WebSvgLoader {
    cache: Arc<Mutex<Cache>>,
    jobs: OnceLock<Result<std::sync::mpsc::SyncSender<Job>, String>>,
}
impl WebSvgLoader {
    fn jobs(&self) -> Result<&std::sync::mpsc::SyncSender<Job>, String> {
        self.jobs.get_or_init(|| {
            let (sender, receiver) = std::sync::mpsc::sync_channel::<Job>(QUEUED);
            let receiver = Arc::new(Mutex::new(receiver));
            for index in 0..WORKERS {
                let receiver = receiver.clone();
                let cache = self.cache.clone();
                std::thread::Builder::new().name(format!("rustidian-svg-{index}")).spawn(move || {
                    loop {
                        let job = receiver.lock().unwrap_or_else(|e| e.into_inner()).recv();
                        let Ok(job) = job else { break };
                        let valid = cache.lock().unwrap_or_else(|e| e.into_inner()).entries.get(&job.key)
                            .is_some_and(|entry| matches!(entry.state, State::Drawing(token) if token == job.generation));
                        if !valid { continue; }
                        complete(&cache, job);
                    }
                }).map_err(|error| format!("Cannot start SVG rendering: {error}"))?;
            }
            Ok(sender)
        }).as_ref().map_err(Clone::clone)
    }
}

fn render(bytes: &[u8], hint: SizeHint) -> Result<egui::ColorImage, String> {
    if bytes.len() > SOURCE_BYTES {
        return Err("SVG exceeds the 1 MiB source limit.".into());
    }
    let text = std::str::from_utf8(bytes).map_err(|_| "SVG must be uncompressed UTF-8.")?;
    let xml = usvg::roxmltree::Document::parse_with_options(
        text,
        usvg::roxmltree::ParsingOptions {
            allow_dtd: false,
            nodes_limit: 16_384,
        },
    )
    .map_err(|error| format!("SVG exceeds its parsing budget or is invalid: {error}"))?;
    let ids: HashMap<_, _> = xml
        .descendants()
        .filter_map(|node| node.attribute("id").map(|id| (id, node)))
        .collect();
    let mut expanded = xml.descendants().count();
    let glyphs = |root: usvg::roxmltree::Node<'_, '_>| {
        root.descendants()
            .filter(|node| {
                node.is_text() && node.ancestors().any(|parent| parent.has_tag_name("text"))
            })
            .map(|node| node.text().unwrap_or("").chars().count())
            .sum::<usize>()
    };
    let mut expanded_glyphs = glyphs(xml.root());
    for node in xml.descendants().filter(|node| node.is_element()) {
        if node.ancestors().take(34).count() > 32 {
            return Err("SVG exceeds the nesting limit.".into());
        }
        if node.has_tag_name("use") {
            let target = node
                .attributes()
                .find(|attr| attr.name() == "href")
                .and_then(|attr| attr.value().strip_prefix('#'));
            if let Some(target) = target.and_then(|id| ids.get(id)) {
                expanded += target.descendants().count();
                expanded_glyphs += glyphs(*target);
                if expanded > 16_384 || target.descendants().any(|node| node.has_tag_name("use")) {
                    return Err("SVG exceeds the reference expansion budget.".into());
                }
            }
        }
    }
    if expanded_glyphs > 32_768 {
        return Err("SVG exceeds the text shaping budget.".into());
    }
    let mut options = svg_options();
    let resource_error = Arc::new(Mutex::new(false));
    let failed = resource_error.clone();
    let decoded_pixels = std::sync::atomic::AtomicU64::new(0);
    let default = usvg::ImageHrefResolver::default_data_resolver();
    let decode = Arc::new(
        move |mime: &str, data: Arc<Vec<u8>>, options: &usvg::Options<'_>| {
            let dimensions = image::ImageReader::new(std::io::Cursor::new(data.as_slice()))
                .with_guessed_format()
                .ok()
                .and_then(|reader| reader.into_dimensions().ok());
            let pixels = dimensions.map_or(u64::MAX, |(w, h)| u64::from(w) * u64::from(h));
            if data.len() > SOURCE_BYTES
                || dimensions.is_none_or(|(w, h)| {
                    w > MAX_DIMENSION as u32
                        || h > MAX_DIMENSION as u32
                        || pixels > MAX_PIXELS as u64
                })
                || decoded_pixels
                    .fetch_add(pixels, std::sync::atomic::Ordering::Relaxed)
                    .saturating_add(pixels)
                    > 16_000_000
            {
                *failed.lock().unwrap_or_else(|e| e.into_inner()) = true;
                None
            } else {
                default(mime, data, options)
            }
        },
    );
    let embedded = decode.clone();
    options.image_href_resolver.resolve_data =
        Box::new(move |mime, data, options| embedded(mime, data, options));
    let failed = resource_error.clone();
    options.image_href_resolver.resolve_string = Box::new(move |href, options| {
        use std::io::Read as _;
        let path = options.get_abs_path(std::path::Path::new(href));
        let file = std::fs::File::open(path).ok()?;
        let mut data = Vec::new();
        if file
            .take((SOURCE_BYTES + 1) as u64)
            .read_to_end(&mut data)
            .is_err()
            || data.len() > SOURCE_BYTES
        {
            *failed.lock().unwrap_or_else(|e| e.into_inner()) = true;
            return None;
        }
        decode("text/plain", Arc::new(data), options)
    });
    let tree = usvg::Tree::from_xmltree(&xml, &options).map_err(|error| error.to_string())?;
    if *resource_error.lock().unwrap_or_else(|e| e.into_inner()) {
        return Err("SVG image resource exceeds the embedded image budget.".into());
    }
    let source = egui::vec2(tree.size().width(), tree.size().height());
    let size = match hint {
        SizeHint::Width(w) => source * (w as f32 / source.x),
        SizeHint::Height(h) => source * (h as f32 / source.y),
        SizeHint::Size {
            width,
            height,
            maintain_aspect_ratio,
        } => {
            let target = egui::vec2(width as f32, height as f32);
            if maintain_aspect_ratio {
                source * (target.x / source.x).min(target.y / source.y)
            } else {
                target
            }
        }
        SizeHint::Scale(scale) => source * scale.into_inner(),
    }
    .round();
    if !size.is_finite()
        || size.x < 1.0
        || size.y < 1.0
        || size.x > MAX_DIMENSION
        || size.y > MAX_DIMENSION
        || size.x * size.y > MAX_PIXELS
    {
        return Err("SVG exceeds the 4096 px / 4 million pixel rendering limit.".into());
    }
    fn cost(
        group: &usvg::Group,
        canvas: f64,
        count: &mut usize,
        work: &mut f64,
        pixels: &mut f64,
    ) -> bool {
        *count += group.children().len();
        *pixels += canvas / 25.0 * group.children().len() as f64;
        if *count > 16_384 {
            return false;
        }
        if group.should_isolate() {
            *work += canvas
                * (1.0
                    + group
                        .filters()
                        .iter()
                        .map(|filter| filter.primitives().len() as f64)
                        .sum::<f64>());
        }
        let mut valid = *work <= 16_000_000.0 && *pixels <= 256_000_000.0;
        for node in group.children() {
            if let usvg::Node::Group(child) = node {
                valid &= cost(child, canvas, count, work, pixels);
            }
            node.subroots(|root| {
                valid &= cost(root, canvas, count, work, pixels);
            });
        }
        valid
    }
    if !cost(
        tree.root(),
        f64::from(size.x * size.y) * 25.0,
        &mut 0,
        &mut 0.0,
        &mut 0.0,
    ) {
        return Err("SVG exceeds the rendering work and layer memory budget.".into());
    }
    let (width, height) = (size.x as u32, size.y as u32);
    let mut pixmap = resvg::tiny_skia::Pixmap::new(width, height)
        .ok_or("Cannot allocate the bounded SVG image.")?;
    resvg::render(
        &tree,
        usvg::Transform::from_scale(size.x / source.x, size.y / source.y),
        &mut pixmap.as_mut(),
    );
    Ok(
        egui::ColorImage::from_rgba_premultiplied([width as usize, height as usize], pixmap.data())
            .with_source_size(source),
    )
}

fn path_of(uri: &str) -> String {
    uri.split(['?', '#'])
        .next()
        .unwrap_or(uri)
        .to_ascii_lowercase()
}

/// A web address this loader reads by the server's type: not named as another image type.
fn badge(uri: &str) -> bool {
    let path = path_of(uri);
    (uri.starts_with("https://") || uri.starts_with("http://"))
        && ![
            ".svg", ".png", ".jpg", ".jpeg", ".gif", ".webp", ".bmp", ".ico",
        ]
        .iter()
        .any(|extension| path.ends_with(extension))
}

/// A named SVG file or web address. The app's own embedded icons (`bytes://`) stay with egui's
/// loader: they are small, hold no text and must be ready in the first frame.
fn svg_named(uri: &str) -> bool {
    (uri.starts_with("file://") || uri.starts_with("http://") || uri.starts_with("https://"))
        && path_of(uri).ends_with(".svg")
}

/// Sizes round up to 256 px steps, so a window being resized does not redraw the diagram at
/// every width.
fn coarse(hint: SizeHint) -> SizeHint {
    let up = |pixels: u32| pixels.div_ceil(256).max(1).saturating_mul(256);
    match hint {
        SizeHint::Width(width) => SizeHint::Width(up(width)),
        SizeHint::Height(height) => SizeHint::Height(up(height)),
        SizeHint::Size {
            width,
            height,
            maintain_aspect_ratio,
        } => SizeHint::Size {
            width: up(width),
            height: up(height),
            maintain_aspect_ratio,
        },
        scale => scale,
    }
}

/// SVG by the server's type, or by the document's start when the type is missing.
pub fn is_svg(mime: Option<&str>, bytes: &[u8]) -> bool {
    if let Some(mime) = mime {
        return mime.to_ascii_lowercase().contains("image/svg");
    }
    let start = String::from_utf8_lossy(&bytes[..bytes.len().min(512)]).to_ascii_lowercase();
    let start = start.trim_start_matches('\u{feff}').trim_start();
    start.starts_with("<svg") || start.starts_with("<?xml") && start.contains("<svg")
}

impl ImageLoader for WebSvgLoader {
    fn id(&self) -> &str {
        egui::generate_loader_id!(WebSvgLoader)
    }

    fn load(&self, ctx: &egui::Context, uri: &str, size_hint: SizeHint) -> ImageLoadResult {
        let named = svg_named(uri);
        if !named && !badge(uri) {
            return Err(LoadError::NotSupported);
        }
        // A badge keeps its own size: the editor's width hint would stretch it to the page width.
        let hint = if named {
            coarse(size_hint)
        } else {
            SizeHint::default()
        };
        if uri.len() > 4096 {
            return Err(LoadError::Loading(
                "SVG address exceeds its size limit.".into(),
            ));
        }
        let key = (uri.to_owned(), hint);
        let observed = {
            let mut cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
            cache.clock += 1;
            let clock = cache.clock;
            if let Some(entry) = cache.entries.get_mut(&key) {
                entry.touched = clock;
                return match &entry.state {
                    State::Done(result) => result
                        .clone()
                        .map(|image| ImagePoll::Ready { image })
                        .map_err(LoadError::Loading),
                    State::Drawing(_) => Ok(ImagePoll::Pending { size: None }),
                };
            }
            cache.generation
        };
        match ctx.try_load_bytes(uri)? {
            BytesPoll::Pending { size } => Ok(ImagePoll::Pending { size }),
            BytesPoll::Ready { bytes, mime, .. } => {
                if !named && !is_svg(mime.as_deref(), &bytes) {
                    return Err(LoadError::NotSupported);
                }
                if bytes.len() > SOURCE_BYTES {
                    return Err(LoadError::Loading(
                        "SVG exceeds the 1 MiB source limit.".into(),
                    ));
                }
                let jobs = self.jobs().map_err(LoadError::Loading)?;
                let mut cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
                if cache.generation != observed || cache.entries.contains_key(&key) {
                    ctx.request_repaint();
                    return Ok(ImagePoll::Pending { size: None });
                }
                cache.trim(uri.len());
                cache.generation += 1;
                let generation = cache.generation;
                let touched = cache.clock;
                cache.entries.insert(
                    key.clone(),
                    Cached {
                        state: State::Drawing(generation),
                        touched,
                    },
                );
                let job = Job {
                    key: key.clone(),
                    generation,
                    bytes,
                    ctx: ctx.clone(),
                };
                match jobs.try_send(job) {
                    Ok(()) => {}
                    Err(std::sync::mpsc::TrySendError::Full(_)) => {
                        cache.entries.remove(&key);
                        ctx.request_repaint_after(std::time::Duration::from_millis(16));
                    }
                    Err(std::sync::mpsc::TrySendError::Disconnected(_)) => {
                        let error = "SVG rendering service stopped.".to_owned();
                        cache.publish(&key, generation, Err(error.clone()));
                        return Err(LoadError::Loading(error));
                    }
                }
                Ok(ImagePoll::Pending { size: None })
            }
        }
    }
    fn forget(&self, uri: &str) {
        let mut cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        cache.generation += 1;
        cache.entries.retain(|(key, _), _| key != uri);
    }
    fn forget_all(&self) {
        let mut cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        cache.generation += 1;
        cache.entries.clear();
    }
    fn byte_size(&self) -> usize {
        self.cache.lock().unwrap_or_else(|e| e.into_inner()).bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn badges_are_read_by_type_or_content_and_named_images_are_left_to_their_loaders() {
        assert!(badge(
            "https://img.shields.io/badge/skills-19-orange?style=flat-square"
        ));
        assert!(badge("https://skills.sh/b/MasihMoafi/skills"));
        assert!(
            !badge("https://example.com/logo.svg") && svg_named("https://example.com/logo.svg"),
            "a named SVG is drawn with the page's size, not as a badge"
        );
        assert!(svg_named("file:///vault/diagram.SVG?x=1") && !svg_named("file:///vault/a.png"));
        assert!(
            !svg_named("bytes://icons/folder.svg"),
            "app icons stay with egui"
        );
        assert!(!badge("https://example.com/photo.PNG?x=1"));
        assert!(!badge("file:///vault/badge"));
        assert!(is_svg(Some("image/svg+xml;charset=utf-8"), b""));
        assert!(!is_svg(Some("image/png"), b"<svg"));
        assert!(is_svg(
            None,
            b"  <svg xmlns=\"http://www.w3.org/2000/svg\"/>"
        ));
        assert!(is_svg(None, b"<?xml version=\"1.0\"?>\n<svg/>"));
        assert!(!is_svg(None, b"\x89PNG\r\n"));
    }

    #[test]
    fn sizes_round_up_in_coarse_steps() {
        assert_eq!(coarse(SizeHint::Width(1)), SizeHint::Width(256));
        assert_eq!(coarse(SizeHint::Width(917)), SizeHint::Width(1024));
        assert_eq!(coarse(SizeHint::Width(1024)), SizeHint::Width(1024));
    }

    #[test]
    fn a_diagram_with_titles_and_legends_draws_its_text() {
        let diagram = br##"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="60"><rect width="200" height="60" fill="#fff"/><text x="100" y="35" fill="#000" font-family="Inter" font-size="20" text-anchor="middle">Run 1</text></svg>"##;
        let image = egui_extras::image::load_svg_bytes_with_size(
            diagram,
            SizeHint::default(),
            &svg_options(),
        )
        .unwrap();
        let dark = image.pixels.iter().filter(|pixel| pixel.r() < 100).count();
        assert!(dark > 20, "the title is drawn ({dark} dark pixels)");
    }

    #[test]
    fn a_shields_badge_renders_with_its_text() {
        let badge = br##"<svg xmlns="http://www.w3.org/2000/svg" width="62" height="20"><rect width="62" height="20" fill="#e05d44"/><text x="31" y="14" fill="#fff" font-family="Verdana,Geneva,DejaVu Sans,sans-serif" font-size="11" text-anchor="middle">skills</text></svg>"##;
        let image = egui_extras::image::load_svg_bytes_with_size(
            badge,
            SizeHint::default(),
            &svg_options(),
        )
        .unwrap();
        assert_eq!(image.size, [62, 20]);
        let white = image
            .pixels
            .iter()
            .filter(|pixel| pixel.r() > 200 && pixel.g() > 200 && pixel.b() > 200)
            .count();
        assert!(
            white > 10,
            "the badge's text is drawn ({white} white pixels)"
        );
    }
}

#[cfg(test)]
#[path = "svg_loader_tests.rs"]
mod service_tests;
