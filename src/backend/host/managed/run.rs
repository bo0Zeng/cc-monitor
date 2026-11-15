//! 托管进程本体：`ccm -- --host-run <id> [键=值…] -- <argv>`（A3-3-1）。
//!
//! 持一个伪终端（里面跑 `argv`），把它的输出算进屏（[`super::screen`]），经托管口（[`super::wire`] · [`super::serve`]）给各端；
//! 输入只收持有者的。子进程退了 ⇒ 各端收 `ended{code}`、删自己的目录、退出（Claude 退出，终端跟着收）。
//! 收到 `end` ⇒ 子进程组 SIGHUP，等它退。
//!
//! 线程按需（[`process`]）：一个托管进程只托管这一个终端，跑在自己那一个单线程运行时上（收连接与各端的读写是它上面的任务）；
//! 常驻两条自己的线：读伪终端（阻塞读，永远不等哪一端）· 等子进程（不挂在运行时的阻塞池上：子进程不退它们就不回，挂在池上丢运行时要等它们）；
//! 写伪终端那一条借运行时的阻塞池，有字要写才起、写空了就还（写满了只堵它自己），闲了由池收掉。空闲时三条线、不随核数变；池有上限（[`blocking_cap`]）。
//! 那一张账（[`Hub`]）在一把锁里，谁改它谁当场把要做的事做掉（[`Ctx::apply`]）。

use super::home::{Facts, Home};
use super::serve::{Effect, Hub, Outbox};
use super::wire::{self, Size};
use crate::platform::child::Terminal;
use crate::platform::fs::Own;
use copy_core::copy_text;
use copy_core::said::Said;
use std::collections::{BTreeMap, VecDeque};
use std::io::{Read, Write};
use std::sync::{Arc, Mutex, MutexGuard};
use tokio::io::AsyncWriteExt;

/// 没给尺寸时的样子。
pub(crate) const DEFAULT_SIZE: Size = Size { rows: 24, cols: 80 };

/// 托管进程里那个子进程的环境：`CCM_HOST` 让里面的 ccm 写便条时带上宿主（A3-3-4）；终端型号照 tmux 的 `default-terminal` 那一类给。
pub const HOST_ENV: &str = "CCM_HOST";
const CHILD_ENV: [(&str, &str); 2] = [("TERM", "xterm-256color"), ("COLORTERM", "truecolor")];

/// 起一个托管终端要的那几样。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Spec {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) size: Size,
    pub(crate) cwd: String,
    pub(crate) agent: String,
    /// 起它时点名的号（`--account`；空 ＝ 没点名）。
    pub(crate) account: String,
    pub client: String,
    pub(crate) argv: Vec<String>,
}

/// `<id> [name=N] [size=列x行] [cwd=D] [agent=A] [account=号] [client=C] -- <argv…>`（`--host-run` 之后那一串；那个词住 `wire::ccm_line::argv::flag`）。
/// 选项写成 `键=值`：它们是 ccm 起托管进程时自己拼的，不是给人敲的旗标。
pub(crate) fn parse(args: &[String]) -> Result<Spec, Said> {
    let bad = |what: &str| -> Said { copy_text("beHost.run.badArgs", &[("what", what)]).into() };
    let mut it = args.iter();
    let id = it.next().ok_or_else(|| bad("id"))?.clone();
    if !super::home::id_ok(&id) {
        return Err(copy_text("beHost.home.badId", &[("id", &id)]).into());
    }
    let mut spec = Spec {
        name: id.clone(),
        id,
        size: DEFAULT_SIZE,
        cwd: String::new(),
        agent: String::new(),
        account: String::new(),
        client: String::new(),
        argv: Vec::new(),
    };
    while let Some(a) = it.next() {
        if a == crate::wire::ccm_line::argv::flag::END {
            spec.argv = it.cloned().collect();
            break;
        }
        let (k, v) = a.split_once('=').ok_or_else(|| bad(a))?;
        let v = v.to_string();
        match k {
            "name" if super::home::id_ok(&v) => spec.name = v,
            "size" => spec.size = size_of(&v).ok_or_else(|| bad(a))?,
            "cwd" => spec.cwd = v,
            "agent" => spec.agent = v,
            "account" => spec.account = v,
            "client" => spec.client = v,
            _ => return Err(bad(a)),
        }
    }
    if spec.argv.is_empty() {
        return Err(bad("argv"));
    }
    Ok(spec)
}

