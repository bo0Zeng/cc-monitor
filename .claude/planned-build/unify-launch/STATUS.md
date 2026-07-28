# 状态 / STATUS — unify-launch（恢复工作的入口，每次先读这里）

> **2026-07-28 重大更新**：用户对 F02 Phase E/F 之后（即 F03-F10 全部）的产出**失去信心**
> （那一段由能力较弱的模型执行），要求推倒重看。已完成一次四视角独立复核（整体设计 / 文档 /
> 代码工程+伪测试专项 / 实现细节 / 波及面精确计数 / IR 内核独立重设计对拍）。
> **结论：IR 内核保留**（一个自由设计的独立方案撞回了 R11 与 fail-soft 两个坑，见下），
> **该重构的是它上面两层 + 验证信号本身**。本轮工作重排为 R 段 + B 段，见「§R/§B」。

## 当前状态

- **当前阶段**：R 段（重构 Sonnet 产出 + 信号修复）—— **当前重心**
- **当前功能**：**R 段 9/9 全部完成并 commit** → 下一个 **B 段**（B01 cc-bus 搬进仓，四份计划均已就绪且前提实测核实）
- **已完成功能**：**F01**（tmux 目标精确匹配）、**F02**（统一启动 CLI `ccm` + 重构 bashrc，含 R11 追加修复 `ef1310b`）、**F03**（LaunchPlan IR + 双渲染器 + 维度注册表）、**F04**（会话身份统一，根治 R10）、**F05**（AccountResolver：判别联合 + `resolveAccount` + `ACCOUNT_DIMENSION.applies` 恒真接上 F03 移交点，顺带发现并修复 R11 同型潜在 bug）、**F06**（本地路径并入 IR：`history.rs` 两套 PowerShell builder 收拢成 `build_local_ps_command`，`planLocal` 让本地路径首次真正实例化 `transport:{kind:"local"}`）、**F07**（每账号默认模型：维度注册表**架构验收**通过——新增 `MODEL_DIMENSION` 零改 `buildLaunchPlan`/两个渲染器主体结构；`applies` 条件式 vs 恒真的判断依据记入 INVARIANTS §37；新增 R14）、**F11**（预信任能力上提进 `ccm`：`shared/ccm` 的 `--tmux` 建会话路径新增预信任 + `pretrusted` 追踪 + screen-scrape 轮询兜底 + `CCM_NO_PRETRUST` opt-out；范围收窄不碰仓库外的 `cc-spawn` 本体；双 agent 审各自独立复现真实阻塞项，含修复既有 `e2e/ccm-acceptance.sh` 污染真实全局配置的回归）、**F08**（终端集成收尾：`ccm --model` 闭合 R14；`canRenderCli` 针对性特判而非机械塞进 `CLI_REQUIRED_CAPS`；别名生成器+越层启动器诊断合并落点，紧邻彼此、不再按主机重复渲染；commit `06a9c76`）、**F09**（UI 收敛：动作×修饰——R12 降级为已归档设计决策；归档 tab 收敛成 `Resume`+账号×容器 3 级级联 flyout；存活 tab 收敛成 `Restart`+账号 flyout；徽章恒显身份（R7 语义反转）；对齐全套全仓删除；双 agent 审 1 阻塞+5 重要全部修复）
- **下一个功能**：**B01**（cc-bus 逐字节原样搬进 `shared/cc-bus/`）→ B02（`cc-spawn` 收编，成功标准② 第二次架构验收）→ B03/B04 → P 段 → Phase G
- **阻塞 / 待用户确认**：无
- **最近一次计划回看时间**：2026-07-28（MASTERPLAN 变更记录 16）
- **自动模式（/loop）**：**全自动**（连续 B→G）。用户 2026-07-27 追加授权：**具体设计决策由本席开
  agent 讨论分析后自行决定，不必逐项停下来问**——除非真遇到阻塞或用户主动打断
- **本轮 loop 目标**：**R 段已满（9/9）** → 进 B 段：B01 → B02 → B03 → B04
- **loop 停止条件**：计划≠现实 / 同一步≥2 失败 / 全部完成→Phase G / 用户打断

