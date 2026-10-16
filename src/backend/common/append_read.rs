//! 只往后追加的文件（会话记录）的增量读，observe 与 history 共用（原 `observe/fs.rs`，10-13 下沉）。读盘本身经 `platform::fs::Ro`。

use crate::platform::fs::{Ro, RoFile};
use std::path::Path;

/// 增量读一份**只往后追加**的文件（会话记录）的那本账：上一次读完时的（stat 的修改时刻, 真读到的长度）·
/// `[0, consumed)` 已经交给调用方的完整行 · `consumed` 之前最多 [`WITNESS_BYTES`] 字节的见证。
/// 全文搜索的常驻索引与历史清单的那份缓存共用这一本（「没变不读 · 变长且见证对得上只读尾巴 · 其余整份」只此一处）。
#[derive(Default, Clone, serde::Serialize, serde::Deserialize)]
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
    /// 变长了、见证对得上：停在 `consumed` 处的块读者（[`Blocks`]，按块读到尾；读了多少由调用方读完记进账）。
    Appended(Blocks<RoFile>),
    /// 别的一切（第一次 · 变短 · 被改写 · 调用方说不许追加）：整份重读（调用方 [`Blocks::open`] 按块读，或逐行流式）。
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
        let meta = Ro::metadata(path).map_err(|e| e.to_string())?;
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

    /// 先只读见证核一下：对得上 ⇒ 停在 `consumed` 处的块读者；对不上 ⇒ `None`（被改写过；只白读了见证那几字节）。
    fn appended(&self, path: &Path, bytes: &mut u64) -> Result<Option<Blocks<RoFile>>, String> {
        use std::io::{Read, Seek, SeekFrom};
        let w = self.witness.len();
        let mut f = Ro::open(path).map_err(|e| e.to_string())?;
        let mut got = vec![0u8; w];
        f.seek(SeekFrom::Start(self.consumed - w as u64))
            .and_then(|_| f.read_exact(&mut got))
            .map_err(|e| e.to_string())?;
        *bytes += w as u64;
        Ok((got == self.witness).then(|| Blocks::new(f, READ_BLOCK)))
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

/// 整份 / 一大截读的时候一次读多少字节（一行比它长就接着读到那一行的行尾）。
pub(crate) const READ_BLOCK: usize = 1 << 20;

/// 按块读到尾：每块止于一个 `\n`（一行比块长就接着读到它的行尾 —— 一行要整行解析，躲不掉）；读到尾时没有 `\n` 的那一截
/// 由 [`Self::rest`] 交出。读的时候手里最多 ≈ 一块 ＋ 正在读的那一行，交出去的那块调用方用完就放。
/// 记录的整份读（搜索索引 · 清单缓存的追加读 · 续读的从 0 重读 · 子运行记录）都走它，不再一次读进一个 `Vec`。
pub(crate) struct Blocks<R> {
    src: R,
    block: usize,
    carry: Vec<u8>,
    read: u64,
    done: bool,
}

impl Blocks<RoFile> {
    /// 从头读一份文件。
    pub(crate) fn open(path: &Path) -> Result<Self, String> {
        Ro::open(path)
            .map(|f| Self::new(f, READ_BLOCK))
            .map_err(|e| e.to_string())
    }
}

impl<R: std::io::Read> Blocks<R> {
    /// 从 `src` 此刻的位置读起，一次读 `block` 字节。
    pub(crate) fn new(src: R, block: usize) -> Self {
        Self {
            src,
            block: block.max(1),
            carry: Vec::new(),
            read: 0,
            done: false,
        }
    }

    /// 下一块整行（止于 `\n`）；读到尾 ⇒ `Ok(None)`（没写完的那截留给 [`Self::rest`]）；读出错 ⇒ `Err`，之后不再有块。
    pub(crate) fn next(&mut self) -> std::io::Result<Option<Vec<u8>>> {
        while !self.done {
            let at = self.carry.len();
            self.carry.reserve_exact(self.block);
            self.carry.resize(at + self.block, 0);
            let got = self.src.read(&mut self.carry[at..]);
            self.carry.truncate(at + *got.as_ref().unwrap_or(&0));
            let n = match got {
                Ok(0) => {
                    self.done = true;
                    break;
                }
                Ok(n) => n,
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(e) => {
                    self.done = true;
                    return Err(e);
                }
            };
            self.read += n as u64;
            // 只在新读进来的那几字节里找（长行不反复从头扫）。
            if let Some(k) = self.carry[at..].iter().rposition(|&b| b == b'\n') {
                let rest = self.carry[at + k + 1..].to_vec();
                self.carry.truncate(at + k + 1);
                return Ok(Some(std::mem::replace(&mut self.carry, rest)));
            }
        }
        Ok(None)
    }

    /// 读到尾之后没有 `\n` 的那一截（可能为空）。
    pub(crate) fn rest(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.carry)
    }

    /// 到此刻读了多少字节。
    pub(crate) fn read(&self) -> u64 {
        self.read
    }
}

// d（整份读按块）几份判据共用的夹具与量法住测试树。
#[cfg(test)]
#[path = "../../../tests/backend/common/streamed_read_support.rs"]
pub(crate) mod test_support;

#[cfg(test)]
#[path = "../../../tests/backend/common/streamed_read_tests.rs"]
mod tests;
