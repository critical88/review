use super::{
    context::GraphicsContext,
    draw::{DrawParam, DrawUniforms},
    gpu::{
        bind_group::{BindGroupBuilder, BindGroupCache, BindGroupEntryKey, BindGroupLayoutBuilder},
        growing::{ArenaAllocation, GrowingBufferArena},
        pipeline::{PipelineCache, RenderPipelineInfo},
        text::{Extra, TextRenderer, TextVertex},
    },
    image::Image,
    mesh::{Mesh, Vertex},
    sampler::{Sampler, SamplerCache},
    shader::Shader,
    BlendMode, Color, InstanceArray, LinearColor, Rect, Text, Transform, WgpuContext,
};
use crate::{GameError, GameResult};
use crevice::std140::AsStd140;
use glyph_brush::GlyphCruncher;
use glam::{Mat4, Vec2, Vec4};
use std::{
    collections::{hash_map::DefaultHasher, HashMap},
    hash::{Hash, Hasher},
    num::NonZeroU64,
};

/// A canvas represents a render pass and is how you render primitives such as meshes and text onto images.
#[allow(missing_debug_implementations)]
pub struct InternalCanvas<'a> {
    wgpu: &'a WgpuContext,
    bind_group_cache: &'a mut BindGroupCache,
    pipeline_cache: &'a mut PipelineCache,
    sampler_cache: &'a mut SamplerCache,
    text_renderer: &'a mut TextRenderer,
    fonts: &'a HashMap<String, glyph_brush::FontId>,
    uniform_arena: &'a mut GrowingBufferArena,

    shader: Shader,
    shader_bind_group: Option<(wgpu::BindGroup, wgpu::BindGroupLayout, u32)>,
    text_shader: Shader,
    text_shader_bind_group: Option<(wgpu::BindGroup, wgpu::BindGroupLayout, u32)>,

    shader_ty: Option<ShaderType>,
    dirty_pipeline: bool,
    queuing_text: bool,
    blend_mode: BlendMode,
    pass: wgpu::RenderPass<'a>,
    samples: u32,
    format: wgpu::TextureFormat,
    text_uniforms: ArenaAllocation,

    draw_sm: &'a wgpu::ShaderModule,
    instance_sm: &'a wgpu::ShaderModule,
    instance_unordered_sm: &'a wgpu::ShaderModule,
    text_sm: &'a wgpu::ShaderModule,

    transform: glam::Mat4,
    curr_image: Option<wgpu::TextureView>,
    curr_sampler: Sampler,
    next_sampler: Sampler,
    premul_text: bool,
}

impl<'a> InternalCanvas<'a> {
    pub fn from_image(
        gfx: &'a mut GraphicsContext,
        clear: impl Into<Option<Color>>,
        image: &'a Image,
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
                depth_stencil_attachment: None,
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
                depth_stencil_attachment: None,
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
        let text_renderer = &mut gfx.text;
        let fonts = &gfx.fonts;
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

        let text_shader = Shader {
            vs_module: None,
            fs_module: None,
        };

        let text_uniforms =
            uniform_arena.allocate(&wgpu.device, TextUniforms::std140_size_static() as _);

        wgpu.queue.write_buffer(
            &text_uniforms.buffer,
            text_uniforms.offset,
            (TextUniforms { transform }).as_std140().as_bytes(),
        );

        Ok(InternalCanvas {
            wgpu,
            bind_group_cache,
            pipeline_cache,
            sampler_cache,
            text_renderer,
            fonts,
            uniform_arena,

            shader,
            shader_bind_group: None,
            text_shader,
            text_shader_bind_group: None,

            shader_ty: None,
            dirty_pipeline: true,
            queuing_text: false,
            blend_mode: BlendMode::ALPHA,
            pass,
            samples,
            format,
            text_uniforms,

            draw_sm: &gfx.draw_shader,
            instance_sm: &gfx.instance_shader,
            instance_unordered_sm: &gfx.instance_unordered_shader,
            text_sm: &gfx.text_shader,

            transform,
            curr_image: None,
            curr_sampler: Sampler::default(),
            next_sampler: Sampler::default(),
            premul_text: true,
        })
    }

