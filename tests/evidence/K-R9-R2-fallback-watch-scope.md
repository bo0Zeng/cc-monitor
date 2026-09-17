# `K-R9` 落定拍 · 静默兜底那两条判据的重打与死值验

**量于**：工作树 `k-r9b`，基点 `4eb271f`（本树**未铺** `src-tauri/embedded-daemons/` ⇒ `none` 口径）。
**跑法**：一律沙箱（`ccmon-devbox:latest`，与 `.claude/devbox/gate` 逐项相同的挂载，
只把最后那句 `bash scripts/gate.sh` 换成一条 `cargo test`）。**宿主上没跑过任何测试。**

---

## §A 尺子怎么切的（先写分母，再写数）

| 量 | 尺子逐字 | 量于 |
|---|---|---|
| monitor 侧分母 | `find src-tauri/src -name '*.rs' \| wc -l` —— 与 `guard_core::assert_block_comment_model_holds` 的遍历同语义（递归、只认扩展名 `rs`） | `4eb271f` |
| daemon 侧分母 | `find remote-daemon-proto/src -name '*.rs' \| wc -l` | `4eb271f` |
| 「走兜底几份」 | **不是**我另写的复刻词法，而是**那两条判据自己跑一趟**：绿 = 0 份，红 = 报文逐份点名 | `4eb271f` |
| `production_code(` 调用点 | `git grep -F 'production_code(' -- '*.rs'`，**排掉 guard-core 本体那一份、排掉 `//` 打头的纯注释行**（这把尺子是从件文件 `§D ㈡` 那张表反推出来的，见下） | 三个提交各一遍 |
| 全仓 `.rs` | `git ls-tree -r --name-only <提交> \| grep '\.rs$' \| grep -v vendor` | 两个提交各一遍 |

★ **那把「252」的尺子是复现出来的，不是猜的**：同一条命令在 `7f8ffcd` 上打出 **252 处 / 81 份**，
与件文件那张表逐字相同 ⇒ 尺子对上了，今天的数才可比。

---

## §B ㈡ 两条判据重打（PM 说分母变了 —— 结论是**没变**，但这是量出来的，不是抄的）

| 判据 | 分母（份） | 走兜底 | 上一拍写的 | 今天 |
|---|---|---|---|---|
| `local_daemon.rs::tests::no_monitor_file_falls_back_to_leaving_block_comments_in` | **105** | **0** | 105 / 0 | **两个数都还成立** |
| `single_stream_guard.rs::tests::no_daemon_file_falls_back_to_leaving_block_comments_in` | **73** | **0** | 73 / 0 | **两个数都还成立** |
| `guard-core/src/lib.rs::this_crate_never_falls_back_to_not_stripping` | **1** | **0** | （上一拍没单列） | 绿 |

沙箱实跑（三条各一趟，全绿）：

```
test local_daemon::tests::no_monitor_file_falls_back_to_leaving_block_comments_in ... ok
test single_stream_guard::tests::no_daemon_file_falls_back_to_leaving_block_comments_in ... ok
test tests::this_crate_never_falls_back_to_not_stripping ... ok
```

⚠ **「分母没变」这件事本身要写清是怎么来的**：`00dcc9d..4eb271f` 之间合进来 7 个合并
（`K-R2` `K-R12` `K-R16` `K-R22` `K-P4` `K-R23` `K-R24`），**它们一份 `.rs` 都没新增到这两棵子树里**
—— 不是「没人动过」，是「动的不是文件数这一维」。同期 `production_code(` 调用点从 **252 → 256**（同一把尺子）。

---

## §C §3 死值验：三刀

### 刀 1 —— 让 monitor 那份真的走进兜底（**判据红，且逐份点名**）

往 `local_daemon.rs` 里塞一行**能编译**的 Rust：

```rust
let _kr9b_poison = (br"\", "/*");
```

机制（读 `guard-core/src/lib.rs::try_strip_block_comments` 得到，实测证实）：
`br"…"` 是**原始**字节串（不认转义），而剥法在 `raw_string_open` 拿到 `h == 0 && sb[i] == b'b'`
这一格时把它当**认转义的普通串**处理 ⇒ 那个 `\` 把收尾引号吃掉、词法与文本失步；
失步之后紧跟的 `"/*"` 在它眼里落在**码**状态上 ⇒ `depth` 开到 1 且此后再没有 `*/` 合上
⇒ 扫完 `depth != 0` ⇒ **兜底：一个字都不剥**。

判据当场红，报文逐字：

```
1 份文件走了块注释剥法的**兜底**（一个字都没剥）⇒ 这几份上「块注释喂饱判据」那个洞此刻是**开着**的：
["…/k-r9b/src-tauri/src/local_daemon.rs"]
```

⇒ **这条判据不是摆设**：它红、它点名、它把「哪一份」说出来了。

### 刀 1b —— 同一状态下跑**全量** monitor（买到的是「兜底到底有多静默」这个读数）

```
test result: FAILED. 1276 passed; 1 failed; 10 ignored
```

⇒ 一份文件整个掉进兜底，**全树只有这一条判据会说话**，其余 1276 条一个数都不动。
★ 这正是那条判据存在的理由，**现在它是一个读数，不再是一句散文**。

### 刀 2 —— 🔴 **块级兜底：判据全绿，而洞是开着的**

刀 1 只证了「文件掉进兜底 ⇒ 判据红」。反过来问一句就出事了：
**剥法真正跑在哪个单位上？** 读 `local_daemon.rs::tests::every_test_that_starts_the_real_daemon_demands_a_private_tmux`
——它先把源码按 `#[test]` 切成**块**，再对**每一块**调 `guard_core::strip_comment_lines`。
⇒ **剥法的单位是「块」，而看门判据量的是「文件」。两把尺子的作用域对不上。**

