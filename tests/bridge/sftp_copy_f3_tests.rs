//! **秤 F3**（`设计/17 §6.9`）：零流量复制的对拍 —— **两个方向都钉**。
//!
//! # 它接的是哪一句话
//!
//! `设计/17 §6.9` 逐字：
//!
//! > **F3** | 零流量复制的对拍 | 一次远端内部复制里，**客户端侧的读/写包条数** |
//! > 照 `真相源/98 §1.2 ③` 那条现打配方（`sftp -vvv` 数
//! > `Sending read|Sending write|SSH2_FXP_WRITE|SSH2_FXP_DATA`）。**恒等钉在 0** |
//! > 🔴 **它还要反过来钉退路**：换成不支持 `copy-data` 的服务端，
//! > 这个数必须**不是 0**，而且「退了路」要出声
//!
//! 同一节紧接着写明那条配方**还不是一杆秤**：
//!
//! > 它是一次手工现打，没有进任何执行链……**按本仓纪律：判据不在执行链上就等于不存在。**
//!
//! ⇒ 本文件就是把那条配方搬上执行链的那一拍。它跑在门禁 `cargo` 那一格
//! （`cargo test --workspace --exclude code-picture-core --lib`）里，每趟出货都过。
//!
//! # 为什么**不是**去跑 `sftp -vvv`
//!
//! 三条，逐条是硬理由：
//!
//! 1. **那把尺子量的不是我们的代码。** `sftp -vvv` 量的是 OpenSSH **自己的**客户端；
//!    它绿了只说明 OpenSSH 会用 `copy-data`，一个字都没说 `sftp_pool::copy_remote_path`
//!    会。本仓记过这一形（`memory/judge-not-in-exec-chain-is-no-judge`：
//!    **判据不在执行链上就等于不存在**，而「在执行链上」指的是**被测对象**在链上）。
//! 2. **反向那一半它做不到。** 「换成不支持 `copy-data` 的服务端」——
//!    本机只有一个 `sftp-server`，而且它**支持**。要造不支持的那一侧，
//!    只能自己立一个服务端。
//! 3. **门禁断网、且不许依赖装了什么。** 门禁跑在 `--network none` 的沙箱里，
//!    而 `/usr/lib/openssh/sftp-server` 在不在那个镜像里**没人保证**。
//!    一条「装了才跑、没装就跳过」的判据 = 空真（本仓反复治的那一形）。
//!
//! ⇒ 取法：**在内存里立一个讲 SFTP v3 裸字节的服务端**，两种人格各一个
//!    （报 `copy-data` / 不报），中间夹一层**按字节数包**的计数器。
//!    计数器数的是**真的走过那条流的包**，不是被测代码自己汇报的数字
//!    —— 那一条是本文件全部意义所在（`memory/judge-not-in-exec-chain-is-no-judge`
//!    的另一半：**恒等两侧同源会恒真**）。
//!
//! # 语料：**采结构不采内容**
//!
//! 被复制的字节是**编译期拼出来的**合成语料（`synthetic_corpus()`），
//! 一个真会话正文的字节都没有 —— 本仓纪律（`memory/test-fixtures-no-real-transcript`）。
//! 它唯一要具备的性质是「够长到跨多个 `CHUNK`」，好让退路那一侧的包数**大于 1**。
//!
//! # 它不守什么（逐条，别读大）
//!
//! - **它不判真远端。** 这里的服务端是本进程里的一个 `tokio::io::duplex` 对端 ⇒
//!   端到端时延、真 sshd 的行为、`真相源/98 §1.4` 那三条边界（`copy-data` 是哪个
//!   OpenSSH 版本进的 · Windows 版有没有 · 非 OpenSSH 服务端）**一条都没答**。
//!   本文件买的是「我们这一侧的选路与包面对不对」。
//! - **它不判 `copy-data` 真的没搬字节。** 服务端是我们自己写的，它「搬了」是因为
//!   我们让它搬。真服务端真的零流量那一条由 `真相源/98 §1.2 ③` 的现打背书
//!   （8 MiB、读写包 0 条、逐字节相同），那是**外部证据**，不是本文件的断言。
//! - **消息体格式**不靠本文件背书：它由 `src/bridge/src/sftp.rs` 头注那张
//!   **七刀突变表**（拿本机真 `sftp-server` 现打的）钉住，本文件只再加一条
//!   **黄金字节向量**防它被静默改形。

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use russh_sftp::protocol::{
    Attrs, Data, FileAttributes, Handle, Packet, Status, StatusCode, Version,
};

use crate::sftp::{CopyDataExtension, RawSftp, COPY_DATA, COPY_DATA_VERSION};
use crate::sftp_pool::copy_remote_path;

// ═══ ① 计数器：**按字节数包**，谁都骗不了它 ═══════════════════════════════════
//
// `真相源/98 §1.2 ③` 的配方数的是四个词：
//   `Sending read` · `Sending write` · `SSH2_FXP_WRITE` · `SSH2_FXP_DATA`
// 那是 OpenSSH 客户端**自己的日志**。本文件换成等价而更强的取法：
// 在客户端与服务端之间的那条流上，**逐包读出包类型字节**并计数。
//   · `Sending read`   ⇔ 客户端**发出**的 `SSH_FXP_READ`(5)
//   · `Sending write` / `SSH2_FXP_WRITE` ⇔ 客户端**发出**的 `SSH_FXP_WRITE`(6)
//   · `SSH2_FXP_DATA`  ⇔ 客户端**收到**的 `SSH_FXP_DATA`(103)
// 三类合起来就是「有没有文件字节经过这台机器」的充要面。

