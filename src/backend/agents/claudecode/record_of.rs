//! Claude 盘上一行（[`JsonlRecord`]，已带后端补的成品格）⇒ 通用记录（[`Record`]）。翻译表只住这里：
//!
//! | 盘上 | 通用记录 |
//! |---|---|
//! | `user` | `said`：`who` · `message.content` ⇒ `blocks` · 工具结果的一句 ⇒ `results` · `cwd` |
//! | `assistant` | `reply`；`isApiErrorMessage` ⇒ `error{reason, status}`；`message.model == "<synthetic>"`（且不是报错）⇒ `autoReply` |
//! | `system` · `subtype == "api_error"` | `retry` |
//! | `ai-title` / `custom-title` | `title{by: agent / user}` |
//! | `attachment` · `queued_command` · 人说的（一轮中途插进来的那一句） | `said`（身份 · 时刻是这一行自己的） |
//! | 看不懂的（`Unrecognized`：没见过的类型 · 形状变了） | `unread`（写好的一句 ＋ 原文摘录）；没见过的类型又没有链上身份 ⇒ 无记录（[`quiet_unread_of`]，会话事实里记数） |
//! | 其余（别的 `system` · `attachment` · 状态行 · 元数据） | 无记录（链上那一半由 [`super::chain`] 给） |

use super::schema::JsonlRecord;
use crate::agents::record::{Block, Body, Record, TitleBy, UnreadWhy};
use serde_json::Value;

/// 这一家在线上的 `agent` 值。
const AGENT: &str = "claude";
/// 代理那一侧自动写的应答，型号名写成这个。
const AUTO_REPLY_MODEL: &str = "<synthetic>";

/// 代理那一侧自动写的应答（不是 API 报错那一条：报错那条也写这个型号，画成报错卡）。流里不画它。
fn is_auto_reply(api_error: bool, model: Option<&str>) -> bool {
    !api_error && model == Some(AUTO_REPLY_MODEL)
}

/// 一条已解析的记录在消息流里落在哪（注册表 `TextFace.lands`）：自动写的应答流里不画；
/// 只装工具结果的那条 user 记录不自己成卡、收进它那次调用的那一块（取第一个结果块的 `tool_use_id`）；其余落在自己那张卡上。
pub(crate) fn lands_of(v: &Value) -> crate::agents::Lands {
    use crate::agents::Lands;
    let s = |k: &str| v.get(k).and_then(Value::as_str);
    let message = v.get("message");
    match s("type") {
        Some("assistant") => {
            let api_error = v.get("isApiErrorMessage").and_then(Value::as_bool) == Some(true);
            let model = message.and_then(|m| m.get("model")).and_then(Value::as_str);
            if is_auto_reply(api_error, model) {
                Lands::Nowhere
            } else {
                Lands::Own
            }
        }
        Some("user") => {
            let blocks = message
                .and_then(|m| m.get("content"))
                .and_then(Value::as_array)
                .filter(|b| !b.is_empty());
            let Some(blocks) = blocks else {
                return Lands::Own;
            };
            if !blocks
                .iter()
                .all(|b| b.get("type").and_then(Value::as_str) == Some("tool_result"))
            {
                return Lands::Own;
            }
            blocks[0]
                .get("tool_use_id")
                .and_then(Value::as_str)
                .filter(|id| !id.is_empty())
                .map_or(Lands::Own, |id| Lands::InTool(id.to_string()))
        }
        _ => Lands::Own,
    }
}

