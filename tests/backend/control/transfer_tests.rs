//! `control/transfer.rs` 的判据：传输台住本机常驻后端。
//!
//! 台架：`dial::sftp::rig`（合成 SFTP 服务端，逐条记改动路径与写偏移）＋ 本机临时目录。
//! 上传 / 下载两份本体的语料是一条 SFTP 会话（`Session::over` —— 生产那一个口），判据直接喂它；
//! 票表（`Desk`）那一半判记账与帧，不起真 SSH（真 sshd 那一维在 `tests/evidence/SR1b-sftp-loopback.py`）。
//!
//! 来历：这几条的形状照 monitor 那一侧 F7c（暂存区那一族）/ 步 24（秤 F4 · 续传）的判据 ——
//! 传输本体搬过来了，判据跟着搬（monitor 那几份整份删了），被判的是后端这一份实现。

use super::*;
use crate::dial::sftp::rig::{self, Entry};
use crate::platform::fs::Face;
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

/// 🔴🔴 **暂存区之外零写**：成功 · 失败 · 续传 · 撤四趟，
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
    // 正控：同一张表认得出一次暂存区外的写（SFTP 写根只剩暂存区、围栏造不出这一笔 ⇒ 直接记进服务端那张改动表）。
    fs.lock()
        .unwrap()
        .mutated
        .push(("mkdir".to_string(), ".cc-monitor/bin".to_string()));
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
    let (n, sha) = upload_to_staging(&s, &local, KEY, &Cancel::default(), &sink)
        .await
        .unwrap();
    assert_eq!(n, body.len() as u64);
    // 整份摘要 == 本机那份的（另一份实现对拍，异源）。
    assert_eq!(sha, sha2_hex(&body), "上传交的摘要不是本机那份的");
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
///
/// 这一条在负载 ~18 的机器上全量跑时红过一次（单跑绿）。病根不是墙钟，是**在路上的写**：
/// `russh-sftp` 的写是流水线的，第一条坏回话就让上传返回，后面已发出的写还没被服务端处理；本条随即把
/// `fail_write_after` 清掉 ⇒ 那几条晚到的写**成功**落盘（`write_offsets` 第一格不再是续传起点，暂存件中间留一个洞）。
/// 调度快的时候它们恰好在返回之前处理完 —— 所以只在负载高时现形。
/// ⇒ 产品侧修（上传失败后把已发出的写全部等到回话再走，`transfer.rs::drain_sent_writes`），台架侧把那个竞态
/// 从运气变成设定（`yield_per_write`：服务端每条写先让出几次），并**按事件判**：上传返回的那一刻，服务端已经看到
/// 客户端发出的全部写（盖到整份的长度）—— 不看墙钟、不 sleep。
#[tokio::test]
async fn a_failed_upload_keeps_the_part_and_the_retry_resumes_from_its_tail() {
    let fs = rig::home(false, true);
    let s = rig::session_on(fs.clone()).await;
    let tmp = Tmp::dir("resume-up");
    let body = rig::corpus(200_000);
    let local = tmp.file("c.bin", &body);
    {
        let mut g = fs.lock().unwrap();
        g.fail_write_after = Some(2);
        g.yield_per_write = 8;
    }
    upload_to_staging(&s, &local, KEY, &Cancel::default(), &no_progress)
        .await
        .expect_err("第 3 块坏");
    assert_eq!(
        fs.lock().unwrap().seen_write_end,
        body.len() as u64,
        "上传返回时还有写在路上（服务端没看到的那几条会在之后落盘 —— 暂存件中间留洞）"
    );
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
        g.yield_per_write = 0;
        g.write_offsets.clear();
    }
    let (_, sha) = upload_to_staging(&s, &local, KEY, &Cancel::default(), &no_progress)
        .await
        .expect("续传该成");
    assert_eq!(
        fs.lock().unwrap().write_offsets.first(),
        Some(&(have as u64)),
        "续传没从尾块接上"
    );
    // 续传那一趟交的也是**整份**的摘要（前缀在本机读一遍算进去），不是只算接上之后那一截。
    assert_eq!(sha, sha2_hex(&body), "续传交的摘要没把前缀算进去");
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
    assert_eq!(
        e.said,
        copy_core::copy_text("beTransfer.upload.notDeployed", &[]),
        "没说清为什么"
    );
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

/// `### FILES2` Q4「落到 Linux 本机 ⇒ 字节原样当文件名」。
/// 本机落点是非 UTF-8 的原始字节（线上 `local_path: {"b16": …}`）⇒ 落出来的那份文件名逐字节就是它，`.part` 不留。
#[cfg(unix)]
#[tokio::test]
async fn a_download_lands_under_a_non_utf8_local_name_byte_for_byte() {
    use std::os::unix::ffi::OsStrExt as _;
    let fs = rig::home(false, false);
    remote_file(&fs, "srv/x.bin", b"BYTES".to_vec());
    let s = rig::session_on(fs.clone()).await;
    let tmp = Tmp::dir("dl-raw");
    let name = std::ffi::OsStr::from_bytes(b"f\xfe\xff.bin");
    let local = tmp.path("").join(name);
    let args = serde_json::json!({
        "local_path": crate::files::raw::to_json(crate::files::raw::path_bytes(&local)),
    });
    let landed: cmd_wire::files::WirePath =
        serde_json::from_value(args["local_path"].clone()).expect("b16 落点没认出来");
    assert_eq!(crate::common::path_wire::to_path_buf(landed.bytes()), local);
    download_to_local(
        &Face::issue(),
        &s,
        "srv/x.bin",
        &local,
        &Cancel::default(),
        &no_progress,
    )
    .await
    .unwrap();
    assert_eq!(std::fs::read(&local).unwrap(), b"BYTES");
    let mut part = name.to_os_string();
    part.push(".part");
    assert!(
        std::fs::symlink_metadata(tmp.path("").join(part)).is_err(),
        "`.part` 没收掉"
    );
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
    let n = download_to_local(
        &Face::issue(),
        &s,
        "srv/a.bin",
        &local,
        &Cancel::default(),
        &no_progress,
    )
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
    download_to_local(
        &Face::issue(),
        &s,
        "srv/b.bin",
        &local,
        &Cancel::default(),
        &sink,
    )
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
    download_to_local(
        &Face::issue(),
        &s,
        "srv/c.bin",
        &local,
        &Cancel::default(),
        &sink,
    )
    .await
    .unwrap();
    assert_eq!(std::fs::read(&local).unwrap(), body);
    assert_eq!(seen.into_inner().unwrap().first(), Some(&0));
}

