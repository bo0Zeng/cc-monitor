use super::*;
use std::path::PathBuf;

const P: Shell = Shell::Posix;

/// 判据用的门：本进程那几条 `files-*` 原样走（[`crate::stream::inbound::LocalFiles`]），只有 `files-home` 答临时目录 ——
/// 写的规则不在这里判（那一份住后端 `files_write_tests.rs`），也**绝不碰真实家目录**。
pub(crate) struct HomeDoor(pub PathBuf);
impl Door for HomeDoor {
    fn ask(&self, cmd: &str, args: Value) -> Result<Value, (String, String)> {
        if cmd == "files-home" {
            return Ok(json!({ "path": self.0.display().to_string() }));
        }
        crate::stream::inbound::LocalFiles.ask(cmd, args)
    }
}

/// 读回口的薄包装：落在临时 home 上的门。
fn read_in(home: &Path, shell: Shell, extra: Option<&str>) -> Result<AliasListing, String> {
    read_via(&HomeDoor(home.to_path_buf()), shell, extra)
}

/// 候选表那一跳同一个门。
fn rc_candidates_in(home: &Path, shell: Shell, extra: Option<&Path>) -> Vec<StartupFile> {
    let extra = extra.map(|p| p.display().to_string());
    rc_candidates_via(
        &HomeDoor(home.to_path_buf()),
        &home.display().to_string(),
        shell,
        extra.as_deref(),
        &[],
    )
    .expect("门读候选")
}

/// 别名文件在临时 home 下的路径（`alias_file_in` 收 / 回字符串：那台机器的写法）。
fn profiles_path(home: &Path) -> PathBuf {
    PathBuf::from(profile::path_in(&home.display().to_string()))
}

fn alias_path(home: &Path, shell: Shell) -> PathBuf {
    PathBuf::from(alias_file_in(&home.display().to_string(), shell))
}

/// 存一份清单（每条一段、清单里没了的那一段删掉；「基于」照盘上那一段留着），再照它生成别名文件与链接 —— 与设置窗存的那一步
/// 同一组函数（`profile::apply_changes` · `write_profiles` · `sync_outputs`）。回：动没动；整份合不下来 ⇒ 那几句。
fn inst(h: &TmpHome, list: &[Alias], shell: Shell) -> Result<bool, String> {
    let d = door(h);
    let store = load(&d)?;
    let mut changes: Vec<profile::Change> = store
        .book
        .profiles
        .iter()
        .filter(|p| !list.iter().any(|a| a.name == p.name))
        .map(|p| profile::Change::Remove(p.name.clone()))
        .collect();
    for a in list {
        let from = store.book.find(&a.name).and_then(|p| p.from.clone());
        changes.push(profile::Change::Set(edit_of_alias(a, from)));
    }
    let text = profile::apply_changes(store.text.as_deref(), &changes)?;
    let bad = profile::problems_after(&text);
    if !bad.is_empty() {
        return Err(bad.join("\n"));
    }
    let wrote =
        write_profiles(&d, &store.home, store.text.as_deref(), &text).map_err(|e| e.said())?;
    let out = sync_outputs(&d, &store.home, &profile::parse_book(&text), Some(shell));
    Ok(wrote || out.links_changed || !out.rewrote.is_empty())
}

/// 盘上配置文件里那几段的名字（不在 ⇒ 空）。
fn names_on_disk(h: &TmpHome) -> Vec<String> {
    std::fs::read_to_string(profiles_path(&h.0))
        .map(|t| {
            profile::parse_book(&t)
                .profiles
                .into_iter()
                .map(|p| p.name)
                .collect()
        })
        .unwrap_or_default()
}

/// 「这份文本里**有整整一行**逐字等于它」。
///
/// ⚠ 刻意不是 `hay.contains(needle)`：那种比法的**匹配单位（子串）比事实（一整行）小**
/// —— 别名那一行被截断、或被加了个没人要的修饰，子串比法照样绿。
/// `needle_anchor_registry` 里那条递减棘轮数的正是这一族，本模块一处都不往上加。
fn pinned(hay: &str, needle: &str) -> bool {
    hay.lines().any(|l| l == needle)
}

