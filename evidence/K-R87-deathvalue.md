# K-R87 死值验原文

本件的三条 dod 各自的刀、逐刀的**完整原文读数**住这里；件文件 `§3` 只放摘要。

## 规矩（每一刀都要满足，缺一条这份证据不算数）

- **全部在沙箱里跑**：`PB_WS=backend-consolidation .claude/devbox/gate <本工作树绝对路径> k-r87`。
  **不许直接在本机跑**（`K31` / `R15`）。
- **一趟一刀**：下一刀之前先把上一刀还原干净，`git status --porcelain` 贴出来自证盘上没留变异。
- 🔴 **还原不许用 `cp -a`** —— 它连 mtime 一起还原，`cargo` 会判「没变」而沿用上一刀的产物，
  于是你会读到**上一刀的红**。用 `cp` ＋ `touch`。
- **点名**：每一刀要写清「**哪一条判据红了**」，不是只写「红了」。
  阴性对照（本该绿的那一刀）同样要贴读数。

## 量具住址（本轮的，只属于本件）

- 刀本体：`/tmp/claude-1000/-home-zbl----claudecode-frontend/f80a7aea-0ccf-4854-9b7a-12961a9c55aa/scratchpad/kr87/`
  下的 `cut.sh`（跑一趟：按变异脚本第 2 行的 `# FILES:` 备份 → 变异 → 打印「变异已落地」的 diff →
  沙箱门禁 → `cp` 还原 ＋ `touch` → 打 `git status --porcelain`）与 `mut_M*.py` / `mut_7u.py`
  （逐刀一份，每份开头 `assert s.count(old) == 1` = **锚点恰好命中 1 次**，命中不到 1 次脚本直接失败、刀不落地）。
  ⚠ 那个目录是**会话私有**的临时目录，不随仓走；**被测对象恒是**
  `/home/zbl/文档/claudecode-frontend/.claude/worktrees/k-r87`（分支 `track/k-r87`）。
- 门禁：`PB_WS=backend-consolidation .claude/devbox/gate <上面那棵树> k-r87`，
  target 目录 `.claude/pm-targets/k-r87`。

## `M0` 基线 —— **本轮自己现打的，没抄 PM 给的数**

**`GATE: OK —— 13 格全绿`**，量于 **`995724c`**（`track/k-r87` 的基点）＋ 盘上两份**空骨架**
（`control/oneshot_session.rs` 只有头注、`evidence/K-R87-deathvalue.md` 是 PM 建的空壳），
本轮一行实现代码都还没写。⚠ `node_modules` 软链**已铺**（`.gitignore` 盖着它、不是写区项；
不铺 `npm` 那一格必红 `tsx: not found`）。

| 格 | 读数 |
|---|---|
| hooks | 11 passed |
| fmt | 1 passed（绿/红两态） |
| fmt-daemon | 1 passed（绿/红两态） |
| winchk | 1 passed（绿/红两态） |
| cargo | **1595 passed**（9 个包合计） |
| generated | 与 Rust 源一致 |
| daemon | **728 passed**（单包 `remote-daemon-proto`） |
| npm | **1714 passed** |
| ccm e2e ×4 | print-parity **12** · rbind-title **8** · cli **46** · contract-parity **45** |
| pb check | FAIL=0 BROKEN=0 |

⚠ **分母**：本树**未铺** `src-tauri/embedded-daemons/` ⇒ `embedded_daemons` cfg 不置
⇒ 上面那个 cargo 合计里**少了「本地后端真的能起来吗」那一族（4 条）**。门禁自己每趟都印这一行。

## `M0.5` —— **第一趟实现跑出来是红的，如实记**

**`GATE: FAIL —— cargo（退出码 101）`**，monitor 侧 `1462 passed / 1 failed`。
红的是 **`byte_cap_registry::tests::every_byte_cap_says_what_it_bounds_and_what_happens_past_it`**，
报错逐字点名：`remote-daemon-proto/src/control/oneshot_session.rs  MAX_TTL_SECS = Some(86400)`。

