# S26-A1 · 「有代码、但现在的设计里没有」全仓普查 —— 读数

> **这一路只产清单，不删一行代码，也不改 `src/`。**
> 下面每个数都是 **2026-09-18 现打**，没有一个是从别的文档抄来的
> （本轮已逮到 `99 §3.2` 那张表的「文档提过吗」一列整列腐了，见 §4）。
>
> 量具：`tests/evidence/S26-A1-design-coverage-census.py`（定义写死在它的头注）
> 复算（仓根下）：
> ```
> python3 tests/evidence/S26-A1-design-coverage-census.py              # 主报告（十一面）
> python3 tests/evidence/S26-A1-design-coverage-census.py --addresses  # 逐项住址 + 判法
> python3 tests/evidence/S26-A1-design-coverage-census.py --json       # 机读
> python3 tests/evidence/S26-A1-design-coverage-census.py --selftest   # 反空真死值验（五刀）
> python3 tests/evidence/S26-A1-design-coverage-census.py --backtest   # 条 67 那一刀的标定
> ```
> 仓位：`f3e78788` / `wave0/delete-usage-and-fix-gate`
> 设计语料：`调研/设计/*.md` **22 篇**（非 git 仓，读的是 2026-09-18 当时的盘面）

---

## 0. 先看这三格 —— 尺子有没有在切东西

普查最容易出的错是「报了个 0，而那个 0 是尺子卡住了」。所以先看对照。

| 对照 | 怎么验 | 本轮结果 |
|---|---|---|
| **反空真地板** | 12 条分母地板，任一触底 ⇒ 退出码 3 | **12/12 过**（见 §1） |
| **历史标定** `--backtest` | 语料换成 `4f472059^`（删 `sidecars/` 之前那一刻），验**已知答案**能复现 | **过** —— 那 4 个文件全报「入口走不到」 |
| **活体标定** `面⑪` | 拿**代码自己写的那句话**当标准答案 | **过** —— `plugin/mod.rs` 自陈「`probe` 今天零生产调用方」，本量具量到 `unreachable` |

⇒ 所以下面那个「**甲档 0 项**」是**真空的**，不是量不出来。

### 反空真死值验（`--selftest`）—— 五刀，每刀点名它该让谁红

| 刀 | 变异 | 该让谁红 | 结果 |
|---|---|---|---|
| 1 | 语料换成空目录 | 任意地板 | ✔ 触底 **8** 条 |
| 2a | 设计正文清空 | 丙档的「裁定锚文本」判据 | ✔ 当场红（丙档在脚本里**没有副本**） |
| 2b | 设计正文清空（丙档表摘掉） | 候选必须暴涨到人群全体 | ✔ 甲+乙 = **251 / 251** |
| 3 | 把 `mod x;` **当成一条边** | 历史标定 | ✔ 失真（`main.rs` 一步走到 `sidecars/`） |
| 4 | **不剥 `#[cfg(test)]`** | `judged_files` 地板 | ✔ 判据层 41 → **18**，塌到地板 25 以下 |
| 5 | **不遮注释** | 活体标定 | ✔ 1/1 条红 |

> 🔴 刀 3/4/5 是**逐个**关，不是一起关。第一版是「三样一起关」，结果三样全关标定照样过
> —— 那说明那一刀根本没碰到承重的那一步。**一起关只能证明它们合起来有用，
> 逐个关才能证明每一条都在承重。**

---

## 1. 面① · 人群与分母

| 量 | 现打 | 地板 | |
|---|---:|---:|---|
| 生产文件 | **251** | 200 | ✔ |
| 生产行（去空行 · Rust 已剥 `cfg(test)`） | **57 307** | 45 000 | ✔ |
| 　· `.ts` | 120 个 / **25 985** 行 | | |
| 　· `.rs` | 131 个 / **31 322** 行 | | |
| 设计篇 | **22** | 20 | ✔ |
| 设计正文（去否定区后） | **500 494** 字符 | 300 000 | ✔ |
| 从设计抽出的带后缀住址 | **485** | 150 | ✔ |
| 从设计抽出的目录住址 | **452** | 20 | ✔ |
| TS import 边 | **381** | 300 | ✔ |
| Rust 生产引用边 | **432** | 400 | ✔ |
| T1（全路径逐字）命中 | **76** | 40 | ✔ |
| 丁档（设计提过也在做） | **206** | 100 | ✔ |
| 甲+乙 | **40** | 5 | ✔ |
| 出人群的「判据层」 | **41** 个 / 26 915 总行 | 25 | ✔ |

