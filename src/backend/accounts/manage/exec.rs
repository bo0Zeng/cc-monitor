//! **执行与备份**：按一份计划落盘 —— 每一步都经这台的文件管理面（[`Door`]：`files-mkdir` · `Door::link` · `files-copy` ·
//! `files-rename` · `files-delete` · `files-chmod` · `Door::put`），判据给的是落在临时家目录上的同一扇门。
//!
//! 先备份再改：一趟改动之前在 `<账号库>/.backup-<时间戳>/` 建一份备份（`0700`）；每一步动一份既有的东西之前先把它原样
//! 拷进 `root/<它的绝对路径>`，**这一步真做成了之后**才往 `undo.tsv` 记一行（`RESTORE\t<路径>` = 还原那一份 ·
//! `DELETE\t<路径>` = 删掉这一趟新建的）；同一个文件系统里挪（`Move`）是改名、不丢东西 ⇒ 不拷，记 `UNMOVE\t<原处>\t<挪到的那一处>`
//! （回滚从挪到的那一处拷回原处）⇒ 撤销清单里的每一行都对应一次真发生过的改动，中途停下也能回滚。
//! 回滚（[`rollback`]）按撤销清单倒着来：先把该还原的都还原（现场的那一份先挪进 `pre-rollback/`，不直接删），
//! 再删这一趟新建的；每一条自己的错误不挡别的条。
//!
//! key 表（上游选择自己的状态）是唯一不经文件管理面写的那一份：删号清它那一行、回滚放回那一行，都经门递进来的
//! [`KeyTable`] 那两口（写者只有它自己那一处）。备份照旧是整份拷进 `root/`，撤销清单记 `REKEY\t<表>\t<号的目录>`，
//! 回滚只从备份那一份里取回这一个号那一行，表里别的行不动。

use super::layout::{describe, Op, Plan};
use super::model::Manifest;
use super::scan::{is_under, item_at, join, Roots, BACKUP_PREFIX, ROLLED_BACK_NAME, UNDO_NAME};
use crate::common::door::{self, Door, Refused};
use crate::platform::acct_view::Item;
use copy_core::copy_text;

/// 一趟执行的结局。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Applied {
    /// 这一趟留的备份（`.backup-` 之后那一段）；计划是空的 ⇒ `None`。
    pub backup: Option<String>,
    pub steps: Vec<String>,
}

/// 执行失败：那一句（带着「做到第几步 · 用哪份备份回滚」）。
pub(crate) type Failed = (&'static str, String);

/// 这台 key 表的两口（账号库不认识它的格式、也不自己写它；门递进来）。
pub(crate) struct KeyTable<'a> {
    /// 那份文件的绝对路径。
    pub(crate) path: String,
    /// 它此刻有哪几个号（账号 id）。
    pub(crate) ids: Vec<String>,
    /// 摘掉 `configDir` 那个号那一行。
    pub(crate) drop: &'a dyn Fn(&str) -> Result<(), String>,
    /// 从 `from`（备份里那一份）把 `configDir` 那个号那一行放回去。
    pub(crate) restore: &'a dyn Fn(&str, &str) -> Result<(), String>,
}

struct Run<'a> {
    d: &'a dyn Door,
    r: &'a Roots,
    keys: Option<&'a KeyTable<'a>>,
    /// 备份目录（绝对）。
    bk: String,
    undo: String,
    undo_written: bool,
}

