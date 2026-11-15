//! 账号库那几条帧命令（`accounts-init` · `accounts-add` · `accounts-remove` · `accounts-set-default` · `accounts-repair` ·
//! `accounts-rollback` · `accounts-verify` · `accounts-login-cmd` · `accounts-mcp-*` · `accounts-list`）的**线上形状** —— 两侧同一份：
//! 后端按这里的入参严格收（多一个键 ⇒ 拒）、按这里的成品回；界面用从这里导出的类型（`src/frontend/ui/generated/`）。

use serde::{Deserialize, Serialize};

/// 一个号靠什么鉴权。字面量与清单里 `authKind` 那一格同一套（`acct_core::AUTH_KIND_SUBSCRIPTION` · `acct_core::AUTH_KIND_API_KEY`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(rename_all = "kebab-case")]
pub enum AccountKind {
    Subscription,
    ApiKey,
}

/// `accounts-init`：建账号库，把这台机器现在登录的那个身份收成名叫 `name` 的默认号。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
pub struct AccountInitArgs {
    pub name: String,
    /// 真 ⇒ 只算不做（回将要做的那几步）。
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub dry_run: Option<bool>,
}

/// `accounts-add`：新建一个号。订阅号可以从一份已有的凭据文件导入；API 号带端点与 key（key 只进那台的 apikey 表，不回显）。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
pub struct AccountAddArgs {
    pub name: String,
    pub kind: AccountKind,
    /// 订阅号：导入哪一份凭据文件（家目录底下；`~/` 开头也认）。缺席 ⇒ 建好之后另行登录（`login-start`）。
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub cred_file: Option<String>,
    /// API 号：上游地址（缺席 / 空 = 这个 agent 的默认上游）。
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub base_url: Option<String>,
    /// API 号：key 明文。只在入方向出现一次。
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub key: Option<String>,
    /// 真 ⇒ 建好后它是默认号（`ccm` 不带 `--account` 时用它）。
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub is_default: Option<bool>,
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub dry_run: Option<bool>,
    /// 这个号是哪一家的号（线上的 kind）；缺席 ⇒ 缺省那一家，注册表里没有 ⇒ `refused`。
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub agent: Option<crate::AgentKind>,
}

/// `accounts-remove`：删一个号（只删它自己的目录，共享库不动；改动前留备份）。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
pub struct AccountRemoveArgs {
    pub name: String,
    /// 删的是默认号时必须给真。
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub force: Option<bool>,
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub dry_run: Option<bool>,
}

/// `accounts-set-default` · `accounts-login-cmd`：只带一个账号名。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
pub struct AccountNameArgs {
    pub name: String,
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub dry_run: Option<bool>,
}

/// `accounts-repair`：补齐共享链接 · 修权限 · 刷新邮箱 · 补账号别名（幂等）。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
pub struct AccountRepairArgs {
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub dry_run: Option<bool>,
}

/// `accounts-rollback`：按一份备份把改动还原。`backup` 缺席 ⇒ 最近一份还没还原过的。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
pub struct AccountRollbackArgs {
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub backup: Option<String>,
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub dry_run: Option<bool>,
}

/// 刚建好的那个号。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
pub struct AccountRef {
    pub name: String,
    pub config_dir: String,
}

/// 建号 / 删号那一刻别名这一侧怎么样了：用户的配置文件不动，号那三条是内置的、照账号表现算，这一下只重生成别名文件与链接。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
pub struct AliasChange {
    /// 用户的配置文件（`~/.cc-monitor/profiles.toml`，这一趟不写它）。
    pub path: String,
    /// 别名文件或链接这一趟动了没有（内容没变 ⇒ 一个字节不写）。
    pub changed: bool,
    /// 建号：多出来的内置别名（如 `workcc` `workcct` `workcch`）。
    pub added: Vec<String>,
    /// 删号：跟着号一起没了的内置别名。
    pub removed: Vec<String>,
    /// 建号：用户文件里已有同名的一段、盖着内置的那几个（照用户那段起）。
    pub skipped: Vec<String>,
    /// 没能重生成时的那句话（比如配置文件整份写坏了）；`None` = 一切照常。
    pub note: Option<String>,
}

