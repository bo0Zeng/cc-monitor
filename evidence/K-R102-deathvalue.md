# K-R102 死值验读数原文

> 本件：**「子命令表里有、分派臂没有」这一形，今天没有判据看得见。**
> 交付形态：**甲（立总伞）** —— 理由是下面 `KR102D1` 现打出来的那个数（**裸着 19/26**），
> 远高于件文件里「若 ≤2 则乙很可能才是对的答案」那条线。
>
> 一切读数**量于沙箱**（`K31`：宿主上不跑 `cargo` / `npm`）。
> 门禁命令逐字：`PB_WS=backend-consolidation .claude/devbox/gate <工作树绝对路径> k-r102`，
> **从 `/home/zbl/文档/claudecode-frontend` 起跑**。

## 〇 · 量具住址与被测对象（brief 12：住址要能唯一定位到那一份）

| 量具 | 住址（代码仓，随 `track/k-r102` 提交） | 被测对象指向哪棵树 |
|---|---|---|
| 只读尺子 | `evidence/K-R102-ruler.py` | `Path(__file__).resolve().parents[1]` ⇒ **它自己住的那棵工作树**，不接受参数指树 |
| 变异刀（fail-closed） | `evidence/K-R102-cut.py` | 同上；备份落 `<工作树>/.k-r102-cut-backup/`，`--revert` 从它还原并核 md5 |
| 改动面 | `evidence/K-R102-ruler.py --fn-md5 <rev>` | 同上（逐 `fn` 整块 md5，基点 vs 工作树） |

本轮那棵树：`/home/zbl/文档/claudecode-frontend/.claude/worktrees/k-r102`，
分支 `track/k-r102`，基点 `b0953e5`。

## 一 · `KR102D1` —— 人群与裸着的那几条（本件唯一不可省的产出）

### 1.1 尺子怎么收人（**不按名字**）

件文件点名的失效方向是「按 `fn` 名收人（名里含 `reachable`/`dispatched`）」。本尺子**不这么收**：

- **人群靠遍历**：扫全 crate 每一处 `include_str!("main.rs")` / `("../main.rs")`，
  逐处必须在 `MAIN_SOURCE_READERS` 里表态（是不是某条子命令**分派臂**的伞 ＋ 理由）。
  **表里没有的当场 fail-closed，读数作废。** 这一形抄 `daemon_kill.rs::CREATION_PATHS`
  （逐字：「人群靠遍历发现，不靠手写清单」）。
- **判档靠模拟摘臂**：对每条 token 摘掉它的分派臂（`SUBCOMMANDS` 那张表一个字不动），
  再看**今天真在盘上的**那几条谓词里有谁会红。一条都不红 ⇒ **裸着**。
- 🔴 **谓词人群 = 遍历到的 ∩ 登记为伞的** —— 判据被摘掉，它的谓词当场退出人群。
  第一版写死了一张谓词表，摘掉判据读数一动不动（数的是「臂在不在」，不是「有没有伞看着它」）
  —— **那一版是被下面死值验 ① 当场逮住的**，不是自己想到的。

### 1.2 入场读数（量于 `unharden` 之后 = 把本件实现整段退掉的那棵树）

```
人群（`SUBCOMMANDS` 现打）：26 条
读 `main.rs` 源码文本的地方（遍历）：16 处，逐处表态
其中登记为伞、且真在盘上的谓词：5 条
  arms>=7 · count:--dial · pin:--capture-pane · pin:--oneshot-session · xfile:accounts
CLI 派生面（`REGISTRY` 非 Builtin ＋ PROBE_FLAG）：10 条

已有伞：7/26 —— --account-trust · --account-trust-zero · --capture-pane · --dial ·
                --list-accounts · --oneshot-session · --session-accounts
裸着：19/26 —— --bus-kill · --bus-list · --bus-send · --daemon-probe · --fork-session ·
                --kill · --launch · --list-subagents · --list-projects · --list-sessions ·
                --ping · --read-session · --read-session-from-offset · --read-session-tail ·
                --relay · --resolve · --search · --tmux-notify · --usage
自检全过（剥法 · 反喂饱 · 人群遍历 · 射程 · 阳性回测）
```

