# K-W1C · D3 / D4 死值验（逐刀真实输出）

**量于**：`.claude/worktrees/k-w1c` 分支尖 `3d0a21e`（每刀切在这个尖上，跑完 `git checkout --`
复位，`git diff --stat` 空）。
**跑法**（唯一合法那条）：`PB_WS=backend-consolidation .claude/devbox/gate <本树绝对路径> k-w1c`。
**读的是哪一格**：`cargo` 那一格里 **`src-tauri` 包**自己那行 `test result`。

**分母**（两个数，别混）：
- `src-tauri` 包 **1285** 条（= 基线 1280 + 本波 5），每刀的 `passed + failed` 都等于它 —— 这就是「判定行没掉」的核对法；
- 门禁 `cargo` 格印的是**8 个包的 passed 合计**：基线 **1390** → 本波 **1395**（+5）。

**本波新增的 5 格**（判据名逐字）：
1. `a_local_session_that_is_gone_gets_forgotten_in_memory_and_on_disk`
2. `a_local_session_that_was_superseded_gets_forgotten_too`
3. `a_remote_session_classified_as_archive_gets_forgotten`
4. `a_remote_session_that_only_went_idle_keeps_its_binding`
5. `verify_binding_cannot_tell_that_the_window_changed_hands`

---

## 刀表

| 刀 | 切在哪（锚点逐字 · 命中数） | 变异 | 真实读数（`src-tauri` 包那行） | 红的是哪几条 | 判 |
|---|---|---|---|---|---|
| **B** 不变异 | —— | 无 | `1395 passed（8 个包合计）` · 九格里八格 ok | 无 | ✅ **不是恒红** |
| **C** 上游 diff | `session_map.rs` 的 `Some(new_sid) if new_sid != sid.as_str() => RemovedSid::superseded(sid.clone()),` · **1 次** | `superseded` → `gone` | `1283 passed; 2 failed` | `session_map::tests::diff_detects_superseded_only_with_positive_identity_evidence`<br>`ssh_source::f032_idle_tests::the_tmux_cache_has_one_writer_and_only_origin_keys` | 见下「刀 C 怎么读」 |
| **D** 单断 `Superseded` | `bind.rs` 的 `self.forget(&removed.sid);` · **1 次** | 改成 `if removed.cause == RemovalCause::Gone { … }` | `1283 passed; 2 failed`（与刀 G 同趟） | `a_local_session_that_was_superseded_gets_forgotten_too` | ✅ Superseded 那格**有牙** |
| **G** 单断 `Idle` | `bind.rs` 的 `if matches!(disposition, crate::ssh_source::RemovedDisposition::Archive) {` · **1 次** | 改成无条件 `self.forget(sid);` | 同上一趟 | `a_remote_session_that_only_went_idle_keeps_its_binding` | ✅ Idle 那格**有牙** |
| **E** 单断 `Gone` | 同刀 D 的锚点 · **1 次** | 改成 `if removed.cause == RemovalCause::Superseded { … }` | `1283 passed; 2 failed`（与刀 F 同趟） | `a_local_session_that_is_gone_gets_forgotten_in_memory_and_on_disk` | ✅ Gone 那格**有牙** |
| **F** 单断 `Archive` | 同刀 G 的锚点 · **1 次** | 整块换成 `let _ = (sid, disposition);` | 同上一趟 | `a_remote_session_classified_as_archive_gets_forgotten` | ✅ Archive 那格**有牙** |
| **H** 单断「落盘那一半」 | `bind.rs` 的 `if self.by_sid.write().remove(sid).is_some() {` · **1 次** | 删掉块内那句 `self.persist();` | `1282 passed; 3 failed`（与刀 I 同趟） | 两条本机判据，**而红的正是落盘那一句**：逐字「内存忘了、**落盘文件里还留着** —— monitor 一重启这条死绑定就复活。」与「被顶替的那条绑定还留在落盘文件里 —— 重启即复活。」 | ✅ DoD 里「**且落盘文件里也没了**」那半**不是装饰** |
| **I** 单断 D4 | `bind.rs` `verify_binding` 里的 `let mut cur_owner: u32 = 0;` · **1 次** | 前面插一行 `let _ = &binding.title_at_bind;` | 同上一趟 | `verify_binding_cannot_tell_that_the_window_changed_hands`，诊断逐字把那一行端出来：`` `verify_binding` 开始看窗口标题了（命中 `title_at_bind`）：["        let _ = &binding.title_at_bind;"] `` | ✅ D4 那格**有牙**，且**说得出是哪一行** |
| **J** D4 的自检的自检 | `bind.rs` 的 `guard_core::production_code(include_str!("bind.rs"))` · **1 次** | 指到 `include_str!("utils.rs")` | `1284 passed; 1 failed` | 同上那一条，逐字「生产段里没有 `pub fn verify_binding` —— 抽取器坏了，本条此刻无效」 | ✅ 抽取器坏了会**出声**，不会零命中地绿 |
| **K** 全断（= `7u`） | 刀 D 与刀 F 的两个锚点 · 各 **1 次** | 两个新入口的函数体**都掏空** | `1282 passed; 3 failed` | gone · superseded · archive | 见下「`7u`」 |

