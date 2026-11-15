//! 一行一帧：写一帧 ＝ 一个 JSON 对象 ＋ 换行；读一行解一帧。

use crate::frame::Frame;
use std::fmt;
use std::io::{self, BufRead, BufReader, Read, Write};

/// 一行最长多少字节（超了这一行整行丢掉、报 `TooLong`）。
pub const MAX_LINE: usize = 8 << 20;

#[derive(Debug)]
pub enum WireError {
    Io(io::Error),
    /// 不是一个认得出的帧：不是 JSON 对象、没有 `t`、键或类型不对
    Bad {
        line: String,
        why: String,
    },
    /// `t` 是认不得的帧名
    Unknown {
        tag: String,
    },
    TooLong,
}

impl fmt::Display for WireError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WireError::Io(e) => write!(f, "读连接出错：{e}"),
            WireError::Bad { line, why } => write!(f, "坏行（{why}）：{line}"),
            WireError::Unknown { tag } => write!(f, "认不得的帧：{tag}"),
            WireError::TooLong => write!(f, "一行超过 {MAX_LINE} 字节"),
        }
    }
}

impl std::error::Error for WireError {}

/// 一帧的文字（不带换行）。
pub fn encode(f: &Frame) -> String {
    serde_json::to_string(f).expect("帧总序列化得了")
}

/// 解一行。
pub fn decode(line: &str) -> Result<Frame, WireError> {
    let bad = |why: String| WireError::Bad {
        line: clip(line),
        why,
    };
    let v: serde_json::Value = serde_json::from_str(line).map_err(|e| bad(e.to_string()))?;
    let tag = match v.get("t").and_then(|t| t.as_str()) {
        Some(t) if v.is_object() => t.to_string(),
        _ => return Err(bad("不是带 t 的对象".into())),
    };
    if !Frame::NAMES.contains(&tag.as_str()) {
        return Err(WireError::Unknown { tag });
    }
    serde_json::from_value(v).map_err(|e| bad(e.to_string()))
}

fn clip(s: &str) -> String {
    let mut end = s.len().min(200);
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    s[..end].to_string()
}

/// 写一帧并 flush。
pub fn write_frame<W: Write + ?Sized>(w: &mut W, f: &Frame) -> io::Result<()> {
    let mut s = encode(f);
    s.push('\n');
    w.write_all(s.as_bytes())?;
    w.flush()
}

/// 从一条字节流里一帧一帧读。
pub struct FrameReader<R> {
    r: BufReader<R>,
}

impl<R: Read> FrameReader<R> {
    pub fn new(r: R) -> Self {
        FrameReader {
            r: BufReader::new(r),
        }
    }

    /// 读一帧；读到结尾 ⇒ `Ok(None)`。坏的那一行报错，下一次接着读下一行。
    pub fn read_frame(&mut self) -> Result<Option<Frame>, WireError> {
        let mut buf = Vec::new();
        let n = (&mut self.r)
            .take(MAX_LINE as u64 + 1)
            .read_until(b'\n', &mut buf)
            .map_err(WireError::Io)?;
        if n == 0 {
            return Ok(None);
        }
        if buf.last() != Some(&b'\n') && buf.len() > MAX_LINE {
            // 丢掉这一行剩下的
            loop {
                let chunk = self.r.fill_buf().map_err(WireError::Io)?;
                if chunk.is_empty() {
                    break;
                }
                if let Some(i) = chunk.iter().position(|b| *b == b'\n') {
                    self.r.consume(i + 1);
                    break;
                }
                let l = chunk.len();
                self.r.consume(l);
            }
            return Err(WireError::TooLong);
        }
        while matches!(buf.last(), Some(b'\n' | b'\r')) {
            buf.pop();
        }
        let line = String::from_utf8_lossy(&buf);
        decode(&line).map(Some)
    }
}
