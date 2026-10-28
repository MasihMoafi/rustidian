//! The Agent tab: a coding agent's own terminal interface, live, in the vault.
use crate::{theme, ui_probe};
use eframe::egui::{
    self, Align, Align2, Color32, Event, FontId, Key, Layout, Modifiers, MouseWheelUnit, Pos2,
    Rect, RichText, Sense, Stroke, StrokeKind, Vec2,
    emath::GuiRounding as _,
    text::{LayoutJob, TextFormat},
};
use portable_pty::ExitStatus;
use rustidian::terminal::{self, Backend, Session};
use serde::{Deserialize, Serialize};
use std::{
    io::Write,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

/// The terminal's keyboard focus; while it has focus, keys go to the agent.
pub fn focus_id() -> egui::Id {
    egui::Id::unique("agent-terminal")
}

/// The vault's agent choice, in `$XDG_STATE_HOME/rustidian/agent/<vault>.json`.
#[derive(Default, Serialize, Deserialize)]
struct Saved {
    #[serde(default)]
    backend: Option<Backend>,
}

/// Cells chosen with the mouse, as screen (row, column), from where the drag
/// began to where it is now.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Selection {
    anchor: (u16, u16),
    end: (u16, u16),
}

impl Selection {
    /// The first and last cell in reading order.
    fn ordered(self) -> ((u16, u16), (u16, u16)) {
        if self.anchor <= self.end {
            (self.anchor, self.end)
        } else {
            (self.end, self.anchor)
        }
    }

    fn contains(self, row: u16, col: u16) -> bool {
        let (first, last) = self.ordered();
        first <= (row, col) && (row, col) <= last
    }

    /// The selected text, lines joined as the screen wrapped them.
    fn text(self, screen: &vt100::Screen) -> String {
        let (first, last) = self.ordered();
        screen.contents_between(first.0, first.1, last.0, last.1 + 1)
    }
}

#[derive(Default)]
pub struct Panel {
    root: Option<PathBuf>,
    chosen: Option<Backend>,
    installed: Vec<(Backend, PathBuf)>,
    session: Option<Session>,
    error: Option<String>,
    /// Start at the next show: the first one, Restart or another agent.
    start: bool,
    focus: bool,
    keys: Keys,
    /// Ctrl+Shift+V: ask the window for the clipboard at the next show.
    paste: bool,
    /// Text to put on the clipboard at the next show.
    copy: Option<String>,
    wheel: f32,
    selection: Option<Selection>,
    /// Re-armed by every draw: output asks for one frame, none while the tab is hidden.
    awake: Arc<AtomicBool>,
}

impl Panel {
    /// Give the terminal keyboard focus at the next show.
    pub fn focus(&mut self) {
        self.focus = true;
    }

    fn open(&mut self, root: &Path) {
        if self.root.as_deref() == Some(root) {
            return;
        }
        // Replacing the panel ends the previous vault's agent.
        *self = Self {
            root: Some(root.to_owned()),
            chosen: load(root),
            start: true,
            focus: self.focus,
            ..Default::default()
        };
    }

    /// The agent to run: the vault's choice when installed, else the first installed.
    fn backend(&self) -> Option<&(Backend, PathBuf)> {
        self.installed
            .iter()
            .find(|(backend, _)| Some(*backend) == self.chosen)
            .or_else(|| self.installed.first())
    }

    /// Send the window's keyboard input to the agent while the terminal has
    /// focus; Ctrl+Shift+A stays with the app. This runs before egui reads the
    /// input, so Tab, arrows and Escape do not move focus.
    pub fn raw_input(&mut self, ctx: &egui::Context, raw: &mut egui::RawInput) {
        let Self {
            session,
            keys,
            paste,
            copy,
            selection,
            ..
        } = self;
        let Some(session) = session.as_mut() else {
            return;
        };
        if session.exit().is_some() || !ctx.memory(|memory| memory.has_focus(focus_id())) {
            return;
        }
        let (application_cursor, bracketed) = {
            let parser = session.lock();
            let screen = parser.screen();
            (screen.application_cursor(), screen.bracketed_paste())
        };
        let mut bytes = Vec::new();
        raw.events.retain(|event| {
            match route(
                event,
                keys,
                application_cursor,
                bracketed,
                selection.is_some(),
            ) {
                Routed::Keep => return true,
                Routed::Send(sent) => bytes.extend(sent),
                Routed::RequestPaste => *paste = true,
                Routed::CopySelection => {
                    *copy = selection
                        .take()
                        .map(|chosen| chosen.text(session.lock().screen()));
                }
            }
            false
        });
        if !bytes.is_empty() {
            // Typing brings the view back to the bottom and ends a selection.
            session.lock().screen_mut().set_scrollback(0);
            *selection = None;
            session.write(bytes);
        }
    }

