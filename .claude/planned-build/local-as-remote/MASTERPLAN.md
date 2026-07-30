# 主计划 / MASTERPLAN — local-as-remote（本地 = 不走 ssh 的远端 · 含 Linux 平台）

> 所有功能宏观设计的**单一事实来源**。跨功能的任何决策以此为准。
> 每次修订都在末尾「§7 变更记录」追加一行。
>
> **状态：✅ 主计划用户 2026-07-30 已批准**（原话「批准local-as-remote」）。Phase A 已落盘，B 起开工。
> **本工作区落地的是 `doc/INVARIANTS.md` §40**（用户 2026-07-29 拍板的方向性约束）。
> 路线图第 ④ 项。

---

## §0.0 当前事实

**用户拍板的方向**（原话）：「我的目的就是把本地当成不走 ssh 的远端。**后面都要这么搞。**」
已记为 `doc/INVARIANTS.md` **§40**，本工作区只是它的落地。

**为什么现在这个方向是便宜的**（用户的观察，我核实后成立）：
今天的形态是 **Windows 上的 app 连 Linux 远端**。而远端那条路是 POSIX + tmux + `ccm`。
所以 **Linux 远端几乎无缝就是 Linux 本地——只差一跳 ssh**。Windows 本地不是，见 §0.2。

**已实测的起点（2026-07-29）**

| 项 | 实测 |
|---|---|
| `transport` 类型 | **已存在**：`src/launch-plan.ts:97,158` 的 `transport: {kind:"local"} \| {kind:"ssh"}`，零 payload 标记（`origin` 不进 transport，见该文件头注第 14 行）。**两个渲染器已经在按它分支** |
| `planLocal` | **生产零调用点**（`src/launch-requests.ts:139` 只剩一句注释记录 R07 删掉了那次 `buildLaunchPlan` 调用） |
| Windows 本地启动 | `src-tauri/src/history.rs:930` `build_local_ps_command`——只拼 `{bin} <resume_flag> <sid>` + 启动器别名。**不注入任何 env**：无 `CLAUDE_CONFIG_DIR`、无 `ANTHROPIC_MODEL`、无嵌套 env 清理。**不引用任何 IR 类型** |
| 账号功能的适用面 | **只对远端**。`accounts.rs:1` 自陈「A2 monitor 侧：**远端**多账号（cc-acct-iso）的**只读**查询命令」；`acct_iso_deploy.rs` 走 `connect_sftp`/`RemoteConfig`，**只往远端部署**。⇒ **Windows 本地会话压根没有账号概念** |
| Linux 应用打包 | **不存在**。`release.yml` 的 `ubuntu-latest` job 只做 **musl daemon 交叉编译** |
| Linux 测试基础设施 | **已经在了**：`ci.yml` 有 4 个 ubuntu job，**8 套真机 e2e 里 6 套跑在 ubuntu 上** |
| Windows-only 的代码 | `#[cfg(windows)]` / PowerShell / `wt.exe` 散在 `config.rs` `data_paths.rs` `auto_launch.rs` `session_map.rs` `sftp.rs` `utils.rs` `history.rs` `watcher.rs` 等（`launch.rs::launch_powershell_window`、`profile_installer.rs` 的 `$PROFILE`、`discover_profiles` 的 `dirs::document_dir()`、`ReplaceFileW` 的 ACL 保留） |

**顺手要修的一条历史过度声明**：`unify-launch/MASTERPLAN.md` 曾把 F06「本地路径并入 IR」
标为**完成**，2026-07-29 Phase G 文档-代码交叉对比证伪并已订正（F06 真正交付的是
「两套 PowerShell 拼装收成一个函数」）。**L2 做完之后那句话才第一次成真。**

---

## §0.1 目标与范围

- **总体目标**：让「起一个会话」在三种目标上是**同一条路 + 一个 transport 差异**：
  远端（ssh）· POSIX 本地（不走 ssh）· Windows 本地（唯一的显式例外）。

