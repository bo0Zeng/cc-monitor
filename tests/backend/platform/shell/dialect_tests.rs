//! 〔AL1c · 第四波 4B〕`shell_dialect.rs`（〔MIG-3a〕后端 `assets/aliases/dialect.rs`；〔OSA · V156〕今天是 `platform/shell/dialect.rs`）的判据：`设计/71 §4.4` 那组接口在两种方言上逐项的**读法**。
//!
//! 规则（合不合格）不在这里判 —— 那一份住 `account_aliases`，判据在 `account_aliases_tests.rs`。
//! 🔴 PowerShell 那一臂**一次都没被 PowerShell 解析过**（本机无 `pwsh`，Win11 虚拟机不许碰）：
//! 这里只能钉黄金串与「与 POSIX 臂同契约」的对拍。

use super::*;

/// 〔OSA〕通用层交给方言的那条调用形状（`ccm` · `--`）。
const C: Call = crate::assets::aliases::CALL;

/// 〔OSA〕通用层交给方言的「我们自己那块别名块」正文。
fn own(sh: Shell) -> String {
    crate::assets::aliases::block::own_block(sh)
}

fn sv(xs: &[&str]) -> Vec<String> {
    xs.iter().map(|s| s.to_string()).collect()
}

/// 同一份清单（带空格、单引号、弯引号、中文、`--ccm-tmux=`、`--` 透传），两个方言各钉逐字一份。
fn sample() -> Vec<(&'static str, Vec<String>)> {
    vec![
        ("zcc", sv(&["--account", "z"])),
        (
            "convz",
            sv(&[
                "--ccm-tmux=w.1",
                "--account",
                "z",
                "--cwd",
                "/home/u/文档/c c",
            ]),
        ),
        ("mo", sv(&["--model", "it's", "--", "--verbose"])),
        ("curly", sv(&["--model", "a\u{2019}b"])),
        ("bare", vec![]),
    ]
}

/// 黄金串 · POSIX：与 AL1 那一版逐字一样（本路只搬了住址，一个字节都没改）。
#[test]
fn posix_golden() {
    let got: Vec<String> = sample()
        .iter()
        .map(|(n, a)| Posix.render_alias(C, n, a))
        .collect();
    assert_eq!(
        got,
        vec![
            r#"zcc() { ccm --account z "$@"; }"#.to_string(),
            r#"convz() { ccm --ccm-tmux=w.1 --account z --cwd '/home/u/文档/c c' "$@"; }"#
                .to_string(),
            r#"mo() { ccm --model 'it'\''s' "$@" -- --verbose; }"#.to_string(),
            "curly() { ccm --model 'a\u{2019}b' \"$@\"; }".to_string(),
            r#"bare() { ccm "$@"; }"#.to_string(),
        ]
    );
    assert_eq!(
        Posix.source_line("/h/.cc-monitor/aliases.sh"),
        r#"if [ -r "/h/.cc-monitor/aliases.sh" ]; then . "/h/.cc-monitor/aliases.sh"; fi"#
    );
}