/// 改账号库那几条命令的成品（`dryRun` 为真时 `applied` 恒假、`steps` 是将要做的那几步）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Default)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
pub struct AccountChange {
    pub applied: bool,
    /// 做了（或将要做）的每一步，一句一行。
    pub steps: Vec<String>,
    /// 提示（不挡这一趟）。
    pub notes: Vec<String>,
    /// 这一趟留的备份（回滚用的名字）；没改动 ⇒ `None`。
    pub backup: Option<String>,
    /// `accounts-add`：建出来的那个号。
    pub account: Option<AccountRef>,
    /// `accounts-init` / `accounts-add`：这个号会自动拿到的别名名字（预演时也给）。
    pub alias_names: Vec<String>,
    /// `accounts-add` 的 API 号：key 写进 apikey 表之后的掩码。
    pub key_masked: Option<String>,
    /// 号建好了、key 却没写进去时那一句（界面据此让人在那一行上重填）。
    pub key_problem: Option<String>,
    /// 建号 / 删号那一刻改了的别名文件，一份一条；别的命令 ⇒ 空。
    pub aliases: Vec<AliasChange>,
}

/// 核对里一条的档。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
pub enum CheckLevel {
    Ok,
    Warn,
    Fail,
    Skip,
}

/// 核对里的一条。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
pub struct VerifyCheck {
    pub level: CheckLevel,
    /// 说的是哪个号；全局的那几条 ⇒ `None`。
    pub account: Option<String>,
    pub text: String,
}

/// `accounts-verify` 的成品：先对齐账号库（缺的共享链接补上），再把链接 · 权限 · 邮箱逐项核一遍。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
pub struct VerifyReport {
    /// 没有一条 `fail`。
    pub pass: bool,
    pub fails: u32,
    pub checks: Vec<VerifyCheck>,
    /// 有对不上的 ⇒ 写好的那一句（第一条对不上的那句，再接「另 ×N」）：账号页核对条照摆，不数、不挑、不拼。都对得上 ⇒ 不上线。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub headline: Option<String>,
}

/// `accounts-login-cmd` 的成品：在终端里起 claude 登录这个号的那一行（已 quote）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
pub struct AccountLoginCmd {
    pub cmd: String,
}

/// `accounts-mcp-remove`：从这台各号共用的用户级 MCP 里删 `agent` 那一家的一条（那一家所有号一起撤）。
/// `agent` 缺席 ⇒ 缺省那一家（同 `mcp-read`）；不是同步 MCP 的那几家之一 ⇒ `bad_args`。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
pub struct AccountMcpNameArgs {
    pub name: String,
    #[serde(default)]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub agent: Option<crate::AgentKind>,
}

/// `accounts-mcp-pick`：`agent` 那一家两边都改了的那一条，用哪一版。`from` = 那个号里的那一版；缺席 / `null` = 共享的那一版。
/// `agent` 同 [`AccountMcpNameArgs::agent`]。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
pub struct AccountMcpPickArgs {
    pub name: String,
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub from: Option<String>,
    #[serde(default)]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub agent: Option<crate::AgentKind>,
}

/// `accounts-mcp-sync`：停 / 开各号之间同步用户级 MCP（停了已同步的不删）。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
pub struct AccountMcpSyncArgs {
    pub on: bool,
}

/// 冲突里的一版（只带号名，不带定义 —— 定义里可能有密钥）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
pub struct AccountMcpChoice {
    /// 挑这一版时交回去的 `from`（`None` = 共享的那一版）。
    pub from: Option<String>,
    /// 此刻是这一版的那几个号。
    pub holders: Vec<String>,
    /// 这一版是「没有这一条」（在 cc-monitor 里删过）。
    pub gone: bool,
}

