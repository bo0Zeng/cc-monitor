# 功能计划 — T02 配置面审计视图

> **一句话**：让用户在一个页面上看清「cc-monitor 到底动过我哪些文件、动成什么样、还能不能撤」。
> 对内它还有一个硬任务：**给 `ToolSpec` 找到真实的生产消费者**，否则按 T01 的登记条件该删掉注册表。

## 0. 动手前先证成

T01 结束时留了一条自我约束（审计 I2）：`tool_registry.rs` **零生产消费者**，
若 T02 收工时仍无消费者**就删掉它**。所以 T02 的第一判据不是"页面好不好看"，而是
**`TOOLS` 的每个字段是否真被这个视图用上**：

| 字段 | T02 怎么用 |
|---|---|
| `id` | 行分组键 / DOM dataset |
| `display_name` | 分组标题 |
| `source` | 「这东西从哪来」列（仓内文件 / vendored+指纹 / 内嵌二进制 / 现场生成） |
| `destination` | 决定这条路径**在本机还是在远端**（本机才查得到现状） |
| `installable` | 没实现部署的工具，行尾标「尚未支持部署」而不是给一个点了没反应的按钮 |
| `uninstallable` | 「能否撤」列 |
| `touches` | 表格主体 |

七个字段全部有真实用途 → 注册表留下。**做完必须核对 clippy 的 6 条 `never used` 是否消失**
（T01 §12 把那 6 条警告定为这笔债的存根）。

## 1. 计划 ≠ 现实：第一处必须先改的地方

一上手就撞上一条（iron law 4，记录而不是默默绕开）：**`TouchedFile.path` 目前不是机器可解析的路径**，
里面混着给人看的散文：

| 现状 | 问题 |
|---|---|
| `~/.bashrc（或所选 profile）` | 括号注释进不了 `Path` |
| `~/.claude/settings.json 的 hooks 段` | 同上，且"的 hooks 段"是语义不是路径 |
| `~/.local/bin/cc-*（12 条软链）` | 既是 glob 又带注释 |
| `~/.cc-bus/（运行期状态）` | 同上 |
| `远端 ~/.local/bin/ccm-daemon` | "远端"这个前缀是**位置信息**，不该编在路径里 |

**改法**：`TouchedFile` 拆成 `path`（机器可解析，可含 `~` 与 glob）+ `note: Option<&'static str>`（散文）。
「本机还是远端」**不新增字段**——从 `ToolSpec.destination` 推导（`RemoteHomeRelative` → 远端），
并写一条测试把这个推导钉住；哪天出现"本机工具却碰远端文件"的组合，那条测试会红，届时再加字段。
（先不加：现在 6 个工具没有一个是那样，加了就是为假想需求设计。）

`note` 的 ≥2 判据：ccm 1 处 + cc-bus 2 处 → 够格。**如实写明**：T01 的字段纪律扫描
只枚举 `ToolSpec` 的字段，**不覆盖 `TouchedFile`**；这一条靠人判断，不谎称有门禁。

## 2. DoD

- [ ] `ToolSpec` 七个字段全部有生产消费者；`cargo clippy` 对 `tool_registry` 的 6 条 `never used` **消失**
- [ ] 新增 `config_surface.rs`：**纯函数** `resolve_touched_path()` + 一条 Tauri 命令
- [ ] 路径解析**不猜**：解析不了的（远端 / 项目相对 / `$PROFILE` / glob）一律带**明确理由**回报，
      绝不显示成"缺失"——「对能用的安装报假警报」是 B04 审计已抓过一次的病
- [ ] UI 一张表：工具 / 文件 / 我们做什么 / 现状 / 能否撤销；`GenerateOnly` 行明确写「我们不写，只给你待贴文本」
- [ ] 收 B04 登记项：诊断**同时看** `<cfg>/settings.json` 与 `<cfg>/settings.local.json`；
      项目级 `.claude/settings.json` **明说没查**（需先指定项目目录），不假装查过
- [ ] 只读。**一次性按需读，不新增轮询**（红线）
- [ ] 不写 `~/.claude/settings.json` / `~/.bashrc`（红线）

