//! Native, cached syntax colors; source text and Markdown ranges stay unchanged.
use eframe::egui::{self, Color32};
use rust_notes::editor::Markdown;
use std::{ops::Range, sync::OnceLock};
use syntect::{
    easy::HighlightLines,
    highlighting::{Color, StyleModifier, Theme, ThemeItem, ThemeSettings},
    parsing::SyntaxSet,
    util::LinesWithEndings,
};

#[derive(Clone)]
pub struct Token {
    pub range: Range<usize>,
    pub color: Color32,
}

struct Settings {
    syntaxes: SyntaxSet,
    dark: Theme,
    light: Theme,
    reading_dark: Theme,
    reading_light: Theme,
}

fn color(value: Color32) -> Color {
    Color {
        r: value.r(),
        g: value.g(),
        b: value.b(),
        a: 255,
    }
}

fn palette(dark: bool, reading: bool) -> Theme {
    let normal = if dark {
        crate::theme::DARK_MUTED
    } else {
        crate::theme::LIGHT_MUTED
    };
    let (comment, keyword, string, number, operator, macros) = if dark {
        (
            crate::theme::DARK_FAINT,
            Color32::from_rgb(250, 153, 205),
            Color32::from_rgb(68, 207, 110),
            Color32::from_rgb(168, 130, 255),
            Color32::from_rgb(251, 70, 76),
            Color32::from_rgb(83, 223, 221),
        )
    } else {
        (
            Color32::from_rgb(171, 171, 171),
            Color32::from_rgb(213, 57, 132),
            Color32::from_rgb(8, 185, 78),
            Color32::from_rgb(120, 82, 238),
            Color32::from_rgb(233, 49, 71),
            Color32::from_rgb(0, 191, 188),
        )
    };
    let mut scopes: Vec<_> = [
        ("comment", comment),
        ("string", string),
        ("constant.numeric", number),
        ("keyword, storage.type, storage.modifier", keyword),
        ("keyword.operator", operator),
        (
            "support.macro, support.function.macro, entity.name.function.macro",
            macros,
        ),
        ("storage.type.primitive, storage.type.numeric", normal),
    ]
    .into_iter()
    .map(|(scope, foreground)| ThemeItem {
        scope: scope.parse().expect("Static syntax scope selector"),
        style: StyleModifier {
            foreground: Some(color(foreground)),
            ..Default::default()
        },
    })
    .collect();
    if reading {
        scopes.push(ThemeItem {
            scope: "entity.name.function"
                .parse()
                .expect("Static function scope"),
            style: StyleModifier {
                foreground: Some(color(if dark {
                    Color32::from_rgb(224, 222, 113)
                } else {
                    Color32::from_rgb(224, 172, 0)
                })),
                ..Default::default()
            },
        });
    }
    Theme {
        settings: ThemeSettings {
            foreground: Some(color(normal)),
            ..Default::default()
        },
        scopes,
        ..Default::default()
    }
}

fn settings() -> &'static Settings {
    static SETTINGS: OnceLock<Settings> = OnceLock::new();
    SETTINGS.get_or_init(|| Settings {
        syntaxes: SyntaxSet::load_defaults_newlines(),
        dark: palette(true, false),
        light: palette(false, false),
        reading_dark: palette(true, true),
        reading_light: palette(false, true),
    })
}

#[derive(Default)]
struct Highlighter;

impl egui::cache::ComputerMut<(&str, &str, bool, bool), Vec<Token>> for Highlighter {
    fn compute(&mut self, (code, language, dark, reading): (&str, &str, bool, bool)) -> Vec<Token> {
        let settings = settings();
        let Some(syntax) = settings.syntaxes.find_syntax_by_token(language) else {
            return Vec::new();
        };
        let theme = match (dark, reading) {
            (true, false) => &settings.dark,
            (false, false) => &settings.light,
            (true, true) => &settings.reading_dark,
            (false, true) => &settings.reading_light,
        };
        let normal = if dark {
            crate::theme::DARK_MUTED
        } else {
            crate::theme::LIGHT_MUTED
        };
        let mut highlighter = HighlightLines::new(syntax, theme);
        let mut tokens: Vec<Token> = Vec::new();
        let mut offset = 0;
        for line in LinesWithEndings::from(code) {
            let Ok(highlighted) = highlighter.highlight_line(line, &settings.syntaxes) else {
                return Vec::new();
            };
            for (style, text) in highlighted {
                let end = offset + text.len();
                let mut foreground =
                    Color32::from_rgb(style.foreground.r, style.foreground.g, style.foreground.b);
                // Syntect groups Rust primitives with declaration keywords.
                // The measured reference leaves these types in the normal code color.
                if !reading
                    && syntax.name == "Rust"
                    && matches!(
                        text,
                        "u8" | "u16"
                            | "u32"
                            | "u64"
                            | "u128"
                            | "usize"
                            | "i8"
                            | "i16"
                            | "i32"
                            | "i64"
                            | "i128"
                            | "isize"
                            | "f32"
                            | "f64"
                            | "bool"
                            | "char"
                            | "str"
                    )
                {
                    foreground = normal;
                }
                if offset < end && foreground != normal {
                    if let Some(previous) = tokens.last_mut()
                        && previous.range.end == offset
                        && previous.color == foreground
                    {
                        previous.range.end = end;
                    } else {
                        tokens.push(Token {
                            range: offset..end,
                            color: foreground,
                        });
                    }
                }
                offset = end;
            }
        }
        tokens
    }
}

