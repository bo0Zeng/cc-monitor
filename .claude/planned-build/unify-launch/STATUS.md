# 状态 / STATUS — unify-launch（恢复工作的入口，每次先读这里）

- **当前阶段**：F09 已完成签收（Phase B→F 全过，commit 待落）
- **当前功能**：无——F09 收尾完成，下一步进 F10（剩余账号 UX）
- **已完成功能**：**F01**（tmux 目标精确匹配）、**F02**（统一启动 CLI `ccm` + 重构 bashrc，含 R11 追加修复 `ef1310b`）、**F03**（LaunchPlan IR + 双渲染器 + 维度注册表）、**F04**（会话身份统一，根治 R10）、**F05**（AccountResolver：判别联合 + `resolveAccount` + `ACCOUNT_DIMENSION.applies` 恒真接上 F03 移交点，顺带发现并修复 R11 同型潜在 bug）、**F06**（本地路径并入 IR：`history.rs` 两套 PowerShell builder 收拢成 `build_local_ps_command`，`planLocal` 让本地路径首次真正实例化 `transport:{kind:"local"}`）、**F07**（每账号默认模型：维度注册表**架构验收**通过——新增 `MODEL_DIMENSION` 零改 `buildLaunchPlan`/两个渲染器主体结构；`applies` 条件式 vs 恒真的判断依据记入 INVARIANTS §37；新增 R14）、**F11**（预信任能力上提进 `ccm`：`shared/ccm` 的 `--tmux` 建会话路径新增预信任 + `pretrusted` 追踪 + screen-scrape 轮询兜底 + `CCM_NO_PRETRUST` opt-out；范围收窄不碰仓库外的 `cc-spawn` 本体；双 agent 审各自独立复现真实阻塞项，含修复既有 `e2e/ccm-acceptance.sh` 污染真实全局配置的回归）、**F08**（终端集成收尾：`ccm --model` 闭合 R14；`canRenderCli` 针对性特判而非机械塞进 `CLI_REQUIRED_CAPS`；别名生成器+越层启动器诊断合并落点，紧邻彼此、不再按主机重复渲染；commit `06a9c76`）、**F09**（UI 收敛：动作×修饰——R12 降级为已归档设计决策；归档 tab 收敛成 `Resume`+账号×容器 3 级级联 flyout；存活 tab 收敛成 `Restart`+账号 flyout；徽章恒显身份（R7 语义反转）；对齐全套全仓删除；双 agent 审 1 阻塞+5 重要全部修复）
- **下一个功能**：F10（剩余账号 UX：面板砍卡片/加号一键化/用量）→ Phase G
- **阻塞 / 待用户确认**：无
- **最近一次计划回看时间**：2026-07-27（MASTERPLAN 变更记录 15）
- **自动模式（/loop）**：**全自动**（连续 B→G）。用户 2026-07-27 追加授权：**具体设计决策由本席开
  agent 讨论分析后自行决定，不必逐项停下来问**——除非真遇到阻塞或用户主动打断
- **本轮 loop 目标**：commit F09 → 开 F10（Phase A/B 规划）
- **loop 停止条件**：计划≠现实 / 同一步≥2 失败 / 全部完成→Phase G / 用户打断

## F09 结果摘要（R12 降级为已归档设计决策，R7 已落地关闭）

- Phase B：两个独立 Plan agent 论证 R12（container/agent 要不要收进 `LAUNCH_DIMENSIONS`）对立
  方向，综合后采纳"维持三轴三机制，只在 UI 层收敛"——`src/launch-menu.ts`（新，`enumerateModifierGroups`）
  是独立于 `LaunchDimension` 的 UI 发现层；判断准则记入 `doc/INVARIANTS.md` §38。
