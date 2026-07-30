# 主计划 / MASTERPLAN — rust-ts-boundary（Rust↔TS 边界从人工纪律改成生成物）

> 所有功能宏观设计的**单一事实来源**。跨功能的任何决策以此为准。
> 每次修订都在末尾「§7 变更记录」追加一行。
>
> **状态：主计划用户 2026-07-29 已批准。技术选型于 C01 开工前实测订正，见 §8。**
> **这是路线图第 ① 项，也是 ③（门禁）与 ④（Linux）的共同地基。**

---

## §0.0 当前事实

**这条边界今天靠人工纪律维持，而纪律今天确实守住了**（Phase G 代码工程视角实测）：

| 项 | 实测 |
|---|---|
| Tauri 命令名 | **119 个**。声明数 = 注册数 = TS 引用数，**双向 diff 空** —— 今天零漂移 |
| 唯一的契约门禁 | `src/settings/cc-bus-hooks-section.vitest.ts:317-340` 的单文件白名单，覆盖 **3/119 个命令、1/29 个文件** |
| 类型 | **128 个 Rust struct ↔ 209 个 TS type**，**22 个文件**靠 `#[serde(rename_all="camelCase")]` 手工改名 |
| codegen | **无 specta / ts-rs / typeshare，零生成物** |
| invoke 调用面 | **29 个 TS 文件各自 `import { invoke }`**，无类型化包装层 |
| 依赖现状 | `tauri = "2"`，无任何 codegen 相关 crate |

**已知的具体重复与损失**：

- `sftp_pool.rs:33-42` ↔ `src/sftp/paths.ts:6-13`（6 字段）——**`u64 → number` 是静默有损**
  （JS `number` 是 f64，安全整数上限 2^53-1）
- `accounts.rs` 6 个 struct ↔ `src/accounts.ts` 6 个 type ——**连中文注释都是拷贝的**
- `bridge.rs` 10 个事件 payload ↔ `src/events.ts` 手抄

**事件半边比命令半边纪律好**（这条决定了迁移顺序，见 §4）：10 个事件名集中在
`bridge.rs:11-51` 的常量里，TS 侧每个字面量在生产代码中**恰好出现一次**，且基本都在
`src/events.ts` 这个**单一订阅枢纽**。命令半边缺的正是这个形状——29 个文件各自 `invoke`。

**为什么现在做**：

1. 它是 **④ Linux 平台**的地基。§40 那条约束要让 `transport:{kind:"local"}` 真正有含义，
   IR 类型会在边界上来回穿；边界没有生成物，Linux 那批改动就是在手抄面上再加一层手抄。
2. 它是 **aterm 联调**的地基。跟另一个仓谈「你消费这个生成的类型」远比谈
   「你照着这段文档手写」可靠。
3. 它是**「要不要用 Rust 重写前端」这个问题的真正答案**。Rust UI 唯一真实的收益就是消灭这条
   边界；codegen 能拿到其中八成，代价是几天而不是几个月，而且**不用作废 29300 行生产 TS +
   14128 行有牙的测试**。做完这一项，重写的收益少一半——那时再谈重写才谈得清。

---

## §0.1 目标与范围

- **总体目标**：把「Rust 与 TS 两侧的类型/命令签名保持一致」从**人工纪律**变成**生成物 + 门禁**。
  今天没漂移是因为有人一直在盯；目标是让**不盯也不会漂**。

- **设计原则**：
  1. **生成物是单向的**：Rust 是源，TS 是产物。**绝不反向**（不从 TS 生成 Rust）。
  2. **部分生成比全手写更坏**。如果一半类型是生成的、一半是手写的，而看代码分辨不出来，
     那比统一手写更危险。所以要么整条边界迁完，要么用门禁保证
     **「不许新增手写的跨边界类型」**（见 F05）。
  3. **静默有损必须变成显式决定**。`u64 → number` 不许再悄悄发生（见 F03）。
  4. **不改运行时行为**。本工作区是纯类型层工作，任何一次 commit 都应该
     「行为逐字节不变」——判据是 8 套真机套件 152 条断言全绿。

- **范围内**：**`ts-rs` v12** 接入（选型订正见 §8）· 生成 TS 类型 · 类型化 `invoke` 包装层（手写、由钉死 119 命令的守卫兜）·
  128 个 struct 迁移 · 10 个事件 payload 迁移 · 大整数类型的显式处置 ·
  CI 门禁「生成物是最新的」

- **范围外**：
  - **不重写前端**（见 §0.0 第 3 条）
  - 不改任何命令的**行为**或参数语义
  - 不动 `shared/ccm`、不动 daemon
  - 不合并/重命名现有命令（119 个命令名一个都不改——改名会同时动两侧，风险与收益不匹配）

