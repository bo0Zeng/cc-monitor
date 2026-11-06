//! 从一套 [`Opts`] 算出「这一趟到底要干什么」，以及 `--ccm-print` 那条等价命令行（前叫 `--print`）。
//!
//! # 为什么计划与执行分开
//!
//! `--ccm-print` 是这套 CLI 的**平价预言机**：它吐的那一行必须与真跑那一趟**同源**，
//! 否则「print 说的」与「真做的」会各漂各的（`tests/e2e/ccm-contract-parity.sh` 的 A / A′ 两组
//! 整组就是在钉这一条，07-31 真逮到过一次「print 退回读文件」）。
//! ⇒ 这里只产出 [`Plan`]，`--ccm-print` 与真跑**读同一个 `Plan`**，结构上不可能分叉。
//!
//! # 本文件不许出现 ccm 旗标的字面量
//!
//! 旗标名的唯一住址是 [`crate::ccm_line::argv::flag`]。由
//! `argv::tests::ccm_flag_literals_live_only_in_argv_rs` 机检。

use crate::ccm_line::argv::{flag, parse_size, CwdSpec, Die, Opts};
use crate::platform::fs::Ro;
use copy_core::copy_text;
use relay_route_core::{Cap, Over};
// 铸名避让那张 hash 表的**唯一**来源（字段模块私有 ⇒ 这里造不出第二份）。
use crate::accounts::upstream_select::endpoint::LaunchAccount;
use crate::common::session_snapshot::TakenNames;
use crate::platform::shell::posix;
use shell_quote_core::posix_quote as sq;

/// 直路注入中转地址那一句（`--ccm-print` 与非得经 shell 那一趟同一份）：`var` ＝ 那一家认的地址环境变量，
/// `pass` ＝ 这台的只许直通那一把（[`Env::relay_pass_key`]）。
pub fn relay_export(var: &str, url: &str, pass: Option<&str>) -> String {
    posix::export(
        var,
        &posix_relay_word(url, url, pass).unwrap_or_else(|| qarg(url)),
    )
}

/// 地址拼进参数的那一家：argv 里装着那条不带钥匙的地址的那个词 ⇒ 切成（地址前那一截 ＋ 口之后那一格, 路由起那一截 ＋ 地址后那一截），
/// 钥匙插在两截中间。不是这个词 / 地址不是构造口产物 ⇒ `None`。exec 插真钥匙与 shell 那一行插钥匙段都按它切。
pub(crate) fn split_at_key(word: &str, url: &str) -> Option<(String, String)> {
    let (before, after) = word.split_once(url)?;
    let (head, tail) = crate::accounts::upstream_select::endpoint::base_url_halves(url)?;
    Some((format!("{before}{head}"), format!("{tail}{after}")))
}

/// 装着那条中转地址的那个词在 shell 那一行里怎么插钥匙段：按地址的路由（`/t/` ⇒ 照字面插只许直通那一把；
/// `/s/` ⇒ 读那台钥匙文件的命令替换，全权那一把不进打印出来的命令）。
enum KeyedWord {
    Literal(String),
    FromFile(String, String),
}

/// 不是那个词 / `/t/` 而这台读不到那一把 ⇒ `None`（照原样写，地址不带钥匙）。
fn keyed_word(word: &str, url: &str, pass: Option<&str>) -> Option<KeyedWord> {
    let (head, tail) = split_at_key(word, url)?;
    if crate::accounts::upstream_select::endpoint::takes_pass_key(url) {
        Some(KeyedWord::Literal(format!("{head}{}{tail}", pass?)))
    } else {
        Some(KeyedWord::FromFile(head, tail))
    }
}

fn posix_relay_word(word: &str, url: &str, pass: Option<&str>) -> Option<String> {
    Some(match keyed_word(word, url, pass)? {
        KeyedWord::Literal(w) => qarg(&w),
        KeyedWord::FromFile(head, tail) => {
            posix::home_file_between(&sq(&head), relay_route_core::KEY_FILE_REL, &sq(&tail))
        }
    })
}

fn ps_relay_word(word: &str, url: &str, pass: Option<&str>) -> Option<String> {
    use crate::platform::shell::dialect::ps_literal as pq;
    Some(match keyed_word(word, url, pass)? {
        KeyedWord::Literal(w) => pq(&w),
        KeyedWord::FromFile(head, tail) => crate::platform::shell::powershell::home_file_between(
            &pq(&head),
            relay_route_core::KEY_FILE_REL,
            &pq(&tail),
        ),
    })
}

/// 环境里继承来的那一条上游地址（[`Env::base_url_env`] 那个变量）里，**用户自己的端点**那一个（不是我们的中转那一形）。
///
/// 我们的中转地址属于某一个号（路由里带着账号段）：它是外层 shell / 上一趟 ccm 留下的，
/// 不是这一发的 ⇒ 不认（容器路不转进 pane，直路按这一发的目标账号重问）。用户自己的端点 ⇒ 这条会话的上游
/// （照样注入中转，中转按这条会话的上游描述直通过去；没注入 ⇒ 原样不动）。
///
/// 〔继承表〕中转地址那一格：只有用户自己的端点往下传。
fn user_base_url(env: &Env) -> Option<&str> {
    env.inherited_base_url
        .as_deref()
        .filter(|v| !v.is_empty() && !crate::accounts::upstream_select::endpoint::ours(v))
}

// ═══════════════════════════════════════════════════════════════════════════
// 继承表：新会话从调用方（终端 · 某条会话的 shell · 脚本）拿什么
// ═══════════════════════════════════════════════════════════════════════════

/// 继承表的一格是哪一项。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Item {
    /// 工作目录。
    Cwd,
    /// 账号。
    Account,
    /// 轮换（换号那一份设置）。
    Rotation,
    /// 中转地址。
    Relay,
    /// 主会话（会话血缘）。
    Parent,
    /// 嵌套标记 · 各家「我是哪个会话」的变量。
    Nested,
    /// 启动命令 · 模型 · 进不进 tmux。
    MachinePrefs,
    /// 机器级环境（`CCM_ENV` 那一串）。
    MachineEnv,
    /// 别名。
    Alias,
    /// 标签页（进不进标签栏、历史默认清单）。
    Tab,
    /// 其余环境变量。
    OtherEnv,
    /// 后端自己的载体（常驻开关 · 诊断文件 · 后端内部那几族）。
    BackendCarriers,
}

/// 一格在一条路上怎么来。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Pass {
    /// 原样从调用方拿。
    Caller,
    /// 从调用方拿，显式写进窗格里那一条命令（tmux 边界会把环境吃掉，不写就没了）。
    Carried,
    /// 不看调用方的，这一发自己定（重新问 · 只认这一发自己写的）。
    Fresh,
    /// 照这台的设置，不随主会话。
    Machine,
    /// 那台 tmux 的环境（不搬调用方的）。
    Pane,
    /// 清掉，不往下传。
    Cleared,
}

/// 谁能改这一格（`Flags` 是这一发就能改的 ccm 选项：「来自」照它去合并结果里找；界面起会话时把这台偏好渲成这几个旗标交进来）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Change {
    /// 这几个 ccm 选项（命令行或别名里写）。
    Flags(&'static [&'static str]),
    /// 这一发的 `@名`。
    AliasName,
    /// 设置里（这台的 · 会话自己的那一份）。
    Settings,
    /// 改不了。
    Nobody,
}

/// 继承表的一行：哪一项 · 直路怎么来 · tmux 路怎么来 · 谁能改 · 判它的函数（`crate::` 开头的全路径，头注带「〔继承表〕」）。
#[derive(Debug, Clone, Copy)]
pub(crate) struct Inherit {
    pub(crate) item: Item,
    pub(crate) direct: Pass,
    pub(crate) tmux: Pass,
    pub(crate) change: Change,
    /// 只有判据读它（两向对拍）。
    #[cfg_attr(not(test), expect(dead_code, reason = "只有判据读它（两向对拍）"))]
    pub(crate) judge: &'static str,
}

/// **继承表**：新会话从调用方拿什么，一格一行。判还是那几个函数判，这张表是它们唯一的出处与说明；
/// 判据两向对拍（表里每一格点名的函数在、头注带「〔继承表〕」；带它的函数每一个都在表里）。`--ccm-print` 的「来自」一节按它的项写。
///
/// - 密钥不靠继承走：我们的中转钥匙在 exec 那一刻从这台钥匙文件读进来插上，打印出来的命令里没有；继承来的我们那一形地址不认（[`Item::Relay`]）。
/// - 用户自己设的环境变量（含他自己的密钥）不按名字猜着剥：直路照继承，tmux 路不搬（[`Item::OtherEnv`]）。
///   tmux 路要一样就得把调用方整份环境写进 tmux 命令或键入的那一行，等于把密钥摊在进程表与窗格回滚里 ⇒ 两条路这一格就是不一样。
/// - 轮换：新会话照「新会话用」那一份，不跟随主会话（血缘今天只记下，轮换不读它）；`--no-rotation` 那一发不轮换。
pub(crate) const INHERIT: &[Inherit] = &[
    Inherit {
        item: Item::Cwd,
        direct: Pass::Caller,
        tmux: Pass::Carried,
        change: Change::Flags(&[flag::CWD, flag::CWD_IF]),
        judge: "crate::control::ccm::plan::resolve_cwd",
    },
    Inherit {
        item: Item::Account,
        direct: Pass::Caller,
        tmux: Pass::Carried,
        change: Change::Flags(&[flag::ACCOUNT, flag::ACCOUNT_DIR, flag::BASE]),
        judge: "crate::control::ccm::plan::account_for",
    },
    Inherit {
        item: Item::Rotation,
        direct: Pass::Fresh,
        tmux: Pass::Fresh,
        // 不写 ⇒ 照这台设置里「新会话用」那一份；`--no-rotation` ⇒ 这一发不轮换。
        change: Change::Flags(&[flag::NO_ROTATION]),
        judge: "crate::accounts::quota::setup::uses::Setup::for_new",
    },
    Inherit {
        item: Item::Relay,
        direct: Pass::Fresh,
        tmux: Pass::Fresh,
        change: Change::Nobody,
        judge: "crate::control::ccm::plan::user_base_url",
    },
    Inherit {
        item: Item::Parent,
        direct: Pass::Caller,
        tmux: Pass::Carried,
        change: Change::Nobody,
        judge: "crate::control::ccm::plan::parent_from",
    },
    Inherit {
        item: Item::Nested,
        direct: Pass::Cleared,
        tmux: Pass::Cleared,
        change: Change::Nobody,
        judge: "crate::control::ccm::plan::nested_of",
    },
    Inherit {
        item: Item::MachinePrefs,
        direct: Pass::Machine,
        tmux: Pass::Machine,
        change: Change::Flags(&[flag::LAUNCHER, flag::TMUX, flag::TMUX_BASE]),
        judge: "crate::control::launch_render::Launch::launchers",
    },
    Inherit {
        item: Item::MachineEnv,
        direct: Pass::Machine,
        tmux: Pass::Pane,
        change: Change::Settings,
        judge: "crate::control::ccm::plan::machine_env",
    },
    Inherit {
        item: Item::Alias,
        direct: Pass::Fresh,
        tmux: Pass::Carried,
        change: Change::AliasName,
        judge: "crate::assets::aliases::profile::resolve",
    },
    Inherit {
        item: Item::Tab,
        direct: Pass::Machine,
        tmux: Pass::Machine,
        change: Change::Flags(&[flag::TAB]),
        judge: "crate::history::session_hidden::judge",
    },
    Inherit {
        item: Item::OtherEnv,
        direct: Pass::Caller,
        tmux: Pass::Pane,
        change: Change::Nobody,
        judge: "crate::control::ccm::plan::carried_into_pane",
    },
    Inherit {
        item: Item::BackendCarriers,
        direct: Pass::Cleared,
        tmux: Pass::Cleared,
        change: Change::Nobody,
        judge: "crate::platform::child_env::is_internal",
    },
];