🔴 **这条尺子 PM 的 `§0e` 没给、我事前的普查也没列出来** —— 它住 **monitor 那棵树**
（`src-tauri/src/byte_cap_registry.rs`），而人群含 `remote-daemon-proto/src` 的生产段，
收人判据是「`const` 名字里有 `MAX`/`CAP`/`LIMIT`/`BYTES` ＋ 类型 ∈ `u64/usize/u32`
＋ 值 ≥ `SMALLEST_PLAUSIBLE_SIZE`(1024)」。⇒ 一个**秒数**上界因为叫 `MAX_TTL_SECS`、
值是 `86_400` 就进了「字节上限」的人群。

**处置：重新裁定，把那条上界撤掉**（`MIN_TTL_SECS` 留着 —— `0` 秒的看门狗等于没有看门狗，
那是**形状**）。理由不是「为了让门禁绿」：回去重读那条上界，它**说不出一句机制**
——「多久算太久」是**偏好**，而 `K37` 逐字裁了「后端只给机制，不给偏好」。

⚠ **另外两条路都试过、都不走**，写下来免得下一个人再走一遍：
- **改名躲开**（`TTL_SECS_CEILING` 之类）：同一份登记的**默认拒绝**那条
  （`every_size_typed_constant_is_either_a_cap_or_registered_as_not_one`）会接住它 ——
  它扫「所有 ≥1024 的尺寸类 const」，名字不在关键词里且不在 `NOT_A_SIZE_CAP` 里就红。
  **这条判据造得好，躲不掉。**
- **补登记**（往 `byte_cap_registry::NOT_A_SIZE_CAP` 加一行「它量的是时间（秒），不是体量」）：
  那是**语义上最对**的一条，形状与表里现有的 `CHANNEL_CAPACITY` / `LIMIT_MAX` 逐字同族。
  🔴 **但 `src-tauri/src/byte_cap_registry.rs` 不在本件写区** ⇒ 本轮不走它。
  **PM 若认为上界该留，这就是那一行；请裁 + 补登记。**

## 逐刀

### `M1` · `KR87D1` 刀①：**把看门狗那一句整个摘掉**（⇒ 该红）

- 刀：`mut_M1.py`，锚点 = `start_on` 里那一行 `if let Err(…) = spawn_watchdog(…)`，
  **命中恰好 1 次**（脚本 `assert` 住）。变异已落地的 diff（原文）：

  ```
  403c403,405
  <     if let Err((code, why)) = spawn_watchdog(launcher, socket, ttl_secs, &handle) {
  ---
  >     let _ = (launcher, &handle);
  >     if false {
  >         let (code, why): (&'static str, String) = ("watchdog_failed", String::new());
  ```

  ⇒ 形状全留着（回滚那一段仍在，类型契约不变），只是**看门狗一次都不起** ——
  「形状对、恒答其中一张脸」（`brief` 第 7 条）。
- 读数：**`GATE: FAIL —— daemon（退出码 101）`**；daemon **740 passed / 2 failed**（基线 742/0）。
- **红的是哪两条**（点名）：
  1. 🔴 `control::oneshot_session::tests::the_session_is_gone_at_its_deadline_while_its_neighbour_stays`
     —— 报错逐字：「**过了 1 秒那个一次性会话还在 —— 看门狗没起作用（或根本没起来）**」。
     **`KR87D1` 刀① 逐字要求的那一格在这一条上兑现。**
  2. `control::oneshot_session::tests::a_watchdog_that_cannot_start_is_never_reported_as_success`
     —— 报错逐字：「看门狗起不来时不许回成功 —— 那就是「凭空返回一个成功值」:
     `Oneshot { name: "ccm-oneshot-nodog", handle: "$0", ttl_secs: 60 }`」。
     ⚠ 它跟着红是**对的**：这一刀连「起不来」这件事都不去问了。
- 其余 12 格全绿（cargo 1595 · npm 1714 · e2e 12/8/46/45 · pb check FAIL=0）。
- ⚠ **这一刀的射程比 `KR87D1` 大一格**（它同时打掉了 `KR87D3`）⇒ 下面 `M2` 是它的**最小面**版本。

