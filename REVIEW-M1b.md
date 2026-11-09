# REVIEW-M1b

里程碑：存储、密钥与首次启动、设置界面。分支 `m1b-store`，从 `m1a-guard` 的 `c4f36dd` 拉出。没有提交，没有推送。没有改 QuotaPot。

## 数据表

数据库文件是 `wanxiang.sqlite`，放在 Tauri 应用数据目录（identifier `local.wanxiang.assistant`）。`crates/core/src/store.rs` 建四张表。没有对话正文、消息、提示词或转写列。

`settings`：`key TEXT PRIMARY KEY`，`value TEXT`。键是 `retention_months`、`launch_at_login`、`notifications`、`context_threshold_percent`、`space_threshold_gb`、`refresh_on_launch`、`periodic_refresh`、`summary_mode`、`menu_bar_icon`、`close_to_tray`、`setup_complete`。布尔存 `"0"` / `"1"`。摘要存 `ask` / `local` / `ai`。

默认：用量保留 12 个月，开机启动关，系统通知关，上下文阈值 80，空间阈值 5 GB，启动时刷新开，定期刷新开，摘要每次询问，菜单栏图标开，关闭窗口时留在菜单栏，首次启动未完成。保留期允许 1 到 120 个月，上下文 1 到 100，空间 1 到 10000。非法值拒绝写入。

`grants`：`id`、`tool`、`capability`、`root`、`pattern`、`dev`、`ino`、`status`。唯一索引是 `(tool, capability, root, pattern)`。`dev` / `ino` 用十进制文本保存。`status` 是 `active` 或 `needs_reauth`。工具和能力用 snake_case 名字。返回给界面的 `StoredGrant` 不带 `root`。

启动时只恢复 `active` 行。先对根目录做 canonicalize 并取当前 `(dev, inode)`，和库里的路径、设备号、inode 一致才交给守卫 `grant`。对不上、根不存在或根不是目录，把这一行改成 `needs_reauth`，不插入活授权。已经是 `needs_reauth` 的行下次启动不再尝试，目录回来也不会自动放行。

`usage_daily`：`day`、`tool`、`metric`、`amount`，主键 `(day, tool, metric)`。本步没有写入用量的生产接口。保留期按注入时钟的 UTC 日期计算，删掉早于「今天减去 N 个月」（日号超界则夹到该月最后一天）的行，等于截止日期的留下。`set_settings` 会再跑一次保留期。`cleanup_history` 不参与保留期。

`cleanup_history`：`id`、`created_at`、`summary`。`clear_history` 只删 `usage_daily` 和 `cleanup_history`。

## 命令

`app/build.rs` 把下面 11 个命令交给 Tauri 生成 `allow-*` 权限。`app/capabilities/default.json` 在 `core:default` 之外只放行这些。`core:default` 是窗口、托盘和路径解析要用的，里面没有这 11 个之外的自定义命令，也没有网络权限。CSP 的 `connect-src` 只有 `ipc:` 和 `http://ipc.localhost`。

| 命令 | 作用 |
| --- | --- |
| `list_grants` | 列出授权，不带根路径 |
| `grant` | 登记一条授权 |
| `revoke` | 按 id 撤销 |
| `get_settings` / `set_settings` | 读、写设置；写入时应用保留期 |
| `save_deepseek_key` | 写入钥匙串后丢掉明文 |
| `delete_deepseek_key` | 删除；没有条目也算成功 |
| `deepseek_key_saved` | 只返回是否已保存 |
| `clear_history` | 清两张历史表 |
| `discover_tools` | 只判断目录是否存在，并合成已发现状态 |
| `complete_setup` | 把 `setup_complete` 写成真 |

错误返回 `Display` 文本。密钥为空是「密钥为空」，钥匙串失败是「钥匙串不可用」，需要重新授权是「需要重新授权」。这些文本不含路径和密钥。

