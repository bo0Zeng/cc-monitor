//! 别名做成 `~/.cc-monitor/bin/<名>` → `ccm` 的链接（只在 POSIX）：ccm 看到自己被叫成 `<名>` 就等同 `ccm @<名>`
//! （`control/ccm/mod.rs::route`）。新建的别名已开的终端马上能用（那个目录在 `PATH` 里，别名块装的那一行），任何 shell 都行。
//!
//! 经这台的文件管理面（`files-ls` · `files-stat` · `files-link` · `files-delete`），不自己碰盘。
//! 认「是我们放的」只凭一条：那个目录里、指向 `ccm`（链接文本逐字）的链接。别的文件一个都不动。

use serde_json::{json, Value};

use crate::assets::door::{self, Door};

/// 链接住的目录（ccm 自己住的那个，`~/.cc-monitor/bin`）。
pub(crate) fn bin_rel() -> &'static str {
    relay_route_core::BACKEND_LANDING_REL
        .rsplit_once('/')
        .map_or("", |(dir, _)| dir)
}

/// 链接的目标文本：同一目录下的 ccm（ccm 的落点那一段的末段）。
pub(crate) fn target() -> &'static str {
    relay_route_core::BACKEND_LANDING_REL
        .rsplit('/')
        .next()
        .unwrap_or_default()
}

/// 那个目录里此刻是我们放的那几条链接的名字（目录不在 ⇒ 空）。
pub(crate) fn ours(d: &dyn Door, home: &str) -> Result<Vec<String>, String> {
    let dir = door::join_under(home, bin_rel());
    let listed = match d.ask("files-ls", json!({ "path": dir })) {
        Ok(v) => v,
        Err((code, _)) if code == "not_found" => return Ok(Vec::new()),
        Err((_, said)) => return Err(said),
    };
    let mut out = Vec::new();
    for e in listed
        .get("entries")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        if e.get("kind").and_then(Value::as_str) != Some("symlink") {
            continue;
        }
        let Some(path) = e.get("path").and_then(Value::as_str) else {
            continue;
        };
        let to = d
            .ask("files-stat", json!({ "path": path }))
            .ok()
            .and_then(|v| {
                v.get("link_target")
                    .and_then(Value::as_str)
                    .map(str::to_string)
            });
        if to.as_deref() == Some(target()) {
            if let Some(name) = path.rsplit(['/', '\\']).next() {
                out.push(name.to_string());
            }
        }
    }
    out.sort();
    Ok(out)
}

/// 「名字：那一句」。
fn named(name: &str, said: &str) -> String {
    copy_core::copy_text("beProfile.named.say", &[("name", name), ("said", said)])
}

/// 链接这一趟做了什么：动没动 · 没做成的几条（每条一句，带名字）。
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct Synced {
    pub changed: bool,
    pub problems: Vec<String>,
}

/// 让那个目录里我们放的链接恰好是 `want` 那几个名字：缺的补上、多的删掉；同名的别的文件不动、说一句。
pub(crate) fn sync(d: &dyn Door, home: &str, want: &[String]) -> Synced {
    let mut out = Synced::default();
    if !want.is_empty()
        && door::stat_kind(d, &door::join_under(home, bin_rel()))
            .ok()
            .flatten()
            .is_none()
    {
        // 这台还没有那个目录（ccm 还没放过）：逐级补上。
        let mut at = String::new();
        for seg in bin_rel().split('/') {
            at = if at.is_empty() {
                seg.to_string()
            } else {
                format!("{at}/{seg}")
            };
            if door::stat_kind(d, &door::join_under(home, &at))
                .ok()
                .flatten()
                .is_none()
            {
                if let Err(e) = door::mkdir(d, home, &at) {
                    out.problems.push(e);
                    return out;
                }
            }
        }
    }
    let have = match ours(d, home) {
        Ok(h) => h,
        Err(e) => {
            out.problems.push(e);
            return out;
        }
    };
    let rel_of = |n: &str| format!("{}/{n}", bin_rel());
    for gone in have.iter().filter(|h| !want.contains(h)) {
        match door::remove(d, home, &rel_of(gone), false) {
            Ok(()) => out.changed = true,
            Err(e) => out.problems.push(named(gone, &e)),
        }
    }
    for name in want.iter().filter(|w| !have.contains(w)) {
        if let Some(kind) = door::stat_kind(d, &door::join_under(home, &rel_of(name)))
            .ok()
            .flatten()
        {
            out.problems.push(copy_core::copy_text(
                "beProfile.link.occupied",
                &[("name", name), ("kind", &kind)],
            ));
            continue;
        }
        match door::link(d, home, &rel_of(name), target()) {
            Ok(()) => out.changed = true,
            Err(e) => out.problems.push(named(name, &e)),
        }
    }
    out
}
