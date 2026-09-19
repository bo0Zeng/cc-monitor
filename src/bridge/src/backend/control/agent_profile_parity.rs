//! F06：**agent 适配表的跨语言对拍** —— C4「ccm 变零决策执行臂」的前置。
//!
//! # 为什么这是前置，而不是 F06 本体
//!
//! C4 要 ccm 只「上报上下文 → 拿 argv → 设 env → `exec`」。摸底实测**今天搬不动**
//! （见功能件 §0）—— 但原文给的**两条理由，今天只剩一条站得住**。
//!
//! 〔墓碑 —— 第一条理由原话逐字：「本机没有后端在跑（F05b 未做）」。
//!  🔴 **它 2026-09-10 起判不了。** 先是前提翻了：F05b 已经做完并随 v3.7.0 发出去了 ——
//!  09-10 干净 win11 虚拟机上现打（PM，真安装包 + 真裸 exe 各一趟）：装出来那份
//!  `C:\Program Files\cc-monitor\` 下 `cc-monitor-backend.exe` **2 个进程在跑**、
//!  裸 `monitor.exe` 那份 **0 个**；`externalBin` 住 `src/bridge/tauri.sidecar.conf.json`，
//!  发版那一步 `--config` 注入，**刻意不进基础 `tauri.conf.json`**
//!  ⇒「基础配置里没有」≠「没配」。
//!  ⇒ 「本机没有后端在跑」**在装出来那份上不成立**；而「跑 ccm 的那台机上有没有」
//!  **按用户手里是哪一份产物分叉**（裸 exe / 开发树那份是 0），今天没人跑出过读数。
//!  **要什么才测得了**（缺一不可）：① 在装出来那份跑着的机器上起一个 ccm 会话；
//!  ② 读两样 —— 那台机上本机后端进程在不在 · ccm 那一跳**真去问了它**（而不是自己算 argv）。
//!  🔴 不许拿「装了就有」去替换「没有后端在跑」：前者也是一个没验过的新结论。〕
//!
//! **第二条理由 F05b 一个字没动，今天是它独自扛着这个结论**：远端那条 `--resolve` 的契约
//! **与仓外 aterm 冻结在 2026-07-18**、范围是 MVP（只产「首个候选 + `--resume <sid>`」），
//! 给不出 ccm 要的 argv，**为 ccm 扩它就是破坏那份冻结的契约**。
//! ⚠ 这一条**本轮没重验**（没去读那份冻结契约，也没跑过 `--resolve`）——
//! 它是原文自己的说法，如实标出来，别读成「有人 09-10 核过」。
//!
//! 那今天该做什么？——**把「搬之前必须成立的那个前提」钉住**：
//! **三份副本今天逐字一致。** 不一致的话搬完不知道搬没搬对，
//! 而且那种不一致**今天不会红**（三份各自的测试都过）。
//!
//! # 三份副本，形状还不一样
//!
//! | 副本 | 覆盖的 agent | 形态 |
//! |---|---|---|
//! | monitor Rust `adapter::for_kind` | claude + codex | trait 方法 |
//! | `shared/ccm` 的 `agent_*` | claude + codex | shell `case` |
//! | ~~前端 `AGENT_PROFILE`~~ | ~~**只有 claude**~~ | ~~单 profile 常量~~ |
//!
//! 🔴 **第三行 2026-09-12 起是墓碑（`K-R93`）。** 原文逐字：「⚠ 第三份**只有 claude**
//! —— 那不是漏，是它今天只服务 claude 那条路。」**那句辩解被推翻了**：`K-R54` 表第 11 行
//! 把它记成「同一件事两份实现」，而「只有 claude」正是那一格漏的。
//! 今天前端**不再持有副本**，它是这条链的末端：
//!
//! ```text
//! adapter.rs::agent_profile_facts
//!   └─(cargo test --lib export_bindings ＝ npm run gen:types)→
//!      src/generated/agent-profile-table.ts → src/agent-profile.ts
//! ```
//!
//! ⇒ **改后端那一份，前端拿到的值跟着变**；改了不重跑生成，门禁第六格 `generated` 红。
//! 本模块仍只对拍 Rust 那一轨与夹具，另加一条「取数口与夹具一致」（见下）。
//!
//! ⚠⚠ **TS 那一轨的描述原先整句两半都假**〔devbench F04, 08-10 订正〕。原文写着
//! 「TS 那轨由 `test/agent-profile.vitest.ts` 自己读同一份夹具」——
//! ① 那个文件**不存在**（真实文件名是 `tests/agent-profile-parity.vitest.ts`）；
//! ② 它也**不读这份夹具** —— 它 `readFileSync` 读的是 `adapter/claude_code.rs` 的**源码**，
//!    从里面抽字段值来对拍。夹具（`fixtures/agent-profile-golden.tsv`）只被 Rust 这一侧读。
//! ⇒ 如实改写：**TS 那轨与 Rust 这轨走的是两条不同的对拍路径**，
//! 一条读源码、一条读夹具，**它们之间没有共享的真相源**。
//! ★ 这正是「指向不存在的东西比没有注释更坏」那一族（铁律 14）：
//! 原措辞让人以为两侧共享一份夹具，于是不会去查那条链其实是断的。
//!
//! 🔴 **上面这一段 `K-R93`（09-12）也要接着改**：TS 那一轨**今天读这份夹具了** ——
//! `agent-profile-parity.vitest.ts` 新增的那个 `describe` 拿 `golden()` 与生成物逐格对拍
//! （4 个 key × 2 个 agent），另外 5 格（工具名 / 判活进程名）对 `adapter.rs` 的源码。
//! ⇒ 「两条路径、没有共享真相源」那句话**从 09-12 起只对一半**：共享的那一份就是这份夹具，
//! 而夹具管不到的那 5 格，两侧对的是同一份 Rust 源。
//!
//! # ★ 两项 ccm **独有**的决策，Rust 侧根本没有对侧
//!
//! `agent_has_identity`（有没有 per-PID session 文件 ⇒ 这个会话有没有身份）与
//! `agent_needs_bus_id`（要不要把 tmux 会话名注入 `CC_BUS_ID`）——
//! **monitor 的 `AgentAdapter` trait 里没有这两个方法。**
//!
//! ⇒ C4 要求 ccm 零决策，而这两个决策**今天没有地方可搬** ——
//! 得先在 Rust 侧建它们。**如实登记为 F06 的真实阻塞**（见 `THE_TWO_CCM_ONLY_DECISIONS`），
//! 不假装「搬一搬就好了」。

#[cfg(test)]
#[path = "../../../../../tests/bridge/backend/control/agent_profile_parity_tests.rs"]
mod tests;