- **设计原则**（§40 的三条，此处不重述理由，只列判据）：
  1. 新增任何「起会话」能力，先问**能不能只是远端那条路少一跳 ssh**。能，就不许另起一套。
  2. **Windows 本地是唯一被允许的例外**，且必须是**类型上的显式分支**，不是「IR 管不到的地方」。
  3. **不许长出第三套**。要走 PowerShell 分支，得写下为什么这台机器上做不到 POSIX 那条。

- **范围内**：
  - `{kind:"local"}` 在 POSIX 上真正有含义（复用 `ccm` + tmux，不经 ssh）
  - Linux 应用可构建、可打包、可跑起来
  - `planLocal` 复活；Windows 本地进 IR（PowerShell 渲染器分支）
  - Windows-only 代码路径的显式化与收敛

- **范围外**：
  - **不改 `shared/ccm` 本体**（它在 POSIX 本地上已经够用：`--tmux`/`--detach`/`--account`/
    `--model`/预信任/身份回填全套）
  - **daemon 零改**（本工作区只碰启动路径，不碰会话监视）
  - **不新增轮询**
  - **不做 Windows 的账号支持**（= L3，只登记；理由见 §0.2 与 §6 风险 1）
  - 不做 macOS

- **整体成功标准**：
  1. **在 Linux 上能构建、能起 app、能起一个本地会话**，且那条会话经过 IR
     （`transport:{kind:"local"}` 有真实生产调用点）。
  2. **POSIX 本地与远端共用同一个 payload 编译路径**——判据：给同一个 plan 换 transport，
     除 ssh 包装外输出逐字节相同。
  3. **Windows 本地也经过 IR**，且 PowerShell 分支在类型上是显式的（`grep` 找不到
     「绕过 IR 直接拼本地命令」的第二处）。
  4. **`unify-launch/MASTERPLAN.md` 的 F06 那句话第一次为真**，且订正记录不再需要。
  5. 8 套真机套件 152 条断言在 Windows 上仍全绿（**L2 是主平台主路径，这条是硬门槛**）。

---

## §0.2 为什么 Linux 本地便宜、Windows 本地不便宜

| | POSIX 本地（L1） | Windows 本地（L2/L3） |
|---|---|---|
| 会话容器 | **tmux，已有** | **没有 tmux**。`wt.exe` 没有等价的多路复用 |
| 启动器 | **`ccm`，已有全套修饰** | PowerShell + `wt.exe`，无 `ccm` |
| 账号注入 | `ccm --account` **已有** | **cc-acct-iso 是 bash 且只往远端部署** ⇒ 一整块功能不适用 |
| 身份回填 | `ccm` 的 poller **已有** | 无 |
| 预信任 | `ccm` **已有** | 无 |
| 差异 | **只差一跳 ssh** | **结构不同** |

⇒ **L1 是「把已有的东西少走一跳」，L2 是「给一个结构不同的平台补一条分支」，
L3 是「给一个 bash 工具做 Windows 等价物」。** 三件事的成本差一个量级，所以拆开排。

**L2 拿不到什么**：账号注入与 per-account model 都依赖账号基础设施 ⇒ 归 L3。
**L2 能拿到什么**：① §40 的例外从「IR 管不到」变成类型上的显式分支
② **嵌套 env 清理在 Windows 本地首次生效**（今天没有——从一个 agent 会话里起本地会话
会泄漏继承的 agent env；这一条不需要任何账号基础设施）③ 给 L3 准备落点。

---

## §1 功能清单