/// **没有链上身份的未知顶层类型 ⇒ 不成卡**（10-14 起）：对话内容每行都带 `uuid` / `parentUuid`，两样都没有的只可能是这一家的簿记行
/// （Claude Code 每一版都会添几种，一行一张「认不出的记录」会刷满消息流）。不悄悄丢：会话事实 `quietUnread` 照实记几行、哪几种
/// （注册表 `RecordFace.quiet_unread`）。带身份的未知行照旧 `unread`；认识的类型形状变了（不论有无身份）照旧 `unread`。
///
/// 判只这一处：[`record_of`] 对 `Unrecognized` 用同一个 [`no_identity`]、同一个「类型 serde 认不出」（`unknown-type` 那一支就是 serde 落到 `Unknown`），
/// 判据逐行对拍两处一致。这一问给的是那个类型名；不是这种行 ⇒ `None`。
pub fn quiet_unread_of(v: &Value) -> Option<String> {
    let s = |k: &str| v.get(k).and_then(Value::as_str);
    if !no_identity(s("uuid"), s("parentUuid")) {
        return None;
    }
    let t = s("type")?;
    // 只拿判别格试解：落到 `Unknown` ⇔ 类型 serde 不认识（认识的类型缺格是 Err，不算）。
    matches!(
        serde_json::from_value::<JsonlRecord>(serde_json::json!({ "type": t })),
        Ok(JsonlRecord::Unknown)
    )
    .then(|| t.to_string())
}

/// 链上身份两样都没有（抢救时取的就是这两格的字符串值，[`super::parse`]）。
fn no_identity(uuid: Option<&str>, parent_uuid: Option<&str>) -> bool {
    uuid.is_none() && parent_uuid.is_none()
}

/// 一条解析好的盘上记录 ⇒ 通用记录；不进界面 ⇒ `None`。`start` ＝ 这一行在记录里的起点字节：没有身份的那几类（标题 · 排队）`id` 按它合成
/// （[`crate::agents::line_id`]，会话内唯一），图那一块的引用也记它（[`Block::Image`] 的 `at`）。
pub fn record_of(rec: JsonlRecord, start: u64) -> Option<Record> {
    let mut images = Images { at: start, n: 0 };
    let rec_of = |id: String, at: Option<String>, body: Body| {
        let id = if id.is_empty() {
            crate::agents::line_id(start)
        } else {
            id
        };
        crate::agents::record::record(AGENT.into(), id, at, body)
    };
    match rec {
        JsonlRecord::User {
            uuid,
            timestamp,
            message,
            cwd,
            user_text,
            tool_results,
            ..
        } => {
            let blocks = blocks_of(&message.content, &mut images);
            let (who, attachments) = with_attachments(user_text, &blocks, cwd.as_deref());
            Some(rec_of(
                uuid,
                Some(timestamp),
                Body::Said {
                    who,
                    blocks,
                    results: tool_results,
                    cwd,
                    attachments,
                },
            ))
        }
        JsonlRecord::Assistant {
            uuid,
            timestamp,
            message,
            is_api_error_message,
            api_error_status,
            api_reason,
            tool_cards,
            tool_steps,
            child_runs,
            ..
        } => {
            let error = is_api_error_message.then(|| {
                crate::agents::record::reply_error(
                    api_reason.unwrap_or(crate::agents::ApiReason::Unknown),
                    api_error_status,
                )
            });
            let auto_reply = is_auto_reply(error.is_some(), message.model.as_deref());
            Some(rec_of(
                uuid,
                Some(timestamp),
                Body::Reply {
                    blocks: crate::agents::record::reply_blocks(blocks_of(
                        &message.content,
                        &mut images,
                    )),
                    model: message.model.filter(|m| m != AUTO_REPLY_MODEL),
                    auto_reply,
                    cards: tool_cards,
                    steps: tool_steps,
                    runs: child_runs,
                    error,
                },
            ))
        }
        JsonlRecord::System {
            subtype,
            timestamp,
            uuid,
            retry_attempt,
            max_retries,
            api_reason,
            ..
        } if subtype.as_deref() == Some("api_error") => Some(rec_of(
            uuid.unwrap_or_default(),
            Some(timestamp),
            Body::Retry {
                reason: api_reason.unwrap_or(crate::agents::ApiReason::Unknown),
                attempt: retry_attempt,
                max: max_retries,
                text: None,
            },
        )),
        JsonlRecord::AiTitle { ai_title, .. } => Some(rec_of(
            String::new(),
            None,
            Body::Title {
                text: ai_title,
                by: TitleBy::Agent,
            },
        )),
        JsonlRecord::CustomTitle { custom_title, .. } => Some(rec_of(
            String::new(),
            None,
            Body::Title {
                text: custom_title,
                by: TitleBy::User,
            },
        )),
        // 一轮中途插进来的那一句：这一行就是它在记录里的位置（身份 · 打字那一刻都是它自己的），当人说的一句出。
        JsonlRecord::Attachment {
            uuid,
            timestamp,
            cwd,
            attachment: Some(a),
            user_text: Some(who),
            ..
        } => {
            let blocks = blocks_of(&a.prompt, &mut images);
            let (who, attachments) = with_attachments(who, &blocks, cwd.as_deref());
            Some(rec_of(
                uuid,
                Some(timestamp),
                Body::Said {
                    who,
                    blocks,
                    results: Default::default(),
                    cwd,
                    attachments,
                },
            ))
        }
        // 认不出（没见过的类型 · 见过的类型形状变了）：每行一条 `unread`；相邻同类并成一条是出记录页那一遍的事。
        // 没见过的类型、又没有链上身份 ⇒ 簿记行，不成卡（[`quiet_unread_of`]）。
        JsonlRecord::Unrecognized {
            ref uuid,
            ref parent_uuid,
            ref reason,
            ..
        } if reason == "unknown-type" && no_identity(uuid.as_deref(), parent_uuid.as_deref()) => {
            None
        }
        JsonlRecord::Unrecognized {
            uuid,
            timestamp,
            original_type,
            raw,
            reason,
            ..
        } => {
            // 连 `type` 都没有的一行算「没见过的类型」（serde 报的是缺判别格，不是认识的类型形状变了）。
            let why = if reason == "unknown-type" || original_type.is_none() {
                UnreadWhy::UnknownType
            } else {
                UnreadWhy::ParseFailed
            };
            Some(rec_of(
                uuid.unwrap_or_default(),
                timestamp,
                crate::agents::record::unread_body(why, original_type, &raw),
            ))
        }
        _ => None,
    }
}

