//! A2：多账号（cc-acct-iso「隔离又同步」管线）的**只读**消费侧。
//!
//! - `--list-accounts [--accts-dir <p>]`
//!   → 第 1 行 `{"kind":"accounts-meta",…}`，其后每账号一行 JSON。
//! - `--session-accounts [--accts-dir <p>]`
//!   → 每条运行中会话一行：它的 `CLAUDE_CONFIG_DIR` 属于哪个账号。
//! - `--account-trust <configDir> <cwd> [--accts-dir <p>]`
//!   → 单行 `{"trusted":bool,"known":bool}`：目标账号是否已信任该目录
//!   （换号 resume 前的预检——首次用某账号进某目录，CC 会弹信任确认，会卡住编排）。
//! - `--account-trust-zero <cwd>`
//!   → 同上，但问的是**账号 0**（见下）。它没有 configDir，`.claude.json` 在 `$HOME`。
//!
//! # Z01：账号 0
//! manifest 里 `configDir` **键缺席**的那一条 = 账号 0 =「不设 `CLAUDE_CONFIG_DIR`」
//! 这个状态本身。本模块**结构性**地认它（看键在不在），**不认名字**——不在 Rust 里
//! 硬编码 "0"。它的 config dir 是共享库（`sharedStore`）、`.claude.json` 在 `$HOME`。
//! **空串不算缺席**：`is_safe_config_dir("")` 会挡掉它（空值 ≠ 未设）。
//!
//! 输出协议同 `history_query`：每行一个 JSON 对象（**不是** wire::Frame）。
//! 成功 exit 0；`--account-trust` 的硬错误 exit 2 + stderr 纯 `{code,message}` JSON
//! （照 `resolve_query` 的结构化错误约定，客户端可整段 parse）。
//!
//! # 只读铁律（doc/INVARIANTS.md §1）
//! 本模块只 `read` / `read_dir` / `metadata`，**零写入**，且**不 shell out**
//! （daemon 是非登录 shell、PATH 很瘦；直接读 manifest 文件即可，省掉 PATH 依赖
//! 与"让只读组件去跑写工具"的争议面）。
//!
//! # 凭据边界（本模块最重要的约束）
//! - `.credentials.json` **只 stat 存在性，绝不读内容**。
//! - `.claude.json` 只取 `projects[<cwd>].hasTrustDialogAccepted` 一个布尔；
//!   **绝不回传文件内容**——那里面有 `mcpServers` 的环境变量（可能含 API key）。
//! - `/proc/<pid>/environ` 只抠 `CLAUDE_CONFIG_DIR` 一个键，**不回传整个环境快照**。
//! - `--account-trust` 的 `configDir` 必须逐字等于 manifest 里某个账号的 `configDir`，
//!   否则拒绝——避免它退化成"任意文件读"原语。`--account-trust-zero` 不收路径参数
//!   （路径是 `$HOME/.claude.json`，写死在代码里），所以它连这个面都没有。

use std::io::Read;
use std::path::{Path, PathBuf};

/// `.claude.json` 读取上限（照 `mcp.rs` 的 32MB 约定）。真机上它约 115KB。
const MAX_CLAUDE_JSON_BYTES: u64 = 32 * 1024 * 1024;
/// manifest 读取上限。账号数有限，几 MB 足矣，此处宽松给 8MB 兜底。
const MAX_MANIFEST_BYTES: u64 = 8 * 1024 * 1024;
/// 单个 `sessions/<PID>.json` 读取上限（正常几百字节）。
const MAX_SESSION_FILE_BYTES: u64 = 1024 * 1024;
/// `sessions/*.json` 扫描上限，防病态目录拖垮一次性查询。
const MAX_SESSION_FILES: usize = 500;

/// **安全读取**：先确认是常规文件（挡掉 FIFO / 字符设备 / socket——它们的
/// `metadata().len()` 报 0 会骗过大小检查，而 `read_to_string` 无上限 → 远端 OOM，
/// 审计实测 symlink→/dev/zero 6 秒涨 11GB），再 `take(cap)` 限量读，
/// 一步消掉 metadata↔read 之间的 TOCTOU。symlink 会被 `metadata()`（跟随）解析到
/// 目标类型：目标是常规文件才放行、是设备就拒。
pub(crate) fn read_regular_capped(path: &Path, cap: u64) -> Result<Vec<u8>, String> {
    let meta = std::fs::metadata(path).map_err(|e| format!("{e}"))?;
    if !meta.is_file() {
        return Err("不是常规文件（可能是 FIFO/设备/目录）".into());
    }
    let f = std::fs::File::open(path).map_err(|e| format!("{e}"))?;
    let mut buf = Vec::new();
    // take(cap+1)：读到 cap+1 就知道超限了，不必读满整个（可能无界的）文件
    f.take(cap + 1)
        .read_to_end(&mut buf)
        .map_err(|e| format!("{e}"))?;
    if buf.len() as u64 > cap {
        return Err(format!("超过 {cap} 字节上限"));
    }
    Ok(buf)
}

// ---------------------------------------------------------------- manifest

#[derive(serde::Deserialize)]
struct RawAccount {
    name: String,
    #[serde(default)]
    email: Option<String>,
    /// **Z01：可以缺席。** 缺席 = 账号 0 =「不设 `CLAUDE_CONFIG_DIR`」这个状态本身，
    /// 它的 config dir 就是共享库。**判据是结构性的（这个键在不在），不认名字**——
    /// 不在这里硬编码 "0"，manifest 想叫它什么都行。
    /// 空串**不算缺席**：`is_safe_config_dir("")` 会把它挡掉（空值 ≠ 未设）。
    #[serde(rename = "configDir", default)]
    config_dir: Option<String>,
    #[serde(rename = "isDefault", default)]
    is_default: bool,
    #[serde(default)]
    mode: Option<String>,
}

struct Manifest {
    updated_at: Option<String>,
    shared_store: Option<String>,
    accounts: Vec<RawAccount>,
}

/// 会被用来做视觉欺骗的 Unicode 码点（双向覆盖 / 零宽 / 异常空白 / 行段分隔）。
/// 这些不是 `is_control`（Rust 的 `char::is_control` 只覆盖 C0/C1），但在 UI 里能
/// 伪造同形/反向的账号名与路径，是真实的钓鱼面。两端（daemon + cc-acct-iso
/// `path_shell_safe`）都拒它们，避免"写侧放行、读侧丢弃 → 账号凭空消失"的不一致。
fn is_deceptive_char(c: char) -> bool {
    matches!(c,
        '\u{0085}'                          // NEL（C1 换行，不在 char::is_control 里）
        | '\u{00A0}'                        // NBSP
        | '\u{200B}'..='\u{200F}'           // 零宽空格/连接符 + LRM/RLM
        | '\u{2028}' | '\u{2029}'           // 行分隔 / 段分隔
        | '\u{202A}'..='\u{202E}'           // 双向嵌入/覆盖
        | '\u{2066}'..='\u{2069}'           // 双向隔离
        | '\u{FEFF}'                        // ZWNBSP / BOM
    )
}

/// 路径是否可安全地交给下游（cc-monitor 会把 configDir 拼进 `export CLAUDE_CONFIG_DIR='…'`）。
/// 与 cc-acct-iso 的 `path_shell_safe` 同一套字符集——两端对齐，避免一端放行另一端炸。
/// 允许普通空格与常规非 ASCII（如中文；单引号内无害且常见），拒绝引号/命令替换/
/// 重定向/通配/控制字符 + 视觉欺骗类 Unicode。
fn is_safe_config_dir(p: &str) -> bool {
    if !p.starts_with('/') {
        return false;
    }
    if p == "/" || p.contains("/../") || p.ends_with("/..") {
        return false;
    }
    !p.chars().any(|c| {
        c.is_control()
            || is_deceptive_char(c)
            || matches!(
                c,
                '\'' | '"'
                    | '\\'
                    | '`'
                    | '$'
                    | ';'
                    | '|'
                    | '&'
                    | '<'
                    | '>'
                    | '*'
                    | '?'
                    | '('
                    | ')'
                    | '!'
            )
    })
}

