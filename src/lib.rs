use crevice::std140::AsStd140;
use glam::*;
use itertools::Itertools as _;

pub type Color = rgb::RGBA8;

/// Represents a rectangle.
#[derive(Debug, Clone, Copy)]
pub struct Rect {
    /// Offset of the rectangle.
    pub offset: IVec2,
    /// Size of the rectangle.
    pub size: UVec2,
}

impl Rect {
    /// Creates a new rectangle.
    pub fn new(x: i32, y: i32, width: u32, height: u32) -> Self {
        Self {
            offset: IVec2::new(x, y),
            size: UVec2::new(width, height),
        }
    }

    /// Gets the x coordinate of top-left corner.
    pub const fn left(&self) -> i32 {
        self.offset.x
    }

    /// Gets the y coordinate of top-left corner.
    pub const fn top(&self) -> i32 {
        self.offset.y
    }

    /// Gets the x coordinate of bottom-right corner.
    pub const fn right(&self) -> i32 {
        self.offset.x + self.size.x as i32
    }

    /// Gets the y coordinate of bottom-right corner.
    pub const fn bottom(&self) -> i32 {
        self.offset.y + self.size.y as i32
    }
}

/// Represents a sprite to draw.
#[derive(Debug, Clone)]
pub struct Sprite<'a> {
    /// The texture to draw from.
    pub texture: &'a wgpu::Texture,

    /// Source rectangle from the texture to draw from.
    pub src: Rect,

    /// Transformation of the source rectangle into screen space.
    pub transform: Affine2,

    /// Tint.
    pub tint: Color,
}

/// Encapsulates static state for rendering.
pub struct Renderer {
    render_pipeline: wgpu::RenderPipeline,
    texture_bind_group_layout: wgpu::BindGroupLayout,
    target_uniforms_buffer: wgpu::Buffer,
    target_uniforms_bind_group: wgpu::BindGroup,
    texture_uniforms_buffer: wgpu::Buffer,
    prepared_groups: Vec<PreparedGroup>,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    sampler: wgpu::Sampler,
}

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct Vertex {
    position: [f32; 3],
    tex_coords: [f32; 2],
    tint: [f32; 4],
}

#[repr(C)]
#[derive(Copy, Clone, Debug, AsStd140)]
struct TextureUniforms {
    size: Vec3,
    is_mask: u32,
}

#[repr(C)]
#[derive(Copy, Clone, Debug, AsStd140)]
struct TargetUniforms {
    size: Vec3,
}

impl Vertex {
    const BUFFER_LAYOUT: wgpu::VertexBufferLayout<'static> = wgpu::VertexBufferLayout {
        array_stride: std::mem::size_of::<Self>() as wgpu::BufferAddress,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x2, 2 => Float32x4],
    };
}

fn ensure_buffer_size(
    buffer: &mut wgpu::Buffer,
    label: Option<&str>,
    device: &wgpu::Device,
    size: u64,
) {
    if buffer.size() >= size {
        return;
    }
    *buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label,
        size,
        usage: buffer.usage(),
        mapped_at_creation: false,
    })
}

struct PreparedGroup {
    texture_bind_group: wgpu::BindGroup,
    index_buffer_start: u32,
    index_buffer_end: u32,
}

