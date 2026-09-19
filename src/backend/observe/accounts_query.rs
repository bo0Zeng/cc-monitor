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
//! # ⚠ 谓词的作用面**不对称**，这里如实写下〔audit-0805 08-06 抽样核实〕
//!
//! `is_safe_config_dir` 今天有三处调用点，全部在**清单侧**（`--list-accounts` /
//! `--account-trust` 那几条路：源是 accounts manifest）。而 `--session-accounts`
//! 那条路的 `configDir` 来自 **`/proc/<pid>/environ`**（`proc_claude_config_dir`），
//! **没有过谓词**就作为帧字段发给 monitor。
//!
//! ## 为什么**不**顺手给它套上谓词
//!
//! 逐条量过后果，套上去**更糟**：
//! - **归属那一半已经是安全的** —— 进程侧的值只拿去与 `by_dir` 比对，
//!   而 `by_dir` 只装清单侧、已过谓词的目录 ⇒ 不安全的值匹配不上，`account` 恒 `None`；
//! - 而若把不安全值直接丢弃（置 `None`），那条会话就会变成 `bare: true` ——
//!   **语义上等同于「账号 0」**，于是一个可疑会话反而被贴成默认账号。
//!   那不是收紧，是把一种坏结果换成另一种更坏的。
//! - 真要处理，得给帧加一个「configDir 不可信」的状态位 ——
//!   那是**改上线契约**（D6：暴露给第三方 = 契约冻结成本），不属本区范围（只修缺陷，不加能力）。
//!
//! ⇒ 结论：**归属安全、展示未净化**。登记在 `ROADMAP §5`，
//! 解锁条件 = 帧契约允许新增状态位时，把「不可信的 configDir」表达成一个显式状态，
//! 而不是让它退化成 `bare`。

//!
//! 输出协议同 `history_query`：每行一个 JSON 对象（**不是** wire::Frame）。
//! 成功 exit 0；`--account-trust` 的硬错误 exit 2 + stderr 纯 `{code,message}` JSON
//! （照 `resolve_query` 的结构化错误约定，客户端可整段 parse）。
//!
//! # 只读铁律（src/doc/INVARIANTS.md §1）
//! 本模块只 `read` / `read_dir` / `metadata`，**零写入**，且**不 shell out**
//! （backend 是非登录 shell、PATH 很瘦；直接读 manifest 文件即可，省掉 PATH 依赖
//! 与"让只读组件去跑写工具"的争议面）。
//!
//! # 凭据边界（本模块最重要的约束）
//! - `.credentials.json` **只 stat 存在性，绝不读内容**。
//! - `.claude.json` 只取 `projects[<cwd>].hasTrustDialogAccepted` 一个布尔；
//!   **绝不回传文件内容**——那里面有 `mcpServers` 的环境变量（可能含 API key）。
//! - `/proc/<pid>/environ` 只抠**两个写死的键**（`CLAUDE_CONFIG_DIR` 与
//!   `CCM_LAUNCH_ID`），**不回传整个环境快照**。
//!   ⚠ `K-P5f` 加第二个键那一拍要求把「两个键」与「整个快照」的界说清楚，界在这里：
//!   **键名是本文件里的两个常量**（`paths::CONFIG_DIR_ENV` 与 [`LAUNCH_ID_ENV`]），
//!   **不接受任何调用方传进来的键名**。一旦键名成为一维参数，这条查询就退化成
//!   「任意环境变量读」原语 —— 与 `--account-trust` 那条「`configDir` 必须 ∈ manifest，
//!   否则就是任意文件读」是同一形的退化。⇒ 判据 `the_only_env_keys_this_module_reads_are_the_two_named_constants`
//!   数着本文件生产段里 `proc_env_var(` 的调用点，**多一处 ⇒ 红**。
//! - `--account-trust` 的 `configDir` 必须逐字等于 manifest 里某个账号的 `configDir`，
//!   否则拒绝——避免它退化成"任意文件读"原语。`--account-trust-zero` 不收路径参数
//!   （路径是 `$HOME/.claude.json`，写死在代码里），所以它连这个面都没有。