### 设计点名的档位分布（每个文件取最强的一档）

| 档 | 判法 | 项数 |
|---|---|---:|
| T1 | 全相对路径逐字出现 | 76 |
| T2 | 父目录 + 基名 | 25 |
| T3 | 基名 | 72 |
| T4 | 多词 stem（含 `_`/`-`） | 10 |
| T4b | 单词 stem，且长成代码 token | 11 |
| T5 | **只到目录级** | 17 |
| — | **一个字都没有** | **40** |

---

## 2. 四档读数

| 档 | 判法 | 项数 | 生产行 | 总行 |
|---|---|---:|---:|---:|
| **甲** | 设计零提及 ＋ 入口走不到 | **0** | 0 | 0 |
| **乙** | 设计零提及 ＋ 入口走得到 | **40** | **3 903** | 7 351 |
| **丙** | 设计提过、判成搁置（codex） | **5** | 398 | 825 |
| **丁** | 设计提过、也在做 | 206 | — | — |

### 甲 = 0 的两条旁证（别把它读成「都设计过了」）

1. 全仓「入口走不到」的文件今天只有 **1 个**：`src/backend/plugin/probe.rs`
   （114 生产行 / 424 总行）—— 而它**设计提过**（`设计/00` 的 `plugin/` 树里点了 `probe.rs`），
   且 `plugin/mod.rs` 头注已自陈「零生产调用方」并挂了 `allow(dead_code)`。⇒ 它是丁，不是甲。
2. 条 67 那一刀之后，`src/backend/sidecars/` 那一族已经没了；
   `--backtest` 证明**同一把尺子在那一刻能抓到它**。

### 乙的 40 项按族归堆（这一档才是可裁的）

| 族 | 项数 | 生产行 | 成员（生产行） |
|---|---:|---:|---|
| **拉起 / launch** | 6 | **670** | `remote-launch-run.ts`(350) · `remote-launch.ts`(86) · `launch-dimensions.ts`(96) · `launch-render-cli.ts`(66) · `launch-cli-wire.ts`(48) · `launch-menu.ts`(24) |
| **bridge Rust 散件** | 5 | **810** | `session_map.rs`(406) · `subagent.rs`(154) · `drift_ledger.rs`(115) · `fenced_block.rs`(87) · `verified_write.rs`(48) |
| **views 小件** | 8 | **555** | `port-forward.ts`(173) · `inbox-view.ts`(147) · `pane-preview.ts`(69) · `cc-bus-view.ts`(53) · `history-prefs.ts`(44) · `history-actions.ts`(35) · `counted.ts`(21) · `history-cache.ts`(13) |
| **分叉 / fork** | 5 | **493** | `fork-flow.ts`(150) · `fork-ask.ts`(143) · `fork-start.ts`(80) · `branch-button.ts`(63) · `fork-launch.ts`(57) |
| **远端配置 / 健康** | 3 | **239** | `remote-config.ts`(199) · `remote-health.ts`(32) · `remote-health-throttle.ts`(8) |
| **面板** | 2 | **308** | `tasks-panel.ts`(185) · `agents-panel.ts`(123) |
| **其余散件** | 11 | **828** | `error-toast.ts`(119) · `behavior.ts`(104) · `e2e-probe.ts`(95) · `turn-notify.ts`(80) · `first-run-hint.ts`(74) · `agent-profile.ts`(68) · `tmux-sessions.ts`(48) · `session-backend.ts`(42) · `live-window.ts`(37) · `account-restart.ts`(134)〔注〕 · `session-status.ts`(27) |

〔注〕`account-restart.ts` 也可以归进「拉起」族，这里按它的文件名归了散件 —— **分族是人分的，不是脚本分的**，边界处按哪边算都对，别拿它当判据。

逐项的四问答案（① 判法 · ② 调用方名单 · ③ 删了会红哪几格 · ④ 行数）见主报告面④，
或 `--addresses` 的全量一行一项。

### 丙 · codex 那一族（`99 §3.3` / 条 16：用户 09-17 明确搁置）

