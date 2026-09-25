use super::{
    context::GraphicsContext,
    gpu::{
        bind_group::{BindGroupCache, BindGroupEntryKey},
        growing::{ArenaAllocation, GrowingBufferArena},
        pipeline::{PipelineCache, RenderPipelineInfo},
    },
    image::Image,
    instance3d::InstanceArray3d,
    sampler::{Sampler, SamplerCache},
    shader::Shader,
    AlphaMode, BlendMode, Color, Draw3d, DrawCommand3d, DrawParam3d, DrawUniforms3d, LinearColor,
    Rect, RenderedMesh3d, Vertex3d, WgpuContext,
};
use crate::{GameError, GameResult};
use crevice::std140::AsStd140;
use glam::{Mat4, Vec4};
use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    num::NonZeroU64,
};

/// A canvas represents a render pass and is how you render meshes .
#[allow(missing_debug_implementations)]
pub struct InternalCanvas3d<'a> {
    wgpu: &'a WgpuContext,
    bind_group_cache: &'a mut BindGroupCache,
    pipeline_cache: &'a mut PipelineCache,
    sampler_cache: &'a mut SamplerCache,
    uniform_arena: &'a mut GrowingBufferArena,

    shader: Shader,
    shader_bind_group: Option<(wgpu::BindGroup, wgpu::BindGroupLayout, u32)>,

    shader_ty: Option<ShaderType3d>,
    dirty_pipeline: bool,
    alpha_mode: AlphaMode,
    blend_mode: BlendMode,
    pass: wgpu::RenderPass<'a>,
    samples: u32,
    format: wgpu::TextureFormat,

    draw_sm: &'a wgpu::ShaderModule,
    instance_sm: &'a wgpu::ShaderModule,
    instance_unordered_sm: &'a wgpu::ShaderModule,

    transform: glam::Mat4,
    curr_image: Option<wgpu::TextureView>,
    curr_sampler: Sampler,
    next_sampler: Sampler,

    uniform_alloc: Option<ArenaAllocation>,
}

