//! `control/transfer_copy.rs` 的判据。
//! - 本体（`copy_with`）：两头用本机两个目录上的替身（[`Local`]）：源那台 ＝ `a/`，落点那台 ＝ `b/`；路径照远端那一形写（绝对路径的字节）。
//! - 生产那一份两头（[`Pair`]）：本机那一端是真的 [`Here`]；远端那一端是真的 [`There`]，问那台后端经一台假后端（[`Back`]：
//!   入参照真的解码器解，列 · 读块 · 建目录 · 送块 · 提交都走文件管理那一份真的，盘是一个临时目录），SFTP 腿接合成台架。

use super::*;
use cmd_wire::batch::OnExists;
use cmd_wire::transfer::CopyFrom;
use std::fs;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

#[cfg(unix)]
fn os(b: &[u8]) -> &std::ffi::OsStr {
    use std::os::unix::ffi::OsStrExt as _;
    std::ffi::OsStr::from_bytes(b)
}

fn pb(b: &[u8]) -> PathBuf {
    crate::common::path_wire::to_path_buf(b)
}

fn bytes_of(p: &Path) -> Vec<u8> {
    p.as_os_str().as_encoded_bytes().to_vec()
}

/// 本机目录上的两头。`files` 数传了几份；`after` 传完第几份时调一次（撤的判据在那儿立旗）。
struct Local {
    files: AtomicUsize,
    after: Box<dyn Fn(usize) + Send + Sync>,
}

impl Ends for Local {
    fn walk<'a>(&'a self, path: &'a [u8]) -> Fut<'a, Result<Vec<Entry>, Said>> {
        Box::pin(async move {
            let p = pb(path);
            let Ok(m) = fs::metadata(&p) else {
                return Err(Said::new("io_failed", format!("missing {}", p.display())));
            };
            let mut out = vec![Entry {
                rel: Vec::new(),
                dir: m.is_dir(),
                size: if m.is_dir() { 0 } else { m.len() },
            }];
            if m.is_dir() {
                // 源那台上的树按铺的时候那张清单答（判据不自己遍历目录）。
                for (rel, dir) in TREE {
                    let r = rel.strip_prefix("d/").unwrap_or(rel);
                    out.push(Entry {
                        rel: r.as_bytes().to_vec(),
                        dir: *dir,
                        size: fs::metadata(p.join(r)).map(|m| m.len()).unwrap_or(0),
                    });
                }
            }
            Ok(out)
        })
    }
    fn there<'a>(&'a self, dir: &'a [u8], name: &'a [u8]) -> Fut<'a, Option<bool>> {
        Box::pin(async move {
            fs::symlink_metadata(pb(dir).join(pb(name)))
                .ok()
                .map(|m| m.is_dir())
        })
    }
    fn mkdir<'a>(&'a self, dir: &'a [u8], name: &'a [u8]) -> Fut<'a, Result<(), Said>> {
        Box::pin(async move {
            fs::create_dir(pb(dir).join(pb(name)))
                .map_err(|e| Said::new("io_failed", e.to_string()))
        })
    }
    fn copy_file<'a>(
        &'a self,
        from: &'a [u8],
        dir: &'a [u8],
        name: &'a [u8],
        overwrite: bool,
        _stop: &'a Stop<'a>,
        sent: &'a (dyn Fn(u64) + Send + Sync),
    ) -> Fut<'a, Result<u64, Said>> {
        Box::pin(async move {
            let to = pb(dir).join(pb(name));
            if to.exists() && !overwrite {
                return Err(Said::new("exists", "exists".to_string()));
            }
            // 走到一半报一次（替身里「一半」＝ 一半字节），传完报整份。
            let len = fs::metadata(pb(from)).map(|m| m.len()).unwrap_or(0);
            sent(len / 2);
            let n = fs::copy(pb(from), &to).map_err(|e| Said::new("io_failed", e.to_string()))?;
            sent(n);
            let k = self.files.fetch_add(1, Ordering::SeqCst) + 1;
            (self.after)(k);
            Ok(n)
        })
    }
}

/// 铺在源那台上的树（相对 `a/`，先序）：[`rig`] 照它铺。
const TREE: &[(&str, bool)] = &[("d/s", true), ("d/s/y.txt", false), ("d/x.txt", false)];

fn local() -> Local {
    Local {
        files: AtomicUsize::new(0),
        after: Box::new(|_| {}),
    }
}

fn scratch(tag: &str) -> PathBuf {
    let base = std::env::temp_dir().join(format!("ccm-tc-{tag}-{}", std::process::id()));
    fs::remove_dir_all(&base).ok();
    fs::create_dir_all(&base).unwrap();
    base
}

