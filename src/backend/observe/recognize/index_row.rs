//! 骨架索引的那一行怎么认（`history-index` 的原料）：给一行，出它的字节区间 · 记录类 · 正文几个数 · 来源 · 用户输入摘要。

/// 骨架索引的一行：**位置 ＋ 身份 ＋ 宽度无关料**（第一格 · `§2.5b 路 D`）。
///
/// 键名刻意短（每条记录一行，大会话上万行）；**缺省即零 / 假**，零值不序列化。
///
/// # 宽度无关料是什么、**不是**什么
///
/// 是「这条记录渲染出来**会有多少东西**」的计数 —— 与列宽无关，所以用户拉窗口不会让它失效；
/// 前端拿它乘以当前列宽下的字宽/行高做**第一级粗估**（`height-estimate.ts::estimateFromFacts`）。
/// **不是**高度：高度依赖列宽，后端不知道列宽（`§2.5b` 那条「决定性的理由」）。
///
/// ⚠ **口径是近似的，而且近似得有方向**：它不知道前端哪些记录最终不建卡（`stripInternalNoise`、
/// ESC 折叠…），只按记录的**原料**数。前端据 `dr` / `t` / `fd` 分档，
/// 分不准的那几档落到偏保守的常数（`§2.5b`：「粗估用偏保守的常数，精算后往下修」）。
#[derive(Debug, Default, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct IndexRow {
    /// 行起点字节偏移（绝对，0-based）—— 按需取正文就是 `[o, o+n)`。
    pub o: u64,
    /// 行字节长（**含**结尾 `\n`）。
    pub n: u64,
    /// 这一行翻成的那条通用记录的类（`said` · `reply` · `retry` · `title` · `unread`，与读正文时那条记录的 `t` 同一个词）；
    /// 不进界面（元数据 · 解析不出）⇒ 省略。取自整条翻译出的那条记录。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub t: Option<crate::agents::record::RecordClass>,
    /// `uuid`（帧面不逐行运它：只在用户输入那张小表里带；扫描图按它认一条记录在第几轮）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub u: Option<String>,
    /// 这条记录属于某个子运行（适配层 `RecordFace::run_of` 答得出）。
    #[serde(skip_serializing_if = "is_false")]
    pub sc: bool,
    /// 画成哪一形（就是那条通用记录的 `draw`）；不画 ⇒ 省略。
    #[serde(skip_serializing_if = "undrawn")]
    pub dr: crate::agents::record::Draw,
    /// 人在这条里说了话（就是那条通用记录的 `human`）。
    #[serde(skip_serializing_if = "is_false")]
    pub hu: bool,
    /// 正文字符数（**代码块之外**；Unicode 标量计，不含换行）。
    #[serde(skip_serializing_if = "is_zero")]
    pub ch: u32,
    /// 其中 CJK/全宽字符数（口径 = `height-estimate.ts::factLines` 的码点 `> 0x2E80`）。
    #[serde(skip_serializing_if = "is_zero")]
    pub cj: u32,
    /// 正文**非空**硬行数（代码块之外）。
    #[serde(skip_serializing_if = "is_zero")]
    pub pl: u32,
    /// 围栏代码块数（```` ``` ```` / `~~~` 开合一对算一个；没合上的算到文末）。
    #[serde(skip_serializing_if = "is_zero")]
    pub cb: u32,
    /// 代码块内总行数。
    #[serde(skip_serializing_if = "is_zero")]
    pub cl: u32,
    /// 折叠单元数：`tool_use` / `tool_result` / `thinking` / `redacted_thinking` / `image` 块。
    /// 它们各渲染成一行 summary（纯工具的记录画成一张「几步」卡，每步一行），与正文长短无关。
    #[serde(skip_serializing_if = "is_zero")]
    pub fd: u32,
    /// 这一行是一条**用户输入**（大纲的一项）⇒ 它的摘要；不是 ⇒ 省略。
    ///
    /// 判定只住 [`crate::observe::recognize::user_inputs::user_input_of`]（与大纲清单 **同一个函数**），
    /// 摘要同 `excerpt`、uuid 就是本行的 `u`。有了它，首屏的「索引」与「大纲清单」合成一趟读：
    /// 前端见到索引里**有** `x` ⇒ 对面是会出它的后端、每一条用户输入都带着 ⇒ 不再单独要清单；
    /// 一个 `x` 都没有 ⇒ 分不清「老后端」还是「真的零条」⇒ 照旧要一份（形状登记 `IPC-PROTOCOL.md §10.3`）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub x: Option<String>,
    /// 同上那一行的 `timestamp`（空串 ⇒ 省略；清单那边的空串 == 这里缺席）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ts: Option<String>,
}

