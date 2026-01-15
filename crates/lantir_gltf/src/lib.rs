use anyhow::Context;
use glam::{Mat4, Quat, Vec2, Vec3, Vec4};
use lantir_render::resources::{
    MaterialHandle, MeshHandle, PbrMaterial, TextureHandle, TriMesh, Vertex, INVALID_RESOURCE_HANDLE,
};
use lantir_render::resources::resource_manager::ResourceManager;
use lantir_render::world_renderer::WorldRenderer;

pub struct GltfScene {
    pub nodes: Vec<GltfNode>,
}

pub struct GltfNode {
    pub name: String,
    pub children: Vec<usize>,
    pub mesh: Option<MeshHandle>,
    pub material: Option<MaterialHandle>,
    pub transform: Mat4,
}

pub fn load_gltf(world_renderer: &WorldRenderer, gltf_bytes: &[u8]) -> anyhow::Result<GltfScene> {

    fn node_local_transform(node: &gltf::Node) -> Mat4 {
        match node.transform() {
            gltf::scene::Transform::Matrix { matrix } => Mat4::from_cols_array_2d(&matrix),
            gltf::scene::Transform::Decomposed {
                translation,
                rotation,
                scale,
            } => {
                let t = Mat4::from_translation(Vec3::from(translation));
                let r = Mat4::from_quat(Quat::from_array(rotation));
                let s = Mat4::from_scale(Vec3::from(scale));
                t * r * s
            }
        }
    }

    fn decode_gltf_image(gltf: &gltf::Gltf, image: &gltf::Image) -> anyhow::Result<image::DynamicImage> {
        let bytes: Vec<u8> = match image.source() {
            gltf::image::Source::View { view, .. } => {
                let buffer = view.buffer();
                let blob = gltf.blob.as_deref().context("Missing GLB BIN blob")?;
                match buffer.source() {
                    gltf::buffer::Source::Bin => {
                        let start = view.offset();
                        let end = start + view.length();
                        blob.get(start..end)
                            .context("Image buffer view out of bounds")?
                            .to_vec()
                    }
                    gltf::buffer::Source::Uri(_) => {
                        anyhow::bail!("External image buffers are not supported (GLB-only)")
                    }
                }
            }
            gltf::image::Source::Uri { uri, .. } => {
                anyhow::bail!("Image URI is not supported (expected embedded image in .glb): {uri}")
            }
        };

        image::load_from_memory(&bytes).context("decode embedded glTF image")
    }

    fn upload_textures(
        rm: &ResourceManager,
        gltf: &gltf::Gltf,
    ) -> anyhow::Result<std::collections::HashMap<usize, TextureHandle>> {
        let mut image_to_texture = std::collections::HashMap::new();
        for gltf_img in gltf.images() {
            let idx = gltf_img.index();
            let decoded = decode_gltf_image(gltf, &gltf_img)
                .with_context(|| format!("decode glTF image #{idx}"))?;
            let handle = rm.add_texture(decoded).with_context(|| format!("upload glTF image #{idx}"))?;
            image_to_texture.insert(idx, handle);
        }
        Ok(image_to_texture)
    }

    fn create_primitive_mesh(rm: &ResourceManager, gltf: &gltf::Gltf, primitive: &gltf::Primitive) -> anyhow::Result<MeshHandle> {
        let reader = primitive.reader(|buffer| match buffer.source() {
            gltf::buffer::Source::Bin => gltf.blob.as_deref(),
            gltf::buffer::Source::Uri(_) => None,
        });

        let positions = reader
            .read_positions()
            .context("primitive has no positions")?
            .collect::<Vec<[f32; 3]>>();

        let normals: Vec<[f32; 3]> = reader
            .read_normals()
            .map(|it| it.collect())
            .unwrap_or_else(|| vec![[0.0, 1.0, 0.0]; positions.len()]);

        let tex_coords: Vec<[f32; 2]> = reader
            .read_tex_coords(0)
            .map(|it| it.into_f32().collect())
            .unwrap_or_else(|| vec![[0.0, 0.0]; positions.len()]);

        let indices: Vec<u32> = reader
            .read_indices()
            .map(|it| it.into_u32().collect())
            .unwrap_or_else(|| (0..positions.len() as u32).collect());

        let mut vertices = Vec::with_capacity(positions.len());
        for i in 0..positions.len() {
            vertices.push(Vertex {
                position: Vec3::from(positions[i]),
                normal: Vec3::from(normals[i]),
                color: Vec4::ONE,
                uv: Vec2::from(tex_coords[i]),
            });
        }

        rm.add_mesh(&TriMesh { vertices, indices })
            .context("resource_manager.add_mesh")
    }

    fn create_primitive_material(
        rm: &ResourceManager,
        image_to_texture: &std::collections::HashMap<usize, TextureHandle>,
        material: &gltf::Material,
    ) -> anyhow::Result<MaterialHandle> {
        let pbr = material.pbr_metallic_roughness();

        let base_color = Vec4::from_array(pbr.base_color_factor());

        let (albedo_tex, albedo_sampler) = if let Some(info) = pbr.base_color_texture() {
            let img_index = info.texture().source().index();
            let handle = image_to_texture
                .get(&img_index)
                .copied()
                .unwrap_or(INVALID_RESOURCE_HANDLE);
            (handle, 0)
        } else {
            (INVALID_RESOURCE_HANDLE, INVALID_RESOURCE_HANDLE)
        };

        rm.add_material(PbrMaterial {
            albedo_tex,
            albedo_sampler,
            normal_tex: INVALID_RESOURCE_HANDLE,
            normal_sampler: INVALID_RESOURCE_HANDLE,
            metallic_roughness_tex: INVALID_RESOURCE_HANDLE,
            metallic_roughness_sampler: INVALID_RESOURCE_HANDLE,
            emissive_tex: INVALID_RESOURCE_HANDLE,
            emissive_sampler: INVALID_RESOURCE_HANDLE,
            base_color,
            emissive_color: Vec3::ZERO,
            metallness: pbr.metallic_factor(),
            roughness: pbr.roughness_factor(),
        })
        .context("resource_manager.add_material")
    }

    fn add_node_recursive(
        nodes: &mut Vec<GltfNode>,
        gltf: &gltf::Gltf,
        rm: &ResourceManager,
        image_to_texture: &std::collections::HashMap<usize, TextureHandle>,
        node: gltf::Node,
        parent_world: Mat4,
    ) -> anyhow::Result<usize> {
        let local = node_local_transform(&node);
        let world = parent_world * local;

        let base_name = node.name().unwrap_or("").to_string();
        let this_index = nodes.len();
        nodes.push(GltfNode {
            name: base_name.clone(),
            children: Vec::new(),
            mesh: None,
            material: None,
            transform: world,
        });

        // If node has a mesh with primitives, represent each primitive as a child node with identity transform.
        if let Some(mesh) = node.mesh() {
            for (prim_i, primitive) in mesh.primitives().enumerate() {
                let mesh_handle = create_primitive_mesh(rm, gltf, &primitive)
                    .with_context(|| format!("create mesh for node '{base_name}' primitive {prim_i}"))?;
                let mat_handle = create_primitive_material(rm, image_to_texture, &primitive.material())
                    .with_context(|| format!("create material for node '{base_name}' primitive {prim_i}"))?;

                let prim_index = nodes.len();
                let prim_name = if base_name.is_empty() {
                    format!("primitive#{prim_i}")
                } else {
                    format!("{base_name}#prim{prim_i}")
                };
                nodes.push(GltfNode {
                    name: prim_name,
                    children: Vec::new(),
                    mesh: Some(mesh_handle),
                    material: Some(mat_handle),
                    transform: world,
                });
                nodes[this_index].children.push(prim_index);
            }
        }

        for child in node.children() {
            let child_index = add_node_recursive(nodes, gltf, rm, image_to_texture, child, world)?;
            nodes[this_index].children.push(child_index);
        }

        Ok(this_index)
    }

    let rm = world_renderer.get_resource_manager();
    let gltf = gltf::Gltf::from_slice(gltf_bytes).context("parse glTF")?;

    // Enforce GLB-only: no external buffers via URI.
    for buffer in gltf.buffers() {
        if matches!(buffer.source(), gltf::buffer::Source::Uri(_)) {
            anyhow::bail!(
                "External buffers are not supported anymore; provide a .glb with embedded BIN chunk"
            );
        }
    }

    let image_to_texture = upload_textures(rm, &gltf)?;

    let mut nodes = Vec::new();
    for scene in gltf.scenes() {
        for node in scene.nodes() {
            add_node_recursive(
                &mut nodes,
                &gltf,
                rm,
                &image_to_texture,
                node,
                Mat4::IDENTITY,
            )?;
        }
    }

    Ok(GltfScene { nodes })
}