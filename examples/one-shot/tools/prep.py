"""Turn the downloaded stock footage and photos into the textures in media/.

    python3 -I tools/prep.py <work-dir> <example-dir>

<work-dir> holds the sources fetched by tools/fetch.sh (Pexels clips, Vision masks
from tools/vision.swift, photo cutouts). Every output is a derivative: heat fields
for the thermal shader, a 52px pixel atlas for the slop pot, resized cutouts and
sample points inside the fframes wordmark.

Heat atlases pack three consecutive frames into the R, G and B channels of one cell;
`heat.sksl` picks the channel. Value 0 is empty, the shader maps the rest through a
palette.
"""

import glob
import os
import subprocess
import sys

import numpy as np
from PIL import Image, ImageFilter
from scipy import ndimage

WORK, OUT = sys.argv[1], sys.argv[2]
MEDIA = os.path.join(OUT, "media")


def frames(video, start, count, step=1, crop=None):
    """Decode `count` frames as float RGB arrays via ffmpeg."""
    vf = [f"select='gte(n\\,{start})*not(mod(n-{start}\\,{step}))'"]
    if crop:
        x, y, w, h = crop
        vf.append(f"crop={w}:{h}:{x}:{y}")
    probe = subprocess.check_output(
        ["ffprobe", "-v", "error", "-select_streams", "v:0", "-show_entries",
         "stream=width,height", "-of", "csv=p=0", video]).decode().strip().split(",")
    w, h = (crop[2], crop[3]) if crop else (int(probe[0]), int(probe[1]))
    raw = subprocess.check_output(
        ["ffmpeg", "-v", "error", "-i", video, "-vf", ",".join(vf), "-fps_mode", "passthrough",
         "-frames:v", str(count), "-f", "rawvideo", "-pix_fmt", "rgb24", "-"])
    arr = np.frombuffer(raw, np.uint8).reshape(-1, h, w, 3).astype(np.float32) / 255
    return arr


def masks(folder, start, count, step=1, crop=None):
    out = []
    for i in range(start, start + count * step, step):
        m = np.asarray(Image.open(f"{folder}/{i:05d}.png"), np.float32) / 255
        if crop:
            x, y, w, h = crop
            m = m[y:y + h, x:x + w]
        out.append(m)
    return np.stack(out)


def blur(a, radius):
    img = Image.fromarray(np.clip(a * 255, 0, 255).astype(np.uint8))
    return np.asarray(img.filter(ImageFilter.GaussianBlur(radius)), np.float32) / 255


def dilate(a, size):
    img = Image.fromarray(np.clip(a * 255, 0, 255).astype(np.uint8))
    return np.asarray(img.filter(ImageFilter.MaxFilter(size)), np.float32) / 255


def smooth_time(stack):
    padded = np.concatenate([stack[:1], stack, stack[-1:]])
    return padded[:-2] * 0.25 + padded[1:-1] * 0.5 + padded[2:] * 0.25


def smoothstep(e0, e1, x):
    t = np.clip((x - e0) / (e1 - e0), 0, 1)
    return t * t * (3 - 2 * t)


