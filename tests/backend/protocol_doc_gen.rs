//! 协议参考 `src/doc/IPC-COMMANDS.md` 从代码生成：帧与出参读源码里的 serde 类型（字段说明取 `///` 第一句），
//! 命令读 `inbound::REGISTRY`（一句话 · 字段 · 码 · 档位），CLI 子命令读 `SUBCOMMANDS` ＋ `CLI_ONLY_DOCS`。
//!
//! 判据只有一条：重新生成 == 仓里那份。改了帧字段 / 命令登记之后红是预期的，重生成：
//! `CCM_REGEN_PROTOCOL_DOC=yes cargo test --lib -- protocol_doc_gen`（在 `src/backend` 下）。

use crate::stream::inbound::{CommandSpec, Dir, Run, REGISTRY};
use std::fmt::Write as _;

const OUT: &str = "src/doc/IPC-COMMANDS.md";
const REGEN: &str = "CCM_REGEN_PROTOCOL_DOC";

/// 协议级错误码：与命令无关、只有 `stream/inbound/` 发得出（`protocol_doc_guard` 的 R4 钉「控制层零命中」）。
pub(crate) const PROTOCOL_CODES: &[(&str, &str)] = &[
    (
        "bad_request",
        "这一行不是合法的请求信封 JSON（`id` 无从得知时回空串）",
    ),
    ("line_too_long", "单行超过 1 MiB，整行丢弃"),
    ("unknown_command", "`cmd` 不在本后端的命令表里"),
    ("duplicate_id", "同一个 `id` 的命令还在跑"),
    ("handler_panicked", "处理器内部崩了（进程照常活着）"),
    (
        "not_cancellable",
        "`cancel` 指向一条阻塞档命令：开跑之后撤不动，去等它自己的应答",
    ),
    ("shutting_down", "后端在收场，新来的阻塞档命令一个字节不动"),
];

// ───────────────────────────── 源码里的 serde 类型 ─────────────────────────────

/// 一个成员（字段 / 变体）：文档注释 · 属性 · 头（`name: Type` 或 `Variant`）· 花括号里的内容。
#[derive(Debug, Default)]
struct Member {
    docs: Vec<String>,
    attrs: Vec<String>,
    head: String,
    inner: Option<String>,
}

/// 从 `at`（指向某个开括号）起找配对的收括号，返回它的下标。跳过字符串字面量。
fn close_of(s: &[u8], at: usize) -> usize {
    let (open, close) = match s[at] {
        b'{' => (b'{', b'}'),
        b'[' => (b'[', b']'),
        b'(' => (b'(', b')'),
        _ => panic!("close_of 不在开括号上"),
    };
    let mut depth = 0i32;
    let mut i = at;
    while i < s.len() {
        match s[i] {
            b'"' => {
                i += 1;
                while i < s.len() && s[i] != b'"' {
                    if s[i] == b'\\' {
                        i += 1;
                    }
                    i += 1;
                }
            }
            c if c == open => depth += 1,
            c if c == close => {
                depth -= 1;
                if depth == 0 {
                    return i;
                }
            }
            _ => {}
        }
        i += 1;
    }
    panic!("括号没收尾");
}