## §0 工作约定（跨 compact 必须保留 —— 恢复工作时先读这一节）

**自动度**：全自动连续跑 planned-build 的 B→F 循环。具体设计决策自行判断，不逐项问。
**每个功能的 Phase D 必须开一个独立的对抗性 agent**——任务是论证「这个功能不该这么做 / 不该做」，
不是复核实现；prompt 自包含 + 带 §0 核心思想全文 + 明确要求「不要为了对抗而对抗，核实后若认同
就说认同并给证据」。**UX 视角与实现视角都要覆盖。**
**停止条件**：真阻塞 / 计划≠现实 / 同一步连续 2 次失败 / 全部完成 → Phase G。

**commit 约定**：本地 commit，**不加 `Co-Authored-By`**，一功能一 commit，
`git add` 用显式文件清单（绝不 `git add -A`）。
**推送授权（2026-07-28，唯一一次对外动作）**：R00 允许开一个 **draft PR → main** 以触发全套 CI。
**不 merge、不发版、不 bump、不改 workflow 触发条件。**

**门禁纪律**：所有门禁命令 `set -o pipefail`，输出**重定向到文件后 Read/grep 核实**，
绝不信内联回显（本仓踩过：裸管道掩盖 cargo 编译失败、误报全绿）。

**红线**：daemon 零改 · `TMUX_LS_FMT` 与 `remote-daemon-proto/src/watcher.rs` 逐字节一致 ·
不代替用户改 `~/.bashrc` / `~/.claude/settings.json` / user scope MCP（只读+诊断+生成待贴文本）·
cc-monitor 侧不新增轮询 · 不用 emoji · **不启动真实已认证的 `claude` 子进程**
（会烧真实额度且交互不可控；F10 的 `/usage` 解析因此全部基于训练知识猜测、未经真机验证，
真机验证清单见 `features/F10-remaining-account-ux.md` §7，留给用户上线前跑）。

### 已独立复核为真绿的基线（2026-07-28，不要重复怀疑/重跑）

`tsc` 0 · `npm test` **697** · `cargo test --all` **390** + daemon 125 + vendor `code-picture-core` 25 ·
覆盖率地板 / `npm audit --omit=dev --audit-level=high` / `vite build` /
`shellcheck --severity=error e2e/*.sh` / `py_compile` / daemon `cargo fmt` 全过
（**注意用 CI 的原样命令**：不带 `--audit-level=high` 会被一条 low 级 dompurify advisory 判红、
不带 `--severity=error` 会被 357 行 info/style 判红——两者 CI 都不门禁，我一度误报成红）·
**7 套真机 e2e 共 126 条断言全绿**（tmux-target 26 / tmux-guarded 14 / ccm-cli 39 /
ccm-print-parity 12 / ccm-acceptance 15 / ccm-pretrust 13 / usage-probe 7），
且跑完真实 `~/.claude.json`、`~/.codex/config.toml` 的 md5 未变（沙盒未污染）·
四条红线逐条核实干净 · 文档引用的 INVARIANTS 章节号全部真实存在、无悬空。

~~唯一的红：`cargo fmt --check` 28 处~~ → **R00 已修（实为 29 处 hunk / 7 文件），现全绿。**
**且已在 CI 上真跑过**：draft PR #83，6 个 job 全 success。

### F10 已 commit（R01 完成）

commit `bb71172`。收口时补的最后一条是 `launchPayload` 内容断言——本功能里唯一真正被送到
远端 shell 执行的字符串，此前只被 `objectContaining` 查了 origin/accountName、**内容从未被断言**。
已补逐字节断言 + 5 条 fail-closed 边界（引号/`;`/`$()`/相对路径/路径穿越），
并做两次变异验证：去掉账号隔离前缀 → 6 条转红；去掉嵌套 env 清理 → 1 条转红。
（第一条变异模拟的正是"探针探到错账号的用量、界面却完全正常"这类静默错误，R11 那一族的形状。）

## §R 段 — 重构 Sonnet 产出 + 信号修复（当前重心）

