//! L3a（local-as-remote）：**本机**多账号枚举 —— 只读，直接读 manifest。
//!
//! `accounts.rs` 是这件事的**远端**那半（把 daemon 的 `--list-accounts` 包成 Tauri 命令）。
//! 本模块是它的本地对侧：**同样的输出类型**（`AccountsResult`），前端拿到的形状逐字段一致
//! ——那正是 §40「本地 = 不走 ssh 的远端」在这一格上的意思。
//!
//! # 为什么是第三份实现，而不是复用 daemon 那份
//!
//! `remote-daemon-proto/src/accounts_query.rs` 已经有一份完整的 Rust manifest 读取器
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
//! `contract_matches_the_daemon_implementation`——它**读 daemon 的源文件**，钉住四条契约。
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
use std::path::{Path, PathBuf};

/// manifest 读取上限（与 daemon 侧同值；账号数有限，8MB 是兜底不是预期）。
const MANIFEST_CAP: u64 = 8 * 1024 * 1024;

/// 账号库目录名（`$HOME/.claude-accts`）。**双写点**：daemon 侧同名常量在
/// `accounts_query.rs`，`cc-acct-iso` 的 `NATIVE_IDENTITY` 里也有一份。
const ACCTS_DIR_NAME: &str = ".claude-accts";
/// manifest 文件名。同上，三处双写。
const MANIFEST_NAME: &str = "accounts.json";
/// 「什么算已登录」的判据文件名。**Z06 双写点**：daemon 侧有守卫钉它与 bash 声明一致，
/// 本模块这一份由 `contract_matches_the_daemon_implementation` 钉住。
const CREDENTIALS_NAME: &str = ".credentials.json";
/// 本模块认得的 manifest schema 版本。与 daemon 同值。
const SUPPORTED_SCHEMA: u64 = 1;

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

/// 会被用来做视觉欺骗的 Unicode 码点（双向覆盖 / 零宽 / 异常空白 / 行段分隔）。
///
/// 逐字对齐 daemon 侧的同名函数 —— 这是**平台无关的安全性质**：它们不是
/// `char::is_control`（那只覆盖 C0/C1），但在 UI 里能伪造同形/反向的账号名与路径。
fn is_deceptive_char(c: char) -> bool {
    matches!(c,
        '\u{200B}'..='\u{200F}'
            | '\u{202A}'..='\u{202E}'
            | '\u{2060}'..='\u{2064}'
            | '\u{2066}'..='\u{2069}'
            | '\u{00A0}'
            | '\u{1680}'
            | '\u{2000}'..='\u{200A}'
            | '\u{2028}'
            | '\u{2029}'
            | '\u{202F}'
            | '\u{205F}'
            | '\u{3000}'
            | '\u{FEFF}')
}

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
    // 平台无关的那一半：shell 元字符 + 视觉欺骗字符。**反斜杠不在此列**
    // ——Windows 路径分隔符就是它；本模块产出的值不进任何 shell（本地注入走 env，不拼命令）。
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
        fn write_manifest(&self, json: &str) {
            std::fs::write(self.0.join(MANIFEST_NAME), json).expect("write manifest");
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

        std::fs::write(a.join(CREDENTIALS_NAME), "{}").unwrap();
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

    /// ★ **跨 crate 契约守卫**：本模块是这份数据的第三个读者（bash 写侧 · daemon 远端读 ·
    /// 本模块本地读），而 daemon crate 是 bin-only + 刻意不进 workspace ⇒ 复用不了。
    /// 那就照本仓既有纪律**把契约钉住**（同 `TMUX_LS_FMT` 双写点那条守卫的做法）：
    /// **读 daemon 的源文件**，确认四条判据两侧一致。
    #[test]
    fn contract_matches_the_daemon_implementation() {
        let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("remote-daemon-proto")
            .join("src")
            .join("accounts_query.rs");
        let raw = std::fs::read_to_string(&p).expect("读 daemon 的 accounts_query.rs");
        // 反向自检：真读到了（不写 `> 0`——空转也满足）。
        assert!(
            raw.len() > 10_000,
            "只读到 {} 字节，多半路径错了",
            raw.len()
        );
        // ★ 只扫**生产段**、且**剥掉注释**。第一版两条都没做，代价是它当场被变异证伪：
        // 把 daemon **测试模块**里的一个同名常量改掉，守卫照样绿——因为 `contains` 在别处
        // （生产字面量、甚至散文）还能找到同一串。**那样的守卫是安慰剂。**
        // 剥 cfg(test) 的锚点用转义写法，与真正的换行不相等 ⇒ 不会匹配到本行自己。
        let marker = "\n#[cfg(test)]\nmod tests";
        let prod = match raw.find(marker) {
            Some(i) => &raw[..i],
            None => raw.as_str(),
        };
        let src: String = prod
            .lines()
            .filter(|l| !l.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            src.len() > 5_000 && src.len() < raw.len(),
            "剥完生产段只剩 {} 字节（原文 {}）——剥法坏了",
            src.len(),
            raw.len()
        );
        assert!(
            src.contains("fn list_accounts("),
            "锚点没对上，daemon 侧结构变了？"
        );

        for (what, needle) in [
            ("manifest 文件名", format!("\"{MANIFEST_NAME}\"")),
            ("凭据文件名", format!("\"{CREDENTIALS_NAME}\"")),
            ("账号库目录名", format!("\"{ACCTS_DIR_NAME}\"")),
            ("schema 版本", format!("Some({SUPPORTED_SCHEMA}) =>")),
        ] {
            assert!(
                src.contains(needle.as_str()),
                "{what}（本机侧 {needle}）在 daemon 侧找不到 —— 两侧漂了。\n\
                 这份数据有三个读者（cc-acct-iso 写侧 / daemon 远端读 / 本模块本地读），\n\
                 daemon crate 是 bin-only 且刻意不进 workspace（Windows CI 会被拖垮）⇒ 复用不了，\n\
                 只能靠这条守卫。改任一侧都要来对一次。"
            );
        }
    }
}
