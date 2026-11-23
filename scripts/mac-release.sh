#!/bin/bash
# 官网分发包：Developer ID 签名 → 公证 → 装订 → 打 dmg → 再公证 dmg → 检查。
# 用法：bash scripts/mac-release.sh
# 前提（只需一次）：
#   1. 钥匙串里有「Developer ID Application」证书（含私钥）
#   2. 公证凭据存进钥匙串，名字 wanxiang-notary：
#        xcrun notarytool store-credentials wanxiang-notary --apple-id <Apple ID> --team-id <团队编号>
#      专用密码由本人在终端里输入，脚本不接触。
# 日志只写证书指纹前几位与团队编号，不写姓名、邮箱、密码。
set -u
cd "$(dirname "$0")/.."
export PATH="${HOME}/.cargo/bin:/opt/homebrew/bin:/usr/local/bin:${PATH}"

PROFILE="wanxiang-notary"
# 通用包：Apple 芯片与 Intel 都能装（需要 rustup 的 x86_64-apple-darwin 目标）
TARGET="universal-apple-darwin"
NAME="万象小助手"
fail=0

step() { printf '\n== %s\n' "$1"; }
die() { echo "失败：$1"; echo; echo "结论：未完成"; exit 1; }

step "证书与公证凭据"
rustup target list --installed 2>/dev/null | grep -q x86_64-apple-darwin || die "缺少 Intel 编译目标：rustup target add x86_64-apple-darwin"
DEV_ID=$(security find-identity -v -p codesigning 2>/dev/null | grep '"Developer ID Application' | head -1 | awk '{print $2}')
[ -n "${DEV_ID}" ] || die "钥匙串里没有 Developer ID Application 证书"
TEAM=$(security find-identity -v -p codesigning | grep "${DEV_ID}" | sed -E 's/.*\(([A-Z0-9]{10})\)".*/\1/' | head -1)
echo "签名：Developer ID Application（${DEV_ID:0:8}…），团队 ${TEAM:-未知}"
xcrun notarytool history --keychain-profile "${PROFILE}" >/dev/null 2>&1 || die "钥匙串里没有公证凭据 ${PROFILE}"
echo "公证凭据：${PROFILE} 可用"

VERSION=$(/usr/bin/python3 -c 'import json;print(json.load(open("app/tauri.conf.json"))["version"])')
OUT="dist/release-${VERSION}"
[ "${RESUME:-0}" = "1" ] || rm -rf "${OUT}"
mkdir -p "${OUT}"
echo "版本：${VERSION}"

step "构建并签名 .app"
APP="${OUT}/${NAME}.app"
if [ "${RESUME:-0}" = "1" ] && [ -d "${APP}" ]; then
  echo "沿用上次构建的 .app"
else
export APPLE_SIGNING_IDENTITY="${DEV_ID}"
# 不让打包工具自己去公证，公证在下面单独做
unset APPLE_ID APPLE_PASSWORD APPLE_TEAM_ID APPLE_API_KEY APPLE_API_ISSUER APPLE_API_KEY_PATH
CI=true npm run tauri -- build --target "${TARGET}" --bundles app >"${OUT}/build.log" 2>&1 || { tail -30 "${OUT}/build.log"; die "构建失败"; }
APP_SRC="target/${TARGET}/release/bundle/macos/${NAME}.app"
[ -d "${APP_SRC}" ] || die "没有找到构建出的 .app"
ditto "${APP_SRC}" "${APP}" && rm -rf "${APP_SRC}"
fi
codesign --verify --deep --strict "${APP}" || die "签名校验不通过"
codesign -dvv "${APP}" 2>&1 | grep -E "^(Identifier|TeamIdentifier|Runtime Version)=" | sed 's/^/  /'
codesign -d --entitlements :- "${APP}" 2>/dev/null | grep -q "get-task-allow" && die "带有调试权限 get-task-allow，不能公证"
WANXIANG_UNIVERSAL=1 bash scripts/check-release-app.sh "${APP}" >/dev/null || die "发布包检查不通过（开发开关）"
echo "  发布包检查：通过"
echo "  架构：$(lipo -archs "${APP}/Contents/MacOS/"* 2>/dev/null | head -1)"

