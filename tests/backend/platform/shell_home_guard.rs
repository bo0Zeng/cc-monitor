//! ——「生成alias这个东西是不是也应该后端搞? 这样就可以融入os适配层」⇒ shell 方言知识只住后端 OS 适配层
//! `platform/shell/`（判据原文：「PowerShell / fish 语法字面量在 `platform/shell/` 之外零命中（两向）」；加了 POSIX 那几样）。
//!
//! 人群：后端 `src/backend/` 生产段（`guard_core::production_code`：剥注释与测试段）里**字符串字面量的内容**
//! （转义与续行解开；字符字面量不算）。数的是 [`NEEDLES`] 那张方言专属语法字面量清单，`(文件, 记号) → 处数`：
//! ① `platform/shell/` 之外 == [`PROSE`]（两处散文里提到 `exec` 这个词，不产 shell；两向相等）；
//! ② `platform/shell/` 之内量得到的记号集 == 清单 − [`NOT_WRITTEN_HERE`]（两向：清单没有死行，适配层也没丢一格）。
//! 正控：合成语料上字面量抽取器认得转义 / 续行 / 原始串、不数注释与字符字面量；往一份消费者副本里塞一行渲染数得出。
//!
//! 买不到：换一种拼法造出同一句（`"ex" + "port "`）这把尺子看不见；monitor 那棵树（本机终端集成的 PowerShell 模板 ·
//! 开窗那一跳）不在人群里；tmux 容器配方（`if [ -n "${TMUX:-}" ]` · `2>/dev/null || true`）是 tmux 的知识，不在清单里；
//! 读 JSON / JSONL 时剥 BOM 用字符字面量，是通用解码、不在人群里（写盘加 BOM 那一形在）。

use std::collections::{BTreeMap, BTreeSet};

/// 方言专属语法字面量（解开转义后的样子）。每一根都是「只有那一种 shell 这么写」的那几个字。
const NEEDLES: &[&str] = &[
    // PowerShell（`$env:` · `$null` 两根摘了，理由同下面那三根）
    "[char]",
    // `Test-Path` 一根摘了：唯一用它的是「接上别名文件那一行」的渲染，那一行只住别名块模板里（给人贴的那一行随之删）。
    "Get-Alias",
    "Get-Command",
    // `Get-Content` · `Write-Host` · `Join-Path` 三根摘了：只有本机起会话那一行的 PowerShell 中转前缀用它们，
    //   那一层随起会话只交一行 `ccm …` 删了（中转地址由 `ccm` 在进程里定，不再渲 PowerShell）。
    "CmdletBinding",
    "RemainingArgs",
    "powershell.exe",
    "function ",
    "\u{feff}",
    // fish
    "set -gx",
    "set -e ",
    // POSIX sh
    "export ",
    "unset ",
    "exec ",
    "command -v",
    "\"$@\"",
    "$HOME",
    ".bashrc",
    ".zshrc",
    ".bash_profile",
];

/// 这台后端**不写**的方言：fish（`platform/shell/dialect.rs` 头注 —— fish `source` 不了 POSIX 函数，候选里不列它）。
/// 这两根只在「之外零命中」那一向上有牙。
const NOT_WRITTEN_HERE: &[&str] = &["set -gx", "set -e "];

/// `platform/shell/` 之外**允许**的命中：只有散文（串里讲 `exec` 这个词，不是一句要执行的 shell）。
const PROSE: &[(&str, &str, usize)] = &[
    // 能力表那一栏的说明文字（「要 shell 拆词：`set -f; exec $cmd`」）。
    ("lib.rs", "exec ", 1),
    // 判活那一格的说明（「exec 窗口，或进程已成僵尸」）。
    ("platform/proc.rs", "exec ", 1),
    // 足迹申报表里 `ccm` 远端那一份碰的 rc 文件（`~/.bashrc`，一条**申报路径**，给人看「动过哪份文件」，不产 shell）。
    ("footprint/registry.rs", ".bashrc", 1),
];

const HOME: &str = "platform/shell/";

