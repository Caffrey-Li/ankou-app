# REVIEW-M1a

里程碑：授权守卫核心。分支 `m1a-guard`，从 `8d1fe9f` 拉出。只改了 `crates/core` 的逻辑和测试，外加离线扫描对守卫黑名单名字的放行。没有界面，没有 SQLite。没有提交，没有推送。

独立审查见 [审查授权守卫](f443d03b-1234-4822-8f6f-8959af2ddfa7)。那次审查没有跑测试。审查之后补了两处：相对路径改成打开授权根下的绝对路径；去掉 `\\?\` 之后如果还有 `..`，词法检查返回 `PathEscape`，不再把 `..` 丢掉。

## 改了什么

`wanxiang_core::guard` 提供授权表、路径检查、凭据黑名单、只读句柄和确认令牌。调度器用 `ToolScheduler::on_revoked(tool, capability)` 通知，时钟用可注入的 `Clock`。确认令牌默认 10 分钟。

登记根目录时 `canonicalize`，保存真实路径和 `(dev, inode)`。Windows 上用卷序列号和 file index，比较前去掉 `\\?\` 前缀。父目录是符号链接时不拒绝。打开时如果根的真实路径或 inode 变了，返回 `SymlinkEscape`。

`open_read` 先做词法检查，再打开文件，然后从已打开的句柄取真实路径：macOS `F_GETPATH`，Windows `GetFinalPathNameByHandleW`，Linux `/proc/self/fd`。真实路径不在根内，或是凭据文件，就关掉句柄并拒绝。`SpaceScan` 只能走 `metadata`（大小、修改时间、是否目录），调用 `open_read` 返回 `NotGranted`。

直接依赖 `sha2`、`getrandom`。开发依赖 `proptest`。`ui/offline.test.mjs` 仍然禁止网络客户端和 `state.vscdb`。`auth.json`、`cursorauth`、`.credentials` 只允许出现在守卫源码和它的两个测试文件里，因为拒绝这些名字必须把名字写出来。

## 自查

对照任务书第 1 节：

1. 通过。黑名单在读内容之前拒绝 `auth.json`、`.credentials.yaml`、路径中的 `credentials` / `.credentials*`、WorkBuddy 的 `models.json`。`classify_cursor_global_key` 拒绝 `cursorAuth` 和 `cursorAuth/*`。`classify_keychain` 一律拒绝，且不调用系统钥匙串。没有读取验证码、登录链接或密钥内容的代码。
2. 通过。打开文件只用读权限。没有 SQLite，没有写 `state.vscdb`。
3. 通过。测试目录来自 `std::env::temp_dir()`。没有读取本机 Cursor 或其他 AI 工具的数据目录。
4. 通过。没有交接、放入或清理用户数据的函数。`Cleanup` 只是授权种类。测试结束只删除自己的临时目录。
5. 通过。没有运行 `probe-locate.mjs`。测试字节是代码里写死的 `wanxiang-fixture`、`secret-auth` 这类假数据。错误文本不包含路径。
6. 通过。没有改 QuotaPot。没有做界面和 SQLite。

对照本步 5 项：

1. 通过。`grant`、`grants`、`grants_for`、`revoke`。范围是根目录加 `*` / `**` 文件模式。
2. 通过。`lexical_within` 拒绝 `..`，句柄路径跳出根返回 `SymlinkEscape`，未登记根返回 `UnregisteredRoot`。`path_props.rs` 有 5 条属性测试，各 64 例。
3. 通过。上述凭据即使落在授权根里也拒绝。模式写成 `auth.json` 同样拒绝。
4. 通过。`ScopedRead::read` 只读。撤销后下一次读取返回 `GrantRevoked`，并按工具和能力通知调度器。同一工具的另一项能力、另一个根、另一个工具的句柄仍可读。
5. 通过。只有创建目标对话、放入内容、清理能签发令牌。哈希绑定工具、能力和计划字节。内容、工具或能力一变就作废。成功兑换后不能再用。

对照审核追加的 7 条：第 1、5、6、7 条有对应测试并已在本机通过。第 2 条里，本机 `/tmp` 的父级符号链接由 `grant_stores_the_canonical_root` 覆盖；inode 被替换、根被换成符号链接这两条也通过。Windows 的 file index 和 `\\?\` 前缀只做了交叉编译，没有在 Windows 上跑。第 3 条的 macOS `F_GETPATH` 由符号链接测试跑过；Windows 和 Linux 的取路径只编译了。第 4 条里，`auth.json.`、结尾空格和 `auth.json::$DATA` 在本机按名字拒绝并通过；8.3 短名测试只在 Windows 编译，没有执行。硬链接按审查要求记在下面。

## 命令和输出摘要

`cargo test -p wanxiang-core`（本机 macOS，aarch64）：

- 库内测试 0
- `tests/guard.rs`：34 通过，0 失败，约 0.01s
- `tests/path_props.rs`：5 通过，0 失败，约 0.19s
- `tests/shutdown.rs`：3 通过，0 失败，约 0.12s（含原有的退出测试）
- 文档测试 0

编译无 warning。实现之前，守卫测试因 `not yet implemented` 失败。相对路径测试在修正前失败，错误是 `Io(NotFound)`，因为当时打开的是进程当前目录。

`cargo check -p wanxiang-core --target x86_64-pc-windows-msvc --tests`：通过。这是编译，不是运行。

`npm test`：4 通过（tokens、icons、offline、shell）。

## 测试清单

本机通过：

| 测试 | 覆盖 |
| --- | --- |
| `empty_grant_table_is_empty` | 空表 |
| `grant_is_visible_with_canonical_root` | 授予后能查到工具、能力、规范根和模式 |
| `grants_keep_tools_capabilities_and_scopes_apart` | 工具、能力、根目录分开 |
| `identical_grant_returns_the_same_id` | 相同授权不重复 |
| `revoke_removes_only_that_grant` | 撤销一条，再撤报不存在，只通知一次 |
| `grant_rejects_a_missing_or_unusable_root` | 缺根、相对路径、文件根 |
| `grant_rejects_invalid_patterns` | 空模式、绝对路径、`..`、反斜杠 |
| `grant_stores_the_canonical_root` | 保存 canonicalize 后的根，并能读文件 |
| `symlink_root_is_stored_as_its_target` | 根自身是符号链接时保存目标 |
| `file_patterns_match_one_segment_or_a_tree` | `*`、`*.json`、`sub/*.json`、`**/*.json` |
| `dot_component_is_normalized_and_dotdot_is_rejected` | `.` 规范化，`..` 拒绝，根外哨兵不变 |
| `paths_outside_the_granted_root_are_rejected` | 根外、别的工具、别的能力、字符串前缀 |
| `missing_file_and_directory_are_not_read` | 不存在为「文件不存在」，目录不是文件 |
| `symlink_out_of_root_is_not_read` | 跳出根的符号链接，打开和元数据都拒绝 |
| `replaced_root_inode_is_rejected` | 根目录换成新 inode 后拒绝 |
| `root_swapped_for_a_symlink_is_rejected` | 根被换成指向别处的符号链接后拒绝 |
| `credential_files_are_denied_inside_a_grant` | 根内 `auth.json`、`.credentials.yaml`，模式点名也拒绝 |
| `credential_names_are_case_insensitive` | 大小写、结尾点、结尾空格、`::$DATA` |
| `workbuddy_models_json_is_denied` | WorkBuddy 下的 `models.json` |
| `symlink_to_auth_json_is_denied` | 普通名字指向 `auth.json` |
| `auth_json_without_a_grant_is_still_denied` | 没有授权也拒绝 |
| `credentials_path_component_is_denied` | `credentials`、`.credentials`、`.Credentials.bak` |
| `cursor_global_keys_and_keychain_entries_are_classified_without_files` | `cursorAuth/*` 与钥匙串，不读文件 |
| `space_scan_returns_metadata_and_cannot_read_bytes` | 能拿到大小和修改时间，读不到字节 |
| `revoked_handle_cannot_read_and_other_capability_stays` | 撤销后句柄失效，另一项能力仍可读，通知带能力 |
| `revoking_one_scope_leaves_the_other_scope` | 撤一个根，另一个根仍可读 |
| `revoking_one_tool_leaves_another_tools_handle` | 撤一个工具，另一个工具的句柄仍可读 |
| `relative_path_opens_inside_the_granted_root` | 相对路径打开的是根里的文件 |
| `confirmation_redeem_is_single_use_and_bound_to_the_plan` | 单次、内容或工具或能力一变就作废、空计划、Debug 不露编号 |
| `confirmation_rejects_unknown_read_only_and_ungranted_actions` | 非写入、未授权、未知令牌 |
| `unknown_token_is_rejected` | 另一个守卫持有的令牌不能兑换 |
| `confirmation_expires_after_ten_minutes` | 差 1 秒仍有效，满 10 分钟过期 |
| `confirmation_fails_after_its_grant_is_revoked` | 对应写入授权撤销后兑换返回 `NotGranted` |
| `error_text_omits_paths` | 显示文本不含临时目录 |
| `safe_relative_paths_open` 等 5 条属性测试 | 随机安全路径、`..`、兄弟目录、前缀陷阱、`auth.json` |

只编译、没有运行：

| 测试 | 原因 |
| --- | --- |
| `windows_short_name_of_auth_json_is_denied` | `#[cfg(windows)]`。交叉编译通过，本机不是 Windows |

## 未验证

- 没有在 Windows 上运行测试。file index、`\\?\` 前缀、`GetFinalPathNameByHandleW`、8.3 短名是否展开成 `auth.json`，都没有运行结果。短名测试在 8.3 被关闭的卷上会失败。
- 没有在 Linux 上跑 `/proc/self/fd`。
- 硬链接是已知限制。用另一个名字硬链接到 `auth.json` 时，macOS 的 `F_GETPATH` 返回打开时的那个名字，句柄检查认不出它。本机用临时文件确认过这个系统行为，守卫没有为此开洞测试，也没有去读真实凭据。升级路径是再按 `(dev, inode)` 比对已见过的凭据文件。
- 重复授予不刷新 inode。根目录被换成新目录后，再授同样的工具、能力和模式仍返回旧编号，打开继续 `SymlinkEscape`，要先撤销再授。这是故意不把换过的目录自动重新授权。
- 已经进入系统 `read` 的那一次不会被撤销打断。下一次调用会失败。
- 同一写入能力有两条范围时，撤销一条后，旧令牌只要还剩签发时记下的另一条，仍可兑换。测试只覆盖了唯一一条被撤销。
- 没有做界面、SQLite、真正的定时任务，也没有把授权存下来。万象自己的 DeepSeek Key 不走 `classify_keychain`。
- 没有读 QuotaPot，没有读本机 AI 工具的数据或凭据。

## 审核后修改

1. WorkBuddy 的明文 Key 在 `~/.workbuddy/models.json`。判断目录名时先去掉开头的点，再和不区分大小写的 `workbuddy` 比较。`.workbuddy`、`.WorkBuddy`、`WorkBuddy` 都会命中。`<根>/.workbuddy/models.json` 返回 `CredentialDenied(WorkBuddyModels)`；`<根>/.workbuddy/sessions.json` 在授权和模式允许时可以读取。
2. Unix 上 `open_handle` 带 `O_NONBLOCK | O_CLOEXEC`。打开后用 `fstat`（`File::metadata`）确认类型：`open_read` 只要普通文件，`metadata` 要普通文件或目录。命名管道、套接字、设备文件返回 `NotAFile`。临时目录里的 `mkfifo` 测试会立刻返回 `NotAFile`，不会卡住。修正前这条测试会停在打开管道上。

`cargo test -p wanxiang-core`：`tests/guard.rs` 36 通过，`tests/path_props.rs` 5 通过，`tests/shutdown.rs` 3 通过。新增的两条都在其中。`npm test`：4 通过。