/// `列x行`（同 `--tmux-size` 的写法）。
pub(crate) fn size_of(s: &str) -> Option<Size> {
    let (c, r) = s.split_once('x')?;
    let (cols, rows) = (c.parse::<u16>().ok()?, r.parse::<u16>().ok()?);
    (cols > 0 && rows > 0).then_some(Size { rows, cols })
}

/// 后端词的入口：解参数、跑，交回进程退出码。起它的那一方经回信管道等它（[`crate::platform::child::Child::detach_until_ready`]）：
/// 托管口绑好、事实写好 ⇒ 说起好了；起不来 ⇒ 原话写进去、退 2。跑完 ⇒ 子进程的码（没有码 ⇒ 1）。
pub(super) async fn main(args: &[String], own: &Own) -> i32 {
    let done = match parse(args) {
        Ok(spec) => serve(spec, own, |_| crate::platform::child::say_ready()).await,
        Err(s) => Err(s),
    };
    match done {
        Ok(code) => code.unwrap_or(1),
        Err(s) => {
            crate::platform::child::say_not_up(&format!("ccm: {}", s.logged()));
            2
        }
    }
}

/// 托管进程那一形的整个身子（二进制入口认出 `-- --host-run …` 就交到这里，不进后端那个按核数开工作线程的运行时）：
/// 在自己那一个单线程运行时（[`runtime`]）上跑完 [`main`]，交回进程退出码。
/// 跑完不等阻塞池里还没回来的那几条（孙进程攥着终端时读伪终端那条回不来）：家已收、码已定，进程这就退。
pub fn process(args: &[String], own: &Own) -> i32 {
    // 远端 musl 版的分配器放回来的空页当场还给系统（同后端那一形的入口）。
    crate::platform::return_freed_pages_at_once();
    match runtime() {
        Ok(rt) => {
            let code = rt.block_on(main(args, own));
            rt.shutdown_background();
            code
        }
        Err(e) => {
            crate::platform::child::say_not_up(&format!("ccm: {e}"));
            2
        }
    }
}

/// 托管进程里各条线的名字（`ps -L` · `top -H` 里认得出）。
pub(crate) const THREAD_NAME: &str = "ccm-host";

/// 托管进程的运行时：单线程（跑它的就是调它的那一条）；阻塞池按需起线、闲了收，上限照这台的线程额度算（[`blocking_cap`]）。
pub(crate) fn runtime() -> std::io::Result<tokio::runtime::Runtime> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .thread_name(THREAD_NAME)
        .max_blocking_threads(blocking_cap(nproc_here(), pids_max_here()))
        .build()
}

/// 阻塞池上限的下限：池里跑的是写伪终端那一条（同一时刻只有一条），少于它字就送不进去。
pub(crate) const CAP_FLOOR: usize = 1;
/// 线程额度一样都读不到时的上限（保守值：写那一条之外只多留一条）。
pub(crate) const CAP_UNKNOWN: usize = CAP_FLOOR + 1;
/// 上限的顶：池里的活就那一样，额度再大也不多开。
pub(crate) const CAP_CEIL: usize = 4;
/// 只用线程额度的一小份：每这么多条额度给一条。
pub(crate) const CAP_SHARE: u64 = 1024;

