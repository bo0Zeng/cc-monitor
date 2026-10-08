//! **对外文案表的 Rust 取文口** —— 全仓 Rust 一侧唯一的一份实现（从 monitor 的
//! `copy_table.rs` 搬来：那一份原样转发到这里）。
//!
//! 要求：「**所有对外文案与报错都从一张表来**（结构化的 key → 文本，插值点留在表里）」；
//! 决定 2 逐字「**一份文件，两侧各读，零转换**」。
//!
//! # 为什么住共享 crate、不住某一个宿主里
//!
//! 读它的有三方：monitor（界面进程）· 常驻后端（`src/backend`，自己出句子）
//! · `creds-core`（被上面两方同时链接，自己也有要对人说的话）。
//! `creds-core` 够不着任何一个宿主的 `crate::copy_table`；只能各写一份取文实现。
//!
//! # 纪律（与 TS 那一侧 `src/frontend/ui/copy-table.ts::copyText` 同一套，判据住 `tests/copy/copy-table.vitest.ts`）
//!
//! - key 必须是字面量（判据按调用形状从 `.rs` 里抠 `copy_text("…", &[…])`，与表两向相等）；
//! - 参数是 `&[("名", 值)]` 的数组字面量，名的集合 == 表里那一条的 `args`；
//! - 占位符只许具名 `{name}`。
//!
//! 表是**编译期内嵌**的：改一句话要重编才生效；远端那台后端说的是它自己那一版的话（与它的行为同版）。

use std::sync::OnceLock;

/// 同一份表（前端 `copy-table.ts` 经 Vite 读它；这里编译期内嵌）。
pub const TABLE_JSON: &str = include_str!("../../../shared/copy/table.json");

fn entries() -> &'static serde_json::Map<String, serde_json::Value> {
    static TABLE: OnceLock<serde_json::Map<String, serde_json::Value>> = OnceLock::new();
    TABLE.get_or_init(|| {
        serde_json::from_str::<serde_json::Value>(TABLE_JSON)
            .ok()
            .and_then(|v| v.get("entries").and_then(|e| e.as_object()).cloned())
            .unwrap_or_default()
    })
}

/// 取一条文案并填上具名占位符。
///
/// 表里没有这个 key ⇒ 回 ``（走不到：`copy-table.vitest.ts` 把 `.rs` 的每个调用点与表两向对拍；
/// 但不许 panic —— 一句话缺了不该拖垮它所在的那条路）。
///
/// **单趟**：从左到右扫一遍模板，`{名}` 且这个名给了值 ⇒ 换成值，**值本身不再被扫**；
/// 没给的原样留 `{名}`（Rust 读口的处置，认可的两读口差异，金样 `_differences` 登记着）。
/// 先前逐个参数对整串 `replace` —— 前一个参数的值里若含 `{后一个参数名}`，会被后一个再换一遍：插值重新解释了值。
/// 与前端读口 `copy-table.ts::copyText` 那一趟同形（金样 `tests/__fixtures__/copy-interpolation.golden.json` 两侧各对）。
///
/// 〔`rules.json` C-L5〕值与相邻汉字之间的空格随值定（[`join_seams`]）：「{machine}上」与「{machine} 上」同一个意思。
pub fn copy_text(key: &str, args: &[(&str, &str)]) -> String {
    let Some(zh) = entries()
        .get(key)
        .and_then(|e| e.get("zh"))
        .and_then(|z| z.as_str())
    else {
        return format!("〔{key}〕");
    };
    // 「字面 · 值 · 字面 · 值 … 字面」交替；没给值的 `{x}` 算字面。
    let mut parts: Vec<String> = vec![String::new()];
    let mut rest = zh;
    while let Some(open) = rest.find('{') {
        parts
            .last_mut()
            .expect("parts 恒非空")
            .push_str(&rest[..open]);
        let after = &rest[open + 1..];
        let given = after.find('}').and_then(|close| {
            let name = &after[..close];
            args.iter()
                .find(|(n, _)| *n == name)
                .map(|(_, v)| (close, *v))
        });
        match given {
            Some((close, value)) => {
                parts.push(value.to_string());
                parts.push(String::new());
                rest = &after[close + 1..];
            }
            // 不是一个给了值的占位符 ⇒ 这个 `{` 原样留下，从它后面接着扫（`{a{b}` 里的 `{b}` 照样认）。
            None => {
                parts.last_mut().expect("parts 恒非空").push('{');
                rest = after;
            }
        }
    }
    parts.last_mut().expect("parts 恒非空").push_str(rest);
    join_seams(&parts)
}

