//! 找记录与过围栏：路径限在记录树 `projects/` 之内（canonicalize 后前缀校验，防 `../` 穿越）· 按 sid 找记录 · 子运行的记录住哪 ·
//! 冷开那几问要的扫描图（过同一道围栏再问 [`crate::observe::record_scan`]）。只读：只 read_dir / read。

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
        let Some(v) = super::super::lines::parse(line.as_bytes()) else {
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
        .filter_map(|l| super::super::lines::parse(l.as_bytes()))
        .find_map(|v| faces.run_of(&v))
        .map(|m| m.run)
}

/// 按路径读一份会话之前的围栏：Claude 的 `projects/` ∪ 注册表里各家合成历史面给的记录根。
pub(crate) fn validate_session_path(
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

/// 冷开那几问共用的扫描图（过读会话那同一道围栏；没变就是留着的那张，见 [`super::super::record_scan`]）。
pub(crate) fn cold_scan(
    agent_home: &Path,
    jsonl_path: &str,
) -> Result<std::sync::Arc<super::super::record_scan::ScanMap>, String> {
    let target = validate_session_path(agent_home, jsonl_path)?;
    super::super::record_scan::global()
        .get(&target)
        .map_err(|e| format!("open failed: {e}"))
}

/// 会话事实（同一道围栏）：图在 ⇒ 照图；图被挤掉 ⇒ 接着它留下的事实尾巴读；都没有 ⇒ 现扫（见 [`super::super::record_scan::RecordScans::facts`]）。
pub(crate) fn facts(
    agent_home: &Path,
    jsonl_path: &str,
    limits: &super::super::recognize::facts::ContextLimits,
    relay: Option<bool>,
) -> Result<super::super::recognize::facts::SessionFacts, String> {
    let target = validate_session_path(agent_home, jsonl_path)?;
    super::super::record_scan::global()
        .facts(&target, limits, relay)
        .map_err(|e| format!("open failed: {e}"))
}

/// 续取用（同一道围栏）：从 `from` 起问的那一张扫描图（同 [`cold_scan`]：留着且没变 ⇒ 那一张；变长了 ⇒ 接着扫追加的那一截；
/// 不在了 ⇒ 照骨架起步或整份重扫）。`from` 越过文件尾 ⇒ 报错（截断 / 重写过，调用方从 0 重要）。
pub(crate) fn scan_from(
    agent_home: &Path,
    jsonl_path: &str,
    from: u64,
) -> Result<std::sync::Arc<super::super::record_scan::ScanMap>, String> {
    let target = validate_session_path(agent_home, jsonl_path)?;
    let len = Ro::metadata(&target)
        .map_err(|e| format!("stat failed: {e}"))?
        .len();
    if from > len {
        return Err(format!(
            "`from` {from} is past EOF ({len} bytes): file was truncated or rewritten"
        ));
    }
    super::super::record_scan::global()
        .get(&target)
        .map_err(|e| format!("open failed: {e}"))
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

/// 这条会话此刻谁持着它（[`super::super::accounts_query::session_holders`]，先过同一道围栏）；过不了围栏 / 判不了 ⇒ `None`。
pub(crate) fn facts_holders(agent_home: &Path, jsonl_path: &str) -> Option<Vec<u32>> {
    let target = validate_session_path(agent_home, jsonl_path).ok()?;
    super::super::accounts_query::session_holders(agent_home, &target)
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
/// 只答在不在、查的是哪棵树 —— 不回内容，不收别的路径。答了的「在 / 不在」交这台的会话账本（[`super::super::record_hook::seen`]：
/// 说过已结束的那条随之在「已结束 ⇄ 记录已不在」之间翻）。
pub(crate) fn record_for(
    agent_home: &Path,
    config_dir: Option<&str>,
    sid: &str,
) -> Result<RecordProbe, String> {
    let tree = match config_dir {
        None => agent_home.to_path_buf(),
        Some(d) if super::super::accounts_query::is_safe_config_dir(d) => {
            std::path::PathBuf::from(d)
        }
        Some(d) => {
            return Err(crate::common::contract::malformed(&format!(
                "configDir is not a queryable account config dir \
                 (absolute path, no `..`, no shell metacharacters): {d:?}"
            )))
        }
    };
    let probe = record_in(&tree, sid)?;
    super::super::record_hook::seen(sid, probe.present);
    Ok(probe)
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

#[cfg(test)]
#[path = "../../../../tests/backend/observe/query/locate_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../../../../tests/backend/observe/query/run_source_tests.rs"]
mod f07_tests;

#[cfg(test)]
#[path = "../../../../tests/backend/observe/query/scan_from_tests.rs"]
mod user_inputs_tests;