**为什么先修信号**（R00 已完成，此段保留为决策依据）：`account-ux` 曾领先 main 15 commit /
13204 插入行且**从未跑过任何 CI**；7 套真机 e2e 共 126 条断言**既不在 CI 也不在 `npm test`**；
`cargo fmt --check`（CI 唯一阻断性 Rust 门）红 29 处（`git archive` 验证 merge-base(v3.3.0)
时全干净 → 全部本分支引入）；已确认 3 条伪测试。**信号失效时做重构 = 蒙眼开刀。**
→ 现已全部修完并在 CI 上真跑过（PR #83，6 job 全绿）。**后续 R 段各项都是在可信信号下动刀。**

| ID | 内容 | 依据 | 状态 |
|----|------|------|------|
| R00 | 信号修复：`cargo fmt` 29 处 / 分支首次跑 CI（draft PR #83）/ 7 套 e2e 进 ubuntu CI job | 四视角审计共同结论 | **已完成**（见下「R00 结果」） |
| R01 | F10 收口 commit | 工作区已改完大半 | **已完成**（commit `bb71172`） |
| R02 | 伪测试扫荡：对每个功能挑 1-2 条「声称验证核心防线」的测试做**变异检查**（改坏实现确认转红） | 已确认 3 条（F10 内，全部已修） | **已完成**（11 条变异检查全 RED，见下「R02 结果」） |
| R03 | 位置参数长列车 → `LaunchModifiers` | 三方独立证伪账本「e2e 锁死签名」之说 | **已完成**（commit `4f6c313`）。**只达成「零改调用点」那一半**（32 行透传编辑归零），「零改 builder」未达成（4 个 planXxx 仍各 2 行）——原声称过度，已订正。扩围到 `withAccount` 回调（车头在那里）。审计 0 阻塞+4 重要全修 |
| R04 | 结构性收紧四处：`tryRenderCli` Result / `requiredCaps` 下放维度 / `EnvOp` unset 收窄 / `WrapSpec` 纯数据 | IR 内核独立重设计对拍 §2.2 | **已完成**（commit `fb81fe1`）。审计 **1 阻塞+5 重要**全修，推翻我三处声称（测试落在失败聚合点后=零守护；`prelude` 表达不出唯一用例、实测 rc=127；`unset` 收窄丢了编译期穷尽性）。顺带把 `tsconfig` 的 `include` 加上 `e2e` 关掉 tsc 盲区 |
| R05 | UI 层：删死代码 + 收敛真重复 + `"__base__"` 类型化 | 波及面计数 + 对抗审计 | **已完成**（`fb20c03`）。判别联合**顺带修了一个真 bug**：账号名可含下划线，真实账号叫 `__base__` 时改造前会被判成基座、静默丢号且 Restart 入口消失。审计 0 阻塞+5 重要全修（最要紧：我唯一实质重写的那行零覆盖、三个变异全存活，已补 6 条行为测试）。**否决账本「5 处账号菜单收敛」**，但核实后实为 7 处并登记 R16 |
| R06 | **成功标准① 首次可验收**：重写 `INVENTORY.md` | 已逐条核实失效 | **已完成**（改用符号名+可复跑 grep 锚点；11 条锚点逐条验证，首轮即抓到 1 条空锚点——`renderLaunchCommand` 实为模块私有非导出，正是「锚点自己会报错」的设计意图） |
| R07 | `planLocal` → `validateLocalLaunch`（纯校验，不再构造 IR） | IR 内核对拍 §2.2 第 6 条 | **已完成**（`2c365a4`）。审计 1 阻塞+5 重要全修，**推翻了本计划最核心的论证**：否决「真接上」引的是错误论据（Get-Command 只排除"TS 全量渲染"，真正理由是 F06 §3.2「无信息增量」）；「走一遍 buildLaunchPlan」零门禁守护且是 fail-closed 风险，已删 |
| R08 | **查证并修复：容器路径丢失继承账号**（实测复现，见下「新发现的真 bug」） | 本席 2026-07-28 `ccm --print` 实测 | **已完成**（commit `9dc0aad`，e2e 断言 126→131，CI 全绿） |
| R09 | 查证 `@ccm_sid` vs `@ccm_sid_expect` 语义分叉 | IR 内核对拍 §2.2 第 9 条 | **已完成——判定为「设计正确，只有一句文档不准确」，不改代码**（见下「R09 结论」） |