/// 阻塞池上限：`RLIMIT_NPROC` 软上限与 cgroup `pids.max` 里小的那一样（没限的不算）的一小份（[`CAP_SHARE`]），
/// 夹在 [`CAP_FLOOR`] 与 [`CAP_CEIL`] 之间；两样都读不到 ⇒ [`CAP_UNKNOWN`]。
pub(crate) fn blocking_cap(nproc: Option<u64>, pids_max: Option<u64>) -> usize {
    match nproc.into_iter().chain(pids_max).min() {
        None => CAP_UNKNOWN,
        Some(q) => usize::try_from(q / CAP_SHARE)
            .unwrap_or(CAP_CEIL)
            .clamp(CAP_FLOOR, CAP_CEIL),
    }
}

/// `/proc/self/limits` 里 `Max processes` 那一行的软上限（`unlimited` ⇒ 没限）。
pub(crate) fn nproc_of_limits(text: &str) -> Option<u64> {
    let row = text.lines().find_map(|l| l.strip_prefix("Max processes"))?;
    row.split_whitespace().next()?.parse().ok()
}

/// cgroup v2 的 `pids.max`（`max` ⇒ 没限）。
pub(crate) fn pids_max_of(text: &str) -> Option<u64> {
    text.trim().parse().ok()
}

/// `/proc/self/cgroup` 里 cgroup v2 那一行（`0::<路径>`）的路径。
pub(crate) fn cgroup_path_of(text: &str) -> Option<&str> {
    text.lines().find_map(|l| l.strip_prefix("0::"))
}

/// 这台的线程额度从哪读：只 Linux 有（`/proc` · cgroup v2）；别的平台两样都读不到 ⇒ [`blocking_cap`] 取保守值。
#[cfg(target_os = "linux")]
use quota::{nproc_here, pids_max_here};

#[cfg(target_os = "linux")]
mod quota {
    use super::{cgroup_path_of, nproc_of_limits, pids_max_of};
    use crate::platform::fs::Ro;

    /// 读线程额度那几份的上限（都是一两 KiB 的内核文本）。
    const QUOTA_FILE_CAP: cap_core::Cap = cap_core::Cap {
        bytes: 16 * 1024,
        over: cap_core::Over::Reject,
    };

    fn text(p: impl AsRef<std::path::Path>) -> Option<String> {
        Ro::read_string_capped(p, QUOTA_FILE_CAP).ok()
    }

    /// 这个进程的 `RLIMIT_NPROC` 软上限。
    pub(super) fn nproc_here() -> Option<u64> {
        nproc_of_limits(&text("/proc/self/limits")?)
    }

    /// 这个进程所在 cgroup 一路往上各层 `pids.max` 里最小的（不是 cgroup v2 · 各层都没限 ⇒ 没有）。
    pub(super) fn pids_max_here() -> Option<u64> {
        let root = std::path::Path::new("/sys/fs/cgroup");
        let own = root.join(cgroup_path_of(&text("/proc/self/cgroup")?)?.trim_start_matches('/'));
        own.ancestors()
            .take_while(|d| d.starts_with(root))
            .filter_map(|d| pids_max_of(&text(d.join("pids.max"))?))
            .min()
    }
}

/// 不是 Linux：读不到 `RLIMIT_NPROC`（照实答「不知道」）。
#[cfg(not(target_os = "linux"))]
fn nproc_here() -> Option<u64> {
    None
}

/// 不是 Linux：没有 cgroup `pids.max`（照实答「不知道」）。
#[cfg(not(target_os = "linux"))]
fn pids_max_here() -> Option<u64> {
    None
}

/// 写伪终端那一头：要写的排成一队；有字要写才借阻塞池一条线写（[`drain`]），写空了把写口还回来。
struct PtyIn {
    q: VecDeque<Vec<u8>>,
    /// 写口：写的那条线写的时候拿在手里，写空了放回来；写坏了就没了。
    writer: Option<Box<dyn Write + Send>>,
    /// 此刻有一条线在写（或已交池去起）：再来的字只排队，由它取走。
    writing: bool,
    /// 写失败过（子进程那头关了）：往后的字不排了。
    broken: bool,
}