**不做**：撤销动作的**执行**（沿用各工具既有的卸载入口，不新写写入路径）；T07 的面板 IA 重构；远端逐条 stat。

## 3. 实现步骤

1. `TouchedFile` 拆 `path` + `note`；6 个工具的声明改成机器可解析路径；加"远端性从 destination 推导"的测试。
2. `config_surface.rs`：`PathResolution` 枚举（`Local(PathBuf)` / `Remote` / `NeedsProjectDir` / `WindowsProfile` / `Glob`）
   + 纯函数 `resolve_touched_path(declared, dest, home, cfg_dir, is_dir)`。`~/.claude/...` 要走
   `hooks_diag::settings_path` 同一套 `CLAUDE_CONFIG_DIR` 规则——**不能两处各解释一次**。
3. `SurfaceState`：`Present{bytes}` / `Absent` / `Undetermined{why}`。glob 走 `Present{n 项}` 或 `Absent`。
4. Tauri 命令 `config_surface_rows()`：`spawn_blocking` 一次性扫完返回。
5. UI `src/settings/config-surface-section.ts`：渲染 + 复制诊断文本（沿用既有形态）。
6. B04 登记项：`settings.local.json` 一并读；两个文件各自一行，都进这张表。

## 4. 测试策略

- 纯函数逐条：五种解析结果 + `CLAUDE_CONFIG_DIR` 生效/回落 + glob 计数
- **结构性守卫**：`TOOLS` 里每个 `touches[].path` 必须能被 `resolve_touched_path` 归到五种之一，
  **不得落进"解析失败"**（用 `structural_scan::ScanReport::require` 拿计数自检）
- **反向自检**：把某个 path 改成散文（如加回 `（12 条软链）`）必须红
- UI：`invoke` 返回 `undefined` 时不许崩（B03 踩过的真 bug，两处已修，第三处别再犯）

## 5. 风险

- `~` 展开与 `CLAUDE_CONFIG_DIR` 两套规则打架 → 强制复用 `hooks_diag::settings_path`
- glob 扫目录可能很慢（`~/.local/bin/cc-*`）→ 只 `read_dir` 一层 + 上限计数
- 「现状」列会让人以为可以点着修 → 措辞明确：本页**只读**

## 6. 代码审计结果（Phase D）
（待填）

## 7. 工程审计结果（Phase E）
（待填）

## 8. 签收
- [ ] 通过代码审计（无阻塞项）
- [ ] 通过工程审计
- [ ] 主计划已据此更新（含变更记录）

---

## 6. 代码审计结果（Phase D，2026-07-29）

独立对抗性 agent，34 次工具调用 / 15 分钟。它**实际做了变异并全部还原**（收工时 `git status` 干净）。
两条阻塞我逐条独立复现后才动手，另外**自己查出三条它没报的、更严重的**。

### 阻塞 1（已修）：`~/.cc-bus/` 声明「我们不写」是假话

原声明 `TouchEffect::ReadOnly` → 审计页渲染「只读（诊断用），我们不写」。
但 cc-monitor 自己的 cc-bus 驾驶舱有两个按钮：`cc_bus::cc_bus_send`（跑 `cc-send`）与
`cc_bus::cc_bus_spawn`（跑 `cc-spawn`）。我复现的证据链：
`cc-bus-lib.sh:221` = `printf '%s\n' "$line" >> "$inbox"`、`cc-spawn:141` 追加 `spawned.tsv`、
`cc-register:25` 用 `mv` 换掉 `agents.tsv`。

**「我们只是调了别人的命令」不改变"用户的文件因为在我们这儿点了一下而变了"这件事。**
这一页的全部价值是可信告知，在自己的主张上失信比不做这一页更坏。
→ 新增 `TouchEffect::IndirectWrite`，措辞改成「我们不直接写它；但你在 cc-monitor 里的操作会让它被写」。

### 阻塞 2（已修）：`~/.local/bin/cc-*` 声明「由 cc-monitor 拥有」是假话

