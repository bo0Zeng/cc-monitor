# 状态 / STATUS — unify-launch（恢复工作的入口，每次先读这里）

- **当前阶段**：F 回看更新**已完成**，F01 签收。下一轮从 F02 的 Phase B 起
- **当前功能**：无（F01 已闭环）
- **已完成功能**：**F01 tmux 目标精确匹配**（B→F 全过，双 agent 审计无阻塞、7 项重要发现全修）
- **下一个功能**：**F02 统一启动 CLI `ccm`** —— 核心思想的载体，也是 F03 IR 的唯一主渲染目标
- **阻塞 / 待用户确认**：无
- **最近一次计划回看时间**：2026-07-27（MASTERPLAN 变更记录 03）
- **自动模式（/loop）**：**全自动**（连续 B→G），附加双 agent 审门禁
- **本轮 loop 目标**：F02 走完 B→F 并 commit
- **loop 停止条件**：计划≠现实 / 同一步≥2 失败 / 全部完成→Phase G

## F01 结果摘要

- 修的是**今天正在损坏数据**的生产 bug：裸 `-t <名>` 是「精确→名字开头→glob」三级解析，
  换号重启会把 `/exit` 敲进兄弟会话里还活着的 claude 并 kill 它，而 UI 报告「已重启」。
- 统一为 `=名:`（**尾冒号不可省**——`=名` 在 send-keys/capture-pane/set-option 上 rc=1 完全失效）。
- 新立 `doc/INVARIANTS.md §31a`（三处同源 + 漂移守卫）。
- 新增常设门禁 **`npm run test:tmux-target`**（26 项真机行为验收，输入取自真 builder）。
- 门禁：tsc 0 / `npm test` 41 文件 598 测试 + 13 个 tsx 套件 / `cargo test` 369 /
  e2e restart-suite 24 ok · resume-suite 17 ok · resume-daemon-frames 7 ok。

## 已拍板的决策（2026-07-27）

| 决策 | 结论 |
|---|---|
| 坏 P0 工作区 | **整体 revert 回干净基线** —— 已执行 |
| 越层启动器调和 | **只诊断，不自动降格**。根治靠 F02 参数化 CLI：`ccm [动作] [修饰]` + 用户别名组合 |
| 本地路径 | **纳入本轮**（F06） |
| 自动度 | **全自动**，但每个重要架构必须过后端架构 agent + UX agent 两道独立审 |

## 双 agent 审门禁（用户 2026-07-27 指定）

除 Phase D 常规审计外，**架构承载型功能**（F02 / F03 / F04 / F05 / F06 / F07 / F09）必须额外过：

1. **后端架构 agent** —— 把握 MASTERPLAN §0 核心思想，审后端架构是否被破坏、是否留够扩展空间。
2. **UX agent** —— 把握同一份核心思想，审交互是否**真的收敛**（而不是换个地方堆复杂度）。

两个 agent 的 prompt 必须**自包含**且**带上 §0 核心思想全文**（它们看不到对话历史）。
F01 是纯 bug 修复，已走常规双视角 Phase D（正确性 + 计划/架构符合度）。

## 备注

- 主计划 = `MASTERPLAN.md`（**先读 §0 核心思想**）；入口全量清单与验收矩阵 = `INVENTORY.md`。
- 四视角审计原文在 `../account-onboarding/AUDIT-v2-FINDINGS.md`（本计划反复引用其 C1-C7 / D1-D9 / E1-E9 / P1-P3）。
- **已由真机证据关闭的旧计划项**：D7（wrapper 账号感知）证伪为 no-op；rbind 门控「未部署会话立死」证伪。
- **门禁教训（已写进 MASTERPLAN §5.2）**：
  1. `npm test` + `cargo test` + `tsc` 全绿仍放行了一个让 send-keys 完全失效的改动——它们只断言字符串形状。
  2. 真机验收的输入必须从**真 builder** 取，手搓等价命令会重蹈覆辙。
  3. 探针载荷不能用真 `claude`（会启动并清屏 → 「兄弟未被污染」的 grep 给出**假 PASS**，本坑真实踩过），
     用 `CCMPROBE` 这类纯字母词。
  4. e2e 的 shell 探针也必须 `=名:`——探针前缀匹配会说谎（存活断言假阳、fixture 被写错会话）。
- `vitest` 的 `include` 只收 `src/**/*.vitest.ts`；黄金串在 `*.test.ts` 由 tsx 跑——**只跑 `npx vitest run` 会假绿**，必须 `npm test`。
- 命名偏离说明：规范 CLI 名取 `ccm` 而非用户举例的 `cc`（`cc` 是 Linux 的 C 编译器；`ccm` 本就由 cc-monitor 拥有并安装）。`cc` 作为用户别名由安装器生成，设计意图不变。
