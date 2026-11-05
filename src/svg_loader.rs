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

enum State {
    /// A thread is drawing it; the page shows a placeholder until then.
    Drawing,
    Done(Entry),
}

/// Every SVG the page shows: web badges without an extension, and `.svg` files or addresses.
/// egui's own SVG loader draws no text (it loads no fonts) and draws on the UI thread; this one
/// uses Rustidian's fonts (5 Nov: Elpis's diagrams lost every title, legend and axis label) and
/// draws on a thread, crisp at the size the page asks for.
#[derive(Default)]
pub struct WebSvgLoader {
    cache: Arc<Mutex<HashMap<(String, SizeHint), State>>>,
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
    let up = |pixels: u32| pixels.div_ceil(256).max(1) * 256;
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
        let key = (uri.to_owned(), hint);
        match self.cache.lock().unwrap().get(&key) {
            Some(State::Done(entry)) => {
                return entry
                    .clone()
                    .map(|image| ImagePoll::Ready { image })
                    .map_err(LoadError::Loading);
            }
            Some(State::Drawing) => return Ok(ImagePoll::Pending { size: None }),
            None => {}
        }
        match ctx.try_load_bytes(uri)? {
            BytesPoll::Pending { size } => Ok(ImagePoll::Pending { size }),
            BytesPoll::Ready { bytes, mime, .. } => {
                if !named && !is_svg(mime.as_deref(), &bytes) {
                    return Err(LoadError::NotSupported);
                }
                self.cache
                    .lock()
                    .unwrap()
                    .insert(key.clone(), State::Drawing);
                let cache = Arc::clone(&self.cache);
                let ctx = ctx.clone();
                std::thread::spawn(move || {
                    let entry =
                        egui_extras::image::load_svg_bytes_with_size(&bytes, hint, &svg_options())
                            .map(Arc::new);
                    cache.lock().unwrap().insert(key, State::Done(entry));
                    ctx.request_repaint();
                });
                Ok(ImagePoll::Pending { size: None })
            }
        }
    }

    fn forget(&self, uri: &str) {
        self.cache.lock().unwrap().retain(|(key, _), _| key != uri);
    }

    fn forget_all(&self) {
        self.cache.lock().unwrap().clear();
    }

    fn byte_size(&self) -> usize {
        self.cache
            .lock()
            .unwrap()
            .values()
            .map(|state| match state {
                State::Done(Ok(image)) => image.pixels.len() * 4,
                _ => 0,
            })
            .sum()
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