- **整体成功标准**：
  1. **删掉一个 Rust struct 字段，`npx tsc --noEmit` 报错。** 今天不会。
  2. **改一个 Rust 命令的参数类型，`tsc` 报错。** 今天不会。
  3. `git diff --exit-code` 在重新生成后为空，**且这条进 CI**（防「改了 Rust 忘了重生成」）。
  4. 全仓 `import { invoke }` 的直接调用点从 **29 个文件**降到
     **1 个（包装层）+ `tabs.ts`（等授权）+ `accounts.ts`（等 `account-zero` 的 Z02）= 3**。
     **C04d 批 6a/6c 两次改写了这一条**，现在的数字是实测的、不是估的：
     - **原文写「+ 3 个动态派发口」——那是错的**。C04d 批 6a/6b/6c 逐个查实：
       那 3 处「动态」**从来不是任意字符串**（两处是 `origin ? "A" : "B"` 的两字面量三元、
       一处是 `doWrite(cmd, args)` 而调用方传的全是字面量）⇒ 全部改成静态调用/thunk，
       **`invokeDynamic` 逃生口没有做**（为一件其实是静态的事加 `string` 键后门 =
       亲手造一个守卫扫不到的洞）。**连带把 C04a 记的「7 个命令 TS 静态看不见」盲区清零**
       ——TS 字面量命令名 112 → **119**，`DYNAMIC_ONLY` 现在 `toEqual([])`。
     - **原文漏了 `accounts.ts`**。它被主计划 §3 的**跨工作区冲突协议**挡住
       （`account-zero` 优先，本区必须排在 Z02 之后；Z01/Z02 卡外部授权）
       ——这与 `tabs.ts` 红线是**两条独立的阻塞原因**，别混。
     **度量已机检**：`generated-boundary-guard.vitest.ts` 用等号钉住「直接 import invoke 的
     生产文件数」，C04d 每批必须让它降一次（29→23→19→14→12→10→8→7→6→5→**4**，
     批 7 做完 `panorama/api.ts` 到 **3** 即达成）。
  5. 大整数字段在 TS 侧的类型**不再是 `number`**，或者有一条写明「为什么这里 f64 够用」的记录。
  6. 8 套真机套件 152 条断言全绿、`cargo test` / `npm test` 数字不降。

---

## §1 功能清单

| ID | 功能 | 一句话目标 | 状态 | 依赖 | 优先级 |
|----|------|-----------|------|------|--------|
| C01 | **样板：一条命令走通全链** | 接 **`ts-rs` v12**，选**一个**低风险命令生成类型 + 类型化 `invoke`，跑通「改 Rust → tsc 报错」 | **完成**（`20c9dd7` + `8abd489` 审计闭环） | — | **P0** |
| C02 | **事件半边先迁完** | **8 个**事件 payload + `TaskEntry` → 生成；`events.ts` / `remote-health.ts` / `tasks-panel.ts` / `main.ts` 改成消费生成物；新增事件名钉死守卫。**`JsonlLine`/`JsonlBatch` 延后**（卡 C03 的 `seq: u64`，理由见 C02 §2 订正块） | **完成**（`682d5a5` + 审计闭环） | C01 | P0 |
| C03 | **大整数的显式处置** | 策略=默认 `number` 但绝不许是 `ts-rs` 的默认 `bigint`；每个大整数字段必须配 `#[ts(type = …)]`，守卫**打在源上**。三个应用面：`SftpEntry.size`（唯一已确认的静默有损点）· `TransferProgress`（2）· `UsageTotals`（4） | **完成** | C01 | P0 |
| **C04 → 已拆成四个** | ~~命令半边全量迁移~~ | **Phase B 结论：拆开**（见 `features/C04-command-half.md`）。原范围是 **63 个待生成 struct + 119 个命令签名 + 161 个调用点**——比 C01+C02+C03 加起来还大，违反 planned-build 自己的粒度准则；且成功标准「29 文件→1」**不松 `tabs.ts` 红线结构性达不到**（该文件 15 处调用点） | **已拆** | — | — |
| C04a | **类型化 `invoke` 包装层 + 钉死 119 个命令** | 建机制：手写包装层（`ts-rs` 不生成命令签名）+ 把只覆盖 3/119 的白名单守卫扩到 119；先迁一个模块做样板 | **完成**（Phase D 两份审计闭环：3 阻塞 + 6 重要 + 11 建议全部处置） | C01 | P1 |
| C04b | **两处已登记的内联字面量** | `main.ts:744` 的 `ActiveSessionPayload`（**已做**）+ `tabs.ts:1632` 的 `SessionActivityPayload`（**已跳过，等 `tabs.ts` 红线授权**——实测是**一行改动**，类型已生成好、字段逐字节一致，零技术障碍） | **完成（一半，另一半等授权）** | C04a | P1 |
| C04c | **JSONL 边界（含 `JsonlRecord` 本体）** | **Phase B 修订了处置**：不用逃生口，**直接生成 `JsonlRecord`**（它就是线定义）+ `ApiMessage`/`Usage`/`ForkedFrom` + 两个 payload = 6 个生成物。原计划以为是「渲染层重构」，实测只有 6 个 tsc 错（4 机械 + 2 处 `null` vs `undefined`），一行 `tabs.ts` 都不用碰。4 处缺口**换成生成物后自动消失**，不是登记不修 | **完成** | C03 | P2 |
| C04d | **按模块分批迁移剩余 63 个 struct + 161 个调用点** | 每批一模块、一 commit、一次全门禁；`import { invoke }` 文件数逐批下降 | 待规划 | C04a | P2 |
| C05 | **门禁** | CI 检查生成物最新（重新生成后 `git diff --exit-code` 空）+ 禁止新增手写跨边界类型 | **完成**（`f7dd23c`） | C02 | P1 |

**为什么 C01 只做一条命令**：这一整轮会话反复固化的一条纪律是
**「抽象之前先数清有几个真实使用者」**，而这里的对偶是**「大规模机械迁移之前先证明这条链真的会红」**。
C01 的验收不是「跑通了」，是**变异验收**：删一个 Rust 字段，`tsc` 必须报错；
不报错就说明生成物没被真正消费，那 119 个命令迁过去也是白迁。

---

## §2 架构概览

