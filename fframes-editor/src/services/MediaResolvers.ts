import {
  MediaResolver,
  StaticMediaResolver,
  resolveMedia,
} from "../../src/services/mediaLoader.gen";
import { fontInfo } from "src/WasmController.gen";
import { Input, UrlSource, ALL_FORMATS } from "mediabunny";
import { registerVideo } from "./VideoFrameBufferManager";

interface VideoMetadata {
  width: number;
  height: number;
  duration: number;
}

const audioContext = new AudioContext();

export const resolveAudio: MediaResolver = async ({
  url,
  wasmController,
  name,
}) => {
  const response = await fetch(url);
  const arrayBuffer = await response.arrayBuffer();

  const audioData = await audioContext.decodeAudioData(arrayBuffer);
  const channelData = audioData.getChannelData(0);
  wasmController.add_audio_source(name, audioData.sampleRate, channelData);

  return resolveMedia(name, {
    tag: "Audio",
    value: {
      audioData,
      fltpData: channelData,
      sampleRate: audioData.sampleRate,
      duration: audioData.length / audioData.sampleRate,
    },
  });
};

export const resolveStaticAudios: StaticMediaResolver = async ({
  wasmController,
}) => {
  let i = 0;
  while (true) {
    let audio = wasmController.get_static_audio_data_by_index(i);
    if (!audio) {
      break;
    }

    const audioBuffer = audioContext.createBuffer(
      1,
      audio.fltp_data.length,
      audio.sample_rate
    );
    audioBuffer.copyToChannel(audio.fltp_data, 0);

    resolveMedia(audio.name, {
      tag: "Audio",
      value: {
        audioData: audioBuffer,
        sampleRate: audio.sample_rate,
        duration: audio.fltp_data.length / audio.sample_rate,
        fltpData: audio.fltp_data,
      },
    });

    i++;
  }
};

async function prepareFontMediaData(
  fontInfo: fontInfo,
  arrayBuffer: ArrayBuffer,
  name: string
) {
  if (!fontInfo.name || fontInfo.name === "") {
    console.error("Failed to process font file", name);
    return;
  }

  const fontFace = new FontFace(fontInfo.name, arrayBuffer);
  const loadedFont = await fontFace.load();
  document.fonts.add(loadedFont);

  return resolveMedia(name, {
    tag: "Font",
    value: {
      name: fontInfo.name,
      style: fontInfo.style.toLowerCase(),
      weight: fontInfo.weight,
      unicodeRange: loadedFont.unicodeRange,
    },
  });
}

export const resolveStaticFonts: StaticMediaResolver = async ({
  wasmController,
}) => {
  let i = 0;
  let fonts = [];
  while (true) {
    let font = wasmController.get_static_font_data_by_index(i);
    if (!font) {
      break;
    }

    fonts.push(font);
    i++;
  }

  await Promise.all(
    fonts.map(({ data, info, name }) => {
      return prepareFontMediaData(info, data.buffer, name);
    })
  );
};

export const resolveFont: MediaResolver = async ({
  name,
  url,
  wasmController,
}) => {
  const response = await fetch(url);
  const arrayBuffer = await response.arrayBuffer();

  let fontInfo = wasmController.ingest_font(new Uint8Array(arrayBuffer));
  return prepareFontMediaData(fontInfo, arrayBuffer, name);
};

export const resolveSubtitles: MediaResolver = async ({
  name,
  url,
  wasmController,
}) => {
  const response = await fetch(url);
  const text = await response.text();
  let phrasesCount = 0;

  wasmController.add_subtitles_source(name, text);

  return resolveMedia(name, {
    tag: "Subtitles",
    value: phrasesCount,
  });
};

// We load image through this very unfamiliar way because
// 1. It is fast enough and caches image in browser memory
// 2. We need to get image natural width and height which can be done only through encoding.
const loadImage = (url: string, sizeLimit: number) => {
  return Promise.all([
    fetch(url)
      .then(response => response.blob())
      .then(imageBlob => {
        if (imageBlob.size > sizeLimit) {
          return Promise.resolve(null);
        }

        return new Promise<string>((resolve, reject) => {
          const reader = new FileReader();
          reader.onload = e => {
            if (typeof e.target?.result !== "string") {
              return reject("Failed to read image as data url");
            }

            return resolve(e.target.result);
          };
          reader.onerror = e => {
            return reject(e);
          };

          reader.readAsDataURL(imageBlob);
        });
      }),
    new Promise<{ width: number; height: number }>((resolve, reject) => {
      const image = new Image();
      image.onload = () =>
        resolve({ width: image.naturalWidth, height: image.naturalHeight });
      image.onerror = reject;
      image.src = url;
    }),
  ]);
};

export const resolveImage: MediaResolver = async ({
  name,
  url,
  wasmController,
  wasmControllerOptions,
}) => {
  const [image, { width, height }] = await loadImage(
    url,
    wasmControllerOptions.dynamicImageSizeLimitBytes
  );

  wasmController.add_image_source(name, url, width, height, image ?? undefined);

  return resolveMedia(name, {
    tag: "Image",
    value: {
      width,
      height,
      src: url,
    },
  });
};

function loadVideoMetadataViaElement(url: string) {
  return new Promise<VideoMetadata>((resolve, reject) => {
    const videoElement = document.createElement("video");
    videoElement.src = url;

    const timeout = setTimeout(() => {
      videoElement.src = "";
      reject(new Error(`Video metadata load timeout for ${url}`));
    }, 10000);

    videoElement.addEventListener(
      "loadedmetadata",
      () => {
        clearTimeout(timeout);
        const metadata: VideoMetadata = {
          width: videoElement.videoWidth,
          height: videoElement.videoHeight,
          duration: videoElement.duration,
        };

        resolve(metadata);
      },
      { once: true }
    );

    videoElement.addEventListener(
      "error",
      e => {
        clearTimeout(timeout);
        reject(e);
      },
      { once: true }
    );
  });
}

async function loadVideoMetadataViaMediabunny(
  url: string
): Promise<VideoMetadata> {
  const source = new UrlSource(url);
  const input = new Input({ source, formats: ALL_FORMATS });
  try {
    const videoTrack = await input.getPrimaryVideoTrack();
    if (!videoTrack) throw new Error("No video track found");
    const duration = await input.computeDuration();
    return {
      width: videoTrack.codedWidth,
      height: videoTrack.codedHeight,
      duration,
    };
  } finally {
    input.dispose();
  }
}

async function loadVideoMetadata(url: string): Promise<VideoMetadata> {
  try {
    return await loadVideoMetadataViaElement(url);
  } catch {
    return await loadVideoMetadataViaMediabunny(url);
  }
}

export const resolveVideo: MediaResolver = async options => {
  const { url, wasmController, name } = options;

  let width: number, height: number, duration: number;
  try {
    ({ width, height, duration } = await loadVideoMetadata(url));
  } catch (e) {
    console.warn(`Could not load video metadata for ${name}, skipping.`, e);
    return "MediaResolved";
  }

  wasmController.add_video_source_placeholder(
    name,
    url,
    width,
    height,
    duration
  );

  registerVideo(name, url);

  try {
    return await resolveAudio(options);
  } catch (e) {
    // Videos without an audio track still render; only their audio is skipped.
    console.warn(`Could not decode the audio track of ${name}.`, e);
    return "MediaResolved";
  }
};
