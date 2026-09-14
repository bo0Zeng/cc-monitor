# `K-R116` 死值验与读数 —— 全仓措辞：不再说「daemon」，全部叫「后端」

> 量于 **2026-09-14**，工作树 `.claude/worktrees/k-r116`（分支 `track/k-r116`，基点主干 `897afec`）。
> 门禁一律走**唯一许可命令**：`PB_WS=backend-consolidation .claude/devbox/gate <工作树> k-r116`
> （沙箱 `ccmon-devbox:latest`）。宿主上一次 `cargo` / `npm` 都没跑。
> 刀住 `evidence/K-R116-cut.py`（被测树 = 它自己所在的那棵工作树，每趟印在第一行）；
> 尺子住 `evidence/K-R116-ruler.py`；人群普查在 `evidence/K-R116-census.md`。
> ⚠ 每个读数带「量于哪一刻 · 用什么量的」；带轮次 / 分支尖的句子下一轮自动变成假话。

## §A 门禁逐格读数（`M0` 基线 → `M2` 终态）

⚠ **`M0` 与 `M2` 的分母不是同一个**：`M0` 量于 `98a8d51`（**15 格**），
`K-R118` 09-14 并进主干之后门禁多了 `tsc` 一格 ⇒ `M2` 量于 `897afec`（**16 格**）。
`tsc` 那一行 `M0` 栏写「不存在这一格」，不是「没跑」。

| 格 | `M0` 基线（`98a8d51`，动手前） | `M2` 终态（`897afec` ＋ 本件） | 分母怎么数的 |
|---|---|---|---|
| `hooks` | 11 passed | 11 passed | 每个被跟踪的 hook 文件 3 条 ＋ 8 条阳性对照；现打 `hooks/` 下 1 份 ⇒ 11 |
| `copy2` | 11 passed | 11 passed | `evidence/*.py` 里 `shutil` 保元数据复制族的**调用点** 11 处 —— 本件新加两份 `.py` 都不用 `shutil`，人群没涨 |
| `fmt` | 1 passed | 1 passed | 绿/红两态，分母 = `src-tauri` 那个 workspace 全体成员 |
| `fmt-daemon` | 1 passed | 1 passed | 绿/红两态，分母 = `remote-daemon-proto` 唯一成员 |
| `winchk` | 1 passed | 1 passed | 绿/红两态，`-p monitor` 一个包 |
| `cargo` | **1611** passed（9 个包合计） | **1613** passed（9 个包合计） | ＋2 = 本件新立的两道闸（`daemon_wording_registry` ＋ `frozen_daemon_census`） |
| `deadcode` | 41 passed | 41 passed | `cargo check -p monitor` 非 test 构建里 `never used` 的条数，**恒等钉 41**；本件全部落在 `cfg(test)` |
| `generated` | ok | ok | `git diff --quiet -- src/generated/` |
| `daemon` | 762 passed | 762 passed | 单包 `remote-daemon-proto`；本件只改了它 4 处**字面量**，没加判据 |
| `tsc` | **不存在这一格** | **366 passed** | `K-R118` 09-14 加的第 16 格；本件一个 `.ts` 都没动 |
| `npm` | 1726 passed | 1726 passed | 取最大值 ⇒ 恒是 `test:dom` 那个数 |
| `ccm e2e/ccm-print-parity` | PASS=12（地板 12，恒等） | 同 | — |
| `ccm e2e/ccm-rbind-title` | PASS=8（地板 8，恒等） | 同 | — |
| `ccm e2e/ccm-cli` | PASS=46（地板 46，恒等） | 同 | — |
| `ccm e2e/ccm-contract-parity` | PASS=45（地板 45，恒等） | 同 | — |
| `pb check` | `FAIL=0 BROKEN=0` | `FAIL=0 BROKEN=0` | 共享计划仓 `backend-consolidation` |
| **裁决** | `GATE: OK —— 15 格全绿` | `GATE: OK —— 16 格全绿` | — |

⚠ **两趟之间还有一趟 `M1` 是红的，如实记**：`M1`（还没 ff 到 `897afec` 那一版）
`GATE: FAIL —— fmt（1）；cargo（101）`，红的两条都**不是本件的正题**，而且都值钱：

