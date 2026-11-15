//! 送源码：照 `devsched-policy` 的送出清单，rsync `--files-from` 送到那台的 `<根>/projects/<项目>/<树名>`。
//! 按内容比、不带 mtime（`-lpD -c`）；不另给 rsync 排除参数。
//! 删远端多出来的：上一趟送过、这一趟清单里没有的（本机删了的），一并写进清单、`--delete-missing-args` 让对面删；
//! 远端自己生成的（产物目录这类，本来就不在清单里）不碰。

use crate::config::{Host, Way};
use crate::dispatch::{SyncError, Syncer};
use crate::plan::Plan;
use crate::queue::State;
use crate::rewrite::Rewriter;
use devsched_policy::{send_list, ExcludeList};
use std::collections::BTreeSet;
use std::io;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::process::Command;

/// 这一趟派不派得了这台（`need_own` 的只派可信机器；这一版 `need_own` 另限只本机，由计划管）。
pub fn may_send(host: &Host, plan: &Plan) -> bool {
    // 项目配置 `hosts`：写了 ⇒ 只派这几台远端
    let listed = plan
        .project
        .as_ref()
        .map(|p| p.list("hosts"))
        .unwrap_or_default();
    (listed.is_empty() || listed.contains(&host.name))
        && devsched_policy::may_dispatch(
            devsched_policy::Trust::from_host_opts(&host.opts_string()),
            plan.own,
        )
}

/// rsync 经的那条 ssh（`-e` 的值，rsync 自己按空白拆）。
pub fn rsync_ssh() -> String {
    format!("ssh {}", crate::dispatch::SSH_OPTS.join(" "))
}

/// 一份要送的目录。
#[derive(Debug, Clone, PartialEq)]
pub struct Send {
    pub src: PathBuf,
    /// 那台上的目录
    pub dest: String,
    /// 只读附带（data）
    pub read_only: bool,
}

/// 这一趟要送哪几份：树根 ＋ `siblings` ＋ `data`。
pub fn sends(plan: &Plan, rw: &Rewriter) -> Vec<Send> {
    let mut v = vec![Send {
        src: plan.root.clone(),
        dest: rw.remote_tree(plan),
        read_only: false,
    }];
    if let Some(p) = &plan.project {
        for d in p.list("siblings") {
            v.push(Send {
                dest: rw.out(&d),
                src: PathBuf::from(d),
                read_only: false,
            });
        }
        for d in p.list("data") {
            v.push(Send {
                dest: rw.out(&d),
                src: PathBuf::from(d),
                read_only: true,
            });
        }
    }
    v
}

/// `--rsync-path` 那一项：目的目录由远端那头的 shell 先建（远端 rsync 3.1 没有 --mkpath）。送源码与送工具共用。
pub fn rsync_path_mkdir(dest: &str) -> String {
    format!(
        "--rsync-path=mkdir -p '{}' && rsync",
        dest.replace('\'', "'\\''")
    )
}

/// rsync 的命令数组（`list` 是清单文件：NUL 分隔的相对路径；`ssh` 是 `-e` 的值）。
pub fn rsync_argv(dest_host: &str, s: &Send, list: &Path, ssh: &str) -> Vec<String> {
    let mut v: Vec<String> = ["rsync", "-lpD", "-c", "--delete-missing-args", "--from0"]
        .iter()
        .map(|x| x.to_string())
        .collect();
    let dest = s.dest.trim_end_matches('/');
    v.push(rsync_path_mkdir(dest));
    v.push(format!("--timeout={RSYNC_IO_TIMEOUT}"));
    if s.read_only {
        v.push("--chmod=Fa-w".into());
    }
    v.push(format!("--files-from={}", list.display()));
    v.push("-e".into());
    v.push(ssh.to_string());
    v.push(format!("{}/", s.src.display()));
    v.push(format!("{dest_host}:{dest}/"));
    v
}

/// 起一条命令数组（不经 shell）：返回退出码。测试里换成记下命令的假的。
pub trait Runner {
    fn run(&self, argv: &[String]) -> io::Result<i32>;
    /// 同 `run`，整趟时限换成 `wall`（往镜像首推整段历史，慢链路上要比送一份目录久）。
    fn run_within(&self, argv: &[String], _wall: std::time::Duration) -> io::Result<i32> {
        self.run(argv)
    }
    /// 起一条命令、收它的标准输出（列远端树用）：返回（退出码, 标准输出）。默认起不了。
    fn output(&self, _argv: &[String]) -> io::Result<(i32, Vec<u8>)> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "这个起法不收输出",
        ))
    }
    /// 去那台的 ssh（一串词：rsync `-e` 与 git `core.sshCommand` 原样用，起那一下 ssh 按空白拆）。
    /// 测试里换成在本机起对面那条命令的。
    fn ssh(&self) -> String {
        rsync_ssh()
    }
}

