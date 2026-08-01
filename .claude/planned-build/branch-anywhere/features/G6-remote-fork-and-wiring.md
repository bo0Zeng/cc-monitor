# G6 — 远端解禁 + 两个调用点接线（本区最后一个功能）

G2 让 daemon 会分叉，G3a/G3b 让「起会话」知道该带什么参数。本步把这两半接起来，
并把两处 `⑂` 的 origin 门打开 —— **远端会话也能分叉了**。

## §1 交付面

| 面 | 交付 |
|---|---|
| Rust | `ssh_source::connect_and_exec_capture`（**收全** stdout/stderr/退出码）· `remote_branch.rs`（新命令 `create_remote_branch_session`） |
| 前端 | `fork-ask.ts`（追问小窗）· `fork-flow.ts`（生产接线 + 源事实采集）· `tmux-sessions.ts`（判据从 `tabs.ts` 搬出，破环） |
| 接线 | `branch-button.ts` 认 origin；`tabs.ts` 两条渲染路径 + `views/session-viewer.ts` 三处门全开 |
| 契约 | INVARIANTS §1 加一段（daemon 在远端写的三条收窄）；平价对账表 `history.branch` **欠账还清** |

## §2 ★ 既有的远端查询路**看不见失败**

`run_list_query` 走 `Channel::into_stream()`，而 russh 0.61 的 `into_stream` **只搬
`ChannelMsg::Data`** —— `ExtendedData`（stderr）与 `ExitStatus` 都被丢掉。
所以那条路上「命令失败」与「查询成功但结果为空」**在类型上不可区分**。

列举类查询忍得了（空结果本来就合法）。分叉忍不了：daemon 恰恰把失败原因写在
**stderr + exit 2** 上，而「失败必须可见」是本步的硬要求。⇒ 新写一条 `connect_and_exec_capture`
直接驱动 `channel.wait()`，三样全收。

**两个坑，都写进了代码注释**：

1. **见 `Eof` 不能 break** —— 服务端是在 EOF **之后**才送 exit-status 的。
2. **`exit_status: None` 绝不能当 0** —— 那正好把「没跑成」读成「跑成了」。变异 R1 钉住。

## §3 `abort_marker`：不让超时把最有用的诊断吞掉

旧 daemon 不认 `--fork-session` 会**掉进流模式**（长连接、永不 EOF）。老实收到通道关闭
就只能等 30s 超时，而超时报出来的是一句「超时」，不是「去重新部署 daemon」。

⇒ `connect_and_exec_capture` 收一个 `abort_marker`：stdout 一出现 `"kind":"hello"` 就立刻收工。
**必须是子串判、不能是整行判** —— hello 帧可能还没到换行就该抽身了（测试 `hello_marker_matches_partial_line`）。

## §4 daemon 只收 sid，所以两条路是**两条命令**，不是一条命令加个 origin

- 本机 `create_branch_session(sourceJsonlPath, messageUuid)` —— monitor 自己读写文件。
- 远端 `create_remote_branch_session(origin, sourceSessionId, messageUuid)` —— daemon 自己干活，
  **刻意不接受路径入参**（少一条路径穿越面）。

签名不同 ⇒ 命令也不同。变异 R6 + 测试「远端命令里不许出现任何本机路径」钉住这条：
那个 `jsonlPath` 是**本机视角**的，发过去在远端根本不成立。

## §5 ★★ 两个信号各答各的，不许互相顶替（`deriveForkSource`）

- **活没活着 / 在哪个 tmux 里** → tmux 清单（`@ccm_sid` 精确匹配，INVARIANTS §30）
- **属于哪个账号** → pidfile（`--session-accounts`）

tmux 清单里**没有账号信息**。所以「tmux 里找到了、但账号查不到」是一个真实且常见的状态
（账号功能没启用 / cc-acct-iso 没部署）。此时 `liveConfigDir` 必须是 **`undefined`（不知道）**，
写成 `null` 就是宣称「确认是账号 0」—— 分叉会静默起在账号 0 上。变异 R5 钉住。

**本机没有对侧探针**：daemon 的 `--session-accounts` 是远端专属，本机侧至今没有
「某 sid 现在跑在哪个账号下」的查询（`local_accounts.rs` 只枚举账号，不认会话）。
⇒ 本机一律按「查不出来」处理，**问一次**，而不是拿当前账号顶替。如实记在这里，不粉饰。

## §6 追问小窗：默认值就是防线

默认值会被大量用户直接回车确认掉。所以账号那一格的默认位摆的是**账号 0**（不注入），
不是「当前账号」—— 后者等于把 `fork-launch.ts` P1 那条防线绕过去，只不过多了一次点击。

