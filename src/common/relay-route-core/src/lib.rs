//! **后端的门牌与家的契约**：monitor 与后端两边必须对上的那几样 —— 这台机器上后端的落点、常驻监听口与中转口、
//! 中转路由路径的语法，以及后端住在 `~/.cc-monitor/` 下的每一样东西的相对路径。
//!
//! # 它今天管什么
//!
//! - **门牌**：中转口 [`PORT`] · 常驻监听口 [`listen_port_for`] · 两把钥匙 [`KEY_FILE_REL`] / [`LISTEN_TOKEN_FILE_REL`] ·
//!   路由路径的语法（两个前缀 · 段闸 · 拼 · 拆）。
//! - **家**（相对家目录，`.cc-monitor/` 开头的每一个常量）：后端落点 [`BACKEND_LANDING_REL`] · 暂存区 ·
//!   退出行为设置 · 两份别名文件 · skill 装记录 · 资产目录 · 账号库 [`ACCOUNTS_DIR_REL`] 与它的清单 ·
//!   监听口的进程记录 [`listen_pid_file_name`]。
//!   后端各写者引它们落盘；monitor 的数据位置页（`data_paths.rs::backend_entries`）按它们列出，判据两向对着这一族。
//! - crate 名还是「relay-route」—— 它最早只装中转门牌。
//!
//! # 它为什么是一个共享 crate
//!
//! 这几样两个半边都要（端口 · 钥匙文件相对路径 · 两个前缀 · 段闸 · 拼路由），各写一份会零对拍地漂开。
//!
//! 两个二进制不共享源码树（后端刻意不在 monitor 的 workspace 里）⇒ 共享 crate 是「一份实现两侧 use」的唯一载体
//! （同 `shell-quote-core` 的形状）。从此漂开**不可表示**：想不一致得先把 `use` 删掉。
//!
//! # 它**不是**业务 crate
//!
//! 这里一个账号 / 凭据 / 上游的名字都没有：两个段是**位置**（第 1/2 段），谁是 agent、谁是账号只在后端上游选择那一层
//! 才有名字。⇒ 通信层成员 `relay/route.rs` 可以 `use` 它（`C2` 禁的是业务 crate）。
//!
//! # 谁用哪几样
//!
//! - 后端：中转 `relay/route.rs::parse`（[`parse_target`]）· 门 `relay/door.rs`（[`KEY_FILE_REL`]）·
//!   上游选择 `accounts/upstream_select/endpoint.rs`（[`base_url`]：起会话那一发注入哪个地址，**只有它拼**）·
//!   起会话载荷 `control/launch_render/payload.rs`（[`PORT`] · 钥匙段渲成 `$(cat ~/<钥匙>)` 的 [`KEY_FILE_REL`] ·
//!   中转地址的 fail-closed 校验 [`base_url_shape_ok`]（[`base_url`] 的逆）· 起会话身份 token 的字符集 [`segment_is_safe`]）。
//! - monitor：起本机后端时交的端口（[`PORT`]）· 常驻监听口（[`listen_port_for`]）· 后端落点（[`BACKEND_LANDING_REL`]）·
//!   监听口的进程记录（`local_backend_host::pid_path`，[`listen_pid_file_name`]）· 数据位置页列家那一族（`data_paths.rs::backend_entries`）。
//! - 后端按家那一族落盘：`control/exit_policy` · `control/files_commit` · `control/resident` ·
//!   `assets/skill_ledger` · `assets/asset_catalog` · `platform/shell/dialect`（两份别名文件）·
//!   `accounts/manage`（账号库）；读账号库清单的还有 `observe/accounts_query` · `control/ccm`。

