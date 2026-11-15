//! 找记录与过围栏：路径限在记录树 `projects/` 之内（canonicalize 后前缀校验，防 `../` 穿越）· 按 sid 找记录 · 子运行的记录住哪 ·
//! 冷开那几问要的扫描图（过同一道围栏再问 [`crate::observe::record_scan`]）。只读：只 read_dir / read。

use crate::agents::AgentHome;
use crate::common::fence::Fence;
use crate::common::said::Said;
use crate::history::listing::NO_TREE;
use crate::platform::fs::{Ro, RoFile};
use copy_core::copy_text;
use std::path::Path;

/// 一个子运行的记录住哪：父记录 ＋（子运行是哪个 ‖ 派出它的那次工具调用）⇒（那份记录, 子运行）。只问适配层给的那几格：
/// 子运行的记录在哪几份（`ChildFace::sources`）· 一条记录属于哪个子运行（`run_of`）· 派出链接（`child_link`）。
///
/// 给的是工具调用 id ⇒ 先问子运行记录那一侧派出那一刻落盘的对上（`ChildFace::spawned`，前台派出的在跑时只有它），
/// 再在父记录里找那次调用的派出链接（只解析含这个 id 的行）；都对不上 ⇒ `not_found`。
pub fn run_source(
    agent_home: impl crate::agents::IntoHomes,
    parent: &str,
    run: Option<&str>,
    tool: Option<&str>,
) -> Result<(std::path::PathBuf, String), Said> {
    let agent_home_v = crate::agents::IntoHomes::into_homes(agent_home);
    let agent_home: &[crate::agents::AgentHome] = &agent_home_v;
    let parent_path =
        validate_session_path(agent_home, parent).map_err(|e| Said::new("path_refused", e))?;
    let faces = crate::agents::record_face_of(&parent_path)
        .map(|f| crate::agents::RunFaces::of(&f))
        .unwrap_or(crate::agents::RunFaces::NONE);
    let run = match (run, tool) {
        (Some(r), _) if !r.is_empty() => r.to_string(),
        (_, Some(t)) if !t.is_empty() => run_of_tool(&parent_path, &faces, t)
            .ok_or_else(|| Said::new("not_found", copy_text("rsRun.load.notLinked", &[])))?,
        _ => {
            return Err(Said::new(
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
    Err(Said::new(
        "not_found",
        copy_text("rsRun.load.notFound", &[]),
    ))
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

/// 按路径读一份会话之前的围栏：家目录记录树的 `projects/` ∪ 根表上各家合成历史面的那几个根（[`crate::agents::history_roots_among`]）。
pub(crate) fn validate_session_path(
    agent_home: impl crate::agents::IntoHomes,
    jsonl_path: &str,
) -> Result<std::path::PathBuf, String> {
    let agent_home_v = crate::agents::IntoHomes::into_homes(agent_home);
    let agent_home: &[crate::agents::AgentHome] = &agent_home_v;
    validate_session_path_among(crate::agents::REGISTRY, agent_home, jsonl_path)
}

/// [`validate_session_path`] 的可喂夹具那一半：另外认哪几个根 · 认不认是一份会话记录都照 `registry` 问。
///
/// 历史清单列得出的合成历史那几家的会话（各号的家 · 日期树与归档根都算）要经同一道围栏打得开。根由适配层照各号的家给
/// （`HistoryFace.roots`，经根表），这里不写死路径。另外那几个根**只收绝对路径**（相对路径的意思只在 `projects/` 下有定义）；
/// 都不在 ⇒ 回 `projects/` 那一句拒绝（它是今天所有调用方认得的那一句）。
fn validate_session_path_among(
    registry: &[crate::agents::Adapter],
    agent_home: impl crate::agents::IntoHomes,
    jsonl_path: &str,
) -> Result<std::path::PathBuf, String> {
    admitted_among(registry, agent_home, jsonl_path).map(|(_, p)| p)
}

/// [`validate_session_path_among`] 的本体：连放行它的那一根那一家一起交回（家目录记录树 ⇒ 那个家那一家；合成历史的根 ⇒ 根表上那一根那一家）。
fn admitted_among(
    registry: &[crate::agents::Adapter],
    agent_home: impl crate::agents::IntoHomes,
    jsonl_path: &str,
) -> Result<(&'static str, std::path::PathBuf), String> {
    let agent_home_v = crate::agents::IntoHomes::into_homes(agent_home);
    let agent_home: &[crate::agents::AgentHome] = &agent_home_v;
    let candidate = Path::new(jsonl_path);
    let (kind, target) = match crate::history::listing::fenced(agent_home, candidate) {
        Ok((home, t)) => (home.kind, t),
        Err(refused) if candidate.is_absolute() => crate::agents::history_roots_among(registry)
            .iter()
            .find_map(|(kind, r)| {
                Fence::at(r)
                    .and_then(|f| f.admit(candidate))
                    .ok()
                    .map(|t| (*kind, t))
            })
            .ok_or(refused)?,
        Err(refused) => return Err(refused),
    };
    if !crate::agents::record_face_among(registry, &target)
        .is_some_and(|r| (r.is_session_file)(&target))
    {
        return Err("refusing to read non-jsonl file".into());
    }
    Ok((kind, target))
}

/// 过围栏的那一份与它归的那一家（放行它的那一根上取的，扫描图照它读，[`super::super::record_scan::Family`]）。
fn scan_target(
    agent_home: &[AgentHome],
    jsonl_path: &str,
) -> Result<
    (
        std::path::PathBuf,
        Option<super::super::record_scan::Family>,
    ),
    String,
> {
    let (kind, target) = admitted_among(crate::agents::REGISTRY, agent_home, jsonl_path)?;
    Ok((target, super::super::record_scan::Family::of(kind)))
}

/// 冷开那几问共用的扫描图（过读会话那同一道围栏；没变就是留着的那张，见 [`super::super::record_scan`]）。
pub fn cold_scan(
    agent_home: impl crate::agents::IntoHomes,
    jsonl_path: &str,
) -> Result<std::sync::Arc<super::super::record_scan::ScanMap>, String> {
    let agent_home_v = crate::agents::IntoHomes::into_homes(agent_home);
    let agent_home: &[crate::agents::AgentHome] = &agent_home_v;
    let (target, family) = scan_target(agent_home, jsonl_path)?;
    super::super::record_scan::global()
        .get(&target, family)
        .map_err(|e| format!("open failed: {e}"))
}

/// 按行号取正文的起点：过围栏的那一份（围栏只在这里校一次，读的一方照它交回的路径开）与第 `want` 个可计行的前一行从哪开始、
/// 它是第几行（[`super::super::record_scan::RecordScans::seek_line`]）。围栏拒 ⇒ `Err(Refused)`；图取不到 ⇒ `Err(Failed)`。
pub fn line_seek(
    agent_home: impl crate::agents::IntoHomes,
    jsonl_path: &str,
    want: u64,
) -> Result<(std::path::PathBuf, (u64, u64)), SeekError> {
    let agent_home_v = crate::agents::IntoHomes::into_homes(agent_home);
    let (target, family) = scan_target(&agent_home_v, jsonl_path).map_err(SeekError::Refused)?;
    let at = super::super::record_scan::global()
        .seek_line(&target, family, want)
        .map_err(|e| SeekError::Failed(format!("open failed: {e}")))?;
    Ok((target, at))
}

/// [`line_seek`] 拒在哪一步：围栏不放行 · 放行了但读不了。
#[derive(Debug)]
pub enum SeekError {
    Refused(String),
    Failed(String),
}

/// 会话事实（同一道围栏）：图在 ⇒ 照图；图被挤掉 ⇒ 接着它留下的事实尾巴读；都没有 ⇒ 现扫（见 [`super::super::record_scan::RecordScans::facts`]）。
pub fn facts(
    agent_home: impl crate::agents::IntoHomes,
    jsonl_path: &str,
    limits: &super::super::recognize::facts::ContextLimits,
    relay: Option<bool>,
) -> Result<super::super::recognize::facts::SessionFacts, String> {
    let agent_home_v = crate::agents::IntoHomes::into_homes(agent_home);
    let agent_home: &[crate::agents::AgentHome] = &agent_home_v;
    let (target, family) = scan_target(agent_home, jsonl_path)?;
    super::super::record_scan::global()
        .facts(&target, family, limits, relay)
        .map_err(|e| format!("open failed: {e}"))
}

/// 会话事实照这台活会话表配的那两样（`facts_in` 与 [`live_line`] 共用这一处）：pidfile 此刻说在等 ⇒ 配上记录里没结果的那一步判种类，
/// 交回那一份需手动与何时起等（[`Needs`](super::super::recognize::facts::Needs)，还没配待答请求；调用方写进 `needs` 或交给状态一句）；
/// 说一轮停了、后台命令还在跑 ⇒ 写好后台那一句（`background`）。都不累加，每次现查。
pub fn dress_live(
    facts: &mut super::super::recognize::facts::SessionFacts,
    live: &crate::history::live_table::LiveTable,
    sid: &str,
    path: &Path,
    now_ms: u64,
) -> Option<super::super::recognize::facts::Needs> {
    use super::super::recognize::facts;
    facts.background = live
        .background_since(sid)
        .map(|born| facts::background_of(&facts.bg_tasks, born, now_ms));
    facts::needs_of(
        crate::agents::record_kind_of(path),
        &facts.pending,
        live.wait_of(sid).as_ref(),
    )
}

/// **一条活会话此刻的状态一句**（定格的字 · 会走的那一截 · 悬停句；句子只由 [`super::super::recognize::facts::line_of`] 写）：
/// `activity` ＝ 造脸那一处判好的那一态（不活 ⇒ `None` ⇒ 没有这一句，不读记录）；记录照 `history-facts` 读的同一份扫描图（[`facts`]，过同一道围栏）；
/// 需手动的种类与后台那一句照调用方给的活会话表配（[`dress_live`]）。记录读不了 ⇒ `None`。读者：watcher 造脸那一处。
pub(crate) fn live_line(
    agent_home: impl crate::agents::IntoHomes,
    sid: &str,
    jsonl_path: &str,
    activity: Option<crate::agents::SessionActivity>,
    live: &crate::history::live_table::LiveTable,
) -> Option<crate::wire::StateLine> {
    let agent_home_v = crate::agents::IntoHomes::into_homes(agent_home);
    let agent_home: &[crate::agents::AgentHome] = &agent_home_v;
    let activity = activity?;
    let limits = super::super::recognize::facts::ContextLimits::default();
    let mut f = facts(agent_home, jsonl_path, &limits, None).ok()?;
    let hung = crate::history::needs_changed::pending_of(sid)
        .into_iter()
        .next();
    line_with(
        activity,
        &mut f,
        live,
        sid,
        Path::new(jsonl_path),
        hung,
        crate::common::time::now_ms(),
    )
}

/// [`live_line`] 读完记录之后那一截：照活动表配好在等的那一份，再配上挂着的那一条请求（[`dress_ask_with`](super::super::recognize::facts::dress_ask_with)，
/// 同会话事实那一处），状态一句照配好的那一份写（脸上的点分不分待答请求只看它交没交 `ask`）。`hung` ＝ 这条会话最早挂上的那一条。
pub(crate) fn line_with(
    activity: crate::agents::SessionActivity,
    f: &mut super::super::recognize::facts::SessionFacts,
    live: &crate::history::live_table::LiveTable,
    sid: &str,
    path: &Path,
    hung: Option<crate::agents::asks::Asked>,
    now_ms: u64,
) -> Option<crate::wire::StateLine> {
    use super::super::recognize::facts;
    let needs = facts::dress_ask_with(dress_live(f, live, sid, path, now_ms), hung, now_ms);
    facts::line_of(Some(activity), f, needs.as_ref())
}

/// 续取用（同一道围栏）：从 `from` 起问的那一张扫描图（同 [`cold_scan`]：留着且没变 ⇒ 那一张；变长了 ⇒ 接着扫追加的那一截；
/// 不在了 ⇒ 照骨架起步或整份重扫）。`from` 越过文件尾 ⇒ 报错（截断 / 重写过，调用方从 0 重要）。
pub(crate) fn scan_from(
    agent_home: impl crate::agents::IntoHomes,
    jsonl_path: &str,
    from: u64,
) -> Result<std::sync::Arc<super::super::record_scan::ScanMap>, String> {
    let agent_home_v = crate::agents::IntoHomes::into_homes(agent_home);
    let agent_home: &[crate::agents::AgentHome] = &agent_home_v;
    let (target, family) = within(agent_home, jsonl_path, from)?;
    super::super::record_scan::global()
        .get(&target, family)
        .map_err(|e| format!("open failed: {e}"))
}

/// 续取的那两条（[`scan_from`] · [`index_rows`]）共用：过围栏、`from` 不越过文件尾。
fn within(
    agent_home: &[AgentHome],
    jsonl_path: &str,
    from: u64,
) -> Result<
    (
        std::path::PathBuf,
        Option<super::super::record_scan::Family>,
    ),
    String,
> {
    let (target, family) = scan_target(agent_home, jsonl_path)?;
    let len = Ro::metadata(&target)
        .map_err(|e| format!("stat failed: {e}"))?
        .len();
    if from > len {
        return Err(format!(
            "`from` {from} is past EOF ({len} bytes): file was truncated or rewritten"
        ));
    }
    Ok((target, family))
}

/// 骨架索引从字节 `from` 起的行（同一道围栏；照图切还是只读那一截见 [`super::super::record_scan::RecordScans::index_rows`]）。
/// `from` 越过文件尾 ⇒ 报错（截断 / 重写过，调用方从 0 重要）。
pub(crate) fn index_rows(
    agent_home: impl crate::agents::IntoHomes,
    jsonl_path: &str,
    from: u64,
    take: impl FnMut(&super::super::recognize::index_row::IndexRow) -> bool,
) -> Result<u64, String> {
    let agent_home_v = crate::agents::IntoHomes::into_homes(agent_home);
    let agent_home: &[crate::agents::AgentHome] = &agent_home_v;
    let (target, family) = within(agent_home, jsonl_path, from)?;
    super::super::record_scan::global()
        .index_rows(&target, family, from, take)
        .map_err(|e| format!("open failed: {e}"))
}

/// `history-facts` 的项目目录那一格：过读会话那同一道围栏，再问记录归属的那一家（只读开头）。
pub fn facts_project_dir(
    agent_home: impl crate::agents::IntoHomes,
    jsonl_path: &str,
) -> Option<String> {
    // 围栏与归属同一份注册表（生产那一份只在这里取）。
    let registry = crate::agents::REGISTRY;
    let agent_home_v = crate::agents::IntoHomes::into_homes_among(agent_home, registry);
    let agent_home: &[crate::agents::AgentHome] = &agent_home_v;
    let target = validate_session_path_among(registry, agent_home, jsonl_path).ok()?;
    crate::agents::project_dir_among(registry, &target)
}

/// `history-facts` 的 `agent`：这份记录是哪一家的（适配层按记录认）。
pub fn facts_agent(agent_home: impl crate::agents::IntoHomes, jsonl_path: &str) -> Option<String> {
    let agent_home_v = crate::agents::IntoHomes::into_homes(agent_home);
    let agent_home: &[crate::agents::AgentHome] = &agent_home_v;
    let target = validate_session_path(agent_home, jsonl_path).ok()?;
    crate::agents::record_kind_of(&target).map(str::to_string)
}

/// 这条会话此刻谁持着它（[`super::super::accounts_query::session_holders`]，先过同一道围栏）；过不了围栏 / 判不了 ⇒ `None`。
pub fn facts_holders(
    agent_home: impl crate::agents::IntoHomes,
    jsonl_path: &str,
) -> Option<Vec<u32>> {
    let agent_home_v = crate::agents::IntoHomes::into_homes(agent_home);
    let agent_home: &[crate::agents::AgentHome] = &agent_home_v;
    let target = validate_session_path(agent_home, jsonl_path).ok()?;
    super::super::accounts_query::session_holders(agent_home, &target)
}

/// 过了围栏的会话文件路径（`history-find` 先拿它问常驻索引）。
pub fn session_path_at(
    agent_home: impl crate::agents::IntoHomes,
    jsonl_path: &str,
) -> Result<std::path::PathBuf, String> {
    let agent_home_v = crate::agents::IntoHomes::into_homes(agent_home);
    let agent_home: &[crate::agents::AgentHome] = &agent_home_v;
    validate_session_path(agent_home, jsonl_path)
}

/// 过围栏、打开、定位到 `offset` —— 骨架索引与会话内查找共用这一处
/// （帧面那一臂出成品，不经 `out`，见 `read_face.rs`）。
pub fn open_session_at(
    agent_home: impl crate::agents::IntoHomes,
    jsonl_path: &str,
    offset: u64,
) -> Result<std::io::BufReader<RoFile>, String> {
    let agent_home_v = crate::agents::IntoHomes::into_homes(agent_home);
    let agent_home: &[crate::agents::AgentHome] = &agent_home_v;
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
/// - **只收 sid、不收路径**（`INVARIANTS §41.6` 收窄第 3 条）：找文件那一步是 `agents::find_session`
///   （逐家在自己的记录根下找，与分叉同一份；符号链接不算命中）。
/// - sid 形状不合法 ⇒ `Err`，**先于任何 IO**；找不到 ⇒ `Ok(present = false)` —— 这是一个答案，不是错误。
/// - 回 `root`：找到 ⇒ 那份所在的记录根；找不到 ⇒ 记录树的根（报错时要说清查了什么）。
///
/// ⚠ **射程如实写**：记录树那几家查的是**这台后端**上各自的记录树（`agent_home` 交来的那几家的家）。会话若起在另一个
/// 账号的配置根下（`CLAUDE_CONFIG_DIR` 指向别处），这里答「不在」而那边其实有 —— 调用方拿到「不在」
/// 时报的话要说清是「这棵树里没有」，不是「世上没有」。
pub(crate) fn record_in(
    agent_home: impl crate::agents::IntoHomes,
    sid: &str,
) -> Result<RecordProbe, String> {
    let agent_home_v = crate::agents::IntoHomes::into_homes(agent_home);
    let agent_home: &[crate::agents::AgentHome] = &agent_home_v;
    if !shell_quote_core::session_id_ok(sid) {
        return Err(crate::common::contract::malformed(&format!(
            "bad session id shape (letters, digits, hyphen; 1..=64): {sid:?}"
        )));
    }
    let (present, root) = match crate::agents::find_session_in(agent_home, sid) {
        Ok(at) => (true, at.root.to_string_lossy().into_owned()),
        Err(_) => {
            let roots: Vec<String> = agent_home
                .iter()
                .filter_map(AgentHome::records_root)
                .map(|r| r.to_string_lossy().into_owned())
                .collect();
            if roots.is_empty() {
                return Err(NO_TREE.to_string());
            }
            (false, roots.join(", "))
        }
    };
    Ok(RecordProbe { present, root })
}

/// 这台那条会话的记录文件（同 [`record_in`] 那一找，哪一家的都算）。
pub fn session_record(
    agent_home: impl crate::agents::IntoHomes,
    sid: &str,
) -> Result<std::path::PathBuf, String> {
    session_at(agent_home, sid).map(|at| at.file)
}

/// 按 sid 找到的那一条（[`crate::agents::SessionAt`]：哪一家 · 那个号的家 · 记录根 · 文件）。认家照这一下找到它的那一家，不照路径另猜。
pub fn session_at(
    agent_home: impl crate::agents::IntoHomes,
    sid: &str,
) -> Result<crate::agents::SessionAt, String> {
    let agent_home_v = crate::agents::IntoHomes::into_homes(agent_home);
    let agent_home: &[crate::agents::AgentHome] = &agent_home_v;
    crate::agents::find_session_in(agent_home, sid)
}

/// [`record_in`] 按**这次 resume 要用的那个账号配置目录**查（`history-record` 的 `configDir`）。
///
/// 会话起在另一个账号根下（`CLAUDE_CONFIG_DIR` 指别处）时，只查这台后端自己的家目录会答「不在」、误拦 resume
/// （上面那段「射程如实写」）。monitor 把这次 resume 交给起会话那一格的同一个目录带过来，
/// 这里就在那棵树里找。
/// - `None` ⇒ 这台的家目录（与改之前逐字同一问）；
/// - `Some(d)` ⇒ 先过账号库那一个形状关（`accounts_query::is_safe_config_dir`：绝对 · 不上跳 · 无 shell 元字符与
///   欺骗字符），不过 ⇒ `Err`，**一次 IO 都不做**；过了 ⇒ 查 `<d>/projects`。
///
/// 只答在不在、查的是哪棵树 —— 不回内容，不收别的路径。答了的「在 / 不在」交这台的会话账本（[`super::super::record_hook::seen`]：
/// 说过已结束的那条随之在「已结束 ⇄ 记录已不在」之间翻）。
pub fn record_for(
    agent_home: impl crate::agents::IntoHomes,
    config_dir: Option<&str>,
    sid: &str,
) -> Result<RecordProbe, String> {
    let agent_home_v = crate::agents::IntoHomes::into_homes(agent_home);
    let agent_home: &[crate::agents::AgentHome] = &agent_home_v;
    let tree: Vec<AgentHome> = match config_dir {
        None => agent_home.to_vec(),
        Some(d) if super::super::accounts_query::is_safe_config_dir(d) => agent_home
            .iter()
            .filter_map(|h| {
                Some(AgentHome {
                    kind: h.kind,
                    home: crate::agents::home_of(h.kind, Some(Path::new(d)), false)?,
                })
            })
            .collect(),
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
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct RecordProbe {
    /// 那条会话的记录在这台找得到（哪一家的都算）。
    pub present: bool,
    /// 找到 ⇒ 那份所在的记录根；找不到 ⇒ 查的那棵记录树的根。
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
