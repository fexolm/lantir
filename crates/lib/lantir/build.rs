use std::env;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};

fn main() {
    // Keep `cfg(rust_analyzer)` available for IDE-only macro branches without triggering
    // `unexpected_cfgs` warnings on newer Rust.
    println!("cargo::rustc-check-cfg=cfg(rust_analyzer)");

    let manifest_dir =
        PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR not set"));
    let shader_src_root = manifest_dir.join("shaders");

    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR not set"));
    let shader_out_root = out_dir.join("shaders");

    // Re-run if the shader directory structure changes.
    println!("cargo:rerun-if-changed={}", shader_src_root.display());

    fs::create_dir_all(&shader_out_root).expect("failed to create OUT_DIR/shaders");

    let compiler = shaderc::Compiler::new().expect("failed to create shaderc compiler");

    let is_debug = env::var("PROFILE").as_deref() == Ok("debug");

    compile_shaders_recursive(
        &compiler,
        &shader_src_root,
        &shader_src_root,
        &shader_out_root,
        is_debug,
    );
}

fn compile_shaders_recursive(
    compiler: &shaderc::Compiler,
    base_dir: &Path,
    dir: &Path,
    out_root: &Path,
    is_debug: bool,
) {
    let entries = fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("failed to read shader dir {}: {e}", dir.display()));

    for entry in entries {
        let entry = entry.unwrap_or_else(|e| panic!("failed to read dir entry: {e}"));
        let path = entry.path();

        if path.is_dir() {
            compile_shaders_recursive(compiler, base_dir, &path, out_root, is_debug);
            continue;
        }

        let Some(ext) = path.extension().and_then(OsStr::to_str) else {
            continue;
        };

        let (kind, is_hlsl) = if ext == "hlsl" {
            // Expect shader stage in the preceding extension: `*.vert.hlsl`, `*.frag.hlsl`, ...
            let Some(stem) = path.file_stem().and_then(OsStr::to_str) else {
                continue;
            };
            let Some((_, stage_ext)) = stem.rsplit_once('.') else {
                continue;
            };

            let kind = match stage_ext {
                "vert" => shaderc::ShaderKind::Vertex,
                "frag" => shaderc::ShaderKind::Fragment,
                "comp" => shaderc::ShaderKind::Compute,
                "geom" => shaderc::ShaderKind::Geometry,
                "tesc" => shaderc::ShaderKind::TessControl,
                "tese" => shaderc::ShaderKind::TessEvaluation,
                _ => continue,
            };

            (kind, true)
        } else {
            let kind = match ext {
                "vert" => shaderc::ShaderKind::Vertex,
                "frag" => shaderc::ShaderKind::Fragment,
                "comp" => shaderc::ShaderKind::Compute,
                "geom" => shaderc::ShaderKind::Geometry,
                "tesc" => shaderc::ShaderKind::TessControl,
                "tese" => shaderc::ShaderKind::TessEvaluation,
                _ => continue,
            };

            (kind, false)
        };

        println!("cargo:rerun-if-changed={}", path.display());

        let rel = path.strip_prefix(base_dir).unwrap_or_else(|_| {
            panic!(
                "shader path {} is not under {}",
                path.display(),
                base_dir.display()
            )
        });

        let out_path = out_root.join(format!("{}.spv", rel.display()));
        if let Some(parent) = out_path.parent() {
            fs::create_dir_all(parent)
                .unwrap_or_else(|e| panic!("failed to create {}: {e}", parent.display()));
        }

        compile_shader(compiler, &path, &out_path, kind, is_hlsl, is_debug);
    }
}

fn compile_shader(
    compiler: &shaderc::Compiler,
    src_path: &Path,
    out_path: &Path,
    kind: shaderc::ShaderKind,
    is_hlsl: bool,
    is_debug: bool,
) {
    let source = fs::read_to_string(src_path)
        .unwrap_or_else(|e| panic!("failed to read shader {}: {e}", src_path.display()));

    let name = src_path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("shader");

    let mut options = shaderc::CompileOptions::new().expect("failed to create shaderc options");
    options.set_target_env(
        shaderc::TargetEnv::Vulkan,
        shaderc::EnvVersion::Vulkan1_3 as u32,
    );
    options.set_target_spirv(shaderc::SpirvVersion::V1_5);

    if is_hlsl {
        options.set_source_language(shaderc::SourceLanguage::HLSL);
    } else {
        options.set_source_language(shaderc::SourceLanguage::GLSL);
    }

    // Nice for RenderDoc/debugprintf in dev builds.
    if is_debug {
        options.set_generate_debug_info();
        options.set_optimization_level(shaderc::OptimizationLevel::Zero);
    } else {
        options.set_optimization_level(shaderc::OptimizationLevel::Performance);
    }

    let binary = compiler
        .compile_into_spirv(&source, kind, name, "main", Some(&options))
        .unwrap_or_else(|e| panic!("shader compilation failed for {}: {e}", src_path.display()));

    fs::write(out_path, binary.as_binary_u8())
        .unwrap_or_else(|e| panic!("failed to write {}: {e}", out_path.display()));

    // Also emit a Rust source file with a u32 word array for safe inclusion via `include!()`.
    // This avoids 4-byte alignment issues that can happen with `include_bytes!()`.
    let rs_path = out_path.with_extension("spv.rs");
    let words = binary.as_binary();

    let mut rs = String::new();
    rs.push_str("#[allow(dead_code)]\n");
    rs.push_str("pub static WORDS: &[u32] = &[\n");
    for (i, w) in words.iter().enumerate() {
        if i % 8 == 0 {
            rs.push_str("    ");
        }
        rs.push_str(&format!("0x{w:08x}u32,"));
        if i % 8 == 7 {
            rs.push('\n');
        } else {
            rs.push(' ');
        }
    }
    if (words.len() % 8) != 0 {
        rs.push('\n');
    }
    rs.push_str("];\n");

    fs::write(&rs_path, rs)
        .unwrap_or_else(|e| panic!("failed to write {}: {e}", rs_path.display()));
}
