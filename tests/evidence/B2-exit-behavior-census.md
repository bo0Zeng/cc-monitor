# B2 · 「退出行为」那个值搬家 —— 现打（第三波，基线 `aede6f5d`）

出处：`设计/01 §3.3b`（2026-09-18 拍板 · 条 66）· 判据 `E1–E4`。本文件是子步 1：**先现打、再动手**。
读数全部现打于本树（`/home/zbl/cc-wt/w3-b2`），不照抄 `W20-cond51-66-readings.md`（那份是 09-19 的，下面逐格核过）。

---

## 一、那个值今天住哪、谁写、谁读（全集）

| # | 住址 | 角色 | 搬家后 |
|---|---|---|---|
| 1 | `src/backend-policy.ts` 的 `KEY = "backendPolicy"` ＋ `readPolicy` / `killOnExit` / `setKillOnExit`（带 `writeChain` 串行链） | **持久化**：monitor 自己的 `config.json`，按 origin 分键 | 🔴 删（`§3.3b ②`「monitor 的 config 里不许再留一份」） |
| 2 | 同上 `pushPolicyToBackend` / `initBackendPolicy` | 启动时把整表**推**给 Rust | 🔴 删（`§3.3b ④`「那条推送随之退役」） |
| 3 | `src/main.ts` 启动时调 `initBackendPolicy()` | 推送的触发点 | 🔴 删那两行 |
| 4 | `src/config.ts` 键登记 `backendPolicy: "src/backend-policy.ts"` ＋ `tests/config-unknown-keys.vitest.ts` 那一行 | 配置键的归属表 | 🔴 删（恒等地少一键） |
| 5 | `src/ipc/commands.ts::set_backend_kill_on_exit` → `src/bridge/src/backend_policy.rs::set_backend_kill_on_exit`（`#[tauri::command]`）→ 进程内 `table()`（`OnceLock<Mutex<HashMap>>`） | **生效值**：monitor 进程内存里的表 | 🔴 删整条推送链 ＋ 那张表（`E2` 点名的「启动时快照」正是它） |
| 6 | `src/bridge/src/backend_policy.rs::kill_on_exit(origin)` ＋ `DEFAULT_KILL_ON_EXIT` | 读那张表 | 🔴 删；缺省值搬到后端 |
| 7 | `src/bridge/src/lib.rs` 的 `RunEvent::Exit` 臂：`let kill = backend_policy::kill_on_exit(LOCAL_ORIGIN)` → 被监护那条 `h.stop()` ＋ `shutdown_detached_ways_on_exit(kill, PRODUCTION_EXIT_SHUTDOWN)`（常驻 ＋ 中转两个收口点） | **退出决策处**（monitor 侧唯一一处） | 改：见 §三 |
| 8 | `src/bridge/src/backend/control/backend_control.rs::backend_status` 回 `"killOnExit"` 一格 | 状态查询顺带报值 | 🔴 删那一格（值不在 monitor 了） |
| 9 | `src/settings/backend-section.ts`：`initBackendPolicy()` ＋ `killOnExit(this.policy, …)` 画勾 ＋ `setKillOnExit` 改勾 | 界面 | 改：读写都走后端命令 |
| 10 | `tests/bridge/parity_ledger_tests.rs`：`("set_backend_kill_on_exit", "app.backend-policy", Side::Both)` ＋ 命令总数 153 | 命令登记 | 换成新命令（恒等计数跟着改） |
| 11 | `tests/ipc/commands.vitest.ts`：包装层 143 条 | 命令登记 | 跟着改 |

**后端那一侧（`src/backend/`）今天对这个值一无所知**：`grep -rn "backend\.json" src/ tests/`（排除 `tests/evidence/`）**0 处**；
`kill_on_exit` / `killOnExit` 在 `src/backend/` **0 处**。⇒ 这次搬家在后端侧是**从零立**，不是改。

## 二、E1–E4 的前提今天各差什么

| # | 判据 | 今天 | 要补 |
|---|---|---|---|
| E1 | `backend.json` 写者全仓只有一处、在后端 | 0 个写者（文件不存在） | 后端立唯一写口 ＋ 全仓零命中判据（两向） |
| E2 | 退出决策处现读，路径上无 memo/`OnceCell`/启动快照 | 🔴 决策处读的就是 #5 那张 `OnceLock` 表 ＝ 启动时推进来的快照 | 决策处改成现读；缓存点数 == 登记表 0（相等） |
| E3 | 那几句文案只有一个家（人群 +1） | TS `EXIT_*` **三**条（`grep -c "^export const EXIT_" src/backend-policy.ts` = 3）；Rust `EXIT_COPY` 三条逐字对拍 | 加「读不出来」那一句 ⇒ **三变四**（`01 §3.3b ⑤` 写的「四变五」是假前提，`W20` 已逮，本次现打仍是 3） |
| E4 | 折进前端那一档 `describeExitBehavior` 回「不适用」 | 函数签名里**没有「壳」这一维**；全仓也没有「折进前端」那个壳（`W20 §二` 三条现打仍成立） | 给入参加「壳」一维（值由后端答），纯函数判据逐格钉 |

## 三、「退出」这一片今天有几个进程、各由谁收（`§3.3b ⑥` 的前提）

