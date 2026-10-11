//! cc-bus 在 monitor 侧**只剩一句共用的说法**（机器怎么称呼）与 id 规则的再导出。
//!

use crate::copy_table::copy_text;

/// cc-bus id 合法性 —— 共享 crate 那一份的再导出（`INVARIANTS §47` ①；规则住 `shell_quote_core::bus_id_ok`）。
///
/// 违反此约束见 `src/doc/INVARIANTS.md` § 47（外部值拼进 shell / 交给对端之前本侧先过放行判定；照「修改本文档」第 2 条补的反指）。
// 读收件箱搬进后端之后生产段零处用它（留名给判据与登记表点）⇒ 非测试构建不报未用。
#[cfg_attr(not(test), allow(unused_imports))]
pub use shell_quote_core::bus_id_ok as is_valid_bus_id;

/// 一句话里怎么称呼这台机器 —— **纯函数**。
///
/// 🔴 它是「本机与远端两条路可观测行为等价」这条性质的**承重件**〔`KR98D1` 第③刀〕：
/// 两侧的每一句话都由**同一个** `format!` 渲染出来，唯一允许不同的就是这里回的这个称呼。
/// 一旦有人给本机另写一句「更亲切的」话，两条路就从措辞开始漂 ——
/// 而措辞正是用户唯一看得见的那一面。
pub(crate) fn machine_label(origin: &str) -> String {
    if origin == crate::inbound_client::LOCAL_ORIGIN {
        copy_text("rsCcBus.machine.local", &[])
    } else {
        origin.to_string()
    }
}

#[cfg(test)]
#[path = "../../../../tests/frontend/shell/cc_bus_tests.rs"]
mod tests;
