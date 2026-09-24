//! 〔F7c · 第三波 · 2026-09-24〕**暂存区那一族**的判据（`设计/60 §13`）。
//!
//! # 台架：一台**逐条记改动路径**的合成 SFTP 服务端
//!
//! F4 那台（`sftp_pool_f4_tests`）记的是读写**偏移**，不记路径、不认建目录与列目录。
//! 本族要判的恰好是**路径**（「暂存区之外零写」）⇒ 另起一台：内存里的文件表 ＋ 目录表，
//! 每一次**会改东西**的请求（带写意图的 `OPEN` · `WRITE` · `REMOVE` · `MKDIR` · `RENAME` · `SETSTAT`）
//! 都按到达顺序记下它碰的路径。**判据自己看这张表**，不信被测那一侧的自述。
//!
//! # 买到什么
//!
//! - 🔴 **暂存区之外零写**（`设计/60 §13.6` 判据 2）：成功 · 撤 · 失败 · 续传四趟跑下来，
//!   记下的每一个改动路径都在 `.cc-monitor/staging` 底下；**正控**：同一台服务端上
//!   一次暂存区外的写被这张表认出来（表不瞎）。
//! - 撤 ⇒ 暂存件没了；失败 ⇒ 暂存件还在，下一趟从尾块接上（服务端记下的第一个写偏移 == 续传起点）。
//! - （孤儿扫不在这一侧：它住后端 `files_commit::sweep_stale`，判据在后端那棵树里。）
//! - `~/.cc-monitor` 不在 ⇒ 报错且**一次改动都没有**（不顺手建后端的家）。
//! - 两个 crate 的常量逐字相同（暂存区那一段 · 键长）。
//!
//! # 买不到什么
//!
//! - 真 sshd 上的一趟（本仓红线不许起真连接）；真远端的起始目录是不是 home（`ChrootDirectory` 那一形）。

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use russh_sftp::protocol::{
    Attrs, Data, File, FileAttributes, Handle, Name, OpenFlags, Status, StatusCode, Version,
};

use super::{
    staging_key, staging_part, upload_to_staging, STAGING_DIR, STAGING_KEY_LEN, STAGING_PARENT,
};

// ═══ ① 合成服务端 ═══════════════════════════════════════════════════════════

/// 一份文件：字节 ＋ 修改时间（孤儿扫要它）。
#[derive(Clone, Debug, Default)]
struct Entry {
    bytes: Vec<u8>,
    mtime: u32,
}

#[derive(Default, Debug)]
pub(super) struct Fs {
    files: BTreeMap<String, Entry>,
    dirs: BTreeSet<String>,
    handles: HashMap<String, String>,
    next_handle: u32,
    /// 列目录句柄 → 还没交出去的那一批（一次交完，第二次 `EOF`）。
    dir_handles: HashMap<String, Option<String>>,
    /// 🔴 **本台架的针**：每一次会改东西的请求碰的路径，按到达顺序。
    pub(super) mutated: Vec<(String, String)>,
    /// 第 N 次（0 起）`WRITE` 之后开始回 `FAILURE`（「失败留」那一格要一个中途真坏的形状）。
    fail_write_after: Option<usize>,
    write_calls: usize,
    /// 每一次 `WRITE` 的偏移（续传那一格判「从哪儿接上的」）。
    write_offsets: Vec<u64>,
    /// 新建 / 写过的文件打上的修改时间（判据自己定，不看墙钟）。
    now: u32,
}

impl Fs {
    fn alloc(&mut self) -> String {
        let id = self.next_handle;
        self.next_handle += 1;
        String::from_utf8(id.to_be_bytes().to_vec()).expect("序号 < 0x80")
    }
    fn touch(&mut self, verb: &str, path: &str) {
        self.mutated.push((verb.to_string(), path.to_string()));
    }
}

struct Server {
    fs: Arc<Mutex<Fs>>,
}

fn ok(id: u32) -> Status {
    Status {
        id,
        status_code: StatusCode::Ok,
        error_message: "Success".to_string(),
        language_tag: "en-US".to_string(),
    }
}

