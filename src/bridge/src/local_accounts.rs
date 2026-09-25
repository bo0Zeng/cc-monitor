//! L3a（local-as-remote）：**本机**多账号枚举 —— 只读。
//!
//! `accounts.rs` 是这件事的**远端**那半（把后端的 `--list-accounts` 包成 Tauri 命令）。
//! 本模块是它的本地对侧：**同样的输出类型**（`AccountsResult`），前端拿到的形状逐字段一致
//! ——那正是 §40「本地 = 不走 ssh 的远端」在这一格上的意思。
//!
//! # 〔C4c · 第四波 4B〕本机账号清单与信任预检**不在这里了**
//!
//! 两条 Tauri 命令（`list_local_accounts` · 本机那一侧的信任预检）退役：本机与远端同一条路 —— 前端经通道问
//! `<local>` 那条长连接的 `accounts-list` / `accounts-trust`，后端出成品（并表也在后端）。本文件今天只剩两样：
//! 下面那份**零生产调用方**的参照实现（`list_from_dir` 一族，理由见它的头注）与 `acct-iso` 的两问（文件末尾）。
//! 下面 `N-F1c` 那几节记的是来历（「问本机后端 `--list-accounts`」那一跳今天是帧面，不是一次性 exec）。
//!
//! # 🔴 `N-F1c`（2026-09-05）：读口**不再自己读磁盘**，改成问本机后端
//!
//! 用户拍的板逐字：「claude code 真实运行在哪台机器，他的账号就应该归哪台机器的后端管」。
//! ⇒ [`list_local_accounts`] 现在 exec 一次本机后端的 `--list-accounts`，〔散文墓碑〕
//! 把它吐的那几行交给 `accounts::parse_accounts_lines`（**与远端那条同一套解析、不同传输**），〔散文墓碑〕
//! 调用形状照同文件 E79 那条先例（〔C4a〕那一条已退役、改走通道），没有另造第二种调用法。
//!
//! 落地要先搬开两块石头，两块都在本轮搬掉了：
//! - backend 那份 `is_safe_config_dir` 第一条是 `p.starts_with('/')` ⇒ Windows 的账号目录
//!   `C:\Users\…` 会被判成不安全、**列表恒空**。已按下面那一节的原话拆成两半。
//! - 开发树里**没有** local_backend（发版时才注入）⇒ 读口必须**诚实降级**：
//!   [`LocalAccountsOutcome`] 是三档 tagged 返回，「够不着」绝不许渲染成「你没有账号」。
//!
//! # 为什么当年是第三份实现（历史；那一段代码今天仍在，理由见 [`list_from_dir`]）
//!
//! `src/backend/observe/accounts_query.rs` 已经有一份完整的 Rust manifest 读取器
//! （直接读文件系统）。当年**复用不了**，理由是结构性的：
//!
//! - 那个 crate 是 **bin-only**（无 `[lib]`），且 `Cargo.toml` 注释写明**刻意不进 workspace**
//!   ——「a workspace would pull this Linux-only backend into the Windows CI
//!   `cargo test --all` and break the build」。
//! - monitor **必须在 Windows 上构建**。让它依赖一个 Linux-only crate，正是那条注释在防的事。
//!
//! ⚠ `N-F1c` 绕开这条限制的办法**不是**去链接它，而是 **exec 它**：进程边界上没有 crate 依赖，
//! 所以那条注释挡的事一件都没发生。⇒ 「复用不了」在**链接**这个意义上今天仍然成立。
//!
//! ⇒ 于是这份数据有 **三个读者**：`cc-acct-iso`（bash，写侧）· backend（读侧，两条路都走它）·
//! 本模块保留的那份参照实现（只被判据驱动）。
//! 这是**已知代价**，处置照本仓既有纪律：**双写点必须有守卫**
//!（同 `TMUX_LS_FMT` / 观测取值 / Z06 凭据文件名那几条）。见本文件测试模块里的
//! **U7-3 起四条契约常量与欺骗字符判据都住在共享 crate `acct-core`** ——
//! 两侧 import 同一份，漂移**不可表示**（此前靠一条读对面源文件的守卫发现，已退役）。
//!
//! # 与后端那份**故意不同**的一处：路径绝对性判据
//!
//! backend 的 `is_safe_config_dir` 第一条是 `p.starts_with('/')`。那条在后端里是对的
//!（它只跑在 Linux 上），但**本模块要在 Windows 上跑**，而 Windows 的 config dir 是
//! `C:\Users\…`——照抄会把每一个 Windows 账号都判成不安全、列表恒空。
//!
//! ⇒ 拆开看这个判据在**防什么**：① shell 元字符与视觉欺骗字符（**平台无关的安全性质**，
//! 逐字照搬）② 「是绝对路径」（**平台相关的形式**，各写各的）。
//! **判据落在性质上，不落在表面特征上** —— 照抄 `starts_with('/')` 是抄了形式、丢了性质。