/// 把一段成员列表（结构体字段 / 枚举变体）拆成 [`Member`]。
fn members(body: &str) -> Vec<Member> {
    let b = body.as_bytes();
    let mut out = Vec::new();
    let mut cur = Member::default();
    let mut i = 0;
    while i < b.len() {
        let rest = &body[i..];
        if b[i].is_ascii_whitespace() || b[i] == b',' {
            i += 1;
        } else if let Some(doc) = rest.strip_prefix("///") {
            let end = doc.find('\n').unwrap_or(doc.len());
            let line = doc[..end].strip_prefix(' ').unwrap_or(&doc[..end]);
            cur.docs.push(line.trim_end().to_string());
            i += 3 + end;
        } else if let Some(c) = rest.strip_prefix("//") {
            i += 2 + c.find('\n').unwrap_or(c.len());
        } else if rest.strip_prefix("#[").is_some() {
            let end = close_of(b, i + 1);
            cur.attrs.push(body[i + 2..end].to_string());
            i = end + 1;
        } else {
            // 头：读到顶层的 `,` / `{` / 结尾（泛型里的逗号按尖括号深度跳过）。
            let mut j = i;
            let mut angle = 0i32;
            while j < b.len() {
                match b[j] {
                    b'<' => angle += 1,
                    b'>' if j > 0 && b[j - 1] != b'-' && b[j - 1] != b'=' => angle -= 1,
                    b'(' | b'[' => j = close_of(b, j),
                    b',' if angle == 0 => break,
                    b'{' if angle == 0 => break,
                    _ => {}
                }
                j += 1;
            }
            cur.head = body[i..j].trim().to_string();
            if j < b.len() && b[j] == b'{' {
                let end = close_of(b, j);
                cur.inner = Some(body[j + 1..end].to_string());
                j = end + 1;
            }
            out.push(std::mem::take(&mut cur));
            i = j;
        }
    }
    out
}

/// 去掉打头的可见性（`pub` / `pub(crate)` / `pub(super)`）。
fn unpub(s: &str) -> &str {
    let s = s.trim_start();
    let Some(rest) = s.strip_prefix("pub") else {
        return s;
    };
    match rest.strip_prefix('(') {
        Some(r) => r.split_once(')').map_or(s, |(_, after)| after.trim_start()),
        None => rest.trim_start(),
    }
}

/// 一个顶层类型：`struct X { … }` / `enum X { … }`（可见性不论），连同它前面的文档注释与属性。
struct TypeDef {
    name: String,
    is_enum: bool,
    docs: Vec<String>,
    attrs: Vec<String>,
    body: String,
}

fn type_defs(src: &str) -> Vec<TypeDef> {
    let mut out = Vec::new();
    for top in members_top(src) {
        let h = unpub(top.head.as_str());
        let (is_enum, rest) = if let Some(r) = h.strip_prefix("enum ") {
            (true, r)
        } else if let Some(r) = h.strip_prefix("struct ") {
            (false, r)
        } else {
            continue;
        };
        let Some(body) = top.inner else { continue };
        out.push(TypeDef {
            name: rest
                .split(|c: char| !c.is_alphanumeric() && c != '_')
                .next()
                .unwrap_or("")
                .to_string(),
            is_enum,
            docs: top.docs,
            attrs: top.attrs,
            body,
        });
    }
    out
}

/// 顶层只关心 `pub struct` / `pub enum` 带花括号体的那些；其余条目（fn · impl · const …）整块跳过。
fn members_top(src: &str) -> Vec<Member> {
    let b = src.as_bytes();
    let mut out = Vec::new();
    let mut docs: Vec<String> = Vec::new();
    let mut attrs: Vec<String> = Vec::new();
    let mut i = 0;
    while i < b.len() {
        let rest = &src[i..];
        let eol = rest.find('\n').unwrap_or(rest.len());
        let line = rest[..eol].trim_start();
        if let Some(doc) = line.strip_prefix("///") {
            let t = doc.strip_prefix(' ').unwrap_or(doc);
            docs.push(t.trim_end().to_string());
            i += eol + 1;
        } else if line.strip_prefix("#[").is_some() {
            let at = i + (eol - line.len());
            let end = close_of(b, at + 1);
            attrs.push(src[at + 2..end].to_string());
            i = end + 1;
        } else if ["struct ", "enum "]
            .iter()
            .any(|kw| unpub(line).strip_prefix(kw).is_some())
        {
            // 从关键字起找（`pub(crate)` 里的括号不算）。
            let kw_at = (eol - line.len()) + (line.len() - unpub(line).len());
            let head_end = rest[kw_at..]
                .find(['{', ';', '('])
                .map_or(rest.len(), |k| kw_at + k);
            let head = rest[..head_end].trim().to_string();
            if rest.as_bytes().get(head_end) == Some(&b'{') {
                let end = close_of(b, i + head_end);
                out.push(Member {
                    docs: std::mem::take(&mut docs),
                    attrs: std::mem::take(&mut attrs),
                    head,
                    inner: Some(src[i + head_end + 1..end].to_string()),
                });
                i = end + 1;
            } else {
                docs.clear();
                attrs.clear();
                i += eol + 1;
            }
        } else {
            if !line.is_empty() && line.strip_prefix("//").is_none() {
                docs.clear();
                attrs.clear();
            }
            i += eol + 1;
        }
    }
    out
}

