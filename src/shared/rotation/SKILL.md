---
name: rotation
description: cc-monitor 的账号轮换（第五版）怎么用、怎么写策略。后端给积木（每个号的事实 · 热换 · 停发 · 什么时候叫策略来判 · 推演 · 策略出错时照号列表）；「该用哪个号、什么时候换」全在策略里：一份普通的 Rhai 文件，文件名是标题，开头的注释是说明，接着是头（自己的设置和设置的声明），代码里一个 `decide(ctx)`。讲命令、策略文件的接口与积木库 `lib`、怎么试跑和推演、怎么装。不存策略、不常驻跑东西。用在——「写一份轮换策略」「按某某办法换号」「改轮换的设置」「这个会话为什么换到这个号」「试跑 / 推演一下」「装一份策略」。
---

# 账号轮换：积木 · 策略

一句话：**后端没有任何换号的想法。** 权重、权重容差、强制切换阈值、用量上限、耗尽后启用、启用延迟、单次用量上限，都是策略文件里的设置，由策略自己的代码解释（积木库 `lib` 只给算上限 · 余量 · 排先后 · 推演这类只算不决定的函数）。没有随包策略：策略文件都是用户自己的（自己写的 · 让 AI 照这份 skill 写的 · 升级时从旧配置换出来的）；没选策略的会话不轮换，只用开会话的号。

本 skill 不存策略、不替用户存草稿、不常驻跑任何东西；动 `rotation.json` 与策略目录只经 §2 的命令。

## 1. 后端给什么

- **事实**：每个号两窗（5 小时 · 7 天）的用量 · 重置时刻 · 用量涨得多快（额度账里量出来的，量不到就空）· 被拒到几点 · 登录 · 超额。
- **一判**：策略交回「此刻照什么先后用号」，后端取第一个事实上发得出去的号；跟此刻用的不同就热换；一个都没有就停发（回那一家的「用满」回包）到策略说的时刻。
- **什么时候判**：只在节点上判，结论缓存到下一个节点：被拒 · 重置 · 外来的事实变了 · 设置改了 · 策略约的时刻（`wake.at`）· 策略约的读数（`wake.when`）。节点之间一发都不叫策略。
- **用户拖出来的顺序**：到策略说该换的那一刻先用它（策略此刻不列的跳过）。
- **策略出错时**：策略编不过 · 跑出错 · 超时 · 回的形状不对 ⇒ 此刻的号发得出去就照用，否则照设置里号列表的先后取第一个发得出去的；界面上提示「策略出错」带行号。

## 2. 命令

都经 `~/.cc-monitor/bin/ccm -- --<命令>`：参数从 stdin 读一段 JSON，stdout 一行 JSON。

<!-- 命令：rotation-*（生成物：照 `ccm -- --commands` 现打，勿手改；重打见 `tests/backend/stream/catalog_tests.rs`） -->
- `ccm -- --rotation-facts`：每个号此刻的事实。参数 `RotationFactsReq`
- `ccm -- --rotation-try`：试跑一份策略。参数 `RotationTryReq`
- `ccm -- --rotation-strategies`：策略目录。参数 `RotationStrategiesReq`
- `ccm -- --rotation-view`：一个会话或一份策略此刻的轮换。参数 `RotationViewReq`
- `ccm -- --rotation-settings`：改一份策略的设置。参数 `RotationSettingsReq`
- `ccm -- --rotation-use`：会话载入一份策略。参数 `RotationUseReq`
- `ccm -- --rotation-reload`：在用旧版的会话都载入新版。参数 `RotationReloadReq`
- `ccm -- --rotation-for-new`：新会话用哪份策略。参数 `RotationForNewReq`
- `ccm -- --rotation-temp`：会话侧的临时动作。参数 `RotationTempReq`
- `ccm -- --rotation-use-now`：现在用这个号。参数 `RotationUseNowReq`
- `ccm -- --rotation-strategy-add`：加一份策略。参数 `RotationStrategyAddReq`
- `ccm -- --rotation-strategy-copy`：复制一份策略。参数 `RotationStrategyCopyReq`
- `ccm -- --rotation-strategy-rename`：给策略改名。参数 `RotationStrategyRenameReq`
- `ccm -- --rotation-strategy-delete`：删一份策略。参数 `RotationStrategyDeleteReq`
- `ccm -- --rotation-strategy-tz`：给策略设时区。参数 `RotationStrategyTzReq`

