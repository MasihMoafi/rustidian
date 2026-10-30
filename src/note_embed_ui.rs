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

pub fn resolve(
    vault: &Vault,
    current_path: &Path,
    current_text: &str,
    open: &[(&Path, &str)],
    from: &Path,
    target: &str,
) -> Result<Source, String> {
    let target = target.split('|').next().unwrap_or(target);
    let path = vault
        .resolve(from, target)
        .ok_or_else(|| format!("Note not found: {target}"))?;
    let full = text(vault, current_path, current_text, open, &path)
        .ok_or_else(|| format!("Note not found: {target}"))?;
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
            ui.horizontal(|ui| {
                let title = ui.link(&embed.target);
                crate::ui_probe::record(
                    ui.ctx(),
                    format!("note-embed-open:{}", embed.target),
                    title.rect,
                );
                if title.clicked() {
                    self.actions.push(crate::page_preview::Action::Open {
                        from: from.into(),
                        target: embed.target.clone(),
                    });
                }
                if edit_source {
                    let response = ui.small_button("Edit embed");
                    crate::ui_probe::record(
                        ui.ctx(),
                        format!("note-embed-edit:{}", embed.target),
                        response.rect,
                    );
                    edit = response.clicked();
                }
            });
            ui.add_space(6.0);
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
            if rendered != source.text {
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
