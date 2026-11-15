//! 账号库的只读那一面：账号清单成品 · 会话归属哪个号（写侧是本目录的 `exec` · `wire` 那几块）。
//!
//! 帧面那几条（宿主 `faces/read_face.rs` · `faces/session_account_face.rs` …）：
//! - `accounts-list` → `{meta, accounts, notice}`（[`list_product_with`]）；
//! - 正在跑的会话各属哪个号（[`session_accounts`]）· 停这台之前会打断什么（[`machine_product`]）。
//! 会话宣告时判号（[`account_of_process_here`]）由进程入口接进观测侧那一格（观测侧不认识账号库）。
//!
//! # 账号 0
//!
//! manifest 里 `configDir` 键缺席的那一条 = 账号 0 =「不设 `CLAUDE_CONFIG_DIR`」这个状态本身。结构性地认它（看键在不在），
//! 不认名字（不硬编码 "0"）—— 认法住读口那一面（`acct_core::ManifestAccount`）。它的 config dir 是共享库（`sharedStore`）、`.claude.json` 在 `$HOME`。
//! 空串不算缺席：`configDir: ""` 过不了配置目录那一关，那一条不列（空值 ≠ 未设）。
//!
//! # 读清单
//!
//! 清单只经 [`super::scan::manifest_at`] 那一个读口（剥 BOM · 读取上限 · 版本闸都在那里），每趟只读一次，本文件不另解析。
//!
//! # 谓词的作用面不对称
//!
//! 清单侧的配置目录都过了读口那一关（`acct_core::config_dir_ok`，过不了的那条不列）；会话那几行（[`session_accounts`]）的 `configDir` 来自
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
//! - 凭据文件（那个号那一家声明的 `LoginFace.creds_file`）只 stat 存在性，绝不读内容。
//! - `/proc/<pid>/environ` 只抠两个写死的键（账号配置目录 · `ANTHROPIC_BASE_URL`，键名由适配层给），不回传整个环境快照。
//!   `ANTHROPIC_BASE_URL` 的值带中转钥匙 ⇒ 只折成一个布尔（`viaRelay`），值本身不出参、不进日志。键名不接受调用方传进来
//!   （否则就退化成「任意环境变量读」原语）；判据数着本文件生产段里读环境的调用点，多一处 ⇒ 红。

use crate::common::cells::Words;
use crate::history::live_table::LiveTable;
use acct_core::{auth_kind_from_manifest, auth_kind_with_apikey_table, auth_ready};
pub use cmd_wire::accounts::{AccountRow, AccountsList, Badge, ListMeta, ScannedAccount};
use copy_core::copy_text;
use relay_route_core::{ACCOUNTS_DIR_REL, ACCOUNTS_MANIFEST_NAME};
use std::path::{Path, PathBuf};

use super::model::Manifest;

// ---------------------------------------------------------------- manifest