界面调用了列出、撤销、读写设置、保存、删除、是否已保存、清除历史、发现、完成首次启动。没有调用 `grant`：这一步每项能力都是未验证或暂不提供，勾选框是禁用的。

钥匙串服务名 `local.wanxiang.assistant`，账户 `deepseek`。正式路径用 `keyring` 4 的 v1 初始化系统钥匙串，macOS 目标打开 `apple-native-keyring-store`。测试先安装 `keyring-core` 的内存模拟后端，之后只使用 `keyring_core::Entry`。v1 的构造函数会把默认仓库换成系统钥匙串，所以模拟后端装上之后不再调用它。

发现路径由适配器声明，只调用 `is_dir`：

- DeepSeek Harness 和 DS Bot：`~/.dsh` 是目录。两个产品共用这一处，本步不猜应用包名。
- Cursor：漫游目录下的 `Cursor`，或 `/Applications/Cursor.app`。
- Codex：`~/.codex`，或 `/Applications/Codex.app`。同名文件不算已安装。

测试传入临时目录。生产路径在命令里现算：`HOME` 或 `USERPROFILE`，Windows 用 `APPDATA`，macOS 用资源库下的应用支持目录，Linux 用 `~/.local/share`。源码里不拼出「Application Support」这四个字。

开机启动、系统通知、菜单栏图标只写入设置，不改登录项、不申请通知权限、不拆掉托盘。关闭窗口时：留在菜单栏则隐藏，选退出则结束进程。

## 测试输出摘要

本机 macOS，aarch64。`cargo test -p wanxiang-core --offline`：

- `tests/guard.rs`：36 通过，0 失败，约 0.01s（含 M1a）
- `tests/path_props.rs`：5 通过，约 0.17s
- 库内测试：1 通过（`query_stays_on_the_mock_store`）
- `tests/secret.rs`：1 通过，约 0.00s
- `tests/shutdown.rs`：3 通过，约 0.12s
- `tests/store.rs`：8 通过，约 0.03s

`store.rs` 覆盖：授权写入后重新打开仍在，并能经守卫读出 8 字节夹具；根目录换掉后标为需要重新授权，守卫里没有活授权，再打开一次仍然是需要重新授权；清除历史后设置和授权还在；时钟停在 2026-11-09，12 个月删掉 2025-11-08，留下 2025-11-09 和当天，改成 1 个月后留下 2026-10-09 和当天；默认 12 个月且开机启动、通知为关；两张历史表没有正文类列；已安装加上授权状态得到未发现、未授权、已授权、需要重新授权；打开失败的错误文本不含路径。

三个适配器各 2 通过：仍是未验证，以及临时目录下的已安装和未安装。`wanxiang-app` 库测试 2 通过：capability 里每个命令恰好一条 `allow-*`；临时 HOME 下已安装和未安装，且不读文件内容。

`npm test`：5 通过（commands、icons、offline、shell、tokens）。`tsc --noEmit` 通过。界面测试核对命令名和 capability 一致，源码不含「已连接」「已选」，含「未发现」「未授权」「未验证」，两个常驻开关默认关，密钥输入不是明文。

## 对照图

阈值 0.05。比例是不匹配像素除以该区域像素。设计稿里的已连接、已选 N 项、蓝色可用勾选没有照搬，所以正文区域差得比较多。

设置，节点 20:459，缩放到 1280×900：

| 图 | 路径 |
| --- | --- |
| 参考 | `design/refs/20-459-export.png`，裁切后 `design/refs/settings.png` |
| 实现 | `design/actual/settings.png` |
| 差异 | `design/diff/settings.png` |
| 并排 | `design/diff/settings-side-by-side.png` |

整窗 12.21%。侧栏 13.92%，工具栏 3.33%，内容 12.21%，左侧表单 9.21%，右侧授权 13.59%。

首次启动，节点 21:57 的窗口卡片 21:61，按卡片对齐，参考尺寸 1100×1000：