    /// The tab's content. `font` is the editor's code font.
    pub fn show(&mut self, ui: &mut egui::Ui, root: &Path, font: &FontId) {
        self.open(root);
        // Output asks for at most one frame per draw, and none while the tab is hidden.
        self.awake.store(true, Ordering::Release);
        theme::interface_style(ui);
        let starting = std::mem::take(&mut self.start);
        if starting {
            self.installed = terminal::discover(&std::env::var_os("PATH").unwrap_or_default());
            self.error = None;
        }
        let current = self.backend().map(|(backend, _)| *backend);
        let status = if self.installed.is_empty() {
            "No agent".to_owned()
        } else if self.error.is_some() {
            "Failed".to_owned()
        } else if let Some(exit) = self.session.as_mut().and_then(Session::exit) {
            describe(exit)
        } else if self.session.is_some() {
            "Running".to_owned()
        } else {
            "Starting".to_owned()
        };
        let mut picked = current;
        let mut restart = false;
        let header = ui.horizontal(|ui| {
            ui.set_min_height(28.0);
            ui.add_enabled_ui(!self.installed.is_empty(), |ui| {
                let combo = egui::ComboBox::from_id_salt("agent-backend")
                    .icon(theme::combo_chevron)
                    .selected_text(current.map_or("No agent", Backend::label))
                    .show_ui(ui, |ui| {
                        theme::menu_style(ui);
                        for (backend, _) in &self.installed {
                            let item =
                                ui.selectable_value(&mut picked, Some(*backend), backend.label());
                            ui_probe::record(
                                ui.ctx(),
                                format!("agent-backend:{}", backend.label()),
                                item.rect,
                            );
                        }
                    });
                ui_probe::record(ui.ctx(), "agent-backend", combo.response.rect);
            });
            let button = theme::glyph_button(ui, "↻", "Restart");
            ui_probe::record(ui.ctx(), "agent-restart", button.rect);
            restart = button.clicked();
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let label =
                    ui.add(egui::Label::new(RichText::new(&status).small().weak()).truncate());
                ui_probe::record(ui.ctx(), format!("agent-status:{status}"), label.rect);
            });
        });
        ui_probe::record(ui.ctx(), "agent-header", header.response.rect);
        if picked != current
            && let Some(backend) = picked
        {
            self.chosen = Some(backend);
            if let Err(error) = save(root, backend) {
                eprintln!("Cannot save the agent choice: {error}");
            }
            restart = true;
        }
        if restart {
            // The old agent ends off this thread; the new one starts with the next frame.
            self.session = None;
            self.selection = None;
            self.start = true;
            self.focus = true;
            ui.ctx().request_repaint();
        }
        ui.add_space(4.0);
        let cell =
            ui.fonts_mut(|fonts| egui::vec2(fonts.glyph_width(font, 'M'), fonts.row_height(font)));
        let area = ui.available_rect_before_wrap();
        let size = (
            (area.height() / cell.y).floor().clamp(1.0, 500.0) as u16,
            (area.width() / cell.x).floor().clamp(2.0, 1000.0) as u16,
        );
        if starting && let Some((backend, program)) = self.backend().cloned() {
            let (ctx, awake) = (ui.ctx().clone(), self.awake.clone());
            let repaint = move || {
                if awake.swap(false, Ordering::AcqRel) {
                    ctx.request_repaint();
                }
            };
            match Session::start(&program, root, size, repaint) {
                Ok(session) => self.session = Some(session),
                Err(error) => {
                    self.error = Some(format!("Couldn’t start {}: {error}", backend.label()));
                }
            }
        }
        if let Some(session) = &mut self.session
            && session.exit().is_none()
        {
            session.resize(size);
        }
        ui.allocate_rect(area, Sense::hover());
        let response = ui.interact(area, focus_id(), Sense::click_and_drag());
        if response.clicked() || response.drag_started() || std::mem::take(&mut self.focus) {
            response.request_focus();
        }
        ui_probe::record(ui.ctx(), "agent-terminal", area);
        if std::mem::take(&mut self.paste) {
            ui.ctx()
                .send_viewport_cmd(egui::ViewportCommand::RequestPaste);
            ui.ctx().request_repaint();
        }
        if let Some(text) = self.copy.take() {
            ui.ctx().copy_text(text);
        }
        let painter = ui.painter_at(area);
        let colors = Colors::new(ui.visuals());
        match &self.session {
            Some(session) => {
                let mut parser = session.lock();
                parser.callbacks_mut().colors = [colors.foreground, colors.background].map(rgb);
                let screen_size = parser.screen().size();
                // A drag selects text, unless the agent asked for mouse reports;
                // Shift selects anyway.
                let reporting =
                    parser.screen().mouse_protocol_mode() != vt100::MouseProtocolMode::None;
                if let Some(pointer) = response.interact_pointer_pos() {
                    let at = |point| cell_at(point, area, cell, screen_size);
                    if response.drag_started()
                        && (!reporting || ui.input(|input| input.modifiers.shift))
                    {
                        // egui starts a drag once the pointer has moved: anchor where it was pressed.
                        let origin = ui.input(|input| input.pointer.press_origin());
                        self.selection = Some(Selection {
                            anchor: at(origin.unwrap_or(pointer)),
                            end: at(pointer),
                        });
                    } else if (response.dragged() || response.drag_stopped())
                        && let Some(selection) = &mut self.selection
                    {
                        selection.end = at(pointer);
                    }
                }
                if response.clicked() {
                    self.selection = None;
                }
                if response.hovered()
                    && scroll(ui, &mut self.wheel, &mut parser, area, cell, session)
                {
                    self.selection = None;
                }
                let screen = parser.screen();
                paint(
                    &painter,
                    area.min,
                    cell,
                    font,
                    screen,
                    &colors,
                    response.has_focus(),
                );
                if let Some(selection) = self.selection {
                    let (first, last) = selection.ordered();
                    for row in first.0..=last.0 {
                        let start = if row == first.0 { first.1 } else { 0 };
                        let end = if row == last.0 {
                            last.1
                        } else {
                            screen_size.1.saturating_sub(1)
                        };
                        let rect = Rect::from_min_size(
                            area.min
                                + egui::vec2(f32::from(start) * cell.x, f32::from(row) * cell.y),
                            egui::vec2(f32::from(end.saturating_sub(start) + 1) * cell.x, cell.y),
                        );
                        painter.rect_filled(rect, 0, ui.visuals().selection.bg_fill);
                    }
                }
                if ui_probe::enabled() {
                    for (row, text) in screen.rows(0, screen_size.1).enumerate() {
                        let text = text.trim_end();
                        if !text.is_empty() {
                            let top = area.top() + row as f32 * cell.y;
                            ui_probe::record(
                                ui.ctx(),
                                format!("terminal-row:{row:02}:{text}"),
                                Rect::from_min_max(
                                    egui::pos2(area.left(), top),
                                    egui::pos2(area.right(), top + cell.y),
                                ),
                            );
                        }
                    }
                    ui_probe::metric(ui.ctx(), "terminal-scrollback", screen.scrollback() as f32);
                    ui_probe::metric(ui.ctx(), "terminal-pid", session.pid() as f32);
                    ui_probe::metric(
                        ui.ctx(),
                        "terminal-selected",
                        f32::from(u8::from(self.selection.is_some())),
                    );
                }
            }
            None => {
                let message = match (&self.error, current) {
                    (Some(error), _) => error.clone(),
                    (None, None) => {
                        "No agent CLI on PATH: install elpis, codex or claude.".to_owned()
                    }
                    (None, Some(_)) => String::new(),
                };
                let color = ui.visuals().weak_text_color();
                let galley = painter.layout(
                    message,
                    egui::TextStyle::Body.resolve(ui.style()),
                    color,
                    area.width(),
                );
                painter.galley(area.left_top(), galley, color);
            }
        }
        ui_probe::metric(ui.ctx(), "terminal-rows", f32::from(size.0));
        ui_probe::metric(ui.ctx(), "terminal-cols", f32::from(size.1));
        ui_probe::metric(
            ui.ctx(),
            "terminal-focused",
            f32::from(u8::from(response.has_focus())),
        );
    }
}

