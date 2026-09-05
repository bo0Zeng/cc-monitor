# K-W1B · D1 · 「agent 解耦」的人群，一次切清

量具（唯一住址）：`evidence/K-W1B-D1-agent-coupling-census.py`（本件独占的名字；被测对象 =
它自己所在那棵工作树，不写死路径）。
本文件的每个数都由那份量具**现打**，量于 **`1260ad8`**（`git status` 空）。

## 0 · 量具自己的标定 —— 不标定的尺子不许报数

```
CALIBRATION: OK —— 逐文件逐字相等（8 文件 / 27 处）
```

尺子① **不是**本量具的产物：它是 daemon 侧
`agent_locality_guard::general_layer_adapter_call_sites_are_enumerated_one_by_one`
在数的东西，而那条判据**逐文件相等**、每趟 cargo 都在跑。⇒ 量具把
`ADAPTER_CALL_SITES` 那张登记表从 Rust 源码里**现读**出来（不复述、不留副本），
与自己量的逐文件读数对拍。对不上就 `CALIBRATION: FAIL`、整份读数作废。

★ 那就是这份 `production_code` Python 移植的地板：它剥得对不对，不靠我说，
靠一张被 cargo 钉了十天的表。
⚠ **诚实边界**：标定只覆盖尺子①走的那条路（daemon 树 · 6 根路径针 · 行粒度）。
尺子②③④⑤ 用同一个剥法，但**它们各自的针没有第二个权威源** —— 那几个数只有本量具一个来源。

## 1 · 六把尺子，每把五样

| # | 量哪棵树 | 分母（人群怎么切的） | 粒度 | 针（逐字） | 量于 | 读数 |
|---|---|---|---|---|---|---|
| ① | `remote-daemon-proto/src/**/*.rs` | 该树 `.rs` **73**；扣三个家 **11** ＋ `scan_tree!` 摘掉调用者自己 **1** ⇒ **61** 进针扫；判据④再扣注册表文件 `agents/mod.rs` **1** ⇒ **60** | 行（一行两次算一次） | `agents::codex::` `codex::` `agents::claudecode::` `claudecode::` `agents::fake::` `fake::`（由 `HOMES` 派生） | `1260ad8` | **27** |
| ② | `src-tauri/src/**/*.rs` | 该树 `.rs` **106**，一份不扣 | 行 | `::active(` | `1260ad8` | **6** |
| ②b | `src-tauri/src/adapter.rs`（**单文件**） | 人群 1；扣 `fn active()` 定义行与 `::active(` 那半 | 行 | 裸 `active()` | `1260ad8` | **6** |
| ③ | `src-tauri/src/**/*.rs` | **106**；扣 `parse_for_kind(` · `fn for_kind(` 定义行 · 实参是 `AgentKind::` 字面量的那些 | 行 | `for_kind(` 且实参不以 `AgentKind::` 打头 | `1260ad8` | **4** |
| ④ | `src-tauri/src/**/*.rs` | **106**，一份不扣，**原文一个字不剥** | 行 | `for_kind(` | `1260ad8` | **37**（扣已登记噪音 14 ⇒ **23**） |

⚠ **②③④ 那个 106 里有一份是本件自己新增的判据文件**（`agent_dispatch_registry.rs`）。
落地之前该树是 **105** —— 件计划 `§0c` 那几个读数量于 `4eb271f`，分母是 105。
分母变了 ⇒ 引用旧读数时要连分母一起引（本表已重打）。
| ⑤ | `src-tauri/crates/**/*.rs` | 该树 `.rs` **9**，一份不扣 | 行 | `claude` `CLAUDE` `Claude` `codex` `CODEX` `Codex` | `1260ad8` | **7**（扣已登记噪音 1 ⇒ **6**） |

**它们为什么互不相等 —— 逐对说清**

- **① vs ②③④**：量的是**两棵不同的树**。① 只在 daemon 树上跑，桌面侧一根针都没伸进去（件计划
  `§0b` 的第一个实例）。而两侧机制不同：daemon **故意还没有 trait**、直呼 `agents::<名>::`；
  桌面**有** trait ＋ `for_kind` 派发 ⇒ ① 那 6 根针在桌面树上恒零命中。**两个数不许相加。**
- **② vs ②b**：同一件事的两种**调用语法**。②b 是 `adapter.rs` 内部裸调（同模块，写不出 `::`），
  ② 是模块外的路径限定调。分开报是因为件计划 `§0c` 要求「两个数都上表」；
  🔴 **答 PM 那一问：算进人群**（论据见 §3）。
