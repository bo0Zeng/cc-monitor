# F-remote-pull-identity(#41 + #72)

> 类1 第 1 个 feature。用户真机命中的"远端拉/接身份"族。两 bug 同根:cc-monitor 靠 `@ccm_sid` 认会话,而(a)标记传播有延迟且 ↗ 扫描重试不足(#41),(b)cc-monitor 自建 resume 会话根本没设 `@ccm_sid`(#72)。

## DoD / 验收
- **#72**:cc-monitor 的 Resume(tmux) / 新建 tmux 会话的 create 序列里,`tmux new-session` 成功后 `tmux set-option -t <target> @ccm_sid <sid>`(session-scoped、完整 sid、key `@ccm_sid`)。之后自建 `cc-<sid8>` 会话带身份 → `findClaudeTmux` 精确命中 → attach/kill 菜单正常、不弹 cwd 回退警告。加单测(payload/命令串含 set-option、sid 完整、session-scoped)。
- **#41(残)**:`src-tauri/src/lib.rs:1504` verify-fail 重绑路 `try_bind()` → `try_bind_with_retry(&session_id, ON_DEMAND_BIND_ATTEMPTS, ON_DEMAND_BIND_STEP_MS)`(镜像兄弟 cache-miss 路 :1489)。+ 前端 ↗ 首次扫不到自动短轮询(几秒、退避),让第一次点就成、不弹"未绑定窗口"。
- **不做**:重试窗口的真机最终标定(留用户真机确认,记为 carry-forward);/branch 漂移追踪(属 #43 族,defer);Codex resume 的 @ccm_sid(远端 Codex 未上)。
- **验证**:cargo test(monitor)+ vitest(前端 remote-launch/session-backend/tabs)+ clippy 不新增;关键路径:构造 resume-tmux 命令串断言含 `set-option … @ccm_sid <full-sid>`。

## 与主计划对接 / 共享面
- `src/session-backend.ts` `createRunAttach` + `src/remote-launch.ts` `buildResumeTmuxCmd`/`buildLauncherCmd`:#72 加 set-option。需把 `sid` 传进 createRunAttach(现只有 target/quotedCwd/quotedPayload)。**这是账本项**:tmux create 序列的最终形态 = new-session → set-option @ccm_sid → send-keys → attach。
- `src-tauri/src/lib.rs:1504` + `src-tauri/src/bind.rs`(#41 重试常量)。
- 前端 ↗:`src/tabs.ts` `bringRemoteTerminalToFront`(~2346-2360)+ ↗ handler(~1985-1996)加轮询。
- **§3 契约**:`@ccm_sid` set 规格已与 aterm 字节对齐(session-scoped/-t/full sid/key `@ccm_sid`/create 分支)。cc-monitor 落自己那半。

## 步骤(Phase C 逐条)
1. `session-backend.ts`:`createRunAttach` 签名加 `sid`(或 `ccmSid?`);tmux 序列 `new-session -d … && tmux set-option -t <target> @ccm_sid <sid> && send-keys …; attach`。裸拼 sid 前校验 `[A-Za-z0-9_-]`(与读取侧 charset 一致,防注入)。
2. `remote-launch.ts`:`buildResumeTmuxCmd`/`buildLauncherCmd` 把已知 sid 传进 createRunAttach(buildLauncher 新会话无 sid → 不设,或用其将得的 sid;新会话 sid 未知则跳过,靠 wrapper——记边界)。
3. 单测:`remote-launch.test.ts` 断言 resume-tmux 串含 `set-option -t cc-<sid8> @ccm_sid <full-sid>`、session-scoped、完整 sid。
4. `lib.rs:1504`:改 `try_bind_with_retry`。
5. 前端 ↗:`bringRemoteTerminalToFront` 首次失败 → 短轮询(如 5×600ms 或复用后端重试),让首点成功;失败文案保留。
6. gate:cargo build+test+clippy(monitor)、vitest、tsc。

## 审计(Phase D)——2 视角并行,均**无阻塞/无重要**
- **正确性+安全**:注入安全(sid 过 `isValidSessionId` 才裸拼、无其它 createRunAttach 调用者)、`&&` 短路正确(set-option 不碰已存在会话)、@ccm_sid 读写全等 round-trip(完整 sid/key 一致)、#41 重试/超时耦合成立(4s<8s、spawn_blocking 不持锁)。无 panic。
- **计划符合+回归**:§3 契约字节精确、DoD 全覆盖(前端轮询走"复用后端重试"的计划内允许替代)、零回归(buildLauncher/直连 resume 不变、测试断新行为未削弱)、无 scope creep、createRunAttach 加 `ccmSid?` 向后兼容。
- **建议处置**:建议-1(set-option 在 `&&` 链里、失败会阻断 resume)→ **已修**:包成 `(… 2>/dev/null || true) &&`,身份标记不再能阻断 resume(古董 tmux 降级到无标记、resume 照跑)。建议-2(4s stall,carry-forward 真机调)/建议-3(病态双重试>8s,实际不可能、既有 race 型)→ 记录、不动。

## 工程审计(Phase E)——主线程对账
- 主计划自洽:唯一共享面 `session-backend.ts::createRunAttach` 为**加法式可选参数**,不破坏边界、无新耦合。
- §3 契约:`|| true` 非阻断是 cc-monitor **内部健壮性**,不改 @ccm_sid 的 value/key/scope(读侧契约不变)→ 已 FYI aterm(建议同做,非阻塞)。
- 无新技术债;下一 feature(F-render)不碰本文件。build/test 全绿。

## carry-forward(真机,用户)
- `ON_DEMAND_BIND_*` 窗口(现 4s)的最终标定 + ↗ 真机端到端(首点即成、无"未绑定"弹窗)。
- #72 真机验:cc-monitor 自建 resume 会话 attach 不再弹 cwd 回退警告(需重装 app)。

## 签收:代码审计[x] 工程审计[x] 主计划已更新[x]