fn attrs_of(e: &Entry) -> FileAttributes {
    FileAttributes {
        size: Some(e.bytes.len() as u64),
        mtime: Some(e.mtime),
        ..Default::default()
    }
}

impl russh_sftp::server::Handler for Server {
    type Error = StatusCode;

    fn unimplemented(&self) -> Self::Error {
        StatusCode::OpUnsupported
    }

    fn init(
        &mut self,
        _version: u32,
        _extensions: HashMap<String, String>,
    ) -> impl std::future::Future<Output = Result<Version, Self::Error>> + Send {
        async move {
            Ok(Version {
                version: 3,
                extensions: HashMap::new(),
            })
        }
    }

    fn open(
        &mut self,
        id: u32,
        filename: String,
        pflags: OpenFlags,
        _attrs: FileAttributes,
    ) -> impl std::future::Future<Output = Result<Handle, Self::Error>> + Send {
        let fs = self.fs.clone();
        async move {
            let mut fs = fs.lock().unwrap();
            let writes =
                pflags.intersects(OpenFlags::CREATE | OpenFlags::WRITE | OpenFlags::TRUNCATE);
            if writes {
                fs.touch("open-w", &filename);
            }
            if pflags.contains(OpenFlags::CREATE) {
                let parent = filename.rsplit_once('/').map(|(p, _)| p.to_string());
                if parent.is_some_and(|p| !fs.dirs.contains(&p)) {
                    return Err(StatusCode::NoSuchFile);
                }
                let now = fs.now;
                if pflags.contains(OpenFlags::TRUNCATE) || !fs.files.contains_key(&filename) {
                    fs.files.insert(
                        filename.clone(),
                        Entry {
                            bytes: Vec::new(),
                            mtime: now,
                        },
                    );
                }
            } else if !fs.files.contains_key(&filename) {
                return Err(StatusCode::NoSuchFile);
            }
            let h = fs.alloc();
            fs.handles.insert(h.clone(), filename);
            Ok(Handle { id, handle: h })
        }
    }

    fn close(
        &mut self,
        id: u32,
        handle: String,
    ) -> impl std::future::Future<Output = Result<Status, Self::Error>> + Send {
        let fs = self.fs.clone();
        async move {
            let mut fs = fs.lock().unwrap();
            fs.handles.remove(&handle);
            fs.dir_handles.remove(&handle);
            Ok(ok(id))
        }
    }

    fn read(
        &mut self,
        id: u32,
        handle: String,
        offset: u64,
        len: u32,
    ) -> impl std::future::Future<Output = Result<Data, Self::Error>> + Send {
        let fs = self.fs.clone();
        async move {
            let fs = fs.lock().unwrap();
            let path = fs.handles.get(&handle).ok_or(StatusCode::Failure)?;
            let e = fs.files.get(path).ok_or(StatusCode::NoSuchFile)?;
            let start = offset as usize;
            if start >= e.bytes.len() {
                return Err(StatusCode::Eof);
            }
            let end = (start + len as usize).min(e.bytes.len());
            Ok(Data {
                id,
                data: e.bytes[start..end].to_vec(),
            })
        }
    }

    fn write(
        &mut self,
        id: u32,
        handle: String,
        offset: u64,
        data: Vec<u8>,
    ) -> impl std::future::Future<Output = Result<Status, Self::Error>> + Send {
        let fs = self.fs.clone();
        async move {
            let mut fs = fs.lock().unwrap();
            let path = fs.handles.get(&handle).ok_or(StatusCode::Failure)?.clone();
            fs.touch("write", &path);
            fs.write_calls += 1;
            if fs.fail_write_after.is_some_and(|n| fs.write_calls > n) {
                return Err(StatusCode::Failure);
            }
            fs.write_offsets.push(offset);
            let now = fs.now;
            let e = fs.files.get_mut(&path).ok_or(StatusCode::NoSuchFile)?;
            let at = offset as usize;
            if e.bytes.len() < at + data.len() {
                e.bytes.resize(at + data.len(), 0);
            }
            e.bytes[at..at + data.len()].copy_from_slice(&data);
            e.mtime = now;
            Ok(ok(id))
        }
    }

