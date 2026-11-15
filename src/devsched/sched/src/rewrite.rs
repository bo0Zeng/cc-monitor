//! 路径改写（只这一处做）：出门把本机树根 · 树外产物目录改成节点那台的写法，回来把输出里的改回。

use crate::plan::{digest8, Plan};
use std::collections::BTreeMap;
use std::path::Path;

/// 一对一对的本机 ↔ 远端前缀。
#[derive(Debug, Clone, PartialEq)]
pub struct Rewriter {
    /// (本机, 远端)，长的在前
    pairs: Vec<(String, String)>,
    /// 这一趟在那台 `<根>/projects/<项目>/` 下用的、跟着这棵树走的名字：树本身 · 树外产物目录 `target-<摘要>`
    own: Vec<String>,
    /// 同一层里送去的、却不是树的名字：兄弟仓（siblings）· 只读附带（data）
    shared: Vec<String>,
}

/// 路径名里的字：前缀之后紧跟着这些就不是这条路径（`/a/w1` 不改 `/a/w12`）。
fn is_name_char(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '-' | '_' | '.' | '+' | '@' | '~')
}

fn replace_prefix(s: &str, from: &str, to: &str) -> String {
    if from.is_empty() {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find(from) {
        let after = &rest[i + from.len()..];
        let before_ok = rest[..i]
            .chars()
            .next_back()
            .is_none_or(|c| !is_name_char(c) && c != '/');
        let after_ok = after.chars().next().is_none_or(|c| !is_name_char(c));
        out.push_str(&rest[..i]);
        if before_ok && after_ok {
            out.push_str(to);
        } else {
            out.push_str(from);
        }
        rest = after;
    }
    out.push_str(rest);
    out
}

impl Rewriter {
    /// `node_root` 是那台 hello 报的根。
    pub fn new(plan: &Plan, node_root: &str) -> Rewriter {
        let project = plan
            .project
            .as_ref()
            .map(|p| p.name.as_str())
            .unwrap_or("_");
        let base = format!("{}/projects/{project}", node_root.trim_end_matches('/'));
        let local_root = plan.root.to_string_lossy().to_string();
        let mut pairs = vec![(local_root.clone(), format!("{base}/{}", plan.tree))];
        let mut own = vec![plan.tree.clone()];
        let mut shared = Vec::new();
        // 兄弟仓（siblings）· 只读附带（data）：照树根一样送，路径照树根一样改
        if let Some(p) = &plan.project {
            for d in p.list("siblings").into_iter().chain(p.list("data")) {
                let name = crate::plan::tree_name(Path::new(&d));
                pairs.push((
                    d.trim_end_matches('/').to_string(),
                    format!("{base}/{name}"),
                ));
                shared.push(name);
            }
        }
        if let Some(t) = &plan.target {
            if !t.starts_with(&plan.root) {
                let tl = t.to_string_lossy().to_string();
                let name = format!("target-{}", digest8(&tl));
                pairs.push((tl.clone(), format!("{base}/{name}")));
                own.push(name);
            }
        }
        pairs.sort_by_key(|p| std::cmp::Reverse(p.0.len()));
        Rewriter { pairs, own, shared }
    }

    /// 这一趟跟着这棵树走、在项目目录下占的名字（树 · 树外产物目录）：一起记「用过」，这一趟都不删。
    pub fn own_names(&self) -> &[String] {
        &self.own
    }

    /// 项目目录下送去的、却不是树的名字（兄弟仓 · 只读附带）：按上限删树时不算。
    pub fn shared_names(&self) -> &[String] {
        &self.shared
    }

    /// 那台上这棵树的根。
    pub fn remote_tree(&self, plan: &Plan) -> String {
        self.out(&plan.root.to_string_lossy())
    }

    pub fn out(&self, s: &str) -> String {
        self.pairs
            .iter()
            .fold(s.to_string(), |acc, (l, r)| replace_prefix(&acc, l, r))
    }

    pub fn back(&self, s: &str) -> String {
        let mut by_remote: Vec<&(String, String)> = self.pairs.iter().collect();
        by_remote.sort_by_key(|p| std::cmp::Reverse(p.1.len()));
        by_remote
            .into_iter()
            .fold(s.to_string(), |acc, (l, r)| replace_prefix(&acc, r, l))
    }

    pub fn out_argv(&self, argv: &[String]) -> Vec<String> {
        argv.iter().map(|a| self.out(a)).collect()
    }

    pub fn out_env(&self, env: &BTreeMap<String, String>) -> BTreeMap<String, String> {
        env.iter().map(|(k, v)| (k.clone(), self.out(v))).collect()
    }
}

/// 一行最多攒多少字节等换行；超了照改照放（不无限攒）。
pub const PENDING_CAP: usize = 64 * 1024;

/// 一路输出（标准输出或标准错误）没收完的那半行：一条路径可能被拆在两帧之间，攒到换行再改回本机写法。
#[derive(Debug, Default)]
pub struct Pending {
    buf: String,
}

impl Pending {
    /// 收一帧：放出到最后一个换行为止（改好的），剩下的半行留着。
    pub fn feed(&mut self, rw: &Rewriter, data: &str) -> String {
        self.buf.push_str(data);
        if self.buf.len() > PENDING_CAP {
            return self.flush(rw);
        }
        match self.buf.rfind('\n') {
            Some(i) => {
                let rest = self.buf.split_off(i + 1);
                let done = std::mem::replace(&mut self.buf, rest);
                rw.back(&done)
            }
            None => String::new(),
        }
    }

    /// 放出留着的半行（收尾 · 一阵没有新输出时）。
    pub fn flush(&mut self, rw: &Rewriter) -> String {
        rw.back(&std::mem::take(&mut self.buf))
    }
}

/// 树里的相对目录在那台的写法（同一个，只是确认不带头尾 `/`）。
pub fn rel_for_job(rel: &str) -> String {
    Path::new(rel)
        .to_string_lossy()
        .trim_matches('/')
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Project;
    use crate::plan::Vars;
    use std::path::PathBuf;

    fn plan(root: &str, target: Option<&str>) -> Plan {
        Plan {
            project: Some(Project {
                name: "alpha".into(),
                ..Default::default()
            }),
            root: PathBuf::from(root),
            tree: crate::plan::tree_name(Path::new(root)),
            cwd: PathBuf::from(root),
            rel: String::new(),
            argv: vec![],
            needs: Default::default(),
            own: false,
            local: None,
            heavy: false,
            prio: false,
            target: target.map(PathBuf::from),
            vars: Vars::new(),
        }
    }

    #[test]
    fn tree_root_and_outside_target_go_out_and_come_back() {
        let p = plan("/home/u/wt/w1", Some("/home/u/tgt"));
        let rw = Rewriter::new(&p, "/srv/ds/");
        let tree = format!("/srv/ds/projects/alpha/{}", p.tree);
        assert_eq!(rw.remote_tree(&p), tree);
        let argv = vec![
            "--manifest-path=/home/u/wt/w1/Cargo.toml".to_string(),
            "/home/u/wt/w12/x".to_string(),
        ];
        let out = rw.out_argv(&argv);
        assert_eq!(out[0], format!("--manifest-path={tree}/Cargo.toml"));
        assert_eq!(out[1], "/home/u/wt/w12/x", "别的树不动");
        let tdig = digest8("/home/u/tgt");
        let mut env = BTreeMap::new();
        env.insert("CARGO_TARGET_DIR".to_string(), "/home/u/tgt".to_string());
        assert_eq!(
            rw.out_env(&env)["CARGO_TARGET_DIR"],
            format!("/srv/ds/projects/alpha/target-{tdig}")
        );
        let remote_out = format!(
            "error at {tree}/src/a.rs:1\n  --> /srv/ds/projects/alpha/target-{tdig}/debug\n"
        );
        assert_eq!(
            rw.back(&remote_out),
            "error at /home/u/wt/w1/src/a.rs:1\n  --> /home/u/tgt/debug\n"
        );
        // 跨趟不变
        assert_eq!(Rewriter::new(&p, "/srv/ds"), rw);
    }

    #[test]
    fn target_inside_tree_follows_the_tree() {
        let p = plan("/home/u/wt/w1", Some("/home/u/wt/w1/.build"));
        let rw = Rewriter::new(&p, "/srv/ds");
        assert_eq!(
            rw.out("/home/u/wt/w1/.build/x"),
            format!("/srv/ds/projects/alpha/{}/.build/x", p.tree)
        );
    }

    #[test]
    fn path_split_across_frames_still_comes_back() {
        let p = plan("/home/u/wt/w1", None);
        let rw = Rewriter::new(&p, "/srv/ds");
        let tree = format!("/srv/ds/projects/alpha/{}", p.tree);
        let (a, b) = tree.split_at(12);
        let mut pend = Pending::default();
        let mut got = pend.feed(&rw, &format!("ok\nerror at {a}"));
        got += &pend.feed(&rw, &format!("{b}/src/a.rs:1\nnext"));
        assert_eq!(
            got, "ok\nerror at /home/u/wt/w1/src/a.rs:1\n",
            "半截行留着等下一帧"
        );
        assert_eq!(pend.flush(&rw), "next");
        assert_eq!(pend.flush(&rw), "");
        // 一行太长不等换行：照样放出去，不无限攒
        let long = "x".repeat(PENDING_CAP + 10);
        assert_eq!(pend.feed(&rw, &long), long);
    }

    #[test]
    fn rel_has_no_slashes_at_ends() {
        assert_eq!(rel_for_job("/a/b/"), "a/b");
        assert_eq!(rel_for_job(""), "");
    }
}