impl Renderer {
    /// Creates a new renderer.
    pub fn new(device: &wgpu::Device, texture_format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::include_wgsl!("shader.wgsl"));
        let texture_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("spright: texture_bind_group_layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            multisampled: false,
                            view_dimension: wgpu::TextureViewDimension::D2,
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
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

        let target_uniforms_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("spright: target_uniforms_bind_group_layout"),
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

        let texture_uniforms_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("spright: texture_uniforms_buffer"),
            size: std::mem::size_of::<Std140TextureUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let target_uniforms_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("spright: target_uniforms_buffer"),
            size: std::mem::size_of::<Std140TargetUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let target_uniforms_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("spright: target_uniforms_bind_group"),
            layout: &target_uniforms_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: target_uniforms_buffer.as_entire_binding(),
            }],
        });

        let vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("spright: vertex_buffer"),
            size: std::mem::size_of::<Vertex>() as u64 * 1024,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let index_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("spright: vertex_buffer"),
            size: std::mem::size_of::<u32>() as u64 * 1024,
            usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Self {
            render_pipeline: device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("spright: render_pipeline"),
                cache: None,
                layout: Some(
                    &device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                        label: Some("spright: render_pipeline.layout"),
                        bind_group_layouts: &[
                            &texture_bind_group_layout,
                            &target_uniforms_bind_group_layout,
                        ],
                        push_constant_ranges: &[],
                    }),
                ),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: "vs_main",
                    buffers: &[Vertex::BUFFER_LAYOUT],
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: "fs_main",
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: texture_format,
                        blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                        write_mask: wgpu::ColorWrites::all(),
                    })],
                }),
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                multiview: None,
            }),
            texture_bind_group_layout,
            target_uniforms_buffer,
            target_uniforms_bind_group,
            texture_uniforms_buffer,
            vertex_buffer,
            index_buffer,
            prepared_groups: vec![],
            sampler: device.create_sampler(&wgpu::SamplerDescriptor {
                address_mode_u: wgpu::AddressMode::ClampToEdge,
                address_mode_v: wgpu::AddressMode::ClampToEdge,
                address_mode_w: wgpu::AddressMode::ClampToEdge,
                mag_filter: wgpu::FilterMode::Nearest,
                min_filter: wgpu::FilterMode::Nearest,
                mipmap_filter: wgpu::FilterMode::Nearest,
                ..Default::default()
            }),
        }
    }

    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        target_size: wgpu::Extent3d,
        sprites: &[Sprite<'_>],
    ) {
        queue.write_buffer(
            &self.target_uniforms_buffer,
            0,
            TargetUniforms {
                size: Vec3 {
                    x: target_size.width as f32,
                    y: target_size.height as f32,
                    z: 0.0,
                },
            }
            .as_std140()
            .as_bytes(),
        );

        self.prepared_groups.clear();

        let mut texture_uniforms = vec![];
        let min_uniform_buffer_offset_alignment =
            device.limits().min_uniform_buffer_offset_alignment;

        let grouped = sprites
            .iter()
            .chunk_by(|s| s.texture.global_id())
            .into_iter()
            .map(|(_, chunk)| chunk.collect::<Vec<_>>())
            .collect::<Vec<_>>();

        for sprites in grouped.iter() {
            let texture = sprites.first().unwrap().texture;

            texture_uniforms.extend(
                TextureUniforms {
                    size: Vec3 {
                        x: texture.width() as f32,
                        y: texture.height() as f32,
                        z: 0.0,
                    },
                    is_mask: if texture.format() == wgpu::TextureFormat::R8Unorm {
                        1
                    } else {
                        0
                    },
                }
                .as_std140()
                .as_bytes()
                .into_iter()
                .cloned()
                .chain(std::iter::repeat(0))
                .take(min_uniform_buffer_offset_alignment as usize),
            );
        }
        ensure_buffer_size(
            &mut self.texture_uniforms_buffer,
            Some("spright: texture_uniforms_buffer"),
            device,
            bytemuck::cast_slice::<_, u8>(&texture_uniforms[..]).len() as u64,
        );
        queue.write_buffer(
            &mut self.texture_uniforms_buffer,
            0,
            bytemuck::cast_slice::<_, u8>(&texture_uniforms[..]),
        );

        let mut vertices = vec![];
        let mut indices = vec![];

        for (i, sprites) in grouped.into_iter().enumerate() {
            let texture = sprites.first().unwrap().texture;

            let index_buffer_start = indices.len() as u32;

            for s in sprites {
                let offset = vertices.len() as u32;

                let tint = [
                    s.tint.r as f32 / 255.0,
                    s.tint.g as f32 / 255.0,
                    s.tint.b as f32 / 255.0,
                    s.tint.a as f32 / 255.0,
                ];

                vertices.extend([
                    Vertex {
                        position: s
                            .transform
                            .transform_point2(Vec2::new(0.0, 0.0))
                            .extend(0.0)
                            .to_array(),
                        tex_coords: [s.src.left() as f32, s.src.top() as f32],
                        tint,
                    },
                    Vertex {
                        position: s
                            .transform
                            .transform_point2(Vec2::new(0.0, s.src.size.y as f32))
                            .extend(0.0)
                            .to_array(),
                        tex_coords: [s.src.left() as f32, s.src.bottom() as f32],
                        tint,
                    },
                    Vertex {
                        position: s
                            .transform
                            .transform_point2(Vec2::new(s.src.size.x as f32, 0.0))
                            .extend(0.0)
                            .to_array(),
                        tex_coords: [s.src.right() as f32, s.src.top() as f32],
                        tint,
                    },
                    Vertex {
                        position: s
                            .transform
                            .transform_point2(Vec2::new(s.src.size.x as f32, s.src.size.y as f32))
                            .extend(0.0)
                            .to_array(),
                        tex_coords: [s.src.right() as f32, s.src.bottom() as f32],
                        tint,
                    },
                ]);

                indices.extend(
                    [
                        0, 1, 2, //
                        1, 2, 3,
                    ]
                    .map(|v| v + offset),
                );
            }

            self.prepared_groups.push(PreparedGroup {
                texture_bind_group: device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("spright: texture_bind_group"),
                    layout: &self.texture_bind_group_layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(
                                &texture.create_view(&wgpu::TextureViewDescriptor::default()),
                            ),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::Sampler(&self.sampler),
                        },
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                                buffer: &self.texture_uniforms_buffer,
                                offset: (i * min_uniform_buffer_offset_alignment as usize) as u64,
                                size: Some(
                                    std::num::NonZero::new(
                                        min_uniform_buffer_offset_alignment as u64,
                                    )
                                    .unwrap(),
                                ),
                            }),
                        },
                    ],
                }),
                index_buffer_start,
                index_buffer_end: indices.len() as u32,
            });
        }

        ensure_buffer_size(
            &mut self.vertex_buffer,
            Some("spright: vertex_buffer"),
            device,
            bytemuck::cast_slice::<_, u8>(&vertices[..]).len() as u64,
        );
        queue.write_buffer(
            &mut self.vertex_buffer,
            0,
            bytemuck::cast_slice(&vertices[..]),
        );

        ensure_buffer_size(
            &mut self.index_buffer,
            Some("spright: index_buffer"),
            device,
            bytemuck::cast_slice::<_, u8>(&indices[..]).len() as u64,
        );
        queue.write_buffer(
            &mut self.index_buffer,
            0,
            bytemuck::cast_slice(&indices[..]),
        );
    }

    /// Renders prepared sprites.
    pub fn render<'rpass>(&'rpass self, rpass: &mut wgpu::RenderPass<'rpass>) {
        rpass.set_pipeline(&self.render_pipeline);
        rpass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        rpass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
        rpass.set_bind_group(1, &self.target_uniforms_bind_group, &[]);
        for prepared_group in self.prepared_groups.iter() {
            rpass.set_bind_group(0, &prepared_group.texture_bind_group, &[]);
            rpass.draw_indexed(
                prepared_group.index_buffer_start..prepared_group.index_buffer_end,
                0,
                0..1,
            );
        }
    }
}
