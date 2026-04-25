"""生成 Windows 桌面应用 "纸条" 的应用图标。

设计：macOS 备忘录风格。背后一张浅米色便签作为层次，
前景一张金黄渐变便签，右上折角，便签上绘制三条仿文字横线。

输出：
    win-desktop/icons/icon.png          (512x512)
    win-desktop/icons/32x32.png
    win-desktop/icons/128x128.png
    win-desktop/icons/128x128@2x.png    (256x256)
    win-desktop/icons/icon.ico          (16/32/48/64/128/256 多尺寸)
"""

from __future__ import annotations

import os
from PIL import Image, ImageDraw, ImageFilter


ICON_DIR = os.path.join(os.path.dirname(os.path.abspath(__file__)), "win-desktop", "icons")

# 颜色配置与新 UI 一致
COLOR_BG_NOTE = (244, 239, 224, 235)        # #F4EFE0 背景便签（浅米色）
COLOR_NOTE_TOP = (244, 199, 68)             # #F4C744 主便签渐变顶部
COLOR_NOTE_BOTTOM = (232, 181, 71)          # #E8B547 主便签渐变底部
COLOR_FOLD_FRONT = (255, 242, 201)          # #FFF2C9 折角正面（更浅）
COLOR_FOLD_SHADOW = (180, 140, 40, 90)      # 折角投影
COLOR_LINE = (74, 55, 25, 150)              # 文字线（深棕半透明）
COLOR_SHADOW = (45, 42, 38, 120)            # 便签投影


# ---------------------------------------------------------------------------
# 绘制一张带圆角的纯色便签（返回 RGBA 图像）
# 调用处：make_icon 绘制背景便签图层
# ---------------------------------------------------------------------------
def draw_solid_rounded_rect(size, rect, radius, color):
    """在 size 画布上以 color 填充 rect（含圆角）。"""
    layer = Image.new("RGBA", size, (0, 0, 0, 0))
    ImageDraw.Draw(layer).rounded_rectangle(rect, radius=radius, fill=color)
    return layer


# ---------------------------------------------------------------------------
# 生成线性垂直渐变图（用于主便签填充）
# 调用处：make_icon 绘制主便签图层
# ---------------------------------------------------------------------------
def make_vertical_gradient(size, top_color, bottom_color):
    """生成 size 大小的垂直线性渐变 RGB 图。"""
    w, h = size
    grad = Image.new("RGB", size)
    pixels = grad.load()
    for y in range(h):
        t = y / max(1, h - 1)
        r = int(top_color[0] * (1 - t) + bottom_color[0] * t)
        g = int(top_color[1] * (1 - t) + bottom_color[1] * t)
        b = int(top_color[2] * (1 - t) + bottom_color[2] * t)
        for x in range(w):
            pixels[x, y] = (r, g, b)
    return grad


# ---------------------------------------------------------------------------
# 生成一个圆角矩形 alpha 蒙版
# 调用处：make_icon 把渐变裁成圆角
# ---------------------------------------------------------------------------
def make_rounded_mask(size, radius):
    """返回 size 大小的单通道蒙版（圆角矩形为 255）。"""
    mask = Image.new("L", size, 0)
    ImageDraw.Draw(mask).rounded_rectangle([(0, 0), (size[0] - 1, size[1] - 1)], radius=radius, fill=255)
    return mask


