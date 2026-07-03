//! [`Gpu`] — the one place that touches wgpu. It turns a pure [`Frame`] of
//! colored quads into an [`RgbaFrame`] by rendering offscreen and reading the
//! pixels back. No window, no surface: this is the "render anywhere" path — the
//! same code drives Metal (macOS), Vulkan (Linux), and DX12 (Windows), and the
//! very same crate compiles to WebGPU under WASM.
//!
//! It is deliberately kept out of the `#[test]` surface: CI runners have no GPU
//! adapter, so the hardware path is proven by the `counter_gpu` example (which
//! runs on any real machine), while the pure [`Frame`] projection is what the
//! unit tests assert. Missing hardware is a **loud, named** [`GpuError::NoAdapter`],
//! never a silent software fallback.

use std::error::Error;
use std::fmt;

use wgpu::util::DeviceExt;

use crate::frame::{Frame, Primitive, RgbaFrame};

/// The minimal colored-quad shader: pass NDC position + vertex color straight
/// through. Inline (not an `include_str!`) so there is no on-disk asset to
/// resolve relative to a CWD.
const SHADER: &str = r"
struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) color: vec4<f32>,
};

@vertex
fn vs_main(@location(0) pos: vec2<f32>, @location(1) color: vec4<f32>) -> VsOut {
    var out: VsOut;
    out.pos = vec4<f32>(pos, 0.0, 1.0);
    out.color = color;
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    return in.color;
}
";

/// Linear texture format so a rasterized channel reads back as exactly
/// `round(channel * 255)` — no sRGB curve to reason about when asserting pixels.
const TARGET_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

/// wgpu requires each copied texture row to be a multiple of this many bytes.
const COPY_ALIGN: u32 = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Vertex {
    pos: [f32; 2],
    color: [f32; 4],
}

/// What can go wrong bringing up or driving the GPU. Every variant names the
/// cause — there is no fallback path to hide behind.
#[derive(Debug)]
pub enum GpuError {
    /// No GPU adapter was available (e.g. a headless CI runner). Loud on
    /// purpose: the caller decides, we never quietly software-render.
    NoAdapter,
    /// The adapter refused a device with our (downlevel) requirements.
    RequestDevice(wgpu::RequestDeviceError),
    /// Mapping the readback buffer failed.
    BufferMap(wgpu::BufferAsyncError),
}

impl fmt::Display for GpuError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GpuError::NoAdapter => write!(
                f,
                "no GPU adapter available (headless environment?) — positron-wgpu does not software-fallback"
            ),
            GpuError::RequestDevice(e) => write!(f, "GPU device request failed: {e}"),
            GpuError::BufferMap(e) => write!(f, "GPU readback buffer map failed: {e}"),
        }
    }
}

impl Error for GpuError {}

/// A ready GPU device + queue, reusable across many [`rasterize`](Gpu::rasterize)
/// calls. Construction is the expensive part (adapter + device); hold one and
/// render many frames through it.
pub struct Gpu {
    device: wgpu::Device,
    queue: wgpu::Queue,
}

impl Gpu {
    /// Bring up a headless GPU: pick an adapter, request a device with
    /// downlevel-default limits (so it runs on modest hardware and under WebGPU).
    /// Fails loud with [`GpuError::NoAdapter`] when there is no GPU at all.
    pub fn headless() -> Result<Self, GpuError> {
        let instance = wgpu::Instance::default();
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            force_fallback_adapter: false,
            compatible_surface: None,
        }))
        .ok_or(GpuError::NoAdapter)?;

        let (device, queue) = pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("positron-wgpu device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::downlevel_defaults(),
                memory_hints: wgpu::MemoryHints::default(),
            },
            None,
        ))
        .map_err(GpuError::RequestDevice)?;

        Ok(Self { device, queue })
    }

    /// Render `frame` offscreen and read the pixels back as an [`RgbaFrame`].
    /// Clears to `frame.clear`, then draws each [`Primitive::Quad`] as two
    /// triangles in pixel space.
    pub fn rasterize(&self, frame: &Frame) -> Result<RgbaFrame, GpuError> {
        let (width, height) = (frame.width, frame.height);
        let vertices = quads_to_vertices(frame);

        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("positron-wgpu target"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: TARGET_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        let pipeline = self.build_pipeline();
        let vertex_buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("positron-wgpu vertices"),
                contents: bytemuck::cast_slice(&vertices),
                usage: wgpu::BufferUsages::VERTEX,
            });

        // Readback needs each row padded to COPY_ALIGN; we strip the padding
        // after mapping so the returned RgbaFrame is tightly packed.
        let unpadded_bytes_per_row = width * 4;
        let padded_bytes_per_row = padded_row(unpadded_bytes_per_row);
        let output_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("positron-wgpu readback"),
            size: (padded_bytes_per_row * height) as u64,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("positron-wgpu encoder"),
            });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("positron-wgpu pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(clear_color(frame)),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            if !vertices.is_empty() {
                pass.set_pipeline(&pipeline);
                pass.set_vertex_buffer(0, vertex_buffer.slice(..));
                pass.draw(0..vertices.len() as u32, 0..1);
            }
        }

        encoder.copy_texture_to_buffer(
            wgpu::ImageCopyTexture {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::ImageCopyBuffer {
                buffer: &output_buffer,
                layout: wgpu::ImageDataLayout {
                    offset: 0,
                    bytes_per_row: Some(padded_bytes_per_row),
                    rows_per_image: Some(height),
                },
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        self.queue.submit(Some(encoder.finish()));

        self.read_back(&output_buffer, width, height, padded_bytes_per_row)
    }

    fn build_pipeline(&self) -> wgpu::RenderPipeline {
        let shader = self
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("positron-wgpu shader"),
                source: wgpu::ShaderSource::Wgsl(SHADER.into()),
            });
        let layout = self
            .device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("positron-wgpu layout"),
                bind_group_layouts: &[],
                push_constant_ranges: &[],
            });
        self.device
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("positron-wgpu pipeline"),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: "vs_main",
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    buffers: &[wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<Vertex>() as u64,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x4],
                    }],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: "fs_main",
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: TARGET_FORMAT,
                        blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    ..Default::default()
                },
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                multiview: None,
                cache: None,
            })
    }

    fn read_back(
        &self,
        output_buffer: &wgpu::Buffer,
        width: u32,
        height: u32,
        padded_bytes_per_row: u32,
    ) -> Result<RgbaFrame, GpuError> {
        let slice = output_buffer.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            // Send can only fail if the receiver was dropped; we wait on it below.
            let _ = tx.send(result);
        });
        self.device.poll(wgpu::Maintain::Wait);
        rx.recv()
            .expect("map_async callback never fired")
            .map_err(GpuError::BufferMap)?;

        let unpadded_bytes_per_row = (width * 4) as usize;
        let padded = padded_bytes_per_row as usize;
        let mapped = slice.get_mapped_range();
        let mut pixels = Vec::with_capacity(unpadded_bytes_per_row * height as usize);
        for row in 0..height as usize {
            let start = row * padded;
            pixels.extend_from_slice(&mapped[start..start + unpadded_bytes_per_row]);
        }
        drop(mapped);
        output_buffer.unmap();

        Ok(RgbaFrame {
            width,
            height,
            pixels,
        })
    }
}