/// "Exited (code N)", or the signal that ended the agent.
fn describe(status: &ExitStatus) -> String {
    match status.signal() {
        Some(signal) => format!("Exited ({signal})"),
        None => format!("Exited (code {})", status.exit_code()),
    }
}

/// The screen cell under a point of the terminal, clamped to the screen.
fn cell_at(point: Pos2, area: Rect, cell: Vec2, (rows, cols): (u16, u16)) -> (u16, u16) {
    let offset = point - area.min;
    let clamp = |value: f32, count: u16| {
        value.floor().clamp(0.0, f32::from(count.saturating_sub(1))) as u16
    };
    (
        clamp(offset.y / cell.y, rows),
        clamp(offset.x / cell.x, cols),
    )
}

/// The mouse wheel scrolls back through the output, or goes to an agent that
/// asked for mouse reports or, in its alternate screen, for arrow keys.
/// `wheel` keeps the part of a line not yet scrolled. True when it scrolled.
fn scroll(
    ui: &egui::Ui,
    wheel: &mut f32,
    parser: &mut terminal::Parser,
    area: Rect,
    cell: Vec2,
    session: &Session,
) -> bool {
    let size = parser.screen().size();
    *wheel += ui.input(|input| {
        input
            .events
            .iter()
            .map(|event| match event {
                Event::MouseWheel { unit, delta, .. } => match unit {
                    MouseWheelUnit::Point => delta.y / cell.y,
                    MouseWheelUnit::Line => delta.y * 3.0,
                    MouseWheelUnit::Page => delta.y * f32::from(size.0),
                },
                _ => 0.0,
            })
            .sum::<f32>()
    });
    let steps = wheel.trunc();
    *wheel -= steps;
    if steps == 0.0 {
        return false;
    }
    let alternate_scroll = parser.callbacks().alternate_scroll;
    let screen = parser.screen_mut();
    let count = steps.abs() as usize;
    if screen.mouse_protocol_mode() != vt100::MouseProtocolMode::None {
        let pointer = ui
            .input(|input| input.pointer.hover_pos())
            .unwrap_or(area.min);
        let (row, column) = cell_at(pointer, area, cell, size);
        let (row, column) = (row + 1, column + 1);
        let button = if steps > 0.0 { 64 } else { 65 };
        let report = if screen.mouse_protocol_encoding() == vt100::MouseProtocolEncoding::Sgr {
            format!("\x1b[<{button};{column};{row}M").into_bytes()
        } else {
            let [column, row] = [column, row].map(|value| (32 + value.min(223)) as u8);
            vec![0x1b, b'[', b'M', 32 + button, column, row]
        };
        session.write(report.repeat(count));
    } else if screen.alternate_screen() && alternate_scroll {
        let key = if steps > 0.0 {
            Key::ArrowUp
        } else {
            Key::ArrowDown
        };
        let arrow =
            key_bytes(key, Modifiers::NONE, screen.application_cursor()).unwrap_or_default();
        session.write(arrow.repeat(count));
    } else {
        let offset = screen.scrollback().saturating_add_signed(steps as isize);
        screen.set_scrollback(offset);
    }
    true
}

/// What a keyboard event does while the terminal has focus.
#[derive(Debug, PartialEq)]
enum Routed {
    /// The app keeps it.
    Keep,
    Send(Vec<u8>),
    RequestPaste,
    CopySelection,
}

