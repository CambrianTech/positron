#![forbid(unsafe_code)]
#![warn(missing_docs)]
#![warn(rust_2018_idioms)]

//! # positron-wgpu
//!
//! The GPU reference [`Renderer`](positron_core::Renderer) for positron —
//! **outlier B** in the contract's outlier-validation. Where `counter-cli`'s
//! `String` and `positron-ratatui`'s `Paragraph` are both CPU text trees (a weak
//! outlier pair), here a renderer's `type Output` is a [`Frame`]: GPU-bound
//! *geometry* (colored quads headed for a vertex buffer). If [`Renderer`] fits
//! this without forcing, it carries no hidden CPU-tree assumption — which is the
//! whole claim behind "web ≠ the DOM": one Rust wgpu renderer runs native
//! (Metal / Vulkan / DX12) and web (WebGPU under WASM) from the same source.
//!
//! The crate splits cleanly along the same seam as `positron-ratatui`:
//! - [`Frame`] / [`Primitive`] / [`RgbaFrame`] — pure data, no hardware. The
//!   projection `render(state) -> Frame` is unit-tested headlessly.
//! - [`Gpu`] — the one type that touches wgpu; [`Gpu::rasterize`] turns a
//!   [`Frame`] into pixels. Proven by the `counter_gpu` example (runs on any real
//!   machine), kept out of the CI test surface (runners have no GPU adapter).
//! - [`render_to_rgba`] — the consumer entry point: project a [`ViewState`]
//!   through a [`Renderer`] and rasterize it in one call.
//!
//! positron owns the *contract*; this crate owns *one surface projection*. It
//! knows nothing of any substrate's state — it renders whatever `ViewState` it
//! is given into quads.

mod frame;
mod gpu;

pub use frame::{Frame, Primitive, Rect, Rgba, RgbaFrame};
pub use gpu::{Gpu, GpuError};

use positron_core::{Renderer, ViewState};

/// Project a [`ViewState`] through a [`Renderer`] (whose `Output` is a [`Frame`])
/// and rasterize it to pixels on `gpu`. The GPU-side twin of
/// `positron_ratatui::render_to_buffer`: one call, state in, [`RgbaFrame`] out.
pub fn render_to_rgba<S, R>(gpu: &Gpu, renderer: &R, state: &S) -> Result<RgbaFrame, GpuError>
where
    S: ViewState,
    R: Renderer<S, Output = Frame>,
{
    gpu.rasterize(&renderer.render(state))
}

#[cfg(test)]
mod tests {
    use super::*;

    // Outlier-B fixture: a counter rendered as GPU geometry — one colored quad
    // per unit of magnitude, NOT a text tree. This is the point of the crate.
    #[derive(Debug, Clone)]
    struct Counter {
        value: i64,
    }

    impl ViewState for Counter {
        fn kind(&self) -> &'static str {
            "counter"
        }
    }

    const CANVAS: (u32, u32) = (320, 64);
    const CELL: f32 = 24.0;
    const GAP: f32 = 8.0;
    const BG: Rgba = Rgba::rgb(0.05, 0.05, 0.08);
    const POSITIVE: Rgba = Rgba::rgb(0.0, 1.0, 0.0);
    const NEGATIVE: Rgba = Rgba::rgb(1.0, 0.0, 0.0);

    struct CounterRenderer;
    impl Renderer<Counter> for CounterRenderer {
        // The whole reason this crate exists: Output is a GPU Frame, not a String.
        type Output = Frame;
        fn render(&self, state: &Counter) -> Frame {
            let color = if state.value >= 0 { POSITIVE } else { NEGATIVE };
            let mut frame = Frame::new(CANVAS.0, CANVAS.1, BG);
            for i in 0..state.value.unsigned_abs() {
                let x = GAP + i as f32 * (CELL + GAP);
                frame = frame.with_quad(Rect::new(x, GAP, CELL, CELL), color);
            }
            frame
        }
    }

    // what this catches: a Renderer whose Output is GPU geometry (not text)
    // projects a ViewState into the expected quads — magnitude → count, sign →
    // color — with no GPU involved. This is outlier B's headless half: proof the
    // contract carries a non-text surface. The rasterization half is the example.
    #[test]
    fn renderer_projects_view_state_into_gpu_quads() {
        let frame = CounterRenderer.render(&Counter { value: 3 });
        assert_eq!(frame.width, CANVAS.0);
        assert_eq!(frame.clear, BG);
        assert_eq!(frame.primitives.len(), 3, "magnitude 3 → three quads");
        assert!(
            frame
                .primitives
                .iter()
                .all(|p| matches!(p, Primitive::Quad { color, .. } if *color == POSITIVE)),
            "positive value → green quads",
        );

        // Negative magnitude → same count, negative color.
        let neg = CounterRenderer.render(&Counter { value: -2 });
        assert_eq!(neg.primitives.len(), 2);
        assert!(matches!(
            neg.primitives[0],
            Primitive::Quad { color, .. } if color == NEGATIVE
        ));

        // Zero → nothing to draw but a cleared frame.
        assert!(CounterRenderer
            .render(&Counter { value: 0 })
            .primitives
            .is_empty());
    }

    // what this catches: quads are laid out left-to-right without overlap — the
    // Nth quad starts one cell+gap past the (N-1)th. If the stride math drifts,
    // the rasterized bars would overlap or drift off-canvas.
    #[test]
    fn quads_lay_out_left_to_right_without_overlap() {
        let frame = CounterRenderer.render(&Counter { value: 3 });
        let xs: Vec<f32> = frame
            .primitives
            .iter()
            .map(|Primitive::Quad { rect, .. }| rect.x)
            .collect();
        assert_eq!(xs, vec![GAP, GAP + (CELL + GAP), GAP + 2.0 * (CELL + GAP)]);
    }
}
