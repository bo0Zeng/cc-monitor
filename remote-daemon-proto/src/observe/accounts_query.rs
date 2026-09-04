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
//! # 只读铁律（doc/INVARIANTS.md §1）
//! 本模块只 `read` / `read_dir` / `metadata`，**零写入**，且**不 shell out**
//! （daemon 是非登录 shell、PATH 很瘦；直接读 manifest 文件即可，省掉 PATH 依赖
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

// U2：**这里原本有第二份 `proc_starttime`**（`/proc/<pid>/stat` 的 field 22）。
// 与 `platform::proc::proc_starttime` 逐字同语义（都返回 boot 起的 jiffies，解析都是
// `nth(22-3)`），只是这一份把解析内联了、那一份走 `parse_starttime_from_stat`。
// 合并前**逐条核过单位**：单位不同的话它们就不是重复，合并就是引 bug。
use crate::common::fs::read_regular_capped;
use crate::agents::claudecode::accounts as cc_accounts;
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
/// monitor 侧的家是 `src-tauri/src/history.rs::LAUNCH_ID_VAR`，而
/// `remote-daemon-proto` 是**另一个 crate、另一份 `Cargo.lock`**（`src-tauri/Cargo.toml`
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
/// | ① `@ccm_sid` 那一侧的 `procStart` 冒名检查（`identity_tag.rs` 头注逐字：daemon 打标前已过 `pid_alive` + `add_time_verdict`）| **PID 复用** | ❌ **威胁模型不对** —— 与上面那道是同一族，继承一格都不防 |
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
                // ⚠ 老 daemon 不出这个键，下游读成 `None`（additive）。
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
            eprintln!("cc-monitor-remote accounts error: unknown argument: {other:?}");
            2
        }
    }
}

// ---------------------------------------------------------------- tests

#[cfg(test)]
mod tests {
    // U7-3：`credential_filename_matches_native_identity_declaration` 已搬进
    // 共享 crate `acct-core`（`the_credential_filename_matches_the_cc_acct_iso_declaration`）。
    // 常量住那儿，跟 bash 声明对账的守卫就该住那儿，否则又是两份。
    // 它原先还带一半「本文件真的在用这个字面量」—— 常量共享之后那半**结构上不可能不成立**。

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
        let r = read_regular_capped(&link, cc_accounts::MAX_CONFIG_BYTES);
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

