# `K-R25` 真改拍 · 剥法的输入单位与看门判据的输入单位对齐 —— 普查 · 死值验 · 诚实边界

**被测对象**：工作树 `/home/zbl/文档/claudecode-frontend/.claude/worktrees/k-r25`，分支 `track/k-r25`，
基点 `d305ffa`，本拍出货尖 **`3aab95a`**。本树**未铺** `src-tauri/embedded-daemons/`
⇒ `embedded_daemons` cfg 不置（cargo 合计里少「本地后端真的能起来吗」那 4 条）。

**跑法**：一律沙箱 —— `PB_WS=backend-consolidation .claude/devbox/gate <本树绝对路径> k-r25`，默认断网。
**宿主上没跑过任何测试**（宿主上只跑过量具 `.py` 与 `git`）。

**量具住址（唯一，只属于本道）**：
- `evidence/K-R25-D1-strip-input-unit-census.py` —— 剥法输入单位普查。
  被测对象由**它自己所在的树**决定（往上找 `.git`，不读环境变量），输出头一行印被测对象路径 + `HEAD` sha。
- 那把**定位棘轮 +3 落在哪几行**的复刻尺子是临时的、**不进 evidence**（住 scratchpad），
  理由见 `§E`：它是 `needle_anchor_registry` 取法的复刻，只用来定位，权威数只来自那条判据本身。

⚠ **下面引到的 `scratchpad/kr25-*.log` 是本次会话的临时目录**
（`/tmp/claude-1000/-home-zbl----claudecode-frontend/abfa3e79-…/scratchpad/`），
**会被清掉、别人也够不着** ⇒ 每一刀的**逐字读数与判定行都抄在正文里**，日志名只是我这一趟的出处。
要复跑就照每一刀写的「锚点 · 口径 · 量于哪个 commit」重打，别去找那个文件。

---

## §A `KR25D1` · 先量：全仓「先切块（切片）再剥」的判据有几条，分母怎么切

### ㈠ 尺子（先写分母，再写数）

| 量 | 尺子逐字 | 量于 |
|---|---|---|
| 采集面 | `git ls-files '*.rs'`，排掉 `src-tauri/vendor/` 与 `src-tauri/crates/guard-core/src/lib.rs`（剥法本体：它内部的自调用是实现，不是调用点） | `d305ffa` / `3aab95a` 各一遍 |
| 命中 | `production_code` · `strip_comment_lines` · `strip_block_comments` · `strip_trailing_comments` · `production_source` · `test_source` · `block_comment_model_holds` 这七个名字（**前面必须是非标识符字符**）后紧跟 `(`；再排掉三类：`//` 打头的纯注释行 · `fn <名字>(` 的定义 · 参数是字符串字面量的自检语料 | 同上 |
| 输入单位 | 四档 `FILE` / `PRESTRIP` / `SUBUNIT` / `PARAM`，判法逐条写在量具头注里 | 同上 |
| 看门判据 | `assert_block_comment_model_holds(` 的调用点单列成 `WATCH` | 同上 |

🔴 **`PARAM` 不是「没事」，是「本量具判不了」** —— 它是形参 / 闭包参数 / 跟不到的名字。
下面 ㈢ 把 `d305ffa` 那趟的 **24 条 `PARAM` 逐条手核**了，结论也逐条写出来。

⚠ **PM 题面里那个「287 处调用点」我复不出同一个数，顶回来**：本量具在 `d305ffa` 上命中 **285** 处
（七个剥法合计，含 `WATCH` 2）；只数 `production_code(` 一个名字、按「排 guard-core 本体 + 排 `//` 打头行」
那把旧尺子是 **268** 处。287 这个数我没重打出来，**不知道它的分母是哪一把**。
⇒ 下面所有的数用我这把尺子，尺子的住址在量具头注里。

### ㈡ 读数（量于 `d305ffa` = 改之前）

```
命中总数 285（另排掉：注释行 12 处 · fn 定义 1 处 · 字面量语料 3 处）
  FILE        247      整份文件的原文
  PRESTRIP      0      切小了，但底座已过过一遍文件级剥法
  SUBUNIT       8      交进去的就是原文的一小块  ← 人群
  PARAM        24      本量具判不了 ← 逐条手核见 ㈢
  WATCH         2      看门判据本体
```

**`SUBUNIT` 8 处逐条（量于 `d305ffa`）**：