/// 一段**已剥注释**的 Rust 源码里每个字符串字面量的内容（转义解开、续行接上；`r"…"` / `r#"…"#` 原样；字符字面量跳过）。
fn string_literals(code: &str) -> Vec<String> {
    let cs: Vec<char> = code.chars().collect();
    let ident = |c: char| c.is_alphanumeric() || c == '_';
    let mut out = Vec::new();
    let mut i = 0usize;
    while i < cs.len() {
        let c = cs[i];
        // 原始串：`r"…"` / `r#"…"#` / `br"…"`（前一个字符不是标识符的一部分）
        if c == 'r'
            && (i == 0 || !ident(cs[i - 1]) || (cs[i - 1] == 'b' && (i < 2 || !ident(cs[i - 2]))))
        {
            let mut j = i + 1;
            while j < cs.len() && cs[j] == '#' {
                j += 1;
            }
            if cs.get(j) == Some(&'"') {
                let hashes = j - i - 1;
                let mut k = j + 1;
                let start = k;
                loop {
                    assert!(k < cs.len(), "原始串没收尾 —— 抽取器坏了");
                    if cs[k] == '"'
                        && cs[k + 1..]
                            .iter()
                            .take(hashes)
                            .filter(|&&h| h == '#')
                            .count()
                            == hashes
                    {
                        break;
                    }
                    k += 1;
                }
                out.push(cs[start..k].iter().collect());
                i = k + 1 + hashes;
                continue;
            }
        }
        match c {
            '\'' => {
                // 字符字面量 `'x'` / `'\n'` / `'\u{…}'` 跳过；否则是生命周期标记，只跳这一个 `'`。
                if cs.get(i + 1) == Some(&'\\') {
                    let mut k = i + 2;
                    while k < cs.len() && cs[k] != '\'' {
                        k += 1;
                    }
                    i = k + 1;
                } else if cs.get(i + 2) == Some(&'\'') {
                    i += 3;
                } else {
                    i += 1;
                }
            }
            '"' => {
                let mut s = String::new();
                let mut k = i + 1;
                while k < cs.len() && cs[k] != '"' {
                    if cs[k] != '\\' {
                        s.push(cs[k]);
                        k += 1;
                        continue;
                    }
                    let e = cs[k + 1];
                    k += 2;
                    match e {
                        'n' => s.push('\n'),
                        't' => s.push('\t'),
                        'r' => s.push('\r'),
                        '0' => s.push('\0'),
                        '\n' | '\r' => {
                            while k < cs.len() && cs[k].is_whitespace() {
                                k += 1;
                            }
                        }
                        'u' => {
                            let close = (k..cs.len())
                                .find(|&x| cs[x] == '}')
                                .expect("\\u{…} 没收尾");
                            let hex: String = cs[k + 1..close].iter().collect();
                            s.push(
                                char::from_u32(
                                    u32::from_str_radix(&hex, 16).expect("\\u 不是十六进制"),
                                )
                                .expect("不是字符"),
                            );
                            k = close + 1;
                        }
                        'x' => {
                            let hex: String = cs[k..k + 2].iter().collect();
                            s.push(char::from(
                                u8::from_str_radix(&hex, 16).expect("\\x 不是十六进制"),
                            ));
                            k += 2;
                        }
                        other => s.push(other),
                    }
                }
                out.push(s);
                i = k + 1;
            }
            _ => i += 1,
        }
    }
    out
}

/// 一份 Rust 源码（原文）→ `记号 → 处数`（只收非零）。
fn census_of(src: &str) -> BTreeMap<&'static str, usize> {
    let lits = string_literals(&guard_core::production_code(src));
    NEEDLES
        .iter()
        .map(|n| (*n, lits.iter().map(|l| l.matches(n).count()).sum::<usize>()))
        .filter(|(_, k)| *k > 0)
        .collect()
}

/// 后端生产树：`(相对 src/backend 的路径, 记号) → 处数`。
fn census() -> BTreeMap<(String, &'static str), usize> {
    let root = crate::guard_support::src_root();
    let files = guard_core::scan_tree_excluding(&root, &["rs"], &[]);
    // 抽取器自检：人群里必须有适配层那几份与登记散文的那两份（遍历塌了 ⇒ 本条零命中地绿）。
    for must in [
        "platform/shell/mod.rs",
        "platform/shell/dialect.rs",
        "platform/shell/posix.rs",
        "platform/shell/powershell.rs",
        "lib.rs",
        "platform/proc.rs",
    ] {
        assert!(
            files.iter().any(|(p, _)| p
                .strip_prefix(&root)
                .is_ok_and(|r| r == std::path::Path::new(must))),
            "后端生产树里没扫到 `{must}`（共 {} 份）—— 遍历坏了",
            files.len()
        );
    }
    let mut out = BTreeMap::new();
    for (p, src) in files {
        let rel = p
            .strip_prefix(&root)
            .unwrap_or(&p)
            .to_string_lossy()
            .replace('\\', "/");
        for (n, k) in census_of(&src) {
            out.insert((rel.clone(), n), k);
        }
    }
    out
}

