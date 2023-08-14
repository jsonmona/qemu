use super::{compositor::GraphicCompositor, device::GraphicDevice};

pub struct GraphicState {
    device: GraphicDevice,
    compositor: GraphicCompositor,
}

impl GraphicState {
    pub async fn new(w: u32, h: u32) -> Self {
        let mut device = GraphicDevice::new(w, h).await.unwrap();
        let compositor = GraphicCompositor::new(&mut device).await;

        GraphicState { device, compositor }
    }

    pub fn width(&self) -> u32 {
        self.device.width
    }

    pub fn height(&self) -> u32 {
        self.device.height
    }

    pub fn cmd_update_framebuffer_whole(&mut self, data: &[u32]) {
        self.compositor
            .cmd_update_framebuffer_whole(&mut self.device, data);
    }

    pub async fn render(&mut self, output: &mut [u8]) {
        self.compositor.render(&mut self.device, output).await;
    }
}
