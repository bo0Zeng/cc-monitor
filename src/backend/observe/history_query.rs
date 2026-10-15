//! 历史读面里记录级的那一半：尾段图 · 骨架索引 · 分页读 · 大纲 · 会话内查找的打开口 · 记录在不在。
//! 项目清单 · 会话清单 · 叫会话什么那一族在 `history/listing.rs`（10-13 挪过去，这里反过来调它）。
//! 帧面那几条（`history-*`，宿主 `faces/read_face.rs`）经这里读；
//! 一次性 `--read-session*` · `--list-*` · `--find-in-session` 那几条手写子命令删了（第 7 批第二题）。
//!
//! 安全：所有路径参数严格限制在 `<claude_dir>/projects/` 之内（canonicalize 后
//! 前缀校验，防 `../` 穿越）；project_dir 参数不允许含路径分隔符。
//! 只读铁律（cc-monitor 不写远端）在此同样成立：本模块只 read_dir / read。

use crate::common::fence::Fence;
use crate::history::listing::{fence_under_projects, records_root, NO_TREE};
use crate::platform::fs::{Ro, RoFile};
use copy_core::copy_text;
use std::path::Path;

/// 一个子运行的记录住哪：父记录 ＋（子运行是哪个 ‖ 派出它的那次工具调用）⇒（那份记录, 子运行）。只问适配层给的那几格：
/// 子运行的记录在哪几份（`ChildFace::sources`）· 一条记录属于哪个子运行（`run_of`）· 派出链接（`child_link`）。
///
/// 给的是工具调用 id ⇒ 先问子运行记录那一侧派出那一刻落盘的对上（`ChildFace::spawned`，前台派出的在跑时只有它），
/// 再在父记录里找那次调用的派出链接（只解析含这个 id 的行）；都对不上 ⇒ `not_found`。
pub(crate) fn run_source(
    agent_home: &Path,
    parent: &str,
    run: Option<&str>,
    tool: Option<&str>,
) -> Result<(std::path::PathBuf, String), (&'static str, String)> {
    let parent_path = validate_session_path(agent_home, parent).map_err(|e| ("path_refused", e))?;
    let faces = crate::agents::record_face_of(&parent_path)
        .map(|f| crate::agents::RunFaces::of(&f))
        .unwrap_or(crate::agents::RunFaces::NONE);
    let run = match (run, tool) {
        (Some(r), _) if !r.is_empty() => r.to_string(),
        (_, Some(t)) if !t.is_empty() => run_of_tool(&parent_path, &faces, t)
            .ok_or_else(|| ("not_found", copy_text("rsRun.load.notLinked", &[])))?,
        _ => {
            return Err((
                "bad_args",
                crate::common::contract::malformed("history-run needs run or tool"),
            ))
        }
    };
    for c in faces.sources(&parent_path) {
        if first_run_of(&c, &faces).as_deref() == Some(run.as_str()) {
            return Ok((c, run));
        }
    }
    Err(("not_found", copy_text("rsRun.load.notFound", &[])))
}

/// 「这次工具调用派出的是哪个子运行」：子运行记录那一侧落了盘的对上 ‖ 父记录里带了子运行的那一条派出链接。
fn run_of_tool(parent: &Path, faces: &crate::agents::RunFaces, tool: &str) -> Option<String> {
    use std::io::BufRead;
    if let Some(r) = faces
        .sources(parent)
        .iter()
        .filter_map(|c| faces.spawned(c))
        .find(|l| l.tool.as_deref() == Some(tool))
        .and_then(|l| l.run)
    {
        return Some(r);
    }
    let f = Ro::open(parent).ok()?;
    for line in std::io::BufReader::new(f).lines().map_while(Result::ok) {
        if !line.contains(tool) {
            continue;
        }
        let Ok(v) =
            serde_json::from_str::<serde_json::Value>(line.trim_start_matches('\u{feff}').trim())
        else {
            continue;
        };
        if let Some(r) = faces
            .child_links(&v)
            .into_iter()
            .find(|l| l.tool.as_deref() == Some(tool))
            .and_then(|l| l.run)
        {
            return Some(r);
        }
    }
    None
}

