//! Bevy host support for Mara-owned borderless window chrome.
//!
//! The core crate owns the hit-test contract. This module maps those
//! host-neutral results onto Bevy/winit native-window operations.

use bevy::math::CompassOctant;
use bevy::prelude::*;
use bevy::window::{CursorIcon, PrimaryWindow, SystemCursorIcon, Window};
use bevy_egui::{EguiContext, EguiPreUpdateSet, EguiPrimaryContextPass, PrimaryEguiContext, egui};

/// Runtime switches for Mara's borderless native-window chrome.
#[derive(Resource, Clone, Copy, Debug)]
pub struct MaraWindowChromeSettings {
    /// Whether Mara should drive native borderless move/resize.
    pub enabled: bool,
    /// Allow edge/corner resize hit-testing.
    pub resize: bool,
    /// Allow dragging the published main-bar empty regions to move
    /// the native window.
    pub move_from_drag_regions: bool,
}

/// Mara window-chrome regions copied out of egui during the egui
/// pass, then consumed by Bevy's native window system in PreUpdate.
///
/// This keeps native move/resize hit-testing out of `egui::Context`
/// during Bevy's main schedules, avoiding egui lock contention.
#[derive(Resource, Clone, Debug, Default)]
pub struct MaraWindowChromeRegions {
    pub regions: mara_core::WindowChromeRegions,
}

/// One-frame Bevy-side input claim for native window chrome.
#[derive(Resource, Clone, Copy, Debug, Default)]
pub struct MaraWindowChromeInputClaim {
    state: mara_core::WindowChromeState,
    claimed: bool,
}

impl MaraWindowChromeInputClaim {
    #[must_use]
    pub fn claimed(self) -> bool {
        self.claimed
    }
}

/// Systems sets for Mara's Bevy window-chrome bridge.
///
/// Add app UI systems that publish Mara chrome regions before
/// `SyncRegions` when same-frame native hit-testing matters.
#[derive(SystemSet, Clone, Hash, Debug, Eq, PartialEq)]
pub enum MaraWindowChromeSet {
    /// Reconciles stale native claims from egui's pointer state.
    ReleaseClaim,
    /// Copies the egui-published chrome regions into Bevy resources.
    SyncRegions,
}

impl Default for MaraWindowChromeSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            resize: true,
            move_from_drag_regions: true,
        }
    }
}

/// Installs Bevy-native move/resize behavior for Mara borderless
/// window chrome.
///
/// Apps still decide whether the OS window uses native decorations.
/// This plugin is intended for windows with `Window::decorations =
/// false`; it reads the Mara chrome regions published by the ribbon
/// renderer and theme-owned resize metrics from `mara_core::style`.
pub struct MaraWindowChromePlugin;

impl Plugin for MaraWindowChromePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MaraWindowChromeSettings>()
            .init_resource::<MaraWindowChromeRegions>()
            .init_resource::<MaraWindowChromeInputClaim>()
            .add_systems(
                PreUpdate,
                mara_window_chrome_system
                    .after(EguiPreUpdateSet::ProcessInput)
                    .before(crate::consume_egui_input_system),
            )
            .configure_sets(
                EguiPrimaryContextPass,
                (
                    MaraWindowChromeSet::ReleaseClaim,
                    MaraWindowChromeSet::SyncRegions,
                )
                    .chain(),
            )
            .add_systems(
                EguiPrimaryContextPass,
                (
                    release_window_chrome_claim_system.in_set(MaraWindowChromeSet::ReleaseClaim),
                    sync_window_chrome_regions_system.in_set(MaraWindowChromeSet::SyncRegions),
                ),
            );
    }
}

fn resize_direction_to_compass(direction: mara_core::WindowResizeDirection) -> CompassOctant {
    match direction {
        mara_core::WindowResizeDirection::North => CompassOctant::North,
        mara_core::WindowResizeDirection::NorthEast => CompassOctant::NorthEast,
        mara_core::WindowResizeDirection::East => CompassOctant::East,
        mara_core::WindowResizeDirection::SouthEast => CompassOctant::SouthEast,
        mara_core::WindowResizeDirection::South => CompassOctant::South,
        mara_core::WindowResizeDirection::SouthWest => CompassOctant::SouthWest,
        mara_core::WindowResizeDirection::West => CompassOctant::West,
        mara_core::WindowResizeDirection::NorthWest => CompassOctant::NorthWest,
    }
}

