//! L3a（local-as-remote）：**本机**多账号枚举 —— 只读。
//!
//! `accounts.rs` 是这件事的**远端**那半（把 daemon 的 `--list-accounts` 包成 Tauri 命令）。
//! 本模块是它的本地对侧：**同样的输出类型**（`AccountsResult`），前端拿到的形状逐字段一致
//! ——那正是 §40「本地 = 不走 ssh 的远端」在这一格上的意思。
//!
//! # 🔴 `N-F1c`（2026-09-05）：读口**不再自己读磁盘**，改成问本机后端
//!
//! 用户拍的板逐字：「claude code 真实运行在哪台机器，他的账号就应该归哪台机器的后端管」。
//! ⇒ [`list_local_accounts`] 现在 exec 一次本机 sidecar 的 `--list-accounts`，
//! 把它吐的那几行交给 `accounts::parse_accounts_lines`（**与远端那条同一套解析、不同传输**），
//! 调用形状照同文件 [`list_local_session_accounts`] 那个先例，没有另造第二种调用法。
//!
//! 落地要先搬开两块石头，两块都在本轮搬掉了：
//! - daemon 那份 `is_safe_config_dir` 第一条是 `p.starts_with('/')` ⇒ Windows 的账号目录
//!   `C:\Users\…` 会被判成不安全、**列表恒空**。已按下面那一节的原话拆成两半。
//! - 开发树里**没有** sidecar（发版时才注入）⇒ 读口必须**诚实降级**：
//!   [`LocalAccountsOutcome`] 是三档 tagged 返回，「够不着」绝不许渲染成「你没有账号」。
//!
//! # 为什么当年是第三份实现（历史；那一段代码今天仍在，理由见 [`list_from_dir`]）
//!
//! `src/backend/observe/accounts_query.rs` 已经有一份完整的 Rust manifest 读取器
//! （直接读文件系统）。当年**复用不了**，理由是结构性的：
//!
//! - 那个 crate 是 **bin-only**（无 `[lib]`），且 `Cargo.toml` 注释写明**刻意不进 workspace**
//!   ——「a workspace would pull this Linux-only daemon into the Windows CI
//!   `cargo test --all` and break the build」。
//! - monitor **必须在 Windows 上构建**。让它依赖一个 Linux-only crate，正是那条注释在防的事。
//!
//! ⚠ `N-F1c` 绕开这条限制的办法**不是**去链接它，而是 **exec 它**：进程边界上没有 crate 依赖，
//! 所以那条注释挡的事一件都没发生。⇒ 「复用不了」在**链接**这个意义上今天仍然成立。
//!
//! ⇒ 于是这份数据有 **三个读者**：`cc-acct-iso`（bash，写侧）· daemon（读侧，两条路都走它）·
//! 本模块保留的那份参照实现（只被判据驱动）。
//! 这是**已知代价**，处置照本仓既有纪律：**双写点必须有守卫**
//!（同 `TMUX_LS_FMT` / 观测取值 / Z06 凭据文件名那几条）。见本文件测试模块里的
//! **U7-3 起四条契约常量与欺骗字符判据都住在共享 crate `acct-core`** ——
//! 两侧 import 同一份，漂移**不可表示**（此前靠一条读对面源文件的守卫发现，已退役）。
//!
//! # 与 daemon 那份**故意不同**的一处：路径绝对性判据
//!
//! daemon 的 `is_safe_config_dir` 第一条是 `p.starts_with('/')`。那条在 daemon 里是对的
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

/// manifest 读取上限（与 daemon 侧同值；账号数有限，8MB 是兜底不是预期）。
const MANIFEST_CAP: u64 = 8 * 1024 * 1024;

// 四条契约常量（账号库目录名 / manifest 文件名 / 凭据文件名 / schema 版本）
// U7-3 起住在 `acct-core`，见文件顶部的 `use`。**它们不再是双写点** ——
// 本模块与 daemon import 同一份，想不一致得先把 import 删掉。
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
// 实测搬之前两侧集合不同：daemon 缺 word joiner / 各类空白（**真洞**，那些 is_control 是 false）；
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

/// 去掉尾部分隔符，让不同来源写法能对上（daemon 侧同义）。
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
/// **绝不碰用户真实的 `~/.claude-accts`**。
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
///   还与 daemon 侧同名上限做**跨 crate 相等对拍**；
/// - `is_safe_config_dir` 是本仓「安全性质 / 平台形式」那条拆法的**参照实现** ——
///   `N-F1c` 就是照着它把 daemon 那份改对的；
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
        // 本实现原生认得「configDir 缺席 = 账号 0」⇒ 恒 true（不像旧 daemon 要降级提示）。
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
        // K-A1：分类与就绪各只有一处实现，都住 `acct-core` —— daemon 的
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

