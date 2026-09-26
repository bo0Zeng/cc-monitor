//! B04：cc-bus 钩子在 `~/.claude/settings.json` 里的**只读诊断** + 生成待贴文本。
//!
//! **绝不写入**（用户 2026-07-28 定调）。理由不是保守：`cc-bus-install.sh` 第 3 行同样写着
//! "只做可逆的本地安装：不改全局 settings.json、不 systemctl"——**两边一致，是这个生态的
//! 既定约定**，不是我的加码。这个文件里因此连一个写文件的函数都没有。
//!
//! **判据为什么不是字符串等值比较**（见 features/B04-hook-states-from-real-disk.md）：
//! 实测用户盘上装的是 `"$HOME/.local/bin/cc-register" >/dev/null 2>&1 || true`，
//! 而 `cc-bus-install.sh` 的规范片段是 `cc-register >/dev/null 2>&1 || true`。
//! 两者**功能完全等价**，字符串却不等。若按等值比较，会把一套**完全正确的安装**报成
//! 「装了但指向别的路径」，然后建议用户去修一个没坏的东西。判据必须对着真实盘面校准，
//! 不能对着自己写的样板校准——这与 B03 那几个发现同源。

use crate::copy_table::copy_text;

/// 一个钩子的诊断结论。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../../src/generated/"))]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum HookState {
    /// 没有任何一条 command 调到这个程序。
    NotInstalled,
    /// 裸命令形态（走 PATH）。
    InstalledViaPath { command: String },
    /// 显式路径且**该路径存在**。这是用户当前的实际状态，**不是问题**。
    InstalledAtPath { command: String, path: String },
    /// 显式路径但**该路径不存在** —— 真正的第三态：看着像装了，其实指不到东西。
    PathMissing { command: String, path: String },
    /// **无法判断**（B04 审计 B04-4）：命令里出现了目标程序名，但它不是被直接执行的那个
    /// （包在 `sh -c` / `bash -lc` / `env` / `timeout` / `exec` 里，或命令形态复杂）。
    ///
    /// 为什么必须有这一态：源码原本写着「对看不懂的输入**返回 None 而不是猜**——猜错会把
    /// '未装'说成'已装'，比说不知道更坏」，但 `None` 在 `diagnose_event` 里落到了
    /// `NotInstalled`，UI 渲染成确定性的**「未装」**。于是"说不知道"这个设计意图
    /// **根本没有对应的状态**——装了 `sh -c` 包装钩子的用户会被告知"未装"，
    /// 然后去贴一份重复的钩子。**猜"未装"和猜"已装"一样是猜。**
    Unknown { command: String },
}

impl HookState {
    /// 只有这两种算"能用"。`PathMissing` 刻意不算——它最容易被误报成已装。
    pub fn is_working(&self) -> bool {
        matches!(
            self,
            HookState::InstalledViaPath { .. } | HookState::InstalledAtPath { .. }
        )
    }

    /// 是否"无法判断"——UI 据此渲染成中性色，而不是当成问题。
    pub fn is_unknown(&self) -> bool {
        matches!(self, HookState::Unknown { .. })
    }
}

/// 整份诊断。`note` 装"为什么没读到"这类说明（文件缺失/坏 JSON），**不为空即应展示**。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../../src/generated/"))]
pub struct HooksDiagnosis {
    pub session_start: HookState,
    pub stop: HookState,
    pub note: String,
}

/// 从一条 shell 命令串里取出**被执行的程序名**。
///
/// 只做够用的事，不写一个 shell 解析器：剥前导 `VAR=x` 环境赋值 → 取第一个词 →
/// 去掉包裹的引号 → 取 basename。这足以覆盖实测见到的全部形态
/// （裸命令、`"$HOME/.local/bin/x"`、`env A=1 x`），且对看不懂的输入**返回 None
/// 而不是猜**——猜错会把"未装"说成"已装"，那比说不知道更坏。
/// 剥掉**配对的**一层包裹引号。不配对就原样返回。
///
/// B04 登记项：原先两处都用 `trim_matches(|c| c == '"' || c == '\'')`，它是**逐字符两端剥**
/// ——`"a'` 会被剥成 `a`（两端引号种类都不同）、`''x''` 会被剥干净。
/// 不配对的引号意味着这条命令形状可疑，替用户猜一个"本意"比原样交给下游判断更坏。
fn unquote_once(tok: &str) -> &str {
    let b = tok.as_bytes();
    if b.len() >= 2 && (b[0] == b'"' || b[0] == b'\'') && b[b.len() - 1] == b[0] {
        &tok[1..tok.len() - 1]
    } else {
        tok
    }
}

