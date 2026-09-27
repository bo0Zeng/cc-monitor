//! 从一套 [`Opts`] 算出「这一趟到底要干什么」，以及 `--ccm-print` 那条等价命令行（V138 前叫 `--print`）。
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
//! 旗标名的唯一住址是 [`super::argv::flag`]。由
//! `argv::tests::the_ccm_argv_is_parsed_in_exactly_one_place` 机检（`KR48D2`）。

use super::argv::{flag, parse_size, CwdSpec, Die, Opts};
use copy_core::copy_text;
// `K-R96`：铸名避让那张 hash 表的**唯一**来源（字段模块私有 ⇒ 这里造不出第二份）。
use crate::common::session_snapshot::TakenNames;
use shell_quote_core::posix_quote as sq;

/// 〔US1 · RK1 报 2〕把继承来的 `ANTHROPIC_BASE_URL` 显式化进新 pane 载荷时，`export … =` 右边那个 shell 词。
///
/// 继承来的值是上一个 pane 的 shell **展开过**的：我们注入的中转地址里那一段 `$(cat "$HOME/<钥匙文件>")`
/// 已经变成了 64 位钥匙本身。原样 `export` ⇒ 钥匙进这一次 `tmux send-keys` 的 **argv**（同机别的用户 `ps` 看得见）、
/// 进 pane 的 shell 历史与回滚。⇒ 认得出是我们注入的那一形（`relay_route_core::split_keyed_base_url`：回环字面量 ＋ 口 ＋
/// 形状对的钥匙段 ＋ 构造口产得出的路由）就渲回与起会话那一侧（monitor `payload::relay_env_prefix_posix`）同形的
/// `'<钥匙之前>'"$(cat "$HOME/<钥匙文件>")"'<钥匙之后>'`，在新 pane 里现读；认不出（用户自己的端点）⇒ 原样。
fn base_url_word(v: &str) -> String {
    match relay_route_core::split_keyed_base_url(v) {
        Some((head, tail)) => format!(
            "{}\"$(cat \"$HOME/{}\")\"{}",
            sq(head),
            relay_route_core::KEY_FILE_REL,
            sq(tail)
        ),
        None => sq(v),
    }
}

/// 这一趟能看见的**外界**。做成结构体的唯一理由：让整条计划面可以在单测里跑，
/// **一个字节都不碰这台机器**（`K31`「只能做产品，不能动机器」）。
#[derive(Debug, Clone, Default)]
pub(crate) struct Env {
    pub(crate) home: String,
    pub(crate) pwd: String,
    /// `$TMUX` —— 只有**被 attach 的那个进程**才有它。
    pub(crate) tmux: Option<String>,
    /// 调用方**继承来的**账号配置目录（`agents::claudecode::paths::CONFIG_DIR_ENV`
    /// 那个环境变量的值），**不是**本进程设的。
    ///
    /// ⚠ 名字刻意不叫 `claude_*`：那会让 `agent_locality_guard` 的 `.claude` 那根针
    ///   在**字段访问**上命中（`env.claude_config_dir` 里逐字含 `.claude`）——
    ///   而这个字段装的是「调用方选了哪个号」，不是 Claude 的目录布局。
    pub(crate) inherited_config_dir: Option<String>,
    pub(crate) anthropic_base_url: Option<String>,
    pub(crate) ccm_launch_id: Option<String>,
    /// 〔S5 · 第四波〕本进程环境里的启动期令牌（`CCM_RBIND_TOKEN`，变量名借自 `identity_tag`）。
    /// 只有直路上 `--ccm-sid` 那一格看它（[`DirectIdentity`]）；形状不在这里判，原样装着。
    pub(crate) launch_token: Option<String>,
    /// 起 agent 前要 eval 的机器级 env 串。
    pub(crate) ccm_env: String,
    // 🔴 `K-R58`：这里原来有一个 `workspace: String`（`$CCM_WORKSPACE`，`$HOME` 下裸敲时
    //    的落点）。它**唯一的消费者**就是 [`resolve_cwd`] 里那一档「站在 $HOME 就跳工作区」，
    //    那一档按 `K37` 删了 ⇒ 这个字段跟着删，`CCM_WORKSPACE` 这个环境变量**不再被读**。
    //    留着它会变成「猜」的一个待命开关，而 `KR58D3` 的失效方向逐字就是「把猜挪进别处」。
    /// 账号库 manifest 的**完整路径**。
    pub(crate) accts_manifest: String,
    /// **账号维度的载体**：切账号靠改哪个环境变量。由 `mod.rs` 从
    /// `agents::account_env_of(<这一趟的 agent>)` 取来 —— 本文件不认识任何 agent 的名字。
    pub(crate) account_env: String,
    /// 「怎么叫我」（内层载荷要用它把自己再叫一次）：`[argv0]`（〔09-27〕分流不看 argv0，
    /// 入口② `[argv0, "ccm"]`〔散文墓碑〕）。取法住 [`super::self_invocation`]。
    /// 〔MC1 · 2026-09-24〕从前还有一档「设了 `CCM_SELF` 就用那个值」—— 那个环境变量删了
    /// （`设计/01 §6.7b`：它只为远端 shim 存在）。
    pub(crate) self_argv: Vec<String>,
    /// `CCM_NO_PRETRUST=1`。
    pub(crate) no_pretrust: bool,
    /// cc-bus 脚本目录（`CC_BUS_SCRIPTS`），找不到就空。
    pub(crate) bus_scripts: Option<String>,
}

impl Env {
    /// 从真实进程取一份。**只读环境与那一个配置文件**，不写任何东西。
    ///
    /// 优先级逐条照旧：**环境变量 > 配置文件 > 内置默认**（默认值住
    /// [`super::argv::Defaults`]，这里一个字面量都不许再写）。
    ///
    /// ⚠ **配置文件那一层是收窄过的**：旧实现 `. "$CCM_CONFIG"`（真 source 一段 bash，
    /// 里面可以写任意 shell）；这里只认 `KEY=value`（值两侧的成对引号会被剥掉）。
    /// **这不是等价**，登记在模块头注。
    pub(crate) fn from_process() -> Self {
        use super::argv::Defaults;
        // 〔WIN1〕家目录：`HOME`，没有再退 `USERPROFILE` —— 与本 crate 其余各处同一个口径
        //   （`exit_policy::policy_path` · `asset_catalog` · `panorama`）。从前只认 `HOME`：Windows 上
        //   默认没有它 ⇒ 账号库落成 `/.claude-accts/accounts.json`（当前盘的根），找不到号还说「不在那个文件里」。
        let home = home_of(|k| std::env::var(k).ok());
        let get = |k: &str| std::env::var(k).ok().filter(|v| !v.is_empty());
        // 🔴 **`$CCM_CONFIG` 这一层本实现不认，而且不许静默不认。**
        //
        // 旧实现是 `. "$CCM_CONFIG"` —— 真 source 一段 bash，里面可以写任意 shell。
        // 在原生实现里没有等价物：要么退化成「只认 `KEY=value`」（那是**换了一套语义**
        // 而用户不会知道），要么起一个 shell 去 source 它（那就把刚删掉的 bash 请回来了）。
        // ⇒ 选第三条：**发现它存在就说一句，然后照常跑**。
        // 静默忽略正是本工作区反复消灭的那类病（写了个配置、看起来生效了、其实被吃掉）。
        let cfg_path = get("CCM_CONFIG").unwrap_or_else(|| under_home(&home, Defaults::CONFIG_REL));
        if std::path::Path::new(&cfg_path).is_file() {
            eprintln!(
                "{}",
                copy_text("bePlan.env.configIgnored", &[("path", &cfg_path)])
            );
        }
        let pick = |k: &str, fallback: String| -> String { get(k).unwrap_or(fallback) };
        Env {
            pwd: std::env::current_dir()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default(),
            tmux: get("TMUX"),
            anthropic_base_url: get("ANTHROPIC_BASE_URL"),
            ccm_launch_id: get("CCM_LAUNCH_ID"),
            launch_token: get(crate::control::identity_tag::rbind_token_env()),
            ccm_env: pick("CCM_ENV", Defaults::ENV.to_string()),
            accts_manifest: pick(
                "CCM_ACCTS_MANIFEST",
                under_home(&home, Defaults::ACCTS_MANIFEST_REL),
            ),
            // 继承值与载体名一样，要等**解析完 argv 知道是哪一家**才填得了 ⇒ 由 `mod.rs` 补。
            inherited_config_dir: None,
            account_env: String::new(),
            // 🔴 内层载荷要用**「我是被当作什么叫的」**那一段：这个进程**自己被怎么叫的**
            //   （`[argv0]`，CC1 的 `self_invocation`；〔09-27〕入口②删了，分流不看名字）。
            // 〔MC1 · 2026-09-24〕这里从前先看环境变量 `CCM_SELF`、没设才落到 argv。
            //   它存在的唯一理由是远端 shim：shim `exec <后端> ccm "$@"` 之后 `argv[0]` 是真身路径，
            //   而当时这一段只取 `argv[0]`、丢了 `ccm` 那个词 ⇒ 要 shim 把 `$0` 塞进环境变量补回来。
            //   CC1 之后不再丢词 ⇒ **shim 制造的那个问题没了，补丁也就不需要了**
            //   （`设计/01 §6.7b` 逐字「`CCM_SELF` 这个环境变量随之删掉」）。
            self_argv: super::self_invocation(&std::env::args().collect::<Vec<_>>()),
            no_pretrust: std::env::var("CCM_NO_PRETRUST").as_deref() == Ok("1"),
            bus_scripts: discover_bus_scripts(),
            home,
        }
    }
}

