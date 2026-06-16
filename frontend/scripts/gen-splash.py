from PIL import Image, ImageDraw, ImageFont

BG=(11,11,12); MARK=(237,237,235); WORD=(150,150,160)
CJK="/usr/share/fonts/opentype/noto/NotoSerifCJK-Bold.ttc"

# (cssW, cssH, dpr) portrait — current iPhone family
DEVICES=[
 (320,568,2),(375,667,2),(375,812,3),(390,844,3),(393,852,3),(402,874,3),
 (414,736,3),(414,896,2),(414,896,3),(428,926,3),(430,932,3),(440,956,3),
]
links=[]
for cssW,cssH,dpr in DEVICES:
    pw,ph=cssW*dpr,cssH*dpr
    img=Image.new("RGB",(pw,ph),BG); d=ImageDraw.Draw(img)
    mark_sz=int(pw*0.20); word_sz=int(pw*0.052)
    fmark=ImageFont.truetype(CJK,mark_sz,index=0)
    fword=ImageFont.truetype(CJK,word_sz,index=0)
    # 古古
    t="古古"
    b=d.textbbox((0,0),t,font=fmark); tw,th=b[2]-b[0],b[3]-b[1]
    mx=(pw-tw)/2-b[0]; my=ph/2-th-int(pw*0.03)-b[1]
    d.text((mx,my),t,font=fmark,fill=MARK)
    # Kogu
    w="Kogu"
    b2=d.textbbox((0,0),w,font=fword); ww=b2[2]-b2[0]
    d.text(((pw-ww)/2-b2[0], ph/2+int(pw*0.02)),w,font=fword,fill=WORD)
    fn=f"public/splash/apple-splash-{pw}x{ph}.png"
    img.save(fn)
    media=(f"(device-width: {cssW}px) and (device-height: {cssH}px) and "
           f"(-webkit-device-pixel-ratio: {dpr}) and (orientation: portrait)")
    links.append(f'    <link rel="apple-touch-startup-image" media="{media}" href="/splash/apple-splash-{pw}x{ph}.png" />')
print("\n".join(links))
