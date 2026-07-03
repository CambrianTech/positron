//! Rasterize the same `Counter` `ViewState` the crate's tests project, but for
//! real: bring up a GPU, render each state offscreen via wgpu, and write the
//! pixels to PNG. This is the "define once, project many" thesis on the GPU
//! surface — the identical `Renderer` that a `TestBackend`-style unit test
//! asserts against here drives actual hardware.
//!
//! Run: `cargo run -p positron-wgpu --example counter_gpu`
//! Output: `./positron-wgpu-frames/counter_<n>.png`

use positron_core::{Renderer, ViewState};
use positron_wgpu::{render_to_rgba, Frame, Gpu, Rect, Rgba};

#[derive(Debug, Clone)]
struct Counter {
    value: i64,
}

impl ViewState for Counter {
    fn kind(&self) -> &'static str {
        "counter"
    }
}

struct CounterRenderer;
impl Renderer<Counter> for CounterRenderer {
    type Output = Frame;
    fn render(&self, state: &Counter) -> Frame {
        let color = if state.value >= 0 {
            Rgba::rgb(0.0, 1.0, 0.0)
        } else {
            Rgba::rgb(1.0, 0.0, 0.0)
        };
        let mut frame = Frame::new(320, 64, Rgba::rgb(0.05, 0.05, 0.08));
        for i in 0..state.value.unsigned_abs() {
            let x = 8.0 + i as f32 * 32.0;
            frame = frame.with_quad(Rect::new(x, 8.0, 24.0, 24.0), color);
        }
        frame
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let gpu = Gpu::headless()?;
    let out_dir = std::path::Path::new("positron-wgpu-frames");
    std::fs::create_dir_all(out_dir)?;

    for value in [0i64, 3, 7, -4] {
        let rgba = render_to_rgba(&gpu, &CounterRenderer, &Counter { value })?;
        let name = format!("counter_{value}.png");
        let path = out_dir.join(&name);
        image::save_buffer(
            &path,
            &rgba.pixels,
            rgba.width,
            rgba.height,
            image::ColorType::Rgba8,
        )?;
        println!("wrote {} ({}x{})", path.display(), rgba.width, rgba.height);
    }

    println!("\nGPU rasterization complete — {} frames.", 4);
    Ok(())
}
