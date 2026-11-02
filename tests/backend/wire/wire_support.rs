//! 帧面零件的判据夹具：本包的判据与上层（主 crate）的判据经 `test-support` 共用。

use super::Topic;

/// 全部主题（表的行序；逐行核表用：主题表的判据 · 帧的判据都读这一份）。
pub const ALL: [Topic; 17] = [
    Topic::Accounts,
    Topic::Profiles,
    Topic::Quota,
    Topic::Rotation,
    Topic::RotationStrategies,
    Topic::Plan,
    Topic::Tasks,
    Topic::Needs,
    Topic::Login,
    Topic::Machines,
    Topic::Machine,
    Topic::Empty,
    Topic::SessionHidden,
    Topic::FilesDir,
    Topic::HistoryList,
    Topic::Groups,
    Topic::SessionQueue,
];

// ═══ 第二个前端在读的形状：冻结（只许加字段）═══════════════════════════════
//
// 两个前端吃同一个后端：桌面端（monitor）与第二个前端（手机端）。后者按下表逐字段读帧，缺一格就把整帧当坏帧丢
// （症状是「会话列表有、点进去永远空白」，零报错）。⇒ 表里登记的每一格不许改名、删、换类型；加新字段照旧（additive）。
// 会话三帧（`session_added` · `session_status` · `session_left`）在格目录里，冻结的格登在那边（`cells_catalog::tests::FROZEN`），不在这张表。

/// （帧 kind 或 `request`, 字段, JSON 类型）。`request` 是入方向请求信封。帧的判据逐格核它，`build_id_guard` 也读它。
pub const SECOND_FRONTEND_READS: &[(&str, &str, &str)] = &[
    ("hello", "build_id", "string"),
    ("hello", "host_arch", "string"),
    ("line", "session_id", "string"),
    ("line", "path", "string"),
    ("line", "seq", "number"),
    ("line", "byte_offset", "number"),
    // 〔第二个前端 2026-10-10〕通用记录那一团。里面的格（`said.results` 的 `patch` · `file` …）整件冻结在格目录
    // （格目录冻结表 `cells_catalog::tests::FROZEN` 的 `record`，`Frozen::All`）；这里只钉「line 帧上有这一格」。
    ("line", "record", "object"),
    ("overflow", "said", "string"),
    // 〔第二个前端 live1 10-13〕**活卡**（聊天屏的逐字长出，核心拼好的成品）。`live_frames_have_exactly_these_bytes` 钉的是字节，
    // 本表钉的是**有人在读它**。⚠ 一份样本只带得动一种 `op`：取整卡（`open`）那一支；增量与撤卡那几种的字节由逐字节判据钉。
    ("live_card", "sid", "string"),
    ("live_card", "run", "string"),
    ("live_card", "card", "number"),
    ("live_card", "n", "number"),
    ("live_card", "op", "string"),
    ("live_card", "rid", "string"),
    ("live_card", "phase", "string"),
    ("live_card", "clipped", "bool"),
    ("live_card", "blocks", "array"),
    // 〔第二个前端 2026-10-08〕**一切帧命令的应答与撤销**。失败那三格（`code` / `said` / `detail`）是
    // 失败卡与「复制详情」的全部原料：`detail` 跟着帧回，而最常见的那条错正好是「联系不上」—— 那时再去取就取不到。
    ("reply", "id", "string"),
    ("reply", "ok", "bool"),
    ("reply", "code", "string"),
    ("reply", "said", "string"),
    ("reply", "detail", "string"),
    ("reply", "data", "object"),
    ("cancelled", "id", "string"),
    ("request", "id", "string"),
    ("request", "cmd", "string"),
    ("request", "args", "object"),
    ("request", "within_ms", "number"),
];

/// 一条最少格的 `said` 通用记录（结构占位）。
pub fn said_record(id: &str, text: &str) -> crate::agents::record::Record {
    use crate::agents::record::{Block, Body, Record};
    Record {
        agent: "claude".into(),
        id: id.into(),
        at: None,
        time_text: None,
        at_ms: None,
        draw: crate::agents::record::Draw::Card,
        human: true,
        body: Body::Said {
            who: crate::agents::UserText {
                speaker: crate::agents::Speaker::Human,
                text: text.into(),
                pasted: Vec::new(),
            },
            blocks: vec![Block::text(text.into())],
            results: Default::default(),
            cwd: None,
            attachments: Vec::new(),
        },
    }
}
