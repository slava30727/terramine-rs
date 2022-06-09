pub mod vector;
pub mod matrix;
pub mod angle;
pub mod plane;
pub mod bounding_volumes;

pub mod prelude {
	pub use super::{
		vector::{swizzle::*, *},
		matrix::*,
		angle::*,
		plane::*,
		bounding_volumes::{
			aabb::*,
		},
	};
}