1. 🔴 `needle_anchor_registry::bare_contains_on_disk_corpora_only_goes_down` —— **34 > 棘轮 33**。
   根因**不是**我写了一处裸 `contains("…")`：我在新判据里写了
   `let lower = text.to_ascii_lowercase(); let b = lower.as_bytes();`，
   而 `text` / `b` 这两个短名**在同一份文件里早就被别的判据当语料变量用着** ——
   那条棘轮的传递闭包**按名字**跑（不看类型）⇒ 它把同文件里
   `name.starts_with("README")` / `c.contains("::")` 那一族**早已存在**的匹配一起卷进了人群。
   ⇒ 处置是**把局部变量起成长名**（`folded_haystack` / `folded_bytes` / `site_text` / `frozen_text`），
   并把 `read_to_string` 包成 `read_frozen()` 不让它出现在 `let` 右边。改完复打回 33。
   **这一条写进了那两处的注释里**，因为下一个人极容易再踩。
2. 🔴 `tool_registry::every_place_that_still_says_the_old_name_is_registered_and_only_shrinks` ——
   我在新判据的**自检夹具串**里写了「远端 ＋ daemon」连写的那一形，
   而那张表是**旧名字的存量账**（`Why::Wording` 逐格按等号认）⇒ 一处夹具被读成一笔真债。
   ⇒ 处置是把夹具串换成「常驻 daemon 的 stdin」，账一行没动。

⇒ **本件因此额外交出一条盘面事实**：本仓已经有一张「旧名字还留在哪儿」的账
（`src-tauri/src/tool_registry.rs::SITES`，射程 = `src-tauri/src` ＋ `src-tauri/crates` 的 `.rs`），
它与本件新立的两道闸**不重叠**（那张账只看 `.rs`，本件只看 `.md`）。

## §B `KR116D1` —— 先量后改：两个数分开

### B1 两个数

| | 数 | 量于 · 用什么量的 |
|---|---|---|
| **出现次数** | **2111** | `897afec`，`evidence/K-R116-ruler.py`，人群 `git ls-files '*.md'` 现打 96 份 |
| 🔴 **该改几处** | **358** | 同上，落在「该改」那一档的 |
| 差 | **1753** | 冻结 1478 ＋ 写区外 121 ＋ 代码标识符 139 ＋ 登记例外 15 |

派工单那个 **1828** 是 `K-R107` 09-13 量的；复量得 2111（差在 `K-R108`…`K-R118` 十一件
各自往 `evidence/` 落的留档）。**两个数都不是工作量。** 逐档与逐份见 `evidence/K-R116-census.md`。

### B2 阳性回测（`KR116D1` ② · 纪律 ⑳）

`python3 evidence/K-R116-cut.py --backtest`（语料 = `git show 897afec:doc/IPC-PROTOCOL.md`，77187 字节）：

```
· 出现次数 ：159
     冻结·不许改        0
     写区外·未改        0
     标识符·不改        31
     登记例外·不改       3
     该改             125
阳性回测：OK —— 159/159 全部落在**本轮写区**这一档里（0 冻结 · 0 写区外）
```

🔴 **两个口径要分开说，别混**：
- **按文件级**（`§0a` 那张表的档）：那 159 处**全部**落进「该改」那一档 —— 尺子真的跑到了这份文件上。
- **按逐处**：159 = **125 该改** ＋ 31 代码标识符 ＋ 3 登记例外。
  「159 处都该改」按逐处**不成立**，而这正是 `KR116D1` 自己要的三分档
  （dod 逐字：「**该改的几处 · 不许改的几处 · 代码标识符那一档几处**」）。

### B3 三刀