### `M2` · `KR87D1` 刀①-最小面：**看门狗照起，只是期限被换到很远**（⇒ 该红，且只该红一条）

- 刀：`mut_M2.py`，锚点 = 同一行里那个 `ttl_secs` 实参，**命中恰好 1 次**。
  变异已落地的 diff（原文）：

  ```
  403c403
  <     if let Err((code, why)) = spawn_watchdog(launcher, socket, ttl_secs, &handle) {
  ---
  >     if let Err((code, why)) = spawn_watchdog(launcher, socket, 3_600, &handle) {
  ```

- 读数：**`GATE: FAIL —— daemon（退出码 101）`**；daemon **741 passed / 1 failed**
  —— **最小面：只红了该红的那一条。**
- **红的是哪一条**（点名）：`control::oneshot_session::tests::the_session_is_gone_at_its_deadline_while_its_neighbour_stays`，
  报错逐字：「过了 1 秒那个一次性会话还在 —— 看门狗没起作用（或根本没起来）」。
- ★ 这一刀值钱的地方：看门狗**真的起来了**（`KR87D3` 那条照常绿），起进程登记、argv 逐元素、
  名字空间那几条也全绿 —— 唯一变的是「**它到点没到点**」。
  ⇒ 这条判据钉的确实是**那个会话到点真的不在了**，不是「串里有 `setsid` / `sleep` 字面量」
  （件文件 `§1` 逐字点名的那个失效方向）。

### `M3` · `KR87D1` 刀③：**把「等」搬进 daemon 自己的代码**（⇒ `no_timer_guard` 必须红）

- 刀：`mut_M3.py`，锚点 = `spawn_watchdog` 里 `Command::new(launcher)` 那两行，**命中恰好 1 次**；
  在它前面插一行 `std::thread::sleep(std::time::Duration::from_millis(1));`。
  ⚠ **刻意只插 1 毫秒**：行为几乎没变（看门狗照起、会话照死），这样红的就只可能是**铁律那一族**。
- 读数：**`GATE: FAIL —— daemon（退出码 101）`**；daemon **739 passed / 3 failed**。
- **红的是哪三条**（点名）：
  1. 🔴 `no_timer_guard::tests::daemon_production_code_has_no_periodic_wakeups`
     —— 报错逐字：「零定时器护栏违规（P6，**按调用形态**逮到）：生产代码
     `control/oneshot_session.rs` 里有 `sleep(` 调用」。
     **`KR87D1` 刀③ 逐字要求的「`no_timer_guard` 必须红」在这一条上兑现。**
  2. `no_timer_guard::tests::every_duration_use_is_registered_as_non_timer`
     —— 逐字：「生产段 `Duration::from_*` 有 **5** 处，登记表里只有 **4** 条：
     `[("control/oneshot_session.rs", 1), ("dial/mod.rs", 1), ("observe/watcher.rs", 1),
     ("relay/server.rs", 1), ("relay/upstream.rs", 1)]`」。
  3. `no_timer_guard::g6_reach::the_counterexample_is_still_on_the_board_and_still_passes`
     —— 逐字：「禁用构件在人群里出现了：`control/oneshot_session.rs: thread::sleep`」。
- ⚠ **本件新立的那条判据（`the_oneshot_watchdog_script_carries_no_loop`）没有跟着红，而那是对的**：
  这一刀加的是**Rust 侧的 sleep**，不是往看门狗那条 shell 脚本里加循环。两件事，两条判据，各管各的。
- ⇒ **`§0b` / `§0d` 判对了**：外部进程那一形合法（本件收官那趟全绿），
  搬进本 crate 当场撞铁律（这一刀）。**两侧都实测过，不是推的。**

### `M4` · `KR87D2` 刀①：**撞名时静默接回既有会话**（⇒ 该红）

