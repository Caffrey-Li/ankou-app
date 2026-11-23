# 发版与更新

万象启动时、以及之后每 6 小时，读 `<base_url>/latest.json` 检查更新。

| 文件 | 作用 |
| --- | --- |
| `config.json` | 更新服务地址（Cloudflare R2 公开地址）和存储桶名。`app/tauri.conf.json` 的 `plugins.updater.endpoints` 必须是 `<base_url>/latest.json`。 |
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
