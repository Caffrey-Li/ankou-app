#!/bin/bash
# M6.1 在 Mac 上的一键验证：生成图标、跑测试、打本机 ad-hoc 包、检查产物。
# 不签名证书、不公证、不上传、不提交、不打开产物。日志写到交接文件夹，供审核读取。
set -u
cd "$(dirname "$0")/.."
LOG="$HOME/tmp/agent-work/2026-11-08-万象小助手交接/对话/回报-M61-mac.md"
mkdir -p "$(dirname "$LOG")"
exec > >(tee "$LOG") 2>&1
step() { printf '\n## %s\n\n' "$1"; }
fail=0
run() { printf '```\n$ %s\n' "$*"; "$@"; local code=$?; printf '退出码 %s\n```\n' "$code"; [ $code -eq 0 ] || fail=1; return $code; }

echo "# 回报-M61-mac：$(date '+%Y-%m-%d %H:%M')"
step "环境"
sw_vers; uname -m
echo "分支：$(git branch --show-current)  HEAD：$(git log --oneline -1)"
if [ -d /Applications/Xcode.app ]; then
  export DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer
  echo "本次构建临时使用 DEVELOPER_DIR=${DEVELOPER_DIR} （不改系统设置）"
fi
xcrun actool --version --output-format human-readable-text 2>&1 | grep -a -i "version" || echo "找不到 actool：Tahoe 新图标会被跳过，只用 .icns"
env | awk -F= '/^APPLE_/ {print "注意：存在变量 " $1}'

step "图标"
run python3 scripts/gen-macos-icon.py
run npm run icons

step "测试"
run npx tsc --noEmit -p ui/tsconfig.json
run npm test
run cargo test --workspace --offline

step "打包（ad-hoc，本机）"
run npm run tauri -- build --target aarch64-apple-darwin --bundles app,dmg
APP="target/aarch64-apple-darwin/release/bundle/macos/万象小助手.app"
DMG=$(ls -t target/aarch64-apple-darwin/release/bundle/dmg/*.dmg 2>/dev/null | head -1)

step "产物检查"
if [ -d "$APP" ]; then
  run scripts/check-release-app.sh "$APP"
  codesign -dv --verbose=4 "$APP" 2>&1 | grep -E "^(Identifier|Signature|CodeDirectory)"
  echo "Info.plist："
  /usr/libexec/PlistBuddy -c 'Print :CFBundleIconName' "$APP/Contents/Info.plist" 2>&1 | sed 's/^/  CFBundleIconName = /'
  /usr/libexec/PlistBuddy -c 'Print :CFBundleIconFile' "$APP/Contents/Info.plist" 2>&1 | sed 's/^/  CFBundleIconFile = /'
  /usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' "$APP/Contents/Info.plist" 2>&1 | sed 's/^/  版本 = /'
  if [ -f "$APP/Contents/Resources/Assets.car" ]; then
    echo "Assets.car：有"
    xcrun assetutil --info "$APP/Contents/Resources/Assets.car" 2>/dev/null | grep -E '"(AssetType|Name)"' | sort | uniq -c | head -20
  else
    echo "Assets.car：无（Tahoe 会用 .icns）"
  fi
  lipo -archs "$APP/Contents/MacOS/"* 2>&1
else
  echo "没有找到 ${APP}"; fail=1
fi
[ -n "$DMG" ] && { echo "dmg：${DMG}"; shasum -a 256 "$DMG"; du -sh "$DMG"; }

step "工作区"
git status --porcelain
echo
if [ $fail -eq 0 ]; then echo "结论：全部通过"; else echo "结论：有失败项，见上"; fi
