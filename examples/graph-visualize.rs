//! To run this example use ```cargo run --example graph_visualize --features graph-visualize```
//!
//! This example shows how graph looks like as a structure in .dot format
//! Prints out
//!
use std::sync::Arc;

use graphica::context::GraphicsContext;
use graphica::device::DeviceContext;
use graphica::render_graph::operations::draw_call::{DrawCall, DrawData, DrawGeometry};
use graphica::render_graph::operations::gpu_operation::Operation;
use graphica::render_graph::operations::upload::{UploadBufferOp, UploadImageOp};
use graphica::render_graph::render_graph::RenderGraph;
use graphica::rendering::buffer_container::CreateBuffer;
use graphica::rendering::descriptor_container::DescriptorWriter;
use graphica::rendering::label_container::LabelId;
use graphica::rendering::renderer_bundle::RendererBundle;
use graphica::rendering::shader_container::ShaderType;
use graphica::rendering::texture_container::{
    CreateTexture, CreateTextureView, Filter, SamplingOptions, TextureFormat, WrapOption,
};
use graphica::swapchain::SwapChain;
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ControlFlow, EventLoop};
use winit::window::WindowAttributes;

#[derive(Default)]
pub struct App {}
impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        let attributes = WindowAttributes::default().with_title("Tri");
        let window = event_loop
            .create_window(attributes)
            .expect("Window creation went wrong");

        let graphics_context =
            GraphicsContext::init(&window).expect("Couldn't create graphic config");

        let binding = graphics_context
            .instance()
            .list_devices()
            .expect("Couldn't get devices");
        let best_device = binding
            .iter()
            .max_by(|x, x1| x.rate_default().cmp(&x1.rate_default()))
            .expect("No gpu is available");
        let device_context = DeviceContext::new(&graphics_context, best_device)
            .expect("Couldn't init device context");

        let shared_graphics_context = Arc::new(graphics_context);
        let shared_device_context = Arc::new(device_context);

        let mut swapchain =
            SwapChain::new(&shared_graphics_context, &shared_device_context, &window, 1)
                .expect("couldn't create swapchain");
        let mut bundle = RendererBundle::new();
        let vertex_shader_id = bundle
            .shader_container
            .insert(
                "
                #version 450

                layout(location = 0) out vec2 uv;

                void main() {
                    uv = vec2((gl_VertexIndex << 1) & 2, gl_VertexIndex & 2);
                    gl_Position = vec4(uv * 2.0 - 1.0, 0.0, 1.0);
                    uv.y = -uv.y;
                }",
                ShaderType::Vertex,
            )
            .unwrap();
        let fragment_shader_id = bundle
            .shader_container
            .insert(
                "#version 450

            layout(location = 0) in vec2 uv;
            layout(location = 0) out vec4 outColor;

            layout(set = 0, binding = 0) uniform sampler tex_s;
            layout(set = 0, binding = 1) uniform texture2D tex;
            layout(set = 0, binding = 2) uniform UniformData{
                float px_size_x;
                float px_size_y;
                float std_d;
            } ud;

            float lum(vec3 color){
                return dot(color, vec3(0.3,0.59,0.11));
            }
            const float PI = 3.14159265359;
            float gaus_coeff(vec2 offset, float std_deviation){
                float sq_dev = std_deviation*std_deviation;
                float expo = -(offset.x*offset.x+offset.y*offset.y)/(2*sq_dev);
                float denom = 1/(2*PI*sq_dev);
                return exp(expo)/denom;
            }
            void main() {
                int kernel_rad = int(max(1.0, ceil(ud.std_d * 3.0)));
                vec3 final = vec3(0.0);
                float denom = 0;
                for(int i = -kernel_rad; i <= kernel_rad;i++){
                    for (int j = -kernel_rad; j <= kernel_rad;j++){
                        vec2 offset = vec2(j,i);
                        vec3 fetched_pix = texture(sampler2D(tex, tex_s), uv+offset*vec2(ud.px_size_x,ud.px_size_y)).rgb;
                        float coeff = gaus_coeff(offset,ud.std_d);
                        denom+=coeff;
                        final += fetched_pix*coeff;
                    }
                }
                final /= denom;
                outColor = vec4(final,1.0);
            }",
                ShaderType::Fragment,
            )
            .unwrap();

        let pass_id = bundle
            .pass_container
            .add_pass::<()>(
                &bundle.shader_container,
                [vertex_shader_id, fragment_shader_id].to_vec(),
            )
            .unwrap();
        let image = image::load_from_memory(include_bytes!("Vulkan-logo.png"))
            .unwrap()
            .to_rgba8();

        let texture = bundle
            .texture_container
            .create_texture(
                &shared_device_context,
                CreateTexture::new()
                    .image_format(TextureFormat::R8G8B8A8)
                    .dimensions(image.width(), image.height(), 1),
            )
            .unwrap();
        // Creates label that would be used for visualization or else it would be shown as a raw id
        bundle
            .label_container
            .insert_label(LabelId::Texture(texture), "InTex".to_owned());
        let texture_view = bundle
            .texture_container
            .create_texture_view(
                &shared_device_context,
                CreateTextureView::new()
                    .texture_id(texture)
                    .format(TextureFormat::R8G8B8A8),
            )
            .unwrap();

        let buffer = bundle
            .buffer_container
            .create_uniform_buffer(
                &shared_device_context,
                CreateBuffer::new().len(1).staging(true),
            )
            .unwrap();
        // Creates label that would be used for visualization or else it would be shown as a raw id
        bundle
            .label_container
            .insert_label(LabelId::Buffer(*buffer), "UniformBuffer".to_owned());
        let mut descriptor_group = DescriptorWriter::default();
        descriptor_group.set_sampler(
            "tex_s".to_owned(),
            SamplingOptions::new()
                .filter(Filter::Linear)
                .wrap(WrapOption::Repeat),
        );
        descriptor_group.set_texture("tex".to_owned(), texture_view);
        descriptor_group.set_uniform_buffer("ud".to_owned(), buffer);

        let mut render_graph = RenderGraph::new();
        render_graph.add_operation(Operation::WriteBuffer(
            UploadBufferOp::uniform_buffer(
                buffer,
                [PixSize {
                    x: 1.0 / (image.width() as f32),
                    y: 1.0 / (image.height() as f32),
                    std_deviation: 6.0,
                }]
                .to_vec(),
                0,
            )
            .unwrap(),
        ));
        render_graph.add_operation(Operation::UploadImage(UploadImageOp::new(image, texture)));

        let frame_data = swapchain.next_frame(&shared_device_context);
        let (swapchain_texture, view) = bundle
            .texture_container
            .insert_frameimage(frame_data.image());
        bundle
            .label_container
            .insert_label(LabelId::Texture(swapchain_texture), "OutTex".to_owned());
        let framebuffer_id = bundle
            .framebuffer_container
            .create_framebuffer([view].to_vec());
        for i in 0..3 {
            render_graph.add_operation_labeled(
                Operation::DrawCall(DrawCall::Direct {
                    draw_param: DrawData::new(
                        DrawGeometry::Procedural { count: 3 },
                        framebuffer_id,
                        pass_id,
                        None,
                        Some(descriptor_group.clone()),
                    ),
                }),
                format!("Draw no {}", i),
            );
        }
        render_graph.add_target_op(Operation::Present(frame_data.image().clone()));
        let instant = std::time::Instant::now();
        if let Some(dot) = render_graph.compile_into_dot(&mut bundle) {
            println!("{}", dot);
        }
        println!("passed: {}", instant.elapsed().as_secs_f64());
        event_loop.exit();
    }

    fn window_event(
        &mut self,
        _: &winit::event_loop::ActiveEventLoop,
        _: winit::window::WindowId,
        _: WindowEvent,
    ) {
        ()
    }
}
fn main() {
    simple_logger::init().expect("Couldn't initialize logger");
    let event_loop = EventLoop::new().unwrap();
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut app = App::default();
    event_loop.run_app(&mut app).unwrap();
}
use graphica::rendering::buffer_container::uniform_data;
#[uniform_data]
#[derive(Debug, Clone, Copy)]
pub struct PixSize {
    x: f32,
    y: f32,
    std_deviation: f32,
}