/// 文档注释的第一句：第一段里到第一个句号为止（反引号里的不算），去掉来历标记（打头的工单号 · `〔…〕`）与 rustdoc 链接括号。
fn first_sentence(docs: &[String]) -> String {
    let para: Vec<&str> = docs
        .iter()
        .map(|s| s.trim())
        .skip_while(|s| s.is_empty())
        .take_while(|s| !s.is_empty() && !s.starts_with('#'))
        .collect();
    let mut joined = String::new();
    for line in &para {
        let cjk_seam = joined.chars().last().is_some_and(|c| !c.is_ascii())
            && line.chars().next().is_some_and(|c| !c.is_ascii());
        if !joined.is_empty() && !cjk_seam {
            joined.push(' ');
        }
        joined.push_str(line);
    }
    let mut out = String::new();
    let mut in_code = false;
    let chars: Vec<char> = joined.chars().collect();
    for (k, &ch) in chars.iter().enumerate() {
        if ch == '`' {
            in_code = !in_code;
        }
        if !in_code && (ch == '。' || (ch == '.' && chars.get(k + 1).is_none_or(|c| *c == ' '))) {
            break;
        }
        out.push(ch);
    }
    tidy(&out)
}

/// 去来历标记：`〔…〕` 整段 · 打头的「工单号（…）：」· rustdoc 的 `[`X`]` 链接括号 · 落单的 `**`。
fn tidy(s: &str) -> String {
    let mut t = String::new();
    let mut depth = 0;
    for ch in s.chars() {
        match ch {
            '〔' => depth += 1,
            '〕' if depth > 0 => depth -= 1,
            _ if depth == 0 => t.push(ch),
            _ => {}
        }
    }
    let mut t = t.trim().to_string();
    if let Some(k) = t.find('：') {
        let head = &t[..k];
        let mut paren = 0;
        let tag_like = !head.is_empty()
            && head.chars().all(|c| {
                match c {
                    '（' | '(' => paren += 1,
                    '）' | ')' => paren -= 1,
                    _ => {}
                }
                paren > 0
                    || matches!(
                        c,
                        '（' | '）'
                            | '('
                            | ')'
                            | '`'
                            | '*'
                            | ' '
                            | '-'
                            | '_'
                            | '#'
                            | '/'
                            | '②'
                            | '③'
                            | '①'
                    )
                    || c.is_ascii_alphanumeric()
            });
        if tag_like {
            t = t[k + '：'.len_utf8()..].trim().to_string();
        }
    }
    let t = t.replace("[`", "`").replace("`]", "`");
    let t = if t.split("**").count() % 2 == 0 {
        t.replace("**", "")
    } else {
        t
    };
    t.trim()
        .trim_end_matches(['，', '；', ',', ';'])
        .replace('|', "\\|")
}

fn has_attr(attrs: &[String], needle: &str) -> bool {
    attrs.iter().any(|a| a.contains(needle))
}

/// 按 `rename_all` 换形（变体名是 UpperCamel，字段名是 snake）。
fn renamed(name: &str, rule: Option<&str>) -> String {
    match rule {
        Some("snake_case") => snake(name),
        Some("lowercase") => name.to_lowercase(),
        Some("kebab-case") => snake(name).replace('_', "-"),
        Some("camelCase") if name.contains('_') => camel_of(name),
        Some("camelCase") => {
            let mut c = name.chars();
            c.next()
                .map(|f| f.to_lowercase().chain(c).collect())
                .unwrap_or_default()
        }
        Some("SCREAMING_SNAKE_CASE") => snake(name).to_uppercase(),
        _ => name.to_string(),
    }
}