pub fn program_of(cmd: &str) -> Option<(String, String)> {
    let mut rest = cmd.trim();
    // 剥前导环境赋值：`FOO=bar baz` 里的 `FOO=bar`
    loop {
        let Some(tok) = rest.split_whitespace().next() else {
            return None;
        };
        // `A=B` 形态且等号不在首位 → 是环境赋值，跳过它
        let is_assign = matches!(tok.find('='), Some(i) if i > 0)
            && tok[..tok.find('=').unwrap()]
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_');
        if !is_assign {
            break;
        }
        rest = rest[tok.len()..].trim_start();
    }
    let tok = rest.split_whitespace().next()?;
    // 去掉包裹引号（实测用户那条就是 `"$HOME/.local/bin/cc-register"`）。
    // **只剥配对的一层**（B04 登记项，T03 收）：原先 `trim_matches(|c| c == '"' || c == '\'')`
    // 会把 `"a'` 这种**不配对**的也剥成 `a`、把 `''x''` 剥干净。不配对的引号说明这条命令
    // 本身形状可疑，剥掉它等于替用户猜一个"本意"。
    let unq = unquote_once(tok);
    if unq.is_empty() {
        return None;
    }
    let base = unq.rsplit('/').next().unwrap_or(unq);
    if base.is_empty() {
        return None;
    }
    Some((base.to_string(), unq.to_string()))
}

/// 判一条命令是不是在调 `want`，并据此定态。`exists` 用于问"这个路径在不在"
/// （注入进来而不是直接查文件系统，纯函数才好测）。
/// 命令里是否**出现过**目标程序（不一定是被直接执行的那个）。
/// 用于把包装器写法判成 `Unknown` 而不是 `NotInstalled`。
fn mentions_program(cmd: &str, want: &str) -> bool {
    cmd.split(|c: char| c.is_whitespace() || matches!(c, '"' | '\'' | ';' | '&' | '|' | '(' | ')'))
        .any(|tok| {
            let t = unquote_once(tok);
            !t.is_empty() && t.rsplit('/').next().unwrap_or(t) == want
        })
}

pub fn classify_command(cmd: &str, want: &str, exists: &dyn Fn(&str) -> bool) -> Option<HookState> {
    let Some((base, full)) = program_of(cmd) else {
        // 连第一个词都取不到，但命令里提到了目标 → 说不知道，别说"未装"
        return mentions_program(cmd, want).then(|| HookState::Unknown {
            command: cmd.trim().to_string(),
        });
    };
    if base != want {
        // 被直接执行的不是它，但命令里提到了它 → `sh -c "cc-register"` / `env X cc-register`
        // / `timeout 5 cc-register` 这类包装写法。**判"无法判断"，不判"未装"。**
        return mentions_program(cmd, want).then(|| HookState::Unknown {
            command: cmd.trim().to_string(),
        });
    }
    // 裸命令（没有路径分隔符）→ 走 PATH
    if !full.contains('/') {
        return Some(HookState::InstalledViaPath {
            command: cmd.trim().to_string(),
        });
    }
    if exists(&full) {
        Some(HookState::InstalledAtPath {
            command: cmd.trim().to_string(),
            path: full,
        })
    } else {
        Some(HookState::PathMissing {
            command: cmd.trim().to_string(),
            path: full,
        })
    }
}

