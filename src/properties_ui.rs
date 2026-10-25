use eframe::egui::{self, FontId, RichText, TextEdit};
use rustidian::{metadata, tab::Tab};
use serde_yaml_ng::Value;
use std::path::PathBuf;

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Text,
    List,
    Number,
    Checkbox,
    Other,
}
impl Kind {
    fn icon(self, name: &str) -> crate::theme::Icon {
        use crate::theme::Icon;
        if name.eq_ignore_ascii_case("aliases") {
            return Icon::PropertyAliases;
        }
        if name.eq_ignore_ascii_case("tags") {
            return Icon::PropertyTags;
        }
        match self {
            Self::Text | Self::Other => Icon::PropertyText,
            Self::List => Icon::PropertyList,
            Self::Number => Icon::PropertyNumber,
            Self::Checkbox => Icon::PropertyCheckbox,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Text => "Text",
            Self::List => "List",
            Self::Number => "Number",
            Self::Checkbox => "Checkbox",
            Self::Other => "Source",
        }
    }
}
struct Field {
    name: String,
    kind: Kind,
    text: String,
    items: Vec<String>,
    new_item: String,
    checked: bool,
}
impl Field {
    fn new(name: String, value: &Value) -> Self {
        let is_string_list = value
            .as_sequence()
            .is_some_and(|values| values.iter().all(|value| value.as_str().is_some()));
        let kind = if value.is_bool() {
            Kind::Checkbox
        } else if value.is_number() {
            Kind::Number
        } else if is_string_list
            || matches!(name.as_str(), "aliases" | "tags" | "cssclasses")
                && (value.is_null() || value.is_string())
        {
            Kind::List
        } else if value.is_string() || value.is_null() {
            Kind::Text
        } else {
            Kind::Other
        };
        Self {
            name,
            kind,
            text: value.as_str().map(str::to_owned).unwrap_or_else(|| {
                if value.is_null() {
                    String::new()
                } else {
                    serde_yaml_ng::to_string(value)
                        .unwrap_or_default()
                        .trim()
                        .to_owned()
                }
            }),
            items: if let Some(values) = value.as_sequence() {
                values
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect()
            } else {
                value
                    .as_str()
                    .map(|value| vec![value.into()])
                    .unwrap_or_default()
            },
            new_item: String::new(),
            checked: value.as_bool().unwrap_or(false),
        }
    }
    fn value(&self) -> Result<Value, String> {
        match self.kind {
            Kind::Text => Ok(Value::String(self.text.clone())),
            Kind::Checkbox => Ok(Value::Bool(self.checked)),
            Kind::List => Ok(Value::Sequence(
                self.items.iter().cloned().map(Value::String).collect(),
            )),
            Kind::Number => match serde_yaml_ng::from_str(&self.text) {
                Ok(value @ Value::Number(_)) => Ok(value),
                _ => Err(format!("{} needs a number.", self.name)),
            },
            Kind::Other => Err("Edit this property in source mode.".into()),
        }
    }
}

#[derive(Default)]
pub struct Properties {
    path: PathBuf,
    header: String,
    fields: Vec<Field>,
    error: Option<String>,
}