/// 写那条线：拿着写口一批一批取队里的写，取空了把写口放回去、落下 `writing` 就回（线还给阻塞池）。
/// 顺序照排队的先后：只有拿到写口的那一条在写（写口只有一个）。拿不到 ⇒ 别的一条正拿着、它会取走队里的，这一条什么都不动就回。
fn drain(pty: &Mutex<PtyIn>) {
    let Some(mut w) = locked(pty).writer.take() else {
        return;
    };
    loop {
        let batch: Vec<Vec<u8>> = {
            let mut p = locked(pty);
            if p.q.is_empty() {
                p.writer = Some(w);
                p.writing = false;
                return;
            }
            p.q.drain(..).collect()
        };
        for b in batch {
            if w.write_all(&b).and_then(|()| w.flush()).is_err() {
                let mut p = locked(pty);
                p.broken = true;
                p.writing = false;
                p.q.clear();
                return;
            }
        }
    }
}

/// 几条线 · 几个任务共用的那一份。
struct Ctx {
    hub: Mutex<Hub>,
    term: Terminal,
    /// 写伪终端那一头（[`PtyIn`]）。
    pty_in: Arc<Mutex<PtyIn>>,
    /// 起写那条线用的运行时（读伪终端那条线上也要起）。
    rt: tokio::runtime::Handle,
    /// 收尾时取走、删掉。
    home: Mutex<Option<Home>>,
    facts: Mutex<Facts>,
    /// 各端读那一半的任务（`bye` · 坏帧 ⇒ 掐掉）。
    readers: Mutex<BTreeMap<u64, tokio::task::AbortHandle>>,
}

fn locked<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

impl Ctx {
    fn apply(&self, effects: Vec<Effect>) {
        for e in effects {
            match e {
                Effect::Write(b) => self.write(b),
                Effect::Resize(s) => {
                    if let Err(e) = self.term.resize(s.rows, s.cols) {
                        tracing::warn!("托管终端：改尺寸没改成：{e}");
                    }
                }
                Effect::End => self.term.hangup(),
                Effect::Tag { sid, agent } => self.update_facts(|f| {
                    f.sid = sid;
                    if !agent.is_empty() {
                        f.agent = agent;
                    }
                }),
                // 只认托管终端的子进程（里面那个 ccm 自己）交来的那一跳。
                Effect::Hop { by, pid, wrapped } => {
                    if locked(&self.facts).child_pid == Some(by) {
                        self.update_facts(|f| {
                            f.hop_pid = Some(pid);
                            f.hop_wrapped = wrapped;
                        });
                    } else {
                        tracing::warn!(
                            "托管终端：pid {by} 不是子进程，它交来的那一跳（pid {pid}）不记"
                        );
                    }
                }
                Effect::Attached(a) => self.update_facts(|f| f.attached = a),
                Effect::Drop(id) => {
                    let more = locked(&self.hub).gone(id);
                    if let Some(h) = locked(&self.readers).remove(&id) {
                        h.abort();
                    }
                    self.apply(more);
                }
            }
        }
    }

    /// 排进写伪终端那一队；没有线在写 ⇒ 借阻塞池一条起来写（[`drain`]）。
    fn write(&self, b: Vec<u8>) {
        let start = {
            let mut p = locked(&self.pty_in);
            if p.broken {
                tracing::warn!("托管终端：写伪终端那一头已经坏了，这几个字节没送进去");
                return;
            }
            p.q.push_back(b);
            !std::mem::replace(&mut p.writing, true)
        };
        if start {
            let pty = Arc::clone(&self.pty_in);
            // 这台一时起不了线（线程额度满了）时池会 panic：不让它打掉调这一下的那条线（常是读伪终端那条）——
            // 字留在队里、落下 `writing`；那份活已进池的队，池里一有线就跑它，下一次来字也会再起一份（拿不到写口的那份空回）。
            let spawned = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                drop(self.rt.spawn_blocking(move || drain(&pty)));
            }));
            if spawned.is_err() {
                locked(&self.pty_in).writing = false;
                tracing::warn!(
                    "托管终端：起不了写伪终端那条线（线程额度满了？），字先排着，下一次来字再起"
                );
            }
        }
    }

    /// 改一格事实、整份写回（家已收尾 ⇒ 只改内存里那份）。
    fn update_facts(&self, change: impl FnOnce(&mut Facts)) {
        let mut f = locked(&self.facts);
        change(&mut f);
        if let Some(home) = locked(&self.home).as_ref() {
            if let Err(e) = home.write_facts(&f) {
                tracing::warn!("托管终端：写事实没写成：{}", e.logged());
            }
        }
    }
}