⚠ **那个 26 是现打的，不是抄的**：件文件 `§0a` 写的 25（量于 `995724c`），PM 上一拍读到 26。
本树（基点 `b0953e5`）现打 **26**。

**三条路怎么分的**（这是「裸着 19」这个数的分母怎么数出来的）：

| 分派形状 | 几条 | 摘臂那一刀长什么样 |
|---|---|---|
| 字面量臂（`main.rs` 块内 `Some("--x") => …`） | 14 | 删那一行 |
| CLI 派生臂（`Some(f) if cli_control::handles(f)`） | 7 | 删那一行 —— **共享一刀**，走它的 7 条同生共死 |
| `_` 兜底臂（转 `observe::history_query::run`，臂住那份文件自己的 `match` 块） | 5 | 删 history_query 里那一行 |

### 1.3 死值验 ① —— 摘掉一条**已有伞**的判据，分档读数必须跟着变

刀：`unharden,ruler-drop-capture-umbrella`（后者把 `K-R86` 那条
`the_capture_pane_subcommand_is_actually_reachable` **整段**摘掉；锚点 span 起/止各命中 1）。

| | 已有伞 | 裸着 | 尺子退出码 |
|---|---|---|---|
| 入场 | **7**/26 | **19**/26 | 0（自检全过） |
| 入场 ＋ 摘掉那把伞 | **6**/26 | **20**/26 | **1** |

摘掉之后尺子多印两行，**两行都点名**：

```
登记在案、今天不在盘上的判据：… ('main.rs', 'the_capture_pane_subcommand_is_actually_reachable')
🔴 fail-closed —— 读数作废
  · [阳性回测] `--capture-pane` 没被数进「已有伞」—— 尺子没跑或人群画错，读数作废
```

⇒ 分档读数**真的跟着变了**（`--capture-pane` 从「已有伞」掉进「裸着」），
证明它数的是**真伞**，不是「名字里带 reachable」。

### 1.4 死值验 ② —— 阳性回测（纪律 ⑳）

入场那一趟里 `--capture-pane` **被数进「已有伞」**，且**红的正是**
`main.rs::the_capture_pane_subcommand_is_actually_reachable`（不是别条顺手接住的）。
这两格由尺子自己 fail-closed 地断（`[阳性回测]` 那两条），数不到就退出码 1、读数作废。

### 1.5 ⚠ 这个数**买不到**什么（如实登记）

1. **射程只到 `remote-daemon-proto` 这一个 crate 的判据面。** crate 外的伞不在分母里。
   现打过：本 crate **没有** `tests/` 集成目录、**零** `CARGO_BIN_EXE`；
   monitor 侧那两处真调子命令的地方（`backend/observe/local_query.rs::a_short_circuit_cannot_fake_the_honest_degrade`
   与 `subagent.rs::the_candidate_set_comes_from_the_backend_not_from_this_machines_disk`）
   **自己的前提自检就断言「测试环境里没有 sidecar」** ⇒ 够不到 daemon 的分派。
   这两条今天由尺子的 `assert_no_out_of_crate_umbrella()` 各钉一刀，翻了就 fail-closed。
2. **尺子是静态模拟，不是真跑 `cargo`。** 转写对不对由下面第二节那几刀在沙箱里回测。
3. **`is_query_mode` 那道闸门那一半不在本分档里。** `listed_subcommands_still_enter_query_mode`
   会遍历 `SUBCOMMANDS` 断闸门 —— 但摘掉**臂**它不红（表没动）⇒ 它不是「臂」那一侧的伞。

## 二 · `KR102D2` —— 甲：立总伞

### 2.1 立了什么

`remote-daemon-proto/src/main.rs` 的 `mod argv_table_guard` 里加一条
`every_listed_subcommand_has_a_live_dispatch_route`，外加一张 `const DUAL_ROUTE_ARMS`。
另抽出两个块内取法 `dispatch_block` / `arm_tokens`（`dispatch_block` 同时被既有那条
`every_dispatch_arm_actually_calls_an_implementation` 复用 —— 块界锚点从此只有一个住址）。

