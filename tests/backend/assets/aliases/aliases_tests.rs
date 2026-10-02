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
fn alias_path(home: &Path, shell: Shell) -> PathBuf {
    PathBuf::from(alias_file_in(&home.display().to_string(), shell))
}

/// 写别名文件：指纹取盘上此刻那一份（「读回之后没人动过」那一形）。
fn inst(h: &TmpHome, list: &[Alias], shell: Shell) -> Result<AliasInstallReport, InstallErr> {
    let now = std::fs::read_to_string(alias_path(&h.0, shell)).ok();
    install_in(
        &door(h),
        list,
        shell,
        fingerprint_of(now.as_deref()).as_deref(),
    )
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
    TmpHome(d)
}

// 这里原来是「形状围栏」那一条（`validate_alias_line`〔散文墓碑〕只放行
// `名字() { ccm <已知修饰...> "$@"; }` 一形）与四条 `apply(dry_run)` 的判据。前端不再递 shell 文本
// （递的是结构，文本由 `render` 出），围栏没有输入了 ⇒ 挡注入那件事换成下面
// `injection_attempts_arrive_as_plain_args_in_bash`：**让真 bash 执行渲染出来的那一行**，
// 看那几个危险值是不是原样、作为一个参数到了 `ccm` 手里。「预览不写」那一条也不需要了 ——
// `render` 连 home 都不收，结构上够不到任何文件。

/// ★★ 注入：值里的 `;` / `$( )` / 反引号 / `|` / 换行以外的一切，到了 shell 里**只是一个参数**。
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
    ];
    for v in evil {
        let a = al("evil", &["--account", v]);
        assert_eq!(check_alias(&a, P), Ok(()), "{v:?}");
        let script = format!(
            "ccm() {{ printf '%s\\n' \"$@\"; }}\n{}\nevil\n",
            render_line(&a, P)
        );
        let out = std::process::Command::new("bash")
            .arg("-c")
            .arg(&script)
            .output()
            .expect("起 bash");
        assert!(
            out.status.success(),
            "{v:?}：{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let got: Vec<String> = String::from_utf8_lossy(&out.stdout)
            .lines()
            .map(str::to_string)
            .collect();
        assert_eq!(got, a.args, "{v:?} 没有原样到达");
    }
    // 换行 / 控制字符根本进不了渲染（一条别名只许占一行）。
    assert!(check_alias(&al("nl", &["--account", "a\nb"]), P).is_err());
}

/// ★★ **整份重写**：清单里删了一条，它那一行当场没了（「往 rc 里追加」那条路删不掉）。
#[test]
fn rewriting_drops_the_alias_you_deleted() {
    let h = tmp_home("regen");
    let two = vec![
        al("alphacc", &["--account", "z"]),
        al("betacc", &["--account", "b"]),
    ];
    assert!(inst(&h, &two, P).unwrap().wrote_alias_file);
    let p = alias_path(&h.0, P);
    let after = std::fs::read_to_string(&p).unwrap();
    for a in &two {
        assert!(pinned(&after, &render_line(a, P)), "{after}");
    }
    let one = vec![al("alphacc", &["--account", "z"])];
    inst(&h, &one, P).unwrap();
    let after = std::fs::read_to_string(&p).unwrap();
    assert!(
        pinned(&after, &render_line(&one[0], P)),
        "留下来的那条没了：{after}"
    );
    assert!(
        !pinned(&after, &render_line(&two[1], P)),
        "★ 删了一条，它那一行还在：{after}"
    );
}

/// 同名两条 ⇒ 拒，一个字节都不写（后一条会静默盖掉前一条，而用户只会看见「少了一个命令」）。
#[test]
fn duplicate_names_are_refused() {
    let h = tmp_home("dup");
    let two = vec![
        al("alphacc", &["--account", "z"]),
        al("alphacc", &["--account", "b"]),
    ];
    assert!(inst(&h, &two, P).is_err());
    assert!(!alias_path(&h.0, P).exists());
}