use crate::accounts::{AccountsMeta, AccountsResult, AuthKind, RemoteAccount};
// `N-F1c`：本机读口那一跳的传输。**只做调用方**，一个字都不改它的语义。
use crate::backend::observe::local_query::{run_query, QueryOutcome};
use acct_core::{
    auth_ready, is_deceptive_char, ACCTS_DIR_NAME, CREDENTIALS_NAME, MANIFEST_NAME,
    SUPPORTED_SCHEMA,
};
use std::path::{Path, PathBuf};

/// manifest 读取上限（与后端侧同值；账号数有限，8MB 是兜底不是预期）。
const MANIFEST_CAP: u64 = 8 * 1024 * 1024;

// 四条契约常量（账号库目录名 / manifest 文件名 / 凭据文件名 / schema 版本）
// U7-3 起住在 `acct-core`，见文件顶部的 `use`。**它们不再是双写点** ——
// 本模块与 backend import 同一份，想不一致得先把 import 删掉。
// bash 写侧（`cc-acct-iso`）是另一门语言、共享不了常量，那条对账留在
// `acct-core::tests::the_credential_filename_matches_the_cc_acct_iso_declaration`。

#[derive(serde::Deserialize)]
struct RawAccount {
    name: String,
    #[serde(default)]
    email: Option<String>,
    /// **Z01：可以缺席。** 缺席 = 账号 0 =「不设 `CLAUDE_CONFIG_DIR`」这个状态本身。
    /// **判据是结构性的（这个键在不在），不认名字**；空串**不算缺席**
    ///（`is_safe_config_dir("")` 会挡掉它 —— 空值 ≠ 未设）。
    #[serde(rename = "configDir", default)]
    config_dir: Option<String>,
    #[serde(rename = "isDefault", default)]
    is_default: bool,
    #[serde(default)]
    mode: Option<String>,
    /// **K-A1：鉴权方式。可以缺席** —— 缺席 = 旧 manifest = 订阅号（裁决见 `KA6d`）。
    /// 分类规则的唯一住址是 `acct_core::auth_kind_from_manifest`（经 `AuthKind::from_manifest`），
    /// **这里不许再写一份 match**。
    #[serde(rename = "authKind", default)]
    auth_kind: Option<String>,
}

#[derive(serde::Deserialize)]
struct RawManifest {
    version: Option<u64>,
    #[serde(rename = "updatedAt", default)]
    updated_at: Option<String>,
    #[serde(rename = "sharedStore", default)]
    shared_store: Option<String>,
    #[serde(default)]
    accounts: Vec<serde_json::Value>,
}

// U7-3：`is_deceptive_char` 搬进共享 crate `acct-core`。
// 它是**平台无关的安全性质**（不是 `char::is_control` —— 那只覆盖 C0/C1），
// 两侧读的是同一份 manifest，「什么算欺骗」必须一致。
// 实测搬之前两侧集合不同：backend 缺 word joiner / 各类空白（**真洞**，那些 is_control 是 false）；
// 本机缺 NEL（**非真洞** —— is_control 本来就挡着，U7-3 我误报过，U7-4 已证伪）。

