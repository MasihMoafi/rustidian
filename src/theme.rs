use eframe::egui::{self, Color32, FontFamily, FontId, Stroke, TextStyle, Visuals};

#[path = "emoji_glyphs.rs"]
mod emoji_glyphs;

#[path = "palette.rs"]
pub mod palette;

#[path = "system_theme.rs"]
mod system_theme;
pub use system_theme::SystemTheme;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Appearance {
    #[default]
    System,
    Dark,
    Light,
}

impl Appearance {
    pub fn from_dark(dark: bool) -> Self {
        if dark { Self::Dark } else { Self::Light }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::System => "System",
            Self::Dark => "Dark",
            Self::Light => "Light",
        }
    }

    fn preference(self) -> egui::ThemePreference {
        match self {
            Self::System => egui::ThemePreference::System,
            Self::Dark => egui::ThemePreference::Dark,
            Self::Light => egui::ThemePreference::Light,
        }
    }
}

pub const ACCENT: Color32 = Color32::from_rgb(101, 183, 182);
pub const DARK_BG: Color32 = Color32::from_rgb(23, 27, 29);
pub const DARK_SIDE: Color32 = Color32::from_rgb(17, 21, 23);
pub const DARK_TEXT: Color32 = Color32::from_rgb(233, 229, 220);
pub const DARK_MUTED: Color32 = Color32::from_rgb(155, 165, 165);
pub const LIGHT_MUTED: Color32 = Color32::from_rgb(101, 112, 113);
pub const DARK_FAINT: Color32 = Color32::from_rgb(104, 115, 117);
pub const DEFAULT_DOCUMENT_FONT_SIZE: u8 = 18;

pub fn sidebar_scroll() -> egui::ScrollArea {
    egui::ScrollArea::vertical().auto_shrink([false, false])
}

fn interface_fonts(style: &mut egui::Style) {
    for (kind, font) in [
        (TextStyle::Body, FontId::proportional(14.0)),
        (TextStyle::Button, FontId::proportional(14.0)),
        (TextStyle::Small, FontId::proportional(12.0)),
        (
            TextStyle::Heading,
            FontId::new(18.0, FontFamily::Name("semibold".into())),
        ),
        (TextStyle::Monospace, FontId::monospace(14.0)),
    ] {
        style.text_styles.insert(kind, font);
    }
}

pub fn interface_style(ui: &mut egui::Ui) {
    interface_fonts(ui.style_mut());
}

/// A shared compact slider; document typography must not resize its thumb.
pub fn slider(ui: &mut egui::Ui, slider: egui::Slider<'_>) -> egui::Response {
    ui.scope(|ui| {
        let dark = ui.visuals().dark_mode;
        let style = ui.style_mut();
        style.spacing.interact_size.y = 16.0;
        style.spacing.button_padding = egui::vec2(4.0, 2.0);
        style.spacing.slider_rail_height = 4.0;
        style
            .text_styles
            .insert(TextStyle::Body, FontId::proportional(13.0));
        for (visuals, fill, outline) in [
            (&mut style.visuals.widgets.inactive, 66, 170),
            (&mut style.visuals.widgets.hovered, 108, 190),
            (&mut style.visuals.widgets.active, 128, 200),
        ] {
            visuals.bg_fill = Color32::from_gray(if dark { fill } else { 230 - fill / 2 });
            visuals.fg_stroke = Stroke::new(
                1.0,
                Color32::from_gray(if dark { outline } else { 230 - outline }),
            );
            visuals.expansion = 0.0;
        }
        ui.add(
            slider
                .handle_shape(egui::style::HandleShape::Circle)
                .trailing_fill(true),
        )
    })
    .inner
}

/// Apply document typography inside its own UI, leaving window controls unchanged.
pub fn document_style(ui: &mut egui::Ui, size: u8) {
    let scale = f32::from(size) / f32::from(DEFAULT_DOCUMENT_FONT_SIZE);
    let style = ui.style_mut();
    for (kind, base) in [
        (TextStyle::Body, 18.0),
        (TextStyle::Heading, 32.436),
        (TextStyle::Monospace, 16.0),
    ] {
        if let Some(font) = style.text_styles.get_mut(&kind) {
            font.family = match kind {
                TextStyle::Heading => FontFamily::Name("bold".into()),
                TextStyle::Monospace => FontFamily::Monospace,
                _ => FontFamily::Proportional,
            };
            font.size = if kind == TextStyle::Body {
                f32::from(size)
            } else {
                base * scale
            };
        }
    }
    crate::ui_probe::metric(ui.ctx(), "document-font-size", f32::from(size));
}

