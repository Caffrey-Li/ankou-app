# M6 计划：macOS 打包、签名、公证

## 2026-11-11 变更

这台 Mac 上没有 Developer ID 证书。用户决定先出未签名的本机包。第 2、3 节（签名、公证）和第 8 节的 Gatekeeper 验收暂缓，以后有了证书再做。其余各节不变。本机包用 ad-hoc 签名（`"-"`）把整个包签好。依据是 Tauri 官方 macOS 签名文档的 Ad-Hoc Signing：把 `signingIdentity` 设为 `"-"`。以后有了 Developer ID 证书，打算用环境变量 `APPLE_SIGNING_IDENTITY` 覆盖它。官方文档只写身份可以来自配置或该环境变量，没有写环境变量会覆盖已经写在配置里的 `"-"`。覆盖这一点未验证。

这一步只定计划。不改代码，不打包，不签名，不公证，不创建或导出证书，不运行 `notarytool`。执行前另一次确认；上传给 Apple 的那一步要单独再问一次。

依据只用这三个词：官方文档、已核对代码、未验证。本机还没有打过已签名的正式包，所以签名、公证、首次打开的系统提示都还没有实测。

当前配置（已核对代码）：

- `app/tauri.conf.json`：`productName` 是「万象小助手」，`version` 是 `0.1.0`，`identifier` 保持 `local.wanxiang.assistant`。`devUrl` 仍是 `http://127.0.0.1:1420`，只给开发构建用；`frontendDist` 是 `../ui/dist`。CSP 不改。`bundle.targets` 仍是 `all`，打包时用 `--bundles app,dmg` 收窄。`bundle.macOS.minimumSystemVersion` 是 `12.0`，`hardenedRuntime` 显式为 `true`，`entitlements` 指向 `app/entitlements.plist`（空的 `dict`）。不写 `signingIdentity`。
- `app/Cargo.toml` 的 Tauri 打开了 `macos-private-api`。`tauri.conf.json` 的 `macOSPrivateApi` 是 `true`。Tauri 2.12 的配置说明写：这个开关打开透明背景，并在 2.12.1 之后不再额外做事，因为相关接口已经总是打开。本仓库的 crate 版本是 `tauri = "2"`，锁定的具体小版本执行时再读 `Cargo.lock`。
- 数据库在 `app.path().app_data_dir()` 下的 `wanxiang.sqlite`（`app/src/commands.rs`）。Tauri 在 macOS 上把这个目录放在 `~/Library/Application Support/` 加上 identifier。早期记录里见过这个目录。
- 钥匙串服务名是源码里的常量 `local.wanxiang.assistant`，账户名 `deepseek`（`crates/core/src/secret.rs`）。它不是运行时从包标识读出来的。

## 0. 执行顺序

下面每一步都等上一节的选择定了再做。

1. 标识符、架构、版本号已定，见第 1、4、5 节。entitlements 仍是空的 `dict`。
2. 发布剥离的前置改动在 `m6-release` 上。剥离验收不过就不打包。
3. 只签名、不上传，跑剥离脚本和 entitlements 复查。
4. 用户自己把公证凭据存进钥匙串配置。
5. 单独同意之后，才 `notarytool submit`。
6. `stapler staple`，然后 `spctl -a -vv`。
7. 用户亲手做首次打开。代理不点任何系统对话框。

## 1. 标识符

已定：保持 `local.wanxiang.assistant`。不改 identifier，不改钥匙串服务名 `local.wanxiang.assistant`，不迁移数据库。