⚠ **两刀同趟的归因论证**（刀 D+G · 刀 E+F · 刀 H+I 各是一趟）：能同趟是因为**每条判据的代码路径只碰到那两处变异中的一处** ——
`a_local_*` 两条只调 `apply_local_removal`，`a_remote_*` 两条只调 `apply_remote_disposition`，
D4 那条**一个 `forget` 都不调**（它读源文本）。
⇒ 观测到的红集恰好等于两个预期红集的并，而每条红只可能由那一处变异造成。
🔴 **要单独复核就一刀一趟重跑**，命令与锚点上表逐行都在，复得出来。

## 刀 C 怎么读（★ 件计划 `§3` 预言过这一格，实测与预言不同，如实写）

件计划写「今天这一刀大概率仍绿 ⇒ 说明 D3 只买到 Gone 那一格，Superseded 仍是碰巧对」。
**实测：我的 5 条判据确实全绿，但『没人看着』这个结论不成立** —— 当场红了**两条既有判据**。

正确的读法是：**刀 C 打的不是 D3 的那条边。**
- 刀 C 打的是**上游**那一步：「同 pid + 同 procStart 换 sid **该判** `Superseded`」。
  它由 `diff_detects_superseded_only_with_positive_identity_evidence` 与
  `the_tmux_cache_has_one_writer_and_only_origin_keys` 两条既有判据守着。
- D3 打的是**下游**那一步：「`Superseded` 这个事实**到达之后**，缓存该变成什么样」。
  它的牙在**刀 D**（只在 `Gone` 时 forget ⇒ superseded 那条单独红）。
- ⇒ 这是**两条边**，D3 的射程按设计不含前者。「Superseded 那一格」在本波之后**不再是碰巧对**，
  但**证据是刀 D，不是刀 C**。

⚠ **诚实边界**：本波**没有**给「本机 `/branch` 从头到尾走通」补端到端判据 ——
上游那一段（`diff_sessions`）与下游那一段（缓存）各有判据，**中间那一跳（`lib.rs` 的 emitter 把
`RemovedSid` 交给入口）今天仍然没人验**，因为它住 `lib.rs`、而 `lib.rs` 不在本波写区。
D1 边表里边 2 / 边 4 的判据栏写 `0` 说的就是这件事。

## `7u`（把实现整个退掉，还有多少条新断言仍绿）

刀 K：两个新入口的函数体都掏空 ⇒ **5 条里 3 条红、2 条仍绿**。

| 仍绿的 | 为什么它仍绿（不是仪式） |
|---|---|
| `a_remote_session_that_only_went_idle_keeps_its_binding` | 它是一条**负向**性质（「不忘」）⇒ 「什么都不做」当然满足它。它的牙在**反方向**那一刀上（刀 G：改成无条件 forget ⇒ 当场红）。⚠ 而它**值得留**：`Idle` 该不该忘还没裁（件计划 D5），这一条钉的是「今天是这样」，哪天有人改它会出声。 |
| `verify_binding_cannot_tell_that_the_window_changed_hands` | 它判的是**存量**行为 —— `verify_binding` 我**一个字节没改**（逐函数 md5：`d305ffa` 上的 31 个 fn 全部逐字节不变）⇒ 「退掉本波的实现」跟它无关。它的牙在刀 I / 刀 J 上。★ 本波在 D4 上唯一的产出**就是这条判据本身**，说它「7u 仍绿」是同义反复，不是发现。 |
