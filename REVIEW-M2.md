# REVIEW-M2

实现停在审核前。没有提交，没有推送。这一轮没有读真实密钥、没有复制真实 `usage.sqlite`、没有对本机 Codex 发 `account/rateLimits/read`，也没有打开 Cursor 数据目录。

## 改 1–3

### 改 1：Codex 额度

依据写成「官方协议源码（文档未列出），未验证」。`developers.openai.com/codex/app-server` 会跳到 `learn.chatgpt.com/docs/app-server`，文档只支持握手和 stdio JSONL，没有列出 `account/rateLimits/read`。

请求是一行 JSON，不带 `jsonrpc`，`params` 省略。不发送 `excludeResetCreditDetails`、`supportsLunaReserve`、`ordinaryUsageAllowed`。界面只显示已用百分比和重置时间，没有「是否可用未确认」。

`usedPercent` 按整数解析，不在 0–100 的窗口整段丢掉。`windowDurationMins` 为空时标题是「周期未知」，不猜 5 小时或 7 天。`resetsAt` 为空时不显示重置时间。`rateLimitResetCredits` 以及 `credits`、`planType`、`limitName`、`individualLimit`、`spendControlReached` 在解析时丢弃，快照结构里没有这些字段。

二进制只查 `PATH`，再查固定的 `/opt/homebrew/bin` 和 `/usr/local/bin`。不查 `Codex.app`，也不扫磁盘。文件名必须和目录项完全一致，不能只靠 `is_file`。见下面「审核修复」。

### 改 2：账户连接

没有 `account-links/deepseek` 目录。新增 `account_links` 表，字段是 `tool`、`capability`（只允许 `account_link`）、`status`（`active` / `revoked`），不存路径、dev、inode。Harness 和 DS Bot 的设置行都写「DeepSeek 账户 · 余额」，库里只有 `deepseek_harness` 这一条。

`Guard::account_link_active` 对这两个工具读同一条标志。撤销走 `revoke_account_link`，退出走现有 `Supervisor::shutdown`。`open_read` 遇到 `AccountLink` 直接拒绝。撤销之后余额请求不会发出；请求已经开始、返回前授权没了，结果不落库。

### 改 3：保留期

`usage_daily.day` 和保留截止都用本机日历日。月份往前推 N 个月，日号超出当月就取最后一天，等于截止日的行保留。已去掉时区那行 `ponytail` 说明。测试把时钟放在 UTC 已是 2026-11-10 05:00、偏移 −6 小时（本地仍是 2026-11-09），12 个月截止日是 2025-11-09：2025-11-08 删除，2025-11-09 和 2025-11-10 保留。

## 产品选择

定期刷新 15 分钟。`agent:` / `other` 不计入，横幅写「有 N 条调用无法归属，未计入」。费用列是「暂不提供」。余额挂在这一条 DeepSeek 账户连接上。

## 命令和 capability

新增命令：`get_usage`、`refresh_usage`。对应权限 `allow-get-usage`、`allow-refresh-usage`。两个命令只返回数字和固定中文，不返回密钥。

没有新增 capability 枚举。账户连接继续用已有的 `account_link`，只是改由 `account_links` 表保存，不再伪装成路径授权。`codex_app_server` 是设置项，默认关，这一轮界面不能把它打开。

webview 的 `connect-src` 仍是 `ipc: http://ipc.localhost`。没有 Tauri HTTP 插件。唯一的 HTTP 在 Rust 里，用 `ureq` 2.12.1 的 `tls`（rustls），超时 10 秒，不跟随重定向。离线前已能从 crates.io 取到这个库，没有换成别的 HTTP 库。

## 测试

`cargo test --offline --workspace`：退出码 0。工作区 103 个测试通过，doc-tests 为 0。

`npm test`：退出码 0。8 个文件通过（`commands`、`icons`、`ipc-names`、`offline`、`release-strip`、`shell`、`tokens`、`usage`）。

`npx tsc --noEmit -p ui/tsconfig.json`：退出码 0。