/// 〔两个端口〕**常驻监听口**的门牌也住这里（它与中转口是这台机器上后端的两个门）：
/// 这台机器 ＋ 这个家（`~/.cc-monitor`；隔离跑时 `CCM_DATA_DIR`）⇒ 那一个口。门牌只跟着家走：与 Claude 目录、
/// 与哪一家 agent 都无关（改一个设置、换一个终端起 monitor，都还是同一个口、同一个后端）。
/// 本机宿主（monitor `local_backend_host`）与后端 `--resident-ensure` / `--resident-stop` 同一个函数
/// ⇒ 一台机器一个常驻后端，本机 / 远端视角收敛。FNV-1a 写死（`DefaultHasher` 跨 Rust 版本不稳定，升级后要算出同一个口）。
pub fn listen_port_for(data_home: &std::path::Path) -> u16 {
    const PORT_BASE: u16 = 49152;
    const PORT_SPAN: u32 = 16384;
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in data_home.to_string_lossy().as_bytes() {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x1000_0000_01b3);
    }
    PORT_BASE + ((h % u64::from(PORT_SPAN)) as u16)
}

/// **后端的落点**（相对家目录）：那个文件就是后端二进制本身，名字叫 `ccm`；本机与远端同一个。
/// 远端的 `backendPath`（可填的格）删了，monitor 与后端往那台拼命令、推字节都只认这一处。
pub const BACKEND_LANDING_REL: &str = ".cc-monitor/bin/ccm";

/// 同一个落点在远端 POSIX shell 里的写法（远端今天只承诺 POSIX）：`$HOME` 在那台上展开，其余字节都是安全字符。
pub const BACKEND_LANDING_SHELL: &str = "\"$HOME\"/.cc-monitor/bin/ccm";

/// 常驻监听口的钥匙文件（相对家目录；0600，本机宿主与远端 `--resident-ensure` 同一份）。
pub const LISTEN_TOKEN_FILE_REL: &str = ".cc-monitor/listen-token";

/// 中转在回环上听的那个口。**本机**：monitor 起常驻后端时以 `CCM_RELAY_PORT` 交给它（它在进程里起中转）；
/// **远端**：`--resident-ensure` 起那台的常驻后端时交同一个值（本机远端同形）。
pub const PORT: u16 = 8788;

/// 中转钥匙文件相对家目录的路径（`INVARIANTS §48.1a`）：**中转所在那台机器**上 `0600`，中转绑上口之后自己读回或铸。
/// 注入的 URL 不带钥匙本身，渲染成 `$(cat ~/<本常量>)` 在那台机器的 pane shell 里展开（RK1）。
pub const KEY_FILE_REL: &str = ".cc-monitor/relay-key";

/// 只许直通（`/t/`）的那把钥匙的文件（同机同权限、同一个铸法）。地址只能经命令行参数交给的那一家只拿它（`INVARIANTS §48.1a`）。
pub const PASS_KEY_FILE_REL: &str = ".cc-monitor/relay-pass-key";

/// 〔「一台机器一个家」〕后端的**上传暂存区**（相对家目录）：SFTP 传输台只往这里写 `<key>.part`，
/// 传完由那台后端提交、挪进目标。后端按它落盘（`control/files_commit.rs::STAGING_DIR`），monitor 的数据位置页按它列出。
pub const STAGING_DIR_REL: &str = ".cc-monitor/staging";

/// 〔「家里的都进唯一枚举」〕这台机器上后端的**退出行为设置**（只有后端写，`control/exit_policy.rs`）。
pub const BACKEND_POLICY_REL: &str = ".cc-monitor/backend.json";

/// 〔同上〕别名块文件：POSIX shell 读的那一份（后端 `platform/shell/dialect.rs` 写，rc 里那一行 source 它）。
pub const POSIX_ALIASES_REL: &str = ".cc-monitor/aliases.sh";

/// 〔同上〕别名块文件：PowerShell 读的那一份。
pub const PS_ALIASES_REL: &str = ".cc-monitor/aliases.ps1";

