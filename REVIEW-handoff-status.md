# 对话页状态

分支 `m4-handoff-status`，从 `m4-verify` 的 `cadff03` 拉出。未提交，未推送。`VERIFY-M4.md` 里原有的未提交更正没有动。

## 改了哪些文件

- `app/src/handoff.rs`：每个来源一个状态，不再带自由文本 `notes`。
- `ui/src/api.ts`、`ui/src/__fixtures__/ipc-names.json`、`ui/ipc-names.test.mjs`：契约名 `source_status`。
- `ui/src/HandoffPage.tsx`、`ui/src/App.tsx`：空列表按来源显示原因；副标题按授权显示。
- `ui/handoff-status.test.mjs`：前端状态组合、读取中、列表失败。
- `ui/release-strip.test.mjs`：正式包里夹具参数不生效。
- `design/native/m4-handoff-unauthorized.png`：真实应用，当前是未授权。本轮没有重拍。
- `design/native/m1b-setup.png`：覆盖。图上是 ChatGPT。这是这台电脑上已经有授权的状态，不是全新安装。本轮没有重拍。
- `design/actual/handoff-mixed.png`：本轮重拍。
- `design/actual/handoff-read-failed.png`：本轮新拍。
- `design/actual/handoff-server-unavailable.png`、`design/actual/handoff-no-records.png`：上一轮夹具，本轮没有重拍。副标题和「没有本机对话」的底色以 mixed 这张为准。

开发服务器上，`fixture=handoff` 加上 `codex`、`bot` 可以摆出其余状态。正式包里这两个参数不生效。

## 每种状态的依据

取值是 `unauthorized`、`needs_reauth`、`read_failed`、`server_unavailable`、`no_records`、`ok`。授权匹配没改：仍只认对话能力。读取时机没改：没开本机服务就不调；找到命令行才调 `list_threads`；服务没有给出列表时，仍按原来的本地回退读一次。`NeedsReauth` 不进入这段读取。

只有完全没有对话授权时才是 `unauthorized`。不读本地列表，也不调 app-server。

`needs_reauth`：这个来源有对话授权，但状态是 `NeedsReauth`（根目录被换过，恢复时守卫拒绝）。不把这次拒绝当成读取失败，也不调 app-server。文案是「授权需要重新确认，可在设置里处理」。同一工具若同时还有一条有效授权，按有效授权读取，不标成需要重新确认。

`read_failed`：已有有效授权，但本地列表或数据库没有读成。守卫拒绝和 IO 错误都算。测试里用不可读的会话文件和打不开的数据库。文案是「读取失败，没有读到本机会话」。有效授权下如果没有形成一次成功读取，也归这里，避免再显示成未授权。服务曾经调过并且失败、随后本地读取又失败时，状态是 `read_failed`，不是「应用未运行」。

`server_unavailable`：只在真正调用过 app-server 并且失败、随后本地列表成功但为空时使用。命令行没找到或开关没开，都算没调过，不能给这个状态。文案仍是「ChatGPT 应用未运行，没有读到本机会话」。

`no_records`：有效授权，读取成功，列表是空的。没调过 app-server 时的空列表也是这个状态。文案是「没有本机对话」。

`ok`：服务或本地列表里有对话。服务失败但本地仍有记录时是 `ok`。

DS Bot 从不调 app-server，所以不会是 `server_unavailable`。

空列表时每个非 `ok` 来源一行，前面带显示名。`ok` 不写空态说明。写死的「应用未运行」兜底已删。

底色：`unauthorized`、`needs_reauth`、`read_failed`、`server_unavailable` 用原来的提示底色。`no_records` 用和 `handoff-note` 相同的中性底色。

副标题：两个来源都是 `unauthorized` 时只写「未授权」，不带数字。否则恢复为「N 条对话 · M 条超过提醒阈值」。来源还是 `null` 时表示读取中，副标题和空态都不写状态文案。`listConversations` 失败时两个来源都是 `read_failed`，不是未授权。

## 正式包里的夹具参数

`fixture=handoff` 以及 `codex`、`bot` 只写在 `import.meta.env.DEV` 分支里。`ui/release-strip.test.mjs` 检查 vite build 产物不含 `get("bot")`、`get("codex")` 和 `fixture=handoff`。这三条字符串只出现在那段参数读取里。测试通过，说明这段在正式包里已被去掉。状态文案本身会留在包里，因为真实界面要用。

## 测试

- `cargo test --workspace --offline`：189 通过，0 失败，退出码 0。
- `npm test`：14 通过，0 失败。
- `tsc --noEmit -p ui/tsconfig.json`：退出码 0。

Rust 覆盖每种读取结果，包括需要重新确认、读取失败，以及「一个未授权、一个无记录」的两个方向。前端对六种状态的全部组合检查副标题、文案和底色，并检查读取中与列表失败。

## 截图

本轮重拍的两张都不是黑图。没有点「稍后」或「继续」，没有勾授权，没有开 app-server。真机图没有重拍。

| 文件 | 修改时间 |
| --- | --- |
| `design/actual/handoff-mixed.png` | 2026-11-11 10:25:07 -0600 |
| `design/actual/handoff-read-failed.png` | 2026-11-11 10:25:08 -0600 |
| `design/native/m1b-setup.png` | 2026-11-11 09:54:18 -0600 |
| `design/native/m4-handoff-unauthorized.png` | 2026-11-11 10:01:16 -0600 |

mixed 的副标题是「0 条对话 · 0 条超过提醒阈值」。ChatGPT 是提示底色的「未授权，可在设置里开启」，DS Bot 是中性底色的「没有本机对话」。read_failed 两行都是提示底色的「读取失败，没有读到本机会话」。

`m1b-setup.png` 是这台电脑上工具已经显示为已授权的首次启动页，不是一台没有任何授权的新安装。图上的名称是 ChatGPT。真实对话页副标题仍是「未授权」。
