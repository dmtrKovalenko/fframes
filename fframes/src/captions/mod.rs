//! Timed captions from ordinary JSON or `WebVTT`, rendered with native SVG text.
//!
//! Enable the `captions` feature for rendering. The existing media [`crate::media::Subtitles`]
//! parser accepts both JSON and `WebVTT` without that feature.
//! Times are seconds; intervals are `[start, end)`.
//! A cue contains either plain `text` or explicitly timed `words`. Phrase-level
//! subtitles never receive guessed word timings.
//! Use plain-text VTT payloads: imported markup and cue settings remain source data,
//! rather than automatically becoming SVG styling, placement or word timings.
//!
//! ```no_run
//! use fframes::captions::{CaptionTrack, Captions, CaptionMode, CaptionRect};
//!
//! let directory = fframes::MediaDirectory::read_folder("media")?;
//! let media = directory.process_media_source()?;
//! let captions = Captions::from_media(
//!     "speech.json", CaptionRect::new(100.0, 1320.0, 880.0, 360.0))
//!     .font("DM Sans", 64)
//!     .mode(CaptionMode::Highlight)
//!     .active_fill("#ffd166")
//!     .outline("#000000", 4.0);
//! // In Video::render_frame, insert {captions.render(&mut frame, ctx)} into svgr!.
//! // Pass `media` in RenderOptions, or embed the file with include_media_dir!.
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! JSON containing `cues` is subtitle media; other JSON can be Lottie media.
//! Both subtitle formats use the same media provider and `ctx.get_subtitles` lookup.
//! Layout is cached by loaded track identity and rebuilt when the provider replaces it.
//! Reconstruct the component when fonts or layout settings change.
//! [`Captions::highlight_with`] accepts arbitrary word-local SVG for active words.
//! [`Captions::render_with`] customizes every visible word. Callbacks can retain the
//! preset content and use [`CaptionWordScope::mask_text`] for gradients or shader fills.
//! JSON `metadata` objects are preserved without imposing an application schema.
//! Call `scope.word.metadata.typed::<WordStyle>()` or
//! `scope.cue_metadata.typed::<CueStyle>()` directly: each requested Serde type is
//! decoded lazily once per metadata object. Successes and failures are cached
//! independently by type and shared across clones and threads. No registration,
//! extra media-loader configuration or per-frame JSON parsing is needed.
//! Rendering defaults to scene time; use [`CaptionClock::Global`] for a video-wide
//! transcript, or the explicit `render_at`/`highlight_at_with` variants for mapped time.
//! Legacy frame phrase/cue helpers retain inclusive cue ends. This new component
//! and [`CaptionTrack::active_cues`] use half-open intervals.
//! [`Captions::words_at`] exposes state, progress and
//! advance boxes for custom SVG effects. Layout uses the existing native
//! text measurements; it does not implement general bidirectional shaping.

mod data;
mod render;
#[cfg(test)]
mod tests;

pub use data::*;
pub use render::*;
