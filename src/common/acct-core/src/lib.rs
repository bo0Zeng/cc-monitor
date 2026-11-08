//! 账号库清单契约的**唯一定义**（清单常量 · 全仓唯一的号表类型 [`AccountBook`]），外加平台无关的名字安全判据。账号库那几条帧命令的线上形状住 `cmd-wire::accounts`。
//!
//! 今天的消费者：
//! - 后端：`accounts/manage/`（写账号库）· `accounts/manage/library.rs`（读清单）· `accounts/upstream_select/file_face.rs` ·
//!   `accounts/upstream_select/endpoint.rs` · `control/ccm/plan.rs`；号表由 `accounts/manage` 读清单时填，`accounts/upstream_select/rotate.rs`（换号）· `control/ccm/plan.rs`（起会话）· `accounts/manage/library.rs`（账号清单）读；
//! - 界面只经**生成物**用它：`tests/frontend/shell/payload_judgment_rules.rs` 从 [`AUTH_KINDS`] 现生成
//!   `src/frontend/ui/generated/judgment-rules.ts`。
//!
//! # 为什么 `is_safe_config_dir` 两种形没有合成一个
//!
//! 判据按它防的东西拆开：① shell 元字符与视觉欺骗字符 = **平台无关的安全性质**，一套（[`config_dir_char_unsafe`]）；
//! ② 「是绝对路径」= **平台相关的形式**，POSIX 形（[`config_dir_posix_ok`]，拼 POSIX 命令之前）与任一平台形（[`config_dir_ok`]，
//! 读清单时认 Windows 盘符）各一条。`\` 不在拒绝集里：它是 Windows 的分隔符，由 POSIX 形那一条自己拒。

// 账号库与清单住哪（相对家目录）是后端的家那一族，住 `relay_route_core::ACCOUNTS_DIR_REL` / `ACCOUNTS_MANIFEST_NAME`。
/// 凭据文件名（每个账号的 config dir 下）；只 stat 存在性，**绝不读内容**。
pub const CREDENTIALS_NAME: &str = ".credentials.json";

/// 这台本来那一份登录（不设配置目录的状态）的号名：账号库清单里写它的名字、`/t/` 路由第 2 段写它的账号段、号名的字认它，都是这一个。
/// 起会话时没说是哪个号（路由段 `_`）用的也是这一份登录 —— 两个段写给人看的是同一个名字。
pub const BARE_ACCOUNT: &str = "0";
/// 本仓支持的 manifest schema 版本。**不支持的版本不是错误**，是「未启用多账号」。
///
/// 加 `authKind` 这一维**没有 bump**：它是可选键，读侧一律 `#[serde(default)]` ＋ 忽略未知键 ⇒ 新旧清单双向都读得动；
/// bump 反而会让盘上所有 `version: 1` 的清单被读成「没启用多账号」，整张账号列表消失。
pub const SUPPORTED_SCHEMA: u64 = 1;

// ---------------------------------------------------------------- 鉴权方式

/// 鉴权方式 = 订阅登录（`~/.claude` 那套 `.credentials.json`）。
pub const AUTH_KIND_SUBSCRIPTION: &str = "subscription";
/// 鉴权方式 = 第三方 / 官方 API key（**不**看订阅凭据文件）。
pub const AUTH_KIND_API_KEY: &str = "api-key";

/// 鉴权方式的闭集。不是 `bool`：这一维看得见第三档（`apiKeyHelper` / bedrock / vertex 各是一种鉴权方式）。
/// 本数组只是「今天认识哪些字面量」。加第三档：这里加一个常量 + `auth_ready` 里给它一条规则（界面那份 `AuthKind` 从这里现生成，
/// 后端成品与金样 `tests/__fixtures__/accounts.golden.json` 跟着变）。
pub const AUTH_KINDS: [&str; 2] = [AUTH_KIND_SUBSCRIPTION, AUTH_KIND_API_KEY];

/// manifest 的 `authKind` 键 → 鉴权方式。这是这条分类规则的唯一住址。
/// - 缺席（旧 manifest / 写侧还没加这个键）⇒ [`AUTH_KIND_SUBSCRIPTION`]。
/// - 认不出的值（写侧先加了 `bedrock` 而读侧还没升）⇒ 也落 [`AUTH_KIND_SUBSCRIPTION`]，刻意的保守方向：订阅这一档保留
///   「缺凭据 ⇒ 不可选」那道保护（用户看到「未登录」，看得见、可修），落到 api-key 那一档会让它变成可选却断开（看不见的坏）。
pub fn auth_kind_from_manifest(raw: Option<&str>) -> &'static str {
    match raw {
        Some(s) if s == AUTH_KIND_API_KEY => AUTH_KIND_API_KEY,
        _ => AUTH_KIND_SUBSCRIPTION,
    }
}

