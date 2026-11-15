//! 账号面：`AccountsFace` 一家的账号库布局 · 登录 · 会话环境里的账号键。

use super::*;

/// 一份身份文件的原生根：没设账号环境变量时它住哪。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdentityRoot {
    /// 住 `<配置根>/<名>`（设了账号环境变量就跟着走）。
    ConfigDir,
    /// 住 `$HOME/<名>`（不跟账号环境变量走）。
    Home,
}

/// 一份身份文件是什么。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdentityClass {
    /// 身份本体：必须 600、必须每个号一份、绝不从别的号复制。
    Secret,
    /// 本机状态：每个号一份（共享会串号，或高频写互相覆盖）。
    State,
    /// 跟着别的项走的附属物（备份之类）。
    Derived,
}

/// 见 [`Adapter::accounts`]。
#[derive(Clone, Copy)]
pub struct AccountsFace {
    /// 每个号各有一份的那几项：`(名, 原生根, 类别)`。共享的只是下面两张表里的；两边都不在的名字各号各用各的、不链。
    pub identity: &'static [(&'static str, IdentityRoot, IdentityClass)],
    /// 已知各号共用的顶层项：共享库里有才链回去（没有不建）。与 `shared_on_demand` 不重。
    pub shared: &'static [&'static str],
    /// 那一家按需才在配置根下建、各号该共用的目录：共享库里还没有也照样链（先在共享库里建出来）。
    pub shared_on_demand: &'static [&'static str],
    /// 账号级配置文件的名字（各号的住号的配置目录）。账号 0 那一份住哪照 [`Self::identity`] 里它的原生根推
    /// （家目录 ⇒ `$HOME`，配置根 ⇒ 账号 0 的配置目录；推法只住 `accounts::manage::layout::base_config_of`）。
    pub config_file: &'static str,
    /// 每个号用户级 MCP 住的那一格（各号之间同步的就是它，别的不碰）；这一家不同步 MCP ⇒ `None`。
    pub mcp: Option<McpCells>,
    /// 各号的 MCP 登录住的那份文件（各号之间同步 MCP 登录只经这一格）；这一家不同步 MCP 登录 ⇒ `None`。
    pub mcp_logins: Option<McpLogins>,
    /// 账号 0 的配置目录：家目录 ⇒ 不设号载体时那一家用的配置根（有共用项的家，各号链回去的共享库也是它）。
    /// 「账号 0 住哪」只这一格：账号库布局 · 续期选号 · 信任同步都读它；账号 0 那份配置文件住哪照
    /// [`Self::identity`] 里 [`Self::config_file`] 那一项的原生根推，不另立格。
    pub shared_root: fn(&Path) -> PathBuf,
    /// 一个配置根下登录的邮箱（读不到 ⇒ `None`）。
    pub email_in: fn(&Path) -> Option<String>,
    /// 各号的订阅号登录那份文件的格式；没有 ⇒ `None`。
    pub login: Option<LoginFace>,
    /// 在 app 里登录一个号那一形（起那一家的登录子命令、认链接、判粘回来的东西）；不在 app 里登 ⇒ `None`。
    pub sign_in: Option<SignInFace>,
    /// 账号归属读会话进程环境时读哪几个键（账号 · 上游地址）。
    pub session_env: SessionEnvKeys,
    /// 「这个目录信任过」在那份配置文件里记在哪一格（各号之间同步信任 · 起会话前预标都只碰它）；没有这件事的家 ⇒ `None`。
    pub trust: Option<TrustCells>,
    /// 这一家的号什么时候由我方续期（[`Renew`]）。
    pub renew: Renew,
}

