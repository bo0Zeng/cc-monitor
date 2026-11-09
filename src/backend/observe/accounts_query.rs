//! 观测面里问活会话的那几问：谁持着一条会话（[`session_holders`]）· 每个活会话此刻在干什么（[`live_doing`]）·
//! 需手动的会话（[`live_needs`]）· 这台活会话里断开的 MCP（[`live_mcp_failed`]）· 配置目录那一个形状关（`is_safe_config_dir`）。
//!
//! 不认识账号库：账号清单成品与会话归属哪个号住 `accounts::manage::library`。
//!
//! # 只读（src/doc/INVARIANTS.md §1）
//!
//! 只读活会话表、记录与注解，零写入，不 shell out。

use crate::history::live_table::LiveTable;
use crate::platform::fs::Ro;
use crate::wire::SessionFaceExt as _;
pub use cmd_wire::needs::NeedsRow;
use std::path::{Path, PathBuf};

/// 路径是否可安全地交给下游。规则是 `acct_core::config_dir_ok`（全仓唯一一份，认 Windows 形：`\` 是那边的路径分隔符）；
/// 拼进 POSIX 命令之前另要过 `acct-core` 的 posix 那一条（它拒 `\`）—— 分层校验。
/// 允许普通空格与常规非 ASCII（如中文），拒绝引号 / 命令替换 / 重定向 / 通配 / 控制字符 + 视觉欺骗类 Unicode。
pub(crate) fn is_safe_config_dir(p: &str) -> bool {
    // 规则住共享 crate（`acct_core::config_dir_ok`）：后端 `control/ccm` 起会话也要这张全表，而 `control → observe` 是禁止方向。
    acct_core::config_dir_ok(p)
}

/// 这台活会话里那一家说断开的一个 MCP 服务器（[`live_mcp_failed`] 的一项）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveMcpFailed {
    pub name: String,
    /// 说它的那条记录的时刻（epoch ms）；记录没写 / 解不出 ⇒ `None`。
    pub at_ms: Option<i64>,
    /// 那一家写的原话（复制详情用）。
    pub detail: Option<String>,
    /// 说它的那条会话的标题（同历史清单那一格的口径）。
    pub title: String,
}

/// 各条活会话记录里 MCP 那一格读到了哪：记录 ⇒ （读过的字节数 · 那一格）。扩展页每打开一次只读各条记录新长出来的那一截。
#[derive(Debug, Default)]
pub(crate) struct McpBook(
    std::collections::BTreeMap<PathBuf, (u64, Vec<super::recognize::facts::McpTrouble>)>,
);

impl McpBook {
    /// 这条记录此刻的 MCP 那一格：从上次读到的地方接着读（记录比上次读到的短 ⇒ 换了一份，从头读）。
    pub(crate) fn read(
        &mut self,
        path: &Path,
    ) -> std::io::Result<Vec<super::recognize::facts::McpTrouble>> {
        use std::io::{Seek, SeekFrom};
        let mut file = Ro::open(path)?;
        let len = file.metadata()?.len();
        let (at, mcp) = self.0.entry(path.to_path_buf()).or_default();
        if len < *at {
            *at = 0;
            mcp.clear();
        }
        file.seek(SeekFrom::Start(*at))?;
        let kind = crate::agents::record_kind_of(path).unwrap_or_default();
        *at += super::recognize::facts::mcp_from(std::io::BufReader::new(file), kind, mcp)?;
        Ok(mcp.clone())
    }

    /// 只留这几条（这一轮不活了的那几条忘掉）。
    pub(crate) fn keep_only(&mut self, live: &[PathBuf]) {
        self.0.retain(|p, _| live.contains(p));
    }
}

/// 本进程那一本（常驻后端里跨请求留着；一次性 CLI 进程里每次都是空的，等于从头读）。
static MCP_BOOK: std::sync::Mutex<Option<McpBook>> = std::sync::Mutex::new(None);