/// 「是绝对路径」——**这一半是平台相关的**（见模块头注）。
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

/// config dir 是否可用。**空串在这里被挡掉** —— 空值 ≠ 未设（账号 0 是「键缺席」）。
fn is_safe_config_dir(p: &str) -> bool {
    if !looks_absolute(p) {
        return false;
    }
    if p == "/" || p.contains("/../") || p.ends_with("/..") {
        return false;
    }
    if p.contains("\\..\\") || p.ends_with("\\..") {
        return false;
    }
    // 平台无关的那一半：shell 元字符 + 视觉欺骗字符。**反斜杠不在此列** —— Windows 路径分隔符就是它。
    //
    // ⚠ **U8c-1 订正（2026-08-02）**：这里原本写着「本模块产出的值**不进任何 shell**
    // （本地注入走 env，不拼命令）」—— **那句今天是假的**。本模块的 `config_dir` 经
    // `fork-flow.ts` → tauri 命令 `resume_history_session` → `history.rs::build_local_posix_command`
    // 进了一条 `bash -lic` 的 shell 串。
    //
    // **实际无害**，但理由要说对：放行 `\` 在这里是安全的，因为**下游那一层自己会拒**
    // （`backend::control::payload::config_dir_command_safe` 明确把 `\` 列进拒绝集，POSIX 路径里不该有它）。
    // 也就是说这是**分层校验**，不是「不进 shell 所以不用管」。
    !p.chars().any(|c| {
        c.is_control()
            || is_deceptive_char(c)
            || matches!(
                c,
                '\'' | '"' | '`' | '$' | ';' | '|' | '&' | '<' | '>' | '*' | '?' | '(' | ')' | '!'
            )
    })
}

/// 去掉尾部分隔符，让不同来源写法能对上（backend 侧同义）。
fn norm_dir(p: &str) -> &str {
    let t = p.trim_end_matches('/');
    let t = t.trim_end_matches('\\');
    if t.is_empty() {
        p
    } else {
        t
    }
}

fn read_capped(path: &Path, cap: u64) -> Result<Vec<u8>, String> {
    let meta = std::fs::metadata(path).map_err(|e| format!("{}：{e}", path.display()))?;
    if !meta.is_file() {
        return Err(format!("{} 不是普通文件", path.display()));
    }
    if meta.len() > cap {
        return Err(format!("{} 过大（{} 字节）", path.display(), meta.len()));
    }
    std::fs::read(path).map_err(|e| format!("{}：{e}", path.display()))
}

/// 本机账号库目录。`None` = 取不到 HOME（无法枚举，不是「没有账号」）。
///
/// 🔴 `N-F1c` 起**零生产调用方**（读口改问后端，账号库在哪由后端自己解析 ——
/// 它还认 `~/.cc-acct-iso/config` 里的 `ACCTS_DIR=` 覆盖，本函数从来不认）。
/// 留着的理由只有一条、而且是硬的：`ACCTS_DIR_NAME` 这个**三方共用的契约名**
/// 今天在全仓只有一处逐字判据钉着它，就是驱动本函数的那一条
/// （`the_accounts_library_lives_under_the_contract_directory_name`）。
/// 删函数 = 删那条判据 = 降强度，而那不是实现方能自批的事。
/// ⇒ 退役条件：那条契约判据在 `acct-core` 里找到新家之后，本函数与它一起删。
#[allow(dead_code)] // 上面那段：留着是为了不删掉唯一钉住契约目录名的那条判据
fn local_accts_dir() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(ACCTS_DIR_NAME))
}

