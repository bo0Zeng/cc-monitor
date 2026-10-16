//! 多账号（账号库：每个号一个配置目录，共享项链回同一个配置根）的只读消费侧（写侧是 `accounts/manage/`）。
//!
//! 帧面三条（`inbound::REGISTRY` 的账号族，宿主 `faces/read_face.rs`）：
//! - `accounts-list` → `{meta, accounts, notice}`（[`list_product_with`]）。
//! - `accounts-sessions` → 每条运行中会话一行：它的 `CLAUDE_CONFIG_DIR` 属于哪个账号（[`session_accounts`]）。
//! - `accounts-trust` → `{trusted, known}`：目标账号是否已信任该目录（换号 resume 前的预检 —— 首次用某账号进某目录，
//!   CC 会弹信任确认，会卡住编排）；`configDir` 缺席 = 账号 0（它没有 configDir，`.claude.json` 在 `$HOME`）。
//!
//! # 账号 0
//!
//! manifest 里 `configDir` 键缺席的那一条 = 账号 0 =「不设 `CLAUDE_CONFIG_DIR`」这个状态本身。结构性地认它（看键在不在），
//! 不认名字（不硬编码 "0"）。它的 config dir 是共享库（`sharedStore`）、`.claude.json` 在 `$HOME`。
//! 空串不算缺席：`is_safe_config_dir("")` 会挡掉它（空值 ≠ 未设）。
//!
//! # 谓词的作用面不对称
//!
//! `is_safe_config_dir` 的调用点都在清单侧（源是 accounts manifest）；`accounts-sessions` 那条路的 `configDir` 来自
//! `/proc/<pid>/environ`，没过谓词就作为帧字段发出去。不给它套谓词：进程侧的值只拿去与 `by_dir`（只装过了谓词的目录）比对，
//! 不安全的值匹配不上、`account` 恒 `None`；若把它丢弃（置 `None`），那条会话就变成 `bare: true` —— 等于把一个可疑会话贴成默认账号。
//! ⇒ 归属安全、展示未净化；要表达「configDir 不可信」得给帧加一个状态位（改上线契约）。

//!
//! # 只读（src/doc/INVARIANTS.md §1）
//!
//! 只 `read` / `read_dir` / `metadata`，零写入，不 shell out（直接读 manifest 文件，不依赖 PATH）。
//!
//! # 凭据边界
//!
//! - `.credentials.json` 只 stat 存在性，绝不读内容。
//! - `.claude.json` 只取 `projects[<cwd>].hasTrustDialogAccepted` 一个布尔；绝不回传文件内容（里面有 `mcpServers` 的环境变量，可能含 API key）。
//! - `/proc/<pid>/environ` 只抠两个写死的键（账号配置目录 · `ANTHROPIC_BASE_URL`，键名由适配层给），不回传整个环境快照。
//!   `ANTHROPIC_BASE_URL` 的值带中转钥匙 ⇒ 只折成一个布尔（`viaRelay`），值本身不出参、不进日志。键名不接受调用方传进来
//!   （否则就退化成「任意环境变量读」原语）；判据数着本文件生产段里读环境的调用点，多一处 ⇒ 红。
//! - `accounts-trust` 的 `configDir` 必须逐字等于 manifest 里某个账号的 `configDir`，否则拒绝（不退化成任意文件读）；
//!   问账号 0 时不收路径参数。

use crate::common::cells::Words;
use crate::history::live::{parse_procstart_ticks, pidfiles};
use crate::platform::fs::Ro;
use acct_core::{
    auth_kind_from_manifest, auth_kind_with_apikey_table, auth_ready, CREDENTIALS_NAME,
    SUPPORTED_SCHEMA,
};
pub(crate) use cmd_wire::accounts::{AccountRow, AccountsList, Badge, ListMeta, ScannedAccount};
pub(crate) use cmd_wire::needs::NeedsRow;
use copy_core::copy_text;
use relay_route_core::{ACCOUNTS_DIR_REL, ACCOUNTS_MANIFEST_NAME};
use std::path::{Path, PathBuf};

/// manifest 读取上限。账号数有限，几 MB 足矣，此处宽松给 8MB 兜底。
const MAX_MANIFEST_BYTES: u64 = 8 * 1024 * 1024;
// ---------------------------------------------------------------- manifest

#[derive(serde::Deserialize)]
struct RawAccount {
    name: String,
    #[serde(default)]
    email: Option<String>,
    /// 可以缺席：缺席 = 账号 0 =「不设 `CLAUDE_CONFIG_DIR`」，它的 config dir 就是共享库。判据是结构性的（这个键在不在），不认名字。
    /// 空串不算缺席（`is_safe_config_dir("")` 会挡掉）。
    #[serde(rename = "configDir", default)]
    config_dir: Option<String>,
    #[serde(rename = "isDefault", default)]
    is_default: bool,
    #[serde(default)]
    mode: Option<String>,
    /// 鉴权方式。可以缺席 —— 缺席 = 订阅号。分类规则的唯一住址是 `acct_core::auth_kind_from_manifest`，这里不再写一份 match。
    #[serde(rename = "authKind", default)]
    auth_kind: Option<String>,
}

struct Manifest {
    updated_at: Option<String>,
    shared_store: Option<String>,
    accounts: Vec<RawAccount>,
}

/// 路径是否可安全地交给下游。规则是 `acct_core::config_dir_ok`（全仓唯一一份，认 Windows 形：`\` 是那边的路径分隔符）；
/// 拼进 POSIX 命令之前另要过 `acct-core` 的 posix 那一条（它拒 `\`）—— 分层校验。
/// 允许普通空格与常规非 ASCII（如中文），拒绝引号 / 命令替换 / 重定向 / 通配 / 控制字符 + 视觉欺骗类 Unicode。
pub(crate) fn is_safe_config_dir(p: &str) -> bool {
    // 规则住共享 crate（`acct_core::config_dir_ok`）：后端 `control/ccm` 起会话也要这张全表，而 `control → observe` 是禁止方向。
    acct_core::config_dir_ok(p)
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
    crate::platform::paths::home_dir()
}

/// 账号库目录：`$HOME/.cc-monitor/accounts`。位置只跟着家走 —— 没有另指位置的变量或选项
/// （测试换掉家目录就碰不到真数据）。
fn resolve_accts_dir() -> PathBuf {
    match home_dir() {
        Some(h) => h.join(ACCOUNTS_DIR_REL),
        None => PathBuf::from(ACCOUNTS_DIR_REL),
    }
}

fn manifest_path(accts_dir: &Path) -> PathBuf {
    accts_dir.join(ACCOUNTS_MANIFEST_NAME)
}

/// **这台机器上**那份账号 manifest 在哪（与 `accounts-list` 读的是同一份）。watcher 盯着它、变了发 `changed {accounts}`。
pub(crate) fn default_manifest_path() -> PathBuf {
    manifest_path(&resolve_accts_dir())
}

