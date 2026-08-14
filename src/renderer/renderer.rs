use crate::error::MdlError;
use crate::material::{FilterMode, Material, MaterialUniform};
use crate::model::model::Model;
use crate::model::texture::Texture;
use crate::renderer::camera::CameraState;
use crate::renderer::geoset_render_info::{
    GeosetRenderInfo, PreparedDraw, SCENE_FRONT_FACE, ScenePipelineState, pass_rank,
};
use crate::renderer::line_vertex::LineVertex;
use crate::renderer::vertex::{Vertex, prepare_draw_vertices, texture_matrix};
use crate::scene::types::ScenePacket;
use crate::settings::Settings;
use crate::texture::scene::{
    CpuAlphaEncoding, CpuColorSpace, ResolvedSceneTexture, TextureAddressMode,
};
use std::collections::BTreeMap;
use wgpu::util::DeviceExt;
use winit::window::Window;

pub struct Renderer {
    pub surface: wgpu::Surface<'static>,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub config: wgpu::SurfaceConfiguration,
    pub render_pipeline: wgpu::RenderPipeline,
    pub wireframe_pipeline: wgpu::RenderPipeline,
    pub transparent_pipeline: wgpu::RenderPipeline,
    pub wireframe_transparent_pipeline: wgpu::RenderPipeline,
    pub additive_pipeline: wgpu::RenderPipeline,
    pub wireframe_additive_pipeline: wgpu::RenderPipeline,
    pub line_pipeline: wgpu::RenderPipeline,
    pub vertex_buffer: wgpu::Buffer,
    pub index_buffer: wgpu::Buffer,
    pub num_indices: u32,
    pub geosets: Vec<GeosetRenderInfo>,
    materials: Vec<Material>,
    pub textures: Vec<Texture>,
    pub line_vertex_buffer: wgpu::Buffer,
    pub num_lines: u32,
    pub skeleton_vertex_buffer: wgpu::Buffer,
    pub num_skeleton_lines: u32,
    pub bounding_box_vertex_buffer: wgpu::Buffer,
    pub num_bounding_box_lines: u32,
    pub camera_buffer: wgpu::Buffer,
    pub camera_bind_group: wgpu::BindGroup,
    pub texture_bind_groups: Vec<wgpu::BindGroup>, // One bind group per texture
    texture_views: Vec<Option<wgpu::TextureView>>, // Store texture views for egui
    texture_bind_group_layout: wgpu::BindGroupLayout,
    // Material uniform - single bind group for all materials
    pub material_buffer: wgpu::Buffer,
    pub material_bind_group: wgpu::BindGroup,
    // Store white texture components to create bind groups for missing textures
    white_texture_view: wgpu::TextureView,
    white_texture_sampler: wgpu::Sampler,
    pub team_color: [f32; 3],
    grid_major_color: [f32; 3],
    grid_minor_color: [f32; 3],
    pub skybox_color: [f32; 3],
    pub camera: CameraState,
    model_center: [f32; 3],
    pub egui_renderer: egui_wgpu::Renderer,
    pub view_proj_matrix: nalgebra_glm::Mat4,
    // Store original vertices for animation
    original_vertices: Vec<Vertex>,
    // Store model for accessing vertex groups during animation
    model: Option<Model>,
    #[allow(dead_code)]
    pub(crate) scene_prepared: Option<PreparedScene>,
    #[allow(dead_code)]
    pub(crate) scene_gpu: Option<SceneGpu>,
    #[allow(dead_code)]
    pub(crate) scene_pipelines: Vec<(ScenePipelineState, wgpu::RenderPipeline)>,
    #[allow(dead_code)]
    pub(crate) scene_texture_resources: Option<SceneTextureGpuSet>,
    #[allow(dead_code)]
    scene_material_bind_group_layout: wgpu::BindGroupLayout,
    pub(crate) scene_error: Option<MdlError>,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct SceneDrawUniform {
    pub legacy_team_color: [f32; 4],
    pub legacy_material: [f32; 4],
    pub legacy_padding: [f32; 4],
    pub geoset_color_alpha: [f32; 4],
    pub layer_options: [f32; 4],
    pub texture_transform: [[f32; 4]; 4],
    padding: [f32; 28],
}

const SCENE_UNIFORM_ALIGNMENT: u32 = 256;

#[allow(dead_code)]
pub(crate) struct PreparedScene {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
    pub draws: Vec<PreparedDraw>,
    pub uniforms: Vec<SceneDrawUniform>,
    pub texture_slots: BTreeMap<u32, u32>,
}

pub(crate) struct SceneGpu {
    pub(crate) vertex_buffer: wgpu::Buffer,
    pub(crate) index_buffer: wgpu::Buffer,
    #[allow(dead_code)]
    pub(crate) uniform_buffer: wgpu::Buffer,
    pub(crate) uniform_bind_group: wgpu::BindGroup,
}

#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OffscreenSceneRgba {
    pub width: u32,
    pub height: u32,
    pub adapter: wgpu::AdapterInfo,
    /// Tightly packed RGBA8 rows in top-left origin order.
    pub rgba: Vec<u8>,
}

#[allow(dead_code)]
const OFFSCREEN_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;
#[allow(dead_code)]
const OFFSCREEN_DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

#[allow(dead_code)]
pub async fn render_scene_offscreen(
    input: OffscreenSceneInput<'_>,
    options: OffscreenSceneOptions,
) -> Result<OffscreenSceneRgba, MdlError> {
    validate_offscreen_options(options)?;
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
        backends: wgpu::Backends::DX12,
        ..Default::default()
    });
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: None,
            force_fallback_adapter: false,
        })
        .await
        .map_err(|error| MdlError::new("renderer-offscreen-adapter-unavailable").push_std(error))?;
    let adapter_info = adapter.get_info();
    if adapter_info.backend != wgpu::Backend::Dx12 {
        return Err(MdlError::new("renderer-offscreen-backend-mismatch")
            .with_arg("expected", "Dx12")
            .with_arg("actual", format!("{:?}", adapter_info.backend)));
    }
    let (device, queue) = adapter
        .request_device(&wgpu::DeviceDescriptor {
            label: Some("MDLVis offscreen ScenePacket"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::default(),
            memory_hints: wgpu::MemoryHints::MemoryUsage,
            ..Default::default()
        })
        .await
        .map_err(|error| MdlError::new("renderer-offscreen-device-error").push_std(error))?;
    let max_dimension = device.limits().max_texture_dimension_2d;
    if options.width > max_dimension || options.height > max_dimension {
        return Err(MdlError::new("renderer-offscreen-device-size-limit")
            .with_arg("width", options.width)
            .with_arg("height", options.height)
            .with_arg("max", max_dimension));
    }

    device.push_error_scope(wgpu::ErrorFilter::Validation);
    let mut prepared = prepare_scene(
        input.packet,
        SceneView {
            eye: options.eye,
            forward: options.forward,
        },
    )?;
    apply_texture_metadata(&mut prepared, input.packet, input.textures)?;
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("MDLVis ScenePacket shader"),
        source: wgpu::ShaderSource::Wgsl(include_str!("../shader.wgsl").into()),
    });
    let camera_layout = scene_camera_layout(&device);
    let texture_layout = scene_texture_layout(&device);
    let legacy_layout = scene_legacy_material_layout(&device);
    let scene_layout = scene_dynamic_material_layout(&device);
    let pipelines = all_scene_pipeline_states()
        .into_iter()
        .map(|state| {
            let pipeline = create_scene_pipeline(
                &device,
                &shader,
                OFFSCREEN_FORMAT,
                &camera_layout,
                &texture_layout,
                &legacy_layout,
                &scene_layout,
                state,
            );
            (state, pipeline)
        })
        .collect::<Vec<_>>();
    let camera_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Offscreen camera"),
        contents: bytemuck::cast_slice(&options.view_proj),
        usage: wgpu::BufferUsages::UNIFORM,
    });
    let camera_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("Offscreen camera bind group"),
        layout: &camera_layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: camera_buffer.as_entire_binding(),
        }],
    });
    let offscreen_scene_textures = upload_resolved_textures(
        &device,
        &queue,
        &texture_layout,
        input.packet,
        input.textures,
    )?;
    let legacy_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Offscreen legacy material"),
        contents: bytemuck::cast_slice(&[[0.0_f32; 12]]),
        usage: wgpu::BufferUsages::UNIFORM,
    });
    let legacy_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("Offscreen legacy material bind group"),
        layout: &legacy_layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: legacy_buffer.as_entire_binding(),
        }],
    });
    let scene_gpu = upload_scene_gpu(&device, &prepared, &scene_layout)?;
    let color = offscreen_texture(
        &device,
        "Offscreen ScenePacket color",
        options.width,
        options.height,
        OFFSCREEN_FORMAT,
        wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
    );
    let color_view = color.create_view(&wgpu::TextureViewDescriptor::default());
    let depth = offscreen_texture(
        &device,
        "Offscreen ScenePacket depth",
        options.width,
        options.height,
        OFFSCREEN_DEPTH_FORMAT,
        wgpu::TextureUsages::RENDER_ATTACHMENT,
    );
    let depth_view = depth.create_view(&wgpu::TextureViewDescriptor::default());
    let bytes_per_row = options
        .width
        .checked_mul(4)
        .ok_or_else(|| MdlError::new("renderer-offscreen-size-overflow"))?;
    let padded_bytes_per_row = align_to(bytes_per_row, wgpu::COPY_BYTES_PER_ROW_ALIGNMENT)?;
    let readback_size = u64::from(padded_bytes_per_row)
        .checked_mul(u64::from(options.height))
        .ok_or_else(|| MdlError::new("renderer-offscreen-size-overflow"))?;
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Offscreen ScenePacket readback"),
        size: readback_size,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("Offscreen ScenePacket encoder"),
    });
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Offscreen ScenePacket pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &color_view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: f64::from(options.clear[0]),
                        g: f64::from(options.clear[1]),
                        b: f64::from(options.clear[2]),
                        a: f64::from(options.clear[3]),
                    }),
                    store: wgpu::StoreOp::Store,
                },
                depth_slice: None,
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &depth_view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Discard,
                }),
                stencil_ops: None,
            }),
            occlusion_query_set: None,
            timestamp_writes: None,
        });
        crate::renderer::render::record_prepared_scene(
            &mut pass,
            &prepared,
            &scene_gpu,
            &pipelines,
            crate::renderer::render::ScenePassBindings {
                camera: &camera_bind_group,
                white_texture: &offscreen_scene_textures.white_bind_group,
                textures: &offscreen_scene_textures.bind_groups,
                legacy_material: &legacy_bind_group,
                scene_material: &scene_gpu.uniform_bind_group,
            },
            &[],
        )?;
    }
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: &color,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &readback,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(padded_bytes_per_row),
                rows_per_image: Some(options.height),
            },
        },
        wgpu::Extent3d {
            width: options.width,
            height: options.height,
            depth_or_array_layers: 1,
        },
    );
    let submission = queue.submit(Some(encoder.finish()));
    let (sender, receiver) = std::sync::mpsc::sync_channel(1);
    let slice = readback.slice(..);
    slice.map_async(wgpu::MapMode::Read, move |result| {
        let _ = sender.send(result);
    });
    device
        .poll(wgpu::PollType::Wait {
            submission_index: Some(submission),
            timeout: Some(std::time::Duration::from_secs(10)),
        })
        .map_err(|error| MdlError::new("renderer-offscreen-poll-error").push_std(error))?;
    receiver
        .recv_timeout(std::time::Duration::from_secs(1))
        .map_err(|error| MdlError::new("renderer-offscreen-map-timeout").push_std(error))?
        .map_err(|error| MdlError::new("renderer-offscreen-map-error").push_std(error))?;
    let mapped = slice.get_mapped_range();
    let tight_size = usize::try_from(
        u64::from(bytes_per_row)
            .checked_mul(u64::from(options.height))
            .ok_or_else(|| MdlError::new("renderer-offscreen-size-overflow"))?,
    )
    .map_err(|_| MdlError::new("renderer-offscreen-size-overflow"))?;
    let mut rgba = Vec::with_capacity(tight_size);
    for row in mapped.chunks_exact(padded_bytes_per_row as usize) {
        rgba.extend_from_slice(&row[..bytes_per_row as usize]);
    }
    drop(mapped);
    readback.unmap();
    if let Some(error) = device.pop_error_scope().await {
        return Err(MdlError::new("renderer-offscreen-validation-error").with_arg("error", error));
    }
    Ok(OffscreenSceneRgba {
        width: options.width,
        height: options.height,
        adapter: adapter_info,
        rgba,
    })
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OffscreenSceneOptions {
    pub width: u32,
    pub height: u32,
    pub view_proj: [[f32; 4]; 4],
    pub eye: [f32; 3],
    pub forward: [f32; 3],
    pub clear: [f32; 4],
}

pub struct OffscreenSceneInput<'a> {
    pub packet: &'a ScenePacket,
    pub textures: &'a [ResolvedSceneTexture],
}

pub(crate) struct SceneTextureGpuSet {
    pub(crate) white_bind_group: wgpu::BindGroup,
    pub(crate) bind_groups: Vec<wgpu::BindGroup>,
    _textures: Vec<wgpu::Texture>,
    _views: Vec<wgpu::TextureView>,
    _samplers: Vec<wgpu::Sampler>,
}