### commit 卫生瑕疵：`77d1486`（R05）单独不可编译（已如实记录，选择向前修而非改写历史）

**事实**：我在 commit R05 **之前**就把 `tabs.ts` 里的 `planLocal` 改成了 `validateLocalLaunch`
（那是 R07 的活），而 R05 的 `git add` 显式列了 `src/tabs.ts` → 那一行被卷进了 R05 的 commit。
后果：`git show 77d1486:src/tabs.ts` 引用 `validateLocalLaunch`，
而 `git show 77d1486:src/launch-requests.ts` 仍导出 `planLocal`——**该 commit 单独 checkout 不可编译**。

**为什么 CI 没抓到**：CI 跑的是 PR 的分支尖端，不是逐 commit。分支尖端一直是自洽的
（R07 的改动都在工作区，下一个 commit 就补上）。

**处置：向前修，不改写已推送的历史。**
- 影响面：`git bisect` 跨过这一个 commit 会构建失败（`bisect skip` 可绕过）。窗口只有一个 commit。
- 不 force-push 的理由：改写已发布历史是对外可见、较难撤销的动作，而这个 draft PR 本就只为跑 CI、
  不会 merge；为一个可 `skip` 的 bisect 断点去重写历史不划算。**若用户要求，可随时 rebase 修掉。**

**教训（已并入门禁纪律）**：一次只做一个功能时，**动手改文件之前先确认上一个功能已经 commit 完**。
本轮为了"等审计期间不闲着"提前做了 R07 的改名，就是在这里出的岔子——
"不闲着"不能以"把两个功能的改动混进同一个工作区"为代价。
更稳的做法：审计期间只做**新文件**（计划文档）与**只读核实**，不碰将被下一个功能改到的源文件。

### R 段收尾时登记的遗留条目（都**有意不在 R 段做**，各自需要自己的 DoD）

| ID | 内容 | 为什么现在不做 |
|----|------|---------------|
| **R15** | `LaunchContext.passThrough` 纯透传子集，可闭合成功标准②「零改 builder」那一半 | 今天纯透传只有 `modelOverride` **一个**元素（`configDir`/`accountName` 都要经 `accountOf` 解析），一个元素撑不起抽象——同 R12「提前建等于为假想需求设计」。等 B02 的 `--bus-id` 进来有第二个元素再抽。**硬约束**：绝不能把 `configDir`/`accountName` 也搬进 ctx，那会让未解析的原始字段与已解析的 `account` 判别联合并存，未来某维度读了原始字段就绕过解析——正是 R11 那族病 |
| **R16** | `views/history.ts::appendAccountResumeItems` **缺基座逃生口**（`grep "基座" src/views/` 零命中），而其默认 resume 走 `follow`——正是 #75 场景。tabs 有、设置页新会话对话框有，**只有 history 没有**；计划文里查无决策记录，疑为遗漏 | 加一个菜单项是**行为变化**，该走自己的 DoD 与 UX 判断，不该塞进一个"删死代码"的功能里 |
| （轻）| 「≥2 可选账号」阈值写在三处（`launch-menu.ts` / `tabs.ts` / `views/history.ts`）；同一"基座"概念三处三种文案 | 这才是账号菜单**真正该收敛**的东西——收的是**规则与文案**，不是渲染器 |

### R00 结果（2026-07-28，全部 6 个 CI job 绿）

commit：`c6bbe32` 计划重排 · `bb71172` F10 · `bcf2005` fmt · `3a5d286` CI e2e job ·
`754674f`/`ac874a9`/`5f3788e`/`29bdda4` 四轮修 e2e。draft PR **#83**（不 merge，仅为跑 CI）。

**开这个 PR 的价值被立刻兑现——分支首次 CI 揪出 2 个本地永远看不见的真缺陷**：

1. **`ccm-print-parity` 一直在验开发者装机版 ccm，不是仓内 `shared/ccm`。**
   `renderCli` 产出的命令行以裸 `ccm` 开头（本该如此），脚本 `bash -c` 执行时经 PATH 解析
   → 本机解析到 `~/.local/bin/ccm`。今天两份内容恰好相同，但那是巧合：改了仓内 ccm 忘了重装，
   这套「平价预言机」会继续绿。已改成 mktemp bin 指向 `$REPO/shared/ccm`，两次变异验证。