- 刀：`mut_M4.py`，锚点 = `new_session_argv` 的头四行，**命中恰好 1 次**；
  给 `new-session` 加一个 `-A`（tmux 自己那条「在就复用」的旗，数组长度 8 → 9，类型契约照旧）。
  ⇒ 撞名时 tmux **回退出码 0 ＋ 既有会话的句柄**，本模块那条 `name_taken` 分支**根本走不到**
  —— 这正是 `control/launch.rs` 的 `create-or-attach` 今天的行为，也正是本件不许有的那一下。

- 读数：**`GATE: OK —— 13 格全绿`**；daemon **742 passed / 0 failed** ——
  🔴 **这一刀活下来了，而它不是判据的洞** ⇒ **这一刀不算一次读数**，下面 `M4b` 是重切的。
- **为什么它不算**（现打，不是推的）：在沙箱里对真 tmux 3.4 打了三条 ——

  ```
  $ tmux -S $S -f /dev/null new-session -d -s ccm-oneshot-taken     # 建占位
  $ tmux -S $S -u new-session    -d -P -F '#{session_id}' -s ccm-oneshot-taken
  duplicate session: ccm-oneshot-taken            ; rc=1
  $ tmux -S $S -u new-session -A -d -P -F '#{session_id}' -s ccm-oneshot-taken
  open terminal failed: not a terminal            ; rc=1
  $ tmux -V
  tmux 3.4
  ```

  ⇒ tmux 3.4 上 `new-session -A` 撞到既有会话时**走的是 attach 那条路**（`-d` 在那儿的意思
  变成「把别的客户端踢下来」，不是「别 attach」）⇒ 在 daemon 这种**没有终端**的进程里
  它 **rc=1**。于是本模块的 `has-session` 兜底照常判出撞名、照常回 `name_taken`
  —— **变异落地了，语义没落地**。这正是 `brief` 第 7 条说的那一类：
  「我换上去的这个，它的类型契约还成立吗」—— 类型成立了，**语义没成立**。
- ⚙ **顺带一条对产品有用的读数，留在这里**：「顺手加个 `-A` 就能复用」这条捷径
  在**无终端**的 daemon 里**根本走不通**。下一个想给一次性会话加幂等的人省一趟。

### `M4b` · `KR87D2` 刀①（重切，忠实版）：**撞名时真的把既有会话接回来**（⇒ 该红）

- 刀：`mut_M4b.py`，锚点 = `create_session` 里 `name_taken` 那一支的头两行，**命中恰好 1 次**；
  换成「拿 `display-message -p '#{session_id}'` 问出**既有会话**的句柄，当自己刚建的那个返回」。
  ⇒ 这才是「静默接回」的真实形状：调用方拿到 `Ok`，而看门狗被挂到了**别人的会话**上。
- 读数：**`GATE: FAIL —— fmt-daemon（退出码 1）；daemon（退出码 101）`**；
  daemon **741 passed / 1 failed** —— **最小面：只红了该红的那一条。**
  ⚠ `fmt-daemon` 那一格红是**变异脚本插进去的代码没过 `cargo fmt`**，是刀的产物、不是读数。
- **红的是哪一条**（点名）：`control::oneshot_session::tests::a_taken_name_is_refused_and_the_other_session_is_left_untouched`，
  报错逐字：「**撞名不许回成功 —— 那就是静默接回**:
  `Oneshot { name: "ccm-oneshot-taken", handle: "$0", ttl_secs: 1 }`」。
  ⇒ 注意那个 `handle: "$0"` —— 它是**占位会话**的句柄，也就是说这条实现真的把**别人的会话**
  当成了自己的。
- ⚠ **这一刀停在第一格上**：`expect_err` 一失败，后面那两格（「别人的会话还在」「过了 ttl 它还在」）
  就没跑到。**那两格的牙本轮没有单独实测过** —— 如实登记，别读成「三格都验过了」。

### `M5` · `KR87D3` 刀①：**看门狗起不来也照回「成功」**（⇒ 该红，且只该红一条）

- 刀：`mut_M5.py`，锚点 = `start_on` 里那一行 `if let Err(…) = spawn_watchdog(…)`，
  **命中恰好 1 次**；换成 `let _ = spawn_watchdog(…);` ＋ `if false {`
  ⇒ 看门狗**照起**（有 `setsid` 的时候一切照旧），只是**它起不来的时候没人管**。
