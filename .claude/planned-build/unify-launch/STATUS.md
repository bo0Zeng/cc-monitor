# 状态 / STATUS — unify-launch（恢复工作的入口，每次先读这里）

- **当前阶段**：F02 已闭环（B→F 全过，即将 commit）；进 **F03** 的 Phase A/B（架构 fan-out）
- **当前功能**：F03 LaunchPlan IR + 双渲染器 + 维度注册表
- **已完成功能**：**F01**（tmux 目标精确匹配）、**F02**（统一启动 CLI `ccm` + 重构 bashrc）
- **下一个功能**：F03 之后按依赖图：F04/F05/F06/F07（可并列）→ F08/F09/F11 → F10 → Phase G
- **阻塞 / 待用户确认**：无
- **最近一次计划回看时间**：2026-07-27（MASTERPLAN 变更记录 06）
- **自动模式（/loop）**：**全自动**（连续 B→G）。用户 2026-07-27 追加授权：**具体设计决策由本席开
  agent 讨论分析后自行决定，不必逐项停下来问**——除非真遇到阻塞或用户主动打断
- **本轮 loop 目标**：F03 走完 B→F 并 commit
- **loop 停止条件**：计划≠现实 / 同一步≥2 失败 / 全部完成→Phase G / 用户打断

## F02 结果摘要

- 统一启动 CLI `ccm`（`~/.local/bin/ccm`，可执行文件）落地：`new`/`resume`/`attach` × `--tmux` /
  `--account|--base` / `--cwd auto|<dir>` / `--agent claude|codex` / `--launcher` / `--ccm-sid` /
  `--print` / `--ccm-probe`。用户 `~/.bashrc` 4 个 block（187 行）→ 1 个别名 block；已真机部署
  （备份 `~/.bashrc.ccm-backup-20260727-031051`）。
- 双 agent 审（后端架构 + UX）+ 真机测试各自揪出净退化，全部修复并复验：账号打错字会"生效到错账号
  上"（`die` 在子 shell 里只杀子 shell）、不传账号会掉进未登录基座、`resume` 被 `--cwd auto` 带偏到
  git 仓父目录、needle 守卫空转、六个带值 flag 缺取值校验、中文目录名塌缩导致误接错会话。
- 真机端到端验证：终端 `cct` 起真 claude，账号穿透 tmux 边界（对照组证明旧 `cct` 会丢账号）、身份两
  通道（建时打标 + poller 2 秒回填）、cc-monitor 六列齐全（能 attach/预览/换号重启）。
- 门禁：tsc 0 / npm test 598 + 13 tsx 套件 / cargo 370 / `test:tmux-target` 26 / `test:ccm-cli` 32 /
  `test:ccm-acceptance` 12，全绿。
- 遗留六条按功能分派（不是孤儿债务）：idle-tmux 复用→F04；agent 轴 codex resume 不一致→F06；
  `--ccm-probe` 无消费者→F03；`--tmux` inner 透传手工枚举→F03；`--help` 不够→F08；第三 agent 扩展性→F07。

## 本轮新增的用户观察 → 已登记

- **R10**（MASTERPLAN §6）：一个 sid 可能同时活在 ≥2 个 tmux 里，`findClaudeTmux` 的 `.find()`
  静默只挑第一个，另一个变成 app 完全看不见的僵尸会话。用户 2026-07-27 观察触发核实；核实当时机器上
  无重复（现存活会话按 sid 去重为空），是**结构性风险**非已发作 bug。用户拍板：**留给 F04 一起根治**
  （三道门 + `@ccm_sid_expect`/`@ccm_sid` 仲裁 + resume 前"已存活"检查须原子化 + 命中 >1 时不静默
  只取第一个），不单独打补丁。
- **F11**（Feature Inventory）：`cc-spawn`（cc-bus 的独立协作 agent 派生器）是第三套独立 tmux 启动
  实现，收编进 `ccm`；其"预信任写入"（`~/.claude.json`/`~/.codex/config.toml`）应上提进 `ccm` 核心
  ——直接解决 R10 调研中发现的"claude 卡信任确认页数小时、从不生成 sessionId、@ccm_sid 永不写入"。

## 双 agent 审门禁（用户 2026-07-27 指定，持续生效）

架构承载型功能（F03/F04/F05/F06/F07/F09/F11）必须过：
1. **后端架构 agent** —— 把握 MASTERPLAN §0 核心思想，审后端架构是否被破坏、扩展空间是否够。
2. **UX agent** —— 把握同一份核心思想，审交互是否真的收敛。
两者 prompt 必须自包含且带 MASTERPLAN §0 核心思想全文。**真机测试和门禁复核不能替代双 agent 审**——
本轮真机测试另外独立揪出了 3 条审计没报的 bug，两者互补、缺一不可。

## 备注

- 主计划 = `MASTERPLAN.md`（**先读 §0 核心思想**）；入口全量清单 = `INVENTORY.md`。
- 四视角审计原文在 `../account-onboarding/AUDIT-v2-FINDINGS.md`（反复引用其 C1-C7/D1-D9/E1-E9/P1-P3）。
- **教训清单（持续适用）**：
  1. 门禁只锁字符串形状不锁行为——每个碰 tmux/shell 命令构造的功能都要过真机验收表（`test:tmux-target`
     开了先例，F02 又加了 `test:ccm-cli`/`test:ccm-acceptance`）。
  2. 真机验收输入必须取自真 builder，不能手搓等价命令。
  3. 探针载荷不能用真 `claude`（会清屏，导致"未被污染"断言假 PASS）。
  4. e2e 的 shell 探针本身也要 `=名:`，否则探针前缀匹配会说谎。
  5. **本轮新增**：真机测试环境必须显式隔离 `$TMUX`/账号库/工作区变量——不隔离会让开发者本机状态
     污染测试断言（本轮至少踩过两次：`--print` 依赖实时 tmux 状态、账号变量泄漏进黄金串）。
  6. **本轮新增**：改 shell 脚本时，任何"需要值"的 flag 都要有统一的取值校验，不能只挑几个手动加——
     漏了的那个会被漏到生产（本轮真机漏到用户真实 tmux 上过一次）。
- `vitest` 的 `include` 只收 `src/**/*.vitest.ts`；黄金串在 `*.test.ts` 由 tsx 跑——只跑
  `npx vitest run` 会假绿，必须 `npm test`。
- 命名偏离说明：规范 CLI 名取 `ccm` 而非用户举例的 `cc`（`cc` 是 Linux 的 C 编译器；`ccm` 本就由
  cc-monitor 拥有并安装）。`cc` 作为用户别名由安装器生成，设计意图不变。
