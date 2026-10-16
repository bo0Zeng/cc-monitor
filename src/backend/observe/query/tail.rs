//! 尾段那张图（`history-tail`）：最新 N 可计行从哪个字节起 · 文件身份。

use super::locate::validate_session_path;
use crate::platform::fs::{Ro, RoFile};
use std::path::Path;

/// 尾段那张图：扫描图留着且没变 ⇒ 从它切；否则现数一遍行（只数行、不解析，比等整遍扫描快得多 —— 冷开时界面最先要它）。
pub(crate) fn tail_now(agent_home: &Path, jsonl_path: &str, n: usize) -> Result<TailPlan, String> {
    let target = validate_session_path(agent_home, jsonl_path)?;
    if let Some(map) = super::super::record_scan::global().peek(&target) {
        let f = Ro::open(&target).map_err(|e| format!("open failed: {e}"))?;
        let head =
            first_counted(std::io::BufReader::new(f)).map_err(|e| format!("scan failed: {e}"))?;
        return Ok(map.tail(n, file_id(head.as_deref())));
    }
    tail_plan(agent_home, jsonl_path, n).map(|(plan, _)| plan)
}

/// 文件身份（`history-tail.file`）：第一个可计行正文的摘要（[`super::body::line_hash`]，16 位十六进制）；没有 ⇒ 空串。
pub(crate) fn file_id(first: Option<&[u8]>) -> String {
    first.map_or_else(String::new, |b| {
        format!("{:016x}", super::body::line_hash(b))
    })
}

/// 从头读到第一个可计行（不含 `\n`）；一行都没有 ⇒ `None`。
fn first_counted<R: std::io::BufRead>(r: R) -> std::io::Result<Option<Vec<u8>>> {
    let mut lines = super::super::lines::Lines::new(r, 0);
    while let Some(row) = lines.next()? {
        if super::super::lines::counts(row.body()) {
            return Ok(Some(row.body().to_vec()));
        }
    }
    Ok(None)
}

/// 尾段那一趟扫描的结果：线上形住 `cmd-wire::session`（`history-tail` 的回包）。
pub(crate) use cmd_wire::session::TailPlan;

/// 扫一遍，出 [`TailPlan`] 与已打开的那份文件（调用方按区间去读）。
pub(crate) fn tail_plan(
    agent_home: &Path,
    jsonl_path: &str,
    n: usize,
) -> Result<(TailPlan, RoFile), String> {
    let target = validate_session_path(agent_home, jsonl_path)?;
    // 审计 D：整文件 std::fs::read 在 Pi 级设备上对数百 MB 会话有 OOM 风险
    // （更早的整份透传是 io::copy 流式）——改单遍流式扫描（环形缓冲只存
    // 最近 N 个可计行的字节偏移，O(N) 内存）+ 两次 seek 范围拷贝。
    let f = Ro::open(&target).map_err(|e| format!("open failed: {e}"))?;
    let mut reader = std::io::BufReader::new(f);
    let plan = tail_plan_of(&mut reader, n).map_err(|e| format!("scan failed: {e}"))?;
    Ok((plan, reader.into_inner()))
}

/// [`tail_plan`] 的扫描本体：读 `reader`（从字节 0 起）出 [`TailPlan`]。**纯 I/O 泛型**。
pub(crate) fn tail_plan_of<R: std::io::BufRead>(reader: R, n: usize) -> std::io::Result<TailPlan> {
    let mut recent: std::collections::VecDeque<u64> = std::collections::VecDeque::new();
    let keep = n.max(1);
    let mut total: u64 = 0;
    let mut head: Option<Vec<u8>> = None;
    let mut lines = super::super::lines::Lines::new(reader, 0);
    while let Some(row) = lines.next()? {
        if !super::super::lines::counts(row.body()) {
            continue; // 空行不计（与实时跟读同一个判定：`lines::counts`）
        }
        if head.is_none() {
            head = Some(row.body().to_vec());
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
    Ok(TailPlan {
        total,
        tail_from,
        split_at,
        end: complete_end,
        file: file_id(head.as_deref()),
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