- 读数：**`GATE: FAIL —— daemon（退出码 101）`**；daemon **741 passed / 1 failed**
  —— **最小面。**
- **红的是哪一条**（点名）：`control::oneshot_session::tests::a_watchdog_that_cannot_start_is_never_reported_as_success`，
  报错逐字：「看门狗起不来时不许回成功 —— 那就是「凭空返回一个成功值」:
  `Oneshot { name: "ccm-oneshot-nodog", handle: "$0", ttl_secs: 60 }`」。
- ★ 与 `M1` 的对照值钱：`M1`（看门狗**一次都不起**）同时打红了 `KR87D1` 与 `KR87D3` 两条；
  `M5`（起，但不管它起没起来）**只打红 `KR87D3`** ——
  `control::oneshot_session::tests::the_session_is_gone_at_its_deadline_while_its_neighbour_stays`
  在这一趟里是**绿**的。⇒ 两条 dod 的判据**真的各管各的**，不是同一条断言的两个名字。

### 🔴 量具自己出过一次错 —— 写在这里，因为它污染过一趟读数

`cut.sh` 第一版用 `sed -n '2s/^# FILES: //p' "$MUT"` 只认**第 2 行**的 `# FILES:`。
而 `mut_M4b.py` 把那一行写在**第 4 行**（前面三行是刀的说明）⇒ `FILES` **是空串**
⇒ 备份那一圈与还原那一圈**都静默跳过**（`for f in ; do … done` 是合法的空循环）。

后果：`M4b` 的变异**没有被还原**，紧接着的 `7u` **叠在它上面跑**。
逮到它的是 `7u` 那一趟的 `fmt-daemon` 诊断 —— 里面赫然印着 `M4b` 的 `display-message` 那几行。

处置（四步，逐步自证）：

1. 那份污染的读数**作废并改名留档**：`mut_7u-contaminated.log` / `cuts-run2-contaminated.txt`。
   **本文件里所有 `7u` 的数都是重跑那一趟（`cuts-run3.txt`）的。**
2. **逐处掏空** `M4b` 的残留（一段精确的逆替换，`assert count == 1`）——
   🔴 **没有用 `git checkout <base> -- <file>`**（那是派工单的红线）。
   掏完 `git status --porcelain` **输出为空**。
3. `cut.sh` 改成**全文件**搜 `# FILES:`，并**取不到时拒跑**（`exit 3`）——
   静默跳过还原这件事从此不可能再发生。
4. `7u` 在干净树上重跑。

★ 这一条本身就是 `brief` 第 12 条那一族的活体：**量具的作用域对不上事实**
（我以为它在备份，它其实在跑空循环），而**两者在终端上长得一模一样**。

### `7u` · **把实现整个退掉，还有多少条新断言仍绿**

- 刀：`mut_7u.py`，**逐处掏空、签名留着**（🔴 不是整份回滚），**七处**、每处锚点各**命中恰好 1 次**：
  ① `mint_name` 恒 `Ok(slug.to_string())`（不加前缀、不做形状校验）；
  ② `checked_ttl` 恒 `Ok(MIN_TTL_SECS)`；
  ③ `checked_handle` 恒原样收；
  ④ `is_oneshot_name` 恒 `true`；
  ⑤ `watchdog_args` 恒 `Vec::new()`（形状仍是 `Vec<String>`）；
  ⑥ `create_session` 恒 `Ok(String::new())`（不起进程、不判撞名）；
  ⑦ `spawn_watchdog` 恒 `Ok(())`（不起进程）。
- 读数：**`GATE: FAIL —— fmt-daemon（退出码 1）；daemon（退出码 101）`**；
  daemon **730 passed / 12 failed** ⇒ 本件新增的 **14** 条断言里 **12 条红 · 2 条仍绿**。
  （新增条数的分母：收官那一趟 daemon `742` − 基线 `728` = **14**。）
  ⚠ `fmt-daemon` 那一格是掏空脚本插进去的代码没过 `cargo fmt`，是刀的产物、不是读数。
