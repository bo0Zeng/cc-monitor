# F02 — kill_remote_tmux 白名单（修 I1）

> full-audit 重要 I1：`kill_remote_tmux` 缺 `is_ccm_tmux_name` 白名单（`tmux_send_keys` 有），
> F79 cwd 回退可拿到用户自己 `tmux new -s work` 的非 cc- 名 → `kill-session -t work` 端掉整个会话所有 window。

## DoD
- [x] `kill_remote_tmux` 首行加 `is_ccm_tmux_name` 守卫（与 send-keys 对称，放在 config load 之前）。
- [x] 非 cc-* 名返回清晰的教育性错误（说明为何拒 + 让用户到该 tmux 自行处理）。
- [x] Rust 回归测：非 cc-* 名（work/web/0/my-session/cc-a b）被早退拒绝（含"拒绝"）；变异验证（去 guard → 红）。
- [x] 订正 `account-restart.ts:21` 的假注释（原称"后端白名单只认 cc-*"对 kill 曾是假的）。

## 不做（防蔓延）
- 不改前端菜单是否**呈现**"杀死会话"项——后端安全拒绝 + 教育性提示即可（cc-* 名照常放行，含 cwd 回退命中的自建会话）。前端"非 cc- 不呈现 kill"的可选优化不在本功能。
- 不改 kill-session 为 kill-window（那是另一种设计，越界）。

## 与主计划对接
- 共享面「`src-tauri/src/tmux.rs`」→ 落 kill 守卫，复用既有 `is_ccm_tmux_name`（与 F05 清理孤儿同校验）。账本最终形态：kill 与 send-keys 对称。

## 实现步骤
1. ✅ `tmux.rs` `kill_remote_tmux` 首行 `if !is_ccm_tmux_name(&target) { return Err(...) }`。
2. ✅ Rust 测 `kill_remote_tmux_rejects_non_ccm_name`（tokio::test，guard 早退不触 SSH）。
3. ✅ 订正 account-restart.ts:21 注释。

## 测试策略
- Rust 单元（tokio::test）：非 cc-* 名早退拒绝。cc-* 名不在此拦（会到 SSH，不在此验）→ 由既有 `ccm_tmux_name_whitelist` 覆盖谓词。
- 回归纪律：变异（移除 guard）→ 非 cc- 名落到 config-not-found 错、不含"拒绝" → 测红。已验证。

## 审计结果
- **代码审计(D)**（低风险主线程自审）：guard 与 send-keys 逐字对称、放在 config load 前（早退不触 SSH）；错误信息教育性；cc-* 放行不影响自建会话（含 cwd 回退命中的 cc-<sid8>）；无 daemon/双写点/bashrc 触碰。
- **工程审计(E)**：复用既有谓词、无新耦合；与 F05 清理孤儿同校验（账本一致）；cargo fmt 0 + 353 Rust 测绿 + tsc 0 + npm test 570 绿。

## 签收
- [x] 过代码审计（D）
- [x] 过工程审计（E）
- [x] 主计划已更新（rev 04）
- [x] F02 完成