/// 人说的那一条里认附件（送字那一下 [`super::ATTACH`] 写的那几形）：正文结尾那几行是指进附件目录的 `@相对路径` ⇒ 收进附件、从正文去掉；
/// 这一条带图片块 ⇒ 开头那几个 `[Image #N]`（括号粘贴图片路径收成的占位）也去掉，图照旧在图片块里。别的说话人 · 别的 `@` 行不动。
/// 附件在不在、多大，按这一条的工作目录现看一眼；没有工作目录 ⇒ 说不出（不标已清理）。
fn with_attachments(
    mut who: cmd_wire::record::UserText,
    blocks: &[Block],
    cwd: Option<&str>,
) -> (
    cmd_wire::record::UserText,
    Vec<cmd_wire::attachment::Attachment>,
) {
    use crate::common::attachment_dir as layout;
    if who.speaker != cmd_wire::record::Speaker::Human {
        return (who, Vec::new());
    }
    let mut lines: Vec<&str> = who.text.lines().collect();
    let mut found = Vec::new();
    while let Some(rel) = lines
        .last()
        .and_then(|l| l.trim().strip_prefix('@'))
        .filter(|r| r.starts_with(layout::DIR))
    {
        let Some((id, name)) = layout::parse_rel(rel) else {
            break;
        };
        found.push(attachment_at(cwd, id, name));
        lines.pop();
    }
    found.reverse();
    let body = lines.join("\n");
    let has_image = blocks.iter().any(|b| matches!(b, Block::Image { .. }));
    let kept = if has_image {
        strip_image_marks(&body)
    } else {
        body.as_str()
    };
    if found.is_empty() && kept.len() == body.len() {
        return (who, found);
    }
    let cut = u32::try_from(body[..body.len() - kept.len()].encode_utf16().count()).unwrap_or(0);
    let text = kept.trim_end().to_string();
    let end = u32::try_from(text.encode_utf16().count()).unwrap_or(0);
    who.pasted = std::mem::take(&mut who.pasted)
        .into_iter()
        .filter(|p| p.start >= cut && p.end - cut <= end)
        .map(|mut p| {
            p.start -= cut;
            p.end -= cut;
            p.body_start -= cut;
            p.body_end -= cut;
            p
        })
        .collect();
    who.text = text;
    (who, found)
}

