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
/// ESC 折叠、slash/compact 细条…），只按记录的**原料**数。前端据 `t` / `sp` / `fd` 分档，
/// 分不准的那几档落到偏保守的常数（`§2.5b`：「粗估用偏保守的常数，精算后往下修」）。
#[derive(Debug, Default, Clone, serde::Serialize, PartialEq, Eq)]
pub(crate) struct IndexRow {
    /// 行起点字节偏移（绝对，0-based）—— 按需取正文就是 `[o, o+n)`。
    pub(crate) o: u64,
    /// 行字节长（**含**结尾 `\n`）。
    pub(crate) n: u64,
    /// 这一行会翻成哪一类通用记录（`said` · `reply` · `retry` · `title` · `queued`，与读正文时那条记录的 `t` 同一个词）；
    /// 不进界面（元数据 · 解析不出 · 认不出）⇒ 省略。判定在适配层（注册表 `RecordFace.class`，与它的翻译表对拍）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) t: Option<crate::agents::record::RecordClass>,
    /// `uuid`（前端 `uuidToIdx` —— 跳转与对账的锚）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) u: Option<String>,
    /// 这条记录属于某个子运行（适配层 `RecordFace::run_of` 答得出）。
    #[serde(skip_serializing_if = "is_false")]
    pub(crate) sc: bool,
    /// user 记录是谁说的（`Speaker` 的 `kind`），人说的与工具结果省略（那两种按正文 / 折叠单元就分得清）。
    /// 界面据它决定这一行建不建卡（判定在适配层，画不画在界面）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) sp: Option<&'static str>,
    /// 正文字符数（**代码块之外**；Unicode 标量计，不含换行）。
    #[serde(skip_serializing_if = "is_zero")]
    pub(crate) ch: u32,
    /// 其中 CJK/全宽字符数（口径 = `height-estimate.ts::fallbackTextHeight` 的 `> 0x2E80`）。
    #[serde(skip_serializing_if = "is_zero")]
    pub(crate) cj: u32,
    /// 正文**非空**硬行数（代码块之外）。
    #[serde(skip_serializing_if = "is_zero")]
    pub(crate) pl: u32,
    /// 围栏代码块数（```` ``` ```` / `~~~` 开合一对算一个；没合上的算到文末）。
    #[serde(skip_serializing_if = "is_zero")]
    pub(crate) cb: u32,
    /// 代码块内总行数。
    #[serde(skip_serializing_if = "is_zero")]
    pub(crate) cl: u32,
    /// 折叠单元数：`tool_use` / `tool_result` / `thinking` / `redacted_thinking` / `image` 块。
    /// 它们各渲染成一行 summary（纯工具的记录画成一张「几步」卡，每步一行），与正文长短无关。
    #[serde(skip_serializing_if = "is_zero")]
    pub(crate) fd: u32,
    /// 这一行是一条**用户输入**（大纲的一项）⇒ 它的摘要；不是 ⇒ 省略。
    ///
    /// 判定只住 [`crate::observe::recognize::user_inputs::user_input_of`]（与大纲清单 **同一个函数**），
    /// 摘要同 `excerpt`、uuid 就是本行的 `u`。有了它，首屏的「索引」与「大纲清单」合成一趟读：
    /// 前端见到索引里**有** `x` ⇒ 对面是会出它的后端、每一条用户输入都带着 ⇒ 不再单独要清单；
    /// 一个 `x` 都没有 ⇒ 分不清「老后端」还是「真的零条」⇒ 照旧要一份（形状登记 `IPC-PROTOCOL.md §10.3`）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) x: Option<String>,
    /// 同上那一行的 `timestamp`（空串 ⇒ 省略；清单那边的空串 == 这里缺席）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) ts: Option<String>,
}

fn is_false(b: &bool) -> bool {
    !*b
}
fn is_zero(n: &u32) -> bool {
    *n == 0
}