2. **`shared/ccm` 在 git 里没有可执行位**（100644，本机 775）。干净 checkout 拿到的 ccm
   不可执行，而容器路径 send-keys 的载荷按绝对路径直接执行它。已 `--chmod=+x`。
   **生产不受影响**（`sftp.rs` 用 `include_str!` + `upload_atomic(0o755)` 显式给权限），
   受影响的是干净 checkout 与任何 `./shared/ccm` 用法；MASTERPLAN §2.2 要求它是可执行文件，
   仓里记 644 与设计意图矛盾。

另修 2 处测试健壮性（非 CI 也受益）：`ccm-acceptance` 与 `ccm-pretrust` 原用固定 `sleep`
等异步副作用，改轮询等待（本地 25s→10s / 24s→5s，断言数不变）。`ccm-pretrust` 的等待条件
选得有依据：预信任块（`shared/ccm:386-432`）**先于** `new-session`（:454），故「socket 上
出现会话」充分证明预信任已走完——连幂等/负向场景也不必回退固定 sleep。

**CI 结构**：按「要不要 Rust 工具链」拆两个 ubuntu job——`e2e-tmux`（105 条，秒级）
+ `e2e-tmux-rust`（21 条，需 webkit2gtk 一套 Linux Tauri 构建依赖）。

**方法学教训（写给以后的自己）**：`send-keys` 进 tmux 的载荷失败时，错误信息**只存在于那个
pane 里**，测试进程只看到"结果是空的"。三轮 CI 里我第一轮修对、第二轮判错（误以为是慢
runner 上 sleep 不够，其实 20s 轮询照样超时）、第三轮改成「让被测对象自己说话」（超时即
`dump_panes`：pane 文本 + PATH + SHELL + default-shell）才一发命中真因。
**猜的代价是每轮一次 CI；加诊断的代价是一次。** `dump_panes` 已留在脚本里。

### R02 结果（2026-07-28，11 条变异检查全部 RED）

对 F01-F11 的核心防线逐条做变异检查（改坏生产代码 → 门禁必须转红）。**全部 RED = 全部真有
测试守护**，没有新增伪测试。此前确认的 3 条伪测试全在 F10 内、已随 `bb71172` 修完。

覆盖：F01 `exact_target` 的 `=name:` · F03 维度顺序不变量 · F03 #76 闸门（send-into 强制降级）
· F04 Gate2 身份 union · F05 `ACCOUNT_DIMENSION.applies` 恒真（R11 族）· F08 model 能力特判
· F09 徽章门控 · F02 带值 flag 取值校验 · F11 `cwd_abs` 规范化 · F10 探针会话过滤 · R11 继承闸。

其中 F05 那条被**两层独立守护**（单测「applies 恒真」+ e2e 经真 `shared/ccm` 断言 `--base`
真的传进内层调用），是 R11 教训落地得最扎实的一处。

**变异检查本身的三个失效模式（我在这一轮里全踩了一遍，故记下）**：
1. **变异不可达** —— `exit 7` 追加在文件末尾，而脚本先 `exec` 了 → 永不执行，套件照绿。
2. **变异语义无效** —— 把 `applies: () => true` 改成 `!!ctx.account`，而 `ctx.account` 是
   **非空**判别联合（无"未决定"第三态）→ 恒真，等于没改。差点误报 F05 是伪测试。
3. **门禁太窄** —— 用 `npx vitest run src/` 当门禁，漏掉 tsx 跑的黄金串（`*.test.ts`）。
   差点误报 #76 闸门是伪测试（实际由 `test:launch-render-cli` 精确抓住）。
4. **断言写在失败聚合点之后**（R04 Phase D 新发现）—— tsx 手写测试文件里
   `if (failed > 0) throw` 若不在文件末尾，之后追加的测试就落进**双向死区**：
   全绿时跑但不设门禁，有红时根本不执行。R04 的三条新测试就踩了这个，
   删掉被测防线的全部要害后 `npm test` 仍 exit=0 / 701 passed。
