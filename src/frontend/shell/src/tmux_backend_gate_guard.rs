//! **§34 Gate 2 的两侧账**：backend 侧的身份门必须在；monitor 侧 **kill 与 send-keys
//! 都必须已切过去**（F04b 翻了 kill 那半，F04c 翻了另一半 —— 那条禁令整个翻完了）。
//!
//! # 病史：U10 立的那条前提触发器，F03 让它红了 —— 可它没红
//!
//! U10 摸底发现：monitor 的 `tmux_send_keys` / `kill_remote_tmux` 带着 §34 的 **Gate 2 union**
//! （名字命中 `cc-*`/`<X>-cc` **或**远端 `@ccm_sid` 已设，不通过回 `CCM_GUARD_REJECTED`），
//! 而后端的 `control/launch.rs` **只核会话存在性**。把那两条改走 backend
//! ＝ **静默丢掉一道门**：功能看起来一样、门禁全绿，而门没了。
//!
//! 于是立了一条**前提触发器**：「backend 一出现身份守卫 ⇒ 本护栏主动红，逼人回来重新裁定」。
//!
//! ⚠ **F03 真的给后端装了门（`control/gate.rs`），而本护栏纹丝不动。**
//! 根因：它的扫描面是**一张硬编码的两文件表**（`launch.rs` + `tmux_hook.rs`），
//! 新加的 `gate.rs` **根本不在它眼里**。这是本仓「扫描面画小了」那一族的又一次 ——
//! `readonly_guard::spawn_registry` 的头注里逐字记着同样的事（那是第五次，而且也是
//! 「新增一个文件，硬编码清单扫不到」）。**同一个坑，同一个仓，第二个模块。**
//! ⇒ F03 把扫描面改成**递归遍历 `control/`**，并配抽取器自检钉住文件数地板。
//!
//! # 今天这个模块钉两件事（前提已变，禁令的理由跟着换）
//!
//! 1. **反向锚点**：backend 侧的身份门**必须还在**。删了它就红 ——
//!    从「不许出现」翻成「必须存在」，是 F03 之后前提变了的直接后果。
//! 2. **禁令整个翻面了**（F04b 切 kill、**F04c 切 send-keys**）：定框 C6
//!    「先搬 Gate 2，再切 kill / send-keys」**走完了**。今天钉的是反向 ——
//!    **两条命令都必须走后端，不许退回**
//!    （原是 `kill_now_routes_through_the_backend` / `send_keys_now_routes_through_the_backend` 两条〔散文墓碑〕，
//! 随那两条命令迁到界面翻面了，见第 4 条）。
//!    ★★ **`K-R72`（09-12）：那两条又各加了一格 —— 回潮闸。**
//!    `C7` 那条过渡期 SSH 回落**删了**（`K-R54` 裁定表第 1 · 2 处），于是这两条判据
//!    从「主路必须走 backend **且回落必须还在**」变成「主路必须走 backend
//!    **且盘上不许再有第二条路**」。⚠ 两次翻面的方向是相反的，别读成同一格改了措辞：
//!    先前那半逐字要求 `connect_and_exec_cmd` **在**，今天逐字要求它**不在**。
//!    ⚠ F04c 当年给 backend 补过一个裸键 mode `send-keys-raw`（打断当前回合的 `Escape`）；
//! 换号重启不再发 `Escape` ⇒ 无调用者，mode 已删。
//! 3. ~~Gate 3 的前提触发器~~ **已在 F04a 触发并改写**：backend 现在**有** Gate 3
//!    （`control/gate.rs::admit_destructive` + `control/kill.rs`）。那条触发器
//!    「backend 一出现 `session_windows`/`kill-session` 就红」**如设计般红了一次**
//!    （`出现了 Gate 3 / kill 的标志 ["session_windows", "kill-session"] —— 这多半是好事`），
//!    于是按它自己的要求翻面：从「不许出现」改成 [`the_backend_now_has_gate3`]（**不许消失**）。
//!    ⚠ **它红了不是误报，是它的岗位。** 删掉它才是错的处置（铁律 13）。
//!
//! 4. **两条命令整条迁到界面**（`src/frontend/ui/tmux-control.ts::killSession` / `sendKeys` 经通道直接说
//!    后端的 `kill` / `launch`）：monitor 里 `kill_remote_tmux` / `tmux_send_keys`〔散文墓碑〕那两个函数体不在了，
//!    第 2 条那两格（`kill_now_routes_through_the_backend` / `send_keys_now_routes_through_the_backend`〔散文墓碑〕）
//!    翻成 [`tests::the_monitor_has_no_second_path_that_kills_a_session`]（monitor 里一处杀会话的 shell 串都没有）
//!    ＋ [`tests::the_front_end_speaks_the_tmux_control_ops_only_through_one_module`]（界面只经一处说）。
//!    第 1、3 条（后端的门还在路上）**一格不动** —— 界面从此只靠它们。
//!
//! ⚠ **约定型守卫**（同 `readonly_guard` 一族）：查的是符号名的源码形态，
//! 挡得住「顺手把这两条改走后端」，挡不住「换个名字继续错」。**比没有强，别读成证明。**

#[cfg(test)]
#[path = "../../../../tests/frontend/shell/tmux_backend_gate_guard_tests.rs"]
mod tests;