/// 黄金串 · PowerShell：函数体与 `src/shared/cc.ps1.tpl` 里的 `function cc` 逐字同形（那一形在 `K-R132` 真机上
/// `parse-errors=0`），预置参数**每个都单引号**、四种引号字符都双写。
#[test]
fn powershell_golden() {
    let got: Vec<String> = sample()
        .iter()
        .map(|(n, a)| PowerShell.render_alias(C, n, a))
        .collect();
    let body = |call: &str, name: &str| {
        format!(
            "function {name} {{\n    [CmdletBinding()] param(\n        [Parameter(ValueFromRemainingArguments = $true)] $RemainingArgs\n    )\n    if (Get-Command __ccm_bind -CommandType Function -ErrorAction SilentlyContinue) {{ __ccm_bind }}\n    & ccm{call} $RemainingArgs\n}}"
        )
    };
    assert!(
        got[2].ends_with("    & ccm '--model' 'it''s' $RemainingArgs '--' '--verbose'\n}"),
        "{}",
        got[2]
    );
    assert_eq!(
        got,
        vec![
            body(" '--account' 'z'", "zcc"),
            body(
                " '--ccm-tmux=w.1' '--account' 'z' '--cwd' '/home/u/文档/c c'",
                "convz"
            ),
            // 〔V151〕`$RemainingArgs`（敲别名时跟的）交 claude，别名的 ccm 选项在单引号的 `'--'` 右边。
            got[2].clone(),
            body(" '--model' 'a\u{2019}\u{2019}b'", "curly"),
            body("", "bare"),
        ]
    );
    assert_eq!(
        PowerShell.source_line(r"C:\Users\u\.cc-monitor/aliases.ps1"),
        r"if (Test-Path -LiteralPath 'C:\Users\u\.cc-monitor/aliases.ps1') { . 'C:\Users\u\.cc-monitor/aliases.ps1' }"
    );
    // 与自带 `cc` 同形这一句**不是**抄来的：从那份模板现渲染一个 `cc`，逐行比骨架。
    let cc = crate::assets::aliases::block::render_cc_code("cc", true, std::path::Path::new("/_"));
    for fixed in [
        "    [CmdletBinding()] param(",
        "        [Parameter(ValueFromRemainingArguments = $true)] $RemainingArgs",
        "    )",
    ] {
        assert!(
            cc.lines().any(|l| l == fixed),
            "别名函数的骨架与模板里的 `cc` 不再同形了：模板里没有 `{fixed}`"
        );
    }
    assert!(
        cc.lines().any(|l| l == "    & ccm $RemainingArgs"),
        "模板里 `cc` 的调用那一行变了 —— 别名那一行的尾巴得跟着看"
    );
}

/// 🔴 **与 POSIX 臂同契约的对拍**（PowerShell 真执行买不到时能买到的最强那一格）：
/// 同一份清单，两个方言各自渲染 → 各自读回 → **两边都等于原清单**（两向：读回的不多一条、不少一条）。
#[test]
fn both_dialects_read_back_exactly_what_they_wrote() {
    for (sh, d) in [Shell::Posix, Shell::PowerShell].map(|x| (x, x.dialect())) {
        let text: String = sample()
            .iter()
            .map(|(n, a)| d.render_alias(C, n, a) + "\n")
            .collect();
        let back: Vec<(String, Vec<String>)> = d
            .parse_file(C, &text)
            .into_iter()
            .map(|r| r.unwrap_or_else(|e| panic!("{:?} 读不回自己写的：{e}", sh)))
            .collect();
        let want: Vec<(String, Vec<String>)> = sample()
            .into_iter()
            .map(|(n, a)| (n.to_string(), a))
            .collect();
        assert_eq!(back, want, "{:?}", sh);
    }
}

/// PowerShell 读回只认**生成的那一形**：函数体被手改过 ⇒ 整块算认不出（带原因），不猜。
/// 块外的非注释行、没收尾的函数也都说出来，不静默丢。
#[test]
fn powershell_reader_names_what_it_cannot_take() {
    let good = PowerShell.render_alias(C, "zcc", &sv(&["--account", "z"]));
    let edited = good.replace("__ccm_bind }", "__ccm_bind; Write-Host hi }");
    let text = format!(
        "\u{feff}# 注释\nSet-Alias x ls\n{good}\n{}\nfunction open {{\n",
        edited.replace("zcc", "zcd")
    );
    let got = PowerShell.parse_file(C, PowerShell.decode_from_disk(&text));
    assert_eq!(got.len(), 4, "{got:?}");
    assert_eq!(got[1], Ok(("zcc".to_string(), sv(&["--account", "z"]))));
    assert!(
        matches!(&got[0], Err(e) if e.starts_with("Set-Alias x ls（")),
        "{got:?}"
    );
    assert!(
        matches!(&got[2], Err(e) if e.contains("zcd") && e.contains("手改")),
        "{got:?}"
    );
    assert!(matches!(&got[3], Err(e) if e.contains("收尾")), "{got:?}");
}

