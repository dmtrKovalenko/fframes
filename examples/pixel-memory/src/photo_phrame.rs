use fframes::{Color, Transform, media::ImageData};

pub struct PhotoFrame {
    pub x: f32,
    pub y: f32,
    pub width: u32,
    pub height: u32,
    pub stroke_width: u32,
    pub stroke_color: Color,
    pub rx: u32,
    pub ry: u32,
    pub opacity: f32,
    pub transform: Transform,
    pub transform_origin: String,
}

impl Default for PhotoFrame {
    fn default() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            width: 100,
            height: 100,
            stroke_width: 24,
            stroke_color: Color::WHITE,
            rx: 0,
            ry: 0,
            opacity: 1.0,
            transform: Transform::default(),
            transform_origin: "".to_string(),
        }
    }
}

pub trait FramedImage {
    fn render_framed(&self, frame: PhotoFrame) -> fframes::Svgr;
}

impl FramedImage for ImageData<'_> {
    fn render_framed(&self, frame: PhotoFrame) -> fframes::Svgr {
        fframes::svgr!(
            <rect
                x={frame.x}
                y={frame.y}
                width={frame.width}
                height={frame.height}
                rx={frame.rx}
                ry={frame.ry}
                stroke={frame.stroke_color}
                stroke-width={frame.stroke_width}
                opacity={frame.opacity}
                transform={frame.transform}
                transform-origin={frame.transform_origin.clone()}
            />

            <image
                href={self.href()}
                x={frame.x}
                y={frame.y}
                width={frame.width}
                height={frame.height}
                opacity={frame.opacity}
                transform={frame.transform}
                transform-origin={frame.transform_origin}
            />
        )
    }
}
