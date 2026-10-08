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
/// 被「文件在才读」守着的不报：`[ -f X ]` / `[[ -r X ]]` / `test -f X`（`-f` `-r` `-e` `-s`）守着同一个 X ——
/// 同一行 `守 && source X`，或在 `if 守; then … fi` 里（一行写完的、`then` 另起一行的都认）。
pub(crate) fn dead_source_lines(
    text: &str,
    home: Option<&std::path::Path>,
) -> Vec<(usize, String)> {
    let key = |t: &str| {
        expand_home_target(t, home).map_or_else(|| t.to_string(), |p| p.display().to_string())
    };
    let mut out = Vec::new();
    let mut inside = false;
    // 开着的 `if` 各守着哪个文件（不是「文件在才读」的那种 `if` ⇒ `None`）。
    let mut guards: Vec<Option<String>> = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let l = line.trim();
        if l.starts_with("# === cc-monitor") {
            inside = l.contains("BEGIN");
            continue;
        }
        if inside || l.starts_with('#') {
            continue;
        }
        if l == "fi" || l.starts_with("fi;") || l.starts_with("fi ") {
            guards.pop();
            continue;
        }
        if l == "else" || l.starts_with("else ") {
            if let Some(top) = guards.last_mut() {
                *top = None;
            }
            continue;
        }
        if let Some(rest) = l.strip_prefix("elif ") {
            let g = split_then(rest).0;
            if let Some(top) = guards.last_mut() {
                *top = guard_target(g).map(key);
            }
            continue;
        }
        let mut local: Option<String> = None;
        let mut body = l;
        if let Some(rest) = l.strip_prefix("if ") {
            let (cond, after) = split_then(rest);
            let g = guard_target(cond).map(key);
            match after.map(str::trim) {
                Some(a) if a.ends_with("fi") && a[..a.len() - 2].trim_end().ends_with(';') => {
                    local = g;
                    body = a;
                }
                Some(a) => {
                    guards.push(g);
                    body = a;
                }
                None => {
                    guards.push(g);
                    continue;
                }
            }
        } else if let Some((head, tail)) = l.split_once("&&") {
            if let Some(g) = guard_target(head) {
                local = Some(key(g));
                body = tail;
            }
        }
        let Some(target) = source_target(body) else {
            continue;
        };
        let k = key(target);
        if local.as_deref() == Some(k.as_str()) || guards.iter().flatten().any(|g| *g == k) {
            continue;
        }
        if let Some(p) = expand_home_target(target, home).or_else(|| {
            (target.starts_with('/') && !target.contains('$'))
                .then(|| std::path::PathBuf::from(target))
        }) {
            if !p.exists() {
                out.push((i + 1, line.to_string()));
            }
        }
    }
    out
}

/// `~/x` · `$HOME/x` · `${HOME}/x` ⇒ 这台家目录底下那一条；别的形 / 没有家目录 ⇒ `None`。
fn expand_home_target(t: &str, home: Option<&std::path::Path>) -> Option<std::path::PathBuf> {
    let r = t
        .strip_prefix("~/")
        .or_else(|| t.strip_prefix("$HOME/"))
        .or_else(|| t.strip_prefix("${HOME}/"))?;
    home.map(|h| h.join(r))
}

/// `source X …` / `. X …` 的 X（去掉引号与收尾的 `;`）。
fn source_target(seg: &str) -> Option<&str> {
    let s = seg.trim_start();
    let rest = s.strip_prefix("source ").or_else(|| s.strip_prefix(". "))?;
    rest.split_whitespace().next().map(|t| {
        t.trim_end_matches(';')
            .trim_matches(|c| c == '"' || c == '\'')
    })
}

/// `if` 后面那一段 ⇒ `(条件, then 之后那一段)`；这一行没有 `then` ⇒ 第二格 `None`。
fn split_then(rest: &str) -> (&str, Option<&str>) {
    match rest
        .split_once("; then")
        .or_else(|| rest.split_once(";then"))
    {
        Some((c, a)) => (c, Some(a)),
        None => (rest, None),
    }
}

/// 「文件在才读」那种条件守着的文件：`[ -f X ]` · `[[ -r X ]]` · `test -e X`（`-f` `-r` `-e` `-s`）；别的条件 ⇒ `None`。
fn guard_target(cond: &str) -> Option<&str> {
    let c = cond.trim().trim_end_matches(';').trim_end();
    let (inner, close) = if let Some(r) = c.strip_prefix("[[ ") {
        (r, Some("]]"))
    } else if let Some(r) = c.strip_prefix("[ ") {
        (r, Some("]"))
    } else if let Some(r) = c.strip_prefix("test ") {
        (r, None)
    } else {
        return None;
    };
    let mut w = inner.split_whitespace();
    if !matches!(w.next()?, "-f" | "-r" | "-e" | "-s") {
        return None;
    }
    let t = w.next()?.trim_matches(|c| c == '"' || c == '\'');
    let closed = match close {
        Some(cl) => w.next() == Some(cl),
        None => true,
    };
    (closed && w.next().is_none()).then_some(t)
}