| 刀 | 切在哪 · 锚点命中 | 门禁读数 | 是不是那一格 | 最小面？ |
|---|---|---|---|---|
| `d1-1` 把**判据**的 token 规则里 `-` 那一档掐掉 | `doc_claim_registry::daemon_wording_registry::token_at`，锚点**命中 1 次** | `GATE: FAIL —— fmt（1）；cargo（101）`；cargo 里红**恰好一条**：`daemon_wording_registry::no_prose_in_the_wording_sites_still_says_daemon`，报文逐字「切 token 判错了：`"remote-daemon-proto"` 切出 `"daemon"`，期望「裸词=false」」。`copy2 11` · `deadcode 41` · `daemon 762` · `tsc 366` 照旧 | ✅ 正是那一条 | ⚠ **两格**：`fmt` 那一格是刀自己留下的排版伤（删掉 `b'-' \| ` 之后那一行 rustfmt 要重排），不是判据 |
| `d1-1b` 同一刀切在**量具**那一侧（`K-R116-ruler.py::token_at`） | 锚点**命中 1 次**；本刀**不跑门禁**（纯静态 python），跑量具本身 | 分档读数当场变：`标识符·不改` **139 → 79**、`该改` **0 → 60** | ✅ 「尺子分得开两者」有了一个数 | ✅ 只动量具 |
| `d1-2` 阴性对照：`d1-1` ＋ 把那条判据整条拿掉（4467 字节） | 同上 ＋ `fn no_prose_in_the_wording_sites_still_says_daemon` 整块 | `GATE: FAIL —— fmt（1）`。🔴 **`cargo` 回绿：1613 → 1612** | ✅ 本笔要证的那一半成立 | ⚠ **不是「一条都不红」** —— `fmt` 仍红（同 `d1-1`，外加拿掉整块之后补回的那个花括号） |
| `d1-3` 假红方向：往 `doc/ARCHITECTURE.md` 追加一段**正当**的代码标识符提及（`remote-daemon-proto/src/relay/mod.rs` · `daemon_launch.rs` · `--daemon-probe`，180 字节） | 追加式，无文本锚点 | `GATE: OK —— 16 格全绿`，`cargo 1613` 一条没红 | ✅ **必须不红，实测不红** | — |

⚠ **`d1-2` 头一趟是 CRASH，不是读数，如实记**：第一版 `cut_block` 把 `fn` 的收尾花括号
与 `mod` 的收尾花括号**一起**切掉了 ⇒ 四格齐红（`fmt` / `winchk` / `cargo` / `deadcode`），
那时红的是**编译**，不是「判据不在了」。修法写进了 `K-R116-cut.py::cut_block` 的 `keep` 参数与它的头注。

## §C `KR116D2` —— 冻结面那道闸

处置：`src-tauri/src/doc_claim_registry.rs::frozen_daemon_census`，
**逐文件地板**（`REGISTERED` 70 行：69 份 `evidence/*.md` ＋ `CHANGELOG.md`，量于 `897afec`）
＋ **整棵 `evidence/` 的合计地板**（`daemon` 1388 · `ccm` 442）。
理由（`KR116D2` 逐字要它进头注）写在那个模块的头注里：那两档装的是「某年某月现打是多少」，
**改错了改不回来**（`brief` 第 12 条）。

🔴 **为什么是地板不是等号**：`CHANGELOG.md` 每次发版都会长，等号会天天假红；
而**批量替换只会让数变小**，地板正好卡在那个方向上。

| 刀 | 切在哪 · 锚点命中 | 门禁读数 | 是不是那一格 | 最小面？ |
|---|---|---|---|---|
| `d2-1` **批量替换**：`evidence/K-R113-deathvalue.md` 里 43 处 `daemon` 一次换光 | 锚点**命中 43 次**（落刀前断言过） | `GATE: FAIL —— cargo（101）`，红**恰好一条**：`frozen_daemon_census::the_frozen_history_never_loses_a_daemon`，报文点名 `evidence/K-R113-deathvalue.md：daemon 0 < 登记 43` | ✅ | ✅ 只红一格一条 |
| `d2-1s` 最小面：**同一份里只换一处**（43 → 42） | 按字节偏移换 1 处，其余 42 处不动 | `GATE: FAIL —— cargo（101）`，红**恰好一条**，报文**两层都点名**：`evidence/K-R113-deathvalue.md：daemon 42 < 登记 43` ＋ `evidence/ 整棵树：daemon 合计 1387 < 地板 1388` | ✅ | ✅ **一处就够红** |
| `d2-2` 阴性对照：`d2-1` ＋ 把那条判据整条拿掉（2344 字节） | 同上 ＋ `fn the_frozen_history_never_loses_a_daemon` 整块 | `GATE: FAIL —— fmt（1）`。🔴 **`cargo` 绿：1612 passed** | ✅ 本笔要证的那一半成立 | ⚠ 同 `d1-2`：`fmt` 仍红（补回的那个花括号没按 rustfmt 排），**不是「一条都不红」** |

## §D `KR116D3` —— 引用文档句子的判据要跟着走

### D1 🔴 人群：PM 给的是 **7 份 `include_str!` ＋ 2 份遍历**，本轮现打是 **14 份**