// 〔W5-ALIAS〕预览那一份单独一个 `impl` 块：`from_process` 那一块的判据按「块尾」截函数体（`plan_tests` 的家目录那条）。
impl Env {
    /// 〔W5-ALIAS · 第五波先行〕别名预览用的那一份（帧命令 `ccm-print`，`设计/71 §2.3`）：
    /// **「从这台机器家目录里的一个新终端敲这条别名」**。问的人是常驻后端进程，而它的 cwd / 环境
    /// 不是那个终端的 ⇒ 这几格写死、而且写在这一处（判据 `ccm::tests` 的预览那几条逐格钉）：
    /// - `self_argv = ["ccm"]` —— 别名叫的就是 `ccm`（容器路内层要把自己再叫一次，叫法就是它）；
    /// - `pwd = home` · 不在 tmux 里 · 没有继承来的中转地址 / 启动号 / 令牌；
    /// - 账号目录变量不继承（由 [`super::plan_of`] 的 `inherit_account = false` 管）。
    ///
    /// 其余照这台机器的真值：账号库 manifest · `CCM_ENV` · cc-bus 脚本目录 · `CCM_NO_PRETRUST`。
    /// ⚠ 与 [`Env::from_process`] 不同，这里**不**对 `$CCM_CONFIG` 出声：那一句是说给终端里的人听的，
    /// 常驻后端的 stderr 进的是日志。
    pub(crate) fn for_preview() -> Self {
        use super::argv::Defaults;
        let home = home_of(|k| std::env::var(k).ok());
        let get = |k: &str| std::env::var(k).ok().filter(|v| !v.is_empty());
        let pick = |k: &str, fallback: String| -> String { get(k).unwrap_or(fallback) };
        Env {
            pwd: home.clone(),
            tmux: None,
            anthropic_base_url: None,
            ccm_launch_id: None,
            launch_token: None,
            ccm_env: pick("CCM_ENV", Defaults::ENV.to_string()),
            accts_manifest: pick(
                "CCM_ACCTS_MANIFEST",
                under_home(&home, Defaults::ACCTS_MANIFEST_REL),
            ),
            inherited_config_dir: None,
            account_env: String::new(),
            self_argv: vec![super::SUBCOMMAND_WORD.to_string()],
            no_pretrust: std::env::var("CCM_NO_PRETRUST").as_deref() == Ok("1"),
            bus_scripts: discover_bus_scripts(),
            home,
        }
    }
}

/// 这个文件在不在、而且**跑得起来**吗。
///
/// 🔴 **`is_file()` 不够**〔`K-R48` 第二拍 09-11 实测逮到〕：旧 bash 实现这三处判的全是
/// `-x`，而首版原生实现写的是 `is_file()` ——「脚本在、但没有执行位」于是被读成「它能用」，
/// 拼进 seq 里执行时静默失败（整段是 `|| true`）。`tests/e2e/cc-spawn-uplift.sh` 的
/// 「台账脚本不可执行 ⇒ 明说『不进 spawn 台账』」那两格钉的正是它。
fn is_exec(p: &std::path::Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        return std::fs::metadata(p)
            .map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
            .unwrap_or(false);
    }
    // 〔WIN1 · WN1 件 G〕从前这一臂是 `p.is_file()` ⇒ Windows 上「可执行」退化成「存在」
    //   （`真相源/106 §3.4`：种一个两行文本的 `cc-register`，照样被认了）。Windows 上「跑得起来」
    //   看的是扩展名在不在 `PATHEXT` 里 —— 判定住 [`runnable_on_windows`]（纯函数，Linux 上直接测）。
    #[cfg(not(unix))]
    {
        runnable_on_windows(p.is_file(), p, std::env::var("PATHEXT").ok().as_deref())
    }
}

/// Windows 上「这个文件跑得起来吗」：**是文件，且扩展名在 `PATHEXT` 里**（大小写不敏感）。
///
/// `PATHEXT` 缺席 / 空 ⇒ 用 Windows 自己的默认值（`.COM;.EXE;.BAT;.CMD`）—— 不是猜，
/// 那是 `cmd.exe` 在这个变量没设时认的那一份。没有扩展名的文件（例如 POSIX 的 `cc-register`）
/// 在 Windows 上**不是**可执行的 ⇒ 判 `false`。
#[cfg(any(not(unix), test))]
pub(crate) fn runnable_on_windows(
    is_file: bool,
    p: &std::path::Path,
    pathext: Option<&str>,
) -> bool {
    if !is_file {
        return false;
    }
    let Some(ext) = p.extension().and_then(|e| e.to_str()) else {
        return false;
    };
    let list = pathext
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(".COM;.EXE;.BAT;.CMD");
    list.split(';')
        .map(|e| e.trim().trim_start_matches('.'))
        .any(|e| !e.is_empty() && e.eq_ignore_ascii_case(ext))
}

/// 从一组环境变量里取家目录：`HOME`，空 / 缺席再退 `USERPROFILE`；两个都没有 ⇒ 空串（照旧）。
/// 纯函数（`get` 注入），两平台的判定表在 Linux 上直接测。
pub(crate) fn home_of(get: impl Fn(&str) -> Option<String>) -> String {
    get("HOME")
        .filter(|h| !h.is_empty())
        .or_else(|| get("USERPROFILE").filter(|h| !h.is_empty()))
        .unwrap_or_default()
}

/// 家目录下一个 `/` 分隔的相对路径 → 本平台的完整路径（逐段 `join`，Windows 上不再 `\` 与 `/` 混拼）。
/// 家目录为空时照旧拼成 `/<rel>`（与改之前逐字相同 —— 这一格不是本件要改的行为）。
pub(crate) fn under_home(home: &str, rel: &str) -> String {
    if home.is_empty() {
        return format!("/{rel}");
    }
    rel.split('/')
        .filter(|s| !s.is_empty())
        .fold(std::path::PathBuf::from(home), |p, s| p.join(s))
        .to_string_lossy()
        .into_owned()
}

