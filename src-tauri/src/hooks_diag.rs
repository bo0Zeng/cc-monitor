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
}

impl HookState {
    /// 只有这两种算"能用"。`PathMissing` 刻意不算——它最容易被误报成已装。
    pub fn is_working(&self) -> bool {
        matches!(
            self,
            HookState::InstalledViaPath { .. } | HookState::InstalledAtPath { .. }
        )
    }
}

/// 整份诊断。`note` 装"为什么没读到"这类说明（文件缺失/坏 JSON），**不为空即应展示**。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
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
    // 去掉包裹引号（实测用户那条就是 `"$HOME/.local/bin/cc-register"`）
    let unq = tok.trim_matches(|c| c == '"' || c == '\'');
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
pub fn classify_command(cmd: &str, want: &str, exists: &dyn Fn(&str) -> bool) -> Option<HookState> {
    let (base, full) = program_of(cmd)?;
    if base != want {
        return None;
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
                fallback.get_or_insert(st); // 记下 PathMissing，继续找有没有能用的
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

/// 生成待贴的 JSON 片段。**两种形态**让用户挑：
///   · `home` = true：`$HOME/.local/bin/...` 显式路径——**默认推荐**，因为实测这台机器上
///     用的就是这个形态、被验证过能工作；
///   · `home` = false：裸命令，简洁但依赖 PATH。
/// 生成的是**待贴文本**，本模块绝不代写文件。
pub fn snippet(home: bool) -> String {
    let (reg, stop) = if home {
        (
            "\"$HOME/.local/bin/cc-register\" >/dev/null 2>&1 || true",
            "\"$HOME/.local/bin/cc-bus-stop-hook\"",
        )
    } else {
        ("cc-register >/dev/null 2>&1 || true", "cc-bus-stop-hook")
    };
    format!(
        "{{\n  \"hooks\": {{\n    \"SessionStart\": [ {{ \"hooks\": [ {{ \"type\": \"command\",\n      \"command\": \"{}\" }} ] }} ],\n    \"Stop\": [ {{ \"hooks\": [ {{ \"type\": \"command\",\n      \"command\": \"{}\" }} ] }} ]\n  }}\n}}",
        reg.replace('"', "\\\""),
        stop.replace('"', "\\\"")
    )
}

// ===== IPC 层：本机与远端各读一次 settings.json。**全程只读。** =====
//
// 远端形状照抄 `mcp.rs::fetch_remote_claude_json`：定值命令（零用户输入拼接 → 零注入面）、
// 30s 超时、大小上限、宽容解析。本机直接 `read_to_string`。
// **本模块没有任何写路径**——下方 `this_module_never_writes` 那条测试把它变成门禁，
// 而不是只靠我记得。

/// 一次诊断的完整回报（含用于展示的两种待贴片段）。
#[derive(Debug, Clone, serde::Serialize)]
pub struct HooksReport {
    pub diagnosis: HooksDiagnosis,
    /// `$HOME/.local/bin/...` 显式路径形态——**默认推荐**，因为实测这台机器上用的就是它。
    pub snippet_home: String,
    /// 裸命令形态，简洁但依赖 PATH。
    pub snippet_bare: String,
    /// 读到的原文路径（展示用，让用户知道诊断的是哪个文件）。
    pub source: String,
}

fn report(diagnosis: HooksDiagnosis, source: String) -> HooksReport {
    HooksReport {
        diagnosis,
        snippet_home: snippet(true),
        snippet_bare: snippet(false),
        source,
    }
}

/// 诊断**本机**的 `~/.claude/settings.json`。只读。
#[tauri::command]
pub async fn diagnose_local_cc_bus_hooks() -> Result<HooksReport, String> {
    tokio::task::spawn_blocking(|| {
        let Some(home) = dirs::home_dir() else {
            return report(diagnose(None, &|_| false), "（取不到 HOME）".to_string());
        };
        let p = home.join(".claude").join("settings.json");
        let raw = std::fs::read_to_string(&p).ok();
        // 路径存在性判定用真实文件系统；`$HOME` 前缀先展开再查，否则显式路径一律判成缺失。
        let home2 = home.clone();
        let exists = move |s: &str| -> bool {
            let expanded = if let Some(rest) = s.strip_prefix("$HOME/") {
                home2.join(rest)
            } else if let Some(rest) = s.strip_prefix("~/") {
                home2.join(rest)
            } else {
                std::path::PathBuf::from(s)
            };
            expanded.exists()
        };
        report(
            diagnose(raw.as_deref(), &exists),
            p.to_string_lossy().into_owned(),
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
    r#"for f in "$HOME/.local/bin/cc-register" "$HOME/.local/bin/cc-bus-stop-hook"; do "#,
    r#"[ -x "$f" ] && printf '%s\n' "$f"; done; "#,
    r#"command -v cc-register >/dev/null 2>&1 && echo HAS_PATH_REGISTER; "#,
    r#"command -v cc-bus-stop-hook >/dev/null 2>&1 && echo HAS_PATH_STOPHOOK; true"#
);

pub const HOOKS_SPLIT_MARKER: &str = "@@CCMON-HOOKS-SPLIT@@";

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
        let present: Vec<String> = probe_part.lines().map(|l| l.trim().to_string()).collect();
        let exists = move |s: &str| -> bool {
            // 远端不能 stat，靠上面那条命令探到的清单反查。`$HOME/x` 与远端回报的绝对路径
            // 对不上，故按 basename 匹配（清单里只会有那两个已知程序，不存在歧义）。
            let base = s.rsplit('/').next().unwrap_or(s);
            present
                .iter()
                .any(|p| p.rsplit('/').next().unwrap_or(p) == base)
        };
        let d = if json_part.trim().is_empty() {
            diagnose(None, &exists)
        } else {
            diagnose(Some(&json_part), &exists)
        };
        report(d, format!("[{origin}] ~/.claude/settings.json"))
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
    #[test]
    fn snippet_is_valid_json_and_round_trips() {
        for home in [true, false] {
            let s = snippet(home);
            let v: serde_json::Value = serde_json::from_str(&s).expect("生成的片段必须是合法 JSON");
            // 把自己生成的东西再喂给自己的诊断——闭环，防止生成一段自己都不认的文本
            let d = diagnose(Some(&s), &always);
            assert!(
                d.session_start.is_working(),
                "home={home} 生成的片段自己都不认: {s}"
            );
            assert!(d.stop.is_working(), "home={home}");
            assert!(v.get("hooks").is_some());
        }
        assert!(snippet(true).contains("$HOME/.local/bin/"));
        assert!(!snippet(false).contains("$HOME"));
    }

    #[test]
    fn this_module_never_writes() {
        // 结构性守卫：本模块不得出现任何写文件/起进程的调用。
        // 用户定调"绝不写入"，`cc-bus-install.sh` 第 3 行同样拒绝改 settings.json
        // ——把它变成门禁，而不是只靠我记得。
        //
        // **只扫非测试部分**：第一版扫整个文件，结果**扫到了自己**——下面这份禁用词清单
        // 本身就是测试源码里的字面量，守卫因此从一开始就必红。标记用拼接写，
        // 免得它在本文件里自匹配。
        let src = include_str!("hooks_diag.rs");
        let marker = concat!("#[cfg", "(test)]");
        let code = src.split(marker).next().unwrap_or(src);
        for bad in [
            concat!("fs::", "write"),
            concat!("File::", "create"),
            concat!("Open", "Options"),
            concat!("Command::", "new"),
            concat!("std::", "process"),
            concat!("remove_", "file"),
        ] {
            assert!(!code.contains(bad), "只读模块里不该出现 {bad:?}");
        }
        // 反向自检：守卫真的看到了代码（不是切成空串在空转）
        assert!(code.contains("pub fn diagnose"), "截断点错了，守卫扫了个空");
        assert!(code.len() > 1000, "扫到的代码太少，守卫形同虚设");
    }
}