/// 〔同上〕配置文件：一段一组 ccm 选项、可「基于」另一段（后端 `assets/aliases/profile.rs` 读写；用户也可以手改）。
pub const PROFILES_REL: &str = ".cc-monitor/profiles.toml";

/// 〔同上〕旧别名清单一次性转进配置文件之后那张说明（转了几条 · 转不进去的几条；设置窗页首说一次，「知道了」后删）。
pub const PROFILES_MIGRATED_REL: &str = ".cc-monitor/profiles-migrated.json";

/// 〔同上〕配置文件上次经 cc-monitor 写出去的那一份的指纹（后端读配置文件时拿它比 ⇒「之后有人手改过」；删了只是不再说那一句）。
pub const PROFILES_WRITTEN_REL: &str = ".cc-monitor/profiles-written.json";

/// 〔同上〕「要你动手」里记下的选择：点过「不用了」的那几件 · 选了「我自己贴」的那份启动文件（后端 `footprint/chores/marks.rs` 写）。
pub const CHORES_REL: &str = ".cc-monitor/chores.json";

/// 〔同上〕计划「要你看」的认可与「退回」的记录（后端 `plan/review.rs` 写；计划仓本身一个字节不写）。
pub const PLAN_REVIEW_REL: &str = ".cc-monitor/plan-review.json";

/// 〔同上〕离线那台的上次值：本机后端替界面记下每台最近一次读成的账号清单与「文件与数据」那一份（后端 `footprint/last_seen.rs` 写；连不上时照它画、跨重启还在）。
pub const LAST_SEEN_REL: &str = ".cc-monitor/last-seen.json";

/// 〔同上〕skill 装记录（后端 `assets/skill_ledger.rs` 写；卸的时候按它删）。
pub const SKILL_LEDGER_REL: &str = ".cc-monitor/skill-installs.json";

/// 〔同上〕资产目录（后端 `assets/asset_catalog.rs` 写；各台机器之间自动对上）。
pub const ASSET_CATALOG_REL: &str = ".cc-monitor/assets-catalog.json";

/// 从一台卸掉不是 cc-monitor 装的扩展之前，先挪（skill 目录）/ 抄（MCP 配置）到这里（后端 `assets/ext.rs` 经文件管理面写）。
pub const EXT_BACKUPS_DIR_REL: &str = ".cc-monitor/backups";

/// 这台账号库里各号共用的用户级 MCP（共享集合 ＋ 上次同步时各号的样子；后端 `accounts/manage/mcp_share_exec.rs` 写，0600）。
pub const ACCOUNTS_MCP_REL: &str = ".cc-monitor/accounts-mcp.json";

/// 〔「一台机器一个家」〕**额度账**（后端账号域写）：这台各号最近一次从回包头看到的额度快照 ＋ 看到的时刻。
pub const QUOTA_LEDGER_REL: &str = ".cc-monitor/quota.json";

/// 〔同上〕**账号轮换**（后端账号域写）：默认池与换号时机 · 每个会话的覆盖与此刻钉在哪个号 · 换号记录。
pub const ROTATION_REL: &str = ".cc-monitor/rotation.json";

/// 〔同上〕**起会话用的号**（后端观测侧写）：每条会话上次用哪个号起的（`sid → 号`），跟随选号读它。
pub const LAUNCH_ACCOUNTS_REL: &str = ".cc-monitor/launch-accounts.json";

/// 〔同上〕**主机钥匙**（后端拨号侧写，OpenSSH 格式）：握手认下的每台主机钥匙；开终端那一行交给 `ssh` 认它，不再问、不写用户的 `~/.ssh`。
pub const KNOWN_HOSTS_REL: &str = ".cc-monitor/known_hosts";

/// 〔同上〕**起会话便条**（`ccm` 最终那一跳写、观测侧认完即清）：一个进程一张 `<pid>.json`（号 ＋ 时刻）。
pub const LAUNCH_NOTES_DIR_REL: &str = ".cc-monitor/launch-pending";

