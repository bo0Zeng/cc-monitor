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
//! backend 的 `launch`（U8a-2b）**零生产调用方**、它**结构上也不 attach**（平面 ③）。
//! 🔴 **`设计/50`（删用量）订正上一句的举例**：原话举的例子是 `account_usage.rs` 那个
//! 用量探针构造器 —— 用量 ②③ 两轴整轴退役之后**那个例子本身没了**，
//! 而**本模块的结论一个字没变**：`launch` 那条仍然不 attach，外层没有整个退役。
//! 逐格实况与量法见 `src/doc/INVARIANTS.md` §33b。
//!
//! # 它为什么住在这里（P4b，§1.4b）
//!
//! 它原来在共享 crate（当时叫 `launch-core`）里 —— 而 **backend 对它零引用**。放在那儿的真实原因是
//! 「monitor 侧当时一个边界都没有，没处放」。P4a 划出 `backend/control/` 之后它回到了归属地：
//! §1.3 把最终 exec 钉在**用户自己的终端进程**里，U8a-2b 把后端的执行面定成
//! **argv 直传、不过 shell** ⇒ **「渲染一条 shell 命令串」永远属于开终端的那一侧。**
//!
//! 唯一留在共享 crate 里的是 [`shell_quote_core::posix_quote`]（backend 的 `tmux_hook` 真的在用）。
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
/// TS 侧 `sendIntoViaBackend` 的 catch 此前把两件事混成一件：**IPC 异常**（后端崩/序列化坏）
/// 与**载荷渲染被拒**。它的注释推理「两者都在后端那一跳之前 ⇒ 能证明什么都没发出去
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

