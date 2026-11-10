# M3 计划：对话与交接

分支 `m3-handoff`，从 `m2-verify` 的 `a1695c6199aebaa8bf3874ead5a2761deb1a6b73` 拉出。这一步只定计划，不写实现代码。不提交、不推送，也不改 QuotaPot。

任务书 §3 的验收是：原文与摘要不落盘；取消后内存清除；深链超过 8,000 字符退回复制。§6 要求先列接口和失败测试，再实现。界面在实现时按 §5 重读 Figma，这一步只把节点和状态定下来。

## 审核修订（2026-11-09）

这七处覆盖后文里冲突的句子。

1. `thread/list` 每次请求都带 `useStateDbOnly: true`。依据是官方文档：省略或为 false 时会扫描 JSONL 日志来修复元数据，那是对第三方数据的写入。假 app-server 要断言每一次 `thread/list` 都带了这个参数。
2. 不使用 `thread/read` 的 `includeTurns: true`（官方文档已标为废弃）。正文用 `thread/turns/list` 加 `thread/items/list` 分页读。`thread/read` 不带 `includeTurns`，只取摘要信息。`thread/list` 的返回按官方示例核对 `id`、`preview`、`createdAt`、`updatedAt`、`name`、`status`，其中没有 `cwd`。app-server 路径上的工作目录一律缺失、不显示。只有 jsonl 回退、且 rollout 里有 `cwd` 时才显示。
3. 分页在最近 40 个回合或累计 80,000 字处停止，先到为准。每个回合的条目最多再翻 4 页。停在上限上时，预览标「长对话只取了最近部分」。
4. 读消息正文和列出 DS Bot 会话使用 `Conversation` 授权，不复用 `Usage`。`measured.rs` 记下 `codex_conversation`（根 `~/.codex`，模式 `sessions/**/*.jsonl`）和 `ds_bot_conversation`（根 `~/.dsh`，模式 `bot/usage.sqlite*`）。这两项保持未验证，`set_capability` 拒绝打开，界面复选框不可勾选。开发和测试用夹具，授权在内存里发。
5. 发送 AI 摘要的前提是 DeepSeek 账户连接有效，并且钥匙串里有 Key。点「发送并生成」时签发一枚和请求体哈希绑定的一次性令牌。不挂 `InsertContent`，也不另加工具授权。打开 Cursor 深链需要 Cursor 的 `CreateConversation` 授权（未验证、默认关）加上和原文哈希绑定的令牌。导出走系统保存对话框，用户选了位置即视为同意，不发令牌。
6. 摘要模型的依据是：现行文档列出的可选值是 `deepseek-flash` 和 `deepseek-v4-pro`。请求使用 `deepseek-flash`。
7. M2 的四项真实验证已在 2026-11-09 完成，见 `VERIFY-M2.md`。

硬边界照旧：不读 `auth.json`、`.credentials.yaml`、WorkBuddy `models.json`、`cursorAuth/*`；不读真实 Cursor 目录，也不碰 `state.vscdb`。模型仍用 Grok 4.7 high。

## 范围

做：四个工具的对话列表和上下文占用、交接的选择到预览、本地整理、DeepSeek 摘要、Cursor 深链、复制、概览里超过阈值的提醒。

不做：空间占用、清理、改 QuotaPot、把对话正文或摘要写入数据库和日志、在验证时对真实对话执行整理或放入。设计稿上的 84%、约 16.8 万 token、19:01、1,240 字、约 1.9k token、`~/Projects/site` 都是示例，界面不抄这些数。

实现界面前重新调用 Figma `get_design_context`。这一步读过的节点：

| 节点 | 内容 |
| --- | --- |
| 18:642 | 对话与交接页 |
| 18:646 / 18:647 / 18:711 | 窗口、侧栏、工具栏 |
| 20:69 / 20:73 | 交接内容预览页和窗口 |
| 20:216 | 预览面板：选择、生成、预览、放入 |
| 21:198 | 发送给 AI、已复制、放入结果的轻提示 |

预览按七段排：目标、进展、未完成、关键决策和约束、下一步、不确定事项、资料位置。窗口 1280×900。不装 Tailwind，沿用 `ui/src/tokens.css`。

## 1. 每个工具的对话列表和上下文