impl Run<'_> {
    fn rel(&self, abs: &str) -> Result<String, String> {
        door::rel_under(&self.r.home, abs)
    }

    /// 逐级补齐 `abs` 这个目录（每一级只建最后一段）。
    fn mkdirs(&self, abs: &str) -> Result<(), String> {
        mkdirs(self.d, &self.r.home, abs)
    }

    fn chmod(&self, abs: &str, mode: u32) -> Result<(), String> {
        door::chmod(self.d, &self.r.home, &self.rel(abs)?, mode)
    }

    /// 把 `p` 原样拷进备份（同一路径只拷第一次：留住最原始的那一份）。不在 ⇒ 什么都不做。
    fn backup(&self, p: &str) -> Result<(), String> {
        let it = item_at(p);
        if !it.exists() {
            return Ok(());
        }
        let dest = format!("{}/root{p}", self.bk);
        if item_at(&dest).exists() {
            return Ok(());
        }
        if let Some((parent, _)) = dest.rsplit_once('/') {
            self.mkdirs(parent)?;
        }
        place_copy(self.d, &self.r.home, &it, p, &dest)
    }

    /// 记一行撤销并立刻落盘（整份重写 `undo.tsv`，比对上一次写进去的那一份）。
    fn record(&mut self, lines: &[(&str, &str)]) -> Result<(), String> {
        let before = self.undo.clone();
        // 回滚只删账号库里的东西（[`delete_one`]）⇒ 账号库外这一趟新建的（共享库里按需建的目录 · 挪进共享库的那一份）不记删：
        // 回滚留着它（Claude Code 自己头一回用也会建它），号里那一份照备份取回。
        for (op, p) in lines
            .iter()
            .filter(|(op, p)| *op != "DELETE" || is_under(p, &self.r.accts))
        {
            self.undo.push_str(&format!("{op}\t{p}\n"));
        }
        let rel = self.rel(&join(&self.bk, UNDO_NAME))?;
        let expect = self.undo_written.then_some(before.as_str());
        door::put(self.d, &self.r.home, &rel, &self.undo, expect, false, false)
            .map_err(Refused::said)?;
        self.undo_written = true;
        Ok(())
    }

    fn op(&mut self, op: &Op, manifest: Option<(&str, Option<&str>)>) -> Result<(), String> {
        let (d, home) = (self.d, self.r.home.clone());
        match op {
            Op::MkDir(p) => match item_at(p) {
                Item::Dir { .. } => self.chmod(p, 0o700),
                Item::Absent => {
                    door::mkdir(d, &home, &self.rel(p)?)?;
                    self.record(&[("DELETE", p)])?;
                    self.chmod(p, 0o700)
                }
                _ => Err(copy_text("beAcctExec.mkdir.notDir", &[("path", p)])),
            },
            Op::Link { target, at } => {
                door::link(d, &home, &self.rel(at)?, target)?;
                self.record(&[("DELETE", at)])
            }
            Op::Relink { target, at } => {
                self.backup(at)?;
                door::remove(d, &home, &self.rel(at)?, false)?;
                door::link(d, &home, &self.rel(at)?, target)?;
                self.record(&[("RESTORE", at)])
            }
            Op::Move { from, to } => {
                let to_dir = to.rsplit_once('/').map_or(to.as_str(), |(p, _)| p);
                let dev = |p: &str| crate::platform::paths::device_of(std::path::Path::new(p));
                let keep = move_keeps_backup(dev(from), dev(to_dir));
                if keep {
                    self.backup(from)?;
                }
                door::rename(d, &home, &self.rel(from)?, &self.rel(to)?)?;
                if keep {
                    self.record(&[("RESTORE", from), ("DELETE", to)])
                } else {
                    self.record(&[("UNMOVE", &format!("{from}\t{to}")), ("DELETE", to)])
                }
            }
            Op::Isolate { from, at } => self.isolate(from, at),
            Op::Copy { from, to } => {
                let tmp = format!("{to}.ccm-tmp-{}", std::process::id());
                door::copy(d, &home, &self.rel(from)?, &self.rel(&tmp)?)?;
                let landed = self
                    .chmod(&tmp, 0o600)
                    .and_then(|()| door::rename(d, &home, &self.rel(&tmp)?, &self.rel(to)?));
                if let Err(e) = landed {
                    let _ = door::remove(d, &home, &self.rel(&tmp)?, false);
                    return Err(e);
                }
                self.record(&[("DELETE", to)])
            }
            Op::Remove(p) => {
                if !is_under(p, &self.r.accts) || *p == self.r.accts {
                    return Err(copy_text(
                        "beAcctExec.remove.outside",
                        &[("path", p), ("accts", &self.r.accts)],
                    ));
                }
                self.backup(p)?;
                let recursive = matches!(item_at(p), Item::Dir { .. });
                door::remove(d, &home, &self.rel(p)?, recursive)?;
                self.record(&[("RESTORE", p)])
            }
            Op::Chmod { mode, at } => match item_at(at) {
                Item::File { .. } | Item::Dir { .. } => self.chmod(at, *mode),
                _ => Ok(()),
            },
            Op::DropKey { table, config_dir } => {
                let k = self
                    .keys
                    .filter(|k| k.path == *table)
                    .ok_or_else(|| copy_text("beAcctExec.key.noDoor", &[("path", table)]))?;
                self.backup(table)?;
                (k.drop)(config_dir)?;
                self.record(&[("REKEY", &format!("{table}\t{config_dir}"))])
            }
            Op::WriteManifest => {
                let (text, before) = manifest.ok_or_else(|| copy_text("reason.code.bug", &[]))?;
                let path = self.r.manifest();
                let existed = item_at(&path).exists();
                if existed {
                    self.backup(&path)?;
                }
                door::put(d, &home, &self.rel(&path)?, text, before, false, false).map_err(
                    |e| match e {
                        Refused::Stale(_) => copy_text("beAcctExec.manifest.stale", &[]),
                        other => other.said(),
                    },
                )?;
                self.record(&[(if existed { "RESTORE" } else { "DELETE" }, &path)])?;
                self.chmod(&path, 0o600)
            }
        }
    }

    /// 链接（或空位）换成共享库那一份的私有副本：先在旁边复制一份、核共享库那一份在复制期间没被改过、
    /// 摘掉链接（绝不跟着链接写回共享库）、换名上位、再核一次落位的不是链接且大小对得上；不对 ⇒ 还原成链接。
    fn isolate(&mut self, from: &str, at: &str) -> Result<(), String> {
        let (d, home) = (self.d, self.r.home.clone());
        let sig = |it: &Item| match it {
            Item::File { size, mtime, .. } => Some((*size, *mtime)),
            _ => None,
        };
        let src = item_at(from);
        if !src.exists() {
            return Err(copy_text("beAcctExec.isolate.noSource", &[("path", from)]));
        }
        let was = item_at(at);
        self.backup(at)?;
        let tmp = format!("{at}.ccm-tmp-{}", std::process::id());
        let dir = matches!(src, Item::Dir { .. });
        door::copy(d, &home, &self.rel(from)?, &self.rel(&tmp)?)?;
        let drop_tmp = |r: &Run| {
            if let Ok(rel) = r.rel(&tmp) {
                let _ = door::remove(r.d, &r.r.home, &rel, dir);
            }
        };
        if !dir {
            if let Err(e) = self.chmod(&tmp, 0o600) {
                drop_tmp(self);
                return Err(e);
            }
        }
        if sig(&item_at(from)) != sig(&src) {
            drop_tmp(self);
            return Err(copy_text("beAcctExec.isolate.changed", &[("path", from)]));
        }
        if was.is_link() {
            if let Err(e) = door::remove(d, &home, &self.rel(at)?, false) {
                drop_tmp(self);
                return Err(e);
            }
        }
        door::rename(d, &home, &self.rel(&tmp)?, &self.rel(at)?)?;
        let now = item_at(at);
        let ok = !now.is_link()
            && match (&now, &src) {
                (Item::File { size: a, .. }, Item::File { size: b, .. }) => a == b,
                (Item::Dir { .. }, Item::Dir { .. }) => true,
                _ => false,
            };
        if !ok {
            let _ = door::remove(d, &home, &self.rel(at)?, dir);
            if let Item::Link { target, .. } = &was {
                let _ = door::link(d, &home, &self.rel(at)?, target);
            }
            return Err(copy_text("beAcctExec.isolate.selfCheck", &[("path", at)]));
        }
        self.record(&[(if was.exists() { "RESTORE" } else { "DELETE" }, at)])
    }
}

