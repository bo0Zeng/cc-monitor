//! **读行**：一份会话记录从某个字节起一行行读到最后一个完整行（残尾不计）· 这一行算不算一行 · 一行怎么解析。
//! observe 里读记录的各处（冷开扫描图 · 尾段 · 按行号取 · 事实续算 · 会话内查找 · 实时跟读数行号）都经这里，口径只此一份。

use std::io::{BufRead, Take};

/// 一行行读：`r` 已定位在字节 `from`；[`Lines::upto`] 给了就读到那个字节为止（之后的不读）。
pub(crate) struct Lines<R> {
    r: Take<R>,
    end: u64,
    buf: Vec<u8>,
}

/// 读出来的一行：起点字节 ＋ 连换行在内的原文。
pub(crate) struct Row<'a> {
    pub(crate) at: u64,
    /// 连结尾 `\n` 在内（长度就是这一行占的字节数）。
    pub(crate) whole: &'a [u8],
}

impl Row<'_> {
    /// 不含结尾 `\n` 的那一截（`\r` 留着：算不算一行与解析都会去掉它）。
    pub(crate) fn body(&self) -> &[u8] {
        &self.whole[..self.whole.len() - 1]
    }
}

impl<R: BufRead> Lines<R> {
    pub(crate) fn new(r: R, from: u64) -> Self {
        Lines {
            r: r.take(u64::MAX),
            end: from,
            buf: Vec::new(),
        }
    }

    /// 读到字节 `upto` 为止（不含）。
    pub(crate) fn upto(mut self, upto: u64) -> Self {
        self.r.set_limit(upto.saturating_sub(self.end));
        self
    }

    /// 下一个完整行；读完了（或只剩半行）⇒ `None`，半行不计进 [`Self::end`]。
    pub(crate) fn next(&mut self) -> std::io::Result<Option<Row<'_>>> {
        self.buf.clear();
        let read = self.r.read_until(b'\n', &mut self.buf)?;
        if read == 0 || self.buf.last() != Some(&b'\n') {
            return Ok(None);
        }
        let at = self.end;
        self.end += read as u64;
        Ok(Some(Row {
            at,
            whole: &self.buf,
        }))
    }

    /// 最后一个完整行之后的字节位置（下一次接着读的起点）。
    pub(crate) fn end(&self) -> u64 {
        self.end
    }
}

/// 这一行**算不算一行**：BOM 与全空白不算（不占行号）。
///
/// 🔴 **全仓只有这一个住址** —— 实时跟读数 `seq`、冷开扫描图数 `total`，
/// 两边判得必须**一模一样**；口径一分家，计数就对不上，而那种错**不会报错**。
///
/// # 为什么先看字节（秤 7）
///
/// 原来这一跳对**每一行**做 `String::from_utf8_lossy(整行)` 再 `trim()`，
/// **只为回答「这行是不是空的」** —— 那是一次**整文件 UTF-8 校验**。
/// 实测（15.1 MiB 会话，laptop）：`tail` **19.52 → 3.79 ms**、其中「数行」那一趟
/// **17.89 → 2.98 ms（↓83%）**，**出字节一字节不差**。
/// 剩下的 ~3 ms/15.1 MiB（≈5 GiB/s ≈ `memchr`）才是说的
/// 那个**真·不可避免的 O(文件)**。
///
/// # 短路为什么是对的
///
/// - 任何 `< 0x80` 的字节都是**独立的 ASCII 字符** —— UTF-8 多字节序列的
///   首字节与续字节**全部 ≥ 0x80** ⇒ 不会把半个字符误读成 ASCII。
/// - BOM（`U+FEFF` ＝ `EF BB BF`）**三个字节全非 ASCII** ⇒ 短路条件碰不到它。
/// - ⇒ 只要有一个 ASCII 非空白字节，这行剥 BOM、`trim` 完**必然非空**。
/// - 短路**不成立**时退回原判定（保守），所以这不是近似，是**等价**。
///
/// ⚠⚠ **谓词必须是 `char::from(b).is_whitespace()`，不能写成 `b.is_ascii_whitespace()`**
/// —— 后者**不含 `U+000B`**（垂直制表），而 `str::trim` 含它。
/// 秤 7 的第一版变体就是这么写错的，**靠穷举才逮出来**（全部 1/2/3 字节共
/// 16 843 008 个 ＋ 300 万随机串，分歧 0）——
/// **单测没逮住，因为没有那个样本。**
pub(crate) fn counts(line: &[u8]) -> bool {
    // 快路：有 ASCII 非空白 ⇒ 必非空，不必解码
    if line
        .iter()
        .any(|b| b.is_ascii() && !char::from(*b).is_whitespace())
    {
        return true;
    }
    // 慢路：可能全是空白 / 多字节 ⇒ 按原判定来
    let text = String::from_utf8_lossy(line);
    !text.trim_start_matches('\u{feff}').trim().is_empty()
}

/// 一行记录解析成 `Value`（去 BOM 与两头空白，同适配层那几问吃的 [`crate::agents::Line`]）。解析不了 ⇒ `None`。
pub(crate) fn parse(line: &[u8]) -> Option<serde_json::Value> {
    crate::agents::Line::new(&String::from_utf8_lossy(line), 0).into_value()
}

/// 尾段那张图（`history-tail` 回包里那一块）＋ 最后一个可计行的起点（记号从它算到 `end`；一行都没有 ⇒ `None`）。
/// 现数一遍行（查询那一层）与扫描图上切（扫描图那一层）都出它。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Tail {
    pub(crate) plan: cmd_wire::session::TailPlan,
    pub(crate) last: Option<u64>,
}

#[cfg(test)]
#[path = "../../../tests/backend/observe/lines_tests.rs"]
mod tests;
