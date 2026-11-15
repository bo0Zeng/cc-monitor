//! 派活：一圈一圈挑台、拿位、连上、过门槛、交作业、收输出、收尾。
//!
//! 失败三类：本机准备没成（不冷那台，这一趟改只本机）· 远端连接不成 / 半路断 / 心跳断（冷那台 5 分钟、回队里）·
//! 远端跑红（命令自己的退出码）。节点报的停因：撞停线 · 线程上限 ⇒ 回队里、冷 60 秒；起不了进程 ⇒ 回队里、冷 5 分钟；
//! 临时目录路径超长 ⇒ 这一趟不再派那台、回队里、不冷。

use crate::config::{Config, Host, Way};
use crate::env::{carried, check_programs};
use crate::gate::{gate, local_facts, stop_lines};
use crate::plan::Plan;
use crate::queue::{HostView, State, Ticket};
use crate::rewrite::{rel_for_job, Pending, Rewriter};
use devsched_wire::conn::{ChildConn, Conn, Recv};
use devsched_wire::{Frame, Hello, Job, StopReason, Stream};
use std::collections::BTreeSet;
use std::io::{self, Write};
use std::time::{Duration, Instant};

/// 调度自己的退出码：本机准备不成。
pub const EXIT_LOCAL_PREP: i32 = 78;
/// 调度自己的退出码：命令退 0 而有跳过的段。
pub const EXIT_SKIPPED: i32 = 79;
/// 调度自己的退出码：超时（排队过了时限，或整趟跑过了 `timeout_s`）。
pub const EXIT_TIMEOUT: i32 = 124;
/// 整趟时限的默认（秒）：项目配置 `timeout_s` ＞ 主机表那一行 `timeout_s=` ＞ 这个。
pub const TRIP_LIMIT_DEFAULT: u64 = 7200;
/// 每台每项目留几棵远端树的默认（主机表 `trees` 没写时）。
pub const DEFAULT_TREES: i64 = 10;
/// 一趟最多删几棵远端树（每删一棵等节点回一次心跳，一趟拖不长）。
pub const MAX_CLEAN_PER_TRIP: usize = 4;
/// 一台一个项目至少多久对一遍镜像（这一趟删了树的当场对）。
pub const SWEEP_EVERY: Duration = Duration::from_secs(24 * 3600);

/// 这一趟最多跑多久。
pub fn trip_limit(h: &Host, plan: &Plan) -> Duration {
    let from_project = plan
        .project
        .as_ref()
        .and_then(|p| p.get("timeout_s").trim().parse::<u64>().ok());
    let s = from_project
        .or_else(|| h.num("timeout_s").map(|n| n.max(0) as u64))
        .unwrap_or(TRIP_LIMIT_DEFAULT);
    Duration::from_secs(s)
}

/// 冷多久（秒）。
pub const COOL_DOWN: u64 = 300;
pub const COOL_BREACH: u64 = 60;
pub const COOL_SPAWN: u64 = 300;
/// 状态目录 `log/` 下远端输出原文（`<号>.<台>.raw`）的总量上限：每趟写完从最旧的删到这以内。
pub const RAW_LOG_CAP: u64 = 50 << 20;
pub const COOL_GATE: u64 = 30;

/// 怎么连上一台。
pub trait Connector {
    fn open(&self, host: &Host) -> io::Result<Box<dyn Conn>>;
}

/// 真的：`ssh <去法> <根>/state/devsched-node --root <根>`（`<根>` ＝ 主机表 `base=`）。
pub struct SshConnector;

/// ssh 那条命令数组。
pub fn ssh_argv(host: &Host) -> io::Result<Vec<String>> {
    let Way::Ssh(dest) = &host.way else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "本机那一行不走 ssh",
        ));
    };
    let base = host
        .base()
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
    let q = |s: &str| format!("'{}'", s.replace('\'', "'\\''"));
    let mut v = vec!["ssh".to_string()];
    v.extend(SSH_OPTS.iter().map(|s| s.to_string()));
    v.push(dest.clone());
    let mut cmd = format!(
        "{} --root {}",
        q(&format!("{base}/state/devsched-node")),
        q(base)
    );
    // 主机表 `slurm=<srun 参数，逗号分隔>`：节点自己经 srun 起在分到的那一份里
    if let Some(s) = host.opt("slurm").filter(|s| !s.is_empty()) {
        cmd.push_str(&format!(" --slurm {}", q(s)));
    }
    v.push(cmd);
    Ok(v)
}

/// 连编译节点与 rsync 共用的 ssh 选项。
pub const SSH_OPTS: &[&str] = &[
    "-o",
    "BatchMode=yes",
    "-o",
    "ConnectTimeout=10",
    "-o",
    "ServerAliveInterval=15",
    "-o",
    "ServerAliveCountMax=4",
];

impl Connector for SshConnector {
    fn open(&self, host: &Host) -> io::Result<Box<dyn Conn>> {
        Ok(Box::new(ChildConn::spawn(&ssh_argv(host)?)?))
    }
}

/// 送源码没成：本机这边 · 远端那边。
#[derive(Debug, Clone, PartialEq)]
pub enum SyncError {
    Local(String),
    Remote(String),
}

pub trait Syncer {
    fn sync(&self, host: &Host, plan: &Plan, rw: &Rewriter) -> Result<(), SyncError>;
    /// 送源码要那台报的一项能力（真的 rsync 那一路要 `rsync`：节点只在版本够时报）。
    fn needs(&self) -> Option<&'static str> {
        None
    }
    /// 那台 `<根>/projects/<项目>/` 下有哪几项（名字原样，调度端自己挑哪些算树）；列不了 ⇒ `Err(为什么)`。
    /// 默认列出来是空的（不删）。
    fn trees(&self, _host: &Host, _plan: &Plan, _rw: &Rewriter) -> Result<Vec<String>, String> {
        Ok(Vec::new())
    }
    /// 清掉送源码那一路给已不在的树留在那台的别的东西（`git = 1` 的镜像里那几棵的记录）。
    fn sweep_mirror(&self, _host: &Host, _plan: &Plan, _rw: &Rewriter) -> Result<(), String> {
        Ok(())
    }
}

/// 本机那一支跑完的结果。
#[derive(Debug, Clone, PartialEq)]
pub enum LocalOutcome {
    Exit(i32),
    /// 撞了本机的停线（为什么）
    Breach(String),
    /// 起不来（为什么）
    CannotStart(String),
    /// 跑过了整趟时限（秒），已整组停掉
    Timeout(u64),
}

pub trait LocalRun {
    fn run(&self, host: &Host, plan: &Plan, slot: u32) -> LocalOutcome;
}

/// 各种时限（测试里调小）。
#[derive(Debug, Clone)]
pub struct Timing {
    /// 一圈之后歇多久
    pub poll: Duration,
    /// 心跳间隔
    pub beat: Duration,
    /// 连上之后等 hello 与资源事实多久
    pub hello: Duration,
    /// 排队最多等多久（None ＝ 一直等）
    pub deadline: Option<Duration>,
}

impl Default for Timing {
    fn default() -> Self {
        Timing {
            poll: Duration::from_secs(5),
            beat: Duration::from_secs(5),
            hello: Duration::from_secs(30),
            deadline: None,
        }
    }
}

enum Trip {
    Finished(i32),
    Requeue,
    LocalPrepFailed(String),
}

/// 调度端一趟。
pub struct Sched<'a> {
    pub cfg: Config,
    pub state: State,
    pub connector: &'a dyn Connector,
    pub syncer: &'a dyn Syncer,
    pub local: &'a dyn LocalRun,
    pub out: &'a mut dyn Write,
    pub err: &'a mut dyn Write,
    pub timing: Timing,
}

/// 调度自己收尾的那一行的头（只有收尾才印它）。
pub const TAIL: &str = "[devsched]";
/// 途中的说明的头（跟收尾那一行分开：命令自己没往标准错误写东西时，末行也认不错）。
pub const NOTE: &str = "[devsched 途中]";

