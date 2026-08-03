//! 「起一个会话」的**载荷编译器** —— `env 前缀 → cd → argv → wrap` 那一段串的唯一 Rust 真相源。
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
//! 本 crate 只管**内层**。⚠ **不是因为外层「已经没了」** —— U8c-1 第一版这么写，被审计证伪：
//! daemon 的 `launch`（U8a-2b）**零生产调用方**、它**结构上也不 attach**（平面 ③），
//! 而 `account_usage.rs::build_usage_probe_cmd` 今天就在 Rust 里拼一整条外层 tmux 串。
//! 外层的四个产出方一个都没退役，处置见 `doc/INVARIANTS.md` §33b。
//!
//! # 为什么是共享 crate
//!
//! 同一段载荷今天有**五份、跨三种语言**（账本 S28；U8c-2a 退役了一份）：
//! TS `launch-render-fallback.ts` · ~~TS `remote-launch.ts::buildUsageProbePayload`~~（**已退役**）·
//! Rust `history.rs`（本机 POSIX）·
//! Rust `history.rs`（Windows，`$env:` 变体，是平台特化不是副本）·
//! **`shared/ccm` 的 `--print` 段与 exec 段**（S10 已裁定刻意不合并）。
//! 消费方跨两种宿主（daemon 远端执行面 / monitor 本机拉起），
//! ⇒ 与 `branch-core`/`usage-core`/`acct-core`/`guard-core` 同一个理由、同一个形态。
//!
//! # 诚实边界（U8c-1 交付时的实况，别读成「合完了」）
//!
//! - 本 crate 今天有**两个**生产消费方：`history.rs` 的 POSIX 分支（只用
//!   [`config_dir_prefix_posix`]）与 `account_usage.rs` 的用量探针（U8c-2a 起走
//!   [`usage_probe_payload`] → [`render_payload`]，**后者的第一个生产调用方**）。
//!   远端起会话主路仍在 TS 手里，要等 **U8c-2b**。
//! - **Windows 分支不在这里** —— `$env:CLAUDE_CONFIG_DIR=$null; ` 与它自己那套
//!   「什么算绝对路径」（盘符 / UNC / `\` 分隔）是刻意的平台特化。
//!   `acct-core` 头注已经为同族的 `is_safe_config_dir` 裁决过「不合」，本 crate 不推翻它。
//! - TS 侧还剩 `launch-render-fallback.ts` 一个产出点（U8c-2b 收编、U8c-3 删除）。跨语言一致性今天由
//!   `crates/launch-core/fixtures/payload-golden.json` 的**逐字节对拍**保证：
//!   TS 侧生成并入库、Rust 侧读同一份文件自己渲染再比。

pub mod cli;

use std::fmt::Write as _;

/// POSIX 单引号 quote：整体 `'…'` 包裹，内部 `'` 断开为 `'\''`。
///
/// 与 TS `shell-quote.ts::posixQuote` 逐字节同义（对拍夹具里有带引号的样本）。
pub fn posix_quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('\'');
    for c in s.chars() {
        if c == '\'' {
            out.push_str("'\\''");
        } else {
            out.push(c);
        }
    }
    out.push('\'');
    out
}

/// 两种 shell 共用的元字符黑名单。
///
/// **`\` 不在里面** —— 见 [`config_dir_command_safe`]：POSIX 侧由调用点额外拒掉它
/// （那边的路径里不该有反斜杠），而 Windows 侧的账号目录长成 `C:\Users\z\.claude-accts\z`，
/// 把 `\` 一律禁掉等于禁掉整个平台。它在两种 shell 的**单引号**里都是字面量
/// （POSIX `'…'` 无转义；PowerShell `'…'` 无插值），真正要挡的是能提前闭合引号或另起命令的那几个。
const SHELL_META_COMMON: &str = "'\"`$;|&<>*?()!";

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
                return Err("具名账号的 configDir 是空的（账号 0 请用 base）".into());
            }
            if !config_dir_command_safe(d) {
                return Err(format!("拒绝拼入命令：非法 CLAUDE_CONFIG_DIR {d:?}"));
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
    /// 已 sanitize 过的 launcher（sanitize **必须先于** wrap —— 那是函数组合上的结构保证，
    /// 本 crate 收的是**结果**，不在这里再 sanitize 一次）。
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
                    return Err("configDir 是空串（账号 0 请用 UnsetConfigDir）".into());
                }
                if !config_dir_command_safe(value) {
                    return Err(format!("拒绝拼入命令：非法 CLAUDE_CONFIG_DIR {value:?}"));
                }
                let _ = write!(out, "export CLAUDE_CONFIG_DIR={}; ", posix_quote(value));
            }
            EnvOp::ExportModel { value } => {
                let _ = write!(out, "export ANTHROPIC_MODEL={}; ", posix_quote(value));
            }
            EnvOp::UnsetConfigDir => out.push_str(UNSET_CONFIG_DIR_PREFIX),
            EnvOp::UnsetNestedEnv { keys } => {
                if keys.is_empty() {
                    return Err("嵌套 env 键表是空的 ⇒ 会渲染出裸 `unset ; `".into());
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
            return Err(format!(
                "拒绝拼入命令：参数 {a:?} 不在放行集里 —— 载荷是 `join(\" \")` 拼的，\
                 空白会让它裂成多个参数、shell 元字符会另起一条命令。\n\
                 放行集是 `[A-Za-z0-9] + -_.:/=,@+`；**非 ASCII 也一律拒**（已知过严，\
                 且与 `config_dir_command_safe` 放行中文不对称，见 `arg_is_join_safe` 头注）"
            ));
        }
    }
    let mut argv = vec![spec.launcher];
    argv.extend_from_slice(spec.args);
    let inner = argv.join(" ");
    let cd = match spec.cwd {
        Some("") => return Err("cwd 是空串 —— 空值 ≠ 未设；不加 cd 请用 None".into()),
        Some(c) => format!("cd {} && ", posix_quote(c)),
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
            return Err("用量探针需要显式 configDir（账号 0 请传 None，空串是坏数据）".into())
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
    use super::*;

    #[test]
    fn posix_quote_breaks_single_quotes_the_posix_way() {
        assert_eq!(posix_quote("/p"), "'/p'");
        assert_eq!(posix_quote("a'b"), "'a'\\''b'");
        assert_eq!(posix_quote(""), "''");
    }

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
