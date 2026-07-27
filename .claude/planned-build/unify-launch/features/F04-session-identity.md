# 功能计划 — F04 会话身份统一（@ccm_sid 三道门 + 根治 R10）

> 对应主计划 §1 的 F04。本文件是该功能从规划到签收的全程记录。
> **动手前先读 MASTERPLAN §0 核心思想** —— 推论⑤「身份随行」是本功能的直接落点：
> `@ccm_sid` 由 CLI 统一负责，无论从 app 起还是从终端起，cc-monitor 都能无缝识别。

## 0. 本计划的来源（Phase B 方法论说明）

按标准全自动流程，本计划出自两个独立 Plan agent 的架构方案综合：
- **方案 A（原子性优先）**：聚焦 `tmux.rs` 三道门 + `display-message` 原子 verify+act 命令构造，
  给出 4 种具体渲染出的 shell 脚本；TS 侧只做最小必要改动（`findClaudeTmuxMatches` 纯增量函数，
  `findClaudeTmux` 行为逐字节不变）；发现 `session-backend.ts:113` 兜底渲染器的 `@ccm_sid` 直写
  **不应该**跟着改名（无 poller、无提升机制）；新增 `resumingSids` 互斥（对称 `restartingSids`
  既有模式）。
- **方案 B（身份模型优先）**：核心洞见是 R10 本质是**类型层面的错误**——`findClaudeTmux` 用
  `.find()` 把「零/一/多」的真实关系压扁成「零/一」，修复应该是让类型系统本身不允许调用方漏判
  「多」这个分支；逐一分析了 6 个调用点谁真的需要富类型谁不需要；提出 `@ccm_sid_expect` 的
  "意图 vs 事实"精确定义（通道A=建时立即声明、通道B=poller 独立读会话文件后才确认）。

两版在核心机制上高度一致（三道门形状、`@ccm_sid_expect`/`@ccm_sid` 拆分方向、`TMUX_LS_FMT` 冻结
不碰、`pickFreshTmuxName`/`findIdleTmux`/`claudeExited` 核心逻辑不用变），分歧集中在**表达方式**与
**部分调用点的严重度分级**。本计划采纳的取舍见 §2。

## 1. 目标与验收标准（DoD）

- **目标**：把「一个 sid 可能同时活在 ≥2 个 tmux 里、`findClaudeTmux` 静默只挑第一个」这一结构性
  风险（R10）根治：破坏性操作（kill）必须原子验证身份与安全性，非破坏性操作至少能**发现**重复
  并诚实告知，绝不静默动作在错的目标上或制造第三个孤儿。

- **验收标准**：
  - [x] `src-tauri/src/tmux.rs`：`is_safe_tmux_target`（gate 1，恒强制，只拒空 target——不额外
        收紧字符集，见步骤1 的实现修正）+ 身份判据扩为 `@ccm_sid` 已设 ∪ `cc-*` 前缀（gate 2，
        `is_ccm_tmux_name` 不删除、降级为 OR 的一支）+ `windows==1`（gate 3，仅 kill）；三者折进
        **一条原子远端命令**（`display-message` 取 `session_windows`/`@ccm_sid`，同一 round-trip
        内判断后再执行动作）
  - [x] `capture_remote_pane` 补 gate 1（今天唯一无门的入口，空 target 会落到「当前会话」）
  - [x] `shared/ccm`：通道A（建时立即声明）两处 `set-option @ccm_sid` 改名 `@ccm_sid_expect`；
        通道B（poller 确认）仍是 `@ccm_sid` 的唯一写者；`session-backend.ts:113` 兜底渲染器的直写
        **保持不变**（无 poller、无提升机制，改名会让兜底路径创建的会话永远没有事实身份）
  - [x] `src/tabs.ts`：新增 `findClaudeTmuxMatches`（纯函数，返回全部命中，`findClaudeTmux`
        用它重实现、行为逐字节不变）；resume-attach/restart-guard/菜单-kill 三个调用点按严重度
        分级处理重复命中（见 §3.2）；新增 `resumingSids` 互斥（对称 `restartingSids`）
  - [x] 门禁：`tsc`/`npm test`（614）/`cargo test`（377）/`test:tmux-target`/`test:ccm-cli`/
        `test:ccm-acceptance`（15）/`test:ccm-print-parity`（9）全绿；**新增真机验收**
        `e2e/tmux-guarded-acceptance.sh`（`npm run test:tmux-guarded`，14 项）覆盖三道门在真
        隔离 `-L` socket 上的实际效果（原计划只写 Rust 单测，落实时按 R1 教训补上）

