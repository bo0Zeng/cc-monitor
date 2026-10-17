//! **两台之间复制一组**（`transfer-copy`）：经本机中转，A → 本机暂存 → B 暂存 → B 提交，两台都经本机后端已有的连接。
//!
//! - 目录整棵：在 B 上建目录、逐个文件传，逐项结局照实回（不打包）；目录里那几格的键是 `序号/相对段`。
//! - 同名（落点那台上那一格已在）：先把整组的落点探一遍，没答的 ⇒ 一项不做、回 `exists` 带同名清单；
//!   答法同同一台上的复制（覆盖只对文件 · 目录同名只许跳过或保留两份）。
//! - 一份文件半路失败或撤 ⇒ 清两头暂存（本机那一份 · B 那头的暂存件）。
//! - 票 · 进度 · 撤同同一台上那三条（`common/batch_book.rs`）：带票 ⇒ 断开照常做完、同一张票再问回同一份结局。
//!   进度多报一样字节：整组文件的字节和是总数，一份文件两腿（A → 本机 · 本机 → B）各占它一半，传到一半也动。
//!
//! 两头的动作经 [`Ends`] 这一层：生产那一份是 SFTP ＋ 问 B 那台后端（[`Wired`]）；判据一族用本机两个目录上的替身，
//! 一趟用两台合成 SFTP 台架 ＋ 假远端判生产那一份（入参照真的解码器解、提交走真的那一份）。

use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;

use cmd_wire::batch::{BatchOutcome, BatchReply, Conflict, Conflicts, Decide, ItemState};
use cmd_wire::files as cwf;
use cmd_wire::transfer::TransferCopyReq;
use copy_core::copy_text;

use crate::common::batch_book::{choice_of, key_of, minted, said_of, ticketed, Run};
use crate::common::said::Said;
use crate::wire::typed::{self, codes};

/// 一个要等的结果。
pub(crate) type Fut<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// 一处没成：码 ＋ 那一句（原话另带）。
pub(crate) type Miss = (&'static str, Said);

/// 源那台上的一格（相对那一项本身；顶上那一格是 `""`）。
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Entry {
    pub(crate) rel: String,
    pub(crate) dir: bool,
    /// 文件的字节数（目录 ⇒ 0）。
    pub(crate) size: u64,
}

/// 两头的动作。
pub(crate) trait Ends: Send + Sync {
    /// 源那台：这一条是目录 ⇒ 整棵（先序，目录在它的内容之前，顶上那一格打头）；是文件 ⇒ 只它自己。
    fn walk<'a>(&'a self, path: &'a str) -> Fut<'a, Result<Vec<Entry>, Miss>>;
    /// 落点那台：这一格在不在（在 ⇒ 是不是目录）。
    fn there<'a>(&'a self, path: &'a str) -> Fut<'a, Option<bool>>;
    /// 落点那台：在 `dir` 底下建一层 `name`。
    fn mkdir<'a>(&'a self, dir: &'a str, name: &'a str) -> Fut<'a, Result<(), Miss>>;
    /// 一份文件：源那台的 `from` → 落点那台的 `dir/name`（经本机中转）；`stop()` 真 ⇒ 当场撤（清两头暂存）。回落了几个字节。
    /// `sent(n)`：这一份走到第几个字节了（两腿合起来折成一份文件的字节，单调不减）。
    fn copy_file<'a>(
        &'a self,
        from: &'a str,
        dir: &'a str,
        name: &'a str,
        overwrite: bool,
        stop: &'a (dyn Fn() -> bool + Send + Sync),
        sent: &'a (dyn Fn(u64) + Send + Sync),
    ) -> Fut<'a, Result<u64, Miss>>;
}

codes! {
    /// `transfer-copy` 的码（一项没成不是整组没成：那一项的码在它的结局里）。
    pub(crate) enum CopyErr {
        /// 落点那台有同名的、还没答怎么办（一项都没做）：`data` 是同名清单。
        Exists = "exists" => Conflicts,
        /// 源或落点连接失败 · 问不到落点后端的家目录。
        Unreachable = "unreachable",
        /// 落点那台的 SFTP 起始目录不是它后端的家目录（暂存件会落到后端看不见的地方）：一个字节不写。
        SftpHomeMismatch = "sftp_home_mismatch",
    }
}

/// `transfer-copy` 那一条（typed，帧面硬臂那一档：进度要走本连接的应答通道）。
pub(crate) struct TransferCopy;

impl typed::Command for TransferCopy {
    const NAME: &'static str = "transfer-copy";
    type Req = TransferCopyReq;
    type Reply = BatchReply;
    type Err = CopyErr;
}