🔴 **两边的发现机制不同（本条的承重要求）**：

| 侧 | 发现机制 | 它凭什么进不了对面的扫描面 |
|---|---|---|
| 表侧 | `crate::SUBCOMMANDS` 这个**编译后的常量值** | 根本不是文本 —— 没有抽取器可坏 |
| 臂侧 ① | `main.rs` 一次性查询 `match` **块内**的 `Some("--x")` | 那张表住在**块外**；判据自带一条反喂饱断言（块内不许出现表名） |
| 臂侧 ② | 派生臂**在不在**看块内文本；它**认哪几条**由 `cli_control::spec_for` **运行期**从 `inbound::REGISTRY` 派生 | 运行期求值，不看 `main.rs` 一个字 |
| 臂侧 ③ | `observe/history_query.rs` 里**它自己那个 `match` 块** | 另一份文件、另一段块界 |

老那条 `every_listed_subcommand_is_actually_dispatched` 之所以瞎，正是因为它拿
`SUBCOMMANDS` 去比**整份文件**的 `"--` 字面量扫描（`DISPATCH_FILES` 第一项就是 `main.rs`）
—— 表自己就住在被扫的生产段里，两侧在同一次扫描里互相喂饱。那条的头注本轮**认领**了这句话
（`§0a` 逮到的病是「盘上早就说破了，只是没人认领」）。

### 2.2 交回读数（尺子）

```
已有伞：26/26     裸着：0/26     自检全过
```

### 2.3 沙箱变异表（每一刀都是真跑门禁）

见下一节。

## 三 · 变异表逐行真实输出

**门禁那一趟怎么跑的**（可重跑）：`bash` 脚本 = 落刀 → 门禁 → `--revert`，一刀一趟，
`CARGO_TARGET_DIR=<项目>/.claude/pm-targets/k-r102`（本件独占那一个 tag）。
每一趟落刀都先断言锚点命中数（下表「锚点·命中」列 = brief 第 7 条要求的那个数），
对不上**一个字节都不改、整趟放弃**。