/// 一行（已经解析过；解析不了 ⇒ `None`，那一行照样占位 —— 丢了它，后面每一行的 seq 都会错一位）→ [`IndexRow`]。**纯函数**，不碰文件系统。
/// `kind` ＝ 这份记录是哪一家的（[`crate::agents::record_kind_of`]）：这一行进不进界面、谁说的都照那一家的认法判。
pub(crate) fn index_row_of(
    kind: &str,
    v: Option<&serde_json::Value>,
    beat: Option<&crate::agents::beat::Beat>,
    offset: u64,
    len: u64,
) -> IndexRow {
    let mut row = IndexRow {
        o: offset,
        n: len,
        ..IndexRow::default()
    };
    let Some(v) = v else {
        return row;
    };
    row.u = beat.and_then(|b| b.id.as_deref()).map(str::to_string);
    row.t = crate::agents::record_class_of(kind, v);
    row.sc = crate::agents::run_of_record(kind, &v).is_some();
    let said = beat.and_then(|b| b.said());
    row.sp = said
        .map(|u| u.speaker.kind())
        .filter(|k| !matches!(*k, "human" | "toolResult"));
    // 大纲那一项（判定只住 `user_inputs`；这里只搬字段）
    if let Some(ui) =
        beat.and_then(|b| crate::observe::recognize::user_inputs::user_input_given(kind, v, b))
    {
        row.x = Some(ui.excerpt);
        row.ts = Some(ui.timestamp).filter(|t| !t.is_empty());
    }
    // 正文在哪：记录树那一家的原文就是通用块那一形 —— user/assistant 在 `message.content`（字符串或块数组）；system 在顶层 `content`。
    // 不住记录树的那一家（原文是它自己的形状）⇒ 数它翻出来的那条通用记录的块（同一个数法，下面那一段）。
    let translated = crate::agents::record_face(kind)
        .filter(|f| f.tree.is_none())
        .map(|f| translated_of(&f, v));
    if let Some((_, Some(said_by))) = &translated {
        row.sp = Some(*said_by).filter(|k| !matches!(*k, "human" | "toolResult"));
    }
    let content = match &translated {
        Some((blocks, _)) => blocks.as_ref(),
        None => v
            .get("message")
            .and_then(|m| m.get("content"))
            .or_else(|| v.get("content")),
    };
    match content {
        Some(serde_json::Value::String(s)) => count_prose(s, &mut row),
        Some(serde_json::Value::Array(blocks)) => {
            for b in blocks {
                match b.get("type").and_then(|t| t.as_str()) {
                    Some("text") => {
                        if let Some(s) = b.get("text").and_then(|t| t.as_str()) {
                            count_prose(s, &mut row);
                        }
                    }
                    Some(
                        "tool_use" | "tool_result" | "thinking" | "redacted_thinking" | "image",
                    ) => row.fd += 1,
                    _ => {}
                }
            }
        }
        _ => {}
    }
    row
}

/// 那一家把这一行翻成的通用记录：它的块（序列化成线上那一形 `[{type, text, …}]`）· 人那一侧是谁说的（`Speaker` 的 `kind`）。
/// 不进界面 / 没有块 ⇒ 块缺。
fn translated_of(
    face: &crate::agents::RecordFace,
    v: &serde_json::Value,
) -> (Option<serde_json::Value>, Option<&'static str>) {
    use crate::agents::record::Body;
    let Some(rec) = (face.parse)(&crate::agents::Line::new(&v.to_string(), 0))
        .ok()
        .flatten()
        .and_then(|t| t.record)
    else {
        return (None, None);
    };
    match &rec.body {
        Body::Said { blocks, who, .. } => {
            (serde_json::to_value(blocks).ok(), Some(who.speaker.kind()))
        }
        Body::Reply { blocks, .. } => (serde_json::to_value(blocks).ok(), None),
        _ => (None, None),
    }
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

#[cfg(test)]
#[path = "../../../../tests/backend/observe/recognize/index_row_tests.rs"]
pub(crate) mod tests; // `pub(crate)`：从原文起的那一行 `tests::index_row` 给扫描图 · 格目录的判据共用
