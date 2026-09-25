//! 〔`设计/10` 骨架 · 子步 3〕**monitor 侧接上「从偏移读」**：骨架索引 ＋ 按偏移取一段正文。
//!
//! # 它是什么
//!
//! 后端早就有 `--read-session-from-offset <path> <offset>`（`observe/history_query.rs`），
//! 而 monitor 侧**零调用点**（`设计/10 §3.1 A1`）。本模块是它的两个调用点：
//!
//! | 命令 | 跑什么 | 给谁 |
//! |---|---|---|
//! | [`read_session_index`] | `--read-session-from-offset <p> <from> --index` | 前端骨架：一次拿到总条数 ＋ 每条的偏移/uuid/宽度无关料（`IPC-PROTOCOL.md §10.3`） |
//! | [`read_session_range`] | `--read-session-from-offset <p> <offset> --until <end>` | 前端「只物化可见区」：按索引里的行边界取**那一段**正文 |
//!
//! 两条都走 [`crate::subagent::Backend`]（本机 exec 本机后端 / 远端 ssh exec 同一个二进制）——
//! 「这条查询谁去跑」全仓只有那一处分流。
//!
//! # 🔴 诚实降级不是可选项
//!
//! 索引是一个**选项**不是新子命令（新子命令要 bump `BUILD_ID`，本轮不许）⇒ 已部署的老后端
//! **不认 `--index`、照旧透传 jsonl 字节**。本侧据首行认出来（[`parse_index_output`]），
//! 回 `available: false` ＋ 原因 —— **不是错误**（前端退回今天的尾部窗口，照常能用），
//! 也**绝不**把 jsonl 行当索引行解析。本机后端不在（开发树里常态）同样走这一档。
//!
//! # 买不到
//!
//! - **续传没接到断线重连上**：重连路住 `ssh_source.rs`（本轮不许碰）。本模块给出了它要的那一半
//!   （`read_session_index(from)` 的 `end` 就是续传令牌、`read_session_range` 就是按偏移续拉），
//!   剩下的是在 `ssh_source` 的重连处把「从 seq 0 重发」换成调这两条。
//! - **远端整体 30s 超时**（`run_list_query` 的 `LIST_TIMEOUT`）：弱网上超大会话的索引可能撞上。

use crate::parser::parse_line;
use crate::subagent::Backend;

/// 骨架索引的回包。`available == false` 时 `rows` 为空、`reason` 说清为什么（**不是错误**）。
#[derive(Debug, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../../src/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct SessionIndexResult {
    pub available: bool,
    #[cfg_attr(test, ts(optional))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// 索引从哪个字节起（= 请求的 `from_offset`）。
    #[cfg_attr(test, ts(type = "number"))]
    pub from: u64,
    /// 最后一个完整行的末字节 ＝ **下一次续传该带的 `from_offset`**。
    #[cfg_attr(test, ts(type = "number"))]
    pub end: u64,
    /// 每个可计行一条（形状 = 后端 `IndexRow`，前端类型 `SkeletonFacts`）。
    /// 本侧**不解释**这些行，只核头尾 —— 形状的唯一住址是 `IPC-PROTOCOL.md §10.3`。
    #[cfg_attr(test, ts(type = "Array<import(\"../height-estimate\").SkeletonFacts>"))]
    pub rows: Vec<serde_json::Value>,
}

/// 本侧认得出的「拿不到索引」的几种样子。
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum IndexUnavailable {
    /// 首行不是 `session_index` 头 —— 对面是不认 `--index` 的老后端（它透传了 jsonl 字节）。
    OldBackend,
    /// 有头没尾，或尾行条数与实到行数对不上 —— 输出被截断了，**不许当全量用**。
    Truncated { got: usize, claimed: Option<u64> },
}

impl IndexUnavailable {
    fn reason(&self) -> String {
        match self {
            Self::OldBackend => "对面后端不认 --index（老版本，需要重装后端才有骨架索引）".into(),
            Self::Truncated { got, claimed } => {
                format!("索引输出被截断：实到 {got} 行，尾行声称 {claimed:?} —— 不当全量用")
            }
        }
    }
}

