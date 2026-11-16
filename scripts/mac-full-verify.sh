#!/bin/bash
# 全量验证：取依赖、跑全部测试、真实数据只读核对、打本机 ad-hoc 包并检查产物。
# 不签名证书、不公证、不上传、不提交。日志里的用户目录一律换成 ~。
set -u
cd "$(dirname "$0")/.."
HANDOFF="$(pwd)/.claude-work"
MODE="${1:-full}"
case "$MODE" in
  quick) NAME="快速核对" ;;
  probe) NAME="数据探查" ;;
  *) NAME="全量验证" ;;
esac
LOG="$HANDOFF/对话/回报-${NAME}.md"
mkdir -p "$(dirname "$LOG")"
rm -f "$HANDOFF/对话/${NAME}.done"
exec > >(LC_ALL=C sed -u "s#$HOME#~#g; s#/Users/[^/ ]*#~#g" > "$LOG") 2>&1
export PATH="$HOME/.cargo/bin:/opt/homebrew/bin:/usr/local/bin:$PATH"
[ -s "$HOME/.nvm/nvm.sh" ] && . "$HOME/.nvm/nvm.sh" >/dev/null 2>&1
step() { printf '\n## %s\n\n' "$1"; }
fail=0
failed=()
run() {
  printf '```\n$ %s\n' "$*"
  "$@"
  local code=$?
  printf '退出码 %s\n```\n' "$code"
  if [ $code -ne 0 ]; then fail=1; failed+=("$*"); fi
  return $code
}

echo "# 回报-${NAME}：$(date '+%Y-%m-%d %H:%M')"
step "环境"
sw_vers | tr '\n' ' '; echo; uname -m
echo "分支：$(git branch --show-current)  HEAD：$(git log --oneline -1)"
command -v cargo >/dev/null || { echo "找不到 cargo"; }
cargo --version; node --version; npm --version
if [ -d /Applications/Xcode.app ]; then
  export DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer
  echo "本次构建临时使用 DEVELOPER_DIR=${DEVELOPER_DIR}"
fi

step "依赖"
run cargo fetch

finish() {
  echo
  if [ $fail -eq 0 ]; then
    echo "结论：全部通过"
  else
    echo "结论：有失败项"
    for item in "${failed[@]}"; do echo "- $item"; done
  fi
  date '+%H:%M' > "$HANDOFF/对话/${NAME}.done"
  exit 0
}

if [ "$MODE" = probe ]; then
  step "真实数据只读核对（只打印数量）"
  run cargo run -q --offline -p wanxiang-adapter-dsh --example verify_dsh
  run cargo run -q --offline -p wanxiang-adapter-codex --example verify_codex
  run cargo run -q --offline -p wanxiang-adapter-cursor --example verify_cursor
  finish
fi

if [ "$MODE" = quick ]; then
  step "Rust 测试"
  WANXIANG_WRITE_FIXTURE=1 run cargo test -p wanxiang-app --offline m2_fixture_page
  run cargo test --workspace --offline
  step "真实数据只读核对（只打印数量）"
  run cargo run -q --offline -p wanxiang-adapter-dsh --example verify_dsh
  run cargo run -q --offline -p wanxiang-adapter-codex --example verify_codex
  run cargo run -q --offline -p wanxiang-adapter-cursor --example verify_cursor
  finish
fi

step "签名证书"
if ! security find-certificate -c "Wanxiang Local Signing" >/dev/null 2>&1; then
  echo "没有本机签名证书，请先运行「生成签名证书.command」"; fail=1; failed+=("缺少本机签名证书")
else
  echo "本机签名证书：有"
fi

step "图标"
run python3 scripts/gen-macos-icon.py
run npm run icons
run bash scripts/build-assets-car.sh

step "Rust 测试"
WANXIANG_WRITE_FIXTURE=1 run cargo test -p wanxiang-app --offline m2_fixture_page
run cargo test --workspace --offline

step "前端测试"
run npx tsc --noEmit -p ui/tsconfig.json
run npm test

step "真实数据只读核对（只打印数量）"
run cargo run -q --offline -p wanxiang-adapter-dsh --example verify_dsh
run cargo run -q --offline -p wanxiang-adapter-codex --example verify_codex
run cargo run -q --offline -p wanxiang-adapter-cursor --example verify_cursor

step "打包（ad-hoc，本机）"
CI=true run npm run tauri -- build --target aarch64-apple-darwin --bundles app,dmg
APP="target/aarch64-apple-darwin/release/bundle/macos/万象小助手.app"
DMG=$(ls -t target/aarch64-apple-darwin/release/bundle/dmg/*.dmg 2>/dev/null | head -1)

step "产物检查"
if [ -d "$APP" ]; then
  run scripts/check-release-app.sh "$APP"
  /usr/libexec/PlistBuddy -c 'Print :CFBundleIconName' "$APP/Contents/Info.plist" 2>&1 | sed 's/^/  CFBundleIconName = /'
  /usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' "$APP/Contents/Info.plist" 2>&1 | sed 's/^/  版本 = /'
  [ -f "$APP/Contents/Resources/Assets.car" ] && echo "Assets.car：有" || echo "Assets.car：无"
  codesign -dvv "$APP" 2>&1 | grep -E "^(Identifier|Authority|Signature)=" | sed 's/^/  /' 
  lipo -archs "$APP/Contents/MacOS/"* 2>&1
else
  echo "没有找到 .app"; fail=1; failed+=("找不到 .app")
fi
[ -n "$DMG" ] && { echo "dmg：$(basename "$DMG")"; shasum -a 256 "$DMG" | cut -c1-16; du -sh "$DMG" | cut -f1; }

step "工作区"
git status --porcelain | head -80
finish
