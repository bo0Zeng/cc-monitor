//! 合成历史面：`HistoryFace` 会话不住记录树的那一家，历史清单由通用层并进来；
//! 与各家各号的记录根那一张表（[`record_roots_among`]）：认家 · 围栏 · 按 sid 找 · 删与归档都照它。

use super::*;

/// 一家的合成历史面：函数指针（同 [`Adapter::home`]，不立 trait）。
#[derive(Clone, Copy)]
pub struct HistoryFace {
    /// 这台机器上这一家每个号底下的会话（没装 ⇒ 空）：`homes` 是这一家各号的家（[`AccountHome`]，账号库给；账号 0 是那一家的默认家），
    /// 各根各列。
    pub sessions: fn(&[AccountHome]) -> Vec<SynthSession>,
    /// 一份会话的首条真用户话（列表摘要）。
    pub excerpt: fn(&Path) -> String,
    /// 这几个家底下这一家会话记录的根（每个家可以有几个：日期树 · 归档根都算；没有 ⇒ 空）。
    /// 通用层只经根表（[`record_roots_among`]）问它，一个家问一次，根跟着那个家走。
    pub roots: fn(&[AccountHome]) -> Vec<PathBuf>,
}

/// 合成历史里的一个会话（通用层按 `cwd` 分组成项目）。
#[derive(Debug, Clone, PartialEq)]
pub struct SynthSession {
    pub sid: String,
    pub path: PathBuf,
    /// 分组键；缺 ⇒ 空串（归「(<kind>)」组）。
    pub cwd: String,
    pub mtime_ms: i64,
}

/// 一家的一个号的家：号名（账号 0 ⇒ `None`）· 号目录（账号 0 ⇒ 那一家的默认家）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountHome {
    pub account: Option<String>,
    pub home: PathBuf,
}

/// 账号库那一侧交来的「`kind` 那一家各具名号的家」（[`on_account_homes`] 接上；本层不读账号库清单）。
static ACCOUNT_HOMES: std::sync::OnceLock<fn(&str) -> Vec<AccountHome>> =
    std::sync::OnceLock::new();

/// 进程入口接上账号库那一手（`accounts::manage::library::account_homes_here`）；接过一次 ⇒ `Err`（第一次那一手留着）。
pub fn on_account_homes(
    f: fn(&str) -> Vec<AccountHome>,
) -> Result<(), fn(&str) -> Vec<AccountHome>> {
    ACCOUNT_HOMES.set(f)
}

/// 这一行那一家各号的家：账号 0（那一行的 [`Adapter::home`]，说得出才有）＋ 账号库里那一家的各具名号（没接上 ⇒ 只有账号 0）。
pub fn homes_among(a: &Adapter) -> Vec<AccountHome> {
    (a.home)()
        .map(|home| AccountHome {
            account: None,
            home,
        })
        .into_iter()
        .chain(ACCOUNT_HOMES.get().map(|f| f(a.kind)).unwrap_or_default())
        .collect()
}

/// [`homes_among`] 在生产注册表上：`kind` 那一家各号的家（认不出 ⇒ 空）。合成历史并清单 · 起会话框列最近目录都照它。
pub fn homes_of(kind: &str) -> Vec<AccountHome> {
    adapter_among(REGISTRY, kind).map_or_else(Vec::new, homes_among)
}

/// 根表的一项：哪一家 · 那个号的家（号目录）· 那个家底下的一个记录根。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordRoot {
    pub kind: &'static str,
    pub home: PathBuf,
    pub root: PathBuf,
}

/// **根表**：`registry` 里每一家、`homes` 给的每个家底下的记录根（注册序 · 家序 · 那一家报的根序）。
/// 记录树那一家 ⇒ 每个家一棵树（[`RecordTree::root`]）；合成历史那一家 ⇒ 它照那个家报的那几个根（[`HistoryFace::roots`]）。
/// 认家 · 读会话围栏 · 清单 · 按 sid 找 · 删与归档找那一份都照这一张表，不各算一遍。
pub fn record_roots_among(
    registry: &[Adapter],
    homes: &dyn Fn(&Adapter) -> Vec<AccountHome>,
) -> Vec<RecordRoot> {
    let mut out = Vec::new();
    for a in registry {
        let tree = a.records.and_then(|r| r.tree);
        if tree.is_none() && a.history.is_none() {
            continue;
        }
        for h in homes(a) {
            let roots = match (a.history, tree) {
                (Some(f), _) => (f.roots)(std::slice::from_ref(&h)),
                (None, Some(t)) => vec![(t.root)(&h.home)],
                (None, None) => Vec::new(),
            };
            out.extend(roots.into_iter().map(|root| RecordRoot {
                kind: a.kind,
                home: h.home.clone(),
                root,
            }));
        }
    }
    out
}

/// [`record_roots_among`] 照 [`homes_among`] 取家。
pub fn record_roots_of(registry: &[Adapter]) -> Vec<RecordRoot> {
    record_roots_among(registry, &homes_among)
}

/// 注册表里有合成历史面的每一家：`(kind, 面)`（注册序）。
pub fn history_faces() -> Vec<(&'static str, HistoryFace)> {
    REGISTRY
        .iter()
        .filter_map(|a| a.history.map(|h| (a.kind, h)))
        .collect()
}

/// 内容搜索不覆盖（没有文本面）、而这台上又有它的会话记录的那几家：它们对用户的叫法（注册序）。
pub fn content_search_skips() -> Vec<String> {
    let roots = history_roots_among(REGISTRY);
    REGISTRY
        .iter()
        .filter(|a| {
            a.records.is_none_or(|r| r.text.is_none())
                && roots.iter().any(|(k, r)| *k == a.kind && r.is_dir())
        })
        .map(|a| family_said_among(REGISTRY, a.kind))
        .collect()
}

/// `registry` 里各家合成历史面在根表上的根（`(那一家, 根)`，表序；只问合成历史那几家的家）。
/// 认家 · 按路径读会话的围栏 · 全文搜索在家目录记录树之外还认这几个。
pub fn history_roots_among(registry: &[Adapter]) -> Vec<(&'static str, PathBuf)> {
    let synth_homes = |a: &Adapter| match a.history {
        Some(_) => homes_among(a),
        None => Vec::new(),
    };
    record_roots_among(registry, &synth_homes)
        .into_iter()
        .map(|r| (r.kind, r.root))
        .collect()
}
