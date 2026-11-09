//! **把一组搬到另一台**（`transfer-copy`）：上传 · 下载 · 两台之间同这一条，两台都可以是本机；编排全在本机后端。
//!
//! - 目录整棵：在落点建目录、逐个文件传，逐项结局照实回（不打包）；目录里那几格的键是 `序号/相对段`。
//! - 同名（落点那台上那一格已在）：先把整组的落点探一遍，没答的 ⇒ 一项不做、回 `exists` 带同名清单；
//!   答法同同一台上的复制（覆盖只对文件 · 目录同名只许跳过或保留两份）。
//! - 一份文件半路失败或撤 ⇒ 清本机暂存那一份；远端暂存件交那台的孤儿扫。
//! - 票 · 进度 · 撤同同一台上那三条（`common/batch_book.rs`）：带票 ⇒ 断开照常做完、同一张票再问回同一份结局。
//!   进度多报一样字节：整组文件的字节和是总数，一份文件两腿（源 → 本机 · 本机 → 落点）各占它一半，传到一半也动。
//! - 路径一路是原始字节（`WirePath`）：非 UTF-8 的名字照字节到落点；落到本机的那一段名字过 `platform::paths::legal_name`。
//!
//! 两头经 [`Ends`] 这一层：生产那一份是两端拼成的 [`Pair`]，每一端是一个 [`Side`]：
//! - 本机（[`Here`]）：读用 `Ro`、建目录用 `Face`、落进用户目录只经 `common::staging::land`。
//! - 远端（[`There`]）：在不在 · 列一层 · 建目录 · 提交都问那台后端（[`Far`]，生产经那台的常驻流 `control::feeds`）；
//!   搬字节每份文件选一条腿：那台 SFTP 起始目录 == 它后端的家目录、且这一侧的路径是 UTF-8 ⇒ SFTP 腿，否则分块腿
//!   （`files-read-chunk` · `files-stage-chunk`，循环是 `chunk_core` 那一份）。提交回 `stale` ⇒ 这一份从头重传一次。
//!
//! 判据一族用本机两个目录上的替身判 `copy_with`，另有几趟用两端真拼（本机 · SFTP 台架 ＋ 假那台后端）判 [`Pair`]。

use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};

use cmd_wire::batch::{BatchOutcome, BatchReply, Conflict, Conflicts, Decide, ItemState};
use cmd_wire::files::{self as cwf, WirePath};
use cmd_wire::transfer::TransferCopyReq;
use copy_core::{copy_text, io_reason, Discard, Why};

use crate::common::batch_book::{choice_of, key_of, minted, ticketed, Run};
use crate::common::root_path::{walk_in_root, Last};
use crate::common::said::Said;
use crate::common::staging::{self, Content, LandErr, Was};
use crate::platform::fs::{Face, Resolved, Ro};
use crate::wire::typed::{self, codes};
use batch_core::said_of;

/// 一个要等的结果。
pub(crate) type Fut<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// 撤了没有（每一块之前问一次）。
pub(crate) type Stop<'s> = dyn Fn() -> bool + Send + Sync + 's;
/// 一份文件这一腿走到哪了：`(已到, 总共)`。
pub(crate) type Moved<'s> = dyn Fn(u64, u64) + Send + Sync + 's;

/// 源那台上的一格（相对那一项本身，段之间 `/`；顶上那一格是空）。
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Entry {
    pub(crate) rel: Vec<u8>,
    pub(crate) dir: bool,
    /// 文件的字节数（目录 ⇒ 0）。
    pub(crate) size: u64,
}

/// 两头的动作。
pub(crate) trait Ends: Send + Sync {
    /// 源那台：这一条是目录 ⇒ 整棵（先序，目录在它的内容之前，顶上那一格打头）；是文件 ⇒ 只它自己。
    fn walk<'a>(&'a self, path: &'a [u8]) -> Fut<'a, Result<Vec<Entry>, Said>>;
    /// 落点那台：`dir` 底下 `name` 那一格在不在（在 ⇒ 是不是目录）。
    fn there<'a>(&'a self, dir: &'a [u8], name: &'a [u8]) -> Fut<'a, Option<bool>>;
    /// 落点那台：在 `dir` 底下建一层 `name`。
    fn mkdir<'a>(&'a self, dir: &'a [u8], name: &'a [u8]) -> Fut<'a, Result<(), Said>>;
    /// 一份文件：源那台的 `from` → 落点那台的 `dir/name`；`stop()` 真 ⇒ 当场撤。回落了几个字节。
    /// `sent(n)`：这一份走到第几个字节了（两腿合起来折成一份文件的字节，单调不减）。
    fn copy_file<'a>(
        &'a self,
        from: &'a [u8],
        dir: &'a [u8],
        name: &'a [u8],
        overwrite: bool,
        stop: &'a Stop<'a>,
        sent: &'a (dyn Fn(u64) + Send + Sync),
    ) -> Fut<'a, Result<u64, Said>>;
    /// 落点那台怎么叫这一段名字（落本机 ⇒ 这台认的合法名；远端照原字节）。排步时每一段都过它，后面几下照它给的名字做。
    fn landing_name(&self, name: &[u8]) -> Vec<u8> {
        name.to_vec()
    }
}

codes! {
    /// `transfer-copy` 的码（一项没成不是整组没成：那一项的码在它的结局里）。
    pub enum CopyErr {
        /// 落点那台有同名的、还没答怎么办（一项都没做）：`data` 是同名清单。
        Exists = "exists" => Conflicts,
        /// 源或落点连接失败 · 问不到那台后端的家目录。
        Unreachable = "unreachable",
    }
}