- **② vs ③**：② 数「**说不出**指哪个 agent」，③ 数「**传了运行时 kind**」——
  后者是接口**正在被正确使用**，方向相反。混成一个数，棘轮就会奖励「把真分派改回写死」。
- **③ vs ④**：同一个词 `for_kind`，一个剥生产段 + 认实参形状，一个裸子串。
  ④ 的 37 行里，`parse_for_kind`（parser 的另一个派发器）**9** · 注释行 **7** ·
  定义行 **1** · 剩下 **20**。⇒ 它答非所问，留着它**就是为了展示这把尺子为什么不能用**。
- **④ 的第三条不能用的理由（现打）**：本件的 D2 判据一落地，④ 从 **23 涨到 37**，
  而那 14 行全部来自 `agent_dispatch_registry.rs` 自己的头注、登记表与反向夹具 ——
  **量具数到了为量它而写的那份判据**。噪音已逐条登记（`RULER4_NOISE`），扣掉后 **23**，
  与落地前逐字相同 ⇒ 本件一行代码都没动 ④ 的真人群。
  ★ 对照：D2 那条判据**免疫**同一个病 —— 它走 `guard_core::scan_tree!`（按 `file!()`
  构造性摘除自己）＋ `production_code`。差别不是小心，是**用了本仓为这一族立的那两个原语**。
- **⑤ vs 全部**：⑤ 量的是**第三棵树**（`src-tauri/crates/`），而它同时在 ① 与 ②③④ 的量程之外
  —— 见 §2。

## 2 · 🔴 `KP2` 那一问：27 之外还有没有扫不到的 agent 知识

件计划 `§0b` 给了第一个实例（**整个桌面侧不在尺子①的量程里**）。要找的是**第二个**。

### 第二个实例：`usage_core::codex_delta` —— Codex 用量口径的**唯一权威源**住在两棵树之外

住址：`src-tauri/crates/usage-core/src/lib.rs`。那个函数的 docstring 逐字自称
「★ **Codex 用量口径的唯一权威源**：字段名 + 「input 不含 cached」这个减法」。

它为什么**每一把尺子都够不着**（三条，逐条给住址）：

1. **不在尺子① 的树里** —— ① 扫 `remote-daemon-proto/src`，`src-tauri/crates/` 是另一棵；
2. **不在尺子②③④ 的树里** —— 它们扫 `src-tauri/src`，而 `crates/` 是它的**同级**目录；
3. **针也够不着** —— ②③④ 数的是 `active()` / `for_kind(`，那是 monitor 适配层的 API；
   本量具实测：这两根针在整棵共享 crate 树上命中 **0**。

而它**同时被两侧依赖**：`src-tauri/Cargo.toml:64` 与 `remote-daemon-proto/Cargo.toml:43`
各有一行 `usage-core = { path = … }`。daemon 侧的消费点是
`remote-daemon-proto/src/agents/codex/parse.rs:110`（`usage_core::codex_delta(usage)`）——
**而那个文件被判据① 按 `HOMES` 构造性扣出人群**。

🔴 **这就是它比第一个实例更坏的地方，也是它答得上 `KP2` 的地方：**
第一个实例是「一棵树没人扫」（指一把尺子过去就补上了 —— 那正是 D2 做的事）；
这一个是**知识被搬到了一个「合法」的地方，而那个地方在所有人群之外**。
`agents/codex/parse.rs:105` 的 docstring 逐字写着「字段名与减法**不在这里** ——
它们住 `usage_core::codex_delta`（唯一权威源）」⇒ **代码自己记录了这次搬迁**，
而搬完之后没有任何一条判据数得到它。

⚠ **必要条件写在一起**（brief 16j）：这条声称成立的前提是「尺子的人群按**树**切」。
只要有人把某把尺子的树扩到 `src-tauri/crates/`，这一处就进视野了 —— 本条不是「不可能被数到」，
是「**今天没有任何一把尺子的树包含它**」。

### 顺带量出的第三个实例（不重复计数，只登记住址）

`shared/ccm`（**shell**）里 `agent_has_identity` 与 `agent_needs_bus_id` 两个决策
**在 Rust 侧根本没有对侧** —— 不是「没被扫到」，是**没有地方可被扫到**。
这一格已经有人登记了，但**登记在另一个问题下**：
`src-tauri/src/backend/control/agent_profile_parity.rs` 的 `THE_TWO_CCM_ONLY_DECISIONS`
把它们记成「F06 的真实阻塞」，不是「agent 耦合」。⇒ 对「加一个 agent 要改几处」这一问，
它今天**不在任何一个计数里**。★ 给 `K-W3` 的含义：合并时那份清单里有两条住在 shell 里。

