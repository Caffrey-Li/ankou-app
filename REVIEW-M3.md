# REVIEW-M3

实现停在审核前。没有提交，没有推送。这一轮没有读真实对话正文，没有对真实 Codex 发 `thread/list`，没有向 DeepSeek 发摘要，没有打开深链，没有点「整理」「一键开新」「继承对话」「复制整理」「复制继承」「发送并生成」「放入」「导出」。`REVIEW-M0.md` 里改成 `~/Projects/Wanxiang` 的那一处保留。

## 必改 1：`thread/list` 不写 Codex 数据

每次 `thread/list` 都带 `useStateDbOnly: true`。依据是官方文档：省略或为 false 时会扫描 JSONL 日志来修复元数据，那是对第三方数据的写入。`PLAN-M3.md` 审核修订第 1 条写了同一句。

`thread_list_line` 把这个字段写进每一条列表请求。假 app-server 把收到的每一行记下来。

- `thread_list_line_always_sets_use_state_db_only`：通过
- `fake_app_server_requires_use_state_db_only_on_every_list`：通过。至少有一条 `thread/list`，每一条都带 `"useStateDbOnly":true`，没有就失败

## 必改 2：正文的读法和字段

`thread/read` 不带 `includeTurns`，只取摘要信息。正文用 `thread/turns/list` 和 `thread/items/list` 分页。`thread/list` 的返回按官方示例取 `id`、`preview`、`createdAt`、`updatedAt`、`name`、`status`。这条路径没有 `cwd`，工作目录不显示。jsonl 回退只在 rollout 里有 `cwd` 时显示。上下文百分比只在 `last_token_usage.total_tokens` 和 `model_context_window` 都在、且窗口大于 0 时计算；app-server 这条路径没有这两个字段，百分比记为缺失。

分页停在三个上限：最近 40 个回合，或累计 80,000 字，每个回合最多 4 页 items。超过时预览标「长对话只取了最近部分」。

- `thread_read_omits_include_turns`：通过
- `server_list_drops_cwd_and_keeps_documented_fields`：通过
- `jsonl_context_uses_last_usage_over_the_window`：通过。80/100 得到 80，累计 99999 不参与

## 必改 3：读正文单独授权

读消息正文、列出 DS Bot 会话走 `Conversation`，不复用 `Usage`。`measured.rs` 记下两项，界面不提供对应复选框，`set_capability` 拒绝打开，授权保持未验证：

- `codex_conversation`：根 `~/.codex`，模式 `sessions/**/*.jsonl`
- `ds_bot_conversation`：根 `~/.dsh`，模式 `bot/usage.sqlite*`

开发和测试用夹具，授权在内存里发。只有 Usage 授权时，列表和正文都被守卫拒绝。

- `usage_grant_does_not_read_the_conversation`：通过
- `usage_grant_does_not_list_bot_sessions`：通过
- `measured_grants_use_the_two_roots_and_can_turn_off`：通过。两项范围与上面的根、模式一致；`set` 对这两项返回错误，授权表保持空
- `ui/handoff.test.mjs`：通过。设置页源码里没有这两项标识，未验证行保持不可勾选

## 必改 4：令牌和授权

发送 AI 摘要：DeepSeek 账户连接有效，并且钥匙串里有 Key。点「发送并生成」时 `begin_summary` 签发一枚和请求体哈希绑定的一次性令牌，`send_summary` 核销后再由 Rust 发请求。不挂 `InsertContent`，没有额外的工具授权。夹具里点这个按钮只打开预览，不发请求。

打开 Cursor 深链和导出的做法已由文末「审核修复」替换。超过 8000 个 Unicode 标量，或编码后的 URL 超过 10000 字符，不打开、不截断，改为复制，文案是「超过 8000 字，不能打开深链。全文仍可复制。」夹具模式不弹保存对话框，提示「先选择保存位置」。

