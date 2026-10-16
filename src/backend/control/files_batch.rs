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

use std::path::{Path, PathBuf};

use cmd_wire::batch::{BatchOutcome, BatchReply, Conflict, Conflicts, Decide, ItemState};
use cmd_wire::files as wire;
use copy_core::copy_text;

use super::files_write::{
    change_mode_reporting, copy_entry, copy_tree, delete_empty_dir, delete_entry,
    delete_file_expecting, delete_tree_upto, lexical_in_root, rel_of, root_of, wire_of, WriteFail,
    WriteRefusal,
};
use crate::common::batch_book::{
    cancel_rest, choice_of, key_of, minted, moment_text, mtime_secs, said_of, ticketed, Run,
};
use crate::platform::fs::{Face, Ro};

use crate::common::batch_book::MINT_TRIES;
/// 递归删一棵树一段删几条（段与段之间看撤的旗、报进度）。
const DELETE_STRIDE: usize = 256;

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

// ═══ 复制 ═══════════════════════════════════════════════════════════════════════════

/// 盘上那一格是什么（不跟链接）＋ 大小 · 修改时刻。
pub(crate) fn look(p: &Path) -> Option<(wire::FileKind, Option<u64>, Option<i64>)> {
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
    Some((kind, size, mtime_secs(m.modified())))
}

/// 一项同名时的答 ＋ 那一格是不是目录（不合并）。
struct Clash {
    choice: Option<Decide>,
    dir_into: bool,
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
/// 同名清单里的时刻按 `tz`（看的那一台的时区）写好。
pub(crate) fn copy(
    face: &Face,
    req: wire::FilesCopyReq,
    tz: &crate::common::time::Tz,
) -> Result<BatchReply, CopyRefused> {
    let root = root_of(&req.root, "root")?;
    let on_exists = req.on_exists.unwrap_or_default();
    let items: Vec<(PathBuf, PathBuf)> = req
        .items
        .iter()
        .map(|i| (rel_of(&i.from), rel_of(&i.to)))
        .collect();
    if req.single == Some(true) {
        for (from, to) in &items {
            super::files_write::names_ok(true, to).map_err(CopyRefused::Fail)?;
            if from == to {
                return Err(CopyRefused::Fail(WriteFail::from((
                    super::files_write::BAD_NAME,
                    copy_text("beFilesWrite.copy.same", &[("path", &to.to_string_lossy())]),
                ))));
            }
        }
    }
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
                    mtime_text: moment_text(mtime, tz),
                    there,
                    there_size,
                    there_mtime_text: moment_text(there_mtime, tz),
                    answers: crate::common::batch_book::answers_of(dir_into, true),
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
    ticketed(
        req.ticket.as_deref(),
        total,
        |m| CopyRefused::Fail(WriteFail::from(("bad_args", m))),
        probe,
        |clashes, run| {
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
        },
    )
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
        |m| WriteFail::from(("bad_args", m)),
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
    let mode = super::files_write::mode_of_text(&req.mode)?;
    let items = req.items;
    ticketed(
        req.ticket.as_deref(),
        total,
        |m| WriteFail::from(("bad_args", m)),
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
        // 门不问人 ⇒ 同名清单（连同它的时刻字）不出门，时区照缺。
        copy(face, self, &Default::default()).map_err(|e| match e {
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