impl Default for OffscreenSceneOptions {
    fn default() -> Self {
        Self {
            width: 256,
            height: 256,
            view_proj: nalgebra_glm::Mat4::identity().into(),
            eye: [0.0, 0.0, 10.0],
            forward: [0.0, 0.0, -1.0],
            clear: [0.0, 0.0, 0.0, 0.0],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
#[allow(dead_code)]
pub(crate) struct SceneView {
    pub eye: [f32; 3],
    pub forward: [f32; 3],
}

#[allow(dead_code)]
pub(crate) fn prepare_scene(
    packet: &ScenePacket,
    view: SceneView,
) -> Result<PreparedScene, MdlError> {
    packet.validate()?;
    if !view
        .eye
        .iter()
        .chain(view.forward.iter())
        .all(|value| value.is_finite())
    {
        return Err(MdlError::new("renderer-non-finite-view"));
    }
    let forward_length = view.forward.iter().map(|value| value * value).sum::<f32>();
    if forward_length <= f32::EPSILON {
        return Err(MdlError::new("renderer-invalid-view-forward"));
    }

    let texture_slots = packet
        .textures
        .iter()
        .enumerate()
        .map(|(slot, request)| {
            Ok((
                request.index.0,
                u32::try_from(slot).map_err(|_| MdlError::new("renderer-too-many-textures"))?,
            ))
        })
        .collect::<Result<BTreeMap<_, _>, MdlError>>()?;
    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    let mut draws = Vec::new();
    let mut uniforms = Vec::new();

    for draw in &packet.draws {
        let visibility_threshold = match draw.filter_mode {
            crate::scene::types::SceneFilterMode::Transparent => 0.75,
            _ => 0.01,
        };
        if draw.geoset_alpha <= 0.01 || draw.layer_alpha < visibility_threshold {
            continue;
        }
        let mesh = packet.meshes.get(draw.mesh as usize).ok_or_else(|| {
            MdlError::new("renderer-invalid-mesh-index").with_arg("mesh", draw.mesh)
        })?;
        let draw_vertices = prepare_draw_vertices(mesh, draw)?;
        let vertex_start = u32::try_from(vertices.len())
            .map_err(|_| MdlError::new("renderer-vertex-offset-out-of-range"))?;
        vertices.extend(draw_vertices);
        let texture_slot = draw
            .texture
            .map(|texture| {
                texture_slots.get(&texture.0).copied().ok_or_else(|| {
                    MdlError::new("renderer-missing-texture-slot").with_arg("texture", texture.0)
                })
            })
            .transpose()?;
        let uniform = SceneDrawUniform {
            legacy_team_color: [view.eye[0], view.eye[1], view.eye[2], 0.0],
            legacy_material: [0.0; 4],
            legacy_padding: [0.0; 4],
            geoset_color_alpha: [
                draw.geoset_color[0],
                draw.geoset_color[1],
                draw.geoset_color[2],
                draw.geoset_alpha,
            ],
            layer_options: [
                draw.layer_alpha,
                draw.filter_mode as u32 as f32,
                scene_flag_bits(draw.render_state, draw.material_state) as f32,
                texture_slot.unwrap_or(u32::MAX) as f32,
            ],
            texture_transform: texture_matrix(draw.texture_transform)?,
            padding: [0.0; 28],
        };
        let pipeline = ScenePipelineState::from_scene(draw.filter_mode, draw.render_state);
        let draw_pass_rank = pass_rank(draw.filter_mode);
        let split_triangles = draw_pass_rank > 0
            || draw.material_state.sort_primitives_far_z
            || draw.sort_class == crate::scene::types::SceneSortClass::BackToFrontTriangles;
        if split_triangles {
            for (triangle_ordinal, triangle) in mesh.triangles.iter().enumerate() {
                let index_start = u32::try_from(indices.len())
                    .map_err(|_| MdlError::new("renderer-index-offset-out-of-range"))?;
                for index in triangle {
                    indices.push(
                        vertex_start
                            .checked_add(*index)
                            .ok_or_else(|| MdlError::new("renderer-index-offset-out-of-range"))?,
                    );
                }
                let uniform_offset = push_uniform(&mut uniforms, uniform)?;
                draws.push(PreparedDraw {
                    source_ordinal: draw.source_ordinal,
                    priority_plane: draw.priority_plane,
                    pass_rank: draw_pass_rank,
                    index_start,
                    index_count: 3,
                    texture: draw.texture,
                    texture_slot,
                    bounds_center: render_position(triangle_center(mesh, *triangle)?),
                    sort_class: crate::scene::types::SceneSortClass::BackToFrontTriangles,
                    triangle_ordinal: u32::try_from(triangle_ordinal)
                        .map_err(|_| MdlError::new("renderer-triangle-ordinal-out-of-range"))?,
                    geoset: draw.geoset.0,
                    pipeline,
                    uniform_offset,
                });
            }
        } else {
            let index_start = u32::try_from(indices.len())
                .map_err(|_| MdlError::new("renderer-index-offset-out-of-range"))?;
            for triangle in &mesh.triangles {
                for index in triangle {
                    indices.push(
                        vertex_start
                            .checked_add(*index)
                            .ok_or_else(|| MdlError::new("renderer-index-offset-out-of-range"))?,
                    );
                }
            }
            let index_count = u32::try_from(indices.len())
                .ok()
                .and_then(|end| end.checked_sub(index_start))
                .ok_or_else(|| MdlError::new("renderer-index-count-out-of-range"))?;
            let uniform_offset = push_uniform(&mut uniforms, uniform)?;
            draws.push(PreparedDraw {
                source_ordinal: draw.source_ordinal,
                priority_plane: draw.priority_plane,
                pass_rank: draw_pass_rank,
                index_start,
                index_count,
                texture: draw.texture,
                texture_slot,
                bounds_center: render_position(mesh.bounds.center),
                sort_class: draw.sort_class,
                triangle_ordinal: 0,
                geoset: draw.geoset.0,
                pipeline,
                uniform_offset,
            });
        }
    }
    sort_draws(&mut draws, view);
    Ok(PreparedScene {
        vertices,
        indices,
        draws,
        uniforms,
        texture_slots,
    })
}

fn apply_texture_metadata(
    prepared: &mut PreparedScene,
    packet: &ScenePacket,
    resolved: &[ResolvedSceneTexture],
) -> Result<(), MdlError> {
    let encodings = resolved
        .iter()
        .map(|texture| (texture.index.0, texture.alpha_encoding))
        .collect::<BTreeMap<_, _>>();
    if encodings.len() != resolved.len()
        || encodings.len() != packet.textures.len()
        || packet
            .textures
            .iter()
            .any(|request| !encodings.contains_key(&request.index.0))
    {
        return Err(MdlError::new("renderer-resolved-texture-set-mismatch"));
    }
    for draw in &prepared.draws {
        let uniform_index = (draw.uniform_offset / SCENE_UNIFORM_ALIGNMENT) as usize;
        let uniform = prepared.uniforms.get_mut(uniform_index).ok_or_else(|| {
            MdlError::new("renderer-invalid-uniform-offset").with_arg("offset", draw.uniform_offset)
        })?;
        uniform.legacy_padding[0] = match draw.texture {
            Some(index) => match encodings.get(&index.0).ok_or_else(|| {
                MdlError::new("renderer-missing-resolved-texture").with_arg("texture", index.0)
            })? {
                CpuAlphaEncoding::Straight => 0.0,
                CpuAlphaEncoding::Premultiplied => f32::from(matches!(
                    draw.pipeline.blend,
                    None | Some((
                        crate::renderer::geoset_render_info::BlendFactor::SrcAlpha,
                        _,
                    ))
                )),
            },
            None => 0.0,
        };
    }
    Ok(())
}

fn push_uniform(
    uniforms: &mut Vec<SceneDrawUniform>,
    uniform: SceneDrawUniform,
) -> Result<u32, MdlError> {
    let offset = uniforms
        .len()
        .checked_mul(SCENE_UNIFORM_ALIGNMENT as usize)
        .and_then(|offset| u32::try_from(offset).ok())
        .ok_or_else(|| MdlError::new("renderer-uniform-offset-out-of-range"))?;
    uniforms.push(uniform);
    Ok(offset)
}

fn sort_draws(draws: &mut [PreparedDraw], view: SceneView) {
    draws.sort_by(|left, right| {
        left.pass_rank
            .cmp(&right.pass_rank)
            .then_with(|| left.priority_plane.cmp(&right.priority_plane))
            .then_with(|| {
                if left.pass_rank > 0
                    || left.sort_class == crate::scene::types::SceneSortClass::BackToFrontTriangles
                    || right.sort_class == crate::scene::types::SceneSortClass::BackToFrontTriangles
                {
                    depth(right.bounds_center, view).total_cmp(&depth(left.bounds_center, view))
                } else {
                    std::cmp::Ordering::Equal
                }
            })
            .then_with(|| left.source_ordinal.cmp(&right.source_ordinal))
            .then_with(|| left.triangle_ordinal.cmp(&right.triangle_ordinal))
    });
}

fn refresh_prepared_scene_view(
    prepared: &mut PreparedScene,
    view: SceneView,
) -> Result<(), MdlError> {
    if !view
        .eye
        .iter()
        .chain(view.forward.iter())
        .all(|value| value.is_finite())
    {
        return Err(MdlError::new("renderer-non-finite-view"));
    }
    let forward_length = view.forward.iter().map(|value| value * value).sum::<f32>();
    if forward_length <= f32::EPSILON {
        return Err(MdlError::new("renderer-invalid-view-forward"));
    }
    for uniform in &mut prepared.uniforms {
        uniform.legacy_team_color = [view.eye[0], view.eye[1], view.eye[2], 0.0];
    }
    sort_draws(&mut prepared.draws, view);
    Ok(())
}

fn depth(point: [f32; 3], view: SceneView) -> f32 {
    (0..3)
        .map(|axis| (point[axis] - view.eye[axis]) * view.forward[axis])
        .sum()
}

fn render_position(position: [f32; 3]) -> [f32; 3] {
    [position[0], -position[1], position[2]]
}

#[cfg(test)]
fn sphere_uv(position: [f32; 3], normal: [f32; 3], eye: [f32; 3]) -> [f32; 2] {
    let position = render_position(position);
    let normal = render_position(normal);
    let view = normalize3(sub3(eye, position)).unwrap_or([0.0, 0.0, 1.0]);
    let normal = normalize3(normal).unwrap_or([0.0, 0.0, 1.0]);
    let incident = [-view[0], -view[1], -view[2]];
    let projection = 2.0 * dot3(incident, normal);
    let reflected = [
        incident[0] - projection * normal[0],
        incident[1] - projection * normal[1],
        incident[2] - projection * normal[2],
    ];
    let denominator = (2.0
        * (reflected[0] * reflected[0]
            + reflected[1] * reflected[1]
            + (reflected[2] + 1.0) * (reflected[2] + 1.0))
            .sqrt())
    .max(0.00001);
    [
        reflected[0] / denominator + 0.5,
        reflected[1] / denominator + 0.5,
    ]
}

#[cfg(test)]
fn sub3(left: [f32; 3], right: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|axis| left[axis] - right[axis])
}

#[cfg(test)]
fn dot3(left: [f32; 3], right: [f32; 3]) -> f32 {
    left.iter()
        .zip(right)
        .map(|(left, right)| left * right)
        .sum()
}

#[cfg(test)]
fn normalize3(value: [f32; 3]) -> Option<[f32; 3]> {
    let length_squared = dot3(value, value);
    (length_squared.is_finite() && length_squared > f32::EPSILON).then(|| {
        let inverse = length_squared.sqrt().recip();
        std::array::from_fn(|axis| value[axis] * inverse)
    })
}

fn triangle_center(
    mesh: &crate::scene::types::SceneMesh,
    triangle: [u32; 3],
) -> Result<[f32; 3], MdlError> {
    let [a, b, c] = triangle.map(|index| {
        mesh.positions
            .get(index as usize)
            .copied()
            .ok_or_else(|| MdlError::new("renderer-invalid-triangle-index"))
    });
    let [a, b, c] = [a?, b?, c?];
    Ok(std::array::from_fn(|axis| {
        (a[axis] + b[axis] + c[axis]) / 3.0
    }))
}

fn scene_flag_bits(
    state: crate::scene::types::SceneRenderState,
    material: crate::scene::types::SceneMaterialState,
) -> u32 {
    u32::from(state.unshaded)
        | (u32::from(state.sphere_env_map) << 1)
        | (u32::from(state.unfogged) << 2)
        | (u32::from(material.constant_color) << 3)
        | (u32::from(material.full_resolution) << 4)
        | (u32::from(material.sort_primitives_far_z) << 5)
}

fn all_scene_pipeline_states() -> Vec<ScenePipelineState> {
    let mut states = Vec::new();
    for filter in [
        crate::scene::types::SceneFilterMode::None,
        crate::scene::types::SceneFilterMode::Transparent,
        crate::scene::types::SceneFilterMode::Blend,
        crate::scene::types::SceneFilterMode::Additive,
        crate::scene::types::SceneFilterMode::AddAlpha,
        crate::scene::types::SceneFilterMode::Modulate,
        crate::scene::types::SceneFilterMode::Modulate2x,
    ] {
        for two_sided in [false, true] {
            for no_depth_test in [false, true] {
                for no_depth_write in [false, true] {
                    let state = ScenePipelineState::from_scene(
                        filter,
                        crate::scene::types::SceneRenderState {
                            two_sided,
                            no_depth_test,
                            no_depth_write,
                            ..Default::default()
                        },
                    );
                    if !states.contains(&state) {
                        states.push(state);
                    }
                }
            }
        }
    }
    states
}

fn validate_offscreen_options(options: OffscreenSceneOptions) -> Result<(), MdlError> {
    if options.width == 0 || options.height == 0 {
        return Err(MdlError::new("renderer-offscreen-invalid-size")
            .with_arg("width", options.width)
            .with_arg("height", options.height));
    }
    if options.width > 16_384 || options.height > 16_384 {
        return Err(MdlError::new("renderer-offscreen-size-limit")
            .with_arg("width", options.width)
            .with_arg("height", options.height));
    }
    if !options
        .view_proj
        .iter()
        .flatten()
        .chain(options.eye.iter())
        .chain(options.forward.iter())
        .chain(options.clear.iter())
        .all(|value| value.is_finite())
    {
        return Err(MdlError::new("renderer-offscreen-non-finite-options"));
    }
    if options
        .clear
        .iter()
        .any(|value| !(0.0..=1.0).contains(value))
    {
        return Err(MdlError::new("renderer-offscreen-invalid-clear"));
    }
    Ok(())
}

fn align_to(value: u32, alignment: u32) -> Result<u32, MdlError> {
    value
        .checked_add(alignment - 1)
        .map(|value| value / alignment * alignment)
        .ok_or_else(|| MdlError::new("renderer-offscreen-size-overflow"))
}

fn scene_camera_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("Scene camera layout"),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::VERTEX,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        }],
    })
}

fn scene_texture_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("Scene texture layout"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ],
    })
}

fn scene_legacy_material_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("Scene legacy material layout"),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        }],
    })
}

fn scene_dynamic_material_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("Scene dynamic material layout"),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: true,
                min_binding_size: std::num::NonZeroU64::new(size_of::<SceneDrawUniform>() as u64),
            },
            count: None,
        }],
    })
}

