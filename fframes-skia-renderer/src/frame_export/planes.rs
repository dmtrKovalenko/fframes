use fframes::YuvMatrix;
use fframes::ffmpeg_sys_fframes::AVPixelFormat::{self, *};
use fframes::{
    FFramesRendererError, FFramesRendererResult, FrameLayout, FramePool, RenderEncodingError,
    VideoFrame,
};
use skia_safe::runtime_effect::ChildPtr;
use skia_safe::{
    AlphaType, BlendMode, ColorType, Data, FilterMode, ImageInfo, MipmapMode, Paint, Rect,
    RuntimeEffect, SamplingOptions, Surface, TileMode, gpu,
};

/// A channel of the encoded picture as weights of the premultiplied RGBA the renderer draws.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Channel {
    Y,
    U,
    V,
    A,
}

impl Channel {
    /// `([r, g, b, a], offset)` when converting with `matrix`.
    pub(crate) fn weights(self, matrix: YuvMatrix) -> ([f32; 4], f32) {
        let yuv = |row: usize| {
            let [r, g, b, offset] = matrix.rgb_to_yuv()[row];
            ([r, g, b, 0.], offset)
        };

        match self {
            Self::Y => yuv(0),
            Self::U => yuv(1),
            Self::V => yuv(2),
            Self::A => ([0., 0., 0., 1.], 0.),
        }
    }
}

/// A plane of a pixel format: one channel per byte, or two alternating ones (NV12).
#[derive(Debug, Clone, Copy)]
struct PlaneSpec {
    channels: (Channel, Option<Channel>),
    /// Frame pixels per sample horizontally and vertically.
    subsampling: (i32, i32),
}

const fn plane(channel: Channel, subsampling: (i32, i32)) -> PlaneSpec {
    PlaneSpec {
        channels: (channel, None),
        subsampling,
    }
}

const fn interleaved(first: Channel, second: Channel, subsampling: (i32, i32)) -> PlaneSpec {
    PlaneSpec {
        channels: (first, Some(second)),
        subsampling,
    }
}

/// The planes of the 8 bit YUV formats this converter produces, in libav plane order.
fn plane_specs(format: AVPixelFormat) -> Option<&'static [PlaneSpec]> {
    use Channel::{A, U, V, Y};

    const FULL: (i32, i32) = (1, 1);
    const HALF: (i32, i32) = (2, 2);
    const HALF_WIDTH: (i32, i32) = (2, 1);

    const YUV420P: &[PlaneSpec] = &[plane(Y, FULL), plane(U, HALF), plane(V, HALF)];
    const YUVA420P: &[PlaneSpec] = &[
        plane(Y, FULL),
        plane(U, HALF),
        plane(V, HALF),
        plane(A, FULL),
    ];
    const NV12: &[PlaneSpec] = &[plane(Y, FULL), interleaved(U, V, HALF)];
    const NV21: &[PlaneSpec] = &[plane(Y, FULL), interleaved(V, U, HALF)];
    const YUV422P: &[PlaneSpec] = &[plane(Y, FULL), plane(U, HALF_WIDTH), plane(V, HALF_WIDTH)];
    const YUV444P: &[PlaneSpec] = &[plane(Y, FULL), plane(U, FULL), plane(V, FULL)];

    match format {
        AV_PIX_FMT_YUV420P => Some(YUV420P),
        AV_PIX_FMT_YUVA420P => Some(YUVA420P),
        AV_PIX_FMT_NV12 => Some(NV12),
        AV_PIX_FMT_NV21 => Some(NV21),
        AV_PIX_FMT_YUV422P => Some(YUV422P),
        AV_PIX_FMT_YUV444P => Some(YUV444P),
        _ => None,
    }
}

/// A plane placed inside the packed surface.
#[derive(Debug, Clone, Copy)]
struct PlacedPlane {
    spec: PlaneSpec,
    x: i32,
    y: i32,
    /// Size in bytes (= surface pixels) and rows.
    width: i32,
    height: i32,
}

/// Where the planes of a format go in the packed surface: shelves of planes, every plane
/// starting at a column that keeps its lines aligned for the encoder.
#[derive(Debug)]
struct Packing {
    width: i32,
    height: i32,
    planes: Vec<PlacedPlane>,
}

impl Packing {
    const LINE_ALIGN: i32 = 64;
    const PLANE_ALIGN: i32 = 32;
    /// Lines the pixel buffer extends past the packed planes.
    const BOTTOM_PADDING: i32 = 32;