/// 逐级补齐 `abs` 这个目录（家目录底下；每一级经 `files-mkdir` 只建最后一段）。
fn mkdirs(d: &dyn Door, home: &str, abs: &str) -> Result<(), String> {
    if abs.trim_end_matches('/') == home.trim_end_matches('/') {
        return Ok(());
    }
    let rel = door::rel_under(home, abs)?;
    let mut cur = home.trim_end_matches('/').to_string();
    let mut sofar = String::new();
    for seg in rel.split('/').filter(|s| !s.is_empty()) {
        cur = join(&cur, seg);
        sofar = if sofar.is_empty() {
            seg.to_string()
        } else {
            format!("{sofar}/{seg}")
        };
        match item_at(&cur) {
            Item::Dir { .. } => {}
            Item::Absent => door::mkdir(d, home, &sofar)?,
            _ => return Err(copy_text("beAcctExec.mkdir.notDir", &[("path", &cur)])),
        }
    }
    Ok(())
}

/// 把 `from`（此刻是 `it`）原样放到 `to`：链接 ⇒ 同一段目标文本新建一条；文件 ⇒ 复制（权限位随它）；目录 ⇒ 整棵复制。
fn place_copy(d: &dyn Door, home: &str, it: &Item, from: &str, to: &str) -> Result<(), String> {
    let to_rel = door::rel_under(home, to)?;
    match it {
        Item::Link { target, .. } => door::link(d, home, &to_rel, target),
        Item::File { .. } | Item::Dir { .. } => {
            door::copy(d, home, &door::rel_under(home, from)?, &to_rel)
        }
        Item::Absent | Item::Other => Err(copy_text("beAcctExec.copy.kind", &[("path", from)])),
    }
}

