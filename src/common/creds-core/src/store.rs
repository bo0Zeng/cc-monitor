//! 那份凭据文件长什么样，以及「读一份 / 改一个键 / 再写回去」这三步的纯逻辑。
//!
//! 本模块一个文件系统调用都没有，刻意的：
//! 1. backend 的写面逐模块登记（`readonly_guard`，账号域那一份凭据写口在第四层），那张表扫不到本 crate ⇒ 写调用放进来是给护栏挖洞；
//! 2. `src/frontend/shell/src/write_site_registry.rs` 与 `atomic_replace_registry.rs` 的扫描根都是 `src/frontend/shell/src`，`src/common/` 不在它们的人群里；
//! 3. 纯函数好测：要测的是交错（人改了 A，程序写 B，A 还在不在），而「写的那一刻才去读」由调用方兑现，
//!    本模块只保证「给我旧内容 + 新 key，我还你一份没吃掉任何东西的新内容」。
//!
//! # 格式：人能读能改，改完就生效
//!
//! 一份明文 JSON 对象。认的键之外其余键一律原样留着。文件不存在时给 [`template`] —— 一个「能手编但没人知道格式」的文件等于不能手编。

use crate::SecretKey;
use serde_json::{Map, Value};

/// key 住在哪个字段。这个名字是契约：人手编的时候写的就是它。顶层与 [`ACCOUNTS_FIELD`] 里每一条账号内部刻意同名（人手编时不必记两套词）。
pub const KEY_FIELD: &str = "api_key";

/// 多账号那张表住哪个字段。**这个名字是契约**。
///
/// 形状：`{"accounts": {"<账号 id>": {"api_key": "…", "base_url": "…"}}}`。
/// `<账号 id>` 会**原样**变成路由键里那一段（`/s/<agent>/<账号 id>/<key>/…`）
/// ⇒ 它必须是路由段放得下的字符；放不下的那一条**永远匹配不上**，
/// 由上游选择装表时出声（`accounts::upstream_select::table::build` 那条「这一行进不了表」）。
pub const ACCOUNTS_FIELD: &str = "accounts";

/// 一条账号的上游端点住哪个字段。缺席 / 空串 ⇒ 用这个 agent 的默认上游（适配层 `agents::Adapter::upstream`）。
/// 它不是回落：「这一行用默认端点」与「这一行根本不在表里」（404）是两件事。
pub const BASE_URL_FIELD: &str = "base_url";

/// 一条账号怎么把 key 交给上游住哪个字段。缺席 / 空串 ⇒ [`AuthStyle::DEFAULT`]。
/// 它说的只有「鉴权头怎么写」，不是「上游说哪种方言」：中转对请求体一个字节都不解析（`relay/` 生产段零 `serde_json`），没有资格声称知道上游要哪种 body。
pub const AUTH_STYLE_FIELD: &str = "auth_style";

/// 顶层那把 key 在表里叫什么名字。它是一个有名字的行，不是「默认行」：只有路由键里账号段逐字是 `default` 的请求才用它；
/// 别的账号段查不到 ⇒ 404（「查不到就拿它顶上」= 拿 A 的 key 发 B 的请求）。
///
/// - 读：[`read_accounts`] 把顶层那一把折成一条 id 为本常量的行 —— 老用户手上那份文件、以及手编那条路照常能用（不许顺手删，
///   由 `the_legacy_top_level_key_becomes_one_named_row_not_a_default_row` 与 `an_unconfigured_file_yields_no_rows_at_all` 钉着）。
/// - 写：界面那条路（那台后端的写口 `accounts/upstream_select/file_face.rs`）落的是 `accounts.<id>`，一个字节都不往顶层那一格写
///   （后端 `file_face_tests::gp1_the_write_side_never_targets_the_legacy_top_level_slot`）：写顶层那一格，读回来 id 逐字是本常量，
///   而起会话那一侧按账号目录末段名索引 ⇒ 从界面配的 key 永远匹配不上任何账号。
/// [`merge_key`] 仍是 [`merge_account_key`] 内部改「某一条里那个 `api_key`」的实现。
pub const LEGACY_ACCOUNT_ID: &str = "default";

