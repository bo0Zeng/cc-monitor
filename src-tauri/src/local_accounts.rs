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
//! `remote-daemon-proto/src/observe/accounts_query.rs` 已经有一份完整的 Rust manifest 读取器
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
use crate::backend::control::local_query::{run_query, QueryOutcome};
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
        classify_local_accounts(run_query(env!("CCM_TARGET_TRIPLE"), &["--list-accounts"]))
            .into_result()
    })
    .await
    .map_err(|e| format!("枚举本机账号失败：{e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    /// 每个测试独占的临时目录。仓库约定不引 `tempfile`，用 pid + 计数器保唯一。
    /// **测试绝不碰用户真实的 `~/.claude-accts`** —— 全部在这里面。
    struct Sandbox(PathBuf);
    static SEQ: AtomicU64 = AtomicU64::new(0);
    impl Sandbox {
        fn new() -> Self {
            let p = std::env::temp_dir().join(format!(
                "l3a-{}-{}",
                std::process::id(),
                SEQ.fetch_add(1, Ordering::SeqCst)
            ));
            std::fs::create_dir_all(&p).expect("mkdir sandbox");
            Sandbox(p)
        }
        /// 写 manifest。**文件名写死成字面量，刻意不用 `MANIFEST_NAME`。**
        ///
        /// U7-4：此前这里是 `self.0.join(MANIFEST_NAME)` —— 测试的**写侧**与生产的**读侧**
        /// 用同一个常量，常量一起变，测试**结构上不可能因为它变了而失败**。
        /// U7-3 实测：把内核里的 `MANIFEST_NAME` 改成 `"accts.json"`，daemon 红了 9 条，
        /// monitor **全绿**。那不是「没测到」，是「测不到」。
        ///
        /// 常量是**实现**，文件名是**契约**（bash 写侧 / daemon / 本机三方共用）。
        /// 测试该钉契约，所以这里用字面量。
        fn write_manifest(&self, json: &str) {
            std::fs::write(self.0.join("accounts.json"), json).expect("write manifest");
        }
    }
    impl Drop for Sandbox {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// 〔audit-0805 08-06〕**`read_capped` 的三种失败必须仍然分得开**，外加上限的边界语义。
    ///
    /// # 为什么这条值得钉
    ///
    /// `ROADMAP §5` 的 3j 复核结论逐字是：这三种（文件不存在 / 不是普通文件 / 过大）
    /// 走的是同一个 `Err` 臂，**但理由串是分得开的**，而设置面板真的把它渲染出来
    /// （`renderNotEnabled` 那行「原因：…」）。⇒ 那句「分得开」**此前没有任何判据读它** ——
    /// 谁把三条消息合成一句「读不了」，前端就退回一个没有身份的失败（E4 要治的正是这个），
    /// 而**没有东西会红**。
    ///
    /// 顺带钉上限的边界：`meta.len() > cap` ⇒ **正好等于 cap 是放行的**。
    /// E5 要求「上限与超限语义成对定义」，而 `>` 与 `>=` 的差别正是这一对里最容易滑的一格。
    ///
    /// ⚠ 全程在 `Sandbox` 的临时目录里，**绝不碰用户真实的 `~/.claude-accts`**（同本文件既有约定）。
    #[test]
    fn read_capped_keeps_its_three_failures_distinguishable() {
        let sb = Sandbox::new();

        // ① 不存在：理由里要有路径，且**不能**冒充另外两种。
        let missing = sb.0.join("nope.json");
        let e = read_capped(&missing, 1024).expect_err("不存在的文件必须是 Err");
        assert!(
            e.contains("nope.json"),
            "理由里没有路径，用户看不出是哪一个：{e}"
        );
        assert!(
            !e.contains("不是普通文件") && !e.contains("过大"),
            "「不存在」被说成了另一种失败：{e}"
        );

        // ② 不是普通文件（目录）。
        let dir = sb.0.join("adir");
        std::fs::create_dir_all(&dir).expect("mkdir");
        let e = read_capped(&dir, 1024).expect_err("目录必须是 Err");
        assert!(e.contains("不是普通文件"), "目录没有得到自己那条理由：{e}");

        // ③ 过大：理由里要带**实际字节数**（不然用户不知道差多少）。
        let big = sb.0.join("big.json");
        std::fs::write(&big, vec![b'x'; 10]).expect("write");
        let e = read_capped(&big, 4).expect_err("超限必须是 Err");
        assert!(e.contains("过大"), "超限没有得到自己那条理由：{e}");
        assert!(e.contains("10"), "理由里没带实际字节数：{e}");

        // ④ 三条理由**两两不同** —— 合并成一句就在这里红。
        let e_missing = read_capped(&missing, 1024).unwrap_err();
        let e_dir = read_capped(&dir, 1024).unwrap_err();
        let e_big = read_capped(&big, 4).unwrap_err();
        assert!(
            e_missing != e_dir && e_dir != e_big && e_missing != e_big,
            "三种失败给了相同的理由串，前端只能显示一个没有身份的「读不了」：\n               不存在={e_missing}\n  目录={e_dir}\n  过大={e_big}"
        );

        // ⑤ 边界：正好等于上限**放行**（`>` 不是 `>=`）；差一个字节就拒。
        let exact = sb.0.join("exact.json");
        std::fs::write(&exact, vec![b'y'; 8]).expect("write");
        assert_eq!(
            read_capped(&exact, 8).expect("正好等于上限应当放行"),
            vec![b'y'; 8],
            "读回来的内容与写进去的不一致"
        );
        assert!(
            read_capped(&exact, 7).is_err(),
            "超出一个字节没被拒 —— 上限那一格滑了"
        );
    }

    #[test]
    fn no_manifest_is_not_an_error_just_disabled() {
        let sb = Sandbox::new();
        let r = list_from_dir(&sb.0);
        assert!(r.available, "「本机没启用多账号」是正常状态，不是能力缺失");
        assert!(!r.meta.as_ref().unwrap().enabled);
        assert!(r.meta.as_ref().unwrap().error.is_some(), "要给出人话原因");
        assert!(r.accounts.is_empty());
    }

    #[test]
    fn bad_json_and_bad_schema_are_reported_not_panicked() {
        let sb = Sandbox::new();
        sb.write_manifest("{ not json");
        assert!(!list_from_dir(&sb.0).meta.unwrap().enabled);
        sb.write_manifest(r#"{"version":99,"accounts":[]}"#);
        let m = list_from_dir(&sb.0).meta.unwrap();
        assert!(!m.enabled);
        assert!(m.error.unwrap().contains("99"));
    }

    /// ★ Z01：**`configDir` 键缺席 = 账号 0**，判据是结构性的，不认名字。
    #[test]
    fn account_zero_is_the_absent_key_not_a_name_and_not_an_empty_string() {
        let sb = Sandbox::new();
        let shared = sb.0.join("shared");
        std::fs::create_dir_all(&shared).unwrap();
        std::fs::write(shared.join(CREDENTIALS_NAME), "{}").unwrap();
        sb.write_manifest(&format!(
            r#"{{"version":1,"sharedStore":{shared:?},"accounts":[
                 {{"name":"zero","mode":"bare"}},
                 {{"name":"empty","configDir":""}},
                 {{"name":"named-0","configDir":{cfg:?}}}
               ]}}"#,
            shared = shared.to_string_lossy(),
            cfg = sb.0.join("acct-a").to_string_lossy(),
        ));
        let r = list_from_dir(&sb.0);
        let names: Vec<&str> = r.accounts.iter().map(|a| a.name.as_str()).collect();
        assert_eq!(
            names,
            vec!["zero", "named-0"],
            "空串那条应被丢弃：空值 ≠ 未设"
        );

        let zero = &r.accounts[0];
        assert!(zero.config_dir.is_none(), "账号 0 对外出 None，不是空串");
        assert!(zero.exists, "「裸起」这个状态永远可达");
        assert!(zero.logged_in, "账号 0 的登录态查共享库");
        // 名字叫什么都行——判据是键在不在，不是名字。
        assert_eq!(zero.name, "zero");
    }

    #[test]
    fn logged_in_is_existence_only_and_unknown_is_false() {
        let sb = Sandbox::new();
        let a = sb.0.join("acct-a");
        std::fs::create_dir_all(&a).unwrap();
        sb.write_manifest(&format!(
            r#"{{"version":1,"accounts":[
                 {{"name":"a","configDir":{a:?}}},
                 {{"name":"gone","configDir":{gone:?}}},
                 {{"name":"zero"}}
               ]}}"#,
            a = a.to_string_lossy(),
            gone = sb.0.join("nope").to_string_lossy(),
        ));
        let r = list_from_dir(&sb.0);
        assert!(!r.accounts[0].logged_in, "目录在但没有凭据文件 ⇒ false");
        assert!(r.accounts[0].exists);
        assert!(!r.accounts[1].exists, "目录不在 ⇒ exists false");
        // 账号 0 且 manifest 没写 sharedStore ⇒ 探不到 ⇒ false（「不知道」，不假装已登录）
        assert!(!r.accounts[2].logged_in);

        // 同 `write_manifest`：文件名写死成字面量，刻意不用 `CREDENTIALS_NAME`。
        // 用常量的话，测试写哪个文件、生产找哪个文件会一起变 ⇒ 测不出常量漂移。
        std::fs::write(a.join(".credentials.json"), "{}").unwrap();
        assert!(list_from_dir(&sb.0).accounts[0].logged_in);
    }

    #[test]
    fn unsafe_config_dirs_are_dropped_not_fatal() {
        let sb = Sandbox::new();
        for bad in [
            "relative/path",
            "/",
            "/a/../b",
            "/a/b;id",
            "/a/$(id)",
            "/a/\u{202E}b",
            "",
        ] {
            assert!(!is_safe_config_dir(bad), "应判不安全: {bad:?}");
        }
        for good in [
            "/home/u/.claude-accts/a",
            "C:\\Users\\u\\accts\\a",
            "\\\\srv\\share\\a",
        ] {
            assert!(is_safe_config_dir(good), "应判安全: {good:?}");
        }
        // 坏条目只丢自己，不拖垮整表。
        sb.write_manifest(
            r#"{"version":1,"accounts":[{"name":"bad","configDir":"rel"},{"name":"zero"}]}"#,
        );
        let r = list_from_dir(&sb.0);
        assert_eq!(r.accounts.len(), 1);
        assert_eq!(r.accounts[0].name, "zero");
    }

    /// ★ U7-4：**欺骗字符的覆盖面**，逐组各取一个代表。
    ///
    /// # 这条为什么单独立一件事做
    ///
    /// U7-3 把 `is_deceptive_char` 抽进 `acct-core` 时做变异验证：
    /// 删掉内核里的 NEL（`U+0085`）⇒ acct-core 红、daemon 红、**monitor 全绿**。
    /// 当时如实登记了原因 —— 不是接线没生效，是本模块**只测过 `U+202E` 一个码位**，
    /// 而那恰好是两侧本来都有的。
    ///
    /// 我刻意**没在那次重构里顺手补** —— 补测试要单独设计，
    /// 混在重构里做等于用新写的测试给新写的代码背书。
    ///
    /// # 判据：按**来源分组**取代表，不是堆码位
    ///
    /// 每组各一个，任何一组从内核里掉出去，本测试立刻红：
    #[test]
    fn every_group_of_deceptive_characters_is_rejected_in_a_config_dir() {
        // (码位, 这一组是什么, U7-3 之前谁缺它)
        let groups: &[(char, &str, &str)] = &[
            // ⚠ NEL 属 Cc 类，`is_control()` 本来就挡着它 ⇒ 把它从内核集合里删掉
            // **不会**让本测试红。U7-3 我曾把「本机缺 NEL」当安全洞报出来，U7-4 实测证伪：
            // 集合确实差过一项，可观察行为没差。留在表里是为了这条注记本身。
            (
                '\u{0085}',
                "NEL（C1 换行；is_control 已覆盖，非真洞）",
                "集合差过、行为没差",
            ),
            ('\u{00A0}', "NBSP", "两侧都有"),
            ('\u{1680}', "Ogham space mark", "daemon 缺"),
            ('\u{2003}', "各类空格（U+2000..200A）", "daemon 缺"),
            ('\u{200B}', "零宽空格/连接符", "两侧都有"),
            ('\u{2028}', "行分隔", "两侧都有"),
            ('\u{202E}', "双向覆盖（RLO）", "两侧都有"),
            ('\u{202F}', "narrow NBSP", "daemon 缺"),
            ('\u{205F}', "medium mathematical space", "daemon 缺"),
            ('\u{2060}', "word joiner / 不可见运算符", "daemon 缺"),
            ('\u{2066}', "双向隔离", "两侧都有"),
            ('\u{3000}', "ideographic space", "daemon 缺"),
            ('\u{FEFF}', "ZWNBSP / BOM", "两侧都有"),
        ];
        assert!(
            groups.len() >= 13,
            "分组表被削短了（{} 组）—— 本断言在空转",
            groups.len()
        );
        for (c, what, who) in groups {
            let path = format!("/home/u/.claude-accts/a{c}b");
            assert!(
                !is_safe_config_dir(&path),
                "U+{:04X}（{what}；U7-3 之前{who}）没被挡下 —— \n\
                 它能在 UI 里把账号名/路径伪造成另一个样子。",
                *c as u32
            );
        }
        // 反向：去掉欺骗字符之后同一条路径必须**通过**，否则上面全是空转。
        assert!(
            is_safe_config_dir("/home/u/.claude-accts/ab"),
            "干净路径被误判成不安全 —— 上面那些断言全都不算数了"
        );
    }

    /// ★ U7-4：账号库目录名是**契约**，写死成字面量核对。
    ///
    /// `local_accts_dir()` 拼的是 `$HOME/<ACCTS_DIR_NAME>`。此前没有任何测试碰它 ——
    /// 常量改了、本机就去别处找账号库，而 UI 上的表现只是「一个账号都没有」。
    #[test]
    fn the_accounts_library_lives_under_the_contract_directory_name() {
        let d = local_accts_dir().expect("取不到 HOME —— 本断言在空转");
        assert_eq!(
            d.file_name().and_then(|s| s.to_str()),
            Some(".claude-accts"),
            "账号库目录名变了。这是 bash 写侧 / daemon / 本机三方共用的契约名，\n\
             改了它本机就去别处找账号库，UI 上只表现为「一个账号都没有」。"
        );
    }

    // U7-3：**那条读对面源文件的跨 crate 契约守卫已退役。**
    //
    // 它是真的（剥注释、剥测试段、有字节地板与锚点自检，注释里还记着第一版是安慰剂、
    // 被变异证伪后修好）—— 但守卫只能**发现**漂移。四个常量与 `is_deceptive_char`
    // 现在都住在共享 crate `acct-core`，两侧 import 同一份 ⇒ 漂移**不可表示**，
    // 想不一致得先把 import 删掉。
    //
    // 这是 U6b-3 那条横切约定的又一次应用：判据能被绕过时，先问「能不能让它不可表示」。
    // 不可表示之后，判据本身是死重量。
    //
    // ⚠ **没有一并合掉的两个同名函数**：`is_safe_config_dir` 与 `norm_dir`。
    //
    // 🔴 **`N-F1c`（09-05）订正上半句的理由**：那句话原先逐字写着
    // 「本机侧要认 Windows 盘符（`looks_absolute`）且必须允许 `\` 作分隔符，
    //  所以改成拒 `\..\`；daemon 是 Linux-only，直接把 `\` 当危险字符拒掉。
    //  硬合只能二选一：要么本机失去 Windows 路径，要么 daemon 失去对 `\` 的拒绝。」
    // —— **后半段今天不成立了**：本机读口改问后端之后，那个「Linux-only」的前提没了
    // （同一个二进制要在 Windows 上答本机的账号），于是 daemon 那份**也**按同一句话
    // 拆成了「安全性质 + 平台形式」，两边的字符集现在逐字相同。
    // ⇒ 「二选一」那个两难是**假的**：它假设了 daemon 只跑 Linux。
    //
    // ⚠ 那**仍然不等于该合**（两条理由，都还硬着）：
    //   ① daemon crate 是 bin-only、刻意不进 workspace，import 不了 `acct-core` 之外的东西
    //      —— 而这两个函数要合就得先有个共同的家，`acct-core` 是唯一候选；
    //   ② `norm_dir` 两侧仍然真的不同（本机多剥一层 `\`），合它是另一件事。
    // ⇒ 合并条件写清：把这两个函数搬进 `acct-core`（连同它们的判据），两侧 import 同一份。
    // ⚠ **同一句订正在 `acct-core` 的模块头注里还有一份没改** —— 那份文件不在 `N-F1c`
    //   的写区里（本件只许动 daemon 那一个函数），已按诚实边界交回 PM。

    // ---- K-A1：鉴权方式这一维（生产者②） ----

    /// ★ **跨生产者对拍，本机这一半。**
    ///
    /// 喂的是 `acct_core::auth_kind_parity_manifest`（daemon 那半喂的是**同一个函数**
    /// 的输出），断的是 `acct_core::AUTH_KIND_PARITY_CASES` 里手写的金样。
    /// ⇒ 两个生产者里任意一个自己填一个默认值，它那半当场红。
    /// daemon 那半住 `remote-daemon-proto/src/observe/accounts_query.rs::
    /// tests::auth_kind_parity_daemon_side`。
    ///
    /// ⚠ 射程如实写（`KA6c`）：**只覆盖 `authKind` / `authReady` 这一维**。
    /// 其余 6 个字段今天仍是两份实现各写一遍，这条对拍看不见它们漂。
    #[test]
    fn auth_kind_parity_local_side() {
        let sb = Sandbox::new();
        for c in &acct_core::AUTH_KIND_PARITY_CASES {
            let d = sb.0.join(c.name);
            std::fs::create_dir_all(&d).unwrap();
            if c.credentials_present {
                // 同 `write_manifest`：文件名刻意写死，别用常量（否则测不出常量漂移）。
                std::fs::write(d.join(".credentials.json"), "{}").unwrap();
            }
        }
        std::fs::create_dir_all(sb.0.join("shared")).unwrap();
        sb.write_manifest(&acct_core::auth_kind_parity_manifest(
            &sb.0.to_string_lossy(),
        ));
        let r = list_from_dir(&sb.0);
        assert_eq!(
            r.accounts.len(),
            acct_core::AUTH_KIND_PARITY_CASES.len(),
            "账号数对不上，逐格断言会漏掉没出来的那几个：{:?}",
            r.accounts.iter().map(|a| &a.name).collect::<Vec<_>>()
        );
        for (i, c) in acct_core::AUTH_KIND_PARITY_CASES.iter().enumerate() {
            let a = &r.accounts[i];
            assert_eq!(a.name, c.name, "顺序变了，下面几格就对错人了");
            assert_eq!(
                a.auth_kind.map(|k| k.as_contract_str()),
                Some(c.expect_auth_kind),
                "{}：本机产出的 authKind 与金样不一致",
                c.name
            );
            assert_eq!(
                a.auth_ready,
                Some(c.expect_auth_ready),
                "{}：本机产出的 authReady 与金样不一致",
                c.name
            );
            // `loggedIn` 逐字节旧语义：仍然只是「凭据文件在不在」。
            assert_eq!(
                a.logged_in, c.credentials_present,
                "{}：loggedIn 的语义被这次改动动了（它该只是 stat 结果）",
                c.name
            );
        }
    }

    /// ★ `KAY4` 的 **Rust 侧那一格**（那条零命中守卫是 vitest，扫不到这里）。
    ///
    /// 守的性质：本文件里这一维**不许有第二条计算路径** —— `auth_ready` 只许来自
    /// `acct_core::auth_ready(`，`authKind` 只许来自 `AuthKind::from_manifest(`。
    /// 有人在这儿手写 `if kind == AuthKind::ApiKey { true } else { … }`，本条红。
    ///
    /// ⚠ 射程如实写：**只管本文件**（daemon 那份由它自己那条同名判据守），
    /// 而且是**字面量扫描** —— 把 helper 重新 `use` 成别名就绕得过去。
    /// 真正的地板不是它，是 `acct-core` 里只有一份实现。
    #[test]
    fn the_auth_dimension_has_exactly_one_computation_path() {
        let me = include_str!("local_accounts.rs");
        assert!(
            me.len() > 20_000,
            "include_str! 没读到源码，本条在空转（实得 {} 字节）",
            me.len()
        );
        let cut = me
            .find("#[cfg(test)]")
            .expect("找不到 #[cfg(test)] 锚点 —— 切法失效了");
        // **去注释口径**：本文件的注释里就在解释这一维，按裸文本数会把散文也数进来。
        let prod: String = me[..cut]
            .lines()
            .filter(|l| {
                let t = l.trim_start();
                !t.starts_with("//")
            })
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            prod.contains("fn list_from_dir(accts_dir: &Path)") && prod.len() > 4_000,
            "剥注释剥过头了（实得 {} 字节）—— 下面几条会零命中地绿",
            prod.len()
        );
        assert_eq!(
            prod.matches("auth_ready(").count(),
            1,
            "生产段里 `auth_ready(` 出现了不止一次 —— 要么有了第二条计算路径，要么该收进 acct-core"
        );
        assert_eq!(
            prod.matches("AuthKind::").count(),
            1,
            "生产段里出现了不止一处 `AuthKind::` —— 分类只许经 `AuthKind::from_manifest`"
        );
        assert_eq!(
            prod.matches("ApiKey").count(),
            0,
            "生产段里出现了 `ApiKey` —— 按 kind 分流的规则只许住 acct-core"
        );
    }

    // ---- `N-F1c`：读口改问本机后端 ----

    /// daemon `--list-accounts` 出参的一份**已知假清单**：首行 meta，其后每账号一行。
    ///
    /// ⚠ 逐字写死成字面量、**不用**任何生产常量拼 —— 同本文件 `write_manifest`
    /// 那条纪律：测试该钉契约，用生产常量拼的话常量一起变、判据结构上不可能红。
    fn fake_listing() -> String {
        [
            r#"{"kind":"accounts-meta","enabled":true,"acctsDir":"/h/lib","manifestPath":"/h/lib/accounts.json","updatedAt":"2026-09-05T00:00:00Z","sharedStore":"/h/shared","count":3,"error":null,"accountZeroAware":true}"#,
            r#"{"name":"zero","email":"","configDir":null,"isDefault":false,"mode":"isolated","exists":true,"loggedIn":true}"#,
            r#"{"name":"alice","email":"a@x.edu","configDir":"/h/lib/alice","isDefault":true,"mode":"isolated","exists":true,"loggedIn":true}"#,
            r#"{"name":"bob","email":"","configDir":"/h/lib/bob","isDefault":false,"mode":"isolated","exists":false,"loggedIn":false}"#,
        ]
        .join("\n")
    }

    /// ★★ `NF1cD1` 的**正面读数** —— 只断「调用了 `run_query`」不算数。
    ///
    /// 喂一份**已知的假清单**，断它答出几个、名字是什么、字段有没有在这一跳里丢掉。
    ///
    /// ⚠ 它买不到什么，如实写：**不证明后端真的会这么答**（那要真 sidecar，
    /// 属 e2e；开发树里 `externalBin` 现打零命中，`nc2` 已登记）。
    /// 它证明的是**这一跳的解析与折叠是对的**，而 `NcM1` 那一刀（读口改回直读磁盘）
    /// 会让它当场红 —— 直读磁盘那条路根本不看这份 stdout。
    #[test]
    fn a_known_fake_listing_comes_back_with_the_right_accounts() {
        let (meta, accounts) = match classify_local_accounts(QueryOutcome::Ok(fake_listing())) {
            LocalAccountsOutcome::Listed { meta, accounts } => (meta, accounts),
            other => panic!("喂了一份合法清单，却没落到 Listed 那一档：{other:?}"),
        };
        assert_eq!(accounts.len(), 3, "答出来的账号数不对");
        assert_eq!(
            accounts.iter().map(|a| a.name.as_str()).collect::<Vec<_>>(),
            vec!["zero", "alice", "bob"],
            "名字或顺序不对 —— 顺序是承重的（界面按后端给的次序排）"
        );
        assert_eq!(meta.count, 3);
        assert!(meta.enabled);
        assert_eq!(meta.shared_store.as_deref(), Some("/h/shared"));
        assert_eq!(meta.manifest_path, "/h/lib/accounts.json");
        assert!(
            meta.account_zero_aware,
            "后端说了它认账号 0，这一跳不许把这一位丢掉（丢了界面会多喊一句降级）"
        );
        // 账号 0：`configDir` 是 null ⇒ 下游据此「不注入」。空串**不算**缺席，
        // 所以这一跳不许把 null 折成 `Some(\"\")`。
        assert!(
            accounts[0].config_dir.is_none(),
            "账号 0 的 configDir 被改写了：{:?}",
            accounts[0].config_dir
        );
        assert_eq!(accounts[1].config_dir.as_deref(), Some("/h/lib/alice"));
        assert!(accounts[1].is_default);
        assert_eq!(accounts[1].email, "a@x.edu");
        assert!(!accounts[2].exists, "bob 的目录不存在，这一位要如实带过来");

        // 折成前端那个形状之后仍然是「答出来了」。
        let r = classify_local_accounts(QueryOutcome::Ok(fake_listing())).into_result();
        assert!(r.available && r.error.is_none());
        assert_eq!(r.accounts.len(), 3);
        assert_eq!(r.meta.expect("meta 丢了").count, 3);
    }

    /// ★★ `NF1cD3`：**三个结局在类型上分得开，三句话两两不同。**
    ///
    /// 形状照 `daemon_policy` 那族「四条文案两两不同」——
    /// 判定不是「源码里出现了三个枚举名」，是**说出来的那三句话真的不一样**。
    #[test]
    fn the_three_endings_are_told_apart_and_say_different_things() {
        const EMPTY_ANSWER: &str = r#"{"kind":"accounts-meta","enabled":false,"acctsDir":"/h/lib","manifestPath":"/h/lib/accounts.json","updatedAt":null,"sharedStore":null,"count":0,"error":"清单不可读","accountZeroAware":true}"#;
        let listed = classify_local_accounts(QueryOutcome::Ok(EMPTY_ANSWER.into()));
        let no_backend = classify_local_accounts(QueryOutcome::NoBackend(
            "sidecar 不在旁边；找过 [\"/opt/x\"]".into(),
        ));
        let unreadable = classify_local_accounts(QueryOutcome::Failed {
            code: Some(2),
            stderr: "unknown argument\n".into(),
        });

        // ① 类型上就分得开（不靠读字符串）。
        assert!(
            matches!(listed, LocalAccountsOutcome::Listed { .. }),
            "后端答了一份 meta，却没落到 Listed：{listed:?}"
        );
        assert!(
            matches!(no_backend, LocalAccountsOutcome::NoBackend(_)),
            "「后端不在」落错档：{no_backend:?}"
        );
        assert!(
            matches!(unreadable, LocalAccountsOutcome::Unreadable(_)),
            "「答了但读不动」落错档：{unreadable:?}"
        );

        // ② 三句话**两两不同**，且每一句都说得下去（掏空成一个短语，上面那条照样绿）。
        let copies = [listed.copy(), no_backend.copy(), unreadable.copy()];
        for i in 0..copies.len() {
            for j in (i + 1)..copies.len() {
                assert_ne!(
                    copies[i], copies[j],
                    "第 {i} 档与第 {j} 档说的是同一句话 —— 那两格在用户眼里就没分开"
                );
            }
        }
        for (n, c) in copies.iter().enumerate() {
            assert!(
                c.chars().count() >= 30,
                "第 {n} 档只有 {} 个字 —— 一句说不出原因与下一步的话等于只给了个名字",
                c.chars().count()
            );
        }
        // 「后端不在」那一句必须**明说它不是「你没有账号」**，并说得出下一步。
        assert!(
            copies[1].contains("不是") && copies[1].contains("发版"),
            "「后端不在」那句话没把「够不着 ≠ 没有」说出来，也没给下一步：{}",
            copies[1]
        );

        // ③ 🔴 `NcM4` 那一刀的落点：**「后端不在」与「后端说这里有 0 个账号」必须分得开。**
        //    后者是**答案**（`enabled:false` + 原因），前者是**够不着**。
        let answer = classify_local_accounts(QueryOutcome::Ok(EMPTY_ANSWER.into())).into_result();
        let cannot_reach =
            classify_local_accounts(QueryOutcome::NoBackend("x".into())).into_result();
        assert!(
            answer.available && answer.error.is_none() && answer.meta.is_some(),
            "「后端答了、只是这台机没启用多账号」被折成了不可用"
        );
        assert!(answer.accounts.is_empty());
        assert!(
            !cannot_reach.available,
            "「后端不在」被折成 available:true —— 那就是把「够不着」渲染成「你没有账号」"
        );
        assert!(cannot_reach.error.is_some(), "够不着的时候必须说得出理由");
        assert!(
            cannot_reach.meta.is_none(),
            "够不着的时候不许伪造一份 meta —— 界面会拿它的 count 去渲染一张空表"
        );
        assert_ne!(
            (
                answer.available,
                answer.error.is_some(),
                answer.meta.is_some()
            ),
            (
                cannot_reach.available,
                cannot_reach.error.is_some(),
                cannot_reach.meta.is_some()
            ),
            "两种「零个账号」在前端拿到的形状上一模一样 —— 界面就只能猜"
        );
    }

    /// ★★ 「代码还在但走不到」那一族在**本件读口**上的落点。
    ///
    /// 形状照 `a_short_circuit_cannot_fake_the_honest_degrade`（`local_query.rs` 里那条）：
    /// 单测环境**没有** sidecar ⇒ 真走了那条路才拿得到**带理由**的诚实降级；
    /// 被短路（比如 `return Ok(Default::default())`）给出的是「空但成功」——
    /// 而扫源码的守卫看不见那种错，调用那行文字还在。
    ///
    /// ⚠ 它证明的是「调用发生了、且拿到了后端不在的答复」，
    /// **不证明** happy path 正确（那一格由上面那条正面读数与将来的 e2e 分担）。
    #[tokio::test]
    async fn the_read_port_really_asks_the_backend() {
        // 前提自检：本环境必须没有 sidecar，否则下面几条会走 happy path 而空转。
        let probe = run_query(env!("CCM_TARGET_TRIPLE"), &["--list-accounts"]);
        assert!(
            matches!(probe, QueryOutcome::NoBackend(_)),
            "测试环境里居然找得到 sidecar —— 本条的前提不成立，下面几条会空转。\n\
             （若哪天单测环境真带 sidecar，本条要改成显式指一个不存在的 target triple）"
        );
        let r = list_local_accounts()
            .await
            .expect("这条路的诚实降级是 Ok(available=false)，不该是 Err");
        assert!(!r.available, "没有 sidecar 却报 available=true");
        let why = r.error.expect(
            "`list_local_accounts` 返回了「空但成功」——\n\
             没有 sidecar 时它**必须说出理由**（定框 §5：tagged 返回 + reason）。\n\
             ⇒ 拿不出 reason 就意味着那条查询根本没发生（被短路了）。",
        );
        assert!(
            why.contains("本机后端不在"),
            "理由没说清是「后端不在」还是别的：{why}"
        );
        assert!(r.accounts.is_empty() && r.meta.is_none());
    }
}

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
    use crate::backend::control::local_query::{run_query, QueryOutcome};
    tokio::task::spawn_blocking(|| {
        let unavailable = |msg: String| crate::accounts::SessionAccountsResult {
            available: false,
            error: Some(msg),
            sessions: Vec::new(),
        };
        let stdout = match run_query(env!("CCM_TARGET_TRIPLE"), &["--session-accounts"]) {
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
