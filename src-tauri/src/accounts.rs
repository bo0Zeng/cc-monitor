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
}

/// manifest 里的一个账号（daemon 已剔除 configDir 不安全的条目）。
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RemoteAccount {
    pub name: String,
    #[serde(default)]
    pub email: String,
    pub config_dir: String,
    #[serde(default)]
    pub is_default: bool,
    /// `isolated`（正常）/ `in-place`（逃生口，前端应拒绝使用）。
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
            Ok(AccountsResult {
                available: true,
                error: None,
                meta,
                accounts,
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
#[tauri::command]
pub async fn check_account_trust(
    origin: String,
    config_dir: String,
    cwd: String,
) -> Result<AccountTrustResult, String> {
    let cfg = match cfg_for(&origin)? {
        Ok(c) => c,
        Err(msg) => return Ok(unavailable(msg)),
    };
    // 两个位置参数必须各自 posix 引用后再拼进命令行
    let args = format!(
        "--account-trust {} {}",
        ssh_source::shell_quote(&config_dir),
        ssh_source::shell_quote(&cwd)
    );
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
