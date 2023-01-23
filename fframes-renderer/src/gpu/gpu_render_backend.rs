use std::{num::NonZeroU32, sync::Arc};

use crate::{
    concatenator::fill_audio_stream,
    encoder::{Encoder, EncoderFrame, EncoderOptions},
    fframes_logger::FFramesLogger,
    render_backend::FFramesRenderBackend,
    renderer_error::FFramesResult,
};
use fframes::{video::Video, BreaksLruCache, ResolvedAudioMap};
use futures::executor::block_on;
use wgpu::{include_wgsl, util::DeviceExt};

use super::tesselator::{tesselate_svg, GpuGlobals, GpuPrimitive, GpuTransform, GpuVertex};

#[derive(Default)]
pub struct GpuRenderingBackend {
    pub text_cache_capacity: usize,
}

impl FFramesRenderBackend for GpuRenderingBackend {
    fn render<'a, TVideo: Video + Sync + Sized>(
        &self,
        output: &'a str,
        video: TVideo,
        logger: Arc<dyn FFramesLogger>,
        usvg_options: &usvgr::Options,
        duration_in_frames: usize,
        render_options: EncoderOptions<'a>,
        _fontdb: &usvgr_text_layout::fontdb::Database,
        ctx: fframes::FFramesContext,
    ) -> FFramesResult<()> {
        let instance = wgpu::Instance::new(wgpu::Backends::PRIMARY);

        // create an adapter
        let adapter = block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: None,
            force_fallback_adapter: false,
        }))
        .unwrap();

        let (device, queue) = block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: None,
                features: wgpu::Features::default(),
                limits: wgpu::Limits::default(),
            },
            // trace_path can be used for API call tracing
            None,
        ))
        .unwrap();

        let vs_module = device.create_shader_module(&include_wgsl!("../shaders/geometry.vs.wgsl"));
        let fs_module = device.create_shader_module(&include_wgsl!("../shaders/geometry.fs.wgsl"));

        // texture must be square (and probably multiplier of 256 but for some reason it works without this restriction)
        let texture_size = usize::max(TVideo::WIDTH, TVideo::HEIGHT) as u32;

        let texture_desc = wgpu::TextureDescriptor {
            label: Some("Multisampled frame descriptor"),
            size: wgpu::Extent3d {
                width: texture_size,
                height: texture_size,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 4,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::COPY_SRC | wgpu::TextureUsages::RENDER_ATTACHMENT,
        };

        let texture = device.create_texture(&texture_desc);
        let texture_view = texture.create_view(&Default::default());
        let msaa_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Multisampled frame descriptor"),
            size: wgpu::Extent3d {
                width: texture_size,
                height: texture_size,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::COPY_SRC | wgpu::TextureUsages::RENDER_ATTACHMENT,
        });

        let msaa_texture_view = msaa_texture.create_view(&Default::default());
        let text_cache = BreaksLruCache::new(self.text_cache_capacity);

        unsafe {
            Encoder::with_output(
                TVideo::WIDTH as i32,
                TVideo::HEIGHT as i32,
                TVideo::FPS as i32,
                output,
                render_options.preferred_codec,
                &logger,
                true,
                &mut |video_encoder| -> FFramesResult<()> {
                    let mut frame = EncoderFrame::make(&video_encoder.video_stream);

                    for fr in 0..duration_in_frames {
                        let svg = video
                            .render_frame(
                                fframes::Frame {
                                    fps: TVideo::FPS,
                                    index: fr,
                                    global_index: fr,
                                    breaks_lru_cache: text_cache.clone(),
                                },
                                &ctx,
                            )
                            .into_string();

                        let rtree = usvgr::Tree::from_str(&svg, usvg_options).unwrap();
                        let (mesh, transforms, primitives) = tesselate_svg(rtree);

                        let prim_buffer_byte_size =
                            (primitives.len() * std::mem::size_of::<GpuPrimitive>()) as u64;
                        let transform_buffer_byte_size =
                            (transforms.len() * std::mem::size_of::<GpuTransform>()) as u64;
                        let globals_buffer_byte_size = std::mem::size_of::<GpuGlobals>() as u64;

                        let vbo = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                            label: None,
                            contents: bytemuck::cast_slice(&mesh.vertices),
                            usage: wgpu::BufferUsages::VERTEX,
                        });

                        let ibo = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                            label: None,
                            contents: bytemuck::cast_slice(&mesh.indices),
                            usage: wgpu::BufferUsages::INDEX,
                        });

                        let prims_ssbo = device.create_buffer(&wgpu::BufferDescriptor {
                            label: Some("Prims ssbo"),
                            size: prim_buffer_byte_size,
                            usage: wgpu::BufferUsages::VERTEX
                                | wgpu::BufferUsages::STORAGE
                                | wgpu::BufferUsages::COPY_DST,
                            mapped_at_creation: false,
                        });

                        let transforms_ssbo = device.create_buffer(&wgpu::BufferDescriptor {
                            label: Some("Transforms ssbo"),
                            size: transform_buffer_byte_size,
                            usage: wgpu::BufferUsages::VERTEX
                                | wgpu::BufferUsages::STORAGE
                                | wgpu::BufferUsages::COPY_DST,
                            mapped_at_creation: false,
                        });

                        let globals_ubo = device.create_buffer(&wgpu::BufferDescriptor {
                            label: Some("Globals ubo"),
                            size: globals_buffer_byte_size,
                            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                            mapped_at_creation: false,
                        });

                        let bind_group_layout =
                            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                                label: Some("Bind group layout"),
                                entries: &[
                                    wgpu::BindGroupLayoutEntry {
                                        binding: 0,
                                        visibility: wgpu::ShaderStages::VERTEX,
                                        ty: wgpu::BindingType::Buffer {
                                            ty: wgpu::BufferBindingType::Uniform,
                                            has_dynamic_offset: false,
                                            min_binding_size: wgpu::BufferSize::new(
                                                globals_buffer_byte_size,
                                            ),
                                        },
                                        count: None,
                                    },
                                    wgpu::BindGroupLayoutEntry {
                                        binding: 1,
                                        visibility: wgpu::ShaderStages::VERTEX,
                                        ty: wgpu::BindingType::Buffer {
                                            ty: wgpu::BufferBindingType::Storage {
                                                read_only: true,
                                            },
                                            has_dynamic_offset: false,
                                            min_binding_size: wgpu::BufferSize::new(
                                                prim_buffer_byte_size,
                                            ),
                                        },
                                        count: None,
                                    },
                                    wgpu::BindGroupLayoutEntry {
                                        binding: 2,
                                        visibility: wgpu::ShaderStages::VERTEX,
                                        ty: wgpu::BindingType::Buffer {
                                            ty: wgpu::BufferBindingType::Storage {
                                                read_only: true,
                                            },
                                            has_dynamic_offset: false,
                                            min_binding_size: wgpu::BufferSize::new(
                                                transform_buffer_byte_size,
                                            ),
                                        },
                                        count: None,
                                    },
                                ],
                            });

                        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                            label: Some("Bind group"),
                            layout: &bind_group_layout,
                            entries: &[
                                wgpu::BindGroupEntry {
                                    binding: 0,
                                    resource: wgpu::BindingResource::Buffer(
                                        globals_ubo.as_entire_buffer_binding(),
                                    ),
                                },
                                wgpu::BindGroupEntry {
                                    binding: 1,
                                    resource: wgpu::BindingResource::Buffer(
                                        prims_ssbo.as_entire_buffer_binding(),
                                    ),
                                },
                                wgpu::BindGroupEntry {
                                    binding: 2,
                                    resource: wgpu::BindingResource::Buffer(
                                        transforms_ssbo.as_entire_buffer_binding(),
                                    ),
                                },
                            ],
                        });

                        let pipeline_layout =
                            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                                bind_group_layouts: &[&bind_group_layout],
                                push_constant_ranges: &[],
                                label: None,
                            });

                        let render_pipeline_descriptor = wgpu::RenderPipelineDescriptor {
                            label: None,
                            layout: Some(&pipeline_layout),
                            vertex: wgpu::VertexState {
                                module: &vs_module,
                                entry_point: "main",
                                buffers: &[wgpu::VertexBufferLayout {
                                    array_stride: std::mem::size_of::<GpuVertex>() as u64,
                                    step_mode: wgpu::VertexStepMode::Vertex,
                                    attributes: &[
                                        wgpu::VertexAttribute {
                                            offset: 0,
                                            format: wgpu::VertexFormat::Float32x2,
                                            shader_location: 0,
                                        },
                                        wgpu::VertexAttribute {
                                            offset: 8,
                                            format: wgpu::VertexFormat::Uint32,
                                            shader_location: 1,
                                        },
                                    ],
                                }],
                            },
                            fragment: Some(wgpu::FragmentState {
                                module: &fs_module,
                                entry_point: "main",
                                targets: &[wgpu::ColorTargetState {
                                    format: wgpu::TextureFormat::Rgba8Unorm,
                                    blend: None,
                                    write_mask: wgpu::ColorWrites::ALL,
                                }],
                            }),
                            primitive: wgpu::PrimitiveState {
                                topology: wgpu::PrimitiveTopology::TriangleList,
                                polygon_mode: wgpu::PolygonMode::Fill,
                                front_face: wgpu::FrontFace::Ccw,
                                strip_index_format: None,
                                cull_mode: None,
                                unclipped_depth: false,
                                conservative: false,
                            },
                            depth_stencil: None,
                            multisample: wgpu::MultisampleState {
                                count: 4,
                                mask: !0,
                                alpha_to_coverage_enabled: false,
                            },
                            multiview: None,
                        };

                        let render_pipeline =
                            device.create_render_pipeline(&render_pipeline_descriptor);
                        queue.write_buffer(&transforms_ssbo, 0, bytemuck::cast_slice(&transforms));

                        queue.write_buffer(&prims_ssbo, 0, bytemuck::cast_slice(&primitives));

                        queue.write_buffer(
                            &globals_ubo,
                            0,
                            bytemuck::cast_slice(&[GpuGlobals {
                                aspect_ratio: 1.,
                                zoom: [
                                    2.0 / f32::max(texture_size as f32, texture_size as f32),
                                    2.0 / f32::max(texture_size as f32, texture_size as f32),
                                ],
                                pan: [texture_size as f32 / -2.0, texture_size as f32 / -2.0],
                                _pad: 0.0,
                            }]),
                        );

                        let u32_size = std::mem::size_of::<u32>() as u32;

                        let output_buffer_size =
                            (u32_size * texture_size * texture_size) as wgpu::BufferAddress;
                        let output_buffer_desc = wgpu::BufferDescriptor {
                            size: output_buffer_size,
                            // this tells wpgu that we want to read this buffer from the cpu
                            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                            label: None,
                            mapped_at_creation: false,
                        };
                        let output_buffer = device.create_buffer(&output_buffer_desc);
                        let mut encoder =
                            device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                                label: Some("Encoder"),
                            });

                        {
                            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                                label: None,
                                color_attachments: &[wgpu::RenderPassColorAttachment {
                                    view: &texture_view,
                                    ops: wgpu::Operations {
                                        load: wgpu::LoadOp::Clear(wgpu::Color::WHITE),
                                        store: false,
                                    },
                                    resolve_target: Some(&msaa_texture_view),
                                }],
                                depth_stencil_attachment: None,
                            });

                            pass.set_pipeline(&render_pipeline);
                            pass.set_bind_group(0, &bind_group, &[]);
                            pass.set_index_buffer(ibo.slice(..), wgpu::IndexFormat::Uint32);
                            pass.set_vertex_buffer(0, vbo.slice(..));

                            pass.draw_indexed(0..(mesh.indices.len() as u32), 0, 0..1);
                        }

                        encoder.copy_texture_to_buffer(
                            wgpu::ImageCopyTexture {
                                aspect: wgpu::TextureAspect::All,
                                texture: &msaa_texture,
                                mip_level: 0,
                                origin: wgpu::Origin3d::ZERO,
                            },
                            wgpu::ImageCopyBuffer {
                                buffer: &output_buffer,
                                layout: wgpu::ImageDataLayout {
                                    offset: 0,
                                    bytes_per_row: NonZeroU32::new(u32_size * texture_size),
                                    rows_per_image: NonZeroU32::new(texture_size),
                                },
                            },
                            texture_desc.size,
                        );

                        let frame_pixmap_buffer_len =
                            TVideo::WIDTH as u64 * TVideo::HEIGHT as u64 * 4;
                        let buffer_slice = output_buffer.slice(..frame_pixmap_buffer_len);
                        queue.submit(Some(encoder.finish()));

                        // NOTE: We have to create the mapping THEN device.poll() before await
                        // the future. Otherwise the application will freeze.
                        let mapping = buffer_slice.map_async(wgpu::MapMode::Read);
                        device.poll(wgpu::Maintain::Wait);
                        block_on(mapping).unwrap();

                        let data = buffer_slice.get_mapped_range();
                        frame.fill_from_rgba_pixmap(fr as i64, &data);

                        let stream = video_encoder.video_stream;
                        video_encoder.send_frame(&stream, frame)?;

                        logger.log_frame(fr, 0, &svg)
                    }

                    let resolved_audio_map: Option<ResolvedAudioMap> = video.audio().resolve(&ctx);
                    fill_audio_stream(video_encoder, resolved_audio_map.as_ref(), &ctx)?;

                    Ok(())
                },
            )?
        }?;

        logger.success(output, None);
        Ok(())
    }

    fn debug_frame<TVideo: Video + Sync + Sized>(
        &self,
        _frame: fframes::Frame,
        _out: &str,
        _video: TVideo,
        _usvg_options: &usvgr::Options,
        _fontdb: &usvgr_text_layout::fontdb::Database,
        _ctx: fframes::FFramesContext,
    ) -> FFramesResult<()> {
        todo!()
    }
}
