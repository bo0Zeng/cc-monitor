//! 账号库那几条帧命令（`accounts-init` · `accounts-add` · `accounts-remove` · `accounts-set-default` · `accounts-repair` ·
//! `accounts-isolate` · `accounts-rollback` · `accounts-verify` · `accounts-login-cmd`）的**线上形状** —— 两侧同一份：
//! 后端按这里的入参严格收（多一个键 ⇒ 拒）、按这里的成品回；界面用从这里导出的类型（`src/frontend/ui/generated/`）。

use serde::{Deserialize, Serialize};

/// 一个号靠什么鉴权。字面量与清单里 `authKind` 那一格同一套（[`crate::AUTH_KIND_SUBSCRIPTION`] · [`crate::AUTH_KIND_API_KEY`]）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../../frontend/ui/generated/"))]
pub enum AccountKind {
    #[serde(rename = "subscription")]
    Subscription,
    #[serde(rename = "api-key")]
    ApiKey,
}

/// `accounts-init`：建账号库，把这台机器现在登录的那个身份收成名叫 `name` 的默认号。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../../frontend/ui/generated/"))]
pub struct AccountInitArgs {
    pub name: String,
    /// 真 ⇒ 只算不做（回将要做的那几步）。
    #[cfg_attr(test, ts(optional))]
    pub dry_run: Option<bool>,
}

/// `accounts-add`：新建一个号。订阅号可以从一份已有的凭据文件导入；API 号带端点与 key（key 只进那台的 apikey 表，不回显）。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../../frontend/ui/generated/"))]
pub struct AccountAddArgs {
    pub name: String,
    pub kind: AccountKind,
    /// 订阅号：导入哪一份凭据文件（家目录底下；`~/` 开头也认）。缺席 ⇒ 建好之后在终端里登录。
    #[cfg_attr(test, ts(optional))]
    pub cred_file: Option<String>,
    /// API 号：上游地址（缺席 / 空 = 这个 agent 的默认上游）。
    #[cfg_attr(test, ts(optional))]
    pub base_url: Option<String>,
    /// API 号：key 明文。只在入方向出现一次。
    #[cfg_attr(test, ts(optional))]
    pub key: Option<String>,
    /// 真 ⇒ 建好后它是默认号（`ccm` 不带 `--account` 时用它）。
    #[cfg_attr(test, ts(optional))]
    pub is_default: Option<bool>,
    #[cfg_attr(test, ts(optional))]
    pub dry_run: Option<bool>,
}

/// `accounts-remove`：删一个号（只删它自己的目录，共享库不动；改动前留备份）。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../../frontend/ui/generated/"))]
pub struct AccountRemoveArgs {
    pub name: String,
    /// 删的是默认号时必须给真。
    #[cfg_attr(test, ts(optional))]
    pub force: Option<bool>,
    #[cfg_attr(test, ts(optional))]
    pub dry_run: Option<bool>,
}

/// `accounts-set-default` · `accounts-login-cmd`：只带一个账号名。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../../frontend/ui/generated/"))]
pub struct AccountNameArgs {
    pub name: String,
    #[cfg_attr(test, ts(optional))]
    pub dry_run: Option<bool>,
}

/// `accounts-repair`：补齐共享链接 · 修权限 · 刷新邮箱 · 补账号别名（幂等）。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../../frontend/ui/generated/"))]
pub struct AccountRepairArgs {
    #[cfg_attr(test, ts(optional))]
    pub dry_run: Option<bool>,
}

/// `accounts-isolate`：把共享库里的一项变成每个号各自一份（共享库那份留作新号的模板）。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../../frontend/ui/generated/"))]
pub struct AccountIsolateArgs {
    pub item: String,
    #[cfg_attr(test, ts(optional))]
    pub dry_run: Option<bool>,
}

/// `accounts-rollback`：按一份备份把改动还原。`backup` 缺席 ⇒ 最近一份还没还原过的。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../../frontend/ui/generated/"))]
pub struct AccountRollbackArgs {
    #[cfg_attr(test, ts(optional))]
    pub backup: Option<String>,
    #[cfg_attr(test, ts(optional))]
    pub dry_run: Option<bool>,
}