fn upload_scene_gpu(
    device: &wgpu::Device,
    prepared: &PreparedScene,
    scene_layout: &wgpu::BindGroupLayout,
) -> Result<SceneGpu, MdlError> {
    let vertex_bytes = checked_buffer_bytes::<Vertex>(prepared.vertices.len(), "vertex")?;
    let index_bytes = checked_buffer_bytes::<u32>(prepared.indices.len(), "index")?;
    let uniform_bytes_len =
        checked_buffer_bytes::<SceneDrawUniform>(prepared.uniforms.len(), "uniform")?;
    let limits = device.limits();
    for (owner, bytes) in [
        ("vertex", vertex_bytes),
        ("index", index_bytes),
        (
            "uniform",
            uniform_bytes_len.max(u64::from(SCENE_UNIFORM_ALIGNMENT)),
        ),
    ] {
        if bytes > limits.max_buffer_size {
            return Err(MdlError::new("renderer-scene-buffer-limit")
                .with_arg("owner", owner)
                .with_arg("bytes", bytes)
                .with_arg("max", limits.max_buffer_size));
        }
    }
    if limits.min_uniform_buffer_offset_alignment > SCENE_UNIFORM_ALIGNMENT
        || limits.max_uniform_buffer_binding_size < SCENE_UNIFORM_ALIGNMENT
    {
        return Err(MdlError::new("renderer-scene-uniform-limit")
            .with_arg("alignment", limits.min_uniform_buffer_offset_alignment)
            .with_arg("binding", limits.max_uniform_buffer_binding_size));
    }
    let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Scene vertex buffer"),
        contents: bytemuck::cast_slice(&prepared.vertices),
        usage: wgpu::BufferUsages::VERTEX,
    });
    let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Scene index buffer"),
        contents: bytemuck::cast_slice(&prepared.indices),
        usage: wgpu::BufferUsages::INDEX,
    });
    let uniform_bytes = bytemuck::cast_slice(&prepared.uniforms);
    let uniform_buffer = if uniform_bytes.is_empty() {
        device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Empty scene uniform buffer"),
            size: SCENE_UNIFORM_ALIGNMENT as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
    } else {
        device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Scene uniform buffer"),
            contents: uniform_bytes,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        })
    };
    let uniform_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("Scene uniform bind group"),
        layout: scene_layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                buffer: &uniform_buffer,
                offset: 0,
                size: std::num::NonZeroU64::new(size_of::<SceneDrawUniform>() as u64),
            }),
        }],
    });
    Ok(SceneGpu {
        vertex_buffer,
        index_buffer,
        uniform_buffer,
        uniform_bind_group,
    })
}

fn checked_buffer_bytes<T>(count: usize, owner: &'static str) -> Result<u64, MdlError> {
    count
        .checked_mul(size_of::<T>())
        .and_then(|bytes| u64::try_from(bytes).ok())
        .ok_or_else(|| {
            MdlError::new("renderer-scene-buffer-size-overflow").with_arg("owner", owner)
        })
}

fn offscreen_texture(
    device: &wgpu::Device,
    label: &'static str,
    width: u32,
    height: u32,
    format: wgpu::TextureFormat,
    usage: wgpu::TextureUsages,
) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage,
        view_formats: &[],
    })
}

fn white_texture_bind_group(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    layout: &wgpu::BindGroupLayout,
) -> (
    wgpu::BindGroup,
    wgpu::Texture,
    wgpu::TextureView,
    wgpu::Sampler,
) {
    let texture = offscreen_texture(
        device,
        "Scene white texture",
        1,
        1,
        OFFSCREEN_FORMAT,
        wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
    );
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &[255, 255, 255, 255],
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(4),
            rows_per_image: Some(1),
        },
        wgpu::Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        },
    );
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("Scene repeat sampler"),
        address_mode_u: wgpu::AddressMode::Repeat,
        address_mode_v: wgpu::AddressMode::Repeat,
        address_mode_w: wgpu::AddressMode::Repeat,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    });
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("Scene white texture bind group"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(&sampler),
            },
        ],
    });
    (bind_group, texture, view, sampler)
}

fn upload_resolved_textures(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    layout: &wgpu::BindGroupLayout,
    packet: &ScenePacket,
    resolved: &[ResolvedSceneTexture],
) -> Result<SceneTextureGpuSet, MdlError> {
    let by_index = resolved
        .iter()
        .map(|texture| (texture.index.0, texture))
        .collect::<BTreeMap<_, _>>();
    if by_index.len() != resolved.len() {
        return Err(MdlError::new("renderer-duplicate-resolved-texture"));
    }
    if by_index.len() != packet.textures.len()
        || packet
            .textures
            .iter()
            .any(|request| !by_index.contains_key(&request.index.0))
    {
        return Err(MdlError::new("renderer-resolved-texture-set-mismatch")
            .with_arg("expected", packet.textures.len())
            .with_arg("actual", resolved.len()));
    }
    let (white_bind_group, white_texture, white_view, white_sampler) =
        white_texture_bind_group(device, queue, layout);

    let mut textures = Vec::with_capacity(packet.textures.len());
    let mut views = Vec::with_capacity(packet.textures.len());
    let mut samplers = Vec::with_capacity(packet.textures.len());
    let max_dimension = device.limits().max_texture_dimension_2d;
    for request in &packet.textures {
        let resolved = by_index[&request.index.0];
        let (expected_bytes, bytes_per_row) =
            validate_resolved_texture_data(resolved, max_dimension)?;
        if resolved.rgba.len() != expected_bytes {
            return Err(MdlError::new("renderer-invalid-resolved-texture-bytes")
                .with_arg("texture", resolved.index.0)
                .with_arg("expected", expected_bytes)
                .with_arg("actual", resolved.rgba.len()));
        }
        if resolved.color_space != CpuColorSpace::Srgb {
            return Err(MdlError::new("renderer-unsupported-texture-color-space"));
        }
        // Generated team glow is premultiplied in legacy byte space. Keeping it in
        // an unorm texture makes a 1/255 edge contribute 1/255 to ONE/ONE and lets
        // SrcAlpha pipelines recover straight RGB without an sRGB decode mismatch.
        let texture_format = match resolved.alpha_encoding {
            CpuAlphaEncoding::Straight => wgpu::TextureFormat::Rgba8UnormSrgb,
            CpuAlphaEncoding::Premultiplied => wgpu::TextureFormat::Rgba8Unorm,
        };
        let gpu_texture = offscreen_texture(
            device,
            "Scene resolved texture",
            resolved.width,
            resolved.height,
            texture_format,
            wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        );
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &gpu_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &resolved.rgba,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(bytes_per_row),
                rows_per_image: Some(resolved.height),
            },
            wgpu::Extent3d {
                width: resolved.width,
                height: resolved.height,
                depth_or_array_layers: 1,
            },
        );
        let view = gpu_texture.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Scene resolved texture sampler"),
            address_mode_u: gpu_address_mode(resolved.address_u),
            address_mode_v: gpu_address_mode(resolved.address_v),
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        textures.push(gpu_texture);
        views.push(view);
        samplers.push(sampler);
    }
    let bind_groups = views
        .iter()
        .zip(&samplers)
        .map(|(view, sampler)| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Scene resolved texture bind group"),
                layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(sampler),
                    },
                ],
            })
        })
        .collect();
    Ok(SceneTextureGpuSet {
        white_bind_group,
        bind_groups,
        _textures: std::iter::once(white_texture).chain(textures).collect(),
        _views: std::iter::once(white_view).chain(views).collect(),
        _samplers: std::iter::once(white_sampler).chain(samplers).collect(),
    })
}

fn validate_resolved_texture_data(
    resolved: &ResolvedSceneTexture,
    max_dimension: u32,
) -> Result<(usize, u32), MdlError> {
    if resolved.width == 0
        || resolved.height == 0
        || resolved.width > max_dimension
        || resolved.height > max_dimension
    {
        return Err(MdlError::new("renderer-invalid-resolved-texture-size")
            .with_arg("texture", resolved.index.0)
            .with_arg("width", resolved.width)
            .with_arg("height", resolved.height)
            .with_arg("max", max_dimension));
    }
    let bytes_per_row = resolved
        .width
        .checked_mul(4)
        .ok_or_else(|| MdlError::new("renderer-resolved-texture-size-overflow"))?;
    let expected_bytes = usize::try_from(
        u64::from(bytes_per_row)
            .checked_mul(u64::from(resolved.height))
            .ok_or_else(|| MdlError::new("renderer-resolved-texture-size-overflow"))?,
    )
    .map_err(|_| MdlError::new("renderer-resolved-texture-size-overflow"))?;
    Ok((expected_bytes, bytes_per_row))
}

fn gpu_address_mode(mode: TextureAddressMode) -> wgpu::AddressMode {
    match mode {
        TextureAddressMode::Clamp => wgpu::AddressMode::ClampToEdge,
        TextureAddressMode::Repeat => wgpu::AddressMode::Repeat,
    }
}

#[allow(clippy::too_many_arguments)]
fn create_scene_pipeline(
    device: &wgpu::Device,
    shader: &wgpu::ShaderModule,
    surface_format: wgpu::TextureFormat,
    camera_layout: &wgpu::BindGroupLayout,
    texture_layout: &wgpu::BindGroupLayout,
    legacy_material_layout: &wgpu::BindGroupLayout,
    scene_material_layout: &wgpu::BindGroupLayout,
    state: ScenePipelineState,
) -> wgpu::RenderPipeline {
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("Scene Pipeline Layout"),
        bind_group_layouts: &[
            camera_layout,
            texture_layout,
            legacy_material_layout,
            scene_material_layout,
        ],
        push_constant_ranges: &[],
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("Scene Pipeline"),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs_scene"),
            buffers: &[Vertex::desc()],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some("fs_scene"),
            targets: &[Some(wgpu::ColorTargetState {
                format: surface_format,
                blend: state.blend.map(|(source, destination)| wgpu::BlendState {
                    color: wgpu::BlendComponent {
                        src_factor: wgpu_blend_factor(source),
                        dst_factor: wgpu_blend_factor(destination),
                        operation: wgpu::BlendOperation::Add,
                    },
                    alpha: wgpu::BlendComponent {
                        src_factor: wgpu_blend_factor(source),
                        dst_factor: wgpu_blend_factor(destination),
                        operation: wgpu::BlendOperation::Add,
                    },
                }),
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            strip_index_format: None,
            front_face: SCENE_FRONT_FACE,
            cull_mode: state.cull_mode(),
            polygon_mode: wgpu::PolygonMode::Fill,
            unclipped_depth: false,
            conservative: false,
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: wgpu::TextureFormat::Depth32Float,
            depth_write_enabled: state.depth_write,
            depth_compare: if state.depth_always {
                wgpu::CompareFunction::Always
            } else {
                wgpu::CompareFunction::LessEqual
            },
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }),
        multisample: wgpu::MultisampleState::default(),
        multiview: None,
        cache: None,
    })
}

fn wgpu_blend_factor(value: crate::renderer::geoset_render_info::BlendFactor) -> wgpu::BlendFactor {
    match value {
        crate::renderer::geoset_render_info::BlendFactor::Zero => wgpu::BlendFactor::Zero,
        crate::renderer::geoset_render_info::BlendFactor::One => wgpu::BlendFactor::One,
        crate::renderer::geoset_render_info::BlendFactor::SrcAlpha => wgpu::BlendFactor::SrcAlpha,
        crate::renderer::geoset_render_info::BlendFactor::OneMinusSrcAlpha => {
            wgpu::BlendFactor::OneMinusSrcAlpha
        }
        crate::renderer::geoset_render_info::BlendFactor::Src => wgpu::BlendFactor::Src,
        crate::renderer::geoset_render_info::BlendFactor::Dst => wgpu::BlendFactor::Dst,
    }
}

