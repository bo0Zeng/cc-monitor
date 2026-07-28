# 功能计划 — R04 IR 内核结构性收紧（四处）

> 本文件在实现期补写（Phase B 的内容原本写在 `STATUS.md` §R 段 R04 行里，四处子项已逐条列明）。
> 补写的动机不是形式：实现过程中发现了一处**没预见到的行为变化**（见 §3.1），
> 需要一个地方把它记清楚。

## 0. 为什么做这四处

依据是「IR 内核独立重设计对拍」§2.2——那次让一个 agent **先独立设计 IR、后读实现**，
结论是内核该保留（它的自由方案反而撞回了 R11 与 fail-soft 两个坑），
但指出了四处「现有实现是对的，只是靠约定而非结构维持」的地方。四处的共性：
**把只写在注释里的纪律，变成类型/结构上做不到的事。**

## 1. 四处子项与 DoD

### ① `canRenderCli` + `renderCli` → `tryRenderCli`（返回 Result）
**问题**：`renderCli` 里 `if (flags) tokens.push(...flags)` 对 `cliFlags` 返回 `null`
**静默跳过**，继续渲染 → 产出一条**丢了该修饰**的命令。于是"诚实降级"（INVARIANTS §33）
只是**调用约定**（必须先问 `canRenderCli`），不是结构保证。丢的恰好是账号这类东西 →
症状即 R11/R08 那族「看起来生效了，只是用了错的号」。
- [x] 合成单一导出 `tryRenderCli(plan, ctx, probe, ccmPath?) → {ok:true;cmd} | {ok:false;reason}`
- [x] `null` 在同一次遍历里 → `ok:false`（**拿不到 cmd**）
- [x] `reason` 把"为什么降级"带出来（此前这个信息是丢掉的）
- [x] 唯一生产调用点 `renderLaunchCommand` 改成 `if (r.ok) return r.cmd; return renderFallback(plan)`

### ② 能力要求下放到维度 `requiredCaps?(ctx)`
**问题**：渲染器里并存两套机制——静态 `CLI_REQUIRED_CAPS`（语义"每次调用都要求"，只对
`applies` 恒真的维度成立）+ 一条给 `model` 的**针对性特判**（因为 `MODEL_DIMENSION.applies`
是条件式，塞进静态列表会误伤未配模型偏好的多数会话，见 F08 §3.2 / INVARIANTS §37）。
那条特判本身对，但它把"这个维度需要远端 ccm 支持什么"放在**离维度定义很远**的地方。
- [x] `LaunchDimension` 加 `requiredCaps?(ctx): readonly string[]`
- [x] 渲染器**只对已触发的维度**收集 → "条件式维度只在真触发时才要求能力"成为结构保证
- [x] `ACCOUNT_DIMENSION.requiredCaps = () => ["account"]`，从静态列表移出
- [x] `MODEL_DIMENSION.requiredCaps = () => ["model"]`，删掉渲染器里的特判

### ③ `EnvOp` 的 `unset` 侧收窄
**问题**：export 侧当初刻意用窄变体（`export-config-dir` 而非通用 `{op:"export";key;value}`），
理由写在 `launch-plan.ts` 头注第 3 条（防维度绕开 `isValidConfigDir` 塞任意变量名，审计 D7）。
**同一条理由从未应用到 unset 侧**——`{kind:"unset"; keys: string[]}` 是自由字符串数组。
- [x] 收窄为 `{kind:"unset-config-dir"}` / `{kind:"unset-nested-env"}` 两个无参变体
- [x] 键表移进渲染器按 kind 查（`AGENT_PROFILE.nestedEnvVars` 仍是唯一来源）
- [x] **渲染输出逐字节不变**（推论④ 硬约束）