/// 两边都改了、等用户挑的一条。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
pub struct AccountMcpConflict {
    pub name: String,
    /// 哪一家的（线上的 kind）；缺席 ⇒ 缺省那一家（同 `mcp-read`）。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub agent: Option<crate::AgentKind>,
    /// 这一栏不止一家时那一家对人的叫法（后端照注册表说好；只一家 ⇒ 缺席，界面不画家）。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub agent_said: Option<String>,
    pub choices: Vec<AccountMcpChoice>,
}

/// 共享集合里的一条服务器：名字 ＋ 哪一家的（缺席 ⇒ 缺省那一家，同 `mcp-read`；同名的两家各是一条，不相撞）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
pub struct AccountMcpServer {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub agent: Option<crate::AgentKind>,
    /// 这一栏不止一家时那一家对人的叫法（后端照注册表说好；只一家 ⇒ 缺席，界面不画家）。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub agent_said: Option<String>,
}

/// `accounts-mcp-read` · `accounts-mcp-remove` · `accounts-mcp-pick` · `accounts-mcp-sync` 的成品：这台各号共用的用户级 MCP 此刻的样子。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Default)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
pub struct AccountMcpView {
    /// 这台有没有账号库（没有 ⇒ 不做同步，下面几格都是空的）。
    pub enabled: bool,
    /// 各号之间在不在同步（用户停了 ⇒ `false`：各号各管各的，`conflicts` 恒空）。
    pub sync: bool,
    /// 共享集合里的服务器：照家序，一家之内按名字排好序。
    pub servers: Vec<AccountMcpServer>,
    pub conflicts: Vec<AccountMcpConflict>,
    /// 这一趟改写了哪几个号（只读那一条恒空）。
    pub changed: Vec<String>,
    /// 提示（某个号读不出来 · 这一趟没写进去）。
    pub notes: Vec<String>,
}

// ── accounts-list 的回包 ──────────────────────────────────────────────────────────────────

/// `accounts-list` 的回包：清单 ＋ 这台的 apikey 表并上去的那几格。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct AccountsList {
    pub meta: ListMeta,
    /// 每账号一行，清单次序。
    pub accounts: Vec<AccountRow>,
    /// 「能用但有缺」的那一句（几件都有 ⇒ 连成一句）：启用了却一个账号 0 都没有（写清单的那一侧旧到不认账号 0）·
    /// 这台的 API key 凭据文件不是只给本人（宽在哪 ＋ 怎么修；查不出 ⇒ 为什么）；都没有 ⇒ `null`。
    pub notice: Option<String>,
}

/// `AccountsList::meta`。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(rename_all = "camelCase")]
pub struct ListMeta {
    /// 这台建了账号库（清单读得出）。
    pub enabled: bool,
    pub manifest_path: String,
    pub updated_at: Option<String>,
    /// 清单里认得的号数。
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub count: usize,
    /// 清单读不出的那一句。
    pub error: Option<String>,
    /// 这台做不了多账号的那一句（做得了 ⇒ `null`）。
    pub unsupported: Option<String>,
    /// 删掉默认号之后新会话默认谁（同删号那一条的答案）。
    pub next_default: Option<String>,
    /// 这台的默认号（标了的第一个，没标 ⇒ 第一个；`acct_core::effective_default`）。
    pub effective_default: Option<String>,
}

/// 扫清单得出的一个号（`AccountRow` 里除了并表那几格的部分）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(rename_all = "camelCase")]
pub struct ScannedAccount {
    pub name: String,
    /// 号在人眼里叫什么（账号 0 ⇒ 「~/.claude」那一句，其余照原名）。
    pub label: String,
    pub email: String,
    /// `null` ＝ 账号 0（下游据此不注入配置目录那一格）。
    pub config_dir: Option<String>,
    pub is_default: bool,
    /// `isolated` · `in-place`。
    pub mode: String,
    pub exists: bool,
    /// 凭据文件在不在（不代表凭据有效；可用性看 `authReady`）。
    pub logged_in: bool,
    /// `subscription` · `api-key`。
    pub auth_kind: String,
    pub auth_ready: bool,
    /// 能拿来起会话 / 选为默认（`acct_core::account_selectable`）。
    pub selectable: bool,
    /// 号色（`--acct-c0..c7` 第几格，账号库里的事实），界面只画它；库里没给 ⇒ 缺（画中性）。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub color: Option<u8>,
    /// 这个号是哪一家的号（线上的 kind；清单里没写的照缺省那一家给出）；认不出 ⇒ 缺。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub agent: Option<crate::AgentKind>,
    /// 号表里那一格写了、注册表里认不出的那一家（原样）：`agent` 这时缺。认得出 · 没写 ⇒ 缺（没写的照缺省那一家进 `agent`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub unknown_agent: Option<String>,
}

