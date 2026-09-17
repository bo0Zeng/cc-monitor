//! `.md` 解析(F05):frontmatter `covers:` / 就近 README / 正文内联链接 → DocLink。
//! 纯字符串解析,无 IO(内容由 engine 读入传下)。只认指向 `.rs` 的目标或目录。

use crate::model::{DocLink, LinkSource};
use std::path::Path;

pub fn parse_md(doc_rel: &str, content: &str) -> Vec<DocLink> {
    let mut out = Vec::new();

    // 1) 就近:README.md → 覆盖其所在目录(顶层 README 跳过,不覆盖整仓)
    if is_readme(doc_rel) {
        if let Some(dir) = parent_dir(doc_rel) {
            out.push(DocLink {
                doc_path: doc_rel.to_string(),
                target_file: dir,
                target_symbol: None,
                source: LinkSource::Colocation,
            });
        }
    }

    // 2) frontmatter covers:
    for spec in frontmatter_covers(content) {
        push_target(&mut out, doc_rel, &spec, LinkSource::Frontmatter);
    }

    // 3) 正文内联链接 [..](target);先剥离代码块/行内代码,避免示例链接误登记
    for target in inline_link_targets(&strip_code(content)) {
        push_target(&mut out, doc_rel, &target, LinkSource::Inline);
    }

    out
}

fn push_target(out: &mut Vec<DocLink>, doc_rel: &str, spec: &str, source: LinkSource) {
    if let Some((file, symbol)) = parse_target(spec) {
        out.push(DocLink {
            doc_path: doc_rel.to_string(),
            target_file: file,
            target_symbol: symbol,
            source,
        });
    }
}

fn is_readme(rel: &str) -> bool {
    rel.rsplit('/')
        .next()
        .map(|n| n.eq_ignore_ascii_case("README.md"))
        .unwrap_or(false)
}

/// README 所在目录(含尾 '/');顶层 README(无 '/')返回 None。
fn parent_dir(rel: &str) -> Option<String> {
    rel.rfind('/').map(|i| rel[..=i].to_string())
}

/// 把一个目标 spec 解析成 (文件或目录, 符号?)。非 .rs / 非目录 / 外链 → None。
fn parse_target(raw: &str) -> Option<(String, Option<String>)> {
    let spec = raw.trim();
    if spec.is_empty()
        || spec.starts_with("http://")
        || spec.starts_with("https://")
        || spec.starts_with('#')
    {
        return None;
    }
    let (path_part, frag) = match spec.split_once('#') {
        Some((p, f)) => (p, Some(f)),
        None => (spec, None),
    };
    let p = path_part.trim();
    let path = p.strip_prefix("./").unwrap_or(p);
    // 约定:目标为**仓库根相对**路径;拒绝含 `..` 的越界路径(防 drift 越界读文件)
    if path.split('/').any(|seg| seg == "..") {
        return None;
    }
    if path.ends_with('/') {
        return Some((path.to_string(), None)); // 目录目标
    }
    if !path.ends_with(".rs") {
        return None; // 只认 Rust 代码文件(或目录)
    }
    let symbol = match frag {
        // 行号锚 #L123 → 退化为文件级
        Some(f)
            if f.len() > 1 && f.starts_with('L') && f[1..].bytes().all(|b| b.is_ascii_digit()) =>
        {
            None
        }
        Some(f) if !f.is_empty() => Some(f.to_string()),
        _ => None,
    };
    Some((path.to_string(), symbol))
}

/// 从 frontmatter(开头 `---` 到下一个 `---`)提取 `covers:` 列表(支持 `- item` 与 `[a, b]`)。
fn frontmatter_covers(content: &str) -> Vec<String> {
    let mut lines = content.lines();
    if lines.next().map(|l| l.trim_end()) != Some("---") {
        return vec![];
    }
    let mut fm: Vec<&str> = Vec::new();
    let mut closed = false;
    for line in lines.by_ref() {
        if line.trim_end() == "---" {
            closed = true;
            break;
        }
        fm.push(line);
    }
    if !closed {
        return vec![];
    }

    let mut out = Vec::new();
    let mut in_covers = false;
    for line in fm {
        let trimmed = line.trim();
        if in_covers {
            if let Some(item) = trimmed.strip_prefix('-') {
                let it = clean(item);
                if !it.is_empty() {
                    out.push(it);
                }
            } else if trimmed.is_empty() {
                // 空行,继续
            } else {
                in_covers = false; // 新 key,列表结束
            }
            continue;
        }
        if trimmed == "covers:" {
            in_covers = true;
        } else if let Some(rest) = trimmed.strip_prefix("covers:") {
            let rest = rest.trim();
            if let Some(inner) = rest.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
                for item in inner.split(',') {
                    let it = clean(item);
                    if !it.is_empty() {
                        out.push(it);
                    }
                }
            } else if !rest.is_empty() {
                out.push(clean(rest));
            }
        }
    }
    out
}