- `summary_needs_account_and_key_but_not_insert_content`：通过
- `summary_token_rejects_a_different_body`：通过
- `summary_token_expires`：通过
- `summary_tokens_reject_mismatch_reuse_and_expiry`：通过
- `deeplink_needs_create_conversation_and_a_token`：通过
- `export_writes_the_chosen_path_without_a_token`：通过
- `eight_thousand_ascii_characters_build_a_url`：通过
- `chinese_characters_are_not_counted_as_bytes`：通过。8000 个汉字过字数门、不过 URL 门；20 个汉字可以构造 URL
- `character_limit_ignores_byte_length`：通过

## 小改

摘要模型的依据改为：现行文档列出的可选值是 `deepseek-flash` 和 `deepseek-v4-pro`。请求使用 `deepseek-flash`。「deepseek-chat 已停用」已从计划删掉。

计划第 6 节末句改为：M2 的四项真实验证已在 2026-11-09 完成，见 `VERIFY-M2.md`。这四项没有重做。

概览副标题回到日期。「其余工具运行正常」已去掉。已知百分比达到提醒阈值时，概览出现「N 个对话超过提醒阈值」。

## 对照

夹具页 `?compare=1&fixture=m3&page=handoff`，参考图裁到窗口外缘。窗口在导出图里的内容边界是 x=70、y=40、1280×900。pixelmatch 阈值 0.05。夹具文案和稿面示例数字不同，比例差主要来自这里。下面这张表是补「+ 新建交接」和单独裁预览面板之前的比例。这一轮的比例在文末「审核修复」。

| 画面 | 区域 | 尺寸 | 差异比例 |
| --- | --- | --- | --- |
| 对话页 | 整窗 | 1280×900 | 0.0573 |
| 对话页 | 工具栏 | 990×54 | 0.1102 |
| 对话页 | 侧栏 | 224×880 | 0.0372 |
| 对话页 | 对话列表 | 336×780 | 0.0392 |
| 对话页 | 详情 | 620×780 | 0.0759 |
| 预览 | 整窗 | 1280×900 | 0.2804 |
| 预览 | 工具栏 | 990×54 | 0.4860 |
| 预览 | 侧栏 | 224×880 | 0.0546 |
| 预览 | 对话列表 | 336×780 | 0.3754 |
| 预览 | 详情 | 620×780 | 0.3212 |

预览整窗更高，是因为遮罩后面仍是夹具列表，和稿面的示例对话整片不同。

图在：

- `design/refs/m3-handoff.png`、`design/refs/m3-preview.png`（裁到窗口边缘）
- `design/actual/m3-handoff.png`、`design/actual/m3-preview.png`、`design/actual/m3-overview.png`
- `design/diff/m3-handoff.png`、`design/diff/m3-handoff-side-by-side.png`
- `design/diff/m3-preview.png`、`design/diff/m3-preview-side-by-side.png`

## 原生截图

调试窗口曾出现在窗口服务器里：标题「万象小助手」，位置 154,67，大小 1254×883，标记为在屏幕上。对这个窗口取到的图是 2508×1766，像素全是 0。整屏 `screencapture` 得到 3024×1964，同样全黑。`-R` 和按窗口号截取都返回无法创建图像。这个未签名的调试二进制不在可按应用截取的应用列表里。空帧没有留在 `design/native/`。这一项没有可用的原生画面。

## 正文和摘要不落盘

`handoff_text_does_not_land_in_the_database_or_logs`：通过。临时目录里先写入种子数据库并建好空的日志目录，再在内存里做本地整理和摘要请求体。数据库字节与种子相同，日志目录仍为空，正文标记不在数据库文件里。

本机数据目录 `~/Library/Application Support/local.wanxiang.assistant` 在拉起夹具窗口的前后各做了一次校验，没有打开数据库：

