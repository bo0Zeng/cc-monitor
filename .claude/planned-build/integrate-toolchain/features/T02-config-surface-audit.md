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
