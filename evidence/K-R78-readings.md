# `K-R78` 逐刀读数 —— C 类第一拍（一次性部署动作搬进后端）

量于工作树 `.claude/worktrees/k-r78`，基点 `1d2b0e7`，**本树未铺 `src-tauri/embedded-daemons/`**
（编译期实证：`cargo` 每趟都印「缺少内嵌 daemon x86_64 / aarch64 …… `embedded_daemons` cfg 不置」）。
一切 `cargo` / `python3` 都在 `ccmon-devbox` 沙箱里跑（`K31`）。

量具两条腿：

- **真相源** —— `src-tauri/src/sftp_move_ledger.rs`（Rust 判据，从源码派生再逐格比对）。
- **第二条腿** —— `evidence/K-R78-dial-census.py`（同一把尺子的 python 实现，PM 不进沙箱也能复算）。
  两条腿今天同值；`.py` 头注写死了它与 `guard_core::production_code` 剥法不同的那一格。

---

## §0 一句话结论

- **`KR78D1` 没有达成。** 甲那一半**一个字节都没搬** —— 不是没做完，是**现打逮到四条挡路石，其中一条是用户拍的红线（铁律 I7）**，实现方不自批。逐条住址在 `§3`。
- **`KR78D2` 达成。** 乙那一半的「为什么还没搬」落成一张**有读数、由源码派生**的登记，三样过界物逐条点名、逐条带逐字校验位；死值验 5 刀。
- **`KR78D3` 达成，而且那一格**没动**（仍 6）。** 交代在 `§4`；另外新买到一条：**谁把那一格翻成「已搬」，判据当场红并印出界面里还剩几处拨号。**

---

## §1 现打普查 —— 🔴 派工单那句「9 处 / 4 文件」是错的

`connect_sftp` 的**生产段调用点**（剔掉定义行）：

| 文件 | 生产段字节 | 调用点 | 行号（09-12 现打，仅供复核，判据里一处裸行号都没留） | 类 |
|---|---|---|---|---|
| `sftp.rs` | 48 510 | **6** | 602 · 762 · 807 · 880 · 1091 · 1169 | 甲 4 ＋ **派工单没归类的 2** |
| `sftp_pool.rs` | 21 253 | **4** | 136 · 143 · 356 · 464 | 乙 |
| `mcp.rs` | 20 038 | **3** | 450 · 470 · 487 | 丙 |
| `acct_iso_deploy.rs` | 11 070 | **1** | 181 | 甲 |
| **合计** | | **14 处 / 4 份文件** | | |

差在哪：派工单那张分类表**自己加起来是 5 ＋ 4 ＋ 3 = 12**，而 `sftp.rs` 里另有 **2 处它一类都没归** ——
`ensure_daemon_deployed`（连接流程里的**自动**部署）与 `remove_remote_file`（F11 删远端会话 jsonl）。

⚠ **两把尺子的作用域不是一回事，别混**（本区最高频的那一类错）：

- 记分牌（`ssh_source::dial_move_judge::DIAL_SITES` ＋ `dial_home_registry` 那条棘轮）数的是
  **`connect_session(` 的调用点** ⇒ `sftp.rs` 在它那里只占 **1 处**（整份文件只有 `connect_sftp` 一个函数去握手）。
- 本表数的是 **`connect_sftp(` 的调用点** ⇒ **14 处**，那才是「有多少条路要改」。

**「6」与「14」都对，它们回答的是两个问题。**

---

## §2 `KR78D2` —— 乙为什么还没搬（三样过界物 ＋ 读数）

**读数**：`sftp_pool.rs` 今天 **11 个 `#[tauri::command]` / 4 处拨号**（生产段现打）。

