//! 这台 sshd 的配置关没关端口转发（`-L`）：纯函数，读文件与列目录由调用方给（[`super::gather`] 给真的，判据喂夹具）。

use std::path::{Path, PathBuf};

/// 条件段之外先读到的、关掉本地转发的那一行。
pub(crate) struct ForwardingOff {
    /// 那一行所在的文件（主文件或 Include 进来的那一份）。
    pub path: String,
    /// 第几行（1 起）。
    pub line: usize,
    /// 那一行原文。
    pub text: String,
    /// 改好的那一行。
    pub fix: String,
}

/// 管本地转发的那两个关键字（小写）：`AllowTcpForwarding` · `DisableForwarding`。
const ALLOW: &str = "allowtcpforwarding";
const DISABLE: &str = "disableforwarding";

/// Include 套几层就不追了（sshd 自己也有上限；成环的配置 ⇒ 说不清）。
const INCLUDE_DEPTH: usize = 16;

/// 一个关键字先读到的那一句：在哪 · 第几行 · 原文 · 值（小写）。
struct First {
    path: String,
    line: usize,
    text: String,
    value: String,
}

#[derive(Default)]
struct Walk {
    allow: Option<First>,
    disable: Option<First>,
    /// 读过 `Match` 行：之后的都是条件段。
    in_match: bool,
    /// 条件段里也写了这两个关键字 ⇒ 对某些连接改了值，说不清。
    match_mentions: bool,
}

/// 照 sshd 的读法走 `main`：关键字不分大小写、**先读到的算数**、`Include` 就地展开（相对路径按主配置所在目录、
/// 末段带 `*` 的按字典序）、`Match` 起往后都是条件段（它会改掉条件段之外的值）。
/// 条件段之外先读到 `DisableForwarding yes` 或 `AllowTcpForwarding no|remote`，且条件段里没写这两个关键字 ⇒ 那一行。
/// 说不清（读不到 · Include 认不出 / 读不到 · 条件段里也写了）⇒ `None`，不猜。
pub(crate) fn forwarding_off(
    main: &Path,
    read: &dyn Fn(&Path) -> Option<String>,
    list: &dyn Fn(&Path) -> Option<Vec<PathBuf>>,
) -> Option<ForwardingOff> {
    let base = main.parent()?;
    let mut w = Walk::default();
    walk(main, base, read, list, &mut w, 0)?;
    if w.match_mentions {
        return None;
    }
    let off = |f: &First, fix: &str| ForwardingOff {
        path: f.path.clone(),
        line: f.line,
        text: f.text.clone(),
        fix: fix.to_string(),
    };
    if let Some(d) = w.disable.as_ref().filter(|d| d.value == "yes") {
        return Some(off(d, "DisableForwarding no"));
    }
    w.allow
        .as_ref()
        .filter(|a| matches!(a.value.as_str(), "no" | "remote"))
        .map(|a| off(a, "AllowTcpForwarding yes"))
}

fn walk(
    file: &Path,
    base: &Path,
    read: &dyn Fn(&Path) -> Option<String>,
    list: &dyn Fn(&Path) -> Option<Vec<PathBuf>>,
    w: &mut Walk,
    depth: usize,
) -> Option<()> {
    if depth > INCLUDE_DEPTH {
        return None;
    }
    let text = read(file)?;
    for (i, raw) in text.lines().enumerate() {
        let Some((key, rest)) = split(raw) else {
            continue;
        };
        match key.as_str() {
            "match" => w.in_match = true,
            "include" => {
                for pat in rest.split_whitespace() {
                    for f in expand(pat.trim_matches('"'), base, list)? {
                        walk(&f, base, read, list, w, depth + 1)?;
                    }
                }
            }
            ALLOW | DISABLE if w.in_match => w.match_mentions = true,
            ALLOW | DISABLE => {
                let slot = if key == ALLOW {
                    &mut w.allow
                } else {
                    &mut w.disable
                };
                slot.get_or_insert_with(|| First {
                    path: file.display().to_string(),
                    line: i + 1,
                    text: raw.to_string(),
                    value: rest
                        .split_whitespace()
                        .next()
                        .unwrap_or("")
                        .trim_matches('"')
                        .to_ascii_lowercase(),
                });
            }
            _ => {}
        }
    }
    Some(())
}

/// 一行 ⇒（关键字小写, 余下）；空行 · 注释 ⇒ `None`。关键字与值之间是空白或 `=`。
fn split(raw: &str) -> Option<(String, &str)> {
    let s = raw.trim();
    if s.is_empty() || s.starts_with('#') {
        return None;
    }
    let end = s
        .find(|c: char| c.is_whitespace() || c == '=')
        .unwrap_or(s.len());
    let rest = s[end..].trim_start();
    let rest = rest.strip_prefix('=').unwrap_or(rest).trim();
    Some((s[..end].to_ascii_lowercase(), rest))
}

/// Include 的一项 ⇒ 那几份文件。只认末段带 `*` 的通配；别处带通配符 ⇒ 说不清（`None`）。
fn expand(
    pat: &str,
    base: &Path,
    list: &dyn Fn(&Path) -> Option<Vec<PathBuf>>,
) -> Option<Vec<PathBuf>> {
    let p = Path::new(pat);
    let p = if p.is_absolute() {
        p.to_path_buf()
    } else {
        base.join(p)
    };
    let name = p.file_name()?.to_str()?.to_string();
    let dir = p.parent()?;
    if dir.to_str()?.contains(['*', '?', '[']) || name.contains(['?', '[']) {
        return None;
    }
    let Some((pre, post)) = name.split_once('*') else {
        return Some(vec![p]);
    };
    if post.contains('*') {
        return None;
    }
    let mut hits: Vec<PathBuf> = list(dir)?
        .into_iter()
        .filter(|f| {
            f.file_name().and_then(|n| n.to_str()).is_some_and(|n| {
                n.len() >= pre.len() + post.len() && n.starts_with(pre) && n.ends_with(post)
            })
        })
        .collect();
    hits.sort();
    Some(hits)
}

#[cfg(test)]
#[path = "../../../../tests/backend/footprint/chores_sshd_tests.rs"]
mod tests;