/// 一个属性串里 `key = "值"` 的值。
fn attr_value<'a>(attrs: &'a [String], key: &str) -> Option<&'a str> {
    let pat = format!("{key} = \"");
    attrs.iter().find_map(|a| {
        a.split(pat.as_str())
            .nth(1)
            .and_then(|r| r.split('"').next())
    })
}

/// 字段名 / 变体名 → snake_case。
fn snake(name: &str) -> String {
    let mut out = String::new();
    for (k, ch) in name.chars().enumerate() {
        if ch.is_uppercase() {
            if k > 0 {
                out.push('_');
            }
            out.extend(ch.to_lowercase());
        } else {
            out.push(ch);
        }
    }
    out
}

fn camel_of(name: &str) -> String {
    let mut out = String::new();
    let mut up = false;
    for ch in name.chars() {
        if ch == '_' {
            up = true;
        } else if up {
            out.extend(ch.to_uppercase());
            up = false;
        } else {
            out.push(ch);
        }
    }
    out
}

/// 类型写成读者认得的样子；第二个值 = 它是不是可缺的（`Option`）。
fn wire_type(ty: &str) -> (String, bool) {
    let ty = ty.trim();
    if let Some(inner) = ty.strip_prefix("Option<").and_then(|t| t.strip_suffix('>')) {
        return (wire_type(inner).0, true);
    }
    if let Some(inner) = ty.strip_prefix("Vec<").and_then(|t| t.strip_suffix('>')) {
        return (format!("[{}]", wire_type(inner).0), false);
    }
    let last = ty.rsplit("::").next().unwrap_or(ty);
    if let Some(kv) = last
        .strip_prefix("BTreeMap<")
        .or_else(|| last.strip_prefix("HashMap<"))
        .and_then(|t| t.strip_suffix('>'))
    {
        let (k, v) = kv.split_once(',').unwrap_or((kv, ""));
        return (format!("{{{}: {}}}", wire_type(k).0, wire_type(v).0), false);
    }
    let t = match last {
        "String" | "PathBuf" => "string".to_string(),
        t if t.starts_with('&') && t.ends_with("str") => "string".to_string(),
        "u8" | "u16" | "u32" | "u64" | "usize" | "i32" | "i64" | "f64" => "number".to_string(),
        "bool" => "bool".to_string(),
        "Value" => "JSON".to_string(),
        other => other.to_string(),
    };
    (t, false)
}

/// 一组字段（结构体体 / 结构体变体体）写成表。`skip(serializing)` 的不上线、不出。
fn field_table(out: &mut String, body: &str, rule: Option<&str>) -> usize {
    let mut rows = Vec::new();
    for m in members(body) {
        let skipped = m.attrs.iter().any(|a| {
            a.strip_prefix("serde(")
                .and_then(|a| a.strip_suffix(')'))
                .is_some_and(|a| {
                    a.split(',')
                        .any(|t| matches!(t.trim(), "skip" | "skip_serializing"))
                })
        });
        if skipped {
            continue;
        }
        let head = m
            .head
            .trim_start_matches("pub(crate) ")
            .trim_start_matches("pub ");
        let Some((name, ty)) = head.split_once(':') else {
            continue;
        };
        let wire = m
            .attrs
            .iter()
            .find_map(|a| {
                a.split("rename = \"")
                    .nth(1)
                    .and_then(|r| r.split('"').next())
            })
            .map(str::to_string)
            .unwrap_or_else(|| renamed(name.trim(), rule));
        let (t, opt) = wire_type(ty);
        let opt = opt || has_attr(&m.attrs, "skip_serializing_if") || has_attr(&m.attrs, "default");
        rows.push(format!(
            "| `{}` | {}{} | {} |",
            wire,
            t,
            if opt { "?" } else { "" },
            first_sentence(&m.docs)
        ));
    }
    if rows.is_empty() {
        out.push_str("（无字段）\n");
        return 0;
    }
    out.push_str("| 字段 | 类型 | 说明 |\n|---|---|---|\n");
    for r in &rows {
        out.push_str(r);
        out.push('\n');
    }
    rows.len()
}

