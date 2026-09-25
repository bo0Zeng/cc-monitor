//! 〔SR1b · 2026-09-24〕`control/transfer.rs` 的判据：传输台住本机常驻后端。
//!
//! 台架：`dial::sftp::rig`（合成 SFTP 服务端，逐条记改动路径与写偏移）＋ 本机临时目录。
//! 上传 / 下载两份本体的语料是一条 SFTP 会话（`Session::over` —— 生产那一个口），判据直接喂它；
//! 票表（`Desk`）那一半判记账与帧，不起真 SSH（真 sshd 那一维在 `tests/evidence/SR1b-sftp-loopback.py`）。
//!
//! 来历：这几条的形状照 monitor 那一侧 F7c / 步 24 的判据（`sftp_staging_tests` · `sftp_pool_f4_tests` ·
//! `sftp_pool_tests`）—— 传输本体搬过来了，判据跟着搬，被判的是后端这一份实现。

use super::*;
use crate::dial::sftp::rig::{self, Entry};
use std::sync::atomic::AtomicUsize;

const KEY: &str = "00112233445566778899aabbccddeeff";

fn no_progress(_: u64, _: u64) {}

/// 本机一份夹具文件 / 目录（落 `temp_dir`，跑完就删）。
struct Tmp(PathBuf);
impl Tmp {
    fn dir(tag: &str) -> Self {
        static N: AtomicUsize = AtomicUsize::new(0);
        let p = std::env::temp_dir().join(format!(
            "ccm-sr1b-{tag}-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::SeqCst)
        ));
        std::fs::create_dir_all(&p).expect("建夹具目录");
        Self(p)
    }
    fn file(&self, name: &str, bytes: &[u8]) -> String {
        let p = self.0.join(name);
        std::fs::write(&p, bytes).expect("写夹具文件");
        p.to_string_lossy().into_owned()
    }
    fn path(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}
impl Drop for Tmp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// 改动表里**不在暂存区底下**的那几条。
fn outside_staging(fs: &Arc<Mutex<rig::Fs>>) -> Vec<(String, String)> {
    let root = sftp::STAGING_ROOT;
    let prefix = format!("{root}/");
    fs.lock()
        .unwrap()
        .mutated
        .iter()
        .filter(|(_, p)| p != root && !p.starts_with(&prefix))
        .cloned()
        .collect()
}

// ═══ 上传 ══════════════════════════════════════════════════════════════════════════════

/// 🔴🔴 **暂存区之外零写**（`设计/60 §13.6` 判据 2）：成功 · 失败 · 续传 · 撤四趟，
/// 服务端记下的每一个改动路径都在暂存区底下。**正控**：同一台服务端上一次暂存区外的写被这张表认出来（表不瞎）。
#[tokio::test]
async fn a_staging_upload_writes_nothing_outside_the_staging_area() {
    let fs = rig::home(false, false);
    let s = rig::session_on(fs.clone()).await;
    let tmp = Tmp::dir("up");
    let local = tmp.file("a.bin", &rig::corpus(300_000));
    // ① 成功（暂存区还不在 ⇒ 建最后那一段）。
    upload_to_staging(&s, &local, KEY, &Cancel::default(), &no_progress)
        .await
        .expect("第一趟该成");
    // ② 失败：写到第 3 块就坏。
    fs.lock().unwrap().fail_write_after = Some(3);
    let k2 = "ffeeddccbbaa99887766554433221100";
    upload_to_staging(&s, &local, k2, &Cancel::default(), &no_progress)
        .await
        .expect_err("中途坏了该报错");
    // ③ 续传。
    fs.lock().unwrap().fail_write_after = None;
    upload_to_staging(&s, &local, k2, &Cancel::default(), &no_progress)
        .await
        .expect("续传该成");
    // ④ 撤。
    let c = Cancel::default();
    c.fire();
    let k3 = "0123456789abcdef0123456789abcdef";
    upload_to_staging(&s, &local, k3, &c, &no_progress)
        .await
        .expect_err("撤了该报错");
    assert!(
        !fs.lock().unwrap().mutated.is_empty(),
        "四趟下来服务端一处改动都没记 —— 台架瞎了，下面那个零是空转"
    );
    assert_eq!(outside_staging(&fs), vec![], "暂存区之外有写");
    // 正控：同一张表认得出一次暂存区外的写。
    sftp::make_dir(&s, ".cc-monitor/bin")
        .await
        .expect("正控：bin 是另一个根");
    assert_eq!(
        outside_staging(&fs),
        vec![("mkdir".to_string(), ".cc-monitor/bin".to_string())],
        "正控没被认出来 —— 这张表是瞎的"
    );
}

/// 字节原样落进 `staging/<key>.part`（不改名、不提交 —— 提交归远端后端 `files-commit-upload`）。
#[tokio::test]
async fn the_bytes_land_at_the_staging_part_verbatim() {
    let fs = rig::home(false, true);
    let s = rig::session_on(fs.clone()).await;
    let tmp = Tmp::dir("verbatim");
    let body = rig::corpus(100_001);
    let local = tmp.file("b.bin", &body);
    let got = std::sync::Mutex::new(Vec::<(u64, u64)>::new());
    let sink = |a: u64, b: u64| got.lock().unwrap().push((a, b));
    let n = upload_to_staging(&s, &local, KEY, &Cancel::default(), &sink)
        .await
        .unwrap();
    assert_eq!(n, body.len() as u64);
    assert_eq!(
        fs.lock().unwrap().bytes(&staging_part(KEY)),
        Some(body.clone())
    );
    let got = got.into_inner().unwrap();
    assert_eq!(got.first(), Some(&(0, body.len() as u64)), "第一格该是 0");
    assert_eq!(
        got.last(),
        Some(&(body.len() as u64, body.len() as u64)),
        "最后一格该是整份"
    );
}

/// 失败 ⇒ **留**暂存件；重拖同一份 ⇒ 从尾块接上（服务端记下的第一个写偏移 == 续传起点）。
#[tokio::test]
async fn a_failed_upload_keeps_the_part_and_the_retry_resumes_from_its_tail() {
    let fs = rig::home(false, true);
    let s = rig::session_on(fs.clone()).await;
    let tmp = Tmp::dir("resume-up");
    let body = rig::corpus(200_000);
    let local = tmp.file("c.bin", &body);
    fs.lock().unwrap().fail_write_after = Some(2);
    upload_to_staging(&s, &local, KEY, &Cancel::default(), &no_progress)
        .await
        .expect_err("第 3 块坏");
    let have = fs
        .lock()
        .unwrap()
        .bytes(&staging_part(KEY))
        .map(|b| b.len())
        .expect("失败之后暂存件该留着");
    assert!(have > 0, "失败之后暂存件是空的 —— 续传没有本钱");
    {
        let mut g = fs.lock().unwrap();
        g.fail_write_after = None;
        g.write_offsets.clear();
    }
    upload_to_staging(&s, &local, KEY, &Cancel::default(), &no_progress)
        .await
        .expect("续传该成");
    assert_eq!(
        fs.lock().unwrap().write_offsets.first(),
        Some(&(have as u64)),
        "续传没从尾块接上"
    );
    assert_eq!(fs.lock().unwrap().bytes(&staging_part(KEY)), Some(body));
}

/// 暂存件的尾块与本机那份对不上（同名不同内容）⇒ **从 0 重来**，不缝。
#[tokio::test]
async fn an_upload_with_a_mismatched_staging_part_starts_over() {
    let fs = rig::home(false, true);
    fs.lock().unwrap().files.insert(
        staging_part(KEY),
        Entry {
            bytes: vec![0xAB; 40_000],
        },
    );
    let s = rig::session_on(fs.clone()).await;
    let tmp = Tmp::dir("mismatch-up");
    let body = rig::corpus(90_000);
    let local = tmp.file("d.bin", &body);
    upload_to_staging(&s, &local, KEY, &Cancel::default(), &no_progress)
        .await
        .unwrap();
    assert_eq!(fs.lock().unwrap().write_offsets.first(), Some(&0));
    assert_eq!(fs.lock().unwrap().bytes(&staging_part(KEY)), Some(body));
}

/// 撤 ⇒ **删**暂存件（用户说了不要）。
#[tokio::test]
async fn a_cancelled_upload_removes_its_part() {
    let fs = rig::home(false, true);
    fs.lock().unwrap().files.insert(
        staging_part(KEY),
        Entry {
            bytes: b"half".to_vec(),
        },
    );
    let s = rig::session_on(fs.clone()).await;
    let tmp = Tmp::dir("cancel-up");
    let local = tmp.file("e.bin", &rig::corpus(80_000));
    let c = Cancel::default();
    c.fire();
    upload_to_staging(&s, &local, KEY, &c, &no_progress)
        .await
        .expect_err("撤了");
    assert_eq!(
        fs.lock().unwrap().bytes(&staging_part(KEY)),
        None,
        "撤了暂存件还在"
    );
}

/// `~/.cc-monitor` 不在（后端没部署）⇒ 报错且**一次改动都没有**（不顺手建后端的家，D11）。
#[tokio::test]
async fn without_the_backend_home_nothing_is_written_and_it_says_so() {
    let fs = Arc::new(Mutex::new(rig::Fs::default()));
    let s = rig::session_on(fs.clone()).await;
    let tmp = Tmp::dir("nohome");
    let local = tmp.file("f.bin", b"x");
    let e = upload_to_staging(&s, &local, KEY, &Cancel::default(), &no_progress)
        .await
        .expect_err("后端的家不在，不该写");
    assert!(e.contains("后端还没部署"), "没说清为什么：{e}");
    assert!(fs.lock().unwrap().mutated.is_empty());
}

/// 键：同一份文件同一个键；长度与字母表 == 提交那一侧认的（`files_commit::is_key`，异源：提交一侧的判定）。
#[test]
fn the_staging_key_is_deterministic_and_shaped_like_the_commit_side_wants() {
    let a = staging_key("/tmp/x", 10, 7);
    assert_eq!(a, staging_key("/tmp/x", 10, 7));
    assert_ne!(a, staging_key("/tmp/x", 11, 7));
    assert_ne!(a, staging_key("/tmp/y", 10, 7));
    assert!(
        crate::control::files_commit::is_key(&a),
        "键形状提交那一侧不认：{a}"
    );
    // 提交那一侧从同一个键拼出来的暂存件，与这里写的是同一份。
    let home = std::path::Path::new("/home/rig");
    let theirs = crate::control::files_commit::staged_path(home, &a).expect("键被拒");
    assert_eq!(theirs, home.join(staging_part(&a)));
}

// ═══ 下载 ══════════════════════════════════════════════════════════════════════════════

fn remote_file(fs: &Arc<Mutex<rig::Fs>>, path: &str, bytes: Vec<u8>) {
    let mut g = fs.lock().unwrap();
    if let Some((dir, _)) = path.rsplit_once('/') {
        g.dirs.insert(dir.to_string());
    }
    g.files.insert(path.to_string(), Entry { bytes });
}

/// 下载：字节原样落地、`.part` 改名上位后不留。
#[tokio::test]
async fn a_download_lands_verbatim_and_leaves_no_part() {
    let fs = rig::home(false, false);
    let body = rig::corpus(123_457);
    remote_file(&fs, "srv/a.bin", body.clone());
    let s = rig::session_on(fs.clone()).await;
    let tmp = Tmp::dir("dl");
    let local = tmp.path("a.bin").to_string_lossy().into_owned();
    let n = download_to_local(&s, "srv/a.bin", &local, &Cancel::default(), &no_progress)
        .await
        .unwrap();
    assert_eq!(n, body.len() as u64);
    assert_eq!(std::fs::read(&local).unwrap(), body);
    assert!(!tmp.path("a.bin.part").exists(), "`.part` 没收掉");
    assert!(
        fs.lock().unwrap().mutated.is_empty(),
        "下载在远端留下了改动"
    );
}

/// `.part` 的尾块与远端对得上 ⇒ 从那里接着读（落地仍逐字节相同）。
#[tokio::test]
async fn a_download_with_a_matching_part_resumes_from_the_verified_tail() {
    let fs = rig::home(false, false);
    let body = rig::corpus(150_000);
    remote_file(&fs, "srv/b.bin", body.clone());
    let s = rig::session_on(fs.clone()).await;
    let tmp = Tmp::dir("dl-resume");
    tmp.file("b.bin.part", &body[..70_000]);
    let local = tmp.path("b.bin").to_string_lossy().into_owned();
    let seen = std::sync::Mutex::new(Vec::<u64>::new());
    let sink = |a: u64, _b: u64| seen.lock().unwrap().push(a);
    download_to_local(&s, "srv/b.bin", &local, &Cancel::default(), &sink)
        .await
        .unwrap();
    assert_eq!(std::fs::read(&local).unwrap(), body);
    assert_eq!(
        seen.into_inner().unwrap().first(),
        Some(&70_000),
        "续传没从 `.part` 的尾巴接上"
    );
}

/// 尾块对不上 ⇒ 从 0 重来（不缝出一个坏文件）。
#[tokio::test]
async fn a_download_with_a_mismatched_part_starts_over_instead_of_stitching() {
    let fs = rig::home(false, false);
    let body = rig::corpus(90_000);
    remote_file(&fs, "srv/c.bin", body.clone());
    let s = rig::session_on(fs.clone()).await;
    let tmp = Tmp::dir("dl-mismatch");
    tmp.file("c.bin.part", &vec![0x5A; 50_000]);
    let local = tmp.path("c.bin").to_string_lossy().into_owned();
    let seen = std::sync::Mutex::new(Vec::<u64>::new());
    let sink = |a: u64, _b: u64| seen.lock().unwrap().push(a);
    download_to_local(&s, "srv/c.bin", &local, &Cancel::default(), &sink)
        .await
        .unwrap();
    assert_eq!(std::fs::read(&local).unwrap(), body);
    assert_eq!(seen.into_inner().unwrap().first(), Some(&0));
}

/// 撤 ⇒ **留** `.part`（续传的本钱）；失败 ⇒ 删 `.part`。
#[tokio::test]
async fn a_cancelled_download_keeps_its_part_and_a_failed_one_cleans_up() {
    let fs = rig::home(false, false);
    remote_file(&fs, "srv/d.bin", rig::corpus(60_000));
    let s = rig::session_on(fs.clone()).await;
    let tmp = Tmp::dir("dl-cancel");
    let local = tmp.path("d.bin").to_string_lossy().into_owned();
    let c = Cancel::default();
    c.fire();
    download_to_local(&s, "srv/d.bin", &local, &c, &no_progress)
        .await
        .expect_err("撤了");
    assert!(tmp.path("d.bin.part").exists(), "撤了 `.part` 却没了");
    assert!(!tmp.path("d.bin").exists());
    // 失败：读到第 2 块就坏（`.part` 已经建出来、写进了一块）⇒ 报错且 `.part` 被收掉。
    remote_file(&fs, "srv/e.bin", rig::corpus(200_000));
    fs.lock().unwrap().fail_read_after = Some(1);
    let local2 = tmp.path("e.bin").to_string_lossy().into_owned();
    let seen = std::sync::Mutex::new(0u64);
    let sink = |a: u64, _b: u64| *seen.lock().unwrap() = a;
    let r = download_to_local(&s, "srv/e.bin", &local2, &Cancel::default(), &sink).await;
    assert!(r.is_err(), "读坏了竟然成了");
    assert!(!tmp.path("e.bin.part").exists(), "失败之后 `.part` 还在");
    assert!(!tmp.path("e.bin").exists());
}

/// 🔴 **B6**：本机落点是一份 Claude 会话数据 ⇒ **围栏拒**（开单那一判就拒，一个字节都没碰）。
#[test]
fn a_download_onto_a_session_file_is_refused_by_the_fence() {
    let protected = "/home/u/.claude/projects/dash-proj/abc-123.jsonl";
    assert!(
        crate::agents::claudecode::paths::is_protected_session_path(std::path::Path::new(
            protected
        )),
        "夹具那条路径不被判定为受保护 —— 本条此刻在量别的东西"
    );
    let e = land_check(protected).expect_err("往会话文件上落地竟然过了围栏");
    assert!(e.contains("会话数据"), "拒的不是围栏那一句：{e}");
    // 阴性对照：一个普通的落点过得了（父目录在盘上）。
    let tmp = Tmp::dir("fence-ok");
    land_check(&tmp.path("ok.txt").to_string_lossy()).expect("普通落点该过");
    // 相对路径不认。
    assert!(land_check("rel/x.txt").is_err());
}

// ═══ 票表与帧 ══════════════════════════════════════════════════════════════════════════

fn dial() -> serde_json::Value {
    serde_json::json!({"host":"h","port":22,"user":"u","key_path":null,"host_key_fingerprint":null})
}

fn reply_of(f: &Frame) -> (bool, Option<String>, Option<serde_json::Value>) {
    match f {
        Frame::Reply { ok, code, data, .. } => (*ok, code.clone(), data.clone()),
        other => panic!("不是应答帧：{other:?}"),
    }
}

/// 开单 / 起跑 / 撤的记账：键形状 · 同键第二张票 `busy` · 起跑未知票 · 撤幂等 · 围栏拒的码。
#[tokio::test]
async fn the_desk_books_tickets_and_refuses_what_it_should_with_a_code() {
    let (tx, _rx) = mpsc::channel(64);
    let desk = Desk::new(tx);
    let tmp = Tmp::dir("desk");
    let local = tmp.file("g.bin", b"hello");
    let (ok, _, data) = reply_of(&desk.upload(
        "r1",
        &serde_json::json!({"dial": dial(), "local_path": local}),
    ));
    assert!(ok);
    let data = data.unwrap();
    let key = data["key"].as_str().unwrap().to_string();
    assert!(crate::control::files_commit::is_key(&key));
    let tid = data["id"].as_str().unwrap().to_string();
    assert!(tid.starts_with("xfer-"));
    // 同一份文件第二张票 ⇒ busy。
    let (ok, code, _) = reply_of(&desk.upload(
        "r2",
        &serde_json::json!({"dial": dial(), "local_path": local}),
    ));
    assert_eq!((ok, code.as_deref()), (false, Some("busy")));
    // 缺字段 / 不是文件。
    let (_, code, _) = reply_of(&desk.upload("r3", &serde_json::json!({"local_path": local})));
    assert_eq!(code.as_deref(), Some("bad_args"));
    let (_, code, _) = reply_of(&desk.upload(
        "r4",
        &serde_json::json!({"dial": dial(), "local_path": tmp.0.to_string_lossy()}),
    ));
    assert_eq!(code.as_deref(), Some("bad_args"));
    // 下载落点踩线 ⇒ refused。
    let (_, code, _) = reply_of(&desk.download(
        "r5",
        &serde_json::json!({"dial": dial(), "remote_path": "/x", "local_path": "/home/u/.claude/projects/p/abc-1.jsonl"}),
    ));
    assert_eq!(code.as_deref(), Some("refused"));
    // 起跑一张不在册的票。
    let (_, code, _) = reply_of(&desk.start("r6", &serde_json::json!({"id": "xfer-nope"})));
    assert_eq!(code.as_deref(), Some("no_such_transfer"));
    // 撤：没起跑的票当场摘；再撤一次幂等。
    assert_eq!(desk.len(), 1);
    assert!(reply_of(&desk.stop("r7", &serde_json::json!({"id": tid}))).0);
    assert_eq!(desk.len(), 0, "没起跑的票撤了之后还在册");
    assert!(reply_of(&desk.stop("r8", &serde_json::json!({"id": tid}))).0);
    // 摘了之后同一份文件又能开单。
    assert!(
        reply_of(&desk.upload(
            "r9",
            &serde_json::json!({"dial": dial(), "local_path": local})
        ))
        .0
    );
}

/// 起跑：拨一个拨不通的地址 ⇒ 帧流以 `failed` 收场、票摘掉；起跑两次 ⇒ `already_started`。
/// （拨不通走的是生产那一条 `open_for_transfer` —— 本条不起 SSH 服务端，量的是「失败也有终局帧」。）
#[tokio::test]
async fn a_started_transfer_always_ends_with_a_final_frame_and_leaves_the_desk() {
    let (tx, mut rx) = mpsc::channel(64);
    let desk = Desk::new(tx);
    let tmp = Tmp::dir("start");
    let local = tmp.file("h.bin", b"x");
    // 端口 1：本机上一定连不上（拒绝连接，立刻失败，不等期限）。
    let d = serde_json::json!({"host":"127.0.0.1","port":1,"user":"u","key_path":"/nonexistent","host_key_fingerprint":null});
    let (_, _, data) =
        reply_of(&desk.upload("r1", &serde_json::json!({"dial": d, "local_path": local})));
    let tid = data.unwrap()["id"].as_str().unwrap().to_string();
    assert!(reply_of(&desk.start("r2", &serde_json::json!({"id": tid}))).0);
    let (_, code, _) = reply_of(&desk.start("r3", &serde_json::json!({"id": tid})));
    assert_eq!(code.as_deref(), Some("already_started"));
    let mut frames = Vec::new();
    loop {
        let f = tokio::time::timeout(std::time::Duration::from_secs(20), rx.recv())
            .await
            .expect("终局帧 20 s 没来")
            .expect("通道关了");
        if let Frame::Transfer { id, end, .. } = &f {
            assert_eq!(id, &tid);
            let over = end.is_some();
            frames.push(f);
            if over {
                break;
            }
        }
    }
    match frames.last() {
        Some(Frame::Transfer {
            end: Some(TransferEnd::Failed { why }),
            ..
        }) => assert!(!why.is_empty()),
        other => panic!("终局不是 failed：{other:?}"),
    }
    // 票在终局帧送出之后摘掉（给 spawn 那一侧一个调度点）。
    for _ in 0..50 {
        if desk.len() == 0 {
            break;
        }
        tokio::task::yield_now().await;
    }
    assert_eq!(desk.len(), 0, "收场之后票还在册");
}

/// 撤得动**等拨号**那一段：`Cancel::wait` 在旗先落、后等的顺序下也立刻返回（不丢那一次通知）。
#[tokio::test]
async fn cancel_wakes_a_waiter_whichever_side_comes_first() {
    let c = Arc::new(Cancel::default());
    c.fire();
    tokio::time::timeout(std::time::Duration::from_secs(5), c.wait())
        .await
        .expect("旗先落、后等 —— 等的人没醒");
    let c2 = Arc::new(Cancel::default());
    let w = {
        let c2 = Arc::clone(&c2);
        tokio::spawn(async move { c2.wait().await })
    };
    tokio::task::yield_now().await;
    c2.fire();
    tokio::time::timeout(std::time::Duration::from_secs(5), w)
        .await
        .expect("先等、后落 —— 等的人没醒")
        .unwrap();
}

/// 本机流断了 ⇒ 票表被丢 ⇒ 在册的每一趟都被撤（= 旧形「连接断了 ⇒ 撤」）。
#[test]
fn dropping_the_desk_cancels_every_ticket() {
    let (tx, _rx) = mpsc::channel(4);
    let desk = Desk::new(tx);
    let tmp = Tmp::dir("drop");
    let local = tmp.file("i.bin", b"x");
    reply_of(&desk.upload(
        "r1",
        &serde_json::json!({"dial": dial(), "local_path": local}),
    ));
    let cancels: Vec<Arc<Cancel>> = lock(&desk.tickets)
        .values()
        .map(|t| Arc::clone(&t.cancel))
        .collect();
    assert_eq!(cancels.len(), 1);
    assert!(!cancels[0].is_set());
    drop(desk);
    assert!(cancels[0].is_set(), "票表丢了，在册那一趟没被撤");
}

/// 进度转发：第一格是此刻、终局那一格是最后一格；发送端堵着时中间几格**合并**（不堆积）。
#[tokio::test]
async fn progress_frames_start_now_coalesce_and_end_with_the_final_one() {
    let (tx, mut rx) = mpsc::channel(1);
    let (ptx, prx) = watch::channel(Progress::default());
    let fwd = tokio::spawn(forward_progress("xfer-9".into(), prx, tx));
    // 第一格（0/0）出去了；通道容量 1 ⇒ 下一格会堵在 send 上。
    let first = rx.recv().await.unwrap();
    assert!(matches!(
        first,
        Frame::Transfer {
            got: 0,
            total: 0,
            end: None,
            ..
        }
    ));
    for g in 1..=100u64 {
        ptx.send_modify(|p| {
            p.got = g;
            p.total = 100;
        });
    }
    ptx.send_modify(|p| p.end = Some(TransferEnd::Done { bytes: 100 }));
    let mut rest = Vec::new();
    while let Some(f) = rx.recv().await {
        rest.push(f);
    }
    fwd.await.unwrap();
    assert!(
        rest.len() < 100,
        "100 次变更出了 {} 帧 —— 没合并",
        rest.len()
    );
    match rest.last() {
        Some(Frame::Transfer {
            got: 100,
            end: Some(TransferEnd::Done { bytes: 100 }),
            ..
        }) => {}
        other => panic!("最后一格不是终局：{other:?}"),
    }
}