/// 核头尾、剥出行。**纯函数**，两条 transport 的输出（逐行、已 trim、已剔空行）都走它。
pub(crate) fn parse_index_output(
    lines: &[String],
) -> Result<(u64, u64, Vec<serde_json::Value>), IndexUnavailable> {
    let parse = |l: &String| serde_json::from_str::<serde_json::Value>(l).ok();
    let head = lines.first().and_then(parse);
    let Some(head) =
        head.filter(|h| h.get("kind").and_then(|k| k.as_str()) == Some("session_index"))
    else {
        return Err(IndexUnavailable::OldBackend);
    };
    let from = head.get("from").and_then(|v| v.as_u64()).unwrap_or(0);
    let tail = lines.last().filter(|_| lines.len() >= 2).and_then(parse);
    let tail = tail.filter(|t| t.get("kind").and_then(|k| k.as_str()) == Some("session_index_end"));
    let got = lines.len().saturating_sub(2);
    let Some(tail) = tail else {
        return Err(IndexUnavailable::Truncated { got, claimed: None });
    };
    let claimed = tail.get("count").and_then(|v| v.as_u64());
    if claimed != Some(got as u64) {
        return Err(IndexUnavailable::Truncated { got, claimed });
    }
    let end = tail.get("end").and_then(|v| v.as_u64()).unwrap_or(from);
    let mut rows = Vec::with_capacity(got);
    for l in &lines[1..lines.len() - 1] {
        match parse(l) {
            Some(v) => rows.push(v),
            // 中间一行坏了 ⇒ 后面的 seq 全错一位 ⇒ 整份不要
            None => return Err(IndexUnavailable::Truncated { got, claimed }),
        }
    }
    Ok((from, end, rows))
}

/// 索引那条的 argv。🔴 **选项写在位置参数前面** —— 让不认它的老后端**零字节失败**：
/// 老后端只看 `args[1]`/`args[2]`，选项在后它会把整份会话透传回来（弱网上几十 MB，只为认出「它不会」）；
/// 选项在前它拿路径当 offset 解析 ⇒ `offset must be a number` ⇒ 退出 2、stdout 0 字节
/// （现打读数在后端 `FromOffsetOpts` 的头注里）。新后端两种位置都认。
pub(crate) fn index_argv(jsonl_path: &str, from_offset: u64) -> Vec<String> {
    vec![
        "--read-session-from-offset".into(),
        "--index".into(),
        jsonl_path.into(),
        from_offset.to_string(),
    ]
}

/// 按偏移取正文那条的 argv（同一条纪律：选项在前）。
pub(crate) fn range_argv(jsonl_path: &str, offset: u64, until: u64) -> Vec<String> {
    vec![
        "--read-session-from-offset".into(),
        "--until".into(),
        until.to_string(),
        jsonl_path.into(),
        offset.to_string(),
    ]
}

/// 路径的廉价预检（与 `load_subagent` / `stream_read_remote_session` 同一条纪律）：
/// 真正的越权读由后端 `fence_under_projects` 兜底。
fn precheck(jsonl_path: &str) -> Result<(), String> {
    if jsonl_path.contains("..") || !jsonl_path.ends_with(".jsonl") {
        return Err(format!("非法会话路径: {jsonl_path}"));
    }
    Ok(())
}