| ID | 功能 | 一句话目标 | 状态 | 依赖 | 优先级 |
|----|------|-----------|------|------|--------|
| L0 | **Linux 可构建 + 可跑** | Tauri 在 Linux 上构建通过、app 能起、既有远端功能在 Linux 宿主上可用；WebKitGTK 依赖摸清 | 待规划 | — | **P0** |
| L1 | **POSIX 本地 = 不走 ssh 的远端** | `transport:{kind:"local"}` 在 POSIX 上有真实含义：复用 `ccm` + tmux，本地 exec 不经 ssh | 待规划 | L0 | **P0** |
| L2 | **Windows 本地进 IR** | `planLocal` 复活 + PowerShell 渲染器分支 honour `plan.env`；`build_local_ps_command` 变成该分支的实现而非平行世界 | 待规划 | L1 | P1 |
| **L3a** | **本地账号枚举（只读）** | Rust 直接读 `%USERPROFILE%\.claude-accts\accounts.json`（manifest 格式已定），`loggedIn` 由 `.credentials.json` 是否存在判定。**不需要 bash、不需要 cc-acct-iso** ⇒ 本地立刻有：账号列表 / 选号 / 账号注入 / per-account model | 待规划 | L2 | **P1** |
| L3b | **本地账号管理（写）** | 建 / 迁 / 删 / 改默认号。需要 cc-acct-iso 的 Windows 等价物（PowerShell 或 Rust 实现） | 待规划 | **`account-zero` 全部落地** + L3a | P2 |
| L4 | **Linux 打包 + 进 CI/release** | Linux 产物（AppImage/deb 择一）+ CI 构建 job；`release.yml` 加 Linux 产物 | 待规划 | L0,L1 | P2 |
| L5 | **平价对账表 + 门禁** | 枚举全部 **120** 个 Tauri 命令（**开工复测订正：计划原写 119**；以 `generate_handler!` 条目为准，实测 120 = 120、零缺口），每条要么两侧都有、要么在白名单表里且带理由；**新增命令不登记就红** | **Phase B 复测已做**（`features/L5-parity-ledger.md`），实现待续 | — | P1 |

### Windows 本地排期：原来的 L3 拆成 L3a / L3b（2026-07-29 用户要求排期后重新推导）

**上一版把「Windows 账号支持」当成一件事、硬依赖 `account-zero` 全部落地，那个判断太粗。**
重新推导：本地缺的其实是**两件性质完全不同的事**。

| | 缺的是什么 | 需要什么 | 依赖 |
|---|---|---|---|
| **L3a 枚举（读）** | 本地**没有账号列表的来源**。`fetchAccounts(origin)` → `list_remote_accounts(origin)` → 远端 `cc-acct-iso --list-accounts`，**这条链在本地没有对应物** ⇒ UI 拿不出账号可选 ⇒ `plan.env` 永远不会含 `export CLAUDE_CONFIG_DIR` | **只要读一个 JSON 文件**。manifest 格式已定（`accounts.json`：`version`/`sharedStore`/`acctsDir`/`accounts[]`，每项 `name`/`configDir`/`isDefault`），Rust 读它 + 检查 `.credentials.json` 是否存在 = 约百行。`email` 也照远端的做法**现读**该 config dir 的 `.claude.json` | **只依赖 L2**（要有地方 honour `plan.env`） |
| **L3b 管理（写）** | 本地无法**建 / 迁 / 删 / 改默认号** | cc-acct-iso 的 Windows 等价物。它要做 symlink 布局、隔离集搬迁、备份回滚 —— 是**一整个工具** | **`account-zero` 全部落地**（manifest 数据模型正在变） |

**这个拆分的意义**：用户能感受到的那部分（**选号、注入、per-account model 在本地生效**）
归 L3a，**不依赖 `account-zero`、不依赖任何 bash、可以排在 L2 之后立刻做**。
真正贵且必须等模型定稿的只是「在 Windows 上建账号」这件事。

**上一版为什么判错**：我把「账号功能」当成一个整体，于是它继承了最贵那部分的依赖。
按本会话反复用的那把尺子——**先数清现实里有几件事，再决定依赖**——它是两件。

### L3 的优先级由 P3 升到 P1（用户 2026-07-29 追加原则）

用户追加：「我们的原则是**本地的功能要和远程功能一致**（虽然现在远程是重点）。」
已记为 `doc/INVARIANTS.md` **§40 追加**。

这句话把 L3 的**性质**改了：它不再是「可选的平台移植」，而是**已核实的最大一处平价欠账**
——账号功能（列表 / 切号 / 按会话切号 / 用量 / per-account model）**在本地完全不存在**。

**但依赖关系不变，排期也不变**：L3 仍然硬依赖 `account-zero` 全部落地
（在一个正在变形的账号模型上做平台移植 = 本会话反复批评的形状），
且用户明说「现在远程是重点」⇒ **优先级升了，位置没变**。这两件事不矛盾：
优先级说的是「该不该还」，位置说的是「什么时候还」。