| # | 刀 | 锚点·命中 | `GATE:` | `daemon` 那一格 | 红的是谁（逐字） |
|---|---|---|---|---|---|
| `M0` | 无（基点 `b0953e5`，写区未动，`20:06`）| —— | `FAIL —— pb check` | **755 passed** | 只有第 13 格 `FAIL=1`：`[J3 陈账] INDEX.md 比源文件旧` —— **共享计划仓，不是本件** |
| `M0p` | 无（本件实现落地，`20:25`）| —— | `FAIL —— fmt-daemon(1)` | **756 passed（+1 = 本件那条）** | `fmt-daemon`：新判据里一行按 CJK 宽算超了 `max_width=100`（**我自己的**，`M6` 修完重跑）。第 13 格已变 `FAIL=0` |
| `M1` | `arm-relay`（摘掉 `--relay` 的分派臂，`SUBCOMMANDS` 一个字不动）| line ×**1** | `FAIL —— fmt-daemon(1)；daemon(101)` | `755 passed; 1 failed` | 🔴 **只红一条，正是本件那条**：`argv_table_guard::every_listed_subcommand_has_a_live_dispatch_route`（`main.rs:2447`）⇒ **D2 ① 成立，且是最小面** |
| `M2` | `arm-relay,unharden`（阴性对照**第一版**）| line 1 ＋ span 1 | `FAIL —— cargo(101)` | **755 passed，0 failed** | `daemon` 那一面**一条都不红** ✔；但 `cargo`（src-tauri）红一条：`structural_scan::tests::every_dead_name_named_in_the_prose_is_declared_dead` 逐字「`remote-daemon-proto/src/main.rs` `every_listed_subcommand_has_a_live_dispatch_route` 盘上 1 处，登记表写 0 处」⇒ 🔴 **那不是牙，是刀不干净**：`unharden` 只退了判据本体，我加在隔壁头注里的交叉引用还留着 |
| `M2b` | `arm-relay,unharden,unharden-doc`（阴性对照**干净版**）| line 1 ＋ span 1 ＋ span 1 | 🟢 **`OK —— 13 格全绿`** | **755 passed，0 failed** | 🔴 **一条都不红** ⇒ **D2 ③ 成立**：`M1` 那一红是本件买的，不是别处顺手接住的 |
| `M3` | `table-relay`（**反方向**：`SUBCOMMANDS` 里删 `"--relay"`，臂还在）| line ×**1** | `FAIL —— fmt-daemon(1)；daemon(101)` | `754 passed; 2 failed` | ① `argv_table_guard::every_dispatched_token_is_classified`（`main.rs:2267`）逐字「这些 token 被分派了但不在 argv 三分表里：["--relay"]」· ② `build_id_guard::tests::adding_a_subcommand_forces_a_build_id_bump`（`build_id_guard.rs:350`）逐字「daemon 的子命令集变了（+[] / -["--relay"]），而 BUILD_ID 还是 …」⇒ **D2 ② 成立：反方向有话说，而且不是本件这条** —— 与新判据头注登记的射程逐字一致 |
| `M4` | `arm-derived`（摘掉派生臂，CLI 面那 7 条一起失联）| line ×**1** | `FAIL —— fmt-daemon(1)；daemon(101)` | `755 passed; 1 failed` | 🔴 **只红一条，正是本件那条**（`main.rs:2419`）逐字「分派块里没有那条派生臂（`cli_control::handles`）—— 走 CLI 面的子命令一条都调不到了」 |
| `M5` | `arm-history-list-projects`（摘掉 `_` 兜底那一路里 `--list-projects` 的臂，**改的是另一份文件**）| line ×**1** | `FAIL —— fmt-daemon(1)；daemon(101)` | `755 passed; 1 failed` | 🔴 **只红一条，正是本件那条**（`main.rs:2448`）逐字「这些子命令**登记在表里，却没有任何一条分派路够得到**：["--list-projects"]」 |

**每行的分母怎么数的**：`daemon` 那一格 = `cd remote-daemon-proto && cargo test`，
**单包 `remote-daemon-proto`，只有一行 `test result`** ⇒ 那个数就是合计（门禁自己印的分母口径）。
`755 passed; 1 failed` 里的 `755` 与 `M0` 的 `755` **不是同一个集合**：`M0` 是「本件那条还没加」，
`M1`/`M4`/`M5` 是「加了但它红了」⇒ 两边都是 755，是巧合，别读成「没变化」。
`cargo` 那一格 = `cd src-tauri && cargo test --workspace --exclude code-picture-core --lib`（9 个包合计）。
⚠ 本树**未铺** `embedded-daemons/` ⇒ cargo 那个合计里少「本地后端真的能起来吗」那一族 4 条。

**CRASH：0 次。** 六刀都编得过（摘掉的臂只让被调函数变成 `dead_code` 警告，本 crate 没有
`deny(warnings)`），没有一趟是「台子炸了」而不是读数。

### 三之二 · 把实现整个退掉，还有多少条新断言仍绿

本件**只新增一条判据**（`every_listed_subcommand_has_a_live_dispatch_route`，含两格断言：
主断言「表里每条都有活的分派路」＋ 第二格「双路那几条的字面量臂还在」）。
`M2b` 把它连同 `DUAL_ROUTE_ARMS` 与头注交叉引用**整段退掉** ⇒ **仍绿的新断言 0 条**
（退光了，没有「退掉之后还剩几条仪式性的绿」这一档）。
`daemon` 从 `756` 掉回 `755`，差正好是它。

### 三之三 · 改动面（逐 `fn` 整块 md5，基点 `b0953e5` vs 工作树）

量具：`python3 evidence/K-R102-ruler.py --fn-md5 b0953e5`（在本工作树里跑）。
`remote-daemon-proto/src/main.rs` —— 基点 **39** 块 · 工作树 **42** 块：