- **明确不做什么**：
  - **不做跨 origin 去重检测**——tab 本来就按 origin 钉死，同 sid 活在另一台机器上的场景不在
    `list_remote_tmux` 的查询范围内，结构性超出本功能边界（两版方案都指出，非选择性遗漏）。
  - **不做 UI 层的候选选择器**（多个候选如何呈现、点哪个）——留给 F09，本功能只保证数据不被
    静默丢弃。
  - **不把 `@ccm_sid_expect` 塞进 `TMUX_LS_FMT`**——守 MASTERPLAN 明确排除的 daemon 零改动范围
    （`TMUX_LS_FMT` 双写点不动）；`_expect` 只在窄场景按需查（且本轮窄场景本身也不做，见下条）。
  - **不做 `findIdleTmux` 的 `_expect` 回退**（confidence: expected-only 的空闲会话复用判据）——
    两版方案都同意这是**孤儿规避的锦上添花**，不是破坏性操作安全性的必需项（空闲占位重复的
    危害远低于活会话重复——不会静默继续跑/计费）。本次不做，留给以后按需评估。
  - **不做菜单标签展示重复数**（如「⚠+1 重复」）——纯 UI 展示决定，留给 F09。
  - **不放宽 gate 1 的字符集**（今天含空格的自定义会话名已经打不到 kill/send-keys，这是既有已知
    限制不是本次引入——放宽字符集是另一个独立决定，不顺手在 R10 修复里捎带做）。
  - 不碰 daemon；不改 `TMUX_LS_FMT` 双写点；不碰 `~/.bashrc`；不改 `cc-<sid8>` 语义。

## 2. 与主计划的对接 + 对两版方案分歧的取舍（附理由）

**触及的共享面**：`src-tauri/src/tmux.rs`、`shared/ccm`、`src/tabs.ts`、`src/session-backend.ts`
（只读确认不改）、`e2e/ccm-acceptance.sh`、`e2e/ccm-print-parity.sh`、`src-tauri/src/sftp.rs`
（needle 清单）。

**四处取舍**：

1. **原子命令用 `display-message` 而非 `show-options`**（采纳方案 A）：`show-options` 对未设置的
   option 是 `rc=1` + stderr，需要脆弱的 rc/stderr 联合判断；`display-message -p -t <target> '<fmt>'`
   是这个仓库已经验证过、已经在生产用的惯例（`TMUX_LS_FMT` 本身、`shared/ccm:340` 的 cwd 探测都是
   同一机制），未设置的 option 在格式串里静默展开成空串，用「捕获串是否为空」就能同时区分「目标
   不存在」与「目标存在但 option 未设」，不需要发明第二套判断惯例。

2. **`findClaudeTmuxMatches` 用简单数组返回（采纳方案 A）而非判别式联合类型（方案 B 提议）**：
   两版方案实际上**对哪些调用点需要富信息的结论高度一致**（真正分歧点只有"怎么表达"）。方案 A
   的路径是纯增量：`findClaudeTmux` 用 `.filter(pred)[0]` 重新实现、与今天的 `.find(pred)` 逐字节
   同结果，**21 处既有调用点/断言零改动**；只在 3 个真正需要判断"是否 >1"的调用点用
   `matches.length` 分支。方案 B 的判别式联合类型能让 TypeScript 在编译期强制穷尽处理，安全性
   稍高，但代价是新增一个跨文件导出类型、且要求所有消费方改造成 `switch(kind)`。选方案 A 是因为
   ①与 F03 对 `TmuxTarget` 的选型理由完全同构（"最小 diff 优先于结构纯粹性"，见 F03 计划 §2 第
   1 条）；②`findClaudeTmux` 的 21 个既有消费者零改动是可验证、可核实的低风险事实，不是猜测；
   ③3 个真正需要分支的调用点，`matches.length` 检查已经足够清晰可读，判别式联合的编译期穷尽保证
   在只有 3 个调用点、且这 3 处已经被本计划显式点名审查的情况下，边际收益不足以覆盖新类型的
   维护成本。

