use crate::theme::{self, Icon};
use eframe::egui::{
    self, Align, Color32, FontId, Layout, Margin, PointerButton, Sense, ViewportCommand,
};
use std::sync::atomic::{AtomicI32, Ordering};

/// Leave Rustidian's exact Ctrl+Q as the only quit key. egui's built-in one
/// also matches Ctrl+Shift+Q and Ctrl+Alt+Q, and fires under dialogs.
pub fn disable_builtin_quit(ctx: &egui::Context) {
    ctx.options_mut(|options| options.quit_shortcuts.clear());
}

/// Whether Ctrl+Q (Cmd+Q on macOS) was pressed with no other modifier;
/// consumes it.
pub fn quit_key(ctx: &egui::Context) -> bool {
    ctx.input_mut(|input| {
        let before = input.events.len();
        input.events.retain(|event| {
            !matches!(
                event,
                egui::Event::Key {
                    key: egui::Key::Q,
                    pressed: true,
                    modifiers,
                    ..
                } if modifiers.matches_exact(egui::Modifiers::COMMAND)
            )
        });
        input.events.len() < before
    })
}

fn close_source_id() -> egui::Id {
    egui::Id::new("rustidian-close-source")
}

/// Close the window because of `source`. The close handler prints
/// `close: <source>` once the close goes ahead, so `app.log` names it.
pub fn close(ctx: &egui::Context, source: &'static str) {
    ctx.data_mut(|data| data.insert_temp(close_source_id(), source));
    ctx.send_viewport_cmd(ViewportCommand::Close);
    // A hidden window runs its next pass only on request.
    ctx.request_repaint();
}

/// Why the window was asked to close: what `close` was given, otherwise the
/// window manager (Alt+F4, the dock, the overview).
pub fn take_close_source(ctx: &egui::Context) -> &'static str {
    ctx.data_mut(|data| data.remove_temp::<&'static str>(close_source_id()))
        .unwrap_or("window manager")
}

/// The write end of the pipe that wakes the signal thread.
static SIGNAL_PIPE: AtomicI32 = AtomicI32::new(-1);
/// The process that installed the handler. A child forked to start a helper
/// runs the handler too until its program starts; it must not close this window.
static SIGNAL_OWNER: AtomicI32 = AtomicI32::new(0);
/// The signal the window has not yet acted on.
static SIGNALLED: AtomicI32 = AtomicI32::new(0);

const SIG_DFL: usize = 0;
const SIG_IGN: usize = 1;

unsafe extern "C" {
    /// libc's `signal`; std already links libc.
    #[link_name = "signal"]
    fn set_disposition(signal: i32, handler: usize) -> usize;
}

extern "C" fn on_signal(signal: i32) {
    // Only async-signal-safe calls here: getpid and write.
    if rustix::process::getpid().as_raw_nonzero().get() != SIGNAL_OWNER.load(Ordering::Relaxed) {
        return;
    }
    let pipe = SIGNAL_PIPE.load(Ordering::Relaxed);
    if pipe >= 0 {
        // SAFETY: the write end stays open until the process exits.
        let pipe = unsafe { std::os::fd::BorrowedFd::borrow_raw(pipe) };
        let _ = rustix::io::write(pipe, &[signal as u8]);
    }
}

/// SIGTERM, SIGHUP and SIGINT close the window the way its close button does,
/// without asking about unsaved edits, so the Agent terminal and the helper
/// processes end with it. A second signal, or a window still open five
/// seconds later, ends the process the way the signal would have. Signals
/// the launch ignored (nohup, a background job) stay ignored.
pub fn close_on_signals(ctx: egui::Context) {
    use rustix::process::Signal;
    use std::io::Read as _;
    let Ok((mut reader, writer)) = std::io::pipe() else {
        return;
    };
    SIGNAL_OWNER.store(
        rustix::process::getpid().as_raw_nonzero().get(),
        Ordering::Relaxed,
    );
    SIGNAL_PIPE.store(
        std::os::fd::IntoRawFd::into_raw_fd(writer),
        Ordering::Relaxed,
    );
    let handler = on_signal as extern "C" fn(i32) as usize;
    for signal in [Signal::TERM, Signal::HUP, Signal::INT] {
        // SAFETY: `on_signal` makes only async-signal-safe calls.
        unsafe {
            if set_disposition(signal.as_raw(), handler) == SIG_IGN {
                set_disposition(signal.as_raw(), SIG_IGN);
            }
        }
    }
    let _ = std::thread::Builder::new()
        .name("rustidian-signals".into())
        .spawn(move || {
            let mut received = [0];
            if reader.read_exact(&mut received).is_err() {
                return;
            }
            let number = i32::from(received[0]);
            SIGNALLED.store(number, Ordering::Relaxed);
            ctx.request_repaint();
            let mut next = [rustix::event::PollFd::new(
                &reader,
                rustix::event::PollFlags::IN,
            )];
            let _ = rustix::event::poll(
                &mut next,
                Some(&rustix::event::Timespec {
                    tv_sec: 5,
                    tv_nsec: 0,
                }),
            );
            // SAFETY: restores the default disposition, which ends the process.
            unsafe { set_disposition(number, SIG_DFL) };
            if let Some(signal) = Signal::from_named_raw(number) {
                let _ = rustix::process::kill_process(rustix::process::getpid(), signal);
            }
        });
}

