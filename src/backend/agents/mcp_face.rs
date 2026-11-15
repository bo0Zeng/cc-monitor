//! MCP 面：`McpFace` 一家的 MCP 读。

use super::*;

/// 一家的 MCP 读面：函数指针（同 [`Adapter::home`]，不立 trait）。入参是项目目录（可缺）＋ 判状态要看的那几个家。
#[derive(Clone, Copy)]
pub struct McpFace {
    pub(crate) read: fn(Option<&Path>, &McpLook) -> McpRead,
    /// 登录一个 MCP 服务器要敲的那一行：服务器名 ⇒ argv（程序名或斜杠命令在头）。读者照它拼、不认哪一家（待办那一件复制它）；
    /// `None` ＝ 这一家不给这一项（待办里不出那一件）。
    pub(crate) login: Option<fn(&str) -> Vec<String>>,
    /// 在 app 里登录一个 MCP 服务器那一形（子命令最后再接服务器名）；`None` ＝ 只能在会话里登。
    pub sign_in: Option<SignInFace>,
    /// 各号家目录里记「要登录」的那份文件（盯它：翻过来 ⇒ 会话外的需手动清单可能变了）。
    pub(crate) needs_login_file: Option<&'static str>,
}

/// 判状态要看的：这台各号的家目录（账号库里的名字 · 那个号的家；没设账号的那一份名字是 `None`，与某个号同一个目录时不另列）
/// ＋ 没设账号时那一家的家（`bare`：没设账号那一份配置照它认缓存，与它在不在 `homes` 里、在里面叫什么无关）＋ 此刻（epoch ms）。
/// 账号库是通用层的知识，由通用层收齐交给那一家；那一家只认自己家目录里的文件。
#[derive(Debug, Default, Clone, PartialEq)]
pub struct McpLook {
    pub homes: Vec<(Option<String>, PathBuf)>,
    pub bare: Option<PathBuf>,
    pub now_ms: u64,
}

/// 一条 MCP server 此刻的状态（中立值；各家的字由适配层翻成它；线上名即 serde 名，闭集）。
/// 配置层（`mcp-read`）只说说得准的：停用（配置里写着）· 要登录（那一家最近一次连它时记下的、还在有效期内）· 其余 `Unknown`；
/// 会话事实（`history-facts` 的 `mcp`）只列那一家在记录里说有毛病的：要登录 · 断开 · 还在连。两处都不说「连上了」。
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub enum McpStatus {
    NeedsLogin,
    Failed,
    Pending,
    Disabled,
    #[default]
    Unknown,
}

/// 一条记录里那一家说的各 MCP 服务器此刻的样子：三张表各一格；这一条没写的那一格 ⇒ `None`（沿用上一条说的）。
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct McpSaid {
    pub pending: Option<Vec<String>>,
    pub needs_login: Option<Vec<String>>,
    /// 断开的：名字 · 那一家写的原话（没写 ⇒ `None`）。
    pub failed: Option<Vec<(String, Option<String>)>>,
}

/// `kind` 那一家在这条记录里说的 MCP 样子。那一家不说 / 不是这种记录 ⇒ `None`。
pub fn mcp_said_of(kind: &str, v: &serde_json::Value) -> Option<McpSaid> {
    record_face(kind)
        .and_then(|r| r.mcp_said)
        .and_then(|f| f(v))
}

/// 一家读出来的 MCP 事实：条目（user / local / project）· 用过的项目目录（`dirs`）· 读不出来的那几份（说出来，不当成空）。
#[derive(Debug, Default, Clone, PartialEq)]
pub struct McpRead {
    pub entries: Vec<McpEntry>,
    pub dirs: Vec<String>,
    pub problems: Vec<String>,
}

/// 一条 MCP server：`server` 原样（宽容，未知字段不丢）。
#[derive(Debug, Clone, PartialEq)]
pub struct McpEntry {
    pub scope: &'static str,
    pub name: String,
    pub server: serde_json::Value,
    pub source: String,
    pub status: McpStatus,
    /// 要登录：是哪几位（账号库里的名字；`None` ＝ 没设账号的那一份），排好序（`None` 在前）；要登录时恒非空，别的状态恒空。
    pub login_in: Vec<Option<String>>,
    /// 要登录：最近一次看到是何时（epoch ms）；别的状态 `None`。
    pub seen_ms: Option<u64>,
}

/// 一条成品要的 MCP 条目：[`McpEntry`] ＋ 它在哪几个号里 ＋ 同名的几条配置是否不一致（[`mcp_held`] 写好）。
#[derive(Debug, Clone, PartialEq)]
pub struct McpHeld {
    pub entry: McpEntry,
    /// 出自哪几个号的家（账号库里的名字，排好序）；没设账号的那一份 · 不在哪个号家里的不出名字。
    pub accounts: Vec<String>,
    /// 同一 scope 同名、配置原文却不一样的还有别的几条（各自照实列）；项目级恒 `false`。
    pub mismatch: bool,
}

/// 项目级那一段：几条同名是项目里一层盖一层（Codex 每层一份），不是各号各一份 ⇒ 不合、不标不一致。
const PROJECT_SCOPE: &str = "project";

