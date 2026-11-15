//! 托管终端的状态行（照 tmux 的 `status`）：模板 · 字段表 · 写成那一句。纯的，不碰盘、不问钟。
//!
//! 模板里 `{键}` 是一格、别的是原样的字；头一个 `|` 分左右两段。认不出的键原样留着那几个字（偏好页照 [`Template::unknown`] 标出来）。
//! 模板空（去掉空白）⇒ 不留那一行（[`Template::shows`]）。字段表 [`FIELDS`] 只登记在这里：偏好页的说明 · 预览 · 瘦客户端都照它。
//! 各格的值由上层凭盘现取（`faces::status_line_face`）；`holder` · `time` 由瘦客户端照它自己那一刻填。

use cmd_wire::quota::{SLOT_5H, SLOT_7D};
use copy_core::copy_text;
use std::collections::BTreeMap;

/// 随包的缺省模板（偏好里没设那一格时用它）。
pub const DEFAULT_TEMPLATE: &str =
    "{name} · {cwd} · {branch}|{queue} · {holder} · {model} · {ctx} · {5h} · {time}";

/// 偏好里那一格 ⇒ 照用的那一条模板：没设 ⇒ 随包的缺省；设了（连空串：不留那一行）⇒ 那一条。判缺省只在这里。
pub fn template_of(set: Option<&str>) -> &str {
    set.unwrap_or(DEFAULT_TEMPLATE)
}

/// 路径那一格至多几格宽（超了从左截）。
pub const PATH_COLS: usize = 32;

/// 字段表的一行：键 · 说明那一句 · 示例值 · 要不要分钟拍，一格的事全写在这一行。
#[derive(Debug, Clone, Copy)]
pub struct Field {
    /// 模板里写的键（`{键}`）。
    pub key: &'static str,
    /// 说明那一句（偏好页的字段表照它列）。
    pub says: fn() -> String,
    /// 示例值（预览里没有活着的托管终端时用）。
    pub sample: fn() -> String,
    /// 这一格按分钟走（模板里有它 ⇒ 瘦客户端起一个分钟拍）。
    pub ticks: bool,
}

/// 时刻那一格的键（瘦客户端按它那一刻填）。
pub const TIME: &str = "time";
/// 输入在谁手里那一格的键（瘦客户端按收的那一端填）。
pub const HOLDER: &str = "holder";

/// 状态行认得的字段（只登记在这里）。
pub const FIELDS: &[Field] = &[
    Field {
        key: "name",
        says: || copy_text("beStatusField.name.says", &[]),
        sample: || copy_text("beStatusField.name.sample", &[]),
        ticks: false,
    },
    Field {
        key: "cwd",
        says: || copy_text("beStatusField.cwd.says", &[]),
        sample: || copy_text("beStatusField.cwd.sample", &[]),
        ticks: false,
    },
    Field {
        key: "account",
        says: || copy_text("beStatusField.account.says", &[]),
        sample: || copy_text("beStatusField.account.sample", &[]),
        ticks: false,
    },
    Field {
        key: "model",
        says: || copy_text("beStatusField.model.says", &[]),
        sample: || copy_text("beStatusField.model.sample", &[]),
        ticks: false,
    },
    Field {
        key: "ctx",
        says: || copy_text("beStatusField.ctx.says", &[]),
        sample: || copy_text("beStatusField.ctx.sample", &[]),
        ticks: false,
    },
    Field {
        key: SLOT_5H,
        says: || copy_text("beStatusField.fiveHour.says", &[]),
        sample: || copy_text("beStatusField.fiveHour.sample", &[]),
        ticks: false,
    },
    Field {
        key: SLOT_7D,
        says: || copy_text("beStatusField.sevenDay.says", &[]),
        sample: || copy_text("beStatusField.sevenDay.sample", &[]),
        ticks: false,
    },
    Field {
        key: "queue",
        says: || copy_text("beStatusField.queue.says", &[]),
        // 排队 · 持有者的示例照真那一句写。
        sample: || copy_text("beQueue.said.waiting", &[("n", "2")]),
        ticks: false,
    },
    Field {
        key: HOLDER,
        says: || copy_text("beStatusField.holder.says", &[]),
        sample: || copy_text("beTerminal.holder.here", &[]),
        ticks: false,
    },
    Field {
        key: "host",
        says: || copy_text("beStatusField.host.says", &[]),
        sample: || copy_text("beStatusField.host.sample", &[]),
        ticks: false,
    },
    Field {
        key: "branch",
        says: || copy_text("beStatusField.branch.says", &[]),
        sample: || copy_text("beStatusField.branch.sample", &[]),
        ticks: false,
    },
    Field {
        key: TIME,
        says: || copy_text("beStatusField.time.says", &[]),
        sample: || copy_text("beStatusField.time.sample", &[]),
        ticks: true,
    },
];

