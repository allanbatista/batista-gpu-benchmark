pub mod overlay;
pub mod panels;
pub mod system_info;

use bevy::prelude::*;
use bevy_egui::{EguiPlugin, EguiPrimaryContextPass, egui};

/// egui 0.35 removed ctx-level panels; panels are shown inside a root `Ui`
/// spanning the viewport background.
pub(crate) fn root_ui(ctx: &egui::Context, id: &'static str) -> egui::Ui {
    egui::Ui::new(
        ctx.clone(),
        id.into(),
        egui::UiBuilder::new()
            .layer_id(egui::LayerId::background())
            .max_rect(ctx.viewport_rect()),
    )
}

pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(EguiPlugin::default())
            .init_resource::<system_info::SystemInfo>()
            .add_systems(EguiPrimaryContextPass, (panels::panels_ui, overlay::overlay_ui).chain());
    }
}