/// 读 + 解析 manifest。缺文件/坏 JSON/不支持的 schema 都是 `Err(人话原因)`——
/// 调用方据此输出 `enabled:false` 而**不是**失败退出（"没启用多账号"是正常状态）。
///
/// 账号数组**逐条**解析：单个坏账号（缺 name/configDir 等）被跳过而非拖垮整份
/// manifest（避免手改 manifest 时一坏全灭；写侧 `accounts/manage/model.rs` 对这种条目也是原样留着、不去动它）。
/// 这台启用了多账号没有（账号库清单读得出 ⇒ 是）：与账号清单那一份的 `meta.enabled` 同一个判法（[`scan_accounts`]）。
/// 首次运行「给现在登录的号起个名字」那一步读它（`footprint/readiness.rs`）。
pub(crate) fn accounts_enabled_here() -> bool {
    load_manifest(&resolve_accts_dir()).is_ok()
}

/// 账号库清单在、却读不懂（读不出 · 不是 JSON · 版本不认）⇒ 为什么（同账号清单 `meta.error` 那一句）；读得懂 · 没有清单
/// （没启用多账号，正常状态）⇒ `None`。需手动清单据此回 `unreadable`，不把读不懂当成「没有需登录的号」。
pub(crate) fn library_unreadable(accts_dir: &Path) -> Option<String> {
    let why = load_manifest(accts_dir).err()?;
    (!matches!(manifest_path(accts_dir).try_exists(), Ok(false))).then_some(why)
}

/// [`library_unreadable`]，这台的账号库。
pub(crate) fn library_unreadable_here() -> Option<String> {
    library_unreadable(&resolve_accts_dir())
}

