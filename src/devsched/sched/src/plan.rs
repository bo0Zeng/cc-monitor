//! 树根与这一趟要什么：只照项目配置的几个正则与调用方的声明，不从命令文字里另猜。

use crate::config::Project;
use regex::Regex;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// 调用方此刻的环境。
pub type Vars = BTreeMap<String, String>;

/// 这一趟的计划。
#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    /// 认出的项目；认不出 ⇒ None（只本机）
    pub project: Option<Project>,
    /// 树根（认不出项目时是当前目录）
    pub root: PathBuf,
    /// 树名：末段 ＋ 路径摘要前 8 位
    pub tree: String,
    /// 起命令的目录
    pub cwd: PathBuf,
    /// 树里的相对目录（空 ＝ 树根，不带头尾的 `/`）
    pub rel: String,
    pub argv: Vec<String>,
    /// 要远端有的能力
    pub needs: BTreeSet<String>,
    /// 会产出进仓文件（need_own）
    pub own: bool,
    /// 只本机：为什么
    pub local: Option<String>,
    /// 本机重活（heavy_if）
    pub heavy: bool,
    /// 插队（prio_env · DEVSCHED_PRIO）
    pub prio: bool,
    /// 产物目录（同一个同一时刻只一趟）
    pub target: Option<PathBuf>,
    /// 调用方的环境（带去远端的从这里挑）
    pub vars: Vars,
}

/// 摘要前 8 位。
pub fn digest8(s: &str) -> String {
    devsched_wire::digest(s.as_bytes())[..8].to_string()
}

/// 树名：树根路径摘要前 8 位（跨趟不变）。远端树在 `<根>/projects/<项目>/<树名>`，长度不跟本机树根末段走
/// （带末段时远端树比本机长出二十几字节，tmux 套接字一类按路径长度算的在远端先超）。
pub fn tree_name(root: &Path) -> String {
    digest8(&root.to_string_lossy())
}

/// 调度记录里那一列：树根末段 ＋ 树名，人认得出是哪棵。
pub fn tree_label(root: &Path, tree: &str) -> String {
    let last = root
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "root".into());
    format!("{last}-{tree}")
}

/// 从 `cwd` 往上，第一个带某个项目 marker 的目录；同一层几个项目都认得 ⇒ 项目文件名排前的那个。
pub fn find_tree<'a>(cwd: &Path, projects: &'a [Project]) -> Option<(&'a Project, PathBuf)> {
    let mut d = Some(cwd);
    while let Some(x) = d {
        for p in projects {
            let m = p.get("marker");
            if !m.is_empty() && x.join(m).exists() {
                return Some((p, x.to_path_buf()));
            }
        }
        d = x.parent();
    }
    None
}

/// 正则匹配；写坏的正则当没写，另记一句。
fn hit(re: &str, subject: &str, notes: &mut Vec<String>) -> Option<String> {
    if re.is_empty() {
        return None;
    }
    match Regex::new(re) {
        Ok(r) => r.find(subject).map(|m| m.as_str().trim().to_string()),
        Err(e) => {
            notes.push(format!("正则写坏了（{re}）：{e}"));
            None
        }
    }
}

fn flag(vars: &Vars, name: &str) -> bool {
    !name.is_empty() && vars.get(name).is_some_and(|v| v == "1")
}