原声明 `OwnedFile` + `installable: false`，于是同一行同时显示「12 项匹配」+
「整个文件由 cc-monitor 拥有，部署时整体覆盖」+「尚未支持部署，也就无所谓撤销」。
真机核实：那些软链是**用户自己的安装脚本**在 7/17 与 7/26 建的，
cc-monitor 侧**一行创建代码都没有**（`grep -rn "local/bin/cc-"` 只命中注册表自己 + 钩子诊断的两条只读检查）。

**而且我顺手发现审计漏了的第三条**：那个 glob 实际匹配 **11 条 cc-bus + 1 条 cc-acct-iso**
——`cc-acct-iso` 是注册表里**另一个工具**。所以审计看到的"12 项匹配"本身就是跨工具误记，
而 note 写的"12 条软链"也是错的（cc-bus 只软链 11 条）。
→ 改 `ReadOnly` + note 如实写明「不是 cc-monitor 建的 / 只查其中两条 / 本页 glob 计数偏大 1」。
→ 加 `owned_file_implies_installable`（照 `fenced_block_implies_uninstallable` 的形状）。

### 我自己追出来的三条（比审计报的更严重）：六条声明里**三条没有任何代码支撑**

审计的重要 6 是「注册表与真写入方零耦合」。顺着查下去：

| 工具 | 原声明 | 真实情况 |
|---|---|---|
| `remote-daemon` | `RemoteHomeRelative(".local/bin/ccm-daemon")` | 这个字符串**全仓只出现在注册表自己里**；真实路径是 `RemoteConfig.daemon_path`，每个远端各自配置（`remote_history.rs:46` 直接 `shell_quote(&cfg.daemon_path)`） |
| `cc-acct-iso` | `LocalHomeRelative(".claude/skills/cc-acct-iso")` | `acct_iso_deploy::deploy_remote_acct_iso(cfg, dest_dir)` 是**远端**部署，落点还是**前端传进来的** `dest_dir` |
| `cc-bus` | `LocalHomeRelative(".claude/skills/cc-bus")` | 部署未实现，这是**愿景**不是事实 |

前两条是我凭印象写的常量。**声明一个不存在的常量比不声明更坏**——审计页会拿它去查一个
没人写的路径，然后言之凿凿地报"缺失"。
→ 新增 `ToolDestination::UserConfiguredPath { token, what }`（2 个使用者，够 ≥2），
申报路径改成占位符 `$DAEMON_PATH` / `$ACCT_ISO_DEST`（与 `$PROFILE` 同一套写法，仍满足 ASCII-graphic 判据），
解析出 `PathResolution::NeedsUserConfig { what }` → 「路径由配置决定（去哪儿看）——本页不猜它当前是什么值」。

**真有常量的两条用 `pin_definition` 钉死**（`declared_destinations_are_pinned_to_the_real_writers`）。
双向变异验证：改注册表 → 红「注册表声明的 ccm 落点与 sftp.rs 的 CCM_CLI_REMOTE_PATH 不一致」；
改 `sftp.rs` 真常量 → 红「定义必须逐字是 …」。**两个方向都红，耦合是真的。**

### 重要 1（已修）：`locality_is_derivable_from_destination_today` 是同义反复 → 删

`PathResolution::Remote` 只由 `RemoteHomeRelative` 臂产生、且必然产生，所以断言恒真。
审计实测：把 `ccm` 的 destination 翻成 `LocalHomeRelative`（会让两行从"远端未确定"变成
去 stat 本机 `~/.bashrc`）→ **492 项照样全绿**。它自称守的那件事在类型上根本表达不出来。
→ **删掉，不留永远不会红的钉子**，并在文档里如实登记「远端性没有门禁」。
换上两条有牙的跨字段一致性：`installable_tools_declare_where_they_land` + `owned_file_implies_installable`。

真要门禁得给 `TouchedFile` 加 `host`，而它已经有真实的第二消费者在等：`~/.cc-bus/` 被本页
解析成**本机**，而 `cc_bus.rs` 是按 `origin` 在**可能是远端**的主机上读它——
一个 `const destination` 表达不了"按运行期 origin 跨主机"。**留给 T04 连 origin 模型一起做。**