use acct_core::{
    auth_kind_from_manifest, auth_ready, is_deceptive_char, ACCTS_DIR_NAME, CREDENTIALS_NAME,
    MANIFEST_NAME, SUPPORTED_SCHEMA,
};
use std::path::{Path, PathBuf};

/// manifest 读取上限。账号数有限，几 MB 足矣，此处宽松给 8MB 兜底。
const MAX_MANIFEST_BYTES: u64 = 8 * 1024 * 1024;
/// 单个 `sessions/<PID>.json` 读取上限（正常几百字节）。
const MAX_SESSION_FILE_BYTES: u64 = 1024 * 1024;
/// `sessions/*.json` 扫描上限，防病态目录拖垮一次性查询。
const MAX_SESSION_FILES: usize = 500;

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
    /// **K-A1：鉴权方式。可以缺席** —— 缺席 = 旧 manifest = 订阅号
    /// （裁决与排除见件计划 `KA6d`；分类规则的唯一住址是
    /// `acct_core::auth_kind_from_manifest`，**这里不许再写一份 match**）。
    #[serde(rename = "authKind", default)]
    auth_kind: Option<String>,
}

struct Manifest {
    updated_at: Option<String>,
    shared_store: Option<String>,
    accounts: Vec<RawAccount>,
}

