use lantir_hal::acceleration_structure::AccelerationStructure;

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
pub const META_BUFFER_BINDING_INDEX: u32 = 6;
pub const META_BUFFER_BINDING_TLAS: u32 = 7;

#[repr(C)]
#[derive(Default, Copy, Clone)]
pub struct Skybox {
    pub tex: u32,
    pub sampler: u32,
    pub irradiance_tex: u32,
    pub irradiance_sampler: u32,
    pub prefiltered_tex: u32,
    pub prefiltered_sampler: u32,
    pub brdf_lut_tex: u32,
    pub brdf_lut_sampler: u32,
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
    /// Tangent in model space. xyz = tangent direction, w = bitangent sign (+1 or -1).
    /// Must match the `float4 tangent` field in common.hlsli `Vertex`.
    pub tangent: glam::Vec4,
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

#[repr(u32)]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum PbrBlendMode {
    Opaque = 0,
    Masked = 1,
    Transparent = 2,
}

#[repr(C)]
#[derive(Copy, Clone)]
pub struct DrawItem {
    pub model_matrix: glam::Mat4,
    pub normal_matrix: glam::Mat4,
    /// Raw mesh handle (index into uploaded_meshes). Used by CPU-side code only.
    pub mesh: MeshHandle,
    /// Raw material handle (index into material buffer).
    pub material: MaterialHandle,
    /// Packed offsets for RT hit shader use:
    ///   low  32 bits = vertex_offset (first vertex in the global VB for this mesh, as u32)
    ///   high 32 bits = index_offset  (first index in the global IB for this mesh)
    pub mesh_offsets: u64,
}

/// An uploaded mesh, including its pre-built BLAS for ray tracing.
pub struct UploadedMesh {
    pub vertex_offset: i32,
    pub index_offset: u32,
    pub index_count: u32,
    /// BLAS built once at mesh upload time.
    pub blas: AccelerationStructure,
}

pub struct TriMesh {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
}