fn route(
    event: &Event,
    keys: &mut Keys,
    application_cursor: bool,
    bracketed: bool,
    selecting: bool,
) -> Routed {
    let shortcut = |key: Key, modifiers: &Modifiers| {
        matches!(event, Event::Key { key: which, pressed: true, .. } if *which == key)
            && modifiers.command
            && modifiers.shift
    };
    match event {
        // Ctrl+Shift+A hides the tab. Releases stay too: egui tracks held keys.
        Event::Key {
            key: Key::A,
            modifiers,
            ..
        } if modifiers.command && modifiers.shift => Routed::Keep,
        Event::Key { pressed: false, .. } => Routed::Keep,
        Event::Key { modifiers, .. } if shortcut(Key::V, modifiers) => Routed::RequestPaste,
        Event::Key { modifiers, .. } if shortcut(Key::C, modifiers) => {
            if selecting {
                Routed::CopySelection
            } else {
                Routed::Send(Vec::new())
            }
        }
        // egui turns Ctrl+C into a copy: it copies a selection, else interrupts.
        Event::Copy if selecting => Routed::CopySelection,
        // Ctrl+V with text pastes it; with an image the agent reads the clipboard itself.
        Event::Paste(text) => Routed::Send(pasted(text, bracketed)),
        Event::PasteImage(_) => Routed::Send(vec![0x16]),
        Event::Key { .. } | Event::Text(_) | Event::Copy | Event::Cut | Event::Ime(_) => {
            Routed::Send(keys.encode(event, application_cursor))
        }
        _ => Routed::Keep,
    }
}

/// Pasted text as a terminal sends it: lines end in CR, escape bytes are
/// dropped so a paste cannot end bracketed mode, bracketed when the agent asks.
fn pasted(text: &str, bracketed: bool) -> Vec<u8> {
    let body = text
        .replace("\r\n", "\r")
        .replace('\n', "\r")
        .replace('\x1b', "");
    if bracketed {
        format!("\x1b[200~{body}\x1b[201~").into_bytes()
    } else {
        body.into_bytes()
    }
}

/// Turns egui's keyboard events into what xterm sends.
#[derive(Default)]
struct Keys {
    /// Alt was held with a key whose text follows as the next event.
    alt: bool,
}

impl Keys {
    fn encode(&mut self, event: &Event, application_cursor: bool) -> Vec<u8> {
        match event {
            Event::Text(text) | Event::Ime(egui::ImeEvent::Commit(text)) => {
                let mut bytes = if std::mem::take(&mut self.alt) {
                    vec![0x1b]
                } else {
                    Vec::new()
                };
                bytes.extend_from_slice(text.as_bytes());
                bytes
            }
            // egui turns Ctrl+C and Ctrl+X into these.
            Event::Copy => {
                self.alt = false;
                vec![0x03]
            }
            Event::Cut => {
                self.alt = false;
                vec![0x18]
            }
            Event::Key {
                key,
                pressed: true,
                modifiers,
                ..
            } => {
                self.alt = false;
                if let Some(bytes) = key_bytes(*key, *modifiers, application_cursor) {
                    return bytes;
                }
                self.alt = modifiers.alt && !modifiers.ctrl;
                Vec::new()
            }
            _ => Vec::new(),
        }
    }
}

/// xterm's sequence for a special key, or the control byte for Ctrl+key.
fn key_bytes(key: Key, modifiers: Modifiers, application_cursor: bool) -> Option<Vec<u8>> {
    let parameter =
        1 + u8::from(modifiers.shift) + 2 * u8::from(modifiers.alt) + 4 * u8::from(modifiers.ctrl);
    let cursor = |end: char| {
        if parameter > 1 {
            format!("\x1b[1;{parameter}{end}")
        } else if application_cursor {
            format!("\x1bO{end}")
        } else {
            format!("\x1b[{end}")
        }
    };
    let function = |end: char| {
        if parameter > 1 {
            format!("\x1b[1;{parameter}{end}")
        } else {
            format!("\x1bO{end}")
        }
    };
    let tilde = |number: u8| {
        if parameter > 1 {
            format!("\x1b[{number};{parameter}~")
        } else {
            format!("\x1b[{number}~")
        }
    };
    let alt = |text: &str| {
        if modifiers.alt {
            format!("\x1b{text}")
        } else {
            text.to_owned()
        }
    };
    let sequence = match key {
        Key::ArrowUp => cursor('A'),
        Key::ArrowDown => cursor('B'),
        Key::ArrowRight => cursor('C'),
        Key::ArrowLeft => cursor('D'),
        Key::Home => cursor('H'),
        Key::End => cursor('F'),
        Key::Insert => tilde(2),
        Key::Delete => tilde(3),
        Key::PageUp => tilde(5),
        Key::PageDown => tilde(6),
        Key::F1 => function('P'),
        Key::F2 => function('Q'),
        Key::F3 => function('R'),
        Key::F4 => function('S'),
        Key::F5 => tilde(15),
        Key::F6 => tilde(17),
        Key::F7 => tilde(18),
        Key::F8 => tilde(19),
        Key::F9 => tilde(20),
        Key::F10 => tilde(21),
        Key::F11 => tilde(23),
        Key::F12 => tilde(24),
        Key::Enter => alt("\r"),
        Key::Tab if modifiers.shift => "\x1b[Z".to_owned(),
        Key::Tab => alt("\t"),
        Key::Backspace if modifiers.ctrl => alt("\x08"),
        Key::Backspace => alt("\x7f"),
        Key::Escape => alt("\x1b"),
        // Ctrl+Shift+letter belongs to the terminal itself, as in GNOME Terminal.
        _ if modifiers.ctrl && !modifiers.shift => alt(&char::from(control(key)?).to_string()),
        _ => return None,
    };
    Some(sequence.into_bytes())
}