5. **变异未落在代码行上**（R04 审计自己踩到并记下）—— `replace(..., 1)` 可能命中注释里的
   同一串。纪律：**变异后先 `diff` 打出实际改动行再判色**。
→ 结论：**报"伪测试"之前，必须先证明变异真的生效、且门禁真的覆盖到那条测试。**
   一次性审计脚本不进仓（sed 锚点会随代码漂移）。

### R05 摸底：账本三条声明**逐条核实为真**（2026-07-28，R03 等审计期间查的）

1. **`enumerateModifierGroups` 的 container 组是死代码**（造出来、生产侧从不读）。
   全仓生产调用点**只有一个**：`tabs.ts:2325`，第二参硬编码 `"tmux"`；
   紧接着 `:2327` 只做 `groups.find(g => g.id === "account")`——**container 组从未被消费**。
   其余调用点全在 `launch-menu.vitest.ts`，且只有它（`:47`）跑过 `"none"` 分支——
   即那条分支**只被测试驱动过，生产从未走到**。
   连带效应：因第二参恒 `"tmux"`，container 组里的 `selected` 标志也恒定退化。
2. **`tabs.ts` 自己硬编码了逐字相同的一份**：`:2265-2266` 的
   `{ label: "tmux" }` / `{ label: "直连（不建 tmux）" }` 与 `launch-menu.ts:72-73` 的
   label **逐字相同**，两处各写一遍。
3. **`"__base__"` 是跨文件的裸魔法串**：`launch-menu.ts:61` 产出，
   `tabs.ts:2275` / `:2338` 各自 `=== "__base__"` 消费，无类型约束——
   拼错一个字符 tsc 抓不到，行为是"基座选项静默变成一个普通账号名"。

→ R05 三项都有据可依，可直接动手：删 container 组（连带那条只被测试驱动的 `"none"` 分支）、
把两处 label 收敛到单一来源、`"__base__"` 类型化。

### R09 结论（2026-07-28）：分离是对的，但「唯一写者」这句话是错的

**查证结果：`@ccm_sid` 全仓有两个写者，不是一个。**
1. `shared/ccm` 的 poller（通道B，读到会话文件确认后才写）——文档描述的那个。
2. `src/session-backend.ts::TMUX_BACKEND.createRunAttach`——**兜底渲染器**自己拼 tmux 命令时，
   在 create 分支**直写裸 `@ccm_sid`**。

**这不是漏改，是 F04 Phase B 方案A 的明确取舍**（账本 `shared/ccm-wrapper.sh` 那行就记着
「`session-backend.ts:113` 兜底渲染器的 `@ccm_sid` 直写不应跟着改名（无 poller 无提升机制）」）：
兜底路径没有 poller → 没有「意图→事实」的提升机制 → 若那里改写 `@ccm_sid_expect`，
这个 key **永远不会被提升**，Gate 2 的 `@ccm_sid` 半支永久判不出它，**该会话变得不可 kill**
（正是 §5.1 第 3 条要防的向后兼容回归）。所以两侧**故意写不同的 key**。

**两个方向相反的断言各自都已被钉住**（本轮实测确认，非纸面推断）：
- `sftp.rs` 的 needle 扫描：`shared/ccm` **必须**写 `@ccm_sid_expect`；
- `session-backend.test.ts`（「#72 + F03.4甲′」黄金串）：兜底渲染器**必须**写裸 `@ccm_sid`。
  变异验证：把兜底侧改成 `_expect` → 该测试转红（实测 1 failed）。

**对成功标准④ 无影响**（这是 R09 被登记时最担心的一点）：终端起会话那条路径全程在
`shared/ccm` 内（写 expect → poller 提升 → cc-monitor 认得），与兜底渲染器的例外无交集。

**唯一的真问题是一句不准确的文档**：多处写作「通道B 是 `@ccm_sid` **唯一**写者」——少了作用域限定。
已改为「在 `shared/ccm` 内部，通道B 是唯一写者」，并在 `sftp.rs` 的教训清单里补上这个例外的
完整理由 + 指向两条反向断言的交叉引用，免得后来者看到不一致就顺手改成一致、把会话改成不可 kill。