/// `transfer-copy` 那一条（typed，帧面硬臂那一档：进度要走本连接的应答通道）。
pub struct TransferCopy;

impl typed::Command for TransferCopy {
    typed::by_call!(TransferCopyReq);
    type Err = CopyErr;
}

/// 一段路径的分隔符：照目录自己的写法（只有 `\` ⇒ `\`，否则 `/`）。
fn sep_of(dir: &[u8]) -> u8 {
    if !dir.contains(&b'/') && dir.contains(&b'\\') {
        b'\\'
    } else {
        b'/'
    }
}

fn join(dir: &[u8], name: &[u8]) -> Vec<u8> {
    let sep = sep_of(dir);
    let mut out = dir.to_vec();
    if !out.ends_with(&[sep]) {
        out.push(sep);
    }
    out.extend_from_slice(name);
    out
}

/// 末段名：照这条路径自己的分隔符切（同 [`sep_of`]；unix 上名字里的 `\` 不当分隔符）。
fn base(path: &[u8]) -> &[u8] {
    let sep = sep_of(path);
    let mut end = path.len();
    while end > 1 && path[end - 1] == sep {
        end -= 1;
    }
    let p = &path[..end];
    match p.iter().rposition(|b| *b == sep) {
        Some(i) => &p[i + 1..],
        None => p,
    }
}

/// 保留两份另起的名（`名 (k).ext`；名字不是 UTF-8 ⇒ 整段后面接 ` (k)`）。
fn minted_name(name: &[u8], k: u32) -> Vec<u8> {
    match std::str::from_utf8(name) {
        Ok(s) => minted(Path::new(s), k)
            .to_string_lossy()
            .into_owned()
            .into_bytes(),
        Err(_) => {
            let mut out = name.to_vec();
            out.extend_from_slice(format!(" ({k})").as_bytes());
            out
        }
    }
}

fn lossy(b: &[u8]) -> String {
    String::from_utf8_lossy(b).into_owned()
}

/// 一步：建一层目录 · 传一份文件。都出一格结局。
enum Step {
    Mkdir {
        key: String,
        dir: Vec<u8>,
        name: Vec<u8>,
    },
    File {
        key: String,
        from: Vec<u8>,
        dir: Vec<u8>,
        name: Vec<u8>,
        overwrite: bool,
        size: u64,
    },
    /// 开做之前就定了的那一格（跳过 · 列不出源）。
    Settled(BatchOutcome),
}

impl Step {
    fn key(&self) -> &str {
        match self {
            Step::Mkdir { key, .. } | Step::File { key, .. } => key,
            Step::Settled(o) => &o.key,
        }
    }
}

fn missed(key: &str, s: Said) -> BatchOutcome {
    let code = s.code_or_fallback();
    BatchOutcome {
        code: Some(code.to_string()),
        reason: Some(copy_core::code_reason(code, s.raw.as_deref())),
        detail: Some(match &s.raw {
            Some(r) => format!("{}\n{r}", s.said),
            None => s.said.clone(),
        }),
        ..BatchOutcome::bare(key, ItemState::Failed)
    }
}

/// 一项探过之后的样子。
struct Probed {
    from: Vec<u8>,
    /// 落到那边叫什么（另存为的名 · 缺 ⇒ 源的末段名）。
    name: Vec<u8>,
    walked: Result<Vec<Entry>, Said>,
    /// 落点那一格已在 ⇒ 答（`None` ＝ 没答）· 那一格或源是不是目录。
    clash: Option<(Option<Decide>, bool)>,
}

/// 搬一组（本体；两头经 `ends`，等的那几下在 `rt` 上等 —— 调用方在阻塞线程上）。
pub(crate) fn copy_with(
    ends: &dyn Ends,
    rt: &tokio::runtime::Handle,
    req: TransferCopyReq,
) -> Result<BatchReply, CopyErr> {
    let on_exists = req.on_exists.unwrap_or_default();
    let to_dir = req.to_dir.0.clone();
    let decide = req.decide;
    let mut probed: Vec<Probed> = Vec::with_capacity(req.items.len());
    let mut ask: Vec<Conflict> = Vec::new();
    for (i, item) in req.items.into_iter().enumerate() {
        let key = key_of(i);
        let from = item.path.0;
        let name = ends.landing_name(&item.name.map_or_else(|| base(&from).to_vec(), |n| n.0));
        let walked = rt.block_on(ends.walk(&from));
        let src_dir = walked
            .as_ref()
            .ok()
            .and_then(|w| w.first())
            .is_some_and(|e| e.dir);
        let clash = rt.block_on(ends.there(&to_dir, &name)).map(|there_dir| {
            let choice = choice_of(&key, on_exists, decide.as_ref());
            if choice.is_none() {
                let kind = |d: bool| {
                    if d {
                        cwf::FileKind::Dir
                    } else {
                        cwf::FileKind::File
                    }
                };
                ask.push(Conflict {
                    key: key.clone(),
                    name: WirePath(name.clone()),
                    kind: kind(src_dir),
                    size: None,
                    size_text: None,
                    mtime_text: None,
                    there: kind(there_dir),
                    there_size_text: None,
                    there_mtime_text: None,
                    answers: crate::common::batch_book::answers_of(src_dir || there_dir, true),
                });
            }
            (choice, src_dir || there_dir)
        });
        probed.push(Probed {
            from,
            name,
            walked,
            clash,
        });
    }
    if !ask.is_empty() {
        let n = ask.len().to_string();
        return Err(CopyErr::Exists(
            copy_text("beFiles.batch.clash", &[("n", &n)]),
            Conflicts { conflicts: ask },
        ));
    }
    let steps = plan(ends, rt, &to_dir, probed);
    let total = steps.len();
    let bytes_total: u64 = steps
        .iter()
        .map(|s| match s {
            Step::File { size, .. } => *size,
            _ => 0,
        })
        .sum();
    ticketed(
        req.ticket.as_deref(),
        total,
        CopyErr::BadArgs,
        || Ok(steps),
        |steps, run| {
            let mut out = Vec::with_capacity(total);
            let mut it = steps.into_iter();
            let mut before = 0u64;
            run.sent(0, bytes_total);
            for step in it.by_ref() {
                if run.stopped() {
                    out.push(BatchOutcome::bare(step.key(), ItemState::Cancelled));
                    break;
                }
                run.step(out.len(), None);
                let size = match &step {
                    Step::File { size, .. } => *size,
                    _ => 0,
                };
                out.push(do_step(ends, rt, step, run, before, bytes_total));
                // 这一份不论成败都算过去了（总数不变，进度不倒退）。
                before += size;
                run.sent(before, bytes_total);
            }
            out.extend(it.map(|s| BatchOutcome::bare(s.key(), ItemState::Cancelled)));
            run.step(out.len(), None);
            let verb = copy_text("render.codeBlock.copy", &[]);
            let said = said_of(&verb, &out, Some(&lossy(&to_dir)));
            BatchReply {
                outcomes: out,
                said,
            }
        },
    )
}

