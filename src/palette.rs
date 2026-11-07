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

/// The palette belongs to the app, not to one vault: every vault, folder and file opens in the
/// palette chosen last. One small file beside the per-vault sessions holds it.
pub fn global_file(state_base: &std::path::Path) -> std::path::PathBuf {
    rustidian::paths::app_directory(state_base).join("palette")
}

fn state_base() -> Option<std::path::PathBuf> {
    std::env::var_os("XDG_STATE_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME").map(|home| std::path::PathBuf::from(home).join(".local/state"))
        })
}

pub fn read_global(state_base: &std::path::Path) -> Option<Palette> {
    serde_json::from_slice(&std::fs::read(global_file(state_base)).ok()?).ok()
}

pub fn write_global(state_base: &std::path::Path, palette: Palette) -> std::io::Result<()> {
    let path = global_file(state_base);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let temporary = path.with_extension("tmp");
    std::fs::write(&temporary, serde_json::to_vec(&palette)?)?;
    std::fs::rename(temporary, path)
}

/// The palette to open with: the app-wide choice, or this session's own the first time (which
/// then becomes the app-wide choice, so an existing palette is kept when upgrading).
pub fn open_with(session: Palette) -> Palette {
    let Some(base) = state_base() else {
        return session;
    };
    read_global(&base).unwrap_or_else(|| {
        let _ = write_global(&base, session);
        session
    })
}

/// The app-wide palette for a vault that has no saved session yet; teal until one is chosen.
pub fn app_wide() -> Palette {
    state_base()
        .and_then(|base| read_global(&base))
        .unwrap_or_default()
}

/// Light, dark or system also belongs to the app: switching vault never changes it (5 Nov).
fn appearance_file(state_base: &std::path::Path) -> std::path::PathBuf {
    rustidian::paths::app_directory(state_base).join("appearance")
}

fn read_appearance(state_base: &std::path::Path) -> Option<super::Appearance> {
    serde_json::from_slice(&std::fs::read(appearance_file(state_base)).ok()?).ok()
}

fn write_appearance(
    state_base: &std::path::Path,
    appearance: super::Appearance,
) -> std::io::Result<()> {
    let path = appearance_file(state_base);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let temporary = path.with_extension("tmp");
    std::fs::write(&temporary, serde_json::to_vec(&appearance)?)?;
    std::fs::rename(temporary, path)
}

/// The appearance to open with: the app-wide choice, or this session's own the first time.
pub fn open_appearance_with(session: super::Appearance) -> super::Appearance {
    let Some(base) = state_base() else {
        return session;
    };
    read_appearance(&base).unwrap_or_else(|| {
        let _ = write_appearance(&base, session);
        session
    })
}

/// The app-wide appearance for a vault that has no saved session yet.
pub fn app_wide_appearance() -> super::Appearance {
    state_base()
        .and_then(|base| read_appearance(&base))
        .unwrap_or_default()
}

/// Remember an appearance chosen by the user for every vault.
pub fn remember_appearance(appearance: super::Appearance) {
    if let Some(base) = state_base() {
        let _ = write_appearance(&base, appearance);
    }
}

/// Remember a palette chosen in Settings for every vault.
pub fn remember(palette: Palette) {
    if let Some(base) = state_base() {
        let _ = write_global(&base, palette);
    }
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
        if let Err(error) = sync_launcher_icon(palette) {
            eprintln!("Cannot update Rustidian's launcher icon: {error}");
        }
    }
}

/// Shows the palette's logo in the dock by replacing the themed `rustidian`
/// icon. Launcher files are never edited: GNOME reloads a changed entry and
/// then loses track of the running window (no running mark, no raise).
#[cfg(target_os = "linux")]
fn sync_launcher_icon(palette: Palette) -> std::io::Result<()> {
    use std::{io::Write, path::PathBuf};
    let Some(data) = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share")))
    else {
        return Ok(());
    };
    let theme = data.join("icons/hicolor");
    let icon = theme.join("256x256/apps/rustidian.png");
    if std::fs::read(&icon).ok().as_deref() == Some(palette.logo().1) {
        return Ok(());
    }
    let parent = icon.parent().expect("The icon path has a parent");
    std::fs::create_dir_all(parent)?;
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    file.write_all(palette.logo().1)?;
    file.persist(&icon)?;
    // Icon themes rescan only when the theme folder's time changes.
    std::fs::File::open(&theme)?.set_modified(std::time::SystemTime::now())
}

pub fn picker(ui: &mut egui::Ui, selected: &mut Palette, dark: bool) -> bool {
    let before = *selected;
    ui.strong("Palette");
    ui.add_space(crate::theme::space::S);
    egui::ScrollArea::horizontal()
        .id_salt("theme-cards")
        .show(ui, |ui| {
            let width = ((ui.available_width() - 3.0 * ui.spacing().item_spacing.x) / 4.0)
                .clamp(116.0, 210.0);
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
                    ui.painter()
                        .rect_filled(rect, super::radius::CARD, background);
                    ui.painter().rect_stroke(
                        rect,
                        super::radius::CARD,
                        Stroke::new(
                            if active || response.has_focus() {
                                2.0
                            } else {
                                1.0
                            },
                            if active || response.hovered() || response.has_focus() {
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
                    let gap = super::space::XS;
                    let swatch_width = (width - 2.0 * super::space::M - 3.0 * gap) / 4.0;
                    for (index, color) in swatches.into_iter().enumerate() {
                        let swatch = egui::Rect::from_min_size(
                            rect.min
                                + egui::vec2(
                                    super::space::M + index as f32 * (swatch_width + gap),
                                    91.0,
                                ),
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
    ui.add_space(crate::theme::space::M);
    before != *selected
}

#[cfg(test)]
mod global_tests {
    use super::*;

    #[test]
    fn the_chosen_palette_is_kept_for_every_vault() {
        let base = tempfile::tempdir().unwrap();
        assert!(read_global(base.path()).is_none());
        write_global(base.path(), Palette::LoveSpace).unwrap();
        assert!(read_global(base.path()) == Some(Palette::LoveSpace));
        write_global(base.path(), Palette::Signature).unwrap();
        assert!(read_global(base.path()) == Some(Palette::Signature));
        assert!(!global_file(base.path()).with_extension("tmp").exists());
    }

    #[test]
    fn light_dark_or_system_is_kept_for_every_vault() {
        use crate::theme::Appearance;
        let base = tempfile::tempdir().unwrap();
        assert!(read_appearance(base.path()).is_none());
        write_appearance(base.path(), Appearance::Dark).unwrap();
        assert_eq!(read_appearance(base.path()), Some(Appearance::Dark));
        write_appearance(base.path(), Appearance::Light).unwrap();
        assert_eq!(read_appearance(base.path()), Some(Appearance::Light));
    }

    #[test]
    fn a_damaged_file_means_no_choice_not_a_crash() {
        let base = tempfile::tempdir().unwrap();
        let path = global_file(base.path());
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, b"{not json").unwrap();
        assert!(read_global(base.path()).is_none());
    }
}
