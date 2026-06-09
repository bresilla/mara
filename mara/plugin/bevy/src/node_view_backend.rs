//! Bevy host backend for `mara_graph::node_view`.
//!
//! Bridges the mara_core-side sharp-zoom node graph (secondary
//! `egui::Context` rendered to its own wgpu texture) into Bevy's
//! render world, so `bevy_egui` can sample the result as a
//! regular `egui::Image` in the parent UI.
//!
//! ## Pipeline
//!
//! 1. The `mara_node_graph` widget runs in main world. mara_core
//!    allocates a wgpu render target via the backend's
//!    `wgpu()` (= Bevy's `RenderDevice` device clone), runs its
//!    secondary egui context, tessellates, and paints the result
//!    into the target via `egui-wgpu`.
//! 2. The widget calls `backend.register_native(...)` once per
//!    size-change to allocate (or look up) a Bevy `Image` asset
//!    of matching size. The handle is registered with
//!    `EguiUserTextures` so `bevy_egui` knows about it; the
//!    returned `egui::TextureId` is what the parent UI's
//!    `Image` widget samples.
//! 3. Each frame after rendering, `backend.after_render(...)`
//!    pushes a copy entry into [`PendingNodeViewCopies`].
//! 4. [`PendingNodeViewCopies`] is extracted to render world.
//! 5. The render-world system [`copy_node_view_textures`] queues
//!    `CopyTextureToTexture` for each entry: mara_core-source →
//!    Bevy `GpuImage` of the registered handle.
//! 6. `bevy_egui`'s normal renderer samples the GpuImage when
//!    drawing the parent UI's `Image` widget.
//!
//! Add [`NodeViewPlugin`] to your `App` to wire everything up.

use std::collections::HashMap;

use bevy::asset::{Assets, RenderAssetUsages};
use bevy::image::Image;
use bevy::prelude::*;
use bevy::render::{
    Render, RenderApp, RenderSystems,
    extract_resource::{ExtractResource, ExtractResourcePlugin},
    render_asset::RenderAssets,
    render_resource::{Extent3d, TextureDimension},
    renderer::{RenderDevice, RenderQueue},
    texture::GpuImage,
};
use bevy_egui::{EguiTextureHandle, EguiUserTextures};
use mara_graph::node_view::NodeViewBackend;

/// Bevy plugin that wires the cross-world copy of mara_core's
/// node-view render into a Bevy `Image` asset for `bevy_egui` to
/// sample. Add to your `App` once at startup.
pub struct NodeViewPlugin;

impl Plugin for NodeViewPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PendingNodeViewCopies>();
        app.init_resource::<NodeViewSlots>();
        app.add_plugins(ExtractResourcePlugin::<PendingNodeViewCopies>::default());
        // Clear the queue at the START of each main-world frame
        // so register_native + after_render only see this frame's
        // entries when they push.
        app.add_systems(First, clear_pending_node_view_copies);
        if let Some(render_app) = app.get_sub_app_mut(RenderApp) {
            render_app.add_systems(
                Render,
                copy_node_view_textures.in_set(RenderSystems::Render),
            );
        }
    }
}

/// Per-frame queue of copy commands. Populated by
/// [`BevyNodeViewBackend::after_render`] in main world; consumed
/// by [`copy_node_view_textures`] in render world.
#[derive(Resource, Default, Clone)]
pub struct PendingNodeViewCopies {
    pub entries: Vec<NodeViewCopy>,
}

/// One pending texture-to-texture copy.
#[derive(Clone)]
pub struct NodeViewCopy {
    /// mara_core-allocated source texture (= what `egui-wgpu`
    /// rendered the secondary context's output into this frame).
    pub source_texture: wgpu::Texture,
    /// Bevy `Image` asset whose GpuImage receives the copy.
    /// `bevy_egui` samples this on the parent UI side.
    pub target_handle: Handle<Image>,
    pub size_pixels: [u32; 2],
}

impl ExtractResource for PendingNodeViewCopies {
    type Source = Self;
    fn extract_resource(source: &Self::Source) -> Self {
        source.clone()
    }
}

/// Persistent map from texture-pixel-size to the Bevy Image
/// asset + egui `TextureId` we previously allocated for it.
/// Avoids re-allocating Image assets every frame.
#[derive(Resource, Default)]
pub struct NodeViewSlots {
    by_size: HashMap<[u32; 2], (Handle<Image>, egui::TextureId)>,
}

fn clear_pending_node_view_copies(mut pending: ResMut<PendingNodeViewCopies>) {
    pending.entries.clear();
}