/// 在 settings JSON 里找某个事件下调 `want` 的钩子。
///
/// **逐层容忍**：`hooks` 缺失/不是对象、事件值不是数组、条目缺 `hooks`、`command` 缺失或
/// 空白——统统跳过而不是抛。一个坏条目不该吃掉整份诊断（同 B03 解析器的契约）。
/// 同一事件挂多条钩子时，**只要有一条命中就算装了**（不要求独占：用户完全可能同时挂
/// 别的工具的钩子）。命中多条时优先报"能用"的那条。
pub fn diagnose_event(
    root: &serde_json::Value,
    event: &str,
    want: &str,
    exists: &dyn Fn(&str) -> bool,
) -> HookState {
    let mut fallback: Option<HookState> = None;
    let entries = root
        .get("hooks")
        .and_then(|h| h.get(event))
        .and_then(|e| e.as_array());
    for entry in entries.into_iter().flatten() {
        let inner = entry.get("hooks").and_then(|h| h.as_array());
        for hk in inner.into_iter().flatten() {
            let Some(cmd) = hk.get("command").and_then(|c| c.as_str()) else {
                continue;
            };
            if cmd.trim().is_empty() {
                continue;
            }
            if let Some(st) = classify_command(cmd, want, exists) {
                if st.is_working() {
                    return st; // 能用的优先，立刻返回
                }
                // 记下 PathMissing / Unknown，继续找有没有能用的。
                // 两者都比"未装"更接近真相，所以都进 fallback。
                fallback.get_or_insert(st);
            }
        }
    }
    fallback.unwrap_or(HookState::NotInstalled)
}

/// 完整诊断。`raw` 是 settings.json 的原文；解析失败 → 两态皆 NotInstalled + note 说明原因。
pub fn diagnose(raw: Option<&str>, exists: &dyn Fn(&str) -> bool) -> HooksDiagnosis {
    let Some(raw) = raw else {
        return HooksDiagnosis {
            session_start: HookState::NotInstalled,
            stop: HookState::NotInstalled,
            note: copy_text("rsHooksDiag.diagnose.noSettings", &[]),
        };
    };
    // BOM 容忍（同 mcp.rs 读 ~/.claude.json 的处理）
    let Ok(v) = serde_json::from_str::<serde_json::Value>(raw.trim_start_matches('\u{feff}'))
    else {
        return HooksDiagnosis {
            session_start: HookState::NotInstalled,
            stop: HookState::NotInstalled,
            note: copy_text("rsHooksDiag.diagnose.badJson", &[]),
        };
    };
    if !v.is_object() {
        return HooksDiagnosis {
            session_start: HookState::NotInstalled,
            stop: HookState::NotInstalled,
            note: copy_text("rsHooksDiag.diagnose.notObject", &[]),
        };
    }
    HooksDiagnosis {
        session_start: diagnose_event(&v, "SessionStart", "cc-register", exists),
        stop: diagnose_event(&v, "Stop", "cc-bus-stop-hook", exists),
        note: String::new(),
    }
}

/// 生成一段待贴片段时**盘上的实况**。两个字段都来自已有的探测，不新增探测机制：
/// 本机走 `exists` 闭包（含按 `$PATH` 逐目录反查），远端走 `REMOTE_HOOKS_CMD` 里
/// 已经在吐的 `-x` 判定与 `command -v` 输出。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SnippetProbe {
    /// `$HOME/.local/bin/{cc-register,cc-bus-stop-hook}` 是否都在盘上。
    /// **`None` = 取不到，不猜。**
    ///
    /// 上一版这里是 `bool`，与下面的 `on_path: Option<bool>` **不对称**——T03 审计
    /// 阻塞 3 正是从这个不对称进来的：同一份含混证据被用出两个结论，
    /// 一个说"分不清所以不猜"，一个确定地说"在"。两个字段现在同一档口径。
    pub home_path_exists: Option<bool>,
    /// 裸命令是否解析得到。**`None` = 取不到，不猜**（同本仓其它"说不知道"的地方）。
    pub on_path: Option<bool>,
}

/// 一段待贴片段 + 可能的警示。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../../src/generated/"))]
pub struct Snippet {
    pub text: String,
    /// 选的形态与盘上实况冲突时的警示。`None` = 没冲突。**UI 必须把它显示出来。**
    pub warning: Option<String>,
}