### 重要 2（已修）：只读守卫可绕，三种手法实测

审计实测第一种：注入 `use std::fs;` + `fs::write(...)` → **17/17 全绿**（守卫只找字面
`std::fs::` 前缀，`write` 也不在那 4 个禁用词里）。我按同族又推了两种。
→ 改成扫**任意前缀**的 `fs::` + 钉死 `use` 列表（要件 4）。修后三种全红：

| 手法 | 修前 | 修后 |
|---|---|---|
| `use std::fs;` + `fs::write` | 17/17 绿 | 红 `发现 fs::write` |
| `tokio::fs::write` | （同族，未测） | 红 `发现 fs::write` |
| `std::os::unix::fs::symlink` | （同族，未测） | 红 `发现 fs::symlink` |

### 重要 3（已修）：审计手法**下移一层**仍然有效

给 `TouchedFile` 加 `pub needs_sudo: bool`（10 个字面量里 1 真 9 假）→ **492 全绿、零 warning**。
`pub` 字段在 lib crate 里连 `dead_code` 都不报，所以连 T01 依赖的"clippy 存根"都没有。
→ `declared_fields_of(code, struct_name)` / `literals_of(code, type_name)` /
`field_discipline_of(...)` 全部参数化，`TouchedFile` 与 `ToolSpec` 走同一条纪律。

**顺带更正我自己文档里说反的一句**：原文写「`note` 的 ≥2 判据是人工数的，不谎称有门禁」
——低估了。`note` 当时**已经有**一条机器门禁（`rows_cover_…` 里 `with_note.len() >= 2`）；
真正一条门禁都没有的是 `path` / `effect` 和**将来新增的字段**，而审计正是从那个口子进来的。

### 重要 4（已修）：`invoke undefined 不许炸` 是安慰剂

删掉那段 `Array.isArray` 形状校验后，`render(undefined)` 抛的 TypeError 被同一个 try/catch
吞掉，产生**一模一样**的"扫描失败"+toast → 那条测试照样绿。它守的是 catch 存在，不是形状校验存在。
→ 改成断言那句专属文案「形状不对」。反验证：删校验后**由 1 条红变 2 条红**。

（过程记录：我第一次复现这条时 python 锚点没对上、变异没写进文件，输出的"15 passed"是
**未变异**的结果。当场识别并重做——**先 diff 确认改动行再判色**这条纪律又用上了一次。）

### 重要 5（已修）：这一页两个核心列在 DOM 层完全无门禁

把 `config-surface-effect` 与 `config-surface-undo` 两段渲染整体删掉 → **15/15 全绿**。
`effect_label` / `describeUndo` 只作为纯函数被断言，没人管它们有没有上屏——
「我们做什么」和「能否撤」可以静默消失。
→ 加 `「我们做什么」和「能否撤」必须真上屏`，断言 DOM 里有这两个 class **且内容是真措辞**。
反验证：删两段渲染 → 红。

### 重要 7（已修）：「不适用」和「查不到」不是一回事

`$PROFILE` 原先一律说「Windows 侧 $PROFILE，本机无从解析」——在 Linux 上这暗示
"可能有东西、只是查不到"，实际是**这一项根本不适用**。而在 Windows 上仓里已经有能力查它
（`profile_installer::scan_path` 给出 path/exists/has_ccm_block/size），那边该指路而不是耸肩。
→ 按 `cfg!(target_os = "windows")` 分两种措辞。

### 重要 8（已修，改措辞而非改判据）：`contains` 与钩子诊断页会给出不同话

真机核实**当前不矛盾**（`~/.claude/settings.json` 里 2 处命中都在 `hooks.*.command` 里）。
但假阳性面是真实的：权限白名单里一条 `Bash(cc-register)`、被改了事件名的钩子、
甚至一句 `"description": "装 cc-register 用"` 都会命中。
→ 判据**不改**（本页要的就是"文件里有没有这个字样"这个粗信号），但措辞必须先讲明：
常量文档 + `precedence_note` + 前端文案都写成「字样粗匹配，**不等于装上了**；准确判定看『cc-bus 钩子』页」。
两页对同一文件说不同的话是**设计如此**，不是 bug——但必须让用户知道。