/// 一组探过之后排成一串步：顶上那一格先定落名（跳过 · 保留两份另起名），目录展开成建目录 ＋ 逐个文件。
fn plan(
    ends: &dyn Ends,
    rt: &tokio::runtime::Handle,
    to_dir: &[u8],
    probed: Vec<Probed>,
) -> Vec<Step> {
    let mut steps = Vec::new();
    for (i, p) in probed.into_iter().enumerate() {
        let key = key_of(i);
        let walked = match p.walked {
            Ok(w) => w,
            Err(m) => {
                steps.push(Step::Settled(missed(&key, m)));
                continue;
            }
        };
        let mut name = p.name;
        let mut overwrite = false;
        match p.clash {
            None => {}
            Some((Some(Decide::Skip) | None, _)) | Some((Some(Decide::Overwrite), true)) => {
                steps.push(Step::Settled(BatchOutcome::bare(&key, ItemState::Skipped)));
                continue;
            }
            Some((Some(Decide::Overwrite), false)) => overwrite = true,
            Some((Some(Decide::KeepBoth), _)) => {
                let free = (2..=crate::common::batch_book::MINT_TRIES)
                    .map(|k| ends.landing_name(&minted_name(&name, k)))
                    .find(|n| rt.block_on(ends.there(to_dir, n)).is_none());
                match free {
                    Some(n) => name = n,
                    None => {
                        steps.push(Step::Settled(missed(
                            &key,
                            Said::new("exists", copy_text("beFiles.batch.mintSpent", &[])),
                        )));
                        continue;
                    }
                }
            }
        }
        for e in walked {
            let (dir, leaf, k) = if e.rel.is_empty() {
                (to_dir.to_vec(), name.clone(), key.clone())
            } else {
                let segs: Vec<&[u8]> = e.rel.split(|b| *b == b'/').collect();
                let (leaf, parents): (&[u8], &[&[u8]]) = match segs.split_last() {
                    Some((l, p)) => (l, p),
                    None => (b"", &[]),
                };
                let mut d = join(to_dir, &name);
                for s in parents {
                    d = join(&d, &ends.landing_name(s));
                }
                (
                    d,
                    ends.landing_name(leaf),
                    format!("{key}/{}", lossy(&e.rel)),
                )
            };
            steps.push(if e.dir {
                Step::Mkdir {
                    key: k,
                    dir,
                    name: leaf,
                }
            } else {
                let from = if e.rel.is_empty() {
                    p.from.clone()
                } else {
                    join(&p.from, &e.rel)
                };
                Step::File {
                    key: k,
                    from,
                    dir,
                    name: leaf,
                    overwrite,
                    size: e.size,
                }
            });
        }
    }
    steps
}

fn do_step(
    ends: &dyn Ends,
    rt: &tokio::runtime::Handle,
    step: Step,
    run: &Run,
    before: u64,
    bytes_total: u64,
) -> BatchOutcome {
    let landed = |key: &str, at: Vec<u8>, bytes: Option<u64>| BatchOutcome {
        path: Some(WirePath(at)),
        bytes,
        ..BatchOutcome::bare(key, ItemState::Done)
    };
    match step {
        Step::Settled(o) => o,
        Step::Mkdir { key, dir, name } => match rt.block_on(ends.mkdir(&dir, &name)) {
            Ok(()) => landed(&key, join(&dir, &name), None),
            Err(m) => missed(&key, m),
        },
        Step::File {
            key,
            from,
            dir,
            name,
            overwrite,
            size,
        } => {
            let stop = || run.stopped();
            let sent = |n: u64| run.sent(before + n.min(size), bytes_total);
            match rt.block_on(ends.copy_file(&from, &dir, &name, overwrite, &stop, &sent)) {
                Ok(n) => landed(&key, join(&dir, &name), Some(n)),
                Err(_) if run.stopped() => BatchOutcome::bare(&key, ItemState::Cancelled),
                Err(m) => missed(&key, m),
            }
        }
    }
}

// ═══ 生产那一份两头：两端拼成 ════════════════════════════════════════════════════════════