fn clean(s: &str) -> String {
    s.trim()
        .trim_matches('"')
        .trim_matches('\'')
        .trim()
        .to_string()
}

/// 剥离围栏代码块(``` / ~~~)与行内代码(`...`),避免示例里的 `](path)` 被误当链接。
fn strip_code(content: &str) -> String {
    let mut out = String::with_capacity(content.len());
    let mut in_fence = false;
    for line in content.lines() {
        let t = line.trim_start();
        if t.starts_with("```") || t.starts_with("~~~") {
            in_fence = !in_fence;
            out.push('\n');
            continue;
        }
        if in_fence {
            out.push('\n');
            continue;
        }
        // 行内 code span:去掉反引号之间的内容
        let mut in_code = false;
        for ch in line.chars() {
            if ch == '`' {
                in_code = !in_code;
                continue;
            }
            if !in_code {
                out.push(ch);
            }
        }
        out.push('\n');
    }
    out
}

/// 提取正文所有 `](target)` 的 target(取首个空白前 token,忽略标题)。
fn inline_link_targets(content: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = content;
    while let Some(p) = rest.find("](") {
        let after = &rest[p + 2..];
        if let Some(end) = after.find(')') {
            let tok = after[..end].split_whitespace().next().unwrap_or("");
            if !tok.is_empty() {
                out.push(tok.to_string());
            }
            rest = &after[end + 1..];
        } else {
            break;
        }
    }
    out
}

// ── F08:写/管理 frontmatter `covers:`(只动 covers 块,其余逐字节保留;LF 文件)──

fn has_frontmatter(content: &str) -> bool {
    let mut lines = content.lines();
    if lines.next().map(|l| l.trim_end()) != Some("---") {
        return false;
    }
    lines.any(|l| l.trim_end() == "---")
}

/// 是否 YAML 列表项(`- x` / `-`);**排除 frontmatter 分隔符 `---`**(它也以 `-` 开头)。
fn is_list_item(line: &str) -> bool {
    let t = line.trim_start();
    t == "-" || t.starts_with("- ")
}

fn list_item_value(line: &str) -> String {
    line.trim_start()
        .trim_start_matches('-')
        .trim()
        .trim_matches('"')
        .trim_matches('\'')
        .to_string()
}

fn first_line_is_delim(content: &str) -> bool {
    content.lines().next().map(|l| l.trim_end()) == Some("---")
}

/// 保留原换行风格(出现过 CRLF 就用 CRLF,否则 LF),避免整文件行尾被规范化。
fn newline_of(content: &str) -> &'static str {
    if content.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    }
}

fn reassemble(lines: &[String], nl: &str, had_trailing: bool) -> String {
    let mut s = lines.join(nl);
    if had_trailing {
        s.push_str(nl);
    }
    s
}