/// 一个号的用户级 MCP 配置那一格（[`AccountsFace::mcp`]）：文件名 · 读 · 改一个服务器。格式知识全在这几个函数里，
/// 通用层只交文本、收文本：一个服务器的定义在两边都是 JSON 值（名 → 定义），盘上写成什么样由这一家定。
#[derive(Clone, Copy)]
pub struct McpCells {
    /// 号的配置目录里那份文件的名字。
    pub file: &'static str,
    /// 文本 ⇒ 各服务器（名 → 定义）；那一节没有 ⇒ 空表；读不懂 ⇒ `Err`（一句话，只带位置，不带任何值）。
    pub read: fn(&str) -> Result<serde_json::Map<String, serde_json::Value>, String>,
    /// 文本 ＋ 名 ＋ 新定义（`None` = 删）⇒ 新文本：只动那一个服务器那一节，别的键与写法原样。
    /// 文件还不在 ⇒ 交空串。读不懂 / 定义写不成那一家的格式 ⇒ `Err`。
    pub edit: fn(&str, &str, Option<&serde_json::Value>) -> Result<String, String>,
}

/// 一个号的 MCP 登录那份文件（[`AccountsFace::mcp_logins`]）：文件名 · 读出各条 · 改一条 · 写前拿哪几把锁 · 一条里通用层要看的那几格。
/// 格式知识全在 `read` · `edit` 里：通用层只交文本、收文本；一条在两边都是 JSON 值（键 → 一条，整条原样搬）。
#[derive(Debug, Clone, Copy)]
pub struct McpLogins {
    /// 号的配置目录里那份文件的名字（一段名字）。
    pub file: &'static str,
    /// 文本 ⇒ 各条（键 → 一条）；文本里没有那一节 ⇒ 空表；读不懂 ⇒ `Err`（一句话，只带位置，不带任何值）。
    pub read: fn(&str) -> Result<serde_json::Map<String, serde_json::Value>, String>,
    /// 文本 ＋ 键 ＋ 新的一条（`None` = 删）⇒ 新文本：只动那一条，别的条 · 别的键与写法原样。读不懂 ⇒ `Err`。
    pub edit: fn(&str, &str, Option<&serde_json::Value>) -> Result<String, String>,
    /// 写一条之前依次拿的锁（那一家自己写这份文件时拿的同一套）；空 ＝ 不锁。任一把有人持着 ⇒ 这一条这一趟不写。
    pub lock: &'static [McpLoginLock],
    /// 一条里放访问令牌 · 刷新令牌 · 过期时刻（毫秒）的那三格（判能不能用、算版本只看它们）。
    pub access: &'static str,
    pub refresh: &'static str,
    pub expires_ms: &'static str,
}