/// 刚建好的那个号。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../../frontend/ui/generated/"))]
pub struct AccountRef {
    pub name: String,
    pub config_dir: String,
}

/// 建号 / 删号那一刻，一份别名文件（`~/.cc-monitor/aliases.sh` · Windows 上另有 `aliases.ps1`）这一趟怎么样了。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../../frontend/ui/generated/"))]
pub struct AliasChange {
    pub path: String,
    /// 这一趟真写了没有（内容没变 ⇒ 一个字节不写）。
    pub changed: bool,
    /// 建号：加进去的别名（如 `zcc` `zcct`）。
    pub added: Vec<String>,
    /// 删号：删掉的别名（参数指向这个号的全部）。
    pub removed: Vec<String>,
    /// 建号：名字被别的别名占着、没加的那几个。
    pub skipped: Vec<String>,
    /// 没能自动改它时的那句话（比如文件里有认不出的行）；`None` = 一切照常。
    pub note: Option<String>,
}

/// 改账号库那几条命令的成品（`dryRun` 为真时 `applied` 恒假、`steps` 是将要做的那几步）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Default)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../../frontend/ui/generated/"))]
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
    /// `accounts-add` 的订阅号没导入凭据：在终端里跑这一行登录（claude 自己的登录界面）。
    pub login_cmd: Option<String>,
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
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../../frontend/ui/generated/"))]
pub enum CheckLevel {
    Ok,
    Warn,
    Fail,
    Skip,
}

/// 核对里的一条。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../../frontend/ui/generated/"))]
pub struct VerifyCheck {
    pub level: CheckLevel,
    /// 说的是哪个号；全局的那几条 ⇒ `None`。
    pub account: Option<String>,
    pub text: String,
}

/// `accounts-verify` 的成品：链接 · 权限 · 邮箱逐项核一遍（只读）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../../frontend/ui/generated/"))]
pub struct VerifyReport {
    /// 没有一条 `fail`。
    pub pass: bool,
    pub fails: u32,
    pub warns: u32,
    pub checks: Vec<VerifyCheck>,
}

/// `accounts-login-cmd` 的成品：在终端里起 claude 登录这个号的那一行（已 quote）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../../frontend/ui/generated/"))]
pub struct AccountLoginCmd {
    pub cmd: String,
}

/// `accounts-mcp-remove`：从这台各号共用的用户级 MCP 里删一条（所有号一起撤）。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../../frontend/ui/generated/"))]
pub struct AccountMcpNameArgs {
    pub name: String,
}

/// `accounts-mcp-pick`：两边都改了的那一条，用哪一版。`from` = 那个号里的那一版；缺席 / `null` = 共享的那一版。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../../frontend/ui/generated/"))]
pub struct AccountMcpPickArgs {
    pub name: String,
    #[cfg_attr(test, ts(optional))]
    pub from: Option<String>,
}

/// 冲突里的一版（只带号名，不带定义 —— 定义里可能有密钥）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../../frontend/ui/generated/"))]
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
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../../frontend/ui/generated/"))]
pub struct AccountMcpConflict {
    pub name: String,
    pub choices: Vec<AccountMcpChoice>,
}

/// `accounts-mcp-read` · `accounts-mcp-remove` · `accounts-mcp-pick` 的成品：这台各号共用的用户级 MCP 此刻的样子。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Default)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../../frontend/ui/generated/"))]
pub struct AccountMcpView {
    /// 这台有没有账号库（没有 ⇒ 不做同步，下面几格都是空的）。
    pub enabled: bool,
    /// 共享集合里的名字（排好序）。
    pub servers: Vec<String>,
    pub conflicts: Vec<AccountMcpConflict>,
    /// 这一趟改写了哪几个号（只读那一条恒空）。
    pub changed: Vec<String>,
    /// 提示（某个号读不出来 · 这一趟没写进去）。
    pub notes: Vec<String>,
}