- ⚠ **既有判据也红了两条，那是这一刀的副作用，不算进上面的分母**：
  ⑥⑦ 把两处 `Command::new(` 都掏掉了 ⇒ `readonly_guard::spawn_registry::every_process_spawn_in_production_is_registered`
  的相等断言（13）与 `the_registry_has_no_ghost_entries` 会红。
  ⚙ 顺带是一格**正面读数**：那张登记表真的在数「盘上还有没有这两处起进程」。
- **仍绿的 2 条，逐条给理由 ＋ 它的牙在哪一刀（都已实测，不是推的）**：

| 仍绿的那一条 | 为什么这一刀咬不到它 | 它的牙在哪一刀（**已实测**） |
|---|---|---|
| `no_timer_guard::f09_external_beat::the_oneshot_watchdog_script_carries_no_loop` | 它断的是**那条常量脚本的文本**里没有循环关键字。掏空只**减**东西，常量原封不动 ⇒ 本来就该绿 | **`M6`**：把 `WATCHDOG_SCRIPT` 换成 `while :; do sleep "$1"; tmux "${@:2}"; done` ⇒ 当场红，报错逐字「看门狗脚本里出现了循环关键字 `while` —— 那就从「到点一次」变成了「靠外部 shell 提供节拍」」 |
| `stream_flag_tests::the_oneshot_session_subcommand_is_actually_reachable` | 它断的是 `main.rs` 的**闸门**与**分派臂**，而掏空只动 `control/oneshot_session.rs` 一份文件 | **`M7`**：删掉 `main.rs` 那一行分派臂 ⇒ 当场红 |

⇒ **真空真 0 条**：两条各自都有一刀实测过它会红。

### `M6` · `7u` 仍绿者①的牙：**往看门狗那条脚本里塞一个循环**（⇒ 该红）

- 刀：`mut_M6.py`，锚点 = `WATCHDOG_SCRIPT` 那一行常量声明，**命中恰好 1 次**；
  `sleep "$1"; shift; exec tmux "$@"` → `while :; do sleep "$1"; tmux "${@:2}"; done`。
- 读数：**`GATE: FAIL —— daemon（退出码 101）`**；daemon **739 passed / 3 failed**。
- **红的是哪三条**（点名）：
  1. 🔴 `no_timer_guard::f09_external_beat::the_oneshot_watchdog_script_carries_no_loop`
     （**本件新立的那条**），报错逐字：「看门狗脚本里出现了循环关键字 `while` ——
     那就从「到点一次」变成了「靠外部 shell 提供节拍」，也就是 C12 的 ⚠ 点名的那一形。
     实得：`"while :; do sleep \"$1\"; tmux \"${@:2}\"; done"`」；
  2. `control::oneshot_session::tests::the_watchdog_hands_the_shell_a_constant_script_and_positional_arguments`
     （argv 逐元素那条）；
  3. `control::oneshot_session::tests::the_session_is_gone_at_its_deadline_while_its_neighbour_stays`
     —— **顺带一格真读数**：`${@:2}` 是 bash 语法，`sh` 里跑不动 ⇒ 那个会话到点真的没死。

### `M7` · `7u` 仍绿者②的牙：**把 `main.rs` 那条分派臂删掉**（⇒ 该红，且只该红一条）

- 刀：`mut_M7.py`，锚点 = `main.rs` 那一行单行分派臂，**命中恰好 1 次**（整行删掉）。
- 读数：**`GATE: FAIL —— daemon（退出码 101）`**；daemon **741 passed / 1 failed** —— **最小面。**
- **红的是哪一条**（点名）：`stream_flag_tests::the_oneshot_session_subcommand_is_actually_reachable`，
  报错逐字：「一次性查询的分派里没有那条把 `--oneshot-session` 交给原语本体的臂：
  没有任何一行 trim 之后等于 `Some("--oneshot-session") => control::oneshot_session::run(&args),`。」