/// 账号库目录（相对家目录）那一段字面量，**全仓只写在这里**。宏而不是常量：足迹那张静态表要
/// `concat!` 出带 `~/` 的那一形，`concat!` 只认字面量。别处一律用 [`ACCOUNTS_DIR_REL`]。
#[macro_export]
macro_rules! accounts_dir_rel {
    () => {
        ".cc-monitor/accounts"
    };
}

/// 〔「一台机器一个家」〕**账号库**（后端 `accounts/manage/` 建和维护）：清单 [`ACCOUNTS_MANIFEST_NAME`] ＋
/// 每个号一个配置目录 `<本目录>/<号>/`（就是那个号的 `CLAUDE_CONFIG_DIR`）。位置只跟着家走，没有另指位置的变量或选项。
pub const ACCOUNTS_DIR_REL: &str = accounts_dir_rel!();

/// 账号库清单在 [`ACCOUNTS_DIR_REL`] 下的文件名（后端写；ccm 起会话 · 账号查询 · 账号之间同步 MCP 都读它）。
pub const ACCOUNTS_MANIFEST_NAME: &str = "accounts.json";

/// 〔同上〕常驻监听口的进程记录的文件名（与 [`LISTEN_TOKEN_FILE_REL`] 同一个目录）：本机宿主与远端
/// `--resident-ensure` 按同一个口（[`listen_port_for`]）找同一份。
pub fn listen_pid_file_name(port: u16) -> String {
    format!("listen-{port}.pid")
}

/// 相对家目录的一段 ⇒ 最后一截（文件名）。给各写者定自己那个 `FILE_NAME`（临时件的名字从它拼），名字仍只住上面那一处。
pub const fn file_name_of(rel: &'static str) -> &'static str {
    let b = rel.as_bytes();
    let mut i = b.len();
    while i > 0 && b[i - 1] != b'/' {
        i -= 1;
    }
    let (_, tail) = b.split_at(i);
    match core::str::from_utf8(tail) {
        Ok(s) => s,
        Err(_) => rel,
    }
}

/// 钥匙的形状：恰好 64 个小写十六进制字符（32 字节 = 256 位，中转 `relay/door.rs` 铸的就是这一形）。
pub fn key_shape_ok(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// 一条**已经把钥匙段展开进去**的中转地址（`http://127.0.0.1:<口>/<钥匙>/<前缀>/…`，pane shell 展开
/// `$(cat ~/<KEY_FILE_REL>)` 之后 agent 进程环境里的那一形）切成「钥匙之前」「钥匙之后」两半：
/// `("http://127.0.0.1:<口>/", "/<前缀>/<seg1>/<seg2>")`。
///
/// 认的条件全在这里一处：钥匙段过 [`key_shape_ok`] · 两半拼回去（去掉钥匙段）过 [`base_url_shape_ok`]。
/// 认不出 ⇒ `None`（那就不是我们注入的地址，原样对待）。读者：观测侧判一条会话是不是经中转 ·
/// 上游选择判一条地址是不是我们那一形（`ccm` 因此不把外层别的号的中转地址带进这一发）。
pub fn split_keyed_base_url(url: &str) -> Option<(&str, &str)> {
    let after_scheme = url.strip_prefix("http://127.0.0.1:")?;
    let head_len = "http://127.0.0.1:".len() + after_scheme.find('/')? + 1;
    let (head, keyed) = url.split_at(head_len);
    let slash = keyed.find('/')?;
    let (key, tail) = keyed.split_at(slash);
    (key_shape_ok(key) && base_url_shape_ok(&format!("{head}{}", &tail[1..])))
        .then_some((head, tail))
}

/// [`split_keyed_base_url`] 的逆：把一把钥匙插进一条构造口产物（[`base_url`]）的口之后 ⇒
/// `http://127.0.0.1:<口>/<钥匙>/<前缀>/<seg1>/<seg2>`。地址过不了 [`base_url_shape_ok`]、或钥匙过不了 [`key_shape_ok`] ⇒ `None`。
/// 读者：给用户自己贴进 agent 设置文件的那一段（那里不能写 `$(cat …)`，只能写展开后的这一形）。
pub fn keyed_base_url(url: &str, key: &str) -> Option<String> {
    if !(base_url_shape_ok(url) && key_shape_ok(key)) {
        return None;
    }
    let after_scheme = url.strip_prefix("http://127.0.0.1:")?;
    let head_len = "http://127.0.0.1:".len() + after_scheme.find('/')? + 1;
    let (head, tail) = url.split_at(head_len);
    Some(format!("{head}{key}/{tail}"))
}

/// 路由路径第一段的两个前缀 = 两种模式（「为什么用两个前缀而不是一个哨兵段」）。
///
/// `/s/` 代入：上游选择的表里必须有这一行，没有 ⇒ 404；`/t/` 直通：中转**永不**代入凭据，第 2 段只当标签。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RouteMode {
    /// `/s/`
    Substitute,
    /// `/t/`
    Passthrough,
}

impl RouteMode {
    /// 闭集。**两个前缀的字面量只在 [`RouteMode::prefix`] 里。**
    pub const ALL: [RouteMode; 2] = [RouteMode::Substitute, RouteMode::Passthrough];

    /// 这个模式的前缀（带前后两个 `/`）。
    pub fn prefix(self) -> &'static str {
        match self {
            RouteMode::Substitute => "/s/",
            RouteMode::Passthrough => "/t/",
        }
    }
}