/// 一个源文件里上线的那几个类型（derive / impl 了 `Serialize` 或 `Deserialize` 的）。
fn wire_types(src: &str) -> Vec<TypeDef> {
    type_defs(src)
        .into_iter()
        .filter(|t| {
            has_attr(&t.attrs, "Serialize")
                || has_attr(&t.attrs, "Deserialize")
                || src.contains(&format!("impl Serialize for {}", t.name))
        })
        .collect()
}

fn render_type(out: &mut String, t: &TypeDef, src: &str) {
    let _ = writeln!(out, "#### `{}`\n", t.name);
    let doc = first_sentence(&t.docs);
    if !doc.is_empty() {
        let _ = writeln!(out, "{doc}。\n");
    }
    if src.contains(&format!("impl Serialize for {}", t.name)) {
        out.push_str("（手写序列化，形状见上面那一句。）\n\n");
        return;
    }
    let rule = attr_value(&t.attrs, "rename_all");
    if t.is_enum {
        let tag = attr_value(&t.attrs, "tag");
        let mut lines = Vec::new();
        for m in members(&t.body) {
            let v = m.head.split(['(', ' ', '{']).next().unwrap_or("");
            let w = attr_value(&m.attrs, "rename")
                .map(str::to_string)
                .unwrap_or_else(|| renamed(v, rule));
            let d = first_sentence(&m.docs);
            let fields = m.inner.as_deref().map(inline_fields).unwrap_or_default();
            let key = match tag {
                Some(tag) => format!("`{tag}: \"{w}\"`"),
                None => format!("`{w}`"),
            };
            let mut line = format!("- {key}");
            if !fields.is_empty() {
                let _ = write!(line, "，带 {fields}");
            }
            if !d.is_empty() {
                let _ = write!(line, " —— {d}");
            }
            lines.push(line);
        }
        out.push_str(&lines.join("\n"));
        out.push_str("\n\n");
    } else {
        field_table(out, &t.body, rule);
        out.push('\n');
    }
}

/// 结构体变体的字段写成一行：`bytes` number · `sha256` string?
fn inline_fields(body: &str) -> String {
    members(body)
        .iter()
        .filter_map(|m| {
            let (name, ty) = m.head.split_once(':')?;
            let (t, opt) = wire_type(ty);
            let opt = opt || has_attr(&m.attrs, "skip_serializing_if");
            Some(format!(
                "`{}` {}{}",
                name.trim(),
                t,
                if opt { "?" } else { "" }
            ))
        })
        .collect::<Vec<_>>()
        .join(" · ")
}

// ───────────────────────────── 渲染 ─────────────────────────────

fn read(rel: &str) -> String {
    std::fs::read_to_string(crate::guard_support::src_root().join(rel))
        .unwrap_or_else(|e| panic!("读不到 {rel}：{e}"))
}

/// 帧定义里 `pub use crate::<模块>::<类型>;` 引进来的线上类型（类型住在别处、帧面只引用它）：
/// 就地换成那份类型定义的原文（连着它的文档与属性），于是它在参考里的位置与写在帧定义里时一样。
fn inline_reexports(src: &str) -> String {
    let mut out = String::new();
    for line in src.lines() {
        let reexport = line
            .trim()
            .strip_prefix("pub use crate::")
            .and_then(|r| r.strip_suffix(';'))
            .and_then(|r| r.rsplit_once("::"));
        let Some((module, name)) = reexport else {
            out.push_str(line);
            out.push('\n');
            continue;
        };
        let file = module.replace("::", "/");
        let home = std::fs::read_to_string(
            crate::guard_support::src_root().join(format!("{file}/mod.rs")),
        )
        .or_else(|_| {
            std::fs::read_to_string(crate::guard_support::src_root().join(format!("{file}.rs")))
        })
        .unwrap_or_else(|e| panic!("读不到 `{module}` 的源码：{e}"));
        let lines: Vec<&str> = home.lines().collect();
        let head = lines
            .iter()
            .position(|l| {
                l.starts_with(&format!("pub struct {name} "))
                    || l.starts_with(&format!("pub enum {name} "))
            })
            .unwrap_or_else(|| panic!("`{module}` 里没有 `{name}` 的定义"));
        let mut from = head;
        while from > 0 && (lines[from - 1].starts_with("///") || lines[from - 1].starts_with("#["))
        {
            from -= 1;
        }
        let to = head
            + lines[head..]
                .iter()
                .position(|l| *l == "}")
                .expect("定义没收尾");
        // 引用那一行自己的文档（就在它上面）不进参考：换成定义原文里的那一段。
        while out.ends_with("\n")
            && out
                .trim_end_matches('\n')
                .lines()
                .last()
                .is_some_and(|l| l.trim_start().starts_with("///"))
        {
            let keep = out.trim_end_matches('\n').rfind('\n').map_or(0, |i| i + 1);
            out.truncate(keep);
        }
        for l in &lines[from..=to] {
            out.push_str(l);
            out.push('\n');
        }
    }
    out
}

