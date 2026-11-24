#!/bin/bash
# 把 mac-release.sh 的产物发到 GitHub Releases（release/config.json 的 github_repo）。
# 用法：bash scripts/gh-release.sh <产物目录>   目录里要有 Wanxiang-<版本>-universal.app.tar.gz(.sig)、
#       Wanxiang-<版本>.dmg、Wanxiang.dmg、latest.json。需要对该仓库有写权限的 gh（只用 REST 接口）。
# 先建草稿、传完全部文件再公开，避免 releases/latest 指向缺文件的版本。日志不写令牌。
set -euo pipefail
cd "$(dirname "$0")/.."
DIR="${1:?产物目录}"
VERSION=$(python3 -c 'import json;print(json.load(open("app/tauri.conf.json"))["version"])')
REPO=$(python3 -c 'import json;print(json.load(open("release/config.json"))["github_repo"])')
TAG="v${VERSION}"
FILES=("Wanxiang-${VERSION}-universal.app.tar.gz" "Wanxiang-${VERSION}-universal.app.tar.gz.sig"
       "Wanxiang-${VERSION}.dmg" "Wanxiang.dmg" "latest.json")
for f in "${FILES[@]}"; do [ -s "${DIR}/$f" ] || { echo "缺少 ${DIR}/$f"; exit 1; }; done
python3 - "${DIR}/latest.json" "${VERSION}" "${REPO}" <<'PY'
import json, sys
m = json.load(open(sys.argv[1])); v, repo = sys.argv[2], sys.argv[3]
assert m["version"] == v, "latest.json 版本不对"
for p in m["platforms"].values():
    assert p["url"].startswith(f"https://github.com/{repo}/releases/download/v{v}/"), "latest.json 的更新包地址不是 GitHub"
PY
ID=$(gh api "repos/${REPO}/releases?per_page=30" --jq ".[] | select(.tag_name==\"${TAG}\") | .id" | head -1)
if [ -z "${ID}" ]; then
  echo "== 创建 ${TAG}（草稿）"
  ID=$(gh api -X POST "repos/${REPO}/releases" -f tag_name="${TAG}" -f target_commitish=main \
       -f name="万象小助手 ${VERSION}" -F body=@"release/notes/${VERSION}.md" -F draft=true --jq .id)
else
  echo "== ${TAG} 已存在（${ID}），替换文件"
fi
for f in "${FILES[@]}"; do
  old=$(gh api "repos/${REPO}/releases/${ID}/assets?per_page=100" --jq ".[] | select(.name==\"$f\") | .id")
  [ -n "${old}" ] && gh api -X DELETE "repos/${REPO}/releases/assets/${old}" >/dev/null
  case "$f" in *.json) type=application/json;; *.dmg) type=application/x-apple-diskimage;; *.gz) type=application/gzip;; *) type=application/octet-stream;; esac
  size=$(gh api -X POST "https://uploads.github.com/repos/${REPO}/releases/${ID}/assets?name=$f" \
         -H "Content-Type: ${type}" --input "${DIR}/$f" --jq .size)
  echo "  已上传 $f（${size} 字节）"
done
gh api -X PATCH "repos/${REPO}/releases/${ID}" -F draft=false -f make_latest=true --jq '"== 已公开：" + .html_url'