/// 撤 ⇒ **留** `.part`（续传的本钱）。失败（读到半路那台答坏 / 连接没了）⇒ **也留**；只有一个字节都没落的空 `.part` 才清。
#[tokio::test]
async fn a_cancelled_or_failed_download_keeps_its_part_unless_nothing_landed() {
    let fs = rig::home(false, false);
    remote_file(&fs, "srv/d.bin", rig::corpus(60_000));
    let s = rig::session_on(fs.clone()).await;
    let tmp = Tmp::dir("dl-cancel");
    let local = tmp.path("d.bin").to_string_lossy().into_owned();
    let c = Cancel::default();
    c.fire();
    download_to_local(&Face::issue(), &s, "srv/d.bin", &local, &c, &no_progress)
        .await
        .expect_err("撤了");
    assert!(tmp.path("d.bin.part").exists(), "撤了 `.part` 却没了");
    assert!(!tmp.path("d.bin").exists());
    // 失败，一个字节都没落（第一读就坏）⇒ 空 `.part` 清掉。
    remote_file(&fs, "srv/e.bin", rig::corpus(200_000));
    fs.lock().unwrap().fail_read_after = Some(0);
    let local2 = tmp.path("e.bin").to_string_lossy().into_owned();
    let r = download_to_local(
        &Face::issue(),
        &s,
        "srv/e.bin",
        &local2,
        &Cancel::default(),
        &no_progress,
    )
    .await;
    assert!(r.is_err(), "读坏了竟然成了");
    assert!(
        !tmp.path("e.bin.part").exists(),
        "一个字节都没落，空 `.part` 却留着"
    );
    assert!(!tmp.path("e.bin").exists());
}

/// **T1：下载读到半路断了 ⇒ `.part` 留着，重拖同一份从它的尾巴接上**（「失败留」· NT1 报备 2）。
///
/// 判据全用字节数的相等：① 留下的 `.part` 恰是那一份的前缀（长度 L > 0，逐字节 == 源的前 L 字节）；
/// ② 重拖那一趟，服务端（台架自己记，异源）收到的偏移 < L 的读 **恰好**是尾块对拍那一格（`L − min(L, 块)`）——
/// 前缀没有被重新读一遍；③ 落地逐字节 == 源。
#[tokio::test]
async fn a_download_cut_off_midway_keeps_its_part_and_the_retry_resumes_from_its_tail() {
    let fs = rig::home(false, false);
    // 远大于一趟读回来的量（客户端一次读请求可以要很多块）：两趟读之后就坏，落下的必是半截。
    let body = rig::corpus(4_000_000);
    remote_file(&fs, "srv/f.bin", body.clone());
    let s = rig::session_on(fs.clone()).await;
    let tmp = Tmp::dir("dl-cut");
    let local = tmp.path("f.bin").to_string_lossy().into_owned();
    fs.lock().unwrap().fail_read_after = Some(2);
    download_to_local(
        &Face::issue(),
        &s,
        "srv/f.bin",
        &local,
        &Cancel::default(),
        &no_progress,
    )
    .await
    .expect_err("读到半路坏了");
    let part = std::fs::read(tmp.path("f.bin.part")).expect("断了之后 `.part` 该留着");
    let l = part.len();
    assert!(l > 0 && l < body.len(), "`.part` 长度 {l} 不是半截");
    assert_eq!(part, body[..l], "`.part` 不是源的前缀");
    assert!(!tmp.path("f.bin").exists(), "半截就上位了");
    {
        let mut g = fs.lock().unwrap();
        g.fail_read_after = None;
        g.read_offsets.clear();
    }
    download_to_local(
        &Face::issue(),
        &s,
        "srv/f.bin",
        &local,
        &Cancel::default(),
        &no_progress,
    )
    .await
    .expect("续传该成");
    let probe = (l as u64).min(CHUNK as u64);
    let below: Vec<u64> = fs
        .lock()
        .unwrap()
        .read_offsets
        .iter()
        .copied()
        .filter(|o| *o < l as u64)
        .collect();
    assert_eq!(
        below,
        vec![l as u64 - probe],
        "前缀被重新读了（或尾块没对）"
    );
    assert_eq!(std::fs::read(&local).unwrap(), body, "落地不是源的字节");
    assert!(!tmp.path("f.bin.part").exists(), "上位之后 `.part` 还在");
}

/// **续传不多读一个预读窗**（「一次续传多读一个预读窗口」· DP1 报备 9）。
///
/// 判据是字节数的相等（台架自己记、异源）：重拖那一趟服务端交出去的读字节 **恰好** == 从探针起点到末尾的长度
/// （`total − (have − 探针)`）—— 每个字节只交一次。探针之后 seek 一下（哪怕 seek 回原地）⇒ russh-sftp 的读缓冲被扔掉、
/// 从 `have` 起那一截再要一遍 ⇒ 多出「第一读的量 − 探针」那么多（真 sshd 上是 228 352 字节）⇒ 红。
/// ⚠ 源要比第一读的量（服务端最大包）长得多，前缀也要比探针长：否则两种写法交出去的一样多、分不开。
#[tokio::test]
async fn a_resumed_download_asks_for_every_byte_from_the_probe_on_exactly_once() {
    let fs = rig::home(false, false);
    let body = rig::corpus(3_000_000);
    remote_file(&fs, "srv/g.bin", body.clone());
    let s = rig::session_on(fs.clone()).await;
    let tmp = Tmp::dir("dl-no-reseek");
    let have = 700_000usize;
    tmp.file("g.bin.part", &body[..have]);
    let local = tmp.path("g.bin").to_string_lossy().into_owned();
    fs.lock().unwrap().served_bytes = 0;
    download_to_local(
        &Face::issue(),
        &s,
        "srv/g.bin",
        &local,
        &Cancel::default(),
        &no_progress,
    )
    .await
    .expect("续传该成");
    let probe = (have as u64).min(CHUNK as u64);
    assert_eq!(
        fs.lock().unwrap().served_bytes,
        body.len() as u64 - (have as u64 - probe),
        "续传那一趟有字节被要了两遍（探针之后又 seek 了一次，读缓冲被扔掉）"
    );
    assert_eq!(std::fs::read(&local).unwrap(), body, "落地不是源的字节");
}

