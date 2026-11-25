"""Draws the application icon (three metro lines forking from one trunk) as a 1024x1024 PNG.

Usage: python3 scripts/make-icon.py <output.png>
Then:  bunx tauri icon <output.png>   (generates every size into src-tauri/icons)
"""

import sys

from PIL import Image, ImageDraw

SIZE = 1024
SCALE = 4  # draw big, then shrink for smooth edges
S = SIZE * SCALE

BACKGROUND = ((31, 41, 55), (15, 23, 42))  # top, bottom
LINES = {"center": "#ffa63d", "right": "#3fd0e0", "left": "#c178e0"}


def lerp(a, b, t):
    return tuple(round(x + (y - x) * t) for x, y in zip(a, b))


def rounded_path(points, radius, steps=24):
    """Polyline through `points` whose corners are rounded with quadratic curves."""
    out = [points[0]]
    for i in range(1, len(points) - 1):
        (x0, y0), (x1, y1), (x2, y2) = points[i - 1], points[i], points[i + 1]

        def toward(ax, ay, bx, by, r):
            dx, dy = bx - ax, by - ay
            length = (dx * dx + dy * dy) ** 0.5
            r = min(r, length / 2)
            return ax + dx / length * r, ay + dy / length * r

        start = toward(x1, y1, x0, y0, radius)
        end = toward(x1, y1, x2, y2, radius)
        for k in range(steps + 1):
            t = k / steps
            x = (1 - t) ** 2 * start[0] + 2 * (1 - t) * t * x1 + t**2 * end[0]
            y = (1 - t) ** 2 * start[1] + 2 * (1 - t) * t * y1 + t**2 * end[1]
            out.append((x, y))
    out.append(points[-1])
    return out


def stroke(draw, path, color, width):
    """Strokes a polyline by stamping discs along it: smooth caps and corners at any angle."""
    r = width / 2
    for (x0, y0), (x1, y1) in zip(path, path[1:]):
        steps = max(1, int(((x1 - x0) ** 2 + (y1 - y0) ** 2) ** 0.5 / 2))
        for k in range(steps + 1):
            t = k / steps
            x, y = x0 + (x1 - x0) * t, y0 + (y1 - y0) * t
            draw.ellipse((x - r, y - r, x + r, y + r), fill=color)


def station(draw, x, y, color, radius, ring):
    draw.ellipse((x - radius, y - radius, x + radius, y + radius), fill=color)
    inner = radius - ring
    draw.ellipse((x - inner, y - inner, x + inner, y + inner), fill=(250, 250, 250))


def main(output):
    image = Image.new("RGBA", (S, S), (0, 0, 0, 0))

    # Background: rounded square with a vertical gradient, inside the macOS icon margin.
    margin = int(S * 0.098)
    box = (margin, margin, S - margin, S - margin)
    gradient = Image.new("RGBA", (S, S))
    gd = ImageDraw.Draw(gradient)
    for y in range(S):
        gd.line((0, y, S, y), fill=lerp(BACKGROUND[0], BACKGROUND[1], y / S) + (255,))
    mask = Image.new("L", (S, S), 0)
    ImageDraw.Draw(mask).rounded_rectangle(box, radius=int(S * 0.2), fill=255)
    image.paste(gradient, (0, 0), mask)

    draw = ImageDraw.Draw(image)
    u = S / 1024  # design units are 1024-based
    w = int(64 * u)
    corner = 70 * u
    P = lambda x, y: (x * u, y * u)  # noqa: E731

    top, bottom = 250, 774
    cx = 512
    # Trunk in the middle, with a branch to each side leaving and rejoining it.
    stroke(draw, [P(cx, top), P(cx, bottom)], LINES["center"], w)
    right = [P(cx, 350), P(700, 350), P(700, 670), P(cx, 670)]
    left = [P(cx, 430), P(324, 430), P(324, 590), P(cx, 590)]
    stroke(draw, rounded_path(right, corner), LINES["right"], w)
    stroke(draw, rounded_path(left, corner), LINES["left"], w)

    ring = int(22 * u)
    r = int(52 * u)
    station(draw, *P(cx, top), LINES["center"], r, ring)
    station(draw, *P(cx, bottom), LINES["center"], r, ring)
    station(draw, *P(700, 510), LINES["right"], r, ring)
    station(draw, *P(324, 510), LINES["left"], r, ring)
    station(draw, *P(cx, 510), LINES["center"], r, ring)

    image.resize((SIZE, SIZE), Image.LANCZOS).save(output)


if __name__ == "__main__":
    main(sys.argv[1] if len(sys.argv) > 1 else "icon.png")