/// 这一趟能看见的**外界**。做成结构体的唯一理由：让整条计划面可以在单测里跑，
/// **一个字节都不碰这台机器**（「只能做产品，不能动机器」）。
#[derive(Debug, Clone, Default)]
pub struct Env {
    pub home: String,
    pub pwd: String,
    /// `$TMUX` —— 只有**被 attach 的那个进程**才有它。计划不认它的值（不认 socket），只问在不在 tmux 里（[`Env::in_tmux`]）。
    pub tmux: Option<String>,
    /// 这一趟是 tmux 那一形的自检（[`SELF_CHECK_IN_PANE`] 设的标记）：照窗格里那一跳算（在 tmux 里），不为它去问 tmux server。
    pub self_check: bool,
    /// 不在 tmux 里敲 `cc` 落托管容器（各平台都能托管；已在托管终端里 —— `CCM_HOST` 设了 —— 不再套一层）；`false` ⇒ 直路。
    pub can_host: bool,
    /// 调用方**继承来的**账号配置目录（`agents::claudecode::paths::CONFIG_DIR_ENV`
    /// 那个环境变量的值），**不是**本进程设的。
    ///
    /// ⚠ 名字刻意不叫 `claude_*`：那会让 `agent_locality_guard` 的 `.claude` 那根针
    ///   在**字段访问**上命中（`env.claude_config_dir` 里逐字含 `.claude`）——
    ///   而这个字段装的是「调用方选了哪个号」，不是 Claude 的目录布局。
    pub inherited_config_dir: Option<String>,
    /// 这一家找上游读的那个环境变量（`agents::base_url_env_of`，由 `mod.rs` 解析完 argv 补；没登记上游 ⇒ 空串：不注入、不清）。
    pub base_url_env: String,
    /// 调用方**继承来的**那个变量的值（同上，由 `mod.rs` 补；预览不继承）。
    pub inherited_base_url: Option<String>,
    /// 起 agent 前要 eval 的机器级 env 串。
    pub ccm_env: String,
    // 🔴 没有「工作区」那一格（`$CCM_WORKSPACE` 不读）：[`resolve_cwd`] 不替用户挑目录，
    //    留一个这样的字段就是「猜」的一个待命开关 —— 失效方向正是「把猜挪进别处」。
    /// 账号库 manifest 的**完整路径**：`<家目录>/.cc-monitor/accounts/accounts.json`，只跟着家走
    /// （没有另指位置的环境变量；判据夹具直接填这一格）。
    pub accts_manifest: String,
    /// **账号维度的载体**：切账号靠改哪个环境变量。由 `mod.rs` 从
    /// `agents::account_env_of(<这一趟的 agent>)` 取来 —— 本文件不认识任何 agent 的名字。
    pub account_env: String,
    pub self_argv: Vec<String>,
    /// 问「此刻哪些会话在跑」的那一次扫描（观测层的，由入口注入 —— control 不引用 observe）。
    /// 入参 = 这一趟要用的账号配置目录（`None` = agent 自己的默认家目录）。`None` = 这一趟不问（预览 / 不是 resume）。
    pub running_sessions: Option<super::RunningScan>,
    /// 这一发指到哪个中转地址（上游选择那张决策表，入口注入）。`None` = 不问（判据 / 不起 agent）。
    pub relay: Option<RelayAsk>,
    /// 〔会话血缘〕调用方是哪个会话的 shell：（那一家导的变量名, 会话编号）—— 某一家的「我是哪个会话」变量
    /// （`agents::self_sid_envs`）在环境里、值过段闸。`None` ＝ 不在任何会话里（或预览）。
    pub inherited_parent: Option<(String, String)>,
    /// 〔会话血缘〕这一趟现铸的来处（拼进中转地址尾上，中转第一次看见就绑给这个新会话）。`None` ＝ 不铸（预览 / 判据）。
    pub origin_token: Option<String>,
    /// 这台盘上根钥匙派生出来的只许直通那一把：非得经 shell 那一趟、`/t/` 地址照字面写进那一行（它只开得了 `/t/`）。
    /// 读不到 ⇒ `None`（那条地址写成不带钥匙的）。
    pub relay_pass_key: Option<String>,
    /// 答题 hook 叫的那个 `ccm`（这台的入口 `<家>/.cc-monitor/bin/ccm`，绝对路径；后端升级换真身也不变）。
    /// `None` ＝ 不挂（预览 / 判据 / 家目录说不出）：这一发起的会话里的框只能在终端里答。
    pub answer_hook_entry: Option<String>,
    /// 读用户自己给的会话级设置文件（并进答题 hook 用）；`None` ＝ 不读（预览 / 判据没喂）：用户给了那个旗标就原样交出去、不并、也不说。
    pub read_text: Option<fn(&std::path::Path) -> Option<String>>,
    /// 换了启动器那一支要问的这台事实（插件目录变量继承来的值 · 那一家的版本 · 本机 managed 策略 · 读不读得出别的进程的环境）；
    /// `None` ＝ 不问（预览 / 判据没喂）⇒ 照「自定义启动器」标挂不上。
    pub sideload: Option<fn(&crate::agents::PluginFace) -> Sideload>,
}

/// 换了启动器那一支起会话前看的这台事实（[`Env::sideload`]）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Sideload {
    /// 调用方继承来的那个插件目录变量（用户自己设的）：我们的目录接在它后面。
    pub inherited: Option<String>,
    /// `PATH` 上那一家的版本；认不出 ⇒ `None`。
    pub version: Option<[u64; 3]>,
    /// 本机 managed 策略不许这一路。
    pub policy_blocked: bool,
    /// 这台读得出别的进程起来时的环境（hook 判子进程、会话事实判接没接上都靠它；今天只 Linux）。
    pub env_readable: bool,
}

/// 环境里认出调用方那个会话（[`Env::inherited_parent`]）：各家登记的变量按注册序，头一个有值、值过段闸的。
///
/// 〔继承表〕主会话那一格。
pub(crate) fn parent_from(get: &dyn Fn(&str) -> Option<String>) -> Option<(String, String)> {
    crate::agents::self_sid_envs().into_iter().find_map(|k| {
        get(k)
            .filter(|v| relay_route_core::segment_is_safe(v))
            .map(|v| (k.to_string(), v))
    })
}

/// 现铸一个来处：16 位小写十六进制（时刻 · 进程 · 随机种子；只要一台机器上不撞）。
pub(crate) fn mint_origin() -> String {
    use std::hash::{BuildHasher, Hash, Hasher};
    let mut h = std::collections::hash_map::RandomState::new().build_hasher();
    crate::common::time::now().hash(&mut h);
    std::process::id().hash(&mut h);
    format!("{:016x}", h.finish())
}

/// 起 agent 前要 eval 的机器级环境串：这台设的 `CCM_ENV`，没设 ⇒ 内置默认（[`crate::ccm_line::argv::Defaults::ENV`]）。
///
/// 〔继承表〕机器级环境那一格。
pub(crate) fn machine_env(get: impl Fn(&str) -> Option<String>) -> String {
    get("CCM_ENV").unwrap_or_else(|| crate::ccm_line::argv::Defaults::ENV.to_string())
}

/// 问中转地址的那一只手：`(适配器 id, 这一发用哪个号, 上游是不是用户自己的)` ⇒ 不带钥匙的地址（`None` = 不注入）或拒的那一句。
pub(crate) type RelayAsk = fn(
    &str,
    &crate::accounts::upstream_select::endpoint::LaunchAccount,
    bool,
) -> Result<Option<String>, String>;

impl Env {
    /// 从真实进程取一份。**只读环境**，不写任何东西。
    ///
    /// 优先级：**环境变量 > 内置默认**（默认值住 [`crate::ccm_line::argv::Defaults`]，这里一个字面量都不许再写）。
    pub(crate) fn from_process(process_argv: &[String]) -> Self {
        // 家目录：`HOME`，没有再退 `USERPROFILE` —— 与本 crate 其余各处同一个口径
        //   （`exit_policy::policy_path` · `asset_catalog`）。只认 `HOME` 的话，Windows 上
        //   默认没有它 ⇒ 账号库落到当前盘的根上，找不到号还说「不在那个文件里」。
        let home = home_of(|k| std::env::var(k).ok());
        let get = |k: &str| std::env::var(k).ok().filter(|v| !v.is_empty());
        Env {
            pwd: std::env::current_dir()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default(),
            tmux: get("TMUX"),
            self_check: get(PANE_SELF_CHECK_ENV).is_some(),
            // 托管进程给里面那一跳设了 `CCM_HOST` ⇒ 已在托管终端里：直路（不再套一层）。
            can_host: get(crate::host::managed::run::HOST_ENV).is_none(),
            // 找上游那个变量名与它的继承值，要等**解析完 argv 知道是哪一家**才填得了 ⇒ 由 `mod.rs` 补。
            base_url_env: String::new(),
            inherited_base_url: None,
            ccm_env: machine_env(get),
            accts_manifest: accts_manifest_under(&home),
            // 继承值与载体名一样，要等**解析完 argv 知道是哪一家**才填得了 ⇒ 由 `mod.rs` 补。
            inherited_config_dir: None,
            account_env: String::new(),
            // 🔴 内层载荷要用**「我是被当作什么叫的」**那一段：这个进程**自己被怎么叫的**
            //   （`[argv0]`，`self_invocation`；分流不看名字）。
            //   远端 shim `exec <后端> ccm "$@"` 之后 `argv[0]` 是真身路径，`self_invocation` 连 `ccm` 那个词一起取
            //   ⇒ 不需要环境变量补词。
            // argv 由调用方交（`stream/entry.rs` 取的那一次），本模块不自己读。
            self_argv: super::self_invocation(process_argv),
            running_sessions: None,
            relay: Some(crate::accounts::upstream_select::endpoint::relay_for_exec),
            inherited_parent: parent_from(&|k| std::env::var(k).ok()),
            origin_token: Some(mint_origin()),
            relay_pass_key: crate::accounts::upstream_select::endpoint::pass_key_for_shell(
                std::path::Path::new(&home),
            ),
            answer_hook_entry: crate::platform::paths::installed_ccm_entry(),
            read_text: Some(|p| Ro::read_string_capped(p, SETTINGS_FILE_CAP).ok()),
            sideload: Some(sideload_here),
            home,
        }
    }
}

// 预览那一份单独一个 `impl` 块：`from_process` 那一块的判据按「块尾」截函数体（`plan_tests` 的家目录那条）。
impl Env {
    /// 别名预览用的那一份（帧命令 `ccm-print`）：
    /// **「从这台机器家目录里的一个新终端敲这条别名」**。问的人是常驻后端进程，而它的 cwd / 环境
    /// 不是那个终端的 ⇒ 这几格写死、而且写在这一处（判据 `ccm::tests` 的预览那几条逐格钉）：
    /// - `self_argv = ["ccm"]` —— 别名叫的就是 `ccm`（容器路内层要把自己再叫一次，叫法就是它）；
    /// - `pwd = home` · 不在 tmux 里 · 没有继承来的中转地址 / 启动号 / 令牌；
    /// - 账号目录变量不继承（由 [`super::plan_of`] 的 `inherit_account = false` 管）。
    ///
    /// 其余照这台机器的真值：账号库 manifest · `CCM_ENV`。
    pub fn for_preview() -> Self {
        Self::for_preview_in(home_of(|k| std::env::var(k).ok()))
    }

    /// 同 [`Env::for_preview`]，家目录由调用方给（设置窗那一页按这台文件管理面答的家目录算）。
    pub fn for_preview_in(home: String) -> Self {
        let get = |k: &str| std::env::var(k).ok().filter(|v| !v.is_empty());
        Env {
            pwd: home.clone(),
            tmux: None,
            self_check: false,
            // 新终端里敲：不在 tmux、不在托管终端里 ⇒ 落托管（同真跑）。
            can_host: true,
            base_url_env: String::new(),
            inherited_base_url: None,
            ccm_env: machine_env(get),
            accts_manifest: accts_manifest_under(&home),
            inherited_config_dir: None,
            account_env: String::new(),
            self_argv: vec![super::SUBCOMMAND_WORD.to_string()],
            running_sessions: None,
            relay: Some(crate::accounts::upstream_select::endpoint::relay_for_preview),
            // 预览是「从这台家目录里的一个新终端敲」：不在任何会话里，也不铸来处（每次预览都一样）。
            inherited_parent: None,
            origin_token: None,
            // 与真跑同一行：`/t/` 照字面带只许直通那一把（它只开得了 `/t/`）；全权那一把照旧写成读钥匙文件。
            relay_pass_key: crate::accounts::upstream_select::endpoint::pass_key_for_shell(
                std::path::Path::new(&home),
            ),
            // 预览不挂答题 hook：那一串设置是起会话那一下的事，别名本身不带它（真跑那一份照常挂）。
            answer_hook_entry: None,
            read_text: None,
            sideload: None,
            home,
        }
    }
}