/// 认不认得这个键。
pub fn field(key: &str) -> Option<&'static Field> {
    FIELDS.iter().find(|f| f.key == key)
}

/// 一格的值（键 → 写好的字；没有那一键 ⇒ 当空）。
pub type Values = BTreeMap<String, String>;

/// 交给瘦客户端的那一份：模板原文 ＋ 各格的值（`holder` · `time` 由它自己填）。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Snap {
    pub template: String,
    pub values: Values,
}

/// 模板里的一小段。
#[derive(Debug, Clone, PartialEq, Eq)]
enum Piece {
    Text(String),
    /// 认得的一格。
    Field(&'static str),
}

/// 解析好的模板。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Template {
    left: Vec<Piece>,
    right: Vec<Piece>,
    unknown: Vec<String>,
    shows: bool,
}

impl Template {
    /// 解析一条模板。
    pub fn parse(t: &str) -> Template {
        let shows = !t.trim().is_empty();
        let (l, r) = match t.split_once('|') {
            Some((l, r)) => (l, r),
            None => (t, ""),
        };
        let mut unknown = Vec::new();
        let left = pieces(l, &mut unknown);
        let right = pieces(r, &mut unknown);
        Template {
            left,
            right,
            unknown,
            shows,
        }
    }

    /// 留不留那一行（模板空 ⇒ 不留）。
    pub fn shows(&self) -> bool {
        self.shows
    }

    /// 认不出的键（照出现的先后，不重）。
    pub fn unknown(&self) -> &[String] {
        &self.unknown
    }

    /// 用没用到这一格。
    pub fn uses(&self, key: &str) -> bool {
        self.left
            .iter()
            .chain(&self.right)
            .any(|p| matches!(p, Piece::Field(k) if *k == key))
    }

    /// 有没有要分钟拍的那一格（字段表 [`Field::ticks`]）。
    pub fn ticks(&self) -> bool {
        self.shows && FIELDS.iter().any(|f| f.ticks && self.uses(f.key))
    }

    /// 左右两段各写成的字（不截）。
    pub fn halves(&self, v: &Values) -> (String, String) {
        (fill(&self.left, v), fill(&self.right, v))
    }

    /// 写成 `cols` 格宽的那一行：左段靠左、右段靠右，中间至少一格空；放不下先截左段尾部，还放不下再截右段尾部。
    pub fn render(&self, v: &Values, cols: usize) -> String {
        let (l, r) = self.halves(v);
        line_of(&l, &r, cols)
    }
}

/// 左右两段摆成 `cols` 格宽的一行（瘦客户端在左段前面接提示那一句时也照它摆）。
pub fn line_of(l: &str, r: &str, cols: usize) -> String {
    let rw = width(r);
    if r.is_empty() {
        return cut(l, cols);
    }
    if rw >= cols {
        return cut(r, cols);
    }
    let room = cols - rw;
    let l = if width(l) + 1 > room {
        cut(l, room.saturating_sub(1))
    } else {
        l.to_string()
    };
    let gap = cols - rw - width(&l);
    format!("{l}{}{r}", " ".repeat(gap))
}

fn pieces(s: &str, unknown: &mut Vec<String>) -> Vec<Piece> {
    let mut out: Vec<Piece> = Vec::new();
    let mut text = String::new();
    let mut rest = s;
    while let Some(at) = rest.find('{') {
        text.push_str(&rest[..at]);
        let tail = &rest[at + 1..];
        match tail.find('}') {
            Some(end) => {
                let key = &tail[..end];
                match field(key) {
                    Some(f) => {
                        if !text.is_empty() {
                            out.push(Piece::Text(std::mem::take(&mut text)));
                        }
                        out.push(Piece::Field(f.key));
                    }
                    None => {
                        text.push('{');
                        text.push_str(key);
                        text.push('}');
                        if !unknown.iter().any(|u| u == key) {
                            unknown.push(key.to_string());
                        }
                    }
                }
                rest = &tail[end + 1..];
            }
            None => {
                text.push_str(&rest[at..]);
                rest = "";
            }
        }
    }
    text.push_str(rest);
    if !text.is_empty() {
        out.push(Piece::Text(text));
    }
    out
}

