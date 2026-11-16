# VERIFY-M6-local：本机包（ad-hoc 签名）

2026-11-11。分支 `m6-release`。没有用证书，没有公证，没有上传，没有提交，没有打开产物。

当前产物是 ad-hoc（`signingIdentity` 为 `"-"`）签过的整个包。下面两节是更早的记录：构建命令的工作目录，以及只有链接器签名的那一次。sha256 以本节为准。

## 当前产物

`bundle.macOS.signingIdentity` 是 `"-"`。`hardenedRuntime` 仍是 `true`，`entitlements` 仍指向空的 `entitlements.plist`。`scripts/check-release-app.sh` 的 .app 模式在通过前要求 `codesign --verify --deep --strict`。

`APPLE_` 变量名为空。仓库根目录 `npm run tauri -- build --target aarch64-apple-darwin --bundles app,dmg`，退出码 0。日志有 `Signing with identity "-"`，并警告因为没有公证用的 `APPLE_ID` / `APPLE_PASSWORD` / `APPLE_TEAM_ID` 或 API key 变量而跳过公证。没有系统弹窗。

- `~/Projects/Wanxiang/target/aarch64-apple-darwin/release/bundle/macos/万象小助手.app`
- `~/Projects/Wanxiang/target/aarch64-apple-darwin/release/bundle/dmg/万象小助手_0.1.0_aarch64.dmg`（22M）

```
33eb3418b0273e93477ea11a801552d9b52e9aa49c0cb06e79e6a3d475386da4  万象小助手_0.1.0_aarch64.dmg
```

`codesign -dv --verbose=4`：

```
Identifier=local.wanxiang.assistant
CodeDirectory v=20500 size=65625 flags=0x10002(adhoc,runtime) hashes=2040+7 location=embedded
Executable Segment flags=0x1
Signature=adhoc
```

`codesign -d --entitlements :-` 是空的 `dict`。另有一行弃用警告（路径里的 `:`），以及 `Executable=` 指向包内 `Contents/MacOS/wanxiang-app`。

```
warning: Specifying ':' in the path is deprecated and will not work in a future release
<?xml version="1.0" encoding="UTF-8"?><!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "https://www.apple.com/DTDs/PropertyList-1.0.dtd"><plist version="1.0"><dict></dict></plist>
```

`codesign --verify --deep --strict --verbose=2` 退出码 0：

```
万象小助手.app: valid on disk
万象小助手.app: satisfies its Designated Requirement
```

`Contents/_CodeSignature` 里有 `CodeResources`。

`scripts/check-release-app.sh` 退出码 0：

```
主二进制含 devUrl 127.0.0.1:1420。预览地址不在。release 使用 frontendDist，不加载这个地址。
app ok target/aarch64-apple-darwin/release/bundle/macos/万象小助手.app
```

`lipo` 仍是 `arm64`。PlistBuddy 四个字段仍是 `local.wanxiang.assistant`、`0.1.0`、`0.1.0`、`12.0`。同回报-05。

`npm test`：14 passed，0 failed。日志里有 `WebSocket server error: Port 24678 is already in use`，没有失败。

## 计划

`PLAN-M6.md` 开头的「2026-11-11 变更」写明：本机包用 ad-hoc 签名（`"-"`）把整个包签好。`"-"` 来自 Tauri 官方 macOS 签名文档的 Ad-Hoc Signing。环境变量能否覆盖配置里的 `"-"`，官方文档没有写明，标为未验证。第 2、3、8 节的 Developer ID 与 Gatekeeper 验收仍暂缓。

## 构建命令的工作目录（已实测）

第一次构建失败。npm 调试日志里的 cwd 是仓库根目录，不是 `app/`。当时的命令和错误：

```
     Running beforeBuildCommand `npm --prefix .. run build`
npm error code ENOENT
npm error syscall open
npm error path $HOME/Projects/package.json
npm error errno -2
npm error enoent Could not read package.json: Error: ENOENT: no such file or directory, open '$HOME/Projects/package.json'
beforeBuildCommand `npm --prefix .. run build` failed with exit code 254
       Error beforeBuildCommand `npm --prefix .. run build` failed with exit code 254
```

依据这段日志，Tauri 在仓库根目录执行 `beforeBuildCommand` / `beforeDevCommand`。已改为 `npm run build` 和 `npm run dev`。`frontendDist` 仍是 `../ui/dist`，`devUrl` 仍是 `http://127.0.0.1:1420`。

`@tauri-apps/cli` 固定为 2.12.4，写入开发依赖。`package.json` 增加 `"tauri": "tauri"`。没有再使用 `npx --yes` 下载。

## 只有链接器签名的那一次构建

`env | awk -F= '/^APPLE_/ {print $1}'` 输出为空。

在仓库根目录：

```
npm run tauri -- build --target aarch64-apple-darwin --bundles app,dmg
```

CLI 找到了 `app/tauri.conf.json`。退出码 0。没有弹出 Finder 控制授权。没有设置任何 `APPLE_` 变量。

产物：

- `~/Projects/Wanxiang/target/aarch64-apple-darwin/release/bundle/macos/万象小助手.app`
- `~/Projects/Wanxiang/target/aarch64-apple-darwin/release/bundle/dmg/万象小助手_0.1.0_aarch64.dmg`

```
 32M	万象小助手.app
 22M	万象小助手_0.1.0_aarch64.dmg
03028bb11846a6c3bde641ee6d9c603a853a10fa5d98f9c5c24f81399b7284b0  万象小助手_0.1.0_aarch64.dmg
```

`shasum -a 256` 的对象是上面的 dmg。`du -sh` 的对象是 .app 和 .dmg。

## 验收

`codesign -dv --verbose=4` 对 .app，只留这几行：

```
Identifier=wanxiang_app-262738b0d703c27e
CodeDirectory v=20400 size=261208 flags=0x20002(adhoc,linker-signed) hashes=8159+0 location=embedded
Executable Segment flags=0x1
Signature=adhoc
```

`codesign --verify --deep --strict --verbose=2` 对 .app，失败，未修：

```
万象小助手.app: code has no resources but signature indicates they must be present
```

`lipo -archs` 主二进制 `Contents/MacOS/wanxiang-app`：

```
arm64
```

PlistBuddy：

```
local.wanxiang.assistant
0.1.0
0.1.0
12.0
```

顺序是 `CFBundleIdentifier`、`CFBundleShortVersionString`、`CFBundleVersion`、`LSMinimumSystemVersion`。

icns 解到 `$TMPDIR`。`icon_512x512@2x.png` 的 `pixelWidth` 是 1024。

`scripts/check-release-app.sh` 退出码 0，完整输出：

```
主二进制含 devUrl 127.0.0.1:1420。预览地址不在。release 使用 frontendDist，不加载这个地址。
app ok target/aarch64-apple-darwin/release/bundle/macos/万象小助手.app
```

`npm test`：14 passed，0 failed。日志里有一行 `WebSocket server error: Port 24678 is already in use`，测试仍通过。

`cargo test --workspace --offline`：退出码 0。各包 `test result` 合计 189 passed，0 failed。