/// 从一组环境变量里取家目录（规则住 `platform::paths::home_dir_from`）；没有 ⇒ 空串（照旧）。
/// 纯函数（`get` 注入），两平台的判定表在 Linux 上直接测。
pub fn home_of(get: impl Fn(&str) -> Option<String>) -> String {
    crate::platform::paths::home_dir_from(&|k| get(k).map(Into::into))
        .map(|h| h.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// 这台的账号库清单：`<家目录>/.cc-monitor/accounts/accounts.json`（两段都住契约 crate；没有另指位置的变量）。
pub fn accts_manifest_under(home: &str) -> String {
    under_home(
        home,
        &format!(
            "{}/{}",
            relay_route_core::ACCOUNTS_DIR_REL,
            relay_route_core::ACCOUNTS_MANIFEST_NAME
        ),
    )
}

/// 家目录下一个 `/` 分隔的相对路径 → 本平台的完整路径（逐段 `join`，Windows 上不再 `\` 与 `/` 混拼）。
/// 家目录为空时照旧拼成 `/<rel>`（与改之前逐字相同 —— 这一格不是这里要改的行为）。
pub fn under_home(home: &str, rel: &str) -> String {
    if home.is_empty() {
        return format!("/{rel}");
    }
    crate::platform::paths::join_under(home, rel)
}

use crate::control::launch_account::{pick, AccountAsk, Settled};
pub(crate) use cmd_wire::accounts::AccountsList;

/// 报「不可用」必须同时告诉用户有哪些可用（成品里的号名）。⚠ 无尾空格。
/// 🔴 清单**在却读不懂**（读不动 · 坏 JSON · 版本不认）⇒ 照实说，不谎称「没有账号库」（Win11 上一份带
/// UTF-8 BOM 的清单，用 `unwrap_or_default()` 读会被整张吃掉、报成「(无账号库)」—— BOM 由账号库那一个读口剥掉）。
pub fn names(list: &AccountsList, manifest_path: &str) -> String {
    if !list.meta.enabled {
        return if std::path::Path::new(manifest_path).exists() {
            copy_text("bePlan.names.unreadable", &[])
        } else {
            copy_text("bePlan.names.none", &[])
        };
    }
    if list.accounts.is_empty() {
        return copy_text("bePlan.names.none", &[]);
    }
    list.accounts
        .iter()
        .map(|a| a.scanned.name.as_str())
        .collect::<Vec<_>>()
        .join(" ")
}

/// tmux 那一形的自检照窗格里那一跳的环境算：窗格里有 `TMUX` ⇒ 那一跳算的是直路那一份计划（启动器在计划里就解析）。
/// 自检交给计划的是「在 tmux 里」这一事实本身（[`PANE_SELF_CHECK_ENV`]），不拿一个值去冒充 `TMUX`：tmux 客户端照 `TMUX` 找 server，
/// 冒充的值不是 socket 路径。直路的计划不认 socket、用不着会话名单 ⇒ 自检那一趟不问 tmux server（[`Env::self_check`]）。
pub const SELF_CHECK_IN_PANE: &str = "CCM_PANE_SELF_CHECK=1 ";

/// 自检那一格的名字（[`SELF_CHECK_IN_PANE`] 设它；空值当没有）。
pub const PANE_SELF_CHECK_ENV: &str = "CCM_PANE_SELF_CHECK";

impl Env {
    /// 在不在 tmux 里（计划只认这一格，不认 socket）：`$TMUX` 有值，或这一趟是 tmux 那一形的自检。
    pub fn in_tmux(&self) -> bool {
        self.tmux.is_some() || self.self_check
    }
}

/// 一趟要干的事。`--ccm-print` 与真跑读的是**同一个**。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Plan {
    /// 不起 agent，直接接回一个既有会话。
    Attach { name: String },
    /// 建一个容器（tmux 会话 · 托管终端），把「同一条命令去掉 `--ccm-tmux` / `--hosted`」送进去。
    Container(Container),
    /// 在**本进程**里设好环境、`cd`、然后 `exec`。
    Direct(Direct),
    /// resume 的那条会话已经在终端 `name` 里跑（tmux 会话 · 托管终端）⇒ 接上它，不另起一份（`--detach` ⇒ 只报名字）。
    Rejoin {
        name: String,
        sid: String,
        detach: bool,
        host: TerminalHost,
    },
}

pub(crate) use crate::wire::TerminalHost;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Container {
    /// 住在哪种宿主里。
    pub host: TerminalHost,
    /// tmux 会话名 · 托管终端的名字（也是它的句柄）。
    pub name: String,
    /// 托管那一形：起托管进程的那一整串（[`host_run_argv`]；脱离起）。tmux 那一形空。
    pub host_run: Vec<String>,
    /// 托管那一形：脱离起托管进程那一下从环境里摘掉的变量（会话血缘：外层没有父 ⇒ 全摘；有 ⇒ 只留那一个）。
    /// tmux 那一形这件事写在载荷里（`env -u …`），这一格空。
    pub unset: Vec<String>,
    pub(crate) cwd: String,
    pub(crate) agent: String,
    pub ccm_sid: String,
    pub(crate) size: Option<(String, String)>,
    pub detach: bool,
    /// 送进容器的那条内层命令（已经是一整条 POSIX 命令串）。
    pub payload: String,
    /// **同一条内层命令的自检形**：同一段 `export` 前缀、同一个入口、同一串参数，
    /// 只在 `--` 前多一个 `--ccm-print`。收尾那段在登记之前先跑它（见 [`render_container_tail`]）。
    pub self_check: String,
    /// 同一个自检的 argv（内层那一串 ＋ `--ccm-print`）：托管那一形真跑时原生起它（不经 shell，环境与托管进程同一份）。
    pub self_check_argv: Vec<String>,
    /// 送进容器那条内层命令到 `--` 之前的词（入口 ＋ 交给 agent 的那几个；不带 `env` / `export` 前缀）：只给短形（[`render_short`]）用，
    /// 载荷由同一串接上 `--` 与 ccm 自己的参数渲出。
    pub(crate) inner_head: Vec<String>,
    // ★★ **撞名退让在 [`build`] 里做**（用户定：「**不就是先校验冲突然后取名吗? 搞个 hash 表**不就好了」）：
    //   它拿一份 [`TakenNames`]（那张 hash 表，来源只有会话快照一处），
    //   **算完就把最终名钉进 `Container::name`**。于是：
    //   ① 「产名」与「避让」不是两个人干的两件事（前端 `mintTmuxName` 那条纪律的对侧）；
    //   ② `--ccm-print` 与真跑吐的是**同一个名字** —— 平价预言机在名字这一维上也是平的；
    //   ③ `--ccm-print` 的「纯」口径是**相对于快照**：同一份快照 ＋ 同一份输入 ⇒ 同一份输出。
    //
    // 三条取名路的态度**仍然不一样，别合并**（判据见
    // `only_two_of_the_three_naming_paths_step_aside_on_a_collision`）：
    // - 显式 `--tmux=<名>` ⇒ **不退让**（调用方说的就是要这个名；撞了走 `C14` 响亮失败）；
    // - `--tmux-base=<基名>` ⇒ **退让**（`C15`：调用方随后要读回真名字）；
    // - 不给名、从 cwd 派生 ⇒ **退让**（幂等接回同一目录的会话，撞了说明有别人占了）。
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Direct {
    /// `CCM_ENV`（机器级 env，先于会话级）。
    pub(crate) ccm_env: String,
    /// 账号维度的载体（环境变量名）—— 见 [`Env::account_env`]。
    pub(crate) account_env: String,
    /// 要 export 的账号目录。
    pub config_dir: String,
    /// 那个目录是账号库里哪个号（说不出 ⇒ 空串：`--account-dir` / 继承来的）。
    pub(crate) account_name: String,
    /// `--base`：显式 `unset CLAUDE_CONFIG_DIR`。
    pub(crate) unset_config_dir: bool,
    /// 要 unset 的：那一家的嵌套标记（claude 四个 / codex 零个）∪ 各家「我是哪个会话」的变量（[`nested_of`]）。
    pub nested: Vec<String>,
    pub cwd: String,
    /// 最终 exec 的 argv：启动器 ＋ 原样透传（`--resume` 这类 ccm 不吃，照写交出去）。
    pub argv: Vec<String>,
    /// 要注入的中转地址（不带钥匙；钥匙在 exec 那一刻从那台的钥匙文件读）。`None` = 不注入。
    pub relay: Option<String>,
    /// 这一家怎么指到中转（适配层那一格的计划形）。
    pub relay_via: RelayVia,
    /// 随中转地址一起设的环境（名, 值；适配层 `DefaultUpstream.relay_env`）：注入地址时（`relay` 有）才设，打印那一行与真跑那一趟都按它。
    pub relay_env: Vec<(String, String)>,
    /// 注入了中转、而这条会话的上游是用户自己的（环境里继承来的上游地址不是我们那一形）⇒ 那个地址原样：
    /// 进这条会话的上游描述，中转按来处把请求直通过去。`None` ＝ 上游不是用户的，或没注入中转（那时环境里那一条原样不动）。
    pub own_upstream: Option<String>,
    /// 环境里继承来的是我们的中转那一形（别的号的），而这一发不注入 ⇒ exec 之前清掉它。
    pub clears_inherited_relay: bool,
    /// 非得经 shell 那一趟、`/t/` 地址照字面插的只许直通那一把（[`Env::relay_pass_key`]；不注入 ⇒ `None`）。
    pub(crate) relay_pass_key: Option<String>,
    /// 答题 hook 挂不上（用户自己给的会话级设置并不进去 · 自定义启动器起的）⇒ 起的进程环境里带 [`crate::agents::asks::ANSWER_HOOK_OFF_ENV`]
    /// （值是原因的短码），会话事实据此写明为什么在这里答不了。
    pub answer_hook_off: Option<crate::agents::asks::HookOff>,
    /// 换了启动器那一支：答题插件经环境交下去 ⇒ `(变量名, 整串值)`（用户已有的在前、我们的目录接在后面）。
    pub plugin_dirs: Option<(String, String)>,
    /// 同上那一支：插件目录里的每一份（相对路径, 内容）；exec 之前经写自家状态的句柄写进 [`crate::agents::asks::plugin_dir_under`]。
    pub(crate) plugin_files: Vec<(&'static str, String)>,
    /// `--tab`（与写它的别名）：最后那一跳记进便条，观测侧认便条时落进启动台账。
    pub(crate) tab: Option<crate::control::launch_account::LaunchTab>,
    /// 这一发经哪条别名起的：最后那一跳记进便条，观测侧认便条时落进启动台账（补足一组时按它数）。`None` ⇒ 不经别名。
    pub(crate) alias: Option<String>,
    /// `--no-rotation`：exec 之前在轮换账本里记一条「不轮换」。`None` ⇒ 不记。
    pub(crate) no_rotation: Option<NoRotation>,
}

/// exec 之前要记成「不轮换」的那个会话：哪一家（线上的 kind）· 会话ID（也在 [`Direct::argv`] 里交给了那一家）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NoRotation {
    pub(crate) agent: String,
    pub(crate) sid: String,
}

/// 一家怎么指到中转，落到计划里的样子（[`crate::agents::Inject`] 的两形；没登记 ⇒ [`RelayVia::None`]）。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum RelayVia {
    /// 插上钥匙的地址进这个环境变量（继承来的我们那一形不注入时清掉它）。
    Env(String),
    /// 不带钥匙的地址已经拼进 argv；exec 那一刻在那个词里插上钥匙（[`split_at_key`]）。
    Args,
    #[default]
    None,
}

/// POSIX argv 元素的**最省引号**写法：能裸写就裸写。
///
/// 与 `--ccm-print` 的可读性直接相关：`exec claude --resume abc` 比
/// `exec 'claude' '--resume' 'abc'` 好读，而两者语义相同。
pub fn qarg(s: &str) -> String {
    if s.is_empty()
        || !s
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "_@%+=:,./-".contains(c))
    {
        sq(s)
    } else {
        s.to_string()
    }
}

/// tmux 会话名里**一段**的净化（不含 `-cc` 后缀）：取末段路径（`/` 与 Windows 的 `\` 都算分隔）→ 非 `[A-Za-z0-9_-]` 换 `-` → 折叠 → 截 32 → 剥首尾 `-`。
/// 结果可能是空串（输入全是分隔符 / 根目录），调用方给兜底。
///
/// 全仓**唯一一份**：前端那份（`shell-quote.ts::tmuxNameSegment` · `remote-launch.ts::deriveTmuxName`）删了，
/// 起会话要名字由这台铸（[`super::terminal_name_mint_with`]）。
fn name_segment(raw: &str) -> String {
    let sep = |c: char| c == '/' || c == '\\';
    let trimmed = raw.trim_end_matches(sep);
    let base = trimmed.rsplit(sep).next().unwrap_or("");
    let mut s = String::new();
    let mut last_dash = false;
    for c in base.chars() {
        let c = if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
            c
        } else {
            '-'
        };
        if c == '-' {
            if last_dash {
                continue;
            }
            last_dash = true;
        } else {
            last_dash = false;
        }
        s.push(c);
    }
    let s: String = s.chars().take(32).collect();
    s.trim_matches('-').to_string()
}

/// 从一个目录派生 tmux 会话名（**基名**，还没避让）：`<段>-cc`；段为空 ⇒ `session-cc`。
///
/// 同规则 = 终端里敲 `ccm` 与 app「开新 Claude」在同一目录造出**同一个基名**。
/// 界面那份 `deriveTmuxName` 删了（原先两边逐字同规则、`ccm-cli.test.sh` 跨语言对拍钉着）：规则只剩这一份。
pub fn derive_tmux_name(cwd: &str) -> String {
    let s = name_segment(cwd);
    if s.is_empty() {
        "session-cc".to_string()
    } else {
        format!("{s}-cc")
    }
}

/// 分叉出来那条会话的**基名**：`<源名去掉末尾 -cc 再净化>-fork-cc`；净化后为空 ⇒ `session-fork-cc`。
///
/// 源是源会话所在的 tmux 名；源会话已退出、没有 tmux 名可继承时调用方交它的 cwd（同一个净化器 ⇒ 产不出非法名，
/// Phase G 审计抓过的那一下：`/home/pi/proj-fork-cc`）。**必须与源名不同**，否则 `ccm` 会把新会话接进原会话那个窗口。
pub fn fork_tmux_base(source: &str) -> String {
    let seg = name_segment(source.strip_suffix("-cc").unwrap_or(source));
    let seg = if seg.is_empty() {
        "session"
    } else {
        seg.as_str()
    };
    format!("{seg}-fork-cc")
}

// 撞名退不退让只问 `ccm_line::tmux_steps_aside`（别名页那句说明照的是同一处）。
use crate::ccm_line::{tmux_steps_aside, TmuxNaming};

/// 给一个基名、一份**那张快照**里的已占用名 ⇒ 最终名（撞了往后排）。
/// 起会话铸名（[`super::terminal_name_mint_with`]）与 [`build`] 走同一个 [`next_free_name`]；`taken` 只收 [`TakenNames`]（造不出第二份）。
pub fn mint_tmux_name(base: &str, taken: &TakenNames) -> String {
    next_free_name(base, taken.as_slice())
}