/// 路径是否可安全地交给下游（cc-monitor 会把 configDir 拼进 `export CLAUDE_CONFIG_DIR='…'`）。
/// 与 cc-acct-iso 的 `path_shell_safe` **在安全字符那一半上**同一套——两端对齐，避免一端放行另一端炸。
/// ⚠ `N-F1c`（09-05）之后**不再是整套同一**：`\` 从本函数的拒绝集里拿出来了（Windows 的路径
/// 分隔符就是它），而上面那一侧照旧拒。这不是漂移，是**分层校验** ——
/// monitor 把 `configDir` 拼进 POSIX 命令之前要过 `config_dir_command_safe`，那个函数明确拒 `\`。
/// 〔旧文逐字，留作来历：「同一套字符集——两端对齐」——「整套同一」今天不成立。〕
/// ⚠⚠ 上面那个名字**在本文件里只许出现一次** —— `structural_scan::INVENTORY` 按**处数**钉着它
/// （PM 09-05 改这段注释时多写了一次，当场红：「盘上 2 处，登记表写 1 处」）。
/// 允许普通空格与常规非 ASCII（如中文；单引号内无害且常见），拒绝引号/命令替换/
/// 重定向/通配/控制字符 + 视觉欺骗类 Unicode。
fn is_safe_config_dir(p: &str) -> bool {
    // 🔴 `N-F1c`：这个判据被**拆成两半**了。拆法逐字照 monitor 那份同名实现的模块头注
    // （`src/bridge/src/local_accounts.rs` 顶部那一节，逐字：「判据落在性质上，不落在表面
    // 特征上 —— 照抄 `starts_with('/')` 是抄了形式、丢了性质」）：
    //   ① shell 元字符与视觉欺骗字符 = **平台无关的安全性质**，两侧逐字同一套；
    //   ② 「是绝对路径」= **平台相关的形式**，各写各的。
    //
    // 为什么现在才拆：本函数此前只服务远端（backend 只跑在 Linux 上），而
    // `N-F1c` 起 **monitor 的本机账号清单也来问这个二进制**（`--list-accounts`），
    // 而那份 monitor 要在 Windows 上跑 —— Windows 的账号目录是 `C:\Users\…`，
    // 旧的第一条会把每一个 Windows 账号判成不安全 ⇒ **清单恒空**。
    // ⚠ 障碍是这条检查，**不是**「backend 不能在 Windows 上跑」：发版流水线的
    //   `build-windows` 里有原生本机后端构建，产物装进 `externalBin`。
    fn looks_absolute(p: &str) -> bool {
        if p.starts_with('/') {
            return true; // POSIX
        }
        // Windows：盘符（`C:\` / `C:/`）或 UNC（`\\server\share`）。
        let b = p.as_bytes();
        let drive = b.len() >= 3
            && b[0].is_ascii_alphabetic()
            && b[1] == b':'
            && (b[2] == b'\\' || b[2] == b'/');
        drive || p.starts_with("\\\\")
    }
    if !looks_absolute(p) {
        return false;
    }
    if p == "/" || p.contains("/../") || p.ends_with("/..") {
        return false;
    }
    // 反斜杠成了合法分隔符 ⇒ **上跳那一手也要按反斜杠再拒一次**，否则放宽绝对路径的同时
    // 就把 `C:\Users\..\..\x` 一起放进来了（monitor 那份加这一条正是为此）。
    if p.contains("\\..\\") || p.ends_with("\\..") {
        return false;
    }
    // 平台无关的那一半：shell 元字符 + 视觉欺骗字符，**与 monitor 那份逐字同一套**。
    // **反斜杠不在此列** —— Windows 的路径分隔符就是它。放行它在这里是安全的，
    // 因为**下游那一层自己会拒**：monitor 把 configDir 拼进 POSIX 命令之前要过
    // `config_dir_command_safe`，那个函数明确把 `\` 列进拒绝集。
    // ⇒ 这是**分层校验**，不是「反正没人拿它拼命令」。
    !p.chars().any(|c| {
        c.is_control()
            || is_deceptive_char(c)
            || matches!(
                c,
                '\'' | '"' | '`' | '$' | ';' | '|' | '&' | '<' | '>' | '*' | '?' | '(' | ')' | '!'
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
/// 一律不猜（backend 不跑 shell），让用户走 `--accts-dir` 显式覆盖。
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
/// **正则式纯文本解析，绝不 source**（那是 shell 文件，backend 不跑 shell）。
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
        return h.join(ACCTS_DIR_NAME);
    }
    PathBuf::from(ACCTS_DIR_NAME)
}

fn manifest_path(accts_dir: &Path) -> PathBuf {
    accts_dir.join(MANIFEST_NAME)
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
        Some(SUPPORTED_SCHEMA) => {}
        Some(v) => return Err(format!("manifest schema 版本 {v} 不受支持（本后端只认 1）")),
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

// U2：**这里原本有第二份 `proc_starttime`**（`/proc/<pid>/stat` 的 field 22）。
// 与 `platform::proc::proc_starttime` 逐字同语义（都返回 boot 起的 jiffies，解析都是
// `nth(22-3)`），只是这一份把解析内联了、那一份走 `parse_starttime_from_stat`。
// 合并前**逐条核过单位**：单位不同的话它们就不是重复，合并就是引 bug。
use crate::agents::claudecode::accounts as cc_accounts;
use crate::common::fs::read_regular_capped;
use crate::platform::proc::{proc_env_var, proc_starttime, EnvRead};

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

// ------------------------------------------------- K-P5f：身份 token 读回来那一侧

/// cc-monitor 起会话时铸进下一跳进程环境的**身份 token**〔`K-P5b` 写侧，`K-P5f` 读侧〕。
///
/// # 🔴 双写点，且**共享不了常量** —— 界在这里说清楚
///
/// monitor 侧的家是 `src/bridge/src/history.rs::LAUNCH_ID_VAR`，而
/// `src/backend` 是**另一个 crate、另一份 `Cargo.lock`**（`src/bridge/Cargo.toml`
/// 的 workspace members 里逐字没有它）⇒ 两侧不可能 `use` 同一个 `const`。
/// 与 `CREDENTIALS_NAME` 那个双写点（Rust ↔ bash）同形，处置也照它：
/// **由测试对拍**（[`tests::the_launch_id_env_var_matches_the_monitor_side_home`]，
/// `include_str!` 直接读 monitor 那份源码）。**改这里必须改那边，反之亦然。**
///
/// # ⚠ 它**不住** `agents/claudecode/paths.rs`，这不是疏忽
///
/// 那一层装的是「**Claude** 的目录布局与环境变量」（`CONFIG_DIR_ENV` 住那儿是对的：
/// 那是 Claude 认的变量）。而本变量是 **cc-monitor 自己**铸的 token，Claude 一个字都不认
/// ⇒ 把它塞进 `agents/claudecode/` 会让那一层多出一件不属于它的知识。
/// 今天读它的只有本文件这一处，家就设在这里。
const LAUNCH_ID_ENV: &str = "CCM_LAUNCH_ID";

/// 身份 token 的字符集 —— **fail closed**，形状不对就不往下游递。
///
/// 与铸法那一侧同一条：`payload::relay_segment_is_safe` 逐字是
/// 「只许字母数字与 `-` `_`，1..=128 字节」，而 `route_key_for_session` 铸出来的
/// 要么是 UUID v4（`[0-9a-f-]`，36 字节）、要么是过了那条白名单的 sid ⇒ 两种都在集内。
///
/// # 为什么读回来还要再核一次（"来源可信"不是放行的理由）
///
/// 这个值来自 `/proc/<pid>/environ`，而**谁都能 `export CCM_LAUNCH_ID=…` 再起 claude** ——
/// 它是本模块唯一一个**任意用户可控**的出参。同 `identity_tag::sid_is_safe` 那条头注
/// 逐字记的纪律：「本仓栽过的那些坑里，最贵的一类就是『这个值不可能有问题』」。
fn launch_id_is_safe(v: &str) -> bool {
    !v.is_empty()
        && v.len() <= 128
        && v.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// 一条会话在出参里的全部字段（`--session-accounts` 每行一个）。
///
/// `K-P5f` 之前这里没有中间结构、边算边 `json!` —— 现在要有，理由是**防冒名那一格
/// 只能在看完整批之后才判得出来**（见 [`suppress_inherited_launch_ids`]）。
struct SessionRow {
    pid: u32,
    sid: Option<String>,
    cwd: Option<String>,
    config_dir: Option<String>,
    account: Option<String>,
    alive: bool,
    /// 🔴 `K-R21`：读 `CLAUDE_CONFIG_DIR` 的**那一次**，`/proc/<pid>/environ`
    /// **这一刻取不到**（读失败 / 读回 0 字节）。
    ///
    /// 它与 `config_dir: None` 是**两件事**：`None` 说的是「读到了、这个键没设」，
    /// 本格说的是「这一刻我读不出来」。⇒ 本格为真时**不归属账号、也不算裸起**。
    ///
    /// ⚠ **不进出参**：出参形状一个字节都没动（`configDir:null` + `account:null`
    /// + `bare:false` 今天就表达得了「不知道」）。要把「为什么不知道」也发出去，
    /// 那是给出参加状态位、是改上线契约 —— 同 [`suppress_inherited_launch_ids`]
    /// 头注里那条已被前人裁死的口径。
    cfg_env_unreadable: bool,
    /// 从 `/proc/<pid>/environ` 抠到、且过了 [`launch_id_is_safe`] 的原值。
    /// 还没过防冒名那一格 —— **别直接往出参里填这一格**。
    launch_id: Option<String>,
}

/// 🔴🔴 **防冒名：不唯一的身份 token 一律不作数**〔`KP5FD5`〕。
///
/// # 它防的是什么（与本文件里另一道身份检查**不是同一件事**）
///
/// 同一个文件里已经有一道 [`session_process_identity_ok`]，它防的是 **PID 复用**：
/// pidfile 记的 `procStart` 必须与 `/proc/<pid>` 当前的 starttime 精确相等，
/// 否则这个 pid 已经是别人的了。⇒ 它买到的是「**这个 pid 就是写那份 pidfile 的那个进程**」。
///
/// **它一个字都不管环境变量是从哪继承来的。** 而 `CCM_LAUNCH_ID` 是**继承型**变量：
/// `export CCM_LAUNCH_ID=…; claude …` 之后，claude 再 spawn 的**子进程原样继承它**
/// （SDK 起的、claude 自己起的 claude）。那些子进程会写**自己的** `sessions/<PID>.json`
/// ⇒ 它们过得了 `procStart` 对拍（pid 与 pidfile 确实是同一个进程），
/// **但按 pid 读回来的 token 是父会话的**。
/// ⚠ 这两件事很容易被当成一件 —— 看见那一行 `session_process_identity_ok` 就以为
/// 「防冒名」已经打过勾了。**那正是本工作区最贵的那族病。**
///
/// # 为什么不照抄盘上那两条已上线的防法（`KP5FD5` 要求说清选的是哪条、为什么）
///
/// | 盘上的防法 | 它防的 | 能不能用在这里 |
/// |---|---|---|
/// | ① `@ccm_sid` 那一侧的 `procStart` 冒名检查（`identity_tag.rs` 头注逐字：backend 打标前已过 `pid_alive` + `add_time_verdict`）| **PID 复用** | ❌ **威胁模型不对** —— 与上面那道是同一族，继承一格都不防 |
/// | ② `CC_BUS_ID` 那一侧的「无条件覆盖继承值」（`shared/ccm:1128`–`:1136`，记着一次**有可复现反例**的事故）| 继承 | ⚠ **原则可用、实现抄不了**：它成立靠「会话名是这个会话身份的唯一事实来源」——`derive_bus_id` 在**本地**就算得出真值，所以敢无条件覆盖。`CCM_LAUNCH_ID` **没有这样的本地真值**（token 是起会话方现铸的 nonce，被起的那一方无从复算）⇒ 写侧无法分辨「监视器刚给我的」与「我从父进程继承的」 |
///
/// ⇒ 本函数落的是 **② 的原则在读侧的兑现**：`CC_BUS_ID` 那条头注最后一句逐字是
/// 「**继承来的值一律不作数**」。读侧能独立判出来的「不作数」只有一条 ——
/// **一个 launch token 只对应一次拉起，因而只该落在一条活会话上**；
/// 落在两条以上，其中至少一条是继承来的，而**谁是原主判不出来**
/// ⇒ 照 `account: null` 那条「查不到就是查不到，**不猜**」，涉事的**全部**置 `None`。
///
/// # ⚠ 它买不到什么（如实写，别读宽）
///
/// - **父会话已经死了**的那一格买不到：死进程过不了 `procStart` 对拍 ⇒ `alive:false`
///   ⇒ 根本不读它的 environ ⇒ 撞不出重复，活着的那个子进程会带着继承来的 token 出现。
///   ⇒ 这条读回路的诚实边界是「**同一批里唯一**」，不是「**确实是它的**」。
/// - **跨批次**不判：本查询是一次性的（`exec` 一次、`ssh` 一次），没有跨调用的记忆。
/// - `launchId: null` **不区分原因**（没设 / 形状不对 / 不唯一 / 进程已死 /
///   **读那一刻环境取不到**）。要区分就得给出参加状态位，那是**改上线契约**——
///   与本文件头注给 `configDir` 那一格写下的裁决同一条（「不属本区范围」），此处照办。
///   ⚠ 第五种是 `K-R21`（09-03）现打出来的：它**一直都在**，只是从前混在「没设」里
///   数不出来（`platform/proc.rs` 那个 `None` 装着四件事）。**这条不是新增的行为，
///   是把「四种」这句旧话订正成实话** —— 而 `configDir` 那一半已经把它拆出来了
///   （`SessionRow::cfg_env_unreadable`），只有身份这一半仍按上面那条裁定合并着。
fn suppress_inherited_launch_ids(rows: &mut [SessionRow]) {
    let mut seen: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    for r in rows.iter() {
        if let Some(v) = r.launch_id.as_deref() {
            *seen.entry(v).or_insert(0) += 1;
        }
    }
    let dup: std::collections::HashSet<String> = seen
        .into_iter()
        .filter(|(_, n)| *n > 1)
        .map(|(v, _)| v.to_string())
        .collect();
    for r in rows.iter_mut() {
        if r.launch_id.as_deref().is_some_and(|v| dup.contains(v)) {
            tracing::warn!(
                "会话 pid={} 的 {LAUNCH_ID_ENV} 与别的活会话撞了 —— 至少有一条是继承来的，\
                 判不出谁是原主 ⇒ 两边都不作数（launchId: null）",
                r.pid
            );
            r.launch_id = None;
        }
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
                // 只 stat 存在性，绝不读内容。
                // **Z06 双写点**：这个文件名是「什么算已登录」的判据，而 cc-acct-iso
                // 的 `NATIVE_IDENTITY` 声明里也各写了一份（bash 侧 `cc-acct-iso` 的
                // `logged=` 那行）。两个进程、两种语言，无法共享常量 ⇒ 由本文件测试
                // 模块里的 `credential_filename_matches_native_identity_declaration`
                // 钉住（同 `TMUX_LS_FMT` 双写点那条守卫的做法）。**改这里必须改声明。**
                // 探不到 config dir（账号 0 且 manifest 没写 sharedStore）⇒ false，
                // 那是「不知道」，不假装已登录。
                let credentials_present = probe_dir
                    .as_ref()
                    .is_some_and(|d| d.join(CREDENTIALS_NAME).exists());
                // K-A1：鉴权方式这一维。分类与就绪**各只有一处实现**，都住 `acct-core`
                // ——本文件与 `local_accounts.rs` 都调它，所以「两个生产者各填一个不同的
                // 默认值」在结构上不可表示（`KAY1` 那条 acceptor 的失效模式就是这个）。
                let auth_kind = auth_kind_from_manifest(a.auth_kind.as_deref());
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
                        // 逐字节旧语义：仅 stat `.credentials.json` 存在性，**不代表凭据有效**
                        // （`KA6b` 今天之后仍然成立）。可用性**不再**直接读它，走 `authReady`。
                        "loggedIn": credentials_present,
                        // K-A1：订阅 / api-key。旧 manifest 缺这个键 ⇒ 订阅（`KA6d`）。
                        "authKind": auth_kind,
                        // K-A1：「鉴权方式这一维不再阻塞它被选中」。**不等于真能连上**
                        // （api-key 号还没有配端点的路 ⇒ `KA6a`，UI 必须把这个状态说出来）。
                        "authReady": auth_ready(auth_kind, credentials_present),
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
                    // Z01 能力标记：本后端认识「configDir 缺席 = 账号 0」。
                    // **旧后端不会出这个键**（它把账号 0 当坏数据跳过了）⇒ monitor 侧
                    // default=false ⇒ 能**明说**「远端后端太旧，列表里少了账号 0」，
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
fn session_accounts(agent_home: &Path, accts_dir: &Path) -> Vec<String> {
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

    let mut out: Vec<SessionRow> = Vec::new();
    let dir = crate::agents::claudecode::paths::sessions_root(agent_home);
    let Ok(rd) = std::fs::read_dir(&dir) else {
        return Vec::new(); // 没有 sessions/ → 零行（exit 0）
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
            Err(e) => {
                // ★〔audit-0805 §5 1x〕**跳过要说清是谁**。
                //
                // 这里原来是 `Err(_) => continue` —— 一个超过 `MAX_SESSION_FILE_BYTES`
                // 的 pidfile 会被**静默丢掉**，那个会话就永远不归属到任何账号，
                // 而没有任何东西说得出是哪一个。
                // ⚠ 而**同一个函数里** `MAX_SESSION_FILES` 超限是会 warn 的
                // （上面几行）—— 同一个函数里两种上限、两种态度。
                //
                // 登记表把它记成「硬报错」，那是**假的**：真实处置是跳过。
                // 定框 **E4**：静默失败一律给身份。⇒ 给它身份，并把登记改成实话
                // （新语义「跳过+说清」，与被刻意排除的「静默截断」的分界就在这个 warn）。
                tracing::warn!(
                    "会话文件 {} 读不了，跳过（不归属该会话）：{e}",
                    path.display()
                );
                continue;
            }
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
        // 🔴 `K-R21`（09-03）：**这一处从前在说一句斩钉截铁的假话。**
        //
        // 从前它是 `let cfg = if alive { proc_env_var(...) } else { None };` ——
        // 而 `proc_env_var` 那个 `None` 装着四件事，其中「**环境这一刻取不到**」
        // （读失败，或读回 0 字节：exec 窗口 / 僵尸进程）会一路走成
        // `cfg_norm = None` ⇒ 在下面的 `by_dir` 里**正好撞上账号 0 那个 `None` 键**
        // （`:568` 逐字 `None => Some((None, a.name))`）
        // ⇒ 出参不是「不知道」，是 `account: "<账号0>"` + `bare: true`。
        // ⇒ **一条真跑在账号 Z 下的会话，会被报成账号 0 的**，而且无声无息：
        // `alive` 仍是 `true`（它读 `/proc/<pid>/stat`，与 `environ` 不是同一次读）。
        //
        // 现在：「取不到」单独一支，**不归属、不算裸起**（`configDir:null` + `account:null`
        // + `bare:false` 今天就是一个可表达的状态 ⇒ 出参形状一个字节都没改）。
        // ⚠ 另两支（键不在 / 值是空串）**仍然合并**，理由在 `EnvRead` 的类型头注。
        let (cfg, cfg_env_unreadable) = if alive {
            match proc_env_var(pid, crate::agents::claudecode::paths::CONFIG_DIR_ENV) {
                EnvRead::Value(v) => (Some(v), false),
                EnvRead::Unset => (None, false),
                EnvRead::Unreadable => (None, true),
            }
        } else {
            // 进程已死时一个字节都不读 ⇒ 谈不上「取不到」，那一格照旧是「没读」。
            (None, false)
        };
        // `K-P5f`：**第二个键**。进程已死时一个字节都不读（同 `configDir` 那一格的理由：
        // `/proc/<pid>/environ` 在进程消失那一刻整个不存在，读了也只是 `None`；
        // 而万一 pid 被复用，读到的就是**别人的**环境）。
        // 形状不对 ⇒ 直接当没有（fail closed，见 `launch_id_is_safe`）。
        // ⚠ `K-R21` **刻意没动这一处**：它是 fail-closed 的（「取不到」与「没设」
        // 都得同一个保守答案 `None`），而 `:425` 那条裁定写着要区分就得给出参加状态位、
        // 那是改上线契约。⇒ 这里用 `.value()`，等于声明「这两件事对我等价」。
        // 🔴 代价如实登记：那条 flaky 判据
        // （`an_inherited_launch_id_is_never_reported_as_the_childs_own_identity`）
        // 红在**这条**读回路上，不是上面那条 ⇒ **本拍没有让它变绿，也不该被读成让它变绿**。
        let launch_id = if alive {
            proc_env_var(pid, LAUNCH_ID_ENV)
                .value()
                .filter(|v| launch_id_is_safe(v))
        } else {
            None
        };
        let cfg_norm = cfg.as_deref().map(|c| norm_dir(c).to_string());
        // 归属：有 configDir 就逐字匹配；没有、**进程确实活着**、**且环境这一刻读得到**
        // 才是账号 0。
        // 进程已死时不归属（cfg 恒 None，归给账号 0 会把死会话贴成账号 0 的）。
        // 🔴 `cfg_env_unreadable` 时也不归属（`K-R21`）：那一刻我们**不知道**它设没设，
        // 而「不知道」与「确实没设」在这里的差别就是一句假话的差别。
        let account = if alive && !cfg_env_unreadable {
            by_dir
                .iter()
                .find(|(d, _)| d.as_deref() == cfg_norm.as_deref())
                .map(|(_, n)| n.clone())
        } else {
            None
        };
        out.push(SessionRow {
            pid,
            sid: sid.map(str::to_string),
            cwd: cwd.map(str::to_string),
            config_dir: cfg_norm,
            account,
            alive,
            cfg_env_unreadable,
            launch_id,
        });
    }
    // 🔴 防冒名那一格**只能在这里判**：它要看完整批才知道有没有撞（`KP5FD5`）。
    suppress_inherited_launch_ids(&mut out);
    out.into_iter()
        .map(|r| {
            serde_json::json!({
                "pid": r.pid,
                "sessionId": json_str(r.sid.as_deref()),
                "cwd": json_str(r.cwd.as_deref()),
                "configDir": json_str(r.config_dir.as_deref()),
                "account": json_str(r.account.as_deref()),
                // 🔴 **`bare` 的语义钉死在 `CLAUDE_CONFIG_DIR` 上，加第二个键没有把它拓宽。**
                // 〔`KP5FD4`，`K-P5f` 第二拍现打的一格〕它的全部含义是「进程活着（身份已确认）
                // 但没设 `CLAUDE_CONFIG_DIR`」，**Z01 起这不再是异常**：它就是账号 0
                // （上面的 `account` 会给出名字）。字段保留是因为下游要用它区分
                // 「账号 0」与「设了 configDir 的账号」——语义从「告警」变成「事实」。
                // ⚠ **没设 `CCM_LAUNCH_ID` 不进这一格**，理由是两件事：`bare` 答的是
                // **账号**这一维（它与 `account` 是一对），`launchId` 答的是**身份**这一维。
                // 让一个布尔同时表示两个变量的缺席，正是本工作区最贵的那族病
                // （「一个值装了两件事」）；`launchId: null` 自己就说得清「没有」。
                // 🔴 **`K-R21`（09-03）给它补了第三个合取项，而语义没有拓宽、是收窄**：
                // 「没设」这句话只有在**环境读得到**的时候才说得出口。环境这一刻取不到时
                // （exec 窗口 / 僵尸进程）从前这里会斩钉截铁地报 `bare:true` ——
                // 那不是「裸起」，那是「不知道」。⇒ 加 `!r.cfg_env_unreadable`。
                // ⚠ 布尔仍然只答**账号**这一维，一格都没多装（那正是上一段在防的病）。
                "bare": r.alive && !r.cfg_env_unreadable && r.config_dir.is_none(),
                "alive": r.alive,
                // `K-P5f`：起会话方铸的身份 token（`CCM_LAUNCH_ID`），过了形状核与防冒名两道。
                // **`null` = 不作数**（没设 / 形状不对 / 与别的活会话撞了 / 进程已死 /
                // **读那一刻环境取不到**），五种原因**刻意不区分** —— 同 `account: null` 那条「不猜」。
                // ⚠ 第五种是 `K-R21`（09-03）现打出来的，**它一直都在、只是没人写出来**：
                // 从前它混在「没设」里数不出来。⇒ 这里不是新增了一种行为，是把一句
                // 「四种」的旧话订正成实话。
                // ⚠ 老后端不出这个键，下游读成 `None`（additive）。
                "launchId": json_str(r.launch_id.as_deref()),
            })
            .to_string()
        })
        .collect()
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
    cc_accounts::trust_of_config(&cc_accounts::config_path_in(Path::new(want)), cwd)
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
            "拿不到 $HOME，无法定位账号 0 的配置文件".to_string(),
        )
    })?;
    cc_accounts::trust_of_config(&cc_accounts::config_path_in(&home), cwd)
}

/// 查询模式入口。返回进程退出码（0 ok / 2 err），同 `history_query::run` 约定。
pub fn run(agent_home: &Path, args: &[String]) -> i32 {
    let accts_dir = resolve_accts_dir(args);
    match args.first().map(String::as_str) {
        Some("--list-accounts") => {
            for l in list_accounts(&accts_dir) {
                println!("{l}");
            }
            0
        }
        Some("--session-accounts") => {
            for l in session_accounts(agent_home, &accts_dir) {
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
            eprintln!("cc-monitor-backend accounts error: unknown argument: {other:?}");
            2
        }
    }
}

// ---------------------------------------------------------------- tests

#[cfg(test)]
#[path = "../../../tests/backend/observe/accounts_query_tests.rs"]
mod tests;
