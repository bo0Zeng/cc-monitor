# 状态 / STATUS — unified-backend（恢复入口，每次先读这里）

- **当前阶段**：**U-1 的 Phase C/D 已闭环 + 文档面已补齐 + Phase E 进行中**（2026-08-01 08:2x 用户手动续跑，07:22 那次唤醒未用上）。
- **当前功能**：**U-1 护栏当下缺陷修复**（第零梯队，v3 新增）

## 续跑记录（2026-08-01 08:2x）—— 暂停点四件事的进度

**代码已全部落地且门禁全绿**（daemon `cargo test` **193 passed** / monitor `--lib` **661 passed** / tsc 0 / vitest 79 文件 1148 例 / fmt 两侧 OK，均 RC=0）。工作区未提交，`git status` 见下。

1. ✅ **补文档面**（Phase D 审计 I4）—— 五处全改完：`doc/RELEASING.md:22`（warning→三种 panic + 必须同步 `.build_id`）· `doc/REMOTE-PHASE0-DEPLOY.md:15-40`（手工打包必写清单 + rust-lld 零安装路线及其**不等价于 CI 产物**的限定 + 三种 panic 表）· `doc/CONTRIBUTING.md:82`（现为机器强制）· `doc/INVARIANTS.md:1188`（「扫全 crate 生产段」改动前是假的，加订正标注）· `doc/INVARIANTS.md §41.4`（两条派生纪律→**三条**，新增「剥法必须同时防欠剥与过剥」，含 B1 完整病历 + 「同一个坑 `readonly_guard` 早填过、另外三处没跟」的元教训）。
   > 写文档时**当场订正了自己一句过头话**：初稿写「那段 dispatch 从来没被任何一条守卫扫过」，去读 `readonly_guard.rs:12-22` 才发现它 2025 年就用按括号配平逐块剥、注释里明写「不能简单从首个 `#[cfg(test)]` 截断到 EOF」——**它扫到了**。改成只点 `no_timer_guard` 一族，并把这个反差写成元教训。
2. ✅ **更新 feature 文件** —— 补 Phase D 全部结果（阻塞 B1 + 重要 I1-I4 + 两个我自造的坑 + 5 条建议）、登记**偏离④**（地板判据从「数量相等」改成「字节下限」当时漏记，现已两条**都**上）、订正变异表第 1 行的 `tmp_probe`/`tmux_probe` 笔误。
   > 笔误**不靠回忆订正**——重跑了一次那个变异取实测（RC=101，诊断逐字 `tmp_probe/probe.rs`），顺带发现原记录漏了**第二条测试**（`every_duration_use_is_registered_as_non_timer`）也会一起红。跑完 `rm -rf` + `touch` 还原，复验 193 绿。
3. ✅ **Phase E 工程审计**（主线程对账 + 1 个聚焦 agent）。**潜在阻塞排除**：`release.yml:56-58` 的 build-daemons **确实写 `.build_id` 清单**（从源码 `grep -oP` 抠），`build-windows:113-118` 还会再对拍 ⇒ 三条 panic 在发版链上够不着，发版不会被打挂。阻塞 0 项、重要 5 项**全部当轮修完**（详见 feature 文件「工程审计结果」）。账本 **S1/S2/S8/S13 已落账**。
   > **最要紧的一条**：`readonly_guard::strip_cfg_test` 有两条**过剥（fail-open）** —— ① 锚点没钉行首，`main.rs:23` 行尾注释里的 `#[cfg(test)]` 就能起跳；② 无花括号体声明会吃掉后文，**触发它的正是我这轮加的 `#[cfg(test)] mod guard_support;`**。合起来让 `main.rs:23–40`（15 条 `mod` + 2 条 `use`）从来不在扫描面里。两条一起修，扫描面 217_853→221_928。判定证据：同一处 `fs::write` 探针，旧剥法**假绿**、新剥法 **RC=101**。
   > **审计有一半说错了，我核实后订正**：它说「只有 `guard_support.rs` 一个文件残留 5 个 `#[test]`、其余 15 个为 0」——HEAD 就有 4 个文件残留（watcher 76 / accounts_query 25 / resolve_query 10 / history_query 4），我的文件是第五个也是最小的。
   > **另外我自己抄错了一个数**：字节基线注释写 119_454，用护栏自己的口径实测是 **121_131**（审计对）。已订正，并把复测办法写进注释，不再手抄。