- 前：`wanxiang.sqlite`，65536 字节，SHA-256 `8e28bcf948764d4106118a703bcfe5a095af56d8a10df043880aaf3a6e641428`
- 后：同一路径、同一大小、同一哈希
- 目录清单前后相同，只有这一个数据库文件，没有日志目录

这次拉起只打开夹具页，没有发送摘要，也没有导出。

## 测试清单

`cargo test --offline --workspace`、`npx tsc --noEmit -p ui/tsconfig.json`、`npm test` 都通过。Rust 下面每一项都是通过。UI 11 个文件、0 失败。

M3 直接相关：

- `thread_list_line_always_sets_use_state_db_only`
- `fake_app_server_requires_use_state_db_only_on_every_list`
- `thread_read_omits_include_turns`
- `server_list_drops_cwd_and_keeps_documented_fields`
- `jsonl_context_uses_last_usage_over_the_window`
- `usage_grant_does_not_read_the_conversation`
- `usage_grant_does_not_list_bot_sessions`
- `measured::tests::measured_grants_use_the_two_roots_and_can_turn_off`
- `summary_needs_account_and_key_but_not_insert_content`
- `summary_token_rejects_a_different_body`
- `summary_token_expires`
- `summary_tokens_reject_mismatch_reuse_and_expiry`
- `deeplink_needs_create_conversation_and_a_token`
- `export_writes_the_chosen_path_without_a_token`
- `handoff_text_does_not_land_in_the_database_or_logs`
- `eight_thousand_ascii_characters_build_a_url`
- `chinese_characters_are_not_counted_as_bytes`
- `character_limit_ignores_byte_length`
- `ui/handoff.test.mjs`
- `ui/usage.test.mjs`（确认「其余工具运行正常」已不在，概览有「个对话超过提醒阈值」）

同一次 `cargo test` 里其余既有测试也通过：