| 住址 | 生产行 | 总行 | 族外调用方 |
|---|---:|---:|---:|
| `src/bridge/src/codex_record.rs` | 261 | 382 | 2 |
| `src/backend/agents/codex/parse.rs` | 83 | 274 | 11 |
| `src/bridge/src/adapter/codex.rs` | 43 | 75 | 1 |
| `src/backend/agents/codex/mod.rs` | 6 | 53 | 1 |
| `src/backend/agents/codex/resume.rs` | 5 | 41 | 1 |
| **合计** | **398** | **825** | |

🔴 **这 398 行不是 `99 §3.3` 说的那 3 186 行** —— 两个量不可比，别混：

- `99 §3.3` 数的是**前端** codex 适配（`cards/index.ts` 为主）。而那一侧的 codex 适配
  是**文件内的分支**，不是独立文件：现打 `src/cards/index.ts` 里「codex」出现 **1 次**、
  `src/agent-profile.ts` **3 次**。⇒ **文件级的普查看不见它**，本量具的射程到不了
  （头注排除项第 6 条：符号级/行级残留不在射程内）。
- `src/bridge/crates/codex-token-core/`（**87 行**）不在用户给的人群里（crates/ 排除）。
- ⚠ 顺带两处文档腐：`99 §3.3` 写「`cards/index.ts` 1125 行」—— **现打 1 201**。

---

## 3. 「宽」买到了什么 —— 两条弱覆盖带，要人核

纪律 2 要求匹配**宁可宽**（错要错成「漏报一项待裁」，不许错成「把设计过的东西列进删除候选」）。
代价就是下面这两带：它们**算作「设计提过」（丁）而没进候选**，但点名得很弱。

### T5 · 只到目录级 —— 17 项 / 2 724 生产行

它们**没有被逐个点名**，只是住在一个被点名的目录里。

| 住址 | 生产行 | 凭哪一篇的哪个目录 |
|---|---:|---|
| `src/settings/mcp-section.ts` | 651 | `10` 提了 `src/settings` |
| `src/settings/cc-bus-section.ts` | 534 | 同上 |
| `src/keybindings/editor.ts` | 261 | `10` 提了 `keybindings` |
| `src/settings/cc-bus-hooks-section.ts` | 242 | `10` 提了 `src/settings` |
| `src/settings/daemon-section.ts` | 195 | 同上 |
| `src/cards/interactive.ts` | 119 | `17` 提了 `src/cards` |
| `src/settings/plugins-section.ts` | 107 | `10` 提了 `src/settings` |
| `src/settings/machine-status.ts` | 102 | 同上 |
| `src/cards/subagent.ts` | 101 | `17` 提了 `src/cards` |
| `src/settings/restart-notice.ts` | 81 | `10` 提了 `src/settings` |
| `src/settings/acct-deploy.ts` | 75 | 同上 |
| `src/panorama/subgraph-layers.ts` | 72 | `15` 提了 `panorama` |
| `src/settings/info-icon.ts` | 69 | `10` 提了 `src/settings` |
| `src/bridge/src/backend/control/daemon_send_keys.rs` | 53 | `00` 提了 `src/bridge/src/backend` |
| `src/panorama/session-files.ts` | 26 | `15` 提了 `panorama` |
| `src/keybindings/store.ts` | 25 | `10` 提了 `keybindings` |
| `src/ipc/local-tmux-name.ts` | 11 | `00` 提了 `src/ipc` |

🔴 **其中 9 项是 `src/settings/` 的 section（1 656 生产行）**，而 `设计/70` 就是设置界面那一篇。
现打 `设计/70` 逐字点名的 `.ts` 只有 7 个：`config-surface-section.ts` · `data-section.ts` ·
`diagnostics-section.ts` · `panel.ts` · `router.ts` · `events.ts` · `machine-context.ts`。
⇒ `mcp-section` / `cc-bus-section` / `cc-bus-hooks-section` / `daemon-section` /
`plugins-section` / `machine-status` / `restart-notice` / `acct-deploy` / `info-icon`
**一个字都没被点名**。`70` 里 MCP 出现 3 次、cc-bus 4 次 —— 是**概念上**碰到了，
**住址上**没有。⇒ 「设置界面的设计今天不完整」这句话（`99 §3.2` 末尾）依然成立，
只是缺的那几块**不是它原先列的那三个**（那三个 `70 §10` 已经补了）。

