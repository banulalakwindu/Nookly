"""The taskbar's pictures for its page and the README (run via
make_images.py, after make_shots.py: the notch's docs render draws the
taskbar's widget faces into the data folder, see bar_faces in
src/app/selftest.rs).

`nookly-taskbar --render-docs` draws the bar and its pop-ups with made-up
apps and windows (src/bin/nookly-taskbar/render.rs); here each one goes on
a desktop, cut to what matters, with a title, a line and chips under it, as
the notch's pictures are.
"""

import os
import struct
import subprocess
from pathlib import Path

import numpy as np
from PIL import Image, ImageDraw

ROOT = Path(__file__).resolve().parent.parent.parent
IMAGES = ROOT / "docs" / "images"
EXE = ROOT / "target" / "release" / "nookly-taskbar.exe"
BG = (13, 12, 20)
TEXT = (245, 245, 247)
DIM = (160, 164, 176)
CHIP = (30, 26, 40)
AMBER = (255, 180, 60)
TEAL = (92, 225, 210)
W = 1600
PAD = 40
# The screen the taskbar is drawn on (render.rs): 1280 x 720 at 2x, and
# the bottom of it each render holds.
SW, SH, SHOT = 2560, 1440, 1040


def desktop(wallpaper, fake_windows, light=False):
    """The whole screen behind the bar: the notch pictures' desktop, or a
    light app window filling it (for light glass)."""
    if light:
        img = Image.new("RGBA", (SW, SH), (238, 241, 246, 255))
        d = ImageDraw.Draw(img)
        for k in range(14):
            y = 120 + k * 92
            d.rounded_rectangle((160, y, 160 + [1900, 1500, 1750, 1200][k % 4], y + 26), 13, fill=(205, 211, 222))
        for k, c in enumerate([(66, 133, 244), (52, 168, 83), (251, 188, 5), (234, 67, 53)]):
            d.rounded_rectangle((300 + k * 520, 1180, 700 + k * 520, 1420), 24, fill=c)
        return img
    img = wallpaper(SW, SH, 0.6)
    fake_windows(img)
    return img


def write_raw(img, path):
    a = np.array(img.convert("RGBA"))
    bgra = a[..., [2, 1, 0, 3]].copy()
    path.write_bytes(struct.pack("<II", img.width, img.height) + bgra.tobytes())