/// ★ 正题：方言专属语法字面量**只住 `platform/shell/`**（之外只剩登记的两处散文）；清单每一根在适配层里都量得到。
#[test]
fn shell_dialect_syntax_lives_only_in_the_platform_shell_layer() {
    let all = census();
    let outside: BTreeMap<(String, &str), usize> = all
        .iter()
        .filter(|((f, _), _)| !f.starts_with(HOME))
        .map(|(k, v)| (k.clone(), *v))
        .collect();
    let want: BTreeMap<(String, &str), usize> = PROSE
        .iter()
        .map(|(f, n, k)| ((f.to_string(), *n), *k))
        .collect();
    assert_eq!(
        outside, want,
        "shell 方言的写法长在了 `platform/shell/` 之外（方言知识只住后端 OS 适配层）。\n\
         ⇒ 那一句改调 `platform::shell::{{posix, powershell, dialect}}`；是散文不是 shell 的，写进 `PROSE` 并说清。"
    );
    let inside: BTreeSet<&str> = all
        .keys()
        .filter(|(f, _)| f.starts_with(HOME))
        .map(|(_, n)| *n)
        .collect();
    let expected: BTreeSet<&str> = NEEDLES
        .iter()
        .copied()
        .filter(|n| !NOT_WRITTEN_HERE.contains(n))
        .collect();
    assert_eq!(
        inside, expected,
        "清单与适配层对不上：多出来的是适配层写了清单没收的（清单该加一根），少了的是清单里有、适配层里一处都没有（死行，或那一格丢了）"
    );
}

/// 正控：抽取器认得转义 / 续行 / 原始串，不数注释与字符字面量；往副本里塞一行渲染数得出。
#[test]
fn the_literal_census_sees_planted_syntax_and_ignores_comments() {
    let lits = string_literals(
        "let a = \"x \\\"$@\\\"\"; let b = r#\"$env:A\"#; let c = '\"'; fn f<'a>() {}\n\
         let d = \"if command -v a; then \\\n           exec b; fi\"; let e = \"\\u{feff}\";",
    );
    assert_eq!(
        lits,
        vec![
            "x \"$@\"".to_string(),
            "$env:A".to_string(),
            "if command -v a; then exec b; fi".to_string(),
            "\u{feff}".to_string(),
        ]
    );
    let base = "fn f() -> String {\n    // export X=1; 注释里的不算\n    String::new()\n}\n";
    assert_eq!(census_of(base), BTreeMap::new());
    let planted = base.replace("String::new()", "format!(\"export X={}; \", 1)");
    assert_ne!(planted, base, "正控的锚没落在靶上");
    assert_eq!(census_of(&planted), BTreeMap::from([("export ", 1)]));
    let real = include_str!("../../../src/backend/control/launch_render/local.rs");
    let planted_real =
        format!("{real}\nfn planted() -> String {{ String::from(\"Get-Alias x; [char]9\") }}\n");
    assert_eq!(
        census_of(&planted_real),
        BTreeMap::from([("Get-Alias", 1), ("[char]", 1)]),
        "往真消费者的副本里塞一行 PowerShell 渲染，尺子该只数出塞进去的那两根"
    );
}

/// 「值直接接在单引号后」与「双写单引号的替换串」—— 手写 PowerShell 单引号字面量的两种形。
const SQ_NEEDLES: &[&str] = &["'{", "''"];

/// 唯一出口 `dialect::ps_literal` 之外允许的命中（都不是 PowerShell 字面量，逐条说清）。
const SQ_ELSEWHERE: &[(&str, &str, usize)] = &[
    // 别名表单那一格文本的写法（界面输入框里给人看、再由它自己切回去），不进任何 shell。
    ("assets/aliases/form.rs", "'{", 1),
    // 报错句里「起不来的是哪个程序」那个主语，不进 shell。
    ("control/ccm/mod.rs", "'{", 1),
    // POSIX：tmux 的 `-F` 格式串是常量。
    ("observe/tmux_observe.rs", "'{", 2),
];

fn sq_census_of(src: &str) -> BTreeMap<&'static str, usize> {
    let lits = string_literals(&guard_core::production_code(src));
    SQ_NEEDLES
        .iter()
        .map(|n| (*n, lits.iter().map(|l| l.matches(n).count()).sum::<usize>()))
        .filter(|(_, k)| *k > 0)
        .collect()
}

/// ★ 要求：「高危四条（M/N PowerShell 引号注入 …）发版前修」。
/// 后端生产段里 PowerShell 单引号字面量只有一个出口（`platform/shell/dialect.rs::ps_literal`）：
/// 两根手写形的命中 == [`SQ_ELSEWHERE`]（两向）；正控：往副本里塞回旧的只转 ASCII 那一形数得出。
#[test]
fn powershell_single_quoted_literals_have_one_exit() {
    let root = crate::guard_support::src_root();
    let mut got = BTreeMap::new();
    for (p, src) in guard_core::scan_tree_excluding(&root, &["rs"], &[]) {
        let rel = p
            .strip_prefix(&root)
            .unwrap_or(&p)
            .to_string_lossy()
            .replace('\\', "/");
        for (n, k) in sq_census_of(&src) {
            got.insert((rel.clone(), n), k);
        }
    }
    let want: BTreeMap<(String, &str), usize> = SQ_ELSEWHERE
        .iter()
        .map(|(f, n, k)| ((f.to_string(), *n), *k))
        .collect();
    assert_eq!(
        got, want,
        "有值绕过 `dialect::ps_literal` 自己进了单引号（PowerShell 还把 ‘ ’ ‚ ‛ 当引号）⇒ 改调它；\
         确实不是 PowerShell 字面量的，写进 `SQ_ELSEWHERE` 并说清"
    );
    let planted = "fn q(s: &str) -> String {\n    format!(\"'{}'\", s.replace('\\'', \"''\"))\n}\n";
    assert_eq!(
        sq_census_of(planted),
        BTreeMap::from([("'{", 1), ("''", 1)])
    );
}

