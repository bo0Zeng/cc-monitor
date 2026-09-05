# K-R24 `D7` · `launch.rs` 那条开窗判据的 `ETXTBSY` —— 「一条判据的前提没人建立」的教科书例子

量具：`evidence/K-R24-D7-load-axis-stress.py`（复现台 + 出声计数）·
`evidence/K-R24-D7-exec-after-write-census.py`（人群的候选面）。
一切在 `ccmon-devbox:latest`（`sha256:553f5932…`，建于 09-03T11:17）里跑，
一律 `--network none`，宿主上零测试。工作树 `.claude/worktrees/k-r24c`，基点 `d305ffa`。

---

## 一 · 病历（PM 09-04 夜给的，我独立复读了它的日志）

同一棵树、同一个提交（`d305ffa`）两趟门禁：

| 趟 | cargo 那一格 | 红的是谁 |
|---|---|---|
| 第一趟 | **1279 passed / 1 failed** | `launch::tests::the_terminal_we_hand_the_command_to_really_gets_the_relay_prefix` |
| 第二趟 | **1390 passed** | 全绿 |

红的报文逐字：`spawn 假终端应成功: "spawn 本地命令失败: Text file busy (os error 26)"`。

**⇒ 一个读数装着两件事**：① 开窗那一支的 spawn 真坏了（真缺陷）；
② 这一趟 exec 撞上了 `ETXTBSY`（环境时序，与被测那一半无关）。
★ **这正是 `K-R24` 的正题，长在一条测试上。**

**PM 今晚全部日志我自己重打了一遍**（`grep -l 'Text file busy'`，量于 22:19）：
`ETXTBSY` 在 **15 份 `d305ffa` 门禁日志里出现 1 份**（`baseline-k-r25.log`）。
分母 = `warm-k-*.log` **12** + `baseline-k-r25.log` / `-rerun.log` **2** + `base-d305ffa.log` **1**。
⚠ 这 15 趟的负载条件**不同质**（PM 说 12 棵那批是「3 棵一批并行冷编译」下跑的），
**别把 1/15 读成一个均匀的概率**。

---

## 二 · 机制：是不是 fork 竞态，证据是什么

`ETXTBSY` 的定义（唯一必要条件）：**execve 的目标文件此刻正被某个进程打开着写。**

三条证据，第一条是本拍新装的判据自己给的：

1. **【实测 · 判据自己断言的】攥着一把写句柄时，execve 必然 `ETXTBSY`。**
   新判据 `an_open_write_handle_reads_as_an_unmet_premise_not_as_a_broken_spawn` 的腿②
   就是这一格：自己 `OpenOptions::write(true).open()` 一把不放，然后起同一个文件 ⇒
   实测逐字 `spawn 本地命令失败: Text file busy (os error 26)`（刀 A 那一趟的输出里看得见）。
   ⇒ **报文与病历里那一句逐字相同** ⇒ 病历那一趟的成因**只能是**「有人开着写」。

2. **【读代码 · 排除法】那个「有人」不是本进程自己。**
   判据写脚本走的是 `install_fake_terminal`：具名句柄 → `sync_all` → **显式 drop** → 才 chmod、才 exec。
   ⇒ 本进程在 exec 那一刻**确定性地**没有这个文件的写 fd。
   （改之前是 `std::fs::write`，它**也**会关，但那是实现细节 —— 见第四节「修法」。）

3. **【实测 · 剩下的唯一来路，而且它真的在发生】别的线程起进程时的 fork 窗口。**
   这个测试二进制的**生产段**有 40+ 处真起进程的调用点（`write_site_registry` 的 `SPAWNS`
   逐条登记着；现打 `.spawn()/.output()/.status()` 在 `src-tauri/src` 下 **42** 处）。
   起进程要么 fork 要么 posix-spawn，两者都把父进程此刻打开的 fd **复制一份给子进程**，
   而 close-on-exec 要到**子进程 exec 那一刻**才生效 ⇒ 「fork 之后、exec 之前」那段窗口里，
   那个子进程就是**一个握着我们这个文件写 fd 的进程**。
   ★ **它真的在发生，有计数**：本拍给重试加了一行出声，`N0` 格 20 趟里
   **自然发生 1 次**（见第三节）。
   ⚠ **诚实边界**：这一条证明「前提确实被破过，而且不是本进程自己破的」，
   **不证明**「破它的那一次一定发生在某个具体的兄弟测试的 fork 里」——
   要点到那一次是谁，得在内核侧盯 `i_writecount`，本拍**没做，判不了**。

