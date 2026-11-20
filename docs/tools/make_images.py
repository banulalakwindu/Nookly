"""Build Nookly's README media: the demo video/GIF and the screenshots.

    python docs/tools/make_images.py          # everything
    python docs/tools/make_images.py demo     # only the demo video + GIF
    python docs/tools/make_images.py shots    # only the screenshots

Needs: a release build (cargo build --release), Python 3 with Pillow and
numpy, and ffmpeg on PATH. Nothing here touches your real Nookly data:
the app runs against a throwaway data folder seeded with made-up content.

How it works: `nookly --render-demo DIR` plays a scripted session (a
virtual 30 fps clock and a scripted cursor driving the real UI) and writes
every frame of the 780x360 canvas at 2x as raw premultiplied BGRA, plus
demo.log with cursor positions, clicks and caption markers. This script puts
each frame on a "screen" card, draws the cursor and captions, and pipes the
result to ffmpeg. `--render-docs DIR` writes one still per view for the
screenshots.
"""

import math
import os
import shutil
import struct
import subprocess
import sys
import tempfile
import time
from pathlib import Path

import numpy as np
from PIL import Image, ImageDraw, ImageFilter, ImageFont

ROOT = Path(__file__).resolve().parent.parent.parent
EXE = ROOT / "target" / "release" / "nookly.exe"
IMAGES = ROOT / "docs" / "images"
WORK = Path(tempfile.gettempdir()) / "nookly-media"
FONTS = Path(os.environ.get("WINDIR", r"C:\Windows")) / "Fonts"

BG = (18, 20, 25)
TEXT = (245, 245, 247)
DIM = (160, 164, 176)
ACCENT = (255, 149, 0)
FPS = 30


def font(size, weight="regular"):
    names = {
        "regular": ["segoeui.ttf"],
        "semibold": ["seguisb.ttf", "segoeuib.ttf"],
        "bold": ["segoeuib.ttf"],
    }[weight]
    for n in names:
        if (FONTS / n).exists():
            return ImageFont.truetype(str(FONTS / n), size)
    return ImageFont.load_default()


# ------------------------------------------------------------------ seeding


def seed(data: Path):
    """A data folder with believable made-up content."""
    if data.exists():
        shutil.rmtree(data)
    files = data / "Demo files"
    files.mkdir(parents=True)
    for name, kb in [("Q3 report.pdf", 820), ("Invoice-2041.xlsx", 46), ("Logo final.png", 210), ("Meeting notes.docx", 64)]:
        (files / name).write_bytes(b"\0" * kb * 1024)
    (data / "settings.txt").write_text(
        "edge=top\nstyle=full\npersonality=1\nmascot=eyes\nvolume_overlay=1\nprivacy_dots=1\n"
        "clip_on=1\nclip_mode=text\nonboarded=1\nsound=chime\nnotif_on=0\nnotif_text=1\nlyrics=0\n",
        encoding="utf-8",
    )
    now = time.time()
    today = time.strftime("%Y-%m-%d")
    todo = [
        (1, 0, now + 25 * 60, "", "Send the weekly update"),
        (2, 0, now + 70 * 60, "", "Review pull request #42"),
        (3, 0, 0, "", "Book flights for the conference"),
        (4, 1, 0, today, "Call Kasun back"),
        (5, 1, 0, today, "Pick up groceries"),
    ]
    (data / "todo.txt").write_text("".join(f"{i}\t{d}\t{due}\t{day}\t{t}\t0\n" for i, d, due, day, t in todo), encoding="utf-8")
    water = "bottle=750\ngoal=2000\nsip=250\nstart=9\nend=21\nreminders=1\nsince_refill=500\n"
    water += "".join(f"log {today},{m},{ml}\n" for m, ml in [(560, 250), (640, 250), (735, 500), (870, 250)])
    (data / "water.txt").write_text(water, encoding="utf-8")
    (data / "shelf.txt").write_text("".join(f"{p}\n" for p in sorted(files.iterdir())), encoding="utf-8")
    (data / "clipboard.txt").write_text(f"t\t{now - 3600:.0f}\tMeeting room 4B, 3rd floor\n", encoding="utf-8")
    album_art(data / "art.png")
    glass_scene(data)