fn frames(out: &mut String) {
    let src = inline_reexports(&read("stream/wire.rs"));
    let types = wire_types(&src);
    let frame = types
        .iter()
        .find(|t| t.name == "Frame")
        .expect("wire.rs 里没有 `Frame`");
    out.push_str("## 1. 出方向帧\n\n每一帧一行 JSON，`kind` 是下面的小标题。`?` ＝ 可缺（缺 ＝ 没有这一格，不是 `null`）。\n\n");
    let mut n = 0;
    for v in members(&frame.body) {
        let name = v
            .head
            .split(['{', ' ', '('])
            .next()
            .unwrap_or("")
            .to_string();
        let _ = writeln!(out, "### `{}`\n", snake(&name));
        let doc = first_sentence(&v.docs);
        if !doc.is_empty() {
            let _ = writeln!(out, "{doc}。\n");
        }
        match &v.inner {
            Some(body) => {
                field_table(out, body, None);
            }
            None => out.push_str("（无字段）\n"),
        }
        out.push('\n');
        n += 1;
    }
    assert!(n >= 20, "只切出 {n} 种帧 —— 解析坏了");

    out.push_str("## 2. 信封与帧里用到的类型\n\n");
    for t in types.iter().filter(|t| t.name != "Frame") {
        render_type(out, t, &src);
    }
}

fn protocol_codes(out: &mut String) {
    out.push_str("## 3. 协议级错误码\n\n与哪条命令无关；命令自己的码列在各命令下面。\n\n| 码 | 什么时候 |\n|---|---|\n");
    for (c, why) in PROTOCOL_CODES {
        let _ = writeln!(out, "| `{c}` | {why} |");
    }
    out.push('\n');
}

fn dir_mark(d: Dir) -> &'static str {
    match d {
        Dir::In => "→",
        Dir::Out => "←",
        Dir::Both => "→ ←",
    }
}

/// 命令那一行「怎么跑」：收不收载荷 · 撤不撤得动 · CLI 面 · 依赖什么。
fn traits_of(spec: &CommandSpec) -> String {
    let mut t = Vec::new();
    t.push(
        if spec.takes_input {
            "收 `args`"
        } else {
            "不收 `args`"
        }
        .to_string(),
    );
    t.push(
        match spec.run {
            Run::Blocking(_) | Run::BlockingData(_) => {
                "撤不动（阻塞档，`cancel` 回 `not_cancellable`）"
            }
            Run::Builtin => "连接内就地做完",
            Run::Async(_) | Run::AsyncData(_) => "可撤",
        }
        .to_string(),
    );
    if crate::control::cli_control::cli_exposed(spec) {
        t.push(format!("CLI：`ccm -- {}`", crate::cli_flag(spec.name)));
    } else {
        t.push("只在流上".to_string());
    }
    if spec.codes.contains(&crate::NO_TMUX) {
        t.push("没有 tmux 的机器上做不到（hello `unavailable` 会列它）".to_string());
    }
    if spec.codes.contains(&crate::NO_UNIX_MODE) {
        t.push("没有 unix 权限位的机器上做不到".to_string());
    }
    t.join(" · ")
}

