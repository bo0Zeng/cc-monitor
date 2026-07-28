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
| `src/tabs.ts:2024` | `planLocal({ kind: "resume", sid }, tab.cwd ?? "");` |

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

- ~~既有 8 条测试改名后必须全绿，内容不用改~~ → **实际是 9 条，且被重写拆分**
  （Phase D 审计订正）：函数改返回 `void` 后，原先断言 `ctx`/`plan` 字段的那几条无处落脚——
  **而那正是问题本身**，它们检查的是生产侧被丢弃的产物。已拆成「函数契约」与
  「维度注册表在 transport:local 下的行为」两组，后者直接冲 `buildLaunchPlan` 去。
- 变异检查：把 `isValidSessionId` 那道校验删掉 → 相关测试必须转红
  （证明"改名没有顺手把校验弄没了"）。
- 4 个调用点无行为变化，靠 tsc + 既有 DOM 测试（`tabs.vitest.ts` / history 相关）兜住。

## 4. 代码审计结果（Phase D）

独立对抗性 agent（53 次工具调用 / 10 条变异 / 整仓 rsync 沙箱 + `git archive HEAD` 对照沙箱）。
**1 阻塞 + 5 重要，全部已修**，并**推翻了本计划最核心的那条论证**。

### 阻塞（已修）：`77d1486` 是坏 commit——HEAD 上本地 resume 是功能性损坏的
我在 commit R05 **之前**就改了 `tabs.ts` 的 `planLocal` → `validateLocalLaunch`（那是 R07 的活），
R05 的 `git add src/tabs.ts` 把它卷了进去。审计在 `git archive HEAD` 纯净快照上实跑：
- `tsc` → `TS2305: has no exported member 'validateLocalLaunch'`（exit 2）
- `npm test` → 红：`本地归档 tab → 仍走 resume_history_session` 期望 `invoke('resume_history_session')`、实收 `frontend_perf_log`

**这不是编译噪音**：HEAD 上 `validateLocalLaunch` 是 `undefined`，调用抛 TypeError → 落进 catch →
弹「无法构造 resume 命令」→ **本地 tab resume 永远拉不起终端**。R05 的 commit message 自称
"npm test 照绿"，as-committed 不成立。
**我先前把这条评估成"bisect 会构建失败"是低估了**——它是功能损坏，不只是历史洁癖问题。

**处置**：核实 `77d1486` **从未 push**（`git branch -r --contains` 为空）→ 直接 amend，
把 tabs.ts 的改名退回 `planLocal`，让 R05 那个 commit 自洽；改名整体归入本 commit。
amend 后用 `git archive` 实测该 commit **tsc exit 0**。**没有改写任何已发布历史。**

### 阻塞连带（已修）：否决"真接上"引用了**错误的论据**，而且写进了规范性文档
`Get-Command` 论证在 F06 里**真实存在**（`:27-30`），但它排除的是"**TS 全量渲染好字符串、
Rust 只管 exec**"这一形态，**并不排除**"TS 构造 IR、Rust 只补 `Get-Command` 那一步"。
更要命的是 `F06-local-path-ir.md:64-72` 有一条**已勾 `[x]`** 的 DoD 逐字要求
"从产出的 `LaunchPlan` 取 `action`/`cwd`/`launcher` 三个字段映射回现有 Tauri 调用参数"
——**它从未实现**，是在 §3.2 被撤回的，撤回理由是
**"`plan.action`/`plan.cwd` 恒等于输入，没有信息增量可取回"**，与 Get-Command 毫无关系。

即：**"不接"是因为接了也拿不到新东西，不是因为技术上不可能**。两个理由的强度与适用范围完全不同。
已订正函数头注与 INVARIANTS §36，并在 F06 那条 `[x]` 旁就地标注撤回。

### 重要（已修）1：「走一遍 `buildLaunchPlan` 是便宜的一致性检查」这个声称零门禁守护
审计变异 M3b：删掉整段 ctx 构造 + `buildLaunchPlan(ctx)`、只补一句 `void cwd;`
→ `tsc` **绿**、`npm test` **705 全绿**。改造前同一变异**红 5 条**——因为那时返回类型让这次调用
在**类型层**是承重的；**改成 `void` 恰恰把类型层强制降级成了一句谁都能顺手删的裸语句**。
（M2 只删调用不删 ctx 会被 `noUnusedLocals` 挡住，但那是副作用不是设计。）
而它想验的东西**别处已经在验**：`launch-render-cli.test.ts` 有
`ctxOf({ transport: { kind: "local" } })` → `buildLaunchPlan` 的用例。

**处置：删掉那一遍**（审计给的 (a) 选项，与 R07 诚实化主旨最一致）。理由三条：
生产侧结果被丢、纯浪费；是 **fail-closed 风险**（将来任何对 `transport:local` 抛异常的新维度
会让本地 resume 彻底拉不起来而收益为零）；它想验的已被别处覆盖。
函数现在只剩 sid 校验——**复跑变异确认这唯一的语句是承重的**（删掉 → 转红）。