/// 生成待贴的 JSON 片段。**两种形态**让用户挑：
///   · `home` = true：`$HOME/.local/bin/...` 显式路径，不依赖 PATH；
///   · `home` = false：裸命令，简洁但依赖 PATH。
/// 生成的是**待贴文本**，本模块绝不代写文件。
///
/// ## 为什么要吃 `probe`（B04 登记项，T03 收）
///
/// 上一版签名是 `snippet(home: bool) -> String`——**只按一个布尔选形态，完全不看盘上实况**。
/// 于是面板可以推荐 `$HOME/.local/bin/cc-register` 而那个文件根本不存在，
/// **贴上去就是一个 `path-missing` 的钩子**，而这一步没有任何测试能发现。
/// （B04 审计当时只删了面板上那句"与本机现状一致"的假承诺，根因没动。）
/// 现在形态与实况冲突就带 `warning`，并有测试钉住；UI 侧另有测试钉住它真的上屏。
pub fn snippet(home: bool, probe: &SnippetProbe) -> Snippet {
    let (reg, stop) = if home {
        (
            "\"$HOME/.local/bin/cc-register\" >/dev/null 2>&1 || true",
            "\"$HOME/.local/bin/cc-bus-stop-hook\"",
        )
    } else {
        ("cc-register >/dev/null 2>&1 || true", "cc-bus-stop-hook")
    };
    let text = format!(
        "{{\n  \"hooks\": {{\n    \"SessionStart\": [ {{ \"hooks\": [ {{ \"type\": \"command\",\n      \"command\": \"{}\" }} ] }} ],\n    \"Stop\": [ {{ \"hooks\": [ {{ \"type\": \"command\",\n      \"command\": \"{}\" }} ] }} ]\n  }}\n}}",
        reg.replace('"', "\\\""),
        stop.replace('"', "\\\"")
    );
    // **形态与实况冲突就说出来**，而不是安静地生成一段贴上去指不到东西的钩子。
    // **只在确定"不在"时才警示。** `None`（取不到）不警示——报一个我们并不知道的问题，
    // 和漏报一样是失信。
    let warning = if home && probe.home_path_exists == Some(false) {
        Some(copy_text("rsHooksDiag.snippet.homeMissing", &[]))
    } else if !home && probe.on_path == Some(false) {
        Some(copy_text("rsHooksDiag.snippet.bareMissing", &[]))
    } else {
        None
    };
    Snippet { text, warning }
}

/// 按 `$PATH` 逐目录反查一个裸命令在不在。**复用注入的 `exists`，不新增探测机制。**
/// `path_env` 为 `None`（取不到环境变量）时返回 `None`——**不猜**。
///
/// ## 切分必须用 `std::env::split_paths`，不能写死 `':'`（T03 审计阻塞 1）
///
/// 第一版是 `pe.split(':')`。**本应用的生产平台是 Windows**
/// （`ci.yml` 与 `release.yml` 的打包 job 都是 `windows-latest`），而 Windows 的 PATH
/// 用 `';'` 分隔且盘符自带冒号：`C:\Windows;C:\Users\me\.local\bin` 按 `':'` 切成
/// `["C", "\Windows;C", "\Users\me\.local\bin"]`，逐个拼 `/cc-register` 全不存在
/// → 返回 `Some(false)` 而**不是** `None`。
///
/// 后果比"算错"更坏：本模块文档头花六行论证「不能对能用的安装报假警报」，
/// `SnippetProbe::on_path` 的注释写着「取不到就不猜」——而它在生产平台上
/// **既没取到、又给了一个确定的否定答案**，于是裸命令形态**恒**带一句
/// 「这两个命令不在 PATH 上」，把用户从一个能用的形态劝走。
/// 旧测试还把错的行为钉绿了：它硬编码 `"/usr/bin:/opt/bin"`，锁死的是 Unix 语义。
pub fn resolves_on_path(
    prog: &str,
    path_env: Option<&str>,
    exists: &dyn Fn(&str) -> bool,
) -> Option<bool> {
    let pe = path_env?;
    if pe.trim().is_empty() {
        return None;
    }
    Some(std::env::split_paths(pe).any(|d| {
        let d = d.to_string_lossy();
        let d = d.trim_end_matches(['/', '\\']);
        !d.is_empty() && exists(&format!("{d}/{prog}"))
    }))
}

