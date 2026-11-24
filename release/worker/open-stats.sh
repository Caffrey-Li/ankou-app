#!/bin/bash
# 在默认浏览器里打开下载统计页（密钥只在本机 ~/.tauri/wanxiang-stats.key）。可加参数天数：open-stats.sh 90
cd "$(dirname "$0")"
/usr/bin/open "$(cat url.txt)/stats?days=${1:-30}&key=$(cat "${HOME}/.tauri/wanxiang-stats.key")"