fn resize_cursor_icon(direction: mara_core::WindowResizeDirection) -> SystemCursorIcon {
    match direction {
        mara_core::WindowResizeDirection::North => SystemCursorIcon::NResize,
        mara_core::WindowResizeDirection::NorthEast => SystemCursorIcon::NeResize,
        mara_core::WindowResizeDirection::East => SystemCursorIcon::EResize,
        mara_core::WindowResizeDirection::SouthEast => SystemCursorIcon::SeResize,
        mara_core::WindowResizeDirection::South => SystemCursorIcon::SResize,
        mara_core::WindowResizeDirection::SouthWest => SystemCursorIcon::SwResize,
        mara_core::WindowResizeDirection::West => SystemCursorIcon::WResize,
        mara_core::WindowResizeDirection::NorthWest => SystemCursorIcon::NwResize,
    }
}

fn mara_window_chrome_system(
    mut commands: Commands,
    mut mouse: ResMut<ButtonInput<MouseButton>>,
    settings: Res<MaraWindowChromeSettings>,
    regions: Res<MaraWindowChromeRegions>,
    mut input_claim: ResMut<MaraWindowChromeInputClaim>,
    mut primary_window: Query<(Entity, &mut Window), With<PrimaryWindow>>,
    mut last_resize_cursor: Local<Option<mara_core::WindowResizeDirection>>,
) {
    let Ok((entity, mut window)) = primary_window.single_mut() else {
        return;
    };

    let Some(cursor) = window.cursor_position() else {
        if !settings.enabled {
            input_claim.state.clear_claim();
            input_claim.claimed = false;
        }
        if last_resize_cursor.take().is_some() {
            commands
                .entity(entity)
                .insert(CursorIcon::from(SystemCursorIcon::Default));
        }
        return;
    };
    let pos = egui::pos2(cursor.x, cursor.y);
    let window_size = egui::vec2(window.width(), window.height());

    let update = input_claim.state.update(
        &regions.regions,
        mara_core::WindowChromeInput {
            pointer_pos: Some(pos),
            window_size,
            primary_pressed: mouse.just_pressed(MouseButton::Left),
            primary_released: mouse.just_released(MouseButton::Left),
            // Bevy's native drag path can temporarily lose the held
            // state after handing the operation to the compositor.
            // Release is reconciled from egui's pointer state in the
            // egui pass below.
            primary_down: None,
        },
        mara_core::style::theme().window_chrome,
        mara_core::WindowChromePolicy {
            enabled: settings.enabled,
            resize: settings.resize,
            move_from_drag_regions: settings.move_from_drag_regions,
        },
    );
    input_claim.claimed = update.claimed;

    let hit = update.hit;

    if !settings.enabled {
        if last_resize_cursor.take().is_some() {
            commands
                .entity(entity)
                .insert(CursorIcon::from(SystemCursorIcon::Default));
        }
        return;
    }

    let resize_cursor = match hit {
        Some(mara_core::WindowChromeHit::Resize(direction)) => Some(direction),
        _ => None,
    };
    if resize_cursor != *last_resize_cursor {
        let cursor_icon = resize_cursor
            .map(resize_cursor_icon)
            .unwrap_or(SystemCursorIcon::Default);
        commands
            .entity(entity)
            .insert(CursorIcon::from(cursor_icon));
        *last_resize_cursor = resize_cursor;
    }

    if !mouse.just_pressed(MouseButton::Left) {
        return;
    }

    let handled_native_chrome = match update.start {
        Some(mara_core::WindowChromeHit::Resize(direction)) => {
            window.start_drag_resize(resize_direction_to_compass(direction));
            true
        }
        Some(mara_core::WindowChromeHit::Move) => {
            window.start_drag_move();
            true
        }
        None => false,
    };

    if handled_native_chrome {
        mouse.clear_just_pressed(MouseButton::Left);
    }
}

fn sync_window_chrome_regions_system(
    mut egui_ctx_q: Query<&mut EguiContext, With<PrimaryEguiContext>>,
    mut regions: ResMut<MaraWindowChromeRegions>,
) {
    let Ok(mut egui_ctx) = egui_ctx_q.single_mut() else {
        return;
    };
    regions.regions = mara_core::window_chrome_regions(egui_ctx.get_mut());
}

fn release_window_chrome_claim_system(
    mut egui_ctx_q: Query<&mut EguiContext, With<PrimaryEguiContext>>,
    settings: Res<MaraWindowChromeSettings>,
    mut input_claim: ResMut<MaraWindowChromeInputClaim>,
) {
    let Ok(mut egui_ctx) = egui_ctx_q.single_mut() else {
        return;
    };
    let ctx = egui_ctx.get_mut();
    mara_core::publish_window_chrome_host_capabilities(
        ctx,
        mara_core::WindowChromeHostCapabilities {
            native_move: settings.enabled && settings.move_from_drag_regions,
            native_resize: settings.enabled && settings.resize,
        },
    );

    if !input_claim.state.claimed() {
        return;
    }

    let primary_down = ctx.input(|input| input.pointer.primary_down());
    input_claim.state.release_if_pointer_up(primary_down);
    input_claim.claimed = input_claim.state.claimed();
}