    /// 🔴🔴 **`K-R21`：「环境这一刻取不到」不许被报成「账号 0 + 裸起」。**
    ///
    /// # 它守的那句假话长什么样
    ///
    /// `proc_env_var` 从前把四件事压成一个 `None`，其中「**这一刻读不出来**」会一路走成
    /// `configDir: null` ⇒ 在 `by_dir` 里**正好撞上账号 0 那个 `None` 键**
    /// ⇒ 出参是**斩钉截铁**的 `account:"0"` + `bare:true`。
    /// 而 `alive` 仍是 `true`（它读 `/proc/<pid>/stat`，与 `environ` **不是同一次读**）
    /// ⇒ **无声无息**：一条真跑在账号 Z 下的会话，会被报成账号 0 的。
    ///
    /// # 三个活体：两个是病，一个是对照
    ///
    /// | 活体 | 它让那次读走哪一支 | 该报什么 |
    /// |---|---|---|
    /// | 甲 · **僵尸**（子进程已退、故意不回收）| `std::fs::read` 回 **`Err`**（mm 已释放）| `account:null` · `bare:false` |
    /// | 乙 · **空环境活体**（`env_clear` 起的 `sleep`）| 回 **`Ok(vec![])`**（0 字节）| 同上 |
    /// | 丙 · **对照**：环境读得到、非空、确实没设那个键 | `Unset` | `account:"0"` · `bare:true` |
    ///
    /// 🔴 **丙这一格非有不可**：没有它，「三条全是 `null`」也会绿 ——
    /// 而那正是本条最容易退化成的样子（把归属整个摘掉也是这个读数）。
    /// 🔴 甲乙的 `alive` **都必须是 `true`**：那正是这句假话的杀伤力所在 ——
    /// 判活与读环境不是同一次读，所以「活着」与「读不出来」可以同时成立。
    ///
    /// # ⚠ 乙身上有一格**如实登记的重合**（别把本条读宽）
    ///
    /// `env_clear` 起的进程，它的环境**读得到、而且真的是空的** ——
    /// 也就是说「0 字节」这个信号自己也装着两件事：「这一刻读不出来」与「环境真的是空的」。
    /// 生产上后者不会发生（真 claude 进程至少有 `PATH`/`HOME`），且两者都落到**保守**的
    /// 那一侧（报「不知道」而不是报「账号 0」）⇒ `K-R21` **刻意不拆它**，登记在 `§7`。
    /// **别把本条读成「daemon 分得清这两件事」—— 它分不清。**
    #[cfg(target_os = "linux")]
    #[test]
    fn an_unreadable_environ_is_never_reported_as_the_zero_account() {
        let root = tmpdir("envhole");
        let claude = root.join("claude");
        let sessions = claude.join("sessions");
        fs::create_dir_all(&sessions).unwrap();
        let accts = root.join("accts");
        write_manifest(
            &accts,
            r#"{"version":1,"accounts":[{"name":"0","mode":"bare"}]}"#,
        );

        // 甲：僵尸 —— 起一个立刻退出的子进程，**故意不 `wait`**（不回收 ⇒ `/proc/<pid>` 还在）。
        let mut zombie = std::process::Command::new("sh")
            .arg("-c")
            .arg("exit 0")
            .spawn()
            .expect("起不来 sh —— 活体夹具起不来就**不许当绿**");
        let zpid = zombie.id();
        // 乙：空环境活体（真的在跑，state = S）。
        let mut empty = std::process::Command::new("sleep")
            .arg("60")
            .env_clear()
            .spawn()
            .expect("起不来 sleep（空环境活体）");
        let epid = empty.id();
        // 丙：对照活体 —— 环境读得到、**非空**、就是没设那个键。
        let mut plain = std::process::Command::new("sleep")
            .arg("60")
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .spawn()
            .expect("起不来 sleep（对照活体）");
        let ppid = plain.id();

        // 等甲真的成了僵尸（`/proc/<pid>/stat` 的 state 字段 = `Z`）。**不睡死等**：
        // 成不了僵尸就让下面的自检把它打红，而不是让本条零命中地绿。
        let state_of = |pid: u32| -> Option<char> {
            let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
            let after = stat.rfind(')')? + 2;
            stat[after..].chars().next()
        };
        for _ in 0..2_000 {
            if state_of(zpid) == Some('Z') {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }

        // ── 🔴 反空真自检：三个活体**此刻**真的各在自己那一支上 ──────────────
        // 没有这一段，夹具悄悄退化（比如僵尸被回收了、或空环境没生效）时本条会
        // 零命中地绿 —— 那正是 `K-G4 §7 裁六` 记的那一形。
        let zombie_state = state_of(zpid);
        let read_a = std::fs::read(format!("/proc/{zpid}/environ"));
        let read_b = std::fs::read(format!("/proc/{epid}/environ"));
        let read_c = std::fs::read(format!("/proc/{ppid}/environ"));
        let a_unreadable = match &read_a {
            Err(_) => true,
            Ok(b) => b.is_empty(),
        };
        let b_zero = matches!(&read_b, Ok(b) if b.is_empty());
        let c_ok = matches!(&read_c, Ok(b) if !b.is_empty());

        // 判据本体跑在**三条真活体**上（不是任何复刻）。
        let write_pidfile = |pid: u32, sid: &str| {
            let ticks = proc_starttime(pid)
                .unwrap_or_else(|| panic!("读不到 pid={pid} 的 starttime —— 活体没活着"));
            fs::write(
                sessions.join(format!("{pid}.json")),
                format!(r#"{{"sessionId":"{sid}","cwd":"/w","procStart":"{ticks}"}}"#),
            )
            .unwrap();
        };
        write_pidfile(zpid, "sid-zombie");
        write_pidfile(epid, "sid-emptyenv");
        write_pidfile(ppid, "sid-control");
        let by = sid_map(&session_accounts(&claude, &accts));

        // 先收拾活体，再断言（断言失败也不留孤儿进程）。
        let _ = empty.kill();
        let _ = empty.wait();
        let _ = plain.kill();
        let _ = plain.wait();
        let _ = zombie.wait();
        let _ = fs::remove_dir_all(&root);

        assert_eq!(
            zombie_state,
            Some('Z'),
            "甲没成僵尸（state={zombie_state:?}）—— 夹具没装上，下面几格量的不是本条要的东西"
        );
        assert!(
            a_unreadable,
            "甲的 environ 这一刻**读得出内容**（{:?}）—— 它就不在「取不到」那一支上了",
            read_a.as_ref().map(|b| b.len())
        );
        assert!(
            b_zero,
            "乙的 environ 不是 0 字节（{:?}）—— `env_clear` 没生效，支四那一格是空转",
            read_b.as_ref().map(|b| b.len())
        );
        assert!(
            c_ok,
            "丙的 environ 读不到或是空的（{:?}）—— 对照就不成其为对照了",
            read_c.as_ref().map(|b| b.len())
        );

        for sid in ["sid-zombie", "sid-emptyenv"] {
            assert_eq!(
                by[sid]["alive"], true,
                "{sid} 没判活 —— 而「活着」与「环境读不出来」同时成立正是这句假话的杀伤力所在"
            );
            assert_eq!(
                by[sid]["account"],
                serde_json::Value::Null,
                "\n🔴🔴 {sid}：环境这一刻**取不到**，而出参斩钉截铁地说它属于账号 0。\n\
                 那不是「裸起」，那是「不知道」——`by_dir` 里账号 0 的键正好也是 `None`，\n\
                 于是「读不出来」与「确实没设」撞在同一格上。\n\
                 ⇒ 一条真跑在账号 Z 下的会话会被报成账号 0 的，而 `alive` 仍是 `true`、无声无息。"
            );
            assert_eq!(
                by[sid]["bare"],
                false,
                "\n🔴 {sid}：`bare:true` 的含义是「进程活着、**读到了**、就是没设那个变量」。\n\
                 这一刻根本没读到 ⇒ 它说不出这句话。"
            );
            assert_eq!(by[sid]["configDir"], serde_json::Value::Null);
        }
        // 对照：读得到、非空、确实没设 ⇒ 归属**照旧**。掏掉归属那一格这里会红。
        assert_eq!(by["sid-control"]["alive"], true);
        assert_eq!(
            by["sid-control"]["account"], "0",
            "\n🔴 对照红了：环境读得到、非空、确实没设 `CLAUDE_CONFIG_DIR` —— 这就是**真裸起**，\n\
             它必须仍然归到账号 0。上面两格的 `null` 若与这一格同值，本条就退化成\n\
             「把归属整个摘掉也绿」。"
        );
        assert_eq!(by["sid-control"]["bare"], true);
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

    /// `agents::claudecode::accounts::trust_of_config` 是两个 trust 入口共用的那份实现（避免第二份）。
    /// ⚠ `S3` 把实现搬去了适配层，**本测原地留下**：它测的是"消费侧看到的行为"，
    /// 而消费侧（`--account-trust` / `--account-trust-zero`）还在本模块。断言一字未改。
    /// 账号 0 走 `$HOME/.claude.json`——声明里 `.claude.json` 的原生根就是 home。
    #[test]
    fn trust_of_claude_json_reads_only_the_three_booleans() {
        let root = tmpdir("acct0tz");
        fs::create_dir_all(&root).unwrap();
        let cj = root.join(".claude.json");

        // 文件不存在 ⇒ known:false，不是错误
        let v: serde_json::Value =
            serde_json::from_str(&cc_accounts::trust_of_config(&cj, "/w").unwrap()).unwrap();
        assert_eq!(v["known"], false);
        assert_eq!(v["trusted"], false);

        fs::write(
            &cj,
            r#"{"projects":{"/w":{"hasTrustDialogAccepted":true}},
                "mcpServers":{"x":{"env":{"API_KEY":"sk-SECRET"}}}}"#,
        )
        .unwrap();
        let out = cc_accounts::trust_of_config(&cj, "/w").unwrap();
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
        let main_raw = include_str!("../main.rs");
        // 只看生产段 + 剥行注释：两个文件的散文里都会提到这些字面量，
        // 不剥的话「main 的注释里写了它」也会让守卫变绿——那正是安慰剂。
        // U-1：剥法收敛到 `guard_support`。旧的内联版锚 `mod tests`，而 main.rs 的测试模块
        // 叫 `mod stream_flag_tests` ⇒ 匹配不上 ⇒ **这条守卫一直在拿测试段里那份副本对账**。
        let strip = crate::guard_support::production_code;
        let mine = strip(me);
        let main_prod = strip(main_raw);
        assert!(
            main_prod.len() > 3_000 && main_prod.len() < main_raw.len(),
            "剥完 main 生产段只剩 {} 字节（原文 {}）——剥法坏了",
            main_prod.len(),
            main_raw.len()
        );
        // ★ 上面那条 `len` 自检**检不出「测试段没剥掉」**（光靠剥注释就满足）。
        // 真正的判据是这条：剥完不许再有测试属性。
        crate::guard_support::assert_no_test_code("accounts_query/main.rs", &main_prod);
        crate::guard_support::assert_no_test_code("accounts_query/self", &mine);

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
            me.contains("config_path_in(&home)"),
            "账号 0 的配置文件必须来自 $HOME（声明里它的原生根是 home）。\n\
             ⚠ `S3` 前这条比的是字面量 `home.join(\".claude.json\")`——文件名随适配层搬走了，\n\
             比对对象换成那个 helper 的名字，**性质一字未变**：路径的根仍必须是 $HOME。"
        );
        assert!(me.len() > 1000, "include_str! 没读到源码，上面的断言是空转");
    }

    // ---- K-A1：鉴权方式这一维（生产者①） ----

    /// ★ **跨生产者对拍，daemon 这一半。**
    ///
    /// 喂的是 `acct_core::auth_kind_parity_manifest`（**两个 crate 共用的那一份**），
    /// 断的是 `acct_core::AUTH_KIND_PARITY_CASES` 里手写的金样。
    /// `local_accounts.rs` 那半断的是**同一张表**，所以「两个生产者各填一个不同的默认值」
    /// 会让其中一半当场红 —— 这正是 `KAY1` 那条 acceptor 点名的失效模式。
    ///
    /// ⚠ 它**只覆盖 `authKind` / `authReady` 这一维**（`KA6c`）：其余 6 个字段今天仍是
    /// 两份实现各写一遍，这条对拍看不见它们漂。
    #[test]
    fn auth_kind_parity_daemon_side() {
        let root = tmpdir("authkind-parity");
        for c in &acct_core::AUTH_KIND_PARITY_CASES {
            let d = root.join(c.name);
            fs::create_dir_all(&d).unwrap();
            if c.credentials_present {
                fs::write(d.join(CREDENTIALS_NAME), "{\"tok\":\"SECRET-TOKEN\"}").unwrap();
            }
        }
        fs::create_dir_all(root.join("shared")).unwrap();
        write_manifest(
            &root,
            &acct_core::auth_kind_parity_manifest(&root.to_string_lossy()),
        );
        let lines = list_accounts(&root);
        // 首行是 meta，之后一行一个账号 —— 行数自检，防「少了几行也照样逐格绿」。
        assert_eq!(
            lines.len(),
            acct_core::AUTH_KIND_PARITY_CASES.len() + 1,
            "行数对不上，逐格断言会漏掉没出来的那几个账号：{lines:?}"
        );
        for (i, c) in acct_core::AUTH_KIND_PARITY_CASES.iter().enumerate() {
            let v: serde_json::Value = serde_json::from_str(&lines[i + 1]).unwrap();
            assert_eq!(v["name"], c.name, "顺序变了，下面几格就对错人了");
            assert_eq!(
                v["authKind"], c.expect_auth_kind,
                "{}：daemon 产出的 authKind 与金样不一致",
                c.name
            );
            assert_eq!(
                v["authReady"], c.expect_auth_ready,
                "{}：daemon 产出的 authReady 与金样不一致",
                c.name
            );
            // `loggedIn` 逐字节旧语义：仍然只是「凭据文件在不在」。
            assert_eq!(
                v["loggedIn"], c.credentials_present,
                "{}：loggedIn 的语义被这次改动动了（它该只是 stat 结果）",
                c.name
            );
        }
        for l in &lines {
            assert!(!l.contains("SECRET-TOKEN"), "输出里出现了凭据内容：{l}");
        }
        let _ = fs::remove_dir_all(&root);
    }

    /// ★ `KAY4` 的 **Rust 侧那一格**（vitest 那条守卫扫不到这里）。
    ///
    /// 守的性质：本文件里「鉴权方式这一维」**不许有第二条计算路径** ——
    /// `authReady` 只许来自 `acct_core::auth_ready(`，`authKind` 只许来自
    /// `acct_core::auth_kind_from_manifest(`。有人在这儿手写一个
    /// `if kind == "api-key" { true } else { … }`，本条红。
    ///
    /// ⚠ 射程如实写：它**只管本文件**（另一个生产者由
    /// `local_accounts.rs::the_auth_dimension_has_exactly_one_computation_path` 守自己那份），
    /// 而且是**字面量扫描** —— 把两个 helper 重新 `use` 成别名就绕得过去。
    /// 真正的地板不是它，是 `acct-core` 里只有一份实现。
    #[test]
    fn the_auth_dimension_has_exactly_one_computation_path() {
        let me = include_str!("accounts_query.rs");
        assert!(me.len() > 20_000, "include_str! 没读到源码，本条在空转（实得 {} 字节）", me.len());
        // 只看生产段：`#[cfg(test)]` 之前的那一半（本文件的测试段自己就会提到这些名字）。
        let marker = "#[cfg(test)]";
        let cut = me.find(marker).expect("找不到 #[cfg(test)] 锚点 —— 切法失效了");
        let prod_with_comments = &me[..cut];
        assert!(
            prod_with_comments.len() > 15_000,
            "生产段只切出 {} 字节 —— 锚点挪了，下面几条会零命中地绿",
            prod_with_comments.len()
        );
        // **去注释口径**：本文件的注释里就在解释这一维，按裸文本数会把散文也数进来
        // （第一版正是这么假红的：注释里两处 `api-key` 被当成了第二条计算路径）。
        // 全部是行注释，所以按行剥就够；剥完做锚点自检，防剥过头。
        let prod: String = prod_with_comments
            .lines()
            .filter(|l| !l.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            prod.contains("fn list_accounts(accts_dir: &Path)") && prod.len() > 8_000,
            "剥注释剥过头了（实得 {} 字节）—— 下面几条会零命中地绿",
            prod.len()
        );
        assert_eq!(
            prod.matches("auth_ready(").count(),
            1,
            "生产段里 `auth_ready(` 出现了不止一次 —— 要么有了第二条计算路径，要么该收进 acct-core"
        );
        assert_eq!(
            prod.matches("auth_kind_from_manifest(").count(),
            1,
            "生产段里 `auth_kind_from_manifest(` 出现了不止一次"
        );
        assert_eq!(
            prod.matches("\"authReady\"").count(),
            1,
            "`authReady` 这个键在生产段里被写了不止一处"
        );
        // 阴性对照式自检：本文件的生产段里**不许**出现 api-key 这个字面量
        // （分类规则住 acct-core；这里出现它就意味着有人在本地又判了一次）。
        assert_eq!(
            prod.matches("api-key").count(),
            0,
            "生产段里出现了 `api-key` 字面量 —— 鉴权方式的分类只许住 acct-core"
        );
    }

    // ============================================================================
    // `K-P5f` 第二拍：身份 token 读回来那一侧
    // ============================================================================

    /// 本文件生产段的**去注释**文本。
    ///
    /// 🔴 **它自己不写剥法，调共享原语** —— 第一版写了一份（切到 `#[cfg(test)]` + 过滤
    /// `//` 开头的行），`structural_scan::every_comment_stripping_transformer_is_registered`
    /// 当场逮住它，逐字问：「先问共享原语为什么不够 —— 答得出来就登记，答不出来就改成调它」。
    /// **答不出来**（`production_code` 做的就是这两件事）⇒ 改成调它。
    /// ⚠ 那张登记表住 `src-tauri/src/structural_scan.rs`，**不在本拍写区** ——
    /// 而它给的第一条出路本来就不需要动登记表。〔与 `launcher_identity_registry` 头注
    /// 记的那一次是同一条：那一次也是这条判据逮的，处置也一样。〕
    fn production_text() -> String {
        let me = include_str!("accounts_query.rs");
        assert!(
            me.len() > 20_000,
            "include_str! 没读到源码，本条在空转（实得 {} 字节）",
            me.len()
        );
        let prod = crate::guard_support::production_code(me);
        crate::guard_support::assert_no_test_code("accounts_query.rs", &prod);
        assert!(
            prod.contains("fn session_accounts(agent_home: &Path")
                && prod.contains("fn suppress_inherited_launch_ids(")
                && prod.len() > 8_000,
            "剥过头 / 锚点挪了（实得 {} 字节）—— 用它的那几条会零命中地绿",
            prod.len()
        );
        prod
    }

    /// ★★ **双写点对拍**：身份变量名两侧必须逐字一致。
    ///
    /// monitor 侧的家是 `src-tauri/src/history.rs::LAUNCH_ID_VAR`，而这里是
    /// [`LAUNCH_ID_ENV`] —— 两个 crate、两份 `Cargo.lock`，**共享不了常量**
    /// （同 `CREDENTIALS_NAME` 那个 Rust ↔ bash 的双写点，处置照它：由测试钉住）。
    ///
    /// # 🔴 为什么**运行时读**对面那份源码，而不是编译期把它拉进来
    ///
    /// 本 crate 已有的跨树先例（`control/launch.rs` 钉 `TMUX_LS_FMT` 那个双写点）走的是
    /// 编译期那条。**本条刻意不走**：编译期那一形是「两半之间的编译期边」，
    /// 由 `src-tauri/src/cross_half_edge_registry.rs` 的登记表逐条数着（多一条 ⇒ 红），
    /// 而**那张表不在本拍写区**。实打过：第一版用编译期那条，那条判据当场红
    /// （`实得 18，登记 16`）。⇒ 改成运行时读，代价与补偿如实写：
    /// - **代价**：文件不在 / 路径挪了时，编译期那条编不过（响亮），运行时这条只有本条红；
    /// - **补偿**：`expect` + 字节数地板 —— 读不到就 panic，**不许静默成 0 字节地绿**。
    ///
    /// ⚠ 它买不到什么：只买「**那个字面量两边一样**」。写侧真的把它 `export` 出去了没有，
    /// 是 monitor 那边 `the_launcher_plants_the_session_identity_into_the_process_environment`
    /// 五格的活，本条不重复买。
    #[test]
    fn the_launch_id_env_var_matches_the_monitor_side_home() {
        let monitor_history_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("remote-daemon-proto 的上级 = 仓根")
            .join("src-tauri/src/history.rs");
        let monitor_history = std::fs::read_to_string(&monitor_history_path)
            .unwrap_or_else(|e| panic!("读不到 {monitor_history_path:?}：{e}"));
        assert!(
            monitor_history.len() > 50_000,
            "只读到 {} 字节的 monitor `history.rs` —— 没读到真文件，本条在空转",
            monitor_history.len()
        );
        let expected = format!("LAUNCH_ID_VAR: &str = \"{LAUNCH_ID_ENV}\"");
        assert!(
            monitor_history.contains(&expected),
            "\n★★ 身份变量名双写点漂移：monitor 侧 `history.rs` 里找不到 {expected:?}。\n\
             写侧（`history.rs::LAUNCH_ID_VAR`）与读侧（本文件的 `LAUNCH_ID_ENV`）\n\
             必须是同一个字面串 —— 漂开的症状是**读侧恒 `null`**，\n\
             而 `null` 在本查询里是合法值（「不作数」）⇒ **不会有任何东西报错**。"
        );
    }

    /// ★★ **本文件读环境这件事的射程不许悄悄变大**〔本文件头注那条「两个写死的键」的判据〕。
    ///
    /// 🔴 **主语就是「本文件」，不是「daemon 全体」**〔`K-R21` 09-03 收窄的措辞〕：
    /// 本条的分母是 `include_str!("accounts_query.rs")` 的生产段 —— **一个文件**，
    /// 与测试名里那个 `this_module` 逐字对齐。
    /// ⚠ daemon 里**还有第三处**在读 `/proc/<pid>/environ`：`control/identity_tag.rs`
    /// 读 `TMUX_PANE`（09-03 现打，生产段共 3 个调用方）。它**不在本条视野里**，
    /// 而这句话原来读起来像全仓 —— 那正是本仓登记过的「量具的作用域对不上事实」那一形。
    /// ⇒ `K-R21` PM 裁定：**不为此装第二把尺子**（那个人群今天产出过 0 条假话，
    /// 为它付一把新尺子的固定成本不划算 —— 同 `K-R19` 裁「闸 G 不装」的口径），
    /// **改的是这句话的主语**。别把这一格读成「全 daemon 只读两个键」。
    ///
    /// 守的性质：`/proc/<pid>/environ` 只抠**两个常量键**，键名**不许成为一维参数**。
    /// 多一处 `proc_env_var(pid, …)` ⇒ 红，来这里回答「新那个键是什么、为什么它不
    /// 把本查询变成任意环境变量读原语」。
    #[test]
    fn the_only_env_keys_this_module_reads_are_the_two_named_constants() {
        let prod = production_text();
        let total = prod.matches("proc_env_var(pid, ").count();
        assert_eq!(
            total, 2,
            "\n本文件生产段里 `proc_env_var(pid, …)` 有 {total} 处（登记 2 处）。\n\
             **多了** ⇒ 又读了第三个环境变量：来模块头注那一格写清它是什么、\n\
             以及为什么这条查询仍然不是「任意环境变量读」原语。\n\
             **少了** ⇒ 有一条读回路被摘掉了。"
        );
        assert_eq!(
            prod.matches("proc_env_var(pid, crate::agents::claudecode::paths::CONFIG_DIR_ENV)")
                .count(),
            1,
            "抠 `CLAUDE_CONFIG_DIR` 那一处不见了 / 变形了"
        );
        assert_eq!(
            prod.matches("proc_env_var(pid, LAUNCH_ID_ENV)").count(),
            1,
            "抠身份 token 那一处不见了 / 变形了（`K-P5f` 读侧的正主）"
        );
    }

    /// ★★ **「盘上写着的」↔「我们真发的」对拍**〔`KP5FD4` 那句「判据要自己长出来」〕。
    ///
    /// # 它为什么非有不可
    ///
    /// `K-P5f` 摸底现打过：往 `--session-accounts` 加字段这条路**撞 0 道机检**
    /// （`protocol_doc_guard` 那两条一条够不着它 —— 出参是本文件里一个就地
    /// `serde_json::json!`，不是 `wire.rs` 里的类型；另一条数的是**子命令名**，
    /// 而 `--session-accounts` 早在表里）。⇒ 文档那一行**只靠人记得改**。
    /// 而「没有闸看着的文档事实」正是本工作区反复判过的假绿源
    /// （`K-P5c §7 上报-3`「写着有、其实没有」同族）。**这条就是那道闸。**
    ///
    /// # 三格
    ///
    /// | 格 | 断的是什么 | 翻掉它的形状 |
    /// |---|---|---|
    /// | ① | 出参字段表逐字等于生产段真发的那几个键（**顺序也算**） | 加一个字段不改文档 / 改了名字 |
    /// | ② | 文档那一行把**两个**环境变量键都点了名 | 加第二个键、却留着「只抠 `CLAUDE_CONFIG_DIR` 一个键」那句假话 |
    /// | ③ | 本文件头注也把两个键都点了名 | 只改文档、漏了 `:55` 那句同义的诚实边界（派工单逐字：「两处都改，漏一处就是留假话」） |
    ///
    /// # ⚠ 它买不到什么
    ///
    /// 只买「**那几个名字都在场**」。文档那一行**说得对不对**（比如 `launchId` 的语义
    /// 解释）它一个字都判不了 —— 那是评审的活。
    #[test]
    fn the_protocol_doc_row_for_session_accounts_matches_what_we_emit() {
        const DOC: &str = include_str!("../../../doc/IPC-PROTOCOL.md");
        assert!(
            DOC.len() > 20_000,
            "只读到 {} 字节的 `doc/IPC-PROTOCOL.md` —— include_str! 没读到，本条在空转",
            DOC.len()
        );
        let row = DOC
            .lines()
            .find(|l| l.starts_with("- `--session-accounts "))
            .expect("`doc/IPC-PROTOCOL.md` 里找不到 `--session-accounts` 那一行 —— 锚点挪了");

        // ── ① 出参字段表：从生产段把 `json!` 的键抠出来，与文档里那个花括号表对拍 ──
        let prod = production_text();
        let start = prod
            .find("fn session_accounts(agent_home")
            .expect("找不到 `session_accounts` —— 抽取器坏了");
        let body = &prod[start..];
        let j = body
            .find("serde_json::json!({")
            .expect("`session_accounts` 里找不到出参 `json!` —— 抽取器坏了");
        let mut keys: Vec<String> = Vec::new();
        for l in body[j..].lines().skip(1) {
            let t = l.trim();
            if t == "})" {
                break;
            }
            if let Some(r) = t.strip_prefix('"') {
                if let Some(i) = r.find("\":") {
                    keys.push(r[..i].to_string());
                }
            }
        }
        assert_eq!(
            keys.len(),
            8,
            "从出参 `json!` 只抠到 {} 个键（09-02 现打 8）—— 抽取器坏了，下面那格会零命中地绿：{keys:?}",
            keys.len()
        );
        let table = format!("{{{}}}", keys.join(","));
        assert!(
            row.contains(&table),
            "\n★★ `doc/IPC-PROTOCOL.md` 的 `--session-accounts` 那一行里找不到字段表 {table:?}。\n\
             出参加了字段 / 改了名 / 换了顺序，而文档没跟着改 —— **盘上留了一句假话**，\n\
             而这条路撞 0 道机检，除了本条没有任何东西会说。\n\
             文档那一行现在写的是：\n  {row}"
        );

        // ── ② / ③ 「只抠几个键」那句诚实边界：文档与本文件头注都得把两个键点到名 ──
        let header: String = include_str!("accounts_query.rs")
            .lines()
            .take_while(|l| l.starts_with("//!") || l.trim().is_empty())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            header.len() > 2_000,
            "头注只切出 {} 字节 —— 切法坏了，③ 那格会零命中地绿",
            header.len()
        );
        for (what, hay) in [("doc/IPC-PROTOCOL.md 那一行", row), ("本文件头注", header.as_str())] {
            for key in ["CLAUDE_CONFIG_DIR", LAUNCH_ID_ENV] {
                assert!(
                    hay.contains(key),
                    "\n★ {what} 里没点名 `{key}` —— 「`/proc/<pid>/environ` 只抠哪几个键」\n\
                     这句诚实边界在那儿就成了假话（漏一处就是留假话，派工单逐字）。"
                );
            }
        }
    }

