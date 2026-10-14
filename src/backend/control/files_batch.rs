//! **同一台上一组一条**（复制 · 删除 · 改权限）：探 · 问 · 做 · 逐项结局 · 票 · 进度 · 撤，判法只此一处。
//!
//! - **探 · 问**：复制先把整组的落点探一遍；同名的那几项没有答（`on_exists: ask` 而 `decide` 里没有它）⇒ 一项都不做，
//!   回码 `exists`、`data` 是同名清单（[`Conflicts`]）。界面问一次，带 `decide` 再发。
//! - **做**：逐项做，一项没成不算整组没成（那一项的结局是 `failed`，码 · 原因词 · 原话都在那一项上）。
//!   探完之后才冒出来的同名：照旧 `O_EXCL` 不盖，那一项回 `exists`。保留两份：另起 `名 (2).ext`，撞上往后试。目录同名不合并：只跳过或保留两份。
//! - **票**：带了票 ⇒ 这台按票记这一组。连接断了照常做完（都是这台本地盘上的事）；带同一张票再问 ⇒ 正在做就等它做完，
//!   做完了回那一份结局，不重做。记最多 [`KEEP`] 张，满了丢最早做完的。还没开做就回了的（同名要问 · 入参不对）不记。
//! - **进度**：按票有人听（[`listen`]，帧面那一层每条连接挂一个）就把 `{done, total, current}` 交过去；交法由听的那一侧定（帧面按变更合并、零定时器）。
//! - **撤**：[`stop`] 立旗；做完手上那一项就停，剩下的回 `cancelled`。递归删一棵大树按段删，段与段之间也看旗。
//!
//! 结局那一行（`said`）在这里拼，界面原样画。

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};

use cmd_wire::batch::{BatchOutcome, BatchReply, Conflict, Conflicts, Decide, ItemState};
use cmd_wire::files as wire;
use copy_core::copy_text;

use super::files_write::{
    change_mode_reporting, copy_entry, copy_tree, delete_empty_dir, delete_entry,
    delete_file_expecting, delete_tree_upto, lexical_in_root, rel_of, root_of, wire_of, WriteFail,
    WriteRefusal,
};
use crate::platform::fs::{Face, Ro};

/// 撤一组那一条的线上名。
pub(crate) const STOP: &str = "files-batch-stop";

/// 最多记几张票。
pub(crate) const KEEP: usize = 64;
/// 票的上限长度（界面给的是一个 UUID）。
const TICKET_MAX: usize = 64;
/// 保留两份另起名时最多往后试到第几个。
const MINT_TRIES: u32 = 999;
/// 递归删一棵树一段删几条（段与段之间看撤的旗、报进度）。
const DELETE_STRIDE: usize = 256;

/// 一组此刻做到哪了。
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Progress {
    pub(crate) ticket: String,
    pub(crate) done: u64,
    pub(crate) total: u64,
    /// 手上那一项（根底下的相对段，线上那一形）。
    pub(crate) current: Option<wire::WirePath>,
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
    fn stopped(&self) -> bool {
        self.job
            .as_ref()
            .is_some_and(|j| j.stop.load(Ordering::SeqCst))
    }

    fn step(&self, done: usize, current: Option<&Path>) {
        let Some(job) = &self.job else { return };
        let p = Progress {
            ticket: self.ticket.clone(),
            done: done as u64,
            total: self.total,
            current: current.map(wire_of),
        };
        *lock(&job.progress) = p.clone();
        for e in lock(&BOOK.ears).iter().filter(|e| e.ticket == self.ticket) {
            (e.tell)(&p);
        }
    }
}