def album_art(path: Path, n=480):
    """Made-up cover for "Golden Hour Drive": the sun setting over the sea,
    its light on the water, and a palm tree against the sky."""
    rng = np.random.default_rng(7)
    y, x = np.mgrid[0:n, 0:n] / n
    horizon = 0.62
    # Sky: violet at the top through pink to warm orange at the horizon.
    stops = [(0.0, (58, 36, 110)), (0.35, (214, 76, 120)), (horizon, (255, 166, 77))]
    sky = np.zeros((n, n, 3))
    for (y0, c0), (y1, c1) in zip(stops, stops[1:]):
        k = ((y - y0) / (y1 - y0)).clip(0, 1)[..., None]
        band = ((y >= y0) & (y <= y1))[..., None]
        sky = np.where(band, np.array(c0) * (1 - k) + np.array(c1) * k, sky)
    img = sky / 255
    # The sun, half under the horizon, with a soft glow.
    d2 = (x - 0.5) ** 2 + ((y - horizon) * 1.0) ** 2
    sun = (d2 < 0.018) & (y < horizon)
    glow = np.exp(-d2 / 0.05)[..., None]
    img = img + np.array([1.0, 0.75, 0.4]) * 0.35 * glow
    img = np.where(sun[..., None], np.array([255, 226, 150]) / 255, img)
    # The sea: deep teal, darker towards the bottom, with the sun's path.
    k = ((y - horizon) / (1 - horizon)).clip(0, 1)[..., None]
    sea = np.array([24, 70, 96]) / 255 * (1 - k) + np.array([8, 28, 44]) / 255 * k
    img = np.where((y >= horizon)[..., None], sea, img)
    for _ in range(160):
        cy = rng.uniform(horizon + 0.01, 0.98)
        spread = 0.04 + (cy - horizon) * 0.35
        cx = 0.5 + rng.normal(0, spread / 2)
        w = rng.uniform(0.01, 0.05)
        row = int(cy * n)
        a, b = int((cx - w) * n), int((cx + w) * n)
        img[row:row + 2, max(a, 0):max(min(b, n), 0)] = np.array([255, 200, 120]) / 255
    # A palm tree on the left, in silhouette.
    palm = np.zeros((n, n), bool)
    for t in np.linspace(0, 1, 400):
        px, py = 0.16 + 0.06 * t ** 2, 1.0 - 0.62 * t
        r = 0.012 * (1.2 - t * 0.5)
        palm |= (x - px) ** 2 + (y - py) ** 2 < r * r
    top = (0.22, 0.38)
    for ang in np.linspace(-2.9, -0.1, 7):
        for t in np.linspace(0, 1, 160):
            px = top[0] + 0.2 * t * np.cos(ang)
            py = top[1] + 0.2 * t * np.sin(ang) + 0.12 * t * t
            r = 0.011 * (1 - t) + 0.002
            palm |= (x - px) ** 2 + (y - py) ** 2 < r * r
    img = np.where(palm[..., None], np.array([30, 14, 34]) / 255, img)
    Image.fromarray((img * 255).clip(0, 255).astype(np.uint8)).save(path)