/// 基名撞了就退让：`<基名>` → `<基名>-2` → `<基名>-3` … 取第一个没被占的。
///
/// 纯函数（已占用的名字由调用方给）—— 这样「退让规则」测得了。
///
/// **谁给那份 `taken`，今天只有一个答案**：[`build`] 从
/// [`TakenNames`] 里拿，而 `TakenNames` 的字段是 `common::session_snapshot` 模块私有的
/// ⇒ 「另起一份名字集合」在类型层面就造不出来。`--ccm-print` 与真跑用的是同一份。
pub fn next_free_name(base: &str, taken: &[String]) -> String {
    if !taken.iter().any(|t| t == base) {
        return base.to_string();
    }
    let mut k = 2usize;
    loop {
        let cand = format!("{base}-{k}");
        if !taken.contains(&cand) {
            return cand;
        }
        k += 1;
    }
}

/// 会话名的形状校验 —— 显式名与基名都过这里（`ccm` 要**新建**的会话名）。
///
/// **规则只有一份**，住共享 crate（`crate::control::gate_rules::new_tmux_name_issue`：非空 · 不以 `-` 开头 ·
/// 无 `*?.:=` · 无控制符与视觉欺骗字符 · ≤128）；monitor 的载荷外层与 `ccm …` 调用行调的是同一个函数。本函数只剩「说哪一句」。
/// 这里原来自己写了一份（自称「唯一一份」，而 monitor 与界面各还有一份、规则各不相同）；比那一份多出来的两格
/// （欺骗字符 · 超长）是三份取交集时从 monitor 载荷那一份与界面那一份带进来的。
pub fn validate_tmux_name(n: &str) -> Result<(), Die> {
    // 说哪一句住 `launch::new_tmux_name_said`（后端 `launch` 新建那一支也用它）。
    match crate::control::launch::new_tmux_name_said(n) {
        None => Ok(()),
        Some(said) => Err(Die(said)),
    }
}

/// 不给 `--cwd` 时的落点。**它是恒等**：调用方站在哪儿，会话就起在哪儿。
///
/// 🔴 **不替用户挑目录** —— 新会话起在调用者的 `cwd`：
/// 不跳某个「工作区」目录，也不跳所在仓的父目录。
///
/// 〔用户定，逐字〕「`cc` 默认就起会话就行，**跳目录是我自己的设置，不要搞进 app**。」
/// 判据：**把这个行为去掉，用户还做不做得到同一件事**？——写 `--cwd <目录>` 照样
/// 起得了会话 ⇒ 它是**偏好**，不是机制，不进 app。默认的形状：
/// **诚实的默认 = 恒等 / 不作为 / 沿用调用者已有状态** —— `cwd` 本来就是进程的属性，
/// 不是从一张表里编出来的。★ **省的是书写，不是语义。**
///
/// ⚠ **`resume`/`attach` 那一支与 `new` 同值**：
/// 那两个动作的目标目录由 sid / 会话决定、调用方已经定位好了，再解析一次会把工作目录
/// 换掉 ⇒ claude 按 `projects/<enc(cwd)>/<sid>.jsonl` **找不到会话**（实测踩过）。
/// 🔴 **谁要是往这里加一档非恒等的分支，它必须只对「新起会话」生效**（透传里没有 `--resume` / `--continue`）——
/// 不留那个 `if`，是因为恒等之下它是死代码，而不是因为那条约束不在了。
///
/// 相对的 `--cwd`（`.` / `../x`）按调用方当前目录补成绝对、按字面折掉 `.` / `..`，之后再过 §47 形式判定；
/// 已是绝对的原样交给判定（带 `..` 照旧拒）。
///
/// `--cwd` 与 `--cwd-if` 两边同一种写法：打头的 `~` 先换成家目录（[`expand_home`]），相对的再按当前目录补成绝对。
/// `--cwd-if <在哪> <进哪>` 先比：按写的顺序，**正好**站在「在哪」（不含子目录）的第一条 ⇒ 进「进哪」；
/// 两边先展开 `~`、解开链接再比（大小写按这台的文件系统，`platform::paths::path_key`）。都没对上才轮到 `--cwd` / 当前目录。
///
/// 〔继承表〕工作目录那一格。
pub fn resolve_cwd(o: &Opts, env: &Env) -> String {
    cwd_and_why(o, env).0
}

/// 工作目录落在哪条上（`--ccm-print` 的「来自」照它写）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CwdWhy {
    /// `--cwd-if` 那一条对上了。
    Rule,
    /// `--cwd` 给的。
    Given,
    /// 调用方的当前目录。
    Caller,
}

/// [`resolve_cwd`] 的本体：落到哪个目录 ＋ 落在哪条上。
pub(crate) fn cwd_and_why(o: &Opts, env: &Env) -> (String, CwdWhy) {
    let here = same_dir_key(&env.pwd);
    let hit = o
        .cwd_if
        .iter()
        .find(|(at, _)| !env.pwd.is_empty() && same_dir_key(&expand_home(at, &env.home)) == here);
    if let Some((_, to)) = hit {
        return (
            absolute_from(&expand_home(to, &env.home), &env.pwd),
            CwdWhy::Rule,
        );
    }
    match &o.cwd_spec {
        CwdSpec::Explicit(d) => (
            absolute_from(&expand_home(d, &env.home), &env.pwd),
            CwdWhy::Given,
        ),
        CwdSpec::Auto => (env.pwd.clone(), CwdWhy::Caller),
    }
}

/// 相对的按 `pwd` 补成绝对（按字面折 `.` / `..`）；已是绝对的原样（带 `..` 由 §47 判定拒）。
fn absolute_from(d: &str, pwd: &str) -> String {
    if !std::path::Path::new(d).is_absolute() && !pwd.is_empty() {
        lexical_join(pwd, d)
    } else {
        d.to_string()
    }
}

/// 打头的 `~` / `~/…` 换成家目录（家目录说不出 ⇒ 原样）。
pub(crate) fn expand_home(p: &str, home: &str) -> String {
    crate::platform::paths::expand_tilde_from(
        p,
        (!home.is_empty()).then(|| std::path::Path::new(home)),
    )
}

/// 比「是不是同一个目录」用的键：解开链接（解不开就按原样），再按这台的大小写规矩折。
fn same_dir_key(p: &str) -> std::path::PathBuf {
    let raw = std::path::Path::new(p);
    let real = Ro::canonicalize(raw).unwrap_or_else(|_| raw.to_path_buf());
    crate::platform::paths::path_key(&real)
}

/// `base` ＋ 相对的 `rel`，按字面折掉 `.` / `..`（不碰盘、不解符号链接；`..` 到根为止）。
fn lexical_join(base: &str, rel: &str) -> String {
    use std::path::{Component, PathBuf};
    let mut out = PathBuf::new();
    for c in std::path::Path::new(base).join(rel).components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                if out.parent().is_some() {
                    out.pop();
                }
            }
            other => out.push(other.as_os_str()),
        }
    }
    out.to_string_lossy().into_owned()
}

/// 启动器按空格拆成词（`ccr code` ⇒ `ccr` `code`），打头的 `~/` 换成家目录 —— 与载荷那条路交给 shell 拆词、展开 `~` 同一个结果
/// （拆词）。字符白名单在 [`free_text_gate`] 对整串判过（`launcher_refused_char`）。
fn launcher_words(launcher: &str, home: &str) -> Vec<String> {
    let mut words: Vec<String> = launcher
        .split(' ')
        .filter(|w| !w.is_empty())
        .map(str::to_string)
        .collect();
    if let Some(first) = words.first_mut() {
        if let (Some(rest), false) = (
            first.strip_prefix(shell_quote_core::LAUNCHER_HOME_PREFIX),
            home.is_empty(),
        ) {
            *first = format!("{}/{rest}", home.trim_end_matches('/'));
        }
    }
    words
}

/// 账号那一格：命令行这几形先定「要哪个号」，**判**只经 `launch_account::pick`（界面起会话同一条：能不能用照成品的 `selectable`）。
///
/// - 显式 `--account X` ⇒ 必须选得了，否则**中止**（显式选号绝不静默降级到别的号；没登录 · 目录不在 · 逃生口模式同界面那一侧一样拒）。
/// - 显式 `--base` ⇒ 不注入（issue #75 逃生口）。直接给了目录（`--account-dir`，说不出名字的那一形）⇒ 就用它，不查账号库。
/// - 都不传 ⇒ **只在调用方没有已选定账号时**才跟随这台的默认号（选得了才用，用不了 ⇒ 不指定号，同界面「跟随」）。
///
/// ✅ 裁定逐字：「把调用方选中的号静默换掉 / **不要这么做** / 不是有选默认账号吗? **就用那个**」⇒ 两支都是裁定的一部分：
///   · **继承那一支**（`CLAUDE_CONFIG_DIR` 非空 ⇒ 保留，不覆盖）= 「不要静默换掉」。
///     🔴 **理由不许删**：这道闸（`-z "$CLAUDE_CONFIG_DIR"`）挡的是一次**真机复现过的静默换号**。
///   · **裸终端那一支**（都没给 ⇒ 跟随默认号）= 「就用那个」。
///
/// `accounts` 只在真要判的时候读（这台的账号清单成品，由入口交：control 不读观测层）。〔继承表〕账号那一格。
pub fn account_for(
    o: &Opts,
    env: &Env,
    accounts: &dyn Fn() -> AccountsList,
) -> Result<(String, String), Die> {
    let none = || Ok((String::new(), String::new()));
    // 这一家没有账号这一维（载体名空）⇒ 不选号、不落默认号（选号的那几形 `argv::validate` 已经拒了）。
    if env.account_env.is_empty() {
        return none();
    }
    let asked = if !o.account.is_empty() {
        AccountAsk::Named {
            name: o.account.clone(),
        }
    } else if !o.account_dir.is_empty() {
        return Ok((o.account_dir.clone(), String::new()));
    } else if o.use_base
        || env
            .inherited_config_dir
            .as_deref()
            .is_some_and(|v| !v.is_empty())
    {
        // 调用方已经选好号 ⇒ **尊重它**，不覆盖（真机复现过的静默换号）。
        return none();
    } else {
        AccountAsk::Follow
    };
    let list = accounts();
    match pick(&asked, &list, None) {
        Ok(Settled::Account(a)) => Ok((a.config_dir, a.name)),
        Ok(Settled::Base | Settled::Unsaid) => none(),
        Err(u) => Err(Die(copy_text(
            "bePlan.resolveAccount.unavailable",
            &[
                ("account", &u.requested),
                ("manifestPath", &env.accts_manifest.to_string()),
                ("names", &names(&list, &env.accts_manifest)),
            ],
        ))),
    }
}

/// 〔`INVARIANTS §47` ②〕**自由文本**那几格拼进 shell（`--ccm-print` 那一串 · pane 里键入的载荷 ·
/// 收尾那几段 `sh -c`）之前的放行判定：
///
/// | 格 | 形式（按本机语境） | 拒绝集 |
/// |---|---|---|
/// | 工作目录（`--cwd` / 当前目录） | 绝对路径（`Path::is_absolute`，Windows 上认 `C:\` 那一形）· 没有 `..` 段 | NUL / CR / LF |
/// | 透传给 agent 的参数 · 登记备注（可以跨行：多行初始任务） | — | NUL / CR（`shell_quote_core::arg_text_ok`） |
/// | 继承来的账号目录变量 / 地址变量（`CLAUDE_CONFIG_DIR` / `ANTHROPIC_BASE_URL` 那一类） | — | NUL / CR / LF |
///
/// 〔`INVARIANTS §47` ③〕**启动器不在上表**：它是命令片段（带参数 · alias · 路径），不是自由文本 ——
/// 过全仓那一张白名单 `shell_quote_core::launcher_refused_char`（与 monitor 本机 `history.rs` · 远端载荷 `payload.rs` 同一条；
/// 不只是拒 NUL / CR / LF）。空 = 没给（下面用这个 agent 的默认启动器）。
///
/// **不拒 shell 元字符**（`Bob's` · `(2019)` 照放，交给唯一的 quote）。拒绝集住 `shell_quote_core::free_text_ok` / `arg_text_ok`。
/// ⚠ 模型名与 `--ccm-sid` **不在这里**：它们的规则在判定的唯一住址里统一定。
/// ⚠ 继承来的那两个只在它们真会被拼进去的时候才判（容器路把它们显式化进载荷，[`inherited_gate`]）：
///   环境里一个用不上的怪值不该挡住起会话（拒过头）。
fn free_text_gate(cwd: &str, o: &Opts) -> Result<(), Die> {
    let p = std::path::Path::new(cwd);
    if !p.is_absolute()
        || p.components().any(|c| c == std::path::Component::ParentDir)
        || !shell_quote_core::free_text_ok(cwd)
    {
        return Err(refuse(flag::CWD, cwd));
    }
    // ccm 跑在目标机上：启动器照这一台的平台认（同上面工作目录照 `Path::is_absolute`）。
    if let Some(c) = shell_quote_core::launcher_refused_char(
        &o.launcher,
        crate::platform::shell::LOCAL_TERMINAL_IS_POWERSHELL,
    ) {
        return Err(Die(copy_text(
            "bePlan.build.launcherRefused",
            &[
                ("what", flag::LAUNCHER),
                ("value", &format!("{:?}", o.launcher)),
                ("c", &format!("{c:?}")),
            ],
        )));
    }
    if let Some(a) = o
        .passthru
        .iter()
        .find(|a| !shell_quote_core::arg_text_ok(a))
    {
        return Err(refuse(flag::END, a));
    }
    Ok(())
}