| # | 住址 | 剥法 | 切法 | 在本拍写区吗 |
|---|---|---|---|---|
| 1 | `src-tauri/src/local_daemon.rs:3793` | `strip_comment_lines` | 按 `#[test]` 切出的**块** | ✅ **本拍改了** |
| 2 | `src-tauri/src/local_daemon.rs:3922` | `strip_comment_lines` | `me[at..].lines().take(20)` **行窗口** | ✅ **本拍改了** |
| 3 | `src-tauri/src/creds_store.rs:1169` | `strip_comment_lines` | `brace_block(&src, at)` 函数体窗口 | ❌ 写区外 |
| 4 | `src-tauri/src/parity_ledger.rs:468` | `strip_comment_lines` | `&src[open + 1..end]` 切片 | ❌ 写区外 |
| 5 | `src-tauri/src/structural_scan.rs:746` | `strip_comment_lines` | `raw[body_at..]` 起 3000 字符、按大括号配平（配不平退回 700 字符）的窗口 | ❌ 写区外 |
| 6 | `src-tauri/src/tmux_daemon_gate_guard.rs:442` | `production_code` | `body_of(MONITOR_TMUX, "pub async fn kill_remote_tmux(")` | ❌ 写区外 |
| 7 | `src-tauri/src/tmux_daemon_gate_guard.rs:477` | `production_code` | 同上 | ❌ 写区外 |
| 8 | `src-tauri/src/tmux_daemon_gate_guard.rs:512` | `production_code` | `body_of(MONITOR_TMUX, "pub async fn tmux_send_keys(")` | ❌ 写区外 |

⚠ 第 3 处（`creds_store.rs:1169`）**改后被量具重判成 `PRESTRIP`** —— 那不是它变了，是量具补上了
「切小之前底座已经过过一遍文件级剥法」这一档（`let src = production_code(raw);` 之后才 `brace_block(&src, at)`）
⇒ 块注释在文件级那一趟已被抹成等长空格，**这一处的单位再小也不会新掉进兜底**。
**它不在人群里**，登记下来是因为「同形而无害」这件事本身要写清，不然下一个人会把它算进去。

### ㈢ 24 条 `PARAM` 逐条手核（`d305ffa`）

**手核结论：24 条里 23 条是整份文件，1 条是块 —— 而那一条量具漏了，人群要 +1。**