/// 跑一个托管终端，直到子进程退出、收完尾，交回子进程的退出码（被信号收掉 ⇒ `None`）。`ready` 在托管口绑好、事实写好之后调一次
/// （交套接字路径；判据靠它知道能连了）。
pub(crate) async fn serve(
    spec: Spec,
    own: &Own,
    ready: impl FnOnce(&std::path::Path),
) -> Result<Option<i32>, Said> {
    super::home::sweep_stale(own);
    let home = Home::claim(own, &spec.id)?;
    // 托管口先绑、再起子进程：子进程一存在口就连得上（里面的 claude 写 pidfile、后端来打标时不扑空）。
    // 绑好到开始收之间连上来的在内核的队里等，收是在事实写好之后 ⇒ 打标写的 sid 不会被起步那一份事实盖掉。
    let listener = match own_chan::bind(home.sock()) {
        Ok(l) => l,
        Err(e) => {
            let said = Said::with_raw(
                copy_text(
                    "beHost.run.bindFailed",
                    &[("path", &home.sock().display().to_string())],
                ),
                &e,
            );
            home.clear();
            return Err(said);
        }
    };
    #[expect(
        clippy::disallowed_methods,
        reason = "托管终端在自己的伪终端里起 ccm 交来的那一行（`--host-run` 之后的 argv，不过 shell）：同 tmux 在窗格里起它，里面的程序写什么归它自己"
    )]
    let mut child = crate::platform::child::Child::new(&spec.argv[0])
        .args(&spec.argv[1..])
        .env(HOST_ENV, &spec.id);
    for (k, v) in CHILD_ENV {
        child = child.env(k, v);
    }
    // 不往下传 `TMUX` · `TMUX_PANE`：里面不是 tmux。
    child = crate::common::tmux_utf8::unnested(child);
    if !spec.cwd.is_empty() {
        child = child.current_dir(&spec.cwd);
    }
    let (term, io, waiter) = match child.in_terminal(spec.size.rows, spec.size.cols) {
        Ok(t) => t,
        Err(e) => {
            home.clear();
            return Err(copy_text(
                "beHost.run.spawnFailed",
                &[("program", &spec.argv[0]), ("why", &e.to_string())],
            )
            .into());
        }
    };
    let facts = Facts {
        id: spec.id.clone(),
        name: spec.name.clone(),
        host_pid: std::process::id(),
        started_ms: crate::common::time::now_ms() as i64,
        child_pid: term.pid(),
        hop_pid: None,
        hop_wrapped: false,
        cwd: spec.cwd.clone(),
        agent: spec.agent.clone(),
        client: spec.client.clone(),
        account: spec.account.clone(),
        attached: false,
        sid: String::new(),
    };
    home.write_facts(&facts)?;

    let sock = home.sock().to_path_buf();
    // 读伪终端 · 等子进程这两条常驻（子进程不退就一直不回）：自己的线，不挂在运行时的阻塞池上 ——
    // 挂在池上，丢运行时那一下要等它们回来，子进程还活着就永远等下去（判据 `a_runtime_hosting_a_terminal_drops_while_the_child_still_waits`）。
    // 写伪终端那一条有字才起、写空就回，走池（按需起、闲了收、有上限）；子进程不读输入、它卡在写上时，丢运行时那一下会等它
    // （托管进程的入口 [`process`] 收场用 `shutdown_background`，不等）。
    let ctx = Arc::new(Ctx {
        hub: Mutex::new(Hub::new(&spec.id, spec.size)),
        term,
        // 写伪终端：一条队，有字才起线写（[`Ctx::write`]），写满了只堵那条线。
        pty_in: Arc::new(Mutex::new(PtyIn {
            q: VecDeque::new(),
            writer: Some(io.writer),
            writing: false,
            broken: false,
        })),
        rt: tokio::runtime::Handle::current(),
        home: Mutex::new(Some(home)),
        facts: Mutex::new(facts),
        readers: Mutex::new(BTreeMap::new()),
    });

    // 读伪终端：阻塞读，读到就算进屏、交各端；读到头 ⇒ 交 `eof`。
    let (eof_tx, eof_rx) = tokio::sync::oneshot::channel::<()>();
    {
        let ctx = Arc::clone(&ctx);
        let mut reader = io.reader;
        std::thread::spawn(move || {
            let mut buf = vec![0u8; 64 * 1024];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        let eff = locked(&ctx.hub).pty_out(&buf[..n]);
                        ctx.apply(eff);
                    }
                }
            }
            if eof_tx.send(()).is_err() {
                tracing::debug!("托管终端：读到头时收尾那一方已经不等了");
            }
        });
    }
    // 等子进程。
    let (exit_tx, exit_rx) = tokio::sync::oneshot::channel::<Option<i32>>();
    std::thread::spawn(move || {
        if exit_tx.send(waiter.wait()).is_err() {
            tracing::debug!("托管终端：子进程退了，收尾那一方已经不在了");
        }
    });
    // 收连接：只收本人的（`own-chan` 核对端 uid）。
    let writers: Arc<Mutex<Vec<tokio::task::JoinHandle<()>>>> = Arc::default();
    let accept = {
        let ctx = Arc::clone(&ctx);
        let writers = Arc::clone(&writers);
        tokio::spawn(async move {
            loop {
                let s = match listener.accept().await {
                    Ok(own_chan::Accepted::Ours(s)) => s,
                    Ok(own_chan::Accepted::Foreign(uid)) => {
                        tracing::warn!("托管口：拒了一条别的账号的连接（uid {uid:?}）");
                        continue;
                    }
                    Err(e) => {
                        tracing::warn!("托管口：收连接失败：{e}");
                        continue;
                    }
                };
                let w = serve_conn(&ctx, s);
                locked(&writers).push(w);
            }
        })
    };
    ready(&sock);

    let code = exit_rx.await.ok().flatten();
    // 它最后写的那几行先读完再报结束（读不到头 ⇒ 有逃出组的孙进程攥着终端：给一小段宽限就不等了）。
    wait_briefly(eof_rx, LAST_LINES_MS).await;
    accept.abort();
    locked(&ctx.hub).child_exited(code);
    let pending: Vec<_> = std::mem::take(&mut *locked(&writers));
    for w in pending {
        wait_briefly(w, LAST_WRITE_MS).await;
    }
    for (_, h) in std::mem::take(&mut *locked(&ctx.readers)) {
        h.abort();
    }
    // 读伪终端那条线可能还攥着 `ctx`（孙进程攥着终端）：家照收（目录删掉、锁放掉）。
    if let Some(home) = locked(&ctx.home).take() {
        home.clear();
    }
    Ok(code)
}

