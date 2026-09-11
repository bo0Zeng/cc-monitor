//! 从一套 [`Opts`] 算出「这一趟到底要干什么」，以及 `--print` 那条等价命令行。
//!
//! # 为什么计划与执行分开
//!
//! `--print` 是这套 CLI 的**平价预言机**：它吐的那一行必须与真跑那一趟**同源**，
//! 否则「print 说的」与「真做的」会各漂各的（`e2e/ccm-contract-parity.sh` 的 A / A′ 两组
//! 整组就是在钉这一条，07-31 真逮到过一次「print 退回读文件」）。
//! ⇒ 这里只产出 [`Plan`]，`--print` 与真跑**读同一个 `Plan`**，结构上不可能分叉。
//!
//! # 本文件不许出现 ccm 旗标的字面量
//!
//! 旗标名的唯一住址是 [`super::argv::flag`]。由
//! `argv::tests::the_ccm_argv_is_parsed_in_exactly_one_place` 机检（`KR48D2`）。

use super::argv::{flag, parse_size, Action, CwdSpec, Die, Opts};
use shell_quote_core::posix_quote as sq;

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
    /// 起 agent 前要 eval 的机器级 env 串。
    pub(crate) ccm_env: String,
    /// `$HOME` 下裸敲时的落点。
    pub(crate) workspace: String,
    /// 账号库 manifest 的**完整路径**。
    pub(crate) accts_manifest: String,
    /// **账号维度的载体**：切账号靠改哪个环境变量。由 `mod.rs` 从
    /// `agents::account_env_of(<这一趟的 agent>)` 取来 —— 本文件不认识任何 agent 的名字。
    pub(crate) account_env: String,
    /// 这一趟的 `argv[0]`（内层载荷要用它把自己再叫一次）。
    pub(crate) self_path: String,
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
        let home = std::env::var("HOME").unwrap_or_default();
        let get = |k: &str| std::env::var(k).ok().filter(|v| !v.is_empty());
        // 🔴 **`$CCM_CONFIG` 这一层本实现不认，而且不许静默不认。**
        //
        // 旧实现是 `. "$CCM_CONFIG"` —— 真 source 一段 bash，里面可以写任意 shell。
        // 在原生实现里没有等价物：要么退化成「只认 `KEY=value`」（那是**换了一套语义**
        // 而用户不会知道），要么起一个 shell 去 source 它（那就把刚删掉的 bash 请回来了）。
        // ⇒ 选第三条：**发现它存在就说一句，然后照常跑**。
        // 静默忽略正是本工作区反复消灭的那类病（写了个配置、看起来生效了、其实被吃掉）。
        let cfg_path = get("CCM_CONFIG")
            .unwrap_or_else(|| format!("{home}/{}", Defaults::CONFIG_REL));
        if std::path::Path::new(&cfg_path).is_file() {
            eprintln!(
                "ccm: {cfg_path} 在，但本实现**不读它**（旧版是 source 一段 bash，原生实现没有等价物）。\n                 里面那三个值请改成环境变量：CCM_WORKSPACE / CCM_ACCTS_MANIFEST / CCM_ENV。"
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
            ccm_env: pick("CCM_ENV", Defaults::ENV.to_string()),
            workspace: pick("CCM_WORKSPACE", format!("{home}/{}", Defaults::WORKSPACE_REL)),
            accts_manifest: pick(
                "CCM_ACCTS_MANIFEST",
                format!("{home}/{}", Defaults::ACCTS_MANIFEST_REL),
            ),
            // 继承值与载体名一样，要等**解析完 argv 知道是哪一家**才填得了 ⇒ 由 `mod.rs` 补。
            inherited_config_dir: None,
            account_env: String::new(),
            // 🔴 `CCM_SELF` 优先于 `argv[0]`：内层载荷要用**「我是被当作什么叫的」**那个名字。
            //   `argv[0]` 在「一个二进制多个名字」下拿到的可能是真身路径，而内层要的是
            //   用户 `PATH` 上那个入口 —— 两者在软链 / 别名下不是同一个东西。
            self_path: get("CCM_SELF")
                .unwrap_or_else(|| std::env::args().next().unwrap_or_default()),
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
/// 拼进 seq 里执行时静默失败（整段是 `|| true`）。`e2e/cc-spawn-uplift.sh` 的
/// 「台账脚本不可执行 ⇒ 明说『不进 spawn 台账』」那两格钉的正是它。
fn is_exec(p: &std::path::Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        return std::fs::metadata(p)
            .map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
            .unwrap_or(false);
    }
    #[cfg(not(unix))]
    {
        p.is_file()
    }
}

/// cc-bus 的脚本目录：`CC_BUS_SCRIPTS` → 本二进制旁边的 `cc-bus/scripts` → `PATH`。
///
/// 🔴 **第三档（`PATH`）是 `K-R48` 第二拍补回来的，别再删**：旧 bash `ccm` 住在
/// `shared/`（部署形态下 `~/.claude/skills/ccm`），它的**兄弟目录**正好就是
/// `cc-bus/scripts` ⇒ 第二档几乎总是命中。今天 `ccm` 是后端二进制、住
/// `~/.cc-monitor/bin/` —— **它旁边永远没有 `cc-bus/`** ⇒ 第二档在真实部署里**恒不命中**，
/// 而 `PATH` 那一档是唯一还够得着的。首版漏了它（docstring 写着、实现里没有），
/// 后果是「`--bus-register` 要了登记，却谁也没登记」。
fn discover_bus_scripts() -> Option<String> {
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

/// 账号表。**唯一真相源是那份 manifest** —— 从前 `shared/ccm` 要跨一次进程去问 daemon
/// 才拿得到它（`--list-accounts --accts-dir`），那一整段是 bash 与后端说话的**税**，
/// 不是功能。同一个二进制之下它整块消失。
#[derive(Debug, Clone, Default)]
pub(crate) struct AccountTable(pub(crate) Vec<Account>);

impl AccountTable {
    /// 读那份 manifest。读不到 / 解析不动 ⇒ **空表**，不是失败
    ///（「这台机器没有账号库」是一个合法状态：退化为基座启动器，一个字不说）。
    pub(crate) fn load(manifest_path: &str) -> Self {
        let raw = match std::fs::read_to_string(manifest_path) {
            Ok(s) => s,
            Err(_) => return Self::default(),
        };
        let m: Manifest = serde_json::from_str(&raw).unwrap_or_default();
        Self(m.accounts.into_iter().filter(|a| !a.name.is_empty()).collect())
    }

    /// 名字 → 存在的 configDir。**目录不存在 ⇒ 当作不可用**
    ///（「manifest 里写着」与「盘上真有」是两件事）。
    fn config_dir_of(&self, want: &str) -> Option<String> {
        let a = self.0.iter().find(|a| a.name == want)?;
        let c = a.config_dir.as_deref().filter(|c| !c.is_empty())?;
        std::path::Path::new(c).is_dir().then(|| c.to_string())
    }

    /// 报「不可用」必须同时告诉用户有哪些可用。⚠ 无尾空格。
    fn names(&self) -> String {
        if self.0.is_empty() {
            return "(无账号库)".to_string();
        }
        self.0
            .iter()
            .map(|a| a.name.as_str())
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// `isDefault: true` 的第一个。
    fn default_name(&self) -> Option<&str> {
        self.0
            .iter()
            .find(|a| a.is_default)
            .map(|a| a.name.as_str())
    }
}

/// 一趟要干的事。`--print` 与真跑读的是**同一个**。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Plan {
    /// 不起 agent，直接接回一个既有会话。
    Attach { name: String },
    /// 建一个 tmux 容器，把「同一条命令去掉 `--tmux`」送进去。
    Container(Container),
    /// 在**本进程**里设好环境、`cd`、然后 `exec`。
    Direct(Direct),
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
    /// 要不要挂那段「抓信任框、自动按 Enter」的兜底轮询。
    pub(crate) trust_poll: bool,
    /// 这个名字**撞了要不要退让**。
    ///
    /// 三条取名路的态度**不一样，别合并**：
    /// - 显式 `--tmux=<名>` ⇒ **不退让**（调用方说的就是要这个名；撞了走 `C14` 响亮失败）；
    /// - `--tmux-base=<基名>` ⇒ **退让**（`C15` 给 cc-spawn 的那条路，它随后要读回真名字）；
    /// - 不给名、从 cwd 派生 ⇒ **退让**（幂等接回同一目录的会话，撞了说明有别人占了）。
    ///
    /// ⚠ 它**只在真跑那条路上生效**：`--print` 不查实时 tmux 状态（那是它「纯」的全部含义），
    /// 所以 `--print` 吐的是**没退让过的**名字 —— `shared/ccm::avoid_name_collision`
    /// 那句 `[ "$do_print" != 1 ] && tmux has-session …` 逐字就是这个意思。
    pub(crate) avoid_collision: bool,
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
    /// 账号维度的载体（环境变量名）—— 见 [`Env::account_env`]。
    pub(crate) account_env: String,
    /// 要 export 的账号目录。
    pub(crate) config_dir: String,
    /// `--base`：显式 `unset CLAUDE_CONFIG_DIR`。
    pub(crate) unset_config_dir: bool,
    pub(crate) model: String,
    /// 要 unset 的嵌套标记（claude 四个 / codex 零个）。
    pub(crate) nested: Vec<String>,
    pub(crate) cwd: String,
    /// 最终 exec 的 argv。
    pub(crate) argv: Vec<String>,
    /// 要不要先问后端「这个会话该怎么起」（`resume` 且没给显式 `--launcher`）。
    pub(crate) resolve_sid: Option<String>,
    pub(crate) passthru: Vec<String>,
    /// 这一趟有没有身份面（claude 有、codex 没有）。
    pub(crate) has_identity: bool,
}

/// POSIX argv 元素的**最省引号**写法：能裸写就裸写。
///
/// 与 `--print` 的可读性直接相关：`exec claude --resume abc` 比
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
/// 纯函数（已占用的名字由调用方给）—— 这样「退让规则」测得了，而**查实时 tmux 状态**
/// 那一步留在真跑那条路上（`--print` 不查，见 [`Container::avoid_collision`]）。
pub(crate) fn next_free_name(base: &str, taken: &[String]) -> String {
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

/// 会话名的形状校验（**唯一一份** —— 显式名与基名都过这里）。
pub(crate) fn validate_tmux_name(n: &str) -> Result<(), Die> {
    if n.is_empty() || n.starts_with('-') {
        return Err(Die(format!("非法 tmux 会话名（空或以 - 开头）: '{n}'")));
    }
    if n.chars().any(|c| "*?.:=".contains(c)) {
        return Err(Die(format!(
            "非法 tmux 会话名（含 glob 或 tmux 目标语法 * ? . : =）: '{n}'"
        )));
    }
    if n.chars().any(|c| c.is_control()) {
        return Err(Die(format!("非法 tmux 会话名（含控制字符）: '{n}'")));
    }
    Ok(())
}

/// `auto` 的三条分支。逐字复刻旧 `_cc_resolve_target`。
///
/// ⚠ **auto 只对 `new` 生效**：`resume`/`attach` 的目标目录由 sid / 会话决定、
/// 调用方已经定位好了；再 auto 解析一次会把工作目录换成 git 仓的**父目录**
/// ⇒ claude 按 `projects/<enc(cwd)>/<sid>.jsonl` 找不到会话（实测踩过）。
///
/// ⚠ **与 bash 那版的一处如实差别**：那版调 `git rev-parse --show-toplevel`（起一个进程）；
/// 这里改成**自己往上走找 `.git`**。少一次 `fork`，也少一条外部依赖；
/// 代价是 `GIT_DIR` / `GIT_WORK_TREE` / `GIT_CEILING_DIRECTORIES` 这几个环境变量它不认
/// —— 那几条在「用户在自己的仓里敲 `cc`」这个人群里没有出现过，但**这是一格差别，不是等价**。
pub(crate) fn resolve_cwd(o: &Opts, env: &Env) -> String {
    match &o.cwd_spec {
        CwdSpec::Explicit(d) => return d.clone(),
        CwdSpec::Auto => {}
    }
    if o.action != Action::New {
        return env.pwd.clone();
    }
    if env.pwd == env.home {
        return env.workspace.clone();
    }
    let mut p = std::path::Path::new(&env.pwd);
    loop {
        if p.join(".git").exists() {
            return p
                .parent()
                .map(|x| x.to_string_lossy().to_string())
                .unwrap_or_else(|| env.pwd.clone());
        }
        match p.parent() {
            Some(up) if up != p => p = up,
            _ => break,
        }
    }
    env.pwd.clone()
}

/// 账号解析。三态，**一个字都没改**（这是从 `shared/ccm:996-1015` 搬过来的语义）：
///
/// - 显式 `--account X` ⇒ 必须解析成功，否则**中止**（显式选号绝不静默降级到别的号）。
/// - 显式 `--base` ⇒ 不注入（issue #75 逃生口）。
/// - 都不传 ⇒ **只在调用方没有已选定账号时**才落 manifest 的 `isDefault`。
///
/// 🔴 **最后那一条是一个已知病灶，本轮刻意原样搬过来、并在这里指名它。**
/// 它的形状是：不给 `--account`、不给 `--base`、而 `CLAUDE_CONFIG_DIR` 为空
/// ⇒ 落默认号。`R08` 给它加了 `-z "$CLAUDE_CONFIG_DIR"` 这道闸，于是
/// 「外层已经选好号」的那条路不再被覆盖；但**裸终端**那条路仍然是「替调用方选一个号」。
/// PM 09-11 逐字：这一格「归待问用户」。⇒ **本实现判不了，不自批**：
/// 照搬 = 行为不变，改 = 越权。要改的话落点就在这个函数，别处没有第二处。
pub(crate) fn resolve_account(
    o: &Opts,
    env: &Env,
    table: &AccountTable,
) -> Result<(String, String), Die> {
    if !o.account.is_empty() {
        return match table.config_dir_of(&o.account) {
            Some(d) => Ok((d, o.account.clone())),
            None => Err(Die(format!(
                "账号 '{}' 不可用（不在 {}，或其目录不存在）。可用: {}",
                o.account,
                env.accts_manifest,
                table.names()
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
        None => Err(Die(format!(
            "默认账号 '{def}' 目录不存在（manifest 说它是 isDefault）。用 --base 起基座，或 --account <名> 指定。"
        ))),
    }
}

/// 从 [`Opts`] ＋ 外界 ⇒ [`Plan`]。**这是计划面的唯一入口。**
pub(crate) fn build(o: &Opts, env: &Env, table: &AccountTable) -> Result<Plan, Die> {
    // ── attach：不起 agent，早于容器逻辑就定了 ──────────────────────────
    if o.action == Action::Attach {
        // `ccm attach foo --tmux --detach` 从前会**静默吞掉** `--detach` 照样 attach。
        // 静默忽略正是本工作区反复消灭的病。
        if o.detach {
            return Err(Die("attach 动作与 --detach 矛盾（attach 的语义就是接进去）".into()));
        }
        if !o.tmux_size.is_empty() {
            return Err(Die(
                "attach 动作不支持 --tmux-size（接回既有会话，尺寸归它自己）".into(),
            ));
        }
        return Ok(Plan::Attach {
            name: o.attach_name.clone(),
        });
    }

    let cwd = resolve_cwd(o, env);
    let (config_dir, account) = resolve_account(o, env, table)?;
    let launcher = if o.launcher.is_empty() {
        super::default_launcher(&o.agent).to_string()
    } else {
        o.launcher.clone()
    };

    // ── 已在 tmux 里且没给名 ⇒ 就地起（旧 `cct` 那条分支）────────────────
    let mut use_tmux = o.use_tmux;
    if use_tmux
        && env.tmux.is_some()
        && o.tmux_name.is_empty()
        && o.tmux_base.is_empty()
    {
        // 这条分支会让 `--detach` / `--tmux-size` 双双落空，而调用方要的是
        // 「建完就返回」，拿到的却是**阻塞式 exec** —— 语义完全相反。显式 die。
        if o.detach || !o.tmux_size.is_empty() {
            return Err(Die(
                "已在 tmux 内且未给会话名 → 会就地起而非建容器，--detach/--tmux-size 无法生效。请用 --tmux=<显式名>".into(),
            ));
        }
        use_tmux = false;
    }

    if use_tmux {
        let (name, avoid_collision) = if !o.tmux_base.is_empty() {
            validate_tmux_name(&o.tmux_base)?;
            (o.tmux_base.clone(), true)
        } else if !o.tmux_name.is_empty() {
            validate_tmux_name(&o.tmux_name)?;
            (o.tmux_name.clone(), false)
        } else {
            (derive_tmux_name(&cwd), true)
        };
        // 内层：同一条命令去掉 `--tmux`，并把**继承来的**那几个变量显式化。
        let mut inner: Vec<String> = vec![env.self_path.clone()];
        if o.action == Action::Resume {
            inner.push("resume".into());
            inner.push(o.sid.clone());
        }
        inner.push(flag::CWD.into());
        inner.push(cwd.clone());
        inner.push(flag::AGENT.into());
        inner.push(o.agent.clone());
        if !account.is_empty() {
            inner.push(flag::ACCOUNT.into());
            inner.push(account.clone());
        }
        if o.use_base {
            inner.push(flag::BASE.into());
        }
        if !o.model.is_empty() {
            inner.push(flag::MODEL.into());
            inner.push(o.model.clone());
        }
        if !launcher.is_empty() {
            inner.push(flag::LAUNCHER.into());
            inner.push(launcher.clone());
        }
        if !o.ccm_sid.is_empty() {
            inner.push(flag::CCM_SID.into());
            inner.push(o.ccm_sid.clone());
        }
        if !o.passthru.is_empty() {
            inner.push(flag::END.into());
            inner.extend(o.passthru.iter().cloned());
        }
        let mut payload = inner
            .iter()
            .map(|a| sq(a))
            .collect::<Vec<_>>()
            .join(" ");
        // 🔴 **把继承来的那几个显式化** —— tmux 的 `update-environment` 默认列表不含它们，
        // 外层那句 `export` 在 tmux 进程边界上会被整个吃掉（账号注入 100% 失效，实测过）。
        if account.is_empty() && !o.use_base {
            if let Some(v) = env.inherited_config_dir.as_deref().filter(|v| !v.is_empty()) {
                payload = format!("export {}={}; {payload}", env.account_env, sq(v));
            }
        }
        if let Some(v) = env.anthropic_base_url.as_deref().filter(|v| !v.is_empty()) {
            payload = format!("export ANTHROPIC_BASE_URL={}; {payload}", sq(v));
        }
        if let Some(v) = env.ccm_launch_id.as_deref().filter(|v| !v.is_empty()) {
            payload = format!("export CCM_LAUNCH_ID={}; {payload}", sq(v));
        }

        // 🔴 **要了登记而登记不成，必须出声**〔`K-R48` 第二拍 09-11 补回〕。
        //
        // 旧 bash 实现这两处各有一句 stderr：找不到 cc-bus 脚本目录 ⇒「**没有登记**」；
        // 找得到目录但 `cc-spawned-record` 不可执行 ⇒「**不进 spawn 台账**」。
        // 首版原生实现把这两句**整个丢了** —— `--bus-register` 于是变成一个
        // 「要了、没做、也不说」的旗标，而那正是本工作区反复消灭的那类静默降级。
        // 〔`e2e/cc-spawn-uplift.sh` 的「且没有一声不吭」「明说『不进 spawn 台账』」两组钉着它。〕
        let bus = if o.bus_register {
            match env.bus_scripts.as_ref() {
                None => {
                    eprintln!(
                        "ccm: --bus-register 要了登记，但找不到 cc-bus 的脚本（CC_BUS_SCRIPTS / <本程序目录>/cc-bus/scripts / PATH）——**没有登记**"
                    );
                    None
                }
                Some(d) => {
                    let has_spawned_record = is_exec(&std::path::Path::new(d).join("cc-spawned-record"));
                    if !has_spawned_record {
                        eprintln!(
                            "ccm: {d}/cc-spawned-record 不可执行 —— 会话照建、也会登记，但**不进 spawn 台账**（孤儿检测看不到它）"
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
            trust_poll: o.agent == "claude" && !env.no_pretrust,
            avoid_collision,
            bus,
        }));
    }

    // ── 非容器路 ────────────────────────────────────────────────────────
    let mut argv = vec![launcher.clone()];
    if o.action == Action::Resume {
        if let Some(rf) = super::resume_flag(&o.agent) {
            argv.push(rf.to_string());
            argv.push(o.sid.clone());
        }
    }
    argv.extend(o.passthru.iter().cloned());

    Ok(Plan::Direct(Direct {
        ccm_env: env.ccm_env.clone(),
        bus_id_recipe: super::needs_bus_id(&o.agent),
        account_env: env.account_env.clone(),
        config_dir,
        unset_config_dir: o.use_base,
        model: o.model.clone(),
        nested: super::nested_env(&o.agent),
        cwd,
        argv,
        resolve_sid: (o.action == Action::Resume && !o.launcher_explicit)
            .then(|| o.sid.clone()),
        passthru: o.passthru.clone(),
        has_identity: super::has_identity(&o.agent),
    }))
}

/// `--print`：吐出这一趟的**等价 shell**。
///
/// `resolved` = 后端对 `resume` 那一问的答案（没有就 `None`）。它是**唯一**的外部输入，
/// 其余全部来自 [`Plan`] ⇒ 与真跑同源。
pub(crate) fn render(plan: &Plan, resolved: Option<&str>) -> String {
    match plan {
        Plan::Attach { name } => format!("tmux attach -t {}", sq(&format!("={name}:"))),
        Plan::Container(c) => render_container(c),
        Plan::Direct(d) => render_direct(d, resolved),
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
        sq(super::NAME_TAKEN_FMT),
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
    seq.push_str(&format!(" && tmux send-keys -t {t} {} Enter", sq(&c.payload)));
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

fn render_direct(d: &Direct, resolved: Option<&str>) -> String {
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
    if !d.model.is_empty() {
        line.push_str(&format!("export ANTHROPIC_MODEL={}; ", sq(&d.model)));
    }
    if !d.nested.is_empty() {
        line.push_str(&format!("unset {}; ", d.nested.join(" ")));
    }
    if !d.cwd.is_empty() {
        line.push_str(&format!("cd {} && ", sq(&d.cwd)));
    }
    let local_exec = std::iter::once("exec".to_string())
        .chain(d.argv.iter().map(|a| qarg(a)))
        .collect::<Vec<_>>()
        .join(" ");
    match resolved {
        Some(cmd) if !cmd.is_empty() && d.resolve_sid.is_some() => {
            // 后端答得出「这个会话该怎么起」⇒ 用它那条，透传参数接在后面。
            //
            // 🔴 **`set -f` 不许省。** 后端回的是**一整条命令串**，它要被 shell 拆成词才跑得了
            // （`exec $cmd` 而不是 `exec "$cmd"`）—— 拆词那一步同时会**做路径展开**：
            // 命令里一个 `*` 会被当前目录的文件名改写掉。`set -f` 关掉的正是这一步。
            // ⚠ 它**不**关命令替换：`$(…)` 靠的是「这条串没有再经过 `eval`」，
            // 而这里也确实没有 —— 两条各守一半，别把其中一条读成两条都买到了。
            //〔搬自 `e2e/ccm-contract-parity.sh` A′g 那两条；那套 e2e 的 `shared/ccm` 侧
            //  逐字也是 `set -f; exec $_ccm_c`。〕
            let pt = d
                .passthru
                .iter()
                .map(|a| qarg(a))
                .collect::<Vec<_>>()
                .join(" ");
            line.push_str("set -f; exec ");
            line.push_str(cmd);
            if !pt.is_empty() {
                line.push(' ');
                line.push_str(&pt);
            }
        }
        _ => line.push_str(&local_exec),
    }
    line
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::control::ccm::argv::{parse, Parsed};

    fn env() -> Env {
        Env {
            home: "/home/pi".into(),
            pwd: "/p".into(),
            workspace: "/home/pi/claude-conversation".into(),
            accts_manifest: "/nonexistent/accounts.json".into(),
            account_env: "CLAUDE_CONFIG_DIR".into(),
            self_path: "/usr/local/bin/ccm".into(),
            ..Default::default()
        }
    }

    fn plan_of(args: &[&str], env: &Env, t: &AccountTable) -> Plan {
        let a: Vec<String> = args.iter().map(|s| s.to_string()).collect();
        match parse(&a).expect("该解析得动") {
            Parsed::Opts(o) => build(&o, env, t).expect("该算得出计划"),
            other => panic!("{other:?}"),
        }
    }

    fn printed(args: &[&str]) -> String {
        render(&plan_of(args, &env(), &AccountTable::default()), None)
    }

    /// 〔搬自 `ccm-cli`「零修饰」「resume <sid>」「--launcher 覆盖」「--base」「--model」
    /// 「--agent codex」「-- 透传」与 `ccm-contract-parity` B 组的顺序那几条〕
    ///
    /// 这一条钉的是**一条起会话命令长什么样**：段的顺序就是契约
    /// （CCM_ENV → CC_BUS_ID → 账号目录 → 模型 → 清嵌套 → cd → exec）。
    #[test]
    fn the_shape_of_one_launch_command_line() {
        let nested = "unset CLAUDECODE CLAUDE_CODE_ENTRYPOINT CLAUDE_CODE_SESSION_ID CLAUDE_CODE_CHILD_SESSION";
        assert_eq!(
            printed(&["--cwd", "/p"]),
            format!("{nested}; cd '/p' && exec claude")
        );
        assert_eq!(
            printed(&["resume", "abc-123", "--cwd", "/p", "--launcher", "claude"]),
            format!("{nested}; cd '/p' && exec claude --resume abc-123")
        );
        assert_eq!(
            printed(&["--cwd", "/p", "--base"]),
            format!("unset CLAUDE_CONFIG_DIR; {nested}; cd '/p' && exec claude")
        );
        assert_eq!(
            printed(&["--cwd", "/p", "--model", "opus"]),
            format!("export ANTHROPIC_MODEL='opus'; {nested}; cd '/p' && exec claude")
        );
        // codex：换启动器 + **不清** claude 的嵌套标记 + cc-bus 身份配方
        assert_eq!(
            printed(&["--cwd", "/p", "--agent", "codex"]),
            format!("{} cd '/p' && exec codex", super::super::BUS_ID_RECIPE)
        );
        // 透传参数含特殊字符 ⇒ 正确 quote
        assert_eq!(
            printed(&["--cwd", "/p", "--", "-p", "hi there"]),
            format!("{nested}; cd '/p' && exec claude -p 'hi there'")
        );
    }

    /// 〔搬自 `ccm-contract-parity`「claude 不得被注入 CC_BUS_ID」〕
    #[test]
    fn only_codex_gets_the_bus_id_recipe() {
        assert!(!printed(&["--cwd", "/p"]).contains("CC_BUS_ID"));
        assert!(printed(&["--cwd", "/p", "--agent", "codex"]).contains("CC_BUS_ID"));
    }

    /// 〔搬自 `ccm-cli`「--account 与 --model 组合：账号目录先、模型偏好次」〕
    ///
    /// **顺序即契约**（见 `launch-dimensions.ts` 的 order）。
    #[test]
    fn the_account_dir_comes_before_the_model() {
        let d = tempdir();
        let t = table(&[("z", Some(d.as_str()), true)]);
        let p = plan_of(&["--cwd", "/p", "--account", "z", "--model", "opus"], &env(), &t);
        let line = render(&p, None);
        let i_acct = line.find("CLAUDE_CONFIG_DIR").expect("该有账号目录");
        let i_model = line.find("ANTHROPIC_MODEL").expect("该有模型");
        assert!(i_acct < i_model, "账号目录必须排在模型之前：{line}");
    }

    /// 〔搬自 `ccm-cli` 账号那一族：显式 / 继承 / 默认号 / --base 四条路〕
    ///
    /// 🔴 最后那条（裸终端落默认号）**是原样搬过来的已知病灶**，见 [`resolve_account`] 头注。
    #[test]
    fn the_four_ways_an_account_gets_picked() {
        let dz = tempdir();
        let db = tempdir();
        let t = table(&[("z", Some(dz.as_str()), true), ("b", Some(db.as_str()), false)]);
        let mut e = env();
        // ① 显式 --account 赢
        assert!(render(&plan_of(&["--cwd", "/p", "--account", "b"], &e, &t), None)
            .contains(&format!("export CLAUDE_CONFIG_DIR='{db}'")));
        // ② 裸终端（无继承）⇒ 落 manifest 默认号 z
        assert!(render(&plan_of(&["--cwd", "/p"], &e, &t), None)
            .contains(&format!("export CLAUDE_CONFIG_DIR='{dz}'")));
        // ③ 外层已继承 ⇒ **保留继承的**，不被默认号静默覆盖（`R08` 的原病）
        e.inherited_config_dir = Some(db.clone());
        let line = render(&plan_of(&["--cwd", "/p"], &e, &t), None);
        assert!(!line.contains("export CLAUDE_CONFIG_DIR"), "不许覆盖继承：{line}");
        // ④ --base 显式清空，不受继承影响
        assert!(render(&plan_of(&["--cwd", "/p", "--base"], &e, &t), None)
            .contains("unset CLAUDE_CONFIG_DIR"));
        // ⑤ 显式 --account 压过继承
        assert!(render(&plan_of(&["--cwd", "/p", "--account", "z"], &e, &t), None)
            .contains(&format!("export CLAUDE_CONFIG_DIR='{dz}'")));
    }

    /// 〔搬自 `ccm-cli`「账号不存在 → 中止」「可用列表」「无账号库 → 退化为基座」〕
    #[test]
    fn picking_an_account_never_falls_back_to_a_different_one() {
        let dz = tempdir();
        let t = table(&[("z", Some(dz.as_str()), true), ("f", None, false)]);
        let a: Vec<String> = ["--cwd", "/p", "--account", "nope"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let o = match parse(&a).expect("解析得动") {
            Parsed::Opts(o) => o,
            other => panic!("{other:?}"),
        };
        let Die(msg) = build(&o, &env(), &t).expect_err("不存在的账号必须中止");
        assert!(msg.starts_with("账号 'nope' 不可用"), "{msg}");
        assert!(msg.contains("可用: z f"), "报不可用必须说出有哪些可用：{msg}");
        // 无账号库 ⇒ 一个字都不说，退化为基座（没有 CLAUDE_CONFIG_DIR 注入）
        let empty = AccountTable::default();
        assert!(!render(&plan_of(&["--cwd", "/p"], &env(), &empty), None)
            .contains("CLAUDE_CONFIG_DIR="));
    }

    /// 〔搬自 `ccm-cli`「deriveTmuxName 对拍」那 5 条（跨语言双写点的**本侧**）〕
    ///
    /// ⚠ **如实边界**：bash 那版是拿 `npx tsx` 真跑前端那个函数来对拍的（跨语言）。
    /// 这里只钉**本侧**的规则，**跨语言那一半没了** —— 登记在件文件 `§8`，不许读成等价。
    #[test]
    fn the_session_name_derivation_rule() {
        assert_eq!(derive_tmux_name("/home/pi/proj"), "proj-cc");
        assert_eq!(derive_tmux_name("/home/pi/a  b"), "a-b-cc");
        assert_eq!(derive_tmux_name("/home/pi/proj///"), "proj-cc");
        assert_eq!(derive_tmux_name("/"), "session-cc");
        assert_eq!(derive_tmux_name("/home/pi/.hidden.dir"), "hidden-dir-cc");
        // 截 32 之后再剥首尾 `-`（顺序承重：先剥后截会留下一个尾 `-`）
        assert_eq!(derive_tmux_name(&format!("/x/{}", "a".repeat(40))), format!("{}-cc", "a".repeat(32)));
    }

    /// 〔搬自 `ccm-cli` / `cc-spawn-uplift` 的取名那一族〕**三条取名路的退让态度不一样。**
    ///
    /// 🔴 这一条是**自查逮到的**（铁律 15 那一拍）：头一版原生实现**整个没有退让**，
    /// 于是 `--tmux-base=<基名>`（`C15` 给 cc-spawn 的那条路，它的全部意义就是「撞了就退让」）
    /// **静默退化成了 `--tmux=<名>`** —— 写了个修饰、看起来生效了、实际被吃掉。
    #[test]
    fn only_two_of_the_three_naming_paths_step_aside_on_a_collision() {
        let e = env();
        let t = AccountTable::default();
        let path = |args: &[&str]| match plan_of(args, &e, &t) {
            Plan::Container(c) => (c.name, c.avoid_collision),
            other => panic!("该是容器路：{other:?}"),
        };
        // 显式名：**不退让** —— 调用方说的就是要这个名，撞了走 C14 响亮失败
        assert_eq!(path(&["--tmux=n1", "--cwd", "/p"]), ("n1".into(), false));
        // 基名：退让（cc-spawn 随后要读回真名字）
        assert_eq!(path(&["--tmux-base", "n1", "--cwd", "/p"]), ("n1".into(), true));
        // 不给名、从 cwd 派生：退让
        assert_eq!(path(&["--tmux", "--cwd", "/x/proj"]), ("proj-cc".into(), true));
        // 退让规则本身
        assert_eq!(next_free_name("n1", &[]), "n1");
        assert_eq!(next_free_name("n1", &["other".into()]), "n1");
        assert_eq!(next_free_name("n1", &["n1".into()]), "n1-2");
        assert_eq!(next_free_name("n1", &["n1".into(), "n1-2".into()]), "n1-3");
        // 只撞中间那个不影响：2 空着就取 2
        assert_eq!(next_free_name("n1", &["n1".into(), "n1-3".into()]), "n1-2");
    }

    /// 〔搬自 `ccm-cli` 名字校验那一族〕—— 会话名会被拼进 tmux 目标语法，是一条注入面。
    #[test]
    fn a_session_name_that_would_confuse_tmux_is_refused() {
        assert!(validate_tmux_name("ok-name").is_ok());
        for bad in ["", "-lead", "a*b", "a?b", "a.b", "a:b", "a=b", "a\u{1}b"] {
            assert!(validate_tmux_name(bad).is_err(), "'{bad}' 不该被放行");
        }
    }

    /// 〔搬自 `ccm-print-parity` 全 5 个场景 ＋ `ccm-cli` R08 那 5 条〕
    ///
    /// 容器路：外层 tmux 编排必须带上会话名 / cwd / 意图标 / 内层载荷，
    /// 而**内层载荷必须显式带上账号**（不能靠继承穿 tmux 边界 —— `R08` 的原病）。
    #[test]
    fn the_container_path_carries_every_intent_inward() {
        let db = tempdir();
        let t = table(&[("z", Some(db.as_str()), true), ("b", Some(db.as_str()), false)]);
        let mut e = env();
        e.inherited_config_dir = Some(db.clone());
        let p = plan_of(
            &["resume", "p1", "--tmux=cc-p1", "--cwd", "/tmp", "--ccm-sid", "p1", "--base"],
            &e,
            &t,
        );
        let out = render(&p, None);
        let Plan::Container(c) = &p else { panic!("该是容器路：{p:?}") };
        assert!(out.contains("-s 'cc-p1'"), "会话名没进 new-session：{out}");
        assert!(out.contains("-c '/tmp'"), "cwd 没进 -c：{out}");
        assert!(out.contains("@ccm_sid_expect 'p1'"), "意图标没打：{out}");
        assert!(
            !out.contains("@ccm_sid ") && !out.contains("@ccm_sid'"),
            "不许写**事实**标记 @ccm_sid（那是通道 B 的，破坏性动作只认它）：{out}"
        );
        // 内层载荷：按 argv 元素逐个 quote 过一层，所以判的是 payload 本身
        assert!(c.payload.contains("'resume' 'p1'"), "resume 没进内层：{}", c.payload);
        assert!(c.payload.contains("'--base'"), "--base 没进内层（账号维度恒显式表态）：{}", c.payload);
        assert!(c.payload.contains("'--ccm-sid' 'p1'"), "{}", c.payload);
        // 〔搬自 `ccm-print-parity` 场景 newTmuxCustomLauncher / resumeTmuxWithModel〕
        let p4 = plan_of(
            &["--tmux=n4", "--cwd", "/p", "--launcher", "CCMPROBE", "--model", "opus"],
            &env(),
            &AccountTable::default(),
        );
        let Plan::Container(c4) = &p4 else { panic!("该是容器路") };
        assert!(c4.payload.contains("'--launcher' 'CCMPROBE'"), "{}", c4.payload);
        assert!(c4.payload.contains("'--model' 'opus'"), "{}", c4.payload);
        // 继承账号那条路：内层必须显式 export 继承来的那个目录
        let p2 = plan_of(&["--tmux=n1", "--cwd", "/p"], &e, &t);
        let Plan::Container(c2) = &p2 else { panic!("该是容器路") };
        assert!(
            c2.payload.starts_with(&format!("export CLAUDE_CONFIG_DIR={}; ", sq(&db))),
            "内层没把继承来的账号显式化（tmux 边界会吃掉它）：{}",
            c2.payload
        );
        // 而且绝不能落到默认号 z 上
        assert!(!c2.payload.contains("'--account'"), "不许悄悄换成默认号：{}", c2.payload);
        // 裸终端（无继承）⇒ 内层仍落默认号 z（粘滞体验）
        let mut e2 = env();
        e2.inherited_config_dir = None;
        let p3 = plan_of(&["--tmux=n1", "--cwd", "/p"], &e2, &t);
        let Plan::Container(c3) = &p3 else { panic!("该是容器路") };
        assert!(c3.payload.contains("'--account' 'z'"), "裸终端该落默认号：{}", c3.payload);
    }

    /// 〔搬自 `ccm-print-parity`「含空格 cwd 正确带引号」与 `ccm-cli` 的 quote 那族〕
    #[test]
    fn every_value_that_reaches_a_shell_is_quoted() {
        let out = render(&plan_of(&["--tmux", "--cwd", "/home/pi/my proj"], &env(), &AccountTable::default()), None);
        assert!(out.contains("-c '/home/pi/my proj'"), "{out}");
        assert!(out.contains("-s 'my-proj-cc'"), "{out}");
        assert_eq!(qarg("a b"), "'a b'");
        assert_eq!(qarg("ok-1.2/x"), "ok-1.2/x");
        assert_eq!(qarg(""), "''");
        assert_eq!(qarg("it's"), "'it'\\''s'");
    }

    /// 〔搬自 `ccm-print-parity`「attach 到 cc-p1」〕—— `=名:` 是 tmux 的**精确匹配**形。
    #[test]
    fn attach_uses_the_exact_match_target() {
        assert_eq!(printed(&["attach", "cc-p1"]), "tmux attach -t '=cc-p1:'");
    }

    /// 〔搬自 `ccm-cli`「布局 1–5」〕—— `auto` 的三条分支。
    #[test]
    fn auto_cwd_has_exactly_three_branches() {
        let mut e = env();
        // 布局1：在 $HOME ⇒ 工作区
        e.pwd = e.home.clone();
        assert_eq!(cwd_of(&["new"], &e), e.workspace);
        // 布局4：非 git 目录 ⇒ 目录自己（用一个真实存在但没有 .git 的临时目录）
        let d = tempdir();
        e.pwd = d.clone();
        assert_eq!(cwd_of(&["new"], &e), d);
        // 布局2/3：git 仓（根 / 子目录）⇒ 仓的**父目录**
        std::fs::create_dir_all(format!("{d}/repo/sub")).expect("造夹具");
        std::fs::write(format!("{d}/repo/.git"), "gitdir: /elsewhere").expect("造夹具");
        e.pwd = format!("{d}/repo");
        assert_eq!(cwd_of(&["new"], &e), d);
        e.pwd = format!("{d}/repo/sub");
        assert_eq!(cwd_of(&["new"], &e), d);
        // resume/attach **不做 auto 解析**（cc-monitor 已经 cd 到会话目录了）
        e.pwd = format!("{d}/repo");
        assert_eq!(cwd_of(&["resume", "s"], &e), format!("{d}/repo"));
    }

    fn cwd_of(args: &[&str], e: &Env) -> String {
        let a: Vec<String> = args.iter().map(|s| s.to_string()).collect();
        match parse(&a).expect("解析得动") {
            Parsed::Opts(o) => resolve_cwd(&o, e),
            other => panic!("{other:?}"),
        }
    }

    /// 〔搬自 `ccm-contract-parity` B 组「CCM_ENV 被 eval / --print 里也在 / 早于会话级 env」〕
    ///
    /// `CCM_ENV` 是**机器级** env（代理等，旧 `CC_ENV` 的搬家）：它必须排在会话级 env
    /// **之前**，否则 `--account` 想覆盖它时反而被它盖回去。差分对顺序失明 ⇒ 单钉。
    #[test]
    fn the_machine_level_env_comes_first_and_the_session_level_one_wins() {
        let dz = tempdir();
        let t = table(&[("z", Some(dz.as_str()), true)]);
        let mut e = env();
        e.ccm_env = "export CCM_ENV_PROBE=from-ccm-env".into();
        let line = render(&plan_of(&["--cwd", "/p", "--account", "z"], &e, &t), None);
        assert!(line.starts_with("export CCM_ENV_PROBE=from-ccm-env; "), "{line}");
        let i_env = line.find("CCM_ENV_PROBE").expect("该有机器级 env");
        let i_acct = line.find("CLAUDE_CONFIG_DIR").expect("该有账号目录");
        assert!(i_env < i_acct, "机器级 env 必须排在会话级之前：{line}");
    }

    /// 🔴 〔搬自 `ccm-contract-parity` A′g 那两条〕**后端回的那条命令串不许被 shell 改写。**
    ///
    /// 它要被拆成词才跑得了（`exec $cmd`），而拆词那一步会做路径展开 ——
    /// 命令里一个 `*` 会被当前目录的文件名顶掉。`set -f` 关掉的正是这一步。
    #[test]
    fn a_command_from_the_backend_is_never_rewritten_by_the_shell() {
        let p = plan_of(&["resume", "abc-123", "--cwd", "/p"], &env(), &AccountTable::default());
        let line = render(&p, Some("claude --resume abc-123 --glob *"));
        assert!(
            line.contains("set -f; exec claude --resume abc-123 --glob *"),
            "少了 `set -f` ⇒ 那个 `*` 会被 cwd 的文件名改写：{line}"
        );
        // 反向：没有后端答案时走本地那条，argv 逐个 quote，本来就不经拆词
        let local = render(&p, None);
        assert!(!local.contains("set -f"), "本地那条不需要 set -f：{local}");
        assert!(local.ends_with("exec claude --resume abc-123"), "{local}");
    }

    /// 〔搬自 `ccm-contract-parity` A / A′ 两组「print↔exec 一致」〕
    ///
    /// 从前那两组要**真跑一趟**再与 `--print` 差分，因为两条路是两份代码。
    /// 今天它们读的是**同一个 [`Plan`]** ⇒ 这条判据钉的是那个结构事实：
    /// 渲染函数的全部输入只有 `Plan` 与 `resolved`，没有第二个来源。
    #[test]
    fn print_and_exec_cannot_drift_because_they_read_the_same_plan() {
        let p = plan_of(&["--cwd", "/p", "--model", "opus"], &env(), &AccountTable::default());
        assert_eq!(render(&p, None), render(&p, None), "渲染必须是纯函数");
        let Plan::Direct(d) = &p else { panic!("该是 Direct") };
        // 真跑那一侧读的就是这几个字段（`run::exec_direct`），逐个在这里点名。
        assert_eq!(d.model, "opus");
        assert_eq!(d.cwd, "/p");
        assert_eq!(d.argv, vec!["claude".to_string()]);
    }

    // ── 夹具 ────────────────────────────────────────────────────────────
    fn tempdir() -> String {
        let p = std::env::temp_dir().join(format!(
            "ccm-plan-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("时钟")
                .as_nanos()
        ));
        std::fs::create_dir_all(&p).expect("造夹具目录");
        p.to_string_lossy().to_string()
    }

    fn table(rows: &[(&str, Option<&str>, bool)]) -> AccountTable {
        AccountTable(
            rows.iter()
                .map(|(n, c, d)| Account {
                    name: (*n).to_string(),
                    config_dir: c.map(|x| x.to_string()),
                    is_default: *d,
                })
                .collect(),
        )
    }

    /// 账号表的**读法只有一处**：那份 manifest。〔承接 `ccm-cli` KCY1 那一族的语义半〕
    #[test]
    fn the_account_table_has_exactly_one_source() {
        let d = tempdir();
        let m = format!("{d}/accounts.json");
        std::fs::write(
            &m,
            format!(r#"{{"accounts":[{{"name":"z","configDir":"{d}","isDefault":true}}]}}"#),
        )
        .expect("造夹具");
        let t = AccountTable::load(&m);
        assert_eq!(t.0.len(), 1);
        assert_eq!(t.default_name(), Some("z"));
        assert_eq!(t.config_dir_of("z"), Some(d.clone()));
        // manifest 不在 ⇒ **空表**，不是失败（那台机器就是没有账号库）
        assert!(AccountTable::load("/nonexistent/accounts.json").0.is_empty());
        // manifest 里写着、盘上没有 ⇒ 当作不可用（目录存在性自己判）
        let m2 = format!("{d}/a2.json");
        std::fs::write(&m2, r#"{"accounts":[{"name":"g","configDir":"/nonexistent/gone"}]}"#)
            .expect("造夹具");
        assert_eq!(AccountTable::load(&m2).config_dir_of("g"), None);
    }
}