/// 清单里列出来的那几行：读口读出来的号表（`Manifest::book`，照清单顺序、含账号 0）。
fn listed(m: &Manifest) -> &[acct_core::ManifestAccount] {
    &m.book.accounts
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
/// （测试换掉家目录就碰不到真数据）。家说不出 ⇒ 照实拒（判法只在 `platform::paths::under_home`），不落到相对路径上。
fn resolve_accts_dir() -> Result<PathBuf, String> {
    crate::platform::paths::under_home(home_dir().as_deref(), ACCOUNTS_DIR_REL)
}

fn manifest_path(accts_dir: &Path) -> PathBuf {
    accts_dir.join(ACCOUNTS_MANIFEST_NAME)
}

/// 账号库清单在、却读不懂（读不出 · 不是 JSON · 版本不认）⇒ 为什么（同账号清单 `meta.error` 那一句）；读得懂 · 没有清单
/// （没启用多账号，正常状态）⇒ `None`。需手动清单据此回 `unreadable`，不把读不懂当成「没有需登录的号」。
pub fn library_unreadable(accts_dir: &Path) -> Option<String> {
    crate::accounts::manage::scan::manifest_at(&manifest_path(accts_dir))
        .err()
        .map(crate::accounts::manage::scan::Unread::into_said)
}

/// [`library_unreadable`]，这台的账号库。
/// 家说不出 ⇒ 那一句（照实说，不当成「没有需登录的号」）。
pub fn library_unreadable_here() -> Option<String> {
    resolve_accts_dir().map_or_else(Some, |d| library_unreadable(&d))
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
pub struct SessionAccount {
    pub(crate) pid: u32,
    pub sid: Option<String>,
    pub(crate) cwd: Option<String>,
    /// 进程环境里的配置目录（规范过的写法）；没设 · 不知道 ⇒ `None`。
    pub config_dir: Option<String>,
    pub account: Option<String>,
    /// 判活过了启动时刻对拍（活会话表 `start_checked`：PID 会被复用，错标签比缺标签危害大）。账号归属只认这一格。
    pub alive: bool,
    /// 脸上那一条活着的会话：活会话表那一判（`LiveRow.alive`，判活唯一的判法，会话脸上说活着也照它）且不是后台任务。
    /// 跟 `alive` 不是包含关系：启动时刻核不上（Windows 上记法恒不等）照样算活；守护池的备用进程 · 后台任务核上了也不算。
    /// 数「会打断几条会话」· 归号说「现在 / 上次」· 分叉判源会话活没活都认这一格（[`machine_product`]）。
    pub live: bool,
    /// 进程活着（身份已确认）、环境读得到、且没设配置目录 —— 就是账号 0（`account` 给出名字）。
    /// 环境这一刻取不到（读失败 / 读回 0 字节）⇒ 假：不归属、也不算裸起。
    pub bare: bool,
    /// 这条会话的 `ANTHROPIC_BASE_URL` 是不是**本机中转那一形**（回环 ＋ 钥匙段 ＋ 路由，
    /// `relay_route_core::split_keyed_base_url` 认得出）。`None` = 不知道（进程已死 · 启动时刻没核上、不读它的环境 · 那一家没有账号面 · 环境这一刻取不到 · 起会话时的上游描述也说不清）。
    /// 用途：机器页「停」本机后端之前数一数有几条会话会断。
    pub(crate) via_relay: Option<bool>,
}

// ---------------------------------------------------------------- 命令

/// 清单的**扫描本体**：`(meta, 账号们)`，帧面成品（[`list_product_with`]）经它读 manifest / 判安全 / 判鉴权方式；
/// 每个号是哪一家 · 那一家的凭据文件叫什么都问 `registry`（生产交 `REGISTRY`，判据喂合成注册表）。
///
/// `in_table(configDir, 那个号的家)`：这个号在**这台机器**的 apikey 表里有没有行（调用方按 `acct_core::apikey_routed_subset` 答，家照号自己的那一家：表的行只属于它挂的那一家）。它是鉴权方式的**第二个输入**，
/// 进的是同一条计算路径（`auth_kind_with_apikey_table` → `auth_ready`，各只一处）。
fn scan_accounts_among(
    registry: &[crate::agents::Adapter],
    accts_dir: &Path,
    in_table: &dyn Fn(&str, Option<&str>) -> bool,
) -> (ListMeta, Vec<ScannedAccount>) {
    // 这一趟只读一次清单：号色 · 删掉默认号之后默认谁都从这一份算。
    let read = manifest_or_why(accts_dir);
    let next = read.as_ref().ok().and_then(next_default);
    let meta = |enabled: bool,
                updated_at: Option<String>,
                count: usize,
                error: Option<String>,
                effective_default: Option<String>| ListMeta {
        enabled,
        manifest_path: manifest_path(accts_dir).to_string_lossy().into_owned(),
        updated_at,
        count,
        error,
        unsupported: crate::platform::acct_view::unsupported_said(),
        next_default: next.clone(),
        // 这台的默认号是谁（标了的第一个，没标 ⇒ 第一个）：规则只住 `acct_core::effective_default`，界面与起会话都读这一格。
        effective_default,
    };
    match read {
        Err(e) => (meta(false, None, 0, Some(e), None), Vec::new()),
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
                // 只 stat 存在性，绝不读内容。文件名只问这个号那一家声明的那一份（`agents::creds_file_among`）。
                // 探不到 config dir（账号 0 且 manifest 没写 sharedStore）· 认不出家 · 那一家没声明凭据文件 ⇒ false，
                // 那是「不知道」，不假装已登录。
                let creds_file = family_of(registry, a)
                    .and_then(|k| crate::agents::creds_file_among(registry, k));
                let credentials_present = probe_dir
                    .as_ref()
                    .zip(creds_file)
                    .is_some_and(|(d, f)| d.join(f).exists());
                // 鉴权方式这一维：分类与就绪各只有一处实现，都住 `acct-core`（本文件与 `local_accounts.rs` 都调它）。
                // 第二个输入：这台机器的 apikey 表里有没有它。
                let in_apikey_table = cfg_out
                    .as_deref()
                    .is_some_and(|d| in_table(d, family_of(registry, a)));
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
                    // 号在人眼里叫什么（账号 0 ⇒ 这个号那一家默认家目录的短形，其余照原名）：唯一一处 `name_words::account_text_among`（照账号库手里那张注册表问），界面 · 手机照抄。
                    // 认不出是哪一家的号照缺交，号名怎么说只在那一处判。
                    label: crate::accounts::quota::name_words::account_text_among(
                        registry,
                        family_of(registry, a),
                        &a.name,
                    ),
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
                    // 仅 stat 那一家的凭据文件存在性，不代表凭据有效；可用性走 `authReady`。
                    logged_in: credentials_present,
                    // 订阅 / api-key。manifest 缺这个键 ⇒ 订阅。
                    auth_kind: auth_kind.to_string(),
                    // 鉴权方式这一维不阻塞它被选中；不等于真能连上（界面要把这个状态说出来）。
                    auth_ready: ready,
                    // 号色（`--acct-c0..c7` 第几格）：账号库里的事实，界面只画它（[`colors_of`]；库里没给 ⇒ 缺，画中性）。
                    color: colors.get(&a.name).copied(),
                    // 这个号是哪一家的号（清单里没写的照缺省那一家给出；认不出 ⇒ 缺）：界面 · 起会话照它，不另判。
                    agent: family_of(registry, a).map(cmd_wire::AgentKind::new),
                    // 写了、认不出 ⇒ 原样留着（不当成没写、不归缺省那一家）：需手动清单照它说认不出是哪一家的号。
                    unknown_agent: a.agent.clone().filter(|_| family_of(registry, a).is_none()),
                });
            }
            let effective_default =
                acct_core::effective_default(&lines, |a| a.is_default).map(|a| a.name.clone());
            (
                meta(
                    true,
                    m.updated_at.clone(),
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
pub use cmd_wire::accounts::AccountsListReq;

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

/// ★ 进不进新建会话的选号表（唯一一处，清单成品那一格再并上 [`row_of_family`]）：隔离、有配置目录。账号 0 不经选号；原地模式的号不拿来起会话。
pub(crate) fn new_session_of(a: &ScannedAccount) -> bool {
    a.config_dir.is_some() && a.mode == "isolated"
}

/// ★ 这个号是不是 `agent` 那一家的（唯一一处）：说不清是哪一家的不算。选号（`launch_account::pick`）与新建会话的选号表都照它。
pub fn row_of_family(a: &ScannedAccount, agent: &cmd_wire::AgentKind) -> bool {
    a.agent.as_ref() == Some(agent)
}

/// 选不了只差登录（四条里只有「需登录」那一条没过）。
fn needs_login(a: &ScannedAccount) -> bool {
    !a.selectable
        && a.config_dir.is_some()
        && a.mode == "isolated"
        && a.exists
        && a.auth_kind != acct_core::AUTH_KIND_API_KEY
}

/// ★ 每个号给不给［登录…］·［退出登录］·［登录状态］（唯一一处）：每一行照它自己那一家（`scanned.agent`）的登录形
/// （`sign_in_of`：那一家 ⇒ 它的 `SignInFace`，不在 app 里登 · 认不出那一家 ⇒ `None`）判，不拿请求那一家套整张表：
/// - ［登录…］：只差登录、而那一家能在 app 里登录 ⇒ 给；只差登录却登不了 ⇒ 那一句换成「需登录 · 不支持在 app 里登录」。
/// - ［退出登录］：那一家声明了退出那一条、这个号是登着的订阅号 ⇒ 给。
/// - ［登录状态］：那一家声明了状态那一条、这个号是订阅号 ⇒ 给。
pub fn with_sign_in(
    list: &mut AccountsList,
    sign_in_of: impl Fn(Option<&str>) -> Option<crate::agents::SignInFace>,
) {
    for r in &mut list.accounts {
        let face = sign_in_of(r.scanned.agent.as_deref());
        let need = needs_login(&r.scanned);
        let sub = r.scanned.auth_kind != acct_core::AUTH_KIND_API_KEY && r.scanned.exists;
        r.login = need && face.is_some();
        r.sign_out = sub && r.scanned.logged_in && face.is_some_and(|f| f.sign_out.is_some());
        r.status = sub && face.is_some_and(|f| f.status.is_some());
        if need && face.is_none() {
            r.unselectable = Some(Words(copy_text("accounts.row.loginElsewhere", &[])));
        }
    }
}

/// 删掉默认号之后新会话默认谁（默认号只有清单里那一份）：问写侧那一条规则（`Manifest::without`），这里不另判。
/// 没有默认号 / 删了之后没有别的号 ⇒ `None`。
fn next_default(m: &Manifest) -> Option<String> {
    let current = m.managed().find(|a| a.is_default)?.name.clone();
    m.without(&current).1
}

/// 这台账号库给每个号的号色（名 ⇒ 第几格；读不出 ⇒ 空表）：别处画号色（轮换那一页）与 `accounts-list` 同一份；
/// 不在表里的号不给（同清单那一行：画中性）。
pub fn account_colors() -> std::collections::BTreeMap<String, u8> {
    resolve_accts_dir()
        .and_then(|d| manifest_or_why(&d))
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

/// 这台 API key 表里一个号那一行给清单的两格：（掩码，端点地址）。调用方照账号 id 从那张表里找来（账号库不认识那张表）；
/// 表里没有它那一行 ⇒ `None`。
pub type KeyOf<'a> = &'a dyn Fn(&str) -> Option<(Option<String>, Option<String>)>;

/// ★ 徽章一处判（每一态只出自这里）：原地模式 ⇒ 不隔离；API key 号 ⇒ 这台 key 表里有没有它那一行 · 这台的中转在不在
/// （两格都成立才「经中转」）；订阅号 ⇒ 登录了没有。`routed` ＝ 表里有它那一行（账号 0 没有目录名 ⇒ `None`：说不出，不替它下判断）。
pub(crate) fn badge_of(
    in_place: bool,
    api_key: bool,
    auth_ready: bool,
    routed: Option<bool>,
    relay_running: bool,
) -> Badge {
    let b = |text: String, warn: bool| Badge {
        text: Words(text),
        warn,
    };
    if in_place {
        return b(copy_text("accounts.badge.inPlace", &[]), true);
    }
    if api_key {
        return match routed {
            Some(true) if relay_running => b(copy_text("accounts.badge.apikeyRelayed", &[]), false),
            Some(true) => b(copy_text("accounts.badge.apikeyRelayDown", &[]), true),
            Some(false) | None => b(copy_text("accounts.badge.apikeyNoEndpoint", &[]), true),
        };
    }
    if !auth_ready {
        return b(copy_text("acct.tag.login", &[]), true);
    }
    b(copy_text("accounts.badge.signedIn", &[]), false)
}

/// 账号库落点推不出（家目录说不出）时的成品：没启用、清单路径空着、`meta.error` 就是拒的那一句，一行都没有、一处不读。
pub fn refused_list(why: String) -> AccountsList {
    AccountsList {
        meta: ListMeta {
            enabled: false,
            manifest_path: String::new(),
            updated_at: None,
            count: 0,
            error: Some(why),
            unsupported: crate::platform::acct_view::unsupported_said(),
            next_default: None,
            effective_default: None,
        },
        accounts: Vec::new(),
        notice: None,
    }
}

/// 帧面 `accounts-list` 的成品（账号库目录是参数：判据拿夹具喂它，不碰真家目录；apikey 表那两样由调用方交进来，agent 随请求带）。
///
/// `{meta, accounts, notice}`：清单经 [`scan_accounts_among`] 扫，并上这台机器自己那份 apikey 表
/// （`rows`：表里有哪几条账号 id；`key_of`：一个号那一行的掩码与端点 —— 调用方从中转那一侧的上游选择读来，同一个出处）；
/// 「哪几个号在表里有行」只问 `acct_core::apikey_routed_subset`（`table_agent`：这台机器上那份文件属于哪一家）。
/// `notice`：「能用但有缺」—— manifest 启用了、却一个账号 0 都没有（写它的那一侧旧到不认账号 0）。措辞不说「远端」：本机远端同一条路。
/// `relay_running` ＝ 这台的中转在不在（徽章要它；调用方读本进程的监听状态 ⇒
/// 只有常驻进程答得出真话，`accounts-list` 在 `STREAM_ONLY`）。
///
/// - 每个号 `configDirShown`：号目录的显示形，按 `home`（这台的家目录）写（[`crate::platform::paths::home_short`]），界面照抄。
/// - 每个号 `keyMasked` · `endpointHost`：API 号在这台 apikey 表里那一行的掩码（只留末四位）与端点的主机（`name_words::endpoint_host` 写）；
///   订阅号 / 表里没有它那一行 / 没配 key ⇒ `null`。key 本体从不出后端。
pub fn list_product_with(
    accts_dir: &Path,
    home: Option<&Path>,
    rows: &[String],
    key_of: KeyOf,
    agent: &str,
    table_agent: &str,
    relay_running: bool,
) -> AccountsList {
    list_product_among(
        crate::agents::REGISTRY,
        accts_dir,
        home,
        rows,
        key_of,
        Some(agent),
        table_agent,
        relay_running,
    )
}

/// [`list_product_with`]，注册表是参数（判据拿替身家喂它）。
/// 每个号 `mcpLogins`：它那一家的 MCP 登录不在号之间同步（判定只一处 `mcp_logins_exec::logins_shared`）⇒ 那一句；
/// 同步 · 认不出那一家 ⇒ 缺。
/// 账号库几家混列：「key 表里有没有它那一行」照每个号自己的那一家判（表的行只属于 `table_agent` 那一家），不照起的那一家。
/// `launching` ＝ 起的那一家（新建会话的选号表只列它的号）；`None` ＝ 不按起的那一家过滤（登录那一面），`newSession` 照每个号自己的家给。
#[expect(
    clippy::too_many_arguments,
    reason = "同 list_product_with 那几样，多一格注册表；收成结构只多一层拆装"
)]
pub fn list_product_among(
    registry: &[crate::agents::Adapter],
    accts_dir: &Path,
    home: Option<&Path>,
    rows: &[String],
    key_of: KeyOf,
    launching: Option<&str>,
    table_agent: &str,
    relay_running: bool,
) -> AccountsList {
    // key 表挂的那一家给的是路由名，号带的是 kind：换成 kind 再比。
    let table_kind = crate::agents::kind_of_route_among(registry, table_agent);
    let routed = |dir: &str, family: Option<&str>| {
        family.is_some_and(|k| {
            !acct_core::apikey_routed_subset(&[dir.to_string()], rows, k, table_kind).is_empty()
        })
    };
    let (meta, scanned) = scan_accounts_among(registry, accts_dir, &routed);
    // 起的那一家（请求给的是路由名或 kind，都换成 kind）。
    let launching = launching
        .map(|agent| cmd_wire::AgentKind::new(crate::agents::kind_of_route_among(registry, agent)));
    let accounts: Vec<AccountRow> = scanned
        .into_iter()
        .map(|a| {
            let api_key = a.auth_kind == acct_core::AUTH_KIND_API_KEY;
            let (key_masked, base_url) = api_key
                .then(|| {
                    a.config_dir
                        .as_deref()
                        .and_then(acct_core::apikey_account_id_of_dir)
                })
                .flatten()
                .and_then(|id| key_of(&id))
                .unwrap_or_default();
            let badge = badge_of(
                a.mode == "in-place",
                api_key,
                a.auth_ready,
                a.config_dir
                    .as_deref()
                    .map(|d| routed(d, a.agent.as_ref().map(cmd_wire::AgentKind::as_str))),
                relay_running,
            );
            let endpoint_host = base_url
                .as_deref()
                .map(crate::accounts::quota::name_words::endpoint_host);
            AccountRow {
                kind_line: crate::accounts::quota::name_words::kind_line(
                    if api_key {
                        cmd_wire::quota::Kind::Api
                    } else {
                        cmd_wire::quota::Kind::Sub
                    },
                    a.logged_in,
                    &a.email,
                    endpoint_host.as_ref(),
                ),
                config_dir_shown: a
                    .config_dir
                    .as_deref()
                    .map(|d| crate::platform::paths::home_short(d, home)),
                key_masked,
                endpoint_host,
                badge,
                plate: Words(crate::accounts::quota::name_words::plate_text(&a.name)),
                slots: Vec::new(),
                quota: Vec::new(),
                unselectable: unselectable_of(&a, api_key).map(Words),
                mcp_logins: a
                    .agent
                    .as_ref()
                    .filter(|k| {
                        super::mcp_logins_exec::logins_shared(registry, k.as_str()).is_none()
                    })
                    .map(|_| Words(copy_text("accounts.fact.mcpLoginsNotShared", &[]))),
                login: false,
                sign_out: false,
                status: false,
                // 别家的号起不了这一家（选号那一判），不列、也就不给它的［登录…］；不按起的那一家过滤 ⇒ 照号自己的家（认得出是哪一家的就列）。
                new_session: new_session_of(&a)
                    && launching
                        .as_ref()
                        .map_or(a.agent.is_some(), |k| row_of_family(&a, k)),
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

/// 这条会话跑在哪个号的家目录下：活会话表里持着它、启动时刻核过的那个进程 ⇒ 读进程环境里的配置目录（没设 ⇒ 那一家的默认家目录，`agents::home_of`）。
/// 找不到 · 进程不在了 · 环境读不到 ⇒ `None`（不猜）。
pub fn session_home(table: &LiveTable, sid: &str) -> Option<PathBuf> {
    let pid = table.checked_pid(sid)?;
    // 号载体照这条会话那一家的账号面读（活会话表那一行记着是哪一家写的）。
    let kind = table.rows().iter().find(|r| r.pid == pid)?.fact.kind;
    let keys = env_keys_of(crate::agents::REGISTRY, kind)?;
    let owner = owner_of(pid, keys.config_dir, &[]);
    if owner.env_unreadable {
        return None;
    }
    crate::agents::home_of(kind, owner.config_dir.as_deref().map(Path::new), false)
}

/// 家目录 `home` 下的 `<rel>` 跟这台账号库里别的号共用同一份（[`shared_with_other_accounts_in`]）。
pub fn shared_with_other_accounts(home: &Path, rel: &str) -> bool {
    resolve_accts_dir().is_ok_and(|d| shared_with_other_accounts_in(&d, home, rel))
}

/// 看的是链接布局：`home` 下那一份解开链接后的真身，跟账号库里另一个号（家目录不是同一个）的那一份是同一份 ⇒ 共用。
/// 它自己一份 · 没有账号库 · 这份文件还没有（解不开）⇒ `false`。
pub fn shared_with_other_accounts_in(accts_dir: &Path, home: &Path, rel: &str) -> bool {
    use crate::common::fence::real_path;
    let (Some(me), Some(my_home)) = (real_path(&home.join(rel)), real_path(home)) else {
        return false;
    };
    // 有本机布局的每一家：它那几个号的家各看一遍。
    crate::agents::REGISTRY
        .iter()
        .filter(|a| a.local.is_some())
        .any(|a| {
            dirs_of_kind(crate::agents::REGISTRY, accts_dir, a.kind)
                .iter()
                .filter_map(|(d, _)| {
                    crate::agents::home_of(a.kind, d.as_deref().map(Path::new), false)
                })
                .any(|other| {
                    real_path(&other).is_some_and(|h| h != my_home)
                        && real_path(&other.join(rel)).is_some_and(|f| f == me)
                })
        })
}

/// 号表里一行是哪一家的号：清单那一格（缺席归缺省那一家，`agents::account_kind_among`）；认不出 ⇒ `None`。
fn family_of(
    registry: &[crate::agents::Adapter],
    a: &acct_core::ManifestAccount,
) -> Option<&'static str> {
    crate::agents::account_kind_among(registry, a.agent.as_deref())
}

/// [`dirs_of`] 里 `kind` 那一家的号（[`family_of`]）· 账号 0（没设号载体 ＝ 每一家各自的默认家，哪一家都算）。
/// 会话归号只在它那一家的号里对。
fn dirs_of_kind(
    registry: &[crate::agents::Adapter],
    accts_dir: &Path,
    kind: &str,
) -> Vec<(Option<String>, String)> {
    let Ok(m) = manifest_or_why(accts_dir) else {
        return Vec::new();
    };
    listed(&m)
        .iter()
        .filter(|a| a.config_dir.is_none() || family_of(registry, a) == Some(kind))
        .map(|a| {
            let dir = a.config_dir.as_deref().map(|c| norm_dir(c).to_string());
            (dir, a.name.clone())
        })
        .collect()
}

/// 认家那一路读的号表：每份清单（路径）⇒ 读那一下的戳（`Ro::stamp`）· 读成的那一份（读不懂 ⇒ `None`）。
type HomesBook = std::collections::HashMap<PathBuf, (crate::platform::fs::Stamp, Option<Manifest>)>;

/// 认家每问一次都要各号的家：清单戳没变就用上次读的那一份，变了才重读；清单不在 ⇒ 没有号、不记。
static HOMES_BOOK: std::sync::LazyLock<std::sync::Mutex<HomesBook>> =
    std::sync::LazyLock::new(Default::default);

/// `kind` 那一家各具名号的家（号名 · 号目录，照清单顺序；号归哪一家照 [`family_of`]）。账号 0 不在里面（它的家是那一家的默认家，
/// 根表那一侧自己补）。号表取 [`HOMES_BOOK`] 那一份缓存：清单文件没变不重读。
pub fn account_homes_among(
    registry: &[crate::agents::Adapter],
    accts_dir: &Path,
    kind: &str,
) -> Vec<crate::agents::AccountHome> {
    let p = manifest_path(accts_dir);
    let mut g = HOMES_BOOK.lock().unwrap_or_else(|e| e.into_inner());
    let Some(stamp) = crate::platform::fs::Ro::stamp(&p) else {
        g.remove(&p);
        return Vec::new();
    };
    if g.get(&p).is_none_or(|(s, _)| *s != stamp) {
        g.insert(p.clone(), (stamp, manifest_or_why(accts_dir).ok()));
    }
    let Some((_, Some(m))) = g.get(&p) else {
        return Vec::new();
    };
    listed(m)
        .iter()
        .filter(|a| family_of(registry, a) == Some(kind))
        .filter_map(|a| {
            Some(crate::agents::AccountHome {
                account: Some(a.name.clone()),
                home: PathBuf::from(norm_dir(a.config_dir.as_deref()?)),
            })
        })
        .collect()
}

/// [`account_homes_among`]，这台的账号库、生产注册表：进程入口接进根表那一手（`agents::on_account_homes`）。
pub fn account_homes_here(kind: &str) -> Vec<crate::agents::AccountHome> {
    resolve_accts_dir()
        .map(|d| account_homes_among(crate::agents::REGISTRY, &d, kind))
        .unwrap_or_default()
}

/// 读记录那一路交上来的一行自报额度（`observe::watcher::Reactions::quota_said`）⇒ 认号进账：`kind` 那一家的号（连账号 0：那一家的默认家）
/// 照它们的号目录经 [`crate::agents::owner_of_record`] 认出这份记录是哪个号的，记进额度账（来源 `usage`，同 `quota-probe` 那一口）。
/// 认不出号 ⇒ 不进账、记一行；写不成 ⇒ 记一行。
pub fn quota_said_here(
    own: &crate::platform::fs::Own,
    kind: &'static str,
    record: &Path,
    windows: Vec<crate::agents::QuotaWindow>,
) {
    let accts = match resolve_accts_dir() {
        Ok(d) => d,
        Err(why) => {
            tracing::info!("自报额度认不出号（{why}），不进账");
            return;
        }
    };
    quota_said_among(
        crate::agents::REGISTRY,
        &accts,
        own,
        kind,
        record,
        windows,
        crate::common::time::now_secs(),
    );
}

/// [`quota_said_here`] 的可喂夹具那一半：注册表 · 账号库目录 · 此刻都由调用方给。额度账记在那一家的路由名下（[`crate::agents::pick_route_among`]）。
pub(crate) fn quota_said_among(
    registry: &[crate::agents::Adapter],
    accts_dir: &Path,
    own: &crate::platform::fs::Own,
    kind: &'static str,
    record: &Path,
    windows: Vec<crate::agents::QuotaWindow>,
    now: u64,
) {
    let Ok(route) = crate::agents::pick_route_among(registry, Some(kind)) else {
        return;
    };
    let base = crate::agents::adapter_among(registry, kind).and_then(|a| (a.home)());
    let accounts: Vec<(String, PathBuf)> = manifest_or_why(accts_dir)
        .map(|m| {
            listed(&m)
                .iter()
                .filter(|a| a.config_dir.is_none() || family_of(registry, a) == Some(kind))
                .filter_map(|a| {
                    let dir = match a.config_dir.as_deref() {
                        Some(d) => PathBuf::from(norm_dir(d)),
                        None => base.clone()?,
                    };
                    Some((a.route_id()?, dir))
                })
                .collect()
        })
        .unwrap_or_default();
    let Some(account) = crate::agents::owner_of_record(registry, kind, record, &accounts) else {
        tracing::info!("自报额度认不出号（{}），不进账", record.display());
        return;
    };
    let ledger = crate::accounts::quota::ledger::Ledger::at(
        own.root()
            .map(|d| crate::accounts::quota::ledger::path_under(d.as_path())),
    );
    if let Err(e) = crate::accounts::quota::ledger::record_probe(
        &ledger,
        own,
        route.id(),
        &account,
        windows,
        now,
    ) {
        tracing::warn!("自报额度记不进额度账（号 {account}）：{}", e.logged());
    }
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

/// 会话进程环境里该读哪两个键（账号 · 上游地址）：只问写这条会话的那一家的账号面（`AccountsFace.session_env`），
/// 本文件向适配层要键名只经这一处；那一家没有账号面 ⇒ `None`。键名是那一家的常量，不是调用方给的参数。
fn env_keys_of(
    registry: &[crate::agents::Adapter],
    kind: &str,
) -> Option<crate::agents::SessionEnvKeys> {
    crate::agents::accounts_face_among(registry, kind).map(|f| f.session_env)
}

/// 一条刚宣告的会话（`kind` 那一家写的）跑在哪个号下：照那一家的号载体读进程环境、只对那一家的号（[`dirs_of_kind`]）；
/// 那一家没有账号面 · 判不了 ⇒ `None`。会话宣告带的就是它。
pub fn account_of_process(pid: u32, kind: &str, accts_dir: &Path) -> Option<String> {
    account_of_process_among(crate::agents::REGISTRY, pid, kind, accts_dir)
}

/// [`account_of_process`] 的可喂夹具那一半。
pub fn account_of_process_among(
    registry: &[crate::agents::Adapter],
    pid: u32,
    kind: &str,
    accts_dir: &Path,
) -> Option<String> {
    let keys = env_keys_of(registry, kind)?;
    let by_dir = dirs_of_kind(registry, accts_dir, kind);
    owner_of(pid, keys.config_dir, &by_dir).account
}

/// [`account_of_process`]，账号库取这台的那一份：进程入口把它接进观测侧宣告会话那一格（`observe::watcher::Reactions::account_of`）。
pub fn account_of_process_here(pid: u32, kind: &'static str) -> Option<String> {
    account_of_process(pid, kind, &resolve_accts_dir().ok()?)
}

/// 正在跑的会话各属哪个号：活会话表（`history::live_table`）每个进程一行；每一行照写它的那一家的账号面读进程环境
/// （账号 · 上游地址两个键，`AccountsFace.session_env`），只对那一家的号。那一家没有账号面 ⇒ 那一行不归号、不判走不走中转。
/// 账号库 `accts_dir` 说不出（`None`，家目录说不出）⇒ 会话照列，号一个都不认，一处不读。
pub fn session_accounts(
    agent_home: impl crate::agents::IntoHomes,
    accts_dir: Option<&Path>,
) -> Vec<SessionAccount> {
    let agent_home = agent_home.into_homes();
    let table = crate::history::live_table::now(&agent_home);
    let mut by_kind: std::collections::BTreeMap<&str, Vec<(Option<String>, String)>> =
        std::collections::BTreeMap::new();
    table
        .rows()
        .iter()
        .map(|r| {
            let pid = r.pid;
            let kind = r.fact.kind;
            let env_keys = env_keys_of(crate::agents::REGISTRY, kind);
            let cwd = r.fact.cwd.as_deref();
            // 判活必须过启动时刻对拍（表里那一格）：PID 会被复用，只按存在性判活会把已死会话误贴成「活着 + 别人的账号」。
            // 身份不符 → 当作该会话已死，不读 environ、不归属。
            let alive = r.start_checked;
            // 进程已死 · 那一家没有账号面时一个字节都不读 ⇒ 谈不上「取不到」，那一格照旧是「没读」。
            let read = env_keys.filter(|_| alive);
            let owner = match read {
                Some(keys) => {
                    // 那一家的号只在头一回用到时读一次清单（同一趟里各行共用）。
                    let by_dir = by_kind.entry(kind).or_insert_with(|| {
                        accts_dir
                            .map(|d| dirs_of_kind(crate::agents::REGISTRY, d, kind))
                            .unwrap_or_default()
                    });
                    owner_of(pid, keys.config_dir, by_dir)
                }
                None => Owner::default(),
            };
            // 第二个键（那一家从环境变量读上游地址才有）：键名是适配层的常量，不是参数。
            let via_relay = read.and_then(|env_keys| {
                via_relay_of(
                    env_keys
                        .base_url
                        .map(|base_url| proc_env_var(pid, base_url)),
                    &|| launched_relay_here(pid),
                    &|| {
                        // 那一家的家（写这一行的那一家在 `agent_home` 里的那一份）。
                        let Some(home) = agent_home.iter().find(|h| h.kind == kind) else {
                            return false;
                        };
                        (env_keys.settings_may_set_base_url)(
                            owner.config_dir.as_deref().map(Path::new),
                            &home.home,
                            cwd.map(Path::new),
                        )
                    },
                )
            });
            SessionAccount {
                pid,
                sid: r.fact.sid.clone().filter(|s| !s.is_empty()),
                cwd: r.fact.cwd.clone().filter(|s| !s.is_empty()),
                bare: read.is_some() && !owner.env_unreadable && owner.config_dir.is_none(),
                config_dir: owner.config_dir,
                account: owner.account,
                alive,
                live: r.alive && !r.fact.background,
                via_relay,
            }
        })
        .collect()
}

/// 「进程 `pid` 起来时注入了中转没有」那一手：判法在上游选择那一侧（起会话时留的上游描述），账号库不引它，由进程入口接上。
static LAUNCHED_RELAY: std::sync::OnceLock<fn(u32) -> Option<bool>> = std::sync::OnceLock::new();

/// 进程入口接上那一手；接过一次 ⇒ `Err`（第一次那一手留着）。
pub fn on_launched_relay(f: fn(u32) -> Option<bool>) -> Result<(), fn(u32) -> Option<bool>> {
    LAUNCHED_RELAY.set(f)
}

/// 没接上 ⇒ 说不清。
fn launched_relay_here(pid: u32) -> Option<bool> {
    LAUNCHED_RELAY.get().and_then(|f| f(pid))
}

/// 一条活会话经不经本机中转（只折成一个布尔；值带钥匙，这一下之后就丢掉）：
/// - 那一家从环境变量读上游地址（`SessionEnvKeys.base_url` 有）⇒ `base` 是读到的那一格：是中转地址、又没被那一家自己的设置文件压过 ⇒ 是；
///   设置文件可能压过（`settings_win`）⇒ 说不清；不是中转地址 · 没设 ⇒ 不是；读不出 ⇒ 说不清。
/// - 不从环境变量读（经参数注入的那一家，`base` 是 `None`）⇒ 照起会话时留的那一份（`launched`：上游描述记着这一趟注入了中转）。
fn via_relay_of(
    base: Option<EnvRead>,
    launched: &dyn Fn() -> Option<bool>,
    settings_win: &dyn Fn() -> bool,
) -> Option<bool> {
    let Some(read) = base else {
        return launched();
    };
    match read {
        EnvRead::Value(v) if relay_route_core::split_keyed_base_url(&v).is_some() => {
            (!settings_win()).then_some(true)
        }
        EnvRead::Value(_) | EnvRead::Unset => Some(false),
        EnvRead::Unreadable => None,
    }
}

/// `machine-interrupts` 的成品：停 / 重启 / 更新 / 卸载这台的 cc-monitor 之前会打断什么。
/// `rows` 是这台活会话那几行（[`session_accounts`]），数的是脸上活着的会话（`live`；同一条会话几个进程只算一条）：
/// 有一个进程经本机中转 ⇒ `relayedSessions`；不然有一个说不清走不走（环境读不出 · 身份没核上没读 · 设置可能压过）⇒ `relayedMaybe`；
/// 都算 `liveStreams`（停的那几秒 cc-monitor 里这几条不更新）。`forwards` 是这台后端账上通往 `machine` 的转发（没给 `machine` ⇒ 0）。
/// **纯函数**。
pub fn machine_product(rows: &[SessionAccount], forwards: u32) -> Interrupts {
    // 一条会话 ⇒ 它那几个进程走不走中转并成一格（是 ＞ 说不清 ＞ 不是）；说不出会话 id 的进程各算一条
    // （界面会话列表不列它，可它的请求照样会断；pidfile 几乎都带会话 id）。
    let mut sessions: Vec<(Option<&str>, Option<bool>)> = Vec::new();
    for r in rows.iter().filter(|r| r.live) {
        let sid = r.sid.as_deref();
        match sessions
            .iter_mut()
            .find(|(s, _)| sid.is_some() && *s == sid)
        {
            Some((_, via)) => {
                *via = match (*via, r.via_relay) {
                    (Some(true), _) | (_, Some(true)) => Some(true),
                    (None, _) | (_, None) => None,
                    _ => Some(false),
                }
            }
            None => sessions.push((sid, r.via_relay)),
        }
    }
    let (mut relayed, mut maybe) = (0u32, 0u32);
    for (_, via) in &sessions {
        match via {
            Some(true) => relayed += 1,
            Some(false) => {}
            None => maybe += 1,
        }
    }
    let live = u32::try_from(sessions.len()).unwrap_or(u32::MAX);
    Interrupts {
        relayed_sessions: relayed,
        relayed_maybe: maybe,
        live_streams: live,
        forwards,
    }
}

/// 这台的那几格（[`machine_product`]）：停 / 重启 / 更新 / 卸载这台的 cc-monitor 之前会打断什么。不上线：`machine-interrupts` 的回包照它填
/// （`faces/read_face.rs::reply_of`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Interrupts {
    /// 这台的活会话里经本机中转走请求的几个（停了就断，直到再启动）。
    pub relayed_sessions: u32,
    /// 活着、说不清走不走中转的几个（环境这一刻读不出 / agent 自己的设置可能压过它）。
    pub relayed_maybe: u32,
    /// 这台活着的会话数（停的那几秒 cc-monitor 里它们不更新）。
    pub live_streams: u32,
    /// 这台账上通往问的那台机器的转发（问的是重启 / 退出 cc-monitor ⇒ 账上全部）。
    pub forwards: u32,
}

/// 「现在重启 cc-monitor」之前那一问（`machine-interrupts` 带 `appExit`）：这台后端随 cc-monitor 退出一起停（`kill` ＝ 它自己那份退出行为）
/// ⇒ 这台的活会话照 [`machine_product`] 数、账上全部转发（`all_forwards`）都断；留着 ⇒ 什么都不断。**纯函数**。
pub fn app_exit_product(kill: bool, rows: &[SessionAccount], all_forwards: u32) -> Interrupts {
    if kill {
        machine_product(rows, all_forwards)
    } else {
        machine_product(&[], 0)
    }
}

/// 此刻有没有活会话跑在号目录 `dir` 下（[`dir_in_use`]，这台的活会话表）：只在没有会话用这个号时才续的那一家（`Renew::Idle`）续之前问它。
pub fn dir_in_use_here(dir: &Path) -> bool {
    dir_in_use(&session_accounts_here(&crate::agents::local_homes()), dir)
}

/// `rows`（[`session_accounts`]）里有活着的一条，进程环境里的号目录就是 `dir`。号目录照清单那一形规范过再比。
pub fn dir_in_use(rows: &[SessionAccount], dir: &Path) -> bool {
    let want = norm_dir(&dir.to_string_lossy()).to_string();
    rows.iter()
        .any(|r| r.alive && r.config_dir.as_deref() == Some(want.as_str()))
}

/// 帧面那几问要的「哪个会话在用哪个号」（[`session_accounts`]，账号库取这台的那一份）。
pub fn session_accounts_here(agent_home: impl crate::agents::IntoHomes) -> Vec<SessionAccount> {
    session_accounts(agent_home, resolve_accts_dir().ok().as_deref())
}

// ---------------------------------------------------------------- tests

// 照类型造清单成品 · 盘上一份账号库的夹具住测试树：起会话挑号 · 需手动清单几份判据共用（本包与上层判据经 `test-support`）。
#[cfg(any(test, feature = "test-support"))]
#[path = "../../../../tests/backend/accounts/manage/library_support.rs"]
#[expect(
    clippy::disallowed_methods,
    reason = "测试档夹具在沙箱目录里摆一份账号库（同各包测试段放开碰盘）"
)]
pub mod test_support;

#[cfg(test)]
#[path = "../../../../tests/backend/accounts/manage/library_tests.rs"]
mod tests;