    fn stat(
        &mut self,
        id: u32,
        path: String,
    ) -> impl std::future::Future<Output = Result<Attrs, Self::Error>> + Send {
        let fs = self.fs.clone();
        async move {
            let fs = fs.lock().unwrap();
            if fs.dirs.contains(&path) {
                return Ok(Attrs {
                    id,
                    attrs: FileAttributes {
                        permissions: Some(0o040_755),
                        ..Default::default()
                    },
                });
            }
            let e = fs.files.get(&path).ok_or(StatusCode::NoSuchFile)?;
            Ok(Attrs {
                id,
                attrs: attrs_of(e),
            })
        }
    }

    fn lstat(
        &mut self,
        id: u32,
        path: String,
    ) -> impl std::future::Future<Output = Result<Attrs, Self::Error>> + Send {
        self.stat(id, path)
    }

    fn fstat(
        &mut self,
        id: u32,
        handle: String,
    ) -> impl std::future::Future<Output = Result<Attrs, Self::Error>> + Send {
        let fs = self.fs.clone();
        async move {
            let fs = fs.lock().unwrap();
            let path = fs.handles.get(&handle).ok_or(StatusCode::Failure)?;
            let e = fs.files.get(path).ok_or(StatusCode::NoSuchFile)?;
            Ok(Attrs {
                id,
                attrs: attrs_of(e),
            })
        }
    }

    fn remove(
        &mut self,
        id: u32,
        filename: String,
    ) -> impl std::future::Future<Output = Result<Status, Self::Error>> + Send {
        let fs = self.fs.clone();
        async move {
            let mut fs = fs.lock().unwrap();
            fs.touch("remove", &filename);
            match fs.files.remove(&filename) {
                Some(_) => Ok(ok(id)),
                None => Err(StatusCode::NoSuchFile),
            }
        }
    }

    fn mkdir(
        &mut self,
        id: u32,
        path: String,
        _attrs: FileAttributes,
    ) -> impl std::future::Future<Output = Result<Status, Self::Error>> + Send {
        let fs = self.fs.clone();
        async move {
            let mut fs = fs.lock().unwrap();
            fs.touch("mkdir", &path);
            let parent_ok = path
                .rsplit_once('/')
                .is_none_or(|(p, _)| fs.dirs.contains(p));
            if !parent_ok || fs.dirs.contains(&path) {
                return Err(StatusCode::Failure);
            }
            fs.dirs.insert(path);
            Ok(ok(id))
        }
    }

    fn rename(
        &mut self,
        id: u32,
        oldpath: String,
        newpath: String,
    ) -> impl std::future::Future<Output = Result<Status, Self::Error>> + Send {
        let fs = self.fs.clone();
        async move {
            let mut fs = fs.lock().unwrap();
            fs.touch("rename-from", &oldpath);
            fs.touch("rename-to", &newpath);
            let e = fs.files.remove(&oldpath).ok_or(StatusCode::NoSuchFile)?;
            fs.files.insert(newpath, e);
            Ok(ok(id))
        }
    }

    fn setstat(
        &mut self,
        id: u32,
        path: String,
        _attrs: FileAttributes,
    ) -> impl std::future::Future<Output = Result<Status, Self::Error>> + Send {
        let fs = self.fs.clone();
        async move {
            fs.lock().unwrap().touch("setstat", &path);
            Ok(ok(id))
        }
    }

    fn opendir(
        &mut self,
        id: u32,
        path: String,
    ) -> impl std::future::Future<Output = Result<Handle, Self::Error>> + Send {
        let fs = self.fs.clone();
        async move {
            let mut fs = fs.lock().unwrap();
            if !fs.dirs.contains(&path) {
                return Err(StatusCode::NoSuchFile);
            }
            let h = fs.alloc();
            fs.dir_handles.insert(h.clone(), Some(path));
            Ok(Handle { id, handle: h })
        }
    }