派工单逐字「**PM 现打给了人群，别自己重推**」。本轮照着核了一遍，**核出来人群更大** ——
PM 那张表按 `include_str!` 取样，而读这 9 份文档的路**不止 `include_str!` 一条**。
现打（量于 `897afec`，人群 = 非 `evidence/` 的 `.rs`/`.ts`，判法 = 文件里有一处**字符串字面量**
逐字等于那 9 份文档之一的住址，注释行不算）：

| 判据文件 | 怎么够到文档 | 在 PM 那张表里吗 | 本轮动了吗 |
|---|---|---|---|
| `src-tauri/src/sftp.rs` | `include_str!` IPC | ✅ | 没动（它的锚点是 `` shared/ccm-aliases.sh`，** ``，不含 daemon） |
| `src-tauri/src/doc_claim_registry.rs` | `include_str!` INVARIANTS ＋ IPC · 遍历 `doc/` ＋ `README*` | ✅ | 🔴 **动了**（本件的两道新闸就住在这里，纯追加 20263 字节） |
| `src-tauri/src/sftp_move_ledger.rs` | `include_str!` INVARIANTS | ✅ | 🔴 **动了 1 处**（校验位，见 D2） |
| `src-tauri/src/profile_installer.rs` | `include_str!` IPC ＋ ARCHITECTURE | ✅ | 没动（锚点是 `{deadline}ms` 这类数字与握手顺序） |
| `src-tauri/src/launch.rs` | `scan_tree!` 遍历 `doc/` ＋ 两份 README | ✅ | 没动（needle 是「会话容器本来就是 tmux」，不含 daemon） |
| `remote-daemon-proto/src/sidecar_fetch_guard.rs` | `include_str!` IPC | ✅ | 没动（锚点是 `` `sidecar_ `` 开头的 code） |
| `remote-daemon-proto/src/protocol_doc_guard.rs` | `include_str!` IPC | ✅ | 🔴 **动了 4 处**（§10 的章标题，见 D2） |
| `remote-daemon-proto/src/observe/accounts_query.rs` | `include_str!` IPC | ✅ | 没动（锚点是 `- \`--session-accounts ` 行首） |
| 🔴 `src-tauri/src/doc_copy_registry.rs` | `PROSE_FILES` 常量 ＋ `read_to_string` | ❌ **不在 PM 那张表里** | 🔴 **动了**（写区外随动，见 D3） |
| 🔴 `src-tauri/src/arch_doc_shape_guard.rs` | `read_to_string("doc/ARCHITECTURE.md")` | ❌ | 没动（探针是 `backend = 读` / `零轮询` / `ssh_source.rs` 这一族，一条都不含 daemon；改完复打：文件名提及 63 ≤ 70 · 表格行 12 ∈ [6,25] · `Arc<` 行 0） |
| 🔴 `src-tauri/src/atomic_replace_registry.rs` | `read_to_string("doc/INVARIANTS.md")` | ❌ | 没动（锚点是 `## 4`） |
| 🔴 `src-tauri/src/backend/control/daemon_kill.rs` | 遍历整棵 `doc/` | ❌ | 没动（needle 运行期拼「过渡期回落」，不含 daemon） |
| 🔴 `remote-daemon-proto/src/control/launch.rs` | `read_to_string("doc/IPC-PROTOCOL.md")` | ❌ | 没动（三个 needle：「只有 `send-keys` 的退出码那么强」`copy-mode`、判据名） |
| 🔴 `src-tauri/src/structural_scan.rs` | `INVENTORY` 逐条 `(文档, 符号名, 处数)` | ❌ | 没动（钉的是**符号名**，本件一个符号名都没改；改前改后逐格复打相同） |

⇒ **`K-R106` 那次「从一份 `include_str!` 扩到整棵 `doc/`」的教训，这一轮的形状是「从
`include_str!` 这一种取法扩到**所有取法**」。** 本轮的处置是：动任何一句之前，
跑一把**机械的对拍**——把非 `evidence/` 的 `.rs`/`.ts` 里每一个**字符串字面量**拿去与
那 9 份文档改前 / 改后各比一次，**改前在、改后不在**的逐条列出来。现打命中 15 条，
逐条定性之后只有 3 条是真耦合（下面 D2 / D3），其余 12 条是**巧合子串**
（`accounts.rs` 的 `"旧 daemon"`、`local_accounts.rs` 的 `"daemon 缺"` 这类界面文案，
它们碰巧也出现在文档里，而没有任何判据拿它去对文档）。

### D2 因为这次改动而**真的跟着动**的判据：**2 份 5 处**