/// 一台那一端（源或落点都用这一个口）。
pub(crate) trait Side: Send + Sync {
    /// 源：同 [`Ends::walk`]。
    fn walk<'a>(&'a self, path: &'a [u8]) -> Fut<'a, Result<Vec<Entry>, Said>>;
    /// 落点：同 [`Ends::there`]。
    fn there<'a>(&'a self, dir: &'a [u8], name: &'a [u8]) -> Fut<'a, Option<bool>>;
    /// 落点：同 [`Ends::mkdir`]。
    fn mkdir<'a>(&'a self, dir: &'a [u8], name: &'a [u8]) -> Fut<'a, Result<(), Said>>;
    /// 源：这一份交到本机一份读得到的文件 ⇒ `(在哪, 是不是暂存的)`。本机这一端 ⇒ 它自己（不搬）；远端 ⇒ 下进本机暂存区的 `into`。
    fn fetch<'a>(
        &'a self,
        from: &'a [u8],
        into: &'a Path,
        stop: &'a Stop<'a>,
        moved: &'a Moved<'a>,
    ) -> Fut<'a, Result<(PathBuf, bool), Said>>;
    /// 落点：本机那一份 `local` 落到 `dir/name`（`temp` ⇒ 它是暂存区里的一份，落本机时可以直接改名上位）。回落了几个字节。
    #[expect(
        clippy::too_many_arguments,
        reason = "一份文件那几格各是一样，捆成一个类型只为过这条提示"
    )]
    fn put<'a>(
        &'a self,
        local: &'a Path,
        temp: bool,
        dir: &'a [u8],
        name: &'a [u8],
        overwrite: bool,
        stop: &'a Stop<'a>,
        moved: &'a Moved<'a>,
    ) -> Fut<'a, Result<u64, Said>>;
    /// 落点：同 [`Ends::landing_name`]。
    fn landing_name(&self, name: &[u8]) -> Vec<u8> {
        name.to_vec()
    }
}

/// 两端拼成的 [`Ends`]：源那一端取到本机（本机那一端不搬），落点那一端从本机落。
pub(crate) struct Pair {
    face: Face,
    /// 本机暂存区（`$HOME/.cc-monitor/staging`）：远端源那一份先下到这里。
    staging: PathBuf,
    from: Box<dyn Side>,
    to: Box<dyn Side>,
}

/// 按名字开远端那一端（生产 [`There::open`]；判据给台架）。
pub(crate) type OpenFar<'o> =
    dyn Fn(Face, cmd_wire::Machine) -> Fut<'o, Result<Box<dyn Side>, CopyErr>> + Send + Sync + 'o;

impl Pair {
    /// 两端：本机 ⇒ [`Here`]，远端 ⇒ `far` 开；本机暂存区在 `home` 底下（不在就建）。
    pub(crate) async fn open(
        face: Face,
        home: &Path,
        from: &cmd_wire::Machine,
        to: &cmd_wire::Machine,
        far: &OpenFar<'_>,
    ) -> Result<Pair, CopyErr> {
        let staging = staging::ensure(&face, home).map_err(|s| CopyErr::Unreachable(s.said))?;
        let side = |m: &cmd_wire::Machine| -> Fut<'_, Result<Box<dyn Side>, CopyErr>> {
            if m.is_local() {
                let here: Box<dyn Side> = Box::new(Here { face: face.clone() });
                Box::pin(async move { Ok(here) })
            } else {
                far(face.clone(), m.clone())
            }
        };
        let from = side(from).await?;
        let to = side(to).await?;
        Ok(Pair {
            face,
            staging,
            from,
            to,
        })
    }
}

impl Ends for Pair {
    fn walk<'a>(&'a self, path: &'a [u8]) -> Fut<'a, Result<Vec<Entry>, Said>> {
        self.from.walk(path)
    }

    fn there<'a>(&'a self, dir: &'a [u8], name: &'a [u8]) -> Fut<'a, Option<bool>> {
        self.to.there(dir, name)
    }

    fn mkdir<'a>(&'a self, dir: &'a [u8], name: &'a [u8]) -> Fut<'a, Result<(), Said>> {
        self.to.mkdir(dir, name)
    }

    fn landing_name(&self, name: &[u8]) -> Vec<u8> {
        self.to.landing_name(name)
    }

    fn copy_file<'a>(
        &'a self,
        from: &'a [u8],
        dir: &'a [u8],
        name: &'a [u8],
        overwrite: bool,
        stop: &'a Stop<'a>,
        sent: &'a (dyn Fn(u64) + Send + Sync),
    ) -> Fut<'a, Result<u64, Said>> {
        Box::pin(async move {
            let tmp = format!("{}.copy", fresh_key(&(from, dir, name)));
            let into = self.staging.join(&tmp);
            // 两腿各占这一份的一半：取那一腿报 got/2，落那一腿报 (总 ＋ got)/2；从头重传那一趟不往回报（取报过的最大）。
            let up = AtomicBool::new(false);
            let high = std::sync::atomic::AtomicU64::new(0);
            let moved = |got: u64, total: u64| {
                let at = if up.load(Ordering::Relaxed) {
                    (total + got) / 2
                } else {
                    got / 2
                };
                sent(high.fetch_max(at, Ordering::Relaxed).max(at));
            };
            let r = async {
                let (local, temp) = self.from.fetch(from, &into, stop, &moved).await?;
                up.store(true, Ordering::Relaxed);
                self.to
                    .put(&local, temp, dir, name, overwrite, stop, &moved)
                    .await
            }
            .await;
            // 本机暂存那一份不论成败都删（落本机时已经改名上位、不在了）。
            if Ro::symlink_metadata(&into).is_ok() {
                if let Ok(at) = walk_in_root(Last::Link, &self.staging, &tmp) {
                    self.face.delete_file(&at).discard(Why::CleanupAfterFailure);
                    // 删不掉只是暂存区里多一份，孤儿扫会收
                }
            }
            r
        })
    }
}