/// 定这一趟的计划。`local_flag` ＝ 命令行 `--local`。
pub fn make_plan(
    projects: &[Project],
    cwd: &Path,
    argv: &[String],
    vars: &Vars,
    local_flag: bool,
) -> (Plan, Vec<String>) {
    let mut notes = Vec::new();
    let found = find_tree(cwd, projects);
    let (project, root) = match found {
        Some((p, r)) => (Some(p.clone()), r),
        None => (None, cwd.to_path_buf()),
    };
    let rel = cwd
        .strip_prefix(&root)
        .map(|r| r.to_string_lossy().to_string())
        .unwrap_or_default();
    // 匹配的对象：「树里的相对目录 ＋ 整条命令」，写法见 CONFIG.md「正则」（`/<相对目录> <命令> `）
    let rel_slash = if rel.is_empty() {
        String::new()
    } else {
        format!("/{rel}")
    };
    let subject = format!("{rel_slash} {} ", argv.join(" "));
    let mut plan = Plan {
        project: project.clone(),
        tree: tree_name(&root),
        root: root.clone(),
        cwd: cwd.to_path_buf(),
        rel,
        argv: argv.to_vec(),
        needs: BTreeSet::new(),
        own: false,
        local: None,
        heavy: false,
        prio: flag(vars, "DEVSCHED_PRIO"),
        target: None,
        vars: vars.clone(),
    };
    if let Some(c) = vars.get("DEVSCHED_NEED") {
        plan.needs.extend(
            c.split(',')
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string()),
        );
    }
    let Some(p) = project else {
        plan.local = Some("认不出是哪个项目的树".into());
        return (plan, notes);
    };
    if flag(vars, p.get("prio_env")) {
        plan.prio = true;
    }
    for (cap, re) in p.needs() {
        if hit(&re, &subject, &mut notes).is_some() {
            plan.needs.insert(cap);
        }
    }
    plan.own = hit(p.get("need_own"), &subject, &mut notes).is_some();
    plan.heavy = hit(p.get("heavy_if"), &subject, &mut notes).is_some();
    plan.local = if local_flag {
        Some("--local".into())
    } else if flag(vars, p.get("local_env")) {
        Some(format!("{}=1", p.get("local_env")))
    } else if plan.heavy {
        Some("本机重活（heavy_if）".into())
    } else if plan.own {
        Some("会产出进仓文件（need_own）：这一版只在本机跑".into())
    } else if !p.get("remote_if").is_empty()
        && hit(p.get("remote_if"), &subject, &mut notes).is_none()
    {
        Some("不合 remote_if".into())
    } else {
        hit(p.get("local_if"), &subject, &mut notes).map(|m| format!("命中只本机的「{m}」"))
    };
    // 产物目录：命令里的 CARGO_TARGET_DIR=… ＞ 调用方环境 ＞ 项目 build
    plan.target = argv
        .iter()
        .filter_map(|a| {
            a.strip_prefix("CARGO_TARGET_DIR=")
                .filter(|v| v.starts_with('/'))
        })
        .next_back()
        .map(PathBuf::from)
        .or_else(|| {
            vars.get("CARGO_TARGET_DIR")
                .filter(|v| !v.is_empty())
                .map(PathBuf::from)
        })
        .or_else(|| {
            let b = p.get("build");
            (!b.is_empty()).then(|| root.join(b))
        });
    (plan, notes)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::config::{fixtures, Config};
    use crate::testkit::TempDir;

    pub(crate) fn sv(xs: &[&str]) -> Vec<String> {
        xs.iter().map(|s| s.to_string()).collect()
    }

    fn tree() -> (TempDir, Config) {
        let t = TempDir::new("plan");
        std::fs::create_dir_all(t.path().join("w1/tests/scripts")).unwrap();
        std::fs::write(t.path().join("w1/tests/scripts/quick-check.sh"), "").unwrap();
        std::fs::create_dir_all(t.path().join("w1/src/frontend/shell")).unwrap();
        (t, Config::load(&fixtures()).unwrap())
    }

    #[test]
    fn needs_come_from_rel_dir_and_command() {
        let (t, c) = tree();
        let cwd = t.path().join("w1/src/frontend/shell");
        let (p, notes) = make_plan(
            &c.projects,
            &cwd,
            &sv(&["cargo", "test", "--offline"]),
            &Vars::new(),
            false,
        );
        assert!(notes.is_empty(), "{notes:?}");
        assert_eq!(p.project.as_ref().unwrap().name, "alpha");
        assert_eq!(p.root, t.path().join("w1"));
        assert_eq!(p.rel, "src/frontend/shell");
        assert_eq!(
            p.needs,
            ["cargo", "gui", "tmuxnew"]
                .iter()
                .map(|s| s.to_string())
                .collect()
        );
        assert_eq!(p.local, None);
        assert!(!p.own && !p.heavy && !p.prio);
        assert_eq!(p.target, Some(t.path().join("w1/.build")));
        assert_eq!(p.tree.len(), 8);
    }

    #[test]
    fn tree_name_length_does_not_follow_the_root_name() {
        // 远端树路径 ＝ <根>/projects/<项目>/<树名>：树名跟着本机树根末段变长，套接字一类按路径长度算的就在远端先超
        let short = tree_name(Path::new("/r/w"));
        let long = tree_name(Path::new(&format!("/r/{}", "x".repeat(40))));
        assert_eq!(short.len(), long.len(), "{short} · {long}");
        assert_ne!(short, long);
    }

    #[test]
    fn tmux_path_in_env_assignment_is_not_starting_tmux() {
        let (t, c) = tree();
        let cwd = t.path().join("w1");
        let argv = sv(&["env", "TMUX_TMPDIR=/x/tmux", "cargo", "test", "-p", "a"]);
        let (p, _) = make_plan(&c.projects, &cwd, &argv, &Vars::new(), false);
        assert_eq!(p.local, None);
        let (p, _) = make_plan(&c.projects, &cwd, &sv(&["tmux", "ls"]), &Vars::new(), false);
        assert!(p.local.is_some(), "不合 remote_if");
        let (p, _) = make_plan(
            &c.projects,
            &cwd,
            &sv(&["cargo", "build", "--release"]),
            &Vars::new(),
            false,
        );
        assert_eq!(p.local.as_deref(), Some("不合 remote_if"));
    }

    #[test]
    fn declared_needs_flags_and_target() {
        let (t, c) = tree();
        let cwd = t.path().join("w1");
        let mut v = Vars::new();
        v.insert("DEVSCHED_NEED".into(), "jdk, android".into());
        v.insert("ALPHA_PRIO".into(), "1".into());
        v.insert("CARGO_TARGET_DIR".into(), "/elsewhere/t".into());
        let (p, _) = make_plan(&c.projects, &cwd, &sv(&["cargo", "check"]), &v, false);
        assert!(p.needs.contains("jdk") && p.needs.contains("android"));
        assert!(p.prio);
        assert_eq!(p.target, Some(PathBuf::from("/elsewhere/t")));
        let (p, _) = make_plan(
            &c.projects,
            &cwd,
            &sv(&["env", "CARGO_TARGET_DIR=/x/y", "cargo", "check"]),
            &v,
            true,
        );
        assert_eq!(p.target, Some(PathBuf::from("/x/y")));
        assert_eq!(p.local.as_deref(), Some("--local"));
        v.insert("ALPHA_LOCAL".into(), "1".into());
        let (p, _) = make_plan(&c.projects, &cwd, &sv(&["cargo", "check"]), &v, false);
        assert_eq!(p.local.as_deref(), Some("ALPHA_LOCAL=1"));
    }

    #[test]
    fn own_and_heavy_are_local_only() {
        let (t, c) = tree();
        let cwd = t.path().join("w1");
        let (p, _) = make_plan(
            &c.projects,
            &cwd,
            &sv(&["cargo", "test", "bless"]),
            &Vars::new(),
            false,
        );
        assert!(p.own && p.local.is_some());
        let (p, _) = make_plan(
            &c.projects,
            &cwd,
            &sv(&["bash", "tests/scripts/quick-check.sh", "--only", "mobile"]),
            &Vars::new(),
            false,
        );
        assert!(p.heavy);
        assert_eq!(p.local.as_deref(), Some("本机重活（heavy_if）"));
    }

    #[test]
    fn nearest_marker_wins_across_projects() {
        let (t, c) = tree();
        // w1（alpha）里头嵌一棵 beta 的树
        let inner = t.path().join("w1/vendor/b");
        std::fs::create_dir_all(inner.join("crates/beta-core")).unwrap();
        std::fs::write(inner.join("crates/beta-core/Cargo.toml"), "").unwrap();
        let (p, _) = make_plan(
            &c.projects,
            &inner.join("crates"),
            &sv(&["cargo", "test"]),
            &Vars::new(),
            false,
        );
        assert_eq!(p.project.unwrap().name, "beta");
        assert_eq!(p.root, inner);
    }

    #[test]
    fn unknown_tree_is_local_only() {
        let t = TempDir::new("plan-unknown");
        let c = Config::load(&fixtures()).unwrap();
        let (p, _) = make_plan(
            &c.projects,
            t.path(),
            &sv(&["cargo", "test"]),
            &Vars::new(),
            false,
        );
        assert!(p.project.is_none());
        assert_eq!(p.local.as_deref(), Some("认不出是哪个项目的树"));
    }
}
