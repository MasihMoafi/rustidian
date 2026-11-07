use eframe::egui;
use egui_commonmark::CommonMarkCache;
use rustidian::vault::{Document, Vault};
use std::{
    ops::Range,
    path::{Path, PathBuf},
};

#[derive(Clone, PartialEq)]
pub struct Source {
    pub path: PathBuf,
    pub range: Range<usize>,
    pub text: String,
}

#[derive(Clone, PartialEq)]
pub struct Embed {
    pub id: egui::Id,
    pub target: String,
    pub source: Result<Source, String>,
}

#[derive(Clone, Copy)]
struct Height {
    width: f32,
    height: f32,
}

pub fn height(ctx: &egui::Context, id: egui::Id, width: f32) -> f32 {
    ctx.data(|data| data.get_temp::<Height>(id.with("height")))
        .filter(|cached| (cached.width - width).abs() < 1.0)
        .map_or(120.0, |cached| cached.height)
}

pub fn is_note(target: &str) -> bool {
    let file = target
        .split('|')
        .next()
        .unwrap_or(target)
        .split('#')
        .next()
        .unwrap_or_default();
    !file.contains("://")
        && (file.is_empty()
            || Path::new(file).extension().is_none()
            || rustidian::vault::is_markdown(Path::new(file)))
}

fn text<'a>(
    vault: &'a Vault,
    current_path: &Path,
    current_text: &'a str,
    open: &'a [(&'a Path, &'a str)],
    path: &Path,
) -> Option<&'a str> {
    if path == current_path {
        Some(current_text)
    } else {
        open.iter()
            .find_map(|(candidate, text)| (*candidate == path).then_some(*text))
            .or_else(|| {
                vault
                    .notes
                    .iter()
                    .find(|note| note.path == path)
                    .map(|note| note.text.as_str())
            })
    }
}

fn section(source: &str, fragment: &str) -> Option<Range<usize>> {
    if let Some(id) = fragment.strip_prefix('^') {
        return rustidian::structure::blocks(source)
            .into_iter()
            .find(|block| block.id.as_deref() == Some(id))
            .map(|block| block.range);
    }
    let headings = rustidian::structure::outline(source);
    let index = headings
        .iter()
        .position(|heading| heading.title.to_lowercase() == fragment.trim().to_lowercase())?;
    let heading = &headings[index];
    let byte = |character| {
        source
            .char_indices()
            .nth(character)
            .map_or(source.len(), |(index, _)| index)
    };
    let end = headings[index + 1..]
        .iter()
        .find(|next| next.level <= heading.level)
        .map_or(source.len(), |next| byte(next.character));
    Some(byte(heading.character)..end)
}

/// A note an outside file names, found in that file's own vault (the nearest folder above
/// it holding `.obsidian`, else its folder): e.g. Main Vault's day page, open as a guest in
/// another window, embeds `![[Now]]` from Main Vault. Reads are cached for two seconds,
/// since embeds are drawn every frame.
fn resolve_outside(from: &Path, target: &str) -> Option<(PathBuf, String)> {
    use std::collections::HashMap;
    use std::sync::{LazyLock, Mutex};
    use std::time::{Duration, Instant};
    type Found = Option<(PathBuf, String)>;
    static CACHE: LazyLock<Mutex<HashMap<(PathBuf, String), (Instant, Found)>>> =
        LazyLock::new(Default::default);
    let folder = from.parent()?;
    let home = folder
        .ancestors()
        .find(|dir| dir.join(".obsidian").is_dir())
        .unwrap_or(folder)
        .to_owned();
    let file = target.split('#').next().unwrap_or(target).trim();
    if file.is_empty() {
        return None;
    }
    let key = (home.clone(), file.to_owned());
    let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((at, found)) = cache.get(&key)
        && at.elapsed() < Duration::from_secs(2)
    {
        return found.clone();
    }
    let name = rustidian::paths::markdown_name(Path::new(file));
    let direct = [folder.join(&name), home.join(&name)]
        .into_iter()
        .find(|path| path.is_file());
    let path = direct.or_else(|| {
        let wanted = name.file_name()?.to_owned();
        walkdir::WalkDir::new(&home)
            .into_iter()
            .filter_entry(|entry| !entry.file_name().to_string_lossy().starts_with('.'))
            .filter_map(Result::ok)
            .find(|entry| entry.file_type().is_file() && entry.file_name() == wanted)
            .map(|entry| entry.into_path())
    });
    let found = path.and_then(|path| Some((path.clone(), std::fs::read_to_string(&path).ok()?)));
    cache.insert(key, (Instant::now(), found.clone()));
    found
}

pub fn resolve(
    vault: &Vault,
    current_path: &Path,
    current_text: &str,
    open: &[(&Path, &str)],
    from: &Path,
    target: &str,
) -> Result<Source, String> {
    let target = target.split('|').next().unwrap_or(target);
    let missing = || format!("\u{201c}{target}\u{201d} could not be found.");
    let outside;
    let (path, full) = match vault.resolve(from, target).and_then(|path| {
        Some((
            path.clone(),
            text(vault, current_path, current_text, open, &path)?,
        ))
    }) {
        Some(found) => found,
        None if from.is_absolute() => {
            let (path, text) = resolve_outside(from, target).ok_or_else(missing)?;
            outside = text;
            (path, outside.as_str())
        }
        None => return Err(missing()),
    };
    let range = if let Some((_, fragment)) = target
        .split_once('#')
        .filter(|(_, fragment)| !fragment.is_empty())
    {
        section(full, fragment).ok_or_else(|| format!("Heading or block not found: {fragment}"))?
    } else {
        0..full.len()
    };
    Ok(Source {
        path,
        text: full[range.clone()].into(),
        range,
    })
}