def glass_scene(data: Path, w=1560, h=720):
    """A detailed desktop behind the notch for the Liquid Glass pictures:
    a sunset photo on the right, and a light app window on the left with a
    tab strip, text, buttons and a bar chart, so the glass has sharp things
    to blur and bend. The size of the whole canvas at 2x. Saved as PNG (for
    the picture) and raw BGRA (for the app's render)."""
    album_art(data / "scene_photo.png", n=720)
    img = Image.new("RGB", (w, h), (232, 236, 242))
    photo = Image.open(data / "scene_photo.png").convert("RGB")
    img.paste(photo, (w - 720, 0))
    d = ImageDraw.Draw(img)
    # The app window (a browser / dashboard), left of the photo.
    win = (40, 0, w - 560, h - 40)
    d.rounded_rectangle(win, 18, fill=(250, 250, 252), outline=(205, 210, 220), width=2)
    # Tab strip and address bar along the top, under the notch.
    d.rectangle((win[0], 0, win[2], 64), fill=(226, 231, 240))
    for k, (title, c) in enumerate([("Quarterly report", (66, 133, 244)), ("Inbox (3)", (234, 67, 53)), ("Roadmap", (52, 168, 83)), ("Design review", (251, 188, 5))]):
        x = win[0] + 20 + k * 230
        d.rounded_rectangle((x, 14, x + 218, 60), 10, fill=(255, 255, 255) if k == 0 else (236, 240, 247))
        d.ellipse((x + 14, 28, x + 32, 46), fill=c)
        d.text((x + 42, 37), title, font=font(20, "semibold"), fill=(40, 44, 52), anchor="lm")
    d.rounded_rectangle((win[0] + 20, 76, win[2] - 20, 114), 19, fill=(238, 241, 246))
    d.text((win[0] + 44, 95), "nookly.app/dashboard/q3", font=font(19), fill=(90, 96, 110), anchor="lm")
    # Heading, text lines and buttons.
    d.text((win[0] + 40, 160), "Q3 overview", font=font(44, "bold"), fill=(24, 28, 36), anchor="lm")
    for k, wd in enumerate([620, 560, 640, 500]):
        y = 210 + k * 34
        d.rounded_rectangle((win[0] + 40, y, win[0] + 40 + wd, y + 14), 7, fill=(196, 202, 214))
    for k, (label, c) in enumerate([("Export", (66, 133, 244)), ("Share", (52, 168, 83)), ("Archive", (255, 149, 0))]):
        x = win[0] + 40 + k * 170
        d.rounded_rectangle((x, 360, x + 150, 410), 25, fill=c)
        d.text((x + 75, 385), label, font=font(22, "semibold"), fill=(255, 255, 255), anchor="mm")
    # A bar chart.
    base = h - 90
    for k, (v, c) in enumerate(zip([0.45, 0.7, 0.55, 0.9, 0.65, 0.8, 0.5], [(66, 133, 244), (52, 168, 83), (251, 188, 5), (234, 67, 53), (142, 68, 173), (0, 172, 193), (255, 112, 67)])):
        x = win[0] + 720 + k * 62
        d.rounded_rectangle((x, base - int(v * 230), x + 40, base), 8, fill=c)
    d.line((win[0] + 700, base, win[0] + 720 + 7 * 62, base), fill=(180, 186, 198), width=3)
    img.save(data / "scene.png")
    a = np.array(img.convert("RGBA"))
    bgra = a[..., [2, 1, 0, 3]].copy()
    (data / "scene.raw").write_bytes(struct.pack("<II", w, h) + bgra.tobytes())


def run(args, data: Path):
    env = dict(os.environ, NOOKLY_DATA_DIR=str(data), NOOKLY_DEMO_ART=str(data / "art.png"), NOOKLY_GLASS_SCENE=str(data / "scene.raw"))
    subprocess.run([str(EXE), *args], env=env, check=True, timeout=1800)


# ------------------------------------------------------------------ frames


def read_raw(path: Path) -> Image.Image:
    """A raw frame (u32 w, u32 h, premultiplied BGRA rows) as straight RGBA."""
    b = path.read_bytes()
    w, h = struct.unpack("<II", b[:8])
    a = np.frombuffer(b, np.uint8, offset=8).reshape(h, w, 4).astype(np.float32)
    alpha = a[..., 3:4]
    rgb = np.where(alpha > 0, a[..., 2::-1] * 255.0 / np.maximum(alpha, 1), 0)
    return Image.fromarray(np.concatenate([rgb, alpha], -1).clip(0, 255).astype(np.uint8), "RGBA")


def wallpaper(w, h, t=0.0):
    """Nookly's desktop: deep ink with a warm amber glow up top, a teal one
    low down, and a faint dot grid (the brand colours of the logo)."""
    y, x = np.mgrid[0:h, 0:w].astype(np.float32)
    x /= w
    y /= h
    a = np.array([30, 24, 48], np.float32)
    b = np.array([14, 30, 40], np.float32)
    k = np.clip(0.35 * x + 0.65 * y, 0, 1)[..., None]
    img = a * (1 - k) + b * k
    for cx, cy, r, col, amt, sp in [(0.5, -0.05, 0.42, (255, 150, 60), 0.42, 0.06), (0.88, 0.95, 0.45, (60, 200, 185), 0.32, 0.05), (0.08, 0.7, 0.35, (120, 80, 220), 0.25, 0.04)]:
        cx += 0.04 * math.sin(t * sp * 6.28)
        cy += 0.03 * math.cos(t * sp * 6.28)
        g = np.exp(-(((x - cx) * w / h) ** 2 + (y - cy) ** 2) / (r * r))[..., None]
        img = img * (1 - amt * g) + np.array(col, np.float32) * amt * g
    # Dot grid, every 28 px.
    yy, xx = np.mgrid[0:h, 0:w]
    dots = ((xx % 28 == 0) & (yy % 28 == 0))[..., None]
    img = np.where(dots, img * 0.8 + 255 * 0.2 * 0.35, img)
    return Image.fromarray(img.clip(0, 255).astype(np.uint8)).convert("RGBA")