/// 写一条 MCP 登录之前拿的一把锁（[`McpLogins::lock`]）。名字都相对那个号的配置目录。
#[derive(Debug, Clone, Copy)]
pub enum McpLoginLock {
    /// 整份一把 `mkdir` 锁（一段名字）；修改时刻早过 `stale_ms` 毫秒 ⇒ 当无主收回。
    WholeMkdir { name: &'static str, stale_ms: u64 },
    /// 一条一把 `mkdir` 锁：键 ⇒ 名字（一段名字）；修改时刻早过 `stale_ms` 毫秒 ⇒ 当无主收回。
    EntryMkdir {
        name: fn(&str) -> String,
        stale_ms: u64,
    },
    /// 一条一份文件上的排他文件锁：键 ⇒ 那份锁文件（相对路径，最多在一层子目录里，子目录不在就建）；持有方不在了系统自己放。
    EntryFile { name: fn(&str) -> String },
    /// 整份一份文件上的排他文件锁：那份锁文件（相对路径，最多在一层子目录里，子目录不在就建）；持有方不在了系统自己放。
    /// 我方只在写一条时拿（排他）；那一家读时拿的是同一份上的共享锁，排他锁在手时它读不进来、写完它读到的是整份新的。
    WholeFile { name: &'static str },
}

/// `kind` 那一家的号什么时候由我方续期（[`AccountsFace::renew`]）；认不出 · 那一家没有账号面 ⇒ `Anytime`（同原来那一形）。
pub fn renew_of(kind: &str) -> Renew {
    renew_among(crate::agents::REGISTRY, kind)
}

/// [`renew_of`] 照给定注册表那一半。
pub fn renew_among(registry: &[Adapter], kind: &str) -> Renew {
    accounts_face_among(registry, kind).map_or(Renew::Anytime, |f| f.renew)
}

/// 我方什么时候替这一家的一个号续访问令牌。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Renew {
    /// 要用时快过期就续（有会话在用这个号也续：那一家续期与我方续期在同一把锁里排队）。
    Anytime,
    /// 只在此刻没有活会话用这个号时续；有会话在用 ⇒ 留给那一家自己续。
    Idle,
}

/// 信任记在配置文件里的那一格：顶层 `table` 是「目录 → 那个目录的一项」的表；`flag` 是那一项里记「信任过」的那一格
/// （`None` ⇒ 那一项本身就是 `true`）。通用层只按这几格读写，不认任何一家的键名。
#[derive(Clone, Copy)]
pub struct TrustCells {
    pub table: &'static str,
    pub flag: Option<&'static str>,
    /// 一个工作目录要标表里哪几个键（这一家进程查 / 存信任认的那几形，如解开符号链接 · 换分隔符 · 所在仓的根）；
    /// 通用层只把这几个键都标上，不判那一家先查哪一个。
    pub dir_keys: fn(&str) -> Vec<String>,
    /// 这一家自己的改法与读法（那份配置不是 JSON 的家）；声明了 ⇒ 读写都只经它，`table` · `flag` 不看；`None` ⇒ 照 JSON 那一形按上面几格。
    pub edit: Option<TrustEdit>,
}

/// 一家自己的信任改法与读法（[`TrustCells::edit`]）。通用层不解那份文本，只交给这两手。
#[derive(Clone, Copy)]
pub struct TrustEdit {
    /// 原文 ⇒ 信任过的目录（原文是空串 ＝ 那份文件还不在）。读不懂 ⇒ `Err(一句原因)`。
    pub trusted: fn(&str) -> Result<std::collections::BTreeSet<String>, String>,
    /// 原文里把那几个目录键都标成信任过：只动那几格，别的键与写法原样；都已经是 ⇒ `Ok(None)`；读不懂 ⇒ `Err`、不出新原文。
    pub mark: fn(&str, &std::collections::BTreeSet<String>) -> Result<Option<String>, String>,
}

/// 账号归属从会话进程环境里读的那两个键，收成一处：账号（配置根）· 上游地址。
#[derive(Clone, Copy)]
pub struct SessionEnvKeys {
    pub config_dir: &'static str,
    /// 上游地址那个变量；那一家不从环境变量读中转地址（经参数注入）⇒ `None`：会话归号不读它，「经不经本机中转」照起会话时留的上游描述认。
    pub base_url: Option<&'static str>,
    /// 这条会话自己的设置文件会不会压过进程环境里的上游地址：`(配置根, cwd, 家目录)` ⇒ 说不清就是 `true`。
    pub settings_may_set_base_url: fn(Option<&Path>, &Path, Option<&Path>) -> bool,
}

/// `kind` 那一家的账号库布局。认不出 / 那一家没有 ⇒ `None`。
pub fn accounts_face(kind: &str) -> Option<AccountsFace> {
    accounts_face_among(REGISTRY, kind)
}

/// 账号库共享库那一家的账号面。账号库在盘上是几家共用的一份清单（每个号带自己那一家，没写的归 [`account_kind`] 那一家）；
/// 共享库与它的布局（身份项 · 共享项 · 链）只有一份、归清单里不写家的号那一家，别的家的号只列在清单里、不照它布局。
pub fn store_family_face() -> Option<AccountsFace> {
    account_kind(None).and_then(accounts_face)
}

/// 某一家的号载体（环境变量名）：「这一家有没有账号这一维」只经这一口问（`ccm` 收 `--account` / `--account-dir` · 起会话那一行的渲染 · 别处）。
/// 认不出这家 · 那一家没声明 ⇒ `None`。判据喂替身行就喂这里。
pub fn account_env_among(registry: &[Adapter], kind: &str) -> Option<&'static str> {
    adapter_among(registry, kind).and_then(|a| a.account_env)
}

/// [`account_env_among`] 在生产注册表上。
///
/// ⚠ **它是通用层拿这个名字的唯一入口** —— 直接 `use agents::<名>::…` 会让
/// `agent_locality_guard` 判据④的读数凭空上涨，而那个数只许降（见 [`Adapter::account_env`]）。
pub fn account_env_of(kind: &str) -> Option<&'static str> {
    account_env_among(REGISTRY, kind)
}