pub fn document_size(ui: &egui::Ui, size: f32) -> f32 {
    size * (TextStyle::Body.resolve(ui.style()).size / f32::from(DEFAULT_DOCUMENT_FONT_SIZE))
}

pub fn prose_size(ui: &egui::Ui, heading: u8) -> f32 {
    document_size(
        ui,
        match heading {
            1 => 32.436,
            2 => 28.836,
            3 => 25.632,
            4.. => 22.788,
            _ => 18.0,
        },
    )
}

pub fn link_clicked(response: &egui::Response) -> bool {
    response.clicked() || response.clicked_by(egui::PointerButton::Middle)
}

pub fn hover_fill(ui: &egui::Ui) -> Color32 {
    ui.visuals().widgets.hovered.weak_bg_fill
}

/// Paint the current response without waiting for another layout pass.
pub fn control_feedback(ui: &egui::Ui, response: &egui::Response) {
    if response.enabled() && (response.hovered() || response.is_pointer_button_down_on()) {
        ui.painter().rect_filled(response.rect, 4, hover_fill(ui));
    }
    if response.has_focus() {
        ui.painter().rect_stroke(
            response.rect,
            4,
            Stroke::new(1.0, interactive(ui)),
            egui::StrokeKind::Inside,
        );
    }
}

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
    ui.visuals().hyperlink_color
}

pub fn marker(ui: &egui::Ui) -> Color32 {
    if ui.visuals().dark_mode {
        DARK_FAINT
    } else {
        ui.visuals().weak_text_color()
    }
}

pub fn interactive(ui: &egui::Ui) -> Color32 {
    if palette::current(ui.ctx()) != palette::Palette::Teal {
        return accent(ui);
    }
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
    apply_appearance(ctx, Appearance::from_dark(dark));
}

pub fn apply_appearance(ctx: &egui::Context, appearance: Appearance) {
    ctx.set_theme(appearance.preference());
    palette::select(ctx, palette::current(ctx));
    ctx.options_mut(|options| {
        options.input_options.line_scroll_speed = 150.0;
        options.input_options.wheel_smoothing = false;
    });
    for dark in [false, true] {
        apply_style(ctx, dark);
    }
}