    pub fn set_shader_params(
        &mut self,
        bind_group: wgpu::BindGroup,
        layout: wgpu::BindGroupLayout,
        offset: u32,
    ) {
        self.flush_text();
        self.dirty_pipeline = true;
        self.shader_bind_group = Some((bind_group, layout, offset));
    }

    pub fn reset_shader_params(&mut self) {
        self.flush_text();
        self.dirty_pipeline = true;
        self.shader_bind_group = None;
    }

    pub fn set_shader(&mut self, shader: Shader) {
        self.flush_text();
        self.dirty_pipeline = true;
        self.shader = shader;
    }

    pub fn set_text_shader_params(
        &mut self,
        bind_group: wgpu::BindGroup,
        layout: wgpu::BindGroupLayout,
        offset: u32,
    ) {
        self.flush_text();
        self.dirty_pipeline = true;
        self.text_shader_bind_group = Some((bind_group, layout, offset));
    }

    pub fn reset_text_shader_params(&mut self) {
        self.flush_text();
        self.dirty_pipeline = true;
        self.text_shader_bind_group = None;
    }

    pub fn set_text_shader(&mut self, shader: Shader) {
        self.flush_text();
        self.dirty_pipeline = true;
        self.text_shader = shader;
    }

    pub fn set_sampler(&mut self, sampler: Sampler) {
        self.flush_text();
        self.next_sampler = sampler;
    }

    pub fn set_blend_mode(&mut self, blend_mode: BlendMode) {
        self.flush_text();
        self.dirty_pipeline = true;
        self.blend_mode = blend_mode;
    }

    pub fn set_premultiplied_text(&mut self, premultiplied_text: bool) {
        self.flush_text();
        self.premul_text = premultiplied_text;
    }

    pub fn set_projection(&mut self, proj: impl Into<mint::ColumnMatrix4<f32>>) {
        self.flush_text();
        self.transform = proj.into().into();
        self.text_uniforms = self
            .uniform_arena
            .allocate(&self.wgpu.device, TextUniforms::std140_size_static() as _);
        self.wgpu.queue.write_buffer(
            &self.text_uniforms.buffer,
            self.text_uniforms.offset,
            (TextUniforms {
                transform: self.transform,
            })
            .as_std140()
            .as_bytes(),
        );
    }

    pub fn set_scissor_rect(&mut self, (x, y, w, h): (u32, u32, u32, u32)) {
        self.flush_text();
        self.pass.set_scissor_rect(x, y, w, h);
    }

