//! **一组一条**那几条命令（同一台上的复制 · 删除 · 改权限，两台之间的复制）共用的：票 · 进度 · 撤 · 同名怎么答 · 结局那一行。
//!
//! - **票**：带了票 ⇒ 这台按票记这一组。连接断了照常做完；带同一张票再问 ⇒ 正在做就等它做完，做完了回那一份结局，不重做。
//!   记最多 [`KEEP`] 张，满了丢最早做完的。还没开做就回了的（同名要问 · 入参不对）不记。
//! - **进度**：按票有人听（[`listen`]，帧面那一层每条连接挂一个）就把 `{done, total, current, bytes}` 交过去；交法由听的那一侧定。
//!   `bytes` 只有搬字节的那一种（两台之间的复制）报：整组按字节走到哪了，一份大文件传到一半也动。
//! - **撤**：[`stop`] 立旗；做的那一侧做完手上那一项就停，剩下的回 `cancelled`。
//!
//! 住 `common/`：文件管理那一块（同台三条）与原生那一块（两台之间的复制）各引各的，两块之间不互相引。

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};

use cmd_wire::batch::{BatchOutcome, BatchReply, Decide, ItemState};
use cmd_wire::files as wire;
use copy_core::copy_text;

/// 最多记几张票。
pub(crate) const KEEP: usize = 64;
/// 票的上限长度（界面给的是一个 UUID）。
const TICKET_MAX: usize = 64;

/// 一组此刻做到哪了。
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Progress {
    pub(crate) ticket: String,
    pub(crate) done: u64,
    pub(crate) total: u64,
    /// 手上那一项（根底下的相对段，线上那一形）。
    pub(crate) current: Option<wire::WirePath>,
    /// 整组按字节：已传 · 一共（只有搬字节的那一种报）。
    pub(crate) bytes: Option<(u64, u64)>,
}

/// 按票记的一组。
struct Job {
    stop: AtomicBool,
    progress: Mutex<Progress>,
    end: Mutex<Option<BatchReply>>,
    ended: Condvar,
}

struct Book {
    jobs: Mutex<VecDeque<(String, Arc<Job>)>>,
    ears: Mutex<Vec<Ear>>,
}

/// 听一张票的进度的一位。
struct Ear {
    id: u64,
    ticket: String,
    tell: Box<dyn Fn(&Progress) + Send + Sync>,
}

static BOOK: Book = Book {
    jobs: Mutex::new(VecDeque::new()),
    ears: Mutex::new(Vec::new()),
};
static EAR_SEQ: AtomicU64 = AtomicU64::new(0);

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

