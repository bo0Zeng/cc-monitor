# Z02 — 「未选账号」消失（**部分交付 + 部分卡红线**）

> 主计划：`../MASTERPLAN.md` §1 Z02（第 209 行）· §3 账本第 3/4/5 行 · §6 开放问题 2
> 前置：**Z01**（`7ad1ae3`）· **Z04**（`f97bb76`）
> 状态：**跨语言契约守卫已交付**；**UI 三态化卡 `tabs.ts` 红线，未做**

## 1. 开工复测：计划的文件清单不准

| 计划说（§3 账本第 5 行） | 实测 |
|---|---|
| 7 个文件 | 生产代码 **10 个**（另加 7 个测试文件也提「基座」，全仓 17） |
| `tabs.ts` **~14 处** | **20 处**「基座」 |
| `views/history.ts` 是其中之一 | **0 处**。计划把它列进来是错的 |
| `accounts.ts:164` | 实测「基座」在 `accounts.ts` 有 4 处，`kind:"base"` 的类型定义在 `:530` |
| 计划没提 | `ipc/commands.ts` · `launch-dimensions.ts` · `launcher-diagnostics.ts` · `launch-plan.ts` 四个生产文件也在其中 |

## 2. ★ 最重要的实测：**Z01 记错了一句话**

Z01 在 `accounts.ts::isSelectable` 上方写死了一段理由：

> 「从 UI 起账号 0 需要『显式 unset `CLAUDE_CONFIG_DIR`』这条注入路径，
> 而 `launch-plan` 今天只会 export。」

**这句是错的。** 那条路径**早就有了**，而且两条渲染路各一份：

| 渲染路 | 怎么 unset |
|---|---|
| **CLI 路径** | `ACCOUNT_DIMENSION.cliFlags` 对非 `account` 态吐 `--base`；`shared/ccm` 收到 `--base` 会 `unset CLAUDE_CONFIG_DIR`（**两处落点**：send-keys 载荷行 `:572` + 会话级 env `:598`） |
| **兜底渲染路径** | `ENV_RESET_DIMENSION` 推 `{kind:"unset-config-dir"}` op，`launch-render-fallback.ts:23` 渲染成 `unset CLAUDE_CONFIG_DIR; ` |

**根因**：我在 Z01 里只查了 `launch-dimensions.ts::ACCOUNT_DIMENSION.apply`（那里确实只 `push export-config-dir`），
**没往下查 `cliFlags` 与 `ccm`**。这与 Z07 把 D2 的推理照搬给 D1b 是同一类错：
**在一个层面看到的事实，被当成了整条链路的事实。**

⇒ 已订正三处：`accounts.ts::isSelectable` 的注释（写清真正缺的是什么）、Z01 feature 文档、STATUS 的登记项。

### 2.1 那么真正卡住的是什么

不是注入形态，是**选择链路**：

1. `accountConfigDir()` 对账号 0 返回 `null` ⇒ `resolveAccount` **说不出**「用户显式选了账号 0」
   （只能说 `unavailable` = 「你要的号不能用」，语义完全不同）
2. `AccountModifierOption` 里没有账号 0 这个选项（`launch-menu.ts:74` 只有 `base` 和具名账号）
3. **`tabs.ts:2283`**：`opt.kind === "base" ? containerLeaves(undefined, true) : containerLeaves(opt.name, false)`
   —— 加一个 `{kind:"account0"}` 变体**不会编译报错**，它会被静默送进 else 分支，
   拿 `opt.name === undefined` 去起会话。**加变体前必须先改它。**

第 3 条卡 `tabs.ts` 红线。

## 3. 本轮交付：一条没人钉的跨语言契约

monitor 侧整套「基座 = 不注入」的语义，**全部压在一个没有任何东西钉住的假设上**：

> `--base` 吐出去之后，`shared/ccm` 会照它 `unset CLAUDE_CONFIG_DIR`。

今天 `launch-dimensions.test.ts:107` 只断言 monitor **发**了 `--base`；
**没有任何东西断言 ccm 会照它 unset**。

**这条漂了会怎样**：CLI 路径起出来的会话继承远端 shell 里那句
`export CLAUDE_CONFIG_DIR=<默认账号>`（`cc-acct-iso shellinit` 生成的就是这一句）
⇒ **用户以为在起账号 0，实际烧的是默认账号的额度**。UI 上完全看不出来
—— 正是 BACKLOG E37 那类「静默共用身份」的形态。

