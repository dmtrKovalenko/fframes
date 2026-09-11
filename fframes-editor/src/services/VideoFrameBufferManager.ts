import { VideoFrameDecoder } from "./VideoFrameDecoder";

interface VideoState {
  decoder: VideoFrameDecoder;
  buffer: Map<number, string>; // pts -> blob URL
  width: number;
  height: number;
  inFlightDecodes: Set<number>;
  lastHeldFrame: { pts: number; url: string } | null;
  posterFrame: { url: string; width: number; height: number } | null;
  initPromise: Promise<void> | null;
  videoUrl: string;
}

interface FrameData {
  url: string;
  posterUrl?: string;
  width: number;
  height: number;
}

export class VideoFrameBufferManager {
  private readonly fps: number;
  private readonly maxBufferSize: number;
  private readonly maxInFlight: number;
  private readonly videos: Map<string, VideoState> = new Map();
  private readonly lastRequestedPts: Map<string, number> = new Map();
  private readonly videoUrls: Map<string, string> = new Map();

  constructor(
    fps: number,
    maxBufferSize: number = 48,
    maxInFlight: number = 4
  ) {
    this.fps = fps;
    this.maxBufferSize = maxBufferSize;
    this.maxInFlight = maxInFlight;
  }

  /** Makes a video known to the manager and starts decoding its first frames. */
  registerVideo(filename: string, videoUrl: string): void {
    this.videoUrls.set(filename, videoUrl);
    this.schedulePreload(filename, videoUrl, 0);
  }

  /**
   * Frame to display for `pts`. Playback requests also steer the decode
   * queue; timeline previews only read what is already buffered.
   */
  requestFrame(
    filename: string,
    pts: number,
    preview: boolean
  ): FrameData | null {
    const videoUrl = this.videoUrls.get(filename);
    if (videoUrl && !preview) {
      this.schedulePreload(filename, videoUrl, pts);
    }
    return this.getFrame(filename, pts);
  }

  getFrame(filename: string, pts: number): FrameData | null {
    const state = this.videos.get(filename);
    if (!state || state.width === 0 || state.height === 0) {
      return null;
    }

    const frameData = (url: string): FrameData => ({
      url,
      posterUrl: state.posterFrame?.url,
      width: state.width,
      height: state.height,
    });

    const exactFrame = state.buffer.get(pts);
    if (exactFrame) {
      state.lastHeldFrame = { pts, url: exactFrame };
      return frameData(exactFrame);
    }

    // Otherwise hold the latest decoded frame before `pts`.
    let bestPts = -1;
    let bestUrl: string | null = null;
    for (const [framePts, url] of state.buffer) {
      if (framePts <= pts && framePts > bestPts) {
        bestPts = framePts;
        bestUrl = url;
      }
    }

    if (bestUrl) {
      state.lastHeldFrame = { pts: bestPts, url: bestUrl };
      return frameData(bestUrl);
    }

    if (state.lastHeldFrame) {
      return frameData(state.lastHeldFrame.url);
    }

    return state.posterFrame;
  }

  schedulePreload(
    filename: string,
    videoUrl: string,
    currentPts: number
  ): void {
    this.lastRequestedPts.set(filename, currentPts);

    let state = this.videos.get(filename);

    if (!state) {
      const decoder = new VideoFrameDecoder();
      state = {
        decoder,
        buffer: new Map(),
        width: 0,
        height: 0,
        inFlightDecodes: new Set(),
        lastHeldFrame: null,
        posterFrame: null,
        initPromise: null,
        videoUrl,
      };
      this.videos.set(filename, state);
    }

    if (!state.initPromise && state.decoder.isAvailable()) {
      state.initPromise = this.initializeDecoder(state);
    }

    for (let i = 0; i <= 8; i++) {
      if (state.inFlightDecodes.size >= this.maxInFlight) {
        break;
      }

      const targetPts = currentPts + i;

      if (state.buffer.has(targetPts) || state.inFlightDecodes.has(targetPts)) {
        continue;
      }

      state.inFlightDecodes.add(targetPts);
      this.decodeFrame(filename, state, targetPts);
    }

    this.evictOldFrames(state, currentPts);
  }

  private async initializeDecoder(state: VideoState): Promise<void> {
    try {
      await state.decoder.initialize(state.videoUrl);
      if (!state.decoder.isFailed()) {
        state.width = state.decoder.getWidth();
        state.height = state.decoder.getHeight();

        const posterUrl = await state.decoder.decodeFrameAsDataUrl(0);
        if (posterUrl) {
          state.posterFrame = {
            url: posterUrl,
            width: state.width,
            height: state.height,
          };

          window.dispatchEvent(new Event("fframes-poster-ready"));
        }
      }
    } catch (error) {
      console.error("Failed to initialize video decoder:", error);
    }
  }

  private async decodeFrame(
    filename: string,
    state: VideoState,
    pts: number
  ): Promise<void> {
    try {
      if (state.initPromise) {
        await state.initPromise;
      }

      if (state.decoder.isFailed()) {
        return;
      }

      const latestPts = this.lastRequestedPts.get(filename) ?? pts;
      if (Math.abs(pts - latestPts) > this.fps) {
        return;
      }

      const timeSeconds = pts / this.fps;
      const blobUrl = await state.decoder.decodeFrameAtTime(timeSeconds);

      if (blobUrl) {
        state.buffer.set(pts, blobUrl);
      }
    } catch (error) {
      console.error(
        `Failed to decode frame at pts ${pts} for ${filename}:`,
        error
      );
    } finally {
      state.inFlightDecodes.delete(pts);
    }
  }

  private evictOldFrames(state: VideoState, currentPts: number): void {
    if (state.buffer.size <= this.maxBufferSize) {
      return;
    }

    const keepRadius = this.fps;
    const toEvict: number[] = [];

    for (const [pts, url] of state.buffer) {
      if (Math.abs(pts - currentPts) > keepRadius) {
        toEvict.push(pts);
        URL.revokeObjectURL(url);
        if (state.lastHeldFrame?.pts === pts) {
          // Never hand out a revoked blob URL.
          state.lastHeldFrame = null;
        }
      }
    }

    for (const pts of toEvict) {
      state.buffer.delete(pts);
    }
  }

  dispose(): void {
    for (const state of this.videos.values()) {
      for (const url of state.buffer.values()) {
        URL.revokeObjectURL(url);
      }
      state.buffer.clear();
      state.decoder.dispose();
    }
    this.videos.clear();
    this.videoUrls.clear();
  }
}

let globalBufferManager: VideoFrameBufferManager | null = null;
// Videos registered before the player (and thus the manager) exists.
const pendingVideos = new Map<string, string>();

/**
 * Creates the manager the WASM bridge calls into. The WASM side imports
 * `__fframes_get_video_frame` as a global function, so it is installed on
 * `window`; see `wasm_editor.rs`.
 */
export function initVideoFrameBufferManager(fps: number): void {
  globalBufferManager?.dispose();
  globalBufferManager = new VideoFrameBufferManager(fps);
  for (const [filename, url] of pendingVideos) {
    globalBufferManager.registerVideo(filename, url);
  }

  (window as any).__fframes_get_video_frame = (
    filename: string,
    pts: number,
    preview: boolean
  ) => globalBufferManager?.requestFrame(filename, pts, preview) ?? null;
}

export function registerVideo(filename: string, videoUrl: string): void {
  pendingVideos.set(filename, videoUrl);
  globalBufferManager?.registerVideo(filename, videoUrl);
}