/// The signal that asked the window to close, once.
pub fn take_signal() -> Option<&'static str> {
    use rustix::process::Signal;
    let number = SIGNALLED.swap(0, Ordering::Relaxed);
    [
        (Signal::TERM, "SIGTERM"),
        (Signal::HUP, "SIGHUP"),
        (Signal::INT, "SIGINT"),
    ]
    .into_iter()
    .find_map(|(signal, name)| (signal.as_raw() == number).then_some(name))
}

/// Keep window controls in the native app so hover feedback matches every other control.
pub fn title_bar(
    ui: &mut egui::Ui,
    title: &str,
    file_actions_enabled: bool,
) -> Option<rustidian::native_picker::Kind> {
    use rustidian::{commands, native_picker::Kind};
    let mut action = None;
    resize_edges(ui);
    let maximized = ui.input(|input| input.viewport().maximized.unwrap_or(false));
    egui::Panel::top("window-title-bar")
        .exact_size(34.0)
        .resizable(false)
        .frame(
            egui::Frame::NONE
                .fill(ui.visuals().panel_fill)
                .inner_margin(Margin::symmetric(6, 3)),
        )
        .show(ui, |ui| {
            theme::menu_style(ui);
            ui.horizontal(|ui| {
                let menu = ui.menu_button("File", |ui| {
                    theme::menu_style(ui);
                    ui.set_min_width(240.0);
                    for (kind, label, shortcut, probe) in [
                        (
                            Kind::File,
                            commands::OPEN_FILE,
                            commands::OPEN_FILE_SHORTCUT,
                            "file-menu-open-file",
                        ),
                        (
                            Kind::Folder,
                            commands::OPEN_FOLDER,
                            commands::OPEN_FOLDER_SHORTCUT,
                            "file-menu-open-folder",
                        ),
                    ] {
                        let button = ui.add_enabled(
                            file_actions_enabled,
                            egui::Button::new(label).shortcut_text(shortcut),
                        );
                        crate::ui_probe::record(ui.ctx(), probe, button.rect);
                        if button.clicked() {
                            action = Some(kind);
                            ui.close();
                        }
                    }
                    if !file_actions_enabled {
                        ui.label("Finish the current dialog before opening another item.");
                    }
                });
                crate::ui_probe::record(ui.ctx(), "file-menu", menu.response.rect);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if control(ui, Icon::Close, "Close window", true).clicked() {
                        // The close-request handler keeps unsaved changes safe.
                        close(ui.ctx(), "title bar");
                    }
                    if control(
                        ui,
                        if maximized {
                            Icon::Restore
                        } else {
                            Icon::Maximize
                        },
                        if maximized {
                            "Restore window"
                        } else {
                            "Maximize window"
                        },
                        false,
                    )
                    .clicked()
                    {
                        ui.ctx()
                            .send_viewport_cmd(ViewportCommand::Maximized(!maximized));
                    }
                    if control(ui, Icon::Minimize, "Minimize window", false).clicked() {
                        ui.ctx().send_viewport_cmd(ViewportCommand::Minimized(true));
                    }
                    let rect = ui.available_rect_before_wrap();
                    let response = ui.interact(
                        rect,
                        ui.scope_id().with("title-drag"),
                        Sense::click_and_drag(),
                    );
                    crate::ui_probe::record(ui.ctx(), "window-title-drag", rect);
                    let galley =
                        egui::WidgetText::from(egui::RichText::new(title).size(12.0).weak())
                            .into_galley(
                                ui,
                                Some(egui::TextWrapMode::Truncate),
                                (rect.width() - 12.0).max(1.0),
                                FontId::proportional(12.0),
                            );
                    ui.painter().galley(
                        rect.left_center() + egui::vec2(6.0, -galley.size().y / 2.0),
                        galley,
                        ui.visuals().weak_text_color(),
                    );
                    if response.double_clicked() {
                        ui.ctx()
                            .send_viewport_cmd(ViewportCommand::Maximized(!maximized));
                    } else if response.is_pointer_button_down_on()
                        && ui.input(|input| input.pointer.primary_pressed())
                    {
                        ui.ctx().send_viewport_cmd(ViewportCommand::StartDrag);
                    }
                });
            });
        });
    action
}

fn control(ui: &mut egui::Ui, icon: Icon, label: &str, close: bool) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(34.0, 28.0), Sense::click());
    if close && response.hovered() {
        ui.painter()
            .rect_filled(rect, 6.0, Color32::from_rgb(185, 53, 67));
    } else {
        theme::control_feedback(ui, &response);
    }
    let tint = if close && response.hovered() {
        Color32::WHITE
    } else if response.hovered() {
        ui.visuals().text_color()
    } else {
        ui.visuals().weak_text_color()
    };
    theme::icon_image(icon, tint).paint_at(
        ui,
        egui::Rect::from_center_size(rect.center(), egui::vec2(14.0, 14.0)),
    );
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, label));
    crate::ui_probe::record(ui.ctx(), format!("window:{label}"), rect);
    response.on_hover_text(label)
}