/// 🔴 **B6**：本机落点是一份 Claude 会话记录的形状 ⇒ **照样开得出单**。
///
/// 从前这一格叫「落点是会话数据 ⇒ 围栏拒」。用户「文件管理器全部都可以改. 不需要任何围栏」⇒
/// 下载落点只过路径解析（绝对路径 · 有文件名 · 父目录在盘上、解开之后落点仍在它底下）。
#[test]
fn a_download_onto_a_session_file_is_let_through() {
    let tmp = Tmp::dir("fence-session");
    std::fs::create_dir_all(tmp.path("projects/-x")).expect("铺会话目录");
    let session = tmp.path("projects/-x/abc-123.jsonl");
    assert!(
        crate::agents::claudecode::paths::is_session_record_path(&session),
        "夹具那条路径不是会话记录的形状 —— 本条此刻在量别的东西"
    );
    land_check(session.to_string_lossy().as_ref()).expect("🔴 往会话文件那个位置上落地被拒了");
    // 阴性对照：父目录不在盘上 ⇒ 拒（路径解析那一关）。
    assert!(land_check(tmp.path("nope/x.txt").to_string_lossy().as_ref()).is_err());
    // 相对路径不认。
    assert!(land_check("rel/x.txt").is_err());
}

// ═══ 票表与帧 ══════════════════════════════════════════════════════════════════════════

fn dial() -> serde_json::Value {
    serde_json::json!({"machine": {"host":"h","port":22,"user":"u"}})
}

fn reply_of(f: &Frame) -> (bool, Option<String>, Option<serde_json::Value>) {
    match f {
        Frame::Reply(crate::wire::Reply { ok, code, data, .. }) => {
            // 开单两条收一组：下面那些判据问的是一项的开法 ⇒ 一组一项那一项没开成就当整条没开成，开了就交那一项（`key` ＝ 暂存件的键）。
            match data
                .as_ref()
                .and_then(|d| d.get("items"))
                .and_then(|i| i.get(0))
            {
                Some(it) if it["state"] == "failed" => {
                    (false, it["code"].as_str().map(str::to_string), None)
                }
                Some(it) => {
                    let mut v = serde_json::json!({ "id": it["id"] });
                    if let Some(k) = it.get("staged") {
                        v["key"] = k.clone();
                    }
                    (true, None, Some(v))
                }
                None => (*ok, code.clone(), data.clone()),
            }
        }
        other => panic!("不是应答帧：{other:?}"),
    }
}

/// 一项的开单问法 ⇒ 一组一项（上传：`items: [{local_path}]`；下载：落点拆成目录 ＋ 落名、同名照盖）。缺的格照缺（判「缺一格 ⇒ bad_args」）。
fn batched(cmd: &str, a: &serde_json::Value) -> serde_json::Value {
    let mut o = a.as_object().cloned().unwrap_or_default();
    let mut item = serde_json::Map::new();
    if cmd == TRANSFER_UPLOAD {
        if let Some(l) = o.remove("local_path") {
            item.insert("local_path".into(), l);
        }
        o.entry("remote_dir").or_insert(serde_json::json!("/r"));
    } else {
        if let Some(r) = o.remove("remote_path") {
            item.insert("remote_path".into(), r);
        }
        if let Some(l) = o.remove("local_path") {
            let bytes = crate::common::path_wire::from_json(&l).unwrap_or_default();
            let p = crate::common::path_wire::to_path_buf(&bytes);
            let wire = |q: &Path| crate::files::raw::to_json(crate::files::raw::path_bytes(q));
            o.insert(
                "local_dir".into(),
                wire(p.parent().unwrap_or(Path::new("/"))),
            );
            item.insert(
                "name".into(),
                wire(Path::new(p.file_name().unwrap_or_default())),
            );
            o.insert("on_exists".into(), serde_json::json!("overwrite"));
        }
    }
    o.insert("items".into(), serde_json::json!([item]));
    serde_json::Value::Object(o)
}

/// 一个不等任何东西的 future 当场取值（下面两份落点替身都在本机盘上当场答）。
fn ready<T>(f: impl std::future::Future<Output = T>) -> T {
    let mut f = std::pin::pin!(f);
    match f
        .as_mut()
        .poll(&mut std::task::Context::from_waker(std::task::Waker::noop()))
    {
        std::task::Poll::Ready(v) => v,
        std::task::Poll::Pending => panic!("落点替身该当场答"),
    }
}

/// 远端落点上什么都没有（开单那几条判据不问同名）。
struct Bare;
impl Landing for Bare {
    fn look<'a>(&'a self, _: &'a [u8]) -> crate::control::transfer_copy::Fut<'a, Option<Seen>> {
        Box::pin(async { None })
    }
}

/// 本机一个目录当远端落点（判同名那几条）：照盘上那一格答（不跟链接）。
struct Here;
impl Landing for Here {
    fn look<'a>(&'a self, path: &'a [u8]) -> crate::control::transfer_copy::Fut<'a, Option<Seen>> {
        let m = Ro::symlink_metadata(crate::common::path_wire::to_path_buf(path)).ok();
        Box::pin(async move {
            let m = m?;
            Some(Seen {
                kind: if m.is_dir() {
                    cwf::FileKind::Dir
                } else {
                    cwf::FileKind::File
                },
                size: m.is_file().then(|| m.len()),
                mtime: crate::common::batch_book::mtime_secs(m.modified()),
            })
        })
    }
}

impl Desk {
    /// 上传开单经它的本体（[`upload_with`]，帧面那一发同一份）问、落点照 `land` 答，回应答帧。
    fn upload_frame(
        &self,
        id: &str,
        args: &serde_json::Value,
        land: &dyn Landing,
        tz: &crate::Tz,
    ) -> Frame {
        typed::answer::<TransferUpload>(id, args, |req| {
            ready(upload_with(&self.tickets, land, req, tz))
        })
    }

    /// 一项的开单问法经答口问（同 [`batched`]）；上传那一条的落点上什么都没有。
    fn ask1(&self, cmd: &str, id: &str, a: &serde_json::Value) -> Frame {
        if cmd == TRANSFER_UPLOAD {
            return self.upload_frame(id, &batched(cmd, a), &Bare, &crate::Tz::default());
        }
        self.answer_wire(cmd, id, &batched(cmd, a), &crate::Tz::default())
    }
}

