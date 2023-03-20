pub mod voxel_data;
pub mod atlas;
pub mod generator;

use {
    crate::app::utils::{
        cfg,
        terrain::chunk::{FullVertex, LowVertex},
        terrain::voxel::VoxelData,
        reinterpreter::*,
    },
    voxel_data::*,
    math_linear::prelude::*,
};

/// Represents voxel.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Voxel {
    pub data: &'static VoxelData,
    pub pos: Int3,
}

impl Voxel {
    pub const SIZE: f32 = cfg::terrain::VOXEL_SIZE;

    /// Voxel constructor.
    pub fn new(position: Int3, data: &'static VoxelData) -> Self {
        Voxel { data, pos: position }
    }

    pub fn is_air(&self) -> bool {
        self.data.id == AIR_VOXEL_DATA.id
    }
}

pub fn is_id_valid(id: Id) -> bool {
    let id = id as usize;
    (0..VOXEL_DATA.len()).contains(&id)
}

/// Generalization of voxel details.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LoweredVoxel {
    Transparent,
    Colored(Color),
}



unsafe impl AsBytes for Voxel {
    fn as_bytes(&self) -> Vec<u8> {
        self.data.id.as_bytes()
            .into_iter()
            .chain(self.pos.as_bytes())
            .collect()
    }
}

unsafe impl FromBytes for Voxel {
    fn from_bytes(source: &[u8]) -> Result<Self, ReinterpretError> {
        let mut reader = ByteReader::new(source);
        let id: Id = reader.read()?;
        let pos = reader.read()?;

        Ok(Self { pos, data: &VOXEL_DATA[id as usize] })
    }
}

unsafe impl StaticSize for Voxel {
    fn static_size() -> usize { 16 }
}



#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reinterpret_voxel1() {
        let before = Voxel::new(Int3::new(123, 4212, 11), STONE_VOXEL_DATA);
        let after = Voxel::from_bytes(&before.as_bytes()).unwrap();

        assert_eq!(before, after);
    }

    #[test]
    fn reinterpret_voxel2() {
        let before = Voxel::new(Int3::new(-213, 4212, 11), LOG_VOXEL_DATA);
        let after = Voxel::from_bytes(&before.as_bytes()).unwrap();

        assert_eq!(before, after);
    }
}



pub mod shape {
    use super::{*, atlas::UV};

    // TODO: move to cfg
    pub const BACK_NORMAL:   (f32, f32, f32) = ( 1.0,  0.0,  0.0 );
    pub const FRONT_NORMAL:  (f32, f32, f32) = (-1.0,  0.0,  0.0 );
    pub const TOP_NORMAL:    (f32, f32, f32) = ( 0.0,  1.0,  0.0 );
    pub const BOTTOM_NORMAL: (f32, f32, f32) = ( 0.0, -1.0,  0.0 );
    pub const RIGHT_NORMAL:  (f32, f32, f32) = ( 0.0,  0.0,  1.0 );
    pub const LEFT_NORMAL:   (f32, f32, f32) = ( 0.0,  0.0, -1.0 );

    pub const BACK_TANGENT:   (f32, f32, f32) = ( 0.0,  1.0,  0.0 );
    pub const FRONT_TANGENT:  (f32, f32, f32) = BACK_TANGENT;
    pub const TOP_TANGENT:    (f32, f32, f32) = (-1.0,  0.0,  0.0 );
    pub const BOTTOM_TANGENT: (f32, f32, f32) = TOP_TANGENT;
    pub const RIGHT_TANGENT:  (f32, f32, f32) = BACK_TANGENT;
    pub const LEFT_TANGENT:   (f32, f32, f32) = BACK_TANGENT;