/// 容器路要把继承来的那两个显式化进载荷（tmux 边界会吃掉它们）⇒ 那一刻才判（见 [`free_text_gate`]）。
fn inherited_gate(env: &Env) -> Result<(), Die> {
    for (what, v) in [
        (
            env.account_env.as_str(),
            env.inherited_config_dir.as_deref(),
        ),
        (env.base_url_env.as_str(), user_base_url(env)),
    ] {
        if let Some(v) = v.filter(|v| !shell_quote_core::free_text_ok(v)) {
            return Err(refuse(what, v));
        }
    }
    Ok(())
}

/// 起托管进程的那一串：`<自己> -- --host-run <名> name=<名> cwd=<目录> agent=<那一家> client=<谁建的> [size=宽x高] -- <里面那一跳>`。
/// 句柄就是名字（调用方已避让过这台的 tmux 会话名与托管终端）；托管进程直接起里面那一跳（`inner`）、不经 shell：
/// 托管进程是脱离起的，环境原样往下传，用不着 tmux 那一形为跨 tmux 边界写的 `export`（里面那一跳一退、托管终端收尾）。
/// 起会话那一形：`inner` 与 tmux 那一形键进窗格的是同一串词，`client` 是 `gate_rules::CLIENT_TERMINAL`（同 tmux 那一形 ccm 起的会话）；
/// 开空 shell 那一形（`ccm::open_hosted_shell`）：`agent` 空、`client` 是建它的那个前端。
pub(crate) fn host_run_argv(
    self_argv: &[String],
    name: &str,
    cwd: &str,
    agent: &str,
    client: &str,
    size: Option<&(String, String)>,
    inner: &[String],
) -> Vec<String> {
    let mut v: Vec<String> = self_argv.to_vec();
    v.extend([
        flag::END.to_string(),
        flag::HOST_RUN.to_string(),
        name.to_string(),
        format!("name={name}"),
        format!("cwd={cwd}"),
        format!("agent={agent}"),
        format!("client={client}"),
    ]);
    if let Some((w, h)) = size {
        v.push(format!("size={w}x{h}"));
    }
    v.push(flag::END.to_string());
    v.extend(inner.iter().cloned());
    v
}

/// 外层没有父时摘掉的那几个会话血缘变量（`env -u` 那一形的词：tmux 窗格里那一跳 · 托管那一形的等价写法同一份）。
fn unset_prefix(vars: &[impl AsRef<str>]) -> String {
    if vars.is_empty() {
        return String::new();
    }
    let us: Vec<String> = vars.iter().map(|k| format!("-u {}", k.as_ref())).collect();
    format!("env {} ", us.join(" "))
}

/// 这一发进哪种容器（`None` ⇒ 直路）：`--hosted` ⇒ 托管；要 tmux 容器（`use_tmux`：已在 tmux 里且没给名的那一形已摘掉）⇒ tmux；
/// 都没说、不在 tmux 窗格里、这台能托管（[`Env::can_host`]）⇒ 托管。在 tmux 窗格里敲 · 已在托管终端里 · 不能托管 ⇒ 直路。
fn container_host(o: &Opts, env: &Env, use_tmux: bool) -> Option<TerminalHost> {
    if o.hosted {
        Some(TerminalHost::Managed)
    } else if use_tmux {
        Some(TerminalHost::Tmux)
    } else if !o.use_tmux && !env.in_tmux() && env.can_host {
        Some(TerminalHost::Managed)
    } else {
        None
    }
}

/// 自由文本那几格判不过时的那句话（哪一格 · 原值）。
fn refuse(what: &str, v: &str) -> Die {
    Die(copy_text(
        "bePlan.build.freeTextRefused",
        &[("what", what), ("value", &format!("{v:?}"))],
    ))
}

/// 从 [`Opts`] ＋ 外界 ⇒ [`Plan`]。**这是计划面的唯一入口。**
///
/// `taken` = 那一刻**已被占用的会话名**（铸名避让那张 hash 表），
/// 来源只有 `common::session_snapshot` 一处；`None` = **问不到**
/// （tmux 起不来 / 没装）⇒ **不退让**，与那份已删的 bash `ccm` 那句
/// `tmux has-session … 2>/dev/null`（问不出来当没占）同义。真撞上了还有
/// `created:false` ⇒ rc=3 那条响亮失败兜底。
///
/// 🔴 **本函数相对 `taken` 是纯的**：同一份快照 ＋ 同一份 `o`/`env`/`accounts`
/// ⇒ 同一份 `Plan`（`--ccm-print` 的「纯」今天就是这个口径）。
pub fn build(
    o: &Opts,
    env: &Env,
    accounts: &dyn Fn() -> AccountsList,
    taken: Option<&TakenNames>,
) -> Result<Plan, Die> {
    build_among(crate::agents::REGISTRY, o, env, accounts, taken)
}

/// 开口那句（`--say`）交不交、怎么交：这一发落在 tmux 里（`--ccm-tmux` 那一路，连已在 tmux 里就地起）、起新会话、
/// 透传里还没有那一家自己的 `--`（这一趟当场给了第一句 ⇒ 它盖过别名的）⇒ 接在透传末尾成 `-- <那句>`（`None` ＝ 照原样）；
/// 窗格里那一趟拿到的就是普通透传。不进 tmux 的那一发起在当前终端里，开口留给人，不交。
/// 那一家起的时候不收第一句（`LaunchFace::first_prompt_arg`）⇒ 说清、不起。
fn say_into(
    o: &Opts,
    env: &Env,
    face: Option<crate::agents::LaunchFace>,
) -> Result<Option<Vec<String>>, Die> {
    // 落在容器里（tmux · 托管）或已在 tmux 里就地起；直路那一发起在当前终端里。
    let contained = o.use_tmux || container_host(o, env, false).is_some();
    if o.say.is_empty()
        || !contained
        || o.resumes.is_some()
        || o.passthru.iter().any(|a| a == flag::END)
    {
        return Ok(None);
    }
    if !face.is_some_and(|f| f.first_prompt_arg) {
        return Err(Die(copy_text(
            "bePlan.build.sayNotTaken",
            &[("agent", &o.agent)],
        )));
    }
    let mut passthru = o.passthru.clone();
    passthru.extend([flag::END.to_string(), o.say.clone()]);
    Ok(Some(passthru))
}

