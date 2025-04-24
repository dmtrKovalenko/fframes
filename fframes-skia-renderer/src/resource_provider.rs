use fframes::{
    FFramesContext, VideoDecodersWorker, media::decode_video_resource, usvgr::PreloadedImageData,
};
use skia_safe::{
    AlphaType, ColorType, Data, FontMgr, ISize, Image, ImageInfo, Matrix, SamplingOptions,
    image_asset::{CustomImageAsset, ImageSizeFit, create_image_frame_data},
    images::raster_from_data,
    resources::{ImageAsset, ImageFrameData, ResourceProvider},
};

#[derive(Debug, Clone)]
pub(crate) struct SkiaFFramesProvider {
    pub(crate) worker_local_decoders: VideoDecodersWorker,
    pub(crate) ctx: &'static FFramesContext<'static, 'static>,
}

impl SkiaFFramesProvider {
    pub fn new(worker_local_decoders: VideoDecodersWorker, ctx: &FFramesContext) -> Self {
        // This is that we do to satisfy the lifetime requirements of the
        // skia, as we always guarantee that fframes context will be living the whole lifetime
        // of the render functions be executed but there is not way to pass prove that for skia
        // bindings. So we do the most efficient and the easiest workaround -> casting pointer
        // as a static lifetime.
        Self {
            ctx: unsafe {
                std::mem::transmute::<&FFramesContext, &FFramesContext<'static, 'static>>(ctx)
            },
            worker_local_decoders,
        }
    }
}

unsafe impl Send for SkiaFFramesProvider {}
unsafe impl Sync for SkiaFFramesProvider {}

impl ResourceProvider for SkiaFFramesProvider {
    fn load_image_asset(
        &self,
        _resource_path: &str,
        resource_name: &str,
        _resource_id: &str,
    ) -> Option<ImageAsset> {
        let image = if let Some((resource_name, pts)) = decode_video_resource(resource_name) {
            self.worker_local_decoders
                .get_buffered_frame_image(pts, resource_name)
                .ok()
                .flatten()?
        } else {
            self.ctx.media_source?.resolve_image(resource_name)?.href()
        };

        let skia_image = FFramesSkiaImage::new(&image)?;
        ImageAsset::from_custom_image_asset(skia_image)
    }

    fn load(&self, _resource_path: &str, _resource_name: &str) -> Option<skia_safe::Data> {
        None
    }

    fn load_typeface(&self, _name: &str, _url: &str) -> Option<skia_safe::Typeface> {
        None
    }

    fn font_mgr(&self) -> FontMgr {
        FontMgr::empty()
    }
}

struct FFramesSkiaImage(Image);
impl FFramesSkiaImage {
    fn new(image: &PreloadedImageData) -> Option<Self> {
        let image_info = ImageInfo::new(
            ISize::new(image.width as i32, image.height as i32),
            ColorType::RGBA8888,
            AlphaType::Premul,
            None,
        );

        let sk_image = raster_from_data(
            &image_info,
            unsafe { Data::new_bytes(&image.data) },
            image.width as usize * 4,
        )?;

        Some(Self(sk_image))
    }
}

impl CustomImageAsset for FFramesSkiaImage {
    fn is_multi_frame(&self) -> bool {
        false
    }

    fn get_frame_data(&self, _: f32) -> ImageFrameData {
        create_image_frame_data(
            &self.0,
            Matrix::default(),
            SamplingOptions::default(),
            ImageSizeFit::kFill,
        )
    }
}