    #[derive(Debug)]
    pub struct CubeDetailed<'c> {
        data: &'c VoxelData,
        half_size: f32,
    }

    #[derive(Debug)]
    pub struct CubeLowered {
        half_size: f32,
    }

    impl<'c> CubeDetailed<'c> {
        /// Constructs new cube maker with filled voxel data.
        pub fn new(data: &'c VoxelData) -> Self {
            Self { data, half_size: Voxel::SIZE * 0.5 }
        }

        /// Edit default size.
        #[allow(dead_code)]
        pub fn size(mut self, new_size: f32) -> Self {
            self.half_size = new_size * 0.5;
            return self
        }

        pub fn by_offset(&self, offset: Int3, position: vec3, vertices: &mut Vec<FullVertex>) {
            let position = 2.0 * self.half_size * position;
            match offset.as_tuple() {
                ( 1,  0,  0) => self.back(position, vertices),
                (-1,  0,  0) => self.front(position, vertices),
                ( 0,  1,  0) => self.top(position, vertices),
                ( 0, -1,  0) => self.bottom(position, vertices),
                ( 0,  0,  1) => self.right(position, vertices),
                ( 0,  0, -1) => self.left(position, vertices),
                _ => panic!("There's no offset {:?}", offset),
            }
        }

        /// Cube front face vertex array.
        pub fn front(&self, position: vec3, vertices: &mut Vec<FullVertex>) {
            /* UVs for front face */
            let uv = UV::new(self.data.textures.front);
            
            /* Shortcuts */
            let (x, y, z) = position.as_tuple();
            let normal = FRONT_NORMAL;
            let tangent = FRONT_TANGENT;

            vertices.push(FullVertex { position: (-self.half_size + x, -self.half_size + y, -self.half_size + z), tex_coords: (uv.x_hi, uv.y_hi), normal, tangent });
            vertices.push(FullVertex { position: (-self.half_size + x,  self.half_size + y, -self.half_size + z), tex_coords: (uv.x_hi, uv.y_lo), normal, tangent });
            vertices.push(FullVertex { position: (-self.half_size + x,  self.half_size + y,  self.half_size + z), tex_coords: (uv.x_lo, uv.y_lo), normal, tangent });
            vertices.push(FullVertex { position: (-self.half_size + x, -self.half_size + y, -self.half_size + z), tex_coords: (uv.x_hi, uv.y_hi), normal, tangent });
            vertices.push(FullVertex { position: (-self.half_size + x,  self.half_size + y,  self.half_size + z), tex_coords: (uv.x_lo, uv.y_lo), normal, tangent });
            vertices.push(FullVertex { position: (-self.half_size + x, -self.half_size + y,  self.half_size + z), tex_coords: (uv.x_lo, uv.y_hi), normal, tangent });
        }

        /// Cube back face vertex array.
        pub fn back(&self, position: vec3, vertices: &mut Vec<FullVertex>) {
            /* UVs for back face */
            let uv = UV::new(self.data.textures.back);
            
            /* Shortcuts */
            let (x, y, z) = position.as_tuple();
            let normal = BACK_NORMAL;
            let tangent = BACK_TANGENT;

            vertices.push(FullVertex { position: (self.half_size + x, -self.half_size + y, -self.half_size + z), tex_coords: (uv.x_lo, uv.y_hi), normal, tangent });
            vertices.push(FullVertex { position: (self.half_size + x, -self.half_size + y,  self.half_size + z), tex_coords: (uv.x_hi, uv.y_hi), normal, tangent });
            vertices.push(FullVertex { position: (self.half_size + x,  self.half_size + y,  self.half_size + z), tex_coords: (uv.x_hi, uv.y_lo), normal, tangent });
            vertices.push(FullVertex { position: (self.half_size + x, -self.half_size + y, -self.half_size + z), tex_coords: (uv.x_lo, uv.y_hi), normal, tangent });
            vertices.push(FullVertex { position: (self.half_size + x,  self.half_size + y,  self.half_size + z), tex_coords: (uv.x_hi, uv.y_lo), normal, tangent });
            vertices.push(FullVertex { position: (self.half_size + x,  self.half_size + y, -self.half_size + z), tex_coords: (uv.x_lo, uv.y_lo), normal, tangent });
        }

        /// Cube top face vertex array.
        pub fn top(&self, position: vec3, vertices: &mut Vec<FullVertex>) {
            /* UVs for top face */
            let uv = UV::new(self.data.textures.top);
            
            /* Shortcuts */
            let (x, y, z) = position.as_tuple();
            let normal = TOP_NORMAL;
            let tangent = TOP_TANGENT;

            vertices.push(FullVertex { position: ( self.half_size + x,  self.half_size + y, -self.half_size + z), tex_coords: (uv.x_lo, uv.y_hi), normal, tangent });
            vertices.push(FullVertex { position: ( self.half_size + x,  self.half_size + y,  self.half_size + z), tex_coords: (uv.x_hi, uv.y_hi), normal, tangent });
            vertices.push(FullVertex { position: (-self.half_size + x,  self.half_size + y, -self.half_size + z), tex_coords: (uv.x_lo, uv.y_lo), normal, tangent });
            vertices.push(FullVertex { position: (-self.half_size + x,  self.half_size + y, -self.half_size + z), tex_coords: (uv.x_lo, uv.y_lo), normal, tangent });
            vertices.push(FullVertex { position: ( self.half_size + x,  self.half_size + y,  self.half_size + z), tex_coords: (uv.x_hi, uv.y_hi), normal, tangent });
            vertices.push(FullVertex { position: (-self.half_size + x,  self.half_size + y,  self.half_size + z), tex_coords: (uv.x_hi, uv.y_lo), normal, tangent });
        }

        /// Cube bottom face vertex array.
        pub fn bottom(&self, position: vec3, vertices: &mut Vec<FullVertex>) {
            /* UVs for bottom face */
            let uv = UV::new(self.data.textures.bottom);
            
            /* Shortcuts */
            let (x, y, z) = position.as_tuple();
            let normal = BOTTOM_NORMAL;
            let tangent = BOTTOM_TANGENT;

            vertices.push(FullVertex { position: (-self.half_size + x, -self.half_size + y, -self.half_size + z), tex_coords: (uv.x_lo, uv.y_lo), normal, tangent });
            vertices.push(FullVertex { position: ( self.half_size + x, -self.half_size + y,  self.half_size + z), tex_coords: (uv.x_hi, uv.y_hi), normal, tangent });
            vertices.push(FullVertex { position: ( self.half_size + x, -self.half_size + y, -self.half_size + z), tex_coords: (uv.x_lo, uv.y_hi), normal, tangent });
            vertices.push(FullVertex { position: (-self.half_size + x, -self.half_size + y, -self.half_size + z), tex_coords: (uv.x_lo, uv.y_lo), normal, tangent });
            vertices.push(FullVertex { position: (-self.half_size + x, -self.half_size + y,  self.half_size + z), tex_coords: (uv.x_hi, uv.y_lo), normal, tangent });
            vertices.push(FullVertex { position: ( self.half_size + x, -self.half_size + y,  self.half_size + z), tex_coords: (uv.x_hi, uv.y_hi), normal, tangent });
        }

        /// Cube left face vertex array.
        pub fn left(&self, position: vec3, vertices: &mut Vec<FullVertex>) {
            /* UVs for left face */
            let uv = UV::new(self.data.textures.left);
            
            /* Shortcuts */
            let (x, y, z) = position.as_tuple();
            let normal = LEFT_NORMAL;
            let tangent = LEFT_TANGENT;

            vertices.push(FullVertex { position: ( self.half_size + x, -self.half_size + y, -self.half_size + z), tex_coords: (uv.x_lo, uv.y_hi), normal, tangent }); // 0 (uv.x_lo, uv.y_lo)
            vertices.push(FullVertex { position: ( self.half_size + x,  self.half_size + y, -self.half_size + z), tex_coords: (uv.x_lo, uv.y_lo), normal, tangent }); // 1 (uv.x_lo, uv.y_hi)
            vertices.push(FullVertex { position: (-self.half_size + x,  self.half_size + y, -self.half_size + z), tex_coords: (uv.x_hi, uv.y_lo), normal, tangent }); // 2 (uv.x_hi, uv.y_hi)
            vertices.push(FullVertex { position: ( self.half_size + x, -self.half_size + y, -self.half_size + z), tex_coords: (uv.x_lo, uv.y_hi), normal, tangent }); // 0
            vertices.push(FullVertex { position: (-self.half_size + x,  self.half_size + y, -self.half_size + z), tex_coords: (uv.x_hi, uv.y_lo), normal, tangent }); // 2
            vertices.push(FullVertex { position: (-self.half_size + x, -self.half_size + y, -self.half_size + z), tex_coords: (uv.x_hi, uv.y_hi), normal, tangent }); // 3 (uv.x_hi, uv.y_lo)
        }

        /// Cube right face vertex array.
        pub fn right(&self, position: vec3, vertices: &mut Vec<FullVertex>) {
            /* UVs for right face */
            let uv = UV::new(self.data.textures.right);
            
            /* Shortcuts */
            let (x, y, z) = position.as_tuple();
            let normal = RIGHT_NORMAL;
            let tangent = RIGHT_TANGENT;

            vertices.push(FullVertex { position: ( self.half_size + x, -self.half_size + y,  self.half_size + z), tex_coords: (uv.x_lo, uv.y_hi), normal, tangent }); // lolo (uv.x_lo, uv.y_lo)
            vertices.push(FullVertex { position: (-self.half_size + x,  self.half_size + y,  self.half_size + z), tex_coords: (uv.x_hi, uv.y_lo), normal, tangent }); // hihi
            vertices.push(FullVertex { position: ( self.half_size + x,  self.half_size + y,  self.half_size + z), tex_coords: (uv.x_lo, uv.y_lo), normal, tangent }); // lohi (uv.x_lo, uv.y_hi)
            vertices.push(FullVertex { position: ( self.half_size + x, -self.half_size + y,  self.half_size + z), tex_coords: (uv.x_lo, uv.y_hi), normal, tangent }); // lolo (uv.x_lo, uv.y_lo)
            vertices.push(FullVertex { position: (-self.half_size + x, -self.half_size + y,  self.half_size + z), tex_coords: (uv.x_hi, uv.y_hi), normal, tangent }); // hilo
            vertices.push(FullVertex { position: (-self.half_size + x,  self.half_size + y,  self.half_size + z), tex_coords: (uv.x_hi, uv.y_lo), normal, tangent }); // hihi
        }

        /// Cube all sides.
        #[allow(dead_code)]
        pub fn all(&self, position: vec3, vertices: &mut Vec<FullVertex>) {
            self.left(position, vertices);
            self.right(position, vertices);
            self.front(position, vertices);
            self.back(position, vertices);
            self.top(position, vertices);
            self.bottom(position, vertices);
        }
    }

    impl CubeLowered {
        pub fn new(size: f32) -> Self {
            Self { half_size: size / 2.0 }
        }

        pub fn by_offset(&self, offset: Int3, position: vec3, color: Color, vertices: &mut Vec<LowVertex>) {
            match offset.as_tuple() {
                ( 1,  0,  0) => self.back(position, color, vertices),
                (-1,  0,  0) => self.front(position, color, vertices),
                ( 0,  1,  0) => self.top(position, color, vertices),
                ( 0, -1,  0) => self.bottom(position, color, vertices),
                ( 0,  0,  1) => self.right(position, color, vertices),
                ( 0,  0, -1) => self.left(position, color, vertices),
                _ => panic!("There's no offset {:?}", offset),
            }
        }

        /// Cube front face vertex array.
        pub fn front(&self, position: vec3, color: Color, vertices: &mut Vec<LowVertex>) {
            /* Shortcuts */
            let (x, y, z) = position.as_tuple();
            let color = color.as_tuple();
            let normal = FRONT_NORMAL;

            vertices.push(LowVertex { position: (-self.half_size + x, -self.half_size + y, -self.half_size + z), color, normal });
            vertices.push(LowVertex { position: (-self.half_size + x,  self.half_size + y, -self.half_size + z), color, normal });
            vertices.push(LowVertex { position: (-self.half_size + x,  self.half_size + y,  self.half_size + z), color, normal });
            vertices.push(LowVertex { position: (-self.half_size + x, -self.half_size + y, -self.half_size + z), color, normal });
            vertices.push(LowVertex { position: (-self.half_size + x,  self.half_size + y,  self.half_size + z), color, normal });
            vertices.push(LowVertex { position: (-self.half_size + x, -self.half_size + y,  self.half_size + z), color, normal });
        }

        /// Cube back face vertex array.
        pub fn back(&self, position: vec3, color: Color, vertices: &mut Vec<LowVertex>) {
            /* Shortcuts */
            let (x, y, z) = position.as_tuple();
            let color = color.as_tuple();
            let normal = BACK_NORMAL;

            vertices.push(LowVertex { position: (self.half_size + x, -self.half_size + y, -self.half_size + z), color, normal });
            vertices.push(LowVertex { position: (self.half_size + x, -self.half_size + y,  self.half_size + z), color, normal });
            vertices.push(LowVertex { position: (self.half_size + x,  self.half_size + y,  self.half_size + z), color, normal });
            vertices.push(LowVertex { position: (self.half_size + x, -self.half_size + y, -self.half_size + z), color, normal });
            vertices.push(LowVertex { position: (self.half_size + x,  self.half_size + y,  self.half_size + z), color, normal });
            vertices.push(LowVertex { position: (self.half_size + x,  self.half_size + y, -self.half_size + z), color, normal });
        }

        /// Cube top face vertex array.
        pub fn top(&self, position: vec3, color: Color, vertices: &mut Vec<LowVertex>) {
            /* Shortcuts */
            let (x, y, z) = position.as_tuple();
            let color = color.as_tuple();
            let normal = TOP_NORMAL;

            vertices.push(LowVertex { position: ( self.half_size + x,  self.half_size + y, -self.half_size + z), color, normal });
            vertices.push(LowVertex { position: ( self.half_size + x,  self.half_size + y,  self.half_size + z), color, normal });
            vertices.push(LowVertex { position: (-self.half_size + x,  self.half_size + y, -self.half_size + z), color, normal });
            vertices.push(LowVertex { position: (-self.half_size + x,  self.half_size + y, -self.half_size + z), color, normal });
            vertices.push(LowVertex { position: ( self.half_size + x,  self.half_size + y,  self.half_size + z), color, normal });
            vertices.push(LowVertex { position: (-self.half_size + x,  self.half_size + y,  self.half_size + z), color, normal });
        }

        /// Cube bottom face vertex array.
        pub fn bottom(&self, position: vec3, color: Color, vertices: &mut Vec<LowVertex>) {
            /* Shortcuts */
            let (x, y, z) = position.as_tuple();
            let color = color.as_tuple();
            let normal = BOTTOM_NORMAL;

            vertices.push(LowVertex { position: (-self.half_size + x, -self.half_size + y, -self.half_size + z), color, normal });
            vertices.push(LowVertex { position: ( self.half_size + x, -self.half_size + y,  self.half_size + z), color, normal });
            vertices.push(LowVertex { position: ( self.half_size + x, -self.half_size + y, -self.half_size + z), color, normal });
            vertices.push(LowVertex { position: (-self.half_size + x, -self.half_size + y, -self.half_size + z), color, normal });
            vertices.push(LowVertex { position: (-self.half_size + x, -self.half_size + y,  self.half_size + z), color, normal });
            vertices.push(LowVertex { position: ( self.half_size + x, -self.half_size + y,  self.half_size + z), color, normal });
        }

        /// Cube left face vertex array.
        pub fn left(&self, position: vec3, color: Color, vertices: &mut Vec<LowVertex>) {
            /* Shortcuts */
            let (x, y, z) = position.as_tuple();
            let color = color.as_tuple();
            let normal = LEFT_NORMAL;

            vertices.push(LowVertex { position: ( self.half_size + x, -self.half_size + y, -self.half_size + z), color, normal });
            vertices.push(LowVertex { position: ( self.half_size + x,  self.half_size + y, -self.half_size + z), color, normal });
            vertices.push(LowVertex { position: (-self.half_size + x,  self.half_size + y, -self.half_size + z), color, normal });
            vertices.push(LowVertex { position: ( self.half_size + x, -self.half_size + y, -self.half_size + z), color, normal });
            vertices.push(LowVertex { position: (-self.half_size + x,  self.half_size + y, -self.half_size + z), color, normal });
            vertices.push(LowVertex { position: (-self.half_size + x, -self.half_size + y, -self.half_size + z), color, normal });
        }

        /// Cube right face vertex array.
        pub fn right(&self, position: vec3, color: Color, vertices: &mut Vec<LowVertex>) {
            /* Shortcuts */
            let (x, y, z) = position.as_tuple();
            let color = color.as_tuple();
            let normal = RIGHT_NORMAL;

            vertices.push(LowVertex { position: ( self.half_size + x, -self.half_size + y,  self.half_size + z), color, normal });
            vertices.push(LowVertex { position: (-self.half_size + x,  self.half_size + y,  self.half_size + z), color, normal });
            vertices.push(LowVertex { position: ( self.half_size + x,  self.half_size + y,  self.half_size + z), color, normal });
            vertices.push(LowVertex { position: ( self.half_size + x, -self.half_size + y,  self.half_size + z), color, normal });
            vertices.push(LowVertex { position: (-self.half_size + x, -self.half_size + y,  self.half_size + z), color, normal });
            vertices.push(LowVertex { position: (-self.half_size + x,  self.half_size + y,  self.half_size + z), color, normal });
        }

        /// Cube all sides.
        #[allow(dead_code)]
        pub fn all(&self, position: vec3, color: Color, vertices: &mut Vec<LowVertex>) {
            self.left(position, color, vertices);
            self.right(position, color, vertices);
            self.front(position, color, vertices);
            self.back(position, color, vertices);
            self.top(position, color, vertices);
            self.bottom(position, color, vertices);
        }}
}