/// 那份文件在 monitor 数据目录**根上**的名字（[`credentials_path`]）。**两侧共用的唯一契约。**
///
/// # 为什么住这里，而不是两边各写一份字面量
///
/// monitor（`config::resolve_monitor_data_dir`）与后端（`upstream_select::creds::resolve_path`）各自解析，
/// 但**落点必须是同一个** —— 两边各写一份字符串，漂开的那天没有任何东西会说，而症状是
/// 「界面上配好了，上游选择说没配」这种**查不出来**的形状。
///
/// ⚠ 它**不**跟随 `claudeDir` 覆盖（数据目录本来就不跟随，`INVARIANTS §2`）。
pub const FILE_NAME: &str = "apikey-credentials.json";

/// monitor 数据目录的覆盖变量（monitor `paths::DATA_DIR_ENV` 与远端常驻后端的默认推导共用这一个名字）。
pub const DATA_DIR_ENV: &str = "CCM_DATA_DIR";

/// 历史注解文件在 monitor 数据目录下的名字（monitor `history::metadata_path` 与常驻后端默认推导共用）。
pub const HISTORY_METADATA_FILE: &str = "history-metadata.json";

/// monitor 数据目录：`CCM_DATA_DIR`（非空且绝对）优先，否则 `<home>/.cc-monitor`（一台机器一个家，与后端同一个）；
/// 设了却不是绝对路径 ⇒ `None`（不退回真 profile）。两侧共用这一份，谁起常驻后端推出来的路径都一样。
/// **这个目录的默认住址只在这里拼**（判据 `paths_tests::the_data_dir_is_spelled_in_one_place`）。
pub fn monitor_data_dir(
    env_val: Option<&str>,
    home: Option<std::path::PathBuf>,
) -> Option<std::path::PathBuf> {
    match env_val.map(str::trim).filter(|t| !t.is_empty()) {
        Some(t) => {
            let p = std::path::PathBuf::from(t);
            p.is_absolute().then_some(p)
        }
        None => Some(home?.join(".cc-monitor")),
    }
}

/// 家目录的两个环境变量名（只在这里写：「哪个变量算家」是两侧必须对上的契约）。
const HOME_ENV: &str = "HOME";
const USERPROFILE_ENV: &str = "USERPROFILE";

/// **这台机器的家目录只有这一条规矩**（monitor 与后端都调它，[`monitor_data_dir`] 同一家）：
/// 按平台惯例 —— Windows：`USERPROFILE` → `HOME`；其余：`HOME` → `USERPROFILE`（与 Claude Code / Node `os.homedir()` 一致）。
/// 空串当没有；都没有 ⇒ `None`（调用方明说，不猜一个路径）。
pub fn home_dir() -> Option<std::path::PathBuf> {
    home_dir_from(&|k| std::env::var_os(k))
}

/// 同上，环境由 `get` 答（注入环境的调用方与判据用）。
pub fn home_dir_from(
    get: &dyn Fn(&str) -> Option<std::ffi::OsString>,
) -> Option<std::path::PathBuf> {
    home_dir_on(get, cfg!(windows))
}

/// 规矩本体：`windows` = 按 Windows 那一臂取（判据两臂各喂一次）。
pub fn home_dir_on(
    get: &dyn Fn(&str) -> Option<std::ffi::OsString>,
    windows: bool,
) -> Option<std::path::PathBuf> {
    let order = if windows {
        [USERPROFILE_ENV, HOME_ENV]
    } else {
        [HOME_ENV, USERPROFILE_ENV]
    };
    order
        .iter()
        .find_map(|k| get(k).filter(|v| !v.is_empty()))
        .map(std::path::PathBuf::from)
}

/// 凭据文件住数据目录根上：`<数据目录>/apikey-credentials.json`。
pub fn credentials_path(data_dir: &std::path::Path) -> std::path::PathBuf {
    data_dir.join(FILE_NAME)
}