★ 同一个成因在别的工具链上是有名的：go 与 cargo 都是靠**对 `ETXTBSY` 有上限地重试**收的。

---

## 三 · 复现率读数（口径与分母都写明）

**口径**：`docker run` 逐字复刻 `.claude/devbox/gate` 的挂载与环境（`HOME=/home/zbl` +
那句 `mkdir` + 同一个 `CARGO_TARGET_DIR` + `PB_WS`），**只多 `--cpus`**；
一格 = 在**同一个容器**里把 `cargo test -p monitor --lib -- --nocapture` 重复跑 R 趟
（容器启动不进读数）。分母 = 日志里 `===== ROUND n =====` 的个数（**开了头的轮数**，不是 R
——「没跑」与「跑了没红」在终端上一模一样）。

**出声计数怎么分的**（这一格第一版错过一次，如实记）：那一行是被测代码自己打的，
它把重试上限写在里面 ⇒ **上限 50** = 真判据那一处（`FAKE_TERM_ETXTBSY_TRIES`）= **自然发生**；
**上限 2** = 那条「主动把前提破掉」的判据自己造的 = **每趟恒定 2 次，不进人群**。
不分开数会把 40 次自造的算进去 —— 那是一次假读数。

| 格 | 口径 | 轮数 | cargo 判决 | **自然 `ETXTBSY` 次数** | 判据自造 |
|---|---|---|---|---|---|
| `N0`（修后） | 无 `--cpus` 限制 | **20** | `ok 1281 passed / 0 failed` × 20 | **1** | 40 |
| `N2`（修后） | `--cpus=1` | **8** | `ok 1281 passed / 0 failed` × 8 | **0** | 16 |
| `N2`（修**前**） | `--cpus=1` | **6** | `ok 1280 passed / 0 failed` × 6 | 量不到（修前没有那行出声） | — |

★ **`--cpus=1` 不是这条竞态的放大器，是抑制器** —— 这一格是本拍的一个**否证**：
只有 1 个 CPU 时同时真正在跑的线程更少，「我的写窗口」与「别人的 fork 窗口」**更难重叠**。
⇒ 想放大这条竞态，要的是**宿主级超卖**（很多可运行线程真并发），不是容器配额。

🔴 **我拒绝造一次 16 核满载来把复现率打上去**：本波还有 12 道 agent 在**同一台机器**上跑门禁，
那会把别人的判决弄红，而那些红会被记到**别人的账**上。
⇒ 代价如实写：**这条轴上我的受控放大器是失败的**，高负载那一半的读数只能借 PM 12 棵树那批。

---

## 四 · 修法：把那一个读数拆成三个

代码住 `src-tauri/src/launch.rs` 的**测试段**（生产段一个字节没动）。

| 坏法 | 现在红的是哪一句 |
|---|---|
| ① **前提没建立**（上限内一直 `ETXTBSY`） | `前提不成立：exec 那一刻一直有人握着这个假终端的写 fd …… 这条今天判不了，它不是「开窗那一支的 spawn 坏了」` |
| ② **spawn 那一半真坏了**（别的错） | `★★ 开窗那一支的 spawn 真的失败了（不是 ETXTBSY，所以不是前提问题）` |
| ③ **夹具没装好**（没有执行位） | `假终端没有执行位（mode=…）—— 红的是夹具，不是被测那一半` |

三样落地：

- **建立**：`install_fake_terminal` —— 具名写句柄 + `sync_all` + **显式 drop**。
  改之前是 `std::fs::write`（它也关 fd，但那是**实现细节**：读的人看不出「关写 fd」
  是这条判据的前提之一）。**本件要买的正是「让前提看得见」。**
- **检查 + 出声**：`spawn_fake_terminal` —— **只对 `ETXTBSY`** 有上限地重试（50 × 20ms ≈ 1s），
  每次重试**出声**；上限到了**照样红**，只是红的那句话换成「前提不成立」。
  **别的错一次都不重试**（重试一个真缺陷 = 把它变成偶尔绿的偶发红，比今天更糟）。
- **分类器认 `os error 26`，不认 `Text file busy`**：后半句由 C 库按 `LC_MESSAGES` 打
  （glibc 有中文翻译），拿它当判据等于**再挂一条隐式的 locale 前提** —— 那正是本件在治的病，
  而 `D6` 那一册量的就是那条轴。前半句是 errno 的十进制，与 locale 无关。