/// 一份子运行记录属于哪个子运行：头几条里第一条答得出的（头几条可能是不归任何运行的元数据）。
fn first_run_of(path: &Path, faces: &crate::agents::RunFaces) -> Option<String> {
    use std::io::BufRead;
    const HEAD_LINES: usize = 8;
    let f = Ro::open(path).ok()?;
    std::io::BufReader::new(f)
        .lines()
        .map_while(Result::ok)
        .take(HEAD_LINES)
        .filter_map(|l| {
            serde_json::from_str::<serde_json::Value>(l.trim_start_matches('\u{feff}').trim()).ok()
        })
        .find_map(|v| faces.run_of(&v))
        .map(|m| m.run)
}

/// 按路径读一份会话之前的围栏：Claude 的 `projects/` ∪ 注册表里各家合成历史面给的记录根。
fn validate_session_path(
    agent_home: &Path,
    jsonl_path: &str,
) -> Result<std::path::PathBuf, String> {
    validate_session_path_among(agent_home, &crate::agents::history_roots(), jsonl_path)
}

/// [`validate_session_path`] 的本体，「另外认哪几个根」是参数（判据喂临时目录，不去动进程环境）。
///
/// 历史浏览器本机远端都会列出 Codex 会话（C4d 起后端合成），而本机冷读也改走后端之后，
/// 这道围栏只认 `projects/` ⇒ 列得出、打不开。根由适配层给（`HistoryFace.root`），这里不写死路径。
/// 另外那几个根**只收绝对路径**（相对路径的意思只在 `projects/` 下有定义）；
/// 都不在 ⇒ 回 `projects/` 那一句拒绝（它是今天所有调用方认得的那一句）。
fn validate_session_path_among(
    agent_home: &Path,
    extra_roots: &[std::path::PathBuf],
    jsonl_path: &str,
) -> Result<std::path::PathBuf, String> {
    let candidate = Path::new(jsonl_path);
    let target = match fence_under_projects(agent_home, candidate) {
        Ok(t) => t,
        Err(refused) if candidate.is_absolute() => extra_roots
            .iter()
            .find_map(|r| Fence::at(r).and_then(|f| f.admit(candidate)).ok())
            .ok_or(refused)?,
        Err(refused) => return Err(refused),
    };
    if !crate::agents::is_own_session_file(&target) {
        return Err("refusing to read non-jsonl file".into());
    }
    Ok(target)
}

/// 冷开那几问共用的扫描图（过读会话那同一道围栏；没变就是留着的那张，见 [`super::record_scan`]）。
pub(crate) fn cold_scan(
    agent_home: &Path,
    jsonl_path: &str,
) -> Result<std::sync::Arc<super::record_scan::ScanMap>, String> {
    let target = validate_session_path(agent_home, jsonl_path)?;
    super::record_scan::global()
        .get(&target)
        .map_err(|e| format!("open failed: {e}"))
}

/// 续取用（同一道围栏）：扫描图留着 ⇒ 接着扫到此刻的那一张（记录又长了只读新的那一截）；没留着 ⇒ `None`
/// （分页的后几页 · 轮次 / 大纲 / 索引续取从它切，切不了就从文件那一处扫）。
pub(crate) fn held_scan(
    agent_home: &Path,
    jsonl_path: &str,
) -> Result<Option<std::sync::Arc<super::record_scan::ScanMap>>, String> {
    let target = validate_session_path(agent_home, jsonl_path)?;
    Ok(super::record_scan::global().held(&target))
}

/// 尾段那张图：扫描图留着且没变 ⇒ 从它切；否则现数一遍行（只数行、不解析，比等整遍扫描快得多 —— 冷开时界面最先要它）。
pub(crate) fn tail_now(agent_home: &Path, jsonl_path: &str, n: usize) -> Result<TailPlan, String> {
    let target = validate_session_path(agent_home, jsonl_path)?;
    if let Some(map) = super::record_scan::global().peek(&target) {
        return Ok(map.tail(n));
    }
    tail_plan(agent_home, jsonl_path, n).map(|(plan, _)| plan)
}

/// 大纲清单的打开口（围栏 ＋「起点越过文件尾 ⇒ 报错」＋ 定位）。
pub(crate) fn open_user_inputs_at(
    agent_home: &Path,
    jsonl_path: &str,
    from: u64,
) -> Result<std::io::BufReader<RoFile>, String> {
    use std::io::{Seek, SeekFrom};
    let target = validate_session_path(agent_home, jsonl_path)?;
    let mut f = Ro::open(&target).map_err(|e| format!("open failed: {e}"))?;
    let len = f.metadata().map_err(|e| format!("stat failed: {e}"))?.len();
    if from > len {
        return Err(format!(
            "`from` {from} is past EOF ({len} bytes): file was truncated or rewritten"
        ));
    }
    f.seek(SeekFrom::Start(from))
        .map_err(|e| format!("seek failed: {e}"))?;
    Ok(std::io::BufReader::new(f))
}

