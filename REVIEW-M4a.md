# REVIEW-M4a

第一步停在审核前。没有提交，没有推送。没有改 QuotaPot，没有改 `cleanup_history`，没有界面。没有扫描或清理真实的 `~/.dsh`、`~/.codex`、Cursor 用户目录。没有调用系统废纸篓。

`PLAN-M4.md` 的三处审核修订已经写进实现。四处和设计稿的差异仍按计划，留给第二步的界面。

## 扫描

每次扫描都把授权根完整走一遍，只用 `symlink_metadata`。不跟随符号链接，不进入另一块盘，硬链接按设备号和 inode 只加一次大小。凭据文件名和 `cursorAuth` 在 `stat` 之前跳过。取消放在目录边界：置位后不再 `stat` 后面的条目。

- `grown_file_counts_even_when_directory_mtime_stays`：目录 mtime 拨回原值，文件从 10 字节加到 15 字节，第二次合计是 15
- `cancel_after_one_directory_does_not_stat_the_next`：`a` 读完后不再 `stat` `b`
- `other_device_is_unscanned_and_not_entered`：另一设备号的目录不进入，里面的文件不被 `stat`
- `credential_names_are_not_stat`：`auth.json`、`.credentials.yaml`、`cursorAuth` 一旦被 `stat` 测试就失败
- `symlink_is_not_followed_and_hardlink_is_counted_once`：符号链接指向的 8000 字节不加进来；硬链接两个名字只算 5 字节，而且不进清单

## 清理

可清理目录本身留下。清单是它的直接子项，记下内容总量、文件数、设备号和 inode。令牌用现有的 `issue_confirmation` / `redeem_confirmation`，绑这组规范字节。执行前再遍历一次。总量、文件数或 inode 对不上，结果是「已跳过 · 内容已变化」。目录自己的 size 和 mtime 不参与。

废纸篓是 `Trasher`。测试注入记录路径的假实现。真的 `trashItem` 还没接。

授权根之外、受保护路径、凭据路径，即使写进清单并且令牌对得上，也不会交给废纸篓。子目录里夹着凭据时，这一项整项拒绝，凭据文件不被 `stat`。

- `appended_nested_file_is_skipped_even_if_the_entry_inode_stays`：`cache/job` 的 inode 没变，往 `nested` 里追加文件后该项被跳过；旁边没变的 `cache/keep` 才会交给假废纸篓
- `directory_mtime_alone_does_not_skip`：只改目录 mtime，假废纸篓仍收到该项
- `changed_list_voids_the_token_and_trash_is_not_called`
- `outside_protected_and_secret_entries_are_refused`：`../out`、`bot/usage.sqlite`、`auth.json` 三条都被拒绝
- `secret_inside_a_listed_entry_is_not_stat_and_not_trashed`
- `trash_faults_use_the_planned_labels`：占用、权限不足、移到废纸篓失败

内存里的 `CleanupRecord` 只有工具、条目数、总大小。`format!("{:?}", record)` 不含 `cache`。逐项结果的相对路径只留在当次返回值里，没有写入数据库。

## 运行中的工具

Cursor 看 `com.todesktop.230313mzl4w4u92`，Codex 看 `com.openai.codex`。`~/.dsh` 同时看 `com.deepseek.dsh` 和进程名。

进程名从 Harness 应用包里读，不写死在判断函数里。只取两处的可执行文件：`Contents/MacOS`，以及 `Contents/Resources/runtime/cli/bin`。本机包里这两处是 `DeepSeek Harness` 和 `dsh`。`dsh` 是一段脚本，里面会 exec 到 `DeepSeek Harness`，所以后台进程的名字是后者；`dsh` 仍参加比对，因为它确实是包里的可执行文件。python、node 不在这份名单里。

读不到应用列表、进程列表或可执行文件名时，停用这棵树的清理，文案是「无法确认 DeepSeek Harness / DS Bot 是否在运行，已停止清理」。应用正在运行时提示「请先退出 DeepSeek Harness」。进程名对上时提示「请先退出 DeepSeek Harness / DS Bot」。这个判断发生在核销令牌之前，所以退出应用后同一枚令牌还能用。

实时进程表还没接。这一步没有清理命令。`RunningView` 由调用方注入；下一步接系统进程名时只读名字，不读参数和环境变量。读失败就传空，清理停用。

- `dsh_process_list_blocks_only_bundle_executable_names`
- `executable_names_come_from_the_bundle_directories_only`：夹具里的 `App Bin`、`cli-tool` 入选，同包里的 `python3` 不入选
- `installed_harness_names_match_the_bundle`：本机 DeepSeek Harness.app 读到的名字包含 `DeepSeek Harness` 和 `dsh`，不含 `python3` 和 `node`
- `running_harness_blocks_before_the_token_is_spent`

## 测试

`cargo test --offline -p wanxiang-core` 通过。其中空间相关是库内 4 项、`tests/space.rs` 16 项、`tests/space_props.rs` 3 项。属性测试覆盖清单字节的顺序和内容、总量或 inode 变化后跳过（mtime 不是入参）、符号链接和硬链接。

## 还没做

界面、概览空间卡、`cleanup_history` 改表、真实只读扫描、真实移到废纸篓。后两件按计划另问。图标那处改动还在工作区，没有单独提交。