fn load_manifest(accts_dir: &Path) -> Result<Manifest, String> {
    let p = manifest_path(accts_dir);
    // 没有清单 ＝ 没启用多账号（正常状态）：只回那一句，不记日志（记了就是每次列账号都多一行 WARN，CLI 的 stderr 也跟着脏）。
    if matches!(p.try_exists(), Ok(false)) {
        return Err(copy_text(
            "beAccountsQuery.loadManifest.unreadable",
            &[
                ("path", &(p.display()).to_string()),
                ("why", &copy_core::io_reason(std::io::ErrorKind::NotFound)),
            ],
        ));
    }
    // 屏上那一句只带原因词；系统原话记一行日志（这一形的失败是给账号页一句话，没有原话位）。
    let bytes = Ro::read_regular_capped(&p, MAX_MANIFEST_BYTES).map_err(|e| {
        crate::common::said::IntoNote::into_note(e.wrap(|why| {
            copy_text(
                "beAccountsQuery.loadManifest.unreadable",
                &[("path", &(p.display()).to_string()), ("why", why)],
            )
        }))
    })?;
    // UTF-8 BOM 剥掉再解析：PowerShell 5.1 `-Encoding UTF8` 与记事本默认写 BOM，`serde_json` 不吃它 ⇒ 不剥就是账号页整块空。
    // 同一份文件的另一个读者是 `control/ccm/plan.rs::AccountTable::load`，两个读者读出同一张表由 `tests::both_readers_of_the_manifest_see_the_same_accounts` 钉。
    let body = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(&bytes);
    let root: serde_json::Value = serde_json::from_slice(body).map_err(|e| {
        crate::common::said::IntoNote::into_note(crate::common::said::Said::with_raw(
            copy_text("beAccountsQuery.loadManifest.badJson", &[]),
            e,
        ))
    })?;
    match root.get("version").and_then(|v| v.as_u64()) {
        Some(SUPPORTED_SCHEMA) => {}
        Some(v) => {
            return Err(copy_text(
                "beAccountsQuery.loadManifest.badVersion",
                &[("v", &v.to_string())],
            ))
        }
        None => return Err(copy_text("beAccountsQuery.loadManifest.noVersion", &[]).into()),
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

// `/proc/<pid>/stat` 的 starttime 只有一份：`platform::proc::proc_starttime`。
use crate::platform::proc::{proc_env_var, proc_starttime, EnvRead};

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

/// 一条会话在出参里的全部字段（`accounts-sessions` 每行一个）。
struct SessionRow {
    pid: u32,
    sid: Option<String>,
    cwd: Option<String>,
    config_dir: Option<String>,
    account: Option<String>,
    alive: bool,
    /// 读 `CLAUDE_CONFIG_DIR` 的那一次，`/proc/<pid>/environ` 这一刻取不到（读失败 / 读回 0 字节）。
    /// 与 `config_dir: None`（读到了、这个键没设）是两件事 ⇒ 本格为真时不归属账号、也不算裸起。
    /// 不进出参：`configDir:null` + `account:null` + `bare:false` 已经表达「不知道」。
    cfg_env_unreadable: bool,
    /// 这条会话的 `ANTHROPIC_BASE_URL` 是不是**本机中转那一形**（回环 ＋ 钥匙段 ＋ 路由，
    /// `relay_route_core::split_keyed_base_url` 认得出）。`None` = 不知道（进程已死 / 环境这一刻取不到）。
    /// 用途：机器页「停」本机后端之前数一数有几条会话会断。
    via_relay: Option<bool>,
}

// ---------------------------------------------------------------- 命令

/// 清单的**扫描本体**：`(meta, 账号们)`，帧面成品（[`list_product_with`]）经它读 manifest / 判安全 / 判鉴权方式。
///
/// `in_table(configDir)`：这个号在**这台机器**的 apikey 表里有没有行（调用方按 `acct_core::apikey_routed_subset` 答）。它是鉴权方式的**第二个输入**，
/// 进的是同一条计算路径（`auth_kind_with_apikey_table` → `auth_ready`，各只一处）。
fn scan_accounts(
    accts_dir: &Path,
    in_table: &dyn Fn(&str) -> bool,
) -> (ListMeta, Vec<ScannedAccount>) {
    let meta = |enabled: bool,
                updated_at: Option<String>,
                shared_store: Option<String>,
                count: usize,
                error: Option<String>,
                effective_default: Option<String>| ListMeta {
        enabled,
        accts_dir: accts_dir.to_string_lossy().into_owned(),
        manifest_path: manifest_path(accts_dir).to_string_lossy().into_owned(),
        updated_at,
        shared_store,
        count,
        error,
        unsupported: crate::platform::acct_view::unsupported_said(),
        next_default: next_default(accts_dir),
        // 这台的默认号是谁（标了的第一个，没标 ⇒ 第一个）：规则只住 `acct_core::effective_default`，界面与起会话都读这一格。
        effective_default,
        home: None,
    };
    match load_manifest(accts_dir) {
        Err(e) => (meta(false, None, None, 0, Some(e), None), Vec::new()),
        Ok(m) => {
            let colors = colors_at(accts_dir);
            let mut lines = Vec::new();
            for a in &m.accounts {
                // configDir 缺席 = 账号 0：它的 config dir 就是共享库 ⇒ 登录态查那儿，`configDir` 在帧里出 null（下游据此不注入 CLAUDE_CONFIG_DIR）。
                let (cfg_out, probe_dir) = match a.config_dir.as_deref() {
                    None => (None, m.shared_store.as_deref().map(PathBuf::from)),
                    Some(c) => {
                        if !is_safe_config_dir(c) {
                            tracing::warn!("账号 {} 的 configDir 不安全，已丢弃：{}", a.name, c);
                            continue;
                        }
                        let n = norm_dir(c);
                        (Some(n.to_string()), Some(PathBuf::from(n)))
                    }
                };
                // 只 stat 存在性，绝不读内容。这个文件名就是身份表
                // （`agents::claudecode::accounts::NATIVE_IDENTITY`）里凭据那一项的同一个常量。
                // 探不到 config dir（账号 0 且 manifest 没写 sharedStore）⇒ false，
                // 那是「不知道」，不假装已登录。
                let credentials_present = probe_dir
                    .as_ref()
                    .is_some_and(|d| d.join(CREDENTIALS_NAME).exists());
                // 鉴权方式这一维：分类与就绪各只有一处实现，都住 `acct-core`（本文件与 `local_accounts.rs` 都调它）。
                // 第二个输入：这台机器的 apikey 表里有没有它。
                let in_apikey_table = cfg_out.as_deref().is_some_and(in_table);
                let auth_kind = auth_kind_with_apikey_table(
                    auth_kind_from_manifest(a.auth_kind.as_deref()),
                    in_apikey_table,
                );
                let mode = a.mode.clone().unwrap_or_else(|| "isolated".into());
                let exists = match &probe_dir {
                    Some(d) if a.config_dir.is_some() => d.is_dir(),
                    _ => a.config_dir.is_none(),
                };
                let ready = auth_ready(auth_kind, credentials_present);
                lines.push(ScannedAccount {
                    name: a.name.clone(),
                    // 号在人眼里叫什么（账号 0 ⇒ 「~/.claude」那一句，其余照原名）：唯一一处 `name_words::account_text`，界面 · 手机照抄。
                    label: crate::accounts::quota::name_words::account_text(&a.name),
                    email: a.email.clone().unwrap_or_default(),
                    config_dir: cfg_out,
                    is_default: a.is_default,
                    // 能拿来起会话 / 选为默认：规则只住 `acct_core::account_selectable`，界面与起会话都读这一格。
                    selectable: acct_core::account_selectable(
                        &mode,
                        ready,
                        exists,
                        a.config_dir.is_some(),
                    ),
                    mode,
                    // 账号 0 恒 exists（「裸起」这个状态永远可达）；有 configDir 的看目录在不在。
                    exists,
                    // 仅 stat `.credentials.json` 存在性，不代表凭据有效；可用性走 `authReady`。
                    logged_in: credentials_present,
                    // 订阅 / api-key。manifest 缺这个键 ⇒ 订阅。
                    auth_kind: auth_kind.to_string(),
                    // 鉴权方式这一维不阻塞它被选中；不等于真能连上（界面要把这个状态说出来）。
                    auth_ready: ready,
                    // 号色（`--acct-c0..c7` 第几格）：账号库里的事实，界面只画它（[`colors_at`]；形状不合规矩的那条按名字哈希兜底）。
                    color: colors
                        .get(&a.name)
                        .copied()
                        .unwrap_or_else(|| crate::accounts::manage::model::color_fallback(&a.name)),
                });
            }
            let effective_default =
                acct_core::effective_default(&lines, |a| a.is_default).map(|a| a.name.clone());
            (
                meta(
                    true,
                    m.updated_at.clone(),
                    m.shared_store.clone(),
                    lines.len(),
                    None,
                    effective_default,
                ),
                lines,
            )
        }
    }
}

/// `accounts-list` 的入参。
#[derive(Debug, Clone, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields)]
pub(crate) struct AccountsListReq {
    /// 这次起会话的是哪一家（线上的 kind，同 `session-new-facts` 给的那一格；空串 ⇒ 默认那一家，注册表里没有 ⇒ `bad_args`）。
    pub(crate) agent: crate::agents::AgentKind,
}

/// ★ 选不了的那一句（唯一一处；判定本身是 `acct_core::account_selectable`，这里只说是四条里的哪一条没过）。
/// 账号 0 不经选号 ⇒ 没有这一句。
fn unselectable_of(a: &ScannedAccount, api_key: bool) -> Option<String> {
    if a.selectable || a.config_dir.is_none() {
        return None;
    }
    Some(if a.mode != "isolated" {
        copy_text("accounts.badge.inPlace", &[])
    } else if !a.exists {
        copy_text("accounts.row.missing", &[])
    } else if api_key {
        copy_text("acct.tag.key", &[])
    } else {
        copy_text("acct.tag.login", &[])
    })
}

/// 选不了只差登录（四条里只有「需登录」那一条没过）。
fn needs_login(a: &ScannedAccount) -> bool {
    !a.selectable
        && a.config_dir.is_some()
        && a.mode == "isolated"
        && a.exists
        && a.auth_kind != acct_core::AUTH_KIND_API_KEY
}

/// ★ 每个号给不给［登录…］（唯一一处）：只差登录、而这一家能在 app 里登录（`sign_in`）⇒ 给；只差登录却登不了 ⇒ 那一句换成「需登录 · 不支持在 app 里登录」。
pub(crate) fn with_sign_in(list: &mut AccountsList, sign_in: bool) {
    for r in &mut list.accounts {
        let need = needs_login(&r.scanned);
        r.login = need && sign_in;
        if need && !sign_in {
            r.unselectable = Some(Words(copy_text("accounts.row.loginElsewhere", &[])));
        }
    }
}

/// 号目录里那份凭据文件的名字（「已登录」只 stat 它在不在；watcher 盯它出现 / 变了）。
pub(crate) const CREDENTIALS_FILE: &str = CREDENTIALS_NAME;

/// 删掉默认号之后新会话默认谁（默认号只有清单里那一份）：问写侧那一条规则（`Manifest::without`），这里不另判。
/// 清单读不出 / 没有默认号 / 删了之后没有别的号 ⇒ `None`。
fn next_default(accts_dir: &Path) -> Option<String> {
    let m = library_model(accts_dir)?;
    let current = m.managed().find(|a| a.is_default)?.name.clone();
    m.without(&current).1
}

/// 写侧那份模型读出来的清单（读不出 ⇒ `None`）。
fn library_model(accts_dir: &Path) -> Option<crate::accounts::manage::model::Manifest> {
    let bytes = Ro::read_regular_capped(&manifest_path(accts_dir), MAX_MANIFEST_BYTES).ok()?;
    let text = String::from_utf8(bytes).ok()?;
    crate::accounts::manage::model::Manifest::parse(text.trim_start_matches('\u{feff}')).ok()
}

/// 这台账号库给每个号的号色（名 ⇒ 第几格；读不出 ⇒ 空表）：别处画号色（轮换那一页）与 `accounts-list` 同一份；
/// 不在表里的号照 `color_fallback` 按名字算（同清单那一行）。
pub(crate) fn account_colors() -> std::collections::BTreeMap<String, u8> {
    colors_at(&resolve_accts_dir())
}

/// 清单里每个号的号色（名 ⇒ 第几格）：盘上有就照盘上；还没补过色的照补色那一处现算（`Manifest::with_colors`，
/// 与常驻那一个补完写回去的同一份）。读不出 ⇒ 空表（行上按名字哈希兜底）。
fn colors_at(accts_dir: &Path) -> std::collections::BTreeMap<String, u8> {
    let Some(m) = library_model(accts_dir).map(|m| m.with_colors()) else {
        return Default::default();
    };
    m.managed()
        .filter_map(|a| a.color.map(|c| (a.name.clone(), c)))
        .chain(
            m.zero_color
                .map(|c| (acct_core::BARE_ACCOUNT.to_string(), c)),
        )
        .collect()
}

/// 帧面 `accounts-list` 的成品（账号域读自己那台的 apikey 表，agent 随请求带）。
///
/// `{meta, accounts, notice}`：清单经 [`scan_accounts`] 扫，并上这台机器自己那份 apikey 表
/// （`rows`：表里有哪几条账号 id，调用方从 `accounts::upstream_select::file_face` 读来 —— 与中转里的上游选择同一个出处）；
/// 「哪几个号在表里有行」只问 `acct_core::apikey_routed_subset`（`table_agent`：这台机器上那份文件属于哪一家）。
/// `notice`：「能用但有缺」—— manifest 启用了、却一个账号 0 都没有（写它的那一侧旧到不认账号 0）。措辞不说「远端」：本机远端同一条路。
/// `relay_running` ＝ 这台的中转在不在（徽章要它；调用方读本进程的监听状态，与 `relay-optin` 的 `listening` 同一个判准 ⇒
/// 只有常驻进程答得出真话，`accounts-list` 在 `STREAM_ONLY`）。
pub(crate) fn list_product(
    rows: &[String],
    agent: &str,
    table_agent: &str,
    relay_running: bool,
) -> AccountsList {
    let creds = crate::accounts::upstream_select::file_face::machine_path()
        .ok()
        .and_then(|p| crate::accounts::upstream_select::creds::perm_notice(&p));
    with_notice(
        list_product_with(
            &resolve_accts_dir(),
            home_dir().as_deref(),
            rows,
            &crate::accounts::upstream_select::file_face::machine_key_facts(),
            agent,
            table_agent,
            relay_running,
        ),
        creds,
    )
}

/// 清单的 `notice` 并上另一句（凭据文件权限那一句，KS11）：两句都有 ⇒ 连成一句；只有一句 ⇒ 就是它。
pub(crate) fn with_notice(mut v: AccountsList, more: Option<String>) -> AccountsList {
    let Some(more) = more else { return v };
    v.notice = Some(match v.notice.take() {
        Some(a) => copy_text(
            "beAccountsQuery.listProductAt.joined",
            &[("a", &a), ("b", &more)],
        ),
        None => more,
    });
    v
}

/// ★ 徽章一处判（每一态只出自这里）：原地模式 ⇒ 不隔离；API key 号 ⇒ 这台 key 表里有没有它那一行 · 这台的中转在不在
/// （两格都成立才「经中转」）；订阅号 ⇒ 登录了没有。`routed` ＝ 表里有它那一行（账号 0 没有目录名 ⇒ `None`：说不出，不替它下判断）。
pub(crate) fn badge_of(
    in_place: bool,
    api_key: bool,
    auth_ready: bool,
    routed: Option<bool>,
    relay_running: bool,
) -> Badge {
    let b = |text: String, warn: bool, title: String| Badge {
        text: Words(text),
        warn,
        title: Words(title),
    };
    if in_place {
        return b(
            copy_text("accounts.badge.inPlace", &[]),
            true,
            copy_text("accounts.badge.inPlaceHint", &[]),
        );
    }
    if api_key {
        return match routed {
            Some(true) if relay_running => b(
                copy_text("accounts.badge.apikeyRelayed", &[]),
                false,
                copy_text("accounts.badge.apikeyRelayedHint", &[]),
            ),
            Some(true) => b(
                copy_text("accounts.badge.apikeyRelayDown", &[]),
                true,
                copy_text("accounts.badge.apikeyRelayDownHint", &[]),
            ),
            // 两种「没配上」的成因，各说各的 —— 合成一句就等于又写下一句说不准的话。
            Some(false) => b(
                copy_text("accounts.badge.apikeyNoEndpoint", &[]),
                true,
                copy_text(
                    "accounts.badge.apikeyNoEndpointHint",
                    &[("why", &copy_text("accounts.badge.whyNoRow", &[]))],
                ),
            ),
            None => b(
                copy_text("accounts.badge.apikeyNoEndpoint", &[]),
                true,
                copy_text(
                    "accounts.badge.apikeyNoEndpointHint",
                    &[("why", &copy_text("accounts.badge.whyUnknown", &[]))],
                ),
            ),
        };
    }
    if !auth_ready {
        return b(
            copy_text("accounts.badge.notSignedIn", &[]),
            true,
            copy_text("accounts.badge.notSignedInHint", &[]),
        );
    }
    b(
        copy_text("accounts.badge.signedIn", &[]),
        false,
        String::new(),
    )
}

/// [`list_product`] 的全参本体（账号库目录是参数：判据拿夹具喂它，不碰真家目录）。
///
/// - `meta.home`：这台的家目录（界面把路径里的它缩成 `~`；推不出 ⇒ `null`）。
/// - 每个号 `keyMasked` · `baseUrl`：API 号在这台 apikey 表里那一行的掩码（只留末四位）与端点；
///   订阅号 / 表里没有它那一行 / 没配 key ⇒ `null`。key 本体从不出后端。
pub(crate) fn list_product_with(
    accts_dir: &Path,
    home: Option<&Path>,
    rows: &[String],
    keys: &[crate::accounts::upstream_select::file_face::KeyFact],
    agent: &str,
    table_agent: &str,
    relay_running: bool,
) -> AccountsList {
    let routed = |dir: &str| {
        !acct_core::apikey_routed_subset(&[dir.to_string()], rows, agent, table_agent).is_empty()
    };
    let (mut meta, scanned) = scan_accounts(accts_dir, &routed);
    meta.home = home.map(|h| h.to_string_lossy().into_owned());
    let accounts: Vec<AccountRow> = scanned
        .into_iter()
        .map(|a| {
            let api_key = a.auth_kind == acct_core::AUTH_KIND_API_KEY;
            let fact = api_key
                .then(|| {
                    a.config_dir
                        .as_deref()
                        .and_then(acct_core::apikey_account_id_of_dir)
                })
                .flatten()
                .and_then(|id| keys.iter().find(|k| k.id == id));
            let badge = badge_of(
                a.mode == "in-place",
                api_key,
                a.auth_ready,
                a.config_dir.as_deref().map(&routed),
                relay_running,
            );
            AccountRow {
                key_masked: fact.and_then(|k| k.masked.clone()),
                base_url: fact.and_then(|k| k.base_url.clone()),
                badge,
                plate: Words(crate::accounts::quota::name_words::plate_text(&a.name)),
                quota: None,
                unselectable: unselectable_of(&a, api_key).map(Words),
                login: false,
                scanned: a,
            }
        })
        .collect();
    let notice = (meta.enabled && !accounts.iter().any(|a| a.scanned.config_dir.is_none()))
        .then(|| copy_text("beAccountsQuery.listProductAt.noDefault", &[]));
    AccountsList {
        meta,
        accounts,
        notice,
    }
}

/// 这台活会话里那一家说断开的一个 MCP 服务器（[`live_mcp_failed`] 的一项）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LiveMcpFailed {
    pub name: String,
    /// 说它的那条记录的时刻（epoch ms）；记录没写 / 解不出 ⇒ `None`。
    pub at_ms: Option<i64>,
    /// 那一家写的原话（复制详情用）。
    pub detail: Option<String>,
    /// 说它的那条会话的标题（同历史清单那一格的口径）。
    pub title: String,
}

/// 各条活会话记录里 MCP 那一格读到了哪：记录 ⇒ （读过的字节数 · 那一格）。扩展页每打开一次只读各条记录新长出来的那一截。
#[derive(Debug, Default)]
pub(crate) struct McpBook(
    std::collections::BTreeMap<PathBuf, (u64, Vec<super::facts_query::McpTrouble>)>,
);

impl McpBook {
    /// 这条记录此刻的 MCP 那一格：从上次读到的地方接着读（记录比上次读到的短 ⇒ 换了一份，从头读）。
    pub(crate) fn read(
        &mut self,
        path: &Path,
    ) -> std::io::Result<Vec<super::facts_query::McpTrouble>> {
        use std::io::{Seek, SeekFrom};
        let mut file = Ro::open(path)?;
        let len = file.metadata()?.len();
        let (at, mcp) = self.0.entry(path.to_path_buf()).or_default();
        if len < *at {
            *at = 0;
            mcp.clear();
        }
        file.seek(SeekFrom::Start(*at))?;
        *at += super::facts_query::mcp_from(std::io::BufReader::new(file), mcp)?;
        Ok(mcp.clone())
    }

    /// 只留这几条（这一轮不活了的那几条忘掉）。
    pub(crate) fn keep_only(&mut self, live: &[PathBuf]) {
        self.0.retain(|p, _| live.contains(p));
    }
}

/// 本进程那一本（常驻后端里跨请求留着；一次性 CLI 进程里每次都是空的，等于从头读）。
static MCP_BOOK: std::sync::Mutex<Option<McpBook>> = std::sync::Mutex::new(None);

/// 这台此刻活着的交互会话（`homes` 各号的家里的 pidfile；判活同 [`live_session_ids`]，后台任务不算）里，
/// 那一家说断开的 MCP：每个名字取说它最晚的那一条，按名字排。会话结束了的不算（那份原话已经过时）。
/// 每条会话只读 MCP 那一格、只读新长出来的那一截（[`McpBook`]）；标题取历史清单的缓存（`history::listing::session_title_of`）。
pub(crate) fn live_mcp_failed(homes: &[PathBuf]) -> Vec<LiveMcpFailed> {
    let mut book = MCP_BOOK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let book = book.get_or_insert_with(McpBook::default);
    let mut seen: Vec<PathBuf> = Vec::new();
    let mut out: Vec<LiveMcpFailed> = Vec::new();
    for home in homes {
        for (pid, v) in pidfiles(home) {
            if crate::agents::pidfile_background(&v)
                || !crate::platform::proc::session_alive(pid, parse_procstart_ticks(&v))
            {
                continue;
            }
            let Some(sid) = v
                .get("sessionId")
                .and_then(|x| x.as_str())
                .filter(|s| !s.is_empty())
            else {
                continue;
            };
            let Ok(path) = super::history_query::session_record(home, sid) else {
                continue;
            };
            seen.push(path.clone());
            let Ok(said) = book.read(&path) else {
                continue;
            };
            let mut title: Option<String> = None;
            for m in said {
                if m.status != crate::agents::McpStatus::Failed {
                    continue;
                }
                let at_ms =
                    m.at.as_deref()
                        .and_then(crate::common::time::parse_iso8601_ms);
                if out.iter().any(|o| o.name == m.name && o.at_ms >= at_ms) {
                    continue;
                }
                out.retain(|o| o.name != m.name);
                let title = title
                    .get_or_insert_with(|| crate::history::listing::session_title_of(&path))
                    .clone();
                out.push(LiveMcpFailed {
                    name: m.name,
                    at_ms,
                    detail: m.detail,
                    title,
                });
            }
        }
    }
    book.keep_only(&seen);
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// 这台此刻活着的交互会话的工作目录（判活同 [`live_session_ids`]；后台任务不算），去重、排好序。计划读面从这里找工作区。
pub(crate) fn live_cwds(agent_home: &Path) -> Vec<String> {
    let mut v: Vec<String> = pidfiles(agent_home)
        .into_iter()
        .filter(|(_, v)| !crate::agents::pidfile_background(v))
        .filter(|(pid, v)| crate::platform::proc::session_alive(*pid, parse_procstart_ticks(v)))
        .filter_map(|(_, v)| {
            v.get("cwd")
                .and_then(|x| x.as_str())
                .filter(|c| !c.is_empty())
                .map(str::to_string)
        })
        .collect();
    v.sort();
    v.dedup();
    v
}

/// 此刻持着 `sid` 的活交互进程 pid（同一批 pidfile、同 [`live_session_ids`] 的判活；后台任务不算），升序。
/// 不止一个 ⇒ 同一条会话有几个进程在同时写。
pub(crate) fn session_writers(agent_home: &Path, sid: &str) -> Vec<u32> {
    let mut pids: Vec<u32> = pidfiles(agent_home)
        .into_iter()
        .filter(|(_, v)| v.get("sessionId").and_then(|x| x.as_str()) == Some(sid))
        .filter(|(_, v)| !crate::agents::pidfile_background(v))
        .filter(|(pid, v)| crate::platform::proc::session_alive(*pid, parse_procstart_ticks(v)))
        .map(|(pid, _)| pid)
        .collect();
    pids.sort_unstable();
    pids
}

/// **这条会话此刻谁持着它**（冷读那几处判活只问这一处）：`record` 是这条会话的记录（已过围栏），
/// 哪一家照它落在哪棵树下认（`agents::record_kind_of`），怎么判照那一家声明的（[`crate::agents::Adapter::live`]，见 [`session_holders_by`]）。
pub(crate) fn session_holders(agent_home: &Path, record: &Path) -> Option<Vec<u32>> {
    let kind = crate::agents::record_kind_of(record)?;
    session_holders_by(crate::agents::live_by(kind), agent_home, record)
}

/// [`session_holders`] 按判活方式那一半（判据直接喂方式）：
/// - pidfile ⇒ 这台 pidfile 里持着这个 sid（记录文件名）的活交互进程（[`session_writers`]）；
/// - 开着写 ⇒ 此刻开着这份记录写的进程（`platform::writers`；这个平台认不出写者 ⇒ 判不了）；
/// - 判不了 ⇒ `None`。
///
/// `Some(空)` ＝ 判过了、此刻没有谁持着它（走了）；`None` ＝ 判不了（不是「没有」）。pid 升序。
pub(crate) fn session_holders_by(
    live: crate::agents::LiveBy,
    agent_home: &Path,
    record: &Path,
) -> Option<Vec<u32>> {
    use crate::agents::LiveBy;
    match live {
        LiveBy::Pidfiles => Some(session_writers(agent_home, record.file_stem()?.to_str()?)),
        LiveBy::HeldOpen => {
            let want = crate::platform::writers::absolute(record);
            let found = crate::platform::writers::writers_of(&|p| p == want)?;
            Some(
                found
                    .get(&want)
                    .map(|w| w.iter().map(|w| w.pid).collect())
                    .unwrap_or_default(),
            )
        }
        LiveBy::Unknown => None,
    }
}

/// 那台 pidfile 说这条会话**此刻在等人**（适配层翻成 [`crate::agents::SessionActivity::NeedsYou`]）：等的是什么框（适配层翻好的 [`crate::agents::WaitOn`]）· 从何时起等（`statusUpdatedAt`，epoch ms）。
/// 判活同 [`session_writers`]；不在等 / 没有活进程持着它 ⇒ `None`。几个进程同时持着、有一个在等 ⇒ 取它（等得最早的那个）。
/// 「等的是什么」不在这里判：配上记录里那个还没有结果的工具调用，在 `facts_query::needs_of`。
pub(crate) fn session_wait(agent_home: &Path, sid: &str) -> Option<super::facts_query::PidWait> {
    pidfiles(agent_home)
        .into_iter()
        .filter(|(_, v)| v.get("sessionId").and_then(|x| x.as_str()) == Some(sid))
        .filter(|(_, v)| {
            crate::agents::pidfile_activity(v) == Some(crate::agents::SessionActivity::NeedsYou)
        })
        .filter(|(_, v)| !crate::agents::pidfile_background(v))
        .filter(|(pid, v)| crate::platform::proc::session_alive(*pid, parse_procstart_ticks(v)))
        .map(|(_, v)| super::facts_query::PidWait {
            waiting_for: crate::agents::pidfile_wait(&v),
            since_ms: v.get("statusUpdatedAt").and_then(serde_json::Value::as_u64),
            read_at_ms: read_at_ms(),
        })
        .min_by_key(|w| w.since_ms.unwrap_or(u64::MAX))
}

/// 此刻（epoch ms，这台的钟）：读 pidfile 那一刻，「已等多久」拿它减同一份 pidfile 里的起点。
fn read_at_ms() -> u64 {
    crate::common::time::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
}

/// 那台 pidfile 说这条会话**一轮停了、后台命令还在跑**（适配层翻成 [`crate::agents::SessionActivity::BackgroundWork`]）⇒
/// `Some(那个进程何时起的，epoch ms；没写 ⇒ None)`；不是这一态 / 没有活进程持着它 ⇒ `None`。判活同 [`session_writers`]；
/// 几个进程同时持着 ⇒ 取起得最早的那个（它留下的后台命令最多）。
pub(crate) fn session_background(agent_home: &Path, sid: &str) -> Option<Option<u64>> {
    pidfiles(agent_home)
        .into_iter()
        .filter(|(_, v)| v.get("sessionId").and_then(|x| x.as_str()) == Some(sid))
        .filter(|(_, v)| {
            crate::agents::pidfile_activity(v)
                == Some(crate::agents::SessionActivity::BackgroundWork)
        })
        .filter(|(_, v)| !crate::agents::pidfile_background(v))
        .filter(|(pid, v)| crate::platform::proc::session_alive(*pid, parse_procstart_ticks(v)))
        .map(|(_, v)| v.get("startedAt").and_then(serde_json::Value::as_u64))
        .min_by_key(|b| b.unwrap_or(u64::MAX))
}

/// 一个活会话此刻在干什么 —— 与主窗口标签页同一判：活动态（pidfile，适配层翻好的）· 在等你时等的是什么。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct Doing {
    /// 说不清 ⇒ `None`（主窗口那颗点照「在跑」画）。
    pub(crate) activity: Option<crate::agents::SessionActivity>,
    /// 只在 `activity` 是在等你时有：配上记录里没结果的那一步判的种类（[`super::facts_query::needs_of`]）。
    pub(crate) needs: Option<super::facts_query::NeedsKind>,
}

/// 这台此刻活着的每个会话在干什么（判活同 [`live_session_ids`]）。同一会话几个进程持着 ⇒ 在等人 ＞ 在跑 ＞ 后台命令在跑 ＞ 空闲 ＞ 说不清。
/// 在等你 ⇒ 等的是什么同 [`waiting_needs`]（只留种类）。
pub(crate) fn live_doing(agent_home: &Path) -> std::collections::BTreeMap<String, Doing> {
    live_activity(agent_home)
        .into_iter()
        .map(|(sid, activity)| {
            let needs = (activity == Some(crate::agents::SessionActivity::NeedsYou))
                .then(|| waiting_needs(agent_home, &sid).map(|n| n.kind))
                .flatten();
            (sid, Doing { activity, needs })
        })
        .collect()
}

/// **这台上需手动的会话**：此刻活着、那台说在等人的每一个（判活与「在等人」同 [`live_doing`]），带它在等什么（[`waiting_needs`]，不另判），
/// 先答的在前（`needs.rank` 小的先，[`super::facts_query::needs_rank`]）。
/// `asked` ＝ 此刻有待答请求挂着的会话（常驻进程那一份登记表，顶层壳交进来）；`dress` ＝ 给一条 `needs` 配上它挂着的请求（同上）。
/// `hidden(sid, 是不是分身)` ＝ 那一格（顶层壳经 `control/session_hidden.rs` 判）。
pub(crate) fn live_needs(
    agent_home: &Path,
    asked: &[String],
    dress: &dyn Fn(Option<super::facts_query::Needs>, &str) -> Option<super::facts_query::Needs>,
    hidden: &dyn Fn(&str, bool) -> Option<cmd_wire::session::Hidden>,
) -> Vec<NeedsRow> {
    let live = live_activity(agent_home);
    let bg = live_background(agent_home);
    // 那台说在等的 ∪ 有请求挂着的（挡着终端的那种 pidfile 不说在等，照样是要人答的）；后者只收此刻活着的会话。
    let mut sids: Vec<String> = live
        .iter()
        .filter(|(_, a)| **a == Some(crate::agents::SessionActivity::NeedsYou))
        .map(|(sid, _)| sid.clone())
        .collect();
    for sid in asked {
        if live.contains_key(sid) && !sids.contains(sid) {
            sids.push(sid.clone());
        }
    }
    let mut rows = sids
        .into_iter()
        .filter_map(|sid| {
            let waiting = live.get(&sid) == Some(&Some(crate::agents::SessionActivity::NeedsYou));
            Some(NeedsRow {
                needs: dress(
                    waiting.then(|| waiting_needs(agent_home, &sid)).flatten(),
                    &sid,
                )?,
                hidden: hidden(&sid, bg.contains(&sid)),
                sid,
            })
        })
        .collect::<Vec<_>>();
    // 先答哪个照核心那一处（同一档、同样久 ⇒ 按会话 id，稳定）。
    rows.sort_by(|a, b| {
        a.needs
            .rank
            .cmp(&b.needs.rank)
            .then_with(|| a.sid.cmp(&b.sid))
    });
    rows
}

/// 这条会话此刻在不在等人：那台说在等（同 [`live_needs`] 那一判）或有挂着的待答请求（`asked`，只算此刻活着的）。
pub(crate) fn waiting_now(agent_home: &Path, sid: &str, asked: &[String]) -> bool {
    match live_activity(agent_home).get(sid) {
        Some(Some(crate::agents::SessionActivity::NeedsYou)) => true,
        Some(_) => asked.iter().any(|a| a == sid),
        None => false,
    }
}

/// 这台此刻活着的分身会话（pidfile 那一家说是后台的，`agents::pidfile_background`）。
pub(crate) fn live_background(agent_home: &Path) -> std::collections::BTreeSet<String> {
    pidfiles(agent_home)
        .into_iter()
        .filter(|(pid, v)| {
            crate::agents::pidfile_background(v)
                && crate::platform::proc::session_alive(*pid, parse_procstart_ticks(v))
        })
        .filter_map(|(_, v)| {
            v.get("sessionId")
                .and_then(|x| x.as_str())
                .filter(|s| !s.is_empty())
                .map(str::to_string)
        })
        .collect()
}

/// 这台此刻活着的每个会话的活动态（同一会话几个进程持着 ⇒ 取最要紧的那一个，序见 [`live_doing`]）。
fn live_activity(
    agent_home: &Path,
) -> std::collections::BTreeMap<String, Option<crate::agents::SessionActivity>> {
    use crate::agents::SessionActivity as A;
    let rank = |a: Option<A>| match a {
        Some(A::NeedsYou) => 4,
        Some(A::Working) => 3,
        Some(A::BackgroundWork) => 2,
        Some(A::Idle) => 1,
        None => 0,
    };
    let mut out: std::collections::BTreeMap<String, Option<A>> = std::collections::BTreeMap::new();
    for (pid, v) in pidfiles(agent_home) {
        if !crate::platform::proc::session_alive(pid, parse_procstart_ticks(&v)) {
            continue;
        }
        let Some(sid) = v
            .get("sessionId")
            .and_then(|x| x.as_str())
            .filter(|s| !s.is_empty())
        else {
            continue;
        };
        let activity = crate::agents::pidfile_activity(&v);
        let cell = out.entry(sid.to_string()).or_default();
        if rank(activity) > rank(*cell) {
            *cell = activity;
        }
    }
    out
}

/// 一条在等人的会话此刻要人做什么：读它的记录（扫描图缓存）配上没结果的调用，经 [`super::facts_query::needs_of`] 判（同 `history-facts.needs`）；
/// 记录找不到 ⇒ 不挂哪一步，框是哪种照那台说的（那台说在等 ⇒ 恒有一份）。
fn waiting_needs(agent_home: &Path, sid: &str) -> Option<super::facts_query::Needs> {
    let pending = super::history_query::session_record(agent_home, sid)
        .ok()
        .and_then(|p| super::history_query::cold_scan(agent_home, &p.to_string_lossy()).ok())
        .map(|m| m.pending().to_vec())
        .unwrap_or_default();
    let wait = session_wait(agent_home, sid).unwrap_or(super::facts_query::PidWait {
        waiting_for: None,
        since_ms: None,
        read_at_ms: 0,
    });
    super::facts_query::needs_of(&pending, Some(&wait))
}

/// 正在跑的会话各属哪个号：扫 `<claude_dir>/sessions/<PID>.json`，每条一行。
pub(crate) fn session_accounts(agent_home: &Path, accts_dir: &Path) -> Vec<String> {
    // 向注册表要「会话进程环境里该读哪两个键」只问这一次（账号 · 上游地址；账号库那一家的 `AccountsFace.session_env`）。
    let Some(env_keys) = crate::agents::account_library_face().map(|f| f.session_env) else {
        return Vec::new();
    };
    // `None` 这个 key 是账号 0（configDir 缺席）：裸起的会话归属到它。
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
    for (pid, v) in pidfiles(agent_home) {
        let sid = v.get("sessionId").and_then(|x| x.as_str());
        let cwd = v.get("cwd").and_then(|x| x.as_str());
        // 判活必须过 procStart 身份对拍：PID 会被复用，只按 /proc/<pid> 存在性判活会把已死会话误贴成「活着 + 别人的账号」。
        // 身份不符 → 当作该会话已死，不读 environ、不归属。
        let alive = session_process_identity_ok(pid, &v);
        // 环境这一刻取不到（读失败，或读回 0 字节：exec 窗口 / 僵尸进程）单独一支：不归属、不算裸起。
        // 不然它会走成 `cfg_norm = None`、在下面的 `by_dir` 里正好撞上账号 0 那个 `None` 键 ⇒ 一条真跑在账号 Z 下的会话被报成账号 0 的。
        // 另两支（键不在 / 值是空串）仍然合并，理由在 `EnvRead` 的类型头注。
        let (cfg, cfg_env_unreadable) = if alive {
            match proc_env_var(pid, env_keys.config_dir) {
                EnvRead::Value(v) => (Some(v), false),
                EnvRead::Unset => (None, false),
                EnvRead::Unreadable => (None, true),
            }
        } else {
            // 进程已死时一个字节都不读 ⇒ 谈不上「取不到」，那一格照旧是「没读」。
            (None, false)
        };
        // 第二个键：只折成「走不走本机中转」一个布尔；值带钥匙，这一行之后就丢掉。
        let via_relay = if alive {
            match proc_env_var(pid, env_keys.base_url) {
                // 环境里是中转地址 ≠ 真走中转：agent 自己的设置文件可能压过它（`settings_may_set_base_url`）⇒ 那时说不清。
                EnvRead::Value(v) if relay_route_core::split_keyed_base_url(&v).is_some() => {
                    let settings_win = (env_keys.settings_may_set_base_url)(
                        cfg.as_deref().map(Path::new),
                        agent_home,
                        cwd.map(Path::new),
                    );
                    (!settings_win).then_some(true)
                }
                EnvRead::Value(_) => Some(false),
                EnvRead::Unset => Some(false),
                EnvRead::Unreadable => None,
            }
        } else {
            None
        };
        let cfg_norm = cfg.as_deref().map(|c| norm_dir(c).to_string());
        // 归属：有 configDir 就逐字匹配；没有、进程确实活着、且环境这一刻读得到才是账号 0。
        // 进程已死时不归属（cfg 恒 None）；环境取不到时也不归属：那一刻不知道它设没设。
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
            via_relay,
        });
    }
    out.into_iter()
        .map(|r| {
            serde_json::json!({
                "pid": r.pid,
                "sessionId": json_str(r.sid.as_deref()),
                "cwd": json_str(r.cwd.as_deref()),
                "configDir": json_str(r.config_dir.as_deref()),
                "account": json_str(r.account.as_deref()),
                // `bare` = 进程活着（身份已确认）、环境读得到、且没设 `CLAUDE_CONFIG_DIR` —— 就是账号 0（上面的 `account` 会给出名字）。
                // 下游用它区分「账号 0」与「设了 configDir 的账号」；它只答账号这一维。
                "bare": r.alive && !r.cfg_env_unreadable && r.config_dir.is_none(),
                "alive": r.alive,
                // 走不走本机中转：`true` / `false` / `null`（不知道：进程已死或环境这一刻取不到）。
                // ⚠ 老后端不出这个键，下游读成 `null`（additive）。
                "viaRelay": r.via_relay,
            })
            .to_string()
        })
        .collect()
}

