from PIL import Image, ImageDraw, ImageFont

BG = (11, 11, 12)        # --bg #0b0b0c (the branded splash background)
MARK = (237, 237, 235)   # --text #ededeb
CJK = "/usr/share/fonts/opentype/noto/NotoSerifCJK-Bold.ttc"

# (cssW, cssH, dpr) PORTRAIT base — current iPhone family
DEVICES = [
    (320, 568, 2), (375, 667, 2), (375, 812, 3), (390, 844, 3), (393, 852, 3), (402, 874, 3),
    (414, 736, 3), (414, 896, 2), (414, 896, 3), (428, 926, 3), (430, 932, 3), (440, 956, 3),
]


def draw(pw, ph, path):
    img = Image.new("RGB", (pw, ph), BG)
    d = ImageDraw.Draw(img)
    # 古古 brand mark, centred. No progress bar (a static bar can't animate and reads as "stuck").
    f = ImageFont.truetype(CJK, int(min(pw, ph) * 0.20), index=0)
    t = "古古"
    b = d.textbbox((0, 0), t, font=f)
    d.text(((pw - (b[2] - b[0])) / 2 - b[0], (ph - (b[3] - b[1])) / 2 - b[1]), t, font=f, fill=MARK)
    img.save(path)


links = []
for cssW, cssH, dpr in DEVICES:
    pw, ph = cssW * dpr, cssH * dpr
    # portrait
    draw(pw, ph, f"public/splash/apple-splash-{pw}x{ph}.png")
    links.append(
        f'    <link rel="apple-touch-startup-image" href="/splash/apple-splash-{pw}x{ph}.png" '
        f'media="(device-width: {cssW}px) and (device-height: {cssH}px) and '
        f'(-webkit-device-pixel-ratio: {dpr}) and (orientation: portrait)" />')
    # landscape (swap dims + orientation) — iOS may check landscape at the launch instant
    draw(ph, pw, f"public/splash/apple-splash-{ph}x{pw}.png")
    links.append(
        f'    <link rel="apple-touch-startup-image" href="/splash/apple-splash-{ph}x{pw}.png" '
        f'media="(device-width: {cssW}px) and (device-height: {cssH}px) and '
        f'(-webkit-device-pixel-ratio: {dpr}) and (orientation: landscape)" />')
print("\n".join(links))