4. ⏳ **commit**（显式 `git add` 文件清单，**绝不 `-A`** —— `src-tauri/crates/branch-core/Cargo.lock` 是未跟踪且**不在 gitignore 里**的新文件，别误加）。然后推进 **U0**。

**本轮新增的计划改动（用户 tmux 命名提问触发）**：
用户先问「是不是还有命名漂移」，我按**仓库级**答了（四套并存、`monitor`/`cc-monitor`/`ccmonitor` 三拼）；
用户澄清**问的是 tmux 会话名、要后缀 `-cc`、其余不动** ⇒ 我上一版答复答偏了问题。
按澄清后的范围复查：`-cc` 早在 2026-07-31 S4b-3b 就反转过了，但**漏了最后一个生产生成点**
（`launch-requests.ts:67` 的默认值仍是 `cc-<sid8>`；线上没冒出来只因三条调用路径碰巧都传显式 name，
而它是**公开导出 API 的默认值**）。已修 + 加「默认名 == `pickFreshTmuxName` 基名」对拍断言 +
修 9 处仍在描述旧形态的注释（其中 `pickFreshTmuxName` 的头注**与它自己下一行的代码自相矛盾**）。
`cc-` 的**识别**半支保留不动（老会话没有 `@ccm_sid`，删了就是把用户正在跑的会话变成失管会话）。
§5 D3 已从「待拍板」移进「已闭合」；仓库级命名作为观察留在 U13 行里，动手前需单独拍板。

**已改动文件（`git status`）**：
```
 M .claude/planned-build/README.md
 M remote-daemon-proto/src/{accounts_query,build_id_guard,main,no_timer_guard}.rs
 M src-tauri/build.rs
 M src-tauri/src/{parity_ledger,ssh_source}.rs
?? .claude/planned-build/unified-backend/
?? remote-daemon-proto/src/guard_support.rs
?? src-tauri/crates/branch-core/Cargo.lock   ← 不是本功能产物，别 add
```
另：`src-tauri/embedded-daemons/` 下四个文件被替换（gitignore，不进 git）。

**Phase D 第一轮三视角结论**：一条阻塞（**我这次改动自己制造的**：`#[cfg(test)]\nmod guard_support;` 是无花括号体的声明，被旧剥法当成模块体，把 `main.rs:26-179` 整段吞掉 ⇒ `no_timer_guard` 在那一段静默变瞎。两份审计独立命中）—— **已根治**（要求那一行以 `{` 收尾）+ 两条回归钉 + 一条语义钉。产物审计**零阻塞**：两个内嵌二进制的 `p1v-attachable` 已**反汇编到指令级证实**。

**还欠的（已登记，不在 U-1 范围）**：`#[cfg(test)]` 修饰的**自由函数**（`history_query.rs:232/309`）永远不被剥且 `assert_no_test_code` 检不出 · 共享剥法只收敛 8 处里的 3 处（其余 5 处当下安全，U2/U3 搬文件时会咬） · 三处新递归都跟随符号链接 · aarch64 那份内嵌二进制**从未被执行过**（本机无 qemu）· 建议在 `.build_id` 清单里存一份 sha256 把「清单↔字节」焊死。
- **待拍板**：**1 条 —— D1「间接写算不算违反 §41.6」**（见 `MASTERPLAN.md §5`）。它不阻塞 U-1/U0/U1a/U2/U3，最晚在 **U8a 之前**必须有答案。我给了推荐（①收窄铁律 + 把预信任单列为受管例外），若无异议我按推荐走并在 U8a 的 feature 文件里再显著标一次。

