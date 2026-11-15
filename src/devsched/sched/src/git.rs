//! 远端树带上 git（项目配置 `git = 1`）：快检这类要问 git「改了哪些」的命令在远端照样问得出、答得跟本机一样。
//!
//! 送源码那一步多两下（源码本身照旧 rsync）：
//! - 推：本机把索引照原样写成一个提交（带上已 add 未提交的；身份固定 `devsched`），连同 HEAD · main · origin/main ·
//!   上游推进那台的裸镜像 `<根>/projects/<项目>/<mirror>`（没有就当场建；同一镜像经 `flock` 一次一个）。
//!   只推这几个引用：stash · notes · tag · 钩子 · 本机 `.git/config` 一样不带。
//! - 记（rsync 之后）：镜像里这棵树的工作树记录（`worktrees/<树名>/` 的 gitdir · commondir · HEAD）、远端树的 `.git` 指针、
//!   索引照那个提交读进来再刷新；分支有上游 ⇒ 镜像里只写 `branch.<分支>.remote = .` 与 `merge = <上游的完整引用名>`。
//!
//! 节点不碰镜像；这里只动自己写的那几样（镜像 · `refs/qc/<树名>/*` · `worktrees/<树名>/` · 远端树的 `.git`）。

use crate::dispatch::SyncError;
use crate::plan::Plan;
use crate::rewrite::Rewriter;
use devsched_policy::ExcludeList;
use std::path::{Component, Path, PathBuf};
use std::process::Command;

/// 镜像默认放在 `<根>/projects/<项目>/` 下的这个名字。
pub const DEFAULT_MIRROR: &str = "_mirror.git";

/// 这一趟要推的与要在那台写的。
#[derive(Debug, Clone, PartialEq)]
pub struct GitPrep {
    /// 本机树根（推从这里起）
    pub root: PathBuf,
    /// 那台上的镜像
    pub mirror: String,
    /// 那台上的树
    pub tree: String,
    /// 镜像里这棵树的工作树记录
    pub admin: String,
    /// 工作树记录里 HEAD 的那一行（`ref: refs/heads/<分支>` 或提交号）
    pub headline: String,
    /// 索引写成的那个提交
    pub index_commit: String,
    /// 所在分支（分离的 ⇒ 空）
    pub branch: String,
    /// 上游的完整引用名（没有 ⇒ 空）
    pub upstream: String,
    /// 推的引用（`+<提交>:<引用>`）
    pub specs: Vec<String>,
    /// 本机没跟踪又没忽略、送得出去的（远端的这一类里只留这几份）
    pub keep: Vec<String>,
}

