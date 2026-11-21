#!/usr/bin/env python3
"""从单一源图生成各平台应用图标。

用法:
    python3 scripts/gen_icons.py [源图路径]

产出:
    assets/icon/png/                    各种尺寸 PNG (Linux / 通用)
    assets/icon/AppIcon.icns            macOS 图标（画面铺满画布，无留白）
    assets/icon/AppIcon-padded.icns     macOS 图标（按苹果图标网格留 100px 边距）
    assets/icon/icon.ico                Windows 图标

依赖: macOS 自带的 sips / iconutil（无第三方包）
"""

from __future__ import annotations

import os
import struct
import subprocess
import sys
import zlib

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
ICON_DIR = os.path.join(ROOT, "assets", "icon")
SRC = sys.argv[1] if len(sys.argv) > 1 else os.path.join(ICON_DIR, "icon.png")

# 需要导出的 PNG 尺寸
PNG_SIZES = [16, 24, 32, 48, 64, 128, 256, 512, 1024]

# macOS .iconset 所需文件 -> 像素尺寸
ICONSET = {
    "icon_16x16.png": 16,
    "icon_16x16@2x.png": 32,
    "icon_32x32.png": 32,
    "icon_32x32@2x.png": 64,
    "icon_128x128.png": 128,
    "icon_128x128@2x.png": 256,
    "icon_256x256.png": 256,
    "icon_256x256@2x.png": 512,
    "icon_512x512.png": 512,
    "icon_512x512@2x.png": 1024,
}

# Windows .ico 内含尺寸
ICO_SIZES = [16, 24, 32, 48, 64, 128, 256]

MASTER = 1024

# macOS 图标网格：1024 画布中图标主体占 824，四周各留 100 的透明边距。
MACOS_CANVAS = 1024
MACOS_CONTENT = 824

# 中间文件目录
WORK_DIR = os.path.join(ROOT, ".iconwork")


def run(*args: str) -> None:
    subprocess.run(args, check=True, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)


def resize(src: str, dst: str, size: int) -> None:
    """用 sips 高质量重采样。"""
    run("sips", "-z", str(size), str(size), src, "--out", dst)


def build_pngs(master: str, out_dir: str) -> dict[int, str]:
    """从 1024 母版向下缩放，避免逐级累积误差。"""
    os.makedirs(out_dir, exist_ok=True)
    paths: dict[int, str] = {}
    for size in PNG_SIZES:
        path = os.path.join(out_dir, f"icon_{size}x{size}.png")
        if size == MASTER:
            run("sips", "-z", str(MASTER), str(MASTER), master, "--out", path)
        else:
            resize(master, path, size)
        paths[size] = path
        print(f"  PNG {size:>5}x{size:<5} -> {os.path.relpath(path, ROOT)}")
    return paths


def build_icns(pngs: dict[int, str], out_dir: str, stem: str = "AppIcon") -> str:
    iconset = os.path.join(out_dir, f"{stem}.iconset")
    os.makedirs(iconset, exist_ok=True)
    for name, size in ICONSET.items():
        run("sips", "-z", str(size), str(size), pngs[MASTER],
            "--out", os.path.join(iconset, name))
    icns = os.path.join(out_dir, f"{stem}.icns")
    run("iconutil", "-c", "icns", iconset, "-o", icns)
    print(f"  ICNS -> {os.path.relpath(icns, ROOT)}")
    return iconset


# --- 极简 PNG 读写（仅 8 位 RGBA），用于把画面缩进留白画布 ----------------

def read_png(path: str) -> tuple[int, int, bytes]:
    """读出 8 位/通道的真彩色 PNG，返回 (宽, 高, RGBA 字节)。"""
    with open(path, "rb") as f:
        data = f.read()
    if data[:8] != b"\x89PNG\r\n\x1a\n":
        raise ValueError(f"不是 PNG: {path}")

    idat = b""
    width = height = 0
    pos = 8
    while pos < len(data):
        length, ctype = struct.unpack(">I4s", data[pos:pos + 8])
        body = data[pos + 8:pos + 8 + length]
        if ctype == b"IHDR":
            width, height, depth, color, _, _, _ = struct.unpack(">IIBBBBB", body)
            if depth != 8 or color != 6:
                raise ValueError(f"只支持 8 位 RGBA，得到 depth={depth} color={color}")
        elif ctype == b"IDAT":
            idat += body
        elif ctype == b"IEND":
            break
        pos += 12 + length

    raw = zlib.decompress(idat)
    stride = width * 4
    out = bytearray(stride * height)
    prev = bytearray(stride)
    off = 0
    for y in range(height):
        ftype = raw[off]
        off += 1
        line = bytearray(raw[off:off + stride])
        off += stride
        # 逐字节还原 PNG 的行滤波
        for i in range(stride):
            a = line[i - 4] if i >= 4 else 0
            b = prev[i]
            c = prev[i - 4] if i >= 4 else 0
            if ftype == 1:
                line[i] = (line[i] + a) & 0xFF
            elif ftype == 2:
                line[i] = (line[i] + b) & 0xFF
            elif ftype == 3:
                line[i] = (line[i] + ((a + b) >> 1)) & 0xFF
            elif ftype == 4:
                p = a + b - c
                pa, pb, pc = abs(p - a), abs(p - b), abs(p - c)
                pred = a if pa <= pb and pa <= pc else (b if pb <= pc else c)
                line[i] = (line[i] + pred) & 0xFF
        out[y * stride:(y + 1) * stride] = line
        prev = line
    return width, height, bytes(out)


