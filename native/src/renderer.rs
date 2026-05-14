use crate::params::{CloudParams, PostParams};
use anyhow::{Context, Result};
use std::sync::Arc;
use wgpu::util::DeviceExt;
use winit::window::Window;

const BLOOM_LEVELS: usize = 5;
const HDR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;

pub struct Renderer {
    pub window: Arc<Window>,
    pub surface: wgpu::Surface<'static>,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub config: wgpu::SurfaceConfiguration,

    // Cloud (scene) pass.
    pub scene_pipeline: wgpu::RenderPipeline,
    pub cube_pipeline: wgpu::RenderPipeline,
    pub cloud_uniform_buf: wgpu::Buffer,
    pub scene_bind_group: wgpu::BindGroup,

    // Bloom passes.
    pub bloom_bgl: wgpu::BindGroupLayout,
    pub extract_pipeline: wgpu::RenderPipeline,
    pub downsample_pipeline: wgpu::RenderPipeline,
    pub upsample_pipeline: wgpu::RenderPipeline,
    pub post_uniform_buf: wgpu::Buffer,
    pub linear_sampler: wgpu::Sampler,

    // Composite.
    pub composite_bgl: wgpu::BindGroupLayout,
    pub composite_pipeline: wgpu::RenderPipeline,

    // Resolution-dependent resources (recreated on resize).
    pub targets: Targets,
}

pub struct Targets {
    pub scene_view: wgpu::TextureView,
    pub _scene_tex: wgpu::Texture,
    pub bloom_views: Vec<wgpu::TextureView>,
    pub _bloom_texs: Vec<wgpu::Texture>,
    pub bloom_bind_groups: Vec<wgpu::BindGroup>, // one per source level (scene + each bloom level)
    pub composite_bind_group: wgpu::BindGroup,
}

impl Renderer {
    pub async fn new(window: Arc<Window>) -> Result<Self> {
        let size = window.inner_size();
        let w = size.width.max(1);
        let h = size.height.max(1);

        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::VULKAN | wgpu::Backends::GL,
            ..Default::default()
        });
        let surface = instance.create_surface(window.clone())?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .context("no compatible wgpu adapter")?;
        log::info!("adapter: {:?}", adapter.get_info());

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: Some("device"),
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::downlevel_defaults()
                        .using_resolution(adapter.limits()),
                    memory_hints: wgpu::MemoryHints::Performance,
                },
                None,
            )
            .await?;

        let config = surface
            .get_default_config(&adapter, w, h)
            .context("surface unsupported by adapter")?;
        surface.configure(&device, &config);

        let cloud_uniform_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("cloud-uniforms"),
            contents: bytemuck::bytes_of(&CloudParams::default()),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let post_uniform_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("post-uniforms"),
            contents: bytemuck::bytes_of(&PostParams::default()),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        // ---- scene bind group layout & pipeline ----
        let scene_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("scene-bgl"),
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
        let scene_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("scene-bg"),
            layout: &scene_bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: cloud_uniform_buf.as_entire_binding(),
            }],
        });
        let scene_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("clouds.wgsl"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/clouds.wgsl").into()),
        });
        let scene_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("scene-pl"),
            bind_group_layouts: &[&scene_bgl],
            push_constant_ranges: &[],
        });
        let scene_pipeline = make_fullscreen_pipeline(
            &device, &scene_pl, &scene_shader,
            "vs_fullscreen", "fs_clouds",
            HDR_FORMAT, None, "scene-clouds",
        );
        let cube_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("cube.wgsl"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/cube.wgsl").into()),
        });
        let cube_pipeline = make_fullscreen_pipeline(
            &device, &scene_pl, &cube_shader,
            "vs_fullscreen", "fs_cube",
            HDR_FORMAT, None, "scene-cube",
        );

        // ---- bloom bind group layout & pipelines ----
        let bloom_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("bloom-bgl"),
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
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let post_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("post.wgsl"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/post.wgsl").into()),
        });
        let bloom_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("bloom-pl"),
            bind_group_layouts: &[&bloom_bgl],
            push_constant_ranges: &[],
        });

        let extract_pipeline = make_fullscreen_pipeline(
            &device, &bloom_pl, &post_shader,
            "vs_fullscreen", "fs_extract",
            HDR_FORMAT, None, "bloom-extract",
        );
        let downsample_pipeline = make_fullscreen_pipeline(
            &device, &bloom_pl, &post_shader,
            "vs_fullscreen", "fs_downsample",
            HDR_FORMAT, None, "bloom-downsample",
        );
        // Upsample blends additively into the destination level.
        let add_blend = Some(wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent::REPLACE,
        });
        let upsample_pipeline = make_fullscreen_pipeline(
            &device, &bloom_pl, &post_shader,
            "vs_fullscreen", "fs_upsample",
            HDR_FORMAT, add_blend, "bloom-upsample",
        );

        // ---- composite bind group layout & pipeline ----
        let composite_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("composite-bgl"),
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
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let composite_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("composite.wgsl"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/composite.wgsl").into()),
        });
        let composite_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("composite-pl"),
            bind_group_layouts: &[&composite_bgl],
            push_constant_ranges: &[],
        });
        let composite_pipeline = make_fullscreen_pipeline(
            &device, &composite_pl, &composite_shader,
            "vs_fullscreen", "fs_composite",
            config.format, None, "composite-pipeline",
        );

        let linear_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("linear-clamp"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let targets = Targets::new(
            &device, w, h, &bloom_bgl, &composite_bgl,
            &linear_sampler, &post_uniform_buf,
        );

        Ok(Self {
            window, surface, device, queue, config,
            scene_pipeline, cube_pipeline, cloud_uniform_buf, scene_bind_group,
            bloom_bgl, extract_pipeline, downsample_pipeline, upsample_pipeline,
            post_uniform_buf, linear_sampler,
            composite_bgl, composite_pipeline,
            targets,
        })
    }

    pub fn resize(&mut self, w: u32, h: u32) {
        if w == 0 || h == 0 {
            return;
        }
        self.config.width = w;
        self.config.height = h;
        self.surface.configure(&self.device, &self.config);
        self.targets = Targets::new(
            &self.device, w, h, &self.bloom_bgl, &self.composite_bgl,
            &self.linear_sampler, &self.post_uniform_buf,
        );
    }

    pub fn write_cloud_params(&self, params: &CloudParams) {
        self.queue
            .write_buffer(&self.cloud_uniform_buf, 0, bytemuck::bytes_of(params));
    }

    pub fn write_post_params(&self, params: &PostParams) {
        self.queue
            .write_buffer(&self.post_uniform_buf, 0, bytemuck::bytes_of(params));
    }
}

