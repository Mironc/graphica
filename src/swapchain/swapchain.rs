use std::{error::Error, sync::Arc};

use ash::{
    khr::swapchain,
    vk::{
        self, Extent2D, Fence, FenceCreateFlags, FenceCreateInfo, Format, Image, ImageView,
        SwapchainKHR,
    },
};
use winit::window::Window;

use crate::{
    context::GraphicsContext,
    device::DeviceContext,
    queue::logical_queue::Queue,
    swapchain::{FrameImage, ImageSync, frame_data::FrameData, frame_sync::FrameSync},
};

///Hardcoded count of frames in flight
const FIF_COUNT: usize = 2;
pub struct SwapChain {
    device: swapchain::Device,
    device_context: Arc<DeviceContext>,
    swapchain_khr: SwapchainKHR,
    current_frame: usize,
    syncs: Vec<FrameSync>,
    frames: Vec<FrameImage>,
}
impl SwapChain {
    pub fn new(
        context: &Arc<GraphicsContext>,
        device_context: &Arc<DeviceContext>,
        window: &Window,
    ) -> Result<Self, Box<dyn Error>> {
        Self::create_swapchain(context, device_context, window, None)
    }
    fn create_swapchain(
        context: &Arc<GraphicsContext>,
        device_context: &Arc<DeviceContext>,
        window: &Window,
        previous: Option<&Self>,
    ) -> Result<Self, Box<dyn Error>> {
        let formats = device_context.pdevice().surface_formats();
        let format_khr = {
            if formats.len() == 1 && formats[0].format == vk::Format::UNDEFINED {
                vk::SurfaceFormatKHR {
                    format: vk::Format::B8G8R8A8_UNORM,
                    color_space: vk::ColorSpaceKHR::SRGB_NONLINEAR,
                }
            } else {
                *formats
                    .iter()
                    .find(|format| {
                        format.format == vk::Format::B8G8R8A8_UNORM
                            && format.color_space == vk::ColorSpaceKHR::SRGB_NONLINEAR
                    })
                    .unwrap_or(&formats[0])
            }
        };

        let present_modes = device_context.pdevice().surface_present_modes();
        let present_mode = if present_modes.contains(&vk::PresentModeKHR::IMMEDIATE) {
            vk::PresentModeKHR::IMMEDIATE
        } else {
            vk::PresentModeKHR::FIFO
        };

        let capabilities = device_context.pdevice().surface_capabilities();
        let extent2d = {
            if capabilities.current_extent.width != u32::MAX {
                capabilities.current_extent
            } else {
                let min = capabilities.min_image_extent;
                let max = capabilities.max_image_extent;
                let width = window.inner_size().width.min(max.width).max(min.width);
                let height = window.inner_size().height.min(max.height).max(min.height);
                Extent2D { width, height }
            }
        };

        let image_count = capabilities.min_image_count.max(3);
        log::debug!(
            "Swapchain format: {:?} present mode: {:?}  extent: {:?} image count: {:?}",
            format_khr,
            present_mode,
            extent2d,
            image_count
        );
        let queue_families = [
            device_context
                .render_queue()
                .graphics_queue()
                .queue_family()
                .id() as u32,
            device_context
                .render_queue()
                .present_queue()
                .queue_family()
                .id() as u32,
        ];
        let create_info = {
            let mut builder = vk::SwapchainCreateInfoKHR::default()
                .surface(context.instance().khr_surface())
                .min_image_count(image_count)
                .image_format(format_khr.format)
                .image_color_space(format_khr.color_space)
                .image_extent(extent2d)
                .image_array_layers(1)
                .image_usage(
                    vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::TRANSFER_DST,
                );
            if let Some(previous) = previous {
                builder = builder.old_swapchain(previous.swapchain_khr);
            }
            builder = if device_context.render_queue().shared() {
                builder.image_sharing_mode(vk::SharingMode::EXCLUSIVE)
            } else {
                builder
                    .image_sharing_mode(vk::SharingMode::CONCURRENT)
                    .queue_family_indices(&queue_families)
            };

            builder
                .pre_transform(capabilities.current_transform)
                .composite_alpha(vk::CompositeAlphaFlagsKHR::OPAQUE)
                .present_mode(present_mode)
                .clipped(true)
        };
        let sdevice = swapchain::Device::new(context.instance(), device_context);
        let swapchain_khr = unsafe { sdevice.create_swapchain(&create_info, None) }?;
        let images = unsafe { sdevice.get_swapchain_images(swapchain_khr)? };
        let image_views = images
            .iter()
            .map(|image| {
                let create_info = vk::ImageViewCreateInfo::default()
                    .image(*image)
                    .view_type(vk::ImageViewType::TYPE_2D)
                    .format(format_khr.format)
                    .subresource_range(vk::ImageSubresourceRange {
                        aspect_mask: vk::ImageAspectFlags::COLOR,
                        base_mip_level: 0,
                        level_count: 1,
                        base_array_layer: 0,
                        layer_count: 1,
                    });

                unsafe { device_context.create_image_view(&create_info, None) }
            })
            .collect::<Result<Vec<_>, _>>()?;

        let syncs = (0..FIF_COUNT)
            .map(|_| FrameSync::new(device_context))
            .collect::<Vec<FrameSync>>();
        let frames = (0..image_count as usize)
            .map(|i| {
                FrameImage::new(
                    i as u32,
                    images[i],
                    extent2d,
                    image_views[i],
                    format_khr.format,
                    ImageSync::new(device_context),
                )
            })
            .collect::<Vec<FrameImage>>();

        Ok(Self {
            device: sdevice,
            swapchain_khr,
            device_context: device_context.clone(),
            current_frame: 0,
            frames,
            syncs,
        })
    }

