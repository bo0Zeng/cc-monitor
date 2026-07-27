# 功能计划 — F06 本地路径并入 IR（Windows 本地 PowerShell 两套 builder 收进同一意图模型）

> 对应主计划 §1 的 F06。本文件是该功能从规划到签收的全程记录。
> **动手前先读 MASTERPLAN §0 核心思想**——本功能是推论「一个动作 + 若干正交修饰、且这个模型在
> IR、CLI、UI 三处是同一个」在**跨平台/跨语言边界**上的延伸：`transport:{kind:"local"}` 从 F03
> 起就是 `LaunchContext`/`LaunchPlan` 类型定义里的一个合法分支，但从未被任何调用点真正实例化过
> ——本功能要把它从"类型层面存在但从未活过的死形状"变成真正被走过的路径。

## 0. 本计划的来源（Phase B 方法论说明）

先开一个 Explore fork 摸清现状（`history.rs` 的两套 PowerShell builder 的精确范围、账号维度是否
真的零介入、前端调用点是否已构造过 `LaunchContext`、`renderFallback`/`renderCli` 能否直接复用），
再据其证据直接规划——**未开 Plan agent fanout**，理由：fork 已经用具体代码证据把"IR 前端构造
下发 vs Rust 侧同构 renderer"这条 ledger 里悬而未决的分叉，实际收窄成了一个由平台约束（`Get-Command`
探测必须在目标机器上跑）单向决定的问题,不是两个旗鼓相当、需要比较的架构方案。

**Explore fork 的关键证据**：

- `history.rs:921-946`（`build_resume_ps_command`）与 `history.rs:962-976`
  （`build_new_session_ps_command`）——除了是否带 `{flag} {sid}` 后缀，`Get-Command`-探测-回退
  分支逐字符复制。两者都调 `crate::adapter::active()` 取 `resume_flag`/`default_launcher`/
  `launcher_alias`，形状与 `LaunchPlan.action: {kind:"new"}|{kind:"resume",sid}` 已经吻合。
- **账号维度确认零介入**：两个函数都不触碰 `CLAUDE_CONFIG_DIR`/账号；前端调用点
  （`history.ts:1511-1523`/`:1539-1545`、`tabs.ts:2039`、`session-viewer.ts:355`）只传
  `{sessionId,cwd,launcher}`/`{cwd,launcher}`，从未构造过 `LaunchContext`。`grep` 全仓确认
  **`transport:{kind:"local"}` 从未被实例化**——纯类型层面的死分支。
- **IR 分叉的决定性证据**：`Get-Command {alias} -ErrorAction SilentlyContinue` 这条探测**必须在
  目标机器（本机 Windows）上跑**才有意义——TS 侧没有能力预先知道用户机器上是否装了 `cc` 这个
  PowerShell profile 函数。这就排除了"TS 全量渲染好字符串、Rust 只管 exec"（IR 前端构造下发）
  这条路，唯一站得住的是"TS 构造 `LaunchContext`/`LaunchPlan`（供 UI/未来维度统一推理），Rust
  侧独立同构渲染成 PowerShell"（Rust 侧同构 renderer）。
- **`plan.env` 对 local 的处理**：额外核实 `NESTED_ENV_RESET_DIMENSION`（`launch-dimensions.ts:82`,
  `applies: ctx.action.kind==="new"||"resume"`——不看 transport）一旦 local 也构造真 `LaunchContext`
  就会触发，产出一条 `unset` `EnvOp`。核实这条 op 是否需要在 Rust 侧真的渲染成 PowerShell
  的 `Remove-Item Env:\X`：读 `src-tauri/src/lib.rs:71-80,109`（`scrub_env_vars`）+
  `adapter.rs:65`（`nested_env_to_scrub`）发现issue #24 的嵌套 env 污染，在 **cc-monitor.exe 自己
  的进程启动阶段**（`lib.rs:109`，`setup()` 里，早于任何窗口被 spawn）就已经被 `remove_var` 清掉
  了——`launch_powershell_window` 用的 `Command::new(...).spawn()` 默认继承调用进程（已被清洗过
  的 cc-monitor.exe）的环境表，因此**本地路径的嵌套污染保护已经在别处（启动期一次性清洗）生效
  完毕，不需要每次 launch 时再清一次**。结论：`plan.env` 对 local 是"算出来但故意不消费"——不是
  遗漏，是这个维度原本就在保护一个和 tmux 持久 shell 场景（远端）不同的攻击面，本地场景的等价
  保护已经在另一层做好，见 §3 说明。