/// 带票做一组：`probe` 是开做之前那几问（入参 · 同名；不成 ⇒ 不记票、原样回）；`work` 逐项做、交回结局。
/// 见过这张票 ⇒ 正在做就等它做完；做完了 ⇒ 回那一份（两样都不跑）。
fn ticketed<P, E: From<WriteFail>>(
    ticket: Option<&str>,
    total: usize,
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
        return Err(E::from(WriteFail::from((
            "bad_args",
            crate::common::contract::malformed("`ticket` must be 1-64 of [A-Za-z0-9-]"),
        ))));
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

/// 一项的键：它在 `items` 里的序号。
pub(crate) fn key_of(i: usize) -> String {
    i.to_string()
}

// ═══ 结局 ═══════════════════════════════════════════════════════════════════════════

/// 一项没成 ⇒ 那一项的结局（码 · 原因词 · 那一句与原话进复制详情）。
fn failed(key: &str, f: WriteFail) -> BatchOutcome {
    let reason = copy_core::code_reason(f.code, f.raw.as_deref());
    let detail = match &f.raw {
        Some(r) => format!("{}\n{r}", f.said),
        None => f.said.clone(),
    };
    BatchOutcome {
        code: Some(f.code.to_string()),
        reason: Some(reason),
        detail: Some(detail),
        ..BatchOutcome::bare(key, ItemState::Failed)
    }
}

/// 后端拼好的那一行：全成 ⇒ `已复制 5 项`（落在同一个目录 ⇒ `→ 目录`）；别的 ⇒ 逐样数出来。
fn said_of(verb: &str, outcomes: &[BatchOutcome], into: Option<&str>) -> String {
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
fn cancel_rest(out: &mut Vec<BatchOutcome>, from: usize, total: usize) {
    out.extend((from..total).map(|i| BatchOutcome::bare(key_of(i), ItemState::Cancelled)));
}

// ═══ 复制 ═══════════════════════════════════════════════════════════════════════════

/// 盘上那一格是什么（不跟链接）＋ 大小 · 修改时刻。
fn look(p: &Path) -> Option<(wire::FileKind, Option<u64>, Option<i64>)> {
    let m = Ro::symlink_metadata(p).ok()?;
    let t = m.file_type();
    let kind = if t.is_dir() {
        wire::FileKind::Dir
    } else if t.is_file() {
        wire::FileKind::File
    } else if t.is_symlink() {
        wire::FileKind::Symlink
    } else {
        wire::FileKind::Other
    };
    let size = t.is_file().then(|| m.len());
    let mtime = m
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64);
    Some((kind, size, mtime))
}

/// 一项同名时的答 ＋ 那一格是不是目录（不合并）。
struct Clash {
    choice: Option<Decide>,
    dir_into: bool,
}

/// 保留两份：`名 (k).ext` 里 `k` 从 2 起。
fn minted(to: &Path, k: u32) -> PathBuf {
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

/// 复制的一格做成了什么：落点 · 字节 · 几份文件 · 几个目录。
struct Landed {
    at: PathBuf,
    bytes: u64,
    files: u64,
    dirs: u64,
}

/// 复制的一格：目录整棵、别的一份（`overwrite` 只对文件）。
fn copy_one(
    face: &Face,
    root: &Path,
    from: &Path,
    to: &Path,
    dir: bool,
    overwrite: bool,
) -> Result<Landed, WriteRefusal> {
    if dir {
        copy_tree(face, root, from, to).map(|t| Landed {
            at: t.path,
            bytes: t.bytes,
            files: t.files as u64,
            dirs: t.dirs as u64,
        })
    } else {
        copy_entry(face, root, from, to, overwrite).map(|(at, bytes)| Landed {
            at,
            bytes,
            files: 1,
            dirs: 0,
        })
    }
}

/// `files-copy`。
pub(crate) fn copy(face: &Face, req: wire::FilesCopyReq) -> Result<BatchReply, CopyRefused> {
    let root = root_of(&req.root, "root")?;
    let on_exists = req.on_exists.unwrap_or_default();
    let items: Vec<(PathBuf, PathBuf)> = req
        .items
        .iter()
        .map(|i| (rel_of(&i.from), rel_of(&i.to)))
        .collect();
    let total = items.len();
    let decide = req.decide;
    let probe = || -> Result<Vec<Option<Clash>>, CopyRefused> {
        let mut clashes = Vec::with_capacity(total);
        let mut ask = Vec::new();
        for (i, (from, to)) in items.iter().enumerate() {
            let key = key_of(i);
            // 路径不合规的那一项留到做的时候各自回 `failed`，不挡整组。
            let (Ok(src), Ok(dst)) = (lexical_in_root(&root, from), lexical_in_root(&root, to))
            else {
                clashes.push(None);
                continue;
            };
            let Some((there, there_size, there_mtime)) = look(&dst) else {
                clashes.push(None);
                continue;
            };
            let (kind, size, mtime) = look(&src).unwrap_or((wire::FileKind::Other, None, None));
            let dir_into = kind == wire::FileKind::Dir || there == wire::FileKind::Dir;
            let choice = choice_of(&key, on_exists, decide.as_ref());
            if choice.is_none() {
                ask.push(Conflict {
                    key,
                    name: wire_of(Path::new(dst.file_name().unwrap_or_default())),
                    kind,
                    size,
                    mtime,
                    there,
                    there_size,
                    there_mtime,
                    dir_into,
                });
            }
            clashes.push(Some(Clash { choice, dir_into }));
        }
        if !ask.is_empty() {
            return Err(exists_fail(ask));
        }
        Ok(clashes)
    };
    let into = common_dir(items.iter().map(|(_, to)| to.as_path()));
    ticketed(req.ticket.as_deref(), total, probe, |clashes, run| {
        let mut out = Vec::with_capacity(total);
        for (i, ((from, to), clash)) in items.iter().zip(clashes).enumerate() {
            if run.stopped() {
                cancel_rest(&mut out, i, total);
                break;
            }
            run.step(i, Some(to));
            out.push(copy_item(face, &root, &key_of(i), from, to, clash));
        }
        run.step(out.len(), None);
        let verb = copy_text("beFiles.batch.verbCopy", &[]);
        let said = said_of(&verb, &out, into.as_deref());
        BatchReply {
            outcomes: out,
            said,
        }
    })
}

/// 复制开做之前没过的那一问：同名要问（那一句 ＋ 同名清单）· 别的（码 ＋ 那一句）。
#[derive(Debug)]
pub(crate) enum CopyRefused {
    Clash(String, Conflicts),
    Fail(WriteFail),
}

impl From<WriteFail> for CopyRefused {
    fn from(f: WriteFail) -> Self {
        CopyRefused::Fail(f)
    }
}

/// 码 `exists` ＋ 同名清单。
fn exists_fail(conflicts: Vec<Conflict>) -> CopyRefused {
    let n = conflicts.len().to_string();
    CopyRefused::Clash(
        copy_text("beFiles.batch.clash", &[("n", &n)]),
        Conflicts { conflicts },
    )
}

/// 那几项都落在同一个目录 ⇒ 那个目录（给人看的一行）。
fn common_dir<'a>(mut tos: impl Iterator<Item = &'a Path>) -> Option<String> {
    let first = tos.next()?.parent()?.to_path_buf();
    tos.all(|t| t.parent() == Some(first.as_path()))
        .then(|| first.to_string_lossy().into_owned())
        .filter(|s| !s.is_empty())
}

fn copy_item(
    face: &Face,
    root: &Path,
    key: &str,
    from: &Path,
    to: &Path,
    clash: Option<Clash>,
) -> BatchOutcome {
    let dir = lexical_in_root(root, from)
        .ok()
        .and_then(|p| look(&p))
        .is_some_and(|(k, ..)| k == wire::FileKind::Dir);
    let done = |l: Landed, new_name: Option<&Path>| BatchOutcome {
        path: Some(wire_of(&l.at)),
        new_name: new_name
            .and_then(Path::file_name)
            .map(|n| wire_of(Path::new(n))),
        bytes: Some(l.bytes),
        files: Some(l.files),
        dirs: Some(l.dirs),
        ..BatchOutcome::bare(key, ItemState::Done)
    };
    let fresh =
        |to: &Path| -> Result<Landed, WriteRefusal> { copy_one(face, root, from, to, dir, false) };
    let raced = |e: WriteRefusal, at: &Path| -> BatchOutcome {
        // 探的时候不在、做的时候在了 ⇒ 没盖，这一项回 `exists`。
        if lexical_in_root(root, at)
            .ok()
            .and_then(|p| look(&p))
            .is_some()
        {
            BatchOutcome {
                code: Some(super::files_write::EXISTS.to_string()),
                reason: Some(copy_core::code_reason(super::files_write::EXISTS, None)),
                detail: Some(e.message().to_string()),
                ..BatchOutcome::bare(key, ItemState::Exists)
            }
        } else {
            failed(key, WriteFail::from(e))
        }
    };
    match clash {
        None => match fresh(to) {
            Ok(d) => done(d, None),
            Err(e) => raced(e, to),
        },
        Some(Clash { choice: None, .. }) => BatchOutcome::bare(key, ItemState::Skipped),
        Some(Clash {
            choice: Some(Decide::Skip),
            ..
        }) => BatchOutcome::bare(key, ItemState::Skipped),
        // 目录不合并：「覆盖」只作用于文件，两头有一头是目录的那一项跳过。
        Some(Clash {
            choice: Some(Decide::Overwrite),
            dir_into: true,
        }) => BatchOutcome::bare(key, ItemState::Skipped),
        Some(Clash {
            choice: Some(Decide::Overwrite),
            dir_into: false,
        }) => match copy_one(face, root, from, to, false, true) {
            Ok(d) => done(d, None),
            Err(e) => failed(key, WriteFail::from(e)),
        },
        Some(Clash {
            choice: Some(Decide::KeepBoth),
            ..
        }) => {
            let mut last = None;
            for k in 2..=MINT_TRIES {
                let at = minted(to, k);
                if lexical_in_root(root, &at)
                    .ok()
                    .and_then(|p| look(&p))
                    .is_some()
                {
                    continue;
                }
                match fresh(&at) {
                    Ok(d) => return done(d, Some(&at)),
                    // 试的时候被人占了 ⇒ 往后试下一个。
                    Err(e)
                        if lexical_in_root(root, &at)
                            .ok()
                            .and_then(|p| look(&p))
                            .is_some() =>
                    {
                        last = Some(e);
                    }
                    Err(e) => return failed(key, WriteFail::from(e)),
                }
            }
            let e = last
                .unwrap_or_else(|| WriteRefusal::Exists(copy_text("beFiles.batch.mintSpent", &[])));
            failed(key, WriteFail::from(e))
        }
    }
}

// ═══ 删除 ═══════════════════════════════════════════════════════════════════════════

/// `files-delete`。
pub(crate) fn delete(face: &Face, req: wire::FilesDeleteReq) -> Result<BatchReply, WriteFail> {
    let root = root_of(&req.root, "root")?;
    if req
        .items
        .iter()
        .any(|i| i.recursive.unwrap_or(false) && i.expect.is_some())
    {
        return Err(WriteFail::from((
            "bad_args",
            crate::common::contract::malformed("`expect` and `recursive` are mutually exclusive"),
        )));
    }
    let total = req.items.len();
    let items = req.items;
    ticketed(
        req.ticket.as_deref(),
        total,
        || Ok::<(), WriteFail>(()),
        |(), run| {
            let mut out = Vec::with_capacity(total);
            for (i, item) in items.into_iter().enumerate() {
                if run.stopped() {
                    cancel_rest(&mut out, i, total);
                    break;
                }
                let rel = rel_of(&item.rel);
                run.step(i, Some(&rel));
                out.push(delete_item(face, &root, &key_of(i), &rel, item, run));
            }
            run.step(out.len(), None);
            let verb = copy_text("beFiles.batch.verbDelete", &[]);
            let said = said_of(&verb, &out, None);
            BatchReply {
                outcomes: out,
                said,
            }
        },
    )
}

fn delete_item(
    face: &Face,
    root: &Path,
    key: &str,
    rel: &Path,
    item: wire::DeleteItem,
    run: &Run,
) -> BatchOutcome {
    let done = |at: PathBuf, n: usize| BatchOutcome {
        path: Some(wire_of(&at)),
        removed: Some(n as u64),
        ..BatchOutcome::bare(key, ItemState::Done)
    };
    if item.recursive.unwrap_or(false) {
        // 一段一段删：段与段之间看撤的旗（撤了 ⇒ 这一项回 `cancelled`，删掉的那几条在 `bytes` 里）。
        let mut removed = 0usize;
        loop {
            match delete_tree_upto(face, root, rel, Some(DELETE_STRIDE)) {
                Ok((at, n, 0)) => return done(at, removed + n),
                Ok((_, n, _)) => {
                    removed += n;
                    if run.stopped() {
                        return BatchOutcome {
                            removed: Some(removed as u64),
                            ..BatchOutcome::bare(key, ItemState::Cancelled)
                        };
                    }
                }
                Err(e) => return failed(key, WriteFail::from(e)),
            }
        }
    }
    let got = match item.expect {
        Some(wire::DeleteExpect::Bytes(b)) => delete_file_expecting(face, root, rel, &b.0),
        Some(wire::DeleteExpect::EmptyDir) => delete_empty_dir(face, root, rel),
        None => delete_entry(face, root, rel),
    };
    match got {
        Ok(at) => done(at, 1),
        Err(e) => failed(key, WriteFail::from(e)),
    }
}

// ═══ 改权限 ═════════════════════════════════════════════════════════════════════════

/// `files-chmod`。
pub(crate) fn chmod(face: &Face, req: wire::FilesChmodReq) -> Result<BatchReply, WriteFail> {
    let root = root_of(&req.root, "root")?;
    let total = req.items.len();
    let mode = req.mode;
    let items = req.items;
    ticketed(
        req.ticket.as_deref(),
        total,
        || Ok::<(), WriteFail>(()),
        |(), run| {
            let mut out = Vec::with_capacity(total);
            for (i, item) in items.iter().enumerate() {
                if run.stopped() {
                    cancel_rest(&mut out, i, total);
                    break;
                }
                let rel = rel_of(&item.rel);
                run.step(i, Some(&rel));
                out.push(match change_mode_reporting(face, &root, &rel, mode) {
                    Ok((at, before)) => BatchOutcome {
                        path: Some(wire_of(&at)),
                        before,
                        ..BatchOutcome::bare(key_of(i), ItemState::Done)
                    },
                    Err(e) => failed(&key_of(i), WriteFail::from(e)),
                });
            }
            run.step(out.len(), None);
            let verb = copy_text("beFiles.batch.verbChmod", &[]);
            let said = said_of(&verb, &out, None);
            BatchReply {
                outcomes: out,
                said,
            }
        },
    )
}

// 资产域那扇门（`doors::LocalFiles`）问这三条也经这一处（同那几条帧命令）：同名要问那一形门上不会出现（门不问人），照码交回。
impl super::files_write::Act for wire::FilesCopyReq {
    type Reply = BatchReply;
    fn act(self, face: &Face) -> Result<BatchReply, WriteFail> {
        copy(face, self).map_err(|e| match e {
            CopyRefused::Clash(said, _) => WriteFail::from((super::files_write::EXISTS, said)),
            CopyRefused::Fail(f) => f,
        })
    }
}

impl super::files_write::Act for wire::FilesDeleteReq {
    type Reply = BatchReply;
    fn act(self, face: &Face) -> Result<BatchReply, WriteFail> {
        delete(face, self)
    }
}

impl super::files_write::Act for wire::FilesChmodReq {
    type Reply = BatchReply;
    fn act(self, face: &Face) -> Result<BatchReply, WriteFail> {
        chmod(face, self)
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/control/files_batch_tests.rs"]
mod tests;