| 判据 | 它引的那句话 | 本轮改成 |
|---|---|---|
| `src-tauri/src/sftp_move_ledger.rs::REGISTERED` 里「daemon 只读铁律 I7」那一行的**逐字校验位**（`:179`，1 处） | `doc/INVARIANTS.md §41.6` 的**现措辞**「daemon 不许改动用户既有数据」 | 校验位改成「后端不许改动用户既有数据」，与文档同拍 |
| `remote-daemon-proto/src/protocol_doc_guard.rs`（`:1016` `:1215` `:1259` `:1315`，4 处） | `doc/IPC-PROTOCOL.md` 的**章标题**「## 10. 远端 daemon wire 协议」 | 标题与 4 处锚点同拍改成「## 10. 远端后端 wire 协议」 |

⚠ **`sftp_move_ledger` 那条判据的报文逐字警告过**「别顺手把校验位改成新的原文 ——
那等于把『有人动过』这件事抹掉」。本轮**是有意改的**，所以在这里点名；
那一行的**标签**（`"daemon 只读铁律 I7"`）**刻意没改** —— 它是表里的键，不是散文。

### D3 死值验（一刀 ＋ 两刀补充）

| 刀 | 切在哪 · 锚点命中 | 门禁读数 | 是不是那一格 | 最小面？ |
|---|---|---|---|---|
| `d3-1` 把 `doc/INVARIANTS.md` 那句改回旧措辞 | `**现措辞**：**后端不许改动用户既有数据`，锚点**命中 1 次** | `GATE: FAIL —— cargo（101）`，红**两条**：① `sftp_move_ledger::every_blocker_this_ledger_registers_is_still_quoted_verbatim_on_disk`（逐字「挡路石「daemon 只读铁律 I7」的逐字校验位在 `doc/INVARIANTS.md` 里找不到了」）· ② `daemon_wording_registry::no_prose_in_the_wording_sites_still_says_daemon`（点名 `doc/INVARIANTS.md:1661`） | ✅ 目标那条红了 | ⚠ **两条** —— 第二条是**设计如此**：把措辞改回去本来就是一次措辞回潮 |
| `d3-1m` 最小面版：`d3-1` ＋ 同拍把那句登进 `EXEMPT`（`EXEMPT_HITS` 15 → 16） | 同上 ＋ 判据里两处锚点各**命中 1 次** | `GATE: FAIL —— cargo（101）`，红**恰好一条**：`sftp_move_ledger::every_blocker…` | ✅ | ✅ **只打该盖的最小面** —— 证明 `d3-1` 那两条红**分得开两个来源** |
| `d3-2` 把 `doc/IPC-PROTOCOL.md` §10 章标题改回旧措辞 | 锚点**命中 1 次** | `GATE: FAIL —— cargo（101）；daemon（101）`，`daemon` 那一格红**四条**：`protocol_doc_guard` 的 `every_wire_frame_kind_has_a_row_in_the_frame_table` · `every_wire_field_appears_in_the_protocol_doc` · `every_inbound_command_appears_in_the_protocol_doc` · `every_dispatched_subcommand_appears_in_the_protocol_doc`；`cargo` 那一格红的是措辞闸 | ✅ 四条全是引那个标题的 | ⚠ **五条** —— 四条是同一个锚点的四个消费者，第五条同 `d3-1` |

## §E 把实现整个退掉，还有多少条新断言仍绿

本件的「实现」= **改 358 处散文** ＋ **立两道闸**。逐处退：

