use eframe::egui::{self, Color32, FontFamily, FontId, Stroke, TextStyle, Visuals};

pub const ACCENT: Color32 = Color32::from_rgb(101, 183, 182);
pub const DARK_BG: Color32 = Color32::from_rgb(23, 27, 29);
pub const DARK_SIDE: Color32 = Color32::from_rgb(17, 21, 23);
pub const DARK_TEXT: Color32 = Color32::from_rgb(233, 229, 220);
pub const DARK_MUTED: Color32 = Color32::from_rgb(155, 165, 165);
pub const LIGHT_MUTED: Color32 = Color32::from_rgb(101, 112, 113);
pub const DARK_FAINT: Color32 = Color32::from_rgb(104, 115, 117);

/// Keep CSS-like fractional row spacing after egui's per-row pixel rounding.
pub fn set_line_height(galley: &mut std::sync::Arc<egui::Galley>, line_height: f32) {
    let shaped = std::sync::Arc::make_mut(galley);
    let mut height = 0.0;
    shaped.mesh_bounds = egui::Rect::NOTHING;
    for row in &mut shaped.rows {
        row.pos.y = height;
        std::sync::Arc::make_mut(&mut row.row).size.y = line_height;
        shaped.mesh_bounds |= row.visuals.mesh_bounds.translate(row.pos.to_vec2());
        height += line_height;
    }
    shaped.rect.max.y = height;
}

pub fn accent(ui: &egui::Ui) -> Color32 {
    if ui.visuals().dark_mode {
        ACCENT
    } else {
        Color32::from_rgb(14, 117, 124)
    }
}

pub fn marker(ui: &egui::Ui) -> Color32 {
    if ui.visuals().dark_mode {
        DARK_FAINT
    } else {
        ui.visuals().weak_text_color()
    }
}

pub fn interactive(ui: &egui::Ui) -> Color32 {
    if ui.visuals().dark_mode {
        Color32::from_rgb(23, 127, 131)
    } else {
        Color32::from_rgb(14, 117, 124)
    }
}

pub fn heading(ui: &egui::Ui, level: u8) -> Color32 {
    match (ui.visuals().dark_mode, level) {
        (true, 1) => Color32::from_rgb(244, 240, 232),
        (true, 2) => Color32::from_rgb(217, 231, 228),
        (true, 3) => Color32::from_rgb(183, 207, 203),
        (false, 1) => Color32::from_rgb(23, 33, 35),
        (false, 2) => Color32::from_rgb(40, 87, 90),
        _ => ui.visuals().text_color(),
    }
}

pub fn apply(ctx: &egui::Context, dark: bool) {
    let mut visuals = if dark {
        Visuals::dark()
    } else {
        Visuals::light()
    };
    visuals.panel_fill = if dark {
        DARK_SIDE
    } else {
        Color32::from_rgb(233, 235, 231)
    };
    visuals.extreme_bg_color = if dark {
        DARK_SIDE
    } else {
        Color32::from_rgb(227, 230, 226)
    };
    visuals.window_fill = if dark {
        DARK_BG
    } else {
        Color32::from_rgb(247, 246, 242)
    };
    visuals.override_text_color = Some(if dark {
        DARK_TEXT
    } else {
        Color32::from_rgb(34, 42, 43)
    });
    visuals.weak_text_color = Some(if dark { DARK_MUTED } else { LIGHT_MUTED });
    visuals.code_bg_color = if dark {
        Color32::from_rgb(27, 32, 35)
    } else {
        Color32::from_rgb(240, 239, 234)
    };
    visuals.hyperlink_color = if dark {
        ACCENT
    } else {
        Color32::from_rgb(14, 117, 124)
    };
    visuals.selection.bg_fill = Color32::from_rgba_unmultiplied(23, 127, 131, 110);
    visuals.selection.stroke = Stroke::new(1.0, visuals.hyperlink_color);
    let border = if dark {
        Color32::from_rgb(44, 52, 55)
    } else {
        Color32::from_rgb(209, 215, 211)
    };
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, border);
    visuals.widgets.inactive.weak_bg_fill = Color32::TRANSPARENT;
    visuals.widgets.inactive.bg_fill = if dark {
        Color32::from_rgb(27, 32, 35)
    } else {
        Color32::from_rgb(240, 239, 234)
    };
    visuals.widgets.hovered.weak_bg_fill = if dark {
        Color32::from_rgb(34, 41, 44)
    } else {
        Color32::from_rgb(220, 230, 225)
    };
    visuals.widgets.active.weak_bg_fill = Color32::from_rgba_unmultiplied(23, 127, 131, 55);
    ctx.set_visuals(visuals);
    ctx.all_styles_mut(|style| {
        style
            .text_styles
            .insert(TextStyle::Body, FontId::new(18.0, FontFamily::Proportional));
        style.text_styles.insert(
            TextStyle::Button,
            FontId::new(14.0, FontFamily::Proportional),
        );
        style.text_styles.insert(
            TextStyle::Small,
            FontId::new(12.0, FontFamily::Proportional),
        );
        style.text_styles.insert(
            TextStyle::Heading,
            FontId::new(32.436, FontFamily::Name("bold".into())),
        );
        style.text_styles.insert(
            TextStyle::Monospace,
            FontId::new(16.0, FontFamily::Monospace),
        );
        style.spacing.item_spacing = egui::vec2(8.0, 8.0);
        style.spacing.button_padding = egui::vec2(10.0, 7.0);
        style.spacing.window_margin = egui::Margin::same(22);
        style.visuals.window_corner_radius = egui::CornerRadius::same(10);
    });
}

pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    for (name, bytes) in [
        (
            "inter",
            include_bytes!("../assets/fonts/Inter-Regular.otf").as_slice(),
        ),
        (
            "semibold",
            include_bytes!("../assets/fonts/Inter-SemiBold.otf").as_slice(),
        ),
        (
            "bold",
            include_bytes!("../assets/fonts/Inter-Bold.otf").as_slice(),
        ),
        (
            "italic",
            include_bytes!("../assets/fonts/Inter-Italic.otf").as_slice(),
        ),
        (
            "mono",
            include_bytes!("../assets/fonts/DejaVuSansMono.ttf").as_slice(),
        ),
        (
            "arabic",
            include_bytes!("../assets/fonts/NotoSansArabic-Regular.ttf").as_slice(),
        ),
        (
            "arabic-bold",
            include_bytes!("../assets/fonts/NotoSansArabic-Bold.ttf").as_slice(),
        ),
    ] {
        fonts
            .font_data
            .insert(name.into(), egui::FontData::from_static(bytes).into());
        fonts
            .families
            .insert(FontFamily::Name(name.into()), vec![name.into()]);
    }
    for (name, fallback) in [
        ("inter", "arabic"),
        ("italic", "arabic"),
        ("semibold", "arabic-bold"),
        ("bold", "arabic-bold"),
    ] {
        fonts
            .families
            .get_mut(&FontFamily::Name(name.into()))
            .unwrap()
            .push(fallback.into());
    }
    fonts
        .families
        .get_mut(&FontFamily::Proportional)
        .unwrap()
        .insert(0, "inter".into());
    fonts
        .families
        .get_mut(&FontFamily::Proportional)
        .unwrap()
        .insert(1, "arabic".into());
    fonts
        .families
        .get_mut(&FontFamily::Monospace)
        .unwrap()
        .insert(0, "mono".into());
    ctx.set_fonts(fonts);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Icon {
    Files,
    Search,
    NewNote,
    NewFolder,
    Sort,
    CollapseAll,
    Disclosure,
    NewTab,
    SidebarLeft,
    SidebarRight,
    Back,
    Forward,
    Reading,
    Edit,
    Split,
    Save,
    Commands,
    Vault,
    Appearance,
    Outline,
    More,
    Close,
    Copy,
    PropertyText,
    PropertyList,
    PropertyNumber,
    PropertyCheckbox,
    PropertyAliases,
    PropertyTags,
    PropertyDate,
    PropertyDateTime,
}

impl Icon {
    pub const ALL: [Self; 31] = [
        Self::Files,
        Self::Search,
        Self::NewNote,
        Self::NewFolder,
        Self::Sort,
        Self::CollapseAll,
        Self::Disclosure,
        Self::NewTab,
        Self::SidebarLeft,
        Self::SidebarRight,
        Self::Back,
        Self::Forward,
        Self::Reading,
        Self::Edit,
        Self::Split,
        Self::Save,
        Self::Commands,
        Self::Vault,
        Self::Appearance,
        Self::Outline,
        Self::More,
        Self::Close,
        Self::Copy,
        Self::PropertyText,
        Self::PropertyList,
        Self::PropertyNumber,
        Self::PropertyCheckbox,
        Self::PropertyAliases,
        Self::PropertyTags,
        Self::PropertyDate,
        Self::PropertyDateTime,
    ];