/// [`build`] 的内核：按哪一家起会有什么不同，**只问** `registry` 里这一家的起会话事实（`agents::LaunchFace`），
/// 不认名字。生产走 `agents::REGISTRY`；判据喂含夹具家的合成注册表。认不出这一家 ⇒ 每一格都按「没有」算。
pub fn build_among(
    registry: &[crate::agents::Adapter],
    o: &Opts,
    env: &Env,
    accounts: &dyn Fn() -> AccountsList,
    taken: Option<&TakenNames>,
) -> Result<Plan, Die> {
    let face = crate::agents::launch_face_among(registry, &o.agent);
    // ── attach：不起 agent，早于容器逻辑就定了 ──────────────────────────
    if !o.attach_name.is_empty() {
        // `ccm --attach foo --tmux --detach` 不许**静默吞掉** `--detach` 照样 attach：
        // 静默忽略正是本工作区反复消灭的病。
        if o.detach {
            return Err(Die(copy_text("bePlan.build.attachDetach", &[])));
        }
        if !o.tmux_size.is_empty() {
            return Err(Die(copy_text("bePlan.build.attachSize", &[])));
        }
        return Ok(Plan::Attach {
            name: o.attach_name.clone(),
        });
    }

    // 开口那句落成那一家的第一句：之后两路（容器 · 直起）读的都是接好的透传。
    let said: Opts;
    let o = match say_into(o, env, face)? {
        Some(passthru) => {
            said = Opts {
                passthru,
                ..o.clone()
            };
            &said
        }
        None => o,
    };

    let cwd = resolve_cwd(o, env);
    free_text_gate(&cwd, o)?;
    let (config_dir, account) = account_for(o, env, accounts)?;
    // 〔§47〕账号配置目录（manifest 里来的）是本仓自管的路径，该走全表（`acct_core::config_dir_ok`，全仓唯一一份）。
    //   空串 = 账号 0 / 继承，不注入、不判。
    if !config_dir.is_empty() && !acct_core::config_dir_ok(&config_dir) {
        return Err(refuse(&env.account_env, &config_dir));
    }
    // ── resume 先查是否已在跑（「只看不吃 `--resume` 以复用 tmux 名」）──
    //    判活是观测层起步初扫那一份（`observe::watcher::running_sessions`，入口注入），只对声明认 pidfile 的那几家问（`Adapter::live`）；
    //    `None` = 问不了（预览 / 不认 pidfile 的那一家）。
    let alive = match (o.resumes.as_deref(), env.running_sessions) {
        (Some(sid), Some(scan))
            if crate::agents::live_by_among(registry, &o.agent)
                == crate::agents::LiveBy::Pidfiles =>
        {
            let dir = if !config_dir.is_empty() {
                Some(config_dir.as_str())
            } else if o.use_base {
                None
            } else {
                env.inherited_config_dir.as_deref()
            };
            let mut pids: Vec<u32> = scan(dir.map(std::path::Path::new))
                .into_iter()
                .filter(|(s, _)| s == sid)
                .map(|(_, pid)| pid)
                .collect();
            pids.sort_unstable();
            // 已经不止一个进程在写它 ⇒ 接哪一个都不对、更不另起：明说是哪几个。
            if pids.len() > 1 {
                let said: Vec<String> = pids.iter().map(u32::to_string).collect();
                return Err(Die(copy_text(
                    "bePlan.build.manyWriters",
                    &[
                        ("sid", sid),
                        ("n", &pids.len().to_string()),
                        ("pids", &said.join(", ")),
                    ],
                )));
            }
            Some(pids.first().copied())
        }
        _ => None,
    };
    if let Some(sid) = o.resumes.as_deref() {
        // 托管终端里那一家一退终端就收 ⇒ 托管那几行挂着它就是在跑。
        let running = taken.and_then(|t| {
            t.running(sid)
                .map(|n| (n, TerminalHost::Tmux))
                .or_else(|| t.running_hosted(sid).map(|n| (n, TerminalHost::Managed)))
        });
        match (running, alive) {
            // 标记说它在那个终端里，进程也真在（或问不了）⇒ 接上它，不另起第二份（两份 agent 同写一份记录）。
            //   标记在、进程已经没了（agent 退了、只剩 shell 的那个会话）⇒ 不是在跑：照常起（就地 resume 键进的正是那个 pane）。
            (Some((name, host)), Some(Some(_)) | None) => {
                return Ok(Plan::Rejoin {
                    name: name.to_string(),
                    sid: sid.to_string(),
                    detach: o.detach,
                    host,
                });
            }
            // 在跑、却不在 ccm 认得的 tmux 会话里 ⇒ 接不上，也不另起第二份：明说。
            (None, Some(Some(pid))) => {
                return Err(Die(copy_text(
                    "bePlan.build.runningElsewhere",
                    &[("sid", sid), ("pid", &pid.to_string())],
                )));
            }
            _ => {}
        }
    }
    // 认不出这一家 ⇒ 空串（exec 当场说起不来，不猜成别的哪一家）。
    let launcher = if o.launcher.is_empty() {
        face.map_or("", |f| f.default_launcher).to_string()
    } else {
        o.launcher.clone()
    };

    // ── 已在 tmux 里且没给名 ⇒ 就地起（旧 `cct` 那条分支）────────────────
    let mut use_tmux = o.use_tmux;
    if use_tmux && env.in_tmux() && o.tmux_name.is_empty() && o.tmux_base.is_empty() {
        // 这条分支会让 `--detach` / `--tmux-size` 双双落空，而调用方要的是
        // 「建完就返回」，拿到的却是**阻塞式 exec** —— 语义完全相反。显式 die。
        if o.detach || !o.tmux_size.is_empty() {
            return Err(Die(copy_text("bePlan.build.inTmuxNoName", &[])));
        }
        use_tmux = false;
    }
    let host = container_host(o, env, use_tmux);

    if let Some(host) = host {
        inherited_gate(env)?;
        let (base, step_aside) = if !o.tmux_base.is_empty() {
            validate_tmux_name(&o.tmux_base)?;
            (o.tmux_base.clone(), tmux_steps_aside(TmuxNaming::Base))
        } else if !o.tmux_name.is_empty() {
            validate_tmux_name(&o.tmux_name)?;
            (o.tmux_name.clone(), tmux_steps_aside(TmuxNaming::Fixed))
        } else {
            (derive_tmux_name(&cwd), tmux_steps_aside(TmuxNaming::Auto))
        };
        // ★★ **退让就在这里发生**，`--ccm-print` 与真跑因此拿到同一个名字。
        let name = match (step_aside, taken) {
            (true, Some(t)) => mint_tmux_name(&base, t),
            // 不退让 / 问不到快照 ⇒ 原样（后者是诚实降级，见本函数头注）。
            _ => base,
        };
        // 内层：同一条命令去掉 `--ccm-tmux`，并把**继承来的**那几个变量显式化。
        // 形状 `self <交给 agent 的…> -- <ccm 自己的…>`（ccm 那一半恒非空 ⇒ 恒带 `--`，透传里有 claude 自己的 `--` 也切得对）。
        let mut opts: Vec<String> = vec![
            flag::CWD.into(),
            cwd.clone(),
            flag::AGENT.into(),
            o.agent.clone(),
        ];
        if !account.is_empty() {
            opts.push(flag::ACCOUNT.into());
            opts.push(account.clone());
        } else if !o.account_dir.is_empty() {
            opts.push(flag::ACCOUNT_DIR.into());
            opts.push(o.account_dir.clone());
        }
        if o.use_base {
            opts.push(flag::BASE.into());
        }
        if !launcher.is_empty() {
            opts.push(flag::LAUNCHER.into());
            opts.push(launcher.clone());
        }
        if !o.ccm_sid.is_empty() {
            opts.push(flag::CCM_SID.into());
            opts.push(o.ccm_sid.clone());
        }
        // 标签页那一格随这一发走（别名在窗格那一趟的命令行里已经没有了 ⇒ 写它的那个别名一起交下去）。
        if let Some(t) = o.tab {
            opts.push(flag::TAB.into());
            opts.push(crate::ccm_line::argv::tab_word(t).into());
            if !o.tab_from.is_empty() {
                opts.push(flag::TAB_FROM.into());
                opts.push(o.tab_from.clone());
            }
        }
        // 经哪条别名起的随这一发走（窗格那一趟的命令行里已经没有别名了）。
        if !o.alias.is_empty() {
            opts.push(flag::CCM_ALIAS.into());
            opts.push(o.alias.clone());
        }
        // 不轮换由窗格里那一趟记（它定会话ID、交给那一家）。
        if o.no_rotation {
            opts.push(flag::NO_ROTATION.into());
        }
        let mut inner: Vec<String> = env.self_argv.clone();
        inner.extend(o.passthru.iter().cloned());
        let inner_head = inner.clone();
        inner.push(flag::END.into());
        inner.extend(opts);
        // 🔴 **自检那一趟与 pane 里那一趟是同一串 argv**，只在 ccm 那一半末尾多一个 `--ccm-print`。
        //    两条都从这一个 `inner` 渲出来，不许在别处另拼一份。
        let mut dry = inner.clone();
        dry.push(flag::CCM_PRINT.into());
        // 〔会话血缘〕父只给内层这一条命令（不 export 进窗格的 shell：之后在那个窗格里敲的不该都算成它的孩子）；
        // 外层没有父 ⇒ 显式摘掉，窗格从 tmux server 全局环境里继承来的旧值（别的会话起 server 时留下的）读不到。
        let scoped = match &env.inherited_parent {
            Some((k, v)) => format!("{k}={} ", sq(v)),
            None => unset_prefix(&crate::agents::self_sid_envs()),
        };
        let mut payload = format!(
            "{scoped}{}",
            inner.iter().map(|a| sq(a)).collect::<Vec<_>>().join(" ")
        );
        let bare = payload.clone();
        // 🔴 **把继承来的那几个显式化** —— tmux 的 `update-environment` 默认列表不含它们，
        // 外层那句 `export` 在 tmux 进程边界上会被整个吃掉（账号注入 100% 失效，实测过）。
        // 中转地址只转用户自己的端点：我们的中转那一形属于外层的号，pane 里那一趟按目标账号自己问（见直路）。
        payload = format!("{}{payload}", carried_into_pane(o, env, &account));
        // 自检**共用载荷那一段 export 前缀**（它要在 pane 那份环境里跑）：上面只往前面加，
        // ⇒ 前缀 = 载荷去掉末尾那段裸命令。
        let exports = &payload[..payload.len() - bare.len()];
        let self_check = format!(
            "{exports}{scoped}{SELF_CHECK_IN_PANE}{}",
            dry.iter().map(|a| sq(a)).collect::<Vec<_>>().join(" ")
        );

        let size = parse_size(&o.tmux_size);
        let (host_run, unset, self_check) = match host {
            TerminalHost::Tmux => (Vec::new(), Vec::new(), self_check),
            // 托管那一形：环境由脱离起托管进程那一下交（摘 `unset` 那几个，余下原样继承）⇒ 自检那一形不带 `export` 前缀。
            TerminalHost::Managed => {
                let unset: Vec<String> = crate::agents::self_sid_envs()
                    .into_iter()
                    .filter(|k| env.inherited_parent.as_ref().is_none_or(|(p, _)| p != k))
                    .map(str::to_string)
                    .collect();
                // 同真跑那一下（`ccm::hosted_self_check_child`）：带上托管终端的标记，算的是里面那一跳那一份计划。
                let check = format!(
                    "{}{}={} {}",
                    unset_prefix(&unset),
                    crate::host::managed::run::HOST_ENV,
                    sq(&name),
                    dry.iter().map(|a| sq(a)).collect::<Vec<_>>().join(" ")
                );
                let run = host_run_argv(
                    &env.self_argv,
                    &name,
                    &cwd,
                    &o.agent,
                    crate::control::gate_rules::CLIENT_TERMINAL,
                    size.as_ref(),
                    &inner,
                );
                (run, unset, check)
            }
        };
        return Ok(Plan::Container(Container {
            host,
            host_run,
            unset,
            self_check_argv: dry,
            name,
            cwd,
            agent: o.agent.clone(),
            ccm_sid: o.ccm_sid.clone(),
            size,
            detach: o.detach,
            payload,
            self_check,
            inner_head,
        }));
    }

    // ── 非容器路（最终 exec 的那一处）────────────────────────────────────
    // 中转地址只在这里定，按**这一发的目标账号**问上游选择那张表（容器路的 pane 里那一趟也走到这里）。
    // 环境里继承来的：用户自己的端点 ⇒ 照样问（说上游是自己的），注入了就把那个地址交给这条会话的上游描述，没注入就原样不动；
    // 我们的中转那一形（别的号的）⇒ 不认，重问，
    // 这一发不注入时还要清掉它（否则 agent 拿着别的号的路由出去）。
    let relay_via = match crate::agents::inject_among(registry, &o.agent) {
        Some(crate::agents::Inject::Env(v)) => RelayVia::Env(v.to_string()),
        Some(crate::agents::Inject::Args(_)) => RelayVia::Args,
        None => RelayVia::None,
    };
    let own = user_base_url(env);
    let relay = match (env.relay, face) {
        (Some(ask), Some(f)) if relay_via != RelayVia::None => {
            let account = if !config_dir.is_empty() {
                LaunchAccount::Named {
                    config_dir: config_dir.clone(),
                }
            } else if o.use_base {
                LaunchAccount::Base
            } else {
                match env
                    .inherited_config_dir
                    .as_deref()
                    .filter(|v| !v.is_empty())
                {
                    Some(d) => LaunchAccount::Named {
                        config_dir: d.to_string(),
                    },
                    None => LaunchAccount::Undeclared,
                }
            };
            ask(f.adapter_id, &account, own.is_some()).map_err(Die)?
        }
        _ => None,
    };
    // 〔会话血缘〕地址尾上带这一趟的来处（＋ 调用方那个会话当父）：中转第一次看见就绑给新会话，它里面再起的都继承这条地址。
    // 上游是用户自己的 ⇒ 中转按来处找那条会话的上游描述 ⇒ 拼不上来处就不注入（用户的地址原样留着），不让它落到默认上游去。
    let relay = relay.and_then(|url| {
        let tagged = env.origin_token.as_deref().and_then(|token| {
            relay_route_core::with_origin(
                &url,
                token,
                env.inherited_parent.as_ref().map(|(_, v)| v.as_str()),
            )
        });
        match (tagged, own) {
            (Some(tagged), _) => Some(tagged),
            (None, None) => Some(url),
            (None, Some(_)) => None,
        }
    });
    let own_upstream = relay.as_ref().and(own).map(str::to_string);
    let clears_inherited_relay = relay.is_none()
        && own.is_none()
        && matches!(relay_via, RelayVia::Env(_))
        && env
            .inherited_base_url
            .as_deref()
            .is_some_and(|v| !v.is_empty());

    let mut argv = launcher_words(&launcher, &env.home);
    // 这一家要垫在最前面的参数（Codex 的 `--no-daemon`）；透传里自己写了的不重复垫。
    for a in face.map_or(&[][..], |f| f.launch_args) {
        if !o.passthru.iter().any(|p| p == a) {
            argv.push((*a).to_string());
        }
    }
    // 答题 hook（会话级，只挂在这一个会话上，不碰用户自己的设置文件）：这一家有答题口、这一发知道 ccm 入口 ⇒
    //   透传里没有那个旗标 ⇒ 垫在透传之前；有 ⇒ 读他那一份（JSON 串原样 / 文件按工作目录补全）、并进我们那一组、换成并好的 JSON 串交出去；
    //   读不出 / 形状不认 ⇒ 原样交出去，标「挂不上」（进程环境里带一个变量，会话事实据此写明只能在终端里答批准）。
    //   换了启动器（不是这一家默认的那一个）⇒ 不垫：它后面的词由那个启动器自己原样转交（`env VAR=x <agent>` 那一形头几个词是 env 的），
    //   ccm 不知道它认不认这一家的旗标 ⇒ 同一组 hook 做成小插件、经环境交下去（启动器只要把环境往下传就挂得上）；
    //   本机策略不许 · 版本太低 · 这台判不了子进程 ⇒ 不设，标挂不上（原因各一种）。
    let mut passthru = o.passthru.clone();
    let mut answer_hook_off = None;
    let mut plugin_dirs = None;
    let mut plugin_files = Vec::new();
    let own_launcher = face.is_some_and(|f| launcher == f.default_launcher);
    if let (Some(entry), Some(f)) = (
        env.answer_hook_entry.as_deref(),
        crate::agents::ask_face_among(registry, &o.agent),
    ) {
        use crate::agents::asks::{HookOff, SettingsWhy};
        if !own_launcher {
            match sideload_of(f.plugin, env) {
                Ok(v) => {
                    plugin_dirs = Some(v);
                    plugin_files = f.plugin.map_or_else(Vec::new, |p| (p.files)(entry));
                }
                Err(why) => answer_hook_off = Some(why),
            }
        } else {
            match settings_value_at(&passthru, f.launch_flag) {
                None => argv.extend((f.launch_args)(entry)),
                Some(at) => {
                    if let Some(read) = env.read_text {
                        let merged = at
                            .value(&passthru)
                            .and_then(|raw| {
                                if raw.trim_start().starts_with('{') {
                                    Some(raw.to_string())
                                } else {
                                    read(std::path::Path::new(&absolute_from(raw, &cwd)))
                                }
                            })
                            .ok_or(SettingsWhy::Unread)
                            .and_then(|t| {
                                serde_json::from_str::<serde_json::Value>(&t)
                                    .map_err(|_| SettingsWhy::NotJson)
                            })
                            .and_then(|v| (f.merge)(&v, entry).ok_or(SettingsWhy::Shape));
                        match merged {
                            Ok(m) => at.put(&mut passthru, f.launch_flag, m.to_string()),
                            Err(why) => answer_hook_off = Some(HookOff::Settings(why)),
                        }
                    }
                }
            }
        }
    }
    // 地址只能拼进参数的那一家：不带钥匙的地址垫在透传之前（钥匙走环境，见 [`RelayVia::Args`]）。
    if let (Some(url), Some(crate::agents::Inject::Args(words))) =
        (&relay, crate::agents::inject_among(registry, &o.agent))
    {
        argv.extend(words(url));
    }
    // 不轮换（`--no-rotation`）：要先记「不轮换」的那个会话ID —— 透传里写了这一家先定会话ID的旗标就用那个值，
    // 没写就铸一个、插在那一家自己的 `--` 之前交下去（定不了会话ID的那一家解析那一步已拒）。
    let no_rotation = match face.and_then(|f| f.preset_sid).filter(|_| o.no_rotation) {
        None => None,
        Some(sid_flag) => {
            let sid = match value_of(&passthru, sid_flag) {
                Some(v) => v,
                None => {
                    let sid = crate::accounts::quota::rotation::new_session_id();
                    let at = passthru
                        .iter()
                        .position(|a| a == flag::END)
                        .unwrap_or(passthru.len());
                    passthru.splice(at..at, [sid_flag.to_string(), sid.clone()]);
                    sid
                }
            };
            Some(NoRotation {
                agent: o.agent.clone(),
                sid,
            })
        }
    };
    argv.extend(passthru);

    Ok(Plan::Direct(Direct {
        ccm_env: env.ccm_env.clone(),
        account_env: env.account_env.clone(),
        config_dir,
        account_name: account,
        unset_config_dir: o.use_base,
        nested: nested_of(face.map(|f| f.nested_env).unwrap_or(&[])),
        cwd,
        argv,
        relay_pass_key: relay.as_ref().and(env.relay_pass_key.clone()),
        relay_env: match &relay {
            Some(_) => crate::agents::relay_env_among(registry, &o.agent)
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            None => Vec::new(),
        },
        relay,
        relay_via,
        own_upstream,
        clears_inherited_relay,
        answer_hook_off,
        plugin_dirs,
        plugin_files,
        tab: o.tab.map(|tab| crate::control::launch_account::LaunchTab {
            tab,
            alias: (!o.tab_from.is_empty()).then(|| o.tab_from.clone()),
        }),
        alias: (!o.alias.is_empty()).then(|| o.alias.clone()),
        no_rotation,
    }))
}

/// 交给那一家的那一串里 `<旗标> <值>` / `<旗标>=<值>` 的值（那一家自己的 `--` 之后不看）。
fn value_of(words: &[String], name: &str) -> Option<String> {
    let mut it = words.iter().take_while(|w| *w != flag::END);
    while let Some(w) = it.next() {
        if w == name {
            return it.next().cloned();
        }
        if let Some(v) = w.strip_prefix(name).and_then(|r| r.strip_prefix('=')) {
            return Some(v.to_string());
        }
    }
    None
}

