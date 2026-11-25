//! Settings → Appearance → Font, as in Obsidian: an installed family for the
//! interface, for note text and for code. An empty name keeps Rustidian's own font.
use eframe::egui::{self, FontFamily};
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Fonts {
    pub interface: String,
    pub text: String,
    pub monospace: String,
}

impl Fonts {
    /// Obsidian's `appearance.json` holds CSS font lists; the first family is the choice.
    pub fn from_obsidian(appearance: &serde_json::Value) -> Self {
        let first = |key: &str| {
            appearance
                .get(key)
                .and_then(serde_json::Value::as_str)
                .and_then(|list| list.split(',').next())
                .map(|name| name.trim().trim_matches(['"', '\'']).to_owned())
                .unwrap_or_default()
        };
        Self {
            interface: first("interfaceFontFamily"),
            text: first("textFontFamily"),
            monospace: first("monospaceFontFamily"),
        }
    }
}

/// The faces the interface uses; note text has its own copy of each, `text-<face>`.
const FACES: [&str; 4] = ["regular", "semibold", "bold", "italic"];

/// The note-text family for an interface face.
pub fn text(face: &str) -> FontFamily {
    FontFamily::Name(format!("text-{face}").into())
}

fn interface(face: &str) -> FontFamily {
    match face {
        "regular" => FontFamily::Proportional,
        face => FontFamily::Name(face.into()),
    }
}

/// Installed families, by name, for the pickers; monospace ones only if `mono`.
pub fn installed(mono: bool) -> &'static [String] {
    static ALL: OnceLock<Vec<String>> = OnceLock::new();
    static MONO: OnceLock<Vec<String>> = OnceLock::new();
    let list = |pattern: &str| {
        let output = std::process::Command::new("fc-list")
            .args([pattern, "family"])
            .output()
            .map(|output| String::from_utf8_lossy(&output.stdout).into_owned())
            .unwrap_or_default();
        let mut names: Vec<String> = output
            .lines()
            .filter_map(|line| line.split(',').next())
            .map(|name| name.trim().to_owned())
            .filter(|name| !name.is_empty() && !name.starts_with('.'))
            .collect();
        names.sort_by_key(|name| name.to_lowercase());
        names.dedup();
        names
    };
    if mono {
        MONO.get_or_init(|| list(":spacing=mono"))
    } else {
        ALL.get_or_init(|| list(":"))
    }
}

/// Installed face path and index returned by Fontconfig when `family` exists.
fn match_face(family: &str, face: &str) -> Option<(String, u32)> {
    let style = match face {
        "semibold" => ":weight=semibold",
        "bold" => ":weight=bold",
        "italic" => ":slant=italic",
        _ => "",
    };
    let output = std::process::Command::new("fc-match")
        .args([
            "-f",
            "%{file}\n%{index}\n%{family}",
            &format!("{family}{style}"),
        ])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let output = String::from_utf8_lossy(&output.stdout);
    let mut lines = output.lines();
    let (path, index, families) = (lines.next()?, lines.next()?, lines.next()?);
    // fc-match always answers; a different family means this one is not installed.
    families
        .split(',')
        .any(|name| name.trim().eq_ignore_ascii_case(family.trim()))
        .then_some(())?;
    Some((path.to_owned(), index.parse().ok()?))
}

/// A loadable installed face, or `None` if Fontconfig cannot supply one.
fn file(family: &str, face: &str) -> Option<egui::FontData> {
    let (path, index) = match_face(family, face)?;
    validated_font(std::fs::read(path).ok()?, index)
}

/// True when the note-text italic family resolves to a genuine italic font file rather than
/// falling back to the upright regular face (so egui must not apply extra synthetic shear).
pub fn text_uses_true_italic(fonts: &Fonts) -> bool {
    let name = fonts.text.trim();
    if name.is_empty() || file(name, "regular").is_none() {
        return true;
    }
    match (match_face(name, "italic"), match_face(name, "regular")) {
        (Some(italic), Some(regular)) => italic != regular && file(name, "italic").is_some(),
        _ => false,
    }
}

fn true_italic_id() -> egui::Id {
    egui::Id::unique("rustidian-text-true-italic")
}