/// rsync 多久没收发一个字节就自己退（`--timeout`，秒）。
pub const RSYNC_IO_TIMEOUT: u32 = 60;
/// 送一份目录整趟最多多久：超了停掉、算那台的（冷那台）。
pub const RSYNC_WALL: std::time::Duration = std::time::Duration::from_secs(30 * 60);
/// 推进镜像整趟最多多久（首推带整段历史；推一半断了下回从头推）。
pub const GIT_PUSH_WALL: std::time::Duration = std::time::Duration::from_secs(3 * 3600);
/// 列一回远端树最多多久。
pub const LIST_WALL: std::time::Duration = std::time::Duration::from_secs(60);

/// 真起：超过 `wall` 还没退 ⇒ 杀掉、收掉，报 `TimedOut`。
pub struct RealRunner {
    pub wall: std::time::Duration,
}

impl Default for RealRunner {
    fn default() -> Self {
        RealRunner { wall: RSYNC_WALL }
    }
}

impl Runner for RealRunner {
    fn run(&self, argv: &[String]) -> io::Result<i32> {
        self.run_within(argv, self.wall)
    }

    fn output(&self, argv: &[String]) -> io::Result<(i32, Vec<u8>)> {
        use std::io::Read;
        let (p, a) = argv
            .split_first()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "空命令"))?;
        let mut child = Command::new(p)
            .args(a)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .spawn()?;
        let mut out = child.stdout.take().expect("piped");
        let reader = std::thread::spawn(move || {
            let mut b = Vec::new();
            let _ = out.read_to_end(&mut b);
            b
        });
        let code = self.wait_within(&mut child, p, LIST_WALL)?;
        Ok((code, reader.join().unwrap_or_default()))
    }

    fn run_within(&self, argv: &[String], wall: std::time::Duration) -> io::Result<i32> {
        let (p, a) = argv
            .split_first()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "空命令"))?;
        let mut child = Command::new(p).args(a).spawn()?;
        self.wait_within(&mut child, p, wall)
    }
}

impl RealRunner {
    /// 等它退；超过 `wall` 杀掉、收掉，报 `TimedOut`。
    fn wait_within(
        &self,
        child: &mut std::process::Child,
        p: &str,
        wall: std::time::Duration,
    ) -> io::Result<i32> {
        let t0 = std::time::Instant::now();
        loop {
            if let Some(s) = child.try_wait()? {
                return Ok(s.code().unwrap_or(128));
            }
            if t0.elapsed() >= wall {
                let _ = child.kill();
                let _ = child.wait();
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    format!("{} 超过 {} 秒没跑完", p, wall.as_secs()),
                ));
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    }
}

/// 照清单送。
pub struct RsyncSync<'a> {
    pub state: State,
    pub runner: &'a dyn Runner,
}

fn read_list(p: &Path) -> BTreeSet<PathBuf> {
    match std::fs::read(p) {
        Ok(b) => b
            .split(|c| *c == 0)
            .filter(|x| !x.is_empty())
            .map(|x| PathBuf::from(std::ffi::OsStr::from_bytes(x)))
            .collect(),
        Err(_) => BTreeSet::new(),
    }
}

fn write_list(p: &Path, items: &[PathBuf]) -> io::Result<()> {
    if let Some(d) = p.parent() {
        std::fs::create_dir_all(d)?;
    }
    let mut b = Vec::new();
    for i in items {
        b.extend_from_slice(i.as_os_str().as_bytes());
        b.push(0);
    }
    std::fs::write(p, b)
}