/// 目标账号（`configDir`）是否已信任该目录。
/// `configDir` 必须 ∈ manifest（否则这就成了任意文件读原语）。
fn account_trust(
    accts_dir: &Path,
    config_dir: &str,
    cwd: &str,
) -> Result<String, (String, String)> {
    if !is_safe_config_dir(config_dir) {
        return Err((
            "unsafe_config_dir".into(),
            copy_text("beAccountsQuery.accountTrust.unsafeDir", &[]).into(),
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
            copy_text("beAccountsQuery.accountTrust.notListed", &[]).into(),
        ));
    }
    trust_in(Path::new(want), cwd)
}

/// **账号 0** 的信任预检。
///
/// 为什么要单独一个入口而不是给上一个传个空 `configDir`：账号 0 **没有**
/// config dir，空串是被明令禁止的拼法（空值 ≠ 未设，见 `RawAccount::config_dir`）。
/// 而且它的 `.claude.json` 也不在共享库里 —— 原生根是 `$HOME`（身份表 `NATIVE_IDENTITY` 里它的原生根是家目录）⇒ 路径来源本就不同，
/// 用同一个入口只能靠哨兵值区分，那比多一个动词更容易出错。
fn account_trust_zero(cwd: &str) -> Result<String, (String, String)> {
    let home = home_dir().ok_or_else(|| {
        (
            "no_home".to_string(),
            copy_text("beAccountsQuery.accountTrustZero.noHome", &[]),
        )
    })?;
    trust_in(&home, cwd)
}

