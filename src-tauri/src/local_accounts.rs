//! L3a（local-as-remote）：**本机**多账号枚举 —— 只读，直接读 manifest。
//!
//! `accounts.rs` 是这件事的**远端**那半（把 daemon 的 `--list-accounts` 包成 Tauri 命令）。
//! 本模块是它的本地对侧：**同样的输出类型**（`AccountsResult`），前端拿到的形状逐字段一致
//! ——那正是 §40「本地 = 不走 ssh 的远端」在这一格上的意思。
//!
//! # 为什么是第三份实现，而不是复用 daemon 那份
//!
//! `remote-daemon-proto/src/observe/accounts_query.rs` 已经有一份完整的 Rust manifest 读取器
//! （1383 行，直接读文件系统）。**复用不了**，理由是结构性的：
//!
//! - 那个 crate 是 **bin-only**（无 `[lib]`），且 `Cargo.toml` 注释写明**刻意不进 workspace**
//!   ——「a workspace would pull this Linux-only daemon into the Windows CI
//!   `cargo test --all` and break the build」。
//! - monitor **必须在 Windows 上构建**。让它依赖一个 Linux-only crate，正是那条注释在防的事。
//!
//! ⇒ 于是这份数据有 **三个读者**：`cc-acct-iso`（bash，写侧）· daemon（远端读）· 本模块（本地读）。
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

use crate::accounts::{AccountsMeta, AccountsResult, RemoteAccount};
use acct_core::{
    is_deceptive_char, ACCTS_DIR_NAME, CREDENTIALS_NAME, MANIFEST_NAME, SUPPORTED_SCHEMA,
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
fn local_accts_dir() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(ACCTS_DIR_NAME))
}

/// 纯函数核心：给定账号库目录，产出与远端**同形**的结果。
///
/// 拆成纯函数是为了测试能在 `mktemp` 造的沙盒目录上跑，
/// **绝不碰用户真实的 `~/.claude-accts`**。
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
            // 只 stat 存在性，**绝不读内容**。探不到目录 ⇒ false，那是「不知道」，不假装已登录。
            logged_in: probe_dir
                .as_deref()
                .map(|d| d.join(CREDENTIALS_NAME).is_file())
                .unwrap_or(false),
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

/// L3a：列出**本机**的账号（读 `$HOME/.claude-accts/accounts.json`）。
///
/// `list_remote_accounts` 的本地对侧，**输出类型完全相同**。只读：不写任何文件、
/// 不起任何进程、不读凭据内容（只 stat 存在性）。
#[tauri::command]
pub async fn list_local_accounts() -> Result<AccountsResult, String> {
    let dir = local_accts_dir().ok_or_else(|| "取不到 HOME，无法定位本机账号库".to_string())?;
    // 读文件是阻塞 IO，挪到阻塞线程池（与 `list_remote_accounts` 同处理）。
    tokio::task::spawn_blocking(move || list_from_dir(&dir))
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
    // 它们两侧确实不同，但那是**刻意的平台特化**不是漂移 ——
    // 本机侧要认 Windows 盘符（`looks_absolute`）且必须允许 `\` 作分隔符，
    // 所以改成拒 `\..\`；daemon 是 Linux-only，直接把 `\` 当危险字符拒掉。
    // 硬合只能二选一：要么本机失去 Windows 路径，要么 daemon 失去对 `\` 的拒绝。
}

// ─────────────────────────────────────────────────────────────────────────────
// E79：本机的「某个 sid 现在跑在哪个账号下」
// ─────────────────────────────────────────────────────────────────────────────

// 〔F10b 第二批·下半〕`MAX_LOCAL_SESSION_FILES` / `MAX_LOCAL_SESSION_FILE_BYTES` **已删** ——
// 它们是 daemon 侧同名上限的**第二份**（`observe/accounts_query.rs:47/:49`，值逐字相同：
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
/// - ⚠ 只抠 `CLAUDE_CONFIG_DIR` 一个键、`configDir` 过白名单 —— 那两条现在由 daemon 侧守
///   （它的 `observe/accounts_query.rs` 头注逐字写着同一套边界）。
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