    pub fn asset(self) -> (&'static str, &'static [u8]) {
        match self {
            Self::Close => (
                "bytes://icons/close.svg",
                include_bytes!("../assets/icons/close.svg"),
            ),
            Self::Copy => (
                "bytes://icons/copy.svg",
                include_bytes!("../assets/icons/copy.svg"),
            ),
            Self::Files => (
                "bytes://icons/files.svg",
                include_bytes!("../assets/icons/files.svg"),
            ),
            Self::Search => (
                "bytes://icons/search.svg",
                include_bytes!("../assets/icons/search.svg"),
            ),
            Self::NewFolder => (
                "bytes://icons/new-folder.svg",
                include_bytes!("../assets/icons/new-folder.svg"),
            ),
            Self::Sort => (
                "bytes://icons/sort.svg",
                include_bytes!("../assets/icons/sort.svg"),
            ),
            Self::CollapseAll => (
                "bytes://icons/collapse-all.svg",
                include_bytes!("../assets/icons/collapse-all.svg"),
            ),
            Self::Disclosure => (
                "bytes://icons/disclosure.svg",
                include_bytes!("../assets/icons/disclosure.svg"),
            ),
            Self::NewNote => (
                "bytes://icons/new-note.svg",
                include_bytes!("../assets/icons/new-note.svg"),
            ),
            Self::NewTab => (
                "bytes://icons/new-tab.svg",
                include_bytes!("../assets/icons/new-tab.svg"),
            ),
            Self::SidebarLeft => (
                "bytes://icons/sidebar-left.svg",
                include_bytes!("../assets/icons/sidebar-left.svg"),
            ),
            Self::SidebarRight => (
                "bytes://icons/sidebar-right.svg",
                include_bytes!("../assets/icons/sidebar-right.svg"),
            ),
            Self::Back => (
                "bytes://icons/back.svg",
                include_bytes!("../assets/icons/back.svg"),
            ),
            Self::Forward => (
                "bytes://icons/forward.svg",
                include_bytes!("../assets/icons/forward.svg"),
            ),
            Self::Reading => (
                "bytes://icons/reading.svg",
                include_bytes!("../assets/icons/reading.svg"),
            ),
            Self::Edit => (
                "bytes://icons/edit.svg",
                include_bytes!("../assets/icons/edit.svg"),
            ),
            Self::Split => (
                "bytes://icons/split.svg",
                include_bytes!("../assets/icons/split.svg"),
            ),
            Self::Save => (
                "bytes://icons/save.svg",
                include_bytes!("../assets/icons/save.svg"),
            ),
            Self::Commands => (
                "bytes://icons/commands.svg",
                include_bytes!("../assets/icons/commands.svg"),
            ),
            Self::Vault => (
                "bytes://icons/vault.svg",
                include_bytes!("../assets/icons/vault.svg"),
            ),
            Self::Appearance => (
                "bytes://icons/appearance.svg",
                include_bytes!("../assets/icons/appearance.svg"),
            ),
            Self::More => (
                "bytes://icons/more.svg",
                include_bytes!("../assets/icons/more.svg"),
            ),
            Self::Outline => (
                "bytes://icons/outline.svg",
                include_bytes!("../assets/icons/outline.svg"),
            ),
            Self::PropertyText => (
                "bytes://icons/property-text.svg",
                include_bytes!("../assets/icons/property-text.svg"),
            ),
            Self::PropertyList => (
                "bytes://icons/property-list.svg",
                include_bytes!("../assets/icons/property-list.svg"),
            ),
            Self::PropertyNumber => (
                "bytes://icons/property-number.svg",
                include_bytes!("../assets/icons/property-number.svg"),
            ),
            Self::PropertyCheckbox => (
                "bytes://icons/property-checkbox.svg",
                include_bytes!("../assets/icons/property-checkbox.svg"),
            ),
            Self::PropertyAliases => (
                "bytes://icons/property-aliases.svg",
                include_bytes!("../assets/icons/property-aliases.svg"),
            ),
            Self::PropertyTags => (
                "bytes://icons/property-tags.svg",
                include_bytes!("../assets/icons/property-tags.svg"),
            ),
            Self::PropertyDate => (
                "bytes://icons/property-date.svg",
                include_bytes!("../assets/icons/property-date.svg"),
            ),
            Self::PropertyDateTime => (
                "bytes://icons/property-date-time.svg",
                include_bytes!("../assets/icons/property-date-time.svg"),
            ),
        }
    }
}

