# 状态 / STATUS — unify-launch（恢复工作的入口，每次先读这里）

- **当前阶段**：F05 已完成签收（Phase B→F 全过，commit 待落）
- **当前功能**：无——F05 收尾完成，下一步进 F06/F07
- **已完成功能**：**F01**（tmux 目标精确匹配）、**F02**（统一启动 CLI `ccm` + 重构 bashrc，含 R11 追加修复 `ef1310b`）、**F03**（LaunchPlan IR + 双渲染器 + 维度注册表）、**F04**（会话身份统一，根治 R10）、**F05**（AccountResolver：判别联合 + `resolveAccount` + `ACCOUNT_DIMENSION.applies` 恒真接上 F03 移交点，顺带发现并修复 R11 同型潜在 bug）
- **下一个功能**：F06/F07（正交、互不阻塞，都只依赖 F03）→ F08/F09/F11 → F10 → Phase G
- **阻塞 / 待用户确认**：无
- **最近一次计划回看时间**：2026-07-27（MASTERPLAN 变更记录 10）
- **自动模式（/loop）**：**全自动**（连续 B→G）。用户 2026-07-27 追加授权：**具体设计决策由本席开
  agent 讨论分析后自行决定，不必逐项停下来问**——除非真遇到阻塞或用户主动打断
- **本轮 loop 目标**：commit F05 → 开 F06 或 F07（Phase B 规划）
- **loop 停止条件**：计划≠现实 / 同一步≥2 失败 / 全部完成→Phase G / 用户打断

## F05 结果摘要

- `src/accounts.ts` 新增 `AccountResolution` 判别联合（`{kind:"account",name,configDir}` |
  `{kind:"base"}` | `{kind:"unavailable",requestedName?}`）+ 纯函数 `resolveAccount(state,opts)`；
  `withAccount` 内部改用它，`run` 回调扩成 `(configDir?, accountName?) => Promise<void>`（行为
  逐字节保持，6 个既有调用点全部核对）。`LaunchAccount`（`launch-plan.ts`）account 变体加可选
  `name` 字段；`ACCOUNT_DIMENSION.applies` 从"仅选中账号时为真"改**恒真**，`cliFlags` 三分支
  （有名字→`--account <name>`／base→`--base`／无名字→`null` 强制降级），把 F03 遗留的移交点
  接上。**顺带发现并修复一个 R11 同型潜在 bug**：`applies` 原先只在选中账号态为真，导致最常见
  的"未选账号（base）"场景从未过 `cliFlags` 的 null 安全网检查——CLI 渲染器可能吐出既不带
  `--account` 也不带 `--base` 的命令，让远端会话静默继承 `ccm` 自己的默认账号。
- 双 agent 审（后端架构 + UX）各揪出发现，全部修复：`doc/INVARIANTS.md` 计划里承诺的新不变量
  最初只留源码注释未落文档——已补新增 §35；6 个 `withAccount` 调用点此前测试只覆盖
  `accountName` 恒 `undefined` 场景，接线本身从未被验证——已补 4 条集成测试（含发现并修复
  `fetchAccounts` 模块级缓存的测试污染 bug，双向：向后泄漏+被更早测试的陈旧缓存挡住）；UX 审
  发现 `shared/ccm` 的 `--base` 是无条件 `unset CLAUDE_CONFIG_DIR`（非无害透传），F05 让每次
  未选账号的调用都携带它，对手动管理该环境变量的边缘配置用户是新的静默覆盖——判定为可接受
  代价，登记 **R13**（非阻塞，`forceLegacyLaunchRenderer` 逃生口可退避），不回退设计。
- 实现期自己踩了一次坑又自己修：`LaunchAccount.name` 最初误设计成必需字段，导致
  `remote-launch.test.ts`（F03 就定的"零编辑"硬约束）出现 3 个真回归——已改 `name` 为可选、
  `configDir` 单独触发 `account` 态（同 F03 原行为），`cliFlags` 对"有 configDir 无 name"这个
  合法但不可 CLI 化的状态诚实返回 `null`（强制降级），而非静默改变行为。
- 门禁：tsc 0 / npm test 625 / cargo test 377 / test:tmux-target 26 / test:ccm-cli 36 /
  test:ccm-acceptance 15 / test:ccm-print-parity 10 / test:tmux-guarded 14 / resume-suite 17 /
  restart-suite 24，全绿；`e2e/resume-cmd-driver.ts`/`restart-cmd-driver.ts`/`remote-launch.ts`
  全程零 diff。

## F04 结果摘要

- `tmux.rs` 三道门（Gate1 空 target 恒拒/Gate2 `@ccm_sid`∪`cc-*` union/Gate3 仅 kill 要求
  `windows==1`）+ `build_guarded_tmux_cmd` 原子 verify+act（`display-message` 单次 round-trip，
  新增真机验收 `e2e/tmux-guarded-acceptance.sh` 14 项证明 TOCTOU 真的消除，非仅字符串断言）；
  `shared/ccm` 的 `@ccm_sid_expect`（意图，通道A）/`@ccm_sid`（事实，poller 通道B 唯一写者）
  拆分，`sftp.rs` 结构性锚点防回归；`tabs.ts::findClaudeTmuxMatches`（不折叠成第一个）+ 三个
  真正需要分级的调用点全部升级（resume-attach 警告继续/restart-kill 拒绝/菜单 kill 项禁用）+
  `resumingSids` 互斥（对称既有 `restartingSids`）。
