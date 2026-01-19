pub mod resource_manager;

pub type TextureHandle = u64;
pub type SamplerHandle = u64;
pub type MeshHandle = u64;
pub type MaterialHandle = u64;

pub const INVALID_RESOURCE_HANDLE: u64 = u64::MAX;

pub const DEFAULT_SAMPLER_HANDLE: SamplerHandle = 0;

pub const MAX_TEXTURES: usize = 1024;
pub const MAX_SAMPLERS: usize = 1024;
pub const MAX_MESHES: usize = 4096;
pub const MAX_MATERIALS: usize = 4096;
pub const MAX_DRAW_ITEMS: usize = 16384;
pub const MAX_INDIRECT_DRAWS: usize = MAX_DRAW_ITEMS * 8;
pub const MAX_VERTICES: usize = 16_000_000;
pub const MAX_INDICES: usize = 32_000_000;

pub const META_BUFFER_BINDING_VERTEX: u32 = 0;
pub const META_BUFFER_BINDING_MATERIAL: u32 = 1;
pub const META_BUFFER_BINDING_TEXTURE: u32 = 2;
pub const META_BUFFER_BINDING_DRAW_ITEMS: u32 = 3;
pub const META_BUFFER_BINDING_SAMPLER: u32 = 4;
pub const META_BUFFER_BINDING_SKYBOX: u32 = 5;

#[repr(C)]
#[derive(Default, Copy, Clone)]
pub struct Skybox {
    pub tex: u32,
    pub sampler: u32,
    pub exposure: f32,
    pub ambient_floor: f32,
}

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
    pub albedo_sampler: SamplerHandle,
    pub normal_tex: TextureHandle,
    pub normal_sampler: SamplerHandle,
    pub metallic_roughness_tex: TextureHandle,
    pub metallic_roughness_sampler: SamplerHandle,
    pub emissive_tex: TextureHandle,
    pub emissive_sampler: SamplerHandle,

    pub base_color: glam::Vec4,
    pub emissive_color: glam::Vec3,
    pub metallness: f32,
    pub roughness: f32,
    pub blend_mode: PbrBlendMode,
    pub alpha_cutoff: f32,
}
impl PbrMaterial {
    pub(crate) fn is_transparent(&self) -> bool {
        matches!(self.blend_mode, PbrBlendMode::Transparent)
    }
}

#[repr(u32)]
#[derive(Copy, Clone)]
pub enum PbrBlendMode {
    Opaque = 0,
    Masked = 1,
    Transparent = 2,
}

#[repr(C)]
#[derive(Copy, Clone)]
pub struct DrawItem {
    pub transform: glam::Mat4,
    pub mesh: MeshHandle,
    pub material: MaterialHandle,
}

#[derive(Copy, Clone)]
pub struct UploadedMesh {
    pub vertex_offset: i32,
    pub index_offset: u32,
    pub index_count: u32,
}

pub struct TriMesh {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
}
