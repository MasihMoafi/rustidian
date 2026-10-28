//! The Agent tab: a coding agent's own terminal interface, live, in the vault.
use crate::{theme, ui_probe};
use eframe::egui::{
    self, Align, Align2, Color32, Event, FontId, Key, Layout, Modifiers, MouseWheelUnit, Rect,
    RichText, Sense, Stroke, StrokeKind, Vec2,
    emath::GuiRounding as _,
    text::{LayoutJob, TextFormat},
};
use rustidian::terminal::{self, Backend, Session};
use serde::{Deserialize, Serialize};
use std::{
    io::Write,
    path::{Path, PathBuf},
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

#[derive(Default, PartialEq)]
enum Paste {
    #[default]
    None,
    /// Ctrl+Shift+V: ask the window for the clipboard.
    Wanted,
    /// The clipboard arrives with the next frame's input.
    Requested,
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
    paste: Paste,
    wheel: f32,
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
    /// focus; Ctrl+Shift+A and Ctrl+Q stay with the app. This runs before egui
    /// reads the input, so Tab, arrows and Escape do not move focus.
    pub fn raw_input(&mut self, ctx: &egui::Context, raw: &mut egui::RawInput) {
        let Self {
            session,
            keys,
            paste,
            ..
        } = self;
        let Some(session) = session.as_mut() else {
            return;
        };
        if session.exit_code().is_some() || !ctx.memory(|memory| memory.has_focus(focus_id())) {
            return;
        }
        let (application_cursor, bracketed) = {
            let parser = session.lock();
            let screen = parser.screen();
            (screen.application_cursor(), screen.bracketed_paste())
        };
        let mut bytes = Vec::new();
        raw.events.retain(|event| {
            if reserved(event) || !keyboard(event) {
                return true;
            }
            match event {
                Event::Key {
                    key: Key::V,
                    pressed: true,
                    modifiers,
                    ..
                } if modifiers.command && modifiers.shift => *paste = Paste::Wanted,
                // The clipboard may arrive a pass later: eframe's logic-only
                // passes run this hook too. The request lasts until it arrives.
                Event::Paste(text) if *paste == Paste::Requested => {
                    bytes.extend(pasted(text, bracketed));
                    *paste = Paste::None;
                }
                // Ctrl+V reaches egui as a paste; the agent reads images itself.
                Event::Paste(_) | Event::PasteImage(_) => bytes.push(0x16),
                event => {
                    // A key pressed meanwhile ends a request that got no text.
                    if *paste == Paste::Requested
                        && matches!(event, Event::Key { pressed: true, .. } | Event::Text(_))
                    {
                        *paste = Paste::None;
                    }
                    bytes.extend(keys.encode(event, application_cursor));
                }
            }
            false
        });
        session.write(bytes);
    }

    /// The tab's content. `font` is the editor's code font.
    pub fn show(&mut self, ui: &mut egui::Ui, root: &Path, font: &FontId) {
        self.open(root);
        theme::interface_style(ui);
        let starting = std::mem::take(&mut self.start);
        if starting {
            self.installed = terminal::discover(&std::env::var_os("PATH").unwrap_or_default());
            self.error = None;
        }
        let current = self.backend().map(|(backend, _)| *backend);
        let mut code = self.session.as_mut().and_then(Session::exit_code);
        let status = if self.installed.is_empty() {
            "No agent"
        } else if self.error.is_some() {
            "Failed"
        } else if code.is_some() {
            "Exited"
        } else if self.session.is_some() {
            "Running"
        } else {
            "Starting"
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
                let label = ui.label(RichText::new(status).small().weak());
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
            // The old agent ends now; the new one starts with the next frame.
            self.session = None;
            self.start = true;
            self.focus = true;
            code = None;
            ui.ctx().request_repaint();
        }
        ui.add_space(4.0);
        let cell =
            ui.fonts_mut(|fonts| egui::vec2(fonts.glyph_width(font, 'M'), fonts.row_height(font)));
        let line = if code.is_some() {
            ui.text_style_height(&egui::TextStyle::Small) + 8.0
        } else {
            0.0
        };
        let available = ui.available_rect_before_wrap();
        let area = Rect::from_min_max(available.min, available.max - egui::vec2(0.0, line));
        let size = (
            (area.height() / cell.y).floor().clamp(1.0, 500.0) as u16,
            (area.width() / cell.x).floor().clamp(2.0, 1000.0) as u16,
        );
        if starting && let Some((backend, program)) = self.backend().cloned() {
            let ctx = ui.ctx().clone();
            match Session::start(&program, root, size, move || ctx.request_repaint()) {
                Ok(session) => self.session = Some(session),
                Err(error) => {
                    self.error = Some(format!("Couldn’t start {}: {error}", backend.label()));
                }
            }
        }
        if code.is_none()
            && let Some(session) = &mut self.session
        {
            session.resize(size);
        }
        ui.allocate_rect(area, Sense::hover());
        let response = ui.interact(area, focus_id(), Sense::click());
        if response.clicked() || std::mem::take(&mut self.focus) {
            response.request_focus();
        }
        ui_probe::record(ui.ctx(), "agent-terminal", area);
        if self.paste == Paste::Wanted {
            ui.ctx()
                .send_viewport_cmd(egui::ViewportCommand::RequestPaste);
            self.paste = Paste::Requested;
            ui.ctx().request_repaint();
        }
        let painter = ui.painter_at(area);
        let colors = Colors::new(ui.visuals());
        match &self.session {
            Some(session) => {
                let mut parser = session.lock();
                parser.callbacks_mut().colors = [colors.foreground, colors.background].map(rgb);
                if response.hovered() {
                    scroll(ui, &mut self.wheel, &mut parser, area, cell, session);
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
                if ui_probe::enabled() {
                    for (row, text) in screen.rows(0, screen.size().1).enumerate() {
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
        if let Some(code) = code {
            ui.add_space(4.0);
            let exit = ui.label(
                RichText::new(format!("Exited (code {code})"))
                    .small()
                    .weak(),
            );
            ui_probe::record(ui.ctx(), format!("agent-exit:{code}"), exit.rect);
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

/// The mouse wheel scrolls back through the output, or goes to an agent that
/// asked for mouse reports. `wheel` keeps the part of a line not yet scrolled.
fn scroll(
    ui: &egui::Ui,
    wheel: &mut f32,
    parser: &mut terminal::Parser,
    area: Rect,
    cell: Vec2,
    session: &Session,
) {
    let rows = f32::from(parser.screen().size().0);
    *wheel += ui.input(|input| {
        input
            .events
            .iter()
            .map(|event| match event {
                Event::MouseWheel { unit, delta, .. } => match unit {
                    MouseWheelUnit::Point => delta.y / cell.y,
                    MouseWheelUnit::Line => delta.y * 3.0,
                    MouseWheelUnit::Page => delta.y * rows,
                },
                _ => 0.0,
            })
            .sum::<f32>()
    });
    let steps = wheel.trunc();
    *wheel -= steps;
    if steps == 0.0 {
        return;
    }
    let screen = parser.screen_mut();
    if screen.mouse_protocol_mode() == vt100::MouseProtocolMode::None {
        let offset = screen.scrollback().saturating_add_signed(steps as isize);
        screen.set_scrollback(offset);
        return;
    }
    let pointer = ui
        .input(|input| input.pointer.hover_pos())
        .unwrap_or(area.min)
        - area.min;
    let column = (pointer.x / cell.x).max(0.0) as u16 + 1;
    let row = (pointer.y / cell.y).max(0.0) as u16 + 1;
    let button = if steps > 0.0 { 64 } else { 65 };
    let report = if screen.mouse_protocol_encoding() == vt100::MouseProtocolEncoding::Sgr {
        format!("\x1b[<{button};{column};{row}M").into_bytes()
    } else {
        let [column, row] = [column, row].map(|value| (32 + value.min(223)) as u8);
        vec![0x1b, b'[', b'M', 32 + button, column, row]
    };
    session.write(report.repeat(steps.abs() as usize));
}

/// Keys that stay with the app while the terminal has focus: Ctrl+Shift+A
/// hides the tab and Ctrl+Q quits.
fn reserved(event: &Event) -> bool {
    match event {
        Event::Key {
            key: Key::A,
            modifiers,
            ..
        } => modifiers.command && modifiers.shift,
        Event::Key {
            key: Key::Q,
            modifiers,
            ..
        } => modifiers.command && !modifiers.shift && !modifiers.alt,
        _ => false,
    }
}

fn keyboard(event: &Event) -> bool {
    matches!(
        event,
        Event::Key { .. }
            | Event::Text(_)
            | Event::Paste(_)
            | Event::PasteImage(_)
            | Event::Copy
            | Event::Cut
            | Event::Ime(_)
    )
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

/// Draw the visible rows: one text run per stretch of plain cells in one style,
/// each other character at its own cell so the grid stays aligned.
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
            let mut text = String::new();
            let mut end = col;
            if first.is_wide() || !first.contents().is_ascii() {
                text.push_str(first.contents());
                end += if first.is_wide() { 2 } else { 1 };
            } else {
                while let Some(next) = screen.cell(row, end) {
                    if next.is_wide()
                        || !next.contents().is_ascii()
                        || Style::of(next, colors) != style
                    {
                        break;
                    }
                    text.push_str(if next.has_contents() {
                        next.contents()
                    } else {
                        " "
                    });
                    end += 1;
                }
            }
            let rect = at(row, col, end - col);
            if style.background != colors.background {
                painter.rect_filled(rect.round_to_pixels(pixels), 0, style.background);
            }
            if !text.trim().is_empty() || style.underline {
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
                if style.bold {
                    // No bold monospace face ships with the app: overstrike by a pixel.
                    painter.galley(
                        rect.min + egui::vec2(1.0 / pixels, 0.0),
                        galley.clone(),
                        style.foreground,
                    );
                }
                painter.galley(rect.min, galley, style.foreground);
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
    fn pastes_end_lines_in_cr_and_cannot_close_bracketed_mode() {
        assert_eq!(pasted("one\ntwo\r\n", false), b"one\rtwo\r");
        assert_eq!(pasted("a\x1b[201~b", true), b"\x1b[200~a[201~b\x1b[201~");
    }

    #[test]
    fn the_app_keeps_hide_and_quit_while_the_terminal_has_focus() {
        let command = Modifiers::COMMAND;
        assert!(reserved(&key(Key::A, command | Modifiers::SHIFT)));
        assert!(reserved(&key(Key::Q, command)));
        for other in [
            key(Key::A, command),
            key(Key::P, command),
            key(Key::Q, command | Modifiers::SHIFT),
            key(Key::Tab, Modifiers::NONE),
        ] {
            assert!(!reserved(&other));
        }
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