/// 这台此刻活着的交互会话（`homes` 各号的家那一张活会话表，后台任务不算）里，
/// 那一家说断开的 MCP：每个名字取说它最晚的那一条，按名字排。会话结束了的不算（那份原话已经过时）。
/// 每条会话只读 MCP 那一格、只读新长出来的那一截（[`McpBook`]）；标题取历史清单的缓存，带上这台注解里改过的名（`history::listing::session_title_of`）。
pub fn live_mcp_failed(homes: &[PathBuf]) -> Vec<LiveMcpFailed> {
    let mut book = MCP_BOOK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let book = book.get_or_insert_with(McpBook::default);
    let mut seen: Vec<PathBuf> = Vec::new();
    let mut out: Vec<LiveMcpFailed> = Vec::new();
    // 这台的注解（叫什么带上改过的名）：真有断开的 MCP 要配标题时才读一次。
    let annotations = std::cell::OnceCell::new();
    for home in homes {
        let table = crate::history::live_table::now(home);
        for row in table.interactive() {
            let Some(sid) = row.sid() else {
                continue;
            };
            let Ok(path) = super::query::locate::session_record(home, sid) else {
                continue;
            };
            seen.push(path.clone());
            let Ok(said) = book.read(&path) else {
                continue;
            };
            let mut title: Option<String> = None;
            for m in said {
                if m.status != crate::agents::McpStatus::Failed {
                    continue;
                }
                let at_ms =
                    m.at.as_deref()
                        .and_then(crate::common::time::parse_iso8601_ms);
                if out.iter().any(|o| o.name == m.name && o.at_ms >= at_ms) {
                    continue;
                }
                out.retain(|o| o.name != m.name);
                let title = title
                    .get_or_insert_with(|| {
                        let annotations = annotations.get_or_init(|| {
                            match crate::history::history_annotations::load() {
                                crate::history::history_annotations::Loaded::Read(t) => t,
                                _ => Default::default(),
                            }
                        });
                        crate::history::listing::session_title_of(
                            &path,
                            annotations.get(sid).and_then(|e| e.custom_title.clone()),
                        )
                    })
                    .clone();
                out.push(LiveMcpFailed {
                    name: m.name,
                    at_ms,
                    detail: m.detail,
                    title,
                });
            }
        }
    }
    book.keep_only(&seen);
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// **这条会话此刻谁持着它**（冷读那几处判活只问这一处）：`record` 是这条会话的记录（已过围栏），
/// 哪一家照它落在哪棵树下认（`agents::record_kind_of`），怎么判照那一家声明的（[`crate::agents::Adapter::live`]，见 [`session_holders_by`]）。
pub fn session_holders(agent_home: &Path, record: &Path) -> Option<Vec<u32>> {
    let kind = crate::agents::record_kind_of(record)?;
    session_holders_by(crate::agents::live_by(kind), agent_home, record)
}

/// [`session_holders`] 按判活方式那一半（判据直接喂方式）：
/// - pidfile ⇒ 活会话表里持着这个 sid（记录文件名）的活交互进程（`LiveTable::writers`）；
/// - 开着写 ⇒ 此刻开着这份记录写的进程（`platform::writers`；这个平台认不出写者 ⇒ 判不了）；
/// - 判不了 ⇒ `None`。
///
/// `Some(空)` ＝ 判过了、此刻没有谁持着它（走了）；`None` ＝ 判不了（不是「没有」）。pid 升序。
pub(crate) fn session_holders_by(
    live: crate::agents::LiveBy,
    agent_home: &Path,
    record: &Path,
) -> Option<Vec<u32>> {
    use crate::agents::LiveBy;
    match live {
        LiveBy::Pidfiles => {
            Some(crate::history::live_table::now(agent_home).writers(record.file_stem()?.to_str()?))
        }
        LiveBy::HeldOpen => {
            let want = crate::platform::writers::absolute(record);
            let found = crate::platform::writers::writers_of(&|p| p == want)?;
            Some(
                found
                    .get(&want)
                    .map(|w| w.iter().map(|w| w.pid).collect())
                    .unwrap_or_default(),
            )
        }
        LiveBy::Unknown => None,
    }
}

/// 一个活会话此刻在干什么 —— 与主窗口标签页同一判：活动态（活会话表）· 在等你时等的是什么。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Doing {
    /// 说不清 ⇒ `None`（主窗口那颗点照「在跑」画）。
    pub activity: Option<crate::agents::SessionActivity>,
    /// 只在 `activity` 是在等你时有：配上记录里没结果的那一步判的种类（[`super::recognize::facts::needs_of`]）。
    pub needs: Option<super::recognize::facts::NeedsKind>,
}

