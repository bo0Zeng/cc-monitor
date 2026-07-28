# 功能计划 — R07 `planLocal` 的假声明处置

## 0. 问题（逐条核实过，非转述）

`src/launch-requests.vitest.ts` 的头注写着：

> F06（unify-launch）：`planLocal` 单测——**证明本地路径真的在用同一套维度注册表（不是套了个
> 类型皮的假装）**，并锁死实现期修正加的 sid 校验。

**这句话是假的。** 核实（`grep -rn planLocal src/ --include=*.ts | grep -v vitest`）：

| 调用点 | 形态 |
|---|---|
| `src/views/history.ts:1517` | `planLocal({ kind: "resume", sid: ctx.sessionId }, ctx.cwd);` |
| `src/views/history.ts:1557` | `planLocal({ kind: "new" }, ctx.cwd);` |
| `src/views/session-viewer.ts:357` | `planLocal({ kind: "resume", sid: sessionId }, cwd ?? "");` |
| `src/tabs.ts:2020` | `planLocal({ kind: "resume", sid }, tab.cwd ?? "");` |

**四个调用点全部把返回值当语句丢弃**（`{ctx, plan}` 没有任何消费者）。
真正下发的命令来自 Rust：`invoke("resume_history_session")` / `invoke("new_local_session")`
→ `history.rs::build_local_ps_command`。

**（订正：STATUS §R 的 R07 行写"三个生产调用点"，实际是 4 个。）**

所以 `planLocal` 唯一的真实作用是**校验**——它在 sid 非法时 throw，调用点因此把它摆在 `invoke`
之前。`tabs.ts:2017` 的注释其实已经如实写了这件事：
「F06：走一遍本地 IR 构造，sid 校验先于 `resume_history_session` 这次 invoke」。
即**代码注释是诚实的，测试头注不诚实**。

## 1. 两个选项与取舍

### 选项 A：真接上（让本地路径真的消费 IR 输出）
**否决。** F06 已经论证过这条路走不通，账本 `src-tauri/src/history.rs` 那一行写着：
采用「Rust 侧同构 renderer」而非「IR 前端构造下发」，因为 `Get-Command`（探测本机有没有 `cc`
PowerShell 函数）是 **render-time 决策、只能在目标机器上做**，TS 无法预先渲染好交给它。
真接上等于推翻 F06 已论证并落地的决策，而 R 段的定位是"收紧既有产出"，不是重开设计。

### 选项 B：改名 + 订正注释（采纳）
把函数名改成它实际做的事，让"假装"变成"如实"。

## 2. DoD

- [ ] `planLocal` → `validateLocalLaunch`，返回类型从 `LaunchPlanBuild` 改 `void`
      （返回值本就无人消费；保留返回值等于继续邀请误解）。
- [ ] 函数头注写清它的**真实职责**：本地路径的**前置校验**（sid 字符集 + cwd），
      以及为什么它不产出命令（指向 INVARIANTS §36 与账本 `history.rs` 那一行）。
- [ ] `launch-requests.vitest.ts` 头注那句假话改成实话。
- [ ] 4 个调用点改名（tsc 逐个揪出）。
- [ ] **不改任何行为**：校验仍在 `invoke` 之前、错误文案逐字不变、
      4 个调用点的两阶段 catch 结构不动（F06 Phase D 统一过的 toast headline 措辞）。
- [ ] INVARIANTS §36 补一句：本地路径**只借 IR 做校验、不消费其输出**，这是设计不是半成品
      （§36 现在说的是 `plan.env` 故意算出来不消费，语义相邻但不是同一件事）。

**不做什么**：
- **不**删掉这次校验（它是 F06 实现期发现并补上的真实一致性缺口——本地路径此前唯一缺
  `isValidSessionId`，其余 4 个 `planXxx` 早有）。
- **不**动 Rust 侧 `build_local_ps_command`。
- **不**顺手把校验挪进 Rust（那是另一件事，且会让"校验先于 IPC"这条顺序保证消失）。

## 3. 测试策略

- 既有 8 条 `launch-requests.vitest.ts` 测试改名后必须全绿（它们本来就在测校验与 ctx 构造，
  内容不用改，只改被调函数名）。
- 变异检查：把 `isValidSessionId` 那道校验删掉 → 相关测试必须转红
  （证明"改名没有顺手把校验弄没了"）。
- 4 个调用点无行为变化，靠 tsc + 既有 DOM 测试（`tabs.vitest.ts` / history 相关）兜住。

## 4. 代码审计结果（Phase D）
（待填）

## 5. 工程审计结果（Phase E）
（待填）

## 6. 签收
- [ ] 通过代码审计（无阻塞项）
- [ ] 通过工程审计
- [ ] 主计划已据此更新（含变更记录）
