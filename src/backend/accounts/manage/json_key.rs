//! **换一个顶层键的值** —— 纯：一份 JSON 对象的原文进、新原文出，只有那个键的值那一段字节变（键不在就补在末尾），
//! 别的字节一个不动。各号的配置文件里还有登录与账号状态，整份解开再序列化会改掉它们的排版与数字写法，所以按字节切。

use copy_core::copy_text;
use serde_json::Value;

/// 顶层对象里一个成员：键那一段（含引号）的起点 · 值那一段 `[start, end)`。
struct Member {
    key_start: usize,
    value: (usize, usize),
}

/// 顶层对象扫一遍：每个成员的位置 ＋ 收尾那个 `}` 的位置。
struct Top {
    members: Vec<(String, Member)>,
    close: usize,
}

fn ws(b: &[u8], mut i: usize) -> usize {
    while i < b.len() && matches!(b[i], b' ' | b'\t' | b'\n' | b'\r') {
        i += 1;
    }
    i
}

fn bad() -> String {
    copy_text("beAcctJsonKey.parse.notObject", &[])
}

/// 从开头那个引号走到字符串收尾之后。
fn string_end(b: &[u8], start: usize) -> Result<usize, String> {
    let mut i = start + 1;
    while i < b.len() {
        match b[i] {
            b'\\' => i += 2,
            b'"' => return Ok(i + 1),
            _ => i += 1,
        }
    }
    Err(bad())
}

/// 一个值从 `start` 走到它收尾之后（对象 / 数组按括号配平，字符串里的括号不算）。
fn value_end(b: &[u8], start: usize) -> Result<usize, String> {
    match b.get(start) {
        Some(b'"') => string_end(b, start),
        Some(b'{' | b'[') => {
            let mut depth = 0usize;
            let mut i = start;
            while i < b.len() {
                match b[i] {
                    b'"' => {
                        i = string_end(b, i)?;
                        continue;
                    }
                    b'{' | b'[' => depth += 1,
                    b'}' | b']' => {
                        depth -= 1;
                        if depth == 0 {
                            return Ok(i + 1);
                        }
                    }
                    _ => {}
                }
                i += 1;
            }
            Err(bad())
        }
        Some(_) => {
            let mut i = start;
            while i < b.len() && !matches!(b[i], b',' | b'}' | b']' | b' ' | b'\t' | b'\n' | b'\r')
            {
                i += 1;
            }
            if i == start {
                Err(bad())
            } else {
                Ok(i)
            }
        }
        None => Err(bad()),
    }
}

fn scan(text: &str) -> Result<Top, String> {
    let b = text.as_bytes();
    let mut i = ws(b, if text.starts_with('\u{feff}') { 3 } else { 0 });
    if b.get(i) != Some(&b'{') {
        return Err(bad());
    }
    i = ws(b, i + 1);
    let mut members = Vec::new();
    if b.get(i) == Some(&b'}') {
        return tail_ok(b, i).map(|()| Top { members, close: i });
    }
    loop {
        if b.get(i) != Some(&b'"') {
            return Err(bad());
        }
        let key_start = i;
        let key_end = string_end(b, i)?;
        let key: String = serde_json::from_str(&text[key_start..key_end]).map_err(|_| bad())?;
        i = ws(b, key_end);
        if b.get(i) != Some(&b':') {
            return Err(bad());
        }
        let vs = ws(b, i + 1);
        let ve = value_end(b, vs)?;
        members.push((
            key,
            Member {
                key_start,
                value: (vs, ve),
            },
        ));
        i = ws(b, ve);
        match b.get(i) {
            Some(b',') => i = ws(b, i + 1),
            Some(b'}') => return tail_ok(b, i).map(|()| Top { members, close: i }),
            _ => return Err(bad()),
        }
    }
}

/// 收尾那个 `}` 之后只许有空白。
fn tail_ok(b: &[u8], close: usize) -> Result<(), String> {
    if ws(b, close + 1) == b.len() {
        Ok(())
    } else {
        Err(bad())
    }
}

/// `at` 所在那一行在它之前全是空白 ⇒ 那段缩进；否则 `None`（压成一行的文件）。
fn indent_before(text: &str, at: usize) -> Option<&str> {
    let line_start = text[..at].rfind('\n').map_or(0, |p| p + 1);
    let lead = &text[line_start..at];
    lead.chars().all(|c| c == ' ' || c == '\t').then_some(lead)
}

/// 值的写法跟着键所在的那一行：键独占一行 ⇒ 两格缩进的展开写法、续行补上键的缩进；否则压成一行。
fn render(value: &Value, indent: Option<&str>) -> Result<String, String> {
    let bad_value =
        |e: serde_json::Error| copy_text("beAcctJsonKey.render.failed", &[("e", &e.to_string())]);
    match indent {
        Some(ind) => {
            let pretty = serde_json::to_string_pretty(value).map_err(bad_value)?;
            Ok(pretty.replace('\n', &format!("\n{ind}")))
        }
        None => serde_json::to_string(value).map_err(bad_value),
    }
}

/// 顶层键 `key` 的值换成 `value`（不在就补在最后一个成员之后）。原文不是 JSON 对象 / 那个键出现了不止一次 ⇒ `Err`、不出新原文。
pub(crate) fn set_top_key(text: &str, key: &str, value: &Value) -> Result<String, String> {
    let out = splice_top(text, key, value)?;
    verify(text, &out, key, value)?;
    Ok(out)
}