fn is_han(c: char) -> bool {
    matches!(c, '\u{3400}'..='\u{4dbf}' | '\u{4e00}'..='\u{9fff}' | '\u{f900}'..='\u{faff}')
}

fn is_ascii_visible(c: char) -> bool {
    matches!(c, '!'..='~')
}

/// 一道接缝两边的字：汉字挨着 ASCII 可见字符 ⇒ 要一个空格（`Some(true)`）；汉字挨着汉字 ⇒ 不要（`Some(false)`）；别的（全角标点 · 空白 · 其它文字）⇒ 不管。
fn seam_wants(a: char, b: char) -> Option<bool> {
    let (ha, hb) = (is_han(a), is_han(b));
    if (ha && is_ascii_visible(b)) || (is_ascii_visible(a) && hb) {
        Some(true)
    } else if ha && hb {
        Some(false)
    } else {
        None
    }
}

/// 拼回一句：`parts` 是「字面 · 值 · 字面 · 值 … 字面」交替（字面可为空）。每个非空的值与它两边的**模板字**之间
/// 按 [`seam_wants`] 补上或拿掉**一个**空格；接缝上两个以上空格是排版，照留；两个值之间只隔空格的那一段不动。
/// 与前端 `copy-table.ts::joinSeams` 同形（两侧各对插值金样）。
fn join_seams(parts: &[String]) -> String {
    let mut lit: Vec<String> = parts
        .iter()
        .enumerate()
        .map(|(i, p)| if i % 2 == 0 { p.clone() } else { String::new() })
        .collect();
    let mut i = 1;
    while i < parts.len() {
        let v = &parts[i];
        let (Some(first), Some(last)) = (v.chars().next(), v.chars().next_back()) else {
            i += 2;
            continue;
        };
        // 左边：前一段字面的尾巴
        let l: Vec<char> = lit[i - 1].chars().collect();
        let l_space = l.len() >= 2 && l[l.len() - 1] == ' ' && l[l.len() - 2] != ' ';
        let lc = if l_space {
            l.get(l.len() - 2)
        } else {
            l.last()
        };
        if let Some(&lc) = lc {
            match seam_wants(lc, first) {
                Some(true) if !l_space => lit[i - 1].push(' '),
                Some(false) if l_space => {
                    lit[i - 1].pop();
                }
                _ => {}
            }
        }
        // 右边：后一段字面的开头
        if i + 1 < lit.len() {
            let r: Vec<char> = lit[i + 1].chars().collect();
            let r_space = r.len() >= 2 && r[0] == ' ' && r[1] != ' ';
            let rc = if r_space { r.get(1) } else { r.first() };
            if let Some(&rc) = rc {
                match seam_wants(last, rc) {
                    Some(true) if !r_space => lit[i + 1].insert(0, ' '),
                    Some(false) if r_space => {
                        lit[i + 1].remove(0);
                    }
                    _ => {}
                }
            }
        }
        i += 2;
    }
    parts
        .iter()
        .enumerate()
        .map(|(i, p)| {
            if i % 2 == 0 {
                lit[i].as_str()
            } else {
                p.as_str()
            }
        })
        .collect()
}