依据只用这四个词：官方文档、历史样本、已实测、未验证。万象还没有对真实对话发过 `thread/list` 或 `thread/read`，所以下面没有「已实测」。比例只在两个字段都在、且窗口大于 0 时计算：`last.totalTokens / modelContextWindow`，取整数百分比。缺任一字段就写缺失，界面显示「未知」，不用别的数字估算，也不把缺失显示成 0。

### Codex

列表走 app-server 的 `thread/list`，参数固定带 `useStateDbOnly: true`。依据：官方文档。响应是 `data` 和 `nextCursor`。按官方示例使用的字段是 `id`、`preview`、`createdAt`、`updatedAt`、`name`、`status`。返回里没有 `cwd`，这条路径不显示工作目录。`preview` 不当标题。`name` 为空时标题用「Codex 」加 `id` 前 8 位，依据是历史样本里旧端口的降级规则，不用工作目录，也不用消息正文。

Thread 上没有上下文字段。`thread/tokenUsage/updated` 通知里有 `tokenUsage.last.totalTokens` 和 `tokenUsage.modelContextWindow`，依据是官方文档，但那是已加载线程的推送。M3 不为此留一个常驻进程。列表上的比例因此在 app-server 路径上是缺失。

不可用时回退只读 `sessions/**/*.jsonl`，沿用 M2 的用量授权（根 `~/.codex`，模式 `sessions/**/*.jsonl`）。历史样本里的 rollout 有 `last_token_usage.total_tokens` 和 `model_context_window`（样本窗口 258400）。比例只用这一对，不用累计的 `total_token_usage`。两个字段缺一个，比例缺失。

M2 的 `codex_file` 缓存只服务用量，不给对话列表复用。对话读取走另一项授权，避免只有用量授权时从缓存里露出列表。不把正文、摘要或 `preview` 写入该缓存。

交接开始时才读正文，并且先要有 `Conversation` 授权。`thread/read` 不带 `includeTurns`，只取摘要信息。正文用 `thread/turns/list` 加 `thread/items/list` 分页，最近 40 个回合或 80,000 字，每个回合最多 4 页条目。回退路径只在同一次交接里读对应 jsonl 的消息行；有 `cwd` 才显示工作目录。读到的正文只留在这次交接的内存里。

开关沿用 `settings.codex_app_server`，默认关。关掉时直接走 jsonl 回退，不启动 app-server。

### DS Bot / Harness

DS Bot 能列出来的只有会话头，依据是历史样本：`~/.dsh/bot/usage.sqlite` 的 `calls.session`、`calls.time`，以及 `owners` 的 `owner`、`name`。M2 夹具和建表测试已经覆盖这几列。显示 Bot 或群的名称、会话短号、最近一条调用的时间。这张表没有对话标题、没有消息正文、没有上下文窗口。标题不拿会话编号冒充。上下文比例缺失，界面写「暂不提供」。正文也写「暂不提供」。

Harness 的官方文档描述过 JSONL 里的 `session/title` 事件，以及环境变量 `DSH_CONTEXT_WINDOW`。那个环境变量是容量配置，不是某次对话的已用 token。本仓库没有桌面会话 JSONL 的历史样本，桌面落盘路径未验证。M3 不扫未知目录。Harness 的列表、正文、上下文都写「暂不提供」。

两端都允许用户在交接时自己贴材料。贴进来的文字同样只留在内存里。

### Cursor

列表和正文只用夹具开发。夹具放在适配器的 fixtures 目录，字段按旧端口记录过的名称摆：标题、`contextTokensUsed`、`contextTokenLimit`、`contextUsagePercent`。这些字段标未验证。不读真实 Cursor 目录，不打开 `state.vscdb`，不碰 `cursorAuth/*`，也不跑 locate 探针。真实验证另行征得同意，不在本里程碑的默认验证里做。

比例只在夹具同时给出已用和上限、且上限大于 0 时计算。缺了就写缺失，显示「未知」。

## 2. 交接流程

页面是对话列表加右侧详情。筛选是「全部」「需要处理」「最近」，搜索只匹配已有标题。点一条对话只显示列表里已经有的标题、时间、比例、工作目录，不读正文。

「需要处理」是已知比例大于等于设置里的阈值。比例未知的行显示「未知 / 仍可交接」，不进这个筛选。

用户点「生成交接内容」才读取正文。读完放在界面进程的内存里。不写 sqlite，不写日志，不写 REVIEW。取消、完成、或离开预览，把原文和摘要从内存清掉。运行中的对话是读取那一刻的快照，界面标明快照时间。