### 新发现的真 bug（R08，已实测复现 → **已修，commit `9dc0aad`**）

`ccm --print` 实测：外层已 `export CLAUDE_CONFIG_DIR=<账号b>` + 走容器路径（`--tmux`）时，
内层 send-keys 载荷是 `ccm resume SID --cwd … --agent claude --launcher claude`
——**没有 `--account`、没有 `--base`、没有 export**。内层 ccm 在 tmux fork 的新 shell 里
`CLAUDE_CONFIG_DIR` 为空、又无账号 flag → 按账号解析的 `elif` **落 manifest 默认号**。
即 R11 的症状（看起来生效了，只是换成了错的号）在容器路径上残留。
R11 那条注释说「两个场景用同一条 if 天然区分」，**只对非容器路径成立**——
容器路径上"尊重继承值"只做到了"在这个 shell 里不覆盖"，没做到"带过容器边界"。
命中条件：启动器自己建容器（`cct` = `ccm --tmux`）+ 账号只靠继承环境变量传递。
app 自己的 CLI 渲染器恒吐 `--account`/`--base`（`applies` 恒真）故安全；兜底渲染器自己拼
tmux 命令、export 在载荷内故安全。

### 四视角复核的关键结论（决定"重构什么、不重构什么"）

- **IR 内核保留。** 让一个 agent **先独立设计、后读实现**，它的方案会**重新引入 R11**
  （"轴有值就贡献、无值就跳过"的 `Option<V>` 语义 = 账号沉默 → 落默认号），且缺"次要动作
  允许失败"这一维（`(tmux set-option … || true) &&`，老 tmux 上 set 失败不能挡住 send-keys）。
  它列了 11 条"现有实现做对了而我没想到"。**这是不重写内核的最强证据。**
- **账本「e2e driver 传递性锁死 executor 签名」是假的**（三方独立证伪）：
  `restart-cmd-driver.ts` 只 import 已是对象参数的 `restartWithAccount`，shim 在 Tauri IPC
  边界拦截。→ R03 的代价远小于账本描述。
- **不建通用"取值目录"契约**：真正需要异步取值的只有 account 一条轴；container 是两个固定
  字面量、agent 只有一个元素。IR 设计者**独立地**也怀疑"一个注册表统治所有轴"在 agent 轴上
  站不住（`AGENT_PROFILE` 被 15 个文件消费、5 个与启动无关）。→ R05 只做删死代码 + 收敛重复。
- **不动 `shared/ccm` 本体**（R08 除外，那是修 bug）：12 条 print-parity + 39 条 ccm-cli 是
  外部预言机，动它风险最高、可观测性最低。

## §B 段 — cc-bus（仍是「起会话」主线，R 段之后）

用户 2026-07-28 定性：**cc-bus 不是"又一个要集成的工具"，它是"起会话被写死成 N 套实现"的
又一个病灶**——`cc-spawn`（136 行）内部自己 `tmux new-session` + 送环境 + 送任务。
这正是本工作区账本里**未达成的那一行**，当时因"文件在仓外、需用户另行授权"收窄；
用户现已授权 + 要求搬进仓。

| ID | 内容 | 状态 |
|----|------|------|
| B01 | cc-bus 搬进本仓：`~/.claude/skills/cc-bus/`（1118 行 bash / 12 命令 / SKILL.md / examples）**原样固化为仓内基线，不趁搬家重构**（盘上有 3 个 `scripts.bak-*` + 2 个脚本各一份 `.bak`，说明一直手改）；部署走「备份→写→读回比对→回滚」 | 待做 |
| B02 | **`cc-spawn` 收编**：建会话/送环境/送任务改经 `ccm`，只保留 cc-bus 专属部分（`cc-register` 总线登记 / `spawned.tsv` 台账 / 复用判定）。**闭合账本未达成行** | 待做 |
| B03 | 驾驶舱：cc-monitor 看见/管理 bus 上的 agent、派活、读 inbox、`cc-spawn` 图形化 | 待做 |
| B04 | settings.json 钩子的「读+诊断+生成待贴文本」（**不写**——用户定调；cc-bus 自己的安装脚本第 3 行同样拒绝改它） | 待做 |

