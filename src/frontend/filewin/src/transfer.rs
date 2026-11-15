//! **一组搬到另一台**：上传（本机 → 这一台）· 下载（这一台 → 本机）· 复制到另一台，都是同一条 `transfer-copy`。
//!
//! ```text
//! 发   source::ask_batch(line, 本机, "transfer-copy", {from, to, items, to_dir, on_exists?, decide?, ticket}) → BatchReply
//! 看   source::watch_cells(line, 本机, "files-batch/<票>", …)                                              → 一格一格的快照
//! 撤   source::ask_typed(line, 本机, files-batch-stop{ticket})
//! ```
//!
//! - 寻址恒是本机：编排（两头各选哪条腿 · 同名先探 · 撞 `stale` 重传 · 本机落名 · 剩多久）全在本机后端
//!   （`control/transfer_copy.rs`）；窗口只填两头是哪台、搬哪几项、落到哪个目录，一组一行「进度」（[`super::batch`]）。
//! - 路径一路是原始字节（`WirePath`）：有损名照字节搬，落到本机的那一段名字由本机后端改成这台认的合法名。

use super::source::Origin;
use cmd_wire::files::WirePath;
use cmd_wire::transfer::{CopyFrom, TransferCopyReq};

/// 一组搬到另一台的那一条。
pub const CMD_TRANSFER: &str = <TransferCopyReq as cmd_wire::Call>::NAME;

/// 一次「那儿有没有东西」的往返上限（调用方给的期限）。
pub const PROBE_BUDGET: std::time::Duration = std::time::Duration::from_secs(10);

/// 一组的入参：`from` 那台的 `items` 搬进 `to` 那台的 `to_dir`（票由看板铸、发之前写进去，[`super::batch::BatchBoard::new`]）。
pub fn copy_req(
    from: &Origin,
    to: &Origin,
    items: Vec<CopyFrom>,
    to_dir: WirePath,
    on_exists: Option<cmd_wire::batch::OnExists>,
) -> TransferCopyReq {
    TransferCopyReq {
        from: from.clone(),
        to: to.clone(),
        items,
        to_dir,
        on_exists,
        decide: None,
        ticket: None,
    }
}

/// 那一组交给看板的计划：`names` 与 `req.items` 一一对上，`into` 是标题里「到哪」那一段，`machine` ＝ 窗口看着的那台（断开那一行写它）。
pub fn plan(
    verb: batch_core::Verb,
    req: &TransferCopyReq,
    names: Vec<String>,
    into: String,
    machine: String,
) -> batch_core::Plan {
    batch_core::Plan {
        verb,
        cmd: CMD_TRANSFER,
        args: serde_json::to_value(req).unwrap_or_default(),
        names,
        into: Some(into),
        machine,
    }
}

/// **上传**：本机那几份（拖进来的 · 选到的）放进 `here` 那台的 `dir`（`dir_raw` ＝ 目录名不是 UTF-8 时的原始字节）。
/// 拿不到末段名的（`/` · 空串）不进这一组、不编一个名字出来；一份都没有 ⇒ `None`。
pub fn upload_plan(
    here: &Origin,
    machine: String,
    local_paths: &[String],
    dir: &str,
    dir_raw: Option<&[u8]>,
) -> Option<batch_core::Plan> {
    let mut items = Vec::new();
    let mut names = Vec::new();
    for p in local_paths {
        let Some(name) = std::path::Path::new(p)
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .filter(|s| !s.is_empty())
        else {
            continue;
        };
        items.push(CopyFrom {
            path: WirePath::from(p.as_str()),
            name: None,
        });
        names.push(name);
    }
    if items.is_empty() {
        return None;
    }
    let to_dir = match dir_raw {
        Some(b) => WirePath(b.to_vec()),
        None => WirePath::from(dir),
    };
    let req = copy_req(&Origin::local(), here, items, to_dir, None);
    Some(plan(
        batch_core::Verb::Upload,
        &req,
        names,
        dir.to_string(),
        machine,
    ))
}

#[cfg(test)]
#[path = "../../../../tests/frontend/filewin/transfer_tests.rs"]
pub(crate) mod tests;