/// 文件不存在 / 是空的时候给出去的模板：说明 + 一个空字段，让人一眼看出该填哪儿。JSON 没有注释语法，所以说明写成一个未知键（`_note`），
/// 而「未知键原样保留」正是 [`merge_key`] 的性质。三句说明住文案表（`credsStore.template.*`）；JSON 骨架留这里，运行期拼（键序与 [`to_pretty_json`] 同一个排法）。
/// 不列举 `auth_style` 的合法值：闭集只住 [`AuthStyle::ALL`]，合法值由上游选择装表时现算印出来（`accounts::upstream_select::creds::announce`）。
pub fn template() -> String {
    let example = format!(
        "\"{ACCOUNTS_FIELD}\": {{ \"my-account\": {{ \"{KEY_FIELD}\": \"sk-...\", \"base_url\": \"https://api.example.com\" }} }}"
    );
    let note = copy_core::copy_text("credsStore.template.note", &[("keyField", KEY_FIELD)]);
    let note_accounts = copy_core::copy_text(
        "credsStore.template.noteAccounts",
        &[
            ("accountsField", ACCOUNTS_FIELD),
            ("keyField", KEY_FIELD),
            ("example", &example),
        ],
    );
    let note_auth_style = copy_core::copy_text("credsStore.template.noteAuthStyle", &[]);
    let mut doc = Map::new();
    doc.insert("_note".into(), Value::String(note));
    doc.insert("_note_accounts".into(), Value::String(note_accounts));
    doc.insert("_note_auth_style".into(), Value::String(note_auth_style));
    doc.insert(ACCOUNTS_FIELD.into(), Value::Object(Map::new()));
    doc.insert(KEY_FIELD.into(), Value::String(String::new()));
    to_pretty_json(&doc)
}

/// 读不动那份文件时的说法。**三态，不是两态** —— 「不存在」与「读坏了」必须分开：
/// 前者是正常的（还没配），后者要出声（人手编时打错了一个逗号，不该被当成「没配」）。
#[derive(Debug, PartialEq, Eq)]
pub enum StoreError {
    /// 不是合法 JSON。带上解析器的话，人手编打错时能看懂。
    NotJson(String),
    /// 是合法 JSON，但顶层不是对象（例如整份是个数组）。
    NotAnObject,
}

impl StoreError {
    /// 给人看的那一句（`path` = 读的是哪一份文件）＋ 解析器的原话（进复制详情 / 日志，不上句子）。不再把整份模板插进报错。
    pub fn said(&self, path: &std::path::Path) -> copy_core::said::Said {
        match self {
            StoreError::NotJson(e) => copy_core::said::Said::with_raw(
                copy_core::copy_text("credsStore.error.notJson", &[]),
                e,
            ),
            StoreError::NotAnObject => copy_core::copy_text(
                "credsStore.error.notObject",
                &[("path", &path.display().to_string()), ("shape", "{ … }")],
            )
            .into(),
        }
    }
}

/// 解析一份盘上的内容。**空内容 = 空对象**（还没配，不是错）。
pub fn parse(raw: &str) -> Result<Map<String, Value>, StoreError> {
    if raw.trim().is_empty() {
        return Ok(Map::new());
    }
    let v: Value = serde_json::from_str(raw).map_err(|e| StoreError::NotJson(e.to_string()))?;
    match v {
        Value::Object(m) => Ok(m),
        _ => Err(StoreError::NotAnObject),
    }
}

/// 从一份已解析的文档里取 key。没有 / 不是字符串 / 是空串 ⇒ `None`（都当成「还没配」）。
///
/// ⚠ **不许在这里对 key 做任何「顺手清理」**（去引号、剥前缀…）：
/// 人手编写进去什么，上游就该收到什么。猜错一次的代价是一条查不出来的 401。
/// 只 `trim` 首尾空白 —— 那是编辑器留下的，不是人的意思。
pub fn read_key(doc: &Map<String, Value>) -> Option<SecretKey> {
    let s = doc.get(KEY_FIELD)?.as_str()?.trim();
    if s.is_empty() {
        return None;
    }
    Some(SecretKey::new(s))
}

/// 从一份已解析的文档里取上游端点。缺席 / 不是字符串 / 空串 ⇒ `None`（用默认上游）。
///
/// ⚠ 与 [`read_key`] 同一条纪律：**只 `trim` 首尾空白，别的一个字节都不动**。
/// 「顺手补个 `https://`」这种清理猜错一次的代价是**连到另一个地方去**。
fn read_base_url(doc: &Map<String, Value>) -> Option<String> {
    let s = doc.get(BASE_URL_FIELD)?.as_str()?.trim();
    if s.is_empty() {
        return None;
    }
    Some(s.to_string())
}