```
src-tauri/src/**.rs   ──(ts-rs 派生 + cargo test 导出)──▶  生成的 TS 类型（+ 手写 invoke 包装）
   ↑ 唯一的源                                   ↓ 唯一的消费入口
   │                                    src/generated/bindings.ts（只读、不许手改）
   │                                            ↓
   └── 行为不变，只加派生宏          src/**.ts 各处 import 生成的类型与函数
```

- **生成物落点**：`src/generated/`（新目录）。**头部标记不是 `// @generated`**——`ts-rs` 的头是硬编码的 `// This file was generated by [ts-rs]… Do not edit this file manually.`，改不了。守卫按那句原话断言（C01 Phase D 审计 S6 指出原文与实现矛盾，已改准）。
  **不进 `src/` 的既有目录**，否则和手写代码混在一起分辨不出。
- **类型化 `invoke` 包装**：生成的 `commands.xxx(args)` 取代裸 `invoke("xxx", args)`。
  这是「29 个文件 → 1 个」的机制。
- **`camelCase` 的处置**：`#[serde(rename_all="camelCase")]` 保留（它是运行时契约的一部分），
  而 **`ts-rs` 认 serde 属性**——这正是选它而不选 `typeshare` 的主要理由。
  **C01 的验收必须包含一条 camelCase 字段的对拍**：生成物里的字段名要是 camelCase 而不是 snake_case。

---

## §3 ★共享面账本

| 共享面 | 涉及功能 | 最终形态设计 | 当前状态 | 备注 |
|---|---|---|---|---|
| **1. `src/events.ts` + `src/remote-health.ts` + `src/tasks-panel.ts` + `src/main.ts`** | C02,C04b,C05 | 单一订阅枢纽保留（这个形状是对的），**C02 交付 9 个**生成类型（8 payload + `TaskEntry`）；`JsonlLine`/`JsonlBatch` 延后。**事件名常量不生成**（`ts-rs` 只生成类型、不生成 `const`）——改为由**钉死 10 个名字的结构性守卫**对拍，形状照 `every_host_declaration_is_pinned` | **C02 交付 9 个；C04b 加 `ActiveSessionPayload`（第 15 个）；C04c 再加 `JsonlLinePayload`/`JsonlBatchPayload` + 4 个传递依赖（→ 21 个）⇒ 事件半边的「手抄镜像全部由生成物取代」基本达成** | **C02 Phase B 实测订正两处**：① 原文写「事件名常量也从 `bridge.rs` 生成」——`ts-rs` 做不到；② 原文只写 `events.ts`，**漏了 `src/remote-health.ts`**（`RemoteHealthPayload` 的手写版在那儿）。另：payload struct 实为 **11 个**（含方向相反的 `FrontendReadyPayload`，`Deserialize`），不是 10 个 |
| **2. `src/sftp/paths.ts` + `src/sftp/panel.ts` + `src/views/usage-pivot.ts`** | C03,C04 | 三处手写镜像全部由生成物取代；`u64` 一律 `#[ts(type = "number")]` + **按量纲分开算**的上限论证（字节数 8 PB / token 量 9 亿天，不套用同一条） | **已完成**（C03） | 这是**唯一已确认的数据损失点**，所以它是 C03 的第一批 |
| **3. `src/accounts.ts` 6 个 type** | C04 | 由生成物取代；**中文注释也应该从 Rust 侧 doc comment 生成**（今天是拷贝的） | 手写，注释都是拷贝的 | 与 `account-zero` 工作区**同时在改** ⇒ 见下方冲突协议 |
| **4. IR 类型 `src/launch-plan.ts`** | C04 | `LaunchPlan`/`LaunchContext`/`LaunchAccount`/`EnvOp`/`WrapSpec` —— **这几个今天只活在 TS 侧**（Rust 不认识它们）。**本工作区不动它们**：它们不是跨边界类型，不该为了统一而强行搬去 Rust | 纯 TS | **重要判断**：IR 是前端的意图模型，Rust 侧只收渲染好的命令串。**别把它拖过边界。**`account-zero` Z02 与 `local-as-remote` 都要改这些类型，本工作区**不插手** |
| **5. `lib.rs` 的 `invoke_handler`（119 项）** | C01,C04 | **保持手写**（`ts-rs` 不管命令签名），由**钉死全部 119 项的结构性守卫**对拍——把今天覆盖 3/119 的白名单测试扩到 119，形状照 `every_host_declaration_is_pinned` + `structural_scan::require` | 手写列表，`lib.rs:894-1032`（**C04a 已配 119/119 守卫**，且守卫不认行锚——实测 `cargo fmt` 不进 `generate_handler!`，同行/漏尾逗号都是 fmt 合法的） | `lib.rs` 已经同时是顶和底（Phase G 整体设计视角重要 7：29 处上行引用）。**本工作区不解决那个问题**，只保证列表不漂 |
| **7. `src/ipc/commands.ts`（类型化 `invoke` 包装层）** | C04a,C04b,C04c,C04d | **扁平的 `命令名 → 函数` 映射，四条形状约束**：① 不许按模块嵌套（`commands.sftp.delete`）、不许塞非命令键——动态派发等逃生口必须是**另一个导出**（塞了会被守卫第 2 条抓红，这个 fail-safe 是刻意的）；② **命令名在每个条目里只出现一次的语义**由守卫兑现（键名 == 传给 `invoke` 的字面量，机检）；③ 返回类型按 §5 三桶规则；④ 每加一条就把对应模块的裸 `invoke` 换掉，不许两条路并存 | **C04a 已建，覆盖 1/119**；键↔字面量对拍已机检 | **C04a Phase D 审计的阻塞项就出在这里**：键不动、只把字面量抄成另一个**真实存在**的命令（`"open_log_file"`，它返回 `Result<(), String>`），`tsc` 0 错、10 条守卫全绿，运行时消费方拿到 `null` 直接崩。<br>**留给 C04d 的一个按数字决策**：审计实测过「单写形状」（`SPECS` + 映射类型，命令名只出现一次）**tsc 更省**——119 条时 24533 实例化 / 0.79s vs 现形状 49781 / 1.34s（`as const` 本身不产生额外实例化，两者相同）。C04a **没有采纳**，理由是今天只有 1 条命令，为 119 条假想条目引入映射类型 + `as Api` 断言（编译器查不了的谎）属于「为假想消费者建抽象」；守卫已把重复写法变安全 ⇒ 无正确性债，只剩人机效率债。**手抄条目真的变多时按这组数字换形状。** |
| **6. CI（`ci.yml`）** | C05 | 新增一步「重新生成 + `git diff --exit-code`」。**必须在 windows job 之外的某个 job 里**（生成是平台无关的，别占 Windows 配额） | 4 个 windows job + 4 个 ubuntu job | 与 `gate-integrity` 工作区**同时在改 `ci.yml`** ⇒ 见下方冲突协议 |