impl Properties {
    /// Returns true when the user requests another property.
    pub fn show(&mut self, ui: &mut egui::Ui, tab: &mut Tab) -> bool {
        let parsed = match metadata::frontmatter(&tab.document.text) {
            Ok(Some(parsed)) => parsed,
            Ok(None) => return false,
            Err(error) => {
                ui.colored_label(
                    egui::Color32::from_rgb(220, 135, 110),
                    RichText::new(error).size(13.0),
                );
                return false;
            }
        };
        let header = &tab.document.text[parsed.range];
        if self.path != tab.document.path || self.header != header {
            self.path = tab.document.path.clone();
            self.header = header.into();
            self.error = None;
            self.fields = parsed
                .values
                .iter()
                .filter_map(|(name, value)| {
                    name.as_str().map(|name| Field::new(name.into(), value))
                })
                .collect();
        }
        let open_id = ui.scope_id().with(("properties-open", &tab.document.path));
        let mut open = ui
            .ctx()
            .data_mut(|data| data.get_temp::<bool>(open_id).unwrap_or(true));
        ui.add_space(12.0);
        let heading = ui
            .scope(|ui| {
                ui.spacing_mut().button_padding = egui::Vec2::ZERO;
                ui.add(
                    egui::Button::new(
                        RichText::new("Properties").size(crate::theme::document_size(ui, 18.0)),
                    )
                    .frame_when_inactive(false),
                )
            })
            .inner;
        crate::ui_probe::record(ui.ctx(), "properties:toggle", heading.rect);
        if heading.clicked() {
            open = !open;
            ui.ctx().data_mut(|data| data.insert_temp(open_id, open));
        }
        if !open {
            ui.add_space(16.0);
            return false;
        }
        ui.add_space(14.0);
        let mut changed = None;
        let mut removed = None;
        let value_width = (ui.available_width() - 166.0).max(120.0);
        egui::Grid::new(ui.scope_id().with(("properties", &tab.document.path)))
            .num_columns(3)
            .min_col_width(0.0)
            .spacing(egui::vec2(4.0, 2.0))
            .min_row_height(33.5)
            .show(ui, |ui| {
                for field in &mut self.fields {
                    let mut kind = field.kind;
                    let type_button = ui
                        .scope(|ui| {
                            ui.spacing_mut().button_padding = egui::vec2(2.0, 6.75);
                            ui.spacing_mut().interact_size.x = 0.0;
                            ui.visuals_mut().widgets.inactive.weak_bg_fill =
                                egui::Color32::TRANSPARENT;
                            let image = crate::theme::icon_image(
                                kind.icon(&field.name),
                                ui.visuals().weak_text_color(),
                            );
                            ui.menu_image_button(image, |ui| {
                                for candidate in
                                    [Kind::Text, Kind::List, Kind::Number, Kind::Checkbox]
                                {
                                    ui.selectable_value(&mut kind, candidate, candidate.name());
                                }
                                if ui.button("Remove property").clicked() {
                                    removed = Some(field.name.clone());
                                }
                            })
                        })
                        .inner;
                    crate::ui_probe::record(
                        ui.ctx(),
                        format!("property:{}:type", field.name),
                        type_button.response.rect,
                    );
                    type_button
                        .response
                        .on_hover_text(format!("{} property", kind.name()));
                    ui.allocate_ui_with_layout(
                        egui::vec2(136.0, 33.5),
                        egui::Layout::left_to_right(egui::Align::Center),
                        |ui| {
                            ui.set_min_width(136.0);
                            ui.add(
                                egui::Label::new(
                                    RichText::new(&field.name)
                                        .size(crate::theme::document_size(ui, 18.0))
                                        .weak(),
                                )
                                .truncate(),
                            );
                        },
                    );
                    let mut edited = false;
                    if kind != field.kind {
                        if kind == Kind::List && field.kind != Kind::List {
                            field.items = if field.text.is_empty() {
                                Vec::new()
                            } else {
                                vec![field.text.clone()]
                            };
                        }
                        if kind == Kind::Text && field.kind == Kind::List {
                            field.text = field.items.join(", ");
                        }
                        if kind == Kind::Text && field.kind == Kind::Checkbox {
                            field.text = field.checked.to_string();
                        }
                        field.kind = kind;
                        edited = true;
                    }
                    match field.kind {
                        Kind::Text | Kind::Number => {
                            let input = ui.add(
                                TextEdit::singleline(&mut field.text)
                                    .id_salt((&tab.document.path, &field.name))
                                    .font(FontId::proportional(crate::theme::document_size(
                                        ui, 18.0,
                                    )))
                                    .frame(egui::Frame::NONE)
                                    .desired_width(value_width)
                                    .hint_text(if field.kind == Kind::Number {
                                        "Number"
                                    } else {
                                        "Empty"
                                    }),
                            );
                            crate::ui_probe::record(
                                ui.ctx(),
                                format!("property:{}:value", field.name),
                                input.rect,
                            );
                            edited |= input.changed();
                        }
                        Kind::Checkbox => {
                            let input = crate::theme::checkbox(ui, &mut field.checked);
                            crate::ui_probe::record(
                                ui.ctx(),
                                format!("property:{}:value", field.name),
                                input.rect,
                            );
                            edited |= input.changed();
                        }
                        Kind::List => {
                            let tags = field.name.eq_ignore_ascii_case("tags");
                            ui.horizontal_wrapped(|ui| {
                                ui.spacing_mut().item_spacing.x = 4.0;
                                let mut remove = None;
                                for (index, value) in field.items.iter_mut().enumerate() {
                                    let fill = if tags {
                                        crate::theme::accent(ui).gamma_multiply(0.15)
                                    } else {
                                        egui::Color32::TRANSPARENT
                                    };
                                    egui::Frame::NONE
                                        .fill(fill)
                                        .corner_radius(8)
                                        .inner_margin(if tags {
                                            egui::Margin::symmetric(6, 2)
                                        } else {
                                            egui::Margin::ZERO
                                        })
                                        .show(ui, |ui| {
                                            ui.horizontal(|ui| {
                                                let width = ui
                                                    .fonts_mut(|fonts| {
                                                        fonts
                                                            .layout_no_wrap(
                                                                value.clone(),
                                                                FontId::proportional(
                                                                    crate::theme::document_size(
                                                                        ui, 18.0,
                                                                    ),
                                                                ),
                                                                ui.visuals().text_color(),
                                                            )
                                                            .size()
                                                            .x
                                                            + 4.0
                                                    })
                                                    .clamp(24.0, value_width.max(24.0));
                                                let mut input = TextEdit::singleline(value)
                                                    .id_salt((
                                                        &tab.document.path,
                                                        &field.name,
                                                        index,
                                                    ))
                                                    .font(FontId::proportional(
                                                        crate::theme::document_size(ui, 18.0),
                                                    ))
                                                    .frame(egui::Frame::NONE)
                                                    .desired_width(width);
                                                if tags {
                                                    input =
                                                        input.text_color(crate::theme::accent(ui));
                                                }
                                                edited |= ui.add(input).changed();
                                                if ui
                                                    .add(
                                                        egui::Button::new(
                                                            RichText::new("×").size(14.0).weak(),
                                                        )
                                                        .frame_when_inactive(false),
                                                    )
                                                    .clicked()
                                                {
                                                    remove = Some(index);
                                                }
                                            });
                                        });
                                }
                                if let Some(index) = remove {
                                    field.items.remove(index);
                                    edited = true;
                                }
                                let entry = ui.add(
                                    TextEdit::singleline(&mut field.new_item)
                                        .id_salt((&tab.document.path, &field.name, "new-item"))
                                        .font(FontId::proportional(crate::theme::document_size(
                                            ui, 18.0,
                                        )))
                                        .frame(egui::Frame::NONE)
                                        .desired_width(140.0)
                                        .hint_text(if field.items.is_empty() {
                                            "Empty"
                                        } else {
                                            ""
                                        }),
                                );
                                crate::ui_probe::record(
                                    ui.ctx(),
                                    format!("property:{}:add", field.name),
                                    entry.rect,
                                );
                                if entry.lost_focus()
                                    && ui.input(|input| input.key_pressed(egui::Key::Enter))
                                    && !field.new_item.trim().is_empty()
                                {
                                    field.items.push(
                                        std::mem::take(&mut field.new_item).trim().to_owned(),
                                    );
                                    edited = true;
                                    entry.request_focus();
                                }
                            });
                        }
                        Kind::Other => {
                            ui.label(
                                RichText::new("Edit this value in Source mode")
                                    .size(13.0)
                                    .weak(),
                            );
                        }
                    }
                    if edited {
                        match field.value() {
                            Ok(value) => {
                                changed = Some((field.name.clone(), value));
                                self.error = None;
                            }
                            Err(error) => self.error = Some(error),
                        }
                    }
                    ui.end_row();
                }
            });
        let removed_name = removed.clone();
        let result = if let Some(name) = removed {
            Some(metadata::remove_property(&mut tab.document.text, &name))
        } else {
            changed
                .map(|(name, value)| metadata::set_property(&mut tab.document.text, &name, value))
        };
        if let Some(result) = result {
            match result {
                Ok(()) => {
                    tab.edited();
                    if let Some(name) = removed_name {
                        self.fields.retain(|field| field.name != name);
                    }
                    if let Ok(Some(parsed)) = metadata::frontmatter(&tab.document.text) {
                        self.header = tab.document.text[parsed.range].into();
                    }
                }
                Err(error) => self.error = Some(error),
            }
        }
        if let Some(error) = &self.error {
            ui.colored_label(
                egui::Color32::from_rgb(220, 135, 110),
                RichText::new(error).size(12.0),
            );
        }
        let add = ui
            .scope(|ui| {
                ui.spacing_mut().button_padding = egui::vec2(6.0, 2.0);
                let fill = crate::theme::hover_fill(ui);
                ui.add(
                    egui::Button::new(RichText::new("+ Add property").size(15.75))
                        .fill(fill)
                        .corner_radius(6)
                        .min_size(egui::vec2(0.0, 31.5)),
                )
            })
            .inner
            .clicked();
        ui.add_space(29.0);
        add
    }
}