/// panic 也要清干净（`profile_installer` 那条纪律的同一份）。
struct TmpHome(PathBuf);
impl Drop for TmpHome {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
/// 落盘经「门」（生产 = 本机后端的文件管理那一面）；判据用落在临时目录上的替身门。
/// 写的规则（备份 · 原子替换 · 回读 · 回滚）不在这里判 —— 那一份住后端（`files_write_tests.rs`）。
fn door(h: &TmpHome) -> HomeDoor {
    HomeDoor(h.0.clone())
}
fn hs(h: &TmpHome) -> String {
    h.0.display().to_string()
}
/// 后端这一族是同步的（门就是本进程的 `files-*`）；留一个恒等包装，判据正文不必逐处改写。
fn run<T>(x: T) -> T {
    x
}
fn tmp_home(tag: &str) -> TmpHome {
    let d = std::env::temp_dir().join(format!(
        "ccm-alias-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|x| x.as_nanos())
            .unwrap_or(0)
    ));
    std::fs::create_dir_all(&d).expect("建 tempdir");
    // 链接指向的那个 ccm（生产上就是这台的后端本身）：不在的话链接是断的，认不出是我们放的。
    std::fs::create_dir_all(d.join(links::bin_rel())).expect("建 bin");
    std::fs::write(d.join(links::bin_rel()).join("ccm"), "").expect("放 ccm");
    TmpHome(d)
}

// 这里原来是「形状围栏」那一条（`validate_alias_line`〔散文墓碑〕只放行
// `名字() { ccm <已知修饰...> "$@"; }` 一形）与四条 `apply(dry_run)` 的判据。前端不再递 shell 文本
// （递的是结构，文本由 `render` 出），围栏没有输入了 ⇒ 挡注入那件事换成下面
// `injection_attempts_arrive_as_plain_args_in_bash`：**让真 bash 执行渲染出来的那一行**，
// 看那几个危险值是不是原样、作为一个参数到了 `ccm` 手里。「预览不写」那一条也不需要了 ——
// `render` 连 home 都不收，结构上够不到任何文件。

/// ★★ 注入：值里的 `;` / `$( )` / 反引号 / `|` / 引号，进了配置文件、再经 ccm 合下来，**只是一个参数**、逐字不变；
/// 别名文件里那一行只有名字（`名字() { ccm @名字 "$@"; }`），真 bash 执行下来调用时跟的词原样接在 `@名字` 后面。
#[test]
fn injection_attempts_arrive_as_plain_args_in_bash() {
    let evil = [
        "z; rm -rf ~",
        "$(id -u)",
        "`id -u`",
        "a | sh",
        "a > ~/.bashrc",
        "it's",
        "\"$@\"",
        "x\"\"\" = 1\n[y]",
    ];
    let h = tmp_home("evil");
    for v in evil {
        let a = al("evil", &["--model", v]);
        inst(&h, &[a], P).expect("写");
        let text = std::fs::read_to_string(profiles_path(&h.0)).unwrap();
        let book = profile::parse_book(&text);
        assert!(
            book.problems.is_empty(),
            "{v:?}：{:?}\n{text}",
            book.problems
        );
        let r = profile::resolve(&book, "evil", &[]).expect("合得下来");
        let crate::control::ccm::argv::Parsed::Opts(o) = r.parsed else {
            panic!()
        };
        assert_eq!(
            o.passthru,
            ["--model".to_string(), v.to_string()],
            "{v:?} 没有原样到达"
        );
    }
    let script = format!(
        "ccm() {{ printf '%s\\n' \"$@\"; }}\n{}\nevil 'a b' \"it's\" '$(id)'\n",
        render_line("evil", P)
    );
    let out = std::process::Command::new("bash")
        .arg("-c")
        .arg(&script)
        .output()
        .expect("起 bash");
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        "@evil\na b\nit's\n$(id)\n"
    );
}

/// 链接所在目录里那一条（指向的文本；不是链接 ⇒ `None`）。
fn link_of(h: &TmpHome, name: &str) -> Option<String> {
    std::fs::read_link(h.0.join(links::bin_rel()).join(name))
        .ok()
        .map(|p| p.display().to_string())
}

/// ★★ 清单里删了一条，它那一段与它的链接当场没了；留下的那条照旧；别的文件一个不碰。
#[test]
fn rewriting_drops_the_alias_you_deleted() {
    let h = tmp_home("regen");
    let two = vec![
        al("workcc", &["--account", "work"]),
        al("teamcc", &["--account", "team"]),
    ];
    assert!(inst(&h, &two, P).unwrap());
    assert_eq!(link_of(&h, "workcc").as_deref(), Some("ccm"));
    assert_eq!(link_of(&h, "teamcc").as_deref(), Some("ccm"));
    let one = vec![al("workcc", &["--account", "work"])];
    inst(&h, &one, P).unwrap();
    assert_eq!(
        link_of(&h, "workcc").as_deref(),
        Some("ccm"),
        "留下来的那条没了"
    );
    assert_eq!(link_of(&h, "teamcc"), None, "★ 删了一条，它的链接还在");
    assert_eq!(names_on_disk(&h), ["workcc"], "配置文件里只剩留下的那一段");
    assert!(
        h.0.join(links::bin_rel()).join("ccm").is_file(),
        "ccm 本身不许动"
    );
}

/// 🔴 别名文件改名 `account-aliases.sh` → `aliases.sh`，**不留兼容**：盘上只有旧名那份时读回口不读它（当作没有、给首建那几条）。
#[test]
fn the_old_alias_file_name_is_not_read() {
    let h = tmp_home("rename");
    std::fs::create_dir_all(h.0.join(".cc-monitor")).unwrap();
    std::fs::write(
        h.0.join(".cc-monitor/account-aliases.sh"),
        "workcc() { ccm --account work \"$@\"; }\n",
    )
    .expect("铺旧文件");
    read_in(&h.0, P, None).expect("读回");
    assert!(
        !profiles_path(&h.0).exists(),
        "读回口去读了旧名（拿它迁移了）"
    );
}

/// 🔴 **装了别名块的人，候选表认得「已接上」**：别名块里那一行写的是 `$HOME/.cc-monitor/…`（没展开）。
#[test]
fn an_rc_that_sources_it_via_home_var_is_recognized() {
    let h = tmp_home("homevar");
    let rc = h.0.join(".bashrc");
    let line =
        "if [ -r \"$HOME/.cc-monitor/aliases.sh\" ]; then . \"$HOME/.cc-monitor/aliases.sh\"; fi\n";
    std::fs::write(&rc, format!("# mine\n{line}")).expect("写 rc");
    let me = rc_candidates_in(&h.0, P, None)
        .into_iter()
        .find(|c| c.path == rc.display().to_string())
        .expect("候选里该有 .bashrc");
    assert!(me.sourced, "候选表没认出 `$HOME` 那一形：{me:?}");
}

/// ★★**两种方言对称：别名块里恰好一行接上别名文件**（「source 那一行只许一处装」；
/// `AL1d.md §5` 第 4 条：从前 PowerShell 的别名块不接，那一侧只剩代装那一处）。
///
/// 人群：POSIX 别名块 = `block::CCM_WRAPPER_SNIPPET`（就是 `src/shared/ccm-aliases.sh`）；PowerShell 别名块 =
/// `block::render_block`。每一份里「那种方言认得出的接上行」恰好一行（零 = 不接，二 = 两处）。
/// 另一半：本模块生产段里写用户文件的口恰好一处（`door::edit(` —— 写别名文件那一处；代装 rc 那一处退役）。
#[test]
fn the_alias_block_is_the_one_place_that_sources_the_alias_file_in_both_dialects() {
    let count = |text: &str, sh: Shell| {
        text.lines()
            .filter(|l| !l.trim_start().starts_with('#'))
            .filter(|l| sh.dialect().sources_our_file(l))
            .count()
    };
    assert_eq!(count(block::CCM_WRAPPER_SNIPPET, P), 1, "POSIX 别名块");
    let block = block::render_block(Shell::PowerShell, "/h").expect("渲染");
    assert_eq!(
        count(&block, Shell::PowerShell),
        1,
        "PowerShell 别名块：{block}"
    );
    // 正控：认法对别人家的行不命中（不然上面的「恰好 1」可能是整份都算）。
    assert_eq!(count("export PATH=/x\nalias ll='ls -l'\n", P), 0);
    let prod = guard_core::production_code(include_str!(
        "../../../../src/backend/assets/aliases/mod.rs"
    ));
    guard_core::find_pinned(&prod, "door::edit(")
        .expect("本模块写用户文件的口不是恰好一处（只许写别名文件那一处）");
}