/// 去掉尾部 `/`，让 manifest 里的路径与 `/proc` 环境变量里的写法能对上。
fn norm_dir(p: &str) -> &str {
    let t = p.trim_end_matches('/');
    if t.is_empty() {
        "/"
    } else {
        t
    }
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

/// 把 `$HOME/x` / `~/x` 前缀展开成绝对路径。仅支持前缀形式——更花哨的 shell 写法
/// 一律不猜（daemon 不跑 shell），让用户走 `--accts-dir` 显式覆盖。
fn expand_home_prefix(raw: &str, home: Option<&Path>) -> String {
    let home = match home {
        Some(h) => h.to_string_lossy().into_owned(),
        None => return raw.to_string(),
    };
    for pat in ["$HOME/", "${HOME}/", "~/"] {
        if let Some(rest) = raw.strip_prefix(pat) {
            return format!("{}/{}", home.trim_end_matches('/'), rest);
        }
    }
    match raw {
        "$HOME" | "${HOME}" | "~" => home,
        _ => raw.to_string(),
    }
}

/// 从 `~/.cc-acct-iso/config` 的文本里抠 `ACCTS_DIR=` 的值。
/// **正则式纯文本解析，绝不 source**（那是 shell 文件，daemon 不跑 shell）。
/// 取最后一次有效赋值（后写覆盖先写，与 shell 语义一致）；跳过注释行。
fn parse_accts_dir_from_config(text: &str) -> Option<String> {
    let mut found = None;
    for line in text.lines() {
        let mut l = line.trim_start();
        if l.starts_with('#') {
            continue;
        }
        // cc-acct-iso 那个 config 是被真正 `. source` 的（lib.sh），所以 `export ACCTS_DIR=…`
        // / `declare -x ACCTS_DIR=…` 都是合法写法，且 export 是极常见习惯。逐个剥掉可选前缀，
        // 否则纯文本解析会漏认 → 回落默认路径 → 账号功能在该主机"静默判失效"。
        for pfx in [
            "export ",
            "declare -x ",
            "declare ",
            "typeset -x ",
            "typeset ",
        ] {
            if let Some(rest) = l.strip_prefix(pfx) {
                l = rest.trim_start();
                break;
            }
        }
        let Some(rest) = l.strip_prefix("ACCTS_DIR") else {
            continue;
        };
        // `=` 必须紧跟变量名（shell 赋值语义：`ACCTS_DIR =/x` 是命令不是赋值；
        // `ACCTS_DIRX=…` 是别的变量）。不 trim `=` 前的空白，正好把这两种都排除。
        let Some(val) = rest.strip_prefix('=') else {
            continue;
        };
        let val = val.trim();
        // 去掉行尾注释（仅未被引号包裹时）
        let val = if val.starts_with('"') {
            val.strip_prefix('"').and_then(|v| v.split('"').next())
        } else if val.starts_with('\'') {
            val.strip_prefix('\'').and_then(|v| v.split('\'').next())
        } else {
            Some(val.split('#').next().unwrap_or("").trim())
        };
        if let Some(v) = val {
            if !v.is_empty() {
                found = Some(v.to_string());
            }
        }
    }
    found
}

/// 账号库目录：`--accts-dir <p>` > `~/.cc-acct-iso/config` 的 `ACCTS_DIR` > `$HOME/.claude-accts`。
fn resolve_accts_dir(args: &[String]) -> PathBuf {
    if let Some(i) = args.iter().position(|a| a == "--accts-dir") {
        if let Some(p) = args.get(i + 1) {
            if !p.is_empty() {
                return PathBuf::from(p);
            }
        }
    }
    let home = home_dir();
    if let Some(h) = home.as_deref() {
        let cfg = h.join(".cc-acct-iso").join("config");
        if let Ok(text) = std::fs::read_to_string(&cfg) {
            if let Some(raw) = parse_accts_dir_from_config(&text) {
                let expanded = expand_home_prefix(&raw, Some(h));
                if expanded.starts_with('/') {
                    return PathBuf::from(expanded);
                }
            }
        }
        return h.join(".claude-accts");
    }
    PathBuf::from(".claude-accts")
}

fn manifest_path(accts_dir: &Path) -> PathBuf {
    accts_dir.join("accounts.json")
}

/// 读 + 解析 manifest。缺文件/坏 JSON/不支持的 schema 都是 `Err(人话原因)`——
/// 调用方据此输出 `enabled:false` 而**不是**失败退出（"没启用多账号"是正常状态）。
///
/// 账号数组**逐条**解析：单个坏账号（缺 name/configDir 等）被跳过而非拖垮整份
/// manifest，与 cc-acct-iso 写侧「丢单条」策略一致（避免手改 manifest 时一坏全灭）。
fn load_manifest(accts_dir: &Path) -> Result<Manifest, String> {
    let p = manifest_path(accts_dir);
    let bytes = read_regular_capped(&p, MAX_MANIFEST_BYTES)
        .map_err(|e| format!("manifest 不可读（{}）：{e}", p.display()))?;
    let root: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|e| format!("manifest 不是合法 JSON：{e}"))?;
    match root.get("version").and_then(|v| v.as_u64()) {
        Some(1) => {}
        Some(v) => {
            return Err(format!(
                "manifest schema 版本 {v} 不受支持（本 daemon 只认 1）"
            ))
        }
        None => return Err("manifest 缺 version 字段（或不是数字）".into()),
    }
    let mut accounts = Vec::new();
    if let Some(arr) = root.get("accounts").and_then(|a| a.as_array()) {
        for (i, item) in arr.iter().enumerate() {
            match serde_json::from_value::<RawAccount>(item.clone()) {
                Ok(a) => accounts.push(a),
                Err(e) => tracing::warn!("manifest 第 {i} 个账号解析失败，已跳过：{e}"),
            }
        }
    }
    Ok(Manifest {
        updated_at: root
            .get("updatedAt")
            .and_then(|v| v.as_str())
            .map(String::from),
        shared_store: root
            .get("sharedStore")
            .and_then(|v| v.as_str())
            .map(String::from),
        accounts,
    })
}

fn json_str(v: Option<&str>) -> serde_json::Value {
    match v {
        Some(s) => serde_json::Value::String(s.to_string()),
        None => serde_json::Value::Null,
    }
}

// ---------------------------------------------------------------- /proc

/// `/proc/<pid>/stat` 的 starttime（field 22，boot 起的 jiffies）。形状照 watcher。
/// 这是 PID 的**身份指纹**：PID 会被复用，但 (PID, starttime) 对唯一。
fn proc_starttime(pid: u32) -> Option<u64> {
    #[cfg(target_os = "linux")]
    {
        let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
        // starttime = comm 右括号之后的第 (22-3) 个 whitespace token
        let after_comm = &stat[stat.rfind(')')? + 1..];
        after_comm
            .split_whitespace()
            .nth(22 - 3)?
            .parse::<u64>()
            .ok()
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = pid;
        None
    }
}

/// 从 pidfile 字节里取 `procStart`（CC 写的是 starttime ticks 的十进制字符串；容忍裸数字）。
fn parse_procstart_ticks(v: &serde_json::Value) -> Option<u64> {
    let field = v.get("procStart")?;
    if let Some(s) = field.as_str() {
        return s.trim().parse::<u64>().ok();
    }
    field.as_u64()
}

/// 该 pidfile 记录的进程**当前是否仍是同一个进程**（防 PID 复用 → 误归属账号）。
/// 严于 watcher 的判活：这里的结果直接喂给"按会话切账号"，**错标签比缺标签危害大**，
/// 所以要求 pidfile 的 `procStart` 与当前 `/proc/<pid>` 的 starttime **精确相等**才认。
/// 缺 `procStart`（老 pidfile）或读不到 starttime（进程已死）→ 不认（宁缺毋错）。
fn session_process_identity_ok(pid: u32, pidfile: &serde_json::Value) -> bool {
    match (parse_procstart_ticks(pidfile), proc_starttime(pid)) {
        (Some(recorded), Some(current)) => recorded == current,
        _ => false,
    }
}

