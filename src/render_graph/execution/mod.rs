use std::{
    collections::HashMap,
    ffi::{CStr, CString},
    str::FromStr,
};

use ash::vk::{CommandBuffer, CommandBufferBeginInfo, DebugUtilsLabelEXT};

use crate::{
    device::DeviceContext,
    render_graph::{
        operations::{
            draw_call::{DrawCall, DrawParameters},
            gpu_operation::Operation,
        },
        render_graph::Action,
    },
    rendering::renderer_bundle::RendererBundle,
    swapchain::FrameData,
};

pub trait Executor {
    fn execute(
        &self,
        device: &DeviceContext,
        bundle: &mut RendererBundle,
        frame_data: &FrameData,
    ) -> CommandBuffer;
}
#[derive(Debug, Clone)]
pub struct EasyExecutor {
    pub actions: Vec<Action>,
}
impl Executor for EasyExecutor {
    fn execute(
        &self,
        device: &DeviceContext,
        bundle: &mut RendererBundle,
        frame_data: &FrameData,
    ) -> CommandBuffer {
        let command_buffer = device
            .render_queue()
            .graphics_queue()
            .get_commandpool(device, &frame_data)
            .get_buffer(device);
        let label_str = CString::from_str("Graph pass").unwrap();
        let debug_label = DebugUtilsLabelEXT::default()
            .color([0.0, 0.0, 0.0, 1.0])
            .label_name(&label_str);
        let mut last_state = HashMap::new();
        unsafe {
            device
                .begin_command_buffer(command_buffer, &CommandBufferBeginInfo::default())
                .unwrap();
            // device
            //     .debug_fns()
            //     .cmd_begin_debug_utils_label(command_buffer, &debug_label);
            for action in self.actions.iter() {
                // println!("executed:{:?}", action);
                if let Action::Sync(new_state) = &action {
                    for (_, sync_ops) in new_state.sync_ops().iter() {
                        for sync_op in sync_ops.iter() {
                            last_state
                                .insert(sync_op.resource_id(), sync_op.resource_state_after());
                        }
                    }
                }
                if let Action::Op(Operation::DrawCall(DrawCall::Direct { draw_param })) = &action {
                    if let Some(new_states) = draw_param.resource_state(bundle) {
                        for new_state in new_states {
                            last_state.insert(new_state.resource_id(), new_state);
                        }
                    }
                };
                action.execute(bundle, command_buffer, device);
            }
            //device.debug_fns().cmd_end_debug_utils_label(command_buffer);
            device.end_command_buffer(command_buffer).unwrap();
        }
        for (_, new_state) in last_state.iter() {
            bundle
                .resource_state
                .insert_or_set(new_state.resource_id(), *new_state)
        }
        command_buffer
    }
}