⇒ 新增 `src/base-flag-contract-guard.vitest.ts`（8 条），照
`tmux.rs::tmux_ls_fmt_double_write_point_stays_in_sync` 的范式：读**另一侧源文件** + 锚定那几行。
**`shared/ccm` 是红线，本文件只读它。**

钉住四件事：① ccm 认识 `--base` ② 载荷行会 unset ③ 会话级 env 也会 unset
④ `--account`/`--base` 互斥仍在（否则可能同时 export + unset，结果由顺序决定）。
外加**两处计数 == 2**（只钉一处的话，另一处被改掉守卫照样绿）与一条反向自检（真读到了文件）。

## 4. 变异验收（Phase D）

**`shared/ccm` 是红线（不改本体）** ⇒ ccm 侧的四条变异在 **scratchpad 副本**上做，
逐条验锚点有判别力；monitor 侧那条直接改源码再回滚。

| 变异 | 结果 |
|---|---|
| **M1** 拿掉载荷行的 unset | **成立**：红「落点 1」+「两处计数」 |
| **M2** 拿掉会话级 env 的 unset | **成立**：红「落点 2」+「两处计数」 |
| **M3** `--base)` 改成什么都不做 | **成立**：红「认参数」 |
| **M4** 拿掉 `--account`/`--base` 互斥 | **成立**：红「互斥」 |
| **M5**（monitor 侧，真改源码再回滚）`cliFlags` 不再吐 `--base` | **成立**：红「monitor 对非选中账号态吐 --base」；先 `tsc` 确认编译过再判色 |

**如实标注**：铁律 8 要求并行多 agent 审计；本会话常驻指令「除非用户要求不开 agent」
⇒ 主线程变异 + 全门禁代替。**这是欠账，不是强度裁剪。**

## 5. 刻意**不做**的：UI 文案

计划账本第 5 行要求把「基座」一词从用户可见文案里换成「账号 0」。**本轮不做**，理由是硬的：

> 在菜单还**不能真的选**账号 0 之前，把「基座（不隔离）」改叫「账号 0」，
> 就是把**「没选」标成「账号 0」** —— 正是 Z02 要消除的那个合并，方向反了。

**「没选」不等于「选了账号 0」**：
- 「选了账号 0」= 用户明确要不设 `CLAUDE_CONFIG_DIR`
- 「没选」= 系统替他决定了，而今天这个决定**恰好**渲染成同一个 `--base`

两者今天渲染结果相同，**但意图不同**，而 UI 只能说出意图。⇒ 文案必须跟着语义一起改，不能先改。

## 6. 一旦 `tabs.ts` 红线松开，先做这几步（顺序有依赖）

1. **`tabs.ts:2283`** 那个三元先改成 `switch`/穷尽判别（**先做这步**，否则第 2 步会静默错）
2. `launch-plan.ts`：`LaunchAccount` 加 `{kind:"account0"}`；`{kind:"base"}` 的**文档**改成
   「没选（尚未消除）」，别急着删——删它会让 `launch-requests.ts:28/179`、`resolveAccount`
   两条下沉分支同时失去落点
3. `launch-dimensions.ts`：`cliFlags` 对 `account0` 吐 `--base`（**与 base 同渲染、不同意图**）；
   `apply` 对 `account0` 不推 env op
4. `accounts.ts`：`isSelectable` 放开 `mode === "bare"`；`resolveAccount` 增 `account0` 出口
5. `launch-menu.ts:74`：菜单项从「基座（不隔离）」换成账号 0，`kind` 用 `account0`
6. **最后**才是 20 处 `tabs.ts` + 4 处 `settings/*` 的文案
7. Z03 接在这之后（用量探针 + 按会话切号）

## 7. 门禁

| 门禁 | 前 | 后 |
|---|---|---|
| vitest | 847 / 56 files | **855 / 57 files**（+8，新守卫） |
| tsc · eslint · fmt | 0 · 7 基线 · clean | 同左 |
| cargo / daemon / vendored 自测 | 618 · 149 · 294 | **不变**（Z02 只碰 TS） |

## 8. 签收（部分）

- [x] 交付：`--base` 跨语言契约守卫（五条变异全部成立）
- [x] **订正 Z01 的一句错记录**（注入形态早已存在；真正缺的是选择链路）
- [x] 订正计划的文件清单（10 个生产文件而非 7；`tabs.ts` 20 处而非 ~14；`history.ts` 是 0）
- [ ] **UI 三态化：卡 `tabs.ts` 红线，未做** —— 需用户松开红线，恢复步骤见 §6
- [ ] UI 文案：**刻意不先做**（§5 论证：先改文案 = 把「没选」标成「账号 0」）