pub fn highlights(ui: &egui::Ui, source: &str, parsed: &Markdown) -> Vec<Token> {
    let mut result = Vec::new();
    for block in &parsed.code_blocks {
        let Some(range) = block.content_range(source) else {
            continue;
        };
        let tokens = code_tokens(ui, &source[range.clone()], &block.language, false);
        result.extend(tokens.into_iter().map(|mut token| {
            token.range = token.range.start + range.start..token.range.end + range.start;
            token
        }));
    }
    result
}

pub fn code_tokens(ui: &egui::Ui, body: &str, language: &str, reading: bool) -> Vec<Token> {
    let language = language.split_whitespace().next().unwrap_or("");
    if ["", "text", "txt", "plain", "plaintext"]
        .iter()
        .any(|plain| language.eq_ignore_ascii_case(plain))
    {
        return Vec::new();
    }
    type Cache = egui::cache::FrameCache<Vec<Token>, Highlighter>;
    ui.ctx().memory_mut(|memory| {
        memory
            .caches
            .cache::<Cache>()
            .get((body, language, ui.visuals().dark_mode, reading))
            .clone()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reading_and_editing_keep_their_measured_function_and_primitive_colors() {
        let code = "fn main() { let answer: u32 = 42; }\n";
        let ctx = egui::Context::default();
        for dark in [true, false] {
            crate::theme::apply(&ctx, dark);
            let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
                let normal = ui.visuals().weak_text_color();
                for reading in [false, true, false] {
                    let tokens = code_tokens(ui, code, "rust", reading);
                    let at = |needle: &str| {
                        let byte = code.find(needle).unwrap();
                        tokens
                            .iter()
                            .find(|token| token.range.contains(&byte))
                            .map_or(normal, |token| token.color)
                    };
                    assert_eq!(
                        at("main"),
                        if reading {
                            if dark {
                                Color32::from_rgb(224, 222, 113)
                            } else {
                                Color32::from_rgb(224, 172, 0)
                            }
                        } else {
                            normal
                        }
                    );
                    assert_eq!(
                        at("u32"),
                        if reading {
                            if dark {
                                Color32::from_rgb(250, 153, 205)
                            } else {
                                Color32::from_rgb(213, 57, 132)
                            }
                        } else {
                            normal
                        }
                    );
                }
            });
            output.textures_delta.clear();
        }
    }

    #[test]
    fn rust_colors_follow_the_reference_and_keep_strings_comments_and_unicode_literal() {
        let source = "fn outside\n\n~~~rust\nfn main() {\n let answer: u32 = 42;\n let text = r###\"first\nfn pretend **literal** [[Wiki]] π 你好\n\"###;\n /* outer\n let fake = 99;\n */ let live = 3;\n println!(\"hello {answer}\");\n}\n~~~\n";
        for dark in [true, false] {
            let ctx = egui::Context::default();
            crate::theme::apply(&ctx, dark);
            let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
                let tokens = highlights(ui, source, &Markdown::parse(source));
                let at = |needle: &str| {
                    let byte = source.find(needle).unwrap();
                    tokens
                        .iter()
                        .find(|token| token.range.contains(&byte))
                        .map_or(ui.visuals().weak_text_color(), |token| token.color)
                };
                let keyword = if dark {
                    Color32::from_rgb(250, 153, 205)
                } else {
                    Color32::from_rgb(213, 57, 132)
                };
                let string = if dark {
                    Color32::from_rgb(68, 207, 110)
                } else {
                    Color32::from_rgb(8, 185, 78)
                };
                let comment = if dark {
                    crate::theme::DARK_FAINT
                } else {
                    Color32::from_rgb(171, 171, 171)
                };
                assert_eq!(at("fn main"), keyword);
                assert_eq!(at("let live"), keyword);
                assert_eq!(at("fn pretend"), string);
                assert_eq!(at("let fake"), comment);
                assert_eq!(at("u32"), ui.visuals().weak_text_color());
                assert_eq!(
                    at("println!"),
                    if dark {
                        Color32::from_rgb(83, 223, 221)
                    } else {
                        Color32::from_rgb(0, 191, 188)
                    }
                );
                assert!(!tokens.iter().any(|token| token.range.contains(&0)));
                assert!(
                    tokens
                        .iter()
                        .all(|token| source.get(token.range.clone()).is_some())
                );
                for plain in [
                    "~~~text\nfn plain() {}\n~~~",
                    "~~~missing-language\nfn plain() {}\n~~~",
                ] {
                    assert!(highlights(ui, plain, &Markdown::parse(plain)).is_empty());
                }
            });
            output.textures_delta.clear();
        }
    }
}
