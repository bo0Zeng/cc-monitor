//! 队与锁：各次 `devsched` 调用靠状态目录里的文件互相看见。
//!
//! ```text
//! <状态目录>/q/<纳秒时刻>.<pid>   一趟一个号；内容一行一项：prio · local · needs <能力…>
//! <状态目录>/slots/<台>.<序号>    位锁（flock）
//! <状态目录>/target/<摘要>        产物目录锁（flock）
//! <状态目录>/cool/<台>            冷到何时（unix 秒）
//! <状态目录>/caps/<台>            那台上次 hello 报的能力，一行一个
//! <状态目录>/used/<台>/<项目>/<树> 那棵远端树上回经本机用过的时刻（修改时刻）
//! <状态目录>/swept/<台>/<项目>    那台那个项目的镜像上回对过的时刻（修改时刻）
//! <状态目录>/log/                 devsched.log 一趟一行；<号>.<台>.raw 远端输出原文
//! ```

use crate::config::Host;
use crate::plan::{Plan, Vars};
use std::collections::BTreeSet;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::os::unix::io::AsRawFd;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// 记下的能力多久作废（`State::caps`）。
pub const CAPS_TTL: Duration = Duration::from_secs(10 * 60);

/// 状态目录：`DEVSCHED_STATE` ＞ `~/.local/state/devsched/`。
pub fn state_dir(vars: &Vars) -> Option<PathBuf> {
    if let Some(p) = vars.get("DEVSCHED_STATE").filter(|p| !p.is_empty()) {
        return Some(PathBuf::from(p));
    }
    vars.get("HOME")
        .filter(|h| !h.is_empty())
        .map(|h| Path::new(h).join(".local/state/devsched"))
}

/// 一个号写的那几项。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TicketInfo {
    /// 文件名（`<纳秒时刻>.<pid>`，等宽 ⇒ 按名排就是按先后）
    pub name: String,
    pub pid: u32,
    pub prio: bool,
    pub local: bool,
    pub needs: BTreeSet<String>,
}

impl TicketInfo {
    pub fn for_plan(name: String, pid: u32, plan: &Plan) -> TicketInfo {
        TicketInfo {
            name,
            pid,
            prio: plan.prio,
            local: plan.local.is_some(),
            needs: plan.needs.clone(),
        }
    }

    fn render(&self) -> String {
        let mut s = String::new();
        if self.prio {
            s.push_str("prio\n");
        }
        if self.local {
            s.push_str("local\n");
        }
        s.push_str("needs");
        for n in &self.needs {
            s.push(' ');
            s.push_str(n);
        }
        s.push('\n');
        s
    }

    fn parse(name: &str, text: &str) -> Option<TicketInfo> {
        let pid = name.rsplit_once('.')?.1.parse().ok()?;
        let mut t = TicketInfo {
            name: name.to_string(),
            pid,
            prio: false,
            local: false,
            needs: BTreeSet::new(),
        };
        for l in text.lines() {
            match l.trim() {
                "prio" => t.prio = true,
                "local" => t.local = true,
                x if x.starts_with("needs") => {
                    t.needs
                        .extend(x["needs".len()..].split_whitespace().map(|s| s.to_string()));
                }
                _ => {}
            }
        }
        Some(t)
    }

    /// `self` 排在 `other` 前面。
    pub fn ahead_of(&self, other: &TicketInfo) -> bool {
        (self.prio && !other.prio) || (self.prio == other.prio && self.name < other.name)
    }
}

/// 一台此刻的样子：挑台与挡不挡都按它判。
pub struct HostView<'a> {
    pub host: &'a Host,
    /// 上次 hello 报的能力；没见过 ⇒ None
    pub caps: Option<BTreeSet<String>>,
    /// 本机那一行：能出本机的趟此刻许不许用本机的位（fallback 没到时不许）
    pub local_open_to_remote_ok: bool,
}