| 退掉哪一处 | 还绿的新断言 | 实测还是推理 | 为什么仍绿 |
|---|---|---|---|
| 退掉 `daemon_wording_registry` 整条（`d1-2` 的后半，4467 字节） | `frozen_daemon_census` 仍绿 | 🔬 **实测**（`d1-2`：`cargo 1612 passed`，1613 − 1 = 只少了被拿掉的那条） | 它盯的是 `evidence/` ＋ `CHANGELOG`，与写区**不相交** —— 不是仪式，是它本来就该对写区沉默 |
| 退掉 `frozen_daemon_census` 整条（`d2-2` 的后半，2344 字节） | `daemon_wording_registry` 仍绿 | 🔬 **实测**（`d2-2`：`cargo 1612 passed`） | 同上，反向 |
| 退掉 `daemon_wording_registry` 整条 | `sftp_move_ledger` / `protocol_doc_guard` 那 5 处引文仍绿 | 🔬 **实测**（`d1-2` 那趟 `cargo` 绿 · `daemon 762` 照旧） | 引文那 5 处由**别人**（既有判据）钉着，本件只是把它们的引文同拍改了 —— **它们的牙不在本件这条闸上** |
| 退掉**全部 358 处散文改动**（`git checkout` 那 9 份） | `frozen_daemon_census` 仍绿 · `daemon_wording_registry` **红 358 条** | ⚠ **推理，本轮没单独跑这一趟**（`d3-1` 只退了其中 1 处，那一趟 `daemon_wording_registry` 就红了并点名 `doc/INVARIANTS.md:1661`） | 冻结面与写区不相交 |
| 退掉 `EXEMPT` 那 15 条登记（表清空） | 两道闸都**红** | ⚠ **推理，本轮没跑**（`d3-1m` 跑的是反方向 —— 往表里**加**一行，`EXEMPT_HITS` 15 → 16 同拍改，两条断言都过） | `EXEMPT_HITS` 恒等断言 ＋ 15 处裸词落进 offenders |

⇒ 🔴 **一条如实的话**：本件那 358 处散文改动，**没有任何一条新断言在「改对了」这个方向上有牙** ——
`daemon_wording_registry` 买的是「**明天写脏了当场红**」，不是「今天改得对」。
「改得对不对」（读起来通不通顺 · 语义有没有改错）**没有机检，只有评审**，如实登记在这里。

## §F ⚠ 诚实边界（两侧都写出来）

1. **射程只有 9 份 `.md`**。`doc/` 今天 11 份，写区点了 4 份；`.md` 那一面还欠 **121 处**
   （逐份点名在 `evidence/K-R116-census.md §4`），`.md` 之外还有 **8749 处**（§5）。
   **这不是漏了，是本轮口径就这么给的。**
2. **界面文案一处没动**。`src/settings/machine-card.ts` 那两个按钮逐字还写着「安装 daemon」
   「卸载 daemon」；`src-tauri/README.md` 里引用它们的那两句因此**登进了 `EXEMPT`**
   （只改文档不改界面 = 文档当场说假话）。UI 文案那一档整体交回 PM 另派。
3. **token 规则有两份实现**（`K-R116-ruler.py::token_at` 与
   `doc_claim_registry::daemon_wording_registry::token_at`），**没有任何东西钉它们同步**。
   闭集那一半已经收成一个住址（量具**解析**判据里的 `SITES` / `EXEMPT`，不抄第二份），
   **算法那一半没有**。`d1-1` / `d1-1b` 两刀只证明「两侧此刻都在按这条规则判」，
   **不证明**「两侧永远一致」。
4. **两形认不出**（`census §7`）：中文夹缝里的标识符（`daemon-协议-v1`）会被切成裸词、
   英文连字符形容词（`daemon-spawned`）会被当成标识符放过。今天前者靠 `EXEMPT`、
   后者靠 `REWRITES` 逐条兜；**同形的新增两道闸都看不见**。
5. **`frozen_daemon_census` 的逐文件那一档只盖量于 `897afec` 的那 70 份**。
   之后新长出来的 `evidence/*.md` 只由整棵树的合计地板兜 ⇒
   「在一份新文件里替换掉 N 处、同一拍另一份新文件又新增 ≥N 处」**本闸静默**。
6. **它判的是处数，不判「那一处还是不是原来那句话」** —— 同一份里删一句、加一句同词的话，本闸看不见。
7. **`d1-2` / `d2-2` 两条阴性对照都不是「一条都不红」**：`fmt` 那一格仍红，
   那是刀自己留下的排版伤（`keep="}"` 补回的花括号没按 rustfmt 排），不是判据在装样子。
   `cargo` 那一格从 1613 掉到 1612 并回绿 —— 那才是这两条要证的东西。
8. **量具住址**：`evidence/K-R116-ruler.py` 与 `evidence/K-R116-cut.py` 都只属于 `K-R116`，
   被测对象是**它们自己所在的那棵工作树**（`parents[1]` / `git rev-parse --show-toplevel`，
   每趟印在第一行）。门禁日志落在会话私有的 scratchpad（`cut-<刀名>.log`），
   ⚠ **那个目录是按项目路径共享的** —— 同名文件可能被别的 agent 覆盖，本报告里的读数
   一律**逐条抄进了这份文件**，不指望那些日志还在。
