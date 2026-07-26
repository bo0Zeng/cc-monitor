# F-Vwin — 真 Windows 验证 #74/#41（↗ HWND 绑定）+ 灰灯渲染冒烟

> 发版前的真机冒烟。#74/#41 从没在真 Windows 跑过（aya=Linux，bind 全 `#[cfg(windows)]`，Linux 桩恒 false）。
> 分支 account-ux。红线：daemon 零改·不改 TMUX_LS_FMT·不碰 ~/.bashrc·不 push/发版/bump·不用 emoji。

## 关键 recon 结论（agent 摸底，file:line 已核）
- **#74/#41 = 远端 ↗ 拉前台**：ccm-wrapper 给终端打 OSC 标题 `ccm-rbind-<sid>`（`shared/ccm-wrapper.sh:17`，四跳传播 远端→ssh -t→tmux→本地 Win 终端）；`RemoteHwndCache.try_bind`（`bind.rs:642-661`）= `find_window_by_marker_substr`（`bind.rs:293-360`，`EnumWindows`+`GetWindowTextW` 子串匹配标题）；↗ handler `bring_remote_terminal_to_front`（`lib.rs:1523-1570`）→ `verify_binding`（`bind.rs:473-501`）→ `bind::activate`（`bind.rs:509-529`，`SetForegroundWindow`）。
- **★可不用 SSH/aya/claude 验**：`try_bind` 只扫窗口标题 → **开任意窗口、标题设 `ccm-rbind-<testsid>`，即可端到端驱动 find→verify→activate**（agent 明证）。省掉整条远端链。
- **可观测**：成功 bind 打 `remote bind: sid=… → hwnd bound`（`lib.rs:579-581`）；命令返回 `Ok` vs 确定的 `Err` 串。
- **native 测试空白**：`try_bind`/`activate`/`find_window_by_marker_substr` **无任何 native 测试**（`bind.rs:812-837` 只验 map 语义、显式绕过 Win32）——这正是"从没在 Windows 验过"的缺口。
- **`SetForegroundWindow 真的把窗口拉前** = 半自动**：前台锁可能返回 true 却没真聚焦（`bind.rs:522-525`/`lib.rs:1332-1339`）；`activate` 不记 SFW 结果 → 要 `GetForegroundWindow==hwnd` 探针或肉眼。
- **灰灯 on Windows = 平台无关**：`ssh_source/tmux/tmux_reconcile` 零 `cfg(windows)`（仅 3 处 SSH-agent 命名管道无关灰灯）；逻辑已被 F-E1 Linux 全覆盖 → **不需 Windows 逻辑验证，只需 WebView2 渲染看一眼灰点**（CSS `grid-monitor.ts:424-425`/`tabs.ts:117-127`）。

## DoD（可验证）
- [ ] **A（自动，最高价值）**：新增 `#[cfg(windows)]` native 测试于 `bind.rs`：测内自建一个标题含 `ccm-rbind-<testsid>` 的真 Win32 窗口（`CreateWindowExW`）→ 调 `find_window_by_marker_substr` 断言找到该 hwnd+owner_pid=本进程 → `RemoteHwndCache.try_bind` 断言绑定 → `verify_binding` 断言 true → 关窗后 `verify_binding` 断言 false（`IsWindow` 失效路径）→ 清理销毁窗口。**在 VM 上 `cargo test`（session 0 SSH 即可，find/verify 不需前台）跑绿。**
- [ ] **B（自动，附带）**：VM 上 `cargo test --manifest-path src-tauri/Cargo.toml` 整体在真 Windows 目标跑绿（首次证明整个 src-tauri 测试套件在 msvc target 通过，非只 Linux）。
- [ ] **C（半自动/肉眼，如实标注）**：session-1 里开一个 `ccm-rbind-<sid>` 标题窗口 + 跑一个探针（`GetForegroundWindow==hwnd` after `activate`）确认真拉前；或人工看一眼。**明确写清这步不保证 CI 级稳（前台锁），是 smoke 不是 gate。**
- [ ] **D（渲染看一眼）**：灰灯灰点在 WebView2 渲染正常（并入 F-E5 的 DOM 冒烟里顺带截一眼，或手动）。
- [ ] 不做：真 SSH+claude+终端标题四跳传播的端到端（realistic-only、贵、非验 fix 必需——A 的假窗口已覆盖 fix 本身）。

## 与主计划对接（共享面）
- 只加 `bind.rs` 的 `#[cfg(windows)]` 测试（测试码，非生产逻辑改动，行为等价）。不碰 daemon、不碰灰灯逻辑。
- runner：复用 VM 的 `ssh win11` + session-0 `cargo test`（A/B）；C 复用 session-1 hop（schtasks /it）跑探针。

## 步骤
1. 写 A 的 native 测试（bind.rs 内 `#[cfg(all(windows,test))]`，用 windows crate 建窗口 helper）。本地 aya 只能 `cargo check`（`--target x86_64-pc-windows-msvc` 若装了 target，否则跳过——测试逻辑靠 VM 真跑）。
2. scp 更新后的源到 VM（git bundle 增量 or scp bind.rs）→ VM `cargo test bind` 跑 A，读结果。
3. VM 全量 `cargo test`（B）。
4. C：session-1 探针（开假窗口→invoke/直接调 activate 路径→GetForegroundWindow 比对），如实记结果+标 flaky。
5. D：并入 F-E5 DOM 冒烟截灰点。
6. 回报：#74/#41 的 bind+verify 层真 Windows 验过（A/B 绿）；activate 拉前=smoke（C）；灰灯逻辑无需 Windows 验（Linux 已覆盖），渲染 OK（D）。

## 测试策略
A/B = `cargo test` 真 Windows（回盘读结果，别信内联）。C = 探针+如实标注。发版判断：A/B 绿 = #74/#41 修复在真 Windows 成立的最强自动证据；C 是人工确认 ↗ 真拉前。

## 审计结果
（待 D 阶段填）

## 签收
- [ ] 代码审计（D）
- [ ] 工程审计（E）
- [ ] 主计划已更新（F）
