use crate::app::utils::graphics::{
	VertexBuffer,
	Shader
};

use glium::{
	DrawParameters,
	Frame,
	Surface,
	DrawError,
	uniforms::Uniforms
};

/// Handles vertex_buffer and shader.
pub struct Mesh<'dp> {
	vertex_buffer: VertexBuffer,
	shader: Shader,
	draw_params: DrawParameters<'dp>
}

impl<'dp> Mesh<'dp> {
	/// Constructs new mesh.
	pub fn new(vertex_buffer: VertexBuffer, shader: Shader, draw_params: DrawParameters<'dp>) -> Self {
		Mesh {
			vertex_buffer: vertex_buffer,
			shader: shader,
			draw_params: draw_params
		}
	}

	/// Renders mesh.
	pub fn render<U: Uniforms>(&self, target: &mut Frame, uniforms: &U) -> Result<(), DrawError> {
		target.draw(&self.vertex_buffer.vertex_buffer, &self.vertex_buffer.indices, &self.shader.program, uniforms, &self.draw_params)
	}
}