/// 两头：`a/` 里一份文件 `f.txt` ＋ 一棵 `d/{x.txt, s/y.txt}`，`b/` 空。
fn rig(tag: &str) -> (PathBuf, PathBuf) {
    let base = scratch(tag);
    let (a, b) = (base.join("a"), base.join("b"));
    fs::create_dir_all(a.join("d/s")).unwrap();
    fs::create_dir_all(&b).unwrap();
    fs::write(a.join("f.txt"), b"F").unwrap();
    fs::write(a.join("d/x.txt"), b"X").unwrap();
    fs::write(a.join("d/s/y.txt"), b"YY").unwrap();
    (a, b)
}

fn copy_req(
    from: cmd_wire::Machine,
    to: cmd_wire::Machine,
    paths: Vec<Vec<u8>>,
    to_dir: Vec<u8>,
    on: OnExists,
    ticket: Option<&str>,
) -> TransferCopyReq {
    TransferCopyReq {
        from,
        to,
        items: paths
            .into_iter()
            .map(|p| CopyFrom {
                path: WirePath(p),
                name: None,
            })
            .collect(),
        to_dir: WirePath(to_dir),
        on_exists: Some(on),
        decide: None,
        ticket: ticket.map(str::to_string),
    }
}

fn req(a: &Path, b: &Path, names: &[&str], on: OnExists, ticket: Option<&str>) -> TransferCopyReq {
    copy_req(
        cmd_wire::Machine::named("a"),
        cmd_wire::Machine::named("b"),
        names.iter().map(|n| bytes_of(&a.join(n))).collect(),
        bytes_of(b),
        on,
        ticket,
    )
}

fn keyed(r: &BatchReply) -> Vec<(String, ItemState)> {
    r.outcomes
        .iter()
        .map(|o| (o.key.clone(), o.state))
        .collect()
}

fn on_rt<T: Send>(f: impl FnOnce(&tokio::runtime::Handle) -> T + Send) -> T {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .enable_all()
        .build()
        .unwrap();
    let h = rt.handle().clone();
    std::thread::scope(|s| s.spawn(|| f(&h)).join().unwrap())
}

#[test]
fn a_directory_goes_over_file_by_file_with_an_outcome_for_each() {
    let (a, b) = rig("tree");
    let ends = local();
    let r = on_rt(|rt| copy_with(&ends, rt, req(&a, &b, &["f.txt", "d"], OnExists::Ask, None)))
        .unwrap();
    assert_eq!(
        keyed(&r),
        vec![
            ("0".into(), ItemState::Done),
            ("1".into(), ItemState::Done),
            ("1/s".into(), ItemState::Done),
            ("1/s/y.txt".into(), ItemState::Done),
            ("1/x.txt".into(), ItemState::Done),
        ],
        "目录展开成建目录 ＋ 逐个文件，各一格结局"
    );
    assert_eq!(fs::read(b.join("d/s/y.txt")).unwrap(), b"YY");
    assert_eq!(fs::read(b.join("d/x.txt")).unwrap(), b"X");
    assert_eq!(fs::read(b.join("f.txt")).unwrap(), b"F");
    assert_eq!(ends.files.load(Ordering::SeqCst), 3);
}

/// 另存为的名字（`CopyFrom.name`）就是落点那一格的名字。
#[test]
fn an_item_lands_under_the_name_it_was_given() {
    let (a, b) = rig("named");
    let ends = local();
    let mut q = req(&a, &b, &["f.txt"], OnExists::Ask, None);
    q.items[0].name = Some(WirePath::from("g.txt"));
    let r = on_rt(|rt| copy_with(&ends, rt, q)).unwrap();
    assert_eq!(keyed(&r), vec![("0".into(), ItemState::Done)]);
    assert_eq!(fs::read(b.join("g.txt")).unwrap(), b"F");
    assert!(!b.join("f.txt").exists());
}

/// ★ 路径一路是字节：源那一份名字不是 UTF-8 ⇒ 落点那一格的名字字节原样、结局里那条路径也是那几个字节。
#[cfg(unix)]
#[test]
fn a_name_that_is_not_utf8_lands_byte_for_byte() {
    let (a, b) = rig("raw");
    let name: &[u8] = b"r\xff\xfe.bin";
    fs::write(a.join(os(name)), b"RAW").unwrap();
    let ends = local();
    let mut src = bytes_of(&a);
    src.push(b'/');
    src.extend_from_slice(name);
    let q = copy_req(
        cmd_wire::Machine::named("a"),
        cmd_wire::Machine::named("b"),
        vec![src],
        bytes_of(&b),
        OnExists::Ask,
        None,
    );
    let r = on_rt(|rt| copy_with(&ends, rt, q)).unwrap();
    assert_eq!(keyed(&r), vec![("0".into(), ItemState::Done)], "{r:?}");
    assert_eq!(fs::read(b.join(os(name))).unwrap(), b"RAW");
    let mut want = bytes_of(&b);
    want.push(b'/');
    want.extend_from_slice(name);
    assert_eq!(r.outcomes[0].path, Some(WirePath(want)));
}