pub struct Context<'a> {
    pub vault: &'a Vault,
    current: &'a Document,
    open: &'a [(&'a Path, &'a str)],
    stack: Vec<(PathBuf, Range<usize>)>,
    pub actions: &'a mut Vec<crate::page_preview::Action>,
    pub error: &'a mut Option<String>,
}

impl<'a> Context<'a> {
    pub fn new(
        vault: &'a Vault,
        current: &'a Document,
        open: &'a [(&'a Path, &'a str)],
        actions: &'a mut Vec<crate::page_preview::Action>,
        error: &'a mut Option<String>,
    ) -> Self {
        Self {
            vault,
            current,
            open,
            stack: vec![(current.path.clone(), 0..current.text.len())],
            actions,
            error,
        }
    }

    pub fn nested(
        &mut self,
        ui: &mut egui::Ui,
        cache: &mut CommonMarkCache,
        from: &Path,
        target: &str,
        id: egui::Id,
    ) {
        let embed = Embed {
            id,
            target: target.into(),
            source: resolve(
                self.vault,
                &self.current.path,
                &self.current.text,
                self.open,
                from,
                target,
            ),
        };
        self.show(ui, cache, from, &embed, false);
    }

    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        cache: &mut CommonMarkCache,
        from: &Path,
        embed: &Embed,
        edit_source: bool,
    ) -> bool {
        let width = ui.available_width();
        let mut edit = false;
        let frame = egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.set_width((width - 18.0).max(40.0));
            let source = match &embed.source {
                Ok(source) => source,
                Err(error) => {
                    let response = ui.weak(error);
                    crate::ui_probe::record(
                        ui.ctx(),
                        format!("note-embed-error:{}", embed.target),
                        response.rect,
                    );
                    return;
                }
            };
            let key = (source.path.clone(), source.range.clone());
            if self.stack.contains(&key) {
                ui.weak(format!("Recursive embed: {}", embed.target));
                crate::ui_probe::record(ui.ctx(), "note-embed-cycle", ui.min_rect());
                return;
            }
            if self.stack.len() >= 8 {
                ui.weak("Embed nesting limit reached");
                return;
            }
            self.stack.push(key);
            let mut rendered = source.text.clone();
            let follow = crate::preview::show_embedded(
                ui,
                cache,
                &mut rendered,
                &source.path,
                embed.id,
                self,
            );
            self.stack.pop();
            if let Some(target) = follow {
                self.actions.push(crate::page_preview::Action::Open {
                    from: source.path.clone(),
                    target,
                });
            }
            if rendered != source.text && !source.path.is_absolute() {
                let outcome = (|| {
                    let full = text(
                        self.vault,
                        &self.current.path,
                        &self.current.text,
                        self.open,
                        &source.path,
                    )
                    .ok_or("The embedded note is no longer available.")?;
                    if full.get(source.range.clone()) != Some(source.text.as_str()) {
                        return Err("The embedded note changed. Try the edit again.".to_owned());
                    }
                    let mut before = self.vault.read(&source.path)?;
                    let is_open = source.path == self.current.path
                        || self.open.iter().any(|(path, _)| *path == source.path);
                    if !is_open && before.text != full {
                        return Err(
                            "The embedded note changed on disk. Reload it before editing.".into(),
                        );
                    }
                    before.text = full.into();
                    let mut after = before.clone();
                    after.text.replace_range(source.range.clone(), &rendered);
                    Ok(crate::page_preview::Action::Edit { before, after })
                })();
                match outcome {
                    Ok(action) => self.actions.push(action),
                    Err(error) => *self.error = Some(error),
                }
            }
        });
        crate::ui_probe::record(
            ui.ctx(),
            format!("note-embed:{}", embed.target),
            frame.response.rect,
        );
        // Open and Edit sit in the top-right corner and show on hover, as in Obsidian; no
        // title row above the embedded text.
        let rect = frame.response.rect;
        let hovered = ui.rect_contains_pointer(rect);
        let corner = |slot: f32| {
            egui::Rect::from_min_size(
                egui::pos2(rect.right() - 26.0 * (slot + 1.0), rect.top() + 4.0),
                egui::vec2(22.0, 22.0),
            )
        };
        let mut corner_button = |slot: f32, name: &str, icon, hint: String| {
            let area = corner(slot);
            let response = ui
                .interact(area, embed.id.with(name), egui::Sense::click())
                .on_hover_text(hint);
            crate::ui_probe::record(ui.ctx(), format!("{name}:{}", embed.target), area);
            if hovered || response.hovered() {
                let color = if response.hovered() {
                    ui.visuals().strong_text_color()
                } else {
                    ui.visuals().weak_text_color()
                };
                crate::theme::icon_image(icon, color).paint_at(ui, area.shrink(3.0));
            }
            response.clicked()
        };
        if corner_button(
            0.0,
            "note-embed-open",
            crate::theme::Icon::OpenLink,
            format!("Open {}", embed.target),
        ) {
            self.actions.push(crate::page_preview::Action::Open {
                from: from.into(),
                target: embed.target.clone(),
            });
        }
        if edit_source
            && corner_button(
                1.0,
                "note-embed-edit",
                crate::theme::Icon::Edit,
                "Edit embed".into(),
            )
        {
            edit = true;
        }
        if edit_source {
            let measured = frame.response.rect.height();
            if (height(ui.ctx(), embed.id, width) - measured).abs() > 1.0 {
                ui.ctx().data_mut(|data| {
                    data.insert_temp(
                        embed.id.with("height"),
                        Height {
                            width,
                            height: measured,
                        },
                    )
                });
                ui.ctx().request_repaint();
            }
        }
        edit
    }
}