/// `kind` 那一家的号目录里那份凭据文件的名字（[`LoginFace::creds_file`]）：判号登没登 · 导入凭据写成哪一名都只问它。
/// 认不出 · 那一家没声明 ⇒ `None`。
pub fn creds_file_among(registry: &[Adapter], kind: &str) -> Option<&'static str> {
    accounts_face_among(registry, kind)
        .and_then(|f| f.login)
        .map(|l| l.creds_file)
}

/// 注册表里各家声明的凭据文件名（[`LoginFace::creds_file`]；去重，照注册序）：监看认「某号凭据变了」照它。
pub fn creds_file_names_among(registry: &[Adapter]) -> Vec<&'static str> {
    let mut out = Vec::new();
    for n in registry
        .iter()
        .filter_map(|a| a.accounts.and_then(|f| f.login))
        .map(|l| l.creds_file)
    {
        if !out.contains(&n) {
            out.push(n);
        }
    }
    out
}

/// [`accounts_face`] 的可喂夹具那一半。
pub fn accounts_face_among(registry: &[Adapter], kind: &str) -> Option<AccountsFace> {
    adapter_among(registry, kind).and_then(|a| a.accounts)
}

/// 账号库里一个号是哪一家的号：清单那一格（缺席 ⇒ 缺省那一家），照账号面问 [`kind_with_face_among`]。
pub fn account_kind_among(registry: &[Adapter], agent: Option<&str>) -> Option<&'static str> {
    kind_with_face_among(registry, agent, |a| a.accounts.is_some())
}

/// [`account_kind_among`] 在生产注册表上。
pub fn account_kind(agent: Option<&str>) -> Option<&'static str> {
    account_kind_among(REGISTRY, agent)
}

/// 流式 watcher 盯的各号记录根：根表（[`record_roots_among`]，`homes` 给各家各号的家）上的每一根，照表序；
/// 那一家没有记录解释面 ⇒ 不列；几根链成同一处（解开链接后同一个目录）的只留头一根。
/// `known` 是此刻已经在跟的那几个根（家目录那棵记录树之类）：链回它们的不再算一根。号表变了照它补 · 摘耳朵。
/// 交出的根是解开链接的那一份（`platform::writers::absolute`：写者那一路报回的是真路径，根要跟它比得上；去重也照它）。
pub fn watched_roots_among(
    registry: &[Adapter],
    homes: &dyn Fn(&Adapter) -> Vec<AccountHome>,
    known: &[PathBuf],
) -> Vec<AccountRoot> {
    let real = |p: &Path| crate::platform::writers::absolute(p);
    let mut seen: Vec<PathBuf> = known.iter().map(|p| real(p)).collect();
    let mut out = Vec::new();
    for r in record_roots_among(registry, homes) {
        let Some(a) = adapter_among(registry, r.kind) else {
            continue;
        };
        let Some(face) = a.records else {
            continue;
        };
        let same = real(&r.root);
        if !seen.contains(&same) {
            seen.push(same.clone());
            out.push(AccountRoot {
                kind: a.kind,
                home: r.home.clone(),
                root: same,
                face,
                live: a.live,
            });
        }
    }
    out
}

/// 一个号的一个记录根（[`watched_roots_among`] 的一项）：哪一家 · 那个号的家 · 记录根 · 那一家的记录解释 · 那一家怎么判活。
#[derive(Clone)]
pub struct AccountRoot {
    pub kind: &'static str,
    /// 那个号的家（号目录）。
    pub home: PathBuf,
    pub root: PathBuf,
    pub face: RecordFace,
    pub live: LiveBy,
}