### ④ `WrapSpec` 闭包 → 纯数据
**问题**：`wrap: (inner) => string` 让 `LaunchPlan` 不可序列化/不可比较/不可对拍，
且闭包能做任意事 = 在 IR 里开了个"绕过渲染器自己拼字符串"的后门，与头注
"绝不拼字符串——字符串化是渲染器的事"直接冲突。
**趁 `plan.wrap` 今天恒空（全仓唯一赋值点是 `buildLaunchPlan` 的 `wrap: []`，零生产者）做，
成本为零**；等 F04 的 rbind 真落进来就不是了。
- [x] 改 `{id, order, prelude}`，折叠逻辑 `( <prelude>; exec <inner> )` 收回渲染器
- [x] 把「rbind 到底走不走 wrap」这条**悬空设计**写进 INVARIANTS §39（见下）

**不做什么**：
- **不**动 `shared/ccm`（外部预言机，风险最高可观测性最低）。
- **不**动 `renderFallback` 的输出（推论④）。只改它**怎么拿到**要渲染的东西。
- **不**删 `wrap` 字段（虽然零生产者）——见 §4 对这条的论证。

## 2. 与主计划的对接

账本 `src/launch-render-fallback.ts`+`launch-render-cli.ts`+`ccm-probe.ts` 那一行的最终形态是
"双渲染器 + `canRenderCli` 对任一维度 `cliFlags` 返回 `null` 或容器 `mode!=="create-or-attach"`
一律强制走兜底（诚实放弃，防 #76 复发，见 INVARIANTS §33）"。
本功能**不改这个语义**，只把它从"两个函数 + 调用约定"改成"一个函数 + 结构保证"。
账本该行需据此更新（Phase F）：`canRenderCli`/`renderCli` 已合成 `tryRenderCli`。

INVARIANTS §33 需**更新而非新增**——它原文写"`canRenderCli` 是两者之间的唯一分流点"，
那是**意图**不是当时的事实（`renderCli` 可被直接调用并静默丢修饰）。已补 R04① 更新段。

## 3. 测试策略

- ③④ 的核心是"输出零变化"：`launch-dimensions.test.ts` 补两条——
  ③ 断言两个 unset 的**字面输出与顺序**（`unset CLAUDE_CONFIG_DIR; unset <嵌套env 全套>; `）
  且类型层已无 `keys` 字段；④ 断言 wrap 折叠的嵌套顺序与 `exec` 不丢。
- ① 用**真实可达**的状态验结构保证（不编造）：`LaunchAccount` 允许"有 configDir 但无名字"
  （老式直调路径，见 `launch-requests.ts::accountOf` 头注），此时 `cliFlags` 返回 `null`
  → 断言 `ok:false` 且**结果里没有 `cmd` 字段**。
- ② 的等价性由既有 F08 三条测试（配 model + 无 model 能力 → false 等）**仍全绿**证明；
  另补一条 account 的对称用例。
- `ccm-print-parity` 12 条是对 `shared/ccm` 输出的外部预言机，本功能不该动它一个字符
  ——**实际动了一行**，见 §3.1。

## 3.1 实现期的两个发现（必须记下来）

**发现一：`e2e/` 在 tsc 盲区里。**
`tsconfig.json` 的 `include: ["src"]` 不含 `e2e/`，所以 `e2e/ccm-print-parity-emit.mts`
里那句 `import { renderCli }` 在 ① 删掉该导出后**tsc 抓不到**：
`npx tsc --noEmit` 0 + `npm test` 701 全绿，而 `npm run test:ccm-print-parity` **12 条全红**
（`SyntaxError: does not provide an export named 'renderCli'`）。
→ 已改用 `tryRenderCli` + 一个"能力齐全"的假探测结果（本预言机要验的是命令行与真 ccm 的解析
是否对得上，不是降级逻辑，故必须走成功分支）。
→ 这条正是 **R00 把 7 套 e2e 接进 CI 的理由的实证**：改生产侧导出签名时，只有真跑 e2e 才暴露。
→ 已顺带 grep 全仓（含 `e2e/` `scripts/`）确认无其它盲区引用。

**发现二（行为变化，非计划内）：attach 路径不再要求远端 ccm 支持 `account` 能力。**
`tryRenderCli` 的 attach 分支在维度循环**之前**就 `return`（沿用改造前 `renderCli` 的
"attach 分支不读其余修饰"结构）。而 ② 把 `"account"` 从静态 `CLI_REQUIRED_CAPS` 移进了
`ACCOUNT_DIMENSION.requiredCaps` —— 于是 attach 走不到维度循环、**不再收集这条能力要求**。
改造前 `CLI_REQUIRED_CAPS` 是无条件检查的，attach **会**要求 `account`。