- codex 适配器：`tests::stays_unverified`、`locate::tests::extra_dirs_are_only_the_two_fixed_bins`、`tests::discovery_uses_the_temp_home_not_this_machine`、`app_server_snapshot_drops_account_fields`、`illegal_used_percent_drops_only_that_window`、`missing_program_is_app_server_unavailable`、`cumulative_rollout_counts_each_increment_once`、`duplicate_rollout_counts_only_the_last_sample`、`reset_rollout_starts_a_new_segment`、`mixed_rollouts_exclude_unmetered_sessions`、`unmetered_rollout_is_missing_not_zero`、`only_explicit_compacted_events_count`、`fixtures_contain_no_email_or_secret_prefix`、`codex_cli_skips_the_desktop_binary_and_finds_the_path_name`、`readers_do_not_open_credential_files`、`app_server_reads_rate_limits_without_extra_params`
- cursor 适配器：`tests::stays_unverified`、`tests::discovery_uses_the_temp_home_not_this_machine`
- dsh 适配器：`tests::stays_unverified`、`tests::discovery_checks_only_the_declared_directory`、`dsh_fixture_matches_the_old_daily_totals`、`calls_are_not_added_to_harness`、`balance_fixture_is_cny_minor_units_not_tokens`、`balance_request_reports_network_failure`、`balance_request_reports_invalid_key_without_echoing_it`、`copied_database_round_trip_and_candidates_skip_credentials`
- app：`commands::tests::grant_parses_the_ui_tool_name`、`commands::tests::capability_lists_each_command_once`、`collect::tests::readers_do_not_open_credential_files`、`discover::tests::temp_home_shows_installed_and_missing_without_reading_files`、`fixture::m2_fixture_page_is_the_parsed_golden_sample`、`collect::tests::periodic_rounds_do_not_start_app_server`、`collect::tests::second_codex_scan_is_at_least_five_times_faster`
- core：`secret::tests::query_stays_on_the_mock_store`、`open_read_rejects_account_link`、`revoked_account_link_does_not_send_the_balance_request`、`revoked_during_balance_request_does_not_persist`、`account_link_is_one_row_for_both_tools`、`failed_refresh_keeps_the_previous_amount`、`confirmation_expires_after_ten_minutes`、`confirmation_redeem_is_single_use_and_bound_to_the_plan`、`confirmation_rejects_unknown_read_only_and_ungranted_actions`、`confirmation_fails_after_its_grant_is_revoked`、`credential_names_are_case_insensitive`、`empty_grant_table_is_empty`、`auth_json_without_a_grant_is_still_denied`、`cursor_global_keys_and_keychain_entries_are_classified_without_files`、`error_text_omits_paths`、`grant_is_visible_with_canonical_root`、`dot_component_is_normalized_and_dotdot_is_rejected`、`grant_rejects_a_missing_or_unusable_root`、`credentials_path_component_is_denied`、`grant_rejects_invalid_patterns`、`credential_files_are_denied_inside_a_grant`、`grant_stores_the_canonical_root`、`dotted_workbuddy_models_json_is_denied_and_other_files_can_be_read`、`identical_grant_returns_the_same_id`、`grants_keep_tools_capabilities_and_scopes_apart`、`missing_file_and_directory_are_not_read`、`revoke_removes_only_that_grant`、`file_patterns_match_one_segment_or_a_tree`、`relative_path_opens_inside_the_granted_root`、`paths_outside_the_granted_root_are_rejected`、`revoked_handle_cannot_read_and_other_capability_stays`、`replaced_root_inode_is_rejected`、`revoking_one_tool_leaves_another_tools_handle`、`revoking_one_scope_leaves_the_other_scope`、`symlink_root_is_stored_as_its_target`、`root_swapped_for_a_symlink_is_rejected`、`space_scan_returns_metadata_and_cannot_read_bytes`、`unknown_token_is_rejected`、`symlink_to_auth_json_is_denied`、`symlink_out_of_root_is_not_read`、`workbuddy_models_json_is_denied`、`named_pipe_is_not_a_file_and_does_not_block`、`ipc_names_match_the_ui_contract`、`auth_json_leaf_is_denied`、`dotdot_is_always_rejected`、`string_prefix_of_the_root_is_unregistered`、`sibling_directories_are_unregistered`、`safe_relative_paths_open`、`missing_window_duration_is_unknown_period`、`partial_sessions_join_the_banner`、`app_server_windows_say_actual_and_shorter_period_comes_first`、`excluded_calls_are_named_in_the_banner`、`empty_usage_names_the_gaps_and_skips_example_figures`、`four_usage_states_stay_distinct`、`refresh_interval_is_fifteen_minutes`、`revoking_codex_usage_stops_its_child`、`shutdown_stops_the_refresh_loop_and_its_child`、`save_query_and_delete_never_return_the_key`、`shutdown_can_be_called_twice`、`shutdown_stops_background_task`、`shutdown_stops_child_process`、`presence_uses_install_and_grant_state`、`store_error_text_has_no_path`、`default_settings_keep_history_for_twelve_months_and_leave_residency_off`、`history_tables_do_not_store_conversation_text`、`grant_survives_a_restart`、`replaced_root_needs_reauth_and_is_not_granted`、`retention_cutoff_uses_the_local_day_when_utc_has_moved_ahead`、`clear_history_leaves_settings_and_grants`、`retention_drops_days_older_than_the_window`、`minor_units_keep_two_decimal_places`、`percent_outside_0_to_100_is_missing`、`retention_keeps_the_cutoff_day`、`money_strings_do_not_become_zero_when_invalid`

UI 文件，全部通过：

- `ui/commands.test.mjs`
- `ui/handoff.test.mjs`
- `ui/icons.test.mjs`
- `ui/ipc-names.test.mjs`
- `ui/offline.test.mjs`
- `ui/release-strip.test.mjs`
- `ui/settings-tags.test.mjs`
- `ui/shell.test.mjs`
- `ui/tokens.test.mjs`
- `ui/tool-status.test.mjs`
- `ui/usage.test.mjs`

