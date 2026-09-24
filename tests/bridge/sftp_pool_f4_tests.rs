//! **秤 F4**：`设计/60 §2 档③` 那一排里做成的三件，逐件钉一个**相等读数**。
//!
//! # 它接的是哪三句话
//!
//! `设计/60 §2 档③` 那张表（每一行都写着「为什么缺」，而每一条的主语都是**我们**）：
//!
//! | 能力 | 今天 | 为什么缺 |
//! |---|---|---|
//! | 边传边浏览 | ❌ | **我们**单连接 ＋ 锁串行。SFTP 本身允许多个未完成请求 |
//! | 并发 / 队列传输 | ❌ | 同上 |
//! | 断点续传 | ❌ | 没做（`READ`/`WRITE` 带 offset，协议支持） |
//!
//! 硬前置写在 `设计/15 §3.2` 那张「池化的硬冲突」表第 3 条：**必须有 per-session
//! channel 上限（建议 ≤6）**，因为 OpenSSH `MaxSessions` 默认 10。
//!
//! # 形状照秤 F3 办（`tests/bridge/sftp_copy_f3_tests.rs`）
//!
//! 同一套构件、同一种取法：**进程内一台讲 SFTP v3 裸字节的合成服务端**
//! （`russh_sftp::server::run` 跨一对 `tokio::io::duplex`）· **编译期拼的合成语料**
//! （采结构不采内容）· **相等断言当主锚**。
//!
//! ⚠ **刻意没有共用 F3 那一份服务端**，理由写死在这里、不是图省事：
//! 本秤要量的东西 F3 那份 `FakeFs` 一样都不记 —— **每一次 `READ`/`WRITE` 的偏移**
//! （续传那三条判据的针就是它）· **按需让 `read` 失败**（「报错删半成品」那一条要它）。
//! 把这两样加进 F3，就是在改一个**刚落地、正被门禁盯着**的秤的被测面；
//! 而 F3 那杆秤两个方向今天照跑，本件一个字节都没碰它。
//!
//! # 秤盘落在**服务端**那一侧，不是被测代码自己汇报的数
//!
//! F3 把计数器夹在流中间数包类型，因为它要证明的是「**零**条读写包」。
//! 本秤要的是「**从哪个偏移**开始读/写」，而那个事实的最干净观测点就是**对端**：
//! `FakeFs` 逐条记下它**真的被要求**读/写的偏移。那不是 `download_inner` 自称的数
//! （`memory/judge-not-in-exec-chain-is-no-judge` 的另一半：**恒等两侧同源会恒真**）。
//!
//! # 它不守什么（逐条，别读大）
//!
//! - **它不判真远端。** 服务端是本进程里一个 `duplex` 的对端 ⇒ 真 sshd 的
//!   `MaxSessions` 到底肯给几条通道、多通道在真链路上的吞吐、`open_sftp_channel`
//!   那三个往返的真实代价 —— **一条都没答**。本秤买的是「我们这一侧的闸与偏移对不对」。
//! - **它不判 `OriginPool` 那一层。** `OriginPool` 攥着一条真 SSH 连接，本进程里
//!   构造不出来。被判的是它的**全部策略**所在的 [`ChannelSet`]（两道闸 · 复用 · 世代），
//!   以及两条传输核心 `download_inner` / `upload_inner`。
//!   `OriginPool` 剩下的部分是**接线**：把 `connect_sftp` 的结果喂给上面两者。
//!   ⚠ 那几行接线**没有判据**，如实登记在这里。
//! - **它不判 `MaxSessions` 这个数配得对不对** —— 6 是本地自律，远端真值量不到
//!   （理由逐字写在 `sftp_pool::SESSION_CHANNEL_CAP` 的头注里）。

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use russh_sftp::protocol::{Attrs, Data, FileAttributes, Handle, Status, StatusCode, Version};

use super::{download_inner, upload_inner, ChannelSet, SESSION_CHANNEL_CAP, TRANSFER_LANE_CAP};

// ═══ ① 合成服务端：一张内存表 ＋ **逐条记偏移** ════════════════════════════════

/// 服务端那一侧的「文件系统」。**内存里，不碰磁盘。**
#[derive(Default, Debug)]
struct FakeFs {
    files: HashMap<String, Vec<u8>>,
    /// `句柄 → 路径`。句柄按 OpenSSH 的做法编：4 字节大端的序号。
    handles: HashMap<String, String>,
    next_handle: u32,
    /// **本秤的针**：服务端真的被要求读的每一个偏移，按到达顺序。
    read_offsets: Vec<u64>,
    /// 同上，写那一侧。
    write_offsets: Vec<u64>,
    /// 第 N 次（0 起）`read` 之后开始回 `SSH_FX_FAILURE`。
    /// 「报错删半成品 / 取消留半成品」那一对判据要一个**中途真的失败**的形状。
    fail_read_after: Option<usize>,
    read_calls: usize,
}

impl FakeFs {
    fn alloc_handle(&mut self, path: &str) -> String {
        let id = self.next_handle;
        self.next_handle += 1;
        // 序号 < 0x80 时 4 字节大端必是合法 UTF-8 —— 与真 sftp-server 在这一档同形。
        let h = String::from_utf8(id.to_be_bytes().to_vec()).expect("序号 < 0x80");
        self.handles.insert(h.clone(), path.to_string());
        h
    }
}

