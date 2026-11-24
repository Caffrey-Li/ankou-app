#!/bin/bash
# 部署下载统计 Worker：建 D1 库和表、设统计页密钥、发布。
# 前提：wrangler 已登录（与 mac-publish.sh 相同）。日志不写账号、邮箱和密钥。
set -u
cd "$(dirname "$0")"
export PATH="${HOME}/.cargo/bin:/opt/homebrew/bin:/usr/local/bin:${PATH}"
W=(npx --yes wrangler@4)
mask() { sed -E 's/[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}/<邮箱>/g; s#/accounts/[0-9a-f]{32}#/accounts/<id>#g; s/[0-9a-f]{32}/<id>/g'; }
die() { echo "失败：$1"; echo; echo "结论：未部署"; exit 1; }

"${W[@]}" whoami 2>/dev/null | grep -qi "logged in" || die "wrangler 没有登录"

echo "== D1 数据库"
if grep -q '__D1_ID__' wrangler.toml; then
  ID=$("${W[@]}" d1 list --json 2>/dev/null | /usr/bin/python3 -c 'import sys,json
try:
  print(next((d.get("uuid") or d.get("id") or "") for d in json.load(sys.stdin) if d.get("name")=="wanxiang-stats"))
except Exception: print("")')
  if [ -z "${ID}" ]; then
    ID=$("${W[@]}" d1 create wanxiang-stats 2>&1 | grep -oE '[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}' | head -1)
    [ -n "${ID}" ] || die "建库失败"
    echo "  已新建 wanxiang-stats"
  else
    echo "  已存在 wanxiang-stats"
  fi
  /usr/bin/sed -i '' "s/__D1_ID__/${ID}/" wrangler.toml
else
  echo "  已配置"
fi
"${W[@]}" d1 execute wanxiang-stats --remote --file schema.sql --yes >/dev/null 2>&1 || die "建表失败"
echo "  表已就绪"

echo "== 发布 Worker"
OUT=$("${W[@]}" deploy 2>&1) || { printf '%s\n' "${OUT}" | mask | tail -20; die "发布失败"; }
URL=$(printf '%s\n' "${OUT}" | grep -oE 'https://[a-z0-9.-]+\.workers\.dev' | head -1)
[ -n "${URL}" ] || { printf '%s\n' "${OUT}" | mask | tail -20; die "没拿到 workers.dev 地址"; }
printf '%s\n' "${URL}" > url.txt
echo "  地址：${URL}"

echo "== 统计页密钥"
KEYFILE="${HOME}/.tauri/wanxiang-stats.key"
if [ ! -s "${KEYFILE}" ]; then
  mkdir -p "$(dirname "${KEYFILE}")"
  ( umask 077; /usr/bin/openssl rand -hex 16 > "${KEYFILE}" )
  echo "  已生成（保存在 ~/.tauri/wanxiang-stats.key）"
fi
tr -d '\n' < "${KEYFILE}" | "${W[@]}" secret put STATS_KEY >/dev/null 2>&1 || die "设置密钥失败"
echo "  已设置"

echo "== 线上核对（自测请求不计入统计）"
H=(-H "x-wanxiang-selftest: 1")
code() { curl -s -o /dev/null -w '%{http_code}' --max-time 30 "${H[@]}" "$@"; }
sleep 5
echo "  latest.json：$(code "${URL}/latest.json")"
echo "  安装包（HEAD）：$(code -I "${URL}/download")"
echo "  安装包大小：$(curl -sI --max-time 30 "${H[@]}" "${URL}/download" | tr -d '\r' | awk -F': ' 'tolower($1)=="content-length"{print $2}') 字节"
echo "  统计页（无密钥）：$(code "${URL}/stats")"
echo "  统计页（带密钥）：$(code "${URL}/stats?key=$(cat "${KEYFILE}")")"
echo
echo "结论：已部署"