    ///Recreates swapchain
    pub fn recreate(
        &mut self,
        context: &Arc<GraphicsContext>,
        device_context: &Arc<DeviceContext>,
        window: &Window,
    ) -> Result<Self, Box<dyn Error>> {
        unsafe {
            device_context
                .device_wait_idle()
                .expect("Panic on idle wait");
            for frame in self.frames.iter() {
                self.device_context
                    .destroy_image_view(frame.image_view(), None);
                device_context.destroy_semaphore(frame.image_sync().render_finished(), None);
            }
            self.syncs.iter().for_each(|x| x.destroy(device_context));
            let swapchain_khr_handle = self.swapchain_khr;
            let new_swapchain =
                Self::create_swapchain(context, device_context, window, Some(self))?;
            new_swapchain
                .device
                .destroy_swapchain(swapchain_khr_handle, None);
            Ok(new_swapchain)
        }
    }
    pub fn size(&self) -> Extent2D {
        self.frames[0].extent()
    }
    ///This function is designed to be called once per frame
    ///
    ///It may block thread, if swapchain has no free images and previous frames are not dropped via present_frame()
    pub fn next_frame(&mut self, device_context: &DeviceContext) -> FrameData {
        let current_sync = self.syncs[self.current_frame].clone();
        current_sync.wait(device_context);

        let image_id = unsafe {
            let res = self
                .device
                .acquire_next_image(
                    self.swapchain_khr,
                    u64::MAX,
                    current_sync.image_available(),
                    Fence::null(),
                )
                .expect("Couldn't acquire next image");
            res.0
        };
        self.current_frame = (self.current_frame + 1) % FIF_COUNT;
        current_sync.clear(device_context);
        FrameData::new(
            self.current_frame,
            current_sync,
            self.frames[image_id as usize],
        )
    }
    ///This call presents frame on the screen with framedata
    pub fn present_frame(
        &self,
        present_queue: &Queue,
        frame_data: FrameData,
    ) -> Result<(), Box<dyn Error>> {
        let image_indices = [frame_data.image().image_id()];
        let swapchain_khr = [self.swapchain_khr];
        let render_finished = [frame_data.image().image_sync().render_finished()];
        let present_info = vk::PresentInfoKHR::default()
            .wait_semaphores(&render_finished)
            .swapchains(&swapchain_khr)
            .image_indices(&image_indices);

        unsafe {
            self.device
                .queue_present(present_queue.handle(), &present_info)?
        };
        Ok(())
    }

    pub fn frames(&self) -> &[FrameImage] {
        &self.frames
    }
}
impl Drop for SwapChain {
    fn drop(&mut self) {}
}
