# 壳（`src/frontend/shell/`）

monitor 的 Rust 半：Tauri 2 应用，crate `monitor`（lib 名 `monitor_lib`）。它只做宿主（窗口 · 起子进程 · 开终端 · 放字节 · 本机后端起停）与通信层的客户端；读会话、判定、写用户文件都在后端（[ARCHITECTURE §2.4](../../doc/ARCHITECTURE.md)）。

相关清单：界面（TS）[`src/README.md`](../../README.md) · 后端 [`src/backend/README.md`](../../backend/README.md) · 文件窗口本体 `src/frontend/filewin/`。

下表的文件名都相对 `src/frontend/shell/src/`。每份 `.rs` 顶上的 `//!` 是那一份的说明；这里只按用途分组指路，不抄函数表。

## 按用途分组

| 用途 | 文件 |
|---|---|
| 装配 | `lib.rs`（`run()`：插件 · State · Tauri 命令注册 · 工作线程）· `main.rs` |
| 本机后端 | `local_backend_host.rs`（起 / 停 / 状态；常驻那一种经本人 Unix 套接字 `<家>/run/backend.sock` 接上）· `local_backend.rs`（stdio 监护那一种）· `backend_control.rs` · `backend_policy.rs` |
| 远端 | `remote_resident.rs`（远端常驻后端：经本机后端那条 SSH 跑 `--resident-attach` 中继）· `dial_host.rs`（一台远端的配置 → 拨号请求）· `link_mux.rs`（经本机后端开链路）· `sftp.rs`（自部署那一半）· `sftp_pool.rs`（传输台中继）· `byte_table.rs`（全仓唯一的取字节口） |
| 会话流 | `stream_source/`（远端与本机两条流进同一个收口 `batch.rs` 的 `LineIntake`）· `local_lines.rs` · `event_replay.rs`（重放缓冲 · 订阅 · 就绪点）· `session_book.rs`（那台说过的会话成品，纯缓存）· `session_tap.rs` · `snapshot_resume.rs` · `frame_query.rs` · `frame_tally.rs` · `inbound_client.rs` |
| 通道 | `chan/`（`webview.rs`：主界面的 `chan_call` / `chan_subscribe`；`host.rs`：文件窗口那对父子管道）；路由器与线上词汇住 `src/comms/inward/` |
| 终端与窗口 | `launch.rs`（开终端窗口）· `bind.rs`（↗ 认窗口：点那一刻沿进程链问每一级显示在哪个窗口）· `terminal_screen_relay.rs` · `spawn_managed.rs`（起子进程的唯一出口） |
| 文件窗口 | `filewin/`（开窗那一侧；窗口本体是独立包 `src/frontend/filewin/`） |
| monitor 自己的事 | `config.rs`（`config.json` 唯一写口 `patch_config_at`）· `logging.rs` · `ui_error.rs`（要让用户知道的出错只此一种事件）· `ui_contract.rs`（事件名与载荷）· `data_paths.rs` · `diagnostics_report.rs` · `auto_launch.rs` · `app_restart.rs` · `clipboard.rs` · `desktop_notify.rs` · `footprint_client.rs` · `machine_state.rs` · `probe_relay.rs` · `asset_sync.rs` · `creds_store.rs` · `copy_table.rs` · `detail.rs` · `utils.rs` |
| 本机 `ccm` 与集成 | `ccm_probe.rs` · `ccm_cli_contract.rs` · `profile_installer.rs`（`~/.cc-monitor/bin` 在不在用户级 PATH 上）· `cc_bus.rs` · `cc_bus_deploy.rs` |
| 平台原语 | `platform/`（壳里平台 cfg 的唯一住址：文件原语 · 控制台 · 窗口 · 进程 · 通知 · 单实例 …） |
| 判据宿主 | 名字带 `_registry` / `_guard` / `_ledger` 的，以及 `structural_scan.rs` · `doc_claim_registry.rs` · `guard_support.rs`：生产树里只挂 `#[cfg(test)]` 模块，判据本体住 `tests/frontend/shell/` |

## 要加什么去哪

| 要加 | 去哪 |
|---|---|
| Tauri 命令 | 写在归属的模块里，`lib.rs` 的 `invoke_handler!` 注册，并登记进 `command_home_registry`（碰后端的进不了这张表：那一类做成后端帧命令）；界面只经 `src/frontend/ui/ipc/commands.ts` 调 |
| 事件 | `ui_contract.rs` 的 `events` 加常量与载荷 |
| 起子进程 | 只经 `spawn_managed.rs` |
| 平台相关代码 | 只在 `platform/` |
| jsonl 记录类型 · 帧命令 · 设置项 | [CONTRIBUTING.md](../../doc/CONTRIBUTING.md) |
| 打包配置 | `tauri.conf.json` · [BUILDING.md](../../doc/BUILDING.md) |

不变量全集在 [INVARIANTS.md](../../doc/INVARIANTS.md)。