- Phase C：归档远端 tab 的 8-10 个扁平 resume 菜单项收敛成 1 个 `Resume` 一级项 + 账号×容器
  3 级级联 flyout（`resumeTabTmux` 补齐此前不支持显式选号的实现缺口，账号×容器真正正交）；
  存活远端 tab 收敛成 1 个 `Restart` 一级项 + 账号 flyout（无容器轴，restart 仍是编排、不进
  `LaunchAction`）；`updateAccountBadge` 从"仅不一致才显"改"账号已知即恒显"（R7 语义反转）；
  对齐全套（⇄/`alignAll`/`countAccountMismatches`/`account.align-active`）全仓删除（R8 落地前
  核实 e2e driver 零 import `tabs.ts`，安全）。
- Phase D 双 agent 审：后端架构 0 阻塞+2 重要（`openTimer` 未清理/`configDir` 过滤行为分歧，
  均已处置）；UX 审计 1 阻塞（`updateTabContextMenuItem` 替换 DOM 丢失 flyout 展开态，命中 R4
  "悬停+点击都可触发"契约）+4 重要（safe-triangle 悬停收起零延迟/三级级联无视口边缘碰撞检测/
  `restartingSids` 守卫对用户不可见/6 处过时注释），全部修复；另指出 §1"不做什么"对批量对齐
  删除的原始论证误用 MASTERPLAN 推论③，已改写为诚实的范围/复杂度取舍说明。
- 门禁：tsc 0 / npm test 648 / cargo test 379 / 全部既有 e2e 套件（含 `resume-suite.sh` 17/
  `restart-suite.sh` 24 真机回归）全绿；`remote-launch.test.ts`/两个 e2e driver/`src-tauri/`
  全程零 diff。

## F08 结果摘要（R14 已关闭）

- 5 个子项：①`invalidateCcmProbeCache` 接进安装成功分支（函数早就写好，此前从未被调用）；
  ②`ccm` 学会 `--model`（参数解析/tmux 内层命令/`--print`/非容器 env 导出四处 +
  `--ccm-probe` 能力位）；③`MODEL_DIMENSION.cliFlags` 从恒 `null` 改成真吐 flag，关闭 R14①，
  R14②随之自动关闭（终端 `ccm --model` 与 app 现在同一条命令）；④别名生成器 + 越层启动器
  诊断；⑤旧 swap 退役确认已关闭（F02 落地，无需代码）；⑥`--help` 内容补齐。
- **实现期修正**：`canRenderCli` 的能力探测没有照计划把 `"model"` 塞进
  `CLI_REQUIRED_CAPS`（那个列表语义是"每次调用都要求"，只对 `applies` 恒真的维度成立——
  `account` 能进去是因为 F05 后 `ACCOUNT_DIMENSION.applies` 恒真；`MODEL_DIMENSION.applies`
  是条件式，机械照抄会误伤未配模型偏好的多数会话），改用针对性特判
  `ctx.modelOverride && !probe.capabilities.has("model")`。
- 双 agent 审：后端架构审计因安全护栏两次误判被打断（讨论 shell 转义/注入防护的常规防御性
  代码审查被误判成攻击性安全测试）——本席直接接手核实剩余项（`CLI_REQUIRED_CAPS` 设计决策/
  两个渲染器的 flag 顺序/`shared/ccm` 四处 `--model` 触点/`buildAliasLine` 转义逻辑/重跑
  e2e 用例），UX 审计由自动化 agent 完整跑完，发现 1 阻塞（别名名字零校验可拼出语法错误的
  shell 代码，实测复现：`bash -n <<< 'my alias() { ccm "$@"; }'` 真报语法错误——已修）+
  多条重要（生成器与诊断分居两处且按主机重复渲染，已合并进同一设置分组、紧邻彼此；
  `--base`/`--account` 从静默优先级改成控件级主动互斥；复制 toast 补齐 source/新终端提示；
  诊断正则语义订正）。