    pub fn draw_mesh(&mut self, mesh: &'a Mesh, image: &Image, param: DrawParam, scale: bool) {
        self.flush_text();

        if mesh.index_count == 0 {
            return;
        }

        // The mesh hot path is deliberately flat: the pipeline selection that
        // used to be update_pipeline, the arena bump from
        // GrowingBufferArena::allocate and the image bookkeeping from set_image
        // all happen inline so the whole render-pass state change is visible
        // right here.
        let ty = ShaderType::Draw;

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

            // the uniform binding is seeded by the shader kind so that plain
            // draws, instanced draws and text never share a layout entry
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

            // the empty layout/group pair is only used to reserve slot 2
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

            if let ShaderType::Instance { .. } = ty {
                groups.push(Some(&instance_layout));
            } else {
                // the dummy group ensures the user's bind group is at index 3
                groups.push(Some(&dummy_layout));
                self.pass.set_bind_group(2, &dummy_group, &[]);
            }

            let shader = match ty {
                ShaderType::Draw | ShaderType::Instance { .. } => {
                    if let Some((ref bind_group, ref bind_group_layout, offset)) =
                        self.shader_bind_group
                    {
                        self.pass.set_bind_group(3, bind_group, &[offset]);
                        groups.push(Some(bind_group_layout));
                    }

                    &self.shader
                }
                ShaderType::Text => {
                    if let Some((ref bind_group, ref bind_group_layout, offset)) =
                        self.text_shader_bind_group
                    {
                        self.pass.set_bind_group(3, bind_group, &[offset]);
                        groups.push(Some(bind_group_layout));
                    }

                    &self.text_shader
                }
            };

            // pipeline layout, keyed by the hashed bind group layouts
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
                            ShaderType::Draw => self.draw_sm.clone(),
                            ShaderType::Instance { ordered: true } => self.instance_sm.clone(),
                            ShaderType::Instance { ordered: false } => {
                                self.instance_unordered_sm.clone()
                            }
                            ShaderType::Text => self.text_sm.clone(),
                        }
                    },
                    fs: if let Some(fs_module) = &shader.fs_module {
                        fs_module.clone()
                    } else {
                        match ty {
                            ShaderType::Draw | ShaderType::Instance { .. } => self.draw_sm.clone(),
                            ShaderType::Text => self.text_sm.clone(),
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
                    depth: None,
                    vertices: true,
                    topology: match ty {
                        ShaderType::Text => wgpu::PrimitiveTopology::TriangleStrip,
                        _ => wgpu::PrimitiveTopology::TriangleList,
                    },
                    vertex_layout: match ty {
                        ShaderType::Text => TextVertex::layout(),
                        _ => Vertex::layout(),
                    },
                    cull_mode: None,
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

        let alloc_size = DrawUniforms::std140_size_static() as u64;

        // arena bump, spelled out: align the request, walk the buffers for one
        // with room left, and mint a fresh buffer when the arena is full
        // (the old helper grew the arena and recursed; this is the same
        // behavior with one less jump).
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

        // the uniform bind group for this draw, straight from its cache key
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

        // set_image body inlined, including the sampler cache and the
        // image-local bind group cache it used to consult
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

            // fast path first: the image caches uploaded bind groups by sampler
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

        let mut uniforms = DrawUniforms::from_param(
            &param,
            if scale {
                Some(glam::Vec2::new(image.width() as f32, image.height() as f32).into())
            } else {
                None
            },
        );
        uniforms.transform = self.transform * uniforms.transform;

        // 1. allocate some uniform buffer memory from GrowingBufferArena.
        // 2. write the uniform data to that memory
        // 3. use a "dynamic offset" to offset into the memory

        self.wgpu.queue.write_buffer(
            &uniform_alloc.buffer,
            uniform_alloc.offset,
            uniforms.as_std140().as_bytes(),
        );

        self.pass.set_bind_group(
            0,
            &uniform_bind_group,
            &[uniform_alloc.offset as u32], // <- the dynamic offset
        );

        self.pass.set_vertex_buffer(0, mesh.verts.slice(..));
        self.pass
            .set_index_buffer(mesh.inds.slice(..), wgpu::IndexFormat::Uint32);

        self.pass.draw_indexed(0..mesh.index_count as _, 0, 0..1);
    }

    pub fn draw_mesh_instances(
        &mut self,
        mesh: &'a Mesh,
        instances: &'a InstanceArrayView,
        param: DrawParam,
        scale: bool,
    ) -> GameResult {
        self.flush_text();

        if instances.len == 0 || mesh.index_count == 0 {
            return Ok(());
        }

        // instanced draws carry their pipeline-creation path inline as well.
        // Keeping the two mesh entry points independent of update_pipeline
        // and set_image means everything needed for one draw is textually
        // local to the caller.
        let ty = ShaderType::Instance {
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

            // uniform layout, distinct per shader kind via the seed
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

            if let ShaderType::Instance { .. } = ty {
                groups.push(Some(&instance_layout));
            } else {
                // the dummy group ensures the user's bind group is at index 3
                groups.push(Some(&dummy_layout));
                self.pass.set_bind_group(2, &dummy_group, &[]);
            }

            let shader = match ty {
                ShaderType::Draw | ShaderType::Instance { .. } => {
                    if let Some((ref bind_group, ref bind_group_layout, offset)) =
                        self.shader_bind_group
                    {
                        self.pass.set_bind_group(3, bind_group, &[offset]);
                        groups.push(Some(bind_group_layout));
                    }

                    &self.shader
                }
                ShaderType::Text => {
                    if let Some((ref bind_group, ref bind_group_layout, offset)) =
                        self.text_shader_bind_group
                    {
                        self.pass.set_bind_group(3, bind_group, &[offset]);
                        groups.push(Some(bind_group_layout));
                    }

                    &self.text_shader
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
                            ShaderType::Draw => self.draw_sm.clone(),
                            ShaderType::Instance { ordered: true } => self.instance_sm.clone(),
                            ShaderType::Instance { ordered: false } => {
                                self.instance_unordered_sm.clone()
                            }
                            ShaderType::Text => self.text_sm.clone(),
                        }
                    },
                    fs: if let Some(fs_module) = &shader.fs_module {
                        fs_module.clone()
                    } else {
                        match ty {
                            ShaderType::Draw | ShaderType::Instance { .. } => self.draw_sm.clone(),
                            ShaderType::Text => self.text_sm.clone(),
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
                    depth: None,
                    vertices: true,
                    topology: match ty {
                        ShaderType::Text => wgpu::PrimitiveTopology::TriangleStrip,
                        _ => wgpu::PrimitiveTopology::TriangleList,
                    },
                    vertex_layout: match ty {
                        ShaderType::Text => TextVertex::layout(),
                        _ => Vertex::layout(),
                    },
                    cull_mode: None,
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

        // the arena bump, inline: align to GPU uniform-buffer offset
        // requirements, bump the first cursor with room, or append a brand
        // new buffer (which is exactly what a grow + retry would have picked)
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

        // uniform bind group through its cache entry
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

        // set_image body inlined for the instance image
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

        let uniforms = InstanceUniforms {
            // image scaling is non-sensical for instance array itself as the image scaling is applied locally (see below)
            transform: self.transform * DrawUniforms::from_param(&param, None).transform,
            color: Vec4::from_array(param.color.into()),
            // this is the actual image scale that we apply in the vertex shader.
            // we can't apply this when we first convert the instance array drawparams because we don't know the image size at the time the user inserts the drawparams.
            // we also can't apply image scaling in the global instance transform as it *must* be applied in local space.
            scale: if scale {
                glam::Vec2::new(
                    instances.image.width() as f32,
                    instances.image.height() as f32,
                )
            } else {
                glam::Vec2::ZERO
            },
        };

        self.wgpu.queue.write_buffer(
            &uniform_alloc.buffer,
            uniform_alloc.offset,
            uniforms.as_std140().as_bytes(),
        );

        self.pass
            .set_bind_group(0, &uniform_bind_group, &[uniform_alloc.offset as u32]);
        self.pass.set_bind_group(2, &instances.bind_group, &[]);

        self.pass.set_vertex_buffer(0, mesh.verts.slice(..));
        self.pass
            .set_index_buffer(mesh.inds.slice(..), wgpu::IndexFormat::Uint32);

        self.pass
            .draw_indexed(0..mesh.index_count as _, 0, 0..instances.len as _);

        Ok(())
    }

    pub fn draw_bounded_text(&mut self, text: &Text, mut param: DrawParam) -> GameResult {
        if let Transform::Values { dest, offset, .. } = &mut param.transform {
            if offset.x > 0. || offset.y > 0. {
                // inlined Text::measure_raw: lay the section out the way a
                // no-frills measure would and read back its overall bounds.
                // The section assembly that as_section used to do is spelled
                // out so the offset fix below works on the same terms.
                let bounds = {
                    let defaults = DrawParam::default();
                    let section = glyph_brush::Section {
                        screen_position: (0., 0.),

                        bounds: (text.bounds.x, text.bounds.y),
                        layout: if text.wrap {
                            glyph_brush::Layout::default_wrap()
                        } else {
                            glyph_brush::Layout::default_single_line()
                        }
                        .h_align(text.layout.h_align.into())
                        .v_align(text.layout.v_align.into()),

                        text: text
                            .fragments
                            .iter()
                            .map(|fragment| {
                                let font = fragment.font.as_ref().unwrap_or(&text.font);
                                Ok(glyph_brush::Text {
                                    text: &fragment.text,
                                    scale: fragment.scale.unwrap_or(text.scale),
                                    font_id: *self
                                        .fonts
                                        .get(font)
                                        .ok_or_else(|| GameError::FontSelectError(font.clone()))?,
                                    extra: Extra {
                                        color: fragment.color.unwrap_or(defaults.color).into(),
                                        transform: defaults.transform.to_bare_matrix_glam(),
                                    },
                                })
                            })
                            .collect::<GameResult<Vec<_>>>()?,
                    };
                    self.text_renderer
                        .glyph_brush
                        .borrow_mut()
                        .glyph_bounds(section)
                        .map(|rect| mint::Vector2::<f32> {
                            x: rect.width(),
                            y: rect.height(),
                        })
                        .unwrap_or_else(|| mint::Vector2::<f32> { x: 0., y: 0. })
                };
                dest.x -= offset.x * bounds.x;
                dest.y -= offset.y * bounds.y;
                *offset = mint::Point2 { x: 0., y: 0. };
            }
        }

        // queue the section for the next flush; the section shape is again
        // written inline rather than borrowed from as_section, this time with
        // the real draw params
        let section = glyph_brush::Section {
            screen_position: (0., 0.),

            bounds: (text.bounds.x, text.bounds.y),
            layout: if text.wrap {
                glyph_brush::Layout::default_wrap()
            } else {
                glyph_brush::Layout::default_single_line()
            }
            .h_align(text.layout.h_align.into())
            .v_align(text.layout.v_align.into()),

            text: text
                .fragments
                .iter()
                .map(|fragment| {
                    let font = fragment.font.as_ref().unwrap_or(&text.font);
                    Ok(glyph_brush::Text {
                        text: &fragment.text,
                        scale: fragment.scale.unwrap_or(text.scale),
                        font_id: *self
                            .fonts
                            .get(font)
                            .ok_or_else(|| GameError::FontSelectError(font.clone()))?,
                        extra: Extra {
                            color: fragment.color.unwrap_or(param.color).into(),
                            transform: param.transform.to_bare_matrix_glam(),
                        },
                    })
                })
                .collect::<GameResult<Vec<_>>>()?,
        };
        self.text_renderer.glyph_brush.borrow_mut().queue(section);

        // set_text_image inlined: bind the glyph atlas view at slot 1, keyed
        // by sampler in the shared caches
        let view = self.text_renderer.cache_view.clone();
        if self.curr_sampler != self.next_sampler
            || self.curr_image.as_ref().is_none_or(|curr| *curr != view)
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

            let text_image_layout = self
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
            let image_bind = self
                .bind_group_cache
                .groups
                .entry(vec![
                    BindGroupEntryKey::Image(view.clone()),
                    BindGroupEntryKey::Sampler(sample.clone()),
                ])
                .or_insert_with(|| {
                    self.wgpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
                        label: None,
                        layout: &text_image_layout,
                        entries: &[
                            wgpu::BindGroupEntry {
                                binding: 0,
                                resource: wgpu::BindingResource::TextureView(&view),
                            },
                            wgpu::BindGroupEntry {
                                binding: 1,
                                resource: wgpu::BindingResource::Sampler(&sample),
                            },
                        ],
                    })
                })
                .clone();

            self.curr_image = Some(view);

            self.pass.set_bind_group(1, &image_bind, &[]);
        }

        // the text uniforms bind group, constructed through its cache entry
        // now that BindGroupBuilder is no longer used here
        let alloc_size = TextUniforms::std140_size_static() as u64;
        let text_uniforms_layout = self
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
        let text_uniforms_bind = self
            .bind_group_cache
            .groups
            .entry(vec![BindGroupEntryKey::Buffer {
                buffer: self.text_uniforms.buffer.clone(),
                offset: 0,
                size: Some(alloc_size),
            }])
            .or_insert_with(|| {
                self.wgpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: None,
                    layout: &text_uniforms_layout,
                    entries: &[wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                            buffer: &self.text_uniforms.buffer,
                            offset: 0,
                            size: NonZeroU64::new(alloc_size),
                        }),
                    }],
                })
            })
            .clone();

        self.pass
            .set_bind_group(0, &text_uniforms_bind, &[self.text_uniforms.offset as u32]);

        // bring the pass into text shape right away instead of waiting for the
        // flush: every flush rebuilds the pipeline when anything about it has
        // changed in the meantime, so setting it up at queue time is safe and
        // keeps the whole text path inside this method
        let ty = ShaderType::Text;
        if self.dirty_pipeline || self.shader_ty != Some(ty) {
            self.dirty_pipeline = false;
            self.shader_ty = Some(ty);

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

            if let ShaderType::Instance { .. } = ty {
                groups.push(Some(&instance_layout));
            } else {
                // the dummy group ensures the user's bind group is at index 3
                groups.push(Some(&dummy_layout));
                self.pass.set_bind_group(2, &dummy_group, &[]);
            }

            let shader = match ty {
                ShaderType::Draw | ShaderType::Instance { .. } => {
                    if let Some((ref bind_group, ref bind_group_layout, offset)) =
                        self.shader_bind_group
                    {
                        self.pass.set_bind_group(3, bind_group, &[offset]);
                        groups.push(Some(bind_group_layout));
                    }

                    &self.shader
                }
                ShaderType::Text => {
                    if let Some((ref bind_group, ref bind_group_layout, offset)) =
                        self.text_shader_bind_group
                    {
                        self.pass.set_bind_group(3, bind_group, &[offset]);
                        groups.push(Some(bind_group_layout));
                    }

                    &self.text_shader
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
                            ShaderType::Draw => self.draw_sm.clone(),
                            ShaderType::Instance { ordered: true } => self.instance_sm.clone(),
                            ShaderType::Instance { ordered: false } => {
                                self.instance_unordered_sm.clone()
                            }
                            ShaderType::Text => self.text_sm.clone(),
                        }
                    },
                    fs: if let Some(fs_module) = &shader.fs_module {
                        fs_module.clone()
                    } else {
                        match ty {
                            ShaderType::Draw | ShaderType::Instance { .. } => self.draw_sm.clone(),
                            ShaderType::Text => self.text_sm.clone(),
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
                    depth: None,
                    vertices: true,
                    topology: match ty {
                        ShaderType::Text => wgpu::PrimitiveTopology::TriangleStrip,
                        _ => wgpu::PrimitiveTopology::TriangleList,
                    },
                    vertex_layout: match ty {
                        ShaderType::Text => TextVertex::layout(),
                        _ => Vertex::layout(),
                    },
                    cull_mode: None,
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

        self.queuing_text = true;

        Ok(())
    }

    fn flush_text(&mut self) {
        if self.queuing_text {
            self.queuing_text = false;
            let mut premul = false;
            if self.premul_text && self.blend_mode == BlendMode::ALPHA {
                premul = true;
                self.set_blend_mode(BlendMode::PREMULTIPLIED);
            }
            self.update_pipeline(ShaderType::Text);
            self.text_renderer
                .draw_queued(&self.wgpu.device, &self.wgpu.queue, &mut self.pass);
            if premul {
                self.set_blend_mode(BlendMode::ALPHA);
            }
        }
    }

    pub fn finish(mut self) {
        self.finalize();
    }

    fn finalize(&mut self) {
        self.flush_text();
    }

    fn update_pipeline(&mut self, ty: ShaderType) {
        if self.dirty_pipeline || self.shader_ty != Some(ty) {
            self.dirty_pipeline = false;
            self.shader_ty = Some(ty);

            let texture_layout = BindGroupLayoutBuilder::new()
                .image(wgpu::ShaderStages::FRAGMENT)
                .sampler(wgpu::ShaderStages::FRAGMENT)
                .create(&self.wgpu.device, self.bind_group_cache);

            let instance_layout = BindGroupLayoutBuilder::new()
                .buffer(
                    wgpu::ShaderStages::VERTEX,
                    wgpu::BufferBindingType::Storage { read_only: true },
                    false,
                )
                .buffer(
                    wgpu::ShaderStages::VERTEX,
                    wgpu::BufferBindingType::Storage { read_only: true },
                    false,
                )
                .create(&self.wgpu.device, self.bind_group_cache);

            let uniform_layout = BindGroupLayoutBuilder::new()
                .seed(ty)
                .buffer(
                    wgpu::ShaderStages::VERTEX,
                    wgpu::BufferBindingType::Uniform,
                    true,
                )
                .create(&self.wgpu.device, self.bind_group_cache);

            let (dummy_group, dummy_layout) =
                BindGroupBuilder::new().create(&self.wgpu.device, self.bind_group_cache);

            let mut groups = vec![Some(&uniform_layout), Some(&texture_layout)];

            if let ShaderType::Instance { .. } = ty {
                groups.push(Some(&instance_layout));
            } else {
                // the dummy group ensures the user's bind group is at index 3
                groups.push(Some(&dummy_layout));
                self.pass.set_bind_group(2, &dummy_group, &[]);
            }

            let shader = match ty {
                ShaderType::Draw | ShaderType::Instance { .. } => {
                    if let Some((ref bind_group, ref bind_group_layout, offset)) =
                        self.shader_bind_group
                    {
                        self.pass.set_bind_group(3, bind_group, &[offset]);
                        groups.push(Some(bind_group_layout));
                    }

                    &self.shader
                }
                ShaderType::Text => {
                    if let Some((ref bind_group, ref bind_group_layout, offset)) =
                        self.text_shader_bind_group
                    {
                        self.pass.set_bind_group(3, bind_group, &[offset]);
                        groups.push(Some(bind_group_layout));
                    }

                    &self.text_shader
                }
            };

            let layout = self.pipeline_cache.layout(&self.wgpu.device, &groups);
            let pipeline = self.pipeline_cache.render_pipeline(
                &self.wgpu.device,
                RenderPipelineInfo {
                    layout,
                    vs: if let Some(vs_module) = &shader.vs_module {
                        vs_module.clone()
                    } else {
                        match ty {
                            ShaderType::Draw => self.draw_sm.clone(),
                            ShaderType::Instance { ordered: true } => self.instance_sm.clone(),
                            ShaderType::Instance { ordered: false } => {
                                self.instance_unordered_sm.clone()
                            }
                            ShaderType::Text => self.text_sm.clone(),
                        }
                    },
                    fs: if let Some(fs_module) = &shader.fs_module {
                        fs_module.clone()
                    } else {
                        match ty {
                            ShaderType::Draw | ShaderType::Instance { .. } => self.draw_sm.clone(),
                            ShaderType::Text => self.text_sm.clone(),
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
                    depth: None,
                    vertices: true,
                    topology: match ty {
                        ShaderType::Text => wgpu::PrimitiveTopology::TriangleStrip,
                        _ => wgpu::PrimitiveTopology::TriangleList,
                    },
                    vertex_layout: match ty {
                        ShaderType::Text => TextVertex::layout(),
                        _ => Vertex::layout(),
                    },
                    cull_mode: None,
                },
            );

            self.pass.set_pipeline(&pipeline);
        }
    }
}

impl Drop for InternalCanvas<'_> {
    fn drop(&mut self) {
        self.finalize();
    }
}

#[derive(Debug)]
pub struct InstanceArrayView {
    pub bind_group: wgpu::BindGroup,
    pub image: Image,
    pub len: u32,
    pub ordered: bool,
}

impl InstanceArrayView {
    pub fn from_instances(ia: &InstanceArray) -> GameResult<Self> {
        Ok(InstanceArrayView {
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
enum ShaderType {
    Draw,
    Instance { ordered: bool },
    Text,
}

#[derive(crevice::std140::AsStd140)]
struct InstanceUniforms {
    pub transform: Mat4,
    pub color: Vec4,
    pub scale: Vec2,
}

#[derive(crevice::std140::AsStd140)]
struct TextUniforms {
    transform: Mat4,
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