/// 一家在 app 里登录一个号那一形（本体 `accounts/sign_in.rs`：起它、转链接、递码；令牌那一家自己写）。
#[derive(Clone, Copy)]
pub struct SignInFace {
    /// 登录子命令（程序照这台的 `PATH` 找）。
    pub program: &'static str,
    pub args: &'static [&'static str],
    /// 号的配置目录交给这一格；账号 0（默认配置目录）⇒ 摘掉它。
    pub dir_env: &'static str,
    /// 起它时摘掉的（会盖掉那个号自己的登录的那几格）。
    pub env_remove: &'static [&'static str],
    /// 输出的一行里那条登录链接（只认那一家的登录页）；没有 ⇒ `None`。
    pub link_in: fn(&str) -> Option<String>,
    /// 它要终端才肯问（起在伪终端里：`platform::child::Child::converse_in_terminal`）。
    pub terminal: bool,
    /// 这一趟怎么走完：粘回什么（界面据此说「粘登录页上给的码」还是「粘浏览器地址栏里那一整条」；粘回那两形带着怎么判粘的）·
    /// 不粘回（显示码 · 等浏览器回跳）。
    pub paste: PasteShape,
    /// 它自己在它那一台上开浏览器打开登录页（不带「别开浏览器」那一类旗标）。
    pub opens_browser: bool,
    /// 退出登录：同一个程序跑这几个参数（号的配置目录照 `dir_env` 交，摘掉 `env_remove`）；成败只看退出码。这一家不在 app 里退 ⇒ `None`。
    pub sign_out: Option<&'static [&'static str]>,
    /// 登录状态：跑哪条 · 怎么读成登没登。这一家不在 app 里问 ⇒ `None`。
    pub status: Option<SignInStatus>,
}

/// 问一个号登没登那一形（[`SignInFace::status`]）：同一个程序跑 `args`，`read` 把退出码 ＋ 它说的几行读成登没登（读不出 ⇒ `None`）。
#[derive(Clone, Copy)]
pub struct SignInStatus {
    pub args: &'static [&'static str],
    pub read: fn(code: Option<i32>, lines: &[String]) -> Option<bool>,
}

/// 一趟登录怎么走完（中立值；线上名由帧面那一份给）。
#[derive(Clone, Copy)]
pub enum PasteShape {
    /// 粘回登录页上给的那一段码。
    Code(Paste),
    /// 粘回浏览器回跳到本机那一整条地址（回不来时地址栏里那一条）。
    RedirectUrl(Paste),
    /// 显示码：子命令自己印链接与一段码，人在哪台的浏览器里打开链接、填那段码都行；不粘回，它自己退 0 就算成。
    /// 那段码从它说过的几行里读（交这一家的读法；读不出 ⇒ `None`，接着听）。
    ShowCode(fn(&[String]) -> Option<String>),
    /// 不粘回：子命令自己等浏览器回跳到它那一台的本机口、不读 stdin；只在它那一台的浏览器里登得完，它自己退 0 就算成。
    NoPaste,
}

/// 粘回那两形怎么判粘回来的东西（[`PasteShape::Code`] · [`PasteShape::RedirectUrl`] 的载荷）。
#[derive(Clone, Copy)]
pub struct Paste {
    /// 粘回来的东西对不对得上这一趟的链接（格式 ＋ 这一趟的 state）。
    pub fits: fn(pasted: &str, link: &str) -> bool,
    /// 输出的一行说「粘的码不对」（子命令接着等）。
    pub said_bad: fn(&str) -> bool,
}

impl PasteShape {
    /// 「这一形要不要粘回」只这一处：粘回那两形 ⇒ 它怎么判粘的；不粘回的那两形 ⇒ `None`（核心不往 stdin 写、不认「粘的不对」）。
    pub fn pasted(self) -> Option<Paste> {
        match self {
            PasteShape::Code(p) | PasteShape::RedirectUrl(p) => Some(p),
            PasteShape::ShowCode(_) | PasteShape::NoPaste => None,
        }
    }
}

/// 两形相等 ＝ 同一形（各形的载荷不比：函数指针比不出意思）。
impl PartialEq for PasteShape {
    fn eq(&self, other: &Self) -> bool {
        std::mem::discriminant(self) == std::mem::discriminant(other)
    }
}

impl Eq for PasteShape {}