/// 🔴 别名文件改名 `account-aliases.sh` → `aliases.sh`，**不留兼容**：盘上只有旧名那份时读回口不读它（当作没有、给首建那几条）。
#[test]
fn the_old_alias_file_name_is_not_read() {
    let h = tmp_home("rename");
    std::fs::create_dir_all(h.0.join(".cc-monitor")).unwrap();
    std::fs::write(
        h.0.join(".cc-monitor/account-aliases.sh"),
        "alphacc() { ccm --account z \"$@\"; }\n",
    )
    .expect("铺旧文件");
    let listing = read_in(&h.0, P, None).expect("读回");
    assert!(
        !listing.exists && listing.aliases == first_aliases(P),
        "读回口去读了旧名：{:?}",
        listing.aliases
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
/// `__ccm_bind`（＋ 接上别名文件）。`cc` / `cct` / `cca` 住清单（[`first_aliases`]）。
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
    assert_eq!(fns(PS, &ps), ["__ccm_bind"]);
    // 正控：认法认得出函数。
    assert_eq!(fns(P, "cc() { ccm \"$@\"; }\n"), ["cc"]);
    let note = collision_note("__ccm_bind", PS).expect("我们的块占着 `__ccm_bind`，该出声");
    assert!(note.contains("__ccm_bind"), "{note}");
    assert!(collision_note("zzz_no_such_command_anywhere", P).is_none());
}

/// 生成文件里**没有时间戳** —— 有了就永远比不出「内容没变」。
#[test]
fn the_generated_file_is_byte_stable() {
    let lines = vec!["alphacc() { ccm --account 'z' \"$@\"; }".to_string()];
    assert_eq!(render_file(P, &lines), render_file(P, &lines));
    assert!(render_file(P, &[]).contains("一条别名都没有"));
}

// ═══════════════════════════════════════════════════════════════════════
// 一类别名 · 两跳（渲染纯 / 写入唯一副作用）· 读回口
// ═══════════════════════════════════════════════════════════════════════

/// 本文件的夹具把 ccm 选项与 claude 的词混写 ⇒ 这里换成 `<交给 claude 的…> -- <ccm 自己的…>` 排列，
/// 意图逐词不变。ccm 的词 = [`ALIAS_FLAGS`] ∪ [`NOT_IN_ALIASES`] ∪ `--ccm-tmux=…`。
fn al(name: &str, args: &[&str]) -> Alias {
    let (mut left, mut right) = (Vec::new(), Vec::new());
    let mut i = 0;
    while i < args.len() {
        let w = args[i];
        if w == "--" {
            left.extend(args[i + 1..].iter().map(|s| s.to_string()));
            break;
        }
        let takes = ALIAS_FLAGS
            .iter()
            .find(|(f, _)| *f == w)
            .map(|(_, t)| *t)
            .or_else(|| {
                NOT_IN_ALIASES
                    .contains(&w)
                    .then_some(usize::from(w == "--ccm-sid"))
            });
        match takes {
            _ if w.starts_with("--ccm-tmux=") => right.push(w.to_string()),
            Some(t) => {
                right.push(w.to_string());
                for _ in 0..t {
                    if i + 1 < args.len() {
                        i += 1;
                        right.push(args[i].to_string());
                    }
                }
            }
            None => left.push(w.to_string()),
        }
        i += 1;
    }
    if !right.is_empty() || left.iter().any(|w| w == "--") {
        left.push("--".into());
        left.extend(right);
    }
    Alias::new(name, left)
}

/// 黄金串：同一份清单渲染出的每一行逐字钉住。
#[test]
fn rendering_is_byte_stable_and_quotes_only_what_needs_it() {
    let r = render(
        &[
            al("alphacc", &["--account", "z"]),
            al(
                "convz",
                &["--ccm-tmux", "--account", "z", "--cwd", "/home/u/文档/c c"],
            ),
            al("mo", &["--model", "it's", "--", "--verbose"]),
        ],
        P,
    );
    assert!(r.problems.is_empty(), "{:?}", r.problems);
    assert_eq!(
        r.lines,
        vec![
            // 调用时跟的参数（`"$@"`）交 claude，别名自己的 ccm 选项在 `--` 右边。
            r#"alphacc() { ccm "$@" -- --account z; }"#.to_string(),
            r#"convz() { ccm "$@" -- --ccm-tmux --account z --cwd '/home/u/文档/c c'; }"#
                .to_string(),
            r#"mo() { ccm --model 'it'\''s' --verbose "$@"; }"#.to_string(),
        ]
    );
    for l in &r.lines {
        assert!(pinned(&r.file_text, l), "整份代码里缺这一行：{l}");
    }
    assert_eq!(
        render(&[al("alphacc", &["--account", "z"])], P).file_text,
        render(&[al("alphacc", &["--account", "z"])], P).file_text
    );
}

/// 🔴 **异源判据**：让真的 bash 去执行渲染出来的那一行 —— 调用时再给的参数接在预置参数后面，
/// 值里的空格 / 单引号 / 中文原样到达 `ccm`（用一个假 `ccm` 函数把收到的 argv 一行一个吐出来）。
#[test]
fn a_rendered_alias_really_appends_the_callers_args_in_bash() {
    let a = al(
        "convz",
        &["--ccm-tmux", "--cwd", "/tmp/a b/文档", "--model", "it's"],
    );
    let line = render_line(&a, P);
    let script = format!("ccm() {{ printf '%s\\n' \"$@\"; }}\n{line}\nconvz --cwd /elsewhere\n");
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
    let got: Vec<String> = String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(str::to_string)
        .collect();
    // 调用时跟的参数落在 `--` 左边（交 claude），别名自己的 ccm 选项留在右边。
    let cut = a.args.iter().rposition(|w| w == "--").expect("有 ccm 部分");
    let mut want = a.args[..cut].to_vec();
    want.extend(["--cwd".to_string(), "/elsewhere".to_string()]);
    want.extend(a.args[cut..].iter().cloned());
    assert_eq!(got, want, "bash 真执行下来的 argv 与清单对不上");
}

/// 写进去的就是预览的那一份；读回来的清单与写进去的**两向相等**。
#[test]
fn what_is_installed_reads_back_as_the_same_list() {
    let h = tmp_home("roundtrip");
    let list = vec![
        al("alphacc", &["--account", "z"]),
        al("alphacct", &["--ccm-tmux", "--account", "z"]),
        al(
            "mine",
            &["--cwd", "/x y", "--ccm-agent", "codex", "--", "--foo"],
        ),
    ];
    let rep = inst(&h, &list, P).expect("写");
    assert!(rep.wrote_alias_file);
    let on_disk = std::fs::read_to_string(alias_path(&h.0, P)).unwrap();
    assert_eq!(
        on_disk,
        render(&list, P).file_text,
        "落盘的不是预览的那一份"
    );
    let back = read_in(&h.0, P, None).expect("读回");
    assert!(
        back.exists && back.unparsed.is_empty(),
        "{:?}",
        back.unparsed
    );
    assert_eq!(back.aliases, list);
    // 再写一次同一份 ⇒ 一个字节都不写。
    assert!(!inst(&h, &list, P).unwrap().wrote_alias_file);
}

/// 读回口认得盘上那份**旧的**（v1 头、值带引号、从前 `"${CCM:-…}"` 那种调用词），
/// 认不出的行**不静默丢**：原文 ＋ 原因。
#[test]
fn the_reader_takes_the_old_file_and_names_what_it_cannot_parse() {
    let h = tmp_home("old");
    let p = alias_path(&h.0, P);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(
        &p,
        "# === cc-monitor account aliases BEGIN v1 ===\n\
         # 注释\n\
         alphacc() { ccm \"$@\" -- --account 'z'; }\n\
         betacct() { \"${CCM:-/h/.cc-monitor/bin/ccm}\" \"$@\" -- --ccm-tmux --account 'b'; }\n\
         alias x=ls\n\
         bad() { ccm \"$@\" -- --ccm-print; }\n\
         # === cc-monitor account aliases END ===\n",
    )
    .unwrap();
    let back = read_in(&h.0, P, None).unwrap();
    assert_eq!(
        back.aliases,
        vec![
            al("alphacc", &["--account", "z"]),
            al("betacct", &["--ccm-tmux", "--account", "b"])
        ]
    );
    assert_eq!(back.unparsed.len(), 2, "{:?}", back.unparsed);
    assert!(
        back.unparsed[0].starts_with("alias x=ls（"),
        "{:?}",
        back.unparsed
    );
    // `--print` 是 claude 的，拿 ccm 的诊断口 `--ccm-print` 当认不出的那一行。
    assert!(
        back.unparsed[1].contains("--ccm-print"),
        "{:?}",
        back.unparsed
    );
    // 文件不在 ≠ 读失败（清单的起点是首建那几条）。
    let empty = tmp_home("none");
    let l = read_in(&empty.0, P, None).unwrap();
    assert!(!l.exists && l.aliases == first_aliases(P) && l.fingerprint.is_none());
}

/// 组合规则：每一条各有一个会被拦下的例子；有一条不合格 ⇒ **整批不写**。
#[test]
fn every_combination_rule_stops_a_bad_alias_and_nothing_is_written() {
    let bad = [
        al("1x", &[]),
        al("a", &["--account", "z", "--base"]),
        al("b", &["--ccm-tmux=w", "--tmux-base", "w"]),
        al("c", &["--ccm-tmux", "--bus-register"]),
        al("d", &["--detach"]),
        al("e", &["--tmux-size", "80x24"]),
        // 第三档（诊断口 · `--attach`）；相对 / 带 `..` 的 `--cwd`。
        al("f", &["--ccm-print"]),
        al("g", &["--attach", "abc"]),
        al("j", &["--cwd", "rel/dir"]),
        al("k", &["--cwd", "/a/../b"]),
        al("h", &["--account"]),
        al("i", &["--cwd", "a\nb"]),
        al("l", &["--ccm-agent", "gemini"]),
    ];
    for a in &bad {
        assert!(check_alias(a, P).is_err(), "该拦没拦：{a:?}");
    }
    // 写错的 agent 说的是 ccm 运行时同一句（列出认得的几家），不是一句笼统的「参数不对」。
    assert_eq!(
        check_alias(&al("l", &["--ccm-agent", "gemini"]), P),
        Err("不认识这个 agent：gemini（认得的：claude / codex）".to_string())
    );
    let good = [
        al(
            "ok1",
            &[
                "--ccm-tmux",
                "--detach",
                "--bus-register",
                "--bus-note",
                "x",
            ],
        ),
        al("ok2", &["--ccm-tmux=w", "--tmux-size", "80x24"]),
        al("ok3", &["--base", "--ccm-agent", "codex"]),
        // 交给 claude 的词原样放行；绝对 `--cwd` 放行（正控）。
        al("ok4", &["--model", "opus", "--continue"]),
        al("ok5", &["--cwd", "/srv/my proj"]),
        // 写空的 agent ⇒ 默认那一家（与 ccm 运行时同一条）。
        al("ok6", &["--ccm-agent", ""]),
    ];
    for a in &good {
        assert_eq!(check_alias(a, P), Ok(()), "{a:?}");
    }
    // PowerShell 目标：盘符根 / UNC 放行，相对与 POSIX 形拒。
    for (cwd, want_ok) in [
        ("C:\\work", true),
        ("\\\\srv\\share", true),
        ("work", false),
        ("/srv", false),
        ("C:\\a\\..\\b", false),
    ] {
        assert_eq!(
            check_alias(&al("w", &["--cwd", cwd]), PS).is_ok(),
            want_ok,
            "{cwd:?}"
        );
    }
    let h = tmp_home("bad");
    let mut list = good.to_vec();
    list.push(bad[1].clone());
    let e = inst(&h, &list, P).unwrap_err();
    assert!(
        matches!(&e, InstallErr::Refused(m) if m.contains("一条都没写")),
        "{e:?}"
    );
    assert!(!alias_path(&h.0, P).exists(), "有一条不合格却写了");
    // 重名也是一条问题。
    let r = render(&[al("z", &[]), al("z", &["--ccm-tmux"])], P);
    assert_eq!(r.problems.len(), 1);
}

/// `--cwd-if` 两个目录都许 `~` 打头或绝对（不含 `..`）；接回会话那一形（调用时的词交给 ccm）只许单放 `--attach`，
/// 没有 tmux 的目标拒（能力闸先说）。真 bash 执行：`cca foo` 到 ccm 手里是 `-- --attach foo`。
#[test]
fn cwd_if_and_the_attach_form_are_checked_like_ccm_takes_them() {
    let raw = |args: &[&str], rest_to: RestTo| Alias {
        name: "x".into(),
        args: args.iter().map(|s| s.to_string()).collect(),
        rest_to,
    };
    let c = RestTo::Agent;
    for ok in [
        &["--", "--cwd-if", "~", "~/文档/c c"][..],
        &[
            "--", "--cwd-if", "/a", "/b", "--cwd-if", "~/x", "/c", "--cwd", "/d",
        ],
        &["--", "--cwd", "~/文档"],
        &["--", "--cwd", "~"],
    ] {
        assert_eq!(check_alias(&raw(ok, c), P), Ok(()), "{ok:?}");
    }
    for bad in [
        &["--", "--cwd-if", "rel", "/b"][..],
        &["--", "--cwd-if", "~/a/../b", "/b"],
        &["--", "--cwd-if", "/a"],
        &["--", "--cwd", "~/../x"],
        &["--", "--cwd", "rel"],
    ] {
        assert!(check_alias(&raw(bad, c), P).is_err(), "{bad:?}");
    }
    let cca = first_aliases(P)
        .into_iter()
        .find(|a| a.name == "cca")
        .expect("首建带 cca");
    assert_eq!(
        (&cca.args[..], cca.rest_to),
        (&raw(&["--", "--attach"], RestTo::Ccm).args[..], RestTo::Ccm)
    );
    assert_eq!(check_alias(&cca, P), Ok(()));
    assert!(check_alias(&cca, PS).is_err_and(|m| m.contains("没有 tmux")));
    for bad in [
        raw(&["--", "--account", "z", "--attach"], RestTo::Ccm),
        raw(&["-p", "--", "--attach"], RestTo::Ccm),
        raw(&["--", "--ccm-tmux"], RestTo::Ccm),
        raw(&["--", "--attach"], c),
    ] {
        assert!(check_alias(&bad, P).is_err(), "{bad:?}");
    }
    let script = format!(
        "ccm() {{ printf '%s\\n' \"$@\"; }}\n{}\ncca foo\n",
        render_line(&cca, P)
    );
    let out = std::process::Command::new("bash")
        .arg("-c")
        .arg(&script)
        .output()
        .expect("起 bash");
    assert_eq!(String::from_utf8_lossy(&out.stdout), "--\n--attach\nfoo\n");
}

/// 读回口：每条的归组 · 这台的账号表 · 哪个号缺哪一条 · 指纹（盘上那一份的；不在 ⇒ `null`）。
/// 归组只看参数（改了名的账号那一形照样归账号组），缺不缺也只看形（不看名字）。
#[test]
fn the_listing_groups_by_shape_and_names_what_each_account_lacks() {
    let h = tmp_home("groups");
    let accts = h.0.join(".cc-monitor/accounts");
    std::fs::create_dir_all(&accts).unwrap();
    std::fs::write(
        accts.join("accounts.json"),
        json!({ "version": 1, "accounts": [
            { "name": "z", "configDir": format!("{}/.cc-monitor/accounts/z", hs(&h)) },
            { "name": "b", "configDir": format!("{}/.cc-monitor/accounts/b", hs(&h)) },
        ]})
        .to_string(),
    )
    .unwrap();
    let list = vec![
        al("cc", &[]),
        al("zz", &["--account", "z"]),
        al("alphacct", &["--ccm-tmux", "--account", "z"]),
        al("work", &["--account", "b", "--cwd", "/w"]),
    ];
    inst(&h, &list, P).expect("写");
    let l = read_in(&h.0, P, None).expect("读回");
    assert_eq!(l.accounts, ["z", "b"]);
    let shape = |a: &str, t: bool| {
        Some(AccountShape {
            account: a.into(),
            tmux: t,
        })
    };
    assert_eq!(
        l.groups,
        vec![None, shape("z", false), shape("z", true), None]
    );
    let lacks: Vec<(String, bool, String)> = l
        .missing
        .iter()
        .map(|m| (m.account.clone(), m.tmux, m.alias.name.clone()))
        .collect();
    assert_eq!(
        lacks,
        vec![
            ("b".into(), false, "betacc".into()),
            ("b".into(), true, "betacct".into())
        ]
    );
    assert_eq!(l.missing[0].alias, al("betacc", &["--account", "b"]));
    let on_disk = std::fs::read_to_string(alias_path(&h.0, P)).unwrap();
    assert_eq!(l.fingerprint, fingerprint_of(Some(&on_disk)));
    assert_ne!(
        fingerprint_of(Some(&on_disk)),
        fingerprint_of(Some(&format!("{on_disk} ")))
    );
}

/// 🔴 存的那一刻盘上那份不是读回时那一份（被别处改过 / 删了）⇒ 一个字节不写、`Stale`；对得上 ⇒ 照写。
#[test]
fn an_alias_file_changed_since_it_was_read_is_not_overwritten() {
    let h = tmp_home("stale");
    let list = vec![al("alphacc", &["--account", "z"])];
    let p = alias_path(&h.0, P);
    let fp0 = read_in(&h.0, P, None).unwrap().fingerprint;
    install_in(&door(&h), &list, P, fp0.as_deref()).expect("不在 ⇒ 指纹 null 写得进");
    let fp1 = read_in(&h.0, P, None).unwrap().fingerprint;
    std::fs::write(&p, "# 别处改的\n").unwrap();
    let e = install_in(&door(&h), &list, P, fp1.as_deref()).expect_err("被别处改过");
    assert!(
        matches!(&e, InstallErr::Stale(m) if m.contains("别处改过")),
        "{e:?}"
    );
    assert_eq!(
        std::fs::read_to_string(&p).unwrap(),
        "# 别处改的\n",
        "被改过还写了"
    );
    let fp2 = read_in(&h.0, P, None).unwrap().fingerprint;
    assert!(
        install_in(&door(&h), &list, P, fp2.as_deref())
            .unwrap()
            .wrote_alias_file
    );
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
    let from = guard_core::find_pinned(&usage_src, "选项\n").expect("用法里「选项」那一段锚不住");
    let usage = &usage_src[from..];
    for (flag, _) in ALIAS_FLAGS {
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
            ALIAS_FLAGS.iter().all(|(f, _)| f != &third),
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

/// 读后端 `control/ccm/mod.rs::CCM_TMUX_CARRIED` 的原文（两棵树不共享源码 ⇒ 异源）。
fn backend_tmux_carried() -> Vec<String> {
    let src = std::fs::read_to_string(
        crate::guard_support::repo_root().join("src/backend/control/ccm/mod.rs"),
    )
    .expect("读后端 ccm/mod.rs");
    let from = guard_core::find_pinned(&src, "pub(crate) const CCM_TMUX_CARRIED: &[&str] = &[\n")
        .expect("`CCM_TMUX_CARRIED` 那张表锚不住");
    let body = &src[from..];
    let body = &body[..body.find("];").expect("表没收尾")];
    body.lines()
        .skip(1)
        .filter_map(|l| {
            let l = l.trim();
            let rest = l.strip_prefix('"')?;
            Some(rest[..rest.find('"')?].to_string())
        })
        .collect()
}

/// 🔴 D6：能力闸拒的那几个旗标 **==** 后端「靠 tmux 活着的 ccm 能力」∩ 别名能放的旗标（两向相等，异源）。
/// 行为那一格：每个旗标**单独**放进一条别名，PowerShell 目标下必以「没有 tmux」被拒；其余旗标单独放进去，
/// 被拒也不许是因为 tmux（闸不许比事实宽）。
#[test]
fn the_tmux_gate_is_exactly_the_backends_tmux_carried_flags() {
    let carried = backend_tmux_carried();
    assert!(
        carried.len() >= 5,
        "后端那张表抽出来太少，抽取器坏了：{carried:?}"
    );
    let want: std::collections::BTreeSet<String> = carried
        .iter()
        // 用户 09-26：能力 `tmux` 的旗标改名成 `--ccm-tmux`（claude 自己有 `--tmux`），其余能力名即旗标名。
        .map(|c| {
            if c == "tmux" {
                "--ccm-tmux".to_string()
            } else {
                format!("--{c}")
            }
        })
        .filter(|f| ALIAS_FLAGS.iter().any(|(a, _)| a == f))
        .collect();
    let got: std::collections::BTreeSet<String> =
        NEEDS_TMUX.iter().map(|s| s.to_string()).collect();
    assert_eq!(got, want, "能力闸与后端的 tmux 载体表对不上");
    for (flag, takes) in ALIAS_FLAGS {
        let mut args = vec![flag.to_string()];
        for _ in 0..*takes {
            args.push("v".to_string());
        }
        let e = check_alias(
            &al("x", &args.iter().map(String::as_str).collect::<Vec<_>>()),
            PS,
        );
        let by_tmux = matches!(&e, Err(m) if m.contains("没有 tmux"));
        assert_eq!(
            by_tmux,
            NEEDS_TMUX.contains(flag),
            "`{flag}` 在 PowerShell 目标下：{e:?}"
        );
    }
    assert!(check_alias(&al("x", &["--ccm-tmux=w"]), PS).is_err_and(|m| m.contains("没有 tmux")));
    // 同一条别名在 POSIX 目标下是合格的（闸只在没有 tmux 的地方关）。
    let full = al(
        "x",
        &[
            "--ccm-tmux",
            "--detach",
            "--bus-register",
            "--tmux-size",
            "80x24",
        ],
    );
    assert_eq!(check_alias(&full, P), Ok(()));
    assert!(check_alias(&full, PS).is_err());
}

/// 🔴 D7：`--bus-note` 要有 `--bus-register` —— 后端 `argv.rs` 那道闸在别名这一侧也有（异源：读后端原文锚它还在）。
#[test]
fn a_bus_note_without_a_registration_is_refused_like_the_backend_does() {
    let argv = std::fs::read_to_string(
        crate::guard_support::repo_root().join("src/backend/control/ccm/argv.rs"),
    )
    .expect("读后端 argv.rs");
    guard_core::find_pinned(&argv, "if !o.bus_note.is_empty() && !o.bus_register {")
        .expect("后端那道「备注要有登记」的闸不在了 —— 本侧这条规则要跟着重看");
    let e = check_alias(&al("x", &["--ccm-tmux", "--detach", "--bus-note", "n"]), P).unwrap_err();
    assert!(e.contains("--bus-register"), "{e}");
    assert_eq!(
        check_alias(
            &al(
                "x",
                &[
                    "--ccm-tmux",
                    "--detach",
                    "--bus-register",
                    "--bus-note",
                    "n"
                ]
            ),
            P
        ),
        Ok(())
    );
}

/// PowerShell：名字大小写不敏感 ⇒ `Zcc` 与 `alphacc` 算重名；POSIX 不算。
#[test]
fn powershell_duplicates_fold_case() {
    let two = [al("alphacc", &[]), al("Zcc", &["--base"])];
    assert_eq!(render(&two, PS).problems.len(), 1);
    assert!(render(&two, P).problems.is_empty());
}

/// PowerShell 传不过去的值在渲染前就拦住（方言答「传不传得过去」，通用层判「那就不合格」）。
#[test]
fn a_value_powershell_would_mangle_is_a_problem_there_only() {
    // 之后 `--cwd` 要是这种 shell 的绝对路径 ⇒ 把那个值换到交给 claude 的位置上测。
    let a = al("x", &["--", "C:\\a \"b\""]);
    assert!(check_alias(&a, PS).is_err());
    assert_eq!(check_alias(&a, P), Ok(()));
}

/// 🔴 PowerShell 那一臂的写与读回：别名文件 `aliases.ps1` 带 BOM；读回的清单 == 写进去的；再写一次一个字节都不动；
/// POSIX 那份文件不受影响。往 `$PROFILE` 装上别名块之后，候选表答「已经接上」—— 与 POSIX 那一侧对称。
#[test]
fn the_powershell_arm_writes_and_reads_back_the_same_list() {
    let h = tmp_home("ps");
    let list = vec![
        al("alphacc", &["--account", "z"]),
        al(
            "mine",
            &["--cwd", "C:\\x y", "--ccm-agent", "codex", "--", "--foo"],
        ),
    ];
    let profile =
        h.0.join("Documents/WindowsPowerShell/Microsoft.PowerShell_profile.ps1");
    let rep = inst(&h, &list, PS).expect("写");
    assert!(rep.wrote_alias_file, "{rep:?}");
    let ours = alias_path(&h.0, PS);
    assert!(ours.ends_with(".cc-monitor/aliases.ps1"), "{ours:?}");
    let bytes = std::fs::read(&ours).unwrap();
    assert_eq!(
        &bytes[..3],
        &[0xEF, 0xBB, 0xBF],
        "PowerShell 的别名文件落盘没有 BOM"
    );
    let text = String::from_utf8(bytes).unwrap();
    assert_eq!(
        text,
        format!("\u{feff}{}", render(&list, PS).file_text),
        "落盘的不是预览的那一份"
    );
    let back = read_in(&h.0, PS, None).expect("读回");
    assert!(
        back.exists && back.unparsed.is_empty(),
        "{:?}",
        back.unparsed
    );
    assert_eq!(back.aliases, list);
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
    assert!(!inst(&h, &list, PS).unwrap().wrote_alias_file);
    // POSIX 那份是另一个文件：没写过就是不在。
    let posix = read_in(&h.0, P, None).unwrap();
    assert!(!posix.exists && posix.aliases == first_aliases(P));
}

// ═══════════════════════════════════════════════════════════════════════════
// 别名块与别名文件那一行共用一份候选、一次扫描
// ═══════════════════════════════════════════════════════════════════════════

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
        assert!(e.starts_with("拒绝写这个配置文件"), "{bad}：{e}");
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
    let here = render(&[al("sh", &[])], P).collisions;
    assert!(
        here.iter().any(|c| c.contains("sh")),
        "这台的 PATH 上有 `sh`，该报 —— {here:?}"
    );
    assert!(render(&[al("zzz_no_such_command_anywhere", &[])], P)
        .collisions
        .is_empty());
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
        assert!(e.starts_with("拒绝写这个配置文件"), "{bad}：{e}");
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
        assert!(e.contains("符号链接"), "{e}");
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
        let (code, _) = answer_render(&json!({ "aliases": [], "shell": "powershell" }))
            .expect_err("线上那一口也该拒");
        assert_eq!(code, "refused");
    }
}

/// `new` 是 ccm 自己的位置词：别名里只许是 `--` 右边第一个词；写在左边就是交给 claude 的一个词（照放）。
#[test]
fn new_is_accepted_only_as_the_first_word_right_of_the_end() {
    let raw = |args: &[&str]| Alias::new("n", args.iter().map(|s| s.to_string()).collect());
    assert_eq!(check_alias(&raw(&["-p", "--", "new", "--base"]), P), Ok(()));
    assert_eq!(
        check_alias(&raw(&["new"]), P),
        Ok(()),
        "左边的 new 是 claude 的"
    );
    assert!(
        check_alias(&raw(&["--", "--base", "new"]), P).is_err(),
        "new 不在右边第一个却放行了"
    );
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

/// 渲染 → 写 → 读回三口的成品 == 金样（`<HOME>` 换成临时 home）；块那三口的码集合 == 登记表。
/// 临时 home 上铺一份账号库清单（一个号 `z`、一个号 `b`）⇒ 读回带账号表、归组与缺的那几条。
/// 写那一口：指纹不对 ⇒ `stale`、一个字节不写；对 ⇒ 写。
#[test]
fn the_alias_wire_matches_the_cross_language_golden() {
    let g = golden();
    let h = tmp_home("golden");
    std::fs::write(h.0.join(".bashrc"), g["rc"].as_str().unwrap()).unwrap();
    let d = door(&h);
    let home = hs(&h);
    let accts = h.0.join(".cc-monitor/accounts");
    std::fs::create_dir_all(&accts).unwrap();
    std::fs::write(
        accts.join("accounts.json"),
        sub_home(&g["manifest"], &home).to_string(),
    )
    .unwrap();
    let args = json!({ "aliases": g["aliases"], "shell": "posix" });
    let render = answer_render(&args).expect("渲染");
    let bad = json!({ "aliases": g["aliases"], "shell": "posix", "fingerprint": null });
    let (code, _) = answer_install(&d, &bad).expect_err("有一条不合格 ⇒ 整批不写");
    assert_eq!(code, "refused");
    let good = json!({ "aliases": [g["aliases"][0]], "shell": "posix", "fingerprint": null });
    let install = answer_install(&d, &good).expect("写");
    let (code, _) = answer_install(&d, &good).expect_err("盘上已不是读到的那一份（不在）⇒ 不写");
    assert_eq!(code, "stale");
    let read = answer_read(&d, &json!({ "shell": "posix", "rcPath": null })).expect("读回");
    let fp = read["fingerprint"].clone();
    assert!(fp.is_string(), "{read}");
    let again = json!({ "aliases": [g["aliases"][0]], "shell": "posix", "fingerprint": fp });
    assert_eq!(
        answer_install(&d, &again).expect("指纹对 ⇒ 照常")["wroteAliasFile"],
        false
    );
    let got = json!({ "renderReply": render, "installReply": install, "readReply": read });
    let norm = serde_json::from_str::<Value>(&got.to_string().replace(&home, "<HOME>")).unwrap();
    for k in ["renderReply", "installReply", "readReply"] {
        assert_eq!(
            sub_home(&g[k], &home),
            got[k],
            "{k} 与金样不一样；现算（换回占位）：{}",
            serde_json::to_string_pretty(&norm[k]).unwrap()
        );
    }
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
