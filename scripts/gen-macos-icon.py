# 生成万象小助手 macOS 图标源：brand/icon-macos.svg（.icns 用）与 app/icons/Wanxiang.icon/Assets/*.svg（Icon Composer 分层，Tahoe 用）。
# 用法：在仓库根目录运行 python3 scripts/gen-macos-icon.py，然后 npm run icons。
import os
ROOT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..")
os.chdir(ROOT)
import math
def squircle(cx, cy, half, n=5.0, steps=720):
    pts=[]
    for i in range(steps):
        t=2*math.pi*i/steps
        c,s=math.cos(t),math.sin(t)
        x=half*math.copysign(abs(c)**(2/n),c); y=half*math.copysign(abs(s)**(2/n),s)
        pts.append(f"{cx+x:.2f} {cy+y:.2f}")
    return "M"+" L".join(pts)+"Z"


def continuous(left, top, size, r):
    R=left+size; B=top+size
    def P(x,y): return f"{x:.2f} {y:.2f}"
    k=[1.52866483,1.08849323,0.86840689,0.63149399,0.07491100,0.37282392,0.16905899]
    a,b,c,d,e,f,g=[v*r for v in k]
    p=[f"M{P(left+a,top)}", f"L{P(R-a,top)}",
       f"C{P(R-b,top)} {P(R-c,top)} {P(R-d,top+e)}",
       f"C{P(R-f,top+g)} {P(R-g,top+f)} {P(R-e,top+d)}",
       f"C{P(R,top+c)} {P(R,top+b)} {P(R,top+a)}",
       f"L{P(R,B-a)}",
       f"C{P(R,B-b)} {P(R,B-c)} {P(R-e,B-d)}",
       f"C{P(R-g,B-f)} {P(R-f,B-g)} {P(R-d,B-e)}",
       f"C{P(R-c,B)} {P(R-b,B)} {P(R-a,B)}",
       f"L{P(left+a,B)}",
       f"C{P(left+b,B)} {P(left+c,B)} {P(left+d,B-e)}",
       f"C{P(left+f,B-g)} {P(left+g,B-f)} {P(left+e,B-d)}",
       f"C{P(left,B-c)} {P(left,B-b)} {P(left,B-a)}",
       f"L{P(left,top+a)}",
       f"C{P(left,top+b)} {P(left,top+c)} {P(left+e,top+d)}",
       f"C{P(left+g,top+f)} {P(left+f,top+g)} {P(left+d,top+e)}",
       f"C{P(left+c,top)} {P(left+b,top)} {P(left+a,top)}Z"]
    return " ".join(p)

BODY=continuous(102,102,820,184.5)   # Apple 连续圆角（与系统图标轮廓同一套系数），比 824 模板内收 2px
S=1.62                       # 圆环整体放大到主体宽度约 77%（11-22 用户反馈图标偏小，从 1.34 放大）
T=512*(1-S)
ring_mist="M535.408 345.624C579.534 351.832 619.386 375.315 646.198 410.906C673.009 446.498 684.584 491.282 678.376 535.408C672.168 579.534 648.685 619.386 613.094 646.198C577.502 673.009 532.718 684.584 488.592 678.376"
ring_graphite="M422.96 654.464C389.462 633.535 364.409 601.473 352.202 563.908C339.994 526.344 341.413 485.679 356.208 449.056"
ring_ochre="M395.296 391.152C415.078 372.042 439.25 358.081 465.688 350.496"

def layer_ring(color_id, d):
    return f'<path d="{d}" stroke="url(#{color_id})" stroke-width="56" stroke-linecap="round" fill="none"/>'

ring_group = f'''<g transform="matrix({S} 0 0 {S} {T:.2f} {T:.2f})">
    {layer_ring("mist", ring_mist)}
    {layer_ring("graphite", ring_graphite)}
    {layer_ring("ochre", ring_ochre)}
    <circle cx="512" cy="512" r="42" fill="url(#dot)"/>
  </g>'''
