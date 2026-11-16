#!/bin/bash
# 逐个变体调用 actool，找出 Wanxiang.icon 编译失败的原因。只在临时目录里操作。
set -u
cd "$(dirname "$0")/.."
export DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer
HANDOFF="$(pwd)/.claude-work"
LOG="$HANDOFF/对话/回报-图标探查.md"
exec > >(LC_ALL=C sed -u "s#$HOME#~#g; s#/Users/[^/ ]*#~#g; s#/var/folders/[^ ]*/T/#TMP/#g" > "$LOG") 2>&1
WORK=$(mktemp -d)
echo "# 图标探查 $(date '+%H:%M')"
xcrun actool --version 2>&1 | grep -a -A1 bundle-version | head -3
try() {
  local name=$1 dir=$2
  rm -rf "$WORK/out"; mkdir -p "$WORK/out"
  local msg
  msg=$(xcrun actool "$dir" --compile "$WORK/out" --platform macosx --minimum-deployment-target 11.0 \
    --app-icon Icon --output-partial-info-plist "$WORK/out/info.plist" --target-device mac 2>&1)
  if [ -f "$WORK/out/Assets.car" ]; then echo "- ${name}：成功（Assets.car $(du -h "$WORK/out/Assets.car" | cut -f1)）"
  else echo "- ${name}：失败 · $(printf '%s' "$msg" | grep -a 'error:' | head -2 | sed 's/.*error: //' | tr '\n' ' ')"; fi
}
# v1 原样
cp -R app/icons/Wanxiang.icon "$WORK/Icon.icon"; try "v1 原样" "$WORK/Icon.icon"
# v2 去掉 SVG 里的 metadata
rm -rf "$WORK/v2"; mkdir -p "$WORK/v2"; cp -R app/icons/Wanxiang.icon "$WORK/v2/Icon.icon"
for f in "$WORK/v2/Icon.icon/Assets/"*.svg; do perl -0pi -e 's#<metadata>.*?</metadata>##s; s# xmlns:c2pa="[^"]*"##' "$f"; done
try "v2 去掉 metadata" "$WORK/v2/Icon.icon"
# v3 v2 + 名称改成英文、去掉 dark 特化
rm -rf "$WORK/v3"; cp -R "$WORK/v2" "$WORK/v3"
python3 - "$WORK/v3/Icon.icon/icon.json" <<'PY'
import json,sys
p=sys.argv[1]; d=json.load(open(p))
for gi,g in enumerate(d["groups"]):
    g["name"]=f"group{gi}"
    for li,l in enumerate(g["layers"]): l["name"]=f"layer{gi}{li}"
d["fill-specializations"]=[x for x in d["fill-specializations"] if "appearance" not in x]
json.dump(d,open(p,"w"),indent=2)
PY
try "v3 英文名、无深色特化" "$WORK/v3/Icon.icon"
# v4 v3 + PNG 图层
rm -rf "$WORK/v4"; cp -R "$WORK/v3" "$WORK/v4"
node -e '
const {Resvg}=require("@resvg/resvg-js"),fs=require("fs"),path=require("path");
const dir=process.argv[1];
for (const f of fs.readdirSync(dir)) if (f.endsWith(".svg")) {
  const png=new Resvg(fs.readFileSync(path.join(dir,f)),{fitTo:{mode:"width",value:1024}}).render().asPng();
  fs.writeFileSync(path.join(dir,f.replace(/\.svg$/,".png")),png); fs.unlinkSync(path.join(dir,f));
}' "$WORK/v4/Icon.icon/Assets"
sed -i '' 's/\.svg"/.png"/g' "$WORK/v4/Icon.icon/icon.json"
try "v4 PNG 图层" "$WORK/v4/Icon.icon"
# v5 最小：单组单层 PNG、纯色底
rm -rf "$WORK/v5"; mkdir -p "$WORK/v5/Icon.icon/Assets"; cp "$WORK/v4/Icon.icon/Assets/L4-mist.png" "$WORK/v5/Icon.icon/Assets/a.png"
cat > "$WORK/v5/Icon.icon/icon.json" <<'J'
{
  "fill" : { "solid" : "srgb:1.00000,1.00000,1.00000,1.00000" },
  "groups" : [ { "layers" : [ { "image-name" : "a.png", "name" : "a" } ] } ],
  "supported-platforms" : { "squares" : "shared" }
}
J
try "v5 最小" "$WORK/v5/Icon.icon"
# v6 v3 的 json 换成 fill 而非 fill-specializations
rm -rf "$WORK/v6"; cp -R "$WORK/v2" "$WORK/v6"
python3 - "$WORK/v6/Icon.icon/icon.json" <<'PY'
import json,sys
p=sys.argv[1]; d=json.load(open(p))
fill=[x["value"] for x in d.pop("fill-specializations") if "appearance" not in x][0]
d["fill"]=fill
json.dump(d,open(p,"w"),indent=2,ensure_ascii=False)
PY
try "v6 用 fill 字段（中文名、SVG 去 metadata）" "$WORK/v6/Icon.icon"
ls -la "$WORK/v1" 2>/dev/null | head -2
echo "临时目录：TMP"
date '+%H:%M' > "$HANDOFF/对话/图标探查.done"
