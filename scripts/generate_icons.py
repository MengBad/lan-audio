"""Generate app icons for LAN Audio.

Design: A rounded square with a gradient background (dark teal to dark blue),
featuring a stylized audio waveform / speaker symbol in white/teal.

Outputs:
- apps/desktop/src-tauri/icons/icon.ico (Windows, multi-size)
- apps/android_flutter/android/app/src/main/res/mipmap-*/ic_launcher.png
"""

import math
import os
import sys

from PIL import Image, ImageDraw, ImageFilter

REPO_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))


def draw_icon(size: int, padding_ratio: float = 0.0) -> Image.Image:
    """Draw the LAN Audio icon at the given size."""
    img = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    draw = ImageDraw.Draw(img)

    # Background: rounded rectangle with gradient
    pad = int(size * padding_ratio)
    box = (pad, pad, size - pad, size - pad)
    corner = int(size * 0.18)

    # Draw gradient background (top-left teal to bottom-right dark)
    for y in range(pad, size - pad):
        t = (y - pad) / max(1, (size - 2 * pad))
        r = int(12 + t * 0)
        g = int(20 + (1 - t) * 15)
        b = int(30 + (1 - t) * 10)
        draw.line([(pad, y), (size - pad - 1, y)], fill=(r, g, b, 255))

    # Apply rounded corners by masking
    mask = Image.new("L", (size, size), 0)
    mask_draw = ImageDraw.Draw(mask)
    mask_draw.rounded_rectangle(box, radius=corner, fill=255)
    img.putalpha(mask)

    # Draw audio waveform bars (centered, stylized)
    cx = size // 2
    cy = size // 2
    bar_count = 5
    bar_width = max(2, int(size * 0.06))
    bar_gap = max(3, int(size * 0.09))
    total_width = bar_count * bar_width + (bar_count - 1) * bar_gap
    start_x = cx - total_width // 2

    # Heights for each bar (symmetric pattern)
    heights = [0.25, 0.55, 0.75, 0.55, 0.25]
    max_bar_h = int(size * 0.45)

    # Teal accent color
    teal = (0, 212, 170, 255)
    white = (240, 242, 247, 255)

    for i, h_ratio in enumerate(heights):
        x = start_x + i * (bar_width + bar_gap)
        bar_h = int(max_bar_h * h_ratio)
        y_top = cy - bar_h // 2
        y_bot = cy + bar_h // 2
        # Center bar is teal, others are white with slight transparency
        color = teal if i == 2 else white
        draw.rounded_rectangle(
            [x, y_top, x + bar_width, y_bot],
            radius=bar_width // 2,
            fill=color,
        )

    # Draw a small WiFi/signal arc in top-right corner
    arc_cx = cx + int(size * 0.22)
    arc_cy = cy - int(size * 0.22)
    for j in range(3):
        r = int(size * (0.06 + j * 0.045))
        arc_box = [arc_cx - r, arc_cy - r, arc_cx + r, arc_cy + r]
        alpha = 255 - j * 60
        draw.arc(arc_box, start=220, end=320, fill=(0, 212, 170, alpha), width=max(1, size // 64))

    return img


def generate_windows_ico():
    """Generate multi-size .ico for Windows."""
    sizes = [16, 24, 32, 48, 64, 128, 256]
    images = [draw_icon(s) for s in sizes]

    ico_path = os.path.join(REPO_ROOT, "apps", "desktop", "src-tauri", "icons", "icon.ico")
    os.makedirs(os.path.dirname(ico_path), exist_ok=True)
    images[0].save(ico_path, format="ICO", sizes=[(s, s) for s in sizes], append_images=images[1:])
    print(f"Generated: {ico_path}")


def generate_android_icons():
    """Generate Android mipmap icons at all densities."""
    densities = {
        "mipmap-mdpi": 48,
        "mipmap-hdpi": 72,
        "mipmap-xhdpi": 96,
        "mipmap-xxhdpi": 144,
        "mipmap-xxxhdpi": 192,
    }

    res_dir = os.path.join(
        REPO_ROOT, "apps", "android_flutter", "android", "app", "src", "main", "res"
    )

    for folder, size in densities.items():
        out_dir = os.path.join(res_dir, folder)
        os.makedirs(out_dir, exist_ok=True)
        icon = draw_icon(size)
        icon_path = os.path.join(out_dir, "ic_launcher.png")
        icon.save(icon_path, format="PNG")
        print(f"Generated: {icon_path}")


if __name__ == "__main__":
    generate_windows_ico()
    generate_android_icons()
    print("\nDone! All icons generated.")
