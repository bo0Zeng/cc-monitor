//! 〔`设计/10` 骨架 · 子步 3〕**monitor 侧接上「从偏移读」**：按偏移取一段正文。
//!
//! # 它是什么
//!
//! 后端早就有 `--read-session-from-offset <path> <offset>`（`observe/history_query.rs`），
//! 而 monitor 侧**零调用点**（`设计/10 §3.1 A1`）。本模块是它的调用点：
//!
//! | 命令 | 跑什么 | 给谁 |
//! |---|---|---|
//! | [`read_session_range`] | `--read-session-from-offset <p> <offset> --until <end>` | 前端「只物化可见区」：按索引里的行边界取**那一段**正文 |
//!
//! 走 [`crate::subagent::Backend`]（〔LOC1a〕本机与远端同一条：那台机器常驻后端的长连接，本机 = `<local>`）——
//! 「这条查询谁去跑」全仓只有那一处分流。
//!
//! 〔C4b · 第四波 4B〕**骨架索引那一条（`read_session_index`〔散文墓碑〕）不在这里了**：后端帧命令 `history-index`
//! 直接出成品，界面经通道问（`src/session-reads.ts::readSessionIndex`）；这里那一份「核头尾、剥行」随之删了。
//! 取正文这一条还留着：它的应答要过记录解析（`parse_line`，ts-rs 类型的来源）与可计行号（[`LineNumberer`]），
//! 那两样住 monitor（`frame_query_tests::HELD_BACK` 里 `history-read` 那一行的理由）。
//!
//! # 买不到
//!
//! - **续传没接到断线重连上**：重连路住 `ssh_source.rs`。续传要的两半（骨架索引的 `end` 就是续传令牌、
//!   [`read_session_range`] 就是按偏移续拉）都在，剩下的是在 `ssh_source` 的重连处把「从 seq 0 重发」换成调这两条。
//! - **整体 30s 超时**（帧面按行那一档的期限，〔LOC1a〕本机远端同一个）：弱网上超大的一段可能撞上。

use crate::copy_table::copy_text;
use crate::parser::parse_line;
use crate::subagent::Backend;

/// 按偏移取正文那条的 argv。🔴 **选项写在位置参数前面** —— 让不认它的老后端**零字节失败**：
/// 老后端只看 `args[1]`/`args[2]`，选项在前它拿 `--until` 当路径 ⇒ 围栏拒、退出 2、stdout 0 字节
/// （现打读数在后端 `FromOffsetOpts` 的头注里）。新后端两种位置都认。
pub(crate) fn range_argv(jsonl_path: &str, offset: u64, until: u64) -> Vec<String> {
    vec![
        "--read-session-from-offset".into(),
        "--until".into(),
        until.to_string(),
        jsonl_path.into(),
        offset.to_string(),
    ]
}

/// 路径的廉价预检（与 `load_subagent` / `history·rs::stream_read_session_jsonl` 同一条纪律）：
/// 真正的越权读由后端 `fence_under_projects` 兜底。
fn precheck(jsonl_path: &str) -> Result<(), String> {
    if jsonl_path.contains("..") || !jsonl_path.ends_with(".jsonl") {
        return Err(copy_text(
            "rsSessionSkeleton.precheck.badPath",
            &[("path", &jsonl_path.to_string())],
        ));
    }
    Ok(())
}

/// 〔U3b〕**monitor 侧「这一行占不占 seq」的唯一住址** —— 与后端 `history_query·rs::line_counts`
/// 同一口径（剥 BOM 再 `trim`，空了就不占号）。两个调用方：读一整份会话（`history·rs::SessionPager`，
/// 〔LOC1b · 4D〕本机远端合成一条 —— 从前是本机 / 远端两个读者各调一次）· 按偏移取正文（[`range_payloads`]）。
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

