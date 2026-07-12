from PIL import Image, ImageDraw, ImageFont

BG = (11, 11, 12)        # --bg #0b0b0c (the branded splash background)
MARK = (237, 237, 235)   # --text #ededeb
SUB = (122, 122, 130)    # muted grey for the explainer line (secondary to the mark)
CJK = "/usr/share/fonts/opentype/noto/NotoSerifCJK-Bold.ttc"
SANS = "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf"

# One-line note below the mark, on the NATIVE launch image only. iOS holds this static
# image while it cold-boots the PWA webview — a wait that is the OS opening the app, not
# Kogu. (The #shell loader with the progress bar is Kogu's own JS mounting, so it carries
# no such disclaimer.) Saying so keeps a slow OS launch from reading as the app being slow.
SUBTITLE = "This wait is your device opening the app, not Kogu."

# (cssW, cssH, dpr) PORTRAIT base — current iPhone family
DEVICES = [
    (320, 568, 2), (375, 667, 2), (375, 812, 3), (390, 844, 3), (393, 852, 3), (402, 874, 3),
    (414, 736, 3), (414, 896, 2), (414, 896, 3), (428, 926, 3), (430, 932, 3), (440, 956, 3),
]


def draw(pw, ph, path):
    img = Image.new("RGB", (pw, ph), BG)
    d = ImageDraw.Draw(img)
    m = min(pw, ph)
    # 古古 brand mark, centred. No progress bar (a static bar can't animate and reads as "stuck").
    f = ImageFont.truetype(CJK, int(m * 0.20), index=0)
    t = "古古"
    b = d.textbbox((0, 0), t, font=f)
    mark_h = b[3] - b[1]
    d.text(((pw - (b[2] - b[0])) / 2 - b[0], (ph - mark_h) / 2 - b[1]), t, font=f, fill=MARK)

    # Explainer, centred below the mark. Wrapped to fit narrow (portrait) widths.
    sf = ImageFont.truetype(SANS, max(12, int(m * 0.028)))
    max_w = pw * 0.80
    words, lines, cur = SUBTITLE.split(), [], ""
    for w in words:
        trial = (cur + " " + w).strip()
        if d.textlength(trial, font=sf) <= max_w:
            cur = trial
        else:
            if cur:
                lines.append(cur)
            cur = w
    if cur:
        lines.append(cur)
    asc, desc = sf.getmetrics()
    lh = (asc + desc) * 1.3
    y = ph / 2 + mark_h / 2 + m * 0.045
    for ln in lines:
        d.text(((pw - d.textlength(ln, font=sf)) / 2, y), ln, font=sf, fill=SUB)
        y += lh
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