/// 方言只看目标文件的扩展名（`.ps1` 大小写都认），不看宿主平台。
#[test]
fn the_dialect_comes_from_the_target_file() {
    for ps in [
        "/h/Documents/WindowsPowerShell/Microsoft.PowerShell_profile.ps1",
        "/h/x.PS1",
    ] {
        assert_eq!(Shell::of_target(Path::new(ps)), Shell::PowerShell, "{ps}");
    }
    for rc in ["/h/.bashrc", "/h/.zshrc", "/h/.profile", "/h/x.ps1.bak"] {
        assert_eq!(Shell::of_target(Path::new(rc)), Shell::Posix, "{rc}");
    }
}

/// BOM：PowerShell 写时加、读时剥；POSIX 两头都原样（用户 rc 开头那三个字节不是我们的）。
#[test]
fn only_powershell_gets_a_bom() {
    let ps = PowerShell.encode_for_disk("# x\n");
    assert!(ps.starts_with('\u{feff}') && ps.matches('\u{feff}').count() == 1);
    assert_eq!(PowerShell.decode_from_disk(&ps), "# x\n");
    assert_eq!(Posix.encode_for_disk("# x\n"), "# x\n");
    assert_eq!(Posix.decode_from_disk("\u{feff}# x\n"), "\u{feff}# x\n");
}

/// 名字：两种方言同一个字符集；PowerShell 大小写不敏感（`Zcc` 与 `zcc` 是同一个函数）。
#[test]
fn names_are_portable_and_powershell_folds_case() {
    for (sh, d) in [Shell::Posix, Shell::PowerShell].map(|x| (x, x.dialect())) {
        for ok in ["zcc", "_x", "a1_b"] {
            assert!(d.name_is_valid(ok), "{:?} {ok}", sh);
        }
        for bad in ["", "1a", "a-b", "a.b", "a b", "名字"] {
            assert!(!d.name_is_valid(bad), "{:?} {bad}", sh);
        }
    }
    assert!(PowerShell.same_name("Zcc", "zcc"));
    assert!(!Posix.same_name("Zcc", "zcc"));
}

/// 值能不能原样到达 ccm：POSIX 恒能；PowerShell 拒空串、拒 `"`、拒「含空白且以 `\` 结尾」。
#[test]
fn powershell_refuses_values_it_would_mangle() {
    for w in ["", "a\"b", "C:\\a b\\"] {
        assert!(PowerShell.arg_is_passable(w).is_err(), "{w:?}");
        assert!(Posix.arg_is_passable(w).is_ok(), "{w:?}");
    }
    for w in ["C:\\a\\", "it's", "a b", "--x=a.b"] {
        assert!(PowerShell.arg_is_passable(w).is_ok(), "{w:?}");
    }
}

/// 「接上了我们那份」：POSIX 认 `$HOME/…` 没展开那一形；PowerShell 两种分隔符、大小写都认。
#[test]
fn a_startup_file_that_already_sources_us_is_recognized() {
    assert!(Posix.sources_our_file(
        "if [ -r \"$HOME/.cc-monitor/aliases.sh\" ]; then . \"$HOME/.cc-monitor/aliases.sh\"; fi"
    ));
    assert!(!Posix.sources_our_file(". ~/.cc-monitor/account-aliases.sh"));
    assert!(PowerShell.sources_our_file(r". 'C:\Users\U\.CC-MONITOR\Aliases.ps1'"));
    assert!(PowerShell.sources_our_file(&PowerShell.source_line("/h/.cc-monitor/aliases.ps1")));
    assert!(!PowerShell.sources_our_file(&Posix.source_line("/h/.cc-monitor/aliases.sh")));
}