- 门禁：tsc 0 / npm test 658 / cargo test 379 / 全部既有 e2e 套件（`test:ccm-cli` 39/
  `test:ccm-print-parity` 12/其余不变）全绿；真实全局配置文件复测确认干净。

## F11 结果摘要

- `shared/ccm` 的 `--tmux` 建会话路径新增预信任逻辑（照抄 `~/.claude/skills/cc-bus/scripts/
  cc-spawn` 的 `~/.claude.json`/`~/.codex/config.toml` 写入，逐行对齐不重新设计）。范围收窄为
  **只上提进 `ccm`，不改 `cc-spawn` 本体**——`cc-spawn` 物理上不在这个仓库，是跨项目共享的
  cc-bus 基础设施，改动需要用户另行明确授权，不该被"全部自动做"隐式覆盖（这条决策经两位
  审计 agent 独立评估认可）。
- 双 agent 审各自独立发现并**实测复现**真实阻塞项（本功能是迄今审计命中率最高的一次，因为
  它是唯一"写用户真实全局配置文件"的功能）：① `--cwd <相对路径>` 场景下 `$cwd` 非绝对路径
  导致预信任写出字面量野 key（如"proj1"），对真实信任表零效果、还污染用户真实配置文件、
  `jq` 校验照样通过不报错——已修（专用 `cwd_abs` 规范化）；② 完全丢失了 cc-spawn 真正的
  安全网（`pretrusted` 追踪 + 写入未成功时轮询抓 pane 文本自动按 Enter），而 cc-monitor 前端
  对本地 tmux 会话零可见性——没有这层兜底信任框会静默卡死无人发现，已补齐并用真实假 launcher
  端到端验证轮询确实生效；③ 本功能让**既有**（非新增）`e2e/ccm-acceptance.sh` 从"纯净沙盒"
  变成真实污染开发机 `~/.claude.json`/`~/.codex/config.toml` 的测试——已实测复现两次、清理
  本机污染、给该脚本补齐隔离。另修：计划承诺的 stderr 诊断补齐；受众从 cc-spawn 窄众变宽后
  加 `CCM_NO_PRETRUST` opt-out；e2e 测试从 6 场景扩到 9 场景。
- 门禁：tsc 0 / npm test 640 / cargo test 379 / 全部既有 e2e 套件（含新增
  `test:ccm-pretrust` 13/13）全绿；真实全局配置文件复测确认干净。

## F07 结果摘要（架构验收通过）

- `launch-dimensions.ts` 新增第 5 个维度 `MODEL_DIMENSION`（order 25，卡在 `account`(20) 与
  `nested-env-reset`(30) 之间）——落地后核对 `buildLaunchPlan`/`renderCli`/`canRenderCli`/
  `renderFallback` 既有分支结构 diff 为零，唯一改动是 `renderEnvOps` 的 switch 加一个成比例的
  `"export-model"` 分支，**兑现了 MASTERPLAN §0.1 成功标准②"加一个新维度零改渲染器主体"的
  承诺**。`applies:!!ctx.modelOverride` 是**条件式**而非像 `ACCOUNT_DIMENSION` 那样恒真——判断
  依据记入新增 `doc/INVARIANTS.md` §37（"这个维度不触发时的沉默，是否等价于用户期望"，不是
  "是不是账号相关"），防未来维度作者机械照抄 F05 的"恒真"教训。`cliFlags` 恒 `null`（`ccm` 无
  `--model`，诚实降级，留给 F08 关闭）。`accounts.ts` 新增 `getModelForAccount`/
  `setModelForAccount`（本机 `config.json` 的 `modelByAccount` 映射，`defaultName` 单值模式的
  复数版），`withAccount` 的 `run` 回调再扩一参 `(configDir?, accountName?, modelOverride?)`；
  4 个 `planXxx`/5 个 executor 各加末尾可选参数；`settings/accounts-section.ts` 加每账号"默认
  模型"输入框。