/// 🔴 别名块只管接入：POSIX 那一块一个函数都不定义（`ccm` 进 PATH ＋ 接上别名文件）；PowerShell 那一块只定义拉前握手
/// `__ccm_bind` 与带窗口标签的那层 `ssh`（＋ 接上别名文件）。`cc` / `cct` / `cca` 住清单（[`first_aliases`]）。
/// 名字被我们的块占着 ⇒ 撞名提示照出声（`__ccm_bind`）；块不再占 `cc`。
#[test]
fn the_alias_blocks_only_wire_up_ccm() {
    let fns = |sh: Shell, text: &str| -> Vec<String> {
        text.lines()
            .filter_map(|l| sh.dialect().declared_function(l.trim_start()))
            .collect()
    };
    assert_eq!(fns(P, block::CCM_WRAPPER_SNIPPET), Vec::<String>::new());
    let ps = block::render_block(PS, "/h").expect("渲染");
    assert_eq!(fns(PS, &ps), ["__ccm_bind", "ssh"]);
    // 正控：认法认得出函数。
    assert_eq!(fns(P, "cc() { ccm \"$@\"; }\n"), ["cc"]);
    let note = collision_note("__ccm_bind", PS).expect("我们的块占着 `__ccm_bind`，该出声");
    assert!(note.contains("__ccm_bind"), "{note}");
    assert!(collision_note("zzz_no_such_command_anywhere", P).is_none());
}

/// 生成文件里**没有时间戳** —— 有了就永远比不出「内容没变」。
#[test]
fn the_generated_file_is_byte_stable() {
    let lines = vec!["workcc() { ccm --account 'work' \"$@\"; }".to_string()];
    assert_eq!(render_file(P, &lines), render_file(P, &lines));
    assert!(render_file(P, &[]).contains(copy_core::copy_static!("rsAccountAliases.file.empty")));
}

// ═══════════════════════════════════════════════════════════════════════
// 一类别名 · 两跳（渲染纯 / 写入唯一副作用）· 读回口
// ═══════════════════════════════════════════════════════════════════════

/// 本文件的夹具把 ccm 选项与 claude 的词混写 ⇒ 这里换成 `<交给 claude 的…> -- <ccm 自己的…>` 排列，
/// 意图逐词不变。哪个词是 ccm 的、跟几个值，问 ccm 自己的解析器（缺值的照原样只放那一个词）。
fn al(name: &str, args: &[&str]) -> Alias {
    use crate::control::ccm::argv;
    let words: Vec<String> = args.iter().map(|s| s.to_string()).collect();
    let (mut left, mut right) = (Vec::new(), Vec::new());
    let mut i = 0;
    while i < words.len() {
        let w = &words[i];
        if w == "--" {
            left.extend(words[i + 1..].iter().cloned());
            break;
        }
        if !argv::is_ccm_word(w) {
            left.push(w.clone());
            i += 1;
            continue;
        }
        let n = argv::word_at(&words, i).map_or(1, |g| g.len);
        right.extend(words[i..i + n].iter().cloned());
        i += n;
    }
    if !right.is_empty() || left.iter().any(|w| w == "--") {
        left.push("--".into());
        left.extend(right);
    }
    Alias::new(name, left)
}

/// 🔴 **异源判据**：真 bash 执行别名文件里那一行 —— 调用时跟的词（空格 / 单引号 / 中文 / `--` 之后的 ccm 选项）原样接在 `@名字` 后面。
#[test]
fn a_rendered_alias_really_appends_the_callers_args_in_bash() {
    let line = render_line("cc", P);
    let script =
        format!("ccm() {{ printf '%s\\n' \"$@\"; }}\n{line}\ncc '你 好' \"it's\" -- --account x\n");
    let out = std::process::Command::new("bash")
        .arg("-c")
        .arg(&script)
        .output()
        .expect("起 bash");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        "@cc\n你 好\nit's\n--\n--account\nx\n"
    );
}

/// **一次性迁移**：配置文件还不在、旧形状的别名文件在 ⇒ 每条转成一段、写出配置文件、别名文件重写成新形状、补链接；
/// 认不出的行与转不进去的（诊断口 · 接回会话那一形）不进配置文件（记进日志）；配置文件在了就不再看旧文件。
#[test]
fn the_old_alias_file_is_migrated_once_into_the_profiles_file() {
    let h = tmp_home("old");
    let p = alias_path(&h.0, P);
    std::fs::write(
        &p,
        "# === cc-monitor account aliases BEGIN v1 ===\n\
         # 注释\n\
         workcc() { ccm \"$@\" -- --account 'work'; }\n\
         teamcct() { \"${CCM:-/h/.cc-monitor/bin/ccm}\" \"$@\" -- new --ccm-tmux --account 'team'; }\n\
         cca() { ccm -- --attach \"$@\"; }\n\
         alias x=ls\n\
         bad() { ccm \"$@\" -- --ccm-print; }\n\
         # === cc-monitor account aliases END ===\n",
    )
    .unwrap();
    read_in(&h.0, P, None).unwrap();
    let book = profile::parse_book(&std::fs::read_to_string(profiles_path(&h.0)).unwrap());
    assert!(book.problems.is_empty(), "{:?}", book.problems);
    let got: Vec<(String, Vec<String>)> = book
        .profiles
        .iter()
        .map(|p| (p.name.clone(), p.ccm_words()))
        .collect();
    let w = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
    assert_eq!(
        got,
        vec![
            ("workcc".to_string(), w(&["--account", "work"])),
            (
                "teamcct".to_string(),
                w(&["--ccm-tmux", "--account", "team"])
            ),
        ]
    );
    let new_file = std::fs::read_to_string(&p).unwrap();
    assert!(
        !new_file.contains("--account"),
        "别名文件里不再有规则：{new_file}"
    );
    assert_eq!(link_of(&h, "workcc").as_deref(), Some("ccm"));
    assert_eq!(link_of(&h, "teamcct").as_deref(), Some("ccm"));
    // 迁过了 ⇒ 旧文件再出现也不看。
    std::fs::write(&p, "altcc() { ccm \"$@\" -- --account alt; }\n").unwrap();
    read_in(&h.0, P, None).unwrap();
    assert!(!names_on_disk(&h).contains(&"altcc".to_string()));
    // 两样都不在 ≠ 读失败（配置文件不建）。
    let empty = tmp_home("none");
    read_in(&empty.0, P, None).unwrap();
    assert!(!profiles_path(&empty.0).exists());
}

