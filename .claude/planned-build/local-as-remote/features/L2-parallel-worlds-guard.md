# L2 — Windows 本地进 IR：**计划写的三件事一件都不该做**，改做它真正的意图

> 主计划：`../MASTERPLAN.md` §1 L2（P1）· §0.2 · §2「关键判断」· §3 账本第 2/3 行 · §4 顺序表第 4 位
> 前置：L1（`d8a9df6` + `04d33ca`）
> **本轮结论：原方案否决 + 记录订正 + 改做一条今天真实存在的漂移点守卫。**

## 1. 开工复测：L2 的三条组成部分逐条撞墙

主计划 L2 = 「`planLocal` 复活 + PowerShell 渲染器分支 honour `plan.env`；
`build_local_ps_command` 变成该分支的实现而非平行世界」。

| 组成部分 | 复测结论 |
|---|---|
| **PowerShell 渲染器 honour `plan.env`** | ❌ **撞 `doc/INVARIANTS.md` §36 的铁律**，逐字禁止 |
| **`planLocal` 复活** | ❌ **撞 R07 已审计的决定**：本地借 IR 做校验、不消费其输出，因为「**接了也拿不到新东西**」 |
| **`build_local_ps_command` 变成该分支的实现** | ❌ 依赖上面两条 |
| 收益 ②「嵌套 env 清理在 Windows 本地**首次生效**」 | ❌ **事实错误**（§2 实证） |
| 收益 ①「§40 的例外变成类型上的显式分支」 | ⚠ 兜底渲染器**只服务远端**，给它加「Windows 本地」臂 = 给一个本地永不经过的渲染器加死分支（§3） |

### 1.1 §36 那条铁律，逐字

> **铁律**：**给本地渲染器补一段读 `plan.env`、把 `unset` 翻成 PowerShell `Remove-Item Env:\X`
> 的代码，是错的「修复」** —— 两层保护本来就分工不同（远端：渲染期逐次清；本地：启动期一次清），
> 本地补一层不会更安全，只会引入一段从未有真机（Windows/`pwsh`）验证过的新 PowerShell 语法，
> **纯增加风险不增加收益**。

L2 要做的**正是这条禁止的事**。而 §36 不是随口一写：它带着 F06 的实现期修正 + R07 的 Phase D 审计订正。

## 2. 收益 ② 是事实错误 —— 实证，不是推断

主计划写：「嵌套 env 清理在 Windows 本地**首次生效**（今天没有 —— 从一个 agent 会话里
起本地会话会泄漏继承的 agent env）」。

**实测 `src-tauri/src/lib.rs`**：

| 行 | 内容 |
|---|---|
| **124** | `let scrubbed_env = scrub_env_vars(adapter::active().nested_env_to_scrub());` |
| 161 | `let mut builder = tauri::Builder::default();` |

⇒ **清洗在 `Builder` 构造之前就跑完了**，直接 `std::env::remove_var` 清掉 cc-monitor 自己进程的环境；
后续 `Command::new(...)` 默认继承**已清洗过的**父进程环境。
「今天没有」这句话不成立 —— 保护**一直都在**，只是形态是「启动期一次清」而不是「渲染期逐次清」。

§36 早就把这件事写清楚了，包括为什么两种形态**分工不同而不是缺一半**：
`NESTED_ENV_RESET_DIMENSION` 防的是「tmux **持久 server** 的环境表跨多次 resume 累积污染」，
而 Windows 本地**没有持久 server** —— 每次都是全新 `wt.exe`/`powershell.exe` spawn。

## 3. 收益 ① 也不成立：兜底渲染器根本不在本地路径上

`renderFallback` 的全部生产调用点（`grep`，剔除测试）：

```
src/remote-launch.ts     ×5   （resume-direct / resume-tmux / attach-existing / launcher / attach）
src/remote-launch-run.ts ×1   （renderLaunchCommand 的兜底分支）
```

**全是远端。** 本地路径（`views/history.ts` → `new_local_session`/`resume_history_session`
→ Rust）**一次都不经过它**。给它加一个「Windows 本地」臂，加出来的是一条**永不执行的分支**
—— 而穷尽 `match` 的价值恰恰在于「加维度时编译器会问你」，一个没人走的臂只会让下次加维度时
多回答一个假问题。

## 4. 但 L2 的**意图**是成立的，且那个漂移点今天真的存在

L2 的意图：**别让本地与远端变成会静默漂移的平行世界。**
复测发现漂移点确实有一个 —— 只是不在计划猜的地方：

| 值 | Rust 侧（本地路径用） | TS 侧（远端路径用） |
|---|---|---|
| resume flag | `adapter/claude_code.rs::resume_flag()` = `--resume` | `AGENT_PROFILE.resumeFlag` |
| 默认启动器 | `…::default_launcher()` = `claude` | `AGENT_PROFILE.defaultLauncher` |
| 嵌套 env 清单 | `…::CLAUDE_NESTED_ENV`（4 条） | `AGENT_PROFILE.nestedEnvVars`（4 条） |

**两侧各写一份，而对应关系此前只活在 `agent-profile.ts` 的一句注释里**
（「对应 Rust `adapter.nested_env_to_scrub`」）—— **没有任何东西钉住**。
（先查了「有没有人已经在守」：全仓无测试同时引用两侧，`base-flag-contract-guard.vitest.ts`
守的是另一条 monitor↔`shared/ccm` 的契约。）