impl Syncer for RsyncSync<'_> {
    fn needs(&self) -> Option<&'static str> {
        Some("rsync")
    }

    /// `git = 1` 的项目：一下 ssh 对一遍镜像，树已不在的那几棵的工作树记录与 `refs/qc/<树>/*` 删掉（见 git.rs `SWEEP`）。
    fn sweep_mirror(&self, host: &Host, plan: &Plan, rw: &Rewriter) -> Result<(), String> {
        let Some(p) = plan.project.as_ref().filter(|p| p.get("git") == "1") else {
            return Ok(());
        };
        let Way::Ssh(dest) = &host.way else {
            return Ok(());
        };
        let (mirror, dir) = crate::git::mirror_paths(plan, rw, p).map_err(sync_why)?;
        let argv = crate::git::sweep_argv(dest, &self.runner.ssh(), &mirror, &dir);
        run_remote(self.runner, &argv, Some(LIST_WALL), "对不成镜像").map_err(sync_why)
    }

    /// 一下 ssh 列 `<根>/projects/<项目>/` 这一层的目录（零散文件不算；目录不在 ⇒ 空）。
    fn trees(&self, host: &Host, plan: &Plan, rw: &Rewriter) -> Result<Vec<String>, String> {
        let Way::Ssh(dest) = &host.way else {
            return Err("本机那一行不列".into());
        };
        let dir = crate::git::project_dir(plan, rw);
        let mut v: Vec<String> = self
            .runner
            .ssh()
            .split_whitespace()
            .map(String::from)
            .collect();
        v.push(dest.clone());
        v.push(format!(
            "if cd '{}' 2>/dev/null; then ls -1Ap; fi",
            dir.replace('\'', "'\\''")
        ));
        match self.runner.output(&v) {
            // `ls -p` 给目录名后面加 `/`：只留目录
            Ok((0, out)) => Ok(String::from_utf8_lossy(&out)
                .lines()
                .filter_map(|l| l.strip_suffix('/'))
                .filter(|l| !l.is_empty())
                .map(str::to_string)
                .collect()),
            Ok((c, _)) => Err(format!("ssh 退 {c}")),
            Err(e) => Err(e.to_string()),
        }
    }

    fn sync(&self, host: &Host, plan: &Plan, rw: &Rewriter) -> Result<(), SyncError> {
        let Way::Ssh(dest_host) = &host.way else {
            return Err(SyncError::Local("本机那一行不送源码".into()));
        };
        let ex = ExcludeList::parse(
            plan.project
                .as_ref()
                .map(|p| p.get("exclude"))
                .unwrap_or(""),
        );
        // 项目配置 git = 1：先把提交与引用推进那台的镜像（见 git.rs）
        let git = crate::git::prepare(plan, rw, &ex, &self.state.dir.join("git"))?;
        if let Some(g) = &git {
            run_remote(
                self.runner,
                &g.push_argv(dest_host, &self.runner.ssh()),
                Some(GIT_PUSH_WALL),
                "推不进镜像",
            )?;
        }
        for s in sends(plan, rw) {
            let now = send_list(&s.src, &ex)
                .map_err(|e| SyncError::Local(format!("走不了 {}：{e}", s.src.display())))?;
            let key =
                devsched_wire::digest(format!("{}\0{}", host.name, s.src.display()).as_bytes());
            let sent = self.state.dir.join("sent").join(&key);
            let prev = read_list(&sent);
            let cur: BTreeSet<PathBuf> = now.iter().cloned().collect();
            let mut items = now.clone();
            // 上一趟送过、这一趟不在清单里的：只有本机已经没了的才列（rsync 照 --delete-missing-args 在那边删掉）；
            // 本机还在、只是这一趟排除了的不列，列了就会照样送过去
            items.extend(
                prev.difference(&cur)
                    .filter(|p| std::fs::symlink_metadata(s.src.join(p)).is_err())
                    .cloned(),
            );
            let list = self
                .state
                .dir
                .join("sent")
                .join(format!("{key}.{}.list", std::process::id()));
            write_list(&list, &items).map_err(|e| SyncError::Local(format!("写不了清单：{e}")))?;
            let r = self
                .runner
                .run(&rsync_argv(dest_host, &s, &list, &self.runner.ssh()));
            let _ = std::fs::remove_file(&list);
            match r {
                Err(e) if e.kind() == io::ErrorKind::TimedOut => {
                    return Err(SyncError::Remote(format!("rsync {e}")))
                }
                Err(e) => return Err(SyncError::Local(format!("起不了 rsync：{e}"))),
                Ok(0) | Ok(24) => {}
                Ok(c @ (1 | 4)) => {
                    return Err(SyncError::Local(format!("rsync 退 {c}（本机这边的参数）")))
                }
                Ok(c) => return Err(SyncError::Remote(format!("rsync 退 {c}"))),
            }
            write_list(&sent, &now)
                .map_err(|e| SyncError::Local(format!("写不了送过的清单：{e}")))?;
        }
        // 源码到了之后：远端树的 .git 指针 · 镜像里这棵树的记录 · 索引
        if let Some(g) = &git {
            run_remote(
                self.runner,
                &g.finish_argv(dest_host, &self.runner.ssh()),
                None,
                "远端树的 git 记录没写成",
            )?;
        }
        Ok(())
    }
}