整理二选一，默认跟着已有的 `summary_mode`，其默认值是「每次询问」。

本地整理不联网。能从正文里认出的七段标题就拆进去。认不出时整段放进「进展」，其余留空，并写「本地整理没有拆开段落」。不编造句子。来源里如果有角色或工作目录，预览上保留这两项标签。

AI 摘要是 M3 的第二个网络请求，第一个仍是 M2 的余额。发送前弹出确认，文案对齐节点 21:198：说明对话名、快照时间、将要发送的正文范围和字数，并写明发到 DeepSeek API、不含附件和其他对话、万象不保存。勾选「只对本次有效」不改设置。按钮是「发送并生成」「改用本地整理」「取消」。取消不发请求，内存按上面的规则清。设计稿里的「约 3.8 万 token」是示例。没有分词器的结果就不显示 token 数，只显示字数。

请求放在 Rust 里，复用已有的 ureq 和 rustls，不从 WebView 发。CSP 保持 `connect-src` 只有 `ipc:` 和 `http://ipc.localhost`。接口依据是官方文档，万象尚未发过，标未验证：

- `POST https://api.deepseek.com/chat/completions`
- `Authorization: Bearer`，钥匙只来自钥匙串里用户保存的 DeepSeek 项。不读环境变量，不读 `.credentials.yaml`
- `model` 用 `deepseek-flash`。现行文档列出的可选值是 `deepseek-flash` 和 `deepseek-v4-pro`
- `stream` 为 false，`thinking` 为 `{ "type": "disabled" }`
- 响应取 `choices[0].message.content`，再拆成七段

预览里每一段可改。页脚显示字数，以及「关闭后不保留原文与摘要」。长对话由模型精简时，预览写「长对话已精简，可能遗漏细节」。这句只在确实做了精简的响应上出现。

放入：

| 工具 | 做法 | 依据 |
| --- | --- | --- |
| Cursor | 官方 prompt 深链 `cursor://anysphere.cursor-deeplink/prompt?text=` | 官方文档。任务书要求默认关闭 |
| Codex | 只提供复制 | 没有已核对的官方写入方式。历史样本里旧端口的打开和生成都返回失败。不执行 `codex resume` |
| DS Bot / Harness | 只提供复制 | 同上，没有已核对的官方深链或稳定的命令行写入 |

Cursor 深链加一个设置项，默认关，旁边标「未验证」。关掉时「放入」按钮改成复制，不构造 URL。打开后才允许构造并打开。

长度按 Unicode 标量值计，不按字节。8,000 个字符可以打开，8,001 个不行。8,000 个汉字即使 UTF-8 有 24,000 字节，仍然允许。超过 8,000 字符不截断、不打开深链，全文改为复制，并提示「超过 8000 字，不能打开深链。全文仍可复制。」官方文档另写编码后的 URL 上限是 10,000 字符。编码后超过这个上限时同样改为复制，不截断正文。

Codex、DS Bot、Harness 复制成功后用节点 21:198 的对话框：「已复制，请手动粘贴」。可以再打开对应应用，打开时不附带提示词，也不声称已经写入草稿。

导出是用户在预览里主动保存的一份副本，走系统保存对话框。万象不另留一份。

### 确认令牌

沿用 M1a 的 `issue_confirmation` / `redeem_confirmation`。令牌绑定 `plan_hash(tool, capability, content)`，十分钟过期。哈希不一致会作废令牌。用过一次不能再用。

需要令牌的动作：

- 打开 Cursor 深链：已有的 `issue_confirmation`，工具是 Cursor，能力是 `CreateConversation`，哈希是将要放进 `text` 的原文。这项授权未验证，默认不发，界面不能勾选。
- 发送摘要：`issue_summary_confirmation`，只绑定请求体哈希，不挂 `InsertContent`，不要求工具授权。签发前先确认账户连接和钥匙串里的 Key。点「发送并生成」时签发。

导出不发令牌。用户在系统保存对话框里选了位置，命令只按这个路径写入。复制到剪贴板不发令牌。只打开应用、且不附带正文，也不发令牌。

这些写入能力默认仍是未授权。测试在内存里发授权。界面上未授权时，对应按钮不可用，并说明要先在设置里授权。