def rounded_mask(w, h, r):
    m = Image.new("L", (w, h), 0)
    ImageDraw.Draw(m).rounded_rectangle((0, 0, w - 1, h - 1), r, fill=255)
    return m


def fake_windows(card: Image.Image):
    """Two faint app windows so the card reads as a desktop. Drawn on their
    own layer and composited (drawing translucent shapes straight onto an
    opaque image replaces pixels instead of blending)."""
    layer = Image.new("RGBA", card.size, (0, 0, 0, 0))
    d = ImageDraw.Draw(layer)
    w, h = card.size
    for x0, y0, x1, y1 in [(0.06, 0.42, 0.44, 0.97), (0.54, 0.5, 0.95, 1.05)]:
        box = (int(x0 * w), int(y0 * h), int(x1 * w), int(y1 * h))
        d.rounded_rectangle(box, 22, fill=(255, 255, 255, 16), outline=(255, 255, 255, 40), width=2)
        d.rounded_rectangle((box[0], box[1], box[2], box[1] + 44), 22, fill=(255, 255, 255, 14))
        for i in range(3):
            d.ellipse((box[0] + 22 + i * 26, box[1] + 15, box[0] + 36 + i * 26, box[1] + 29), fill=(255, 255, 255, 46))
        for i in range(4):
            yy = box[1] + 80 + i * 40
            if yy + 14 < box[3] - 20:
                d.rounded_rectangle((box[0] + 30, yy, box[0] + 30 + int((box[2] - box[0]) * (0.7 - 0.12 * i)), yy + 14), 7, fill=(255, 255, 255, 22))
    card.alpha_composite(layer)


CURSOR = [(0, 0), (0, 33), (8, 25), (14, 39), (20, 36), (14, 23), (25, 23)]


def draw_cursor(im: Image.Image, x, y, alpha=1.0, s=1.35):
    if alpha <= 0:
        return
    layer = Image.new("RGBA", im.size, (0, 0, 0, 0))
    d = ImageDraw.Draw(layer)
    pts = [(x + px * s, y + py * s) for px, py in CURSOR]
    shadow = [(px + 3, py + 4) for px, py in pts]
    d.polygon(shadow, fill=(0, 0, 0, int(90 * alpha)))
    layer = layer.filter(ImageFilter.GaussianBlur(3))
    d = ImageDraw.Draw(layer)
    d.polygon(pts, fill=(255, 255, 255, int(255 * alpha)), outline=(10, 10, 12, int(255 * alpha)), width=3)
    im.alpha_composite(layer)


def ease(p):
    p = min(max(p, 0.0), 1.0)
    return 1 - (1 - p) ** 3


# ------------------------------------------------------------------ demo

STEPS = {
    "intro": ("", "Nookly"),
    "tour": ("01 · WELCOME TOUR", "Learn it in a minute"),
    "styles": ("02 · NOTCH STYLES", "Five looks, one notch"),
    "open": ("03 · HOVER TO OPEN", "Your music, right there"),
    "tabs": ("04 · CLICK OR SCROLL", "Everything in one place"),
    "alerts": ("05 · ALERTS", "Alerts that feel native"),
    "settings": ("06 · SETTINGS", "Make it yours"),
    "outro": ("FREE AND OPEN SOURCE", "Get Nookly"),
}


def parse_log(path: Path):
    frames, scenes, subs, rclicks = [], [], [], []
    for line in path.read_text(encoding="utf-8").splitlines():
        p = line.split(" ", 2)
        if p[0] == "f":
            n, x, y, c = line.split()[1:5]
            frames.append((float(x), float(y), c == "1"))
        elif p[0] == "scene":
            scenes.append((int(p[1]), p[2]))
        elif p[0] == "sub":
            subs.append((int(p[1]), p[2]))
        elif p[0] == "rclick":
            rclicks.append(int(p[1]))
    return frames, scenes, subs, rclicks