### T4b · 靠一个单词 stem 命中 —— 11 项 / 703 生产行

这是**最弱的一档**：bare word 匹配，那个词很可能说的是**另一个同名文件**。

| 住址 | 生产行 | 命中的词 | 出自 | 核过没有 |
|---|---:|---|---|---|
| `src/bridge/src/panorama.rs` | 226 | `panorama` | `15` | ✔ **真** —— `15` 里有 `panorama/api.ts`·`views/panorama.ts`·`panorama.code-graph` |
| `src/keybindings/registry.ts` | 167 | `registry` | `16` | ⚠ **疑** —— `16` 讲的是 Rust 侧那 15 份 `*_registry.rs` |
| `src/panorama/types.ts` | 99 | `types` | `15` | ⚠ **疑** —— 命中的是 `disallowed-types` |
| `src/sftp/paths.ts` | 58 | `paths` | `00` | ⚠ **疑** —— `00 §503` 那句「`paths`+`records`+…五块」说的是 `claudecode/paths.rs` |
| `src/cards/slash.ts` | 48 | `slash` | `17` | ⚠ **疑** |
| `src/format.ts` | 39 | `format` | `15` | ⚠ **疑** —— 命中的是 `format!("{}{}", …)` |
| `src/cards/compact.ts` | 27 | `compact` | `20` | ⚠ **疑** |
| `src/paths.ts` | 21 | `paths` | `00` | ⚠ **疑**（同 `sftp/paths.ts`） |
| `src/backend/agents/claudecode/records.rs` | 8 | `records` | `00` | ✔ **真** —— `00 §503` 那句正是说它 |
| `src/backend/agents/codex/resume.rs` | 5 | `resume` | `00` | ✔ **真**（同上） |
| `src/backend/agents/claudecode/resume.rs` | 5 | `resume` | `00` | ✔ **真**（同上） |

⇒ **7 项「疑」合计 459 生产行**，是本轮**最可能藏着待裁项**的地方。
按纪律 2 它们留在丁档（偏保守），但**要人核一眼**。

---

## 4. 🔴 顺手逮到的文档腐 —— `99 §3.2` 那张表整列假了

`99 §3.2`「没有任何设计文档的界面」那张表的**行数一列全对**（逐项复核过），
但**「文档提过吗」一列今天 10 行里 8 行已经假**：

| `99 §3.2` 那一行 | 它写的 | 现打 2026-09-18 |
|---|---|---|
| `src/views/session-viewer.ts` 650 | 「无设计」 | ❌ **`设计/30` T1 点名** `views/session-viewer.ts` |
| `src/remote-launch-run.ts` 626 | 「零提及」 | ✔ **仍然零提及**（乙档，350 生产行） |
| `src/views/grid-monitor.ts` 480 | 「只在普查里」 | ❌ **`设计/17` T1 点名** |
| `src/panorama/layout.ts` 453 | 「零提及」 | ❌ **`设计/17` T1 点名** |
| `src/settings/config-surface-section.ts` 427 | 「零提及」 | ❌ **`设计/10` T1 ＋ `70 §10`** |
| `src/cards/diff.ts` 407 | 「零提及」 | ❌ **`设计/17` T1 点名** |
| `src/settings/data-section.ts` 300 | 「零提及」 | ❌ **`设计/70` 点名**（`§10`，2026-09-18 补） |
| `src/settings/diagnostics-section.ts` 299 | 「零提及」 | ❌ **`设计/70` 点名**（同上，且 `99` 条 69 也点了） |
| `src/tasks-panel.ts` 263 | 「零提及」 | ✔ **仍然零提及**（乙档，185 生产行） |
| `src/keybindings/` 四文件 879 | 「`registry.ts` 提过；`editor.ts` 零提及」 | ⚠ 半对 —— `actions.ts` **`10` T1 点名**；`editor.ts`/`store.ts` 只到**目录级**（T5）；`registry.ts` 只靠 T4b（疑） |

**为什么会整列腐**：那一列描述的是**当下**（「今天有没有设计提过它」），
而 `70 §10`（2026-09-18 补三块）与 `17`（算法与复杂度，新篇）一落地，它立刻就假了。
⇒ 这正是 `doc_claim_registry` 头注那句「**状态列与实测答案是耐久文档里最易腐的两种字段**」
的第 N 次复发。**本量具就是那一列的替代品**：那句话别再抄进文档，让判据去现打。