/// 子进程退了之后，等它最后那几行读完的宽限（毫秒）。
const LAST_LINES_MS: u64 = 500;
/// 各端把 `ended` 写完的宽限（毫秒）。
const LAST_WRITE_MS: u64 = 2000;

/// 收尾时的有界等待：等的东西一直不来就不等了（不是节拍，只等一次）。
#[expect(
    clippy::disallowed_methods,
    reason = "托管终端收尾时等最后那几行 · 各端写完 ended 的一次有界宽限：孙进程攥着终端或某一端不读时不能一直挂着"
)]
async fn wait_briefly<F: std::future::Future>(f: F, within_ms: u64) {
    if tokio::time::timeout(std::time::Duration::from_millis(within_ms), f)
        .await
        .is_err()
    {
        tracing::info!("托管终端收尾：等了 {within_ms} 毫秒没等到，不等了");
    }
}

/// 接一条连接：登记进账、起读写两个任务；交回写那一半的任务（收尾时等它把 `ended` 写完）。
fn serve_conn(ctx: &Arc<Ctx>, s: own_chan::Stream) -> tokio::task::JoinHandle<()> {
    let pid = own_chan::peer_pid(&s).ok();
    let (id, out) = locked(&ctx.hub).connect(pid, crate::common::time::now_ms() as i64);
    let (mut rd, mut wr) = tokio::io::split(s);
    let reader = {
        let ctx = Arc::clone(ctx);
        tokio::spawn(async move {
            while let Ok(Some(f)) = wire::read_frame(&mut rd).await {
                let eff = locked(&ctx.hub).frame(id, f, crate::common::time::now_ms() as i64);
                ctx.apply(eff);
            }
            let eff = locked(&ctx.hub).gone(id);
            ctx.apply(eff);
        })
    };
    locked(&ctx.readers).insert(id, reader.abort_handle());
    tokio::spawn(async move { write_out(&out, &mut wr, frame_window).await })
}

