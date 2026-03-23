use std::{collections::HashMap, error::Error};

use ash::vk::{
    Extent3D, Format, Image, ImageAspectFlags, ImageCreateInfo, ImageLayout, ImageSubresourceRange,
    ImageTiling, ImageType, ImageUsageFlags, ImageView, ImageViewCreateInfo, ImageViewType,
    SampleCountFlags, SamplerAddressMode, SamplerCreateInfo, SamplerMipmapMode,
};
use gpu_allocator::{
    MemoryLocation,
    vulkan::{self, Allocation, AllocationCreateDesc},
};
use slotmap::{SlotMap, new_key_type};

use crate::{
    device::DeviceContext,
    swapchain::{FrameData, FrameImage},
};

/// Centralized container for managing all that related to textures, but not array of textures
#[derive(Debug, Default)]
pub struct TextureContainer {
    images: SlotMap<TextureId, Texture>,
    image_views: SlotMap<RawTextureViewId, TextureView>,
    samplers: HashMap<SamplingOptions, Sampler>,
    swapchain_frame: HashMap<FrameImage, (TextureId, TextureViewId)>,
}
impl TextureContainer {
    /// Creates empty `TextureContainer`
    pub fn new() -> Self {
        Self {
            images: SlotMap::default(),
            image_views: SlotMap::default(),
            samplers: HashMap::new(),
            swapchain_frame: HashMap::new(),
        }
    }

    /// Creates `Texture` with given `CreateTexture`
    ///
    /// # Errors
    /// returns error if image creation or memory allocation fails
    pub fn create_texture(
        &mut self,
        device: &DeviceContext,
        create: CreateTexture,
    ) -> Result<TextureId, Box<dyn Error>> {
        let image_type = if create.dimensions.depth > 1 {
            ImageType::TYPE_3D
        } else if create.dimensions.height > 1 {
            ImageType::TYPE_2D
        } else {
            ImageType::TYPE_1D
        };
        //TODO:Add more parameters to create texture
        let image_create = ImageCreateInfo::default()
            .extent(create.dimensions)
            .image_type(image_type)
            .initial_layout(ImageLayout::UNDEFINED)
            .format(create.texture_format.to_image_format())
            .mip_levels(1)
            .array_layers(1)
            .usage(
                ImageUsageFlags::TRANSFER_SRC
                    | ImageUsageFlags::TRANSFER_DST
                    | ImageUsageFlags::SAMPLED
                    | ImageUsageFlags::COLOR_ATTACHMENT,
            )
            .samples(SampleCountFlags::TYPE_1)
            .tiling(ImageTiling::OPTIMAL);
        let image = unsafe { device.create_image(&image_create, None)? };
        let image_mem_req = unsafe { device.get_image_memory_requirements(image) };

        let alloc = device.allocator().allocate(&AllocationCreateDesc {
            name: "Texture",
            requirements: image_mem_req,
            location: MemoryLocation::GpuOnly,
            linear: false,
            allocation_scheme: vulkan::AllocationScheme::DedicatedImage(image.clone()),
        })?;
        unsafe { device.bind_image_memory(image, alloc.memory(), alloc.offset()) }.unwrap();
        let texture = Texture {
            alloc,
            image,
            extent: create.dimensions,
            image_type,
            texture_format: create.texture_format,
        };
        Ok(self.images.insert(texture))
    }

    /// Creates `TextureView` with given `CreateTexture`
    pub fn create_texture_view(
        &mut self,
        device: &DeviceContext,
        create: CreateTextureView,
    ) -> Result<TextureViewId, Box<dyn Error>> {
        if let Some(texture_id) = create.texture_id {
            let subresource = ImageSubresourceRange::default()
                .base_mip_level(0)
                .aspect_mask(ImageAspectFlags::COLOR)
                .base_array_layer(0)
                .layer_count(1)
                .level_count(1);
            let image_view_createinfo = ImageViewCreateInfo::default()
                .format(create.view_format.to_image_format())
                .view_type(ImageViewType::TYPE_2D)
                .image(self.images[texture_id].image)
                .subresource_range(subresource);
            let texture = self.get_image(texture_id).unwrap();
            let image_view = unsafe { device.create_image_view(&image_view_createinfo, None) }?;
            let texture_view = TextureView {
                handle: image_view,
                extent: texture.dimensions(),
                format: create.view_format,
            };
            let raw_id = self.image_views.insert(texture_view);
            let view_id = TextureViewId {
                texture: texture_id,
                raw_id,
            };
            return Ok(view_id);
        }
        Err("No TextureId was provided".into())
    }