| 测试 | 结果 |
| --- | --- |
| `open_read_rejects_account_link` | 通过 |
| `revoked_account_link_does_not_send_the_balance_request` | 通过 |
| `revoked_during_balance_request_does_not_persist` | 通过 |
| `failed_refresh_keeps_the_previous_amount` | 通过 |
| `account_link_is_one_row_for_both_tools` | 通过 |
| `retention_cutoff_uses_the_local_day_when_utc_has_moved_ahead` | 通过 |
| `retention_keeps_the_cutoff_day` | 通过 |
| `money_strings_do_not_become_zero_when_invalid` | 通过 |
| `percent_outside_0_to_100_is_missing` | 通过 |
| `minor_units_keep_two_decimal_places` | 通过 |
| `dsh_fixture_matches_the_old_daily_totals` | 通过 |
| `balance_fixture_is_cny_minor_units_not_tokens` | 通过 |
| `calls_are_not_added_to_harness` | 通过 |
| `balance_request_reports_invalid_key_without_echoing_it` | 通过 |
| `balance_request_reports_network_failure` | 通过 |
| `copied_database_round_trip_and_candidates_skip_credentials` | 通过 |
| `cumulative_rollout_counts_each_increment_once` | 通过 |
| `duplicate_rollout_counts_only_the_last_sample` | 通过 |
| `reset_rollout_starts_a_new_segment` | 通过 |
| `unmetered_rollout_is_missing_not_zero` | 通过 |
| `mixed_rollouts_exclude_unmetered_sessions` | 通过 |
| `only_explicit_compacted_events_count` | 通过 |
| `illegal_used_percent_drops_only_that_window` | 通过 |
| `missing_window_duration_is_unknown_period` | 通过 |
| `app_server_snapshot_drops_account_fields` | 通过 |
| `app_server_reads_rate_limits_without_extra_params` | 通过 |
| `extra_dirs_are_only_the_two_fixed_bins` | 通过 |
| `codex_cli_skips_the_desktop_binary_and_finds_the_path_name` | 通过 |
| `four_usage_states_stay_distinct` | 通过 |
| `m2_fixture_page_is_the_parsed_golden_sample` | 通过 |
| `readers_do_not_open_credential_files`（Codex 适配器，以及 `app` 里的同名测试） | 通过 |
| `fixtures_contain_no_email_or_secret_prefix` | 通过 |
| `missing_program_is_app_server_unavailable` | 通过 |
| `refresh_interval_is_fifteen_minutes` | 通过 |
| `shutdown_stops_the_refresh_loop_and_its_child` | 通过 |
| `revoking_codex_usage_stops_its_child` | 通过 |
| `empty_usage_names_the_gaps_and_skips_example_figures` | 通过 |
| `excluded_calls_are_named_in_the_banner` | 通过 |

黄金样本只复制了 `usage.json`、`balance.json` 和六份 rollout jsonl。没有复制 `credentials.yaml`。界面源码不含设计稿示例数字。

## 审核修复

逻辑层维持原样。这一轮只改查找、文案、夹具对照和原生截图。没有提交，没有推送。

### 1. Codex 可执行文件

删掉了应用包里的全部候选，包括 `Contents/MacOS`。只查 `PATH`，再查固定的 `/opt/homebrew/bin` 和 `/usr/local/bin`，不查别的目录。找到文件后用 `read_dir` 列出父目录，条目名必须和 `codex` 逐字相同，不能只靠 `is_file`。

`extra_dirs_are_only_the_two_fixed_bins` 锁住这两个目录，并确认源码里没有应用包路径。`codex_cli_skips_the_desktop_binary_and_finds_the_path_name` 在临时目录放了 `Codex.app/Contents/MacOS/Codex`，确认它不会被选中；`PATH` 里的 `codex` 会被选中。

捆绑 CLI 的位置先不猜，列入待确认。真实验证之后再决定要不要加回来。

### 2. 「未授权」和「暂不提供」

「暂不提供」只用于 Cursor 用量、Harness 用量和费用。DS Bot、Codex 没有有效授权时，明细、合计和额度卡都显示「未授权」，并带「去设置授权」。授权失效显示「需要重新授权」。已经授权、还没有数据时显示「无本机记录」。合计没有任何可计数据时，数字是「—」，下面一行小字写原因，不写「暂不提供 token」。部分覆盖提示条按同一口径重拼：Cursor 和 Harness 仍是「用量暂不提供，未计入」。

`four_usage_states_stay_distinct` 覆盖这四种状态：未授权、需要重新授权、暂不提供、无本机记录。

### 3. 夹具对照

开发构建可以打开 `?compare=1&fixture=m2`。页面数据是已提交黄金样本解析后的结果，DeepSeek 余额用 `balance.json`。不调用真实命令。发布构建里这段被去掉：`ui/release-strip.test.mjs` 跑一次 `vite build`，确认产物里没有 `12.34`，也没有 `fixture-m2`。`m2_fixture_page_is_the_parsed_golden_sample` 用那六份 jsonl、`usage.json` 和 `balance.json` 生成页面，并核对 `ui/src/fixture-m2.json`。

