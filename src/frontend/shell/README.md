# 壳（`src/frontend/shell/`）

monitor 的 Rust 半：Tauri 2 应用，crate `monitor`（lib 名 `monitor_lib`）。它只做宿主（窗口 · 起子进程 · 开终端 · 放字节 · 本机后端起停）与通信层的客户端；读会话、判定、写用户文件都在后端（[ARCHITECTURE §2.4](../../doc/ARCHITECTURE.md)）。

相关清单：界面（TS）[`src/README.md`](../../README.md) · 后端 [`src/backend/README.md`](../../backend/README.md) · 文件窗口本体 `src/frontend/filewin/`。

下表的文件名都相对 `src/frontend/shell/src/`。每份 `.rs` 顶上的 `//!` 是那一份的说明；这里只按用途分组指路，不抄函数表。

## 按用途分组

| 用途 | 文件 |
|---|---|
| 装配 | `lib.rs`（`run()`：插件 · State · Tauri 命令注册 · 工作线程）· `main.rs` |
| 本机后端 | `local_backend_host.rs`（起 / 停 / 状态；常驻那一种经本人 Unix 套接字 `<家>/run/backend.sock` 接上）· `local_backend.rs`（stdio 监护那一种）· `backend_control.rs` · `backend_policy.rs` |
| 远端 | 各台的常驻流归本机后端（`control/feeds.rs`），壳这一侧只剩：`remote_resident.rs`（停一台：请本机后端 `machine-stop`）· `deploy.rs`（装那一趟在本机后端 `deploy-install`，这里交字节路径、收进度）· `transfer_relay.rs`（传输台中继）· `byte_table.rs`（全仓唯一的取字节口）· `machine_table.rs`（本机后端 `machines-read` 最近答的那一份机器表） |
| 会话流 | `stream_source/`（只读本机后端那一条，各台流上的事由它原样转来 `relayed`，进同一个收口 `batch.rs` 的 `LineIntake`）· `local_lines.rs` · `event_replay.rs`（重放缓冲 · 订阅 · 就绪点）· `stream_watch.rs`（每条长连接报「在看哪几个会话」）· `session_book.rs`（那台说过的会话成品，纯缓存；每台一本，折法是端核心那一份 `resident_client::book`）· `snapshot_resume.rs` · `frame_query.rs` · `frame_tally.rs` · `inbound_client.rs` |
| 通道 | `chan/`（`webview.rs`：主界面的 `chan_call` / `chan_subscribe`；`host.rs`：文件窗口那对父子管道）；路由器与线上词汇住 `src/comms/inward/` |
| 终端与窗口 | `launch.rs`（开终端窗口）· `bind.rs`（↗ 认窗口：点那一刻问本机后端 `session-window`，先按窗口标签、再沿进程链问每一级显示在哪个窗口）· `spawn_managed.rs`（起子进程的唯一出口） |
| 文件窗口 | `filewin/`（开窗那一侧；窗口本体是独立包 `src/frontend/filewin/`）· `files_batch_relay.rs`（后端推来的 `files_batch` 进度按票转给订了它的那扇窗口） |
| monitor 自己的事 | `config.rs`（`config.json` 唯一写口 `patch_config_at`）· `logging.rs` · `ui_error.rs`（要让用户知道的出错只此一种事件）· `ui_contract.rs`（事件名与载荷）· `data_paths.rs` · `diagnostics_report.rs` · `auto_launch.rs` · `app_restart.rs` · `clipboard.rs` · `footprint_client.rs` · `asset_sync.rs` · `detail.rs` · `utils.rs` |
| 一次性迁移（下一版整份删） | `bookmarks_migration.rs`（桌面旧书签迁到各台后端） |
| 本机 `ccm` | `backend_once.rs`（问自己放下去的那份后端一次） |
| 平台原语 | `platform/`（壳里平台 cfg 的唯一住址：文件原语 · 控制台 · 窗口 · 进程 · 通知 · 单实例 …） |
| 判据宿主 | 生产树里不放判据：判据本体住 `tests/frontend/shell/`，由 `lib.rs` 经 `#[path]` 挂成 `#[cfg(test)]` 模块（`judges_live_in_tests` 钉）。历史的事都在那台后端，壳里没有 |
| 截图台架 | `shots_shell.rs`：壳那几块真代码不带窗口跑起来给截图台架用，只在特性 `shots` 下编，发版构建里没有 |

## 要加什么去哪

| 要加 | 去哪 |
|---|---|
| Tauri 命令 | 只限 monitor 自己的事，步骤见 [CONTRIBUTING.md](../../doc/CONTRIBUTING.md) §2.2；碰后端的做成后端帧命令 |
| 事件 | `ui_contract.rs` 的 `events` 加常量与载荷 |
| 起子进程 | 只经 `spawn_managed.rs` |
| 平台相关代码 | 只在 `platform/` |
| jsonl 记录类型 · 帧命令 · 设置项 | [CONTRIBUTING.md](../../doc/CONTRIBUTING.md) |
| 打包配置 | `tauri.conf.json` · [BUILDING.md](../../doc/BUILDING.md) |

不变量全集在 [INVARIANTS.md](../../doc/INVARIANTS.md)。