/// 账号库那一家（`AccountsFace.trust_in`）对某个配置根下某个 cwd 的信任状态。没有账号库那一家 ⇒ 照实拒。
/// 查的键与预标同一组：cwd 本身 ＋ 适配层说的那几个（[`crate::agents::TrustCells::dir_keys`]，如所在仓的根）；
/// 任一格信任过就算信任过，任一格记过就算记过（子目录里起、信任记在仓根上的，不误报「没信任」）。
fn trust_in(root: &Path, cwd: &str) -> Result<String, (String, String)> {
    let face = crate::agents::account_library_face().ok_or_else(|| {
        (
            "no_accounts_face".to_string(),
            "no agent here keeps an account library".to_string(),
        )
    })?;
    let mut keys = vec![cwd.to_string()];
    for k in face.trust.map(|c| (c.dir_keys)(cwd)).unwrap_or_default() {
        if !keys.contains(&k) {
            keys.push(k);
        }
    }
    let (mut trusted, mut known) = (false, false);
    for k in &keys {
        let line = (face.trust_in)(root, k)?;
        let v: serde_json::Value = serde_json::from_str(&line)
            .map_err(|e| ("trust_line_invalid".to_string(), e.to_string()))?;
        trusted |= v["trusted"] == serde_json::Value::Bool(true);
        known |= v["known"] == serde_json::Value::Bool(true);
    }
    Ok(
        serde_json::json!({"trusted": trusted, "known": known, "error": serde_json::Value::Null})
            .to_string(),
    )
}

