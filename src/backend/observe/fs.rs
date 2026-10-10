//! observe 内部共享的文件元信息读取。

use std::path::Path;
use std::time::UNIX_EPOCH;

/// 文件 mtime 的毫秒时间戳；任何读取失败都退化成 `0`（排序时沉到最旧：读失败与「1970 年的文件」混成同一个值，要改得连同两个调用点的排序语义一起想）。
/// 住 observe：两个调用点（`history_query` / `search_query`）同属 observe，不够 `common/` 的「≥2 个上层用」门槛。
pub(crate) fn mtime_ms(p: &Path) -> i64 {
    std::fs::metadata(p)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// 增量读一份**只往后追加**的文件（会话记录）的那本账：上一次读完时的（stat 的修改时刻, 真读到的长度）·
/// `[0, consumed)` 已经交给调用方的完整行 · `consumed` 之前最多 [`WITNESS_BYTES`] 字节的见证。
/// 全文搜索的常驻索引与历史清单的那份缓存共用这一本（「没变不读 · 变长且见证对得上只读尾巴 · 其余整份」只此一处）。
#[derive(Default)]
pub(crate) struct Tailed {
    seen: (Option<std::time::SystemTime>, u64),
    consumed: u64,
    witness: Vec<u8>,
}

/// 追加读之前核的那一段：`consumed` 之前最多这么多字节；对不上 ⇒ 被改写过、整份重读。
pub(crate) const WITNESS_BYTES: usize = 256;

/// [`Tailed::step`] 看完之后该怎么办。
pub(crate) enum Step {
    /// （修改时刻, 长度）都没变：不读。
    Same,
    /// 变长了、见证对得上：`consumed` 之后新读到的字节（可能以半行结尾）。
    Appended(Vec<u8>),
    /// 别的一切（第一次 · 变短 · 被改写 · 调用方说不许追加）：整份重读（怎么读由调用方定：一次读进来 / 逐行流式）。
    Whole,
}

impl Tailed {
    /// 看一眼这一份，回该怎么办与这一次 stat 到的（修改时刻, 长度）。`prev` ＝ 上一次那本账（没有 ⇒ 整份）；
    /// `may_append` 为假 ⇒ 变了就整份（调用方另有理由）。
    /// `bytes` 累加这一趟真读的字节（追加读见证没对上、再整份读的，两趟都算）。打不开 / 读不了 ⇒ `Err(原因)`。
    pub(crate) fn step(
        prev: Option<&Self>,
        path: &Path,
        may_append: bool,
        bytes: &mut u64,
    ) -> Result<(Step, (Option<std::time::SystemTime>, u64)), String> {
        let meta = std::fs::metadata(path).map_err(|e| e.to_string())?;
        let seen = (meta.modified().ok(), meta.len());
        if let Some(prev) = prev {
            if prev.seen == seen {
                return Ok((Step::Same, seen));
            }
            if may_append && seen.1 > prev.seen.1 {
                if let Some(new) = prev.appended(path, bytes)? {
                    return Ok((Step::Appended(new), seen));
                }
            }
        }
        Ok((Step::Whole, seen))
    }

    /// 整份读进来（[`Step::Whole`] 时一次读完的那一形用）。`bytes` 同 [`Self::step`]。
    pub(crate) fn read_whole(path: &Path, bytes: &mut u64) -> Result<Vec<u8>, String> {
        use std::io::Read;
        let mut buf = Vec::new();
        std::fs::File::open(path)
            .and_then(|mut f| f.read_to_end(&mut buf))
            .map_err(|e| e.to_string())?;
        *bytes += buf.len() as u64;
        Ok(buf)
    }

    /// 从见证起读到尾：见证对得上 ⇒ `consumed` 之后的新字节；对不上 ⇒ `None`（被改写过）。
    fn appended(&self, path: &Path, bytes: &mut u64) -> Result<Option<Vec<u8>>, String> {
        use std::io::{Read, Seek, SeekFrom};
        let w = self.witness.len() as u64;
        let mut f = std::fs::File::open(path).map_err(|e| e.to_string())?;
        let mut buf = Vec::new();
        f.seek(SeekFrom::Start(self.consumed - w))
            .and_then(|_| f.read_to_end(&mut buf))
            .map_err(|e| e.to_string())?;
        *bytes += buf.len() as u64;
        if buf.len() as u64 >= w && buf[..w as usize] == self.witness[..] {
            buf.drain(..w as usize);
            return Ok(Some(buf));
        }
        Ok(None)
    }

    /// 收下一段完整行（推进 `consumed`、换见证；可以一行一行地交，见证只留最后那 [`WITNESS_BYTES`] 字节，不复制整段）。
    pub(crate) fn took(&mut self, whole: &[u8]) {
        self.consumed += whole.len() as u64;
        if whole.len() >= WITNESS_BYTES {
            self.witness.clear();
            self.witness
                .extend_from_slice(&whole[whole.len() - WITNESS_BYTES..]);
        } else {
            self.witness.extend_from_slice(whole);
            let over = self.witness.len().saturating_sub(WITNESS_BYTES);
            self.witness.drain(..over);
        }
    }

    /// 记下这一次：（stat 到的修改时刻, 真读到的末尾）。
    pub(crate) fn saw(&mut self, seen: (Option<std::time::SystemTime>, u64)) {
        self.seen = seen;
    }

    /// 已经交给调用方的完整行到哪个偏移。
    pub(crate) fn consumed(&self) -> u64 {
        self.consumed
    }

    /// 见证占的字节（常驻估算用）。
    pub(crate) fn witness_len(&self) -> usize {
        self.witness.len()
    }
}

/// 一段新读到的字节切成「最后一个 `\n` 之前的完整行」与「之后的半行」（切点紧跟 `\n`，两段各自是不是合法 UTF-8 与整份同一个答案）。
pub(crate) fn split_whole_lines(new: &[u8]) -> (&[u8], &[u8]) {
    let cut = new.iter().rposition(|&b| b == b'\n').map_or(0, |k| k + 1);
    new.split_at(cut)
}
