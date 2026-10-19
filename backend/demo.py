"""Original procedural demo artwork. No third-party footage or network requests."""

import random
from pathlib import Path

from PIL import Image, ImageDraw, ImageFilter, ImageFont

FONT = str(
    Path(__file__).resolve().parent.parent / "assets" / "fonts" / "DejaVuSans.ttf"
)


def make_art(path, scene):
    w, h = 1280, 720
    im = Image.new("RGB", (w, h))
    d = ImageDraw.Draw(im)
    rng = random.Random(40 + scene)
    top, bottom = (
        ((17, 30, 46), (110, 146, 162)) if scene != 1 else ((15, 27, 30), (71, 96, 91))
    )
    for y in range(h):
        t = y / h
        d.line(
            (0, y, w, y),
            fill=tuple(int(a + (b - a) * t) for a, b in zip(top, bottom, strict=False)),
        )
    for _ in range(130):
        x, y = rng.randrange(w), rng.randrange(h // 2)
        d.ellipse((x, y, x + 1, y + 1), fill=(155, 176, 181))
    d.ellipse((1030, 90, 1120, 180), fill=(192, 201, 184))
    for _layer, base, col in [
        (0, 480, (57, 77, 81)),
        (1, 550, (39, 54, 56)),
        (2, 650, (24, 35, 37)),
    ]:
        points = (
            [(0, h)]
            + [(x, base + rng.randint(-55, 25)) for x in range(-20, w + 80, 75)]
            + [(w, h)]
        )
        d.polygon(points, fill=col)
    if scene == 1:
        for y in range(300, h, 70):
            d.line((0, y, w, y - 80), fill=(38, 51, 50), width=2)
        for x in range(0, w, 80):
            d.line((x, 260, x, h), fill=(38, 51, 50), width=2)
        # Workshop blueprint with physical components.
        d.rounded_rectangle(
            (180, 320, 1020, 450),
            radius=50,
            fill=(189, 198, 183),
            outline=(235, 238, 221),
            width=3,
        )
        for x in [300, 540, 800]:
            d.line((x, 323, x, 447), fill=(82, 102, 99), width=3)
        d.polygon([(1020, 320), (1140, 385), (1020, 450)], fill=(217, 105, 59))
        d.polygon([(200, 320), (120, 260), (330, 320)], fill=(207, 108, 61))
        d.polygon([(200, 450), (120, 510), (330, 450)], fill=(207, 108, 61))
        for x, y, label in [
            (470, 245, "AIRFRAME / 98 mm"),
            (690, 550, "FLIGHT COMPUTER"),
            (190, 610, "BUILD 001"),
        ]:
            d.text(
                (x, y), label, fill=(160, 200, 179), font=ImageFont.truetype(FONT, 18)
            )
            d.line((x, y - 10, x + 120, y - 10), fill=(93, 133, 119), width=1)
    else:
        rx = 680 if scene == 0 else 730
        ry = 185 if scene == 0 else 80
        if scene == 2:
            d.polygon(
                [
                    (rx - 16, ry + 160),
                    (rx + 12, ry + 160),
                    (rx - 160, 720),
                    (rx - 350, 720),
                ],
                fill=(153, 152, 127),
            )
        else:
            d.line((rx - 80, 190, rx - 80, 630), fill=(69, 83, 82), width=7)
            for y in range(230, 600, 55):
                d.line((rx - 105, y, rx - 55, y + 40), fill=(80, 94, 91), width=3)
        smoke = Image.new("RGBA", (w, h))
        s = ImageDraw.Draw(smoke)
        for i in range(80):
            t = i / 80
            x = rx - 70 * t + rng.randint(-int(t * 220 + 5), int(t * 220 + 5))
            y = ry + 170 + t * 500
            r = 10 + t * 120
            s.ellipse(
                (x - r, y - r * 0.6, x + r, y + r * 0.6),
                fill=(190, 180, 151, int(14 + 32 * t)),
            )
        im = Image.alpha_composite(
            im.convert("RGBA"), smoke.filter(ImageFilter.GaussianBlur(12))
        )
        d = ImageDraw.Draw(im)
        d.polygon(
            [(rx - 10, ry + 165), (rx + 10, ry + 165), (rx + 2, ry + 285)],
            fill=(244, 163, 68),
        )
        d.polygon(
            [(rx - 5, ry + 170), (rx + 5, ry + 170), (rx, ry + 245)],
            fill=(255, 236, 170),
        )
        d.polygon(
            [(rx - 13, ry + 50), (rx, ry), (rx + 13, ry + 50)], fill=(221, 105, 59)
        )
        d.rectangle((rx - 13, ry + 50, rx + 13, ry + 165), fill=(221, 224, 208))
        d.rectangle((rx - 13, ry + 90, rx + 13, ry + 100), fill=(68, 88, 91))
        d.polygon(
            [(rx - 13, ry + 130), (rx - 35, ry + 177), (rx - 13, ry + 166)],
            fill=(216, 111, 64),
        )
        d.polygon(
            [(rx + 13, ry + 130), (rx + 35, ry + 177), (rx + 13, ry + 166)],
            fill=(216, 111, 64),
        )
    im.convert("RGB").save(path)