/// 纯函数核心：给定账号库目录，产出与远端**同形**的结果。
///
/// 拆成纯函数是为了测试能在 `mktemp` 造的沙盒目录上跑，
/// **绝不碰用户真实的 `~/.claude-alt`**。
///
/// # 🔴 `N-F1c`：**零生产调用方了**，而它仍然留着 —— 理由逐条写清
///
/// 读口（[`list_local_accounts`]）改走后端之后，本函数与它下面那一套
/// （`read_capped` / `MANIFEST_CAP` / `is_safe_config_dir` / `norm_dir` /
/// `RawAccount` / `RawManifest`）只剩判据在驱动。**这不是「暂时没人用就留着」**，
/// 是本仓自己那条墓碑纪律的正面适用 —— 同文件上一次删代码时逐字写的判准是
/// 「它们没有判据、没有测试、也不是任何东西的唯一锚点」，而这一套**三条全占**：
///
/// - `MANIFEST_CAP` 是 `byte_cap_registry` 那张表里点名的一格，
///   还与后端侧同名上限做**跨 crate 相等对拍**；
/// - `is_safe_config_dir` 是本仓「安全性质 / 平台形式」那条拆法的**参照实现** ——
///   `N-F1c` 就是照着它把后端那份改对的；
/// - 这一套下面挂着十来条判据（三种读失败分得开 / 欺骗字符 / `authKind` 跨生产者对拍 …），
///   删掉它们是降强度。
///
/// ⇒ 退役条件：上面三个锚点各自搬到新家（`acct-core` 是自然去处）之后，整套一起删。
/// ⚠ **它不是回落路径**：后端不在的时候读口走的是诚实降级
/// （[`LocalAccountsOutcome::NoBackend`]），**不是**偷偷回来读盘。
#[allow(dead_code)] // 上面那一节：三个锚点还挂在它身上，退役条件已写明
fn list_from_dir(accts_dir: &Path) -> AccountsResult {
    let mpath = accts_dir.join(MANIFEST_NAME);
    let meta_of = |enabled: bool, count: u32, updated_at, shared_store, error| AccountsMeta {
        enabled,
        accts_dir: accts_dir.to_string_lossy().into_owned(),
        manifest_path: mpath.to_string_lossy().into_owned(),
        updated_at,
        shared_store,
        count,
        error,
        // 本实现原生认得「configDir 缺席 = 账号 0」⇒ 恒 true（不像旧后端要降级提示）。
        account_zero_aware: true,
    };

    let bytes = match read_capped(&mpath, MANIFEST_CAP) {
        Ok(b) => b,
        Err(e) => {
            return AccountsResult {
                available: true, // 「本机没启用多账号」是正常状态，不是能力缺失
                error: None,
                meta: Some(meta_of(false, 0, None, None, Some(e))),
                accounts: Vec::new(),
                notice: None,
            };
        }
    };
    let raw: RawManifest = match serde_json::from_slice(&bytes) {
        Ok(m) => m,
        Err(e) => {
            return AccountsResult {
                available: true,
                error: None,
                meta: Some(meta_of(
                    false,
                    0,
                    None,
                    None,
                    Some(format!("manifest 不是合法 JSON：{e}")),
                )),
                accounts: Vec::new(),
                notice: None,
            }
        }
    };
    match raw.version {
        Some(v) if v == SUPPORTED_SCHEMA => {}
        Some(v) => {
            return AccountsResult {
                available: true,
                error: None,
                meta: Some(meta_of(
                    false,
                    0,
                    None,
                    None,
                    Some(format!("manifest schema 版本 {v} 不受支持（本机只认 1）")),
                )),
                accounts: Vec::new(),
                notice: None,
            }
        }
        None => {
            return AccountsResult {
                available: true,
                error: None,
                meta: Some(meta_of(
                    false,
                    0,
                    None,
                    None,
                    Some("manifest 缺 version 字段（或不是数字）".into()),
                )),
                accounts: Vec::new(),
                notice: None,
            }
        }
    }

    // 逐条解析：**单条坏不拖垮整表**（与 cc-acct-iso 写侧「丢单条」策略一致）。
    let mut out: Vec<RemoteAccount> = Vec::new();
    for (i, v) in raw.accounts.iter().enumerate() {
        let a: RawAccount = match serde_json::from_value(v.clone()) {
            Ok(a) => a,
            Err(e) => {
                tracing::warn!("本机 manifest 第 {i} 个账号解析失败，已跳过：{e}");
                continue;
            }
        };
        // Z01：configDir 缺席 = 账号 0。它的 config dir 就是共享库 ⇒ 登录态查那儿，
        // 而对外**出 `None`**（下游据此「不注入 CLAUDE_CONFIG_DIR」）。
        let (cfg_out, probe_dir) = match a.config_dir.as_deref() {
            None => (None, raw.shared_store.as_deref().map(PathBuf::from)),
            Some(c) => {
                if !is_safe_config_dir(c) {
                    tracing::warn!("本机账号 {} 的 configDir 不安全，已丢弃", a.name);
                    continue;
                }
                let n = norm_dir(c);
                (Some(n.to_string()), Some(PathBuf::from(n)))
            }
        };
        let has_cfg = a.config_dir.is_some();
        // 只 stat 存在性，**绝不读内容**。探不到目录 ⇒ false，那是「不知道」，不假装已登录。
        let credentials_present = probe_dir
            .as_deref()
            .map(|d| d.join(CREDENTIALS_NAME).is_file())
            .unwrap_or(false);
        // K-A1：分类与就绪各只有一处实现，都住 `acct-core` —— backend 的
        // `observe/accounts_query.rs` 调的是同两个函数。⇒「两个生产者各填一个不同的默认值」
        // 在结构上不可表示（`KAY1` 那条 acceptor 点名的失效模式）。
        let kind = AuthKind::from_manifest(a.auth_kind.as_deref());
        out.push(RemoteAccount {
            name: a.name,
            email: a.email.unwrap_or_default(),
            config_dir: cfg_out,
            is_default: a.is_default,
            mode: a.mode.unwrap_or_else(|| "isolated".into()),
            // 账号 0 恒 exists（「裸起」这个状态永远可达）；有 configDir 的看目录在不在。
            exists: match &probe_dir {
                Some(d) if has_cfg => d.is_dir(),
                _ => !has_cfg,
            },
            // 逐字节旧语义（`KA6b`）：只是 stat 结果，**不代表凭据有效**。
            // 可用性**不再**直接读它 —— 走下面的 `auth_ready`。
            logged_in: credentials_present,
            auth_kind: Some(kind),
            auth_ready: Some(auth_ready(kind.as_contract_str(), credentials_present)),
        });
    }

    let count = out.len() as u32;
    AccountsResult {
        available: true,
        error: None,
        meta: Some(meta_of(true, count, raw.updated_at, raw.shared_store, None)),
        accounts: out,
        notice: None,
    }
}

