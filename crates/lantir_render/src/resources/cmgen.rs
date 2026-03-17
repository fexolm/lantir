use std::path::{Path, PathBuf};

use glam::{Vec2, Vec3};

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

pub fn build_prefiltered_atlas(
    levels: &[CmgenCubemapLevel],
) -> anyhow::Result<image::DynamicImage> {
    if levels.is_empty() {
        anyhow::bail!("cannot build prefiltered atlas from empty mip list");
    }

    let base_face_size = levels[0].faces[0].width();
    if base_face_size == 0 || levels[0].faces[0].height() != base_face_size {
        anyhow::bail!("cmgen base face must be non-empty and square");
    }

    let atlas_width = base_face_size * 6;
    let atlas_height: u32 = levels.iter().map(|level| level.faces[0].height()).sum();
    let mut atlas = image::Rgba32FImage::new(atlas_width, atlas_height);

    let mut y_offset = 0u32;
    for level in levels {
        let face_size = level.faces[0].width();
        if face_size == 0 || level.faces[0].height() != face_size {
            anyhow::bail!("cmgen face for mip {} must be square", level.mip_level);
        }

        for (face_idx, face) in level.faces.iter().enumerate() {
            let face_rgba = face.to_rgba32f();
            if face_rgba.width() != face_size || face_rgba.height() != face_size {
                anyhow::bail!(
                    "cmgen face dimensions mismatch in mip {} face {}",
                    level.mip_level,
                    face_idx
                );
            }

            let x_offset = face_idx as u32 * face_size;
            for y in 0..face_size {
                for x in 0..face_size {
                    let p = *face_rgba.get_pixel(x, y);
                    atlas.put_pixel(x_offset + x, y_offset + y, p);
                }
            }
        }

        y_offset += face_size;
    }

    Ok(image::DynamicImage::ImageRgba32F(atlas))
}

fn radical_inverse_vdc(mut bits: u32) -> f32 {
    bits = bits.rotate_right(16);
    bits = ((bits & 0x5555_5555) << 1) | ((bits & 0xAAAA_AAAA) >> 1);
    bits = ((bits & 0x3333_3333) << 2) | ((bits & 0xCCCC_CCCC) >> 2);
    bits = ((bits & 0x0F0F_0F0F) << 4) | ((bits & 0xF0F0_F0F0) >> 4);
    bits = ((bits & 0x00FF_00FF) << 8) | ((bits & 0xFF00_FF00) >> 8);
    (bits as f32) * 2.328_306_4e-10
}

fn hammersley(i: u32, n: u32) -> Vec2 {
    Vec2::new(i as f32 / n as f32, radical_inverse_vdc(i))
}

fn importance_sample_ggx(xi: Vec2, roughness: f32) -> Vec3 {
    let a = roughness * roughness;
    let a2 = a * a;

    let phi = 2.0 * std::f32::consts::PI * xi.x;
    let cos_theta = ((1.0 - xi.y) / (1.0 + (a2 - 1.0) * xi.y)).sqrt();
    let sin_theta = (1.0 - cos_theta * cos_theta).max(0.0).sqrt();

    Vec3::new(phi.cos() * sin_theta, phi.sin() * sin_theta, cos_theta)
}

fn geometry_schlick_ggx(ndotv: f32, roughness: f32) -> f32 {
    let r = roughness + 1.0;
    let k = (r * r) / 8.0;
    ndotv / ((ndotv * (1.0 - k) + k).max(1e-6))
}

fn geometry_smith(ndotv: f32, ndotl: f32, roughness: f32) -> f32 {
    geometry_schlick_ggx(ndotv, roughness) * geometry_schlick_ggx(ndotl, roughness)
}

fn integrate_brdf(ndotv: f32, roughness: f32, sample_count: u32) -> Vec2 {
    let v = Vec3::new((1.0 - ndotv * ndotv).max(0.0).sqrt(), 0.0, ndotv);

    let mut scale = 0.0;
    let mut bias = 0.0;

    for i in 0..sample_count {
        let xi = hammersley(i, sample_count);
        let h = importance_sample_ggx(xi, roughness);
        let l = (2.0 * v.dot(h) * h - v).normalize_or_zero();

        let ndotl = l.z.max(0.0);
        let ndoth = h.z.max(0.0);
        let vdoth = v.dot(h).max(0.0);

        if ndotl > 0.0 {
            let g = geometry_smith(ndotv, ndotl, roughness);
            let g_vis = (g * vdoth) / (ndoth * ndotv).max(1e-6);
            let fresnel = (1.0 - vdoth).powi(5);

            scale += (1.0 - fresnel) * g_vis;
            bias += fresnel * g_vis;
        }
    }

    Vec2::new(scale, bias) / sample_count as f32
}

pub fn generate_brdf_lut_image(size: u32, sample_count: u32) -> image::DynamicImage {
    let mut lut = image::Rgba32FImage::new(size, size);

    for y in 0..size {
        let roughness = (y as f32 + 0.5) / size as f32;
        for x in 0..size {
            let ndotv = (x as f32 + 0.5) / size as f32;
            let integrated = integrate_brdf(ndotv, roughness, sample_count);
            lut.put_pixel(x, y, image::Rgba([integrated.x, integrated.y, 0.0, 1.0]));
        }
    }

    image::DynamicImage::ImageRgba32F(lut)
}

#[cfg(test)]
mod tests {
    use super::{build_prefiltered_atlas, generate_brdf_lut_image, load_cmgen_skybox_files};

    #[test]
    fn loads_cmgen_sunset_assets() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/assets/sunset");
        let assets = load_cmgen_skybox_files(&dir).expect("load cmgen assets");

        assert_eq!(assets.prefiltered_levels.len(), 5);
        assert_eq!(assets.sh.len(), 9);
    }

    #[test]
    fn builds_prefiltered_atlas() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/assets/sunset");
        let assets = load_cmgen_skybox_files(&dir).expect("load cmgen assets");
        let atlas = build_prefiltered_atlas(&assets.prefiltered_levels).expect("build atlas");

        assert_eq!(atlas.width(), 1536);
        assert_eq!(atlas.height(), 496);
    }

    #[test]
    fn generates_brdf_lut() {
        let lut = generate_brdf_lut_image(16, 16);
        assert_eq!(lut.width(), 16);
        assert_eq!(lut.height(), 16);
    }
}
