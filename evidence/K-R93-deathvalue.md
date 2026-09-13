# K-R93 死值验留档 —— agent 适配表：前端那一份改成从后端取值

量于工作树 `.claude/worktrees/k-r93`（分支 `track/k-r93`，基点 `f1f89f5`），
量具与门禁一律在沙箱里跑（`K31`），唯一命令：

```
PB_WS=backend-consolidation .claude/devbox/gate <工作树> k-r93
```

**七刀全部实打**（`KR93D1` 三刀 · `KR93D2` 两刀 · `KR93D3` 两刀），每一刀一趟完整门禁；
「绿」那几刀共用同一趟干净跑（下面 `§1`），「红」那几刀各自一趟（`§2`–`§4`）。
变异脚本住本会话 scratchpad 的 `mut.py`（`apply` / `undo` 对称，undo 之后
`git diff` 为空 —— 交回的树与出绿那一趟**逐字节相同**）。

---

## §0 这条链长什么样（判据认的是**值从哪来**，不是形状）

```text
src-tauri/src/adapter.rs::agent_profile_facts      ← 唯一的值源（后端）
  └─（cargo test --lib export_bindings ＝ npm run gen:types）→
     src/generated/agent-profile-table.ts          ← 生成物，不许手改
       └─→ src/agent-profile.ts
             ├─ AGENT_PROFILE（= 后端 `active()` 那一行）→ 20+ 个既有消费者
             ├─ listAgents() / lookupAgentProfile() → launcher-diagnostics 的 `--agent` 清单
             └─ fullAgentProfile()（`null` 那一格当场抛）
```

**两道门各盖一半，别只报一边**（同 `C05` 的拆法）：

| 谁 | 盖的是 | 在没有 Rust 的那一侧成不成立 |
|---|---|---|
| 门禁第六格 `generated`（`git diff --exit-code -- src/generated/`，跑在 `cargo` 之后） | 「已提交的生成物 == Rust 源」 | ✗（要 cargo） |
| `src/agent-profile-parity.vitest.ts` 新增的那个 `describe` | 「TS 消费方 == 已提交的生成物 == `adapter.rs` 那几张表 / 金表」 | ✓ |

🔴 **在整趟门禁里，`cargo` 先跑、它会把生成物重写一遍** ⇒ vitest 那一侧看到的已经是修好的文件。
**所以「改了后端不重跑生成」这一刀的牙在 `generated`，不在 vitest** —— `§2` 的读数逐字证了这一点。

---

## §1 干净那一趟（刀②/绿的那几刀共用）

`GATE: OK —— 13 格全绿`，逐格读数：

| 格 | 读数 | 预登记 | 对账 |
|---|---|---|---|
| hooks | 11 passed | 11 ±0 | ✔ |
| fmt / fmt-daemon / winchk | 绿（各 1 passed，两态格） | 绿 | ✔ |
| cargo | **1556** passed（8 个包合计） | 1554 起，押 +2~+8 | ✔ **+2**，落在带的下沿 |
| generated | 与 Rust 源一致 | 绿 | ✔ |
| daemon | 699 passed | 699 ±0 | ✔ |
| npm | **1698** passed | 1688 起，押 +4~+12 | ✔ **+10** |
| 四套 ccm e2e | 12 / 8 / 46 / 45 | 12/8/46/45 ±0 | ✔ |
| pb check | FAIL=0 BROKEN=0 | — | ✔ |

**cargo `+2` 逐条**：`adapter.rs::export_bindings_agent_profile_table`（生成器）
＋ `backend/control/agent_profile_parity.rs::the_front_end_read_port_agrees_with_the_golden_table`。
**npm `+10` 逐条**：`agent-profile-parity.vitest.ts` 新增的那个 `describe` 里 10 条
（抽取器自检 · 生成物标记 · 五张表对 `adapter.rs` · 对金表 · `AGENT_PROFILE` 就是那一行 ·
第三刀「不许写死」· 认几个 agent · `KR93D2` · `KR93D3` 问不到 · `KR93D3` null）。
`import-cycle-guard` 那两条断言加在**已有的**那条判据里 ⇒ 条数不变。

**单独给的那一格读数 —— 前端那份认几个 agent**：现打（`f1f89f5`）**1**（`AGENT_PROFILE` 单画像，只有 claude）
⇒ 做完 **2**（`listAgents()` = `["claude", "codex"]`，且与金表 `agent-profile-golden.tsv`
认的 agent 集合逐项相等）。**押中**。

---

## §2 `KR93D1` 刀① —— 后端加一个工具名而前端读不到 ⇒ **必须红**