**B03 的两条已知张力**（Phase B 必须先解决）：
① 与「不新增轮询」红线冲突——`~/.cc-bus/{agents.tsv,inbox/,spawned.tsv}` 是 aya 本机文件，
cc-monitor 在 Windows 侧只能经 SSH 看。默认取**按需刷新**（同 F10 用量探针的懒加载），
除非能论证复用 daemon 既有 inotify watcher 且不破 daemon 零改红线。
② `cc-spawn` 图形化**必须建立在 B02 之上**，否则等于在 cc-monitor 侧再造第四套起会话实现——
亲手制造本工作区刚消灭的病。


### B 段摸底结论（2026-07-28，R 段进行中顺带查的，非猜测）

盘面事实（`~/.claude/skills/cc-bus/`）：13 个脚本 / **1118 行** / `cc-spawn` 136 行；
备份是 **2 个 `scripts.bak-*` 目录 + 2 个 `.bak` 文件**（`cc-whoami.bak-*` / `cc-spawn.bak-*`）
——账本原记"3 个目录"，实为 2 个，已订正；结论不变（确实一直手改、无版本管理，故 B01 原样固化）。

**B02 的三段靶心已定位到行**（`cc-spawn`）：
- `:100` 建会话 `tmux new-session -d -s "$name" -c "$absdir" **-x 220 -y 50**`
- `:108-109` 送环境 `inj=""`；**仅 codex** 时 `inj="CC_BUS_ID=<name> "`
- `:111/:113` 送任务 `send-keys "${inj}$LAUNCH $(printf '%q' "$task")"`
- `:56-86` 预信任 + `:128` 轮询按 Enter —— **F11 已上提进 ccm**，B02 不必重做，删即可

**三条实测发现，直接改写 B02 的工作量估计：**

1. **`ccm` 已经支持 `--` 透传，B02 不需要新增"送任务"能力。**
   实测 `ccm --tmux --cwd /p --print -- "分析这个项目的架构"` 的内层载荷确实带
   `'--' '分析这个项目的架构'`，且 `e2e/ccm-cli.test.sh:58` 早有覆盖
   （"-- 之后透传给 agent，含特殊字符正确 quote"）。
   → **订正我此前的决策**：我曾把"给 ccm 加透传能力"列为 B02 的一项，那是错的，F02 就做了。
   B02 因此比原估更小。

2. **`CC_BUS_ID` 会在 tmux 边界被吃掉——同一个病，换了个变量。**
   `cc-spawn` 今天靠 `send-keys` 载荷里的 `inj=` 前缀把 `CC_BUS_ID` 送进容器内侧，所以现在是对的。
   但若 B02 天真地把建会话换成 `ccm --tmux`，`CC_BUS_ID` 就只能靠环境继承——
   而 `update-environment` 默认列表同样不含它，**会被整个吃掉**（R08 刚踩过的完全同型问题）。
   → **B02 必须把 `CC_BUS_ID` 显式化**。这恰好是**成功标准② 的第二次真实架构验收**：
   给 `ccm` 加一个新维度（如 `--bus-id`），验证"注册一个 dimension + CLI 加一个 flag，
   零改 9 个函数签名与 7 个调用点"。R03 刚做完的 `LaunchModifiers` 正好在这里兑现。
   **暴露面窄**：`inj` 只在 `tool=codex` 时非空；claude 路径下 cc-bus 身份来自 tmux 会话名
   （`cc-whoami` 优先级：`$CC_BUS_ID` → pane 标签 `@cc_id` → 会话名），无需携带。

3. **`ccm` 不设窗口尺寸，`cc-spawn` 设了 `-x 220 -y 50`。**
   detached tmux 会话默认 80x24，cc-spawn 刻意放宽免得 agent 输出被窄折。
   → B02 要么给 ccm 加上（属容器轴细节，不是新维度），要么显式接受行为变化。
   **不能默默丢掉**——这类"看起来无害的细节"正是 F02 真机测试揪出净退化的那一类。

## §P 段 — code-picture（B 段之后，见 `../integrate-toolchain/`）

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