| # | 要跟着过界的 | 逐字校验位（在 `sftp_pool.rs` 生产段） | 它挡的是什么 |
|---|---|---|---|
| ① | 进度通道 | `Channel<TransferProgress>`（命中 5 处） | 它是 **tauri 的 IPC channel**，`TransferProgress` 还带 `ts_rs::TS` 导出给前端。而 **daemon 那棵树根本没有 tauri 依赖**（现打：`remote-daemon-proto/Cargo.toml` 里 `tauri =` **0 处**，monitor 1 处）；monitor 侧 `backend/` 那道宿主无关守卫 `the_backend_layer_stays_host_agnostic` 的禁词表里已经有 `Emitter` / `State<` / `.emit(` 一族 |
| ② | per-origin 连接池 | `static POOL: std::sync::OnceLock<Mutex<HashMap<String, Slot>>>`（1 处） | 池是**进程全局**的，靠界面进程活着才有意义。而今天界面 ↔ 本机后端**唯一**那条一次性传输是 `backend::observe::local_query::run_query` —— 它 **exec 一次子进程拿 stdout**（判据钉着 `std::process::Command::new(&bin)`）⇒ 池搬过去会**活不过一次调用**。**这不是工作量问题，是形状问题** |
| ③ | 死连接重建重试 | `Err(e) if looks_like_dead_conn(&e) =>`（1 处） | 与 ② 是**同一个状态**的两面：没有池就没有「这条连接死了」这回事 ⇒ 两样必须一起过界，拆不开 |

⚠ 校验位刻意钉**那条 match 臂**而不是函数名：函数名改掉是编译错（判据轮不到说话），
而**把那条臂整条摘掉**编得过、只留一个 unused 警告 —— 那才是这条登记要拦的那一形。

🔴 **本件没有给乙编闹钟**：这一点由判据 `this_ledger_does_not_wind_up_an_alarm_clock` 守着
（禁词表运行时拼，本文件自己不许出现「等那天 / 到时候会红 / 那一刻会红 / 自动提醒」这一族措辞）。

---

## §3 `KR78D1` 为什么没做 —— 四条挡路石，逐条有住址

### ① 🔴 daemon 只读铁律 I7 —— **用户拍的红线，实现方不自批**

`doc/INVARIANTS.md §41.6` **现措辞逐字**：

> **daemon 不许改动用户既有数据；新增文件须 `O_EXCL` 且限于白名单模块。**

而甲里 `install_remote_ccm_helper` / `uninstall_remote_ccm_helper` 改的正是**用户既有的** `~/.bashrc`
（备份 → 覆盖写 → 读回校验 → 回滚），`uninstall_remote_daemon` 是删文件。
⇒ 搬进后端 = **daemon 进程自身**去改用户既有数据。

`DECISIONS.md#R32` 通篇**没有处置这一格**。原措辞里那个限定词（「daemon 对**被观测文件系统**必须只读」）
在收窄时**被拿掉了** ⇒ 现措辞字面上是绝对的。**「它是不是只管本机」这句话谁都没写过**，
⇒ **归 PM / 用户裁**。

⚠ **更要紧的一格**：`remote-daemon-proto/src/readonly_guard.rs::FS_MUTATION_PATTERNS`
全都是 `fs::` / `File::` / `OpenOptions` 命名空间（现打：那张表里 `sftp` 零命中）
⇒ SFTP 那套写（`sftp.remove_file(…)` / `sftp.rename(…)` / `open_with_flags_and_attributes`）
**一条都不匹配** ⇒ **真搬过去，机检不会红** —— 那是一次**静默越线**，正是本区反复抓的那一形。
护栏自己的头注也逐字认过这个洞：人群「不含非 `fs::` 命名空间的写路径」。

### ② 依赖签字闭集**容不下这一档**

daemon 那棵树只有 `russh`，**没有** `russh-sftp`（现打：`russh-sftp =` daemon 0 处 / monitor 1 处）。
加它 ⇒ `remote-daemon-proto/src/readonly_guard.rs::g6_dependency_signoff::SIGNED`
必须同拍加一行（那条判据逐字：「新加一条而没签字 ⇒ 当场红」）。
而它的判档是**闭集三档**：`已量·有写面` / `已量·未见写面` / `未量·靠用法签字`，
其中 `已量·有写面` 的定义逐字要求写清「**凭什么进不了发布二进制**」
⇒ **没有一档容得下「有写面、而且发布二进制里就是要它写」。**

〔本条**没有牙**：住对面那棵树。从 monitor 侧 `include_str!` 过去会**新增一条跨半边编译期边**，
而那张登记表（`cross_half_edge_registry`）不在本件写区 ——
**这与 `dial_move_judge` 立乙半时给的理由逐字同一条**，不是本件新编的借口。〕