/// 帧面那两条（`accounts-list` / `accounts-sessions`）的入口。
///
/// 跑的是 CLI 那两臂**同一个函数**（[`list_accounts`] / [`session_accounts`]），账号库目录
/// `machine-interrupts` 的成品：停 / 重启 / 更新 / 卸载这台的 cc-monitor 之前会打断什么。
/// `lines` 是这台活会话那几行（`accounts-sessions` 同一份）：活着且经本机中转 ⇒ `relayedSessions`；活着而说不清走不走 ⇒ `relayedMaybe`；
/// 活着的都算 `liveStreams`（停的那几秒 cc-monitor 里这几个不更新）。`forwards` 是这台后端账上通往 `machine` 的转发（没给 `machine` ⇒ 0）。
/// 读不懂的行跳过。**纯函数**。
pub(crate) fn machine_product(lines: &[String], forwards: u32) -> Interrupts {
    let (mut relayed, mut maybe, mut live) = (0u32, 0u32, 0u32);
    for l in lines {
        let Ok(row) = serde_json::from_str::<serde_json::Value>(l) else {
            continue;
        };
        if row["alive"] != serde_json::Value::Bool(true) {
            continue;
        }
        live += 1;
        match row.get("viaRelay") {
            Some(serde_json::Value::Bool(true)) => relayed += 1,
            Some(serde_json::Value::Bool(false)) => {}
            _ => maybe += 1,
        }
    }
    Interrupts {
        relayed_sessions: relayed,
        relayed_maybe: maybe,
        live_streams: live,
        forwards,
    }
}