/// 分类的第二个输入：这台机器的 apikey 表里有没有这个号的一行。有 ⇒ [`AUTH_KIND_API_KEY`]；没有 ⇒ 原样回 `manifest_kind`。
/// 清单里 `authKind` 缺席、key 后配进 apikey 表的号，按上面那条会落订阅而选不中 —— 这一格兜住。「这个号是什么种类」只许有一个家：
/// 界面各处照读 `authKind`，可选性也经 [`auth_ready`] 跟着对上。喂第二个输入的是那台机器自己的后端（`accounts-list` 出成品时读它自己那份 apikey 表）。
pub fn auth_kind_with_apikey_table(
    manifest_kind: &'static str,
    in_apikey_table: bool,
) -> &'static str {
    if in_apikey_table {
        AUTH_KIND_API_KEY
    } else {
        manifest_kind
    }
}

/// 「一个 configDir 对应 apikey 表里哪个 id」—— 路径的最后一段（`Path::file_name`）。这是这条规则的唯一住址：
/// 两边各写一个 basename 规则，漂开的那天症状是「账号页说走 apikey 端点改写、起会话时没走」，而两边看起来都没错。
pub fn apikey_account_id_of_dir(config_dir: &str) -> Option<String> {
    std::path::Path::new(config_dir.trim())
        .file_name()
        .and_then(|s| s.to_str())
        .map(str::to_string)
}

/// 给一批 configDir 与这台机器 apikey 表里的 id，答「哪几个号在表里有行」。这是这条规则的唯一住址。
/// 「有行」说的是 (agent, 账号) 这一对：凭据文件里的行只属于 `table_agent` 那一家（后端传 `accounts::upstream_select::CREDENTIALS_FILE_AGENT`）⇒ `agent` 不是那一家 ⇒ 空集。
/// 它答的是「表里有没有这一行」，不是「这个 key 能不能用」，也不是「这次拉起会不会真的注入」（那还要过「中转在不在跑」那一格）。
pub fn apikey_routed_subset(
    config_dirs: &[String],
    rows: &[String],
    agent: &str,
    table_agent: &str,
) -> Vec<String> {
    if agent != table_agent {
        return Vec::new();
    }
    config_dirs
        .iter()
        .filter(|d| apikey_account_id_of_dir(d).is_some_and(|id| rows.iter().any(|r| *r == id)))
        .cloned()
        .collect()
}

/// 「鉴权方式这一维不再阻塞这个号被选中」。这是这条规则的唯一住址（读者：`src/backend/accounts/manage/library.rs`）。
/// - 订阅号：凭据文件在不在。
/// - api-key 号：恒真，它不用那个文件。
/// `true` 不等于「真能连上」（徽章要把这个状态说出来，`src/backend/accounts/manage/library.rs::badge_of`），也不等于「凭据有效」（订阅那一支只 stat 存在性，过期 / 吊销看不出来）。
pub fn auth_ready(auth_kind: &str, credentials_present: bool) -> bool {
    match auth_kind {
        k if k == AUTH_KIND_API_KEY => true,
        _ => credentials_present,
    }
}

// ---------------------------------------------------------------- 能不能选 · 默认号是谁

/// 「这个号能拿来起会话、能被选为默认」。这是这条判定的唯一住址：清单成品逐号写 `selectable`（后端 `accounts/manage/library.rs`），
/// 起会话的判定（`control/launch_account.rs`）与界面都只读那一格。
/// 四条都要：隔离模式（`in-place` 是逃生口、账号 0 是 `bare`）· 鉴权那一维就绪（[`auth_ready`]）· 号目录在 · 清单里写了号目录（账号 0 不经选号）。
/// 跨三处的金样：`tests/__fixtures__/account-default.golden.json`。
pub fn account_selectable(
    mode: &str,
    auth_ready: bool,
    exists: bool,
    has_config_dir: bool,
) -> bool {
    mode == "isolated" && auth_ready && exists && has_config_dir
}

/// **这台的默认号**：清单里标了默认（`isDefault`）的第一个；一个都没标 ⇒ 清单里第一个；清单空 ⇒ 没有。
/// 这是这条判定的唯一住址：清单成品的 `meta.effectiveDefault`（起会话跟随读它）· 命令行不带 `--account` 时（`control/ccm/plan.rs`）都调它；
/// 界面只读 `meta.effectiveDefault`。只说「默认号是谁」，不说它能不能用（那一格是 [`account_selectable`]）。
pub fn effective_default<T>(accounts: &[T], is_default: impl Fn(&T) -> bool) -> Option<&T> {
    accounts
        .iter()
        .find(|a| is_default(a))
        .or_else(|| accounts.first())
}

