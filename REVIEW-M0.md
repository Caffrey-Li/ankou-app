# REVIEW-M0

里程碑：工程骨架。没有开始 M1。没有执行 `git init`，没有提交，没有安装或替换应用。

## 改了什么

在 `/Users/caffrey/Projects/Wanxiang` 搭了 Tauri 2 工程。Rust workspace 成员：

- `crates/core`：`Supervisor`，退出时停后台任务和子进程
- `crates/adapters/dsh`
- `crates/adapters/cursor`
- `crates/adapters/codex`
- `app`：Tauri 窗口、菜单栏 / 托盘、退出

界面在 `ui/`，React + TypeScript。设计令牌在 `ui/src/tokens.css`。品牌矢量在 `brand/`，来自 Figma 组件导出，再光栅化到 `app/icons/`。

三个适配器只返回「未验证」，不读本机数据。应用里没有 `fetch`、没有 HTTP 客户端。刷新菜单项不发请求。

窗口 1280×900。侧栏是五个导航的空框架，工具位用字母占位。点导航只换标题，不进入后续里程碑的页面。关闭窗口会隐藏；菜单「退出」先 `shutdown()` 再退出。macOS 点 Dock 会重新显示窗口。

CI：`.github/workflows/ci.yml`，`macos-latest` 和 `windows-latest`，跑 `npm ci`、`npm test`、`npm run build`、`cargo test --workspace --locked`、`cargo build --locked -p wanxiang-app`。

## 命令和输出摘要

前端：

- `npm install react react-dom`：added 3 packages
- `npm install -D vite @vitejs/plugin-react typescript @types/react @types/react-dom @resvg/resvg-js`：added 23 packages
- `node scripts/render-icons.mjs`：写出 `app/icons/` 的 png、ico、icns
- `npm test`：3 项通过（令牌、图标文件、源码不含网络客户端和凭据路径）
- `npm run build`：`ui/dist/index.html` 0.40 kB，CSS 4.11 kB，JS 222.38 kB

Rust：

- 本机 stable 工具链原先不完整（目录里只有 `cargo`，`rustc -vV` 报 missing manifest）。执行了 `rustup toolchain install stable --profile minimal`。装上的是 rustc 1.99.1（2026-10-13）。因为之前没有默认工具链，rustup 把默认工具链设成了 `stable-aarch64-apple-darwin`。这是安装过程的副作用，不是单独改的配置。
- `cargo test --workspace --locked`：全部通过，编译无 warning。适配器各 1 个测试，`shutdown` 3 个测试（停 sleep 子进程、停协作任务、重复调用），最后一轮 shutdown 测试 0.13s。
- `cargo build -p wanxiang-app --locked`：通过。

界面：

- `npm run dev` 后在浏览器打开 `http://127.0.0.1:1420`。点了「设置」和「额度与用量」，标题跟着变，选中态跟着变，仍显示「未验证」，没有数字。
- `cargo run -p wanxiang-app`：原生窗口打开，侧栏、标志、五个导航和字母占位都在。该进程没有子进程。之后发了 SIGTERM，进程退出。菜单里的「退出」没有在界面上点过。

Figma 只通过 Figma MCP 读了文件 `ui7zTemjbm3XTVMZh69KKB`。没有用 curl 下载截图 URL。

## 未验证

- GitHub Actions 没有跑。仓库还没有 `git init`，也没有推送，所以「两端 CI 绿」还没发生。
- Windows 构建、托盘图标、主题切换、`icon.ico` 都没有在 Windows 上运行。
- macOS 菜单栏图标已在启动时设成 18×18 模板图（启动没报错，说明托盘创建成功），但没有在菜单栏上目视确认。Retina 的 @2x 表示没有单独嵌入。
- 没有点菜单「退出」。子进程停止的证据是单测：`sleep 30` 在 `shutdown()` 后进程不存在。这次图形进程本身没有子进程。
- 没有做安装包，没有签名，没有替换已安装的应用。
- 没有读 QuotaPot，没有读 Cursor 或其他工具的本机数据目录，没有读凭据。

## 和设计稿的差异

按 01 设计规范 `22:57`、品牌 `24:54`，窗口尺寸参考概览 `18:74`，菜单行为参考 `21:291`。

- 快速面板没做。设计稿里的示例数字（84%、8.2 GB、¥128.40）不会出现。菜单是系统菜单：打开万象、刷新（未验证）、退出。
- 待处理圆点没做。M0 没有事项。
- 侧栏导航没有设计稿里的线性图标，只有文字。
- 工具图标是 H / B / C / Cx 字母占位，状态写「未检测」。没有去读本机已安装应用的图标。
- 窗口外框圆角交给系统，没有再自绘一层 24px。红黄绿按钮也是系统的。
- 规范色板的窗口底色是 `#F5F5F7`，材质说明的 L1 是 `#F7F7F8`。概览窗口 `18:74` 实际是 `#F9F9FA` 到 `#F4F4F6` 的竖向渐变。三个值都收进令牌；壳层用的是 `18:74` 的渐变。
- 侧栏是白 55% 加 30px 模糊，圆角 16。这是 `18:75` 的实测。规范文字写的是白 55–62%。窗口不透明，模糊看不到桌面壁纸。
- 选中导航是雾蓝 12%、圆角 9px，来自 `18:83`。9px 不在圆角阶梯 24 / 22 / 16 / 12 / 全圆里。卡片另有 20px 令牌，来自材质说明里的「16/20」，圆角行只写了 16。
- 彩色浅底用同色 12%，落在规范写的 8–13% 里。
- 字体按规范用系统字体（PingFang SC、SF Pro、Microsoft YaHei UI、Segoe UI Variable），没有嵌入设计稿用的 Noto Sans SC 和 Inter。数字用 tabular-nums，壳层里没有大数字。
- macOS 应用图标按 Figma 导出的 SVG 光栅化。主体圆角是 185。节点上的连续圆角平滑 0.6 没有出现在 SVG 导出里，所以图标里也没有。没有用 Icon Composer 做系统玻璃。
- Windows 图标：32 及以上用完整三段圆环，24 及以下用两段简化版。托盘是单色，浅色 `#1D1D1F`，深色白色，16px。这些文件都生成了，没有在 Windows 上看过。
- macOS 菜单栏模板图是纯黑 18×18。设计稿说有待处理时加圆点；当前没有这个状态。