- 双 agent 审（后端架构 + UX）各揪出重要发现，全部修复，含 1 条**阻塞项**：模型输入框保存路径
  最初不校验，非法值（如带空格/shell 元字符）会静默落盘，之后该账号**每一次**会话拉起都会在
  `MODEL_DIMENSION.apply` 里统一 throw、用户难以联想到根因——已把 `isValidModelName` 校验移到
  `setModelForAccount` 写入点（fail-closed，同其余账号写入点校验的既有惯例）。另修：保存无 toast
  反馈+失败真无声消失（设置窗口没有主窗那个全局 unhandledrejection 兜底）——已按 `selectDefault`
  既有模式补齐；`cliFlags` 恒 `null` 的隐藏 CLI 降级 + 终端 `ccm` 不识别此偏好，处置力度未达 F05
  R13 先例——已登记 **R14**（已接受，非阻塞）+ 输入框 tooltip 补边界提示；`canRenderCli` 的模型
  降级此前零端到端测试覆盖——已补 2 条；`modelOverride` 在集成层（`tabs.ts` 等）的真值转传此前
  从未被验证——已补 1 条真实字符串集成测试（同 F05 曾堵过的同一类缺口）；`doc/INVARIANTS.md`
  计划承诺的新维度落地样例最初漏写——已补 §37。
- 门禁：tsc 0 / npm test 640 / cargo test 379 / 全部既有 e2e 套件不变（本功能不碰 Rust/远端
  tmux 路径）全绿；`remote-launch.test.ts`/两个 e2e driver 全程零 diff。

## F06 结果摘要

- `history.rs` 的 `build_resume_ps_command`/`build_new_session_ps_command`（曾逐字符重复的
  `Get-Command` 探测-回退分支）收拢成 `build_local_ps_command`（`LocalPsAction` 枚举驱动，同构
  TS `LaunchAction`），旧两个函数降级成薄委托，新增 2 条黄金串对拍测试锁死重构前后逐字节同输出。
  `src/launch-requests.ts` 新增 `planLocal`，让本地 resume/新建两条路径首次真正构造
  `LaunchContext`（`transport:{kind:"local"}`——F03 起就有的类型分支，此前从未被任何调用点
  实例化过）并跑一遍 `LAUNCH_DIMENSIONS`；4 个调用点（`history.ts`×2/`tabs.ts`/`session-viewer.ts`）
  接入。实现期自己发现并修一个一致性缺口：本地路径此前唯一缺失 `isValidSessionId` 校验（同其余
  4 个 `planXxx` 早有的模式），已补齐。`plan.env` 因 `NESTED_ENV_RESET_DIMENSION`（不看 transport）
  对本地场景恒非空，判定**故意不消费**——本地场景的嵌套 env 污染保护已经在
  `lib.rs::scrub_env_vars`（进程启动期一次性清洗）做完，已记 `doc/INVARIANTS.md` §36。
- 双 agent 审（后端架构 + UX）各揪出 2 条重要发现，全部修复：Rust/TS 两处 sid 字符集校验方向
  安全但不完全相同（措辞订正，未改代码）；`launch-render-cli.test.ts` 一处过期测试标题漏改；
  本地 sid 校验失败的错误 toast headline 与远端同类失败不一致（4 个调用点统一改成两阶段 catch，
  对齐远端"无法构造 resume 命令"措辞）；`planLocal`/`getBehavior()` 相对顺序在 4 个调用点不统一
  导致一处注释不准确（已订正）。Phase D 一份初次汇报误判"计划 checkbox 未勾"为阻塞（复核后
  判定只是 Phase F 文档收尾尚未进行）；另排除一条误报（`nestedEnvVars` 顺序差异，内容实际相同）。
- 门禁：tsc 0 / npm test 631 / cargo test 379 / 全部既有 e2e 套件不变（本功能不碰远端/tmux 路径）
  全绿；`remote-launch.test.ts`/两个 e2e driver 全程零 diff。

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