#[test]
fn a_clash_on_the_target_leaves_every_byte_alone() {
    let (a, b) = rig("clash");
    fs::write(b.join("f.txt"), b"old").unwrap();
    let ends = local();
    let got = on_rt(|rt| copy_with(&ends, rt, req(&a, &b, &["d", "f.txt"], OnExists::Ask, None)));
    let Err(CopyErr::Exists(_, c)) = got else {
        panic!("有同名而没答 ⇒ exists ＋ 同名清单：{got:?}")
    };
    assert_eq!(c.conflicts.len(), 1);
    assert!(!b.join("d").exists(), "整组有同名时别的项也不先做");
    assert_eq!(fs::read(b.join("f.txt")).unwrap(), b"old");
}

#[test]
fn a_directory_clash_keeps_both_and_never_merges() {
    let (a, b) = rig("dirboth");
    fs::create_dir(b.join("d")).unwrap();
    fs::create_dir(b.join("d (2)")).unwrap();
    let ends = local();
    let over =
        on_rt(|rt| copy_with(&ends, rt, req(&a, &b, &["d"], OnExists::Overwrite, None))).unwrap();
    assert_eq!(
        keyed(&over),
        vec![("0".into(), ItemState::Skipped)],
        "目录同名「覆盖」⇒ 跳过（不合并）"
    );
    let r =
        on_rt(|rt| copy_with(&ends, rt, req(&a, &b, &["d"], OnExists::KeepBoth, None))).unwrap();
    assert!(
        r.outcomes.iter().all(|o| o.state == ItemState::Done),
        "{r:?}"
    );
    assert_eq!(fs::read(b.join("d (3)/s/y.txt")).unwrap(), b"YY");
    assert!(
        fs::remove_dir(b.join("d")).is_ok(),
        "原来那个目录一个字节没动"
    );
}

#[test]
fn stopping_cancels_the_files_not_yet_sent() {
    let (a, b) = rig("stop");
    let t = format!("tc-stop-{}", std::process::id());
    let tt = t.clone();
    let ends = Local {
        files: AtomicUsize::new(0),
        after: Box::new(move |k| {
            if k == 1 {
                crate::common::batch_book::stop(&tt);
            }
        }),
    };
    let seen = Arc::new(AtomicUsize::new(0));
    let s2 = seen.clone();
    let _ear = crate::common::batch_book::listen(
        &t,
        Box::new(move |_| {
            s2.fetch_add(1, Ordering::SeqCst);
        }),
    );
    let r = on_rt(|rt| {
        copy_with(
            &ends,
            rt,
            req(&a, &b, &["f.txt", "d"], OnExists::Ask, Some(&t)),
        )
    })
    .unwrap();
    assert_eq!(r.outcomes[0].state, ItemState::Done);
    assert!(
        r.outcomes[1..]
            .iter()
            .all(|o| o.state == ItemState::Cancelled),
        "第一份传完时撤 ⇒ 剩下的回 cancelled：{:?}",
        keyed(&r)
    );
    assert_eq!(r.outcomes.len(), 5, "每一格都有结局");
    assert!(!b.join("d").exists());
    assert!(seen.load(Ordering::SeqCst) >= 1);
}