/// cc-bus 的脚本目录：`CC_BUS_SCRIPTS` → 本二进制旁边的 `cc-bus/scripts` → `PATH`。
///
/// 🔴 **第三档（`PATH`）是 `K-R48` 第二拍补回来的，别再删**：旧 bash `ccm` 住在
/// `shared/`（部署形态下 `~/.claude/skills/ccm`），它的**兄弟目录**正好就是
/// `cc-bus/scripts` ⇒ 第二档几乎总是命中。今天 `ccm` 是后端二进制、住
/// `~/.cc-monitor/bin/` —— **它旁边永远没有 `cc-bus/`** ⇒ 第二档在真实部署里**恒不命中**，
/// 而 `PATH` 那一档是唯一还够得着的。首版漏了它（docstring 写着、实现里没有），
/// 后果是「`--bus-register` 要了登记，却谁也没登记」。
pub(crate) fn discover_bus_scripts() -> Option<String> {
    if let Ok(d) = std::env::var("CC_BUS_SCRIPTS") {
        if !d.is_empty() && is_exec(&std::path::Path::new(&d).join("cc-register")) {
            return Some(d);
        }
    }
    if let Some(sibling) = std::env::current_exe()
        .ok()
        .and_then(|me| me.parent().map(|p| p.join("cc-bus").join("scripts")))
    {
        if is_exec(&sibling.join("cc-register")) {
            return Some(sibling.to_string_lossy().to_string());
        }
    }
    // `PATH` 上的 `cc-register`（旧实现逐字：`command -v cc-register` 再取 `dirname`）。
    for dir in std::env::split_paths(&std::env::var_os("PATH")?) {
        if is_exec(&dir.join("cc-register")) {
            return Some(dir.to_string_lossy().to_string());
        }
    }
    None
}

/// 一个账号。manifest 里 `configDir` **键缺席** = 账号 0（不设 `CLAUDE_CONFIG_DIR`）。
#[derive(Debug, Clone, serde::Deserialize)]
pub(crate) struct Account {
    pub(crate) name: String,
    #[serde(rename = "configDir", default)]
    pub(crate) config_dir: Option<String>,
    #[serde(rename = "isDefault", default)]
    pub(crate) is_default: bool,
}

#[derive(Debug, Clone, serde::Deserialize, Default)]
struct Manifest {
    #[serde(default)]
    accounts: Vec<Account>,
}

/// 账号表。**唯一真相源是那份 manifest** —— 从前 `shared/ccm` 要跨一次进程去问 backend
/// 才拿得到它（`--list-accounts --accts-dir`），那一整段是 bash 与后端说话的**税**，
/// 不是功能。同一个二进制之下它整块消失。
#[derive(Debug, Clone, Default)]
pub(crate) struct AccountTable {
    pub(crate) accounts: Vec<Account>,
    /// 🔴 那份 manifest **在盘上、有内容，却解析不动** —— 与「这台机器没有账号库」
    /// 是**两个状态**，而从前它们在输出上一模一样。
    unreadable: bool,
}

impl AccountTable {
    /// 现成的一摞号建一张表（判据夹具用；生产侧只经 [`Self::load`] 进来）。
    pub(crate) fn from_accounts(accounts: Vec<Account>) -> Self {
        Self {
            accounts,
            unreadable: false,
        }
    }

    /// 读那份 manifest。
    ///
    /// # 三个状态，不是两个（这一条是承重的）
    ///
    /// | 盘上是什么 | 从前 | 现在 |
    /// |---|---|---|
    /// | 没有这份文件 | 空表，一个字不说 | 同 —— 「这台机器没有账号库」是合法状态 |
    /// | 有，解析得动 | 用它 | 同 |
    /// | **有、有内容、解析不动** | 🔴 **空表，一个字不说** | 空表，**但说得出来** |
    ///
    /// ## 🔴 第三格是真机量到的，不是假想
    ///
    /// 2026-09-21 在本机那台 Win11 虚拟机上：`accounts.json` 带 **UTF-8 BOM**
    ///（`EF BB BF` —— **PowerShell 5.1 `-Encoding UTF8` 的默认值**）⇒ `serde_json`
    /// 在第 1 列就失手（现打逐字：`expected value at line 1 column 1`）
    /// ⇒ 从前那句 `unwrap_or_default()` 把**整张账号库静默吃掉**，
    /// 而给用户的话是 [`Self::names`] 那句「(无账号库)」。
    /// ⇒ **用户拿 PowerShell 碰过这份文件，他的号全消失，还被告知本来就没有。**
    ///
    /// ## 两件事各修一半（刻意不合成一件）
    ///
    /// ① **BOM 剥掉** —— 那是真正的缺陷：那份文件是**合法的**，只是前面带了三个
    ///    字节序标记。剥掉之后它照常解析得动，用户什么都不用改。
    /// ② 剩下那些**真的**解析不动的（手写坏了 JSON），**不许再静默** ——
    ///    [`Self::names`] 会把「解析不动」这件事说出来，而不是谎称「没有账号库」。
    ///
    /// ⚠ 刻意**不**在这里报错退出：「读不到账号库」照旧是合法的降级路径
    /// （退化为基座启动器）。改的只是**那句话的真假**。
    pub(crate) fn load(manifest_path: &str) -> Self {
        let raw = match std::fs::read_to_string(manifest_path) {
            Ok(s) => s,
            Err(_) => return Self::default(),
        };
        // ① BOM：`read_to_string` 不剥它，`serde_json` 也不吃它。
        let body = raw.trim_start_matches('\u{FEFF}');
        if body.trim().is_empty() {
            // 空文件 == 没有账号库（**不是**「解析不动」，别报成坏文件）。
            return Self::default();
        }
        match serde_json::from_str::<Manifest>(body) {
            Ok(m) => Self::from_accounts(
                m.accounts
                    .into_iter()
                    .filter(|a| !a.name.is_empty())
                    .collect(),
            ),
            // ② 有内容却解析不动 ⇒ 空表，**但这件事说得出来**。
            Err(_) => Self {
                accounts: Vec::new(),
                unreadable: true,
            },
        }
    }

    /// 名字 → 存在的 configDir。**目录不存在 ⇒ 当作不可用**
    ///（「manifest 里写着」与「盘上真有」是两件事）。
    fn config_dir_of(&self, want: &str) -> Option<String> {
        let a = self.accounts.iter().find(|a| a.name == want)?;
        let c = a.config_dir.as_deref().filter(|c| !c.is_empty())?;
        std::path::Path::new(c).is_dir().then(|| c.to_string())
    }

