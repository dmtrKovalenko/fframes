import { Input, CanvasSink, UrlSource, ALL_FORMATS } from "mediabunny";

export class VideoFrameDecoder {
  private input: Input | null = null;
  private sink: CanvasSink | null = null;
  private width: number = 0;
  private height: number = 0;
  private initialized: boolean = false;
  private failed: boolean = false;

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

  private async decodeToCanvas(
    timeSeconds: number
  ): Promise<HTMLCanvasElement | null> {
    if (!this.initialized || !this.sink) {
      return null;
    }

    const wrapped = await this.sink.getCanvas(timeSeconds);
    if (!wrapped) return null;

    // The sink recycles its canvases (`poolSize`), so copy the frame out
    // before the next decode overwrites it.
    const srcCanvas = wrapped.canvas as HTMLCanvasElement;
    const tmp = document.createElement("canvas");
    tmp.width = srcCanvas.width;
    tmp.height = srcCanvas.height;
    const ctx = tmp.getContext("2d");
    if (!ctx) return null;
    ctx.drawImage(srcCanvas, 0, 0);
    return tmp;
  }

  /**
   * Decodes one frame into a blob URL. The caller owns the URL and must
   * revoke it (see `VideoFrameBufferManager.evictOldFrames`).
   */
  async decodeFrameAtTime(timeSeconds: number): Promise<string | null> {
    try {
      const canvas = await this.decodeToCanvas(timeSeconds);
      if (!canvas) return null;

      const blob = await new Promise<Blob | null>(resolve =>
        canvas.toBlob(resolve, "image/jpeg", 0.85)
      );
      if (!blob) return null;

      // Ownership of the URL passes to the caller, which revokes it.
      return URL.createObjectURL(blob);
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
    if (this.input) {
      try {
        this.input.dispose();
      } catch {}
      this.input = null;
    }

    this.sink = null;
    this.initialized = false;
  }
}
