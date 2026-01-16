#![warn(clippy::all)]

pub mod bevy;
pub mod render_pass;
pub mod resources;
pub mod scene;
pub mod world_renderer;

#[macro_export]
macro_rules! include_shader {
    ($name:literal) => {{
        // rust-analyzer expands macros without running build scripts in some configurations,
        // which means `OUT_DIR` may be unavailable and would otherwise produce diagnostics.
        // For IDE purposes we return an empty slice; real builds always go through Cargo.
        #[cfg(rust_analyzer)]
        {
            &[] as &[u32]
        }

        #[cfg(not(rust_analyzer))]
        {
            #[allow(dead_code)]
            mod __lantir_included_shader {
                include!(concat!(env!("OUT_DIR"), "/shaders/", $name, ".spv.rs"));
            }
            __lantir_included_shader::WORDS
        }
    }};
}