| 住址 | `PARAM` 的原因 | 手核结论 |
|---|---|---|
| `remote-daemon-proto/src/agent_boundary_guard.rs:190` | `production_lines(src)` 的形参 | 调用点 4 处：`:212` 从 `read_to_string` 来 · `:368/:378/:387` 是字面量语料 ⇒ **FILE** |
| `remote-daemon-proto/src/main.rs:848` | `raise_sites_are_gated(prod)` 的形参 | `:923` 的 `prod` 来自 `scan_tree!` + 文件级 `production_code`；`:938/:944/:948/:953` 是字面量语料 ⇒ **PRESTRIP** |
| `remote-daemon-proto/src/readonly_guard.rs:770` | `for (name, raw) in &files`，`files` 由 `Vec::new()` 起 | `files.push((rel, read_to_string(&path)…))` ⇒ **FILE** |
| `src-tauri/src/backend/control/daemon_kill.rs:305` | `let Ok(raw) = read_to_string(&p)` 是 `let-else` | ⇒ **FILE** |
| `src-tauri/src/backend/control/launch_wire.rs:338` | `production_ts(src)` 的形参 | 调用点 `:394/:468/:476/:600/:607` 全是 `read_ts(<整份 .ts>)`，`:362/:365` 是字面量语料 ⇒ **FILE**（⚠ 但那是 `.ts`，见 `§C-2`） |
| **`src-tauri/src/backend/control/local_backend.rs:2379`** | `for c in real_e2e`，`chunks` 由 `Vec::new()` 起 | 🔴 **SUBUNIT** —— `src = include_str!("local_backend.rs")` 原文按 `#[test]` 切块，再逐块 `strip_comment_lines(c)`。**与 `local_daemon.rs:3793` 同形**（`every_real_daemon_e2e_demands_a_private_tmux_dir`）。**量具漏了它，靠手核逮到** |
| `src-tauri/src/backend/control/payload.rs:2257` | `for (path, raw) in &files` | `let files = scan_tree!(&root, &["rs"])` ⇒ **FILE** |
| `src-tauri/src/doc_claim_registry.rs:480` | `prod = |rel| production_code(&read(rel))` | `read = |rel| read_to_string(root.join(rel))` ⇒ **FILE** |
| `src-tauri/src/doc_claim_registry.rs:745` | `for (p, raw) in &srcs` | `srcs` 由 `scan_tree!` 三棵树扩起来 ⇒ **FILE** |
| `src-tauri/src/doc_claim_registry.rs:1355` | 同上 | ⇒ **FILE** |
| `src-tauri/src/launcher_identity_registry.rs:254` | `let raw = read(path)` | `read` 读整份 ⇒ **FILE** |
| `src-tauri/src/local_daemon.rs:2296` | `.filter(|(_, s)| production_code(s)…)` 的闭包参数 | `files = scan_tree!(&root, &["rs"])` ⇒ **FILE** |
| `src-tauri/src/local_read_surface_registry.rs:504` | `for (rel, src) in rust_files()` | `rust_files()` 递归读整份 ⇒ **FILE** |
| `src-tauri/src/local_read_surface_registry.rs:565` | `me` 从 `rust_files()` 里挑一份 | ⇒ **FILE** |
| `src-tauri/src/local_read_surface_registry.rs:585` | 同上闭包参数 | ⇒ **FILE** |
| `src-tauri/src/needle_anchor_registry.rs:277` | `for (path, src) in &files` | `files` 由 `scan_tree!` 三棵树扩起来 ⇒ **FILE** |
| `src-tauri/src/rust_timer_registry.rs:221` | `production(raw)` 的形参 | 调用点 `:364/:386/:513/:606/:637` 全是整份 ⇒ **FILE** |
| `src-tauri/src/structural_scan.rs:1264` | `addr_declarations(&corpus)` 的形参 | `addr_corpus()` = 三棵树 `scan_tree!` + `build.rs` + 自己 ⇒ **FILE** |
| `src-tauri/src/structural_scan.rs:1836` | `dead_name_split(path, text)` 的形参 | `:1870` 来自 corpus；`:2366/:2375/:2412/:2415` 是字面量语料 ⇒ **FILE** |
| `src-tauri/src/tool_registry.rs:836` | 参数是 `concat!(…)` 字面量语料 | ⚠ **而且那个 `production_code` 是本文件自己的私有实现**（`tool_registry.rs:577`），不是 `guard_core` 的 ⇒ **不进人群**，另见 `§C-1` |
| `src-tauri/src/usage.rs:416` | `for (who, raw, …) in [ … ]` 数组 | 数组元素全是 `include_str!` ⇒ **FILE** |
| `src-tauri/src/write_site_registry.rs:165` | `for (p, src) in corpus()` | `corpus()` = `scan_tree!` + `build.rs` ⇒ **FILE** |
| `src-tauri/src/write_site_registry.rs:472` | 同上 | ⇒ **FILE** |
| `src-tauri/src/write_site_registry.rs:526` | `write_fns(src)` 的形参 | 调用点在 `corpus()` 循环里 ⇒ **FILE** |

### ㈣ `KR25D1` 的答案

> **「先切块（切片）再剥」的判据，量于 `d305ffa`：9 处。**
> 分母 = 上面那把尺子的 **285** 处命中；`SUBUNIT` 机检 **8** ＋ 手核从 `PARAM` 里捞回 **1**
> （`local_backend.rs:2379`）。**其中在本拍写区的只有 2 处**（`local_daemon.rs` 那两处），
> 另 7 处**一个字没动**，走上报口（`§D`）。

**按剥法拆**：`strip_comment_lines` 5 处（其中块粒度 3 · 切片 / 窗口 2）· `production_code` 4 处（全是 `body_of` 函数体窗口）。
**按树拆**：monitor `src-tauri/src` **9 处** · daemon `remote-daemon-proto/src` **0 处**。
⇒ 🔴 **顶回 PM 题面一格**：件文件 `KR25D1` 写「daemon 侧那条同名判据……**两侧都在人群里**」——
那句话对**看门判据**成立（两侧都按文件量），对**「先切块再剥」的调用点**不成立：
**daemon 侧今天 0 处**。这一侧本拍装上的块级那一半是**预防**，不是修补。
（⚠ 那个 0 的分母是上面那把尺子的射程，不是「所有写法」的全称。）

**改后（`3aab95a`）复打同一把尺子**：`SUBUNIT` **8 → 6**，`PRESTRIP` **0 → 1**，
`local_daemon.rs:3793` 与 `:3922` 从人群里退出；连手核那一处，人群 **9 → 7**。

---

## §B `KR25D2` · 修：两条出路选哪条、两格前置问题先答

### ㈠ 选：**甲和乙都做**，不是二选一。给论据。

