//! 尾段那张图（`history-tail`）：最新 N 可计行从哪个字节起 · 记号。

use super::locate::validate_session_path;
use crate::platform::fs::Ro;
use std::path::Path;

/// 尾段那张图：扫描图留着且没变 ⇒ 从它切；否则现数一遍行（只数行、不解析，比等整遍扫描快得多 —— 冷开时界面最先要它）。
pub(crate) fn tail_now(agent_home: &Path, jsonl_path: &str, n: usize) -> Result<Tail, String> {
    let target = validate_session_path(agent_home, jsonl_path)?;
    if let Some(map) = super::super::record_scan::global().peek(&target) {
        return Ok(map.tail(n));
    }
    let f = Ro::open(&target).map_err(|e| format!("open failed: {e}"))?;
    tail_plan_of(std::io::BufReader::new(f), n).map_err(|e| format!("scan failed: {e}"))
}

/// 记号（`history-tail` 的 `held.mark`）：`[start, end)` 那一段原始字节的位置与摘要。
/// 文件不到 `end` ⇒ `None`；那一段比一行的上限还长 ⇒ 空串（不记，下一次不核）。核的时候按同一个函数重算、比相等。
pub(crate) fn mark_at(
    agent_home: &Path,
    jsonl_path: &str,
    start: u64,
    end: u64,
    cap: usize,
) -> Result<Option<String>, String> {
    use std::io::{Read, Seek, SeekFrom};
    let target = validate_session_path(agent_home, jsonl_path)?;
    let mut f = Ro::open(&target).map_err(|e| format!("open failed: {e}"))?;
    let len = f.metadata().map_err(|e| format!("stat failed: {e}"))?.len();
    if end > len || start > end {
        return Ok(None);
    }
    if end - start > cap as u64 {
        return Ok(Some(String::new()));
    }
    f.seek(SeekFrom::Start(start))
        .map_err(|e| format!("seek failed: {e}"))?;
    let mut bytes = vec![0; (end - start) as usize];
    f.read_exact(&mut bytes)
        .map_err(|e| format!("read failed: {e}"))?;
    Ok(Some(format!(
        "{start}-{end}-{:016x}",
        super::body::line_hash(&bytes)
    )))
}

/// 记号里的那一段 `[start, end)`；认不出 ⇒ `None`。
pub(crate) fn mark_span(mark: &str) -> Option<(u64, u64)> {
    let mut it = mark.splitn(3, '-');
    let start = it.next()?.parse().ok()?;
    let end = it.next()?.parse().ok()?;
    it.next()?;
    Some((start, end))
}

/// 尾段那一趟扫描的结果：线上形住 `cmd-wire::session`（`history-tail` 的回包里那一块）。
pub(crate) use cmd_wire::session::TailPlan;

/// 尾段那张图 ＋ 最后一个可计行的起点（记号从它算到 `end`；一行都没有 ⇒ `None`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Tail {
    pub(crate) plan: TailPlan,
    pub(crate) last: Option<u64>,
}

/// 尾段那张图的扫描本体：读 `reader`（从字节 0 起）出 [`Tail`]。**纯 I/O 泛型**。
///
/// 流式单遍（环形缓冲只存最近 N 个可计行的字节偏移，O(N) 内存）：整文件读进内存在 Pi 级设备上对数百 MB 会话有 OOM 风险。
pub(crate) fn tail_plan_of<R: std::io::BufRead>(reader: R, n: usize) -> std::io::Result<Tail> {
    let mut recent: std::collections::VecDeque<u64> = std::collections::VecDeque::new();
    let keep = n.max(1);
    let mut total: u64 = 0;
    let mut lines = super::super::lines::Lines::new(reader, 0);
    while let Some(row) = lines.next()? {
        if !super::super::lines::counts(row.body()) {
            continue; // 空行不计（与实时跟读同一个判定：`lines::counts`）
        }
        total += 1;
        recent.push_back(row.at);
        if recent.len() > keep {
            recent.pop_front();
        }
    }
    let complete_end = lines.end();
    let tail_from = total - recent.len() as u64;
    let split_at = recent.front().copied().unwrap_or(complete_end);
    Ok(Tail {
        plan: TailPlan {
            total,
            tail_from,
            split_at,
            end: complete_end,
        },
        last: recent.back().copied(),
    })
}

/// 纯函数：把文件字节按"最新 N 可计行优先"切成 (meta 行, 尾段, 头段)。
/// 只处理到最后一个 `\n`（torn 残尾不进任何段——F14 口径）。
/// 生产路径已流式化（read_session_tail，审计 D 内存修订）；本函数保留为
/// 口径锚点（tail_tests 锚定语义），流式版与它的等价性由本机行为验证对账
/// （真实 18MB 会话：meta/字节输出逐段一致，见 Batch9 feature 留档）。
/// **仅测**（生产走流式版、不调本函数）→ `#[cfg(test)]` 不进生产二进制。
#[cfg(test)]
pub(crate) fn split_tail(bytes: &[u8], n: usize) -> (String, &[u8], &[u8]) {
    let complete_end = bytes.iter().rposition(|&b| b == b'\n').map_or(0, |i| i + 1);
    let complete = &bytes[..complete_end];
    // 收集每个可计行的起始字节偏移（口径 = watcher::read_new_lines：BOM/全空白跳过）
    let mut starts: Vec<usize> = Vec::new();
    let mut pos = 0usize;
    for line in complete.split_inclusive(|&b| b == b'\n') {
        if super::super::lines::counts(&line[..line.len() - 1]) {
            starts.push(pos);
        }
        pos += line.len();
    }
    let total = starts.len();
    let tail_from = total.saturating_sub(n.max(1));
    let meta =
        format!("{{\"kind\":\"snapshot_meta\",\"total\":{total},\"tail_from\":{tail_from}}}\n");
    let split_at = starts.get(tail_from).copied().unwrap_or(complete_end);
    (meta, &complete[split_at..], &complete[..split_at])
}

#[cfg(test)]
#[path = "../../../../tests/backend/observe/query/tail_tests.rs"]
mod tests;
