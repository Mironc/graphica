#[cfg(feature = "graph-visualize")]
use crate::rendering::renderer_bundle::RendererBundle;
use crate::rendering::{buffer_container::GeneralBufferId, texture_container::TextureId};

///This struct is used to unify all ids
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ResourceId {
    Texture(TextureId),
    Buffer(GeneralBufferId),
}

#[cfg(feature = "graph-visualize")]
impl ResourceId {
    pub fn fmt_dot(&self, bundle: &RendererBundle) -> String {
        match self {
            ResourceId::Texture(texture_id) => {
                use crate::rendering::label_container::LabelId;

                if let Some(label) = bundle
                    .label_container
                    .get_label(&LabelId::Texture(*texture_id))
                {
                    format!("Texture - {}", label)
                } else {
                    format!("{:?}", texture_id)
                }
            }
            ResourceId::Buffer(general_buffer_id) => {
                use crate::rendering::label_container::LabelId;

                if let Some(label) = bundle
                    .label_container
                    .get_label(&LabelId::Buffer(*general_buffer_id))
                {
                    format!("Buffer - {}", label)
                } else {
                    format!("{:?}", general_buffer_id.key_data())
                }
            }
        }
    }
}