## 仍需单独同意的真实验证

这四项没有做：对用户指定的一条线程发一次 `thread/list` 和一次不带 `includeTurns` 的 `thread/read`；用户看过预览后再发一次 DeepSeek 摘要；用户打开开关后再打开一次短深链；读取真实 Cursor 对话库。

## 审核修复

没有提交，没有推送。官方插件已经进了本地仓库，离线构建没有改用别的打开方式或别的对话框。`tauri-plugin-opener` 2.7.2，`tauri-plugin-dialog` 2.8.4。

### 深链确认令牌

`open_deeplink` 已从命令、`app/build.rs` 和 `app/capabilities/default.json` 去掉。预览出现时界面调用 `prepare_deeplink(text)`：先看深链开关和 Cursor 的 `CreateConversation` 授权，再查长度；都通过才签发和原文哈希绑定的令牌，返回令牌，不兑换，不返回 URL。用户点「放入 Cursor 新对话」时调用 `place_deeplink(token, text)`：兑换、构造 URL、用 `app.opener().open_url` 打开，都在 Rust 里。命令返回空，URL 不回给界面。

opener 只给了 `opener:allow-open-url`，允许范围只有 `cursor://anysphere.cursor-deeplink/*`。没有 `opener:default`、`allow-default-urls`、`allow-open-path`。没有 `Command::new("open")`。

- `deeplink_needs_create_conversation_and_a_token`：通过。未授权是 `NotGranted`，开关关闭是 `Off`；签发后兑换打开一次，同一枚令牌再兑换不会打开
- `place_deeplink_does_not_open_without_a_prepared_token`：通过。没有这枚令牌、令牌和原文哈希不一致、令牌用过一次，都不会打开
- `prepare_deeplink_does_not_redeem_in_the_same_call`：通过。同一次调用只签发；随后 `place_deeplink` 还能兑换这枚令牌，说明签发时没有把它用掉
- `export_and_place_commands_take_no_path_and_cannot_open_an_arbitrary_url`：通过。命令源码里没有 `fn open_deeplink`、`osascript`、`Command::new("open")`
- `capability_lists_each_command_once`：通过。capability 里没有能打开任意 URL 的命令；opener 的允许列表只有上面那一个范围

### 导出

`export_handoff` 只收 `text`。保存位置由 `app.dialog().file().blocking_save_file()` 弹出系统保存对话框，macOS 和 Windows 都走这个插件。`osascript` 已删。对话框返回空就不写文件。capability 里只有 `dialog:allow-save`。

- `export_and_place_commands_take_no_path_and_cannot_open_an_arbitrary_url`：通过。`export_handoff` 的签名有 `text: String`，没有路径参数
- `export_dialog_cancel_writes_nothing`：通过。`write_export_if_chosen` 在路径为 `None` 时不写任何文件

### 界面

对照 18:646、20:73、20:216。

- 对话列表：百分比下面有 64×4 的进度条。达到提醒阈值用赭色，其余用蓝色。「暂不提供」和「未知」各自带一行灰色小字「仍可交接」
- 工具栏恢复「+ 新建交接」。选中一条对话后，这个按钮打开交接预览。搜索在列表卡片里，交接页工具栏没有搜索图标
- 详情增加「换到其他工具」，副文案是「Codex、DS Bot、Harness · 复制后手动粘贴」。工作目录缺失时这一行不出现
- 预览弹窗在窗口里居中，外宽 780。关闭按钮是 28px 圆形 ×。选择、生成是勾，预览是蓝色圆点，放入是灰色圆点。来源行是「工具 · 对话名（HH:MM 快照）→ 目标」。复制按钮带图标，放入按钮带箭头，文案是「放入 Cursor 新对话」