/// 启动文件候选：POSIX 只列在的；PowerShell 的 5.1 两份恒列（不在也列），7 的两份只在它的目录在时列。
///
/// 〔AL2 · 第四波 4D〕方言**只给路径与列法**、一个字节的盘都不读（在不在由那台后端答 —— 那一半的判据是
/// `aliases_tests.rs::the_candidates_are_listed_by_each_dialects_rule_through_the_door`）。
/// ⇒ 这里拿一个**盘上不存在的 home 字符串**也判得完：读了盘就量不出这张表。
#[test]
fn startup_files_follow_each_shells_own_convention() {
    let home = "/nonexistent-ccm-dialect-home";
    assert!(
        !std::path::Path::new(home).exists(),
        "夹具前提：这个 home 不在盘上"
    );
    let posix: Vec<(String, Listed)> = Posix
        .startup_candidates(home)
        .into_iter()
        .map(|c| (c.path, c.listed))
        .collect();
    assert_eq!(
        posix,
        [".bashrc", ".zshrc", ".bash_profile", ".profile"]
            .iter()
            .map(|n| (format!("{home}/{n}"), Listed::IfFileExists))
            .collect::<Vec<_>>()
    );
    let docs = std::path::Path::new(home).join("Documents");
    let at = |d: &str, f: &str| docs.join(d).join(f).display().to_string();
    let ps7 = Listed::IfDirExists(docs.join("PowerShell").display().to_string());
    let ps: Vec<(String, Listed)> = PowerShell
        .startup_candidates(home)
        .into_iter()
        .map(|c| (c.path, c.listed))
        .collect();
    assert_eq!(
        ps,
        vec![
            (
                at("WindowsPowerShell", "Microsoft.PowerShell_profile.ps1"),
                Listed::Always
            ),
            (at("WindowsPowerShell", "profile.ps1"), Listed::Always),
            (
                at("PowerShell", "Microsoft.PowerShell_profile.ps1"),
                ps7.clone()
            ),
            (at("PowerShell", "profile.ps1"), ps7),
        ]
    );
    // 远端那一形：home 是那台答的 POSIX 字符串 ⇒ 拼出来全是 `/`（不许混进 monitor 这台的分隔符）。
    assert!(Posix
        .startup_candidates("/home/zbl")
        .iter()
        .all(|c| !c.path.contains('\\')));
}

/// 撞名（只出声）：PowerShell 认终端集成模板里的函数（从模板现算，大小写不敏感）。
#[test]
fn powershell_knows_the_names_its_own_block_defines() {
    for n in ["__ccm_bind", "CC"] {
        let note = PowerShell
            .name_taken(n, &own(Shell::PowerShell))
            .unwrap_or_else(|| panic!("`{n}` 在终端集成模板里就有，却一声不吭"));
        assert!(note.contains("终端集成块"), "{note}");
    }
    assert!(PowerShell
        .name_taken("zzz_no_such_command_anywhere", &own(Shell::PowerShell))
        .is_none());
    // 〔AL2〕不查 `PATH`（远端）时，模板里的名字照样认得出 —— 那一格不是本机才答得了的事实。
    assert!(PowerShell
        .name_taken("__ccm_bind", &own(Shell::PowerShell))
        .is_some());
}

// ═══════════════════════════════════════════════════════════════════════════
// 〔AL1d · 第四波 4B〕P1：`$PROFILE` 在哪，全仓只有一个住址（`调研/第四波记录/AL1d.md §2.3`）
// ═══════════════════════════════════════════════════════════════════════════

/// 「`$PROFILE` 在哪」的三个记号。**运行时拼**：本文件自己不许被自己数到（本文件不在 `src/` 下，这是第二道保险）。
fn profile_needles() -> [String; 3] {
    [
        format!("Microsoft.{}_profile.ps1", "PowerShell"),
        format!("{}.ps1", "profile"),
        format!("Windows{}", "PowerShell"),
    ]
}

/// 一段**生产代码**（注释已剥）里某个记号出现几处。`profile.ps1` 按词边界数 ——
/// 它是 `Microsoft.PowerShell_profile.ps1` 的后缀，裸 `matches` 会把一处数成两处。
fn needle_hits(code: &str, needle: &str) -> usize {
    let word = |c: char| c.is_ascii_alphanumeric() || c == '_';
    code.match_indices(needle)
        .filter(|(i, _)| !code[..*i].chars().next_back().is_some_and(word))
        .count()
}

