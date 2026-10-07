//! 一格文本 ⇄ 一串词：设置窗表单里「交给 agent」那一格是一串词，写法照 shell —— 空白分词；`'…'` 里原样；
//! `"…"` 里 `\"` 与 `\\` 是转义；引号外 `\` 只转义空白、引号与它自己，别的照原样（Windows 路径 `C:\work` 不用改写）。
//! 拼回一格时能原样写的原样写、否则加引号 —— 切回来逐字相等。

/// 一格文本 → 词（写法见头注）。引号没配对 ⇒ `Err`。
pub(crate) fn split_box(s: &str) -> Result<Vec<String>, ()> {
    let cs: Vec<char> = s.chars().collect();
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut started = false;
    let mut i = 0;
    // 下一个字符是不是 `want` 里的一个（不越界）。
    let next_in = |i: usize, want: &dyn Fn(char) -> bool| cs.get(i + 1).is_some_and(|&n| want(n));
    while i < cs.len() {
        let c = cs[i];
        match c {
            c if c.is_whitespace() => {
                if started {
                    out.push(std::mem::take(&mut cur));
                    started = false;
                }
            }
            '\'' => {
                started = true;
                let close = cs[i + 1..].iter().position(|&x| x == '\'').ok_or(())?;
                cur.extend(&cs[i + 1..i + 1 + close]);
                i += close + 1;
            }
            '"' => {
                started = true;
                loop {
                    i += 1;
                    match cs.get(i) {
                        Some('"') => break,
                        Some('\\') if next_in(i, &|n| matches!(n, '"' | '\\')) => {
                            i += 1;
                            cur.push(cs[i]);
                        }
                        Some(&x) => cur.push(x),
                        None => return Err(()),
                    }
                }
            }
            '\\' => {
                started = true;
                if next_in(i, &|n| n.is_whitespace() || matches!(n, '\'' | '"' | '\\')) {
                    i += 1;
                    cur.push(cs[i]);
                } else {
                    cur.push('\\');
                }
            }
            c => {
                started = true;
                cur.push(c);
            }
        }
        i += 1;
    }
    if started {
        out.push(cur);
    }
    Ok(out)
}

/// 一个词写进一格：切回来恰是它自己、又不以 `\` 结尾（后面那个分隔空格会被它转义掉）⇒ 原样；
/// 否则没有 `'` 就用 `'…'`，有就用 `"…"`（`"` 与 `\` 前加 `\`）。
fn join_word(w: &str) -> String {
    if !w.ends_with('\\') && split_box(w).is_ok_and(|v| v.len() == 1 && v[0] == w) {
        return w.to_string();
    }
    if !w.contains('\'') {
        return format!("'{w}'");
    }
    let mut s = String::from('"');
    for c in w.chars() {
        if c == '"' || c == '\\' {
            s.push('\\');
        }
        s.push(c);
    }
    s.push('"');
    s
}

/// 词 → 一格文本（[`split_box`] 的逆）。
pub(crate) fn join_box(words: &[String]) -> String {
    words
        .iter()
        .map(|w| join_word(w))
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
#[path = "../../../../tests/backend/assets/aliases/form_tests.rs"]
mod tests;
