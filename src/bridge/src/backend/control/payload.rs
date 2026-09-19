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
//! daemon 的 `launch`（U8a-2b）**零生产调用方**、它**结构上也不 attach**（平面 ③）。
//! 🔴 **`设计/50`（删用量）订正上一句的举例**：原话举的例子是 `account_usage.rs` 那个
//! 用量探针构造器 —— 用量 ②③ 两轴整轴退役之后**那个例子本身没了**，
//! 而**本模块的结论一个字没变**：`launch` 那条仍然不 attach，外层没有整个退役。
//! 逐格实况与量法见 `src/doc/INVARIANTS.md` §33b。
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
//! - 生产消费方今天有两个：`history.rs` 的 POSIX 分支（只用 [`config_dir_prefix_posix`]）、
//!   以及 `backend/control/launch_wire.rs` 的 `render_launch_payload`（`container:"none"` 那格）。
//!   〔`设计/50`：原先的第三个是 `account_usage.rs` 的用量探针（`usage_probe_payload`），  〔散文墓碑〕
//!   随用量 ③ 轴整轴退役。〕
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
/// 记在 `src/doc/INVARIANTS.md` §33b；U8c-2/3 收编 TS 时要一并把那边也改成 fail-closed。
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
/// ⚠ 它与 `src/backend/relay/server.rs::DEFAULT_PORT` 是**同一个数字的两处写法**，
/// 而两处**今天不由任何东西对拍**。之所以不疼：起中转那条路**显式传** `CCM_RELAY_PORT`
/// ⇒ 子进程用的是这里这个值，daemon 那个默认值在这条路上根本不参与。
/// **端口通告面本件不做**（`§0e` 裁五，跟进件 `己1-f26`）——
/// ⇒ 「同机两个 monitor」这一形今天是：第二个中转绑不上、**退 2 并出声**，不静默。
pub const RELAY_PORT: u16 = 8788;

/// 一段路由键里允许的字符 —— **与 `src/backend/relay/route.rs::segment_is_safe`
/// 是同一条规则**（白名单，不是黑名单；`.` 与 `/` 都不在里面 ⇒ `..` 构造不出来）。
///
/// # ⚠ 它是**第二份实现**，这件事必须说清楚，不许读成「共用了一份」
///
/// 两侧分家的原因是结构性的：`src/backend` 依赖 `src/bridge/crates/*`（单向），
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
/// ⚠ `src/doc/INVARIANTS.md §36` 那条铁律**只绑 Windows**（`P3t` 收窄过），而本函数正是
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
#[path = "../../../../../tests/bridge/backend/control/payload_tests.rs"]
mod tests;
