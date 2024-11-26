use fframes::{media::ImageData, FFramesContext};
use skia_safe::{
    image_asset::{create_image_frame_data, CustomImageAsset, ImageSizeFit},
    images::raster_from_data,
    resources::{ImageAsset, ImageFrameData, ResourceProvider},
    AlphaType, ColorType, Data, FontMgr, ISize, Image, ImageInfo, Matrix, SamplingOptions,
};

#[derive(Debug, Clone, Copy)]
pub(crate) struct SkiaFFramesProvider(pub &'static FFramesContext<'static, 'static>);

impl SkiaFFramesProvider {
    pub fn new(ctx: &FFramesContext) -> Self {
        // This is  that we do to satisfy the lifetime requirements of the
        // skia, as we always guarantee that fframes context will be living the whole lifetime
        // of the render functions be executed but there is not way to pass prove that for skia
        // bindings. So we do the most efficient and the easiest workaround -> casting pointer
        // as a static lifetime.
        #[allow(clippy::missing_transmute_annotations)]
        Self(unsafe { std::mem::transmute(ctx) })
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
        let ctx = self.0;

        let image = ctx.media_source?.resolve_image(resource_name)?;
        let skia_image = FFramesSkiaImage::new(image)?;

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
    fn new(image: &ImageData) -> Option<Self> {
        let image_info = ImageInfo::new(
            ISize::new(image.metadata.width as i32, image.metadata.height as i32),
            ColorType::RGBA8888,
            AlphaType::Premul,
            None,
        );

        let sk_image = raster_from_data(
            &image_info,
            unsafe { Data::new_bytes(image.get_bytes()) },
            image.metadata.width as usize * 4,
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
