# 方向修订 v2（2026-07-27，用户暂停 loop 后追加）

用户在 F1 完成、F2 开始前暂停 loop，给出更大范围的重定向。以下需求要并入主计划、重新排序（会话启动统一插到最前）。F5(9d3a7e6)/F1(7680a43) 已 commit。

## R1 — 会话启动机制统一（最高优先，超越原 F2-F7）
全程序所有启动/恢复会话收敛到一致机制：
- 所有 **tmux 起**同一机制；所有**直起**同一机制；所有**指定账号起**同一机制。
- **launcher 可配置**：默认 `claude`，用户自己填（cc/cct/…），只集成行为不硬塞 cc。
- **同步终端行为**：cc-monitor 与 ~/.bashrc 的 cc/ccm/cct/__ccm_rbind/_cc_acct_last/CC_ENV 共用同一语义（账号注入 / 代理 / @ccm_sid / tmux 命名 一处定义）。
- 修 restart/resume「缺 @ccm_sid」经常失败 + 右键换号重启/用某账号 resume 不起作用（根因=机制不统一 + @ccm_sid 盲区）。
- 「为什么还进 tmux ccm」也在此澄清/处理。
- （枚举 agent a6c54efe 正在出五维度对照 + 失败根因，作为本项地基。）

## R2 — tab 账号标识：多账号系统启用时**每个 tab 常显**（用户已确认）
- 当前是「信息才显」（仅会话账号≠当前账号时挂徽章）+ U8 休眠（<2 账号不点亮颜色）。
- 改成：**多账号系统启用（≥2 账号）时，每个 tab 都常显账号徽章**（不再只在不一致时显）。
- **单账号 / 未启用多账号系统 → 不显示**（用户原话：「单账号的时候不显示，只有启动我们的多账号系统才显示」）。即显隐门 = 多账号系统是否启用，而非一致与否。
- 影响：tabs.ts updateAccountBadge 的「信息才显」逻辑改成「多账号启用即常显」；休眠门（accountColorsActive/≥2）复用为总显隐门。

## R3 — 全局切换语义：只做普通（future-only）～~强制全局~~（用户砍掉）
- 全局切当前账号 = **只影响之后起的会话**（现状：新会话 + 未指定 resume 用它，在跑不动）——保持，仅此一档。
- ~~强制全局切换（连在跑的也切）~~ → **用户明确不做（「没什么用」）**。
- **连带简化**：原 F2 要保留到命令面板的**批量对齐 alignAll 整个可以删**（强制全局是它唯一的留存理由，既然不要，就全清）。换会话账号只保留**单会话**入口（R4 的 tab 右键「切到 X」）。→ 清 alignAll / alignAllToCurrentAccount / accountMismatchSids / countAccountMismatches / ⇄ / ⚠k / account-commands 里的对齐命令，全套移除。

## R4 — 右键菜单分级（flyout 子菜单），降复杂度（触发=悬停+点击都行，用户已确认）
- 现状：所有 per-account 项平铺在同一级右键菜单（用账号 z 重启 / 用账号 b 重启 / 用账号 z resume …）→ 太复杂。
- 改成**分级**：一级菜单给动作行（如「Resume ▸」「重启 ▸」），行右侧一个指向右的三角 ▸；**鼠标悬停到右侧 / 点击右侧**再展开二级菜单列各账号供选。
  - 例：「Resume ▸」→ hover/点 → [用账号 z] [用账号 b] …
  - 「重启 ▸」→ hover/点 → [切到账号 z] [切到账号 b] …
- 即右键菜单要有**层级**，账号选择收进二级 flyout，不再一级平铺。
- 影响：tabs.ts 的 appendTabContextMenuItem 体系需支持 submenu/flyout；这是新的 context-menu 分级能力。

## 待澄清（问用户）
- R3「强制全局切换」的触发方式（chip 下拉里两个选项？还是修饰键？）与确认（破坏性，要不要二次确认）。
- R4 flyout 触发：悬停展开 vs 点击展开（用户说「悬停后或者点击右侧」——两者都要？）。
- R2 徽章常显后，颜色休眠（<2 账号）还要不要（常显但单账号时颜色无区分意义）。

## R5 — 启动/resume 要可扩展加参数（用户已强调，别写死）
- 现在的启动维度：+tmux、+账号；以后可能还有别的参数。
- 设计成**可组合的 LaunchSpec（选项对象）+ 单一 builder**，新参数=加字段+一个修饰器，**不是**再拼一套 builder。禁止一次性写死。

## 枚举 agent 诊断要点（a6c54efe，全带文件行号，见下 UNIFIED-PLAN）
- **根因1（最致命）**：cc-monitor 自己「开新 Claude」(A1) 跑**裸 claude 且不设 @ccm_sid** → 自己起的会话之后 restart/resume 必失败。
- **根因5**：终端 cct 起的会话名 `&lt;dir&gt;_cc`，被 restart 的 kill/send-keys 的 `cc-*` 白名单拒 → 能 attach 不能重启。
- **大冲突（账号模型不兼容）**：你 bashrc 的 `_cc_acct` 是**凭据 swap 进单一 ~/.claude、互斥**的旧方案；cc-acct-iso 是 **CLAUDE_CONFIG_DIR 隔离、并发**。两套并存会打架（cc-acct-iso SKILL.md 明说要删旧 swap 方案）。@ccm_sid/代理/session 路径三处都因账号在哪套跑而分叉。
- 根因2/3/4：wrapper 读死 ~/.claude 无视 CLAUDE_CONFIG_DIR；A3 resume @ccm_sid 焊死建时 sid、/branch 漂移后失准；session 级选项多 pane 失准。

## R6 — 红线全解，目标=统一整个软件、架构做干净（用户 2026-07-27）
- 原架构红线（remote-launch 注入不动 / daemon 零改 / 不新增轮询）**全部解除**，为清架构自由重构。
- 仍守的是**用户长期偏好**（非本任务红线）：不用 emoji、git commit 无 Co-Authored-By。~/.bashrc **默认不静默改**（安全），但账号集成可主动管理 shell 集成（见 R7），需要改 rc 时先征得同意/走生成+安装而非偷改。

## R7 — 账号模型集成进 cc-monitor，做成「双集成」+ 向下兼容（用户 2026-07-27）
- 像现在集成 daemon/cc-acct-iso 那样，把**账号模型**也纳入 cc-monitor 管理。
- **两套集成**：① 用户**没开多账号** → 用**原本单账号机制**（尽量保持不变，裸 ~/.claude）；② 开了多账号 → CLAUDE_CONFIG_DIR 隔离。
- **关键**：多账号集成要能**无缝向下兼容单账号**——单账号 = 统一模型里 `account=null` 的退化态，**同一套启动/wrapper/@ccm_sid 路径**跑两种模式，而非两套并行代码。**全面思考架构**。

## 重规划影响
原顺序 F2→F3→F4→F6→F7 需重排：R1（启动统一）最前；R2/R3/R4 并入或调整 F1/F2/F3；原 F2「撤对齐主 UI」被 R3 改写（对齐变「强制全局切换」而非纯删）。待枚举 agent 回来 + 用户澄清后出 v2 主计划。