```ts
type AgentKind = string;
type RotEdit = { "set": { key: string, row?: number, v: unknown, } } | { "seg": { key: string, row?: number, window: string, at?: number, seg?: RotSeg, } } | { "same": { key: string, row?: number, window: string, same: boolean, } } | { "move": { from: number, to: number, } } | { "add": { account: string, } } | { "remove": { row: number, } };
type RotSeg = { from: string, to: string, v: number, };
type RotTarget = { sid: string, } | { strategy: string, };
type RotTempOp = { "move": { account: string, to: number, } } | "clear" | { "off": { account: string, off: boolean, } };
type RotationFactsReq = { 
/**
 * 哪一家（线上的 kind）；缺 ⇒ 有账号库的那一家。
 */
agent?: AgentKind, };
type RotationForNewReq = { 
/**
 * `null` ⇒ 新会话不轮换（只用开会话的号）。
 */
strategy: string | null, };
type RotationReloadReq = { strategy: string, };
type RotationSettingsReq = { strategy: string, edit: RotEdit, ifRev: string, };
type RotationStrategiesReq = Record<symbol, never>;
type RotationStrategyAddReq = { 
/**
 * 一份策略文件的原文（说明 ＋ 头 ＋ 代码）：从文件添加；缺 ⇒ 新建（照编辑器那一份，号列表 ＝ 账号库全部号，时区 ＝ 这台的）。
 */
source?: string, 
/**
 * 存成的文件名（＝ 标题）。
 */
name: string, };
type RotationStrategyCopyReq = { id: string, name: string, };
type RotationStrategyDeleteReq = { id: string, };
type RotationStrategyRenameReq = { id: string, name: string, ifRev: string, };
type RotationStrategyTzReq = { id: string, tz: string, ifRev: string, };
type RotationTempReq = { sid: string, op: RotTempOp, };
type RotationTryReq = { 
/**
 * 一份策略文件的原文（头 ＋ 代码）。
 */
source: string, 
/**
 * 换掉头里的设置（形状由这份策略自己定；缺 ⇒ 用头里的）。
 */
settings?: unknown, 
/**
 * 开会话用的号（缺 ⇒ 事实里第一个号）。
 */
start?: string, 
/**
 * 在用的号（缺 ⇒ 开会话用的号）。
 */
current?: string, 
/**
 * 临时条目。
 */
temp: Array<string>, 
/**
 * 往后推几小时（1–48，缺 ⇒ 12）。
 */
hours?: number, 
/**
 * 试跑「现在用这个号」：事实上发得出去 ⇒ 判的是换过去、放进拖出来的顺序最前之后的那一判；发不出去 ⇒ `useNowRefused` 带几点恢复。
 */
useNow?: string, 
/**
 * 哪一家（缺 ⇒ 有账号库的那一家）。
 */
agent?: AgentKind, };
type RotationUseNowReq = { sid: string, account: string, };
type RotationUseReq = { sid: string, 
/**
 * `null` ⇒ 这个会话不再轮换（只用开会话的号）。
 */
strategy: string | null, };
type RotationViewReq = { target: RotTarget, 
/**
 * 设置页那一形往后推几小时（1–48，缺 ⇒ 12）；会话那一形不画推演，不看它。
 */
hours?: number, };
```
<!-- /命令 -->

几样从形状上看不出来的：
- 会话用自己的副本：会话选一份策略那一刻载入它（代码 ＋ 设置 ＋ 版本），之后文件再改不跟；载入新版才换（选同一份 · 或 `--rotation-reload`）。新会话默认不轮换。
- 看会话（`target: {sid}`）⇒ 照它载入的那份副本，不带推演；顺序 `lineup` 一号一行，`group`：`use` 在用 · `next` 接下来 · `out` 此刻用不了 · `off` 先不用；`state` 第二格推得出就带「约 HH:MM」。看策略（`target: {strategy}`）⇒ 照文件此刻的：每一格的值与从哪来（自己设的 · 跟总体 · 默认）· 每个号的事实 · 这一判 · 推演 `timeline`（`hours` 只对它）。
- 改设置一次一个意图：`seg` 里给 `at` 和 `seg` ⇒ 改第 `at` 段，只给 `at` ⇒ 删那一段，只给 `seg` ⇒ 加一段；`same` ⇒ 后面那一窗同第一窗 / 另设。照声明改完、校验，当场存成新版（只重写头那一块，说明和代码一个字不动）。
- 临时动作只动这个会话、不改策略：`move` 把一行挪到第 `to` 格（第 0 格 ＝ 现在用它）· `"clear"` 清掉拖出来的先后 · `off` 先不用某个号。现在就换号只拿事实拦（被拒 · 没登录 · 没 key · 用满）。
- 策略文件：改名时指它的会话与「新会话用」跟着改；有活会话在用 · 是新会话用的那份 ⇒ 不删；改时区只换设置块里的 `tz`。名字：1–32 字，字母数字（含中文）与 ` -_·()（）+`；同名已有 ⇒ `bad_args`。
- 试跑拿一份原文照此刻的事实判一次、往后推演，不写盘不换号。