### ③ 别名 snippet **只许有一个家**（有牙）

`profile_installer.rs::the_alias_snippet_has_exactly_one_home_in_the_rust_tree` 断言
`shared/ccm-aliases.sh` 在 monitor `src` 树里**恰好一处**（逐字校验位 `vec!["sftp.rs".to_string()]`，现打命中 1 处），
就是 `sftp.rs::CCM_WRAPPER_SNIPPET`；同文件
`the_posix_arm_borrows_the_remote_implementation_instead_of_growing_a_second_one`
另断言**本机**那条 POSIX 路借的就是远端这一份（三个符号各恰好 1 次）。

⇒ 把 `install_remote_ccm_helper` 整条搬走：
留下第二份 snippet ⇒ 前者红；把 snippet 一起搬走 ⇒ 本机那条路失去实现 ⇒ 后者红。
**出路只有一条**：先立一个两侧共用的 crate —— 而那要 `shared_crate_registry` 签字，不在本件写区。

### ④ 要部署的那份字节**住在界面这一侧**（有牙）

远端 daemon 的字节由 `src-tauri/build.rs` 的 `embedded_daemons` cfg 内嵌进 **monitor** 这份二进制
（`sftp::daemon_binary`），而它**不止部署路在用** —— `local_daemon.rs` 释放本机后端时读的是同一份
（逐字校验位 `crate::sftp::daemon_binary(std::env::consts::ARCH)`，现打命中 1 处）。

⇒ 字节搬不走 ⇒ 后端要部署，字节得**从界面递过去** ⇒
过界物就不再是 `R32` 裁定二第 ③ 种那个干净的 `{origin, 动作} → 只回结果`。
**这一格要 PM 拍**：认下「动作带载荷」，还是让后端自己去取那份字节
（daemon 侧 `sidecars/codepicture/fetch.rs` 有同形的拉取协议，但它**自陈**「今天那三样一样都没有」）。

### ⑤ 附带一条（不是独立挡路石，是代价）：SFTP 写原语是三类共用的（有牙）

`connect_sftp` / `upload_atomic` / `upload_atomic_verified`（逐字校验位 `pub(crate) async fn upload_atomic_verified`，命中 1 处）/
`ensure_dir_all` / `read_optional` 的消费者**横跨甲乙丙 ＋ 那 2 处没归类的**。
⇒ **只搬甲**，两个 crate 里就各有一份 SFTP 写层（同一件事两个家）；
要不重复就得连乙丙一起搬 —— 而那正是件计划逐字禁掉的「为了让棘轮降一格把乙硬塞进本件」。

⇒ **「甲最便宜」这半句，本件现打证伪。** 便宜的是它的**过界形状**（一次 `await` 回一个结果，没有进度通道，
这一点派工单说对了）；贵的是**它的落点** —— 落点在一棵有只读铁律、没有 tauri、没有 SFTP 依赖、
而且拿不到那份字节的树上。

---

## §4 `KR78D3` —— `DIAL_SITES` 那一格**没动**，交代如下

`K-R74` 那条棘轮今天 **6 / 历史最低档 6，余量 0**，本件**一格没动**（甲没搬 ⇒ 本来也动不了）。

**为什么就算甲搬完了它也多半不动**：那一格数的是 `connect_session(`，而 `sftp.rs` 只有
`connect_sftp` 一个函数去握手；只要乙（`sftp_pool.rs` 4 处）、丙（`mcp.rs` 3 处）
与那 2 处没归类的还在用它，`connect_sftp` 就搬不走 ⇒ 那一格恒 `false`。
**要它动，得等那一整份文件的 14 处调用点全清空** —— 这是「整处搬完才算 `moved`」这条口径的直接推论。

🔴 **本件不动记分牌本身**（`K-R74` 立的那两条一个字节没改，`git status` 为证）。
但**新买到一条**：`the_sftp_row_on_the_scoreboard_is_still_unmoved_and_this_ledger_says_why`
—— 谁把那一行的第三栏翻成 `true`，判据当场红并印出「界面进程里还剩几处 `connect_sftp`」。

**这一条堵的正是件计划点名的那个失效方向**：棘轮**只禁涨不禁降**，
所以「把 `moved` 翻成 `true`」在 `dial_home_registry` 那条递减棘轮眼里是**合法的一格进展**。
实测见 `§5 M4`。