| 进程 | 怎么起的 | 宿主一走它会怎样 | 今天谁按策略收它 |
|---|---|---|---|
| 本机后端 · **常驻（脱离）** | `local_backend_host::start_detached`（Linux 默认；`CCM_NO_DETACH` 关掉） | 回环口上的**流**断开；进程**继续跑**（`main.rs::serve_listening` ③ 回到空转） | monitor 退出臂 → `stop_detached_backend_on_exit` |
| 本机后端 · **被监护（stdio）** | `local_backend::supervise_with_stdio`（非 Linux / 关掉脱离时） | stdin 读到 EOF；**不敏感**，直到下一次写失败才退 | monitor 退出臂 → `h.stop()` |
| 本机**中转**（`--relay`） | `local_backend_host::start_local_relay` → `local_backend::supervise`（**stdin = null**，无消费者） | 什么都感觉不到（它的 stdin 本来就是 null） | monitor 退出臂 → `stop_relay_on_exit` |
| 远端后端 | `ssh_source` 经 SSH exec，stdio | SSH 通道断 ⇒ 管道破裂退出 | **没有人**：退出臂只读 `LOCAL_ORIGIN` 那一格 ⇒ 远端那一行的勾今天**一个进程都不收**（它本来就会随 SSH 死，所以没说谎，但那一格的值从来没被读过） |

⇒ 两件**结构性**的事，决定了切法：

1. **「最后一个客户断开」只在常驻那一支上是一个后端观察得到的事件**（`serve_listening` 的 ③）。
   stdio 那两支的「客户」就是起它的那个 monitor 进程本身（管道），常驻的是「谁连上来」。
   ⇒ `§3.3b ⑥` 的连接计数只在常驻那一支上有意义；今天那条载体**单流**（`listen::admit` 第二条流回 `stream-busy`，
   由 `single_stream_guard.rs` 看着）⇒ 计数恒 ∈ {0, 1}，「归零」＝「那一条流断了」。
2. **中转的 stdin 是 null**：它自己感觉不到宿主离开 ⇒ 它的去留只能由它的宿主（monitor）决定，
   而宿主手里已经没有那个值 ⇒ 宿主要在决定那一刻**现问**后端。

## 四、`lingerMs`（`§3.3b ⑦`）与后端零定时器铁律（P6）撞了

`tests/backend/no_timer_guard.rs::backend_production_code_has_no_periodic_wakeups` 按**调用形态**禁
`sleep(` / `timeout(` / `interval(` … 八个名字，**没有登记口**（`REGISTERED_DURATION_USES` 只登记 `Duration::from_*`，
不放行 `sleep`）；报文逐字「确有必要请先改本护栏的登记表并说明理由」。
「归零后等 `lingerMs` 再决定」= 一个会自己醒来的一次性构件 ⇒ **在后端里做就必须给 P6 开一个口**。
`§3.3b ⑦` 论证过「通信层零期限常量那条不受影响」，**没论证过后端 P6** ⇒ 这是设计里没碰过的一格。
⇒ **本路不开那个口，也不把 `lingerMs` 写进文件当摆设**：归零即现读即决定；这一格上报主会话拍板（见交付报告）。

## 五、与 `lib.rs::windows_to_destroy_after`（A2+ST1）的边界

| | `windows_to_destroy_after` | 退出行为（本路） |
|---|---|---|
| 回答的问题 | **monitor 这个进程要不要退**（主窗销毁时连带销毁隐藏的设置窗，否则看不见的窗口吊住进程） | **monitor 退了之后，后端那几个进程怎么办** |
| 所在层 | 窗口层（`WindowEvent::Destroyed`） | 进程层（`RunEvent::Exit` ＋ 后端自己的流结束事件） |
| 关系 | 它是退出行为的**上游触发器**：窗口全没了 ⇒ tauri 走 `RunEvent::Exit` | 它从不决定 monitor 退不退 |

两者不共享任何值、不互相调用；本路**不碰** `windows_to_destroy_after` 及其判据。

## 六、写区（现打定下）

- 后端：`src/backend/control/exit_policy.rs`（新）· `control/mod.rs` 一行 `mod` · `inbound.rs` 两条登记 ＋ `COMMANDS` · `lib.rs::SUBCOMMANDS`（CLI 面是 `cli_exposed` 派生的必然）· `main.rs::serve_listening` 流结束那一臂 · `tests/backend/readonly_guard.rs`（后端写盘人群要登记这个新写者）· `src/doc/IPC-PROTOCOL.md` 两小节。
- monitor：`src/bridge/src/backend_policy.rs`（推送链退役 ＋ 新的两条命令 ＋ 退出那一刻现问）· `lib.rs` 退出臂 ＋ 命令表那一行 · `local_backend_host.rs` 常驻那个收口点退役 · `backend_control.rs::backend_status` 那一格。
- 前端：`src/backend-policy.ts` · `src/settings/backend-section.ts` · `src/main.ts` 两行 · `src/config.ts` 一行 · `src/ipc/commands.ts` 两条。
- 对应 tests 与登记（`parity_ledger` · `commands.vitest` · `backend_route_tests::SENDERS` · `config-unknown-keys` · CP1 台账）。
- 设计文档：`设计/01` 末尾。
