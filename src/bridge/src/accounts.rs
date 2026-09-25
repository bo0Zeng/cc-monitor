//! 账号清单的**形状**（`AccountsMeta` / `RemoteAccount` / `AuthKind` / `AccountsResult`）。
//!
//! 〔C4c · 第四波 4B〕本模块**不再有任何一条 Tauri 命令**：A2 那两条账号清单（远端 `list_remote_accounts`、
//! 本机那条在 `local_accounts.rs`）与换号前的信任预检（`check_account_trust`）退役 —— 前端经通道直接说帧命令
//! `accounts-list` / `accounts-trust`，**那台机器的后端出成品**（`src/backend/observe/accounts_query.rs::list_product`，
//! 并表规则住 `acct-core`）。这里那一份行解析（`parse_accounts_lines`）· 降级说明（`degraded_notice`）· 〔散文墓碑〕
//! trust 拼参与解析〔散文墓碑〕一起删了。
//!
//! 留下的只有形状：`RemoteAccount` / `AuthKind` 仍是 TS 侧 `Account` / `AuthKind` 的生成源（`src/generated/`），
//! 本机那份参照实现（`local_accounts.rs::list_from_dir`，零生产调用方、只被判据驱动，理由见它的头注）仍产出这份结构。
//!
//! # 凭据边界
//! backend 侧已保证不输出任何凭据/密钥内容（见 `accounts_query.rs` 模块文档）。

/// `--list-accounts` 的首行 meta。
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AccountsMeta {
    /// 远端是否已启用多账号（manifest 可读且 schema 受支持）。
    pub enabled: bool,
    pub accts_dir: String,
    pub manifest_path: String,
    pub updated_at: Option<String>,
    pub shared_store: Option<String>,
    pub count: u32,
    /// `enabled:false` 时的人话原因（给部署引导用）。
    pub error: Option<String>,
    /// Z01：远端后端认不认「configDir 缺席 = 账号 0」。**旧后端不出这个键**
    /// ⇒ `false` ⇒ 它会把账号 0 当坏数据跳过，列表里就少一行。见 `degraded_notice`。
    #[serde(default)]
    pub account_zero_aware: bool,
}

/// 一个账号的**鉴权方式**（K-A1）。
///
/// # 为什么是枚举而不是 `bool`
///
/// 这一维今天就看得见第三档（`apiKeyHelper` / bedrock / vertex 各是一种鉴权方式）。
/// 而本仓已经因为「布尔装两件事」栽过一次：`unified-backend` 记着 pidfile 的 `kind`
/// 被写成「非 `interactive` 即隐藏」，第三档来的时候那个布尔装不下。
///
/// # 取值的**唯一住址**是 `acct-core`
///
/// 两个字面量（`subscription` / `api-key`）住 `acct_core::AUTH_KIND_*`，
/// 本枚举的 serde 名与它们由 `tests::the_wire_names_match_the_shared_contract` 钉住。
/// 为什么不直接把这个枚举也搬进 `acct-core`：`ts_rs` 是 `src/bridge` 的 **dev 依赖**，
/// 而 `generated-boundary-guard` 的扫描面逐字是 `walk("src/bridge/src")`
/// ⇒ 派生放到 `src/bridge/crates/` 底下，那条守卫的两条通用性质对它**整个失效**
/// （那正是它 C04d 批 3 栽过的那个坑）。⇒ 派生留在扫描面内，字面量共享。
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../../src/generated/"))]
#[serde(rename_all = "kebab-case")]
pub enum AuthKind {
    /// 订阅登录：可用性看 `.credentials.json` 在不在（**逐字节旧行为**）。
    #[default]
    Subscription,
    /// API key 鉴权：**不看**那个凭据文件。
    ///
    /// ⚠ 它今天「选得中、起得来、但请求发不出去」（件计划 `KA6a`）——
    /// 配端点那条路还没有。UI 必须把这个状态说出来，落点
    /// `src/accounts.ts::accountStatusBadge`。
    ApiKey,
}