/// 一段路由里允许的字符 —— 白名单：ASCII 字母数字与 `-` `_`，1..=128 字节。
///
/// `.` 与 `/` 不在里面 ⇒ `..` 构造不出来；路由段要进 tee 行与日志，放开任意字节等于给换行 / 控制字符开一条路。
/// ⚠ 中转给流打标签的请求头值也过它（流标签不再是路由段）。
pub fn segment_is_safe(seg: &str) -> bool {
    !seg.is_empty()
        && seg.len() <= 128
        && seg
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// 拼 `/<前缀>/<seg1>/<seg2>`。**任一段过不了 [`segment_is_safe`] ⇒ `None`**（fail-closed：
/// 拼错一段的症状是中转回一个查不出来的 404，所以宁可当场拒）。
///
/// 没有第 3 段：会话 id 归 agent 自己，启动器不往地址里塞会话身份 ⇒ 这条地址**不随会话变**，
/// 中转从 agent 请求里自带的头认会话。
pub fn route_path(mode: RouteMode, seg1: &str, seg2: &str) -> Option<String> {
    (segment_is_safe(seg1) && segment_is_safe(seg2))
        .then(|| format!("{}{seg1}/{seg2}", mode.prefix()))
}

/// 注入给 agent 的 base URL：`http://127.0.0.1:<port>` ＋ [`route_path`]。**恒回环**（回环是自指的：
/// 同一个字面串写进哪台机器就指哪台）。
pub fn base_url(port: u16, mode: RouteMode, seg1: &str, seg2: &str) -> Option<String> {
    if port == 0 {
        return None;
    }
    route_path(mode, seg1, seg2).map(|p| format!("http://127.0.0.1:{port}{p}"))
}

/// [`base_url`] 的**逆**：`http://127.0.0.1:<1–65535>` ＋ 一个前缀 ＋ 恰好两段、每段过闸。别的一律 `false`
/// （`localhost` · `https` · 查询串 · 尾斜杠 · 少段多段 · 端口前导空）。
pub fn base_url_shape_ok(url: &str) -> bool {
    let Some(rest) = url.strip_prefix("http://127.0.0.1:") else {
        return false;
    };
    let Some(slash) = rest.find('/') else {
        return false;
    };
    let (port, path) = rest.split_at(slash);
    let port_ok = !port.is_empty()
        && port.bytes().all(|b| b.is_ascii_digit())
        && port.parse::<u16>().is_ok_and(|p| p != 0);
    let Some(segs) = RouteMode::ALL
        .iter()
        .find_map(|m| path.strip_prefix(m.prefix()))
    else {
        return false;
    };
    let parts: Vec<&str> = segs.split('/').collect();
    port_ok && parts.len() == 2 && parts.iter().all(|p| segment_is_safe(p))
}

/// 请求目标 `/<前缀>/<seg1>/<seg2>/<rest>` 切出来的样子（中转那一侧用）。
#[derive(Debug, PartialEq, Eq)]
pub struct Parsed<'a> {
    pub mode: RouteMode,
    pub seg1: &'a str,
    pub seg2: &'a str,
    /// 第 2 段之后的全部（**不带**开头那个 `/`），原样交上游。
    pub rest: &'a str,
}