夹具对照仍用窗口外缘裁切，pixelmatch 阈值 0.05。预览弹窗按白色面板单独裁出，外宽 780，不把背后的列表算进去。夹具文案和稿面示例不同，比例差主要在文案。

| 画面 | 区域 | 尺寸 | 差异比例 |
| --- | --- | --- | --- |
| 对话页 | 整窗 | 1280×900 | 0.0558 |
| 对话页 | 工具栏 | 990×54 | 0.0597 |
| 对话页 | 侧栏 | 224×880 | 0.0372 |
| 对话页 | 对话列表 | 336×780 | 0.0409 |
| 对话页 | 详情 | 620×780 | 0.0770 |
| 预览 | 整窗（含背后列表） | 1280×900 | 0.2779 |
| 预览弹窗 | 单独白面板 | 780×492 | 0.0546 |

图在 `design/actual/m3-handoff.png`、`design/actual/m3-preview.png`、`design/diff/m3-preview-panel.png`、`design/diff/m3-preview-panel-side-by-side.png`。

### 原生截图

整屏截图已经有画面，屏幕录制对这次截图进程是开着的。按窗口号截「万象小助手」，网页内容仍是黑的，只剩标题栏一条。按窗口矩形去截屏幕时，坐标和窗口对不齐，没有得到可用的窗口画面，那张图没有放进 `design/native/`。

请在系统设置里确认「隐私与安全性 → 屏幕录制」已经允许终端或 Cursor。若仍不方便用自动截图，请按 Cmd+Shift+4，再按空格，点「万象小助手」窗口，把对话页和打开预览后的两张图存进 `design/native/`。

拉起调试窗口时系统弹出了钥匙串授权。没有点允许，程序已经退出。若这个窗口还在，请点「拒绝」。数据库没有打开，`wanxiang.sqlite` 仍是 65536 字节，SHA-256 `8e28bcf948764d4106118a703bcfe5a095af56d8a10df043880aaf3a6e641428`。

### 测试

- `cargo test --offline --workspace`：通过
- `npx tsc --noEmit -p ui/tsconfig.json`：通过
- `npm test`：11 个文件，0 失败

停在这里等审核。上面「导出」一节里的 `blocking_save_file` 和 capability 里的 `dialog:allow-save`、opener 范围，已由下面「第二轮」替换。

## 第二轮

### capability 清理

`app/capabilities/default.json` 不再放行 opener 和 dialog 插件，也没有 `core:default`。网页不能直接调用打开 URL 或保存对话框。打开和保存只留在 Rust 命令里：`place_deeplink` 用 `app.opener().open_url`，`export_handoff` 用 `app.dialog().file().save_file`。插件仍在 `run()` 里注册。

- `capability_has_no_opener_dialog_or_plugin_defaults`：通过。每一项权限都不是 `opener:`、`dialog:`，名字里也没有 `default` 段
- `capability_lists_each_command_once`：通过。没有 `open-deeplink`、`allow-open-path`、`allow-default-urls`

### 导出改成异步

`export_handoff` 是 `async`。保存对话框用 `save_file` 的回调把选择送进通道，命令在通道上等待。不再调用 `blocking_save_file`。用户取消时通道得到空值，`write_export_if_chosen` 不写文件。

- `export_and_place_commands_take_no_path_and_cannot_open_an_arbitrary_url`：通过。签名是 `pub async fn export_handoff`，只有 `text: String`，源码里没有 `blocking_save_file`
- `export_dialog_cancel_writes_nothing`：通过

### 原生窗口

导出：在夹具预览里点了一次「导出」。系统保存对话框出现，标题是「导出交接」，文件名是「交接.md」。点了 Cancel。对话框消失后，预览还在，「导出」和「放入 Cursor 新对话」仍可找到，界面没有卡住。`~/Documents/交接.md` 不存在。应用数据目录仍只有 `wanxiang.sqlite`，大小和 SHA-256 与打开前相同：65536 字节，`8e28bcf948764d4106118a703bcfe5a095af56d8a10df043880aaf3a6e641428`。没有点 Save，没有点「放入」。

