//! **后端的门牌与家的契约**：monitor 与后端两边必须对上的那几样 —— 这台机器上后端的落点、常驻监听口与中转口、
//! 中转路由路径的语法，以及后端住在 `~/.cc-monitor/` 下的每一样东西的相对路径。
//!
//! # 它今天管什么
//!
//! - **门牌**：中转口 [`PORT`] · 常驻后端的套接字 [`listen_socket_for`] · 中转钥匙 [`KEY_FILE_REL`] ·
//!   路由路径的语法（两个前缀 · 段闸 · 拼 · 拆）。
//! - **家**（相对家目录，`.cc-monitor/` 开头的每一个常量）：后端落点 [`BACKEND_LANDING_REL`] · 暂存区 ·
//!   退出行为设置 · 两份别名文件 · skill 装记录 · 资产目录 · 账号库 [`ACCOUNTS_DIR_REL`] 与它的清单。
//!   后端各写者引它们落盘；后端「文件与数据」那一份成品（`footprint/data.rs::own_rows`，经 [`OWN_HOME_ENTRIES`]）按它们列出，界面照抄；判据两向对着这一族。
//! - **声明表**：每份落盘文件一个 [`Decl`] 常量（`*_FILE`：落点 · 主人类 ①–⑤〔⑤ 即缓存〕· 谁写 · 格式与版本 · 读上限），
//!   客户端那一侧的落点（手机 Room · 界面存储）登在 `CLIENT_SPOTS`（同 `OWN_FILES` 只有判据读）；后端读写口
//!   `common/own_state.rs` 的 `Doc::of` 与 monitor 读写 `config.json`（[`CONFIG_FILE`]）从它取；家里那几样有声明的，「删了会丢」照声明。排成一张的 `OWN_FILES` 今天只有判据读（测试档）。
//! - **读上限**（[`Cap`] · [`Over`]）与**同步整读**：读到流尽 [`read_capped_from`] · 读一个窗口 [`read_window`] ·
//!   读一份本机文件 [`read_capped_regular`]。后端 · 壳 · 文件窗口 · 手机核心同步地整读字节只经这一族（异步那一形住 `comms_inward::line`）。
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
//! 才有名字。⇒ 通信层成员 `outward/route.rs` 可以 `use` 它（`C2` 禁的是业务 crate）。
//!
//! # 谁用哪几样
//!
//! - 中转：`outward/route.rs::parse`（[`parse_target`] · [`segment_is_safe`]）· 门 `outward/door.rs`（钥匙的形状 [`key_shape_ok`]）。
//! - 后端：钥匙 `relay/key.rs`（落点 [`KEY_FILE_REL`] · 带钥匙的地址 [`keyed_base_url`] · 中转地址的 fail-closed 校验 [`base_url_shape_ok`]）·
//!   上游选择 `accounts/upstream_select/endpoint.rs`（[`base_url`]：起会话那一发注入哪个地址，**只有它拼**；[`base_url_shape_ok`] 是它的逆）·
//!   起会话 `control/ccm/plan.rs`（钥匙段渲成 `$(cat ~/<钥匙>)` 的 [`KEY_FILE_REL`] · 起会话身份 token 的字符集 [`segment_is_safe`]）·
//!   中转在不在听（[`PORT`]，`faces/read_face.rs` · `faces/launch_account_face.rs`）。
//! - monitor：起本机后端时交的端口（[`PORT`]）· 常驻后端的套接字（[`listen_socket_for`]）· 后端落点（[`BACKEND_LANDING_REL`]）。
//!   家那一族由后端列（[`OWN_HOME_ENTRIES`]），monitor 不另列。
//! - 后端按家那一族落盘：`control/exit_policy` · `files/commit` · `control/resident` ·
//!   `assets/skill_ledger` · `assets/asset_catalog` · `platform/shell/dialect`（两份别名文件）·
//!   `accounts/manage`（账号库）；读账号库清单的还有 `observe/accounts_query` · `control/ccm`。

/// 常驻后端听的那个 Unix 套接字所在的目录（相对家目录）：只给本人（`0700`）。常驻后端活着时攥着这个目录的独占锁
/// （抢不到 = 已有一个在听），套接字住这里。门由内核给：目录权限 ＋ 对端 uid，没有钥匙。
pub const LISTEN_DIR_REL: &str = ".cc-monitor/run";

/// 套接字的文件名（在 [`LISTEN_DIR_REL`] 里）。
pub const LISTEN_SOCKET_NAME: &str = "backend.sock";

/// 这个家（`~/.cc-monitor`；隔离跑时 `CCM_DATA_DIR`）⇒ 常驻后端那个目录。门牌只跟着家走：与 Claude 目录、与哪一家 agent 都无关。
/// 本机宿主（monitor `local_backend_host`）与后端（常驻后端 · `--resident-ensure` / `--resident-attach` / `--resident-stop`）同一个函数
/// ⇒ 一台机器一个家一个常驻后端。
pub fn listen_dir_for(data_home: &std::path::Path) -> std::path::PathBuf {
    data_home.join(file_name_of(LISTEN_DIR_REL))
}

/// 同上 ⇒ 那个套接字。
pub fn listen_socket_for(data_home: &std::path::Path) -> std::path::PathBuf {
    listen_dir_for(data_home).join(LISTEN_SOCKET_NAME)
}

/// 「这是一次沙箱跑」的标记（值 `1`）：测试 / 台架起后端时在白名单环境里带上。带着它的常驻后端若要占的家落在本账号真家目录的
/// `.cc-monitor` 下 ⇒ 拒绝起（后端 `control/resident.rs::sandbox_refusal`）。仓里起沙箱后端的每个入口都带它（判据钉）。
pub const SANDBOX_ENV: &str = "CCM_SANDBOX";

/// **后端的落点**（相对家目录）：那个文件就是后端二进制本身，名字叫 `ccm`；本机与远端同一个。
/// 远端的 `backendPath`（可填的格）删了，monitor 与后端往那台拼命令、推字节都只认这一处。
pub const BACKEND_LANDING_REL: &str = ".cc-monitor/bin/ccm";

/// 换版本时被盖掉的那几份落点住的那一层的名字：在落点那层 `bin` 底下（[`BACKEND_LANDING_REL`] 的父目录；同一块盘），
/// 里面一份一个 `ccm-<那一份自报的身份>`。后端 `control/place_self.rs` 在盖落点那一下之前留，有上限、过期删；
/// 数据页同落点一样由 `bin` 那一行代表。
pub const BACKEND_KEPT_DIR_NAME: &str = "kept";

/// 同一个落点在远端 POSIX shell 里的写法（远端今天只承诺 POSIX）：`$HOME` 在那台上展开，其余字节都是安全字符。
pub const BACKEND_LANDING_SHELL: &str = "\"$HOME\"/.cc-monitor/bin/ccm";

/// 家目录底下一条只含安全字符的相对路径在远端 POSIX shell 里的写法（同 [`BACKEND_LANDING_SHELL`]）：部署放进暂存区的那一份用它叫。
pub fn home_shell(rel: &str) -> String {
    format!("\"$HOME\"/{rel}")
}

/// 中转在回环上听的那个口。**本机**：monitor 起常驻后端时以 `CCM_RELAY_PORT` 交给它（它在进程里起中转）；
/// **远端**：`--resident-ensure` 起那台的常驻后端时交同一个值（本机远端同形）。
pub const PORT: u16 = 8788;

/// 中转钥匙文件相对家目录的路径（`INVARIANTS §48.1a`）：**中转所在那台机器**上 `0600`，中转绑上口之后自己读回或铸。
/// 注入的 URL 不带钥匙本身，渲染成 `$(cat ~/<本常量>)` 在那台机器的 pane shell 里展开（RK1）。
pub const KEY_FILE_REL: &str = ".cc-monitor/relay-key";