**L5 为什么是 P1 而不是 P3**：这条原则单靠人记不住。没有对账表，
「先做远端、本地以后再说」会像 `BACKLOG.md` 头注记的 U6→U8 那样**无声蒸发**。
L5 **不依赖任何其他功能**，可以立刻做，且做完之后 L1/L2/L3 的进度自动变得可度量。

**L0 为什么独立成一个功能**：它是**唯一可能推翻整个方向**的一步。WebKitGTK 的发行版碎片化
是 Tauri 的知名痛点；如果 L0 很痛，那**正是「要不要用 Rust GUI 重写前端」那个问题的真实数据点**
（见 `rust-ts-boundary/MASTERPLAN.md §0.0` 第 3 条）。所以 L0 的产出除了「能跑」，
还包括**一份如实的痛点记录**，交给重构决策用。

---

## §2 架构概览

```
                       ┌─ transport: {kind:"ssh"}    → bash -lic '<payload>' 经 ssh（launch.rs:133）
buildLaunchPlan ──▶ 渲染器 ─┼─ transport: {kind:"local"} + POSIX → 同一个 payload，本地 exec
   （维度注册表）           └─ transport: {kind:"local"} + Windows → PowerShell 分支（唯一例外）
```

**关键判断**

- **`payload` 是共享的，`transport` 只决定怎么送。** L1 的验收判据就是这一条：
  给同一个 plan 换 transport，除 ssh 包装外输出**逐字节相同**。
- **`ccm` 在 POSIX 本地和远端是同一个二进制、同一套 flag。** 所以 L1 不需要新的账号注入、
  不需要新的身份回填、不需要新的预信任——**这是「本地=远端」这个方向全部的收益来源**。
- **Windows 分支不是「另一个渲染器」，是同一个渲染器的一个 `match` 臂。**
  区别在于：另起一个渲染器 = 又一套可以静默漂移的实现；一个 `match` 臂 = 加维度时编译器会问你。
- **本工作区不碰 IR 的类型设计**（`LaunchAccount` 三态化归 `account-zero` Z02）。
  两个工作区都改 `launch-plan.ts`，见 §3 冲突协议。

---

## §3 ★共享面账本

| 共享面 | 涉及功能 | 最终形态设计 | 当前状态 | 备注 |
|---|---|---|---|---|
| **1. `src/launch-plan.ts` 的 `transport`** | L1,L2 | `{kind:"local"} \| {kind:"ssh"}` **保持零 payload**（不塞 `origin`、不塞 `platform`）。「本地是哪个平台」由**渲染时**的宿主决定，不进 IR——IR 是意图模型，不该知道宿主 | 类型已在，`{kind:"local"}` 无生产生产者 | **不加字段。**「加一个 `platform` 字段」是很自然的冲动，但那会让 IR 依赖宿主，破坏「同一个 plan 在两台机器上渲染出各自正确的命令」这个性质 |
| **2. 兜底渲染器 `src/launch-render-fallback.ts`** | L1,L2 | `container.kind==="none"` 与 tmux 两条既有分支保持；新增「Windows 本地」臂。**必须是穷尽 `match`**（照 `renderEnvOps` 里那个 `_exhaustive: never` 的做法，R04 Phase D 审计的成果） | 今天只编译 POSIX 形态 | 那个 `never` 穷尽守卫是本仓最好的一类结构保证之一，L2 必须沿用而不是绕开 |
| **3. `src-tauri/src/history.rs::build_local_ps_command`** | L2 | 从「平行世界」变成「PowerShell 分支的实现」：**接收编译好的 `plan.env` + argv**，不再自己决定 resume flag / 启动器别名 | 独立构造，不引用任何 IR 类型，自带一份 sid 校验 | 它的 sid 校验（`refuse resume: invalid session_id`）**要保留**——那是一道独立防线，不是重复 |
| **4. `src-tauri/src/launch.rs` 的传输包装** | L1,L2 | 三个送法：ssh（`bash -lic` 包）· POSIX 本地（直接 exec，**不要 ssh 包**）· Windows（PowerShell）。`shell_quote` 那两层校验（TS 渲染时 + Rust `launch.rs:213` 再验）**对三条路一律适用** | 只有 ssh + PowerShell 两条 | `shell_quote` 顺带该搬去 `utils.rs`（BACKLOG **E31**：5 个模块只为它依赖 4847 行的 `ssh_source.rs`）。**L1 顺手收掉** |
| **5. `ci.yml` / `release.yml`** | L0,L4 | 新增 Linux 构建 job；`release.yml` 加 Linux 产物。**不改任何触发条件** | 4 windows + 4 ubuntu job（ubuntu 只做 musl daemon 交叉编译 + e2e） | 与 `gate-integrity` 和 `rust-ts-boundary` **同时在改 `ci.yml`** ⇒ 冲突协议见下 |
| **6. `#[cfg(windows)]` 的分布** | L0,L2 | 每一处要么有 POSIX 对应实现，要么有一条**写下来的**「这台机器上做不到」。**不允许静默的平台缺口** | 散在 8+ 个模块 | L0 的产出之一是这张清单本身 |

