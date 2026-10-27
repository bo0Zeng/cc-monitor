//! 开一片之前问的那一份（`plan-new`）：在哪个目录开、能选哪几档领域、会落在哪、撞没撞上已有的一片、要敲的那一行。只读：只起 `pb dump --domains`。
//!
//! - 领域照 pb：`pb dump --domains <目录>` 从那个目录往上找工作区，找到 ⇒ 它认得的，找不到 ⇒ 插件里的几档（目录还不在也行）。
//!   当前目录设在那个目录往上第一个在的祖先（起子进程要它在）。
//! - 工作区根：pb 找到的那一个；找不到 ⇒ 这个目录本身（`pb repo` 在这里敲，它就成了工作区根）。
//! - 撞上：路径最后一段（＝ 仓库名）等于那个工作区已有的一片 ⇒ 那一片与它的领域；那一行只带名字（pb 的「切到那个仓库」，领域建了不改）。
//! - 名字合不合 pb 的规矩这里不判：提交即起 `pb repo`，照它的退出码与那一句（[`super::command`]）。

use super::locate::Missing;
use super::wire::{PlanNewExisting, PlanNewForm, PlanPb};
use copy_core::copy_text;
use std::path::{Path, PathBuf};

/// 问的是什么：那台机器上的目录 · 相对工作区根的路径 · 选了哪一档领域。
pub(crate) struct Ask<'a> {
    pub(crate) dir: &'a str,
    pub(crate) path: &'a str,
    pub(crate) domain: Option<&'a str>,
}

/// 一个工作区里已有的几片（名字 · 领域）：生产照本子里读好过的那一份（没读好过现读一次）。
pub(crate) type SlicesPort<'a> = &'a dyn Fn(&str) -> Vec<(String, Option<String>)>;

/// [`Ask`] ⇒ 回包。`entry` 是找 pb 的结局（找不到 ⇒ 那一句，不起子进程）。
pub(crate) fn form(ask: &Ask, entry: Result<PathBuf, Missing>, slices: SlicesPort) -> PlanNewForm {
    let dir = trim(ask.dir);
    let mut out = PlanNewForm {
        pb: pb_state("ok", None),
        workspace: None,
        root: dir.to_string(),
        slice: None,
        domains: Vec::new(),
        existing: None,
        repo_dir: None,
        note: None,
        line: None,
        failed: None,
    };
    let entry = match entry {
        Ok(e) => e,
        Err(m) => {
            out.pb = pb_state("missing", Some(m.said()));
            return out;
        }
    };
    let cwd = existing_ancestor(Path::new(dir));
    match super::dump::run_domains(&entry, &cwd, dir) {
        super::dump::Ran::Dump { doc, .. } => {
            out.domains = super::product::domains_of(&doc);
            out.workspace = doc
                .get("workspace")
                .and_then(serde_json::Value::as_str)
                .filter(|w| !w.is_empty())
                .map(|w| trim(w).to_string());
        }
        super::dump::Ran::Unsupported(said) => {
            out.pb = pb_state("unsupported", Some(said));
            return out;
        }
        super::dump::Ran::NotWorkspace(said) => {
            out.failed = Some(said);
            return out;
        }
        super::dump::Ran::Failed { said, .. } => {
            out.failed = Some(said);
            return out;
        }
    }
    if let Some(ws) = &out.workspace {
        out.root = ws.clone();
    }
    let path = ask.path.trim().trim_matches('/');
    let Some(name) = path.rsplit('/').next().filter(|n| !n.is_empty()) else {
        return out;
    };
    out.existing = out.workspace.as_deref().and_then(|ws| {
        slices(ws)
            .into_iter()
            .find(|(n, _)| n == name)
            .map(|(slice, domain)| PlanNewExisting { slice, domain })
    });
    out.slice = Some(name.to_string());
    let repo_dir = format!("{}/{path}", out.root);
    out.note = Some(match &out.existing {
        Some(PlanNewExisting {
            domain: Some(d), ..
        }) => copy_text("bePlan.new.existing", &[("domain", d)]),
        Some(_) => copy_text("bePlan.new.existingBare", &[]),
        None => copy_text("bePlan.new.repoAt", &[("dir", &repo_dir)]),
    });
    out.repo_dir = Some(repo_dir);
    let args = repo_args(
        path,
        ask.domain,
        out.existing.as_ref().map(|e| e.slice.as_str()),
    );
    let shown: Vec<String> = args.iter().map(|a| quoted(a)).collect();
    out.line = Some(format!("pb {}", shown.join(" ")));
    out
}

/// `pb repo` 要敲的那几个参数（[`form`] 写出的那一行 · `plan-command repo` 敲的，同这一判）：撞上 ⇒ 只带名字；没选领域 ⇒ 不带。
pub(crate) fn repo_args(path: &str, domain: Option<&str>, existing: Option<&str>) -> Vec<String> {
    match (existing, domain.filter(|d| !d.is_empty())) {
        (Some(name), _) => vec!["repo".into(), name.into()],
        (None, Some(d)) => vec![
            "repo".into(),
            path.trim().trim_matches('/').into(),
            d.into(),
        ],
        (None, None) => vec!["repo".into(), path.trim().trim_matches('/').into()],
    }
}

fn pb_state(state: &str, said: Option<String>) -> PlanPb {
    PlanPb {
        state: state.to_string(),
        said,
        version: None,
    }
}

fn trim(p: &str) -> &str {
    let t = p.trim();
    match t.trim_end_matches('/') {
        "" => t,
        s => s,
    }
}

/// 起子进程要当前目录在：这个目录不在 ⇒ 往上第一个在的。
fn existing_ancestor(dir: &Path) -> PathBuf {
    dir.ancestors()
        .find(|a| a.is_dir())
        .unwrap_or(Path::new("/"))
        .to_path_buf()
}

/// 那一行里的一个参数：带空白或引号 ⇒ 单引号括起（照 shell 的写法），否则原样。
fn quoted(a: &str) -> String {
    if a.chars()
        .any(|c| c.is_whitespace() || "'\"\\$`".contains(c))
    {
        format!("'{}'", a.replace('\'', r"'\''"))
    } else {
        a.to_string()
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/plan/new_slice_tests.rs"]
mod tests;