## 1. 目标与验收标准（DoD）

- **目标**：把本地 Windows 会话的 resume / 新建两条路径从"直接拼 `{sessionId,cwd,launcher}`
  调 Tauri 命令"改成"构造真实 `LaunchContext`（`transport:{kind:"local"}`）→ 跑维度注册表 →
  从 `LaunchPlan` 取字段调用（IPC 签名不变）"；Rust 侧把两个逐字符重复的 `Get-Command`
  探测-回退分支收拢成一个共享构造函数，以 `LaunchAction` 同构的动作枚举驱动。

- **验收标准**：
  - [x] `src-tauri/src/history.rs`：新增内部枚举 `LocalPsAction { New, Resume(String) }`
        （字段/变体命名同构 TS `LaunchAction` 的 `new`/`resume{sid}`，不含 `attach`——本地从无
        attach 概念）；`build_resume_ps_command`/`build_new_session_ps_command` 收拢成一个
        `build_local_ps_command(action: &LocalPsAction, launcher: Option<&str>) -> Result<String,
        String>`，`Get-Command` 探测-回退分支只写一次；两个 `#[tauri::command]`
        （`resume_history_session`/`new_local_session`）签名、行为、错误文案**逐字节不变**，内部
        改调新函数
  - [x] 新增 Rust 单测：对 `New`/`Resume(sid)` 两个变体，逐字节对比新函数输出与**旧函数在同输入
        下曾经的输出**（把旧函数原样保留成 `#[cfg(test)]` 私有对照实现，或直接内联旧字符串期望值
        ——防止"重实现即回归"）；7 个既有测试（防注入/cc 优先/自定义命令——Phase D 审计更正计数，
        最初误记成 6 个）保持原样全绿（证明
        重构没有悄悄改变校验逻辑）
  - [x] `src/launch-requests.ts`（落点：直接用该文件，与其余 4 个 `planXxx` 同一模块）：新增
        `planLocal`——一个纯函数把"本地 resume"/"本地新建"意图构造成 `LaunchContext`
        （`transport:{kind:"local"}`, `container:{kind:"none"}`, `account:{kind:"base"}`,
        `ccmSid:undefined`, `action` 按调用方给定），跑 `buildLaunchPlan(ctx)`，从产出的
        `LaunchPlan` 取 `action`/`cwd`/`launcher` 三个字段映射回现有 Tauri 调用参数（**IPC 调用
        签名不变**：仍是 `invoke("resume_history_session",{sessionId,cwd,launcher})`/
        `invoke("new_local_session",{cwd,launcher})`——只是这三个值现在来自 `LaunchPlan` 而不是
        直接手传）
  - [x] `src/views/history.ts`（本地 resume 分支 + 本地新建分支）、`src/tabs.ts:2039`
        （本地 resume 调用点）、`src/views/session-viewer.ts:355`（本地 resume 调用点）改用上面的
        新函数——四个调用点行为逐字节不变（仍传同样的 sessionId/cwd/launcher，只是经过了 IR 这一
        跳）
  - [x] `doc/INVARIANTS.md`：新增一条（§36，Phase D 审计前一度漏做，审计发现后补上），记录"`plan.env` 对本地路径恒非空但故意不被消费——嵌套
        env 污染保护已经在 `lib.rs::scrub_env_vars`（进程启动期一次性清洗）里做完，`local` 渲染
        器不需要也不应该重复处理 `plan.env`"，防止未来有人看到 local 渲染器"漏读了 plan.env"
        就顺手加一段其实多余（且未经真机验证）的 PowerShell env-unset 代码
  - [x] MASTERPLAN §1（F06→完成）、§3 账本（`history.rs` 行更新为已落地状态）、§7 变更记录更新
  - [x] 门禁：`tsc`/`npm test`/`cargo test` 全绿；因平台限制（本机是 Linux，无 Windows/`pwsh`
        环境），**没有、也不可能有本地 PowerShell 真机执行验收**——这不是本功能新引入的验收
        缺口，是这两个函数从写下第一行起就有的既有现状（既有 7 个 Rust 测试本身也全部是纯字符串
        断言，从未真实 spawn 过 PowerShell 进程），本功能只是保持同等验证强度，不倒退也不虚假
        拉高

