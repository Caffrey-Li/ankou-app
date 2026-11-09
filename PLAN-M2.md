# M2 计划：用量与额度

分支 `m2-usage`，从 `m1b-store` 的 `9b88423` 拉出。审核已批准，按下面三处修订实现。不提交、不推送，也不改 QuotaPot。

## 审核修订（2026-11-09）

这三处覆盖后文里冲突的句子。

1. Codex 额度的依据改为「官方协议源码（文档未列出），未验证」。现行 app-server 文档只证明握手和 stdio JSONL。请求不带参数，不传 `excludeResetCreditDetails`、`supportsLunaReserve`、`ordinaryUsageAllowed`。界面只显示已用百分比和重置时间。`usedPercent` 按整数解析，范围 0–100，超出则该窗口缺失。`windowDurationMins` 为空显示「周期未知」。`resetsAt` 为空不显示重置时间。`rateLimitResetCredits` 以及 `credits`、`planType`、`limitName`、`individualLimit`、`spendControlReached` 解析时丢掉。二进制只在 `/Applications/Codex.app` 的固定相对路径和 `PATH` 上的 `codex` 里按这个顺序找。
2. 不建 `account-links/deepseek`。新增 `account_links` 表，字段是 `tool`、`capability`（只允许 `account_link`）、`status`（`active` / `revoked`），不存路径。守卫提供 `account_link_active(tool)`。`open_read` 对 `AccountLink` 永远拒绝。界面文案是「DeepSeek 账户 · 余额」，Harness 和 DS Bot 都引用这一条。
3. `usage_daily.day` 和保留截止都用本机日历日。月份计算与 M1b 相同。不保留「时区交界可能多留一天」的说明。

产品选择按计划：15 分钟；`agent:` / `other` 不计入并提示无法归属的条数；费用列「不提供」；余额挂在这一条 DeepSeek 账户连接上。

任务书 §3 的验收是：旧 fixtures 迁成黄金样本并且全部通过；缺失不显示成 0；合计不重复。§6 要求先列接口和失败测试，再实现。界面在实现时按 §5 重读 Figma，这一步只把节点和状态定下来。

## 范围

做：DS Bot 本地调用的解析和去重、Codex rollouts 的解析、Codex app-server 的额度客户端（默认关闭）、DeepSeek 余额、过时和部分覆盖、概览页的额度卡片、用量页。

不做：对话上下文、磁盘占用、交接、清理、Cursor 用量、费用估算、`account/read`、登录、消耗重置次数、给账户发邮件、`account/usage/read`。概览里「对话」「空间」两张卡是别的里程碑，M2 不填那两个示例数字。

设计稿上的 3.49M、¥128.40、62%、17:05 都是示例。界面不抄这些数。对照截图时结构对齐，数字用空态或夹具结果，REVIEW 里说明差异来自示例数字。

实现界面前重新调用 Figma `get_design_context`。这一步读过的节点：

| 节点 | 内容 |
| --- | --- |
| 18:320 | 用量页 |
| 18:324 / 18:325 / 18:389 | 窗口、侧栏、工具栏 |
| 18:397 | 部分覆盖提示 |
| 18:413、18:425、18:435、18:443 | 两张 Codex 窗口卡、余额卡、Cursor 卡 |
| 18:522 / 18:629 | 按日堆叠、明细 |
| 18:195 | 概览里的额度卡；工具栏刷新在 18:69 |

## 1. 每个工具的用量从哪来

依据只用这四个词：官方文档、历史样本、已实测、未验证。万象自己还没有读过任何真实工具数据，所以下面没有「已实测」。

### DeepSeek 余额