/// SFTP v3 包类型号（`draft-ietf-secsh-filexfer-02 §3` 那张表；现打与 russh-sftp
/// 3.0.0 的协议模块逐个核过）。
/// 刻意写成本文件自己的常量而**不**从库里引：库里那几个是私有 `const`，
/// 而且判据的针跟被测对象同源就等于没针。
const FXP_READ: u8 = 5;
const FXP_WRITE: u8 = 6;
const FXP_DATA: u8 = 103;

/// 走过那条流的包，按类型分类计数。
#[derive(Default, Debug)]
struct PacketTally {
    /// 客户端**发出**的 `SSH_FXP_READ` 条数。
    sent_read: AtomicUsize,
    /// 客户端**发出**的 `SSH_FXP_WRITE` 条数。
    sent_write: AtomicUsize,
    /// 客户端**收到**的 `SSH_FXP_DATA` 条数。
    recv_data: AtomicUsize,
    /// 两个方向的全部包条数 —— **反空真的锚**：它必须 > 0，
    /// 否则「零流量」与「这条流上一个包都没走过（计数器坏了 / 测试没跑起来）」
    /// 在读数上一模一样。
    total: AtomicUsize,
}

impl PacketTally {
    /// 秤 F3 的那个数：一次复制里**客户端侧的读/写包条数**。
    fn read_write_packets(&self) -> usize {
        self.sent_read.load(Ordering::SeqCst)
            + self.sent_write.load(Ordering::SeqCst)
            + self.recv_data.load(Ordering::SeqCst)
    }
}

/// 按 SFTP 帧（`uint32 长度` + 载荷）切流并数包类型的增量解析器。
///
/// ⚠ **必须是增量的**：`duplex` 的一次 `poll_read` 给多少字节没有保证，
/// 半个长度前缀是常态。第一版按「一次 read 就是一个包」写，
/// 在 32 KiB 的块上当场把一条 `WRITE` 数成 0 条（帧头被切在两次 read 之间）。
#[derive(Default)]
struct FrameSplitter {
    buf: Vec<u8>,
}

impl FrameSplitter {
    /// 喂进一段字节，返回这一段里**完整帧**的包类型号。
    fn feed(&mut self, bytes: &[u8]) -> Vec<u8> {
        self.buf.extend_from_slice(bytes);
        let mut kinds = Vec::new();
        loop {
            if self.buf.len() < 4 {
                break;
            }
            let len =
                u32::from_be_bytes([self.buf[0], self.buf[1], self.buf[2], self.buf[3]]) as usize;
            if len == 0 || self.buf.len() < 4 + len {
                break;
            }
            kinds.push(self.buf[4]);
            self.buf.drain(..4 + len);
        }
        kinds
    }
}

/// 夹在客户端与服务端之间的计数流。**它是这杆秤的秤盘。**
struct CountingStream<S> {
    inner: S,
    tally: Arc<PacketTally>,
    /// 客户端 → 服务端方向（我们 `poll_write` 出去的字节）。
    out: FrameSplitter,
    /// 服务端 → 客户端方向（我们 `poll_read` 进来的字节）。
    inb: FrameSplitter,
}

impl<S> CountingStream<S> {
    fn new(inner: S, tally: Arc<PacketTally>) -> Self {
        Self {
            inner,
            tally,
            out: FrameSplitter::default(),
            inb: FrameSplitter::default(),
        }
    }

    fn note(&self, kinds: &[u8], outgoing: bool) {
        for k in kinds {
            self.tally.total.fetch_add(1, Ordering::SeqCst);
            match (*k, outgoing) {
                (FXP_READ, true) => {
                    self.tally.sent_read.fetch_add(1, Ordering::SeqCst);
                }
                (FXP_WRITE, true) => {
                    self.tally.sent_write.fetch_add(1, Ordering::SeqCst);
                }
                (FXP_DATA, false) => {
                    self.tally.recv_data.fetch_add(1, Ordering::SeqCst);
                }
                _ => {}
            }
        }
    }
}

impl<S: tokio::io::AsyncRead + Unpin> tokio::io::AsyncRead for CountingStream<S> {
    fn poll_read(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        let before = buf.filled().len();
        let r = std::pin::Pin::new(&mut self.inner).poll_read(cx, buf);
        if r.is_ready() {
            let fresh = buf.filled()[before..].to_vec();
            let kinds = self.inb.feed(&fresh);
            self.note(&kinds, false);
        }
        r
    }
}

impl<S: tokio::io::AsyncWrite + Unpin> tokio::io::AsyncWrite for CountingStream<S> {
    fn poll_write(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        let r = std::pin::Pin::new(&mut self.inner).poll_write(cx, buf);
        if let std::task::Poll::Ready(Ok(n)) = r {
            let kinds = self.out.feed(&buf[..n]);
            self.note(&kinds, true);
        }
        r
    }

    fn poll_flush(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::pin::Pin::new(&mut self.inner).poll_flush(cx)
    }

