//! 多账号（账号库：每个号一个配置目录，共享项链回同一个配置根）的只读消费侧（写侧是 `accounts/manage/`）。
//!
//! 帧面那一条（`inbound::REGISTRY` 的账号族，宿主 `faces/read_face.rs`）：
//! - `accounts-list` → `{meta, accounts, notice}`（[`list_product_with`]）。
//!
//! # 账号 0
//!
//! manifest 里 `configDir` 键缺席的那一条 = 账号 0 =「不设 `CLAUDE_CONFIG_DIR`」这个状态本身。结构性地认它（看键在不在），
//! 不认名字（不硬编码 "0"）—— 认法住读口那一面（`acct_core::ManifestAccount`）。它的 config dir 是共享库（`sharedStore`）、`.claude.json` 在 `$HOME`。
//! 空串不算缺席：`configDir: ""` 过不了配置目录那一关，那一条不列（空值 ≠ 未设）。
//!
//! # 读清单
//!
//! 清单只经 `accounts/manage` 那一个读口（`scan::manifest_at`：剥 BOM · 读取上限 · 版本闸都在那里），每趟只读一次，本文件不另解析。
//!
//! # 谓词的作用面不对称
//!
//! 清单侧的配置目录都过了读口那一关（`is_safe_config_dir` 同一条规则，过不了的那条不列）；会话那几行（[`session_accounts`]）的 `configDir` 来自
//! `/proc/<pid>/environ`，没过谓词。不给它套谓词：进程侧的值只拿去与 `by_dir`（只装过了谓词的目录）比对，
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
//! - `/proc/<pid>/environ` 只抠两个写死的键（账号配置目录 · `ANTHROPIC_BASE_URL`，键名由适配层给），不回传整个环境快照。
//!   `ANTHROPIC_BASE_URL` 的值带中转钥匙 ⇒ 只折成一个布尔（`viaRelay`），值本身不出参、不进日志。键名不接受调用方传进来
//!   （否则就退化成「任意环境变量读」原语）；判据数着本文件生产段里读环境的调用点，多一处 ⇒ 红。

use crate::common::cells::Words;
use crate::history::live_table::LiveTable;
use crate::platform::fs::Ro;
use crate::wire::SessionFaceExt as _;
use acct_core::{
    auth_kind_from_manifest, auth_kind_with_apikey_table, auth_ready, CREDENTIALS_NAME,
};
pub(crate) use cmd_wire::accounts::{AccountRow, AccountsList, Badge, ListMeta, ScannedAccount};
pub(crate) use cmd_wire::needs::NeedsRow;
use copy_core::copy_text;
use relay_route_core::{ACCOUNTS_DIR_REL, ACCOUNTS_MANIFEST_NAME};
use std::path::{Path, PathBuf};

use crate::accounts::manage::model::Manifest;
// ---------------------------------------------------------------- manifest