struct FakeSftpServer {
    fs: Arc<Mutex<FakeFs>>,
}

fn ok_status(id: u32) -> Status {
    Status {
        id,
        status_code: StatusCode::Ok,
        error_message: "Success".to_string(),
        language_tag: "en-US".to_string(),
    }
}

impl russh_sftp::server::Handler for FakeSftpServer {
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
        pflags: russh_sftp::protocol::OpenFlags,
        _attrs: FileAttributes,
    ) -> impl std::future::Future<Output = Result<Handle, Self::Error>> + Send {
        let fs = self.fs.clone();
        async move {
            let mut fs = fs.lock().unwrap();
            let creating = pflags.contains(russh_sftp::protocol::OpenFlags::CREATE);
            let truncating = pflags.contains(russh_sftp::protocol::OpenFlags::TRUNCATE);
            let exclusive = pflags.contains(russh_sftp::protocol::OpenFlags::EXCLUDE);
            if creating {
                if exclusive && fs.files.contains_key(&filename) {
                    return Err(StatusCode::Failure);
                }
                // ★ `TRUNCATE` 才清零。**续传那一路靠的就是不带它**
                //   —— 带着它开，上次那半截当场没了。
                if truncating || !fs.files.contains_key(&filename) {
                    fs.files.insert(filename.clone(), Vec::new());
                }
            } else if !fs.files.contains_key(&filename) {
                return Err(StatusCode::NoSuchFile);
            }
            let handle = fs.alloc_handle(&filename);
            Ok(Handle { id, handle })
        }
    }

    fn close(
        &mut self,
        id: u32,
        handle: String,
    ) -> impl std::future::Future<Output = Result<Status, Self::Error>> + Send {
        let fs = self.fs.clone();
        async move {
            fs.lock().unwrap().handles.remove(&handle);
            Ok(ok_status(id))
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
            let mut fs = fs.lock().unwrap();
            fs.read_offsets.push(offset);
            fs.read_calls += 1;
            if fs.fail_read_after.is_some_and(|n| fs.read_calls > n) {
                return Err(StatusCode::Failure); // 中途真的坏掉（不是 EOF）
            }
            let path = fs.handles.get(&handle).ok_or(StatusCode::Failure)?;
            let bytes = fs.files.get(path).ok_or(StatusCode::NoSuchFile)?;
            let start = offset as usize;
            if start >= bytes.len() {
                return Err(StatusCode::Eof);
            }
            let end = (start + len as usize).min(bytes.len());
            Ok(Data {
                id,
                data: bytes[start..end].to_vec(),
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
            fs.write_offsets.push(offset);
            let path = fs.handles.get(&handle).ok_or(StatusCode::Failure)?.clone();
            let bytes = fs.files.get_mut(&path).ok_or(StatusCode::NoSuchFile)?;
            let at = offset as usize;
            if bytes.len() < at + data.len() {
                bytes.resize(at + data.len(), 0);
            }
            bytes[at..at + data.len()].copy_from_slice(&data);
            Ok(ok_status(id))
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
            let bytes = fs.files.get(&path).ok_or(StatusCode::NoSuchFile)?;
            Ok(Attrs {
                id,
                attrs: FileAttributes {
                    size: Some(bytes.len() as u64),
                    ..Default::default()
                },
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
            let bytes = fs.files.get(path).ok_or(StatusCode::NoSuchFile)?;
            Ok(Attrs {
                id,
                attrs: FileAttributes {
                    size: Some(bytes.len() as u64),
                    ..Default::default()
                },
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
            let existed = fs.lock().unwrap().files.remove(&filename).is_some();
            if existed {
                Ok(ok_status(id))
            } else {
                Err(StatusCode::NoSuchFile)
            }
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
            // 与 `sftp.rs` 头注那句「标准 SFTP `rename` 不覆盖已存在目标」同形。
            if fs.files.contains_key(&newpath) {
                return Err(StatusCode::Failure);
            }
            let bytes = fs.files.remove(&oldpath).ok_or(StatusCode::NoSuchFile)?;
            fs.files.insert(newpath, bytes);
            Ok(ok_status(id))
        }
    }
}

// ═══ ② 台架 ═════════════════════════════════════════════════════════════════

/// 合成语料。**编译期拼**，一个真会话正文的字节都没有
/// （`memory/test-fixtures-no-real-transcript`：采结构不采内容）。
///
/// 要具备的结构性质有两条：
/// ① **长过 `CHUNK`（32 KiB）好几倍** —— 否则「续传只读缺的那截」与「整份重读」
///    在包数上分不开；
/// ② **每 16 字节一个循环节里含不可打印字节** —— 免得谁把它当成可读文本去理解。
fn synthetic_corpus() -> Vec<u8> {
    // 100 KiB = 32 KiB × 3 + 4 KiB。
    let unit: [u8; 16] = *b"F4-corpus-\x00\x01\x02\x03\x04\x05";
    unit.iter().copied().cycle().take(CORPUS_LEN).collect()
}

/// 语料长度与续传起点。**两个数在判据里要相减**，所以写成常量、不抄。
const CORPUS_LEN: usize = 100 * 1024;
/// 续传起点：刻意**不是** `CHUNK` 的整数倍，好让「按块对齐才行」那类实现露馅。
const RESUME_AT: usize = 40 * 1024;
/// 尾块对拍的长度 = `sftp_pool::CHUNK`（判据不另起一个数，那会变成第二份真相）。
const PROBE_LEN: usize = 32 * 1024;

/// 起一个台架：合成服务端 ＋ 一个已 `init` 的**高层**会话，两者共用同一份 `FakeFs`。
async fn session_on(fs: Arc<Mutex<FakeFs>>) -> russh_sftp::client::SftpSession {
    let (client_io, server_io) = tokio::io::duplex(256 * 1024);
    russh_sftp::server::run(server_io, FakeSftpServer { fs }).await;
    russh_sftp::client::SftpSession::new(client_io)
        .await
        .expect("合成服务端上的 sftp 会话应当开得起来")
}

fn new_fs() -> Arc<Mutex<FakeFs>> {
    Arc::new(Mutex::new(FakeFs::default()))
}

/// 夹具目录落 `std::env::temp_dir()`（本仓测试的既有写法），跑完就删。
struct TmpDir(PathBuf);
impl TmpDir {
    fn new(tag: &str) -> Self {
        static N: AtomicUsize = AtomicUsize::new(0);
        let d = std::env::temp_dir().join(format!(
            "ccm-f4-{tag}-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::SeqCst)
        ));
        std::fs::create_dir_all(&d).expect("建夹具目录");
        Self(d)
    }
    fn join(&self, name: &str) -> String {
        self.0.join(name).to_string_lossy().into_owned()
    }
}
impl Drop for TmpDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn never_cancelled() -> Arc<AtomicBool> {
    Arc::new(AtomicBool::new(false))
}

/// 一个**收得下进度**的回调：把每条「已传」攒进一个 `Vec`。
///
/// ★ 第一条进度报的就是**续传起点** —— 那是 `download_inner` 的头一个动作，
/// 也是本秤判「到底从哪儿接上的」的第二个针（第一个针在服务端的偏移表里）。
///
/// 〔F7c · 第三波 09-24〕两条传输核心的进度口从 `tauri::ipc::Channel` 换成了一个普通回调
/// （`Fn(已传, 总共)`）：传输台跑在 monitor 里、进度走通道的订阅流，不再有 webview 那一跳。
/// 老面板那两条 Tauri 命令在入口处把 channel 包成这个回调 —— 本秤喂的是核心，不经那一层。
fn progress_sink() -> (impl Fn(u64, u64) + Sync, Arc<Mutex<Vec<u64>>>) {
    let seen: Arc<Mutex<Vec<u64>>> = Arc::new(Mutex::new(Vec::new()));
    let sink = seen.clone();
    (
        move |transferred: u64, _total: u64| sink.lock().unwrap().push(transferred),
        seen,
    )
}

/// 自旋到 `cond` 成立为止（每轮让出执行权）。超过 `ROUNDS` 轮就当没成立，回 `false`。
///
/// ⚠ **不睡觉、不看墙钟**：本秤全部跑在 `current_thread` 运行时上，
/// 让出执行权就足以让别的 task 推进。拿 `sleep` 去等是把判据变成计时器，
/// 在慢机器上会变成随机红。
async fn spin_until(mut cond: impl FnMut() -> bool) -> bool {
    const ROUNDS: usize = 10_000;
    for _ in 0..ROUNDS {
        if cond() {
            return true;
        }
        tokio::task::yield_now().await;
    }
    cond()
}

// ═══ ③ 通道闸：复用 · 上限 · 队列 ════════════════════════════════════════════

/// 一条**假通道**：`ChannelSet` 对 `C` 一无所知，所以这一族判据用最小的 `C`。
/// 带一个序号，好让「复用的是不是同一条」变成一个可比的值。
#[derive(Debug, PartialEq, Eq)]
struct FakeChan(usize);

/// 造一个「每次开新通道就发下一个序号」的 opener，外带一个**被调次数**计数器。
fn counting_opener() -> (Arc<AtomicUsize>, impl Fn() -> FakeChan) {
    let n = Arc::new(AtomicUsize::new(0));
    let c = n.clone();
    (n, move || FakeChan(c.fetch_add(1, Ordering::SeqCst)))
}

/// 🔴 **复用恒等**：连着借还 20 次，**开过的通道恰好 1 条**。
///
/// 没有这一条，「池」与「每次操作现开一条通道」在行为上看不出差别 ——
/// 而后者每次多付三个往返（`channel_open` ＋ `request_subsystem` ＋ `SSH_FXP_INIT`），
/// 还会在远端把 `MaxSessions` 撞穿。
///
/// **相等，不是地板**：复用整个坏掉时这个数是 20，写成 `<= 20` 两种都绿。
#[tokio::test]
async fn twenty_sequential_leases_open_exactly_one_channel() {
    let set: ChannelSet<FakeChan> = ChannelSet::new(6, 4);
    let (opens, mk) = counting_opener();
    for _ in 0..20 {
        let l = set
            .lease(|| async { Ok(mk()) })
            .await
            .expect("借通道应当成功");
        assert_eq!(*l.get(), FakeChan(0), "复用的应当恒是第 0 条那一条");
    }
    let s = set.stats();
    assert_eq!(s.opened, 1, "连着借还 20 次应当只开过 1 条通道，实得 {s:?}");
    assert_eq!(opens.load(Ordering::SeqCst), 1, "opener 应当只被调用 1 次");
    assert_eq!(s.peak, 1, "从没有两条同时在借，高水位应当是 1");
    assert_eq!(s.in_use, 0, "全还回去了，在借应当是 0");
}

/// 🔴 **`seed` 那一条不白占**：`connect_sftp` 顺手建的通道进池当第 1 条，
/// 之后第一次借用**不开新通道**。
///
/// 死值验对着的是「顺手那条闲着、借用时另开一条」那种写法 ——
/// 它每个 origin 白占远端一格 `MaxSessions`（全部预算的 1/6），而**功能上毫无差别**。
#[tokio::test]
async fn the_seeded_channel_is_the_one_handed_out_first() {
    let set: ChannelSet<FakeChan> = ChannelSet::new(6, 4);
    let (opens, mk) = counting_opener();
    set.seed(FakeChan(999));
    let l = set.lease(|| async { Ok(mk()) }).await.expect("借通道");
    assert_eq!(
        *l.get(),
        FakeChan(999),
        "第一次借到的应当是 seed 进去的那条"
    );
    assert_eq!(
        opens.load(Ordering::SeqCst),
        0,
        "seed 过之后第一次借用**不该**再开新通道"
    );
    assert_eq!(set.stats().opened, 1, "`opened` 要把 seed 那条算进去");
}

/// 🔴 **通道闸恒等**：同时想借 3×cap 条，**高水位恰好是 cap**，多出来的排队。
///
/// 这是 `设计/15 §3.2` 那条硬前置（per-session channel 上限）的机器面。
/// 没有它，`MaxSessions` 会被撞穿，而撞穿的表现是 sshd 拒开通道 ——
/// **本地一条判据都不会红**（那张表第 1 条逐字记过同一形）。
///
/// 三条断言各挡一个方向：
/// ① `peak == cap`（上限真生效；没闸的话是 18）；
/// ② `opened == cap`（排队那 12 个**复用**了前面还回来的，没有各开一条）；
/// ③ 18 个全部完成（闸是**队列**不是**拒绝** —— 拒绝的话用户看到的是随机报错）。
#[tokio::test]
async fn the_channel_gate_pins_the_peak_at_the_cap_and_queues_the_rest() {
    const CAP: usize = 6;
    const WANT: usize = CAP * 3;
    let set: Arc<ChannelSet<FakeChan>> = Arc::new(ChannelSet::new(CAP, 4));
    let opens = Arc::new(AtomicUsize::new(0));
    // 放行用**标志 + 让出**，不用 `Notify` —— `notify_waiters` 只叫醒**此刻已在等**的，
    // 排队那一批是分批醒过来的，漏一个就挂死在这里。
    let release = Arc::new(AtomicBool::new(false));
    let done = Arc::new(AtomicUsize::new(0));

    let mut tasks = Vec::new();
    for _ in 0..WANT {
        let (set, opens, release, done) =
            (set.clone(), opens.clone(), release.clone(), done.clone());
        tasks.push(tokio::spawn(async move {
            let _l = set
                .lease(|| async { Ok(FakeChan(opens.fetch_add(1, Ordering::SeqCst))) })
                .await
                .expect("借通道");
            while !release.load(Ordering::SeqCst) {
                tokio::task::yield_now().await; // 攥着不放，直到主 task 量完
            }
            done.fetch_add(1, Ordering::SeqCst);
        }));
    }

    // 等到闸真的满了 —— 不看墙钟，只让出执行权。
    let converged = spin_until(|| set.stats().in_use == CAP).await;
    assert!(
        converged,
        "在借数一直没到 {CAP} —— 台架没跑起来，下面的相等断言会空转"
    );
    let s = set.stats();
    assert_eq!(s.in_use, CAP, "在借的应当恰好被闸在 {CAP}，实得 {s:?}");
    assert_eq!(
        s.peak, CAP,
        "高水位应当恰好 {CAP}（没闸的话是 {WANT}），实得 {s:?}"
    );
    assert_eq!(s.opened, CAP, "只该开 {CAP} 条通道，实得 {s:?}");

    release.store(true, Ordering::SeqCst);
    for t in tasks {
        tokio::time::timeout(Duration::from_secs(60), t)
            .await
            .expect("排队的借用应当全部走完 —— 闸是队列不是拒绝")
            .expect("借用任务不该 panic");
    }
    assert_eq!(done.load(Ordering::SeqCst), WANT, "{WANT} 个应当全部走完");
    let s = set.stats();
    assert_eq!(
        s.opened,
        CAP,
        "排队那 {} 个应当**复用**还回来的通道，一条都不该新开，实得 {s:?}",
        WANT - CAP
    );
    assert_eq!(s.peak, CAP, "全程高水位应当始终是 {CAP}，实得 {s:?}");
}

/// 🔴 **车道闸恒等 —— 「边传边浏览」那句话的机器面。**
///
/// 场景：`(cap=6, lane_cap=4)`，先塞 10 条**传输**，再来 1 次**浏览**。
///
/// 收敛后在借的应当**恰好 5**（4 传输 ＋ 1 浏览）。
/// **把车道闸拆掉，这个数是 6** —— 6 条传输把通道占满，浏览那一次永远排队，
/// 而档③ 第一行要的就是它不排队。两个数差得开，这就是这条判据的全部。
///
/// ⚠ 次序也钉在这里：`lease_transfer` 里若把车道闸挪到通道闸**之后**，
/// 6 条传输会先把通道抢光再去等车道 ⇒ 同样读到 6。
#[tokio::test]
async fn the_transfer_lane_gate_keeps_room_for_browsing() {
    const CAP: usize = 6;
    const LANES: usize = 4;
    let set: Arc<ChannelSet<FakeChan>> = Arc::new(ChannelSet::new(CAP, LANES));
    let opens = Arc::new(AtomicUsize::new(0));
    let release = Arc::new(AtomicBool::new(false));

    for _ in 0..10 {
        let (set, opens, release) = (set.clone(), opens.clone(), release.clone());
        tokio::spawn(async move {
            let _l = set
                .lease_transfer(|| async { Ok(FakeChan(opens.fetch_add(1, Ordering::SeqCst))) })
                .await
                .expect("借传输通道");
            while !release.load(Ordering::SeqCst) {
                tokio::task::yield_now().await;
            }
        });
    }
    let converged = spin_until(|| set.stats().in_use == LANES).await;
    assert!(converged, "传输在借数一直没到 {LANES} —— 台架没跑起来");
    assert_eq!(
        set.stats().in_use,
        LANES,
        "10 条传输应当被车道闸卡在 {LANES} 条（不是通道闸的 {CAP}）"
    );

    // 浏览这一次**必须当场借到** —— 这就是「边传边浏览」。
    let browse = tokio::time::timeout(
        Duration::from_secs(30),
        set.lease(|| async { Ok(FakeChan(opens.fetch_add(1, Ordering::SeqCst))) }),
    )
    .await
    .expect("传输占满车道时，浏览**不该**排队等到超时")
    .expect("借浏览通道");
    let s = set.stats();
    assert_eq!(
        s.in_use,
        LANES + 1,
        "在借的应当恰好是 {} 条传输 ＋ 1 次浏览 = {}，实得 {s:?}。\n\
         读到 {CAP} ⇒ 车道闸没生效（或者它被挪到了通道闸后面）",
        LANES,
        LANES + 1
    );
    assert_eq!(s.lane_cap, LANES, "读数里的车道闸应当就是建池时那个数");
    drop(browse);
    release.store(true, Ordering::SeqCst);
}

/// 🔴 **反向：通道闸真的会挡人**（而不是只把数记小）。
///
/// 上面那几条量的是「在借数」；这一条量的是**行为** —— 把 cap 条全攥在手里，
/// 第 cap+1 次借用必须**借不到**。没有它，一个「数记对了但谁都不挡」的实现照样全绿。
///
/// # ⚠ 为什么不用 `timeout` / `sleep` 判「借不到」
///
/// 两条：① `tokio::time::pause()` 要 tokio 的 `test-util` feature，那是动 `Cargo.toml`
/// （不在本件写区）；② 真等一段墙钟就是把判据变成计时器 ——
/// 在慢机器上它会随机红，而**红得毫无信息**。
///
/// ⇒ 改判**推进与否**：本秤全跑在 `current_thread` 运行时上，一次 `yield_now()`
/// 就足以让任何**能推进**的 task 推进。让出 `ROUNDS` 轮之后那个标志还是 `false`，
/// 说明挡住它的不是「还没轮到」，是闸。两个方向都钉：放回一条之后它必须**当场**变 `true`。
#[tokio::test]
async fn the_gate_actually_blocks_once_every_channel_is_out() {
    const CAP: usize = 3;
    let set: Arc<ChannelSet<FakeChan>> = Arc::new(ChannelSet::new(CAP, 2));
    let opens = Arc::new(AtomicUsize::new(0));
    let mut held = Vec::new();
    for _ in 0..CAP {
        held.push(
            set.lease(|| async { Ok(FakeChan(opens.fetch_add(1, Ordering::SeqCst))) })
                .await
                .expect("借通道"),
        );
    }
    assert_eq!(set.stats().in_use, CAP, "{CAP} 条应当全在手里");

    let got = Arc::new(AtomicBool::new(false));
    {
        let (set, opens, got) = (set.clone(), opens.clone(), got.clone());
        tokio::spawn(async move {
            let _l = set
                .lease(|| async { Ok(FakeChan(opens.fetch_add(1, Ordering::SeqCst))) })
                .await
                .expect("借通道");
            got.store(true, Ordering::SeqCst);
            // 拿到就不放 —— 免得它自己还回去，让下面「在借数」读花。
            std::future::pending::<()>().await;
        });
    }
    // 让出足够多轮：**能推进的早推进了**。
    for _ in 0..1_000 {
        tokio::task::yield_now().await;
    }
    assert!(
        !got.load(Ordering::SeqCst),
        "通道全借出去之后，第 {} 次借用居然拿到了 —— 那道闸是摆设",
        CAP + 1
    );
    assert_eq!(
        set.stats().in_use,
        CAP,
        "在借数应当还是 {CAP}（多出来的那个在排队）"
    );

    // ── 反过来：还回去一条，它必须当场拿到。挡住它的是闸，不是别的什么卡住了。──
    held.pop();
    let woke = spin_until(|| got.load(Ordering::SeqCst)).await;
    assert!(woke, "还回去一条之后，排队那个应当被唤醒并拿到通道");
    assert_eq!(
        set.stats().in_use,
        CAP,
        "还一条借一条，在借数应当还是 {CAP}"
    );
}

/// 🔴 **世代号那条竞态**：连接死了、池已作废，而**在飞**的那条通道稍后才归还 ——
/// 它不许回池。
///
/// 没有这一条，下一个借用的人会拿到一条**已死**的通道，必然失败一次；
/// 而那一次失败长得像「网络又抖了」，没人会怀疑到池上。
///
/// 相等读数：`opened` 从 1 变 2（新开了一条），而不是 1（把死的那条又发了一遍）。
#[tokio::test]
async fn a_channel_returned_after_invalidate_does_not_go_back_into_the_pool() {
    let set: ChannelSet<FakeChan> = ChannelSet::new(6, 4);
    let (opens, mk) = counting_opener();
    let l = set.lease(|| async { Ok(mk()) }).await.expect("借通道");
    assert_eq!(set.stats().opened, 1);

    set.invalidate(); // 连接死了
    drop(l); // 在飞的那条这会儿才回来

    let l2 = set.lease(|| async { Ok(mk()) }).await.expect("再借一条");
    assert_eq!(
        set.stats().opened,
        2,
        "作废之后归还的那条不该回池 ⇒ 下一次借用必须**新开** ⇒ `opened` 应当是 2"
    );
    assert_eq!(
        *l2.get(),
        FakeChan(1),
        "拿到的应当是新开的第 1 条，不是作废前那条 FakeChan(0)"
    );
    assert_eq!(opens.load(Ordering::SeqCst), 2, "opener 应当被调用 2 次");
}

/// 🔴 **生产那两个数就是被上面这几条量过的那两个数。**
///
/// 上面的判据用的是 `(6, 4)` / `(3, 2)` 这些**判据自己给的**参数 ——
/// 若生产侧那两个常量是别的值，上面全绿而盘上仍然可能是「1 条通道、0 条车道」。
/// ⇒ 这一条把两者接上，并把「浏览永远留得出几格」写成一个相等读数。
#[test]
fn the_production_gates_are_the_ones_this_scale_measured() {
    assert_eq!(
        SESSION_CHANNEL_CAP, 6,
        "per-session channel 上限应当是 6（`设计/15 §3.2` 表第 3 条「建议 ≤6」的上界）"
    );
    assert_eq!(TRANSFER_LANE_CAP, 4, "传输车道上限应当是 4");
    assert_eq!(
        SESSION_CHANNEL_CAP - TRANSFER_LANE_CAP,
        2,
        "**永远留给浏览的格子数**应当是 2。\n\
         它是 `设计/60 §2 档③` 第一行那句「边传边浏览」在盘上的全部本钱：\n\
         这个差一旦变成 0，传输就能把通道占满，那句话当场变回 ❌。"
    );
}

// ═══ ④ 真 SFTP 通道：两条通道同时在用 ════════════════════════════════════════

/// 🔴 **「边传边浏览」，这一次用的是真 SFTP 字节。**
///
/// 上面那一族量的是闸的算术；这一条量的是**那两格里真能同时跑两件事**：
/// 一条通道在逐块读一份 100 KiB 的文件，另一条通道同时 `stat` 成功。
///
/// 三条相等断言：
/// ① `stat` 回的大小 == 语料长度（浏览那一路真的走通了，不是「没报错」）；
/// ② 读回来的字节 == 语料（传输那一路没被浏览打断）；
/// ③ `stat` 是在读**还没读完**的时候回来的 —— 靠「读到一半才放行」的顺序保证。
#[tokio::test]
async fn a_second_channel_serves_a_stat_while_the_first_is_still_reading() {
    let corpus = synthetic_corpus();
    let fs = new_fs();
    fs.lock()
        .unwrap()
        .files
        .insert("/big.bin".to_string(), corpus.clone());

    let reader = session_on(fs.clone()).await; // 通道 #1：传输
    let browser = session_on(fs.clone()).await; // 通道 #2：浏览

    let half_done = Arc::new(AtomicBool::new(false));
    let keep_going = Arc::new(tokio::sync::Notify::new());
    let (hd, kg) = (half_done.clone(), keep_going.clone());

    let read_task = tokio::spawn(async move {
        use tokio::io::AsyncReadExt;
        let mut f = reader
            .open_with_flags(
                "/big.bin".to_string(),
                russh_sftp::protocol::OpenFlags::READ,
            )
            .await
            .expect("打开远端");
        let mut got = Vec::new();
        let mut buf = vec![0u8; 8 * 1024];
        loop {
            let n = f.read(&mut buf).await.expect("读远端");
            if n == 0 {
                break;
            }
            got.extend_from_slice(&buf[..n]);
            if got.len() >= CORPUS_LEN / 2 && !hd.swap(true, Ordering::SeqCst) {
                kg.notified().await; // 读到一半停住，让浏览那一路先走
            }
        }
        got
    });

    // 等传输那一路真的进到「读了一半、还没读完」那个状态。
    let converged = spin_until(|| half_done.load(Ordering::SeqCst)).await;
    assert!(converged, "读任务一直没读到一半 —— 台架没跑起来");

    // ★ 就在这一刻，另一条通道上做一次浏览。**这一句成立 = 边传边浏览成立。**
    let meta = tokio::time::timeout(
        Duration::from_secs(30),
        browser.metadata("/big.bin".to_string()),
    )
    .await
    .expect("传输在飞时的浏览**不该**超时")
    .expect("stat 应当成功");
    assert_eq!(
        meta.len(),
        CORPUS_LEN as u64,
        "另一条通道上 stat 回的大小应当就是语料长度"
    );

    keep_going.notify_waiters();
    let got = tokio::time::timeout(Duration::from_secs(30), read_task)
        .await
        .expect("读任务应当收尾")
        .expect("读任务不该 panic");
    assert_eq!(
        got, corpus,
        "被浏览插了一脚之后，读回来的字节应当逐字节等于语料"
    );
}

// ═══ ⑤ 断点续传：`READ`/`WRITE` 带 offset ════════════════════════════════════

/// 下载：`.part` 里已有**对的**前 40 KiB ⇒ 只补缺的那截。
///
/// 两个针，各自独立：
/// ① **服务端记下的最小读偏移 == `RESUME_AT - PROBE_LEN`**（那是尾块对拍那一趟）。
///    从头重下的话这个数是 **0** —— 差得开。
/// ② **第一条进度报的 `transferred` == `RESUME_AT`**（不是 0）。
/// ③ 落地文件逐字节 == 语料。
#[tokio::test]
async fn a_download_with_a_matching_part_resumes_from_the_verified_tail() {
    let corpus = synthetic_corpus();
    let fs = new_fs();
    fs.lock()
        .unwrap()
        .files
        .insert("/src.bin".to_string(), corpus.clone());
    let sftp = session_on(fs.clone()).await;

    let dir = TmpDir::new("dl-resume");
    let target = dir.join("out.bin");
    std::fs::write(format!("{target}.part"), &corpus[..RESUME_AT]).expect("预置 .part");

    let (ch, seen) = progress_sink();
    download_inner(&sftp, "/src.bin", &target, &never_cancelled(), &ch)
        .await
        .expect("续传应当成功");

    let min_read = *fs
        .lock()
        .unwrap()
        .read_offsets
        .iter()
        .min()
        .expect("服务端应当至少被要求读过一次 —— 一次都没有说明台架空转了");
    assert_eq!(
        min_read,
        (RESUME_AT - PROBE_LEN) as u64,
        "服务端被要求读过的**最小偏移**应当是尾块对拍那一趟（{}）。\n\
         读到 0 ⇒ 根本没续传，整份重下了一遍。",
        RESUME_AT - PROBE_LEN
    );
    assert_eq!(
        seen.lock().unwrap().first().copied(),
        Some(RESUME_AT as u64),
        "第一条进度应当报在续传起点上，而不是 0"
    );
    assert_eq!(
        std::fs::read(&target).expect("落地文件"),
        corpus,
        "续传出来的文件应当逐字节等于语料"
    );
    assert!(
        std::fs::metadata(format!("{target}.part")).is_err(),
        "成功之后 `.part` 应当已经换名上位、不再留着"
    );
}

/// 下载：`.part` 里那 40 KiB 是**别人的字节** ⇒ 不许接，整份重下。
///
/// 这一条才是「尾块对拍」存在的理由。没有它，续传会把两个文件缝成一个 ——
/// 长度对、内容错、而且换名上位之后看起来**像下完了**。
#[tokio::test]
async fn a_download_with_a_mismatched_part_starts_over_instead_of_stitching() {
    let corpus = synthetic_corpus();
    let fs = new_fs();
    fs.lock()
        .unwrap()
        .files
        .insert("/src.bin".to_string(), corpus.clone());
    let sftp = session_on(fs.clone()).await;

    let dir = TmpDir::new("dl-mismatch");
    let target = dir.join("out.bin");
    // 长度一模一样，**内容是别人的**。只看长度的实现会当场接上去。
    std::fs::write(format!("{target}.part"), vec![0xAAu8; RESUME_AT]).expect("预置 .part");

    let (ch, seen) = progress_sink();
    download_inner(&sftp, "/src.bin", &target, &never_cancelled(), &ch)
        .await
        .expect("重下应当成功");

    assert_eq!(
        fs.lock().unwrap().read_offsets.iter().min().copied(),
        Some(0),
        "尾块对不上就必须从 0 重读 —— 最小读偏移应当是 0"
    );
    assert_eq!(
        seen.lock().unwrap().first().copied(),
        Some(0),
        "没续传，第一条进度应当报 0"
    );
    assert_eq!(
        std::fs::read(&target).expect("落地文件"),
        corpus,
        "重下出来的文件应当逐字节等于语料（缝出来的那个会在这里红）"
    );
}

/// 上传：远端 `.tmp` 里已有**对的**前 40 KiB ⇒ `WRITE` 从那里接着写。
///
/// 针是**服务端记下的最小写偏移**：续上 ⇒ `RESUME_AT`；从头 ⇒ `0`。
#[tokio::test]
async fn an_upload_with_a_matching_remote_tmp_resumes_from_the_verified_tail() {
    let corpus = synthetic_corpus();
    let fs = new_fs();
    fs.lock()
        .unwrap()
        .files
        .insert("/dst.bin.tmp".to_string(), corpus[..RESUME_AT].to_vec());
    let sftp = session_on(fs.clone()).await;

    let dir = TmpDir::new("up-resume");
    let src = dir.join("in.bin");
    std::fs::write(&src, &corpus).expect("预置本地源");

    let (ch, seen) = progress_sink();
    upload_inner(&sftp, &src, "/dst.bin", &never_cancelled(), &ch)
        .await
        .expect("续传上传应当成功");

    assert_eq!(
        fs.lock().unwrap().write_offsets.iter().min().copied(),
        Some(RESUME_AT as u64),
        "服务端被要求写的**最小偏移**应当是续传起点 {RESUME_AT}。\n\
         写到 0 ⇒ 没续传（或者 `TRUNCATE` 把上次那半截清了）。"
    );
    assert_eq!(
        seen.lock().unwrap().first().copied(),
        Some(RESUME_AT as u64),
        "第一条进度应当报在续传起点上"
    );
    assert_eq!(
        fs.lock().unwrap().files.get("/dst.bin").cloned(),
        Some(corpus),
        "远端落地的字节应当逐字节等于本地源"
    );
    assert!(
        !fs.lock().unwrap().files.contains_key("/dst.bin.tmp"),
        "成功之后远端 `.tmp` 应当已经换名上位"
    );
}

/// 上传：远端 `.tmp` 是**别人的字节** ⇒ 从 0 重来（且 `TRUNCATE` 要把它清掉）。
#[tokio::test]
async fn an_upload_with_a_mismatched_remote_tmp_starts_over() {
    let corpus = synthetic_corpus();
    let fs = new_fs();
    fs.lock()
        .unwrap()
        .files
        .insert("/dst.bin.tmp".to_string(), vec![0x55u8; RESUME_AT]);
    let sftp = session_on(fs.clone()).await;

    let dir = TmpDir::new("up-mismatch");
    let src = dir.join("in.bin");
    std::fs::write(&src, &corpus).expect("预置本地源");

    let (ch, _seen) = progress_sink();
    upload_inner(&sftp, &src, "/dst.bin", &never_cancelled(), &ch)
        .await
        .expect("重传应当成功");

    assert_eq!(
        fs.lock().unwrap().write_offsets.iter().min().copied(),
        Some(0),
        "尾块对不上就必须从 0 重写"
    );
    assert_eq!(
        fs.lock().unwrap().files.get("/dst.bin").cloned(),
        Some(corpus),
        "远端落地的字节应当逐字节等于本地源（缝出来的那个会在这里红）"
    );
}

// ═══ ⑥ 半成品的存亡：取消留、报错删 ══════════════════════════════════════════

/// 🔴 **取消 ⇒ `.part` 留着。** 那是续传的本钱：暂停与继续是同一件事的两半。
#[tokio::test]
async fn a_cancelled_download_keeps_the_part_file_so_it_can_be_resumed() {
    let corpus = synthetic_corpus();
    let fs = new_fs();
    fs.lock()
        .unwrap()
        .files
        .insert("/src.bin".to_string(), corpus);
    let sftp = session_on(fs.clone()).await;

    let dir = TmpDir::new("dl-cancel");
    let target = dir.join("out.bin");
    let cancel = Arc::new(AtomicBool::new(true)); // 一进循环就撞上取消
    let (ch, _seen) = progress_sink();
    let r = download_inner(&sftp, "/src.bin", &target, &cancel, &ch).await;

    assert!(r.is_err(), "取消应当以 Err 收场");
    assert!(
        std::fs::metadata(format!("{target}.part")).is_ok(),
        "取消之后 `.part` 应当**留着** —— 删了就没有断点可续"
    );
    assert!(
        std::fs::metadata(&target).is_err(),
        "半成品绝不许冒充成品（正名上不该有东西）"
    );
}

/// 🔴 **报错 ⇒ `.part` 删掉。** 崩溃/断网不该在用户目录里堆垃圾。
///
/// 与上一条**成对**：两条一起才说得清那条规矩。只留一条的话，
/// 「永远删」和「永远留」各能被其中一条判成绿。
#[tokio::test]
async fn a_failed_download_still_cleans_up_the_part_file() {
    let corpus = synthetic_corpus();
    let fs = new_fs();
    {
        let mut g = fs.lock().unwrap();
        g.files.insert("/src.bin".to_string(), corpus);
        g.fail_read_after = Some(1); // 头一次 read 给数据，第二次起真的坏掉
    }
    let sftp = session_on(fs.clone()).await;

    let dir = TmpDir::new("dl-fail");
    let target = dir.join("out.bin");
    let (ch, _seen) = progress_sink();
    let r = download_inner(&sftp, "/src.bin", &target, &never_cancelled(), &ch).await;

    assert!(r.is_err(), "服务端中途报错时下载应当以 Err 收场");
    assert!(
        std::fs::metadata(format!("{target}.part")).is_err(),
        "**不是取消**的失败之后，`.part` 应当被清掉"
    );
    assert!(
        std::fs::metadata(&target).is_err(),
        "失败绝不许在正名上留半截文件"
    );
}
