//! Native layouts for HTML that needs children rather than inline shaping.
use super::*;
use crate::theme::space;
use rustidian::html_layout::{self, Token};

pub(super) struct Context<'a> {
    pub links: &'a [(String, String)],
    pub from: &'a std::path::Path,
    pub id: egui::Id,
    pub vault: Option<&'a rustidian::vault::Vault>,
    pub cache: &'a mut CommonMarkCache,
}

fn children(ui: &mut egui::Ui, source: &str, context: &mut Context<'_>) -> Option<String> {
    let source = prepare_images(source, |target| {
        let target = context
            .links
            .iter()
            .find(|(key, _)| key == target)
            .map_or(target, |(_, target)| target);
        rustidian::assets::image_uri(context.vault, context.from, target)
            .or_else(|| rustidian::assets::video_uri(context.vault, context.from, target))
    });
    let parsed = Parsed::new(&source);
    show_blocks(
        ui,
        &parsed.blocks,
        context.links,
        None,
        &mut None,
        context.from,
        context.id,
        None,
        context.cache,
        context.vault,
    )
}

pub(super) fn plain(source: &str) -> String {
    let Some(element) = html_layout::blocks(source).into_iter().next() else {
        return String::new();
    };
    match element.tag.name.as_str() {
        "table" => html_layout::table_rows(source)
            .iter()
            .map(|row| {
                row.iter()
                    .map(|cell| plain_text(&Parsed::new(&cell.source).blocks))
                    .collect::<Vec<_>>()
                    .join("\t")
            })
            .collect::<Vec<_>>()
            .join("\n"),
        "picture" => html_layout::picture_image(source, false)
            .and_then(|image| image.attribute("alt").map(str::to_owned))
            .unwrap_or_default(),
        "video" => element
            .tag
            .attribute("src")
            .unwrap_or("Video")
            .rsplit('/')
            .next()
            .unwrap_or("Video")
            .into(),
        "details" => plain_text(&Parsed::new(&source[element.body]).blocks),
        _ => String::new(),
    }
}

pub(super) fn show(ui: &mut egui::Ui, source: &str, context: &mut Context<'_>) -> Option<String> {
    let element = html_layout::blocks(source).into_iter().next()?;
    match element.tag.name.as_str() {
        "table" => table(ui, source, context),
        "picture" => {
            let image = html_layout::picture_image(source, ui.visuals().dark_mode)?;
            let target = image.attribute("src").unwrap_or_default();
            let uri = rustidian::assets::image_uri(context.vault, context.from, target)
                .unwrap_or_else(|| format!("rust-missing-image:{target}"));
            let width = image.length("width", ui.available_width());
            crate::media_ui::image(
                ui,
                &uri,
                image.attribute("alt").unwrap_or_default(),
                width,
                image.pixels("height"),
                false,
            );
            None
        }
        "video" => {
            let target = element
                .tag
                .attribute("src")
                .map(str::to_owned)
                .or_else(|| {
                    html_layout::tokens(&source[element.body.clone()])
                        .into_iter()
                        .find_map(|token| {
                            if let Token::Tag(tag) = token
                                && tag.name == "source"
                            {
                                tag.attribute("src").map(str::to_owned)
                            } else {
                                None
                            }
                        })
                })
                .unwrap_or_else(|| "Video".into());
            let uri = rustidian::assets::video_uri(context.vault, context.from, &target);
            crate::media_ui::video(ui, &target, uri.as_deref())
        }
        "details" => {
            let summary = html_layout::elements(source)
                .into_iter()
                .find(|element| element.tag.name == "summary");
            let title = summary.as_ref().map_or("Details".into(), |element| {
                plain_text(&Parsed::new(&source[element.body.clone()]).blocks)
            });
            let body = summary.map_or(element.body.clone(), |summary| {
                summary.range.end..element.body.end
            });
            let mut clicked = None;
            let title_key = title.clone();
            let header = egui::CollapsingHeader::new(title)
                .id_salt(context.id)
                .default_open(element.tag.attribute("open").is_some())
                .show(ui, |ui| {
                    clicked = children(ui, &source[body], context);
                });
            crate::ui_probe::record(
                ui.ctx(),
                format!("html-details:{title_key}"),
                header.header_response.rect,
            );
            clicked
        }
        _ => None,
    }
}