### 审计的 CSS 观察（已修）

`config-surface-*` 与 `tone-*` 在 `styles.css` 里**一处定义都没有**，于是「不存在」和
「未确定 —— …」在屏幕上没有区别——16 条测试守的 `tone-unknown` ≠ `tone-bad` 只落在 class 名上。
→ 补了一小段 CSS：ok 绿 / bad 红 / **unknown 用 warn 黄而不是 error 红**（混色就等于把假警报画成警报）。
**登记不修**：`cc-bus-*` / `hooks-*` 同样缺 CSS，那是 B03/B04 的既有欠账，不在本轮扩面。

### 我核实后认同审计的意见

- **`require(10)` 是紧的**：改 `require(11)` 就报"只扫到 10 处"，真实 `checked` = 10 = Σ`touches`。
- **ASCII-graphic 白名单不是过度反应**（审计明确不同意它自己被指派的攻击点 2）：
  判据只作用于**仓内 const 申报路径**，用户机器上的 `C:\Users\张三` 是之后 `join` 出来的、不过这道闸。
  它实测放宽到允许空格也是 492 全绿，即"需要时零代价放开"。**这条我采纳它的反驳，不改。**
- 拆 `path`/`note`、`SurfaceState` 三态 + `why` 必填、目录列不出判 `Undetermined`、
  `claude_config_dir` 提炼到一处、项目级「明说没查」——审计逐条对照变异确认有牙。
- 它认为**不该删注册表也不该降级为 UI 常量**（我给它指派的攻击点 1 它核实后否决了）：
  真问题不是"消费者只有一个"，是"注册表与真写入方零耦合"。**我同意，并按它说的把耦合建起来了。**

### 审计自己声明未验证的

只跑了 `cargo test --lib`（492）与 config-surface 那一个 vitest 文件（15）。
**clippy / fmt / tsc / 全量 npm test / shellcheck / 七套真机套件它都没跑**，
"clippy 6 条 never used 已消失"它没核——那几项是我自己跑的（见下）。
它也没启动 Tauri 应用、没看真实渲染；重要 8 的矛盾场景只核实了"当前不矛盾"。这些声明我认为诚实。

## 7. 工程审计结果（Phase E）

- **主计划仍自洽**：T02 的产出正是 T07（面板 IA 重构）要的那张"全局配置面审计页"，无需返工。
- **给 T04 留下的硬约束**（账本新增一行）：`TouchedFile` 的 `host` 维度 + `origin` 模型要一起做。
  现在 `~/.cc-bus/` 被当本机、`$DAEMON_PATH`/`$ACCT_ISO_DEST` 是"配置项"——这三条都在等 T04
  把"某个工具在某个 origin 上的落点"这件事表达清楚。**不要在 T03 里顺手补**，会又造一个半成品。
- **T03 的正当性未受影响**（仍是 F08 别名生成器 + B04 钩子片段生成器两个真实消费者）。

## 8. 签收
- [x] 通过代码审计（2 阻塞 + 8 重要全部处置；另自查出 3 条更严重的一并修）
- [x] 通过工程审计
- [x] 主计划已据此更新（含变更记录）

### 本轮门禁

cargo test **497**（+5）· cargo fmt 0 · clippy 0 error · tsc 0 · npm test **776**（+1）·
shellcheck 0 · exec-bit guard 过 · **七套真机套件全绿**（26/44/12/15/13/14/11）。
tmux 走强制 `-L` 的 PATH shim，起飞前 canary 双向自检（正向落隔离 socket / 反向默认 socket 清单不变），
跑完再核一次——`cc-9d66c46d`（我自己）/ `cc-claudecode-frontend` / `cc-d7692cdf` 三个会话逐字未变。