/// 从 `/proc/<pid>/environ` 抠 `CLAUDE_CONFIG_DIR` 的值（**只这一个键**）。
/// 读不到（进程已消失 / 非同 uid）→ `None`。形状照 `watcher::proc_cmdline`。
fn proc_claude_config_dir(pid: u32) -> Option<String> {
    #[cfg(target_os = "linux")]
    {
        let bytes = std::fs::read(format!("/proc/{pid}/environ")).ok()?;
        for entry in bytes.split(|b| *b == 0) {
            if entry.is_empty() {
                continue;
            }
            let s = String::from_utf8_lossy(entry);
            if let Some(v) = s.strip_prefix("CLAUDE_CONFIG_DIR=") {
                if v.is_empty() {
                    return None;
                }
                return Some(v.to_string());
            }
        }
        None
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = pid;
        None
    }
}

// ---------------------------------------------------------------- 命令

/// `--list-accounts`：meta 行 + 每账号一行。永远 exit 0（"未启用"是正常状态，不是错误）。
fn list_accounts(accts_dir: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let mpath = manifest_path(accts_dir);
    match load_manifest(accts_dir) {
        Err(e) => {
            out.push(
                serde_json::json!({
                    "kind": "accounts-meta",
                    "enabled": false,
                    "acctsDir": accts_dir.to_string_lossy(),
                    "manifestPath": mpath.to_string_lossy(),
                    "updatedAt": serde_json::Value::Null,
                    "sharedStore": serde_json::Value::Null,
                    "count": 0,
                    "error": e,
                })
                .to_string(),
            );
        }
        Ok(m) => {
            let mut lines = Vec::new();
            for a in &m.accounts {
                // Z01：configDir 缺席 = 账号 0。它的 config dir 就是共享库 ⇒ 登录态查那儿，
                // 而 `configDir` 在帧里出 **null**（下游据此「不注入 CLAUDE_CONFIG_DIR」）。
                let (cfg_out, probe_dir) = match a.config_dir.as_deref() {
                    None => (
                        serde_json::Value::Null,
                        m.shared_store.as_deref().map(PathBuf::from),
                    ),
                    Some(c) => {
                        if !is_safe_config_dir(c) {
                            tracing::warn!("账号 {} 的 configDir 不安全，已丢弃：{}", a.name, c);
                            continue;
                        }
                        let n = norm_dir(c);
                        (
                            serde_json::Value::String(n.to_string()),
                            Some(PathBuf::from(n)),
                        )
                    }
                };
                lines.push(
                    serde_json::json!({
                        "name": a.name,
                        "email": a.email.clone().unwrap_or_default(),
                        "configDir": cfg_out,
                        "isDefault": a.is_default,
                        "mode": a.mode.clone().unwrap_or_else(|| "isolated".into()),
                        // 账号 0 恒 exists（「裸起」这个状态永远可达）；有 configDir 的看目录在不在。
                        "exists": match &probe_dir {
                            Some(d) if a.config_dir.is_some() => d.is_dir(),
                            _ => a.config_dir.is_none(),
                        },
                        // 只 stat 存在性，绝不读内容。
                        // **Z06 双写点**：这个文件名是「什么算已登录」的判据，而 cc-acct-iso
                        // 的 `NATIVE_IDENTITY` 声明里也各写了一份（bash 侧 `cc-acct-iso` 的
                        // `logged=` 那行）。两个进程、两种语言，无法共享常量 ⇒ 由本文件测试
                        // 模块里的 `credential_filename_matches_native_identity_declaration`
                        // 钉住（同 `TMUX_LS_FMT` 双写点那条守卫的做法）。**改这里必须改声明。**
                        // 探不到 config dir（账号 0 且 manifest 没写 sharedStore）⇒ false，
                        // 那是「不知道」，不假装已登录。
                        "loggedIn": probe_dir
                            .as_ref()
                            .is_some_and(|d| d.join(".credentials.json").exists()),
                    })
                    .to_string(),
                );
            }
            out.push(
                serde_json::json!({
                    "kind": "accounts-meta",
                    "enabled": true,
                    "acctsDir": accts_dir.to_string_lossy(),
                    "manifestPath": mpath.to_string_lossy(),
                    "updatedAt": json_str(m.updated_at.as_deref()),
                    "sharedStore": json_str(m.shared_store.as_deref()),
                    "count": lines.len(),
                    "error": serde_json::Value::Null,
                    // Z01 能力标记：本 daemon 认识「configDir 缺席 = 账号 0」。
                    // **旧 daemon 不会出这个键**（它把账号 0 当坏数据跳过了）⇒ monitor 侧
                    // default=false ⇒ 能**明说**「远端 daemon 太旧，列表里少了账号 0」，
                    // 而不是让用户看着一个静默少一行的列表。
                    "accountZeroAware": true,
                })
                .to_string(),
            );
            out.extend(lines);
        }
    }
    out
}

/// `--session-accounts`：扫 `<claude_dir>/sessions/<PID>.json`，每条一行。
fn session_accounts(claude_dir: &Path, accts_dir: &Path) -> Vec<String> {
    // Z01：`None` 这个 key 是账号 0（configDir 缺席）。裸起会话过去归属不到任何账号
    // （`account: null` + `bare: true`），现在它有名字了。
    let by_dir: Vec<(Option<String>, String)> = load_manifest(accts_dir)
        .map(|m| {
            m.accounts
                .into_iter()
                .filter_map(|a| match a.config_dir.as_deref() {
                    None => Some((None, a.name)),
                    Some(c) if is_safe_config_dir(c) => {
                        Some((Some(norm_dir(c).to_string()), a.name))
                    }
                    Some(_) => None,
                })
                .collect()
        })
        .unwrap_or_default();

    let mut out = Vec::new();
    let dir = claude_dir.join("sessions");
    let Ok(rd) = std::fs::read_dir(&dir) else {
        return out; // 没有 sessions/ → 零行（exit 0）
    };
    let mut seen = 0usize;
    for ent in rd.flatten() {
        if seen >= MAX_SESSION_FILES {
            tracing::warn!("sessions/ 条目超过 {MAX_SESSION_FILES}，其余跳过");
            break;
        }
        let path = ent.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let Some(pid) = path
            .file_stem()
            .and_then(|s| s.to_str())
            .and_then(|s| s.parse::<u32>().ok())
        else {
            continue;
        };
        seen += 1;
        let bytes = match read_regular_capped(&path, MAX_SESSION_FILE_BYTES) {
            Ok(b) => b,
            Err(_) => continue,
        };
        let v: serde_json::Value = match serde_json::from_slice(&bytes) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let sid = v.get("sessionId").and_then(|x| x.as_str());
        let cwd = v.get("cwd").and_then(|x| x.as_str());
        // 判活必须过 procStart 身份对拍：PID 会被复用，陈旧 pidfile 的 PID 可能已被
        // 别的进程占用。只按 /proc/<pid> 存在性判活会把已死会话误贴成"活着 + 别人的账号"
        // （审计 R1，已在沙盒复现）。身份不符 → 当作该会话已死，不读 environ、不归属。
        let alive = session_process_identity_ok(pid, &v);
        let cfg = if alive {
            proc_claude_config_dir(pid)
        } else {
            None
        };
        let cfg_norm = cfg.as_deref().map(|c| norm_dir(c).to_string());
        // 归属：有 configDir 就逐字匹配；没有且**进程确实活着**就是账号 0。
        // 进程已死时不归属（cfg 恒 None，归给账号 0 会把死会话贴成账号 0 的）。
        let account = if alive {
            by_dir
                .iter()
                .find(|(d, _)| d.as_deref() == cfg_norm.as_deref())
                .map(|(_, n)| n.clone())
        } else {
            None
        };
        out.push(
            serde_json::json!({
                "pid": pid,
                "sessionId": json_str(sid),
                "cwd": json_str(cwd),
                "configDir": json_str(cfg_norm.as_deref()),
                "account": json_str(account.as_deref()),
                // 进程活着（身份已确认）但没设 CLAUDE_CONFIG_DIR。**Z01 起这不再是异常**：
                // 它就是账号 0（上面的 `account` 会给出名字）。字段保留是因为下游要用它
                // 区分「账号 0」与「设了 configDir 的账号」——语义从「告警」变成「事实」。
                "bare": alive && cfg_norm.is_none(),
                "alive": alive,
            })
            .to_string(),
        );
    }
    out
}

