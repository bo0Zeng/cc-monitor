//! A2 monitor 侧：远端多账号（cc-acct-iso）的**只读**查询命令。
//!
//! 账号 = 一个 `CLAUDE_CONFIG_DIR`。本模块只把远端 daemon 的三个只读命令包装成
//! Tauri command，**不做任何注入、不落任何盘**（注入是 A4、UI 是 A3）。
//!
//! # 「不可用」不是错误
//! 旧 daemon 不认这三个命令（`unknown argument` → exit 2 / 无输出），`daemonless`
//! 主机压根没 daemon。这两种情况一律回 `available:false + error:<人话>`，
//! **而不是** `Err`——前端据此把账号功能整体降级隐藏，不弹错误（设计文档 §7 降级矩阵）。
//! 只有「这台远端根本没配」才回 `Err`（那是调用方的 bug）。
//!
//! # 凭据边界
//! daemon 侧已保证不输出任何凭据/密钥内容（见 `accounts_query.rs` 模块文档）。
//! 本模块只做反序列化与转发，不额外读任何文件。

use crate::remote_history::run_list_query;
use crate::ssh_source;

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
    /// Z01：远端 daemon 认不认「configDir 缺席 = 账号 0」。**旧 daemon 不出这个键**
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
/// 为什么不直接把这个枚举也搬进 `acct-core`：`ts_rs` 是 `src-tauri` 的 **dev 依赖**，
/// 而 `generated-boundary-guard` 的扫描面逐字是 `walk("src-tauri/src")`
/// ⇒ 派生放到 `src-tauri/crates/` 底下，那条守卫的两条通用性质对它**整个失效**
/// （那正是它 C04d 批 3 栽过的那个坑）。⇒ 派生留在扫描面内，字面量共享。
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../src/generated/"))]
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

/// manifest 里的一个账号（daemon 已剔除 configDir 不安全的条目）。
///
/// **K-A1 起这份结构是 TS 侧 `Account` 的生成源**（`src/generated/RemoteAccount.ts`）。
/// 在此之前两侧靠一行「对齐 A2 的返回结构」的注释对齐 —— 那句注释是纪律，不是判据：
/// 往一侧加字段没有任何门禁会红。现在往这里加字段而不跑 `npm run gen:types`，
/// `generated-boundary-guard` + CI 的 `git diff --exit-code -- src/generated/` 会红。
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../src/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct RemoteAccount {
    pub name: String,
    #[serde(default)]
    pub email: String,
    /// **Z01：可以是 `None`** —— 那就是账号 0（「不设 `CLAUDE_CONFIG_DIR`」这个状态）。
    /// 起它就是**什么都不设**；`Some("")` 是非法拼法，daemon 侧已挡（空值 ≠ 未设）。
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
    /// 显示「订阅凭据在不在」，以及给**旧 daemon**（不出 `authReady`）当回落。
    #[serde(default)]
    pub logged_in: bool,
    /// **K-A1：鉴权方式。`None` = 对面没说** —— 那是**旧 daemon**
    /// （本字段之前的版本压根不出这个键）。消费侧一律当订阅（`KA6d`）。
    ///
    /// ⚠ 它是 `Option` **不是**为了给「未知」留一档语义：这一维上「未知」没有真值
    /// （判可用 = 放宽订阅号的缺凭据保护；判不可用 = 今天所有账号立刻不可选）。
    /// 它是 `Option` 只因为**线上真的会缺**（monitor 连任意版本的远端 daemon）。
    #[serde(
        default,
        deserialize_with = "lenient_auth_kind",
        skip_serializing_if = "Option::is_none"
    )]
    #[cfg_attr(test, ts(optional))]
    pub auth_kind: Option<AuthKind>,
    /// **K-A1：「鉴权方式这一维不再阻塞它被选中」。`None` = 旧 daemon ⇒ 回落到 `logged_in`。**
    ///
    /// 规则的唯一住址是 `acct_core::auth_ready`，两个生产者都调它。
    /// ⚠ `true` **不等于**「真能连上」（`KA6a`），也不等于「凭据有效」（`KA6b`）。
    #[serde(
        default,
        deserialize_with = "lenient_auth_ready",
        skip_serializing_if = "Option::is_none"
    )]
    #[cfg_attr(test, ts(optional))]
    pub auth_ready: Option<bool>,
}

/// **宽容**反序列化 `authReady`：认不出的形状 ⇒ `None`，**不是**硬错。
///
/// # 为什么它也要一份（自审补的：同职的地方要一起治，不能只治撞到的那一处）
///
/// 与 [`lenient_auth_kind`] **同一个失效模式**：这一族里任何一个字段一旦硬错，
/// `parse_accounts_lines` 的「坏行跳过」策略会让**整个账号从列表里消失**
/// —— 而少一行是用户看不见、也没法修的那种坏。
/// `bool` 今天不太可能变形状，但这个理由**不该由字段类型来担保** ——
/// 担保它的是「远端 daemon 的版本我们控制不了」这件事，而那对两个字段一模一样。
fn lenient_auth_ready<'de, D>(d: D) -> Result<Option<bool>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    // 用 `Value` 收，再自己判形状：不是 bool 就当「对面没说」。
    let raw = <Option<serde_json::Value> as serde::Deserialize>::deserialize(d)?;
    Ok(raw.and_then(|v| v.as_bool()))
}