fn q(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// 本机起一下 git（不经 shell）：成了给标准输出（去掉行尾），不成给 None。
fn git_out(root: &Path, args: &[&str], envs: &[(&str, &str)]) -> Option<String> {
    let mut c = Command::new("git");
    c.arg("-C").arg(root).args(args);
    for k in ["GIT_DIR", "GIT_WORK_TREE", "GIT_INDEX_FILE"] {
        c.env_remove(k);
    }
    for (k, v) in envs {
        c.env(k, v);
    }
    let o = c.stderr(std::process::Stdio::null()).output().ok()?;
    o.status
        .success()
        .then(|| String::from_utf8_lossy(&o.stdout).trim_end().to_string())
}

/// 镜像那一格：相对 `<根>/projects/<项目>/` 的一段路径，不许绝对、不许 `..`。
fn mirror_rel(v: &str) -> Result<&str, SyncError> {
    let v = if v.is_empty() { DEFAULT_MIRROR } else { v };
    let ok = Path::new(v)
        .components()
        .all(|c| matches!(c, Component::Normal(_)));
    if ok {
        Ok(v.trim_end_matches('/'))
    } else {
        Err(SyncError::Local(format!(
            "项目配置 mirror = {v} 不对：要是 <根>/projects/<项目>/ 下的相对路径"
        )))
    }
}

/// 树里提交着的路径有没有排除清单点到的（连它上面的哪一层目录被点到也算）。
fn excluded_tracked(root: &Path, ex: &ExcludeList) -> Result<Option<String>, SyncError> {
    let out = git_out(root, &["ls-files", "-z"], &[])
        .ok_or_else(|| SyncError::Local("读不了本机索引（git ls-files）".into()))?;
    Ok(out
        .split('\0')
        .find(|p| !p.is_empty() && hit_by(Path::new(p), ex))
        .map(String::from))
}

/// 这条路径（或它上面哪一层目录）被排除清单点到了吗。
fn hit_by(path: &Path, ex: &ExcludeList) -> bool {
    let mut hit = ex.is_excluded(path, false);
    let mut cur = path.parent();
    while let Some(d) = cur.filter(|d| !d.as_os_str().is_empty()) {
        hit |= ex.is_excluded(d, true);
        cur = d.parent();
    }
    hit
}

/// 本机没跟踪又没忽略、送得出去的那几份（远端「没跟踪又没忽略的」里只留这几份）。名字里带换行的不算（那一份远端会被删，下一趟照送）。
fn local_untracked(root: &Path, ex: &ExcludeList) -> Result<Vec<String>, SyncError> {
    let out = git_out(
        root,
        &["ls-files", "-z", "--others", "--exclude-standard"],
        &[],
    )
    .ok_or_else(|| SyncError::Local("读不了本机没跟踪的文件（git ls-files --others）".into()))?;
    Ok(out
        .split('\0')
        .filter(|p| !p.is_empty() && !p.contains('\n') && !hit_by(Path::new(p), ex))
        .map(String::from)
        .collect())
}

/// 照项目配置算这一趟的 git 那一份；不是 `git = 1`、或树根不是 git 顶层 ⇒ None。
/// `work` 是本机放临时索引的目录。
pub fn prepare(
    plan: &Plan,
    rw: &Rewriter,
    ex: &ExcludeList,
    work: &Path,
) -> Result<Option<GitPrep>, SyncError> {
    let Some(p) = plan.project.as_ref().filter(|p| p.get("git") == "1") else {
        return Ok(None);
    };
    let root = &plan.root;
    let top = git_out(root, &["rev-parse", "--show-toplevel"], &[]);
    let same = top
        .as_deref()
        .and_then(|t| Path::new(t).canonicalize().ok())
        .zip(root.canonicalize().ok())
        .is_some_and(|(a, b)| a == b);
    if !same {
        return Ok(None);
    }
    // 镜像那一格写错 ⇒ 这一趟只本机（推之前就说）
    mirror_rel(p.get("mirror"))?;
    if let Some(hit) = excluded_tracked(root, ex)? {
        return Err(SyncError::Local(format!(
            "仓里提交着排除清单点到的 {hit}：git = 1 不推，这一趟只本机"
        )));
    }
    let lf = |m: &str| SyncError::Local(m.to_string());
    // 索引拷一份再写树（write-tree 会回写索引，不碰本机那份）
    // `--git-path` 给的可能是相对 `-C` 那一层的（`--path-format=absolute` 要 git 2.31，不用）
    let idx = root.join(
        git_out(root, &["rev-parse", "--git-path", "index"], &[])
            .ok_or_else(|| lf("找不到本机索引"))?,
    );
    std::fs::create_dir_all(work).map_err(|e| lf(&format!("建不了 {}：{e}", work.display())))?;
    let tmp = work.join(format!("index.{}", std::process::id()));
    std::fs::copy(&idx, &tmp).map_err(|e| lf(&format!("拷不了本机索引：{e}")))?;
    let tmp_s = tmp.display().to_string();
    let tree = git_out(root, &["write-tree"], &[("GIT_INDEX_FILE", &tmp_s)]);
    let _ = std::fs::remove_file(&tmp);
    let tree = tree.ok_or_else(|| lf("索引写不成树（有没解决的冲突？）"))?;
    let who = [
        ("GIT_AUTHOR_NAME", "devsched"),
        ("GIT_AUTHOR_EMAIL", "devsched@localhost"),
        ("GIT_AUTHOR_DATE", "@1 +0000"),
        ("GIT_COMMITTER_NAME", "devsched"),
        ("GIT_COMMITTER_EMAIL", "devsched@localhost"),
        ("GIT_COMMITTER_DATE", "@1 +0000"),
    ];
    let name = &plan.tree;
    let index_commit = git_out(
        root,
        &[
            "commit-tree",
            "--no-gpg-sign",
            "-m",
            &format!("devsched index {name}"),
            &tree,
        ],
        &who,
    )
    .ok_or_else(|| lf("索引写不成提交"))?;
    let head = git_out(root, &["rev-parse", "-q", "--verify", "HEAD^{commit}"], &[])
        .ok_or_else(|| lf("没有 HEAD"))?;
    let head_ref = git_out(root, &["symbolic-ref", "-q", "HEAD"], &[]).unwrap_or_default();
    let mut specs = vec![format!("+{index_commit}:refs/qc/{name}/index")];
    let headline = if head_ref.is_empty() {
        specs.push(format!("+{head}:refs/qc/{name}/HEAD"));
        head.clone()
    } else {
        specs.push(format!("+{head}:{head_ref}"));
        format!("ref: {head_ref}")
    };
    let upstream = git_out(
        root,
        &["rev-parse", "--symbolic-full-name", "@{upstream}"],
        &[],
    )
    .filter(|u| u.starts_with("refs/heads/") || u.starts_with("refs/remotes/"))
    .unwrap_or_default();
    for r in [
        "refs/heads/main",
        "refs/remotes/origin/main",
        upstream.as_str(),
    ] {
        if r.is_empty() || specs.iter().any(|s| s.ends_with(&format!(":{r}"))) {
            continue;
        }
        if let Some(c) = git_out(
            root,
            &["rev-parse", "-q", "--verify", &format!("{r}^{{commit}}")],
            &[],
        ) {
            specs.push(format!("+{c}:{r}"));
        }
    }
    let remote_tree = rw.remote_tree(plan);
    let (mirror, _) = mirror_paths(plan, rw, p)?;
    Ok(Some(GitPrep {
        root: root.clone(),
        admin: format!("{mirror}/worktrees/{name}"),
        mirror,
        tree: remote_tree,
        headline,
        index_commit,
        branch: head_ref
            .strip_prefix("refs/heads/")
            .unwrap_or_default()
            .to_string(),
        upstream,
        specs,
        keep: local_untracked(root, ex)?,
    }))
}

/// 推的那一头在那台起的：建镜像（没有才建）、拿镜像的锁，再交给 receive-pack（git 在后面接上镜像路径）。
const RECEIVE: &str = "test -d \"$1\" || { git init -q --bare \"$1\" && git -C \"$1\" config receive.denyCurrentBranch ignore && git -C \"$1\" config gc.auto 0; }; exec git receive-pack \"$1\"";

/// rsync 之后在那台写的：工作树记录 · 远端树的 .git 指针 · 上游那两项 · 索引；
/// 最后删掉远端「没跟踪又没忽略的」里本机没有的（`$8`：本机那一份，一行一个；被忽略的不碰）。
const FINISH: &str = r#"set -e
M=$1 A=$2 T=$3 H=$4 C=$5 B=$6 U=$7 K=$8
exec 9>"$M.lock"; flock 9
mkdir -p "$A"
printf '%s\n' "$T/.git" > "$A/gitdir"
printf '../..\n' > "$A/commondir"
printf '%s\n' "$H" > "$A/HEAD"
printf 'gitdir: %s\n' "$A" > "$T/.git"
if [ -n "$B" ]; then
  git --git-dir="$M" config --unset-all "branch.$B.remote" || true
  git --git-dir="$M" config --unset-all "branch.$B.merge" || true
  if [ -n "$U" ]; then git --git-dir="$M" config "branch.$B.remote" . && git --git-dir="$M" config "branch.$B.merge" "$U"; fi
fi
cd "$T"
git read-tree -m "$C" 2>/dev/null || git read-tree "$C"
git update-index -q --refresh >/dev/null || true
printf '%s\n' "$K" > "$A/keep"
git ls-files -z --others --exclude-standard | tr '\0' '\n' | { grep -vxF -f "$A/keep" || true; } | tr '\n' '\0' | xargs -0 -r rm -f --"#;

/// 在那台对一遍镜像：`$P/<树>` 已经不在的树（删了的 · 手动删的），删它的工作树记录与 `refs/qc/<树>/` 下的引用。
/// 在镜像锁里做（写记录的 `FINISH` 也拿这把锁，且总在 rsync 建好树之后写）；镜像不在 ⇒ 什么都不做。
const SWEEP: &str = r#"set -e
M=$1 P=$2
[ -d "$M" ] || exit 0
exec 9>"$M.lock"; flock 9
for w in "$M"/worktrees/*; do
  [ -e "$w" ] || continue
  t=${w##*/}
  [ -d "$P/$t" ] || rm -rf "$w"