fn table(ui: &mut egui::Ui, source: &str, context: &mut Context<'_>) -> Option<String> {
    let rows = html_layout::table_rows(source);
    let columns = rows.iter().map(Vec::len).max().unwrap_or(0);
    if columns == 0 {
        return None;
    }
    let available = ui.available_width().max(1.0);
    let mut widths = vec![None; columns];
    for row in &rows {
        for (column, cell) in row.iter().enumerate() {
            if widths[column].is_none() {
                widths[column] = cell.tag.length("width", available);
            }
        }
    }
    let requested: f32 = widths.iter().flatten().sum();
    let automatic = widths.iter().filter(|width| width.is_none()).count();
    let remaining = if requested < available {
        available - requested
    } else {
        available * automatic as f32 / columns as f32
    };
    let mut widths: Vec<_> = widths
        .into_iter()
        .map(|width| {
            width
                .unwrap_or(remaining / automatic.max(1) as f32)
                .max(1.0)
        })
        .collect();
    let total: f32 = widths.iter().sum();
    for width in &mut widths {
        *width *= available / total;
    }
    let mut clicked = None;
    let table_id = context.id.with((source, available.to_bits()));
    let top = ui.cursor().top();
    for (row_index, row) in rows.iter().enumerate() {
        let id = table_id.with(row_index);
        let previous = ui
            .ctx()
            .data(|data| data.get_temp::<Vec<f32>>(id))
            .unwrap_or_default();
        let height = previous.iter().copied().fold(0.0_f32, f32::max);
        let mut measured = Vec::new();
        let response = ui
            .horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                for (column, cell) in row.iter().enumerate() {
                    let align = match cell.tag.attribute("align") {
                        Some(value) if value.eq_ignore_ascii_case("center") => egui::Align::Center,
                        Some(value) if value.eq_ignore_ascii_case("right") => egui::Align::Max,
                        _ => egui::Align::Min,
                    };
                    let remaining =
                        (height - previous.get(column).copied().unwrap_or(height)).max(0.0);
                    let pad = match cell.tag.attribute("valign") {
                        Some(value) if value.eq_ignore_ascii_case("top") => 0.0,
                        Some(value) if value.eq_ignore_ascii_case("bottom") => remaining,
                        _ => remaining / 2.0,
                    };
                    let response = ui
                        .allocate_ui_with_layout(
                            egui::vec2(widths[column], 0.0),
                            egui::Layout::top_down(align),
                            |ui| {
                                ui.set_width(widths[column]);
                                let content = egui::Frame::NONE
                                    .inner_margin(space::margin(space::S, space::S))
                                    .show(ui, |ui| {
                                        ui.set_width((widths[column] - space::L).max(1.0));
                                        ui.add_space(pad);
                                        let start = ui.cursor().top();
                                        let body = if cell.tag.name == "th" {
                                            format!("<b>{}</b>", cell.source)
                                        } else {
                                            cell.source.clone()
                                        };
                                        let body = match align {
                                            egui::Align::Center => {
                                                format!("<div align='center'>\n\n{body}\n\n</div>")
                                            }
                                            egui::Align::Max => {
                                                format!("<div align='right'>\n\n{body}\n\n</div>")
                                            }
                                            _ => body,
                                        };
                                        let mut child = Context {
                                            links: context.links,
                                            from: context.from,
                                            id: id.with(column),
                                            vault: context.vault,
                                            cache: &mut *context.cache,
                                        };
                                        if let Some(link) = children(ui, &body, &mut child) {
                                            clicked = Some(link);
                                        }
                                        measured.push(ui.cursor().top() - start);
                                    });
                                ui.set_min_height(height + space::L);
                                content.response
                            },
                        )
                        .response;
                    crate::ui_probe::record(
                        ui.ctx(),
                        format!("html-cell:{row_index}:{column}"),
                        response.rect,
                    );
                }
            })
            .response;
        if previous != measured {
            ui.ctx().request_repaint();
        }
        ui.ctx().data_mut(|data| data.insert_temp(id, measured));
        let border = ui.visuals().widgets.noninteractive.bg_stroke;
        ui.painter()
            .rect_stroke(response.rect, 0.0, border, egui::StrokeKind::Inside);
        let mut x = response.rect.left();
        for width in widths.iter().take(columns.saturating_sub(1)) {
            x += width;
            ui.painter().line_segment(
                [
                    egui::pos2(x, response.rect.top()),
                    egui::pos2(x, response.rect.bottom()),
                ],
                border,
            );
        }
    }
    crate::ui_probe::record(
        ui.ctx(),
        "html-table",
        egui::Rect::from_min_max(
            egui::pos2(ui.min_rect().left(), top),
            egui::pos2(ui.min_rect().left() + available, ui.cursor().top()),
        ),
    );
    clicked
}