# ---------------------------------------------------------------------------
# 合成一整张指定分辨率的图标
# 调用处：main 按目标尺寸 1024 生成主图，再缩放导出
# ---------------------------------------------------------------------------
def make_icon(size=1024):
    """返回 RGBA 图像：完整的 "纸条" 图标。"""
    img = Image.new("RGBA", (size, size), (0, 0, 0, 0))

    # 外边距（保留图标安全区，约 10%）
    pad = int(size * 0.10)
    side = size - pad * 2
    radius = int(side * 0.22)

    # 背景便签（偏右下、稍微旋转）
    bg_side = int(side * 0.88)
    bg_radius = int(bg_side * 0.22)
    bg_img = draw_solid_rounded_rect((bg_side, bg_side), [(0, 0), (bg_side - 1, bg_side - 1)], bg_radius, COLOR_BG_NOTE)
    bg_img = bg_img.rotate(-8, resample=Image.BICUBIC, expand=True)
    bg_x = pad + int(side * 0.02)
    bg_y = pad + int(side * 0.08)
    img.alpha_composite(bg_img, (bg_x, bg_y))

    # 主便签阴影
    shadow = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    ImageDraw.Draw(shadow).rounded_rectangle(
        [(pad, pad + int(size * 0.018)), (pad + side, pad + int(size * 0.018) + side)],
        radius=radius,
        fill=COLOR_SHADOW,
    )
    shadow = shadow.filter(ImageFilter.GaussianBlur(radius=int(size * 0.025)))
    img = Image.alpha_composite(img, shadow)

    # 主便签（金黄渐变 + 圆角裁剪）
    gradient = make_vertical_gradient((side, side), COLOR_NOTE_TOP, COLOR_NOTE_BOTTOM).convert("RGBA")
    gradient.putalpha(make_rounded_mask((side, side), radius))
    main_layer = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    main_layer.alpha_composite(gradient, (pad, pad))
    img = Image.alpha_composite(img, main_layer)

    # 折角：右上角折起
    fold_size = int(side * 0.24)
    fx = pad + side
    fy = pad
    # 折角阴影（主便签上的三角阴影）
    fold_shadow_layer = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    ImageDraw.Draw(fold_shadow_layer).polygon(
        [(fx - fold_size, fy), (fx, fy + fold_size), (fx - fold_size, fy + fold_size)],
        fill=COLOR_FOLD_SHADOW,
    )
    fold_shadow_layer = fold_shadow_layer.filter(ImageFilter.GaussianBlur(radius=int(size * 0.006)))
    # 折角本体
    fold_layer = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    ImageDraw.Draw(fold_layer).polygon(
        [(fx - fold_size, fy), (fx, fy), (fx, fy + fold_size)],
        fill=COLOR_FOLD_FRONT,
    )
    # 裁剪折角只保留在主便签圆角区域内
    clip_mask = make_rounded_mask((side, side), radius)
    full_mask = Image.new("L", (size, size), 0)
    full_mask.paste(clip_mask, (pad, pad))
    fold_shadow_layer.putalpha(Image.eval(Image.merge("L", (fold_shadow_layer.split()[-1],)).point(lambda v: v), lambda x: x))
    sa = fold_shadow_layer.split()[-1]
    fa = fold_layer.split()[-1]
    fold_shadow_layer.putalpha(Image.composite(sa, Image.new("L", (size, size), 0), full_mask))
    fold_layer.putalpha(Image.composite(fa, Image.new("L", (size, size), 0), full_mask))
    img = Image.alpha_composite(img, fold_shadow_layer)
    img = Image.alpha_composite(img, fold_layer)

    # 文字线（3 条横线，模拟便签内容）
    lines_layer = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    line_draw = ImageDraw.Draw(lines_layer)
    line_h = max(2, int(side * 0.035))
    gap = int(side * 0.125)
    start_y = pad + int(side * 0.50)
    x1 = pad + int(side * 0.18)
    x2_full = pad + side - int(side * 0.20)
    x2_short = pad + int(side * 0.58)
    rows = [
        (x1, start_y, x2_full),
        (x1, start_y + gap, x2_full),
        (x1, start_y + gap * 2, x2_short),
    ]
    for lx1, ly, lx2 in rows:
        line_draw.rounded_rectangle([(lx1, ly), (lx2, ly + line_h)], radius=line_h // 2, fill=COLOR_LINE)
    img = Image.alpha_composite(img, lines_layer)

    return img


# ---------------------------------------------------------------------------
# 主流程：生成 1024 主图并导出各尺寸
# 调用处：脚本入口
# ---------------------------------------------------------------------------
def main():
    os.makedirs(ICON_DIR, exist_ok=True)
    master = make_icon(1024)

    # 主 PNG（512）
    master.resize((512, 512), Image.LANCZOS).save(os.path.join(ICON_DIR, "icon.png"))

    # Tauri 打包所需尺寸
    master.resize((32, 32), Image.LANCZOS).save(os.path.join(ICON_DIR, "32x32.png"))
    master.resize((128, 128), Image.LANCZOS).save(os.path.join(ICON_DIR, "128x128.png"))
    master.resize((256, 256), Image.LANCZOS).save(os.path.join(ICON_DIR, "128x128@2x.png"))

    # Windows ICO 多尺寸：由 PIL 从 master 自动生成
    ico_sizes = [(16, 16), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)]
    master.save(os.path.join(ICON_DIR, "icon.ico"), format="ICO", sizes=ico_sizes)

    print("图标已生成到:", ICON_DIR)


if __name__ == "__main__":
    main()
