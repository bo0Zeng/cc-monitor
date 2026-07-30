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

/// manifest 里的一个账号（daemon 已剔除 configDir 不安全的条目）。
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
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
    #[serde(default)]
    pub logged_in: bool,
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
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SessionAccount {
    pub pid: u32,
    pub session_id: Option<String>,
    pub cwd: Option<String>,
    pub config_dir: Option<String>,
    /// configDir 反查 manifest 得到的账号名；查不到 = `None`（**不猜**）。
    pub account: Option<String>,
    /// 进程活着但没设 `CLAUDE_CONFIG_DIR`（迁移后不该出现）。
    #[serde(default)]
    pub bare: bool,
    #[serde(default)]
    pub alive: bool,
}

#[derive(serde::Serialize, Debug, Clone, Default)]
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
