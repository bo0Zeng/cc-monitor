//! 分层锚点解析(F04):路径(直连 / git 改名跟随)→ 文件内按符号名 → 全仓按名兜底。
//! 现场解析**工作树**(反映当前代码),非索引快照。非 Resolved 一律显式暴露。

use crate::model::{Anchor, AnchorState, Location, Resolution, Symbol};
use crate::{git, scan, symbols};
use std::path::Path;

/// 分发:有 `quote`(块级字段)→ 块级解析(F09);否则符号级(F04)。
pub fn resolve(repo: &Path, anchor: &Anchor) -> Resolution {
    if anchor.quote.as_deref().is_some_and(|q| !q.is_empty()) {
        return resolve_block(repo, anchor);
    }
    resolve_symbol(repo, anchor)
}

fn resolve_symbol(repo: &Path, anchor: &Anchor) -> Resolution {
    // 第 1 层:定位文件
    let current_file: Option<String> = if git::file_exists(repo, &anchor.file) {
        Some(anchor.file.clone())
    } else {
        git::follow_rename(repo, &anchor.file)
    };
    let renamed = matches!(&current_file, Some(f) if f != &anchor.file);

    // 第 2 层:文件内按符号名
    if let Some(f) = &current_file {
        let syms = fresh_symbols_in_file(repo, f);
        if let Some(s) = syms.iter().find(|s| s.name == anchor.symbol) {
            let state = if renamed {
                AnchorState::MovedFileRenamed
            } else {
                AnchorState::Resolved
            };
            return Resolution {
                state,
                location: Some(Location {
                    file: f.clone(),
                    line: s.start_line,
                }),
                content_changed: anchor.orig_body_hash.map(|h| h != s.body_hash),
                candidates: vec![],
            };
        }
    }

    // 第 3 层:全仓按名兜底
    let all = fresh_all_symbols(repo);
    let matches: Vec<&Symbol> = all.iter().filter(|s| s.name == anchor.symbol).collect();
    match matches.len() {
        1 => {
            let s = matches[0];
            Resolution {
                state: AnchorState::MovedToOtherFile,
                location: Some(Location {
                    file: s.file.clone(),
                    line: s.start_line,
                }),
                content_changed: anchor.orig_body_hash.map(|h| h != s.body_hash),
                candidates: vec![],
            }
        }
        0 => Resolution {
            state: AnchorState::Orphaned,
            location: None,
            content_changed: None,
            candidates: vec![],
        },
        _ => Resolution {
            state: AnchorState::Ambiguous,
            location: None,
            content_changed: None,
            candidates: matches
                .iter()
                .map(|s| Location {
                    file: s.file.clone(),
                    line: s.start_line,
                })
                .collect(),
        },
    }
}

fn fresh_symbols_in_file(repo: &Path, rel: &str) -> Vec<Symbol> {
    match std::fs::read_to_string(repo.join(rel)) {
        Ok(src) => symbols::symbols_in_source(&src, rel),
        Err(_) => vec![],
    }
}

fn fresh_all_symbols(repo: &Path) -> Vec<Symbol> {
    let mut out = Vec::new();
    for (rel, _lang) in scan::source_files(repo) {
        out.extend(fresh_symbols_in_file(repo, &rel));
    }
    out
}

// ── 块级锚点(F09,Hypothesis 式 TextQuoteSelector:quote + 上下文消歧 + 模糊兜底)──
// node_path(AST 子节点)本轮保留不解析;quote/模糊已足够抗漂移。

pub(crate) fn hash_str(s: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    s.hash(&mut h);
    h.finish()
}

fn resolve_block(repo: &Path, anchor: &Anchor) -> Resolution {
    let quote = anchor.quote.as_deref().unwrap_or_default();
    // 定位文件(git 改名跟随,复用 F04)
    let file = if git::file_exists(repo, &anchor.file) {
        Some(anchor.file.clone())
    } else {
        git::follow_rename(repo, &anchor.file)
    };
    let file = match file {
        Some(f) => f,
        None => return orphaned(),
    };
    let content = std::fs::read_to_string(repo.join(&file)).unwrap_or_default();

    let hits = find_exact(
        &content,
        quote,
        anchor.prefix.as_deref(),
        anchor.suffix.as_deref(),
    );
    match hits.len() {
        1 => Resolution {
            state: AnchorState::Resolved,
            location: Some(Location {
                file,
                line: offset_to_line(&content, hits[0]),
            }),
            // content_hash 校验:精确匹配 → 内容未变
            content_changed: anchor.content_hash.map(|h| hash_str(quote) != h),
            candidates: vec![],
        },
        0 => {
            // 模糊:quote 变了,用 prefix/suffix 括出漂移块
            match fuzzy_region(&content, anchor.prefix.as_deref(), anchor.suffix.as_deref()) {
                Some(off) => Resolution {
                    state: AnchorState::Resolved,
                    location: Some(Location {
                        file,
                        line: offset_to_line(&content, off),
                    }),
                    content_changed: Some(true), // 内容漂移,待复查
                    candidates: vec![],
                },
                None => orphaned(),
            }
        }
        _ => Resolution {
            state: AnchorState::Ambiguous,
            location: None,
            content_changed: None,
            candidates: hits
                .iter()
                .map(|&o| Location {
                    file: file.clone(),
                    line: offset_to_line(&content, o),
                })
                .collect(),
        },
    }
}

