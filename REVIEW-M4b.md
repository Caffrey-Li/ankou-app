# REVIEW-M4b

M4b 第二轮停在审核前。没有提交，没有推送。没有改 QuotaPot。没有扫描或清理真实的 `~/.dsh`。原生验证只用临时目录，并把其中一个夹具文件放进了系统废纸篓。

## M4a 补丁

执行前从授权根到条目逐级 `symlink_metadata`。中间每一级必须是目录、不是符号链接、设备号和根相同。最后一级必须是文件或目录，不能是符号链接。任何一级不满足，这一项是「已跳过 · 内容已变化」，假废纸篓收不到。

- `swapped_cache_symlink_is_skipped`：清单生成后把 `cache` 换成指向临时目录外的符号链接，执行时这一项被跳过

Cleanup 不再使用 `**`。`CLEANUP_PATTERNS` 是 `cache/*`、`logs/*`、`attachments/v1/tmp/*`、`storages/session_projcache/*`。`cleanup_root` 要求这 4 条都在、根相同，再用 `stamp` 核对根的设备号和 inode。不对根调用 Cleanup 的 `metadata`，因为根匹配不上 `cache/*`。

`measured.rs` 用 `harness_cleanup` 登记这 4 条。空间扫描另有 `harness_space`，模式是 `**`，否则统计不到受保护文件。关掉 `harness_cleanup` 后这 4 条都撤掉。

- `guard_rejects_nested_and_protected_children`：`cache/job/nested` 和 `bot/note` 是 `PatternMismatch`，`cache/item` 放行
- `harness_cleanup_is_four_direct_child_patterns`：登记结果就是这 4 条，深层路径和受保护子项同样被守卫拒绝

## 命令和 capability

capability 比上一轮多一条 `allow-cancel-scan`。四条命令：

| 命令 | capability | 参数 |
| --- | --- | --- |
| `scan_space` | `allow-scan-space` | `tool`。async，扫描在 `spawn_blocking` 里 |
| `cancel_scan` | `allow-cancel-scan` | `tool`。把该工具的取消标志设为真 |
| `prepare_cleanup` | `allow-prepare-cleanup` | `tool`，条目的相对路径和清单数字，返回令牌 |
| `run_cleanup` | `allow-run-cleanup` | `tool`，令牌，同样的条目。async |

界面不传绝对路径。`ui_entry` 拒绝绝对路径、`..`、空段和带冒号的路径。返回给界面的扫描结果没有授权根。清理结果有显示名和状态，没有绝对路径。

`scan_space` 和 `run_cleanup` 都不再在命令里调用 `run_on_main_thread`。取消标志放在 `Hub` 的 `ScanSlots` 里，每个工具一份 `Arc<AtomicBool>`。`begin` 开始一次扫描时把标志清成假；`cancel_scan` 把它设为真。扫描在每个目录边界读这个标志，置位后不再 stat 后面的条目。

`run_cleanup` 把进程核对和 `NSFileManager.trashItemAtURL` 放进 `spawn_blocking`。这两处不需要主线程。读进程失败时列表是空的 `Option`，清理停在「无法确认」。Windows 上这条命令直接返回「Windows 暂不支持」，不调用废纸篓。按钮禁用，文案相同。

开发构建里 `WANXIANG_SPACE_FIXTURE` 只替换 DeepSeek Harness 的 SpaceScan 和 Cleanup 授权根。`set()` 里的用量授权仍用真实的 `~/.dsh`。发布构建不包含这段读取，`release-strip` 检查二进制里没有这个字符串。夹具模式也不再读钥匙串。

进程列表：`NSWorkspace.runningApplications` 取 bundle id；`proc_listpids` 加 `proc_name` 只取进程名。不读参数、环境变量，也不调用 `proc_pidpath`。Harness 可执行文件名仍从应用包的 `Contents/MacOS` 和 `Contents/Resources/runtime/cli/bin` 读取。

## cleanup_history

列是 `created_at`、`tool`、`entry_count`、`total_bytes`。没有路径，也没有原来的 `summary`。旧表如果还有 `summary`，迁移时整表丢掉重建，不把旧行拷过来。

- `cleanup_history_records_counts_without_paths`
- `old_cleanup_summary_rows_are_dropped`
- `commands_scan_prepare_and_record_without_paths`：绝对路径签不出令牌；相对路径清理后历史只有工具、1 条、4 字节

## 测试

`cargo test --offline --workspace` 全部通过。`npm test` 12 项通过，包含发布构建剥离 `WANXIANG_SPACE_FIXTURE`。

和这一轮直接相关的有：