件文件给的两条：
- **甲**：剥法按**文件**跑，切块在剥完之后做；
- **乙**：看门判据按**块**量。

**我两条都做了，理由是它们买的东西不同，缺一格就漏**：

| | 甲（剥法抬到文件） | 乙（看门判据加块这个单位） |
|---|---|---|
| 治的是 | **这两处调用点自己**的单位错配 | 「有别的调用点错配」这件事**将来有人看着** |
| 覆盖 | 只覆盖我改的那两处 | 覆盖**全树每一份文件的每一个 `#[test]` 块** |
| 退掉它会怎样 | 那两条守卫回到「注释里的文本当活代码」（`§C 刀 4` 实测 1 红） | 「整份配平、单块不配平」那一形没人看着（`§C 刀 3` 实测 1 红） |

⇒ **只做甲**：`local_backend.rs` 那条同形守卫（写区外）仍然开着，而且没有任何东西会说话。
⇒ **只做乙**：本条守卫仍然把注释里的文本当活代码判「合规」，只是**旁边多一条判据在喊** ——
判决还是错的（`§C 刀 3` 实测：正题那条**绿着**通过了一段注释里的假合规）。
⇒ 两条一起，**这一形有两个独立红源**，单断 / 全断三格逐格实测在 `§C`。

**乙为什么不是「哪些判据要按文件跑」的词表**（`KR25D3` 点名拒绝的东西）：
它不列判据，也不列文件 —— 它把**「块」这个单位**加进看门判据自己的遍历里，
对全树每一份文件一视同仁。谁将来在任何地方写「先切块再剥」，那一形都被这一条看见。
⚠ 它确实钉了**一个具体的切法**（`#[test]` 块）—— 那是它的射程，逐字写在
`assert_block_comment_model_holds` 头注与 `§C-2` 的诚实边界里，**不是全称**。

### ㈡ 甲的前置问题：**剥完再切块，`#[test]` 边界本身会不会被剥掉？**

**会 —— 而且那是对的。** 读数不是散文，落在 `guard-core` 那条新判据的第 ⑤ 格
（`cutting_first_and_stripping_first_do_not_give_the_same_answer`）：

- 一份 `/*` 落 A 块、`*/` 落 B 块的夹具，**剥之前**切出 **2** 块，**剥之后**切出 **1** 块 ——
  因为第二条 `#[test]` 恰好落在那条块注释**里面**，被抹成了等长空格。
- **它不再是边界，前后两块合成一块。** 那是**对的**：注释里的 `#[test]` 不是一条测试，
  它本来就不该切出一个块来；上一版把它当边界，正是「把注释当代码」的同一个病。
- **行位不动**：`strip_block_comments` 等长抹空格、`strip_comment_lines` 把整行注释换空行，
  两步都不改行数、不改字节位 ⇒ 同一条判据里那个 `take(20)` 的窗口仍然是原文那 20 行。
  这一格也断在同一条判据里（`stripped.lines().count() == 原文.lines().count()`）。
- **今天全树的代价 = 0 块**：`105 份 / 1280 块` 与改之前逐字相同（`§C-0` 的读数），
  也就是说今天没有任何一条真的 `#[test]` 落在块注释里。

⚠ 顺带一格：锚点也跟着搬到了剥过的文本上找（`me.find("fn demand() -> Self {")`）。
**那是更严不是放水**：锚点要是落进块注释里，`find` 会找不到并当场 panic（fail closed）；
上一版在**原文**里找，一句注释里的同名文本就能把窗口带偏。

### ㈢ 乙的前置问题：**块级看门的分母怎么报？**

**换尺子，两个数一起印，两个地板各自量。**
`assert_block_comment_model_holds(root, min_files, min_blocks)`：
- `min_files` 挡「遍历坏了」（旧的那一格，一个字没改）；
- `min_blocks` 挡「切法坏了」（新的那一格）——
  切法一坏（比如有人把 `test_attr_chunks` 改成恒返回一个块），块数会塌，而**块级那一半会静默变空转**；
- 报错模板里**两个数一起印**（「本趟扫了 N 份文件 · 切出 M 块 —— 块数不是文件数，两个数各自读」），
  免得下一个人拿块数去对文件数。

**三处地板都实测有牙**（`§C-0`：把 `min_blocks` 临时调到 `9_999_999`，三条各自红并印出真值）：