/// 家目录那两个环境变量名（字面量**整串相等**才算，`"$HOME/…"` 这类 shell 文本不算）。
const HOME_VARS: &[&str] = &["HOME", "USERPROFILE"];

/// 后端生产段里允许的命中（逐条说清）。读法本身住共享契约 `creds_core::store::home_dir_on`，后端一处都不写。
const HOME_ELSEWHERE: &[(&str, &str, usize)] = &[
    // 交给插件子进程**透传**的变量名表（不是读家目录）。
    ("plugin/invoke.rs", "HOME", 1),
];

fn home_census_of(src: &str) -> (BTreeMap<&'static str, usize>, usize) {
    let prod = guard_core::production_code(src);
    let lits = string_literals(&prod);
    let vars = HOME_VARS
        .iter()
        .map(|v| (*v, lits.iter().filter(|l| l.as_str() == *v).count()))
        .filter(|(_, k)| *k > 0)
        .collect();
    (vars, prod.matches("env::home_dir").count())
}

/// ★ 要求：「高危四条 … 与中低各条发版前修」（后端多处只读 `HOME`、另有多处各自手写 `HOME.or(USERPROFILE)`）。
/// 读家目录的规矩只在共享契约 `creds_core::store::home_dir_on` 一处（后端 `platform::paths::home_dir` 转调它）：
/// 后端生产段 `"HOME"` / `"USERPROFILE"` 字面量的 `(文件, 名字) → 处数` == [`HOME_ELSEWHERE`]（两向）· 契约那份各恰好 1 处；
/// `std::env::home_dir` 的调用零处（`dial/ssh_config.rs` 那最后一处改调了 `platform::paths::home_dir`）。
/// 正控：往副本里塞回一处手写读数 · 一处 `std::env::home_dir` 都数得出。
#[test]
fn the_home_directory_is_read_in_one_place() {
    let root = crate::guard_support::src_root();
    let mut vars = BTreeMap::new();
    let mut std_calls = BTreeMap::new();
    for (p, src) in guard_core::scan_tree_excluding(&root, &["rs"], &[]) {
        let rel = p
            .strip_prefix(&root)
            .unwrap_or(&p)
            .to_string_lossy()
            .replace('\\', "/");
        let (v, n) = home_census_of(&src);
        for (name, k) in v {
            vars.insert((rel.clone(), name), k);
        }
        if n > 0 {
            std_calls.insert(rel, n);
        }
    }
    let want: BTreeMap<(String, &str), usize> = HOME_ELSEWHERE
        .iter()
        .map(|(f, v, k)| ((f.to_string(), *v), *k))
        .collect();
    assert_eq!(
        vars, want,
        "家目录又有人自己读了（哪个变量算家是两侧的契约）⇒ 改调 `platform::paths::home_dir` / `home_dir_from`；\
         不是读家目录的（透传表之类），写进 `HOME_ELSEWHERE` 并说清"
    );
    let contract = home_census_of(include_str!("../../../src/common/creds-core/src/store.rs"));
    assert_eq!(
        contract,
        (BTreeMap::from([("HOME", 1), ("USERPROFILE", 1)]), 0),
        "读家目录的那一处（`creds_core::store`）量不出那两个变量名 —— 规矩搬走了或尺子瞎了"
    );
    assert_eq!(
        std_calls,
        BTreeMap::new(),
        "有人又调了 `std::env::home_dir`（Linux 上它还会退到 passwd，与 `platform::paths::home_dir` 答的不是同一个家）⇒ 改调后者"
    );
    let planted = "fn h() -> Option<std::ffi::OsString> {\n    std::env::var_os(\"HOME\")\n}\n";
    assert_eq!(home_census_of(planted), (BTreeMap::from([("HOME", 1)]), 0));
    let planted_std = "fn h() -> Option<std::path::PathBuf> {\n    std::env::home_dir()\n}\n";
    assert_eq!(home_census_of(planted_std), (BTreeMap::new(), 1));
}
