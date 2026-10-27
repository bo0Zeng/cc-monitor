//! 托管进程本体：`ccm -- --host-run <id> [选项] -- <argv>`（A3-3-1）。
//!
//! 持一个伪终端（里面跑 `argv`），把它的输出算进屏（[`super::screen`]），经托管口（[`super::wire`] · [`super::serve`]）给各端；
//! 输入只收持有者的。子进程退了 ⇒ 各端收 `ended{code}`、删自己的目录、退出（10-05：Claude 退出终端跟着收）。
//! 收到 `end` ⇒ 子进程组 SIGHUP，等它退。
//!
//! 线程：读伪终端一条（阻塞读，永远不等哪一端）· 写伪终端一条（写满了只堵它自己）· 等子进程一条；
//! 收连接与各端的读写是 tokio 任务。那一张账（[`Hub`]）在一把锁里，谁改它谁当场把要做的事做掉（[`Ctx::apply`]）。

use super::home::{Facts, Home};
use super::serve::{Effect, Hub, Outbox};
use super::wire::{self, Frame, Size};
use crate::platform::child::Terminal;
use crate::platform::fs::Own;
use copy_core::copy_text;
use copy_core::said::Said;
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::sync::{Arc, Mutex, MutexGuard};
use tokio::io::AsyncWriteExt;

/// 后端词。
pub(crate) const WORD: &str = "--host-run";
/// 没给尺寸时的样子。
pub(crate) const DEFAULT_SIZE: Size = Size { rows: 24, cols: 80 };

/// 托管进程里那个子进程的环境：`CCM_HOST` 让里面的 ccm 写便条时带上宿主（A3-3-4）；终端型号照 tmux 的 `default-terminal` 那一类给。
pub(crate) const HOST_ENV: &str = "CCM_HOST";
const CHILD_ENV: [(&str, &str); 2] = [("TERM", "xterm-256color"), ("COLORTERM", "truecolor")];
/// 不往下传：里面不是 tmux。
const CHILD_ENV_REMOVED: [&str; 2] = ["TMUX", "TMUX_PANE"];

/// 起一个托管终端要的那几样。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Spec {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) size: Size,
    pub(crate) cwd: String,
    pub(crate) agent: String,
    pub(crate) client: String,
    pub(crate) argv: Vec<String>,
}