/// **骨架索引**：从字节 `from_offset` 起（冷启动 0 / 续传传上次的 `end`）。
#[tauri::command]
pub async fn read_session_index(
    origin: crate::origin::Origin,
    jsonl_path: String,
    from_offset: u64,
) -> Result<SessionIndexResult, String> {
    let route = origin.route("read_session_index")?;
    precheck(&jsonl_path)?;
    let unavailable = |reason: String| SessionIndexResult {
        available: false,
        reason: Some(reason),
        from: from_offset,
        end: from_offset,
        rows: Vec::new(),
    };
    let backend = Backend::for_origin(route)?;
    let argv = index_argv(&jsonl_path, from_offset);
    let argv: Vec<&str> = argv.iter().map(String::as_str).collect();
    // 「后端不在 / 查询失败」对骨架来说**同一个处置**：没有索引，退回尾部窗口 —— 原因原样带给前端显示。
    let lines = match backend.query(&argv).await {
        Ok(l) => l,
        Err(e) => return Ok(unavailable(e.message)),
    };
    Ok(match parse_index_output(&lines) {
        Ok((from, end, rows)) => SessionIndexResult {
            available: true,
            reason: None,
            from,
            end,
            rows,
        },
        Err(u) => {
            tracing::info!(
                "read_session_index({jsonl_path}): {} 退回尾部窗口：{}",
                backend.whose(),
                u.reason()
            );
            unavailable(u.reason())
        }
    })
}

/// 〔U3b〕**monitor 侧「这一行占不占 seq」的唯一住址** —— 与后端 `history_query·rs::line_counts`
/// 同一口径（剥 BOM 再 `trim`，空了就不占号）。三个调用方：本机历史读（`history·rs::stream_read_session_jsonl`）·
/// 远端历史读（`remote_history·rs::stream_read_remote_session`）· 按偏移取正文（[`range_payloads`]）。
///
/// # 为什么要有它
///
/// 查看器那两支原先只给**可显示**的记录编号（`permission-mode` 之类不占号）⇒ 查看器的 seq
/// 与 watcher / 骨架索引的行号空间**不是同一个**，骨架接不上。改成「每个可计行占一个号、
/// 不可显示的占号不出 payload」之后，三条读路与后端索引同一个空间：索引第 k 行 = seq `base+k`。
///
/// ⚠ 跨 crate 的对拍住 `tests/__fixtures__/skeleton-seq-space.jsonl`（＋同名 `.golden`）：后端索引与本类各自读同一份夹具、
/// 各自对同一份金标准（两侧实现不同源）。
#[derive(Debug, Default)]
pub(crate) struct LineNumberer {
    next: u64,
}

impl LineNumberer {
    pub(crate) fn starting_at(base: u64) -> Self {
        Self { next: base }
    }

    /// 一行原文 → `Some((seq, 去掉 BOM 与首尾空白的正文))`；不占号的行 ⇒ `None`。
    pub(crate) fn number<'a>(&mut self, raw: &'a str) -> Option<(u64, &'a str)> {
        let body = raw.trim_start_matches('\u{feff}').trim();
        if body.is_empty() {
            return None;
        }
        let seq = self.next;
        self.next += 1;
        Some((seq, body))
    }

    /// 已经发出去的号数（= 下一个号 − 起点）之外，调用方关心的只有「下一个号」。
    pub(crate) fn next_seq(&self) -> u64 {
        self.next
    }
}

/// 〔U3b〕**先占号、后过滤** —— 两条历史读路与 [`range_payloads`] 共用的那一步。
///
/// 顺序就是全部要点：不可显示的记录（`permission-mode` …）与解析不出的行**照占号**、只是不出记录；
/// 把占号挪到过滤之后，seq 就退回「可显示序号」、与索引对不上（`tests/bridge/session_skeleton_tests.rs`
/// 的金标准那一格会红）。返回 `None` = 这一行不出记录（号可能已经占了）。
pub(crate) fn numbered_displayable<'a, E>(
    numberer: &mut LineNumberer,
    raw: &'a str,
    parse: impl FnOnce(&'a str) -> Result<Option<crate::messages::JsonlRecord>, E>,
) -> Option<(u64, crate::messages::JsonlRecord)> {
    let (seq, body) = numberer.number(raw)?;
    match parse(body) {
        Ok(Some(rec)) if rec.is_displayable() => Some((seq, rec)),
        _ => None,
    }
}

