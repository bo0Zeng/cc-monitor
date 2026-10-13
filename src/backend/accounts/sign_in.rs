//! **登录一个号**（官方客户端自己登）：核心在后台起那一家的登录子命令（Claude：`claude auth login`），从它的输出里认出登录链接交给界面；
//! 人在浏览器里登完，把页面上给的码粘回来，核心原样写进那个子进程的 stdin。令牌由那一家自己换、自己写盘，**核心碰不到令牌**。
//!
//! - 作业表：每个号最多一份作业，新开就替掉旧的；放手即杀组（[`Conversation`]）。
//! - 不装定时器：作业只在这几种时刻收 —— 它自己退了（登成 / 登不成）· 取消 · 被替掉 · 发起它的那条连接断了（[`Desk`]）。
//!   等链接 · 等换码结果那两处是**发起方的有界等待**（[`START_WITHIN`] · [`FINISH_WITHIN`]），不是节拍。
//! - 粘回来的东西不进 argv · 日志 · 应答；链接里的 `state` 也不进日志。
//! - 帧那一侧递一个「结束了」回调（推一帧给发起它的那条连接、敲需手动的铃），本模块不认帧。

use crate::agents::SignInFace;
use crate::common::said::Said;
use crate::platform::child::{Child, Conversation, Heard};
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{mpsc, Mutex};
use std::time::Duration;

/// 起登录子命令之后等它打印链接，至多这么久（它要先问一次服务端拿登录参数）。
pub(crate) const START_WITHIN: Duration = Duration::from_millis(30_000);
/// 码写进去之后等它换完（成 · 不成 · 说码不对），至多这么久（它要去令牌端点换一次、再拉一次资料）。
pub(crate) const FINISH_WITHIN: Duration = Duration::from_millis(60_000);
/// 失败时原话里带它最后说的几行。
const TAIL_LINES: usize = 12;

/// 一份作业怎么收的（交给「结束了」回调；也是 `login-finish` 等到的那一件）。
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum SignInEnd {
    /// 登成了（那一家退出码 0）。
    Done,
    /// 登不成：那一句 ＋ 它最后说的几行（原话）。
    Failed(Said),
}

/// 「结束了」回调：作业号 · 怎么收的。只在作业自己退了时调（取消 · 替掉 · 连接断不调）。
pub(crate) type OnEnd = Box<dyn Fn(&str, &SignInEnd) + Send>;

/// 起一份作业要的：那一家的登录形 · 号（名 ＋ 配置目录；账号 0 ⇒ 无目录）· 发起它的那条连接。
pub(crate) struct SignInAsk<'a> {
    pub(crate) face: SignInFace,
    pub(crate) account: &'a str,
    pub(crate) dir: Option<&'a str>,
    pub(crate) conn: u64,
    /// 起它时的工作目录（家）。
    pub(crate) cwd: Option<std::path::PathBuf>,
    /// 判据换 `PATH`（假的登录子命令放在临时目录里）；生产 ⇒ `None`（照这台的环境找）。
    pub(crate) path: Option<OsString>,
}

/// 起好了：作业号 ＋ 登录链接。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Started {
    pub(crate) job: String,
    pub(crate) url: String,
}

/// `finish` 不成。
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum FinishFail {
    /// 没有这份作业（收了 · 被替掉 · 从没有过）。
    NoJob,
    /// 粘的东西格式不对，或那一家说码不对：作业还在，可以再粘。
    BadPaste,
    /// 那一家换码没成（作业收了，要重来）：那一句 ＋ 原话。
    Refused(Said),
    /// 等了 [`FINISH_WITHIN`] 还没答（作业还在）。
    Slow,
}

struct Job {
    conn: u64,
    account: String,
    conv: Conversation,
    face: SignInFace,
    url: String,
    /// `finish` 正在等结局 ⇒ 读线程把结局交给它。
    waiter: Option<mpsc::Sender<Waited>>,
}

enum Waited {
    SignInEnd(SignInEnd),
    BadPaste,
}

static JOBS: Mutex<BTreeMap<String, Job>> = Mutex::new(BTreeMap::new());

fn jobs() -> std::sync::MutexGuard<'static, BTreeMap<String, Job>> {
    JOBS.lock().unwrap_or_else(|p| p.into_inner())
}