/// `<id> [--name N] [--size 列x行] [--cwd D] [--agent A] [--client C] -- <argv…>`。
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
        client: String::new(),
        argv: Vec::new(),
    };
    while let Some(a) = it.next() {
        if a == "--" {
            spec.argv = it.cloned().collect();
            break;
        }
        let v = it.next().cloned().ok_or_else(|| bad(a))?;
        match a.as_str() {
            "--name" if super::home::id_ok(&v) => spec.name = v,
            "--size" => spec.size = size_of(&v).ok_or_else(|| bad("--size"))?,
            "--cwd" => spec.cwd = v,
            "--agent" => spec.agent = v,
            "--client" => spec.client = v,
            other => return Err(bad(other)),
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

/// 后端词的入口：解参数、跑，交回进程退出码（起不来 2，跑完 0）。
pub(crate) async fn main(args: &[String], own: &Own) -> i32 {
    let done = match parse(args) {
        Ok(spec) => run(spec, own, |_| {}).await,
        Err(s) => Err(s),
    };
    match done {
        Ok(()) => 0,
        Err(s) => {
            eprintln!("ccm: {}", s.logged());
            2
        }
    }
}

/// 几条线 · 几个任务共用的那一份。
struct Ctx {
    hub: Mutex<Hub>,
    term: Terminal,
    /// 写伪终端那条线的队。
    pty_in: Mutex<std::sync::mpsc::Sender<Vec<u8>>>,
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
                Effect::Write(b) => {
                    let _ = locked(&self.pty_in).send(b);
                }
                Effect::Resize(s) => {
                    if let Err(e) = self.term.resize(s.rows, s.cols) {
                        tracing::warn!("托管终端：改尺寸没改成：{e}");
                    }
                }
                Effect::End => self.term.hangup(),
                Effect::Tag { sid, agent } => {
                    let mut f = locked(&self.facts);
                    f.sid = sid;
                    if !agent.is_empty() {
                        f.agent = agent;
                    }
                    if let Some(home) = locked(&self.home).as_ref() {
                        if let Err(e) = home.write_facts(&f) {
                            tracing::warn!("托管终端：写事实没写成：{}", e.logged());
                        }
                    }
                }
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
}

/// 跑一个托管终端，直到子进程退出、收完尾。`ready` 在托管口绑好、事实写好之后调一次（交套接字路径；判据靠它知道能连了）。
pub(crate) async fn run(
    spec: Spec,
    own: &Own,
    ready: impl FnOnce(&std::path::Path),
) -> Result<(), Said> {
    let home = Home::claim(own, &spec.id)?;
    let mut child = crate::platform::child::Child::new(&spec.argv[0])
        .args(&spec.argv[1..])
        .env(HOST_ENV, &spec.id);
    for (k, v) in CHILD_ENV {
        child = child.env(k, v);
    }
    for k in CHILD_ENV_REMOVED {
        child = child.env_remove(k);
    }
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
        started_ms: crate::common::time::now_ms(),
        child_pid: term.pid(),
        cwd: spec.cwd.clone(),
        agent: spec.agent.clone(),
        client: spec.client.clone(),
        sid: String::new(),
    };
    home.write_facts(&facts)?;
    let listener = own_chan::bind(home.sock()).map_err(|e| {
        Said::with_raw(
            copy_text("beHost.run.bindFailed", &[("path", &home.sock().display().to_string())]),
            &e,
        )
    })?;

    let sock = home.sock().to_path_buf();
    let (wtx, wrx) = std::sync::mpsc::channel::<Vec<u8>>();
    let ctx = Arc::new(Ctx {
        hub: Mutex::new(Hub::new(&spec.id, spec.size)),
        term,
        pty_in: Mutex::new(wtx),
        home: Mutex::new(Some(home)),
        facts: Mutex::new(facts),
        readers: Mutex::new(BTreeMap::new()),
    });

    // 写伪终端：一条线、一条队，写满了只堵这条线。
    {
        let mut writer = io.writer;
        std::thread::spawn(move || {
            for b in wrx {
                if writer.write_all(&b).and_then(|()| writer.flush()).is_err() {
                    return;
                }
            }
        });
    }
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
            let _ = eof_tx.send(());
        });
    }
    // 等子进程。
    let (exit_tx, exit_rx) = tokio::sync::oneshot::channel::<Option<i32>>();
    std::thread::spawn(move || {
        let _ = exit_tx.send(waiter.wait());
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
    wait_briefly(eof_rx, std::time::Duration::from_millis(500)).await;
    accept.abort();
    locked(&ctx.hub).child_exited(code);
    let pending: Vec<_> = std::mem::take(&mut *locked(&writers));
    for w in pending {
        wait_briefly(w, std::time::Duration::from_secs(2)).await;
    }
    for (_, h) in std::mem::take(&mut *locked(&ctx.readers)) {
        h.abort();
    }
    // 读伪终端那条线可能还攥着 `ctx`（孙进程攥着终端）：家照收（目录删掉、锁放掉）。
    if let Some(home) = locked(&ctx.home).take() {
        home.clear();
    }
    Ok(())
}

/// 收尾时的有界等待：等的东西一直不来就不等了（不是节拍，只等一次）。
#[allow(
    clippy::disallowed_methods,
    reason = "托管终端收尾时等最后那几行 · 各端写完 ended 的一次有界宽限：孙进程攥着终端或某一端不读时不能一直挂着"
)]
async fn wait_briefly<F: std::future::Future>(f: F, within: std::time::Duration) {
    let _ = tokio::time::timeout(within, f).await;
}

/// 接一条连接：登记进账、起读写两个任务；交回写那一半的任务（收尾时等它把 `ended` 写完）。
fn serve_conn(ctx: &Arc<Ctx>, s: own_chan::Stream) -> tokio::task::JoinHandle<()> {
    let pid = s.peer_cred().ok().and_then(|c| c.pid()).and_then(|p| u32::try_from(p).ok());
    let (id, out) = locked(&ctx.hub).connect(pid, crate::common::time::now_ms());
    let (mut rd, mut wr) = tokio::io::split(s);
    let reader = {
        let ctx = Arc::clone(ctx);
        tokio::spawn(async move {
            loop {
                match wire::read_frame(&mut rd).await {
                    Ok(Some(f)) => {
                        let eff = locked(&ctx.hub).frame(id, f, crate::common::time::now_ms());
                        ctx.apply(eff);
                    }
                    Ok(None) | Err(_) => break,
                }
            }
            let eff = locked(&ctx.hub).gone(id);
            ctx.apply(eff);
        })
    };
    locked(&ctx.readers).insert(id, reader.abort_handle());
    tokio::spawn(async move { write_out(&out, &mut wr).await })
}

/// 写那一半：等铃、取队列、写；队列关了且取空 ⇒ 关这条连接。
async fn write_out<W: tokio::io::AsyncWrite + Unpin>(out: &Outbox, wr: &mut W) {
    loop {
        out.bell.notified().await;
        loop {
            let (frames, closed) = locked(&out.q).drain();
            if frames.is_empty() {
                if closed {
                    let _ = wr.shutdown().await;
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

#[cfg(test)]
#[path = "../../../../tests/backend/host/run_tests.rs"]
mod tests;
