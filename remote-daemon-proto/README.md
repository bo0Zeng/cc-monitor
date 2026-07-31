# cc-monitor-remote (远端 daemon)

cc-monitor 的 SSH-远端功能后端 daemon（issue #15 起，已历 F14–F30+ 多轮迭代）。它 tail 远端
`~/.claude` 会话 JSONL 并流式回传已连接的 client（cc-monitor / 未来 aterm）。协议契约的权威文档是
[`../doc/IPC-PROTOCOL.md`](../doc/IPC-PROTOCOL.md)；部署见 [`../doc/REMOTE-PHASE0-DEPLOY.md`](../doc/REMOTE-PHASE0-DEPLOY.md)。

- **运行期 Linux-only。** inotify watcher 流式（`src/watcher.rs`）+ 一次性历史查询子命令（`src/history_query.rs`/`src/search_query.rs`）。
- **交叉编译 + 内嵌部署。** 由 CI（`release.yml`）用 `cargo zigbuild` 交叉编译 x86_64/aarch64 musl 静态二进制，
  **内嵌进 `cc-monitor.exe`**（`src-tauri/embedded-daemons/`），首次连远端时经 SFTP 自部署到 `~/.cc-monitor/bin/`
  （`sftp::ensure_daemon_deployed`，按 arch + `.build_id` 门控）。**不再需要在目标机上手动 build**。
- **Standalone crate。** 故意 *不* 进 Cargo workspace、无根 `Cargo.toml` 引用它——避免 Windows/Tauri CI 去
  编译这个 Linux-only crate。CI 单独在 ubuntu 跑它的 `cargo fmt --check`/`clippy`/`test`（`.github/workflows/ci.yml` 的 daemon job）。

## 版本 / 身份 / 能力（三轴正交，见 `../doc/INVARIANTS.md` §26/§28）
- `PROTO_VERSION`（`main.rs`）：只在**破坏性 wire 变更**时 bump；additive 新帧/新能力**不** bump。
- `BUILD_ID`（`main.rs`）：人读构建标 = **身份**，管 staleness 检测 + 重部署确认（单源自源码，`build.rs` 编译期提取）。
- `CAPABILITIES`（`main.rs`）：daemon 在 hello 帧自报的**能力 token 集**——monitor 按声明发流模式 flag（F66/#58③，
  取代旧「build_id 精确匹配」门控）。**加新能力 token = 同时加 `split_stream_flags` 剥离分支**（`every_capability_token_is_strippable` 测试强制，防 §26 死循环）。

## Wire protocol
每行一个 UTF-8 JSON 对象、`\n` 结尾、对象内无裸 `\n`/`\r`。`Frame` 类型见 `src/wire.rs`，外部 `kind` tag（snake_case），共 **9 个**：
`hello`（首帧握手，带 `v`/`build_id`/`host_arch`/`claude_dir`/`capabilities`）、`line`（tail 到的一行原始 jsonl）、
`session_added`（新会话文件出现）、`session_status`（红绿灯状态变化，F27）、
`session_removed`（会话消失；**S0 起带 `cause`**：`gone` = 真没了 / `superseded` = 同一 pidfile 原地换 sid，即 `/branch`、`/clear`）、
`turn_end`（一轮对话结束）、`tmux_session_closed`（P5：某个 tmux 会话关闭的正向死亡帧，只带 `name`）、
`tmux_sessions`（B2：`tmux ls` 原始 stdout，P1 起可带 `classification`）、
`overflow`（拥塞丢帧哨兵，#32）。字段细节以 `../doc/IPC-PROTOCOL.md` §10 为准。

> **2026-07-31 Phase G 更正**：这里此前写「共 6 个」，漏了 `turn_end` / `tmux_session_closed` /
> `tmux_sessions` 三个，`session_removed` 也没跟上 S0 加的 `cause`；而下游那句
> 「字段细节以 IPC-PROTOCOL.md 为准」指向的那份**同样漏了这三个**。两处已一并补齐。

一次性历史查询（带参数 exec，干完即退、不进流式协议）：`--list-projects` / `--list-sessions <dir>` /
`--read-session[-tail] <path>` / `--search` / `--usage` / `--resolve` /
`--list-accounts` / `--session-accounts` / `--account-trust <configDir> <cwd>`（A2 多账号，全只读）。
