# B2：tmux 对账改 daemon 推送（消除每 8s SSH 刷屏）

> 用户 2026-07-20 报 bug：远端 tmux 对账 poller 每 8s 新建 SSH（`connect_and_exec_cmd`）跑 `tmux ls` 再关 → 远端 sshd 日志刷屏。选 **B2**：daemon 在远端**本地**跑 `tmux ls`、经流上报 → monitor 零额外 SSH。

## 根因
`tmux_reconcile.rs::run_tmux_reconcile_poller`（`POLL_INTERVAL=8s`）每轮逐 origin 调 `tmux::list_remote_tmux` → `connect_and_exec_cmd`（**每次新 SSH 登录+登出**）。它是「tmux 被杀→灰 sid」的备份对账；主力判活是 daemon pidfile（走常驻流、不新建连接、不刷屏）。

## 设计（daemon 推送、monitor 消费）
1. **daemon**（`watcher.rs` watch_loop + `wire.rs`）：
   - watch_loop 的 2s tick 里按 `TMUX_EMIT_INTERVAL`（~8s）节流，**本地** `std::process::Command` 跑 `sh -c "if command -v tmux …; then tmux ls -F '<FMT>' 2>/dev/null || true; else printf 'NO_TMUX\n'; fi"`（**与 monitor `list_remote_tmux` 同命令/同 `TMUX_LS_FMT`**）。
   - 发 `Frame::TmuxSessions{ raw: String }`——**送 tmux ls stdout 原文**（含 `NO_TMUX`），照 Line 帧「送 raw、monitor 解析」哲学。**不在 daemon 复刻 parse_tmux_ls**（零解析重复/parity 风险）。
   - `EMITS` 加 `"tmux_sessions"`。**BUILD_ID bump**（行为变）。**不 bump PROTO_VERSION**（additive）。daemon 首次用 process::Command——注意 PATH（用 `sh -c` + `command -v` 门控，同 monitor）。
2. **wire**（`wire.rs` Frame + monitor `ssh_source.rs` InboundFrame）：
   - `Frame::TmuxSessions{ raw: String }`（snake_case、additive）。
   - monitor `InboundFrame::TmuxSessions{raw}` + `parse_frame` 分发。
3. **monitor**（`ssh_source.rs` + `tmux_reconcile.rs`）：
   - stream_loop 收 TmuxSessions → 存**每 origin 最新 tmux 状态**到共享 map（新 `snapshot_tmux_by_origin` 或塞进现有 registry）；用 monitor 现有 `parse_tmux_ls(raw)` 解析（`NO_TMUX`→None）。
   - `run_tmux_reconcile_poller` 改**读该共享 map**（帧喂）而非 `list_remote_tmux` SSH → `reconcile_step` 逻辑不变。**删对账路的 SSH 轮询**（刷屏源）。
   - **on-demand `list_remote_tmux` 保留**（F51 attach 右键反查、用户触发、不刷屏）——只去掉**周期对账**的 SSH。

## 边界 / caveat
- **daemonless 远端**（无 daemon→无帧）：帧驱动对账不适用；保留其现有行为（daemonless 自己的 2s SSH poll 判活，或对账降级）。B2 治的是 daemon 模式（常见）。
- daemon 无 tmux / 无 server → 送 `NO_TMUX` / 空 → monitor `parse_tmux_ls` 得 None/空 → 对账**空 backend 保守跳过**（同现逻辑，防整服务抖动误灰）。
- 灰延迟 ≈ `TMUX_EMIT_INTERVAL × RETIRE_MISS_THRESHOLD`（daemon 端节流）；主力灰仍 daemon pidfile（快）。

## aterm 协调（共享 wire）
- `Frame::TmuxSessions` 是**新帧、additive、monitor 专属**（aterm DaemonTransport 未知 kind 跳过、不崩）。**知会 aterm**（契约卫生 + 确认帧名不撞它未来帧），非阻塞。

## 步骤
DG-tmux-1 wire（Frame + InboundFrame + parse_frame + 序列化测）→ DG-tmux-2 daemon 发射（本地 tmux ls + 节流 + emit + EMITS + BUILD_ID）→ DG-tmux-3 monitor 消费（共享 map + reconcile 改读 map + 删 SSH 轮询）→ Phase D 审计 → 发版（re-embed daemon + 版本 + CHANGELOG）。

## 零回归红线
- Claude 帧字节不变（新帧 additive）；不 bump PROTO_VERSION；on-demand list_remote_tmux/attach 不动；daemonless 行为不变。
- gate：daemon cargo+clippy --all-targets、monitor cargo+clippy+前端、Phase D。

## Phase D 审计（2026-07-22，2 视角并行）
- **视角1 回归+daemonless**：无阻塞。flood 确认消除（对账体零 SSH）；Claude 零回归（PROTO_VERSION 仍 1、仅 bump BUILD_ID→p1p、`reconcile_step` 逻辑+9 单测逐字不变）；on-demand attach 保留；重连清理正确（键两侧同 `origin_label`、`TMUX_LS_FMT` 两侧逐字一致）。**重要**：daemonless 失去 tmux 加速灰 → 退回 30min mtime 兜底（**判可接受**，建议补文档+给 run_tmux_ls 加超时）。
- **视角2 正确性+安全**：无阻塞。无注入（脚本编译期固定、无用户输入）、错误→空串下游保守跳过、节流正确、注册表键一致无泄漏、retire 守卫全保留、字节 roundtrip 一致、新路径无危险 unwrap/index。**重要①**：`run_tmux_ls()` 无超时→阻塞会冻结整个 watcher（reader/notify/判活全停）。**重要②**：无能力门控→旧/无 daemon 时对账静默失效。**建议**：`TMUX_LS_FMT` 双写点无相等测试（漂移隐患）。

## 审计后修复（本次落地）
- **[重要①→已修]** `watcher.rs`：`run_tmux_ls` 挪到**一次性后台线程**跑，watch_loop 只非阻塞 `try_recv`；gate 在「无在途探测」上 → 最多一个探测线程；tmux 卡死只泄漏该线程、reader 永不冻结。函数 doc 记「无超时→只能 off-thread」。
- **[重要②→文档化]** `tmux_reconcile.rs` poller doc 记「B2 能力前提」caveat：daemonless / 陈旧旧 daemon 无 `TmuxSessions` 帧 → 该 origin 静默退化（非 panic/误灰）。可接受依据：daemonless 有 30min mtime 兜底；陈旧 daemon 已由 `StaleBuild` 版本协商警告提示升级；正常路径 daemon 连上自动部署到 p1p 恒发帧。**未加 tmux 专属 warn**（正确 warn 需把 daemon `emits` 解进 Hello + 每-origin 能力登记；启发式会在 daemonless/首连误报）——留作可选后续。
- **[建议→留后续]** `TMUX_LS_FMT` 双写点：现两侧逐字一致 + 注释约束；收敛/加跨 crate 相等断言留作后续（不在本次扩审计面）。

gate（修复后）：daemon fmt 干净 / clippy --all-targets 干净 / 109 测过；monitor tmux 17 测 + reconcile 10 测过、doc 改动零新增 clippy。