3. **`resolveAttachMenuItem`/菜单构建按 action 严重度拆分而非整体一刀切**（本计划自己的综合，
   两版方案都把这当一个整体调用点，但各自只给了一个笼统结论——A 说"可选，留给 F09"，B 说"整体
   禁用 attach/kill、换成诊断条目"）：菜单里 attach/preview/kill 三个动作严重度完全不同——
   preview 只读、attach 非破坏性（可撤销：重新点一次就能换目标）、kill 破坏性且不可逆。命中
   ≥2 时：**preview 不受影响；attach 沿用 resume 一致的"警告并按 matches[0] 继续"（见下条理由）；
   kill 沿用 restart 一致的"拒绝并报数"**。不对整个菜单做统一处理，是因为把 preview/attach 也
   一并禁用会造成不成比例的可用性损失（用户本来就是想看看/接进自己的会话，不该因为一个只影响
   kill 安全性的风险被连坐挡住）。

4. **resume-attach 命中 ≥2 时"警告并继续"，restart/kill 命中 ≥2 时"拒绝"**（综合两版的分歧：
   方案 A 认为 attach 该警告后继续（非破坏性、可逆、拒绝是可用性倒退），方案 B 认为该拒绝
   （SS-5/SS-9「找不到就报不存在，绝不静默换一个」的精神应该对称适用到"多个"的情形）——本计划
   按**动作是否破坏性**分级，而非对"命中多个"这一件事本身用同一个动词：resume-attach 到其中
   一个会话，用户可以立刻在里面看到内容、发现不对、再手动去终端核实——**代价可逆**；
   restart/kill 一旦执行，错误的那次操作**代价不可逆**（可能杀掉了对的那个、留下错的那个继续
   跑）。SS-5/SS-9 的精神是"不确定就别装作确定"，而"警告+继续"与"拒绝"都满足"不装作确定"这个
   核心要求，只是对不确定性的**反应强度**不同——反应强度应该匹配后果的可逆性，不是一刀切。
   **这条分级本身作为一个开放问题明确转发给 Phase D 的 UX agent 审计**（两版方案都各自标注了
   这是"值得被审计"的判断，本计划采纳其一但保留被推翻的空间，不假装这是无争议的定论）。

**两版方案都独立发现、且已被本计划吸收的关键约束**：
- `session-backend.ts:113` 的兜底渲染器直写 `@ccm_sid`（非 `_expect`）——它没有 poller、没有事实
  确认机制，改名会让兜底路径创建的会话**永远拿不到确认身份**，是方案 A 独立核实源码后指出、
  方案 B 未提及的真实风险，已采纳。
- `capture_remote_pane`（`tmux.rs:155-170`）是今天**唯一无门的入口**——空 target 会解析成
  「当前会话」，只读路径也不该允许，两版方案与 MASTERPLAN 账本口径一致。
- `pickFreshTmuxName`（按名字不按 sid 判撞名）、`findIdleTmux`（空闲占位重复危害低于活会话
  重复）、`claudeExited`（内部改用 `findClaudeTmuxMatches` 但对外坍缩成布尔、既有 10s 超时兜底）
  三者**核心逻辑不用变**，两版方案与既有代码逐一核对后结论一致。
- `e2e/resume-cmd-driver.ts`/`e2e/restart-cmd-driver.ts` 的位置参数签名不受影响——两者都不
  直接触碰 `findClaudeTmux`/`tmux.rs` 内部命令构造细节。

## 3. 接口 / 契约设计

### 3.1 `src-tauri/src/tmux.rs`：三道门 + 原子 verify+act