/// `--account-trust <configDir> <cwd>`：目标账号是否已信任该目录。
/// `configDir` 必须 ∈ manifest（否则这就成了任意文件读原语）。
fn account_trust(
    accts_dir: &Path,
    config_dir: &str,
    cwd: &str,
) -> Result<String, (String, String)> {
    if !is_safe_config_dir(config_dir) {
        return Err((
            "unsafe_config_dir".into(),
            "configDir 含不安全字符或不是绝对路径".into(),
        ));
    }
    let m = load_manifest(accts_dir).map_err(|e| ("manifest_unavailable".to_string(), e))?;
    let want = norm_dir(config_dir);
    if !m.accounts.iter().any(|a| {
        a.config_dir
            .as_deref()
            .is_some_and(|c| is_safe_config_dir(c) && norm_dir(c) == want)
    }) {
        return Err((
            "unknown_config_dir".into(),
            "该 configDir 不在 manifest 的账号列表里，拒绝读取".into(),
        ));
    }
    trust_of_claude_json(&Path::new(want).join(".claude.json"), cwd)
}

/// `--account-trust-zero <cwd>`：**账号 0** 的信任预检。
///
/// 为什么要单独一个入口而不是给 `--account-trust` 传个空 `configDir`：账号 0 **没有**
/// config dir，空串是被明令禁止的拼法（空值 ≠ 未设，见 `RawAccount::config_dir`）。
/// 而且它的 `.claude.json` 也不在共享库里 —— 原生根是 `$HOME`（cc-acct-iso 的
/// `NATIVE_IDENTITY` 里 `.claude.json:home:secret`）⇒ 路径来源本就不同，
/// 用同一个入口只能靠哨兵值区分，那比多一个动词更容易出错。
fn account_trust_zero(cwd: &str) -> Result<String, (String, String)> {
    let home = home_dir().ok_or_else(|| {
        (
            "no_home".to_string(),
            "拿不到 $HOME，无法定位账号 0 的 .claude.json".to_string(),
        )
    })?;
    trust_of_claude_json(&home.join(".claude.json"), cwd)
}

/// 读某个 `.claude.json`，回答「这个 cwd 被信任过吗」。两个 trust 入口共用。
fn trust_of_claude_json(p: &Path, cwd: &str) -> Result<String, (String, String)> {
    if !p.exists() {
        // 该账号还没有 .claude.json（全新账号）→ 肯定没信任过，不是错误
        return Ok(
            serde_json::json!({"trusted": false, "known": false, "error": serde_json::Value::Null})
                .to_string(),
        );
    }
    // 安全读：is_file 挡掉 FIFO/设备（其 metadata().len() 报 0 会骗过大小检查、
    // read 无上限 → 远端 OOM，审计实测 symlink→/dev/zero 6 秒涨 11GB）+ take 限量。
    let bytes = read_regular_capped(p, MAX_CLAUDE_JSON_BYTES)
        .map_err(|e| ("claude_json_unreadable".to_string(), e))?;
    let v: serde_json::Value = serde_json::from_slice(&bytes)
        .map_err(|e| ("claude_json_invalid".to_string(), e.to_string()))?;
    let entry = v.get("projects").and_then(|p| p.get(cwd));
    let known = entry.is_some();
    let trusted = entry
        .and_then(|e| e.get("hasTrustDialogAccepted"))
        .and_then(|b| b.as_bool())
        .unwrap_or(false);
    // 只出这三个字段——.claude.json 里有 mcpServers 的环境变量（可能含 API key）
    Ok(
        serde_json::json!({"trusted": trusted, "known": known, "error": serde_json::Value::Null})
            .to_string(),
    )
}

/// 查询模式入口。返回进程退出码（0 ok / 2 err），同 `history_query::run` 约定。
pub fn run(claude_dir: &Path, args: &[String]) -> i32 {
    let accts_dir = resolve_accts_dir(args);
    match args.first().map(String::as_str) {
        Some("--list-accounts") => {
            for l in list_accounts(&accts_dir) {
                println!("{l}");
            }
            0
        }
        Some("--session-accounts") => {
            for l in session_accounts(claude_dir, &accts_dir) {
                println!("{l}");
            }
            0
        }
        Some("--account-trust") => match (args.get(1), args.get(2)) {
            (Some(cfg), Some(cwd)) => match account_trust(&accts_dir, cfg, cwd) {
                Ok(line) => {
                    println!("{line}");
                    0
                }
                Err((code, message)) => {
                    // 结构化错误：stderr 纯 JSON，客户端可整段 parse（同 --resolve 约定）
                    eprintln!("{}", serde_json::json!({"code": code, "message": message}));
                    2
                }
            },
            _ => {
                eprintln!(
                    "{}",
                    serde_json::json!({
                        "code": "bad_args",
                        "message": "--account-trust requires <configDir> <cwd>"
                    })
                );
                2
            }
        },
        Some("--account-trust-zero") => match args.get(1) {
            Some(cwd) => match account_trust_zero(cwd) {
                Ok(line) => {
                    println!("{line}");
                    0
                }
                Err((code, message)) => {
                    eprintln!("{}", serde_json::json!({"code": code, "message": message}));
                    2
                }
            },
            None => {
                eprintln!(
                    "{}",
                    serde_json::json!({
                        "code": "bad_args",
                        "message": "--account-trust-zero requires <cwd>"
                    })
                );
                2
            }
        },
        other => {
            eprintln!("cc-monitor-remote accounts error: unknown argument: {other:?}");
            2
        }
    }
}

// ---------------------------------------------------------------- tests

#[cfg(test)]
mod tests {
    /// ★ Z06 跨语言双写点守卫：本文件判「已登录」用的那个文件名，必须与
    /// cc-acct-iso 的 `NATIVE_IDENTITY` 声明里**标为 `secret` 的凭据项**一致。
    ///
    /// **为什么需要它**：「有 `.credentials.json` = 已登录」这条判据被**独立实现了两遍**
    /// ——bash 侧 `cc-acct-iso`（`logged=false; [ -f … ] && logged=true`）与本文件。
    /// 两个进程、两种语言，共享不了常量。Claude Code 哪天改了凭据文件名，改一边漏一边的
    /// 表现是**静默错**：`loggedIn` 恒 false，UI 上看不出来。
    ///
    /// 做法照 `src-tauri/src/tmux.rs::tmux_ls_fmt_double_write_point_stays_in_sync`：
    /// `include_str!` 读**vendored** 副本（它与上游逐字节一致，由 `build.rs` 的过期检查兜）
    /// + 锚定声明里那一行。**双向**：改任一侧忘同步即红。
    #[test]
    fn credential_filename_matches_native_identity_declaration() {
        // 本文件用来判「已登录」的文件名。改这里就要改下面的断言，也要改 bash 侧声明。
        const CREDENTIAL_FILE: &str = ".credentials.json";
        let lib_sh = include_str!("../../src-tauri/vendor/cc-acct-iso/scripts/lib.sh");

        // 声明里那一行的精确形状：`<项名>:<原生根>:<类别>`，凭据项必须是 secret。
        let expected_line = format!("{CREDENTIAL_FILE}:cfg:secret");
        assert!(
            lib_sh.contains(&expected_line),
            "Z06 双写点漂移：cc-acct-iso 的 NATIVE_IDENTITY 声明里找不到 {expected_line:?}。\n\
             daemon 判「已登录」用的是 {CREDENTIAL_FILE:?}，两边必须一致——\
             Claude Code 改了凭据文件名就要**同时**改声明与本文件。"
        );
        // 本文件真的在用这个名字（防止有人改了上面的常量却没改 json 里那行字面量）。
        let me = include_str!("accounts_query.rs");
        assert!(
            me.contains(&format!(".join({CREDENTIAL_FILE:?}).exists()")),
            "本文件的 loggedIn 判据没在用 {CREDENTIAL_FILE:?} —— 常量与实际用法脱节了"
        );
        // 反向自检：断言的是「两个源都真读进来了」，不是「命中若干条」。
        assert!(
            lib_sh.len() > 1000 && me.len() > 1000,
            "include_str! 没读到源码，上面的断言是空转"
        );
    }