/// 〔「一台机器一个家」〕后端的**上传暂存区**（相对家目录）：SFTP 传输台只往这里写 `<key>.part`，
/// 传完由那台后端提交、挪进目标。后端按它落盘（`files/commit.rs::STAGING_DIR`），monitor 的数据位置页按它列出。
pub const STAGING_DIR_REL: &str = ".cc-monitor/staging";

/// monitor 的设置（monitor `config.rs::patch_config_at` 是唯一写者）。
/// 住 monitor 的数据目录（默认就是 `~/.cc-monitor/`），不在家里那几样（[`OWN_HOME_ENTRIES`]）里。
pub const CONFIG_REL: &str = ".cc-monitor/config.json";

/// 〔「家里的都进唯一枚举」〕这台机器上后端的**退出行为设置**（只有后端写，`control/exit_policy.rs`）。
pub const BACKEND_POLICY_REL: &str = ".cc-monitor/backend.json";

/// 〔同上〕这台的偏好：每一家的启动命令 · 恢复进不进 tmux · 每个号的模型 · 上下文上限表（只有后端写，`control/machine_prefs.rs`）。
pub const PREFS_REL: &str = ".cc-monitor/prefs.json";

/// 〔同上〕这一端连各台用的机器表：每台的名字 · 地址 · 端口 · 用户 · 钥匙路径 · 备用地址 · 跳板 · 连不连
/// （只有本机后端写，`control/machines.rs`；桌面的 monitor 只经 `machines-read` 读）。
pub const MACHINES_REL: &str = ".cc-monitor/machines.json";

/// 〔同上〕别名块文件：POSIX shell 读的那一份（后端 `platform/shell/dialect.rs` 写，rc 里那一行 source 它）。
pub const POSIX_ALIASES_REL: &str = ".cc-monitor/aliases.sh";

/// 〔同上〕别名块文件：PowerShell 读的那一份。
pub const PS_ALIASES_REL: &str = ".cc-monitor/aliases.ps1";

/// 〔同上〕配置文件：一段一组 ccm 选项、可「基于」另一段（后端 `assets/aliases/profile.rs` 读写；用户也可以手改）。
pub const PROFILES_REL: &str = ".cc-monitor/profiles.toml";

/// 〔同上〕配置文件上次经 cc-monitor 写出去的那一份的指纹（后端读配置文件时拿它比 ⇒「之后有人手改过」；删了只是不再说那一句）。
pub const PROFILES_WRITTEN_REL: &str = ".cc-monitor/profiles-written.json";

/// 〔同上〕「待办」里记下的选择：点过「不用了」的那几件 · 选了「我自己贴」的那份启动文件（后端 `footprint/chores/marks.rs` 写）。
pub const CHORES_REL: &str = ".cc-monitor/chores.json";

/// 〔同上〕计划「要你看」的认可与「退回」的记录（后端 `plan/review.rs` 写；计划仓本身一个字节不写）。
pub const PLAN_REVIEW_REL: &str = ".cc-monitor/plan-review.json";

/// 〔同上〕离线那台的上次值：本机后端替界面记下每台最近一次读成的账号清单与「文件与数据」那一份（后端 `footprint/last_seen.rs` 写；断开时照它画、跨重启还在）。
pub const LAST_SEEN_REL: &str = ".cc-monitor/cache/last-seen.json";

/// 〔同上〕这台的书签：文件窗口与手机收藏的这台上的目录（后端 `control/bookmarks.rs` 写；`bookmarks-read` / `-edit` 读写，本机与远端同形）。
pub const BOOKMARKS_REL: &str = ".cc-monitor/bookmarks.json";

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

/// 〔同上〕**账号轮换**（后端账号域写）：每个会话此刻在用哪个号 · 它载入的那份策略副本 · 拖出来的先后 · 策略交的 memo 与叫醒 ·
/// 换号记录 · 新会话默认用哪份策略（`forNew`）。
pub const ROTATION_REL: &str = ".cc-monitor/rotation.json";

/// 〔同上〕**会话血缘**（后端 `lineage.rs` 写，唯一写者是中转那一路）：谁起的谁 ＋ 起会话地址里每个来处绑给了哪个会话。
pub const LINEAGE_REL: &str = ".cc-monitor/lineage.json";

/// 〔同上〕**起会话用的号**（后端观测侧写）：每条会话上次用哪个号起的（`sid → 号`），跟随选号读它。
pub const LAUNCH_ACCOUNTS_REL: &str = ".cc-monitor/launch-accounts.json";

/// 〔同上〕**会话输入队**（后端 `history/session_queue.rs` 写）：一条会话一份 `<sid>.json`，还没送进会话的那几句与序号。
pub const SESSION_QUEUE_DIR_REL: &str = ".cc-monitor/session-queue";

/// 〔同上〕**cc-bus 的收件箱与状态**（后端 `control/cc_bus/store.rs` 写）：一条会话一份收件箱 `<sid>.jsonl` ＋ 一份 `state.json`（已读位置 · 发信规矩的账）。
pub const CC_BUS_DIR_REL: &str = ".cc-monitor/cc-bus";

/// 〔同上〕**主机钥匙**（后端拨号侧写，OpenSSH 格式）：拨号认它、鉴权过了的首见钥匙记进它；开终端那一行经 [`SSH_CONFIG_REL`] 交给 `ssh`。
pub const KNOWN_HOSTS_REL: &str = ".cc-monitor/known_hosts";

/// 〔同上〕**开终端那一行的 ssh 配置**（后端拨号侧写，`ssh -F` 用）：指到 [`KNOWN_HOSTS_REL`]，再照读用户与系统的 ssh 配置。
pub const SSH_CONFIG_REL: &str = ".cc-monitor/ssh_config";

/// 〔同上〕**起会话便条**（`ccm` 最终那一跳写、观测侧认完即清）：一个进程一张 `<pid>.json`（号 ＋ 时刻）。
pub const LAUNCH_NOTES_DIR_REL: &str = ".cc-monitor/launch-pending";
/// 大记录的骨架（后端冷开扫描图在数据目录底下落的缓存，实时跟读起步也照它；删了照记录重建；`observe/skeleton.rs`）。
pub const SKELETON_DIR_REL: &str = ".cc-monitor/cache/skeleton";
/// 历史清单缓存（后端历史清单在数据目录底下落的一份：每份记录读到哪了、标题与条数那几格；重启照它接着认，删了照记录重建；`history/listing.rs`）。
pub const LISTING_CACHE_REL: &str = ".cc-monitor/cache/listing.json";
/// 远端那几台的会话清单副本（本机后端在数据目录底下落的缓存：一台一份 `<机器>.json`，重启后头一问带它的记号；删了照那台再问一次整份；`history/remote_listing.rs`）。
pub const REMOTE_LISTING_DIR_REL: &str = ".cc-monitor/cache/remote-listing";
/// 补足会话为各组名建的那一组（组名 ⇒ 组 id；`control/session_set.rs`，声明 [`SET_GROUPS_FILE`]）。
pub const SET_GROUPS_REL: &str = ".cc-monitor/set-groups.json";
/// 组的待补：改名 · 解散没送到别台的那几句（`history/groups_owed.rs`，声明 [`GROUPS_OWED_FILE`]）。
pub const GROUPS_OWED_REL: &str = ".cc-monitor/groups-owed.json";
/// 轮换策略的源码（一份一个 `.rhai`，用户自己的；声明 [`STRATEGY_SOURCE_FILE`]）。
pub const STRATEGIES_DIR_REL: &str = ".cc-monitor/strategies";
/// 托管终端那一层：每个一个目录（事实 · 连接入口），按版本的程序副本在 `.bin/` 底下（声明 [`HOST_FACTS_FILE`] · [`HOST_PROGRAM_FILE`]）。
pub const HOSTS_DIR_REL: &str = ".cc-monitor/hosts";

/// 〔同上〕**答题插件**（`ccm` 换了启动器起会话时写、经 `CLAUDE_CODE_PLUGIN_DIRS` 交给那一家）：清单 ＋ 答题那一组 hook，内容由后端生成、随时可重写。
pub const ANSWER_PLUGIN_DIR_REL: &str = ".cc-monitor/answer-plugin";

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