---

## 5. 明写的缺口 —— 本量具**没有**扫的面

| 没扫什么 | 规模（现打） | 为什么 |
|---|---:|---|
| `src/bridge/crates/*`（8 个共享 crate） | **6 036** 行 | 不在用户给的人群里 |
| `src/bridge/vendor/*`（2 个） | **4 378** 行 | 同上（且是第三方 / vendored） |
| `src/styles.css` | **185 984** 字节 | `.css` 没有 import 图，②③ 对它不成立；CSS 面另有 `css-ledger.vitest.ts` |
| `src/generated/`（81 个 `.ts`） | — | ts-rs 生成物，改它要改生成器 |
| **符号级残留** | — | 模块在用、里面某个 `pub fn` 没人用 ⇒ 本量具的单位是**文件**，答不了 |

### ⚠ 一处点名但不判的残留

`src/bridge/tauri.sidecar.conf.json` —— **还在**。
条 67 删的是 `src/backend/sidecars/` 的 `.rs`，这个 `.json` 是同族残留。
`.json` 出本量具射程（且本轮只读、不许改 `.json`），⇒ **点名，不判，不改**。

### ⚠ 面⑧「判不了」—— ② 那一问在这几族上结论不可靠

1. **动态 `invoke("<名>")`** —— `src/ipc/commands.ts` 头注自陈「Rust 有而 TS 静态看不见的
   那 **7** 个动态名」。本量具只认「命令名逐字出现在 `generate_handler![…]` 里」，
   认不出动态构名。
2. **`querySelector` / 事件委托 / `addEventListener`** —— DOM 钩子不是 import 边。
   ⇒ 「一个 `.ts` 没人 import 就不会被加载」这个**文件级**结论成立；
   但「文件内某个导出有没有人用」本量具**不答**。
3. **计算出来的 `import(...)`** —— 现打 **0 个**（全仓的动态 import 说明符都是字面量）。
4. **Rust 的 trait 分派 / 宏里拼出来的路径** —— 引用边靠 `leaf::` ＋ `use` 段名认，
   认不出宏展开后才出现的路径。

---

## 6. 量具自己踩过的四个坑（写下来，别再踩）

这四个都是**现打逮出来的**，不是设想的。每一个都曾让读数**看起来完全正常**。

| # | 坑 | 后果（现打） | 修法 |
|---|---|---|---|
| 1 | `#[cfg(test)]` 的块界在「只遮注释」的文本上配平 | 测块里的**字符串里有花括号**，配平半路归零 ⇒ `remote_write_registry.rs` 真生产行 0，**误算成 120**，一个纯判据文件进了甲档待裁清单 | 块界改在「连字符串一起抹」的**骨架**上配平 |
| 2 | 「零调用方」按**文件**算 | `sidecars/codepicture/fetch.rs` 报「调用方 1 个」，而那是**同族兄弟** `acquire.rs` ⇒ **整族死代码永远算不出零**，而那正是它的标准形状 | 改问「**从入口走不走得到**」；`mod x;` 不算边，`#[tauri::command]` 注册算边 |
| 3 | 别名 `use` 认不出 | `use …::accounts as cc_accounts;` 之后全程只有 `cc_accounts::` ⇒ `claudecode/accounts.rs` **假红成「走不到」** | 加「`leaf` 出现在某条 `use` 语句里」这一支；⚠ 这一支**必须限定在 `use` 内**，否则 `gate::probe(…)` 这种**同名函数调用**会把 `plugin/probe.rs` 假绿成可达（现打两处） |
| 4 | T4b 的「反引号里」判法写成 `` `[^`]*X[^`]*` `` | Markdown 的**三反引号围栏**让「两个反引号之间」可以是几千字 ⇒ 任何词只要出现在任何围栏里就算被点名 | 限成 `[^`\n]{0,40}`（不跨行、有长度上限） |

> 🔴 坑 2 的教训值得单记：**「有几个人引它」和「从入口走不走得到它」不是同一个问题**，
> 而普查要的是后者。前者对「整族自引的死子树」系统性失效 —— 那恰好是最该抓的一类。
