//! Shared native image placeholders and explicit system-player video cards.
use crate::theme::{radius, space};
use eframe::egui;

/// Use the desktop's media association rather than the browser's URL association.
pub fn open_video(uri: &str) -> Result<(), String> {
    use std::process::{Command, Stdio};
    let opener = match std::env::consts::OS {
        "macos" => "open",
        "windows" => "rundll32",
        _ => "xdg-open",
    };
    let mut command = Command::new(opener);
    if cfg!(target_os = "windows") {
        command.arg("url.dll,FileProtocolHandler");
    }
    let mut child = command
        .arg(uri)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .spawn()
        .map_err(|error| format!("Cannot open the video player: {error}"))?;
    std::thread::spawn(move || {
        if let Err(error) = child.wait() {
            eprintln!("Video player: {error}");
        }
    });
    Ok(())
}

pub fn broken(ui: &mut egui::Ui, alt: &str, clickable: bool) -> egui::Response {
    let label = if alt.is_empty() { "Missing image" } else { alt };
    let short: String = label.chars().take(72).collect();
    let response = ui
        .add(
            egui::Label::new(egui::RichText::new(format!("▧ {short}")).weak()).sense(
                if clickable {
                    egui::Sense::click()
                } else {
                    egui::Sense::hover()
                },
            ),
        )
        .on_hover_text(label);
    crate::ui_probe::record(ui.ctx(), format!("broken-image:{alt}"), response.rect);
    response
}

pub fn image(
    ui: &mut egui::Ui,
    uri: &str,
    alt: &str,
    width: Option<f32>,
    height: Option<f32>,
    clickable: bool,
) -> egui::Response {
    if uri.starts_with("rust-missing-image:") {
        return broken(ui, alt, clickable);
    }
    let limit = ui.available_width().max(1.0);
    let image = egui::Image::new(uri).alt_text(alt).sense(if clickable {
        egui::Sense::click()
    } else {
        egui::Sense::hover()
    });
    let natural = match image.load_for_size(ui.ctx(), egui::vec2(limit, f32::INFINITY)) {
        Ok(poll) => poll.size(),
        Err(_) => return broken(ui, alt, clickable),
    };
    let size = size(
        natural.unwrap_or(egui::vec2(width.unwrap_or(180.0), height.unwrap_or(32.0))),
        width,
        height,
        limit,
    );
    let response = ui.add(image.fit_to_exact_size(size));
    crate::ui_probe::record(ui.ctx(), format!("media-image:{alt}"), response.rect);
    response
}

pub fn size(
    natural: egui::Vec2,
    width: Option<f32>,
    height: Option<f32>,
    limit: f32,
) -> egui::Vec2 {
    let mut size = match (width, height) {
        (Some(width), Some(height)) => egui::vec2(width, height),
        (Some(width), None) => natural * (width / natural.x.max(1.0)),
        (None, Some(height)) => natural * (height / natural.y.max(1.0)),
        _ => natural,
    };
    size *= (limit / size.x.max(1.0)).min(1.0);
    size.max(egui::Vec2::splat(1.0))
}

/// Return a URI for the app's existing desktop handoff. Nothing autoplays.
pub fn video(ui: &mut egui::Ui, target: &str, uri: Option<&str>) -> Option<String> {
    let name = target
        .split(['?', '#'])
        .next()
        .unwrap_or(target)
        .rsplit('/')
        .next()
        .unwrap_or(target);
    let name = rustidian::paths::decode(name).unwrap_or_else(|| name.into());
    let mut clicked = false;
    let response = egui::Frame::NONE
        .fill(ui.visuals().faint_bg_color)
        .stroke(ui.visuals().widgets.noninteractive.bg_stroke)
        .corner_radius(radius::CARD)
        .inner_margin(space::margin(space::M, space::M))
        .show(ui, |ui| {
            ui.set_min_width((ui.available_width().min(480.0) - space::XL).max(1.0));
            ui.horizontal(|ui| {
                let play = ui.add_enabled(uri.is_some(), egui::Button::new("▶ Play"));
                crate::ui_probe::record(ui.ctx(), format!("video-play:{target}"), play.rect);
                clicked = play.clicked();
                ui.vertical(|ui| {
                    ui.label(&name);
                    ui.weak(if uri.is_some() {
                        "Opens in your system player"
                    } else {
                        "Video file is missing"
                    });
                });
            });
        })
        .response;
    crate::ui_probe::record(ui.ctx(), format!("video-card:{target}"), response.rect);
    if clicked {
        uri.map(str::to_owned)
    } else {
        None
    }
}