| 判据 | 树 | 份数 | 块数 | 本拍钉的地板 |
|---|---|---|---|---|
| `local_daemon.rs::no_monitor_file_falls_back_to_leaving_block_comments_in` | `src-tauri/src` | **105** | **1280** | `100` / `1200` |
| `single_stream_guard.rs::no_daemon_file_falls_back_to_leaving_block_comments_in` | `remote-daemon-proto/src` | **73** | **603** | `70` / `500` |
| `guard-core::this_crate_never_falls_back_to_not_stripping` | `crates/guard-core/src` | **1** | **41** | `1` / `30` |

⚠ 地板是**「坏了要红」的下界**，不是等式；棘轮纪律：只许升不许降。
⚠ 三个块数都是**这一刻的读数**（量于 `3aab95a`），谁加一条 `#[test]` 它就变 —— 引用前重打。

---

## §C `§3` 死值验 · 七刀（每刀：锚点 · 命中数 · 口径 · 量于哪个 commit · 判定行 · 分母）

**语料（探针）逐字**，两形只差 `*/` 落在哪一块 —— 插在 `local_daemon.rs` 的 `mod tests` 末尾：

```text
形 ①「跨块」（整份配平、单块不配平）：
    #[test]
    fn probe_one() {
        let _ = "CCM_E2E_DAEMON";
        /*
        let shim = demand_tmux_shim("探针");
        let _envs = vec![("PATH".to_string(), format!("{shim}:{}", "x"))];
    }

    #[test]
    fn probe_two() {
        */
        let shim = String::new();
        let _ = shim;
    }

形 ②「块内配平」（对照臂）：把 `*/` 挪进 probe_one 里，probe_two 换成 `let _ = 0;`。
```

**为什么它编得过**：对 rustc 形 ① 就是一条合法的跨行块注释，
`fn probe_two` 的 `#[test]` 与签名整个落在注释里 ⇒ 剥完只剩一个 `probe_one`，
它的花括号由原来 `probe_two` 的那个 `}` 收口。**cargo 只多一条测试。**

**它为什么落在判据的人群里**（必要条件，逐条写）：
㈠ 块里有 `CCM_E2E_DAEMON`（三条来历字面量之一）· ㈡ 块里没有 `include_str!(`（守卫排除）·
㈢ 块里有 `demand_tmux_shim(` 这个字面（否则 `asks_itself` 为假、直接落 `MISS_LOCK2`）·
㈣ 含口那一行 `trim_start()` 之后以 `let ` 打头 · ㈤ 有一行同时含 `"PATH"` 与 `"{shim}:` ·
㈥ `"PATH"` 后紧跟方法调用恰好 1 处。

### 刀 0 —— 地板有牙 + 三个真读数（无探针）

**锚点**：三处 `min_blocks` 实参，各命中 **1** 次；口径 = 把它调到 `9_999_999` 逼判据自己印真值
（这是本仓复跑分母的权威路子，`single_stream_guard.rs` 头注里逐字给过同一招）。
**量于 `3aab95a`**。两趟（一趟 guard-core + daemon、一趟 monitor + daemon，因为 cargo 在第一个失败的
测试二进制上就停）。**判定行**：`guard_core 39 passed; 1 failed` · `monitor 1280 passed; 1 failed` ·
`daemon 541 passed; 1 failed`。逐字读数：

```
只切出 41 个 `#[test]` 块（期望至少 9999999，扫了 1 份文件）
只切出 1280 个 `#[test]` 块（期望至少 9999999，扫了 105 份文件）
只切出 603 个 `#[test]` 块（期望至少 9999999，扫了 73 份文件）
```

⇒ 三条地板都真的会红、都会印出分母。**分母**：`git ls-files` 数出的 `.rs` 份数
（`src-tauri/src` 105 · `remote-daemon-proto/src` 73 · `crates/guard-core/src` 1）与判据自己印的逐字相同。

### 刀 1 —— 🔴 **改之前 · 形 ①：全绿（洞复现）**

**锚点**：探针形 ①，插在 `local_daemon.rs` `mod tests` 末尾（命中 1 处）。
**口径**：三个文件用 `git restore --source=d305ffa --worktree` 退回基点，只留探针。
**量于**：`d305ffa` 的三份源码 ＋ 探针。日志 `scratchpad/kr25-cut1-before-straddle.log`。

| 格 | 读数 |
|---|---|
| `no_monitor_file_falls_back_to_leaving_block_comments_in` | **绿**（cargo 全绿 ⇒ 它在内） |
| `every_test_that_starts_the_real_daemon_demands_a_private_tmux` | **绿** —— 它把**注释里的文本**判成「合规」 |
| **cargo（8 包合计）** | **1391 passed**，0 failed（基线 1390 ＋ 探针 1） |
| daemon / npm / ccm e2e | **542** · **1541** · **12 / 8 / 242 / 68**，全绿 |
| `GATE` | 只有 `pb check FAIL=1`，而那一条是别的道的件文件（`§E-2`） |

⇒ **09-01 那个静默假绿的形状，在 `K-R9` 的修补之上、在本拍的基点上，原样复现了一次。**

### 刀 2 —— **改之后 · 形 ①：两个独立红源同时红（全断）**

**量于** `3aab95a` ＋ 探针形 ①。日志 `scratchpad/kr25-cut2-after-straddle.log`。
**判定行**：`monitor 1279 passed; 2 failed; 10 ignored`（刀 1 那一趟是 1281 passed / 0 failed）。

```
thread 'local_daemon::tests::every_test_that_starts_the_real_daemon_demands_a_private_tmux' panicked:
  这几条会起**真** daemon，却没有 fail-closed 地要 `CCM_E2E_TMUX_SHIM_BIN` **并真的用上它**：