impl Renderer {
    pub async fn new(window: &Window) -> Result<Self, MdlError> {
        let size = window.inner_size();

        // The instance is a handle to our GPU
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });

        // The surface is the part of the window we draw to
        let surface = instance.create_surface(window)?;
        let surface =
            unsafe { std::mem::transmute::<wgpu::Surface<'_>, wgpu::Surface<'static>>(surface) };

        // Adapter is a handle to the GPU
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::default(),
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .unwrap();

        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                required_features: wgpu::Features::POLYGON_MODE_LINE, // Required for wireframe mode
                required_limits: wgpu::Limits::default(),
                label: None,
                memory_hints: wgpu::MemoryHints::default(),
                ..Default::default()
            })
            .await
            .unwrap();

        let surface_caps = surface.get_capabilities(&adapter);
        let surface_format = surface_caps
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
            .unwrap_or(surface_caps.formats[0]);

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: size.width,
            height: size.height,
            present_mode: surface_caps.present_modes[0],
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);

        // Create dummy buffers for now
        let vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Vertex Buffer"),
            size: 0,
            usage: wgpu::BufferUsages::VERTEX,
            mapped_at_creation: false,
        });

        let index_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Index Buffer"),
            size: 0,
            usage: wgpu::BufferUsages::INDEX,
            mapped_at_creation: false,
        });

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../shader.wgsl").into()),
        });

        // Create camera uniform buffer
        let camera_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Camera Buffer"),
            size: 64, // mat4x4<f32>
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let camera_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Camera Bind Group Layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });

        let camera_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Camera Bind Group"),
            layout: &camera_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera_buffer.as_entire_binding(),
            }],
        });

        // Create material uniform buffer
        let material_uniform =
            MaterialUniform::new([1.0, 0.0, 0.0], 0, false, FilterMode::None, 1.0, 0);

        let material_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Material Buffer"),
            contents: bytemuck::cast_slice(&[material_uniform]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let material_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Material Bind Group Layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });

        let material_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Material Bind Group"),
            layout: &material_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: material_buffer.as_entire_binding(),
            }],
        });

        let scene_material_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Scene Material Bind Group Layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: true,
                        min_binding_size: std::num::NonZeroU64::new(
                            size_of::<SceneDrawUniform>() as u64
                        ),
                    },
                    count: None,
                }],
            });

        // Create default white 1x1 texture (for non-team-color materials)
        let texture_size = wgpu::Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        };

        let diffuse_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Default White Texture"),
            size: texture_size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        // Write WHITE pixel data (opaque white for normal materials)
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &diffuse_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &[255, 255, 255, 255], // RGBA white opaque
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4),
                rows_per_image: Some(1),
            },
            texture_size,
        );

        let diffuse_texture_view =
            diffuse_texture.create_view(&wgpu::TextureViewDescriptor::default());
        let diffuse_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Default Sampler"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            address_mode_w: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        // Create texture bind group layout
        let texture_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Texture Bind Group Layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
            });

        // Create initial bind group for white texture
        let initial_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("White Texture Bind Group"),
            layout: &texture_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&diffuse_texture_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&diffuse_sampler),
                },
            ],
        });

        // Initialize texture bind groups vector with the white texture
        let texture_bind_groups = vec![initial_bind_group];

        // Create team color texture (red by default to match team_color value)
        let team_color_data = [255u8, 0, 0, 255]; // Red - matches team_color: [1.0, 0.0, 0.0]
        let team_color_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Team Color Texture"),
            size: wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &team_color_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &team_color_data,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4),
                rows_per_image: Some(1),
            },
            wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
        );

        let render_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Render Pipeline Layout"),
                bind_group_layouts: &[
                    &camera_bind_group_layout,
                    &texture_bind_group_layout,
                    &material_bind_group_layout,
                ],
                push_constant_ranges: &[],
            });

        // Create separate layout for lines (no textures)
        let line_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Line Pipeline Layout"),
            bind_group_layouts: &[&camera_bind_group_layout],
            push_constant_ranges: &[],
        });

        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Render Pipeline"),
            layout: Some(&render_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[Vertex::desc()],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: Some(wgpu::BlendState::REPLACE), // Opaque materials replace background
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Cw,
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill, // Filled mode with checkerboard texture
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview: None,
            cache: None,
        });

        // Wireframe pipeline - same as render pipeline but with Line mode
        let wireframe_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Wireframe Pipeline"),
            layout: Some(&render_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[Vertex::desc()],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Cw,
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Line, // Wireframe mode!
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview: None,
            cache: None,
        });

        // Create transparent rendering pipeline (depth write OFF, depth test ON)
        let transparent_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Transparent Pipeline"),
            layout: Some(&render_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[Vertex::desc()],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Cw,
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: false, // Don't write depth for transparent materials
                depth_compare: wgpu::CompareFunction::LessEqual, // Use LessEqual to allow same-depth layering
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview: None,
            cache: None,
        });

        // Create wireframe transparent rendering pipeline (same as transparent but with wireframe mode)
        let wireframe_transparent_pipeline =
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("Wireframe Transparent Pipeline"),
                layout: Some(&render_pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    buffers: &[Vertex::desc()],
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs_main"),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: config.format,
                        blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    strip_index_format: None,
                    front_face: wgpu::FrontFace::Cw,
                    cull_mode: None,
                    polygon_mode: wgpu::PolygonMode::Line, // Wireframe mode!
                    unclipped_depth: false,
                    conservative: false,
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: wgpu::TextureFormat::Depth32Float,
                    depth_write_enabled: false, // Don't write depth for transparent materials
                    depth_compare: wgpu::CompareFunction::LessEqual, // Use LessEqual to allow same-depth layering
                    stencil: wgpu::StencilState::default(),
                    bias: wgpu::DepthBiasState::default(),
                }),
                multisample: wgpu::MultisampleState {
                    count: 1,
                    mask: !0,
                    alpha_to_coverage_enabled: false,
                },
                multiview: None,
                cache: None,
            });

        // Create additive rendering pipeline (GL_ONE, GL_ONE) for glow effects
        let additive_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Additive Pipeline"),
            layout: Some(&render_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[Vertex::desc()],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: Some(wgpu::BlendState {
                        color: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::One,
                            dst_factor: wgpu::BlendFactor::One,
                            operation: wgpu::BlendOperation::Add,
                        },
                        alpha: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::One,
                            dst_factor: wgpu::BlendFactor::One,
                            operation: wgpu::BlendOperation::Add,
                        },
                    }),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Cw,
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: false, // Don't write depth for additive materials
                depth_compare: wgpu::CompareFunction::Less,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview: None,
            cache: None,
        });

        // Create wireframe additive rendering pipeline
        let wireframe_additive_pipeline =
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("Wireframe Additive Pipeline"),
                layout: Some(&render_pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    buffers: &[Vertex::desc()],
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs_main"),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: config.format,
                        blend: Some(wgpu::BlendState {
                            color: wgpu::BlendComponent {
                                src_factor: wgpu::BlendFactor::One,
                                dst_factor: wgpu::BlendFactor::One,
                                operation: wgpu::BlendOperation::Add,
                            },
                            alpha: wgpu::BlendComponent {
                                src_factor: wgpu::BlendFactor::One,
                                dst_factor: wgpu::BlendFactor::One,
                                operation: wgpu::BlendOperation::Add,
                            },
                        }),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    strip_index_format: None,
                    front_face: wgpu::FrontFace::Cw,
                    cull_mode: None,
                    polygon_mode: wgpu::PolygonMode::Line, // Wireframe mode!
                    unclipped_depth: false,
                    conservative: false,
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: wgpu::TextureFormat::Depth32Float,
                    depth_write_enabled: false,
                    depth_compare: wgpu::CompareFunction::Less,
                    stencil: wgpu::StencilState::default(),
                    bias: wgpu::DepthBiasState::default(),
                }),
                multisample: wgpu::MultisampleState {
                    count: 1,
                    mask: !0,
                    alpha_to_coverage_enabled: false,
                },
                multiview: None,
                cache: None,
            });

        // Create line rendering pipeline
        let line_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Line Pipeline"),
            layout: Some(&line_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_line"),
                buffers: &[LineVertex::desc()],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_line"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::LineList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Cw,
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: false, // Don't write to depth buffer so skeleton is always visible
                depth_compare: wgpu::CompareFunction::Always, // Always pass depth test
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview: None,
            cache: None,
        });

        // Create axes and grid
        let mut line_vertices = Vec::new();

        // Axes (thick main lines from -200 to 200)
        // X axis - Red (right)
        line_vertices.push(LineVertex {
            position: [-200.0, 0.0, 0.0],
            color: [1.0, 0.0, 0.0],
        });
        line_vertices.push(LineVertex {
            position: [200.0, 0.0, 0.0],
            color: [1.0, 0.0, 0.0],
        });
        // Y axis - Green (forward/depth)
        line_vertices.push(LineVertex {
            position: [0.0, -200.0, 0.0],
            color: [0.0, 1.0, 0.0],
        });
        line_vertices.push(LineVertex {
            position: [0.0, 200.0, 0.0],
            color: [0.0, 1.0, 0.0],
        });
        // Z axis - Blue (up)
        line_vertices.push(LineVertex {
            position: [0.0, 0.0, -200.0],
            color: [0.0, 0.0, 1.0],
        });
        line_vertices.push(LineVertex {
            position: [0.0, 0.0, 200.0],
            color: [0.0, 0.0, 1.0],
        });

        // Extra thick endings for axes
        line_vertices.push(LineVertex {
            position: [0.0, 0.0, 200.0],
            color: [0.0, 0.0, 1.0],
        });
        line_vertices.push(LineVertex {
            position: [0.0, 0.0, 210.0],
            color: [0.0, 0.0, 1.0],
        });
        line_vertices.push(LineVertex {
            position: [200.0, 0.0, 0.0],
            color: [1.0, 0.0, 0.0],
        });
        line_vertices.push(LineVertex {
            position: [210.0, 0.0, 0.0],
            color: [1.0, 0.0, 0.0],
        });
        line_vertices.push(LineVertex {
            position: [0.0, 200.0, 0.0],
            color: [0.0, 1.0, 0.0],
        });
        line_vertices.push(LineVertex {
            position: [0.0, 210.0, 0.0],
            color: [0.0, 1.0, 0.0],
        });

        // Grid - XY plane (gray, low density) - ground plane
        for i in -32..=32 {
            let pos = i as f32 * 8.0;
            line_vertices.push(LineVertex {
                position: [pos, -256.0, 0.0],
                color: [0.5, 0.5, 0.5],
            });
            line_vertices.push(LineVertex {
                position: [pos, 256.0, 0.0],
                color: [0.5, 0.5, 0.5],
            });
            line_vertices.push(LineVertex {
                position: [-256.0, pos, 0.0],
                color: [0.5, 0.5, 0.5],
            });
            line_vertices.push(LineVertex {
                position: [256.0, pos, 0.0],
                color: [0.5, 0.5, 0.5],
            });
        }

        // Grid - XY plane (black, high density every 64 units)
        for i in -4..=4 {
            let pos = i as f32 * 64.0;
            line_vertices.push(LineVertex {
                position: [pos, -256.0, 0.0],
                color: [0.0, 0.0, 0.0],
            });
            line_vertices.push(LineVertex {
                position: [pos, 256.0, 0.0],
                color: [0.0, 0.0, 0.0],
            });
            line_vertices.push(LineVertex {
                position: [-256.0, pos, 0.0],
                color: [0.0, 0.0, 0.0],
            });
            line_vertices.push(LineVertex {
                position: [256.0, pos, 0.0],
                color: [0.0, 0.0, 0.0],
            });
        }

        let line_vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Line Vertex Buffer"),
            contents: bytemuck::cast_slice(&line_vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let num_lines = line_vertices.len() as u32;

        // Create empty skeleton buffer initially
        let skeleton_vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Skeleton Vertex Buffer"),
            size: 0,
            usage: wgpu::BufferUsages::VERTEX,
            mapped_at_creation: false,
        });

        // Create empty bounding box buffer initially
        let bounding_box_vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Bounding Box Vertex Buffer"),
            size: 0,
            usage: wgpu::BufferUsages::VERTEX,
            mapped_at_creation: false,
        });

        // Initialize egui
        let egui_renderer = egui_wgpu::Renderer::new(&device, config.format, Default::default());

        let scene_pipelines = all_scene_pipeline_states()
            .into_iter()
            .map(|state| {
                let pipeline = create_scene_pipeline(
                    &device,
                    &shader,
                    config.format,
                    &camera_bind_group_layout,
                    &texture_bind_group_layout,
                    &material_bind_group_layout,
                    &scene_material_bind_group_layout,
                    state,
                );
                (state, pipeline)
            })
            .collect();

        Ok(Self {
            surface,
            device,
            queue,
            config,
            render_pipeline,
            wireframe_pipeline,
            transparent_pipeline,
            wireframe_transparent_pipeline,
            additive_pipeline,
            wireframe_additive_pipeline,
            line_pipeline,
            vertex_buffer,
            index_buffer,
            num_indices: 0,
            geosets: Vec::new(),
            materials: Vec::new(),
            textures: Vec::new(),
            line_vertex_buffer,
            num_lines,
            skeleton_vertex_buffer,
            num_skeleton_lines: 0,
            bounding_box_vertex_buffer,
            num_bounding_box_lines: 0,
            camera_buffer,
            camera_bind_group,
            texture_bind_groups,
            texture_views: Vec::new(),
            texture_bind_group_layout,
            material_buffer,
            material_bind_group,
            white_texture_view: diffuse_texture_view,
            white_texture_sampler: diffuse_sampler,
            team_color: [1.0, 0.0, 0.0],       // Red by default
            grid_major_color: [0.2, 0.2, 0.2], // Dark gray major grid
            grid_minor_color: [0.4, 0.4, 0.4], // Light gray minor grid
            skybox_color: [0.3, 0.5, 0.8],     // Light blue skybox
            camera: CameraState::new(
                0.0,                         // yaw: front view
                std::f32::consts::PI * 0.15, // pitch: 27 degrees down
                500.0,                       // distance
                [0.0, 0.0, 0.0],             // target: origin
            ),
            model_center: [0.0, 0.0, 0.0],
            egui_renderer,
            view_proj_matrix: nalgebra_glm::Mat4::identity(),
            original_vertices: Vec::new(),
            model: None,
            scene_prepared: None,
            scene_gpu: None,
            scene_pipelines,
            scene_texture_resources: None,
            scene_material_bind_group_layout,
            scene_error: None,
        })
    }

    pub fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        if new_size.width > 0 && new_size.height > 0 {
            self.config.width = new_size.width;
            self.config.height = new_size.height;
            self.surface.configure(&self.device, &self.config);
        }
    }

    #[allow(dead_code)]
    pub fn update_scene(&mut self, packet: &ScenePacket) -> Result<(), MdlError> {
        if !packet.textures.is_empty() {
            return Err(MdlError::new("renderer-scene-textures-required")
                .with_arg("count", packet.textures.len()));
        }
        self.update_scene_with_textures(packet, &[])
    }

    #[allow(dead_code)]
    pub fn update_scene_with_textures(
        &mut self,
        packet: &ScenePacket,
        resolved: &[ResolvedSceneTexture],
    ) -> Result<(), MdlError> {
        let eye = [
            self.camera.target[0]
                + self.camera.distance * self.camera.yaw.cos() * self.camera.pitch.cos(),
            self.camera.target[1]
                + self.camera.distance * self.camera.yaw.sin() * self.camera.pitch.cos(),
            self.camera.target[2] + self.camera.distance * self.camera.pitch.sin(),
        ];
        let forward = std::array::from_fn(|axis| self.camera.target[axis] - eye[axis]);
        let mut prepared = prepare_scene(packet, SceneView { eye, forward })?;
        apply_texture_metadata(&mut prepared, packet, resolved)?;
        let resources = upload_resolved_textures(
            &self.device,
            &self.queue,
            &self.texture_bind_group_layout,
            packet,
            resolved,
        )?;
        let gpu = upload_scene_gpu(
            &self.device,
            &prepared,
            &self.scene_material_bind_group_layout,
        )?;
        self.scene_texture_resources = Some(resources);
        self.scene_gpu = Some(gpu);
        self.scene_prepared = Some(prepared);
        self.scene_error = None;
        Ok(())
    }

    #[allow(dead_code)]
    pub fn clear_scene(&mut self) {
        self.scene_prepared = None;
        self.scene_gpu = None;
        self.scene_texture_resources = None;
        self.scene_error = None;
    }

    pub(crate) fn refresh_scene_view(&mut self, view: SceneView) -> Result<(), MdlError> {
        let Some(prepared) = &mut self.scene_prepared else {
            return Ok(());
        };
        refresh_prepared_scene_view(prepared, view)?;
        let gpu = self
            .scene_gpu
            .as_ref()
            .ok_or_else(|| MdlError::new("renderer-missing-scene-gpu"))?;
        self.queue.write_buffer(
            &gpu.uniform_buffer,
            0,
            bytemuck::cast_slice(&prepared.uniforms),
        );
        Ok(())
    }

    #[allow(dead_code)]
    pub fn take_scene_error(&mut self) -> Option<MdlError> {
        self.scene_error.take()
    }

    pub fn update_model(&mut self, model: &Model) {
        if model.geosets.is_empty() {
            return;
        }

        // Store model for animation
        self.model = Some(model.clone());

        let mut all_vertices: Vec<Vertex> = Vec::new();
        let mut all_indices: Vec<u16> = Vec::new();
        let mut geosets_info: Vec<GeosetRenderInfo> = Vec::new();

        for (geoset_idx, geoset) in model.geosets.iter().enumerate() {
            let vertex_offset = all_vertices.len() as u32;
            let index_start = all_indices.len() as u32;

            // Add vertices from this geoset with UV coordinates
            let primary_tex_coords = geoset.tex_coord_sets.first();
            for i in 0..geoset.vertices.len() {
                let uv = primary_tex_coords
                    .and_then(|tex_coords| tex_coords.get(i))
                    .map(|tex_coord| tex_coord.uv)
                    .unwrap_or([0.0, 0.0]);

                all_vertices.push(Vertex {
                    position: geoset.vertices[i].position,
                    normal: if i < geoset.normals.len() {
                        geoset.normals[i].normal
                    } else {
                        [0.0, 0.0, 1.0] // Default normal
                    },
                    uv,
                });
            }

            // Add indices from this geoset, offsetting by current vertex count
            for face in &geoset.faces {
                for &idx in &face.vertices {
                    all_indices.push((vertex_offset + idx) as u16);
                }
            }

            let index_count = (all_indices.len() as u32) - index_start;

            // Store vertex positions for depth sorting
            let vertices: Vec<[f32; 3]> = geoset.vertices.iter().map(|v| v.position).collect();

            // Store faces for depth sorting
            let faces: Vec<Vec<u32>> = geoset.faces.iter().map(|f| f.vertices.to_vec()).collect();

            geosets_info.push(GeosetRenderInfo {
                index_start,
                index_count,
                material_id: geoset.material_id,
                vertices,
                faces,
            });

            println!(
                "  Geoset {}: added {} vertices, {} UV sets, {} faces, material_id: {:?}",
                geoset_idx,
                geoset.vertices.len(),
                geoset.tex_coord_sets.len(),
                geoset.faces.len(),
                geoset.material_id
            );
        }

        self.geosets = geosets_info;
        self.materials = model.materials.clone();
        self.textures = model.textures.clone();

        // Calculate bounding box to understand model position
        if !all_vertices.is_empty() {
            let mut min = all_vertices[0].position;
            let mut max = all_vertices[0].position;
            for v in &all_vertices {
                for i in 0..3 {
                    min[i] = min[i].min(v.position[i]);
                    max[i] = max[i].max(v.position[i]);
                }
            }
            println!(
                "Model bounds: min({:.2}, {:.2}, {:.2}), max({:.2}, {:.2}, {:.2})",
                min[0], min[1], min[2], max[0], max[1], max[2]
            );

            // Store model center for camera targeting
            self.model_center = [
                (min[0] + max[0]) / 2.0,
                (min[1] + max[1]) / 2.0,
                (min[2] + max[2]) / 2.0,
            ];

            println!(
                "Center: ({:.2}, {:.2}, {:.2})",
                self.model_center[0], self.model_center[1], self.model_center[2]
            );
        }

        println!(
            "Total: {} vertices, {} indices ({} triangles)",
            all_vertices.len(),
            all_indices.len(),
            all_indices.len() / 3
        );

        // Store original vertices for animation
        self.original_vertices = all_vertices.clone();

        let vertex_buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Vertex Buffer"),
                contents: bytemuck::cast_slice(&all_vertices),
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            });

        let index_buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Index Buffer"),
                contents: bytemuck::cast_slice(&all_indices),
                usage: wgpu::BufferUsages::INDEX,
            });

        self.vertex_buffer = vertex_buffer;
        self.index_buffer = index_buffer;
        self.num_indices = all_indices.len() as u32;
        println!("Updated num_indices to: {}", self.num_indices);

        // Generate skeleton lines
        if !model.bones.is_empty() || !model.helpers.is_empty() {
            let mut skeleton_vertices = Vec::new();
            let bone_color = [1.0, 1.0, 0.0]; // Yellow for bones
            let helper_color = [0.0, 1.0, 1.0]; // Cyan for helpers

            // Helper function to find pivot point by object_id
            let find_pivot = |object_id: i32| -> Option<[f32; 3]> {
                if object_id < 0 {
                    return None;
                }

                // Search in bones first
                if let Some(bone) = model.bones.iter().find(|b| b.object_id == object_id as u32) {
                    return Some(bone.pivot_point);
                }

                // Then search in helpers
                if let Some(helper) = model
                    .helpers
                    .iter()
                    .find(|h| h.object_id == object_id as u32)
                {
                    return Some(helper.pivot_point);
                }

                None
            };

            // Process bones
            for bone in &model.bones {
                if let Some(parent_pivot) = find_pivot(bone.parent_id) {
                    skeleton_vertices.push(LineVertex {
                        position: parent_pivot,
                        color: bone_color,
                    });
                    skeleton_vertices.push(LineVertex {
                        position: bone.pivot_point,
                        color: bone_color,
                    });
                }
            }

            // Process helpers
            for helper in &model.helpers {
                if let Some(parent_pivot) = find_pivot(helper.parent_id) {
                    skeleton_vertices.push(LineVertex {
                        position: parent_pivot,
                        color: helper_color,
                    });
                    skeleton_vertices.push(LineVertex {
                        position: helper.pivot_point,
                        color: helper_color,
                    });
                }
            }

            if !skeleton_vertices.is_empty() {
                self.skeleton_vertex_buffer =
                    self.device
                        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                            label: Some("Skeleton Vertex Buffer"),
                            contents: bytemuck::cast_slice(&skeleton_vertices),
                            usage: wgpu::BufferUsages::VERTEX,
                        });
                self.num_skeleton_lines = (skeleton_vertices.len() / 2) as u32;
                println!(
                    "Loaded {} bones + {} helpers, generated {} skeleton lines",
                    model.bones.len(),
                    model.helpers.len(),
                    self.num_skeleton_lines
                );
            } else {
                self.num_skeleton_lines = 0;
                println!(
                    "Loaded {} bones + {} helpers, but no lines generated (all roots or invalid parent_ids)",
                    model.bones.len(),
                    model.helpers.len()
                );
            }
        } else {
            self.num_skeleton_lines = 0;
        }

        // Generate bounding box lines from geosets
        self.generate_bounding_box_lines(model);
    }

    /// Reset vertex buffer to original parsed vertices (no animation)
    pub fn reset_to_original_vertices(&mut self) {
        if self.original_vertices.is_empty() {
            return;
        }

        // Update vertex buffer with original data
        self.queue.write_buffer(
            &self.vertex_buffer,
            0,
            bytemuck::cast_slice(&self.original_vertices),
        );
    }

    /// Update vertex buffer with animated vertices
    /// Based on CalcAnimCoords from mdlDraw.pas (line 2310)
    pub fn update_animation(&mut self, animation_system: &crate::animation::AnimationSystem) {
        if self.original_vertices.is_empty() || animation_system.bones.is_empty() {
            return;
        }

        let Some(model) = &self.model else {
            return;
        };

        let mut transformed_vertices = self.original_vertices.clone();
        let mut vertex_offset = 0;

        // Process each geoset
        for geoset in &model.geosets {
            let num_vertices = geoset.vertices.len();

            // Transform each vertex in this geoset
            for i in 0..num_vertices {
                if i >= geoset.vertex_groups.len() {
                    continue;
                }

                let group_idx = geoset.vertex_groups[i] as usize;
                if group_idx >= geoset.matrix_groups.len() {
                    continue;
                }

                // Get bone indices for this vertex
                let bone_indices = &geoset.matrix_groups[group_idx];
                if bone_indices.is_empty() {
                    continue;
                }

                let vertex_idx = vertex_offset + i;
                if vertex_idx >= transformed_vertices.len() {
                    continue;
                }

                let original_pos = nalgebra_glm::vec3(
                    self.original_vertices[vertex_idx].position[0],
                    self.original_vertices[vertex_idx].position[1],
                    self.original_vertices[vertex_idx].position[2],
                );

                let original_normal = nalgebra_glm::vec3(
                    self.original_vertices[vertex_idx].normal[0],
                    self.original_vertices[vertex_idx].normal[1],
                    self.original_vertices[vertex_idx].normal[2],
                );

                // Multi-bone blending: transform by each bone and average
                let mut blended_pos = nalgebra_glm::vec3(0.0, 0.0, 0.0);
                let mut blended_normal = nalgebra_glm::vec3(0.0, 0.0, 0.0);
                let num_bones = bone_indices.len();

                for &bone_idx in bone_indices {
                    let bone_idx = bone_idx as usize;

                    // Get bone or helper
                    let bone = if bone_idx < animation_system.bones.len() {
                        &animation_system.bones[bone_idx]
                    } else {
                        let helper_idx = bone_idx - animation_system.bones.len();
                        if helper_idx < animation_system.helpers.len() {
                            &animation_system.helpers[helper_idx]
                        } else {
                            continue;
                        }
                    };

                    // Get pivot point for this bone
                    let pivot = if bone_idx < animation_system.pivot_points.len() {
                        animation_system.pivot_points[bone_idx]
                    } else {
                        nalgebra_glm::vec3(0.0, 0.0, 0.0)
                    };

                    // Transform vertex: (pos - pivot) * matrix + abs_vector
                    // Based on Delphi code lines 2379-2400
                    let relative_pos = original_pos - pivot;
                    let transformed = bone.abs_matrix * relative_pos + bone.abs_vector;
                    blended_pos += transformed;

                    // Transform normal: normal * matrix (no translation)
                    let transformed_normal = bone.abs_matrix * original_normal;
                    blended_normal += transformed_normal;
                }

                // Average the transformations (Delphi lines 2403-2410)
                if num_bones > 0 {
                    let weight = 1.0 / num_bones as f32;
                    blended_pos *= weight;
                    blended_normal *= weight;

                    // Normalize the normal
                    let normalized_normal = nalgebra_glm::normalize(&blended_normal);

                    transformed_vertices[vertex_idx].position =
                        [blended_pos.x, blended_pos.y, blended_pos.z];

                    transformed_vertices[vertex_idx].normal = [
                        normalized_normal.x,
                        normalized_normal.y,
                        normalized_normal.z,
                    ];
                }
            }

            vertex_offset += num_vertices;
        }

        // Update GPU buffer
        self.queue.write_buffer(
            &self.vertex_buffer,
            0,
            bytemuck::cast_slice(&transformed_vertices),
        );
    }

    pub fn update_colors(&mut self, settings: &Settings, model: Option<&Model>) {
        // Update team color
        self.set_team_color(settings.colors.team_color);

        // Update grid colors
        self.grid_major_color = settings.colors.grid_major_color;
        self.grid_minor_color = settings.colors.grid_minor_color;
        self.regenerate_grid();

        // Update skybox color
        self.skybox_color = settings.colors.skybox_color;

        // Update bounding box color if model is loaded
        if let Some(model) = model {
            if settings.display.show_bounding_box {
                self.generate_bounding_box_lines_with_color(
                    model,
                    settings.colors.bounding_box_color,
                );
            }
        }
    }

    /// Regenerate grid with current grid color
    fn regenerate_grid(&mut self) {
        let mut line_vertices = Vec::new();

        // Axes - red X, green Y, blue Z
        line_vertices.push(LineVertex {
            position: [0.0, 0.0, 0.0],
            color: [1.0, 0.0, 0.0],
        });
        line_vertices.push(LineVertex {
            position: [210.0, 0.0, 0.0],
            color: [1.0, 0.0, 0.0],
        });
        line_vertices.push(LineVertex {
            position: [0.0, 0.0, 0.0],
            color: [0.0, 1.0, 0.0],
        });
        line_vertices.push(LineVertex {
            position: [0.0, 210.0, 0.0],
            color: [0.0, 1.0, 0.0],
        });

        // Minor grid - XY plane (every 8 units)
        for i in -32..=32 {
            let pos = i as f32 * 8.0;
            line_vertices.push(LineVertex {
                position: [pos, -256.0, 0.0],
                color: self.grid_minor_color,
            });
            line_vertices.push(LineVertex {
                position: [pos, 256.0, 0.0],
                color: self.grid_minor_color,
            });
            line_vertices.push(LineVertex {
                position: [-256.0, pos, 0.0],
                color: self.grid_minor_color,
            });
            line_vertices.push(LineVertex {
                position: [256.0, pos, 0.0],
                color: self.grid_minor_color,
            });
        }

        // Major grid - XY plane (every 64 units)
        for i in -4..=4 {
            let pos = i as f32 * 64.0;
            line_vertices.push(LineVertex {
                position: [pos, -256.0, 0.0],
                color: self.grid_major_color,
            });
            line_vertices.push(LineVertex {
                position: [pos, 256.0, 0.0],
                color: self.grid_major_color,
            });
            line_vertices.push(LineVertex {
                position: [-256.0, pos, 0.0],
                color: self.grid_major_color,
            });
            line_vertices.push(LineVertex {
                position: [256.0, pos, 0.0],
                color: self.grid_major_color,
            });
        }

        // Update line vertex buffer
        self.line_vertex_buffer =
            self.device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("Line Vertex Buffer"),
                    contents: bytemuck::cast_slice(&line_vertices),
                    usage: wgpu::BufferUsages::VERTEX,
                });
        self.num_lines = line_vertices.len() as u32;
    }

    /// Load texture from RGBA data and update or add texture bind group
    pub fn load_texture_from_rgba(
        &mut self,
        rgba_data: &[u8],
        width: u32,
        height: u32,
        texture_id: usize,
    ) {
        let texture_size = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };

        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Loaded Texture"),
            size: texture_size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            rgba_data,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4 * width),
                rows_per_image: Some(height),
            },
            texture_size,
        );

        let texture_view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = self.device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Texture Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        // Create new bind group for this texture
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Texture Bind Group"),
            layout: &self.texture_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&texture_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });

        // Ensure both vectors are large enough
        let required_size = texture_id + 1;

        // Expand bind_groups if needed
        while self.texture_bind_groups.len() < required_size {
            // Fill gaps with white texture bind groups
            let white_bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("White Texture Bind Group (Gap Filler)"),
                layout: &self.texture_bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&self.white_texture_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&self.white_texture_sampler),
                    },
                ],
            });
            self.texture_bind_groups.push(white_bind_group);
        }

        // Expand texture_views if needed
        while self.texture_views.len() < required_size {
            self.texture_views.push(None);
        }

        // Update the bind group and view for this texture ID
        self.texture_bind_groups[texture_id] = bind_group;
        self.texture_views[texture_id] = Some(texture_view);
    }

    /// Get egui TextureId for a loaded texture
    pub fn get_egui_texture_id(&mut self, texture_id: usize) -> Option<egui::TextureId> {
        if texture_id < self.texture_views.len() {
            if let Some(texture_view) = &self.texture_views[texture_id] {
                // Register texture in egui renderer and get TextureId
                let egui_texture_id = self.egui_renderer.register_native_texture(
                    &self.device,
                    texture_view,
                    wgpu::FilterMode::Linear,
                );
                return Some(egui_texture_id);
            }
        }
        None
    }
}