| `fn` | 基点 | 工作树 |
|---|---|---|
| `arm_tokens` | （基点没有） | 新增 |
| `dispatch_block` | （基点没有） | 新增 |
| `every_listed_subcommand_has_a_live_dispatch_route` | （基点没有） | 新增 |
| `every_dispatch_arm_actually_calls_an_implementation` | `0db4027de74b` | 变了（块界抽取换成调 `dispatch_block`，断言一个字未动） |

**其余 38 块整块 md5 逐个相同。** 别的文件一个字节没动（`git status` 见 `§三之四`）。

⚠ **这个量具的射程，别读宽**（两处它**盖不到**，我另说）：
① 它比的是**函数块**，`const DUAL_ROUTE_ARMS` 是个常量 —— **不在这张表里**，它是新增的；
② 它**不含**函数上方的 `///` 头注 ⇒ `every_listed_subcommand_is_actually_dispatched`
的**头注我改了**（加了一段「它为什么看不见这一形」＋ 指向新判据），而它的块 md5 **没变** ——
这一处只有这句话在报，机器没在报。

### 三之四 · 🔴 `M6-final` 那一趟是**一次静默的假读数**，作废（自己逮住的）

`M6-final`（`20:36:13`）印的是 `GATE: OK —— 13 格全绿` · **`daemon 755 passed`**。
而本件那条判据**在盘上**（`grep -c '#\[test\]' main.rs` = **19**，基点 **18**）——
`755` 恰好等于「判据被删掉」那两趟（`M2` / `M2b`）的数。**两者在终端上一模一样。**

**成因（实打，不是推的）**：刀具的 `--revert` 用了 `shutil.copy2`，它**连 mtime 一起还原** ⇒

```
还原后 main.rs mtime = 2026-09-13 20:34:31   （落刀之前那一刻）
M2b   那一趟的构建   = 2026-09-13 20:34:37   （落刀之后）
```

⇒ `cargo` 判「源码比上次构建还旧、没变」⇒ **直接复用带刀的产物**，
`M6` 跑的是 `M2b` 那个**已经把判据摘掉**的二进制。

**处置**：`--revert` 改成 `shutil.copyfile`（不带元数据）＋ `os.utime(path, None)`，
理由逐字写进 `do_revert` 的注释里（那是它唯一的住址）。`touch` 一下 `main.rs`（md5 不变，
`30d7ce3b1fc75e77d5729cab0d66cfe1` 前后相同）之后重跑 ⇒ `M7`。
🔴 **`M6` 的读数一概不算数**；本节别的行不受影响 —— **每一把刀那一趟都是先 `write_text` 落刀
（mtime 当场刷新）再跑门禁**，只有「revert 之后不落刀直接跑」这一形会踩到。

### 三之五 · 交回读数 `M7`（写区落定 · 无刀 · `20:39`–`20:41`）

```
GATE: OK —— 13 格全绿（hooks · fmt · fmt-daemon · winchk · cargo · generated · daemon ·
             npm · 四套 ccm e2e · pb check），可以出货
  hooks       11 passed
  fmt          1 passed          fmt-daemon   1 passed        winchk  1 passed
  cargo     1601 passed（9 个包合计；本树未铺 embedded-daemons ⇒ 少那一族 4 条）
  generated   与 Rust 源一致
  daemon     756 passed（M0 的 755 ＋ 本件那条 = 756）
  npm       1722 passed
  ccm e2e   ccm-print-parity 12 · ccm-rbind-title 8 · ccm-cli 46 · ccm-contract-parity 45
  pb check  [backend-consolidation] FAIL=0 BROKEN=0
```

逐格与 `M0` 的差：**只有 `daemon` 那一格 `755 → 756`**，差正好是本件新增的那一条判据；
第 13 格从 `FAIL=1`（`[J3 陈账] INDEX.md 比源文件旧，共享计划仓）变成 `FAIL=0`，
**不是本件做的**（并跑的人在计划仓那侧动过）。其余十一格逐格恒等。
