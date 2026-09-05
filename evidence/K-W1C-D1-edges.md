# K-W1C · D1 边表（现打读数）

量具住址：`evidence/K-W1C-D1-edge-ruler.py`（被测对象由 `--root` 给，默认 = 本文件所在工作树）。
被测对象：`.claude/worktrees/k-w1c`，量于**分支尖**（见本工作树 `git rev-parse HEAD`；
本表所有读数都在同一趟里出，命令逐字 `python3 evidence/K-W1C-D1-edge-ruler.py`）。

⚠ **本表不许当常量引用**：带行号的每一格旁边都抄了那一行的**逐字内容**（13c 的校验位），
重跑一趟即可自证；行号会随任何插入漂，逐字内容不会。

## 尺子怎么切（三个词都承重）

一条**边** = 「一个关于会话的**事实**到达」→「一次**缓存写**」的配对。三道闸，逐条：

| 闸 | 判什么 | 谁被它挡在人群外 |
|---|---|---|
| ① 缓存闭集 | 只有 `SidHwndCache` / `RemoteHwndCache` 这两份**拉前**缓存（闭集只有一个住址 = 量具里的 `CACHE_TYPES`，本表的「2 个」是它现算出来的） | `EventReplay`（回放缓冲）· `tmux_raw_registry`（`tmux ls` 原文）· `REMOTE_IDLE`（灰灯账本） |
| ② 「缓存写」是推出来的、不是写死的 | 那两个 `impl` 块里**真的动 `by_sid` 表**的方法（直接 `by_sid.write()`，或调另一个这样的方法，跑不动点） | 只读方法（`lookup` / `load` / `persist`） |
| ③ 「到达」要从缓存**外面**打进来 | 调用点不在那两个 `impl` 块里 | 写方法内部再调写方法（**内部转调**，事实只到达了一次） |

⚠ **闸③ 认调用点是按整份文本扫的，不按单行**（09-04 接线那一拍现打逼出来的订正）：
`接收者` 与 `.方法(` **换行分开**的写法，按行扫整个看不见。活体就是接线落地的那一句
`remote_cache_for_emitter\n    .apply_remote_disposition(&sid, &disposition);` ——
按行扫时它被漏成一条边，而 `apply_remote_disposition` 当场被印成「入口尚未接线」，
那是一句**自信的错答案**，不是「判不了」。折行的校验位由尺子折回一行给出。

**分母，两层**：语料面 **105 份 `.rs`**（`src-tauri/src/**`）→ 静态认得出的接收者 **6 处**
→ 机检出 **7 条边 + 4 处内部转调**。TS 侧（`src/**/*.ts`）单独量：**缓存写 0 处**
（两份缓存整个住 Rust ⇒ 与 `K28` 的方向一致），只有 **2 处事实到达口**（那两个 `invoke`）。

## 边表：7 条（★ **接线之后**的读数，量于 `lib.rs` 那两行落地之后那一趟）

| # | 类 | 缓存 | 写方法 | 住址（带逐字校验位） | 事实 | 今天有判据吗 |
|---|---|---|---|---|---|---|
| 1 | 建 | SidHwndCache | `record` | `lib.rs:668` 逐字 `cache_for_emitter.record(sid, info.pid, &bind_for_emitter);` | 本机 diff 报 added | **0** |
| 2 | 忘 | SidHwndCache | `apply_local_removal` | `lib.rs:698` 逐字 `cache_for_emitter.apply_local_removal(&removed);` | 本机 diff 报 removed（`Gone` 或 `Superseded`） | `a_local_session_that_is_gone_gets_forgotten_in_memory_and_on_disk`<br>`a_local_session_that_was_superseded_gets_forgotten_too` |
| 3 | 建 | RemoteHwndCache | `try_bind` | `lib.rs:809` 逐字 `if cache.try_bind(&sid) {` | 远端 daemon 帧报 session_added | **0** |
| 4 | 忘 / 不忘（按分流裁决） | RemoteHwndCache | `apply_remote_disposition` | `lib.rs:864` 逐字（折行，尺子折回一行）`remote_cache_for_emitter.apply_remote_disposition(&sid, &disposition);` | 远端 removed，`classify_removed` 的**裁决**到达 | `a_remote_session_classified_as_archive_gets_forgotten`<br>`a_remote_session_that_only_went_idle_keeps_its_binding` |
| 5 | 建 | RemoteHwndCache | `try_bind_with_retry` | `lib.rs:2014` 逐字 `cache.try_bind_with_retry(` | 用户点 ↗ 且缓存没命中 | **0** |
| 6 | 忘 | RemoteHwndCache | `forget` | `lib.rs:2028` 逐字 `cache.forget(&session_id);` | 用户点 ↗ 且旧绑定 verify 失败 | **0** |
| 7 | 建 | RemoteHwndCache | `try_bind_with_retry` | `lib.rs:2032` 逐字 `cache.try_bind_with_retry(` | 同 5（verify-fail 重绑路） | **0** |