> **共享面 6 的协议按实际开工顺序调整（2026-07-29）**：原文「`gate-integrity` 最先改 `ci.yml`」，
> 但它还没开工而 C05 已被提到执行顺序 #2 ⇒ **C05 先改，且只在 `rust` job 末尾追加一步、
> 不重排任何既有步骤**。后到的 `gate-integrity` / `local-as-remote` 同样只追加。
> **协议实质不变**：后到者不重排先到者。


### 跨工作区冲突协议（本轮四个工作区并行的前提）

| 文件 | 谁改 | 协议 |
|---|---|---|
| `src/accounts.ts` | 本区 C04 · `account-zero` Z01/Z02 | **`account-zero` 优先**。本区 C04 迁移 `accounts.ts` 必须排在 `account-zero` Z02 之后，否则会在一个正在变形的类型上做机械迁移 |
| `ci.yml` | 本区 C05 · `gate-integrity` 全部 · `local-as-remote` | **`gate-integrity` 优先**（它就是干这个的）。本区 C05 与 `local-as-remote` 的 CI 改动都在它之后追加 |
| `src/launch-plan.ts` 等 IR 类型 | `account-zero` Z02 · `local-as-remote` | **本区不碰**（共享面 4） |

---

## §4 依赖图与实现顺序

```
C01（样板 + 变异验收）
  ├── C02（事件半边）──── C05（门禁）
  └── C03（大整数策略）── C04（命令半边全量）
```

**顺序与理由**：

1. **C01 先做，且只做一条命令。** 它要回答的是「这条链到底会不会红」。答案是「不会」的话，
   整个工作区的前提就没了——这比迁完 119 个再发现好得多。
2. **C02 次之，因为事件半边已经是单一枢纽。** Phase G 实测：10 个事件名集中在常量里，
   TS 侧每个字面量恰好出现一次。改动面最小、信噪比最高。
3. **C03 在 C04 之前。** 大整数策略必须先定，否则 C04 会把 128 个 struct 里所有
   `u64` 都机械地生成成 `number`，**把一个已知的数据损失批量固化下来**。
4. **C04 最后，分批做**（按模块，每批一个 commit + 一次全门禁）。它是唯一的大体量机械工作。
5. **C05 在 C02 之后就可以上**，不必等 C04 —— 早上门禁，后面的批次自动受保护。

---

## §5 横切关注点与约定

- 不用 emoji · commit 不加 `Co-Authored-By` · `git add` 显式文件清单。
- **门禁基线**（本工作区开工时）：`cargo test --all` **536** · `cargo test -p code-picture-core` **25** ·
  `npm test` **814 / 53 files** · **clippy 无新增**（实测基线 lib 36 + lib-test 44 warnings，CI 里 advisory 不阻断；写「clippy 0」会误导，审计 S7）· tsc 0 · `npm audit --omit=dev --audit-level=high` rc=0 ·
  shellcheck 0 · `bash e2e/exec-bit-guard.sh` rc=0 · **8 套真机套件 26/44/12/15/13/21/14/7 = 152 条**。
- **本工作区每个 commit 的额外硬判据**：**行为逐字节不变**。
  纯类型层改动若让任何一条真机断言变了，说明它不是纯类型层改动，停下来查。
- **生成物不进人工审阅的注意力**：靠 ts-rs 自带的「Do not edit」头 + `.gitattributes` 的 `linguist-generated=true`
  （让 diff 默认折叠）——**已落地**（C01）。原文在这里把同一件事说了两遍、其中一遍写成「并考虑」，
  已删（C04a Phase D 审计 J10）。`.gitattributes` 那里的盲区说明是准的：
  **手改生成物不由 vitest 守卫抓，由 C05 的 CI 门禁抓。**
  **但生成物必须进 git**（否则 CI 与本地会分叉，且 `git diff --exit-code` 那条门禁无从立足）。