/// 人群 = `src/` 全树的 `.rs` / `.ts`（生产段：Rust 剥测试模块与注释，TS 剥注释）。
/// 回：`(仓根相对路径, 记号) → 处数`，只收非零的。
fn profile_location_census(
    files: &[(String, String)],
) -> std::collections::BTreeMap<(String, String), usize> {
    let mut out = std::collections::BTreeMap::new();
    for (rel, src) in files {
        let code = if rel.ends_with(".rs") {
            guard_core::production_code(src)
        } else {
            guard_core::strip_comment_lines(src)
        };
        for n in profile_needles() {
            let k = needle_hits(&code, &n);
            if k > 0 {
                out.insert((rel.clone(), n), k);
            }
        }
    }
    out
}

/// 🔴 **P1**：`$PROFILE` 的文件名 / 目录名在全仓生产代码里**只住 `shell_dialect.rs`**，而且每个恰好一处。
///
/// 从前五处认法（终端集成两份发现表 · TS 换文件名推 AllHosts · 数据页探备份目录那张表 · 这里），
/// 其中一处把 `profile.ps1` 当「装错了的遗留」、这里把它当合法候选 —— 同一个事实三种说法（`AL1d.md §1.3`）。
///
/// 形状：`(文件, 记号) → 处数` 的**整张表相等**（两向：别处多出一处 ⇒ 红；这里那一处没了 / 挪走了 ⇒ 也红）。
/// ⚠ 买不到的：换一种写法认同一个位置（比如拼出 `"Windows" + "PowerShell"`、或问 PowerShell 自己 `$PROFILE`）
/// 这把尺子看不见 —— 它数的是字面记号，不是语义。
#[test]
fn the_profile_location_has_exactly_one_home() {
    // 表名 `SITES`：`scanning_guard_registry::TABLE_DECLS` 那条纪律（扫描面 ＋ 常量表型判据的表名闭集）。
    const SITES: &[(&str, usize, usize)] = &[
        // (住址, 记号在 `profile_needles()` 里的下标, 处数)
        ("src/backend/platform/shell/dialect.rs", 0, 1),
        ("src/backend/platform/shell/dialect.rs", 1, 1),
        ("src/backend/platform/shell/dialect.rs", 2, 1),
        // 〔OSA · 主会话 09-28 裁〕monitor「数据」区探 `$PROFILE` 备份那第二个读者删了：界面经通道问本机后端。
    ];
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let root = root.canonicalize().expect("仓根");
    let files: Vec<(String, String)> =
        guard_core::scan_tree_excluding(&root.join("src"), &["rs", "ts"], &[])
            .into_iter()
            .map(|(p, s)| {
                let rel = p
                    .strip_prefix(&root)
                    .unwrap_or(&p)
                    .to_string_lossy()
                    .replace('\\', "/");
                (rel, s)
            })
            .collect();
    // 抽取器自检：人群塌了 ⇒ 本条零命中地绿。
    assert!(
        files.len() > 500
            && files
                .iter()
                .any(|(r, _)| r == "src/frontend/shell/src/data_paths.rs"),
        "`src/` 只扫到 {} 份，或者没扫到 data_paths.rs —— 遍历坏了",
        files.len()
    );
    let needles = profile_needles();
    let want: std::collections::BTreeMap<(String, String), usize> = SITES
        .iter()
        .map(|(f, i, n)| ((f.to_string(), needles[*i].clone()), *n))
        .collect();
    assert_eq!(
        profile_location_census(&files),
        want,
        "`$PROFILE` 在哪（`Microsoft.PowerShell_profile.ps1` · `profile.ps1` · `WindowsPowerShell`）\n\
         今天该只有 `shell_dialect.rs` 的 PowerShell 那一臂（`startup_files`）答。多出来的那一处要么改问它，\n\
         要么说清为什么这是另一个事实 —— 别往 `SITES` 里加一行了事（那就是第二个住址）。"
    );
}