// ===== IPC 层：本机与远端各读一次 settings.json。**全程只读。** =====
//
// 远端形状照抄 `mcp.rs::fetch_remote_claude_json`：定值命令（零用户输入拼接 → 零注入面）、
// 30s 超时、大小上限（⚠ **超限拒收+回错**，devbench F10b —— 不是「宽容解析」那一档）。
// 本机直接 `read_to_string`。
// **本模块没有任何写路径**——下方 `this_module_never_writes` 那条测试把它变成门禁，
// 而不是只靠我记得。

/// 该读哪个 `settings.json`。纯函数（`is_dir` 注入），因为**这段逻辑必须有门禁**——
/// 它决定诊断读的是不是用户真正在用的那个文件，读错了还会在 `source` 里报出错误路径。
///
/// 规则：`CLAUDE_CONFIG_DIR` 存在**且确实是个目录** → 用它；否则回落 `~/.claude`。
/// 「确实是个目录」这道判定不能省：环境变量里留一个已删目录或一个文件路径都是真实会发生的，
/// 那种情况下回落到 `~/.claude` 比读一个不存在的路径更有用。
pub fn settings_path(
    cfg_dir_env: Option<&std::path::Path>,
    home: &std::path::Path,
    is_dir: &dyn Fn(&std::path::Path) -> bool,
) -> std::path::PathBuf {
    claude_config_dir(cfg_dir_env, home, is_dir).join("settings.json")
}

/// Claude Code 真正在用的配置目录。**这条规则只准在这里解释一次**——
/// `config_surface.rs` 要把 `~/.claude/...` 形态的申报路径解析成真实路径，
/// 若它自己再写一遍 `CLAUDE_CONFIG_DIR` 判定，两处就会各自漂移
/// （账本 §3「不得为新功能另写一套」的同型问题）。
pub fn claude_config_dir(
    cfg_dir_env: Option<&std::path::Path>,
    home: &std::path::Path,
    is_dir: &dyn Fn(&std::path::Path) -> bool,
) -> std::path::PathBuf {
    match cfg_dir_env {
        Some(d) if is_dir(d) => d.to_path_buf(),
        _ => home.join(".claude"),
    }
}

/// 一次诊断的完整回报（含用于展示的两种待贴片段）。
#[derive(Debug, Clone, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../../src/generated/"))]
pub struct HooksReport {
    pub diagnosis: HooksDiagnosis,
    /// `$HOME/.local/bin/...` 显式路径形态。**不再无条件称"默认推荐"**——
    /// 推荐哪个取决于盘上实况，冲突时 `warning` 会说出来（T03 收的 B04 登记项）。
    pub snippet_home: Snippet,
    /// 裸命令形态，简洁但依赖 PATH。
    pub snippet_bare: Snippet,
    /// 读到的原文路径（展示用，让用户知道诊断的是哪个文件）。
    pub source: String,
}

fn report(diagnosis: HooksDiagnosis, source: String, probe: SnippetProbe) -> HooksReport {
    HooksReport {
        diagnosis,
        snippet_home: snippet(true, &probe),
        snippet_bare: snippet(false, &probe),
        source,
    }
}