/// 切请求目标。不是这个形状（前缀不认得 · 少段 · 某段过不了闸）⇒ `None`。
pub fn parse_target(target: &str) -> Option<Parsed<'_>> {
    let (mode, after) = RouteMode::ALL
        .iter()
        .find_map(|m| target.strip_prefix(m.prefix()).map(|r| (*m, r)))?;
    let (seg1, after) = after.split_once('/')?;
    let (seg2, rest) = after.split_once('/')?;
    (segment_is_safe(seg1) && segment_is_safe(seg2)).then_some(Parsed {
        mode,
        seg1,
        seg2,
        rest,
    })
}

#[cfg(test)]
#[path = "../../../../tests/common/relay-route-core/lib_tests.rs"]
mod tests;

/// 〔「家里的都进这一份」〕后端在这台自己家里放的每一样：`(名字, 相对家目录, 是目录, 删了会丢)`。名字是闭集（界面按它取说法）；
/// 后端落点由它所在的 `bin` 那一行代表；只住 monitor 那一侧的（监听口进程记录 · API key 表 · 后端错误输出）不在这里。
/// 后端「文件与数据」那一份成品逐样 stat 它（`footprint/data.rs::own_rows`）。
pub const OWN_HOME_ENTRIES: &[(&str, &str, bool, bool)] = &[
    ("bin", ".cc-monitor/bin", true, false),
    ("staging", STAGING_DIR_REL, true, false),
    ("relayKey", KEY_FILE_REL, false, true),
    ("relayPassKey", PASS_KEY_FILE_REL, false, true),
    ("listenToken", LISTEN_TOKEN_FILE_REL, false, true),
    ("policy", BACKEND_POLICY_REL, false, true),
    ("profiles", PROFILES_REL, false, true),
    ("profilesMigrated", PROFILES_MIGRATED_REL, false, true),
    ("profilesWritten", PROFILES_WRITTEN_REL, false, false),
    ("aliasesPosix", POSIX_ALIASES_REL, false, true),
    ("aliasesPs", PS_ALIASES_REL, false, true),
    ("skillLedger", SKILL_LEDGER_REL, false, true),
    ("chores", CHORES_REL, false, true),
    ("planReview", PLAN_REVIEW_REL, false, true),
    ("lastSeen", LAST_SEEN_REL, false, false),
    ("assetCatalog", ASSET_CATALOG_REL, false, false),
    ("quota", QUOTA_LEDGER_REL, false, true),
    ("rotation", ROTATION_REL, false, true),
    ("launchAccounts", LAUNCH_ACCOUNTS_REL, false, true),
    ("launchNotes", LAUNCH_NOTES_DIR_REL, true, false),
    ("knownHosts", KNOWN_HOSTS_REL, false, false),
    ("accounts", ACCOUNTS_DIR_REL, true, true),
    ("accountsMcp", ACCOUNTS_MCP_REL, false, true),
    ("extBackups", EXT_BACKUPS_DIR_REL, true, true),
];