- **明确不做什么**：
  - **不给 `NESTED_ENV_RESET_DIMENSION` 加 transport 判断，也不让本地渲染器消费 `plan.env`**
    ——见 §0 证据：本地场景的等价保护已经由 `scrub_env_vars` 在进程启动期做完，此处重复实现
    只会引入未经验证的新 PowerShell 语法，得不到对应收益。
  - **不新增任何 UI**——本功能只改「同一个 Resume/新建按钮」内部怎么走到 Rust，不改按钮本身、
    不改菜单、不改本地会话有没有账号选择（本地从来没有、也不打算有，见下条）。
  - **不给本地路径加账号维度**——`account:{kind:"base"}` 是硬编码常量，不是"待填"的占位。
    Windows 本地会话没有 `CLAUDE_CONFIG_DIR` 隔离概念（`fetchAccounts` 只对 SSH origin 生效），
    这不是本功能的范围，也不计划成为任何后续功能的范围（与 F05 计划里"本地路径的账号维度是
    F06 的范围"这句预告不同——摸清现状后判定：本地压根没有"账号"这个概念可维度化，F05 那句
    话描述的是"如果本地将来有账号隔离，账本会记在这"，不是"F06 必须生造一个"）。
  - **不合并 `resume_history_session`/`new_local_session` 两个 `#[tauri::command]`**——它们是
    两个不同的 Tauri IPC 入口，各自的错误文案/前端调用点都不同，合并没有收益、只有前端调用面
    改动风险。

## 2. 与主计划的对接 + 关键决策（附理由）

**触及的共享面**：`src-tauri/src/history.rs`、`src/views/history.ts`、`src/tabs.ts`、
`src/views/session-viewer.ts`；新增一个薄的本地意图构造函数（落点视实现时定，`src/launch-requests.ts`
或新文件）。**不触及** `src/launch-plan.ts`/`launch-dimensions.ts`/两个远端渲染器主体——这正是
"加一条新 transport 走既有维度注册表，零改渲染器主体结构"这条架构承诺（MASTERPLAN §0.1 成功
标准②）在本功能里的验收点（比 F07 的"加新维度"稍弱一档，但方向一致：本功能验的是"加新
transport"，F07 验的是"加新维度"）。

**两处关键决策**：

1. **"Rust 侧同构 renderer"而非"IR 前端构造下发"**——理由见 §0，`Get-Command` 探测本质是
   render-time 决策，只能在目标机器上做，TS 无法预先算出。TS 侧构造 `LaunchContext`/`LaunchPlan`
   的价值不是"预先渲染好交给 Rust"，而是让本地路径的**意图表达**（"这是一个 resume sid=X 的
   动作"vs"这是一个 new 动作"）与远端共享同一套类型系统，为未来 F09 的"同一个 Resume 按钮"统一
   动作模型铺路——今天 IPC 边界上传递的具体字段（`sessionId`/`cwd`/`launcher`）保持不变，只是
   来源换成了从 `LaunchPlan` 里取,而不是直接手传。
   
2. **`plan.env` 故意计算但不消费**——见 §0 证据链（`scrub_env_vars` 在进程启动期已经做完等价
   保护）。这不是"图省事跳过一个该做的事",是**两层保护本来就分工不同**：远端 `nested-env-reset`
   维度保护的是"tmux 持久 server 环境跨多次 resume 累积污染"（同一个 tmux server 可能存活很久，
   每次新 resume 进去的 shell 都从 server 环境继承）；本地场景没有"持久 server"这个概念——每次
   `launch_powershell_window` 都是全新进程，唯一可能的污染源是"cc-monitor.exe 自己被谁以带毒
   环境启动"，而这条攻击面已经被"进程启动时扫一遍、清干净"这个更早、更根本的机制堵死。补一层
   渲染期重复清洗，不会更安全，只会多一段没有真机验证过的 PowerShell 代码。

## 3. 接口 / 契约设计

### 3.1 Rust：`build_local_ps_command`（`history.rs`）

