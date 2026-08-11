#!/usr/bin/env python3
"""生成 Tauri 需要的应用图标，只用标准库。

这台机器没有 Pillow，而 PNG 和 ICO 的容器格式都足够简单：
PNG 是 zlib 压缩的扫描线加 CRC 分块，ICO 就是若干 PNG 加一张目录表。

图案：深色圆角底 + 左侧胶片孔（视频）+ 右侧声波（音频），呼应"视频转音频"。
这是占位图，等有正式品牌资源再换。
"""
import struct
import zlib
import pathlib

OUT = pathlib.Path(__file__).resolve().parent.parent / "icons"

BG = (24, 28, 38, 255)  # 深灰蓝底
ACCENT = (94, 176, 255, 255)  # 声波的亮蓝
FILM = (240, 243, 250, 255)  # 胶片孔的近白


def blend(dst, src):
    """src over dst，直接算 alpha 合成。"""
    sa = src[3] / 255
    if sa >= 1:
        return src
    if sa <= 0:
        return dst
    return tuple(round(src[i] * sa + dst[i] * (1 - sa)) for i in range(3)) + (255,)


def coverage(px, py, inside, samples=3):
    """3x3 超采样求覆盖率，让边缘不锯齿。"""
    hit = 0
    step = 1 / (samples + 1)
    for sy in range(1, samples + 1):
        for sx in range(1, samples + 1):
            if inside(px + sx * step, py + sy * step):
                hit += 1
    return hit / (samples * samples)


def draw(size):
    """画一张 size×size 的 RGBA 图，返回逐行的像素列表。"""
    s = size
    canvas = [[(0, 0, 0, 0)] * s for _ in range(s)]

    radius = s * 0.22
    def in_panel(x, y):
        # 圆角矩形：把点夹到内矩形上再比距离。
        cx = min(max(x, radius), s - radius)
        cy = min(max(y, radius), s - radius)
        return (x - cx) ** 2 + (y - cy) ** 2 <= radius * radius

    # 胶片孔：左侧一列 4 个小圆角方块。
    hole_w, hole_h = s * 0.085, s * 0.075
    hole_x = s * 0.17
    holes = [s * (0.235 + i * 0.177) for i in range(4)]

    def in_holes(x, y):
        for hy in holes:
            if abs(x - hole_x) <= hole_w / 2 and abs(y - hy) <= hole_h / 2:
                return True
        return False

    # 声波：右侧 4 根高度不一的圆头竖条，中间高两侧低。
    bars = [
        (s * 0.44, 0.30),
        (s * 0.565, 0.46),
        (s * 0.69, 0.62),
        (s * 0.815, 0.38),
    ]
    bar_w = s * 0.072

    def in_bars(x, y):
        for bx, height in bars:
            half = s * height / 2
            if abs(x - bx) <= bar_w / 2:
                dy = abs(y - s / 2)
                if dy <= half - bar_w / 2:
                    return True
                # 圆头
                cap = s / 2 + (half - bar_w / 2) * (1 if y > s / 2 else -1)
                if (x - bx) ** 2 + (y - cap) ** 2 <= (bar_w / 2) ** 2:
                    return True
        return False

    for y in range(s):
        for x in range(s):
            a = coverage(x, y, in_panel)
            if a <= 0:
                continue
            pixel = BG[:3] + (round(BG[3] * a),)
            for inside, color in ((in_holes, FILM), (in_bars, ACCENT)):
                c = coverage(x, y, inside)
                if c > 0:
                    pixel = blend(pixel, color[:3] + (round(color[3] * c),))
            canvas[y][x] = pixel
    return canvas


def png_bytes(canvas):
    s = len(canvas)
    raw = bytearray()
    for row in canvas:
        raw.append(0)  # filter type 0
        for pixel in row:
            raw.extend(pixel)

    def chunk(tag, data):
        body = tag + data
        return struct.pack(">I", len(data)) + body + struct.pack(">I", zlib.crc32(body))

    return (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", struct.pack(">IIBBBBB", s, s, 8, 6, 0, 0, 0))
        + chunk(b"IDAT", zlib.compress(bytes(raw), 9))
        + chunk(b"IEND", b"")
    )


def ico_bytes(pngs):
    """ICO = 6 字节头 + 每张 16 字节目录项 + 各 PNG 数据。"""
    header = struct.pack("<HHH", 0, 1, len(pngs))
    offset = 6 + 16 * len(pngs)
    entries, blobs = b"", b""
    for size, data in pngs:
        entries += struct.pack(
            "<BBBBHHII",
            size if size < 256 else 0,
            size if size < 256 else 0,
            0,
            0,
            1,
            32,
            len(data),
            offset,
        )
        blobs += data
        offset += len(data)
    return header + entries + blobs


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    cache = {}

    def png_for(size):
        if size not in cache:
            cache[size] = png_bytes(draw(size))
        return cache[size]

    # Tauri 默认约定的一组尺寸。
    for name, size in [
        ("32x32.png", 32),
        ("128x128.png", 128),
        ("128x128@2x.png", 256),
        ("icon.png", 512),
    ]:
        (OUT / name).write_bytes(png_for(size))
        print(f"  {name}")

    ico = ico_bytes([(s, png_for(s)) for s in (16, 32, 48, 64, 128, 256)])
    (OUT / "icon.ico").write_bytes(ico)
    print(f"  icon.ico ({len(ico)} bytes)")


if __name__ == "__main__":
    main()
