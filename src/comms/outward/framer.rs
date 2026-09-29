//! 增量分帧器 —— `relay/` 里**唯一**一份「攒字节、按分隔符切行」的实现〔`设计/17 §3.7`〕。
//!
//! # 🔴 通信层成员 `COMM-LAYER-MEMBER`〔SC1 · 第四波 4B〕
//!
//! 登记那一侧在 `tests/frontend/shell/comm_boundary_registry_tests.rs::REGISTERED`（两向集合相等）。
//! 它只认**字节与分隔符**，不认里面是什么 —— 那是 `设计/05 §2` 四样里的「载荷（不透明字节）」。
//! 这段代码先前住在成员 `http1.rs` 的 `ChunkedView` 里、受十一条管着；搬出来单住一份，
//! 不同拍盖标记就等于**出了锁**。
//!
//! # 它治的病
//!
//! 先前两处手抄（`tee.rs::SseSplitter` · `http1.rs::ChunkedView`）同一形：攒一个 `Vec`，
//! **每切一行就 `drain` 一次前缀**（每行搬一遍剩下的全部），并且**每次喂进来都从头找分隔符**。
//! 一条 617 KB 的记录卡在缓冲里时，后面每切一行都要搬 ~617 KB；它自己按块陆续到的时候，
//! 每块都把已经攒下的整截重扫一遍。两样都是 O(n²)。
//!
//! # 今天的形状：游标，不 drain
//!
//! - `start`：已消费到哪。取一行 = 借出切片 ＋ 挪游标，**不搬字节、不分配**。
//! - `scanned`：分隔符已经找到哪。下一次从这里接着找 ⇒ **每个字节最多被找一次**。
//! - `push` 时才把上一轮剩下的那截挪到头上（一次 `drain(..start)`；刚好收在行尾时是 `clear()`，零搬运）
//!   ⇒ 一个字节至多被搬一次：它被搬，说明它排在本轮最后一个分隔符之后；下一次 `push` 再搬它之前，
//!   必须有一个分隔符落在它后面 —— 而那一刻它已经被取走了。
//!
//! 判据（次数 / 长度的相等，不看墙钟）住 `tests/comms/outward/framer_tests.rs`。

/// 按一个分隔符增量切行。分隔符**以 `\n` 收尾**（`b"\n"` 或 `b"\r\n"`）：
/// 找的是 `\n`，找到之后再核它前面那几个字节 —— 这样跨两次 `push` 的 `\r` | `\n` 也认得出，
/// 而且每个字节只看一次（按「分隔符长度往回退几格再找」那种写法，跨块处会重看）。
pub(crate) struct LineFramer {
    buf: Vec<u8>,
    /// 已消费到哪（`buf[..start]` 是死字节，下一次 `push` 时一次挪走）。
    start: usize,
    /// 分隔符已经找到哪（`>= start`）。
    scanned: usize,
    /// 找到了、还没被取走的那一个分隔符的**末字节**下标。
    /// `head_len` 找到之后留在这里，`next_line` 直接用，不重找。
    hit: Option<usize>,
    delim: &'static [u8],
    /// 账。**生产路径不读它**，判据读（`framer_tests.rs`）。
    pub(super) ledger: Ledger,
}

/// 分帧器自己记的账：判据拿它做相等断言。
///
/// ⚠ **常开，不是 `#[cfg(test)]`**：测试专用支撑项在 `src/backend` 上有一道只许降的棘轮
/// （`structural_scan_tests.rs::the_split_stays_done_and_p9_is_blocked_for_a_reason_that_says_itself`），
/// 第一版按 `cfg(test)` 罩了四处，当场把那道棘轮顶破。代价是每次找 / 搬 / 喂各多一次整数加法。
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Ledger {
    /// 一共喂进来多少字节。
    pub(super) pushed: u64,
    /// 找分隔符时一共看过多少字节。
    pub(super) examined: u64,
    /// `push` 里挪「上一轮剩下的那截」一共搬了多少字节。
    pub(super) moved: u64,
}

impl LineFramer {
    pub(crate) fn new(delim: &'static [u8]) -> Self {
        debug_assert!(delim.last() == Some(&b'\n'), "分隔符必须以 \\n 收尾");
        Self {
            buf: Vec::new(),
            start: 0,
            scanned: 0,
            hit: None,
            delim,
            ledger: Ledger::default(),
        }
    }

    /// 喂一段字节。先挪走上一轮消费掉的前缀（至多一次搬运），再接上新字节。
    pub(crate) fn push(&mut self, bytes: &[u8]) {
        if self.start == self.buf.len() {
            self.buf.clear();
            self.scanned = 0;
            self.hit = None;
        } else if self.start > 0 {
            self.ledger.moved += (self.buf.len() - self.start) as u64;
            self.buf.drain(..self.start);
            self.scanned -= self.start;
            self.hit = self.hit.map(|h| h - self.start);
        }
        self.start = 0;
        self.ledger.pushed += bytes.len() as u64;
        self.buf.extend_from_slice(bytes);
    }

    /// 还没被消费的字节数。
    pub(crate) fn pending(&self) -> usize {
        self.buf.len() - self.start
    }

    /// 下一个分隔符末字节的下标；没有完整的行就 `None`。从 `scanned` 接着找，不回头。
    fn find(&mut self) -> Option<usize> {
        if self.hit.is_some() {
            return self.hit;
        }
        let lead = self.delim.len() - 1;
        while self.scanned < self.buf.len() {
            let from = self.scanned;
            let rel = self.buf[from..].iter().position(|b| *b == b'\n');
            let seen = rel.map_or(self.buf.len() - from, |r| r + 1);
            self.ledger.examined += seen as u64;
            self.scanned = from + seen;
            let Some(r) = rel else {
                break;
            };
            let at = from + r;
            if at - self.start >= lead && self.buf[at - lead..at] == self.delim[..lead] {
                self.hit = Some(at);
                return self.hit;
            }
        }
        None
    }

    /// 第一行（含分隔符）的长度；没有完整的行就是全部待处理字节。
    pub(crate) fn head_len(&mut self) -> usize {
        self.find().map_or(self.pending(), |at| at + 1 - self.start)
    }

    /// 取下一行（**不含**分隔符）。借出的是缓冲里的切片：不搬、不分配。
    pub(crate) fn next_line(&mut self) -> Option<&[u8]> {
        let at = self.find()?;
        let begin = self.start;
        self.start = at + 1;
        self.hit = None;
        Some(&self.buf[begin..at + 1 - self.delim.len()])
    }

    /// 原样取走至多 `n` 个待处理字节（不找分隔符）。
    pub(crate) fn take(&mut self, n: usize) -> &[u8] {
        let begin = self.start;
        self.skip(n);
        &self.buf[begin..self.start]
    }

    /// 丢掉至多 `n` 个待处理字节（不找分隔符）。
    pub(crate) fn skip(&mut self, n: usize) {
        self.start += n.min(self.pending());
        self.scanned = self.scanned.max(self.start);
        if self.hit.is_some_and(|h| h < self.start) {
            self.hit = None;
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/comms/outward/framer_tests.rs"]
mod tests;