fn command(out: &mut String, spec: &CommandSpec) {
    let _ = writeln!(out, "#### `{}`\n", spec.name);
    let _ = writeln!(out, "{}。\n", spec.summary.trim_end_matches('。'));
    let _ = writeln!(out, "{}\n", traits_of(spec));
    if !spec.fields.is_empty() {
        out.push_str("| 字段 | 向 | 说明 |\n|---|---|---|\n");
        for f in spec.fields {
            let _ = writeln!(
                out,
                "| `{}` | {} | {} |",
                f.name,
                dir_mark(f.dir),
                f.doc.replace('|', "\\|")
            );
        }
        out.push('\n');
    }
    if !spec.codes.is_empty() {
        let codes: Vec<String> = spec.codes.iter().map(|c| format!("`{c}`")).collect();
        let _ = writeln!(out, "码：{}\n", codes.join(" · "));
    }
}

fn commands(out: &mut String) {
    let mod_rs = read("stream/inbound/mod.rs");
    let at = guard_core::find_pinned(&mod_rs, "const FAMILIES: &[&[super::CommandSpec]] = &[")
        .expect("锚不住 `FAMILIES`");
    let table = &mod_rs[at..at + mod_rs[at..].find("];").expect("`FAMILIES` 没收尾")];
    let names: Vec<&str> = table
        .lines()
        .filter_map(|l| l.trim().strip_suffix("::SPECS,"))
        .collect();
    let fams = crate::stream::inbound::registry::FAMILIES;
    assert_eq!(
        names.len(),
        fams.len(),
        "`FAMILIES` 源码里的族数与编进去的对不上"
    );

    out.push_str("## 4. 入方向命令\n\n请求 `{\"id\",\"cmd\",\"args\",\"within_ms\"?}`，应答 `reply` 帧：成功 `ok:true` ＋ `data`，失败 `ok:false` ＋ `code` ＋ `message`（个别码带 `data`）。\n\
字段表里 → ＝ 在 `args` 里，← ＝ 在 `data` 里。嵌套对象的字段平铺在同一张表里。\n\n");
    let mut total = 0;
    for (name, specs) in names.iter().zip(fams) {
        let src = read(&format!("stream/inbound/registry/{name}.rs"));
        let title = src
            .lines()
            .next()
            .and_then(|l| l.strip_prefix("//! "))
            .map(|l| l.trim_start_matches("命令表 · "))
            .unwrap_or(name);
        let title = title.split('：').next().unwrap_or(title);
        let _ = writeln!(out, "### 4.{} {}\n", total_families(&names, name), title);
        for spec in specs.iter() {
            command(out, spec);
            total += 1;
        }
    }
    assert_eq!(total, REGISTRY.len(), "按族渲染的条数与 REGISTRY 对不上");
}

fn total_families(names: &[&str], name: &str) -> usize {
    names.iter().position(|n| *n == name).unwrap_or(0) + 1
}

fn cli(out: &mut String) {
    out.push_str("## 5. CLI 子命令\n\n一次性调用：`ccm -- --子命令 …`（打头的 `--` 之后才归后端）；收 `args` 的从 stdin 读一段 JSON，回一行 JSON 到 stdout。\n\
「＝ 帧命令」的那几条与流上同名命令是同一个处理器。\n\n| 子命令 | 说明 |\n|---|---|\n");
    let mut flags: Vec<&str> = crate::SUBCOMMANDS.to_vec();
    flags.sort_unstable();
    flags.dedup();
    let mut documented = Vec::new();
    for flag in flags {
        if let Some(spec) = crate::control::cli_control::spec_for(flag) {
            let _ = writeln!(
                out,
                "| `{flag}` | ＝ 帧命令 `{}`：{} |",
                spec.name,
                spec.summary.trim_end_matches('。')
            );
            continue;
        }
        let (_, usage, what) = crate::stream::inbound::CLI_ONLY_DOCS
            .iter()
            .find(|(f, _, _)| crate::cli_flag(f) == flag)
            .unwrap_or_else(|| {
                panic!(
                    "CLI 子命令 `{flag}` 在 `CLI_ONLY_DOCS` 里没有说明 —— 加一行（用法 ＋ 一句话）"
                )
            });
        documented.push(flag);
        let usage = if usage.is_empty() {
            String::new()
        } else {
            format!(" `{usage}`")
        };
        let _ = writeln!(out, "| `{flag}`{usage} | {} |", what.replace('|', "\\|"));
    }
    let ghosts: Vec<String> = crate::stream::inbound::CLI_ONLY_DOCS
        .iter()
        .map(|(f, _, _)| crate::cli_flag(f))
        .filter(|f| !documented.contains(&f.as_str()))
        .collect();
    assert!(
        ghosts.is_empty(),
        "`CLI_ONLY_DOCS` 里这几行已经不是 CLI 独有的子命令：{ghosts:?}"
    );
    out.push('\n');
}