/// `accounts-list` 每个号一行。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(rename_all = "camelCase")]
pub struct AccountRow {
    #[serde(flatten)]
    #[cfg_attr(any(test, feature = "ts"), ts(flatten))]
    pub scanned: ScannedAccount,
    /// `configDir` 的显示形（这台按自己的家目录写好的 `~/…`，界面照抄）；账号 0 ⇒ `null`。
    pub config_dir_shown: Option<String>,
    /// API 号在这台 apikey 表里那一行的掩码（只留末四位）；订阅号 / 表里没有 / 没配 key ⇒ `null`。key 本体从不出后端。
    pub key_masked: Option<String>,
    /// 同上，那一行端点的主机（带端口），后端写好、唯一一处（账号行第二行那一句里的主机也是它）；没配端点 ⇒ `null`。
    pub endpoint_host: Option<crate::cells::Words>,
    /// 账号菜单每一项右边那一枚徽章。
    pub badge: Badge,
    /// 号牌上那一两个字（号色那一格里画的，唯一写法在后端）。
    pub plate: crate::cells::Words,
    /// 账号行第二行那一句：类型字（同额度卡首行那一格）· 这个号的那一格（邮箱 · 需登录 · 端点的主机），后端写好，两端照摆。
    pub kind_line: crate::cells::Words,
    /// 这个号紧凑那一格（恢复菜单 · 新会话框 · 手机机器页每号后面那一格；同一份 `quota-read` 成品里取，后端 `with_quota` 写，两端照摆）：
    /// 订阅号 ⇒ 头一个语义位那一格，`text` 是整格的字（位名 · 值 · 重置），`tone` 是那一格的语气（数旧也画 warn）；
    /// 按量号 ⇒ 一格（`按量` · 被拒那一句，`slot` 空串）；账读不出 ⇒ 一格 `5h 读不到`；没出过数的订阅号 · 账上没有 ⇒ 空。至多一格。
    pub slots: Vec<crate::quota::SlotShow>,
    /// 这个号在额度账上那几行（设置页照摆：收到几窗列几行）：同一份 `quota-read` 成品里那一号的 `rows`，后端 `with_quota` 照抄，
    /// 只把抬头里账号行自己有的几格（名 · 类型 · 登录态）去掉、剩被拒到几点 · 无采样（没有就不成那一行）；
    /// 账读不出 ⇒ 一行 `5h 读不到`；账上没有它 ⇒ 空。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[cfg_attr(
        any(test, feature = "ts"),
        ts(optional, as = "Option<crate::quota::Rows>")
    )]
    pub quota: crate::quota::Rows,
    /// 选不了（`selectable: false`）的那一句：需登录 · 没配 key · 不隔离 · 号目录不在；选得了 ⇒ `null`。
    /// 账号 0 不经选号（起它走「不指定号」那一条）⇒ 恒 `null`。
    pub unselectable: Option<crate::cells::Words>,
    /// 这个号那一家的 MCP 登录不在号之间同步（`mcp_logins_exec::logins_shared` 判 `None`）⇒ 那一句，两端第一行照摆；同步 ⇒ 缺。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub mcp_logins: Option<crate::cells::Words>,
    /// 给不给［登录…］：需登录、而这一家能在 app 里登录 ⇒ `true`（需登录却登不了的，`unselectable` 那一句说为什么）。
    pub login: bool,
    /// 给不给［退出登录］：这一家声明了退出那一条、这个号是登着的订阅号 ⇒ `true`（`login-out`）；缺 ＝ 不给。
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional, as = "Option<bool>"))]
    pub sign_out: bool,
    /// 给不给［登录状态］：这一家声明了状态那一条、这个号是订阅号 ⇒ `true`（`login-status`）；缺 ＝ 不给。
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional, as = "Option<bool>"))]
    pub status: bool,
    /// 进不进新建会话的选号表：隔离、有配置目录 ⇒ `true`（需登录的照样列、灰着，选不选得了看 `selectable`）；
    /// 账号 0（不经选号）· 原地模式 ⇒ `false`。两个出口照它列，不自己判。
    pub new_session: bool,
}

