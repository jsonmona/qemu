use anyhow::{Context, Result};
use wgpu::*;

pub struct GraphicDevice {
    pub width: u32,
    pub height: u32,

    pub instance: Instance,
    pub adapter: Adapter,
    pub device: Device,
    pub queue: Queue,
    pub encoder: CommandEncoder,
}

impl GraphicDevice {
    pub async fn new(w: u32, h: u32) -> Result<Self> {
        let instance = wgpu::Instance::new(InstanceDescriptor {
            backends: Backends::PRIMARY,
            dx12_shader_compiler: Dx12Compiler::Fxc,
        });

        let adapter = instance
            .request_adapter(&RequestAdapterOptions {
                power_preference: PowerPreference::HighPerformance,
                force_fallback_adapter: false,
                compatible_surface: None,
            })
            .await
            .context("requesting adapter")?;

        let (device, queue) = adapter
            .request_device(
                &DeviceDescriptor {
                    label: Some("primary"),
                    features: Features::empty(),
                    limits: Limits::downlevel_defaults(),
                },
                None,
            )
            .await?;

        let encoder = device.create_command_encoder(&CommandEncoderDescriptor {
            label: Some("encoder"),
        });

        Ok(GraphicDevice {
            width: w,
            height: h,
            instance,
            adapter,
            device,
            queue,
            encoder,
        })
    }
}