impl Targets {
    fn new(
        device: &wgpu::Device,
        w: u32, h: u32,
        bloom_bgl: &wgpu::BindGroupLayout,
        composite_bgl: &wgpu::BindGroupLayout,
        sampler: &wgpu::Sampler,
        post_uniform_buf: &wgpu::Buffer,
    ) -> Self {
        let scene_tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("scene-hdr"),
            size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: HDR_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let scene_view = scene_tex.create_view(&Default::default());

        let mut bloom_texs = Vec::with_capacity(BLOOM_LEVELS);
        let mut bloom_views = Vec::with_capacity(BLOOM_LEVELS);
        for i in 0..BLOOM_LEVELS {
            let scale = 1u32 << (i + 1);
            let lw = (w / scale).max(1);
            let lh = (h / scale).max(1);
            let tex = device.create_texture(&wgpu::TextureDescriptor {
                label: Some(&format!("bloom-{i}")),
                size: wgpu::Extent3d { width: lw, height: lh, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: HDR_FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            });
            bloom_views.push(tex.create_view(&Default::default()));
            bloom_texs.push(tex);
        }

        // Bind groups for sampling each source level (scene + bloom 0..N-1).
        // Index 0 reads scene (used by extract), indices 1..=N-1 read bloom_views[i-1].
        let mut bloom_bind_groups = Vec::with_capacity(BLOOM_LEVELS + 1);
        let mk_bg = |label: &str, view: &wgpu::TextureView| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some(label),
                layout: bloom_bgl,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(view) },
                    wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(sampler) },
                    wgpu::BindGroupEntry { binding: 2, resource: post_uniform_buf.as_entire_binding() },
                ],
            })
        };
        bloom_bind_groups.push(mk_bg("bg-from-scene", &scene_view));
        for i in 0..BLOOM_LEVELS {
            bloom_bind_groups.push(mk_bg(&format!("bg-from-bloom{i}"), &bloom_views[i]));
        }

        let composite_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("composite-bg"),
            layout: composite_bgl,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&scene_view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(sampler) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(&bloom_views[0]) },
                wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::Sampler(sampler) },
                wgpu::BindGroupEntry { binding: 4, resource: post_uniform_buf.as_entire_binding() },
            ],
        });

        Self {
            scene_view,
            _scene_tex: scene_tex,
            bloom_views,
            _bloom_texs: bloom_texs,
            bloom_bind_groups,
            composite_bind_group,
        }
    }
}

fn make_fullscreen_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    shader: &wgpu::ShaderModule,
    vs_entry: &str,
    fs_entry: &str,
    format: wgpu::TextureFormat,
    blend: Option<wgpu::BlendState>,
    label: &str,
) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(label),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: vs_entry,
            buffers: &[],
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: fs_entry,
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend,
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: Default::default(),
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            ..Default::default()
        },
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview: None,
        cache: None,
    })
}