- ⚠ 与 `K-R86` 那一轮同形：`argv_table_guard::every_listed_subcommand_is_actually_dispatched`
  （名字里写着「真的被分派」）**没有跟着红** —— 它的人群是「生产段里出现过那个 `--token`
  **字面量**」，而 `SUBCOMMANDS` 那张表自己就住在 `main.rs` 的生产段里 ⇒ 臂删了、字面量还在。
  🔴 **本轮独立复现了 `K-R86` `§8c` 报的那条既有盲区**（那一轮是在 `--capture-pane` 上逮到的）。
  本轮同样没动它（改它要重判它整个人群）⇒ 进 `§8`。

### 收工前自查那一刀（`brief` 纪律 15）：**判据的作用域对不上它声称守的面**

拿本轮头号病理（`§0c` 按顶层文件名去数一张住在**模块**里的登记表 · `cut.sh` 按**第 2 行**去找
`# FILES:`）回打自己写的判据，逮到同一形长在
`the_watchdog_runs_the_same_kill_command_the_rollback_runs` 上：
第一版只比「看门狗那条 argv 的尾巴 == `kill_argv`」——**证的是看门狗那一侧**，
而**回滚**那一侧（`kill_handle`）没有任何东西在证它也走同一份。

- 刀：把 `kill_handle` 里的 `.args(kill_argv(handle))` 换成**手抄的、行为完全相同的**
  `.args([UTF8_CLIENT_FLAG, "kill-session", "-t", handle])`。
- 补之前：**照样绿**（判据的标题写着「是同一条」，而它证不到）。
- 补之后现打：**741 passed / 1 failed**，报错逐字把 `kill_handle` 整段函数体印出来
  （「回滚那一处没走 `kill_argv` —— 它和看门狗从此各写一份 argv，
  而「到点杀的是别的东西」这种错只在到点那一刻现形」）。
- ⚠ **量法的射程如实写**：这一刀跑的是**沙箱里的 `cargo test`（单包 `remote-daemon-proto`）**，
  **不是整趟 13 格门禁** —— 那条判据整个住在 daemon 这一棵，别的 12 格与它无关。
  三条 dod 的死值验（`M1`…`M7` ＋ `7u`）**每刀都是整趟门禁**。

⚠ 补这一段的时候**踩了两次同一个坑，逐字记着**：`readonly_guard` 的剥法靠**大括号配平**找
测试模块边界，而**注释或字符串里一个落单的右大括号**会让它提前收尾 ——
「剥完仍有测试属性残留在生产段里」当场红（`[("oneshot_session.rs", 9)]`）。
第一次是切法本身用了大括号，第二次是**解释这件事的那句注释自己写了一个落单的右大括号**。

## 收官那一趟（阴性对照 ＝ `KR87D1` 刀② / `KR87D2` 刀② / `KR87D3` 刀②）

**`GATE: OK —— 13 格全绿`**（hooks · fmt · fmt-daemon · winchk · cargo · generated · daemon ·
npm · 四套 ccm e2e · pb check）。

| 格 | `M0` 基线（`995724c` ＋ 两份空骨架） | 收官 | 差 |
|---|---|---|---|
| hooks | 11 | 11 | 0 |
| fmt / fmt-daemon / winchk | 1 / 1 / 1 | 1 / 1 / 1 | 0 |
| cargo | 1595 | 1595 | **0** |
| generated | 一致 | 一致 | — |
| daemon | 728 | **742** | **+14** |
| npm | 1714 | 1714 | 0 |
| ccm e2e ×4 | 12 / 8 / 46 / 45 | 12 / 8 / 46 / 45 | 0 |
| pb check | FAIL=0 BROKEN=0 | FAIL=0 BROKEN=0 | — |

⚠ **cargo 差 0，那是对的**：本件**没有往 monitor 那棵树加一条断言**
（`src-tauri/src/parity_ledger.rs` 在写区里，但现打本件没有让它任何一句变假 —— 逐条量在件文件 `§8a` B8）。

**新增 14 条断言，逐条点名**（全在 daemon 那棵）：

