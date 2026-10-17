//! `control/transfer_copy.rs` 的判据：两台之间复制一组的本体（`copy_with`）。两头用本机两个目录上的替身（[`Local`]）：
//! 源那台 ＝ `a/`，落点那台 ＝ `b/`；路径照远端那一形写（绝对路径串）。
//! 生产那一份两头（`Wired`）另有一趟：两台都是合成 SFTP 台架，问落点那台后端经一台假远端（解入参 · 提交都走真的那一份）。

use super::*;
use cmd_wire::batch::OnExists;
use cmd_wire::transfer::CopyFrom;
use std::fs;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

/// 本机目录上的两头。`files` 数传了几份；`after` 传完第几份时调一次（撤的判据在那儿立旗）。
struct Local {
    /// 源那台的根（`a/`）：[`TREE`] 里的路径相对它。
    a: PathBuf,
    files: AtomicUsize,
    after: Box<dyn Fn(usize) + Send + Sync>,
}

impl Ends for Local {
    fn walk<'a>(&'a self, path: &'a str) -> Fut<'a, Result<Vec<Entry>, Miss>> {
        Box::pin(async move {
            let p = Path::new(path);
            let Ok(m) = fs::metadata(p) else {
                return Err(("io_failed", Said::from(format!("missing {path}"))));
            };
            // 源那台上的树按铺的时候那张清单答（判据不自己遍历目录）。
            let mut out = vec![Entry {
                rel: String::new(),
                dir: m.is_dir(),
                size: if m.is_dir() { 0 } else { m.len() },
            }];
            if m.is_dir() {
                for (rel, dir) in TREE {
                    let under = format!("{path}/");
                    let full = format!("{}/{rel}", self.a.to_string_lossy());
                    if let Some(r) = full.strip_prefix(&under) {
                        out.push(Entry {
                            rel: r.to_string(),
                            dir: *dir,
                            size: fs::metadata(&full).map(|m| m.len()).unwrap_or(0),
                        });
                    }
                }
            }
            Ok(out)
        })
    }
    fn there<'a>(&'a self, path: &'a str) -> Fut<'a, Option<bool>> {
        Box::pin(async move { fs::symlink_metadata(path).ok().map(|m| m.is_dir()) })
    }
    fn mkdir<'a>(&'a self, dir: &'a str, name: &'a str) -> Fut<'a, Result<(), Miss>> {
        Box::pin(async move {
            fs::create_dir(Path::new(dir).join(name))
                .map_err(|e| ("io_failed", Said::from(e.to_string())))
        })
    }
    fn copy_file<'a>(
        &'a self,
        from: &'a str,
        dir: &'a str,
        name: &'a str,
        overwrite: bool,
        _stop: &'a (dyn Fn() -> bool + Send + Sync),
        sent: &'a (dyn Fn(u64) + Send + Sync),
    ) -> Fut<'a, Result<u64, Miss>> {
        Box::pin(async move {
            let to = Path::new(dir).join(name);
            if to.exists() && !overwrite {
                return Err(("exists", Said::from("exists".to_string())));
            }
            // 走到一半报一次（替身里「一半」＝ 一半字节），传完报整份。
            let len = fs::metadata(from).map(|m| m.len()).unwrap_or(0);
            sent(len / 2);
            let n = fs::copy(from, &to).map_err(|e| ("io_failed", Said::from(e.to_string())))?;
            sent(n);
            let k = self.files.fetch_add(1, Ordering::SeqCst) + 1;
            (self.after)(k);
            Ok(n)
        })
    }
}

/// 铺在源那台上的树（相对 `a/`，先序）：[`rig`] 照它铺。
const TREE: &[(&str, bool)] = &[("d/s", true), ("d/s/y.txt", false), ("d/x.txt", false)];

fn local(a: &Path) -> Local {
    Local {
        a: a.to_path_buf(),
        files: AtomicUsize::new(0),
        after: Box::new(|_| {}),
    }
}

/// 两头：`a/` 里一份文件 `f.txt` ＋ 一棵 `d/{x.txt, s/y.txt}`，`b/` 空。
fn rig(tag: &str) -> (PathBuf, PathBuf) {
    let base = std::env::temp_dir().join(format!("ccm-tc-{tag}-{}", std::process::id()));
    fs::remove_dir_all(&base).ok();
    let (a, b) = (base.join("a"), base.join("b"));
    fs::create_dir_all(a.join("d/s")).unwrap();
    fs::create_dir_all(&b).unwrap();
    fs::write(a.join("f.txt"), b"F").unwrap();
    fs::write(a.join("d/x.txt"), b"X").unwrap();
    fs::write(a.join("d/s/y.txt"), b"YY").unwrap();
    (a, b)
}