/// 诊断**本机**的 `~/.claude/settings.json`。只读。
#[tauri::command]
pub async fn diagnose_local_cc_bus_hooks() -> Result<HooksReport, String> {
    tokio::task::spawn_blocking(|| {
        let Some(home) = dirs::home_dir() else {
            return report(
                diagnose(None, &|_| false),
                copy_text("rsHooksDiag.local.noHome", &[]),
                // 连 HOME 都取不到 → 两项都是"不知道"，**不猜**
                SnippetProbe {
                    home_path_exists: None,
                    on_path: None,
                },
            );
        };
        // **尊重 `CLAUDE_CONFIG_DIR`**（B04 登记项之一）：Claude Code 真正读的是那个目录下的
        // `settings.json`，不是恒定的 `~/.claude/`。本机实测 `CLAUDE_CONFIG_DIR` 指向
        // `~/.claude-accts/z`，而 cc-acct-iso 把它的 `settings.json` **软链**回
        // `~/.claude/settings.json`，所以旧写法**恰好**对得上。
        // 但那是巧合而非保证：某个账号库没做软链（或用户手工维护 CLAUDE_CONFIG_DIR，见 R13）
        // 时，旧写法会**读错文件**，而 `source` 字段还会言之凿凿地报出那个没被读的路径
        // ——诊断错了还给出一个看着很确定的来源，比说不知道更坏。
        // cc-monitor 自己就出多账号隔离这套东西，这里不该假设只有一个 config dir。
        let p = settings_path(
            std::env::var_os("CLAUDE_CONFIG_DIR")
                .map(std::path::PathBuf::from)
                .as_deref(),
            &home,
            &|d: &std::path::Path| d.is_dir(),
        );
        let raw = std::fs::read_to_string(&p).ok();
        // 路径存在性判定用真实文件系统；`$HOME` 前缀先展开再查，否则显式路径一律判成缺失。
        let home2 = home.clone();
        let exists = move |s: &str| -> bool {
            // **`${HOME}/` 也要认**（B04 审计 B04-3）：只认 `$HOME/` 和 `~/` 的话，
            // shell 里等价且常见的 `${HOME}/.local/bin/cc-register` 会被判成
            // 「装了但路径不存在」——**正是本模块文档头声称要避免的那件事，换个花括号就重现了。**
            let expanded = if let Some(rest) = s
                .strip_prefix("$HOME/")
                .or_else(|| s.strip_prefix("${HOME}/"))
                .or_else(|| s.strip_prefix("~/"))
            {
                home2.join(rest)
            } else {
                std::path::PathBuf::from(s)
            };
            expanded.exists()
        };
        // **探测复用同一个 `exists` 闭包**，不新增探测机制（T03）。
        let home_path_exists = Some(
            ["cc-register", "cc-bus-stop-hook"]
                .iter()
                .all(|prog| exists(&format!("$HOME/.local/bin/{prog}"))),
        );
        let path_env = std::env::var("PATH").ok();
        let on_path = ["cc-register", "cc-bus-stop-hook"]
            .iter()
            .map(|prog| resolves_on_path(prog, path_env.as_deref(), &exists))
            // 任一取不到就整体说"不知道"——**不猜**
            .try_fold(true, |acc, r| r.map(|b| acc && b));
        report(
            diagnose(raw.as_deref(), &exists),
            p.to_string_lossy().into_owned(),
            SnippetProbe {
                home_path_exists,
                on_path,
            },
        )
    })
    .await
    .map_err(|e| format!("spawn_blocking: {e}"))
}

// 〔SH1 · V136〕远端那条原先是一条拨号 shell（`cat settings.json` ＋ `-x` / `command -v` 探测，
//   按 basename 宽容匹配）〔散文墓碑〕。今天事实经**那台的后端**问（`footprint-probe` 取家目录 / PATH / agent 家 ·
//   `files-peek` 读 `<agentHome>/settings.json` · 再一趟 `footprint-probe` 逐条 stat）；诊断口径照旧住本文件。

/// 远端诊断问那台后端的期限（三趟往返共用一个总时限）。
const REMOTE_DIAG_BUDGET: std::time::Duration = std::time::Duration::from_secs(30);

/// `$HOME/…` · `${HOME}/…` · `~/…` 按那台的家目录展开；已是绝对路径的原样；其余（裸命令名）回 `None`。
pub(crate) fn expand_on(s: &str, home: &str) -> Option<String> {
    let home = home.trim_end_matches('/');
    let rest = s
        .strip_prefix("$HOME/")
        .or_else(|| s.strip_prefix("${HOME}/"))
        .or_else(|| s.strip_prefix("~/"));
    match rest {
        Some(r) if !home.is_empty() => Some(format!("{home}/{r}")),
        Some(_) => None,
        None if s.starts_with('/') => Some(s.to_string()),
        None => None,
    }
}