**三条不许做的，逐条对账**：不 `#[ignore]`（判据仍在跑、仍在断言 argv 逐格相等）·
不 sleep 掩盖（重试有上限，到点红，且红得说得清是哪种坏）· 判据一条没删（净 **+1** 条）。

---

## 五 · 死值验 · 四刀（口径 `--network none`，量于 `9d5b67e`，跑法 `cargo test -p monitor --lib launch::tests::`）

| # | 刀 | 锚点 · 命中数 | 读数 |
|---|---|---|---|
| **0** | 不变异（对照） | — | **ok 34 passed / 0 failed** |
| **A** | 分类器**恒答「不是 ETXTBSY」**：`spawn_error_is_etxtbsy` 的体 → `false` | `fn spawn_error_is_etxtbsy` 命中 **1** | **FAILED 1**，正是新判据，逐字 `★★ 有人攥着写 fd 时 execve 必是 ETXTBSY，而这条判据把它读成了 Broken("spawn 本地命令失败: Text file busy (os error 26)")` |
| **B** | 分类器**恒答「是 ETXTBSY」**：同一个体 → `true` | 同上 **1** | **FAILED 1**，同一条判据但**换了腿**，逐字 `★★ 一个没有执行位的终端是真失败，不是「前提不成立」，而这条判据把它读成了 PremiseUnmet("撞了 2 次，最后一次逐字：spawn 本地命令失败: Permission denied (os error 13)")` |
| **C**（`7u`） | **把修法那一跳整个退掉**：调用点退回 `launch_local_posix_via(…).expect("spawn 假终端应成功")`，helper 与新判据**原样留着** | `expect("spawn 假终端应成功")` 命中 **1** · `spawn_fake_terminal(` 命中 **4**（定义 1 + 判据 3） | 🔴 **ok 34 passed / 0 failed —— 一条都没红** |

★★ **A 与 B 摆在一起才说得出买到了什么**：分类器**两侧都有牙**。
只报 A 是证不出这一格的 —— 一个「一律认成前提问题」的退化分类器在 A 上不红，
它的后果是**把真缺陷说成「今天判不了」并且重试它**，那比今天更糟。**B 就是那一格。**

🔴🔴 **刀 C 是本册最该被读到的那个读数（我自己的射程边界）**：
**新判据的牙全在分类器上，一点都不在「真判据那一处到底有没有用它」上。**
把调用点退回 `.expect(…)`，全表**一条不红** —— 那一跳今天**没有任何行为判据**。
成因是结构性的：要行为地驱动它，就得**在判据里制造那条 fork 竞态**，而它是概率性的
⇒ 一条「必须撞上竞态才会红」的判据本身就是偶发红。
⚠ **可走的一条路，但它不是本拍能自批的**：加一条**文本**判据（形状照本文件里
`the_thin_wrapper_hands_the_command_straight_through_to_the_via_form` 那条 —— 它就是本文件
既有的先例），钉住「本文件测试段里凡是把一个**自己刚写出来的文件**当 `term` 交出去的调用，
都必须走 `spawn_fake_terminal`」。**值不值得由 PM 裁**（它是一条文本判据，
而本仓对文本判据的态度是「如实登记，不假装钉住了行为」）。

四刀跑完**均已完全复位**（`git status --short src-tauri/src/launch.rs` 空，
交付物里不含任何变异）。

---

## 六 · 人群：全仓还有几条「写出一个可执行文件，然后 exec 它」

**尺子怎么切的**（两个条件都承重）：
① 那个可执行文件是**这一趟自己写出来的**（否则没人在写它）；
② 它随后**真的被 exec**（只 `stat` / 只查 `PATH` 的**不算** —— 那条路不经过 execve）。

🔴 **刻意不切成「写了文件 + chmod 了执行位」** —— 那是**候选面**，不是人群：
实测**有 4 处满足前者而不满足②**。把它当人群会多报 4 条，
并把修法引到「别 chmod」而不是「把前提建起来」。

**分母两级，别混着报**：
- **候选面** = 三个 Rust 根（`src-tauri/src` · `src-tauri/crates` · `remote-daemon-proto/src`，
  **187** 份 `.rs`，去掉 vendor / target）里「给一个路径加执行位」的处数 = **10**（机器数的）。
- **人群** = 候选面里②也成立的那几处 = **5**（**逐处读代码判的，不是机检读数**）。

