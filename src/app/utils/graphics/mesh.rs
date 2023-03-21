use {
    crate::app::utils::graphics::{
        VertexBuffer,
        Shader
    },
    glium::{
        Vertex as TVertex,
        DrawParameters,
        Surface,
        DrawError,
        uniforms::Uniforms,
        index::{NoIndices, IndicesSource}
    },
};

pub type UnindexedMesh<Vertex> = Mesh<NoIndices, Vertex>;

/// Handles vertex_buffer and shader.
#[derive(Debug)]
pub struct Mesh<Idx, Vertex: Copy> {
    vertices: VertexBuffer<Idx, Vertex>,
}

impl<Idx, Vertex: Copy> Mesh<Idx, Vertex> {
    /// Constructs new mesh.
    pub fn new(vertex_buffer: VertexBuffer<Idx, Vertex>) -> Self {
        Mesh { vertices: vertex_buffer }
    }

    /// Renders mesh.
    pub fn render<'a, U>(
        &'a self, target: &mut impl Surface, shader: &Shader,
        draw_params: &DrawParameters<'_>, uniforms: &U) -> Result<(), DrawError>
    where
        U: Uniforms,
        &'a Idx: Into<IndicesSource<'a>>,
    {
        target.draw(&self.vertices.inner, &self.vertices.indices, &shader.program, uniforms, draw_params)
    }

    /// Checks if vertices vector is empty
    pub fn is_empty(&self) -> bool {
        self.vertices.inner.len() == 0
    }
}

impl <Vertex: Copy + TVertex> Mesh<NoIndices, Vertex> {
    pub fn new_empty(display: &dyn glium::backend::Facade) -> Self {
        Mesh { vertices: VertexBuffer::new_empty(display) }
    }
}