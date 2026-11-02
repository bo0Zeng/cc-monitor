//! **历史那几行上按哪台换说法的两格**：「打开目录」出不出（[`reveal_of`]）· 删除确认的正文（[`delete_ask`]）。
//!
//! 两个调用方：本机后端写历史清单的行（`history/history_list.rs::row_of`）· 本机后端几台搜索结果合完之后逐行补（`faces/search_face.rs`）。
//! 出口照摆（`reveal` 为真才出那一项 · 确认框正文照抄 `deleteAsk`），不另判本机 / 远端。
//! 两格都是「问的这一端眼里的」：`origin` 是这台 ⇒ 目录就在这一端、正文不点名机器。

use cmd_wire::cells::Words;
use cmd_wire::listing::CanDelete;
use cmd_wire::Machine;
use copy_core::copy_text;

/// 「打开目录」出不出：答这一行的是问的这一端、目录非空。
pub fn reveal_of(origin: &Machine, project_path: &str) -> bool {
    origin.is_local() && !project_path.is_empty()
}

/// 删除确认的正文：这台的记录不点名机器、那一台的点名；说不清在不在跑 ⇒ 换行再接一句。
pub fn delete_ask(origin: &Machine, delete: CanDelete) -> Words {
    let head = if origin.is_local() {
        copy_text("history.delete.body", &[])
    } else {
        copy_text(
            "history.delete.bodyRemote",
            &[("machine", &origin.to_string())],
        )
    };
    Words(match delete {
        CanDelete::Unsure => format!("{head}\n{}", copy_text("history.delete.unsure", &[])),
        CanDelete::Yes | CanDelete::Live => head,
    })
}

#[cfg(test)]
#[path = "../../../../tests/common/face-core/history_asks_tests.rs"]
mod tests;