/// 把 `--read-session-from-offset … --until` 的输出行编成 payload。**纯函数**。
///
/// `seq_base` = `offset` 那一行的 seq（取自索引）；第 k 个**可计行**是 `seq_base + k`
/// （口径 = 后端 `line_counts`：BOM 与全空白不算）。`max_lines` = 这一段应有的可计行数
/// —— 老后端不认 `--until` 会一路透传到 EOF，**数够就停**，多出来的不要。
/// 不可显示的记录（`is_displayable() == false`）照占号、不出 payload（与 watcher 同口径）。
pub(crate) fn range_payloads(
    lines: &[String],
    seq_base: u64,
    max_lines: u64,
    session_id: &str,
    path: &str,
    origin: &crate::origin::Origin,
) -> Vec<crate::bridge::JsonlLinePayload> {
    // 载荷上的 `origin` 字段本机不序列化、远端是机器名（与 live 行同一口径）
    let label = origin.host_name().map(str::to_string);
    let mut out = Vec::new();
    let mut numberer = LineNumberer::starting_at(seq_base);
    for l in lines {
        if numberer.next_seq() >= seq_base + max_lines {
            break;
        }
        // 〔ST3〕看不懂的行记在 `origin` 名下（本机 / 那台远端，同载荷上那个）。
        let Some((seq, rec)) = numbered_displayable(&mut numberer, l, |b| parse_line(origin, b))
        else {
            continue;
        };
        out.push(crate::bridge::JsonlLinePayload {
            session_id: session_id.to_string(),
            cwd: None,
            path: path.to_string(),
            seq,
            origin: label.clone(),
            message: rec,
        });
    }
    out
}

/// **按偏移取一段正文**：`[offset, until)`，两端取自骨架索引的行边界。
/// `seq_base` / `line_count` 也取自索引（这一段第一行的 seq、这一段的可计行数）。
#[tauri::command]
pub async fn read_session_range(
    origin: crate::origin::Origin,
    jsonl_path: String,
    offset: u64,
    until: u64,
    seq_base: u64,
    line_count: u64,
) -> Result<Vec<crate::bridge::JsonlLinePayload>, String> {
    let route = origin.route("read_session_range")?;
    precheck(&jsonl_path)?;
    let backend = Backend::for_origin(route)?;
    let argv = range_argv(&jsonl_path, offset, until);
    let argv: Vec<&str> = argv.iter().map(String::as_str).collect();
    let lines = backend.query(&argv).await?;
    let file_name = jsonl_path.rsplit(['/', '\\']).next().unwrap_or("");
    let sid = file_name.strip_suffix(".jsonl").unwrap_or(file_name);
    Ok(range_payloads(
        &lines,
        seq_base,
        line_count,
        sid,
        &jsonl_path,
        &origin,
    ))
}

/// 〔U3b · `设计/10` 步 8〕前端对这个会话**接上了骨架** ⇒ monitor 的重放缓冲里它只留尾巴
/// （`event_replay·rs::REPLAY_TAIL_KEEP`，依据写在那个常量的头注里）。返回这次丢掉的条数。
///
/// 只能由**接上骨架之后**的前端调：丢掉的正文从此只能按偏移要回来（`read_session_range`）；
/// 没骨架的会话调了它，F5 之后上翻到头就没了。前端唯一调用点在 `tabs.ts` 的骨架接入那一处。
/// 不吃 origin：重放缓冲在 monitor 本机，按 sid 找（同 `forget_session`）。
#[tauri::command]
pub fn replay_keep_tail_only(
    session_id: String,
    replay: tauri::State<'_, std::sync::Arc<crate::event_replay::EventReplay>>,
) -> Result<u32, String> {
    let dropped = replay.keep_tail_only(&session_id);
    let st = replay.stats();
    tracing::info!(
        "[replay] {session_id} 接上骨架 ⇒ 只留尾巴（这次丢 {dropped}）；history 总长 {} · 只留尾巴的会话 {} · 累计修剪 {}",
        st.history_len,
        st.tail_only_sessions,
        st.trimmed_total
    );
    Ok(u32::try_from(dropped).unwrap_or(u32::MAX))
}

#[cfg(test)]
#[path = "../../../tests/bridge/session_skeleton_tests.rs"]
mod tests;
