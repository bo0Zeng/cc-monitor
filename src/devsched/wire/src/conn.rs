//! 连接：一对字节流上收发帧。读端各在自己一个线程里解析，收的一头按时限等。

use crate::frame::Frame;
use crate::line::{write_frame, FrameReader, WireError};
use std::io::{self, Read, Write};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

/// 收一帧的结果。
#[derive(Debug)]
pub enum Recv {
    Frame(Frame),
    Timeout,
    /// 对面关了（读到结尾或读出错）
    Closed,
    /// 坏的一行；连接还在，接着收
    Bad(WireError),
}

/// 一条连接。
pub trait Conn: Send {
    fn send(&mut self, f: &Frame) -> io::Result<()>;
    fn recv_timeout(&mut self, d: Duration) -> Recv;
    /// 关掉发的一头（对面读到结尾）；收的一头之后照样收得到对面已经发出的。
    fn close(&mut self);
}

enum Item {
    Frame(Frame),
    Bad(WireError),
}

/// 任一对字节流上的连接（编译节点接标准输入 / 输出时用）。
pub struct StreamConn {
    w: Option<Box<dyn Write + Send>>,
    rx: mpsc::Receiver<Item>,
    closed: bool,
}

impl StreamConn {
    pub fn new<R, W>(r: R, w: W) -> Self
    where
        R: Read + Send + 'static,
        W: Write + Send + 'static,
    {
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let mut fr = FrameReader::new(r);
            loop {
                let item = match fr.read_frame() {
                    Ok(None) => break,
                    Ok(Some(f)) => Item::Frame(f),
                    Err(WireError::Io(_)) => break,
                    Err(e) => Item::Bad(e),
                };
                if tx.send(item).is_err() {
                    break;
                }
            }
        });
        StreamConn {
            w: Some(Box::new(w)),
            rx,
            closed: false,
        }
    }
}

impl Conn for StreamConn {
    fn send(&mut self, f: &Frame) -> io::Result<()> {
        match self.w.as_mut() {
            Some(w) => write_frame(w, f),
            None => Err(io::Error::new(io::ErrorKind::BrokenPipe, "这一头已经关了")),
        }
    }

    fn recv_timeout(&mut self, d: Duration) -> Recv {
        if self.closed {
            return Recv::Closed;
        }
        match self.rx.recv_timeout(d) {
            Ok(Item::Frame(f)) => Recv::Frame(f),
            Ok(Item::Bad(e)) => Recv::Bad(e),
            Err(mpsc::RecvTimeoutError::Timeout) => Recv::Timeout,
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                self.closed = true;
                Recv::Closed
            }
        }
    }

    fn close(&mut self) {
        self.w = None;
    }
}

// ── 进程内一对内存管道

struct PipeW(mpsc::Sender<Vec<u8>>);

impl Write for PipeW {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0
            .send(buf.to_vec())
            .map_err(|_| io::Error::new(io::ErrorKind::BrokenPipe, "对面关了"))?;
        Ok(buf.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

struct PipeR {
    rx: mpsc::Receiver<Vec<u8>>,
    buf: Vec<u8>,
    pos: usize,
}

impl Read for PipeR {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        while self.pos >= self.buf.len() {
            match self.rx.recv() {
                Ok(b) => {
                    self.buf = b;
                    self.pos = 0;
                }
                Err(_) => return Ok(0),
            }
        }
        let n = out.len().min(self.buf.len() - self.pos);
        out[..n].copy_from_slice(&self.buf[self.pos..self.pos + n]);
        self.pos += n;
        Ok(n)
    }
}

/// 一对内存管道：(读, 写)。
pub fn mem_pipe() -> (impl Read + Send + 'static, impl Write + Send + 'static) {
    let (tx, rx) = mpsc::channel();
    (
        PipeR {
            rx,
            buf: Vec::new(),
            pos: 0,
        },
        PipeW(tx),
    )
}

/// 进程内假连接（测试用）：`pair()` 给两头，一头发的另一头收。
pub struct MemConn(StreamConn);

impl MemConn {
    pub fn pair() -> (MemConn, MemConn) {
        let (r1, w1) = mem_pipe();
        let (r2, w2) = mem_pipe();
        (
            MemConn(StreamConn::new(r1, w2)),
            MemConn(StreamConn::new(r2, w1)),
        )
    }
}

impl Conn for MemConn {
    fn send(&mut self, f: &Frame) -> io::Result<()> {
        self.0.send(f)
    }
    fn recv_timeout(&mut self, d: Duration) -> Recv {
        self.0.recv_timeout(d)
    }
    fn close(&mut self) {
        self.0.close()
    }
}

/// 子进程管道：起一个命令数组，接它的标准输入 / 输出；标准错误照转到本进程的标准错误。
pub struct ChildConn {
    inner: StreamConn,
    child: Child,
    exited: bool,
}

/// `close` 关了输入之后等它自己退的时限，过了就杀掉。
pub const CLOSE_GRACE: Duration = Duration::from_secs(5);

impl ChildConn {
    pub fn spawn(argv: &[String]) -> io::Result<ChildConn> {
        let (prog, args) = argv
            .split_first()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "命令数组是空的"))?;
        let mut child = Command::new(prog)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()?;
        let w = child.stdin.take().expect("piped");
        let r = child.stdout.take().expect("piped");
        Ok(ChildConn {
            inner: StreamConn::new(r, w),
            child,
            exited: false,
        })
    }

    /// 关输入、等它退，过 `grace` 还没退就杀掉、收掉。
    pub fn close_within(&mut self, grace: Duration) {
        self.inner.close();
        if self.exited {
            return;
        }
        let t0 = Instant::now();
        loop {
            match self.child.try_wait() {
                Ok(Some(_)) | Err(_) => break,
                Ok(None) if t0.elapsed() >= grace => {
                    let _ = self.child.kill();
                    let _ = self.child.wait();
                    break;
                }
                Ok(None) => std::thread::sleep(Duration::from_millis(20)),
            }
        }
        self.exited = true;
    }

    /// 子进程已经收掉了。
    pub fn exited(&self) -> bool {
        self.exited
    }

    pub fn pid(&self) -> u32 {
        self.child.id()
    }
}

impl Conn for ChildConn {
    fn send(&mut self, f: &Frame) -> io::Result<()> {
        self.inner.send(f)
    }
    fn recv_timeout(&mut self, d: Duration) -> Recv {
        self.inner.recv_timeout(d)
    }
    fn close(&mut self) {
        self.close_within(CLOSE_GRACE)
    }
}

impl Drop for ChildConn {
    fn drop(&mut self) {
        self.close_within(CLOSE_GRACE);
    }
}