pub fn set_text_has_true_italic(ctx: &egui::Context, has: bool) {
    ctx.data_mut(|data| data.insert_temp(true_italic_id(), has));
}

pub fn text_has_true_italic(ctx: &egui::Context) -> bool {
    ctx.data(|data| data.get_temp::<bool>(true_italic_id()).unwrap_or(true))
}

fn validated_font(bytes: Vec<u8>, index: u32) -> Option<egui::FontData> {
    let mut data = egui::FontData::from_owned(bytes);
    // Fontconfig puts a variable font's named instance in the high bits.
    // The renderer takes only the collection face index.
    data.index = index & 0xffff;
    data.is_parseable().then_some(data)
}

/// Add the chosen families in front of Rustidian's own, and give note text
/// its own families. Returns the choices that could not be loaded.
pub fn apply(definitions: &mut egui::FontDefinitions, fonts: &Fonts) -> Vec<String> {
    for face in FACES {
        let own = definitions
            .families
            .get(&interface(face))
            .cloned()
            .unwrap_or_default();
        definitions.families.insert(text(face), own);
    }
    let mut missing = Vec::new();
    for (name, family) in [
        (&fonts.interface, interface as fn(&str) -> FontFamily),
        (&fonts.text, text),
    ] {
        let name = name.trim();
        if name.is_empty() {
            continue;
        }
        let Some(regular) = file(name, "regular") else {
            missing.push(name.to_owned());
            continue;
        };
        for face in FACES {
            // A family without this face draws it with its regular file.
            let key = format!("{name}-{face}");
            let data = file(name, face).unwrap_or_else(|| regular.clone());
            definitions.font_data.insert(key.clone(), data.into());
            if let Some(list) = definitions.families.get_mut(&family(face)) {
                list.insert(0, key);
            }
        }
    }
    let name = fonts.monospace.trim();
    if !name.is_empty() {
        match file(name, "regular") {
            Some(data) => {
                definitions.font_data.insert(name.to_owned(), data.into());
                if let Some(list) = definitions.families.get_mut(&FontFamily::Monospace) {
                    list.insert(0, name.to_owned());
                }
            }
            None => missing.push(name.to_owned()),
        }
    }
    missing
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn obsidian_font_lists_give_their_first_family() {
        let appearance = serde_json::json!({
            "textFontFamily": "\"Source Serif 4\", Georgia",
            "monospaceFontFamily": "JetBrains Mono",
        });
        assert_eq!(
            Fonts::from_obsidian(&appearance),
            Fonts {
                interface: String::new(),
                text: "Source Serif 4".into(),
                monospace: "JetBrains Mono".into(),
            }
        );
    }

    #[test]
    fn fontconfig_match_must_open_at_the_reported_face_index() {
        let valid = include_bytes!("../assets/fonts/Inter-Regular.otf");
        assert!(validated_font(valid.to_vec(), 0).is_some());
        assert!(validated_font(valid.to_vec(), 1).is_none());
        assert!(validated_font(valid.to_vec(), 1 << 16).is_some());
        assert!(validated_font(b"%!PS-AdobeFont-1.0".to_vec(), 0).is_none());
    }

    #[test]
    fn note_text_keeps_rustidian_fonts_until_one_is_chosen() {
        let mut definitions = egui::FontDefinitions::default();
        definitions
            .families
            .insert(FontFamily::Name("bold".into()), vec!["Hack".into()]);
        let missing = apply(
            &mut definitions,
            &Fonts {
                text: "No Such Font 4096".into(),
                ..Default::default()
            },
        );
        assert_eq!(missing, ["No Such Font 4096"]);
        assert_eq!(definitions.families[&text("bold")], ["Hack"]);
        assert_eq!(
            definitions.families[&text("regular")],
            definitions.families[&FontFamily::Proportional]
        );
    }

    #[test]
    fn default_note_font_uses_bundled_true_italic() {
        assert!(text_uses_true_italic(&Fonts::default()));
        assert!(text_uses_true_italic(&Fonts {
            text: "No Such Font 4096".into(),
            ..Default::default()
        }));
    }
}