#[cfg(test)]
mod scene_tests {
    use super::*;
    use crate::animation::types::{PlaybackMode, ResolvedFrame};
    use crate::model::ids::{GeosetIndex, MaterialIndex, TextureIndex};
    use crate::renderer::geoset_render_info::{BlendFactor, ScenePipelineState};
    use crate::scene::types::{
        SceneBounds, SceneDraw, SceneFilterMode, SceneMaterialState, SceneMesh, SceneRenderState,
        SceneSortClass, SceneTextureRequest, TextureTransform,
    };
    use bytemuck::Zeroable;

    fn frame() -> ResolvedFrame {
        ResolvedFrame {
            sequence: None,
            sequence_frame: 0.0,
            global_frame: 0.0,
            playback: PlaybackMode::Clamp,
            view: None,
        }
    }

    // Scene vertices flip model Y in `vs_scene`, so this source order reaches the
    // rasterizer clockwise and is the front-facing order for the frozen pipeline.
    const MODEL_TRIANGLE_RENDER_CW: [u32; 3] = [0, 1, 2];

    fn mesh(geoset: u32, center: [f32; 3]) -> SceneMesh {
        SceneMesh {
            geoset: GeosetIndex(geoset),
            positions: vec![
                [center[0] - 1.0, center[1], center[2]],
                [center[0] + 1.0, center[1], center[2]],
                [center[0], center[1] + 1.0, center[2]],
            ],
            normals: vec![[0.0, 0.0, 1.0]; 3],
            uv_sets: vec![
                vec![[0.0, 0.0], [1.0, 0.0], [0.5, 1.0]],
                vec![[0.25, 0.5], [0.75, 0.5], [0.5, 0.75]],
            ],
            triangles: vec![MODEL_TRIANGLE_RENDER_CW],
            bounds: SceneBounds {
                min: [center[0] - 1.0, center[1], center[2]],
                max: [center[0] + 1.0, center[1] + 1.0, center[2]],
                center,
            },
        }
    }

