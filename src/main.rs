#![cfg_attr(feature = "release", windows_subsystem = "windows")]
#![feature(generators, generator_trait, get_mut_unchecked, exhaustive_patterns, associated_type_defaults, never_type)]

#[allow(unused_imports)]
#[macro_use(vecf, veci, vecu, vecs)]
pub extern crate math_linear;

pub mod app;
pub mod prelude;

pub use app::utils::*;

use app::App;
use runtime::RUNTIME;

fn main() {
    env_logger::init();
    app::utils::werror::set_panic_hook();

    RUNTIME.block_on(App::new()).run();
}