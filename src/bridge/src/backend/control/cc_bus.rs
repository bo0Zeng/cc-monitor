//! cc-bus 在 monitor 侧**只剩两句共用的说法**（机器怎么称呼 · 「那台后端太旧」）与 id 规则的再导出。
//!
//! 〔SH1 · V136 · 09-26〕驾驶舱读面（名册 · 收件箱）迁到界面经通道直接问那台后端（`bus-state` / `bus-inbox`，
//! 后端转调 cc-bus 新加的机器可读读命令）：monitor 这里原先那一整套 shell 读（本机 `bash -lc` ＋ 远端拨号链路，
//! `cat agents.tsv` / `tail` 收件箱、自述头、三个解析器、Windows 上找 `bash` 那一族）整份删了〔散文墓碑〕。
//! 坏行「跳过并计数、不因坏行丢好行」那条契约搬进了后端（`control/cc_bus.rs::parse_roster_tsv` 一族）。

use crate::copy_table::copy_text;

/// cc-bus id 合法性 —— 共享 crate 那一份的再导出（`INVARIANTS §47` ①；规则住 `shell_quote_core::bus_id_ok`）。
///
/// 违反此约束见 `src/doc/INVARIANTS.md` § 47（外部值拼进 shell / 交给对端之前本侧先过放行判定；〔TL2〕照「修改本文档」第 2 条补的反指）。
// 〔SH1〕读收件箱搬进后端之后生产段零处用它（留名给判据与登记表点）⇒ 非测试构建不报未用。
#[cfg_attr(not(test), allow(unused_imports))]
pub use shell_quote_core::bus_id_ok as is_valid_bus_id;

/// 一句话里怎么称呼这台机器 —— **纯函数**。
///
/// 🔴 它是「本机与远端两条路可观测行为等价」这条性质的**承重件**〔`KR98D1` 第③刀〕：
/// 两侧的每一句话都由**同一个** `format!` 渲染出来，唯一允许不同的就是这里回的这个称呼。
/// 一旦有人给本机另写一句「更亲切的」话，两条路就从措辞开始漂 ——
/// 而措辞正是用户唯一看得见的那一面。
pub(crate) fn machine_label(origin: &str) -> String {
    if origin == crate::backend::control::inbound_client::LOCAL_ORIGIN {
        copy_text("rsCcBus.machine.local", &[])
    } else {
        origin.to_string()
    }
}

/// 「这台的后端太旧」讲成人话 —— **能力协商的结论**，纯函数。
///
/// 〔C4e · 第四波 4C〕cc-bus 写面迁到界面之后它不再服务 cc-bus 自己，但别的几处发送端（`mcp_sync` · `panorama_call` ·
/// `skill_install` · `user_files`）仍借它说「那台后端太旧」—— 留在原住址，不为挪而挪。
///
/// # 🔴 它为什么必须与超时 / 断连长得不一样〔`KR98D2`〕
///
/// 「这台机器的后端没有这条命令」是一件**问得出答案**的事（`hello` 里那张命令表），
/// 而「超时」「连接断了」是**问不出答案**的事。把它们压成同一句「发消息失败」，
/// 就是本工作区最贵的那一形 —— **一个值装了两件事**：用户拿到它既不知道该升级，
/// 也不知道该重试，只能两样都试一遍。
pub(crate) fn describe_backend_too_old_for(origin: &str, _cmd: &str, outcome: &str) -> String {
    copy_text(
        "rsCcBus.tooOld.for",
        &[
            ("machine", &(machine_label(origin)).to_string()),
            ("outcome", &outcome.to_string()),
        ],
    )
}

#[cfg(test)]
#[path = "../../../../../tests/bridge/backend/control/cc_bus_tests.rs"]
mod tests;