    /// Returns a reference to the `Texture` associated with the `TextureId`
    ///
    /// Returns `None` if texture has been destroyed or the `TextureId` is invalid
    pub fn get_image(&self, texture_id: TextureId) -> Option<&Texture> {
        self.images.get(texture_id)
    }

    /// Returns a reference to the `TextureView` associated with the `TextureViewId`
    ///
    /// Returns `None` if texture has been destroyed or the `TextureViewId` is invalid
    pub fn get_image_view(&self, view_id: TextureViewId) -> Option<&TextureView> {
        self.image_views.get(view_id.raw_id)
    }

    pub fn insert_framedata(&mut self, frame_data: &FrameData) -> (TextureId, TextureViewId) {
        if let Some(res) = self.swapchain_frame.get(frame_data.image()) {
            return *res;
        }
        let texture = Texture {
            image: frame_data.image().image(),
            alloc: Allocation::default(),
            image_type: ImageType::TYPE_2D,
            extent: Extent3D::from(frame_data.image().extent()),
            texture_format: TextureFormat::Swapchain(frame_data.image().format()),
        };
        let texture_id = self.images.insert(texture);
        let texture_view = TextureView {
            handle: frame_data.image().image_view(),
            extent: frame_data.image().extent().into(),
            format: TextureFormat::Swapchain(frame_data.image().format()),
        };
        let raw_id = self.image_views.insert(texture_view);
        let texture_view_id = TextureViewId {
            texture: texture_id,
            raw_id,
        };
        self.swapchain_frame
            .insert(*frame_data.image(), (texture_id, texture_view_id));
        log::info!(
            "inserted framedata: {:?}\n swapchain frames: {:?}",
            frame_data,
            self.swapchain_frame
        );
        (texture_id, texture_view_id)
    }
    pub fn remove_frameimage(
        &mut self,
        frame_image: &FrameImage,
    ) -> Option<(TextureId, TextureViewId)> {
        if let Some(ids) = self.swapchain_frame.get(frame_image) {
            let ids = *ids;
            self.image_views.remove(ids.1.raw_id);
            self.images.remove(ids.0);
            self.swapchain_frame.remove(frame_image);
            Some(ids)
        } else {
            None
        }
    }
    pub(crate) fn get_sampler(
        &mut self,
        device: &DeviceContext,
        options: SamplingOptions,
    ) -> Option<Sampler> {
        if let Some(&sampler) = self.samplers.get(&options) {
            return Some(sampler);
        } else {
            let sampler_createinfo = SamplerCreateInfo::default()
                .anisotropy_enable(false)
                .max_anisotropy(1.0)
                .address_mode_u(options.wrap_x.into_address_mode())
                .address_mode_v(options.wrap_y.into_address_mode())
                .mag_filter(options.mag_filter.into_vk_filter())
                .min_filter(options.min_filter.into_vk_filter())
                .unnormalized_coordinates(false);
            let handle = unsafe { device.create_sampler(&sampler_createinfo, None) }.ok()?;
            let sampler = Sampler { handle, options };
            self.samplers.insert(options, sampler);
            Some(sampler)
        }
    }
    //TODO:IDK, it looks not really good
    // /// **FOR TESTING PURPOSES**
    // ///
    // /// Creates a dummy `Texture`
    // #[cfg(test)]
    // pub fn create_texture_null(&mut self) -> TextureId {
    //     self.images.insert(Texture::default())
    // }

    // /// **FOR TESTING PURPOSES**
    // ///
    // /// Creates a dummy `TextureView`
    // #[cfg(test)]
    // pub fn create_texture_view_null(&mut self) -> TextureViewId {
    //     self.image_views.insert(TextureView::default())
    // }
}

//TODO:Add reference counting for automatic dispose (IDK if this is a good idea)
new_key_type! {
    /// Unique identifier to a `Texture` in a `TextureContainer`
    pub struct TextureId;
}