/// 送源码那一路的错，只要那一句（列树 · 对镜像只记一行，不分本机 · 那台）。
fn sync_why(e: SyncError) -> String {
    match e {
        SyncError::Local(s) | SyncError::Remote(s) => s,
    }
}

/// 起一条去那台的命令：起不来 ⇒ 本机这边；退非 0 · 超时 ⇒ 那台的。
fn run_remote(
    runner: &dyn Runner,
    argv: &[String],
    wall: Option<std::time::Duration>,
    what: &str,
) -> Result<(), SyncError> {
    let r = match wall {
        Some(w) => runner.run_within(argv, w),
        None => runner.run(argv),
    };
    match r {
        Ok(0) => Ok(()),
        Ok(c) => Err(SyncError::Remote(format!("{what}（退 {c}）"))),
        Err(e) if e.kind() == io::ErrorKind::TimedOut => {
            Err(SyncError::Remote(format!("{what}：{e}")))
        }
        Err(e) => Err(SyncError::Local(format!("{what}：起不了 {}：{e}", argv[0]))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{HostTable, Project};
    use crate::testkit::{plan_in, TempDir};
    use std::sync::Mutex;

    /// 记下命令数组与当时清单文件的内容。
    #[derive(Default)]
    struct Rec {
        calls: Mutex<Vec<(Vec<String>, Vec<PathBuf>)>>,
        code: i32,
    }

    impl Runner for Rec {
        fn run(&self, argv: &[String]) -> io::Result<i32> {
            let list = argv
                .iter()
                .find_map(|a| a.strip_prefix("--files-from="))
                .unwrap();
            let items: Vec<PathBuf> = read_list(Path::new(list)).into_iter().collect();
            self.calls.lock().unwrap().push((argv.to_vec(), items));
            Ok(self.code)
        }
    }

    fn setup(t: &TempDir) -> (Plan, crate::config::Host) {
        let root = t.path().join("w1");
        for f in [
            "src/a.rs",
            ".env",
            "api/.env",
            ".planned-build/x",
            "target/o",
            "api/target/p",
        ] {
            let p = root.join(f);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, "x").unwrap();
        }
        let mut plan = plan_in(&root, &["cargo", "test"]);
        let mut keys = std::collections::BTreeMap::new();
        keys.insert("exclude".to_string(), ".env* target".to_string());
        plan.project = Some(Project {
            name: "alpha".into(),
            keys,
        });
        (
            plan,
            HostTable::parse("a 1 ssh:a-dest base=/srv/ds\n")
                .hosts
                .remove(0),
        )
    }

    #[test]
    fn list_has_no_secrets_and_argv_has_no_exclude() {
        let t = TempDir::new("sync");
        let (plan, host) = setup(&t);
        let rw = Rewriter::new(&plan, "/srv/ds");
        let rec = Rec::default();
        let s = RsyncSync {
            state: State::new(t.path().join("state")),
            runner: &rec,
        };
        s.sync(&host, &plan, &rw).unwrap();
        let calls = rec.calls.lock().unwrap();
        let (argv, items) = &calls[0];
        assert_eq!(items, &[PathBuf::from("src/a.rs")]);
        assert!(!argv.iter().any(|a| a.contains("exclude")), "{argv:?}");
        assert_eq!(argv[0], "rsync");
        assert!(argv.contains(&"-c".to_string()) && !argv.iter().any(|a| a == "-a" || a == "-t"));
        assert_eq!(
            argv.last().unwrap(),
            &format!("a-dest:/srv/ds/projects/alpha/{}/", plan.tree)
        );
        assert_eq!(argv[argv.len() - 2], format!("{}/", plan.root.display()));
        assert!(
            !argv.iter().any(|a| a == "--mkpath"),
            "远端 rsync 3.1 没有 --mkpath"
        );
        assert!(
            argv.contains(&format!(
                "--rsync-path=mkdir -p '/srv/ds/projects/alpha/{}' && rsync",
                plan.tree
            )),
            "目的目录由远端 shell 先建：{argv:?}"
        );
    }

    #[test]
    fn files_gone_locally_are_listed_for_remote_delete() {
        let t = TempDir::new("sync2");
        let (plan, host) = setup(&t);
        std::fs::write(plan.root.join("src/b.rs"), "y").unwrap();
        let rw = Rewriter::new(&plan, "/srv/ds");
        let rec = Rec::default();
        let s = RsyncSync {
            state: State::new(t.path().join("state")),
            runner: &rec,
        };
        s.sync(&host, &plan, &rw).unwrap();
        std::fs::remove_file(plan.root.join("src/b.rs")).unwrap();
        s.sync(&host, &plan, &rw).unwrap();
        let calls = rec.calls.lock().unwrap();
        assert_eq!(
            calls[1].1,
            [PathBuf::from("src/a.rs"), PathBuf::from("src/b.rs")]
        );
        assert!(calls[1].0.contains(&"--delete-missing-args".to_string()));
        drop(calls);
        // 第三趟：已经删过了，不再列
        s.sync(&host, &plan, &rw).unwrap();
        assert_eq!(rec.calls.lock().unwrap()[2].1, [PathBuf::from("src/a.rs")]);
    }

    #[test]
    fn newly_excluded_file_still_here_is_not_sent_again() {
        let t = TempDir::new("sync5");
        let (mut plan, host) = setup(&t);
        std::fs::write(plan.root.join("src/secret.txt"), "s").unwrap();
        let rw = Rewriter::new(&plan, "/srv/ds");
        let rec = Rec::default();
        let s = RsyncSync {
            state: State::new(t.path().join("state")),
            runner: &rec,
        };
        s.sync(&host, &plan, &rw).unwrap();
        assert!(rec.calls.lock().unwrap()[0]
            .1
            .contains(&PathBuf::from("src/secret.txt")));
        // 项目配置后加一项排除：本机还在的那份不能因为「上一趟送过」又进清单
        if let Some(p) = plan.project.as_mut() {
            p.keys
                .insert("exclude".into(), ".env* target secret.txt".into());
        }
        s.sync(&host, &plan, &rw).unwrap();
        assert_eq!(rec.calls.lock().unwrap()[1].1, [PathBuf::from("src/a.rs")]);
    }

    #[test]
    fn rsync_failures_are_classified() {
        let t = TempDir::new("sync3");
        let (plan, host) = setup(&t);
        let rw = Rewriter::new(&plan, "/srv/ds");
        let rec = Rec {
            code: 255,
            ..Default::default()
        };
        let s = RsyncSync {
            state: State::new(t.path().join("state")),
            runner: &rec,
        };
        assert!(matches!(
            s.sync(&host, &plan, &rw),
            Err(SyncError::Remote(_))
        ));
        let rec = Rec {
            code: 1,
            ..Default::default()
        };
        let s = RsyncSync {
            state: State::new(t.path().join("state")),
            runner: &rec,
        };
        assert!(matches!(
            s.sync(&host, &plan, &rw),
            Err(SyncError::Local(_))
        ));
        // 树根读不了 ⇒ 本机这边
        let mut bad = plan.clone();
        bad.root = t.path().join("nope");
        let rec = Rec::default();
        let s = RsyncSync {
            state: State::new(t.path().join("state")),
            runner: &rec,
        };
        assert!(matches!(
            s.sync(&host, &bad, &Rewriter::new(&bad, "/srv/ds")),
            Err(SyncError::Local(_))
        ));
    }

    #[test]
    fn rsync_has_io_timeout_and_wall_timeout_is_remote() {
        let t = TempDir::new("sync5");
        let (plan, host) = setup(&t);
        let rw = Rewriter::new(&plan, "/srv/ds");
        let rec = Rec::default();
        let s = RsyncSync {
            state: State::new(t.path().join("state")),
            runner: &rec,
        };
        s.sync(&host, &plan, &rw).unwrap();
        assert!(rec.calls.lock().unwrap()[0]
            .0
            .contains(&format!("--timeout={RSYNC_IO_TIMEOUT}")));
        // 整趟超过时限（跑的那一个报 TimedOut）⇒ 算那台的（冷那台），不算本机准备没成
        struct Slow;
        impl Runner for Slow {
            fn run(&self, _: &[String]) -> io::Result<i32> {
                Err(io::Error::new(io::ErrorKind::TimedOut, "超时"))
            }
        }
        let s = RsyncSync {
            state: State::new(t.path().join("state")),
            runner: &Slow,
        };
        assert!(matches!(
            s.sync(&host, &plan, &rw),
            Err(SyncError::Remote(_))
        ));
    }

    #[test]
    fn trees_lists_the_project_dir_on_that_host_in_one_ssh() {
        let t = TempDir::new("sync-trees");
        let (plan, host) = setup(&t);
        struct Ls(Mutex<Vec<String>>);
        impl Runner for Ls {
            fn run(&self, _: &[String]) -> io::Result<i32> {
                Ok(0)
            }
            fn output(&self, argv: &[String]) -> io::Result<(i32, Vec<u8>)> {
                *self.0.lock().unwrap() = argv.to_vec();
                Ok((
                    0,
                    b"_mirror.git/\n_mirror.git.lock\nab12cd34/\n\nstray.txt\nef56ab78/\n".to_vec(),
                ))
            }
        }
        let ls = Ls(Mutex::new(vec![]));
        let s = RsyncSync {
            state: State::new(t.path().join("state")),
            runner: &ls,
        };
        let rw = Rewriter::new(&plan, "/srv/ds");
        assert_eq!(
            s.trees(&host, &plan, &rw).unwrap(),
            ["_mirror.git", "ab12cd34", "ef56ab78"],
            "只留目录（锁 · 零散文件不算）"
        );
        let argv = ls.0.lock().unwrap().clone();
        assert_eq!(argv[0], "ssh");
        assert_eq!(
            argv.last().unwrap(),
            "if cd '/srv/ds/projects/alpha' 2>/dev/null; then ls -1Ap; fi"
        );
    }

    #[test]
    fn real_runner_output_collects_stdout() {
        let r = RealRunner::default();
        let (c, out) = r
            .output(&["sh".into(), "-c".into(), "echo a; echo b; exit 3".into()])
            .unwrap();
        assert_eq!((c, out.as_slice()), (3, b"a\nb\n".as_slice()));
    }

    #[test]
    fn real_runner_kills_and_reaps_past_wall_limit() {
        let r = RealRunner {
            wall: std::time::Duration::from_millis(300),
        };
        let e = r.run(&["sleep".into(), "30".into()]).unwrap_err();
        assert_eq!(e.kind(), io::ErrorKind::TimedOut);
        assert_eq!(
            RealRunner {
                wall: std::time::Duration::from_secs(5)
            }
            .run(&["true".into()])
            .unwrap(),
            0
        );
    }

    #[test]
    fn siblings_and_data_go_like_the_tree() {
        let t = TempDir::new("sync4");
        let (mut plan, host) = setup(&t);
        let sib = t.path().join("sib");
        let data = t.path().join("corpus");
        std::fs::create_dir_all(&sib).unwrap();
        std::fs::create_dir_all(&data).unwrap();
        std::fs::write(sib.join("s.rs"), "s").unwrap();
        std::fs::write(data.join("d.txt"), "d").unwrap();
        let p = plan.project.as_mut().unwrap();
        p.keys.insert("siblings".into(), sib.display().to_string());
        p.keys.insert("data".into(), data.display().to_string());
        let rw = Rewriter::new(&plan, "/srv/ds");
        let rec = Rec::default();
        let s = RsyncSync {
            state: State::new(t.path().join("state")),
            runner: &rec,
        };
        s.sync(&host, &plan, &rw).unwrap();
        let calls = rec.calls.lock().unwrap();
        assert_eq!(calls.len(), 3);
        assert_eq!(
            calls[1].0.last().unwrap(),
            &format!(
                "a-dest:/srv/ds/projects/alpha/{}/",
                crate::plan::tree_name(&sib)
            )
        );
        assert!(calls[2].0.contains(&"--chmod=Fa-w".to_string()));
        assert!(!calls[1].0.contains(&"--chmod=Fa-w".to_string()));
        // 命令里提到兄弟仓的路径照样改写
        assert_eq!(
            rw.out(&format!("{}/s.rs", sib.display())),
            format!(
                "/srv/ds/projects/alpha/{}/s.rs",
                crate::plan::tree_name(&sib)
            )
        );
    }
}
