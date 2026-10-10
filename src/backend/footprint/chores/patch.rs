//! 按一份 JSON 设置文件**现在的内容**算改法（「待办」里要贴的那几件用）：合好的整份 ＋ 行号 ＋ diff。只算不写。
//!
//! 在原文上就地插 / 换，不重排：别的设置、键序、缩进、注释以外的一切原样保留（解析再整份写回会按字母重排键）。
//! 合好之后再解析一遍核对：不是合法 JSON ⇒ 不给改法（`None`），调用方只说「这份文件读不懂」。

use serde_json::Value;

/// diff 的一行：原样（带现在的行号）· 删（带行号）· 加。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum DiffLine {
    Same(usize, String),
    Del(usize, String),
    Add(String),
}

/// 一份改法。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Plan {
    /// 合好的整份。
    pub whole: String,
    /// 改动那几行，上下各带一行原文。
    pub diff: Vec<DiffLine>,
    /// 改动从哪一行之后开始（1 起；文件开头 ⇒ 0）。
    pub at_line: usize,
}

/// 把 `path` 那一格设成 `v`（沿途没有的对象一并建）。
pub(crate) fn set_member(src: &str, path: &[&str], v: &Value) -> Option<Plan> {
    edit(src, path, v, false)
}

/// 往 `path` 那个数组末尾加一项（数组 / 沿途对象没有就建）。
pub(crate) fn push_item(src: &str, path: &[&str], item: &Value) -> Option<Plan> {
    edit(src, path, item, true)
}

fn edit(src: &str, path: &[&str], v: &Value, push: bool) -> Option<Plan> {
    let (last, parents) = path.split_last()?;
    let leaf = |v: &Value| {
        if push {
            Value::Array(vec![v.clone()])
        } else {
            v.clone()
        }
    };
    if src.trim().is_empty() {
        let mut whole = v.clone();
        whole = leaf(&whole);
        whole = nest(&[*last], whole);
        let whole = nest(parents, whole);
        let text = serde_json::to_string_pretty(&whole).ok()? + "\n";
        return finish(src, text);
    }
    let root: Value = serde_json::from_str(src).ok()?;
    if !root.is_object() {
        return None;
    }
    let b = src.as_bytes();
    let mut obj = skip_ws(b, 0);
    for (i, step) in parents.iter().enumerate() {
        match members(b, obj)?.into_iter().find(|m| m.key == *step) {
            Some(m) if b.get(m.value) == Some(&b'{') => obj = m.value,
            Some(_) => return None,
            None => {
                let rest = nest(&path[i + 1..], leaf(v));
                return finish(src, insert_member(src, obj, step, &rest)?);
            }
        }
    }
    let found = members(b, obj)?.into_iter().find(|m| m.key == *last);
    let text = match (found, push) {
        (None, _) => insert_member(src, obj, last, &leaf(v))?,
        (Some(m), false) => {
            let ind = line_indent(src, m.key_at);
            format!("{}{}{}", &src[..m.value], pretty_at(v, &ind), &src[m.end..])
        }
        (Some(m), true) => {
            if b.get(m.value) != Some(&b'[') {
                return None;
            }
            append_item(src, m.value, m.end, v)?
        }
    };
    finish(src, text)
}

/// 核对合好的那份、算 diff。
fn finish(src: &str, whole: String) -> Option<Plan> {
    serde_json::from_str::<Value>(&whole).ok()?;
    let (diff, at_line) = line_diff(src, &whole);
    Some(Plan {
        whole,
        diff,
        at_line,
    })
}

fn nest(path: &[&str], v: Value) -> Value {
    path.iter().rev().fold(v, |acc, k| {
        let mut m = serde_json::Map::new();
        m.insert((*k).to_string(), acc);
        Value::Object(m)
    })
}

/// 对象里的一个成员：键 · 键引号起点 · 值起点 · 值终点（不含）。
struct Member {
    key: String,
    key_at: usize,
    value: usize,
    end: usize,
}

/// `{` 在 `at` 的那个对象的成员。
fn members(b: &[u8], at: usize) -> Option<Vec<Member>> {
    if b.get(at) != Some(&b'{') {
        return None;
    }
    let mut out = Vec::new();
    let mut i = skip_ws(b, at + 1);
    if b.get(i) == Some(&b'}') {
        return Some(out);
    }
    loop {
        let key_at = i;
        let key_end = skip_string(b, i)?;
        let key: String = serde_json::from_slice(&b[key_at..key_end]).ok()?;
        i = skip_ws(b, key_end);
        if b.get(i) != Some(&b':') {
            return None;
        }
        let value = skip_ws(b, i + 1);
        let end = skip_value(b, value)?;
        out.push(Member {
            key,
            key_at,
            value,
            end,
        });
        i = skip_ws(b, end);
        match b.get(i) {
            Some(b',') => i = skip_ws(b, i + 1),
            Some(b'}') => return Some(out),
            _ => return None,
        }
    }
}

/// 对象（`{` 在 `obj`）开头插一个成员。
fn insert_member(src: &str, obj: usize, key: &str, v: &Value) -> Option<String> {
    let b = src.as_bytes();
    let first = skip_ws(b, obj + 1);
    let outer = line_indent(src, obj);
    let key_text = serde_json::to_string(key).ok()?;
    let empty = b.get(first) == Some(&b'}');
    let gap = &src[obj + 1..first];
    let text = if empty {
        let ind = format!("{outer}  ");
        format!("\n{ind}{key_text}: {}\n{outer}", pretty_at(v, &ind))
    } else if let Some(nl) = gap.rfind('\n') {
        let ind = &gap[nl + 1..];
        format!("\n{ind}{key_text}: {},", pretty_at(v, ind))
    } else {
        format!("{key_text}: {}, ", serde_json::to_string(v).ok()?)
    };
    let cut = if empty { first } else { obj + 1 };
    Some(format!("{}{}{}", &src[..obj + 1], text, &src[cut..]))
}