/// 🔴 **异源**：别名里能放的每个旗标，都得是后端 `ccm --help` 里真有的那个词
/// （用法文本住后端 `control/ccm/mod.rs::USAGE`，两棵树不共享源码 ⇒ 读它的原文）。
/// 反向：用法里那几个「每次取值都不同」的（第三档）不许混进来。
#[test]
fn every_alias_flag_is_a_real_ccm_flag() {
    // `--help` 正文进了文案表（`beCcm.usage.body`）：后端 ccm 那份源码取的就是这一条，正文从表里读。
    let ccm_src = std::fs::read_to_string(
        crate::guard_support::repo_root().join("src/backend/control/ccm/mod.rs"),
    )
    .expect("读后端 ccm 源码");
    assert!(
        ccm_src.contains("\"beCcm.usage.body\""),
        "后端 ccm 的 `--help` 不再取文案表里那一条 —— 下面读的就不是它的用法了"
    );
    let usage_src = copy_core::copy_text("beCcm.usage.body", &[]);
    let from = guard_core::find_pinned(&usage_src, "\n  new ")
        .expect("用法里选项那一段（以 new 那一行起）锚不住");
    let usage = &usage_src[from..];
    for flag in profile::PROFILE_FLAGS {
        assert!(
            usage.contains(&format!("  {flag} ")) || usage.contains(&format!("  {flag}[")),
            "`{flag}` 不是后端 ccm 用法里的一个旗标 —— 生成出来就是一条当场报错的别名"
        );
    }
    for third in [
        "--resume",
        "--ccm-sid",
        "--print",
        "--ccm-probe",
        "--version",
        "--help",
    ] {
        assert!(
            !profile::PROFILE_FLAGS.contains(&third),
            "`{third}` 每次取值都不同，做成固定别名没意义（第三档）"
        );
    }
}

// ═══════════════════════════════════════════════════════════════════════
// / W4：通用层零 shell 文本 · 能力闸 · PowerShell 那一臂的写与读回
// ═══════════════════════════════════════════════════════════════════════

const PS: Shell = Shell::PowerShell;

/// 「shell 语法记号」—— 通用层的生产代码里一个都不许有（条 33：通用层持有结构，不持有任何一种 shell 的文本）。
/// 两种方言各取几样最有辨识度的：POSIX 的函数声明 / `"$@"` / `if [` / 引号器，PowerShell 的 `function ` / 余参 / `Test-Path`。
const SHELL_SYNTAX: &[&str] = &[
    "$@",
    "() { ",
    "if [",
    "; fi",
    "posix_quote",
    "function ",
    "RemainingArgs",
    "Test-Path",
    "CmdletBinding",
];

fn shell_syntax_hits(src: &str) -> Vec<String> {
    let code = guard_core::production_code(src);
    SHELL_SYNTAX
        .iter()
        .filter(|n| code.contains(**n))
        .map(|n| n.to_string())
        .collect()
}

/// 🔴 D1：`account_aliases.rs`（通用层）的生产代码里 shell 语法记号**零命中**；
/// 正控：同一把尺子量 `shell_dialect.rs` 必须两种方言都量得出来，往副本里塞一行 POSIX 渲染也必须数出来。
#[test]
fn the_generic_layer_holds_no_shell_text() {
    let generic = include_str!("../../../../src/backend/assets/aliases/mod.rs");
    assert_eq!(
        shell_syntax_hits(generic),
        Vec::<String>::new(),
        "通用层长出了某种 shell 的文本 —— 写法 / 读法归 `shell_dialect.rs` 那一族"
    );
    // 方言住后端 OS 适配层。
    let dialect = include_str!("../../../../src/backend/platform/shell/dialect.rs");
    let hits = shell_syntax_hits(dialect);
    for must in ["$@", "RemainingArgs", "CmdletBinding"] {
        assert!(
            hits.iter().any(|h| h == must),
            "尺子量不到方言模块里的 `{must}` —— 它瞎了：{hits:?}"
        );
    }
    let planted = generic.replacen(
        "pub(crate) fn render_line(",
        "fn planted() -> &'static str { \"x() { ccm \\\"$@\\\"; }\" }\npub(crate) fn render_line(",
        1,
    );
    assert_ne!(planted, generic, "正控的锚没落在靶上");
    assert!(
        !shell_syntax_hits(&planted).is_empty(),
        "塞进一行 POSIX 渲染，尺子没看见"
    );
}

