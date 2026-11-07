//! **有上限读**的唯一一份：一道字节上限 [`Cap`]（多少字节 · 超了怎么办 [`Over`]）与同步整读 —— 读到流尽 [`read_capped_from`] ·
//! 读一个窗口 [`read_window`] · 读一份本机文件 [`read_capped_regular`]。后端 · 壳 · 文件窗口 · comms · 手机核心同步地整读字节只经这一族
//! （异步那一形住 `comms_inward::line`，结果同一个类型 [`CappedAll`]）。
//!
//! 零依赖、自成一包：后端落盘的声明表（`relay-route-core`）每一行带 `Cap`、文案表（`copy-core`）的 `RAW_CAP` 也是 `Cap`，
//! 平台原语（`platform-core`）又依赖文案表 —— 住进其中哪一包都会成环。

/// 一道字节上限（读一份 · 收一帧 · 攒一截 · 留一份在内存里）：多少字节，超了怎么办。数和处置写在一处，声明的地方就说清越界那一刻发生什么。
/// 量的不是字节的上限（条数 · 字符数 · 毫秒）不用它。
///
/// 没有「静默截断」那一档：截了不说，拿半份当整份用，正是上限要挡的。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cap {
    pub bytes: u64,
    pub over: Over,
}

impl Cap {
    /// 上限的字节数作 `usize`，给拿长度（`len()`）比的调用处用；大过 `usize` 的（32 位上）当 `usize::MAX`。
    pub const fn as_len(self) -> usize {
        if self.bytes > usize::MAX as u64 {
            usize::MAX
        } else {
            self.bytes as usize
        }
    }
}

/// 超了上限怎么办。每一档都让人知道越界了；分界写在各档上。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Over {
    /// 硬报错：这一趟整个失败，错误照实往上交。
    HardError,
    /// 截断＋说清：只留上限以内的那一段，结果里带着「截了」那一格，读的人看得见。
    TruncateAndSay,
    /// 拒收＋回错：一个字节不收、不落、不覆盖，回一句说清超了。自有状态读不出来也是这一档：读面照实回错，写面不覆盖。
    Reject,
    /// 索引截断（不丢数据）：截掉的是可搜性，不是数据 —— 原文一个字节没动，只是超过封顶的那段搜不到。
    /// 与「静默截断」的分界：有没有丢掉用户的东西。
    IndexTrim,
    /// 跳过＋说清：超了的那一份跳过（缓存当没有、照别的东西重建），`warn!` 带上是哪一份。
    /// 跳过本身不是问题，跳过而不说是谁才是：没有 `warn!` 的跳过不许落在这一档。
    SkipAndSay,
    /// 降级＋说清：整份结果降级返回，错误带在结果里出去（不是跳过一条，降的是整份），看得见。
    DegradeAndSay,
    /// 丢弃＋带身份报告：丢掉越界的那一条，往用户那一侧发一个带身份的类型化事件（原位 `Gap`）。
    /// 与「跳过＋说清」的分界是谁被告知：那一档告日志，这一档告用户。跑在没有调用方可回错的后台流里用它。
    DropAndReport,
    /// 分块（不丢数据）：装不进一份的不拒、不截、不丢，改走分块逐块送、读回拼起来 —— 用户的字节一个不少。
    /// 与「拒收＋回错」的分界：越界的那一份照样存成了。
    Chunk,
}

/// 一次带上限整读的结果（同步 [`read_capped_from`] 与 `comms_inward::line` 的异步那一形同一个类型）。
#[derive(Debug, PartialEq, Eq)]
pub enum CappedAll {
    /// 流尽，一共不超过 `cap.bytes` 字节。
    All(Vec<u8>),
    /// 超过 `cap.bytes`：读到第 `cap.bytes + 1` 个字节就停，读进来的整份丢掉。
    OverCap,
}