use crate::copy_table::copy_text;
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
                return Err(refuse(&copy_text("rsPayload.configDir.emptyNamed", &[])));
            }
            if !config_dir_command_safe(d) {
                return Err(refuse(copy_text(
                    "rsPayload.configDir.bad",
                    &[("value", &format!("{:?}", d))],
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

/// 启动期令牌的长度 —— **32 个字符**。
///
/// 单独提成常量是为了让 TS 那侧的对拍判据**从本文件抽这个数**、而不是手抄一个 32
/// （`tests/launch-render-fallback.vitest.ts`）。改这个数 ⇒ TS 那条对拍当场红。
pub const RBIND_TOKEN_LEN: usize = 32;

/// 令牌形状：恰好 [`RBIND_TOKEN_LEN`] 个**小写**十六进制字符。
///
/// **不收大写**（`b'A'..=b'F'` 刻意不在放行集里）：形状只有一种写法，
/// 好让本地那张 `token → HWND` 表与从 `environ` 读回来的串能直接相等比较，
/// 中间不留归一化步骤 —— 归一化是「两侧各写一遍、各写错一遍」的经典落点。
///
/// ⚠ 这条**不是转义**：渲染时照样过 `posix_quote`（同 `ExportModel`）。
/// 「值的形状」与「拼进 shell 安不安全」在本仓是两道闸，不许合并成一道。
pub fn rbind_token_shape_ok(token: &str) -> bool {
    token.len() == RBIND_TOKEN_LEN
        && token
            .bytes()
            .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

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
    /// `设计/80 §8` 步 1：启动期令牌 `CCM_RBIND_TOKEN`（`[0-9a-f]{32}`）。
    ///
    /// 「买到什么 / **买不到什么**」逐字住 TS `launch-plan.ts::EnvOp` 那一段。
    /// 本侧只重复一句要害：**它只是一个不可猜的关联 id，不许承载任何权限语义** ——
    /// 它会进远端的 `/proc/<pid>/environ` 与 `cmdline`，拿到它顶多能让某人的
    /// `↗` 拉错窗口，不能越权。
    ///
    /// ⚠ 形状校验在 [`render_env_ops`] 里、是 **fail-closed 的 `Err`**（不是「宽容渲染」）。
    /// 这与 `ExportModel` 那一格**刻意不同**：模型名渲错了远端 `claude` 会自己报错，
    /// 而令牌渲错了是**静默**的（`↗` 从此拉不到窗口，且归因指向别处）。
    ExportRbindToken {
        value: &'a str,
    },
    /// 〔RL1 · 第四波〕中转地址 `ANTHROPIC_BASE_URL`：远端（与本机「就地 resume」那一格）拉起时，
    /// 值由 [`relay_endpoint_for`] 那个唯一判断口答出、经 tauri `relay_endpoint_for_launch` 交给前端、再原样放进载荷。
    ///
    /// ⚠ 形状校验在 [`render_env_ops`] 里、**fail-closed**（[`relay_base_url_shape_ok`]）：只收
    /// [`relay_base_url_in`] 产得出的那一形。渲错了的症状是「claude 每一发都连不上」，与网络故障同形 ——
    /// 与启动期令牌那一格同一条理由（静默的错不许渲）。渲染只经 [`relay_env_prefix_posix`]
    /// （本文件唯一产出那句 `export` 的地方，判据数着）。
    ExportRelayBaseUrl {
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
                    return Err(refuse(&copy_text("rsPayload.configDir.emptyString", &[])));
                }
                if !config_dir_command_safe(value) {
                    return Err(refuse(copy_text(
                        "rsPayload.configDir.bad",
                        &[("value", &format!("{:?}", value))],
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
            EnvOp::ExportRbindToken { value } => {
                // ★ fail-closed：形状不对**不许渲染成"这次不带令牌"**，也不许照拼 ——
                //   前者把一次铸币 bug 变成「↗ 不明原因失效」，后者把一个未校验的串
                //   送进远端 shell。理由见 `EnvOp::ExportRbindToken` 的文档注释。
                if !rbind_token_shape_ok(value) {
                    return Err(refuse(copy_text(
                        "rsPayload.rbindToken.bad",
                        &[("value", &format!("{:?}", value))],
                    )));
                }
                let _ = write!(
                    out,
                    "export CCM_RBIND_TOKEN={}; ",
                    shell_quote_core::posix_quote(value)
                );
            }
            EnvOp::ExportRelayBaseUrl { value } => {
                if !relay_base_url_shape_ok(value) {
                    return Err(refuse(copy_text(
                        "rsPayload.relayUrl.bad",
                        &[("value", &format!("{:?}", value))],
                    )));
                }
                out.push_str(&relay_env_prefix_posix(value));
            }
            EnvOp::UnsetConfigDir => out.push_str(UNSET_CONFIG_DIR_PREFIX),
            EnvOp::UnsetNestedEnv { keys } => {
                if keys.is_empty() {
                    return Err(refuse(&copy_text("rsPayload.nestedEnv.empty", &[])));
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
            return Err(refuse(copy_text(
                "rsPayload.arg.notSafe",
                &[("arg", &format!("{:?}", a))],
            )));
        }
    }
    // ★ **launcher 也要过一道**〔audit-0805 08-08〕：本函数对 `args` 逐个过白名单，
    // 而 `launcher` 此前**一个检查都没有** —— 它被直接拼进 `argv` 再 `join(" ")`。
    // 这条路的上游是 tauri 命令 `render_launch_payload`：`launcher` 来自 webview。
    //
    // ⚠⚠ **08-08 订正赌注**（本条建成的次轮先核出来的）：这道检查是**纵深，不是边界**。
    // `backend_send_into` 同样是注册命令，它的 `payload: String` 也来自 webview，
    // 而后端侧只查「非空 / 长度 / 无控制字符」（`check_field`）—— 也就是说
    // **前端本来就能绕过本函数，直接送一条任意载荷去键入**。
    // ⇒ 本检查买到的是：① 走**文档化的那条路**时不会把注入串拼进载荷（挡的是**缺陷**，
    // 不是攻击者）；② 与同函数 `args` 那道白名单**姿态一致**（不对称本身会误导下一个人）。
    // 真正的边界在别处：backend 的 `admit`（会话身份）+ 前端执行面（CSP / 能力表）。
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
        return Err(refuse(copy_text(
            "rsPayload.launcher.injection",
            &[
                ("launcher", &format!("{:?}", spec.launcher)),
                ("c", &format!("{:?}", c)),
            ],
        )));
    }
    let mut argv = vec![spec.launcher];
    argv.extend_from_slice(spec.args);
    let inner = argv.join(" ");
    let cd = match spec.cwd {
        Some("") => return Err(refuse(&copy_text("rsPayload.cwd.empty", &[]))),
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
// `设计/90 §4 E`：**外层 tmux 命令那三格** —— 后端把它们产出来
// ═════════════════════════════════════════════════════════════════════════════
//
// # 它补的是本模块头注那张图的**左半边**
//
// ```text
// tmux new-session -d -s '=name:' … ; send-keys -t … '<载荷>' Enter ; tmux attach …
// └────────────────────── 外层：容器 ──────────────────────┘ └─ 内层：载荷 ─┘
//                        ↑ 本节（`设计/90 §4 E`）                ↑ 上面那半（U8c-1）
// ```
//
// 三格逐条：`container:tmux` 的 `create` / `send-into`，加上 `action:attach`。
// 它们今天在 TS 那侧的家是 `src/session-backend.ts`（座），由
// `launch-render-fallback.ts` 三个分支各调一次。
//
// # ⚠ 它**不**住新模块，这是本件的要点之一
//
// `设计/00 §2.5 ④` 的目标是「**5 个渲染实现 → 2 个（Rust CLI + Rust 载荷）**」——
// 整件事的要点是**消灭副本**。给外层单开一个模块会让盘上的渲染实现
// 从 5 变 6，方向是反的。⇒ 外层并进**「Rust 载荷」那一份**（就是本模块），
// 与内层共用同一份 quote 原语、同一套 fail-closed 姿态。
// 盘上还剩几份、各自住哪，由 `the_launch_renderers_on_disk_are_exactly_these`
// **逐份点名 + 恒等**钉着。
//
// # 与 TS 座的两处**刻意分歧**（同 [`render_payload`] 那几条，不是漏）
//
// 1. **校验**：TS 座头注逐字「座只在这些**已安全**的片段外拼后端语法，不做校验/转义」——
//    它收的是**调用方已经 quote 好**的 `quotedCwd` / `quotedPayload`。
//    本侧收的是**生料**，自己 quote、自己校验：会话名 / `@ccm_sid` 不合白名单一律 `Err`。
//    ⇒ **合法输入逐字节相同**（入库金标准钉着），非法输入这侧拒、那侧照拼。
// 2. **空串**：TS 的 `ccmSid ? … : ""` 把 `""` 当「没有」；本侧把 `Some("")` 判成坏数据
//    （空值 ≠ 未设，Z01 的支点）。金标准里因此没有这一形 —— **如实登记，不假装覆盖了**。

/// tmux 目标 —— **判别式，不嗅探**。与 TS `session-backend.ts::TmuxTarget` 一一对应。
///
/// `value` 恒是**明文名字**（两个变体都不预先带引号），变体声明它来自哪条校验路径：
/// `Raw` = 已证明只含 `[A-Za-z0-9_-]`（`cc-<sid8>` 那一族），渲染时裸拼；
/// `Quoted` = 校验时允许空格等自由字符（用户自定义会话名），渲染时 `posix_quote` 包裹。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TmuxTarget<'a> {
    Raw(&'a str),
    Quoted(&'a str),
}

impl<'a> TmuxTarget<'a> {
    fn value(&self) -> &'a str {
        match self {
            Self::Raw(v) | Self::Quoted(v) => v,
        }
    }

    /// `new-session -s <名>` 收的是**名字**不是 target ⇒ 不加 `=`/`:`。
    fn token(&self) -> String {
        match self {
            Self::Raw(v) => (*v).to_string(),
            Self::Quoted(v) => shell_quote_core::posix_quote(v),
        }
    }

    /// `-t` 一律经这里。
    ///
    /// **裸 `-t <名>` 不是精确匹配**：tmux 依次按「精确名 → 名字开头 → glob」解析。
    /// `=name:` 是唯一在 send-keys / attach / set-option 全部动词上都既通用又精确的形式
    /// （尾冒号把串强制成 `session:` 形态，`=` 才落在会话名段上被识别）。
    ///
    /// ⚠ 与 [`super::tmux::exact_target`] 是**同一件事的两处写法，不是副本**：
    /// 那一处服务的是**本机直接 spawn `tmux` 进程**（argv 元素，不过 shell ⇒ 不 quote），
    /// 这一处服务的是**要拼进一条 shell 串**的远端命令（⇒ `Quoted` 那支要 quote）。
    /// `=`/`:` 的形状相同、quote 姿态相反，合并会让其中一侧错。
    fn exact(&self) -> String {
        let marked = format!("={}:", self.value());
        match self {
            Self::Raw(_) => marked,
            Self::Quoted(_) => shell_quote_core::posix_quote(&marked),
        }
    }

    fn check(&self) -> Result<(), String> {
        let v = self.value();
        if v.is_empty() {
            return Err(refuse(&copy_text("rsPayload.tmuxName.empty", &[])));
        }
        match self {
            // 裸拼进命令 ⇒ 白名单必须是 tmux 名字那一族，一个字符都不许多。
            Self::Raw(_) => {
                if !v
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
                {
                    return Err(refuse(copy_text(
                        "rsPayload.tmuxName.rawBad",
                        &[("name", &format!("{:?}", v))],
                    )));
                }
                Ok(())
            }
            // quote 之后 shell 元字符都是字面量了，真正要挡的是控制符与视觉欺骗字符：
            // 前者会把一条命令劈成两条，后者让人眼看不出接的是哪个会话。
            Self::Quoted(_) => {
                if let Some(c) = v
                    .chars()
                    .find(|c| c.is_control() || acct_core::is_deceptive_char(*c))
                {
                    return Err(refuse(copy_text(
                        "rsPayload.tmuxName.control",
                        &[("name", &format!("{:?}", v)), ("c", &format!("{:?}", c))],
                    )));
                }
                Ok(())
            }
        }
    }
}

/// 外层容器那一层的**三格**，与 TS `launch-render-fallback.ts` 的三个分支一一对应。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TmuxOuter<'a> {
    /// `container: tmux` / `mode: create` ⇒ TS `SESSION_BACKEND.createRunAttach`。
    Create {
        target: TmuxTarget<'a>,
        /// `None` = 不带 `-c`。⚠ 这一格的 cwd **不进载荷**（内层没有 `cd`），
        /// 它是 `new-session -c <目录>` 的实参 —— 与 `container:"none"` 那一格分工不同。
        cwd: Option<&'a str>,
        /// `None` = 不打身份标记，`set_sid` 与 `set_title` 两段一起不出现（TS 侧同一个三元）。
        ccm_sid: Option<&'a str>,
    },
    /// `container: tmux` / `mode: send-into` ⇒ TS `SESSION_BACKEND.runInExistingAttach`。
    SendInto { target: TmuxTarget<'a> },
    /// `action: attach` ⇒ TS `SESSION_BACKEND.attach`。**不带载荷。**
    Attach { target: TmuxTarget<'a> },
}

/// `@ccm_sid` 是裸拼进命令的，白名单与 TS 座声称调用方会保证的那一条同口径
/// （座头注逐字「调用方须保证 `ccmSid` 为 `[A-Za-z0-9_-]`（座不做校验、裸拼）」）。
///
/// **本侧不信那句声称** —— 它是一句注释纪律，而这条路的上游是 webview。
fn ccm_sid_safe(sid: &str) -> bool {
    !sid.is_empty()
        && sid
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
}

/// 外层三格的编译。`payload` = **内层已渲染好的生料**（没 quote），本函数负责 quote。
///
/// `Create` / `SendInto` **必须**带载荷；`Attach` **必须不**带 —— 两个方向都 fail-closed，
/// 免得「少送一格」静默变成「建个空会话再把用户接进去」（issue #76 那一形）。
///
/// # 幂等闸为什么不在这里（`C14`〔用 08-12〕：起会话就是起会话，不要 `or`）
///
/// 这一支原来是「幂等 create-or-attach」，那个 `or` 藏在三个符号里：
/// `2>/dev/null` 吞掉「会话已存在」的报错、`&&` 于是短路**不送载荷**、
/// 而 `; tmux attach` 无条件把你接进去 ⇒「静默接回，且不告诉你载荷没送」。
/// ⇒ 今天**不吞错**：`new-session` 失败就整条非零退出；撞名由上游唯一铸造口避让。
/// 尾部 `attach` 保留，但挂在 `&&` 后面（`§1.3`：最终 exec 落在用户自己的终端里）。
pub fn render_tmux_outer(outer: &TmuxOuter, payload: Option<&str>) -> Result<String, String> {
    match outer {
        TmuxOuter::Create {
            target,
            cwd,
            ccm_sid,
        } => {
            target.check()?;
            // ⚠ **不许写 `ok_or_else`**：`every_business_rejection_is_tagged` 把它连同
            //   `ok_or` / `.map_err` 一起列成**禁令**（它们能产出一个没经 [`refuse`] 的错误串，
            //   而 TS 侧按 `REFUSE:` 标分流 ⇒ 不打标的拒绝会被当成 IPC 异常、回落到兜底渲染器
            //   ⇒ 一次 fail-closed 当场变 fail-open）。本件第一次跑就撞上了它。
            let p = match payload {
                Some(p) => p,
                None => return Err(refuse(&copy_text("rsPayload.outer.createNoPayload", &[]))),
            };
            let cflag = match cwd {
                Some("") => return Err(refuse(&copy_text("rsPayload.cwd.empty", &[]))),
                Some(c) => format!(" -c {}", shell_quote_core::posix_quote(c)),
                None => String::new(),
            };
            let t = target.exact();
            // 身份标记与外层标题是**次要**动作，绝不能阻断主要动作（后面那个 `&& send-keys`）
            // ⇒ 各自 `( … 2>/dev/null || true ) && ` 包起来：古董 tmux 上 set 失败就降级到
            // 「无标记」（= #72 之前的行为），resume 照跑，而不是把用户丢进空 shell。
            let (set_sid, set_title) = match ccm_sid {
                None => (String::new(), String::new()),
                Some(s) => {
                    if !ccm_sid_safe(s) {
                        return Err(refuse(copy_text(
                            "rsPayload.sessionMark.bad",
                            &[("value", &format!("{:?}", s))],
                        )));
                    }
                    (
                        format!("(tmux set-option -t {t} @ccm_sid {s} 2>/dev/null || true) && "),
                        // 外层终端窗口标题 = `ccm-rbind-<sid>`，由 tmux 格式 `#{@ccm_sid}`
                        // **从上一句刚设的 option 派生** —— claude 碰不到 option（OSC 只改
                        // pane_title），所以这个标题永远稳定、不需要轮询重刷。
                        format!(
                            "(tmux set-option -t {t} set-titles on 2>/dev/null || true) && \
                             (tmux set-option -t {t} set-titles-string ccm-rbind-#{{@ccm_sid}} 2>/dev/null || true) && "
                        ),
                    )
                }
            };
            Ok(format!(
                "tmux new-session -d -s {}{} && {}{}tmux send-keys -t {} {} Enter && tmux attach -t {}",
                target.token(),
                cflag,
                set_sid,
                set_title,
                t,
                shell_quote_core::posix_quote(p),
                t,
            ))
        }
        // 会话确实在（claude 已退、只剩交互 shell）⇒ **无条件** send-keys + attach，
        // 没有 new-session、没有短路。复用原名 = 不产 `cc-<sid8>-N` 孤儿（治 #76 根因）。
        TmuxOuter::SendInto { target } => {
            target.check()?;
            let p = match payload {
                Some(p) => p,
                None => return Err(refuse(&copy_text("rsPayload.outer.sendIntoNoPayload", &[]))),
            };
            let t = target.exact();
            Ok(format!(
                "tmux send-keys -t {} {} Enter; tmux attach -t {}",
                t,
                shell_quote_core::posix_quote(p),
                t,
            ))
        }
        TmuxOuter::Attach { target } => {
            target.check()?;
            if payload.is_some() {
                return Err(refuse(&copy_text("rsPayload.outer.attachWithPayload", &[])));
            }
            Ok(format!("tmux attach -t {}", target.exact()))
        }
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// `K-H2b`：**接上注入点** —— 起会话那一刻把 `ANTHROPIC_BASE_URL` 指向那台机器的中转（渲染那一半）
// ═════════════════════════════════════════════════════════════════════════════
//
// # 这一段为什么在这里
//
// `K-H2b` 的 `Bx` 换了一把尺子：**注入点不是「入口」的属性，是「渲染 env 前缀那一层」
// 的属性**（`K20`：判据/说法按形状认，不按主题名）。本模块正是那一层在 Rust 侧的家
// —— `config_dir_prefix_posix` 与 `render_env_ops` 都住这儿。
//
// # 〔US1 · 第四波 4D〕「注入什么」那一半**搬走了**，这里只剩「怎么渲」
//
// 先前这一段还住着 `设计/20 §3.2` 那张决策表（`relay_endpoint_for` / `apikey_endpoint_for` / `RelayAsk`〔散文墓碑〕）、
// 凭据文件那一家（`APIKEY_TABLE_AGENT`〔散文墓碑〕）、登记了默认上游的 agent（`AGENTS_WITH_DEFAULT_UPSTREAM`〔散文墓碑〕）、
// 路由语法（两个前缀 · 段闸 · 拼串 · 两份跨半边样例）—— 每一样都是后端那一份的第二份（B-decouple §2.1 必须拆 1）。
// 今天：决策表住后端上游选择 `accounts/upstream/endpoint.rs`（帧命令 `launch-endpoint` 出成品，起会话那一侧只转交、执行：
// `history::relay_endpoint_on`）；端口 · 钥匙路径 · 路由语法住共享 crate `relay_route_core`（`设计/20 §5` 目标），两侧 `use` 同一份。
// 渲染 shell 串属于开终端那一侧（`20 §3.3`），留在这里。
//
// # ⚠ 两条**没买到**的（铁律 14）
//
// 1. **claude 拿到这个变量之后到底怎么走，本仓零证据**（红线 `C7`：绝不起真 claude）。
// 2. **Windows 那一侧只到「编得过」**（`relay_env_prefix_ps` 一行运行时行为都没量过）。

/// 本机中转的端口。**值只住共享 crate**（`relay_route_core::PORT`）：起本机后端时以 `CCM_RELAY_PORT`
/// 交给它（`local_backend_host::relay_host_envs`），远端 `relay-status` / `relay-ensure` 的 `port` 入参也是它；
/// 后端 `src/backend/relay/server.rs::DEFAULT_PORT` 是同一个 const。〔US1〕先前两处各写一个 8788、零对拍。
pub const RELAY_PORT: u16 = relay_route_core::PORT;

/// 〔RL1〕载荷里那条中转地址（`EnvOp::ExportRelayBaseUrl`）的 fail-closed 校验：必须是构造口产得出的形状
/// （`http://127.0.0.1:<1–65535>` ＋ `/s/` 或 `/t/` ＋ 恰好三段、每段过闸）。
/// 〔US1〕构造口搬去后端上游选择之后，这个「逆」与构造口同住共享 crate（`relay_route_core::base_url_shape_ok`），本侧不另写一份。
pub use relay_route_core::base_url_shape_ok as relay_base_url_shape_ok;

/// `<key>` 段（第 3 段，流标签）与起会话身份 token 的**唯一铸造口**〔`KH2B6`〕。
///
/// | 这一格 | 填什么 |
/// |---|---|
/// | resume 一条已有会话 | **那条会话的 sid**（天然是 UUID 形态 ⇒ 过得了段闸） |
/// | 新开一个会话 | **一个启动时生成的 nonce** —— 不等 claude 产 sid |
///
/// ⚠ 它欠的账（`§0e` 裁二逐字采纳）：nonce 与 claude 事后产生的 sid **没有对应关系**；
/// 这一段**不参与选上游、不参与选凭据**（上游选择的键是前两段），只喂 tee。今天不是缺陷、是债。
/// 〔US1〕起会话那一侧把它作为 `launch-endpoint` 的 `key` 交给那台后端（后端再过一次段闸，不另铸）。
fn mint_route_key() -> String {
    // UUID v4 的连字符形态逐字过得了段闸（`[0-9a-f-]`，36 字节）。
    uuid::Uuid::new_v4().to_string()
}

/// 见 [`mint_route_key`]。**这是 `<key>` 段与身份 token 唯一的取值口** —— resume 用 sid，新开用 nonce。
///
/// sid 过不了段闸（`relay_route_core::segment_is_safe`，与中转切键同一份）时**也回落到 nonce**（而不是 `Err`）：
/// 这一段对路由惰性，为它把一次起会话整个拒掉不划算。
pub fn route_key_for_session(sid: Option<&str>) -> String {
    match sid {
        Some(s) if relay_route_core::segment_is_safe(s) => s.to_string(),
        _ => mint_route_key(),
    }
}

/// **POSIX 命令面**的中转前缀 —— **本文件生产段里唯一一处**产出
/// `export ANTHROPIC_BASE_URL=` 的地方〔`KH2B4`，由
/// `only_one_place_in_this_file_exports_the_relay_base_url` 数着〕。
///
/// ⚠ **那条判据的人群只有本文件**（它 `include_str!("payload.rs")`）—— 别把它读成「全仓唯一一处」。
/// 〔US1〕后端 `control/ccm/plan.rs::base_url_word` 是另一处（`ccm` 把继承来的地址转进新 pane），同形、各自一个 crate。
pub fn relay_env_prefix_posix(base_url: &str) -> String {
    // 〔RK1〕钥匙那一段是**读钥匙文件的命令替换**（见 [`RELAY_KEY_FILE_REL`]）：两段常量各自单引号，
    //   中间只有那一个固定的 `$(cat …)` 会被 shell 展开 ⇒ URL 里别的字节一个都不会被解释。
    let (origin, path) = relay_url_halves(base_url);
    format!(
        "export ANTHROPIC_BASE_URL={}\"$(cat \"$HOME/{RELAY_KEY_FILE_REL}\")\"{}; ",
        shell_quote_core::posix_quote(origin),
        shell_quote_core::posix_quote(path),
    )
}

/// 〔RK1 · `INVARIANTS §48.1`〕**中转钥匙文件**相对家目录的路径 —— 中转口进门要出示的那一把就住这里
/// （**中转所在那台机器**上，`0600`，中转绑上口之后自己读回或铸：`src/backend/relay/door.rs::ensure_key`）。
///
/// # 为什么 URL 里不放钥匙本身，而放「去读这个文件」
///
/// 注入的 URL 今天**不是**直接进 agent 的 env：它渲染成 shell 文本，经 `tmux send-keys` 的 **argv** 打进 pane 的交互 shell
/// （[`render_tmux_outer`]），会进 shell 历史、界面的终端回滚；远端那一形还绕 webview 一圈（`relay_endpoint_for_launch`）。
/// 钥匙字面拼进去，就会出现在同机别的用户 `ps` 看得见的 argv 里。⇒ URL 本身**不带钥匙**，
/// 渲染器把钥匙段写成 `$(cat "$HOME/<本常量>")`，在**那台机器的 pane shell 里**展开 ——
/// 钥匙只从 `0600` 文件进 agent 进程自己的 env。`$HOME` 在哪台上展开就读哪台的钥匙（与「回环地址是自指的」同一个道理）。
///
/// 〔US1〕值只住共享 crate（`relay_route_core::KEY_FILE_REL`），后端 `door.rs::KEY_FILE_REL` 是同一个 const。
pub const RELAY_KEY_FILE_REL: &str = relay_route_core::KEY_FILE_REL;

/// 中转 URL 拆成「`http://主机:口/`」与「`/s/…` 那一截」两半，钥匙段插在中间。
/// 拆不开（不是构造口的产物形状）⇒ 整条当前半、后半空 —— 渲染出来的请求会被中转以 403 拒（出声），
/// **不会**退回直连。调用方今天都先过了形状闸（[`relay_base_url_shape_ok`]），这一支走不到。
fn relay_url_halves(base_url: &str) -> (&str, &str) {
    let Some(rest) = base_url.strip_prefix("http://") else {
        return (base_url, "");
    };
    match rest.find('/') {
        // 那个 `/` 两半各带一份：前半以它收尾（钥匙段接在它后面），后半以它开头（钥匙段之后的分隔）。
        Some(i) => {
            let at = "http://".len() + i;
            (&base_url[..=at], &base_url[at..])
        }
        None => (base_url, ""),
    }
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
    // 〔RK1〕与 POSIX 那一形同构：钥匙段现读 `$HOME` 底下那一份（PowerShell 的 `$HOME` 即 `USERPROFILE`，
    //   与后端 `door::key_path` 的退路同一个）。仍只到「编得过」。
    let (origin, path) = relay_url_halves(base_url);
    format!(
        "$env:ANTHROPIC_BASE_URL='{origin}' + (Get-Content -Raw -LiteralPath (Join-Path $HOME '{RELAY_KEY_FILE_REL}')).Trim() + '{path}'; "
    )
}

#[cfg(test)]
#[path = "../../../../../tests/bridge/backend/control/payload_tests.rs"]
mod tests;