/// **宽容**反序列化 `authKind`：认不出的值 ⇒ `None`，**不是**硬错。
///
/// # 为什么必须宽容（这条是被一次实测逼出来的，不是防御性编程）
///
/// serde 对未知 enum variant 是**硬错**，而 `parse_accounts_lines` 的策略逐字是
/// 「认不出的行**跳过**而不是整体失败」⇒ 写侧（`cc-acct-iso`，另一门语言、另一个发布节奏）
/// 哪天先加了 `bedrock`，读侧还没升，那个账号就**整行从列表里消失**。
/// 静默少一行比判错 kind 坏得多 —— 用户看不见的东西没法修。
/// 实测记录：`tests::an_unrecognized_auth_kind_does_not_silently_drop_the_account`
/// 在加本函数**之前**就是红的（实得 1 个账号，期望 2 个）。
///
/// 「认不出」与「对面没说」都落 `None`，因为它们**该走同一条路**：
/// 当订阅、保留「缺凭据 ⇒ 不可选」那道保护（看得见、可修）。
/// 闭集与分类规则仍然只有一处住址（`acct_core::AUTH_KINDS` + `AuthKind::from_manifest`），
/// 本函数只加「不认识就说不知道」这一条策略。
fn lenient_auth_kind<'de, D>(d: D) -> Result<Option<AuthKind>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let raw = <Option<String> as serde::Deserialize>::deserialize(d)?;
    Ok(raw
        .filter(|s| acct_core::AUTH_KINDS.contains(&s.as_str()))
        .map(|s| AuthKind::from_manifest(Some(&s))))
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
    /// false = 该台拿不到账号能力（daemon 旧 / daemonless / 查询失败）→ 前端降级隐藏。
    pub available: bool,
    pub error: Option<String>,
    pub meta: Option<AccountsMeta>,
    pub accounts: Vec<RemoteAccount>,
    /// Z01：**能用但有缺**时的人话说明（前端应显示）。`available` 仍是 true——
    /// 降级不是不可用。`None` = 无缺。**绝不静默降级**是这条字段存在的全部理由。
    pub notice: Option<String>,
}

/// Z01：列表虽然拿到了，但远端版本旧到「账号 0 看不见」时的人话说明。
/// 两种旧法要分开说，因为要用户做的事不一样（更新 daemon vs 更新 cc-acct-iso）。
pub(crate) fn degraded_notice(meta: &AccountsMeta, accounts: &[RemoteAccount]) -> Option<String> {
    if !meta.enabled {
        return None; // 压根没启用多账号，谈不上缺账号 0
    }
    if !meta.account_zero_aware {
        return Some(
            "远端 daemon 版本较旧：它不认识账号 0（未设 CLAUDE_CONFIG_DIR 的那个默认登录），             列表里会少这一行。更新远端 daemon 后即可看到。"
                .into(),
        );
    }
    if !accounts.iter().any(|a| a.config_dir.is_none()) {
        return Some(
            "远端 cc-acct-iso 版本较旧：它的 accounts.json 里没有账号 0（未设              CLAUDE_CONFIG_DIR 的那个默认登录）。在远端跑一次 'cc-acct-iso sync --apply'              （或重新部署 cc-acct-iso）即可补上。"
                .into(),
        );
    }
    None
}

