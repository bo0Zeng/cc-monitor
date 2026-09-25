use super::*;
use crate::shell_dialect::Shell;

const P: Shell = Shell::Posix;

/// POSIX 那一行 source（方言那一份的薄包装，判据里用得多）。
fn source_line(p: &Path) -> String {
    P.dialect().source_line(&p.display().to_string())
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
/// 〔RW1 · 第四波 09-24〕落盘经「门」（生产 = 本机后端的文件管理那一面）；判据用落在临时目录上的替身门。
/// 写的规则（备份 · 原子替换 · 回读 · 回滚）不在这里判 —— 那一份住后端（`files_write_tests.rs`）。
fn door(h: &TmpHome) -> crate::user_files::tests::DiskDoor {
    crate::user_files::tests::DiskDoor::new(&h.0)
}
fn hs(h: &TmpHome) -> String {
    h.0.display().to_string()
}
fn run<T>(f: impl std::future::Future<Output = T>) -> T {
    futures::executor::block_on(f)
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

// 〔AL1 · 2026-09-24〕这里原来是「形状围栏」那一条（`validate_alias_line`〔散文墓碑〕只放行
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
        assert_eq!(
            got,
            vec!["--account".to_string(), v.to_string()],
            "{v:?} 没有原样到达"
        );
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
    assert!(
        run(install_in(&door(&h), &two, None, P))
            .unwrap()
            .wrote_alias_file
    );
    let p = alias_file_in(&h.0, P);
    let after = std::fs::read_to_string(&p).unwrap();
    for a in &two {
        assert!(pinned(&after, &render_line(a, P)), "{after}");
    }
    let one = vec![al("alphacc", &["--account", "z"])];
    run(install_in(&door(&h), &one, None, P)).unwrap();
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
    assert!(run(install_in(&door(&h), &two, None, P)).is_err());
    assert!(!alias_file_in(&h.0, P).exists());
}

/// ★★ rc 那一行：**加一次，第二次一个字节都不写**，而且用户自己的内容一行不动。
#[test]
fn the_rc_source_line_is_added_once_and_keeps_user_content() {
    let h = tmp_home("rc");
    let rc = h.0.join(".bashrc");
    std::fs::write(&rc, "export PATH=$PATH:/opt/bin\nalias ll='ls -l'\n").expect("写 rc");
    let line = source_line(&alias_file_in(&h.0, P));

    let added = run(ensure_rc_source_line(
        &door(&h),
        &hs(&h),
        &rc.display().to_string(),
        &line,
    ))
    .expect("第一次装");
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

    let again = run(ensure_rc_source_line(
        &door(&h),
        &hs(&h),
        &rc.display().to_string(),
        &line,
    ))
    .expect("第二次");
    assert!(!again, "★ 第二次又写了一遍 —— 那正是「重复追加」那条病");
    let twice = std::fs::read_to_string(&rc).expect("读回");
    assert_eq!(twice.matches(&line).count(), 1, "那一行出现了两次：{twice}");

    // 顺带：候选表能认出「已经 source 过了」
    let me = rc_candidates_in(&h.0, P, None)
        .into_iter()
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
    let ccm_block =
        "if [ -r \"$HOME/.cc-monitor/aliases.sh\" ]; then . \"$HOME/.cc-monitor/aliases.sh\"; fi\n";
    std::fs::write(&rc, format!("# mine\n{ccm_block}")).expect("写 rc");
    let before = std::fs::read(&rc).expect("读原文");

    let line = source_line(&alias_file_in(&h.0, P));
    let added = run(ensure_rc_source_line(
        &door(&h),
        &hs(&h),
        &rc.display().to_string(),
        &line,
    ))
    .expect("不该报错");
    assert!(
        !added,
        "★ 已经 source 过了还要再加一行 —— 那正是「重复追加」那条病"
    );
    assert_eq!(std::fs::read(&rc).expect("读回"), before, "rc 必须逐字未变");

    // 候选表也要认得出来（界面上那一项会显「已经 source 过了」）。
    let me = rc_candidates_in(&h.0, P, None)
        .into_iter()
        .find(|c| c.path == rc.display().to_string())
        .expect("候选里该有 .bashrc");
    assert!(me.sourced, "候选表没认出 `$HOME` 那一形：{me:?}");
}

/// 🔴〔RW1 · 第四波 09-24〕别名文件改名 `account-aliases.sh` → `aliases.sh`，**不留兼容**（主会话裁）：
/// 装过旧那一段的 rc，下一次「安装」就被**整块改写**成指向新名字的那一行（同一对围栏），
/// 用户自己的内容一行不动；旧那一行不留、旧名不读。
#[test]
fn an_rc_with_the_old_file_name_is_rewritten_to_the_new_shape_on_next_install() {
    let h = tmp_home("rename");
    let rc = h.0.join(".bashrc");
    let old_line = source_line(&h.0.join(".cc-monitor/account-aliases.sh"));
    std::fs::write(
        &rc,
        format!("# mine\n{RC_BEGIN}\n{old_line}\n{RC_END}\n# tail\n"),
    )
    .expect("铺装过旧名的 rc");
    let line = source_line(&alias_file_in(&h.0, P));
    let added = run(ensure_rc_source_line(
        &door(&h),
        &hs(&h),
        &rc.display().to_string(),
        &line,
    ))
    .expect("改写");
    assert!(added, "旧那一段该被改写");
    let after = std::fs::read_to_string(&rc).expect("读回");
    assert!(pinned(&after, &line), "新那一行没进去：{after}");
    assert!(
        !after.contains("account-aliases.sh"),
        "旧名还留在 rc 里：{after}"
    );
    assert!(
        pinned(&after, "# mine") && pinned(&after, "# tail"),
        "用户内容被动了：{after}"
    );
    assert_eq!(after.matches(RC_BEGIN).count(), 1, "围栏成了两对：{after}");
    // 读回口只认新名：旧文件在盘上也不读（不留读旧名的分支）。
    std::fs::create_dir_all(h.0.join(".cc-monitor")).expect("建目录");
    std::fs::write(
        h.0.join(".cc-monitor/account-aliases.sh"),
        "alphacc() { ccm --account z \"$@\"; }\n",
    )
    .expect("铺旧文件");
    let listing = read_in(&h.0, P, None, 0).expect("读回");
    assert!(
        !listing.exists && listing.aliases.is_empty(),
        "读回口去读了旧名：{:?}",
        listing.aliases
    );
}

/// 那一行在文件不存在时**必须返回 0** —— 它是 `ccm-aliases.sh` 的最后一行。
#[test]
fn the_source_line_is_a_no_op_when_the_file_is_absent() {
    let line = source_line(Path::new("/nowhere/aliases.sh"));
    assert!(
        line.starts_with("if ") && line.ends_with("; fi"),
        "写成了 `&&` 那一形 ⇒ 文件不存在时整行返回 1，而它是那份片段的最后一行：{line}"
    );
    assert!(
        line.contains(P.dialect().our_alias_file_rel().rsplit('/').next().unwrap()),
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
    let line = source_line(&alias_file_in(&h.0, P));
    let e = run(ensure_rc_source_line(
        &door(&h),
        &hs(&h),
        &rc.display().to_string(),
        &line,
    ))
    .expect_err("围栏损坏该中止");
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
        let e = run(ensure_rc_source_line(&door(&h), &hs(&h), bad, "# x")).expect_err("该被围栏拒");
        assert!(e.starts_with("refuse profile path"), "{bad}：{e}");
    }
    // 正例：home 之内那一份真的过得去（文件不存在 ⇒ 停在「读不到」，而不是停在围栏上）。
    let e = run(ensure_rc_source_line(
        &door(&h),
        &hs(&h),
        &h.0.join(".zshrc").display().to_string(),
        "# x",
    ))
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
            collision_note(t, P).unwrap_or_else(|| panic!("`{t}` 在自带别名块里就有，却一声不吭"));
        assert!(note.contains(t), "{note}");
    }
    assert!(
        collision_note("zzz_no_such_command_anywhere", P).is_none(),
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
    if let Some(note) = collision_note("cch", P) {
        assert!(
            !note.contains("ccm-aliases.sh"),
            "`cch` 已经不在自带别名块里了，却仍被报成「自带块占了」：{note}"
        );
    }
}

/// 生成文件里**没有时间戳** —— 有了就永远比不出「内容没变」。
#[test]
fn the_generated_file_is_byte_stable() {
    let lines = vec!["alphacc() { ccm --account 'z' \"$@\"; }".to_string()];
    assert_eq!(render_file(P, &lines), render_file(P, &lines));
    assert!(render_file(P, &[]).contains("一条别名都没有"));
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
    let r = render(
        &[
            al("alphacc", &["--account", "z"]),
            al(
                "convz",
                &["--tmux", "--account", "z", "--cwd", "/home/u/文档/c c"],
            ),
            al("mo", &["--model", "it's", "--", "--verbose"]),
        ],
        P,
    );
    assert!(r.problems.is_empty(), "{:?}", r.problems);
    assert_eq!(
        r.lines,
        vec![
            r#"alphacc() { ccm --account z "$@"; }"#.to_string(),
            r#"convz() { ccm --tmux --account z --cwd '/home/u/文档/c c' "$@"; }"#.to_string(),
            r#"mo() { ccm --model 'it'\''s' -- --verbose "$@"; }"#.to_string(),
        ]
    );
    for l in &r.lines {
        assert!(pinned(&r.code, l), "整份代码里缺这一行：{l}");
    }
    assert_eq!(
        render(&[al("alphacc", &["--account", "z"])], P).code,
        render(&[al("alphacc", &["--account", "z"])], P).code
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
    let mut want = a.args.clone();
    want.extend(["--cwd".to_string(), "/elsewhere".to_string()]);
    assert_eq!(got, want, "bash 真执行下来的 argv 与清单对不上");
}

/// 写进去的就是预览的那一份；读回来的清单与写进去的**两向相等**。
#[test]
fn what_is_installed_reads_back_as_the_same_list() {
    let h = tmp_home("roundtrip");
    let list = vec![
        al("alphacc", &["--account", "z"]),
        al("alphacct", &["--tmux", "--account", "z"]),
        al(
            "mine",
            &["--cwd", "/x y", "--agent", "codex", "--", "--foo"],
        ),
    ];
    let rep = run(install_in(&door(&h), &list, None, P)).expect("写");
    assert!(rep.wrote_alias_file);
    let on_disk = std::fs::read_to_string(alias_file_in(&h.0, P)).unwrap();
    assert_eq!(on_disk, render(&list, P).code, "落盘的不是预览的那一份");
    let back = read_in(&h.0, P, None, 0).expect("读回");
    assert!(
        back.exists && back.unparsed.is_empty(),
        "{:?}",
        back.unparsed
    );
    assert_eq!(back.aliases, list);
    // 再写一次同一份 ⇒ 一个字节都不写。
    assert!(
        !run(install_in(&door(&h), &list, None, P))
            .unwrap()
            .wrote_alias_file
    );
}

/// 读回口认得盘上那份**旧的**（v1 头、值带引号、从前 `"${CCM:-…}"` 那种调用词），
/// 认不出的行**不静默丢**：原文 ＋ 原因。
#[test]
fn the_reader_takes_the_old_file_and_names_what_it_cannot_parse() {
    let h = tmp_home("old");
    let p = alias_file_in(&h.0, P);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(
        &p,
        "# === cc-monitor account aliases BEGIN v1 ===\n\
         # 注释\n\
         alphacc() { ccm --account 'z' \"$@\"; }\n\
         betacct() { \"${CCM:-/h/.cc-monitor/bin/ccm}\" --tmux --account 'b' \"$@\"; }\n\
         alias x=ls\n\
         bad() { ccm --print \"$@\"; }\n\
         # === cc-monitor account aliases END ===\n",
    )
    .unwrap();
    let back = read_in(&h.0, P, None, 0).unwrap();
    assert_eq!(
        back.aliases,
        vec![
            al("alphacc", &["--account", "z"]),
            al("betacct", &["--tmux", "--account", "b"])
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
    let l = read_in(&empty.0, P, None, 0).unwrap();
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
        assert!(check_alias(a, P).is_err(), "该拦没拦：{a:?}");
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
        assert_eq!(check_alias(a, P), Ok(()), "{a:?}");
    }
    let h = tmp_home("bad");
    let mut list = good.to_vec();
    list.push(bad[1].clone());
    let e = run(install_in(&door(&h), &list, None, P)).unwrap_err();
    assert!(e.contains("一条都没写"), "{e}");
    assert!(!alias_file_in(&h.0, P).exists(), "有一条不合格却写了");
    // 重名也是一条问题。
    let r = render(&[al("z", &[]), al("z", &["--tmux"])], P);
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

// ═══════════════════════════════════════════════════════════════════════
// 〔AL1c · 第四波 4B〕`设计/71 §7` W2 / W4：通用层零 shell 文本 · 能力闸 · PowerShell 那一臂的写与读回
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
    let generic = include_str!("../../src/bridge/src/account_aliases.rs");
    assert_eq!(
        shell_syntax_hits(generic),
        Vec::<String>::new(),
        "通用层长出了某种 shell 的文本 —— 写法 / 读法归 `shell_dialect.rs` 那一族（`71 §4.4`）"
    );
    let dialect = include_str!("../../src/bridge/src/shell_dialect.rs");
    let hits = shell_syntax_hits(dialect);
    for must in ["$@", "RemainingArgs", "Test-Path"] {
        assert!(
            hits.iter().any(|h| h == must),
            "尺子量不到方言模块里的 `{must}` —— 它瞎了：{hits:?}"
        );
    }
    let planted = generic.replacen(
        "pub fn render_line(",
        "fn planted() -> &'static str { \"x() { ccm \\\"$@\\\"; }\" }\npub fn render_line(",
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
        .map(|c| format!("--{c}"))
        .filter(|f| ALIAS_FLAGS.iter().any(|(a, _)| a == f))
        .collect();
    let got: std::collections::BTreeSet<String> =
        NEEDS_TMUX.iter().map(|s| s.to_string()).collect();
    assert_eq!(got, want, "能力闸与后端的 tmux 载体表对不上");
    for (flag, takes) in ALIAS_FLAGS {
        let mut args = vec![flag.to_string()];
        if *takes {
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
    assert!(check_alias(&al("x", &["--tmux=w"]), PS).is_err_and(|m| m.contains("没有 tmux")));
    // 同一条别名在 POSIX 目标下是合格的（闸只在没有 tmux 的地方关）。
    let full = al(
        "x",
        &[
            "--tmux",
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
    let e = check_alias(&al("x", &["--tmux", "--detach", "--bus-note", "n"]), P).unwrap_err();
    assert!(e.contains("--bus-register"), "{e}");
    assert_eq!(
        check_alias(
            &al(
                "x",
                &["--tmux", "--detach", "--bus-register", "--bus-note", "n"]
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
    let a = al("x", &["--cwd", "C:\\a \"b\""]);
    assert!(check_alias(&a, PS).is_err());
    assert_eq!(check_alias(&a, P), Ok(()));
}

/// 🔴 PowerShell 那一臂的写与读回：别名文件 `aliases.ps1` 带 BOM；`$PROFILE` 还不在 ⇒ 建出来（带 BOM ＋ 那一对围栏
/// ＋ 一行 source）；读回的清单 == 写进去的；再写一次一个字节都不动；POSIX 那份文件不受影响。
#[test]
fn the_powershell_arm_writes_and_reads_back_the_same_list() {
    let h = tmp_home("ps");
    let list = vec![
        al("alphacc", &["--account", "z"]),
        al(
            "mine",
            &["--cwd", "C:\\x y", "--agent", "codex", "--", "--foo"],
        ),
    ];
    let profile =
        h.0.join("Documents/WindowsPowerShell/Microsoft.PowerShell_profile.ps1");
    let rep = run(install_in(
        &door(&h),
        &list,
        Some(&profile.display().to_string()),
        PS,
    ))
    .expect("写");
    assert!(rep.wrote_alias_file && rep.wrote_rc, "{rep:?}");
    let ours = alias_file_in(&h.0, PS);
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
        format!("\u{feff}{}", render(&list, PS).code),
        "落盘的不是预览的那一份"
    );
    let prof = std::fs::read_to_string(&profile).expect("$PROFILE 该被建出来");
    assert!(prof.starts_with('\u{feff}'), "$PROFILE 没有 BOM：{prof:?}");
    let line = PS.dialect().source_line(&ours.display().to_string());
    assert!(pinned(&prof, &line), "source 那一行没进去：{prof}");
    let body = crate::shell_dialect::strip_bom(&prof);
    assert!(pinned(body, RC_BEGIN) && pinned(body, RC_END), "{prof}");
    let back = read_in(&h.0, PS, None, 0).expect("读回");
    assert!(
        back.exists && back.unparsed.is_empty(),
        "{:?}",
        back.unparsed
    );
    assert_eq!(back.aliases, list);
    let cand = back
        .rc_candidates
        .iter()
        .find(|c| c.path == profile.display().to_string())
        .expect("候选里该有 $PROFILE");
    assert!(cand.sourced && cand.exists, "{cand:?}");
    // 再写一次同一份 ⇒ 两处都一个字节不动。
    let again = run(install_in(
        &door(&h),
        &list,
        Some(&profile.display().to_string()),
        PS,
    ))
    .unwrap();
    assert!(!again.wrote_alias_file && !again.wrote_rc, "{again:?}");
    assert_eq!(std::fs::read_to_string(&profile).unwrap(), prof);
    // POSIX 那份是另一个文件：没写过就是不在。
    let posix = read_in(&h.0, P, None, 0).unwrap();
    assert!(!posix.exists && posix.aliases.is_empty());
}

/// PowerShell 的 `$PROFILE` 里围栏损坏（有 BEGIN 没 END）⇒ 中止、逐字未变（同一份围栏规则，排版按方言）。
#[test]
fn a_damaged_fence_in_the_powershell_profile_is_left_alone() {
    let h = tmp_home("psbad");
    let profile = h.0.join("p.ps1");
    let original = format!("\u{feff}# mine\r\n{RC_BEGIN}\r\nfunction x {{}}\r\n");
    std::fs::write(&profile, &original).unwrap();
    let e = run(install_in(
        &door(&h),
        &[al("alphacc", &[])],
        Some(&profile.display().to_string()),
        PS,
    ))
    .expect_err("围栏损坏该中止");
    assert!(e.contains("找不到配对的 END"), "{e}");
    assert_eq!(std::fs::read_to_string(&profile).unwrap(), original);
}

// ═══════════════════════════════════════════════════════════════════════════
// 〔AL1d · 第四波 4B〕别名块与别名文件那一行共用一份候选、一次扫描（`调研/第四波记录/AL1d.md §2.1`）
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
    run(crate::profile_installer::install_to_profile(
        &door(&h),
        &zsh,
        "cc",
        false,
    ))
    .expect("装进 .zshrc");
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
    run(crate::profile_installer::install_to_profile(
        &door(&h),
        &target,
        "cc",
        true,
    ))
    .expect("装进 $PROFILE");
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
    // 同一次读：别名文件那一行没装过 ⇒ `sourced` 仍是假（两件事各答各的，不串）。
    assert!(!hit.sourced, "{hit:?}");
}

/// 🔴 **P6**（读回口那一半）：人另指的「其它文件」过围栏 —— 跑出 home 的一律拒、原话带「refuse profile path」；
/// home 之内的并进候选、带回过了围栏之后的绝对路径（界面拿它认出刚指的是哪一份）。
#[test]
fn another_startup_file_goes_through_the_fence_before_it_is_read() {
    let h = tmp_home("other-rc");
    for bad in ["/etc/profile", "relative.rc", "~/../x.rc"] {
        let e = read_in(&h.0, P, Some(bad), 0).expect_err(bad);
        assert!(e.starts_with("refuse profile path"), "{bad}：{e}");
    }
    let ok = read_in(&h.0, P, Some("~/.config/x.rc"), 7).expect("home 之内的放行");
    let want = h.0.join(".config/x.rc").display().to_string();
    assert_eq!(ok.other_rc.as_deref(), Some(want.as_str()));
    assert!(ok.rc_candidates.iter().any(|c| c.path == want && !c.exists));
    assert_eq!(ok.bound_terminals, 7, "握手数原样带回（调用方给的）");
}