/// 这一行的 key 用哪种鉴权头交给上游。按协议形状切，不按供应商：现实里只有两种鉴权头形状（`Authorization: Bearer` · `x-api-key`）加一种「不发」；
/// DeepSeek / Kimi / Qwen 用哪一种，是那一行 `auth_style` 里写着的，不是本 crate 里判的。
/// 不说上游要哪种请求体、路径长什么样、用哪个模型名（中转对 body 零解析）。
/// 值的闭集只有一个住址 —— [`AuthStyle::ALL`]（[`AuthStyle::from_field_value`] 也是从它派生的）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthStyle {
    /// `Authorization: Bearer <key>`。缺席时的默认 ⇒ 一份没写 `auth_style` 的文件，上游收到的字节不变。
    Bearer,
    /// `x-api-key: <key>`。
    XApiKey,
    /// 一个鉴权头都不发，并且把客户端自带的那几个也丢掉。本地部署那一格：本机跑的推理服务默认不校验凭据，
    /// 把一把真 key 发给一个不校验的本地端点是把凭据白送出去 ⇒ 无鉴权要能显式表示。
    NoAuth,
}

impl AuthStyle {
    /// 缺席 / 空串时用哪一个。**取今天的行为**，不是取「最安全的那个」——
    /// 换默认值等于给每一份既有文件改行为，那是另一次决定，要另一件去做。
    pub const DEFAULT: AuthStyle = AuthStyle::Bearer;

    /// 这个闭集的**唯一住址**。判据与日志都从这里派生，不许再写第二份字面量。
    pub const ALL: &'static [AuthStyle] =
        &[AuthStyle::Bearer, AuthStyle::XApiKey, AuthStyle::NoAuth];

    /// 人在文件里写的那个词。**这是契约**（手编时写的就是它）。
    ///
    /// ⚠ 刻意不叫 `"anthropic"` / `"openai"`：那两个词说的是**方言**，
    /// 而本枚举一个字节的 body 都不看 —— 用方言名给它命名，会让读的人以为
    /// 配了它中转就会替他翻译。**它不会。**
    pub fn field_value(self) -> &'static str {
        match self {
            AuthStyle::Bearer => "bearer",
            AuthStyle::XApiKey => "x-api-key",
            AuthStyle::NoAuth => "none",
        }
    }

    /// 反过来：文件里那个词 → 这个值。认不出就是 `None`（调用方要**出声**，不许回落）。
    ///
    /// ★ 它**从 [`AuthStyle::ALL`] 派生**，不是第二个 `match`：加一个成员，
    /// 这一头自动跟上；写成第二个 `match` 的话，漏一支就是一次静默回落。
    pub fn from_field_value(s: &str) -> Option<AuthStyle> {
        let s = s.trim();
        AuthStyle::ALL
            .iter()
            .copied()
            .find(|v| v.field_value().eq_ignore_ascii_case(s))
    }
}

/// 文件里那一格 `auth_style` 的**三种状态**。
///
/// # ⚠ 为什么是三态而不是 `Option<AuthStyle>`
///
/// 「缺席」与「写了一个认不出的词」**必须分开**：合成一态就只能回落成默认，
/// 而一个打错字母的 `auth_style` 静默走 Bearer 的症状是**一条查不出来的 401**
/// ——与「key 打错了」同形。〔`K-R21` 那一族逐字：一个 `None` 装了三件事。〕
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthStyleSetting {
    /// 字段缺席 / 空串 ⇒ 用 [`AuthStyle::DEFAULT`]。
    Absent,
    /// 认得的那几个之一。
    Known(AuthStyle),
    /// 字段在、但不是认得的那几个之一。
    ///
    /// ⚠ **它刻意什么都不带** —— 不留那个原字符串。理由：它下一跳的落点是
    /// 中转的启动日志，而 `creds_guard` 那张白名单要的正是「进日志的东西不含文件内容」。
    /// 认不出的那个词长什么样，人自己看那份文件；日志只说「这一条的 auth_style 认不出」。
    Unknown,
}