## 3. 策略文件

`~/.cc-monitor/strategies/<标题>.rhai`：**标题就是文件名**（去 `.rhai`，也是 id）。文件分三段：

```rhai
// 换号时按权重排序，权重相差在权重容差以内算同权，同权按账号列表顺序。
// 在用账号未达用量上限时，重置早于它、且权重超出它强制切换阈值的账号立即切入。

/*--- rotation
{ "format": 5, "tz": "Asia/Shanghai",
  "settings": { "caps": {"5h": [{"from": 0, "to": 1440, "v": 90}]}, "close": 25, "force": 25,
                "accounts": [{"start": true}, {"id": "lab"}, {"id": "team", "fallback": true, "delay": 30}] },
  "form": [ {"key": "caps", "kind": "daySegments", "scope": "each", "inherit": true, "label": {"zh": "用量上限"},
             "help": {"zh": "账号各窗口用量的上限，可按时段分段"}, "windows": ["5h", "7d"], "default": 100},
            {"key": "close", "kind": "percent", "scope": "all", "label": {"zh": "权重容差"}, "default": 0},
            {"key": "force", "kind": "percent", "scope": "all", "nullable": {"zh": "不强制切换"}, "label": {"zh": "强制切换阈值"}, "default": null},
            {"key": "fallback", "kind": "toggle", "scope": "each", "label": {"zh": "耗尽后启用"}, "default": false},
            {"key": "delay", "kind": "minutes", "scope": "each", "label": {"zh": "启用延迟"}, "default": 0} ] }
---*/

fn weight(a, left, mins) { if mins == () { 0.0 } else { 300.0 / mins } }
fn decide(ctx) { … }
```

- **说明**：文件最开头连着的 `//` 行（到第一行不是 `//` 的为止；行首的 `// ` 去掉，空的 `//` 行分段）。界面只显示它、不改它。只讲怎么排、怎么换，**不写设置的值**（值在设置里，界面一改就旧了）。
- **头**（`/*--- rotation … ---*/`）：`format` · `tz` · `settings` · `form`。头里**没有** `name` · `line`（写了 ⇒ 读不懂、形状不对）。后端存设置只重写这一块，说明和代码一个字节不动。
- `settings`：这份策略自己的设置，形状它自己定；号列表固定在 `settings.accounts`（`{start: true}` ＝ 开会话用的号），**号列表的先后就是默认优先级**（权重同权时照它）。
- `form`：设置怎么画、怎么校验。格的种类：`percent` · `minutes` · `toggle` · `choice` · `daySegments`；`scope: all` 画在总体设置，`each` 画在每个号下面（`inherit: true` ＝ 号没另设就跟总体）；`showIf: {key, v}` ＝ 那一格（开关或几选一）取这个值才出；`daySegments` 的 `follow: true` ＝ 后面几窗没另设同第一窗；`lead` ＝ 数格前面那半句；选项的 `mark: {glyph} | {dashed: true}` ＝ 号取这一项时号名旁的记号。`label` 用短的正式名字（用量上限 · 权重容差 · 强制切换阈值 · 耗尽后启用 · 启用延迟 · 单次用量上限），定义写在 `help` 里；不用第二人称，不写讲结构的字。
- `ctx`：`now` · `clock`（策略时区的本地分钟）· `start` · `current` · `settings` · `memo`（上一判交的，原样带回）· `accounts[{id, pos, kind, sendable, used, resets, refusedUntil, rate, tokensPerPct, weekly, off, others}]`（`rate` 用速、点 / 分；`tokensPerPct` 1% ≈ 多少用量、折合输入 token，套餐不同就不一样大，余量跨号比时乘它；两样都是这个号自己量出来的；`weekly` 有周上限；`off` 本会话先不用；`others.w5h` 别的会话压在它 5 小时窗上的用速、点 / 分）· `session{pace, paceShort, inflight, bins, spread, context, writeExtra, ttl, grow, cached}`（本会话：用速、折合输入 token / 分、最近半小时 · 同一口径的最近 5 分钟（会话记满 2 分钟才给）· 最近 5 分钟平均几发同时在途（子 agent · 工作流的请求一并算）· `bins` 最近 6 小时每 10 分钟用了多少、折合输入 token，恒 36 格、旧的在前、最后一格到此刻，会话记到之前的格是 `()` · 起伏 σ/μ · 上下文 token · 写缓存比读缓存多几倍 · 缓存留几秒 · 上下文每分钟长多少 · `cached{号: {until, tokens}}` 各号上的缓存到几点、当时多长；`until` 是 Rhai 的关键字，照 `c["until"]` 取）· `pinned`（在用的号是手动选的）· `temp`（手动排的先后）。量不到的格 ⇒ `()`。各会话的判依次来：后判的看得见先判的刚换到哪。没有文件 · 网络 · 钟 · 随机数。
- `decide(ctx)` 交回 `#{order: [号…], why: #{号: "一句"}, hold_until?, wake: #{at?, when: [#{acct, w, used_at_least}]}, warm?: [号…], memo?}`。后端照 `order` 第一个事实上发得出去的号用：**在用的号排第一就是不换**。
- `warm`：要开 5 小时窗的号。后端只收订阅号 · 此刻发得出去 · 5 小时窗没在计时（没见过重置时刻 · 重置时刻已过）的，用那个号发一句最短的话（与 quota-warm 同一处发法），同一个号 15 分钟内只发一次；不收的记一句日志。不交 ⇒ 不开。