fn apply_style(ctx: &egui::Context, dark: bool) {
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
    visuals.widgets.hovered.bg_fill = visuals.widgets.hovered.weak_bg_fill;
    visuals.widgets.hovered.corner_radius = egui::CornerRadius::same(7);
    visuals.widgets.hovered.bg_stroke = Stroke::NONE;
    visuals.widgets.active.bg_fill = visuals.widgets.active.weak_bg_fill;
    let palette = palette::current(ctx);
    if palette != palette::Palette::Teal {
        let accent = palette.accent(dark);
        visuals.panel_fill = palette.side(dark);
        visuals.extreme_bg_color = visuals.panel_fill;
        visuals.window_fill = palette.background(dark);
        visuals.code_bg_color = visuals.panel_fill;
        visuals.hyperlink_color = accent;
        visuals.selection.bg_fill = accent.gamma_multiply(0.3);
        visuals.selection.stroke = Stroke::new(1.0, accent);
        visuals.widgets.inactive.bg_fill = visuals.panel_fill;
        let strength = if dark { 0.2 } else { 0.1 };
        visuals.widgets.hovered.weak_bg_fill =
            accent.gamma_multiply(strength) + visuals.window_fill.gamma_multiply(1.0 - strength);
        visuals.widgets.hovered.bg_fill = visuals.widgets.hovered.weak_bg_fill;
        visuals.widgets.active.weak_bg_fill = accent.gamma_multiply(0.3);
        visuals.widgets.active.bg_fill = visuals.widgets.active.weak_bg_fill;
    }
    ctx.style_mut_of(egui::Theme::from_dark_mode(dark), |style| {
        style.visuals = visuals;
        style.animation_time = 0.12;
        style.scroll_animation = egui::style::ScrollAnimation::none();
        interface_fonts(style);
        style.spacing.item_spacing = egui::vec2(8.0, 8.0);
        style.spacing.button_padding = egui::vec2(10.0, 7.0);
        style.spacing.window_margin = egui::Margin::same(22);
        style.spacing.scroll = egui::style::ScrollStyle {
            bar_width: 6.0,
            floating_width: 6.0,
            floating_allocated_width: 6.0,
            handle_min_length: 36.0,
            foreground_color: true,
            dormant_background_opacity: 0.04,
            active_background_opacity: 0.07,
            interact_background_opacity: 0.10,
            dormant_handle_opacity: if dark { 0.42 } else { 0.62 },
            active_handle_opacity: if dark { 0.44 } else { 0.66 },
            interact_handle_opacity: if dark { 0.40 } else { 0.58 },
            ..egui::style::ScrollStyle::thin()
        };
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
    fonts.font_data.insert(
        "emoji".into(),
        egui::FontData::from_static(include_bytes!(concat!(
            env!("OUT_DIR"),
            "/NotoColorEmoji.ttf"
        )))
        .into(),
    );
    for family in fonts.families.values_mut() {
        family.push("emoji".into());
    }
    ctx.set_fonts(fonts);
    emoji_glyphs::install(ctx);
}

#[cfg(test)]
#[path = "emoji_tests.rs"]
mod emoji_tests;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Icon {
    Files,
    Search,
    Graph,
    Bookmark,
    DailyNote,
    Templates,
    Recovery,
    NewNote,
    NewFolder,
    Sort,
    CollapseAll,
    Disclosure,
    NewTab,
    Pin,
    SidebarLeft,
    SidebarRight,
    Back,
    Forward,
    OpenLink,
    Reading,
    Edit,
    Split,
    Save,
    Commands,
    Settings,
    Appearance,
    Outline,
    More,
    Close,
    Minimize,
    Maximize,
    Restore,
    Trash,
    Copy,
    PropertyText,
    PropertyList,
    PropertyNumber,
    PropertyCheckbox,
    PropertyAliases,
    PropertyTags,
    PropertyDate,
    PropertyDateTime,
    CalloutNote,
    CalloutAbstract,
    CalloutInfo,
    CalloutTodo,
    CalloutTip,
    CalloutSuccess,
    CalloutQuestion,
    CalloutWarning,
    CalloutFailure,
    CalloutDanger,
    CalloutBug,
    CalloutExample,
    CalloutQuote,
}

impl Icon {
    pub const ALL: [Self; 55] = [
        Self::Files,
        Self::Search,
        Self::Graph,
        Self::Bookmark,
        Self::DailyNote,
        Self::Templates,
        Self::Recovery,
        Self::NewNote,
        Self::NewFolder,
        Self::Sort,
        Self::CollapseAll,
        Self::Disclosure,
        Self::NewTab,
        Self::Pin,
        Self::SidebarLeft,
        Self::SidebarRight,
        Self::Back,
        Self::Forward,
        Self::OpenLink,
        Self::Reading,
        Self::Edit,
        Self::Split,
        Self::Save,
        Self::Commands,
        Self::Settings,
        Self::Appearance,
        Self::Outline,
        Self::More,
        Self::Close,
        Self::Minimize,
        Self::Maximize,
        Self::Restore,
        Self::Trash,
        Self::Copy,
        Self::PropertyText,
        Self::PropertyList,
        Self::PropertyNumber,
        Self::PropertyCheckbox,
        Self::PropertyAliases,
        Self::PropertyTags,
        Self::PropertyDate,
        Self::PropertyDateTime,
        Self::CalloutNote,
        Self::CalloutAbstract,
        Self::CalloutInfo,
        Self::CalloutTodo,
        Self::CalloutTip,
        Self::CalloutSuccess,
        Self::CalloutQuestion,
        Self::CalloutWarning,
        Self::CalloutFailure,
        Self::CalloutDanger,
        Self::CalloutBug,
        Self::CalloutExample,
        Self::CalloutQuote,
    ];

    pub fn asset(self) -> (&'static str, &'static [u8]) {
        match self {
            Self::Graph => (
                "bytes://icons/graph.svg",
                include_bytes!("../assets/icons/graph.svg"),
            ),
            Self::Trash => (
                "bytes://icons/trash.svg",
                include_bytes!("../assets/icons/trash.svg"),
            ),
            Self::Minimize => (
                "bytes://icons/minimize.svg",
                include_bytes!("../assets/icons/minimize.svg"),
            ),
            Self::Maximize => (
                "bytes://icons/maximize.svg",
                include_bytes!("../assets/icons/maximize.svg"),
            ),
            Self::Restore => (
                "bytes://icons/restore.svg",
                include_bytes!("../assets/icons/restore.svg"),
            ),
            Self::Bookmark => (
                "bytes://icons/bookmark.svg",
                include_bytes!("../assets/icons/bookmark.svg"),
            ),
            Self::Pin => (
                "bytes://icons/pin.svg",
                include_bytes!("../assets/icons/pin.svg"),
            ),
            Self::DailyNote => (
                "bytes://icons/daily-note.svg",
                include_bytes!("../assets/icons/daily-note.svg"),
            ),
            Self::Templates => (
                "bytes://icons/templates.svg",
                include_bytes!("../assets/icons/templates.svg"),
            ),
            Self::Recovery => (
                "bytes://icons/recovery.svg",
                include_bytes!("../assets/icons/recovery.svg"),
            ),
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
            Self::OpenLink => (
                "bytes://icons/open-link.svg",
                include_bytes!("../assets/icons/open-link.svg"),
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
            Self::Settings => (
                "bytes://icons/settings.svg",
                include_bytes!("../assets/icons/settings.svg"),
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
            Self::CalloutNote => (
                "bytes://icons/callout-note.svg",
                include_bytes!("../assets/icons/callout-note.svg"),
            ),
            Self::CalloutAbstract => (
                "bytes://icons/callout-abstract.svg",
                include_bytes!("../assets/icons/callout-abstract.svg"),
            ),
            Self::CalloutInfo => (
                "bytes://icons/callout-info.svg",
                include_bytes!("../assets/icons/callout-info.svg"),
            ),
            Self::CalloutTodo => (
                "bytes://icons/callout-todo.svg",
                include_bytes!("../assets/icons/callout-todo.svg"),
            ),
            Self::CalloutTip => (
                "bytes://icons/callout-tip.svg",
                include_bytes!("../assets/icons/callout-tip.svg"),
            ),
            Self::CalloutSuccess => (
                "bytes://icons/callout-success.svg",
                include_bytes!("../assets/icons/callout-success.svg"),
            ),
            Self::CalloutQuestion => (
                "bytes://icons/callout-question.svg",
                include_bytes!("../assets/icons/callout-question.svg"),
            ),
            Self::CalloutWarning => (
                "bytes://icons/callout-warning.svg",
                include_bytes!("../assets/icons/callout-warning.svg"),
            ),
            Self::CalloutFailure => (
                "bytes://icons/callout-failure.svg",
                include_bytes!("../assets/icons/callout-failure.svg"),
            ),
            Self::CalloutDanger => (
                "bytes://icons/callout-danger.svg",
                include_bytes!("../assets/icons/callout-danger.svg"),
            ),
            Self::CalloutBug => (
                "bytes://icons/callout-bug.svg",
                include_bytes!("../assets/icons/callout-bug.svg"),
            ),
            Self::CalloutExample => (
                "bytes://icons/callout-example.svg",
                include_bytes!("../assets/icons/callout-example.svg"),
            ),
            Self::CalloutQuote => (
                "bytes://icons/callout-quote.svg",
                include_bytes!("../assets/icons/callout-quote.svg"),
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
    let scale = document_size(ui, 1.0);
    let (rect, mut response) =
        ui.allocate_exact_size(egui::vec2(18.0, 18.0) * scale, egui::Sense::click());
    if response.clicked() {
        *checked = !*checked;
        response.mark_changed();
    }
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::Role::CheckBox, ui.is_enabled(), *checked, "")
    });
    if ui.is_rect_visible(rect) {
        let color = if *checked {
            interactive(ui)
        } else if response.hovered() {
            ui.visuals().weak_text_color()
        } else {
            marker(ui)
        };
        ui.painter().rect(
            rect,
            5.0 * scale,
            if *checked {
                color
            } else {
                Color32::TRANSPARENT
            },
            Stroke::new(1.0, color),
            egui::StrokeKind::Inside,
        );
        if *checked {
            ui.painter().add(egui::Shape::line(
                vec![
                    rect.min + egui::vec2(4.0, 9.0) * scale,
                    rect.min + egui::vec2(7.5, 12.5) * scale,
                    rect.min + egui::vec2(14.0, 5.0) * scale,
                ],
                Stroke::new(1.8, ui.visuals().window_fill),
            ));
        }
        if response.has_focus() {
            ui.painter().rect_stroke(
                rect.expand(2.0),
                6.0,
                ui.visuals().selection.stroke,
                egui::StrokeKind::Outside,
            );
        }
    }
    response
}

pub fn dialog_style(ui: &mut egui::Ui) {
    interface_style(ui);
    ui.spacing_mut().item_spacing = egui::vec2(8.0, 12.0);
    ui.spacing_mut().button_padding = egui::vec2(14.0, 8.0);
}

pub fn dialog_frame(ctx: &egui::Context) -> egui::Frame {
    egui::Frame::popup(&ctx.global_style())
        .inner_margin(egui::Margin::same(24))
        .corner_radius(10)
}

pub fn dialog_header(ui: &mut egui::Ui, title: &str, close_label: &str) -> egui::Response {
    ui.horizontal(|ui| {
        ui.heading(title);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let response = icon_button(ui, Icon::Close, close_label);
            crate::ui_probe::record(ui.ctx(), "dialog-close", response.rect);
            response
        })
        .inner
    })
    .inner
}

pub fn dialog_actions(
    ui: &mut egui::Ui,
    label: &str,
    destructive: bool,
) -> (egui::Response, egui::Response) {
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let background = if destructive {
                Color32::from_rgb(180, 45, 55)
            } else {
                accent(ui)
            };
            let foreground = if destructive || !ui.visuals().dark_mode {
                Color32::WHITE
            } else {
                Color32::from_rgb(17, 21, 23)
            };
            let submit = ui.add(
                egui::Button::new(egui::RichText::new(label).color(foreground))
                    .fill(background)
                    .stroke(egui::Stroke::NONE)
                    .corner_radius(6)
                    .min_size(egui::vec2(0.0, 36.0)),
            );
            let cancel = ui.add(
                egui::Button::new("Cancel")
                    .fill(ui.visuals().window_fill)
                    .stroke(ui.visuals().widgets.noninteractive.bg_stroke)
                    .corner_radius(6)
                    .min_size(egui::vec2(0.0, 36.0)),
            );
            crate::ui_probe::record(ui.ctx(), "dialog-submit", submit.rect);
            crate::ui_probe::record(ui.ctx(), "dialog-cancel", cancel.rect);
            (submit, cancel)
        })
        .inner
    })
    .inner
}

