#!/bin/bash
# 0.3.2 起安装包和更新包放在 GitHub Releases（release/config.json 的 github_repo），
# 由 Claude 在云端用 gh 上传。本脚本只做一件事：GitHub 上的版本就绪后，把同一份 latest.json
# 放到 R2，让 0.3.1 及更早的版本（更新地址还是 Worker / r2.dev）也能更新到 GitHub 上的新版。
# 用法：
#   bash scripts/mac-publish.sh               核对 GitHub 上的当前版本，再替换 R2 的 latest.json
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

GH_REPO=$(/usr/bin/python3 -c 'import json;print(json.load(open("release/config.json"))["github_repo"])')
GH_LATEST="https://github.com/${GH_REPO}/releases/latest/download"
TARBALL_NAME="Wanxiang-${VERSION}-universal.app.tar.gz"
[ -s "${MANIFEST}" ] || die "缺少 ${MANIFEST}，先运行 scripts/mac-release.sh"

echo "== 核对 GitHub 上的 ${VERSION}"
GH_VERSION=$(curl -fsSL --max-time 30 "${GH_LATEST}/latest.json" 2>/dev/null | /usr/bin/python3 -c 'import sys,json
try: print(json.load(sys.stdin).get("version",""))
except Exception: print("")')
echo "  GitHub latest.json 版本：${GH_VERSION:-读不到}"
[ "${GH_VERSION}" = "${VERSION}" ] || die "GitHub 上还不是 ${VERSION}，先上传 GitHub Releases"
code=$(curl -sL -o /dev/null -w '%{http_code}' -r 0-0 --max-time 30 "https://github.com/${GH_REPO}/releases/download/v${VERSION}/${TARBALL_NAME}")
echo "  更新包可下载：${code}"
dmg=$(curl -sL -o /dev/null -w '%{http_code}' -r 0-0 --max-time 30 "${GH_LATEST}/Wanxiang.dmg")
echo "  安装包可下载：${dmg}"
case "${code}${dmg}" in 200200|206206|200206|206200) ;; *) die "GitHub 上的文件下载不到";; esac

REMOTE=$(remote_version)
if [ -n "${REMOTE}" ] && older_than_remote "${VERSION}" "${REMOTE}"; then
  die "R2 上已经是 ${REMOTE}，比本地 ${VERSION} 新，不覆盖"
fi
echo
echo "== 给旧版本更新 R2 的 latest.json"
put "${MANIFEST}" "latest.json" "application/json" "no-cache, max-age=0"
sleep 3
ONLINE=$(remote_version)
echo "  R2 latest.json 版本：${ONLINE:-读不到}"
[ "${ONLINE}" = "${VERSION}" ] || die "R2 核对不通过"
echo
echo "  下载地址（固定）：${GH_LATEST}/Wanxiang.dmg"
echo "  发布页：https://github.com/${GH_REPO}/releases"
echo; echo "结论：已发布"