    fn poll_shutdown(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::pin::Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}

// ═══ ② 合成服务端：两种人格，同一份文件系统 ═══════════════════════════════════

/// 服务端那一侧的「文件系统」—— 一张 `路径 → 字节` 的表。**内存里，不碰磁盘。**
#[derive(Default, Debug)]
struct FakeFs {
    files: HashMap<String, Vec<u8>>,
    /// `句柄 → (路径, 可读)`。句柄按 OpenSSH 的做法编：4 字节大端的序号
    /// （OpenSSH 的 `sftp-server` 现打就是这个形状：一个 int32 按网络字节序摆进去）。
    handles: HashMap<String, String>,
    next_handle: u32,
    /// 服务端**真的执行过**几次 `copy-data`。快路那一侧要求它恰好 1 次 ——
    /// 「客户端没发读写包」与「客户端压根什么都没做」靠这一格分开。
    copy_data_calls: usize,
    /// `copy-data` 那条消息体逐字段解出来的值（判据要逐个对拍，不许只看「成功了」）。
    last_copy_fields: Option<(String, u64, u64, String, u64)>,
}

impl FakeFs {
    fn alloc_handle(&mut self, path: &str) -> String {
        let id = self.next_handle;
        self.next_handle += 1;
        // OpenSSH 的句柄就是 4 字节大端序号；`< 0x80` 的序号是合法 UTF-8，
        // 与真服务端在这一档上逐字节同形。
        let h = String::from_utf8(id.to_be_bytes().to_vec())
            .expect("序号 < 0x80 时 4 字节大端必是合法 UTF-8");
        self.handles.insert(h.clone(), path.to_string());
        h
    }
}

/// 本服务端的人格。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Persona {
    /// 报 `copy-data`、且真的实现它 —— 本机 `OpenSSH_10.2p1` 那一档。
    SupportsCopyData,
    /// **握手时就不报** `copy-data`（`真相源/98 §1.4` 点名没量的那一族：
    /// 老版 OpenSSH · Windows 版 · 非 OpenSSH 实现）。
    NoCopyData,
    /// 🔴 **最阴的那一种**：握手报了 `copy-data`，真发过去却回 `OP_UNSUPPORTED`。
    /// 只按「握手报没报」选路的实现会在这一档上**硬失败**而不是退路。
    LiesAboutCopyData,
}

struct FakeSftpServer {
    persona: Persona,
    fs: Arc<Mutex<FakeFs>>,
}

