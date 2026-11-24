#!/bin/bash
# 把 mac-release.sh 的产物发到 GitHub Releases（release/config.json 的 github_repo）。
# 用法：bash scripts/gh-release.sh <产物目录>   目录里要有 Wanxiang-<版本>-universal.app.tar.gz(.sig)、
#       Wanxiang-<版本>.dmg、Wanxiang.dmg、latest.json。需要已登录、对该仓库有写权限的 gh。
# 先建草稿、传完全部文件再公开，避免 releases/latest 指向缺文件的版本。日志不写令牌。
set -euo pipefail
cd "$(dirname "$0")/.."
DIR="${1:?产物目录}"
VERSION=$(python3 -c 'import json;print(json.load(open("app/tauri.conf.json"))["version"])')
REPO=$(python3 -c 'import json;print(json.load(open("release/config.json"))["github_repo"])')
TAG="v${VERSION}"
FILES=("${DIR}/Wanxiang-${VERSION}-universal.app.tar.gz" "${DIR}/Wanxiang-${VERSION}-universal.app.tar.gz.sig"
       "${DIR}/Wanxiang-${VERSION}.dmg" "${DIR}/Wanxiang.dmg" "${DIR}/latest.json")
for f in "${FILES[@]}"; do [ -s "$f" ] || { echo "缺少 $f"; exit 1; }; done
python3 - "${DIR}/latest.json" "${VERSION}" "${REPO}" <<'PY'
import json, sys
m = json.load(open(sys.argv[1])); v, repo = sys.argv[2], sys.argv[3]
assert m["version"] == v, "latest.json 版本不对"
for p in m["platforms"].values():
    assert p["url"].startswith(f"https://github.com/{repo}/releases/download/v{v}/"), "latest.json 的更新包地址不是 GitHub"
PY
if gh release view "${TAG}" -R "${REPO}" >/dev/null 2>&1; then
  echo "== ${TAG} 已存在，补传文件"
  gh release upload "${TAG}" "${FILES[@]}" -R "${REPO}" --clobber
else
  echo "== 创建 ${TAG}（草稿）"
  gh release create "${TAG}" "${FILES[@]}" -R "${REPO}" --draft \
    --title "万象小助手 ${VERSION}" --notes-file "release/notes/${VERSION}.md"
fi
gh release edit "${TAG}" -R "${REPO}" --draft=false --latest
echo "== 已公开：https://github.com/${REPO}/releases/tag/${TAG}"
