//! Optional control bounds for native GUI tests; inactive in ordinary runs.
use eframe::egui;
use std::{collections::BTreeMap, path::PathBuf, sync::OnceLock};

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
        data.get_temp_mut_or_default::<BTreeMap<String, [f32; 4]>>(egui::Id::unique("ui-probe"))
            .insert(
                name.into(),
                [rect.min.x, rect.min.y, rect.max.x, rect.max.y],
            );
    });
}

pub fn flush(ctx: &egui::Context) {
    let Some(path) = destination() else {
        return;
    };
    let controls =
        ctx.data_mut(|data| {
            std::mem::take(data.get_temp_mut_or_default::<BTreeMap<String, [f32; 4]>>(
                egui::Id::unique("ui-probe"),
            ))
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
