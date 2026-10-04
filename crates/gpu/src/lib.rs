//! `mara_gpu` — the opaque GPU handle between Mara hosts and GPU modules.
//!
//! [`MaraRenderState`] wraps the egui-wgpu render state without exposing
//! it: only first-party host code mints one (hidden constructor), and
//! only first-party GPU modules unwrap it (hidden accessor). Sealed app
//! code passes the handle around without ever seeing egui-wgpu types
//! (PLAN.md WS6 / ADR 0002).

/// Opaque handle to the host's GPU render state.
#[derive(Clone, Copy)]
pub struct MaraRenderState<'a>(&'a egui_wgpu::RenderState);

impl<'a> MaraRenderState<'a> {
    /// First-party host hook — apps never construct this themselves.
    #[doc(hidden)]
    #[must_use]
    pub fn __internal_new(render_state: &'a egui_wgpu::RenderState) -> Self {
        Self(render_state)
    }

    /// First-party GPU-module hook — sealed consumers never unwrap.
    #[doc(hidden)]
    #[must_use]
    pub fn __internal_raw(&self) -> &'a egui_wgpu::RenderState {
        self.0
    }
}