// 〔C4c · 第四波 4B〕本机账号清单那条 Tauri 命令（`list_local_accounts`）与它的三档结局（`LocalAccountsOutcome`）·
//   行解析转交（`classify_local_accounts`）· 本机并表（`with_apikey_table`）〔散文墓碑〕一起退役：本机与远端同一条路 ——
//   前端经通道说 `accounts-list`，那台机器的后端出成品、并它自己那份 apikey 表（规则住 `acct-core`）。
//   「本机后端不在 ≠ 你没有账号」那一格由通道的失败层级接住（`ipc/chan-caller.ts::saidOf`：够不着 ≠ 答了空表）。

#[cfg(test)]
#[path = "../../../tests/bridge/local_accounts_tests.rs"]
mod tests;

// 〔C4c · 第四波 4B〕本机的「这个账号信任过这个目录吗」（`accounts.trust` 的本机对侧）退役〔散文墓碑〕：`local_trust_argv` /
//   `classify_local_trust` / `local_account_trust` 三个函数随之删了〔散文墓碑〕；信任预检上了帧面（`accounts-trust`），
//   本机与远端同一条路（前端 `accounts.ts::checkTrust` 经通道问）。

// ─────────────────────────────────────────────────────────────────────────────
// E79：本机的「某个 sid 现在跑在哪个账号下」
// ─────────────────────────────────────────────────────────────────────────────