- **★ 成文规则（C04a 立，119 个命令一律照它办）：名字钉死是普遍的，类型生成是按需的。**

  两件事必须分开：
  1. **命令名** —— **119/119 全部纳入守卫**。名字错了是运行时必错（`invoke` 直接 reject），
     与有没有人用返回值无关。
  2. **返回类型** —— **只在 TS 侧真的消费字段时才生成**。TS 侧忽略返回值（裸 `invoke("x", …)`
     不带类型参数、或只 `await` 不读字段）的命令，**不为它生成类型**。

  **理由**（C03 用 `SftpStat` 立的先例）：给一个没人消费的返回值生成类型，就是
  「为假想消费者建抽象」——本工作区已经三次拒绝这种做法（`ProbeStatus` 删掉、
  `--bus-id` 加了又删、`SftpStat` 跳过）。而**名字**不同：它没有「消费者」这个概念，
  拼错就是必错，所以必须全覆盖。

  **落地形态：返回类型分三桶**（**C04a Phase D 审计 Z2 订正——原来只写两桶，是错的**）。
  原文说「该是 `unknown` 的写 `unknown`」，实测分桶后发现那会把 **34 个**返回 `()` 的命令
  从诚实的 `void` 退化成 `unknown`：

  | 桶 | 判据 | 该写的类型 | 条数（2026-07-29 实测） |
  |---|---|---|---|
  | ① | Rust 返回 `()` 或 `Result<(), _>` | `Promise<void>` | **34** |
  | ② | 有 payload，但 TS 侧不读任何字段 | `unknown` **+ 那一行注明「TS 侧不读字段」** | **4** |
  | ③ | TS 侧真消费字段 | 生成物类型 | **81** |

  桶 ② 的四个是 `sftp_stat`（C03 已按此跳过，先例就是它）· `rebuild_search_index` ·
  `start_forward` · `aggregate_usage_all`。

  **为什么少一桶是净退化**：今天已有 **11 处**显式写着 `invoke<void>(…)`。把它们改成 `unknown`
  会让「有人开始读一个本来没有 payload 的命令的字段」不再报错 —— 那正是本工作区要建立的
  「改 Rust → `tsc` 报错」的反面。
  **`unknown` 桶只占 3%（4/119）**，所以规则不会退化成大片 `unknown`，价值站得住。

  **第四类：动态命令名（7 个）**不进包装层的扁平表——它们经 `invoke(cmd, args)` 这种
  运行时决定名字的路走（`sftp/panel.ts:485` · `views/session-viewer.ts:211` · `views/history.ts:489`），
  逃生口必须是**另一个导出**，不许塞进 `commands` 对象（塞了会被守卫抓红，那是 fail-safe）。
  其中 3 个返回 `Result<(), _>`，按桶 ① 处置。

- **测试纪律**（逐条适用，本会话固化）：变异**先 diff 确认落位、再确认它编译得过**，然后才判色 ·
  反向自检 · 计数自检用 `==` 不用 `>=` · **守卫范围要恰好等于性质范围**（栽过三次）·
  **源码文本扫描 ≠ 行为测试** · commit message 里每句「已有测试守着」先跑变异证明。

---

## §6 风险与开放问题

**风险**

1. **~~`tauri-specta` v2 的成熟度未实测~~ —— 已实测并因此换掉，见 §8。`ts-rs` v12 的边角情况仍未实测。** 它对 Tauri 2 是一等支持，但本仓有 119 个命令、
   128 个 struct、22 个文件手工 camelCase，**边角情况一定会撞到**
   （`Result<T, String>` 的映射、`Option<T>` 的 `null` vs `undefined`、
   `#[serde(untagged)]`/`flatten`、异步命令、`State<'_, T>` 参数）。
   **缓解**：C01 只做一条命令就是为了先撞一遍。
2. **`Result<_, String>` 的错误类型。** 本仓几乎所有命令返回 `Result<T, String>`，
   TS 侧靠 `try/catch` 拿字符串。specta 可能把它生成成一个 `Result` 联合类型，
   **那会改变 29 个文件的调用形状** —— 那就不是纯类型层改动了。
   **C01 必须明确这一条，若形状会变，则需要一个「保持 throw 语义」的包装层。**
3. **大整数策略有三个选项，都有代价**：`bigint`（TS 侧要改算术与序列化）·
   `string`（安全但要处处转换）· 保留 `number` 并写下上限论证（`sftp_pool` 那 6 个字段
   是文件大小/时间戳，2^53 字节 = 8 PB，实际够用）。**C03 要在这三个里选，并写下理由。**
4. **生成物的 churn**：128 个 struct 一旦生成，任何 Rust 侧改动都会带一个生成物 diff。
   若 review 时被当噪音跳过，就白做了。**缓解**：`linguist-generated` + 门禁保证它必然最新。
5. **`lib.rs` 已经很脆**（同时是顶和底，29 处上行引用）。C04 会大面积碰它的 `invoke_handler`。
   **本工作区不试图修那个架构问题**，但要小心不把它弄更糟。

**待用户确认的开放问题**

| # | 问题 | 我的建议 |
|---|---|---|
| 1 | C01 撞到「`Result<_,String>` 的形状会变」时怎么办？ | **加一层保持 throw 语义的包装**，不改 29 个文件的调用形状。理由：本工作区的硬判据是行为不变 |
| 2 | 大整数选哪个？ | 建议 **`sftp_pool` 那批保留 `number` 但写下上限论证**（文件大小/时间戳，2^53 够用），**其余一律 `bigint` 或 `string`**。理由：不为一个够用的地方付全局代价，但要**显式记档**而不是继续静默 |
| 3 | C04 要不要真的迁完 128 个？ | 建议**迁完**，但允许分批。理由见 §0.1 原则 2：部分生成 + 看不出来 = 比统一手写更坏。若不迁完，C05 的门禁必须能区分「已迁」和「未迁」 |
| 4 | 生成物进 git 吗？ | **进**。否则 CI 与本地分叉，且「重新生成后 diff 为空」这条门禁立不住 |