/// The control byte Ctrl+key types: Ctrl+A is 1, Ctrl+[ is Escape.
fn control(key: Key) -> Option<u8> {
    match key.name().as_bytes() {
        [letter @ b'A'..=b'Z'] => Some(letter - b'@'),
        _ => match key {
            Key::Space => Some(0),
            Key::OpenBracket => Some(0x1b),
            Key::Backslash => Some(0x1c),
            Key::CloseBracket => Some(0x1d),
            Key::Slash => Some(0x1f),
            _ => None,
        },
    }
}

fn load(root: &Path) -> Option<Backend> {
    let path = rustidian::paths::state_file(root, "agent", "json")?;
    serde_json::from_slice::<Saved>(&std::fs::read(path).ok()?)
        .ok()?
        .backend
}

fn save(root: &Path, backend: Backend) -> Result<(), String> {
    let path = rustidian::paths::state_file(root, "agent", "json")
        .ok_or("Cannot find the state folder.")?;
    let parent = path.parent().ok_or("Cannot find the state folder.")?;
    std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    let bytes = serde_json::to_vec(&Saved {
        backend: Some(backend),
    })
    .map_err(|error| error.to_string())?;
    let mut file = tempfile::NamedTempFile::new_in(parent).map_err(|error| error.to_string())?;
    file.write_all(&bytes).map_err(|error| error.to_string())?;
    file.persist(path).map_err(|error| error.to_string())?;
    Ok(())
}

/// The terminal's colours: the theme's text on the panel, and one fixed set of
/// the 16 ANSI colours per theme that stays legible on it.
struct Colors {
    foreground: Color32,
    background: Color32,
    ansi: [Color32; 16],
}

const DARK: [u32; 16] = [
    0x767e83, 0xef6b73, 0x8cc265, 0xe5c07b, 0x61afef, 0xc678dd, 0x56b6c2, 0xc8ccd0, 0x8f989d,
    0xff8a92, 0xa9dc84, 0xffd98c, 0x8cc8ff, 0xdc9cf0, 0x7fd6e0, 0xf4f4f4,
];
const LIGHT: [u32; 16] = [
    0x222a2b, 0xb3262e, 0x2c6a12, 0x755800, 0x1f5ac0, 0x8e2a8e, 0x0b6871, 0x56605f, 0x46504f,
    0xc4343c, 0x357a18, 0x856600, 0x2b67d0, 0xa0369e, 0x0f7680, 0x2e3738,
];

impl Colors {
    fn new(visuals: &egui::Visuals) -> Self {
        let ansi = if visuals.dark_mode { DARK } else { LIGHT };
        Self {
            foreground: visuals.text_color(),
            background: visuals.panel_fill,
            ansi: ansi.map(|value| {
                let [_, red, green, blue] = value.to_be_bytes();
                Color32::from_rgb(red, green, blue)
            }),
        }
    }

    fn resolve(&self, color: vt100::Color, default: Color32) -> Color32 {
        match color {
            vt100::Color::Default => default,
            vt100::Color::Rgb(red, green, blue) => Color32::from_rgb(red, green, blue),
            vt100::Color::Idx(index @ 0..=15) => self.ansi[usize::from(index)],
            // The 6×6×6 colour cube, then 24 greys.
            vt100::Color::Idx(index @ 16..=231) => {
                let level = |value: u8| if value == 0 { 0 } else { 55 + value * 40 };
                let index = index - 16;
                Color32::from_rgb(level(index / 36), level(index / 6 % 6), level(index % 6))
            }
            vt100::Color::Idx(index) => Color32::from_gray(8 + (index - 232) * 10),
        }
    }
}

