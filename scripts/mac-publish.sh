#!/bin/bash
# 把 mac-release.sh 的产物上传到 Cloudflare R2，供官网下载和应用内更新。
# 用法：
#   bash scripts/mac-publish.sh               上传当前版本（dmg、更新包），最后替换 latest.json
#   bash scripts/mac-publish.sh --policy-only 只按 release/update-policy.json 重写并上传 latest.json
# 前提：wrangler 已登录（npx wrangler login），存储桶已开公开访问。
# 日志不写账号、邮箱和令牌。
set -u
cd "$(dirname "$0")/.."
export PATH="${HOME}/.cargo/bin:/opt/homebrew/bin:/usr/local/bin:${PATH}"

die() { echo "失败：$1"; echo; echo "结论：未发布"; exit 1; }
WRANGLER=(npx --yes wrangler@4)

VERSION=$(/usr/bin/python3 -c 'import json;print(json.load(open("app/tauri.conf.json"))["version"])')
BUCKET=$(/usr/bin/python3 -c 'import json;print(json.load(open("release/config.json"))["bucket"])')
BASE_URL=$(/usr/bin/python3 -c 'import json;print(json.load(open("release/config.json"))["base_url"].rstrip("/"))')
PUBLIC_URL=$(/usr/bin/python3 -c 'import json;c=json.load(open("release/config.json"));print(c.get("public_url",c["base_url"]).rstrip("/"))')
OUT="dist/release-${VERSION}"
MANIFEST="${OUT}/latest.json"

"${WRANGLER[@]}" whoami 2>/dev/null | grep -qi "logged in" || die "wrangler 没有登录"

put() {
  local file="$1" key="$2" type="$3" cache="$4"
  "${WRANGLER[@]}" r2 object put "${BUCKET}/${key}" --file "${file}" --remote \
    --content-type "${type}" --cache-control "${cache}" >/dev/null 2>&1 || die "上传 ${key} 失败"
  echo "  已上传 ${key}"
}

# 线上已有的 latest.json 版本更高时不覆盖（防止误把旧版发成最新）
remote_version() {
  curl -fsS --max-time 15 "${BASE_URL}/latest.json" 2>/dev/null | /usr/bin/python3 -c 'import sys,json
try: print(json.load(sys.stdin).get("version",""))
except Exception: print("")'
}
older_than_remote() {
  /usr/bin/python3 - "$1" "$2" <<'PY'
import sys
def key(v):
    try: return tuple(int(x) for x in v.strip().lstrip("v").split("-")[0].split("."))
    except Exception: return None
local, remote = key(sys.argv[1]), key(sys.argv[2])
sys.exit(0 if local and remote and local < remote else 1)
PY
}

if [ "${1:-}" = "--policy-only" ]; then
  echo "== 只更新强制更新策略"
  CURRENT=$(curl -fsS --max-time 15 "${BASE_URL}/latest.json" 2>/dev/null) || die "读不到线上的 latest.json"
  printf '%s' "${CURRENT}" | /usr/bin/python3 -c '
import json, sys
manifest = json.load(sys.stdin)
policy = json.load(open("release/update-policy.json"))
manifest["minimum_version"] = policy["minimum_version"]
json.dump(manifest, open(sys.argv[1], "w", encoding="utf-8"), ensure_ascii=False, indent=2)
print("  线上版本 %s，最低版本改为 %s" % (manifest["version"], manifest["minimum_version"]))
' "${OUT}/latest-policy.json" || die "改写 latest.json 失败"
  put "${OUT}/latest-policy.json" "latest.json" "application/json" "no-cache, max-age=0"
  echo; echo "结论：已发布"
  exit 0
fi

echo "== 上传 ${VERSION}"
TARBALL="${OUT}/Wanxiang-${VERSION}-universal.app.tar.gz"
DMG="${OUT}/Wanxiang-${VERSION}.dmg"
for file in "${TARBALL}" "${TARBALL}.sig" "${DMG}" "${MANIFEST}"; do
  [ -s "${file}" ] || die "缺少 ${file}，先运行 scripts/mac-release.sh"
done
xcrun stapler validate "${DMG}" >/dev/null 2>&1 || die "dmg 没有公证票据"
REMOTE=$(remote_version)
if [ -n "${REMOTE}" ] && older_than_remote "${VERSION}" "${REMOTE}"; then
  die "线上已经是 ${REMOTE}，比本地 ${VERSION} 新，不覆盖"
fi

# 先传带版本号的文件，最后才替换 latest.json 和官网下载链接，避免用户读到指向未上传文件的清单
put "${TARBALL}" "v${VERSION}/$(basename "${TARBALL}")" "application/gzip" "public, max-age=31536000, immutable"
put "${DMG}" "v${VERSION}/$(basename "${DMG}")" "application/x-apple-diskimage" "public, max-age=31536000, immutable"
put "${DMG}" "Wanxiang.dmg" "application/x-apple-diskimage" "no-cache, max-age=0"
put "${MANIFEST}" "latest.json" "application/json" "no-cache, max-age=0"

echo
echo "== 线上核对"
sleep 3
ONLINE=$(remote_version)
echo "  latest.json 版本：${ONLINE:-读不到}"
code=$(curl -s -o /dev/null -w '%{http_code}' -I --max-time 20 -H "x-wanxiang-selftest: 1" "${PUBLIC_URL}/v${VERSION}/$(basename "${TARBALL}")")
echo "  更新包可下载：${code}"
echo "  官网下载链接：${PUBLIC_URL}/download（统计下载；可加 ?from=来源）"
echo "  备用直链：${BASE_URL}/Wanxiang.dmg（不计入统计）"
[ "${ONLINE}" = "${VERSION}" ] && [ "${code}" = "200" ] || die "线上核对不通过"
echo; echo "结论：已发布"
