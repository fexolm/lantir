use std::path::{Path, PathBuf};

pub struct CmgenCubemapLevel {
    pub mip_level: u32,
    pub faces: [image::DynamicImage; 6],
}

pub struct CmgenSkyboxFiles {
    pub skybox_faces: [image::DynamicImage; 6],
    pub prefiltered_levels: Vec<CmgenCubemapLevel>,
    pub sh: [glam::Vec4; 9],
}

fn cmgen_face_names() -> [&'static str; 6] {
    ["px", "nx", "py", "ny", "pz", "nz"]
}

fn load_cmgen_face_set(dir: &Path, prefix: &str) -> anyhow::Result<[image::DynamicImage; 6]> {
    let faces = cmgen_face_names()
        .into_iter()
        .map(|face| {
            let path = dir.join(format!("{prefix}{face}.exr"));
            image::open(&path).map_err(|e| anyhow::anyhow!("failed to load {}: {e}", path.display()))
        })
        .collect::<Result<Vec<_>, _>>()?;

    faces.try_into().map_err(|faces: Vec<_>| {
        anyhow::anyhow!(
            "expected 6 cmgen faces for prefix '{}' in {}, got {}",
            prefix,
            dir.display(),
            faces.len()
        )
    })
}

fn try_load_cmgen_face_set(
    dir: &Path,
    prefix: &str,
) -> anyhow::Result<Option<[image::DynamicImage; 6]>> {
    let expected: [PathBuf; 6] =
        std::array::from_fn(|i| dir.join(format!("{prefix}{}.exr", cmgen_face_names()[i])));

    let present = expected.iter().filter(|p| p.exists()).count();
    if present == 0 {
        return Ok(None);
    }
    if present != expected.len() {
        anyhow::bail!(
            "incomplete cmgen face set for prefix '{}': found {present}/{} files in {}",
            prefix,
            expected.len(),
            dir.display()
        );
    }

    load_cmgen_face_set(dir, prefix).map(Some)
}

pub fn load_cmgen_sh_file(path: impl AsRef<Path>) -> anyhow::Result<[glam::Vec4; 9]> {
    let path = path.as_ref();
    let text = std::fs::read_to_string(path)
        .map_err(|e| anyhow::anyhow!("failed to read {}: {e}", path.display()))?;

    let mut coeffs = Vec::with_capacity(9);
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let start = line
            .find('(')
            .ok_or_else(|| anyhow::anyhow!("invalid SH line in {}: {}", path.display(), line))?;
        let end = line[start + 1..]
            .find(')')
            .map(|i| start + 1 + i)
            .ok_or_else(|| anyhow::anyhow!("invalid SH line in {}: {}", path.display(), line))?;

        let values: Vec<f32> = line[start + 1..end]
            .split(',')
            .map(|v| v.trim().parse::<f32>())
            .collect::<Result<_, _>>()
            .map_err(|e| anyhow::anyhow!("invalid SH value in {}: {e}", path.display()))?;

        if values.len() != 3 {
            anyhow::bail!(
                "expected 3 SH components in {}, got {} on line '{}'",
                path.display(),
                values.len(),
                line
            );
        }

        coeffs.push(glam::Vec4::new(values[0], values[1], values[2], 0.0));
    }

    if coeffs.len() != 9 {
        anyhow::bail!(
            "expected 9 SH coefficients in {}, got {}",
            path.display(),
            coeffs.len()
        );
    }

    Ok(std::array::from_fn(|i| coeffs[i]))
}

pub fn load_cmgen_skybox_files(dir: impl AsRef<Path>) -> anyhow::Result<CmgenSkyboxFiles> {
    let dir = dir.as_ref();
    let skybox_faces = load_cmgen_face_set(dir, "")?;

    let mut prefiltered_levels = Vec::new();
    for mip_level in 0u32.. {
        let prefix = format!("m{mip_level}_");
        let Some(faces) = try_load_cmgen_face_set(dir, &prefix)? else {
            break;
        };
        prefiltered_levels.push(CmgenCubemapLevel { mip_level, faces });
    }

    if prefiltered_levels.is_empty() {
        anyhow::bail!("no prefiltered cmgen mip levels found in {}", dir.display());
    }

    let sh = load_cmgen_sh_file(dir.join("sh.txt"))?;

    Ok(CmgenSkyboxFiles {
        skybox_faces,
        prefiltered_levels,
        sh,
    })
}

#[cfg(test)]
mod tests {
    use super::load_cmgen_skybox_files;

    #[test]
    fn loads_cmgen_sunset_assets() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/assets/sunset");
        let assets = load_cmgen_skybox_files(&dir).expect("load cmgen assets");

        assert_eq!(assets.prefiltered_levels.len(), 5);
        assert_eq!(assets.sh.len(), 9);
    }
}
