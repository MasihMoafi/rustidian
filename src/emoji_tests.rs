//! Exercise the actual app font definitions, shaping and rasterized atlas pixels.
use super::*;
use std::sync::Arc;

fn context(with_emoji: bool) -> egui::Context {
    let ctx = egui::Context::default();
    install_fonts(&ctx);
    ctx.run_ui(egui::RawInput::default(), |_| {})
        .drop_without_applying_deltas();
    if !with_emoji {
        let mut definitions = ctx.fonts(|fonts| fonts.definitions().clone());
        definitions.font_data.remove("emoji");
        for family in definitions.families.values_mut() {
            family.retain(|name| name != "emoji");
        }
        ctx.set_fonts(definitions);
        ctx.run_ui(egui::RawInput::default(), |_| {})
            .drop_without_applying_deltas();
    }
    ctx
}

fn families(ctx: &egui::Context) -> Vec<FontFamily> {
    ctx.fonts(|fonts| fonts.definitions().families.keys().cloned().collect())
}

fn layout(ctx: &egui::Context, text: &str, family: FontFamily, size: f32) -> Arc<egui::Galley> {
    ctx.fonts_mut(|fonts| {
        // A nonwhite text color catches accidental tinting of color glyphs.
        fonts.layout_no_wrap(text.into(), FontId::new(size, family), Color32::RED)
    })
}

fn painted(galley: &egui::Galley) -> Vec<&egui::epaint::text::Glyph> {
    galley
        .rows
        .iter()
        .flat_map(|row| &row.glyphs)
        .filter(|glyph| !glyph.uv_rect.is_nothing())
        .collect()
}

fn pixels(ctx: &egui::Context, glyph: &egui::epaint::text::Glyph) -> Vec<Color32> {
    let atlas = ctx.fonts(|fonts| fonts.image());
    let width = atlas.width();
    let uv = glyph.uv_rect;
    (uv.min[1]..uv.max[1])
        .flat_map(|y| (uv.min[0]..uv.max[0]).map(move |x| usize::from(y) * width + usize::from(x)))
        .map(|index| atlas.pixels[index])
        .collect()
}

fn assert_one_color_glyph(ctx: &egui::Context, galley: &egui::Galley, text: &str) {
    assert_eq!(
        galley.rows.iter().map(|row| row.text()).collect::<String>(),
        text
    );
    assert_eq!(galley.end().index.0, text.chars().count());
    let glyphs = painted(galley);
    assert_eq!(
        glyphs.len(),
        1,
        "The sequence {text:?} must form one visible glyph"
    );
    let glyph = glyphs[0];
    assert!(glyph.is_color, "{text:?} fell back to a text/tofu glyph");
    assert!(glyph.advance_width > 0.0 && glyph.uv_rect.size.min_elem() > 0.0);
    assert!(
        pixels(ctx, glyph).iter().any(|pixel| {
            pixel.a() > 100
                && (pixel.r().abs_diff(pixel.g()) > 30 || pixel.g().abs_diff(pixel.b()) > 30)
        }),
        "{text:?} has no actual colored atlas pixels"
    );
    // Color glyphs are untinted at tessellation, after the layout mesh is built.
    let primitives = ctx.tessellate(
        vec![egui::epaint::ClippedShape::new(
            egui::Rect::EVERYTHING,
            egui::Shape::galley(egui::Pos2::ZERO, Arc::new(galley.clone()), Color32::RED),
        )],
        ctx.pixels_per_point(),
    );
    assert!(!primitives.is_empty());
    for primitive in primitives {
        let egui::epaint::Primitive::Mesh(mesh) = primitive.primitive else {
            panic!("Expected a text mesh");
        };
        assert!(!mesh.vertices.is_empty());
        assert!(
            mesh.vertices
                .iter()
                .all(|vertex| vertex.color == Color32::WHITE)
        );
    }
}