- 双 agent 审（后端架构 + UX）各揪出 2 条重要发现，全部修复：`CCM_GUARD_REJECTED` 拒绝消息曾
  恒带无关的 `windows=` 字段（send-keys 不受 Gate3 约束）；真机验收脚本缺 cargo-失败前置检查与
  结束时的 tmux server 清理；措辞漂移（"远端"vs"终端"统一）；3 处新增 toast 时长偏离本文件既有
  8000ms 惯例且方向拧了（已对齐）。
- 门禁：tsc 0 / npm test 615 / cargo test 377 / test:tmux-target 26 / test:ccm-cli 36 /
  test:ccm-acceptance 15 / test:ccm-print-parity 9 / test:tmux-guarded 14（新增）/ resume-suite 17 /
  restart-suite 24，全绿；`account-restart.ts`/两个 e2e driver 全程零 diff。

## F04 Phase B：两版 Plan agent 方案综合（存档）

方案 A（原子性优先）给出 `tmux.rs` 三道门的具体 `display-message` 原子命令构造（4 种渲染形态）+
发现 `session-backend.ts:113` 兜底渲染器的 `@ccm_sid` 直写不应跟着改名（无 poller 无提升机制）+
`resumingSids` 互斥新提案。方案 B（身份模型优先）核心洞见是 R10 本质是类型层面错误，逐一分析了
6 个 `findClaudeTmux` 调用点谁真需要富类型；给出 `@ccm_sid_expect`（意图）/`@ccm_sid`（事实）的
精确定义。综合结论见 `features/F04-session-identity.md` §2，四处取舍均已写明理由，其中「resume
命中多个警告继续 vs restart/kill 命中多个拒绝」这条严重度分级取舍已被双 agent 审确认合理。

## F03 结果摘要

- `LaunchPlan`/`LaunchContext` IR + 4 维度注册表（identity/env-reset/account/nested-env-reset，
  顺序不变量模块加载即断言）+ 双渲染器（`renderFallback` 逐字节等于旧行为、`renderCli` 翻译成
  `ccm …` 调用，`canRenderCli` 对不能诚实表达的维度/容器形态强制降级，不近似）+ ccm 探测缓存
  （TS+Rust，5 分钟 TTL）+ `--print` 平价预言机测试 + 6 个 executor 收敛（挑渲染器 + 剪贴板回退
  各自单一实现）。
- 双 agent 审（后端架构 + UX）各揪出 2 条重要发现，全部修复：`canRenderCli` 的 #76 闸门误伤
  `attach-only`（核对 `shared/ccm` 源码确认安全后收窄到只挡 `send-into`）；`settings/panel.ts`
  手搓 `BehaviorConfig` 字面量导致新字段被勾选框操作悄悄重置（tsc 揪出，已修）；container/agent
  轴不经维度注册表的不对称登记为 R12 转发 F09；toast 文案单测覆盖不对称补齐 8 条。
- 门禁：tsc 0 / npm test 606（含新增 launch-dimensions/launch-render-cli/8 条 toast smoke）/
  cargo test 374 / `test:tmux-target` 26 / `test:ccm-cli` 36 / `test:ccm-acceptance` 12 /
  `test:ccm-print-parity` 9（新增）/ `resume-suite` 17 / `restart-suite` 24，全绿；
  `account-restart.ts`/`tabs.ts`/`src/views/history.ts` 全程 `git status` 核对零 diff。

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

## F03 Phase B：两版 agent 方案综合

用户 2026-07-27 授权"具体决策由你开 agent 讨论分析后决定"。开两个独立 Plan agent（增量优先 /
IR 一次到位）并行出架构方案，综合成 `features/F03-launch-plan-ir.md`。三处分歧的取舍：
`TmuxTarget` 判别式对象（不是平级字段，最小 diff）；`EnvOp` 窄变体 `export-config-dir`（不是
通用 `export`，安全考量——防重蹈 D7 的 extraEnv key 无校验注入风险）；保留零成本的
`transport` 标记字段（为 F06 省一次账本变更）。

**综合过程中核对 `shared/ccm` 实际行为，发现并修复 R11**（不是任一 agent 报的，是我交叉核对
两版方案对"账号维度 CLI 化"的分歧时顺带查出来的）：`resumeCommandRemote=ccm` 时 cc-monitor
选中的非默认账号会被 ccm 自己的默认号回退静默覆盖。已修复+补测试+真机复验+commit `ef1310b`，
独立于 F03 本体。

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
  7. **本轮新增（R11）**：改一个函数的"默认值回退"逻辑时，必须显式想清楚"调用方已经替我做过选择、
     只是通过继承的环境变量表达"这种情形——不能默认"没显式传参 = 用户没有意见"，那可能只是
     "意见已经在环境变量里表达过了"。综合两版设计方案、核对实际行为时才挖出这条，两个独立 Plan
     agent 都没报——说明**设计评审对得上文档，不代表对得上运行时真实交互**，真机复核仍不可省。
- `vitest` 的 `include` 只收 `src/**/*.vitest.ts`；黄金串在 `*.test.ts` 由 tsx 跑——只跑
  `npx vitest run` 会假绿，必须 `npm test`。
- 命名偏离说明：规范 CLI 名取 `ccm` 而非用户举例的 `cc`（`cc` 是 Linux 的 C 编译器；`ccm` 本就由
  cc-monitor 拥有并安装）。`cc` 作为用户别名由安装器生成，设计意图不变。