/// `--session-accounts` 的一行：某个**正在跑**的会话属于哪个账号。
///
/// E79：加 ts-rs 导出 —— 前端此前手抄了一份同名 interface（`src/accounts.ts`），
/// 而本机版查询（`list_local_session_accounts`）要经包装层返回它，正好把手抄那份换掉。
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../src/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct SessionAccount {
    pub pid: u32,
    pub session_id: Option<String>,
    pub cwd: Option<String>,
    pub config_dir: Option<String>,
    /// configDir 反查 manifest 得到的账号名；查不到 = `None`（**不猜**）。
    pub account: Option<String>,
    /// 进程活着但没设 `CLAUDE_CONFIG_DIR`（迁移后不该出现）。
    ///
    /// 🔴 **它的语义钉死在 `CLAUDE_CONFIG_DIR` 这一个变量上**〔`K-P5f` `KP5FD4`〕：
    /// `K-P5f` 给出参加了第二个环境变量（[`Self::launch_id`]），而这个布尔**没有**
    /// 跟着拓宽 —— 「没设 `CCM_LAUNCH_ID`」不进这一格，那由 `launch_id: None` 自己表达。
    /// 让一个布尔同时表示两个变量的缺席，正是「一个值装了两件事」那族病。
    #[serde(default)]
    pub bare: bool,
    #[serde(default)]
    pub alive: bool,
    /// `K-P5f`：起会话方铸进这条会话进程环境的**身份 token**（`CCM_LAUNCH_ID`）。
    ///
    /// `None` = **不作数**，四种原因合并成一个 `None`（**不猜**，同 [`Self::account`]）：
    /// ① 进程没设它；② 值的形状过不了白名单；③ 它同时落在别的活会话上
    /// （继承来的，判不出谁是原主）；④ 进程已死（不读它的 environ）。
    ///
    /// ⚠ **additive**：老 daemon 的出参里**没有这个键**，缺了必须读成 `None`，
    /// **不许把老 daemon 判成坏行**（那会让整条会话账号映射消失，症状是徽章整片没了，
    /// 而没有任何地方说得出为什么）。判据 = `session_account_row_parses` 里那条
    /// **逐字节没有 `launchId` 键**的老 daemon 金样行。
    ///
    /// 🔴 **`#[serde(default)]` 在这一格上不是承重的，写清楚免得后人误读**〔`K-P5f` 第二拍死值验现打〕：
    /// 把它删掉，上面那条金样行**照样绿**（serde 的 derive 对 `Option<T>` 本来就把
    /// 「键缺席」当 `None`）。真正会翻掉 additive 的那一刀是**加一个非 `Option`、
    /// 又没有 `default` 的字段** —— 实打过：临时加一个 `pub probe_required: bool`，
    /// 两条金样当场红（`missing field \`probeRequired\``）。
    /// ⇒ 这个属性留着是**声明意图**（与同结构体里 `bare` / `alive` 那两个 `bool` 一致），
    /// 不是那条 additive 判据的牙。
    #[serde(default)]
    pub launch_id: Option<String>,
}

#[derive(serde::Serialize, Debug, Clone, Default)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../src/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct SessionAccountsResult {
    pub available: bool,
    pub error: Option<String>,
    pub sessions: Vec<SessionAccount>,
}

/// `--account-trust` 结果：目标账号是否已信任某目录（换号 resume 前的预检）。
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct AccountTrustResult {
    #[serde(default)]
    pub available: bool,
    pub error: Option<String>,
    /// 已接受该目录的信任对话框。
    #[serde(default)]
    pub trusted: bool,
    /// 该 cwd 在这个账号的 `.claude.json` 里有记录。`false` ⇒ 首次进入，大概率会弹确认。
    #[serde(default)]
    pub known: bool,
}

/// 解析 `--list-accounts` 的输出行（首行 meta + 每账号一行）。纯函数，供单测。
/// 认不出的行**跳过**而不是整体失败（daemon 将来可能加新 kind）。
pub(crate) fn parse_accounts_lines(lines: &[String]) -> (Option<AccountsMeta>, Vec<RemoteAccount>) {
    let mut meta = None;
    let mut accounts = Vec::new();
    for line in lines {
        let v: serde_json::Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        if v.get("kind").and_then(|k| k.as_str()) == Some("accounts-meta") {
            if let Ok(m) = serde_json::from_value::<AccountsMeta>(v) {
                meta = Some(m);
            }
            continue;
        }
        if let Ok(a) = serde_json::from_value::<RemoteAccount>(v) {
            accounts.push(a);
        }
    }
    (meta, accounts)
}

/// 拼 trust 查询的参数串。纯函数，供单测（拼命令行是注入面，必须能直接断言）。
pub(crate) fn trust_args(config_dir: Option<&str>, cwd: &str) -> String {
    match config_dir {
        // 账号 0：**不传路径**。daemon 那边路径是写死的 $HOME/.claude.json ⇒
        // 这条命令连「任意文件读」的面都没有。
        None => format!("--account-trust-zero {}", ssh_source::shell_quote(cwd)),
        Some(c) => format!(
            "--account-trust {} {}",
            ssh_source::shell_quote(c),
            ssh_source::shell_quote(cwd)
        ),
    }
}

fn unavailable<T: Default>(msg: impl Into<String>) -> T
where
    T: HasAvailability,
{
    let mut t = T::default();
    t.set_unavailable(msg.into());
    t
}

pub(crate) trait HasAvailability {
    fn set_unavailable(&mut self, msg: String);
}
impl HasAvailability for AccountsResult {
    fn set_unavailable(&mut self, msg: String) {
        self.available = false;
        self.error = Some(msg);
    }
}
impl HasAvailability for SessionAccountsResult {
    fn set_unavailable(&mut self, msg: String) {
        self.available = false;
        self.error = Some(msg);
    }
}
impl HasAvailability for AccountTrustResult {
    fn set_unavailable(&mut self, msg: String) {
        self.available = false;
        self.error = Some(msg);
    }
}

/// 取某台远端的配置；`daemonless` 台直接判为"无账号能力"。
fn cfg_for(origin: &str) -> Result<Result<ssh_source::RemoteConfig, String>, String> {
    let cfg = crate::load_remote_config_by_label(origin)
        .ok_or_else(|| format!("远端 '{origin}' 未配置或未启用"))?;
    if cfg.daemonless {
        return Ok(Err(
            "该主机配置为 daemonless（无 daemon），账号功能不可用".into()
        ));
    }
    Ok(Ok(cfg))
}