**漂了会怎样**：本地路径（Rust `local_launch_choice`）与远端路径（TS `remote-launch.ts`）
各自据这三个值拼命令。表现是**静默不一致** —— 同一个会话，本地 resume 与远端 resume 拼出
不同的命令行，而**两边各自的测试都是绿的**（它们各自钉的是自己那一份）。
这正是 L2 想防的「平行世界」，只是位置对了。

## 5. 交付：`src/agent-profile-parity.vitest.ts`（6 条断言）

范式照 `base-flag-contract-guard.vitest.ts`（Z02 建立，它自己又照
`tmux.rs::tmux_ls_fmt_double_write_point_stays_in_sync`）：**读另一侧源文件 + 锚点 + 反向自检**，
对 Rust 侧**只读**。**没另造范式。**

| 断言 | 说明 |
|---|---|
| 反向自检 | 真读到了文件（长度下界 + 结构锚点 `impl AgentAdapter for ClaudeCodeAdapter`）+ **剥注释后确实变短了** |
| 剥注释机制有效 | 用一个「注释里写着假 flag」的合成输入直接测 `stripLineComments`，不依赖当前文件恰好有那种注释 |
| resume flag 一致 | |
| 默认启动器一致 | |
| 嵌套 env **集合**一致 | **顺序刻意不钉**：两侧语义都是「要 unset 的名字集合」，TS 那份按 unset 语句排、Rust 按可读性排。钉顺序会把无关差异变成假红 |
| 条数一致 | 与上一条重叠，但**说清意图**：漏加的那侧会少清一个 env，表现是「从 agent 会话里起的会话被误判成嵌套子会话」，UI 上看不出来 |

### 5.1 ★ 我第一版的一条断言是错的，被自己写的测试当场证伪

第一版我写了「**剥注释是必需的**：那个 Rust 文件的注释里逐字写着 `--resume` 与 `claude`」，
并加了一条断言去证明它。**那条断言红了** —— 逐字写着
`/// resume 一个已存在会话的命令 flag(CC = --resume)` 的是**隔壁** `src-tauri/src/adapter.rs`
（trait 定义），不是 `claude_code.rs`。

订正后如实写：**剥注释在这里是 fail-safe，不是当前必需** —— 隔壁文件已经证明这种注释是本仓的
常态写法，这个文件随时可能长出一条。并把那条断言改成**直接测机制**（合成输入），
不再依赖「当前文件恰好有那种注释」这种会随内容飘的事实。

## 6. 变异验收（Phase D）

| 变异 | 结果 |
|---|---|
| **A** Rust 侧把 resume flag 改成 `--continue`，TS 不动 | **成立且隔离**：只红「resume flag 两侧一致」 |
| **B** Rust 侧默认启动器改成 `claude-next` | **成立且隔离**：只红「默认启动器两侧一致」 |
| **C** Rust 侧新增一个嵌套 env，TS 不跟 | **成立**：红「集合一致」+「条数一致」两条（后者本就是前者的显式化） |

恢复后跑「恢复态应全绿」+ `git diff --stat` 确认已还原（本轮守卫是运行时读文件，
没有 `cp -a` 保 mtime 那个编译缓存问题，但流程照走）。

**如实标注**：铁律 8 要求并行多 agent 审计；本会话常驻指令「除非用户要求不开 agent」
⇒ 主线程变异 + 全门禁代替。**这是欠账，不是强度裁剪。**

## 7. 门禁

| 门禁 | 前 | 后 |
|---|---|---|
| npm | 866 / 57 files | **872 / 58 files**（+6） |
| tsc / eslint / check:types | 0 · 7 基线 · 67 | **同左** |
| monitor `cargo test --all` / clippy lib / fmt | 632 · 36 · clean | **同左**（L2 没改 Rust 生产代码） |
| daemon | 173 | **同左** |

## 8. 签收

- [x] **原方案三条逐条否决**，每条给出实证依据（§36 铁律 / R07 审计决定 / `renderFallback` 调用点）
- [x] **收益 ② 的事实错误已实证订正**（`lib.rs:124` 早于 `:161`）
- [x] **改做 L2 的真意图**：钉住今天真实存在的跨语言漂移点（建守卫前已查「有没有人在守」）
- [x] 范式照既有的，**没另造**；剥注释 + 反向自检 + 机制自测
- [x] 三条变异全部成立（A/B 隔离，C 红两条且第二条是第一条的显式化）
- [x] **纠正了自己一条错误断言**（§5.1，被自己写的测试证伪）

## 9. 没做的（登记）

| # | 事项 | 为什么 |
|---|---|---|
| 1 | **计划原本的 L2 三件事** | §1-§3：一条撞铁律、两条撞已审计决定、收益陈述事实错误。**订正记录，不硬做** |
| 2 | **Windows 真机 8 套 152 条断言** | 主计划把它列为 L2 的硬门槛。**本机是 Linux，跑不了 ⇒ 无法验证**。不过本轮**没有改动任何 Rust 生产代码**，Windows 行为面零变化 ⇒ 该门槛本轮不适用（但**没验就是没验**，如实记） |
| 3 | `livenessProcessNames` 未纳入守卫 | 它在 Rust adapter 里**没有对侧**（`grep` 确认），钉不了。若将来 Rust 侧长出对应物，加进这张表 |
| 4 | `launcher_alias()`（`cc`）未纳入 | TS 侧没有对应物 —— 别名探测只发生在 Rust 的两个本地渲染器里（`Get-Command` / `command -v`），远端不做 |