#[test]
#[expect(
    clippy::type_complexity,
    reason = "进度那几格照回调的类型记，另起类型名只为过这条提示"
)]
fn progress_counts_bytes_across_the_group_and_moves_inside_a_file() {
    let (a, b) = rig("bytes");
    let t = format!("tc-bytes-{}", std::process::id());
    let ends = local();
    let seen: Arc<Mutex<Vec<(u64, Option<(u64, u64)>)>>> = Arc::default();
    let s2 = seen.clone();
    let _ear = crate::common::batch_book::listen(
        &t,
        Box::new(move |p| s2.lock().unwrap().push((p.done, p.bytes))),
    );
    let r = on_rt(|rt| {
        copy_with(
            &ends,
            rt,
            req(&a, &b, &["f.txt", "d"], OnExists::Ask, Some(&t)),
        )
    })
    .unwrap();
    assert!(r.outcomes.iter().all(|o| o.state == ItemState::Done));
    let seen = seen.lock().unwrap().clone();
    // 一组的字节：f.txt 1 ＋ s/y.txt 2 ＋ x.txt 1 ＝ 4（目录不算）。
    assert!(
        seen.iter().all(|(_, b)| b.is_none_or(|(_, t)| t == 4)),
        "总字节恒是整组文件的和：{seen:?}"
    );
    assert_eq!(
        seen.last().and_then(|s| s.1),
        Some((4, 4)),
        "收尾那一格字节走满：{seen:?}"
    );
    // y.txt 走到一半（1 字节）时：f.txt 的 1 ＋ 1 ＝ 2，项数还停在 y.txt 开头那一格。
    let at_y = seen
        .iter()
        .find(|(_, b)| *b == Some((2, 4)))
        .unwrap_or_else(|| panic!("一份文件传到一半，字节进度要动：{seen:?}"));
    assert_eq!(
        at_y.0, 3,
        "y.txt 是第 4 格（f.txt · d · d/s 之后）：{seen:?}"
    );
    let mono = seen
        .windows(2)
        .all(|w| w[0].1.map_or(0, |b| b.0) <= w[1].1.map_or(0, |b| b.0));
    assert!(mono, "字节进度不倒退：{seen:?}");
}

/// 还要多久那一句由账本照平均速度写：刚开头 · 走满了不说；走了一半用 10 秒 ⇒ 约 10 秒。
#[test]
fn the_time_left_is_written_by_the_book_from_the_average_speed() {
    use crate::common::batch_book::left_text;
    let about = |d: &str| Some(copy_text("rsFilewinProgress.eta.about", &[("dur", d)]));
    assert_eq!(
        left_text(50, 100, 10.0),
        about(&copy_core::short_duration(10_000))
    );
    assert_eq!(
        left_text(10, 100, 10.0),
        about(&copy_core::short_duration(120_000))
    );
    assert_eq!(left_text(50, 100, 1.0), None, "不足 2 秒不说");
    assert_eq!(left_text(0, 100, 10.0), None, "一个字节都没走不说");
    assert_eq!(left_text(100, 100, 10.0), None, "走满了不说");
}

/// 落到 Windows 那一形的名字：不认的字换 `_`、结尾的点与空格去掉、保留设备名前加 `_`；unix 上字节原样。
#[test]
fn a_name_from_elsewhere_becomes_one_this_machine_takes() {
    use crate::platform::paths::{legal_name, windows_legal_name};
    assert_eq!(windows_legal_name("CON.txt"), "_CON.txt");
    assert_eq!(windows_legal_name("com1"), "_com1");
    assert_eq!(windows_legal_name("a:b. "), "a_b");
    assert_eq!(windows_legal_name("q?.md"), "q_.md");
    assert_eq!(windows_legal_name(". "), "_");
    assert_eq!(windows_legal_name("报告 v2.pdf"), "报告 v2.pdf");
    #[cfg(unix)]
    {
        assert_eq!(legal_name(b"r\xff.bin"), os(b"r\xff.bin"));
        assert_eq!(legal_name(b"a/b"), os(b"a_b"));
    }
}

// ───────────────────────── 生产那一份两头：本机 · 远端（假那台后端 ＋ SFTP 台架）─────────────────────────

use crate::dial::sftp::rig as srig;

/// 那台的后端：认命令名、入参照真的解码器解，列 · 读块 · 建目录 · 送块 · 提交走文件管理那一份真的。
/// 那台的盘 ＝ `disk`。接了 SFTP 台架的（`rig`）：问之前台架整份铺到盘上、问完照盘把台架补齐，入参与回包里的路径按 `srig::HOME` ↔ `disk` 换。
#[derive(Clone)]
struct Back {
    disk: PathBuf,
    /// 那台后端说它的家在哪（与 SFTP 台架的起始目录比）。
    home: String,
    rig: Option<Arc<Mutex<srig::Fs>>>,
    /// 被问过的命令与没成那几条的码。
    asked: Arc<Mutex<Vec<String>>>,
    /// 头几次提交故意回 `stale`（判重传）。
    stale_first: Arc<AtomicUsize>,
}

impl Back {
    fn on_disk(disk: &Path) -> Back {
        fs::create_dir_all(disk).unwrap();
        Back {
            disk: disk.to_path_buf(),
            home: disk.to_string_lossy().into_owned(),
            rig: None,
            asked: Default::default(),
            stale_first: Default::default(),
        }
    }

    fn on_rig(disk: &Path, rig: Arc<Mutex<srig::Fs>>, home: &str) -> Back {
        Back {
            rig: Some(rig),
            home: home.to_string(),
            ..Back::on_disk(disk)
        }
    }

    fn asked(&self) -> Vec<String> {
        self.asked.lock().unwrap().clone()
    }