**换谁、什么时候换**（常见的写法，全在策略自己的代码里）：
- 用满了换谁：每个号算一个权重，从高到低；较高的超出较低的不到权重容差就算同权，照号列表先后。
- 强制切换阈值：在用的号还没到用量上限时，只有重置比它早、权重又超出它这一比例的号才直接换过去（后重置的号剩的不会先作废，不必抢；这样也不会来回换）。不设 ⇒ 用满才换。
- 耗尽后启用：其余的号都没额度时才用它，其余的号一有额度就换走；启用延迟 ＝ 其余的号都没额度满这么久才用；单次用量上限 ＝ 每次换过来 5 小时用量涨到这么多就停用，直到其余的号恢复。起点和记账放在 `memo` 里。
- 权重会随时间、随用量变：开着强制切换阈值时，算出它几时超过在用的号，用 `wake.at`（时刻）· `wake.when`（读数）约下次来问；后端只在这些节点上判。
- 一个都没有：`order` 给空 ⇒ 停发到 `hold_until`。

**积木库 `lib`**（只算、不决定，跟哪种权重都无关）：
- `cap_at(segs, clock)` · `next_edge(segs, clock)` · `segs_of(s, r, w)`：分段上限、下一条边界、号另设还是跟总体；
- `cap_of(s, r, w, lift, clock)` · `left_of(s, r, a, lift, clock)` · `has_quota(s, r, a, lift, clock)`：此刻上限、余量（两窗取小）、有没有额度；`lift` ＝ 放开软上限（`capKind: soft` 的号当 100）；
- `rank(ids, pri, close)`：按权重排先后，`close`% 以内同权、照 `ids` 先后；
- `delay_start(prev, holds, now)` · `delay_over(start, minutes, now)`：启用延迟的起点（存进 `memo`）、到点没有；
- `hold_from(s, rows, accts, ids, now, clock, extra)`：一个都没有时停到几点；
- `cap_or(segs, clock, dflt)`：同 `cap_at`，落空取 `dflt`；`used_of(a, w, now)`：一窗此刻的用量（过了重置 ⇒ 0）。

智能切换用的几样（编辑器那一份 `editor.rhai` 照它们编排；单位：额度是百分点，用量与代价是折合输入 token）：
- `piece(a, c5, c7, now)`：这个号此刻那份额度 `#{amt, exp, wk, open, l7}`（余量 · 几时作废 · 周上限卡着 · 5 小时窗开着 · 周剩的）；作废看卡住它的那一窗；
- `due_rank(p, others, now)`：「先用最先作废的」先后的键（扣掉别的会话到作废前会用掉的；没开窗的 · 周上限卡着的排后）；
- `switch_cost(sess, id, now)` · `write_extra(sess)`：换到 `id` 当场要重写的缓存（缓存期内回去只补写新长的）· 写缓存比读缓存多几倍；
- `usable(free, pace, tpp, minutes)`：本会话在一份额度作废前用得掉多少；`pace_bands(spread)`：用速的几档（量不到起伏只一档，否则 μ、μ ± √3σ）；
- `refills(r5, left7, wk, exp, segs5, dflt, clock, now, horizon)`：视野里每次 5 小时重置后再来的那几份；
- `project(amt, st, ex, who, wk, late, oev, o)`：往后推演一种走法，回卡住的 ＋ 用掉的周上限卡着的 ＋ 一路换号重写缓存的（参数见 `lib.rhai` 里的注释）；一遍约 600 × 份数次操作，一判有 20 万次的上限，号多时先算推得起几遍。

## 4. 怎么装、怎么试

1. 写好原文，`--rotation-try {source}` 看这一判和推演对不对；
2. `--rotation-strategy-add {source, name}` 装进策略目录（文件名 ＝ 标题）（读不懂 ⇒ `bad_strategy` 带行号）；
3. 会话改用它：`--rotation-use {sid, strategy}`；新会话都用：`--rotation-for-new {strategy}`。