/// WCAG's contrast ratio, from 1 (none) to 21 (black on white).
fn contrast(first: Color32, second: Color32) -> f32 {
    let luminance = |color: Color32| {
        let channel = |value: u8| {
            let value = f32::from(value) / 255.0;
            if value <= 0.039_28 {
                value / 12.92
            } else {
                ((value + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * channel(color.r()) + 0.7152 * channel(color.g()) + 0.0722 * channel(color.b())
    };
    let (first, second) = (luminance(first), luminance(second));
    (first.max(second) + 0.05) / (first.min(second) + 0.05)
}

fn rgb(color: Color32) -> [u8; 3] {
    [color.r(), color.g(), color.b()]
}

#[derive(PartialEq)]
struct Style {
    foreground: Color32,
    background: Color32,
    bold: bool,
    italic: bool,
    underline: bool,
}

impl Style {
    fn of(cell: &vt100::Cell, colors: &Colors) -> Self {
        let mut foreground = colors.resolve(cell.fgcolor(), colors.foreground);
        let mut background = colors.resolve(cell.bgcolor(), colors.background);
        if cell.inverse() {
            std::mem::swap(&mut foreground, &mut background);
        }
        // One palette cannot suit every pair (black on white is dark grey on
        // grey in the light theme): on a coloured cell, text that would be
        // hard to read takes the theme's text or panel colour instead.
        if background != colors.background && contrast(foreground, background) < 3.0 {
            foreground = if contrast(colors.foreground, background)
                >= contrast(colors.background, background)
            {
                colors.foreground
            } else {
                colors.background
            };
        }
        if cell.dim() {
            foreground = foreground.gamma_multiply(0.6);
        }
        Self {
            foreground,
            background,
            bold: cell.bold(),
            italic: cell.italic(),
            underline: cell.underline(),
        }
    }
}

/// Strong right-to-left letters: Hebrew and Arabic, Persian included.
fn right_to_left(text: &str) -> bool {
    text.chars().next().is_some_and(|letter| {
        matches!(letter, '\u{0590}'..='\u{08FF}' | '\u{FB1D}'..='\u{FDFF}' | '\u{FE70}'..='\u{FEFF}')
    })
}

/// The cells from `col` drawn as one text, in `style`: right-to-left words
/// (shaped and ordered together), a stretch of plain ASCII cells, or one other
/// character on its own cell so the grid stays aligned. Returns the column
/// after the run, its text and whether it runs right to left.
fn run(
    screen: &vt100::Screen,
    row: u16,
    col: u16,
    style: &Style,
    colors: &Colors,
) -> (u16, String, bool) {
    let contents = |cell: &vt100::Cell| {
        if cell.has_contents() {
            cell.contents().to_owned()
        } else {
            " ".to_owned()
        }
    };
    let alike = |cell: &vt100::Cell| !cell.is_wide() && Style::of(cell, colors) == *style;
    let Some(first) = screen.cell(row, col) else {
        return (col + 1, String::new(), false);
    };
    if right_to_left(first.contents()) {
        // Spaces between right-to-left words stay in the run; trailing ones do not.
        let (mut end, mut last) = (col + 1, col);
        while let Some(next) = screen.cell(row, end).filter(|next| alike(next)) {
            if right_to_left(next.contents()) {
                last = end;
            } else if next.has_contents() && next.contents() != " " {
                break;
            }
            end += 1;
        }
        let text = (col..=last)
            .filter_map(|at| screen.cell(row, at).map(contents))
            .collect();
        return (last + 1, text, true);
    }
    if first.is_wide() || !first.contents().is_ascii() {
        // Claude's tool-output corner is missing from the fonts; └ looks the same.
        let text = contents(first).replace('\u{23BF}', "\u{2514}");
        return (col + 1 + u16::from(first.is_wide()), text, false);
    }
    let mut end = col;
    let mut text = String::new();
    while let Some(next) = screen
        .cell(row, end)
        .filter(|next| alike(next) && next.contents().is_ascii())
    {
        text.push_str(&contents(next));
        end += 1;
    }
    (end.max(col + 1), text, false)
}

/// The dots, as (column, row) of a 2×4 grid, of a braille pattern: most
/// agents' spinners, and no bundled font has them.
fn braille(text: &str) -> Option<Vec<(f32, f32)>> {
    let mut letters = text.chars();
    let (Some(letter), None) = (letters.next(), letters.next()) else {
        return None;
    };
    let bits = u32::from(letter)
        .checked_sub(0x2800)
        .filter(|bits| *bits < 0x100)?;
    // Dots 1-3 and 7 run down the left column, 4-6 and 8 down the right.
    const DOTS: [(f32, f32); 8] = [
        (0.0, 0.0),
        (0.0, 1.0),
        (0.0, 2.0),
        (1.0, 0.0),
        (1.0, 1.0),
        (1.0, 2.0),
        (0.0, 3.0),
        (1.0, 3.0),
    ];
    Some(
        DOTS.into_iter()
            .enumerate()
            .filter(|(bit, _)| bits & (1 << bit) != 0)
            .map(|(_, dot)| dot)
            .collect(),
    )
}

/// Draw the visible rows and the cursor.
fn paint(
    painter: &egui::Painter,
    origin: egui::Pos2,
    cell: Vec2,
    font: &FontId,
    screen: &vt100::Screen,
    colors: &Colors,
    focused: bool,
) {
    let pixels = painter.pixels_per_point();
    let (rows, cols) = screen.size();
    let at = |row: u16, col: u16, width: u16| {
        Rect::from_min_size(
            origin + egui::vec2(f32::from(col) * cell.x, f32::from(row) * cell.y),
            egui::vec2(f32::from(width) * cell.x, cell.y),
        )
    };
    for row in 0..rows {
        let mut col = 0;
        while col < cols {
            let Some(first) = screen.cell(row, col) else {
                break;
            };
            if first.is_wide_continuation() {
                col += 1;
                continue;
            }
            let style = Style::of(first, colors);
            let (end, text, right_to_left) = run(screen, row, col, &style, colors);
            let rect = at(row, col, end - col);
            if style.background != colors.background {
                painter.rect_filled(rect.round_to_pixels(pixels), 0, style.background);
            }
            if let Some(dots) = braille(&text) {
                for (column, row) in dots {
                    let center = rect.min
                        + egui::vec2(
                            rect.width() * (0.3 + 0.4 * column),
                            rect.height() * (0.2 + 0.2 * row),
                        );
                    painter.circle_filled(center, (rect.width() * 0.13).max(0.8), style.foreground);
                }
            } else if !text.trim().is_empty() || style.underline {
                let galley = painter.layout_job(LayoutJob::single_section(
                    text,
                    TextFormat {
                        font_id: font.clone(),
                        color: style.foreground,
                        italics: style.italic,
                        underline: if style.underline {
                            Stroke::new(1.0, style.foreground)
                        } else {
                            Stroke::NONE
                        },
                        ..Default::default()
                    },
                ));
                // Right-to-left words keep to their cells, from the right.
                let (target, position) = if right_to_left {
                    (
                        painter.with_clip_rect(rect),
                        egui::pos2(rect.right() - galley.size().x, rect.top()),
                    )
                } else {
                    (painter.clone(), rect.min)
                };
                if style.bold {
                    // No bold monospace face ships with the app: overstrike by a pixel.
                    target.galley(
                        position + egui::vec2(1.0 / pixels, 0.0),
                        galley.clone(),
                        style.foreground,
                    );
                }
                target.galley(position, galley, style.foreground);
            }
            col = end;
        }
    }
    if screen.scrollback() > 0 || screen.hide_cursor() {
        return;
    }
    let (row, col) = screen.cursor_position();
    if row >= rows || col >= cols {
        return;
    }
    let rect = at(row, col, 1).round_to_pixels(pixels);
    if focused {
        painter.rect_filled(rect, 0, colors.foreground);
        if let Some(under) = screen.cell(row, col).filter(|under| under.has_contents()) {
            painter.text(
                rect.min,
                Align2::LEFT_TOP,
                under.contents(),
                font.clone(),
                colors.background,
            );
        }
    } else {
        painter.rect_stroke(
            rect,
            0,
            Stroke::new(1.0, colors.foreground),
            StrokeKind::Inside,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(key: Key, modifiers: Modifiers) -> Event {
        Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        }
    }

    fn typed(events: &[Event], application_cursor: bool) -> Vec<u8> {
        let mut keys = Keys::default();
        events
            .iter()
            .flat_map(|event| keys.encode(event, application_cursor))
            .collect()
    }

    #[test]
    fn keys_reach_the_agent_as_xterm_sends_them() {
        let none = Modifiers::NONE;
        let text = |value: &str| Event::Text(value.to_owned());
        // egui sends a key event, then the text it typed.
        assert_eq!(
            typed(
                &[key(Key::H, none), text("h"), key(Key::I, none), text("i")],
                false
            ),
            b"hi"
        );
        assert_eq!(typed(&[key(Key::Enter, none)], false), b"\r");
        assert_eq!(typed(&[key(Key::Escape, none)], false), b"\x1b");
        assert_eq!(typed(&[key(Key::Backspace, none)], false), b"\x7f");
        assert_eq!(typed(&[key(Key::Tab, Modifiers::SHIFT)], false), b"\x1b[Z");
        for (arrow, end) in [
            (Key::ArrowUp, 'A'),
            (Key::ArrowDown, 'B'),
            (Key::ArrowRight, 'C'),
            (Key::ArrowLeft, 'D'),
        ] {
            assert_eq!(
                typed(&[key(arrow, none)], false),
                format!("\x1b[{end}").as_bytes()
            );
            assert_eq!(
                typed(&[key(arrow, none)], true),
                format!("\x1bO{end}").as_bytes()
            );
        }
        assert_eq!(
            typed(&[key(Key::ArrowLeft, Modifiers::CTRL)], true),
            b"\x1b[1;5D"
        );
        assert_eq!(typed(&[key(Key::Delete, none)], false), b"\x1b[3~");
        assert_eq!(typed(&[key(Key::PageUp, none)], false), b"\x1b[5~");
        // egui turns Ctrl+C into a copy event.
        assert_eq!(typed(&[Event::Copy], false), b"\x03");
        assert_eq!(typed(&[key(Key::A, Modifiers::CTRL)], false), b"\x01");
        assert_eq!(typed(&[key(Key::D, Modifiers::CTRL)], false), b"\x04");
        assert_eq!(
            typed(&[key(Key::X, Modifiers::ALT), text("x")], false),
            b"\x1bx"
        );
        assert_eq!(
            typed(&[key(Key::B, Modifiers::ALT | Modifiers::CTRL)], false),
            b"\x1b\x02"
        );
        assert_eq!(typed(&[text("سلام")], false), "سلام".as_bytes());
        // Ctrl+Shift+letter is the terminal's own, as in GNOME Terminal.
        assert_eq!(
            typed(&[key(Key::C, Modifiers::CTRL | Modifiers::SHIFT)], false),
            b""
        );
    }

    #[test]
    fn the_terminal_takes_every_key_but_the_tab_toggle() {
        let mut keys = Keys::default();
        let mut routed = |event: Event, selecting| route(&event, &mut keys, false, true, selecting);
        // On Linux egui sets both for the Ctrl key.
        let command = Modifiers::CTRL | Modifiers::COMMAND;
        assert_eq!(
            routed(key(Key::A, command | Modifiers::SHIFT), false),
            Routed::Keep
        );
        // Ctrl+Q reaches the agent; it does not quit the app.
        assert_eq!(
            routed(key(Key::Q, command), false),
            Routed::Send(vec![0x11])
        );
        assert_eq!(
            routed(key(Key::W, command), false),
            Routed::Send(vec![0x17])
        );
        let release = Event::Key {
            key: Key::W,
            physical_key: None,
            pressed: false,
            repeat: false,
            modifiers: command,
        };
        assert_eq!(routed(release, false), Routed::Keep);
        assert_eq!(routed(Event::PointerMoved(Pos2::ZERO), false), Routed::Keep);
        assert_eq!(
            routed(key(Key::V, command | Modifiers::SHIFT), false),
            Routed::RequestPaste
        );
        // Text on the clipboard is pasted, bracketed when the agent asks; an image is Ctrl+V.
        assert_eq!(
            routed(Event::Paste("a\nb".into()), false),
            Routed::Send(b"\x1b[200~a\rb\x1b[201~".to_vec())
        );
        let image = Arc::new(egui::ColorImage::filled([1, 1], Color32::BLACK));
        assert_eq!(
            routed(Event::PasteImage(image), false),
            Routed::Send(vec![0x16])
        );
        // Ctrl+C copies a selection, else interrupts; Ctrl+Shift+C only copies.
        assert_eq!(routed(Event::Copy, false), Routed::Send(vec![0x03]));
        assert_eq!(routed(Event::Copy, true), Routed::CopySelection);
        assert_eq!(
            routed(key(Key::C, command | Modifiers::SHIFT), true),
            Routed::CopySelection
        );
        assert_eq!(
            routed(key(Key::C, command | Modifiers::SHIFT), false),
            Routed::Send(Vec::new())
        );
    }

    #[test]
    fn pastes_end_lines_in_cr_and_cannot_close_bracketed_mode() {
        assert_eq!(pasted("one\ntwo\r\n", false), b"one\rtwo\r");
        assert_eq!(pasted("a\x1b[201~b", true), b"\x1b[200~a[201~b\x1b[201~");
    }

    #[test]
    fn a_selection_copies_its_cells_in_reading_order() {
        let mut parser = vt100::Parser::new(3, 10, 0);
        parser.process(b"first\r\nsecond\r\nthird");
        // Dragged backwards, from "ird" up into "cond".
        let selection = Selection {
            anchor: (2, 2),
            end: (1, 2),
        };
        assert_eq!(selection.text(parser.screen()), "cond\nthi");
        assert!(selection.contains(1, 5) && selection.contains(2, 2));
        assert!(!selection.contains(2, 3) && !selection.contains(1, 1));
    }

    #[test]
    fn right_to_left_words_are_drawn_as_one_text() {
        let mut parser = vt100::Parser::new(1, 20, 0);
        parser.process("ab سلام دنیا cd".as_bytes());
        let colors = Colors::new(&egui::Visuals::dark());
        let screen = parser.screen();
        let at = |col| {
            run(
                screen,
                0,
                col,
                &Style::of(screen.cell(0, col).unwrap(), &colors),
                &colors,
            )
        };
        assert_eq!(at(0), (3, "ab ".to_owned(), false));
        assert_eq!(at(3), (12, "سلام دنیا".to_owned(), true));
        let (end, text, right_to_left) = at(12);
        assert_eq!((end, text.trim_end(), right_to_left), (20, " cd", false));
    }

    #[test]
    fn braille_spinners_draw_as_dots() {
        // ⠋ is dots 1, 2 and 4.
        assert_eq!(braille("⠋"), Some(vec![(0.0, 0.0), (0.0, 1.0), (1.0, 0.0)]));
        assert_eq!(braille("⠀"), Some(Vec::new()));
        assert_eq!(braille("a"), None);
        assert_eq!(braille("⠋⠙"), None);
    }

    #[test]
    fn every_ansi_colour_stays_legible_on_the_panel_in_both_themes() {
        use crate::theme::palette::Palette;
        let ctx = egui::Context::default();
        crate::theme::apply_appearance(&ctx, crate::theme::Appearance::System);
        for theme in [egui::Theme::Light, egui::Theme::Dark] {
            let visuals = ctx.style_of(theme).visuals.clone();
            let colors = Colors::new(&visuals);
            let dark = theme == egui::Theme::Dark;
            let panels = [
                visuals.panel_fill,
                Palette::LoveSpace.side(dark),
                Palette::ElpisGold.side(dark),
                Palette::Signature.side(dark),
            ];
            for panel in panels {
                for (index, color) in colors.ansi.iter().enumerate() {
                    let ratio = contrast(*color, panel);
                    assert!(ratio >= 4.0, "colour {index} on {panel:?}: {ratio}");
                }
            }
            assert_eq!(colors.background, visuals.panel_fill);
            assert_eq!(colors.foreground, visuals.text_color());
        }
    }

    #[test]
    fn text_on_a_coloured_cell_stays_legible_in_both_themes() {
        let ctx = egui::Context::default();
        crate::theme::apply_appearance(&ctx, crate::theme::Appearance::System);
        let mut parser = vt100::Parser::new(1, 20, 0);
        parser.process(b"\x1b[30;47mB\x1b[0;47mW\x1b[0;31mR");
        let screen = parser.screen();
        for theme in [egui::Theme::Light, egui::Theme::Dark] {
            let colors = Colors::new(&ctx.style_of(theme).visuals);
            for col in [0, 1] {
                let style = Style::of(screen.cell(0, col).unwrap(), &colors);
                let ratio = contrast(style.foreground, style.background);
                assert!(ratio >= 3.0, "{theme:?} column {col}: {ratio}");
            }
            // Legible text keeps its own colour.
            let red = Style::of(screen.cell(0, 2).unwrap(), &colors);
            assert_eq!(red.foreground, colors.ansi[1]);
        }
    }

    #[test]
    fn indexed_colours_follow_xterms_cube_and_greys() {
        let colors = Colors::new(&egui::Visuals::dark());
        assert_eq!(
            colors.resolve(vt100::Color::Idx(196), Color32::BLACK),
            Color32::from_rgb(255, 0, 0)
        );
        assert_eq!(
            colors.resolve(vt100::Color::Idx(244), Color32::BLACK),
            Color32::from_gray(128)
        );
        assert_eq!(
            colors.resolve(vt100::Color::Default, Color32::BLUE),
            Color32::BLUE
        );
    }

    #[test]
    fn an_exit_names_its_code_or_signal() {
        assert_eq!(describe(&ExitStatus::with_exit_code(0)), "Exited (code 0)");
        assert_eq!(
            describe(&ExitStatus::with_signal("Killed")),
            "Exited (Killed)"
        );
    }

    #[test]
    fn a_vaults_agent_choice_survives_old_conversation_files() {
        let old = br#"{"thread_id":"t","messages":[],"access":"full","model":"gpt"}"#;
        assert_eq!(serde_json::from_slice::<Saved>(old).unwrap().backend, None);
        let saved = serde_json::to_vec(&Saved {
            backend: Some(Backend::Codex),
        })
        .unwrap();
        assert_eq!(saved, br#"{"backend":"codex"}"#);
        assert!(serde_json::from_slice::<Saved>(br#"{"backend":"gemini"}"#).is_err());
    }
}
