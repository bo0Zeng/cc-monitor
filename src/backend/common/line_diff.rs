//! 两份文本逐行比：行差的算法只此一份（写类工具那一张卡的行差 · 「待办」那几件要贴的改法都调它），线上形是 [`LineDiff`]。
//!
//! 行尾风格不算差别（`\r\n` · `\r` 当 `\n`，末尾恰好一个换行不成行）；行数之积超了 [`DIFF_CELLS_MAX`] ⇒ 不逐行比，出整删 ＋ 整增，带降级那一句；
//! 一行进行差时过长的截到 [`DIFF_LINE_MAX`] 字（[`cell`]）。只算不写，不认那是谁的文件。

use crate::common::cells::Words;
use cmd_wire::record::{LineDiff, LineDiffKind, LineDiffRow};

/// 行差收着时先画几行（两端同一个数）。
const DIFF_SHOWN: usize = 400;
/// 行差里一行至多留多少字（按字符），截了以「…」收尾：压缩过的单行几十万字不进界面。
const DIFF_LINE_MAX: usize = 2000;
/// 从改前改后逐行比的格数上界（改前行数 × 改后行数）：超了不比，出整删 ＋ 整增（数照这一形），行差带降级那一句（[`LineDiff::notice`]）。
const DIFF_CELLS_MAX: usize = 4_000_000;

/// 一行正文进行差的样子：去掉行尾的 `\r`，过长截到 [`DIFF_LINE_MAX`] 字。
pub fn cell(text: &str) -> String {
    let t = text.strip_suffix('\r').unwrap_or(text);
    match t.char_indices().nth(DIFF_LINE_MAX) {
        Some((i, _)) => format!("{}…", &t[..i]),
        None => t.to_string(),
    }
}

/// 攒好的行 ＋ 整份的增删数 ⇒ 行差：收着画几行。`stat` ＝「+N −M」那一句（调用方照过程那一行右侧那一句的写法给）；
/// `notice` ＝ 降级了的那一句（[`ops`] 超了格数上界时写的），照带。
pub fn assemble(
    rows: Vec<LineDiffRow>,
    added: u32,
    removed: u32,
    truncated: bool,
    notice: Option<Words>,
    stat: Words,
) -> LineDiff {
    LineDiff {
        shown: u32::try_from(rows.len().min(DIFF_SHOWN)).unwrap_or(u32::MAX),
        rows,
        truncated,
        added,
        removed,
        stat,
        notice,
    }
}

/// 改前改后的原文 ⇒ 逐行的种类（正文不截）。
/// 一样 ⇒ 全是没变；改前空 ⇒ 全是加的；改后空 ⇒ 全是删的；行数之积超了 [`DIFF_CELLS_MAX`] ⇒ 整删 ＋ 整增，连降级那一句；
/// 余下按最长公共子序列（迭代的表 ＋ 迭代回溯，上千行不靠调用栈），同长时删的排在加的前面。
/// 第二格 ＝ 降级那一句（没降级 ⇒ `None`）。
pub fn ops(old: &str, new: &str) -> (Vec<(LineDiffKind, String)>, Option<Words>) {
    let norm = |s: &str| {
        let t = s.replace("\r\n", "\n").replace('\r', "\n");
        match t.strip_suffix('\n') {
            Some(x) => x.to_string(),
            None => t,
        }
    };
    let (o, n) = (norm(old), norm(new));
    let split = |s: &str| -> Vec<String> {
        if s.is_empty() {
            Vec::new()
        } else {
            s.split('\n').map(str::to_string).collect()
        }
    };
    let (a, b) = (split(&o), split(&n));
    let all = |v: Vec<String>, k: LineDiffKind| v.into_iter().map(move |l| (k, l));
    if o == n {
        return (all(a, LineDiffKind::Ctx).collect(), None);
    }
    let (m, w) = (a.len(), b.len());
    let past_cap = m.saturating_mul(w) > DIFF_CELLS_MAX;
    if m == 0 || w == 0 || past_cap {
        let notice = past_cap.then(|| {
            let (m, w) = (m.to_string(), w.to_string());
            Words(copy_core::copy_text(
                "cards.diff.coarse",
                &[("old", &m), ("new", &w)],
            ))
        });
        let ops = all(a, LineDiffKind::Del)
            .chain(all(b, LineDiffKind::Add))
            .collect();
        return (ops, notice);
    }
    let width = w + 1;
    let mut c = vec![0u32; (m + 1) * width];
    for i in 1..=m {
        for j in 1..=w {
            c[i * width + j] = if a[i - 1] == b[j - 1] {
                c[(i - 1) * width + j - 1] + 1
            } else {
                c[(i - 1) * width + j].max(c[i * width + j - 1])
            };
        }
    }
    let mut ops = Vec::with_capacity(m + w);
    let (mut i, mut j) = (m, w);
    while i > 0 && j > 0 {
        if a[i - 1] == b[j - 1] {
            ops.push((LineDiffKind::Ctx, a[i - 1].clone()));
            i -= 1;
            j -= 1;
        } else if c[i * width + j - 1] >= c[(i - 1) * width + j] {
            ops.push((LineDiffKind::Add, b[j - 1].clone()));
            j -= 1;
        } else {
            ops.push((LineDiffKind::Del, a[i - 1].clone()));
            i -= 1;
        }
    }
    ops.extend(a[..i].iter().rev().map(|l| (LineDiffKind::Del, l.clone())));
    ops.extend(b[..j].iter().rev().map(|l| (LineDiffKind::Add, l.clone())));
    ops.reverse();
    (ops, None)
}

/// [`ops`] 的每一行带上真行号（两份都是整份文件时用：没变的两边都有，删的只有改前，加的只有改后；1 起）。正文不截。
pub(crate) fn numbered(old: &str, new: &str) -> (Vec<LineDiffRow>, Option<Words>) {
    let (ops, notice) = ops(old, new);
    let (mut o, mut n) = (0u32, 0u32);
    let rows = ops
        .into_iter()
        .map(|(kind, text)| {
            let (old_no, new_no) = match kind {
                LineDiffKind::Del => {
                    o += 1;
                    (Some(o), None)
                }
                LineDiffKind::Add => {
                    n += 1;
                    (None, Some(n))
                }
                _ => {
                    o += 1;
                    n += 1;
                    (Some(o), Some(n))
                }
            };
            LineDiffRow {
                kind,
                text,
                old_no,
                new_no,
            }
        })
        .collect();
    (rows, notice)
}