    fn draw(source_ordinal: u32, mesh: u32, filter_mode: SceneFilterMode) -> SceneDraw {
        SceneDraw {
            source_ordinal,
            geoset: GeosetIndex(mesh),
            mesh,
            material: MaterialIndex(0),
            layer: source_ordinal,
            priority_plane: 0,
            geoset_color: [0.25, 0.5, 0.75],
            geoset_alpha: 0.6,
            layer_alpha: 0.4,
            texture: None,
            coord_set: 0,
            texture_transform: TextureTransform::default(),
            filter_mode,
            material_state: SceneMaterialState::default(),
            render_state: SceneRenderState::default(),
            sort_class: SceneSortClass::Stable,
        }
    }

    fn packet(meshes: Vec<SceneMesh>, draws: Vec<SceneDraw>) -> ScenePacket {
        ScenePacket::new(frame(), meshes, draws, Vec::new()).unwrap()
    }

    fn view() -> SceneView {
        SceneView {
            eye: [0.0, 0.0, 0.0],
            forward: [0.0, 0.0, 1.0],
        }
    }

    fn offscreen_input(packet: &ScenePacket) -> OffscreenSceneInput<'_> {
        OffscreenSceneInput {
            packet,
            textures: &[],
        }
    }

    fn triangle_mesh(geoset: u32, left: f32, right: f32) -> SceneMesh {
        let center_x = (left + right) * 0.5;
        SceneMesh {
            geoset: GeosetIndex(geoset),
            positions: vec![[left, -0.9, 0.0], [right, -0.9, 0.0], [center_x, 0.9, 0.0]],
            normals: vec![[0.0, 0.0, 1.0]; 3],
            uv_sets: vec![vec![[0.0, 0.0], [1.0, 0.0], [0.5, 1.0]]],
            triangles: vec![MODEL_TRIANGLE_RENDER_CW],
            bounds: SceneBounds {
                min: [left, -0.9, 0.0],
                max: [right, 0.9, 0.0],
                center: [center_x, 0.0, 0.0],
            },
        }
    }

    fn asymmetric_triangle_mesh(geoset: u32, triangle: [u32; 3]) -> SceneMesh {
        SceneMesh {
            geoset: GeosetIndex(geoset),
            positions: vec![[-0.8, -0.7, 0.0], [0.7, -0.5, 0.0], [-0.2, 0.8, 0.0]],
            normals: vec![[0.0, 0.0, 1.0]; 3],
            uv_sets: vec![vec![[0.0, 0.0], [0.4, 1.0], [1.0, 0.1]]],
            triangles: vec![triangle],
            bounds: SceneBounds {
                min: [-0.8, -0.7, 0.0],
                max: [0.7, 0.8, 0.0],
                center: [-0.1, 0.05, 0.0],
            },
        }
    }

    fn opaque_unshaded_draw(source_ordinal: u32, mesh: u32) -> SceneDraw {
        let mut scene_draw = draw(source_ordinal, mesh, SceneFilterMode::None);
        scene_draw.geoset_color = [1.0; 3];
        scene_draw.geoset_alpha = 1.0;
        scene_draw.layer_alpha = 1.0;
        scene_draw.render_state.unshaded = true;
        scene_draw
    }

    fn pixel(result: &OffscreenSceneRgba, x: u32, y: u32) -> &[u8] {
        let offset = ((y * result.width + x) * 4) as usize;
        &result.rgba[offset..offset + 4]
    }

    #[test]
    fn all_filter_modes_map_to_frozen_pipeline_state() {
        let state = SceneRenderState::default();
        let cases = [
            (SceneFilterMode::None, None, false, true),
            (SceneFilterMode::Transparent, None, true, true),
            (
                SceneFilterMode::Blend,
                Some((BlendFactor::SrcAlpha, BlendFactor::OneMinusSrcAlpha)),
                false,
                false,
            ),
            (
                SceneFilterMode::Additive,
                Some((BlendFactor::One, BlendFactor::One)),
                false,
                false,
            ),
            (
                SceneFilterMode::AddAlpha,
                Some((BlendFactor::SrcAlpha, BlendFactor::One)),
                false,
                false,
            ),
            (
                SceneFilterMode::Modulate,
                Some((BlendFactor::Dst, BlendFactor::Zero)),
                false,
                false,
            ),
            (
                SceneFilterMode::Modulate2x,
                Some((BlendFactor::Dst, BlendFactor::Src)),
                false,
                false,
            ),
        ];
        for (filter, blend, alpha_cutoff, depth_write) in cases {
            assert_eq!(
                ScenePipelineState::from_scene(filter, state),
                ScenePipelineState {
                    blend,
                    alpha_cutoff,
                    depth_write,
                    depth_always: false,
                    cull_back: true,
                }
            );
        }
        assert_eq!(pass_rank(SceneFilterMode::None), 0);
        assert_eq!(pass_rank(SceneFilterMode::Transparent), 0);
        assert_eq!(pass_rank(SceneFilterMode::Blend), 1);
        for filter in [
            SceneFilterMode::Additive,
            SceneFilterMode::AddAlpha,
            SceneFilterMode::Modulate,
            SceneFilterMode::Modulate2x,
        ] {
            assert_eq!(pass_rank(filter), 2);
        }
    }

    #[test]
    fn cross_product_render_flags_override_depth_and_culling() {
        assert_eq!(SCENE_FRONT_FACE, wgpu::FrontFace::Cw);
        for filter in [SceneFilterMode::None, SceneFilterMode::Blend] {
            for two_sided in [false, true] {
                for no_depth_test in [false, true] {
                    for no_depth_write in [false, true] {
                        let state = ScenePipelineState::from_scene(
                            filter,
                            SceneRenderState {
                                two_sided,
                                no_depth_test,
                                no_depth_write,
                                ..Default::default()
                            },
                        );
                        assert_eq!(state.cull_back, !two_sided);
                        assert_eq!(state.cull_mode(), (!two_sided).then_some(wgpu::Face::Back));
                        assert_eq!(state.depth_always, no_depth_test);
                        assert_eq!(
                            state.depth_write,
                            filter == SceneFilterMode::None && !no_depth_write
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn prepare_scene_preserves_u32_indices_above_65535() {
        let vertex_count = 70_001;
        let mut large = SceneMesh {
            geoset: GeosetIndex(0),
            positions: vec![[0.0, 0.0, 0.0]; vertex_count],
            normals: Vec::new(),
            uv_sets: vec![vec![[0.0, 0.0]; vertex_count]],
            triangles: vec![[69_998, 69_999, 70_000]],
            bounds: SceneBounds {
                min: [0.0; 3],
                max: [0.0; 3],
                center: [0.0; 3],
            },
        };
        large.positions[69_999] = [1.0, 0.0, 0.0];
        large.positions[70_000] = [0.0, 1.0, 0.0];
        let prepared = prepare_scene(
            &packet(vec![large], vec![draw(0, 0, SceneFilterMode::None)]),
            view(),
        )
        .unwrap();
        assert_eq!(prepared.indices, [69_998, 69_999, 70_000]);
        assert_eq!(prepared.vertices.len(), vertex_count);
    }

    #[test]
    fn selected_uv_set_and_reconstructed_normals_are_prepared() {
        let mut scene_mesh = mesh(0, [0.0, 0.0, 0.0]);
        scene_mesh.normals.clear();
        let mut scene_draw = draw(0, 0, SceneFilterMode::None);
        scene_draw.coord_set = 1;
        let prepared = prepare_scene(&packet(vec![scene_mesh], vec![scene_draw]), view()).unwrap();
        assert_eq!(prepared.vertices[0].uv, [0.25, 0.5]);
        assert_eq!(prepared.vertices[0].normal, [0.0, 0.0, 1.0]);

        let mut degenerate = mesh(0, [0.0, 0.0, 0.0]);
        degenerate.normals.clear();
        degenerate.positions.fill([0.0; 3]);
        degenerate.bounds = SceneBounds {
            min: [0.0; 3],
            max: [0.0; 3],
            center: [0.0; 3],
        };
        let prepared = prepare_scene(
            &packet(vec![degenerate], vec![draw(0, 0, SceneFilterMode::None)]),
            view(),
        )
        .unwrap();
        assert!(
            prepared
                .vertices
                .iter()
                .all(|vertex| vertex.normal == [0.0, 0.0, 1.0])
        );
    }

    #[test]
    fn txan_is_scale_rotation_translation_and_alpha_channels_stay_separate() {
        let mut scene_draw = draw(0, 0, SceneFilterMode::Blend);
        scene_draw.texture_transform = TextureTransform {
            translation: [3.0, 5.0, 0.0],
            rotation: [0.0, 0.0, 0.0, 1.0],
            scaling: [2.0, 4.0, 1.0],
        };
        let prepared =
            prepare_scene(&packet(vec![mesh(0, [0.0; 3])], vec![scene_draw]), view()).unwrap();
        let uniform = prepared.uniforms[0];
        assert_eq!(uniform.geoset_color_alpha, [0.25, 0.5, 0.75, 0.6]);
        assert_eq!(uniform.layer_options[0], 0.4);
        assert_eq!(uniform.texture_transform[3][0], 6.0);
        assert_eq!(uniform.texture_transform[3][1], 20.0);
    }

    #[test]
    fn draw_sorting_is_pass_priority_depth_and_stable_ties() {
        let meshes = vec![
            mesh(0, [0.0, 0.0, 5.0]),
            mesh(1, [0.0, 0.0, 10.0]),
            mesh(2, [0.0, 0.0, 10.0]),
        ];
        let mut opaque = draw(0, 0, SceneFilterMode::None);
        opaque.priority_plane = 3;
        let mut near = draw(1, 0, SceneFilterMode::Blend);
        near.priority_plane = 2;
        let mut far_later = draw(2, 2, SceneFilterMode::Blend);
        far_later.priority_plane = 2;
        let mut far_first = draw(3, 1, SceneFilterMode::Blend);
        far_first.priority_plane = 2;
        let prepared = prepare_scene(
            &packet(meshes, vec![opaque, near, far_later, far_first]),
            view(),
        )
        .unwrap();
        assert_eq!(
            prepared
                .draws
                .iter()
                .map(|draw| draw.source_ordinal)
                .collect::<Vec<_>>(),
            [0, 2, 3, 1]
        );
    }

    #[test]
    fn far_z_splits_and_sorts_triangles_with_stable_ordinal_tie() {
        let scene_mesh = SceneMesh {
            geoset: GeosetIndex(0),
            positions: vec![
                [0.0, 0.0, 1.0],
                [1.0, 0.0, 1.0],
                [0.0, 1.0, 1.0],
                [0.0, 0.0, 9.0],
                [1.0, 0.0, 9.0],
                [0.0, 1.0, 9.0],
            ],
            normals: vec![[0.0, 0.0, 1.0]; 6],
            uv_sets: vec![vec![[0.0, 0.0]; 6]],
            triangles: vec![[0, 1, 2], [3, 4, 5]],
            bounds: SceneBounds {
                min: [0.0, 0.0, 1.0],
                max: [1.0, 1.0, 9.0],
                center: [0.5, 0.5, 5.0],
            },
        };
        let mut scene_draw = draw(0, 0, SceneFilterMode::Blend);
        scene_draw.material_state.sort_primitives_far_z = true;
        scene_draw.sort_class = SceneSortClass::BackToFrontTriangles;
        let prepared = prepare_scene(&packet(vec![scene_mesh], vec![scene_draw]), view()).unwrap();
        assert_eq!(
            prepared
                .draws
                .iter()
                .map(|draw| draw.triangle_ordinal)
                .collect::<Vec<_>>(),
            [1, 0]
        );
    }

    #[test]
    fn every_blend_family_splits_triangles_and_reverses_with_camera() {
        for filter in [
            SceneFilterMode::Blend,
            SceneFilterMode::Additive,
            SceneFilterMode::AddAlpha,
            SceneFilterMode::Modulate,
            SceneFilterMode::Modulate2x,
        ] {
            let scene_mesh = SceneMesh {
                geoset: GeosetIndex(0),
                positions: vec![
                    [0.0, 0.0, 1.0],
                    [1.0, 0.0, 1.0],
                    [0.0, 1.0, 1.0],
                    [0.0, 0.0, 9.0],
                    [1.0, 0.0, 9.0],
                    [0.0, 1.0, 9.0],
                ],
                normals: vec![[0.0, 0.0, 1.0]; 6],
                uv_sets: vec![vec![[0.0, 0.0]; 6]],
                triangles: vec![[0, 1, 2], [3, 4, 5]],
                bounds: SceneBounds {
                    min: [0.0, 0.0, 1.0],
                    max: [1.0, 1.0, 9.0],
                    center: [0.5, 0.5, 5.0],
                },
            };
            let scene = packet(vec![scene_mesh], vec![draw(0, 0, filter)]);
            let forward = prepare_scene(&scene, view()).unwrap();
            let reverse = prepare_scene(
                &scene,
                SceneView {
                    eye: [0.0; 3],
                    forward: [0.0, 0.0, -1.0],
                },
            )
            .unwrap();
            assert_eq!(
                forward
                    .draws
                    .iter()
                    .map(|draw| draw.triangle_ordinal)
                    .collect::<Vec<_>>(),
                [1, 0],
                "{filter:?}"
            );
            assert_eq!(
                reverse
                    .draws
                    .iter()
                    .map(|draw| draw.triangle_ordinal)
                    .collect::<Vec<_>>(),
                [0, 1],
                "{filter:?}"
            );
        }
    }

    #[test]
    fn uniforms_are_256_byte_aligned_and_sparse_textures_use_logical_slots() {
        assert_eq!(
            size_of::<SceneDrawUniform>(),
            SCENE_UNIFORM_ALIGNMENT as usize
        );
        let textures = vec![
            SceneTextureRequest {
                index: TextureIndex(3),
                filename: "a.blp".into(),
                replaceable_id: 0,
                wrap_u: false,
                wrap_v: false,
            },
            SceneTextureRequest {
                index: TextureIndex(40),
                filename: "b.blp".into(),
                replaceable_id: 0,
                wrap_u: true,
                wrap_v: false,
            },
        ];
        let mut first = draw(0, 0, SceneFilterMode::None);
        first.texture = Some(TextureIndex(40));
        let mut second = draw(1, 0, SceneFilterMode::Blend);
        second.texture = Some(TextureIndex(3));
        let scene = ScenePacket::new(
            frame(),
            vec![mesh(0, [0.0; 3])],
            vec![first, second],
            textures,
        )
        .unwrap();
        let prepared = prepare_scene(&scene, view()).unwrap();
        assert_eq!(prepared.texture_slots, BTreeMap::from([(3, 0), (40, 1)]));
        assert_eq!(
            prepared.draws[0].uniform_offset % SCENE_UNIFORM_ALIGNMENT,
            0
        );
        assert_eq!(
            prepared.draws[1].uniform_offset % SCENE_UNIFORM_ALIGNMENT,
            0
        );
        assert_eq!(prepared.draws[0].texture, Some(TextureIndex(40)));
        assert_eq!(prepared.draws[0].texture_slot, Some(1));
    }

    #[test]
    fn resolved_texture_metadata_requires_exact_logical_identity_without_panicking() {
        let textures = vec![SceneTextureRequest {
            index: TextureIndex(3),
            filename: "a.blp".into(),
            replaceable_id: 0,
            wrap_u: false,
            wrap_v: false,
        }];
        let mut scene_draw = draw(0, 0, SceneFilterMode::None);
        scene_draw.texture = Some(TextureIndex(3));
        let scene =
            ScenePacket::new(frame(), vec![mesh(0, [0.0; 3])], vec![scene_draw], textures).unwrap();
        let mut prepared = prepare_scene(&scene, view()).unwrap();
        let wrong = ResolvedSceneTexture {
            index: TextureIndex(40),
            width: 1,
            height: 1,
            rgba: vec![255; 4],
            color_space: CpuColorSpace::Srgb,
            alpha_encoding: CpuAlphaEncoding::Straight,
            address_u: TextureAddressMode::Clamp,
            address_v: TextureAddressMode::Repeat,
            origin: crate::texture::scene::TextureOrigin::GeneratedTeamColor,
        };
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            apply_texture_metadata(&mut prepared, &scene, &[wrong])
        }));
        assert!(
            matches!(result, Ok(Err(error)) if error.key == "renderer-resolved-texture-set-mismatch")
        );
    }

    #[test]
    fn resolved_texture_dimensions_fail_before_gpu_creation() {
        let oversized = ResolvedSceneTexture {
            index: TextureIndex(3),
            width: 17,
            height: 1,
            rgba: vec![0; 68],
            color_space: CpuColorSpace::Srgb,
            alpha_encoding: CpuAlphaEncoding::Straight,
            address_u: TextureAddressMode::Clamp,
            address_v: TextureAddressMode::Repeat,
            origin: crate::texture::scene::TextureOrigin::GeneratedTeamColor,
        };
        assert!(matches!(
            validate_resolved_texture_data(&oversized, 16),
            Err(error) if error.key == "renderer-invalid-resolved-texture-size"
        ));
        let overflow = ResolvedSceneTexture {
            width: u32::MAX,
            height: 1,
            rgba: Vec::new(),
            ..oversized
        };
        assert!(matches!(
            validate_resolved_texture_data(&overflow, u32::MAX),
            Err(error) if error.key == "renderer-resolved-texture-size-overflow"
        ));
    }

    #[test]
    fn visibility_gates_are_filter_specific_and_ignore_texture_alpha_for_opaque() {
        let mut opaque_hidden = draw(0, 0, SceneFilterMode::None);
        opaque_hidden.layer_alpha = 0.009;
        let mut transparent_hidden = draw(1, 0, SceneFilterMode::Transparent);
        transparent_hidden.layer_alpha = 0.749;
        let mut blend_hidden = draw(2, 0, SceneFilterMode::Blend);
        blend_hidden.layer_alpha = 0.009;
        let hidden = packet(
            vec![mesh(0, [0.0; 3])],
            vec![opaque_hidden, transparent_hidden, blend_hidden],
        );
        assert!(prepare_scene(&hidden, view()).unwrap().draws.is_empty());

        let mut opaque = draw(0, 0, SceneFilterMode::None);
        opaque.layer_alpha = 0.01;
        let mut transparent = draw(1, 0, SceneFilterMode::Transparent);
        transparent.layer_alpha = 0.75;
        let mut blend = draw(2, 0, SceneFilterMode::Blend);
        blend.layer_alpha = 0.01;
        let visible = packet(vec![mesh(0, [0.0; 3])], vec![opaque, transparent, blend]);
        assert_eq!(prepare_scene(&visible, view()).unwrap().draws.len(), 3);
        assert!(include_str!("../shader.wgsl").contains("combined_alpha < 0.75"));
    }

    #[test]
    fn sphere_uv_uses_render_space_eye_and_has_finite_zero_distance_fallback() {
        let position = [1.0, 2.0, 0.5];
        let normal = [0.0, 1.0, 0.0];
        let above = sphere_uv(position, normal, [0.0, 10.0, 3.0]);
        let below = sphere_uv(position, normal, [0.0, -10.0, 3.0]);
        assert_ne!(above, below);
        assert!(above.into_iter().chain(below).all(f32::is_finite));
        assert!(
            sphere_uv(position, normal, position)
                .into_iter()
                .all(f32::is_finite)
        );

        let scene = packet(
            vec![mesh(0, [0.0; 3])],
            vec![draw(0, 0, SceneFilterMode::None)],
        );
        let prepared = prepare_scene(
            &scene,
            SceneView {
                eye: [2.0, 10.0, 4.0],
                forward: [0.0, -1.0, 0.0],
            },
        )
        .unwrap();
        assert_eq!(
            prepared.uniforms[0].legacy_team_color,
            [2.0, 10.0, 4.0, 0.0]
        );
    }

    #[test]
    fn refresh_scene_view_updates_sphere_eye_and_y_axis_triangle_sorting() {
        let y_mesh = SceneMesh {
            geoset: GeosetIndex(0),
            positions: vec![
                [-0.5, 1.0, 0.0],
                [0.5, 1.0, 0.0],
                [0.0, 2.0, 0.0],
                [-0.5, 8.0, 0.0],
                [0.5, 8.0, 0.0],
                [0.0, 9.0, 0.0],
            ],
            normals: vec![[0.0, 0.0, 1.0]; 6],
            uv_sets: vec![vec![[0.0; 2]; 6]],
            triangles: vec![[0, 1, 2], [3, 4, 5]],
            bounds: SceneBounds {
                min: [-0.5, 1.0, 0.0],
                max: [0.5, 9.0, 0.0],
                center: [0.0, 5.0, 0.0],
            },
        };
        let scene = packet(vec![y_mesh], vec![draw(0, 0, SceneFilterMode::Blend)]);
        let mut prepared = prepare_scene(
            &scene,
            SceneView {
                eye: [0.0, -10.0, 0.0],
                forward: [0.0, 1.0, 0.0],
            },
        )
        .unwrap();
        assert_eq!(
            prepared
                .draws
                .iter()
                .map(|draw| draw.triangle_ordinal)
                .collect::<Vec<_>>(),
            [0, 1]
        );
        refresh_prepared_scene_view(
            &mut prepared,
            SceneView {
                eye: [0.0, 10.0, 0.0],
                forward: [0.0, -1.0, 0.0],
            },
        )
        .unwrap();
        assert_eq!(
            prepared
                .draws
                .iter()
                .map(|draw| draw.triangle_ordinal)
                .collect::<Vec<_>>(),
            [1, 0]
        );
        assert!(
            prepared
                .uniforms
                .iter()
                .all(|uniform| uniform.legacy_team_color == [0.0, 10.0, 0.0, 0.0])
        );
    }

    #[test]
    fn public_view_projection_keeps_legacy_camera_while_scene_uses_flipped_geometry() {
        let eye = nalgebra_glm::vec3(3.0, 10.0, 4.0);
        let center = nalgebra_glm::vec3(0.0, 0.0, 0.0);
        let up = nalgebra_glm::vec3(0.0, 0.0, 1.0);
        let public_view = nalgebra_glm::look_at(&eye, &center, &up);
        let legacy_point = public_view * nalgebra_glm::vec4(0.0, 2.0, 0.0, 1.0);
        let historical =
            nalgebra_glm::look_at(&eye, &center, &up) * nalgebra_glm::vec4(0.0, 2.0, 0.0, 1.0);
        assert_eq!(legacy_point, historical);

        let scene_position = render_position([0.0, 2.0, 0.0]);
        let scene_point = public_view
            * nalgebra_glm::vec4(scene_position[0], scene_position[1], scene_position[2], 1.0);
        assert_ne!(legacy_point, scene_point);
        assert_ne!(
            sphere_uv([0.0, 2.0, 0.0], [0.0, 1.0, 0.0], eye.into()),
            sphere_uv([0.0, 2.0, 0.0], [0.0, 1.0, 0.0], [eye.x, -eye.y, eye.z],)
        );
    }

    #[test]
    fn premultiplied_alpha_metadata_maps_by_dynamic_uniform_offset_after_sorting() {
        let textures = vec![SceneTextureRequest {
            index: TextureIndex(3),
            filename: "a.blp".into(),
            replaceable_id: 0,
            wrap_u: false,
            wrap_v: false,
        }];
        let mut first = draw(0, 0, SceneFilterMode::Blend);
        first.texture = Some(TextureIndex(3));
        let mut second = draw(1, 0, SceneFilterMode::None);
        second.texture = Some(TextureIndex(3));
        let scene = ScenePacket::new(
            frame(),
            vec![mesh(0, [0.0; 3])],
            vec![first, second],
            textures,
        )
        .unwrap();
        let mut prepared = prepare_scene(&scene, view()).unwrap();
        let resolved = ResolvedSceneTexture {
            index: TextureIndex(3),
            width: 1,
            height: 1,
            rgba: vec![128, 0, 0, 128],
            color_space: CpuColorSpace::Srgb,
            alpha_encoding: CpuAlphaEncoding::Premultiplied,
            address_u: TextureAddressMode::Clamp,
            address_v: TextureAddressMode::Repeat,
            origin: crate::texture::scene::TextureOrigin::GeneratedTeamGlow,
        };
        apply_texture_metadata(&mut prepared, &scene, &[resolved]).unwrap();
        for draw in &prepared.draws {
            assert_eq!(
                prepared.uniforms[(draw.uniform_offset / SCENE_UNIFORM_ALIGNMENT) as usize]
                    .legacy_padding[0],
                1.0
            );
        }

        let mut additive = draw(0, 0, SceneFilterMode::Additive);
        additive.texture = Some(TextureIndex(3));
        let additive_scene = ScenePacket::new(
            frame(),
            vec![mesh(0, [0.0; 3])],
            vec![additive],
            vec![SceneTextureRequest {
                index: TextureIndex(3),
                filename: "a.blp".into(),
                replaceable_id: 0,
                wrap_u: false,
                wrap_v: false,
            }],
        )
        .unwrap();
        let mut additive_prepared = prepare_scene(&additive_scene, view()).unwrap();
        let glow = ResolvedSceneTexture {
            index: TextureIndex(3),
            width: 1,
            height: 1,
            rgba: vec![1, 0, 1, 1],
            color_space: CpuColorSpace::Srgb,
            alpha_encoding: CpuAlphaEncoding::Premultiplied,
            address_u: TextureAddressMode::Clamp,
            address_v: TextureAddressMode::Repeat,
            origin: crate::texture::scene::TextureOrigin::GeneratedTeamGlow,
        };
        apply_texture_metadata(&mut additive_prepared, &additive_scene, &[glow]).unwrap();
        assert_eq!(additive_prepared.uniforms[0].legacy_padding[0], 0.0);
    }

    #[test]
    fn non_finite_view_returns_error_without_panicking() {
        let scene = packet(
            vec![mesh(0, [0.0; 3])],
            vec![draw(0, 0, SceneFilterMode::None)],
        );
        let result = std::panic::catch_unwind(|| {
            prepare_scene(
                &scene,
                SceneView {
                    eye: [f32::NAN, 0.0, 0.0],
                    forward: [0.0, 0.0, 1.0],
                },
            )
        });
        assert!(matches!(result, Ok(Err(error)) if error.key == "renderer-non-finite-view"));
    }

    #[test]
    fn offscreen_options_have_stable_size_and_finite_errors() {
        let scene = packet(
            vec![mesh(0, [0.0; 3])],
            vec![draw(0, 0, SceneFilterMode::None)],
        );
        let invalid_size =
            tokio::runtime::Runtime::new()
                .unwrap()
                .block_on(render_scene_offscreen(
                    offscreen_input(&scene),
                    OffscreenSceneOptions {
                        width: 0,
                        ..Default::default()
                    },
                ));
        assert!(
            matches!(invalid_size, Err(error) if error.key == "renderer-offscreen-invalid-size")
        );
        let invalid_matrix =
            tokio::runtime::Runtime::new()
                .unwrap()
                .block_on(render_scene_offscreen(
                    offscreen_input(&scene),
                    OffscreenSceneOptions {
                        view_proj: [[f32::NAN; 4]; 4],
                        ..Default::default()
                    },
                ));
        assert!(
            matches!(invalid_matrix, Err(error) if error.key == "renderer-offscreen-non-finite-options")
        );
    }

    #[tokio::test]
    async fn offscreen_seam_reads_tight_top_left_rgba_without_window_or_surface() {
        let empty_scene = packet(Vec::new(), Vec::new());
        let result = render_scene_offscreen(
            offscreen_input(&empty_scene),
            OffscreenSceneOptions {
                width: 65,
                height: 7,
                clear: [0.1, 0.2, 0.3, 1.0],
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!((result.width, result.height), (65, 7));
        assert_eq!(result.rgba.len(), 65 * 7 * 4);
        assert!(!result.adapter.name.is_empty());
        assert_eq!(result.adapter.backend, wgpu::Backend::Dx12);
        assert_eq!(result.rgba[0..4], [89, 124, 149, 255]);
        assert_eq!(
            result.rgba[(65 * 6 * 4)..(65 * 6 * 4 + 4)],
            [89, 124, 149, 255]
        );

        let drawn_packet = packet(
            vec![mesh(0, [0.0, 0.0, 0.0])],
            vec![draw(0, 0, SceneFilterMode::None)],
        );
        let drawn = render_scene_offscreen(
            offscreen_input(&drawn_packet),
            OffscreenSceneOptions {
                width: 65,
                height: 7,
                clear: [0.1, 0.2, 0.3, 1.0],
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert!(
            drawn
                .rgba
                .chunks_exact(4)
                .any(|pixel| pixel != [89, 124, 149, 255])
        );

        let empty_mesh = SceneMesh {
            geoset: GeosetIndex(0),
            positions: Vec::new(),
            normals: Vec::new(),
            uv_sets: vec![Vec::new()],
            triangles: Vec::new(),
            bounds: SceneBounds {
                min: [0.0; 3],
                max: [0.0; 3],
                center: [0.0; 3],
            },
        };
        let no_geometry_packet = packet(vec![empty_mesh], vec![draw(0, 0, SceneFilterMode::None)]);
        let no_geometry = render_scene_offscreen(
            offscreen_input(&no_geometry_packet),
            OffscreenSceneOptions {
                width: 65,
                height: 7,
                clear: [0.1, 0.2, 0.3, 1.0],
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(no_geometry.rgba, result.rgba);
    }

    #[tokio::test]
    #[ignore = "requires frozen RTX 3060 DX12 G3 adapter"]
    async fn offscreen_uses_frozen_rtx3060_dx12() {
        let empty_scene = packet(Vec::new(), Vec::new());
        let options = OffscreenSceneOptions {
            width: 65,
            height: 7,
            clear: [0.1, 0.2, 0.3, 1.0],
            ..Default::default()
        };
        let first = render_scene_offscreen(offscreen_input(&empty_scene), options)
            .await
            .unwrap();
        let second = render_scene_offscreen(offscreen_input(&empty_scene), options)
            .await
            .unwrap();
        for result in [&first, &second] {
            assert_eq!(result.adapter.name, "NVIDIA GeForce RTX 3060");
            assert_eq!(result.adapter.backend, wgpu::Backend::Dx12);
            assert_eq!(result.adapter.device_type, wgpu::DeviceType::DiscreteGpu);
            assert_eq!(result.adapter.vendor, 0x10de);
            assert_eq!(result.adapter.device, 0x2504);
            println!("offscreen adapter: {:?}", result.adapter);
        }
        assert_eq!(first.adapter, second.adapter);
        assert_eq!(first.rgba, second.rgba);
    }

    #[tokio::test]
    #[ignore = "requires frozen RTX 3060 DX12 G3 adapter"]
    async fn offscreen_frozen_rtx3060_dx12_respects_cw_culling() {
        let options = OffscreenSceneOptions {
            width: 65,
            height: 65,
            clear: [0.0, 0.0, 0.0, 1.0],
            ..Default::default()
        };
        let clockwise = packet(
            vec![asymmetric_triangle_mesh(0, MODEL_TRIANGLE_RENDER_CW)],
            vec![opaque_unshaded_draw(0, 0)],
        );
        let clockwise_result = render_scene_offscreen(offscreen_input(&clockwise), options)
            .await
            .unwrap();
        assert_eq!(pixel(&clockwise_result, 32, 32), [255, 255, 255, 255]);

        let counter_clockwise = packet(
            vec![asymmetric_triangle_mesh(0, [0, 2, 1])],
            vec![opaque_unshaded_draw(0, 0)],
        );
        let counter_clockwise_result =
            render_scene_offscreen(offscreen_input(&counter_clockwise), options)
                .await
                .unwrap();
        assert_eq!(pixel(&counter_clockwise_result, 32, 32), [0, 0, 0, 255]);

        let mut two_sided_draw = opaque_unshaded_draw(0, 0);
        two_sided_draw.render_state.two_sided = true;
        let two_sided = packet(
            vec![asymmetric_triangle_mesh(0, [0, 2, 1])],
            vec![two_sided_draw],
        );
        let two_sided_result = render_scene_offscreen(offscreen_input(&two_sided), options)
            .await
            .unwrap();
        assert_eq!(pixel(&two_sided_result, 32, 32), [255, 255, 255, 255]);
    }

    #[tokio::test]
    async fn offscreen_none_uses_white_while_sparse_texture_uses_its_dense_slot() {
        let mut no_texture = opaque_unshaded_draw(0, 0);
        no_texture.texture = None;
        let mut textured = opaque_unshaded_draw(1, 1);
        textured.texture = Some(TextureIndex(3));
        let scene = ScenePacket::new(
            frame(),
            vec![triangle_mesh(0, -0.95, -0.05), triangle_mesh(1, 0.05, 0.95)],
            vec![no_texture, textured],
            vec![SceneTextureRequest {
                index: TextureIndex(3),
                filename: "red.blp".into(),
                replaceable_id: 0,
                wrap_u: false,
                wrap_v: false,
            }],
        )
        .unwrap();
        let red = ResolvedSceneTexture {
            index: TextureIndex(3),
            width: 1,
            height: 1,
            rgba: vec![255, 0, 0, 255],
            color_space: CpuColorSpace::Srgb,
            alpha_encoding: CpuAlphaEncoding::Straight,
            address_u: TextureAddressMode::Clamp,
            address_v: TextureAddressMode::Clamp,
            origin: crate::texture::scene::TextureOrigin::Decoded {
                canonical_path: "red.blp".into(),
            },
        };
        let result = render_scene_offscreen(
            OffscreenSceneInput {
                packet: &scene,
                textures: &[red],
            },
            OffscreenSceneOptions {
                width: 64,
                height: 32,
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(pixel(&result, 16, 16), [255, 255, 255, 255]);
        assert_eq!(pixel(&result, 48, 16), [255, 0, 0, 255]);
    }

    #[tokio::test]
    async fn offscreen_additive_respects_layer_alpha_and_premultiplied_edges() {
        let mut quarter = opaque_unshaded_draw(0, 0);
        quarter.filter_mode = SceneFilterMode::Additive;
        quarter.layer_alpha = 0.25;
        quarter.geoset_alpha = 1.0;
        let mut full_with_half_geoset = opaque_unshaded_draw(1, 1);
        full_with_half_geoset.filter_mode = SceneFilterMode::Additive;
        full_with_half_geoset.layer_alpha = 1.0;
        full_with_half_geoset.geoset_alpha = 0.5;
        let levels = packet(
            vec![triangle_mesh(0, -0.95, -0.05), triangle_mesh(1, 0.05, 0.95)],
            vec![quarter, full_with_half_geoset],
        );
        let result = render_scene_offscreen(
            offscreen_input(&levels),
            OffscreenSceneOptions {
                width: 64,
                height: 32,
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert!((120..=150).contains(&pixel(&result, 16, 16)[0]));
        assert_eq!(&pixel(&result, 48, 16)[0..3], [255, 255, 255]);

        let mut glow_draw = opaque_unshaded_draw(0, 0);
        glow_draw.filter_mode = SceneFilterMode::Additive;
        glow_draw.texture = Some(TextureIndex(3));
        let glow_scene = ScenePacket::new(
            frame(),
            vec![triangle_mesh(0, -0.9, 0.9)],
            vec![glow_draw],
            vec![SceneTextureRequest {
                index: TextureIndex(3),
                filename: "team-glow".into(),
                replaceable_id: 2,
                wrap_u: false,
                wrap_v: false,
            }],
        )
        .unwrap();
        let glow = ResolvedSceneTexture {
            index: TextureIndex(3),
            width: 1,
            height: 1,
            rgba: vec![1, 0, 1, 1],
            color_space: CpuColorSpace::Srgb,
            alpha_encoding: CpuAlphaEncoding::Premultiplied,
            address_u: TextureAddressMode::Clamp,
            address_v: TextureAddressMode::Clamp,
            origin: crate::texture::scene::TextureOrigin::GeneratedTeamGlow,
        };
        let edge = render_scene_offscreen(
            OffscreenSceneInput {
                packet: &glow_scene,
                textures: &[glow],
            },
            OffscreenSceneOptions {
                width: 32,
                height: 32,
                ..Default::default()
            },
        )
        .await
        .unwrap();
        let edge_pixel = pixel(&edge, 16, 16);
        assert!((1..=30).contains(&edge_pixel[0]), "{edge_pixel:?}");
        assert!((1..=30).contains(&edge_pixel[2]), "{edge_pixel:?}");
    }

    #[tokio::test]
    async fn offscreen_transparent_alpha_test_uses_texture_geoset_and_layer_product() {
        let mut transparent = opaque_unshaded_draw(0, 0);
        transparent.filter_mode = SceneFilterMode::Transparent;
        transparent.geoset_alpha = 0.5;
        transparent.layer_alpha = 1.0;
        transparent.texture = Some(TextureIndex(3));
        let scene = ScenePacket::new(
            frame(),
            vec![triangle_mesh(0, -0.9, 0.9)],
            vec![transparent],
            vec![SceneTextureRequest {
                index: TextureIndex(3),
                filename: "opaque-alpha.blp".into(),
                replaceable_id: 0,
                wrap_u: false,
                wrap_v: false,
            }],
        )
        .unwrap();
        let opaque_texture = ResolvedSceneTexture {
            index: TextureIndex(3),
            width: 1,
            height: 1,
            rgba: vec![255; 4],
            color_space: CpuColorSpace::Srgb,
            alpha_encoding: CpuAlphaEncoding::Straight,
            address_u: TextureAddressMode::Clamp,
            address_v: TextureAddressMode::Clamp,
            origin: crate::texture::scene::TextureOrigin::Decoded {
                canonical_path: "opaque-alpha.blp".into(),
            },
        };
        let result = render_scene_offscreen(
            OffscreenSceneInput {
                packet: &scene,
                textures: &[opaque_texture],
            },
            OffscreenSceneOptions {
                width: 32,
                height: 32,
                clear: [0.2, 0.1, 0.0, 1.0],
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(pixel(&result, 16, 16), [124, 89, 0, 255]);
    }

    #[test]
    fn shader_preserves_scene_contract_branches() {
        let shader = include_str!("../shader.wgsl");
        for required in [
            "geoset_color_alpha",
            "layer_options",
            "texture_transform",
            "geoset_color_alpha.a < 0.01",
            "combined_alpha < 0.75",
            "unshaded",
            "sphere_env_map",
            "tex_color.a * scene_draw.geoset_color_alpha.a * layer_alpha",
            "base_uv",
            "texture_transform * vec4<f32>(base_uv",
            "eye_distance_squared > 0.0000001",
        ] {
            assert!(
                shader.contains(required),
                "missing shader contract: {required}"
            );
        }
    }

    #[tokio::test]
    async fn gpu_scene_uniform_buffers_allow_refresh_writes() {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: None,
                force_fallback_adapter: false,
            })
            .await
            .expect("scene uniform validation requires an available adapter");
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("PIPE scene uniform refresh validation"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                memory_hints: wgpu::MemoryHints::MemoryUsage,
                ..Default::default()
            })
            .await
            .unwrap();
        let scene_layout = scene_dynamic_material_layout(&device);
        device.push_error_scope(wgpu::ErrorFilter::Validation);

        let scene = packet(
            vec![mesh(0, [0.0; 3])],
            vec![draw(0, 0, SceneFilterMode::None)],
        );
        let mut prepared = prepare_scene(&scene, view()).unwrap();
        let gpu = upload_scene_gpu(&device, &prepared, &scene_layout).unwrap();
        refresh_prepared_scene_view(
            &mut prepared,
            SceneView {
                eye: [2.0, 3.0, 4.0],
                forward: [-2.0, -3.0, -4.0],
            },
        )
        .unwrap();
        queue.write_buffer(
            &gpu.uniform_buffer,
            0,
            bytemuck::cast_slice(&prepared.uniforms),
        );

        let empty = prepare_scene(&packet(Vec::new(), Vec::new()), view()).unwrap();
        let empty_gpu = upload_scene_gpu(&device, &empty, &scene_layout).unwrap();
        queue.write_buffer(&empty_gpu.uniform_buffer, 0, &[]);
        queue.write_buffer(
            &empty_gpu.uniform_buffer,
            0,
            bytemuck::bytes_of(&SceneDrawUniform::zeroed()),
        );

        let submission = queue.submit(std::iter::empty());
        device
            .poll(wgpu::PollType::Wait {
                submission_index: Some(submission),
                timeout: Some(std::time::Duration::from_secs(10)),
            })
            .unwrap();
        assert!(
            device.pop_error_scope().await.is_none(),
            "wgpu validation rejected a Scene uniform refresh write"
        );
    }

    #[tokio::test]
    async fn gpu_accepts_shader_all_pipeline_states_and_dynamic_uniform_layout() {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::LowPower,
                compatible_surface: None,
                force_fallback_adapter: false,
            })
            .await
            .expect("fixed GPU gate requires an available adapter");
        let (device, _) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("PIPE fixed GPU gate"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                memory_hints: wgpu::MemoryHints::MemoryUsage,
                ..Default::default()
            })
            .await
            .unwrap();
        assert!(device.limits().min_uniform_buffer_offset_alignment <= SCENE_UNIFORM_ALIGNMENT);
        device.push_error_scope(wgpu::ErrorFilter::Validation);
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("PIPE fixed GPU shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../shader.wgsl").into()),
        });
        let camera_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("PIPE camera layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let texture_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("PIPE texture layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let legacy_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("PIPE legacy material layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let scene_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("PIPE scene material layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: true,
                    min_binding_size: std::num::NonZeroU64::new(
                        size_of::<SceneDrawUniform>() as u64
                    ),
                },
                count: None,
            }],
        });
        for state in all_scene_pipeline_states() {
            let _pipeline = create_scene_pipeline(
                &device,
                &shader,
                wgpu::TextureFormat::Rgba8UnormSrgb,
                &camera_layout,
                &texture_layout,
                &legacy_layout,
                &scene_layout,
                state,
            );
        }
        let uniforms = [SceneDrawUniform::zeroed(), SceneDrawUniform::zeroed()];
        let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("PIPE dynamic uniform bytes"),
            contents: bytemuck::cast_slice(&uniforms),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let _bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("PIPE dynamic uniform bind group"),
            layout: &scene_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &buffer,
                    offset: 0,
                    size: std::num::NonZeroU64::new(size_of::<SceneDrawUniform>() as u64),
                }),
            }],
        });
        assert!(
            device.pop_error_scope().await.is_none(),
            "wgpu validation rejected the scene pipeline contract"
        );
    }
}
