use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    // Keep `cfg(rust_analyzer)` available for IDE-only macro branches without triggering
    // `unexpected_cfgs` warnings on newer Rust.
    println!("cargo::rustc-check-cfg=cfg(rust_analyzer)");

    let manifest_dir =
        PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR not set"));
    let shader_src_root = manifest_dir.join("shaders");

    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR not set"));
    let shader_out_root = out_dir.join("shaders");

    println!("cargo:rerun-if-env-changed=VULKAN_SDK");
    println!("cargo:rerun-if-changed={}", shader_src_root.display());

    fs::create_dir_all(&shader_out_root).expect("failed to create OUT_DIR/shaders");

    let is_debug = env::var("PROFILE").as_deref() == Ok("debug");

    let mut sources = Vec::new();
    collect_hlsl_files(&shader_src_root, &mut sources);

    for src_path in sources {
        println!("cargo:rerun-if-changed={}", src_path.display());

        let rel = src_path
            .strip_prefix(&shader_src_root)
            .unwrap_or_else(|_| panic!("{} is not under {}", src_path.display(), shader_src_root.display()));

        let rel_str = rel.to_string_lossy();
        let out_spv = shader_out_root.join(format!("{rel_str}.spv"));
        let out_rs = shader_out_root.join(format!("{rel_str}.spv.rs"));

        if let Some(parent) = out_spv.parent() {
            fs::create_dir_all(parent).expect("failed to create shader OUT_DIR subdir");
        }

        compile_hlsl_to_spirv(&shader_src_root, &src_path, &out_spv, is_debug);
        write_spv_words_rs(&out_spv, &out_rs);
    }
}

fn collect_hlsl_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let entries = fs::read_dir(dir).unwrap_or_else(|e| panic!("failed to read {}: {e}", dir.display()));

    for entry in entries {
        let entry = entry.unwrap_or_else(|e| panic!("failed to read dir entry in {}: {e}", dir.display()));
        let path = entry.path();

        if path.is_dir() {
            collect_hlsl_files(&path, out);
            continue;
        }

        if path.extension().and_then(|s| s.to_str()) == Some("hlsl") {
            out.push(path);
        }
    }
}

fn dxc_path() -> PathBuf {
    if let Ok(sdk) = env::var("VULKAN_SDK") {
        let base = PathBuf::from(sdk);

        #[cfg(windows)]
        {
            let exe = base.join("Bin").join("dxc.exe");
            if exe.exists() {
                return exe;
            }
        }

        let alt = base.join("Bin").join("dxc");
        if alt.exists() {
            return alt;
        }
    }

    PathBuf::from("dxc")
}

fn compile_hlsl_to_spirv(shader_src_root: &Path, src_path: &Path, out_spv: &Path, is_debug: bool) {
    let mut cmd = Command::new(dxc_path());

    // One file -> one SPIR-V module. Entry points come from `[shader("...")]` attributes.
    cmd.arg("-spirv")
        .arg("-T")
        .arg("lib_6_6")
        .arg("-fspv-target-env=vulkan1.3")
        .arg("-fspv-extension=SPV_KHR_ray_tracing")
        .arg("-Fo")
        .arg(out_spv)
        .arg("-I")
        .arg(shader_src_root)
        .arg("-I")
        .arg(src_path.parent().unwrap_or(shader_src_root))
        .arg(src_path);

    if is_debug {
        cmd.arg("-Od").arg("-Zi");
    } else {
        cmd.arg("-O3");
    }

    let status = cmd
        .status()
        .unwrap_or_else(|e| panic!("failed to run dxc for {}: {e}", src_path.display()));

    if !status.success() {
        panic!("dxc failed for {} (exit: {status})", src_path.display());
    }
}

fn write_spv_words_rs(spv_path: &Path, out_rs: &Path) {
    let bytes =
        fs::read(spv_path).unwrap_or_else(|e| panic!("failed to read {}: {e}", spv_path.display()));

    if !bytes.len().is_multiple_of(4) {
        panic!(
            "SPIR-V size is not a multiple of 4: {} ({} bytes)",
            spv_path.display(),
            bytes.len()
        );
    }

    let words: Vec<u32> = bytes
        .chunks_exact(4)
        .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect();

    let mut s = String::new();
    s.push_str("pub const WORDS: &[u32] = &[\n");
    for (i, w) in words.iter().enumerate() {
        if i % 8 == 0 {
            s.push_str("    ");
        }
        s.push_str(&format!("0x{w:08x}, "));
        if i % 8 == 7 {
            s.push('\n');
        }
    }
    if !words.len().is_multiple_of(8) {
        s.push('\n');
    }
    s.push_str("];\n");

    if let Some(parent) = out_rs.parent() {
        fs::create_dir_all(parent).expect("failed to create shader OUT_DIR subdir for .rs");
    }

    fs::write(out_rs, s).unwrap_or_else(|e| panic!("failed to write {}: {e}", out_rs.display()));
}