ring_hi = f'''<g transform="matrix({S} 0 0 {S} {T:.2f} {T:.2f})" opacity="0.55" style="mix-blend-mode:screen">
    <path d="{ring_mist}" stroke="url(#ringHi)" stroke-width="24" stroke-linecap="round" fill="none" transform="translate(-3 -7)"/>
    <path d="{ring_graphite}" stroke="url(#ringHi)" stroke-width="24" stroke-linecap="round" fill="none" transform="translate(-3 -7)"/>
    <path d="{ring_ochre}" stroke="url(#ringHi)" stroke-width="20" stroke-linecap="round" fill="none" transform="translate(-2 -5)"/>
  </g>'''

defs = f'''<defs>
  <linearGradient id="body" x1="512" y1="102" x2="512" y2="922" gradientUnits="userSpaceOnUse">
    <stop stop-color="#FFFFFF"/><stop offset="0.55" stop-color="#F3F5F9"/><stop offset="1" stop-color="#DDE3EC"/>
  </linearGradient>
  <linearGradient id="gloss" x1="512" y1="102" x2="512" y2="560" gradientUnits="userSpaceOnUse">
    <stop stop-color="#FFFFFF" stop-opacity="0.95"/><stop offset="1" stop-color="#FFFFFF" stop-opacity="0"/>
  </linearGradient>
  <linearGradient id="well" x1="512" y1="136" x2="512" y2="888" gradientUnits="userSpaceOnUse">
    <stop stop-color="#E9EDF3"/><stop offset="1" stop-color="#FFFFFF"/>
  </linearGradient>
  <linearGradient id="mist" x1="512" y1="330" x2="560" y2="700" gradientUnits="userSpaceOnUse">
    <stop stop-color="#5A80B3"/><stop offset="1" stop-color="#3E6191"/>
  </linearGradient>
  <linearGradient id="graphite" x1="380" y1="440" x2="420" y2="660" gradientUnits="userSpaceOnUse">
    <stop stop-color="#3A3A3D"/><stop offset="1" stop-color="#141416"/>
  </linearGradient>
  <linearGradient id="ochre" x1="395" y1="350" x2="465" y2="392" gradientUnits="userSpaceOnUse">
    <stop stop-color="#C98550"/><stop offset="1" stop-color="#A9652F"/>
  </linearGradient>
  <radialGradient id="dot" cx="0" cy="0" r="1" gradientUnits="userSpaceOnUse" gradientTransform="translate(500 498) scale(56)">
    <stop stop-color="#45454A"/><stop offset="1" stop-color="#1D1D1F"/>
  </radialGradient>
  <linearGradient id="ringHi" x1="512" y1="330" x2="512" y2="700" gradientUnits="userSpaceOnUse">
    <stop stop-color="#FFFFFF" stop-opacity="0.55"/><stop offset="0.5" stop-color="#FFFFFF" stop-opacity="0"/>
  </linearGradient>
  <clipPath id="bodyClip"><path d="{BODY}"/></clipPath>
  <filter id="bodyShadow" x="40" y="60" width="944" height="944" filterUnits="userSpaceOnUse" color-interpolation-filters="sRGB">
    <feGaussianBlur in="SourceAlpha" stdDeviation="5"/><feOffset dy="8"/>
    <feColorMatrix values="0 0 0 0 0  0 0 0 0 0  0 0 0 0 0  0 0 0 0.24 0" result="near"/>
    <feGaussianBlur in="SourceAlpha" stdDeviation="22"/><feOffset dy="20"/>
    <feColorMatrix values="0 0 0 0 0.1  0 0 0 0 0.15  0 0 0 0 0.25  0 0 0 0.16 0" result="far"/>
    <feMerge><feMergeNode in="far"/><feMergeNode in="near"/><feMergeNode in="SourceGraphic"/></feMerge>
  </filter>
  <filter id="wellInner" x="110" y="110" width="804" height="820" filterUnits="userSpaceOnUse" color-interpolation-filters="sRGB">
    <feOffset in="SourceAlpha" dy="8"/><feGaussianBlur stdDeviation="10" result="o"/>
    <feComposite in="SourceAlpha" in2="o" operator="out" result="inner"/>
    <feColorMatrix in="inner" values="0 0 0 0 0.1  0 0 0 0 0.15  0 0 0 0 0.25  0 0 0 0.16 0" result="shade"/>
    <feMerge><feMergeNode in="SourceGraphic"/><feMergeNode in="shade"/></feMerge>
  </filter>
  <filter id="ringShadow" x="100" y="100" width="824" height="840" filterUnits="userSpaceOnUse" color-interpolation-filters="sRGB">
    <feGaussianBlur in="SourceAlpha" stdDeviation="11"/><feOffset dy="12"/>
    <feColorMatrix values="0 0 0 0 0.1  0 0 0 0 0.15  0 0 0 0 0.25  0 0 0 0.26 0" result="s"/>
    <feMerge><feMergeNode in="s"/><feMergeNode in="SourceGraphic"/></feMerge>
  </filter>
</defs>'''

