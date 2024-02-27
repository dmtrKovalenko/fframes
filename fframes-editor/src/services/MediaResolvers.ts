import {
  MediaResolver,
  StaticMediaResolver,
  resolveMedia,
} from "fframes-editor/src/services/mediaLoader.gen";
import { fontInfo } from "src/WasmController.gen";

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
      audio.sample_rate,
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
  name: string,
  url?: string,
) {
  const decoder = new TextDecoder("utf-8");
  let fontName = fontInfo.name
    ? decoder.decode(new Uint8Array(fontInfo.name).buffer)
    : "unknown";

  if (!fontName) {
    console.error(
      `Can not parse the font file ${url} there is a huge chance that this font file won't work in the renderer. For now trying to fallback to browser based font`,
    );
  }

  let guaranteedFontName = fontName ?? name.replace(/\.[^/.]+$/, "");
  const fontFace = new FontFace(guaranteedFontName, arrayBuffer);

  const loadedFont = await fontFace.load();
  document.fonts.add(loadedFont);

  return resolveMedia(name, {
    tag: "Font",
    value: {
      name: guaranteedFontName,
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
    }),
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
  return prepareFontMediaData(fontInfo, arrayBuffer, name, url);
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
const loadImage = (url: string) =>
  new Promise<HTMLImageElement>((resolve, reject) => {
    const img = new Image();
    img.addEventListener("load", () => resolve(img));
    img.addEventListener("error", (err) => reject(err));
    img.src = url;
  });

const imageToBase64 = (image: HTMLImageElement) => {
  let canvas = document.createElement("canvas");
  canvas.width = image.width;
  canvas.height = image.height;

  let ctx = canvas.getContext("2d");
  ctx?.drawImage(image, 0, 0);

  return canvas.toDataURL("image/png");
};

export const resolveImage: MediaResolver = async ({
  name,
  url,
  wasmController,
  wasmControllerOptions,
}) => {
  const image = await loadImage(url);
  const base64 =
    image.naturalHeight * image.naturalWidth >
    wasmControllerOptions.dynamicImageLengthLimit
      ? null
      : imageToBase64(image);

  wasmController.add_image_source(name, url, base64 ?? undefined);

  return resolveMedia(name, {
    tag: "Image",
    value: {
      width: image.naturalWidth,
      height: image.naturalHeight,
      src: url,
    },
  });
};