new_key_type! {
    /// Unique identifier to a `TextureView` in a `TextureContainer`
    pub struct RawTextureViewId;
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TextureViewId {
    texture: TextureId,
    raw_id: RawTextureViewId,
}

impl TextureViewId {
    pub fn texture(&self) -> TextureId {
        self.texture
    }
}
/// A GPU-driven image resource.
#[derive(Debug, Default)]
pub struct Texture {
    image: Image,
    alloc: Allocation,
    image_type: ImageType,
    extent: Extent3D,
    texture_format: TextureFormat,
    //TODO:Add mipmap level count
}
impl Texture {
    /// Returns the raw handle `VKImage`
    pub fn handle(&self) -> Image {
        self.image
    }

    /// Returns the physical dimensions of texture
    pub fn dimensions(&self) -> Extent3D {
        self.extent
    }

    /// Returns the underlying image `Format`
    pub fn texture_format(&self) -> TextureFormat {
        self.texture_format
    }

    /// Returns the dimensionality type of the texture (1D, 2D, or 3D).
    pub fn image_type(&self) -> ImageType {
        self.image_type
    }

    /// Returns the reference to the allocation info of texture
    pub fn allocation(&self) -> &Allocation {
        &self.alloc
    }
}
/// Configuration parameters for creating a `Texture`.
#[derive(Debug, Clone, Default)]
pub struct CreateTexture {
    dimensions: Extent3D,
    texture_format: TextureFormat,
}
impl CreateTexture {
    /// Creates new `CreateTexture` with default values
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the dimensions of the texture
    pub fn dimensions(mut self, width: u32, height: u32, depth: u32) -> Self {
        self.dimensions = Extent3D {
            width,
            height,
            depth,
        };
        self
    }

    /// Sets the texture format
    pub fn image_format(mut self, texture_format: TextureFormat) -> Self {
        self.texture_format = texture_format;
        self
    }
}

/// A struct that define how renderer will read and write data into `Texture`
#[derive(Debug, Default)]
pub struct TextureView {
    handle: ImageView,
    extent: Extent3D,
    format: TextureFormat,
    //TODO:Add mipmap level
}
impl TextureView {
    pub fn handle(&self) -> ImageView {
        self.handle
    }

    pub fn extent(&self) -> Extent3D {
        self.extent
    }

    pub fn format(&self) -> TextureFormat {
        self.format
    }
}
/// Configuration parameters for creating a `TextureView`.
#[derive(Debug, Clone, Default)]
pub struct CreateTextureView {
    texture_id: Option<TextureId>,
    view_format: TextureFormat,
}
impl CreateTextureView {
    pub fn new() -> Self {
        Self::default()
    }
    /// Sets texture from which `TextureView` will be created
    pub fn texture_id(mut self, texture_id: TextureId) -> Self {
        self.texture_id = Some(texture_id);
        self
    }
    /// Sets `Format` for the TextureView
    pub fn format(mut self, format: TextureFormat) -> Self {
        self.view_format = format;
        self
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TextureFormat {
    R8G8B8A8,
    B8G8R8A8,
    R8G8,
    Depth32F,
    Depth24Stencil8,
    Swapchain(Format),
}
impl TextureFormat {
    pub fn is_color(&self) -> bool {
        matches!(
            self,
            TextureFormat::R8G8B8A8
                | TextureFormat::R8G8
                | TextureFormat::B8G8R8A8
                | Self::Swapchain(_)
        )
    }
    pub fn is_depth(&self) -> bool {
        matches!(self, TextureFormat::Depth32F)
    }
    pub fn is_depth_stencil(&self) -> bool {
        matches!(self, TextureFormat::Depth24Stencil8)
    }
    pub fn to_image_format(&self) -> Format {
        match self {
            TextureFormat::R8G8B8A8 => Format::R8G8B8A8_UNORM,
            TextureFormat::B8G8R8A8 => Format::B8G8R8A8_UNORM,
            TextureFormat::R8G8 => Format::R8G8_UNORM,
            TextureFormat::Depth32F => Format::D32_SFLOAT,
            TextureFormat::Depth24Stencil8 => Format::D24_UNORM_S8_UINT,
            TextureFormat::Swapchain(f) => *f,
        }
    }
}
impl Default for TextureFormat {
    fn default() -> Self {
        TextureFormat::R8G8B8A8
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Sampler {
    handle: ash::vk::Sampler,
    options: SamplingOptions,
}

impl Sampler {
    pub fn handle(&self) -> ash::vk::Sampler {
        self.handle
    }
}
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SamplingOptions {
    mag_filter: Filter,
    min_filter: Filter,
    wrap_x: WrapOption,
    wrap_y: WrapOption,
}
impl SamplingOptions {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn filter(mut self, filter: Filter) -> Self {
        self.mag_filter = filter;
        self.min_filter = filter;
        self
    }
    pub fn wrap(mut self, wrap: WrapOption) -> Self {
        self.wrap_x = wrap;
        self.wrap_y = wrap;
        self
    }
}

// TODO: Add more options
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Filter {
    #[default]
    Point,
    Linear,
}
impl Filter {
    pub fn into_vk_filter(&self) -> ash::vk::Filter {
        match self {
            Filter::Point => ash::vk::Filter::NEAREST,
            Filter::Linear => ash::vk::Filter::LINEAR,
        }
    }
}
// TODO:Add more options
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WrapOption {
    #[default]
    Repeat,
}
impl WrapOption {
    pub fn into_address_mode(&self) -> SamplerAddressMode {
        match self {
            WrapOption::Repeat => SamplerAddressMode::REPEAT,
        }
    }
}