/// 钥匙的形状：恰好 64 个小写十六进制字符（32 字节 = 256 位，后端 `relay/key.rs` 铸的就是这一形，中转的门 `outward/door.rs` 照它认）。
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

/// 一行里每一条插好钥匙的我们那一形地址（[`split_keyed_base_url`] 认得的），钥匙段换成 `…`；其余逐字不变。
/// 读者：「待办」里从用户文件原样摘出的行（贴过的老地址里那把钥匙不出后端）。
pub fn hide_key(line: &str) -> String {
    const HEAD: &str = "http://127.0.0.1:";
    let url_byte =
        |b: u8| b.is_ascii_alphanumeric() || matches!(b, b':' | b'/' | b'-' | b'_' | b'~');
    let mut out = String::with_capacity(line.len());
    let mut rest = line;
    while let Some(at) = rest.find(HEAD) {
        let (before, from) = rest.split_at(at);
        out.push_str(before);
        let len = HEAD.len()
            + from
                .bytes()
                .skip(HEAD.len())
                .position(|b| !url_byte(b))
                .unwrap_or(from.len() - HEAD.len());
        let (url, after) = from.split_at(len);
        match split_keyed_base_url(url) {
            Some((head, tail)) => {
                out.push_str(head);
                out.push('…');
                out.push_str(tail);
            }
            None => out.push_str(url),
        }
        rest = after;
    }
    out.push_str(rest);
    out
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
/// 没有第 3 段：会话 id 归 agent 自己，启动器不往地址里塞会话身份，中转从 agent 请求里自带的头认会话。
/// 起会话那一跳另在尾上拼一段来处（[`with_origin`]，会话血缘）：那是一次起会话的号，不是会话 id，也不进路由键。
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

/// [`base_url`] 的**逆**：`http://127.0.0.1:<1–65535>` ＋ 一个前缀 ＋ 恰好两段、每段过闸，后面可以再跟一段来处
/// （[`with_origin`] 的产物）。别的一律 `false`（`localhost` · `https` · 查询串 · 尾斜杠 · 少段多段 · 端口前导空）。
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
    let route_ok = match parts.as_slice() {
        [a, b] => segment_is_safe(a) && segment_is_safe(b),
        [a, b, o] => segment_is_safe(a) && segment_is_safe(b) && parse_origin(o).is_some(),
        _ => false,
    };
    port_ok && route_ok
}

/// 〔会话血缘〕来处段打头的那个字符。不在段闸字符里 ⇒ 来处段与路由段、与 agent 自己拼上的真路径（`v1/…`）都分得开。
pub const ORIGIN_MARK: char = '~';

/// 来处段 `~<来处>[~<父>]` 切出来的样子。
///
/// - `token`：起会话那一趟（`ccm`）现铸的一个号，一次起会话一个。中转第一次看见它就把它绑给发这一发的那个会话；
///   之后别的会话拿着同一条地址来（在那个会话里直接起的进程原样继承这条地址）⇒ 它们的父就是绑的那一个。
/// - `parent`：起会话那一趟被一个会话的 shell 调用时，那个会话的编号。
///
/// 两格都只是形状，谁绑谁、记在哪由后端那一处管（`lineage.rs`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Origin<'a> {
    pub token: &'a str,
    pub parent: Option<&'a str>,
}

/// 一段是不是来处段、是的话切开。`~` 打头，后面一截或两截（`~` 隔开），每截过段闸。
pub fn parse_origin(seg: &str) -> Option<Origin<'_>> {
    let body = seg.strip_prefix(ORIGIN_MARK)?;
    let mut it = body.split(ORIGIN_MARK);
    let token = it.next().filter(|t| segment_is_safe(t))?;
    let parent = match it.next() {
        None => None,
        Some(p) if segment_is_safe(p) => Some(p),
        Some(_) => return None,
    };
    it.next().is_none().then_some(Origin { token, parent })
}

/// 给一条构造口产物（[`base_url`]，还没带来处）拼上来处段：`<地址>/~<来处>[~<父>]`。
/// 地址不是那一形 / 已经带了来处 / 任一截过不了段闸 ⇒ `None`。读者：`ccm` 起会话最终那一跳。
pub fn with_origin(url: &str, token: &str, parent: Option<&str>) -> Option<String> {
    if !base_url_shape_ok(url)
        || url
            .rsplit('/')
            .next()
            .is_some_and(|l| l.starts_with(ORIGIN_MARK))
    {
        return None;
    }
    let seg = match parent {
        Some(p) => format!("{ORIGIN_MARK}{token}{ORIGIN_MARK}{p}"),
        None => format!("{ORIGIN_MARK}{token}"),
    };
    parse_origin(&seg)?;
    Some(format!("{url}/{seg}"))
}

/// 请求目标 `/<前缀>/<seg1>/<seg2>/<rest>` 切出来的样子（中转那一侧用）。
#[derive(Debug, PartialEq, Eq)]
pub struct Parsed<'a> {
    pub mode: RouteMode,
    pub seg1: &'a str,
    pub seg2: &'a str,
    /// 紧跟第 2 段的来处段（[`parse_origin`]）；没有 ⇒ `None`。
    pub origin: Option<Origin<'a>>,
    /// 第 2 段（有来处段时是来处段）之后的全部（**不带**开头那个 `/`），原样交上游。
    pub rest: &'a str,
}

/// 切请求目标。不是这个形状（前缀不认得 · 少段 · 某段过不了闸 · 第 2 段之后是 `~` 打头却不是合法来处段）⇒ `None`。
pub fn parse_target(target: &str) -> Option<Parsed<'_>> {
    let (mode, after) = RouteMode::ALL
        .iter()
        .find_map(|m| target.strip_prefix(m.prefix()).map(|r| (*m, r)))?;
    let (seg1, after) = after.split_once('/')?;
    let (seg2, rest) = after.split_once('/')?;
    if !(segment_is_safe(seg1) && segment_is_safe(seg2)) {
        return None;
    }
    let (origin, rest) = if rest.starts_with(ORIGIN_MARK) {
        let (seg, rest) = rest.split_once('/')?;
        (Some(parse_origin(seg)?), rest)
    } else {
        (None, rest)
    };
    Some(Parsed {
        mode,
        seg1,
        seg2,
        origin,
        rest,
    })
}

#[cfg(test)]
#[path = "../../../../tests/common/relay-route-core/lib_tests.rs"]
mod tests;

/// 一份落盘文件丢了怎么办：状态（丢了就没了）读不出来 ⇒ 不覆盖、照实说；缓存（照别的东西能重建）读不出来 ⇒ 当没有。
/// 由主人类推出（[`Owner::kind`]）：⑤ 是缓存，别的是状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    State,
    Cache,
}

/// 存储层那五类（ARCHITECTURE「存储层」）：一样东西说的是谁，主人就是谁，住在主人那里。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Owner {
    /// ① 一条会话的事实（住会话那一台）。
    Session,
    /// ② 一台机器的事实 · 那台上的一个路径（住那一台）。
    Machine,
    /// ③ 几台机器上的东西凑成的一组（每台存自己那一片）。
    Set,
    /// ④ 这一端自己的（屏怎么摆 · 手上的东西 · 机器表 · 欠别台的话）。
    End(End),
    /// ⑤ 别人那里东西的副本（读不懂 · 版本对不上就当没有，不迁移）。
    Cache,
}

impl Owner {
    /// 丢了怎么办：⑤ 是缓存，别的是状态。
    pub const fn kind(self) -> Kind {
        match self {
            Owner::Cache => Kind::Cache,
            _ => Kind::State,
        }
    }
}

/// ④ 是哪一端的：桌面那一端 ＝ 壳 ＋ 界面 ＋ 本机后端（本机后端替它存机器表 · 主机指纹 · 欠别台的话）；手机那一端 ＝ 手机核心。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum End {
    Desktop,
    Phone,
}

