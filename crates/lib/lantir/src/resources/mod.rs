pub mod resource_manager;

pub type TextureHandle = u64;
pub type MeshHandle = u64;
pub type MaterialHandle = u64;

pub const INVALID_RESOURCE_HANDLE: u64 = u64::MAX;

pub const MAX_TEXTURES: usize = 1024;
pub const MAX_MESHES: usize = 4096;
pub const MAX_MATERIALS: usize = 4096;
pub const MAX_DRAW_ITEMS: usize = 16384;
pub const MAX_INDIRECT_DRAWS: usize = MAX_DRAW_ITEMS * 8;
pub const MAX_VERTICES: usize = 16_000_000;
pub const MAX_INDICES: usize = 32_000_000;

pub const META_BUFFER_BINDING_VERTEX: u32 = 0;
pub const META_BUFFER_BINDING_MESH: u32 = 1;
pub const META_BUFFER_BINDING_MATERIAL: u32 = 2;
pub const META_BUFFER_BINDING_TEXTURE: u32 = 3;
pub const META_BUFFER_BINDING_DRAW_ITEMS: u32 = 4;

#[repr(C)]
#[derive(Default, Copy, Clone)]
pub struct Vertex {
    pub position: glam::Vec3,
    pub normal: glam::Vec3,
    pub color: glam::Vec4,
    pub uv: glam::Vec2,
}

#[repr(C)]
#[derive(Copy, Clone)]
pub struct PbrMaterial {
    pub albedo_tex: TextureHandle,
    pub normal_tex: TextureHandle,
    pub metallic_roughness_tex: TextureHandle,
    pub emissive_tex: TextureHandle,

    pub base_color: glam::Vec4,
    pub emissive_color: glam::Vec3,
    pub metallness: f32,
    pub roughness: f32,
}

#[repr(C)]
#[derive(Copy, Clone)]
pub struct DrawItem {
    pub transform: glam::Mat4,
    pub mesh: MeshHandle,
    pub material: MaterialHandle,
}

#[repr(C)]
#[derive(Copy, Clone)]
pub struct GpuMesh {
    pub vertex_offset: u64,
    pub index_offset: u64,
    pub index_count: u32,
}

pub struct TriMesh {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
}