/// 🔴 PowerShell 那一臂的写与读回：别名文件 `aliases.ps1` 带 BOM；读回的清单 == 写进去的；再写一次一个字节都不动；
/// POSIX 那份文件不受影响。往 `$PROFILE` 装上别名块之后，候选表答「已经接上」—— 与 POSIX 那一侧对称。
#[test]
fn the_powershell_arm_writes_and_reads_back_the_same_list() {
    let h = tmp_home("ps");
    let list = vec![
        al("workcc", &["--account", "work"]),
        al(
            "mine",
            &["--cwd", "C:\\x y", "--ccm-agent", "codex", "--", "--foo"],
        ),
    ];
    let profile =
        h.0.join("Documents/WindowsPowerShell/Microsoft.PowerShell_profile.ps1");
    assert!(inst(&h, &list, PS).expect("写"));
    let ours = alias_path(&h.0, PS);
    assert!(ours.ends_with(".cc-monitor/aliases.ps1"), "{ours:?}");
    let bytes = std::fs::read(&ours).unwrap();
    assert_eq!(
        &bytes[..3],
        &[0xEF, 0xBB, 0xBF],
        "PowerShell 的别名文件落盘没有 BOM"
    );
    let text = String::from_utf8(bytes).unwrap();
    let lines: Vec<String> = list.iter().map(|a| render_line(&a.name, PS)).collect();
    assert_eq!(
        text,
        format!("\u{feff}{}", render_file(PS, &lines)),
        "落盘的不是每条一个函数的那一份"
    );
    assert_eq!(names_on_disk(&h), ["workcc", "mine"]);
    run(block::install_to_profile(&door(&h), &profile)).expect("装别名块");
    let cand = read_in(&h.0, PS, None)
        .unwrap()
        .rc_candidates
        .into_iter()
        .find(|c| c.path == profile.display().to_string())
        .expect("候选里该有 $PROFILE");
    assert!(
        cand.sourced && cand.exists && cand.block.present,
        "{cand:?}"
    );
    assert!(!inst(&h, &list, PS).unwrap(), "内容一样就一个字节不写");
}

// ═══════════════════════════════════════════════════════════════════════════
// 别名块与别名文件那一行共用一份候选、一次扫描
// ═══════════════════════════════════════════════════════════════════════════

/// 用户 rc 里块外自己定义的同名函数，谁生效要按这一条是链接还是终端函数分开说（10-07 沙箱 `bash -ic 'type …'` 读数）：
/// 链接那几条（`~/.cc-monitor/bin/<名>`）是 PATH 上的程序，shell 函数恒盖过它 ⇒ 不论写在接入行之前之后都是「你写的」；
/// 终端函数那几条（撞名才有）是 `aliases.sh` 里的函数，后定义的赢 ⇒ 照行的先后判（与今天同一条）。
#[test]
fn a_users_function_always_beats_a_link_but_only_a_later_one_beats_a_function() {
    let h = tmp_home("wins");
    inst(
        &h,
        &[
            al("zzlinkq", &["--account", "work"]),
            al("sh", &["--account", "work"]),
        ],
        P,
    )
    .expect("写");
    let rc_path = h.0.join(".bashrc");
    std::fs::write(&rc_path, "zzlinkq() { echo mine; }\nsh() { echo mine; }\n").unwrap();
    run(block::install_to_profile(&door(&h), &rc_path)).expect("装别名块（接入行在用户函数之后）");
    let rc = read_in(&h.0, P, None)
        .unwrap()
        .rc_candidates
        .into_iter()
        .find(|c| c.path.ends_with(".bashrc"))
        .expect("候选里有 .bashrc");
    let wins = |n: &str| {
        rc.block
            .conflicting_functions
            .iter()
            .find(|c| c.name == n)
            .map(|c| c.wins)
            .unwrap_or_else(|| panic!("{n} 没认出同名：{:?}", rc.block))
    };
    assert_eq!(
        wins("zzlinkq"),
        block::Wins::Yours,
        "链接那条：函数恒盖过 PATH 上的程序"
    );
    assert_eq!(
        wins("sh"),
        block::Wins::List,
        "终端函数那条：接入行在后 ⇒ 清单那条后定义、生效"
    );
}

/// 🔴 **P3**：别名块的现状**随候选走**，不另起一扫 —— 往某一份候选里装了块，读回口报「块在」的候选集合
/// **恰好**就是那一份（两向：装了的那份报在；没装的一份都不报在）。两种方言各走一遍。
///
/// 替身门落在临时 home：PowerShell 的候选（`startup_files`）在临时 home 底下退回 `home/Documents`，
/// 结构上碰不到真实家目录；写经 `profile_installer::install_to_profile`（生产那一跳同一个函数）。
#[test]
fn the_block_state_rides_on_the_candidates_it_was_installed_into() {
    let h = tmp_home("block-cands");
    // POSIX：两份在盘上的 rc，装进后一份。
    std::fs::write(h.0.join(".bashrc"), "# mine\n").unwrap();
    std::fs::write(h.0.join(".zshrc"), "# mine\n").unwrap();
    let zsh = h.0.join(".zshrc");
    run(block::install_to_profile(&door(&h), &zsh)).expect("装进 .zshrc");
    let got: Vec<String> = rc_candidates_in(&h.0, P, None)
        .into_iter()
        .filter(|c| c.block.present)
        .map(|c| c.path)
        .collect();
    assert_eq!(
        got,
        vec![zsh.display().to_string()],
        "POSIX：报「块在」的不是恰好装过的那一份"
    );

    // PowerShell：PS 7 的目录在 ⇒ 四份候选；装进 PS 7 的 AllHosts 那一份（它从前被当成「装错了的遗留」）。
    let ps7 = h.0.join("Documents/PowerShell");
    std::fs::create_dir_all(&ps7).unwrap();
    let cands = rc_candidates_in(&h.0, Shell::PowerShell, None);
    assert_eq!(cands.len(), 4, "PS 7 目录在时该列四份：{cands:?}");
    let target = std::path::PathBuf::from(&cands[3].path);
    run(block::install_to_profile(&door(&h), &target)).expect("装进 $PROFILE");
    let after = rc_candidates_in(&h.0, Shell::PowerShell, None);
    let present: Vec<&str> = after
        .iter()
        .filter(|c| c.block.present)
        .map(|c| c.path.as_str())
        .collect();
    assert_eq!(present, vec![target.display().to_string().as_str()]);
    let hit = after.iter().find(|c| c.block.present).unwrap();
    assert!(
        hit.exists && hit.block.version.is_some(),
        "PowerShell 那一对围栏带版本串：{hit:?}"
    );
    // 同一次读：装了别名块就接上了别名文件（块结尾那一行）—— 与 POSIX 对称；
    //   从前这里断言「仍是假」（PowerShell 的别名块不接，那一侧只有代装那一处，`AL1d.md §5` 第 4 条）。
    assert!(hit.sourced, "{hit:?}");
}