def make(data, read_raw, wallpaper, fake_windows, rounded_mask, font):
    out = Path(data) / "taskbar"
    out.mkdir(exist_ok=True)
    dark, light = desktop(wallpaper, fake_windows), desktop(wallpaper, fake_windows, light=True)
    write_raw(dark, out / "scene_dark.raw")
    write_raw(light, out / "scene_light.raw")
    for f in Path(data).glob("bar_*"):
        (out / f.name).write_bytes(f.read_bytes())
    env = dict(os.environ, NOOKLY_DATA_DIR=str(data), NOOKLY_GLASS_SCENE=str(out / "scene_dark.raw"), NOOKLY_GLASS_SCENE_DARK=str(out / "scene_dark.raw"), NOOKLY_GLASS_SCENE_LIGHT=str(out / "scene_light.raw"))
    print("rendering the taskbar...")
    subprocess.run([str(EXE), "--render-docs", str(out)], env=env, check=True, timeout=600)

    def page(card_img, title, sub, chips):
        """Card on the page, then the title block (as make_shots does)."""
        cw, ch = card_img.size
        h = PAD + ch + 36 + 70 + 44 + (64 if chips else 0) + 24
        im = Image.new("RGBA", (W, h), BG + (255,))
        im.paste(card_img, (PAD, PAD), rounded_mask(cw, ch, 26))
        d = ImageDraw.Draw(im)
        y = PAD + ch + 36
        d.rounded_rectangle((PAD + 4, y - 14, PAD + 64, y - 8), 3, fill=AMBER)
        d.ellipse((PAD + 72, y - 15, PAD + 80, y - 7), fill=TEAL)
        d.text((PAD + 4, y), title, font=font(54, "bold"), fill=TEXT)
        y += 72
        d.text((PAD + 4, y), sub, font=font(28), fill=DIM)
        y += 52
        x = PAD
        f = font(22, "semibold")
        for c in chips:
            tw = d.textlength(c, font=f)
            d.rounded_rectangle((x, y, x + tw + 36, y + 42), 21, fill=CHIP, outline=AMBER + (150,), width=2)
            d.text((x + 18, y + 21), c, font=f, fill=(255, 214, 150), anchor="lm")
            x += tw + 48
        return im.convert("RGB")

    def scene(raw, back, margin=70, top=None, focus=None):
        """The render on its desktop, cut around what's drawn (the bar at
        the bottom and any pop-up), as large as the card allows."""
        r = read_raw(out / f"tb_{raw}.raw")
        bg = back.crop((0, SH - SHOT, SW, SH))
        bg.alpha_composite(r)
        x0, y0, x1, _ = r.getbbox()
        y0 = top if top is not None else y0
        # A pop-up: around it (the bar runs on past the sides), large.
        above = r.crop((0, 0, SW, SHOT - 110)).getbbox()
        if focus is not None:
            x0, x1 = focus
        elif above:
            cx = (above[0] + above[2]) // 2
            half = max((above[2] - above[0]) // 2 + 260, 760)
            x0, x1 = max(0, cx - half), min(SW, cx + half)
        x0, x1 = max(0, x0 - margin), min(SW, x1 + margin)
        y0 = max(0, y0 - margin)
        img = bg.crop((x0, y0, x1, SHOT))
        # Where the eye goes, for the card: the pop-up, else the apps.
        focus_x = ((above[0] + above[2]) / 2 - x0) / max(1, x1 - x0) if above and focus is None else 0.3
        cw = W - 2 * PAD
        k = min(1.0, cw / img.width)
        img = img.resize((round(img.width * k), round(img.height * k)), Image.LANCZOS)
        if img.width < cw:
            # Wider: the desktop carries on at the sides.
            wide = bg.crop((max(0, (x0 + x1) // 2 - round(cw / k) // 2), y0, min(SW, (x0 + x1) // 2 + round(cw / k) // 2), SHOT))
            img = wide.resize((cw, round(wide.height * cw / wide.width)), Image.LANCZOS)
        return img, focus_x

    def strip_card(name, img, x0=0, x1=None):
        """A card for a strip (a bar, a stack of bars): the strip as large
        as fits, on a soft, dark copy of itself."""
        from PIL import ImageEnhance, ImageFilter

        img = img.crop((x0, 0, x1 or img.width, img.height)).convert("RGB")
        cw, ch = 960, 600
        k = min(cw / img.width, ch / img.height)
        fg = img.resize((round(img.width * k), round(img.height * k)), Image.LANCZOS)
        back = ImageEnhance.Brightness(img.resize((cw, ch)).filter(ImageFilter.GaussianBlur(30))).enhance(0.5)
        back.paste(fg, ((cw - fg.width) // 2, (ch - fg.height) // 2))
        (IMAGES / "cards").mkdir(exist_ok=True)
        back.save(IMAGES / "cards" / f"{name}.jpg", quality=86, optimize=True, progressive=True)

    def card(name, raw, back, span=None):
        """The website card's picture (16:10, see make_cards.py), from the
        whole render: wide enough for the pop-up and the bar under it (or
        `span`, pixels across, for a bar alone), at the screen's bottom."""
        r = read_raw(out / f"tb_{raw}.raw")
        bg = back.crop((0, SH - SHOT, SW, SH))
        bg.alpha_composite(r)
        above = r.crop((0, 0, SW, SHOT - 110)).getbbox()
        if span is None and above:
            l, t, rr, _ = above
            w = max(rr - l + 160, round((SHOT - t + 80) * 1.6), 900)
            cx = (l + rr) // 2
        else:
            l, rr = span or (150, 1250)
            w, cx = rr - l, (l + rr) // 2
        h = min(SHOT, round(w / 1.6))
        w = round(h * 1.6)
        x0 = min(max(0, cx - w // 2), SW - w)
        img = bg.crop((x0, SHOT - h, x0 + w, SHOT))
        (IMAGES / "cards").mkdir(exist_ok=True)
        img.convert("RGB").resize((960, 600), Image.LANCZOS).save(IMAGES / "cards" / f"{name}.jpg", quality=86, optimize=True, progressive=True)

    def single(name, raw, title, sub, chips, back=dark, span=None, strip=None, **kw):
        img, _ = scene(raw, back, **kw)
        page(img, title, sub, chips).save(IMAGES / f"{name}.png")
        if strip:
            # The part of the bar that matters, from the picture itself.
            strip_card(name, img, round(img.width * strip[0]), round(img.width * strip[1]))
        else:
            card(name, raw, back, span)
        print("  ", name)

    single("taskbar", "bar", "Nookly Taskbar", "Your apps and live widgets on one bar, in the notch's shape. It grows and shrinks to fit.", ["Pins first", "Running dots", "32 widgets", "Like Windows on the right"], top=SHOT - 260, strip=(0.03, 0.55))
    single("taskbar-windows", "windows", "Every window at a glance", "Rest on an app: a live picture of each of its windows. Rest on one to see it on the screen.", ["Live previews", "Peek", "Close from here", "Minimized ones too"])
    single("taskbar-menu", "menu", "Right-click for more", "Recent files and the app's own tasks, then the usual: new window, pin, close, end task.", ["Recent files", "App tasks", "Pin", "Close all"])
    single("taskbar-more", "more", "Apps that don't fit", "The rest go into the last tile, with each one's windows a hover away.", ["Any number of apps", "Drag in and out", "Windows bubble"])
    single("taskbar-group", "group", "Collections", "Drag apps onto each other to keep them together in one tile.", ["Name it", "Open from the grid", "Undo anytime"])
    single("taskbar-tray", "tray", "Hidden icons", "Every app's tray icon, in Nookly's own tray; left and right clicks work as usual.", ["Real tray icons", "Menus work", "Names on hover"])
    if (out / "tb_widget.raw").exists():
        single("taskbar-widget", "widget", "Widgets open up", "Click a widget for its panel, right on the taskbar: the weather, your music, the calendar.", ["Full views", "Type in them", "Same as the notch"])
    single("taskbar-marks", "marks", "Progress, badges and attention", "Downloads and copies fill a bar, apps' own badges show, and an app that needs you glows.", ["Progress", "Paused and error", "Badges", "Win+Alt+J keyboard ring"], top=SHOT - 230, focus=(150, 1150), strip=(0.0, 1.0))
    single("taskbar-other", "other", "On every screen", "The other screens get a bar too, with what's used there: the apps and your widgets.", ["One process", "Your pick per screen", "Plug and unplug"], top=SHOT - 260, strip=(0.03, 0.6))

    # Themes: the bar in four looks, one under the other.
    rows = [("theme_paper", "Paper", dark), ("theme_ocean", "Ocean", dark), ("theme_glass", "Liquid Glass", dark), ("theme_glass_light", "Liquid Glass, light", light)]
    cw = W - 2 * PAD
    tiles = []
    for raw, label, back in rows:
        t, _ = scene(raw, back, top=SHOT - 190)
        d = ImageDraw.Draw(t)
        f = font(22, "semibold")
        tw = d.textlength(label, font=f)
        d.rounded_rectangle((16, 14, 16 + tw + 30, 52), 19, fill=(0, 0, 0, 150))
        d.text((31, 33), label, font=f, fill=TEXT, anchor="lm")
        tiles.append(t)
    gap = 18
    stack = Image.new("RGBA", (cw, sum(t.height for t in tiles) + gap * (len(tiles) - 1)), BG + (255,))
    y = 0
    for t in tiles:
        stack.paste(t, (0, y), rounded_mask(t.width, t.height, 20))
        y += t.height + gap
    strip_card("taskbar-themes", stack, 0, round(stack.width * 0.55))
    page(stack, "Its own look", "The notch's themes, Liquid Glass included, or one of the taskbar's own.", ["Follows the notch", "Or its own theme", "Light and dark glass"]).save(IMAGES / "taskbar-themes.png")
    print("   taskbar-themes")