    fn to_disk(&self) {
        let Some(rig) = &self.rig else { return };
        fs::remove_dir_all(&self.disk).ok();
        fs::create_dir_all(&self.disk).unwrap();
        let g = rig.lock().unwrap();
        for d in &g.dirs {
            fs::create_dir_all(self.disk.join(d)).unwrap();
        }
        for (f, e) in &g.files {
            fs::write(self.disk.join(f), &e.bytes).unwrap();
        }
    }

    /// 问完：台架上的文件盘上没了的删掉，回包里那个落点（建出来的目录 · 提交落地的文件）照盘上补进台架。
    #[expect(
        clippy::wrong_self_convention,
        reason = "「照盘上补进台架」读作从盘上来，改的是台架自己"
    )]
    fn from_disk(&self, landed: Option<&str>) {
        let Some(rig) = &self.rig else { return };
        let mut g = rig.lock().unwrap();
        let disk = &self.disk;
        g.files.retain(|f, _| disk.join(f).exists());
        let Some(rel) = landed.and_then(|p| Path::new(p).strip_prefix(disk).ok()) else {
            return;
        };
        let (p, rel) = (disk.join(rel), rel.to_string_lossy().into_owned());
        if p.is_dir() {
            g.dirs.insert(rel);
        } else if let Ok(bytes) = fs::read(&p) {
            g.files.insert(rel, srig::Entry { bytes });
        }
    }

    fn answer(&self, cmd: &str, args: &serde_json::Value) -> Result<serde_json::Value, Said> {
        use crate::wire::typed::take;
        let face = Face::issue();
        let tz = crate::common::time::Tz::default();
        let v = |r: Result<serde_json::Value, serde_json::Error>| r.unwrap();
        Ok(match cmd {
            "files-home" => {
                take::<cwf::HomeReq>(args)?;
                serde_json::json!({ "path": self.home })
            }
            "files-stat" => v(serde_json::to_value(crate::files::answer(
                take::<cwf::StatReq>(args)?,
                &tz,
            )?)),
            "files-ls" => v(serde_json::to_value(crate::files::answer(
                take::<cwf::LsReq>(args)?,
                &tz,
            )?)),
            "files-read-chunk" => v(serde_json::to_value(crate::files::answer(
                take::<cwf::ReadChunkReq>(args)?,
                &tz,
            )?)),
            "files-mkdir" => v(serde_json::to_value(crate::files::write::answer(
                take::<cwf::FilesMkdirReq>(args)?,
                &face,
            )?)),
            "files-stage-chunk" => v(serde_json::to_value(crate::files::commit::stage_chunk_at(
                &face,
                &self.disk,
                take::<cwf::FilesStageChunkReq>(args)?,
            )?)),
            "files-commit-upload" => {
                let req = take::<cwf::FilesCommitUploadReq>(args)?;
                if self
                    .stale_first
                    .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |n| n.checked_sub(1))
                    .is_ok()
                {
                    return Err(Said::new("stale", "合成：摘要对不上".to_string()));
                }
                v(serde_json::to_value(
                    crate::files::commit::commit_upload_at(&face, &self.disk, req, &tz)
                        .map_err(|e| e.flat())?,
                ))
            }
            other => return Err(Said::new("unknown_command", other.to_string())),
        })
    }
}

impl Far for Back {
    fn ask<'a>(
        &'a self,
        cmd: &'static str,
        args: serde_json::Value,
    ) -> Fut<'a, Result<serde_json::Value, Said>> {
        let disk = self.disk.to_string_lossy().into_owned();
        let args = if self.rig.is_some() {
            serde_json::from_str(&args.to_string().replace(srig::HOME, &disk)).unwrap()
        } else {
            args
        };
        self.to_disk();
        let out = self.answer(cmd, &args);
        self.from_disk(
            out.as_ref()
                .ok()
                .and_then(|v| v.get("path"))
                .and_then(serde_json::Value::as_str),
        );
        self.asked.lock().unwrap().push(match &out {
            Ok(_) => cmd.to_string(),
            Err(s) => format!("{cmd} ✗ {}: {}", s.code_or_fallback(), s.said),
        });
        let back = self.rig.is_some();
        Box::pin(async move {
            out.map(|v| {
                if back {
                    serde_json::from_str(&v.to_string().replace(&disk, srig::HOME)).unwrap()
                } else {
                    v
                }
            })
        })
    }
}

/// 一台远端：假后端 ＋（有台架 ⇒）一条 SFTP 会话，经生产那一口 `There::over` 开（家目录与 SFTP 起始目录在那里比）。
fn there(rt: &tokio::runtime::Handle, back: &Back) -> Box<dyn Side> {
    let sftp = back
        .rig
        .as_ref()
        .map(|r| rt.block_on(srig::session_on(r.clone())));
    let t = rt
        .block_on(There::over(Face::issue(), Box::new(back.clone()), sftp))
        .unwrap_or_else(|e| panic!("那一端没开成：{e:?}"));
    Box::new(t)
}