/// Render-world system. Walks `PendingNodeViewCopies` and, for
/// each entry, queues a `CopyTextureToTexture` from the mara_core
/// source texture into the matching Bevy `GpuImage`. Submits the
/// resulting command buffer immediately so the copy is visible
/// to `bevy_egui`'s render pass downstream.
fn copy_node_view_textures(
    pending: Res<PendingNodeViewCopies>,
    images: Res<RenderAssets<GpuImage>>,
    render_device: Res<RenderDevice>,
    render_queue: Res<RenderQueue>,
) {
    if pending.entries.is_empty() {
        return;
    }
    let device = render_device.wgpu_device();
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("mara_node_view_copy_encoder"),
    });
    let mut any = false;
    for copy in &pending.entries {
        let Some(gpu_image) = images.get(copy.target_handle.id()) else {
            continue;
        };
        encoder.copy_texture_to_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &copy.source_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyTextureInfo {
                texture: &gpu_image.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::Extent3d {
                width: copy.size_pixels[0],
                height: copy.size_pixels[1],
                depth_or_array_layers: 1,
            },
        );
        any = true;
    }
    if any {
        render_queue.submit(Some(encoder.finish()));
    }
}

/// Backend instance. Construct fresh each frame from a Bevy
/// system that has the necessary resources, then pass to
/// `mara_node_graph`.
pub struct BevyNodeViewBackend<'a> {
    device: wgpu::Device,
    queue: wgpu::Queue,
    target_format: wgpu::TextureFormat,
    images: &'a mut Assets<Image>,
    egui_textures: &'a mut EguiUserTextures,
    pending: &'a mut PendingNodeViewCopies,
    slots: &'a mut NodeViewSlots,
}

impl<'a> BevyNodeViewBackend<'a> {
    pub fn new(
        device: &RenderDevice,
        queue: &RenderQueue,
        images: &'a mut Assets<Image>,
        egui_textures: &'a mut EguiUserTextures,
        pending: &'a mut PendingNodeViewCopies,
        slots: &'a mut NodeViewSlots,
    ) -> Self {
        Self {
            device: device.wgpu_device().clone(),
            queue: (**queue.0).clone(),
            target_format: wgpu::TextureFormat::Bgra8UnormSrgb,
            images,
            egui_textures,
            pending,
            slots,
        }
    }

    /// Override the texture format if your render target uses
    /// something other than `Bgra8UnormSrgb`. Must match the
    /// format the underlying Bevy `Image` ends up with — they're
    /// allocated on the same format.
    pub fn with_format(mut self, format: wgpu::TextureFormat) -> Self {
        self.target_format = format;
        self
    }
}

impl<'a> NodeViewBackend for BevyNodeViewBackend<'a> {
    fn wgpu(&self) -> (wgpu::Device, wgpu::Queue) {
        (self.device.clone(), self.queue.clone())
    }

    fn target_format(&self) -> wgpu::TextureFormat {
        self.target_format
    }

    fn register_native(
        &mut self,
        _texture: &wgpu::Texture,
        _view: &wgpu::TextureView,
        size_pixels: [u32; 2],
        _filter: wgpu::FilterMode,
    ) -> egui::TextureId {
        // Look up or allocate a Bevy `Image` for this size, then
        // register it with bevy_egui. Same-size graphs across
        // frames reuse the same slot; only resize allocates.
        let format = self.target_format;
        let (handle_, tex_id_): (Handle<Image>, egui::TextureId) = self
            .slots
            .by_size
            .entry(size_pixels)
            .or_insert_with(|| {
                let img = make_image(size_pixels, format);
                let h = self.images.add(img);
                let id = self
                    .egui_textures
                    .add_image(EguiTextureHandle::Strong(h.clone()));
                (h, id)
            })
            .clone();
        let _ = handle_;
        tex_id_
    }

    fn unregister_native(&mut self, id: egui::TextureId) {
        // Find the slot whose tex_id matches and drop the asset +
        // bevy_egui registration.
        let key = self
            .slots
            .by_size
            .iter()
            .find_map(|(k, (_, tid))| if *tid == id { Some(*k) } else { None });
        if let Some(k) = key
            && let Some((handle, _)) = self.slots.by_size.remove(&k)
        {
            let _ = self.egui_textures.remove_image(handle.id());
            self.images.remove(handle.id());
        }
    }

    fn after_render(
        &mut self,
        texture: &wgpu::Texture,
        tex_id: egui::TextureId,
        size_pixels: [u32; 2],
    ) {
        // Find the Bevy handle for this texture id and queue the
        // copy in render world.
        let target_handle = self.slots.by_size.iter().find_map(|(_, (h, tid))| {
            if *tid == tex_id {
                Some(h.clone())
            } else {
                None
            }
        });
        if let Some(target_handle) = target_handle {
            self.pending.entries.push(NodeViewCopy {
                source_texture: texture.clone(),
                target_handle,
                size_pixels,
            });
        }
    }
}

/// Allocate a Bevy `Image` asset of the given physical pixel
/// size. The texture has `COPY_DST | TEXTURE_BINDING` usage so
/// our render-world system can copy into it and `bevy_egui` can
/// sample it.
fn make_image(size_pixels: [u32; 2], format: wgpu::TextureFormat) -> Image {
    // `bevy::render::render_resource::TextureFormat` is just
    // `wgpu::TextureFormat` re-exported, so the format passes
    // through as-is — no conversion needed.
    let mut img = Image::new_fill(
        Extent3d {
            width: size_pixels[0].max(1),
            height: size_pixels[1].max(1),
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        &[0, 0, 0, 0],
        format,
        RenderAssetUsages::default(),
    );
    img.texture_descriptor.usage =
        wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING;
    img
}