| # | 住址 | ② 成立？ | 判据 / 依据 | 归谁 |
|---|---|---|---|---|
| 1 | `src-tauri/src/launch.rs`（`install_fake_terminal`） | ✅ | 本进程直接 execve 它 —— **就是红过的那一条** | **本件，已治** |
| 2 | `remote-daemon-proto/src/observe/watcher.rs`（`tmux_server_query_yields_nothing_without_a_server`） | ✅ | 写一个假 `tmux`，然后起 `/bin/sh -c '… exec tmux …'` + `PATH` 指过去 ⇒ **由子 shell exec** | `K-R12` 在改这个文件，**只登记** |
| 3 | `remote-daemon-proto/src/observe/watcher.rs`（那个 `write_fake` 闭包，四格反复重写同一个文件再起） | ✅ | 同上，而且**同一个 inode 被反复重写** ⇒ 窗口更多 | `K-R12`，**只登记** |
| 4 | `src-tauri/src/backend/control/local_backend.rs`（`e2e_a_binary_that_always_dies_is_given_up_on_within_the_cap`） | ✅ | 写 `always-dies.sh` + chmod，然后 `supervise` 真起它 | 写区外，**只登记**（且它 `#[ignore]`，门禁 cargo 那格今天不跑它） |
| 5 | 🔴 **生产侧**：`src-tauri/src/backend/control/local_backend.rs::extract_embedded_to` → `src-tauri/src/platform_fs.rs::make_executable` → `src-tauri/src/local_daemon.rs::spawn_detached` / `supervise_with_stdio` | ✅ | `fs::write(&tmp, bytes)` → `make_executable(&tmp)` → `rename(&tmp, &dest)`（**rename 不换 inode**）→ 调用方 exec `dest` | 写区外，**只登记 —— 见下面那一格，这是本册最贵的一条** |
| 6 | `remote-daemon-proto/src/main.rs`（两处「给合成 tmux 上执行位」） | ❌ | 之后走的是 `tmux_in` / `tmux_present` → `plugin::discover::is_executable`（**只读 metadata**） | 射程外 |
| 7 | `remote-daemon-proto/src/plugin/discover.rs`（`the_first_candidate_that_exists_wins`） | ❌ | 之后只 `find(…)` → `is_executable` | 射程外 |
| 8 | `src-tauri/src/cc_bus_deploy.rs`（两处 `0o555` / `0o755`） | ❌ | 那是一个**目录**的权限（造「不可写」夹具再恢复），不是可执行文件 | 射程外 |
| 9 | `src-tauri/src/platform_fs.rs::make_executable`（`0o700` 本体） | —（它是**原语**） | 它自己不写也不 exec；它的**调用方**在第 5 行里 | 记在第 5 行 |

### 🔴 第 5 行单独说：**同一条没人建立的前提，在生产上也有一份，而且会给用户看**

`extract_embedded_to` 的次序是 `fs::write(&tmp, …)` → `make_executable(&tmp)` → `rename(&tmp, &dest)`，
而 `rename` **不换 inode** ⇒ 随后 exec `dest` 时，只要 monitor 里**别的线程**在
`fs::write` 那个窗口里 fork 过一次（monitor 生产段现打 **42** 处起进程的调用点：
`ccm_probe` · `cc_bus` · `account_usage` · `tmux` · `search` …），
那一次 exec 就会 `ETXTBSY` ⇒ **用户看到的是「本地后端起不来: Text file busy」**，
而那句话**说不出**是「二进制坏了」还是「刚好撞上一次竞态」。
★ **这不是我造的假想面**：它与 `launch.rs` 那条红过的判据是**同一条前提的两个住址**，
只不过一个住在测试段（会红给我们看），一个住在生产段（会红给用户看）。

⚠ **它在本树上今天量不到**：本树**未铺** `src-tauri/embedded-daemons/` ⇒ `embedded_daemons`
那个 cfg 不置 ⇒ 「本地后端真的能起来吗」那族 4 条判据**这一格根本没跑**（门禁自己的分母行
逐字写着这一条）。**⇒ 我给的是「读代码判的一条攻击面 + 它的必要条件」，不是一个实测读数。**

**建议的修法形状（交 PM，写区外一个字节没改）**：在 exec 那一处（不是在写那一处）
**对 `os error 26` 做有上限的重试并出声**，与本拍 `spawn_fake_terminal` 同形；
上限到了**换一句话报**（「刚才那次释放出来的二进制被别的进程开着写 —— 重试 N 次仍然如此」），
**不要**与「二进制坏了 / 找不到」共用一个读数。
⚠ 只加 `sync_all` / 显式关句柄**买不到这一格**（本进程的句柄本来就关了，
破前提的是**别的线程的 fork**）—— 这一条我在本拍的判据上实测过。
