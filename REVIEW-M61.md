# M6.1：交互修复、视觉还原、Dock 图标（Claude 直接修改）

分支 `m6-release`，在 M6 准备改动之上。未提交，未推送，未签名证书，未公证。
起因：用户打开 0.1.0 本机包后提出四点——空间页其他工具点了没反应；额度页等组件和 Figma 差距大；Dock 图标显小、没有层次；要做完再看。

## 交互

- **空间页**（`SpacePage.tsx`）：四个工具行都是可选中按钮（`aria-pressed`），右侧详情随选中工具切换。
  - Harness：保留原有清理流程，勾选只属于 Harness，切换工具后保持；确认令牌仍只给 Harness。
  - Cursor、ChatGPT：只统计不清理，没有清理按钮；未扫描时说明原因。
  - DS Bot：说明已计入 Harness，「查看 DeepSeek Harness」跳回。
  - 工具图标改用运行时读取的真实图标（和侧栏同一来源），读取失败才显示字母。
- **自定义日期**（新 `DateRange.tsx`、`date-range.css`）：替换浏览器原生 `<input type="date">`。胶囊按钮显示「11月1日 – 11月12日」，点开是月历范围弹层；切到「自定义」时默认沿用当前范围（原来两端都是今天，和副标题对不上）；不能选未来日期，先后颠倒自动对调，Esc 关闭、方向键移动。Figma 里没有这个控件，按 22:57 规范做，待确认后补进组件区 24:325。
- **额度卡标题**（`crates/core/src/present.rs`）：只读到一个窗口时，缺的那张写明是哪个窗口；未授权时两张卡也保留「ChatGPT · 5 小时窗口 / 7 天窗口」，不再出现「ChatGPT · 窗口」。顺序 5 小时在前（Figma 18:320）。
- **用量标题**随范围变化：今天 / 7 天 / 30 天 / 本月 / 所选范围可核实用量（原来固定写「本月」）。
- **概览的对话、空间卡**（新 `OverviewCards.tsx`）：原来正式包里永远显示「尚未接入」「尚未扫描」，只有夹具才有数字。现在对话卡显示上下文占用最高的对话；空间卡显示空间页最近一次扫描结果（带阈值刻度和图例），超阈值的待办也用真实扫描结果。
- **侧栏状态点**按 22:57「数据」三色：正常雾蓝、需要处理赭色、未连接或过时灰。概览工具行仍按授权状态。

## 视觉（逐节点读 Figma design context 后改）

- 空间页 20:223 / 20:227：指标卡数字 24 半粗 + 单位 13；工具列表行内边距 12、选中雾蓝 9%、图标位 30；详情标题 20 粗 + 图标 40；堆叠条 10 高、2 间隔、8×8 图例；条目副标题为路径 · 说明；页脚提示与「清理所选项…」按钮。
- 额度页 18:320：提示条加警示图标、赭色 8% 底、石墨正文、「去设置授权」链接；小节标题 15 中等；卡片阴影统一为 `--shadow-card`；明细来源列 12 次要色。
- 概览 18:214 / 18:240：三卡等宽；数字 40 半粗 + 单位 18；进度条 248×6；底部分隔线和图例。
- 弹窗 21:198：补回边框；结果点只用三主色（原失败点用了 #C4554D，不在规范里）；图标 22。
- 新增 token：`--shadow-card`。

## Dock 图标

- 原因：Tahoe（macOS 26）对超出系统圆角轮廓的旧式 .icns 会缩小并加灰底。原图主体是普通圆角矩形，外加一圈投影，正属这种情况（依据：heise、9to5Mac、Apple 开发者论坛对 Tahoe 图标的说明）。
- `app/icons/Wanxiang.icon`：Icon Composer 分层图标（底盘 + 三段圆环 + 中心点，环与点开玻璃效果）。`tauri.conf.json` 的 `bundle.icon` 加入它；tauri-bundler 2.9 起会用 actool（需 Xcode 26）编译成 Assets.car（依据：tauri-bundler 源码 `bundle/macos/icon.rs`）。icon.json 字段按真实 Icon Composer 文件整理的样例书写，**未在本机实测**，由 `scripts/m61-mac-verify.sh` 验证。
- `brand/icon-macos.svg`（.icns 来源，旧系统）：主体改为 Apple 连续圆角（半径 184.5，内收 2px）；圆环放大 1.34 倍，外径约占主体 64%（原约 48%）；三段环与中心点加明暗和高光；只用三主色的明暗。生成脚本 `scripts/gen-macos-icon.py`，对照图见 `design/diff/m61-icon.png`。
- Figma 品牌页 24:54 待同步新图标。

## 测试

- 新增：`ui/date-range.test.mjs`、`ui/overview-cards.test.mjs`；扩充 `space.test.mjs`（四个可选中工具、Cursor/ChatGPT/DS Bot 无清理按钮）、`tool-status.test.mjs`（三色点规则）、`usage.test.mjs`；Rust `crates/core/tests/present.rs` 两条窗口标题测试。
- Claude 在 Linux 环境用 esbuild 代替 vite 跑了全部界面测试（release-strip 除外）和 tsc，全部通过；Chrome 里点击验证了工具切换、勾选保持、日期弹层。
- Rust 测试、release-strip、打包、Assets.car 只能在 Mac 上跑：`scripts/m61-mac-verify.sh`，日志写到交接文件夹「对话/回报-M61-mac.md」。

## 仍未对齐（有原因）

- Harness 受保护部分在 Figma 分成「会话记录 / Bot 与群聊记忆 / 账户、凭据与设置」三行；扫描器目前只给出受保护总量，界面显示一行合计。要分项需要改扫描器分类，另行处理。
- Figma 仍写 Codex，应用按决定显示 ChatGPT。字体按规范用系统字体（PingFang SC / SF），Figma 用 Noto Sans SC / Inter。