夹具有两个时钟，都写在这里：按日用量锚在黄金样本的 2026-10-25 12:00 UTC；窗口显示用最后一条 modern 样本的 2026-10-24 11:45:18 UTC，否则主窗口的重置时间已经过去，展示层会把它丢掉。日期范围是 10 月 1 日 – 25 日。夹具事实里把 `codex_app_server` 设为开，只为把已解析的 6% / 5 小时和 1% / 7 天显示出来，没有启动进程。

版式按稿改过：额度卡左边 56px 圆环、右边文字，空态圆环中心是「—」，状态文字在右边；DeepSeek 卡下面没有多出来的「余额」标签，四张卡等高。日期分段控件有底板，标题下面是日期范围。图表保留坐标轴和日期刻度，没有数据的日期是虚线空柱。概览保持稿面版式：需要处理、额度、对话、空间，再加底部工具列表。对话和空间写「尚未接入」。工具时间列是「—」。

`node scripts/compare-m2.mjs`，pixelmatch 阈值 0.05。参考图是 Figma 18:320（1440×1080）和 18:70（1440×1000）。空态另存在 `design/actual/usage-empty.png`、`design/actual/overview-empty.png`，只作状态检查，不计入下面的比例。

| 区域 | 用量页 | 概览页 |
| --- | --- | --- |
| 整图 | 0.2536（394382 / 1440×1080） | 0.2355（339102 / 1440×1000） |
| 工具栏 | 0.3324（1152×97） | 0.3861（1152×80） |
| 提示条 | 0.2438（1152×76） | — |
| 额度卡 | 0.1446（1152×194） | 0.123（432×240） |
| 图表 | 0.1253（1152×281） | — |
| 明细 | 0.1118（1152×367） | — |
| 需要处理 | — | 0.0872（1152×140） |
| 对话与空间 | — | 0.1208（720×240） |
| 工具列表 | — | 0.082（1152×400） |

示例数字和夹具数字不同，单独列出，没有把稿面数字写进界面：

- 稿面 Codex 是 62% / 34%，夹具是解析出的 6% / 1%。
- 稿面余额是 ¥128.40，夹具 `balance.json` 是 ¥12.34。
- 稿面合计是 3.49M、11 月 1 日 – 8 日；夹具合计是 372,589，范围是 10 月 1 日 – 25 日。
- 稿面时间是 17:05 / 21:40；夹具是 11:45，重置时间是 16:29 和 11:29。
- 稿面提示条把 Cursor 写成未授权，并写「去授权 Cursor」。这里按审核口径写成「用量暂不提供」，链接是「去设置授权」。
- 夹具有 1 个会话没有用量记录，提示条多这一句。
- 概览稿面有待办、对话和空间的示例内容。夹具下「需要处理」是「暂时没有需要处理的事」，对话和空间是「尚未接入」。浏览器对照没有本机发现结果，工具行是「未发现」。

工具栏比例高，主要是日期、时间和「更新于」不同。

### 4. 原生窗口

按 M1b 的做法，只问钥匙串里是否已经存过密钥。`is_saved` 的结果把秘密丢掉。没有保存，没有删除，没有启动 app-server，也没有发余额请求或复制数据库。没有点「稍后」或「继续」，没有写 `setup_complete`。

截图是 `design/native/m2-usage.png` 和 `design/native/m2-overview.png`。第二轮按新版式重截了这两张，见下一节。

## 第二轮

修复 1、2 没有再改。这一轮只改对照方法和界面还原。没有提交，没有推送。

### 对照方法

参考图裁到窗口外边缘：用量窗口 1280×980，概览窗口 1280×900，桌面背景和外圈留白去掉。再缩放到和实际截图同一尺寸，按 M0.5 的做法铺平后做 pixelmatch，阈值 0.05。实际图是浏览器里的窗口内容，视口分别是 1280×980 和 1280×900。

夹具模式把显示时钟设到数据的最后一天，本地 2026-10-25。用量副标题是「本月 · 10 月 1 日 – 25 日」，概览日期是「星期日 · 10 月 25 日」。窗口百分比仍用 2026-10-24 11:45:18 那条样本，否则主窗口的重置时间已经过去，展示层会丢掉它。没有启动真实命令。