/// 这台此刻活着的每个会话在干什么（活会话表 `LiveTable::doing`）；在等你 ⇒ 等的是什么同 [`waiting_needs`]（只留种类）。
pub fn live_doing(agent_home: &Path) -> std::collections::BTreeMap<String, Doing> {
    let table = crate::history::live_table::now(agent_home);
    table
        .doing()
        .into_iter()
        .map(|(sid, activity)| {
            let needs = (activity == Some(crate::agents::SessionActivity::NeedsYou))
                .then(|| waiting_needs(agent_home, &table, &sid).map(|n| n.kind))
                .flatten();
            (sid, Doing { activity, needs })
        })
        .collect()
}

/// **这台上需手动的会话**：活会话表说在等人的每一个（那台说在等 ∪ 有请求挂着，`LiveTable::needs`），带它在等什么（[`waiting_needs`]，不另判），
/// 先答的在前（`needs.rank` 小的先，[`super::recognize::facts::needs_rank`]）。
/// `dress` ＝ 给一条 `needs` 配上它挂着的请求（顶层壳交进来）。`hidden(sid, 是不是分身)` ＝ 那一格（顶层壳经 `history/session_hidden.rs` 判）。
pub fn live_needs(
    agent_home: &Path,
    dress: &dyn Fn(
        Option<super::recognize::facts::Needs>,
        &str,
    ) -> Option<super::recognize::facts::Needs>,
    hidden: &dyn Fn(&str, bool) -> Option<cmd_wire::session::Hidden>,
) -> Vec<NeedsRow> {
    let table = crate::history::live_table::now(agent_home);
    let bg = table.background();
    let annotations = match crate::history::history_annotations::load() {
        crate::history::history_annotations::Loaded::Read(t) => t,
        _ => Default::default(),
    };
    let mut rows = table
        .needs()
        .into_iter()
        .filter_map(|sid| {
            Some(NeedsRow {
                label: crate::common::cells::Words(crate::history::session_hidden::title_here(
                    &sid,
                    &annotations,
                )),
                face: crate::wire::SessionFace::of(crate::wire::Life::Live {
                    activity: Some(crate::agents::SessionActivity::NeedsYou),
                    host: None,
                }),
                // 那台说在等 ⇒ 配上它在等什么；只挂着请求（挡着终端的那种 pidfile 不说在等）⇒ 交 `dress` 配请求。
                needs: dress(
                    table
                        .says_waiting(&sid)
                        .then(|| waiting_needs(agent_home, &table, &sid))
                        .flatten(),
                    &sid,
                )?,
                hidden: hidden(&sid, bg.contains(&sid)),
                sid,
            })
        })
        .collect::<Vec<_>>();
    // 先答哪个照核心那一处（同一档、同样久 ⇒ 按会话 id，稳定）。
    rows.sort_by(|a, b| {
        a.needs
            .rank
            .cmp(&b.needs.rank)
            .then_with(|| a.sid.cmp(&b.sid))
    });
    rows
}

/// 一条在等人的会话此刻要人做什么：读它的记录（扫描图缓存）配上没结果的调用，经 [`super::recognize::facts::needs_of`] 判（同 `history-facts.needs`）；
/// 记录找不到 ⇒ 不挂哪一步，框是哪种照那台说的（那台说在等 ⇒ 恒有一份）。
fn waiting_needs(
    agent_home: &Path,
    table: &LiveTable,
    sid: &str,
) -> Option<super::recognize::facts::Needs> {
    let record = super::query::locate::session_record(agent_home, sid).ok();
    let pending = record
        .as_ref()
        .and_then(|p| {
            super::query::locate::facts(agent_home, &p.to_string_lossy(), &Vec::new(), None).ok()
        })
        .map(|f| f.pending)
        .unwrap_or_default();
    // 找不到记录 ⇒ 按写那份 pidfile 的那一家认工具（哪一家跟着那一行走，`ProcFact::kind`）。
    let kind = record
        .as_deref()
        .and_then(crate::agents::record_kind_of)
        .or_else(|| table.kind_of(sid))
        .unwrap_or_default();
    let wait = table
        .wait_of(sid)
        .unwrap_or(super::recognize::facts::PidWait {
            waiting_for: None,
            since_ms: None,
            read_at_ms: 0,
        });
    super::recognize::facts::needs_of(kind, &pending, Some(&wait))
}

// ---------------------------------------------------------------- tests

#[cfg(test)]
#[path = "../../../tests/backend/observe/accounts_query_tests.rs"]
mod tests;