/// 从一份已解析的（子）文档里取 `auth_style`。
///
/// ⚠ 与 [`read_key`] / `read_base_url` 同一条纪律：只 `trim` 首尾空白。
/// 值不是字符串（有人写成数字 / 对象）⇒ 与「写了个认不出的词」**同归 `Unknown`**：
/// 两者都是「人写了东西但我读不懂」，都该出声。
pub fn read_auth_style(doc: &Map<String, Value>) -> AuthStyleSetting {
    let Some(v) = doc.get(AUTH_STYLE_FIELD) else {
        return AuthStyleSetting::Absent;
    };
    let Some(s) = v.as_str() else {
        return AuthStyleSetting::Unknown;
    };
    if s.trim().is_empty() {
        return AuthStyleSetting::Absent;
    }
    match AuthStyle::from_field_value(s) {
        Some(k) => AuthStyleSetting::Known(k),
        None => AuthStyleSetting::Unknown,
    }
}

/// 表里的一条。**上游与 key 在这里还是分开的两个值** ——
/// 把它们焊成一个不可分解的值是**上游选择**的活（`accounts::upstream_select::table` 的 `Row`）。
///
/// ⚠ **刻意不 `derive(Debug)`**：同 `relay::server::Relay` 那条（`KS1` 的第二道）。
/// `SecretKey` 自己的 `Debug` 是遮蔽形，但**少一个能顺手印整条的入口就少一个出口**。
pub struct AccountEntry {
    /// 路由键里那一段账号 id。
    pub id: String,
    /// 这一行的上游端点；`None` = 用这个 agent 的默认上游（适配层 `agents::Adapter::upstream`）。
    pub base_url: Option<String>,
    /// 这一行的 key；`None` = **原样转发下游那份鉴权头**（订阅制那一档是合法状态）。
    pub key: Option<SecretKey>,
    /// 这一行的鉴权头风格。**三态原样带出去，本模块不替它做决定** ——
    /// 同 `base_url`：把 `Unknown` 折成默认值是一次静默回落，而出声那一步在上游选择装表那侧
    /// （它才有日志出口）。
    pub auth_style: AuthStyleSetting,
}

/// 把一份文档读成一张表。顺序由键名定（遍历走 [`ordered_keys`]，与 `Map` 是 `BTreeMap` 还是 `IndexMap` 无关）。
///
/// 1. [`ACCOUNTS_FIELD`] 那张表逐条读：每一个对象都是一条，两个字段一个都没填也算（`base_url` 空 = 用默认上游，`api_key` 空 = 原样转发
///    客户端自己那份鉴权头 —— 显式的一条透传路）；值不是对象的那一条跳过。
/// 2. 顶层那把 key / 那个 `base_url` ⇒ 一条 id 逐字是 [`LEGACY_ACCOUNT_ID`] 的行；`accounts` 里已经有同名的那一条 ⇒ 顶层那半不再加（人手写的那条优先）。
///    顶层完全没有 `api_key` / `base_url` ⇒ 一条都不加（加一条「什么都没配的默认行」就给「查不到就用它」开了门）：没配就是零条，零条就是全部 404。
///
/// 不判 `id` 能不能当路由段、不解析 `base_url`（那两样要后端那一侧的谓词，本 crate 不认识 HTTP）⇒ 由上游选择在装表那一刻判并出声。
pub fn read_accounts(doc: &Map<String, Value>) -> Vec<AccountEntry> {
    let mut out: Vec<AccountEntry> = Vec::new();

    if let Some(Value::Object(m)) = doc.get(ACCOUNTS_FIELD) {
        for id in ordered_keys(m.keys()) {
            let Some(obj) = m[id].as_object() else {
                continue;
            };
            out.push(AccountEntry {
                id: id.clone(),
                base_url: read_base_url(obj),
                key: read_key(obj),
                auth_style: read_auth_style(obj),
            });
        }
    }

    if !out.iter().any(|e| e.id == LEGACY_ACCOUNT_ID) {
        let key = read_key(doc);
        let base_url = read_base_url(doc);
        // ⚠⚠ **顶层的 `auth_style` 单独在，不足以造出这一行**：
        //   它进不了上面那个 `if` 的条件，是刻意的 —— 加进去就等于「只写了
        //   `auth_style` 的文件也有一条什么都没配的 default 行」，而那正是本函数头注
        //   逐字禁的**回落**。⇒ 它只在这一行**因为别的字段已经存在**时被读进来。
        if key.is_some() || base_url.is_some() {
            out.push(AccountEntry {
                id: LEGACY_ACCOUNT_ID.to_string(),
                base_url,
                key,
                auth_style: read_auth_style(doc),
            });
        }
    }

    out.sort_by(|a, b| a.id.cmp(&b.id));
    out
}

