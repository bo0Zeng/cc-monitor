//! 从这台收「待办」要的事实（[`super::Facts`]）：别名块的候选 · PATH 上先找到的 ccm · 设置文件 · 记下的选择。
//! 只读（读用户的启动文件与设置文件、stat）；判定在 [`super::chores`]。

use super::{Clash, DeadLines, Facts, HookTrustTodo, Hooks, Relay, SelfPaste, StaleCcm};
use crate::assets::aliases;
use crate::common::door::Door;
use crate::platform::shell::dialect::Shell;
use cap_core::{Cap, Over};
use std::path::Path;

/// 读一份用户文件的上限（启动文件 / 设置文件都远小于它）；超了跳过那一份、日志里说是哪一份。
const READ_CAP: Cap = Cap {
    bytes: 1 << 20,
    over: Over::SkipAndSay,
};

/// 读不到（不在 · 超上限 · 不是 UTF-8）⇒ `None`，那一件跳过（不在的不说；别的留一行日志说是哪一份）。
fn read_small(p: &Path) -> Option<String> {
    if !p.exists() {
        return None;
    }
    match crate::platform::fs::Ro::read_string_capped(p, READ_CAP) {
        Ok(t) => Some(t),
        Err(e) => {
            tracing::warn!(path = %p.display(), error = %e.logged(), "chores: unreadable, skipped");
            None
        }
    }
}

/// 收齐。`needs_install` 是同一份足迹里这台确实缺的那几样；`registry` 是足迹那一趟取的那一份注册表（各家的家 · MCP 登录 · 收信钩子的信任都照它）。
pub(crate) fn facts(
    door: &dyn Door,
    registry: &[crate::agents::Adapter],
    needs_install: Vec<crate::footprint::data::Missing>,
) -> Facts {
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
    let listing = aliases::read_via_among(registry, door, shell, marks.self_paste.as_deref()).ok();
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
        stale_ccm: home_path.as_deref().and_then(|h| {
            stale_ccm(
                h,
                crate::platform::shell::session_shell_path().as_deref(),
                windows,
            )
        }),
        self_paste,
        clashes,
        dead,
        relay: relay(registry),
        hooks: hooks(registry),
        mcp_login: mcp_login(registry),
        zero_trust: crate::accounts::manage::trust_share_exec::zero_gaps(door, registry),
        outside_library: crate::accounts::manage::trust_share_exec::outside_library(door, registry),
        sshd: sshd_config_path().and_then(|p| sshd_at(&p)),
        declined: marks.declined,
    }
}

/// 这台 sshd 的主配置住哪。
#[cfg(unix)]
fn sshd_config_path() -> Option<std::path::PathBuf> {
    Some(std::path::PathBuf::from("/etc/ssh/sshd_config"))
}

/// Windows 的 OpenSSH 服务端这里不看（说不清 ⇒ 不出那一件）。
#[cfg(not(unix))]
fn sshd_config_path() -> Option<std::path::PathBuf> {
    None
}

