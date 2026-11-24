#!/bin/sh
# 检查已签名的 .app，或只检查 release 主二进制。
# 用法：
#   scripts/check-release-app.sh /path/to/安扣助手.app
#   scripts/check-release-app.sh --binary /path/to/wanxiang-app
# 不签名、不打包、不公证。找不到 .app 时不要用空路径硬跑。
set -eu

csp='default-src '"'"'self'"'"'; script-src '"'"'self'"'"'; style-src '"'"'self'"'"' '"'"'unsafe-inline'"'"'; img-src '"'"'self'"'"' data: asset: http://asset.localhost; font-src '"'"'self'"'"' asset: http://asset.localhost; connect-src ipc: http://ipc.localhost; object-src '"'"'none'"'"'; base-uri '"'"'none'"'"

needles='
WANXIANG_VERIFY
verify_enabled
verify_set
WANXIANG_SPACE_FIXTURE
WANXIANG_CURSOR_FIXTURE
WANXIANG_SKIP_REFRESH
WANXIANG_PREVIEW
fixture=m2
fixture=m3
fixture=m4
fixture=handoff
get("bot")
get("codex")
'

permissions='
opener:default
dialog:default
core:default
'

fail() {
  printf 'check-release-app: %s\n' "$1" >&2
  exit 1
}

contains() {
  # $1 file or directory, $2 literal
  if [ -d "$1" ]; then
    grep -a -R -F -q -e "$2" "$1"
  else
    grep -a -F -q -e "$2" "$1"
  fi
}

check_absent() {
  target=$1
  list=$2
  printf '%s\n' "$list" | while IFS= read -r needle; do
    [ -n "$needle" ] || continue
    if contains "$target" "$needle"; then
      printf 'found %s\n' "$needle" >&2
      exit 1
    fi
  done
}

check_arch() {
  archs=$(lipo -archs "$1")
  # 官网分发包是通用包（WANXIANG_UNIVERSAL=1），本机自用包只要 arm64
  if [ "${WANXIANG_UNIVERSAL:-0}" = "1" ]; then
    case "$archs" in
      "x86_64 arm64" | "arm64 x86_64") ;;
      *) fail "架构是「$archs」，通用包需要 x86_64 和 arm64" ;;
    esac
  else
    [ "$archs" = "arm64" ] || fail "架构是「$archs」，只允许 arm64"
  fi
}

check_binary() {
  bin=$1
  [ -f "$bin" ] || fail "没有主二进制 $bin"
  check_arch "$bin"
  check_absent "$bin" "$needles" || fail "主二进制含开发字符串"
  if contains "$bin" "1420/?page"; then
    fail "主二进制含预览地址"
  fi
  if contains "$bin" "127.0.0.1:1420"; then
    printf '主二进制含 devUrl 127.0.0.1:1420。预览地址不在。release 使用 frontendDist，不加载这个地址。\n'
  fi
}

mode=app
if [ "${1:-}" = "--binary" ]; then
  mode=binary
  shift
fi
[ $# -eq 1 ] || fail "用法: check-release-app.sh [--binary 二进制] App.app"

if [ "$mode" = "binary" ]; then
  check_binary "$1"
  printf 'binary ok %s\n' "$1"
  exit 0
fi

app=$1
[ -d "$app" ] || fail "不是 .app 目录：$app"
info=$app/Contents/Info.plist
[ -f "$info" ] || fail "没有 Info.plist"
bin=$(find "$app/Contents/MacOS" -type f -perm -u+x | head -n 1)
[ -n "$bin" ] || fail "Contents/MacOS 里没有可执行文件"

check_binary "$bin"
check_absent "$app" "$needles" || fail "包内含开发字符串"
check_absent "$app" "$permissions" || fail "包内含默认权限名"
contains "$app" "$csp" || fail "包内没有当前 CSP"

if contains "$info" "127.0.0.1:1420"; then
  fail "Info.plist 含 devUrl"
fi
resources=$app/Contents/Resources
if [ -d "$resources" ]; then
  found=$(find "$resources" -type f \( -name '*.js' -o -name '*.css' -o -name '*.html' \) -exec grep -a -F -l "127.0.0.1:1420" {} + 2>/dev/null || true)
  [ -z "$found" ] || fail "前端资源含 devUrl: $found"
fi

err=$(codesign --verify --deep --strict "$app" 2>&1) || fail "codesign --verify --deep --strict 未通过: $err"

printf 'app ok %s\n' "$app"