| 图 | 路径 |
| --- | --- |
| 参考 | `design/refs/21-61-export.png`，裁切后 `design/refs/setup.png` |
| 实现 | `design/actual/setup.png`（元素原图 `design/actual/setup-raw.png`） |
| 差异 | `design/diff/setup.png` |
| 并排 | `design/diff/setup-side-by-side.png` |

整卡 10.73%。头部 9.20%，列表 11.34%，底部 9.59%。

原生窗口只看了首次启动：`design/native/m1b-setup.png`。当时这台电脑上的结果是找到 4 个工具，DeepSeek Harness 未发现，DS Bot、Cursor、Codex 未授权。没有在原生窗口里点「稍后」或「继续」，所以没有把首次启动标成完成，也没有往钥匙串写密钥。设置页的原生窗口没有截，因为从首次启动进去会写下 `setup_complete`。

## 组件清单

设计值来自节点 21:57 和 20:459。浏览器里用 `?page=setup` 和 `?page=settings` 点过开关、保留期、清除确认、密钥表单和禁用勾选。

| 组件 | 设计 | 实现 | 是否一致 |
| --- | --- | --- | --- |
| 首次启动卡片 | 宽 960，内边距 36 48 28，间距 18，圆角 24 | `setup.css` 同值，圆角用窗口 token | 一致 |
| 标题 | 26px「欢迎使用万象小助手」，副标题「看清用量 · 延续对话 · 管理空间」 | 同文案 | 一致 |
| 步骤 | 发现工具 / 选择授权 / 开始使用，当前是第 2 步 | 同结构 | 一致 |
| 找到几个工具 | 稿面是示例「4 个」且已连接 | 浏览器无 Tauri 时是 0 和未发现；原生窗口是真实的 4 个和未发现 / 未授权 | 按真实状态替换示例 |
| 能力勾选 | 稿面蓝色可勾，徽章是可用、每次确认、可选 | 灰色禁用，徽章是未验证；Cursor 用量是暂不提供 | 按本步要求替换 |
| 底部 | 锁形说明，稍后、继续 | 同文案。两个按钮都调用 `complete_setup` | 一致。浏览器里 `?page=setup` 会一直留在这一页，方便对照 |
| 设置标题 | 「设置」和「阈值、刷新、授权与常驻」 | 同文案，工具栏区域差 3.33% | 一致 |
| 卡片 | 圆角 14，内边距左右 18 | 同值 | 一致 |
| 右栏 | 宽 380，与左栏间距 20 | 同值 | 一致 |
| 开关 | 38×22 | 同尺寸。开机启动、系统通知默认关，菜单栏图标默认开 | 一致，浏览器里点过开机启动，状态从关到开 |
| 用量历史 | 12 个月，清除… | 选项 1、3、6、12、24。清除先确认 | 一致 |
| DeepSeek 密钥 | 「管理」，不回显 | 密码框，只显示已保存或未保存，保存后清空输入 | 一致。浏览器里输入的夹具没有出现在页面文字里 |
| 授权标签 | 稿面蓝色已授权 | 未验证用赭色，暂不提供用灰色，不写示例数字 | 按真实状态替换，右栏差 13.59% |
| 侧栏 | 沿用 M0.5 壳 | 工具点跟着发现状态。浏览器回退全是未发现 | 侧栏差 13.92%，主要是壳和这张稿的像素差，不是新写的表单 |

## 未验证