    fn readdir(
        &mut self,
        id: u32,
        handle: String,
    ) -> impl std::future::Future<Output = Result<Name, Self::Error>> + Send {
        let fs = self.fs.clone();
        async move {
            let mut fs = fs.lock().unwrap();
            let Some(slot) = fs.dir_handles.get_mut(&handle) else {
                return Err(StatusCode::Failure);
            };
            let Some(dir) = slot.take() else {
                return Err(StatusCode::Eof);
            };
            let prefix = format!("{dir}/");
            let files: Vec<File> = fs
                .files
                .iter()
                .filter_map(|(p, e)| {
                    let rest = p.strip_prefix(&prefix)?;
                    (!rest.contains('/')).then(|| File::new(rest, attrs_of(e)))
                })
                .collect();
            Ok(Name { id, files })
        }
    }
}

// ═══ ② 台架 ═════════════════════════════════════════════════════════════════

/// 起一台：`~/.cc-monitor` 在（后端的家），暂存区还不在。
pub(super) fn home_with_backend() -> Arc<Mutex<Fs>> {
    // 新建 / 写过的文件打「此刻」的时间：否则孤儿扫会把这一趟之前刚建的件判成老的。
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as u32)
        .unwrap_or(0);
    let mut fs = Fs {
        now,
        ..Default::default()
    };
    fs.dirs.insert(STAGING_PARENT.to_string());
    Arc::new(Mutex::new(fs))
}

pub(super) async fn session_on(fs: Arc<Mutex<Fs>>) -> russh_sftp::client::SftpSession {
    let (client_io, server_io) = tokio::io::duplex(256 * 1024);
    russh_sftp::server::run(server_io, Server { fs }).await;
    russh_sftp::client::SftpSession::new(client_io)
        .await
        .expect("合成服务端上的 sftp 会话应当开得起来")
}

/// 合成语料：编译期拼、含不可打印字节、长过 `CHUNK` 好几倍（一个真会话正文的字节都没有）。
pub(super) fn corpus(len: usize) -> Vec<u8> {
    let unit: [u8; 16] = *b"F7c-stage\x00\x01\x02\x03\x04\x05\x06";
    unit.iter().copied().cycle().take(len).collect()
}