**取消必须 resolve `null`**，不能退化成 `{}`（那会被编排器当成「用户确认了默认值」照常起）。
三条取消路（按钮 / Esc / 点背景）都测了；变异 R7 钉住。

## §7 本机那条路**不问 tmux**

`resume_history_session` 把载荷交给用户自配的拉起器，tmux 与否根本不在它的表达能力里。
**问一个答案会被忽略的问题，比不问更坏** —— 用户会以为自己选了。
⇒ `fork-start.ts` 在 `origin === null` 时把 tmux 这格从追问清单里摘掉。变异 R8 钉住。

## §8 三处结构性收拾（都不是"顺手"，各有具体病）

1. **`tmux-sessions.ts`**：`findClaudeTmuxMatches` 一族原住 `tabs.ts`，而 `tabs.ts` 要 import
   `fork-flow.ts` ⇒ 反向 import 成环。搬的是**判据**，不是缓存策略（`TMUX_CACHE_TTL_MS`
   与 `tmuxCache` 留在 `TabManager`）。`tabs.ts` 原样 re-export，既有 import 面零改动。
2. **`TmuxSession` 改用生成物**：前端此前**手抄了一份** Rust 类型，两份各写各的。
   Rust 侧加 ts-rs 导出，手抄那份换成 re-export。
3. **`list_remote_tmux` 进包装层**：`generated-boundary-guard` 钉死「直接 import `invoke`
   的生产文件恰好 3 个，且就是设计上该剩的那 3 个」。`fork-flow.ts` 若裸用 `invoke` 就成第 4 个 ——
   **那个数不该为了让我过关而 +1**，该走包装层。

## §9 顺带还清一笔欠账

`parity_ledger.rs` 里 `history.branch` 原是 **ParityDebt**：「远端历史会话不能分支」。
G6 落地后它两侧都有 ⇒ 该行不对称理由**删掉**（留着就是宣称一条已经补上的欠账）。
形状钉死的三个数同步改：命令 121→122 · 不对称 20→19 · 欠账 11→10。

## §10 被顶掉的 `resumeBranch`

`session-viewer` 原来分叉完弹个 toast、用户再点一下才 resume（且只能本机、不带账号）。
现在分叉之后**直接起**。它那条 **F06 纪律（sid 校验先于任何 IPC 往返）没有丢**，
搬进了 `fork-flow.ts` 的 `startLocal`。

## §11 变异（8 条，全红；改完先 grep 计数确认落地，退出码判定）

| # | 变异 | 结果 |
|---|---|---|
| R1 | `exit_status: None` 当成 exit 0 | 红（且核过是断言失败，不是编译失败） |
| R2 | 旧 daemon 检测挪到解析之后 | 红 |
| R3 | `--fork-session` 的 sid / uuid 位置对调 | 红 |
| R4 | 认不出的 stderr 吞掉 | 红 |
| R5 | 账号查不到时落 `null` 而非 `undefined` | 红 |
| R6 | `branch-button` 无视 origin 恒走本机 | 红 |
| R7 | 追问小窗取消时 resolve `{}` | 红 |
| R8 | 本机也照问 tmux | 红 |

## §12 未做（如实）

- **daemon 没为 `--fork-session` 声明 capability token**。今天靠 hello 帧探旧 daemon，够用；
  但 capability 是更干净的判据。远端分叉是**用户手势触发的一次性查询**，不像流模式 flag
  那样需要在握手时就决定，所以没加。要加是 daemon 侧的小改动。
- **真机远端分叉未跑**（红线：不启动真实已认证的 claude / 不碰在跑的 daemon）。
  daemon 那半有 `e2e/daemon-fork-session.sh`（10 条断言、`CLAUDE_CONFIG_DIR` 隔离夹具）覆盖；
  monitor↔daemon 的**接缝**只有单测覆盖。

## §13 门禁

vitest **1103 / 77 文件**（+23）· tsc 0 · build 通过 · `cargo test --all` **653**（+11）·
`cargo test -p branch-core` 8 · daemon crate 183 · 两侧 `cargo fmt --check` 干净 ·
clippy 新代码零告警 · `test:daemon-fork` 10/10 · `test:ccm-rbind-title` 8/8 ·
eslint 7 / stylelint 50 未变基线。

## §14 签收

- [x] 过代码审计（8 条变异全红）
- [x] 过工程审计（三处结构性收拾各有具体病；INVARIANTS §1 加段；平价欠账还清）
- [x] 主计划已更新
