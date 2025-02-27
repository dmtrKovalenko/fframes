use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::sync::mpsc::SyncSender;
use std::sync::{
    Mutex,
    atomic::{AtomicUsize, Ordering},
};

use fframes_renderer::{FFramesRendererError, FFramesRendererResult};

pub trait OrderedRequest {
    fn index(&self) -> usize;
}

pub struct OrderedSender<T>
where
    T: OrderedRequest,
{
    next_frame: AtomicUsize,
    pending_frames: Mutex<BinaryHeap<Reverse<T>>>,
    sender: SyncSender<T>,
}

impl<T> OrderedSender<T>
where
    T: OrderedRequest + Ord,
{
    pub fn new(start: usize, sender: SyncSender<T>) -> Self {
        Self {
            next_frame: AtomicUsize::new(start),
            pending_frames: Mutex::new(BinaryHeap::new()),
            sender,
        }
    }

    pub fn send(&self, request: T) -> FFramesRendererResult<()> {
        let frame_index = request.index();

        if frame_index == self.next_frame.load(Ordering::Relaxed) {
            self.sender.send(request).map_err(|_| {
                FFramesRendererError::Custom("Ordered sender channel closed".to_string())
            })?;

            self.next_frame.fetch_add(1, Ordering::Release);
            self.try_send_pending_frames()
        } else {
            let mut pending = self.pending_frames.lock()?;
            pending.push(Reverse(request));
            Ok(())
        }
    }

    fn try_send_pending_frames(&self) -> FFramesRendererResult<()> {
        let mut pending = self.pending_frames.lock()?;

        while let Some(Reverse(request)) = pending.peek() {
            if request.index() != self.next_frame.load(Ordering::Relaxed) {
                break;
            }

            if let Some(Reverse(request)) = pending.pop() {
                self.sender.send(request).map_err(|_| {
                    FFramesRendererError::Custom("Ordered sender channel closed".to_string())
                })?;

                self.next_frame.fetch_add(1, Ordering::Release);
            }
        }
        Ok(())
    }
}