```rust
enum LocalPsAction {
    New,
    Resume(String), // sid，构造时已校验字符集
}

/// 收拢 build_resume_ps_command / build_new_session_ps_command 的共同逻辑——
/// F34 自定义命令优先 → cc 别名探测优先 → 回退默认拉起，只写一次。
fn build_local_ps_command(action: &LocalPsAction, launcher: Option<&str>) -> Result<String, String> {
    if let LocalPsAction::Resume(sid) = action {
        let valid = !sid.is_empty()
            && sid.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
        if !valid {
            return Err(format!("refuse resume: invalid session_id {sid:?}"));
        }
    }
    let agent = crate::adapter::active();
    if let Some(l) = sanitize_launcher(launcher)? {
        return Ok(match action {
            LocalPsAction::Resume(sid) => format!("{l} {} {sid}", agent.resume_flag()),
            LocalPsAction::New => l,
        });
    }
    let def = agent.default_launcher();
    let suffix = |bin: &str| match action {
        LocalPsAction::Resume(sid) => format!("{bin} {} {sid}", agent.resume_flag()),
        LocalPsAction::New => bin.to_string(),
    };
    Ok(match agent.launcher_alias() {
        Some(alias) => format!(
            "if (Get-Command {alias} -ErrorAction SilentlyContinue) {{ {} }} else {{ {} }}",
            suffix(alias), suffix(def),
        ),
        None => suffix(def),
    })
}
```
`resume_impl`/`new_local_session` 改调 `build_local_ps_command(&LocalPsAction::Resume(sid.into()), launcher)`
/`build_local_ps_command(&LocalPsAction::New, launcher)`，其余机械（`launch_powershell_window`
拉起、错误文案、日志）不变。

### 3.2 TS：`planLocal`——本地意图 → `LaunchContext`/`LaunchPlan`（落点 `launch-requests.ts`，
与其余 4 个 `planXxx` 同一模块、同一命名习惯）

```ts
export function planLocal(action: LaunchAction, cwd: string | null): LaunchPlanBuild {
  if (action.kind === "resume" && !isValidSessionId(action.sid)) {
    throw new Error(`非法 sessionId（拒绝拼入命令）: ${JSON.stringify(action.sid)}`);
  }
  const ctx: LaunchContext = {
    transport: { kind: "local" },
    action,
    container: { kind: "none" },
    cwd,
    account: { kind: "base" }, // 本地无账号隔离概念，恒定常量，非占位
    launcherOverride: undefined, // launcher 由调用方单独传，不经这层
    ccmSid: undefined,
  };
  return { ctx, plan: buildLaunchPlan(ctx) };
}
```
四个调用点（`history.ts` ×2、`tabs.ts` ×1、`session-viewer.ts` ×1）改成：先
`planLocal({kind:"resume",sid}|{kind:"new"}, cwd)`，跑 `buildLaunchPlan`，`launcher` 仍是调用方
已有的裸字符串直接 invoke——不从 `plan.launcher` 取（`plan.launcher` 是给远端渲染器用的、经过
维度改写的字段；本地路径的 `launcher` 是用户在设置面板填的裸字符串，直接透传给 Rust 自己
校验/拼接，不需要经过 IR 这层）。

**实现期修正**（写代码时发现 §1 最初设想的"仅走个形式验证,不改变任何实际传参"会导致调用点
出现"调了函数却丢弃返回值"的死代码味——`plan.action`/`plan.cwd` 在当前维度注册表下恒等于输入
`action`/`cwd`,没有信息增量可"取回"）。改法：给 `planLocal` 补一条**其余 `planXxx` 早就有、本地
路径此前唯一缺失**的校验——`action.kind==="resume"` 时 `isValidSessionId(action.sid)`,不合法
即 `throw`（同其余 4 个远端 `planXxx` 函数的既有模式,不是新发明）。这样调用点里的
`planLocal(...)` 调用变成**有真实作用**（sid 校验提前到任何 IPC 往返之前,失败走既有的
`catch → showActionFailureToast` 路径,文案不变）,不再是摆设。这不是新增能力,是补一个本地
路径本就该有、只是此前完全靠 Rust 侧兜底校验的一致性缺口——本地 sid 100% 来自 cc-monitor 自己
的本地历史索引（可信来源）,这条校验在正常使用下永远不会真的触发,只是防御性收紧,不改变任何
合法输入下的行为字节。

