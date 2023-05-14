use {
    crate::{
        prelude::*,
        graphics::{TextureView, CommandEncoder},
    },
    std::fmt::Debug,
};



#[derive(Debug, Deref)]
pub struct ClearPass<'s>(pub wgpu::RenderPass<'s>);
assert_impl_all!(ClearPass: Send, Sync);

impl<'s> ClearPass<'s> {
    pub fn new(
        encoder: &'s mut wgpu::CommandEncoder,
        target_views: impl IntoIterator<Item = &'s TextureView>,
    ) -> Self {
        use wgpu::{RenderPassColorAttachment, Operations, LoadOp, RenderPassDescriptor};

        let color_attachments: Vec<_> = target_views.into_iter()
            .map(|view| Some(RenderPassColorAttachment {
                view,
                resolve_target: None,
                ops: Operations {
                    load: LoadOp::Clear(cfg::shader::CLEAR_COLOR),
                    store: true,
                },
            }))
            .collect();

        let render_pass = encoder.begin_render_pass(
            &RenderPassDescriptor {
                label: Some("clear_pass"),
                color_attachments: &color_attachments,
                depth_stencil_attachment: None,
            },
        );

        Self(render_pass)
    }

    /// Does a [`ClearPass`] creation and then drops it.
    pub fn clear(
        encoder: &'s mut CommandEncoder,
        target_views: impl IntoIterator<Item = &'s TextureView>,
    ) {
        let _ = Self::new(encoder, target_views);
    }
}



#[derive(Debug, Deref)]
pub struct RenderPass<'s>(pub wgpu::RenderPass<'s>);
assert_impl_all!(ClearPass: Send, Sync);

impl<'s> RenderPass<'s> {
    pub fn new(
        encoder: &'s mut CommandEncoder,
        label: &str,
        target_views: impl IntoIterator<Item = &'s TextureView>
    ) -> Self {
        use wgpu::{RenderPassColorAttachment, Operations, LoadOp, RenderPassDescriptor};

        let color_attachments: Vec<_> = target_views.into_iter()
            .map(|view| Some(RenderPassColorAttachment {
                view,
                resolve_target: None,
                ops: Operations {
                    load: LoadOp::Load,
                    store: true,
                },
            }))
            .collect();

        let render_pass = encoder.begin_render_pass(
            &RenderPassDescriptor {
                label: Some(label),
                color_attachments: &color_attachments,
                depth_stencil_attachment: None,
            },
        );

        Self(render_pass)
    }
}