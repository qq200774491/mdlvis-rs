use crate::error::MdlError;
use crate::material::{FilterMode, Material, MaterialUniform};
use crate::model::model::Model;
use crate::model::texture::Texture;
use crate::renderer::camera::CameraState;
use crate::renderer::geoset_render_info::GeosetRenderInfo;
use crate::renderer::line_vertex::LineVertex;
use crate::renderer::vertex::Vertex;
use crate::settings::Settings;
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
        })
    }

    pub fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        if new_size.width > 0 && new_size.height > 0 {
            self.config.width = new_size.width;
            self.config.height = new_size.height;
            self.surface.configure(&self.device, &self.config);
        }
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
            for i in 0..geoset.vertices.len() {
                let uv = if i < geoset.tex_coords.len() {
                    geoset.tex_coords[i].uv
                } else {
                    [0.0, 0.0] // Default UV if not available
                };

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
                "  Geoset {}: added {} vertices, {} UVs, {} faces, material_id: {:?}",
                geoset_idx,
                geoset.vertices.len(),
                geoset.tex_coords.len(),
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
