import { MediaResolver, resolveMedia } from "./mediaLoader.gen";
import { createDecoder } from "minimp3-wasm/dist/minimp3-wasm";
// @ts-expect-error no  types
import minimp3decoderWasm from "minimp3-wasm/dist/decoder.opt.wasm?url";

const audioContext = new AudioContext();

export const resolveAudio: MediaResolver = async (
  name,
  url,
  wasmController
) => {
  const response = await fetch(url);
  const arrayBuffer = await response.arrayBuffer();

  const decoder = await createDecoder(
    new Uint8Array(arrayBuffer.slice(0)),
    minimp3decoderWasm
  );

  const data = await decoder.decode(decoder.duration);
  const length = Math.floor(data.pcm.length / data.numChannels);

  const monoPcm = new Int16Array(length);
  for (let i = 0, j = 0; i < length; i += 1, j += data.numChannels) {
    monoPcm[i] = data.pcm[j]; // or maybe we should do (data.pcm[j + 1]) / 2?
  }

  wasmController.add_audio_source(name, monoPcm);
  const audioData = await audioContext.decodeAudioData(arrayBuffer);

  return resolveMedia(name, {
    tag: "Audio",
    value: {
      arrayBuffer,
      audioData,
      monoPcmData: monoPcm,
      sampleRate: data.samplingRate,
      duration: monoPcm.length / data.samplingRate,
    },
  });
};

export const resolveSubtitles: MediaResolver = async (
  name,
  url,
  wasmController
) => {
  const response = await fetch(url);
  const text = await response.text();
  const phrasesCount = wasmController.add_subtitles_source(name, text);

  return resolveMedia(name, {
    tag: "Subtitles",
    value: phrasesCount,
  });
};

export const resolveFont: MediaResolver = async (name, url, wasmController) => {
  const response = await fetch(url);
  const arrayBuffer = await response.arrayBuffer();
  const fontInfo = wasmController.ingest_font(new Uint8Array(arrayBuffer));

  const decoder = new TextDecoder("utf-8");
  let fontName = decoder.decode(new Uint8Array(fontInfo.name).buffer);

  if (!fontName) {
    console.error(
      `Can not parse the font file ${url} there is a huge chance that this font file won't work in the renderer. For now trying to fallback to browser based font`
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
};

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

export const resolveImage: MediaResolver = async (
  name,
  url,
  wasmController
) => {
  const image = await loadImage(url);
  const base64 =
    image.naturalHeight * image.naturalWidth > 2073600 // full-hd
      ? null
      : imageToBase64(image);

  wasmController.add_image_source(name, url, base64);

  return resolveMedia(name, {
    tag: "Image",
    value: {
      width: image.naturalWidth,
      height: image.naturalHeight,
      src: url,
    },
  });
};
