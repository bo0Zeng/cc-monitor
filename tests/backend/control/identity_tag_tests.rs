use super::*;

/// 夹具注进每个 `sleep` 的哨兵键（测试进程自己没有它）。
const OWN_ENVIRON_SENTINEL: &str = "CCM_TEST_KID_OWN_ENVIRON";

/// 起一个 `sleep 60`（`shape` 按需改它的环境），**回来时 `/proc/<pid>/environ` 读到的已经是它自己的环境**。
///
/// 🔴 「environ 读得出、非空」**不等于**读到的是子进程自己的环境〔CIFIX-BE 09-30，4.0.0 CI 红两条的根因〕：
/// `posix_spawn` 的 vfork 父进程在子进程 `execve` **换 mm 之前**就被放回来；子进程此刻若被调度走，
/// `/proc/<kid>/environ` 读的是**测试进程自己**的环境 —— 非空、没有令牌、没有 `TMUX_PANE`。
/// 本机要让父进程抢在子进程前面才现形（`sudo chrt -R -f 1 taskset -c 7`：旧门 200/200 读到父进程环境，本门 200/200 对）。
/// ⇒ 门是「读到哨兵」：它只在子进程 exec 完、换上自己的 mm 之后才在；`sleep` 不再 exec ⇒ 之后每次读都是它自己的。
/// 等时**睡着让出 CPU**（子进程可能排在同一个核上）；10 s 等不到就 panic —— 那时的 `None` 是「取不到」不是「没设」。
pub(crate) fn spawn_settled_sleep(
    shape: impl FnOnce(&mut std::process::Command),
) -> std::process::Child {
    let mut cmd = std::process::Command::new("sleep");
    cmd.arg("60");
    shape(&mut cmd);
    // 哨兵在 `shape` 之后注：`shape` 里就算 `env_clear()` 也清不掉它。
    cmd.env(OWN_ENVIRON_SENTINEL, "1")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    let mut kid = cmd
        .spawn()
        .expect("起不来 `sleep` —— 夹具坏了，读数一个字都不能信");
    let pid = kid.id();
    let want = format!("{OWN_ENVIRON_SENTINEL}=1");
    let t0 = std::time::Instant::now();
    let own = loop {
        if let Ok(b) = std::fs::read(format!("/proc/{pid}/environ")) {
            if b.split(|c| *c == 0).any(|e| e == want.as_bytes()) {
                break true;
            }
        }
        if t0.elapsed() > std::time::Duration::from_secs(10) {
            break false;
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    };
    if !own {
        let _ = kid.kill();
        let _ = kid.wait();
        panic!(
            "pid {pid} 等了 10 s 仍读不到它自己的 environ —— 夹具坏了，\
             此刻任何读数说的都不是这个子进程"
        );
    }
    kid
}

/// 〔`INVARIANTS §48.3`〕**那个口真的 fail-closed**：没注入 ⇒ `tag` 炸（连 sid 都不看）；
/// 注入了 ⇒ 探测真落到假 tmux 上（带着子进程环境里的那个 pane）。
/// 住址 `INVARIANTS §48.3` 原文：「拿不到就炸，不许降级裸跑」。
#[cfg(unix)]
#[test]
fn an_in_process_tag_without_a_fake_tmux_blows_up() {
    let blew = std::panic::catch_unwind(|| tag(std::process::id(), "bad sid")).is_err();
    assert!(
        blew,
        "没注入假 tmux，`tag` 却没炸 —— 测试构建里它会去起真 tmux"
    );

    let dir = std::env::temp_dir().join(format!("ccm-resync-rec-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let log = dir.join("argv");
    let script = dir.join("tmux");
    std::fs::write(
        &script,
        format!("#!/bin/sh\necho \"$*\" >> '{}'\nexit 1\n", log.display()),
    )
    .unwrap();
    let mut kid = spawn_settled_sleep(|c| {
        c.env("TMUX_PANE", "%4242");
    });
    let _iso = door::isolate_with(&script);
    let got = tag(kid.id(), "9d66c46d-bf88-4f99-877e-455555555555");
    let _ = kid.kill();
    let _ = kid.wait();
    assert_eq!(got, Outcome::NoSuchPane);
    let said = std::fs::read_to_string(&log).unwrap_or_default();
    let _ = std::fs::remove_dir_all(&dir);
    assert!(
        said.contains("display-message") && said.contains("%4242"),
        "探测没落到注入的假 tmux 上：{said:?}"
    );
}

#[test]
fn a_real_session_id_is_accepted() {
    assert!(sid_is_safe("9d66c46d-bf88-4f99-877e-455555555555"));
    assert!(sid_is_safe("a_b-1"));
}

/// fail closed：能破坏 tmux 格式串 / 命令语义的都不许过。
///
/// ⚠ **`-t` 这种「像旗标」的值刻意不在这张表里** —— 它由 `[A-Za-z0-9_-]` 放行，
/// 而那是对的：`set-option -t <handle> @ccm_sid <值>` 里的值在非选项参数之后，
/// tmux 不会把它再当选项解析；且我们 argv 直传、不过 shell。
/// 写在这里是因为「看起来危险就该禁」是个很容易顺手加进来的错判 —— 真 sid（UUID）
/// 本来就带 `-`，收窄到禁 `-` 会把正常会话全挡掉。
#[test]
fn a_sid_that_could_break_the_format_string_is_rejected() {
    for bad in ["", "a b", "#{session_name}", "a;b", "$(id)", "a\nb", "a'b"] {
        assert!(!sid_is_safe(bad), "{bad:?} 不该被放行");
    }
    assert!(!sid_is_safe(&"x".repeat(129)), "超长 sid 不该被放行");
}

/// ★ 空 pane 目标必须挡住 —— 实测 `-t ''` 会静默解析成「某个会话」。
#[test]
fn an_empty_pane_target_is_rejected() {
    assert!(
        !pane_is_safe(""),
        "空目标放行 ⇒ sid 会被打到一个碰巧的会话上"
    );
    assert!(!pane_is_safe("%"), "只有 % 没有数字也不是 pane id");
}

#[test]
fn only_percent_digits_is_a_pane_id() {
    assert!(pane_is_safe("%0"));
    assert!(pane_is_safe("%12"));
    for bad in ["0", "$0", "@0", "%a", "%1x", " %1", "%1 "] {
        assert!(!pane_is_safe(bad), "{bad:?} 不该被当成 pane id");
    }
}

/// 一个不存在的 pid 拿不到 `TMUX_PANE` ⇒ **不会**去猜一个会话。
///
/// ⚠ 本条**不起 tmux**：`pane_of` 在 `gate::probe` 之前就给出结局。
///
/// 结局从 `NotInTmux` 改成 `PaneUnknown`：PID 0 的环境是**读不到**，不是「读到了、
/// 没设 `TMUX_PANE`」。前者说「不知道」，后者说「不在 tmux 里」—— 容器那一格（`session_added.container`）
/// 不许把前者报成 `"none"`。打标行为不变（两支都不打）。
#[test]
fn a_pid_without_tmux_pane_never_reaches_tmux() {
    let _iso = door::isolate();
    // PID 0 在 Linux 上不是一个可读的 `/proc` 目录 ⇒ 读不到环境。
    assert_eq!(
        tag(0, "9d66c46d-bf88-4f99-877e-455555555555"),
        Outcome::PaneUnknown
    );
}

/// **「不在 tmux 里」那一格真能出来**：本测试进程自己的环境读得到；把 `TMUX_PANE` 摘掉的子进程
/// ⇒ `NotInTmux`（容器 `"none"` 的唯一来源）。与上一条合起来，两格两向各有一个活例。
#[test]
fn a_readable_env_without_tmux_pane_is_not_in_tmux() {
    let _iso = door::isolate();
    let mut child = spawn_settled_sleep(|c| {
        c.env_remove("TMUX_PANE").env_remove("TMUX");
    });
    let got = tag(child.id(), "9d66c46d-bf88-4f99-877e-455555555555");
    let _ = child.kill();
    let _ = child.wait();
    assert_eq!(got, Outcome::NotInTmux);
}

/// sid 不合法时**连环境都不读**（顺序也是判据的一部分：先 fail closed 再做 IO）。
#[test]
fn a_bad_sid_short_circuits_before_any_io() {
    let _iso = door::isolate();
    assert_eq!(tag(std::process::id(), "bad sid"), Outcome::RejectedSid);
}

/// ★★ **那次读的射程不许悄悄变大**。
///
/// `/proc/<pid>/environ` 里有用户全部的密钥类环境变量。本文件抠出来的必须是写死的常量，
/// 键名不许成为一维参数。多一处 `proc_env_var(pid, …)` ⇒ 红，来回答新那个键是什么。
#[test]
fn the_env_key_this_file_reads_is_one_named_constant() {
    let prod = crate::guard_support::production_code(include_str!(
        "../../../src/backend/control/identity_tag.rs"
    ));
    // 反空真地板：抽取塌了 ⇒ 下面几条都会零命中地绿。
    assert!(
        prod.len() > 2_000,
        "剥完只剩 {} 字节 —— 剥法坏了，本条此刻在空转",
        prod.len()
    );
    let total = prod.matches("proc_env_var(pid, ").count();
    assert_eq!(
        total, 1,
        "本文件生产段里 `proc_env_var(pid, …)` 有 {total} 处（登记 1 处）。多了 ⇒ 又读了一个环境变量；少了 ⇒ 读回路被摘掉了。"
    );
    assert_eq!(
        prod.matches("proc_env_var(pid, TMUX_PANE_ENV)").count(),
        1,
        "`proc_env_var(pid, TMUX_PANE_ENV)` 在生产段里不见了 / 变形了"
    );
}

/// 打标结局 → 容器：**七个结局逐格 == 手写表**（`U4b.md §1.1` 那张）。
///
/// 期望是手写的，不从实现生成。要守的方向是「不知道不许报成 `none`」：
/// 四个「不知道」的结局里任何一个被改成 `Some(None)`，界面就会对一条其实在 tmux 里的会话说
/// 「不在 tmux 会话里：程序退了只能 resume」。
#[test]
fn container_maps_every_tag_outcome_to_the_hand_written_table() {
    use crate::stream::wire::{SessionContainer, TerminalHost};
    let hosted = |t: &str| {
        Some(SessionContainer::Hosted {
            host: TerminalHost::Tmux,
            terminal: Some(t.into()),
        })
    };
    let table: Vec<(Outcome, Option<SessionContainer>)> = vec![
        (Outcome::Tagged("tmux-1-2".into()), hosted("tmux-1-2")),
        (
            Outcome::AlreadyCurrent("tmux-1-3".into()),
            hosted("tmux-1-3"),
        ),
        (Outcome::NotInTmux, Some(SessionContainer::None)),
        (Outcome::PaneUnknown, None),
        (Outcome::NoSuchPane, None),
        (Outcome::RejectedSid, None),
        (Outcome::Failed("x".into()), None),
    ];
    for (outcome, want) in &table {
        assert_eq!(outcome.container(), *want, "结局 {outcome:?}");
    }
}

// ═══ 打标失败说出来 ＋ `#[must_use]` ═══════════════════════════════
//
// 要求：「⇒ 失败那一形要说出来 ＋ `#[must_use]`（`§5.1 A2`；W5-VIS）」。

/// 一个会失败 / 会成功的假 tmux（**绝对路径**交给 `set_sid`，不碰进程级 `PATH`）。
///
/// ⚠ 经 `/bin/sh <脚本>` 起、不直接 exec 脚本本身：刚写完就 exec 一个文件，撞上并行测试里别的线程
/// 正在 fork（子进程在 fork 与 exec 之间还攥着那份写 fd）⇒ `ETXTBSY`（全量跑时现打逮到过一次）。
/// `sh` 只是**读**它，不受这一条影响。
#[cfg(unix)]
fn fake_cmd(p: &std::path::Path) -> std::process::Command {
    let mut c = std::process::Command::new("/bin/sh");
    c.arg(p);
    c
}

#[cfg(unix)]
fn fake_tmux(tag: &str, script: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("ccm-w5vis-s2-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("mkdir");
    let p = dir.join("tmux");
    std::fs::write(&p, script).expect("write fake tmux");
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    p
}

/// ★ S2 本体：**打不上的两形说，其余五形不说** —— 七个变体逐格相等（两向）。
#[test]
fn w5vis_s2_the_failure_note_speaks_for_exactly_the_two_untagged_forms() {
    let sid = "9d66c46d-bf88-4f99-877e-455555555555";
    let cases: Vec<(Outcome, Option<&str>)> = vec![
        (
            Outcome::Failed("tmux 报了一句 X".into()),
            Some("tmux 报了一句 X"),
        ),
        (Outcome::RejectedSid, Some("sid 的形状不对")),
        (Outcome::Tagged("tmux-3-1".into()), None),
        (Outcome::AlreadyCurrent("tmux-3-1".into()), None),
        (Outcome::NotInTmux, None),
        (Outcome::PaneUnknown, None),
        (Outcome::NoSuchPane, None),
    ];
    for (o, want) in cases {
        let got = o.failure_note(4242, sid);
        match want {
            None => assert_eq!(got, None, "{o:?} 不是「打标失败」，不该说"),
            Some(why) => {
                let n = got.unwrap_or_else(|| panic!("{o:?} 是打标失败，却一个字都没说"));
                for must in [why, "4242", sid, "wrong_owner"] {
                    assert!(n.contains(must), "{o:?} 那句话里缺 `{must}`：{n}");
                }
            }
        }
    }
}

/// ★ S4 同形：`set-option` 失败时 **tmux 自己说的那句话进原因**（原先 stderr 丢进 `Stdio::null()`）。
/// 真起一个假 tmux 子进程（退出码 · stderr 由它定，不看后端源码 —— 异源）。
#[cfg(unix)]
#[test]
fn w5vis_s2_set_sid_carries_what_tmux_said() {
    let said = "no server running on /tmp/tmux-1000/w5vis";
    let bad = fake_tmux("bad", &format!("#!/bin/sh\necho '{said}' >&2\nexit 1\n"));
    match set_sid(fake_cmd(&bad), "%9".into(), "abc", "tmux-1-9".into()) {
        Outcome::Failed(why) => assert!(why.contains(said), "原因里没有 tmux 的原话：{why}"),
        other => panic!("假 tmux 退出 1，结局却是 {other:?}"),
    }
    let mute = fake_tmux("mute", "#!/bin/sh\nexit 1\n");
    match set_sid(fake_cmd(&mute), "%9".into(), "abc", "tmux-1-9".into()) {
        Outcome::Failed(why) => assert!(why.contains("tmux 没说原因"), "{why}"),
        other => panic!("{other:?}"),
    }
    // 正控：成功那一形是 `Tagged(终端句柄)`。
    let good = fake_tmux("good", "#!/bin/sh\nexit 0\n");
    assert_eq!(
        set_sid(fake_cmd(&good), "%9".into(), "abc", "tmux-1-9".into()),
        Outcome::Tagged("tmux-1-9".into())
    );
    // 起不来（程序不存在）⇒ 仍是 `Failed`，原因说清。
    let gone = std::env::temp_dir().join("ccm-w5vis-s2-definitely-not-here/tmux");
    match set_sid(
        std::process::Command::new(&gone),
        "%9".into(),
        "abc",
        "tmux-1-9".into(),
    ) {
        Outcome::Failed(why) => assert!(why.contains("起不来 tmux"), "{why}"),
        other => panic!("{other:?}"),
    }
    for d in ["bad", "mute", "good"] {
        let _ = std::fs::remove_dir_all(
            std::env::temp_dir().join(format!("ccm-w5vis-s2-{d}-{}", std::process::id())),
        );
    }
}

/// 接线：唯一调用点先经 `failure_note` 再取 `container`（不再是一条链 `tag(..).container()`）；
/// `Outcome` 带 `#[must_use]`。两处都是**剥注释后的生产文本**，各带一个正控（合成语料里的旧形必须被认出）。
#[test]
fn w5vis_s2_the_only_caller_says_the_failure_and_the_outcome_is_must_use() {
    fn caller_says_it(prod: &str) -> Result<(), String> {
        let n = prod.matches("identity_tag::tag(").count();
        if n != 1 {
            return Err(format!(
                "`identity_tag::tag(` 在 watcher 生产段里 {n} 处（要恰好 1）"
            ));
        }
        let at = prod.find("identity_tag::tag(").unwrap();
        let rest = &prod[at..];
        let stmt_end = rest.find(';').ok_or("找不到那条语句的结尾")?;
        if rest[..stmt_end].contains(".container()") {
            return Err("结局在同一条语句里就被取了 `.container()` —— 失败那一形又被吞了".into());
        }
        let note = rest
            .find(".failure_note(")
            .ok_or("调用点后面没有 `.failure_note(`")?;
        let cont = rest
            .find(".container()")
            .ok_or("调用点后面没有 `.container()`")?;
        if note > cont {
            return Err("`.failure_note(` 排在 `.container()` 之后".into());
        }
        Ok(())
    }
    fn must_use_on_outcome(prod: &str) -> Result<(), String> {
        let n = prod.matches("#[must_use").count();
        if n != 1 {
            return Err(format!(
                "`#[must_use` 在 identity_tag 生产段里 {n} 处（要恰好 1）"
            ));
        }
        let at = prod.find("#[must_use").unwrap();
        let next_item = prod[at..]
            .lines()
            .skip(1)
            .find(|l| !l.trim_start().starts_with("#["))
            .unwrap_or("");
        if next_item.trim() != "pub(crate) enum Outcome {" {
            return Err(format!(
                "`#[must_use` 贴的不是 `enum Outcome`，而是 `{}`",
                next_item.trim()
            ));
        }
        Ok(())
    }
    let watcher = crate::guard_support::production_code(include_str!(
        "../../../src/backend/observe/watcher.rs"
    ));
    let tag_src = crate::guard_support::production_code(include_str!(
        "../../../src/backend/control/identity_tag.rs"
    ));
    caller_says_it(&watcher).unwrap_or_else(|e| panic!("{e}"));
    must_use_on_outcome(&tag_src).unwrap_or_else(|e| panic!("{e}"));
    // 正控：S2 之前的原形必须被认出（量具没瞎）。
    let old_caller = format!(
        "let container = crate::control::identity_tag::tag(pid, &sid).container();\n{}\n",
        "sink.send(x);"
    );
    assert!(
        caller_says_it(&old_caller).is_err(),
        "旧的链式写法没被认出 —— 量具瞎了"
    );
    let no_must = tag_src.replacen("#[must_use", "#[doc", 1);
    assert!(
        must_use_on_outcome(&no_must).is_err(),
        "摘掉 must_use 没被认出 —— 量具瞎了"
    );
}
