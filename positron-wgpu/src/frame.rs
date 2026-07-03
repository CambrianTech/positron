//! The GPU renderer's data types — the `type Output` a [`Renderer`] projects a
//! [`ViewState`] into, and the pixels a GPU rasterizes it to.
//!
//! [`Frame`] is **pure data**: no `wgpu`, no device, no I/O. That is the whole
//! point of outlier B — a [`Renderer`](positron_core::Renderer)'s output here is
//! GPU-bound *geometry* (colored quads destined for a vertex buffer), not a CPU
//! text tree like `counter-cli`'s `String` or `positron-ratatui`'s `Paragraph`.
//! Projecting state into a `Frame` needs no GPU, so it is unit-tested headlessly;
//! only turning a `Frame` into an [`RgbaFrame`] touches the hardware.
//!
//! [`RgbaFrame`] is intentionally the same shape as continuum's avatar-renderer
//! output (`width`/`height`/tightly-packed RGBA8 `Vec<u8>`) so the two reconcile
//! at O5 without either side reshaping its frame type.

/// A linear RGBA color, each channel in `0.0..=1.0`. Kept as `f32` because that
/// is what the GPU clear value and vertex colors want; the rasterized
/// [`RgbaFrame`] is where it becomes `u8`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rgba {
    /// Red, `0.0..=1.0`.
    pub r: f32,
    /// Green, `0.0..=1.0`.
    pub g: f32,
    /// Blue, `0.0..=1.0`.
    pub b: f32,
    /// Alpha, `0.0..=1.0`.
    pub a: f32,
}

impl Rgba {
    /// Opaque color from RGB channels (alpha = 1.0).
    pub const fn rgb(r: f32, g: f32, b: f32) -> Self {
        Self { r, g, b, a: 1.0 }
    }

    /// The four channels as an array, for upload to the GPU.
    pub const fn to_array(self) -> [f32; 4] {
        [self.r, self.g, self.b, self.a]
    }
}

/// An axis-aligned rectangle in **pixel space**: origin top-left, `+x` right,
/// `+y` down — the coordinate system the substrate thinks in. The rasterizer is
/// the one place this is mapped to GPU normalized-device coordinates, so nothing
/// upstream carries an NDC assumption.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    /// Left edge, pixels from the frame's left.
    pub x: f32,
    /// Top edge, pixels from the frame's top.
    pub y: f32,
    /// Width in pixels.
    pub w: f32,
    /// Height in pixels.
    pub h: f32,
}

impl Rect {
    /// A rectangle from its top-left corner and size.
    pub const fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }
}

/// One thing to draw. A closed set of GPU-native primitives: today just a
/// colored quad (two triangles). New surfaces (textured quads, glyph runs) are
/// new variants — the renderer projects into them, the rasterizer learns to
/// draw them, and nothing else changes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Primitive {
    /// A solid-colored rectangle.
    Quad {
        /// Where it sits, in pixel space.
        rect: Rect,
        /// Its fill color.
        color: Rgba,
    },
}

/// A complete GPU draw description: the surface size, a background clear color,
/// and the primitives to draw over it. This is a [`Renderer`](positron_core::Renderer)'s
/// `type Output` for the wgpu surface — pure data, GPU-shaped, no hardware
/// touched until [`crate::Gpu::rasterize`] consumes it.
#[derive(Debug, Clone, PartialEq)]
pub struct Frame {
    /// Target width in pixels.
    pub width: u32,
    /// Target height in pixels.
    pub height: u32,
    /// The color the frame is cleared to before primitives are drawn.
    pub clear: Rgba,
    /// The primitives, drawn in order (later ones over earlier ones).
    pub primitives: Vec<Primitive>,
}

impl Frame {
    /// An empty frame of the given size, cleared to `clear`, no primitives.
    pub fn new(width: u32, height: u32, clear: Rgba) -> Self {
        Self {
            width,
            height,
            clear,
            primitives: Vec::new(),
        }
    }

    /// Push a colored quad and return `self`, for fluent construction.
    #[must_use]
    pub fn with_quad(mut self, rect: Rect, color: Rgba) -> Self {
        self.primitives.push(Primitive::Quad { rect, color });
        self
    }
}

/// Rasterized output: a tightly-packed RGBA8 pixel buffer. `pixels.len()` is
/// always `width * height * 4`. Same shape as continuum's avatar `RgbaFrame`, so
/// LiveKit / PNG consumers (and the O5 reconcile) need no adapter.
#[derive(Debug, Clone, PartialEq)]
pub struct RgbaFrame {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Row-major RGBA8, `width * height * 4` bytes, no row padding.
    pub pixels: Vec<u8>,
}

impl RgbaFrame {
    /// The RGBA8 pixel at `(x, y)`, or `None` if out of bounds. Reading a corner
    /// or a known quad center is how the GPU tests assert without a golden image.
    pub fn pixel(&self, x: u32, y: u32) -> Option<[u8; 4]> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let i = ((y * self.width + x) * 4) as usize;
        Some([
            self.pixels[i],
            self.pixels[i + 1],
            self.pixels[i + 2],
            self.pixels[i + 3],
        ])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the pure builder produces exactly the geometry asked
    // for, in order — the projection half of outlier B that needs no GPU. If
    // this drifts, every rasterized frame is wrong before the hardware is even
    // involved.
    #[test]
    fn frame_builder_accumulates_quads_in_order() {
        let red = Rgba::rgb(1.0, 0.0, 0.0);
        let blue = Rgba::rgb(0.0, 0.0, 1.0);
        let frame = Frame::new(64, 32, Rgba::rgb(0.0, 0.0, 0.0))
            .with_quad(Rect::new(0.0, 0.0, 8.0, 8.0), red)
            .with_quad(Rect::new(8.0, 0.0, 8.0, 8.0), blue);

        assert_eq!(frame.width, 64);
        assert_eq!(frame.height, 32);
        assert_eq!(frame.clear, Rgba::rgb(0.0, 0.0, 0.0));
        assert_eq!(
            frame.primitives,
            vec![
                Primitive::Quad {
                    rect: Rect::new(0.0, 0.0, 8.0, 8.0),
                    color: red,
                },
                Primitive::Quad {
                    rect: Rect::new(8.0, 0.0, 8.0, 8.0),
                    color: blue,
                },
            ]
        );
    }

    // what this catches: RgbaFrame::pixel indexes row-major RGBA8 correctly and
    // bounds-checks — the readback accessor the GPU tests trust. An off-by-one
    // here would make a green rasterization assert as passing against garbage.
    #[test]
    fn rgba_frame_indexes_row_major_and_bounds_checks() {
        // 2x2, second pixel of row 1 is green: pixel index (1*2 + 1) = 3.
        let mut pixels = vec![0u8; 2 * 2 * 4];
        let i = 3 * 4;
        pixels[i + 1] = 255;
        pixels[i + 3] = 255;
        let frame = RgbaFrame {
            width: 2,
            height: 2,
            pixels,
        };

        assert_eq!(frame.pixel(0, 0), Some([0, 0, 0, 0]));
        assert_eq!(frame.pixel(1, 1), Some([0, 255, 0, 255]));
        assert_eq!(frame.pixel(2, 0), None);
        assert_eq!(frame.pixel(0, 2), None);
    }
}