def latest(marks, n):
    cur = None
    for at, v in marks:
        if at <= n:
            cur = (at, v)
    return cur


def caption(im, n, scenes, subs, end, area):
    """Step label, title and subtitle, each sliding in when it changes."""
    x0, y0, x1, y1 = area
    sc = latest(scenes, n)
    if not sc:
        return
    start, name = sc
    nxt = next((at for at, _ in scenes if at > start), end)
    label, title = STEPS.get(name, ("", name))
    t_in = (n - start) / FPS
    t_out = (nxt - n) / FPS
    a = ease(t_in / 0.45) * min(1.0, t_out / 0.3)
    layer = Image.new("RGBA", im.size, (0, 0, 0, 0))
    d = ImageDraw.Draw(layer)
    cx = (x0 + x1) // 2
    dy = (1 - ease(t_in / 0.45)) * 26
    if label:
        f = font(26, "semibold")
        spaced = " ".join(label)
        d.text((cx, y0 + 22 + dy), spaced, font=f, fill=ACCENT + (int(255 * a),), anchor="mm")
    if name == "intro":
        logo = Image.open(ROOT / "assets" / "nookly.png").convert("RGBA").resize((92, 92), Image.LANCZOS)
        f = font(88, "bold")
        tw = d.textlength(title, font=f)
        lx = int(cx - (tw + 112) / 2)
        logo.putalpha(logo.getchannel("A").point(lambda v: int(v * a)))
        d.rounded_rectangle((lx - 6, int(y0 + 46 + dy), lx + 98, int(y0 + 150 + dy)), 26, fill=(44, 48, 60, int(255 * a)))
        layer.alpha_composite(logo, (lx, int(y0 + 52 + dy)))
        d.text((lx + 112, y0 + 98 + dy), title, font=f, fill=TEXT + (int(255 * a),), anchor="lm")
    else:
        d.text((cx, y0 + 92 + dy), title, font=font(72, "bold"), fill=TEXT + (int(255 * a),), anchor="mm")
    sb = latest(subs, n)
    if sb and sb[0] >= start - 2:
        s_in = (n - sb[0]) / FPS
        s_next = next((at for at, _ in subs if at > sb[0]), end)
        sa = ease(s_in / 0.35) * min(1.0, (s_next - n) / FPS / 0.2) * a
        sdy = (1 - ease(s_in / 0.35)) * 14
        f = font(40)
        col = ACCENT if name == "outro" else DIM
        d.text((cx, y0 + 168 + sdy), sb[1], font=f, fill=col + (int(255 * sa),), anchor="mm")
    im.alpha_composite(layer)


