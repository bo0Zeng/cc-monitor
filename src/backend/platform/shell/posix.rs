//! 〔OSA · `设计/99 §1` V156〕**POSIX sh 那几句写法** —— 起会话载荷 · 中转前缀 · `ccm` 直路 · 观测探针 ·
//! 读 cc-acct-iso 那份 sh 配置，都只调这里（原各自在 `control/launch_render/payload.rs` · `local.rs` · `control/ccm/{mod,plan}.rs` ·
//! `observe/{watcher,accounts_query}.rs` 里手写，逐字搬来，产出逐字节不变）。
//!
//! 只管「这句怎么写」：值合不合格、要不要引号由调用方先办（引号器是 `shell_quote_core::posix_quote`，
//! `INVARIANTS §2.1` 的唯一一份 —— 这里收的 `word` 一律是**已经成词**的那一串）。

/// `export VAR=<词>; `
pub(crate) fn export(var: &str, word: &str) -> String {
    format!("export {var}={word}; ")
}

/// `unset A B …; `
pub(crate) fn unset<S: AsRef<str>>(vars: &[S]) -> String {
    let names: Vec<&str> = vars.iter().map(AsRef::as_ref).collect();
    format!("unset {}; ", names.join(" "))
}

/// 把这个 shell 换成那条命令：`exec w1 w2 …`（词已成词）。一个词都没有 ⇒ 裸 `exec`（原 `ccm` 直路那一形）。
pub(crate) fn exec<S: AsRef<str>>(words: &[S]) -> String {
    let rest: Vec<&str> = words.iter().map(AsRef::as_ref).collect();
    let mut out = String::from("exec ");
    out.push_str(&rest.join(" "));
    if rest.is_empty() {
        out.pop();
    }
    out
}

/// `( <前一段>; exec <里面那条> )`：子 shell 里先跑一段、再把自己换成里面那条（起会话载荷外包的那一层）。
pub(crate) fn wrap_exec(prelude: &str, inner: &str) -> String {
    format!("( {prelude}; {} )", exec(&[inner]))
}

/// 「这台有没有这条命令」：`command -v <名> >/dev/null 2>&1`（退出码答）。
pub(crate) fn has_command(name: &str) -> String {
    format!("command -v {name} >/dev/null 2>&1")
}

/// 有那条命令 ⇒ 跑 `then`，否则跑 `otherwise`。
pub(crate) fn if_command(name: &str, then: &str, otherwise: &str) -> String {
    format!(
        "if {}; then {then}; else {otherwise}; fi",
        has_command(name)
    )
}

/// `VAR` 在这个 shell 里已经有值 ⇒ 说一句 `say`（已成词）、不动它；否则 `export VAR=<词>`。
pub(crate) fn export_unless_set(var: &str, word: &str, say: &str) -> String {
    format!(
        "[ -n \"${{{var}:-}}\" ] && printf '%s\\n' {say} || {}",
        export(var, word)
    )
}

/// 一个词：`<head>` ＋ 家目录底下 `rel` 那份文件的内容（**现读**，钥匙不进 argv）＋ `<tail>`（head / tail 已成词）。
pub(crate) fn home_file_between(head: &str, rel: &str, tail: &str) -> String {
    format!("{head}\"$(cat \"$HOME/{rel}\")\"{tail}")
}

/// 从一份会被 `.` source 的 sh 配置里抠 `VAR=` 的值 —— **纯文本解析，绝不 source**（后端不跑 shell）。
/// 取最后一次有效赋值（后写覆盖先写，与 shell 语义一致）；跳过注释行。
/// （原 `observe/accounts_query.rs::parse_accts_dir_from_config`，那里只认 `ACCTS_DIR` 一个名字。）
pub(crate) fn assigned_value(text: &str, var: &str) -> Option<String> {
    let mut found = None;
    for line in text.lines() {
        let mut l = line.trim_start();
        if l.starts_with('#') {
            continue;
        }
        // 被真正 `. source` 的文件里 `export VAR=…` / `declare -x VAR=…` 都是合法写法，且 export 是极常见习惯。
        // 逐个剥掉可选前缀，否则纯文本解析会漏认 → 回落默认 → 调用方那一格「静默判失效」。
        for pfx in [
            "export ",
            "declare -x ",
            "declare ",
            "typeset -x ",
            "typeset ",
        ] {
            if let Some(rest) = l.strip_prefix(pfx) {
                l = rest.trim_start();
                break;
            }
        }
        let Some(rest) = l.strip_prefix(var) else {
            continue;
        };
        // `=` 必须紧跟变量名（shell 赋值语义：`VAR =/x` 是命令不是赋值；
        // `VARX=…` 是别的变量）。不 trim `=` 前的空白，正好把这两种都排除。
        let Some(val) = rest.strip_prefix('=') else {
            continue;
        };
        let val = val.trim();
        // 去掉行尾注释（仅未被引号包裹时）
        let val = if val.starts_with('"') {
            val.strip_prefix('"').and_then(|v| v.split('"').next())
        } else if val.starts_with('\'') {
            val.strip_prefix('\'').and_then(|v| v.split('\'').next())
        } else {
            Some(val.split('#').next().unwrap_or("").trim())
        };
        if let Some(v) = val {
            if !v.is_empty() {
                found = Some(v.to_string());
            }
        }
    }
    found
}

/// 把 `$HOME/x` / `${HOME}/x` / `~/x` 前缀展开成 `home` 底下的路径。仅支持前缀形式 —— 更花哨的 shell 写法一律不猜。
/// （原 `observe/accounts_query.rs::expand_home_prefix` 有家目录那一臂。）
pub(crate) fn expand_home(raw: &str, home: &str) -> String {
    for pat in ["$HOME/", "${HOME}/", "~/"] {
        if let Some(rest) = raw.strip_prefix(pat) {
            return format!("{}/{}", home.trim_end_matches('/'), rest);
        }
    }
    match raw {
        "$HOME" | "${HOME}" | "~" => home.to_string(),
        _ => raw.to_string(),
    }
}