impl<'a> InternalCanvas3d<'a> {
    pub fn from_image(
        gfx: &'a mut GraphicsContext,
        clear: impl Into<Option<Color>>,
        image: &'a Image,
        depth: &'a Image,
    ) -> GameResult<Self> {
        if image.samples() > 1 {
            return Err(GameError::RenderError(String::from("non-MSAA rendering requires an image with exactly 1 sample, for this image use Canvas::from_msaa instead")));
        }

        Self::new(gfx, 1, image.format(), |cmd| {
            cmd.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &image.view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: match clear.into() {
                            None => wgpu::LoadOp::Load,
                            Some(color) => wgpu::LoadOp::Clear(LinearColor::from(color).into()),
                        },
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: depth.wgpu(),
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                occlusion_query_set: None,
                timestamp_writes: None,
                multiview_mask: None,
            })
        })
    }

    pub fn from_msaa(
        gfx: &'a mut GraphicsContext,
        clear: impl Into<Option<Color>>,
        msaa_image: &'a Image,
        depth: &'a Image,
        resolve_image: &'a Image,
    ) -> GameResult<Self> {
        if msaa_image.samples() == 1 {
            return Err(GameError::RenderError(String::from(
                "MSAA rendering requires an image with more than 1 sample, for this image use Canvas::from_image instead",
            )));
        }

        if resolve_image.samples() > 1 {
            return Err(GameError::RenderError(String::from(
                "can only resolve into an image with exactly 1 sample",
            )));
        }

        if msaa_image.format() != resolve_image.format() {
            return Err(GameError::RenderError(String::from(
                "MSAA image and resolve image must be the same format",
            )));
        }

        Self::new(gfx, msaa_image.samples(), msaa_image.format(), |cmd| {
            cmd.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &msaa_image.view,
                    depth_slice: None,
                    resolve_target: Some(&resolve_image.view),
                    ops: wgpu::Operations {
                        load: match clear.into() {
                            None => wgpu::LoadOp::Load,
                            Some(color) => wgpu::LoadOp::Clear(LinearColor::from(color).into()),
                        },
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: depth.wgpu(),
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                occlusion_query_set: None,
                timestamp_writes: None,
                multiview_mask: None,
            })
        })
    }

    pub(crate) fn new(
        gfx: &'a mut GraphicsContext,
        samples: u32,
        format: wgpu::TextureFormat,
        create_pass: impl FnOnce(&'a mut wgpu::CommandEncoder) -> wgpu::RenderPass<'a>,
    ) -> GameResult<Self> {
        if gfx.fcx.is_none() {
            return Err(GameError::RenderError(String::from(
                "starting Canvas outside of a frame",
            )));
        }

        let drawable_size = gfx.drawable_size();

        let wgpu = &gfx.wgpu;
        let bind_group_cache = &mut gfx.bind_group_cache;
        let pipeline_cache = &mut gfx.pipeline_cache;
        let sampler_cache = &mut gfx.sampler_cache;
        let uniform_arena = &mut gfx.uniform_arena;

        let mut pass = {
            let fcx = gfx.fcx.as_mut().unwrap(/* see above */);
            create_pass(&mut fcx.cmd)
        };

        pass.set_blend_constant(wgpu::Color::BLACK);

        let screen_coords = Rect {
            x: 0.,
            y: 0.,
            w: drawable_size.0 as _,
            h: drawable_size.1 as _,
        };
        let transform = screen_to_mat(screen_coords);

        let shader = Shader {
            vs_module: None,
            fs_module: None,
        };

        Ok(InternalCanvas3d {
            wgpu,
            bind_group_cache,
            pipeline_cache,
            sampler_cache,
            uniform_arena,

            shader,
            shader_bind_group: None,

            shader_ty: None,
            dirty_pipeline: true,
            alpha_mode: AlphaMode::Discard { cutoff: 5 },
            pass,
            samples,
            format,

            draw_sm: &gfx.draw_shader_3d,
            instance_sm: &gfx.instance_shader_3d,
            instance_unordered_sm: &gfx.instance_unordered_shader_3d,

            transform,
            curr_image: None,
            curr_sampler: Sampler::default(),
            next_sampler: Sampler::default(),
            blend_mode: BlendMode::ALPHA,

            uniform_alloc: None,
        })
    }

    pub fn set_shader_params(
        &mut self,
        bind_group: wgpu::BindGroup,
        layout: wgpu::BindGroupLayout,
        offset: u32,
    ) {
        self.dirty_pipeline = true;
        self.shader_bind_group = Some((bind_group, layout, offset));
    }

    pub fn reset_shader_params(&mut self) {
        self.dirty_pipeline = true;
        self.shader_bind_group = None;
    }

    pub fn set_shader(&mut self, shader: Shader) {
        self.dirty_pipeline = true;
        self.shader = shader;
    }

    pub fn set_sampler(&mut self, sampler: Sampler) {
        self.next_sampler = sampler;
    }

    pub fn set_alpha_mode(&mut self, alpha_mode: AlphaMode) {
        self.dirty_pipeline = true;
        self.alpha_mode = alpha_mode;
    }

    pub fn set_projection(&mut self, proj: impl Into<mint::ColumnMatrix4<f32>>) {
        self.transform = proj.into().into();
    }

    pub fn set_scissor_rect(&mut self, (x, y, w, h): (u32, u32, u32, u32)) {
        self.pass.set_scissor_rect(x, y, w, h);
    }

    pub(crate) fn update_uniform(&mut self, draws: &[DrawCommand3d]) {
        let alignment = self
            .wgpu
            .device
            .limits()
            .min_uniform_buffer_offset_alignment as u64;
        let mut alloc_size = 0;
        let mut uniforms = Vec::new();
        for draw in draws {
            if draw.state.projection != self.transform.into() {
                self.set_projection(draw.state.projection);
            }
            if let Draw3d::Mesh { .. } = &draw.draw {
                alloc_size += alignment;
                let draw_uniform =
                    DrawUniforms3d::from_param(&draw.param).projection(self.transform);
                let mut bytes = draw_uniform.as_std140().as_bytes().to_vec();
                let needed_padding = alignment - (bytes.len() as u64 % alignment); // Pad the uniforms so we can index properly
                bytes.resize(bytes.len() + needed_padding as usize, 0);
                uniforms.extend_from_slice(bytes.as_slice());
            }
        }

        let uniform_alloc = self.uniform_arena.allocate(&self.wgpu.device, alloc_size);
        self.wgpu.queue.write_buffer(
            &uniform_alloc.buffer,
            uniform_alloc.offset,
            uniforms.as_slice(),
        );

        self.uniform_alloc = Some(uniform_alloc);
    }

    pub fn draw_mesh(&mut self, mesh: &'a RenderedMesh3d, image: &Image, idx: usize) {
        // the 3D draw path went the same way as its 2D counterpart: the
        // pipeline assembly from update_pipeline, the bind group caches and
        // the image/sampler bookkeeping from set_image all live inside the
        // draw call now.
        let ty = ShaderType3d::Draw;

        if self.dirty_pipeline || self.shader_ty != Some(ty) {
            self.dirty_pipeline = false;
            self.shader_ty = Some(ty);

            // sampled texture + filtering sampler, both fragment-stage
            let texture_layout = self
                .bind_group_cache
                .layouts
                .entry((
                    vec![
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
                    0,
                ))
                .or_insert_with_key(|(entries, _)| {
                    self.wgpu
                        .device
                        .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                            label: None,
                            entries,
                        })
                })
                .clone();

            // two read-only storage buffers for the instance data, vertex-stage
            let instance_layout = self
                .bind_group_cache
                .layouts
                .entry((
                    vec![
                        wgpu::BindGroupLayoutEntry {
                            binding: 0,
                            visibility: wgpu::ShaderStages::VERTEX,
                            ty: wgpu::BindingType::Buffer {
                                ty: wgpu::BufferBindingType::Storage { read_only: true },
                                has_dynamic_offset: false,
                                min_binding_size: None,
                            },
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 1,
                            visibility: wgpu::ShaderStages::VERTEX,
                            ty: wgpu::BindingType::Buffer {
                                ty: wgpu::BufferBindingType::Storage { read_only: true },
                                has_dynamic_offset: false,
                                min_binding_size: None,
                            },
                            count: None,
                        },
                    ],
                    0,
                ))
                .or_insert_with_key(|(entries, _)| {
                    self.wgpu
                        .device
                        .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                            label: None,
                            entries,
                        })
                })
                .clone();

            // the uniform binding is seeded by the shader kind
            let uniform_seed = {
                let mut hasher = DefaultHasher::new();
                ty.hash(&mut hasher);
                hasher.finish()
            };
            let uniform_layout = self
                .bind_group_cache
                .layouts
                .entry((
                    vec![wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::VERTEX,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: true,
                            min_binding_size: None,
                        },
                        count: None,
                    }],
                    uniform_seed,
                ))
                .or_insert_with_key(|(entries, _)| {
                    self.wgpu
                        .device
                        .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                            label: None,
                            entries,
                        })
                })
                .clone();

            let (dummy_group, dummy_layout) = {
                let dummy_layout = self
                    .bind_group_cache
                    .layouts
                    .entry((Vec::new(), 0))
                    .or_insert_with_key(|(entries, _)| {
                        self.wgpu
                            .device
                            .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                                label: None,
                                entries,
                            })
                    })
                    .clone();
                let dummy_group = self
                    .bind_group_cache
                    .groups
                    .entry(Vec::new())
                    .or_insert_with(|| {
                        self.wgpu
                            .device
                            .create_bind_group(&wgpu::BindGroupDescriptor {
                                label: None,
                                layout: &dummy_layout,
                                entries: &[],
                            })
                    })
                    .clone();
                (dummy_group, dummy_layout)
            };

            let mut groups = vec![Some(&uniform_layout), Some(&texture_layout)];

            if let ShaderType3d::Instance { .. } = ty {
                groups.push(Some(&instance_layout));
            } else {
                // the dummy group ensures the user's bind group is at index 3
                groups.push(Some(&dummy_layout));
                self.pass.set_bind_group(2, &dummy_group, &[]);
            }

            let shader = match ty {
                ShaderType3d::Draw | ShaderType3d::Instance { .. } => {
                    if let Some((ref bind_group, ref bind_group_layout, offset)) =
                        self.shader_bind_group
                    {
                        self.pass.set_bind_group(3, bind_group, &[offset]);
                        groups.push(Some(bind_group_layout));
                    }

                    &self.shader
                }
            };

            // pipeline layout keyed by the hashed bind group layouts
            let layout = {
                let key = {
                    let mut hasher = DefaultHasher::new();
                    for bg in &groups {
                        bg.hash(&mut hasher);
                    }
                    hasher.finish()
                };
                self.pipeline_cache
                    .layouts
                    .entry(key)
                    .or_insert_with(|| {
                        self.wgpu
                            .device
                            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                                label: None,
                                bind_group_layouts: &groups,
                                immediate_size: 0,
                            })
                    })
                    .clone()
            };

            // the pipeline itself, cached by its full descriptor
            let pipeline = {
                let info = RenderPipelineInfo {
                    layout,
                    vs: if let Some(vs_module) = &shader.vs_module {
                        vs_module.clone()
                    } else {
                        match ty {
                            ShaderType3d::Draw => self.draw_sm.clone(),
                            ShaderType3d::Instance { ordered: true } => self.instance_sm.clone(),
                            ShaderType3d::Instance { ordered: false } => {
                                self.instance_unordered_sm.clone()
                            }
                        }
                    },
                    fs: if let Some(fs_module) = &shader.fs_module {
                        fs_module.clone()
                    } else {
                        match ty {
                            ShaderType3d::Draw | ShaderType3d::Instance { .. } => {
                                self.draw_sm.clone()
                            }
                        }
                    },
                    vs_entry: "vs_main",
                    fs_entry: "fs_main",
                    samples: self.samples,
                    format: self.format,
                    blend: Some(wgpu::BlendState {
                        color: self.blend_mode.color,
                        alpha: self.blend_mode.alpha,
                    }),
                    depth: Some(wgpu::CompareFunction::Less),
                    vertices: true,
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    vertex_layout: Vertex3d::desc(),
                    cull_mode: Some(wgpu::Face::Back),
                };
                let vertex_buffers = [Some(info.vertex_layout.clone())];
                self.pipeline_cache
                    .pipelines
                    .entry(info)
                    .or_insert_with_key(|info| {
                        self.wgpu
                            .device
                            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                                label: None,
                                layout: Some(&info.layout),
                                vertex: wgpu::VertexState {
                                    module: &info.vs,
                                    entry_point: Some("vs_main"),
                                    compilation_options:
                                        wgpu::PipelineCompilationOptions::default(),
                                    buffers: if info.vertices { &vertex_buffers } else { &[] },
                                },
                                primitive: wgpu::PrimitiveState {
                                    topology: info.topology,
                                    strip_index_format: None,
                                    front_face: wgpu::FrontFace::Ccw,
                                    cull_mode: info.cull_mode,
                                    unclipped_depth: false,
                                    polygon_mode: wgpu::PolygonMode::Fill,
                                    conservative: false,
                                },
                                depth_stencil: info
                                    .depth
                                    .map(|depth_compare| wgpu::DepthStencilState {
                                        format: wgpu::TextureFormat::Depth32Float,
                                        depth_write_enabled: Some(true),
                                        depth_compare: Some(depth_compare),
                                        stencil: Default::default(),
                                        bias: Default::default(),
                                    }),
                                multisample: wgpu::MultisampleState {
                                    count: info.samples,
                                    mask: !0,
                                    alpha_to_coverage_enabled: false,
                                },
                                fragment: Some(wgpu::FragmentState {
                                    module: &info.fs,
                                    entry_point: Some("fs_main"),
                                    compilation_options:
                                        wgpu::PipelineCompilationOptions::default(),
                                    targets: &[Some(wgpu::ColorTargetState {
                                        format: info.format,
                                        blend: info.blend,
                                        write_mask: wgpu::ColorWrites::ALL,
                                    })],
                                }),
                                multiview_mask: None,
                                cache: None,
                            })
                    })
                    .clone()
            };

            self.pass.set_pipeline(&pipeline);
        }

        let alloc_size = DrawUniforms3d::std140_size_static() as u64;

        // the draw's own uniform bind group, straight from its cache key
        let uniform_bind_layout = self
            .bind_group_cache
            .layouts
            .entry((
                vec![wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: true,
                        min_binding_size: None,
                    },
                    count: None,
                }],
                0,
            ))
            .or_insert_with_key(|(entries, _)| {
                self.wgpu
                    .device
                    .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                        label: None,
                        entries,
                    })
            })
            .clone();
        let uniform_bind_group = self
            .bind_group_cache
            .groups
            .entry(vec![BindGroupEntryKey::Buffer {
                buffer: self.uniform_alloc.as_ref().unwrap().buffer.clone(),
                offset: 0,
                size: Some(alloc_size),
            }])
            .or_insert_with(|| {
                self.wgpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: None,
                    layout: &uniform_bind_layout,
                    entries: &[wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                            buffer: &self.uniform_alloc.as_ref().unwrap().buffer,
                            offset: 0,
                            size: NonZeroU64::new(alloc_size),
                        }),
                    }],
                })
            })
            .clone();

        // set_image body inlined, sampler cache and image-local bind group
        // cache included
        let owned_image = image.clone();
        if self.curr_sampler != self.next_sampler
            || self
                .curr_image
                .as_ref()
                .is_none_or(|curr| *curr != owned_image.view)
        {
            self.curr_sampler = self.next_sampler;

            let sampler_key = self.curr_sampler;
            let sample = self
                .sampler_cache
                .cache
                .entry(sampler_key)
                .or_insert_with(|| {
                    let descriptor: wgpu::SamplerDescriptor = sampler_key.into();
                    self.wgpu.device.create_sampler(&descriptor)
                })
                .clone();

            // the image keeps its own bind group cache keyed by sampler
            let cached_bind = owned_image.cache.read().unwrap().get(&sample).cloned();
            let image_bind = match cached_bind {
                Some(bind) => bind,
                None => owned_image
                    .cache
                    .write()
                    .unwrap()
                    .entry(sample)
                    .or_insert_with_key(|sampler| {
                        let layout =
                            self.wgpu
                                .device
                                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                                    label: None,
                                    entries: &[
                                        wgpu::BindGroupLayoutEntry {
                                            binding: 0,
                                            visibility: wgpu::ShaderStages::FRAGMENT,
                                            ty: wgpu::BindingType::Texture {
                                                sample_type:
                                                    wgpu::TextureSampleType::Float {
                                                    filterable: true,
                                                },
                                                view_dimension: wgpu::TextureViewDimension::D2,
                                                multisampled: false,
                                            },
                                            count: None,
                                        },
                                        wgpu::BindGroupLayoutEntry {
                                            binding: 1,
                                            visibility: wgpu::ShaderStages::FRAGMENT,
                                            ty: wgpu::BindingType::Sampler(
                                                wgpu::SamplerBindingType::Filtering,
                                            ),
                                            count: None,
                                        },
                                    ],
                                });
                        self.wgpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
                            label: None,
                            layout: &layout,
                            entries: &[
                                wgpu::BindGroupEntry {
                                    binding: 0,
                                    resource: wgpu::BindingResource::TextureView(
                                        &owned_image.view,
                                    ),
                                },
                                wgpu::BindGroupEntry {
                                    binding: 1,
                                    resource: wgpu::BindingResource::Sampler(sampler),
                                },
                            ],
                        })
                    })
                    .clone(),
            };

            self.curr_image = Some(owned_image.view);

            self.pass.set_bind_group(1, &image_bind, &[]);
        }

        // 1. allocate some uniform buffer memory from GrowingBufferArena.
        // 2. write the uniform data to that memory
        // 3. use a "dynamic offset" to offset into the memory

        let offset = self.uniform_alloc.as_ref().unwrap().offset + (idx as u64 * 256);

        self.pass.set_bind_group(
            0,
            &uniform_bind_group,
            &[offset as u32], // <- the dynamic offset
        );

        self.pass.set_vertex_buffer(0, mesh.vert_buffer.slice(..));
        self.pass
            .set_index_buffer(mesh.ind_buffer.slice(..), wgpu::IndexFormat::Uint32);

        self.pass.draw_indexed(0..mesh.ind_len as _, 0, 0..1);
    }

    pub fn draw_mesh_instances(
        &mut self,
        mesh: &'a RenderedMesh3d,
        instances: &'a InstanceArrayView3d,
        param: DrawParam3d,
    ) -> GameResult {
        if instances.len == 0 {
            return Ok(());
        }

        // pipeline assembly inlined here too -- instanced draws keep the same
        // layout as their plain siblings so the two paths stay obviously
        // parallel
        let ty = ShaderType3d::Instance {
            ordered: instances.ordered,
        };

        if self.dirty_pipeline || self.shader_ty != Some(ty) {
            self.dirty_pipeline = false;
            self.shader_ty = Some(ty);

            // sampled texture + filtering sampler, both fragment-stage
            let texture_layout = self
                .bind_group_cache
                .layouts
                .entry((
                    vec![
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
                    0,
                ))
                .or_insert_with_key(|(entries, _)| {
                    self.wgpu
                        .device
                        .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                            label: None,
                            entries,
                        })
                })
                .clone();

            // two read-only storage buffers for the instance data, vertex-stage
            let instance_layout = self
                .bind_group_cache
                .layouts
                .entry((
                    vec![
                        wgpu::BindGroupLayoutEntry {
                            binding: 0,
                            visibility: wgpu::ShaderStages::VERTEX,
                            ty: wgpu::BindingType::Buffer {
                                ty: wgpu::BufferBindingType::Storage { read_only: true },
                                has_dynamic_offset: false,
                                min_binding_size: None,
                            },
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 1,
                            visibility: wgpu::ShaderStages::VERTEX,
                            ty: wgpu::BindingType::Buffer {
                                ty: wgpu::BufferBindingType::Storage { read_only: true },
                                has_dynamic_offset: false,
                                min_binding_size: None,
                            },
                            count: None,
                        },
                    ],
                    0,
                ))
                .or_insert_with_key(|(entries, _)| {
                    self.wgpu
                        .device
                        .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                            label: None,
                            entries,
                        })
                })
                .clone();

            // the uniform binding is seeded by the shader kind
            let uniform_seed = {
                let mut hasher = DefaultHasher::new();
                ty.hash(&mut hasher);
                hasher.finish()
            };
            let uniform_layout = self
                .bind_group_cache
                .layouts
                .entry((
                    vec![wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::VERTEX,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: true,
                            min_binding_size: None,
                        },
                        count: None,
                    }],
                    uniform_seed,
                ))
                .or_insert_with_key(|(entries, _)| {
                    self.wgpu
                        .device
                        .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                            label: None,
                            entries,
                        })
                })
                .clone();

            let (dummy_group, dummy_layout) = {
                let dummy_layout = self
                    .bind_group_cache
                    .layouts
                    .entry((Vec::new(), 0))
                    .or_insert_with_key(|(entries, _)| {
                        self.wgpu
                            .device
                            .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                                label: None,
                                entries,
                            })
                    })
                    .clone();
                let dummy_group = self
                    .bind_group_cache
                    .groups
                    .entry(Vec::new())
                    .or_insert_with(|| {
                        self.wgpu
                            .device
                            .create_bind_group(&wgpu::BindGroupDescriptor {
                                label: None,
                                layout: &dummy_layout,
                                entries: &[],
                            })
                    })
                    .clone();
                (dummy_group, dummy_layout)
            };

            let mut groups = vec![Some(&uniform_layout), Some(&texture_layout)];

            if let ShaderType3d::Instance { .. } = ty {
                groups.push(Some(&instance_layout));
            } else {
                // the dummy group ensures the user's bind group is at index 3
                groups.push(Some(&dummy_layout));
                self.pass.set_bind_group(2, &dummy_group, &[]);
            }

            let shader = match ty {
                ShaderType3d::Draw | ShaderType3d::Instance { .. } => {
                    if let Some((ref bind_group, ref bind_group_layout, offset)) =
                        self.shader_bind_group
                    {
                        self.pass.set_bind_group(3, bind_group, &[offset]);
                        groups.push(Some(bind_group_layout));
                    }

                    &self.shader
                }
            };

            let layout = {
                let key = {
                    let mut hasher = DefaultHasher::new();
                    for bg in &groups {
                        bg.hash(&mut hasher);
                    }
                    hasher.finish()
                };
                self.pipeline_cache
                    .layouts
                    .entry(key)
                    .or_insert_with(|| {
                        self.wgpu
                            .device
                            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                                label: None,
                                bind_group_layouts: &groups,
                                immediate_size: 0,
                            })
                    })
                    .clone()
            };

            let pipeline = {
                let info = RenderPipelineInfo {
                    layout,
                    vs: if let Some(vs_module) = &shader.vs_module {
                        vs_module.clone()
                    } else {
                        match ty {
                            ShaderType3d::Draw => self.draw_sm.clone(),
                            ShaderType3d::Instance { ordered: true } => self.instance_sm.clone(),
                            ShaderType3d::Instance { ordered: false } => {
                                self.instance_unordered_sm.clone()
                            }
                        }
                    },
                    fs: if let Some(fs_module) = &shader.fs_module {
                        fs_module.clone()
                    } else {
                        match ty {
                            ShaderType3d::Draw | ShaderType3d::Instance { .. } => {
                                self.draw_sm.clone()
                            }
                        }
                    },
                    vs_entry: "vs_main",
                    fs_entry: "fs_main",
                    samples: self.samples,
                    format: self.format,
                    blend: Some(wgpu::BlendState {
                        color: self.blend_mode.color,
                        alpha: self.blend_mode.alpha,
                    }),
                    depth: Some(wgpu::CompareFunction::Less),
                    vertices: true,
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    vertex_layout: Vertex3d::desc(),
                    cull_mode: Some(wgpu::Face::Back),
                };
                let vertex_buffers = [Some(info.vertex_layout.clone())];
                self.pipeline_cache
                    .pipelines
                    .entry(info)
                    .or_insert_with_key(|info| {
                        self.wgpu
                            .device
                            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                                label: None,
                                layout: Some(&info.layout),
                                vertex: wgpu::VertexState {
                                    module: &info.vs,
                                    entry_point: Some("vs_main"),
                                    compilation_options:
                                        wgpu::PipelineCompilationOptions::default(),
                                    buffers: if info.vertices { &vertex_buffers } else { &[] },
                                },
                                primitive: wgpu::PrimitiveState {
                                    topology: info.topology,
                                    strip_index_format: None,
                                    front_face: wgpu::FrontFace::Ccw,
                                    cull_mode: info.cull_mode,
                                    unclipped_depth: false,
                                    polygon_mode: wgpu::PolygonMode::Fill,
                                    conservative: false,
                                },
                                depth_stencil: info
                                    .depth
                                    .map(|depth_compare| wgpu::DepthStencilState {
                                        format: wgpu::TextureFormat::Depth32Float,
                                        depth_write_enabled: Some(true),
                                        depth_compare: Some(depth_compare),
                                        stencil: Default::default(),
                                        bias: Default::default(),
                                    }),
                                multisample: wgpu::MultisampleState {
                                    count: info.samples,
                                    mask: !0,
                                    alpha_to_coverage_enabled: false,
                                },
                                fragment: Some(wgpu::FragmentState {
                                    module: &info.fs,
                                    entry_point: Some("fs_main"),
                                    compilation_options:
                                        wgpu::PipelineCompilationOptions::default(),
                                    targets: &[Some(wgpu::ColorTargetState {
                                        format: info.format,
                                        blend: info.blend,
                                        write_mask: wgpu::ColorWrites::ALL,
                                    })],
                                }),
                                multiview_mask: None,
                                cache: None,
                            })
                    })
                    .clone()
            };

            self.pass.set_pipeline(&pipeline);
        }

        let alloc_size = u64::from(
            self.wgpu
                .device
                .limits()
                .min_uniform_buffer_offset_alignment,
        );

        // the arena bump from GrowingBufferArena::allocate, unfurled in place:
        // align to the uniform offset limit, walk the cursors, and on the
        // miss path append a new buffer exactly like the old grow-and-retry
        let aligned_size =
            (alloc_size + self.uniform_arena.alignment - 1) & !(self.uniform_arena.alignment - 1);
        assert!(aligned_size <= self.uniform_arena.desc.size);
        let uniform_alloc = {
            let mut found: Option<ArenaAllocation> = None;
            for (buffer, cursor) in &mut self.uniform_arena.buffers {
                if aligned_size <= self.uniform_arena.desc.size - *cursor {
                    let offset = *cursor;
                    *cursor += aligned_size;
                    found = Some(ArenaAllocation {
                        buffer: buffer.clone(),
                        offset,
                    });
                    break;
                }
            }
            match found {
                Some(allocation) => allocation,
                None => {
                    let buffer = self.wgpu.device.create_buffer(&self.uniform_arena.desc);
                    self.uniform_arena.buffers.push((buffer.clone(), aligned_size));
                    ArenaAllocation { buffer, offset: 0 }
                }
            }
        };

        // the instance uniform bind group, through its cache entries
        let uniform_bind_layout = self
            .bind_group_cache
            .layouts
            .entry((
                vec![wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: true,
                        min_binding_size: None,
                    },
                    count: None,
                }],
                0,
            ))
            .or_insert_with_key(|(entries, _)| {
                self.wgpu
                    .device
                    .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                        label: None,
                        entries,
                    })
            })
            .clone();
        let uniform_bind_group = self
            .bind_group_cache
            .groups
            .entry(vec![BindGroupEntryKey::Buffer {
                buffer: uniform_alloc.buffer.clone(),
                offset: 0,
                size: Some(alloc_size),
            }])
            .or_insert_with(|| {
                self.wgpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: None,
                    layout: &uniform_bind_layout,
                    entries: &[wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                            buffer: &uniform_alloc.buffer,
                            offset: 0,
                            size: NonZeroU64::new(alloc_size),
                        }),
                    }],
                })
            })
            .clone();

        // set_image body inlined for the instanced image
        let owned_image = instances.image.clone();
        if self.curr_sampler != self.next_sampler
            || self
                .curr_image
                .as_ref()
                .is_none_or(|curr| *curr != owned_image.view)
        {
            self.curr_sampler = self.next_sampler;

            let inst_sampler_key = self.curr_sampler;
            let inst_sampler = self
                .sampler_cache
                .cache
                .entry(inst_sampler_key)
                .or_insert_with(|| {
                    let descriptor: wgpu::SamplerDescriptor = inst_sampler_key.into();
                    self.wgpu.device.create_sampler(&descriptor)
                })
                .clone();

            let cached_bind = owned_image.cache.read().unwrap().get(&inst_sampler).cloned();
            let image_bind = match cached_bind {
                Some(bind) => bind,
                None => owned_image
                    .cache
                    .write()
                    .unwrap()
                    .entry(inst_sampler)
                    .or_insert_with_key(|sampler| {
                        let layout =
                            self.wgpu
                                .device
                                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                                    label: None,
                                    entries: &[
                                        wgpu::BindGroupLayoutEntry {
                                            binding: 0,
                                            visibility: wgpu::ShaderStages::FRAGMENT,
                                            ty: wgpu::BindingType::Texture {
                                                sample_type:
                                                    wgpu::TextureSampleType::Float {
                                                    filterable: true,
                                                },
                                                view_dimension: wgpu::TextureViewDimension::D2,
                                                multisampled: false,
                                            },
                                            count: None,
                                        },
                                        wgpu::BindGroupLayoutEntry {
                                            binding: 1,
                                            visibility: wgpu::ShaderStages::FRAGMENT,
                                            ty: wgpu::BindingType::Sampler(
                                                wgpu::SamplerBindingType::Filtering,
                                            ),
                                            count: None,
                                        },
                                    ],
                                });
                        self.wgpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
                            label: None,
                            layout: &layout,
                            entries: &[
                                wgpu::BindGroupEntry {
                                    binding: 0,
                                    resource: wgpu::BindingResource::TextureView(
                                        &owned_image.view,
                                    ),
                                },
                                wgpu::BindGroupEntry {
                                    binding: 1,
                                    resource: wgpu::BindingResource::Sampler(sampler),
                                },
                            ],
                        })
                    })
                    .clone(),
            };

            self.curr_image = Some(owned_image.view);

            self.pass.set_bind_group(1, &image_bind, &[]);
        }

        let draw_uniforms = DrawUniforms3d::from_param(&param).projection(self.transform);
        let uniforms = InstanceUniforms3d {
            model_transform: draw_uniforms.model_transform,
            camera_transform: draw_uniforms.camera_transform,
            color: glam::Vec4::from_array(param.color.into()),
        };

        self.wgpu.queue.write_buffer(
            &uniform_alloc.buffer,
            uniform_alloc.offset,
            uniforms.as_std140().as_bytes(),
        );

        self.pass
            .set_bind_group(0, &uniform_bind_group, &[uniform_alloc.offset as u32]);
        self.pass.set_bind_group(2, &instances.bind_group, &[]);

        self.pass.set_vertex_buffer(0, mesh.vert_buffer.slice(..)); // These buffers should always exist if I recall correctly
        self.pass
            .set_index_buffer(mesh.ind_buffer.slice(..), wgpu::IndexFormat::Uint32);

        self.pass
            .draw_indexed(0..mesh.ind_len as _, 0, 0..instances.len as _);

        Ok(())
    }

    pub fn finish(mut self) {
        self.finalize();
    }

    fn finalize(&mut self) {}
}

impl Drop for InternalCanvas3d<'_> {
    fn drop(&mut self) {
        self.finalize();
    }
}

#[derive(Debug)]
pub struct InstanceArrayView3d {
    pub bind_group: wgpu::BindGroup,
    pub image: Image,
    pub len: u32,
    pub ordered: bool,
}

impl InstanceArrayView3d {
    pub fn from_instances(ia: &InstanceArray3d) -> GameResult<Self> {
        Ok(InstanceArrayView3d {
            bind_group: ia
                .bind_group
                .lock()
                .map_err(|_| GameError::LockError)?
                .clone(),
            image: ia.image.clone(),
            len: ia.instances().len() as u32,
            ordered: ia.ordered,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum ShaderType3d {
    Draw,
    Instance { ordered: bool },
}

#[derive(crevice::std140::AsStd140)]
struct InstanceUniforms3d {
    pub color: Vec4,
    pub model_transform: Mat4,
    pub camera_transform: Mat4,
}

pub(crate) fn screen_to_mat(screen: Rect) -> glam::Mat4 {
    glam::camera::rh::proj::directx::orthographic(
        screen.left(),
        screen.right(),
        screen.bottom(),
        screen.top(),
        0.,
        1.,
    )
}