    fn new(specs: &[PlaneSpec], frame_width: i32, frame_height: i32) -> Self {
        let width = (frame_width + Self::LINE_ALIGN - 1) & !(Self::LINE_ALIGN - 1);
        let (mut x, mut y, mut shelf_height) = (0, 0, 0);

        let planes = specs
            .iter()
            .map(|&spec| {
                let samples = (frame_width + spec.subsampling.0 - 1) / spec.subsampling.0;
                let plane_width = samples * if spec.channels.1.is_some() { 2 } else { 1 };
                let plane_height = (frame_height + spec.subsampling.1 - 1) / spec.subsampling.1;

                if x + plane_width > width {
                    x = 0;
                    y += shelf_height;
                    shelf_height = 0;
                }

                let placed = PlacedPlane {
                    spec,
                    x,
                    y,
                    width: plane_width,
                    height: plane_height,
                };
                x = (x + plane_width + Self::PLANE_ALIGN - 1) & !(Self::PLANE_ALIGN - 1);
                shelf_height = shelf_height.max(plane_height);
                placed
            })
            .collect();

        Self {
            width,
            height: y + shelf_height,
            planes,
        }
    }

    /// Bytes of the planes one texel of the packed surface holds.
    const TEXEL_BYTES: i32 = 4;

    /// The packed surface as the pixel buffer of a frame: every plane is a window into it.
    fn frame_layout(&self) -> FrameLayout {
        let mut layout = FrameLayout {
            // encoders may read whole macroblocks, past the last line of the last plane
            size: (self.width * (self.height + Self::BOTTOM_PADDING) + Self::LINE_ALIGN) as usize,
            ..Default::default()
        };
        for (index, plane) in self.planes.iter().enumerate() {
            layout.planes[index] = Some(((plane.y * self.width + plane.x) as usize, self.width));
        }
        layout
    }
}

const PLANE_SHADER: &str = "
uniform shader frame;
// top left texel of the plane in the packed surface
uniform float2 origin;
// frame pixels per sample of the plane
uniform float2 subsampling;
// 1 when two channels alternate along a line
uniform float interleaved;
uniform float4 evenWeights;
uniform float4 oddWeights;
uniform float2 offsets;

// The center of a subsampled sample is the corner between the frame pixels it covers,
// where bilinear filtering returns their average.
float4 color(float index, float row) {
    return float4(frame.eval(float2(index + 0.5, row) * subsampling));
}

half4 main(float2 coord) {
    float2 position = coord - origin;
    // a texel holds four consecutive bytes of the plane
    float first = floor(position.x) * 4.0;
    float row = position.y;

    if (interleaved > 0.5) {
        float4 left = color(first * 0.5, row);
        float4 right = color(first * 0.5 + 1.0, row);
        return half4(float4(
            dot(left, evenWeights) + offsets.x,
            dot(left, oddWeights) + offsets.y,
            dot(right, evenWeights) + offsets.x,
            dot(right, oddWeights) + offsets.y));
    }

    return half4(float4(
        dot(color(first, row), evenWeights),
        dot(color(first + 1.0, row), evenWeights),
        dot(color(first + 2.0, row), evenWeights),
        dot(color(first + 3.0, row), evenWeights)) + offsets.x);
}
";

fn skia_error(message: &str) -> FFramesRendererError {
    FFramesRendererError::Skia(message.to_owned())
}

fn encoding_error(err: RenderEncodingError) -> FFramesRendererError {
    FFramesRendererError::from_chunk(0, err)
}

/// Converts rendered frames into planar YUV frames on the GPU, see the module docs.
pub(crate) struct PlaneExporter {
    surface: Surface,
    info: ImageInfo,
    effect: RuntimeEffect,
    packing: Packing,
    pool: FramePool,
    matrix: YuvMatrix,
}

impl PlaneExporter {
    /// `None` when `format` has no GPU conversion.
    pub(crate) fn new(
        gpu: &mut gpu::DirectContext,
        format: AVPixelFormat,
        matrix: YuvMatrix,
        width: i32,
        height: i32,
    ) -> Option<FFramesRendererResult<Self>> {
        let specs = plane_specs(format)?;
        Some(Self::with_specs(gpu, format, matrix, specs, width, height))
    }

