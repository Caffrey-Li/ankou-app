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
  run cargo run -q --offline -p wanxiang-adapter-claude --example verify_claude
  run cargo run -q --offline -p wanxiang-adapter-antigravity --example verify_antigravity
  run cargo run -q --offline -p wanxiang-adapter-workbuddy --example verify_workbuddy
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
  run cargo run -q --offline -p wanxiang-adapter-claude --example verify_claude
  run cargo run -q --offline -p wanxiang-adapter-antigravity --example verify_antigravity
  run cargo run -q --offline -p wanxiang-adapter-workbuddy --example verify_workbuddy
  finish
fi

step "签名证书"
# 优先用 Apple ID 的开发证书：签名带固定团队编号，钥匙串的「始终允许」跨版本有效。
# 用证书指纹指定，日志里不写证书上的姓名和邮箱。
DEV_ID=$(security find-identity -v -p codesigning 2>/dev/null | grep '"Apple Development' | head -1 | awk '{print $2}')
if [ -n "$DEV_ID" ]; then
  export APPLE_SIGNING_IDENTITY="$DEV_ID"
  echo "签名：Apple Development 开发证书（${DEV_ID:0:8}…）"
elif security find-certificate -c "Wanxiang Local Signing" >/dev/null 2>&1; then
  echo "签名：本机自签证书（没有团队编号，每个新版本钥匙串会再问一次）"
else
  echo "没有可用的签名证书，请先运行「生成签名证书.command」"; fail=1; failed+=("缺少签名证书")
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
run cargo run -q --offline -p wanxiang-adapter-claude --example verify_claude
run cargo run -q --offline -p wanxiang-adapter-antigravity --example verify_antigravity
run cargo run -q --offline -p wanxiang-adapter-workbuddy --example verify_workbuddy

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
  codesign -dvv "$APP" 2>&1 | grep -E "^(Identifier|Signature|TeamIdentifier)=" | sed 's/^/  /'
  codesign -dvv "$APP" 2>&1 | grep -m1 "^Authority=" | sed -E 's/^Authority=([^:]*).*/  Authority=\1/' 
  lipo -archs "$APP/Contents/MacOS/"* 2>&1
else
  echo "没有找到 .app"; fail=1; failed+=("找不到 .app")
fi
[ -n "$DMG" ] && { echo "dmg：$(basename "$DMG")"; shasum -a 256 "$DMG" | cut -c1-16; du -sh "$DMG" | cut -f1; }

step "安装到应用程序"
# 全部通过才装：替换「应用程序」里的旧版，再删掉编译目录里那份，免得启动台出现两个
if [ $fail -eq 0 ] && [ -d "$APP" ]; then
  osascript -e 'quit app id "local.wanxiang.assistant"' >/dev/null 2>&1; sleep 2
  rm -rf "/Applications/万象小助手.app" && ditto "$APP" "/Applications/万象小助手.app" \
    && rm -rf "$APP" target/aarch64-apple-darwin/release/bundle/macos/rw.*.dmg \
    && echo "已安装到 /Applications，编译目录里的副本已删除" \
    || { echo "安装失败"; fail=1; failed+=("安装到应用程序"); }
else
  echo "有失败项，未安装"
fi

step "工作区"
git status --porcelain | head -80
finish