/// 合帧的窗口（毫秒）：一端排着的只有 `out` 时，写那一半先等这么久再取，这段时间里来的 `out` 并成一帧（约每秒 60 帧）。
pub(crate) const FRAME_MS: u64 = 16;

/// 合帧那一下的等：有 `out` 要写才等一次、等一个窗口（不是节拍：没输出就不醒）。
#[expect(
    clippy::disallowed_methods,
    reason = "托管终端合帧：一端排着的只有输出时等一个窗口再取，把这段里的多次重绘并成一帧；没输出不等、不醒"
)]
async fn frame_window() {
    tokio::time::sleep(std::time::Duration::from_millis(FRAME_MS)).await;
}

/// 写那一半：等铃；每取一次之前，排着的只有 `out` ⇒ 先等 `window`（生产交 [`frame_window`]），有控制帧不等；取队列、写；
/// 队列关了且取空 ⇒ 关这条连接。写的那段时间里又来的 `out` 也照等一个窗口，于是每个窗口最多一帧 `out`。
async fn write_out<W, F>(out: &Outbox, wr: &mut W, window: impl Fn() -> F)
where
    W: tokio::io::AsyncWrite + Unpin,
    F: std::future::Future<Output = ()>,
{
    loop {
        out.bell.notified().await;
        loop {
            if locked(&out.q).only_out() {
                window().await;
            }
            let (frames, closed) = locked(&out.q).drain();
            if frames.is_empty() {
                if closed {
                    if let Err(e) = wr.shutdown().await {
                        tracing::debug!("托管口：关一条连接没关干净：{e}");
                    }
                    return;
                }
                break;
            }
            for f in frames {
                if wr.write_all(&f).await.is_err() {
                    return;
                }
            }
        }
    }
}

/// 起托管终端的夹具（沙箱数据目录 · 假子进程 · `start` · `hello` · 读到哪一句）：本包判据与上层判据（经 `test-support`）共用。
#[cfg(any(test, feature = "test-support"))]
#[path = "../../../../tests/backend/host/run_support.rs"]
#[expect(
    clippy::let_underscore_must_use,
    reason = "测试档的夹具：只在测试档与上层测试档（`test-support`）编；清上一趟留下的沙箱目录 · 回报绑好的口时丢结果，同各包测试段放开的那一样"
)]
#[expect(
    clippy::disallowed_methods,
    reason = "测试档的夹具建 · 清沙箱数据目录（同各包测试段放开碰盘）"
)]
pub mod test_support;

#[cfg(test)]
#[path = "../../../../tests/backend/host/run_tests.rs"]
mod tests;