/// 照生产那一口拼两端（`Pair::open`）：本机那一端自己出，远端那一端是 `far`。
fn pair(rt: &tokio::runtime::Handle, home: &Path, q: &TransferCopyReq, far: Box<dyn Side>) -> Pair {
    let far = Mutex::new(Some(far));
    let open = |_: Face, _: cmd_wire::Machine| -> Fut<'_, Result<Box<dyn Side>, CopyErr>> {
        let s = far.lock().unwrap().take().expect("只开一次远端");
        Box::pin(async move { Ok(s) })
    };
    rt.block_on(Pair::open(Face::issue(), home, &q.from, &q.to, &open))
        .unwrap_or_else(|e| panic!("两端没拼成：{e:?}"))
}

fn rig_at(p: &str) -> Vec<u8> {
    format!("{}/{p}", srig::HOME).into_bytes()
}

/// 本机暂存区一份不留（只删得掉空目录）。
fn staging_left_empty(home: &Path) -> bool {
    fs::remove_dir(home.join(relay_route_core::STAGING_DIR_REL)).is_ok()
}

/// ★ ① 源是本机：本机一份文件 ＋ 一棵目录搬到那台（SFTP 腿），那台上逐字节一样、两头暂存都不留。
#[test]
fn from_this_machine_to_another_goes_through_to_the_commit() {
    let base = scratch("up");
    let (home, src) = (base.join("home"), base.join("src"));
    fs::create_dir_all(home.join(".cc-monitor")).unwrap();
    fs::create_dir_all(src.join("d")).unwrap();
    let (one, two) = (srig::corpus(70_000), srig::corpus(1_234));
    fs::write(src.join("f.bin"), &one).unwrap();
    fs::write(src.join("d/x.bin"), &two).unwrap();
    let b = srig::home(false, true);
    b.lock().unwrap().dirs.insert("dst".into());
    let back = Back::on_rig(&base.join("b-disk"), b.clone(), srig::HOME);
    let q = copy_req(
        cmd_wire::Machine::local(),
        cmd_wire::Machine::named("b"),
        vec![bytes_of(&src.join("f.bin")), bytes_of(&src.join("d"))],
        rig_at("dst"),
        OnExists::Ask,
        None,
    );
    let r = on_rt(|rt| {
        let ends = pair(rt, &home, &q, there(rt, &back));
        copy_with(&ends, rt, q.clone())
    })
    .unwrap();
    assert!(
        r.outcomes.iter().all(|o| o.state == ItemState::Done),
        "每一项都该落地：{:?}；那台被问的：{:?}",
        r.outcomes,
        back.asked()
    );
    assert!(
        !back
            .asked()
            .iter()
            .any(|c| c.starts_with("files-stage-chunk")),
        "起始目录就是家 ⇒ 走 SFTP 腿：{:?}",
        back.asked()
    );
    let g = b.lock().unwrap();
    assert_eq!(g.bytes("dst/f.bin"), Some(one), "那份文件逐字节一样");
    assert_eq!(g.bytes("dst/d/x.bin"), Some(two), "目录里那份逐字节一样");
    let left: Vec<_> = g
        .files
        .keys()
        .filter(|k| k.starts_with(".cc-monitor/"))
        .collect();
    assert!(left.is_empty(), "那台不留暂存件：{left:?}");
    drop(g);
    assert!(staging_left_empty(&home), "本机不留暂存件");
    fs::remove_dir_all(&base).ok();
}

