# 状态 / STATUS — local-as-remote（恢复工作的入口，每次先读这里）

- **当前阶段**：**A 主规划已落盘，等用户审批**
- **当前功能**：无（L5 与 L0 待批准后开工）
- **当前步骤**：n/a
- **已完成功能**：无
- **下一个功能**：**L5（平价对账表，独立可先做）** 与 **L0（Linux 可构建可跑）** 并行
- **阻塞 / 待用户确认**：
  - **[待批准] 主计划**
  - **[已批准的方向]** 用户 2026-07-29：「把本地当成不走 ssh 的远端。**后面都要这么搞。**」
    + 追加：「本地的功能要和远程功能一致（**虽然现在远程是重点**）。」
    → 两条都已记为 `doc/INVARIANTS.md` **§40** 与 **§40 追加**
  - **[待定] L0 在哪台机器上做** —— 建议先在 aya 上只做「能构建」（`cargo build` + `npm run build`，
    不起 app）；起 app 那步再决定，因为它会碰真实配置
  - **[待定] Linux 产物格式** —— 建议 AppImage 先（单文件、不管发行版依赖）
  - **[待定] L2 后 `build_local_ps_command` 的调用点要不要一起改** —— 建议保留旧函数名与调用点
    （它头注自己写着「薄委托——保留旧函数名与调用点不变」），只换内部实现
  - **[不现在定] L3 的形态**（PowerShell 版 cc-acct-iso vs Rust 实现 manifest）——
    等 `account-zero` 把模型定下来。现在定就是在变形的模型上决策
- **最近一次计划回看时间**：2026-07-29（Phase A 落盘 + 按用户追加的平价原则调整 L3/L5）
- **自动模式（/loop）**：未起
- **备注**：
  - **路线图第 ④ 项。本工作区就是 `doc/INVARIANTS.md` §40 的落地。**
  - **起点比预想的好**：`transport: {kind:"local"} | {kind:"ssh"}` 这个类型**已经存在**
    （`src/launch-plan.ts:97,158`，零 payload 标记），两个渲染器**已经在按它分支**。
    所以 L1 不是新建抽象，是**让 `{kind:"local"}` 真正有含义**（今天 `planLocal` 生产零调用点）。
  - **三级拆分的理由**（用户的观察，核实后成立）：今天是 Windows app 连 Linux 远端，
    而远端那条路是 POSIX + tmux + `ccm` ⇒ **Linux 远端几乎无缝就是 Linux 本地，只差一跳 ssh**；
    **Windows 本地结构不同**（没 tmux、没 `ccm`、cc-acct-iso 是 bash 只往远端部署）。
  - **已核实的最大平价缺口**：**账号功能在本地完全不存在**
    （`accounts.rs:1` 自陈只管远端 · `acct_iso_deploy.rs` 走 `connect_sftp` 只往远端部署 ·
    `history.rs:930` 不注入任何 env）。**L3 因此从 P3 升到 P1，但位置不变**
    （硬依赖 `account-zero` 全部落地 + 用户明说现在远程是重点）。
  - **L0 是唯一可能推翻整个方向的一步**（WebKitGTK 碎片化）。它的产出除了「能跑」，
    还包括**一份如实的痛点记录，交给「要不要用 Rust GUI 重写前端」那个决策用**。
  - **最高操作风险**：aya 本机就是目标平台，默认 tmux socket 上住着真实 CC 实例
    （`cc-9d66c46d` / `cc-claudecode-frontend` / `cc-d7692cdf`）+ 真实 `~/.cc-bus/`、
    `~/.claude-accts/`。一律走强制 `-L` 的守卫 shim + 起飞前 canary 双向自检 +
    跑完核对会话清单逐字未变。**裸 `tmux kill-server` 是禁用词。**
  - **L2 的硬门槛**：它改主平台主路径 ⇒ **Windows 上 8 套 152 条全绿**才算验收，
    「在 Linux 上跑绿了」不算。
  - **一条要显式禁止的冲动**：往 IR 里加 `platform` 字段。那会破坏
    「同一个 plan 在两台机器上渲染出各自正确的命令」这个性质。见 §3 共享面 1。
  - **顺手要收**：`shell_quote` 搬去 `utils.rs`（BACKLOG **E31**——5 个模块只为它依赖
    4847 行的 `ssh_source.rs`），归 L1。
  - **L2 做完后**，`unify-launch/MASTERPLAN.md` 的 F06「本地路径并入 IR」那句话**第一次为真**
    （Phase G 交叉对比证伪并已订正过一次）。