/// 〔CF2 · 第四波 4B〕按**行号**取回的那一段（前端往上翻过了账本里最老那一条时要）。
#[derive(Debug, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../../src/generated/"))]
pub struct SessionLinesPage {
    /// 这一段第一行的行号（＝ 问的那个）。
    // C03 大整数策略同 `JsonlLinePayload.seq`：量纲是行号，f64 足够；线上是 JSON 数字。
    #[cfg_attr(test, ts(type = "number"))]
    pub from: u64,
    /// 下一段从这一行起。
    #[cfg_attr(test, ts(type = "number"))]
    pub next: u64,
    /// 读过了最后一个完整行（再往后没有了）。
    pub eof: bool,
    /// `[from, next)` 里**可显示**的那些（不可显示的照占号、不出 payload —— 与实时行同一个口径）。
    pub payloads: Vec<crate::bridge::JsonlLinePayload>,
}

/// 〔CF2 · 第四波 4B〕**按行号取一段正文** `[from, until)`（`until` 缺 ＝ 到末尾）—— **不依赖骨架索引**的那条取回路。
///
/// # 为什么要有它（`调研/第四波记录/CF2.md §1`）
///
/// 按偏移取（[`read_session_range`]）要索引给字节边界；没接骨架的 tab（后台 tab · 老后端 · seq 对不上的）
/// 手里没有索引 ⇒ 丢掉的正文无处可回 ⇒ monitor 的重放缓冲不敢设上界。行号不用索引：seq **就是**行号
/// （实时 `line` 帧 · 旁路快照 · 本命令，三条路同一个空间）。前端唯一调用点在 `TabStreamView` 的上翻补批。
///
/// 解析住 monitor（`parse_line`，ts-rs 类型的来源）—— 与 `frame_query_tests::HELD_BACK` 里 `history-read`
/// 那一行同一个理由；帧命令本身（`history-lines`）后端出的是可计行原文。
///
/// 〔DL1 · `设计/05 §3.3.2`〕`left_ms` 是前端那一**件**事还剩多少（与 `chan/webview.rs::chan_call` 的 `left_ms` 同形：
/// 跨进程那一段传「还剩多少」，进来立刻换回绝对时刻）。往上翻是一件一问；会话流丢格之后往后补到末尾是一件多问 ——
/// 前端在那一件开头造一次期限、每问交剩下的（`tab-stream-view.ts::recoverFromGap`），这里不重新计时。
#[tauri::command]
pub async fn read_session_lines(
    origin: crate::origin::Origin,
    jsonl_path: String,
    from: u64,
    until: Option<u64>,
    left_ms: u64,
) -> Result<SessionLinesPage, String> {
    origin.route("read_session_lines")?;
    precheck(&jsonl_path)?;
    use crate::backend::control::frame_query::{self, Deadline};
    let page = frame_query::session_lines(
        &origin,
        &jsonl_path,
        from,
        until,
        Deadline::within(std::time::Duration::from_millis(left_ms)),
    )
    .await?;
    Ok(lines_page(page, &jsonl_path, &origin))
}

/// [`read_session_lines`] 的纯核：后端那一段 ⇒ payload（编号同 [`range_payloads`]：第 k 个可计行是 `from + k`）。
pub(crate) fn lines_page(
    page: crate::backend::control::frame_query::LinesPage,
    path: &str,
    origin: &crate::origin::Origin,
) -> SessionLinesPage {
    let file_name = path.rsplit(['/', '\\']).next().unwrap_or("");
    let sid = file_name.strip_suffix(".jsonl").unwrap_or(file_name);
    let payloads = range_payloads(
        &page.lines,
        page.from,
        page.lines.len() as u64,
        sid,
        path,
        origin,
    );
    SessionLinesPage {
        from: page.from,
        next: page.next,
        eof: page.eof,
        payloads,
    }
}

// 〔CF2 · 第四波 4B〕「接上骨架 ⇒ 重放缓冲只留尾巴」那条命令（`replay_keep_tail_only`〔散文墓碑〕）退役：
//   重放缓冲不再分档，**每个**会话都只留尾巴（`event_replay·rs` 头注「容量」），前端不必再去登记。

#[cfg(test)]
#[path = "../../../tests/bridge/session_skeleton_tests.rs"]
mod tests;
