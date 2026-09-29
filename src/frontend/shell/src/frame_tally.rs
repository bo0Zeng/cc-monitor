//! 〔W5-VIS · `设计/15 §3.4 ②`〕**一条帧流上丢了多少** —— 三条读帧循环共用的一本小账。
//!
//! `设计/15 §3.4 ②` 逐字：「两个真实的静默口：`from_utf8_lossy` 在读路径上（非 UTF-8 字节静默变 U+FFFD）·
//! 坏帧/未知 kind → warn ＋ 跳过、永不中断流（设计如此，但「今天丢了多少帧」没有计数器；W5-VIS）」。
//!
//! 「跳过、永不中断流」这条设计**不动**；本模块只补「丢了多少」：
//! - 认不出 / 坏帧跳过的 ⇒ [`FrameTally::note_unparsed`]；
//! - 这一行不是合法 UTF-8 的（远端那条按替换字符读、本机脱离载体那条整行丢）⇒ [`FrameTally::note_bad_utf8`]。
//!
//! 说话的节奏：每种第 1、2、4、8 … 次（2 的幂）说一行（带累计数与那一行的开头），其余只数 ——
//! 一条活几个小时的流上**看得见在丢**，又不会一帧一行把日志刷满（`ssh_source::parse_frame` 那段
//! `turn_end` 注释写过逐帧刷 warn 的代价：噪声，且真正的坏帧淹没在里面）。
//! 流结束（这本账被丢掉）时 [`Drop`] 出一行总账；两样都是 0 就不说。
//!
//! 三个读帧的地方：远端 `ssh_source::stream_loop`（读任务数非 UTF-8、主循环数认不出）·
//! 本机脱离载体 `local_backend_host::attach_stream` · 本机 stdio 载体 `backend::control::local_backend` 的 stdout 读循环。
//! 接线由 `tests/frontend/shell/frame_tally_tests.rs` 的判据按文件逐一钉住。

/// 一行最多带进日志多少字节（那一行的开头，截在字符边界上）。
const SAMPLE_BYTES: usize = 200;

/// 一条帧流的丢帧账（一条连接一本；读任务与主循环各拿一本也行，各记各的那一样）。
#[derive(Debug)]
pub(crate) struct FrameTally {
    /// 这条流叫什么（日志里认得出是哪台 / 哪条载体）。
    who: String,
    /// 认不出 / 坏帧、跳过的帧数。
    unparsed: u64,
    /// 不是合法 UTF-8 的行数。
    bad_utf8: u64,
}

impl FrameTally {
    pub(crate) fn new(who: impl Into<String>) -> Self {
        FrameTally {
            who: who.into(),
            unparsed: 0,
            bad_utf8: 0,
        }
    }

    /// 一帧认不出（未知 kind / 坏帧 / 不是 JSON），跳过。回一句要打进日志的话（2 的幂次才有）。
    pub(crate) fn note_unparsed(&mut self, line: &str) -> Option<String> {
        self.unparsed += 1;
        self.unparsed.is_power_of_two().then(|| {
            format!(
                "[{}] 跳过一帧认不出的（未知 kind / 坏帧），这条流上累计 {} 帧；这一帧开头：{}",
                self.who,
                self.unparsed,
                sample(line)
            )
        })
    }

    /// 一行不是合法 UTF-8。回一句要打进日志的话（2 的幂次才有）。
    pub(crate) fn note_bad_utf8(&mut self, bytes: &[u8]) -> Option<String> {
        self.bad_utf8 += 1;
        self.bad_utf8.is_power_of_two().then(|| {
            format!(
                "[{}] 一行含非 UTF-8 字节（这条流上累计 {} 行）；这一行开头：{}",
                self.who,
                self.bad_utf8,
                sample(&String::from_utf8_lossy(bytes))
            )
        })
    }

    /// 总账：两样都是 0 ⇒ `None`。
    pub(crate) fn summary(&self) -> Option<String> {
        (self.unparsed > 0 || self.bad_utf8 > 0).then(|| {
            format!(
                "[{}] 这条流结束：跳过了 {} 帧认不出的，{} 行含非 UTF-8 字节",
                self.who, self.unparsed, self.bad_utf8
            )
        })
    }
}

impl Drop for FrameTally {
    /// 流结束（这本账被丢掉）⇒ 出一行总账。
    fn drop(&mut self) {
        if let Some(s) = self.summary() {
            tracing::warn!("{s}");
        }
    }
}

/// 那一行的开头（至多 [`SAMPLE_BYTES`] 字节，截在字符边界上）。
fn sample(line: &str) -> &str {
    if line.len() <= SAMPLE_BYTES {
        return line;
    }
    let mut end = SAMPLE_BYTES;
    while !line.is_char_boundary(end) {
        end -= 1;
    }
    &line[..end]
}

#[cfg(test)]
#[path = "../../../../tests/frontend/shell/frame_tally_tests.rs"]
mod tests;