json_field() {
  /usr/bin/python3 -c 'import sys,json
try: print(json.load(sys.stdin).get(sys.argv[1],""))
except Exception: print("")' "$1"
}

# 提交后记下编号，再每分钟查一次状态（最多 6 小时）。
# 新账号头几次公证可能要排队很久；中途锁屏导致钥匙串暂时读不到，下一轮再查，不算失败。
# 编号存在 ${OUT}/notary-<名字>.id，RESUME=1 重跑时接着等，不重复提交。
notarize() {
  local file="$1" label="$2" idfile="${OUT}/notary-$2.id" id="" status="" tries=0
  if [ "${RESUME:-0}" = "1" ] && [ -s "${idfile}" ]; then
    id=$(cat "${idfile}")
    echo "接着等之前提交的公证：${label}"
  else
    echo "提交公证：${label}"
    id=$(xcrun notarytool submit "${file}" --keychain-profile "${PROFILE}" --output-format json 2>/dev/null | json_field id)
    [ -n "${id}" ] || { echo "  提交失败"; return 1; }
    printf '%s' "${id}" > "${idfile}"
  fi
  while [ ${tries} -lt 360 ]; do
    status=$(xcrun notarytool info "${id}" --keychain-profile "${PROFILE}" --output-format json 2>/dev/null | json_field status)
    case "${status}" in
      Accepted) echo "  结果：Accepted（等了约 ${tries} 分钟）"; return 0 ;;
      Invalid|Rejected)
        echo "  结果：${status}"
        xcrun notarytool log "${id}" --keychain-profile "${PROFILE}" "${OUT}/notary-${label}.json" >/dev/null 2>&1 \
          && echo "  公证日志：${OUT}/notary-${label}.json"
        return 1 ;;
    esac
    [ $((tries % 15)) -eq 0 ] && echo "  排队中（${status:-暂时查不到}），已等 ${tries} 分钟"
    sleep 60
    tries=$((tries + 1))
  done
  echo "  等了 6 小时仍未完成"
  return 1
}

step "公证 .app"
ZIP="${OUT}/${NAME}-app.zip"
[ -f "${ZIP}" ] || ditto -c -k --keepParent "${APP}" "${ZIP}"
notarize "${ZIP}" app || die ".app 公证未通过"
xcrun stapler staple "${APP}" >/dev/null || die ".app 装订公证票据失败"
rm -f "${ZIP}"
echo "  已装订"

step "打 dmg"
DMG="${OUT}/${NAME}-${VERSION}.dmg"
STAGE=$(mktemp -d)
ditto "${APP}" "${STAGE}/${NAME}.app"
ln -s /Applications "${STAGE}/Applications"
hdiutil create -volname "${NAME}" -srcfolder "${STAGE}" -fs HFS+ -format UDZO -ov "${DMG}" >/dev/null || die "打 dmg 失败"
rm -rf "${STAGE}"
codesign --sign "${DEV_ID}" --timestamp "${DMG}" || die "dmg 签名失败"

step "公证 dmg"
notarize "${DMG}" dmg || die "dmg 公证未通过"
xcrun stapler staple "${DMG}" >/dev/null || die "dmg 装订公证票据失败"
echo "  已装订"

step "系统检查（模拟别人第一次打开）"
spctl -a -vv -t exec "${APP}" 2>&1 | sed 's/^/  /' | grep -vE "origin="
spctl -a -t exec "${APP}" >/dev/null 2>&1 || fail=1
spctl -a -vv -t open --context context:primary-signature "${DMG}" 2>&1 | sed 's/^/  /' | grep -vE "origin="
spctl -a -t open --context context:primary-signature "${DMG}" >/dev/null 2>&1 || fail=1
xcrun stapler validate "${DMG}" >/dev/null 2>&1 && echo "  dmg 票据：有效" || fail=1
xcrun stapler validate "${APP}" >/dev/null 2>&1 && echo "  .app 票据：有效" || fail=1

step "产物"
echo "dmg：${DMG}"
echo "大小：$(du -sh "${DMG}" | cut -f1)"
echo "SHA-256：$(shasum -a 256 "${DMG}" | cut -d' ' -f1)"
if [ ${fail} -eq 0 ]; then
  echo; echo "结论：可以分发"
else
  echo; echo "结论：系统检查未通过"; exit 1
fi
