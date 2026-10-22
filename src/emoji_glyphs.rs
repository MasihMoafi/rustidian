//! The six highlight-marker glyphs from the installed Noto Color Emoji font.
use eframe::egui::{
    self, ColorImage, FontPriority, GlyphBitmap, GlyphRasterizer, GlyphRasterizerRequest,
    RasterizedGlyph,
};
use std::sync::OnceLock;

const GLYPHS: [(&str, &[u8]); 6] = [
    (
        "🔵",
        include_bytes!("../assets/fonts/emoji-markers/blue.png"),
    ),
    (
        "🟢",
        include_bytes!("../assets/fonts/emoji-markers/green.png"),
    ),
    (
        "🟠",
        include_bytes!("../assets/fonts/emoji-markers/orange.png"),
    ),
    (
        "🟣",
        include_bytes!("../assets/fonts/emoji-markers/purple.png"),
    ),
    (
        "🔴",
        include_bytes!("../assets/fonts/emoji-markers/red.png"),
    ),
    (
        "🟡",
        include_bytes!("../assets/fonts/emoji-markers/yellow.png"),
    ),
];

fn rasterize(request: &GlyphRasterizerRequest<'_>) -> Option<RasterizedGlyph> {
    let cluster = request
        .cluster
        .strip_suffix('\u{fe0f}')
        .unwrap_or(request.cluster);
    let index = GLYPHS.iter().position(|(symbol, _)| *symbol == cluster)?;
    static IMAGES: OnceLock<[image::RgbaImage; 6]> = OnceLock::new();
    let images = IMAGES.get_or_init(|| {
        std::array::from_fn(|index| {
            image::load_from_memory(GLYPHS[index].1)
                .expect("Bundled Noto glyph PNG")
                .to_rgba8()
        })
    });
    let scale = request.font_size_px / 109.0;
    let width = (136.0 * scale).round().max(1.0) as u32;
    let height = (128.0 * scale).round().max(1.0) as u32;
    let image = image::imageops::resize(
        &images[index],
        width,
        height,
        image::imageops::FilterType::Triangle,
    );
    Some(RasterizedGlyph {
        bitmap: GlyphBitmap {
            image: ColorImage::from_rgba_unmultiplied(
                [width as usize, height as usize],
                image.as_raw(),
            ),
            offset_px: egui::vec2(0.0, -101.0 * scale),
            is_color: true,
        },
        advance_px: request.font_size_px * (2550.0 / 2048.0),
    })
}

pub fn install(ctx: &egui::Context) {
    ctx.add_glyph_rasterizer(
        GlyphRasterizer::new("rust-notes.noto-highlight-markers", rasterize)
            .with_priority(FontPriority::Highest),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marker_glyphs_keep_color_alpha_and_font_metrics_without_overriding_other_text() {
        let family = egui::FontFamily::Monospace;
        for (symbol, _) in GLYPHS {
            let request = GlyphRasterizerRequest {
                cluster: symbol,
                family: &family,
                font_size_px: 18.0,
                subpixel_offset_px: 0.0,
            };
            let rendered = rasterize(&request).unwrap();
            assert!(rendered.bitmap.is_color);
            assert_eq!(rendered.bitmap.image.size, [22, 21]);
            assert!(
                rendered
                    .bitmap
                    .image
                    .pixels
                    .iter()
                    .any(|pixel| pixel.a() == 0)
            );
            assert!(
                rendered
                    .bitmap
                    .image
                    .pixels
                    .iter()
                    .any(|pixel| pixel.a() > 200
                        && (pixel.r().abs_diff(pixel.b()) > 30
                            || pixel.g().abs_diff(pixel.b()) > 30))
            );
            assert!(
                rendered.bitmap.offset_px.y < 0.0 && (22.0..23.0).contains(&rendered.advance_px)
            );
        }
        for cluster in ["A", "سلام", "🔵x", "", "😀"] {
            assert!(
                rasterize(&GlyphRasterizerRequest {
                    cluster,
                    family: &family,
                    font_size_px: 18.0,
                    subpixel_offset_px: 0.0
                })
                .is_none()
            );
        }
    }
}
