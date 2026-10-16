# 后端（`src/backend/`，crate `cc-monitor-backend`）

每台机器上的 `~/.cc-monitor/bin/ccm` 就是那台的后端：读会话、起进程、动 tmux、拨 SSH、管资产、写用户的文件，全在这里；monitor 与外部前端只经帧面说话，用户敲的 `ccm` 走 CLI 面（[ARCHITECTURE §2.2](../doc/ARCHITECTURE.md)）。

- **模块地图**在 ARCHITECTURE §2.2（`platform/` · `observe/` · `control/` · `files/` · `accounts/` · `agents/` · `assets/` · `wire/` · `stream/` · `faces/` …）；每个目录的 `mod.rs` 头注是那一块的说明。这里不再抄一份。
- **协议**：帧、入方向命令、CLI 子命令、错误码逐格在 [IPC-COMMANDS.md](../doc/IPC-COMMANDS.md)（从命令注册表 `stream/inbound/registry/` 与帧类型生成，勿手改）；载体、信封、握手在 [IPC-PROTOCOL.md](../doc/IPC-PROTOCOL.md)。
- **拨号核心另是一个包**：`ssh-dial/`（crate `ssh-dial`，本目录这一棵 workspace 的成员）—— 竞速 · 跳板（可多跳）· host key 判定 · 鉴权（钥匙 · agent · 外部签名器 · 密码）· 保活 · 按身份复用 · 开 channel 之后的几种用法（含 PTY）· SFTP 客户端。手机嵌的 Rust 核心链同一份；平台的事从它的 `ssh-dial/env.rs` 那几个口进，本后端在 `dial/mod.rs::host` 装自己那一套。它住本目录底下，扫描型判据的人群照旧管到它。
- **不是壳的 workspace 成员**：它要能在目标机上原生构建，发版时交叉编成 x86_64 / aarch64 的 musl 静态二进制，内嵌进 monitor，第一次接一台远端时部署过去（计划由帧命令 `deploy-plan` 出）。本机那份随包带上（Windows 上是 monitor 监护的子进程）。手机端经 SSH 连同一份后端，自己不部署。远端只支持 Linux。
- **两个身份数**（都在 `lib.rs`）：`PROTO_VERSION` 只在破坏性线上变更时加；`BUILD_ID` 是这份二进制的身份，换不换后端按它判（加 / 删命令、改帧形、改冻结格时，合进主线那一刻经 `tests/scripts/bump-build-id.sh` 打）。这台接受哪些命令由 hello 的 `commands` 现报（从命令表 `stream/inbound/registry/` 来），不另有能力集。

## 构建与测试

```bash
cd src/backend
cargo test --workspace --lib     # 单测与登记类判据（后端 ＋ 拨号核心；判据本体住 tests/backend/，经 #[path] 挂进来）
cargo fmt --check -p cc-monitor-backend -p ssh-dial
cargo clippy --workspace --all-targets
cargo check --workspace --all-targets --target x86_64-pc-windows-msvc   # 平台线的判据：跨 target 编得过
bash ../../tests/scripts/bless-golden.sh   # 金样判据（协议参考 · 跨语言金样）红了、对过 .build/golden/ 下现打的那份之后拷回
```

测试一律在沙箱里跑：起 tmux 的测试显式 `-S` / `-L` 私有 socket 并摘掉 `$TMUX`，起真后端的测试经唯一的隔离口拿私有 tmux（INVARIANTS §48.3）。