    use super::*;
    use std::fs;

    fn tmpdir(tag: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!(
            "ccm-acct-{tag}-{}-{:?}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        p
    }

    fn write_manifest(accts: &Path, body: &str) {
        fs::create_dir_all(accts).unwrap();
        fs::write(accts.join("accounts.json"), body).unwrap();
    }

    fn meta(lines: &[String]) -> serde_json::Value {
        serde_json::from_str(&lines[0]).unwrap()
    }

    // ---- 1. 正常 manifest ----
    #[test]
    fn list_accounts_happy_path() {
        let root = tmpdir("happy");
        let accts = root.join("accts");
        let z = accts.join("z");
        fs::create_dir_all(&z).unwrap();
        fs::write(z.join(".credentials.json"), "{\"tok\":\"SECRET-TOKEN\"}").unwrap();
        write_manifest(
            &accts,
            &format!(
                r#"{{"version":1,"updatedAt":"2026-07-23T00:00:00Z","sharedStore":"/s",
                    "acctsDir":"{a}","accounts":[
                    {{"name":"z","email":"z@x.edu","configDir":"{z}","isDefault":true,"mode":"isolated"}},
                    {{"name":"b","email":"","configDir":"{a}/b","isDefault":false,"mode":"isolated"}}]}}"#,
                a = accts.display(),
                z = z.display()
            ),
        );
        let lines = list_accounts(&accts);
        let m = meta(&lines);
        assert_eq!(m["enabled"], true);
        assert_eq!(m["count"], 2);
        assert_eq!(m["updatedAt"], "2026-07-23T00:00:00Z");
        assert_eq!(lines.len(), 3);
        let a0: serde_json::Value = serde_json::from_str(&lines[1]).unwrap();
        assert_eq!(a0["name"], "z");
        assert_eq!(a0["isDefault"], true);
        assert_eq!(a0["exists"], true);
        assert_eq!(a0["loggedIn"], true, "有 .credentials.json 应判已登录");
        let a1: serde_json::Value = serde_json::from_str(&lines[2]).unwrap();
        assert_eq!(a1["name"], "b");
        assert_eq!(a1["exists"], false, "目录不存在");
        assert_eq!(a1["loggedIn"], false);
        // 凭据零泄漏
        for l in &lines {
            assert!(!l.contains("SECRET-TOKEN"), "输出里出现了凭据内容：{l}");
        }
        let _ = fs::remove_dir_all(&root);
    }

    // ---- 2. manifest 缺失 / 畸形 / 版本不支持 → enabled:false 且不失败 ----
    #[test]
    fn list_accounts_degrades_gracefully() {
        let root = tmpdir("degrade");
        // 缺文件
        let m = meta(&list_accounts(&root.join("nope")));
        assert_eq!(m["enabled"], false);
        assert!(m["error"].as_str().unwrap().contains("不可读"));
        // 坏 JSON
        let a = root.join("bad");
        write_manifest(&a, "{not json");
        let m = meta(&list_accounts(&a));
        assert_eq!(m["enabled"], false);
        assert!(m["error"].as_str().unwrap().contains("合法 JSON"));
        // 版本不支持
        let a2 = root.join("v2");
        write_manifest(&a2, r#"{"version":2,"accounts":[]}"#);
        let m = meta(&list_accounts(&a2));
        assert_eq!(m["enabled"], false);
        assert!(m["error"].as_str().unwrap().contains("版本 2"));
        // 缺 version
        let a3 = root.join("nover");
        write_manifest(&a3, r#"{"accounts":[]}"#);
        assert_eq!(meta(&list_accounts(&a3))["enabled"], false);
        let _ = fs::remove_dir_all(&root);
    }

    // ---- 3. 非法 configDir 被丢弃，其余正常 ----
    #[test]
    fn unsafe_config_dirs_are_dropped() {
        let root = tmpdir("unsafe");
        let accts = root.join("accts");
        write_manifest(
            &accts,
            r#"{"version":1,"accounts":[
                {"name":"ok","configDir":"/home/u/.claude-accts/ok"},
                {"name":"quote","configDir":"/home/u/ac'ts/x"},
                {"name":"dollar","configDir":"/home/u/$(id)/x"},
                {"name":"dotdot","configDir":"/home/u/../etc"},
                {"name":"rel","configDir":"relative/path"},
                {"name":"root","configDir":"/"}]}"#,
        );
        let lines = list_accounts(&accts);
        assert_eq!(meta(&lines)["count"], 1, "只应留下合法的那一个");
        let a: serde_json::Value = serde_json::from_str(&lines[1]).unwrap();
        assert_eq!(a["name"], "ok");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn safe_config_dir_predicate() {
        assert!(is_safe_config_dir("/home/u/.claude-accts/z"));
        assert!(
            is_safe_config_dir("/home/用户/带 空格/z"),
            "空格与非 ASCII 允许"
        );
        assert!(!is_safe_config_dir("relative"));
        assert!(!is_safe_config_dir("/"));
        assert!(!is_safe_config_dir("/a/../b"));
        assert!(!is_safe_config_dir("/a/b/.."));
        for bad in [
            "/a'b", "/a\"b", "/a`b", "/a$b", "/a;b", "/a|b", "/a&b", "/a<b", "/a>b", "/a*b",
            "/a?b", "/a(b", "/a)b", "/a!b", "/a\\b",
        ] {
            assert!(!is_safe_config_dir(bad), "{bad} 应被拒");
        }
        assert!(!is_safe_config_dir("/a\nb"));
    }

    // ---- 4. --account-trust ----
    #[test]
    fn account_trust_paths() {
        let root = tmpdir("trust");
        let accts = root.join("accts");
        let z = accts.join("z");
        fs::create_dir_all(&z).unwrap();
        write_manifest(
            &accts,
            &format!(
                r#"{{"version":1,"accounts":[{{"name":"z","configDir":"{}"}}]}}"#,
                z.display()
            ),
        );
        // 没有 .claude.json → trusted:false / known:false，不是错误
        let out = account_trust(&accts, &z.to_string_lossy(), "/w").unwrap();
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["trusted"], false);
        assert_eq!(v["known"], false);

        fs::write(
            z.join(".claude.json"),
            r#"{"projects":{"/w":{"hasTrustDialogAccepted":true},"/x":{}},
                "mcpServers":{"gh":{"env":{"GITHUB_TOKEN":"ghp_SUPERSECRET"}}},
                "oauthAccount":{"emailAddress":"z@x.edu"}}"#,
        )
        .unwrap();
        let out = account_trust(&accts, &z.to_string_lossy(), "/w").unwrap();
        assert!(!out.contains("ghp_SUPERSECRET"), "绝不能回传文件内容");
        assert!(!out.contains("z@x.edu"));
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["trusted"], true);
        assert_eq!(v["known"], true);
        // 已记录但未接受 → known:true, trusted:false
        let v: serde_json::Value =
            serde_json::from_str(&account_trust(&accts, &z.to_string_lossy(), "/x").unwrap())
                .unwrap();
        assert_eq!(v["known"], true);
        assert_eq!(v["trusted"], false);
        // manifest 之外的 configDir → 拒（防任意文件读）
        let e = account_trust(&accts, "/etc", "/w").unwrap_err();
        assert_eq!(e.0, "unknown_config_dir");
        // 不安全的 configDir → 拒
        let e = account_trust(&accts, "/a'b", "/w").unwrap_err();
        assert_eq!(e.0, "unsafe_config_dir");
        let _ = fs::remove_dir_all(&root);
    }

    // ---- 5. ACCTS_DIR 配置解析 ----
    #[test]
    fn parse_accts_dir_variants() {
        assert_eq!(
            parse_accts_dir_from_config("ACCTS_DIR=\"/a/b\"\n").as_deref(),
            Some("/a/b")
        );
        assert_eq!(
            parse_accts_dir_from_config("ACCTS_DIR='/a/b'\n").as_deref(),
            Some("/a/b")
        );
        assert_eq!(
            parse_accts_dir_from_config("  ACCTS_DIR=/a/b  # 注释\n").as_deref(),
            Some("/a/b")
        );
        assert_eq!(parse_accts_dir_from_config("#ACCTS_DIR=/x\n"), None);
        assert_eq!(parse_accts_dir_from_config("SHARED_STORE=/x\n"), None);
        // 后写覆盖先写
        assert_eq!(
            parse_accts_dir_from_config("ACCTS_DIR=/a\nACCTS_DIR=\"/b\"\n").as_deref(),
            Some("/b")
        );
        // $HOME 展开
        let h = PathBuf::from("/home/u");
        assert_eq!(
            expand_home_prefix("$HOME/.claude-accts", Some(&h)),
            "/home/u/.claude-accts"
        );
        assert_eq!(expand_home_prefix("${HOME}/x", Some(&h)), "/home/u/x");
        assert_eq!(expand_home_prefix("~/x", Some(&h)), "/home/u/x");
        assert_eq!(expand_home_prefix("/abs/x", Some(&h)), "/abs/x");
    }

    #[test]
    fn accts_dir_cli_override_wins() {
        let args = vec![
            "--list-accounts".to_string(),
            "--accts-dir".to_string(),
            "/custom/accts".to_string(),
        ];
        assert_eq!(resolve_accts_dir(&args), PathBuf::from("/custom/accts"));
    }

    // ---- 6. --session-accounts（procStart 身份对拍是核心）----
    #[test]
    fn session_accounts_marks_dead_and_bare() {
        let root = tmpdir("sess");
        let claude = root.join("claude");
        let sessions = claude.join("sessions");
        fs::create_dir_all(&sessions).unwrap();
        // 一个几乎不可能存在的 pid → alive:false
        fs::write(
            sessions.join("4194300.json"),
            r#"{"sessionId":"sid-dead","cwd":"/w","procStart":"999"}"#,
        )
        .unwrap();
        let lines = session_accounts(&claude, &root.join("no-accts"));
        let mut by_sid = sid_map(&lines);
        assert_eq!(by_sid["sid-dead"]["alive"], false);
        assert_eq!(by_sid["sid-dead"]["bare"], false, "死进程不算裸起");
        assert_eq!(by_sid["sid-dead"]["account"], serde_json::Value::Null);

        // 当前进程 pid + **正确的 procStart** → 身份对拍通过 → alive:true
        #[cfg(target_os = "linux")]
        {
            let me = std::process::id();
            let real_ticks = proc_starttime(me).expect("能读自己的 starttime");
            fs::write(
                sessions.join(format!("{me}.json")),
                format!(r#"{{"sessionId":"sid-live","cwd":"/w2","procStart":"{real_ticks}"}}"#),
            )
            .unwrap();
            let lines = session_accounts(&claude, &root.join("no-accts"));
            by_sid = sid_map(&lines);
            assert_eq!(by_sid["sid-live"]["alive"], true, "procStart 相符应判活");
            if std::env::var_os("CLAUDE_CONFIG_DIR").is_none() {
                assert_eq!(by_sid["sid-live"]["bare"], true);
                assert_eq!(by_sid["sid-live"]["configDir"], serde_json::Value::Null);
            }
        }
        let _ = fs::remove_dir_all(&root);
    }

    /// R1：PID 复用防御——pidfile 的 procStart 与当前进程不符 → 判死，绝不误归属账号。
    #[cfg(target_os = "linux")]
    #[test]
    fn session_accounts_rejects_pid_reuse() {
        let root = tmpdir("reuse");
        let claude = root.join("claude");
        let sessions = claude.join("sessions");
        fs::create_dir_all(&sessions).unwrap();
        let me = std::process::id();
        let real = proc_starttime(me).unwrap();
        // 同一个活 PID，但 pidfile 记的是**别的** procStart（= 该 PID 曾属于一个已退出的
        // claude，现在被本测试进程复用）→ 必须判死、不归属
        fs::write(
            sessions.join(format!("{me}.json")),
            format!(
                r#"{{"sessionId":"sid-stale","cwd":"/w","procStart":"{}"}}"#,
                real + 12345
            ),
        )
        .unwrap();
        let by = sid_map(&session_accounts(&claude, &root.join("no-accts")));
        assert_eq!(
            by["sid-stale"]["alive"], false,
            "procStart 不符 = PID 被复用 → 判死"
        );
        assert_eq!(by["sid-stale"]["bare"], false);
        assert_eq!(by["sid-stale"]["account"], serde_json::Value::Null);

        // 缺 procStart 的老 pidfile 也保守判死（宁缺毋错）
        fs::write(
            sessions.join(format!("{me}.json")),
            r#"{"sessionId":"sid-noproc","cwd":"/w"}"#,
        )
        .unwrap();
        let by = sid_map(&session_accounts(&claude, &root.join("no-accts")));
        assert_eq!(by["sid-noproc"]["alive"], false, "缺 procStart 保守判死");
        let _ = fs::remove_dir_all(&root);
    }

    fn sid_map(lines: &[String]) -> std::collections::HashMap<String, serde_json::Value> {
        let mut m = std::collections::HashMap::new();
        for l in lines {
            let v: serde_json::Value = serde_json::from_str(l).unwrap();
            m.insert(v["sessionId"].as_str().unwrap().to_string(), v);
        }
        m
    }

    #[test]
    fn session_accounts_without_sessions_dir_is_empty() {
        let root = tmpdir("nosess");
        assert!(session_accounts(&root, &root).is_empty());
        let _ = fs::remove_dir_all(&root);
    }

    // ---- 7. 尾斜杠归一 ----
    #[test]
    fn trailing_slash_normalized() {
        assert_eq!(norm_dir("/a/b/"), "/a/b");
        assert_eq!(norm_dir("/a/b"), "/a/b");
        assert_eq!(norm_dir("/"), "/");
    }

    // ---- 8. R2：export/declare 前缀 ----
    #[test]
    fn parse_accts_dir_export_prefix() {
        assert_eq!(
            parse_accts_dir_from_config("export ACCTS_DIR=/a/b\n").as_deref(),
            Some("/a/b")
        );
        assert_eq!(
            parse_accts_dir_from_config("declare -x ACCTS_DIR=\"/a/b\"\n").as_deref(),
            Some("/a/b")
        );
        assert_eq!(
            parse_accts_dir_from_config("  export ACCTS_DIR='/a/b'\n").as_deref(),
            Some("/a/b")
        );
        // 撞名前缀不误认
        assert_eq!(parse_accts_dir_from_config("ACCTS_DIRX=/y\n"), None);
        assert_eq!(parse_accts_dir_from_config("export ACCTS_DIRX=/y\n"), None);
        // `=` 前有空格 = shell 里的命令而非赋值 → 不认
        assert_eq!(parse_accts_dir_from_config("ACCTS_DIR =/x\n"), None);
        // CRLF 行尾
        assert_eq!(
            parse_accts_dir_from_config("export ACCTS_DIR=/a/b\r\n").as_deref(),
            Some("/a/b")
        );
    }

    // ---- 9. 重要-B：特殊文件不绕过大小上限 ----
    #[cfg(unix)]
    #[test]
    fn special_files_are_rejected_not_read() {
        use std::os::unix::fs::symlink;
        let root = tmpdir("special");
        // symlink → /dev/zero：metadata().len() 报 0 会骗过大小检查,read 无上限会 OOM。
        // read_regular_capped 必须靠 is_file() 挡下（跟随 symlink 后目标是字符设备）。
        let link = root.join("evil.json");
        symlink("/dev/zero", &link).unwrap();
        let r = read_regular_capped(&link, MAX_CLAUDE_JSON_BYTES);
        assert!(
            r.is_err(),
            "指向 /dev/zero 的 symlink 必须被拒，而不是读爆内存"
        );
        // 目录也不是常规文件
        assert!(read_regular_capped(&root, 1024).is_err());
        // 正常小文件放行
        let ok = root.join("ok.json");
        fs::write(&ok, "{}").unwrap();
        assert_eq!(read_regular_capped(&ok, 1024).unwrap(), b"{}");
        // 超上限的常规文件被拒
        fs::write(&ok, vec![b'x'; 100]).unwrap();
        assert!(read_regular_capped(&ok, 50).is_err());
        let _ = fs::remove_dir_all(&root);
    }

    // ---- 10. 建议1：单个坏账号被跳过而非拖垮整份 manifest ----
    #[test]
    fn one_bad_account_does_not_kill_the_list() {
        let root = tmpdir("badacct");
        let accts = root.join("accts");
        write_manifest(
            &accts,
            // Z01 起「缺 configDir」不再是坏数据（那是账号 0），所以坏样本换成
            // 缺 name / configDir 不安全这两种真·坏法。
            r#"{"version":1,"accounts":[
                {"name":"good","configDir":"/h/.claude-accts/good"},
                {"configDir":"/h/.claude-accts/noname"},
                {"name":"unsafe","configDir":"relative/path"},
                {"name":"good2","configDir":"/h/.claude-accts/good2"}]}"#,
        );
        let lines = list_accounts(&accts);
        assert_eq!(meta(&lines)["enabled"], true);
        assert_eq!(
            meta(&lines)["count"],
            2,
            "缺 name / 路径不安全的被跳过,好的两个留下"
        );
        let names: Vec<String> = lines[1..]
            .iter()
            .map(|l| {
                serde_json::from_str::<serde_json::Value>(l).unwrap()["name"]
                    .as_str()
                    .unwrap()
                    .to_string()
            })
            .collect();
        assert_eq!(names, vec!["good", "good2"]);
        let _ = fs::remove_dir_all(&root);
    }

    // ---- 11. 建议：Unicode 欺骗字符两端对齐拒绝 ----
    #[test]
    fn deceptive_unicode_rejected() {
        assert!(!is_safe_config_dir("/home/u/\u{202E}gpj.z")); // RLO 反向覆盖
        assert!(!is_safe_config_dir("/home/u/z\u{200B}b")); // 零宽空格
        assert!(!is_safe_config_dir("/home/u/z\u{00A0}b")); // NBSP
        assert!(!is_safe_config_dir("/home/u/z\u{0085}b")); // NEL
        assert!(!is_safe_config_dir("/home/u/z\u{FEFF}b")); // ZWNBSP/BOM
        assert!(!is_safe_config_dir("/home/u/z\u{2069}b")); // 双向隔离
                                                            // 正常中文与普通空格仍放行
        assert!(is_safe_config_dir("/home/用户/带 空格/z"));
    }

    // ---- 12. Z01：账号 0（configDir 键缺席）----

    /// 缺 `configDir` = 账号 0。它的 config dir 就是共享库 ⇒ 登录态查那儿；
    /// 帧里 `configDir` 出 **null**（下游据此「不注入 CLAUDE_CONFIG_DIR」）。
    #[test]
    fn account_zero_is_kept_and_probes_shared_store() {
        let root = tmpdir("acct0");
        let shared = root.join("claude");
        fs::create_dir_all(&shared).unwrap();
        let accts = root.join("accts");
        write_manifest(
            &accts,
            &format!(
                r#"{{"version":1,"sharedStore":{shared:?},"accounts":[
                    {{"name":"z","configDir":{z:?}}},
                    {{"name":"0","isDefault":false,"mode":"bare"}}]}}"#,
                shared = shared.to_string_lossy(),
                z = root.join("accts/z").to_string_lossy()
            ),
        );
        let lines = list_accounts(&accts);
        assert_eq!(meta(&lines)["count"], 2, "账号 0 不得被静默丢掉");
        let zero: serde_json::Value = serde_json::from_str(&lines[2]).unwrap();
        assert_eq!(zero["name"], "0");
        assert_eq!(
            zero["configDir"],
            serde_json::Value::Null,
            "必须是 null，**绝不能是空串**"
        );
        assert_eq!(zero["mode"], "bare");
        assert_eq!(zero["exists"], true, "「裸起」这个状态永远可达");
        assert_eq!(zero["loggedIn"], false, "共享库里还没凭据");

        fs::write(shared.join(".credentials.json"), "{}").unwrap();
        let lines = list_accounts(&accts);
        let zero: serde_json::Value = serde_json::from_str(&lines[2]).unwrap();
        assert_eq!(zero["loggedIn"], true, "共享库凭据 = 账号 0 已登录");
        let _ = fs::remove_dir_all(&root);
    }

    /// ★ 空串 **不是** 缺席。这是整个 Z01 的支点：`CLAUDE_CONFIG_DIR=""` 会被
    /// Claude Code 当成一个空路径，与「未设」完全不同。它必须被当坏数据丢掉，
    /// **不能**退化成账号 0。
    #[test]
    fn empty_config_dir_is_not_account_zero() {
        let root = tmpdir("acct0empty");
        let accts = root.join("accts");
        write_manifest(
            &accts,
            r#"{"version":1,"sharedStore":"/h/.claude","accounts":[
                {"name":"empty","configDir":""}]}"#,
        );
        let lines = list_accounts(&accts);
        assert_eq!(
            meta(&lines)["count"],
            0,
            "空串 configDir 必须被丢掉，不得当成账号 0"
        );
        let _ = fs::remove_dir_all(&root);
    }

    /// manifest 没写 sharedStore 时，账号 0 的登录态是「不知道」⇒ false，
    /// **不得假装已登录**，也不得因此把账号 0 丢掉。
    #[test]
    fn account_zero_without_shared_store_is_not_logged_in() {
        let root = tmpdir("acct0nostore");
        let accts = root.join("accts");
        write_manifest(
            &accts,
            r#"{"version":1,"accounts":[{"name":"0","mode":"bare"}]}"#,
        );
        let lines = list_accounts(&accts);
        assert_eq!(meta(&lines)["count"], 1);
        let zero: serde_json::Value = serde_json::from_str(&lines[1]).unwrap();
        assert_eq!(zero["loggedIn"], false);
        assert_eq!(zero["configDir"], serde_json::Value::Null);
        let _ = fs::remove_dir_all(&root);
    }

    /// 裸起会话（活着但没设 CLAUDE_CONFIG_DIR）现在归属账号 0。
    #[cfg(target_os = "linux")]
    #[test]
    fn bare_session_is_attributed_to_account_zero() {
        if std::env::var_os("CLAUDE_CONFIG_DIR").is_some() {
            return; // 跑在已设了该变量的 shell 里 ⇒ 本用例不适用
        }
        let root = tmpdir("acct0sess");
        let claude = root.join("claude");
        let sessions = claude.join("sessions");
        fs::create_dir_all(&sessions).unwrap();
        let accts = root.join("accts");
        write_manifest(
            &accts,
            r#"{"version":1,"accounts":[{"name":"0","mode":"bare"}]}"#,
        );
        let me = std::process::id();
        let ticks = proc_starttime(me).expect("能读自己的 starttime");
        fs::write(
            sessions.join(format!("{me}.json")),
            format!(r#"{{"sessionId":"sid-zero","cwd":"/w","procStart":"{ticks}"}}"#),
        )
        .unwrap();
        let by = sid_map(&session_accounts(&claude, &accts));
        assert_eq!(by["sid-zero"]["alive"], true);
        assert_eq!(by["sid-zero"]["bare"], true);
        assert_eq!(by["sid-zero"]["account"], "0", "裸起不再是「归属不明」");
        let _ = fs::remove_dir_all(&root);
    }

    /// 反向：manifest 里 **没有** 账号 0 时，裸起会话仍旧行为（account: null）。
    /// 钉住「归属来自 manifest」，而不是在 Rust 里硬编码了个 "0"。
    #[cfg(target_os = "linux")]
    #[test]
    fn bare_session_without_account_zero_stays_unattributed() {
        if std::env::var_os("CLAUDE_CONFIG_DIR").is_some() {
            return;
        }
        let root = tmpdir("acct0none");
        let claude = root.join("claude");
        let sessions = claude.join("sessions");
        fs::create_dir_all(&sessions).unwrap();
        let accts = root.join("accts");
        write_manifest(
            &accts,
            r#"{"version":1,"accounts":[{"name":"z","configDir":"/h/.claude-accts/z"}]}"#,
        );
        let me = std::process::id();
        let ticks = proc_starttime(me).unwrap();
        fs::write(
            sessions.join(format!("{me}.json")),
            format!(r#"{{"sessionId":"sid-none","cwd":"/w","procStart":"{ticks}"}}"#),
        )
        .unwrap();
        let by = sid_map(&session_accounts(&claude, &accts));
        assert_eq!(by["sid-none"]["account"], serde_json::Value::Null);
        let _ = fs::remove_dir_all(&root);
    }

    /// 账号 0 **不得**把 `--account-trust` 变成「读共享库 .claude.json」的口子：
    /// 它没有 configDir ⇒ 任何路径都不在 manifest 里 ⇒ 拒。
    #[test]
    fn account_trust_does_not_accept_shared_store_via_account_zero() {
        let root = tmpdir("acct0trust");
        let shared = root.join("claude");
        fs::create_dir_all(&shared).unwrap();
        fs::write(shared.join(".claude.json"), r#"{"projects":{"/w":{}}}"#).unwrap();
        let accts = root.join("accts");
        write_manifest(
            &accts,
            &format!(
                r#"{{"version":1,"sharedStore":{s:?},"accounts":[{{"name":"0","mode":"bare"}}]}}"#,
                s = shared.to_string_lossy()
            ),
        );
        let e = account_trust(&accts, &shared.to_string_lossy(), "/w").unwrap_err();
        assert_eq!(e.0, "unknown_config_dir");
        let _ = fs::remove_dir_all(&root);
    }

    /// `trust_of_claude_json` 是两个 trust 入口共用的那份实现（避免第二份）。
    /// 账号 0 走 `$HOME/.claude.json`——声明里 `.claude.json` 的原生根就是 home。
    #[test]
    fn trust_of_claude_json_reads_only_the_three_booleans() {
        let root = tmpdir("acct0tz");
        fs::create_dir_all(&root).unwrap();
        let cj = root.join(".claude.json");

        // 文件不存在 ⇒ known:false，不是错误
        let v: serde_json::Value =
            serde_json::from_str(&trust_of_claude_json(&cj, "/w").unwrap()).unwrap();
        assert_eq!(v["known"], false);
        assert_eq!(v["trusted"], false);

        fs::write(
            &cj,
            r#"{"projects":{"/w":{"hasTrustDialogAccepted":true}},
                "mcpServers":{"x":{"env":{"API_KEY":"sk-SECRET"}}}}"#,
        )
        .unwrap();
        let out = trust_of_claude_json(&cj, "/w").unwrap();
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["trusted"], true);
        assert_eq!(v["known"], true);
        assert!(
            !out.contains("sk-SECRET"),
            "绝不能把 .claude.json 的内容回传"
        );
        let _ = fs::remove_dir_all(&root);
    }

    /// ★ **main 必须分发本模块认的每一个子命令** —— v3.4.0 的事故守卫。
    ///
    /// 当时 `--account-trust-zero` 在本模块实现完整（`run` 里有它的臂），但 `main.rs` 的
    /// match 只列了三个字面量 ⇒ 它落进 `_ => history_query::run` ⇒ 回 `unknown argument`
    /// + exit 2。而 monitor 的账号 0 信任预检**真的在发这条命令**，随 v3.4.0 发了出去。
    ///
    /// **为什么既有测试一条都没红**：它们全都直接调 `accounts_query::run`，
    /// **绕过了 main 的调度**——被测的那一半是好的，坏的是没人测的那一半。
    /// ⇒ 这条守卫**跨文件**比对：本模块 `run` 里出现的每个 `Some("--x")`，
    /// 在 `main.rs` 的生产段里都必须出现。
    #[test]
    fn main_dispatches_every_subcommand_we_handle() {
        let me = include_str!("accounts_query.rs");
        let main_raw = include_str!("main.rs");
        // 只看生产段 + 剥行注释：两个文件的散文里都会提到这些字面量，
        // 不剥的话「main 的注释里写了它」也会让守卫变绿——那正是安慰剂。
        let strip = |src: &str| -> String {
            let marker = "\n#[cfg(test)]\nmod tests";
            let prod = match src.find(marker) {
                Some(i) => &src[..i],
                None => src,
            };
            prod.lines()
                .filter(|l| !l.trim_start().starts_with("//"))
                .collect::<Vec<_>>()
                .join("\n")
        };
        let mine = strip(me);
        let main_prod = strip(main_raw);
        assert!(
            main_prod.len() > 3_000 && main_prod.len() < main_raw.len(),
            "剥完 main 生产段只剩 {} 字节（原文 {}）——剥法坏了",
            main_prod.len(),
            main_raw.len()
        );

        // 抠出本模块 `run` 分发的子命令：形如 `Some("--x") =>`。
        let mut subs: Vec<&str> = Vec::new();
        let needle = format!("{}(\"--", "Some");
        for (i, _) in mine.match_indices(needle.as_str()) {
            let rest = &mine[i + needle.len()..];
            if let Some(end) = rest.find('"') {
                let name = &rest[..end];
                if !subs.contains(&name) {
                    subs.push(name);
                }
            }
        }
        // 反向自检：一个都没抠到 = 抠法坏了，而不是「本模块没有子命令」。
        assert_eq!(
            subs.len(),
            4,
            "从本模块抠到 {} 个子命令（真实应为 4）：{subs:?}——加/删子命令时来改这个数",
            subs.len()
        );

        for name in &subs {
            let lit = format!("{}(\"--{name}\")", "Some");
            assert!(
                main_prod.contains(lit.as_str()),
                "`--{name}` 在本模块有完整实现，但 `main.rs` 的调度里找不到 `{lit}`。\n\
                 它会落进 `_` 臂走历史查询 ⇒ 回 `unknown argument` + exit 2，\n\
                 而调用方（monitor）拿到的是一个看起来像「daemon 太旧」的失败。\n\
                 **v3.4.0 就是这么漏出去的。** 加子命令时两处都要加。"
            );
        }
    }

    /// `--account-trust-zero` 只收 cwd，路径写死在代码里 ⇒ 它连「任意文件读」的面都没有。
    /// 钉住入口形状（而不是去改 $HOME 跑真的，那在并行测试里是竞态）。
    #[test]
    fn account_trust_zero_takes_no_path_argument() {
        let me = include_str!("accounts_query.rs");
        assert!(
            me.contains("fn account_trust_zero(cwd: &str)"),
            "账号 0 的 trust 入口一旦收了路径参数，就重新开出了任意文件读的面"
        );
        assert!(
            me.contains(r#"home.join(".claude.json")"#),
            "账号 0 的 .claude.json 必须来自 $HOME（声明里它的原生根是 home）"
        );
        assert!(me.len() > 1000, "include_str! 没读到源码，上面的断言是空转");
    }
}