/// 作业号：不可猜（进程内计数 ＋ 系统随机），也不带号名。
fn new_job_id() -> String {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    use std::hash::{BuildHasher, Hasher};
    let mut h = std::collections::hash_map::RandomState::new().build_hasher();
    h.write_u64(n);
    let salt = h.finish();
    format!("login-{n}-{salt:016x}")
}

/// 起一份作业：起那一家的登录子命令，等它打印链接（至多 [`START_WITHIN`]）。同一个号已有一份 ⇒ 先替掉（杀它）。
/// 起不来 · 没给出链接就退了 · 等不到链接 ⇒ `Err(那一句 ＋ 原话)`。
pub(crate) fn start(a: &SignInAsk, on_end: OnEnd) -> Result<Started, Said> {
    let f = a.face;
    let mut child = Child::new(f.program).args(f.args);
    child = match a.dir {
        Some(d) => child.env(f.dir_env, d),
        None => child.env_remove(f.dir_env),
    };
    for k in f.env_remove {
        child = child.env_remove(k);
    }
    if let Some(p) = &a.path {
        child = child.env("PATH", p);
    }
    if let Some(d) = &a.cwd {
        child = child.current_dir(d);
    }
    let mut conv = child.converse().map_err(|e| {
        e.into_cmd_said("failed", |why| {
            copy_core::copy_text(
                "beLogin.start.failed",
                &[("program", f.program), ("why", why)],
            )
        })
        .1
    })?;
    let heard = conv
        .take_heard()
        .ok_or_else(|| Said::from(copy_core::copy_text("beLogin.start.noLink", &[])))?;
    let job = new_job_id();
    let (link_tx, link_rx) = mpsc::channel::<Result<String, String>>();
    // 先替掉同一个号的旧作业（它的读线程看到作业不在了就不出声）。
    let old: Vec<Job> = {
        let mut t = jobs();
        let ids: Vec<String> = t
            .iter()
            .filter(|(_, j)| j.account == a.account)
            .map(|(id, _)| id.clone())
            .collect();
        ids.into_iter().filter_map(|id| t.remove(&id)).collect()
    };
    drop(old);
    jobs().insert(
        job.clone(),
        Job {
            conn: a.conn,
            account: a.account.to_string(),
            conv,
            face: f,
            url: String::new(),
            waiter: None,
        },
    );
    {
        let job = job.clone();
        std::thread::spawn(move || listen(&job, f, &heard, link_tx, on_end));
    }
    let got = match link_rx.recv_timeout(START_WITHIN) {
        Ok(Ok(url)) => Ok(url),
        Ok(Err(tail)) => Err(Said::with_raw(
            copy_core::copy_text("beLogin.start.exited", &[]),
            &tail,
        )),
        Err(_) => {
            jobs().remove(&job);
            Err(Said::from(copy_core::copy_text(
                "beLogin.start.noLink",
                &[],
            )))
        }
    };
    let url = got?;
    match jobs().get_mut(&job) {
        Some(j) => j.url = url.clone(),
        None => {
            return Err(Said::from(copy_core::copy_text(
                "beLogin.start.exited",
                &[],
            )))
        }
    }
    Ok(Started { job, url })
}

/// 一份作业的读线程：认链接（交给 `start`）· 认「码不对」（交给正在等的 `finish`）· 它退了 ⇒ 收作业、交结局。
fn listen(
    job: &str,
    f: SignInFace,
    heard: &mpsc::Receiver<Heard>,
    link_tx: mpsc::Sender<Result<String, String>>,
    on_end: OnEnd,
) {
    let mut tail: std::collections::VecDeque<String> = std::collections::VecDeque::new();
    let mut linked = false;
    let push = |tail: &mut std::collections::VecDeque<String>, l: &str| {
        if tail.len() == TAIL_LINES {
            tail.pop_front();
        }
        tail.push_back(l.to_string());
    };
    let code = loop {
        match heard.recv() {
            Ok(Heard::Out(l) | Heard::Err(l)) => {
                if !linked {
                    if let Some(url) = (f.link_in)(&l) {
                        linked = true;
                        let _ = link_tx.send(Ok(url));
                        continue;
                    }
                }
                if (f.said_bad_paste)(&l) {
                    if let Some(w) = jobs().get_mut(job).and_then(|j| j.waiter.take()) {
                        let _ = w.send(Waited::BadPaste);
                    }
                }
                push(&mut tail, &l);
            }
            Ok(Heard::Exited(code)) => break code,
            Err(_) => break None,
        }
    };
    let tail: Vec<String> = tail.into_iter().collect();
    let tail = tail.join("\n");
    let still = jobs().remove(job);
    // 没给出链接就退了 ⇒ 告诉还在等链接的 `start`（它照实回「起不成」）。
    if !linked {
        let _ = link_tx.send(Err(tail));
        return;
    }
    // 作业已经不在（取消 · 替掉 · 连接断）⇒ 不出声。
    let Some(j) = still else { return };
    let ended = if code == Some(0) {
        SignInEnd::Done
    } else {
        SignInEnd::Failed(Said::with_raw(
            copy_core::copy_text("beLogin.finish.refused", &[]),
            &tail,
        ))
    };
    if let Some(w) = j.waiter {
        let _ = w.send(Waited::SignInEnd(ended.clone()));
    }
    drop(j.conv);
    on_end(job, &ended);
}