impl std::fmt::Debug for PasteShape {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            PasteShape::Code(_) => "Code",
            PasteShape::RedirectUrl(_) => "RedirectUrl",
            PasteShape::ShowCode(_) => "ShowCode",
            PasteShape::NoPaste => "NoPaste",
        })
    }
}

/// 一家的订阅号登录的格式知识（读写与续期在账号域 `accounts/oauth/`，这里只有这一家的名字与地址）。
#[derive(Debug, Clone, Copy)]
pub struct LoginFace {
    /// 配置目录里那份凭据文件的名字。
    pub creds_file: &'static str,
    /// 凭据文件里放令牌的那一节，与节里的几格。
    pub section: &'static str,
    pub access: &'static str,
    pub refresh: &'static str,
    /// 访问令牌几点过期：那一节里一格毫秒时刻，或交那一家的读法（[`Expiry`]）。
    pub expiry: Expiry,
    /// 那一节里记授权范围的那一格（一串名字）：续期请求带上它（空格连起来），续成写回回包里的那一串。
    /// 这一家不记授权范围 ⇒ `None`：请求体不带 `scope`，回包里的也不写进凭据。
    pub scopes: Option<&'static str>,
    /// 续成写回时，除访问 · 刷新令牌外另换的几格（[`RenewedCell`]）；只换那两格的家 ⇒ 空。
    pub renewed_cells: &'static [RenewedCell],
    /// 这个号登录时用的客户端 id（缺 ⇒ [`Self::client_id`]）。
    pub client_field: &'static str,
    /// 令牌端点（续期发到这里）与缺省的客户端 id：凭据里记了出处（[`Self::origin`]）⇒ 先照出处，没有才用这两格。
    /// 端点写不出（空串）又没有出处 ⇒ 不替这个号续，照实说。
    pub token_url: &'static str,
    pub client_id: &'static str,
    /// 凭据里记着的出处（这个号是在哪个令牌端点 · 用哪个客户端 id 登的）：整份凭据 ⇒ 出处；没记 ⇒ `None`。这一家不记出处 ⇒ `None`。
    pub origin: Option<fn(&serde_json::Map<String, serde_json::Value>) -> Option<LoginOrigin>>,
    /// 续成写回时另记一格「几点续的」：顶层那一格的键 ＋ 此刻（毫秒）⇒ 那一格的值。不记 ⇒ `None`。
    pub renewed_at: Option<(&'static str, fn(u64) -> serde_json::Value)>,
    /// 还剩这么多毫秒就算快过期（同那一家自己的余量）。
    pub margin_ms: u64,
    /// 续期锁：配置目录里那一把的名字 · 配置目录旁边那一把的后缀（那一家自己续期时拿的同一套）。
    /// 那一家续期不拿锁 ⇒ 两格都 `None`：我方不拿、号目录里不多建一样东西（同一台我方几个续期者之间照样经配置目录的跨进程锁排队）。
    pub lock_inside: Option<&'static str>,
    pub lock_beside: Option<&'static str>,
    /// 续期锁目录的修改时刻比现在早过这么多毫秒 ⇒ 持有方已经不在，当无主收回（删了重拿）。
    pub lock_stale_ms: u64,
    /// 那一家**整份写**这份凭据文件时拿的锁（配置目录里那一把的名字与形状，[`WriteLock`]）：它在锁里重读盘、只改自己那一格、整份写回。
    /// 我方任何一次写这份文件也在这把锁里重读、比对、写（续期写回；Claude 的 MCP 登录住同一份文件，它那一格 [`McpLogins::lock`] 也登这一把）。没有这一把的家 ⇒ `None`。
    pub write_lock: Option<WriteLock>,
    /// 那把写锁（`mkdir` 那一形）的修改时刻早过这么多毫秒 ⇒ 当无主收回（文件锁持有方一不在系统就放了，不看它）。
    pub write_lock_stale_ms: u64,
    /// 续成了、写回时那把写锁有人持着 ⇒ 新令牌先留在内存里照用，等它放开（文件事件）再写回；最多等这么多毫秒，
    /// 到期还没放开就照实报写回失败（那个号可能要重新登录）。比 [`Self::write_lock_stale_ms`] 长：到期那一刻再试一次，持有方早已不在的锁就收回了。
    pub write_wait_ms: u64,
    /// 一个号的账号身份住哪份文件：（配置目录, 账号 0 时的家目录）→ 那份文件。
    pub identity_file: fn(&Path, Option<&Path>) -> PathBuf,
    /// 那份文件里的账号身份（请求里跟着鉴权一起换的那一格的值）；读不到 ⇒ `None`。
    pub identity_in: fn(&Path) -> Option<String>,
    /// 请求体里那一格账号身份是谁（这一发实际在用的那份登录）；没有这一格 / 读不出 / 是空串 ⇒ `None`。
    pub identity_of_body: fn(&[u8]) -> Option<String>,
    /// 请求体里那一格账号身份换成给的值（按量号给空串）；只动那几个字节。
    pub rewrite_identity: fn(&[u8], &str) -> IdentityCell,
    /// 账号 0 那份登录我方续不续、写不写（[`BaseLogin`]）；号载体那几个号不看这一格，照常续。
    pub base_login: BaseLogin,
}

/// 一家的账号 0（不设号载体时那份配置）那份登录我方续不续、写不写（[`LoginFace::base_login`]）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BaseLogin {
    /// 同号载体那几个号：照 [`AccountsFace::renew`] 续、续成写回。
    Renew,
    /// 不续不写（那一家自己续它）：令牌没过期照读照用；要续的那一下照实回「不由 cc-monitor 续」，不拿锁、不写盘、不记漏续。
    Untouched,
}

