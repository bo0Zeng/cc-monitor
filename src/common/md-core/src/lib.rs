//! 助手正文的 Markdown：[`parse`] 把一段原文变成 [`Doc`]（块树 ＋ 长正文的段界）；代码高亮看见才算，
//! 由 [`Highlighter::seg`] 一段一段地算（按内容缓存）。
//!
//! 只有这一处解析；两端（桌面 · 手机）只照树排版，不再各自解析、各自定规矩。这里一处定的规矩：
//! - 删除线只认 `~~`：单 `~` 拆回字（按源码位置看定界符长度）。
//! - 原样 HTML 不进树：行内 · 块级都当字面字交，只有 `<br>` 认成硬换行。
//! - 链接只放行 `http` · `https` · `mailto`，图只放行 `http` · `https`；别的交成它的字。
//! - 数学交 TeX 原文；段落里独占的 `$$…$$` · `\[…\]` 提成块级公式。
//! - 软换行：两边都是汉字（东亚宽字，不含谚文）⇒ 交空串，否则交一个空格。
//! - 代码块语言：信息串第一个词，小写，常见别名归一；认不出的不猜、不高亮。
//!
//! 分段见 [`segment`]：按顶层块累计字数，到 [`SEG_BYTES`] 就在下一个顶层块前记一个段起点；[`seg_blocks`] 取一段。
//! 超长代码切开的几块各自高亮（语法状态从每块的头一行起）。

pub use cmd_wire::md;

mod highlight;
mod parse;
mod segment;

pub use highlight::Highlighter;
pub use md::*;
pub use parse::parse;
pub use segment::{seg_blocks, segment, SEG_BYTES};

#[cfg(test)]
#[path = "../../../../tests/common/md-core/md_core_tests.rs"]
mod tests;

// 测试档的家一开始就是沙箱（`src/common/test-home`）：这一行让链接器把它的启动段带进本包的测试二进制。
#[cfg(test)]
#[used]
static TEST_HOME: fn() = test_home::linked;
