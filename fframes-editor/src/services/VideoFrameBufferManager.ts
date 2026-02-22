import { VideoFrameDecoder } from './VideoFrameDecoder';

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

  constructor(fps: number, maxBufferSize: number = 48, maxInFlight: number = 4) {
    this.fps = fps;
    this.maxBufferSize = maxBufferSize;
    this.maxInFlight = maxInFlight;
  }

  getFrame(filename: string, pts: number): FrameData | null {
    const state = this.videos.get(filename);
    if (!state || state.width === 0 || state.height === 0) {
      return null;
    }

    const posterUrl = state.posterFrame?.url;

    // Try exact match first
    const exactFrame = state.buffer.get(pts);
    if (exactFrame) {
      state.lastHeldFrame = { pts, url: exactFrame };
      return {
        url: exactFrame,
        posterUrl,
        width: state.width,
        height: state.height
      };
    }

    // Frame holding: find most recent frame with pts <= requested
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
      return {
        url: bestUrl,
        posterUrl,
        width: state.width,
        height: state.height
      };
    }

    // Fall back to last held frame if available
    if (state.lastHeldFrame) {
      return {
        url: state.lastHeldFrame.url,
        posterUrl,
        width: state.width,
        height: state.height
      };
    }

    // Fall back to poster frame (data URL, works everywhere including SVG-as-image)
    if (state.posterFrame) {
      return state.posterFrame;
    }

    return null;
  }

  schedulePreload(filename: string, videoUrl: string, currentPts: number): void {
    this.lastRequestedPts.set(filename, currentPts);

    let state = this.videos.get(filename);

    if (!state) {
      // Create new video state
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
        videoUrl
      };
      this.videos.set(filename, state);
    }

    // Initialize decoder if needed
    if (!state.initPromise && state.decoder.isAvailable()) {
      state.initPromise = this.initializeDecoder(state);
    }

    // Schedule decode for current frame and next 8 frames,
    // but cap concurrent in-flight decodes to avoid overwhelming the decoder
    for (let i = 0; i <= 8; i++) {
      if (state.inFlightDecodes.size >= this.maxInFlight) {
        break;
      }

      const targetPts = currentPts + i;

      // Skip if already in buffer or in flight
      if (state.buffer.has(targetPts) || state.inFlightDecodes.has(targetPts)) {
        continue;
      }

      // Mark as in flight and start decode
      state.inFlightDecodes.add(targetPts);
      this.decodeFrame(filename, state, targetPts);
    }

    // Evict frames far from current position
    this.evictOldFrames(state, currentPts);
  }

  private async initializeDecoder(state: VideoState): Promise<void> {
    try {
      await state.decoder.initialize(state.videoUrl);
      if (!state.decoder.isFailed()) {
        state.width = state.decoder.getWidth();
        state.height = state.decoder.getHeight();

        // Decode frame 0 as a data URL poster for universal fallback
        // (data URLs work inside SVG-as-image for timeline canvas rendering)
        const posterUrl = await state.decoder.decodeFrameAsDataUrl(0);
        if (posterUrl) {
          state.posterFrame = {
            url: posterUrl,
            width: state.width,
            height: state.height
          };

          // Notify timeline to re-render now that poster is available
          window.dispatchEvent(new Event('fframes-poster-ready'));
        }
      }
    } catch (error) {
      console.error('Failed to initialize video decoder:', error);
    }
  }

  private async decodeFrame(
    filename: string,
    state: VideoState,
    pts: number
  ): Promise<void> {
    try {
      // Wait for initialization if needed
      if (state.initPromise) {
        await state.initPromise;
      }

      if (state.decoder.isFailed()) {
        return;
      }

      // Staleness check: skip if user has scrubbed far away from this target
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
      console.error(`Failed to decode frame at pts ${pts} for ${filename}:`, error);
    } finally {
      state.inFlightDecodes.delete(pts);
    }
  }

  private evictOldFrames(state: VideoState, currentPts: number): void {
    if (state.buffer.size <= this.maxBufferSize) {
      return;
    }

    // Evict frames far from current position in either direction
    const keepRadius = this.fps; // Keep ~1 second around current position
    const toEvict: number[] = [];

    for (const [pts, url] of state.buffer) {
      if (Math.abs(pts - currentPts) > keepRadius) {
        toEvict.push(pts);
        URL.revokeObjectURL(url);
      }
    }

    for (const pts of toEvict) {
      state.buffer.delete(pts);
    }
  }

  dispose(): void {
    for (const state of this.videos.values()) {
      // Revoke all blob URLs
      for (const url of state.buffer.values()) {
        URL.revokeObjectURL(url);
      }
      state.buffer.clear();

      // Dispose decoder
      state.decoder.dispose();
    }
    this.videos.clear();
  }
}

export function setupGlobalVideoFrameCallback(bufferManager: VideoFrameBufferManager): void {
  (window as any).__fframes_get_video_frame = (filename: string, pts: number) => {
    // Auto-trigger preloading when WASM requests a frame
    const videoUrls: Record<string, string> = (window as any).__fframes_video_urls || {};
    const videoUrl = videoUrls[filename];
    if (videoUrl) {
      bufferManager.schedulePreload(filename, videoUrl, pts);
    }

    return bufferManager.getFrame(filename, pts);
  };
}

let globalBufferManager: VideoFrameBufferManager | null = null;

export function initVideoFrameBufferManager(fps: number): void {
  if (globalBufferManager) {
    globalBufferManager.dispose();
  }
  globalBufferManager = new VideoFrameBufferManager(fps);
  setupGlobalVideoFrameCallback(globalBufferManager);
}

export function warmupVideoDecoder(filename: string, videoUrl: string): void {
  if (globalBufferManager) {
    globalBufferManager.schedulePreload(filename, videoUrl, 0);
  }
}