/// 列出某台远端的账号（读 `$ACCTS_DIR/accounts.json`）。
#[tauri::command]
pub async fn list_remote_accounts(origin: String) -> Result<AccountsResult, String> {
    let cfg = match cfg_for(&origin)? {
        Ok(c) => c,
        Err(msg) => return Ok(unavailable(msg)),
    };
    match run_list_query(&cfg, "--list-accounts").await {
        Err(e) => {
            tracing::warn!("远端 [{origin}] --list-accounts 失败: {e}");
            Ok(unavailable(e))
        }
        Ok(lines) => {
            if lines.is_empty() {
                // 旧 daemon 不认该参数 → exit 2 且 stdout 无输出
                return Ok(unavailable(
                    "远端 daemon 不支持账号查询（版本过旧）——请更新 daemon",
                ));
            }
            let (meta, accounts) = parse_accounts_lines(&lines);
            if meta.is_none() {
                return Ok(unavailable(
                    "远端返回的账号数据无法解析（daemon 版本不匹配？）",
                ));
            }
            let notice = meta.as_ref().and_then(|m| degraded_notice(m, &accounts));
            Ok(AccountsResult {
                available: true,
                error: None,
                meta,
                accounts,
                notice,
            })
        }
    }
}

/// 某台远端上**正在跑**的会话各属于哪个账号（`/proc/<pid>/environ` 探测）。
#[tauri::command]
pub async fn list_remote_session_accounts(origin: String) -> Result<SessionAccountsResult, String> {
    let cfg = match cfg_for(&origin)? {
        Ok(c) => c,
        Err(msg) => return Ok(unavailable(msg)),
    };
    match run_list_query(&cfg, "--session-accounts").await {
        Err(e) => {
            tracing::warn!("远端 [{origin}] --session-accounts 失败: {e}");
            Ok(unavailable(e))
        }
        Ok(lines) => {
            let mut sessions = Vec::new();
            for line in &lines {
                match serde_json::from_str::<SessionAccount>(line) {
                    Ok(s) => sessions.push(s),
                    Err(e) => tracing::warn!("远端 [{origin}] session-accounts 行解析失败: {e}"),
                }
            }
            // 零行是合法的（远端没有活会话）；无法与"旧 daemon"区分，但该命令只用于
            // 补充徽章，降级表现一致（没徽章），故不额外判定。
            Ok(SessionAccountsResult {
                available: true,
                error: None,
                sessions,
            })
        }
    }
}