/// 容器路要显式写进窗格那一条命令的继承值（`export …; ` 前缀；tmux 的 `update-environment` 默认列表不含它们，外层那句 `export`
/// 在 tmux 进程边界上会被整个吃掉）：账号目录变量（这一发没另选号时）· 用户自己的上游端点。别的一个都不搬 ——
/// 搬调用方整份环境就要把值写进 tmux 命令或键入的那一行，等于把密钥摊在进程表与窗格回滚里。
///
/// 〔继承表〕其余环境变量那一格（tmux 路只带这两样，其余照那台 tmux 的环境）。
fn carried_into_pane(o: &Opts, env: &Env, account: &str) -> String {
    let mut out = String::new();
    if let Some(v) = user_base_url(env) {
        out.push_str(&posix::export(&env.base_url_env, &sq(v)));
    }
    if account.is_empty() && !o.use_base && o.account_dir.is_empty() {
        if let Some(v) = env
            .inherited_config_dir
            .as_deref()
            .filter(|v| !v.is_empty())
        {
            out.push_str(&posix::export(&env.account_env, &sq(v)));
        }
    }
    out
}

/// 换了启动器那一支：答题插件能不能经环境交下去 ⇒ `(变量名, 整串值)`，或挂不上的原因。
fn sideload_of(
    face: Option<crate::agents::PluginFace>,
    env: &Env,
) -> Result<(String, String), crate::agents::asks::HookOff> {
    use crate::agents::asks::HookOff;
    let (Some(face), Some(probe)) = (face, env.sideload) else {
        return Err(HookOff::Launcher);
    };
    let s = probe(&face);
    if !s.env_readable {
        return Err(HookOff::Launcher);
    }
    if s.policy_blocked {
        return Err(HookOff::Policy);
    }
    if !s.version.is_some_and(|v| v >= face.min_version) {
        return Err(HookOff::Version);
    }
    let dir = crate::agents::asks::plugin_dir_under(&env.home);
    Ok((
        face.env.to_string(),
        crate::agents::asks::dirs_with(s.inherited.as_deref(), &dir),
    ))
}

/// 用户给的会话级设置文件（[`Env::read_text`]）读多大；超了 ⇒ 读不出，答题 hook 照「设置读不出」标挂不上（降级，理由带在结果里）。
const SETTINGS_FILE_CAP: Cap = Cap {
    bytes: 4 << 20,
    over: Over::DegradeAndSay,
};

/// 本机 managed 策略文件读多大；超了 ⇒ 日志里说是哪一份，这一份当没拦。
const POLICY_FILE_CAP: Cap = Cap {
    bytes: 4 << 20,
    over: Over::SkipAndSay,
};

/// 真跑那一份 [`Env::sideload`]：只读环境与这台的几份文件，不写。
fn sideload_here(face: &crate::agents::PluginFace) -> Sideload {
    let policy_blocked = (face.policy_files)().iter().any(|p| {
        Ro::read_json(p, POLICY_FILE_CAP)
            .ok()
            .is_some_and(|v| (face.policy_blocks)(&v))
    });
    Sideload {
        inherited: std::env::var(face.env).ok().filter(|v| !v.is_empty()),
        version: (face.installed_version)(std::env::var("PATH").ok().as_deref()),
        policy_blocked,
        env_readable: cfg!(target_os = "linux"),
    }
}

/// 透传里会话级设置那个旗标的值在哪：`<旗标> <值>`（下一个词）或 `<旗标>=<值>`（同一个词）；旗标在末尾没给值也算（值读不出）。
#[derive(Debug, Clone, Copy)]
enum SettingsAt {
    Next(usize),
    Inline(usize),
}

fn settings_value_at(words: &[String], flag: &str) -> Option<SettingsAt> {
    let eq = format!("{flag}=");
    words.iter().enumerate().find_map(|(i, w)| {
        if w == flag {
            Some(SettingsAt::Next(i + 1))
        } else if w.starts_with(&eq) {
            Some(SettingsAt::Inline(i))
        } else {
            None
        }
    })
}

impl SettingsAt {
    fn value(self, words: &[String]) -> Option<&str> {
        match self {
            SettingsAt::Next(i) => words.get(i).map(String::as_str),
            SettingsAt::Inline(i) => words.get(i).and_then(|w| w.split_once('=')).map(|(_, v)| v),
        }
    }