fn resize_edges(ui: &egui::Ui) {
    use egui::{CursorIcon, ResizeDirection};
    let (position, pressed, locked) = ui.input(|input| {
        (
            input.pointer.interact_pos(),
            input.pointer.button_pressed(PointerButton::Primary),
            input.viewport().maximized.unwrap_or(false)
                || input.viewport().fullscreen.unwrap_or(false),
        )
    });
    if locked {
        return;
    }
    let Some(position) = position else {
        return;
    };
    let rect = ui.max_rect();
    let left = position.x < rect.left() + 4.0;
    let right = position.x > rect.right() - 4.0;
    let top = position.y < rect.top() + 4.0;
    let bottom = position.y > rect.bottom() - 4.0;
    let (direction, cursor) = match (left, right, top, bottom) {
        (true, _, true, _) => (ResizeDirection::NorthWest, CursorIcon::ResizeNwSe),
        (_, true, true, _) => (ResizeDirection::NorthEast, CursorIcon::ResizeNeSw),
        (true, _, _, true) => (ResizeDirection::SouthWest, CursorIcon::ResizeNeSw),
        (_, true, _, true) => (ResizeDirection::SouthEast, CursorIcon::ResizeNwSe),
        (true, _, _, _) => (ResizeDirection::West, CursorIcon::ResizeHorizontal),
        (_, true, _, _) => (ResizeDirection::East, CursorIcon::ResizeHorizontal),
        (_, _, true, _) => (ResizeDirection::North, CursorIcon::ResizeVertical),
        (_, _, _, true) => (ResizeDirection::South, CursorIcon::ResizeVertical),
        _ => return,
    };
    ui.ctx().set_cursor_icon(cursor);
    if pressed {
        ui.ctx()
            .send_viewport_cmd(ViewportCommand::BeginResize(direction));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Run one frame in which Q is pressed with `modifiers`, as egui-winit
    /// reports it on Linux; returns whether `quit_key` fired and whether the
    /// frame asked to close the window.
    fn press_q(ctx: &egui::Context, modifiers: egui::Modifiers) -> (bool, bool) {
        let mut quit = false;
        let output = ctx.run_ui(
            egui::RawInput {
                events: vec![egui::Event::Key {
                    key: egui::Key::Q,
                    physical_key: Some(egui::Key::Q),
                    pressed: true,
                    repeat: false,
                    modifiers,
                }],
                ..Default::default()
            },
            |ui| quit |= quit_key(ui.ctx()),
        );
        let closes = output
            .viewport_output
            .get(&egui::ViewportId::ROOT)
            .is_some_and(|viewport| viewport.commands.contains(&ViewportCommand::Close));
        output.drop_without_applying_deltas();
        (quit, closes)
    }

    const CTRL: egui::Modifiers = egui::Modifiers {
        ctrl: true,
        command: true,
        ..egui::Modifiers::NONE
    };

    #[test]
    fn only_exact_ctrl_q_quits() {
        let ctx = egui::Context::default();
        disable_builtin_quit(&ctx);
        assert_eq!(press_q(&ctx, CTRL), (true, false));
        for (extra, name) in [
            (egui::Modifiers::SHIFT, "Ctrl+Shift+Q"),
            (egui::Modifiers::ALT, "Ctrl+Alt+Q"),
        ] {
            assert_eq!(
                press_q(&ctx, CTRL | extra),
                (false, false),
                "{name} must neither quit nor close the window"
            );
        }
    }

    #[test]
    fn egui_builtin_quit_shortcut_is_off() {
        // egui's default closes the window by itself, even on Ctrl+Shift+Q.
        let ctx = egui::Context::default();
        let builtin_closes = |modifiers| {
            let output = ctx.run_ui(
                egui::RawInput {
                    events: vec![egui::Event::Key {
                        key: egui::Key::Q,
                        physical_key: Some(egui::Key::Q),
                        pressed: true,
                        repeat: false,
                        modifiers,
                    }],
                    ..Default::default()
                },
                |_| {},
            );
            let closes = output
                .viewport_output
                .get(&egui::ViewportId::ROOT)
                .is_some_and(|viewport| viewport.commands.contains(&ViewportCommand::Close));
            output.drop_without_applying_deltas();
            closes
        };
        assert!(builtin_closes(CTRL | egui::Modifiers::SHIFT));
        disable_builtin_quit(&ctx);
        for modifiers in [
            CTRL,
            CTRL | egui::Modifiers::SHIFT,
            CTRL | egui::Modifiers::ALT,
        ] {
            assert!(
                !builtin_closes(modifiers),
                "egui closed the window on {modifiers:?}+Q"
            );
        }
    }
}
