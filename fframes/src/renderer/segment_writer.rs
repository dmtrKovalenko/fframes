use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use super::FFramesLogger;
use super::concatenator::AvPacketAutoFree;
use super::encoder::{Encoder, EncoderOutput};
use super::frame_export::{EncoderInput, RgbaFrameConverter, VideoEncoderInfo, VideoFrame};
use super::renderer_error::{RenderEncodingError, RenderEncodingResult};
use super::scheduler::FrameClaim;
use crate::RenderOptions;

struct Segment {
    encoder: Encoder,
    packet: AvPacketAutoFree,
    next_frame: usize,
    end: Option<usize>,
    /// Frames that arrived before the frames preceding them.
    pending: BTreeMap<usize, VideoFrame>,
    /// Only for frames submitted as RGBA pixels.
    converter: Option<RgbaFrameConverter>,
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
    input: EncoderInput,
    open: Mutex<HashMap<usize, SegmentSlot>>,
    finished: Mutex<Vec<(usize, PathBuf)>>,
}

impl<'a, 'o, 'm> SegmentWriter<'a, 'o, 'm> {
    /// A writer whose encoders take the pixel format requested in `render_options`.
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
            input: EncoderInput::for_options(&render_options.video_encoder_options),
            open: Mutex::new(HashMap::new()),
            finished: Mutex::new(Vec::new()),
        }
    }

    /// Opens the encoders for the frames a backend negotiated, see [`Self::encoder_info`].
    pub fn with_encoder_input(mut self, input: EncoderInput) -> Self {
        self.input = input;
        self
    }

    /// The video encoder the segments will be encoded with, to negotiate its input
    /// (`FFramesRenderBackend::negotiate_encoder_input`).
    pub fn encoder_info(&self) -> RenderEncodingResult<VideoEncoderInfo<'a>> {
        VideoEncoderInfo::for_output(
            &self.segment_path(0),
            (self.width, self.height, self.fps),
            &self.render_options.video_encoder_options,
        )
    }

    /// What every frame given to [`Self::submit_frame`] has to be.
    pub fn encoder_input(&self) -> &EncoderInput {
        &self.input
    }

    fn segment_path(&self, segment: usize) -> PathBuf {
        self.directory
            .join(format!("{segment:010}.{}", self.extension))
    }

    /// Encodes a claimed frame.
    pub fn submit_frame(&self, claim: FrameClaim, frame: VideoFrame) -> RenderEncodingResult<()> {
        self.submit_with(claim, |_| Ok(frame))
    }

    /// Encodes the RGBA pixels of a claimed frame, converting them on the CPU.
    pub fn submit(&self, claim: FrameClaim, rgba: &[u8]) -> RenderEncodingResult<()> {
        self.submit_with(claim, |segment| self.convert(segment, rgba))
    }

    /// Like [`Self::submit`] for callers that recycle their buffers. Returns the buffers
    /// that are not needed anymore.
    pub fn submit_owned(
        &self,
        claim: FrameClaim,
        rgba: Vec<u8>,
    ) -> RenderEncodingResult<Vec<Vec<u8>>> {
        self.submit(claim, &rgba)?;
        Ok(vec![rgba])
    }

    fn convert(&self, segment: &mut Segment, rgba: &[u8]) -> RenderEncodingResult<VideoFrame> {
        if segment.converter.is_none() {
            segment.converter = Some(RgbaFrameConverter::for_input(
                &self.input,
                self.width,
                self.height,
            )?);
        }

        segment
            .converter
            .as_mut()
            .expect("the converter was just created")
            .convert(rgba)
    }

    fn submit_with(
        &self,
        claim: FrameClaim,
        frame: impl FnOnce(&mut Segment) -> RenderEncodingResult<VideoFrame>,
    ) -> RenderEncodingResult<()> {
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
                Encoder::new_with_input(
                    EncoderOutput::IntermediateChunk,
                    self.width,
                    self.height,
                    self.fps,
                    &self.segment_path(claim.segment),
                    self.render_options,
                    &self.input,
                    self.logger,
                )?
            };
            *guard = Some(Segment {
                encoder,
                packet: AvPacketAutoFree::new(),
                next_frame: claim.segment,
                end: None,
                pending: BTreeMap::new(),
                converter: None,
            });
        }

        let segment = guard.as_mut().unwrap();
        if claim.last_in_segment {
            segment.end = Some(claim.frame + 1);
        }

        let frame = frame(segment)?;
        if claim.frame == segment.next_frame {
            Self::encode(segment, claim.frame, &frame)?;
            // the encoder holds its own reference for as long as it needs the pixels
            drop(frame);
            while let Some(frame) = segment.pending.remove(&segment.next_frame) {
                Self::encode(segment, segment.next_frame, &frame)?;
            }
        } else {
            segment.pending.insert(claim.frame, frame);
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

    fn encode(segment: &mut Segment, index: usize, frame: &VideoFrame) -> RenderEncodingResult<()> {
        unsafe {
            // frame indexes are used as pts, av_packet_rescale_ts converts them into the
            // stream time base
            segment
                .encoder
                .send_video_frame(frame, index as i64, segment.packet.get())?;
        }
        segment.next_frame = index + 1;
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