/// 只改 `accounts.<id>.api_key` 那一格，别的条一个字节都不动，两层的未知键都留着。
/// 不收「整张 accounts」：公开面上不许有整份替换 `accounts` 子对象的路，改别的条这件事在本模块的公开面上不可表示。
/// 它挡住的是走本模块的写者：盘上那份文件仍然可以被别的代码整份覆盖（原子替换那一步在 `src/frontend/shell/src/creds_store.rs` 里）。
/// 本函数不自己碰明文：它把那一格转交给 [`merge_key`]，`expose_for_persisting` 的调用点仍然恰好 1 处（在 `merge_key` 里）。
pub fn merge_account_key(
    current: &Map<String, Value>,
    id: &str,
    key: &SecretKey,
) -> Map<String, Value> {
    let mut out = current.clone();

    // ★ 两层都是 clone-then-replace：顶层的未知键、这一条之外的每一条、
    //   以及**这一条自己**的未知键，三样都留着。
    let mut accounts = match out.get(ACCOUNTS_FIELD).and_then(Value::as_object) {
        Some(m) => m.clone(),
        None => Map::new(),
    };
    let entry = match accounts.get(id).and_then(Value::as_object) {
        Some(m) => m.clone(),
        None => Map::new(),
    };
    accounts.insert(id.to_string(), Value::Object(merge_key(&entry, key)));
    out.insert(ACCOUNTS_FIELD.to_string(), Value::Object(accounts));
    out
}

// `base_url` 写之前的那一道形状关搬走了：写口与装表同一个谓词，住共享 crate `upstream_url_core::usable`。

/// 把**某一条**的 `base_url` 并进一份刚从盘上读回来的文档，其余键一个不动。
///
/// 形状与 [`merge_account_key`] 逐条相同（clone-then-replace 两层、签名逼调用方说清改哪一条），
/// 只是这一格不是凭据：`base_url` 是明文端点，不经 `SecretKey`。
/// ⚠ 本 crate 不解析它（不认识 HTTP，见 [`read_accounts`] 头注）—— 形状对不对由上游选择装表时判、并出声。
pub fn merge_account_base_url(
    current: &Map<String, Value>,
    id: &str,
    base_url: &str,
) -> Map<String, Value> {
    let mut out = current.clone();
    let mut accounts = match out.get(ACCOUNTS_FIELD).and_then(Value::as_object) {
        Some(m) => m.clone(),
        None => Map::new(),
    };
    let mut entry = match accounts.get(id).and_then(Value::as_object) {
        Some(m) => m.clone(),
        None => Map::new(),
    };
    entry.insert(
        BASE_URL_FIELD.to_string(),
        Value::String(base_url.trim().to_string()),
    );
    accounts.insert(id.to_string(), Value::Object(entry));
    out.insert(ACCOUNTS_FIELD.to_string(), Value::Object(accounts));
    out
}

/// 删号那一步：把 `accounts.<id>` 那一条整条摘掉，别的条与两层的未知键一个不动。那一条本来就不在 ⇒ 原样返回。
///
/// 形状同 [`merge_account_key`]：签名逼调用方说清摘哪一条，「整份替换 accounts」在公开面上仍不可表示。
pub fn remove_account(current: &Map<String, Value>, id: &str) -> Map<String, Value> {
    let mut out = current.clone();
    if let Some(Value::Object(accounts)) = out.get_mut(ACCOUNTS_FIELD) {
        accounts.remove(id);
    }
    out
}