/// `history-facts` 的项目目录那一格：过读会话那同一道围栏，再问记录归属的那一家（只读开头）。
pub(crate) fn facts_project_dir(agent_home: &Path, jsonl_path: &str) -> Option<String> {
    let target = validate_session_path(agent_home, jsonl_path).ok()?;
    crate::agents::project_dir_of(&target)
}

/// `history-facts` 的 `agent`：这份记录是哪一家的（适配层按记录认）。
pub(crate) fn facts_agent(agent_home: &Path, jsonl_path: &str) -> Option<String> {
    let target = validate_session_path(agent_home, jsonl_path).ok()?;
    crate::agents::record_kind_of(&target).map(str::to_string)
}

/// 这条会话此刻谁持着它（[`super::accounts_query::session_holders`]，先过同一道围栏）；过不了围栏 / 判不了 ⇒ `None`。
pub(crate) fn facts_holders(agent_home: &Path, jsonl_path: &str) -> Option<Vec<u32>> {
    let target = validate_session_path(agent_home, jsonl_path).ok()?;
    super::accounts_query::session_holders(agent_home, &target)
}

/// `history-facts` 的续点：从 `from` 接着读之前先核两件事，任一不成立 ⇒ 报错（调用方从 0 重要一份）：
/// ① `from` 不越过文件尾（越过 = 截断 / 重写）；② `from > 0` 时文件第 `from-1` 字节是 `\n`
/// （续点恒是某个完整行的末字节 —— 不在行边界上 = 被重写过，接着读会从半行起、把后面的事实算歪）。
/// 挡不住的一形：重写成更长、而旧续点恰好也落在新内容的行边界上（与大纲同一个口子）。
pub(crate) fn open_facts_at(
    agent_home: &Path,
    jsonl_path: &str,
    from: u64,
) -> Result<std::io::BufReader<RoFile>, String> {
    use std::io::{Read, Seek, SeekFrom};
    let target = validate_session_path(agent_home, jsonl_path)?;
    let mut f = Ro::open(&target).map_err(|e| format!("open failed: {e}"))?;
    let len = f.metadata().map_err(|e| format!("stat failed: {e}"))?.len();
    if from > len {
        return Err(format!(
            "resume point {from} is past EOF ({len} bytes): file was truncated or rewritten"
        ));
    }
    if from > 0 {
        f.seek(SeekFrom::Start(from - 1))
            .map_err(|e| format!("seek failed: {e}"))?;
        let mut last = [0u8; 1];
        f.read_exact(&mut last)
            .map_err(|e| format!("read failed: {e}"))?;
        if last[0] != b'\n' {
            return Err(format!(
                "resume point {from} is not at a line boundary: file was rewritten"
            ));
        }
    }
    f.seek(SeekFrom::Start(from))
        .map_err(|e| format!("seek failed: {e}"))?;
    Ok(std::io::BufReader::new(f))
}

/// 过了围栏的会话文件路径（`history-find` 先拿它问常驻索引）。
pub(crate) fn session_path_at(
    agent_home: &Path,
    jsonl_path: &str,
) -> Result<std::path::PathBuf, String> {
    validate_session_path(agent_home, jsonl_path)
}

/// 过围栏、打开、定位到 `offset` —— 骨架索引与会话内查找共用这一处
/// （帧面那一臂出成品，不经 `out`，见 `read_face.rs`）。
pub(crate) fn open_session_at(
    agent_home: &Path,
    jsonl_path: &str,
    offset: u64,
) -> Result<std::io::BufReader<RoFile>, String> {
    use std::io::{Seek, SeekFrom};
    let target = validate_session_path(agent_home, jsonl_path)?;
    let mut f = Ro::open(&target).map_err(|e| format!("open failed: {e}"))?;
    f.seek(SeekFrom::Start(offset))
        .map_err(|e| format!("seek failed: {e}"))?;
    Ok(std::io::BufReader::new(f))
}