class Composer:
    """Turns rendered demo frames into finished 1920x1080 video frames."""

    W, H = 1920, 1080
    CARD = (120, 40, 1800, 760)

    def __init__(self, out: Path):
        self.out = out
        self.frames, self.scenes, self.subs, self.rclicks = parse_log(out / "demo.log")
        self.end = len(self.frames)
        x0, y0, x1, y1 = self.CARD
        self.cw, self.ch = x1 - x0, y1 - y0
        self.mask = rounded_mask(self.cw, self.ch, 30)
        self.base = Image.new("RGBA", (self.W, self.H), BG + (255,))
        shadow = Image.new("RGBA", (self.W, self.H), (0, 0, 0, 0))
        ImageDraw.Draw(shadow).rounded_rectangle((x0 - 4, y0 - 4, x1 + 4, y1 + 4), 34, fill=(0, 0, 0, 160))
        self.base.alpha_composite(shadow.filter(ImageFilter.GaussianBlur(24)))
        self.windows = Image.new("RGBA", (self.cw, self.ch), (0, 0, 0, 0))
        fake_windows(self.windows)
        self.clicks = [i for i, f in enumerate(self.frames) if f[2]] + self.rclicks
        self.wall = {}

    def to_screen(self, x, y, canvas_w):
        return self.CARD[0] + (self.cw - canvas_w) // 2 + x, self.CARD[1] + y

    def frame(self, n) -> Image.Image:
        t = n / FPS
        key = int(t * 6)  # the wallpaper drifts; refresh it 6x a second
        if key not in self.wall:
            self.wall.clear()
            wp = wallpaper(self.cw, self.ch, key / 6)
            wp.alpha_composite(self.windows)
            self.wall[key] = wp
        card = self.wall[key].copy()
        canvas = read_raw(self.out / f"f{n:04d}.raw")
        card.alpha_composite(canvas, ((self.cw - canvas.width) // 2, 0))
        im = self.base.copy()
        im.paste(card, self.CARD[:2], self.mask)
        d = ImageDraw.Draw(im, "RGBA")
        for c in self.clicks:
            age = (n - c) / FPS
            if 0 <= age < 0.45:
                rx, ry = self.to_screen(self.frames[c][0], self.frames[c][1], canvas.width)
                r = 10 + 46 * ease(age / 0.45)
                d.ellipse((rx - r, ry - r, rx + r, ry + r), outline=(255, 255, 255, int(200 * (1 - age / 0.45))), width=4)
        cx, cy = self.to_screen(self.frames[n][0], self.frames[n][1], canvas.width)
        draw_cursor(im, cx, cy, min(1.0, max(0.0, (self.CARD[3] - 30 - cy) / 60)))
        caption(im, n, self.scenes, self.subs, self.end, (0, 780, self.W, 1060))
        rgb = im.convert("RGB")
        f = min(1.0, t / 0.5, (self.end - n) / FPS / 0.6)
        if f < 1.0:
            rgb = Image.blend(Image.new("RGB", rgb.size, (0, 0, 0)), rgb, f)
        return rgb


def render_frames(data: Path) -> Path:
    out = WORK / "demo"
    if out.exists():
        shutil.rmtree(out)
    print("rendering demo frames...")
    run(["--render-demo", str(out)], data)
    return out


def make_demo(data: Path):
    out = render_frames(data)
    comp = Composer(out)
    mp4 = WORK / "demo.mp4"
    ff = subprocess.Popen(
        ["ffmpeg", "-y", "-loglevel", "error", "-f", "rawvideo", "-pix_fmt", "rgb24", "-s", f"{comp.W}x{comp.H}", "-r", str(FPS), "-i", "-",
         "-c:v", "libx264", "-preset", "slow", "-crf", "18", "-pix_fmt", "yuv420p", "-movflags", "+faststart", str(mp4)],
        stdin=subprocess.PIPE,
    )
    assert ff.stdin is not None
    print(f"compositing {comp.end} frames...")
    for n in range(comp.end):
        ff.stdin.write(comp.frame(n).tobytes())
        if n % 150 == 0:
            print(f"  {n}/{comp.end}")
    ff.stdin.close()
    ff.wait()
    gif = IMAGES / "demo.gif"
    pal = WORK / "palette.png"
    vf = "fps=12,scale=900:-1:flags=lanczos"
    subprocess.run(["ffmpeg", "-y", "-loglevel", "error", "-i", str(mp4), "-vf", f"{vf},palettegen=max_colors=128:stats_mode=diff", str(pal)], check=True)
    subprocess.run(["ffmpeg", "-y", "-loglevel", "error", "-i", str(mp4), "-i", str(pal), "-lavfi",
                    f"{vf}[x];[x][1:v]paletteuse=dither=bayer:bayer_scale=4:diff_mode=rectangle", str(gif)], check=True)
    shutil.copy(out / "demo.log", WORK / "demo.log")  # scene and caption timings
    shutil.rmtree(out)
    print(f"demo: {mp4} ({mp4.stat().st_size // 1024} KB), {gif} ({gif.stat().st_size // 1024} KB)")


# ------------------------------------------------------------------ main


def main():
    what = sys.argv[1] if len(sys.argv) > 1 else "all"
    if not EXE.exists():
        sys.exit("build first: cargo build --release")
    WORK.mkdir(exist_ok=True)
    data = WORK / "data"
    seed(data)
    if what in ("all", "demo"):
        make_demo(data)
    if what in ("all", "shots"):
        import make_shots  # noqa: E402  (docs/tools/make_shots.py)

        make_shots.make(data, run, read_raw, wallpaper, fake_windows, rounded_mask, font)


if __name__ == "__main__":
    sys.path.insert(0, str(Path(__file__).parent))
    main()