/// 一份暂存件的新键（32 位十六进制，同 `files::commit` 认的那一形）：照这一份是谁 ＋ 此刻。
fn fresh_key(what: &impl std::hash::Hash) -> String {
    let nanos = crate::common::time::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    super::transfer::staging_key(what, 0, nanos)
}

/// 撤了那一句（各腿撤时同一句）。
fn cancelled() -> Said {
    Said::new("cancelled", copy_text("reason.op.cancelled", &[]))
}

fn io_said(said: String, e: &std::io::Error) -> Said {
    Said::with_raw(said, e).coded("io_failed")
}

// ─── 本机那一端 ───────────────────────────────────────────────────────────────────

/// 本机：读用 `Ro`；建目录与落地经文件管理写句柄，落进用户目录只经 `common::staging::land`；落名在排步时已过 `legal_name`（[`Side::landing_name`]），这里照给的名字做。
pub(crate) struct Here {
    pub(crate) face: Face,
}

/// 本机的路径字节 ⇒ `PathBuf`（这一端的路径都是本机自己发出去的那一份回流）。
fn here_path(b: &[u8]) -> PathBuf {
    crate::common::path_wire::to_path_buf(b)
}

impl Side for Here {
    fn walk<'a>(&'a self, path: &'a [u8]) -> Fut<'a, Result<Vec<Entry>, Said>> {
        Box::pin(async move {
            let top = here_path(path);
            let Ok(m) = Ro::metadata(&top) else {
                return Err(Said::new(
                    "io_failed",
                    copy_text("beTransferCopy.from.missing", &[("path", &lossy(path))]),
                ));
            };
            let mut out = vec![Entry {
                rel: Vec::new(),
                dir: m.is_dir(),
                size: if m.is_dir() { 0 } else { m.len() },
            }];
            if !m.is_dir() {
                return Ok(out);
            }
            let mut stack: Vec<Vec<u8>> = vec![Vec::new()];
            while let Some(rel) = stack.pop() {
                let at = if rel.is_empty() {
                    top.clone()
                } else {
                    top.join(here_path(&rel))
                };
                let Ok(rd) = Ro::read_dir(&at) else { continue };
                for e in rd.flatten() {
                    let name = e.file_name();
                    let mut r = rel.clone();
                    if !r.is_empty() {
                        r.push(b'/');
                    }
                    r.extend_from_slice(name.as_encoded_bytes());
                    // 跟链接看它是什么；指向目录的链接不进去（不跟着链接走出这棵树）。
                    let Ok(lm) = Ro::symlink_metadata(e.path()) else {
                        continue;
                    };
                    let Ok(m) = Ro::metadata(e.path()) else {
                        continue;
                    };
                    if m.is_dir() {
                        if lm.file_type().is_symlink() {
                            continue;
                        }
                        stack.push(r.clone());
                        out.push(Entry {
                            rel: r,
                            dir: true,
                            size: 0,
                        });
                    } else if m.is_file() {
                        out.push(Entry {
                            rel: r,
                            dir: false,
                            size: m.len(),
                        });
                    }
                }
            }
            // 先序：目录在它的内容之前（按相对段排，父段恒在子段之前）。
            out.sort_by(|a, b| a.rel.cmp(&b.rel));
            Ok(out)
        })
    }

    fn there<'a>(&'a self, dir: &'a [u8], name: &'a [u8]) -> Fut<'a, Option<bool>> {
        Box::pin(async move {
            let at = here_path(dir).join(here_path(name));
            Ro::symlink_metadata(at).ok().map(|m| m.is_dir())
        })
    }

    fn mkdir<'a>(&'a self, dir: &'a [u8], name: &'a [u8]) -> Fut<'a, Result<(), Said>> {
        Box::pin(async move {
            let at = walk_in_root(Last::Link, &here_path(dir), here_path(name))
                .map_err(|m| Said::new("refused", m))?;
            self.face.make_dir(&at).map_err(|e| {
                io_said(
                    copy_text(
                        "reason.fs.writeFailed",
                        &[
                            ("path", &win_path_core::shown(&at)),
                            ("why", &io_reason(e.kind())),
                        ],
                    ),
                    &e,
                )
            })
        })
    }

    fn fetch<'a>(
        &'a self,
        from: &'a [u8],
        _into: &'a Path,
        _stop: &'a Stop<'a>,
        moved: &'a Moved<'a>,
    ) -> Fut<'a, Result<(PathBuf, bool), Said>> {
        Box::pin(async move {
            let p = here_path(from);
            let n = Ro::metadata(&p).map(|m| m.len()).unwrap_or(0);
            moved(n, n);
            Ok((p, false))
        })
    }

    fn put<'a>(
        &'a self,
        local: &'a Path,
        temp: bool,
        dir: &'a [u8],
        name: &'a [u8],
        overwrite: bool,
        stop: &'a Stop<'a>,
        moved: &'a Moved<'a>,
    ) -> Fut<'a, Result<u64, Said>> {
        Box::pin(async move {
            if stop() {
                return Err(cancelled());
            }
            let staged: Option<Resolved> = if temp {
                let parent = local.parent().unwrap_or(Path::new(""));
                let leaf = local.file_name().unwrap_or_default();
                Some(walk_in_root(Last::Link, parent, leaf).map_err(|m| Said::new("refused", m))?)
            } else {
                None
            };
            let content = match &staged {
                Some(at) => Content::Staged(at),
                None => Content::File(local),
            };
            let rel = here_path(name);
            let was = if overwrite { Was::Either } else { Was::New };
            let (_, n) = staging::land(
                &self.face,
                &here_path(dir),
                &rel,
                content,
                None,
                was,
                &mut || {},
            )
            .map_err(|e| match e {
                LandErr::Exists(m) => Said::new("exists", m),
                LandErr::Stale(m) => Said::new("stale", m),
                LandErr::Refused(m) => Said::new("refused", m),
                LandErr::Io { said, raw } => Said::from((said, Some(raw))).coded("io_failed"),
            })?;
            moved(n, n);
            Ok(n)
        })
    }

    fn landing_name(&self, name: &[u8]) -> Vec<u8> {
        crate::platform::paths::legal_name(name)
            .as_encoded_bytes()
            .to_vec()
    }
}