    fn with_specs(
        gpu: &mut gpu::DirectContext,
        format: AVPixelFormat,
        matrix: YuvMatrix,
        specs: &[PlaneSpec],
        width: i32,
        height: i32,
    ) -> FFramesRendererResult<Self> {
        let packing = Packing::new(specs, width, height);

        let info = ImageInfo::new(
            (packing.width / Packing::TEXEL_BYTES, packing.height),
            ColorType::RGBA8888,
            AlphaType::Premul,
            None,
        );
        let surface = gpu::surfaces::render_target(
            gpu,
            gpu::Budgeted::Yes,
            &info,
            None,
            gpu::SurfaceOrigin::TopLeft,
            None,
            false,
            None,
        )
        .ok_or_else(|| skia_error("can not create the GPU surface for the converted planes"))?;

        let effect = RuntimeEffect::make_for_shader(PLANE_SHADER, None)
            .map_err(|err| FFramesRendererError::Skia(format!("plane shader: {err}")))?;
        let pool = FramePool::with_layout(format, width, height, packing.frame_layout())
            .map_err(encoding_error)?;

        Ok(Self {
            surface,
            info,
            effect,
            packing,
            pool,
            matrix,
        })
    }

    fn uniforms(&self, plane: &PlacedPlane) -> FFramesRendererResult<Data> {
        let (even_weights, even_offset) = plane.spec.channels.0.weights(self.matrix);
        let (odd_weights, odd_offset) = plane
            .spec
            .channels
            .1
            .map_or((even_weights, even_offset), |channel| {
                channel.weights(self.matrix)
            });

        super::pack_uniforms(
            &self.effect,
            &[
                (
                    "origin",
                    &[(plane.x / Packing::TEXEL_BYTES) as f32, plane.y as f32],
                ),
                (
                    "subsampling",
                    &[
                        plane.spec.subsampling.0 as f32,
                        plane.spec.subsampling.1 as f32,
                    ],
                ),
                (
                    "interleaved",
                    &[f32::from(u8::from(plane.spec.channels.1.is_some()))],
                ),
                ("evenWeights", &even_weights),
                ("oddWeights", &odd_weights),
                ("offsets", &[even_offset, odd_offset]),
            ],
        )
    }

    /// Draws the planes of what was drawn into `source`. Nothing reaches the GPU before
    /// the context is flushed or the planes are read back.
    pub(crate) fn convert(&mut self, source: &mut Surface) -> FFramesRendererResult<()> {
        let image = source.image_snapshot();
        let shader_with = |filter: FilterMode| {
            image
                .to_shader(
                    (TileMode::Clamp, TileMode::Clamp),
                    SamplingOptions::new(filter, MipmapMode::None),
                    None,
                )
                .ok_or_else(|| skia_error("can not sample the rendered frame"))
        };
        let full_resolution = shader_with(FilterMode::Nearest)?;
        let subsampled = shader_with(FilterMode::Linear)?;

        for plane in &self.packing.planes {
            let frame = if plane.spec.subsampling == (1, 1) {
                full_resolution.clone()
            } else {
                subsampled.clone()
            };
            let shader = self
                .effect
                .make_shader(self.uniforms(plane)?, &[ChildPtr::Shader(frame)], None)
                .ok_or_else(|| skia_error("can not instantiate the plane shader"))?;

            let mut paint = Paint::default();
            paint.set_shader(shader);
            paint.set_blend_mode(BlendMode::Src);
            // The last texel of a line may reach into the padding next to the plane.
            self.surface.canvas().draw_rect(
                Rect::from_xywh(
                    (plane.x / Packing::TEXEL_BYTES) as f32,
                    plane.y as f32,
                    ((plane.width + Packing::TEXEL_BYTES - 1) / Packing::TEXEL_BYTES) as f32,
                    plane.height as f32,
                ),
                &paint,
            );
        }

        Ok(())
    }

    /// Reads the converted planes back into a frame. `read` submits what was drawn and
    /// copies the packed surface into the buffer it is given (rows without padding).
    pub(crate) fn read_back(
        &mut self,
        read: impl FnOnce(&mut Surface, &ImageInfo, &mut [u8]) -> FFramesRendererResult<()>,
    ) -> FFramesRendererResult<VideoFrame> {
        let frame = self.pool.get().map_err(encoding_error)?;
        // The first plane starts at the beginning of the buffer the whole surface fits in.
        let pixels = unsafe {
            std::slice::from_raw_parts_mut(
                frame.plane(0).0,
                self.packing.width as usize * self.packing.height as usize,
            )
        };
        read(&mut self.surface, &self.info, pixels)?;

        Ok(frame)
    }
}
