#!/bin/bash
# 用 actool 把 Icon Composer 图标 app/icons/Wanxiang.icon 预先编译成 app/icons/Assets.car。
# Tauri 直接调用 actool 时在本机报 Bad file descriptor；手动编译正常，打包时直接用这个 Assets.car。
set -euo pipefail
cd "$(dirname "$0")/.."
[ -d /Applications/Xcode.app ] && export DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer
WORK=$(mktemp -d)
cp -R app/icons/Wanxiang.icon "$WORK/Icon.icon"
mkdir -p "$WORK/out"
xcrun actool "$WORK/Icon.icon" --compile "$WORK/out" --platform macosx \
  --minimum-deployment-target 11.0 --app-icon Icon --target-device mac \
  --output-partial-info-plist "$WORK/out/info.plist" < /dev/null > "$WORK/actool.log" 2>&1 || {
  grep -a "error:" "$WORK/actool.log" | head -5; exit 1; }
[ -f "$WORK/out/Assets.car" ] || { echo "actool 没有生成 Assets.car"; exit 1; }
cp "$WORK/out/Assets.car" app/icons/Assets.car
echo "Assets.car：$(du -h app/icons/Assets.car | cut -f1)"
xcrun assetutil --info app/icons/Assets.car | grep -a -E '"(AssetType|Name)"' | sort | uniq -c | head -6
