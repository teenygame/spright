pub mod texture;

use wgpu::util::DeviceExt as _;

/// Represents a rectangle.
#[derive(Debug, Clone)]
pub struct Rect {
    /// x coordinate of top-left corner.
    pub x: f32,
    /// y coordinate of top-left corner.
    pub y: f32,
    /// Width.
    pub width: f32,
    /// Height.
    pub height: f32,
}

impl Rect {
    /// Gets the x coordinate of top-left corner.
    pub const fn left(&self) -> f32 {
        self.x
    }

    /// Gets the y coordinate of top-left corner.
    pub const fn top(&self) -> f32 {
        self.y
    }

    /// Gets the x coordinate of bottom-right corner.
    pub const fn right(&self) -> f32 {
        self.x + self.width
    }

    /// Gets the y coordinate of bottom-right corner.
    pub const fn bottom(&self) -> f32 {
        self.y + self.height
    }
}

/// Represents a group of sprites from the same texture.
pub struct Group<'a> {
    /// The texture to draw from.
    pub texture: &'a wgpu::Texture,

    /// The sprites to draw.
    pub sprites: &'a [Sprite],
}

/// Represents a chunk of the texture to draw.
pub struct Sprite {
    /// Source rectangle from the texture to draw from.
    pub src: Rect,

    /// Destination rectangle to draw to.
    pub dest: Rect,
}

struct PreparedGroup {
    texture_bind_group: wgpu::BindGroup,
    texture_size_uniform_bind_group: wgpu::BindGroup,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    num_indices: u32,
}

/// Encapsulates static state for rendering.
pub struct Renderer {
    render_pipeline: wgpu::RenderPipeline,
    texture_bind_group_layout: wgpu::BindGroupLayout,
    texture_size_uniform_bind_group_layout: wgpu::BindGroupLayout,
    screen_size_uniform_bind_group: wgpu::BindGroup,
    screen_size_uniform_buffer: wgpu::Buffer,
    sampler: wgpu::Sampler,
    prepared: Vec<PreparedGroup>,
}

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct Vertex {
    position: [f32; 3],
    tex_coords: [f32; 2],
}

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct TextureSizeUniform([f32; 2]);

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct ScreenSizeUniform([f32; 2]);

impl Vertex {
    const ATTRIBS: [wgpu::VertexAttribute; 2] =
        wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x2];

    fn desc() -> wgpu::VertexBufferLayout<'static> {
        use std::mem;

        wgpu::VertexBufferLayout {
            array_stride: mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBS,
        }
    }
}