**有判据的边：2 / 7**（接线之前是 **0 / 7**）。边 4 那一格**一条边盖两种归宿**：
`lib.rs` 那一句是**无条件**调用的，「`Archive` 忘 / `Idle` 不忘」整个住在
`apply_remote_disposition` 里 —— 判断只有一个住址，而那个住址测得动。

**内部转调 4 处**（不算边）：`bind.rs` 里 `self.forget(&removed.sid);` · `self.forget(sid);` ·
两处 `if self.try_bind(sid) {`。

**入口尚未接线：0 个。** ⚠ 空**不是**「没查」，是「今天一个都没有」——
量具里那道反向对账（一个零调用点的写方法没登记 ⇒ 退 3）仍在。

⚠ **仍然没有判据的 5 条**（诚实边界，别读成「这条链看住了」）：3 条**建**边
（`record` / `try_bind` / `try_bind_with_retry` ×2）+ ↗ 那条 verify-fail `forget`。
本波只买了「忘」那一侧的两条；**建**边一条都没买。

## 与 PM 那把粗尺子对账（PM 量于 `4eb271f`，我量于本树尖）

| | PM 的粗尺子 | 本尺子 | 差在哪 |
|---|---|---|---|
| `.forget(` 全仓 | 5 处 | —— | 接线**之前**我在本树重打也是 **5 处**，行号逐个对上（`bind.rs:836` 测试 · `lib.rs:699` · `:878` · `:1710` · `:2025`）⇒ 那个读数**没馊**。⚠ 接线**之后**这个数变成 **3 处**（`lib.rs` 那两句换成了新入口）—— 这正说明「`.forget(` 出现几次」是个会随重构乱跳的数，不是这条链的边数 |
| 与拉前有关的生产 `forget` 点 | 3 | 3（接线前的边 2 · 4 · 6） | 一致 |
| **建**边 | 3（`lib.rs:668` · `:809` · `:2011`/`:2029` 算一处） | **4**（`:668` · `:809` · `:2011` · `:2029`） | PM 把 `:2011` 与 `:2029` 并成一处；它们是**两个调用点、两个事实前件**（cache-miss 路 vs verify-fail 重绑路）⇒ 按「一次调用一条边」的口径是 2 条 |
| 合计 | 6 | **7** | 差的就是上面那 1 条 |
| `lib.rs:1710` `event_replay` | 「无关 1 处」 | 同判**不算边**，而且是**两道独立闸**各自挡住的 | 见下面三刀 |

## 死值验（D1 的刀）

每刀切之前先断言锚点命中数、切完打「变异已落地」、跑完**完全复位**
（量具 md5 `1640804992d13d0fb3bdf6dfee1a25ac`，切前切后逐字节相同）。