// 〔F10b 第二批·下半〕`MAX_LOCAL_SESSION_FILES` / `MAX_LOCAL_SESSION_FILE_BYTES` **已删** ——
// 它们是后端侧同名上限的**第二份**（`accounts_query.rs::MAX_SESSION_FILES` 与
// `accounts_query.rs::MAX_SESSION_FILE_BYTES`，值逐字相同：
// 500 个文件 / 1 MiB）。唯一的用处随 E79 那条本机查询改走本机后端一起消失（〔C4a〕那条查询后来也退役了）
// ⇒ 留着就是「同一个数两处各写一份」（定框 §4）。上限现在只有一个家：backend 那边，
// 且由它自己的测试与 `read_regular_capped` 钉着。

// 〔F10b 第二批〕`proc_claude_config_dir` 与 `pid_alive` **已删** ——
// 它们是后端侧 `platform/proc.rs` 那两个（`:19` / `:80`）的**第二份实现**，
// 而本文件的头注原本就写着「判据与后端侧逐字同源」。
// 唯一的调用方（E79 那条本机查询，〔C4a〕已退役）当时已改走本机后端的 `--session-accounts`
// ⇒ 留着就是「同一件事两处各写一份」（定框 §4），且平台原语该住 `platform/`（C10）。
// ⚠ 不是「暂时没人用就删」（铁律 13 禁的那种）：它们没有判据、没有测试、
//   也不是任何东西的唯一锚点 —— 语义的家在后端那边，且由它自己的测试钉着。

// 〔C4a · 第四波〕E79 那条本机版「某会话跑在哪个账号下」的 Tauri 命令**退役**：
//   它每问一次 exec 一个本机后端 `--session-accounts`、再把行解析一遍 —— 与远端那条
//   （`accounts.rs` 里 A2 那条，同拍退役）是**同一套解析、两种传输**。
//   现在两侧收成一条路：前端经通道（`chan::webview::chan_call`）问那台机器的后端 `accounts-sessions`
//   （本机由 `<local>` 那条长连接答），逐行解释只剩 `src/accounts.ts::parseSessionAccountLines` 一处。
//   上一版头注里记着的边界（本机后端不在 ⇒ 说原因、不伪造空表；Windows 上后端明说观测不到）
//   换成通道的三层错误：没有控制通道 / 后端不认 / 对端说不行，前端一律按「这一次没问出来」。

// ─────────────────────────────────────────────────────────────────────────────
// 〔`A3` 第二波〕`acct-iso.check` / `acct-iso.shellinit` 的本机对侧
// ─────────────────────────────────────────────────────────────────────────────
//
// 远端那两条（`acct_iso_deploy.rs::check_remote_acct_iso` / `remote_acct_iso_shellinit`）
// 吃 `RemoteConfig`、经 SSH 跑一串 shell；本机这两条**问本机后端**
// （`--acct-iso-status` / `--acct-iso-shellinit`，住后端账号层 `accounts/iso.rs`），
// 与 `list_local_accounts` 同一种调用法 —— `NR2`「claude 真实跑在哪台机器，账号就归那台的后端管」。
// 出参类型与远端那条**逐字相同**（`AcctIsoStatus` / 片段文本），前端按同一个形状读。

/// 后端 stderr 那一行 `{code,message}` 里的 `message`；解析不了就原样带回（不猜）。
fn backend_message(stderr: &str) -> String {
    serde_json::from_str::<serde_json::Value>(stderr.trim())
        .ok()
        .and_then(|v| {
            v.get("message")
                .and_then(|m| m.as_str())
                .map(str::to_string)
        })
        .unwrap_or_else(|| stderr.trim().to_string())
}