def pack(fields, name, scale=1.0):
    """Three frames per RGB cell, cells left to right, top to bottom."""
    if scale != 1.0:
        h, w = fields[0].shape
        size = (round(w * scale), round(h * scale))
        fields = [np.asarray(Image.fromarray((f * 255).astype(np.uint8)).resize(size, Image.LANCZOS),
                             np.float32) / 255 for f in fields]
    h, w = fields[0].shape
    cells = (len(fields) + 2) // 3
    cols = int(np.ceil(np.sqrt(cells * h / w)))
    rows = (cells + cols - 1) // cols
    atlas = np.zeros((rows * h, cols * w, 3), np.float32)
    for i, f in enumerate(fields):
        c = i // 3
        atlas[(c // cols) * h:(c // cols + 1) * h, (c % cols) * w:(c % cols + 1) * w, i % 3] = f
    Image.fromarray(np.clip(atlas * 255 + 0.5, 0, 255).astype(np.uint8)).save(
        os.path.join(MEDIA, name), optimize=True)
    print(f"{name}: {len(fields)} frames, cell {w}x{h}, grid {cols}x{rows}")
    return len(fields), w, h, cols


def gun():
    # Toy pistol lying on a sheet, barrel down. Transposed it points right, the
    # hand holds it from the left.
    crop = (520, 0, 480, 700)
    src = frames(f"{WORK}/footage/5257461-hd_1920_1080_25fps.mp4", 0, 40, crop=crop)
    vis = masks(f"{WORK}/footage/gun-mask", 0, 40, crop=crop)
    out = []
    for rgb, m in zip(src, vis):
        luma = rgb @ np.array([0.299, 0.587, 0.114], np.float32)
        sat = rgb.max(-1) - rgb.min(-1)
        # The sheet is bright and grey. Anything dark or coloured is pistol or hand.
        key = np.maximum(smoothstep(0.66, 0.5, luma), smoothstep(0.09, 0.2, sat))
        region = dilate(m, 21)
        solid = (key * region) > 0.5
        # Close notches where camo patches touch the outline.
        yy, xx = np.mgrid[-14:15, -14:15]
        solid = ndimage.binary_closing(solid, structure=(xx * xx + yy * yy) <= 196) & (region > 0.5)
        # The white camo patches look like the sheet; fill enclosed holes smaller
        # than the trigger guard opening.
        holes, count = ndimage.label(~solid)
        sizes = ndimage.sum(np.ones_like(holes), holes, range(1, count + 1))
        border = set(np.unique(np.concatenate([holes[0], holes[-1], holes[:, 0], holes[:, -1]])))
        for label, size in enumerate(sizes, 1):
            if size < 900 and label not in border:
                solid[holes == label] = True
        field = blur(solid.astype(np.float32), 0.8)
        field = smoothstep(0.35, 0.65, field)
        shade = 0.55 + 0.45 * (1 - luma)
        out.append((field * shade).T)
    return pack(out, "gun-heat.png")


def girl():
    # Girl in white running right, the camera tracks her (Pexels 5273821).
    src = frames(f"{WORK}/footage/girl-run.mp4", 1, 42)
    vis = smooth_time(masks(f"{WORK}/footage/girl-mask", 1, 42))
    union = vis.max(0) > 0.3
    ys, xs = np.where(union)
    x0, x1 = max(xs.min() - 24, 0), min(xs.max() + 24, vis.shape[2])
    y0, y1 = max(ys.min() - 24, 0), vis.shape[1]
    print("girl crop", x0, y0, x1 - x0, y1 - y0)
    out = []
    for rgb, m in zip(src, vis):
        m = smoothstep(0.3, 0.7, m)
        r, g, b = rgb[..., 0], rgb[..., 1], rgb[..., 2]
        skin = smoothstep(0.05, 0.14, r - b) * smoothstep(0.3, 0.45, r)
        core = blur(m, 26)
        heat = m * np.clip(0.16 + 0.55 * core + 0.42 * skin, 0, 1)
        out.append(heat[y0:y1, x0:x1])
    return pack(out, "girl-heat.png", 0.8)


def rise():
    # Someone stands up from a low seat against a plain wall (Pexels 8011362,
    # 3.1-5.7 s). Only the silhouette is kept: the heat comes from the mask's
    # depth, not from the pixels, so no face or clothing detail survives.
    picks = list(range(40, 106))
    vis = smooth_time(masks(f"{WORK}/footage/rise-mask", 0, 125))
    union = vis[picks].max(0) > 0.3
    ys, xs = np.where(union)
    x0, x1 = max(xs.min() - 30, 0), min(xs.max() + 30, vis.shape[2])
    y0, y1 = max(ys.min() - 30, 0), min(ys.max() + 10, vis.shape[1])
    print("rise crop", x0, y0, x1 - x0, y1 - y0)
    out = []
    for i in picks:
        m = smoothstep(0.35, 0.65, vis[i])
        deep = blur(m, 22) * blur(m, 60)
        top = np.linspace(1, 0, m.shape[0], dtype=np.float32)[:, None]
        heat = m * np.clip(0.1 + 0.62 * deep + 0.22 * top, 0, 1)
        out.append(heat[y0:y1, x0:x1])
    return pack(out, "rise-heat.png", 0.75)


def slop():
    # Porridge stirred in a camping pot (Pexels 4909363), pixelated to 52px.
    rgb = frames(f"{WORK}/footage/4909363-hd_1920_1080_25fps.mp4", 75, 60, crop=(620, 122, 800, 800))
    n = 52
    small = []
    for f in rgb:
        img = Image.fromarray((f * 255).astype(np.uint8)).resize((n, n), Image.BOX)
        small.append(np.asarray(img, np.float32) / 255)
    stack = np.stack(small)
    # One shared palette so the pixels do not flicker between frames.
    flat = Image.fromarray((stack.reshape(-1, n, 3) * 255).astype(np.uint8))
    pal = flat.quantize(14, method=Image.Quantize.MEDIANCUT)
    yy, xx = np.mgrid[0:n, 0:n]
    d = np.hypot(xx + 0.5 - n / 2, yy + 0.5 - n / 2) / (n / 2)
    alpha = (d < 0.97).astype(np.float32)
    cols = 10
    rows = (len(stack) + cols - 1) // cols
    atlas = np.zeros((rows * n, cols * n, 4), np.uint8)
    for i, f in enumerate(stack):
        q = Image.fromarray((f * 255).astype(np.uint8)).quantize(palette=pal, dither=Image.Dither.NONE)
        cell = np.asarray(q.convert("RGB"))
        r, c = divmod(i, cols)
        atlas[r * n:(r + 1) * n, c * n:(c + 1) * n, :3] = cell
        atlas[r * n:(r + 1) * n, c * n:(c + 1) * n, 3] = alpha * 255
    Image.fromarray(atlas).save(os.path.join(MEDIA, "slop.png"))
    print(f"slop.png: {len(stack)} frames {n}px grid {cols}x{rows}")


TOOLS = [
    "1203819-1", "12997264-1", "13044706-1", "1772123-1", "187334-1", "187334-2",
    "187334-3", "187334-4", "210927-1", "30452350-1", "3394650-1", "4678185-1",
    "6461504-1", "7138915-1", "9227661-2", "9227661-3", "9227661-4", "1117543-1",
    "4523060-1",
]


def tools():
    sizes = []
    for i, name in enumerate(TOOLS):
        im = Image.open(f"{WORK}/cutouts/{name}.png").convert("RGBA")
        bbox = im.getchannel("A").point(lambda a: 255 if a > 24 else 0).getbbox()
        im = im.crop(bbox)
        im.thumbnail((300, 300), Image.LANCZOS)
        im.save(os.path.join(MEDIA, f"tool-{i:02}.png"), optimize=True)
        sizes.append(im.size)
    with open(os.path.join(OUT, "src", "tool_sizes.rs"), "w") as f:
        f.write("// Generated by tools/prep.py: pixel size of media/tool-NN.png.\n")
        f.write(f"pub(crate) const TOOL_SIZES: [[f32; 2]; {len(sizes)}] = [\n")
        for w, h in sizes:
            f.write(f"    [{w}., {h}.],\n")
        f.write("];\n")
    print("tools", len(sizes))


def logo_points():
    """Points on a jittered grid inside the solid letters of the wordmark (viewBox 392x99)."""
    paths = open(os.path.join(OUT, "src", "wordmark.paths")).read().split("\n")
    scale = 8
    svg = (f'<svg xmlns="http://www.w3.org/2000/svg" width="{392 * scale}" height="{99 * scale}" '
           f'viewBox="0 0 392 99"><rect width="392" height="99" fill="#fff"/>'
           f'<path d="{paths[2]}" fill="#000"/></svg>')
    tmp = os.path.join(WORK, "wordmark-raster.svg")
    open(tmp, "w").write(svg)
    png = os.path.join(WORK, "wordmark-raster.png")
    subprocess.check_call(["magick", "-background", "white", tmp, png])
    ink = np.asarray(Image.open(png).convert("L"), np.float32) / 255 < 0.5
    rng = np.random.default_rng(7)
    pts = []
    step = 4.6
    for gy in np.arange(2, 99, step * 0.87):
        for i, gx in enumerate(np.arange(2, 392, step)):
            x = gx + (step / 2 if int(gy / (step * 0.87)) % 2 else 0) + rng.uniform(-0.9, 0.9)
            y = gy + rng.uniform(-0.9, 0.9)
            px, py = int(x * scale), int(y * scale)
            if 0 <= py < ink.shape[0] and 0 <= px < ink.shape[1] and ink[py, px]:
                pts.append((round(x, 2), round(y, 2)))
    with open(os.path.join(OUT, "src", "logo_points.rs"), "w") as f:
        f.write("// Generated by tools/prep.py: points inside the wordmark letters, viewBox 392x99.\n")
        f.write(f"pub(crate) const LOGO_POINTS: [[f32; 2]; {len(pts)}] = [\n")
        for x, y in pts:
            f.write(f"    [{x:.2f}, {y:.2f}],\n")
        f.write("];\n")
    print("logo points", len(pts))


if __name__ == "__main__":
    os.makedirs(MEDIA, exist_ok=True)
    only = sys.argv[3:] or ["gun", "girl", "rise", "slop", "tools", "logo"]
    for step in only:
        {"gun": gun, "girl": girl, "rise": rise, "slop": slop, "tools": tools, "logo": logo_points}[step]()