| 区域 | 用量页 | 概览页 |
| --- | --- | --- |
| 整图 | 0.0659（82655 / 1280×980） | 0.0550（63343 / 1280×900） |
| 工具栏 | 0.0436（990×52） | 0.0248（990×54） |
| 提示条 | 0.0942（970×50） | — |
| 额度卡 | 0.0753（970×136） | 0.1204（312×211，稿面位置） |
| 图表 | 0.0841（970×313） | — |
| 明细 | 0.0700（970×324） | — |
| 需要处理 | — | 0.0640（970×257） |
| 对话与空间 | — | 0.0913（641×211） |
| 工具列表 | — | 0.0446（970×273） |

用量页的额度卡、明细，以及概览的工具列表，都在 10% 以内。用量页工具栏剩下的差异是日期文字：稿面是 11 月 1 日 – 8 日，夹具是 10 月 1 日 – 25 日。

概览的「需要处理」在夹具里没有事项，收成一行，所以下面的额度卡、对话和空间整块上移。按稿面坐标量额度卡是 0.1204。把实际卡片移回稿面那张卡的位置再比，额度卡是 0.0971，工具列表是 0.0444。

示例数字和夹具数字仍然分开算，没有写进界面：62% / 34% 对 6% / 1%，¥128.40 对 ¥12.34，3.49M 和 11 月对 372,589 和 10 月 1 日 – 25 日，17:05 / 21:40 对 11:45、16:29、11:29。

### 组件

| 组件 | 稿面 | 实现 |
| --- | --- | --- |
| 用量工具栏 | 日期分段控件和刷新按钮 | 同样只留这两项。更新于和搜索只在概览 |
| Codex 额度卡 | 标题 12px，56px 圆环，重置 15px，次要行「本机记录」，来源标签 | 同样结构。百分比和重置时间用解析结果 |
| DeepSeek 卡 | 标题「DeepSeek 余额」，¥ 15px 加大金额 26px，「Harness 与 DS Bot 共用」，来源标签 | 同样结构。没有余额时大字是状态，仍保留共用说明 |
| Cursor 卡 | 灰色 20px 大字、一行说明、「了解原因」 | 大字是「暂不提供」，说明是「个人账户来源待验证」，链接是「了解原因」 |
| 图表 | 无 Y 轴刻度。图例 Harness `#4a6fa0`、DS Bot `rgba(74,111,160,0.45)`、Codex `#a1a1a6`。柱宽 22px，柱槽 46px，堆叠间距 2px，空日虚线柱 150×22、圆角 6 | 同样尺寸和颜色。图注仍是「更早没有本机记录，不补算」 |
| 合计 | 30px token 数，旁边 14px「token」。估算费用是一列金额 | token 数用解析合计。估算费用是 14px 常规字「不提供」 |
| 明细 | 名称 230、类型 70、token 120、调用 70、费用 130，后三列右对齐，间距 16。脚注是共用记录那句 | 同样列宽和对齐。费用列是「不提供」。没有来源时是「—」。脚注用稿面那句 |
| 工具列表 | 18:260 起的行：32px 图标、名称加副标题、7px 状态点加状态、90px 时间、箭头。右上角「管理授权」。18:123 是侧栏那一行，不是这张列表 | 同样的行，不再用表格。「管理授权」进入设置。夹具状态来自授权：Codex、DS Bot 是「已授权」，Harness、Cursor 是「暂不提供」。时间是「—」 |
| 需要处理 | 有事项：标签、28px「N 件事需要你处理」、事项行和按钮。稿面的两条示例事项不在 M2 | M2 只列「余额密钥未保存」（按钮「去设置」）和「需要重新授权」（按钮「重新授权」）。没有事项时收成一行：「一切正常」，次要说明「没有需要你处理的事」 |
| 对话、空间 | 大数字和进度条 | 保持「尚未接入」，高度和额度卡同一行拉齐 |

费用列和估算费用从上一轮的「暂不提供」改成稿面的「不提供」。Cursor 用量、Harness 用量和分组页仍是「暂不提供」。

原生窗口按上一轮的限制重截：只查询是否已保存密钥，没有保存、没有删除、没有开 app-server，没有点「稍后」或「继续」。本机没有已保存的密钥。用量页是 `design/native/m2-usage.png`，概览页是 `design/native/m2-overview.png`。概览列出一条「余额密钥未保存」。四个工具都是这次启动时发现到的「未授权」，时间是「—」。截图里没有密钥、邮箱或账户标识。