/// P1 的正控：同一把尺子在合成语料上**数得出**旧写法、**不数**注释、`profile.ps1` 不被 `…_profile.ps1` 重复数。
#[test]
fn the_profile_location_census_sees_the_old_shapes() {
    let n = profile_needles();
    let old_discover = format!(
        "fn discover() -> PathBuf {{\n    home.join(\"{}\").join(\"{}\")\n}}\n// 注释里的 {} 不算\n",
        n[2], n[0], n[1]
    );
    let old_ts = format!("const p = swap(x, \"{}\");\n/** {} */\n", n[1], n[2]);
    let got = profile_location_census(&[
        ("src/frontend/shell/src/data_paths.rs".into(), old_discover),
        ("src/frontend/ui/settings/x.ts".into(), old_ts),
    ]);
    let want: std::collections::BTreeMap<(String, String), usize> = [
        (
            (
                "src/frontend/shell/src/data_paths.rs".to_string(),
                n[0].clone(),
            ),
            1,
        ),
        (
            (
                "src/frontend/shell/src/data_paths.rs".to_string(),
                n[2].clone(),
            ),
            1,
        ),
        (
            ("src/frontend/ui/settings/x.ts".to_string(), n[1].clone()),
            1,
        ),
    ]
    .into_iter()
    .collect();
    assert_eq!(got, want);
}

/// ★ 〔FIX · `设计/71 §8` 第 8 条（逐字「PowerShell **内建别名**优先级高于函数（`ls` / `cd` 这类名字撞上了，定义了也敲不到）」）· WIN2 #4 读数〕
/// `Get-Alias` 那一段的输出读成表（名字不分大小写）；撞上 ⇒ 那句话带它指向谁；没撞 ⇒ 不说；问不到 ⇒ 说问不到（不当成没撞）。
/// ⚠ 买不到：真 PowerShell 那一跳（本机 Linux；读数见 `第四波记录/WIN2.md` #4）。
#[test]
fn a_powershell_builtin_alias_is_named_and_an_unknown_listing_is_said() {
    let listing = "ls\tGet-ChildItem\r\ncd\tSet-Location\r\n%\tForEach-Object\r\n\r\n";
    let table = parse_alias_listing(listing);
    assert_eq!(
        table
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect::<Vec<_>>(),
        vec![
            ("%", "ForEach-Object"),
            ("cd", "Set-Location"),
            ("ls", "Get-ChildItem")
        ]
    );
    let ok: PsAliases = Ok(table);
    let hit = builtin_alias_note("LS", &ok).expect("撞了内建别名却没说");
    assert!(hit.contains("LS") && hit.contains("Get-ChildItem"), "{hit}");
    assert_eq!(builtin_alias_note("zcc", &ok), None);
    let unknown: PsAliases = Err("exit Some(1)".into());
    let said = builtin_alias_note("zcc", &unknown).expect("问不到却当成没撞");
    assert!(said.contains("exit Some(1)"), "{said}");
}

/// PowerShell 5.1 单引号字面量的**最小词法模型**，照它词法器的规则手写（引号集与生产的 `PS_QUOTES` 异源）：
/// 任一引号字符开串；串里一个引号字符后面紧跟一个引号字符 ⇒ 收后面那个；否则串到此为止；之后必须什么都不剩。
const MODEL_PS_QUOTES: [char; 5] = ['\u{27}', '\u{2018}', '\u{2019}', '\u{201a}', '\u{201b}'];

fn model_read_ps_literal(lit: &str) -> Option<String> {
    let q = |c: char| MODEL_PS_QUOTES.contains(&c);
    let mut it = lit.chars().peekable();
    if !it.next().is_some_and(q) {
        return None;
    }
    let mut out = String::new();
    loop {
        let c = it.next()?;
        if !q(c) {
            out.push(c);
            continue;
        }
        match it.next_if(|&n| q(n)) {
            Some(n) => out.push(n),
            None => break,
        }
    }
    it.next().is_none().then_some(out)
}