/// 续成写回时另换的一格（[`LoginFace::renewed_cells`]）：回包里 `answer` 那一格 ⇒ 凭据那一节里 `cell` 那一格（当令牌看：不进日志）。
/// 回包里没有 · 是 null · 是空串 ⇒ 这一格不写、盘上原样（别的几格照写）；有而不是一串、或过不了 `check` ⇒ 整次不落盘，照实报回包读不懂。
#[derive(Debug, Clone, Copy)]
pub struct RenewedCell {
    pub answer: &'static str,
    pub cell: &'static str,
    /// 写之前的校验（如「是不是 JWT」）：不过 ⇒ 整次不写。不校验 ⇒ `None`。
    pub check: Option<fn(&str) -> bool>,
}

/// 访问令牌几点过期从哪读（[`LoginFace::expiry`]）。
#[derive(Debug, Clone, Copy)]
pub enum Expiry {
    /// 凭据那一节里一格毫秒时刻（续成写回这一格）。
    Cell(&'static str),
    /// 交那一家读（如解访问令牌 · 上次续的时刻推）：整份凭据 ⇒ 几点过期（毫秒）；读不出 ⇒ `None`（当没写过期时刻）。
    /// 写回不另写时刻那一格。
    Read(fn(&serde_json::Map<String, serde_json::Value>) -> Option<u64>),
}

/// 凭据里记着的出处（[`LoginFace::origin`]）：续期发到这个地址；记了客户端 id 就用它。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoginOrigin {
    pub token_url: String,
    pub client_id: Option<String>,
}

/// 那一家整份写凭据时拿的那一把锁（[`LoginFace::write_lock`]）：名字是配置目录里的一段名字。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteLock {
    /// `mkdir` 成功即持有（proper-lockfile 那一形）；修改时刻过了门限当无主收回。
    Mkdir(&'static str),
    /// 那份文件上的排他文件锁（`flock` · `LockFileEx`）；持有方不在了系统自己放。
    File(&'static str),
}

impl WriteLock {
    /// 那一把的名字。
    pub fn name(self) -> &'static str {
        match self {
            WriteLock::Mkdir(n) | WriteLock::File(n) => n,
        }
    }
}

/// 请求体里账号身份那一格换没换成。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IdentityCell {
    /// 请求体里没有这一格 ⇒ 原样发。
    Absent,
    /// 换好了的整份请求体（只差那一格）。
    Rewritten(Vec<u8>),
    /// 有这一格，但认不准是哪几个字节 ⇒ 不能拿这份请求体换号。
    Unsure,
}