## 第三轮

用量页保持第二轮的结果。这一轮只改概览的 4 处。没有推送。三项真实数据操作没有做。

### 原生截图

上一张 `design/native/m2-overview.png` 的修改时间是 2026-11-09 14:49:49 UTC，内容还是表格。这一轮重截了这张。仍然只查询是否已保存密钥：没有保存，没有删除，没有启动 app-server，没有点「稍后」或「继续」。本机没有已保存的密钥。

| 文件 | 修改时间 |
| --- | --- |
| `design/native/m2-usage.png` | 2026-11-09 15:35:21 UTC |
| `design/native/m2-overview.png` | 2026-11-09 16:00:09 UTC |

用量页沿用第二轮那张。概览是新的工具行：一条「余额密钥未保存」，四个工具的状态都是「未授权」，点是灰色。Harness 和 Cursor 的副标题是「用量暂不提供」，DS Bot 和 Codex 的副标题是「尚未授权」。时间是「—」。截图里没有密钥、邮箱或账户标识。

### 状态

| 组件 | 稿面 | 实现 |
| --- | --- | --- |
| 状态点 | 7px。授权正常用蓝色，异常用赭色 | 直径 7px。已授权 `#4a6fa0`，需要重新授权 `#b9733f`，未发现和未授权 `#a1a1a6` |
| 状态文字 | 稿面行上写的是运行情况，例如「正常」 | M2 只写授权：未发现、未授权、已授权、需要重新授权 |
| 副标题 | Cursor 那行是「对话与空间已授权 · 用量暂不提供」 | 用量被标成暂不提供时，副标题只写「用量暂不提供」。已授权且有用量时写「已授权用量」 |

侧栏的点和概览工具行读同一次 `toolMarks`。夹具模式也用这一份，授权从明细里的「未授权」「需要重新授权」读取；两条都没有时视为已授权。用量文字「暂不提供」只进副标题。

夹具结果：Codex、DS Bot 是已授权，副标题「已授权用量」。Harness、Cursor 是已授权，副标题「用量暂不提供」。侧栏四个点都是蓝色，和工具行的点、文字一致。

`ui/tool-status.test.mjs` 用同一份状态数据渲染侧栏和工具行，断言两边的状态文字和点的类一致。样本里有已授权、需要重新授权、未授权，以及「用量暂不提供」留在副标题。另一组用本机快照、关掉夹具，两边仍然一致。

### 一切正常

事项行内边距是 12px 16px。收起后的卡用同样的 12px 16px，并去掉概览卡的 120px 最小高度。夹具里这张卡高 47px，一行是「一切正常」和「没有需要你处理的事」。

收起之后额度卡往上移是预期的。第二轮按稿面坐标量到的额度卡比例 0.1204，来自这张卡变矮后下面的内容上移。把额度卡移回稿面位置再比，仍是 0.0971。

## 扫描

工作区源码、夹具和文档（跳过 `.git`、`node_modules`、`target`、`dist`、`design/`）：

- 邮箱：0。仅有的 `@` 是图标文件名里的 `@2x`，不是邮箱。
- 独立的密钥前缀（前面不是字母）：0。
- 没有「打开登录文件」的成功记录。守卫在读字节之前按文件名拒绝；相关测试断言拒绝，不把内容读出来。

## 待确认

捆绑 CLI 的位置。现在只查 `PATH`，以及 `/opt/homebrew/bin`、`/usr/local/bin`。不查应用包。等下面的真实验证做完，再决定要不要加回固定位置。

## 待确认（真实数据，这一轮不做）

1. 在万象里亲手保存一次 DeepSeek 密钥，并点一次刷新。确认余额请求打到 `GET /user/balance`，界面只出现金额和时间，不出现密钥。
2. 同意对 `~/.dsh/bot/usage.sqlite` 做一次只读复制（`-wal` / `-shm` 若存在一并复制），读完删除副本。不打开凭据文件。
3. 二选一，并单独同意：开一次 `codex app-server` 做无参数的 `account/rateLimits/read`，或只扫描已授权的 `sessions/**/*.jsonl`。默认先做只读扫描。app-server 会让 Codex 用它自己的登录去联网。

Cursor 数据目录和 `cursorAuth/*` 不在这三项里，也不读。验证输出进 REVIEW 之前去掉密钥、邮箱、账户标识和带用户名的路径。