// ─── 远端那一端 ───────────────────────────────────────────────────────────────────

/// 问那台后端一条命令（线上那一层：命令名 ＋ 入参 ⇒ 回包）。生产经那台的常驻流（[`Feed`]）；判据给一台假后端。
pub(crate) trait Far: Send + Sync {
    fn ask<'a>(
        &'a self,
        cmd: &'static str,
        args: serde_json::Value,
    ) -> Fut<'a, Result<serde_json::Value, Said>>;
}

/// 经那台的常驻流问（`control::feeds::forward`，与本机后端问那台的别处同一条路）。
struct Feed {
    machine: cmd_wire::Machine,
}

impl Far for Feed {
    fn ask<'a>(
        &'a self,
        cmd: &'static str,
        args: serde_json::Value,
    ) -> Fut<'a, Result<serde_json::Value, Said>> {
        Box::pin(async move {
            let req: crate::wire::Request =
                serde_json::from_value(serde_json::json!({ "id": "", "cmd": cmd, "args": args }))
                    .map_err(|e| {
                    Said::new(
                        "bad_args",
                        crate::common::contract::malformed(&e.to_string()),
                    )
                })?;
            let got = super::feeds::forward(self.machine.to_string(), req).await?;
            Ok(got.unwrap_or(serde_json::Value::Null))
        })
    }
}

/// 问那台一条有类型的命令；没成 ⇒ 码收成这一条认的那几种（别的一律 `io_failed`）。
async fn ask<C: cmd_wire::Call + Sync>(far: &dyn Far, req: &C) -> Result<C::Reply, Said> {
    let args = serde_json::to_value(req).map_err(|e| {
        Said::new(
            "bad_args",
            crate::common::contract::malformed(&e.to_string()),
        )
    })?;
    let v = far.ask(C::NAME, args).await.map_err(|s| {
        let code = match s.code.as_deref() {
            Some(c @ ("exists" | "refused" | "stale" | "bad_name" | "unreachable")) => {
                c.to_string()
            }
            _ => "io_failed".to_string(),
        };
        s.coded(&code)
    })?;
    serde_json::from_value(v)
        .map_err(|_| Said::new("io_failed", copy_core::reply_unreadable(C::NAME)))
}

/// `chunk_core` 那一口：问那台经 [`Far`]。
struct By<'a>(&'a dyn Far);

impl chunk_core::Ask for By<'_> {
    type Err = Said;
    async fn ask<C: cmd_wire::Call + Sync>(&self, req: &C) -> Result<C::Reply, Said> {
        ask(self.0, req).await
    }
}

/// `chunk_core` 那一口：撤 · 进度。
struct Watching<'a> {
    stop: &'a Stop<'a>,
    moved: &'a Moved<'a>,
}

impl chunk_core::Watch for Watching<'_> {
    fn stopped(&self) -> bool {
        (self.stop)()
    }
    fn moved(&mut self, done: u64, total: Option<u64>) {
        (self.moved)(done, total.unwrap_or(done));
    }
}

/// 读回来的一块写进本机暂存那一份。
struct ToFile(crate::platform::fs::FaceFile);

impl chunk_core::Sink for ToFile {
    type Err = Said;
    async fn put(&mut self, bytes: Vec<u8>) -> Result<(), Said> {
        use std::io::Write as _;
        self.0.write_all(&bytes).map_err(|e| {
            io_said(
                copy_text(
                    "beTransfer.download.writeLocalFailed",
                    &[("why", &io_reason(e.kind()))],
                ),
                &e,
            )
        })
    }
}

/// 列一层要的条数上限（一次往返整份回来；比它多 ⇒ 那一层截掉的不搬，那一份照列到的做）。
const LS_ALL: u64 = 1_000_000;

/// 远端：元数据都问那台后端；搬字节每份选一条腿（[`Self::sftp`] 在 ⇒ 那台 SFTP 起始目录就是它后端的家）。
pub(crate) struct There {
    face: Face,
    far: Box<dyn Far>,
    /// SFTP 腿：只在那台 SFTP 起始目录 == 它后端的家目录时留着；没有 ⇒ 每份都走分块腿。
    sftp: Option<crate::dial::sftp::Session>,
}