/// 幂等地在 covers 里加一个 target。已存在 / 畸形则原样返回。
pub fn add_covers(content: &str, target: &str) -> String {
    if frontmatter_covers(content).iter().any(|c| c == target) {
        return content.to_string();
    }
    let nl = newline_of(content);
    if !has_frontmatter(content) {
        // 首行是 --- 却未闭合 → 畸形,不动(避免双 frontmatter 把原键降级为正文)
        if first_line_is_delim(content) {
            return content.to_string();
        }
        return format!(
            "---{n}covers:{n}  - {t}{n}---{n}{c}",
            n = nl,
            t = target,
            c = content
        );
    }
    let had_trailing = content.ends_with('\n');
    let mut lines: Vec<String> = content.lines().map(String::from).collect();
    let close = match (1..lines.len()).find(|&i| lines[i].trim_end() == "---") {
        Some(c) => c,
        None => return content.to_string(),
    };
    let new_item = format!("  - {}", target);
    let covers_idx = (1..close).find(|&i| lines[i].trim().starts_with("covers:"));
    match covers_idx {
        Some(ci) => {
            let t = lines[ci].trim().to_string();
            let indent: String = lines[ci]
                .chars()
                .take_while(|c| c.is_whitespace())
                .collect();
            if t == "covers:" {
                // list 形式:插到最后一个 `- item` 之后
                let mut at = ci + 1;
                while at < close {
                    if lines[at].trim().is_empty() {
                        at += 1;
                        continue;
                    }
                    if !is_list_item(&lines[at]) {
                        break;
                    }
                    at += 1;
                }
                lines.insert(at, new_item);
            } else {
                let after = t.strip_prefix("covers:").unwrap_or("").trim();
                if let Some(inner) = after.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
                    let inner = inner.trim();
                    let new_inner = if inner.is_empty() {
                        target.to_string()
                    } else {
                        format!("{}, {}", inner, target)
                    };
                    lines[ci] = format!("{}covers: [{}]", indent, new_inner);
                } else {
                    // 单标量 covers: x → 转 list(x + target)
                    let existing = after.trim_matches('"').trim_matches('\'');
                    lines[ci] = format!("{}covers:", indent);
                    lines.insert(ci + 1, format!("  - {}", existing));
                    lines.insert(ci + 2, new_item);
                }
            }
        }
        None => {
            lines.insert(close, "covers:".to_string());
            lines.insert(close + 1, new_item);
        }
    }
    reassemble(&lines, nl, had_trailing)
}

/// 从 covers 删一个 target;列表删空则删 `covers:` 键。不存在则原样返回。
pub fn remove_covers(content: &str, target: &str) -> String {
    if !frontmatter_covers(content).iter().any(|c| c == target) {
        return content.to_string();
    }
    let nl = newline_of(content);
    let had_trailing = content.ends_with('\n');
    let mut lines: Vec<String> = content.lines().map(String::from).collect();
    let close = match (1..lines.len()).find(|&i| lines[i].trim_end() == "---") {
        Some(c) => c,
        None => return content.to_string(),
    };
    if let Some(ci) = (1..close).find(|&i| lines[i].trim().starts_with("covers:")) {
        let t = lines[ci].trim().to_string();
        let indent: String = lines[ci]
            .chars()
            .take_while(|c| c.is_whitespace())
            .collect();
        if t == "covers:" {
            // 删匹配项(跳空行,与 parser 的容忍一致)
            let mut i = ci + 1;
            while i < lines.len() {
                if lines[i].trim().is_empty() {
                    i += 1;
                    continue;
                }
                if !is_list_item(&lines[i]) {
                    break;
                }
                if list_item_value(&lines[i]) == target {
                    lines.remove(i);
                } else {
                    i += 1;
                }
            }
            // covers 还有项吗(跳空行)→ 无则删 covers: 键
            let mut has_item = false;
            let mut j = ci + 1;
            while j < lines.len() {
                if lines[j].trim().is_empty() {
                    j += 1;
                    continue;
                }
                has_item = is_list_item(&lines[j]);
                break;
            }
            if !has_item {
                lines.remove(ci);
            }
        } else if let Some(inner) = t
            .strip_prefix("covers:")
            .map(str::trim)
            .and_then(|a| a.strip_prefix('['))
            .and_then(|s| s.strip_suffix(']'))
        {
            let kept: Vec<String> = inner
                .split(',')
                .map(|x| x.trim().trim_matches('"').trim_matches('\'').to_string())
                .filter(|x| !x.is_empty() && x != target)
                .collect();
            if kept.is_empty() {
                lines.remove(ci);
            } else {
                lines[ci] = format!("{}covers: [{}]", indent, kept.join(", "));
            }
        } else {
            // 单标量 covers: x → x==target 则删整行
            let val = t
                .strip_prefix("covers:")
                .unwrap_or("")
                .trim()
                .trim_matches('"')
                .trim_matches('\'');
            if val == target {
                lines.remove(ci);
            }
        }
    }
    reassemble(&lines, nl, had_trailing)
}

/// 写 IO 前的路径守卫:拒绝绝对路径(含 Windows 盘符 `C:\`)与含 `..` 的越界路径。
/// **纵深防御**:即便调用方绕过 `Engine::guard_rel` 直调本模块,也绝不写到仓库外
/// —— 守住「非侵入:只写 `.codepicture/` 与被显式指定的仓内 .md」这条底线。
fn guard_doc_rel(doc_rel: &str) -> std::io::Result<()> {
    let is_abs = doc_rel.starts_with('/')
        || doc_rel.starts_with('\\')
        || doc_rel.chars().nth(1) == Some(':'); // Windows 盘符 C:\ / D:/
    if is_abs || doc_rel.split(['/', '\\']).any(|s| s == "..") {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("路径越界或非法:{doc_rel}"),
        ));
    }
    Ok(())
}