/// 开单 / 起跑 / 撤的记账：键形状 · 同键第二张票 `busy` · 起跑未知票 · 撤幂等 · 路径解析拒的码。
#[tokio::test]
async fn the_desk_books_tickets_and_refuses_what_it_should_with_a_code() {
    let (tx, _rx) = mpsc::channel(64);
    let desk = Desk::new(tx, crate::platform::fs::Face::issue());
    let tmp = Tmp::dir("desk");
    let local = tmp.file("g.bin", b"hello");
    let (ok, _, data) = reply_of(&desk.ask1(
        TRANSFER_UPLOAD,
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
    let (ok, code, _) = reply_of(&desk.ask1(
        TRANSFER_UPLOAD,
        "r2",
        &serde_json::json!({"dial": dial(), "local_path": local}),
    ));
    assert_eq!((ok, code.as_deref()), (false, Some("busy")));
    // 缺字段 / 不是文件。
    let (_, code, _) = reply_of(&desk.ask1(
        TRANSFER_UPLOAD,
        "r3",
        &serde_json::json!({"local_path": local}),
    ));
    assert_eq!(code.as_deref(), Some("bad_args"));
    let (_, code, _) = reply_of(&desk.ask1(
        TRANSFER_UPLOAD,
        "r4",
        &serde_json::json!({"dial": dial(), "local_path": tmp.0.to_string_lossy()}),
    ));
    // 不是普通文件：一组里那一项没开成（`refused`），不是整条入参不对。
    assert_eq!(code.as_deref(), Some("refused"));
    // 下载落点过不了路径解析（父目录不在）⇒ refused。从前这里的语料是一份会话文件，今天那一形放行。
    let a = serde_json::json!({"dial": dial(), "remote_path": "/x", "local_path": tmp.0.join("nope/abc-1.jsonl").to_string_lossy()});
    let (_, code, _) = reply_of(&desk.ask1(TRANSFER_DOWNLOAD, "r5", &a));
    assert_eq!(code.as_deref(), Some("refused"));
    // 另一向：一个规整的本机落点 ⇒ 开得出单（〔SR1b 死值验〕只判「踩线 ⇒ refused」的话，
    //   围栏换成「一律拒」本条照绿 —— 那一刀当场没砍中，补这一向）。另起一张台，不动上面那张的册数。
    let (tx2, _rx2) = mpsc::channel(8);
    let desk2 = Desk::new(tx2, crate::platform::fs::Face::issue());
    let a = serde_json::json!({"dial": dial(), "remote_path": "/x", "local_path": tmp.0.join("dl.bin").to_string_lossy()});
    let (ok, code, data) = reply_of(&desk2.ask1(TRANSFER_DOWNLOAD, "r5b", &a));
    assert!(ok, "规整的下载落点开不出单：{code:?}");
    assert!(data.unwrap()["id"]
        .as_str()
        .is_some_and(|i| i.starts_with("xfer-")));
    // 起跑一张不在册的票。
    let (_, code, _) = reply_of(&desk.answer_wire(
        TRANSFER_START,
        "r6",
        &serde_json::json!({"id": "xfer-nope"}),
        &crate::Tz::default(),
    ));
    assert_eq!(code.as_deref(), Some("no_such_transfer"));
    // 撤：没起跑的票当场摘；再撤一次幂等。
    assert_eq!(desk.len(), 1);
    assert!(
        reply_of(&desk.answer_wire(
            TRANSFER_STOP,
            "r7",
            &serde_json::json!({"id": tid}),
            &crate::Tz::default()
        ))
        .0
    );
    assert_eq!(desk.len(), 0, "没起跑的票撤了之后还在册");
    assert!(
        reply_of(&desk.answer_wire(
            TRANSFER_STOP,
            "r8",
            &serde_json::json!({"id": tid}),
            &crate::Tz::default()
        ))
        .0
    );
    // 摘了之后同一份文件又能开单。
    assert!(
        reply_of(&desk.ask1(
            TRANSFER_UPLOAD,
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
    let desk = Desk::new(tx, crate::platform::fs::Face::issue());
    let tmp = Tmp::dir("start");
    let local = tmp.file("h.bin", b"x");
    // 端口 1：本机上一定断开（拒绝连接，立刻失败，不等期限）。
    let d = serde_json::json!({"machine": {"host":"127.0.0.1","port":1,"user":"u","keyPath":"/nonexistent"}});
    let (_, _, data) = reply_of(&desk.ask1(
        TRANSFER_UPLOAD,
        "r1",
        &serde_json::json!({"dial": d, "local_path": local}),
    ));
    let tid = data.unwrap()["id"].as_str().unwrap().to_string();
    assert!(
        reply_of(&desk.answer_wire(
            TRANSFER_START,
            "r2",
            &serde_json::json!({"id": tid}),
            &crate::Tz::default()
        ))
        .0
    );
    let (_, code, _) = reply_of(&desk.answer_wire(
        TRANSFER_START,
        "r3",
        &serde_json::json!({"id": tid}),
        &crate::Tz::default(),
    ));
    assert_eq!(code.as_deref(), Some("already_started"));
    let mut frames = Vec::new();
    loop {
        let f = tokio::time::timeout(guard_core::wall::CEILING, rx.recv())
            .await
            .expect("终局帧 20 s 没来")
            .expect("通道关了");
        if let Frame::Transfer(crate::wire::Transfer { id, end, .. }) = &f {
            assert_eq!(id, &tid);
            let over = end.is_some();
            frames.push(f);
            if over {
                break;
            }
        }
    }
    match frames.last() {
        Some(Frame::Transfer(crate::wire::Transfer {
            end: Some(TransferEnd::Failed { why, .. }),
            ..
        })) => assert!(!why.is_empty()),
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
    tokio::time::timeout(guard_core::wall::CEILING, c.wait())
        .await
        .expect("旗先落、后等 —— 等的人没醒");
    let c2 = Arc::new(Cancel::default());
    let w = {
        let c2 = Arc::clone(&c2);
        tokio::spawn(async move { c2.wait().await })
    };
    tokio::task::yield_now().await;
    c2.fire();
    tokio::time::timeout(guard_core::wall::CEILING, w)
        .await
        .expect("先等、后落 —— 等的人没醒")
        .unwrap();
}

/// 本机流断了 ⇒ 票表被丢 ⇒ 在册的每一趟都被撤（= 旧形「连接断了 ⇒ 撤」）。
#[test]
fn dropping_the_desk_cancels_every_ticket() {
    let (tx, _rx) = mpsc::channel(4);
    let desk = Desk::new(tx, crate::platform::fs::Face::issue());
    let tmp = Tmp::dir("drop");
    let local = tmp.file("i.bin", b"x");
    reply_of(&desk.ask1(
        TRANSFER_UPLOAD,
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
        Frame::Transfer(crate::wire::Transfer {
            got: 0,
            total: 0,
            end: None,
            ..
        })
    ));
    for g in 1..=100u64 {
        ptx.send_modify(|p| {
            p.got = g;
            p.total = 100;
        });
    }
    ptx.send_modify(|p| {
        p.end = Some(TransferEnd::Done {
            bytes: 100,
            sha256: None,
        })
    });
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
        Some(Frame::Transfer(crate::wire::Transfer {
            got: 100,
            end: Some(TransferEnd::Done { bytes: 100, .. }),
            ..
        })) => {}
        other => panic!("最后一格不是终局：{other:?}"),
    }
}

/// 另一份 SHA-256 实现（`sha2`，只在测试期链接）—— 异源对拍。
fn sha2_hex(bytes: &[u8]) -> String {
    use sha2::Digest as _;
    sha2::Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// **「前缀 ＋ 洞 ＋ 尾巴」尾块对拍看不见，提交那一下看得见**。
///
/// 要求：「只看长度会缝出一个坏文件」＋ §7 第 8 条「续传的尾块对拍看不见中间的洞 …… 要堵得换对拍方式」。
/// 形状（逐步，台架与真盘各半）：① 上传到半路坏了、暂存件留着；② 在暂存件**中间**改坏一截（演「失败后晚到的写留下的洞」，
/// 尾巴原样）；③ 重拖 ⇒ 尾块对得上、**照样续传**（这一步证「尾块对拍确实看不见」—— 正控，不是缺陷被修掉的地方）；
/// ④ 把合成服务端上那份暂存件原样落到一个真 home 的暂存区里，拿上传交的摘要去 `commit_upload` ⇒ `stale`、目标不在、
/// 坏暂存件被删；⑤ 阴性：同样的流程不改坏 ⇒ 提交成、落地逐字节 == 本机那份。
#[tokio::test]
async fn a_hole_the_tail_probe_cannot_see_is_caught_by_the_commit() {
    for corrupt in [true, false] {
        let fs = rig::home(false, true);
        let s = rig::session_on(fs.clone()).await;
        let tmp = Tmp::dir(if corrupt { "hole" } else { "hole-neg" });
        let body = rig::corpus(300_000);
        let local = tmp.file("h.bin", &body);
        fs.lock().unwrap().fail_write_after = Some(4);
        upload_to_staging(&s, &local, KEY, &Cancel::default(), &no_progress)
            .await
            .expect_err("半路坏");
        let part = staging_part(KEY);
        let have = fs.lock().unwrap().bytes(&part).expect("暂存件该留着").len();
        // 洞落在 `[have/4, have/4 + 1000)`：have ≥ 2 块时它整个在尾块 `[have − 块, have)` 之前 ⇒ 尾块对拍看不见它。
        assert!(have >= 2 * CHUNK, "夹具没造出够长的前缀（{have}）");
        let hole = have / 4..have / 4 + 1000;
        assert!(
            hole.end <= have - CHUNK,
            "洞碰到了尾块，这条就判不出「尾块看不见」"
        );
        if corrupt {
            let mut g = fs.lock().unwrap();
            let e = g.files.get_mut(&part).unwrap();
            for b in &mut e.bytes[hole] {
                *b ^= 0xA5;
            }
        }
        {
            let mut g = fs.lock().unwrap();
            g.fail_write_after = None;
            g.write_offsets.clear();
        }
        let (n, sha) = upload_to_staging(&s, &local, KEY, &Cancel::default(), &no_progress)
            .await
            .expect("续传该成");
        assert_eq!(n, body.len() as u64);
        assert_eq!(
            fs.lock().unwrap().write_offsets.first(),
            Some(&(have as u64)),
            "尾块对拍该照样放行续传（它看不见中间那一截 —— 本条要证的正是这一格）"
        );
        let home = tmp.path("home");
        let staging = home.join(crate::control::files_commit::STAGING_DIR);
        std::fs::create_dir_all(&staging).unwrap();
        let staged_bytes = fs.lock().unwrap().bytes(&part).unwrap();
        std::fs::write(staging.join(format!("{KEY}.part")), &staged_bytes).unwrap();
        let root = tmp.path("dest");
        std::fs::create_dir_all(&root).unwrap();
        let r = crate::control::files_commit::commit_upload(
            &Face::issue(),
            &home,
            KEY,
            &root,
            "h.bin",
            false,
            &sha,
        );
        if corrupt {
            let e = r.expect_err("中间有洞的暂存件竟然上位了");
            assert_eq!(e.code(), "stale", "{}", e.message());
            assert!(!root.join("h.bin").exists(), "坏的那份落进了目标");
            assert!(
                !staging.join(format!("{KEY}.part")).exists(),
                "坏暂存件没删 —— 下一次续传会接在它上面"
            );
        } else {
            r.expect("没改坏的那份该提交成");
            assert_eq!(std::fs::read(root.join("h.bin")).unwrap(), body);
        }
    }
}

/// `### FILES2` Q5「连上时比 SFTP `realpath(".")` 与那台后端的 `$HOME`；不一致 ⇒
/// 这台的上传改走后端链路分块写」。判定：去尾 `/` 逐字节相等才算一致（正）；chroot 形（`/` 对 `/home/u`）· 别的目录（反）⇒ 说出两个路径。
#[test]
fn the_start_dir_check_passes_the_same_home_and_names_both_paths_otherwise() {
    for (sftp, home) in [("/home/u", "/home/u"), ("/home/u/", "/home/u"), ("/", "/")] {
        assert_eq!(start_dir_mismatch(sftp, home), None, "{sftp} vs {home}");
    }
    for (sftp, home) in [
        ("/", "/home/u"),
        ("/data/u", "/home/u"),
        ("/home/u2", "/home/u"),
    ] {
        let why =
            start_dir_mismatch(sftp, home).unwrap_or_else(|| panic!("{sftp} vs {home} 判成一致了"));
        assert!(
            why.contains(sftp) && why.contains(home),
            "没说出两个路径：{why}"
        );
    }
    assert_eq!(SFTP_HOME_MISMATCH, "sftp_home_mismatch");
}

/// 复制详情：传坏了 ⇒ 那一句只说原因词，下层原话交给 `raw`（收场帧的 `detail` 从它来），不上句子。
#[tokio::test]
async fn a_failed_upload_says_a_reason_word_and_hands_the_raw_text_on() {
    let fs = rig::home(false, true);
    let s = rig::session_on(fs.clone()).await;
    let tmp = Tmp::dir("raw-up");
    let local = tmp.file("c.bin", &rig::corpus(200_000));
    fs.lock().unwrap().fail_write_after = Some(1);
    let e = upload_to_staging(&s, &local, KEY, &Cancel::default(), &no_progress)
        .await
        .expect_err("第 2 块坏");
    assert!(
        copy_core::copy_matches("beTransfer.upload.writeRemoteFailed", &e.said),
        "{}",
        e.said
    );
    let raw = e
        .raw
        .clone()
        .unwrap_or_else(|| panic!("原话没交出来：{}", e.said));
    assert!(
        !raw.trim().is_empty() && !e.said.contains(&raw),
        "{} / {raw}",
        e.said
    );
}

/// 收场帧：失败那一形多一格 `detail`（复制详情那几行：时刻 · 机器 · 命令 · 原话），缺 ⇒ 不出这一格。
#[test]
fn a_failed_transfer_end_carries_the_detail_beside_the_sentence() {
    let end = crate::wire::TransferEnd::failed(
        "写入远端失败 · 原因不明".into(),
        None,
        super::TRANSFER_START,
        Some("夹具原话"),
    );
    let v = serde_json::to_value(&end).unwrap();
    let d = v["detail"].as_str().expect("没有 detail");
    assert!(
        d.contains(&format!(
            "{}：夹具原话",
            copy_core::copy_text("detail.label.raw", &[])
        )),
        "{d}"
    );
    assert!(
        d.contains(&format!(
            "{}：transfer-start",
            copy_core::copy_text("detail.label.command", &[])
        )),
        "{d}"
    );
    let bare = crate::wire::TransferEnd::Failed {
        why: "x".into(),
        code: None,
        detail: None,
    };
    assert!(serde_json::to_value(&bare).unwrap().get("detail").is_none());
}

// ═══ 开单两条的线上字节（换形之前用旧代码跑出来钉的）══════════════════════════════════

/// 一帧应答的线上字节，票号 · 键 · 夹具目录换成占位（它们每趟不同）。
fn pinned(f: &Frame, tmp: &Tmp) -> String {
    let mut s = serde_json::to_string(f).unwrap();
    if let Frame::Reply(crate::wire::Reply { data: Some(d), .. }) = f {
        // 一组一项：那一项的票号与暂存件的键。
        let d = d.get("items").and_then(|i| i.get(0)).unwrap_or(d);
        if let Some(v) = d.get("staged").and_then(serde_json::Value::as_str) {
            if crate::control::files_commit::is_key(v) {
                s = s.replace(v, "<key>");
            }
        }
        if let Some(v) = d.get("id").and_then(serde_json::Value::as_str) {
            if v.strip_prefix("xfer-")
                .is_some_and(|n| n.bytes().all(|b| b.is_ascii_digit()))
            {
                s = s.replace(v, "xfer-<n>");
            }
        }
        if let Some(v) = d.get("key").and_then(serde_json::Value::as_str) {
            if crate::control::files_commit::is_key(v) {
                s = s.replace(v, "<key>");
            }
        }
    }
    s = s.replace(&tmp.0.to_string_lossy().into_owned(), "<tmp>");
    // 详情第二行（这台的系统 · 架构 · 版本）因机器而异。
    if let Some(i) = s.find("\\n机器：") {
        let j = i + 2 + s[i + 2..].find("\\n").unwrap();
        s.replace_range(i..j, "\\n<machine>");
    }
    // 详情首行的时刻占位（`⟦at:<秒>⟧`）。
    while let Some(i) = s.find("⟦at:") {
        let j = i + s[i..].find('⟧').unwrap();
        s.replace_range(i..j + '⟧'.len_utf8(), "<at>");
    }
    s
}

/// 开单两条经答口（`answer_wire`，帧面硬臂进来的那一处）回的帧：开了单 · 一项没开成（`io_failed` 带原话 · `refused` · `busy` ·
/// `too_many_transfers` 都在那一项上）· 整条入参不对（`bad_args` 两种）。开单换成收一组时有意改了形：回包是逐项，一项没开成不再是整条失败。
#[cfg(unix)]
#[tokio::test]
async fn the_two_openers_answer_with_the_same_bytes() {
    let (tx, _rx) = mpsc::channel(64);
    let desk = Desk::new(tx, crate::platform::fs::Face::issue());
    let tmp = Tmp::dir("pin");
    let local = tmp.file("g.bin", b"hello");
    let other = tmp.file("h.bin", b"world");
    let gone = tmp.path("gone.bin").to_string_lossy().into_owned();
    let up = |id: &str, a: serde_json::Value| pinned(&desk.ask1(TRANSFER_UPLOAD, id, &a), &tmp);
    let down = |id: &str, a: serde_json::Value| pinned(&desk.ask1(TRANSFER_DOWNLOAD, id, &a), &tmp);
    let mut got = vec![
        up(
            "u1",
            serde_json::json!({"dial": dial(), "local_path": local}),
        ),
        up(
            "u2",
            serde_json::json!({"dial": dial(), "local_path": other, "home": "/h"}),
        ),
        up(
            "u3",
            serde_json::json!({"dial": dial(), "local_path": local}),
        ),
        up("u4", serde_json::json!({"local_path": local})),
        up(
            "u5",
            serde_json::json!({"dial": dial(), "local_path": tmp.0.to_string_lossy()}),
        ),
        up(
            "u6",
            serde_json::json!({"dial": dial(), "local_path": gone}),
        ),
        up(
            "u7",
            serde_json::json!({"dial": dial(), "local_path": local, "via": "x"}),
        ),
        down(
            "d1",
            serde_json::json!({"dial": dial(), "remote_path": "/x", "local_path": tmp.path("dl.bin").to_string_lossy()}),
        ),
        down(
            "d2",
            serde_json::json!({"dial": dial(), "remote_path": "/x", "local_path": {"b16": hex_of(&tmp.path("d\u{ff}.bin"))}}),
        ),
        down(
            "d3",
            serde_json::json!({"dial": dial(), "remote_path": "/x", "local_path": tmp.path("nope/a.bin").to_string_lossy()}),
        ),
        down(
            "d4",
            serde_json::json!({"dial": dial(), "local_path": tmp.path("dl.bin").to_string_lossy()}),
        ),
    ];
    // 满册：另一张台开满 64 张，第 65 张。
    let (tx2, _rx2) = mpsc::channel(8);
    let full = Desk::new(tx2, crate::platform::fs::Face::issue());
    for i in 0..MAX_TICKETS_PER_CONNECTION {
        let a = serde_json::json!({"dial": dial(), "remote_path": "/x", "local_path": tmp.path(&format!("f{i}")).to_string_lossy()});
        assert!(reply_of(&full.ask1(TRANSFER_DOWNLOAD, "f", &a)).0);
    }
    got.push(pinned(
        &full.ask1(
            TRANSFER_DOWNLOAD,
            "d5",
            &serde_json::json!({"dial": dial(), "remote_path": "/x", "local_path": tmp.path("over").to_string_lossy()}),
        ),
        &tmp,
    ));
    let want = [
        r#"{"kind":"reply","id":"u1","ok":true,"data":{"items":[{"id":"xfer-<n>","key":"0","staged":"<key>","state":"opened"}]}}"#,
        r#"{"kind":"reply","id":"u2","ok":true,"data":{"items":[{"id":"xfer-<n>","key":"0","staged":"<key>","state":"opened"}]}}"#,
        r#"{"kind":"reply","id":"u3","ok":true,"data":{"items":[{"code":"busy","detail":"同一文件正在上传到该机 · 传完再来","key":"0","reason":"被占用","state":"failed"}]}}"#,
        r#"{"kind":"reply","id":"u4","ok":false,"code":"bad_args","message":"请求格式不对","detail":"时刻：<at>\n<machine>\n命令：transfer-upload\n码：bad_args\n原话：missing field `dial`"}"#,
        r#"{"kind":"reply","id":"u5","ok":true,"data":{"items":[{"code":"refused","detail":"<tmp> 不是普通文件 · 不可传","key":"0","reason":"被拒","state":"failed"}]}}"#,
        r#"{"kind":"reply","id":"u6","ok":true,"data":{"items":[{"code":"io_failed","detail":"读取失败 · 本机 <tmp>/gone.bin · 路径不存在\nNo such file or directory (os error 2)","key":"0","reason":"路径不存在","state":"failed"}]}}"#,
        r#"{"kind":"reply","id":"u7","ok":false,"code":"bad_args","message":"请求格式不对","detail":"时刻：<at>\n<machine>\n命令：transfer-upload\n码：bad_args\n原话：unknown field `via`, expected one of `dial`, `items`, `remote_dir`, `home`, `on_exists`, `decide`"}"#,
        r#"{"kind":"reply","id":"d1","ok":true,"data":{"items":[{"id":"xfer-<n>","key":"0","path":"<tmp>/dl.bin","state":"opened"}]}}"#,
        r#"{"kind":"reply","id":"d2","ok":true,"data":{"items":[{"id":"xfer-<n>","key":"0","path":"<tmp>/dÿ.bin","state":"opened"}]}}"#,
        r#"{"kind":"reply","id":"d3","ok":true,"data":{"items":[{"code":"refused","detail":"解析 <tmp>/nope 失败 · 路径不存在","key":"0","reason":"被拒","state":"failed"}]}}"#,
        r#"{"kind":"reply","id":"d4","ok":false,"code":"bad_args","message":"请求格式不对","detail":"时刻：<at>\n<machine>\n命令：transfer-download\n码：bad_args\n原话：missing field `remote_path`"}"#,
        r#"{"kind":"reply","id":"d5","ok":true,"data":{"items":[{"code":"too_many_transfers","detail":"传输已达上限 64","key":"0","reason":"超出上限","state":"failed"}]}}"#,
    ];
    assert_eq!(got, want, "开单两条回的字节变了");
}

#[cfg(unix)]
fn hex_of(p: &Path) -> String {
    use std::os::unix::ffi::OsStrExt;
    p.as_os_str()
        .as_bytes()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// 下载一组落到本机一个目录：本机落点有同名而没答 ⇒ 一单不开、回 `exists` 带同名清单；保留两份 ⇒ 另起名、撞上往后试。
#[tokio::test]
async fn a_download_group_asks_once_about_local_names_and_keeps_both_on_request() {
    let (tx, _rx) = mpsc::channel(64);
    let desk = Desk::new(tx, crate::platform::fs::Face::issue());
    let tmp = Tmp::dir("dl-group");
    tmp.file("a.txt", b"old");
    tmp.file("a (2).txt", b"old2");
    let dir = tmp.0.to_string_lossy().into_owned();
    let ask = |on: &str| {
        serde_json::json!({"dial": dial(), "local_dir": dir, "on_exists": on,
            "items": [{"remote_path": "/r/a.txt"}, {"remote_path": "/r/b.txt"}]})
    };
    let f = desk.answer_wire(TRANSFER_DOWNLOAD, "g1", &ask("ask"), &crate::Tz::default());
    let Frame::Reply(crate::wire::Reply { ok, code, .. }) = &f else {
        panic!("{f:?}")
    };
    assert!(!ok && code.as_deref() == Some("exists"), "{f:?}");
    assert_eq!(desk.len(), 0, "有同名而没答 ⇒ 一单都不开");
    let f = desk.answer_wire(
        TRANSFER_DOWNLOAD,
        "g2",
        &ask("keep_both"),
        &crate::Tz::default(),
    );
    let Frame::Reply(crate::wire::Reply { data: Some(d), .. }) = &f else {
        panic!("{f:?}")
    };
    assert_eq!(d["items"][0]["state"], "opened");
    assert_eq!(d["items"][0]["new_name"], "a (3).txt");
    assert_eq!(d["items"][1]["state"], "opened");
    assert_eq!(desk.len(), 2);
}

/// 上传一组：远端落点有同名而没答 ⇒ 一单不开、回 `exists` 带同名清单（两头的大小 · 时刻都在，时刻按看的那一台的钟写好）；
/// 答了 ⇒ 跳过的不开单 · 覆盖的提交时盖 · 保留两份的在远端另起名（撞上往后试）；不同名的照开、提交时不盖。
#[tokio::test]
async fn an_upload_group_asks_once_about_remote_names_and_settles_each_answer() {
    let (tx, _rx) = mpsc::channel(64);
    let desk = Desk::new(tx, crate::platform::fs::Face::issue());
    let here = Tmp::dir("up-here");
    let there = Tmp::dir("up-there");
    let a = here.file("a.txt", b"new!");
    let b = here.file("b.txt", b"b");
    there.file("a.txt", b"old");
    there.file("a (2).txt", b"old2");
    let old = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000);
    std::fs::File::options()
        .write(true)
        .open(there.path("a.txt"))
        .unwrap()
        .set_modified(old)
        .unwrap();
    let tz = crate::Tz::named("Asia/Tokyo").unwrap();
    let dir = there.0.to_string_lossy().into_owned();
    let ask = |decide: serde_json::Value| {
        let mut v = serde_json::json!({"dial": dial(), "remote_dir": dir,
            "items": [{"local_path": a}, {"local_path": b}]});
        if !decide.is_null() {
            v["decide"] = decide;
        }
        v
    };
    let f = desk.upload_frame("u1", &ask(serde_json::Value::Null), &Here, &tz);
    let Frame::Reply(crate::wire::Reply {
        ok, code, detail, ..
    }) = &f
    else {
        panic!("{f:?}")
    };
    assert!(!ok && code.as_deref() == Some("exists"), "{f:?}");
    assert_eq!(desk.len(), 0, "有同名而没答 ⇒ 一单都不开");
    let Err(UploadErr::Exists(_, c)) = ready(upload_with(
        &desk.tickets,
        &Here,
        serde_json::from_value(ask(serde_json::Value::Null)).unwrap(),
        &tz,
    )) else {
        panic!("同名清单不在：{detail:?}")
    };
    let mine = std::fs::metadata(&a).unwrap();
    assert_eq!(
        c.conflicts,
        [Conflict {
            key: "0".into(),
            name: cwf::WirePath::from("a.txt"),
            kind: cwf::FileKind::File,
            size: Some(4),
            size_text: Some(copy_core::size_text(4)),
            mtime_text: crate::common::batch_book::moment_text(
                crate::common::batch_book::mtime_secs(mine.modified()),
                &tz
            ),
            there: cwf::FileKind::File,
            there_size: Some(3),
            there_size_text: Some(copy_core::size_text(3)),
            there_mtime_text: Some("2023-11-15 07:13".into()),
            dir_into: false,
        }],
        "只有 a.txt 同名；那台那一格按东京的钟写"
    );
    let item = |f: &Frame, i: usize| match f {
        Frame::Reply(crate::wire::Reply { data: Some(d), .. }) => d["items"][i].clone(),
        other => panic!("{other:?}"),
    };
    let f = desk.upload_frame("u2", &ask(serde_json::json!({"0": "skip"})), &Here, &tz);
    assert_eq!(item(&f, 0)["state"], "skipped");
    assert_eq!(item(&f, 1)["state"], "opened");
    assert_eq!(
        item(&f, 1).get("overwrite"),
        None,
        "不同名的那一项提交时不盖"
    );
    assert_eq!(desk.len(), 1, "跳过的不开单");
    let (tx2, _rx2) = mpsc::channel(64);
    let fresh = Desk::new(tx2, crate::platform::fs::Face::issue());
    let f = fresh.upload_frame(
        "u3",
        &ask(serde_json::json!({"0": "overwrite"})),
        &Here,
        &tz,
    );
    assert_eq!(item(&f, 0)["state"], "opened");
    assert_eq!(item(&f, 0)["overwrite"], true, "答了覆盖 ⇒ 提交时盖");
    assert_eq!(item(&f, 0).get("new_name"), None);
    let (tx3, _rx3) = mpsc::channel(64);
    let fresh = Desk::new(tx3, crate::platform::fs::Face::issue());
    let f = fresh.upload_frame(
        "u4",
        &ask(serde_json::json!({"0": "keep_both"})),
        &Here,
        &tz,
    );
    assert_eq!(item(&f, 0)["state"], "opened");
    assert_eq!(item(&f, 0)["new_name"], "a (3).txt", "撞上 (2) 往后试");
    assert_eq!(item(&f, 0).get("overwrite"), None);
}

/// 远端落点那一格是目录：不合并 ⇒ 答「覆盖」也跳过，清单里写明 `dir_into`。
#[tokio::test]
async fn an_upload_onto_a_remote_dir_never_merges() {
    let (tx, _rx) = mpsc::channel(64);
    let desk = Desk::new(tx, crate::platform::fs::Face::issue());
    let here = Tmp::dir("up-dir-here");
    let there = Tmp::dir("up-dir-there");
    let a = here.file("a", b"x");
    std::fs::create_dir_all(there.path("a")).unwrap();
    let req = |decide: Option<serde_json::Value>| {
        let mut v = serde_json::json!({"dial": dial(), "remote_dir": there.0.to_string_lossy(),
            "items": [{"local_path": a}]});
        if let Some(d) = decide {
            v["decide"] = d;
        }
        serde_json::from_value::<TransferUploadReq>(v).unwrap()
    };
    let Err(UploadErr::Exists(_, c)) = ready(upload_with(
        &desk.tickets,
        &Here,
        req(None),
        &crate::Tz::default(),
    )) else {
        panic!("目录同名也要问")
    };
    assert!(c.conflicts[0].dir_into);
    let got = ready(upload_with(
        &desk.tickets,
        &Here,
        req(Some(serde_json::json!({"0": "overwrite"}))),
        &crate::Tz::default(),
    ))
    .unwrap();
    assert_eq!(got.items[0].state, OpenState::Skipped);
    assert_eq!(desk.len(), 0);
}