## 计划自审结论（2026-08-01，四视角并行）

产物：[`PHASE-A-计划自审-四视角.md`](PHASE-A-计划自审-四视角.md)。**自审打掉我 10 处错**（`MASTERPLAN.md §0.5` 逐条留档），其中三处是**安慰剂式的机检或门禁**：

1. **U0 的前提是错的** —— 五套 `ccm-*` e2e 早有地板，且 CI 有逐对校验的元门禁（`gate-integrity` 的 G-A/G-C 早已收官）。**第一梯队本来是空的。**病根：把摸底 C 转述的陈旧 BACKLOG 条目当现状，而我同一轮开头才读过写着「✅ 全部完成」的工作区索引。
2. **U2 的 cfg 位置机检抓不到它自己引的 bug** —— `pidfd_open` 根本没有 cfg。真判据只有跨 target `--all-targets` 编译（**12 个错，不是 11**）。
3. **S11 的诊断是反的** —— `ccm_cli_has_required_elements` 已有计数自检，ccm 掏空后是**四路一起红**，不是空转变绿。

**顺带查出三个当下就坏着的东西**（不是计划问题，是仓里的）：`no_timer_guard` 非递归（拆模块后会**空转变绿**，而我的硬门槛只查条数、查不出扫描面归零）· 三个守卫共用的测试段 marker 对 `main.rs` 不匹配（**284 行测试段正被当生产段扫**，反向自检结构上检不出）· 内嵌 daemon 处于**半 bump**（源码 p1v / 清单 p1u，是我 bump 了没 re-zigbuild）。

⇒ v3 新增**第零梯队 U-1** 专修这三样；U0 重定义；U1 拆 a/b；U7 拆五、U8 拆四；重命名独立成 U13；文档改动**分散进各功能 DoD**；`IPC-PROTOCOL.md`（七处在说谎）并进 U6「先修再冻结」。

## 自动模式（用户 2026-08-01：「全面审计计划, 全自动做完. 文档工程和代码工程都要管理好」）

- **档位 = 全自动连续跑**。主计划 + 各功能计划**已预批**，loop 连续 B→G，**不在功能门禁停**。
- **本轮 loop 目标**：把流水线推进一个有检查点的单位（通常一个功能走完 C→F）。每轮停在干净检查点（`STATUS.md` 已更新 + 一个独立 commit）。
- **loop 停止条件（任一即停，交回用户）**：
  1. 真阻塞 / 计划与现实冲突需要新决策（铁律 4：停下改计划，绝不默默打补丁）
  2. **同一步 ≥2 次失败**
  3. 门禁红且非在途变异
  4. 需要动用户家目录真实数据、或需要用户在真机上手动做的事
  5. 全部功能完成 → 先跑 Phase G → 再停
- **绝不为跑完而**：放宽红线 · 改用户家目录里的真实数据 · 把没做的标成做完 · 为让守卫变绿而删信号源 · 为过关而 +1 守卫钉死的计数。
- **用户额外强调**：「**文档工程和代码工程都要管理好**」⇒ 文档面与工程面（CI / 门禁 / 测试地板 / lint 覆盖）**不是 U13 的收尾附赠**，每个功能的 DoD 都要各带一条。
- **形态选型（v2.1，2026-08-01）**：**把 cc-monitor 拆成 frontend + backend**。
  backend = 读（observe）+ 控制（control），**一份代码、两种承载**（本机进程 / 远端进程），本地与远端同一套分解。
  用户定框原话：「you can just regard it as we decomposed the monitor. we separate it into 2 functions, read and control. this is the same at local.」
  - **v1 的「决策内核 crate + 三宿主」已被用户否决**（「否则分了个 crate 和宿主出来反而架构不清」），留档在 `MASTERPLAN.md §0.1`。
  - daemon 内部三条解耦线：`platform/`（唯一允许平台 cfg）· `observe/` · `control/`；
    **护栏跟着模块边界走** —— `readonly_guard` 钉死在 `observe/`（不放宽），`control/` 单立窄写护栏。
  - **两种生命周期不许假装一样**（`§1.2b`）：本机 backend 由 frontend 拥有（启停/监督/自愈）；远端 backend 自治。
  - 已定三条：下行通道 **长连接双向** · 超时 **一律推给客户端**（零定时器铁律不改，登记表仍 1 条） · Windows 账号 **daemon 自己起的就记账**。

