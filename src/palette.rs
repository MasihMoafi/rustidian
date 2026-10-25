use eframe::egui::{self, Color32, FontId, Stroke};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Palette {
    LoveSpace,
    ElpisGold,
    Signature,
    #[default]
    #[serde(other)]
    Teal,
}

impl Palette {
    pub fn name(self) -> &'static str {
        match self {
            Self::Teal => "Original teal",
            Self::LoveSpace => "Love Space",
            Self::ElpisGold => "Elpis Gold",
            Self::Signature => "Signature",
        }
    }

    pub fn accent(self, dark: bool) -> Color32 {
        let rgb = match (self, dark) {
            (Self::Teal, true) => [101, 183, 182],
            (Self::Teal, false) => [14, 117, 124],
            (Self::LoveSpace, true) => [236, 87, 94],
            (Self::LoveSpace, false) => [139, 16, 23],
            (Self::ElpisGold, true) => [242, 169, 59],
            (Self::ElpisGold, false) => [126, 76, 0],
            (Self::Signature, true) => [250, 118, 107],
            (Self::Signature, false) => [177, 15, 75],
        };
        Color32::from_rgb(rgb[0], rgb[1], rgb[2])
    }

    pub fn background(self, dark: bool) -> Color32 {
        let rgb = match (self, dark) {
            (Self::Teal, true) => [23, 27, 29],
            (Self::Teal, false) => [247, 246, 242],
            (Self::LoveSpace, true) => [29, 23, 27],
            (Self::LoveSpace, false) => [251, 245, 245],
            (Self::ElpisGold, true) => [28, 27, 23],
            (Self::ElpisGold, false) => [251, 248, 239],
            (Self::Signature, true) => [31, 31, 40],
            (Self::Signature, false) => [252, 245, 242],
        };
        Color32::from_rgb(rgb[0], rgb[1], rgb[2])
    }

    pub fn side(self, dark: bool) -> Color32 {
        let background = self.background(dark);
        let scale = if dark { 0.77 } else { 0.94 };
        Color32::from_rgb(
            (f32::from(background.r()) * scale) as u8,
            (f32::from(background.g()) * scale) as u8,
            (f32::from(background.b()) * scale) as u8,
        )
    }

    pub fn logo(self) -> (&'static str, &'static [u8]) {
        match self {
            Self::Teal => (
                "bytes://rustidian/original.png",
                include_bytes!("../docs/brand/rustidian.png"),
            ),
            Self::LoveSpace => (
                "bytes://rustidian/love-space.png",
                include_bytes!("../docs/brand/themes/love-space.png"),
            ),
            Self::ElpisGold => (
                "bytes://rustidian/elpis-gold.png",
                include_bytes!("../docs/brand/themes/elpis-gold.png"),
            ),
            Self::Signature => (
                "bytes://rustidian/signature.png",
                include_bytes!("../docs/brand/themes/signature.png"),
            ),
        }
    }
}

fn id() -> egui::Id {
    egui::Id::unique("rustidian-palette")
}

pub fn current(ctx: &egui::Context) -> Palette {
    ctx.data(|data| data.get_temp(id()).unwrap_or_default())
}

pub fn select(ctx: &egui::Context, palette: Palette) {
    let previous = ctx.data(|data| data.get_temp::<Palette>(id()));
    ctx.data_mut(|data| data.insert_temp(id(), palette));
    if previous != Some(palette) {
        // Decode only on selection; inactive themes add no startup image work.
        if let Ok(image) = image::load_from_memory(palette.logo().1) {
            let image = image.thumbnail(128, 128).into_rgba8();
            let (width, height) = image.dimensions();
            ctx.send_viewport_cmd(egui::ViewportCommand::Icon(Some(std::sync::Arc::new(
                egui::IconData {
                    rgba: image.into_raw(),
                    width,
                    height,
                },
            ))));
        }
        #[cfg(target_os = "linux")]
        if let Err(error) = sync_launcher(palette) {
            eprintln!("Cannot update Rustidian's launcher icon: {error}");
        }
    }
}

