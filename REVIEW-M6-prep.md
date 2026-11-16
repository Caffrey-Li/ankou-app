# M6 准备

分支 `m6-release`，从 `m4-handoff-status` 的 `55d992b` 拉出。未提交，未推送。没有签名，没有打包，没有碰证书和钥匙串，没有运行 `notarytool`。

用户已定：标识符保持 `local.wanxiang.assistant`；只出 arm64；版本号 `0.1.0`。这三项写进了 `PLAN-M6.md`，其他选项已去掉。

## 改了哪些文件

- `PLAN-M6.md`：写上三项决定；第一版只公证 dmg 一次，并写明拖出 app 后离线会被 Gatekeeper 拦下；dmg 的 `codesign -dv` 和 `spctl -a -t open --context context:primary-signature -vv`；首次打开必须先有 `com.apple.quarantine`；开发版和正式版共用同一个数据目录。
- `app/src/handoff.rs`：`WANXIANG_CURSOR_FIXTURE` 只在 `debug_assertions` 下编译。release 里没有这个字符串。debug 行为不变。
- `app/src/collect.rs`：`WANXIANG_SKIP_REFRESH` 同样只在 debug 构建里存在。release 不跳过刷新。
- `ui/release-strip.test.mjs`：release 二进制再查 `WANXIANG_CURSOR_FIXTURE`、`WANXIANG_SKIP_REFRESH`、`WANXIANG_PREVIEW`。
- `app/tauri.conf.json`：`version` 为 `0.1.0`；`hardenedRuntime` 显式 `true`；`entitlements` 指向 `entitlements.plist`。没有 `signingIdentity`。
- `app/entitlements.plist`：新建，内容是空的 `dict`。
- `scripts/check-release-app.sh`：按计划第 6、7 节检查指定 `.app` 的字符串、默认权限名、CSP 和 `lipo` 只有 arm64。`.app` 路径这次没有跑，因为还没有签名包。

## 测试

- `cargo test --workspace --offline`：189 通过，0 失败，退出码 0。
- `npm test`：14 通过，0 失败。其中 `ui/release-strip.test.mjs` 通过。release 二进制里没有上述开发字符串。
- `tsc --noEmit -p ui/tsconfig.json`：退出码 0。

另外用 `scripts/check-release-app.sh --binary` 对这次 `cargo build --release` 的主二进制跑了二进制部分：通过。`lipo` 只有 `arm64`。主二进制里仍有配置中的 `127.0.0.1:1420`，没有带 `?page=` 的预览地址。脚本按计划把前者记成「配置里有，运行时不加载」。

## git status

分支 `m6-release`，HEAD 仍是 `55d992b`。未提交。

```
 M app/src/collect.rs
 M app/src/handoff.rs
 M app/tauri.conf.json
 M ui/release-strip.test.mjs
?? PLAN-M6.md
?? REVIEW-M6-prep.md
?? app/entitlements.plist
?? scripts/check-release-app.sh
```