---

## §8 技术选型订正：用 `ts-rs` v12，不用 `tauri-specta` rc（2026-07-29，C01 开工前实测）

**主计划初版写的是「接 `tauri-specta`」。实测后改掉。**

| crate | 实测可解析版本 | 结论 |
|---|---|---|
| `tauri-specta` | **只有 `2.0.0-rc.1`**（`cargo add tauri-specta` 解析出的 v1.0.2 是 **Tauri 1** 的版本；`tauri-specta@2` 在 registry 里查不到） | **不用**：本仓 `tauri 2.11.2` 是生产应用（Windows 打包发版），**不给它引入预发布依赖** |
| `ts-rs` | **v12.0.1，稳定** | **采用** |
| `typeshare` | v1.0.5，稳定 | 备选，未采用（理由见下） |
| `specta`（单独） | v1.0.5 稳定，但那是 Tauri-1 时代的；specta v2 同样只有 rc | 不用 |

**为什么 `ts-rs` 而不是 `typeshare`**：
`ts-rs` 是 derive 宏（`#[derive(TS)]`），**认 serde 属性**——包括本仓 22 个文件在用的
`#[serde(rename_all="camelCase")]`，这正是要点。`typeshare` 是独立 CLI 解析源码，
对泛型与复杂类型更受限。而且 **`ts-rs` 的导出走 `cargo test`**（每个类型一条 export 测试），
**与本仓「门禁 = 测试」的既有文化一致**。它还有 `#[ts(type = "…")]` 逃生口，
正好给 C03 的大整数决策用。

### 这个选择的代价，说清楚

`ts-rs` 只生成**类型**，**不生成带每命令签名的 `invoke` 包装**（那是 `tauri-specta` 的功能）。
所以 §0.1 的成功标准要分开看：

| 成功标准 | `ts-rs` 能不能达成 |
|---|---|
| ① 删一个 Rust struct 字段，`tsc` 报错 | **能**（类型是生成的） |
| ② 改一个命令的**参数类型**，`tsc` 报错 | **能**，前提是参数也走 `#[derive(TS)]` 的结构体或生成的类型 |
| ③ 重新生成后 `git diff --exit-code` 空且进 CI | **能** |
| ④ 29 个 `import { invoke }` 收成 1 | **能**，但那一个包装层**是手写的** |
| ⑤ 大整数不再是 `number` | **能**（`#[ts(type=…)]`） |

**手写包装层的漂移风险由「钉死全部 119 个命令」的结构性守卫兜**——把今天那条只覆盖
3/119 的白名单测试（`src/settings/cc-bus-hooks-section.vitest.ts:317-340`）**扩到 119**，
形状照 `config_surface.rs::every_host_declaration_is_pinned`（T02 建立）+
`structural_scan::require(min_checked)`（计数自检，`0` 直接硬失败）。

**这其实更贴本仓的文化**：这个仓通篇不是「相信框架」，是「钉死的表 + 计数自检 + 变异验收」。
把命令签名交给一个 rc 阶段的框架去保证，反而比钉死一张表更不可靠。

**若用户希望改用 `tauri-specta` rc**（省掉手写包装层与那张表），这是一个可逆决定——
但要接受一个预发布依赖进生产打包链。**默认按上表执行。**

## §7 变更记录