fn req(a: &Path, b: &Path, names: &[&str], on: OnExists, ticket: Option<&str>) -> TransferCopyReq {
    TransferCopyReq {
        from: serde_json::json!({}),
        to: serde_json::json!({}),
        items: names
            .iter()
            .map(|n| CopyFrom {
                remote_path: a.join(n).to_string_lossy().into_owned(),
            })
            .collect(),
        to_dir: b.to_string_lossy().into_owned(),
        on_exists: Some(on),
        decide: None,
        ticket: ticket.map(str::to_string),
    }
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
    let ends = local(&a);
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

#[test]
fn a_clash_on_the_target_leaves_every_byte_alone() {
    let (a, b) = rig("clash");
    fs::write(b.join("f.txt"), b"old").unwrap();
    let ends = local(&a);
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
    let ends = local(&a);
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
        a: a.clone(),
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
fn progress_counts_bytes_across_the_group_and_moves_inside_a_file() {
    let (a, b) = rig("bytes");
    let t = format!("tc-bytes-{}", std::process::id());
    let ends = local(&a);
    let seen: Arc<std::sync::Mutex<Vec<(u64, Option<(u64, u64)>)>>> = Arc::default();
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

// ───────────────────────── 生产那一份两头：两台台架 ＋ 假远端 ─────────────────────────

use crate::dial::sftp::rig;

/// 落点那台的后端：命令行里认出子命令、入参照真的解码器解，建目录与提交走真的那一份。
/// 那台的盘 ＝ 一个临时目录（`disk`）：问之前台架整份铺到盘上，问完照盘把台架补齐；入参与回包里的路径按 `rig::HOME` ↔ `disk` 换。
#[derive(Clone)]
struct RigBackend {
    fs: Arc<std::sync::Mutex<rig::Fs>>,
    disk: PathBuf,
    /// 被问过的子命令与没成那几条的码。
    asked: Arc<std::sync::Mutex<Vec<String>>>,
}

impl RigBackend {
    fn to_disk(&self) {
        fs::remove_dir_all(&self.disk).ok();
        fs::create_dir_all(&self.disk).unwrap();
        let g = self.fs.lock().unwrap();
        for d in &g.dirs {
            fs::create_dir_all(self.disk.join(d)).unwrap();
        }
        for (f, e) in &g.files {
            fs::write(self.disk.join(f), &e.bytes).unwrap();
        }
    }

    /// 问完：台架上的文件盘上没了的删掉，回包里那个落点（建出来的目录 · 提交落地的文件）照盘上补进台架。
    fn from_disk(&self, landed: Option<&str>) {
        let mut g = self.fs.lock().unwrap();
        let disk = &self.disk;
        g.files.retain(|f, _| disk.join(f).exists());
        let Some(rel) = landed.and_then(|p| Path::new(p).strip_prefix(disk).ok()) else {
            return;
        };
        let (p, rel) = (disk.join(rel), rel.to_string_lossy().into_owned());
        if p.is_dir() {
            g.dirs.insert(rel);
        } else if let Ok(bytes) = fs::read(&p) {
            g.files.insert(rel, rig::Entry { bytes });
        }
    }

    fn answer(
        &self,
        flag: &str,
        args: &serde_json::Value,
    ) -> Result<serde_json::Value, (String, String)> {
        use crate::wire::typed::take;
        let face = crate::platform::fs::Face::issue();
        let w = |f: crate::control::files_write::WriteFail| (f.code.to_string(), f.said);
        let t = |f: crate::wire::spec::Fail| (f.code, f.message);
        let v = match flag {
            "files-home" => {
                take::<cwf::HomeReq>(args).map_err(t)?;
                serde_json::json!({ "path": rig::HOME })
            }
            "files-mkdir" => serde_json::to_value(
                crate::control::files_write::answer(
                    take::<cwf::FilesMkdirReq>(args).map_err(t)?,
                    &face,
                )
                .map_err(w)?,
            )
            .unwrap(),
            "files-commit-upload" => serde_json::to_value(
                crate::control::files_commit::commit_upload_at(
                    &face,
                    &self.disk,
                    take::<cwf::FilesCommitUploadReq>(args).map_err(t)?,
                    &crate::Tz::default(),
                )
                .map_err(|e| w(e.flat()))?,
            )
            .unwrap(),
            other => return Err(("unknown_command".to_string(), other.to_string())),
        };
        Ok(v)
    }
}

impl crate::dial::remote_ask::Remote for RigBackend {
    fn run<'a>(
        &'a self,
        _dial: &'a serde_json::Value,
        _command: String,
        _stdin: Option<String>,
    ) -> Fut<'a, Result<String, String>> {
        Box::pin(async { Err("走 run_coded".to_string()) })
    }
    fn run_coded<'a>(
        &'a self,
        _dial: &'a serde_json::Value,
        command: String,
        stdin: Option<String>,
    ) -> Fut<'a, Result<String, crate::dial::remote_ask::Said>> {
        let flag = command
            .split(" -- ")
            .nth(1)
            .and_then(|t| t.split('\'').nth(1))
            .unwrap_or_default()
            .trim_start_matches('-')
            .to_string();
        let disk = self.disk.to_string_lossy().into_owned();
        let line = stdin.unwrap_or_default().replace(rig::HOME, &disk);
        let args: serde_json::Value = serde_json::from_str(line.trim()).unwrap();
        self.to_disk();
        let out = self.answer(&flag, &args);
        self.from_disk(
            out.as_ref()
                .ok()
                .and_then(|v| v.get("path"))
                .and_then(serde_json::Value::as_str),
        );
        self.asked.lock().unwrap().push(match &out {
            Ok(_) => flag,
            Err((c, m)) => format!("{flag} ✗ {c}: {m}"),
        });
        Box::pin(async move {
            out.map(|v| v.to_string().replace(&disk, rig::HOME))
                .map_err(|(c, m)| crate::dial::remote_ask::Said {
                    code: Some(c),
                    message: m,
                })
        })
    }
}

