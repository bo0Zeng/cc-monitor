//! 假 agent 的账号面（`AccountsFace`）：账号环境变量名 · 信任判定与信任那一格 · 会话环境的键 · 续期。

use super::FAKE_FILE_CAP;
use crate::platform::fs::Ro;
use std::path::Path;

/// 能力 5：账号（配置根）环境变量名。
pub const ACCOUNT_DIR_ENV: &str = "CCM_FAKE_ACCOUNT_DIR";

/// 能力 6：某个配置根下、对某个 cwd 的信任判定。
///
/// 形状与 Claude 那家**不同**：文件名不同、字段路径不同（`trusted[cwd]` 而不是
/// `projects[cwd].hasTrustDialogAccepted`）。
pub fn trust_of_config(root: &Path, cwd: &str) -> Result<bool, String> {
    let p = root.join("fake-config.json");
    let bytes =
        Ro::read_capped(&p, FAKE_FILE_CAP).map_err(|e| format!("fake_config_unreadable: {e}"))?;
    let v: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|e| format!("fake_config_invalid: {e}"))?;
    Ok(v.get("trusted")
        .and_then(|t| t.get(cwd))
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false))
}

/// 账号库面：只带通用判据会问的那两格（会话进程环境的键 · 信任预检），其余空着。
pub(super) fn accounts_face(account_env: &'static str) -> crate::agents::AccountsFace {
    crate::agents::AccountsFace {
        identity: &[],
        shared: &[],
        shared_on_demand: &[],
        config_file: "fake-config.json",
        mcp: Some(MCP_CELLS),
        mcp_logins: Some(MCP_LOGINS),
        shared_root: |h| h.join("shared"),
        email_in: |_| None,
        login: Some(LOGIN),
        sign_in: None,
        session_env: crate::agents::SessionEnvKeys {
            config_dir: account_env,
            // 与 Claude 那一行不同形：不从环境变量读中转地址（经参数注入的那一形）。
            base_url: None,
            settings_may_set_base_url: |_, _, _| false,
        },
        trust: Some(TRUST_CELLS),
        // 与 Claude 那一行不同形：只在没有活会话用这个号时续。
        renew: crate::agents::Renew::Idle,
    }
}

/// 订阅号登录那一格：凭据文件名与 Claude · Codex 都不同（判「登没登 · 导入写成哪一名只问那一家声明的那一份」用）。
/// 与 Claude 那一行每一格都不同形（判「核心照这几格续、不认哪一家」用）：
/// - 过期时刻不记在一格里，交这一家读：顶层 `stamped`（上次续的那一秒）＋ 一小时；
/// - 出处记在凭据里：`session.issuer`（令牌端点 ＝ `<issuer>/fake/token`）· `session.app`（客户端 id）；
/// - 整份写锁是文件锁 `fake-write.lock`；续成另记顶层 `stamped`；
/// - 续期不拿锁；不记授权范围；续成另换 `session.badge`（回包 `id_token`，不是 JWT 的形状 ⇒ 整次不落盘）；
/// - 账号 0 不续不写。
///
/// 缺省令牌端点指向本机一个没人听的口：没记出处的号续不出东西。
const LOGIN: crate::agents::LoginFace = crate::agents::LoginFace {
    creds_file: "fake-login.ndjson",
    section: "session",
    access: "bearer",
    refresh: "renewal",
    expiry: crate::agents::Expiry::Read(|doc| {
        let s = doc.get(STAMPED)?.as_u64()?;
        Some(s.saturating_mul(1000).saturating_add(3_600_000))
    }),
    // 与 Claude 那一行不同形：不记授权范围（续期不带 scope、回包里的不写）。
    scopes: None,
    // 与 Claude 那一行不同形：续成另换一格 `session.badge`（回包 `id_token`），先核它是 JWT 的形状。
    renewed_cells: &[crate::agents::RenewedCell {
        answer: "id_token",
        cell: "badge",
        check: Some(jwt_shaped),
    }],
    client_field: "app",
    token_url: "http://127.0.0.1:9/fake/token",
    client_id: "fake-client",
    origin: Some(|doc| {
        let issuer = doc.get("session")?.get("issuer")?.as_str()?;
        Some(crate::agents::LoginOrigin {
            token_url: format!("{}/fake/token", issuer.trim_end_matches('/')),
            client_id: doc
                .get("session")
                .and_then(|s| s.get("app"))
                .and_then(serde_json::Value::as_str)
                .map(str::to_string),
        })
    }),
    renewed_at: Some((STAMPED, |ms| serde_json::Value::from(ms / 1000))),
    margin_ms: 60_000,
    // 与 Claude 那一行不同形：续期不拿锁（号目录里不多一样东西）。
    lock_inside: None,
    lock_beside: None,
    lock_stale_ms: 60_000,
    write_lock: Some(crate::agents::WriteLock::File("fake-write.lock")),
    write_lock_stale_ms: 15_000,
    write_wait_ms: 20_000,
    identity_file: |dir, _| dir.join("fake-login.ndjson"),
    identity_in: |_| None,
    identity_of_body: |_| None,
    rewrite_identity: |_, _| crate::agents::IdentityCell::Absent,
    // 与 Claude 那一行不同形：账号 0 不续不写（号载体那几个号照续）。
    base_login: crate::agents::BaseLogin::Untouched,
};