/// 落在哪：本机后端（家目录 `.cc-monitor/` 底下，后端写）· 壳（monitor 写）· 界面（网页存储）· 手机（Room · 核心那几份文件）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Place {
    Backend,
    Shell,
    Ui,
    Phone,
}

/// 一处落点：在哪 · 叫什么（后端那几份是相对家目录的路径；客户端那几处 `room:<表名>` · `storage:<键>` · `phone-file:<名字>`）·
/// 主人类 · 等迁的旧行在哪一版删（`X.Y.Z`；不是等迁的 ⇒ 没有）。同 [`OWN_FILES`] 只有判据读（测试档）。
#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Spot {
    pub place: Place,
    pub at: &'static str,
    pub owner: Owner,
    pub until: Option<&'static str>,
}

/// 手机 Room 的一张表（`room:<表名>`）。
#[cfg(test)]
const ROOM_SPOT: &str = "room:";
/// 界面网页存储的一个键（`storage:<键>`；带参数的键写固定的前半）。
#[cfg(test)]
const STORAGE_SPOT: &str = "storage:";

#[cfg(test)]
const fn room(table: &'static str, owner: Owner) -> Spot {
    Spot {
        place: Place::Phone,
        at: table,
        owner,
        until: None,
    }
}

#[cfg(test)]
const fn ui(key: &'static str) -> Spot {
    Spot {
        place: Place::Ui,
        at: key,
        owner: Owner::End(End::Desktop),
        until: None,
    }
}

#[cfg(test)]
const PHONE: Owner = Owner::End(End::Phone);

/// 〔「客户端的落点也登一行」〕不是后端家里文件的那几处：手机 Room 每张表 · 手机核心的主机指纹 · 界面存储的每个键。
/// 判据两向核：Room 的 `@Entity` 表名 · 界面 `LS_KEYS` 的键与这里对得上；主人类照存储层那五类。
#[cfg(test)]
const CLIENT_SPOTS: &[Spot] = &[
    room("room:hosts", PHONE),
    room("room:identities", PHONE),
    room("room:known_hosts", PHONE),
    room("room:key_groups", PHONE),
    room("room:term_keys", PHONE),
    room("room:settings", PHONE),
    room("room:pending", PHONE),
    room("room:pending_batches", PHONE),
    room("room:drafts", PHONE),
    room("room:sessions", Owner::Cache),
    room("room:list_holds", Owner::Cache),
    room("room:record_pages", Owner::Cache),
    room("room:sync_anchors", Owner::Cache),
    // 书签已归那台（②）：旧表那几行等着迁过去，到那一版删表。
    Spot {
        until: Some("5.0.4"),
        ..room("room:bookmarks", Owner::Machine)
    },
    Spot {
        place: Place::Phone,
        at: "phone-file:known_hosts",
        owner: PHONE,
        until: None,
    },
    ui("storage:cc-monitor.bottom-drawer"),
    ui("storage:cc-monitor.history.expanded-forks"),
    ui("storage:cc-monitor.tool-render."),
    ui("storage:cc-monitor.last-active-sid"),
    ui("storage:cc-monitor.history.prefs"),
    ui("storage:cc-monitor.plan.prefs"),
    ui("storage:cc-monitor.cmdk-hint.seen"),
    ui("storage:cc-monitor.settings.restart-reasons"),
    ui("storage:cc-monitor.tab-bar-w"),
    ui("storage:cc-monitor.stream.process-expanded"),
    ui("storage:cc-monitor.stream.show-injected"),
    ui("storage:cc-monitor.main.zoom"),
    ui("storage:cc-monitor.tab-bar.folded"),
    ui("storage:cc-monitor.queue.pending"),
    ui("storage:cc-monitor.said-edit.drafts"),
    ui("storage:cc-monitor.composer.drafts"),
];

/// 一份落盘文件的格式：我们的 JSON（顶层整数 `version`，缺了当 1；读得懂 `floor..=version`，低于 `version` 的经升级步）·
/// 不带版本（`Foreign`：不写版本、不查版本 —— 不是我们的格式，如 OpenSSH 的主机钥匙；或定了不加版本的，如 monitor 的设置）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Json { version: u32, floor: u32 },
    Foreign,
}

/// 一道字节上限（读一份 · 收一帧 · 攒一截 · 留一份在内存里）：多少字节，超了怎么办。数和处置写在一处，声明的地方就说清越界那一刻发生什么。
/// 量的不是字节的上限（条数 · 字符数 · 毫秒）不用它。
///
/// 没有「静默截断」那一档：截了不说，拿半份当整份用，正是上限要挡的。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cap {
    pub bytes: u64,
    pub over: Over,
}

impl Cap {
    /// 上限的字节数作 `usize`，给拿长度（`len()`）比的调用处用；大过 `usize` 的（32 位上）当 `usize::MAX`。
    pub const fn as_len(self) -> usize {
        if self.bytes > usize::MAX as u64 {
            usize::MAX
        } else {
            self.bytes as usize
        }
    }
}

/// 超了上限怎么办。每一档都让人知道越界了；分界写在各档上。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Over {
    /// 硬报错：这一趟整个失败，错误照实往上交。
    HardError,
    /// 截断＋说清：只留上限以内的那一段，结果里带着「截了」那一格，读的人看得见。
    TruncateAndSay,
    /// 拒收＋回错：一个字节不收、不落、不覆盖，回一句说清超了。自有状态读不出来也是这一档：读面照实回错，写面不覆盖。
    Reject,
    /// 索引截断（不丢数据）：截掉的是可搜性，不是数据 —— 原文一个字节没动，只是超过封顶的那段搜不到。
    /// 与「静默截断」的分界：有没有丢掉用户的东西。
    IndexTrim,
    /// 跳过＋说清：超了的那一份跳过（缓存当没有、照别的东西重建），`warn!` 带上是哪一份。
    /// 跳过本身不是问题，跳过而不说是谁才是：没有 `warn!` 的跳过不许落在这一档。
    SkipAndSay,
    /// 降级＋说清：整份结果降级返回，错误带在结果里出去（不是跳过一条，降的是整份），看得见。
    DegradeAndSay,
    /// 丢弃＋带身份报告：丢掉越界的那一条，往用户那一侧发一个带身份的类型化事件（原位 `Gap`）。
    /// 与「跳过＋说清」的分界是谁被告知：那一档告日志，这一档告用户。跑在没有调用方可回错的后台流里用它。
    DropAndReport,
    /// 分块（不丢数据）：装不进一份的不拒、不截、不丢，改走分块逐块送、读回拼起来 —— 用户的字节一个不少。
    /// 与「拒收＋回错」的分界：越界的那一份照样存成了。
    Chunk,
}

/// 一次带上限整读的结果（同步 [`read_capped_from`] 与 `comms_inward::line` 的异步那一形同一个类型）。
#[derive(Debug, PartialEq, Eq)]
pub enum CappedAll {
    /// 流尽，一共不超过 `cap.bytes` 字节。
    All(Vec<u8>),
    /// 超过 `cap.bytes`：读到第 `cap.bytes + 1` 个字节就停，读进来的整份丢掉。
    OverCap,
}

/// 同步地读到流尽，至多 `cap.bytes` 字节；多读一个字节分得出「正好到上限」与「超了」。超了回 [`CappedAll::OverCap`]、不截断，
/// 超了之后怎么办是调用方照 `cap.over` 办的事。上限在读的时候生效：超了就停，不按流（或文件）的长度分配。
/// 要接着把流读空（管子另一头不读完会卡在写上）⇒ 交 `&mut r`，之后自己读进 `std::io::sink()`。
pub fn read_capped_from<R: std::io::Read>(rd: R, cap: Cap) -> std::io::Result<CappedAll> {
    let out = fill(rd, cap.bytes.saturating_add(1))?;
    Ok(if out.len() as u64 > cap.bytes {
        CappedAll::OverCap
    } else {
        CappedAll::All(out)
    })
}

