use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use super::FFramesLogger;
use super::encoder::{Encoder, EncoderOutput};
use super::encoder_frame::EncoderFrame;
use super::renderer_error::{RenderEncodingError, RenderEncodingResult};
use super::scheduler::FrameClaim;
use crate::RenderOptions;

struct Segment {
    encoder: Encoder,
    frame: EncoderFrame,
    next_frame: usize,
    end: Option<usize>,
    /// RGBA frames that arrived before the frames preceding them.
    pending: BTreeMap<usize, Vec<u8>>,
}

type SegmentSlot = Arc<Mutex<Option<Segment>>>;

/// Encodes the frames handed out by a [`super::FrameScheduler`] into one intermediate
/// file per segment. Frames of a segment may arrive out of order from several threads;
/// they are encoded in order and the file is finalized as soon as its last frame is in.
#[doc(hidden)]
pub struct SegmentWriter<'a, 'o, 'm> {
    directory: &'a Path,
    extension: String,
    width: i32,
    height: i32,
    fps: i32,
    render_options: &'a RenderOptions<'o, 'm>,
    logger: &'a Arc<dyn FFramesLogger>,
    open: Mutex<HashMap<usize, SegmentSlot>>,
    finished: Mutex<Vec<(usize, PathBuf)>>,
}

impl<'a, 'o, 'm> SegmentWriter<'a, 'o, 'm> {
    pub fn new(
        directory: &'a Path,
        extension: &str,
        (width, height, fps): (i32, i32, i32),
        render_options: &'a RenderOptions<'o, 'm>,
        logger: &'a Arc<dyn FFramesLogger>,
    ) -> Self {
        Self {
            directory,
            extension: extension.to_owned(),
            width,
            height,
            fps,
            render_options,
            logger,
            open: Mutex::new(HashMap::new()),
            finished: Mutex::new(Vec::new()),
        }
    }

    fn segment_path(&self, segment: usize) -> PathBuf {
        self.directory
            .join(format!("{segment:010}.{}", self.extension))
    }

    /// Encodes the RGBA pixels of a claimed frame.
    pub fn submit(&self, claim: FrameClaim, rgba: &[u8]) -> RenderEncodingResult<()> {
        let slot = self
            .open
            .lock()
            .unwrap()
            .entry(claim.segment)
            .or_default()
            .clone();

        let mut guard = slot.lock().unwrap();
        if guard.is_none() {
            let encoder = unsafe {
                Encoder::new(
                    EncoderOutput::IntermediateChunk,
                    self.width,
                    self.height,
                    self.fps,
                    &self.segment_path(claim.segment),
                    self.render_options,
                    self.logger,
                )?
            };
            let frame = unsafe { EncoderFrame::new(&encoder.video_stream)? };
            *guard = Some(Segment {
                encoder,
                frame,
                next_frame: claim.segment,
                end: None,
                pending: BTreeMap::new(),
            });
        }

        let segment = guard.as_mut().unwrap();
        if claim.last_in_segment {
            segment.end = Some(claim.frame + 1);
        }

        if claim.frame == segment.next_frame {
            Self::encode(segment, claim.frame, rgba)?;
            while let Some(rgba) = segment.pending.remove(&segment.next_frame) {
                Self::encode(segment, segment.next_frame, &rgba)?;
            }
        } else {
            segment.pending.insert(claim.frame, rgba.to_vec());
        }

        if segment.end == Some(segment.next_frame) {
            let segment = guard.take().unwrap();
            unsafe {
                segment
                    .encoder
                    .flush_stream(&segment.encoder.video_stream)?;
            }
            // dropping the encoder writes the trailer
            drop(segment);

            self.open.lock().unwrap().remove(&claim.segment);
            self.finished
                .lock()
                .unwrap()
                .push((claim.segment, self.segment_path(claim.segment)));
        }

        Ok(())
    }

    fn encode(segment: &mut Segment, frame: usize, rgba: &[u8]) -> RenderEncodingResult<()> {
        unsafe {
            segment.frame.fill_from_rgba_pixmap(rgba);
            // frame indexes are used as pts, av_packet_rescale_ts converts them into the
            // stream time base
            segment.frame.set_pts(frame as i64);
            segment
                .encoder
                .send_frame(&segment.encoder.video_stream, &segment.frame)?;
        }
        segment.next_frame = frame + 1;
        Ok(())
    }

    /// The finished segment files in timeline order.
    pub fn finish(self) -> RenderEncodingResult<Vec<PathBuf>> {
        if !self.open.lock().unwrap().is_empty() {
            return Err(RenderEncodingError::Internal(
                "some video segments did not receive all of their frames".to_owned(),
            ));
        }

        let mut finished = self.finished.into_inner().unwrap();
        finished.sort_by_key(|(segment, _)| *segment);
        Ok(finished.into_iter().map(|(_, path)| path).collect())
    }
}