impl Sched<'_> {
    /// 调度自己收尾：标准错误末尾一行「[devsched] <类>：…」。
    fn say(&mut self, s: &str) {
        let _ = writeln!(self.err, "{TAIL} {s}");
        let _ = self.err.flush();
    }

    /// 途中说一句。
    fn note(&mut self, s: &str) {
        let _ = writeln!(self.err, "{NOTE} {s}");
        let _ = self.err.flush();
    }

    /// 这一趟派得了这台吗（信任 · 能力 · 冷着 · 这一趟排除了），挑台与算 fallback 都用这一份。
    fn remote_candidate(&self, h: &Host, plan: &Plan, excluded: &BTreeSet<String>) -> bool {
        !h.is_local()
            && h.slots > 0
            && plan.local.is_none()
            && crate::sync::may_send(h, plan)
            && !self.state.cooling(&h.name)
            && !excluded.contains(&h.name)
            && self
                .state
                .caps(&h.name)
                .is_none_or(|c| self.trip_needs(plan).is_subset(&c))
    }

    /// 这一趟要那台报的能力：计划里声明的，加送源码那一路要的（rsync 版本够才报 `rsync`）。
    fn trip_needs(&self, plan: &Plan) -> BTreeSet<String> {
        let mut n = plan.needs.clone();
        n.extend(self.syncer.needs().map(String::from));
        n
    }

    fn log(&self, host: &str, plan: &Plan, what: &str) {
        let p = plan
            .project
            .as_ref()
            .map(|p| p.name.as_str())
            .unwrap_or("?");
        self.state.log(
            host,
            p,
            &crate::plan::tree_label(&plan.root, &plan.tree),
            what,
        );
    }

    /// 排队、派、跑完；返回进程退出码。
    pub fn run(&mut self, mut plan: Plan) -> i32 {
        let mut ticket = match self.state.take_ticket(&plan) {
            Ok(t) => t,
            Err(e) => {
                self.say(&format!(
                    "本机准备不成：写不了队（{}）：{e}",
                    self.state.dir.display()
                ));
                return EXIT_LOCAL_PREP;
            }
        };
        let t_enter = Instant::now();
        let mut excluded: BTreeSet<String> = BTreeSet::new();
        let mut last_remote_ok = Instant::now();
        loop {
            self.cfg.reload_hosts();
            let hosts = self.cfg.hosts.hosts.clone();
            if plan.local.is_some() && !hosts.iter().any(|h| h.is_local() && h.slots > 0) {
                self.say(&format!(
                    "本机准备不成：这一趟只本机（{}），主机表里没有位数不为 0 的本机那一行",
                    plan.local.as_deref().unwrap_or("")
                ));
                return EXIT_LOCAL_PREP;
            }
            // fallback：远端有没有一台此刻接得了这趟
            let any_remote = hosts
                .iter()
                .any(|h| self.remote_candidate(h, &plan, &excluded));
            if any_remote {
                last_remote_ok = Instant::now();
            }
            // 这一趟排除掉的（根太长）之外再没远端接得了 ⇒ fallback 当场放开，不等那几分钟
            let remotes_ruled_out = !excluded.is_empty() && !any_remote;
            'hosts: for h in &hosts {
                if h.slots == 0 {
                    continue;
                }
                if h.is_local() {
                    let fallback_open = remotes_ruled_out
                        || match h.num("fallback") {
                            Some(m) => {
                                last_remote_ok.elapsed()
                                    >= Duration::from_secs(m.max(0) as u64 * 60)
                            }
                            None => true,
                        };
                    if (plan.local.is_none() && !fallback_open) || self.state.cooling(&h.name) {
                        continue;
                    }
                    let view = HostView {
                        host: h,
                        caps: None,
                        local_open_to_remote_ok: fallback_open,
                    };
                    if self.state.blocked(&ticket.info, &view) {
                        continue;
                    }
                    for j in 1..=h.slots {
                        if plan.heavy && j > 1 {
                            break;
                        }
                        if gate(h, j, &local_facts(&plan.root)).is_err() {
                            continue;
                        }
                        let Ok(Some(_slot)) = self.state.try_slot(&h.name, j) else {
                            continue;
                        };
                        let _target = match self.target_lock(&plan) {
                            Ok(Some(l)) => l,
                            Ok(None) => break 'hosts,
                            Err(code) => return code,
                        };
                        ticket.withdraw();
                        let t0 = Instant::now();
                        self.note(&format!("→ {}（本机）", h.name));
                        match self.local.run(h, &plan, j) {
                            LocalOutcome::Exit(c) => {
                                self.log(
                                    &h.name,
                                    &plan,
                                    &format!(
                                        "本机 · 退 {c} · 等 {}s · 跑 {}s{}",
                                        (t0 - t_enter).as_secs(),
                                        t0.elapsed().as_secs(),
                                        plan.local
                                            .as_deref()
                                            .map(|w| format!(" · 只本机：{w}"))
                                            .unwrap_or_default()
                                    ),
                                );
                                return c;
                            }
                            LocalOutcome::Breach(why) => {
                                self.log(&h.name, &plan, &format!("本机撞停线 · 回队里：{why}"));
                                self.note(&format!(
                                    "本机这一趟撞停线（{why}）：回队里重排，上面这一截输出作废"
                                ));
                                self.state.cool(&h.name, COOL_BREACH);
                                let _ = ticket.requeue(&plan);
                                break 'hosts;
                            }
                            LocalOutcome::CannotStart(why) => {
                                self.log(&h.name, &plan, &format!("本机起不来：{why}"));
                                self.say(&format!("本机准备不成：{why}"));
                                return EXIT_LOCAL_PREP;
                            }
                            LocalOutcome::Timeout(s) => {
                                self.log(&h.name, &plan, &format!("本机 · 超时（整趟时限 {s}s）· 停了作业 · 退 {EXIT_TIMEOUT}"));
                                self.say(&format!("超时：本机这一趟跑过了整趟时限 {s} 秒（timeout_s），作业已整组停掉"));
                                return EXIT_TIMEOUT;
                            }
                        }
                    }
                } else {
                    if !self.remote_candidate(h, &plan, &excluded) {
                        continue;
                    }
                    let view = HostView {
                        host: h,
                        caps: self.state.caps(&h.name),
                        local_open_to_remote_ok: false,
                    };
                    if self.state.blocked(&ticket.info, &view) {
                        continue;
                    }
                    for j in 1..=h.slots {
                        let Ok(Some(_slot)) = self.state.try_slot(&h.name, j) else {
                            continue;
                        };
                        let _target = match self.target_lock(&plan) {
                            Ok(Some(l)) => l,
                            Ok(None) => break 'hosts,
                            Err(code) => return code,
                        };
                        ticket.withdraw();
                        match self.trip(h, j, &plan, &ticket, &mut excluded, t_enter) {
                            Trip::Finished(c) => return c,
                            Trip::Requeue => {}
                            Trip::LocalPrepFailed(why) => {
                                self.log(
                                    &h.name,
                                    &plan,
                                    &format!("本机这边没准备成（不怪那台）· 改在本机跑：{why}"),
                                );
                                self.note(&format!("本机这边没准备成（{why}）：改在本机跑"));
                                plan.local = Some("远端准备在本机这边没成".into());
                            }
                        }
                        let _ = ticket.requeue(&plan);
                        break 'hosts;
                    }
                }
            }
            if self.wait_or_give_up(t_enter) {
                return self.give_up();
            }
        }
    }

    /// 产物目录锁：只在真派出去的那一下拿、跑完放（歇着的时候不拿着）。
    /// `Ok(Some(锁或不用锁))` 拿到了 · `Ok(None)` 别人在编，这一圈不派 · `Err(码)` 本机准备不成。
    #[allow(clippy::type_complexity)]
    fn target_lock(&mut self, plan: &Plan) -> Result<Option<Option<crate::queue::Lock>>, i32> {
        let Some(t) = &plan.target else {
            return Ok(Some(None));
        };
        match self.state.try_target(t) {
            Ok(Some(l)) => Ok(Some(Some(l))),
            Ok(None) => Ok(None),
            Err(e) => {
                self.say(&format!("本机准备不成：产物目录锁拿不了：{e}"));
                Err(EXIT_LOCAL_PREP)
            }
        }
    }

    /// 歇一圈；过了排队时限 ⇒ true。
    fn wait_or_give_up(&self, t_enter: Instant) -> bool {
        if self.timing.deadline.is_some_and(|d| t_enter.elapsed() >= d) {
            return true;
        }
        std::thread::sleep(self.timing.poll);
        false
    }

    fn give_up(&mut self) -> i32 {
        self.say("排队超时：没派出去");
        EXIT_TIMEOUT
    }

    fn remote_fail(&mut self, h: &Host, plan: &Plan, why: &str) -> Trip {
        self.state.cool(&h.name, COOL_DOWN);
        self.log(
            &h.name,
            plan,
            &format!(
                "远端没成（{why}）· 回队里（这台冷 {} 分钟）",
                COOL_DOWN / 60
            ),
        );
        self.note(&format!("{} 这一趟没成（{why}）：回队里", h.name));
        Trip::Requeue
    }

    fn wait_hello(&mut self, c: &mut dyn Conn) -> Result<(Hello, devsched_wire::Facts), String> {
        let t0 = Instant::now();
        let mut hello = None;
        while t0.elapsed() < self.timing.hello {
            match c.recv_timeout(self.timing.hello.saturating_sub(t0.elapsed())) {
                Recv::Frame(Frame::Hello(h)) => hello = Some(h),
                Recv::Frame(Frame::Facts(f)) => match hello {
                    Some(h) => return Ok((h, f)),
                    None => return Err("hello 之前先来了资源事实".into()),
                },
                Recv::Frame(Frame::Error(e)) => return Err(format!("节点报错：{}", e.msg)),
                Recv::Frame(_) | Recv::Bad(_) => {}
                Recv::Timeout => break,
                Recv::Closed => return Err("连上就断了".into()),
            }
        }
        Err(if hello.is_some() {
            "等不到资源事实".into()
        } else {
            "等不到 hello".into()
        })
    }

    /// 远端编译树按棵数删（CONFIG.md 主机表 `trees`）：项目目录下的树与树外产物目录（`target-*`）一起数，过了上限 ⇒
    /// 从最久没用的起一个一个叫节点 `clean`（有作业在跑的节点不删、回错误，跳过）。只按棵数判（列一下就数得准）；
    /// 盘只由 `disk` 门槛与停线管，这里不为盘删。不算的：这一趟自己的（[`Rewriter::own_names`]）、
    /// 兄弟仓 · 只读附带（[`Rewriter::shared_names`]）、镜像（项目配置 `mirror` 的第一段）、`.` 起头的（节点改了名待删的 `.del-*`）、
    /// 一趟时限之内记过「用过」的（可能正被别的趟送着或跑着）。一趟最多删 [`MAX_CLEAN_PER_TRIP`] 个。
    fn prune_trees(&mut self, h: &Host, plan: &Plan, rw: &Rewriter, conn: &mut dyn Conn) {
        let Some(p) = plan.project.as_ref() else {
            return;
        };
        let project = p.name.clone();
        let mirror = crate::git::mirror_first(p);
        let listed = match self.syncer.trees(h, plan, rw) {
            Ok(v) => v,
            Err(why) => {
                self.log(
                    &h.name,
                    plan,
                    &format!("列不了那台的远端树：{why} · 这一趟不删"),
                );
                return;
            }
        };
        let not_trees: BTreeSet<&str> = rw
            .shared_names()
            .iter()
            .map(String::as_str)
            .chain([mirror.as_str()])
            .collect();
        let trees: Vec<String> = listed
            .into_iter()
            .filter(|t| !t.starts_with('.') && !not_trees.contains(t.as_str()))
            .collect();
        let own = rw.own_names();
        let limit = h.num("trees").unwrap_or(DEFAULT_TREES).max(1) as usize;
        let mut count = trees.len() + own.iter().filter(|o| !trees.contains(o)).count();
        // 一趟时限之内记过用过的不删；记的时刻在将来（本机时钟往回拨过）也算刚用过
        let recent = trip_limit(h, plan);
        let mut cand: Vec<(Option<std::time::SystemTime>, String)> = trees
            .into_iter()
            .filter(|t| !own.contains(t))
            .map(|t| (self.state.used_at(&h.name, &project, &t), t))
            .filter(|(at, _)| !at.is_some_and(|a| a.elapsed().map_or(true, |e| e < recent)))
            .collect();
        // 没记过的（None）排最前；同样没记过的按名字
        cand.sort();
        if count > limit && cand.is_empty() {
            self.log(
                &h.name,
                plan,
                &format!("远端树 {count} 个过了 {limit}，可都是一趟时限内用过的 · 这一趟不删"),
            );
        }
        let mut cleaned = 0;
        for (_, t) in cand {
            if count <= limit || cleaned >= MAX_CLEAN_PER_TRIP {
                break;
            }
            let ask = Frame::Clean(devsched_wire::Clean {
                project: project.clone(),
                tree: t.clone(),
            });
            if conn.send(&ask).is_err() || conn.send(&Frame::Beat).is_err() {
                self.log(&h.name, plan, &format!("删远端树 {t} 时连接断了"));
                return;
            }
            // 帧照先后处理：clean 没成的错误先到，心跳回的资源事实后到
            let mut refused = None;
            let t0 = Instant::now();
            loop {
                match conn.recv_timeout(self.timing.hello.saturating_sub(t0.elapsed())) {
                    Recv::Frame(Frame::Error(e)) => refused = Some(e.msg),
                    Recv::Frame(Frame::Facts(_)) => break,
                    Recv::Frame(_) | Recv::Bad(_) => {}
                    Recv::Timeout | Recv::Closed => {
                        self.log(&h.name, plan, &format!("删远端树 {t} 后等不到节点回话"));
                        return;
                    }
                }
            }
            match refused {
                Some(why) => self.log(&h.name, plan, &format!("没删远端树 {t}：{why}")),
                None => {
                    count -= 1;
                    cleaned += 1;
                    self.log(
                        &h.name,
                        plan,
                        &format!("删了远端树 {t}（最久没用；过了 {limit}）· 还剩 {count} 个"),
                    );
                }
            }
        }
        // 这一趟删了树、或这台这个项目一天没对过：对一遍镜像，清掉已不在的树留下的记录（连手动删的）
        if cleaned > 0 || self.state.sweep_due(&h.name, &project, SWEEP_EVERY) {
            match self.syncer.sweep_mirror(h, plan, rw) {
                Ok(()) => self.state.mark_swept(&h.name, &project),
                Err(why) => self.log(&h.name, plan, &format!("对不成镜像：{why}")),
            }
        }
    }

    fn trip(
        &mut self,
        h: &Host,
        slot: u32,
        plan: &Plan,
        ticket: &Ticket,
        excluded: &mut BTreeSet<String>,
        t_enter: Instant,
    ) -> Trip {
        let mut conn = match self.connector.open(h) {
            Ok(c) => c,
            // 本机这边就起不了连接（没写 base= · 起不了 ssh）：不怪那台；连得上却断的走下面的「连上就断了」
            Err(e) => return Trip::LocalPrepFailed(format!("起不了到 {} 的连接：{e}", h.name)),
        };
        let (hello, facts) = match self.wait_hello(conn.as_mut()) {
            Ok(x) => x,
            Err(why) => return self.remote_fail(h, plan, &why),
        };
        if hello.proto != devsched_wire::proto() {
            return self.remote_fail(
                h,
                plan,
                &format!(
                    "协议指纹不同（那台 {}，本机 {}）",
                    hello.proto,
                    devsched_wire::proto()
                ),
            );
        }
        let caps: BTreeSet<String> = hello.caps.iter().map(|c| c.name.clone()).collect();
        self.state.set_caps(&h.name, &caps);
        if !hello.cargo_config.is_empty() {
            // 那台会生效的 cargo 配置（根在家目录下时家目录的也在）：接受，记一行备查
            self.log(
                &h.name,
                plan,
                &format!("那台 cargo 会读到的配置：{}", hello.cargo_config.join(" ")),
            );
        }
        let needs = self.trip_needs(plan);
        if !needs.is_subset(&caps) {
            let lack: Vec<&String> = needs.difference(&caps).collect();
            self.log(&h.name, plan, &format!("能力不够（缺 {lack:?}）· 不派"));
            conn.close();
            return Trip::Requeue;
        }
        let rw = Rewriter::new(plan, &hello.root);
        // 连上就记这棵树（连它的树外产物目录）用过：列树 · 送源码之前，送着的时候别路删树它不是最旧的
        if let Some(p) = &plan.project {
            for n in rw.own_names() {
                self.state.touch_used(&h.name, &p.name, n);
            }
        }
        // 门槛之前：树过了棵数上限先删最久没用的（盘只由门槛与停线管）
        self.prune_trees(h, plan, &rw, conn.as_mut());
        if let Err(why) = gate(h, slot, &facts) {
            self.state.cool(&h.name, COOL_GATE);
            self.log(&h.name, plan, &format!("门槛没放行：{why}"));
            conn.close();
            return Trip::Requeue;
        }
        let mut env = plan
            .project
            .as_ref()
            .map(|p| {
                let mut e = carried(p, &plan.vars);
                e.extend(crate::env::forced(p));
                e
            })
            .unwrap_or_default();
        for n in check_programs(&mut env, &hello.programs, &h.name) {
            self.note(&n);
        }
        // 心跳间隔：不长于节点时限的三分之一
        let hb = Duration::from_secs(hello.heartbeat_timeout_s.max(1));
        let beat = self.timing.beat.min(hb / 3);
        // 送源码的整段照发心跳（不然送大树超过节点的心跳时限，节点那头先断了）
        let syncer = self.syncer;
        let shared = std::sync::Mutex::new(conn);
        let done = std::sync::atomic::AtomicBool::new(false);
        let synced = std::thread::scope(|s| {
            s.spawn(|| {
                let mut last = Instant::now();
                while !done.load(std::sync::atomic::Ordering::SeqCst) {
                    std::thread::sleep(Duration::from_millis(20).min(beat));
                    if last.elapsed() >= beat {
                        let _ = shared.lock().unwrap().send(&Frame::Beat);
                        last = Instant::now();
                    }
                }
            });
            // 送源码 panic 了也要停掉心跳线程（不然 scope 等它、整个进程挂住）
            struct Done<'d>(&'d std::sync::atomic::AtomicBool);
            impl Drop for Done<'_> {
                fn drop(&mut self) {
                    self.0.store(true, std::sync::atomic::Ordering::SeqCst);
                }
            }
            let _done = Done(&done);
            syncer.sync(h, plan, &rw)
        });
        let mut conn = shared.into_inner().unwrap();
        if let Err(e) = synced {
            conn.close();
            return match e {
                SyncError::Local(why) => Trip::LocalPrepFailed(why),
                SyncError::Remote(why) => self.remote_fail(h, plan, &format!("送源码没成：{why}")),
            };
        }
        let job = Job {
            job: 1,
            project: plan
                .project
                .as_ref()
                .map(|p| p.name.clone())
                .unwrap_or_else(|| "_".into()),
            tree: plan.tree.clone(),
            rel: rel_for_job(&plan.rel),
            argv: rw.out_argv(&plan.argv),
            env: rw.out_env(&env),
            stop: stop_lines(h),
            nice: h.num("nice").map(|n| n as i32),
        };
        self.note(&format!("→ {}", h.name));
        if conn.send(&Frame::Job(job)).is_err() {
            return self.remote_fail(h, plan, "半路断（交作业时）");
        }
        let raw_path = self
            .state
            .log_dir()
            .join(format!("{}.{}.raw", ticket.info.name, h.name));
        let _ = std::fs::create_dir_all(self.state.log_dir());
        let mut raw = std::fs::File::create(&raw_path).ok();
        self.state.prune_raw(&raw_path, RAW_LOG_CAP);
        let t0 = Instant::now();
        let mut last_seen = Instant::now();
        let mut last_beat = Instant::now();
        let limit = trip_limit(h, plan);
        let mut timed_out = false;
        // 一条路径可能被拆在两帧之间：两路各攒到换行再改回本机写法。
        let mut pend = [Pending::default(), Pending::default()];
        loop {
            if !timed_out && t0.elapsed() >= limit {
                // 整趟时限到：叫节点照停作业整组收，等它的退出帧
                timed_out = true;
                let _ = conn.send(&Frame::Stop(devsched_wire::Stop { job: 1 }));
            }
            if last_beat.elapsed() >= beat {
                if conn.send(&Frame::Beat).is_err() {
                    return self.remote_fail(h, plan, "半路断");
                }
                last_beat = Instant::now();
            }
            if last_seen.elapsed() > hb {
                self.flush_pending(&mut pend, &rw);
                conn.close();
                return self.remote_fail(h, plan, "心跳断");
            }
            let wait = beat
                .saturating_sub(last_beat.elapsed())
                .max(Duration::from_millis(1));
            match conn.recv_timeout(wait) {
                Recv::Frame(f) => {
                    last_seen = Instant::now();
                    match f {
                        Frame::Out(o) => {
                            if let Some(r) = raw.as_mut() {
                                let _ = r.write_all(o.data.as_bytes());
                            }
                            let (i, w): (usize, &mut dyn Write) = match o.stream {
                                Stream::Stdout => (0, &mut *self.out),
                                Stream::Stderr => (1, &mut *self.err),
                            };
                            let text = pend[i].feed(&rw, &o.data);
                            let _ = w.write_all(text.as_bytes());
                            let _ = w.flush();
                        }
                        Frame::Exit(e) => {
                            self.flush_pending(&mut pend, &rw);
                            conn.close();
                            let ran = t0.elapsed().as_secs();
                            let waited = (t0 - t_enter).as_secs();
                            if timed_out {
                                self.log(
                                    &h.name,
                                    plan,
                                    &format!(
                                        "超时（整趟时限 {}s）· 停了作业 · 退 {EXIT_TIMEOUT} · 不冷",
                                        limit.as_secs()
                                    ),
                                );
                                self.say(&format!(
                                    "超时：{} 上这一趟跑过了整趟时限 {} 秒（timeout_s），作业已整组停掉",
                                    h.name,
                                    limit.as_secs()
                                ));
                                return Trip::Finished(EXIT_TIMEOUT);
                            }
                            return self.on_exit(h, plan, e, excluded, waited, ran);
                        }
                        Frame::Error(e) => {
                            self.log(&h.name, plan, &format!("节点报错：{}", e.msg));
                            self.note(&format!("{}：{}", h.name, e.msg));
                        }
                        _ => {}
                    }
                }
                Recv::Bad(_) => {}
                // 一阵没有新输出：留着的半行放出去（提示符这类不带换行的不久等）
                Recv::Timeout => self.flush_pending(&mut pend, &rw),
                Recv::Closed => {
                    self.flush_pending(&mut pend, &rw);
                    return self.remote_fail(h, plan, "半路断");
                }
            }
        }
    }

    fn flush_pending(&mut self, pend: &mut [Pending; 2], rw: &Rewriter) {
        for (i, p) in pend.iter_mut().enumerate() {
            let text = p.flush(rw);
            if text.is_empty() {
                continue;
            }
            let w: &mut dyn Write = if i == 0 {
                &mut *self.out
            } else {
                &mut *self.err
            };
            let _ = w.write_all(text.as_bytes());
            let _ = w.flush();
        }
    }

    fn on_exit(
        &mut self,
        h: &Host,
        plan: &Plan,
        e: devsched_wire::Exit,
        excluded: &mut BTreeSet<String>,
        waited: u64,
        ran: u64,
    ) -> Trip {
        let why = match e.breach {
            Some(b) => format!("{:?}", b).to_lowercase(),
            None => String::new(),
        };
        match e.stop {
            StopReason::Done => {
                for s in &e.skipped {
                    self.note(&format!("没跑：{s}"));
                }
                let code = e.code;
                self.log(
                    &h.name,
                    plan,
                    &format!(
                        "退 {} · 等 {waited}s · 跑 {ran}s{}",
                        code.map(|c| c.to_string()).unwrap_or("信号".into()),
                        if e.skipped.is_empty() {
                            String::new()
                        } else {
                            format!(" · 没跑：{}", e.skipped.join(" "))
                        }
                    ),
                );
                match code {
                    Some(0) if !e.skipped.is_empty() => {
                        self.say(&format!("有跳过的段：{}（命令退 0）", e.skipped.join(" ")));
                        Trip::Finished(EXIT_SKIPPED)
                    }
                    Some(c) => Trip::Finished(c),
                    None => {
                        self.say(&format!(
                            "{} 的退出帧没带退出码（帧形说 done 一定带）：当作没跑成",
                            h.name
                        ));
                        Trip::Finished(1)
                    }
                }
            }
            StopReason::Breach | StopReason::Threads => {
                let what = if e.stop == StopReason::Threads {
                    "线程上限".to_string()
                } else {
                    format!("撞停线 {why}")
                };
                self.state.cool(&h.name, COOL_BREACH);
                self.log(
                    &h.name,
                    plan,
                    &format!("{what} · 回队里（这台冷 {COOL_BREACH} 秒）"),
                );
                self.note(&format!(
                    "{} 这一趟{what}：回队里重排，上面这一截输出作废",
                    h.name
                ));
                Trip::Requeue
            }
            StopReason::Spawn => {
                self.state.cool(&h.name, COOL_SPAWN);
                self.log(
                    &h.name,
                    plan,
                    &format!("起不了进程 · 回队里（这台冷 {} 分钟）", COOL_SPAWN / 60),
                );
                self.note(&format!("{} 起不了进程：回队里重排", h.name));
                Trip::Requeue
            }
            StopReason::TmpPath => {
                excluded.insert(h.name.clone());
                let base = h.opt("base").unwrap_or("?");
                self.log(
                    &h.name,
                    plan,
                    &format!("根太长（{base}）· 临时目录路径超长 · 这一趟不再派这台 · 回队里"),
                );
                self.note(&format!(
                    "{} 的根太长（{base}：根 ＋ /t/xxxx ＋ 80 字节 socket 后缀要 < 108）：这一趟不派它",
                    h.name
                ));
                Trip::Requeue
            }
            StopReason::Heartbeat => self.remote_fail(h, plan, "节点说心跳断了"),
            StopReason::Stopped => self.remote_fail(h, plan, "节点停了作业"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::HostTable;
    use crate::testkit::*;
    use devsched_wire::{Breach, StopReason};
    use std::path::Path;

    fn quick() -> Timing {
        Timing {
            poll: Duration::from_millis(20),
            beat: Duration::from_millis(50),
            hello: Duration::from_secs(5),
            deadline: Some(Duration::from_secs(5)),
        }
    }

    struct World {
        tmp: TempDir,
        conn: FakeConnector,
        sync: FakeSync,
        local: FakeLocal,
        out: Buf,
        err: Buf,
    }

    impl World {
        fn new(hosts: &str, nodes: &[(&str, Node)]) -> World {
            let tmp = TempDir::new("dispatch");
            std::fs::create_dir_all(tmp.path().join("cfg/projects")).unwrap();
            std::fs::write(tmp.path().join("cfg/hosts"), hosts).unwrap();
            std::fs::create_dir_all(tmp.path().join("tree")).unwrap();
            World {
                tmp,
                conn: FakeConnector::with(nodes),
                sync: FakeSync::default(),
                local: FakeLocal::default(),
                out: Buf::default(),
                err: Buf::default(),
            }
        }
        fn state(&self) -> State {
            State::new(self.tmp.path().join("state"))
        }
        fn root(&self) -> &Path {
            self.tmp.path()
        }
        fn run(&self, plan: Plan, timing: Timing) -> i32 {
            let mut out = self.out.clone();
            let mut err = self.err.clone();
            let mut s = Sched {
                cfg: Config::load(&self.tmp.path().join("cfg")).unwrap(),
                state: self.state(),
                connector: &self.conn,
                syncer: &self.sync,
                local: &self.local,
                out: &mut out,
                err: &mut err,
                timing,
            };
            s.run(plan)
        }
        fn plan(&self, argv: &[&str]) -> Plan {
            plan_in(&self.tmp.path().join("tree"), argv)
        }
        fn log(&self) -> String {
            std::fs::read_to_string(self.state().log_dir().join("devsched.log")).unwrap_or_default()
        }
    }

    /// 按能力分开排：两趟排队，前一趟只有 A 台接得了、A 满着，后一趟 B 台空着 ⇒ 后一趟当场派 B，不等前一趟。
    #[test]
    fn later_trip_takes_free_b_while_earlier_waits_for_full_a() {
        let w = World::new(
            "a 1 ssh:a\nb 1 ssh:b\n",
            &[("a", Node::new(&["gui"])), ("b", Node::new(&[]))],
        );
        let st = w.state();
        st.set_caps("a", &["gui".to_string()].into_iter().collect());
        st.set_caps("b", &BTreeSet::new());
        // 前一趟：要 gui（只 A 接得了），排在前面、还活着
        let mut earlier = w.plan(&["cargo", "test"]);
        earlier.needs.insert("gui".into());
        let _earlier_ticket = st.take_ticket(&earlier).unwrap();
        // A 满着
        let _a_full = st.try_slot("a", 1).unwrap().unwrap();
        let code = w.run(w.plan(&["cargo", "check"]), quick());
        assert_eq!(code, 0, "{}", w.err.text());
        assert_eq!(w.conn.job_hosts(), ["b"]);
    }

    #[test]
    fn earlier_trip_any_host_can_take_blocks_later() {
        let w = World::new("b 1 ssh:b\n", &[("b", Node::new(&[]))]);
        let st = w.state();
        let earlier = w.plan(&["cargo", "test"]);
        let _t = st.take_ticket(&earlier).unwrap();
        let code = w.run(
            w.plan(&["cargo", "check"]),
            Timing {
                deadline: Some(Duration::from_millis(200)),
                ..quick()
            },
        );
        assert_eq!(code, 124);
        assert!(w.conn.job_hosts().is_empty());
    }

    #[test]
    fn job_frame_carries_rewritten_argv_stop_lines_and_nice() {
        let w = World::new(
            "a 2 ssh:a mem=6/2 psi=25/60 nice=19\n",
            &[("a", Node::new(&[]))],
        );
        let mut plan = w.plan(&["cargo", "test", "--manifest-path"]);
        let root = plan.root.to_string_lossy().to_string();
        plan.argv.push(format!("{root}/Cargo.toml"));
        let tree = plan.tree.clone();
        assert_eq!(w.run(plan, quick()), 0);
        let jobs = w.conn.jobs.lock().unwrap();
        let j = &jobs[0].1;
        assert_eq!(
            j.argv[3],
            format!("/srv/ds/projects/alpha/{tree}/Cargo.toml")
        );
        assert_eq!(j.stop.mem, Some(2048));
        assert_eq!(j.stop.psi, Some(60.0));
        assert_eq!(j.nice, Some(19));
        assert!(j.env.is_empty(), "调度一个变量都不加");
        assert_eq!(
            w.sync.sent.lock().unwrap()[0].1,
            format!("/srv/ds/projects/alpha/{tree}")
        );
    }

    #[test]
    fn red_run_returns_command_code_without_cooling() {
        let w = World::new(
            "a 1 ssh:a\n",
            &[(
                "a",
                Node::new(&[]).reply(Reply::Exit {
                    outs: vec![
                        (Stream::Stdout, "out\n".into()),
                        (Stream::Stderr, "/srv/ds/projects/alpha/x err\n".into()),
                    ],
                    code: Some(3),
                    stop: StopReason::Done,
                    breach: None,
                    skipped: vec![],
                }),
            )],
        );
        assert_eq!(w.run(w.plan(&["cargo", "test"]), quick()), 3);
        assert_eq!(w.out.text(), "out\n");
        assert!(w.err.text().contains("err"));
        assert!(!w.state().cooling("a"));
    }

    #[test]
    fn skipped_segments_are_said_and_exit_79() {
        let w = World::new(
            "a 1 ssh:a\n",
            &[(
                "a",
                Node::new(&[]).reply(Reply::Exit {
                    outs: vec![],
                    code: Some(0),
                    stop: StopReason::Done,
                    breach: None,
                    skipped: vec!["mobile".into(), "win-check".into()],
                }),
            )],
        );
        assert_eq!(w.run(w.plan(&["cargo", "test"]), quick()), EXIT_SKIPPED);
        let e = w.err.text();
        assert!(e.contains("没跑：mobile") && e.contains("没跑：win-check"));
        assert!(
            e.trim_end()
                .lines()
                .last()
                .unwrap()
                .starts_with("[devsched] 有跳过的段"),
            "{e}"
        );
    }

    #[test]
    fn unreachable_host_is_cooled_and_next_one_used() {
        let w = World::new("a 1 ssh:a\nb 1 ssh:b\n", &[("b", Node::new(&[]))]);
        assert_eq!(w.run(w.plan(&["cargo", "test"]), quick()), 0);
        assert!(w.state().cooling("a"));
        assert_eq!(w.conn.job_hosts(), ["b"]);
        assert!(w.log().contains("连上就断了"));
    }

    #[test]
    fn dropped_midway_cools_and_requeues() {
        let w = World::new(
            "a 1 ssh:a\nb 1 ssh:b\n",
            &[
                ("a", Node::new(&[]).reply(Reply::Drop)),
                ("b", Node::new(&[])),
            ],
        );
        assert_eq!(w.run(w.plan(&["cargo", "test"]), quick()), 0);
        assert!(w.state().cooling("a"));
        assert_eq!(w.conn.job_hosts(), ["a", "b"]);
    }

    #[test]
    fn silent_node_is_heartbeat_loss() {
        let mut n = Node::new(&[]).reply(Reply::Silent);
        n.heartbeat_timeout_s = 1;
        let w = World::new("a 1 ssh:a\nb 1 ssh:b\n", &[("a", n), ("b", Node::new(&[]))]);
        assert_eq!(w.run(w.plan(&["cargo", "test"]), quick()), 0);
        assert!(w.state().cooling("a"));
        assert!(w.log().contains("心跳断"));
    }

    #[test]
    fn breach_cools_60s_and_retries_elsewhere() {
        let w = World::new(
            "a 1 ssh:a\nb 1 ssh:b\n",
            &[
                (
                    "a",
                    Node::new(&[]).reply(Reply::Exit {
                        outs: vec![],
                        code: None,
                        stop: StopReason::Breach,
                        breach: Some(Breach::Mem),
                        skipped: vec![],
                    }),
                ),
                ("b", Node::new(&[])),
            ],
        );
        assert_eq!(w.run(w.plan(&["cargo", "test"]), quick()), 0);
        let until: u64 = std::fs::read_to_string(w.state().dir.join("cool/a"))
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        assert!(until <= now + COOL_BREACH && until + 5 >= now + COOL_BREACH);
        assert!(w.log().contains("撞停线 mem"));
    }

    #[test]
    fn tmp_path_excludes_host_without_cooling() {
        let w = World::new(
            "a 1 ssh:a\nb 1 ssh:b\n",
            &[
                ("a", Node::new(&[]).reply(Reply::stop(StopReason::TmpPath))),
                ("b", Node::new(&[])),
            ],
        );
        assert_eq!(w.run(w.plan(&["cargo", "test"]), quick()), 0);
        assert!(!w.state().cooling("a"));
        assert_eq!(w.conn.job_hosts(), ["a", "b"]);
    }

    #[test]
    fn root_too_long_on_the_only_remote_opens_local_fallback_at_once() {
        let w = World::new(
            "a 1 ssh:a\nl 1 local fallback=15\n",
            &[("a", Node::new(&[]).reply(Reply::stop(StopReason::TmpPath)))],
        );
        let rc = w.run(
            w.plan(&["cargo", "test"]),
            Timing {
                deadline: Some(Duration::from_secs(5)),
                ..quick()
            },
        );
        assert_eq!(
            rc,
            0,
            "不等 fallback 的 15 分钟：当场在本机跑；stderr：{}",
            w.err.text()
        );
        assert_eq!(w.local.runs.lock().unwrap().len(), 1);
        assert!(
            w.err.text().contains("a 的根太长"),
            "当场说清：{}",
            w.err.text()
        );
    }

    #[test]
    fn raw_outputs_are_capped_oldest_first() {
        let w = World::new("a 1 ssh:a\n", &[("a", Node::new(&[]))]);
        let log = w.state().log_dir();
        std::fs::create_dir_all(&log).unwrap();
        let big = vec![b'x'; 30 << 20];
        for (i, name) in ["old1.a.raw", "old2.a.raw"].iter().enumerate() {
            let p = log.join(name);
            std::fs::write(&p, &big).unwrap();
            let t = std::time::SystemTime::now() - Duration::from_secs(3600 - i as u64 * 60);
            std::fs::File::options()
                .write(true)
                .open(&p)
                .unwrap()
                .set_modified(t)
                .unwrap();
        }
        assert_eq!(w.run(w.plan(&["cargo", "test"]), quick()), 0);
        assert!(!log.join("old1.a.raw").exists(), "最旧的删掉");
        assert!(log.join("old2.a.raw").exists(), "删到上限内就停");
        let total: u64 = std::fs::read_dir(&log)
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().ends_with(".raw"))
            .map(|e| e.metadata().unwrap().len())
            .sum();
        assert!(total <= RAW_LOG_CAP, "原文总量在上限内：{total}");
        assert!(
            std::fs::read_dir(&log).unwrap().flatten().any(|e| {
                let n = e.file_name().to_string_lossy().to_string();
                n.ends_with(".a.raw") && !n.starts_with("old")
            }),
            "这一趟的在"
        );
    }

    #[test]
    fn spawn_failure_cools_five_minutes() {
        let w = World::new(
            "a 1 ssh:a\nb 1 ssh:b\n",
            &[
                ("a", Node::new(&[]).reply(Reply::stop(StopReason::Spawn))),
                ("b", Node::new(&[])),
            ],
        );
        assert_eq!(w.run(w.plan(&["cargo", "test"]), quick()), 0);
        assert!(w.state().cooling("a"));
        assert!(w.log().contains("起不了进程"));
    }

    #[test]
    fn local_prep_failure_runs_locally_without_cooling() {
        let w = World::new("a 1 ssh:a\nl 1 local\n", &[("a", Node::new(&[]))]);
        *w.sync.fail.lock().unwrap() = Some(SyncError::Local("写不了清单".into()));
        assert_eq!(w.run(w.plan(&["cargo", "test"]), quick()), 0);
        assert!(!w.state().cooling("a"));
        assert_eq!(w.local.runs.lock().unwrap().len(), 1);
        assert!(w.conn.job_hosts().is_empty());
        // 本机也起不来 ⇒ 78
        let w = World::new("a 1 ssh:a\nl 1 local\n", &[("a", Node::new(&[]))]);
        *w.sync.fail.lock().unwrap() = Some(SyncError::Local("写不了清单".into()));
        w.local
            .outcomes
            .lock()
            .unwrap()
            .push(LocalOutcome::CannotStart("没有这个命令".into()));
        assert_eq!(w.run(w.plan(&["cargo", "test"]), quick()), EXIT_LOCAL_PREP);
        assert!(w
            .err
            .text()
            .trim_end()
            .lines()
            .last()
            .unwrap()
            .starts_with("[devsched] 本机准备不成"));
    }

    #[test]
    fn gate_closed_host_is_skipped() {
        let mut n = Node::new(&[]);
        n.facts.psi = None;
        let w = World::new(
            "a 1 ssh:a psi=25/60\nb 1 ssh:b\n",
            &[("a", n), ("b", Node::new(&[]))],
        );
        assert_eq!(w.run(w.plan(&["cargo", "test"]), quick()), 0);
        assert_eq!(w.conn.job_hosts(), ["b"], "判不了当不放行");
    }

    #[test]
    fn stale_caps_are_probed_again() {
        let w = World::new("a 1 ssh:a\n", &[("a", Node::new(&["gui"]))]);
        // 记下的能力是老节点报的（没有 gui），文件 11 分钟前写的
        w.state().set_caps("a", &BTreeSet::new());
        let f = w.state().dir.join("caps/a");
        let old = std::time::SystemTime::now() - Duration::from_secs(11 * 60);
        std::fs::File::options()
            .write(true)
            .open(&f)
            .unwrap()
            .set_modified(old)
            .unwrap();
        let mut p = w.plan(&["cargo", "test"]);
        p.needs.insert("gui".into());
        assert_eq!(
            w.run(
                p,
                Timing {
                    deadline: Some(Duration::from_secs(5)),
                    ..quick()
                }
            ),
            0
        );
        assert_eq!(
            w.conn.job_hosts(),
            ["a"],
            "过期的记录不挡：照旧连一次、照新 hello 派"
        );
    }

    #[test]
    fn host_lacking_caps_is_learned_and_skipped() {
        let w = World::new(
            "a 1 ssh:a\nb 1 ssh:b\n",
            &[("a", Node::new(&["tmux"])), ("b", Node::new(&["gui"]))],
        );
        let mut p = w.plan(&["cargo", "test"]);
        p.needs.insert("gui".into());
        assert_eq!(w.run(p, quick()), 0);
        assert_eq!(w.conn.job_hosts(), ["b"]);
        assert_eq!(
            w.state().caps("a"),
            Some(["tmux".to_string()].into_iter().collect())
        );
        assert!(!w.state().cooling("a"), "能力不够不冷");
    }

    #[test]
    fn host_without_rsync_cap_is_skipped_when_sync_needs_it() {
        let w = World::new(
            "a 1 ssh:a\nb 1 ssh:b\n",
            &[
                ("a", Node::new(&["cargo"])),
                ("b", Node::new(&["cargo", "rsync"])),
            ],
        );
        *w.sync.needs.lock().unwrap() = Some("rsync");
        assert_eq!(w.run(w.plan(&["cargo", "test"]), quick()), 0);
        assert_eq!(
            w.conn.job_hosts(),
            ["b"],
            "a 没报 rsync（版本不够或没装）⇒ 不派"
        );
        assert!(!w.state().cooling("a"), "能力不够不冷");
    }

    #[test]
    fn proto_mismatch_is_a_remote_failure() {
        let mut n = Node::new(&[]);
        n.proto = "ffffffffffffffff".into();
        let w = World::new("a 1 ssh:a\nb 1 ssh:b\n", &[("a", n), ("b", Node::new(&[]))]);
        assert_eq!(w.run(w.plan(&["cargo", "test"]), quick()), 0);
        assert!(w.log().contains("协议指纹不同"));
    }

    #[test]
    fn local_only_never_connects_and_heavy_takes_slot_one() {
        let w = World::new("a 1 ssh:a\nl 2 local\n", &[("a", Node::new(&[]))]);
        let mut p = w.plan(&["gradle", "build"]);
        p.local = Some("本机重活".into());
        p.heavy = true;
        let _held = w.state().try_slot("l", 1).unwrap().unwrap();
        // 第 1 个位被占：重活不拿第 2 个位，等到时限
        assert_eq!(
            w.run(
                p.clone(),
                Timing {
                    deadline: Some(Duration::from_millis(200)),
                    ..quick()
                }
            ),
            124
        );
        drop(_held);
        assert_eq!(w.run(p, quick()), 0);
        assert!(w.conn.opened.lock().unwrap().is_empty());
        assert_eq!(w.local.runs.lock().unwrap()[0].1, 1);
    }

    #[test]
    fn says_which_host_took_the_trip() {
        let w = World::new("a 1 ssh:a\nl 2 local\n", &[("a", Node::new(&[]))]);
        assert_eq!(w.run(w.plan(&["cargo", "test"]), quick()), 0);
        assert!(
            w.err.text().contains(&format!("{NOTE} → a\n")),
            "{}",
            w.err.text()
        );
        let mut p = w.plan(&["cargo", "build"]);
        p.local = Some("只本机".into());
        assert_eq!(w.run(p, quick()), 0);
        assert!(
            w.err.text().contains(&format!("{NOTE} → l（本机）\n")),
            "{}",
            w.err.text()
        );
    }

    #[test]
    fn local_fallback_waits_while_remotes_are_fine() {
        let w = World::new(
            "a 1 ssh:a\nl 1 local fallback=15\n",
            &[("a", Node::new(&[]))],
        );
        let _a = w.state().try_slot("a", 1).unwrap().unwrap();
        assert_eq!(
            w.run(
                w.plan(&["cargo", "test"]),
                Timing {
                    deadline: Some(Duration::from_millis(200)),
                    ..quick()
                }
            ),
            124
        );
        assert!(
            w.local.runs.lock().unwrap().is_empty(),
            "能出本机的不占本机的位"
        );
        // 没写 fallback ⇒ 本机就是排在后面的一台
        let w = World::new("a 1 ssh:a\nl 1 local\n", &[("a", Node::new(&[]))]);
        let _a = w.state().try_slot("a", 1).unwrap().unwrap();
        assert_eq!(w.run(w.plan(&["cargo", "test"]), quick()), 0);
        assert_eq!(w.local.runs.lock().unwrap().len(), 1);
    }

    #[test]
    fn same_target_dir_one_at_a_time() {
        let w = World::new("a 2 ssh:a\n", &[("a", Node::new(&[]))]);
        let mut p = w.plan(&["cargo", "test"]);
        p.target = Some(w.root().join("tgt"));
        let _held = w
            .state()
            .try_target(&w.root().join("tgt"))
            .unwrap()
            .unwrap();
        assert_eq!(
            w.run(
                p.clone(),
                Timing {
                    deadline: Some(Duration::from_millis(200)),
                    ..quick()
                }
            ),
            124
        );
        drop(_held);
        assert_eq!(w.run(p, quick()), 0);
    }

    #[test]
    fn local_breach_requeues() {
        let w = World::new("l 1 local\n", &[]);
        w.local
            .outcomes
            .lock()
            .unwrap()
            .push(LocalOutcome::Breach("可用内存 1G < 2G".into()));
        // 冷 60 秒之后才再派 ⇒ 时限内派不出去
        assert_eq!(
            w.run(
                w.plan(&["cargo", "build"]),
                Timing {
                    deadline: Some(Duration::from_millis(300)),
                    ..quick()
                }
            ),
            124
        );
        assert!(w.err.text().contains("撞停线"));
        assert_eq!(w.local.runs.lock().unwrap().len(), 1);
    }

    #[test]
    fn beats_keep_flowing_while_source_is_sent() {
        let mut n = Node::new(&[]);
        n.heartbeat_timeout_s = 1;
        let w = World::new("a 1 ssh:a\n", &[("a", n)]);
        // 送源码一直送到那台收到 3 次心跳才完：送的期间心跳照发，那台（1 秒时限）才不断
        *w.sync.until_beats.lock().unwrap() = Some((w.conn.beats.clone(), 3));
        assert_eq!(
            w.run(
                w.plan(&["cargo", "test"]),
                Timing {
                    deadline: Some(Duration::from_secs(3)),
                    ..quick()
                }
            ),
            0,
            "{}",
            w.err.text()
        );
        assert_eq!(w.conn.job_hosts(), ["a"]);
        assert!(!w.state().cooling("a"));
    }

    #[test]
    fn cannot_open_connection_is_local_prep_not_remote() {
        let w = World::new("a 1 ssh:a\nl 1 local\n", &[("a", Node::new(&[]))]);
        w.conn.refuse.lock().unwrap().insert("a".into());
        assert_eq!(w.run(w.plan(&["cargo", "test"]), quick()), 0);
        assert!(!w.state().cooling("a"), "不怪那台");
        assert_eq!(w.local.runs.lock().unwrap().len(), 1, "改在本机跑");
    }

    #[test]
    fn panicking_sync_does_not_hang() {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let w = World::new("a 1 ssh:a\n", &[("a", Node::new(&[]))]);
            *w.sync.panic.lock().unwrap() = true;
            let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                w.run(w.plan(&["cargo", "test"]), quick())
            }));
            let _ = tx.send(r.is_err());
        });
        assert_eq!(
            rx.recv_timeout(devsched_wire::WALL_CEILING),
            Ok(true),
            "panic 照抛出来，不挂住"
        );
    }

    #[test]
    fn target_lock_is_not_held_while_waiting() {
        let w = World::new("a 1 ssh:a\n", &[("a", Node::new(&[]))]);
        let mut p = w.plan(&["cargo", "test"]);
        let tgt = w.root().join("tgt");
        p.target = Some(tgt.clone());
        let held = w.state().try_slot("a", 1).unwrap().unwrap();
        let st = w.state();
        std::thread::scope(|s| {
            let h = s.spawn(|| {
                w.run(
                    p,
                    Timing {
                        deadline: Some(Duration::from_millis(600)),
                        ..quick()
                    },
                )
            });
            // 等它排上（号文件出现）再看：等条件，不睡一会儿
            let t = Instant::now();
            while std::fs::read_dir(st.dir.join("q"))
                .map(|d| d.count())
                .unwrap_or(0)
                == 0
                && t.elapsed() < devsched_wire::WALL_CEILING
            {
                std::thread::sleep(Duration::from_millis(5));
            }
            assert!(
                st.try_target(&tgt).unwrap().is_some(),
                "排着的时候不拿着产物目录锁"
            );
            assert_eq!(h.join().unwrap(), 124);
        });
        drop(held);
    }

    #[test]
    fn local_only_without_local_row_says_78() {
        let w = World::new("a 1 ssh:a\n", &[("a", Node::new(&[]))]);
        let mut p = w.plan(&["cargo", "build"]);
        p.local = Some("--local".into());
        assert_eq!(w.run(p, quick()), EXIT_LOCAL_PREP);
        assert!(w
            .err
            .text()
            .trim_end()
            .lines()
            .last()
            .unwrap()
            .starts_with("[devsched] 本机准备不成"));
    }

    #[test]
    fn notes_on_the_way_do_not_look_like_the_tail() {
        let mut n = Node::new(&[]);
        n.programs = vec![];
        let w = World::new("a 1 ssh:a\n", &[("a", n.reply(Reply::ok(4)))]);
        let mut p = w.plan(&["cargo", "test"]);
        let mut keys = std::collections::BTreeMap::new();
        keys.insert("env".to_string(), "RUSTC_WRAPPER".to_string());
        p.project = Some(crate::config::Project {
            name: "alpha".into(),
            keys,
        });
        p.vars.insert("RUSTC_WRAPPER".into(), "sccache".into());
        assert_eq!(w.run(p, quick()), 4);
        let e = w.err.text();
        assert!(e.contains("RUSTC_WRAPPER 不带过去"));
        assert!(
            !e.lines().any(|l| l.starts_with("[devsched] ")),
            "命令自己退的码，末行没有收尾那一行：{e}"
        );
    }

    #[test]
    fn own_output_never_goes_to_untrusted_host() {
        let w = World::new(
            "a 1 ssh:a\nb 1 ssh:b trust=own\n",
            &[("a", Node::new(&[])), ("b", Node::new(&[]))],
        );
        let mut p = w.plan(&["cargo", "test", "bless"]);
        p.own = true; // 计划本来会把它定成只本机；这里不定，单看信任这一道
        assert_eq!(w.run(p, quick()), 0);
        assert_eq!(w.conn.job_hosts(), ["b"]);
    }

    #[test]
    fn ssh_argv_needs_base_and_is_an_array() {
        let t = HostTable::parse("a 1 ssh:a-host base=/srv/ds/\nb 1 ssh:b\n");
        let v = ssh_argv(&t.hosts[0]).unwrap();
        assert_eq!(v[0], "ssh");
        assert_eq!(v[v.len() - 2], "a-host");
        assert_eq!(
            v[v.len() - 1],
            "'/srv/ds/state/devsched-node' --root '/srv/ds'"
        );
        assert!(ssh_argv(&t.hosts[1]).is_err());
    }

    #[test]
    fn ssh_argv_passes_slurm_args_to_the_node() {
        let t = HostTable::parse(
            "a 1 ssh:a base=/srv/ds slurm=-p,cpu,-c,16,--mem=32G\nb 1 ssh:b base=/srv/ds\n",
        );
        assert_eq!(
            ssh_argv(&t.hosts[0]).unwrap().last().unwrap(),
            "'/srv/ds/state/devsched-node' --root '/srv/ds' --slurm '-p,cpu,-c,16,--mem=32G'"
        );
        assert!(
            !ssh_argv(&t.hosts[1])
                .unwrap()
                .last()
                .unwrap()
                .contains("--slurm"),
            "没写 slurm= ⇒ 不带"
        );
    }

    #[test]
    fn sccache_off_project_blanks_rustc_wrapper_in_the_job() {
        let w = World::new("a 1 ssh:a\n", &[("a", Node::new(&[]))]);
        let mut p = w.plan(&["cargo", "test"]);
        let keys = &mut p.project.as_mut().unwrap().keys;
        keys.insert("sccache".into(), "0".into());
        keys.insert("env".into(), "RUSTC_WRAPPER".into());
        // 调用方设着 sccache 也盖掉：那台 ~/.cargo/config.toml 的 rustc-wrapper 也由这个空值盖掉
        p.vars.insert("RUSTC_WRAPPER".into(), "sccache".into());
        assert_eq!(w.run(p, quick()), 0);
        let jobs = w.conn.jobs.lock().unwrap();
        assert_eq!(
            jobs[0].1.env.get("RUSTC_WRAPPER").map(|s| s.as_str()),
            Some(""),
            "{:?}",
            jobs[0].1.env
        );
    }

    #[test]
    fn project_hosts_key_limits_remote_hosts() {
        let w = World::new(
            "a 1 ssh:a\nb 1 ssh:b\n",
            &[("a", Node::new(&[])), ("b", Node::new(&[]))],
        );
        let mut p = w.plan(&["cargo", "test"]);
        p.project
            .as_mut()
            .unwrap()
            .keys
            .insert("hosts".into(), "b".into());
        assert_eq!(w.run(p, quick()), 0);
        assert_eq!(w.conn.job_hosts(), ["b"]);
        assert!(
            !w.conn.opened.lock().unwrap().iter().any(|h| h == "a"),
            "项目只许 b ⇒ a 一趟都不连"
        );
    }

    /// 记下几棵树「上回用过」的先后：越靠前越旧（修改时刻往回拨）。
    fn used_in_order(st: &State, host: &str, trees: &[&str]) {
        let now = std::time::SystemTime::now();
        for (i, t) in trees.iter().enumerate() {
            st.touch_used(host, "alpha", t);
            let p = st.dir.join("used").join(host).join("alpha").join(t);
            let at = now - Duration::from_secs(86400 * (trees.len() - i) as u64);
            std::fs::File::options()
                .write(true)
                .open(&p)
                .unwrap()
                .set_modified(at)
                .unwrap();
        }
    }

    #[test]
    fn over_tree_limit_cleans_oldest_first_skipping_busy_and_keeps_this_one() {
        let mut node = Node::new(&[]);
        node.busy = vec!["t2".into()];
        let w = World::new("a 1 ssh:a trees=3\n", &[("a", node)]);
        let plan = w.plan(&["cargo", "test"]);
        let this = plan.tree.clone();
        let mut on_a: Vec<String> = [
            "_mirror.git",
            ".del-t0.123.1",
            "old1",
            "t1",
            "t2",
            "t3",
            "t4",
            "t5",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        on_a.push(this.clone());
        w.sync.trees.lock().unwrap().insert("a".into(), on_a);
        used_in_order(&w.state(), "a", &["t1", "t2", "t3", "t4", "t5"]);
        assert_eq!(w.run(plan, quick()), 0, "{}", w.err.text());
        let cleaned: Vec<String> = w
            .conn
            .cleaned
            .lock()
            .unwrap()
            .iter()
            .map(|(_, t)| t.clone())
            .collect();
        // 7 棵树（镜像与 .del-* 不算）、留 3 棵：没记过的 old1 最先，t2 在跑跳过；剩 t2 · t5 · 这一趟的
        assert_eq!(cleaned, ["old1", "t1", "t3", "t4"]);
        assert_eq!(*w.sync.swept.lock().unwrap(), ["a"], "删了树 ⇒ 对一遍镜像");
        assert_eq!(w.conn.job_hosts(), ["a"], "删完照派");
        assert!(
            w.state().used_at("a", "alpha", &this).is_some(),
            "这一趟的树记下用过"
        );
    }

    #[test]
    fn low_disk_alone_cleans_nothing_and_the_gate_decides() {
        let mut node = Node::new(&[]);
        node.facts.disk = Some(4 * 1024);
        let w = World::new("a 1 ssh:a disk=10/1\n", &[("a", node)]);
        let plan = w.plan(&["cargo", "test"]);
        w.sync.trees.lock().unwrap().insert(
            "a".into(),
            ["t1", "t2", "t3"].iter().map(|s| s.to_string()).collect(),
        );
        used_in_order(&w.state(), "a", &["t1", "t2", "t3"]);
        // 树数没过上限：盘紧也不删（量数在删的当下跟不上），只由门槛不放行
        let code = w.run(
            plan,
            Timing {
                deadline: Some(Duration::from_millis(300)),
                ..quick()
            },
        );
        assert_eq!(code, EXIT_TIMEOUT);
        assert!(cleaned_on(&w).is_empty(), "{:?}", cleaned_on(&w));
        assert!(w.conn.job_hosts().is_empty());
    }

    fn cleaned_on(w: &World) -> Vec<String> {
        w.conn
            .cleaned
            .lock()
            .unwrap()
            .iter()
            .map(|(_, t)| t.clone())
            .collect()
    }

    #[test]
    fn one_trip_cleans_at_most_four_and_never_the_mirror_whatever_it_is_called() {
        let w = World::new("a 1 ssh:a trees=1\n", &[("a", Node::new(&[]))]);
        let mut plan = w.plan(&["cargo", "test"]);
        plan.project
            .as_mut()
            .unwrap()
            .keys
            .insert("mirror".into(), "mirror.git".into());
        let trees: Vec<String> = ["mirror.git", "t1", "t2", "t3", "t4", "t5", "t6"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        w.sync.trees.lock().unwrap().insert("a".into(), trees);
        used_in_order(&w.state(), "a", &["t1", "t2", "t3", "t4", "t5", "t6"]);
        assert_eq!(w.run(plan, quick()), 0, "{}", w.err.text());
        assert_eq!(
            cleaned_on(&w),
            ["t1", "t2", "t3", "t4"],
            "一趟至多 4 棵、镜像不碰"
        );
        assert!(w.log().contains("还剩 3 个"), "{}", w.log());
    }

    #[test]
    fn target_dirs_go_with_their_tree_and_shared_ones_and_recent_ones_stay() {
        let w = World::new("a 1 ssh:a trees=1\n", &[("a", Node::new(&[]))]);
        let mut plan = w.plan(&["cargo", "test"]);
        let sib = w.root().join("sibling-repo");
        plan.project
            .as_mut()
            .unwrap()
            .keys
            .insert("siblings".into(), sib.display().to_string());
        // 这一趟的树外产物目录
        plan.target = Some(w.root().join("outside-target"));
        let rw = Rewriter::new(&plan, "/srv/ds");
        let sib_name = rw.shared_names()[0].clone();
        let my_target = rw.own_names()[1].clone();
        let mut on_a: Vec<String> = ["t1", "busy-now", "target-0a1b2c3d", "_mirror.git"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        on_a.push(sib_name);
        on_a.push(my_target.clone());
        w.sync.trees.lock().unwrap().insert("a".into(), on_a);
        used_in_order(&w.state(), "a", &["t1"]);
        // 别的趟刚连上、正送着
        w.state().touch_used("a", "alpha", "busy-now");
        assert_eq!(w.run(plan, quick()), 0, "{}", w.err.text());
        // 别人的 target-*（没记过）最先，其次 t1；刚用过的 · 兄弟仓 · 镜像 · 这一趟自己的产物目录不删
        assert_eq!(cleaned_on(&w), ["target-0a1b2c3d", "t1"]);
        assert!(
            w.state().used_at("a", "alpha", &my_target).is_some(),
            "这一趟的产物目录跟着树记下用过"
        );
    }

    #[test]
    fn under_the_limits_nothing_is_cleaned() {
        let w = World::new("a 1 ssh:a trees=3 disk=10/1\n", &[("a", Node::new(&[]))]);
        let plan = w.plan(&["cargo", "test"]);
        w.sync.trees.lock().unwrap().insert(
            "a".into(),
            ["t1", "t2", "_mirror.git"]
                .iter()
                .map(|s| s.to_string())
                .collect(),
        );
        assert_eq!(w.run(plan.clone(), quick()), 0);
        assert!(w.conn.cleaned.lock().unwrap().is_empty());
        // 没对过 ⇒ 这一趟对一遍；一天之内再来、又没删树 ⇒ 不再对
        assert_eq!(*w.sync.swept.lock().unwrap(), ["a"]);
        assert_eq!(w.run(plan, quick()), 0);
        assert_eq!(*w.sync.swept.lock().unwrap(), ["a"]);
    }
}