/// 把粘回来的东西交给那份作业，等那一家的结局（至多 [`FINISH_WITHIN`]）。格式对不上这一趟的链接 ⇒ 不写、`BadPaste`。
pub(crate) fn finish(job: &str, pasted: &str) -> Result<String, FinishFail> {
    let (tx, rx) = mpsc::channel::<Waited>();
    let account = {
        let mut t = jobs();
        let j = t.get_mut(job).ok_or(FinishFail::NoJob)?;
        let pasted = pasted.trim();
        if !(j.face.paste_fits)(pasted, &j.url) {
            return Err(FinishFail::BadPaste);
        }
        if j.conv.say(pasted).is_err() {
            return Err(FinishFail::NoJob);
        }
        j.waiter = Some(tx);
        j.account.clone()
    };
    match rx.recv_timeout(FINISH_WITHIN) {
        Ok(Waited::SignInEnd(SignInEnd::Done)) => Ok(account),
        Ok(Waited::SignInEnd(SignInEnd::Failed(s))) => Err(FinishFail::Refused(s)),
        Ok(Waited::BadPaste) => Err(FinishFail::BadPaste),
        Err(mpsc::RecvTimeoutError::Timeout) => {
            if let Some(j) = jobs().get_mut(job) {
                j.waiter = None;
            }
            Err(FinishFail::Slow)
        }
        Err(mpsc::RecvTimeoutError::Disconnected) => Err(FinishFail::NoJob),
    }
}

/// 取消一份作业（杀它）。回：有没有这一份。
pub(crate) fn cancel(job: &str) -> bool {
    let j = jobs().remove(job);
    j.is_some()
}

/// 一条连接发起的全部作业：连接断了就收（[`Desk`] 的 `Drop`）。
fn withdraw_conn(conn: u64) {
    let gone: Vec<Job> = {
        let mut t = jobs();
        let ids: Vec<String> = t
            .iter()
            .filter(|(_, j)| j.conn == conn)
            .map(|(id, _)| id.clone())
            .collect();
        ids.into_iter().filter_map(|id| t.remove(&id)).collect()
    };
    drop(gone);
}

/// 进程里那条「会话外的需手动清单可能变了」的铃（流的 tap 订它推 `changed {manual}`）：账号清单动了 · 一份登录作业收了都敲。
pub(crate) fn manual_changes() -> &'static tokio::sync::broadcast::Sender<()> {
    static MANUAL_BELL: std::sync::OnceLock<tokio::sync::broadcast::Sender<()>> =
        std::sync::OnceLock::new();
    MANUAL_BELL.get_or_init(|| tokio::sync::broadcast::channel::<()>(16).0)
}

/// 敲一下需手动的铃（没人订 ⇒ 什么都不做）。
pub(crate) fn ring_manual() {
    let _ = manual_changes().send(());
}

/// 一条连接上的登录作业：随连接的读循环一起死 ⇒ 它发起的作业一律收。
pub(crate) struct Desk {
    conn: u64,
}

impl Desk {
    /// `conn` ＝ 这条连接的号（与待答请求同一个号：`control::asks::Desk::conn`）。
    pub(crate) fn of(conn: u64) -> Desk {
        Desk { conn }
    }
}

impl Drop for Desk {
    fn drop(&mut self) {
        withdraw_conn(self.conn);
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/accounts/sign_in_tests.rs"]
mod tests;