/// **骨架索引**的扫描：读 `r`（已定位在 `from`）每个可计行一条 [`IndexRow`] 交给 `on_row`，回 `(count, end)`。
/// `kind` ＝ 这份记录是哪一家的（[`index_row_of`]）。
/// `until`：只收**起点** `< until` 的行（起点在界内的那一行整行收，不劈半行）。`end` ＝ 最后一个完整行的末字节（torn 残尾不计）＝ 下一次续传的起点。
/// 帧面 `history-index`（`read_face.rs`，CLI 面 `--history-index`）用它；「这一行占不占 seq」仍只住 [`line_counts`]。
pub(crate) fn scan_session_index<R: std::io::BufRead>(
    kind: &str,
    mut r: R,
    from: u64,
    until: Option<u64>,
    mut on_row: impl FnMut(&IndexRow) -> std::io::Result<()>,
) -> std::io::Result<(u64, u64)> {
    let mut pos = from;
    let mut end = from;
    let mut count: u64 = 0;
    let mut buf: Vec<u8> = Vec::new();
    loop {
        if until.is_some_and(|u| pos >= u) {
            break;
        }
        buf.clear();
        let read = r.read_until(b'\n', &mut buf)?;
        if read == 0 || buf.last() != Some(&b'\n') {
            break; // EOF / torn 残尾不计（F14 口径）
        }
        let start = pos;
        pos += read as u64;
        end = pos;
        let body = &buf[..buf.len() - 1];
        if !line_counts(body) {
            continue; // 空行不占 seq（判定只有 `line_counts` 一个住址）
        }
        on_row(&index_row(kind, body, start, read as u64))?;
        count += 1;
    }
    Ok((count, end))
}

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
    /// 判定只住 [`crate::observe::user_inputs::user_input_of`]（与大纲清单 **同一个函数**），
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

/// 一行 → [`IndexRow`]。**纯函数**，不碰文件系统。
///
/// 解析失败（半截 / 非 JSON）**不丢这一行** —— 它仍占一个 seq，只是没有料（`t` 省略）；
/// 丢了它，后面每一行的 seq 都会错一位（那是「计数对不上而且不会报错」的那一形）。
pub(crate) fn index_row(kind: &str, line: &[u8], offset: u64, len: u64) -> IndexRow {
    index_row_of(
        kind,
        super::record_scan::parse_record(line).as_ref(),
        offset,
        len,
    )
}