#[test]
fn color_emoji_shape_in_every_family_at_body_and_heading_sizes() {
    let ctx = context(true);
    let missing = context(false);
    for family in families(&ctx) {
        let before = layout(&missing, "📂", family.clone(), 18.0);
        assert!(painted(&before).iter().all(|glyph| !glyph.is_color));
        for size in [14.0, 18.0, 32.436] {
            for text in ["📂", "⚡", "😀", "❤️", "👩‍💻", "👍🏽", "🇮🇷", "1️⃣", "🏳️‍🌈"]
            {
                let galley = layout(&ctx, text, family.clone(), size);
                assert_one_color_glyph(&ctx, &galley, text);
            }
        }
    }
}

#[test]
fn text_and_emoji_presentation_do_not_poison_each_others_fallback_cache() {
    for emoji_first in [false, true] {
        let ctx = context(true);
        for family in [
            FontFamily::Proportional,
            FontFamily::Monospace,
            FontFamily::Name("bold".into()),
            FontFamily::Name("italic".into()),
        ] {
            for (text, emoji) in [("❤", "❤️"), ("1", "1️⃣"), ("#", "#️⃣"), ("*", "*️⃣")]
            {
                let order = if emoji_first {
                    [emoji, text]
                } else {
                    [text, emoji]
                };
                for value in order.into_iter().chain([text, emoji, text]) {
                    let galley = layout(&ctx, value, family.clone(), 18.0);
                    if value == emoji {
                        assert_one_color_glyph(&ctx, &galley, value);
                    } else {
                        assert!(
                            painted(&galley).iter().all(|glyph| !glyph.is_color),
                            "{value:?}"
                        );
                    }
                }
            }
            for text in ["❤\u{fe0e}", "⚡\u{fe0e}", "A\u{fe0f}"] {
                let galley = layout(&ctx, text, family.clone(), 18.0);
                // Lightning has no outline in some named families; the primary
                // monospace font does have one and must honor text presentation.
                if text.starts_with('⚡') && family != FontFamily::Monospace {
                    continue;
                }
                assert!(
                    painted(&galley).iter().all(|glyph| !glyph.is_color),
                    "{text:?}"
                );
            }
        }
    }
}

#[test]
fn emoji_fallback_preserves_ascii_arabic_and_mixed_text_metrics() {
    let baseline = context(false);
    let fixed = context(true);
    for family in families(&fixed) {
        for text in ["AW 0123456789 #* AV fi", "سلام فارسی بِسْمِ", "Rust نسخه 42"]
        {
            let before = layout(&baseline, text, family.clone(), 18.0);
            let after = layout(&fixed, text, family.clone(), 18.0);
            assert_eq!(before.rect, after.rect, "{family:?}: {text}");
            let metrics = |galley: &egui::Galley| {
                galley
                    .rows
                    .iter()
                    .flat_map(|row| &row.glyphs)
                    .map(|glyph| {
                        (
                            glyph.chr,
                            glyph.pos,
                            glyph.advance_width,
                            glyph.font_face_ascent,
                            glyph.font_face_height,
                            glyph.uv_rect.size,
                            glyph.uv_rect.offset,
                        )
                    })
                    .collect::<Vec<_>>()
            };
            assert_eq!(metrics(&before), metrics(&after), "{family:?}: {text}");
        }
    }
}

#[test]
fn six_native_marker_rasterizers_keep_their_exact_pixels_and_metrics() {
    let baseline = context(false);
    let fixed = context(true);
    for family in families(&fixed) {
        for size in [14.0, 18.0, 32.436] {
            for marker in ["🔵", "🟢", "🟠", "🟣", "🔴", "🟡"] {
                for text in [marker.to_owned(), format!("{marker}\u{fe0f}")] {
                    let before = layout(&baseline, &text, family.clone(), size);
                    let after = layout(&fixed, &text, family.clone(), size);
                    assert_one_color_glyph(&fixed, &after, &text);
                    let before = painted(&before)[0];
                    let after = painted(&after)[0];
                    assert_eq!(before.advance_width, after.advance_width);
                    assert_eq!(before.uv_rect.size, after.uv_rect.size);
                    assert_eq!(before.uv_rect.offset, after.uv_rect.offset);
                    assert_eq!(pixels(&baseline, before), pixels(&fixed, after));
                }
            }
        }
    }
}