/// 读一个窗口：至多 `n` 字节，读到 `n` 就停（后面的留在流里），流尽得早就交到流尽为止。不判超：窗口的宽就是调用方要的那一段
/// （读一块 · 读一页 · 读尾巴），后面还有字节是常态，不是越界。
pub fn read_window<R: std::io::Read>(rd: R, n: u64) -> std::io::Result<Vec<u8>> {
    fill(rd, n)
}

/// 同步整读的读循环（只这一份）：读到流尽或满 `limit` 字节为止。只用一次次的 `read`，被信号打断就再读。
fn fill<R: std::io::Read>(mut rd: R, limit: u64) -> std::io::Result<Vec<u8>> {
    let mut out = Vec::new();
    let mut chunk = [0u8; 8 * 1024];
    loop {
        let room = limit.saturating_sub(out.len() as u64);
        if room == 0 {
            return Ok(out);
        }
        let want = chunk.len().min(usize::try_from(room).unwrap_or(usize::MAX));
        let n = match rd.read(&mut chunk[..want]) {
            Ok(n) => n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        };
        if n == 0 {
            return Ok(out);
        }
        out.extend_from_slice(&chunk[..n]);
    }
}

/// 带上限整读一份本机文件的结局（[`read_capped_regular`]）。
#[derive(Debug, PartialEq, Eq)]
pub enum Regular {
    /// 整份（不超过上限）。
    Whole(Vec<u8>),
    /// 不是普通文件（目录 · FIFO · 设备 · socket；不跟链接那一形下还有链接本身）：没打开。
    NotRegular,
    /// 超过上限：打开之前看长度就超了 ⇒ 带上实际大小（没打开）；读的途中长过上限 ⇒ 不带。
    OverCap(Option<u64>),
}

/// 带上限整读一份本机文件：先看是不是普通文件（打开一个 FIFO 会卡住等写端，设备的长度报 0 会骗过大小检查）· 再看长度
/// （拒一份巨型文件之前不先读 `cap.bytes + 1` 进内存）· 再打开、经 [`read_capped_from`] 读（看长度与读之间文件还会长，两道一起才完整）。
/// 跟不跟链接由调用方定：`meta` 是它取的元数据（跟 ⇒ `metadata` · 不跟 ⇒ `symlink_metadata`），`open` 是对应的打开法。
pub fn read_capped_regular<F: std::io::Read>(
    meta: &std::fs::Metadata,
    open: impl FnOnce() -> std::io::Result<F>,
    cap: Cap,
) -> std::io::Result<Regular> {
    if !meta.is_file() {
        return Ok(Regular::NotRegular);
    }
    if meta.len() > cap.bytes {
        return Ok(Regular::OverCap(Some(meta.len())));
    }
    Ok(match read_capped_from(open()?, cap)? {
        CappedAll::All(b) => Regular::Whole(b),
        CappedAll::OverCap => Regular::OverCap(None),
    })
}

/// 声明表的一行：落点（家目录底下 `.cc-monitor/` 那一层起）· 主人类（①–⑤，[`Owner`]）· 谁写（[`Place`]：本机后端 · 壳）·
/// 格式 · 读上限（字节与超了怎么办，见 [`Cap`]）·
/// 下限抬于哪一版（`X.Y.Z`；下限低于版本的行必写：`package.json` 的 version 到了那一版，删升级步、下限抬成版本）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Row {
    pub rel: &'static str,
    pub owner: Owner,
    pub place: Place,
    pub format: Format,
    pub cap: Cap,
    pub floor_raised_in: Option<&'static str>,
}

impl Row {
    /// 丢了怎么办（由主人类推出）。
    pub const fn kind(&self) -> Kind {
        self.owner.kind()
    }
}

/// 种类那一格的类型形：[`State`] · [`Cache`]。
pub trait Class {
    const KIND: Kind;
}

/// 状态（[`Kind::State`]）。
pub struct State;
/// 缓存（[`Kind::Cache`]）。
pub struct Cache;

impl Class for State {
    const KIND: Kind = Kind::State;
}
impl Class for Cache {
    const KIND: Kind = Kind::Cache;
}

/// 挂得了升级步的种类：只有状态（缓存读到旧版就当没有、照别的东西重建）。
///
/// ```compile_fail,E0277
/// use relay_route_core::{upgradable, Cache, Cap, Decl, Over};
/// const C: Decl<Cache> =
///     Decl::<Cache>::json(".cc-monitor/c.json", 2, Cap { bytes: 1024, over: Over::SkipAndSay });
/// let _ = upgradable(&C);
/// ```
///
/// ```
/// use relay_route_core::{upgradable, Cap, Decl, Over, Owner, State};
/// const S: Decl<State> = Decl::<State>::json(
///     ".cc-monitor/s.json",
///     Owner::Machine,
///     2,
///     1,
///     Cap { bytes: 1024, over: Over::Reject },
/// );
/// let _ = upgradable(&S);
/// ```
pub trait Upgradable: Class {}
impl Upgradable for State {}

/// 一份落盘文件的声明（种类写在类型上：升级步只挂得上 `Decl<State>`，见 [`upgradable`]）。
///
/// 读上限只收 [`Cap`]：裸字节数编不过（超了怎么办跟着数一起写）。
///
/// ```compile_fail,E0308
/// use relay_route_core::{Decl, Owner, State};
/// const S: Decl<State> = Decl::<State>::json(".cc-monitor/s.json", Owner::Machine, 1, 1, 1024);
/// ```
pub struct Decl<K: Class> {
    pub row: Row,
    of: core::marker::PhantomData<K>,
}

impl<K: Class> Decl<K> {
    const fn new(rel: &'static str, owner: Owner, format: Format, cap: Cap) -> Self {
        assert!(
            matches!(
                (owner.kind(), K::KIND),
                (Kind::State, Kind::State) | (Kind::Cache, Kind::Cache)
            ),
            "状态的主人类是 ①–④，缓存是 ⑤"
        );
        Decl {
            row: Row {
                rel,
                owner,
                place: Place::Backend,
                format,
                cap,
                floor_raised_in: None,
            },
            of: core::marker::PhantomData,
        }
    }

    /// 删了会丢（家里那几样那一格）。
    pub const fn kept(&self) -> bool {
        matches!(self.row.owner.kind(), Kind::State)
    }

    /// 不是本机后端写的那一份（落点仍在家目录底下）：谁写。
    pub const fn written_by(mut self, place: Place) -> Self {
        self.row.place = place;
        self
    }
}

impl Decl<State> {
    /// 我们的 JSON：此刻写 `version`，读得懂 `floor..=version`（低于 `version` 的经升级步）；主人类 ①–④。
    pub const fn json(rel: &'static str, owner: Owner, version: u32, floor: u32, cap: Cap) -> Self {
        assert!(
            floor >= 1 && floor <= version,
            "版本从 1 起，下限不高于版本"
        );
        Self::new(rel, owner, Format::Json { version, floor }, cap)
    }

    /// 不带版本（不是我们的格式，或定了不加版本的）；主人类 ①–④。
    pub const fn foreign(rel: &'static str, owner: Owner, cap: Cap) -> Self {
        Self::new(rel, owner, Format::Foreign, cap)
    }

    /// 下限抬于哪一版（`X.Y.Z`）：只挂在下限低于版本的那一行上。
    pub const fn floor_raised_in(mut self, release: &'static str) -> Self {
        assert!(
            matches!(self.row.format, Format::Json { version, floor } if floor < version),
            "只有下限低于版本的行才有下限要抬"
        );
        self.row.floor_raised_in = Some(release);
        self
    }
}

impl Decl<Cache> {
    /// 我们的 JSON：缓存不升级，只读得懂这一版。
    pub const fn json(rel: &'static str, version: u32, cap: Cap) -> Self {
        assert!(version >= 1, "版本从 1 起");
        Self::new(
            rel,
            Owner::Cache,
            Format::Json {
                version,
                floor: version,
            },
            cap,
        )
    }

    /// 不带版本（不是我们的格式）。
    pub const fn foreign(rel: &'static str, cap: Cap) -> Self {
        Self::new(rel, Owner::Cache, Format::Foreign, cap)
    }
}