# legacy icns：完整一张图（含投影，旧系统按模板显示）
legacy = f'''<svg width="1024" height="1024" viewBox="0 0 1024 1024" fill="none" xmlns="http://www.w3.org/2000/svg">
<!-- 万象小助手 macOS 图标（.icns 来源，macOS 15 及更早）。主体为 Apple 连续圆角（半径 184.5），820 宽，居中，不超出系统图标轮廓。
     圆环放大 1.62 倍，外径约占主体 77%；只用石墨、雾蓝、赭色三色及其明暗。 -->
{defs}
<g filter="url(#bodyShadow)"><path d="{BODY}" fill="url(#body)"/></g>
<g clip-path="url(#bodyClip)">
  <rect x="100" y="100" width="824" height="460" fill="url(#gloss)" opacity="0.55"/>
  <circle cx="512" cy="512" r="376" fill="url(#well)" filter="url(#wellInner)"/>
  <circle cx="512" cy="512" r="375" stroke="#FFFFFF" stroke-opacity="0.9" stroke-width="2"/>
</g>
<path d="{BODY}" stroke="#FFFFFF" stroke-opacity="0.9" stroke-width="3" fill="none"/>
<g filter="url(#ringShadow)">
  {ring_group}
</g>
{ring_hi}
</svg>
'''
open("brand/icon-macos.svg","w").write(legacy)

# Icon Composer 分层素材（1024 画布、透明底；底色由 icon.json 的 fill 提供，玻璃与投影由系统渲染）
def layer(body):
    return f'<svg width="1024" height="1024" viewBox="0 0 1024 1024" fill="none" xmlns="http://www.w3.org/2000/svg">{body}</svg>\n'
g=f'transform="matrix({S} 0 0 {S} {T:.2f} {T:.2f})"'
open("app/icons/Wanxiang.icon/Assets/L1-well.svg","w").write(layer('<circle cx="512" cy="512" r="376" fill="#F2F5F9"/>'))
open("app/icons/Wanxiang.icon/Assets/L2-graphite.svg","w").write(layer(f'<g {g}><path d="{ring_graphite}" stroke="#1D1D1F" stroke-width="56" stroke-linecap="round"/></g>'))
open("app/icons/Wanxiang.icon/Assets/L3-ochre.svg","w").write(layer(f'<g {g}><path d="{ring_ochre}" stroke="#B9733F" stroke-width="56" stroke-linecap="round"/></g>'))
open("app/icons/Wanxiang.icon/Assets/L4-mist.svg","w").write(layer(f'<g {g}><path d="{ring_mist}" stroke="#4A6FA0" stroke-width="56" stroke-linecap="round"/></g>'))
open("app/icons/Wanxiang.icon/Assets/L5-dot.svg","w").write(layer(f'<g {g}><circle cx="512" cy="512" r="42" fill="#1D1D1F"/></g>'))
print("已生成 brand/icon-macos.svg 和 app/icons/Wanxiang.icon/Assets/*.svg")