/// 要问那台 stat 的路径全集（纯）：诊断在钩子命令里点名过的路径（展开后）· `$HOME/.local/bin` 下那两个程序 ·
/// `PATH` 每一段 × 两个程序名。远端恒是 POSIX ⇒ `PATH` 按 `:` 切（不按本机平台的分隔符）。
pub(crate) fn stat_plan(asked: &[String], home: &str, path: &str) -> Vec<String> {
    let progs = ["cc-register", "cc-bus-stop-hook"];
    let mut out: Vec<String> = asked.iter().filter_map(|s| expand_on(s, home)).collect();
    out.extend(
        progs
            .iter()
            .filter_map(|p| expand_on(&format!("$HOME/.local/bin/{p}"), home)),
    );
    for d in path
        .split(':')
        .map(|d| d.trim_end_matches('/'))
        .filter(|d| d.starts_with('/'))
    {
        out.extend(progs.iter().map(|p| format!("{d}/{p}")));
    }
    out.sort();
    out.dedup();
    out.truncate(256); // `footprint-probe` 一趟最多 256 条
    out
}

/// 远端的诊断：事实问那台后端，判定用本文件那一套（与本机同一个 `diagnose` / `snippet`）。
#[tauri::command]
pub async fn diagnose_remote_cc_bus_hooks(origin: String) -> Result<HooksReport, String> {
    use crate::backend::control::frame_query::{call, Deadline};
    let o = crate::origin::Origin(origin.clone());
    let deadline = Deadline::within(REMOTE_DIAG_BUDGET);
    let env = call(&o, "footprint-probe", serde_json::json!({}), deadline).await?;
    let s = |k: &str| env["env"][k].as_str().unwrap_or("").to_string();
    let (home, path, agent_home) = (s("home"), s("path"), s("agentHome"));
    let door = crate::user_files::BackendDoor::new(o.clone());
    let (raw, note) = match crate::user_files::Door::peek(&door, &agent_home, "settings.json").await
    {
        Ok(p) => (p.text, None),
        Err(e) => (
            None,
            Some(copy_text("rsHooksDiag.remote.peekFailed", &[("e", &e)])),
        ),
    };
    // 先空跑一遍诊断，记下它会问哪几个路径；再一趟 stat 问完。
    let plan = {
        let asked = std::cell::RefCell::new(Vec::<String>::new());
        let record = |q: &str| {
            asked.borrow_mut().push(q.to_string());
            false
        };
        diagnose(raw.as_deref(), &record);
        let asked = asked.into_inner();
        stat_plan(&asked, &home, &path)
    };
    let got = call(
        &o,
        "footprint-probe",
        serde_json::json!({ "stat": plan }),
        deadline,
    )
    .await?;
    let present: std::collections::BTreeSet<String> = plan
        .iter()
        .filter(|p| got["stat"][p.as_str()].get("kind").and_then(|k| k.as_str()) == Some("file"))
        .cloned()
        .collect();
    let exists = |q: &str| expand_on(q, &home).is_some_and(|p| present.contains(&p));
    let progs = ["cc-register", "cc-bus-stop-hook"];
    let known = !home.is_empty();
    let probe = SnippetProbe {
        home_path_exists: known.then(|| {
            progs
                .iter()
                .all(|p| exists(&format!("$HOME/.local/bin/{p}")))
        }),
        on_path: (!path.trim().is_empty()).then(|| {
            progs.iter().all(|p| {
                path.split(':')
                    .map(|d| d.trim_end_matches('/'))
                    .filter(|d| d.starts_with('/'))
                    .any(|d| present.contains(&format!("{d}/{p}")))
            })
        }),
    };
    let mut d = diagnose(raw.as_deref(), &exists);
    if let Some(n) = note {
        d.note = n;
    }
    Ok(report(
        d,
        format!("[{origin}] {agent_home}/settings.json"),
        probe,
    ))
}

#[cfg(test)]
#[path = "../../../tests/bridge/hooks_diag_tests.rs"]
mod tests;