/// 严格解 `copy-data` 的消息体。**独立于 `CopyDataExtension` 的序列化**
/// （手抠字节、按 `src/bridge/src/sftp.rs` 那张现打突变表的布局），
/// 而且**要求把整个消息体吃干净** —— 多一个字段少一个字段都在这里红。
///
/// ★ 这一条是本文件的**抗恒真网**：要是解码也用 `CopyDataExtension`（同一个 serde 派生），
/// 那「客户端写对了」与「客户端写错了而解码器跟着错」在读数上一模一样。
fn decode_copy_data_body(body: &[u8]) -> Result<(String, u64, u64, String, u64), String> {
    let mut i = 0usize;
    let take_str = |i: &mut usize| -> Result<String, String> {
        if body.len() < *i + 4 {
            return Err("string 的长度前缀都不够".into());
        }
        let n = u32::from_be_bytes([body[*i], body[*i + 1], body[*i + 2], body[*i + 3]]) as usize;
        *i += 4;
        if body.len() < *i + n {
            return Err(format!("string 说自己 {n} 字节，剩下的不够"));
        }
        let s = String::from_utf8(body[*i..*i + n].to_vec()).map_err(|e| e.to_string())?;
        *i += n;
        Ok(s)
    };
    let take_u64 = |i: &mut usize| -> Result<u64, String> {
        if body.len() < *i + 8 {
            return Err("uint64 不够 8 字节".into());
        }
        let mut b = [0u8; 8];
        b.copy_from_slice(&body[*i..*i + 8]);
        *i += 8;
        Ok(u64::from_be_bytes(b))
    };
    let read_handle = take_str(&mut i)?;
    let read_offset = take_u64(&mut i)?;
    let read_len = take_u64(&mut i)?;
    let write_handle = take_str(&mut i)?;
    let write_offset = take_u64(&mut i)?;
    if i != body.len() {
        return Err(format!(
            "消息体没吃干净：解完 5 个字段用了 {i} 字节，实到 {} 字节",
            body.len()
        ));
    }
    Ok((
        read_handle,
        read_offset,
        read_len,
        write_handle,
        write_offset,
    ))
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
        let mut extensions = HashMap::new();
        // 这几条不论人格都报 —— 照本机现打的扩展全表（`真相源/98` 那一趟量到 11 条），
        // 好让「copy-data 那一格的差异」是人格之间**唯一**的差异。
        extensions.insert("hardlink@openssh.com".to_string(), "1".to_string());
        extensions.insert("fsync@openssh.com".to_string(), "1".to_string());
        if matches!(
            self.persona,
            Persona::SupportsCopyData | Persona::LiesAboutCopyData
        ) {
            extensions.insert(COPY_DATA.to_string(), COPY_DATA_VERSION.to_string());
        }
        async move {
            Ok(Version {
                version: 3,
                extensions,
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
            let exclusive = pflags.contains(russh_sftp::protocol::OpenFlags::EXCLUDE);
            if creating {
                if exclusive && fs.files.contains_key(&filename) {
                    return Err(StatusCode::Failure);
                }
                fs.files.insert(filename.clone(), Vec::new());
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
            let fs = fs.lock().unwrap();
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
            // 与 russh-sftp 头注那句「标准 SFTP `rename` 不覆盖已存在目标」同形。
            if fs.files.contains_key(&newpath) {
                return Err(StatusCode::Failure);
            }
            let bytes = fs.files.remove(&oldpath).ok_or(StatusCode::NoSuchFile)?;
            fs.files.insert(newpath, bytes);
            Ok(ok_status(id))
        }
    }

    fn extended(
        &mut self,
        id: u32,
        request: String,
        data: Vec<u8>,
    ) -> impl std::future::Future<Output = Result<Packet, Self::Error>> + Send {
        let fs = self.fs.clone();
        let persona = self.persona;
        async move {
            if request != COPY_DATA {
                return Err(StatusCode::OpUnsupported);
            }
            if persona == Persona::LiesAboutCopyData {
                return Err(StatusCode::OpUnsupported);
            }
            let (rh, roff, rlen, wh, woff) =
                decode_copy_data_body(&data).map_err(|_| StatusCode::BadMessage)?;
            let mut fs = fs.lock().unwrap();
            fs.copy_data_calls += 1;
            fs.last_copy_fields = Some((rh.clone(), roff, rlen, wh.clone(), woff));
            let src = fs.handles.get(&rh).ok_or(StatusCode::Failure)?.clone();
            let dst = fs.handles.get(&wh).ok_or(StatusCode::Failure)?.clone();
            let bytes = fs.files.get(&src).ok_or(StatusCode::NoSuchFile)?.clone();
            let from = roff as usize;
            if from > bytes.len() {
                return Err(StatusCode::Failure);
            }
            // `rlen == 0` ⇒ 一直读到 EOF。**这条语义是现打出来的**
            // （`sftp.rs` 那张突变表第三行：填 0 得全长、填 100 得 100 字节）。
            let take = if rlen == 0 {
                bytes.len() - from
            } else {
                (rlen as usize).min(bytes.len() - from)
            };
            let payload = bytes[from..from + take].to_vec();
            let target = fs.files.get_mut(&dst).ok_or(StatusCode::NoSuchFile)?;
            let at = woff as usize;
            if target.len() < at + payload.len() {
                target.resize(at + payload.len(), 0);
            }
            target[at..at + payload.len()].copy_from_slice(&payload);
            Ok(Packet::Status(ok_status(id)))
        }
    }
}

fn ok_status(id: u32) -> Status {
    Status {
        id,
        status_code: StatusCode::Ok,
        error_message: "Success".to_string(),
        language_tag: "en-US".to_string(),
    }
}

// ═══ ③ 台架 ═════════════════════════════════════════════════════════════════

/// 合成语料。**编译期拼**，一个真会话正文的字节都没有
/// （`memory/test-fixtures-no-real-transcript`：采结构不采内容）。
///
/// 唯一要具备的结构性质：**长过一个 `CHUNK`（32 KiB）好几倍**，
/// 否则退路那一侧的包数会退化成 1，而「1」与「被截断成一块」分不开。
fn synthetic_corpus() -> Vec<u8> {
    // 100 KiB = 32 KiB × 3 + 4 KiB ⇒ 退路必然发 4 条 READ（含最后那条短读）
    // 与 4 条 WRITE，而 `CHUNK` 是 32 KiB 这件事写在 `sftp_pool.rs` 里。
    let unit: [u8; 16] = *b"F3-corpus-\x00\x01\x02\x03\x04\x05";
    unit.iter().copied().cycle().take(100 * 1024).collect()
}

struct Rig {
    rs: RawSftp,
    tally: Arc<PacketTally>,
    fs: Arc<Mutex<FakeFs>>,
}

/// 起一个台架：合成服务端（指定人格）＋ 计数流 ＋ 一个已 `init` 的裸客户端会话。
async fn rig(persona: Persona, corpus: &[u8]) -> Rig {
    let fs = Arc::new(Mutex::new(FakeFs::default()));
    fs.lock()
        .unwrap()
        .files
        .insert("/src.bin".to_string(), corpus.to_vec());

    let (client_io, server_io) = tokio::io::duplex(256 * 1024);
    russh_sftp::server::run(
        server_io,
        FakeSftpServer {
            persona,
            fs: fs.clone(),
        },
    )
    .await;

    let tally = Arc::new(PacketTally::default());
    let raw =
        russh_sftp::client::RawSftpSession::new(CountingStream::new(client_io, tally.clone()));
    let version = raw.init().await.expect("裸会话 init 应当成功");
    let copy_data = version
        .extensions
        .get(COPY_DATA)
        .is_some_and(|v| v == COPY_DATA_VERSION);
    Rig {
        rs: RawSftp { raw, copy_data },
        tally,
        fs,
    }
}

fn never_cancelled() -> AtomicBool {
    AtomicBool::new(false)
}

// ═══ ④ 秤 F3 · 正向：**读/写包条数恒等钉 0** ══════════════════════════════════

/// 🔴 **F3 正向。** 一次远端内部复制里，客户端侧读/写包条数 **== 0**。
///
/// 四条断言，缺一条这杆秤就瞎在一个方向上：
///
/// 1. **`read_write_packets() == 0`** —— 秤面本身（**相等**，不是 `<= N`）。
/// 2. **`total > 0`** —— 反空真：这条流上真的走过包。少了这一条，
///    「零流量」与「测试压根没跑起来 / 计数器坏了」在读数上一模一样。
/// 3. **服务端真的执行了 1 次 `copy-data`** —— 「客户端没发读写包」
///    与「客户端什么都没做」靠这一格分开。
/// 4. **目的地逐字节等于源** —— 复制得对不对。
///
/// 死值验见文件尾部那几条「负控」。
#[tokio::test]
async fn f3_forward_a_server_side_copy_costs_zero_client_read_write_packets() {
    let corpus = synthetic_corpus();
    let rig = rig(Persona::SupportsCopyData, &corpus).await;
    let cancel = never_cancelled();

    let verdict = copy_remote_path(&rig.rs, "/src.bin", "/dst.bin", &cancel, &|_, _| {})
        .await
        .expect("支持 copy-data 的服务端上，复制应当成功");

    assert_eq!(
        verdict, None,
        "服务端支持 `copy-data` 却报了退路说明：{verdict:?}\n\
         ★ `None` 是「零流量那条路走上了」的唯一表示形式。"
    );

    // ① 秤面：**恒等钉 0**。
    assert_eq!(
        rig.tally.read_write_packets(),
        0,
        "秤 F3 正向红了：客户端侧读/写包 {} 条（发出 READ {} · 发出 WRITE {} · 收到 DATA {}），\
         钉的是 **0**。\n\
         ★ 这意味着**文件字节经过了这台机器** —— `设计/60 §3.1` 那句「复制也零流量」\
         就是从这个数上买来的，它一变非 0，那句话当场是假的。",
        rig.tally.read_write_packets(),
        rig.tally.sent_read.load(Ordering::SeqCst),
        rig.tally.sent_write.load(Ordering::SeqCst),
        rig.tally.recv_data.load(Ordering::SeqCst),
    );

    // ② 反空真：这条流上真的走过包（OPEN/HANDLE/EXTENDED/STATUS/CLOSE/RENAME…）。
    assert!(
        rig.tally.total.load(Ordering::SeqCst) > 0,
        "这条流上一个包都没数到 —— 那不是「零流量」，是**这杆秤没工作**。\n\
         （`FrameSplitter` 坏了 / 台架没跑起来 / 计数流被绕过了，三种都长这样。）"
    );

    // ③ 服务端真的干了那一件事，而且**恰好一次**。
    let fs = rig.fs.lock().unwrap();
    assert_eq!(
        fs.copy_data_calls, 1,
        "服务端执行 `copy-data` 的次数是 {}，应当恰好 1 —— \
         0 次意味着客户端压根没发那条消息（而读写包又是 0 ⇒ 什么都没做）。",
        fs.copy_data_calls
    );

    // ④ 消息体那五个字段逐个对拍 —— 不许只看「成功了」。
    let (rh, roff, rlen, wh, woff) = fs
        .last_copy_fields
        .clone()
        .expect("服务端应当记下这一趟解出来的字段");
    assert_eq!(
        (roff, rlen, woff),
        (0, 0, 0),
        "`copy-data` 的三个 uint64 应当是 (读偏移 0, 长度 0=到EOF, 写偏移 0)，实得 \
         ({roff}, {rlen}, {woff})"
    );
    assert_ne!(
        rh, wh,
        "读句柄与写句柄解出来是同一个（`{rh:?}`）—— 那会把文件复制到它自己身上"
    );

    // ⑤ 逐字节。
    assert_eq!(
        fs.files.get("/dst.bin").map(|v| v.as_slice()),
        Some(corpus.as_slice()),
        "复制出来的字节与源不同（长度：{:?} vs {}）",
        fs.files.get("/dst.bin").map(|v| v.len()),
        corpus.len()
    );
    assert!(
        !fs.files.contains_key("/dst.bin.part"),
        "`.part` 还留在盘上 —— 换名上位那一步没走完"
    );
}

// ═══ ⑤ 秤 F3 · 反向：**退路那个数必须不是 0，而且要出声** ══════════════════════

/// 🔴 **F3 反向（`设计/17 §6.9` 逐字要求的那一半）。**
/// 换成**不报** `copy-data` 的服务端 ⇒ 读/写包条数**必须不是 0**，
/// 而且「退了路」这件事要出声。
///
/// ⚠ **这里不许写成 `> 0` 就完事**（本仓纪律：地板在「变少」方向是瞎的）。
/// 包条数是**算得出来的**：语料 100 KiB、`CHUNK` 32 KiB ⇒
/// READ 4 条（32+32+32+4）＋ 一条触到 EOF 的 READ ＝ 5 条 · WRITE 4 条 · DATA 4 条。
/// ⇒ 用**相等**钉住，而不是钉一个地板。
#[tokio::test]
async fn f3_reverse_a_server_without_copy_data_falls_back_loudly_and_pays_the_traffic() {
    let corpus = synthetic_corpus();
    let rig = rig(Persona::NoCopyData, &corpus).await;
    let cancel = never_cancelled();

    let verdict = copy_remote_path(&rig.rs, "/src.bin", "/dst.bin", &cancel, &|_, _| {})
        .await
        .expect("退路也要把文件复制成功");

    // ① **出声**：退了路必须有一句话，而且那句话得说清两件事。
    let why = verdict.expect(
        "服务端不支持 `copy-data`，退了路，可返回的是 `None`（= 「零流量走上了」）。\n\
         ★ 这正是 `设计/60 §5` 第二段禁的那一形：**静默退化成 2× 流量**。",
    );
    assert!(
        why.contains("copy-data"),
        "退路说明里没提 `copy-data` —— 用户看不出是哪一样能力没谈成：{why}"
    );
    assert!(
        why.contains(&corpus.len().to_string()),
        "退路说明里没有**实际过网字节数**（语料 {} 字节）—— \
         「慢」是个形容词，用户要的是一个数：{why}",
        corpus.len()
    );

    // ② 秤面反向：**相等**，不是地板。
    let chunk = 32 * 1024usize; // = `sftp_pool::CHUNK`
    let want_data = corpus.len().div_ceil(chunk); // 4 条带数据的 READ 回包
    let want_read = want_data + 1; // 末尾多一条读到 EOF 的
    let want_write = want_data;
    assert_eq!(
        (
            rig.tally.sent_read.load(Ordering::SeqCst),
            rig.tally.sent_write.load(Ordering::SeqCst),
            rig.tally.recv_data.load(Ordering::SeqCst),
        ),
        (want_read, want_write, want_data),
        "退路的包面与算出来的不符。\n\
         语料 {} 字节 · `CHUNK` {chunk} 字节 ⇒ 应当 READ {want_read} 条（含末尾那条 EOF）\
         · WRITE {want_write} 条 · DATA {want_data} 条。\n\
         ⚠ **本条刻意是相等而不是 `> 0`**：地板在「变少」方向是瞎的 —— \
         有人把退路改成「只搬前 32 KiB 就说完事」，`> 0` 照样绿。",
        corpus.len(),
    );
    assert_ne!(
        rig.tally.read_write_packets(),
        0,
        "不支持 `copy-data` 的服务端上，读/写包居然是 0 条 —— \
         那说明它**没有真的搬字节**，而复制却报成功了。"
    );

    // ③ 服务端一次 `copy-data` 都没执行过。
    let fs = rig.fs.lock().unwrap();
    assert_eq!(
        fs.copy_data_calls, 0,
        "服务端没报这条扩展，却被执行了 {} 次 `copy-data`",
        fs.copy_data_calls
    );

    // ④ 退路也得把文件复制对 —— 退路不是「放弃」。
    assert_eq!(
        fs.files.get("/dst.bin").map(|v| v.as_slice()),
        Some(corpus.as_slice()),
        "退路复制出来的字节与源不同"
    );
}

/// 🔴 **第三种人格：握手报了 `copy-data`、真发过去却回 `OP_UNSUPPORTED`。**
///
/// 它不是臆想出来的形状：`真相源/98 §1.4` 点名「非 OpenSSH 的 SFTP 服务端一概没量」，
/// 而「报了但没实现」是协议扩展里最常见的一种不老实。
/// **只按握手选路的实现会在这一档上硬失败**（用户看到的是「复制失败」，
/// 而不是「慢路走完了」）⇒ 单独一条。
#[tokio::test]
async fn f3_a_server_that_advertises_copy_data_but_rejects_it_still_falls_back_loudly() {
    let corpus = synthetic_corpus();
    let rig = rig(Persona::LiesAboutCopyData, &corpus).await;
    let cancel = never_cancelled();

    let verdict = copy_remote_path(&rig.rs, "/src.bin", "/dst.bin", &cancel, &|_, _| {})
        .await
        .expect("握手报了却回 OP_UNSUPPORTED 时，也要退路走完、不许硬失败");

    let why = verdict.expect("这一档同样必须出声");
    assert!(
        why.contains("OP_UNSUPPORTED") || why.contains("UNSUPPORTED"),
        "这一档的说明应当点名它回的那个状态码，否则与「握手就没报」分不开：{why}"
    );
    assert_ne!(
        rig.tally.read_write_packets(),
        0,
        "这一档退了路，读/写包不该是 0"
    );
    let fs = rig.fs.lock().unwrap();
    assert_eq!(
        fs.files.get("/dst.bin").map(|v| v.as_slice()),
        Some(corpus.as_slice()),
        "这一档复制出来的字节与源不同"
    );
}

// ═══ ⑥ 消息体黄金字节向量：**防它被静默改形** ═════════════════════════════════

/// `CopyDataExtension` 序列化出来的字节，**逐字节钉住**。
///
/// # 这个黄金值从哪来
///
/// 它就是 `src/bridge/src/sftp.rs` 头注那张**七刀突变表**里「原样」那一行发过去的布局，
/// 拿本机真 `/usr/lib/openssh/sftp-server`（`OpenSSH_10.2p1 Ubuntu-2ubuntu3.6`）
/// 现打验过 —— 换任一个字段的位置或宽度，那张表里对应那一刀的读数就不是 `Success`。
///
/// ★ **为什么要有这一条**：上面三条判据用的是**我们自己的**服务端。
/// 有人把 `read_from_offset` 和 `read_data_length` 两个字段对调，
/// 那三条**照样全绿**（两侧同源、而且这一路上两个值都是 0）——
/// 本仓记过这一形（`memory/judge-not-in-exec-chain-is-no-judge`：**恒等两侧同源会恒真**）。
/// 这一条是那个洞的补丁：它把布局钉在**外部现打**的那个事实上。
#[test]
fn the_copy_data_body_is_byte_for_byte_the_layout_openssh_accepted() {
    let body: Vec<u8> = CopyDataExtension {
        read_from_handle: "\u{0}\u{0}\u{0}\u{0}".to_string(), // OpenSSH 的句柄 0：4 字节大端
        read_from_offset: 0,
        read_data_length: 0,
        write_to_handle: "\u{0}\u{0}\u{0}\u{1}".to_string(), // 句柄 1
        write_to_offset: 0,
    }
    .try_into()
    .expect("序列化不该失败");

    // string(4B len + 4B 句柄) · uint64 · uint64 · string(4+4) · uint64
    let want: Vec<u8> = [
        vec![0, 0, 0, 4, 0, 0, 0, 0], // read-from-handle = 句柄 0
        vec![0, 0, 0, 0, 0, 0, 0, 0], // read-from-offset = 0
        vec![0, 0, 0, 0, 0, 0, 0, 0], // read-data-length = 0（到 EOF）
        vec![0, 0, 0, 4, 0, 0, 0, 1], // write-to-handle = 句柄 1
        vec![0, 0, 0, 0, 0, 0, 0, 0], // write-to-offset = 0
    ]
    .concat();

    assert_eq!(
        body, want,
        "`copy-data` 的消息体字节变了。\n\
         ★ 这个布局是拿本机真 `sftp-server` **逐字段突变**量出来的\
         （见 `src/bridge/src/sftp.rs` 头注那张七刀表）：\n\
         · 砍掉末尾 `write-to-offset` ⇒ 服务端**直接断连**\n\
         · 偏移量写成 `uint32` ⇒ 服务端**直接断连**\n\
         · 读/写句柄对调 ⇒ `No such file`\n\
         · 句柄换成路径字符串 ⇒ `Failure`\n\
         ⇒ 它不是「怎么写都行」的一段字节，改形就是发一条真服务端读不懂的消息。"
    );
    // 长度也钉一下：4+4 + 8 + 8 + 4+4 + 8 = 40。
    assert_eq!(
        body.len(),
        40,
        "消息体应当恰好 40 字节，实得 {}",
        body.len()
    );

    // ── 🔴 **第二条向量，值全不相同** —— 这一条是死值验逼出来的 ──────────────
    //
    // 上面那条向量里三个 `uint64` **全是 0** ⇒ 把 `read_from_offset` 与
    // `read_data_length` 两个字段**对调**，它产的字节一个都不变、本条照样绿。
    // 〔死值验 `M3` 现打：对调那两个字段，上面那一条**没红**，
    //  只有 `the_strict_body_decoder_rejects_every_reshaping` 红了。〕
    // ⇒ 一条「值全是 0」的黄金向量钉不住**字段顺序**，而顺序正是这里最要紧的东西。
    //   补一条值互不相同的：三个 `uint64` 取 1 / 2 / 3，两个句柄长度也不同。
    let distinct: Vec<u8> = CopyDataExtension {
        read_from_handle: "\u{0}\u{0}\u{0}\u{7}".to_string(),
        read_from_offset: 1,
        read_data_length: 2,
        write_to_handle: "\u{0}\u{0}\u{0}\u{9}".to_string(),
        write_to_offset: 3,
    }
    .try_into()
    .expect("序列化不该失败");
    let want_distinct: Vec<u8> = [
        vec![0, 0, 0, 4, 0, 0, 0, 7], // read-from-handle
        vec![0, 0, 0, 0, 0, 0, 0, 1], // read-from-offset = 1
        vec![0, 0, 0, 0, 0, 0, 0, 2], // read-data-length = 2
        vec![0, 0, 0, 4, 0, 0, 0, 9], // write-to-handle
        vec![0, 0, 0, 0, 0, 0, 0, 3], // write-to-offset = 3
    ]
    .concat();
    assert_eq!(
        distinct, want_distinct,
        "字段**顺序**变了。三个 uint64 在线上必须依次是 \
         读偏移(1) → 长度(2) → 写偏移(3)。\n\
         ★ 现打读数（本机真 `sftp-server`，见 `sftp.rs` 头注那张表第二、三行）：\n\
         · 第三个字段填 100 ⇒ 目的地恰好 100 字节 ⇒ 那一格是**长度**\n\
         · 第二个字段填 100、长度留 0 ⇒ 目的地 3996 字节（源 4096）⇒ 那一格是**读偏移**\n\
         ⇒ 两格对调 = 「从偏移 2 读 1 个字节」而不是「从偏移 1 读 2 个字节」。"
    );
}

/// 扩展名**不带** `@openssh.com` 后缀 —— 现打：带后缀发过去回 `OP_UNSUPPORTED`（码 8）。
///
/// 这一条钉的是 `真相源/98 §1.1` 记过的那一形**反过来的一面**
/// （那次是「搜的时候带了后缀，搜不到，拿搜不到当不存在」）。
#[test]
fn the_extension_name_carries_no_openssh_suffix() {
    assert_eq!(COPY_DATA, "copy-data");
    assert!(
        !COPY_DATA.contains('@'),
        "`copy-data` 一带后缀，本机 10.2p1 现打回 `SSH_FX_OP_UNSUPPORTED`（码 8）—— \
         实得 `{COPY_DATA}`"
    );
    assert_eq!(COPY_DATA_VERSION, "1", "现打：服务端报的修订号是 `1`");
}

// ═══ ⑦ 负控：**这几条证明上面那几条真的会红** ══════════════════════════════════

/// 计数器自己的死值验：**喂一条真 `SSH_FXP_WRITE` 帧进去，它必须数到**。
///
/// ★ 为什么要有这一条：正向那一格断言的是「读/写包 == 0」，
/// 而**一个永远返回 0 的计数器**会让它永远绿。
/// 上面那条 `total > 0` 挡住了「一个包都没数到」，但挡不住
/// 「总数在数、而 READ/WRITE/DATA 那三格的分类坏了」。
/// ⇒ 这一条把分类逐类打一遍，用的是**手拼的帧**，不经任何被测代码。
#[test]
fn the_packet_counter_really_recognises_read_write_and_data_frames() {
    let tally = Arc::new(PacketTally::default());
    let mut sp = FrameSplitter::default();
    // 手拼三条帧：长度前缀 + 类型字节 + 一点载荷。
    let frame = |kind: u8| -> Vec<u8> {
        let payload = vec![kind, 0, 0, 0, 7];
        let mut f = (payload.len() as u32).to_be_bytes().to_vec();
        f.extend_from_slice(&payload);
        f
    };
    let stream = CountingStream::new(tokio::io::empty(), tally.clone());
    for k in [FXP_READ, FXP_WRITE] {
        let kinds = sp.feed(&frame(k));
        assert_eq!(kinds, vec![k], "帧切分器没认出类型 {k}");
        stream.note(&kinds, true);
    }
    let kinds = sp.feed(&frame(FXP_DATA));
    stream.note(&kinds, false);

    assert_eq!(
        (
            tally.sent_read.load(Ordering::SeqCst),
            tally.sent_write.load(Ordering::SeqCst),
            tally.recv_data.load(Ordering::SeqCst),
            tally.total.load(Ordering::SeqCst),
        ),
        (1, 1, 1, 3),
        "计数器的三格分类坏了 —— 那会让正向那条「== 0」变成恒真"
    );
    assert_eq!(tally.read_write_packets(), 3);
}

/// 帧切分器的死值验：**帧头被切在两次 `read` 之间时也得数对**。
///
/// 这是第一版真栽过的那一形（头注里记着）：按「一次 read 就是一个包」写，
/// 32 KiB 的块上 `WRITE` 被数成 0 条 ⇒ **退路那一侧的判据当场假绿**。
#[test]
fn the_frame_splitter_survives_a_header_split_across_reads() {
    let mut sp = FrameSplitter::default();
    let payload = vec![FXP_WRITE, 1, 2, 3];
    let mut frame = (payload.len() as u32).to_be_bytes().to_vec();
    frame.extend_from_slice(&payload);
    // 逐字节喂 —— 最极端的切法。
    let mut seen = Vec::new();
    for b in &frame {
        seen.extend(sp.feed(&[*b]));
    }
    assert_eq!(
        seen,
        vec![FXP_WRITE],
        "逐字节喂同一条帧，切分器应当在最后一个字节到齐时吐出恰好一个类型"
    );
    // 两条帧粘在一包里也得都认出来。
    let mut two = frame.clone();
    two.extend_from_slice(&frame);
    let mut sp2 = FrameSplitter::default();
    assert_eq!(
        sp2.feed(&two),
        vec![FXP_WRITE, FXP_WRITE],
        "两条帧粘包时应当吐出两个类型"
    );
}

/// 严格解码器的死值验：**每一种改形都要红**。
///
/// 服务端那一侧靠 [`decode_copy_data_body`] 把消息体吃干净。
/// 要是它对「少一个字段 / 多一段尾巴」宽容，那 F3 正向就变成了
/// 「客户端发了点什么、服务端凑合解了」—— 那不是判据。
#[test]
fn the_strict_body_decoder_rejects_every_reshaping() {
    let good: Vec<u8> = CopyDataExtension {
        read_from_handle: "\u{0}\u{0}\u{0}\u{0}".to_string(),
        read_from_offset: 7,
        read_data_length: 11,
        write_to_handle: "\u{0}\u{0}\u{0}\u{1}".to_string(),
        write_to_offset: 13,
    }
    .try_into()
    .unwrap();

    let (rh, roff, rlen, wh, woff) =
        decode_copy_data_body(&good).expect("原样必须解得开，而且字段各就各位");
    assert_eq!(
        (roff, rlen, woff),
        (7, 11, 13),
        "字段解错位了 —— 那正是「两侧同源会恒真」那个洞"
    );
    assert_eq!(rh.len(), 4);
    assert_eq!(wh.len(), 4);

    // ① 砍掉末尾 8 字节（= 少了 `write-to-offset`）。
    assert!(
        decode_copy_data_body(&good[..good.len() - 8]).is_err(),
        "少一个 `write-to-offset` 也解得开 —— 真服务端在这一刀上是**直接断连**"
    );
    // ② 尾巴上多一个字节。
    let mut longer = good.clone();
    longer.push(0);
    assert!(
        decode_copy_data_body(&longer).is_err(),
        "多一段尾巴也解得开 ⇒ 解码器没在「吃干净」那一格上出声"
    );
    // ③ 把偏移量写成 uint32（整体短 12 字节）—— 现打：服务端直接断连。
    let short: Vec<u8> = [
        vec![0, 0, 0, 4, 0, 0, 0, 0],
        vec![0, 0, 0, 7],
        vec![0, 0, 0, 11],
        vec![0, 0, 0, 4, 0, 0, 0, 1],
        vec![0, 0, 0, 13],
    ]
    .concat();
    assert!(
        decode_copy_data_body(&short).is_err(),
        "偏移量写成 uint32 也解得开 ⇒ 那一刀在这把尺子上没有牙"
    );
    // ④ 空消息体。
    assert!(decode_copy_data_body(&[]).is_err(), "空消息体必须红");
}

/// 台架自己的死值验：**两种人格真的不一样**。
///
/// 要是 `Persona` 那个开关接错了（两边都报 `copy-data`，或者两边都不报），
/// 正反两条判据里就有一条在测另一条的场景，而两条**都会绿**。
#[tokio::test]
async fn the_two_personas_really_negotiate_differently() {
    let corpus = b"short".to_vec();
    let yes = rig(Persona::SupportsCopyData, &corpus).await;
    let no = rig(Persona::NoCopyData, &corpus).await;
    let lies = rig(Persona::LiesAboutCopyData, &corpus).await;
    assert!(
        yes.rs.copy_data,
        "「支持」那个人格握手后 `copy_data` 应当为 true"
    );
    assert!(
        !no.rs.copy_data,
        "「不支持」那个人格握手后 `copy_data` 应当为 false"
    );
    assert!(
        lies.rs.copy_data,
        "「报了却不干」那个人格**握手时是报的** —— 它的差异在发出去之后才显现"
    );
}