```rust
/// Gate 1（恒强制）：拒绝空 target（会解析成「当前会话」）+ 非法字符（glob/元字符）。
/// 与 is_ccm_tmux_name 共享同一字符集、少前缀要求——不引入新的"安全"字符串类别。
fn is_safe_tmux_target(target: &str) -> bool {
    !target.is_empty() && target.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// exact_target 变 fallible——gate 1 折进这一个函数，任何未来新增的 tmux 动词都不可能绕过它
/// （结构性消灭，不是"记得检查"）。
fn exact_target(target: &str) -> Result<String, String> { ... }

/// Gate 2 的本地半支（名字前缀，零 IO）：is_ccm_tmux_name 不删除，只是从"唯一判据"降级为
/// "OR 的一支"。命中即可跳过远端的 @ccm_sid 判据（该会话不管有没有 @ccm_sid 都算"我们的"）。

/// Gate 2 的远端半支（@ccm_sid 已设）+ Gate 3（仅 kill，windows==1）：折进一条 display-message
/// 原子命令，同一 round-trip 里"查完再判再动"，不给中间留可被抢跑的窗口。
/// name_owned=true（gate2 本地已过）时 kill 仍需远端拿 windows（今天没做，是真正的新增开销，
/// 但对应的是"以前完全没有 gate 3"这个真实缺口）；send-keys 在 name_owned 时零额外开销。
fn build_guarded_tmux_cmd(target: &str, need_sid: bool, need_windows: bool, action: &str) -> Result<String, String> { ... }
```

四种具体渲染形态（gate 组合 × name_owned/not）与新增 sentinel（`CCM_NO_SESSION`/
`CCM_GUARD_REJECTED sid=%s windows=%s`，与既有 `NO_TMUX` 同一体系）见 Phase B 综合来源的
方案 A 报告§4.3（实现时逐字对照，不重新发明）。

### 3.2 `src/tabs.ts`：`findClaudeTmuxMatches` + 三个调用点分级

```ts
/** 新增，纯函数，返回全部精确命中（不折叠）。 */
export function findClaudeTmuxMatches(
  sessions: TmuxSession[] | null | undefined,
  sid: string,
): TmuxSession[] {
  return sessions?.filter((s) => s.sid === sid && isClaudeTmuxCommand(s.command)) ?? [];
}

/** findClaudeTmux 用它重实现——.filter(pred)[0] 与 .find(pred) 同一遍历顺序、同一结果，
 *  今天全部调用点/断言零改动（§2 取舍②的可验证性基础）。 */
export function findClaudeTmux(sessions, sid, cwd): TmuxSession | undefined {
  const matches = findClaudeTmuxMatches(sessions, sid);
  if (matches.length > 0) return matches[0];
  // ...cwd 回退逻辑不变...
}
```

三个需要分级处理的调用点（§2 取舍③④已定分级原则）：
- `resumeTabTmux`（`tabs.ts:2052` 一带）：`matches.length > 1` → 仍 attach 到 `matches[0]`
  （行为不变），追加一次性 toast 告知"该会话身份同时活在 N 个 tmux 里"。
- `restartTabWithAccountInner`（`tabs.ts:2358` 一带）：`matches.length !== 1` 一律拒绝并区分文案
  （`length===0` 沿用今天"未找到"文案；`length>1` 新文案报数并建议手动核实）。
- `resolveAttachMenuItem`/缓存路径的菜单构建（`tabs.ts:2127`/`2861` 一带）：kill 菜单项在
  `matches.length > 1` 时禁用+诊断文案（同 restart 严重度）；attach 菜单项沿用 resume 的
  警告+继续；preview 不受影响。**两处菜单构建逻辑本就高度重复，本次顺手抽一个共享私有函数**
  （工程审计 Phase E 若发现值得做才做，不预先在 Phase B 断言一定要抽，取决于 Phase C 实际改动量）。

新增 `resumingSids: Set<string>`（`TabManager` 私有字段，对称既有 `restartingSids`
`tabs.ts:2321-2329` 的既有模式：入口加入、`finally` 移除、重入短路），guard `resumeTabTmux`——
关闭 A 方案指出的"双击 resume 之间无互斥"这个真实、具体、已有先例可循的竞态。

### 3.3 `shared/ccm`：通道 A/B 拆分

- `shared/ccm:369`（外层 `--tmux=<name>` 分支的建会话序列）与 `shared/ccm:409-410`（内层
  exec 时刻的立即打标）：两处 `@ccm_sid` 改名 `@ccm_sid_expect`。
- `shared/ccm:428`（poller，`agent_has_identity` 分支内的后台循环）：**保持 `@ccm_sid`**，成为
  唯一写者——独占性从"偶然如此"变成"结构保证"。
- `src-tauri/src/sftp.rs`（`ccm_cli_has_required_elements` 的 needle 清单）：新增
  `@ccm_sid_expect` 断言，并加一条结构性检查确认两处立即写确实用的是 `_expect`（防未来改动
  静默改回去、重蹈 D6）。