**判定：这是改对了，不是回退**，理由三条：
1. `ccm attach <名>` 根本不接受 `--account`——对一次纯 attach 要求远端支持 `--account` 是过度收紧。
2. `INVENTORY.md` §A 已把这件事写成设计：attach 是接回一个**已经在跑**的进程，
   它的账号在创建时就定了，此刻注入任何 env 都不会改变那个已存在进程的身份——
   **带账号在这里没有可实现的语义**。
3. 放宽后 attach 在"老 ccm 支持 attach 但不支持 --account"时会走 CLI 而非兜底；
   而 `ccm attach` 与兜底渲染器的 `SESSION_BACKEND.attach()` 逐字同构（INVARIANTS §33 已核对过
   `shared/ccm` 源码），两条路径输出等价，放宽是安全的。

但它**是一个我没预见的变化**。

**订正（Phase D 审计抓到）**：这一段初稿写"已补一条测试把新行为钉住"——**当时并没有补**
（我打算等审计对这条的判断回来再定，却把意图写成了完成态）。审计逐条核实后还发现
attach 一次放宽的是**三**道闸门而不是一道：`account` 能力、`model` 能力、以及
INVARIANTS §33 铁律#1（`cliFlags → null` 强制降级）本身。
且其中 `model` 那道的放宽**真实可达**——`model` 能力是 `06a9c76`（F08）才加进 `shared/ccm`
的 `capabilities=`，所以装了 F02～F08 之间任一版 ccm 的远端就处在"缺 model"状态。
现已补齐 `launch-render-cli.test.ts` 的「attach 豁免组」三条测试（各钉一道闸门），
并在 INVARIANTS §33 写明豁免范围与"若将来 `ccm attach` 学会接受修饰 flag 就必须撤销"。

## 4. 为什么不干脆删掉 `wrap` 字段（它零生产者）

诚实的反方论点：保留一个没人用的字段本身也是债。
保留的理由：① 它**不是投机设计**，是审计 C1 三方独立指出的真实需求
（`( __ccm_rbind; exec claude --resume S )` 是包裹不是追加，扁平字符串没有闭括号槽位）；
② 折叠逻辑今天就有独立测试，删了再加回来要重新验一遍；
③ 成本在改成纯数据后接近零（3 个字段的接口 + 渲染器里一个 reduce）。
**但这条判断有前提**：`plan.wrap` 恒空。R04④ 的测试里显式断言了 `plan.wrap.length === 0`
并注明"这条前提变了就该重新评估" —— 若哪天它有了生产者，说明 rbind 落地了，
那时 INVARIANTS §39 的开放问题必须先回答。

## 5. 代码审计结果（Phase D）

独立对抗性 agent（61 次工具调用 / 9 个变异实验 / 自写 720 场景 × 5 探针的渲染矩阵对拍）。
**1 阻塞 + 5 重要，全部已修**，且它推翻了我三处声称。

### 阻塞（已修）：三条新测试落在测试文件的"死区"，门禁零守护
`launch-render-cli.test.ts` 的失败聚合点（`if (failed > 0) throw`）在文件中段，
而我把 R04 的三条测试加在了它**之后**。后果是一个双向死区：
- **全绿时**它们跑，但不设门禁 → 删掉 R04① 的**全部**要害（`if (flags === null) return`）后，
  `npm test` 仍 **exit=0 / 701 passed**，只有一行 `✗` 混在输出里；
- **有红时**它们根本不执行（进程已在聚合点 throw）。

即 R04① 宣称的"结构保证"在 CI 上曾是**零守护**。已把 R04 块移到聚合点之前并复验：
同一变异现在让 `npm test` **exit=1**。

**这是 R02 记的三个失效模式之外的第四种：断言写在失败聚合点之后。** 已补进纪律清单。