// ---------------------------------------------------------------- 号表

/// 账号库清单里的一个号 —— 全仓**唯一**的号表（[`AccountBook`]）的一行。本 crate 只放这个类型：清单只由后端 `accounts/manage`
/// 那一个读口读（剥 BOM · 读取上限 · 版本闸都在那里），读出来照盘上原样一行一个；换号 · 起会话 · 账号清单 · 各号之间同步都读这一份，
/// 路由段与是不是按量号照下面的方法现算，不另存一张表。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ManifestAccount {
    pub name: String,
    pub email: Option<String>,
    /// 缺席 = 账号 0 =「不设配置目录」这个状态本身（结构性认：看这个键在不在，不认名字）。
    pub config_dir: Option<String>,
    pub is_default: bool,
    /// 盘上 `mode` 原样（缺席 ⇒ 读的那一处按 `isolated`）。
    pub mode: Option<String>,
    /// 盘上 `authKind` 原样（缺席 = 订阅号；分类规则只住 [`auth_kind_from_manifest`]）。
    pub auth_kind: Option<String>,
}

impl ManifestAccount {
    /// `/t/` 路由第 2 段：账号 0 ⇒ [`BARE_ACCOUNT`]；其余 ⇒ 号目录末段（[`apikey_account_id_of_dir`]，同一条规则）。
    pub fn route_id(&self) -> Option<String> {
        match self.config_dir.as_deref() {
            None => Some(BARE_ACCOUNT.to_string()),
            Some(d) => apikey_account_id_of_dir(d),
        }
    }

    /// 按量号（API key）。
    pub fn is_api_key(&self) -> bool {
        auth_kind_from_manifest(self.auth_kind.as_deref()) == AUTH_KIND_API_KEY
    }
}

/// 这台的号表：账号库清单读出来的每一个号，照清单顺序（含账号 0 那一条 —— 清单里写了才有）。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AccountBook {
    pub accounts: Vec<ManifestAccount>,
}

impl AccountBook {
    /// 路由段是 `id` 的那个号（[`ManifestAccount::route_id`]）。
    pub fn by_route(&self, id: &str) -> Option<&ManifestAccount> {
        self.accounts
            .iter()
            .find(|a| a.route_id().as_deref() == Some(id))
    }

    pub fn by_name(&self, name: &str) -> Option<&ManifestAccount> {
        self.accounts.iter().find(|a| a.name == name)
    }

    /// 具名号：写了号目录的那几个，照清单顺序。
    pub fn named(&self) -> impl Iterator<Item = &ManifestAccount> {
        self.accounts.iter().filter(|a| a.config_dir.is_some())
    }

    /// 换号那一侧：账号 0 恒在、排第一个（「裸起」这个状态永远可达）—— 清单里写了那一条就挪到最前，没写就补一条（名字 [`BARE_ACCOUNT`]）。
    pub fn with_bare_first(mut self) -> Self {
        let bare = match self.accounts.iter().position(|a| a.config_dir.is_none()) {
            Some(i) => self.accounts.remove(i),
            None => ManifestAccount {
                name: BARE_ACCOUNT.to_string(),
                ..Default::default()
            },
        };
        self.accounts.insert(0, bare);
        self
    }
}

/// 视觉欺骗字符：看不见或会改变渲染方向 / 边界的码位。账号名与 config dir 会进 UI、也会进命令串（一个夹带 RLO 的名字能在界面上显示成另一个账号），
/// 两侧读的是同一份 manifest ⇒ 「什么算欺骗」只有这一份。集合是并集，并且自足：含 `U+2060..=U+2064` · `U+1680` · `U+2000..=U+200A` · `U+202F` · `U+205F` · `U+3000`
/// 这些 `char::is_control()` 不认的码位；`U+0085`（NEL）`is_control()` 已认，仍留在集合里，让调用方不另查也有完整保护。
pub fn is_deceptive_char(c: char) -> bool {
    matches!(c,
        '\u{0085}'                  // NEL（C1 换行；is_control 已覆盖，留此为让集合自足）
        | '\u{00A0}'                // NBSP
        | '\u{1680}'                // Ogham space mark
        | '\u{2000}'..='\u{200A}'   // en/em 等各类空格
        | '\u{200B}'..='\u{200F}'   // 零宽空格/连接符 + LRM/RLM
        | '\u{2028}' | '\u{2029}'   // 行分隔 / 段分隔
        | '\u{202A}'..='\u{202E}'   // 双向嵌入/覆盖
        | '\u{202F}'                // narrow NBSP
        | '\u{205F}'                // medium mathematical space
        | '\u{2060}'..='\u{2064}'   // word joiner / 不可见运算符
        | '\u{2066}'..='\u{2069}'   // 双向隔离
        | '\u{3000}'                // ideographic space
        | '\u{FEFF}'                // ZWNBSP / BOM
    )
}