/// 本机账号清单的**三个结局**（`N-F1c` `NF1cD3`）。
///
/// | 结局 | 它是什么 | 界面据此说什么 |
/// |---|---|---|
/// | [`Self::Listed`] | 后端答出来了 | 列出来（零个账号也是一个**答案**） |
/// | [`Self::NoBackend`] | 这台机器上问不到 —— 二进制不在 / 起不来 | 说得出下一步 |
/// | [`Self::Unreadable`] | 后端在、也答了，但那份答案读不动 | 说得出坏在哪 |
///
/// # 为什么是三档而不是 `Result<_, String>`
///
/// 与传输那一层 `QueryOutcome` 同一条理由（它的模块头注逐字写过）：
/// 把「后端不在」与「查询失败了」压成一个 `Err`，就是让上层去猜。
/// 而本模块还多一格必须守的东西 ——
/// 🔴 **第二档绝不许渲染成「你没有账号」**：那是把「够不着」说成「没有」。
/// 开发树里 `externalBin` 现打零命中 ⇒ 那一档在本地是**恒真**的，
/// 一旦它退化成一张空表，用户看到的就是一句关于自己机器的假话。
#[derive(Debug)]
pub(crate) enum LocalAccountsOutcome {
    Listed {
        meta: AccountsMeta,
        accounts: Vec<RemoteAccount>,
    },
    NoBackend(String),
    Unreadable(String),
}

impl LocalAccountsOutcome {
    /// 这一档要说的那句话。**三档两两不同**由判据钉着，不是巧合。
    ///
    /// ⚠ 射程如实写：`Listed` 那一句今天**只进日志** —— 界面渲染的是列表本身。
    /// 所以「两两不同」这条判据买到的是「**两个失败档不会退化成同一句**」，
    /// 别把它读成「三档都在界面上说了话」。
    pub(crate) fn copy(&self) -> String {
        match self {
            Self::Listed { meta, accounts } => format!(
                "本机后端答出了 {} 个账号（清单 {}，启用={}）",
                accounts.len(),
                meta.manifest_path,
                meta.enabled
            ),
            Self::NoBackend(reason) => format!(
                "本机后端不在，问不出这台机器上有哪些账号：{reason}。\
                 ⚠ 这**不是**「你没有账号」—— 开发树里本来就没有这个二进制，\
                 它只在发版构建时才装进安装包；装好之后这一节就会把账号列出来。"
            ),
            Self::Unreadable(why) => format!(
                "本机后端答了，但那份账号数据读不动：{why}。\
                 多半是后端版本与界面对不上 —— 重装一次本机后端（或更新 cc-monitor）。"
            ),
        }
    }

    /// 折成前端那个形状。**两个失败档一律 `available:false` + `error`**，
    /// 而不是「空列表 + 成功」—— 后者正是 `NcM4` 那一刀要造的东西。
    pub(crate) fn into_result(self) -> AccountsResult {
        let copy = self.copy();
        match self {
            Self::Listed { meta, accounts } => AccountsResult {
                available: true,
                error: None,
                meta: Some(meta),
                accounts,
                // ⚠ 逐字节保持 `N-F1c` 之前的行为：这条路一直是 `None`。
                // `accounts::degraded_notice` **刻意不接进来** —— 它那两句话逐字都在说
                // 「远端 daemon / 在远端跑一次」，对一台本机来说有两个字是假的，
                // 而改那两句要动 `accounts.rs`（本件写区外）。登记为诚实边界，不是遗漏。
                notice: None,
            },
            Self::NoBackend(_) | Self::Unreadable(_) => AccountsResult {
                available: false,
                error: Some(copy),
                meta: None,
                accounts: Vec::new(),
                notice: None,
            },
        }
    }
}

