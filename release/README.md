# 发版与更新

万象启动时、以及之后每 6 小时检查更新。0.3.0 起先读 `<public_url>/latest.json?v=当前版本&a=芯片`（统计 Worker），失败再读 `<base_url>/latest.json`；0.2.0 及以前只读后者，所以 r2.dev 公开地址要一直保留。

## 下载统计

`public_url` 是 `release/worker` 里的 Cloudflare Worker：代理 R2 里的安装包、更新包和 latest.json，每次完整下载或检查更新记一条到 D1 库 `wanxiang-stats`。只记时间、类型、版本、芯片、系统、国家或地区、来源，不存 IP、不存完整 UA，保留 400 天。

- 官网下载链接用 `<public_url>/download`，推广时可加来源：`<public_url>/download?from=xiaohongshu`。
- 统计页：`<public_url>/stats?key=<密钥>`，密钥在本机 `~/.tauri/wanxiang-stats.key`（不入库）。可加 `&days=90`。
- 改了 Worker 后重新部署：`bash release/worker/deploy.sh`（会核对线上；自测请求带 `x-wanxiang-selftest: 1`，不计入统计）。
- 直接用 r2.dev 地址下载的不计入统计。

| 文件 | 作用 |
| --- | --- |
| `config.json` | `base_url`：R2 公开地址（上传核对、旧版本更新）；`public_url`：统计 Worker 地址（官网下载、新版本检查更新、更新包链接）；存储桶名。`app/tauri.conf.json` 的 `plugins.updater.endpoints` 必须依次是 `<public_url>/latest.json?v={{current_version}}&a={{arch}}` 和 `<base_url>/latest.json`。 |
| `update-policy.json` | `minimum_version`：低于这个版本的用户必须更新（整个应用被挡住，只能更新或退出）。 |
| `notes/<版本>.md` | 这个版本的更新说明，显示在更新提示里。每行一条。 |

## 发新版

1. 改 `app/tauri.conf.json` 的 `version`，写 `release/notes/<新版本>.md`。
2. 需要强制更新时，把 `update-policy.json` 的 `minimum_version` 改成新版本（或任何更高的最低版本）。
3. `bash scripts/mac-release.sh`：构建通用包 → Developer ID 签名 → 公证 → 打 dmg → 公证 → 生成更新包、签名和 `latest.json`。
4. `bash scripts/mac-publish.sh`：上传到 R2，最后才替换 `latest.json`。

## 只改强制更新策略、不发新版

改 `update-policy.json` 后运行 `bash scripts/mac-publish.sh --policy-only`：只重写并上传 `latest.json`。

## 密钥

- 更新包签名私钥：`~/.tauri/wanxiang-updater.key`（不入库）。**务必备份**；丢了以后，已装的旧版本将无法再接受新更新。公钥在 `app/tauri.conf.json`。
- 公证凭据：钥匙串里的 `wanxiang-notary`。
- 上传：`wrangler login` 后的 Cloudflare 授权。