### ⚠ 尺子⑤ 自己的欠算（写出来，别当它是全集）

⑤ 的针是 agent 的**名字**。而 Codex 的 **wire 字段名**（`cached_input_tokens` /
`input_tokens` / `output_tokens`）一个字里都没有 `codex` ⇒ **它们是 agent 知识，而 ⑤ 数不到**。
同理 `acct-core` 的 `CREDENTIALS_NAME = ".credentials.json"`（Claude 的凭据文件名）也逃过了针。
⇒ ⑤ 的 6 是**下界**，不是全集。

## 3 · PM 那两格必答

### 3a · 针怎么不重蹈 `.active(`

三道处置，全部由 D2 的反向夹具正反各钉一格（实测见 §4）：
① 针不带前导点（三种调用写法一网打尽）；② 带闭括号（`watcher.rs` 另有同名的会话活性
函数 `active(&session_id)`，裸 `active(` 会把它数成耦合）；③ 匹配单位带边界
（走 `guard_core::contains_word`）—— **现打的活体**：不带边界时 `map.snapshot_active()`
被数成一处耦合，读数 12 虚高成 **13**。

### 3b · `adapter.rs` 自己身上那几处算不算人群 —— **算，一张表按文件分行**

⚠ 先订正一个数：件计划 `§0c①` 写「另有 **7** 处裸 `active()` 在 `adapter.rs` 自己身上」，
并列了 6 个函数名。现打 **6 个调用点**（`:93` `:141` `:146` `:151` `:156` `:169`），
第 7 行是 `pub fn active()` **定义行本身**。⇒ 带定义行 7、只算调用点 6，**两个数都在表上**。

**算进人群的四条论据：**

1. 两者是**同一个缺陷**：调用点说不出它指哪个 agent。`records_dir(root)` 与
   `crate::adapter::active()` 都是把 kind 抹掉，只是一个抹在门面里、一个抹在调用点。
2. 件计划 `§0c①` 逐字说 `adapter.rs` 那几处「**性质更重**」—— 更重的东西不该被扣出人群。
3. 拆成两张表会**立刻开出** daemon 侧 `AGENT_REGISTRY_SITES` 头注逐字警告的那条捷径：
   把通用层的一处挪进 `adapter.rs`，主表的数就掉下去。daemon 侧为此付了判据⑦当对价；
   而**一张表按文件分行不需要那个对价** —— 挪一处，两边同时红。
4. 「它是适配层文件」这条切法在桌面侧站不住：`adapter.rs` 里那 6 处**不是适配层的实现**
   （实现住 `adapter/claude_code.rs` 与 `adapter/codex.rs`），是**给通用层用的门面**，
   每一个都有一个带 kind 的兄弟（`records_dir_for` / `session_id_from_path_with`），
   退役方式与通用层那几处一样是「把 kind 穿进来」。

## 4 · 刀 D1（死值验）：四把尺子不是四遍同一把

**锚点**：`src-tauri/src/history.rs:1595` 那一行，逐字
`let agent = crate::adapter::active().id();`
**锚点命中数**：在 `history.rs` 里 **1** 处；在整棵 `src-tauri/src` 里 **1** 处（切之前断言过）。
**变异**：换成 `let agent = "claude-code";`（形状对、类型契约不动：两边都是 `&'static str`，
`agent` 仍被下面那次调用消费 ⇒ 不是 CRASH，是「恒答其中一张脸」）。
**口径**：只跑本量具（纯读源码的 Python，**不编译、不跑测试**）；跑完 `git checkout --` 复位。

| 尺子 | 切之前 | 切之后 | 动了吗 |
|---|---|---|---|
| ① daemon 27 | 27 | **27** | 没动 ✓ |
| ② `::active(` 生产 | 6 | **5** | **6→5** ✓（就是它，正是件计划预言的那一格） |
| ②b `adapter.rs` 裸 `active()` | 6 | **6** | 没动 ✓ |
| ③ `for_kind` 真分派 | 4 | **4** | 没动 ✓ |
| ⑤ 共享 crate 树 | 7 | **7** | 没动 ✓ |
| ④ `for_kind(` 裸子串 | 37 | **37** | 没动 ✓ |

⇒ **同一处代码，只有一把尺子看得见它。** 这同时复现了件计划 `§0c③` 那次真涨
（08-28 → 09-04 桌面写面长了一处，而当时没有任何东西红过）。

**复位读数**：`git status --porcelain` 空 · `git diff --quiet` 退出码 **0**
（非空对照：变异在盘上时 `git diff --stat` 报 `1 file changed, 1 insertion(+), 1 deletion(-)`）。