## 4. 实现步骤（严格顺序执行）

- [x] **步骤 1**：`tmux.rs` 加 `is_safe_tmux_target` + `exact_target` 改 fallible；
      `capture_remote_pane`/`build_kill_session_cmd`/`build_send_keys_remote_cmd` 三个既有构造点
      改用 fallible 版本并传播错误。**实现时修正**：`is_safe_tmux_target` 最终只查空串——
      Phase B 综合方案设想的"字符集与 `is_ccm_tmux_name` 共享"会与既有 `tmux_targets_use_exact_match`
      测试钉死的"`si*`/`a'b` 等 glob/元字符必须被安全引号化通过"这一既定行为冲突（`shell_quote`
      已使任意内容安全，字符集收紧是 TS 侧 `isValidNewTmuxName`/`isValidTmuxName` 的职责）。
      — 验证：既有单测按§5「回归」列表逐一修（不是全部推倒重写，只有断言旧单行命令形状的部分
      需要跟新形状对齐）；命令构造先于配置查找（同步顺手改，让 Gate 1 拒绝不依赖测试环境有无
      配置好的 origin）。
- [x] **步骤 2**：加 `build_guarded_tmux_cmd`（`display-message` 原子查询 + gate2 远端半支 +
      gate3），`kill_remote_tmux`/`tmux_send_keys` 改用它；新增 sentinel 解析
      （`CCM_NO_SESSION`/`CCM_GUARD_REJECTED`）+ 对应错误文案。
      — 验证：Rust 单测覆盖四种渲染形态（对照 Phase B 来源方案 A §4.3 的具体串，13 个 tmux 测试
      全绿）+ **新增真机验收 `e2e/tmux-guarded-acceptance.sh`**（`npm run test:tmux-guarded`，
      14 项全绿）：用 `cargo test --lib -- --ignored --nocapture emit_guarded_commands_for_e2e`
      从真 builder 取生产命令串（同 `tmux-target-emit.mts` 对 TS 侧的模式，不手搓等价命令），
      在隔离 `-L` socket 上验证 2-window 会话真的拒绝 kill 且存活、1-window 真的被 kill、
      无 `@ccm_sid` 的自定义名会话被 gate2 挡且 pane 不被污染、有 `@ccm_sid` 时正常送达/kill、
      不存在的目标返回 `CCM_NO_SESSION`——这条门禁是 Phase C 实现后期补的（原计划只写了
      Rust 单测，落实时想起 R1 教训"门禁只锁字符串形状不锁行为"，三道门这类嵌套 if/cut 的真实
      shell 语法复杂度必须过真机）。
- [x] **步骤 3**：`shared/ccm` 两处改名 `_expect`；`sftp.rs` needle 清单同步；
      `e2e/ccm-acceptance.sh` 场景 3 拆成 3a（断言 `_expect`）+ 3b（合成
      `sessions/<pid>.json` 验证通道 B 确实把 `_expect` 提升为 `@ccm_sid`，15 项全绿）；
      `e2e/ccm-print-parity.sh` 的 `NEEDLE_SID_TAG` 更新（9 项全绿）。**顺带**：本机已部署的
      `~/.local/bin/ccm`（F02 真机部署遗留）与仓库 `shared/ccm` 重新同步（否则 print-parity
      对拍的是过期的本机部署副本，假红）。
      — 验证：`npm run test:ccm-acceptance`/`test:ccm-print-parity` 全绿。
- [x] **步骤 4**：`src/tabs.ts` 加 `findClaudeTmuxMatches`，`findClaudeTmux` 重实现（`.filter[0]`
      与旧 `.find` 同一遍历顺序同结果，21 个既有调用点/断言零改动全绿——F03 步骤4 同款纪律）；
      三个调用点分级改造（resume-attach 警告+继续、restart 拒绝、菜单 kill 项禁用+诊断文案，
      preview 不受影响）；`resumingSids` 互斥（对称既有 `restartingSids`）。
      — 验证：`src/tabs.vitest.ts` 既有 `findClaudeTmux`/`findIdleTmux`/`claudeExited` 断言组
      零改动全绿（614 项，证明重实现是行为保持的）；三个调用点各加新的"命中 >1"测试用例
      （+8 条新测试：5 条 `findClaudeTmuxMatches` 基础行为 + 3 条各调用点的分级验证）。
