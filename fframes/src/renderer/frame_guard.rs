use crate::{FFramesContext, Frame, Svgr, Video};
use serde::Serialize;
use std::panic::{AssertUnwindSafe, catch_unwind};

/// Where and why `Video::render_frame` panicked.
#[derive(Debug, Clone, Serialize)]
pub struct FramePanic {
    /// Global frame index.
    pub frame: usize,
    /// Global timestamp in seconds.
    pub seconds: f32,
    /// Name of the scene(s) active at this frame, if the video defines scenes.
    pub scenes: Vec<String>,
    pub message: String,
}

impl std::fmt::Display for FramePanic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "render_frame panicked at frame {} ({:.3}s)",
            self.frame, self.seconds
        )?;
        if !self.scenes.is_empty() {
            write!(f, " in scene {}", self.scenes.join(" + "))?;
        }
        write!(f, ": {}", self.message)
    }
}

/// Calls `Video::render_frame`, turning a panic into an error that says which frame, second
/// and scene it happened in. Rendering backends should call the video through this function.
pub fn render_frame_guarded<'a, TVideo: Video>(
    video: &'a TVideo,
    frame: Frame,
    ctx: &FFramesContext<'a, '_>,
) -> Result<Svgr<'a>, FramePanic> {
    let index = frame.global_index;
    let fps = frame.fps.max(1);

    catch_unwind(AssertUnwindSafe(|| video.render_frame(frame, ctx))).map_err(|payload| {
        let message = payload
            .downcast_ref::<&str>()
            .map(|s| s.to_string())
            .or_else(|| payload.downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "unknown panic payload".to_owned());

        crate::diagnostics::report(crate::diagnostics::Diagnostic::Panic {
            message: message.clone(),
        });

        FramePanic {
            frame: index,
            seconds: index as f32 / fps as f32,
            scenes: ctx
                .scenes_at(index)
                .map(|(_, name)| crate::short_scene_name(name).to_owned())
                .collect(),
            message,
        }
    })
}