    /// fail closed：形状不对的 token 一律不往下游递。
    #[test]
    fn the_launch_id_shape_gate_is_fail_closed() {
        // 两种真形态都得过：UUID v4（新开那一支的 nonce）与 sid（resume 那一支）。
        assert!(launch_id_is_safe("0198f0d2-1111-4222-8333-444455556666"));
        assert!(launch_id_is_safe("a_b-1"));
        // 空 / 超长 / 能破坏下游的字符，全挡。
        assert!(!launch_id_is_safe(""));
        assert!(!launch_id_is_safe(&"a".repeat(129)));
        for bad in [
            "a b",
            "a;rm -rf /",
            "$(id)",
            "a'b",
            "a\"b",
            "a\nb",
            "a\u{0}b",
            "中文",
        ] {
            assert!(!launch_id_is_safe(bad), "{bad:?} 不该被放行");
        }
    }

    fn row(pid: u32, sid: &str, launch: Option<&str>) -> SessionRow {
        SessionRow {
            pid,
            sid: Some(sid.to_string()),
            cwd: None,
            config_dir: None,
            account: None,
            alive: true,
            // 这几条纯函数用例喂的是**已经读完之后**的 rows ⇒ 那一次读是成功的。
            cfg_env_unreadable: false,
            launch_id: launch.map(str::to_string),
        }
    }