- 没有在 Windows 上跑凭据管理器、文件索引或 `\\?\` 前缀。
- 没有在 Linux 上跑 Secret Service。
- 开机启动和系统通知只保存，不改系统设置。菜单栏图标开关不创建、不移除托盘。
- 修正之前的密钥测试走了 v1 构造函数，因此碰到过本机钥匙串，测试结束时删了条目。修正后的测试不再走这条路径。事后按服务名和账户查找，钥匙串里没有这条。原生启动只查询「是否已保存」，没有保存或删除。
- 应用没有写入 `usage_daily`。测试用第二条连接塞进夹具行。
- 界面没有调用 `grant`。命令在 capability 里，行为由存储测试覆盖，没有用 WebView 做命令集成测试。
- 稿面「打开系统设置」没有接上，这一行是禁用勾选加未验证徽章。
- 像素差见上表。没有把示例状态改回去凑比例。
- 硬链接、Windows file index、Linux `/proc` 路径仍是 M1a 的限制，这一步没有重测。

## 自查

独立审查见 [自查存储与设置](7ea054e2-5bcd-4960-b70c-49dcbcd516cf)。

结论：存储、恢复授权、用量、发现、命令白名单和界面状态与任务书一致。审查指出一处不符：`keyring` 4 的 `v1::Entry::new` 第一次执行会把默认仓库换成系统钥匙串，先装的模拟后端留不住，REVIEW 里「测试不写本机钥匙串」当时不成立。

已经改掉。模拟后端装上之后，保存、查询和删除只走 `keyring_core::Entry`。`query_stays_on_the_mock_store` 在查询之后把条目向下转换成模拟凭据，转换失败就不会再保存。该测试和 `save_query_and_delete_never_return_the_key` 都已重新通过。按服务名 `local.wanxiang.assistant`、账户 `deepseek` 查找，钥匙串里没有残留条目。

其余条目审查为通过。没有再改 QuotaPot，没有推送。停在这里等审核。

## 审核修复

### 序列化名字

`ToolId::DeepSeekHarness` 加了 `#[serde(rename = "deepseek_harness")]`。存储里的手写 match 删了，读写都走 serde。修复前 `ipc_names_match_the_ui_contract` 和 `grant_parses_the_ui_tool_name` 失败：实际序列化是 `deep_seek_harness`，界面发的 `deepseek_harness` 解析不了。

发给界面的枚举，序列化名和 TypeScript 联合类型一致：

| 枚举 | 序列化名 |
| --- | --- |
| ToolId | `deepseek_harness`、`ds_bot`、`cursor`、`codex` |
| Capability | `usage`、`conversation`、`account_link`、`create_conversation`、`insert_content`、`protected_location`、`cleanup`、`space_scan` |
| GrantStatus | `active`、`needs_reauth` |
| ToolPresence | `missing`、`unauthorized`、`authorized`、`needs_reauth` |
| SummaryMode | `ask`、`local`、`ai` |

除 `deepseek_harness` 外，其余名字在修复前就已经一致。对照文件是 `ui/src/__fixtures__/ipc-names.json`。

新增测试：

- `ipc_names_match_the_ui_contract`：1 通过。把全部变体序列化后和上面的文件比较。
- `ui/ipc-names.test.mjs`：1 通过。文件里的每一组和 `catalog.ts`、`api.ts` 的联合类型完全相等。
- `grant_parses_the_ui_tool_name`：1 通过。用 `{"tool":"deepseek_harness","capability":"usage",...}` 解析 grant 的参数类型。

### 首次启动底部按钮

卡片高度限制在窗口内。标题和步骤条不滚动，工具列表在中间滚动，底部「稍后 / 继续」固定。默认窗口 1280×900 下展开 Cursor 后，按钮底边在 846，窗口高 900，列表内容高于可视区，所以会滚动。

收起状态下重跑 setup 的 pixelmatch，整卡 10.27%，低于原来的 10.73%。头部 8.26%，列表 9.92%，底部 18.86%。底部区域差得多，是因为收起后列表比设计稿短，脚的位置对不齐示例里展开的 Cursor。

原生截图没有点「稍后」或「继续」：

- `design/native/m1b-setup.png`：覆盖。DeepSeek Harness 和 DS Bot 都是「未授权」。
- `design/native/m1b-setup-expanded.png`：Cursor 展开，底部按钮仍在窗口里。