**Phase D 审计再修正**（后端架构 agent 指出 §1/§4 用"逐字节不变"描述 sid 校验有欠精确）：TS
侧 `isValidSessionId`（`/^[A-Za-z0-9_][A-Za-z0-9_-]{0,127}$/`——首字符禁 `-`、长度 ≤129）与
Rust 侧 `build_local_ps_command` 内联的字符集校验（`!sid.is_empty() && all(alnum|'-'|'_')`——
无长度上限、首字符可以是 `-`）**不是同一份校验的两处复制，是两个独立定义、字符集不完全相同**
的检查——审计验证过方向安全（TS 接受集合是 Rust 接受集合的严格子集，凡通过 TS 新校验的 sid
必然也通过 Rust 校验，反向不成立），且真实 sid 来源（Claude/Codex 的 UUID、`create_branch_session`
的 `Uuid::new_v4()`）三种都验证过不会被误拒。"四个调用点行为逐字节不变"这句话准确的表述应是
"**对所有真实可能出现的 sid，行为逐字节不变**"，不是"字符集完全相同"——本条已订正措辞，不改
代码（不收紧 Rust 现有校验换不到实际收益，也不放宽 TS 新校验削弱防御深度）。

## 4. 实现步骤（严格顺序执行）

- [x] **步骤 1**：Rust 侧收拢两个 builder 成 `build_local_ps_command`（§3.1），`resume_impl`/
      `new_local_session` 改调新函数。
      — 验证：既有 7 个测试全绿（零改动，Phase D 审计更正计数）；新增 2 条"新函数与旧函数曾经
      的输出逐字节相同"测试
      （New 场景 + Resume 场景，各覆盖"有 cc 别名"/"无别名"两分支）。
- [x] **步骤 2**：TS 侧在 `src/launch-requests.ts` 写 `planLocal`（§3.2，含 sid 校验的实现期
      修正），四个调用点改用它。
      — 验证：新增 vitest 覆盖"构造出的 `LaunchContext` 经 `buildLaunchPlan` 后 `plan.action`
      与输入一致、`plan.env` 非空但确认从未被读取、非法 sid 真的 throw"；四个调用点各自既有的
      jsdom/vitest 断言（`history-actions.vitest.ts` 等）零改动全绿（证明四处调用点合法输入下
      行为未变）。
- [x] **步骤 3**：`doc/INVARIANTS.md` 补记 §0/§2 第2条的"`plan.env` 对本地故意不消费"结论
      （防未来误"修"）；`src/launch-render-cli.ts:46` 的注释 `// local = F06，未实现` 已过期
      （F06 落地后 local 不是"未实现"，是"设计上永远不走这条渲染器，有自己独立的 Rust 侧
      renderer"）——改成准确描述，避免误导未来读者以为还要给 local 补一个 CLI 渲染路径。
- [x] **步骤 4**：双 agent 审（后端架构 + UX，prompt 自包含带 MASTERPLAN §0 全文）。
- [x] **步骤 5**：MASTERPLAN §1/§3/§7 更新；全量门禁；commit。

## 5. 测试策略

- **黄金串对拍**：新 `build_local_ps_command` 与旧两个函数在相同输入下逐字节同输出（Rust 单测
  直接把旧实现的字符串期望值内联进新测试，而不是删掉旧函数——旧函数删除后无法再"对拍"，只能
  靠记忆核对，风险更高）。
- **IR 往返验证**：`planLocal` 产出的 `ctx` 经 `buildLaunchPlan` 后，`plan.action` 逐字段等于
  输入 `action`（证明本地路径真的在用同一套维度注册表，不是套了个类型皮的假装）；非法 sid 经
  `planLocal` 真的 throw（证明这条实现期修正的校验确实生效）。
- **回归**：四个前端调用点既有测试零改动全绿；7 个既有 Rust 测试零改动全绿。
- **已知且接受的验证缺口**（不是本功能引入的，写明是为了不让后续审计重复发现同一件事）：无
  Windows/`pwsh` 环境，无法真机验证 PowerShell 字符串确实能被 PowerShell 正确解析执行——这是
  这两个函数从最初写下就有的既有现状，本功能维持同等强度，不承诺、也不可能提升到 tmux 那样的
  真机验收水平。