/// ★ 住址：`设计/99 §2.3`「高危四条（M/N PowerShell 引号注入 …）发版前修」· `第四波记录/WIN3.md §2` M/N。
/// 唯一出口 [`ps_literal`] 渲出的串按模型读回 == 原串（值里的引号字符在哪、几个、混不混排都不许把串提前收尾）；
/// 模型认的引号集 == `PS_QUOTES`（两向）。输入：{字母 · 五个引号 · `$` · 反引号 · `"` · 空格} 上长度 ≤ 4 的全部串。
#[test]
fn every_powershell_literal_reads_back_as_exactly_its_value() {
    use std::collections::BTreeSet;
    assert_eq!(
        MODEL_PS_QUOTES.iter().copied().collect::<BTreeSet<char>>(),
        PS_QUOTES.iter().copied().collect::<BTreeSet<char>>(),
        "生产认的引号集与 PowerShell 词法器认的不是同一组"
    );
    let alphabet: Vec<char> = MODEL_PS_QUOTES
        .iter()
        .copied()
        .chain(['a', '$', '`', '"', ' '])
        .collect();
    let mut layer = vec![String::new()];
    let mut seen = 0usize;
    for _ in 0..=4 {
        for s in &layer {
            let lit = ps_literal(s);
            assert_eq!(
                model_read_ps_literal(&lit).as_deref(),
                Some(s.as_str()),
                "{s:?} 渲成 {lit:?}，PowerShell 读回来不是它"
            );
            seen += 1;
        }
        layer = layer
            .iter()
            .flat_map(|s| alphabet.iter().map(move |c| format!("{s}{c}")))
            .collect();
    }
    assert_eq!(seen, 11_111, "全排列没走全");
    // 模型自己有牙：旧写法（只双写 ASCII `'`）遇上 `’` 就读不回来。
    let old = format!("'{}'", "a\u{2019}b".replace('\'', "''"));
    assert_eq!(model_read_ps_literal(&old), None);
}

/// ★ 住址：`4d-lanes` P5（主会话裁：整段前奏由后端渲，PowerShell 字面量只走 `ps_literal`）· `第四波记录/WF1.md` 报备 ①（本机数据目录带弯引号时握手前奏会断）。
/// 开终端前奏（生产那一条 `dial/terminal.rs::with_bind_prelude`）里数据目录那一格按上面同一个模型读回 == `<数据目录>/ps-await`、
/// marker 那一格读回 == `ccm-rbind-token-<令牌>`：数据目录取 {字母 · 五个引号 · 空格 · `$`} 上长度 ≤ 3 的全部串（引号单个 · 连写 · 混排 · 打头）。
#[test]
fn the_terminal_prelude_reads_back_its_data_dir_whatever_quotes_it_holds() {
    const TOK: &str = "0f1e2d3c4b5a69788796a5b4c3d2e1f0";
    let assigned = |p: &str, name: &str| -> String {
        let head = format!("${name} = ");
        let line = p
            .lines()
            .map(str::trim)
            .find(|l| l.starts_with(&head))
            .unwrap_or_else(|| panic!("前奏里找不到 `{head}` 那一行：\n{p}"));
        line[head.len()..].to_string()
    };
    let alphabet: Vec<char> = MODEL_PS_QUOTES
        .iter()
        .copied()
        .chain(['a', ' ', '$'])
        .collect();
    let mut layer = vec![String::new()];
    let mut seen = 0usize;
    for _ in 0..=3 {
        for seg in &layer {
            let dir = std::path::PathBuf::from(seg).join(".cc-monitor");
            let p = crate::dial::terminal::with_bind_prelude(String::new(), Some(TOK), Some(&dir))
                .expect("合法令牌却渲不出前奏");
            let lit = assigned(&p, "d");
            assert_eq!(
                model_read_ps_literal(&lit),
                Some(dir.join("ps-await").to_string_lossy().into_owned()),
                "数据目录 {dir:?} 渲成 {lit:?}，PowerShell 读回来不是它"
            );
            assert_eq!(
                model_read_ps_literal(&assigned(&p, "m")).as_deref(),
                Some(format!("ccm-rbind-token-{TOK}").as_str())
            );
            seen += 1;
        }
        layer = layer
            .iter()
            .flat_map(|s| alphabet.iter().map(move |c| format!("{s}{c}")))
            .collect();
    }
    assert_eq!(seen, 585, "全排列没走全");
}