- [x] **步骤 5**：`doc/INVARIANTS.md` §30 追加"命中 >1 时的处理法则"（与既有 SS-5/SS-9 并列，
      不替换）+ 新增 §34（三道门+原子 verify+act 不变量，含 `@ccm_sid_expect`/`@ccm_sid` 拆分）；
      MASTERPLAN §1/§3 账本更新（`tmux.rs`/`tabs.ts`/`shared/ccm-wrapper.sh` 三行）；两个独立
      agent（后端架构 + UX）双审已跑完，均无阻塞项，结论见下方 §6。
- [x] **步骤 6**：全量门禁（`tsc`0/`npm test`615/`cargo test`377/`test:tmux-target`26/
      `test:ccm-cli`36/`test:ccm-acceptance`15/`test:ccm-print-parity`9/`test:tmux-guarded`14/
      `resume-suite`17/`restart-suite`24），结果重定向落盘后 Read 核实。

## 5. 测试策略

- **黄金串对拍**：`findClaudeTmux` 重实现前后逐字节同结果（同 F03 步骤4 纪律）。
- **真机验收表**（`e2e/tmux-guarded-acceptance.sh`，隔离 `-L` socket，14 项）：输入取自
  `cargo test --lib -- --ignored --nocapture emit_guarded_commands_for_e2e`（真 builder 产出，
  不手搓）——gate 3 对 2-window 真会话拒绝 kill 且存活、对 1-window 真的 kill；gate 2 对无
  `@ccm_sid` 的自定义名会话拒绝（kill 存活 / send-keys 不污染 pane）、对有 `@ccm_sid` 的放行
  （真 kill / 真送达）；目标不存在 → `CCM_NO_SESSION`；`cc-*` 前缀命中的零 Gate 退化路径仍正常
  工作。`e2e/ccm-acceptance.sh` 场景 3a/3b（真 tmux）另证明 `@ccm_sid_expect`→`@ccm_sid` 的
  通道A/B 提升机制真实可行。
- **回归**：`e2e/resume-suite.sh`/`restart-suite.sh`/`tmux-target-acceptance.sh` 确认不受影响
  （两版方案都已核实这三者不触碰本功能改动的代码路径）；`e2e/ccm-acceptance.sh`/
  `ccm-print-parity.sh` 的 `_expect` 相关断言更新。
- **Rust 单测**：`tmux.rs` 现有 `ccm_tmux_name_whitelist`/`parse_tmux_ls` 系列不受影响；
  `kill_remote_tmux_rejects_non_ccm_name`/`send_keys_cmd_construction`/`tmux_targets_use_exact_match`
  按新形状重写（保留原有"防注入"精神断言，形状随新命令串更新）。

## 6. 代码审计结果（Phase D）

两个独立 agent 并行审（prompt 自包含，各带 MASTERPLAN §0 核心思想全文），均**无阻塞项**。

**后端架构 + 正确性 agent**：逐条核实（Rust 三道门四种渲染形态手工重推、真机验收 14/14、
`cargo test tmux::` 13/13、`tabs.vitest.ts` 150/150、`tsc` 干净、`ccm-acceptance`/`ccm-print-parity`
全绿、`git diff --stat` 确认 driver/account-restart.ts 零改动）。发现 2 条重要项，均已修复：
1. **`CCM_GUARD_REJECTED` 拒绝消息恒带 `windows=%s`**（即便 send-keys 的 `need_windows=false`、
   根本不受 Gate 3 约束），会让用户误以为 windows 数也影响了 send-keys 的判断——已改
   `reject_msg` 按 `(need_sid, need_windows)` 精确匹配实际参与 guard 的字段，补 1 条 Rust 单测
   + 更新 `tmux-guarded-acceptance.sh` 场景 7 的期望值。
2. **`tmux-guarded-acceptance.sh` 缺两处工程细节**：cargo test 失败时会静默产出空命令、导致后续
   场景一堆看似随机的 FAIL 而非清晰诊断（已加 `[ -s ... ] || exit 1` 前置检查）；脚本正常结束
   不清理隔离 tmux server（已在 `trap` 里补 `kill-server`，同 `ccm-acceptance.sh` 的既有模式）。