### 重要（已修）1：R04④ 声称"覆盖已知唯一用例"——实测表达不出来
`applyWraps` 当时套的是**整条 payload**（`envOps + cd + argv`），于是 `exec` 落在 env 前缀前面：
`( __ccm_rbind; exec unset CLAUDECODE …; claude --resume S )`。
我独立复现了审计的 bash 论断：`bash -c '( echo RB; exec unset A B; echo REACHED )'`
→ `exec: unset: 未找到`、**rc=127**，launcher 根本起不来。目标形态是 bashrc 原文的
`( __ccm_rbind; exec claude --resume S )`。
这是 F03 就有的 call-site 缺陷（闭包版拿到的也是同一条拼好的串，一样错），
**但我把它写成了"已知唯一用例已钉死"，还让新测试断言了这个坏形态**——等于把跑不通的契约
钉进回归。已修：两个调用点改成只包 `renderArgv(plan)`（因 `plan.wrap` 零生产者，此刻零风险；
仍满足"sanitize 先于 wrap"），测试断言改成正确形态并补一条"env 前缀须留在包裹外"。

### 重要（已修）2：R04③ 丢掉了编译期穷尽性——纸面安全换真实静默洞
收窄前的末支 `return \`unset ${op.keys.join(" ")}; \`` **读了 `op.keys`**，因此**逼**编译器穷尽；
我的新末支不读 `op` 任何字段，把一切都兜住了。审计实测：给 `EnvOp` 加第 4 个变体
`{kind:"unset-proxy"}` → HEAD 快照 `tsc` **报错**，而我的版本 `tsc` **0 错**且把它
**静默渲染成嵌套 env 的 unset**。这与 R04 自己的立意正好相反。
已补显式 `never` 穷尽守卫并复验：加第 4 个变体现在 `tsc` 在守卫处报错。

**审计对 R04③ 价值的判定我接受**：这条收窄本身是**纸面**的——两个产出点都是硬编码字面量，
`AGENT_PROFILE.nestedEnvVars` 也是硬编码数组、非用户输入；收窄后同一批字符串仍被 `join(" ")`
拼进命令，只是从维度搬到了渲染器。**真实价值只剩"表达力对齐 export 侧"这一条美学收益，
而它的代价是上面那个真实的穷尽性洞。加上穷尽性守卫后值得做；不加则是净负。**

### 重要（已修）3：`launch-dimensions.ts` 的"语义不变"是错的 + 计划里一句假声称
见 §3.1 的订正段。已删掉"语义不变（恒真维度 ⇒ 每次都要求）"、补齐 attach 豁免三条测试、
并在 INVARIANTS §33 写明豁免。

### 重要（已修）4：本次 diff 造成一处自相矛盾的注释 + 一批悬空符号
`CLI_REQUIRED_CAPS` 上方仍写着"**F05 新增 account**……不列会漏掉真实降级场景"，
而紧接的那一行已经把 `account` 删了——读者会被直接误导。
同族悬空符号（`canRenderCli`/`renderCli` 已不存在）：`launch-render-cli.ts:8`/`:19`、
`launch-plan.ts`、`launch-dimensions.ts:54,56`、`remote-launch-run.ts:147`。
**R06 刚把 INVENTORY 改成符号锚点，这批违背同一纪律。** 已全部订正
（保留的少数提及都是明确的历史叙述"改造前是两个独立导出…"，那是解释的正文，不是悬空引用）。

### 重要（已修）5：`r.reason` 生产侧零消费者
R04① 宣称的第二条收益"把为什么降级带出来"当时只活在测试里。
已在 `renderLaunchCommand` 补一行 `console.debug`。**刻意不用 toast 或 `console.warn`**：
走兜底渲染器是**正常且预期**的路径（没装 ccm 的用户每次拉起都走它），对正常行为报警是净噪音。

### 建议（已落实）
- **tsconfig 盲区当场关掉**：审计实测 `include: ["src","e2e"]` → `tsc` **0 错**，立刻可用。
  已改，并验证盲区真的关了：把 emit 脚本的 import 改错 → `tsc` 现在**报错**
  （正是本轮那个漏过去的错误类型）。