/// 凭据顶层那一格「上次续的那一秒」。
const STAMPED: &str = "stamped";

/// JWT 的形状：三段非空、只含 base64url 字符，点隔开（不解、不验签）。
fn jwt_shaped(s: &str) -> bool {
    let parts: Vec<&str> = s.split('.').collect();
    parts.len() == 3
        && parts.iter().all(|p| {
            !p.is_empty()
                && p.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        })
}

/// 另一形信任格：一种行式配置（不是 JSON），由这一家自己声明改法与读法（判「核心不只认 JSON」用）。
/// 每行一项：空行 · `# 注释` · `键 = 值` · `trust <目录>`；别的行 ⇒ 读不懂（不写）。标信任 ⇒ 缺的目录各追加一行 `trust <目录>`，别的字节原样。
pub const LINE_TRUST_CELLS: crate::agents::TrustCells = crate::agents::TrustCells {
    table: "",
    flag: None,
    dir_keys: |cwd| vec![cwd.to_string()],
    edit: Some(crate::agents::TrustEdit {
        trusted: line_trusted,
        mark: line_mark,
    }),
};

fn line_trusted(text: &str) -> Result<std::collections::BTreeSet<String>, String> {
    let mut out = std::collections::BTreeSet::new();
    for (i, line) in text.lines().enumerate() {
        let l = line.trim();
        if l.is_empty() || l.starts_with('#') || l.contains(" = ") {
            continue;
        }
        match l.strip_prefix("trust ") {
            Some(dir) if !dir.is_empty() => {
                out.insert(dir.to_string());
            }
            _ => return Err(format!("第 {} 行读不懂", i + 1)),
        }
    }
    Ok(out)
}

fn line_mark(
    text: &str,
    dirs: &std::collections::BTreeSet<String>,
) -> Result<Option<String>, String> {
    let have = line_trusted(text)?;
    let missing: Vec<&String> = dirs.iter().filter(|d| !have.contains(*d)).collect();
    if missing.is_empty() {
        return Ok(None);
    }
    let mut out = text.to_string();
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
    for d in missing {
        out.push_str(&format!("trust {d}\n"));
    }
    Ok(Some(out))
}

/// 各号用户级 MCP 那一格：一份行式的非 JSON 配置 `fake-mcp.conf`（与 Claude 的 JSON 顶层键 · Codex 的 TOML 节都不同形）。
/// 一个服务器一行 `server <名> <定义的一行 JSON>`；别的行（注释 · 空行 · 别的设置）读时跳过、改时一个字节不动。
pub const MCP_CELLS: crate::agents::McpCells = crate::agents::McpCells {
    file: MCP_FILE,
    read: read_mcp_lines,
    edit: edit_mcp_line,
};

/// 各号 MCP 那份文件的名字。
pub const MCP_FILE: &str = "fake-mcp.conf";

/// 各号的 MCP 登录那一格：与订阅号登录不同的另一份行式文件 `fake-mcp-logins.conf`（与 Claude 的「凭据文件里一节 JSON」不同形）。
/// 一条一行 `login <键> <那一条的一行 JSON>`；别的行（注释 · 空行 · 别的设置）读时跳过、改时一个字节不动。
/// 锁是两把文件锁（与 Claude 的两把 `mkdir` 锁不同形）：写某一条前先拿 `fake-mcp-locks/<键里非字母数字换成 _>.lock`
/// （一条一份），再拿 `fake-mcp-locks/whole.lock`（整份一把），都是排他文件锁。
const MCP_LOGINS: crate::agents::McpLogins = crate::agents::McpLogins {
    file: "fake-mcp-logins.conf",
    read: |text| read_tagged(text, LOGIN_TAG),
    edit: |text, key, to| edit_tagged(text, LOGIN_TAG, key, to),
    lock: &[
        crate::agents::McpLoginLock::EntryFile {
            name: login_lock_of,
        },
        crate::agents::McpLoginLock::WholeFile {
            name: "fake-mcp-locks/whole.lock",
        },
    ],
    access: "bearer",
    refresh: "renewal",
    expires_ms: "until",
};

const LOGIN_TAG: &str = "login ";

fn login_lock_of(key: &str) -> String {
    let safe: String = key
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    format!("fake-mcp-locks/{safe}.lock")
}

const SERVER: &str = "server ";

/// 一行是不是 `<头><名> <一行 JSON>`：是 ⇒（名, JSON 那一截）。
fn tagged_line<'a>(line: &'a str, tag: &str) -> Option<(&'a str, &'a str)> {
    let rest = line.strip_prefix(tag)?;
    Some(rest.split_once(' ').unwrap_or((rest, "")))
}

fn read_mcp_lines(text: &str) -> Result<serde_json::Map<String, serde_json::Value>, String> {
    read_tagged(text, SERVER)
}