**变异**：`src-tauri/src/adapter.rs`

```diff
-static CLAUDE_MD_TOOLS: &[&str] = &["Read", "Grep", "WebFetch", "NotebookRead", "TodoWrite"];
+static CLAUDE_MD_TOOLS: &[&str] = &["Read", "Grep", "WebFetch", "NotebookRead", "TodoWrite", "Ls"];
```

**不重跑生成**（生成物一个字节没手改）。

**读数**（`GATE: FAIL`）：

```
  FAIL generated      src/generated/ 与 Rust 源不一致：
 src/generated/agent-profile-table.ts | 2 +-
```

⇒ **红，而且指对了那一格**。

⚠ **同一趟里两条必须如实说明的**：

1. **`fmt` 那一格也红了** —— 那是**变异体自己的排版**，与判据无关：加了 `"Ls"` 之后数组
   内容 61 字符 > rustfmt 的 `array_width`（默认 60）⇒ rustfmt 要把它竖排。
   （**为什么不用 `"Bash"`**：那会让整行到 101 字符 > `max_width` 100，同样多红一格，
   而且更没法归因。选 `"Ls"` 是为了让这一刀的读数尽量只落在 `generated` 上。）
2. **`cargo` 1556 绿、`npm` 1698 绿** —— 这**不是**判据漏了，正是 `§0` 那条顺序：
   `cargo` 那一格跑 `cargo test --lib`（含生成器）时把生成物重写成了带 `"Ls"` 的版本，
   于是 vitest 看到的是「生成物 == Rust 源」。**这一刀的牙从来就在 `generated`。**

## §2b `KR93D1` 刀③ —— 前端退回写死常量 ⇒ **必须红**

**变异**：`src/agent-profile.ts`

```diff
-    mdTools: researched(facts.mdTools, "mdTools"),
+    mdTools: new Set(["Read", "Grep", "WebFetch", "NotebookRead", "TodoWrite"]),
```

**读数**（与 `§4` 的变异同一趟，两条变异**互不相干**：一条在 `fullAgentProfile` 的返回值里，
一条在 `lookupAgentProfile` 的查不到那一支；两条各自红在**不同的**判据上，见下）：

```
 × ★ `KR93D1` 第三刀：`agent-profile.ts` 的生产段里不许出现后端表里的任何一个值
 AssertionError: 这几个值被写死回前端了 —— 那就退回了「前端自己一份常量」，
   `KR93D1` 判的是**值从哪来**，不是有没有 import 那个模块
```

⇒ **红**。⚠ 注意这一刀**写死的值与后端一模一样** —— 只对拍值的判据在这里会**绿**，
本条钉的是「本文件里不许出现那些值」，所以它抓得住。这正是 `KR93D1` 那条失效方向
（「只判前端有没有 import 那个模块」）的反面。

**刀②（接上后 ⇒ 绿）**：见 `§1`。`KR93D1` 那三条（五张表 == `adapter.rs` 源 ·
生成物 == 金表 4 key × 2 agent · `AGENT_PROFILE` == `ACTIVE_AGENT` 那一行）全绿。

---

## §3 `KR93D2` —— codex 那一格不再是漏的

**刀①（两个 agent 拿到同一份 ⇒ 必须红）**，变异：`src-tauri/src/adapter.rs` 里 codex 那一臂
改成拿 claude 的五张表（`AgentKind::Codex => AgentProfileFacts { agent_tools: Some(CLAUDE_AGENT_TOOLS), … }`）。

**读数**（`GATE: FAIL`）：

```
 × ★ `KR93D2`：codex 那一格不再是漏的 —— codex 的画像 ≠ claude 那一份
 AssertionError: 这几格两个 agent 拿到的**是同一个值** —— 要么是真巧合（那就在这里点名说清），
   要么是 codex 那一格又被 claude 那份顶上了
  FAIL generated      src/generated/ 与 Rust 源不一致
```

⇒ **红**。⚠ 同趟 `KR93D3` 的「`null` 那一格」那条也红了 —— 那是**同一刀的连带**
（codex 那五格不再是 `null`），不是第二条独立证据，如实记在这里。

**刀②（分得开 ⇒ 绿）**：`§1` 那趟绿，而且它断的是**逐格**：12 个字段里
「claude 与 codex 取值相同」的集合**为空** —— 12 格格格不同
（`agent` · `adapterId` · `defaultLauncher` · `launcherAlias`（`cc` / `null`）·
`resumeKind`（flag / subcommand）· `resumeToken` · `nestedEnvVars`（4 项 / 空）·
四张工具名表 ＋ 判活进程名（claude 有值 / codex `null`））。