它同时是 macOS 的 `CFBundleIdentifier` 和应用数据目录名。官方文档：Apple 要求 bundle identifier 用反向域名形式，只含字母、数字、连字符和点（[CFBundleIdentifier](https://developer.apple.com/documentation/bundleresources/information-property-list/cfbundleidentifier)）。Developer ID 在 Mac App Store 之外分发时，公证不要求事先在门户里注册 App ID。`local.` 前缀能不能通过公证，未验证。若第一次提交被拒，停下来再定，不在这次里换标识符。

开发版和正式版共用这一个标识符，所以也共用同一个应用数据目录 `~/Library/Application Support/local.wanxiang.assistant`，以及里面的 `wanxiang.sqlite`。这是有意的：正式包继续用已经写下的设置、授权和 `setup_complete`，不另建一套库。钥匙串服务名也是同一个常量。

验收时要注意：

- 不要同时打开开发版和正式版。两边写的是同一个库。
- 本机第一次打开正式包不是全新安装。打开前先看这个库在不在，并写进记录。不为了做出「第一次」而删库，除非用户另外同意。
- 开发版里已经勾过的授权和设置，正式版会直接读到。
- `defaults read` 该包的 `Info.plist`，`CFBundleIdentifier` 必须是 `local.wanxiang.assistant`。不打开应用去写用户目录。

## 2. 签名

证书类型用 Developer ID Application，在 Mac App Store 之外分发。官方文档：[Tauri macOS Code Signing](https://v2.tauri.app/distribute/sign/macos/) 写明店外分发用这个证书，并且使用它时必须公证。证书要已经在登录钥匙串里。执行时用 `security find-identity -v -p codesigning` 让用户指出哪一条，写入当次环境变量 `APPLE_SIGNING_IDENTITY`。不把身份字符串、Team ID 写进仓库。本计划不创建、不导出证书。

Hardened Runtime 打开。Tauri 的 `bundle.macOS.hardenedRuntime` 默认就是 `true`（[配置说明](https://v2.tauri.app/reference/config/)）。执行时在配置里写明 `true`，避免以后默认值变化。Apple 从 macOS 10.15 起，公证要求开启 Hardened Runtime（[Hardened Runtime](https://developer.apple.com/documentation/security/hardened-runtime)）。

Entitlements 文件单独放，例如 `app/entitlements.plist`，由 `bundle.macOS.entitlements` 指向它。起始内容是空的 `dict`。不加 App Sandbox，也不加下面任何一项：

- `com.apple.security.cs.disable-library-validation`
- `com.apple.security.cs.allow-unsigned-executable-memory`
- `com.apple.security.cs.allow-dyld-environment-variables`
- `com.apple.security.cs.disable-executable-page-protection`
- `com.apple.security.get-task-allow`

这些会放宽代码签名或调试限制。M6 不加。

`com.apple.security.cs.allow-jit` 先不加。WKWebView 的 JavaScript 跑在系统自己的 WebContent 进程里，宿主应用不一定需要这项。未验证。签名后若窗口是白的或脚本不执行，停下来，把现象记下来，再单独问要不要只加这一项。不把它和其他放宽项绑在一起。

为什么不开 App Sandbox：沙箱是上架 Mac App Store 的要求，不是 Developer ID 直接分发的要求（[App Sandbox](https://developer.apple.com/documentation/security/app-sandbox)）。这个程序要读 `~/.dsh`、`~/.codex` 和 Cursor 的用户目录，还要按授权根打开用户选定的路径、拉起本机的 `codex` 命令、用钥匙串、从 Rust 访问 DeepSeek 的接口。沙箱会把这些挡在容器外。不使用 `com.apple.security.app-sandbox`，也就不需要配套的网络、文件、Apple Events 沙箱项。

`macos-private-api` 是为了透明窗口和侧栏材质，已核对代码。它不自动等于上面那些放宽签名的 entitlement。签名后用 `codesign -d --entitlements :-` 把实际清单打出来，多出来的项先停，不拿去公证。

验收：

- `codesign -dv --verbose=4` 对 `.app` 显示 Authority 为 Developer ID Application，flags 含 `runtime`。
- `codesign -dv` 对 `.dmg` 同样显示这个签名身份。dmg 是容器签名，和 app 的签名分开看。
- `spctl -a -t open --context context:primary-signature -vv` 对 `.dmg`。上传前运行并记下输出。公证前它还不会是 Notarized；这一步只确认命令能对 dmg 的主签名做评估。公证后的通过标准在第 3 节。
- `codesign -d --entitlements :-` 的键与本节批准的清单一致，没有禁用库验证的项。
- `codesign --verify --deep --strict` 对 `.app` 通过。

## 3. 公证

用 `xcrun notarytool`，不用旧的 `altool`。官方说明见 [TN3147](https://developer.apple.com/documentation/technotes/tn3147-migrating-to-the-latest-notarization-tool)。

凭据由用户自己执行 `xcrun notarytool store-credentials`。配置名建议 `wanxiang-notary`，用户可以改。计划和以后的脚本只引用这个配置名，或引用环境变量 `WANXIANG_NOTARY_PROFILE`（值是配置名）。不出现 Apple ID、密码、Team ID，也不出现 App 专用密码、API 密钥路径。

Tauri 在设置了 `APPLE_ID`、`APPLE_PASSWORD`、`APPLE_TEAM_ID` 时会自己公证（同上 Tauri 签名文档）。M6 不设置这三个变量，避免构建过程拿密码去上传。签名和公证分开：构建只签名；上传用下面的命令。

第一版只把 dmg 提交公证一次，也只对 dmg 做 `stapler staple`。不单独再提交 app。

```sh
xcrun notarytool submit "<dmg 路径>" --keychain-profile "$WANXIANG_NOTARY_PROFILE" --wait
xcrun stapler staple "<dmg 路径>"
spctl -a -vv "<app 路径>"
codesign -dv "<dmg 路径>"
spctl -a -t open --context context:primary-signature -vv "<dmg 路径>"
```

这样做的限制：票据订在 dmg 上，dmg 里的 app 没有自己的票据。用户把 app 拖出来之后，离线打开时 Gatekeeper 在 app 上查不到票据，会拦下。机器能联网时，Gatekeeper 可以向 Apple 查询这次公证，打开没有问题。未在本机实测这份包；限制本身来自票据只跟着被 staple 的那个文件走。

以后可选的做法：先把 app 提交公证并 `stapler staple`，再装进 dmg，对 dmg 再提交一次。这样拖出来的 app 离线也能过。那是两次上传，每一次都要单独征得同意。第一版不这么做。

公证日志若出现私有 API 或 entitlements 警告，先把日志里的结论记下来，不 staple，等用户看过。

验收：`notarytool log` 的状态是 Accepted；`stapler validate` 对 dmg 成功，不对 app 要求已经订上票据。`spctl -a -vv` 对 app 在联网评估时为 accepted。`codesign -dv` 对 dmg 仍显示 Developer ID。`spctl -a -t open --context context:primary-signature -vv` 对 dmg 为 accepted，来源含 Notarized Developer ID。

## 4. 架构

已定：只出 arm64。命令是 `tauri build --target aarch64-apple-darwin --bundles app,dmg`。到目前为止的 macOS 验证都在这台 Apple 芯片电脑上。x86_64 未验证，第一份包不包含它。

验收：`lipo -archs` 对主二进制只列出 `arm64`。`scripts/check-release-app.sh` 也做这一项。`Info.plist` 的 `LSMinimumSystemVersion` 仍是 `12.0`。

## 5. 产物、版本、图标

产物只保留 `.app` 和 `.dmg`。配置里的 `targets: all` 在 macOS 上还会带上 Tauri 默认的其他包；执行时用 `--bundles app,dmg` 收窄。不启用自动更新。

版本号已定：`0.1.0`。

- 唯一来源是 `app/tauri.conf.json` 的 `version`，现为 `0.1.0`。
- `CFBundleShortVersionString` 与它相同。`CFBundleVersion` 用同一字符串，并且只增不减。
- 改版本号不迁移数据。下一次要改数字时再问。

图标：

- 来源是 `brand/icon-macos.svg`，画布是 1024×1024。已核对文件。
- `scripts/render-icons.mjs` 用它生成 icns，其中 `icon_512x512@2x.png` 是 1024。已核对脚本。
- 菜单栏图标来自 `brand/tray-macos-template.svg`，18×18，只有黑色和透明，`lib.rs` 里 `icon_as_template(true)`。已核对代码。不改图形。

验收：

- `iconutil -c iconset` 解开 app 里的 icns，`sips -g pixelWidth` 确认有 1024 的那张。
- 菜单栏 png 的像素只有黑和透明，尺寸仍是 18×18。
- dmg 里的 app 与单独的 app 是同一次构建。

## 6. 发布剥离

现有 `ui/release-strip.test.mjs` 检查的是 vite 产物和 `cargo build --release` 的主二进制。M6 再对**已签名的 .app** 做一次，因为用户拿到的是包，不是 cargo 的 target 目录。脚本是 `scripts/check-release-app.sh`。输入是 `.app` 路径；`--binary` 只检查 release 主二进制。可以重复跑。这一步只写脚本。签名完成前不拿 `.app` 去跑。对包内每个文件做字符串搜索，至少包括主二进制、`Resources` 里的前端、`Info.plist`。

必须找不到，找到就失败：

- `WANXIANG_VERIFY`
- `verify_enabled`
- `verify_set`
- `WANXIANG_SPACE_FIXTURE`
- `WANXIANG_PREVIEW`
- `fixture=m2`、`fixture=m3`、`fixture=m4`、`fixture=handoff`
- `get("bot")`、`get("codex")`
- `open_preview` 用过的预览地址（带 `?page=` 且指向 `127.0.0.1:1420` 的那些）

已核对代码，正式构建里本来就不应出现的：

- `open_preview` 的地址包在 `#[cfg(debug_assertions)]` 里（`app/src/lib.rs`）。release 编译会去掉。
- `WANXIANG_SPACE_FIXTURE` 的读取也在 `debug_assertions` 里（`app/src/measured.rs`）。release 里对应函数直接返回空。
- `WANXIANG_VERIFY` 的命令和能力文件已从源码去掉。`release-strip` 已在查它。

`WANXIANG_CURSOR_FIXTURE` 和 `WANXIANG_SKIP_REFRESH` 已按 `WANXIANG_SPACE_FIXTURE` 的做法，只留在 `#[cfg(debug_assertions)]` 里。debug 构建的行为不变。release 二进制里这两个字符串也必须没有，`ui/release-strip.test.mjs` 和 `scripts/check-release-app.sh` 都会查。查到就不上传。

`127.0.0.1:1420` 还会留在嵌入的 Tauri 配置里，因为 `devUrl` 写在 `tauri.conf.json`，`generate_context!` 会把配置编进二进制。官方行为是 release 用 `frontendDist`，不用 `devUrl`。脚本对这个地址的规则是：前端资源和 `Info.plist` 里不得出现；主二进制里若出现，打印命中位置。只有在预览地址（带 `?page=`）不存在、并且本机 1420 没有进程时应用仍能打开，才把它记成「配置里有，运行时不可达」。若前端资源里出现，直接失败。

验收：脚本退出码 0，并把命中列表写进当次记录。失败则不公证。

## 7. 权限和安全复查

已核对代码，维持现状，签名时不放宽：

- `app/capabilities/default.json` 只有 `core:event:allow-listen`、`core:event:allow-unlisten`、`core:window:allow-start-dragging`，以及各个 `allow-<命令>`。没有 `opener:`、`dialog:`、也没有任何一段叫 `default` 的权限。`core:default` 不在里面。测试 `capability_has_no_opener_dialog_or_plugin_defaults` 锁着这一点。
- 插件仍在 `lib.rs` 里 `init`。打开链接和保存对话框只发生在 Rust 命令内部。窗口能力不能直接调用它们。
- CSP 保持现在这一条：`connect-src` 只有 `ipc:` 和 `http://ipc.localhost`。不改。
- 正式包不加载 `devUrl`。`beforeBuildCommand` 先跑 `npm run build`，再把 `ui/dist` 打进包。

验收：

- 对已签名 `.app` 再搜 `opener:default`、`dialog:default`、`core:default`、`opener:*` 这种默认权限名，必须没有。
- 包内前端的 CSP 元数据或 Tauri 配置与 `tauri.conf.json` 里现有字符串一致。
- 构建机上不设置 `APPLE_ID`、`APPLE_PASSWORD`、`APPLE_TEAM_ID`。
- 1420 端口没有服务时，正式包仍能画出界面。这一步在用户自己的首次打开里一起看，代理不代点。

## 8. 首次运行

代理不启动这份已公证的包，也不点对话框，也不给文件写隔离属性。用户按下面做，把每个对话框的原文记下来。没有列出的对话框出现时，先停，不要点。

本机 `tauri build` 打出来的 dmg 不带 `com.apple.quarantine`。没有这个属性时，Gatekeeper 不检查，直接双击打开不算首次运行验收。

打开前先确认隔离属性真的在：

1. 对刚打出来的 dmg 跑 `xattr -p com.apple.quarantine`。没有输出，说明这还是构建产物，不能拿来当验收。
2. 由用户自己处理这份 dmg：用浏览器把它下载一次，或者用户自己给它写上隔离属性。代理不执行 `xattr -w`。
3. 再跑 `xattr -p com.apple.quarantine`。必须打出非空的一行。没有就停，不要挂载。
4. 挂载，把 app 拖进「应用程序」。不要从 dmg 里直接运行。
5. 对「应用程序」里的那份 app 再跑 `xattr -p com.apple.quarantine`，确认拖出来之后属性还在，然后再打开。
6. 第一次双击「应用程序」里的那份。若系统说已验证开发者，直接打开。若出现「无法打开，因为无法验证开发者」，停下来，不要用右键打开绕过；回到第 3 节看 `spctl` 和 staple。离线被拦下，而联网能打开，符合第 3 节只公证 dmg 一次的限制，记下来即可。
7. 钥匙串。未签名的调试包曾经问过能否使用服务名 `local.wanxiang.assistant` 的项（早期记录）。签名身份换了之后，正式包很可能再问一次。未验证。要沿用已经保存的密钥就点「允许」。点「拒绝」时，程序按没有密钥继续，不写新项。两种都由用户决定。不要点「始终允许」，除非用户自己要。
8. 文件访问。`~/.dsh`、`~/.codex` 和 Cursor 用户目录在以前的只读扫描里没有另外弹出「文件与文件夹」授权。那是另一份未签名程序，正式包是否同样不弹，未验证。若弹出，先把原文记下来再决定，不默认点允许。
9. 不在这次验收里勾选新的工具授权，不点清理，不点「允许」以外的钥匙串选项，除非上一款明确写了。深链只有在用户自己点「放入 Cursor」时才可能问要不要打开 Cursor；首次打开不应出现。

验收记录：每个对话框的原文、用户点了什么、主界面是否出现。没有对话框也要写明。

## 9. 回滚和不可逆步骤

执行前单独同意的，只有这一件不可逆的事：

- `notarytool submit` 会把包上传给 Apple。票据可以以后失效，但上传本身撤不回。同意范围只覆盖这一次提交的那一个文件。失败后的重试也要再问。

这些不是上传，但做了就会留下痕迹，同样先问：

- 在开发者账号里新建 Developer ID 证书。本计划假定证书已经在钥匙串里。没有的话停下来，不在这一步创建。
- 标识符已定为 `local.wanxiang.assistant`。发出去之后再改，就要另做数据迁移，不在本计划里。

可以重做、不需要单独的「不可逆」同意：

- 本地签名。重新构建会换成新签名。
- `stapler staple`。只改本机文件。
- `store-credentials`。写在用户自己的钥匙串里，用户可以自己删掉该配置。脚本不代跑这条命令。

不推送，不把 dmg 放到仓库里。