/// 同步地读到流尽，至多 `cap.bytes` 字节；多读一个字节分得出「正好到上限」与「超了」。超了回 [`CappedAll::OverCap`]、不截断，
/// 超了之后怎么办是调用方照 `cap.over` 办的事。上限在读的时候生效：超了就停，不按流（或文件）的长度分配。
/// 要接着把流读空（管子另一头不读完会卡在写上）⇒ 交 `&mut r`，之后自己读进 `std::io::sink()`。
pub fn read_capped_from<R: std::io::Read>(rd: R, cap: Cap) -> std::io::Result<CappedAll> {
    let out = fill(rd, cap.bytes.saturating_add(1))?;
    Ok(if out.len() as u64 > cap.bytes {
        CappedAll::OverCap
    } else {
        CappedAll::All(out)
    })
}

/// 读一个窗口：至多 `n` 字节，读到 `n` 就停（后面的留在流里），流尽得早就交到流尽为止。不判超：窗口的宽就是调用方要的那一段
/// （读一块 · 读一页 · 读尾巴），后面还有字节是常态，不是越界。
pub fn read_window<R: std::io::Read>(rd: R, n: u64) -> std::io::Result<Vec<u8>> {
    fill(rd, n)
}

/// 同步整读的读循环（只这一份）：读到流尽或满 `limit` 字节为止。只用一次次的 `read`，被信号打断就再读。
fn fill<R: std::io::Read>(mut rd: R, limit: u64) -> std::io::Result<Vec<u8>> {
    let mut out = Vec::new();
    let mut chunk = [0u8; 8 * 1024];
    loop {
        let room = limit.saturating_sub(out.len() as u64);
        if room == 0 {
            return Ok(out);
        }
        let want = chunk.len().min(usize::try_from(room).unwrap_or(usize::MAX));
        let n = match rd.read(&mut chunk[..want]) {
            Ok(n) => n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        };
        if n == 0 {
            return Ok(out);
        }
        out.extend_from_slice(&chunk[..n]);
    }
}

/// 带上限整读一份本机文件的结局（[`read_capped_regular`]）。
#[derive(Debug, PartialEq, Eq)]
pub enum Regular {
    /// 整份（不超过上限）。
    Whole(Vec<u8>),
    /// 不是普通文件（目录 · FIFO · 设备 · socket；不跟链接那一形下还有链接本身）：没打开。
    NotRegular,
    /// 超过上限：打开之前看长度就超了 ⇒ 带上实际大小（没打开）；读的途中长过上限 ⇒ 不带。
    OverCap(Option<u64>),
}

/// 带上限整读一份本机文件：先看是不是普通文件（打开一个 FIFO 会卡住等写端，设备的长度报 0 会骗过大小检查）· 再看长度
/// （拒一份巨型文件之前不先读 `cap.bytes + 1` 进内存）· 再打开、经 [`read_capped_from`] 读（看长度与读之间文件还会长，两道一起才完整）。
/// 跟不跟链接由调用方定：`meta` 是它取的元数据（跟 ⇒ `metadata` · 不跟 ⇒ `symlink_metadata`），`open` 是对应的打开法。
pub fn read_capped_regular<F: std::io::Read>(
    meta: &std::fs::Metadata,
    open: impl FnOnce() -> std::io::Result<F>,
    cap: Cap,
) -> std::io::Result<Regular> {
    if !meta.is_file() {
        return Ok(Regular::NotRegular);
    }
    if meta.len() > cap.bytes {
        return Ok(Regular::OverCap(Some(meta.len())));
    }
    Ok(match read_capped_from(open()?, cap)? {
        CappedAll::All(b) => Regular::Whole(b),
        CappedAll::OverCap => Regular::OverCap(None),
    })
}

#[cfg(test)]
#[path = "../../../../tests/common/cap-core/lib_tests.rs"]
mod tests;

// 测试档的家一开始就是沙箱（`src/common/test-home`）：这一行让链接器把它的启动段带进本 crate 的测试二进制。
#[cfg(test)]
#[used]
static TEST_HOME: fn() = test_home::linked;