---

## §4 `KR93D3` —— 「问不到」说得出话，不许悄悄退回 claude

**刀①（问不到时静默用 claude 默认 ⇒ 必须红）**，变异：`src/agent-profile.ts`

```diff
-  const facts = table.find((row) => row.agent === agent);
+  const facts = table.find((row) => row.agent === agent) ?? table[0];
```

**读数**：

```
 × ★ `KR93D3`：问不到就说不知道，**不许悄悄回落到 claude**
 AssertionError: 表里没有的 agent 竟然查得到 —— 那多半是回落到别人那一份了:
   expected true to be false
```

⇒ **红**。

**刀②（明说「不知道」⇒ 绿）**：`§1` 那趟绿。今天这一档的形状是
`lookupAgentProfile(x)` 回 `{ known: false, message }`，`message` 逐字带
「问不到 agent「x」的画像：后端那张表里没有它（表里有：claude / codex）。」，
且那一档**结构上带不出画像**（判据另断 `not.toHaveProperty("facts")`）。

⚠ **这一档今天在产品里怎么走到**：`--agent` 下拉的选项本身就来自那张表 ⇒
正常路径上问的都是表里有的。**它是「拿一个表里没有的名字来问」时的答案**，
`fullAgentProfile` 也走同一句话（抛，不给默认画像）。**别把它读成「界面上会看到这句话」。**

⚠ 另一半（`null` ≠ 空）：`fullAgentProfile("codex")` **当场抛**并点名是哪一格
（`agentTools … 没人考据过`），不给一个空 `Set`。那也是「一个值装了两件事」的同族。

---

## §5 一条现打的方法学发现（不属于三条 dod，但值得记账）

**第一趟门禁 `cargo` 四条判据同时红，四条都指对了 —— 而病灶只有一处，在我这一侧。**

生成物的模板原本是**一个**跨行原始字符串字面量，里面有一行 TS 的 `};`
（`AgentProfileRow` 那个类型的收尾）落在 `.rs` 文件的**列 0**。
而 `guard_core::test_module_ranges` 判「测试模块到哪儿收尾」用的正是
**「列 0 的右大括号」**（它刻意不解析字符串/原始字符串，理由写在它自己的头注里）
⇒ **测试模块从那一行被切断**，后面的测试代码整段被当成生产段。四条红：

| 判据 | 它看见了什么 |
|---|---|
| `structural_scan::tests::every_monitor_file_strips_clean` | 剥完仍残留测试属性 |
| `write_site_registry::tests::every_write_site_is_declared_and_installers_name_a_real_tool` | 生成器那句 `fs::write` 成了「未申报的写盘落点」 |
| `agent_dispatch_registry::tests::agent_coupling_sites_are_enumerated_one_by_one` | 测试段里的 agent 名成了生产耦合点 |
| `agent_dispatch_registry::tests::the_coupling_total_only_ever_goes_down` | 同上，合计从 40 涨到 41 |

**处置**：把模板拆成两个字面量（`TABLE_HEADER` / `TABLE_HEADER_TAIL`），
让那一行 `};` 落在 `.rs` 的行中而不是列 0，**并把「为什么不许合回去」逐字写在那两个常量的头注里**
（含这一趟的实测读数）。生成物的字节**一个都没变**。

★ 值得留档的是：**这四条不是假红**。它们各自守的东西都真的失守了 ——
「一个跨行字符串把测试模块切断」这件事，本仓四条判据从四个方向同时叫了。

## §6 第二趟里另一条（同一族：判据认的是**产品里真有的符号**）

第二趟 `cargo` 红在 `structural_scan::tests::every_dead_name_named_in_the_prose_is_declared_dead`：
我在 `adapter.rs` 的注释里逐字点了 `every_monitor_file_strips_clean` ——
**那是一个测试函数名，而那条判据的代码侧语料是「生产段」** ⇒ 它在代码里「不存在」。
（它的登记表 `INVENTORY` 里确实有一行 `("doc/INVARIANTS.md", "every_monitor_file_strips_clean", 1)`
—— 同一个名字、另一个住址。）

**处置**：注释改成点**文件**（`structural_scan.rs` / `write_site_registry.rs` /
`agent_dispatch_registry.rs`）＋ 一句人话，把四个逐字的函数名放到本文件 `§5` 那张表里
（`evidence/` 不在那条判据的语料面里）。**没有删线索，也没有去改那张登记表**
（那是写区外的文件，而且这条注释本来就不该靠登记表活着）。