## 6. 代码审计结果（Phase D）

两个独立 agent 并行审（prompt 自包含，各带 MASTERPLAN §0 核心思想全文），均**无阻塞项**（一份
初次汇报按其审查口径列了一条"阻塞"，复核后判定不成立——见下方说明）。

**后端架构 + 正确性 agent**：逐一压测了 §0/§2 两处关键决策——① `scrub_env_vars` 的调用时点
（`lib.rs::run()` 第一批实质语句，严格早于 `Builder`/`.setup()`/`.invoke_handler()`/`.run()`）
与 `launch_powershell_window` 全程零 `.env()`/`.env_clear()` 调用，全仓无绕开这次清洗的自重启
路径——**判定成立，未发现漏洞**；② TS `isValidSessionId` 与 Rust 内联校验的字符集比对，确认
方向安全（TS 接受集合是 Rust 接受集合的严格子集），且用 Claude/Codex/`create_branch_session` 三
种真实 sid 来源逐一验证不会被误拒。该 agent 最初把"计划文档 §1/§4/§8 checkbox 全未勾 +
`doc/INVARIANTS.md` 缺 F06 条目"列为"阻塞"——**复核后判定这不是功能性阻塞**：checkbox 未勾是
Phase F 文档收尾尚未进行（本报告即在补齐，同 F05 的既有节奏），但 `doc/INVARIANTS.md` 缺条目
这一点是真实的、本该在 Phase D 之前完成的步骤3 遗漏（已在本轮补齐 §36，见下）。发现 2 条重要项：
1. Rust/TS 两处 sid 字符集校验不是同一份定义的两处复制，字符集不完全相同（已在 §3.2 补"Phase D
   审计再修正"说明，订正"逐字节不变"这句表述的精确度，不改代码——方向已验证安全）。
2. `src/launch-render-cli.test.ts:54` 的测试标题仍留着与源码注释同款的过期措辞"F06 未实现"（源码
   注释已订正，测试标题漏改）——已修。

「建议」三条：① 计划文档记账误差"6个既有测试"应为 7 个——已订正；② no-alias 分支（`launcher_alias()`
返回 `None`）在当前 `adapter::active()` 恒返回 `ClaudeCodeAdapter`（`launcher_alias()` 恒
`Some("cc")`）的现状下结构性不可测——记录为已知限制，不为此引入测试专用的 adapter 注入点（不成
比例）；③ 顺带核对 `agent-profile.ts::nestedEnvVars` 与 `adapter/claude_code.rs::CLAUDE_NESTED_ENV`
是否一致——**本席直接复核后判定这是误报**：两份列表都是同样的 4 项（`CLAUDECODE`/
`CLAUDE_CODE_CHILD_SESSION`/`CLAUDE_CODE_SESSION_ID`/`CLAUDE_CODE_ENTRYPOINT`），只是顺序不同，
`unset` 操作彼此独立、顺序不影响语义，不是 bug，未采纳、未登记风险（记录在此是为了不让后续审计
重复排查同一个已排除的假阳性）。

**UX agent**：核实了 4 个调用点的错误呈现一致性与 `planLocal`/`getBehavior()` 相对顺序。发现
2 条重要项，均已修复：
1. 本地 resume 的 sid 校验失败 headline 是"恢复失败"，与远端同类失败的专属 headline"无法构造
   resume 命令"不一致（body 文案本身一致，但 headline 暗示"已尝试执行"不准确）——已把 4 个调用
   点全部改成 §0 corollary②"单一渲染目标"精神下的两阶段 catch：构造失败 → "无法构造 resume
   命令"（对齐远端 `runRemoteResume` 的既有措辞），执行失败 → 原有 headline 不变。
2. 4 个调用点里 `planLocal` 相对 `getBehavior()` 的实际执行顺序不一致（`history.ts::runResume`/
   `session-viewer.ts::resumeBranch` 是 `planLocal` 先行；`history.ts::runNewSession`/
   `tabs.ts::resumeTab` 的 `getBehavior()` 在函数最前面已经跑过一次 IPC），`tabs.ts` 处的注释
   "sid 校验提前到 IPC 往返之前"在这个具体调用点不准确——已订正为"sid 校验先于 resume_history_session
   这次 invoke（不代表本函数此前没有过其它 IPC）"，并给 `runNewSession` 补了一句解释"为什么这里
   不特意重排"（new 动作没有 sid 需要拦截，`getBehavior()` 是 remote 分支也要用的共享读取）。