「建议」级两条（`matches[0]` 的顺序性 tie-break 优化、可读性微调）判定为 F09 范畴或不影响正确性，
不在本轮处理。

架构核心思想符合度结论（agent 原话精神）：R10 被结构性根治，不是表面缝合。三道门 + 单次
round-trip 的原子 verify+act 真正消灭了破坏性动作的 TOCTOU 窗口（真机验证，非仅字符串断言）；
`findClaudeTmuxMatches` 被真正串进全部三个需要分级的调用点（含两处易漏改的近重复菜单构建块，
逐行核对未发现语义漂移）；`@ccm_sid_expect`/`@ccm_sid` 拆分是真正 load-bearing 的（`sftp.rs` 的
结构性锚点会在有人写回裸 `@ccm_sid` 时立即报警，不是文档仪式）。

**UX agent**：Job A（严重度分级独立评估）与 Job B（F09 前瞻兼容性）均无阻塞。确认 §2 取舍④
（resume/attach 警告继续 vs restart/kill 拒绝）站得住脚——"以后果是否可撤销为轴、不对命中多个
本身一刀切"是合理的产品判断；`findClaudeTmuxMatches` 返回的全量数组这个"缝"留得干净，F09 只需
替换 onClick 里"默认选 matches[0]"这行逻辑，不需要重新接管上游查询/过滤。发现 2 条重要项，均已
修复：
1. **措辞不一致**：`restartTabWithAccountInner` 的拒绝消息说"请到**远端**手动核实"，其余 5 处
   同类消息（resume 警告继续 ×2、菜单 kill 禁用 ×2）说"到**终端**核实/处理"——同一件事两种说法。
   已统一改成"终端"。
2. **toast 时长疑似拧了**：新增的 3 处"警告继续"toast 用了 10000ms（我随手定的），而
   `restartTabWithAccountInner` 的拒绝消息沿用了本文件已有的 8000ms 惯例（`warnCwdFallbackAttach`/
   旧"无法换号重启"都是 8000ms）——拒绝类消息信息量更大、更需要用户看清，反而比警告类更短，
   方向拧了。已把 3 处"警告继续"toast 也改成 8000ms，对齐既有惯例（不是发明新数字，是修正偏离）。
「建议」级两条（补"ambiguous 与 viaCwd 互斥"回归测试、kill 禁用项文案挪进 title）——前者已采纳
（新增 1 条测试直接钉死该不变量，不再只靠人工读代码证明）；后者判定为 F09 范畴的展示优化，不做。

## 7. 工程审计结果（Phase E）

主线程对账（读 MASTERPLAN §3/§6 + 本功能计划）：F04 落地后主计划仍自洽——账本 3 行已更新反映
最终形态（`tmux.rs` 三道门/`shared/ccm-wrapper.sh` 通道拆分/`tabs.ts` 身份统一部分）。R10（风险表）
已从"结构性风险"标记为"已修复"，理由见 §6 架构审计结论（真机验证的 TOCTOU 消除 + 全部三个需要
分级的调用点已升级）。`tabs.ts` 行的账本备注已明确区分"F04 落地的身份统一部分"与"F09 仍待做的
菜单 flyout/徽章/对齐删除部分"，不冒领 F09 的范围。

无新增技术债留给后续功能背负——F04 本身在 Phase D 审计中发现的 4 条重要项（2 后端 + 2 UX）全部
就地修复，未推迟。唯一转发的开放项是"`findIdleTmux` 的 `@ccm_sid_expect` 置信度回退"（明确判定
为孤儿规避的锦上添花，非破坏性安全必需项），已在计划 §1"明确不做什么"显式记录，不是模糊的
"以后再说"。

## 8. 签收（Sign-off）

- [x] 通过代码审计（无阻塞项；2 条重要项已修复：拒绝消息误导性字段、真机验收脚本工程细节）
- [x] 通过双 agent 架构/UX 审（含对 §2 取舍④的复核结论：认同该分级；2 条重要项已修复：措辞
      统一、toast 时长对齐既有惯例）
- [x] 通过工程审计（主计划仍自洽；R10 已标记已修复；唯一转发项已显式记录非模糊搁置）
- [x] 主计划已据此更新（§1 状态、§3 账本 3 行、§6 R10 标记已修复、§7 变更记录见下）