/// 开头那几个 `[Image #N]`（连同它们之间与之后的空白）去掉之后的那一截。
fn strip_image_marks(s: &str) -> &str {
    let mut rest = s.trim_start();
    while let Some(after) = rest.strip_prefix("[Image #") {
        let Some(end) = after.find(']') else {
            break;
        };
        if end == 0 || !after[..end].bytes().all(|b| b.is_ascii_digit()) {
            break;
        }
        rest = after[end + 1..].trim_start();
    }
    rest
}

/// 编号 `id` 那一份（原名 `name`）此刻在工作目录 `cwd` 下的样子。
fn attachment_at(cwd: Option<&str>, id: &str, name: &str) -> cmd_wire::attachment::Attachment {
    let rel = crate::common::attachment_dir::rel_of(id, name);
    let meta = cwd.map(|c| {
        crate::platform::fs::Ro::symlink_metadata(std::path::Path::new(c).join(&rel))
            .ok()
            .filter(|m| m.is_file())
    });
    let gone = matches!(meta, Some(None));
    let bytes = meta.flatten().map(|m| m.len());
    let class = crate::common::file_class::class_of_name(name.as_bytes(), false);
    cmd_wire::attachment::Attachment {
        name: name.to_string(),
        class,
        class_text: crate::common::file_class::label(class),
        rel,
        bytes,
        size_text: bytes.map(copy_core::size_text),
        gone,
    }
}
/// `message.content`（字符串 · 块数组）⇒ 通用内容块；认不出的块跳过。
pub(crate) fn blocks_of(content: &Value, images: &mut Images) -> Vec<Block> {
    match content {
        Value::String(s) => vec![Block::text(s.clone())],
        Value::Array(a) => a.iter().filter_map(|b| block_of(b, images)).collect(),
        _ => Vec::new(),
    }
}

/// 一行里的图数到第几张了（[`Block::Image`] 的 `at` · `n`）：一行一份，按块的先后深度优先数。
pub(crate) struct Images {
    at: u64,
    n: u32,
}

fn block_of(b: &Value, images: &mut Images) -> Option<Block> {
    let s = |k: &str| b.get(k).and_then(Value::as_str).map(str::to_string);
    Some(match b.get("type").and_then(Value::as_str)? {
        "text" => Block::text(s("text")?),
        // 近来的思考块几乎都只有签名、`thinking` 是空的：空 / 只剩空白 ⇒ `""`（线上约定：空 ＝ 没写进记录，两端照它画、不各自判）。
        "thinking" => crate::agents::beat::thinking(
            s("thinking")
                .filter(|t| !t.trim().is_empty())
                .unwrap_or_default(),
        ),
        "tool_use" => Block::ToolUse {
            id: s("id")?,
            name: s("name")?,
            input: b.get("input").cloned().unwrap_or(Value::Null),
        },
        "tool_result" => Block::tool_result(
            s("tool_use_id")?,
            b.get("content")
                .map(|c| blocks_of(c, images))
                .unwrap_or_default(),
            b.get("is_error").and_then(Value::as_bool) == Some(true),
        ),
        "image" => image_of(b.get("source")?, images)?,
        _ => return None,
    })
}

/// 图那一块 ⇒ 引用（字节留在块里、不上线）。只认内联 base64 的图片（`{type: base64, media_type: image/…, data}`）；别的来源画不出 ⇒ 不出块。
fn image_of(source: &Value, images: &mut Images) -> Option<Block> {
    let s = |k: &str| source.get(k).and_then(Value::as_str);
    if s("type")? != "base64" {
        return None;
    }
    let media_type = s("media_type").filter(|m| m.starts_with("image/"))?;
    let data = s("data")?;
    let n = images.n;
    images.n += 1;
    Some(Block::Image {
        media_type: media_type.to_string(),
        bytes: (data.trim_end_matches('=').len() as u64) * 3 / 4,
        at: images.at,
        n,
        data: data.to_string(),
    })
}

// 判据（`record_of_tests.rs`）拿骨架行比整条翻译、经观测面 ⇒ 挂在 `observe` 底下。