/// 回滚那一步：把 `from`（删之前留的那一份）里 `accounts.<id>` 那一条原样放回 `current` ——
/// 只这一条，`current` 里别的条（删号之后别人写进来的也算）与未知键一个不动。`from` 里没有它 ⇒ 原样返回。
pub fn restore_account(
    current: &Map<String, Value>,
    id: &str,
    from: &Map<String, Value>,
) -> Map<String, Value> {
    let Some(entry) = from
        .get(ACCOUNTS_FIELD)
        .and_then(Value::as_object)
        .and_then(|m| m.get(id))
        .filter(|v| v.is_object())
    else {
        return current.clone();
    };
    let mut out = current.clone();
    let mut accounts = match out.get(ACCOUNTS_FIELD).and_then(Value::as_object) {
        Some(m) => m.clone(),
        None => Map::new(),
    };
    accounts.insert(id.to_string(), entry.clone());
    out.insert(ACCOUNTS_FIELD.to_string(), Value::Object(accounts));
    out
}

/// 把 key 并进一份刚从盘上读回来的文档，其余键一个不动。收 `current` 而不是 `&mut self`：同一个文件有两个写者（程序 vs 人手）时，
/// 拿一份「界面打开时读的那一份」写回会把另一方刚写进去的键盖掉 ⇒ 签名逼着调用方在写的那一刻把盘上的当前内容递进来。
pub fn merge_key(current: &Map<String, Value>, key: &SecretKey) -> Map<String, Value> {
    merge_secret(current, KEY_FIELD, key)
}

/// 把一格秘密并进一份刚从盘上读回来的文档（那一格换掉，其余键一个不动）。**落盘出口的唯一调用点**：
/// API key（[`merge_key`]）与登录令牌（`token::merge_tokens`）都经它。
pub fn merge_secret(
    current: &Map<String, Value>,
    field: &str,
    key: &SecretKey,
) -> Map<String, Value> {
    let mut out = current.clone();
    out.insert(
        field.to_string(),
        // ★ 用的是**落盘那个出口**，不是换头那个。两个出口的人群刻意不相交，
        //   见 `SecretKey::expose_for_persisting` 的头注。
        Value::String(key.expose_for_persisting().to_string()),
    );
    out
}

/// 键的落盘顺序：按名字，不按到达先后（收迭代器，与 `serde_json::Map` 是哪种实现无关）。
pub fn ordered_keys<'a>(keys: impl Iterator<Item = &'a String>) -> Vec<&'a String> {
    let mut v: Vec<&'a String> = keys.collect();
    v.sort();
    v
}

/// 序列化成盘上那份文本：每一层对象（含数组里的对象）的键按名字排，数组照原序（顺序是数据）。
/// 排序在渲染时做，不靠 `serde_json::Map` 是哪种实现；格式与 `serde_json::to_string_pretty` 逐字节相同。
pub fn to_pretty_json(doc: &Map<String, Value>) -> String {
    render(&Value::Object(doc.clone()), |k| k.sort())
}

/// 按 `order` 排每一层对象的键，写成两格缩进的 JSON（末尾换行）。
fn render(v: &Value, order: fn(&mut Vec<&String>)) -> String {
    let mut out = String::new();
    write_value(v, order, 0, &mut out);
    out.push('\n');
    out
}

fn write_value(v: &Value, order: fn(&mut Vec<&String>), depth: usize, out: &mut String) {
    let pad = |d: usize, out: &mut String| out.push_str(&"  ".repeat(d));
    match v {
        Value::Object(m) if !m.is_empty() => {
            let mut keys: Vec<&String> = m.keys().collect();
            order(&mut keys);
            out.push_str("{\n");
            for (i, k) in keys.iter().enumerate() {
                if i > 0 {
                    out.push_str(",\n");
                }
                pad(depth + 1, out);
                out.push_str(&Value::String((*k).clone()).to_string());
                out.push_str(": ");
                write_value(&m[*k], order, depth + 1, out);
            }
            out.push('\n');
            pad(depth, out);
            out.push('}');
        }
        Value::Array(a) if !a.is_empty() => {
            out.push_str("[\n");
            for (i, x) in a.iter().enumerate() {
                if i > 0 {
                    out.push_str(",\n");
                }
                pad(depth + 1, out);
                write_value(x, order, depth + 1, out);
            }
            out.push('\n');
            pad(depth, out);
            out.push(']');
        }
        other => out.push_str(&other.to_string()),
    }
}

#[cfg(test)]
#[path = "../../../../tests/common/creds-core/store_tests.rs"]
mod tests;