fn is_false(b: &bool) -> bool {
    !*b
}
fn undrawn(d: &crate::agents::record::Draw) -> bool {
    *d == crate::agents::record::Draw::None
}
fn is_zero(n: &u32) -> bool {
    *n == 0
}

/// 一行（已经解析过；解析不了 ⇒ `None`，那一行照样占位 —— 丢了它，后面每一行的 seq 都会错一位）→ [`IndexRow`]。**纯函数**，不碰文件系统。
/// `kind` ＝ 这份记录是哪一家的（[`crate::agents::record_kind_of`]）；`rec` ＝ 这一行整条翻译出的通用记录（不进界面 ⇒ `None`）：
/// 类 · 画法 · 人说没说都取自它，不另判。
pub(crate) fn index_row_of(
    kind: &str,
    v: Option<&serde_json::Value>,
    beat: Option<&crate::agents::beat::Beat>,
    rec: Option<&crate::agents::record::Record>,
    offset: u64,
    len: u64,
) -> IndexRow {
    use crate::agents::record::BodyExt as _;
    let mut row = IndexRow {
        o: offset,
        n: len,
        ..IndexRow::default()
    };
    let Some(v) = v else {
        return row;
    };
    row.u = beat.and_then(|b| b.id.as_deref()).map(str::to_string);
    if let Some(r) = rec {
        row.t = Some(r.body.class());
        row.dr = r.draw;
        row.hu = r.human;
    }
    row.sc = crate::agents::run_of_record(kind, v).is_some();
    // 大纲那一项（判定只住 `user_inputs`；这里只搬字段）
    if let Some(ui) =
        beat.and_then(|b| crate::observe::recognize::user_inputs::user_input_given(kind, v, b))
    {
        row.x = Some(ui.excerpt);
        row.ts = Some(ui.timestamp).filter(|t| !t.is_empty());
    }
    // 正文的形状：那一家有文本面 ⇒ 问它（`agents::body_shape`，正文在哪 · 哪种块算折叠单元是它的格式知识）；
    // 没有 ⇒ 数一拍里的件：正文 ⇒ 字数行数，调用 · 结果 · 思考 · 图 ⇒ 各一个折叠单元（与文本面同一个数法）。
    match crate::agents::body_shape(kind, v) {
        Some(shape) => {
            for s in shape.prose {
                count_prose(s, &mut row);
            }
            row.fd += shape.folded;
        }
        None => {
            use crate::agents::beat::Act;
            for a in beat.map_or(&[][..], |b| &b.acts) {
                match a {
                    Act::Text(t) => count_prose(t, &mut row),
                    Act::Call { .. } | Act::Result { .. } | Act::Thinking | Act::Image => {
                        row.fd += 1
                    }
                    _ => {}
                }
            }
        }
    }
    row
}

/// 数一段 markdown 正文：围栏代码块内外分开数（代码走等宽行高、正文走折行估计，两者算法不同）。
fn count_prose(s: &str, row: &mut IndexRow) {
    let mut in_code = false;
    for line in s.split('\n') {
        let lead = line.trim_start_matches(' ');
        // 围栏：行首至多 3 个空格后 ``` 或 ~~~（CommonMark 口径的近似；不区分开合的字符种类）
        if line.len() - lead.len() <= 3 && (lead.starts_with("```") || lead.starts_with("~~~")) {
            if !in_code {
                row.cb += 1;
            }
            in_code = !in_code;
            continue;
        }
        if in_code {
            row.cl += 1;
            continue;
        }
        let l = line.trim_end_matches('\r');
        if l.trim().is_empty() {
            continue;
        }
        row.pl += 1;
        for c in l.chars() {
            row.ch += 1;
            if c as u32 > 0x2e80 {
                row.cj += 1;
            }
        }
    }
}

// 从原文起的那一行（`test_support::index_row`）：扫描图 · 适配层经观测面那几条 · 格目录的判据共用（上层经 `test-support`）。
#[cfg(any(test, feature = "test-support"))]
#[path = "../../../../tests/backend/observe/recognize/index_row_support.rs"]
pub mod test_support;

#[cfg(test)]
#[path = "../../../../tests/backend/observe/recognize/index_row_tests.rs"]
mod tests;
