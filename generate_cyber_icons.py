import os
from PIL import Image, ImageDraw, ImageFilter

def create_clean_arrow_icon(size):
    scale = 4
    canvas_size = size * scale
    img = Image.new("RGBA", (canvas_size, canvas_size), (0, 0, 0, 0))
    
    bg_margin = int(4 * scale)
    corner_rad = int(14 * scale * (size / 128))
    if corner_rad < 2:
        corner_rad = 2
        
    bg_img = Image.new("RGBA", (canvas_size, canvas_size), (0, 0, 0, 0))
    draw_bg = ImageDraw.Draw(bg_img)
    
    # Obsidian dark background with rounded corners
    draw_bg.rounded_rectangle(
        [bg_margin, bg_margin, canvas_size - bg_margin, canvas_size - bg_margin],
        radius=corner_rad,
        fill=(9, 14, 23, 255),
        outline=(0, 245, 255, 240),
        width=max(1, int(2.5 * scale * (size / 128)))
    )
    
    # Outer subtle glow
    glow_layer = Image.new("RGBA", (canvas_size, canvas_size), (0, 0, 0, 0))
    draw_glow = ImageDraw.Draw(glow_layer)
    draw_glow.rounded_rectangle(
        [bg_margin, bg_margin, canvas_size - bg_margin, canvas_size - bg_margin],
        radius=corner_rad,
        outline=(0, 245, 255, 140),
        width=max(2, int(3.5 * scale * (size / 128)))
    )
    glow_blur = glow_layer.filter(ImageFilter.GaussianBlur(radius=max(1, int(2 * scale * (size / 128)))))
    
    # Draw Arrow & Tray
    arrow_img = Image.new("RGBA", (canvas_size, canvas_size), (0, 0, 0, 0))
    draw_arrow = ImageDraw.Draw(arrow_img)
    
    cx = canvas_size / 2.0
    cy = canvas_size / 2.0
    w = canvas_size * 0.45
    
    # Vertical line of arrow
    top_y = cy - w * 0.5
    bottom_y = cy + w * 0.2
    stroke_w = max(2, int(5 * scale * (size / 128)))
    
    draw_arrow.line([(cx, top_y), (cx, bottom_y)], fill=(0, 245, 255, 255), width=stroke_w)
    
    # Arrowhead
    head_size = w * 0.4
    draw_arrow.line([(cx, bottom_y), (cx - head_size, bottom_y - head_size)], fill=(0, 245, 255, 255), width=stroke_w)
    draw_arrow.line([(cx, bottom_y), (cx + head_size, bottom_y - head_size)], fill=(0, 245, 255, 255), width=stroke_w)
    
    # Horizontal bottom tray
    tray_y = cy + w * 0.55
    tray_w = w * 0.75
    draw_arrow.line([(cx - tray_w, tray_y), (cx + tray_w, tray_y)], fill=(0, 245, 255, 255), width=stroke_w)
    
    # Composite all layers
    final_img = Image.alpha_composite(img, glow_blur)
    final_img = Image.alpha_composite(final_img, bg_img)
    final_img = Image.alpha_composite(final_img, arrow_img)
    
    final_resized = final_img.resize((size, size), Image.Resampling.LANCZOS)
    return final_resized

def main():
    dest_dirs = [
        r"c:\Users\Haider Nasrat\Desktop\Desktop-SuperIDM\extension\icons",
        r"C:\Users\Haider Nasrat\Desktop\SuperIDM-Extension\icons"
    ]
    
    sizes = [16, 32, 48, 128]
    for d in dest_dirs:
        os.makedirs(d, exist_ok=True)
        for s in sizes:
            icon = create_clean_arrow_icon(s)
            out_path = os.path.join(d, f"icon{s}.png")
            icon.save(out_path, "PNG")
            print(f"Generated {out_path}")

if __name__ == "__main__":
    main()
