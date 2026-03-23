use ash::vk::{Extent2D, Format, Image, ImageView};

use crate::swapchain::{ImageSync, frame_sync::FrameSync};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrameData {
    fif_id: usize,
    sync: FrameSync,
    image: FrameImage,
}
impl FrameData {
    pub fn new(fif_id: usize, sync: FrameSync, image: FrameImage) -> Self {
        Self {
            fif_id,
            sync,
            image,
        }
    }

    pub fn fif_id(&self) -> usize {
        self.fif_id
    }

    pub fn sync(&self) -> &FrameSync {
        &self.sync
    }

    pub fn image(&self) -> &FrameImage {
        &self.image
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FrameImage {
    image_id: u32,
    image: Image,
    extent: Extent2D,
    format: Format,
    image_view: ImageView,
    image_sync: ImageSync,
}

impl FrameImage {
    pub fn new(
        image_id: u32,
        image: Image,
        extent: Extent2D,
        image_view: ImageView,
        format: Format,
        image_sync: ImageSync,
    ) -> Self {
        Self {
            image_id,
            image,
            extent,
            image_view,
            format,
            image_sync,
        }
    }

    pub fn image(&self) -> Image {
        self.image
    }

    pub fn format(&self) -> Format {
        self.format
    }

    pub fn extent(&self) -> Extent2D {
        self.extent
    }

    pub fn image_view(&self) -> ImageView {
        self.image_view
    }

    pub fn image_id(&self) -> u32 {
        self.image_id
    }

    pub fn image_sync(&self) -> ImageSync {
        self.image_sync
    }
}