pub fn install_icons(ctx: &egui::Context) {
    egui_extras::install_image_loaders(ctx);
    for icon in Icon::ALL {
        let (uri, bytes) = icon.asset();
        ctx.include_bytes(uri, egui::load::Bytes::Static(bytes));
    }
}

pub fn icon_image(icon: Icon, color: Color32) -> egui::Image<'static> {
    let (uri, _) = icon.asset();
    egui::Image::new(uri)
        .fit_to_exact_size(egui::vec2(18.0, 18.0))
        .tint(color)
}

pub fn disclosure(ui: &mut egui::Ui, openness: f32, response: &egui::Response) {
    icon_image(
        Icon::Disclosure,
        ui.visuals().weak_text_color().gamma_multiply(0.8),
    )
    .rotate(
        openness * std::f32::consts::FRAC_PI_2,
        egui::Vec2::splat(0.5),
    )
    .paint_at(
        ui,
        egui::Rect::from_center_size(response.rect.center(), egui::vec2(12.0, 12.0)),
    );
}

pub fn checkbox(ui: &mut egui::Ui, checked: &mut bool) -> egui::Response {
    ui.scope(|ui| {
        ui.spacing_mut().icon_width = 18.0;
        ui.spacing_mut().icon_width_inner = 12.0;
        ui.spacing_mut().interact_size.y = 18.0;
        let colour = ui.visuals().weak_text_color();
        ui.visuals_mut().widgets.inactive.bg_stroke = Stroke::new(1.0, colour);
        ui.visuals_mut().widgets.inactive.bg_fill = Color32::TRANSPARENT;
        ui.checkbox(checked, "")
    })
    .inner
}

pub fn icon_button(ui: &mut egui::Ui, icon: Icon, tooltip: &str) -> egui::Response {
    let image = icon_image(icon, ui.visuals().text_color().gamma_multiply(0.8));
    let response = ui.add(
        egui::Button::image(image)
            .frame(false)
            .min_size(egui::vec2(30.0, 30.0)),
    );
    response
        .widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, ui.is_enabled(), tooltip));
    response.on_hover_text(tooltip)
}