/// 按计划落盘。`manifest_before` = 读快照时清单的原文（`None` = 当时不在）：写回那一下比对它，中间被别人改过 ⇒ 这一步拒。
pub(crate) fn apply(
    d: &dyn Door,
    r: &Roots,
    plan: &Plan,
    manifest_before: Option<&str>,
    render: &dyn Fn(&Manifest) -> String,
    keys: Option<&KeyTable>,
) -> Result<Applied, Failed> {
    let steps: Vec<String> = plan
        .ops
        .iter()
        .map(|op| describe(op, &r.manifest()))
        .collect();
    if plan.ops.is_empty() {
        return Ok(Applied {
            backup: None,
            steps,
        });
    }
    let fail = |e: String| ("io_failed", e);
    // 账号库自己先在（备份住在它里面）：计划里建它那一步在这里先做，且**不记撤销** —— 回滚删了它就连备份一起删了。
    if !item_at(&r.accts).exists() {
        mkdirs(d, &r.home, &r.accts).map_err(fail)?;
    }
    door::chmod(
        d,
        &r.home,
        &door::rel_under(&r.home, &r.accts).map_err(fail)?,
        0o700,
    )
    .map_err(fail)?;
    let id = backup_id(r);
    let bk = join(&r.accts, &format!("{BACKUP_PREFIX}{id}"));
    let bk_rel = door::rel_under(&r.home, &bk).map_err(fail)?;
    door::mkdir(d, &r.home, &bk_rel).map_err(fail)?;
    door::chmod(d, &r.home, &bk_rel, 0o700).map_err(fail)?;
    door::mkdir(d, &r.home, &format!("{bk_rel}/root")).map_err(fail)?;
    let mut run = Run {
        d,
        r,
        keys,
        bk,
        undo: String::new(),
        undo_written: false,
    };
    run.record(&[]).map_err(fail)?;
    let text = plan.manifest.as_ref().map(render);
    for (i, op) in plan.ops.iter().enumerate() {
        let m = text.as_deref().map(|t| (t, manifest_before));
        if let Err(e) = run.op(op, m) {
            return Err((
                "io_failed",
                copy_text(
                    "beAcctExec.apply.stopped",
                    &[
                        ("step", &(i + 1).to_string()),
                        ("of", &plan.ops.len().to_string()),
                        ("what", &steps[i]),
                        ("why", &e),
                        ("backup", &id),
                    ],
                ),
            ));
        }
    }
    Ok(Applied {
        backup: Some(id),
        steps,
    })
}

/// 挪之前要不要先整份拷进备份：同一个文件系统里挪是改名、不丢东西 ⇒ 不拷（回滚从挪到的那一处拷回来，[`Undo::Back`]）；
/// 跨文件系统 · 认不出在哪个盘 ⇒ 照旧先拷。
pub(crate) fn move_keeps_backup(from_dev: Option<u64>, to_dev: Option<u64>) -> bool {
    !matches!((from_dev, to_dev), (Some(a), Some(b)) if a == b)
}

/// 备份目录那一段名字：UTC `YYYYMMDD-HHMMSS`；同一秒里已有 ⇒ 后面接 `-2` `-3` …
fn backup_id(r: &Roots) -> String {
    let base = utc_stamp(false);
    let mut id = base.clone();
    let mut n = 2;
    while item_at(&join(&r.accts, &format!("{BACKUP_PREFIX}{id}"))).exists() {
        id = format!("{base}-{n}");
        n += 1;
    }
    id
}