/// `--acct-iso-status` 的结局 → `AcctIsoStatus` —— **纯函数**。
///
/// 「没装」是 `Ok(installed:false)`（后端 exit 0 的那一支），不是 `Err`；
/// `Err` 只给「问不出来」的两档（后端不在 / 查询失败 / 出参读不懂），且**不许**说成「没装」。
pub(crate) fn classify_local_acct_iso(
    outcome: QueryOutcome,
) -> Result<crate::acct_iso_deploy::AcctIsoStatus, String> {
    let stdout = match outcome {
        QueryOutcome::Ok(s) => s,
        QueryOutcome::NoBackend(reason) => {
            return Err(format!(
                "本机后端不在，查不出这台机器装没装 cc-acct-iso：{reason}"
            ))
        }
        QueryOutcome::Failed { code, stderr } => {
            return Err(format!(
                "本机后端查不出 cc-acct-iso 装没装（退出码 {code:?}）：{}",
                backend_message(&stderr)
            ))
        }
    };
    let v: serde_json::Value = serde_json::from_str(stdout.trim())
        .map_err(|e| format!("本机后端回的 cc-acct-iso 状态读不懂：{e}"))?;
    let installed = v
        .get("installed")
        .and_then(serde_json::Value::as_bool)
        .ok_or_else(|| "本机后端回的 cc-acct-iso 状态里没有 installed".to_string())?;
    Ok(crate::acct_iso_deploy::AcctIsoStatus {
        installed,
        path: v.get("path").and_then(|p| p.as_str()).map(str::to_string),
        vendor_id: crate::acct_iso_deploy::vendor_id().to_string(),
    })
}

/// `acct-iso.check` 的本机对侧：这台机器装没装 `cc-acct-iso`。
#[tauri::command]
pub async fn check_local_acct_iso() -> Result<crate::acct_iso_deploy::AcctIsoStatus, String> {
    tokio::task::spawn_blocking(|| {
        classify_local_acct_iso(run_query(
            env!("CCM_TARGET_TRIPLE"),
            &["--acct-iso-status"],
            &*crate::spawn_managed::local_backend_one_shot_query(),
        ))
    })
    .await
    .map_err(|e| format!("本机 cc-acct-iso 查询没能跑完：{e}"))?
}

/// `--acct-iso-shellinit` 的结局 → 片段 —— **纯函数**。
///
/// 围栏校验与远端那条**同一个判定**（`acct_iso_deploy::shellinit_fence_state`），
/// 只是话按本机说（远端那句「先在『维护』里部署」对本机是一条走不通的路 ——
/// 本机的安装口今天不存在，`LOCAL_ACCOUNTS_COPY.emptyNext` 逐字写着）。
pub(crate) fn classify_local_shellinit(outcome: QueryOutcome) -> Result<String, String> {
    use crate::acct_iso_deploy::{shellinit_fence_state, FenceState};
    use crate::acct_iso_deploy::{SHELLINIT_FENCE_BEGIN, SHELLINIT_FENCE_END};
    let out = match outcome {
        QueryOutcome::Ok(s) => s,
        QueryOutcome::NoBackend(reason) => {
            return Err(format!("本机后端不在，拿不到这台机器的 rc 片段：{reason}"))
        }
        QueryOutcome::Failed { stderr, .. } => {
            return Err(format!(
                "本机没能产出 rc 片段：{}",
                backend_message(&stderr)
            ))
        }
    };
    match shellinit_fence_state(&out) {
        FenceState::Complete => Ok(out),
        FenceState::Truncated => Err(format!(
            "本机产出的 rc 片段不完整（有 {SHELLINIT_FENCE_BEGIN:?} 但没有 {SHELLINIT_FENCE_END:?}），\
             可能被截断了。别贴：半截片段会让登录 shell 报错。请重试。"
        )),
        FenceState::Missing => Err(format!(
            "本机没能产出 rc 片段（输出里没有 {SHELLINIT_FENCE_BEGIN:?}）。\
             常见原因：这台机器还没跑过 `cc-acct-iso init`。"
        )),
    }
}

/// `acct-iso.shellinit` 的本机对侧：这台机器的 `cc-acct-iso shellinit` 片段（**只读**，不代写 rc）。
#[tauri::command]
pub async fn local_acct_iso_shellinit() -> Result<String, String> {
    tokio::task::spawn_blocking(|| {
        classify_local_shellinit(run_query(
            env!("CCM_TARGET_TRIPLE"),
            &["--acct-iso-shellinit"],
            &*crate::spawn_managed::local_backend_one_shot_query(),
        ))
    })
    .await
    .map_err(|e| format!("本机 rc 片段查询没能跑完：{e}"))?
}