## Phase A 产物

- [`MASTERPLAN.md`](MASTERPLAN.md) — 主计划（待批）
- [`PHASE-A-摸底-A-起停接会话全路径.md`](PHASE-A-摸底-A-起停接会话全路径.md) — 19 条路，无一经 daemon
- [`PHASE-A-摸底-B-daemon平台与通道.md`](PHASE-A-摸底-B-daemon平台与通道.md) — 平台依赖 / 四条护栏 / 无下行通道
- [`PHASE-A-摸底-C-ccm能力盘点.md`](PHASE-A-摸底-C-ccm能力盘点.md) — 662 行里只有 6 处搬不走
- [`PHASE-A-摸底-D-文档改写清单.md`](PHASE-A-摸底-D-文档改写清单.md) — 文档 A–J + 必须保住的 20 条

## Phase A 主线程亲自复核过的 7 条（不是转述 agent）

1. ✅ daemon 在 Windows 上**编不过**：`cargo check --target x86_64-pc-windows-msvc`，**RC=101，11 个错误**，全在 `watcher.rs:156-166`+`211-262`（pidfd/poll），其余全过。
2. ✅ `pid_alive` 非 Linux **恒返回 `true`**（`watcher.rs:1422-1428`）—— 静默错误地雷。
3. ✅ **Linux 宿主上远端终端拉起 100% 失败**：`launch.rs:304` 无条件调 `launch_powershell_window`，后者非 Windows 直接 `Err`（`:268-271`）。**真 bug，非本区制造。**
4. ✅ 会话名漂移：daemon 发 `cc-<sid8>`（`resolve_query.rs:251`）vs monitor 造 `<sid8>-cc`（`remote-launch.ts:157`）—— 对 aterm 是**现网 bug**。
5. ✅ `no_timer_guard` 登记表**恰好 1 条**（`no_timer_guard.rs:63-68`）。
6. ✅ `parity_ledger` `LEDGER.len() == 123`（`local-as-remote/MASTERPLAN.md:119` 写的 120 已过期）。
7. ✅ `ccm-aliases.sh` **只有 3 个别名**（`unify-launch/MASTERPLAN.md:252` 承诺的 8 个从未存在）；`__ccm_rbind` **无定义**却仍有 10 处引用，其中 `launch-plan.ts:92` 拿它给 IR 的 `prelude` 字段做说明。

## 这个工作区要做什么

**把「后端」集成成一个。** 今天「起 / 停 / 接一个会话」有多条各写各的路：
cc-monitor 经 ssh 跑 `shared/ccm`（1300 行 bash）· 用户在终端里直接敲 `ccm` ·
本机走 PowerShell + `wt.exe`（完全不经 IR）· monitor 自己开 ssh 跑 `tmux kill-session` ·
cc-bus 的 spawn 又一套。而 daemon 至今是**纯观察者**，一条执行路都不经它。

## 用户拍板（2026-08-01，原话记录）