    /// 报「不可用」必须同时告诉用户有哪些可用。⚠ 无尾空格。
    fn names(&self) -> String {
        // 🔴 「解析不动」优先于「是空的」—— 反了就又谎称「没有账号库」。
        if self.unreadable {
            return copy_text("bePlan.names.unreadable", &[]);
        }
        if self.accounts.is_empty() {
            return copy_text("bePlan.names.none", &[]);
        }
        self.accounts
            .iter()
            .map(|a| a.name.as_str())
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// `isDefault: true` 的第一个。
    fn default_name(&self) -> Option<&str> {
        self.accounts
            .iter()
            .find(|a| a.is_default)
            .map(|a| a.name.as_str())
    }
}

/// 一趟要干的事。`--ccm-print` 与真跑读的是**同一个**。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Plan {
    /// 不起 agent，直接接回一个既有会话。
    Attach { name: String },
    /// 建一个 tmux 容器，把「同一条命令去掉 `--tmux`」送进去。
    Container(Container),
    /// 在**本进程**里设好环境、`cd`、然后 `exec`。
    Direct(Direct),
    /// 〔FIX · V138〕resume 的那条会话已经在 tmux 会话 `name` 里跑 ⇒ 接上它，不另起一份（`--detach` ⇒ 只报名字）。
    Rejoin {
        name: String,
        sid: String,
        detach: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Container {
    pub(crate) name: String,
    pub(crate) cwd: String,
    pub(crate) agent: String,
    pub(crate) ccm_sid: String,
    pub(crate) size: Option<(String, String)>,
    pub(crate) detach: bool,
    /// 送进容器的那条内层命令（已经是一整条 POSIX 命令串）。
    pub(crate) payload: String,
    /// 〔CC1〕**同一条内层命令的自检形**：同一段 `export` 前缀、同一个入口、同一串参数，
    /// 只在 `--` 前多一个 `--ccm-print`。收尾那段在登记之前先跑它（见 [`render_container_tail`]）。
    pub(crate) self_check: String,
    /// 要不要挂那段「抓信任框、自动按 Enter」的兜底轮询。
    pub(crate) trust_poll: bool,
    // ★★ 〔`K-R96` 09-12〕**`avoid_collision` 这个字段删了** —— 散文墓碑留在这里。
    //
    // 它从前的意思是「这个名字撞了要不要退让」，而退让本身发生在 `mod.rs::execute`
    // ——**只在真跑那条路上**，`--ccm-print` 吐的是没退让过的名字。
    //
    // 用户 09-12 逐字（`R52` 裁定二）：「**不就是先校验冲突然后取名吗? 搞个 hash 表**不就好了」。
    // ⇒ 退让搬进 [`build`]：它拿一份 [`TakenNames`]（那张 hash 表，来源只有会话快照一处），
    //   **算完就把最终名钉进 `Container::name`**。于是：
    //   ① 「产名」与「避让」不再是两个人干的两件事（前端 `mintTmuxName` 那条纪律的对侧）；
    //   ② `--ccm-print` 与真跑吐的是**同一个名字** —— 平价预言机从此在名字这一维上也是平的；
    //   ③ `--ccm-print` 的「纯」口径改成**相对于快照**：同一份快照 ＋ 同一份输入 ⇒ 同一份输出。
    //
    // 三条取名路的态度**仍然不一样，别合并**（判据见
    // `only_two_of_the_three_naming_paths_step_aside_on_a_collision`）：
    // - 显式 `--tmux=<名>` ⇒ **不退让**（调用方说的就是要这个名；撞了走 `C14` 响亮失败）；
    // - `--tmux-base=<基名>` ⇒ **退让**（`C15` 给 cc-spawn 的那条路，它随后要读回真名字）；
    // - 不给名、从 cwd 派生 ⇒ **退让**（幂等接回同一目录的会话，撞了说明有别人占了）。
    /// `--bus-register` 要的登记；找不到 cc-bus 脚本就是 `None`（并出一句声）。
    pub(crate) bus: Option<BusRegister>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BusRegister {
    pub(crate) scripts_dir: String,
    pub(crate) note: String,
    pub(crate) has_spawned_record: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Direct {
    /// `CCM_ENV`（机器级 env，先于会话级）。
    pub(crate) ccm_env: String,
    /// codex 要的 cc-bus 身份配方（claude 不要 —— 会盖掉 `@cc_id` 细分）。
    pub(crate) bus_id_recipe: bool,
    /// 🔴 〔`P19` 09-22〕这一趟**真的在 tmux 里**吗（`$TMUX` 非空）。
    ///
    /// 上一行那段配方（[`super::BUS_ID_RECIPE`]）**整段裹在 `if [ -n "${TMUX:-}" ]` 里** ⇒
    /// 这一格为假时它**一个字都不做**。真跑那一侧靠这一格判断「还要不要请一个 shell 进来」
    /// （[`super::needs_shell`]）—— 而那正是 `--agent codex` 在 Windows 上
    /// `program not found` 的那一跳（读数住 `真相源/106 §3.3`）。
    ///
    /// ⚠ **只有真跑那一侧读它；[`render`] 一个字不看** —— `--ccm-print` 必须对宿主环境
    /// 逐字节稳定（`INVARIANTS §33a` 铁律 2：不查实时 tmux 状态，**值不知道就打印配方**）。
    /// 把这一格接进 `render` 会让同一条计划在 tmux 内外吐出两种输出，那正是 `§33a` 的原病。
    pub(crate) inside_tmux: bool,
    /// 账号维度的载体（环境变量名）—— 见 [`Env::account_env`]。
    pub(crate) account_env: String,
    /// 要 export 的账号目录。
    pub(crate) config_dir: String,
    /// `--base`：显式 `unset CLAUDE_CONFIG_DIR`。
    pub(crate) unset_config_dir: bool,
    /// 要 unset 的嵌套标记（claude 四个 / codex 零个）。
    pub(crate) nested: Vec<String>,
    pub(crate) cwd: String,
    /// 最终 exec 的 argv：启动器 ＋ 原样透传（V138：`--resume` 这类 ccm 不吃，照写交出去）。
    pub(crate) argv: Vec<String>,
    /// 这一趟有没有身份面（claude 有、codex 没有）。
    pub(crate) has_identity: bool,
    /// 〔S5 · 第四波〕`--ccm-sid` 在直路上交给谁 —— 见 [`DirectIdentity`]。
    pub(crate) identity: DirectIdentity,
}

/// 〔S5 · 第四波 · `99 §4.4` 那一行 · `WN1.md §3`〕**直路上 `--ccm-sid` 的语义 = 启动期令牌。**
///
/// 从前 `--ccm-sid` 进了 `Opts` 之后只有容器路消费（写 `@ccm_sid_expect`），直路上**被接受、零效果、不出声**
/// （`lib.rs::TARGET_GAPS` 那一行逐字）。主会话裁：**不报错**（报错 ＝ 让它依赖 tmux，撞 V63
/// 「`--ccm-sid` 不要依赖 tmux」），直路语义走已落地的启动期令牌那条路。
///
/// 为什么是令牌、不是再造一个变量：`--ccm-sid` 要的是「让拉前认得这个会话」，V63 之后这件事的载体
/// 就是 `CCM_RBIND_TOKEN` —— 它在起 agent 的进程环境里 ⇒ `exec` 原样继承 ⇒ 后端从 agent 进程的
/// environ 读回、连同 pidfile 里的**真** sid 报上 wire（`identity_tag.rs` 第二张面）。令牌那条路
/// **不需要**调用方预告 sid。而「把 sid `export` 成一个新变量」没有任何读者（后端读别的进程环境的
/// 只有两族、各两个键，`identity_tag_tests` / `accounts_query_tests` 钉着）⇒ 不造。
///
/// ⇒ 直路要做的只是**看一眼载体在不在**，不在就说一句（不报错、照常起）。`--ccm-print` 不看它
/// （`INVARIANTS §33a` 铁律 2：不看宿主环境；令牌是继承的环境，不是命令文本）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DirectIdentity {
    /// 没给 `--ccm-sid`。
    NotAsked,
    /// 给了，而且环境里有形状合格的令牌 ⇒ `exec` 会把它交给 agent，身份由它承载。
    ByLaunchToken,
    /// 给了，但环境里没有令牌（或形状过不了 `identity_tag::token_is_safe`）⇒ 拉前认不出这个会话。
    NoCarrier,
}

/// 直路上 `--ccm-sid` 交给谁。形状判定只有一份（〔DUP3〕`shell_quote_core::rbind_token_ok`，这里经 `identity_tag::token_is_safe` 那个再导出名调它）。
pub(crate) fn direct_identity(ccm_sid: &str, launch_token: Option<&str>) -> DirectIdentity {
    if ccm_sid.is_empty() {
        return DirectIdentity::NotAsked;
    }
    match launch_token {
        Some(t) if crate::control::identity_tag::token_is_safe(t) => DirectIdentity::ByLaunchToken,
        _ => DirectIdentity::NoCarrier,
    }
}

/// POSIX argv 元素的**最省引号**写法：能裸写就裸写。
///
/// 与 `--ccm-print` 的可读性直接相关：`exec claude --resume abc` 比
/// `exec 'claude' '--resume' 'abc'` 好读，而两者语义相同。
pub(crate) fn qarg(s: &str) -> String {
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

/// 从一个目录派生 tmux 会话名。
///
/// 与前端 `src/shell-quote.ts::deriveTmuxName` **逐字同规则**（跨语言双写点）：
/// basename → 非 `[A-Za-z0-9_-]` 换 `-` → 折叠 → 截 32 → 剥首尾 `-` → 加 `-cc` 后缀。
/// 同规则 = 终端与 app「开新 Claude」在同一目录造出**同一个名字** ⇒ 幂等接回同一会话。
pub(crate) fn derive_tmux_name(cwd: &str) -> String {
    let trimmed = cwd.trim_end_matches('/');
    let base = trimmed.rsplit('/').next().unwrap_or("");
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
    let s = s.trim_matches('-');
    if s.is_empty() {
        "session-cc".to_string()
    } else {
        format!("{s}-cc")
    }
}

/// 基名撞了就退让：`<基名>` → `<基名>-2` → `<基名>-3` … 取第一个没被占的。
///
/// 纯函数（已占用的名字由调用方给）—— 这样「退让规则」测得了。
///
/// 〔`K-R96` 09-12〕**谁给那份 `taken`，今天只有一个答案**：[`build`] 从
/// [`TakenNames`] 里拿，而 `TakenNames` 的字段是 `common::session_snapshot` 模块私有的
/// ⇒ 「另起一份名字集合」在类型层面就造不出来。`--ccm-print` 与真跑用的是同一份。
fn next_free_name(base: &str, taken: &[String]) -> String {
    if !taken.iter().any(|t| t == base) {
        return base.to_string();
    }
    let mut k = 2usize;
    loop {
        let cand = format!("{base}-{k}");
        if !taken.iter().any(|t| *t == cand) {
            return cand;
        }
        k += 1;
    }
}

/// 会话名的形状校验 —— 显式名与基名都过这里（`ccm` 要**新建**的会话名）。
///
/// 〔DUP2 · 主会话 09-26 裁 J6〕**规则只有一份**，住共享 crate（`gate_core::new_tmux_name_issue`：非空 · 不以 `-` 开头 ·
/// 无 `*?.:=` · 无控制符与视觉欺骗字符 · ≤128）；monitor 的载荷外层与 `ccm …` 调用行调的是同一个函数。本函数只剩「说哪一句」。
/// 这里原来自己写了一份（自称「唯一一份」，而 monitor 与界面各还有一份、规则各不相同）；比那一份多出来的两格
/// （欺骗字符 · 超长）是三份取交集时从 monitor 载荷那一份与界面那一份带进来的。
pub(crate) fn validate_tmux_name(n: &str) -> Result<(), Die> {
    // 〔FIX · `99 §2 ㊹`〕说哪一句住 `launch::new_tmux_name_said`（后端 `launch` 新建那一支也用它）。
    match crate::control::launch::new_tmux_name_said(n) {
        None => Ok(()),
        Some(said) => Err(Die(said)),
    }
}

/// 不给 `--cwd` 时的落点。**今天它是恒等**：调用方站在哪儿，会话就起在哪儿。
///
/// 🔴 **`K-R58` / `KR58D3`（`K37` 第三条）：这里原来有两档「替用户挑一个目录」，删了。**
///
/// 删掉的逐字是这两档（留在这里当墓碑，别再长回来）：
/// - `pwd == $HOME` ⇒ 跳 `$CCM_WORKSPACE`（默认 `$HOME/claude-conversation`）；
/// - 往上找得到 `.git` ⇒ 跳**那个仓的父目录**（你在仓里敲，会话起在仓外）。
///
/// 〔用@09-11 逐字〕「`cc` 默认就起会话就行，**跳目录是我自己的设置，不要搞进 app**。」
/// `K37` 的判据：**把这个行为去掉，用户还做不做得到同一件事**？——写 `--cwd <目录>` 照样
/// 起得了会话 ⇒ 它是**偏好**，不是机制，该出去。而 `K37` 第三条给了新默认的形状：
/// **诚实的默认 = 恒等 / 不作为 / 沿用调用者已有状态** —— `cwd` 本来就是进程的属性，
/// 不是从一张表里编出来的。★ **省的是书写，不是语义。**
///
/// ⚠ **`resume`/`attach` 那一支一个字都没动，而它今天与 `new` 同值**：
/// 那两个动作的目标目录由 sid / 会话决定、调用方已经定位好了，再解析一次会把工作目录
/// 换掉 ⇒ claude 按 `projects/<enc(cwd)>/<sid>.jsonl` **找不到会话**（实测踩过）。
/// 🔴 **谁要是哪天再往这里加一档非恒等的分支，它必须只对「新起会话」生效**（透传里没有 `--resume` / `--continue`）——
/// 今天不留那个 `if`，是因为恒等之下它是死代码，而不是因为那条约束过期了。
///
/// 〔一并作废的旧注：从前这里逐字复刻旧 bash `_cc_resolve_target`，并登记着
/// 「自己往上走找 `.git`」与 `git rev-parse --show-toplevel` 在 `GIT_DIR` /
/// `GIT_WORK_TREE` / `GIT_CEILING_DIRECTORIES` 上的那一格不等价 —— 那整条路没了，
/// 那格不等价也跟着没了。〕
///
/// 〔用户 09-26〕相对的 `--cwd`（`.` / `../x`）按调用方当前目录补成绝对、按字面折掉 `.` / `..`，之后再过 §47 形式判定；
/// 已是绝对的原样交给判定（带 `..` 照旧拒）。
pub(crate) fn resolve_cwd(o: &Opts, env: &Env) -> String {
    match &o.cwd_spec {
        CwdSpec::Explicit(d) if !std::path::Path::new(d).is_absolute() && !env.pwd.is_empty() => {
            lexical_join(&env.pwd, d)
        }
        CwdSpec::Explicit(d) => d.clone(),
        CwdSpec::Auto => env.pwd.clone(),
    }
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
/// （用户 09-26 选拆词）。字符白名单在 [`free_text_gate`] 对整串判过（`launcher_refused_char`）。
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

/// 账号解析。三态，**一个字都没改**（这是从 `shared/ccm:996-1015` 搬过来的语义）：
///
/// - 显式 `--account X` ⇒ 必须解析成功，否则**中止**（显式选号绝不静默降级到别的号）。
/// - 显式 `--base` ⇒ 不注入（issue #75 逃生口）。
/// - 都不传 ⇒ **只在调用方没有已选定账号时**才落 manifest 的 `isDefault`。
///
/// ✅ **最后那一条已经裁了**（用户 09-12，住址 `DECISIONS.md#R28`）——
/// 逐字：「把调用方选中的号静默换掉 / **不要这么做** / 不是有选默认账号吗? **就用那个**」。
/// ⇒ 拆成本函数的两支，**两支都是裁定的一部分，缺一条这一裁就没落地**：
///   · **继承那一支**（`CLAUDE_CONFIG_DIR` 非空 ⇒ 保留，不覆盖）= 「不要静默换掉」。
///     🔴 **来历不许删**：这道闸是 `R08` 加的（`-z "$CLAUDE_CONFIG_DIR"`），
///     由一次**真机复现过的静默换号**逼出来 —— 删了来历，下一个人会以为它是可有可无的防御。
///   · **裸终端那一支**（都没给 ⇒ 落 manifest `isDefault`）= 「就用那个」。
/// ⚠ **09-11 那三句读法（「已知病灶」/「归待问用户」/「判不了不自批」）已经过期**，
/// 别再照它把一个定了的问题重新打开。要改行为的落点仍然只在这个函数，别处没有第二处。
pub(crate) fn resolve_account(
    o: &Opts,
    env: &Env,
    table: &AccountTable,
) -> Result<(String, String), Die> {
    if !o.account.is_empty() {
        return match table.config_dir_of(&o.account) {
            Some(d) => Ok((d, o.account.clone())),
            None => Err(Die(copy_text(
                "bePlan.resolveAccount.unavailable",
                &[
                    ("account", &o.account.to_string()),
                    ("manifestPath", &env.accts_manifest.to_string()),
                    ("names", &(table.names()).to_string()),
                ],
            ))),
        };
    }
    if o.use_base {
        return Ok((String::new(), String::new()));
    }
    if env
        .inherited_config_dir
        .as_deref()
        .is_some_and(|v| !v.is_empty())
    {
        // 调用方已经选好号 ⇒ **尊重它**，不覆盖（`R08`：真机复现过的静默换号）。
        return Ok((String::new(), String::new()));
    }
    let Some(def) = table.default_name().map(|s| s.to_string()) else {
        return Ok((String::new(), String::new()));
    };
    match table.config_dir_of(&def) {
        Some(d) => Ok((d, def)),
        None => Err(Die(copy_text(
            "bePlan.resolveAccount.defaultMissing",
            &[("name", &def.to_string())],
        ))),
    }
}

/// 〔TL3 · `INVARIANTS §47` ② · 主会话 09-26 按 V131 裁〕**自由文本**那几格拼进 shell（`--ccm-print` 那一串 · pane 里键入的载荷 ·
/// 收尾那几段 `sh -c`）之前的放行判定：
///
/// | 格 | 形式（按本机语境） | 拒绝集 |
/// |---|---|---|
/// | 工作目录（`--cwd` / 当前目录） | 绝对路径（`Path::is_absolute`，Windows 上认 `C:\` 那一形）· 没有 `..` 段 | NUL / CR / LF |
/// | 透传给 agent 的参数 · 登记备注 · 继承来的 `CLAUDE_CONFIG_DIR` / `ANTHROPIC_BASE_URL` / `CCM_LAUNCH_ID` | — | NUL / CR / LF |
///
/// 〔DUP3 · 主会话 09-26 裁 · `INVARIANTS §47` ③〕**启动器不在上表**：它是命令片段（带参数 · alias · 路径），不是自由文本 ——
/// 过全仓那一张白名单 `shell_quote_core::launcher_refused_char`（与 monitor 本机 `history.rs` · 远端载荷 `payload.rs` 同一条，
/// `设计/01 §6.8`；先前这一格只拒 NUL / CR / LF）。空 = 没给（下面用这个 agent 的默认启动器）。
///
/// **不拒 shell 元字符**（`Bob's` · `(2019)` 照放，交给唯一的 quote）。拒绝集住 `shell_quote_core::free_text_ok`。
/// ⚠ 模型名与 `--ccm-sid` **不在这里**：主会话裁交 DUP1（判定唯一住址那一路）统一定规则（`调研/第四波记录/TL3.md §7.3`）。
/// ⚠ 继承来的那三个只在它们真会被拼进去的时候才判（容器路把它们显式化进载荷，[`inherited_gate`]）：
///   环境里一个用不上的怪值不该挡住起会话（拒过头）。
fn free_text_gate(cwd: &str, o: &Opts) -> Result<(), Die> {
    let p = std::path::Path::new(cwd);
    if !p.is_absolute()
        || p.components().any(|c| c == std::path::Component::ParentDir)
        || !shell_quote_core::free_text_ok(cwd)
    {
        return Err(refuse(flag::CWD, cwd));
    }
    if let Some(c) = shell_quote_core::launcher_refused_char(&o.launcher) {
        return Err(Die(copy_text(
            "bePlan.build.launcherRefused",
            &[
                ("what", &flag::LAUNCHER.to_string()),
                ("value", &format!("{:?}", o.launcher)),
                ("c", &format!("{c:?}")),
            ],
        )));
    }
    if let Some(a) = o
        .passthru
        .iter()
        .find(|a| !shell_quote_core::free_text_ok(a))
    {
        return Err(refuse(flag::END, a));
    }
    if !shell_quote_core::free_text_ok(&o.bus_note) {
        return Err(refuse(flag::BUS_NOTE, &o.bus_note));
    }
    Ok(())
}

/// 容器路要把继承来的那三个显式化进载荷（tmux 边界会吃掉它们）⇒ 那一刻才判（见 [`free_text_gate`]）。
fn inherited_gate(env: &Env) -> Result<(), Die> {
    for (what, v) in [
        (
            env.account_env.as_str(),
            env.inherited_config_dir.as_deref(),
        ),
        ("ANTHROPIC_BASE_URL", env.anthropic_base_url.as_deref()),
        ("CCM_LAUNCH_ID", env.ccm_launch_id.as_deref()),
    ] {
        if let Some(v) = v.filter(|v| !shell_quote_core::free_text_ok(v)) {
            return Err(refuse(what, v));
        }
    }
    Ok(())
}

/// 自由文本那几格判不过时的那句话（哪一格 · 原值）。
fn refuse(what: &str, v: &str) -> Die {
    Die(copy_text(
        "bePlan.build.freeTextRefused",
        &[("what", &what.to_string()), ("value", &format!("{v:?}"))],
    ))
}

/// 从 [`Opts`] ＋ 外界 ⇒ [`Plan`]。**这是计划面的唯一入口。**
///
/// `taken` = 那一刻**已被占用的会话名**（`R52` 裁定二那张 hash 表），
/// 来源只有 `common::session_snapshot` 一处；`None` = **问不到**
/// （tmux 起不来 / 没装）⇒ **不退让**，与那份已删的 bash `ccm` 那句
/// `tmux has-session … 2>/dev/null`（问不出来当没占）同义。真撞上了还有
/// `created:false` ⇒ rc=3 那条响亮失败兜底。
///
/// 🔴 **本函数相对 `taken` 是纯的**：同一份快照 ＋ 同一份 `o`/`env`/`table`
/// ⇒ 同一份 `Plan`（`--ccm-print` 的「纯」今天就是这个口径）。
pub(crate) fn build(
    o: &Opts,
    env: &Env,
    table: &AccountTable,
    taken: Option<&TakenNames>,
) -> Result<Plan, Die> {
    // ── attach：不起 agent，早于容器逻辑就定了 ──────────────────────────
    if !o.attach_name.is_empty() {
        // `ccm --attach foo --tmux --detach` 从前会**静默吞掉** `--detach` 照样 attach。
        // 静默忽略正是本工作区反复消灭的病。
        if o.detach {
            return Err(Die(copy_text("bePlan.build.attachDetach", &[]).into()));
        }
        if !o.tmux_size.is_empty() {
            return Err(Die(copy_text("bePlan.build.attachSize", &[]).into()));
        }
        return Ok(Plan::Attach {
            name: o.attach_name.clone(),
        });
    }

    // ── resume 先查是否已在跑（V138「只看不吃 `--resume` 以复用 tmux 名」· `设计/71 §8` 第 12 条）──
    //    在跑 ⇒ 接上它，不另起第二份（两份 claude 同写一份记录）。判「在跑」只问避让那同一份快照。
    if let Some((sid, name)) = o
        .resumes
        .as_deref()
        .and_then(|sid| taken.and_then(|t| t.running(sid)).map(|n| (sid, n)))
    {
        return Ok(Plan::Rejoin {
            name: name.to_string(),
            sid: sid.to_string(),
            detach: o.detach,
        });
    }

    let cwd = resolve_cwd(o, env);
    free_text_gate(&cwd, o)?;
    let (config_dir, account) = resolve_account(o, env, table)?;
    // 〔TL3 · §47〕账号配置目录（manifest 里来的）是本仓自管的路径，该走全表。
    // 〔DUP1〕全表原先住 `observe/accounts_query.rs`（`control → observe` 是禁止方向，TL3 在这里先只过了自由文本那一层）；
    //   今天整份搬进共享 crate（`acct_core::config_dir_ok`，全仓唯一一份），这里直接用。空串 = 账号 0 / 继承，不注入、不判。
    if !config_dir.is_empty() && !acct_core::config_dir_ok(&config_dir) {
        return Err(refuse(&env.account_env, &config_dir));
    }
    let launcher = if o.launcher.is_empty() {
        super::default_launcher(&o.agent).to_string()
    } else {
        o.launcher.clone()
    };

    // ── 已在 tmux 里且没给名 ⇒ 就地起（旧 `cct` 那条分支）────────────────
    let mut use_tmux = o.use_tmux;
    if use_tmux && env.tmux.is_some() && o.tmux_name.is_empty() && o.tmux_base.is_empty() {
        // 这条分支会让 `--detach` / `--tmux-size` 双双落空，而调用方要的是
        // 「建完就返回」，拿到的却是**阻塞式 exec** —— 语义完全相反。显式 die。
        if o.detach || !o.tmux_size.is_empty() {
            return Err(Die(copy_text("bePlan.build.inTmuxNoName", &[]).into()));
        }
        use_tmux = false;
    }

    if use_tmux {
        inherited_gate(env)?;
        let (base, step_aside) = if !o.tmux_base.is_empty() {
            validate_tmux_name(&o.tmux_base)?;
            (o.tmux_base.clone(), true)
        } else if !o.tmux_name.is_empty() {
            validate_tmux_name(&o.tmux_name)?;
            (o.tmux_name.clone(), false)
        } else {
            (derive_tmux_name(&cwd), true)
        };
        // ★★ `K-R96`：**退让就在这里发生**，`--ccm-print` 与真跑因此拿到同一个名字。
        let name = match (step_aside, taken) {
            (true, Some(t)) => next_free_name(&base, t.as_slice()),
            // 不退让 / 问不到快照 ⇒ 原样（后者是诚实降级，见本函数头注）。
            _ => base,
        };
        // 内层：同一条命令去掉 `--ccm-tmux`，并把**继承来的**那几个变量显式化。
        // 〔V151〕形状 `self <交给 agent 的…> -- <ccm 自己的…>`（ccm 那一半恒非空 ⇒ 恒带 `--`，透传里有 claude 自己的 `--` 也切得对）。
        let mut opts: Vec<String> = vec![
            flag::CWD.into(),
            cwd.clone(),
            flag::AGENT.into(),
            o.agent.clone(),
        ];
        if !account.is_empty() {
            opts.push(flag::ACCOUNT.into());
            opts.push(account.clone());
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
        let mut inner: Vec<String> = env.self_argv.clone();
        inner.extend(o.passthru.iter().cloned());
        inner.push(flag::END.into());
        inner.extend(opts);
        // 🔴 〔CC1〕**自检那一趟与 pane 里那一趟是同一串 argv**，只在 ccm 那一半末尾多一个 `--ccm-print`。
        //    两条都从这一个 `inner` 渲出来，不许在别处另拼一份。
        let mut dry = inner.clone();
        dry.push(flag::CCM_PRINT.into());
        let mut payload = inner.iter().map(|a| sq(a)).collect::<Vec<_>>().join(" ");
        let bare = payload.clone();
        // 🔴 **把继承来的那几个显式化** —— tmux 的 `update-environment` 默认列表不含它们，
        // 外层那句 `export` 在 tmux 进程边界上会被整个吃掉（账号注入 100% 失效，实测过）。
        // ⚠ 这几行的形状（`payload = format!("export <VAR>={}; {payload}", sq(v));`）有人在逐行认：
        //   `bridge` 那条 `KP5ED1`（`payload_tests·rs::forwarded_by_container_path`）从起点
        //   `let mut payload = inner.iter()` 数到登记那段注释，改形状它会红。
        if account.is_empty() && !o.use_base {
            if let Some(v) = env
                .inherited_config_dir
                .as_deref()
                .filter(|v| !v.is_empty())
            {
                payload = format!("export {}={}; {payload}", env.account_env, sq(v));
            }
        }
        if let Some(v) = env.anthropic_base_url.as_deref().filter(|v| !v.is_empty()) {
            payload = format!("export ANTHROPIC_BASE_URL={}; {payload}", base_url_word(v));
        }
        if let Some(v) = env.ccm_launch_id.as_deref().filter(|v| !v.is_empty()) {
            payload = format!("export CCM_LAUNCH_ID={}; {payload}", sq(v));
        }
        // 自检**共用载荷那一段 export 前缀**（它要在 pane 那份环境里跑）：上面只往前面加，
        // ⇒ 前缀 = 载荷去掉末尾那段裸命令。
        let exports = &payload[..payload.len() - bare.len()];
        let self_check = format!(
            "{exports}{}",
            dry.iter().map(|a| sq(a)).collect::<Vec<_>>().join(" ")
        );

        // 🔴 **要了登记而登记不成，必须出声**〔`K-R48` 第二拍 09-11 补回〕。
        //
        // 旧 bash 实现这两处各有一句 stderr：找不到 cc-bus 脚本目录 ⇒「**没有登记**」；
        // 找得到目录但 `cc-spawned-record` 不可执行 ⇒「**不进 spawn 台账**」。
        // 首版原生实现把这两句**整个丢了** —— `--bus-register` 于是变成一个
        // 「要了、没做、也不说」的旗标，而那正是本工作区反复消灭的那类静默降级。
        // 〔`tests/e2e/cc-spawn-uplift.sh` 的「且没有一声不吭」「明说『不进 spawn 台账』」两组钉着它。〕
        let bus = if o.bus_register {
            match env.bus_scripts.as_ref() {
                None => {
                    eprintln!("{}", copy_text("bePlan.bus.noScripts", &[]));
                    None
                }
                Some(d) => {
                    let has_spawned_record =
                        is_exec(&std::path::Path::new(d).join("cc-spawned-record"));
                    if !has_spawned_record {
                        eprintln!(
                            "{}",
                            copy_text(
                                "bePlan.bus.noSpawnRecord",
                                &[(
                                    "path",
                                    &std::path::Path::new(d)
                                        .join("cc-spawned-record")
                                        .display()
                                        .to_string()
                                )]
                            )
                        );
                    }
                    Some(BusRegister {
                        scripts_dir: d.clone(),
                        note: o.bus_note.clone(),
                        has_spawned_record,
                    })
                }
            }
        } else {
            None
        };

        return Ok(Plan::Container(Container {
            name,
            cwd,
            agent: o.agent.clone(),
            ccm_sid: o.ccm_sid.clone(),
            size: parse_size(&o.tmux_size),
            detach: o.detach,
            payload,
            self_check,
            trust_poll: o.agent == "claude" && !env.no_pretrust,
            bus,
        }));
    }

    // ── 非容器路 ────────────────────────────────────────────────────────
    let mut argv = launcher_words(&launcher, &env.home);
    argv.extend(o.passthru.iter().cloned());

    Ok(Plan::Direct(Direct {
        ccm_env: env.ccm_env.clone(),
        bus_id_recipe: super::needs_bus_id(&o.agent),
        // 与那段配方的 `if [ -n "${TMUX:-}" ]` **同一个判准**：`Env::tmux` 就是
        // `var("TMUX").ok().filter(|v| !v.is_empty())` ⇒ 两侧逐字等价，不是近似。
        inside_tmux: env.tmux.is_some(),
        account_env: env.account_env.clone(),
        config_dir,
        unset_config_dir: o.use_base,
        nested: super::nested_env(&o.agent),
        cwd,
        argv,
        has_identity: super::has_identity(&o.agent),
        identity: direct_identity(&o.ccm_sid, env.launch_token.as_deref()),
    }))
}

/// `--ccm-print`：吐出这一趟的**等价 shell**。全部来自 [`Plan`] ⇒ 与真跑同源。
pub(crate) fn render(plan: &Plan) -> String {
    match plan {
        Plan::Attach { name } => format!("tmux attach -t {}", sq(&format!("={name}:"))),
        // 在不在 tmux 里不进 `--ccm-print`（`INVARIANTS §33a` 铁律 2）⇒ 值不知道就打印配方，真跑也跑这一行。
        Plan::Rejoin {
            name, detach: true, ..
        } => format!("echo {}", sq(&format!("ccm-session={name}"))),
        Plan::Rejoin { name, .. } => {
            let t = sq(&format!("={name}:"));
            format!("if [ -n \"${{TMUX:-}}\" ]; then tmux switch-client -t {t}; else tmux attach -t {t}; fi")
        }
        Plan::Container(c) => render_container(c),
        Plan::Direct(d) => render_direct(d),
    }
}

fn render_container(c: &Container) -> String {
    let t = sq(&format!("={}:", c.name));
    let size = match &c.size {
        Some((w, h)) => format!(" -x {w} -y {h}"),
        None => String::new(),
    };
    let mut seq = format!(
        "{{ tmux new-session -d -s {} -c {}{size} 2>/dev/null",
        sq(&c.name),
        sq(&c.cwd)
    );
    seq.push_str(&format!(
        " || {{ printf {} {} >&2; exit 3; }}; }}",
        sq(&super::NAME_TAKEN_FMT),
        sq(&c.name)
    ));
    seq.push_str(&format!(
        " && (tmux set-option -t {t} @ccm_agent {} 2>/dev/null || true)",
        sq(&c.agent)
    ));
    // 通道 A（意图）写 `@ccm_sid_expect`，**不写** `@ccm_sid` —— 后者是通道 B（事实）的，
    // 破坏性动作只认事实标记，不被「声明了但从未真正跑起来」的会话骗过。
    if !c.ccm_sid.is_empty() {
        seq.push_str(&format!(
            " && (tmux set-option -t {t} @ccm_sid_expect {} 2>/dev/null || true)",
            sq(&c.ccm_sid)
        ));
    }
    seq.push_str(&format!(
        " && tmux send-keys -t {t} {} Enter",
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

/// 容器路的**收尾**：兜底轮询 · attach · cc-bus 登记。
///
/// 建会话与键入载荷在真跑那条路上已经由 [`crate::control::launch`] 在**本进程里**做完了，
/// 剩下这几件仍是本机的事（attach 尤其：后端开不了你面前的窗）。
pub(crate) fn render_container_tail(c: &Container) -> String {
    let t = sq(&format!("={}:", c.name));
    let mut seq = String::new();
    // ★★ 〔CC1〕**先自检，再谈兜底 / 接进去 / 登记** —— 排在收尾的最前面，而且只能在这里。
    //
    // 从前 ccm 的 rc=0 只说明「会话建了、载荷键入了、收尾跑了」，pane 里那一跳一个字都没看：
    // BS1b 现打入口② 那一跳当场「unknown argument: --cwd」退回空 bash，而 cc-spawn 照报成功、
    // 还登记上了总线（**假成功**）。登记就发生在本段下面 ⇒ 确认必须排在它前面、住在 ccm 里；
    // 放进 cc-spawn 只能「先登记、再撤回」。
    //
    // 确认的形状：**用 pane 里那一跳的同一个入口、同一串参数、同一段 export 前缀，加 `--ccm-print` 真跑一次**
    //（[`Container::self_check`]，与载荷出自 `build` 里同一个 `inner`）。等的是**一个进程退出**，
    // 不是一段时间 —— 零定时器、零轮询。它不过 ⇒ 把它自己的原话转给调用方、`exit 4`（起不来），
    // 后面的接进去与登记一段都不跑。
    //
    // ⚠ 它**买到**的：入口路由（BS1b 那一形）· 参数解析 · 账号解析 · 被叫的那个入口（`argv[0]`）是一份不认这套参数的旧副本。
    // ⚠ 它**买不到**的（别读成做到了）：`--ccm-print` 不解析启动器 ⇒ 「启动器在 PATH 上找不到」仍会在 pane 里
    //   退回 shell 而这里照报成功；agent exec 起来之后自己当场退出，同样看不见。要看见这两类得有一个
    //   **exec 那一刻的正信号**（今天没有），登记在 `设计/95` 末尾。
    seq.push_str(&format!(
        " && {{ _ccm_e=$({{ {}; }} 2>&1 >/dev/null) || {{ printf '%s\\n' \"$_ccm_e\" >&2; printf {} {} >&2; exit 4; }}; }}",
        c.self_check,
        sq(&super::SELF_CHECK_FAILED_FMT),
        sq(&c.name)
    ));
    // 🔴 下面那条**自带节拍的 shell 串**不是漏进来的，是 `C14` 逐字登记的那个例外
    //（「预信任的『等信任框』没有内核事件源 …… `C8` 的唯一登记例外：`control/` 继续
    // 以 shell 字符串形态产出它」）。节拍由**目标 shell** 提供，后端进程自己一个定时器都没有。
    // ⚠〔`K-R103` 09-13〕`no_timer_guard::f09` 从今天起**扫得到它**（匹配单位从「行」
    // 改成「表达式」之后，`format!(` 的续行不再掉出人群）⇒ 它在
    // `no_timer_guard::f09_external_beat::REGISTERED_EXTERNAL_BEATS` 上**签了字**。
    // 改这一段之前先看那张表：动了这条串的形状，那边会红。
    if c.trust_poll {
        seq.push_str(&format!(
            " && {{ (for _i in 1 2 3 4 5 6; do sleep 0.5; tmux capture-pane -t {t} -p 2>/dev/null | grep -q 'Yes, I trust this folder' && {{ tmux send-keys -t {t} Enter; break; }}; done) || true; }}"
        ));
    }
    if !c.detach {
        seq.push_str(&format!("; tmux attach -t {t}"));
    }
    if let Some(b) = &c.bus {
        seq.push_str(&format!(
            "; {{ _p=$(tmux list-panes -t {t} -F '#{{pane_id}}' 2>/dev/null | head -1)"
        ));
        seq.push_str(&format!(
            "; [ -n \"$_p\" ] && TMUX_PANE=\"$_p\" {} {} >/dev/null",
            sq(&format!("{}/cc-register", b.scripts_dir)),
            sq(&c.name)
        ));
        if b.has_spawned_record {
            seq.push_str(&format!(
                "; {} {} {} {} >/dev/null; }} || true",
                sq(&format!("{}/cc-spawned-record", b.scripts_dir)),
                sq(&c.name),
                sq(&c.cwd),
                sq(&b.note)
            ));
        } else {
            seq.push_str("; } || true");
        }
    }
    seq
}

fn render_direct(d: &Direct) -> String {
    let mut line = String::new();
    if !d.ccm_env.is_empty() {
        line.push_str(&format!("{}; ", d.ccm_env));
    }
    if d.bus_id_recipe {
        line.push_str(super::BUS_ID_RECIPE);
        line.push(' ');
    }
    let cfg_env = &d.account_env;
    if !d.config_dir.is_empty() {
        line.push_str(&format!("export {cfg_env}={}; ", sq(&d.config_dir)));
    }
    if d.unset_config_dir {
        line.push_str(&format!("unset {cfg_env}; "));
    }
    if !d.nested.is_empty() {
        line.push_str(&format!("unset {}; ", d.nested.join(" ")));
    }
    if !d.cwd.is_empty() {
        line.push_str(&format!("cd {} && ", sq(&d.cwd)));
    }
    let exec = std::iter::once("exec".to_string())
        .chain(d.argv.iter().map(|a| qarg(a)))
        .collect::<Vec<_>>()
        .join(" ");
    line.push_str(&exec);
    line
}

#[cfg(test)]
#[path = "../../../../tests/backend/control/ccm/plan_tests.rs"]
mod tests;