/// [`index_row`] 的后半：这一行已经解析过（解析不了 ⇒ `None`，那一行照样占位）。
/// `kind` ＝ 这份记录是哪一家的（[`crate::agents::record_kind_of`]）：这一行进不进界面、谁说的都照那一家的认法判。
pub(crate) fn index_row_of(
    kind: &str,
    v: Option<&serde_json::Value>,
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
    row.u = v.get("uuid").and_then(|u| u.as_str()).map(str::to_string);
    row.t = crate::agents::record_class_of(kind, v);
    row.sc = crate::agents::run_of_record(kind, &v).is_some();
    let said = crate::agents::user_text_of(kind, &v);
    row.sp = said
        .as_ref()
        .map(|u| u.speaker.kind())
        .filter(|k| !matches!(*k, "human" | "toolResult"));
    // 大纲那一项（判定只住 `user_inputs`；这里只搬字段）
    if let Some(ui) = crate::observe::user_inputs::user_input_given(&v, said.as_ref()) {
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

/// 尾段那一趟扫描的结果：线上形住 `cmd-wire::session`（`history-tail` 的回包）。
pub(crate) use cmd_wire::session::TailPlan;

/// 扫一遍，出 [`TailPlan`] 与已打开的那份文件（调用方按区间去读）。
pub(crate) fn tail_plan(
    agent_home: &Path,
    jsonl_path: &str,
    n: usize,
) -> Result<(TailPlan, RoFile), String> {
    let target = validate_session_path(agent_home, jsonl_path)?;
    // 审计 D：整文件 std::fs::read 在 Pi 级设备上对数百 MB 会话有 OOM 风险
    // （更早的整份透传是 io::copy 流式）——改单遍流式扫描（环形缓冲只存
    // 最近 N 个可计行的字节偏移，O(N) 内存）+ 两次 seek 范围拷贝。
    let f = Ro::open(&target).map_err(|e| format!("open failed: {e}"))?;
    let mut reader = std::io::BufReader::new(f);
    let plan = tail_plan_of(&mut reader, n).map_err(|e| format!("scan failed: {e}"))?;
    Ok((plan, reader.into_inner()))
}

/// [`tail_plan`] 的扫描本体：读 `reader`（从字节 0 起）出 [`TailPlan`]。**纯 I/O 泛型**。
pub(crate) fn tail_plan_of<R: std::io::BufRead>(
    mut reader: R,
    n: usize,
) -> std::io::Result<TailPlan> {
    let mut recent: std::collections::VecDeque<u64> = std::collections::VecDeque::new();
    let keep = n.max(1);
    let mut total: u64 = 0;
    let mut pos: u64 = 0;
    let mut complete_end: u64 = 0;
    let mut buf: Vec<u8> = Vec::new();
    loop {
        buf.clear();
        let read = reader.read_until(b'\n', &mut buf)?;
        if read == 0 {
            break;
        }
        let line_start = pos;
        pos += read as u64;
        if *buf.last().unwrap() != b'\n' {
            break; // torn 残尾不计（F14 口径）
        }
        complete_end = pos;
        if !line_counts(&buf[..buf.len() - 1]) {
            continue; // 空行不计（与 watcher/monitor 口径一致 —— 判定只有 `line_counts` 一个住址）
        }
        total += 1;
        recent.push_back(line_start);
        if recent.len() > keep {
            recent.pop_front();
        }
    }
    let tail_from = total - recent.len() as u64;
    let split_at = recent.front().copied().unwrap_or(complete_end);
    Ok(TailPlan {
        total,
        tail_from,
        split_at,
        end: complete_end,
    })
}

/// 帧面按字节区间分页读一份会话的**一页**（`history-read`）。
///
/// # 为什么要分页（`C1` · 2026-09-24）
///
/// 一次性子命令把整份会话**流**回去（`io::copy`），连接关了就是 EOF；帧面是一问一答，
/// 一帧应答要整个进内存、整个过线 —— 而本仓见过 270 MB 的会话，monitor 那头单帧上限
/// 64 MiB。⇒ 调用方给 `[offset, until)`，本函数回**不超过 `page` 字节、切在行尾**的一页，
/// 外加续点 `next`；调用方循环到 `eof`。
///
/// # 切法
///
/// - 这一页之后区间里还有字节 ⇒ 切在**最后一个 `\n` 之后**，不把半行交出去；
/// - 一页里一个 `\n` 都没有（单行比 `page` 还长）⇒ 往后续读到那一行的 `\n`，
///   但**不超过 `line_cap`** —— 超了回 `oversized_line`（不许截半行冒充整行）；
/// - 区间到头（`until` 或读时的文件长度）⇒ 余下的全给，含 torn 残尾（残尾怎么处置由调用方定）。
///
/// 错误是 `(code, message)`：`refused`（围栏拒）· `failed`（读失败）· `oversized_line`
/// （不叫 `line_too_long`：那是入方向信封的**协议级** code，命令级不许撞名）。
pub(crate) fn read_page(
    agent_home: &Path,
    jsonl_path: &str,
    offset: u64,
    until: Option<u64>,
    page: usize,
    line_cap: usize,
) -> Result<ReadPage, (&'static str, String)> {
    use std::io::{Read, Seek, SeekFrom};
    let target = validate_session_path(agent_home, jsonl_path).map_err(|e| ("refused", e))?;
    let mut f = Ro::open(&target).map_err(|e| ("failed", format!("open failed: {e}")))?;
    let len = f
        .metadata()
        .map_err(|e| ("failed", format!("stat failed: {e}")))?
        .len();
    let limit = until.map_or(len, |u| u.min(len));
    if offset >= limit {
        return Ok(ReadPage {
            bytes: Vec::new(),
            next: offset,
            eof: true,
        });
    }
    f.seek(SeekFrom::Start(offset))
        .map_err(|e| ("failed", format!("seek failed: {e}")))?;
    let mut src = f.take(limit - offset);
    let mut bytes = Vec::with_capacity(page.min((limit - offset) as usize));
    (&mut src)
        .take(page as u64)
        .read_to_end(&mut bytes)
        .map_err(|e| ("failed", format!("read failed: {e}")))?;
    let reached_limit = offset + bytes.len() as u64 >= limit;
    if !reached_limit {
        match bytes.iter().rposition(|&b| b == b'\n') {
            Some(i) => bytes.truncate(i + 1),
            None => {
                // 单行比一页还长：续读到它的行尾，但不超过 `line_cap`。
                let mut more = [0u8; 64 * 1024];
                loop {
                    let k = src
                        .read(&mut more)
                        .map_err(|e| ("failed", format!("read failed: {e}")))?;
                    if k == 0 {
                        break;
                    }
                    let done = match more[..k].iter().position(|&b| b == b'\n') {
                        Some(i) => {
                            bytes.extend_from_slice(&more[..=i]);
                            true
                        }
                        None => {
                            bytes.extend_from_slice(&more[..k]);
                            false
                        }
                    };
                    // ⚠ 上限在**每一次**续读之后判，包括找到行尾的那一次 ——
                    //   只在「没找到」那一支判的话，行尾恰好落在这一块里的超长行会被整行交出去。
                    if bytes.len() > line_cap {
                        return Err((
                            "oversized_line",
                            copy_text(
                                "beHistoryQuery.readPage.lineTooLong",
                                &[
                                    ("lineCap", &line_cap.to_string()),
                                    ("offset", &offset.to_string()),
                                ],
                            ),
                        ));
                    }
                    if done {
                        break;
                    }
                }
            }
        }
    }
    let next = offset + bytes.len() as u64;
    Ok(ReadPage {
        bytes,
        next,
        eof: next >= limit,
    })
}

/// [`read_page`] 的一页。
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct ReadPage {
    /// 原始字节（调用方负责解码 —— 帧面那层用 `from_utf8_lossy`）。
    pub bytes: Vec<u8>,
    /// 下一页从这里起。
    pub next: u64,
    /// 区间到头了。
    pub eof: bool,
}

/// 帧面按**行号**取回一段（`history-lines`）：第 `[from, until)` 个可计行的原文。
///
/// # 为什么要有它
///
/// 按字节取正文（[`read_page`]）要调用方先知道字节边界 —— 那只有骨架索引给得出。
/// 没接骨架的会话（后台 tab · 老后端 · seq 对不上的）丢掉的正文就无处可回 ⇒ monitor 的重放缓冲
/// 不敢设上界（级 3）。本函数是**不依赖索引**的那条取回路：调用方只说行号。
///
/// # 口径
///
/// - 「第几行」只有一个家：[`line_counts`]（BOM 与全空白不占号）＋ 没 `\n` 收尾的残尾不计 ——
///   与 [`tail_plan`] 的 `total` · [`scan_session_index`] 的行号 · 实时 `line` 帧的 `seq` 同一个空间。
/// - 只交**可计行**（空行在号外，交出去调用方也不会给它编号）；第 k 条就是第 `from + k` 行。
/// - `until` 缺 ＝ 到最后一个完整行为止。
///
/// # 一帧装得下
///
/// 交出的原文累计达到 `page` 字节就停（至少交一行）；单行超过 `line_cap` ⇒ `oversized_line`
/// （同 [`read_page`]：不截半行）。停在中途时 `eof == false`、`next < until`，调用方接着要。
///
/// # 代价（写清楚）
///
/// **O(第 `from` 行之前的字节)**：后端零状态，每次从文件头数。本机最大一份会话（111 MB / 4.2 万行）
/// 热缓存从头数到尾 ≈ 22 ms；取回只在用户往上翻过了前端手里最老那一条时才发生，一次一批。
/// 真撞上「弱设备上超大会话翻得卡」的读数时再加锚，不先加。
///
/// 错误同 [`read_page`]：`refused` · `failed` · `oversized_line`。
pub(crate) fn read_lines(
    agent_home: &Path,
    jsonl_path: &str,
    from: u64,
    until: Option<u64>,
    page: usize,
    line_cap: usize,
) -> Result<LinesPage, (&'static str, String)> {
    let target = validate_session_path(agent_home, jsonl_path).map_err(|e| ("refused", e))?;
    let f = Ro::open(&target).map_err(|e| ("failed", format!("open failed: {e}")))?;
    read_lines_from(std::io::BufReader::new(f), from, until, page, line_cap)
}

/// [`read_lines`] 的内核：读 `r`（从文件头起）。**纯 I/O 泛型**，单测直接喂字节。
pub(crate) fn read_lines_from<R: std::io::BufRead>(
    mut r: R,
    from: u64,
    until: Option<u64>,
    page: usize,
    line_cap: usize,
) -> Result<LinesPage, (&'static str, String)> {
    let until = until.unwrap_or(u64::MAX);
    let mut lines: Vec<String> = Vec::new();
    let mut starts: Vec<u64> = Vec::new();
    let mut bytes: usize = 0;
    let mut n: u64 = 0; // 下一个可计行的行号
    let mut pos: u64 = 0; // 这一行的起点字节偏移
    let mut buf: Vec<u8> = Vec::new();
    let eof = loop {
        if n >= until {
            break false;
        }
        buf.clear();
        let start = pos;
        let read = r
            .read_until(b'\n', &mut buf)
            .map_err(|e| ("failed", format!("scan failed: {e}")))?;
        pos += read as u64;
        if read == 0 || buf.last() != Some(&b'\n') {
            break true; // 文件到头；torn 残尾不计（同 `tail_plan`）
        }
        let body = &buf[..buf.len() - 1];
        if !line_counts(body) {
            continue;
        }
        let at = n;
        n += 1;
        if at < from {
            continue;
        }
        if body.len() > line_cap {
            return Err((
                "oversized_line",
                copy_text(
                    "beHistoryQuery.readLinesFrom.lineTooLong",
                    &[("at", &at.to_string()), ("lineCap", &line_cap.to_string())],
                ),
            ));
        }
        bytes += body.len();
        lines.push(String::from_utf8_lossy(body).into_owned());
        starts.push(start);
        if bytes >= page {
            break false;
        }
    };
    let next = from + lines.len() as u64;
    Ok(LinesPage {
        from,
        lines,
        starts,
        next,
        eof,
    })
}

/// [`read_lines`] 的一段。不变量：`next == from + lines.len()`。
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct LinesPage {
    /// 第一条的行号（＝ 请求里的 `from`）。
    pub from: u64,
    /// 可计行的原文（不含行尾 `\n`；`\r` 与 BOM 原样留着，调用方剥）。
    pub lines: Vec<String>,
    /// 每一条在文件里的起点字节偏移（与 `lines` 逐条对齐）。
    pub starts: Vec<u64>,
    /// 下一段从这一行起。
    pub next: u64,
    /// 读到了最后一个完整行之后（后面没有了）。
    pub eof: bool,
}

/// 「这条会话的记录还在不在」—— `history-record` 帧命令的本体（`read_face` 是它的宿主）。
///
/// 最后一条逐字：「对方那份记录也没了 ⇒ 重开必失败，要诚实报错，不许静默变成『起了个新会话』」。
/// 前端 resume 一跳（`tab-session-actions.ts`）在开终端之前问一次；答「不在」就不开。
///
/// - **只收 sid、不收路径**（`INVARIANTS §41.6` 收窄第 3 条）：找文件那一步是两侧共用的
///   `agents::find_session_file`（经注册表那一格，本体住 `agents/claudecode/branch.rs`；与分叉 / 删会话同一份；符号链接不算命中）。
/// - sid 形状不合法 ⇒ `Err`，**先于任何 IO**；找不到 ⇒ `Ok(present = false)` —— 这是一个答案，不是错误。
/// - 回 `root`（查的是哪棵记录树）：报错时要说清查了什么。
///
/// ⚠ **射程如实写**：它查的是**这台后端**的记录树（`agent_home()/projects`）。会话若起在另一个
/// 账号的配置根下（`CLAUDE_CONFIG_DIR` 指向别处），这里答「不在」而那边其实有 —— 调用方拿到「不在」
/// 时报的话要说清是「这棵树里没有」，不是「世上没有」。
pub(crate) fn record_in(agent_home: &Path, sid: &str) -> Result<RecordProbe, String> {
    if !shell_quote_core::session_id_ok(sid) {
        return Err(crate::common::contract::malformed(&format!(
            "bad session id shape (letters, digits, hyphen; 1..=64): {sid:?}"
        )));
    }
    let Some(root) = records_root(agent_home) else {
        return Err(NO_TREE.to_string());
    };
    let present = crate::agents::find_session_file(&root, sid).is_ok();
    Ok(RecordProbe {
        present,
        root: root.to_string_lossy().into_owned(),
    })
}

/// 这台记录树里那条会话的记录文件（同 [`record_in`] 那一找）。
pub(crate) fn session_record(agent_home: &Path, sid: &str) -> Result<std::path::PathBuf, String> {
    crate::agents::find_session_file(&records_root(agent_home).ok_or(NO_TREE)?, sid)
}

/// [`record_in`] 按**这次 resume 要用的那个账号配置目录**查（`history-record` 的 `configDir`）。
///
/// 会话起在另一个账号根下（`CLAUDE_CONFIG_DIR` 指别处）时，只查这台后端自己的家目录会答「不在」、误拦 resume
/// （上面那段「射程如实写」）。monitor 把这次 resume 交给起会话那一格的同一个目录带过来，
/// 这里就在那棵树里找。
/// - `None` ⇒ 这台的家目录（与改之前逐字同一问）；
/// - `Some(d)` ⇒ 先过账号库那一个形状关（`accounts_query::is_safe_config_dir`：绝对 · 不上跳 · 无 shell 元字符与
///   欺骗字符），不过 ⇒ `Err`，**一次 IO 都不做**；过了 ⇒ 查 `<d>/projects`。
/// 只答在不在、查的是哪棵树 —— 不回内容，不收别的路径。
pub(crate) fn record_for(
    agent_home: &Path,
    config_dir: Option<&str>,
    sid: &str,
) -> Result<RecordProbe, String> {
    let tree = match config_dir {
        None => agent_home.to_path_buf(),
        Some(d) if super::accounts_query::is_safe_config_dir(d) => std::path::PathBuf::from(d),
        Some(d) => {
            return Err(crate::common::contract::malformed(&format!(
                "configDir is not a queryable account config dir \
                 (absolute path, no `..`, no shell metacharacters): {d:?}"
            )))
        }
    };
    record_in(&tree, sid)
}

/// [`record_in`] 的答案。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct RecordProbe {
    /// `<sid>.jsonl` 在记录树里（根那一层或项目目录那一层）找得到。
    pub present: bool,
    /// 查的那棵记录树的根。
    pub root: String,
}

/// 这一行**算不算一行**（＝ 口径 `watcher::read_new_lines`：BOM 与全空白跳过）。
///
/// 🔴 **全仓只有这一个住址** —— 扫描循环数 `total`、`split_tail` 数 `starts`，
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
pub(crate) fn line_counts(line: &[u8]) -> bool {
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

/// 纯函数：把文件字节按"最新 N 可计行优先"切成 (meta 行, 尾段, 头段)。
/// 只处理到最后一个 `\n`（torn 残尾不进任何段——F14 口径）。
/// 生产路径已流式化（read_session_tail，审计 D 内存修订）；本函数保留为
/// 口径锚点（tail_tests 锚定语义），流式版与它的等价性由本机行为验证对账
/// （真实 18MB 会话：meta/字节输出逐段一致，见 Batch9 feature 留档）。
/// **仅测**（生产走流式版、不调本函数）→ `#[cfg(test)]` 不进生产二进制。
#[cfg(test)]
fn split_tail(bytes: &[u8], n: usize) -> (String, &[u8], &[u8]) {
    let complete_end = bytes.iter().rposition(|&b| b == b'\n').map_or(0, |i| i + 1);
    let complete = &bytes[..complete_end];
    // 收集每个可计行的起始字节偏移（口径 = watcher::read_new_lines：BOM/全空白跳过）
    let mut starts: Vec<usize> = Vec::new();
    let mut pos = 0usize;
    for line in complete.split_inclusive(|&b| b == b'\n') {
        if line_counts(&line[..line.len() - 1]) {
            starts.push(pos);
        }
        pos += line.len();
    }
    let total = starts.len();
    let tail_from = total.saturating_sub(n.max(1));
    let meta =
        format!("{{\"kind\":\"snapshot_meta\",\"total\":{total},\"tail_from\":{tail_from}}}\n");
    let split_at = starts.get(tail_from).copied().unwrap_or(complete_end);
    (meta, &complete[split_at..], &complete[..split_at])
}

#[cfg(test)]
#[path = "../../../tests/backend/observe/history_query_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../../../tests/backend/observe/history_query_tail_tests.rs"]
mod tail_tests;

#[cfg(test)]
#[path = "../../../tests/backend/observe/history_query_f07_tests.rs"]
mod f07_tests;

#[cfg(test)]
#[path = "../../../tests/backend/observe/history_query_index_tests.rs"]
mod index_tests;

#[cfg(test)]
#[path = "../../../tests/backend/observe/history_query_user_inputs_tests.rs"]
mod user_inputs_tests;
