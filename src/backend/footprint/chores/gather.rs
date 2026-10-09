//! 从这台收「要你动手」要的事实（[`super::Facts`]）：别名块的候选 · PATH 上先找到的 ccm · 设置文件 · 记下的选择。
//! 只读（读用户的启动文件与设置文件、stat）；判定在 [`super::chores`]。

use super::{Clash, DeadLines, Facts, Hooks, Relay, SelfPaste, StaleCcm};
use crate::assets::aliases;
use crate::assets::door::Door;
use crate::platform::shell::dialect::Shell;
use serde_json::Value;
use std::path::Path;

/// 读一份用户文件的上限（启动文件 / 设置文件都远小于它）。
const READ_CAP: u64 = 1 << 20;

/// 读不到（不在 · 超上限 · 不是 UTF-8）⇒ `None`，那一件跳过（不在的不说；别的留一行日志说是哪一份）。
fn read_small(p: &Path) -> Option<String> {
    if !p.exists() {
        return None;
    }
    match crate::common::fs::read_regular_capped(p, READ_CAP).map(String::from_utf8) {
        Ok(Ok(t)) => Some(t),
        Ok(Err(_)) => {
            tracing::warn!(path = %p.display(), "chores: not UTF-8, skipped");
            None
        }
        Err(e) => {
            tracing::warn!(path = %p.display(), error = %e, "chores: unreadable, skipped");
            None
        }
    }
}

/// 收齐。`needs_install` 是同一份足迹里这台确实缺的那几样。
pub(crate) fn facts(door: &dyn Door, needs_install: Vec<Value>) -> Facts {
    let windows = cfg!(windows);
    let shell = if windows {
        Shell::PowerShell
    } else {
        Shell::Posix
    };
    let home_path = crate::platform::paths::home_dir();
    let home = home_path
        .as_ref()
        .map(|h| h.display().to_string())
        .unwrap_or_default();
    let marks = super::marks::current(super::marks::marks_path().as_deref());
    let listing = aliases::read_via(door, shell, marks.self_paste.as_deref()).ok();
    let cands = listing
        .as_ref()
        .map(|l| l.rc_candidates.as_slice())
        .unwrap_or(&[]);

    let mut clashes: Vec<Clash> = Vec::new();
    for c in cands {
        for f in &c.block.conflicting_functions {
            if !clashes.iter().any(|k| k.name == f.name) {
                let wins = serde_json::to_value(f.wins)
                    .ok()
                    .and_then(|v| v.as_str().map(str::to_string))
                    .unwrap_or_default();
                clashes.push(Clash {
                    name: f.name.clone(),
                    path: c.path.clone(),
                    line: f.line,
                    wins,
                });
            }
        }
    }

    let self_paste = marks.self_paste.as_ref().and_then(|rc| {
        let present = cands.iter().any(|c| c.path == *rc && c.block.present);
        let lines: Vec<String> =
            aliases::block::render_block(Shell::of_target(Path::new(rc)), &home)
                .ok()?
                .lines()
                .map(str::to_string)
                .collect();
        let after_line = read_small(Path::new(rc)).map_or(0, |t| t.lines().count());
        Some(SelfPaste {
            rc: rc.clone(),
            lines,
            after_line,
            present,
        })
    });

    let dead = if windows {
        vec![]
    } else {
        cands
            .iter()
            .filter(|c| c.exists)
            .filter_map(|c| {
                let text = read_small(Path::new(&c.path))?;
                let lines =
                    crate::platform::shell::posix::dead_source_lines(&text, home_path.as_deref());
                (!lines.is_empty()).then(|| DeadLines {
                    path: c.path.clone(),
                    lines,
                })
            })
            .collect()
    };

    Facts {
        windows,
        needs_install,
        stale_ccm: home_path
            .as_deref()
            .and_then(|h| stale_ccm(h, std::env::var_os("PATH").as_deref(), windows)),
        self_paste,
        clashes,
        dead,
        relay: relay(),
        hooks: hooks(),
        declined: marks.declined,
    }
}

/// PATH 上先找到的 `ccm` 不是 cc-monitor 放的那一份 ⇒ 那一份。
pub(crate) fn stale_ccm(
    home: &Path,
    path: Option<&std::ffi::OsStr>,
    windows: bool,
) -> Option<StaleCcm> {
    let name = if windows { "ccm.exe" } else { "ccm" };
    let first = std::env::split_paths(path?)
        .filter(|d| !d.as_os_str().is_empty())
        .map(|d| d.join(name))
        .find(|p| p.is_file())?;
    let ours = home.join(".cc-monitor").join("bin").join(name);
    let canon = |p: &Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    if canon(&first) == canon(&ours) {
        return None;
    }
    Some(StaleCcm {
        needs_root: !first.starts_with(home),
        path: first.display().to_string(),
    })
}

/// 直接敲的 agent 也走中转：声明了这一形的每一家各一件（`relay-optin` 那一份 ＋ 那份设置文件现在的内容）。
fn relay() -> Vec<Relay> {
    crate::agents::settings_env_families()
        .into_iter()
        .filter_map(|(agent, name, face)| {
            let r = crate::accounts::upstream_select::endpoint::optin_report(agent).ok()?;
            let path = r.source.display().to_string();
            let secret = r.url.as_deref().and_then(key_segment);
            Some(Relay {
                agent: agent.to_string(),
                name: name.to_string(),
                slot: face.slot,
                merge: face.merge,
                state: r.state.name().to_string(),
                text: read_small(&r.source),
                path,
                url: r.url,
                secret,
            })
        })
        .collect()
}

/// 插好钥匙的中转地址里那把钥匙（口之后、路由之前那一截；界面显示时遮住它）。不是那一形 ⇒ `None`。
pub(crate) fn key_segment(url: &str) -> Option<String> {
    let (head, tail) = relay_route_core::split_keyed_base_url(url)?;
    url.get(head.len()..url.len() - tail.len())
        .map(str::to_string)
}

/// cc-bus 自动收信：`hooks-diag` 那一份 ＋ 那份设置文件现在的内容。这台跑不了 cc-bus ⇒ 不出这一件。
fn hooks() -> Option<Hooks> {
    let r = crate::observe::cc_bus_hooks::answer();
    if !r.supported {
        return None;
    }
    let snippet: Option<Value> = r
        .snippet
        .as_deref()
        .and_then(|s| serde_json::from_str(s).ok());
    let items = snippet
        .as_ref()
        .and_then(|s| s.get("hooks")?.as_object().cloned())
        .map(|m| {
            m.into_iter()
                .filter_map(|(ev, arr)| Some((ev, arr.as_array()?.first()?.clone())))
                .collect()
        })
        .unwrap_or_default();
    Some(Hooks {
        ccbus: snippet.is_some(),
        installed: r.diagnosis.session_start.is_working() && r.diagnosis.stop.is_working(),
        text: read_small(Path::new(&r.source)),
        path: r.source,
        items,
    })
}

#[cfg(test)]
#[path = "../../../../tests/backend/footprint/chores_gather_tests.rs"]
mod tests;