/// 票合不合格：非空、有界、只许字母数字与 `-`（它会进日志与进度帧）。
fn ticket_ok(t: &str) -> bool {
    !t.is_empty()
        && t.len() <= TICKET_MAX
        && t.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

/// 听着的那一位：落了就不听了。
pub(crate) struct Listening(u64);

impl Drop for Listening {
    fn drop(&mut self) {
        lock(&BOOK.ears).retain(|e| e.id != self.0);
    }
}

/// 听一张票的进度（那一组正在做 ⇒ 当场先交一次此刻的样子）。
pub(crate) fn listen(ticket: &str, tell: Box<dyn Fn(&Progress) + Send + Sync>) -> Listening {
    let id = EAR_SEQ.fetch_add(1, Ordering::Relaxed);
    if let Some(job) = find(ticket) {
        if lock(&job.end).is_none() {
            tell(&lock(&job.progress).clone());
        }
    }
    lock(&BOOK.ears).push(Ear {
        id,
        ticket: ticket.to_string(),
        tell,
    });
    Listening(id)
}

fn find(ticket: &str) -> Option<Arc<Job>> {
    lock(&BOOK.jobs)
        .iter()
        .find(|(k, _)| k == ticket)
        .map(|(_, j)| j.clone())
}

/// 撤一组：那一组还在做 ⇒ 立旗、回 `true`；没在做（做完了 · 没见过这张票）⇒ `false`。
pub(crate) fn stop(ticket: &str) -> bool {
    match find(ticket) {
        Some(job) if lock(&job.end).is_none() => {
            job.stop.store(true, Ordering::SeqCst);
            true
        }
        _ => false,
    }
}

/// 一组做的时候手里那一份：看旗 · 报进度。
pub(crate) struct Run {
    job: Option<Arc<Job>>,
    ticket: String,
    total: u64,
}

impl Run {
    pub(crate) fn stopped(&self) -> bool {
        self.job
            .as_ref()
            .is_some_and(|j| j.stop.load(Ordering::SeqCst))
    }

    pub(crate) fn step(&self, done: usize, current: Option<&Path>) {
        self.tell(|p| {
            p.done = done as u64;
            p.current = current.map(|p| wire::WirePath(p.as_os_str().as_encoded_bytes().to_vec()));
        });
    }

    /// 整组按字节走到哪了（项数 · 手上那一项不动）。
    pub(crate) fn sent(&self, done: u64, total: u64) {
        self.tell(|p| p.bytes = Some((done.min(total), total)));
    }

    fn tell(&self, change: impl FnOnce(&mut Progress)) {
        let Some(job) = &self.job else { return };
        let p = {
            let mut g = lock(&job.progress);
            change(&mut g);
            g.total = self.total;
            g.clone()
        };
        for e in lock(&BOOK.ears).iter().filter(|e| e.ticket == self.ticket) {
            (e.tell)(&p);
        }
    }
}

/// 带票做一组：`probe` 是开做之前那几问（入参 · 同名；不成 ⇒ 不记票、原样回）；`work` 逐项做、交回结局。
/// 见过这张票 ⇒ 正在做就等它做完；做完了 ⇒ 回那一份（两样都不跑）。
pub(crate) fn ticketed<P, E>(
    ticket: Option<&str>,
    total: usize,
    bad_ticket: impl FnOnce(String) -> E,
    probe: impl FnOnce() -> Result<P, E>,
    work: impl FnOnce(P, &Run) -> BatchReply,
) -> Result<BatchReply, E> {
    let Some(t) = ticket else {
        let p = probe()?;
        let run = Run {
            job: None,
            ticket: String::new(),
            total: total as u64,
        };
        return Ok(work(p, &run));
    };
    if !ticket_ok(t) {
        return Err(bad_ticket(crate::common::contract::malformed(
            "`ticket` must be 1-64 of [A-Za-z0-9-]",
        )));
    }
    let job = {
        let mut q = lock(&BOOK.jobs);
        if let Some((_, j)) = q.iter().find(|(k, _)| k == t) {
            let j = j.clone();
            drop(q);
            return Ok(wait(&j));
        }
        if q.len() >= KEEP {
            // 丢最早做完的那一张；都还在做 ⇒ 丢最早的（它还在做，只是再问不到了）。
            let at = q
                .iter()
                .position(|(_, j)| lock(&j.end).is_some())
                .unwrap_or(0);
            q.remove(at);
        }
        let j = Arc::new(Job {
            stop: AtomicBool::new(false),
            progress: Mutex::new(Progress {
                ticket: t.to_string(),
                done: 0,
                total: total as u64,
                current: None,
                bytes: None,
            }),
            end: Mutex::new(None),
            ended: Condvar::new(),
        });
        q.push_back((t.to_string(), j.clone()));
        j
    };
    let p = match probe() {
        Ok(p) => p,
        Err(f) => {
            lock(&BOOK.jobs).retain(|(_, j)| !Arc::ptr_eq(j, &job));
            return Err(f);
        }
    };
    let run = Run {
        job: Some(job.clone()),
        ticket: t.to_string(),
        total: total as u64,
    };
    run.step(0, None);
    let reply = work(p, &run);
    *lock(&job.end) = Some(reply.clone());
    job.ended.notify_all();
    Ok(reply)
}

fn wait(job: &Job) -> BatchReply {
    let mut end = lock(&job.end);
    loop {
        if let Some(r) = end.as_ref() {
            return r.clone();
        }
        end = job.ended.wait(end).unwrap_or_else(|p| p.into_inner());
    }
}

/// 一组里的一项，`decide` 给了它的那个答 ⇒ 照答；没给 ⇒ 照 `on_exists`（`ask` 而没答 ⇒ `None`：要先问）。
pub(crate) fn choice_of(
    key: &str,
    on_exists: cmd_wire::batch::OnExists,
    decide: Option<&std::collections::BTreeMap<String, Decide>>,
) -> Option<Decide> {
    if let Some(d) = decide.and_then(|m| m.get(key)) {
        return Some(*d);
    }
    match on_exists {
        cmd_wire::batch::OnExists::Ask => None,
        cmd_wire::batch::OnExists::Overwrite => Some(Decide::Overwrite),
        cmd_wire::batch::OnExists::Skip => Some(Decide::Skip),
        cmd_wire::batch::OnExists::KeepBoth => Some(Decide::KeepBoth),
    }
}

/// 同名清单里那两格时刻（文件的修改时刻，unix 秒）⇒ 写好的字：按此刻与看的那一台的时区（同额度与轮换那几处时刻的写法）。
pub(crate) fn moment_text(secs: Option<i64>, tz: &crate::Tz) -> Option<String> {
    secs.filter(|s| *s > 0)
        .map(|s| crate::common::time::fmt_at(s, crate::common::time::now_secs(), tz))
}

/// 盘上那一格的修改时刻（`metadata.modified()` 那一格）⇒ unix 秒。
pub(crate) fn mtime_secs(t: std::io::Result<std::time::SystemTime>) -> Option<i64> {
    t.ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .and_then(|d| i64::try_from(d.as_secs()).ok())
}

/// 一项的键：它在 `items` 里的序号。
pub(crate) fn key_of(i: usize) -> String {
    i.to_string()
}

/// 后端拼好的那一行：全成 ⇒ `已复制 5 项`（落在同一个目录 ⇒ `→ 目录`）；别的 ⇒ 逐样数出来。
pub(crate) fn said_of(verb: &str, outcomes: &[BatchOutcome], into: Option<&str>) -> String {
    let count = |s: &[ItemState]| {
        outcomes
            .iter()
            .filter(|o| s.contains(&o.state))
            .count()
            .to_string()
    };
    let n = outcomes.len().to_string();
    let ok = count(&[ItemState::Done]);
    if ok == n {
        return match into {
            Some(dir) => copy_text(
                "beFiles.batch.doneInto",
                &[("action", verb), ("n", &n), ("dir", dir)],
            ),
            None => copy_text("beFiles.batch.done", &[("action", verb), ("n", &n)]),
        };
    }
    let skip = count(&[ItemState::Skipped]);
    let bad = count(&[ItemState::Failed, ItemState::Exists]);
    let left = count(&[ItemState::Cancelled]);
    if left == "0" {
        return copy_text(
            "beFiles.batch.partial",
            &[
                ("action", verb),
                ("n", &n),
                ("ok", &ok),
                ("skip", &skip),
                ("bad", &bad),
            ],
        );
    }
    copy_text(
        "beFiles.batch.stopped",
        &[
            ("action", verb),
            ("n", &n),
            ("ok", &ok),
            ("skip", &skip),
            ("bad", &bad),
            ("left", &left),
        ],
    )
}

/// 撤了 ⇒ 剩下那几项都回 `cancelled`。
pub(crate) fn cancel_rest(out: &mut Vec<BatchOutcome>, from: usize, total: usize) {
    out.extend((from..total).map(|i| BatchOutcome::bare(key_of(i), ItemState::Cancelled)));
}

/// 保留两份：`名 (k).ext` 里 `k` 从 2 起。
pub(crate) fn minted(to: &Path, k: u32) -> PathBuf {
    let name = to
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let (stem, ext) = match name.rfind('.') {
        Some(i) if i > 0 => (&name[..i], &name[i..]),
        _ => (name.as_str(), ""),
    };
    to.with_file_name(format!("{stem} ({k}){ext}"))
}
