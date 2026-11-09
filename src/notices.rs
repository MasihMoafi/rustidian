//! One short-lived status-strip notice. Render-time failures are reported once;
//! explicit actions can report the same failure again without a growing queue.
use crate::{theme, ui_probe};
use eframe::egui::{self, RichText};
use std::{
    collections::HashSet,
    hash::Hash,
    time::{Duration, Instant},
};

const LIFETIME: Duration = Duration::from_secs(6);

#[derive(Clone)]
struct Notice {
    message: String,
    since: Instant,
}

#[derive(Clone, Default)]
struct State {
    current: Option<Notice>,
    seen: HashSet<egui::Id>,
}

fn state_id() -> egui::Id {
    egui::Id::unique("status-notices")
}

/// An action failed. Calling this again is a new attempt, with a fresh lifetime.
pub fn report(ctx: &egui::Context, message: impl Into<String>) {
    let message = message.into();
    if message.is_empty() {
        return;
    }
    eprintln!("warning: {message}");
    ctx.data_mut(|data| {
        let state = data.get_temp_mut_or_default::<State>(state_id());
        state.current = Some(Notice {
            message,
            since: Instant::now(),
        });
    });
    ctx.request_repaint();
}

/// A background/render failure is consumed even when another notice occupies the
/// strip. It must never turn into a delayed warning after that notice expires.
pub fn once(ctx: &egui::Context, key: impl Hash + std::fmt::Debug, message: impl Into<String>) {
    let message = message.into();
    let show = ctx.data_mut(|data| {
        let state = data.get_temp_mut_or_default::<State>(state_id());
        state.seen.insert(egui::Id::unique(key)) && state.current.is_none()
    });
    if show {
        report(ctx, message);
    }
}

#[derive(Default)]
pub struct Action {
    pub hosted: bool,
    pub setup: bool,
    pub voice: bool,
}

/// Draw after dialogs so their backdrop cannot hide the status warning. The
/// entire area is clipped to the strip, including at narrow window widths.
pub fn show(ctx: &egui::Context, rect: egui::Rect, fallback: Option<(&str, &str)>) -> Action {
    let now = Instant::now();
    let current = ctx.data_mut(|data| {
        let state = data.get_temp_mut_or_default::<State>(state_id());
        if state
            .current
            .as_ref()
            .is_some_and(|notice| notice.since.elapsed() >= LIFETIME)
        {
            state.current = None;
        }
        state.current.clone()
    });
    let mut action = Action::default();
    let Some(notice) = current else { return action };
    let fallback = fallback.filter(|(message, _)| *message == notice.message);
    action.voice = fallback.is_some();
    if rect.width() < 36.0 {
        return action;
    }
    let mut dismiss = false;
    let shown = egui::Area::new(egui::Id::unique("status-warning"))
        .order(egui::Order::Tooltip)
        .constrain(false)
        .pivot(egui::Align2::LEFT_CENTER)
        .fixed_pos(rect.left_center())
        .show(ctx, |ui| {
            ui.set_clip_rect(rect);
            ui.set_max_size(rect.size());
            ui.spacing_mut().button_padding = egui::vec2(3.0, 0.0);
            ui.spacing_mut().interact_size.y = 18.0;
            ui.horizontal(|ui| {
                ui.add(
                    theme::icon_image(theme::Icon::CalloutWarning, ui.visuals().warn_fg_color)
                        .fit_to_exact_size(egui::vec2(12.0, 12.0)),
                );
                // Leave space for the close button and optional voice actions.
                let actions_width = if fallback.is_some() { 150.0 } else { 24.0 };
                let width = (rect.width() - actions_width - 24.0).max(12.0);
                let line = notice.message.lines().next().unwrap_or_default();
                let label = ui
                    .add_sized(
                        egui::vec2(width, 18.0),
                        egui::Label::new(RichText::new(line).size(11.0)).truncate(),
                    )
                    .on_hover_text(&notice.message);
                ui_probe::record(ctx, "error-notice", label.rect.intersect(rect));
                ui_probe::record(
                    ctx,
                    format!("error-message:{}", notice.message),
                    label.rect.intersect(rect),
                );
                if let Some((_, setup)) = fallback {
                    let hosted = ui
                        .small_button("OpenRouter")
                        .on_hover_text("Use OpenRouter (paid)");
                    ui_probe::record(
                        ctx,
                        "voice-fallback-use-openrouter",
                        hosted.rect.intersect(rect),
                    );
                    action.hosted = hosted.clicked();
                    let setup = ui.small_button("Set up").on_hover_text(setup);
                    ui_probe::record(ctx, "voice-fallback-setup", setup.rect.intersect(rect));
                    action.setup = setup.clicked();
                }
                let close = ui.small_button("×");
                ui_probe::record(ctx, "error-dismiss", close.rect.intersect(rect));
                dismiss = close.clicked();
            });
        });
    ctx.data_mut(|data| {
        let state = data.get_temp_mut_or_default::<State>(state_id());
        if dismiss || action.hosted || action.setup {
            state.current = None;
        } else if let Some(current) = &mut state.current {
            if shown.response.contains_pointer() {
                current.since = now;
                ctx.request_repaint_after(Duration::from_millis(250));
            } else {
                ctx.request_repaint_after(LIFETIME.saturating_sub(current.since.elapsed()));
            }
        }
    });
    action
}
