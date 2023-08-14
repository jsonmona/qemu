use std::sync::atomic::Ordering::*;
use std::sync::{atomic::AtomicBool, Arc};

use wgpu::*;

use super::device::GraphicDevice;

pub struct GraphicCompositor {
    tex_framebuffer: Texture,
    tex_output: Texture,
    buf_output_staging: Buffer,
}

impl GraphicCompositor {
    pub async fn new(dev: &mut GraphicDevice) -> Self {
        let tex_framebuffer = dev.device.create_texture(&TextureDescriptor {
            label: Some("tex_framebuffer"),
            size: Extent3d {
                width: dev.width,
                height: dev.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: TextureFormat::Bgra8Unorm,
            usage: TextureUsages::COPY_SRC | TextureUsages::COPY_DST,
            view_formats: &[TextureFormat::Bgra8Unorm],
        });

        let tex_output = dev.device.create_texture(&TextureDescriptor {
            label: Some("tex_output"),
            size: Extent3d {
                width: dev.width,
                height: dev.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: TextureFormat::Bgra8Unorm,
            usage: TextureUsages::COPY_SRC
                | TextureUsages::COPY_DST
                | TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[TextureFormat::Bgra8Unorm],
        });

        // Align width to 256 bytes
        let linesize = align_value(dev.width, 64) * 4;

        let buf_output_staging = dev.device.create_buffer(&BufferDescriptor {
            label: Some("buf_output_staging"),
            size: (dev.height * linesize) as u64,
            usage: BufferUsages::MAP_READ | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        GraphicCompositor {
            tex_framebuffer,
            tex_output,
            buf_output_staging,
        }
    }

    pub fn cmd_update_framebuffer_whole(&mut self, dev: &mut GraphicDevice, data: &[u32]) {
        assert_eq!(
            dev.width * dev.height,
            data.len() as u32,
            "image size mismatch"
        );

        dev.queue.write_texture(
            ImageCopyTexture {
                texture: &self.tex_framebuffer,
                mip_level: 0,
                origin: Origin3d::ZERO,
                aspect: TextureAspect::All,
            },
            bytemuck::cast_slice(data),
            ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(dev.width * 4),
                rows_per_image: Some(dev.height),
            },
            Extent3d {
                width: dev.width,
                height: dev.height,
                depth_or_array_layers: 1,
            },
        );
    }

    pub async fn render(&mut self, state: &mut GraphicDevice, output: &mut [u8]) {
        self.composite(state);

        let linesize = align_value(state.width, 64) * 4;

        state.encoder.copy_texture_to_buffer(
            ImageCopyTexture {
                texture: &self.tex_output,
                mip_level: 0,
                origin: Origin3d::ZERO,
                aspect: TextureAspect::All,
            },
            ImageCopyBuffer {
                buffer: &self.buf_output_staging,
                layout: ImageDataLayout {
                    offset: 0,
                    bytes_per_row: Some(linesize),
                    rows_per_image: Some(state.height),
                },
            },
            Extent3d {
                width: state.width,
                height: state.height,
                depth_or_array_layers: 1,
            },
        );

        let mut alt_encoder = state
            .device
            .create_command_encoder(&CommandEncoderDescriptor {
                label: Some("encoder"),
            });

        std::mem::swap(&mut state.encoder, &mut alt_encoder);

        state.queue.submit(std::iter::once(alt_encoder.finish()));

        let success = Arc::new(AtomicBool::new(false));
        let s_clone = Arc::clone(&success); // unergonomic :(

        let slice = self.buf_output_staging.slice(..);
        slice.map_async(MapMode::Read, move |x| {
            x.unwrap();
            s_clone.store(true, Relaxed);
        });

        state.device.poll(MaintainBase::Wait);

        // OK. Stored in same thread
        assert!(success.load(Relaxed), "buffer not mapped");

        let view = slice.get_mapped_range();

        for y in 0..state.height {
            let len = (state.width * 4) as usize;

            let src_begin = (linesize * y) as usize;
            let src_end = src_begin + len;
            let dst_begin = (state.width * 4 * y) as usize;
            let dst_end = dst_begin + len;

            let src_line = &view[src_begin..src_end];
            let dst_line = &mut output[dst_begin..dst_end];

            dst_line.copy_from_slice(src_line);
        }

        drop(view);
        self.buf_output_staging.unmap();
    }

    fn composite(&mut self, dev: &mut GraphicDevice) {
        dev.encoder.copy_texture_to_texture(
            ImageCopyTexture {
                texture: &self.tex_framebuffer,
                mip_level: 0,
                origin: Origin3d::ZERO,
                aspect: TextureAspect::All,
            },
            ImageCopyTexture {
                texture: &self.tex_output,
                mip_level: 0,
                origin: Origin3d::ZERO,
                aspect: TextureAspect::All,
            },
            Extent3d {
                width: dev.width,
                height: dev.height,
                depth_or_array_layers: 1,
            },
        );

        let view_output = self.tex_output.create_view(&Default::default());

        let _pass = dev.encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("composite render pass"),
            color_attachments: &[Some(RenderPassColorAttachment {
                view: &view_output,
                resolve_target: None,
                ops: Operations {
                    load: LoadOp::Load,
                    store: true,
                },
            })],
            depth_stencil_attachment: None,
        });
    }
}

fn align_value(x: u32, alignment: u32) -> u32 {
    let r = x % alignment;

    if r == 0 {
        x
    } else {
        x + alignment - r
    }
}