impl There {
    /// 按名字开：问那台后端家目录（问不到 ⇒ 那台不可达）；SFTP 照这一端的机器表试开一条，开不成 ⇒ 只走分块腿。
    pub(crate) fn open(
        face: Face,
        m: cmd_wire::Machine,
    ) -> Fut<'static, Result<Box<dyn Side>, CopyErr>> {
        Box::pin(async move {
            let hosts = crate::dial::machines_now();
            let sftp = match super::transfer::dial_by_name(&hosts, &m) {
                Ok((_, d)) => crate::dial::sftp::open_for_transfer(&d).await.ok(),
                Err(_) => None,
            };
            let far = Box::new(Feed { machine: m });
            let there: Box<dyn Side> = Box::new(There::over(face, far, sftp).await?);
            Ok(there)
        })
    }

    /// 问那台后端的家目录；SFTP 起始目录与它不一样 ⇒ 不用 SFTP 腿（暂存件会落到后端看不见的地方）。
    pub(crate) async fn over(
        face: Face,
        far: Box<dyn Far>,
        sftp: Option<crate::dial::sftp::Session>,
    ) -> Result<There, CopyErr> {
        let h = ask(far.as_ref(), &cwf::HomeReq {})
            .await
            .map_err(|s| CopyErr::Unreachable(s.said))?;
        let home = lossy(h.path.bytes());
        let sftp = sftp.filter(|s| super::transfer::start_dir_mismatch(s.home(), &home).is_none());
        Ok(There { face, far, sftp })
    }

    /// 那台列一层：`(名字, 是不是目录, 字节数)`；指向目录的链接 · 别的那几种不列。
    async fn ls(&self, path: &[u8]) -> Result<Vec<(Vec<u8>, bool, u64)>, Said> {
        let req = cwf::LsReq {
            path: WirePath(path.to_vec()),
            limit: Some(LS_ALL),
            dirs_only: None,
            sort: None,
            held: None,
            range: None,
        };
        let got = ask(self.far.as_ref(), &req).await?;
        Ok(got
            .up
            .into_values()
            .filter_map(|e| match e.kind {
                cwf::FileKind::Dir => Some((e.name.0, true, 0)),
                cwf::FileKind::File => Some((e.name.0, false, e.size.unwrap_or(0))),
                cwf::FileKind::Symlink if e.link_dir != Some(true) => {
                    Some((e.name.0, false, e.size.unwrap_or(0)))
                }
                _ => None,
            })
            .collect())
    }

    /// 落一份：SFTP 腿或分块腿进那台暂存区 → 提交。回落了几个字节。
    async fn put_once(
        &self,
        local: &Path,
        dir: &[u8],
        name: &[u8],
        overwrite: bool,
        stop: &Stop<'_>,
        moved: &Moved<'_>,
    ) -> Result<u64, Said> {
        let key = fresh_key(&(local, dir, name));
        let commit =
            |chunks: Option<u64>, bytes: Option<u64>, sha: String| cwf::FilesCommitUploadReq {
                root: WirePath(dir.to_vec()),
                rel: WirePath(name.to_vec()),
                key: key.clone(),
                on_exists: overwrite.then_some(cmd_wire::batch::OnExists::Overwrite),
                expect: cwf::Sha256Expect {
                    sha256: cwf::Sha256Hex(sha),
                },
                chunks,
                bytes,
                single: Some(true),
            };
        if let (Some(s), Some(l)) = (&self.sftp, local.to_str()) {
            let cancel = super::transfer::Cancel::default();
            let sink = |got: u64, total: u64| {
                if stop() {
                    cancel.fire();
                }
                moved(got, total);
            };
            let (n, sha) = super::transfer::upload_to_staging(s, l, &key, &cancel, &sink)
                .await
                .map_err(|e| {
                    if cancel.is_set() {
                        cancelled()
                    } else {
                        e.coded("io_failed")
                    }
                })?;
            let r = ask(self.far.as_ref(), &commit(None, None, sha)).await;
            if r.is_err() {
                crate::dial::sftp::remove(s, &super::transfer::staging_part(&key))
                    .await
                    .discard(Why::CleanupAfterFailure); // 删不掉只是那台暂存区里多一份，孤儿扫会收
            }
            return r.map(|_| n);
        }
        let file = Ro::open(local).map_err(|e| {
            io_said(
                copy_text(
                    "beTransfer.local.readFailed",
                    &[
                        ("path", &local.display().to_string()),
                        ("why", &io_reason(e.kind())),
                    ],
                ),
                &e,
            )
        })?;
        let total = Ro::metadata(local).map(|m| m.len()).ok();
        let mut watch = Watching { stop, moved };
        let staged = chunk_core::stage(
            &By(self.far.as_ref()),
            key.clone(),
            file,
            total,
            &mut watch,
            cancelled,
            |e| {
                io_said(
                    copy_text(
                        "beTransfer.local.readFailed",
                        &[
                            ("path", &local.display().to_string()),
                            ("why", &io_reason(e.kind())),
                        ],
                    ),
                    &e,
                )
            },
        )
        .await?;
        ask(
            self.far.as_ref(),
            &commit(Some(staged.chunks), Some(staged.bytes), staged.sha256),
        )
        .await?;
        Ok(staged.bytes)
    }
}