    /// 防冒名那一格的**纯函数**半：唯一的留下，撞了的一律不作数。
    #[test]
    fn a_launch_id_that_lands_on_more_than_one_session_counts_for_nobody() {
        let mut rows = vec![
            row(1, "sid-a", Some("tok-shared")),
            row(2, "sid-b", Some("tok-shared")),
            row(3, "sid-c", Some("tok-alone")),
            row(4, "sid-d", None),
        ];
        suppress_inherited_launch_ids(&mut rows);
        assert_eq!(rows[0].launch_id, None, "撞了的那一条必须不作数");
        assert_eq!(rows[1].launch_id, None, "撞了的另一条也必须不作数");
        assert_eq!(
            rows[2].launch_id.as_deref(),
            Some("tok-alone"),
            "只落在一条会话上的 token 必须留下 —— 否则本函数是「全部抹掉」，那不是判据是空转"
        );
        assert_eq!(rows[3].launch_id, None);
    }

    /// 🔴🔴 **活体夹具**〔`KP5FD5`〕：造一个**真的**「父的值漏进子进程」的活体，
    /// 让**判据本体**（`session_accounts` 自己，不是它的复刻）跑在上面。
    ///
    /// # 为什么非活体不可（`K-G4 §7 裁六` 的实测）
    ///
    /// 那一拍现打过：掏空共用原语时**方向判据全留绿，只有活体夹具红**。
    /// 这里的等价失效是：把 [`suppress_inherited_launch_ids`] 的函数体清空，
    /// 上面那条纯函数判据当然会红 —— 但那条判据**是我自己喂的 rows**，
    /// 它证明不了「真从 `/proc` 读回来的两条会话真的会撞」。本条证明它。
    ///
    /// # 这个活体是真的（逐条说清哪一格是真的）
    ///
    /// - **真进程**：`sh` 起来之后 `exec sleep`，环境里带着 token；
    /// - **真继承**：它 fork 出的后台 `sleep` 的 `CCM_LAUNCH_ID` **一个字都不是自己的**，
    ///   是从父进程继承的 —— 这正是 `/branch` / SDK 起的子进程 / claude 自己 spawn 的
    ///   那一族在生产上的形状；
    /// - **真 `/proc`**：两条都过 `session_process_identity_ok`（pidfile 的 `procStart`
    ///   是现读的），也就是说**它们过得了本文件里另一道身份检查** ——
    ///   那道防的是 PID 复用，一个字都不防继承；
    /// - **判据本体**：断言跑的是 `session_accounts(...)` 的出参 JSON，不是任何复刻。
    ///
    /// # 非空对照（第二段）
    ///
    /// 把子进程那份 pidfile 删掉再跑一次，父那条**必须**带着 token 回来。
    /// 没有这一段，「全都 `null`」也会绿 —— 而那是本条最容易退化成的样子。
    #[cfg(target_os = "linux")]
    #[test]
    fn an_inherited_launch_id_is_never_reported_as_the_childs_own_identity() {
        use std::io::{BufRead, BufReader};

        const TOKEN: &str = "kp5f-live-0198f0d2-1111-4222-8333";
        let root = tmpdir("inherit");
        let claude = root.join("claude");
        let sessions = claude.join("sessions");
        fs::create_dir_all(&sessions).unwrap();

        // 父：`sh` 先 fork 一个后台 `sleep`（**继承者**），印出它的 pid，再把自己 exec 成 `sleep`。
        // `exec` 不改 pid、不改 starttime、**不改环境** ⇒ 两个进程的 environ 里都有 TOKEN。
        let mut parent = std::process::Command::new("sh")
            .arg("-c")
            .arg("sleep 60 & printf '%s\\n' \"$!\"; exec sleep 60")
            .env(LAUNCH_ID_ENV, TOKEN)
            .stdout(std::process::Stdio::piped())
            .spawn()
            .expect("起不来 sh —— 活体夹具起不来就**不许当绿**");
        let ppid = parent.id();
        let mut line = String::new();
        BufReader::new(parent.stdout.take().expect("拿不到 stdout"))
            .read_line(&mut line)
            .expect("读不到子进程 pid");
        let cpid: u32 = line.trim().parse().expect("子进程 pid 不是数字");

        // 第三个活体：token 的**形状**过不了白名单（空格 + `;`）。它与上面两条不撞 ⇒
        // 它那一格的 `null` **只能**来自形状核 ⇒ 拆掉 `launch_id_is_safe` 那一格它就红。
        // （没有这一段，形状核在活体上一颗牙都没有：`launch_id_is_safe` 的单测是纯函数，
        //  拆掉调用点它照样全绿 —— 那正是 `K-G4 §7 裁六` 记的那一形。）
        let mut evil = std::process::Command::new("sh")
            .arg("-c")
            .arg("exec sleep 60")
            .env(LAUNCH_ID_ENV, "not a token; rm -rf /")
            .spawn()
            .expect("起不来 sh（形状那一格的活体）");
        let epid = evil.id();

        let write_pidfile = |pid: u32, sid: &str| {
            let ticks = proc_starttime(pid)
                .unwrap_or_else(|| panic!("读不到 pid={pid} 的 starttime —— 活体没活着"));
            fs::write(
                sessions.join(format!("{pid}.json")),
                format!(r#"{{"sessionId":"{sid}","cwd":"/w","procStart":"{ticks}"}}"#),
            )
            .unwrap();
        };
        write_pidfile(ppid, "sid-parent");
        write_pidfile(cpid, "sid-child");
        write_pidfile(epid, "sid-badshape");

        // ① 两条都在 ⇒ 撞 ⇒ 两条都不作数。
        let both = sid_map(&session_accounts(&claude, &root.join("no-accts")));
        // ② 非空对照：只留父那一条 ⇒ token 必须回得来。
        fs::remove_file(sessions.join(format!("{cpid}.json"))).unwrap();
        let alone = sid_map(&session_accounts(&claude, &root.join("no-accts")));

        // 先收拾活体，再断言（断言失败也不留孤儿 `sleep`）。
        let _ = parent.kill();
        let _ = parent.wait();
        let _ = evil.kill();
        let _ = evil.wait();
        let _ = std::process::Command::new("kill")
            .arg(cpid.to_string())
            .status();
        let _ = fs::remove_dir_all(&root);

        assert_eq!(
            both["sid-parent"]["alive"], true,
            "父那条没判活 —— 夹具没装上，下面几格量的不是本件的东西"
        );
        assert_eq!(
            both["sid-child"]["alive"], true,
            "子那条没判活 —— 而它**正是**过得了 procStart 对拍、却拿着别人 token 的那一格"
        );
        assert_eq!(
            both["sid-child"]["launchId"],
            serde_json::Value::Null,
            "\n🔴 子会话把**继承来的** token 报成了自己的身份。\n\
             这就是 `CC_BUS_ID` 那条头注记着的、**有可复现反例**的事故换个方向重演：\n\
             父 agent 的身份漏进子 agent ⇒ 冒名。"
        );
        assert_eq!(
            both["sid-parent"]["launchId"],
            serde_json::Value::Null,
            "\n🔴 撞了之后**父那条也不许留** —— 判不出谁是原主时挑一个留下就是猜，\n\
             而本查询的纪律逐字是「查不到就是查不到，不猜」。"
        );
        assert_eq!(
            both["sid-badshape"]["alive"], true,
            "形状那一格的活体没判活 —— 它那一格的 null 就说明不了是形状核干的"
        );
        assert_eq!(
            both["sid-badshape"]["launchId"],
            serde_json::Value::Null,
            "\n🔴 形状过不了白名单的 token 被原样递给了下游。\n\
             它与另外两条**不撞** ⇒ 这一格的 null 只能由 `launch_id_is_safe` 买；\n\
             红了就是那道 fail-closed 被摘掉了（而这个值是任意用户可控的）。"
        );
        assert_eq!(
            alone["sid-parent"]["launchId"], TOKEN,
            "\n🔴 非空对照红了：只有一条会话时 token 都回不来 —— \n\
             那么上面两格的 `null` 证明不了防冒名在起作用（全抹掉也是这个读数）。"
        );
    }
}