造一形：`/*` 落在 A 块里、`*/` 落在 B 块里（**整份文件配平，单块不配平**），
真调用 `demand_tmux_shim(..)` 与那行合规的 `PATH` 写位**都在注释里**，活着的只有一个桩值。
这**能编译**（对 rustc 它就是一条跨行块注释）。

| | 读数 |
|---|---|
| `no_monitor_file_falls_back_to_leaving_block_comments_in` | **绿**（文件级配平，它看不见） |
| `every_test_that_starts_the_real_daemon_demands_a_private_tmux` | **绿** —— 它把**注释里的文本**当活代码判成「合规」 |
| 全量 monitor | **`1278 passed; 0 failed; 10 ignored`** |

⇒ **`K-R9` 09-01 那个「静默假绿」的形状，在 `4eb271f` 上、在 `K-R9` 的修补之上，原样复现了一次。**
⚠ 与 09-01 那次的区别只有一处，而正是这一处要命：**那时看门判据还不存在；今天它在，而且是绿的。**

### 刀 2 的对照臂（同一形状、块内配平）

同一个假 e2e，把 `/*` 与 `*/` 收进同一块里 ⇒ 剥法照常剥掉 ⇒ 判据**当场红**，报文逐字：

```
走的腿：两条腿都没走（连第二道锁 ② 都没有）
差在：没走取 shim 的那个唯一入口 `demand_tmux_shim(..)`，也没委托给 `E2eSandbox::demand()`（**第二道锁 ②**）
```

⇒ `K-R9` 买到的东西**是真的**，它的射程是「**交进剥法的那段文本自己配平**」，不是「文件配平」。

---

## §D 今天的曝光：**0 / 1278**（latent，不是活的）

用 guard-core **自己那把**词法（`guard_core::block_comment_model_holds`，不是复刻）按 `#[test]`
切块普查 `src-tauri/src`：

```
【块级普查】文件 105 份 · 按 #[test] 切出的块 1278 个 · 块自己走兜底 0 个
```

⇒ **今天没有人踩上去**。但「文件级 0」与「块级 0」今天相等是**巧合，不是被谁钉住的**：
刀 2 只改了一处，块级就变成 1 而文件级还是 0。

---

## §E 覆盖面：三条判据一共看着 **179** 份，而排掉 vendor 的全仓是 **188** 份

`105 + 73 + 1 = 179`。差的 **9** 份 = `src-tauri/crates/` 下非 guard-core 的 8 份 ＋ `src-tauri/build.rs`。

🔴 **这 9 份里至少有一份是真被 `production_code` 吃的**：
`usage.rs::tests::the_usage_kou_jing_has_exactly_one_home` 把 `crates/usage-core/src/lib.rs`
整份喂给 `guard_core::production_code`。⇒ 那一份要是掉进兜底，块注释洞当场重开，**三条判据一条都不会响**。

（另两处同族、但**不走** `production_code`，只登记不并案：
`creds_store.rs::tests::the_plaintext_argument_is_only_ever_handed_one_hop_further` 读 `crates/creds-core/src/lib.rs` 原文；
`build.rs` 被 `write_site_registry.rs` 与 `sftp.rs` 各读两处。）

⚠ `guard-core/src/lib.rs::assert_block_comment_model_holds` 的头注今天写着「本仓 187 份 `.rs` 今天是 0 份」——
**「本仓」这个词今天盖不住任何一个我量得出的数**：排 vendor 的全仓是 188（`7f8ffcd` 上也是 188），
三条判据真正扫到的是 179。**guard-core 不在本拍写区，只登记，不改。**

---

## §F 尺子的作用域对不上事实 —— 本轮这一族的落点

| 出口 | 剥法真正跑在什么单位上 | 看门判据量的单位 | 对得上吗 |
|---|---|---|---|
| `local_daemon.rs::tests::every_test_that_starts_the_real_daemon_demands_a_private_tmux` | **`#[test]` 块** | 文件 | ❌ **实测对不上（刀 2）** |
| `local_backend.rs::tests::every_real_daemon_e2e_demands_a_private_tmux_dir` | **`#[test]` 块**（同形写法） | 文件 | ❌ 同形，**本拍没单独打刀** |
| 一大批 `production_code(include_str!(<字面量>))` 型判据 | 整份文件 | 文件 | ✅ 对得上 |

**分母写清**：`strip_comment_lines(` 今天有 **19** 处调用点（排 guard-core 本体与纯注释行，量于 `4eb271f`）。
其中 **5** 处交进去的是「刚从盘上读出来的整份原文」（看门判据覆盖得到），
另 **14** 处交进去的是一个已经在手的变量（块 / 切片 / 派生文本）——
**这 14 处逐处的来历我没有逐个核**，我只实打了其中一处（`local_daemon.rs` 那条守卫的 `#[test]` 块）。
⇒ 「有几处真的对不上」这个数**我给不出**；能确定的是**至少 1 处对不上，而且它是活的、能编译的、全绿的**。