/// 一组「沿这条键路放上这个值」，按序做完、末了核一次：新原文解得开，且恰等于原文解开之后照样放上这几格 ——
/// 不对就不交出去。沿途的对象原样留着（键序与排版不动），只换最后那一格的值；最后那一格不在 ⇒ 补在那一层末尾；
/// 沿途某一层不在 ⇒ 从那一层起补一个只含这条路的对象。沿途某一层不是对象 / 某个键出现了不止一次 ⇒ `Err`。
pub(crate) fn set_paths(text: &str, edits: &[(Vec<String>, Value)]) -> Result<String, String> {
    let mut out = text.to_string();
    for (path, v) in edits {
        out = splice_path(&out, path, v)?;
    }
    let parse = |t: &str| serde_json::from_str::<Value>(t.trim_start_matches('\u{feff}')).ok();
    let failed = || copy_text("beAcctJsonKey.verify.failed", &[]);
    let mut want = parse(text).ok_or_else(failed)?;
    for (path, v) in edits {
        put_at(&mut want, path, v.clone()).ok_or_else(failed)?;
    }
    if parse(&out).as_ref() != Some(&want) {
        return Err(failed());
    }
    Ok(out)
}

/// 在解开的值上沿键路放上 `v`（沿途缺的层补成空对象）；沿途某一层不是对象 ⇒ `None`。
fn put_at(root: &mut Value, path: &[String], v: Value) -> Option<()> {
    let (last, lead) = path.split_last()?;
    let mut at = root;
    for k in lead {
        at = at
            .as_object_mut()?
            .entry(k.clone())
            .or_insert_with(|| Value::Object(Default::default()));
    }
    at.as_object_mut()?.insert(last.clone(), v);
    Some(())
}

/// 沿 `path` 往下找到那一层对象的原文片段，换它里面那一格（不核；核在 [`set_paths`] 末了）。
fn splice_path(text: &str, path: &[String], value: &Value) -> Result<String, String> {
    let (first, rest) = path.split_first().ok_or_else(bad)?;
    if rest.is_empty() {
        return splice_top(text, first, value);
    }
    let top = scan(text)?;
    let hits: Vec<&Member> = top
        .members
        .iter()
        .filter(|(k, _)| k == first)
        .map(|(_, m)| m)
        .collect();
    match hits.as_slice() {
        [m] => {
            let (vs, ve) = m.value;
            let inner = &text[vs..ve];
            if !inner.starts_with('{') {
                return Err(bad());
            }
            let inner = splice_path(inner, rest, value)?;
            Ok(format!("{}{inner}{}", &text[..vs], &text[ve..]))
        }
        [] => {
            let mut nested = value.clone();
            for k in rest.iter().rev() {
                let mut m = serde_json::Map::new();
                m.insert(k.clone(), nested);
                nested = Value::Object(m);
            }
            splice_top(text, first, &nested)
        }
        _ => Err(copy_text("beAcctJsonKey.parse.duplicate", &[])),
    }
}

/// [`set_top_key`] 换字节的那一半（不核）。
fn splice_top(text: &str, key: &str, value: &Value) -> Result<String, String> {
    let top = scan(text)?;
    let hits: Vec<&Member> = top
        .members
        .iter()
        .filter(|(k, _)| k == key)
        .map(|(_, m)| m)
        .collect();
    let out = match hits.as_slice() {
        [m] => {
            let v = render(value, indent_before(text, m.key_start))?;
            format!("{}{v}{}", &text[..m.value.0], &text[m.value.1..])
        }
        [] => match top.members.last() {
            Some((_, last)) => {
                let ind = indent_before(text, last.key_start);
                let key_json = serde_json::to_string(key).map_err(|_| bad())?;
                let v = render(value, ind)?;
                let sep = ind.map_or(String::new(), |i| format!("\n{i}"));
                let at = last.value.1;
                format!("{},{sep}{key_json}: {v}{}", &text[..at], &text[at..])
            }
            None => {
                let key_json = serde_json::to_string(key).map_err(|_| bad())?;
                let v = render(value, Some("  "))?;
                let open = text[..top.close].rfind('{').unwrap_or(0);
                format!(
                    "{}{{\n  {key_json}: {v}\n}}{}",
                    &text[..open],
                    &text[top.close + 1..]
                )
            }
        },
        _ => return Err(copy_text("beAcctJsonKey.parse.duplicate", &[])),
    };
    Ok(out)
}

/// 一份只有这一个键的新文件（两格缩进、末尾换行，与 Claude 自己写的同一种排版）。
pub(crate) fn fresh_with(key: &str, value: &Value) -> Result<String, String> {
    let out = format!(
        "{{\n  {}: {}\n}}\n",
        serde_json::to_string(key).map_err(|_| bad())?,
        render(value, Some("  "))?
    );
    verify("{}", &out, key, value)?;
    Ok(out)
}

/// 新原文解得开、那个键恰是 `value`、别的顶层键与原文逐个相等 —— 不对就不交出去。
fn verify(before: &str, after: &str, key: &str, value: &Value) -> Result<(), String> {
    let parse = |t: &str| {
        serde_json::from_str::<Value>(t.trim_start_matches('\u{feff}'))
            .ok()
            .and_then(|v| match v {
                Value::Object(m) => Some(m),
                _ => None,
            })
    };
    let (Some(mut a), Some(mut b)) = (parse(before), parse(after)) else {
        return Err(copy_text("beAcctJsonKey.verify.failed", &[]));
    };
    let got = b.remove(key);
    a.remove(key);
    if got.as_ref() != Some(value) || a != b {
        return Err(copy_text("beAcctJsonKey.verify.failed", &[]));
    }
    Ok(())
}

#[cfg(test)]
#[path = "../../../../tests/backend/accounts/manage/json_key_tests.rs"]
mod tests;
