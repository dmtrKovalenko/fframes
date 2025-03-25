#[macro_export]
macro_rules! impl_wasm_bridge_for {
    ($x:ty) => {
        #[wasm_bindgen]
        pub struct WasmBridge(fframes_editor_controller::WasmEditor<$x, ()>);

        impl WasmBridge {
            pub fn new(video: $x) -> Self {
                WasmBridge(fframes_editor_controller::WasmEditor::new(video, &()))
            }

            pub async fn update_video(&self, new_video: $x) -> Result<VideoMetadata, JsValue> {
                self.0.replace(new_video).await
            }
        }

        internal_impl_wasm_bridge!();
    };
    ($x:ty, $static_media:ty) => {
        #[wasm_bindgen]
        pub struct WasmBridge(fframes_editor_controller::WasmEditor<$x, $static_media>);

        impl WasmBridge {
            pub fn new(video: $x, static_media: &'static $static_media) -> Self {
                WasmBridge(fframes_editor_controller::WasmEditor::new(
                    video,
                    static_media,
                ))
            }

            pub async fn update_video(&self, new_video: $x) -> Result<VideoMetadata, JsValue> {
                self.0.replace(new_video).await
            }
        }

        internal_impl_wasm_bridge!();
    };
}

#[macro_export]
macro_rules! internal_impl_wasm_bridge {
    () => {
        #[wasm_bindgen]
        impl WasmBridge {
            #[wasm_bindgen(catch)]
            pub async fn prepare(&self, fps: Option<i32>) -> Result<VideoMetadata, JsValue> {
                self.0
                    .prepare(
                        fps.map(|fps| {
                            TryInto::<usize>::try_into(fps)
                                .map_err(|err| JsValue::from_str(&err.to_string()))
                        })
                        .transpose()?,
                    )
                    .await
            }

            #[wasm_bindgen]
            pub fn add_audio_source(&self, file: String, sample_rate: i32, input: &[f32]) {
                self.0.add_audio_source(file, sample_rate, input);
            }

            #[wasm_bindgen]
            pub fn add_subtitles_source(&self, file: String, content: String) -> usize {
                self.0.add_subtitles_source(file, content)
            }

            #[wasm_bindgen]
            pub fn add_image_source(
                &self,
                file: String,
                url: String,
                width: u32,
                height: u32,
                base64_data: Option<String>,
            ) {
                self.0
                    .add_image_source(file, url, width, height, base64_data);
            }

            #[wasm_bindgen]
            pub fn add_video_source_placeholder(
                &self,
                file: String,
                url: String,
                width: u32,
                height: u32,
                duration: f32,
            ) {
                self.0
                    .add_video_source_placeholder(file, url, width, height, duration);
            }

            #[wasm_bindgen]
            pub fn render_frame(&self, frame: i64) -> String {
                self.0.render_frame(frame)
            }

            #[wasm_bindgen]
            pub fn render_preview_frame(&self, frame: i64) -> String {
                self.0.render_preview_frame(frame)
            }

            #[wasm_bindgen]
            pub fn ingest_font(&self, slice: &[u8]) -> JsValue {
                self.0.ingest_font(slice)
            }

            #[wasm_bindgen]
            pub fn populate_static_fonts_db_with_static_fonts(&self) {
                self.0.populate_static_fonts_db_with_static_fonts();
            }

            #[wasm_bindgen]
            pub fn get_static_font_data_by_index(
                &self,
                index: usize,
            ) -> Option<wasm_font_source::StaticFontFace> {
                self.0.get_static_font_data_by_index(index)
            }

            #[wasm_bindgen]
            pub fn get_static_audio_data_by_index(
                &self,
                index: usize,
            ) -> Option<wasm_audio::AudioData> {
                self.0.get_static_audio_data_by_index(index)
            }
        }
    };
}