/// 本机一份夹具文件（落 `temp_dir`，跑完就删）。
pub(super) struct Local(pub(super) PathBuf);
impl Local {
    pub(super) fn new(tag: &str, bytes: &[u8]) -> Self {
        static N: AtomicUsize = AtomicUsize::new(0);
        let p = std::env::temp_dir().join(format!(
            "ccm-f7c-{tag}-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::SeqCst)
        ));
        std::fs::write(&p, bytes).expect("写夹具文件");
        Self(p)
    }
    pub(super) fn path(&self) -> String {
        self.0.to_string_lossy().into_owned()
    }
}
impl Drop for Local {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

const KEY: &str = "00112233445566778899aabbccddeeff";

/// 记下来的改动里，**不在暂存区底下**的那几条。
fn outside_staging(fs: &Arc<Mutex<Fs>>) -> Vec<(String, String)> {
    let prefix = format!("{STAGING_DIR}/");
    fs.lock()
        .unwrap()
        .mutated
        .iter()
        .filter(|(_, p)| p != STAGING_DIR && !p.starts_with(&prefix))
        .cloned()
        .collect()
}

fn no_progress(_: u64, _: u64) {}

// ═══ ③ 判据 ═════════════════════════════════════════════════════════════════

/// 🔴🔴 **暂存区之外零写**（`设计/60 §13.6` 判据 2）—— 成功 · 失败 · 续传 · 撤四趟，
/// 服务端记下的每一个改动路径都在暂存区底下。**正控**：同一台服务端上一次暂存区外的写被认出来。
#[tokio::test]
async fn a_staging_upload_writes_nothing_outside_the_staging_area() {
    let fs = home_with_backend();
    let sftp = session_on(fs.clone()).await;
    let body = corpus(100 * 1024);
    let local = Local::new("zero", &body);

    // ① 成功
    upload_to_staging(
        &sftp,
        &local.path(),
        KEY,
        &AtomicBool::new(false),
        &no_progress,
    )
    .await
    .expect("第一趟该传完");
    // ② 失败（写到第 2 块坏掉）
    let key2 = "ffeeddccbbaa99887766554433221100";
    let so_far = fs.lock().unwrap().write_calls;
    fs.lock().unwrap().fail_write_after = Some(so_far + 2);
    upload_to_staging(
        &sftp,
        &local.path(),
        key2,
        &AtomicBool::new(false),
        &no_progress,
    )
    .await
    .expect_err("中途坏掉该报错");
    // ③ 续传
    fs.lock().unwrap().fail_write_after = None;
    upload_to_staging(
        &sftp,
        &local.path(),
        key2,
        &AtomicBool::new(false),
        &no_progress,
    )
    .await
    .expect("续传该传完");
    // ④ 撤（一开跑就撤）
    let key3 = "0123456789abcdef0123456789abcdef";
    upload_to_staging(
        &sftp,
        &local.path(),
        key3,
        &AtomicBool::new(true),
        &no_progress,
    )
    .await
    .expect_err("撤了该回错");

    let n = fs.lock().unwrap().mutated.len();
    assert!(
        n >= 8,
        "四趟只记到 {n} 次改动 —— 台架的针没接上，下面那条零命中是空真"
    );
    assert_eq!(
        outside_staging(&fs),
        Vec::<(String, String)>::new(),
        "传输层写到了暂存区之外（`设计/60 §13`：SFTP 只许写我们自己的暂存区）"
    );

    // 正控：一次暂存区外的写，这张表认得出来（表不瞎）。
    let mut f = sftp.create("elsewhere.bin").await;
    if let Ok(f) = f.as_mut() {
        use tokio::io::AsyncWriteExt;
        let _ = f.write_all(b"x").await;
        let _ = f.shutdown().await;
    }
    let seen = outside_staging(&fs);
    assert!(
        seen.iter().any(|(_, p)| p == "elsewhere.bin"),
        "正控没认出暂存区外的那一次写：{seen:?}"
    );
}

/// ★ 字节落在暂存件上、逐字节相同；进度的最后一格 == 总长。
#[tokio::test]
async fn the_bytes_land_at_the_staging_part_verbatim() {
    let fs = home_with_backend();
    let sftp = session_on(fs.clone()).await;
    let body = corpus(70 * 1024 + 3);
    let local = Local::new("land", &body);
    let seen: Mutex<Vec<(u64, u64)>> = Mutex::new(Vec::new());
    let sink = |g: u64, t: u64| seen.lock().unwrap().push((g, t));
    let n = upload_to_staging(&sftp, &local.path(), KEY, &AtomicBool::new(false), &sink)
        .await
        .expect("该传完");
    assert_eq!(n, body.len() as u64);
    let got = fs.lock().unwrap().files.get(&staging_part(KEY)).cloned();
    assert_eq!(got.map(|e| e.bytes), Some(body.clone()));
    let last = *seen.lock().unwrap().last().expect("至少报一次进度");
    assert_eq!(last, (body.len() as u64, body.len() as u64));
}

/// ★ **失败留、下一趟从尾块接上**：服务端记下的第二趟第一个写偏移 == 第一趟落盘的长度。
#[tokio::test]
async fn a_failed_upload_keeps_the_part_and_the_retry_resumes_from_its_tail() {
    let fs = home_with_backend();
    let sftp = session_on(fs.clone()).await;
    let body = corpus(160 * 1024);
    let local = Local::new("resume", &body);
    fs.lock().unwrap().fail_write_after = Some(2);
    upload_to_staging(
        &sftp,
        &local.path(),
        KEY,
        &AtomicBool::new(false),
        &no_progress,
    )
    .await
    .expect_err("第 3 块坏掉");
    let have = fs
        .lock()
        .unwrap()
        .files
        .get(&staging_part(KEY))
        .map(|e| e.bytes.len())
        .expect("🔴 失败之后暂存件没了 —— 续传的本钱被删了");
    assert!(have > 0 && have < body.len(), "半截长度不对：{have}");
    let mark = fs.lock().unwrap().write_offsets.len();
    fs.lock().unwrap().fail_write_after = None;
    upload_to_staging(
        &sftp,
        &local.path(),
        KEY,
        &AtomicBool::new(false),
        &no_progress,
    )
    .await
    .expect("续传该传完");
    let first_retry_write = fs.lock().unwrap().write_offsets[mark];
    assert_eq!(
        first_retry_write, have as u64,
        "续传没从尾块接上（从 {first_retry_write} 写起，半截是 {have}）"
    );
    let got = fs.lock().unwrap().files.get(&staging_part(KEY)).cloned();
    assert_eq!(got.map(|e| e.bytes), Some(body));
}

/// ★ **撤 ⇒ 暂存件没了**（用户说了不要；这是暂存区清理「撤」那一格的事件）。
#[tokio::test]
async fn a_cancelled_upload_removes_its_part() {
    let fs = home_with_backend();
    let sftp = session_on(fs.clone()).await;
    let local = Local::new("cancel", &corpus(64 * 1024));
    upload_to_staging(
        &sftp,
        &local.path(),
        KEY,
        &AtomicBool::new(true),
        &no_progress,
    )
    .await
    .expect_err("撤了该回错");
    assert!(
        !fs.lock().unwrap().files.contains_key(&staging_part(KEY)),
        "撤了，暂存件还在"
    );
    // 阴性对照：它**曾经**被建过（不是「根本没开跑所以没有」）。
    assert!(fs
        .lock()
        .unwrap()
        .mutated
        .iter()
        .any(|(v, p)| v == "open-w" && *p == staging_part(KEY)));
}

/// ★ `~/.cc-monitor` 不在 ⇒ 报错，而且**一次改动都没有**（不顺手建后端的家）。
#[tokio::test]
async fn without_the_backend_home_nothing_is_written_and_it_says_so() {
    let fs = Arc::new(Mutex::new(Fs::default()));
    let sftp = session_on(fs.clone()).await;
    let local = Local::new("nohome", b"x");
    let e = upload_to_staging(
        &sftp,
        &local.path(),
        KEY,
        &AtomicBool::new(false),
        &no_progress,
    )
    .await
    .expect_err("没有后端的家该报错");
    assert!(e.contains(STAGING_PARENT), "报错没说缺的是哪儿：{e}");
    assert!(
        fs.lock().unwrap().mutated.is_empty(),
        "没有后端的家，却改了东西：{:?}",
        fs.lock().unwrap().mutated
    );
}

/// ★ 键：确定 · 32 位小写十六进制 · 路径 / 大小 / 修改时间任一变了键就变。
#[test]
fn the_staging_key_is_deterministic_and_shaped_like_the_backend_wants() {
    let k = staging_key("/home/u/a.bin", 10, 7);
    assert_eq!(
        k,
        staging_key("/home/u/a.bin", 10, 7),
        "同一份文件两次键不同 ⇒ 续传永远接不上"
    );
    assert_eq!(k.len(), STAGING_KEY_LEN);
    assert!(
        k.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
        "{k}"
    );
    for other in [
        staging_key("/home/u/b.bin", 10, 7),
        staging_key("/home/u/a.bin", 11, 7),
        staging_key("/home/u/a.bin", 10, 8),
    ] {
        assert_ne!(k, other, "三个输入里有一个没进键");
    }
}

/// 🔴 两个 crate 的那两个常量**逐字相同**（`设计/60 §11.4` 同一条理由：没有共享落点 ⇒ 两份副本 ＋ 相等断言）。
///
/// 现读后端那一份源码（运行期读，不是编译期 `include_str!` —— 不长一条跨半边的编译期边）。
#[test]
fn the_backend_builds_the_same_staging_path_from_the_same_key_length() {
    let src = std::fs::read_to_string(
        crate::guard_support::repo_root().join("src/backend/control/files_commit.rs"),
    )
    .expect("读后端那一份 —— 读不到就是搬走了，同轮改这里");
    // 针**运行时拼**，别让本文件自己成为第三处住址。
    let dir_line = format!("pub const {}: &str = {:?};", "STAGING_DIR", STAGING_DIR);
    let len_line = format!("pub const {}: usize = {};", "KEY_LEN", STAGING_KEY_LEN);
    for want in [&dir_line, &len_line] {
        assert_eq!(
            src.matches(want.as_str()).count(),
            1,
            "后端那一份里找不到逐字相同的 `{want}` —— 两侧拼出来的暂存件不是同一份，提交会答「暂存件不在」"
        );
    }
}