/// ★ ② 那台 SFTP 起始目录不是它后端的家 ⇒ 这一份改走分块腿，照样落地、逐字节一样（不再整组失败）。
#[test]
fn a_start_dir_that_is_not_home_moves_the_bytes_in_chunks() {
    let base = scratch("chunks");
    let (home, src) = (base.join("home"), base.join("src"));
    fs::create_dir_all(home.join(".cc-monitor")).unwrap();
    fs::create_dir_all(&src).unwrap();
    // 比一块大：至少两块。
    let one = srig::corpus(chunk_core::CHUNK + 4_321);
    fs::write(src.join("f.bin"), &one).unwrap();
    let disk = base.join("b-disk");
    fs::create_dir_all(disk.join("dst")).unwrap();
    fs::create_dir_all(disk.join(".cc-monitor")).unwrap();
    let b = srig::home(false, true);
    // SFTP 台架在（起始目录 /home/rig），那台后端说家在别处：不许用 SFTP 腿。
    let back = Back {
        home: "/srv/elsewhere".into(),
        ..Back::on_disk(&disk)
    };
    let q = copy_req(
        cmd_wire::Machine::local(),
        cmd_wire::Machine::named("b"),
        vec![bytes_of(&src.join("f.bin"))],
        bytes_of(&disk.join("dst")),
        OnExists::Ask,
        None,
    );
    let r = on_rt(|rt| {
        let sftp = Some(rt.block_on(srig::session_on(b.clone())));
        let t = rt
            .block_on(There::over(Face::issue(), Box::new(back.clone()), sftp))
            .unwrap_or_else(|e| panic!("起始目录不是家也该开得成：{e:?}"));
        let ends = pair(rt, &home, &q, Box::new(t));
        copy_with(&ends, rt, q.clone())
    })
    .unwrap();
    assert_eq!(
        keyed(&r),
        vec![("0".into(), ItemState::Done)],
        "{r:?} · {:?}",
        back.asked()
    );
    let staged = back
        .asked()
        .iter()
        .filter(|c| *c == "files-stage-chunk")
        .count();
    assert_eq!(staged, 2, "两块各送一次：{:?}", back.asked());
    assert_eq!(fs::read(disk.join("dst/f.bin")).unwrap(), one);
    assert!(
        b.lock().unwrap().touched().is_empty(),
        "SFTP 那条会话一个字节都没写"
    );
    assert!(staging_left_empty(&home));
    fs::remove_dir_all(&base).ok();
}

/// ★ ③ 那台上一份名字不是 UTF-8 的文件下到本机 ⇒ 落名字节原样（分块腿读回），本机暂存不留。
#[cfg(unix)]
#[test]
fn a_remote_name_that_is_not_utf8_comes_down_byte_for_byte() {
    let base = scratch("down");
    let (home, dst) = (base.join("home"), base.join("dst"));
    fs::create_dir_all(home.join(".cc-monitor")).unwrap();
    fs::create_dir_all(&dst).unwrap();
    let disk = base.join("a-disk");
    fs::create_dir_all(&disk).unwrap();
    let name: &[u8] = b"r\xff\xfe.bin";
    let body = srig::corpus(9_999);
    fs::write(disk.join(os(name)), &body).unwrap();
    let back = Back::on_disk(&disk);
    let mut src = bytes_of(&disk);
    src.push(b'/');
    src.extend_from_slice(name);
    let q = copy_req(
        cmd_wire::Machine::named("a"),
        cmd_wire::Machine::local(),
        vec![src],
        bytes_of(&dst),
        OnExists::Ask,
        None,
    );
    let r = on_rt(|rt| {
        let ends = pair(rt, &home, &q, there(rt, &back));
        copy_with(&ends, rt, q.clone())
    })
    .unwrap();
    assert_eq!(
        keyed(&r),
        vec![("0".into(), ItemState::Done)],
        "{r:?} · {:?}",
        back.asked()
    );
    assert_eq!(fs::read(dst.join(os(name))).unwrap(), body);
    assert!(staging_left_empty(&home), "本机暂存那一份已上位");
    fs::remove_dir_all(&base).ok();
}

/// 提交时那台说摘要对不上（`stale`）⇒ 这一份从头再送一次、照样落地。
#[test]
fn a_stale_commit_is_sent_again_from_the_start_once() {
    let base = scratch("stale");
    let (home, src) = (base.join("home"), base.join("src"));
    fs::create_dir_all(home.join(".cc-monitor")).unwrap();
    fs::create_dir_all(&src).unwrap();
    fs::write(src.join("f.bin"), b"FRESH").unwrap();
    let disk = base.join("b-disk");
    fs::create_dir_all(disk.join("dst")).unwrap();
    let back = Back::on_disk(&disk);
    back.stale_first.store(1, Ordering::SeqCst);
    let q = copy_req(
        cmd_wire::Machine::local(),
        cmd_wire::Machine::named("b"),
        vec![bytes_of(&src.join("f.bin"))],
        bytes_of(&disk.join("dst")),
        OnExists::Ask,
        None,
    );
    let r = on_rt(|rt| {
        let ends = pair(rt, &home, &q, there(rt, &back));
        copy_with(&ends, rt, q.clone())
    })
    .unwrap();
    assert_eq!(keyed(&r), vec![("0".into(), ItemState::Done)], "{r:?}");
    let commits = back
        .asked()
        .iter()
        .filter(|c| c.starts_with("files-commit-upload"))
        .count();
    assert_eq!(commits, 2, "头一次 stale、再送一次：{:?}", back.asked());
    assert_eq!(fs::read(disk.join("dst/f.bin")).unwrap(), b"FRESH");
    fs::remove_dir_all(&base).ok();
}