- 08 — 2026-07-29 — **C04 拆成 C04a/b/c/d**（Phase B 结论）— 重取承重数字后发现原范围（63 待生成 struct + 119 命令 + 161 调用点）比 C01+C02+C03 加起来还大，违反粒度准则；且成功标准 4「29 文件→1」**不松 `tabs.ts` 红线（15 处调用点）结构性达不到**，已如实改写不假装达成。订正一处口径：Phase G 记的「128 个 Rust struct」实为 `pub struct` 总数口径，**真跨边界的 `Serialize` struct 是 77 个**（已生成 14 ⇒ 剩 63）。另记一条我自己制造又收回的假警报：一度量出「7 个命令声明未注册」，实为顶层 glob 漏子目录 + 正则撞上注释里提到的 `#[tauri::command]`；剥注释 + 递归全仓后 **119 = 119、两边差集都空**，Phase G 那句成立。
- 07 — 2026-07-29 — **C03 落地：大整数策略 + 第二条源上守卫** — 盘点发现已派生集合里只有 1 个大整数字段且已处置 ⇒ 规则若只覆盖它会平凡通过，故给它找了三个真实应用面（SftpEntry.size 是 Phase G 报的唯一已确认静默有损点 · TransferProgress 2 个 · UsageTotals 4 个）。**刻意的窄越界**：三者都属命令半边（协议归 C04），理由是策略需 ≥2 个应用面才成为规则。跳过 SftpStat（TS 侧裸 invoke 无类型参数、字段没人用 ⇒ 为假想消费者建抽象）。上限论证**按量纲分开算**。实证了「不能断言生成物不含 bigint」——我自己用裸 grep 复现了那个假阳性（全在 JSDoc 散文里）。
- 06 — 2026-07-29 — **C02 落地 + 审计闭环**（`682d5a5` + 修复）— 8 个事件 payload + `TaskEntry` 改生成物；新增事件名钉死守卫并在审计后扩到 **11** 个常量（补 `FRONTEND_READY`——C02 给它上了类型却把名字漏在门禁外）；**修掉一条阻塞**：`skip_serializing_if` 扫描的 400 字符窗口会被隔壁字段的属性喂饱（该性质在 C01 时为真、被 C02 扩到第 2 处相邻同构字段的那一刻失效），窗口收到同字段属性块；顺带治 S2（切块对属性顺序敏感 ⇒ 改以 `pub struct` 为锚往上收属性，C04 要复制 127 次）。**订正一条我写错的延后理由**：`JsonlRecord` 不是「有损模型」而**就是 wire 的定义**，真正卡点是 `seq: u64` 要先有 C03。
- 05 — 2026-07-29 — **C05 落地：门禁拆成两半，各在已有 job 里查** — 实测发现**没有任何 CI job 同时有 Rust 和 node**，所以 `npm run check:types` 那条串联进不了 CI。改为：`rust` job 加一步 `git diff --exit-code -- ../src/generated/`（保证已提交的生成物 == 从 Rust 源生成的）+ `frontend` job 既有的 `tsc`（保证 TS 消费方 == 已提交的生成物），两者合起来即「TS 消费方 == Rust 源」，而 `git diff` 不需要 node ⇒ 代价 ≈ 一条 git 命令。顺带闭合 C01 登记的「手改生成物」盲区（已提交的那种）。
- 04 — 2026-07-29 — **C01 Phase D 审计闭环：执行顺序改动 + 依赖方向改动**（见 features/C01 §7）— C05 由 #4 提到 #2（CI 两次独立 checkout ⇒「忘了重新生成」让所有门禁保持绿色，实测）；ts-rs 移到 dev-dependencies + cfg_attr(test) 派生；§3 的 @generated 与实现矛盾已改准；「clippy 0」改「clippy 无新增」。
- 03 — 2026-07-29 — **共享面 1 两处订正 + 新增一条硬性范围约束**（C02 Phase B 实测）— ① 事件名常量 `ts-rs` 生成不了，改为结构性守卫钉死；② 漏了 `src/remote-health.ts`；③ payload 实为 11 个不是 10 个；④ **线上格式是混的**（3 个 camelCase / 8 个 snake_case），生成物必须忠实复现，**统一它属于行为变化，本工作区一律不做**。
- 02 — 2026-07-29 — **技术选型从 `tauri-specta` 改为 `ts-rs` v12**（见 §8）— C01 开工前实测发现 `tauri-specta` 对 Tauri 2 只有 `2.0.0-rc.1`，不给生产打包链引入预发布依赖。代价是 `invoke` 包装层手写，由「钉死 119 个命令」的结构性守卫兜住。
- 01 — 2026-07-29 — 初版，Phase A 主规划完成 — 路线图第 ① 项。
  由 Phase G 代码工程视角的「命令名零漂移但 payload 形状零门禁」+ 整体设计视角的
  「128 struct ↔ 209 type，22 文件手工 camelCase」两条实测立项；
  同时作为「要不要 Rust 重写前端」那个问题的**便宜答案**。等用户审批。
- 08 — 2026-07-29 — **C04a 落地 + Phase D 两份审计闭环（3 阻塞 / 6 重要 / 11 建议，全部处置）** —
  三条阻塞都是「守卫自己没牙」这一类，而这正是本工作区在治的病：
  ① **包装层的键名 ↔ 传给 `invoke` 的字面量无人对拍**——变异实测：键不动、字面量抄成另一个
  **真实存在**的命令（`"open_log_file"`，返回 `Result<(), String>`），`tsc` 0 错、10 条守卫全绿，
  运行时消费方拿到 `null` 崩。**新机制原封不动地把「手写镜像静默漂移」这个病带了进来。**
  这也证明我原先声称达成的「变异 ②」没真正达成：改成**不存在**的名字确实红，
  但抓它的是「TS 全仓字面量」那条（恰好扫到了包装层这个文件），不是「包装层」那条。
  ② **§5 规则少一个桶**：两桶会把 34 个返回 `()` 的命令从 `void` 退化成 `unknown`（今天已有 11 处
  `invoke<void>`），改成三桶 + 动态名第四类。
  ③ **变异记录没落盘**，无法核实 ⇒ 已补进 feature 文件。
  六条重要里最值钱的两条：**`stripComments` 被字符串字面量骗走**
  （`config_surface.rs:775` 的 `"~/.local/*\/bin"` 里含 `/*`，非贪婪匹配吞掉 **521 行**真 Rust 代码；
  对否定式断言就是**假绿**）⇒ 重写成状态机 + 给它配了 9 条自己的测试；
  **`toBeGreaterThan(50)` 是安慰剂**（把 TS walk 缩到 3 个子目录、可见名 112→81 仍全绿）⇒ 改等号。
  另一条反直觉的实测：**`cargo fmt` 根本不进 `generate_handler!` 的内容**——两个注册项写同一行、
  或最后一项漏尾逗号，`cargo fmt --check` 都 rc=0，而带行锚的守卫会**假红**且诊断说反
  ⇒ 去掉行锚。「本仓 fmt 统一风格所以免疫」这条理由被实测推翻了。
  数字订正三处：**「161 个调用点」不成立**（生产+剥注释+按表达式 = **143**；那个 161 两种口径都复现不出）·
  「29 → 28」是假的（净持平仍 29，现已用等号机检）· 「TS 侧 211 个 type/interface」8 种口径都复现不出，删掉。