/// 一家读出来的那几条 ⇒ 成品要的那几条（`mcp-read` · 需手动清单 · app 里登录前核名字都照它出，界面不去重）：
/// - 出自哪个号：条目的 `source` 落在 `homes` 里哪个家底下（几个家套着 ⇒ 最深的那一个）；那个家没设账号 · 哪个家都不在 ⇒ 不出名字。
/// - 用户级 · 本地级：同一 scope、同名、配置原文相同的几条合成一条（留头一条的来源；号名并起来；要登录的那几位并起来
///   （没设账号那一位也在里面，不随合并丢掉）；最近一次取最晚；头一条说不准的状态取后面说得准的）；配置原文不同 ⇒ 照实分开列，各带各的号，都标 `mismatch`。
/// - 项目级照原样一条一条列。次序照头一次出现的先后。
/// - 合得成几号的前提是读法逐号读出用户级 · 本地级那一份：Claude 的读法逐号读；Codex 的读法此刻只读这台环境那一份。
pub fn mcp_held(entries: &[McpEntry], homes: &[(Option<String>, PathBuf)]) -> Vec<McpHeld> {
    let account_of = |source: &str| -> Option<String> {
        let at = Path::new(source);
        homes
            .iter()
            .filter(|(_, h)| at.starts_with(h))
            .max_by_key(|(_, h)| h.components().count())
            .and_then(|(who, _)| who.clone())
    };
    let mut out: Vec<McpHeld> = Vec::new();
    for e in entries {
        let who = account_of(&e.source);
        // 同一 scope · 同名 · 同配置原文的那一条（按条目的键比，不是在注册表上取哪一家）。
        let same = (e.scope != PROJECT_SCOPE)
            .then(|| {
                out.iter().position(|h| {
                    h.entry.scope == e.scope && h.entry.name == e.name && h.entry.server == e.server
                })
            })
            .flatten();
        if let Some(kept) = same.map(|i| &mut out[i]) {
            kept.accounts.extend(who);
            kept.entry.login_in.extend(e.login_in.iter().cloned());
            kept.entry.seen_ms = kept.entry.seen_ms.max(e.seen_ms);
            if kept.entry.status == McpStatus::Unknown {
                kept.entry.status = e.status;
            }
            continue;
        }
        out.push(McpHeld {
            entry: e.clone(),
            accounts: who.into_iter().collect(),
            mismatch: false,
        });
    }
    for i in 0..out.len() {
        out[i].accounts.sort();
        out[i].accounts.dedup();
        out[i].entry.login_in.sort();
        out[i].entry.login_in.dedup();
        let (scope, name) = (out[i].entry.scope, out[i].entry.name.clone());
        out[i].mismatch = scope != PROJECT_SCOPE
            && out
                .iter()
                .enumerate()
                .any(|(j, h)| j != i && h.entry.scope == scope && h.entry.name == name);
    }
    out
}

/// 声明了 MCP 面的每一家（注册序）：逐家的事（需手动清单 · 待办 · 整台的 `mcp-read`）照它各问一遍。
pub fn mcp_kinds_among(registry: &[Adapter]) -> Vec<&'static str> {
    registry
        .iter()
        .filter(|a| a.mcp.is_some())
        .map(|a| a.kind)
        .collect()
}

/// `agent` 那一家，得声明了 MCP 面（照 MCP 面问 [`kind_with_face_among`]；只收家名，没有不说家的那一问）。
pub fn mcp_kind_among(registry: &[Adapter], agent: &str) -> Option<&'static str> {
    kind_with_face_among(registry, Some(agent), |a| a.mcp.is_some())
}

/// `kind` 那一家的 MCP 读面，读一遍。`None` = 那一家不认得 MCP。
pub fn mcp_read_among(
    registry: &[Adapter],
    kind: &str,
    project_dir: Option<&Path>,
    look: &McpLook,
) -> Option<McpRead> {
    mcp_face_among(registry, kind).map(|f| (f.read)(project_dir, look))
}

/// `kind` 那一家登录 MCP 服务器 `server` 要敲的那一行（[`McpFace::login`]）。那一家不认得 MCP · 没声明这一格 ⇒ `None`。
pub fn mcp_login_argv_among(registry: &[Adapter], kind: &str, server: &str) -> Option<Vec<String>> {
    mcp_face_among(registry, kind)
        .and_then(|f| f.login)
        .map(|f| f(server))
}

/// `kind` 那一家的 MCP 面（[`McpFace`]）。那一家不认得 MCP ⇒ `None`。
pub fn mcp_face(kind: &str) -> Option<McpFace> {
    mcp_face_among(REGISTRY, kind)
}

/// [`mcp_face`] 照给定注册表（需手动清单 · `mcp-read` 的解法照它手里那张表问）。
pub fn mcp_face_among(registry: &[Adapter], kind: &str) -> Option<McpFace> {
    adapter_among(registry, kind).and_then(|a| a.mcp)
}

/// 这台注册表里各家记「MCP 要登录」的那份文件名（watcher 认它）。
pub fn mcp_needs_login_files() -> Vec<&'static str> {
    REGISTRY
        .iter()
        .filter_map(|a| a.mcp.and_then(|m| m.needs_login_file))
        .collect()
}

/// `kind` 那一家没设账号时的家目录（注册表 `home` 那一格）。认不出 / 说不出 ⇒ `None`。
pub fn home_of_kind(kind: &str) -> Option<PathBuf> {
    adapter_among(REGISTRY, kind).and_then(|a| (a.home)())
}

/// [`mcp_read_among`] 对本机注册表（号间同步 MCP · app 里登录前核名字那两处读它）。
pub fn mcp_read(kind: &str, project_dir: Option<&Path>, look: &McpLook) -> Option<McpRead> {
    mcp_read_among(REGISTRY, kind, project_dir, look)
}