/// 数组（`[` 在 `open`，值终点 `end`）末尾加一项。
fn append_item(src: &str, open: usize, end: usize, v: &Value) -> Option<String> {
    let b = src.as_bytes();
    let close = end - 1;
    let outer = line_indent(src, open);
    let first = skip_ws(b, open + 1);
    if first == close {
        let ind = format!("{outer}  ");
        return Some(format!(
            "{}[\n{ind}{}\n{outer}]{}",
            &src[..open],
            pretty_at(v, &ind),
            &src[end..]
        ));
    }
    // 最后一项的终点：从 `]` 往回跳过空白。
    let mut last_end = close;
    while last_end > open && (b[last_end - 1] as char).is_whitespace() {
        last_end -= 1;
    }
    let gap = &src[open + 1..first];
    let ind = gap.rfind('\n').map(|nl| gap[nl + 1..].to_string());
    let text = match &ind {
        Some(ind) => format!(",\n{ind}{}", pretty_at(v, ind)),
        None => format!(", {}", serde_json::to_string(v).ok()?),
    };
    Some(format!("{}{}{}", &src[..last_end], text, &src[last_end..]))
}

/// 一个值写成多行时，第二行起按 `ind` 缩进。
fn pretty_at(v: &Value, ind: &str) -> String {
    let p = serde_json::to_string_pretty(v).unwrap_or_default();
    p.replace('\n', &format!("\n{ind}"))
}

/// `at` 所在那一行开头的空白。
fn line_indent(src: &str, at: usize) -> String {
    let start = src[..at].rfind('\n').map_or(0, |i| i + 1);
    src[start..]
        .chars()
        .take_while(|c| *c == ' ' || *c == '\t')
        .collect()
}

fn skip_ws(b: &[u8], mut i: usize) -> usize {
    while i < b.len() && (b[i] as char).is_ascii_whitespace() {
        i += 1;
    }
    i
}

fn skip_string(b: &[u8], i: usize) -> Option<usize> {
    if b.get(i) != Some(&b'"') {
        return None;
    }
    let mut j = i + 1;
    while j < b.len() {
        match b[j] {
            b'\\' => j += 2,
            b'"' => return Some(j + 1),
            _ => j += 1,
        }
    }
    None
}

fn skip_value(b: &[u8], i: usize) -> Option<usize> {
    match b.get(i)? {
        b'"' => skip_string(b, i),
        b'{' | b'[' => {
            let mut depth = 0usize;
            let mut j = i;
            while j < b.len() {
                match b[j] {
                    b'"' => {
                        j = skip_string(b, j)?;
                        continue;
                    }
                    b'{' | b'[' => depth += 1,
                    b'}' | b']' => {
                        depth -= 1;
                        if depth == 0 {
                            return Some(j + 1);
                        }
                    }
                    _ => {}
                }
                j += 1;
            }
            None
        }
        _ => {
            let mut j = i;
            while j < b.len()
                && !matches!(b[j], b',' | b'}' | b']')
                && !(b[j] as char).is_ascii_whitespace()
            {
                j += 1;
            }
            (j > i).then_some(j)
        }
    }
}

/// 两份文本逐行比（[`line_diff`] 对外那一口：几处改动一次合好之后按原文算 diff）。
pub(crate) fn line_diff_of(old: &str, new: &str) -> (Vec<DiffLine>, usize) {
    line_diff(old, new)
}

/// 两份文本逐行比：改动那几行 ＋ 上下各一行原文；以及改动从哪一行之后开始。
fn line_diff(old: &str, new: &str) -> (Vec<DiffLine>, usize) {
    let a: Vec<&str> = old.lines().collect();
    let c: Vec<&str> = new.lines().collect();
    let (n, m) = (a.len(), c.len());
    let mut lcs = vec![vec![0usize; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            lcs[i][j] = if a[i] == c[j] {
                lcs[i + 1][j + 1] + 1
            } else {
                lcs[i + 1][j].max(lcs[i][j + 1])
            };
        }
    }
    let mut ops = Vec::new();
    let (mut i, mut j) = (0, 0);
    while i < n || j < m {
        if i < n && j < m && a[i] == c[j] {
            ops.push(DiffLine::Same(i + 1, a[i].to_string()));
            i += 1;
            j += 1;
        } else if i < n && (j == m || lcs[i + 1][j] >= lcs[i][j + 1]) {
            ops.push(DiffLine::Del(i + 1, a[i].to_string()));
            i += 1;
        } else {
            ops.push(DiffLine::Add(c[j].to_string()));
            j += 1;
        }
    }
    let changed: Vec<usize> = ops
        .iter()
        .enumerate()
        .filter(|(_, o)| !matches!(o, DiffLine::Same(..)))
        .map(|(k, _)| k)
        .collect();
    let keep = |k: usize| changed.iter().any(|&c| c.abs_diff(k) <= 1);
    let at_line = changed
        .first()
        .and_then(|&f| {
            ops[..f].iter().rev().find_map(|o| {
                if let DiffLine::Same(n, _) | DiffLine::Del(n, _) = o {
                    Some(*n)
                } else {
                    None
                }
            })
        })
        .unwrap_or(0);
    let diff = ops
        .into_iter()
        .enumerate()
        .filter(|(k, _)| keep(*k))
        .map(|(_, o)| o)
        .collect();
    (diff, at_line)
}

#[cfg(test)]
#[path = "../../../../tests/backend/footprint/chores_patch_tests.rs"]
mod tests;