#[cfg(test)]
mod tests {
    #[test]
    fn bidi_paragraphs_keep_the_editors_left_to_right_base_direction() {
        use eframe::egui::{self, Color32, FontId};
        let ctx = egui::Context::default();
        super::install_fonts(&ctx);
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            for text in ["## فارسی", "English 42"] {
                let galley = ui.fonts_mut(|fonts| {
                    fonts.layout_no_wrap(text.into(), FontId::proportional(18.0), Color32::WHITE)
                });
                let row = &galley.rows[0];
                assert_eq!(row.text(), text);
                assert!(row.glyphs[0].pos.x < row.glyphs[3].pos.x, "{text}");
            }
        });
        output.textures_delta.clear();
    }

    #[test]
    fn bidi_end_cursor_and_arabic_marks_keep_their_logical_owners() {
        use eframe::egui::{self, Color32, FontId};
        let ctx = egui::Context::default();
        super::install_fonts(&ctx);
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            for text in ["ملف PDF", "صفحة 12 من 340", "English 42"] {
                let galley = ui.fonts_mut(|fonts| {
                    fonts.layout_no_wrap(text.into(), FontId::proportional(18.0), Color32::WHITE)
                });
                let row = &galley.rows[0];
                let last = row.glyphs.last().unwrap();
                assert!(!last.is_rtl());
                let expected = if row.glyphs.iter().any(|glyph| glyph.is_rtl()) {
                    last.max_x()
                } else {
                    row.size.x
                };
                assert!(
                    (row.x_offset(text.chars().count().into()) - expected).abs() < 0.01,
                    "End cursor left its final logical run in {text:?}"
                );
            }
            let text = "بِسْمِ";
            let galley = ui.fonts_mut(|fonts| {
                fonts.layout_no_wrap(text.into(), FontId::proportional(26.0), Color32::WHITE)
            });
            assert!(
                galley.rows[0].glyphs[2].advance_width > 0.0,
                "The base س must own its advancing glyph before its mark"
            );
            assert_eq!(
                galley.rows[0].text(),
                text,
                "Glyph metadata must retain logical Unicode characters"
            );
        });
        output.textures_delta.clear();
    }

    #[test]
    fn mixed_direction_selection_paints_only_its_logical_characters() {
        use eframe::egui::{
            self, Color32, FontId,
            text::{CCursor, CCursorRange},
        };
        let ctx = egui::Context::default();
        super::install_fonts(&ctx);
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            let mut galley = ui.fonts_mut(|fonts| {
                fonts.layout_no_wrap(
                    "Open ملف now".into(),
                    FontId::proportional(18.0),
                    Color32::WHITE,
                )
            });
            let glyphs = galley.rows[0].glyphs.clone();
            let height = galley.rows[0].height();
            let mut visuals = egui::Visuals::dark();
            let color = Color32::from_rgb(21, 199, 63);
            visuals.selection.bg_fill = color;
            let selection = CCursorRange::two(CCursor::new(0), CCursor::new(5));
            egui::text_selection::visuals::paint_text_selection(
                &mut galley,
                &visuals,
                &selection,
                None,
            );
            let vertices: Vec<_> = galley.rows[0]
                .visuals
                .mesh
                .vertices
                .iter()
                .filter(|vertex| vertex.color == color)
                .map(|vertex| vertex.pos)
                .collect();
            let rectangles: Vec<_> = vertices
                .chunks_exact(4)
                .map(|vertices| {
                    vertices.iter().fold(egui::Rect::NOTHING, |rect, &point| {
                        rect.union(egui::Rect::from_pos(point))
                    })
                })
                .collect();
            for (index, glyph) in glyphs
                .iter()
                .enumerate()
                .filter(|(_, glyph)| glyph.advance_width > 0.0)
            {
                let center = egui::pos2(glyph.logical_rect().center().x, height / 2.0);
                assert_eq!(
                    rectangles.iter().any(|rect| rect.contains(center)),
                    index < 5,
                    "Selection painted the wrong logical glyph {index}"
                );
            }
        });
        output.textures_delta.clear();
    }
    #[test]
    fn prose_and_heading_families_cover_persian_letters() {
        let ctx = eframe::egui::Context::default();
        super::install_fonts(&ctx);
        let mut output = ctx.run_ui(eframe::egui::RawInput::default(), |ui| {
            for family in [
                eframe::egui::FontFamily::Proportional,
                eframe::egui::FontFamily::Name("semibold".into()),
                eframe::egui::FontFamily::Name("bold".into()),
                eframe::egui::FontFamily::Name("italic".into()),
            ] {
                let font = eframe::egui::FontId::new(18.0, family.clone());
                assert!(
                    ui.fonts_mut(|fonts| fonts.has_glyphs(&font, "فارسی سلام دنیا")),
                    "Missing Persian glyphs in {family:?}"
                );
                assert!(ui.fonts_mut(|fonts| fonts.has_glyphs(&font, "English 42")));
                for text in [
                    "سلام دنیا",
                    "Rust نسخه 42",
                    "English 42",
                    "بِسْمِ لا می‌توانیم",
                    "Hebrew אבג 42",
                ] {
                    let galley = ui.fonts_mut(|fonts| {
                        fonts.layout(
                            text.to_owned(),
                            font.clone(),
                            eframe::egui::Color32::WHITE,
                            84.0,
                        )
                    });
                    let rendered: usize = galley
                        .rows
                        .iter()
                        .map(|row| row.char_count_excluding_newline().0)
                        .sum();
                    assert_eq!(
                        rendered,
                        text.chars().count(),
                        "Shaping changed logical cursor count for {text:?} in {family:?}"
                    );
                    assert_eq!(
                        galley.rows.iter().map(|row| row.text()).collect::<String>(),
                        text
                    );
                    assert_eq!(galley.end().index.0, text.chars().count());
                }
            }
        });
        output.textures_delta.clear();
    }
    use super::Icon;
    use std::collections::HashSet;

    #[test]
    fn different_icon_actions_have_distinct_real_vector_artwork() {
        let mut names = HashSet::new();
        let mut artwork = HashSet::new();
        for icon in Icon::ALL {
            let (name, bytes) = icon.asset();
            assert!(names.insert(name), "duplicate asset name: {icon:?}");
            assert!(
                artwork.insert(bytes),
                "two different actions share identical artwork: {icon:?}"
            );
            let svg = std::str::from_utf8(bytes).unwrap();
            assert!(svg.contains("<path"), "missing genuine paths: {name}");
            for forbidden in ["<image", "<script", "<text", "<foreignObject", "data:image"] {
                assert!(
                    !svg.contains(forbidden),
                    "non-vector or active content: {name}"
                );
            }
        }
    }
}