pub fn menu_style(ui: &mut egui::Ui) {
    for style in [egui::TextStyle::Body, egui::TextStyle::Button] {
        ui.style_mut()
            .text_styles
            .insert(style, egui::FontId::proportional(13.0));
    }
    ui.spacing_mut().item_spacing.y = 1.0;
    ui.spacing_mut().button_padding = egui::vec2(8.0, 4.0);
}

pub fn delete_menu_item(ui: &mut egui::Ui, label: &str) -> (egui::Response, bool) {
    let response = menu_item(ui, Icon::Trash, &format!("D  {label}"));
    let shortcut = ui.input_mut(|input| {
        input.modifiers.matches_exact(egui::Modifiers::NONE)
            && input.consume_key(egui::Modifiers::NONE, egui::Key::D)
    });
    if shortcut {
        ui.input_mut(|input| {
            input.events.retain(
                |event| !matches!(event, egui::Event::Text(text) if text.eq_ignore_ascii_case("d")),
            )
        });
    }
    let activated = response.clicked() || shortcut;
    (response, activated)
}

pub fn menu_item(ui: &mut egui::Ui, icon: Icon, label: &str) -> egui::Response {
    let color = if matches!(icon, Icon::Trash) {
        ui.visuals().error_fg_color
    } else {
        ui.visuals().text_color()
    };
    let image = icon_image(icon, color).fit_to_exact_size(egui::vec2(16.0, 16.0));
    let response = ui.add(
        egui::Button::image_and_text(image, egui::RichText::new(label).size(13.0).color(color))
            .min_size(egui::vec2(210.0, 26.0))
            .frame_when_inactive(false)
            .corner_radius(4.0),
    );
    crate::ui_probe::record(ui.ctx(), format!("menu:{label}"), response.rect);
    response
}