/// 清单里列出来的那几行：读口读出来的号表（`Manifest::book`，照清单顺序、含账号 0）。
fn listed(m: &Manifest) -> &[acct_core::ManifestAccount] {
    &m.book.accounts
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
        Some(h) => crate::platform::paths::home_join(&h, ACCOUNTS_DIR_REL),
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

/// 这台启用了多账号没有（账号库清单读得出 ⇒ 是）：与账号清单那一份的 `meta.enabled` 同一个判法（[`scan_accounts`]）。
/// 首次运行「给现在登录的号起个名字」那一步读它（`footprint/readiness.rs`）。
pub(crate) fn accounts_enabled_here() -> bool {
    manifest_or_why(&resolve_accts_dir()).is_ok()
}

/// 账号库清单在、却读不懂（读不出 · 不是 JSON · 版本不认）⇒ 为什么（同账号清单 `meta.error` 那一句）；读得懂 · 没有清单
/// （没启用多账号，正常状态）⇒ `None`。需手动清单据此回 `unreadable`，不把读不懂当成「没有需登录的号」。
pub(crate) fn library_unreadable(accts_dir: &Path) -> Option<String> {
    crate::accounts::manage::scan::manifest_at(&manifest_path(accts_dir))
        .err()
        .map(crate::accounts::manage::scan::Unread::into_said)
}

/// [`library_unreadable`]，这台的账号库。
pub(crate) fn library_unreadable_here() -> Option<String> {
    library_unreadable(&resolve_accts_dir())
}

/// 这台的清单（经 `accounts/manage` 那一个读口；剥 BOM · 上限 · 版本闸都在那里）。没有清单 / 在却读不懂都是 `Err(人话原因)`——
/// 调用方据此输出 `enabled:false` 而**不是**失败退出（"没启用多账号"是正常状态）。形状不合规矩的号原样留在清单里、不列（写侧也不动它）。
fn manifest_or_why(accts_dir: &Path) -> Result<Manifest, String> {
    let p = manifest_path(accts_dir);
    match crate::accounts::manage::scan::manifest_at(&p) {
        Ok(Some(m)) => Ok(m),
        // 没有清单 ＝ 没启用多账号（正常状态）：只回那一句，不记日志（记了就是每次列账号都多一行 WARN，CLI 的 stderr 也跟着脏）。
        Ok(None) => Err(copy_text(
            "beAccountsQuery.loadManifest.unreadable",
            &[
                ("path", &(p.display()).to_string()),
                ("why", &copy_core::io_reason(std::io::ErrorKind::NotFound)),
            ],
        )),
        // 屏上那一句只带原因词；系统原话记一行日志（这一形的失败是给账号页一句话，没有原话位）。
        Err(e) => Err(e.into_said()),
    }
}

// ---------------------------------------------------------------- /proc

use crate::platform::proc::{proc_env_var, EnvRead};

/// 一条会话进程跑在哪个号下（[`session_accounts`] 一个进程一行）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SessionAccount {
    pub(crate) pid: u32,
    pub(crate) sid: Option<String>,
    pub(crate) cwd: Option<String>,
    /// 进程环境里的配置目录（规范过的写法）；没设 · 不知道 ⇒ `None`。
    pub(crate) config_dir: Option<String>,
    pub(crate) account: Option<String>,
    /// 判活过了启动时刻对拍（活会话表 `start_checked`：PID 会被复用，错标签比缺标签危害大）。
    pub(crate) alive: bool,
    /// 进程活着（身份已确认）、环境读得到、且没设配置目录 —— 就是账号 0（`account` 给出名字）。
    /// 环境这一刻取不到（读失败 / 读回 0 字节）⇒ 假：不归属、也不算裸起。
    pub(crate) bare: bool,
    /// 这条会话的 `ANTHROPIC_BASE_URL` 是不是**本机中转那一形**（回环 ＋ 钥匙段 ＋ 路由，
    /// `relay_route_core::split_keyed_base_url` 认得出）。`None` = 不知道（进程已死 / 环境这一刻取不到）。
    /// 用途：机器页「停」本机后端之前数一数有几条会话会断。
    pub(crate) via_relay: Option<bool>,
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
    // 这一趟只读一次清单：号色 · 删掉默认号之后默认谁都从这一份算。
    let read = manifest_or_why(accts_dir);
    let next = read.as_ref().ok().and_then(next_default);
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
        next_default: next.clone(),
        // 这台的默认号是谁（标了的第一个，没标 ⇒ 第一个）：规则只住 `acct_core::effective_default`，界面与起会话都读这一格。
        effective_default,
    };
    match read {
        Err(e) => (meta(false, None, None, 0, Some(e), None), Vec::new()),
        Ok(m) => {
            let colors = colors_of(&m);
            let mut lines = Vec::new();
            for a in listed(&m) {
                // configDir 缺席 = 账号 0：它的 config dir 就是共享库 ⇒ 登录态查那儿，`configDir` 在帧里出 null（下游据此不注入 CLAUDE_CONFIG_DIR）。
                let (cfg_out, probe_dir) = match a.config_dir.as_deref() {
                    None => (None, m.shared_store.as_deref().map(PathBuf::from)),
                    // 配置目录过不了那一关的那条读口不列（[`listed`]）。
                    Some(c) => {
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
                    // 号色（`--acct-c0..c7` 第几格）：账号库里的事实，界面只画它（[`colors_of`]；不在表里的按名字哈希兜底）。
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

/// `accounts-list` 的入参住 `cmd-wire::accounts`（名字与回包挂在 `cmd_wire::Call` 上）。
pub(crate) use cmd_wire::accounts::AccountsListReq;

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

/// ★ 进不进新建会话的选号表（唯一一处）：隔离、有配置目录。账号 0 不经选号；原地模式的号不拿来起会话。
pub(crate) fn new_session_of(a: &ScannedAccount) -> bool {
    a.config_dir.is_some() && a.mode == "isolated"
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
/// 没有默认号 / 删了之后没有别的号 ⇒ `None`。
fn next_default(m: &Manifest) -> Option<String> {
    let current = m.managed().find(|a| a.is_default)?.name.clone();
    m.without(&current).1
}

/// 这台账号库给每个号的号色（名 ⇒ 第几格；读不出 ⇒ 空表）：别处画号色（轮换那一页）与 `accounts-list` 同一份；
/// 不在表里的号照 `color_fallback` 按名字算（同清单那一行）。
pub(crate) fn account_colors() -> std::collections::BTreeMap<String, u8> {
    manifest_or_why(&resolve_accts_dir())
        .map(|m| colors_of(&m))
        .unwrap_or_default()
}

/// 清单里每个号的号色（名 ⇒ 第几格）：盘上有就照盘上；还没补过色的照补色那一处现算（`Manifest::with_colors`，
/// 与常驻那一个补完写回去的同一份）。
fn colors_of(m: &Manifest) -> std::collections::BTreeMap<String, u8> {
    let m = m.with_colors();
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
/// `relay_running` ＝ 这台的中转在不在（徽章要它；调用方读本进程的监听状态 ⇒
/// 只有常驻进程答得出真话，`accounts-list` 在 `STREAM_ONLY`）。
pub(crate) fn list_product(
    rows: &[String],
    agent: &str,
    table_agent: &str,
    relay_running: bool,
) -> AccountsList {
    list_product_with(
        &resolve_accts_dir(),
        home_dir().as_deref(),
        rows,
        &crate::accounts::upstream_select::file_face::machine_key_facts(),
        agent,
        table_agent,
        relay_running,
    )
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
/// - 每个号 `configDirShown`：号目录的显示形，按 `home`（这台的家目录）写（[`crate::platform::paths::home_short`]），界面照抄。
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
    let (meta, scanned) = scan_accounts(accts_dir, &routed);
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
                config_dir_shown: a
                    .config_dir
                    .as_deref()
                    .map(|d| crate::platform::paths::home_short(d, home)),
                key_masked: fact.and_then(|k| k.masked.clone()),
                base_url: fact.and_then(|k| k.base_url.clone()),
                badge,
                plate: Words(crate::accounts::quota::name_words::plate_text(&a.name)),
                quota: None,
                unselectable: unselectable_of(&a, api_key).map(Words),
                login: false,
                new_session: new_session_of(&a),
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
    std::collections::BTreeMap<PathBuf, (u64, Vec<super::recognize::facts::McpTrouble>)>,
);

impl McpBook {
    /// 这条记录此刻的 MCP 那一格：从上次读到的地方接着读（记录比上次读到的短 ⇒ 换了一份，从头读）。
    pub(crate) fn read(
        &mut self,
        path: &Path,
    ) -> std::io::Result<Vec<super::recognize::facts::McpTrouble>> {
        use std::io::{Seek, SeekFrom};
        let mut file = Ro::open(path)?;
        let len = file.metadata()?.len();
        let (at, mcp) = self.0.entry(path.to_path_buf()).or_default();
        if len < *at {
            *at = 0;
            mcp.clear();
        }
        file.seek(SeekFrom::Start(*at))?;
        let kind = crate::agents::record_kind_of(path).unwrap_or_default();
        *at += super::recognize::facts::mcp_from(std::io::BufReader::new(file), kind, mcp)?;
        Ok(mcp.clone())
    }

    /// 只留这几条（这一轮不活了的那几条忘掉）。
    pub(crate) fn keep_only(&mut self, live: &[PathBuf]) {
        self.0.retain(|p, _| live.contains(p));
    }
}

/// 本进程那一本（常驻后端里跨请求留着；一次性 CLI 进程里每次都是空的，等于从头读）。
static MCP_BOOK: std::sync::Mutex<Option<McpBook>> = std::sync::Mutex::new(None);

/// 这台此刻活着的交互会话（`homes` 各号的家那一张活会话表，后台任务不算）里，
/// 那一家说断开的 MCP：每个名字取说它最晚的那一条，按名字排。会话结束了的不算（那份原话已经过时）。
/// 每条会话只读 MCP 那一格、只读新长出来的那一截（[`McpBook`]）；标题取历史清单的缓存，带上这台注解里改过的名（`history::listing::session_title_of`）。
pub(crate) fn live_mcp_failed(homes: &[PathBuf]) -> Vec<LiveMcpFailed> {
    let mut book = MCP_BOOK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let book = book.get_or_insert_with(McpBook::default);
    let mut seen: Vec<PathBuf> = Vec::new();
    let mut out: Vec<LiveMcpFailed> = Vec::new();
    // 这台的注解（叫什么带上改过的名）：真有断开的 MCP 要配标题时才读一次。
    let annotations = std::cell::OnceCell::new();
    for home in homes {
        let table = crate::history::live_table::now(home);
        for row in table.interactive() {
            let Some(sid) = row.sid() else {
                continue;
            };
            let Ok(path) = super::query::locate::session_record(home, sid) else {
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
                    .get_or_insert_with(|| {
                        let annotations = annotations.get_or_init(|| {
                            match crate::history::history_annotations::load() {
                                crate::history::history_annotations::Loaded::Read(t) => t,
                                _ => Default::default(),
                            }
                        });
                        crate::history::listing::session_title_of(
                            &path,
                            annotations.get(sid).and_then(|e| e.custom_title.clone()),
                        )
                    })
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

/// **这条会话此刻谁持着它**（冷读那几处判活只问这一处）：`record` 是这条会话的记录（已过围栏），
/// 哪一家照它落在哪棵树下认（`agents::record_kind_of`），怎么判照那一家声明的（[`crate::agents::Adapter::live`]，见 [`session_holders_by`]）。
pub(crate) fn session_holders(agent_home: &Path, record: &Path) -> Option<Vec<u32>> {
    let kind = crate::agents::record_kind_of(record)?;
    session_holders_by(crate::agents::live_by(kind), agent_home, record)
}

/// [`session_holders`] 按判活方式那一半（判据直接喂方式）：
/// - pidfile ⇒ 活会话表里持着这个 sid（记录文件名）的活交互进程（`LiveTable::writers`）；
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
        LiveBy::Pidfiles => {
            Some(crate::history::live_table::now(agent_home).writers(record.file_stem()?.to_str()?))
        }
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

/// 一个活会话此刻在干什么 —— 与主窗口标签页同一判：活动态（活会话表）· 在等你时等的是什么。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct Doing {
    /// 说不清 ⇒ `None`（主窗口那颗点照「在跑」画）。
    pub(crate) activity: Option<crate::agents::SessionActivity>,
    /// 只在 `activity` 是在等你时有：配上记录里没结果的那一步判的种类（[`super::recognize::facts::needs_of`]）。
    pub(crate) needs: Option<super::recognize::facts::NeedsKind>,
}

/// 这台此刻活着的每个会话在干什么（活会话表 `LiveTable::doing`）；在等你 ⇒ 等的是什么同 [`waiting_needs`]（只留种类）。
pub(crate) fn live_doing(agent_home: &Path) -> std::collections::BTreeMap<String, Doing> {
    let table = crate::history::live_table::now(agent_home);
    table
        .doing()
        .into_iter()
        .map(|(sid, activity)| {
            let needs = (activity == Some(crate::agents::SessionActivity::NeedsYou))
                .then(|| waiting_needs(agent_home, &table, &sid).map(|n| n.kind))
                .flatten();
            (sid, Doing { activity, needs })
        })
        .collect()
}

/// **这台上需手动的会话**：活会话表说在等人的每一个（那台说在等 ∪ 有请求挂着，`LiveTable::needs`），带它在等什么（[`waiting_needs`]，不另判），
/// 先答的在前（`needs.rank` 小的先，[`super::recognize::facts::needs_rank`]）。
/// `dress` ＝ 给一条 `needs` 配上它挂着的请求（顶层壳交进来）。`hidden(sid, 是不是分身)` ＝ 那一格（顶层壳经 `history/session_hidden.rs` 判）。
pub(crate) fn live_needs(
    agent_home: &Path,
    dress: &dyn Fn(
        Option<super::recognize::facts::Needs>,
        &str,
    ) -> Option<super::recognize::facts::Needs>,
    hidden: &dyn Fn(&str, bool) -> Option<cmd_wire::session::Hidden>,
) -> Vec<NeedsRow> {
    let table = crate::history::live_table::now(agent_home);
    let bg = table.background();
    let annotations = match crate::history::history_annotations::load() {
        crate::history::history_annotations::Loaded::Read(t) => t,
        _ => Default::default(),
    };
    let mut rows = table
        .needs()
        .into_iter()
        .filter_map(|sid| {
            Some(NeedsRow {
                label: crate::common::cells::Words(crate::history::session_hidden::title_here(
                    &sid,
                    &annotations,
                )),
                face: crate::wire::SessionFace::of(crate::wire::Life::Live {
                    activity: Some(crate::agents::SessionActivity::NeedsYou),
                    host: None,
                }),
                // 那台说在等 ⇒ 配上它在等什么；只挂着请求（挡着终端的那种 pidfile 不说在等）⇒ 交 `dress` 配请求。
                needs: dress(
                    table
                        .says_waiting(&sid)
                        .then(|| waiting_needs(agent_home, &table, &sid))
                        .flatten(),
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

/// 一条在等人的会话此刻要人做什么：读它的记录（扫描图缓存）配上没结果的调用，经 [`super::recognize::facts::needs_of`] 判（同 `history-facts.needs`）；
/// 记录找不到 ⇒ 不挂哪一步，框是哪种照那台说的（那台说在等 ⇒ 恒有一份）。
fn waiting_needs(
    agent_home: &Path,
    table: &LiveTable,
    sid: &str,
) -> Option<super::recognize::facts::Needs> {
    let record = super::query::locate::session_record(agent_home, sid).ok();
    let pending = record
        .as_ref()
        .and_then(|p| {
            super::query::locate::facts(agent_home, &p.to_string_lossy(), &Vec::new(), None).ok()
        })
        .map(|f| f.pending)
        .unwrap_or_default();
    // 找不到记录 ⇒ 按写那份 pidfile 的那一家认工具（哪一家跟着那一行走，`ProcFact::kind`）。
    let kind = record
        .as_deref()
        .and_then(crate::agents::record_kind_of)
        .or_else(|| table.kind_of(sid))
        .unwrap_or_default();
    let wait = table
        .wait_of(sid)
        .unwrap_or(super::recognize::facts::PidWait {
            waiting_for: None,
            since_ms: None,
            read_at_ms: 0,
        });
    super::recognize::facts::needs_of(kind, &pending, Some(&wait))
}

/// 这条会话跑在哪个号的家目录下：活会话表里持着它、启动时刻核过的那个进程 ⇒ 读进程环境里的配置目录（没设 ⇒ 默认家目录，注册表 `agents::home_at`）。
/// 找不到 · 进程不在了 · 环境读不到 ⇒ `None`（不猜）。
pub(crate) fn session_home(table: &LiveTable, sid: &str) -> Option<PathBuf> {
    let face = crate::agents::account_library_face()?;
    let pid = table.checked_pid(sid)?;
    let owner = owner_of(pid, face.session_env.config_dir, &[]);
    if owner.env_unreadable {
        return None;
    }
    Some(crate::agents::home_at(
        owner.config_dir.as_deref().map(Path::new),
        false,
    ))
}

/// 家目录 `home` 下的 `<rel>` 跟这台账号库里别的号共用同一份（[`shared_with_other_accounts_in`]）。
pub(crate) fn shared_with_other_accounts(home: &Path, rel: &str) -> bool {
    shared_with_other_accounts_in(&resolve_accts_dir(), home, rel)
}

/// 看的是链接布局：`home` 下那一份解开链接后的真身，跟账号库里另一个号（家目录不是同一个）的那一份是同一份 ⇒ 共用。
/// 它自己一份 · 没有账号库 · 这份文件还没有（解不开）⇒ `false`。
pub(crate) fn shared_with_other_accounts_in(accts_dir: &Path, home: &Path, rel: &str) -> bool {
    use crate::common::fence::real_path;
    let (Some(me), Some(my_home)) = (real_path(&home.join(rel)), real_path(home)) else {
        return false;
    };
    dirs_of(accts_dir).iter().any(|(d, _)| {
        let other = crate::agents::home_at(d.as_deref().map(Path::new), false);
        real_path(&other).is_some_and(|h| h != my_home)
            && real_path(&other.join(rel)).is_some_and(|f| f == me)
    })
}

/// 账号库里每个号的配置目录（规范过的写法）⇒ 号名；`None` 这个 key 是账号 0（configDir 缺席）：裸起的会话归属到它。
fn dirs_of(accts_dir: &Path) -> Vec<(Option<String>, String)> {
    manifest_or_why(accts_dir)
        .map(|m| {
            listed(&m)
                .iter()
                .map(|a| {
                    let dir = a.config_dir.as_deref().map(|c| norm_dir(c).to_string());
                    (dir, a.name.clone())
                })
                .collect()
        })
        .unwrap_or_default()
}

/// 一个活进程跑在哪个号下（[`owner_of`]）。
#[derive(Default)]
struct Owner {
    /// 进程环境里的配置目录（规范过的写法）；没设 ⇒ `None`。
    config_dir: Option<String>,
    account: Option<String>,
    /// 环境这一刻取不到（读失败，或读回 0 字节：exec 窗口 / 僵尸进程）：不归属、不算裸起。
    /// 不然它会走成「没设」、正好撞上账号 0 那个 `None` 键 ⇒ 一条真跑在账号 Z 下的会话被报成账号 0 的。
    env_unreadable: bool,
}

/// 读活进程 `pid` 环境里的配置目录键，对上账号库（[`dirs_of`]）：有就逐字匹配，没设就是账号 0。
/// 键不在与值是空串合并成「没设」，理由在 `EnvRead` 的类型头注。
fn owner_of(pid: u32, key: &str, by_dir: &[(Option<String>, String)]) -> Owner {
    let cfg = match proc_env_var(pid, key) {
        EnvRead::Value(v) => Some(norm_dir(&v).to_string()),
        EnvRead::Unset => None,
        EnvRead::Unreadable => {
            return Owner {
                env_unreadable: true,
                ..Owner::default()
            }
        }
    };
    let account = by_dir
        .iter()
        .find(|(d, _)| d.as_deref() == cfg.as_deref())
        .map(|(_, n)| n.clone());
    Owner {
        config_dir: cfg,
        account,
        env_unreadable: false,
    }
}

/// 一条刚宣告的会话跑在哪个号下：那一家有账号库（`face`）才判；判不了 ⇒ `None`。会话宣告带的就是它。
pub(crate) fn account_of_process(
    pid: u32,
    face: &crate::agents::AccountsFace,
    accts_dir: &Path,
) -> Option<String> {
    owner_of(pid, face.session_env.config_dir, &dirs_of(accts_dir)).account
}

/// 这台机器上的账号库目录（宣告时判号读它）。
pub(crate) fn default_accts_dir() -> PathBuf {
    resolve_accts_dir()
}

/// 正在跑的会话各属哪个号：活会话表（`history::live_table`）每个进程一行。
pub(crate) fn session_accounts(agent_home: &Path, accts_dir: &Path) -> Vec<SessionAccount> {
    // 向注册表要「会话进程环境里该读哪两个键」只问这一次（账号 · 上游地址；账号库那一家的 `AccountsFace.session_env`）。
    let Some(env_keys) = crate::agents::account_library_face().map(|f| f.session_env) else {
        return Vec::new();
    };
    let by_dir = dirs_of(accts_dir);
    let table = crate::history::live_table::now(agent_home);
    table
        .rows()
        .iter()
        .map(|r| {
            let pid = r.pid;
            let cwd = r.fact.cwd.as_deref();
            // 判活必须过启动时刻对拍（表里那一格）：PID 会被复用，只按存在性判活会把已死会话误贴成「活着 + 别人的账号」。
            // 身份不符 → 当作该会话已死，不读 environ、不归属。
            let alive = r.start_checked;
            // 进程已死时一个字节都不读 ⇒ 谈不上「取不到」，那一格照旧是「没读」。
            let owner = if alive {
                owner_of(pid, env_keys.config_dir, &by_dir)
            } else {
                Owner::default()
            };
            // 第二个键：只折成「走不走本机中转」一个布尔；值带钥匙，这一行之后就丢掉。
            let via_relay = if alive {
                match proc_env_var(pid, env_keys.base_url) {
                    // 环境里是中转地址 ≠ 真走中转：agent 自己的设置文件可能压过它（`settings_may_set_base_url`）⇒ 那时说不清。
                    EnvRead::Value(v) if relay_route_core::split_keyed_base_url(&v).is_some() => {
                        let settings_win = (env_keys.settings_may_set_base_url)(
                            owner.config_dir.as_deref().map(Path::new),
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
            SessionAccount {
                pid,
                sid: r.fact.sid.clone().filter(|s| !s.is_empty()),
                cwd: r.fact.cwd.clone().filter(|s| !s.is_empty()),
                bare: alive && !owner.env_unreadable && owner.config_dir.is_none(),
                config_dir: owner.config_dir,
                account: owner.account,
                alive,
                via_relay,
            }
        })
        .collect()
}

/// `machine-interrupts` 的成品：停 / 重启 / 更新 / 卸载这台的 cc-monitor 之前会打断什么。
/// `rows` 是这台活会话那几行（[`session_accounts`]）：活着且经本机中转 ⇒ `relayedSessions`；活着而说不清走不走 ⇒ `relayedMaybe`；
/// 活着的都算 `liveStreams`（停的那几秒 cc-monitor 里这几个不更新）。`forwards` 是这台后端账上通往 `machine` 的转发（没给 `machine` ⇒ 0）。
/// **纯函数**。
pub(crate) fn machine_product(rows: &[SessionAccount], forwards: u32) -> Interrupts {
    let (mut relayed, mut maybe, mut live) = (0u32, 0u32, 0u32);
    for r in rows.iter().filter(|r| r.alive) {
        live += 1;
        match r.via_relay {
            Some(true) => relayed += 1,
            Some(false) => {}
            None => maybe += 1,
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
pub(crate) fn app_exit_product(
    kill: bool,
    rows: &[SessionAccount],
    all_forwards: u32,
) -> Interrupts {
    if kill {
        machine_product(rows, all_forwards)
    } else {
        machine_product(&[], 0)
    }
}

/// 帧面那几问要的「哪个会话在用哪个号」（[`session_accounts`]，账号库取这台的那一份）。
pub(crate) fn session_accounts_here(agent_home: &Path) -> Vec<SessionAccount> {
    session_accounts(agent_home, &resolve_accts_dir())
}

// ---------------------------------------------------------------- tests

// 照类型造清单成品的夹具住测试树：起会话挑号 · 需手动清单几份判据共用。
#[cfg(test)]
#[path = "../../../tests/backend/observe/accounts_list_support.rs"]
pub(crate) mod test_support;

#[cfg(test)]
#[path = "../../../tests/backend/observe/accounts_query_tests.rs"]
mod tests;