/// 换号前预检：目标账号是否已信任该工作目录。
/// `config_dir` 必须来自 `list_remote_accounts` 的返回值（daemon 侧还会再校验一次）。
///
/// **Z01**：`config_dir` 为 `None` = 账号 0 ⇒ 走 daemon 的 `--account-trust-zero`
/// （它的 `.claude.json` 在 `$HOME`，不在任何 config dir 里）。**不要**为此传空串：
/// 空串会被 daemon 判成不安全路径并拒掉，用户看到的是一句莫名其妙的错。
#[tauri::command]
pub async fn check_account_trust(
    origin: String,
    config_dir: Option<String>,
    cwd: String,
) -> Result<AccountTrustResult, String> {
    let cfg = match cfg_for(&origin)? {
        Ok(c) => c,
        Err(msg) => return Ok(unavailable(msg)),
    };
    // 位置参数必须各自 posix 引用后再拼进命令行
    let args = trust_args(config_dir.as_deref(), &cwd);
    match run_list_query(&cfg, &args).await {
        Err(e) => {
            tracing::warn!("远端 [{origin}] --account-trust 失败: {e}");
            Ok(unavailable(e))
        }
        Ok(lines) => {
            // daemon 的硬错误走 stderr + exit 2，stdout 无行 → 视为不可用（不阻断编排，
            // 由调用方按"未知信任状态"处理：只警告不拦截）
            let Some(first) = lines.first() else {
                return Ok(unavailable(
                    "远端未返回信任状态（daemon 版本过旧或该 configDir 被拒）",
                ));
            };
            match serde_json::from_str::<AccountTrustResult>(first) {
                Ok(mut r) => {
                    r.available = true;
                    r.error = None;
                    Ok(r)
                }
                Err(e) => Ok(unavailable(format!("信任状态解析失败: {e}"))),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_meta_and_accounts() {
        let lines: Vec<String> = vec![
            r#"{"kind":"accounts-meta","enabled":true,"acctsDir":"/h/.claude-accts","manifestPath":"/h/.claude-accts/accounts.json","updatedAt":"2026-07-23T18:00:00Z","sharedStore":"/h/.claude","count":2,"error":null}"#.into(),
            r#"{"name":"z","email":"z@x.edu","configDir":"/h/.claude-accts/z","isDefault":true,"mode":"isolated","exists":true,"loggedIn":true}"#.into(),
            r#"{"name":"b","email":"","configDir":"/h/.claude-accts/b","isDefault":false,"mode":"isolated","exists":true,"loggedIn":false}"#.into(),
        ];
        let (meta, accts) = parse_accounts_lines(&lines);
        let m = meta.expect("meta 应解析出来");
        assert!(m.enabled);
        assert_eq!(m.count, 2);
        assert_eq!(m.shared_store.as_deref(), Some("/h/.claude"));
        assert_eq!(accts.len(), 2);
        assert_eq!(accts[0].name, "z");
        assert!(accts[0].is_default);
        assert!(accts[0].logged_in);
        assert!(!accts[1].logged_in);
        assert_eq!(accts[1].email, "");
    }

    #[test]
    fn parses_disabled_meta_with_reason() {
        let lines: Vec<String> = vec![
            r#"{"kind":"accounts-meta","enabled":false,"acctsDir":"/h/.claude-accts","manifestPath":"/h/.claude-accts/accounts.json","updatedAt":null,"sharedStore":null,"count":0,"error":"manifest 不可读"}"#.into(),
        ];
        let (meta, accts) = parse_accounts_lines(&lines);
        let m = meta.expect("meta 应解析出来");
        assert!(!m.enabled);
        assert_eq!(m.error.as_deref(), Some("manifest 不可读"));
        assert!(accts.is_empty());
    }

    #[test]
    fn skips_unparsable_lines_instead_of_failing() {
        let lines: Vec<String> = vec![
            r#"{"kind":"accounts-meta","enabled":true,"acctsDir":"/a","manifestPath":"/a/accounts.json","updatedAt":null,"sharedStore":null,"count":1,"error":null}"#.into(),
            "not json at all".into(),
            r#"{"kind":"some-future-kind","x":1}"#.into(),
            r#"{"name":"z","configDir":"/a/z"}"#.into(),
        ];
        let (meta, accts) = parse_accounts_lines(&lines);
        assert!(meta.is_some());
        assert_eq!(accts.len(), 1, "未知 kind 与坏行应被跳过而非整体失败");
        assert_eq!(accts[0].name, "z");
        assert!(!accts[0].logged_in, "缺字段走 default");
    }

    #[test]
    fn session_account_row_parses() {
        let row: SessionAccount = serde_json::from_str(
            r#"{"pid":66936,"sessionId":"9d66c46d","cwd":"/w","configDir":null,"account":null,"bare":true,"alive":true}"#,
        )
        .unwrap();
        assert_eq!(row.pid, 66936);
        assert!(row.bare);
        assert!(row.alive);
        assert!(row.account.is_none());
        // ★ `K-P5f` `KP5FD4` 的 **additive 那一半**：上面这一行是**老 daemon 的出参**
        // （逐字节没有 `launchId` 键）。它必须照样解析成功、读成 `None`。
        // 生产上翻掉它的症状是**整台远端的会话账号映射一条都不剩**（每行解析失败被 warn
        // 跳过），徽章整片消失且没人说得出为什么。
        // ⚠ **翻掉它的那一刀不是删 `#[serde(default)]`**（`Option<T>` 缺键 serde 本来就给
        // `None`，实打过：删掉本条照样绿）——是**加一个非 `Option` 又没有 `default` 的字段**
        // （实打：临时加 `pub probe_required: bool` ⇒ 本条与下一条当场红
        // `missing field \`probeRequired\``）。这一格记在 `SessionAccount::launch_id` 头注里。
        assert!(
            row.launch_id.is_none(),
            "老 daemon 的行缺 launchId 应读成 None，不是报错"
        );
    }

    /// `K-P5f`：新 daemon 那一侧 —— `launchId` 有值时逐字带回来，`null` 读成 `None`。
    ///
    /// ⚠ 这里刻意**不**判「什么时候该是 null」：那是 daemon 侧
    /// `accounts_query::suppress_inherited_launch_ids` 与它的活体夹具的活。
    /// 本条只买「这条线在 monitor 侧接得住」——一个性质两个量法就是本区最贵那族病。
    #[test]
    fn session_account_row_carries_the_launch_identity() {
        let with: SessionAccount = serde_json::from_str(
            r#"{"pid":1,"sessionId":"s","cwd":"/w","configDir":null,"account":null,"bare":true,"alive":true,"launchId":"tok-1"}"#,
        )
        .unwrap();
        assert_eq!(with.launch_id.as_deref(), Some("tok-1"));
        let nulled: SessionAccount = serde_json::from_str(
            r#"{"pid":1,"sessionId":"s","cwd":"/w","configDir":null,"account":null,"bare":true,"alive":true,"launchId":null}"#,
        )
        .unwrap();
        assert!(
            nulled.launch_id.is_none(),
            "daemon 判「不作数」时发的是 null，monitor 侧必须读成 None"
        );
    }

    // ---- Z01：账号 0（configDir 缺席）----

    const META_AWARE: &str = r#"{"kind":"accounts-meta","enabled":true,"acctsDir":"/h/.claude-accts","manifestPath":"/h/.claude-accts/accounts.json","updatedAt":null,"sharedStore":"/h/.claude","count":2,"error":null,"accountZeroAware":true}"#;
    const ACCT_Z: &str =
        r#"{"name":"z","configDir":"/h/.claude-accts/z","mode":"isolated","exists":true}"#;
    const ACCT_ZERO: &str = r#"{"name":"0","email":"me@x.edu","mode":"bare","exists":true,"loggedIn":true,"configDir":null}"#;

    #[test]
    fn account_zero_parses_with_null_config_dir() {
        let lines: Vec<String> = vec![META_AWARE.into(), ACCT_Z.into(), ACCT_ZERO.into()];
        let (meta, accts) = parse_accounts_lines(&lines);
        let m = meta.unwrap();
        assert!(m.account_zero_aware);
        assert_eq!(accts.len(), 2, "账号 0 必须解析出来，不能被当坏行跳过");
        let zero = &accts[1];
        assert_eq!(zero.name, "0");
        assert!(zero.config_dir.is_none(), "必须是 None，不是 Some(\"\")");
        assert_eq!(zero.mode, "bare");
        assert!(zero.logged_in);
        assert!(
            degraded_notice(&m, &accts).is_none(),
            "一切正常时不该有 notice"
        );
    }

    /// configDir 这个键**整个不出现**（而不是显式 null）也一样。
    /// bash 侧写的就是这种形状——两种都得认。
    #[test]
    fn account_zero_parses_with_absent_config_dir_key() {
        let lines: Vec<String> = vec![
            META_AWARE.into(),
            r#"{"name":"0","mode":"bare","exists":true}"#.into(),
        ];
        let (_, accts) = parse_accounts_lines(&lines);
        assert_eq!(accts.len(), 1);
        assert!(accts[0].config_dir.is_none());
    }

    /// 旧 daemon：不出 `accountZeroAware` ⇒ 必须**明说**它会少一行，绝不静默。
    #[test]
    fn old_daemon_gets_an_explicit_notice() {
        let lines: Vec<String> = vec![
            r#"{"kind":"accounts-meta","enabled":true,"acctsDir":"/a","manifestPath":"/a/accounts.json","updatedAt":null,"sharedStore":null,"count":1,"error":null}"#.into(),
            ACCT_Z.into(),
        ];
        let (meta, accts) = parse_accounts_lines(&lines);
        let m = meta.unwrap();
        assert!(!m.account_zero_aware, "缺键 ⇒ default false");
        let n = degraded_notice(&m, &accts).expect("必须给出人话说明");
        assert!(n.contains("daemon"), "要指明是 daemon 旧：{n}");
    }

    /// 新 daemon + 旧 cc-acct-iso：manifest 里根本没有账号 0 ⇒ 也要明说，
    /// 且要指向**另一个**动作（跑 sync / 重新部署 cc-acct-iso），不是更新 daemon。
    #[test]
    fn old_cc_acct_iso_gets_a_different_notice() {
        let lines: Vec<String> = vec![META_AWARE.into(), ACCT_Z.into()];
        let (meta, accts) = parse_accounts_lines(&lines);
        let m = meta.unwrap();
        let n = degraded_notice(&m, &accts).expect("必须给出人话说明");
        assert!(n.contains("cc-acct-iso"), "要指明是 cc-acct-iso 旧：{n}");
        assert!(
            !n.contains("更新远端 daemon"),
            "别把用户指向错误的动作：{n}"
        );
    }

    /// 没启用多账号时不该冒出「缺账号 0」的噪音。
    #[test]
    fn disabled_manifest_has_no_account_zero_notice() {
        let lines: Vec<String> = vec![
            r#"{"kind":"accounts-meta","enabled":false,"acctsDir":"/a","manifestPath":"/a/accounts.json","updatedAt":null,"sharedStore":null,"count":0,"error":"没有 manifest"}"#.into(),
        ];
        let (meta, accts) = parse_accounts_lines(&lines);
        assert!(degraded_notice(&meta.unwrap(), &accts).is_none());
    }

    /// ★ 账号 0 的 trust 查询**不传路径**——传空串会被 daemon 判不安全路径拒掉，
    /// 用户看到一句莫名其妙的错。这条钉住命令行拼法。
    #[test]
    fn trust_args_for_account_zero_passes_no_path() {
        let a = trust_args(None, "/w/proj");
        assert_eq!(a, "--account-trust-zero '/w/proj'");
        assert!(!a.contains("''"), "绝不能出现空串路径参数：{a}");
        let b = trust_args(Some("/h/.claude-accts/z"), "/w/proj");
        assert_eq!(b, "--account-trust '/h/.claude-accts/z' '/w/proj'");
    }

    /// cwd 仍然要被 posix 引用（账号 0 这条新路径不能把注入面漏出来）。
    #[test]
    fn trust_args_quotes_cwd_on_the_account_zero_path() {
        let a = trust_args(None, "/w/it's here; rm -rf /");
        assert!(!a.contains("; rm -rf /'") || a.starts_with("--account-trust-zero '"));
        assert_eq!(
            a,
            format!(
                "--account-trust-zero {}",
                ssh_source::shell_quote("/w/it's here; rm -rf /")
            )
        );
    }

    // ---- K-A1：鉴权方式这一维（这一侧是**中继**，不是生产者） ----

    /// ★ 枚举的 **serde 名**必须逐字等于 `acct-core` 的那两个契约常量。
    ///
    /// 这是本枚举与共享 crate 之间**唯一**的双写点（`kebab-case` 是 derive 算出来的，
    /// 不是我写的字面量）⇒ 改了任一侧，本条红。生成物 `src/generated/AuthKind.ts`
    /// 也是从这两个名字来的，所以 TS 那个字面量联合一并被钉住。
    #[test]
    fn the_wire_names_match_the_shared_contract() {
        assert_eq!(
            serde_json::to_string(&AuthKind::Subscription).unwrap(),
            format!("\"{}\"", acct_core::AUTH_KIND_SUBSCRIPTION)
        );
        assert_eq!(
            serde_json::to_string(&AuthKind::ApiKey).unwrap(),
            format!("\"{}\"", acct_core::AUTH_KIND_API_KEY)
        );
        // 反向：契约里的每个字面量都要能反序列化回来（闭集两头都得通）。
        for k in acct_core::AUTH_KINDS {
            let v: AuthKind = serde_json::from_str(&format!("\"{k}\"")).unwrap();
            assert_eq!(v.as_contract_str(), k);
        }
        // 默认档是订阅（`KA6d`：缺席 ⇒ 订阅，不是 api-key）。
        assert_eq!(AuthKind::default(), AuthKind::Subscription);
        assert_eq!(AuthKind::from_manifest(None), AuthKind::Subscription);
        assert_eq!(
            AuthKind::from_manifest(Some(acct_core::AUTH_KIND_API_KEY)),
            AuthKind::ApiKey
        );
    }

    /// ★ **这一侧是中继，不是第三个生产者。**
    ///
    /// 件计划 §0 把它写成「三个生产者」，Bx 实测订正：`parse_accounts_lines` 一个字段都不算
    /// （通体只有一句 `from_value::<RemoteAccount>`）。⇒ 它该断的性质不是「产出与另两家相同」
    /// （那会写成一条循环自证），而是**原样透传**：daemon 说什么就是什么，一个字都不改。
    #[test]
    fn the_relay_passes_the_auth_dimension_through_verbatim() {
        let lines: Vec<String> = vec![
            r#"{"kind":"accounts-meta","enabled":true,"acctsDir":"/a","manifestPath":"/a/accounts.json","updatedAt":null,"sharedStore":null,"count":2,"error":null,"accountZeroAware":true}"#.into(),
            r#"{"name":"sub","configDir":"/a/sub","mode":"isolated","exists":true,"loggedIn":true,"authKind":"subscription","authReady":true}"#.into(),
            r#"{"name":"api","configDir":"/a/api","mode":"isolated","exists":true,"loggedIn":false,"authKind":"api-key","authReady":true}"#.into(),
        ];
        let (_, accts) = parse_accounts_lines(&lines);
        assert_eq!(accts.len(), 2);
        assert_eq!(accts[0].auth_kind, Some(AuthKind::Subscription));
        assert_eq!(accts[0].auth_ready, Some(true));
        // ★ KAY2 在中继这一侧的那一格：api-key 号 `loggedIn:false` 但 `authReady:true`，
        // 中继不许「顺手修正」成 false。
        assert_eq!(accts[1].auth_kind, Some(AuthKind::ApiKey));
        assert_eq!(accts[1].auth_ready, Some(true));
        assert!(
            !accts[1].logged_in,
            "loggedIn 也得原样透传，不许被 authReady 带着改"
        );
    }

    /// ★ **旧 daemon（两个键都不出）⇒ 中继必须回 `None`，不许悄悄编一个值出来。**
    ///
    /// 这一格是整条降级链的起点：`None` 才让前端能**回落到逐字节旧行为**
    /// （`authReady ?? loggedIn`）。若这里 `#[serde(default)]` 被改成
    /// 「缺键 ⇒ `Some(Subscription)` / `Some(false)`」，前端就分不出
    /// 「对面说它没就绪」和「对面压根没说」——而这两件事该走不同的路。
    #[test]
    fn an_old_daemon_yields_none_not_a_made_up_value() {
        let lines: Vec<String> = vec![
            r#"{"kind":"accounts-meta","enabled":true,"acctsDir":"/a","manifestPath":"/a/accounts.json","updatedAt":null,"sharedStore":null,"count":1,"error":null}"#.into(),
            r#"{"name":"z","configDir":"/a/z","mode":"isolated","exists":true,"loggedIn":true}"#.into(),
        ];
        let (_, accts) = parse_accounts_lines(&lines);
        assert_eq!(accts.len(), 1);
        assert_eq!(accts[0].auth_kind, None, "缺键必须是 None（「对面没说」）");
        assert_eq!(accts[0].auth_ready, None);
        // 而且**序列化回前端时那两个键也不许出现**（`skip_serializing_if`）——
        // 出一个 `null` 会让 TS 那个 `?? ` 回落判据从「缺席」变成「显式空」。
        let json = serde_json::to_string(&accts[0]).unwrap();
        assert!(!json.contains("authKind"), "缺席被序列化出来了：{json}");
        assert!(!json.contains("authReady"), "缺席被序列化出来了：{json}");
        assert!(
            json.contains("\"loggedIn\":true"),
            "别的字段不该跟着掉：{json}"
        );
    }

    /// 认不出的 `authKind` 值（写侧比读侧新）⇒ 整行**不许**被丢掉。
    ///
    /// `serde` 对未知 enum variant 是**硬错**，而 `parse_accounts_lines` 的策略是「坏行跳过」
    /// ⇒ 若不当心，一个 `"authKind":"bedrock"` 会让那个账号**整行消失**（列表静默少一行，
    /// 比判错 kind 更坏）。本条钉住实际行为，别让它变成静默丢账号。
    #[test]
    fn an_unrecognized_auth_kind_does_not_silently_drop_the_account() {
        let lines: Vec<String> = vec![
            r#"{"kind":"accounts-meta","enabled":true,"acctsDir":"/a","manifestPath":"/a/accounts.json","updatedAt":null,"sharedStore":null,"count":2,"error":null}"#.into(),
            r#"{"name":"z","configDir":"/a/z","mode":"isolated","exists":true,"loggedIn":true,"authKind":"bedrock","authReady":true}"#.into(),
            r#"{"name":"b","configDir":"/a/b","mode":"isolated","exists":true,"loggedIn":true}"#.into(),
        ];
        let (_, accts) = parse_accounts_lines(&lines);
        assert_eq!(
            accts.len(),
            2,
            "认不出的 authKind 让整个账号消失了 —— 静默少一行比判错 kind 更坏"
        );
        assert_eq!(accts[0].name, "z");
        assert_eq!(
            accts[0].auth_kind, None,
            "认不出的值该退化成「对面没说」（⇒ 前端当订阅、保留缺凭据保护），不是当 api-key"
        );
    }

    /// ★ `authReady` 那一格**同职同治**（自审补的）：形状不对也不许丢账号。
    ///
    /// 与上一条是同一个失效模式（serde 硬错 + 「坏行跳过」= 静默少一行）。
    /// 先只治撞到的那一处、放着同职的另一处，正是本仓反复栽的那个形。
    #[test]
    fn an_unrecognized_auth_ready_shape_does_not_silently_drop_the_account() {
        let lines: Vec<String> = vec![
            r#"{"kind":"accounts-meta","enabled":true,"acctsDir":"/a","manifestPath":"/a/accounts.json","updatedAt":null,"sharedStore":null,"count":2,"error":null}"#.into(),
            r#"{"name":"z","configDir":"/a/z","mode":"isolated","exists":true,"loggedIn":true,"authKind":"api-key","authReady":"partial"}"#.into(),
            r#"{"name":"b","configDir":"/a/b","mode":"isolated","exists":true,"loggedIn":true}"#.into(),
        ];
        let (_, accts) = parse_accounts_lines(&lines);
        assert_eq!(accts.len(), 2, "authReady 形状不对让整个账号消失了");
        assert_eq!(
            accts[0].auth_kind,
            Some(AuthKind::ApiKey),
            "kind 该照样透传"
        );
        assert_eq!(
            accts[0].auth_ready, None,
            "形状不对 ⇒「对面没说」⇒ 前端回落 loggedIn，而不是硬错丢行"
        );
        // 正常 bool 仍然要认出来 —— 否则上面那条是空真。
        let ok: Vec<String> = vec![
            r#"{"kind":"accounts-meta","enabled":true,"acctsDir":"/a","manifestPath":"/a/x.json","updatedAt":null,"sharedStore":null,"count":1,"error":null}"#.into(),
            r#"{"name":"z","configDir":"/a/z","mode":"isolated","exists":true,"loggedIn":false,"authKind":"api-key","authReady":true}"#.into(),
        ];
        let (_, ok_accts) = parse_accounts_lines(&ok);
        assert_eq!(ok_accts[0].auth_ready, Some(true));
    }

    #[test]
    fn unavailable_helper_sets_flag_and_reason() {
        let r: AccountsResult = unavailable("daemon 太旧");
        assert!(!r.available);
        assert_eq!(r.error.as_deref(), Some("daemon 太旧"));
        assert!(r.accounts.is_empty());
        assert!(r.meta.is_none());
        let s: SessionAccountsResult = unavailable("x");
        assert!(!s.available);
        let t: AccountTrustResult = unavailable("y");
        assert!(!t.available);
        assert!(!t.trusted, "不可用时不得默认判为已信任");
    }
}