- 09 — 2026-07-29 — **C04b 落地一半（另一半如实标注等授权）** —
  `main.ts:744` 的内联字面量换成生成的 `ActiveSessionPayload`（生成物 14 → **15**）。
  **变异 A 是主计划成功标准 1 在「命令返回类型」上的首次成立**：删 `ActiveSessionPayload.cwd`
  并同步改掉 `lib.rs` 的构造点让 Rust **编译得过**（C01 就栽在「变异没编译过时 tsc 什么都不说，
  那种绿是无效结果」），`tsc` 精确报 `main.ts(746,46): Property 'cwd' does not exist`。
  **`tabs.ts:1632` 那一半已跳过，等红线授权**——本轮核实后可以把代价说得更硬：
  `list_session_activity` 的返回类型**就是** `Vec<bridge::SessionActivityPayload>`，
  而 `src/generated/SessionActivityPayload.ts` C02 时已生成、字段逐字节一致
  ⇒ **它是一行改动**（内联字面量换成 import + `SessionActivityPayload[]`），
  不需要新派生、不需要重新生成、不需要改守卫。**零技术障碍，100% 卡在红线上。**
  一条刻意不做的事：`ActiveSessionPayload` 与 `SessionActivityPayload` 在线上都是 **snake_case**
  （都没有 `rename_all = "camelCase"`），**不许顺手统一**——那是行为改动，而本工作区每个 commit
  的硬判据是行为逐字节不变。这个不一致**正是生成它的理由**：手写镜像能静默漂成 camelCase，
  生成物不会。守卫的 camelCase 断言只钉那两个确实是 camelCase 的文件（范围恰好等于性质范围），
  所以加一个 snake_case 生成物不假红——已实测确认。
  另**不把 `list_active_sessions` 加进 C04a 的包装层**：`main.ts` 有 9 个调用点，只迁 1 个既不让
  「29」降一格、又让同一文件里两条路并存（正是账本第 7 行约束 ④ 要防的）。整文件迁移归 C04d。
- 10 — 2026-07-29 — **C04c：修订原处置，直接生成 `JsonlRecord`（生成物 15 → 21）** —
  原计划要用逃生口 `#[ts(type = "import('../cards').JsonlRecord")]` 指向 TS 手写版。
  **那会让生成物指向病灶**：C02 audit I4 已订正过方向——这个边界上 **Rust 的 `JsonlRecord`
  就是线定义**（wire == `serde_json::to_string(它)`），TS 那份是更窄的手抄（**8 vs 12** 个 variant；
  我第一遍报 11 是错的，漏了 `#[serde(other)] Unknown`）。
  原计划以为「用生成物替换是一次渲染层重构」——**实测推翻**：一共 6 个 tsc 错，
  4 个机械（3 个又是 `export type {…} from` 不带名字进本地作用域，**第三次栽**）+ 2 个真错
  （都是 `string | null` vs `string | undefined`，都在消费侧修），**一行 `tabs.ts` 都不用碰**。

  **手抄版被删时暴露三处静默漂移，且都是「换成生成物后自动消失」而不是「登记不修」**：
  ① TS 给 `queue-operation` 声称了一个 Rust 根本没有的 `timestamp?: string`（线上恒 undefined）；
  ② 手抄的 `Usage` 把两个 token 字段标成 optional，而 Rust 只有 `default`、无 `skip_serializing_if`
  ⇒ 线上恒有；③ variant 8 vs 12。

  **一处教科书级的「运行时对、类型说谎」**：`cards/api-error.ts:70-72` 本来就写着
  「`typeof` 而非 `!== undefined`：serde 把 None 序列化成显式 null」并用 `typeof` 防着，
  而签名却是 `retryAttempt?: number`——**中间靠一条注释解释差异**。接上生成物后 `tsc` 当场揭穿。

  **守卫挖出并修掉两个真缺口**：① 两条通用性质扫描**只扫 `pub struct` 不扫 `pub enum`**、
  且字段正则要求 `pub ` 前缀（enum variant 字段没有），后果是 `System.duration_ms: Option<u64>`
  生成出 `bigint | null` 而守卫一声不吭——我是盯生成物发现的，不是它逼我的；
  ② **字段层属性窗口顺序敏感**（只看 serde 属性之后的行）⇒ `ts(optional)` 写在前面就假红。
  C02 审计的 S2 只修了 struct 层，字段层漏了。**新增一条守卫**：`Option<大整数>` 配 `ts(type)`
  时不许丢 `| null`（我自己踩的：`ts(type)` 覆盖整个类型、不只是 Option 内层）。

  **一条变异如实作废 + 一条更细的新教训**：「删 `api_error_status` 字段」两轮都没编译过，
  而第二轮 `tsc` 变红报的是 **`apiErrorStatusRENAMED`** ——它在读**上一次变异遗留的过期生成物**。
  ⇒ 变异链上生成物是中间产物；**变异没编译过时，不只「tsc 沉默」是假信号，「tsc 变红」也可能是**。
  判色前要确认的不只是「编译过了」，还有「**生成物是这次变异产的**」。验收由同性质且构造上
  必然编译的 A1（改 serde rename）承担。

  刻意不做：不派生 `ContentBlock`（`ApiMessage.content` 是 `serde_json::Value`、压根没引用它
  ⇒ 线上不可达，同 C03 跳过 `SftpStat`；TS 侧那个留手写，它是对 `content: unknown` 的解释模型，
  属账本第 4 行那类）· 不给 `Unknown` variant 加 `#[ts(skip)]`（生成物比线上宽一格是 fail-safe 方向，
  收窄则要求我证明它永不上线，那个证明横跨 32k 行 Rust，不做）。