/// 此刻的 UTC 时间：`iso` ⇒ `YYYY-MM-DDTHH:MM:SSZ`（清单的 `updatedAt`）；否则 `YYYYMMDD-HHMMSS`（备份名）。
pub(crate) fn utc_stamp(iso: bool) -> String {
    let secs = crate::common::time::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let (days, rem) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    let (y, m, d) = crate::common::time::civil_from_days(days);
    let (hh, mm, ss) = (rem / 3600, rem % 3600 / 60, rem % 60);
    if iso {
        format!("{y:04}-{m:02}-{d:02}T{hh:02}:{mm:02}:{ss:02}Z")
    } else {
        format!("{y:04}{m:02}{d:02}-{hh:02}{mm:02}{ss:02}")
    }
}

// ─────────────────────────────── 回滚 ───────────────────────────────

/// 撤销清单里的一条。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Undo {
    Restore(String),
    /// 同盘挪走的那一份：从挪到的那一处（`from`）拷回原处（`at`），挪到的那一处留着。
    Back {
        at: String,
        from: String,
    },
    Delete(String),
    /// 把 `config_dir` 那个号那一行放回 key 表 `table`。
    Rekey {
        table: String,
        config_dir: String,
    },
}

/// 备份名能不能用：只许 `[0-9A-Za-z._-]`、不含 `..`（挡目录穿越）。
pub(crate) fn backup_id_ok(id: &str) -> bool {
    !id.is_empty()
        && !id.contains("..")
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
}

/// 撤销清单 ⇒ 回滚要做的那几条（**纯**）：先还原（倒序）、再删（倒序）—— 先救数据再清新建的。认不出的行原样报出来。
/// 同一个路径既要还原又要删（先挪走、再在原处建了一条链接）⇒ 只还原：它在这一趟之前就在，不是这一趟新建的。
pub(crate) fn undo_steps(text: &str) -> (Vec<Undo>, Vec<String>) {
    let mut restore = Vec::new();
    let mut delete = Vec::new();
    let mut bad = Vec::new();
    for line in text.lines().filter(|l| !l.is_empty()) {
        match line.split_once('\t') {
            Some(("RESTORE", p)) if p.starts_with('/') => {
                restore.push(Undo::Restore(p.to_string()))
            }
            Some(("DELETE", p)) if p.starts_with('/') => delete.push(Undo::Delete(p.to_string())),
            Some(("UNMOVE", rest)) => match rest.split_once('\t') {
                Some((at, from)) if at.starts_with('/') && from.starts_with('/') => {
                    restore.push(Undo::Back {
                        at: at.to_string(),
                        from: from.to_string(),
                    })
                }
                _ => bad.push(line.to_string()),
            },
            Some(("REKEY", rest)) => match rest.split_once('\t') {
                Some((t, c)) if t.starts_with('/') && c.starts_with('/') => {
                    restore.push(Undo::Rekey {
                        table: t.to_string(),
                        config_dir: c.to_string(),
                    })
                }
                _ => bad.push(line.to_string()),
            },
            _ => bad.push(line.to_string()),
        }
    }
    let restored: Vec<String> = restore
        .iter()
        .filter_map(|u| match u {
            Undo::Restore(p) | Undo::Back { at: p, .. } => Some(p.clone()),
            _ => None,
        })
        .collect();
    delete.retain(|u| !matches!(u, Undo::Delete(p) if restored.contains(p)));
    restore.reverse();
    delete.reverse();
    restore.extend(delete);
    (restore, bad)
}

/// 一条回滚给人看的那一句。
pub(crate) fn describe_undo(u: &Undo) -> String {
    match u {
        Undo::Restore(p) | Undo::Back { at: p, .. } => {
            copy_text("beAcctExec.rollback.restore", &[("path", p)])
        }
        Undo::Delete(p) => copy_text("beAcctExec.rollback.delete", &[("path", p)]),
        Undo::Rekey { table, config_dir } => copy_text(
            "beAcctExec.rollback.rekey",
            &[("dir", config_dir), ("path", table)],
        ),
    }
}

/// 还原的目标只许落在账号库 · 共享库 · 家目录底下（不是家目录本身）。
fn restore_target_ok(r: &Roots, p: &str) -> bool {
    is_under(p, &r.accts) || is_under(p, &r.shared) || (is_under(p, &r.home) && p != r.home)
}