def write_png(path: str, width: int, height: int, rgba: bytes) -> None:
    stride = width * 4
    raw = b"".join(
        b"\x00" + rgba[y * stride:(y + 1) * stride] for y in range(height)
    )

    def chunk(tag: bytes, body: bytes) -> bytes:
        return (
            struct.pack(">I", len(body))
            + tag
            + body
            + struct.pack(">I", zlib.crc32(tag + body) & 0xFFFFFFFF)
        )

    with open(path, "wb") as f:
        f.write(b"\x89PNG\r\n\x1a\n")
        f.write(chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0)))
        f.write(chunk(b"IDAT", zlib.compress(raw, 9)))
        f.write(chunk(b"IEND", b""))


def pad_centered(src: str, dst: str, canvas: int, content: int) -> None:
    """把 `src` 的内容居中放进 `canvas` 见方的透明画布，内容占 `content` 见方。"""
    tmp = os.path.join(WORK_DIR, "_content.png")
    resize(src, tmp, content)
    cw, ch, pixels = read_png(tmp)

    out = bytearray(canvas * canvas * 4)
    left = (canvas - cw) // 2
    top = (canvas - ch) // 2
    for y in range(ch):
        s = y * cw * 4
        d = ((top + y) * canvas + left) * 4
        out[d:d + cw * 4] = pixels[s:s + cw * 4]
    write_png(dst, canvas, canvas, bytes(out))


def build_ico(pngs: dict[int, str], out_path: str) -> None:
    """手写 ICO 容器：Vista+ 支持在 ICO 内直接存放 PNG 数据。"""
    entries: list[tuple[int, bytes]] = []
    for size in ICO_SIZES:
        with open(pngs[size], "rb") as f:
            entries.append((size, f.read()))

    header = struct.pack("<HHH", 0, 1, len(entries))  # reserved, type=icon, count
    offset = len(header) + 16 * len(entries)
    directory = b""
    payload = b""
    for size, data in entries:
        dim = 0 if size >= 256 else size  # 256 在 ICO 目录中记作 0
        directory += struct.pack(
            "<BBBBHHII", dim, dim, 0, 0, 1, 32, len(data), offset
        )
        payload += data
        offset += len(data)

    with open(out_path, "wb") as f:
        f.write(header + directory + payload)
    print(f"  ICO  ({len(entries)} 尺寸) -> {os.path.relpath(out_path, ROOT)}")


def main() -> None:
    if not os.path.exists(SRC):
        sys.exit(f"找不到源图: {SRC}")

    os.makedirs(WORK_DIR, exist_ok=True)
    print(f"源图: {os.path.relpath(SRC, ROOT)}")
    master = os.path.join(WORK_DIR, "_master_1024.png")
    run("sips", "-z", str(MASTER), str(MASTER), SRC, "--out", master)

    print("生成 PNG:")
    pngs = build_pngs(master, os.path.join(ICON_DIR, "png"))

    print("生成 macOS .icns（无留白）:")
    build_icns(pngs, ICON_DIR, stem="AppIcon")

    print("生成 macOS .icns（标准留白 824/1024）:")
    padded_master = os.path.join(WORK_DIR, "_padded_1024.png")
    pad_centered(master, padded_master, MACOS_CANVAS, MACOS_CONTENT)
    padded = dict(pngs)
    padded[MASTER] = padded_master
    build_icns(padded, ICON_DIR, stem="AppIcon-padded")

    print("生成 Windows .ico:")
    build_ico(pngs, os.path.join(ICON_DIR, "icon.ico"))

    print("完成。")


if __name__ == "__main__":
    main()