## 3. 上下文提醒

阈值用已经存在的 `settings.context_threshold_percent`，默认 80，范围 1 到 100。

已知比例大于等于阈值时，概览「需要处理」加一条：「N 个对话超过提醒阈值」。按钮是「去交接」，打开对话页并落在「需要处理」。未知比例不计数。对话页副标题用真实条数，例如「N 条对话 · M 条超过提醒阈值」，不用设计稿上的 6 和 1。

概览大标题下的副标题改回日期，跟对话页同一套 `formatDate`。删掉「其余工具运行正常」。没有待办时，卡片仍是「一切正常」和「没有需要你处理的事」。

## 4. 失败状态和文案

| 情况 | 文案 |
| --- | --- |
| app-server 不可用 | 列表改走 jsonl。jsonl 也没有时：「Codex 应用未运行，没有读到本机会话」 |
| 对话已被删除 | 「这条对话已经不在，无法交接」。内存里的半成品清掉 |
| 内容超长 | 「超过 8000 字，不能打开深链。全文仍可复制。」 |
| 摘要失败 | 「摘要没有生成。编辑还在，可以改用本地整理或重试。」失败保留这一次的内存编辑，离开页面仍清除 |
| 深链打不开 | 轻提示「未能放入：深链没有打开 · 编辑已保留」，可重试。不猜测 Cursor 是否在运行 |

钥匙串里没有 DeepSeek 钥匙时，不发摘要，对话框改为说明要先在设置里保存钥匙。成功放入后的轻提示是「已放入 Cursor 新对话 · 请检查后发送」，约 4 秒淡出。失败提示留到用户处理。

## 5. 测试

先写失败测试，再写实现。新增测试默认红，现有测试保持绿。

- proptest：8,000 个字符允许构造深链，8,001 个拒绝并给出复制回退。覆盖多字节中文，断言按字符而不是按字节。编码后 URL 超过 10,000 字符也拒绝。
- proptest 或表驱动：令牌哈希不一致返回 `ContentMismatch` 且令牌作废；第二次兑换返回 `ConfirmationUsed`；时钟超过 `CONFIRM_TTL` 返回 `ConfirmationExpired`。
- 假 app-server 进程，stdio JSONL，回应 `initialize`、`thread/list`、`thread/read`。不启动本机的 `codex`。
- 深链只测 URL 构造。测试不调用打开函数，断言打开函数没有被接到构造函数上。
- 交接正文和摘要不出现在 sqlite、日志夹具或命令返回以外的落盘。取消之后内存结构为空。
- 比例缺窗口或缺 last usage 时是缺失，不是 0。累计 `total_token_usage` 不参与除法。
- DS Bot 夹具只有会话头，上下文和正文是「暂不提供」。Cursor 读取函数在测试里只接受夹具根，传真实目录的测试断言拒绝。
- 概览在比例达到阈值时出现「N 个对话超过提醒阈值」。界面源码不再包含「其余工具运行正常」。

`cargo test --offline --workspace`、`npm test`、`tsc --noEmit` 都要过。proptest 已在 `crates/core`。

## 6. 验证时不做的事

验证用夹具和假进程。绝不对真实数据触发「整理」「一键开新」「继承对话」「复制整理」「复制继承」。不改真实草稿，不实际发送摘要，不打开真实深链，不导出到用户目录。

需要另行同意的真实验证：

1. 打开一次 Codex app-server，对用户指定的一条线程做 `thread/list` 和一次 `thread/read`。正文不写入 REVIEW。
2. 或者改为只读扫描 `sessions/**/*.jsonl` 的标题和上下文字段，不把消息正文写入 REVIEW。默认选扫描，不默认开 app-server。
3. 用户看过发送预览和字数之后，同意发一次 DeepSeek 摘要。
4. 用户把 Cursor 深链开关打开，同意用一条短于 8,000 字的夹具文字打开一次深链。
5. 读取真实 Cursor 对话库。这一项即使同意了，也不在未单独点名时做，并且仍然不碰 `cursorAuth/*` 和 `state.vscdb` 以外的授权库。M3 默认不做。

M2 的四项真实验证已在 2026-11-09 完成，见 `VERIFY-M2.md`。

## 7. 顺带

`REVIEW-M0.md` 里带用户名的绝对路径改成 `~/Projects/Wanxiang`。只改这一处，不改其他句子。