深链：未在原生窗口验证。

### 开发版钥匙串弹窗

调试二进制没有签名。启动时 `is_saved` 会读系统钥匙串，macOS 可能弹出授权，问这个程序能否使用 `local.wanxiang.assistant` 的钥匙串项。点「拒绝」后查询失败，界面按未保存密钥继续，不写入钥匙串。不要点「允许」。这一轮的导出验证没有点允许。

### 测试

- `cargo test --offline --workspace`：通过。其中 `capability_has_no_opener_dialog_or_plugin_defaults`、`capability_lists_each_command_once`、`export_and_place_commands_take_no_path_and_cannot_open_an_arbitrary_url`、`export_dialog_cancel_writes_nothing` 都通过
- `npx tsc --noEmit -p ui/tsconfig.json`：通过
- `npm test`：11 个文件，0 失败

用户说明原生截图在 `design/native/m3-handoff.png` 和 `design/native/m3-preview.png`。落笔时这两个文件不在工作区里。

## M3.1

仓库里不再打包第三方工具图标。`ui/src/tool-icons/` 已删除。图标由命令 `tool_icons` 在 Rust 里从应用程序包读取，不读用户数据目录。读不到的继续显示原来的字母块。

### 读到的图标

| 工具 | 结果 |
| --- | --- |
| Cursor | 有。`read_dir(/Applications)` 确认目录名就是 `Cursor.app`，再用 `NSWorkspace iconForFile` 画成 128×128 PNG |
| DeepSeek Harness | 有。不按应用包文件夹名匹配。`/Applications` 里唯一一个 `CFBundleIdentifier` 为 `com.deepseek.dsh` 的包，其 `CFBundleName` 是 DeepSeek Harness，因此读取。缓存路径是 `/Applications/DeepSeek Harness.app` |
| DS Bot | null。没有独立应用，侧栏仍是字母 B |
| Codex | null。`/Applications` 里没有大小写完全一致的 `Codex.app`。不读 ChatGPT.app。侧栏仍是字母 Cx |

Windows 只有接口：`cfg(target_os = "windows")` 下直接返回四个 null。未实现。

PNG 缓存在应用数据目录的 `icon-cache/`。文件名是应用包路径的哈希，旁边的 stamp 记下路径和 mtime；mtime 变了才重新读取。这次启动写出两张 128×128：Cursor 和 DeepSeek Harness。`wanxiang.sqlite` 仍是 65536 字节，SHA-256 `8e28bcf948764d4106118a703bcfe5a095af56d8a10df043880aaf3a6e641428`。

`img-src` 里本来就有 `data:`，这次没有改其他 CSP 指令。`connect-src` 仍只有 `ipc:` 和 `http://ipc.localhost`。

侧栏截图：`design/native/m3.1-sidebar.png`。Harness 是鲸鱼图标，Cursor 是应用图标，DS Bot 是 B，Codex 是 Cx。

### 窗口拖动

红绿灯那一行（`.traffic`）和工具栏标题与按钮之间的空白（`.toolbar-drag`）加了 `data-tauri-drag-region`。按钮和输入框没有这个属性。capability 只多了 `core:window:allow-start-dragging`。`core:` 开头的权限现在是 `core:event:allow-listen`、`core:event:allow-unlisten`、`core:window:allow-start-dragging`。

原生窗口里按住顶部空白拖了一次。拖动前窗口在 (140, 58)，拖动后在 (172, 60)。窗口能移动。拖完侧栏仍在，图标没有回到字母块。没有点「放入」或「导出」。这次没有出现钥匙串弹窗。

### 测试

- `cargo test --offline --workspace`：通过
- `npx tsc --noEmit -p ui/tsconfig.json`：通过
- `npm test`：12 个文件，0 失败