impl HostView<'_> {
    /// 这台接得了这个号吗。能力没见过的台：不要能力的号接得了，要能力的号当接不了（不挡别人）。
    pub fn accepts(&self, t: &TicketInfo) -> bool {
        if self.host.is_local() {
            return t.local || self.local_open_to_remote_ok;
        }
        if t.local {
            return false;
        }
        match &self.caps {
            Some(c) => t.needs.is_subset(c),
            None => t.needs.is_empty(),
        }
    }
}

/// 拿着的一把 flock 锁；放下 ＝ drop。
#[derive(Debug)]
pub struct Lock {
    _f: File,
    pub path: PathBuf,
}

fn try_flock(path: &Path) -> io::Result<Option<Lock>> {
    if let Some(d) = path.parent() {
        fs::create_dir_all(d)?;
    }
    let f = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(path)?;
    // SAFETY: flock 只读写这个 fd 上的锁
    let r = unsafe { libc::flock(f.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
    if r == 0 {
        Ok(Some(Lock {
            _f: f,
            path: path.to_path_buf(),
        }))
    } else {
        let e = io::Error::last_os_error();
        if e.raw_os_error() == Some(libc::EWOULDBLOCK) {
            Ok(None)
        } else {
            Err(e)
        }
    }
}

pub fn pid_alive(pid: u32) -> bool {
    if pid == 0 || pid > i32::MAX as u32 {
        return false;
    }
    // SAFETY: 信号 0 只查在不在
    let r = unsafe { libc::kill(pid as i32, 0) };
    r == 0 || io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// 状态目录。
#[derive(Debug, Clone)]
pub struct State {
    pub dir: PathBuf,
}

/// 一趟的号；排着的时候文件在，派出去时删掉、回队里时照原名再写（先后不丢）。
#[derive(Debug)]
pub struct Ticket {
    pub info: TicketInfo,
    path: PathBuf,
}

impl Ticket {
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// 派出去了：号文件删掉（不再挡别人）。
    pub fn withdraw(&self) {
        let _ = fs::remove_file(&self.path);
    }

    /// 回队里：照原名再写一份（计划可能变了，例如改成只本机）。
    pub fn requeue(&mut self, plan: &Plan) -> io::Result<()> {
        self.info = TicketInfo::for_plan(self.info.name.clone(), self.info.pid, plan);
        fs::write(&self.path, self.info.render())
    }
}

impl Drop for Ticket {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

impl State {
    pub fn new(dir: PathBuf) -> State {
        State { dir }
    }

    fn q(&self) -> PathBuf {
        self.dir.join("q")
    }

    /// 取一个号。
    pub fn take_ticket(&self, plan: &Plan) -> io::Result<Ticket> {
        fs::create_dir_all(self.q())?;
        let pid = std::process::id();
        loop {
            let ns = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0);
            let name = format!("{ns:020}.{pid}");
            let path = self.q().join(&name);
            match OpenOptions::new().write(true).create_new(true).open(&path) {
                Ok(mut f) => {
                    let info = TicketInfo::for_plan(name, pid, plan);
                    f.write_all(info.render().as_bytes())?;
                    return Ok(Ticket { info, path });
                }
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(e),
            }
        }
    }

    /// 队里的号（死 pid 的当场清掉）。
    pub fn tickets(&self) -> Vec<TicketInfo> {
        let mut out = Vec::new();
        let Ok(rd) = fs::read_dir(self.q()) else {
            return out;
        };
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            let Ok(text) = fs::read_to_string(e.path()) else {
                continue;
            };
            let Some(t) = TicketInfo::parse(&name, &text) else {
                continue;
            };
            if !pid_alive(t.pid) {
                let _ = fs::remove_file(e.path());
                continue;
            }
            out.push(t);
        }
        out.sort_by(|a, b| a.name.cmp(&b.name));
        out
    }

    /// 排在 `me` 前面、还活着、也接得了这台的号有没有。
    pub fn blocked(&self, me: &TicketInfo, view: &HostView) -> bool {
        self.tickets()
            .iter()
            .any(|t| t.name != me.name && t.ahead_of(me) && view.accepts(t))
    }

    pub fn try_slot(&self, host: &str, n: u32) -> io::Result<Option<Lock>> {
        try_flock(&self.dir.join("slots").join(format!("{host}.{n}")))
    }

    /// 同一台同一时刻只一个 `--prepare`（`tools/<台>.lock`）。
    pub fn try_prepare(&self, host: &str) -> io::Result<Option<Lock>> {
        try_flock(&self.dir.join("tools").join(format!("{host}.lock")))
    }

    pub fn try_target(&self, target: &Path) -> io::Result<Option<Lock>> {
        let key = devsched_wire::digest(target.to_string_lossy().as_bytes());
        try_flock(&self.dir.join("target").join(key))
    }

    pub fn cooling(&self, host: &str) -> bool {
        fs::read_to_string(self.dir.join("cool").join(host))
            .ok()
            .and_then(|s| s.trim().parse::<u64>().ok())
            .is_some_and(|until| until > now_secs())
    }

    pub fn cool(&self, host: &str, secs: u64) {
        let d = self.dir.join("cool");
        let _ = fs::create_dir_all(&d);
        let _ = fs::write(d.join(host), format!("{}\n", now_secs() + secs));
    }

    /// 那台上次 hello 报的能力；过了 [`CAPS_TTL`] 作废（返回 None ⇒ 照旧连一次重新收）：
    /// 节点升级 · 补装了工具，那台不能因为一份旧记录永远不连。
    pub fn caps(&self, host: &str) -> Option<BTreeSet<String>> {
        let p = self.dir.join("caps").join(host);
        let age = fs::metadata(&p)
            .ok()?
            .modified()
            .ok()?
            .elapsed()
            .unwrap_or_default();
        if age > CAPS_TTL {
            return None;
        }
        let s = fs::read_to_string(&p).ok()?;
        Some(
            s.lines()
                .map(|l| l.trim().to_string())
                .filter(|l| !l.is_empty())
                .collect(),
        )
    }

    pub fn set_caps(&self, host: &str, caps: &BTreeSet<String>) {
        let d = self.dir.join("caps");
        let _ = fs::create_dir_all(&d);
        let body: String = caps.iter().map(|c| format!("{c}\n")).collect();
        let _ = fs::write(d.join(host), body);
    }

    /// 记下这台上这个项目的这棵远端树刚用过（`used/<台>/<项目>/<树>` 的修改时刻）。
    pub fn touch_used(&self, host: &str, project: &str, tree: &str) {
        let d = self.dir.join("used").join(host).join(project);
        let _ = fs::create_dir_all(&d);
        let _ = fs::write(d.join(tree), b"");
    }

    /// 这棵远端树上回经本机用过是什么时候；没记过 ⇒ `None`（按最久没用算）。
    pub fn used_at(&self, host: &str, project: &str, tree: &str) -> Option<SystemTime> {
        fs::metadata(self.dir.join("used").join(host).join(project).join(tree))
            .and_then(|m| m.modified())
            .ok()
    }

    /// 这台这个项目的镜像上回对过之后过了 `every` 没有（没对过 ⇒ 过了）。
    pub fn sweep_due(&self, host: &str, project: &str, every: Duration) -> bool {
        fs::metadata(self.dir.join("swept").join(host).join(project))
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.elapsed().ok())
            .is_none_or(|e| e >= every)
    }

    /// 记下这台这个项目的镜像刚对过（`swept/<台>/<项目>` 的修改时刻）。
    pub fn mark_swept(&self, host: &str, project: &str) {
        let d = self.dir.join("swept").join(host);
        let _ = fs::create_dir_all(&d);
        let _ = fs::write(d.join(project), b"");
    }

    pub fn log_dir(&self) -> PathBuf {
        self.dir.join("log")
    }

    /// `log/` 下远端输出原文（`*.raw`）总量过了 `cap` ⇒ 从最旧的删到不超；`keep`（这一趟正写的）不删。
    pub fn prune_raw(&self, keep: &Path, cap: u64) {
        let Ok(rd) = fs::read_dir(self.log_dir()) else {
            return;
        };
        let mut v: Vec<(SystemTime, u64, PathBuf)> = rd
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().ends_with(".raw") && e.path() != keep)
            .filter_map(|e| {
                let m = e.metadata().ok()?;
                Some((m.modified().unwrap_or(UNIX_EPOCH), m.len(), e.path()))
            })
            .collect();
        let mut total: u64 =
            v.iter().map(|x| x.1).sum::<u64>() + fs::metadata(keep).map(|m| m.len()).unwrap_or(0);
        v.sort();
        for (_, len, p) in v {
            if total <= cap {
                break;
            }
            if fs::remove_file(&p).is_ok() {
                total -= len;
            }
        }
    }

    /// 日志一行：`时刻 台 项目 树 说明`。
    pub fn log(&self, host: &str, project: &str, tree: &str, what: &str) {
        let d = self.log_dir();
        let _ = fs::create_dir_all(&d);
        if let Ok(mut f) = OpenOptions::new()
            .create(true)
            .append(true)
            .open(d.join("devsched.log"))
        {
            let _ = writeln!(f, "{}\t{host}\t{project}\t{tree}\t{what}", now_secs());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Host, Way};
    use crate::testkit::TempDir;

    fn host(name: &str, local: bool) -> Host {
        Host {
            name: name.into(),
            slots: 1,
            way: if local {
                Way::Local
            } else {
                Way::Ssh(name.into())
            },
            opts: vec![],
        }
    }

    fn write_ticket(st: &State, name: &str, body: &str) {
        fs::create_dir_all(st.dir.join("q")).unwrap();
        fs::write(st.dir.join("q").join(name), body).unwrap();
    }

    fn me() -> TicketInfo {
        // 比手写的号都晚
        TicketInfo {
            name: format!("99999999999999999999.{}", std::process::id()),
            pid: std::process::id(),
            prio: false,
            local: false,
            needs: BTreeSet::new(),
        }
    }

    fn caps(xs: &[&str]) -> Option<BTreeSet<String>> {
        Some(xs.iter().map(|s| s.to_string()).collect())
    }

    #[test]
    fn ahead_ticket_only_a_can_take_does_not_block_b() {
        let t = TempDir::new("queue");
        let st = State::new(t.path().to_path_buf());
        let pid = std::process::id();
        write_ticket(&st, &format!("00000000000000000001.{pid}"), "needs gui\n");
        let a = host("a", false);
        let b = host("b", false);
        let me = me();
        assert!(st.blocked(
            &me,
            &HostView {
                host: &a,
                caps: caps(&["gui"]),
                local_open_to_remote_ok: false
            }
        ));
        assert!(!st.blocked(
            &me,
            &HostView {
                host: &b,
                caps: caps(&["tmux"]),
                local_open_to_remote_ok: false
            }
        ));
        // 没见过能力的台：要能力的号不挡
        assert!(!st.blocked(
            &me,
            &HostView {
                host: &b,
                caps: None,
                local_open_to_remote_ok: false
            }
        ));
    }

    #[test]
    fn ahead_ticket_both_can_take_blocks() {
        let t = TempDir::new("queue2");
        let st = State::new(t.path().to_path_buf());
        let pid = std::process::id();
        write_ticket(&st, &format!("00000000000000000001.{pid}"), "needs\n");
        let b = host("b", false);
        let me = me();
        assert!(st.blocked(
            &me,
            &HostView {
                host: &b,
                caps: None,
                local_open_to_remote_ok: false
            }
        ));
        // 只本机的号不挡远端，挡本机
        fs::write(
            st.dir.join("q").join(format!("00000000000000000001.{pid}")),
            "local\nneeds\n",
        )
        .unwrap();
        assert!(!st.blocked(
            &me,
            &HostView {
                host: &b,
                caps: None,
                local_open_to_remote_ok: false
            }
        ));
        let l = host("l", true);
        assert!(st.blocked(
            &me,
            &HostView {
                host: &l,
                caps: None,
                local_open_to_remote_ok: false
            }
        ));
    }

    #[test]
    fn prio_goes_first_and_dead_pids_are_cleared() {
        let t = TempDir::new("queue3");
        let st = State::new(t.path().to_path_buf());
        let pid = std::process::id();
        // 晚来的插队号挡早来的普通号
        write_ticket(&st, &format!("99999999999999999999.{pid}"), "prio\nneeds\n");
        let mut early = me();
        early.name = format!("00000000000000000005.{pid}");
        let b = host("b", false);
        assert!(st.blocked(
            &early,
            &HostView {
                host: &b,
                caps: None,
                local_open_to_remote_ok: false
            }
        ));
        // 死 pid 的号被清掉
        let dead = 0x7fff_fff0u32;
        write_ticket(&st, &format!("00000000000000000001.{dead}"), "needs\n");
        let names: Vec<String> = st.tickets().into_iter().map(|t| t.name).collect();
        assert_eq!(names, [format!("99999999999999999999.{pid}")]);
        assert!(!st
            .dir
            .join("q")
            .join(format!("00000000000000000001.{dead}"))
            .exists());
    }

    /// 位锁 · 产物目录锁：拿到的再拿拿不到、别的号拿得到、放了再拿拿得到。
    /// 在单跑它的子进程里判（[`crate::testkit::alone`]：同一进程里别的测试 fork 的子进程会攥着放了的锁）。
    #[test]
    fn slot_and_target_locks_exclude_each_other() {
        if !crate::testkit::alone("queue::tests::slot_and_target_locks_exclude_each_other") {
            return;
        }
        let t = TempDir::new("queue4");
        let st = State::new(t.path().to_path_buf());
        let l1 = st.try_slot("a", 1).unwrap();
        assert!(l1.is_some());
        assert!(st.try_slot("a", 1).unwrap().is_none());
        assert!(st.try_slot("a", 2).unwrap().is_some());
        drop(l1);
        assert!(st.try_slot("a", 1).unwrap().is_some(), "放了的位再拿拿得到");
        let tl = st.try_target(Path::new("/x/target")).unwrap();
        assert!(tl.is_some());
        assert!(st.try_target(Path::new("/x/target")).unwrap().is_none());
        assert!(st.try_target(Path::new("/y/target")).unwrap().is_some());
    }

    /// 复现：同一进程里别的线程不停起子进程（像同一份测试二进制里并行的 git · ssh 测试）时，锁那一条照样成立。
    /// （那一条要是在这个进程里直接判，这里必红：放了的位被 fork 出的子进程攥着。）
    #[test]
    fn lock_sequence_holds_while_the_process_keeps_forking() {
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let forkers: Vec<_> = (0..8)
            .map(|_| {
                let stop = stop.clone();
                std::thread::spawn(move || {
                    while !stop.load(std::sync::atomic::Ordering::Relaxed) {
                        let _ = std::process::Command::new("true").status();
                    }
                })
            })
            .collect();
        let r = std::panic::catch_unwind(|| {
            for _ in 0..20 {
                crate::testkit::run_alone("queue::tests::slot_and_target_locks_exclude_each_other");
            }
        });
        stop.store(true, std::sync::atomic::Ordering::Relaxed);
        for f in forkers {
            let _ = f.join();
        }
        assert!(r.is_ok());
    }

    #[test]
    fn ticket_file_life() {
        let t = TempDir::new("queue5");
        let st = State::new(t.path().to_path_buf());
        let (plan, _) =
            crate::plan::make_plan(&[], t.path(), &["true".to_string()], &Vars::new(), false);
        let mut tk = st.take_ticket(&plan).unwrap();
        assert!(tk.path().exists());
        assert!(fs::read_to_string(tk.path()).unwrap().contains("local"));
        tk.withdraw();
        assert!(!tk.path().exists());
        tk.requeue(&plan).unwrap();
        assert!(tk.path().exists());
        let p = tk.path().to_path_buf();
        drop(tk);
        assert!(!p.exists());
        st.cool("a", 60);
        assert!(st.cooling("a"));
        assert!(!st.cooling("b"));
        st.set_caps("a", &["gui".to_string()].into_iter().collect());
        assert_eq!(st.caps("a"), caps(&["gui"]));
    }
}
