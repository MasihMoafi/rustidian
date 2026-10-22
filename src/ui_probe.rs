//! Optional control bounds for native GUI tests; inactive in ordinary runs.
use eframe::egui;
use std::{collections::BTreeMap, path::PathBuf, sync::OnceLock};

type Bounds = BTreeMap<String, [f32; 4]>;

fn qualified(prefix: Option<String>, name: String) -> String {
    match prefix {
        Some(prefix) => format!("{prefix}{name}"),
        None => name,
    }
}

fn destination() -> Option<&'static PathBuf> {
    static PATH: OnceLock<Option<PathBuf>> = OnceLock::new();
    PATH.get_or_init(|| std::env::var_os("RUST_NOTES_UI_PROBE").map(PathBuf::from))
        .as_ref()
}

pub fn record(ctx: &egui::Context, name: impl Into<String>, rect: egui::Rect) {
    if destination().is_none() || !rect.is_positive() || !rect.intersects(ctx.content_rect()) {
        return;
    }
    ctx.data_mut(|data| {
        let name = qualified(
            data.get_temp::<String>(egui::Id::unique("ui-probe-prefix")),
            name.into(),
        );
        data.get_temp_mut_or_default::<Bounds>(egui::Id::unique("ui-probe"))
            .insert(name, [rect.min.x, rect.min.y, rect.max.x, rect.max.y]);
    });
}

pub fn scope<R>(ctx: &egui::Context, prefix: &str, show: impl FnOnce() -> R) -> R {
    if !enabled() {
        return show();
    }
    let id = egui::Id::unique("ui-probe-prefix");
    let previous = ctx.data_mut(|data| {
        let previous = data.get_temp::<String>(id);
        data.insert_temp(id, prefix.to_owned());
        previous
    });
    let result = show();
    ctx.data_mut(|data| {
        data.remove::<String>(id);
        if let Some(previous) = previous {
            data.insert_temp(id, previous);
        }
    });
    result
}

pub fn enabled() -> bool {
    destination().is_some()
}

pub fn metric(ctx: &egui::Context, name: &str, value: f32) {
    if destination().is_some() {
        ctx.data_mut(|data| {
            let name = qualified(
                data.get_temp::<String>(egui::Id::unique("ui-probe-prefix")),
                name.to_owned(),
            );
            data.get_temp_mut_or_default::<Bounds>(egui::Id::unique("ui-probe"))
                .insert(name, [value; 4]);
        });
    }
}

pub fn flush(ctx: &egui::Context) {
    let Some(path) = destination() else {
        return;
    };
    let controls = ctx.data_mut(|data| {
        std::mem::take(data.get_temp_mut_or_default::<Bounds>(egui::Id::unique("ui-probe")))
    });
    let result = (|| -> Result<(), Box<dyn std::error::Error>> {
        let temporary = path.with_extension("tmp");
        std::fs::write(&temporary, serde_json::to_vec(&controls)?)?;
        std::fs::rename(temporary, path)?;
        Ok(())
    })();
    if let Err(error) = result {
        eprintln!("Native GUI probe: {error}");
    }
}
