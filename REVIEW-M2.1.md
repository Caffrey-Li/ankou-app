# REVIEW-M2.1

实现停在审核前。没有提交，没有推送。这一轮没有启动真实的 Codex app-server，也没有再扫本机会话内容。截图用 `WANXIANG_SKIP_REFRESH=1` 打开新构建，没有点刷新。

上一轮验证留下的窗口如果还开着，跑的仍是旧程序，定期刷新仍会去连 app-server。请先退出那个窗口。

## 1. app-server 只在启动刷新和手动刷新时启动

`RefreshKind` 把启动、手动、定期分开。定期刷新只跑本地读取和余额查询，不调用 `ServerProbe`。同一轮里只要会问 app-server，文件扫描就不写额度窗口，避免并行时把窗口覆盖掉；设置关着时，文件扫描仍可写下 rollout 窗口。手动刷新在后台线程里跑，`refresh_usage` 立刻返回是否开始了这一轮。关窗口时这个线程跟着监督器停。

测试 `periodic_rounds_do_not_start_app_server`：注入时钟连走 3 个周期，假 app-server 启动次数是 0；再手动刷新一次，启动次数是 1。

## 2. 刷新速度和反馈

四个采集器在 `thread::scope` 里并行，各自完成后单独落库，并各发一次 `usage-item`。界面收到 `started` 后按钮进入转动且不能再点，还在等的合计、明细和额度卡显示「刷新中」；收到某一项后再拉当前日期范围，只替换那一项。已有的单轮限制还在：拿不到刷新锁就返回 false，按钮恢复可点。

Codex 按文件路径把 size、mtime 和解析出的数字摘要放进 SQLite 新表 `codex_file`、`codex_file_day`。没变化的文件复用上次的数字，不重新读内容。表里没有消息文本。

测试 `second_codex_scan_is_at_least_five_times_faster`：2000 个合成 jsonl。第一次 5783 ms，第二次 99 ms，第二次比第一次快约 58 倍。

界面文案由 `ui/usage.test.mjs` 锁住「刷新中」。这一轮没有在原生窗口里点刷新，所以截图里看不到转动和「刷新中」；点下去会按设置去连真实 app-server。

## 3. 额度卡来源和排序

来源记在 `window_source`：1 是 app-server，2 是本机 rollout。卡片来源只写一行：app-server 为「实际 · HH:MM」，rollout 为「本机记录」。窗口按 `windowDurationMins` 从小到大排，未知时长放后面，最多两张。

测试 `app_server_windows_say_actual_and_shorter_period_comes_first`。

已有的窗口样本没有这条来源标记，这一轮又没有再问 app-server，所以原生截图上两张卡都写「本机记录」。较短的 5 小时窗口已经排在 7 天窗口前面。

## 4. 只计入部分用量的会话

首段只计单次用量时，解析结果带 `partial`。采集把这类会话数写成 `partial_sessions`，已授权的 Codex 横幅加一句「有 N 个会话只计入部分用量」。读坏的行、负增量不计入这个 N。

测试 `partial_sessions_join_the_banner`，以及 `duplicate_rollout_counts_only_the_last_sample`（该样本 `partial` 为真）。库里还没有这项计数，这一轮没有重扫本机 jsonl，所以截图横幅里还没有这一句。

## 放开授权

DS Bot 用量、Codex 用量与额度窗口、DeepSeek 账户 · 余额、Codex app-server 改为普通勾选，依据是「已实测 2026-11-09」。Harness 用量、Cursor、对话、放入等仍是未验证或暂不提供。余额两条勾选共用同一条账户连接。根目录仍在 Rust 里解析，不回到界面。

删掉了临时的 `WANXIANG_VERIFY`、`verify_enabled`、`verify_set`。`ui/release-strip.test.mjs` 仍禁止这三处出现在前端包和 release 二进制里；「Codex app-server」已是正式文案，不再列入禁止串。

测试 `measured_grants_use_the_two_roots_and_can_turn_off`。

## 概览副标题

大标题下的副标题：至少一个工具已授权且没有待办时写「其余工具运行正常」；已授权但还有待办时写「已授权 N 个工具」；一个都没授权则留空。事项卡里不再放这句。

## 截图

原生窗口，敏感的金额、token、调用次数和横幅里的计数已打码。百分比、重置时刻和日期仍在。

- `design/native/m2.1-overview.png`
- `design/native/m2.1-usage.png`
- `design/native/m2.1-settings.png`

`VERIFY-M2.md` 和 `design/native/verify-m2-usage.png`、`design/native/verify-m2-overview.png` 仍留着，等这一轮一起提交。

## 测试

`cargo test --offline --workspace` 通过。`npx tsc --noEmit -p ui/tsconfig.json` 通过。`npm test` 9 项通过，含 `ui/release-strip.test.mjs` 和 `ui/usage.test.mjs`。