| # | 原话 | 影响 |
|---|---|---|
| 1 | 「**要用 ccm 就必须装 daemon. 没有 daemon 啥都读不了了还搞什么**」 | **砍掉 daemonless 降级** —— 这是 BACKLOG **E53** 列的头号前置。`ccm` 可以放心退化成 daemon 的薄客户端，逻辑只留一份 |
| 2 | 「**windows 也要搞成本地即不通过 ssh 的远程**」<br>**追加澄清（同日）**：「**windows 没有 tmux. 我只是说后端要做干净, windows 上的后端肯定不搞 tmux**」 | 说的是**架构**要统一，**不是**要把 tmux 弄上 Windows。⇒ 「会话容器」是一个**维度**而不是一个实现：Linux/远端 = tmux，Windows = 别的（甚至可能「没有容器，就是一个进程」）。这正是 issue **#48**「抽象会话后端，tmux 降级为其一」。IR 里 `container` 本来就是维度（`{kind:"tmux",…}`），不是硬编码。<br>**仍要答的**：Windows 上那个「后端」由谁承载（本机 daemon？monitor 自己的 Rust 侧？），以及它与远端那条**共享哪一层**——共享「计划/契约」还是连执行器也共享。动到 `INVARIANTS §40` 的「Windows 例外」段与 §36 |
| 3 | 「**记得全部搞一下文档**」 | 文档面进 DoD，不是收尾附赠。本仓已多次吃过「改完代码文档开始说谎」的亏 |
| 4 | 「**重复审计，要把架构做干净**」 | D/E 两道审计按**高风险**档跑（动核心 + 动共享面 + 动跨语言契约），且不止一轮 |

## 这条早就记着（不是新提议）

- **BACKLOG E53**：「`cc` 改为调 daemon 开会话」——**用户 2026-07-31 已拍板**，状态「未做，方向已定」。
  它自己写着：「真正要处理的是**语义转向：daemon 从观察者变成执行者**」，且「与 `#77` 是同一个方向，应合并考虑」。
- **BACKLOG E49**：原始提议（用户原话「cc 不该自己在 bashrc 搞，应该去调用 daemon 生成会话」），
  并记着我当场被纠正的一处错——我曾说它撞「daemon 只读」铁律，**那是错的**：`readonly_guard`
  守的是 daemon 源码里不许有**文件系统写**，`tmux new-session` 是起进程，护栏拦不到。
- **issue #77**：「起会话 / 拉回话 / resume / attach 等会话生命周期，交给 cc-monitor 本身或 daemon」。
- **issue #82**（控制模式住进 daemon）里明写：「**需解封「daemon 零改」约束**」。

⇒ **本区是 E53 + #77 的落地区**，并把 #82 的前置（解封 daemon）一并处理。

## 做成之后能一次性收掉的

- **E76**（`@ccm_sid` 写入不触发重探）—— 「贴便签的和看便签的是两个进程」这个前提没了。
- **E49** 的跨语言双写点（`derive_tmux_name` shell ↔ `deriveTmuxName` TS）。
- **#76**（tmux 命名冲突 / 孤儿）里那句「来源需全面排查**所有**起 tmux 会话的地方」—— 只剩一个来源。
- **#72**（monitor 自己 Resume 建的会话不设 `@ccm_sid`，producer 侧缺口）。
- **E79 的显示侧**（「起会话与显示读同一处」）—— 后端只有一个之后，事实源自然只有一处。

## 已知的硬约束（开工前就在册，Phase A 要逐条对账）

- `TMUX_LS_FMT` 六列格式不改 · `RETIRE_MISS_THRESHOLD >= 2` 不动。
- daemon 零定时器（§41，`no_timer_guard` 钉住）—— 变执行者之后这条**怎么重写**要先想清。
- `readonly_guard` 两层（G2 收窄完毕）—— 「起进程」不在它管辖内，但边界要写进文档。
- tmux 破坏性命令三道门（§34）、「绝不向用户自己的其它 tmux 会话发按键」（§1 A5）—— 与本变更**正交，必须保住**。
- 不写用户的 `~/.claude/settings.json` / `~/.bashrc` / PowerShell profile / `~/.tmux.conf`（除既有 opt-in 的 BEGIN/END 块）。

## loop / 自动度

尚未设定 —— 主计划审批时一并定。

## 时间线

- 2026-08-01 建区，Phase A 摸底（四视角并行）发起。