fn edit_mcp_line(
    text: &str,
    name: &str,
    def: Option<&serde_json::Value>,
) -> Result<String, String> {
    edit_tagged(text, SERVER, name, def)
}

/// 行式文本里 `<头>` 开头的那几行 ⇒ 名 → 一行 JSON；别的行跳过。
fn read_tagged(
    text: &str,
    tag: &str,
) -> Result<serde_json::Map<String, serde_json::Value>, String> {
    let mut out = serde_json::Map::new();
    for (i, line) in text.lines().enumerate() {
        let Some((name, def)) = tagged_line(line, tag) else {
            continue;
        };
        let v = serde_json::from_str(def)
            .map_err(|_| format!("fake_mcp_line_unreadable: {}", i + 1))?;
        out.insert(name.to_string(), v);
    }
    Ok(out)
}

/// 只换 `<头><名>` 那一行（`None` = 删那一行；没有 ⇒ 末尾追加一行）；别的行一个字节不动。
fn edit_tagged(
    text: &str,
    tag: &str,
    name: &str,
    def: Option<&serde_json::Value>,
) -> Result<String, String> {
    let fresh = def.map(|v| format!("{tag}{name} {v}"));
    let mut out = String::new();
    let mut done = false;
    for line in text.split_inclusive('\n') {
        let body = line.strip_suffix('\n').unwrap_or(line);
        if tagged_line(body, tag).is_some_and(|(n, _)| n == name) {
            done = true;
            if let Some(f) = &fresh {
                out.push_str(f);
                out.push_str(&line[body.len()..]);
            }
            continue;
        }
        out.push_str(line);
    }
    if let (false, Some(f)) = (done, fresh) {
        if !out.is_empty() && !out.ends_with('\n') {
            out.push('\n');
        }
        out.push_str(&f);
        out.push('\n');
    }
    Ok(out)
}

/// 信任那一格：`fake-config.json` → `trusted[<目录>] = true`（目录那一项本身就是那个布尔；键原样）。
pub const TRUST_CELLS: crate::agents::TrustCells = crate::agents::TrustCells {
    table: "trusted",
    flag: None,
    dir_keys: |cwd| vec![cwd.to_string()],
    edit: None,
};

/// 替身家的登录子命令（判据把同名的一份脚本放在 `PATH` 最前）。
const SIGN_IN_PROGRAM: &str = "fake-login";

/// 替身家认的登录页：`https://login.fake.test/…`，带 `ticket=`（与 Claude 的 `state=` · Codex 的 `/oauth/authorize` 都不同形）。
fn fake_link_in(line: &str) -> Option<String> {
    let at = line.find("https://login.fake.test/")?;
    let url = line[at..].split_whitespace().next()?;
    url.contains("ticket=").then(|| url.to_string())
}

/// 显示码那一形的码：`Your code is` 那一行之后头一个非空行（与 Codex 那一形的提示不同形），只许大写字母 · 数字 · `-`。
fn fake_code_in(lines: &[String]) -> Option<String> {
    let at = lines.iter().position(|l| l.trim() == "Your code is")?;
    let code = lines[at + 1..]
        .iter()
        .map(|l| l.trim())
        .find(|l| !l.is_empty())?;
    code.bytes()
        .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == b'-')
        .then(|| code.to_string())
}

/// 状态那一形的读法：退 0 且说 `signed-in` ⇒ 登着；退 3 或说 `signed-out` ⇒ 没登；别的 ⇒ 读不出。
fn fake_status_read(code: Option<i32>, lines: &[String]) -> Option<bool> {
    let said = |w: &str| lines.iter().any(|l| l.trim() == w);
    match code {
        Some(0) if said("signed-in") => Some(true),
        Some(3) => Some(false),
        Some(0) if said("signed-out") => Some(false),
        _ => None,
    }
}

/// 替身家声明的显示码那一形：印链接与码、不读 stdin，人在任一台浏览器里填码它自己退 0；退出 · 状态各声明一条。
pub const SHOW_CODE_FACE: crate::agents::SignInFace = crate::agents::SignInFace {
    program: SIGN_IN_PROGRAM,
    args: &["login", "--show-code"],
    dir_env: ACCOUNT_DIR_ENV,
    env_remove: &["CCM_FAKE_TOKEN"],
    link_in: fake_link_in,
    terminal: false,
    paste: crate::agents::PasteShape::ShowCode(fake_code_in),
    opens_browser: false,
    sign_out: Some(&["logout"]),
    status: Some(crate::agents::SignInStatus {
        args: &["status"],
        read: fake_status_read,
    }),
};

/// 替身家声明的不粘回那一形：自己开它那一台的浏览器、等回跳到它那一台的本机口，不读 stdin；退出 · 状态不声明。
pub const NO_PASTE_FACE: crate::agents::SignInFace = crate::agents::SignInFace {
    args: &["login"],
    paste: crate::agents::PasteShape::NoPaste,
    opens_browser: true,
    sign_out: None,
    status: None,
    ..SHOW_CODE_FACE
};