- **R04② 是缩小而非消除双机制**，已在 `CLI_REQUIRED_CAPS` 注释里如实写明残留边界
  （`new`/`resume`/`attach` 三个动作能力仍被无条件全要求，一次调用只用一个；
  因 `shared/ccm` 从 F02 首版就三动作齐全，实际不可达，故不额外收窄）。
- `launch-dimensions.test.ts` 那条 `!("keys" in o)` 是**空断言**（加 `keys` 会被 excess property
  check 拦在编译期，构造不出类型安全的变异）。已就地标注"文档性断言，不计入守护"。

### 审计独立确认为真的部分（证据比我自己的更强）
- **推论④ 零破坏**：它没读我的 diff，而是 `git archive HEAD` 导出快照 + 自写两份 emit 脚本
  （不 import 项目任何测试），跑 **720 场景**（3 account × 5 container × 3 action × 有无 model ×
  有无 ccmSid × 有无 cwd × 2 launcher）× 5 个探针 → `renderFallback` 输出**差异 0 条**。
- **CLI 侧命令串零变化**：480 处判定差异里"两边都渲染出 CLI 但串不同"= **0**、
  "旧能 CLI、新不能"= **0**（覆盖面只放宽不收窄）；放宽的 480 条**全部**是 `action==="attach"`。
- **R04④「铁律一」成立**：闭包版的生产者可以写出不带 `exec` 的包裹
  （`(inner) => "( X; " + inner + " )"`），纯数据版做不到——`exec` 由渲染器无条件吐。
- **R04② 在 new/resume 路径上真等价且有守护**：删 `MODEL_DIMENSION.requiredCaps` → 既有 F08
  测试转红。特判→维度的迁移不是空转。
- **R04③ 的 rename 无外部影响面**：`EnvOp` 全仓唯一消费者是 `renderEnvOps`，
  不跨 IPC、不落盘、无 wire format。
- **attach 放宽方向正确**（同我的判断），但必须配豁免文档 + 三条测试。
- 两个实现期发现都被诚实记进了 §3.1，没有藏——这点它主动指出。

### 审计自己的方法论自查（值得记）
它第一次的变异**未生效**——`replace(..., 1)` 命中的是注释里的同一串而非代码行，三套件全绿；
它因"预期该红却全绿"回头核对才发现。据此给出一条纪律：
**变异后先 `diff` 打出实际改动行再判色**。这与 R02 记的"变异不可达"同族，已并入纪律清单。

## 6. 工程审计结果（Phase E）

- **主计划是否仍自洽**：是。账本双渲染器那一行需更新（Phase F）：
  `canRenderCli`/`renderCli` 已合成 `tryRenderCli`，且 attach 分支有显式豁免。
- **是否引入拖累后续功能的耦合**：没有。反而给 B02 铺好了路——B02 要给 `ccm` 加
  `--bus-id` 新维度，R04② 的 `requiredCaps` 正是它声明"需要远端 ccm 支持 bus-id"的落点，
  且 `applies` 必须条件式（只 codex）这件事现在有 §37 + R04② 两处依据。
- **是否有应现在就做的统一重构**：`wrap` 字段的去留（审计建议 10：要么修对折叠点、要么删字段，
  保留一个表达不了唯一用例的字段 + 一条钉死坏形态的测试比两个极端都差）。
  **已选"修对折叠点"**——因为需求本身（rbind 必须与 exec 同一子 shell）是审计 C1 三方独立
  证成过的真需求，不是投机设计；且修完之后 §39「铁律二」那句"已知唯一用例能表达"第一次成为真话。
- **工程健康度**：tsc 0（**现含 `e2e/`**）/ npm test 701 / coverage 地板过 / `vite build` 出产物 /
  `npm audit --omit=dev --audit-level=high` 过 / `shellcheck --severity=error` 过 /
  7 套真机 e2e 131 条全绿 / cargo 不受影响（纯 TS + 一处 Rust 注释）。

## 7. 签收
- [x] 通过代码审计（1 阻塞 + 5 重要全部修完，每条各自复跑变异确认转红）
- [x] 通过工程审计
- [x] 主计划已据此更新（含变更记录）