/// 🔴 **P6**（读回口那一半）：人另指的「其它文件」过围栏 —— 跑出 home 的一律拒、原话带「拒绝写这个配置文件」；
/// home 之内的并进候选、带回过了围栏之后的绝对路径（界面拿它认出刚指的是哪一份）。
#[test]
fn another_startup_file_goes_through_the_fence_before_it_is_read() {
    let h = tmp_home("other-rc");
    for bad in ["/etc/profile", "relative.rc", "~/../x.rc"] {
        let e = read_in(&h.0, P, Some(bad)).expect_err(bad);
        assert!(
            [
                "rsProfileInstaller.fence.outsideHome",
                "rsProfileInstaller.fence.notAbsolute",
                "rsProfileInstaller.fence.dotdot"
            ]
            .iter()
            .any(|k| copy_core::copy_matches(k, &e)),
            "{bad}：{e}"
        );
    }
    let ok = read_in(&h.0, P, Some("~/.config/x.rc")).expect("home 之内的放行");
    let want = h.0.join(".config/x.rc").display().to_string();
    assert_eq!(ok.other_rc.as_deref(), Some(want.as_str()));
    assert!(ok.rc_candidates.iter().any(|c| c.path == want && !c.exists));
}

/// 别名文件那条**给人看、也写进启动文件那一行**的绝对路径逐段拼。
///
/// 要求：「「怎么读到这个事实」   → platform      （各平台读法不同）」——
/// `our_alias_file_rel` 是 `/` 分隔的**通用层结构**（交给后端的 `rel`），翻成那台机器上的路径是平台那一步；
/// 读数要求：「`aliases_read` 回的路径分隔符混用：`C:\Users\user\.cc-monitor/aliases.ps1`」。
/// 拼法从 `Path::join`（monitor 这台的分隔符）换成 `user_files::join_under`（**那台 home 自己的分隔符**）：
/// 从前「行为这一半只能在真 Windows 上看」，今天是纯字符串 ⇒ 在这台 Linux 上就能把 Windows 那一形判完 ——
/// ① Windows 的 home 全用 `\`（F8 不回来）② 远端 POSIX 的 home 全用 `/`（Windows 上的 monitor 拼远端不再混出 `\`）
/// ③ 生产那一口就是交给 `join_under`（不在这里另拼一份）。
#[test]
fn the_alias_file_path_is_joined_segment_by_segment() {
    assert_eq!(
        alias_file_in(r"C:\Users\user", Shell::PowerShell),
        r"C:\Users\user\.cc-monitor\aliases.ps1",
        "Windows 的 home：两种分隔符混着就是 F8 那一形"
    );
    assert_eq!(
        alias_file_in("/home/pi", Shell::Posix),
        "/home/pi/.cc-monitor/aliases.sh"
    );
    assert_eq!(
        alias_file_in("/home/pi/", Shell::PowerShell),
        "/home/pi/.cc-monitor/aliases.ps1",
        "home 末尾的分隔符不许拼出双斜杠"
    );
    let body = guard_core::production_code(include_str!(
        "../../../../src/backend/assets/aliases/mod.rs"
    ));
    let f = body
        .split("pub(crate) fn alias_file_in(home: &str, shell: Shell) -> String {")
        .nth(1)
        .and_then(|b| b.split("\n}\n").next())
        .expect("找不到 `alias_file_in` 的函数体 —— 抽取器坏了");
    guard_core::pin_line(
        f,
        "door::join_under(home, shell.dialect().our_alias_file_rel())",
    )
    .expect("拼法不再交给 `join_under` 了 —— 又自己拼一份？");
    assert!(
        !guard_core::contains_word(f, ".join("),
        "又用 `Path::join` 了 —— 那是 monitor 这台的分隔符"
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// 远端别名清单 → 规则进了那台后端，`origin` 退役
// 要求：「`origin` 是本机还是远端，对这些命令没有区别」。
// 从前这里三条判「本机 / 远端同一个函数只差 origin」「远端撞名不拿 monitor 的 PATH 说事」「远端围栏不量 monitor 的盘」——
// 规则住那台后端之后三件事都是**这台自己的事实**（同一个函数、查这台的 PATH、量这台的盘），那一维没了 ⇒ 三条退役，
// 换成下面两条：撞名查这台的 `PATH`（正反两向）· 围栏量这台的盘（符号链接那一步本机远端都做）。
// ═══════════════════════════════════════════════════════════════════════════

/// 🔴 **B3′**：撞名查的是**这台后端进程**的 `PATH`（规则住在那台上）：`sh` 在这台的 `PATH` 上 ⇒ 报（正控）；
/// 一个哪儿都没有的名字 ⇒ 不报。
#[test]
fn a_name_on_this_machines_path_is_reported() {
    let here = collision_note("sh", P);
    assert!(
        here.as_deref().is_some_and(|c| c.contains("sh")),
        "这台的 PATH 上有 `sh`，该报 —— {here:?}"
    );
    assert_eq!(collision_note("zzz_no_such_command_anywhere", P), None);
}

/// 🔴 **B4′**：围栏两层 —— 词法四条按字符串判（一个盘上**不存在**的 home 也判得完；Windows 写法同样）·
/// 符号链接那一步量**这台的盘**：跑出 home 的链接拒。
#[test]
fn the_fence_is_lexical_then_measures_this_machines_disk() {
    use block::{fence, fence_lexical};
    let ghost = "/nonexistent-ccm-al2-home";
    assert!(!Path::new(ghost).exists(), "夹具前提：这个 home 不在盘上");
    assert_eq!(
        fence(ghost, "~/.zshrc").as_deref(),
        Ok("/nonexistent-ccm-al2-home/.zshrc")
    );
    for bad in [
        "/etc/profile",
        "relative.rc",
        "~/../x.rc",
        "/nonexistent-ccm-al2-homeX/.rc",
    ] {
        let e = fence(ghost, bad).expect_err(bad);
        assert!(
            [
                "rsProfileInstaller.fence.outsideHome",
                "rsProfileInstaller.fence.notAbsolute",
                "rsProfileInstaller.fence.dotdot"
            ]
            .iter()
            .any(|k| copy_core::copy_matches(k, &e)),
            "{bad}：{e}"
        );
    }
    assert_eq!(
        fence_lexical(r"C:\Users\user", r"~\Documents\x.ps1").as_deref(),
        Ok(r"C:\Users\user\Documents\x.ps1")
    );
    assert!(fence_lexical(r"C:\Users\user", r"D:\x.ps1").is_err());
    #[cfg(unix)]
    {
        let h = tmp_home("b4-link");
        let outside = tmp_home("b4-outside");
        std::os::unix::fs::symlink(&outside.0, h.0.join("link")).expect("造一条跑出 home 的链接");
        let raw = format!("{}/link/.bashrc", hs(&h));
        let e = fence(&hs(&h), &raw).expect_err("该被符号链接那一步拒");
        assert!(
            copy_core::copy_matches("rsProfileInstaller.fence.symlinkEscape", &e),
            "{e}"
        );
    }
}

/// 🔴 候选**经门**、按各自方言的列法列（表第一行；`AL2.md §2.4`）：
/// POSIX 只列在的（盘上只放 `.zshrc` ⇒ 候选恰好这一份）；PowerShell 5.1 两份恒列、7 的两份目录在才列 ——
/// 「目录在不在」问的是门（`stat_kind`），不是 monitor 这台的盘（替身门恰好落在同一块盘上，所以另数一遍门被问了什么）。
#[test]
fn the_candidates_are_listed_by_each_dialects_rule_through_the_door() {
    let h = tmp_home("cands-door");
    std::fs::write(h.0.join(".zshrc"), "# mine\n").unwrap();
    let got: Vec<String> = rc_candidates_in(&h.0, P, None)
        .into_iter()
        .map(|c| c.path)
        .collect();
    assert_eq!(got, vec![format!("{}/.zshrc", hs(&h))]);
    let ps: Vec<(String, bool)> = rc_candidates_in(&h.0, Shell::PowerShell, None)
        .into_iter()
        .map(|c| (c.path, c.exists))
        .collect();
    assert_eq!(ps.len(), 2, "PS 7 目录不在时只列 5.1 那两份：{ps:?}");
    assert!(
        ps.iter().all(|(_, e)| !e),
        "5.1 那两份不在也列、标「不在」：{ps:?}"
    );
    std::fs::create_dir_all(h.0.join("Documents/PowerShell")).unwrap();
    assert_eq!(rc_candidates_in(&h.0, Shell::PowerShell, None).len(), 4);
}

/// 🔴 **U1**：在盘上、可那台后端读不了的候选 **照列、带原话**，不吞成「没有别名块」（`AL2.md §2.6`；「出声不静默」）。
/// 反向：读得了的那一份 `unreadable` 为空；整趟读回口不因一份读不了的 rc 失败。
#[test]
fn an_unreadable_candidate_is_listed_with_the_backends_words() {
    let h = tmp_home("unreadable");
    std::fs::write(h.0.join(".bashrc"), [0xff_u8, 0xfe, 0x00, 0x80]).unwrap();
    std::fs::write(h.0.join(".zshrc"), "# mine\n").unwrap();
    let got = read_in(&h.0, P, None).expect("一份读不了的 rc 不该让整趟失败");
    let bash = got
        .rc_candidates
        .iter()
        .find(|c| c.path.ends_with("/.bashrc"))
        .expect("读不了的那一份也要列");
    assert!(bash.exists, "{bash:?}");
    assert!(
        bash.unreadable.as_deref().is_some_and(|w| !w.is_empty()),
        "★ 读不了被吞成了「没有别名块」：{bash:?}"
    );
    let zsh = got
        .rc_candidates
        .iter()
        .find(|c| c.path.ends_with("/.zshrc"))
        .expect("正控：读得了的那一份");
    assert_eq!(zsh.unreadable, None, "{zsh:?}");
}

/// 🔴 **P1′**：这台后端说不说 PowerShell 只问它自己的平台（`platform::shell::speaks_powershell`）——
/// 不在 Windows ⇒ PowerShell 形拒、话里点名这台与 PowerShell；POSIX 恒放行（正控）。
/// ⚠ 门禁跑在 Linux 上 ⇒ 只判得到「拒」那一臂；Windows 那一臂放行由 `speaks_powershell` 的 `cfg!(windows)` 一处答。
#[test]
fn a_dialect_this_machine_does_not_speak_is_refused_out_loud() {
    assert_eq!(dialect_here(P), Ok(()));
    if crate::platform::shell::speaks_powershell() {
        assert_eq!(dialect_here(Shell::PowerShell), Ok(()));
    } else {
        let e = dialect_here(Shell::PowerShell).expect_err("不在 Windows 的后端 × PowerShell 该拒");
        assert!(
            e.contains("PowerShell") && e.contains(&crate::assets::asset_catalog::machine_label()),
            "{e}"
        );
        let h = tmp_home("ps-refuse");
        let (code, _) = answer_read(&door(&h), &json!({ "shell": "powershell", "rcPath": null }))
            .expect_err("线上那一口也该拒");
        assert_eq!(code, "refused");
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 线上那六口：跨语言金样（界面 `src/frontend/ui/alias-reads.ts` 的解码器读同一份）＋ 码集合 == 登记表
// ═══════════════════════════════════════════════════════════════════════════

fn golden() -> Value {
    serde_json::from_str(include_str!("../../../__fixtures__/aliases.golden.json")).unwrap()
}

fn sub_home(v: &Value, home: &str) -> Value {
    serde_json::from_str(&v.to_string().replace("<HOME>", home)).unwrap()
}

/// 读回口（启动文件候选）的成品 == 金样（`<HOME>` 换成临时 home）；块预览的键 · 这一族的码集合 == 登记表。
#[test]
fn the_alias_wire_matches_the_cross_language_golden() {
    let g = golden();
    let h = tmp_home("golden");
    std::fs::write(h.0.join(".bashrc"), g["rc"].as_str().unwrap()).unwrap();
    let d = door(&h);
    let home = hs(&h);
    let read = answer_read(&d, &json!({ "shell": "posix", "rcPath": null })).expect("读回");
    let norm = serde_json::from_str::<Value>(&read.to_string().replace(&home, "<HOME>")).unwrap();
    assert_eq!(
        sub_home(&g["readReply"], &home),
        read,
        "readReply 与金样不一样；现算（换回占位）：{}",
        serde_json::to_string_pretty(&norm).unwrap()
    );
    let block = answer_block_render(&d, &json!({ "rcPath": "~/.bashrc" })).expect("块预览");
    let keys: Vec<&str> = block
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(json!(keys), g["blockRenderKeys"]);
    let codes: std::collections::BTreeMap<String, Vec<String>> = crate::stream::inbound::REGISTRY
        .iter()
        .filter(|s| s.name.starts_with("aliases-"))
        .map(|s| {
            (
                s.name.to_string(),
                s.codes.iter().map(|c| c.to_string()).collect(),
            )
        })
        .collect();
    assert_eq!(serde_json::to_value(&codes).unwrap(), g["codes"]);
}

/// 从 monitor `user_files_tests.rs` 搬来：`door::rel_under` 两种路径写法都把写圈在 home 里。
#[test]
fn rel_under_keeps_writes_inside_home_on_both_path_styles() {
    assert_eq!(
        door::rel_under("/home/u", "/home/u/.bashrc").unwrap(),
        ".bashrc"
    );
    assert_eq!(door::rel_under("/home/u/", "/home/u/a/b").unwrap(), "a/b");
    assert_eq!(
        door::rel_under(r"C:\Users\u", r"C:\Users\u\Documents\PowerShell\p.ps1").unwrap(),
        "Documents/PowerShell/p.ps1"
    );
    for bad in ["/home/uu/.bashrc", "/etc/passwd", "/home/u", "/home/u/"] {
        assert!(door::rel_under("/home/u", bad).is_err(), "{bad} 竟然过了");
    }
}

/// ★ 要求：「L 的做法：那台后端只读现问生效策略，块不会加载就明说」。
/// 每份 `$PROFILE` 候选带**加载它的那一代**的执行策略（5.1 目录 ⇒ `powershell`，7 目录 ⇒ `pwsh`）；同一代一次读只问一次；
/// POSIX 候选与人另指的那一份不带（说不出是哪一代加载它）。
#[test]
fn each_profile_candidate_carries_the_policy_of_the_powershell_that_loads_it() {
    use crate::platform::shell::{powershell::ExecPolicy, PsHost};
    let h = tmp_home("ps-policy");
    std::fs::create_dir_all(h.0.join("Documents/PowerShell")).unwrap();
    std::fs::write(h.0.join("mine.ps1"), "# mine\n").unwrap();
    let asked = std::cell::RefCell::new(Vec::new());
    let ask = |host: PsHost| {
        asked.borrow_mut().push(host);
        ExecPolicy {
            host,
            effective: Some(format!("{host:?}")),
            loads: Some(host == PsHost::Core),
            group_policy: false,
            error: None,
        }
    };
    let extra = h.0.join("mine.ps1").display().to_string();
    let got: Vec<(String, Option<PsHost>)> = rc_candidates_asking(
        &door(&h),
        &hs(&h),
        Shell::PowerShell,
        Some(&extra),
        &[],
        &ask,
    )
    .unwrap()
    .into_iter()
    .map(|c| {
        let rel = c.path.strip_prefix(&hs(&h)).unwrap().replace('\\', "/");
        (rel, c.policy.map(|p| p.host))
    })
    .collect();
    let d = Some(PsHost::Desktop);
    let c = Some(PsHost::Core);
    assert_eq!(
        got,
        vec![
            (
                "/Documents/WindowsPowerShell/Microsoft.PowerShell_profile.ps1".into(),
                d
            ),
            ("/Documents/WindowsPowerShell/profile.ps1".into(), d),
            (
                "/Documents/PowerShell/Microsoft.PowerShell_profile.ps1".into(),
                c
            ),
            ("/Documents/PowerShell/profile.ps1".into(), c),
            ("/mine.ps1".into(), None),
        ]
    );
    assert_eq!(
        *asked.borrow(),
        vec![PsHost::Desktop, PsHost::Core],
        "同一代该只问一次"
    );
    std::fs::write(h.0.join(".bashrc"), "# mine\n").unwrap();
    let posix = rc_candidates_asking(&door(&h), &hs(&h), P, None, &[], &ask).unwrap();
    assert!(!posix.is_empty() && posix.iter().all(|c| c.policy.is_none()));
}

/// 「会动 ~/.bashrc 末尾 N 行」的 N：每份候选带上「装进这份会写几行」（与装进去的那一块逐字同源），界面不数。
#[test]
fn each_startup_candidate_says_how_many_lines_the_block_would_add() {
    let h = tmp_home("block-lines");
    std::fs::write(h.0.join(".bashrc"), "export A=1\n").unwrap();
    let cands = rc_candidates_in(&h.0, Shell::Posix, None);
    let rc = cands
        .iter()
        .find(|c| c.path.ends_with(".bashrc"))
        .expect("有 .bashrc 这一份");
    let want = block::render_block(Shell::Posix, &hs(&h))
        .unwrap()
        .lines()
        .count();
    assert!(want > 0);
    assert_eq!(rc.block_lines, want);
}