fn orphaned() -> Resolution {
    Resolution {
        state: AnchorState::Orphaned,
        location: None,
        content_changed: None,
        candidates: vec![],
    }
}

/// quote 的所有出现;多于一处时用 prefix/suffix 消歧。
fn find_exact(
    content: &str,
    quote: &str,
    prefix: Option<&str>,
    suffix: Option<&str>,
) -> Vec<usize> {
    if quote.is_empty() {
        return vec![];
    }
    let mut all = Vec::new();
    let mut start = 0;
    while let Some(rel) = content[start..].find(quote) {
        let off = start + rel;
        all.push(off);
        // 按 quote 长度步进(char 边界安全 + 非重叠;块级不需重叠匹配)
        start = off + quote.len();
    }
    if all.len() <= 1 {
        return all;
    }
    all.into_iter()
        .filter(|&off| {
            let end = off + quote.len();
            let pre_ok = prefix
                .map(|p| p.is_empty() || content[..off].ends_with(p))
                .unwrap_or(true);
            let suf_ok = suffix
                .map(|s| s.is_empty() || content[end..].starts_with(s))
                .unwrap_or(true);
            pre_ok && suf_ok
        })
        .collect()
}

/// quote 找不到时,用 prefix/suffix 括出漂移块的起点。支持单侧上下文(首/尾行块)。
fn fuzzy_region(content: &str, prefix: Option<&str>, suffix: Option<&str>) -> Option<usize> {
    let p = prefix.filter(|s| !s.is_empty());
    let s = suffix.filter(|s| !s.is_empty());
    match (p, s) {
        (Some(p), Some(s)) => {
            let after = content.find(p)? + p.len();
            content[after..].find(s)?; // suffix 须在 prefix 之后
            Some(after)
        }
        (Some(p), None) => Some(content.find(p)? + p.len()), // 只有 prefix:块在其后
        (None, Some(s)) => {
            // 只有 suffix(首行块):块在 suffix 之前。返回块最后一行(紧邻 suffix 上方)
            // 的起点,而非 suffix 自身的偏移(后者会把行号指到块的下一行)。
            let pos = content.find(s)?;
            let head = &content[..pos];
            Some(
                head.trim_end_matches('\n')
                    .rfind('\n')
                    .map(|i| i + 1)
                    .unwrap_or(0),
            )
        }
        (None, None) => None,
    }
}

fn offset_to_line(content: &str, offset: usize) -> usize {
    content[..offset].bytes().filter(|&b| b == b'\n').count() + 1
}

/// 抓块:1-based 行范围 [start,end] → (块文本, 上一行 prefix, 下一行 suffix)。char 边界安全。
pub(crate) fn extract_block(
    content: &str,
    start_line: usize,
    end_line: usize,
) -> Option<(String, String, String)> {
    let ranges = line_ranges(content);
    if start_line < 1 || end_line < start_line || end_line > ranges.len() {
        return None;
    }
    let bs = ranges[start_line - 1].0;
    let be = ranges[end_line - 1].1;
    let block = content[bs..be].to_string();
    let prefix = if start_line > 1 {
        content[ranges[start_line - 2].0..bs].to_string()
    } else {
        String::new()
    };
    let suffix = if end_line < ranges.len() {
        content[be..ranges[end_line].1].to_string()
    } else {
        String::new()
    };
    Some((block, prefix, suffix))
}

/// 每行 (start,end) 字节偏移(end 含尾 `\n`)。
fn line_ranges(content: &str) -> Vec<(usize, usize)> {
    let mut ranges = Vec::new();
    let mut start = 0;
    for (i, b) in content.bytes().enumerate() {
        if b == b'\n' {
            ranges.push((start, i + 1));
            start = i + 1;
        }
    }
    if start < content.len() {
        ranges.push((start, content.len()));
    }
    ranges
}