/// 读真文件、列真目录走 [`super::sshd::forwarding_off`]（主配置不在 = 没装 sshd ⇒ `None`）。
/// 读不到的（不少发行版这份只许 root 读）留一行日志、不报这一件（[`read_small`]）。
pub(crate) fn sshd_at(main: &Path) -> Option<super::sshd::ForwardingOff> {
    if !main.exists() {
        return None;
    }
    let read = |p: &Path| read_small(p);
    let list = |dir: &Path| match crate::platform::fs::Ro::read_dir(dir) {
        Ok(it) => Some(it.filter_map(|e| Some(e.ok()?.path())).collect()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Some(Vec::new()),
        Err(_) => None,
    };
    super::sshd::forwarding_off(main, &read, &list)
}

/// 用户自己配的、要登录的 MCP（判定只在 `accounts::manage::mcp_share_exec::needs_login`，需手动清单同一处）：全局那一段 ＋
/// 这台此刻活着的会话所在项目的那两段。声明了 MCP 面的每一家各问一遍；同一家里同名并成一件、号取并集。
fn mcp_login(registry: &[crate::agents::Adapter]) -> Vec<super::McpLogin> {
    mcp_login_among(registry, |kind| {
        let cwds = crate::agents::adapter_among(registry, kind)
            .and_then(family_home)
            .map(|h| crate::history::live_table::now_among(registry, &h).cwds())
            .unwrap_or_default();
        crate::accounts::manage::mcp_share_exec::needs_login_among(registry, kind, &cwds)
    })
}

/// [`mcp_login`] 的本体：注册表与各家要登录的那几条（`needs`）是参数（判据拿替身家喂它）。
pub(crate) fn mcp_login_among(
    registry: &[crate::agents::Adapter],
    needs: impl Fn(&'static str) -> Vec<crate::accounts::manage::mcp_share_exec::McpNeed>,
) -> Vec<super::McpLogin> {
    let mut all: Vec<super::McpLogin> = Vec::new();
    for kind in crate::agents::mcp_kinds_among(registry) {
        let mut out: Vec<super::McpLogin> = Vec::new();
        for e in needs(kind) {
            // 待办那一件只列号名：没设账号那一位不出名字。
            let who = e.login_in.into_iter().flatten();
            match out.iter_mut().find(|m| m.name == e.name) {
                Some(m) => m.who.extend(who),
                None => {
                    // 那一家没声明登录那一格 ⇒ 不出这一件（说不出怎么登）。
                    let Some(argv) = crate::agents::mcp_login_argv_among(registry, kind, &e.name)
                    else {
                        continue;
                    };
                    out.push(super::McpLogin {
                        kind,
                        command: command_line(&argv),
                        name: e.name,
                        who: who.collect(),
                    });
                }
            }
        }
        for m in &mut out {
            m.who.sort();
            m.who.dedup();
        }
        out.sort_by(|a, b| a.name.cmp(&b.name));
        all.extend(out);
    }
    all
}

/// argv ⇒ 复制的那一行：逐词照 [`crate::platform::shell::dialect::posix_word`]，空格隔开。
fn command_line(argv: &[String]) -> String {
    argv.iter()
        .map(|w| crate::platform::shell::dialect::posix_word(w))
        .collect::<Vec<_>>()
        .join(" ")
}

/// 终端里敲 `ccm` 走到的不是 cc-monitor 放的那一份 ⇒ 那一份（判法只住 `platform::shell::reach`）。
pub(crate) fn stale_ccm(home: &Path, path: Option<&str>, windows: bool) -> Option<StaleCcm> {
    let ours = crate::platform::paths::ccm_entry_under(home, windows);
    let crate::platform::shell::Reach::Other(first) =
        crate::platform::shell::reach(path, &ours, windows, &|p| p.is_file())
    else {
        return None;
    };
    Some(StaleCcm {
        needs_root: !first.starts_with(home),
        path: first.display().to_string(),
    })
}

/// 直接敲的 agent 也走中转：声明了这一形的每一家各一件（`endpoint::optin_report` 那一份 ＋ 那份设置文件现在的内容）。
fn relay(registry: &[crate::agents::Adapter]) -> Vec<Relay> {
    let Some(home) = crate::platform::paths::home_dir() else {
        return Vec::new();
    };
    let up = crate::accounts::upstream_select::Upstreams::from_env(&|k| std::env::var(k).ok());
    relay_at(
        registry,
        &home,
        &crate::accounts::upstream_select::file_face::machine_rows(),
        &|agent| up.as_ref().is_some_and(|u| u.has(agent)),
        &live_projects(registry),
    )
}

/// 一家此刻的家（那一家说不出家 ⇒ `None`）：MCP 登录与中转两件问活着的会话都照它。
fn family_home(a: &crate::agents::Adapter) -> Option<crate::agents::AgentHome> {
    (a.home)().map(|home| crate::agents::AgentHome { kind: a.kind, home })
}

/// 注册表里各家此刻的家（[`family_home`] 逐家取，说不出家的不列）。
pub(crate) fn family_homes(registry: &[crate::agents::Adapter]) -> Vec<crate::agents::AgentHome> {
    registry.iter().filter_map(family_home).collect()
}

/// 这台此刻活着的会话所在的项目（各家的家目录里登记的会话；同 MCP 登录那一件的取法）。
fn live_projects(registry: &[crate::agents::Adapter]) -> Vec<String> {
    let mut v: Vec<String> = family_homes(registry)
        .iter()
        .flat_map(|h| crate::history::live_table::now_among(registry, h).cwds())
        .collect();
    v.sort();
    v.dedup();
    v
}

/// [`relay`] 的本体（注册表 · 家目录 · 表 · 哪几家登记了默认上游 · 活着的会话在哪几个项目都是参数，判据喂夹具）。
pub(crate) fn relay_at(
    registry: &[crate::agents::Adapter],
    home: &Path,
    routed: &crate::accounts::upstream_select::file_face::RoutedRows,
    registered: &dyn Fn(&str) -> bool,
    projects: &[String],
) -> Vec<Relay> {
    crate::agents::settings_env_families(registry)
        .into_iter()
        .map(|(agent, name, face)| {
            let r = crate::accounts::upstream_select::endpoint::optin_report_at(
                home,
                agent,
                &face,
                routed.of(agent),
                registered(agent),
            );
            let path = r.source.display().to_string();
            Relay {
                agent: agent.to_string(),
                name: name.to_string(),
                slot: face.slot,
                merge: face.merge,
                state: r.state.name().to_string(),
                text: read_small(&r.source),
                path,
                url: r.url,
                covered: projects.iter().find_map(|p| {
                    (face.covered_in)(Path::new(p)).map(|f| super::Covered {
                        project: p.clone(),
                        file: f.display().to_string(),
                    })
                }),
            }
        })
        .collect()
}

/// cc-bus 自动收信：`hooks-diag` 每一家那一份 ＋ 那份钩子文件（声明了信任的再加信任那份）现在的内容。这台跑不了 cc-bus ⇒ 一件不出。
fn hooks(registry: &[crate::agents::Adapter]) -> Vec<Hooks> {
    hooks_from(registry, crate::assets::cc_bus_hooks::answer_in(registry))
}

/// [`hooks`] 的本体：诊断那一份是参数（判据拿替身家造一份喂它）；信任那一件照 `registry` 里那一家的 `merge` 并。
pub(crate) fn hooks_from(
    registry: &[crate::agents::Adapter],
    r: crate::assets::cc_bus_hooks::HooksReport,
) -> Vec<Hooks> {
    if !r.supported {
        return Vec::new();
    }
    r.families
        .into_iter()
        .map(|fam| {
            // 要加的那几项（键路径 · 那一项）照诊断给的那一份拆（拼法与格名都住那一家，这里不认）。
            let items = fam
                .snippet
                .as_deref()
                .map(crate::assets::cc_bus_hooks::slots_of)
                .unwrap_or_default();
            // 信任那一件：记上之后的整份由那一家的 `merge` 并（键与值是诊断照它的算法算好的）。
            let merge = crate::agents::footprint_face_among(registry, &fam.kind)
                .and_then(|f| f.hook_trust)
                .map(|t| t.merge);
            let trust = fam.trust.as_ref().map(|t| {
                let text = read_small(Path::new(&t.file));
                let whole = merge.and_then(|m| m(text.as_deref().unwrap_or(""), &t.key, &t.value));
                HookTrustTodo {
                    path: t.file.clone(),
                    trusted: t.trusted,
                    whole,
                    text,
                }
            });
            Hooks {
                ccbus: fam.snippet.is_some(),
                installed: fam.diagnosis.stop.is_working(),
                text: read_small(Path::new(&fam.source)),
                kind: fam.kind,
                path: fam.source,
                items,
                trust,
            }
        })
        .collect()
}

#[cfg(test)]
#[path = "../../../../tests/backend/footprint/chores_gather_tests.rs"]
mod tests;