### 跨工作区冲突协议

| 文件 | 谁改 | 协议 |
|---|---|---|
| `ci.yml` | `gate-integrity`（全部）· `rust-ts-boundary` C05 · 本区 L0/L4 | **`gate-integrity` 最先**（它就是干这个的）→ `rust-ts-boundary` C05 → 本区。**后到者只追加，不重排既有步骤** |
| `src/launch-plan.ts` | `account-zero` Z02（`LaunchAccount` 三态化）· 本区 L1/L2（`transport`） | **两者改的是不同字段**（`account` vs `transport`），可并行；但**谁先落地谁负责把另一方的类型形状留着**。建议 `account-zero` Z02 先，因为它改的是判别联合的变体数（影响穷尽 `match`） |
| `shell_quote` 搬家（E31） | 本区 L1 | 本区做。`rust-ts-boundary` 不碰 |

---

## §4 依赖图与实现顺序

```
L5（平价对账表）        独立，先做——它让后面全部进度可度量

L0（Linux 可构建可跑）── L1（POSIX 本地）──┬── L2（Windows 本地进 IR）──┬── L3a（本地账号枚举·读）
                                          │                           └── L3b（本地账号管理·写）
                                          └── L4（Linux 打包进 CI）           ↑
                                                        L3b 硬依赖 account-zero 全部落地
```

**只有 L3b 一个功能被 `account-zero` 卡住。** L5 / L0 / L1 / L2 / L3a / L4 六个功能
**不依赖 `account-zero`、不依赖任何外部授权**，可以连续跑完。

**顺序与理由**：

1. **L0 先，因为它可能推翻方向。** WebKitGTK 若很痛，后面全部要重排，而且那是重构决策的输入。
   **早失败便宜。**
2. **L1 次之，它是本方向全部收益的来源**，且是**新增而非修改**（今天没有 POSIX 本地路径），
   风险最低、可回退。它同时给 `transport` 这个标记提供**第二个真实消费者**——
   在此之前 `{kind:"local"}` 是零生产者，谈不上抽象。
3. **L2 在 L1 之后。** 三条理由：① 它改的是**主平台的主路径**（Windows 本地会话启动），
   风险最高；② 在 L1 之前做，等于在只有一个实例的情况下设计那个 `match`——
   本会话那把 ≥2 尺子说的正是这个；③ L1 会先把「payload 共享、transport 只管送」
   这个假设验证掉。
4. **L4 与 L2 可并行**（一个碰 CI，一个碰渲染器）。
5. **L3 不排期**，等 `account-zero` 全部落地。

---

## §5 横切关注点与约定

- 不用 emoji · commit 不加 `Co-Authored-By` · `git add` 显式文件清单。
- **门禁基线**（开工时）：`cargo test --all` **536** · `code-picture-core` **25** ·
  `npm test` **814 / 53 files** · clippy 0 · tsc 0 · `npm audit` rc=0 · shellcheck 0 ·
  exec-bit rc=0 · **8 套真机套件 26/44/12/15/13/21/14/7 = 152 条**。
- **L2 的硬门槛：Windows 上 8 套 152 条全绿。** 它改主平台主路径，
  「在 Linux 上跑绿了」不算验收。