- `control/oneshot_session.rs::tests`（**+12**）：
  `the_session_is_gone_at_its_deadline_while_its_neighbour_stays` ·
  `the_watchdog_hands_the_shell_a_constant_script_and_positional_arguments` ·
  `the_watchdog_runs_the_same_kill_command_the_rollback_runs` ·
  `a_taken_name_is_refused_and_the_other_session_is_left_untouched` ·
  `a_real_listing_separates_the_oneshot_namespace_from_user_sessions` ·
  `only_names_under_the_prefix_belong_to_the_oneshot_namespace` ·
  `a_slug_that_would_break_the_target_or_the_snapshot_never_reaches_tmux` ·
  `the_deadline_has_no_default_and_zero_is_refused` ·
  `a_watchdog_that_cannot_start_is_never_reported_as_success` ·
  `the_success_arm_really_carries_a_name_and_a_handle` ·
  `a_handle_that_is_not_a_session_id_is_refused_before_anything_is_killed` ·
  `the_cli_arm_refuses_the_wrong_number_of_positional_arguments`
- `no_timer_guard::f09_external_beat`（**+1**）：`the_oneshot_watchdog_script_carries_no_loop`
- `main.rs::stream_flag_tests`（**+1**）：`the_oneshot_session_subcommand_is_actually_reachable`

⚠ **量于哪个提交**：`track/k-r87` 上本件的**第三个**提交（`c634e47`，收工前自查那一拍之后）。

🔴 **收官那一趟里 `pb check` 红 1 条，成因是我自己，而修它的落点不是我 —— 如实记**：
`FAIL [J3 陈账] INDEX.md 比源文件旧 —— 重跑 `pb index` 落盘`。
成因：派工单要我把 `§3` / `§8` 写进件文件，而 `INDEX.md` 是**生成区**、由 `pb index` 落盘
—— **那是 PM 的落点**（`brief` 第 3 条「计划仓只写不提交」＋ 第 19 条「窗口开着期间不跑生成命令」；
`§2` 也把 `INDEX.md` 标成「PM 用」）。⇒ **我不跑它**，PM 收件时跑一趟即消。
⚙ **同一份代码在「计划仓还没被我写过」那一刻的完整读数是 13 格全绿**，量于 `384a561`
（实现那一拍；那一趟 `pb check FAIL=0 BROKEN=0`）。**代码那 12 格在每一趟里都是绿的。**

⚠ **本树未铺 `src-tauri/embedded-daemons/`**（门禁每趟自印那一行）⇒ cargo 那个合计里
**少了「本地后端真的能起来吗」那 4 条**。本件 bump 了 `BUILD_ID`（p2g → p2h），
**在铺了内嵌 daemon 的树上，bump 必须同拍 re-embed**，否则 `src-tauri/build.rs` 当场 panic
（那是它有意的：内嵌 id ≠ 源码 id ⇒ 装出去会被永远判 stale ⇒ 无限重装）。
本轮**没有** re-embed，归发版那一拍 —— 与 `p2d` / `p2e` / `p2g` 三次的登记逐字同一句。

## 逐刀之后：盘上没留变异（自证）

每一刀跑完都 `cp` 还原（**不是 `cp -a`**）＋ `touch`，随后 `git -C <工作树> status --porcelain`：

| 刀 | 还原自证 |
|---|---|
| `M1` · `M2` · `M3` · `M4` · `M5` | **空输出**（五趟逐趟核过） |
| `M4b` | 🔴 **没还原** —— 量具那个 bug（见上面那一节）。事后**逐处掏空**、自证空 |
| `7u`（重跑那趟） | **空输出** |
| `M6` · `M7` | **空输出** |

⚠ **这些空输出是有信息量的**：它们证明「还原干净」在本轮是**逐字节**成立的 ——
被改过的那两份源文件（`control/oneshot_session.rs` / `main.rs`）**一份都没有出现在里面**。
唯一一次没成立的是 `M4b`，而它**是被下一趟的诊断输出逮到的，不是被我发现的** ——
所以处置里第三步是「取不到 `# FILES:` 就拒跑」，把「靠人记得」换成「机器拦得住」。