/// 写 IO:读指定 .md → add_covers → 写回(不存在则以纯 frontmatter 创建)。docs.rs 的写 IO 面。
pub fn write_doc_link(repo: &Path, doc_rel: &str, target: &str) -> std::io::Result<()> {
    guard_doc_rel(doc_rel)?;
    let p = repo.join(doc_rel);
    // 只在「不存在」时新建;其它读错误(权限/编码)向上抛,**绝不覆写**
    let content = match std::fs::read_to_string(&p) {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(e),
    };
    std::fs::write(&p, add_covers(&content, target))
}

/// 写 IO:删一条 covers。返回原本是否存在(存在才写回)。读失败(非不存在)向上抛。
pub fn remove_doc_link(repo: &Path, doc_rel: &str, target: &str) -> std::io::Result<bool> {
    guard_doc_rel(doc_rel)?;
    let p = repo.join(doc_rel);
    let content = match std::fs::read_to_string(&p) {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(e) => return Err(e),
    };
    let had = frontmatter_covers(&content).iter().any(|c| c == target);
    if had {
        std::fs::write(&p, remove_covers(&content, target))?;
    }
    Ok(had)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn readme_colocation() {
        let links = parse_md("src/auth/README.md", "# auth\n");
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].target_file, "src/auth/");
        assert_eq!(links[0].source, LinkSource::Colocation);
        assert!(links[0].is_dir());
    }

    #[test]
    fn top_level_readme_skipped() {
        // 顶层 README 无就近目录目标
        let links = parse_md("README.md", "# proj\n");
        assert!(links.is_empty());
    }

    #[test]
    fn frontmatter_list_and_inline() {
        let md = "---\ncovers:\n  - src/auth/\n  - src/auth.rs#login\ntitle: x\n---\n见 [login](src/auth.rs#login) 与 [外链](https://x.com)。\n";
        let links = parse_md("docs/auth.md", md);
        // frontmatter: dir + symbol;inline: 同 symbol(去重在 engine 层不做,这里各来源都登记)
        assert!(links
            .iter()
            .any(|l| l.target_file == "src/auth/" && l.source == LinkSource::Frontmatter));
        assert!(links.iter().any(|l| l.target_file == "src/auth.rs"
            && l.target_symbol.as_deref() == Some("login")
            && l.source == LinkSource::Frontmatter));
        assert!(links.iter().any(|l| l.target_file == "src/auth.rs"
            && l.target_symbol.as_deref() == Some("login")
            && l.source == LinkSource::Inline));
        // 外链不登记
        assert!(!links.iter().any(|l| l.target_file.contains("x.com")));
    }

    #[test]
    fn frontmatter_inline_array() {
        let md = "---\ncovers: [src/a.rs, src/b.rs#foo]\n---\n";
        let links = parse_md("d.md", md);
        assert!(links
            .iter()
            .any(|l| l.target_file == "src/a.rs" && l.target_symbol.is_none()));
        assert!(links
            .iter()
            .any(|l| l.target_file == "src/b.rs" && l.target_symbol.as_deref() == Some("foo")));
    }

    #[test]
    fn line_anchor_degrades_to_file() {
        let links = parse_md("d.md", "见 [x](src/x.rs#L10)\n");
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].target_file, "src/x.rs");
        assert!(links[0].target_symbol.is_none());
    }

    #[test]
    fn non_code_targets_ignored() {
        let links = parse_md("d.md", "[other](other.md) [img](pic.png) [anchor](#sec)\n");
        assert!(links.is_empty());
    }

    #[test]
    fn fenced_code_links_ignored() {
        let md = "示例:\n```\n[x](src/example.rs)\n```\n真链接 [real](src/real.rs)\n";
        let links = parse_md("d.md", md);
        assert!(!links.iter().any(|l| l.target_file == "src/example.rs"));
        assert!(links.iter().any(|l| l.target_file == "src/real.rs"));
    }

    #[test]
    fn inline_code_links_ignored() {
        let md = "写法 `[x](src/example.rs)`,真链接 [r](src/real.rs)\n";
        let links = parse_md("d.md", md);
        assert!(!links.iter().any(|l| l.target_file == "src/example.rs"));
        assert!(links.iter().any(|l| l.target_file == "src/real.rs"));
    }

    #[test]
    fn dotdot_target_rejected() {
        let links = parse_md("docs/api/x.md", "见 [a](../../src/auth.rs)\n");
        assert!(links.is_empty());
    }

    // ── F08 add/remove covers ──

    #[test]
    fn add_covers_no_frontmatter_prepends() {
        let md = "# 标题\n正文内容\n";
        assert_eq!(
            add_covers(md, "src/a.rs"),
            "---\ncovers:\n  - src/a.rs\n---\n# 标题\n正文内容\n"
        );
    }

    #[test]
    fn add_covers_existing_frontmatter_preserves_others() {
        let md = "---\ntitle: X\nauthor: zbl\n---\n正文\n";
        let out = add_covers(md, "src/a.rs");
        assert!(out.contains("title: X") && out.contains("author: zbl") && out.contains("正文"));
        assert!(frontmatter_covers(&out).iter().any(|c| c == "src/a.rs"));
    }

    #[test]
    fn add_covers_appends_to_list() {
        let out = add_covers("---\ncovers:\n  - src/a.rs\n---\n", "src/b.rs");
        let c = frontmatter_covers(&out);
        assert!(c.iter().any(|x| x == "src/a.rs") && c.iter().any(|x| x == "src/b.rs"));
    }

    #[test]
    fn add_covers_idempotent() {
        let md = "---\ncovers:\n  - src/a.rs\n---\n正文\n";
        assert_eq!(add_covers(md, "src/a.rs"), md);
    }

    #[test]
    fn add_covers_inline_array() {
        let out = add_covers("---\ncovers: [src/a.rs]\n---\n", "src/b.rs");
        let c = frontmatter_covers(&out);
        assert!(c.iter().any(|x| x == "src/a.rs") && c.iter().any(|x| x == "src/b.rs"));
    }

    #[test]
    fn remove_covers_list_keeps_rest() {
        let md = "---\ncovers:\n  - src/a.rs\n  - src/b.rs\n---\n正文\n";
        let out = remove_covers(md, "src/a.rs");
        let c = frontmatter_covers(&out);
        assert!(!c.iter().any(|x| x == "src/a.rs") && c.iter().any(|x| x == "src/b.rs"));
        assert!(out.contains("正文"));
    }

    #[test]
    fn remove_covers_last_removes_key() {
        let out = remove_covers("---\ntitle: X\ncovers:\n  - src/a.rs\n---\n", "src/a.rs");
        assert!(!out.contains("covers:") && out.contains("title: X"));
    }

    #[test]
    fn body_preserved_byte_for_byte() {
        let body = "# 标题\n\n一些 `代码` 和 [链接](x)。\n\n多段。\n";
        let md = format!("---\ntitle: X\n---\n{}", body);
        let out = add_covers(&md, "src/a.rs");
        // 闭合 --- 之后的正文逐字节不变
        assert_eq!(out.splitn(3, "---\n").nth(2).unwrap(), body);
    }

    #[test]
    fn crlf_preserved() {
        let md = "---\r\ntitle: X\r\n---\r\n正文\r\n";
        let out = add_covers(md, "src/a.rs");
        assert!(out.contains("\r\n") && !out.contains("\n\n")); // 仍是 CRLF,没退化成 LF
        assert!(frontmatter_covers(&out).iter().any(|c| c == "src/a.rs"));
    }

    #[test]
    fn unclosed_frontmatter_unchanged() {
        // 首行 --- 但无闭合 → 畸形,不动(不产双 frontmatter)
        let md = "---\ntitle: X\n正文\n";
        assert_eq!(add_covers(md, "src/a.rs"), md);
    }

    #[test]
    fn remove_scalar_covers() {
        let md = "---\ntitle: X\ncovers: src/a.rs\n---\n正文\n";
        let out = remove_covers(md, "src/a.rs");
        assert!(!frontmatter_covers(&out).iter().any(|c| c == "src/a.rs"));
        assert!(out.contains("title: X") && out.contains("正文"));
    }

    #[test]
    fn blank_line_in_list_no_orphan() {
        // covers 列表内有空行:删 a 后不留孤儿、b 保留
        let md = "---\ncovers:\n  - src/a.rs\n\n  - src/b.rs\n---\n";
        let out = remove_covers(md, "src/a.rs");
        let c = frontmatter_covers(&out);
        assert!(!c.iter().any(|x| x == "src/a.rs") && c.iter().any(|x| x == "src/b.rs"));
    }
}
