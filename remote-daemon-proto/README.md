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

## 内部分层（U2 起，2026-08-01）

`unified-backend` 工作区把 cc-monitor 拆成 **frontend（UI + 开窗）** 与 **backend（读 + 控制）**，
本 crate 就是 backend。§1.1 的三条解耦线，**第一条（平台线）U2 起有了目录，但尚未收口**：

```
src/
├── platform/     平台原语与平台 cfg 的归属地（**两层的生产段现在零平台原语**；main.rs 仍有 3 处，见下）
│   ├── proc.rs       /proc 与进程身份：pid_alive · proc_starttime · parse_starttime_from_stat
│   │                 · parse_btime · USER_HZ · start_epoch_from_ticks · proc_cmdline
│   │                 · proc_claude_config_dir · session_alive + is_same_live_process（纯判定）
│   ├── paths.rs      path_key（NTFS 大小写折叠 —— 路径语义，不是 /proc）
│   ├── signal.rs     send_sigusr1（U3 从 tmux_hook 下沉；身份校验刻意留在调用方——那是域判断）
│   └── pidwatch.rs   pidfd_open + watch_pid_until_exit（零轮询，阻塞在无超时 poll(2)）
├── observe/      ★ 读，不改变世界
│   ├── watcher.rs · history_query · search_query · usage_query · accounts_query
│   ├── turn_detect · codex          两个纯解析核
│   └── fs.rs        mtime_ms（U3 从 common/ 搬回——两个调用点同属 observe，「≥2 层」不成立）
├── control/      ★ 会改变世界，或产出「怎么改变世界」的计划
│   ├── fork_write.rs    写盘（O_EXCL 新建）—— **唯一**写盘白名单，红线 I7 的那个洞口
│   ├── tmux_hook.rs     改 tmux server 状态 + 发 SIGUSR1
│   └── resolve_query.rs 产 CommandPlan（名字里有 query 但它是**计划面**，账本 S14）
├── common/       两层都要、**平台无关**、无域知识（门槛写在 common/mod.rs）
│   ├── paths.rs      projects_root（原有 5 处）
│   └── fs.rs         read_regular_capped（U3 从 accounts_query 搬来，**反向边因此消失**）
└── 顶层           main（组装根）· wire（协议类型）· 四条 guard + layering_guard
```

### 两层之间只有一个方向，而且**条数被钉住**

`observe → control` 允许，**反向一条都不许**（§1.1-2），由 `layering_guard.rs` 机检。
今天正向**恰好一个符号**：`watcher` 调 `control::tmux_hook::install_hooks`。

那不是设计失误 —— tmux hook 活在 **server 进程的内存里**，server 每次重起都要重装，
而「server 起来了」这个事实**只有 observe 知道**（socket 目录 inotify）。硬要反过来只能靠轮询，
与 §41 零定时器铁律正面冲突。**「有一个正当例外」与「这条线随便穿」是两回事，
中间隔着的就是那个计数** —— 多一个就红，逼下一个人把他的理由也写出来。

> U3 摸底时真有过一条**反向边**（`fork_write` → `accounts_query::read_regular_capped`）。
> **没有给它开例外**：那个函数根本不是 observe 的域逻辑，是通用安全读文件，
> 搬进 `common/fs.rs` 之后边自然消失。铁律 6：改结构让问题不存在。

### 「唯一允许平台 cfg 的层」是**目标**，不是现状

Phase D 审计逐条查过，**生产段还有 3 处平台原语在 `platform/` 之外**（U2 时是 4 处，U3 收掉 `tmux_hook` 那处），如实列在这里 —— 三处全在 `main.rs`，它是**组装根**，平台分支留在这里可辩护：

| 位置 | 是什么 | 处置 |
|---|---|---|
| ~~`tmux_hook.rs` 的 `libc::kill`~~ | ~~`#[cfg(unix)]` + 发 SIGUSR1~~ | **U3 已收**进 `platform/signal.rs` |
| `main.rs:345-365` | SIGUSR1 处理器的 `#[cfg(unix)]` / `#[cfg(not(unix))]` | `main.rs` 是**组装根**，平台分支留在这里可辩护。但不能因此说「唯一」 |
| `main.rs:421-433` | `#[cfg(windows)]` USERPROFILE 回退 | 同上 |
| `main.rs:439-467` | `shutdown_signal` 的一对 cfg | 同上 |

（U2 已收的两处曾经也在这张表上：`accounts_query.rs` 的 `proc_claude_config_dir`
读 `/proc/<pid>/environ`，和 `watcher.rs` 里内联的第五处 `join("projects")`。）

### 判据不是「cfg 出现在哪」

计划自审打掉过这条：本 crate 在 Windows 上编不过的 12 个错里，头号的 `pidfd_open`
**根本没有 cfg** —— 它是无条件编译的 Linux-only 代码，cfg 位置扫描抓不到。
真判据只有 `cargo check --all-targets --target x86_64-pc-windows-msvc`，
**今天实测仍是 RC=101 / 12 错**（11 个已集中到 `platform/pidwatch.rs`，1 个是
`watcher.rs` 测试段的 `libc::getuid`）。清零并进 CI 是 **U4** 的 DoD。

⇒ **平台线在 U4 通过那条编译判据之前，都不算落地。** U2 做的是把 11/12 个错**集中到一个文件**，
让 U4 有一个明确的下手点 —— 这是真进展，但不是收口。

**已登记、U2 刻意不修的**：`platform::proc::pid_alive` 的非 Linux 分支恒返回 `true`
（静默错误地雷）。改它 = 决定「Windows 上进程是否存活怎么答」，是 U4 的正题；
在一个声明「行为逐字不变」的纯重构里夹带语义决策是错的。

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