/// 两台之间（都是远端、都接 SFTP 台架）：一份文件 ＋ 一棵目录从头走到提交，落点那台逐字节一样、三处暂存都不留。
#[test]
fn a_copy_between_two_machines_goes_through_to_the_commit_byte_for_byte() {
    let base = scratch("wired");
    let home = base.join("here");
    fs::create_dir_all(home.join(".cc-monitor")).unwrap();
    let (one, two) = (srig::corpus(70_000), srig::corpus(1_234));
    let a = srig::home(false, false);
    {
        let mut g = a.lock().unwrap();
        g.dirs.extend(["src".to_string(), "src/d".to_string()]);
        g.files
            .insert("src/f.bin".into(), srig::Entry { bytes: one.clone() });
        g.files
            .insert("src/d/x.bin".into(), srig::Entry { bytes: two.clone() });
    }
    let b = srig::home(false, true);
    b.lock().unwrap().dirs.insert("dst".into());
    let (fa, fb) = (
        Back::on_rig(&base.join("a-disk"), a.clone(), srig::HOME),
        Back::on_rig(&base.join("b-disk"), b.clone(), srig::HOME),
    );
    let q = copy_req(
        cmd_wire::Machine::named("a"),
        cmd_wire::Machine::named("b"),
        vec![rig_at("src/f.bin"), rig_at("src/d")],
        rig_at("dst"),
        OnExists::Ask,
        None,
    );
    let r = on_rt(|rt| {
        let ends = Pair {
            face: Face::issue(),
            staging: staging::ensure(&Face::issue(), &home).unwrap(),
            from: there(rt, &fa),
            to: there(rt, &fb),
        };
        copy_with(&ends, rt, q.clone())
    })
    .unwrap();
    assert!(
        r.outcomes.iter().all(|o| o.state == ItemState::Done),
        "每一项都该落地：{:?}；落点那台被问的：{:?}",
        r.outcomes,
        fb.asked()
    );
    let g = b.lock().unwrap();
    assert_eq!(g.bytes("dst/f.bin"), Some(one), "那份文件逐字节一样");
    assert_eq!(g.bytes("dst/d/x.bin"), Some(two), "目录里那份逐字节一样");
    let left: Vec<_> = g
        .files
        .keys()
        .filter(|k| k.starts_with(".cc-monitor/"))
        .collect();
    assert!(left.is_empty(), "落点那台不留暂存件：{left:?}");
    drop(g);
    assert!(staging_left_empty(&home), "本机不留暂存件");
    fs::remove_dir_all(&base).ok();
}

/// 落点是本机：每一段落名都过这台的合法名（顶上那一格改了名，它底下的子项跟着落进改过名的那一层），结局里的路径就是盘上那一格。
#[cfg(unix)]
#[test]
fn every_segment_landing_here_takes_the_name_this_machine_accepts() {
    let base = scratch("legal");
    let (home, src, dst) = (base.join("home"), base.join("src"), base.join("dst"));
    fs::create_dir_all(home.join(".cc-monitor")).unwrap();
    fs::create_dir_all(src.join("d")).unwrap();
    fs::create_dir_all(&dst).unwrap();
    fs::write(src.join("d/x.txt"), b"X").unwrap();
    let mut q = copy_req(
        cmd_wire::Machine::local(),
        cmd_wire::Machine::local(),
        vec![bytes_of(&src.join("d"))],
        bytes_of(&dst),
        OnExists::Ask,
        None,
    );
    q.items[0].name = Some(WirePath::from("p/q"));
    let r = on_rt(|rt| {
        let never = |_: Face, _: cmd_wire::Machine| -> Fut<'_, Result<Box<dyn Side>, CopyErr>> {
            panic!("两端都是本机，不开远端")
        };
        let ends = rt
            .block_on(Pair::open(Face::issue(), &home, &q.from, &q.to, &never))
            .unwrap_or_else(|e| panic!("{e:?}"));
        copy_with(&ends, rt, q.clone())
    })
    .unwrap();
    assert_eq!(
        keyed(&r),
        vec![
            ("0".into(), ItemState::Done),
            ("0/x.txt".into(), ItemState::Done)
        ],
        "{r:?}"
    );
    assert_eq!(fs::read(dst.join("p_q/x.txt")).unwrap(), b"X");
    assert_eq!(
        r.outcomes[1].path,
        Some(WirePath(bytes_of(&dst.join("p_q/x.txt")))),
        "结局里的路径是盘上那一格"
    );
    assert!(staging_left_empty(&home));
    fs::remove_dir_all(&base).ok();
}