/// 升级步挂得上的那一份（只收 [`Upgradable`]，即状态）。后端 `own_state::upgrade!` 经它。
pub const fn upgradable<K: Upgradable>(d: &Decl<K>) -> &Decl<K> {
    d
}

/// 退出行为那一份的读上限（只有一格布尔）。
const BACKEND_POLICY_MAX_BYTES: Cap = Cap {
    bytes: 4096,
    over: Over::Reject,
};
/// 退出行为设置。
pub const BACKEND_POLICY_FILE: Decl<State> = Decl::<State>::json(
    BACKEND_POLICY_REL,
    Owner::Machine,
    1,
    1,
    BACKEND_POLICY_MAX_BYTES,
);
/// 这台偏好那一份的读上限（几张小表）。
const PREFS_MAX_BYTES: Cap = Cap {
    bytes: 256 * 1024,
    over: Over::Reject,
};
/// 这台的偏好（第 2 版：恢复那一格从开关 `resumeInTmux` 换成三选 `resumeWhere`；第 1 版经一步升级读，5.0.4 起不读）。
pub const PREFS_FILE: Decl<State> =
    Decl::<State>::json(PREFS_REL, Owner::Machine, 2, 1, PREFS_MAX_BYTES).floor_raised_in("5.0.4");
/// 机器表那一份的读上限（至多 256 台）。
const MACHINES_MAX_BYTES: Cap = Cap {
    bytes: 1024 * 1024,
    over: Over::Reject,
};
/// 机器表。
pub const MACHINES_FILE: Decl<State> = Decl::<State>::json(
    MACHINES_REL,
    Owner::End(End::Desktop),
    1,
    1,
    MACHINES_MAX_BYTES,
);
/// monitor 设置那一份的读上限（超了当读不懂：读那一口报错、写口一个字节不写）。
const CONFIG_MAX_BYTES: Cap = Cap {
    bytes: 4 * 1024 * 1024,
    over: Over::Reject,
};
/// monitor 的设置（不带版本，定了的；monitor 在自己的数据目录里按 [`file_name_of`] 取名）。
pub const CONFIG_FILE: Decl<State> =
    Decl::<State>::foreign(CONFIG_REL, Owner::End(End::Desktop), CONFIG_MAX_BYTES)
        .written_by(Place::Shell);
/// 「待办」选择那一份的读上限（超了当读不懂：不覆盖、`marks_unreadable`）。
const CHORES_MAX_BYTES: Cap = Cap {
    bytes: 64 * 1024,
    over: Over::Reject,
};
/// 「待办」里记下的选择。
pub const CHORES_FILE: Decl<State> =
    Decl::<State>::json(CHORES_REL, Owner::Machine, 1, 1, CHORES_MAX_BYTES);
/// 计划认可与退回那一份的读上限（超了当读不懂：不覆盖）。
const PLAN_REVIEW_MAX_BYTES: Cap = Cap {
    bytes: 1024 * 1024,
    over: Over::Reject,
};
/// 计划的认可与退回。
pub const PLAN_REVIEW_FILE: Decl<State> =
    Decl::<State>::json(PLAN_REVIEW_REL, Owner::Machine, 1, 1, PLAN_REVIEW_MAX_BYTES);
/// 上次值整份的读上限：每份封顶 × 三样（账号 · 文件与数据 · 组）× 台数封顶 ＝ 256 KiB × 3 × 64。
const LAST_SEEN_MAX_BYTES: Cap = Cap {
    bytes: 48 * 1024 * 1024,
    over: Over::SkipAndSay,
};
/// 离线那台的上次值（第 2 版：各台住 `origins` 底下，不和顶层 `version` 同一层）。
pub const LAST_SEEN_FILE: Decl<Cache> = Decl::<Cache>::json(LAST_SEEN_REL, 2, LAST_SEEN_MAX_BYTES);
/// 书签那一份的读上限。
const BOOKMARKS_MAX_BYTES: Cap = Cap {
    bytes: 256 * 1024,
    over: Over::Reject,
};
/// 书签。
pub const BOOKMARKS_FILE: Decl<State> =
    Decl::<State>::json(BOOKMARKS_REL, Owner::Machine, 1, 1, BOOKMARKS_MAX_BYTES);
/// skill 装记录（第 1 版写在 `"v"` 里，第 2 版换成顶层 `version`；第 1 版经一步升级读，5.0.4 起不读）。
pub const SKILL_LEDGER_FILE: Decl<State> = Decl::<State>::json(
    SKILL_LEDGER_REL,
    Owner::Machine,
    2,
    1,
    SKILL_LEDGER_MAX_BYTES,
)
.floor_raised_in("5.0.4");
/// skill 装记录那一份的读上限（超了当读不懂：不覆盖、`ledger_unreadable`）。
const SKILL_LEDGER_MAX_BYTES: Cap = Cap {
    bytes: 4 * 1024 * 1024,
    over: Over::Reject,
};
/// 资产目录。
pub const ASSET_CATALOG_FILE: Decl<State> = Decl::<State>::json(
    ASSET_CATALOG_REL,
    Owner::Machine,
    1,
    1,
    ASSET_CATALOG_MAX_BYTES,
);
/// 资产目录那一份的读上限（读不出来就不覆盖）。
const ASSET_CATALOG_MAX_BYTES: Cap = Cap {
    bytes: 16 * 1024 * 1024,
    over: Over::Reject,
};
/// 额度账。
pub const QUOTA_FILE: Decl<State> =
    Decl::<State>::json(QUOTA_LEDGER_REL, Owner::Machine, 1, 1, QUOTA_MAX_BYTES);
/// 额度账那一份的读上限（读不出来就不覆盖）。
const QUOTA_MAX_BYTES: Cap = Cap {
    bytes: 16 << 20,
    over: Over::Reject,
};
/// 轮换账本那一份的读上限。
const ROTATION_MAX_BYTES: Cap = Cap {
    bytes: 16 << 20,
    over: Over::Reject,
};
/// 账号轮换（第 4 版；第 3 版及更早当没有）。
pub const ROTATION_FILE: Decl<State> =
    Decl::<State>::json(ROTATION_REL, Owner::Machine, 4, 4, ROTATION_MAX_BYTES);
/// 历史注解那一份的读上限：一条约 150 字节 ⇒ 四十万条；真撞上说明那份文件不对劲，不拿截断的当完整的用。
const HISTORY_ANNOTATIONS_MAX_BYTES: Cap = Cap {
    bytes: 64 * 1024 * 1024,
    over: Over::Reject,
};
/// 历史注解（星标 · 改名 · 隐藏 · 组 …；住这台的数据目录，名字同 `creds_core::store::HISTORY_METADATA_FILE`）。
/// 主人类记 ①：一条会话的那几格；组那一节是 ③ 在这台的那一片，同住会话那一台（判据里 ①③ 同一条：只住后端）。
pub const HISTORY_ANNOTATIONS_FILE: Decl<State> = Decl::<State>::json(
    ".cc-monitor/history-metadata.json",
    Owner::Session,
    1,
    1,
    HISTORY_ANNOTATIONS_MAX_BYTES,
);
/// 血缘那一份的读上限（90 天清一次，远到不了）。
const LINEAGE_MAX_BYTES: Cap = Cap {
    bytes: 16 << 20,
    over: Over::Reject,
};
/// 会话血缘。
pub const LINEAGE_FILE: Decl<State> =
    Decl::<State>::json(LINEAGE_REL, Owner::Session, 1, 1, LINEAGE_MAX_BYTES);