/// 把一次 `--list-accounts` 的结局折成 [`LocalAccountsOutcome`]。
///
/// **纯函数**：不 spawn 进程、不碰文件系统 ⇒ 喂一份已知的假清单就能正面断言
/// 「答出来几个、名字是什么」（`NF1cD1` 的 acceptor 逐字要的那一格）。
pub(crate) fn classify_local_accounts(outcome: QueryOutcome) -> LocalAccountsOutcome {
    let stdout = match outcome {
        QueryOutcome::Ok(s) => s,
        QueryOutcome::NoBackend(reason) => return LocalAccountsOutcome::NoBackend(reason),
        QueryOutcome::Failed { code, stderr } => {
            return LocalAccountsOutcome::Unreadable(format!(
                "退出码 {code:?}；后端说：{}",
                stderr.trim()
            ))
        }
    };
    let lines: Vec<String> = stdout
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .collect();
    if lines.is_empty() {
        // 旧后端不认这个子命令时是 exit 2（走上面那支）；exit 0 却什么都没吐
        // 是另一种坏，分开说 —— 照 `list_remote_accounts` 那条同形的处理。
        return LocalAccountsOutcome::Unreadable("它一行都没吐（不认这个子命令？）".into());
    }
    // ★ 与远端那条**同一套解析、不同传输**（C1「一份代码两种承载」在这条查询上的落点）。
    let (meta, accounts) = crate::accounts::parse_accounts_lines(&lines);
    match meta {
        Some(meta) => LocalAccountsOutcome::Listed { meta, accounts },
        None => LocalAccountsOutcome::Unreadable(format!(
            "{} 行里没有一行是 accounts-meta（版本不匹配？）",
            lines.len()
        )),
    }
}

/// L3a：列出**本机**的账号 —— **问本机后端**（`N-F1c`）。
///
/// `list_remote_accounts` 的本地对侧，**输出类型完全相同**；从 `N-F1c` 起连
/// **数据源**也对上了：远端那条走 `ssh host <daemon> --list-accounts`，
/// 本机这条直接 exec 同一个二进制。调用形状照 [`list_local_session_accounts`]
/// 那个先例，没有另造第二种调用法。
///
/// 只读：不写任何文件、不读凭据内容（那两条现在由后端自己的只读铁律守着）。
/// ⚠ 它**起一次进程**了 —— 上一版那句「不起任何进程」从此不成立，这一行就是订正。
#[tauri::command]
pub async fn list_local_accounts() -> Result<AccountsResult, String> {
    // exec 是阻塞 IO，挪到阻塞线程池（与 `list_local_session_accounts` 同处理）。
    tokio::task::spawn_blocking(|| {
        classify_local_accounts(run_query(
            env!("CCM_TARGET_TRIPLE"),
            &["--list-accounts"],
            &*crate::spawn_managed::local_backend_one_shot_query(),
        ))
        .into_result()
    })
    .await
    .map_err(|e| format!("枚举本机账号失败：{e}"))
}

#[cfg(test)]
#[path = "../../../tests/bridge/local_accounts_tests.rs"]
mod tests;

// ─────────────────────────────────────────────────────────────────────────────
// E79：本机的「某个 sid 现在跑在哪个账号下」
// ─────────────────────────────────────────────────────────────────────────────

// 〔F10b 第二批·下半〕`MAX_LOCAL_SESSION_FILES` / `MAX_LOCAL_SESSION_FILE_BYTES` **已删** ——
// 它们是 daemon 侧同名上限的**第二份**（`accounts_query.rs::MAX_SESSION_FILES` 与
// `accounts_query.rs::MAX_SESSION_FILE_BYTES`，值逐字相同：
// 500 个文件 / 1 MiB）。唯一的用处随 `list_local_session_accounts` 改走 sidecar 一起消失
// ⇒ 留着就是「同一个数两处各写一份」（定框 §4）。上限现在只有一个家：daemon 那边，
// 且由它自己的测试与 `read_regular_capped` 钉着。

// 〔F10b 第二批〕`proc_claude_config_dir` 与 `pid_alive` **已删** ——
// 它们是 daemon 侧 `platform/proc.rs` 那两个（`:19` / `:80`）的**第二份实现**，
// 而本文件的头注原本就写着「判据与 daemon 侧逐字同源」。
// 唯一的调用方（`list_local_session_accounts`）已改走 sidecar 的 `--session-accounts`
// ⇒ 留着就是「同一件事两处各写一份」（定框 §4），且平台原语该住 `platform/`（C10）。
// ⚠ 不是「暂时没人用就删」（铁律 13 禁的那种）：它们没有判据、没有测试、
//   也不是任何东西的唯一锚点 —— 语义的家在 daemon 那边，且由它自己的测试钉着。