| 刀 | 锚点 · 命中 | 变异 | 读数 | 判 |
|---|---|---|---|---|
| **D1-刀①（空转自检）** | `--only src-tauri/src/utils.rs`（不含任何缓存写的文件） | 不改量具，换语料 | 写方法 0 · 接收者 0 · **退出码 2**，并印「这不是『答案是 0』，是这把尺子在这份语料上没东西可切」 | ✅ **不静默给 0** |
| **D1-刀①′（同形第二份语料）** | `--only src-tauri/src/session_map.rs`（**有会话事实、无缓存写**） | 同上 | 同上，**退出码 2** | ✅ 空转自检不是只对 `utils.rs` 那一份成立 |
| **D1-刀②（单断闸②）** | `if "by_sid.write()" in body` 命中 **1** 次 | 把「写方法」那道闸放宽成 `"lock()" in body or "by_sid.write()" in body`，闭集不动 | 边仍 **7** 条，`replay.forget` **仍不出现** | ✅ 闸①（接收者类型）**单独**就挡得住 |
| **D1-刀③（单断闸①）** | `CACHE_TYPES = (…)` 命中 **1** 次 | 把 `EventReplay` 加进闭集，写方法那道闸不动 | 边仍 **7** 条；`EventReplay` 的写方法推出来 **0 个**（它没有 `by_sid`）⇒ `replay.forget` **仍不出现** | ✅ 闸②（真的动 `by_sid`）**单独**就挡得住 |
| **D1-刀④（全断）** | 两处锚点各 1 次 | 两道闸同时放宽 | 边 **11** 条，`lib.rs:1710` 逐字 `replay.forget(&session_id);` **当场作为一条边出现** | ✅ 两道闸都是**真闸**，不是仪式 |

⚠ **诚实边界**：刀②/③/④ 证的是「这两道闸各自都在承重」，**不**证明尺子对所有别的同名东西
都分得开（`forget_tmux_raw` / `clear_idle` 是自由函数，压根不在「方法调用点」这个人群里，
被闸①以另一种方式挡住 —— 那一格我**没有**用刀验，只是读过它们的定义）。

## 接线那一拍：量具**自己发现了盘变了**（这就是它该买的东西）

`lib.rs` 那两行落地之后**先不动量具**跑一趟 ⇒ **退出码 3**，逐条点名（原文在
`scratchpad/ruler-after-wiring-BEFORE-update.txt`）：

```
  - 机检到一条边，登记表里没有：…lib.rs:698  cache_for_emitter.apply_local_removal(&removed);
  - 登记表里这条边今天在盘上找不到了：('…/lib.rs', 'forget', 'cache_for_emitter.forget(&sid)')
  - 登记表里这条边今天在盘上找不到了：('…/lib.rs', 'forget', 'remote_cache_for_emitter.forget(&sid)')
  - `apply_local_removal` 已经接上线了 —— 从 `PENDING_ENTRIES` 里删掉这一行，并把对应那条边的判据栏从 0 改成它
```

⇒ 四条对账里**三条形状**（新边未登记 · 旧边消失 · 入口接上线了）各出声一次。

🔴 **而同一趟也把量具自己的一个真缺口暴露了**：那一趟**只报了 `apply_local_removal`**，
没报 `apply_remote_disposition` —— 因为后者的调用点是**折行**写的，而第一版按行扫。
它当时的读数是「入口尚未接线：1 个」，**那是一句自信的错答案**。
修法在上面闸③ 那条 ⚠ 里；修完重跑，两条都报了出来（退 3），登记表更新后退 0。

## 量具自己被逮到的三处（前两处在交付之前自抓，第三处在接线那一拍）

1. **同名 cfg 变体互相覆盖**：`methods_of` 第一版写 `out[name] = body`，而 `try_bind` /
   `try_bind_with_retry` 各有两份（`#[cfg(windows)]` 真写 + `#[cfg(not(windows))]` 返 `false`）
   ⇒ 非 Windows 那份把真写那份**盖掉**，两条建边静默漏掉，尺子当场报 6 条而真值 7 条。
   已改成拼接。
2. **子串陷阱**：登记表锚点 `cache_for_emitter.forget(&sid)` 是
   `remote_cache_for_emitter.forget(&sid);` 的**子串** ⇒ 远端那条边被认成本机那条，
   「事实」栏印出一句**自信的错答案**（不是「找不到」，是「找错了」）。
   已改成 `anchored()`（要求命中落在标识符边界上）。
   ⚠ **同一条病在散文里又犯了一次，登记在此**：交回报告里我写过「锚点
   `cache_for_emitter.forget(&sid);` 全仓 **1** 次」—— 那是 `anchored()` 的**边界口径**下的数；
   照着用裸 `grep -c` 重打得到的是 **2**（`lib.rs:878` 那句是子串误命中）。
   **报一个命中数不给口径，下一个人就复不出来。**
3. **按行扫看不见折行的调用**（接线那一拍现打，见上一节）——
   漏一条边 + 把一个已接线的入口印成「尚未接线」。已改成按整份文本扫，
   校验位由尺子折回一行。
