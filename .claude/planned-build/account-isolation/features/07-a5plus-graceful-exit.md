# A5+ — 换号重启的「优雅退出」（DESIGN §5 ④ / §5.2 ④ 的 V3）

A5 换号重启的 ④ 结束旧进程当前是**直接 `kill_remote_tmux`**（SIGKILL 整会话）。
本功能把它升级为 DESIGN §5 ④ 的完整形态：**先请求 CC 优雅退出（让它把 jsonl/锁 flush 干净）→ 有界等 →
超时或失败才降级 kill**。kill 仍是最终必经门（§5.2 ④「kill 失败＝中止」不变）。

## 已核实的机制（Phase B）
- **退出键序**（claude-code-guide 查文档）：`/exit`+Enter＝文档化的干净退出；活动回合先按 **Esc** 打断；
  会话 jsonl **持续落盘**，干净退出很快。→ 序列 = **Esc（打断）→ `/exit`+Enter（退出）**。
- **tmux 会话不会随 CC 退出而消失**：会话跑的是**交互 shell**，send-keys 把 resume 载荷键进去；CC 退出后
  shell 回到提示符、会话仍在（`remote-launch.ts:110` new-session -d + send-keys 范式）。⇒ 退出检测**不能**靠
  会话消失，要靠 `pane_current_command` 不再是 claude（`isClaudeTmuxCommand` false / `findClaudeTmux` 不再命中）。
- **`kill_remote_tmux` 仍必须跑**（即便优雅退出成功）：否则残留的 shell 占着会话名 → resume 的
  `new-session -d ... 2>/dev/null && send-keys` 会 new-session 失败短路 → 只 attach 到没有 claude 的旧 shell。
  ⇒ 优雅退出 = kill **之前**的 best-effort flush；kill 是清场 + 兜底 SIGKILL。
- **`tmux_send_keys` 现恒附 `Enter`**：Esc 不能带尾 Enter（防误提交队列文本）→ 需把尾 Enter 变可选。

## DoD（可勾选）
- [ ] **Rust**：抽 `build_send_keys_remote_cmd(target, keys, enter) -> String`（把现内联 format! 提纯，顺带补 R1
  「命令构造测缺」）；`tmux_send_keys` 加 `enter: bool`（serde `#[serde(default = ...)]` **默认 true** →
  前端旧调用 `{origin,target,keys}` 逐字节等价、`/compact` 不受影响）；`enter=false` 时命令省去尾 ` Enter`。
  单测：enter=true/false 两分支命令串 + cc-* 白名单不变。**走一次性 ssh，不经 daemon**（无 daemon 重编）。
- [ ] **`account-restart.ts` 步 ④ 改造**（删 TODO(A5 计划内裁剪)）：
  - 新增 `DEFAULT_EXIT_WAIT_MS = 10_000`（§5 ④「等 M 秒，默认 10s」）。
  - 新增可注入 `awaitExit?: () => Promise<boolean>`（true=检测到退出 / false=超时；省略→有界延时兜底）。
  - ④a：`tmux_send_keys(...,"Escape",enter:false)` → 短延时 → `tmux_send_keys(...,"/exit",enter:true)`；
    send-keys 失败**不中止**（落到 ④c kill 兜底），toast 说明。
  - ④b：`await (awaitExit ?? 有界延时)`；超时 → toast「优雅退出超时，改为强制结束」（§5.2 ④ 降级 kill）。
  - ④c：`kill_remote_tmux`（**清场 + 兜底 SIGKILL**）；失败→**中止不续⑤**（§5.2 ④，语义不变）。
  - confirm 文案加一句「会先请求会话优雅退出，最多等 ~10s，再强制结束」。
- [ ] **`tabs.ts` 注入真 awaitExit**：`restartTabWithAccount` 传 `awaitExit: this.awaitExitFor(origin, cwd, sid)`——
  每 ~1s 轮询 `list_remote_tmux`、`findClaudeTmux(sessions,sid,cwd)` 不再精确命中即 resolve(true)，10s 超时 resolve(false)。
  轮询解析复用既有 `findClaudeTmux`（不新增判据）。
