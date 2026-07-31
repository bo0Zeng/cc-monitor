# G3a — 分叉起会话的参数推断（「知道 / 不知道」的内核）

G3 原本是一个功能，开工后拆成两半。**本文是 a 半：推断内核**。
b 半（接进两条 spawn 路 + 追问 UI）为什么单拆，见 §4。

## §1 ★ 第一步是查清，不是设计：**账号在会话退出后还原不出**

主计划把这条列为最高风险，要求「先实测」。三条路全查过：

| 路 | 实测结论 |
|---|---|
| pidfile `sessions/<PID>.json` | **进程退出即消失** ⇒ 只对活着的会话有效 |
| jsonl 所在路径 | 各账号的 `projects/` 是**软链到共享的 `~/.claude/projects`**，同一个 inode（实测 `stat -c %i` 一致）⇒ **路径不编码账号**。这是 cc-acct-iso「隔离又同步」的设计：凭据分家、会话历史共享 |
| jsonl 内容 | 一份 9981 行真语料里 44 个顶层键，**没有**任何账号 / 邮箱 / configDir 字段 |

⇒ **对已退出的会话，账号一律 `unknown`。**这不是「暂时没做」，是**没有信息源**。

**我一度以为路径能答**（各账号有自己的 `projects/`），是那个「37/37/37 三个目录数量一模一样」
的巧合让我起疑才去查 symlink 的 —— 记在这里，免得下一个人重走一遍。

## §2 为什么不肯拿「当前账号」顶替

那会**静默地用错身份跑一条对话**：你从三个月前某条消息分叉，它拿今天的账号起会话，
界面上看不出任何异样。同 `readiness.ts` 那条「缺 ≠ 不知道」——不知道就说不知道，多问一次。

`inferForkLaunch` 因此有一条**刻意的防线**：`sourceIsLive === false` 时**根本不看**
`liveConfigDir` 传了什么值。调用方很可能顺手把「当前账号」塞进来，这里必须挡住。
变异 P1（把这条判断短路）**立刻见红**。

## §3 交付物

`src/fork-launch.ts`（纯函数，不碰 IO）：

- `Slot<T>` = `known(value, from)` | `unknown(why)` —— **每一格都要说出处或说不出的原因**。
- `inferForkLaunch(input)` → `{ cwd, account, tmux }`。
  - `cwd` 来自 jsonl 的 `cwd` 字段 ⇒ **历史会话也答得出**。
  - `account`：活着 → pidfile；已退出 → unknown。**`null` 是「知道，账号 0」，不是 unknown**。
  - `tmux`：活着 → tmux 清单；已退出 → unknown。
- `slotsNeedingInput()` → 要追问哪几格（**齐全时返回空**，否则每次分叉都弹窗，功能就废了）。
- `forkTmuxName(source, taken)` → **必须与原会话不同**，否则 `ccm` 会把新会话 attach
  进原窗口，正好毁掉「两条都活着」。保持 `<X>-cc` 后缀形状（`ccm` 靠它认自己的会话）。

12 条测试；变异 4 条（退出码判定，改完先 grep 计数确认落地）：

| # | 变异 | 结果 |
|---|---|---|
| P1 | 已退出时也信 `liveConfigDir`（= 拿当前账号顶替） | 红 |
| P2 | 账号 0（`null`）被误判成 unknown | 红 |
| P3 | fork 的 tmux 名与原名相同 | 红 |
| P4 | 齐全时也去追问（每次分叉都弹窗） | 红 |

## §4 b 半为什么单拆：**本地那条 spawn 路带不了账号**

复核两条 spawn 路：

| 路 | 能不能带账号 |
|---|---|
| **远端** `launch_remote_terminal` + launch IR | **能**。IR 早有完整的 `account` 维度（`--account <name>` / `--base` / `export CLAUDE_CONFIG_DIR`） |
| **本地** `resume_history_session` → `launch_local` → `build_local_ps_command` / `build_local_posix_command` | **不能** —— 两个命令构造器都不注入 `CLAUDE_CONFIG_DIR` |

**订正一处我先前的话**：我一开始说「要给后端加账号能力」，那是只看了
`resume_history_session` 一个命令就下的结论，**过宽了** —— 远端那条路本来就有。
准确说法是：**分叉目前绕开了 IR 直接拼 resume 命令**，而本地 spawn 确实缺账号注入。

⇒ b 半的范围（**未做，不标完成**）：
1. 分叉的起会话改走 IR（远端可立即完整实现）。
2. 本地 PS/POSIX 命令构造器补 `CLAUDE_CONFIG_DIR` 注入（Windows 面，只能单测覆盖）。
3. `unknown` 那几格的追问 UI（一次性小窗，默认照搬、可改）。

今天 `⑂` 只对**本地**会话出现（`opts.origin` 一非空就关掉），所以讽刺的是：
**现在唯一能分叉的那条路，恰好是带不了账号的那条。** G6 把远端门打开之后这个倒置会消失。

## §5 门禁

vitest **1060 / 73 文件**（+12）· tsc 0 · 前端其余未动。

## §6 签收

- [x] 过代码审计（4 条变异，核心防线 P1 见红）
- [x] 过工程审计（b 半的边界与理由如实写明，未标完成）
- [x] 主计划已更新