thread 'local_daemon::tests::no_monitor_file_falls_back_to_leaving_block_comments_in' panicked:
  1 个 `#[test]` **块**走了块注释剥法的兜底（整份文件是配平的，单块不配平）…：
  ["…/k-r25/src-tauri/src/local_daemon.rs#块28"]
  （分母：本趟扫了 105 份文件 · 切出 1282 块 —— 块数不是文件数，两个数各自读。）
```

⚠ 那个 **1282** 是**带探针**的读数（探针加了 2 条 `#[test]` 边界）；干净树是 **1280**（刀 0 印的）。

### 刀 3 —— **只退甲（剥法退回块级），保留乙：恰好 1 红**

**锚点**：`for (who, src) in &stripped` → `for (who, src) in files`（命中 1 处）
＋ `let code: &str = c;` → 逐块 `strip_comment_lines(c)`（命中 1 处）。**类型契约不变**（两侧都是 `&str`）。
**量于** `3aab95a` ＋ 两处退回 ＋ 探针形 ①。日志 `scratchpad/kr25-cut3-only-yi.log`。
**判定行**：`monitor 1280 passed; 1 failed`。

⇒ 红的**只有** `no_monitor_file_falls_back_to_leaving_block_comments_in`（点名 `#块28`）；
正题那条**又变绿了** —— 它照旧把注释里的文本判成「合规」。
★ **这一格买到的是「乙单独有牙」，同时也是「只做乙不够」的证据**：判决仍然是错的。

### 刀 4 —— **只退乙（看门判据不量块），保留甲：恰好 1 红**

**锚点**：`if !block_comment_model_holds(&c) {` → `if false && !block_comment_model_holds(&c) {`
（命中 1 处；**形状对、恒答其中一张脸**，不是把返回类型换掉）。
**量于** `3aab95a` ＋ 该处变异 ＋ 探针形 ①。日志 `scratchpad/kr25-cut4-only-jia.log`。
**判定行**：`monitor 1280 passed; 1 failed`。

⇒ 红的**只有** `every_test_that_starts_the_real_daemon_demands_a_private_tmux`。
★ **甲单独有牙。**

### 刀 5 —— **对照臂 · 形 ②（块内配平）· 改之后：红**

**量于** `3aab95a` ＋ 探针形 ②。日志 `scratchpad/kr25-cut5-after-contained.log`。
**判定行**：`monitor 1281 passed; 1 failed`。报文逐字：

```
      走的腿：两条腿都没走（连第二道锁 ② 都没有）
      差在：没走取 shim 的那个唯一入口 `demand_tmux_shim(..)`，也没委托给 `E2eSandbox::demand()`（**第二道锁 ②**）。
```

⚠ 看门判据这一趟**绿**（块内配平 ⇒ 块级也不掉兜底）⇒ 红源恰好一个，没有粗刀。

### 刀 6 —— **对照臂 · 形 ② · 改之前：也红**

**量于** `d305ffa` 的三份源码 ＋ 探针形 ②。日志 `scratchpad/kr25-cut6-before-contained.log`。
**判定行**：`monitor 1281 passed; 1 failed`，报文逐字与刀 5 相同。

⇒ **`K-R9` 买到的东西是真的，本拍一格都没弄丢**：它的射程是「交进剥法的那段文本自己配平」，
而本拍改的正是**哪一段文本被交进去**。

### 刀 7 —— `7u`：把实现整个退掉，还有多少条新断言仍绿

本拍新增的断言只有三样，逐条：