impl Side for There {
    fn walk<'a>(&'a self, path: &'a [u8]) -> Fut<'a, Result<Vec<Entry>, Said>> {
        Box::pin(async move {
            let missing = || {
                Said::new(
                    "io_failed",
                    copy_text("beTransferCopy.from.missing", &[("path", &lossy(path))]),
                )
            };
            let st = ask(
                self.far.as_ref(),
                &cwf::StatReq {
                    path: WirePath(path.to_vec()),
                },
            )
            .await
            .map_err(|_| missing())?;
            if st.kind != cwf::FileKind::Dir {
                return Ok(vec![Entry {
                    rel: Vec::new(),
                    dir: false,
                    size: st.size,
                }]);
            }
            let mut out = vec![Entry {
                rel: Vec::new(),
                dir: true,
                size: 0,
            }];
            let mut stack: Vec<Vec<u8>> = vec![Vec::new()];
            while let Some(rel) = stack.pop() {
                let at = if rel.is_empty() {
                    path.to_vec()
                } else {
                    join(path, &rel)
                };
                // 列不出的那一层当空的（同一格不另报：它底下的那几份不在结局里）。
                for (name, dir, size) in self.ls(&at).await.unwrap_or_default() {
                    let mut r = rel.clone();
                    if !r.is_empty() {
                        r.push(b'/');
                    }
                    r.extend_from_slice(&name);
                    if dir {
                        stack.push(r.clone());
                    }
                    out.push(Entry {
                        rel: r,
                        dir,
                        size: if dir { 0 } else { size },
                    });
                }
            }
            out.sort_by(|a, b| a.rel.cmp(&b.rel));
            Ok(out)
        })
    }

    fn there<'a>(&'a self, dir: &'a [u8], name: &'a [u8]) -> Fut<'a, Option<bool>> {
        Box::pin(async move {
            ask(
                self.far.as_ref(),
                &cwf::StatReq {
                    path: WirePath(join(dir, name)),
                },
            )
            .await
            .ok()
            .map(|s| s.kind == cwf::FileKind::Dir)
        })
    }

    fn mkdir<'a>(&'a self, dir: &'a [u8], name: &'a [u8]) -> Fut<'a, Result<(), Said>> {
        Box::pin(async move {
            let req = cwf::FilesMkdirReq {
                root: WirePath(dir.to_vec()),
                rel: WirePath(name.to_vec()),
                single: None,
            };
            ask(self.far.as_ref(), &req).await.map(|_| ())
        })
    }

    fn fetch<'a>(
        &'a self,
        from: &'a [u8],
        into: &'a Path,
        stop: &'a Stop<'a>,
        moved: &'a Moved<'a>,
    ) -> Fut<'a, Result<(PathBuf, bool), Said>> {
        Box::pin(async move {
            if let (Some(s), Ok(f)) = (&self.sftp, std::str::from_utf8(from)) {
                let cancel = super::transfer::Cancel::default();
                let sink = |got: u64, total: u64| {
                    if stop() {
                        cancel.fire();
                    }
                    moved(got, total);
                };
                let got =
                    super::transfer::download_to_local(&self.face, s, f, into, &cancel, &sink)
                        .await;
                if let Err(e) = got {
                    // 下载那一腿没成时留着的半成品（续传的本钱）在这里续不上（`into` 每份现造一个键）⇒ 当场删。
                    let part = super::transfer::download_part_name(f, into);
                    if let Ok(dir) = staging::home().and_then(|h| staging::ensure(&self.face, &h)) {
                        if Ro::symlink_metadata(dir.join(&part)).is_ok() {
                            if let Ok(at) = walk_in_root(Last::Link, &dir, &part) {
                                self.face.delete_file(&at).discard(Why::CleanupAfterFailure);
                                // 删不掉只是暂存区里多一份，孤儿扫会收
                            }
                        }
                    }
                    return Err(if cancel.is_set() {
                        cancelled()
                    } else {
                        e.coded("io_failed")
                    });
                }
                return Ok((into.to_path_buf(), true));
            }
            let parent = into.parent().unwrap_or(Path::new(""));
            let leaf = into.file_name().unwrap_or_default();
            let at = walk_in_root(Last::Link, parent, leaf).map_err(|m| Said::new("refused", m))?;
            let file = self.face.create_new(&at).map_err(|e| {
                io_said(
                    copy_text(
                        "beTransfer.land.createFailed",
                        &[
                            ("path", &win_path_core::shown(&at)),
                            ("why", &io_reason(e.kind())),
                        ],
                    ),
                    &e,
                )
            })?;
            let mut sink = ToFile(file);
            let mut watch = Watching { stop, moved };
            chunk_core::pull(
                &By(self.far.as_ref()),
                WirePath(from.to_vec()),
                &mut sink,
                &mut watch,
                cancelled,
            )
            .await?;
            Ok((into.to_path_buf(), true))
        })
    }

    fn put<'a>(
        &'a self,
        local: &'a Path,
        _temp: bool,
        dir: &'a [u8],
        name: &'a [u8],
        overwrite: bool,
        stop: &'a Stop<'a>,
        moved: &'a Moved<'a>,
    ) -> Fut<'a, Result<u64, Said>> {
        Box::pin(async move {
            // 提交时那台对出来的摘要与送过去的对不上（`stale`）⇒ 这一份从头重传一次。
            match self
                .put_once(local, dir, name, overwrite, stop, moved)
                .await
            {
                Err(s) if s.code.as_deref() == Some("stale") && !stop() => {
                    self.put_once(local, dir, name, overwrite, stop, moved)
                        .await
                }
                r => r,
            }
        })
    }
}

/// 帧面硬臂那一发的本体（阻塞线程上，经 `typed::answer_blocking`）：拼好两端 → 做一组 → 回包。
pub fn run(
    face: Face,
    req: <TransferCopy as typed::Command>::Req,
    _cx: typed::Cx,
) -> Result<<TransferCopy as typed::Command>::Reply, <TransferCopy as typed::Command>::Err> {
    let rt = tokio::runtime::Handle::current();
    let home = staging::home().map_err(|s| CopyErr::Unreachable(s.said))?;
    let ends = rt.block_on(Pair::open(face, &home, &req.from, &req.to, &There::open))?;
    copy_with(&ends, &rt, req)
}

#[cfg(test)]
#[path = "../../../tests/backend/control/transfer_copy_tests.rs"]
mod tests;