pub fn icon_button(ui: &mut egui::Ui, icon: Icon, tooltip: &str) -> egui::Response {
    icon_toggle(ui, icon, tooltip, false)
}

pub fn icon_toggle(ui: &mut egui::Ui, icon: Icon, tooltip: &str, selected: bool) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(30.0, 30.0), egui::Sense::click());
    crate::ui_probe::record(ui.ctx(), format!("button:{tooltip}"), rect);
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::Role::Button, ui.is_enabled(), selected, tooltip)
    });
    if ui.is_rect_visible(rect) {
        if selected {
            ui.painter().rect_filled(rect, 4, hover_fill(ui));
        }
        control_feedback(ui, &response);
        let color = if (selected || response.hovered()) && response.enabled() {
            ui.visuals().text_color()
        } else {
            ui.visuals().weak_text_color()
        };
        icon_image(icon, color).paint_at(
            ui,
            egui::Rect::from_center_size(rect.center(), egui::vec2(16.0, 16.0)),
        );
    }
    response.on_hover_ui(|ui| {
        ui.label(egui::RichText::new(tooltip).size(13.0));
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn system_appearance_changes_both_custom_styles_and_explicit_choices_ignore_os_changes() {
        use super::{Appearance, apply_appearance};
        use eframe::egui::{self, Theme};
        let ctx = egui::Context::default();
        for appearance in [Appearance::System, Appearance::Dark, Appearance::Light] {
            apply_appearance(&ctx, appearance);
            for system_theme in [Theme::Light, Theme::Dark, Theme::Light] {
                ctx.run_ui(
                    egui::RawInput {
                        system_theme: Some(system_theme),
                        ..Default::default()
                    },
                    |ui| {
                        let dark = match appearance {
                            Appearance::System => system_theme == Theme::Dark,
                            Appearance::Dark => true,
                            Appearance::Light => false,
                        };
                        assert_eq!(ui.visuals().dark_mode, dark);
                        assert_eq!(
                            ui.visuals().window_fill,
                            super::palette::Palette::Teal.background(dark)
                        );
                        assert_eq!(egui::TextStyle::Button.resolve(ui.style()).size, 14.0);
                        assert_eq!(ui.spacing().scroll.bar_width, 6.0);
                    },
                )
                .drop_without_applying_deltas();
            }
        }
    }

    #[test]
    fn document_font_controls_change_glyphs_without_resizing_other_ui() {
        use eframe::egui;
        for dark in [false, true] {
            let ctx = egui::Context::default();
            super::apply(&ctx, dark);
            let mut widths = Vec::new();
            ctx.run_ui(egui::RawInput::default(), |ui| {
                let before = ui.label("Navigation").rect.width();
                let button_font = egui::TextStyle::Button.resolve(ui.style());
                for size in [10, 18, 30] {
                    let width = ui
                        .scope(|ui| {
                            super::document_style(ui, size);
                            assert_eq!(
                                egui::TextStyle::Body.resolve(ui.style()).size,
                                f32::from(size)
                            );
                            assert_eq!(egui::TextStyle::Button.resolve(ui.style()), button_font);
                            if size == 18 {
                                assert_eq!(super::document_size(ui, 32.436), 32.436);
                            }
                            ui.label("Document text").rect.width()
                        })
                        .inner;
                    widths.push(width);
                    assert_eq!(egui::TextStyle::Body.resolve(ui.style()).size, 14.0);
                }
                assert_eq!(ui.label("Navigation").rect.width(), before);
            })
            .drop_without_applying_deltas();
            assert!(widths[0] < widths[1] && widths[1] < widths[2], "{widths:?}");
        }
    }

    #[test]
    fn wheel_steps_are_immediate_and_leave_no_smoothing_tail() {
        use eframe::egui::{self, Event, Modifiers, MouseWheelSource, MouseWheelUnit, TouchPhase};
        fn frame(ctx: &egui::Context, time: f64, events: Vec<Event>) -> f32 {
            let mut delta = None;
            ctx.run_ui(
                egui::RawInput {
                    time: Some(time),
                    events,
                    ..Default::default()
                },
                |ui| {
                    if delta.is_none() {
                        delta = Some(ui.input(|input| input.smooth_scroll_delta().y));
                    }
                },
            )
            .drop_without_applying_deltas();
            delta.unwrap()
        }
        let wheel = || Event::MouseWheel {
            unit: MouseWheelUnit::Line,
            delta: egui::vec2(0.0, -1.0),
            phase: TouchPhase::Move,
            source: MouseWheelSource::Unknown,
            modifiers: Modifiers::NONE,
        };
        let before = egui::Context::default();
        let smoothed = frame(&before, 1.0, vec![wheel()]);
        assert!(smoothed.abs() < 40.0);
        let after = egui::Context::default();
        super::apply(&after, true);
        assert_eq!(frame(&after, 1.0, vec![wheel()]), -150.0);
        assert_eq!(frame(&after, 1.016, vec![]), 0.0);
        assert_eq!(frame(&after, 1.1, vec![]), 0.0);
    }

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
