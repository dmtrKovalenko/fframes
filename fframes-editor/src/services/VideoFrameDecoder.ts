import { Input, CanvasSink, UrlSource, ALL_FORMATS } from "mediabunny";

export class VideoFrameDecoder {
  private input: Input | null = null;
  private sink: CanvasSink | null = null;
  private width: number = 0;
  private height: number = 0;
  private initialized: boolean = false;
  private failed: boolean = false;
  private activeBlobUrls: Set<string> = new Set();

  isAvailable(): boolean {
    return typeof VideoDecoder !== "undefined";
  }

  isFailed(): boolean {
    return this.failed;
  }

  async initialize(videoUrl: string): Promise<void> {
    if (!this.isAvailable()) {
      throw new Error("WebCodecs is not available in this browser");
    }

    try {
      const source = new UrlSource(videoUrl);
      this.input = new Input({ source, formats: ALL_FORMATS });

      const videoTrack = await this.input.getPrimaryVideoTrack();
      if (!videoTrack) {
        throw new Error("No video track found");
      }

      const canDecode = await videoTrack.canDecode();
      if (!canDecode) {
        const codec = videoTrack.codec;
        const internalId = videoTrack.internalCodecId;
        throw new Error(
          `Codec not supported by this browser (${internalId || codec || "unknown"})`
        );
      }

      this.sink = new CanvasSink(videoTrack, { alpha: true, poolSize: 4 });
      this.width = videoTrack.codedWidth;
      this.height = videoTrack.codedHeight;
      this.initialized = true;
    } catch (error) {
      this.failed = true;
      this.dispose();
      throw error;
    }
  }

  private decodeToCanvas(timeSeconds: number): Promise<HTMLCanvasElement | null> {
    if (!this.initialized || !this.sink) {
      return Promise.resolve(null);
    }

    return this.sink.getCanvas(timeSeconds).then(wrapped => {
      if (!wrapped) return null;

      const srcCanvas = wrapped.canvas as HTMLCanvasElement;

      // Synchronously copy pixels to a temp canvas to prevent race conditions
      // with concurrent getCanvas() calls that may overwrite the pooled canvas.
      const tmp = document.createElement("canvas");
      tmp.width = srcCanvas.width;
      tmp.height = srcCanvas.height;
      const ctx = tmp.getContext("2d");
      if (!ctx) return null;
      ctx.drawImage(srcCanvas, 0, 0);
      return tmp;
    });
  }

  async decodeFrameAtTime(timeSeconds: number): Promise<string | null> {
    try {
      const canvas = await this.decodeToCanvas(timeSeconds);
      if (!canvas) return null;

      const blob = await new Promise<Blob | null>((resolve) =>
        canvas.toBlob(resolve, "image/jpeg", 0.85)
      );
      if (!blob) return null;

      const url = URL.createObjectURL(blob);
      this.activeBlobUrls.add(url);
      return url;
    } catch (error) {
      console.warn(`Frame decode error at ${timeSeconds}s:`, error);
      return null;
    }
  }

  async decodeFrameAsDataUrl(timeSeconds: number): Promise<string | null> {
    try {
      const canvas = await this.decodeToCanvas(timeSeconds);
      if (!canvas) return null;
      return canvas.toDataURL("image/jpeg", 0.85);
    } catch (error) {
      console.warn(`Poster frame decode error at ${timeSeconds}s:`, error);
      return null;
    }
  }

  getWidth(): number {
    return this.width;
  }

  getHeight(): number {
    return this.height;
  }

  dispose(): void {
    for (const url of this.activeBlobUrls) {
      URL.revokeObjectURL(url);
    }
    this.activeBlobUrls.clear();

    if (this.input) {
      try {
        this.input.dispose();
      } catch (e) {
        // Input disposal may throw if already disposed
      }
      this.input = null;
    }

    this.sink = null;
    this.initialized = false;
  }
}