/// 一个号那一枚徽章（账号菜单每一项右边那一格）：写好的字 · 要不要标警示。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct Badge {
    pub text: crate::cells::Words,
    pub warn: bool,
}

// ── session-account 的回包 ────────────────────────────────────────────────────────────────

/// 这个号是此刻在跑的 · 上次用的 · 不知道。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub enum SessionAccountWhen {
    Now,
    Last,
    Unknown,
}

/// 标签页上那颗号徽章（只在这个会话的号与那台默认号不一样、那台的号查得了时出）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct SessionAccountBadge {
    /// 画成「上次的」（幽灵头像）：不是此刻在跑的那个号。
    pub ghost: bool,
    /// 悬停那一句。
    pub title: String,
}

/// `session-account` 的回包：一条会话的号，会话那台判好、写好的（后端 `faces/session_account_face.rs` 拼）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct SessionAccountReply {
    /// 号名（头像按它取色；`_` ＝ 没指定号）；不知道 ⇒ `null`。
    pub account: Option<String>,
    /// 按钮上的名字（号名表里的叫法；不知道 ⇒ 那一句「未实时显示」）。
    pub name: String,
    /// 生成的类型里就地写开（不另出一份文件）。
    #[cfg_attr(any(test, feature = "ts"), ts(inline))]
    pub when: SessionAccountWhen,
    /// 轮换换过号 ⇒ 开会话的那个号（按钮前那个 ⇄ · 悬停卡末行）；没换过 ⇒ `null`。
    pub from: Option<String>,
    /// 标签页悬停卡那一行「账号 X」；号不知道 ⇒ `null`。
    pub account_line: Option<String>,
    /// 标签页悬停卡那一行「从 X 换过来」；没换过 ⇒ `null`。
    pub from_line: Option<String>,
    /// 用量那几格（额度账里在用那个号那一条）；没走中转 · 账上没有 ⇒ `null`（只画名字）。
    pub usage: Option<crate::quota::Usage>,
    /// 那一条最后一次看到距今超过 30 分钟。
    pub stale: bool,
    /// 悬停卡的行：一格一个字与语气（额度账那一段照它写好的格，出口照语气上色）。
    pub hover: crate::quota::Rows,
    /// 几点采的那一句（「采样于 12:03」，号名那一格的悬停提示，不成行）：出口在后面接上问的是哪台（那台叫什么是出口那一侧的事）；没有 ⇒ `null`。
    #[cfg_attr(any(test, feature = "ts"), ts(type = "string | null"))]
    pub sampled: Option<String>,
    /// 这个号与那台的默认号都确知、而且不一样。
    pub differs: bool,
    #[cfg_attr(any(test, feature = "ts"), ts(inline))]
    pub badge: Option<SessionAccountBadge>,
    /// 上游是自设的（用户自己的网关）⇒ 写好的那一句（额度看不到 · 换号不适用）：这时不画用量、不出徽章，面板照它出一条提示。不是 ⇒ 不上线。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub own_upstream: Option<String>,
}

/// `accounts-list` 的入参。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(deny_unknown_fields)]
pub struct AccountsListReq {
    /// 这次起会话的是哪一家（线上的 kind，同 `session-open-facts` 给的那一格；空串 ⇒ 默认那一家，注册表里没有 ⇒ `bad_args`）。
    pub agent: crate::AgentKind,
}