done
git --git-dir="$M" for-each-ref --format='%(refname)' refs/qc/ | while read -r r; do
  t=${r#refs/qc/}; t=${t%%/*}
  [ -d "$P/$t" ] || git --git-dir="$M" update-ref -d "$r"
done"#;

/// 那台上这个项目的（镜像, 树所在的那一层）：照节点报的根，`<根>/projects/<项目>/<mirror>` 与 `<根>/projects/<项目>`。
/// 送源码 · 列树 · 对镜像都经这一处算。
pub fn mirror_paths(
    plan: &Plan,
    rw: &Rewriter,
    project: &crate::config::Project,
) -> Result<(String, String), SyncError> {
    let dir = project_dir(plan, rw);
    Ok((format!("{dir}/{}", mirror_rel(project.get("mirror"))?), dir))
}

/// 那台上这个项目的树所在的那一层（这一趟远端树的上一层）。
pub fn project_dir(plan: &Plan, rw: &Rewriter) -> String {
    let t = rw.remote_tree(plan);
    Path::new(&t)
        .parent()
        .map(|d| d.display().to_string())
        .unwrap_or_default()
}

/// 镜像在项目目录下的第一段（列树时它与它的 `.lock` 不算树）；配置写错 ⇒ 默认那个名字。
pub fn mirror_first(project: &crate::config::Project) -> String {
    mirror_rel(project.get("mirror"))
        .unwrap_or(DEFAULT_MIRROR)
        .split('/')
        .next()
        .unwrap_or(DEFAULT_MIRROR)
        .to_string()
}

/// 对一遍镜像（见 `SWEEP`）：一下 ssh。
pub fn sweep_argv(dest: &str, ssh: &str, mirror: &str, dir: &str) -> Vec<String> {
    let mut v: Vec<String> = ssh.split_whitespace().map(String::from).collect();
    v.push(dest.to_string());
    v.push(format!("sh -c {} sh {} {}", q(SWEEP), q(mirror), q(dir)));
    v
}

impl GitPrep {
    /// 推：`git push` 经 `ssh` 进那台的镜像；本机的钩子 · tag · 签名一样不带。
    pub fn push_argv(&self, dest: &str, ssh: &str) -> Vec<String> {
        let parent = Path::new(&self.mirror)
            .parent()
            .map(|d| d.display().to_string())
            .unwrap_or_default();
        // 先建这棵树的目录：引用一写进镜像，树目录必在（对镜像的 SWEEP 照目录在不在删引用）
        let rp = format!(
            "mkdir -p {} {} && flock {} sh -c {} sh",
            q(&parent),
            q(&self.tree),
            q(&format!("{}.lock", self.mirror)),
            q(RECEIVE)
        );
        let mut v: Vec<String> = vec![
            "git".into(),
            "-C".into(),
            self.root.display().to_string(),
            "-c".into(),
            format!("core.sshCommand={ssh}"),
            "push".into(),
            "-q".into(),
            "--force".into(),
            "--no-verify".into(),
            "--no-follow-tags".into(),
            "--no-signed".into(),
            "--recurse-submodules=no".into(),
            format!("--receive-pack={rp}"),
            format!("{dest}:{}", self.mirror),
        ];
        v.extend(self.specs.iter().cloned());
        v
    }

    /// 记：一下 ssh 在那台跑 `FINISH`。
    pub fn finish_argv(&self, dest: &str, ssh: &str) -> Vec<String> {
        let mut v: Vec<String> = ssh.split_whitespace().map(String::from).collect();
        v.push(dest.to_string());
        let args = [
            &self.mirror,
            &self.admin,
            &self.tree,
            &self.headline,
            &self.index_commit,
            &self.branch,
            &self.upstream,
            &self.keep.join("\n"),
        ]
        .iter()
        .map(|a| q(a))
        .collect::<Vec<_>>()
        .join(" ");
        v.push(format!("sh -c {} sh {args}", q(FINISH)));
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::HostTable;
    use crate::dispatch::Syncer;
    use crate::queue::State;
    use crate::sync::{RealRunner, RsyncSync, Runner};
    use crate::testkit::{plan_in, TempDir};
    use std::io;
    use std::os::unix::fs::PermissionsExt;
    use std::path::PathBuf;
    use std::process::Command;

    /// 真起命令；ssh 换成一份在本机起对面那条命令的脚本（丢掉选项与去法，剩下的交给 sh）。
    struct LocalSsh {
        real: RealRunner,
        ssh: String,
        /// 起过的：是不是 git push · 给的整趟时限（没给 ⇒ None）
        seen: std::sync::Mutex<Vec<(bool, Option<std::time::Duration>)>>,
    }

    impl Runner for LocalSsh {
        fn run(&self, argv: &[String]) -> io::Result<i32> {
            self.seen
                .lock()
                .unwrap()
                .push((argv.iter().any(|a| a == "push"), None));
            self.real.run(argv)
        }
        fn run_within(&self, argv: &[String], wall: std::time::Duration) -> io::Result<i32> {
            self.seen
                .lock()
                .unwrap()
                .push((argv.iter().any(|a| a == "push"), Some(wall)));
            self.real.run(argv)
        }
        fn ssh(&self) -> String {
            self.ssh.clone()
        }
    }

    fn fake_ssh(dir: &Path) -> String {
        let d = dir.join("bin");
        std::fs::create_dir_all(&d).unwrap();
        let p = d.join("ssh");
        crate::testkit::write_exe(
            &p,
            "#!/bin/sh\nunset GIT_DIR GIT_WORK_TREE GIT_INDEX_FILE\n\
             while [ $# -gt 0 ]; do case \"$1\" in -o|-p|-l|-i|-F) shift 2 ;; --) shift; break ;; -*) shift ;; *) break ;; esac; done\n\
             shift\nexec sh -c \"$*\"\n",
        );
        p.display().to_string()
    }

    /// 测试里起 git：不读本机全局 · 系统配置。
    fn g(dir: &Path, args: &[&str]) -> String {
        let o = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(args)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .env_remove("GIT_INDEX_FILE")
            .output()
            .unwrap();
        assert!(
            o.status.success(),
            "git {args:?} 在 {}：{}",
            dir.display(),
            String::from_utf8_lossy(&o.stderr)
        );
        String::from_utf8_lossy(&o.stdout).into_owned()
    }

    fn w(p: &Path, s: &str) {
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, s).unwrap();
    }

    struct Rig {
        t: TempDir,
        root: PathBuf,
        node: String,
    }

    fn rig(tag: &str) -> Rig {
        let t = TempDir::new(tag);
        let root = t.path().join("w1");
        std::fs::create_dir_all(&root).unwrap();
        // 照最低支持的 git 建仓（`init -b` 要 2.28）
        g(&root, &["init", "-q"]);
        g(&root, &["symbolic-ref", "HEAD", "refs/heads/main"]);
        // 本机的私人项：身份 · remote 地址 · 钩子（推的时候要是跑了钩子就推不成）
        g(&root, &["config", "user.name", "Priv Name"]);
        g(&root, &["config", "user.email", "priv@example.invalid"]);
        g(
            &root,
            &[
                "config",
                "remote.origin.url",
                "https://secret.example.invalid/r.git",
            ],
        );
        w(&root.join(".git/hooks/pre-push"), "#!/bin/sh\nexit 1\n");
        std::fs::set_permissions(
            root.join(".git/hooks/pre-push"),
            std::fs::Permissions::from_mode(0o755),
        )
        .unwrap();
        w(&root.join("a.rs"), "a1\n");
        w(&root.join("b.rs"), "b1\n");
        w(&root.join(".gitignore"), "ignored.log\n");
        g(&root, &["add", "-A"]);
        g(&root, &["commit", "-q", "-m", "c1"]);
        g(&root, &["update-ref", "refs/remotes/origin/main", "HEAD"]);
        g(&root, &["tag", "v1"]);
        g(&root, &["checkout", "-q", "-b", "feat"]);
        w(&root.join("a.rs"), "a2\n");
        g(&root, &["commit", "-q", "-am", "c2"]);
        // stash · notes：不许推过去
        let c1 = g(&root, &["rev-parse", "main"]);
        g(&root, &["update-ref", "refs/stash", c1.trim()]);
        g(&root, &["notes", "add", "-m", "note", "HEAD"]);
        // 已 add 未提交 · 改了没 add · 没 add 的新文件 · 忽略的
        w(&root.join("c.rs"), "c1\n");
        g(&root, &["add", "c.rs"]);
        w(&root.join("b.rs"), "b2\n");
        w(&root.join("d.rs"), "d1\n");
        w(&root.join("ignored.log"), "x\n");
        let node = t.path().join("node").display().to_string();
        Rig { t, root, node }
    }

    fn plan_of(r: &Rig, exclude: &str) -> Plan {
        let mut plan = plan_in(&r.root, &["true"]);
        let p = plan.project.as_mut().unwrap();
        p.keys.insert("git".into(), "1".into());
        p.keys.insert("mirror".into(), "_m.git".into());
        p.keys.insert("exclude".into(), exclude.into());
        plan
    }

    fn sync(r: &Rig, plan: &Plan) -> Result<(), SyncError> {
        let host = HostTable::parse(&format!("a 1 ssh:a-dest base={}\n", r.node))
            .hosts
            .remove(0);
        let rw = Rewriter::new(plan, &r.node);
        let runner = LocalSsh {
            real: RealRunner::default(),
            ssh: fake_ssh(r.t.path()),
            seen: Default::default(),
        };
        let res = RsyncSync {
            state: State::new(r.t.path().join("state")),
            runner: &runner,
        }
        .sync(&host, plan, &rw);
        // 推进镜像的那一下用推的时限（首推带整段历史），别的照送目录的
        for (push, wall) in runner.seen.into_inner().unwrap() {
            assert_eq!(wall, push.then_some(crate::sync::GIT_PUSH_WALL));
        }
        res
    }

    /// 快检读的那几样，本机与远端各问一遍。
    fn views(dir: &Path) -> Vec<String> {
        let base = g(dir, &["merge-base", "HEAD", "origin/main"]);
        let mut v = vec![
            g(dir, &["diff", "--name-only", base.trim()]),
            g(dir, &["ls-files", "--others", "--exclude-standard"]),
            g(dir, &["log", "-1", "--format=%H %an %ae %s"]),
            g(dir, &["diff", "--cached", "--name-only", "HEAD"]),
            g(dir, &["rev-parse", "--abbrev-ref", "HEAD"]),
            base,
        ];
        let up = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(["rev-parse", "--abbrev-ref", "@{upstream}"])
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .output()
            .unwrap();
        v.push(format!(
            "{} {}",
            up.status.success(),
            String::from_utf8_lossy(&up.stdout)
        ));
        v
    }

    #[test]
    fn remote_tree_answers_git_like_the_local_one() {
        let r = rig("git1");
        g(&r.root, &["branch", "-q", "--set-upstream-to=main"]);
        let plan = plan_of(&r, ".git");
        sync(&r, &plan).unwrap();
        let remote = PathBuf::from(Rewriter::new(&plan, &r.node).remote_tree(&plan));
        assert_eq!(
            g(&remote, &["rev-parse", "--show-toplevel"]).trim(),
            remote.display().to_string()
        );
        let local = views(&r.root);
        assert!(
            local[0].contains("b.rs") && local[0].contains("c.rs"),
            "{local:?}"
        );
        assert!(local[3].contains("c.rs"), "已 add 未提交的：{local:?}");
        assert_eq!(views(&remote), local);

        // 镜像里只有推的那几个引用；config 里没有本机的私人项；没有钩子
        let m = Path::new(&r.node).join("projects/alpha/_m.git");
        let refs = g(&m, &["for-each-ref", "--format=%(refname)"]);
        let mut refs: Vec<&str> = refs.lines().collect();
        refs.sort();
        assert_eq!(
            refs,
            [
                "refs/heads/feat",
                "refs/heads/main",
                &format!("refs/qc/{}/index", plan.tree),
                "refs/remotes/origin/main",
            ]
        );
        let cfg = std::fs::read_to_string(m.join("config")).unwrap();
        assert!(
            !cfg.contains("priv") && !cfg.contains("secret.example") && !cfg.contains("Priv"),
            "{cfg}"
        );
        let hooks: Vec<String> = std::fs::read_dir(m.join("hooks"))
            .map(|d| {
                d.flatten()
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .filter(|n| !n.ends_with(".sample"))
                    .collect()
            })
            .unwrap_or_default();
        assert!(hooks.is_empty(), "{hooks:?}");
        let who = g(
            &m,
            &[
                "log",
                "-1",
                "--format=%an %ae %cn %ce",
                &format!("refs/qc/{}/index", plan.tree),
            ],
        );
        assert!(!who.contains("priv") && !who.contains("Priv"), "{who}");

        // 第二趟：去掉上游 · 提交 · 删一份 ⇒ 照旧一致（起点落到 origin/main 那一档），上游那两项从镜像里删掉
        g(&r.root, &["branch", "-q", "--unset-upstream"]);
        g(&r.root, &["commit", "-q", "-m", "c3"]);
        std::fs::remove_file(r.root.join("d.rs")).unwrap();
        w(&r.root.join("e.rs"), "e\n");
        g(&r.root, &["add", "e.rs"]);
        sync(&r, &plan).unwrap();
        assert_eq!(views(&remote), views(&r.root));
        assert!(!std::fs::read_to_string(m.join("config"))
            .unwrap()
            .contains("branch \"feat\""));

        // 第三趟：远端作业写过的（没跟踪的新文件 · 改了跟踪的）照本机复原，被忽略的不碰；本机没跟踪的照留
        w(&remote.join("stale.rs"), "s\n");
        w(&remote.join("gen/stale.ts"), "s\n");
        w(&remote.join("a.rs"), "written by job\n");
        w(&remote.join("out/ignored.log"), "keep\n");
        w(&r.root.join("f.rs"), "f\n");
        sync(&r, &plan).unwrap();
        assert!(!remote.join("stale.rs").exists() && !remote.join("gen/stale.ts").exists());
        assert_eq!(
            std::fs::read_to_string(remote.join("a.rs")).unwrap(),
            "a2\n"
        );
        assert!(remote.join("out/ignored.log").exists(), "被忽略的不碰");
        assert!(remote.join("f.rs").exists());
        assert_eq!(views(&remote), views(&r.root));
    }

    #[test]
    fn tracked_file_hit_by_exclude_is_not_pushed() {
        let r = rig("git2");
        w(&r.root.join("k/secret.key"), "s\n");
        g(&r.root, &["add", "-f", "k/secret.key"]);
        let plan = plan_of(&r, ".git *.key");
        assert!(matches!(sync(&r, &plan), Err(SyncError::Local(_))));
        assert!(!Path::new(&r.node).join("projects/alpha/_m.git").exists());
    }

    #[test]
    fn without_git_key_or_outside_a_repo_nothing_git_goes() {
        let r = rig("git3");
        let mut plan = plan_of(&r, ".git");
        plan.project.as_mut().unwrap().keys.remove("git");
        sync(&r, &plan).unwrap();
        let rw = Rewriter::new(&plan, &r.node);
        assert!(!Path::new(&rw.remote_tree(&plan)).join(".git").exists());
        assert!(!Path::new(&r.node).join("projects/alpha/_m.git").exists());
        // 树根不是 git 顶层（树在仓的子目录里）⇒ 照旧只送源码
        let sub = r.root.join("sub");
        w(&sub.join("x.rs"), "x\n");
        let mut plan = plan_in(&sub, &["true"]);
        let p = plan.project.as_mut().unwrap();
        p.keys.insert("git".into(), "1".into());
        sync(&r, &plan).unwrap();
        assert!(!Path::new(&r.node)
            .join("projects/alpha/_mirror.git")
            .exists());
    }

    #[test]
    fn push_makes_the_tree_dir_before_its_refs_land() {
        let p = GitPrep {
            root: PathBuf::from("/w"),
            mirror: "/r/projects/alpha/_mirror.git".into(),
            tree: "/r/projects/alpha/ab12cd34".into(),
            admin: String::new(),
            headline: String::new(),
            index_commit: String::new(),
            branch: String::new(),
            upstream: String::new(),
            specs: vec![],
            keep: vec![],
        };
        let rp = p
            .push_argv("d", "ssh")
            .into_iter()
            .find(|a| a.starts_with("--receive-pack="))
            .unwrap();
        assert!(
            rp.starts_with(
                "--receive-pack=mkdir -p '/r/projects/alpha' '/r/projects/alpha/ab12cd34' && flock"
            ),
            "{rp}"
        );
    }

    #[test]
    fn sweeping_drops_mirror_records_of_trees_that_are_gone() {
        let t = TempDir::new("git-sweep");
        let src = t.path().join("src");
        std::fs::create_dir_all(&src).unwrap();
        g(&src, &["init", "-q"]);
        w(&src.join("a.rs"), "a\n");
        g(&src, &["add", "-A"]);
        g(
            &src,
            &[
                "-c",
                "user.name=x",
                "-c",
                "user.email=x@invalid",
                "commit",
                "-q",
                "-m",
                "1",
            ],
        );
        let m = t.path().join("projects/alpha/_mirror.git");
        std::fs::create_dir_all(m.parent().unwrap()).unwrap();
        g(t.path(), &["init", "-q", "--bare", m.to_str().unwrap()]);
        for tr in ["aaaa1111", "bbbb2222"] {
            for r in ["index", "HEAD"] {
                g(
                    &src,
                    &[
                        "push",
                        "-q",
                        m.to_str().unwrap(),
                        &format!("+HEAD:refs/qc/{tr}/{r}"),
                    ],
                );
            }
            w(&m.join(format!("worktrees/{tr}/HEAD")), "x\n");
        }
        let r = LocalSsh {
            real: RealRunner::default(),
            ssh: fake_ssh(t.path()),
            seen: Default::default(),
        };
        // bbbb2222 那棵树还在、aaaa1111 已经删了
        let dir = t.path().join("projects/alpha");
        std::fs::create_dir_all(dir.join("bbbb2222")).unwrap();
        let argv = sweep_argv(
            "a-dest",
            &r.ssh(),
            m.to_str().unwrap(),
            dir.to_str().unwrap(),
        );
        assert_eq!(r.run(&argv).unwrap(), 0);
        let refs = g(&m, &["for-each-ref", "--format=%(refname)"]);
        assert_eq!(refs, "refs/qc/bbbb2222/HEAD\nrefs/qc/bbbb2222/index\n");
        assert!(!m.join("worktrees/aaaa1111").exists());
        assert!(
            m.join("worktrees/bbbb2222/HEAD").exists(),
            "树还在的记录照旧"
        );
        // 镜像不在 ⇒ 什么都不做、退 0
        let none = sweep_argv("a-dest", &r.ssh(), "/nonexistent/m.git", "/nonexistent");
        assert_eq!(r.run(&none).unwrap(), 0);
    }

    #[test]
    fn mirror_must_stay_under_the_project_dir() {
        let r = rig("git4");
        for bad in ["/abs.git", "../up.git", "a/../../b.git"] {
            let mut plan = plan_of(&r, ".git");
            plan.project
                .as_mut()
                .unwrap()
                .keys
                .insert("mirror".into(), bad.into());
            assert!(matches!(sync(&r, &plan), Err(SyncError::Local(_))), "{bad}");
        }
    }
}