/// Map a pixel-space [`Frame`] of quads to a flat triangle-list vertex buffer in
/// normalized device coordinates (`-1..1`, `+y` up — so pixel `y` is flipped).
fn quads_to_vertices(frame: &Frame) -> Vec<Vertex> {
    let (fw, fh) = (frame.width as f32, frame.height as f32);
    let ndc = |x: f32, y: f32| [x / fw * 2.0 - 1.0, 1.0 - y / fh * 2.0];

    let mut vertices = Vec::with_capacity(frame.primitives.len() * 6);
    for Primitive::Quad { rect, color } in &frame.primitives {
        let c = color.to_array();
        let tl = ndc(rect.x, rect.y);
        let tr = ndc(rect.x + rect.w, rect.y);
        let br = ndc(rect.x + rect.w, rect.y + rect.h);
        let bl = ndc(rect.x, rect.y + rect.h);
        for pos in [tl, tr, br, tl, br, bl] {
            vertices.push(Vertex { pos, color: c });
        }
    }
    vertices
}

fn clear_color(frame: &Frame) -> wgpu::Color {
    wgpu::Color {
        r: frame.clear.r as f64,
        g: frame.clear.g as f64,
        b: frame.clear.b as f64,
        a: frame.clear.a as f64,
    }
}

/// Round `unpadded` up to the next multiple of [`COPY_ALIGN`].
fn padded_row(unpadded: u32) -> u32 {
    unpadded.div_ceil(COPY_ALIGN) * COPY_ALIGN
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame::{Rect, Rgba};

    // what this catches: the pixel→NDC mapping and triangle winding that the GPU
    // path depends on but can't assert without hardware. A quad at the frame's
    // top-left corner must map to NDC top-left (-1, +1); a full-frame quad must
    // span the whole clip cube. Get this wrong and every rasterized frame is
    // mirrored or offset — invisibly, since CI never runs the GPU.
    #[test]
    fn quads_map_pixel_space_to_ndc_with_flipped_y() {
        let frame = Frame::new(100, 100, Rgba::rgb(0.0, 0.0, 0.0))
            .with_quad(Rect::new(0.0, 0.0, 100.0, 100.0), Rgba::rgb(1.0, 1.0, 1.0));
        let verts = quads_to_vertices(&frame);

        assert_eq!(verts.len(), 6, "one quad = two triangles = six vertices");
        // First vertex is the top-left corner: pixel (0,0) → NDC (-1, +1).
        assert_eq!(verts[0].pos, [-1.0, 1.0]);
        // Third vertex is bottom-right: pixel (100,100) → NDC (+1, -1).
        assert_eq!(verts[2].pos, [1.0, -1.0]);
        // Color rides through unchanged.
        assert_eq!(verts[0].color, [1.0, 1.0, 1.0, 1.0]);
    }

    // what this catches: row padding math — readback copies rows padded to
    // COPY_ALIGN (256). 64px * 4 = 256 is already aligned; 65px * 4 = 260 must
    // round up to 512. An off-by-one here corrupts every row of every readback.
    #[test]
    fn padded_row_rounds_up_to_copy_alignment() {
        assert_eq!(COPY_ALIGN, 256);
        assert_eq!(padded_row(256), 256);
        assert_eq!(padded_row(260), 512);
        assert_eq!(padded_row(1), 256);
    }
}