/// `machine-interrupts` 的回包：停 / 重启 / 更新 / 卸载这台的 cc-monitor 之前会打断什么（[`machine_product`]）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(rename_all = "camelCase")]
pub(crate) struct Interrupts {
    /// 这台的活会话里经本机中转走请求的几个（停了就断，直到再启动）。
    pub(crate) relayed_sessions: u32,
    /// 活着、说不清走不走中转的几个（环境这一刻读不出 / agent 自己的设置可能压过它）。
    pub(crate) relayed_maybe: u32,
    /// 这台活着的会话数（停的那几秒 cc-monitor 里它们不更新）。
    pub(crate) live_streams: u32,
    /// 这台账上通往问的那台机器的转发（问的是重启 / 退出 cc-monitor ⇒ 账上全部）。
    pub(crate) forwards: u32,
}

/// 「现在重启 cc-monitor」之前那一问（`machine-interrupts` 带 `appExit`）：这台后端随 cc-monitor 退出一起停（`kill` ＝ 它自己那份退出行为）
/// ⇒ 这台的活会话照 [`machine_product`] 数、账上全部转发（`all_forwards`）都断；留着 ⇒ 什么都不断。**纯函数**。
pub(crate) fn app_exit_product(kill: bool, lines: &[String], all_forwards: u32) -> Interrupts {
    if kill {
        machine_product(lines, all_forwards)
    } else {
        machine_product(&[], 0)
    }
}