### 重要（已修）2：孤儿 JSDoc
diff 只换了 `export function` 那一行，F06 的旧 JSDoc 块原样留着 → 同一个函数顶着两个 `/** */`，
且旧块开篇"本地路径的 `LaunchContext` 构造"与新块"前置校验"自相矛盾。
**没有直接删**（旧块里"`plan.env` 恒非空、故意不消费、等价保护在 `lib.rs::scrub_env_vars`"
这段新块没有）——已合并成一块。

### 重要（已修）3：§36 把可辩护的现状写成了规范性铁律，且与 F06 已勾的 DoD 直接冲突
§36 是 INVARIANTS（铁律体裁），我加的新段标题写"**这是设计不是半成品**"，
而 F06 §1 有一条 `[x]` 勾着的 DoD 说的正是"值来自 LaunchPlan"——二者只能有一个成立。
已按审计建议处理：F06 那条勾就地标注撤回（含真实撤回理由），§36 改成
"当前决策 + 撤回理由（信息增量为零）+ 论据订正"，并**重写标题**——
原题「本地路径的 `plan.env` 故意算出来但不消费」在 R07 删掉那遍构造后已经不成立，
改为「本地（Windows）路径不经 IR 产出命令」。铁律本身（别给本地渲染器补读 env 的代码）不变，
理由反而更直接：本地路径压根不产出 `plan.env`。

### 重要（已修）4：计划文与实际不符
§3 写"既有 8 条……内容不用改、只改函数名"——实际 9 条且被重写拆分；
DoD"4 个调用点改名"当时工作区只改了 3 个（第 4 个已在坏 commit 里）；§0 行号 `tabs.ts:2020` 实为 2024。

### 重要（已修）5：6 处 `planLocal` 引用未跟进
`MASTERPLAN.md` ×3、`INVENTORY.md` ×4、`STATUS.md` ×4。其中
**`INVENTORY.md:79` 的复核锚点 `grep -n 'export function planLocal'` 现在零命中**
——正是 R06 把 INVENTORY 改成符号锚点时预期要暴露的那种失效（"锚点自己会报错"）。
已全部订正，并重跑 INVENTORY 全表锚点校验：**零空锚点**。

### 建议（已落实）
- 新 describe 的「返回 void → `toBeUndefined()`」与「合法输入不抛」**结构上不可能红**
  （签名是 `: void`；删掉 throw 后"不抛"照样过）。已删掉前者、保留后者并**就地标注"这条是文档不是门禁"**。
- **补回拆分中丢掉的 `cwd: null` 原样透传覆盖**：审计实测 M7（`buildLaunchPlan` 内
  `cwd: ctx.cwd ?? ""`）改造前红、改造后全仓 705 全绿。**这是共享代码**（远端路径也吃），已补一条，
  复跑 M7 确认转红。诚实标注：今天三个 `plan.cwd` 消费者都用真值判断、`""` 与 `null` 等价，
  故当前是等价变异、低危；但钉住的成本近零。
- `npm test` 是 `&&` 链，前面任一 tsx 脚本红就短路、vitest 根本不跑——判色时别只看 exit code。已记。

### 审计核实为真的部分
- **"那句头注是假的"成立，且比我说的更硬**：它用来"证明"注册表起作用的断言是
  `expect(plan.action).toEqual(...)`，而 `buildLaunchPlan` 对 `action` 是**恒等透传**
  ——那条断言对"注册表有没有起作用"零信息量。测试没证明它头注声称的事，那件事本身也不真。
- **4 个调用点、返回值全丢弃、真命令由 Rust 构造**——全部属实。排除了 re-export/barrel/
  动态 import/字符串形式调用/e2e 侧引用五种间接路径。
- **两个 ctx 逐字段一致**，测试**不是**在测虚构输入（8 个字段逐条比对，唯一差异是 cwd 值域收窄，
  已补回）。
- `isValidSessionId` 没在改名中丢（M1 改造前后都红）。
- §36 里其余符号名在改名后**仍全部成立**（逐个反引号符号核对）。

## 5. 工程审计结果（Phase E）

- **主计划是否仍自洽**：是。账本 `src-tauri/src/history.rs` 那一行的"Rust 侧同构 renderer"决策不变；
  但**其论据的适用边界已在 §36 澄清**（Get-Command 只排除"TS 全量渲染"这一形态）。
- **是否引入拖累后续功能的耦合**：没有。反而减了一处 fail-closed 风险。
- **是否有应现在就做的统一重构**：无。R15/R16 已登记，不在此做。
- **工程健康度**：tsc 0 / npm test **705** / coverage / `ccm-print-parity` 12 / `ccm-cli` 44 /
  `tmux-target` 26 / INVENTORY 全表 grep 锚点零空锚点。

## 6. 签收
- [x] 通过代码审计（1 阻塞 + 5 重要全部修完；坏 commit 已 amend 且实测该 commit tsc exit 0）
- [x] 通过工程审计
- [x] 主计划已据此更新（含变更记录）
