//! **POSIX sh 那几句写法** —— `ccm`（直路 · 容器路）· 观测探针 · 几条一次性 exec，都只调这里。
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

/// 「在此打开终端」要在那台跑的那一串（窗口只交意图）：
/// `cd <已 quote 的目录> && exec ${SHELL:-bash} -l`；没有目录 ⇒ 只有登录 shell 那半段。目录合不合格、怎么 quote 由调用方先办
/// （`dial/terminal.rs::command_for_cwd`）。
pub(crate) fn cd_then_login_shell(quoted_dir: Option<&str>) -> String {
    match quoted_dir {
        Some(q) => format!("cd {} && exec ${{SHELL:-bash}} -l", q),
        None => "exec ${SHELL:-bash} -l".to_string(),
    }
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

/// 把**一行**（已成词）并进家目录底下 `dir/file`：目录 `700`、文件 `600`；已有整行相等的一行 ⇒ 打 `already`、不写
/// （`grep -qxF`）；否则文件非空且末字节不是换行先补一个、再追加 ⇒ 打 `added`（两个记号已成词）。整串是**一次** exec
/// （aterm 契约：`printf '%s\n'` 不用 echo · `--` 纵深防御 · 只收单行由调用方先判）。
pub(crate) fn add_line_once(
    dir: &str,
    file: &str,
    line: &str,
    added: &str,
    already: &str,
) -> String {
    format!(
        "k={line}; d=\"$HOME/{dir}\"; f=\"$d/{file}\"; mkdir -p \"$d\" && chmod 700 \"$d\" && touch \"$f\" && chmod 600 \"$f\" && \
         {{ grep -qxF -- \"$k\" \"$f\" && printf '%s\\n' {already} || \
         {{ {{ [ -s \"$f\" ] && [ -n \"$(tail -c1 \"$f\")\" ] && printf '\\n' >> \"$f\"; }}; printf '%s\\n' \"$k\" >> \"$f\" && printf '%s\\n' {added}; }}; }}"
    )
}

/// 一个词：`<head>` ＋ 家目录底下 `rel` 那份文件的内容（**现读**，钥匙不进 argv）＋ `<tail>`（head / tail 已成词）。
/// 不带双引号：这一段可能流进 Windows 那条开终端的路（PowerShell 5.1 向原生程序传参会改坏内嵌的 `"`）；`~/` 展开的结果不分词，
/// 文件内容是十六进制钥匙，不含空白与通配符。
pub(crate) fn home_file_between(head: &str, rel: &str, tail: &str) -> String {
    format!("{head}$(cat ~/{rel}){tail}")
}

/// 一个词：家目录底下 `rel` 那条路径（`"$HOME/<rel>"`，双引号里只展开 `$HOME`）。
pub(crate) fn home_path_word(rel: &str) -> String {
    format!("\"$HOME/{rel}\"")
}

/// `$HOME/x` / `${HOME}/x` / `~/x` ⇒ `Some("x")`（家目录底下那一截）；别的形 ⇒ `None`（不猜）。
pub(crate) fn home_relative(raw: &str) -> Option<&str> {
    ["$HOME/", "${HOME}/", "~/"]
        .into_iter()
        .find_map(|pat| raw.strip_prefix(pat))
}

/// 启动文件里 cc-monitor 围栏之外、确证失效的行：`source X` / `. X` 而 X 不在（`~` · `$HOME` · `${HOME}` 按这台家目录展开；
/// 带别的变量 / 相对路径的说不清，不报）。
pub(crate) fn dead_source_lines(
    text: &str,
    home: Option<&std::path::Path>,
) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    let mut inside = false;
    for (i, line) in text.lines().enumerate() {
        let l = line.trim_start();
        if l.starts_with("# === cc-monitor") {
            inside = l.contains("BEGIN");
            continue;
        }
        if inside || l.starts_with('#') {
            continue;
        }
        let rest = l.strip_prefix("source ").or_else(|| l.strip_prefix(". "));
        let Some(target) = rest.and_then(|r| r.split_whitespace().next()) else {
            continue;
        };
        let target = target.trim_matches(|c| c == '"' || c == '\'');
        let expanded: Option<std::path::PathBuf> = if let Some(r) = target
            .strip_prefix("~/")
            .or_else(|| target.strip_prefix("$HOME/"))
            .or_else(|| target.strip_prefix("${HOME}/"))
        {
            home.map(|h| h.join(r))
        } else if target.starts_with('/') && !target.contains('$') {
            Some(std::path::PathBuf::from(target))
        } else {
            None
        };
        if let Some(p) = expanded {
            if !p.exists() {
                out.push((i + 1, line.to_string()));
            }
        }
    }
    out
}