// ═══════════════════════════════════════════════════════════════════════════
// 〔`INVARIANTS §47` ②〕**账号配置目录的全表** —— 全仓唯一一份。
//
// 配置目录是本仓自管的路径（manifest 里来的），拼进命令之前走**全表**：形式（绝对 · 无 `..` 段）＋ 拒绝集
// （控制符 · 元字符 · 视觉欺骗字符），不是自由文本那一层的「只拒 NUL / CR / LF」。
// 它原来住两处、各一份：monitor 载荷那一层那一份（POSIX 形 ＋ 拒 `\`；那一层后来随起会话只交一行 `ccm …` 删了）与后端
// `observe/accounts_query.rs::is_safe_config_dir`（任一平台形）；而后端 ccm 起会话那一侧（`control/`）要全表却够不着
// （`control → observe` 是禁止方向，TL3 交接的那一格）。⇒ 两份与它们共用的元字符表都搬到这里，
// 两个旧名字各留一个转手的薄壳（调用方与既有判据一个不动），`control` 直接用这里。
// 拆法照 `N-F1c`：「平台无关的安全性质（拒绝集）＋ 平台相关的形式」。
// ═══════════════════════════════════════════════════════════════════════════

/// 两种 shell 共用的元字符黑名单（POSIX `'…'` 与 PowerShell `'…'` 里能提前闭合引号或另起命令的那几个）。
///
/// **`\` 不在里面**：Windows 的账号目录长成 `C:\Users\u\.cc-monitor\accounts\work`，把 `\` 一律禁掉等于禁掉整个平台；
/// POSIX 形那一条（[`config_dir_posix_ok`]）自己额外拒它。
pub const CONFIG_DIR_SHELL_META: &str = "'\"`$;|&<>*?()!";

/// 一个字符能不能出现在**要拼进命令**的配置目录里（平台无关的那一半）：控制符（含 C1）· [`CONFIG_DIR_SHELL_META`] · [`is_deceptive_char`]。
pub fn config_dir_char_unsafe(c: char) -> bool {
    c.is_control() || CONFIG_DIR_SHELL_META.contains(c) || is_deceptive_char(c)
}

/// **POSIX 命令面**的配置目录：`/` 开头 · 不是 `/` 本身 · 无 `..` 段 · 无 `\` · 无 [`config_dir_char_unsafe`] 的字符。
/// fail-closed：稍有可疑即判非法，**绝不拼进命令**。（原 monitor 载荷那一层那一份。）
pub fn config_dir_posix_ok(dir: &str) -> bool {
    if !dir.starts_with('/') || dir == "/" || dir.contains("/../") || dir.ends_with("/..") {
        return false;
    }
    !dir.chars().any(|c| c == '\\' || config_dir_char_unsafe(c))
}

/// **任一平台**的配置目录：POSIX 绝对（`/…`）或 Windows 绝对（盘符 `C:\` / `C:/` · UNC `\\server\share`）·
/// 两种分隔符下都无 `..` 段 · 无 [`config_dir_char_unsafe`] 的字符（`\` 在这里是合法分隔符）。
/// （原后端 `observe/accounts_query.rs::is_safe_config_dir`；monitor 的本机账号清单也问那台后端，所以要认 Windows 形。）
pub fn config_dir_ok(p: &str) -> bool {
    let b = p.as_bytes();
    let drive = b.len() >= 3
        && b[0].is_ascii_alphabetic()
        && b[1] == b':'
        && (b[2] == b'\\' || b[2] == b'/');
    let absolute = p.starts_with('/') || drive || p.starts_with("\\\\");
    if !absolute || p == "/" || p.contains("/../") || p.ends_with("/..") {
        return false;
    }
    if p.contains("\\..\\") || p.ends_with("\\..") {
        return false;
    }
    !p.chars().any(config_dir_char_unsafe)
}

#[cfg(test)]
#[path = "../../../../tests/common/acct-core/lib_tests.rs"]
mod tests;

// 测试档的家一开始就是沙箱（`src/common/test-home`）：这一行让链接器把它的启动段带进本 crate 的测试二进制。
#[cfg(test)]
#[used]
static TEST_HOME: fn() = test_home::linked;