/// 同一条文案，但要一个 `&'static str`（**没有参数**的那种）。
///
/// 后端有几处的类型刻意是 `&'static str`（例：`accounts::upstream_select::table::Rejected::why` ——
/// 「进日志是安全的由类型兜着，不是由记得别把文件内容塞进来兜着」）。句子进表之后不许为了它
/// 把类型放宽成 `String`：每个调用点展开成一个自己的 `static LazyLock<String>`，取一次、住一辈子。
///
/// 判据（`tests/copy/copy-table.vitest.ts::rustRefsIn`）把 `copy_static!("…")` 与 `copy_text("…", &[…])`
/// 一样收进「引用」一侧：key 必须是紧跟的字符串字面量，没有参数。
#[macro_export]
macro_rules! copy_static {
    ($key:literal) => {{
        static TEXT: ::std::sync::LazyLock<::std::string::String> =
            ::std::sync::LazyLock::new(|| $crate::copy_text($key, &[]));
        TEXT.as_str()
    }};
}

/// 一段文字里有没有表里那一条（占位处任意值；取文口在接缝上补 / 删的那一个空格可有可无）。
///
/// 给测试按文案键断言用：值事先算不出、只要那一句在 ⇒ `assert!(copy_matches("键", &text))`，
/// 改表值不撞，改键 / 删键撞。表里没有这个键 ⇒ `false`。
pub fn copy_matches(key: &str, text: &str) -> bool {
    copy_matches_with(key, &[], text)
}

/// 同 [`copy_matches`]，但给出的那几个占位值要按位置出现在文字里（没给的占位处仍是任意值）。
///
/// 给「那一句在、而且那一格是这个数」的断言用（例：`PATH 上的 {pathDirs} 个目录` 里那个数是 0）。
pub fn copy_matches_with(key: &str, known: &[(&str, &str)], text: &str) -> bool {
    let Some(zh) = entries()
        .get(key)
        .and_then(|e| e.get("zh"))
        .and_then(|z| z.as_str())
    else {
        return false;
    };
    // 固定段与给了值的占位交替成一串，各自按顺序找；没给值的占位是两段之间的任意字。
    let mut frags: Vec<&str> = Vec::new();
    let mut rest = zh;
    while let Some(open) = rest.find('{') {
        match rest[open..].find('}') {
            Some(close)
                if rest[open + 1..open + close]
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric())
                    && close > 1 =>
            {
                frags.push(&rest[..open]);
                let name = &rest[open + 1..open + close];
                if let Some((_, v)) = known.iter().find(|(k, _)| *k == name) {
                    frags.push(v);
                }
                rest = &rest[open + close + 1..];
            }
            _ => break,
        }
    }
    frags.push(rest);
    let last = frags.len() - 1;
    let mut at = 0;
    for (i, f) in frags.iter().enumerate() {
        let mut f = *f;
        if i > 0 {
            f = f.strip_prefix(' ').unwrap_or(f);
        }
        if i < last {
            f = f.strip_suffix(' ').unwrap_or(f);
        }
        if f.is_empty() {
            continue;
        }
        match text[at..].find(f) {
            Some(p) => at += p + f.len(),
            None => return false,
        }
    }
    true
}

#[cfg(test)]
#[path = "../../../../tests/common/copy-core/lib_tests.rs"]
mod tests;

/// 一句话里称呼本机的那个词（与界面 `chan-caller.ts::machineName` 同一条）。
pub fn local_machine() -> String {
    copy_text("control.machine.local", &[])
}

/// 因对方版本说不成的两个码，全产品各一句（与界面 `chan-caller.ts::peerVersionSaid` 同两条键）。
/// `machine` 是给人看的称呼（本机 ⇒ [`local_machine`]）。
///
/// - 那台后端不认这条命令 ⇒ [`backend_old`]；
/// - 那台回了、回的东西认不出 ⇒ [`reply_unreadable`]（只说认不出，不猜版本；细目进日志）。
pub fn backend_old(machine: &str) -> String {
    copy_text("peerVersion.said.old", &[("machine", machine)])
}

/// 见 [`backend_old`]。
pub fn reply_unreadable(machine: &str) -> String {
    copy_text("peerVersion.said.unreadable", &[("machine", machine)])
}