| 新断言 | 退掉甲之后 | 退掉乙之后 | 仍绿的理由 / 它的牙在哪一刀上 |
|---|---|---|---|
| `guard-core::cutting_first_and_stripping_first_do_not_give_the_same_answer`（6 组断言） | **仍绿**（刀 3 实测：那一趟它 passed） | **仍绿**（刀 4 实测：那一趟它 passed） | 🔴 **如实写：它对「甲有没有落地」没有牙。** 它断的是**原语与切法本身的性质**，不是某个调用点的次序。它的牙在别的刀上：兜底改成「照剥」⇒ ②③ 断；词法不再跨行延续 ⇒ ①④ 断；剥法不再抹掉注释内容 ⇒ ④ 断；「剥完再切边界会消失」这一格变了 ⇒ ⑤ 断；行数变了 ⇒ ⑤ 的第二格断。⇒ 它是 `KR25D3` 要的那条「关系」，**不是甲的验收** |
| 看门判据的**块**那一半（乙） | 干净树上仍绿；**有洞的树上红**（刀 3：1 红并点名 `#块28`） | —— | **不是仪式**，牙在刀 3 |
| 三处 `min_blocks` 地板 | 仍绿 | 仍绿 | **不是仪式**，牙在刀 0（三条各自红并印真值） |

⇒ **`7u` 的诚实答案：3 条里有 1 条在「退掉甲」这把刀上仍绿**，理由逐字写在上表，
它不是零价值（它是 `KR25D3` 那条关系的落点），但**它证不了甲落地了** —— 那件事由刀 4 证。

---

## §C-2 诚实边界（别把这一拍读大）

1. **对齐的是两个单位，不是所有单位。** 看门判据今天量「整份文件」＋「按 `#[test]` 切出的块」。
   函数体窗口（`body_of` / `brace_block`）· 任意切片 `&src[a..b]` · `.lines().take(n)` 行窗口 ——
   **它一个都看不见**。今天全仓还剩 **7 处**这样的调用点（`§A ㈣`，全在写区外，逐处走上报口）。
2. **只管 `.rs`。** `launch_wire.rs::production_ts` 与 `tool_registry.rs` 那套走的是 `.ts` / 自己的私有剥法，
   两条看门判据的遍历**只认扩展名 `rs`** ⇒ 那一轴今天**没有任何看门判据**。只登记，本拍不动。
3. **兜底的语义一个字没改。** 我没有动 `try_strip_block_comments` 的词法，也没有把兜底从
   「一个字都不剥」改成 panic —— 那会把「宁可留洞，不许造假红」这条写死的取舍翻面，
   而且 `structural_scan.rs:746` 那个「配不平就退回 700 字符」的窗口很可能当场假红（写区外，我改不了）。
   ⇒ **本拍治的是作用域，不是词法**（件文件 `§2` 逐字）。
4. **「daemon 侧 0 处块级调用」那个 0** 的分母是 `§A` 那把尺子的射程（含 24 条手核），
   **不是「所有写法」**。
5. **本文件里所有数都是那一刻的读数**（量于 `d305ffa` / `3aab95a`，逐处标了）。引用前重打。

---

## §D 上报口（写区外，一个字没动，逐条给住址 + 逐字 + 为什么）

- 〔R-K-R25-1〕**本条不挂 dod** —— `src-tauri/src/backend/control/local_backend.rs:2315`-`2379`：
  `every_real_daemon_e2e_demands_a_private_tmux_dir` **与本拍改掉的那条完全同形**（先按 `#[test]`
  切原文、再逐块 `guard_core::strip_comment_lines(c)`）。逐字：
  `let src = include_str!("local_backend.rs");` …（按行切块）… `let code = guard_core::strip_comment_lines(c);`
  ⇒ 它今天仍然「先切块再剥」。**本拍装上的乙（块级看门）会看见它掉进兜底**，
  但它自己的判决仍然会被注释里的文本喂饱。**该同拍治的是同职的两处 —— 我只够得着一处。**
  处置建议：照本拍的甲改（`guard_core::test_attr_chunks` 已经是共享原语，两行的事）。
- 〔R-K-R25-2〕**本条不挂 dod** —— `src-tauri/src/tmux_daemon_gate_guard.rs:442 / :477 / :512 / :552`：
  四处 `guard_core::production_code(&body_of(MONITOR_TMUX, <签名>))`，`MONITOR_TMUX = include_str!("tmux.rs")`。
  `body_of` 从签名切到第一个 `\n}\n` ⇒ **函数体窗口**这个单位，两条看门判据都看不见。
  ⚠ 该窗口里出现一个不收口的 `/*` 就掉兜底。今天没有（`tmux.rs` 生产段 0 处块注释），**是运气不是设计**。