- [ ] **失败语义全程照 §5.2**：优雅退出（send-keys/超时）**不阻断**、kill 失败**中止**、resume 失败走既有剪贴板回退。
- [ ] **全量验证**：tsc 0 / npm test 全绿（扩 account-restart 编排测：优雅退出成功/超时/send-keys失败均续 kill；kill 失败仍中止）/ cargo test --lib 绿（+build_send_keys 测）/ build ✓。真机零改动。
- **不做**：给用户加「是否优雅退出」开关（优雅退出是默认且透明的改进，UI 不变，仍两条菜单项）；Ctrl+D/双 Ctrl-C 备选序列（`/exit` 已文档化足够，Esc 打断兜底）。

## 对接主计划 / 共享面
- 复用：A5 的 `tmux_send_keys`（扩 enter 形参）、`kill_remote_tmux`、`findClaudeTmux`、`restartWithAccount` 编排、awaitCompact 的注入范式。
- 改：`tmux.rs`（提纯 + enter 形参 + 测）、`account-restart.ts`（④ 三段）、`tabs.ts`（awaitExitFor + 注入）、`account-restart.vitest.ts`（扩测）。
- **账本**：`tmux_send_keys` 签名加 `enter?`（默认 true 向后兼容）→ INVARIANTS 的 tmux 名跨语言契约条目补一句「enter 可选、默认附回车」。

## 逐条实现步骤（Phase C）
1. Rust 提纯 `build_send_keys_remote_cmd` + `enter` 形参 + 双分支/白名单测 → cargo 绿。
2. `account-restart.ts` ④ 三段（Esc→/exit→awaitExit→kill）+ DEFAULT_EXIT_WAIT_MS + confirm 文案 → 扩 vitest。
3. `tabs.ts` `awaitExitFor` 轮询 + 注入 restartTabWithAccount。
4. 全量验证 + 真机零改动核查。

## 测试策略
- Rust：`build_send_keys_remote_cmd` enter=true/false 命令串精确断言；`is_ccm_tmux_name` 白名单不变。
- 前端：编排器纯逻辑——优雅退出成功(awaitExit→true)续 kill+resume / 超时(false)toast 后仍续 / send-keys 抛错不中止仍续 kill / kill 失败中止不续（回归 §5.2 ④）；`awaitExitFor` 轮询判据纯函数化后单测（命中→未命中翻转）。

## 审计结果（D，2026-07-24，两视角并行 + 主线程 E）
- **正确性/安全**：**零阻塞零重要**。独立重跑 tsc0/vitest/cargo 亲验为真（无谎报复现）。逐条追：send-keys 抛错→不中止落 kill 兜底；awaitExit true/false/never→都续 kill；kill 抛错→`return` 中止不 resume 不记账（防双进程）。确认 kill 必跑保证旧进程死（kill-session SIGKILL 整会话），resume 仅在 kill resolve 后可达——**无 resume-while-alive 窗口**。核实交互 shell 会话在 `/exit` 后仍存活 → kill 仍成功（无「优雅成功→kill 失败→误中止」）。`enter` 向后兼容亲验（`/compact` 不传 enter→None→true→附回车，字节等价 A5）。`is_ccm_tmux_name` 白名单不变、send-keys 仅 3 处调用。
- **计划符合/架构**：**零阻塞**。features/07 DoD 逐条磁盘核到真身（TODO(A5 计划内裁剪) 已删净）。account-restart.ts 仍纯编排（不 import UI）；`awaitExitFor` 与 `awaitCompactFor` 同形（工厂/超时/清理注入）；`claudeExited` 复用 `findClaudeTmux`、判据与重启守卫一致、单测覆盖。§5.2 顺序合规。**零新增 daemon 命令**（仅扩 `tmux_send_keys` `enter?`，feature 明授权）→ 无 daemon 重编。
- **已采纳的建议（hardening）**：S1 `awaitExitFor` 超时清挂起轮询 timer（追踪 pollTimer + stop 清理）——已修，干净收尾。
- **未采纳（记账）**：defensive `Promise.race`（真实注入恒 resolve，低价值）；「用户手造 cc-* 且 claude 为会话 PID-1」的边角（cc-monitor 自身 resume 流不可达，且降级安全=中止不双进程）。

## 签收
- [x] 过代码审计(D，两视角，零阻塞，S1 hardening 已修) · [x] 过工程审计(E，主线程对账：架构清晰、无新耦合/技术债、daemon 只读守住) · [x] 主计划已更新(F：INVARIANTS **§1** 补 enter?（原文误写 §37） / DESIGN §5④+§1 V3 标已解) · [x] 测试绿（453 vitest + 352 cargo + build）