#[cfg(target_os = "linux")]
fn sync_launcher(palette: Palette) -> std::io::Result<()> {
    use std::{io::Write, path::PathBuf};
    let Some(data) = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share")))
    else {
        return Ok(());
    };
    let launcher = data.join("applications/rustidian.desktop");
    // A per-user override also covers the system-wide Debian installation.
    let source = if launcher.is_file() {
        launcher.clone()
    } else {
        let directories = std::env::var_os("XDG_DATA_DIRS")
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| "/usr/local/share:/usr/share".into());
        let Some(source) = std::env::split_paths(&directories)
            .filter(|path| path.is_absolute())
            .map(|path| path.join("applications/rustidian.desktop"))
            .find(|path| path.is_file())
        else {
            return Ok(());
        };
        source
    };
    let entry = match std::fs::read_to_string(&source) {
        Ok(entry) => entry,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };
    if !entry.lines().any(|line| line == "StartupWMClass=rustidian") {
        return Ok(());
    }
    let mut in_entry = false;
    let mut offset = 0;
    let icon_range = entry.split_inclusive('\n').find_map(|line| {
        let start = offset;
        offset += line.len();
        if line.starts_with('[') {
            in_entry = line.trim_end() == "[Desktop Entry]";
        }
        (in_entry && line.starts_with("Icon=")).then_some(start..offset)
    });
    let Some(range) = icon_range else {
        return Ok(());
    };
    let name = match palette {
        Palette::Teal => "teal",
        Palette::LoveSpace => "love-space",
        Palette::ElpisGold => "elpis-gold",
        Palette::Signature => "signature",
    };
    let icon = data.join(format!("icons/hicolor/256x256/apps/rustidian-{name}.png"));
    let atomic_write = |path: &std::path::Path, bytes: &[u8]| -> std::io::Result<()> {
        let parent = path.parent().expect("Application data paths have a parent");
        std::fs::create_dir_all(parent)?;
        let mut file = tempfile::NamedTempFile::new_in(parent)?;
        file.write_all(bytes)?;
        file.persist(path)?;
        Ok(())
    };
    if std::fs::read(&icon).ok().as_deref() != Some(palette.logo().1) {
        atomic_write(&icon, palette.logo().1)?;
    }
    // A distinct filename per palette invalidates GNOME's cached launcher image.
    let escaped = icon
        .to_string_lossy()
        .replace('\\', "\\\\")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t");
    let replacement = format!("Icon={escaped}\n");
    if &entry[range.clone()] != replacement {
        let mut updated = entry;
        updated.replace_range(range, &replacement);
        atomic_write(&launcher, updated.as_bytes())?;
    }
    Ok(())
}

pub fn picker(ui: &mut egui::Ui, selected: &mut Palette, dark: bool) -> bool {
    let before = *selected;
    ui.strong("Palette");
    ui.add_space(6.0);
    egui::ScrollArea::horizontal()
        .id_salt("theme-cards")
        .show(ui, |ui| {
            let width = ((ui.available_width() - 24.0) / 4.0).clamp(116.0, 210.0);
            ui.horizontal(|ui| {
                for palette in [
                    Palette::Teal,
                    Palette::LoveSpace,
                    Palette::ElpisGold,
                    Palette::Signature,
                ] {
                    let (rect, response) =
                        ui.allocate_exact_size(egui::vec2(width, 154.0), egui::Sense::click());
                    let active = *selected == palette;
                    let accent = palette.accent(dark);
                    let background = palette.background(dark);
                    ui.painter().rect_filled(rect, 7, background);
                    ui.painter().rect_stroke(
                        rect,
                        7,
                        Stroke::new(
                            if active { 2.0 } else { 1.0 },
                            if active || response.hovered() {
                                accent
                            } else {
                                ui.visuals().widgets.noninteractive.bg_stroke.color
                            },
                        ),
                        egui::StrokeKind::Inside,
                    );
                    let (uri, bytes) = palette.logo();
                    egui::Image::from_bytes(uri, bytes).paint_at(
                        ui,
                        egui::Rect::from_center_size(
                            egui::pos2(rect.center().x, rect.top() + 32.0),
                            egui::vec2(44.0, 44.0),
                        ),
                    );
                    let foreground = if dark {
                        Color32::from_rgb(233, 229, 220)
                    } else {
                        Color32::from_rgb(34, 42, 43)
                    };
                    ui.painter().text(
                        egui::pos2(rect.center().x, rect.top() + 65.0),
                        egui::Align2::CENTER_TOP,
                        palette.name(),
                        FontId::proportional(13.0),
                        foreground,
                    );
                    let swatches = match palette {
                        Palette::Signature => [
                            Color32::from_rgb(207, 18, 89),
                            Color32::from_rgb(250, 118, 107),
                            Color32::from_rgb(255, 216, 115),
                            palette.side(dark),
                        ],
                        _ => [
                            accent,
                            accent.gamma_multiply(0.7),
                            foreground,
                            palette.side(dark),
                        ],
                    };
                    let gap = 4.0;
                    let swatch_width = (width - 24.0 - 3.0 * gap) / 4.0;
                    for (index, color) in swatches.into_iter().enumerate() {
                        let swatch = egui::Rect::from_min_size(
                            rect.min + egui::vec2(12.0 + index as f32 * (swatch_width + gap), 91.0),
                            egui::vec2(swatch_width, 18.0),
                        );
                        ui.painter().rect_filled(swatch, 3, color);
                    }
                    ui.painter().text(
                        egui::pos2(rect.center().x, rect.top() + 128.0),
                        egui::Align2::CENTER_CENTER,
                        if active { "Selected" } else { "Use palette" },
                        FontId::proportional(12.0),
                        accent,
                    );
                    crate::ui_probe::record(
                        ui.ctx(),
                        format!("settings-palette:{}", palette.name()),
                        rect,
                    );
                    if response.clicked()
                        || response.has_focus()
                            && ui.input(|input| input.key_pressed(egui::Key::Enter))
                    {
                        *selected = palette;
                    }
                }
            });
        });
    ui.add_space(14.0);
    before != *selected
}