/// manifest 里的一个账号（backend 已剔除 configDir 不安全的条目）。
///
/// **K-A1 起这份结构是 TS 侧 `Account` 的生成源**（`src/generated/RemoteAccount.ts`）。
/// 在此之前两侧靠一行「对齐 A2 的返回结构」的注释对齐 —— 那句注释是纪律，不是判据：
/// 往一侧加字段没有任何门禁会红。现在往这里加字段而不跑 `npm run gen:types`，
/// `generated-boundary-guard` + CI 的 `git diff --exit-code -- src/generated/` 会红。
// 〔C4c · 第四波 4B〕它**不再从线上反序列化**（清单由那台机器的后端出成品、界面严格收，
// `src/accounts.ts::decodeAccountsList`）⇒ `Deserialize` 与那两个「宽容反序列化」的帮手〔散文墓碑〕一起摘了：
// 那两个帮手治的是「monitor 逐行解析时一格硬错会让整个账号静默消失」，今天 monitor 这一侧一行都不解析。
// （写成 `//` 而不是文档注释：ts-rs 会把文档注释抄进 `src/generated/RemoteAccount.ts`。）
#[derive(serde::Serialize, Debug, Clone)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../../src/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct RemoteAccount {
    pub name: String,
    #[serde(default)]
    pub email: String,
    /// **Z01：可以是 `None`** —— 那就是账号 0（「不设 `CLAUDE_CONFIG_DIR`」这个状态）。
    /// 起它就是**什么都不设**；`Some("")` 是非法拼法，backend 侧已挡（空值 ≠ 未设）。
    #[serde(default)]
    pub config_dir: Option<String>,
    #[serde(default)]
    pub is_default: bool,
    /// `isolated`（正常）/ `in-place`（逃生口，前端应拒绝使用）/ `bare`（账号 0）。
    #[serde(default)]
    pub mode: String,
    #[serde(default)]
    pub exists: bool,
    /// 仅 stat `.credentials.json` 存在性得来，**不代表凭据有效**。
    ///
    /// ⚠ **K-A1 起它不再是可用性判据** —— 可用性走 `auth_ready`（订阅号那一支的值与它
    /// 逐字节相同，api-key 号那一支不看这个文件）。它留下来只做两件事：
    /// 显示「订阅凭据在不在」，以及给**旧 backend**（不出 `authReady`）当回落。
    #[serde(default)]
    pub logged_in: bool,
    /// **K-A1：鉴权方式。`None` = 对面没说** —— 那是**旧 backend**
    /// （本字段之前的版本压根不出这个键）。消费侧一律当订阅（`KA6d`）。
    ///
    /// ⚠ 它是 `Option` **不是**为了给「未知」留一档语义：这一维上「未知」没有真值
    /// （判可用 = 放宽订阅号的缺凭据保护；判不可用 = 今天所有账号立刻不可选）。
    /// 它是 `Option` 只因为**线上真的会缺**（monitor 连任意版本的远端后端）。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub auth_kind: Option<AuthKind>,
    /// **K-A1：「鉴权方式这一维不再阻塞它被选中」。`None` = 旧 backend ⇒ 回落到 `logged_in`。**
    ///
    /// 规则的唯一住址是 `acct_core::auth_ready`，两个生产者都调它。
    /// ⚠ `true` **不等于**「真能连上」（`KA6a`），也不等于「凭据有效」（`KA6b`）。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub auth_ready: Option<bool>,
}

impl AuthKind {
    /// manifest 的 `authKind` 键 → 本枚举。**分类规则不在这儿** ——
    /// 它住 `acct_core::auth_kind_from_manifest`，本函数只把那个结果映到枚举上。
    pub fn from_manifest(raw: Option<&str>) -> Self {
        if acct_core::auth_kind_from_manifest(raw) == acct_core::AUTH_KIND_API_KEY {
            Self::ApiKey
        } else {
            Self::Subscription
        }
    }

    /// 本枚举 → `acct-core` 的字面量（喂给 `acct_core::auth_ready`）。
    pub fn as_contract_str(self) -> &'static str {
        match self {
            Self::Subscription => acct_core::AUTH_KIND_SUBSCRIPTION,
            Self::ApiKey => acct_core::AUTH_KIND_API_KEY,
        }
    }
}

#[derive(serde::Serialize, Debug, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct AccountsResult {
    /// false = 该台拿不到账号能力（backend 旧 / 查询失败）→ 前端降级隐藏。
    pub available: bool,
    pub error: Option<String>,
    pub meta: Option<AccountsMeta>,
    pub accounts: Vec<RemoteAccount>,
    /// Z01：**能用但有缺**时的人话说明（前端应显示）。`available` 仍是 true——
    /// 降级不是不可用。`None` = 无缺。**绝不静默降级**是这条字段存在的全部理由。
    pub notice: Option<String>,
}

#[cfg(test)]
#[path = "../../../tests/bridge/accounts_tests.rs"]
mod tests;
