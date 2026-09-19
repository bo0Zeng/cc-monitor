//! 从一套 [`Opts`] 算出「这一趟到底要干什么」，以及 `--print` 那条等价命令行。
//!
//! # 为什么计划与执行分开
//!
//! `--print` 是这套 CLI 的**平价预言机**：它吐的那一行必须与真跑那一趟**同源**，
//! 否则「print 说的」与「真做的」会各漂各的（`tests/e2e/ccm-contract-parity.sh` 的 A / A′ 两组
//! 整组就是在钉这一条，07-31 真逮到过一次「print 退回读文件」）。
//! ⇒ 这里只产出 [`Plan`]，`--print` 与真跑**读同一个 `Plan`**，结构上不可能分叉。
//!
//! # 本文件不许出现 ccm 旗标的字面量
//!
//! 旗标名的唯一住址是 [`super::argv::flag`]。由
//! `argv::tests::the_ccm_argv_is_parsed_in_exactly_one_place` 机检（`KR48D2`）。

use super::argv::{flag, parse_size, Action, CwdSpec, Die, Opts};
// `K-R96`：铸名避让那张 hash 表的**唯一**来源（字段模块私有 ⇒ 这里造不出第二份）。
use crate::common::session_snapshot::TakenNames;
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
    // 🔴 `K-R58`：这里原来有一个 `workspace: String`（`$CCM_WORKSPACE`，`$HOME` 下裸敲时
    //    的落点）。它**唯一的消费者**就是 [`resolve_cwd`] 里那一档「站在 $HOME 就跳工作区」，
    //    那一档按 `K37` 删了 ⇒ 这个字段跟着删，`CCM_WORKSPACE` 这个环境变量**不再被读**。
    //    留着它会变成「猜」的一个待命开关，而 `KR58D3` 的失效方向逐字就是「把猜挪进别处」。
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
        let cfg_path =
            get("CCM_CONFIG").unwrap_or_else(|| format!("{home}/{}", Defaults::CONFIG_REL));
        if std::path::Path::new(&cfg_path).is_file() {
            eprintln!(
                "ccm: {cfg_path} 在，但本实现**不读它**（旧版是 source 一段 bash，原生实现没有等价物）。\n                 里面那两个值请改成环境变量：CCM_ACCTS_MANIFEST / CCM_ENV。\n                 （`CCM_WORKSPACE` 不用改了：`K-R58` 起 ccm 不再替你跳目录，不给 --cwd 就是当前目录。）"
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
        Self(
            m.accounts
                .into_iter()
                .filter(|a| !a.name.is_empty())
                .collect(),
        )
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
    // ★★ 〔`K-R96` 09-12〕**`avoid_collision` 这个字段删了** —— 散文墓碑留在这里。
    //
    // 它从前的意思是「这个名字撞了要不要退让」，而退让本身发生在 `mod.rs::execute`
    // ——**只在真跑那条路上**，`--print` 吐的是没退让过的名字。
    //
    // 用户 09-12 逐字（`R52` 裁定二）：「**不就是先校验冲突然后取名吗? 搞个 hash 表**不就好了」。
    // ⇒ 退让搬进 [`build`]：它拿一份 [`TakenNames`]（那张 hash 表，来源只有会话快照一处），
    //   **算完就把最终名钉进 `Container::name`**。于是：
    //   ① 「产名」与「避让」不再是两个人干的两件事（前端 `mintTmuxName` 那条纪律的对侧）；
    //   ② `--print` 与真跑吐的是**同一个名字** —— 平价预言机从此在名字这一维上也是平的；
    //   ③ `--print` 的「纯」口径改成**相对于快照**：同一份快照 ＋ 同一份输入 ⇒ 同一份输出。
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
/// 纯函数（已占用的名字由调用方给）—— 这样「退让规则」测得了。
///
/// 〔`K-R96` 09-12〕**谁给那份 `taken`，今天只有一个答案**：[`build`] 从
/// [`TakenNames`] 里拿，而 `TakenNames` 的字段是 `common::session_snapshot` 模块私有的
/// ⇒ 「另起一份名字集合」在类型层面就造不出来。`--print` 与真跑用的是同一份。
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
/// 🔴 **谁要是哪天再往这里加一档非恒等的分支，它必须只对 [`Action::New`] 生效** ——
/// 今天不留那个 `if`，是因为恒等之下它是死代码，而不是因为那条约束过期了。
///
/// 〔一并作废的旧注：从前这里逐字复刻旧 bash `_cc_resolve_target`，并登记着
/// 「自己往上走找 `.git`」与 `git rev-parse --show-toplevel` 在 `GIT_DIR` /
/// `GIT_WORK_TREE` / `GIT_CEILING_DIRECTORIES` 上的那一格不等价 —— 那整条路没了，
/// 那格不等价也跟着没了。〕
pub(crate) fn resolve_cwd(o: &Opts, env: &Env) -> String {
    match &o.cwd_spec {
        CwdSpec::Explicit(d) => d.clone(),
        CwdSpec::Auto => env.pwd.clone(),
    }
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
///
/// `taken` = 那一刻**已被占用的会话名**（`R52` 裁定二那张 hash 表），
/// 来源只有 `common::session_snapshot` 一处；`None` = **问不到**
/// （tmux 起不来 / 没装）⇒ **不退让**，与那份已删的 bash `ccm` 那句
/// `tmux has-session … 2>/dev/null`（问不出来当没占）同义。真撞上了还有
/// `created:false` ⇒ rc=3 那条响亮失败兜底。
///
/// 🔴 **本函数相对 `taken` 是纯的**：同一份快照 ＋ 同一份 `o`/`env`/`table`
/// ⇒ 同一份 `Plan`（`--print` 的「纯」今天就是这个口径）。
pub(crate) fn build(
    o: &Opts,
    env: &Env,
    table: &AccountTable,
    taken: Option<&TakenNames>,
) -> Result<Plan, Die> {
    // ── attach：不起 agent，早于容器逻辑就定了 ──────────────────────────
    if o.action == Action::Attach {
        // `ccm attach foo --tmux --detach` 从前会**静默吞掉** `--detach` 照样 attach。
        // 静默忽略正是本工作区反复消灭的病。
        if o.detach {
            return Err(Die(
                "attach 动作与 --detach 矛盾（attach 的语义就是接进去）".into(),
            ));
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
    if use_tmux && env.tmux.is_some() && o.tmux_name.is_empty() && o.tmux_base.is_empty() {
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
        let (base, step_aside) = if !o.tmux_base.is_empty() {
            validate_tmux_name(&o.tmux_base)?;
            (o.tmux_base.clone(), true)
        } else if !o.tmux_name.is_empty() {
            validate_tmux_name(&o.tmux_name)?;
            (o.tmux_name.clone(), false)
        } else {
            (derive_tmux_name(&cwd), true)
        };
        // ★★ `K-R96`：**退让就在这里发生**，`--print` 与真跑因此拿到同一个名字。
        let name = match (step_aside, taken) {
            (true, Some(t)) => next_free_name(&base, t.as_slice()),
            // 不退让 / 问不到快照 ⇒ 原样（后者是诚实降级，见本函数头注）。
            _ => base,
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
        let mut payload = inner.iter().map(|a| sq(a)).collect::<Vec<_>>().join(" ");
        // 🔴 **把继承来的那几个显式化** —— tmux 的 `update-environment` 默认列表不含它们，
        // 外层那句 `export` 在 tmux 进程边界上会被整个吃掉（账号注入 100% 失效，实测过）。
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
        // 〔`tests/e2e/cc-spawn-uplift.sh` 的「且没有一声不吭」「明说『不进 spawn 台账』」两组钉着它。〕
        let bus = if o.bus_register {
            match env.bus_scripts.as_ref() {
                None => {
                    eprintln!(
                        "ccm: --bus-register 要了登记，但找不到 cc-bus 的脚本（CC_BUS_SCRIPTS / <本程序目录>/cc-bus/scripts / PATH）——**没有登记**"
                    );
                    None
                }
                Some(d) => {
                    let has_spawned_record =
                        is_exec(&std::path::Path::new(d).join("cc-spawned-record"));
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
        resolve_sid: (o.action == Action::Resume && !o.launcher_explicit).then(|| o.sid.clone()),
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
            //〔搬自 `tests/e2e/ccm-contract-parity.sh` A′g 那两条；那套 e2e 的 `shared/ccm` 侧
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
#[path = "../../../../tests/backend/control/ccm/plan_tests.rs"]
mod tests;