fn join(dir: &str, name: &str) -> String {
    if dir.ends_with('/') {
        format!("{dir}{name}")
    } else {
        format!("{dir}/{name}")
    }
}

fn base(path: &str) -> &str {
    path.trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or(path)
}

/// 一步：建一层目录 · 传一份文件。都出一格结局。
enum Step {
    Mkdir {
        key: String,
        dir: String,
        name: String,
    },
    File {
        key: String,
        from: String,
        dir: String,
        name: String,
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

fn missed(key: &str, (code, s): Miss) -> BatchOutcome {
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
    from: String,
    walked: Result<Vec<Entry>, Miss>,
    /// 落点那一格已在 ⇒ 答（`None` ＝ 没答）· 那一格或源是不是目录。
    clash: Option<(Option<Decide>, bool)>,
}

/// 两台之间复制一组（本体；两头经 `ends`，等的那几下在 `rt` 上等 —— 调用方在阻塞线程上）。
pub(crate) fn copy_with(
    ends: &dyn Ends,
    rt: &tokio::runtime::Handle,
    req: TransferCopyReq,
) -> Result<BatchReply, CopyErr> {
    let on_exists = req.on_exists.unwrap_or_default();
    let to_dir = req.to_dir.clone();
    let froms: Vec<String> = req.items.iter().map(|i| i.remote_path.clone()).collect();
    let decide = req.decide;
    let mut probed: Vec<Probed> = Vec::with_capacity(froms.len());
    let mut ask: Vec<Conflict> = Vec::new();
    for (i, from) in froms.iter().enumerate() {
        let key = key_of(i);
        let walked = rt.block_on(ends.walk(from));
        let src_dir = walked
            .as_ref()
            .ok()
            .and_then(|w| w.first())
            .is_some_and(|e| e.dir);
        let clash = rt
            .block_on(ends.there(&join(&to_dir, base(from))))
            .map(|there_dir| {
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
                        name: cwf::WirePath::from(base(from)),
                        kind: kind(src_dir),
                        size: None,
                        size_text: None,
                        mtime_text: None,
                        there: kind(there_dir),
                        there_size: None,
                        there_size_text: None,
                        there_mtime_text: None,
                        answers: crate::common::batch_book::answers_of(src_dir || there_dir, true),
                    });
                }
                (choice, src_dir || there_dir)
            });
        probed.push(Probed {
            from: from.clone(),
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
            let verb = copy_text("beFiles.batch.verbCopy", &[]);
            let said = said_of(&verb, &out, Some(&to_dir));
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
    to_dir: &str,
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
        let mut name = base(&p.from).to_string();
        let mut overwrite = false;
        match p.clash {
            None => {}
            Some((Some(Decide::Skip) | None, _)) | Some((Some(Decide::Overwrite), true)) => {
                steps.push(Step::Settled(BatchOutcome::bare(&key, ItemState::Skipped)));
                continue;
            }
            Some((Some(Decide::Overwrite), false)) => overwrite = true,
            Some((Some(Decide::KeepBoth), _)) => {
                let free = (2..=999u32)
                    .map(|k| minted(Path::new(&name), k).to_string_lossy().into_owned())
                    .find(|n| rt.block_on(ends.there(&join(to_dir, n))).is_none());
                match free {
                    Some(n) => name = n,
                    None => {
                        steps.push(Step::Settled(missed(
                            &key,
                            (
                                "exists",
                                Said::from(copy_text("beFiles.batch.mintSpent", &[])),
                            ),
                        )));
                        continue;
                    }
                }
            }
        }
        for e in walked {
            let (dir, leaf, k) = if e.rel.is_empty() {
                (to_dir.to_string(), name.clone(), key.clone())
            } else {
                let parent = Path::new(&e.rel)
                    .parent()
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let mut d = join(to_dir, &name);
                if !parent.is_empty() {
                    d = join(&d, &parent);
                }
                (d, base(&e.rel).to_string(), format!("{key}/{}", e.rel))
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
    let landed = |key: &str, at: String, bytes: Option<u64>| BatchOutcome {
        path: Some(cwf::WirePath::from(at.as_str())),
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

// ═══ 生产那一份两头：SFTP ＋ 问落点那台后端 ════════════════════════════════════════════

/// 两台都已连上：源那台读 · 落点那台写暂存 ＋ 经它后端建目录与提交。本机暂存经写句柄。
pub(crate) struct Wired {
    face: crate::platform::fs::Face,
    from: crate::dial::sftp::Session,
    to: crate::dial::sftp::Session,
    to_dial: serde_json::Value,
    tz: crate::Tz,
    /// 问落点那台后端经它（生产 [`crate::dial::remote_ask::DialRemote`]）。
    remote: Box<dyn crate::dial::remote_ask::Remote>,
    /// 本机暂存区（`$HOME/.cc-monitor/staging`）。
    staging: PathBuf,
}

/// 问落点那台后端一条命令（经那条连接的 CLI 面；入参类型选中那条命令）。
async fn ask_to<C: cwf::Call>(w: &Wired, req: &C) -> Result<C::Reply, Miss> {
    crate::dial::remote_ask::ask_dialed(&w.to_dial, req, &w.tz, w.remote.as_ref())
        .await
        .map_err(|s| {
            let code = match s.code.as_deref() {
                Some("exists") => "exists",
                Some("refused") => "refused",
                Some("stale") => "stale",
                Some("bad_name") => "bad_name",
                _ => "io_failed",
            };
            (code, Said::from(s.message))
        })
}

impl Wired {
    /// 连上两台、问落点那台后端的家目录（与它的 SFTP 起始目录比）、备好本机暂存区。
    pub(crate) async fn open(
        face: crate::platform::fs::Face,
        from: &serde_json::Value,
        to: &serde_json::Value,
        tz: crate::Tz,
    ) -> Result<Wired, CopyErr> {
        use crate::dial::sftp::{open_for_transfer, Dial};
        let fd = Dial::parse(from).map_err(CopyErr::BadArgs)?;
        let td = Dial::parse(to).map_err(CopyErr::BadArgs)?;
        let unreach =
            |s: Said| CopyErr::Unreachable(crate::common::contract::joined(s.said, s.raw));
        let from_s = open_for_transfer(&fd).await.map_err(unreach)?;
        let to_s = open_for_transfer(&td).await.map_err(unreach)?;
        let home = crate::platform::paths::home_dir()
            .ok_or_else(|| CopyErr::Unreachable(copy_text("beTransferCopy.local.noHome", &[])))?;
        let remote = Box::new(crate::dial::remote_ask::DialRemote);
        Wired::over(face, from_s, to_s, to.clone(), tz, &home, remote).await
    }

    /// 两条会话已开：备好本机暂存区（`home` 底下）、问落点那台后端的家目录（与它的 SFTP 起始目录比）。
    pub(crate) async fn over(
        face: crate::platform::fs::Face,
        from: crate::dial::sftp::Session,
        to: crate::dial::sftp::Session,
        to_dial: serde_json::Value,
        tz: crate::Tz,
        home: &Path,
        remote: Box<dyn crate::dial::remote_ask::Remote>,
    ) -> Result<Wired, CopyErr> {
        let staging = home.join(relay_route_core::STAGING_DIR_REL);
        if crate::platform::fs::Ro::symlink_metadata(&staging).is_err() {
            if let Ok(at) = crate::common::root_path::walk_in_root(
                crate::common::root_path::Last::Link,
                home,
                relay_route_core::STAGING_DIR_REL,
            ) {
                let _ = face.own_dir(&at);
            }
        }
        let w = Wired {
            face,
            from,
            to,
            to_dial,
            tz,
            remote,
            staging,
        };
        let h = ask_to(&w, &cwf::HomeReq {})
            .await
            .map_err(|(_, s)| CopyErr::Unreachable(s.said))?;
        let bhome = String::from_utf8_lossy(h.path.bytes());
        if let Some(why) = super::transfer::start_dir_mismatch(w.to.home(), &bhome) {
            return Err(CopyErr::SftpHomeMismatch(why));
        }
        Ok(w)
    }
}

/// 源那台上一格是不是目录（列得出 ⇒ 目录）。
async fn listed(s: &crate::dial::sftp::Session, path: &str) -> Option<Vec<(String, bool, u64)>> {
    crate::dial::sftp::list_entries(s, path).await.ok()
}

impl Ends for Wired {
    fn walk<'a>(&'a self, path: &'a str) -> Fut<'a, Result<Vec<Entry>, Miss>> {
        Box::pin(async move {
            let Some(top) = listed(&self.from, path).await else {
                if crate::dial::sftp::exists(&self.from, path).await == Some(true) {
                    return Ok(vec![Entry {
                        rel: String::new(),
                        dir: false,
                        size: crate::dial::sftp::metadata_size(&self.from, path)
                            .await
                            .flatten()
                            .unwrap_or(0),
                    }]);
                }
                return Err((
                    "io_failed",
                    Said::from(copy_text("beTransferCopy.from.missing", &[("path", path)])),
                ));
            };
            let mut out = vec![Entry {
                rel: String::new(),
                dir: true,
                size: 0,
            }];
            let mut stack: Vec<(String, Vec<(String, bool, u64)>)> = vec![(String::new(), top)];
            while let Some((rel, mut kids)) = stack.pop() {
                kids.sort();
                for (name, dir, size) in kids {
                    let r = if rel.is_empty() {
                        name
                    } else {
                        format!("{rel}/{name}")
                    };
                    out.push(Entry {
                        rel: r.clone(),
                        dir,
                        size: if dir { 0 } else { size },
                    });
                    if dir {
                        let sub = listed(&self.from, &join(path, &r))
                            .await
                            .unwrap_or_default();
                        stack.push((r, sub));
                    }
                }
            }
            // 先序：目录在它的内容之前（按相对段排，父段恒在子段之前）。
            out.sort_by(|a, b| a.rel.cmp(&b.rel));
            Ok(out)
        })
    }

    fn there<'a>(&'a self, path: &'a str) -> Fut<'a, Option<bool>> {
        Box::pin(async move {
            match crate::dial::sftp::exists(&self.to, path).await {
                Some(true) => Some(listed(&self.to, path).await.is_some()),
                _ => None,
            }
        })
    }

    fn mkdir<'a>(&'a self, dir: &'a str, name: &'a str) -> Fut<'a, Result<(), Miss>> {
        Box::pin(async move {
            let req = cwf::FilesMkdirReq {
                root: dir.into(),
                rel: name.into(),
                single: None,
            };
            ask_to(self, &req).await.map(|_| ())
        })
    }

    fn copy_file<'a>(
        &'a self,
        from: &'a str,
        dir: &'a str,
        name: &'a str,
        overwrite: bool,
        stop: &'a (dyn Fn() -> bool + Send + Sync),
        sent: &'a (dyn Fn(u64) + Send + Sync),
    ) -> Fut<'a, Result<u64, Miss>> {
        Box::pin(async move {
            use super::transfer::{
                download_to_local, staging_key, staging_part, upload_to_staging, Cancel,
            };
            let nanos = crate::common::time::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0);
            let key = staging_key(&format!("{from}\u{0}{dir}/{name}"), 0, nanos);
            let local = self.staging.join(format!("{key}.copy"));
            let cancel = Cancel::default();
            // 两腿各占这一份的一半：下那一腿报 got/2，传那一腿报 (总 ＋ got)/2。
            let leg = std::sync::atomic::AtomicBool::new(false);
            let sink = |got: u64, total: u64| {
                if stop() {
                    cancel.fire();
                }
                let up = leg.load(std::sync::atomic::Ordering::Relaxed);
                sent(if up { (total + got) / 2 } else { got / 2 });
            };
            let io = |s: Said| ("io_failed", s);
            let r: Result<u64, Miss> = async {
                download_to_local(&self.face, &self.from, from, &local, &cancel, &sink)
                    .await
                    .map_err(io)?;
                leg.store(true, std::sync::atomic::Ordering::Relaxed);
                let local_s = local.to_string_lossy().into_owned();
                let (n, sha) = upload_to_staging(&self.to, &local_s, &key, &cancel, &sink)
                    .await
                    .map_err(io)?;
                let req = cwf::FilesCommitUploadReq {
                    root: dir.into(),
                    rel: name.into(),
                    key: key.clone(),
                    on_exists: overwrite.then_some(cmd_wire::batch::OnExists::Overwrite),
                    expect: cwf::Sha256Expect {
                        sha256: cwf::Sha256Hex(sha),
                    },
                    chunks: None,
                    bytes: None,
                    single: Some(true),
                };
                ask_to(self, &req).await?;
                Ok(n)
            }
            .await;
            // 本机那一份不论成败都删；没成 ⇒ 落点那台的暂存件也删。
            if let Ok(at) = crate::common::root_path::walk_in_root(
                crate::common::root_path::Last::Link,
                &self.staging,
                format!("{key}.copy"),
            ) {
                let _ = self.face.delete_file(&at);
            }
            if r.is_err() {
                let _ = crate::dial::sftp::remove(&self.to, &staging_part(&key)).await;
            }
            r
        })
    }
}

/// 帧面硬臂那一发的本体（阻塞线程上，经 `typed::answer_blocking`）：连上两台 → 做一组 → 回包。
pub(crate) fn run(
    face: crate::platform::fs::Face,
    req: <TransferCopy as typed::Command>::Req,
    cx: typed::Cx,
) -> Result<<TransferCopy as typed::Command>::Reply, <TransferCopy as typed::Command>::Err> {
    let rt = tokio::runtime::Handle::current();
    let ends = rt.block_on(Wired::open(face, &req.from, &req.to, cx.tz))?;
    copy_with(&ends, &rt, req)
}

#[cfg(test)]
#[path = "../../../tests/backend/control/transfer_copy_tests.rs"]
mod tests;