三条建议均已核实、记录、判定不阻塞：Codex 会话经同一本地 resume 路径会被 `adapter::active()`
硬编码派发成 claude/cc 命令而非 codex 命令——确认是 F06 之前就有的既有缺口（重构前的两个旧函数
同样调用 `active()`），非本次引入的回归，超出 F06 范围；`runNewSession` 里 `planLocal` 的返回值
恒被丢弃是计划已承认的"为 F09 铺路"取舍；本地路径与终端手敲 `cc`/`cct` 的一致性核实为合理，不
构成半成品。

## 7. 工程审计结果（Phase E）

主线程对账（读 MASTERPLAN §0/§0.1/§3/§6 + 本功能计划）：F06 落地后主计划仍自洽——§0.1 成功标准
①（"每个入口都能带账号、带 tmux"）不适用于本地路径（本地无账号/tmux 概念，计划已明确登记为
"不做什么"）；成功标准②（"加一个新维度零改 builder/renderer/调用点"）F06 验的是稍弱一档的
"加一个新 transport"，`buildLaunchPlan`/`renderFallback`/`renderCli`/`canRenderCli` 的既有分支
结构确认零改动（`git diff` 核对：`launch-plan.ts`/`launch-dimensions.ts`/`launch-render-fallback.ts`
全程零改，`launch-render-cli.ts` 只有一行注释订正）；成功标准③（"单账号载荷逐字节相同"）本地
路径本来就没有账号概念，不适用；成功标准④（"终端起的会话 app 无缝识别"）本地路径没有
`@ccm_sid` 概念（`ccmSid: undefined` 恒定），不适用——这些"不适用"都是本地场景的固有属性，不是
本功能故意回避的缺口，均已在计划 §1"明确不做什么"里显式登记，不是模糊的"以后再说"。

**账本预见的重叠，现在优雅处理而非留给以后打补丁**：F06 让本地路径开始说与远端同一种
`LaunchContext`/`LaunchAction` 语言，这正是为 F09（UI 收敛：action×modifier flyout）铺的路——
F09 落地时不需要再为"本地会话怎么表达 resume/new 意图"单独发明一套模型，直接复用 `planLocal`
产出的 `ctx.action` 即可。这条铺垫在 F06 计划 §2 决策1 里已经写明，本次工程审计确认它是真实
成立的（不是空话）：`planLocal` 是纯函数、零副作用、已被 4 个真实生产调用点使用，不是只存在于
测试里的摆设。

未发现任何会拖累后续功能的新耦合/技术债。审计发现的两个"重要"项（sid 字符集措辞、错误 headline
不一致）都是本功能自身范围内的问题，已就地修复，不转发给任何后续功能。`nestedEnvVars` 顺序差异
的假阳性已排除，不登记风险、不占用后续功能的注意力。Rust `no-alias` 分支的测试盲区是既有现状
（F06 之前就有），登记为已知限制，不因为本功能重构了这段代码就临时拔高验证要求。

`doc/INVARIANTS.md` §36 的补记闭环了计划 §4 步骤3 一度被跳过的缺口——这是本次 Phase D 审计
真正抓住的、有实质价值的发现（不是文档措辞层面的小事）：这条不变量原本就是计划专门设计用来
**防止未来某次改动误"修"本地渲染器、给它加一段多余且未经真机验证的 PowerShell env-unset 代码**
的护栏，缺了它，计划明确预见并试图关闭的风险敞口会一直敞着。已在本轮补齐并重新跑通全量门禁。

## 8. 签收（Sign-off）

- [x] 通过代码审计（无阻塞项；1 条重要项——sid 字符集措辞订正——已修）
- [x] 通过双 agent 架构/UX 审（2 条重要项已修：错误 headline 对齐远端 + 调用顺序注释订正；INVARIANTS §36 补齐）
- [x] 通过工程审计
- [x] 主计划已据此更新