/// ★ 两台之间复制从头走到提交：一份文件 ＋ 一棵目录，落点那台上逐字节一样，两头暂存都不留。
#[test]
fn a_copy_between_two_machines_goes_through_to_the_commit_byte_for_byte() {
    let base = std::env::temp_dir().join(format!("ccm-tc-wired-{}", std::process::id()));
    fs::remove_dir_all(&base).ok();
    let (here, disk) = (base.join("here"), base.join("b-disk"));
    fs::create_dir_all(here.join(".cc-monitor")).unwrap();
    let (one, two) = (rig::corpus(70_000), rig::corpus(1_234));
    let a = rig::home(false, false);
    {
        let mut g = a.lock().unwrap();
        g.dirs.extend(["src".to_string(), "src/d".to_string()]);
        g.files
            .insert("src/f.bin".into(), rig::Entry { bytes: one.clone() });
        g.files
            .insert("src/d/x.bin".into(), rig::Entry { bytes: two.clone() });
    }
    let b = rig::home(false, true);
    b.lock().unwrap().dirs.insert("dst".into());
    let back = RigBackend {
        fs: b.clone(),
        disk,
        asked: Default::default(),
    };
    let at = |p: &str| format!("{}/{p}", rig::HOME);
    let r = on_rt(|rt| {
        let (fa, fb) = rt.block_on(async {
            (
                rig::session_on(a.clone()).await,
                rig::session_on(b.clone()).await,
            )
        });
        let w = rt
            .block_on(Wired::over(
                crate::platform::fs::Face::issue(),
                fa,
                fb,
                serde_json::json!({}),
                crate::Tz::default(),
                &here,
                Box::new(back.clone()),
            ))
            .unwrap_or_else(|e| panic!("两头没备好：{e:?}"));
        copy_with(
            &w,
            rt,
            TransferCopyReq {
                from: serde_json::json!({}),
                to: serde_json::json!({}),
                items: ["src/f.bin", "src/d"]
                    .iter()
                    .map(|p| CopyFrom { remote_path: at(p) })
                    .collect(),
                to_dir: at("dst"),
                on_exists: Some(OnExists::Ask),
                decide: None,
                ticket: None,
            },
        )
    })
    .unwrap();
    let asked = back.asked.lock().unwrap().clone();
    assert!(
        r.outcomes.iter().all(|o| o.state == ItemState::Done),
        "每一项都该落地：{:?}；落点那台被问的：{asked:?}",
        r.outcomes
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
    // 只删得掉空目录：本机暂存区在、且一份不留。
    assert!(
        fs::remove_dir(here.join(relay_route_core::STAGING_DIR_REL)).is_ok(),
        "本机不留暂存件"
    );
    drop(g);
    fs::remove_dir_all(&base).ok();
}