/// 起会话用的号。
pub const LAUNCH_ACCOUNTS_FILE: Decl<State> = Decl::<State>::json(
    LAUNCH_ACCOUNTS_REL,
    Owner::Session,
    1,
    1,
    LAUNCH_ACCOUNTS_MAX_BYTES,
);
/// 起会话用的号那一份的读上限（读不出来就不覆盖）。
const LAUNCH_ACCOUNTS_MAX_BYTES: Cap = Cap {
    bytes: 16 << 20,
    over: Over::Reject,
};
/// 主机钥匙（OpenSSH 格式）：这一端认的各台指纹（本机后端替桌面那一端存）。
pub const KNOWN_HOSTS_FILE: Decl<State> = Decl::<State>::foreign(
    KNOWN_HOSTS_REL,
    Owner::End(End::Desktop),
    KNOWN_HOSTS_MAX_BYTES,
);
/// 主机钥匙那一份的读上限（一台一行，远到不了；读不出来 ⇒ 不覆盖、回错）。
const KNOWN_HOSTS_MAX_BYTES: Cap = Cap {
    bytes: 4 << 20,
    over: Over::Reject,
};
/// 开终端那一行的 ssh 配置（`ssh -F` 的格式；照机器表重写）。
pub const SSH_CONFIG_FILE: Decl<Cache> =
    Decl::<Cache>::foreign(SSH_CONFIG_REL, SSH_CONFIG_MAX_BYTES);
/// ssh 配置那一份的读上限（缓存：超了 ⇒ `warn!` 带路径，当没有、照机器表重写）。
const SSH_CONFIG_MAX_BYTES: Cap = Cap {
    bytes: 4 << 20,
    over: Over::SkipAndSay,
};
/// 中转钥匙（一行十六进制；丢了重铸）。
pub const KEY_FILE: Decl<Cache> = Decl::<Cache>::foreign(KEY_FILE_REL, KEY_FILE_MAX_BYTES);
/// 中转钥匙那一份的读上限（一行；超了 ⇒ `warn!` 带路径说读不懂，当没有、重铸一把）。
const KEY_FILE_MAX_BYTES: Cap = Cap {
    bytes: 4096,
    over: Over::SkipAndSay,
};
/// 历史清单缓存的读上限。
const LISTING_CACHE_MAX_BYTES: Cap = Cap {
    bytes: 256 << 20,
    over: Over::SkipAndSay,
};
/// 历史清单缓存。
pub const LISTING_CACHE_FILE: Decl<Cache> =
    Decl::<Cache>::json(LISTING_CACHE_REL, 3, LISTING_CACHE_MAX_BYTES);
/// 一条会话的消息队（[`SESSION_QUEUE_DIR_REL`] 底下一条会话一份）。
pub const SESSION_QUEUE_FILE: Decl<State> = Decl::<State>::json(
    ".cc-monitor/session-queue/<sid>.json",
    Owner::Session,
    1,
    1,
    SESSION_QUEUE_MAX_BYTES,
);
/// 一条会话那一份消息队的读上限（满队 × 最长一句，留余量）。
const SESSION_QUEUE_MAX_BYTES: Cap = Cap {
    bytes: 24 << 20,
    over: Over::Reject,
};
/// 起会话便条（[`LAUNCH_NOTES_DIR_REL`] 底下一个进程一张；丢了只是那一次没记上号）。
pub const LAUNCH_NOTE_FILE: Decl<Cache> = Decl::<Cache>::json(
    ".cc-monitor/launch-pending/<pid>.json",
    1,
    LAUNCH_NOTE_MAX_BYTES,
);
/// 一张起会话便条的读上限。
const LAUNCH_NOTE_MAX_BYTES: Cap = Cap {
    bytes: 16 << 20,
    over: Over::SkipAndSay,
};
/// 一份记录的骨架（[`SKELETON_DIR_REL`] 底下一份记录一份；能照记录重扫）。
pub const SKELETON_FILE: Decl<Cache> = Decl::<Cache>::json(
    ".cc-monitor/cache/skeleton/<digest>.json",
    9,
    SKELETON_MAX_BYTES,
);
/// 一份骨架的读上限。
const SKELETON_MAX_BYTES: Cap = Cap {
    bytes: 1 << 30,
    over: Over::SkipAndSay,
};
/// 远端那台的清单副本（[`REMOTE_LISTING_DIR_REL`] 底下一台一份；原样是那台交来的字节）。
pub const REMOTE_LISTING_FILE: Decl<Cache> = Decl::<Cache>::foreign(
    ".cc-monitor/cache/remote-listing/<machine>.json",
    REMOTE_LISTING_MAX_BYTES,
);
/// 一台远端清单副本的读上限。
const REMOTE_LISTING_MAX_BYTES: Cap = Cap {
    bytes: 64 << 20,
    over: Over::SkipAndSay,
};
/// API key 表（住数据目录根上；人手改的那一份，未知键原样留）。
pub const APIKEY_CREDENTIALS_FILE: Decl<State> = Decl::<State>::json(
    ".cc-monitor/apikey-credentials.json",
    Owner::Machine,
    1,
    1,
    APIKEY_CREDENTIALS_MAX_BYTES,
);
/// API key 表的读上限（一个号一格 key）。
const APIKEY_CREDENTIALS_MAX_BYTES: Cap = Cap {
    bytes: 4 << 20,
    over: Over::Reject,
};
/// 订阅号的凭据文件（一个号一份，住那个号的配置目录、文件名由那一家登记；那一家的格式）。
pub const ACCOUNT_CREDS_FILE: Decl<State> = Decl::<State>::foreign(
    "<account-config-dir>/<credentials-file>",
    Owner::Machine,
    ACCOUNT_CREDS_MAX_BYTES,
);
/// 订阅号凭据文件的读上限。
const ACCOUNT_CREDS_MAX_BYTES: Cap = Cap {
    bytes: 1 << 20,
    over: Over::Reject,
};
/// 一条会话一份的上游描述（起会话便条那个目录底下，按来处存）。
pub const SESSION_UPSTREAM_FILE: Decl<State> = Decl::<State>::json(
    ".cc-monitor/launch-pending/upstream/<origin>.json",
    Owner::Session,
    1,
    1,
    SESSION_UPSTREAM_MAX_BYTES,
);
/// 一份上游描述的读上限。
const SESSION_UPSTREAM_MAX_BYTES: Cap = Cap {
    bytes: 16 << 20,
    over: Over::Reject,
};
/// 一个托管终端一份的事实。
pub const HOST_FACTS_FILE: Decl<State> = Decl::<State>::json(
    ".cc-monitor/hosts/<id>/facts.json",
    Owner::Machine,
    1,
    1,
    HOST_FACTS_MAX_BYTES,
);
/// 事实文件的读上限（几百字节的东西，大了就不是我们写的）。
const HOST_FACTS_MAX_BYTES: Cap = Cap {
    bytes: 64 * 1024,
    over: Over::Reject,
};
/// 托管进程跑的那份按版本的程序副本（后端不读它，只交给系统去跑；没有 · 坏了照自己再复制一份）。
pub const HOST_PROGRAM_FILE: Decl<Cache> = Decl::<Cache>::foreign(
    ".cc-monitor/hosts/.bin/<version>/ccm.exe",
    HOST_PROGRAM_NOT_READ,
);
/// 程序副本不读：上限是 0，真有一处去读，读口就照缓存那一档当没有、`warn!` 带路径。
const HOST_PROGRAM_NOT_READ: Cap = Cap {
    bytes: 0,
    over: Over::SkipAndSay,
};
/// cc-bus 的状态（[`CC_BUS_DIR_REL`] 底下：各会话已读位置 ＋ 发信规矩的账）。
pub const CC_BUS_STATE_FILE: Decl<State> = Decl::<State>::json(
    ".cc-monitor/cc-bus/state.json",
    Owner::Machine,
    1,
    1,
    CC_BUS_STATE_MAX_BYTES,
);
/// cc-bus 状态那一份的读上限。
const CC_BUS_STATE_MAX_BYTES: Cap = Cap {
    bytes: 4 << 20,
    over: Over::Reject,
};
/// 一条会话的 cc-bus 收件箱（[`CC_BUS_DIR_REL`] 底下一条会话一份；一封一行，只往后加）。
pub const CC_BUS_INBOX_FILE: Decl<State> = Decl::<State>::foreign(
    ".cc-monitor/cc-bus/<sid>.jsonl",
    Owner::Session,
    CC_BUS_INBOX_MAX_BYTES,
);
/// 一份 cc-bus 收件箱的读上限：一封最多千来字节，64 MiB 装得下几万封。
const CC_BUS_INBOX_MAX_BYTES: Cap = Cap {
    bytes: 64 << 20,
    over: Over::Reject,
};
/// 答题插件的一份文件（[`ANSWER_PLUGIN_DIR_REL`] 底下；照这一版现写，读回只为比内容一样不一样）。
pub const ANSWER_PLUGIN_FILE: Decl<Cache> =
    Decl::<Cache>::foreign(".cc-monitor/answer-plugin/<file>", ANSWER_PLUGIN_MAX_BYTES);