fn oneshot_types(out: &mut String) {
    out.push_str("## 6. 一次性子命令的出入参类型\n\n");
    for (rel, what) in [
        (
            "control/resolve_query.rs",
            "`--resolve`（入参 `ResumeSpec` 从 stdin，出参 `CommandPlan` / `ResolveError`）",
        ),
        ("control/fork_write.rs", "`--fork-session` 的出参"),
        (
            "observe/history_query.rs",
            "`--read-session-from-offset … --index` 的出参行",
        ),
    ] {
        let src = read(rel);
        let _ = writeln!(out, "### {what}\n");
        for t in wire_types(&src) {
            render_type(out, &t, &src);
        }
    }
}

pub(crate) fn render() -> String {
    let mut out = String::new();
    out.push_str("<!-- 生成物，勿手改：由 tests/backend/protocol_doc_gen.rs 从代码生成。重生成：在 src/backend 下 CCM_REGEN_PROTOCOL_DOC=yes cargo test --lib -- protocol_doc_gen -->\n\n");
    out.push_str("# 协议参考（帧 · 命令 · CLI）\n\n总述（载体 · 信封 · 握手 · 冻结面）在 [IPC-PROTOCOL.md](IPC-PROTOCOL.md)；本页逐格列形状。\n\n");
    frames(&mut out);
    protocol_codes(&mut out);
    commands(&mut out);
    cli(&mut out);
    oneshot_types(&mut out);
    while out.ends_with("\n\n") {
        out.pop();
    }
    out
}

#[test]
fn the_protocol_reference_is_what_the_code_generates() {
    let fresh = render();
    let path = crate::guard_support::repo_root().join(OUT);
    // 设了重生成开关 ⇒ 先写回，再照常比（比的就是刚写进去的那一份）。
    if std::env::var_os(REGEN).is_some() {
        std::fs::write(&path, &fresh).expect("写不回协议参考");
    }
    let on_disk = std::fs::read_to_string(&path).unwrap_or_default();
    if on_disk != fresh {
        let line = on_disk
            .lines()
            .zip(fresh.lines())
            .position(|(a, b)| a != b)
            .unwrap_or_else(|| on_disk.lines().count().min(fresh.lines().count()));
        panic!(
            "\n{OUT} 与代码生成的不一致（第 {} 行起）。\n重生成：在 src/backend 下 {REGEN}=yes cargo test --lib -- protocol_doc_gen\n",
            line + 1
        );
    }
}

#[test]
fn the_parser_sees_every_frame_field() {
    // 量具自检：hello 那一节必须列出这几个冻结键（第二个前端按它们读）。
    let doc = render();
    let at = doc.find("### `hello`").expect("帧表里没有 hello");
    let section = &doc[at..at + doc[at..].find("\n### ").unwrap()];
    for key in [
        "v",
        "build_id",
        "host_arch",
        "claude_dir",
        "capabilities",
        "emits",
        "commands",
        "unavailable",
        "uncancellable",
    ] {
        assert!(
            section.contains(&format!("| `{key}` |")),
            "hello 那一节没列 `{key}` —— 解析坏了"
        );
    }
}