/// 帧面那几问要的「哪个会话在用哪个号」（[`session_accounts`]，只跟着家走）。
pub(crate) fn session_lines_for_frame(agent_home: &Path) -> Vec<String> {
    session_accounts(agent_home, &resolve_accts_dir())
}

/// `accounts-trust` 的回包。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct Trust {
    /// 这个账号接受过该目录的信任对话框。
    pub(crate) trusted: bool,
    /// 这个账号的用户级配置里有该目录的记录（`false` ⇒ 首次进入，大概率会弹确认）。
    pub(crate) known: bool,
}

/// 帧面 `accounts-trust`：换号前的信任预检。`config_dir == None` ⇒ 账号 0（路径写死在 `$HOME`，不收路径参数）；
/// 否则必须 ∈ manifest（否则就成了任意文件读原语）。拒绝码原样上交。成品 `{trusted, known}`。
pub(crate) fn trust_product(
    config_dir: Option<&str>,
    cwd: &str,
) -> Result<Trust, (String, String)> {
    trust_product_at(&resolve_accts_dir(), config_dir, cwd)
}

/// [`trust_product`] 的本体，账号库目录是参数（判据拿夹具喂它）。账号 0 那一形仍读真 `$HOME`（它不收路径）。
pub(crate) fn trust_product_at(
    accts_dir: &Path,
    config_dir: Option<&str>,
    cwd: &str,
) -> Result<Trust, (String, String)> {
    let line = match config_dir {
        None => account_trust_zero(cwd)?,
        Some(c) => account_trust(accts_dir, c, cwd)?,
    };
    // 本体的出参是一行 JSON（`trust_of_config` 造的）；帧面只取那两格。缺一格 ⇒ 契约坏了，不猜。
    let v: serde_json::Value = serde_json::from_str(&line).map_err(|e| {
        (
            "failed".to_string(),
            crate::common::said::IntoNote::into_note(crate::common::said::Said::with_raw(
                copy_text("beAccountsQuery.trustProductAt.unparsable", &[]),
                e,
            )),
        )
    })?;
    match (v["trusted"].as_bool(), v["known"].as_bool()) {
        (Some(trusted), Some(known)) => Ok(Trust { trusted, known }),
        _ => Err((
            "failed".to_string(),
            crate::common::contract::malformed("trust line lacks `trusted` / `known`"),
        )),
    }
}

// ---------------------------------------------------------------- tests

// 照类型造清单成品的夹具住测试树：起会话挑号 · 需手动清单几份判据共用。
#[cfg(test)]
#[path = "../../../tests/backend/observe/accounts_list_support.rs"]
pub(crate) mod test_support;

#[cfg(test)]
#[path = "../../../tests/backend/observe/accounts_query_tests.rs"]
mod tests;
