"""The pictures on the website's cards: the "screen" of each image (what
Nookly looks like on the desktop, without the title and chips under it,
which the card has in words already), cut to the card's 16:10 and small.

The full image stays for clicking: the card's <img> has it in data-full.

    python docs/tools/make_cards.py            every image in docs/images
    python docs/tools/make_cards.py a b        just these (names, no .png)
"""

import sys
from pathlib import Path

from PIL import Image

ROOT = Path(__file__).resolve().parent.parent.parent
IMAGES = ROOT / "docs" / "images"
CARDS = IMAGES / "cards"
PAD = 40
SIZE = (960, 600)


def screen(im):
    """The desktop part: from the top margin down to where the page's own
    background comes back across the whole width."""
    im = im.convert("RGB")
    w, h = im.size
    bg = im.getpixel((5, h - 5))
    plain = lambda y: all(sum(abs(a - b) for a, b in zip(im.getpixel((x, y)), bg)) < 12 for x in range(PAD + 20, w - PAD - 20, 31))
    bottom = next((y for y in range(PAD + 40, h) if plain(y)), h)
    return im.crop((PAD, PAD, w - PAD, bottom))


def content(s):
    """Left and right of Nookly in the screen: the columns where its top rows
    are mostly dark (the notch and its panel; a window there is light)."""
    g = s.convert("L")
    w, h = g.size
    rows = range(2, min(h, 60), 3)
    dark = [x for x in range(0, w, 2) if sum(g.getpixel((x, y)) < 45 for y in rows) > len(rows) * 0.7]
    if not dark:
        return 0, w
    return dark[0], dark[-1] + 1


def card(name):
    src = IMAGES / f"{name}.png"
    s = screen(Image.open(src))
    w, h = s.size
    want = SIZE[0] / SIZE[1]
    if w / h <= want:
        # Tall: from the top.
        s = s.crop((0, 0, w, round(w / want)))
    else:
        # Wide: around Nookly (in the middle at the top), never through it.
        l, r = content(s)
        cw = max(round(h * want), min(w, r - l + 80))
        cx = (l + r) // 2
        x0 = max(0, min(w - cw, cx - cw // 2))
        s = s.crop((x0, 0, x0 + cw, h))
        if cw / h > want:
            # Still wider than the card: whole, on a soft copy of itself.
            from PIL import ImageEnhance, ImageFilter

            ch = round(cw / want)
            back = ImageEnhance.Brightness(s.resize((round(ch * want), ch)).filter(ImageFilter.GaussianBlur(40))).enhance(0.45)
            back.paste(s, (0, 0))
            s = back
    CARDS.mkdir(exist_ok=True)
    s.resize(SIZE, Image.LANCZOS).save(CARDS / f"{name}.jpg", quality=86, optimize=True, progressive=True)


def used():
    """The images the pages show on cards."""
    import re

    names = []
    for page in ("index.html", "taskbar.html"):
        text = (ROOT / "docs" / page).read_text(encoding="utf-8")
        names += re.findall(r'class="card"><img src="images/cards/([\w-]+)\.jpg"', text)
        names += re.findall(r'class="card"><img src="images/([\w-]+)\.png"', text)
    # (The taskbar's are made with its pictures: make_taskbar.py.)
    return sorted(n for n in set(names) if not n.startswith("taskbar"))


if __name__ == "__main__":
    for n in sys.argv[1:] or used():
        card(n)
        print("  card", n)
