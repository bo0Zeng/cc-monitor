//! `K-W1B D2`：**桌面侧「通用层认得出某个 agent」的地方逐条登记** —— 那一半今天零判据。
//!
//! # 名字的来历 —— 它**不叫** `agent_boundary_guard`，而那是刻意的
//!
//! 本模块初版就叫那个名字（派工时的预批名），落地当轮撞出一处真伤害，PM 裁定改名。
//! 把来历留在这儿，是为了让下一个人**不要再把它改回去**：
//!
//! 1. **daemon 那棵树里已经有一个 `agent_boundary_guard`**
//!    （`src/backend/agent_boundary_guard.rs`，`S1`：通用层不许知道任何 agent 的
//!    名字与文件格式，人群由它的 `CORE_FILES` **opt-in** 列举）。**本模块不是它的桌面版**：
//!    那一条判「通用层提没提 agent 的名字/布局」，本条数「通用层认不认得出**具体哪一个**
//!    adapter」—— 形态照的是 daemon 侧**另一条**，`agent_locality_guard` 的判据④
//!    （`general_layer_adapter_call_sites_are_enumerated_one_by_one`）。
//! 2. **同名会当场把一处既有引用指错**：`src/bridge/src/ssh_source.rs` 里有一处逐字写着
//!    「`agent_boundary_guard::FROZEN_COMPAT`」，指的是 **daemon 那一个**。本模块若同名，
//!    那一处在本树里就变成一个**指得到、但指错**的名字（本模块没有 `FROZEN_COMPAT`）。
//!    ⚠ 那一处**不用改**：改名之后它在本树里重新变成唯一解 —— 只指 daemon 那个模块，而那是对的。
//! 3. **`K-W3` 正要把两棵树并成一个 crate** ⇒ 同名同 crate 时必须改，早改比晚改便宜。
//!
//! 名字按 monitor 侧「清账 + 递减棘轮」那族的既有命名取（`local_read_surface_registry` ·
//! `launcher_identity_registry` · `session_name_registry` · `exec_site_registry`），
//! 并说出它**数什么**（派发/调用点），不用含糊的「boundary」。
//!
//! # 它数什么 —— 四张脸，前三张该压到零，第四张方向相反
//!
//! 桌面侧的机制与 daemon 侧**不同**：这边**有** trait（`adapter::AgentAdapter`，9 个方法）
//! ＋ `for_kind()` 派发。⇒ 「直呼 `agents::<名>::`」那根针在这边一处都打不中，
//! 而耦合**换了形状**住在别处。四张脸，逐张一句话：
//!
//! | 脸 | 一行长什么样 | 为什么它是耦合 | 方向 |
//! |---|---|---|---|
//! | [`Face::Active`] | `crate::adapter::active()` · `active().layout()` | 调用点**说不出**它指哪个 agent —— 「当前活跃的那个」今天恒等于 Claude | 压到零 |
//! | [`Face::Facade`] | `crate::adapter::records_dir(&d)` · `has_record_ext(p)` | 门面替调用者把 kind 写死了（每一个都有 per-kind 兄弟 `*_for` / `*_with`）⇒ **调用点连「我要活跃那个」都没说** | 压到零 |
//! | [`Face::KindLiteral`] | `for_kind(AgentKind::Codex)` | 拿**字面量**当实参 —— 那不是分派，那是写死 | 压到零 |
//! | [`Face::RuntimeDispatch`] | `for_kind(kind)` | 传的是**运行时** kind ⇒ 接口正在被正确使用 | **不上棘轮** |
//!
//! ★ 第四张脸单独登记、**刻意不上棘轮**：它是好方向。混进同一个计数，棘轮就会奖励
//! 「把真分派改回写死」。这条分家的理由与 daemon 侧 `AGENT_REGISTRY_SITES` 的头注同族
//! （两类性质相反的东西不许共用一个数），但方向反过来。
//!
//! # 为什么是**逐文件相等**，而不是只比总数
//!
//! daemon 侧那条判据的头注已经写死并实测过：只比总数的话「从 A 文件挪一处到 B 文件」
//! 会**全绿**，而那是把改动面藏起来、不是消掉。⇒ 本条也是**多一处红、少一处也红**。
//!
//! ⚠ **本条比 daemon 侧那条少一个洞**：那边为了不稀释靶子，把注册表文件整个**扣出人群**，
//! 于是得再立一条判据⑦（注册表文件处数钉死 = `REGISTRY.len()`）当对价，堵「挪进去刷数」。
//! 本条**一个文件都不扣** —— `adapter.rs`（接口自己那一份）也在人群里，按文件分行；
//! 唯一不算的是**定义行**（`pub fn active()` / `pub fn records_dir(…)` 那几行本身就是接口）。
//! ⇒ 把通用层的一处挪进 `adapter.rs`：那一行照样被数，总数不掉，两张登记同时红。**不需要对价。**
//!
//! # 🔴 针怎么不重蹈 `.active(` 的覆辙（件文件 `§0c①` 那个活体）
//!
//! 那次的读数是 **0**，而 0 看起来像「问题已经没有了」—— 实际是 `active` 是**自由函数**、
//! 调用形一律 `adapter::active()`（前面是 `::`，不是 `.`）⇒ **带前导点的针结构上够不着它**。
//!
//! 本条的三道处置，缺一道就会退回那个失败形：
//!
//! 1. **针不带前导点**，只写 `active()` —— 全路径 `crate::adapter::active()`、
//!    相对 `adapter::active()`、模块内裸 `active()` **三种写法一网打尽**；
//! 2. **带闭括号**（`active()` 而不是 `active(`）—— `watcher.rs` 里另有一个**同名**的
//!    自由函数 `active(&session_id)` 判**会话活性**，与适配层无关。
//!    「同一个词装了两件事」这一族在本仓有账：裸 `active(` 会把它一起数进来，那是假阳；
//! 3. **匹配单位带边界**（走 [`guard_core::contains_word`]，`needle_anchor_registry` 头注
//!    逐字要求的三个原语之一）—— 现打的活体：不带边界时 `map.snapshot_active()`
//!    会被数成一处耦合（`lib.rs` 那一行），读数 12 虚高成 13。
//!
//! ⇒ 而这三道**都由 [`tests::the_detectors_catch_synthetic_violations`] 反向钉着**：
//! 把针拼坏成 `.active(`、或去掉边界、或去掉闭括号，那条会**红在「针空转」上**，
//! 不是安静地全绿。形状照 daemon 侧的 `the_s4b_detectors_catch_synthetic_violations`。
//!
//! # ⚠ 诚实边界（四条，写在这里而不是只写在件计划里）
//!
//! 1. **改名/别名躲得过**：`use crate::adapter::active as f;` 然后调 `f()` ⇒ 本条一处都不红。
//!    今天全树**零处**这种写法（本条落地时现打），但它**没有判据看着**，是真开着的洞。
//! 2. 认的是**字面形态**，不是语义。有人自己 `PathBuf::from(root).join("projects")`
//!    绕开门面 ⇒ 本条看不见。那一格归 `local_read_surface_registry`（**那本账数的是
//!    「桌面端还在自己读 `~/.claude`」，与本条是两把尺子，别互相报数** —— 件计划 `§2` 逐字）。
//! 3. `production_code` 剥掉注释与 `#[cfg(test)] mod` ⇒ **文档与测试段里怎么写都不红**
//!    （刻意的：本模块自己的散文里就有这些形态），代价是「只在测试里写死一个 agent」逮不到。
//! 4. 门面表 [`tests::KIND_ERASING_FACADES`] 是**手写的闭集**，加一个新门面不会自动进人群。
//!    对价是 [`tests::every_registered_facade_really_erases_the_kind`]：每一个登记的门面
//!    必须真的在 `adapter.rs` 里、且真的经 `active()` 取适配器 —— 表与事实漂开会红。
//!
//! 注：本模块整体在 `#[cfg(test)]` 内，非测试构建为空。

#[cfg(test)]
#[path = "../../../tests/bridge/agent_dispatch_registry_tests.rs"]
mod tests;