- **Linux 上跑真机套件时的 tmux 纪律**（本机 aya 就是 Linux，风险最高）：
  一律走强制 `-L` 的守卫 shim（无 `-L`/`-S` 一律拒）+ 起飞前 canary 双向自检 +
  跑完核对默认 socket 会话清单逐字未变。**裸 `tmux kill-server` 是禁用词**——
  默认 socket 上住着真实的 CC 实例。
- **绝不启动真实已认证的 `claude`/`codex` 子进程。** 启动器一律注入假的（纯 sleep 脚本）。
- **绝不写 `~/.claude/settings.json`、`~/.bashrc`、任何 PowerShell profile**
  （tempdir 可以，但要有 `Drop` 自清）。
- **测试纪律**（逐条适用）：变异**先 diff 确认落位、再确认它编译得过**，然后才判色 ·
  反向自检 · 计数自检用 `==` 不用 `>=` · **守卫范围恰好等于性质范围**（栽过三次）·
  **源码文本扫描 ≠ 行为测试** · **代理指标 ≠ 性质**（顺序/长相/文本扫描三种代理各栽过一次）。

---

## §6 风险与开放问题

**风险**

1. **L3 的模型还在变。** `account-zero` 正在改 cc-acct-iso 的 manifest 数据模型
   （要能表达「不注入」）。**在一个正在变形的模型上做平台移植**是本会话反复批评的形状
   ⇒ L3 硬依赖 `account-zero` 全部落地，不得提前。
2. **WebKitGTK 碎片化**（L0 的主要不确定性）。缓解：L0 的验收包含一份如实的痛点记录，
   并且**不试图支持所有发行版**——先在 aya 这台机器上跑通。
3. **L2 动主平台主路径。** 缓解：L1 先验证假设 + Windows 上 8 套 152 条作为硬门槛 +
   `build_local_ps_command` 的 sid 校验保留不动。
4. **本机就是目标平台（aya = Linux）**，所以 L1 的开发/测试会在一台**住着真实 CC 实例、
   真实 `~/.cc-bus/`、真实 `~/.claude-accts/`** 的机器上进行。这是本工作区最高的操作风险，
   纪律见 §5。
5. **「加一个 `platform` 字段进 IR」的冲动。** 见共享面 1：那会破坏
   「同一个 plan 在两台机器上渲染出各自正确的命令」。**这条要在 L2 的 DoD 里显式禁止。**

**待用户确认的开放问题**

| # | 问题 | 我的建议 |
|---|---|---|
| 1 | L0 在哪台机器上做？aya 本机（有真实 CC 实例）还是另开一个干净环境？ | 建议**先在 aya 上只做「能构建」**（`cargo build` + `npm run build`，不起 app），
起 app 那步再决定。理由：起 app 会碰真实配置 |
| 2 | Linux 产物格式（L4）：AppImage / deb / 两个都要？ | 建议 **AppImage 先**（单文件、不管发行版依赖），deb 以后再说 |
| 3 | L2 做完后，`build_local_ps_command` 的调用点（`resume_impl` 等）要不要一起改？ | 建议**保留旧函数名与调用点**（它头注自己写着「薄委托——保留旧函数名与调用点不变」），只换内部实现。理由：那是既有 DoD 的要求，且减少改动面 |
| 4 | L3 的形态：PowerShell 版 cc-acct-iso，还是把 manifest 逻辑用 Rust 实现一次？ | **现在不定。** 等 `account-zero` 把模型定下来再谈——现在定就是在变形的模型上决策 |

---

## §7 变更记录

- 01 — 2026-07-29 — 初版，Phase A 主规划完成 — 落地 `doc/INVARIANTS.md` §40。
  用户观察「现在都是 windows 连 linux 远端，因此 linux 远端能无缝变成 linux 本地，
  但 windows 本地可能没那么好」促成 L1/L2/L3 三级拆分；
  核实后确认 Windows 本地**连账号概念都没有**（`accounts.rs:1` 自陈只管远端、
  `acct_iso_deploy.rs` 只往远端部署），故 L3 独立并硬依赖 `account-zero`。等用户审批。