/// 一段照值写：段头（头一格之前）· 段尾（末一格之后）的字照留；有值的那几格之间用前一格后面那段字隔开，
/// 空值那一格连它后面那段字一起不写（剩下的末一格后面直接接段尾 ⇒ 连着几格空也不留悬空的分隔）。
fn fill(ps: &[Piece], v: &Values) -> String {
    let val = |k: &str| v.get(k).map(|s| s.trim()).unwrap_or_default().to_string();
    let mut head = String::new();
    // （值, 它后面到下一格之前那段字）
    let mut cells: Vec<(String, String)> = Vec::new();
    for p in ps {
        match (p, cells.last_mut()) {
            (Piece::Text(t), None) => head.push_str(t),
            (Piece::Text(t), Some((_, gap))) => gap.push_str(t),
            (Piece::Field(k), _) => cells.push((val(k), String::new())),
        }
    }
    let tail = cells.pop();
    let mut kept: Vec<(String, String)> =
        cells.into_iter().filter(|(v, _)| !v.is_empty()).collect();
    let tail_text = match tail {
        Some((v, gap)) => {
            if !v.is_empty() {
                kept.push((v, String::new()));
            }
            gap
        }
        None => String::new(),
    };
    let mut s = head;
    let n = kept.len();
    for (i, (v, gap)) in kept.into_iter().enumerate() {
        s.push_str(&v);
        if i + 1 < n {
            s.push_str(&gap);
        }
    }
    s.push_str(&tail_text);
    s
}

/// 一个字占几格（东亚宽字 · 全角 · 表情两格；组合符号零格）。
pub fn char_cols(c: char) -> usize {
    let u = c as u32;
    if matches!(u, 0x0300..=0x036F | 0x200B..=0x200F | 0xFE00..=0xFE0F) {
        return 0;
    }
    if matches!(u,
        0x1100..=0x115F
        | 0x2E80..=0x303E
        | 0x3041..=0x33FF
        | 0x3400..=0x4DBF
        | 0x4E00..=0x9FFF
        | 0xA000..=0xA4CF
        | 0xAC00..=0xD7A3
        | 0xF900..=0xFAFF
        | 0xFE30..=0xFE4F
        | 0xFF00..=0xFF60
        | 0xFFE0..=0xFFE6
        | 0x1F300..=0x1F64F
        | 0x1F900..=0x1F9FF
        | 0x20000..=0x3FFFD)
    {
        return 2;
    }
    usize::from(!c.is_control())
}

/// 一段字占几格。
pub fn width(s: &str) -> usize {
    s.chars().map(char_cols).sum()
}

/// 截到 `cols` 格以内：截了 ⇒ 尾部一个 `…`（截法与记号照 [`copy_core::clip`]，这里只按格数算留几个字）。
pub fn cut(s: &str, cols: usize) -> String {
    if width(s) <= cols {
        return s.to_string();
    }
    if cols == 0 {
        return String::new();
    }
    let mut kept = 0;
    let mut w = 0;
    for c in s.chars() {
        let cw = char_cols(c);
        if w + cw > cols - 1 {
            break;
        }
        kept += 1;
        w += cw;
    }
    copy_core::clip(s, kept)
}

/// 路径那一格的值：在 `home` 底下 ⇒ 写成 `~/…`；超过 [`PATH_COLS`] 格 ⇒ 从左截、前面一个 `…`。
pub fn path_shown(cwd: &str, home: Option<&str>) -> String {
    let shown = match home.map(|h| h.trim_end_matches(['/', '\\'])) {
        Some(h) if !h.is_empty() && cwd == h => "~".to_string(),
        Some(h)
            if !h.is_empty() && cwd.starts_with(h) && cwd[h.len()..].starts_with(['/', '\\']) =>
        {
            format!("~{}", &cwd[h.len()..])
        }
        _ => cwd.to_string(),
    };
    if width(&shown) <= PATH_COLS {
        return shown;
    }
    let mut tail: Vec<char> = Vec::new();
    let mut w = 1;
    for c in shown.chars().rev() {
        let cw = char_cols(c);
        if w + cw > PATH_COLS {
            break;
        }
        tail.push(c);
        w += cw;
    }
    tail.reverse();
    format!("…{}", tail.into_iter().collect::<String>())
}

/// 字段表那一份给人看的：（键, 说明）照登记的先后。
pub fn field_rows() -> Vec<(String, String)> {
    FIELDS
        .iter()
        .map(|f| (f.key.to_string(), (f.says)()))
        .collect()
}

/// 各格的示例值（预览里没有活着的托管终端时用）。
pub fn samples() -> Values {
    FIELDS
        .iter()
        .map(|f| (f.key.to_string(), (f.sample)()))
        .collect()
}

#[cfg(test)]
#[path = "../../../../tests/backend/host/status_tests.rs"]
mod tests;