- `cancel_set_after_start_stops_before_the_next_directory`：扫到目录 `a` 时置位，不再 stat `a` 里的文件，也不进入下一个目录
- `cancel_scan_sets_the_flag_a_running_scan_reads`：`cancel` 改的是 `begin` 交出去的同一份标志，下一次 `begin` 会清掉
- `space_fixture_replaces_the_dsh_scan_and_cleanup_roots`：临时目录替换扫描和清理根，连调两次仍然是 1 条 `**` 和 4 条 Cleanup

上一轮已经在的：

- 符号链接替换 `cache` 后跳过
- 守卫拒绝 `cache` 下更深一层，以及受保护目录的直接子项
- 4 条 Cleanup 授权可以关掉
- 界面条目只接受相对路径
- 历史不记路径，旧的 `summary` 行被丢掉
- 进程读取源码只含 `proc_listpids`、`proc_name`、`runningApplications`、`trashItemAtURL`

## 对照

`scripts/compare-m4.mjs`，pixelmatch 阈值 0.05。参考图先裁到窗口边缘。夹具数字是 6.40 GB / 980 MB，不是稿面的 14.6 GB / 1.60 GB。打开页面默认不勾选。受保护数据合成「其余用户数据」。确认文案是「将移到废纸篓，可以在废纸篓里找回」。

弹窗参考图先按原比例裁出白面板，再等比放进实际弹窗，不再把宽高分别拉满。

| 区域 | 差异比例 |
| --- | --- |
| 空间窗口 20:227 整窗 | 0.1169 |
| 20:227 指标 | 0.1118 |
| 20:227 工具列表 | 0.0690 |
| 20:227 详情 | 0.1073 |
| 空间页 20:223 整窗 | 0.2518 |
| 20:223 指标 | 0.1180 |
| 20:223 工具列表 | 0.0700 |
| 20:223 详情 | 0.1209 |
| 概览 18:70 整窗 | 0.2458 |
| 概览「需要处理」 | 0.1035 |
| 概览空间卡 | 0.1307 |
| 确认弹窗（白面板等比，相对 21:198 第三张卡） | 0.2366 |
| 结果弹窗（白面板等比，相对 21:198 第四张卡） | 0.2268 |

20:223 和概览的整窗仍然更高，裁完后的留白和示例数字都和 20:227 不一样。弹窗比例下来一截。剩下的差主要是正文：确认用的是审核过的废纸篓文案，数字是夹具的 800 MB，不是稿面的 1.60 GB。

这一轮界面：左侧列表 400px，名字和「超过阈值」同一行，进度条在名字下面、宽 170px。受保护行前面有锁。清理按钮是「清理所选项」，带垃圾桶图标。应用在运行时按钮不可用，提示是「请先退出 {应用名}」，没有「建议」。工具栏是「扫描于 HH:MM」和刷新，没有搜索。副标题只有「按工具统计本机已知占用 · 阈值 5 GB」。确认和结果弹窗顶部有图标块，标题和正文居中。结果逐项是名字、状态点、状态。「完成」是蓝色主按钮，「重试失败项」是次要按钮。扫描进行时有进度和「取消」。

浏览器对照图在 `design/actual/m4-space.png`、`design/actual/m4-overview.png`、`design/actual/m4-confirm-card.png`、`design/actual/m4-result-card.png`。并排图在 `design/diff/`。

## 原生验证

调试构建用 `WANXIANG_SPACE_FIXTURE=/tmp/wanxiang-m4-fixture.5WJdLG` 和 `WANXIANG_SKIP_REFRESH=1` 打开。这个临时目录里只有 `cache/wanxiang-m4-drop.txt` 和 `cache/wanxiang-m4-keep.txt`。启动前数据库里没有 SpaceScan 或 Cleanup 授权；夹具模式加上的 5 条都指向这个临时目录。验证结束后这 5 条已经删掉，原来的两条用量授权还在。没有读钥匙串，也没有点「允许」。

窗口里点了刷新。扫描返回后工具栏是「扫描于 05:22」，列表出现这两个文件，界面可以继续点。勾了 drop 那一项，点「清理所选项」，确认弹窗写着「将移到废纸篓，可以在废纸篓里找回」。点「清理」之后结果弹窗是「清理完成」，条目名「运行缓存」，状态「已清理」。`keep` 还在临时目录里。`drop` 现在在 `~/.Trash/wanxiang-m4-drop.txt`。DeepSeek Harness 没有在运行，所以没有出现「请先退出」。

结果弹窗第一行当时写成了「实际释放 0 B」。清理完成后界面把勾选清掉了，这一行还在读已选体积。已经改成读这次清理返回的 `total_bytes`。截图是改之前的那一帧，状态文字「已清理」是对的。

三张图在 `design/native/m4-space.png`、`design/native/m4-confirm.png`、`design/native/m4-result.png`。调试进程已经停掉。