---

## §5 逐刀 —— 死值验（每趟都有「它真的重编过吗」的活体信号）

变异台：`scratchpad/mut.py`。备份落 **scratchpad**（㉑，不落 `src-tauri/src/` 也不落工作树根）；
`restore` 用 `copyfile` ＋ `os.utime(src, None)` **重置 mtime**，避免 `R35 裁定三`那次
「cargo 复用上一刀的二进制 ⇒ 读到上一刀的回声」。

| 刀 | 变异（最小面） | 期望 | 实打 | 活体信号 |
|---|---|---|---|---|
| **M1** | `DIAL_CENSUS` 里 `sftp.rs` 的处数 `6 → 5` | `every_reading_…_derived_from_the_tree` 红 | ✅ **红 1 条**，其余 8 条全绿；报错点名 `sftp.rs` | 重编 True |
| **M2** | `POOL_SHAPE` `(11,4) → (10,4)` | `the_pool_shape_…` 红 | ✅ **红 1 条**（命令数那一半） | 断言行 312 |
| **M2b** | `POOL_SHAPE` `(11,4) → (11,3)` | 同上 | ✅ **红 1 条**（拨号数那一半，断言行 317 ≠ 312 ⇒ **两半各自承重**） | 断言行 317 |
| **M3** | 把 `POOL_SRC` 的 `include_str!` 从 `sftp_pool.rs` 换成 `mcp.rs` | 凡是从这份语料派生的判据都要红 | ✅ **红 5 条**（含地板那条先咬）⇒ **语料绑定是活的，不是常量** | 重编 True |
| **M4** | `ssh_source.rs` 里 `DIAL_SITES` 的 `sftp.rs` 行 `false → true`（**跑全 lib**） | 本件那条红 | ✅ **全 lib 1434 条里红 2 条**：本件 `the_sftp_row_…` ＋ 记分牌自己的 `six_of_the_seven_dial_sites_are_still_in_this_process`。⚠ **`dial_home_registry` 那条递减棘轮一声不吭**（它只禁涨）—— 这正是本件那条判据买到的那一格 | 重编 True |
| **M5** | 挡路石 ① 的逐字校验位改一个字（`数据 → 资料`） | `every_blocker_…_quoted_verbatim_on_disk` 红 | ✅ **红 1 条**，点名 `doc/INVARIANTS.md` | 重编 True |
| **M6** | 往本文件插一句「等那天」 | `this_ledger_does_not_wind_up_an_alarm_clock` 红 | ✅ **红 1 条** | 重编 True |

**反向那半（常驻判据，不靠人去动别人的文件）**：

- `an_empty_corpus_makes_the_census_red_by_itself` —— 喂空语料，普查必须**自己先红**（地板 60 000 B；真语料按 `.py` 那条腿现打 48 510 ＋ 21 253 ＋ 20 038 ＋ 11 070 = **100 871 B**，
  Rust 那条腿的剥法不同、数值未逐字节对拍，只对拍了「都在地板之上」）。
- `a_pool_missing_any_one_of_the_three_is_caught_and_named` —— 把真语料里那三样**逐个挖掉**（合成语料），
  判据必须红**并点名是哪一样**；挖不动就判「本条在空转」。

**CRASH（编译错，单列，不算读数）**：

- `CRASH-1` —— `three_things_present` 里 `for … in mine` 之后又 `mine.len()`（E0382 borrow after move）。
  改成 `&mine`。**一处，一次修好。**

---

## §6 本件刻意**没有**买到的（写死，别读大）

- **甲一个字节没搬** ⇒ `russh` / `russh-sftp` 仍在界面 crate 的依赖树里，`R32` 裁定一那条二值判据**一格没动**。
- **daemon 那一侧的两条挡路石（① 的护栏人群、②）本件没有机检** —— 理由是不长跨半边编译期边（见 `§3②`）。
  它们在盘上带的是**符号住址**，不是校验位。
- 本件那张登记**不会在「乙可以搬了」的那天说话** —— 它没有闹钟，它只在**它引的话变了**的时候红。
- `sftp_pool.rs` · `port_forward.rs` · `K-R74` 立的那两条：**一个字节没动**。