impl Renderer {
    /// Creates a new renderer.
    pub fn new(
        device: &wgpu::Device,
        texture_format: wgpu::TextureFormat,
        screen_size: [f32; 2],
    ) -> Self {
        let screen_size_uniform_buffer =
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("spright: screen_size_uniform_buffer"),
                contents: bytemuck::cast_slice(&[ScreenSizeUniform(screen_size)]),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            });

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
                ],
            });

        let texture_size_uniform_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("spright: texture_size_uniform_bind_group_layout"),
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

        let screen_size_uniform_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("spright: screen_size_uniform_bind_group_layout"),
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

        let screen_size_uniform_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("spright: screen_size_uniform_uniform_bind_group"),
            layout: &screen_size_uniform_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: screen_size_uniform_buffer.as_entire_binding(),
            }],
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
                            &texture_size_uniform_bind_group_layout,
                            &screen_size_uniform_bind_group_layout,
                        ],
                        push_constant_ranges: &[],
                    }),
                ),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: "vs_main",
                    buffers: &[Vertex::desc()],
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
            texture_size_uniform_bind_group_layout,
            screen_size_uniform_bind_group,
            screen_size_uniform_buffer,
            sampler: device.create_sampler(&wgpu::SamplerDescriptor {
                address_mode_u: wgpu::AddressMode::ClampToEdge,
                address_mode_v: wgpu::AddressMode::ClampToEdge,
                address_mode_w: wgpu::AddressMode::ClampToEdge,
                mag_filter: wgpu::FilterMode::Nearest,
                min_filter: wgpu::FilterMode::Nearest,
                mipmap_filter: wgpu::FilterMode::Nearest,
                ..Default::default()
            }),
            prepared: vec![],
        }
    }

    /// Resizes the drawing area.
    pub fn resize(&self, queue: &wgpu::Queue, size: [f32; 2]) {
        queue.write_buffer(
            &self.screen_size_uniform_buffer,
            0,
            bytemuck::cast_slice(&[ScreenSizeUniform(size)]),
        );
    }

    fn prepare_one(
        &self,
        device: &wgpu::Device,
        texture: &wgpu::Texture,
        sprites: &[Sprite],
    ) -> PreparedGroup {
        let texture_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
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
            ],
        });

        let wgpu::Extent3d { width, height, .. } = texture.size();

        let texture_size_uniform_buffer =
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("spright: texture_size_uniform_buffer"),
                contents: bytemuck::cast_slice(&[TextureSizeUniform([
                    width as f32,
                    height as f32,
                ])]),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            });

        let texture_size_uniform_bind_group =
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("spright: texture_size_uniform_bind_group"),
                layout: &self.texture_size_uniform_bind_group_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: texture_size_uniform_buffer.as_entire_binding(),
                }],
            });

        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("spright: vertex_buffer"),
            contents: bytemuck::cast_slice(
                &sprites
                    .iter()
                    .flat_map(|s| {
                        [
                            Vertex {
                                position: [s.dest.left(), s.dest.top(), 0.0],
                                tex_coords: [s.src.left(), s.src.top()],
                            },
                            Vertex {
                                position: [s.dest.left(), s.dest.bottom(), 0.0],
                                tex_coords: [s.src.left(), s.src.bottom()],
                            },
                            Vertex {
                                position: [s.dest.right(), s.dest.top(), 0.0],
                                tex_coords: [s.src.right(), s.src.top()],
                            },
                            Vertex {
                                position: [s.dest.right(), s.dest.bottom(), 0.0],
                                tex_coords: [s.src.right(), s.src.bottom()],
                            },
                        ]
                    })
                    .collect::<Vec<_>>()[..],
            ),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let indices = (0..sprites.len() as u16)
            .flat_map(|i| {
                [
                    0, 1, 2, //
                    1, 2, 3, //
                ]
                .map(|v| v + i * 4)
            })
            .collect::<Vec<_>>();

        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("spright: index_buffer"),
            contents: bytemuck::cast_slice::<u16, _>(&indices[..]),
            usage: wgpu::BufferUsages::INDEX,
        });

        PreparedGroup {
            texture_bind_group,
            texture_size_uniform_bind_group,
            vertex_buffer,
            index_buffer,
            num_indices: indices.len() as u32,
        }
    }

    /// Prepares sprites for rendering.
    pub fn prepare(&mut self, device: &wgpu::Device, groups: &[Group]) {
        self.prepared = groups
            .iter()
            .map(|g| self.prepare_one(device, g.texture, &g.sprites))
            .collect::<Vec<_>>();
    }

    /// Renders prepared sprites.
    pub fn render<'rpass>(&'rpass self, rpass: &mut wgpu::RenderPass<'rpass>) {
        rpass.set_pipeline(&self.render_pipeline);
        for g in self.prepared.iter() {
            rpass.set_vertex_buffer(0, g.vertex_buffer.slice(..));
            rpass.set_index_buffer(g.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
            rpass.set_bind_group(0, &g.texture_bind_group, &[]);
            rpass.set_bind_group(1, &self.screen_size_uniform_bind_group, &[]);
            rpass.set_bind_group(2, &g.texture_size_uniform_bind_group, &[]);
            rpass.draw_indexed(0..g.num_indices, 0, 0..1);
        }
    }
}