/// 答题插件一份文件的读上限（几份小脚本与清单）。
const ANSWER_PLUGIN_MAX_BYTES: Cap = Cap {
    bytes: 1 << 20,
    over: Over::SkipAndSay,
};
/// 套组名 ⇒ 为它建的那一组的 id（顶层键是组名，带 `version` 会撞名 ⇒ 不带版本）。
pub const SET_GROUPS_FILE: Decl<State> =
    Decl::<State>::foreign(SET_GROUPS_REL, Owner::Set, SET_GROUPS_MAX_BYTES);
/// 套组绑定那一份的读上限。
const SET_GROUPS_MAX_BYTES: Cap = Cap {
    bytes: 1 << 20,
    over: Over::Reject,
};
/// 组的待补：改名 · 解散发给各片时没送到的那几台，这一端欠它们的那几句（本机后端替这一端记，`history/groups_owed.rs`；
/// 那台再答 `groups-here` 时补发、补成了消掉）。
pub const GROUPS_OWED_FILE: Decl<State> = Decl::<State>::json(
    GROUPS_OWED_REL,
    Owner::End(End::Desktop),
    1,
    1,
    GROUPS_OWED_MAX_BYTES,
);
/// 待补那一份的读上限（一条几十字节，台数 × 组数封顶）。
const GROUPS_OWED_MAX_BYTES: Cap = Cap {
    bytes: 1 << 20,
    over: Over::Reject,
};
/// 一份轮换策略（`strategies/` 底下一份一个 `.rhai`，用户自己的源码）。
pub const STRATEGY_SOURCE_FILE: Decl<State> = Decl::<State>::foreign(
    ".cc-monitor/strategies/<id>.rhai",
    Owner::Machine,
    STRATEGY_SOURCE_MAX_BYTES,
);
/// 一份轮换策略的读上限（超了不加载，列表里照实写读不懂）。
const STRATEGY_SOURCE_MAX_BYTES: Cap = Cap {
    bytes: 64 * 1024,
    over: Over::Reject,
};

/// 〔「一份落盘文件一行」〕上面那几份排成一张（判据逐行查自洽、查家里那几样与声明对得上 · 壳那一侧读这一段原文查「文件与数据」那一页列全了它）。
#[cfg(test)]
const OWN_FILES: &[Row] = &[
    BACKEND_POLICY_FILE.row,
    PREFS_FILE.row,
    MACHINES_FILE.row,
    CHORES_FILE.row,
    PLAN_REVIEW_FILE.row,
    LAST_SEEN_FILE.row,
    BOOKMARKS_FILE.row,
    SKILL_LEDGER_FILE.row,
    ASSET_CATALOG_FILE.row,
    QUOTA_FILE.row,
    ROTATION_FILE.row,
    HISTORY_ANNOTATIONS_FILE.row,
    LINEAGE_FILE.row,
    LAUNCH_ACCOUNTS_FILE.row,
    KNOWN_HOSTS_FILE.row,
    SSH_CONFIG_FILE.row,
    KEY_FILE.row,
    CONFIG_FILE.row,
    LISTING_CACHE_FILE.row,
    SESSION_QUEUE_FILE.row,
    LAUNCH_NOTE_FILE.row,
    SKELETON_FILE.row,
    REMOTE_LISTING_FILE.row,
    APIKEY_CREDENTIALS_FILE.row,
    ACCOUNT_CREDS_FILE.row,
    SESSION_UPSTREAM_FILE.row,
    HOST_FACTS_FILE.row,
    HOST_PROGRAM_FILE.row,
    CC_BUS_STATE_FILE.row,
    CC_BUS_INBOX_FILE.row,
    ANSWER_PLUGIN_FILE.row,
    SET_GROUPS_FILE.row,
    GROUPS_OWED_FILE.row,
    STRATEGY_SOURCE_FILE.row,
];

/// 家里那几样的一格，照声明取落点与「删了会丢」。
const fn declared<K: Class>(
    id: &'static str,
    d: &Decl<K>,
) -> (&'static str, &'static str, bool, bool) {
    (id, d.row.rel, false, d.kept())
}

/// 〔「家里的都进这一份」〕后端在这台自己家里放的每一样：`(名字, 相对家目录, 是目录, 删了会丢)`。名字是闭集（界面按它取说法）；
/// 后端落点由它所在的 `bin` 那一行代表；只住 monitor 那一侧的（API key 表 · 后端错误输出）不在这里。
/// 有声明（[`OWN_FILES`]）的那几份经 `declared` 照声明取，「删了会丢」不另写。
/// 后端「文件与数据」那一份成品逐样 stat 它（`footprint/data.rs::own_rows`）。
pub const OWN_HOME_ENTRIES: &[(&str, &str, bool, bool)] = &[
    ("bin", ".cc-monitor/bin", true, false),
    ("staging", STAGING_DIR_REL, true, false),
    declared("relayKey", &KEY_FILE),
    ("listenDir", LISTEN_DIR_REL, true, true),
    declared("policy", &BACKEND_POLICY_FILE),
    declared("prefs", &PREFS_FILE),
    declared("machines", &MACHINES_FILE),
    ("profiles", PROFILES_REL, false, true),
    ("profilesWritten", PROFILES_WRITTEN_REL, false, false),
    ("aliasesPosix", POSIX_ALIASES_REL, false, true),
    ("aliasesPs", PS_ALIASES_REL, false, true),
    declared("skillLedger", &SKILL_LEDGER_FILE),
    declared("chores", &CHORES_FILE),
    declared("planReview", &PLAN_REVIEW_FILE),
    declared("lastSeen", &LAST_SEEN_FILE),
    declared("bookmarks", &BOOKMARKS_FILE),
    declared("assetCatalog", &ASSET_CATALOG_FILE),
    declared("quota", &QUOTA_FILE),
    declared("rotation", &ROTATION_FILE),
    declared("lineage", &LINEAGE_FILE),
    declared("launchAccounts", &LAUNCH_ACCOUNTS_FILE),
    ("sessionQueue", SESSION_QUEUE_DIR_REL, true, true),
    ("ccBus", CC_BUS_DIR_REL, true, true),
    ("launchNotes", LAUNCH_NOTES_DIR_REL, true, false),
    ("answerPlugin", ANSWER_PLUGIN_DIR_REL, true, false),
    declared("knownHosts", &KNOWN_HOSTS_FILE),
    declared("sshConfig", &SSH_CONFIG_FILE),
    ("accounts", ACCOUNTS_DIR_REL, true, true),
    ("accountsMcp", ACCOUNTS_MCP_REL, false, true),
    ("extBackups", EXT_BACKUPS_DIR_REL, true, true),
    ("skeleton", SKELETON_DIR_REL, true, false),
    declared("listingCache", &LISTING_CACHE_FILE),
    ("remoteListing", REMOTE_LISTING_DIR_REL, true, false),
    declared("setGroups", &SET_GROUPS_FILE),
    declared("groupsOwed", &GROUPS_OWED_FILE),
    ("strategies", STRATEGIES_DIR_REL, true, true),
    ("hosts", HOSTS_DIR_REL, true, true),
];

// 测试档的家一开始就是沙箱（`src/common/test-home`）：这一行让链接器把它的启动段带进本 crate 的测试二进制。
#[cfg(test)]
#[used]
static TEST_HOME: fn() = test_home::linked;