    fn put(self, words: &mut [String], flag: &str, v: String) {
        match self {
            SettingsAt::Next(i) => words[i] = v,
            SettingsAt::Inline(i) => words[i] = format!("{flag}={v}"),
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// `--ccm-print` 的「来自」一节：继承表里这一发有值的那几项，每一项一行（值 · 从哪来）
// ═══════════════════════════════════════════════════════════════════════════

/// 一项从哪来。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Source {
    /// 命令行上这一发给的。
    Flag,
    /// 别名里写的（那一段的名字）。
    Alias(String),
    /// 从调用方继承的。
    Inherit,
    /// 都没给，照默认。
    Default,
}

/// 「来自」一节的一行：项名 · 值 · 从哪来。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FromRow {
    pub(crate) label: String,
    pub(crate) value: String,
    pub(crate) from: Source,
}

/// 这几个旗标里最后落下的那一个来自哪一层（`origins` 由合并那一处交来，命令行那一层名字是空串）。没写 ⇒ `None`。
fn given_by(origins: &[crate::ccm_line::argv::Origin], flags: &[&str]) -> Option<Source> {
    origins
        .iter()
        .rev()
        .find(|g| flags.contains(&g.flag))
        .map(|g| match g.layer.as_str() {
            "" => Source::Flag,
            name => Source::Alias(name.to_string()),
        })
}

/// 「来自」那几行（只给起会话的两形：直路 · 容器路；接回 / 接上已在跑的不起新会话，一行都不写）。按 [`INHERIT`] 的项与次序写：
/// 这一发有值的那几项各一行；哪几个旗标改得了它照表里那一格（`change`）去合并结果里找最后落下的那一层；
/// 都没写 ⇒ 那一格在这条路上是从调用方拿的（`Caller` · `Carried`）就是「继承」，否则「默认」。
/// 值与这一发的计划同源：工作目录 · 号照 [`cwd_and_why`] / [`account_for`] 现判（同一份 `o` / `env` / `accounts` ⇒ 与计划逐字一样）。
pub(crate) fn from_rows(
    o: &Opts,
    origins: &[crate::ccm_line::argv::Origin],
    env: &Env,
    accounts: &dyn Fn() -> AccountsList,
    plan: &Plan,
) -> Vec<FromRow> {
    let tmux_name = match plan {
        Plan::Container(c) => Some(c.name.clone()),
        Plan::Direct(_) => None,
        Plan::Attach { .. } | Plan::Rejoin { .. } => return Vec::new(),
    };
    let mut out = Vec::new();
    for r in INHERIT {
        let flags: &[&str] = match r.change {
            Change::Flags(f) => f,
            Change::AliasName | Change::Settings | Change::Nobody => &[],
        };
        let pass = if tmux_name.is_some() {
            r.tmux
        } else {
            r.direct
        };
        let implicit = || match pass {
            Pass::Caller | Pass::Carried => Source::Inherit,
            Pass::Fresh | Pass::Machine | Pass::Pane | Pass::Cleared => Source::Default,
        };
        let given = |only: &[&str]| {
            let these: Vec<&str> = flags.iter().copied().filter(|f| only.contains(f)).collect();
            given_by(origins, &these)
        };
        let mut push =
            |label: String, value: String, from: Source| out.push(FromRow { label, value, from });
        match r.item {
            Item::Cwd => {
                let (cwd, why) = cwd_and_why(o, env);
                let from = match why {
                    CwdWhy::Rule => given(&[flag::CWD_IF]),
                    CwdWhy::Given => given(&[flag::CWD]),
                    CwdWhy::Caller => None,
                };
                push(
                    copy_text("newSession.label.cwd", &[]),
                    cwd,
                    from.unwrap_or_else(implicit),
                );
            }
            Item::Account if !env.account_env.is_empty() => {
                let (dir, name) = account_for(o, env, accounts).unwrap_or_default();
                let inherited = env
                    .inherited_config_dir
                    .as_deref()
                    .filter(|v| !v.is_empty());
                let none = || copy_text("beCcm.value.noAccount", &[]);
                let (value, from) = match (given(flags), inherited) {
                    (Some(f), _) if o.use_base => (none(), f),
                    (Some(f), _) => (if name.is_empty() { dir } else { name }, f),
                    (None, Some(d)) => (d.to_string(), implicit()),
                    (None, None) if !name.is_empty() => (name, Source::Default),
                    (None, None) => (none(), Source::Default),
                };
                push(copy_text("beProfile.slot.account", &[]), value, from);
            }
            Item::Parent => {
                if let Some((_, sid)) = &env.inherited_parent {
                    push(copy_text("beCcm.item.parent", &[]), sid.clone(), implicit());
                }
            }
            Item::MachinePrefs => {
                let launcher = if o.launcher.is_empty() {
                    crate::agents::launch_face_among(crate::agents::REGISTRY, &o.agent)
                        .map_or("", |f| f.default_launcher)
                        .to_string()
                } else {
                    o.launcher.clone()
                };
                push(
                    copy_text("beProfile.slot.launcher", &[]),
                    launcher,
                    given(&[flag::LAUNCHER]).unwrap_or_else(implicit),
                );
                let at = tmux_name
                    .clone()
                    .unwrap_or_else(|| copy_text("beCcm.value.here", &[]));
                push(
                    copy_text("beProfile.slot.tmux", &[]),
                    at,
                    given(&[flag::TMUX, flag::TMUX_BASE]).unwrap_or_else(implicit),
                );
            }
            Item::Tab => {
                if let Some(t) = o.tab {
                    push(
                        copy_text("beProfile.slot.tab", &[]),
                        crate::assets::aliases::page::said_of(
                            Some(flag::TAB),
                            &[crate::ccm_line::argv::tab_word(t).to_string()],
                        ),
                        given(&[flag::TAB]).unwrap_or(Source::Flag),
                    );
                }
            }
            Item::Rotation => {
                if let Some(f) = given(flags) {
                    push(
                        copy_text("machinePage.tab.rot", &[]),
                        copy_text("rot.none.label", &[]),
                        f,
                    );
                }
            }
            Item::Account
            | Item::Relay
            | Item::Nested
            | Item::MachineEnv
            | Item::Alias
            | Item::OtherEnv
            | Item::BackendCarriers => {}
        }
    }
    out
}

/// 「来自」那一节的字：每一行打头一个 `#`（混进那条命令一起交给 shell 也只是注释）。
pub(crate) fn render_from(rows: &[FromRow]) -> String {
    if rows.is_empty() {
        return String::new();
    }
    let mut out = format!("# {}\n", copy_text("beCcm.print.from", &[]));
    for r in rows {
        let from = match &r.from {
            Source::Flag => copy_text("beCcm.from.flag", &[]),
            Source::Alias(name) => copy_text("beCcm.from.alias", &[("name", name)]),
            Source::Inherit => copy_text("beCcm.from.inherit", &[]),
            Source::Default => copy_text("resumeSelect.option.defaultBare", &[]),
        };
        out.push_str(&format!(
            "# {}\n",
            copy_text(
                "beCcm.print.row",
                &[("item", &r.label), ("value", &r.value), ("from", &from)]
            )
        ));
    }
    out
}

/// 同一条计划写成 `shell` 那种方言（`--ccm-print` · 别名预览）：PowerShell 只有直路那一形（Windows 上没有 tmux，
/// 容器路 / 接回在那里起不了），别的形照 POSIX。真跑要经 `sh -c` 的那一趟恒用 [`render`]。
pub fn render_for(plan: &Plan, shell: crate::platform::shell::dialect::Shell) -> String {
    match (shell, plan) {
        (crate::platform::shell::dialect::Shell::PowerShell, Plan::Direct(d)) => {
            render_direct_ps(d)
        }
        _ => render(plan),
    }
}

/// **「等于」那一行的短形（唯一一处）**：设置页「敲 xx 等于」那一格照抄，整条脚本另在［复制］里（[`render_for`]）。
/// tmux 那一形只写建哪个会话 ＋ 里面跑的是 ccm（`tmux new-session -d -s 名 … ccm [交给 agent 的] -- …`：ccm 自己的参数省成 `…`）；直路只写进哪个目录、用哪份账号目录、跑什么
/// （环境的清与设、中转、cc-bus 身份那几段不写）；接回 ⇒ 接哪个会话。家目录写成 `~`；只给人看，词不加引号（带空白的才加）。
pub fn render_short(plan: &Plan, home: &str) -> String {
    let word = |w: &str| -> String {
        let w = match w.strip_prefix(home) {
            Some(rest) if !home.is_empty() && (rest.is_empty() || rest.starts_with('/')) => {
                format!("~{rest}")
            }
            _ => w.to_string(),
        };
        if w.is_empty() || w.chars().any(char::is_whitespace) {
            sq(&w)
        } else {
            w
        }
    };
    let words = |ws: &[String]| ws.iter().map(|w| word(w)).collect::<Vec<_>>().join(" ");
    match plan {
        Plan::Rejoin {
            name,
            host: TerminalHost::Managed,
            ..
        } => attach_managed_line(super::SUBCOMMAND_WORD, &word(name)),
        Plan::Attach { name } | Plan::Rejoin { name, .. } => {
            format!("tmux -u attach -t {}", word(name))
        }
        Plan::Container(c) if c.host == TerminalHost::Managed => format!(
            "{} {} {} {} \u{2026} {} {} \u{2026}",
            super::SUBCOMMAND_WORD,
            flag::END,
            flag::HOST_RUN,
            word(&c.name),
            words(&c.inner_head),
            flag::END
        ),
        Plan::Container(c) => format!(
            "tmux -u new-session -d -s {} \u{2026} {} {} \u{2026}",
            word(&c.name),
            words(&c.inner_head),
            flag::END
        ),
        Plan::Direct(d) => {
            let mut line = String::new();
            if !d.cwd.is_empty() {
                line.push_str(&format!("cd {} && ", word(&d.cwd)));
            }
            if !d.config_dir.is_empty() && !d.account_env.is_empty() {
                line.push_str(&format!("{}={} ", d.account_env, word(&d.config_dir)));
            }
            line.push_str(&words(&d.argv));
            line
        }
    }
}

/// 这台的终端说哪种方言（Windows 本机 = PowerShell）。
pub(crate) fn terminal_shell() -> crate::platform::shell::dialect::Shell {
    if crate::platform::shell::LOCAL_TERMINAL_IS_POWERSHELL {
        crate::platform::shell::dialect::Shell::PowerShell
    } else {
        crate::platform::shell::dialect::Shell::Posix
    }
}

/// `--ccm-print`：吐出这一趟的**等价 shell**。全部来自 [`Plan`] ⇒ 与真跑同源。
pub fn render(plan: &Plan) -> String {
    match plan {
        Plan::Attach { name } => crate::common::tmux_utf8::join_session(name),
        Plan::Rejoin {
            name, detach: true, ..
        } => format!("echo {}", sq(&format!("ccm-session={name}"))),
        Plan::Rejoin {
            name,
            host: TerminalHost::Managed,
            ..
        } => attach_managed_line(super::SUBCOMMAND_WORD, &sq(name)),
        Plan::Rejoin { name, .. } => crate::common::tmux_utf8::join_session(name),
        Plan::Container(c) if c.host == TerminalHost::Managed => render_managed(c),
        Plan::Container(c) => render_container(c),
        Plan::Direct(d) => render_direct(d),
    }
}

/// 接上一个托管终端那一行（瘦客户端）：`<ccm> -- --attach <名>`（`who` · `name` 已是那种写法里的一个词）。
fn attach_managed_line(who: &str, name: &str) -> String {
    format!("{who} {} {} {name}", flag::END, flag::ATTACH)
}

/// 托管那一形的等价 shell（只给人看）：脱离起托管进程（[`Container::host_run`]，摘 [`Container::unset`]）→ 收尾（自检 · 接上，
/// [`render_container_tail`]）。真跑那条路上这几件都在进程里做（`ccm::execute_hosted`），不经 shell，起的是同几串 argv。
fn render_managed(c: &Container) -> String {
    let run = c
        .host_run
        .iter()
        .map(|a| sq(a))
        .collect::<Vec<_>>()
        .join(" ");
    format!(
        "{}setsid -f {run} </dev/null >/dev/null 2>&1{}",
        unset_prefix(&c.unset),
        render_container_tail(c)
    )
}

fn render_container(c: &Container) -> String {
    let t = sq(&format!("={}:", c.name));
    let size = match &c.size {
        Some((w, h)) => format!(" -x {w} -y {h}"),
        None => String::new(),
    };
    let mut seq = format!(
        "{{ tmux -u new-session -d -s {} -c {}{size} 2>/dev/null",
        sq(&c.name),
        sq(&c.cwd)
    );
    seq.push_str(&format!(
        " || {{ printf {} {} >&2; exit 3; }}; }}",
        sq(&super::NAME_TAKEN_FMT),
        sq(&c.name)
    ));
    seq.push_str(&format!(
        " && (tmux -u set-option -t {t} @ccm_agent {} 2>/dev/null || true)",
        sq(&c.agent)
    ));
    // 通道 A（意图）写 `@ccm_sid_expect`，**不写** `@ccm_sid` —— 后者是通道 B（事实）的，
    // 破坏性动作只认事实标记，不被「声明了但从未真正跑起来」的会话骗过。
    if !c.ccm_sid.is_empty() {
        seq.push_str(&format!(
            " && (tmux -u set-option -t {t} @ccm_sid_expect {} 2>/dev/null || true)",
            sq(&c.ccm_sid)
        ));
    }
    seq.push_str(&format!(
        " && tmux -u send-keys -t {t} {} Enter",
        sq(&c.payload)
    ));
    let tail = render_container_tail(c);
    if !tail.is_empty() {
        // 收尾那几段在**真跑**那条路上是原样交给 `sh -c` 的（建会话那半已经在进程里做完了）
        // ⇒ 两条路读的是同一份渲染，不可能分叉。
        seq.push_str(&tail);
    }
    seq
}

/// 容器路的**收尾**：自检 · attach。
///
/// 建会话与键入载荷在真跑那条路上已经由 [`crate::control::launch`] 在**本进程里**做完了，
/// 剩下这几件仍是本机的事（attach 尤其：后端开不了你面前的窗）。
pub fn render_container_tail(c: &Container) -> String {
    let mut seq = String::new();
    // ★★ **先自检，再谈接进去** —— 排在收尾的最前面，而且只能在这里。
    //
    // 不自检的话，ccm 的 rc=0 只说明「会话建了、载荷键入了、收尾跑了」，pane 里那一跳一个字都没看：
    // 那一跳当场「unknown argument: --cwd」退回空 bash，调用方照报成功、
    // 还登记上了总线（**假成功**，现打过）。登记就发生在本段下面 ⇒ 确认必须排在它前面、住在 ccm 里；
    // 放进调用方只能「先登记、再撤回」。
    //
    // 确认的形状：**用 pane 里那一跳的同一个入口、同一串参数、同一段 export 前缀，加 `--ccm-print` 真跑一次**
    //（[`Container::self_check`]，与载荷出自 `build` 里同一个 `inner`）。等的是**一个进程退出**，
    // 不是一段时间 —— 零定时器、零轮询。它不过 ⇒ 把它自己的原话转给调用方、`exit 4`（起不来），
    // 后面的接进去与登记一段都不跑。
    //
    // ⚠ 它**买到**的：入口路由（BS1b 那一形）· 参数解析 · 账号解析 · 被叫的那个入口（`argv[0]`）是一份不认这套参数的旧副本 ·
    //   启动器在起会话那个 shell 的 PATH 上找不找得到（自检照窗格里那一跳的环境算，直路的计划里就解析启动器）。
    // ⚠ 它**买不到**的（别读成做到了）：agent exec 起来之后自己当场退出，看不见；要看见得有一个 **exec 那一刻的正信号**（今天没有）。
    seq.push_str(&format!(
        " && {{ _ccm_e=$({{ {}; }} 2>&1 >/dev/null) || {{ printf '%s\\n' \"$_ccm_e\" >&2; printf {} {} >&2; exit 4; }}; }}",
        c.self_check,
        sq(&super::SELF_CHECK_FAILED_FMT),
        sq(&c.name)
    ));
    // agent 起来时若问「信任这个目录吗」（claude 首次进一个目录），由用户在会话里自己答：
    // 那是它的安全检查，收尾一个键都不替用户按（替它按 Enter 按中的是缺省那一项「不信任、退出」）。
    if !c.detach {
        let join = match c.host {
            TerminalHost::Tmux => crate::common::tmux_utf8::join_session(&c.name),
            // 接上的是同一个入口（托管进程那一串打头的那个词）。
            TerminalHost::Managed => {
                let who = c
                    .host_run
                    .first()
                    .map_or_else(|| super::SUBCOMMAND_WORD.to_string(), |w| sq(w));
                attach_managed_line(&who, &sq(&c.name))
            }
        };
        seq.push_str(&format!("; {join}"));
    }
    seq
}

/// argv 里的一个词在 shell 那一行里的写法：地址拼进参数的那一家，装着那条地址的那个词插上钥匙段（[`keyed_word`]）。
fn argv_word(
    d: &Direct,
    word: &str,
    relay_word: fn(&str, &str, Option<&str>) -> Option<String>,
) -> Option<String> {
    let url = match (&d.relay, &d.relay_via) {
        (Some(url), RelayVia::Args) => url,
        _ => return None,
    };
    relay_word(word, url, d.relay_pass_key.as_deref())
}

fn render_direct(d: &Direct) -> String {
    let mut line = String::new();
    if !d.ccm_env.is_empty() {
        line.push_str(&format!("{}; ", d.ccm_env));
    }
    let cfg_env = &d.account_env;
    if !d.config_dir.is_empty() {
        line.push_str(&posix::export(cfg_env, &sq(&d.config_dir)));
    }
    // 没有账号载体的那一家：`--base` 什么都不做。
    if d.unset_config_dir && !cfg_env.is_empty() {
        line.push_str(&posix::unset(&[cfg_env]));
    }
    if !d.nested.is_empty() {
        line.push_str(&posix::unset(&d.nested));
    }
    match (&d.relay, &d.relay_via) {
        (Some(url), RelayVia::Env(var)) => {
            line.push_str(&relay_export(var, url, d.relay_pass_key.as_deref()))
        }
        (None, RelayVia::Env(var)) if d.clears_inherited_relay => {
            line.push_str(&posix::unset(&[var]))
        }
        _ => {}
    }
    for (k, v) in &d.relay_env {
        line.push_str(&posix::export(k, &sq(v)));
    }
    if let Some(off) = d.answer_hook_off {
        line.push_str(&posix::export(
            crate::agents::asks::ANSWER_HOOK_OFF_ENV,
            off.env_value(),
        ));
    }
    if let Some((var, value)) = &d.plugin_dirs {
        line.push_str(&posix::export(var, &sq(value)));
    }
    if !d.cwd.is_empty() {
        line.push_str(&format!("cd {} && ", sq(&d.cwd)));
    }
    let words: Vec<String> = d
        .argv
        .iter()
        .map(|a| argv_word(d, a, posix_relay_word).unwrap_or_else(|| qarg(a)))
        .collect();
    line.push_str(&posix::exec(&words));
    line
}

/// 直路那一趟要清掉的环境（唯一一处，打印出来的那一行与真跑那一趟都按它）：那一家的嵌套标记 ∪ 各家「我是哪个会话」的变量
/// （`agents::self_sid_envs`；起子进程原语对每个子进程也摘它们 —— 不写进打印那一行，`--ccm-print` 跑出来的环境就比真跑多一个父会话号）。
///
/// 〔继承表〕嵌套标记 · 会话编号变量那一格。
pub fn nested_of(face_nested: &[&str]) -> Vec<String> {
    let mut out: Vec<String> = face_nested.iter().map(|s| s.to_string()).collect();
    for k in crate::agents::self_sid_envs() {
        if !out.iter().any(|o| o == k) {
            out.push(k.to_string());
        }
    }
    out
}

/// 直路那一行的 PowerShell 写法：与 [`render_direct`] 逐项对应（设 / 清环境 · 中转地址现读钥匙文件 · 换目录 · 跑）。
/// `CCM_ENV` 是一段 POSIX shell，原样留在最前面（真跑那一趟要经 `sh`）；cc-bus 身份配方整段只在 tmux 里生效，Windows 上没有 tmux ⇒ 不写。
fn render_direct_ps(d: &Direct) -> String {
    use crate::platform::shell::dialect::ps_literal as pq;
    use crate::platform::shell::powershell as ps;
    let mut line = String::new();
    if !d.ccm_env.is_empty() {
        line.push_str(&format!("{}; ", d.ccm_env));
    }
    let cfg_env = &d.account_env;
    if !d.config_dir.is_empty() {
        line.push_str(&ps::set_env(cfg_env, &pq(&d.config_dir)));
    }
    if d.unset_config_dir && !cfg_env.is_empty() {
        line.push_str(&ps::remove_env(&[cfg_env]));
    }
    if !d.nested.is_empty() {
        line.push_str(&ps::remove_env(&d.nested));
    }
    match (&d.relay, &d.relay_via) {
        (Some(url), RelayVia::Env(var)) => {
            let word =
                ps_relay_word(url, url, d.relay_pass_key.as_deref()).unwrap_or_else(|| pq(url));
            line.push_str(&ps::set_env(var, &word));
        }
        (None, RelayVia::Env(var)) if d.clears_inherited_relay => {
            line.push_str(&ps::remove_env(&[var]))
        }
        _ => {}
    }
    for (k, v) in &d.relay_env {
        line.push_str(&ps::set_env(k, &pq(v)));
    }
    if let Some(off) = d.answer_hook_off {
        line.push_str(&ps::set_env(
            crate::agents::asks::ANSWER_HOOK_OFF_ENV,
            &pq(off.env_value()),
        ));
    }
    if let Some((var, value)) = &d.plugin_dirs {
        line.push_str(&ps::set_env(var, &pq(value)));
    }
    if !d.cwd.is_empty() {
        line.push_str(&ps::set_location(&pq(&d.cwd)));
    }
    let words: Vec<String> = d
        .argv
        .iter()
        .map(|a| argv_word(d, a, ps_relay_word).unwrap_or_else(|| pq(a)))
        .collect();
    line.push_str(&ps::call(&words));
    line
}

// 这台没有账号库的那一份账号清单成品（本包的计划判据与挂主 crate 的那几份共用）。
#[cfg(any(test, feature = "test-support"))]
#[path = "../../../../tests/backend/control/ccm/plan_support.rs"]
pub mod test_support;

#[cfg(test)]
#[path = "../../../../tests/backend/control/ccm/inherit_tests.rs"]
mod inherit_tests;