/// 按一份备份回滚（`bk` = 备份目录的绝对路径）。回 `(做了的那几句, 没做成的那几句)`；全做成 ⇒ 留 `.rolled-back` 标记。
pub(crate) fn rollback(
    d: &dyn Door,
    r: &Roots,
    bk: &str,
    steps: &[Undo],
    keys: Option<&KeyTable>,
) -> (Vec<String>, Vec<String>) {
    let mut done = Vec::new();
    let mut failed = Vec::new();
    let aside = join(bk, "pre-rollback");
    for u in steps {
        let res = match u {
            Undo::Restore(p) => restore_one(d, r, &format!("{bk}/root{p}"), &aside, p),
            Undo::Back { at, from } if restore_target_ok(r, from) => {
                restore_one(d, r, from, &aside, at)
            }
            Undo::Back { from, .. } => {
                Err(copy_text("beAcctExec.rollback.outside", &[("path", from)]))
            }
            Undo::Delete(p) => delete_one(d, r, p),
            Undo::Rekey { table, config_dir } => rekey_one(r, bk, keys, table, config_dir),
        };
        match res {
            Ok(()) => done.push(describe_undo(u)),
            Err(e) => failed.push(format!("{}：{e}", describe_undo(u))),
        }
    }
    if failed.is_empty() {
        let rel = door::rel_under(&r.home, &join(bk, ROLLED_BACK_NAME));
        if let Err(e) = rel.and_then(|rel| {
            door::put(d, &r.home, &rel, "", None, false, false)
                .map_err(Refused::said)
                .map(|_| ())
        }) {
            failed.push(e);
        }
    }
    (done, failed)
}

/// 从备份里那一份 key 表取回一个号那一行（号的目录只许在账号库里；表得是门递进来的那一份）。
fn rekey_one(
    r: &Roots,
    bk: &str,
    keys: Option<&KeyTable>,
    table: &str,
    config_dir: &str,
) -> Result<(), String> {
    if !is_under(config_dir, &r.accts) || config_dir == r.accts {
        return Err(copy_text(
            "beAcctExec.rollback.outside",
            &[("path", config_dir)],
        ));
    }
    let k = keys
        .filter(|k| k.path == table)
        .ok_or_else(|| copy_text("beAcctExec.key.noDoor", &[("path", table)]))?;
    let saved = format!("{bk}/root{table}");
    if !item_at(&saved).exists() {
        return Err(copy_text(
            "beAcctExec.rollback.notSaved",
            &[("path", table)],
        ));
    }
    (k.restore)(config_dir, &saved)
}

/// 把 `saved`（备份里那一份，或同盘挪到的那一处）原样拷回 `p`；`p` 上已有的先挪到 `aside` 底下。
fn restore_one(d: &dyn Door, r: &Roots, saved: &str, aside: &str, p: &str) -> Result<(), String> {
    if !restore_target_ok(r, p) {
        return Err(copy_text("beAcctExec.rollback.outside", &[("path", p)]));
    }
    let it = item_at(saved);
    if !it.exists() {
        return Err(copy_text("beAcctExec.rollback.notSaved", &[("path", p)]));
    }
    let (parent, _) = p
        .rsplit_once('/')
        .ok_or_else(|| copy_text("beAcctExec.rollback.outside", &[("path", p)]))?;
    let parent = if parent.is_empty() { "/" } else { parent };
    if item_at(p).exists() {
        let dest = format!("{aside}{p}");
        let mut n = 2;
        let mut dest_free = dest.clone();
        while item_at(&dest_free).exists() {
            dest_free = format!("{dest}.{n}");
            n += 1;
        }
        if let Some((ap, _)) = dest_free.rsplit_once('/') {
            mkdirs(d, &r.home, ap)?;
        }
        door::rename(
            d,
            &r.home,
            &door::rel_under(&r.home, p)?,
            &door::rel_under(&r.home, &dest_free)?,
        )?;
    }
    mkdirs(d, &r.home, parent)?;
    place_copy(d, &r.home, &it, saved, p)
}

fn delete_one(d: &dyn Door, r: &Roots, p: &str) -> Result<(), String> {
    if !is_under(p, &r.accts) {
        return Err(copy_text(
            "beAcctExec.remove.outside",
            &[("path", p), ("accts", &r.accts)],
        ));
    }
    match item_at(p) {
        Item::Absent => Ok(()),
        it => door::remove(
            d,
            &r.home,
            &door::rel_under(&r.home, p)?,
            matches!(it, Item::Dir { .. }),
        ),
    }
}

#[cfg(test)]
#[path = "../../../../tests/backend/accounts/manage/exec_tests.rs"]
mod tests;
