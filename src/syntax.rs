//! Native, cached syntax colors; source text and Markdown ranges stay unchanged.
use eframe::egui::{self, Color32};
use rustidian::editor::Markdown;
use std::{
    collections::{HashMap, HashSet},
    hash::{DefaultHasher, Hash, Hasher},
    ops::Range,
    sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError, mpsc},
    time::{Duration, Instant},
};
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

// Preserve measured default grammars; load additional languages only on demand.
static EXTRA: OnceLock<SyntaxSet> = OnceLock::new();

fn extra_syntaxes() -> &'static SyntaxSet {
    EXTRA.get_or_init(two_face::syntax::extra_newlines)
}

/// Load the default grammars off the UI thread, so the first note with code
/// does not wait for them.
pub fn prepare() {
    let _ = std::thread::Builder::new()
        .name("rustidian-grammars".into())
        .spawn(|| {
            settings();
        });
}

/// Whether `language` can be highlighted without first loading the extra grammars.
fn loaded(language: &str) -> bool {
    EXTRA.get().is_some() || settings().syntaxes.find_syntax_by_token(language).is_some()
}

fn compute(code: &str, language: &str, dark: bool, reading: bool) -> Vec<Token> {
    let settings = settings();
    let syntaxes = if settings.syntaxes.find_syntax_by_token(language).is_some() {
        &settings.syntaxes
    } else {
        extra_syntaxes()
    };
    let Some(syntax) = syntaxes.find_syntax_by_token(language) else {
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
        let Ok(highlighted) = highlighter.highlight_line(line, syntaxes) else {
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

/// Highlighting a note's code blocks on the UI thread is capped per frame;
/// the rest arrives from a background thread (27 Oct: opening a note with 375
/// blocks froze one frame for 764 ms).
const BUDGET: Duration = Duration::from_millis(8);

/// Tokens shared by every view, kept while recent frames use them.
#[derive(Default)]
struct Cache {
    tokens: HashMap<u64, (Arc<Vec<Token>>, u64)>,
    queued: HashSet<u64>,
    /// `BUDGET` spans every layout of one frame.
    spent: (u64, Duration),
}

impl Cache {
    fn lookup(&mut self, key: u64, pass: u64) -> Option<Arc<Vec<Token>>> {
        let (tokens, used) = self.tokens.get_mut(&key)?;
        *used = pass;
        Some(tokens.clone())
    }

    fn insert(&mut self, key: u64, tokens: Arc<Vec<Token>>, pass: u64) {
        // Each edit inside a block leaves its previous text behind.
        if self.tokens.len() >= 4096 {
            self.tokens.retain(|_, (_, used)| *used + 600 > pass);
        }
        self.tokens.insert(key, (tokens, pass));
        GENERATION.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }
}

fn cache() -> MutexGuard<'static, Cache> {
    static CACHE: OnceLock<Mutex<Cache>> = OnceLock::new();
    CACHE
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
}

fn key(code: &str, language: &str, dark: bool, reading: bool) -> u64 {
    let mut hasher = DefaultHasher::new();
    (code, language, dark, reading).hash(&mut hasher);
    hasher.finish()
}

struct Job {
    key: u64,
    code: String,
    language: String,
    dark: bool,
    pass: u64,
    ctx: egui::Context,
}

fn finish(job: &Job) {
    let tokens = Arc::new(compute(&job.code, &job.language, job.dark, false));
    let mut cache = cache();
    cache.queued.remove(&job.key);
    cache.insert(job.key, tokens, job.pass);
}

/// Highlight `job` off the UI thread and repaint once it is ready.
fn queue(job: Job) {
    if !cache().queued.insert(job.key) {
        return;
    }
    static WORKER: OnceLock<mpsc::Sender<Job>> = OnceLock::new();
    let worker = WORKER.get_or_init(|| {
        let (sender, jobs) = mpsc::channel::<Job>();
        let _ = std::thread::Builder::new()
            .name("rustidian-highlight".into())
            .spawn(move || {
                while let Ok(job) = jobs.recv() {
                    finish(&job);
                    let mut ctx = job.ctx;
                    for job in jobs.try_iter() {
                        finish(&job);
                        ctx = job.ctx;
                    }
                    ctx.request_repaint();
                }
            });
        sender
    });
    if let Err(mpsc::SendError(job)) = worker.send(job) {
        finish(&job);
    }
}

static GENERATION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Changes whenever a block's tokens arrive, so a cached layout knows to refresh.
pub fn generation() -> u64 {
    GENERATION.load(std::sync::atomic::Ordering::Relaxed)
}

pub fn highlights(ui: &egui::Ui, source: &str, parsed: &Markdown) -> Vec<Token> {
    let started = Instant::now();
    let dark = ui.visuals().dark_mode;
    let pass = ui.ctx().cumulative_pass_nr();
    let budget = {
        let mut cache = cache();
        if cache.spent.0 != pass {
            cache.spent = (pass, Duration::ZERO);
        }
        BUDGET.saturating_sub(cache.spent.1)
    };
    let mut result = Vec::new();
    for block in &parsed.code_blocks {
        let Some(range) = block.content_range(source) else {
            continue;
        };
        let Some(language) = grammar(&block.language) else {
            continue;
        };
        let code = &source[range.clone()];
        let key = key(code, &language, dark, false);
        let cached = cache().lookup(key, pass);
        let tokens = match cached {
            Some(tokens) => tokens,
            None if started.elapsed() < budget && loaded(&language) => {
                let tokens = Arc::new(compute(code, &language, dark, false));
                cache().insert(key, tokens.clone(), pass);
                tokens
            }
            None => {
                queue(Job {
                    key,
                    code: code.to_owned(),
                    language,
                    dark,
                    pass,
                    ctx: ui.ctx().clone(),
                });
                continue;
            }
        };
        result.extend(tokens.iter().map(|token| Token {
            range: token.range.start + range.start..token.range.end + range.start,
            color: token.color,
        }));
    }
    cache().spent.1 += started.elapsed();
    result
}

/// The grammar name for a fence's language, or `None` for plain text.
fn grammar(language: &str) -> Option<String> {
    let token = language
        .split_whitespace()
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    let language = match token.as_str() {
        "mjs" | "cjs" | "javascript" => "js",
        "typescript" | "mts" | "cts" => "ts",
        // Fancy-regex excludes Babel; TypeScriptReact handles JSX too.
        "jsx" | "typescriptreact" => "tsx",
        "pyw" | "python" => "py",
        "jsonc" => "json",
        "shell" | "sh" => "bash",
        _ => token.as_str(),
    };
    (!["", "text", "txt", "plain", "plaintext"]
        .iter()
        .any(|plain| language.eq_ignore_ascii_case(plain)))
    .then(|| language.to_owned())
}

pub fn code_tokens(ui: &egui::Ui, body: &str, language: &str, reading: bool) -> Vec<Token> {
    let Some(language) = grammar(language) else {
        return Vec::new();
    };
    let dark = ui.visuals().dark_mode;
    let pass = ui.ctx().cumulative_pass_nr();
    let key = key(body, &language, dark, reading);
    let cached = cache().lookup(key, pass);
    let tokens = cached.unwrap_or_else(|| {
        let tokens = Arc::new(compute(body, &language, dark, reading));
        cache().insert(key, tokens.clone(), pass);
        tokens
    });
    tokens.to_vec()
}

pub fn source_job(
    ui: &egui::Ui,
    text: &str,
    path: &std::path::Path,
    width: f32,
    matched: Option<&Range<usize>>,
) -> egui::text::LayoutJob {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("");
    let tokens = code_tokens(ui, text, extension, false);
    let byte = |character| {
        text.char_indices()
            .nth(character)
            .map_or(text.len(), |(byte, _)| byte)
    };
    let matched = matched.map(|range| byte(range.start)..byte(range.end));
    let mut boundaries = vec![0, text.len()];
    for token in &tokens {
        boundaries.extend([token.range.start, token.range.end]);
    }
    if let Some(range) = &matched {
        boundaries.extend([range.start, range.end]);
    }
    boundaries.sort_unstable();
    boundaries.dedup();
    let mut job = egui::text::LayoutJob::default();
    job.wrap.max_width = width;
    let mut token = 0;
    for range in boundaries.windows(2) {
        while token < tokens.len() && tokens[token].range.end <= range[0] {
            token += 1;
        }
        let color = tokens
            .get(token)
            .filter(|token| token.range.contains(&range[0]))
            .map_or(ui.visuals().text_color(), |token| token.color);
        job.append(
            &text[range[0]..range[1]],
            0.0,
            egui::TextFormat {
                font_id: egui::FontId::monospace(crate::theme::document_size(ui, 16.0)),
                color,
                background: if matched
                    .as_ref()
                    .is_some_and(|matched| matched.contains(&range[0]))
                {
                    ui.visuals().selection.bg_fill
                } else {
                    Color32::TRANSPARENT
                },
                ..Default::default()
            },
        );
    }
    job
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

    #[test]
    fn language_files_and_fences_color_real_tokens_without_changing_text() {
        let cases: &[(&[&str], &str, Option<&str>)] = &[
            (
                &["ts", "typescript", "mts", "cts"],
                "const label: string = \"سلام\"; // comment\r\n",
                Some("comment"),
            ),
            (
                &["rs", "rust"],
                "let label = \"سلام\"; // comment\r\n",
                Some("comment"),
            ),
            (
                &["py", "python", "pyw"],
                "label = \"سلام\" # comment\r\n",
                Some("comment"),
            ),
            (
                &["js", "javascript", "mjs", "cjs"],
                "const label = \"سلام\"; // comment\r\n",
                Some("comment"),
            ),
            (
                &["tsx", "jsx", "typescriptreact"],
                "const node = <div title=\"سلام\">hello</div>; // comment\r\n",
                Some("comment"),
            ),
            (&["json", "jsonc"], "{\"label\": \"سلام\"}\r\n", None),
            (
                &["bash", "sh", "shell"],
                "label=\"سلام\" # comment\r\n",
                Some("comment"),
            ),
        ];
        for dark in [true, false] {
            let ctx = egui::Context::default();
            crate::theme::apply(&ctx, dark);
            let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
                let string_color = if dark {
                    Color32::from_rgb(68, 207, 110)
                } else {
                    Color32::from_rgb(8, 185, 78)
                };
                let comment_color = if dark {
                    crate::theme::DARK_FAINT
                } else {
                    Color32::from_rgb(171, 171, 171)
                };
                for &(aliases, body, comment) in cases {
                    for &alias in aliases {
                        for language in [
                            alias.to_owned(),
                            format!("{} options", alias.to_ascii_uppercase()),
                        ] {
                            for reading in [false, true] {
                                let tokens = code_tokens(ui, body, &language, reading);
                                let at = |needle: &str| {
                                    let byte = body.find(needle).unwrap();
                                    tokens
                                        .iter()
                                        .find(|token| token.range.contains(&byte))
                                        .map(|token| token.color)
                                };
                                assert_eq!(
                                    at("سلام"),
                                    Some(string_color),
                                    "{language}, reading={reading}"
                                );
                                if let Some(comment) = comment {
                                    assert_eq!(
                                        at(comment),
                                        Some(comment_color),
                                        "{language}, reading={reading}"
                                    );
                                }
                                assert!(
                                    tokens
                                        .iter()
                                        .all(|token| body.get(token.range.clone()).is_some())
                                );
                            }
                            let source = format!("~~~{language}\r\n{body}~~~\r\n");
                            let byte = source.find("سلام").unwrap();
                            let tokens = highlights(ui, &source, &Markdown::parse(&source));
                            assert!(
                                tokens.iter().any(|token| token.range.contains(&byte)
                                    && token.color == string_color),
                                "{language}"
                            );
                        }
                        let path = std::path::PathBuf::from(format!(
                            "example.{}",
                            alias.to_ascii_uppercase()
                        ));
                        let job = source_job(ui, body, &path, 800.0, None);
                        assert_eq!(
                            job.text, body,
                            "{alias}: CRLF/Unicode source must stay literal"
                        );
                        let byte = body.find("سلام").unwrap();
                        assert_eq!(
                            job.format_at_byte(egui::text::ByteIndex(byte)).color,
                            string_color,
                            "{alias}"
                        );
                    }
                }
            });
            output.textures_delta.clear();
        }
    }
}
