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

/// 一个钩子的诊断结论。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../src/generated/"))]
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
#[cfg_attr(test, ts(export, export_to = "../../src/generated/"))]
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
            note: "没读到 ~/.claude/settings.json（文件不存在或读取失败）".to_string(),
        };
    };
    // BOM 容忍（同 mcp.rs 读 ~/.claude.json 的处理）
    let Ok(v) = serde_json::from_str::<serde_json::Value>(raw.trim_start_matches('\u{feff}'))
    else {
        return HooksDiagnosis {
            session_start: HookState::NotInstalled,
            stop: HookState::NotInstalled,
            note: "~/.claude/settings.json 不是合法 JSON —— 未改动它，请先自行修复".to_string(),
        };
    };
    if !v.is_object() {
        return HooksDiagnosis {
            session_start: HookState::NotInstalled,
            stop: HookState::NotInstalled,
            note: "~/.claude/settings.json 顶层不是对象".to_string(),
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
#[cfg_attr(test, ts(export, export_to = "../../src/generated/"))]
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
        Some(
            "你选的是 $HOME 显式路径形态，但 $HOME/.local/bin/ 下找不到 cc-register / \
             cc-bus-stop-hook——照这段贴进去会得到一个指不到东西的钩子（钩子诊断会报 path-missing）。\
             要么改选「裸命令」形态，要么先把 cc-bus 装到那个位置。"
                .to_string(),
        )
    } else if !home && probe.on_path == Some(false) {
        Some(
            "你选的是裸命令形态，但这两个命令不在 PATH 上——照这段贴进去钩子跑不起来。\
             改选「$HOME 显式路径」形态，或把它们所在目录加进 PATH。"
                .to_string(),
        )
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
// 30s 超时、大小上限、宽容解析。本机直接 `read_to_string`。
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
#[cfg_attr(test, ts(export, export_to = "../../src/generated/"))]
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
                "（取不到 HOME）".to_string(),
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

/// 一条**定值**命令：读回 settings.json 原文 + 那两个钩子程序在远端存不存在。
/// `origin` 只用于选连接配置，**不参与命令串拼接**（同 `mcp.rs` 那条 CMD 常量的形状）。
const REMOTE_HOOKS_CMD: &str = concat!(
    r#"cat "$HOME/.claude/settings.json" 2>/dev/null; "#,
    r#"printf '\n@@CCMON-HOOKS-SPLIT@@\n'; "#,
    // **两类命中各打一个标记**（T03 审计阻塞 3）：`X` = `$HOME/.local/bin/` 下 `-x` 命中，
    // `P` = PATH 上 `command -v` 命中。不打标记的话两者在回报里**完全分不出来**，
    // 于是同一份含混证据被用出了两个互相矛盾的结论：`on_path` 说"分不清所以不猜"，
    // 而 `home_path_exists` 却确定地说"在"。真实假阴性：远端 cc-register 只装在
    // `/usr/local/bin` 且在 PATH 上时，`command -v` 打出它 → 按 basename 匹配上 →
    // `home_path_exists = true` → `$HOME` 形态**不警示** → 用户贴上去正是一个
    // path-missing 钩子，就是这次要修的那件事。
    r#"for f in "$HOME/.local/bin/cc-register" "$HOME/.local/bin/cc-bus-stop-hook"; do "#,
    r#"[ -x "$f" ] && printf 'X\t%s\n' "$f"; done; "#,
    // **把 PATH 上的真实路径也吐出来**（B04 审计 B04-7）：原先只 `-x` 两个固定路径 +
    // 两行 `HAS_PATH_*` 标记，而那两个标记的 basename 与目标不相等、等于没参与匹配。
    // 于是装在 `/usr/local/bin` 且在 PATH 上的**能用的安装**会被报成「指不到」——
    // 注释说这是"保守方向"，但对能用的安装报假警报，方向并不保守。
    r#"for p in cc-register cc-bus-stop-hook; do "#,
    r#"h="$(command -v "$p" 2>/dev/null)" && printf 'P\t%s\n' "$h"; done; true"#
);

pub const HOOKS_SPLIT_MARKER: &str = "@@CCMON-HOOKS-SPLIT@@";

/// 解析远端探测回报：`X\t<path>` = `$HOME/.local/bin/` 下 `-x` 命中，
/// `P\t<path>` = PATH 上 `command -v` 命中。返回 `(宽容清单, 探测结论)`。
///
/// 抽成纯函数是因为**这段此前零覆盖**（T03 审计阻塞 3 实测：把远端
/// `home_path_exists` 改成 `= true`，`cargo test hooks_diag` 24 项照样全绿）。
/// 它藏在 `#[tauri::command] async fn` 里、要一条真 ssh 才走得到 = 不可测 = 没门禁。
///
/// **旧协议（不打标记）的远端一律落到"说不知道"**：两个字段都 `None`，
/// 不拿含混回报当精确证据用。
pub fn parse_remote_probe(probe_part: &str) -> (Vec<String>, SnippetProbe) {
    let (mut x_hits, mut p_hits, mut legacy) = (Vec::new(), Vec::new(), Vec::new());
    for line in probe_part.lines() {
        let l = line.trim();
        if l.is_empty() {
            continue;
        }
        match l.split_once('\t') {
            Some(("X", v)) => x_hits.push(v.to_string()),
            Some(("P", v)) => p_hits.push(v.to_string()),
            _ => legacy.push(l.to_string()),
        }
    }
    let basename_hit = |list: &[String], s: &str| -> bool {
        let base = s.rsplit('/').next().unwrap_or(s);
        list.iter()
            .any(|p| p.rsplit('/').next().unwrap_or(p) == base)
    };
    // 有任一带标记的行 → 新协议；一行都没有（对方啥也没找到）也算新协议；
    // 只有无标记的行 → 旧协议，分不清。
    let tagged = !x_hits.is_empty() || !p_hits.is_empty() || legacy.is_empty();
    let progs = ["cc-register", "cc-bus-stop-hook"];
    let probe = SnippetProbe {
        home_path_exists: tagged.then(|| progs.iter().all(|p| basename_hit(&x_hits, p))),
        on_path: tagged.then(|| progs.iter().all(|p| basename_hit(&p_hits, p))),
    };
    let lenient: Vec<String> = x_hits.into_iter().chain(p_hits).chain(legacy).collect();
    (lenient, probe)
}

/// 诊断**远端**的 `~/.claude/settings.json`。只读，绝不写远端任何文件。
#[tauri::command]
pub async fn diagnose_remote_cc_bus_hooks(origin: String) -> Result<HooksReport, String> {
    use tokio::io::AsyncReadExt;
    let cfg = crate::load_remote_config_by_label(&origin)
        .ok_or_else(|| format!("远端 '{origin}' 未配置或未启用"))?;
    let read = async {
        let stream = crate::ssh_source::connect_and_exec_cmd(&cfg, REMOTE_HOOKS_CMD).await?;
        let mut buf = Vec::new();
        stream
            .take(4 * 1024 * 1024)
            .read_to_end(&mut buf)
            .await
            .map_err(|e| format!("读远端 settings.json 失败: {e}"))?;
        Ok::<Vec<u8>, String>(buf)
    };
    let raw = tokio::time::timeout(std::time::Duration::from_secs(30), read)
        .await
        .map_err(|_| format!("远端 '{origin}' 读取超时（30s）"))??;
    let text = String::from_utf8_lossy(&raw).into_owned();
    tokio::task::spawn_blocking(move || {
        let (json_part, probe_part) = match text.split_once(HOOKS_SPLIT_MARKER) {
            Some((a, b)) => (a.to_string(), b.to_string()),
            // 缺分隔标记 → 宽容降级：全当 JSON，探测结果为空（于是显式路径一律判 PathMissing，
            // 这是**保守**方向：宁可说"指不到"也不要假称"装好了"）。
            None => (text.clone(), String::new()),
        };
        let (lenient, probe) = parse_remote_probe(&probe_part);
        // `exists` 的宽容语义**刻意不变**（B04 审计 B04-7 的决定）：`-x` 与 PATH 命中
        // 任一都算"这个程序在远端存在"。这里只是不再拿它去回答"$HOME 那个具体路径在不在"。
        let exists = move |s: &str| -> bool {
            let base = s.rsplit('/').next().unwrap_or(s);
            lenient
                .iter()
                .any(|p| p.rsplit('/').next().unwrap_or(p) == base)
        };
        let d = if json_part.trim().is_empty() {
            diagnose(None, &exists)
        } else {
            diagnose(Some(&json_part), &exists)
        };
        report(d, format!("[{origin}] ~/.claude/settings.json"), probe)
    })
    .await
    .map_err(|e| format!("spawn_blocking: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn always(_: &str) -> bool {
        true
    }
    fn never(_: &str) -> bool {
        false
    }

    // ===== 该读哪个 settings.json（B04 登记项：尊重 CLAUDE_CONFIG_DIR）=====
    #[test]
    fn settings_path_honors_claude_config_dir() {
        let home = std::path::Path::new("/home/u");
        let yes = |_: &std::path::Path| true;
        let no = |_: &std::path::Path| false;

        // 未设 → 回落 ~/.claude
        assert_eq!(
            settings_path(None, home, &yes),
            std::path::PathBuf::from("/home/u/.claude/settings.json")
        );
        // 设了且是目录 → 用它。**这是本机实况**：CLAUDE_CONFIG_DIR=~/.claude-accts/z
        let acct = std::path::Path::new("/home/u/.claude-accts/z");
        assert_eq!(
            settings_path(Some(acct), home, &yes),
            std::path::PathBuf::from("/home/u/.claude-accts/z/settings.json")
        );
        // 设了但不是目录（已删 / 指向文件）→ 回落，而不是读一个不存在的路径
        assert_eq!(
            settings_path(Some(acct), home, &no),
            std::path::PathBuf::from("/home/u/.claude/settings.json")
        );
    }

    /// 旧写法恒读 `~/.claude/settings.json`。本机之所以**恰好**没出错，是因为
    /// cc-acct-iso 把账号库的 settings.json 软链回了那里——**巧合不是保证**。
    /// 这条断言的是"我们确实按 CLAUDE_CONFIG_DIR 走了"，而不是"结果碰巧一样"。
    #[test]
    fn config_dir_is_not_ignored_even_when_symlinked_to_the_same_place() {
        let home = std::path::Path::new("/home/u");
        let acct = std::path::Path::new("/home/u/.claude-accts/z");
        let got = settings_path(Some(acct), home, &|_| true);
        assert!(
            got.starts_with(acct),
            "必须读 CLAUDE_CONFIG_DIR 下那份（哪怕它软链到别处），实得 {got:?}"
        );
        assert_ne!(
            got,
            std::path::PathBuf::from("/home/u/.claude/settings.json")
        );
    }

    // ===== 核心：用户盘上的**真实**形态不得被误判 =====
    #[test]
    fn real_user_settings_is_installed_not_misreported() {
        // 逐字取自 2026-07-28 的 ~/.claude/settings.json
        let raw = r#"{"hooks":{
          "SessionStart":[{"hooks":[{"type":"command","command":"\"$HOME/.local/bin/cc-register\" >/dev/null 2>&1 || true"}]}],
          "Stop":[{"hooks":[{"type":"command","command":"\"$HOME/.local/bin/cc-bus-stop-hook\""}]}]}}"#;
        let d = diagnose(Some(raw), &always);
        assert!(
            d.session_start.is_working(),
            "实测形态必须判为已装: {:?}",
            d.session_start
        );
        assert!(d.stop.is_working(), "{:?}", d.stop);
        // 且必须是"显式路径"那一态，不能滑成 PATH 态
        assert!(matches!(d.session_start, HookState::InstalledAtPath { .. }));
    }

    #[test]
    fn canonical_snippet_form_is_also_installed() {
        // cc-bus-install.sh 的规范片段（裸命令）同样要认
        let raw = r#"{"hooks":{
          "SessionStart":[{"hooks":[{"type":"command","command":"cc-register >/dev/null 2>&1 || true"}]}],
          "Stop":[{"hooks":[{"type":"command","command":"cc-bus-stop-hook"}]}]}}"#;
        let d = diagnose(Some(raw), &never); // never：证明裸命令**不查路径存在性**
        assert!(matches!(
            d.session_start,
            HookState::InstalledViaPath { .. }
        ));
        assert!(matches!(d.stop, HookState::InstalledViaPath { .. }));
    }

    // ===== 真正的第三态：看着像装了，其实指不到东西 =====
    #[test]
    fn explicit_path_that_does_not_exist_is_the_third_state() {
        let raw = r#"{"hooks":{"SessionStart":[{"hooks":[{"type":"command",
          "command":"/opt/gone/cc-register"}]}]}}"#;
        let d = diagnose(Some(raw), &never);
        match &d.session_start {
            HookState::PathMissing { path, .. } => assert_eq!(path, "/opt/gone/cc-register"),
            other => panic!("应为 PathMissing，实得 {other:?}"),
        }
        assert!(!d.session_start.is_working(), "PathMissing 绝不能算能用");
    }

    #[test]
    fn same_name_elsewhere_but_present_counts_as_installed() {
        // 装在别处但**确实存在** → 算已装（它能跑）。只有不存在才是问题。
        let raw = r#"{"hooks":{"Stop":[{"hooks":[{"type":"command",
          "command":"/usr/local/bin/cc-bus-stop-hook"}]}]}}"#;
        let d = diagnose(Some(raw), &always);
        assert!(d.stop.is_working());
    }

    // ===== program_of：只做够用的事，看不懂就说不知道 =====
    #[test]
    fn program_of_handles_real_shapes() {
        assert_eq!(program_of("cc-register").unwrap().0, "cc-register");
        assert_eq!(
            program_of("\"$HOME/.local/bin/cc-register\" >/dev/null 2>&1 || true").unwrap(),
            ("cc-register".into(), "$HOME/.local/bin/cc-register".into())
        );
        // 前导环境赋值要剥掉
        assert_eq!(
            program_of("FOO=1 BAR=2 cc-register").unwrap().0,
            "cc-register"
        );
        // `=` 在首位不是赋值
        assert_eq!(program_of("=weird").unwrap().0, "=weird");
        assert_eq!(program_of("   "), None);
        assert_eq!(program_of(""), None);
        assert_eq!(program_of("''"), None);
    }

    #[test]
    fn unrelated_hooks_do_not_false_positive() {
        let raw = r#"{"hooks":{"SessionStart":[{"hooks":[{"type":"command",
          "command":"some-other-tool --register"}]}]}}"#;
        let d = diagnose(Some(raw), &always);
        assert_eq!(
            d.session_start,
            HookState::NotInstalled,
            "别的工具的钩子不得算成 cc-bus 的"
        );
    }

    #[test]
    fn coexisting_hooks_do_not_require_exclusivity() {
        // 同一事件挂多条：别的工具在前、cc-bus 在后，仍算已装
        let raw = r#"{"hooks":{"SessionStart":[
          {"hooks":[{"type":"command","command":"other-tool"}]},
          {"hooks":[{"type":"command","command":"cc-register"}]}]}}"#;
        assert!(diagnose(Some(raw), &always).session_start.is_working());
    }

    #[test]
    fn working_entry_wins_over_path_missing() {
        // 一条坏的 + 一条好的 → 报好的（用户实际能用）
        let raw = r#"{"hooks":{"SessionStart":[
          {"hooks":[{"type":"command","command":"/gone/cc-register"}]},
          {"hooks":[{"type":"command","command":"cc-register"}]}]}}"#;
        let d = diagnose(Some(raw), &never);
        assert!(matches!(
            d.session_start,
            HookState::InstalledViaPath { .. }
        ));
    }

    // ===== 脏输入逐层容忍，不抛、不让坏条目吃掉整份诊断 =====
    #[test]
    fn malformed_input_degrades_with_a_reason() {
        assert!(diagnose(None, &always).note.contains("没读到"));
        assert!(diagnose(Some("{not json"), &always)
            .note
            .contains("不是合法 JSON"));
        assert!(diagnose(Some("[1,2]"), &always)
            .note
            .contains("顶层不是对象"));
        // 结构不对的各层：一律降级成未装，且不 panic
        for raw in [
            r#"{}"#,
            r#"{"hooks":null}"#,
            r#"{"hooks":{"SessionStart":"notarray"}}"#,
            r#"{"hooks":{"SessionStart":[{"nohooks":1}]}}"#,
            r#"{"hooks":{"SessionStart":[{"hooks":[{"type":"command"}]}]}}"#,
            r#"{"hooks":{"SessionStart":[{"hooks":[{"command":"   "}]}]}}"#,
        ] {
            let d = diagnose(Some(raw), &always);
            assert_eq!(d.session_start, HookState::NotInstalled, "raw={raw}");
            assert!(d.note.is_empty(), "结构问题不该报成读取失败: raw={raw}");
        }
    }

    #[test]
    fn bom_is_tolerated() {
        let raw =
            "\u{feff}{\"hooks\":{\"Stop\":[{\"hooks\":[{\"command\":\"cc-bus-stop-hook\"}]}]}}";
        assert!(diagnose(Some(raw), &always).stop.is_working());
    }

    // ===== 生成的待贴文本必须是合法 JSON，且两种形态都对 =====

    /// 一切正常的探测（两种形态都不该有 warning）。
    fn ok_probe() -> SnippetProbe {
        SnippetProbe {
            home_path_exists: Some(true),
            on_path: Some(true),
        }
    }

    #[test]
    fn snippet_is_valid_json_and_round_trips() {
        for home in [true, false] {
            let sn = snippet(home, &ok_probe());
            let v: serde_json::Value =
                serde_json::from_str(&sn.text).expect("生成的片段必须是合法 JSON");
            // 把自己生成的东西再喂给自己的诊断——闭环，防止生成一段自己都不认的文本
            let d = diagnose(Some(&sn.text), &always);
            assert!(
                d.session_start.is_working(),
                "home={home} 生成的片段自己都不认: {}",
                sn.text
            );
            assert!(d.stop.is_working(), "home={home}");
            assert!(v.get("hooks").is_some());
            assert!(sn.warning.is_none(), "探测一切正常时不该有警示");
        }
        assert!(snippet(true, &ok_probe())
            .text
            .contains("$HOME/.local/bin/"));
        assert!(!snippet(false, &ok_probe()).text.contains("$HOME"));
    }

    // ===== T03 收的 B04 登记项：片段必须看盘上实况 =====

    /// **上一版 `snippet(home: bool)` 只按布尔选形态，完全不看实况**：于是面板可以推荐
    /// `$HOME/.local/bin/cc-register` 而那个文件根本不存在，贴上去就是一个 `path-missing`
    /// 的钩子，而这一步没有任何测试能发现。这条测试就是那个缺口。
    #[test]
    fn home_form_warns_when_the_path_is_not_on_disk() {
        let probe = SnippetProbe {
            home_path_exists: Some(false),
            on_path: Some(true),
        };
        let sn = snippet(true, &probe);
        let w = sn.warning.expect("显式路径形态 + 路径不存在 → 必须警示");
        assert!(w.contains("$HOME/.local/bin/"), "要指名那个路径：{w}");
        assert!(w.contains("path-missing"), "要说清后果：{w}");
        // **闭环验证后果是真的**：把这段喂回自己的诊断，`exists` 说不存在 → 真的 PathMissing
        let d = diagnose(Some(&sn.text), &|_| false);
        assert!(
            matches!(d.session_start, HookState::PathMissing { .. }),
            "警示说的后果得是真的，实得 {:?}",
            d.session_start
        );
        // 同一份探测下裸命令形态没问题 → 不该警示（否则两种形态都报警，用户无从选择）
        assert!(snippet(false, &probe).warning.is_none());
    }

    #[test]
    fn bare_form_warns_when_not_on_path() {
        let probe = SnippetProbe {
            home_path_exists: Some(true),
            on_path: Some(false),
        };
        let w = snippet(false, &probe)
            .warning
            .expect("裸命令形态 + 不在 PATH → 必须警示");
        assert!(w.contains("PATH"), "{w}");
        assert!(snippet(true, &probe).warning.is_none());
    }

    /// **取不到就别猜**：`on_path: None` 时裸命令形态不许警示
    /// （报一个我们并不知道的问题，和漏报一样是失信）。
    #[test]
    fn unknown_path_status_does_not_fabricate_a_warning() {
        let probe = SnippetProbe {
            home_path_exists: Some(true),
            on_path: None,
        };
        assert!(snippet(false, &probe).warning.is_none());
        assert!(snippet(true, &probe).warning.is_none());
    }

    /// **按平台构造 PATH**（T03 审计阻塞 1）。旧版这条测试硬编码 `"/usr/bin:/opt/bin"`，
    /// 锁死的是 Unix 语义——于是 `split(':')` 这个在**生产平台 Windows 上算错**的实现
    /// 被它钉成了绿的。现在用 `std::env::join_paths` 按当前平台拼，Linux 与 Windows 同一条测试。
    #[test]
    fn resolves_on_path_uses_the_injected_exists() {
        let sep_dir = if cfg!(windows) {
            "C:\\opt\\bin"
        } else {
            "/opt/bin"
        };
        let other = if cfg!(windows) {
            "C:\\Windows"
        } else {
            "/usr/bin"
        };
        let join = |dirs: &[&str]| -> String {
            std::env::join_paths(dirs.iter().map(std::path::Path::new))
                .unwrap()
                .to_string_lossy()
                .into_owned()
        };
        let want = format!("{}/cc-register", sep_dir.trim_end_matches(['/', '\\']));
        let ex = |s: &str| s == want;
        assert_eq!(
            resolves_on_path("cc-register", Some(&join(&[other, sep_dir])), &ex),
            Some(true),
            "PATH 必须按当前平台的分隔符切"
        );
        assert_eq!(
            resolves_on_path("cc-register", Some(&join(&[other])), &ex),
            Some(false)
        );
        // **取不到 PATH → None，不猜 false**
        assert_eq!(resolves_on_path("cc-register", None, &ex), None);
        assert_eq!(resolves_on_path("cc-register", Some("   "), &ex), None);
    }

    /// **结构性守卫：PATH 切分必须用平台自己的切分器，不许写死分隔符。**
    ///
    /// 为什么不是行为测试：`std::env::split_paths` **本身就是平台相关的**
    /// （Linux 上按 `':'`、Windows 上按 `';'`），所以"喂一段 Windows 形态的 PATH，
    /// 断言它不被按 `':'` 切"这个性质在 Linux 上**根本不成立**——我第一版就是这么写的，
    /// 当场红在 `把盘符当目录了：C/cc-register | \Windows;C/cc-register | …`。
    /// 那不是实现的错，是我把一个平台相关的行为断言成了平台无关的。
    ///
    /// 真正想守的是**源码性质**：这里必须调 `std::env::split_paths`，
    /// 不许出现写死的 `split(':')`。行为侧由上面那条 `join_paths` 测试覆盖——
    /// 它在 CI 的 `windows-latest` job 上跑的就是 Windows 语义，那才是真覆盖。
    #[test]
    fn path_splitting_delegates_to_the_platform() {
        let src = include_str!("hooks_diag.rs");
        let body_start = src
            .find("pub fn resolves_on_path(")
            .expect("找不到 resolves_on_path——守卫失效了");
        let body_end = src[body_start..]
            .find("\n}\n")
            .map(|i| body_start + i)
            .expect("取不到函数体");
        let body: String = src[body_start..body_end]
            .lines()
            .filter(|l| !l.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");
        // 反向自检：剥完还看得见真代码
        assert!(body.contains("path_env?"), "剥过头了，守卫在空转");
        assert!(
            body.contains("std::env::split_paths"),
            "PATH 切分必须交给平台，实得:\n{body}"
        );
        for bad in ["split(':')", "split(';')", "split(\":\")"] {
            assert!(
                !body.contains(bad),
                "不许写死分隔符 {bad}——生产平台是 Windows（ci.yml/release.yml 都是 windows-latest）"
            );
        }
    }

    /// **远端探测解析**（T03 审计阻塞 3）。此前这段藏在 `#[tauri::command] async fn` 里，
    /// 要一条真 ssh 才走得到 = 不可测 = 没门禁；审计实测把远端 `home_path_exists` 改成
    /// `= true`，24 项照样全绿。
    #[test]
    fn remote_probe_uses_each_kind_of_evidence_precisely() {
        // 两个程序都在 $HOME/.local/bin 下 `-x` 命中 + 都在 PATH 上
        let (lenient, p) = parse_remote_probe(
            "X\t/home/u/.local/bin/cc-register\nX\t/home/u/.local/bin/cc-bus-stop-hook\n\
             P\t/home/u/.local/bin/cc-register\nP\t/home/u/.local/bin/cc-bus-stop-hook\n",
        );
        assert_eq!(p.home_path_exists, Some(true));
        assert_eq!(p.on_path, Some(true));
        assert_eq!(lenient.len(), 4, "宽容清单要含全部命中（B04-7 的决定不变）");

        // **审计指出的真实假阴性**：只装在 /usr/local/bin 且在 PATH 上——
        // 旧代码按 basename 匹配会把它算成 `$HOME` 路径存在 → `$HOME` 形态不警示 →
        // 用户贴上去正是一个 path-missing 钩子。
        let (_, p) = parse_remote_probe(
            "P\t/usr/local/bin/cc-register\nP\t/usr/local/bin/cc-bus-stop-hook\n",
        );
        assert_eq!(
            p.home_path_exists,
            Some(false),
            "PATH 上有 ≠ $HOME/.local/bin 下有"
        );
        assert_eq!(p.on_path, Some(true));

        // 只有一个命中 → 不能说"都在"
        let (_, p) = parse_remote_probe("X\t/home/u/.local/bin/cc-register\n");
        assert_eq!(p.home_path_exists, Some(false));

        // 新协议但一个都没找到 → 确定地说"都不在"
        let (lenient, p) = parse_remote_probe("");
        assert_eq!(p.home_path_exists, Some(false));
        assert_eq!(p.on_path, Some(false));
        assert!(lenient.is_empty());

        // **旧协议（不打标记）→ 两项都说不知道**，不拿含混回报当精确证据
        let (lenient, p) = parse_remote_probe("/home/u/.local/bin/cc-register\n");
        assert_eq!(p.home_path_exists, None, "旧协议不许下结论");
        assert_eq!(p.on_path, None);
        assert_eq!(lenient.len(), 1, "但宽容清单仍要用它（诊断照旧宽容）");
    }

    /// 两个字段现在**同一档口径**：`None` 一律不警示。
    #[test]
    fn unknown_home_path_does_not_fabricate_a_warning() {
        let probe = SnippetProbe {
            home_path_exists: None,
            on_path: None,
        };
        assert!(snippet(true, &probe).warning.is_none());
        assert!(snippet(false, &probe).warning.is_none());
    }

    /// **B04 登记项②**：`trim_matches` 逐字符两端剥，会把不配对的也剥掉。
    #[test]
    fn unquote_only_strips_a_matched_pair() {
        assert_eq!(unquote_once("\"x\""), "x");
        assert_eq!(unquote_once("'x'"), "x");
        // 不配对 → 原样返回（旧的 trim_matches 会剥成 `a`）
        assert_eq!(unquote_once("\"a'"), "\"a'");
        assert_eq!(unquote_once("\"a"), "\"a");
        assert_eq!(unquote_once("a\""), "a\"");
        // 只剥**一层**（旧的会把 `''x''` 剥干净）
        assert_eq!(unquote_once("''x''"), "'x'");
        assert_eq!(unquote_once(""), "");
        assert_eq!(unquote_once("\""), "\"");
        // 真实形态照旧
        assert_eq!(
            unquote_once("\"$HOME/.local/bin/cc-register\""),
            "$HOME/.local/bin/cc-register"
        );
        // 走到 program_of 上：不配对引号不该被当成正常路径
        let (base, full) = program_of("\"$HOME/.local/bin/cc-register").unwrap();
        assert_eq!(base, "cc-register");
        assert_eq!(
            full, "\"$HOME/.local/bin/cc-register",
            "不配对的引号要留着，让下游看到这条命令形状可疑"
        );
    }

    // ===== B04 审计逼出来的补漏 =====

    /// **B04-4**：包装器写法必须判「无法判断」，不得判「未装」。
    /// 源码注释写着"看不懂就返回 None 而不是猜"，但 None 落到 NotInstalled → UI 渲染成
    /// 确定性的"未装" → 用户去贴一份重复的钩子。**猜"未装"和猜"已装"一样是猜。**
    #[test]
    fn wrapper_forms_are_unknown_not_not_installed() {
        for cmd in [
            r#"sh -c "cc-register""#,
            "bash -lc cc-register",
            "exec cc-register",
            "command cc-register",
            "/usr/bin/env cc-register",
            "timeout 5 cc-register",
            "nohup cc-register &",
            "true && cc-register",
            r#""$HOME/.local/bin/cc-register"; echo hi"#,
        ] {
            let st = classify_command(cmd, "cc-register", &always);
            match st {
                Some(HookState::Unknown { .. }) => {}
                // 直接执行也可以（说明 program_of 认出来了，更好）
                Some(s) if s.is_working() => {}
                other => panic!("{cmd:?} 不该被判成 {other:?}——那会让用户以为没装"),
            }
        }
        // 真的没提到目标 → 仍然是 NotInstalled
        assert_eq!(
            classify_command("other-tool --register", "cc-register", &always),
            None
        );
    }

    #[test]
    fn unknown_is_not_working_but_is_flagged_unknown() {
        let st = classify_command("sh -c cc-register", "cc-register", &always).unwrap();
        assert!(!st.is_working(), "无法判断不能算能用");
        assert!(st.is_unknown(), "要能被 UI 识别成中性态而非问题");
    }

    /// **B04-3**：`${HOME}/` 花括号形态此前被判成「装了但路径不存在」——
    /// 正是本模块文档头声称要避免的那件事，只是换了个花括号写法就重现了。
    #[test]
    fn brace_home_form_is_not_a_false_alarm() {
        // exists 闭包按真实实现的展开逻辑：认 $HOME/ ${HOME}/ ~/
        let home = std::path::PathBuf::from("/home/u");
        let exists = move |s: &str| -> bool {
            let expanded = if let Some(rest) = s
                .strip_prefix("$HOME/")
                .or_else(|| s.strip_prefix("${HOME}/"))
                .or_else(|| s.strip_prefix("~/"))
            {
                home.join(rest)
            } else {
                std::path::PathBuf::from(s)
            };
            // 模拟"这个路径存在"
            expanded == std::path::PathBuf::from("/home/u/.local/bin/cc-register")
        };
        for form in [
            "$HOME/.local/bin/cc-register",
            "${HOME}/.local/bin/cc-register",
            "~/.local/bin/cc-register",
        ] {
            let st = classify_command(form, "cc-register", &exists).unwrap();
            assert!(
                matches!(st, HookState::InstalledAtPath { .. }),
                "{form} 应判为已装，实得 {st:?}"
            );
        }
    }

    /// **B04-5**：`REMOTE_HOOKS_CMD` 此前**一条守卫都没有**。
    /// 对比 `cc_bus.rs` 的 `CC_BUS_CAT_CMD`：既有常量守卫又有调用点守卫。
    /// 「绝不写远端任何文件」这句话在 B04 里曾经是纯口头的。
    #[test]
    fn remote_command_is_readonly_and_reaches_ssh_unmodified() {
        // 常量本身：零插值、无写动词
        assert!(!REMOTE_HOOKS_CMD.contains("{}"));
        assert!(!REMOTE_HOOKS_CMD.contains("$1"));
        for w in [
            "rm ", "mv ", ">>", "tee ", "truncate", "chmod", "kill", "ln -s",
        ] {
            assert!(!REMOTE_HOOKS_CMD.contains(w), "只读命令里不该有 {w:?}");
        }
        assert!(REMOTE_HOOKS_CMD.contains(HOOKS_SPLIT_MARKER));
        assert!(
            REMOTE_HOOKS_CMD.trim_end().ends_with("true"),
            "缺文件时仍须 rc=0"
        );
        // **调用点**：必须原样交给 SSH（包一层 format! 就不再是定值）
        let code = non_test_code();
        assert!(
            code.contains("connect_and_exec_cmd(&cfg, REMOTE_HOOKS_CMD)"),
            "定值命令必须原样交给 SSH"
        );
        assert_eq!(
            code.matches("REMOTE_HOOKS_CMD").count(),
            2,
            "常量只准出现两次：定义 + 唯一调用点"
        );
    }

    /// 取本文件的**非测试、非注释**代码。
    /// 剥注释是必需的——B04 审计实测：朴素子串扫会把**注释里**写的 `File::create`
    /// 当成真调用（假红）。
    fn non_test_code() -> String {
        let src = include_str!("hooks_diag.rs");
        // **扫全文再去掉测试模块**，而不是"只取第一个 #[cfg(test)] 之前"——
        // 后者对写在测试模块**之后**的非测试代码是盲区（B04 审计指出的第二个洞）。
        let marker = concat!("#[cfg", "(test)]");
        let code = src.split(marker).next().unwrap_or(src);
        code.lines()
            .filter(|l| {
                let t = l.trim_start();
                !t.starts_with("//") && !t.starts_with('*') && !t.starts_with("/*")
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn non_test_code_helper_is_sane() {
        let c = non_test_code();
        assert!(c.contains("pub fn diagnose"), "剥过头了");
        assert!(c.contains("pub fn snippet"), "剥过头了");
        assert!(!c.contains("剥注释是必需的"), "注释没剥干净");
        assert!(c.len() > 2000, "剩下的代码太少，守卫形同虚设");
    }

    /// **本模块绝不写盘。**用户定调不改 `~/.claude/settings.json`；`cc-bus-install.sh`
    /// 第 3 行同样拒绝改它。这条守卫把红线变成门禁。
    ///
    /// **第一版是黑名单，被审计当场绕过**：它只列了 6 个字面量
    /// （`fs::write`/`File::create`/`OpenOptions`/`Command::new`/`std::process`/`remove_file`），
    /// 审计往非测试段插了 `fs::rename` + `File::options` + `symlink` + `remove_dir_all`
    /// 的真写盘代码，**15 项测试全绿**。黑名单永远漏，因为写盘的写法列不完。
    ///
    /// 改成**白名单**：非测试代码里凡是 `std::fs::` 的用法，只准是 `read_to_string`。
    /// 想新增任何文件操作都会当场红——包括我还没想到的那些写法。
    #[test]
    fn this_module_never_writes() {
        let code = non_test_code();

        // ① std::fs:: 的白名单——只准读
        let fs_uses: Vec<&str> = code
            .match_indices("fs::")
            .map(|(i, _)| {
                let rest = &code[i + 4..];
                let end = rest
                    .find(|c: char| !c.is_alphanumeric() && c != '_')
                    .unwrap_or(rest.len());
                &rest[..end]
            })
            .collect();
        for u in &fs_uses {
            assert_eq!(
                *u, "read_to_string",
                "本模块只准 fs::read_to_string，发现 fs::{u}"
            );
        }
        // 反向自检：**确实扫到了**那唯一一处读（否则白名单在空转）
        assert!(
            fs_uses.contains(&"read_to_string"),
            "一处 fs 用法都没扫到，守卫在空转"
        );

        // ② 其它写/执行入口一律不准（这些不经 fs:: 前缀）
        for bad in [
            concat!("File::", "create"),
            concat!("File::", "options"),
            concat!("Open", "Options"),
            concat!("Command::", "new"),
            concat!("std::", "process"),
            "symlink",
            "set_permissions",
            "write_all",
            "create_dir",
            "remove_dir",
            ".write(",
        ] {
            assert!(!code.contains(bad), "只读模块里不该出现 {bad:?}");
        }
    }
}