| 项 | 内容 |
| --- | --- |
| 来源 | 用户在设置里保存的 API Key。只从本应用钥匙串读取，账户名沿用 M1b 的 `deepseek`。 |
| 请求 | `GET https://api.deepseek.com/user/balance`，头 `Authorization: Bearer <钥匙串里的 Key>`。 |
| 文档 | [查询余额](https://api-docs.deepseek.com/zh-cn/api/get-user-balance)。页面给出路径、`is_available`，以及 `balance_infos[]` 的 `currency`（`CNY` / `USD`）、`total_balance`、`granted_balance`、`topped_up_balance`（字符串）。 |
| 依据 | 官方文档。主机名是该文档对应的 API 主机。Bearer 是同一套 API 的密钥用法。万象未发过这个请求，标未验证。 |
| 守卫 | `deepseek_harness` 的 `account_link`。设置里只有这一条「余额 · 账户连接」。DS Bot 不另记一条。 |

这是万象进程在 M2 里唯一的 HTTP 请求。响应不进 token 合计。`is_available: false` 且余额字符串是 `0.00` 时，显示真实的 ¥0.00。字段缺失是缺失，不是 0。多种货币分别显示，不相加。

不读 `DEEPSEEK_API_KEY`，不读 `~/.dsh/.credentials.yaml`。旧实现把这两处当余额兜底；万象不沿用。

授权没有第三方目录。现有 `grants.root` 不能空。做法：在本应用数据目录下建 `account-links/deepseek`，授权根就是这个目录，模式用一个永远不会去读的名字。刷新只检查这条授权是否有效，然后读钥匙串。`open_read` 对这个目录下的任何文件仍然拒绝。根目录的设备号或 inode 对不上时，状态变成 `needs_reauth`，余额停止。撤销这条授权也立即停止余额。DS Bot 的 `usage` 撤销只停本地调用记录。

CSP 和 capability：

- `app/tauri.conf.json` 的 `connect-src` 维持 `ipc:` 和 `http://ipc.localhost`。不把 `api.deepseek.com` 加进 webview。密钥不进页面。
- 不启用 Tauri 的 http 插件。
- HTTP 放在 `wanxiang-adapter-dsh`，用 `ureq` 加 rustls。这是 M2 唯一的新依赖。生产地址写死，测试注入 base URL。超时 10 秒。关闭重定向，避免 `Authorization` 被带到别的主机。
- `app/capabilities/default.json` 增加 `allow-get-usage` 和 `allow-refresh-usage`。说明改成：窗口仍然不能发网络请求；这两条命令在 Rust 里执行，返回值没有密钥。`build.rs` 的命令清单同步，否则权限生成会漏。
- 没有有效授权时，刷新命令不发请求。

### Codex 额度窗口

| 项 | 内容 |
| --- | --- |
| 来源 | `codex app-server` 的 `account/rateLimits/read`。默认 stdio，一行一个 JSON。先 `initialize`，再发 `initialized`，然后才发这个方法。 |
| 参数 | `excludeResetCreditDetails: true`。`supportsLunaReserve` 不传。后者会让服务端记录实验曝光。 |
| 使用的字段 | `ordinaryUsageAllowed`；`rateLimits.primary` / `secondary` 的 `usedPercent`、`windowDurationMins`、`resetsAt`。`rateLimitsByLimitId` 只在单桶视图缺窗口时补同一套字段。 |
| 丢掉的字段 | `accountId`、邮箱、计划、upsell、积分明细。解析完成前就丢掉，快照结构里不留这些字段，日志和界面也不留。 |
| 文档 | [Codex app-server](https://developers.openai.com/codex/app-server)；协议结构见 [account.rs](https://github.com/openai/codex/blob/main/codex-rs/app-server-protocol/src/protocol/v2/account.rs)。 |
| 依据 | 官方文档。万象未跑过真实 `codex`，标未验证。 |

`ordinaryUsageAllowed` 为 `null` 时，不从百分比反推「还能不能用」。百分比仍可显示，旁边加「是否可用未确认」。窗口按 `windowDurationMins` 标注：300 是「5 小时窗口」，10080 是「7 天窗口」，其他值显示分钟数。不假设 primary 永远是 5 小时，不用 reserve 池充当通用额度。`usedPercent` 缺失、小于 0 或大于 100 时，这个窗口缺失，另一个窗口不受牵连。

功能开关是设置项 `codex_app_server`，默认 `false`。关闭时不启动子进程，窗口卡显示「应用未运行」，没有百分比。开启且授权有效时，子进程由现有 `Supervisor::spawn_child` 跟踪。只使用 stdio。不传 WebSocket，不把令牌放进参数。子进程会按 Codex 自己的方式使用本机登录；万象的代码不打开 `auth.json`，不把该文件读进内存或日志。

拿不到时：

- 进程不存在、握手失败、方法报错、进程退出：窗口显示「额度窗口不可用」。
- 以前有过成功样本：保留那个百分比和重置时间，标「过时 · HH:MM」。
- 从未成功，但已授权的 rollouts 里有 `rate_limits`：只用时间最新的一条，来源标「本机记录」。超过 15 分钟，或窗口重置时间已过，再标过时。15 分钟是旧实现的过时阈值，不是服务端刷新周期。依据是历史样本。
- 两边都没有：不显示 0%。

不调用 `account/read`、`account/login/*`、`account/usage/read`、`account/rateLimitResetCredit/consume`、`account/sendAddCreditsNudgeEmail`。

### Codex 用量

| 项 | 内容 |
| --- | --- |
| 来源 | 用户授权根下的 `sessions/**/*.jsonl`，只读。模式写到这一层，不使用 `**/*`。 |
| 守卫 | `codex` 的 `usage`。`auth.json` 继续被现有黑名单拒绝，即使模式写宽了也读不到。 |
| 依据 | 历史样本：QuotaPot `ports/codex/README.md` 和六份已脱敏 jsonl。万象未实测。 |

口径沿用那份说明，不另发明：

- `token_count` 在 `event_msg.payload`。`total_token_usage` 是累计值。相邻不变的快照不重复计入。累计回退是新的一段。第一段或重置后的累计大于 `last_token_usage` 时，只计 `last_token_usage`，并标部分覆盖。
- 计入的是 `total_tokens` 的可核实增量。`input_tokens` 已含缓存，`output_tokens` 已含推理，缓存和推理不另加一次。
- 没有 `token_count` 的会话排除，不记 0。文件读不了或记录损坏，也标部分覆盖。范围内有记录但增量为 0 时，才是真实的 0。
- 压缩只认顶层 `type:compacted`。用量下降不当成压缩。
- 上下文窗口和最近上下文留给 M3，不出现在用量页。

明细里 Codex 的「调用」显示「—」。旧实现对「调用」的定义是正增量条数，不等于用户发言数，所以不放进这一列。费用显示「不提供」。

### DeepSeek Harness

本地能确定的只有发现：`~/.dsh` 是目录。官方桌面文档和交接记录都没有给出一份可归属到 Harness 的用量文件。`bot/usage.sqlite` 是 DS Bot 的调用记录。

M2 不把这些调用算进 Harness，也不猜 `agent:` 前缀就是 Harness。Harness 的 token、调用、费用、上下文都显示「暂不提供」。设置里的 Harness「用量」保持「未验证」，并且不能勾选。余额卡上的「Harness 与 DS Bot 共用」只描述这一份账户余额，不表示两边各有一份 token。

没有可归属样本之前，不新增 Harness 的读取模式。

### DS Bot

| 项 | 内容 |
| --- | --- |
| 来源 | 授权根下的 `bot/usage.sqlite`，外加同目录的 `-wal` 和 `-shm`（存在才复制）。 |
| 表 | `calls`：`key, session, time, provider, model, kind, live, input, cache_read, cache_write, output`。`owners`：`session, owner, name`。 |
| 守卫 | `ds_bot` 的 `usage`。模式只包含这三个文件名。`.credentials.yaml` 继续被黑名单拒绝。 |
| 依据 | 历史样本：QuotaPot `ports/dsh/README.md` 写明表结构曾用本机库核对过；`usage.json` 是脱敏夹具。万象未实测。 |

本地能拿到的是 token 和调用次数。token = `input + cache_read + cache_write + output`，一行只加一次。调用次数是行数。`owner` 前缀 `room:` 为群聊，`agent:` 为 agent，`other` 为其他，其余为 bot。按工具、Bot、群聊、模型、会话分组时，分组是同一批行的不同看法，各组相加等于 DS Bot 合计，不把同一行算进两个工具。

读法：守卫放行后复制到临时目录，只读打开副本，读完即删，失败也删。`-wal` 存在但复制失败时，不解析半份数据库，这次刷新失败，保留上次结果。

本地拿不到、界面写「暂不提供」或「—」的：账单费用、上下文比例、耗时、模型状态。没有时长列。余额不从这张表来。

`agent:` 和 `other` 的行无法归属到设计稿里的 Harness 或某个 Bot。它们不进 Harness，也不进 Bot 合计。提示里写「有 N 条调用无法归属，未计入」。这是待确认项；在有归属样本之前不改这个口径。

### Cursor

首批没有用量来源。个人账户的团队管理接口不能证明本机用量。调度器不注册 Cursor 读取，不授予 `usage`，不访问本机 Cursor 目录，也不读 `cursorAuth/*`。

用量页和概览额度卡都显示「暂不提供」。设计稿示例里的「未授权」和「去授权 Cursor」不采用，因为这项不是待授权，是不做。工具行维持现有文案「用量暂不提供」。

## 2. 适配器、写入和调度

四个来源各做一个采集器，由 `wanxiang-core` 定义接口，适配器实现。采集器只读，不写数据库。调度器拿到快照后再写。

| 方法 | 行为 |
| --- | --- |
| `tool` | `ToolId` |
| `capability` | 这一项需要的 `Capability` |
| `collect` | 读一次。成功返回快照；失败返回固定错误码。取消标记已置位时直接返回，不读、不请求。 |

快照里的数量都是 `Option`。`None` 是缺失。`Some(0)` 才是核实过的 0。字段：按日本地 token、按日调用数、余额列表（货币和最小单位）、窗口列表（分钟、基点、重置时间、`ordinaryUsageAllowed`）、明细分组、排除条数和原因码、观察时间。没有账户标识字段。

`usage_daily.amount` 是整数，沿用现表。单位写进 `metric`：

| metric | amount |
| --- | --- |
| `tokens` | token 条数 |
| `calls` | 调用行数，只用于 DS Bot |
| `balance_cny_fen` / `balance_usd_cent` | 余额最小单位 |
| `window_<分钟>_bps` | 已用百分比的基点，62% 存 6200 |

现表没有观察时间，也没有失败状态。空表可以加列：`updated_at TEXT`。另加 `usage_report(tool, metric, state, reported_at)`，`state` 只允许 `ok`、`no_key`、`invalid_key`、`network`、`app_server`、`needs_reauth`、`unreadable`、`unavailable`。不存服务端原文，避免把路径或密钥写进去。打开旧库时看 `table_info`，缺列再 `ALTER`。

写入时机：

- 只有 `collect` 成功才写数量。按日 token 用本次重算结果替换该日该工具的行，重复刷新不累加。
- 余额和窗口是点样本，覆盖「观察日 + 工具 + metric」那一行，多次轮询不求和。
- 源里没有的日期不插入。缺失的柱是空的，文案是「无本机记录」。
- 失败只改 `usage_report`，不把数量写成 0，也不删除上次的行。
- 早于保留截止日的日期不写回。截止日当天保留，这是 M1b 已有的 `day < cutoff`。
- 清除历史仍然只删 `usage_daily` 和 `cleanup_history`，并一起清掉 `usage_report`。

日期用本机日历日，因为界面写的是「11 月 1 日」。保留截止仍是 M1b 的 UTC。这个组合在时区交界可能多留或少留一天。实现时加一行 `ponytail:` 说明这个上限；M2 不改保留算法。

调度替换 `app` 里的 `Nop`：

| 时机 | 行为 |
| --- | --- |
| 启动 | `refresh_on_launch` 为真时，在 `Supervisor::spawn_task` 里对每个仍有效的授权采集一次。 |
| 定期 | `periodic_refresh` 为真时，同一任务循环。间隔 15 分钟，待确认。睡眠切成 1 秒一段并查看 `TaskStop`，退出时才能在 2 秒内 join。 |
| 手动 | 工具栏刷新按钮始终可用，不受两个开关影响。进行中的同一次刷新不叠加。 |
| 退出 | 现有 `Supervisor::shutdown` 置位并终止全部子进程。 |
| 撤销 | `ToolScheduler::on_revoked` 只记下「这个工具的这项能力已停」，并终止该工具登记过的子进程。进行中的余额请求返回后先再看一次授权，已经撤销就把结果丢掉。下一次循环不再碰它。`needs_reauth` 的行不重试。 |

15 分钟没有官方轮询周期可引。它和旧实现的过时阈值相同，只作为本机节奏。DeepSeek 和 Codex 用同一个间隔。手动刷新不额外循环打接口。

app-server 子进程需要单独登记，因为现在的 `Supervisor` 只能在总退出时杀子进程。加一个按 pid 终止的方法，撤销 Codex `usage` 或关掉开关时只杀这一个。

两个开关都为假时，后台不跑。没有任何有效授权时，循环醒来也不读文件、不发请求。

## 3. 失败时界面显示什么

错误的 `Display` 和 `Debug` 只用下面这些固定中文。不拼接路径、密钥、响应体、账户标识、邮箱。

| 状态 | 触发 | 余额或窗口 | 已有数字 | 趋势 |
| --- | --- | --- | --- | --- |
| 密钥未填 | 钥匙串没有 Key | 余额卡「未保存密钥」，没有金额 | 不制造 ¥0 | 不变 |
| 密钥无效 | HTTP 401 或 403 | 「密钥无效」 | 保留上次金额，标「过时 · HH:MM」 | 不改已写入的日 |
| 网络失败 | 连接失败、超时、429、5xx、其他 HTTP | 「网络失败」或无法归类时的「查询失败」 | 同样保留并标过时 | 不改 |
| app-server 不可用 | 找不到二进制、握手失败、方法错误、进程退出 | 「额度窗口不可用」 | 有上次成功样本就保留并标过时；只有 rollout 记录时标「本机记录」 | token 仍来自已授权的本地文件 |
| 开关关闭 | `codex_app_server` 为假 | 「应用未运行」，不显示百分比 | rollout 的 token 仍可显示 | 不把旧百分比当成当前窗口 |
| 需要重新授权 | 根目录标识对不上 | 「需要重新授权」 | 停止读取。历史保留，不显示新的 0 | 空日期保持空，不补算 |
| 本地读不了 | 复制失败、数据库或 jsonl 损坏 | 该项「本地记录读不了」 | 保留上次 | 标部分覆盖 |
| 未提供 | Cursor、Harness，以及 DS Bot 的费用和上下文 | 「暂不提供」 | 无 | 不占堆叠、不进合计 |

部分覆盖条按实际缺口拼，例如「Cursor 用量暂不提供，未计入」「Harness 用量暂不提供，未计入」「Codex 显示 HH:MM 的数据（应用未运行）」「有 N 个会话没有用量记录，未计入」。不出现设计稿那句固定示例，也不放「去授权 Cursor」。

额度窗口和余额不受上方「今天 / 7 天 / 30 天 / 本月 / 自定义」影响，卡片角注用设计稿的「按各工具自身周期，不受上方日期影响」。合计、堆叠和明细跟这个范围走。默认「本月」。

来源标签：余额成功是「实际 · HH:MM」；本地 token 是「本机记录」；过时是用量页的「过时 · HH:MM」，概览环上是「数据为 HH:MM」。概览工具行在 Codex 只有旧样本时用「数据过时 · 应用未运行」。费用列和顶部「估算费用」都是「不提供」，不显示 ¥0。

空日期的说明用「更早没有本机记录，不补算」。授权表没有创建时间，不写「自某日授权后开始积累」。

## 4. 测试清单

先写这些测试，确认失败，再实现。网络测试只用本机模拟服务器。密钥只用明显的假值 `test-key`。断言错误文本、日志参数和快照调试输出里都没有它。

先失败的测试：

1. `usage.json` 的黄金结果与旧自检一致：当日 300 token、2 次调用、名称「示例 Bot」、种类 bot；7 日和 30 日都是 315 token。3 次调用只出现在 7 日口径里，按旧 `check.mjs`。
2. `balance.json` 解析成人民币 1234 分，赠金和充值分开。这 1234 不加进 token。
3. 同一累计快照重复出现只计一次。累计回退另起一段。首段累计大于单次用量时只计单次，并标部分覆盖。
4. 没有 `token_count` 的会话不产生 0。它是唯一会话时，合计是缺失。混在有用量的会话里时，只加有用量的，并给出排除数。
5. 只有顶层 `type:compacted` 计入压缩次数。用量下降不计。
6. `used_percent` 缺失、负数、大于 100 时该窗口缺失。另一窗口保留。
7. 同一批 `calls` 行只进入 DS Bot。Harness 合计保持缺失。Bot、群聊、模型、会话四组是分区，相加等于 DS Bot，不等于两倍。
8. 刷新失败后，原有 `usage_daily` 行还在，没有新的 0 行。`usage_report.state` 变成对应错误码。
9. 撤销后下一次调度跳过该工具。注入时钟不必真等 15 分钟。已返回的结果在授权消失后不落库。
10. `shutdown` 让循环任务结束，并终止登记的假子进程。
11. 401 和连接失败走模拟服务器。请求路径是 `/user/balance`，`Authorization` 等于测试传入的假密钥。错误文本不含这个密钥，也不含临时目录。
12. 假 stdio 进程先收 `initialize`，再收 `account/rateLimits/read`。响应里的 `accountId` 不出现在快照里。`ordinaryUsageAllowed: null` 不把百分比改成「可用」。
13. 调度器的来源列表没有 Cursor、没有 Harness 文件读取。临时目录里放一个名为 `auth.json` 的文件、一个 `.credentials.yaml`、一个 `state.vscdb`，采集过程的打开记录里没有它们。
14. 夹具目录本身：有 `usage.json`、`balance.json` 和六份 jsonl；没有 `credentials.yaml`。扫描不到邮箱，也没有密钥前缀。

proptest，放在现有 `proptest` 依赖上，不新增框架：

- 金额字符串：0 到 2 位小数接受，并换成最小单位。空串、正负号、空格、三位小数、负数、超出 `i64` 的值拒绝。拒绝的结果是缺失，不是 0。
- 百分比：0 和 100 含端点接受。小于 0、大于 100、NaN、无穷拒绝，该窗口缺失。
- 保留边界：等于截止日的行还在；更早的行删除后，再刷新也不会写回去。

只用夹具、不起服务器的：DS Bot 分组、Codex 六份 jsonl、余额 JSON 解析、窗口分钟映射、部分覆盖文案、Cursor 和 Harness 的「暂不提供」。

模拟服务器覆盖：200、401、连接被拒绝。不访问 `api.deepseek.com`。app-server 用测试自己的假可执行文件，不调用本机 `codex`。

黄金样本从 QuotaPot 复制 `ports/dsh/fixtures/usage.json`、`balance.json` 和 `ports/codex/fixtures/` 的六份 jsonl。不复制 `credentials.yaml`。复制后先跑上面的夹具扫描测试。

## 5. 第一次真实数据：待确认

夹具测试通过、界面用空态核对之后，才做一次只读验证。在那之前，设置里的用量和账户连接保持「未验证」并且不能勾选，发布构建里的 app-server 开关保持关闭。验证结果写进 `REVIEW-M2.md`。通过之前不把徽章改成可授权。

要用户点头的操作：

1. 在万象设置里自己填一次 DeepSeek API Key，再点一次用量页的刷新。万象只发余额这一个请求。用户不需要把 Key 发给开发会话。
2. 同意对 `~/.dsh/bot/usage.sqlite` 做一次只读复制。副本放在临时目录，读完删除。不打开 `.credentials.yaml`。
3. 同意二选一：开一次 `codex app-server` 并调用 `account/rateLimits/read`，或只扫描已授权的 `sessions/**/*.jsonl`。默认先做只读扫描。app-server 那一次要单独同意，因为它会让 Codex 按自己的登录去联网。
4. 不访问 Cursor 的数据目录，也不读 `cursorAuth/*`。
5. 验证输出进入 REVIEW 之前，去掉 Key、邮箱、账户标识和带用户名的路径。截图只拍空态或已经脱敏的数字。

同时要定的产品选择：

1. 定期刷新用 15 分钟，是否接受。
2. `agent:` / `other` 的调用继续排除，是否接受。
3. 费用列维持「不提供」，是否接受。设计稿的估算金额没有可引用的价目表。
4. 余额授权只挂在 Harness 的「余额 · 账户连接」上。只使用 DS Bot 的机器也会靠这条授权查询余额，是否接受。

## 落地顺序

审核通过之后按这个顺序做，仍然是先看到测试失败再写实现：

1. 核心里的快照、错误码、`usage_report` 和写入规则。
2. DS Bot 与 Codex rollout 的解析测试和实现。
3. 余额客户端和本机模拟服务器。
4. app-server 假进程、功能开关、调度、撤销和退出。
5. 命令、capability、CSP 核对。webview 的 connect-src 保持不变。
6. 用量页和概览额度卡。实现前重读 Figma。对照走 §5：参考图、实际图、并排图、pixelmatch、`?compare=1`、macOS 原生窗口。示例数字不写入界面。
7. `REVIEW-M2.md`，然后停下。真实数据的四项同意不放进这一轮实现。
