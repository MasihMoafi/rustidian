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

#[derive(Default)]
pub struct WebSvgLoader {
    cache: Mutex<HashMap<(String, SizeHint), Entry>>,
}

/// An address this loader may read: on the web, and not named as another image type.
fn candidate(uri: &str) -> bool {
    let path = uri
        .split(['?', '#'])
        .next()
        .unwrap_or(uri)
        .to_ascii_lowercase();
    (uri.starts_with("https://") || uri.starts_with("http://"))
        && ![
            ".svg", ".png", ".jpg", ".jpeg", ".gif", ".webp", ".bmp", ".ico",
        ]
        .iter()
        .any(|extension| path.ends_with(extension))
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

    fn load(&self, ctx: &egui::Context, uri: &str, _size_hint: SizeHint) -> ImageLoadResult {
        if !candidate(uri) {
            return Err(LoadError::NotSupported);
        }
        // A badge keeps its own size: the editor's width hint would stretch it to the page width.
        let key = (uri.to_owned(), SizeHint::default());
        if let Some(entry) = self.cache.lock().unwrap().get(&key) {
            return entry
                .clone()
                .map(|image| ImagePoll::Ready { image })
                .map_err(LoadError::Loading);
        }
        match ctx.try_load_bytes(uri)? {
            BytesPoll::Pending { size } => Ok(ImagePoll::Pending { size }),
            BytesPoll::Ready { bytes, mime, .. } => {
                if !is_svg(mime.as_deref(), &bytes) {
                    return Err(LoadError::NotSupported);
                }
                let entry = egui_extras::image::load_svg_bytes_with_size(
                    &bytes,
                    SizeHint::default(),
                    &svg_options(),
                )
                .map(Arc::new);
                self.cache.lock().unwrap().insert(key, entry.clone());
                entry
                    .map(|image| ImagePoll::Ready { image })
                    .map_err(LoadError::Loading)
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
            .map(|entry| entry.as_ref().map_or(0, |image| image.pixels.len() * 4))
            .sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn badges_are_read_by_type_or_content_and_named_images_are_left_to_their_loaders() {
        assert!(candidate(
            "https://img.shields.io/badge/skills-19-orange?style=flat-square"
        ));
        assert!(candidate("https://skills.sh/b/MasihMoafi/skills"));
        assert!(
            !candidate("https://example.com/logo.svg"),
            "egui reads .svg itself"
        );
        assert!(!candidate("https://example.com/photo.PNG?x=1"));
        assert!(!candidate("file:///vault/badge"));
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