/// E79：**本机**版的「某会话跑在哪个账号下」——`--session-accounts` 的对侧实现。
///
/// # F10b 第二批：**它不再自己读 `~/.claude` 与 `/proc`**（C1 / C7）
///
/// 从前这里自己 `resolve_claude_dir()` + 读 `sessions/` 的 pidfile + 从
/// `/proc/<pid>/environ` 抠 `CLAUDE_CONFIG_DIR`。现在**问本机后端**：
/// exec 一次 sidecar `--session-accounts`，逐行 JSON 反序列化成 [`crate::accounts::SessionAccount`]。
///
/// ★ 这一迁把 **C1「一份代码、两种承载」在这条查询上做实了**：
/// 本函数与 `accounts::list_remote_session_accounts` 现在是**同一套解析、不同传输** ——
/// 远端那条走 `ssh host <daemon> --session-accounts`，本机这条直接 exec 同一个二进制。
/// 类型（`SessionAccount` / `SessionAccountsResult`）与逐行跳过坏行的做法都照它抄，没新造。
///
/// # 平台差异搬到了该管它的那一侧
///
/// 「Linux 有 `/proc`、Windows 没有」这件事**从此由 daemon 回答**，不再在 monitor 里判
/// （从前这里有一句 `if !cfg!(target_os = "linux")`）。daemon 侧读 `/proc/<pid>/environ`
/// 的那段在 `platform/proc.rs`，而 `platform/fallback_guard` 逐字禁止非目标平台的分支
/// 凭空返回成功值 ⇒ Windows 上它诚实说「观测不到」而不是伪造空表。
/// ⚠ **如实说**：本轮**没有**在真 Windows 上跑过这条（F05b 那次真机验的是 `--list-accounts`
/// 与流模式）⇒ Windows 行为是**从那条护栏推出来的**，不是实测。已进 `ROADMAP §5`。
///
/// # 边界（`available:false` + `error` 这个形状本来就是为这类事准备的）
///
/// - **sidecar 不在**（开发树）⇒ `available:false` + 「本机后端不在…（找过哪些路径）」。
/// - **查询失败** ⇒ `available:false` + 退出码与 stderr 原样带出（定框 §5：诚实降级）。
/// - 零行是合法的（本机没有活会话）—— 同远端那条的判断，不额外区分「旧 daemon」。
/// - ⚠ 只抠**两个写死的键**（`CLAUDE_CONFIG_DIR` 与 `CCM_LAUNCH_ID`）、`configDir` 过白名单
///   —— 那两条现在由 daemon 侧守（它的 `observe/accounts_query.rs` 头注逐字写着同一套边界）。
///   🔴 **第二个键是 `K-P5f` 加的**（身份 token 读回来那一侧）；这句话原先逐字写着
///   「只抠 `CLAUDE_CONFIG_DIR` **一个键**」，**不同拍改它就是在盘上留一句假话** ——
///   而它这一处**没有任何机检看着**（路② 撞 0 道机检，`K-P5f §7 二㈡` 现打），
///   全靠人记得来改。同族病史见 `K-P5c §7 上报-3`「写着有、其实没有」。
///   键名**不是参数**（daemon 侧两个常量），所以「两个键」与「整个环境快照」的界没有松动。
#[tauri::command]
pub async fn list_local_session_accounts() -> Result<crate::accounts::SessionAccountsResult, String>
{
    use crate::backend::observe::local_query::{run_query, QueryOutcome};
    tokio::task::spawn_blocking(|| {
        let unavailable = |msg: String| crate::accounts::SessionAccountsResult {
            available: false,
            error: Some(msg),
            sessions: Vec::new(),
        };
        let stdout = match run_query(
            env!("CCM_TARGET_TRIPLE"),
            &["--session-accounts"],
            &*crate::spawn_managed::local_backend_one_shot_query(),
        ) {
            QueryOutcome::Ok(s) => s,
            QueryOutcome::NoBackend(reason) => {
                return unavailable(format!("本机后端不在，查不出会话属于哪个账号：{reason}"));
            }
            QueryOutcome::Failed { code, stderr } => {
                return unavailable(format!(
                    "本机后端的会话账号查询失败（退出码 {code:?}）：{}",
                    stderr.trim()
                ));
            }
        };
        let mut sessions = Vec::new();
        for line in stdout.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            match serde_json::from_str::<crate::accounts::SessionAccount>(line) {
                Ok(s) => sessions.push(s),
                // 单行坏了不该毁掉整次查询（照远端那条的做法）。
                Err(e) => tracing::warn!("本机 session-accounts 行解析失败（跳过）: {e}"),
            }
        }
        crate::accounts::SessionAccountsResult {
            available: true,
            error: None,
            sessions,
        }
    })
    .await
    .map_err(|e| format!("list_local_session_accounts join 失败: {e}"))
}