- 〔R-K-R25-3〕**本条不挂 dod** —— `src-tauri/src/parity_ledger.rs:468`
  `guard_core::strip_comment_lines(&src[open + 1..end])`：`generate_handler![…]` 方括号内容的切片。
- 〔R-K-R25-4〕**本条不挂 dod** —— `src-tauri/src/structural_scan.rs:746`
  `guard_core::strip_comment_lines(&body)`，`body` = `raw[body_at..]` 起 3000 字符、按大括号配平，
  **配不平就退回定长 700 字符** ⇒ 那一支**很容易切在字符串中间**、当场掉兜底。这一处的风险比另外几处高。
- 〔R-K-R25-5〕**本条不挂 dod** —— `src-tauri/src/tool_registry.rs:577` 有一份**私有的**
  `fn production_code(src: &str) -> String`：它按 `l.find("//")` 截行、再
  `.split(concat!("\n#[cfg","(test)]")).next()` 切测试段。**两点**：① 它**一个块注释都不剥**；
  ② 「第一个 `#[cfg(test)]` 之后全砍」正是 `guard-core` 头注逐字骂过的坑 2。
  ⚠ 我**没有**核它有没有登记进 `structural_scan.rs` 那张「剥注释只许有一个权威实现」的表 ——
  **那是我没查，不是查过没有。**
- 〔R-K-R25-6〕**本条不挂 dod** —— `evidence/K-R25-D4-nine-uncovered-files.md` 里那条建议：
  给 `crates` 与 `build.rs` 补第三条看门判据。**`KR25D4` 写死了「只答不改」，所以我没动手。**

---

## §E 两处「不是我的账」，各带证据

1. **`needle_anchor_registry` 那条递减棘轮，我第一版把它顶红了 —— 已经在本拍改法内解决，不是调上限。**
   第一版写 `let stripped: Vec<(&str, String)> = files.iter().map(..).collect();`
   ⇒ 那条棘轮的语料变量集按名字走传递闭包：`files`（种子 `scan_tree!`）→ `stripped` → `me` →
   `at` → `body` → `head` …（一趟卷进 13 个名字），于是**本文件别的测试里既有的三行**
   （`local_daemon.rs` 的 `body.contains("LOCAL_RELAY")` · `head.contains("target_os = \"linux\"")` ·
   `!me.contains("DefaultHasher")`）被算成新增欠账 ⇒ `.contains( 33→36` · `.find( 8→11` · `.matches( 9→11`
   **三条齐红**（沙箱实打，`scratchpad/kr25-gate-2.log`）。
   ⇒ 那正是那条棘轮头注自己叫的「**判据跑飞**」。**处置：断掉那条派生边**（改成 `Vec::new()` + `push`，
   语义逐字等同），**不调上限**（递减棘轮），**也不改别人那三行**。改后三个数逐字回到 33 / 8 / 9。
   ⚠ 定位用的那把复刻尺子住 scratchpad，**不进 evidence**：它是那条判据取法的复刻，会漂；
   它在 `d305ffa` 上复现出 33（与判据自己印的数逐字相同）才拿来定位，权威数只来自判据本身。
2. **`pb check FAIL=1` 每一趟都有，两趟是两条不同的红，都指向别的道的件文件**：
   一趟 `[J3 陈账] INDEX.md 比源文件旧`（计划仓里 `K-R1` / `K-R2` / `K-R24` / `K-W1C` / `K-W3`
   五份件文件被别的道改着），一趟 `[J5 表格列数] features/K-W3-合并本体先摸底再拆件.md:556`。
   **`pb index` 是生成命令，窗口开着期间不许跑（`brief` 19），归 PM。**
3. **npm 那几趟超时红**：三趟里分别是 `eslint-baseline`（120s 超时）· `commands.vitest.ts`（5s 超时）·
   `panel-groups.vitest.ts`（5s 超时），失败形态全是 `Test timed out`，不是断言。
   同一批里 `eslint-baseline` 一趟跑了 **57.7 秒**（它自己的预算是 120s）⇒ 机器上并发很重（本波 13 道同跑）。
   **而后四趟 npm 全绿（1541 passed）**，包括退回基点那两趟 —— 我的改动**一行 TS 都没碰**。
   ⇒ 判**争用型 flaky**，不是我的账；证据就是上面这四趟。
