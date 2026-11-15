use super::*;

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
    // 窗格前台的进程（有控制终端、在前台），环境里的窗格却是一个那台 tmux 不认的。
    let t = Pane::new("blow");
    let (_, kid) = t.window("exec env TMUX_PANE=%4242 sleep 600", "sleep");
    let _iso = door::isolate_with(&script);
    let got = tag(kid, "9d66c46d-bf88-4f99-877e-455555555555");
    assert_eq!(got, Outcome::NoSuchPane);
    let said = std::fs::read_to_string(&log).unwrap_or_default();
    let _ = std::fs::remove_dir_all(&dir);
    assert!(
        said.contains("display-message") && said.contains("%4242"),
        "探测没落到注入的假 tmux 上：{said:?}"
    );
}

/// 打上去的 sid 是会话 id（`cmd_wire::Sid`，全仓一个值域）：真 sid（UUID，带 `-`）照打。
#[test]
fn a_real_session_id_is_accepted() {
    let _iso = door::isolate();
    assert_ne!(
        tag(std::process::id(), "9d66c46d-bf88-4f99-877e-455555555555"),
        Outcome::RejectedSid
    );
}

/// fail closed：会话 id 值域外的一律不打 —— 能破坏 tmux 格式串 / 命令语义的，连同 `_`（值域里没有）、`-` 开头与超长。
#[test]
fn a_sid_outside_the_session_id_domain_is_rejected() {
    let _iso = door::isolate();
    let long = "x".repeat(shell_quote_core::SESSION_ID_MAX + 1);
    for bad in [
        "",
        "a b",
        "#{session_name}",
        "a;b",
        "$(id)",
        "a\nb",
        "a'b",
        "a_b-1",
        "-t",
        long.as_str(),
    ] {
        assert_eq!(
            tag(std::process::id(), bad),
            Outcome::RejectedSid,
            "{bad:?} 不该被放行"
        );
    }
}

/// 一个读不到环境的 pid ⇒ 结局是 `PaneUnknown`，一次 tmux 都不起（`host::tmux::seat::own_pane` 在 `gate::probe_with` 之前就给出结局；录下的调用为零）。
/// 读不到 ≠ 不在 tmux 里：容器那一格（`session_added.container`）不许把它报成 `"none"`。
#[test]
fn a_pid_without_tmux_pane_never_reaches_tmux() {
    let dir = std::env::temp_dir().join(format!("ccm-resync-none-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let log = dir.join("argv");
    let script = dir.join("tmux");
    std::fs::write(
        &script,
        format!("#!/bin/sh\necho \"$*\" >> '{}'\nexit 1\n", log.display()),
    )
    .unwrap();
    let _iso = door::isolate_with(&script);
    // PID 0 在 Linux 上不是一个可读的 `/proc` 目录 ⇒ 读不到环境。
    let got = tag(0, "9d66c46d-bf88-4f99-877e-455555555555");
    let said = std::fs::read_to_string(&log).ok();
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(got, Outcome::PaneUnknown);
    assert_eq!(said, None, "读不到环境却还起了 tmux");
}

/// **「不在 tmux 里」那一格真能出来**：有控制终端、环境读得到、`TMUX_PANE` 摘掉的进程
/// ⇒ `NotInTmux`（容器 `"none"` 的唯一来源）；同样没 `TMUX_PANE`、却没控制终端的 ⇒ `Detached`（不是谁的终端会话）。
#[cfg(unix)]
#[test]
fn a_readable_env_without_tmux_pane_is_not_in_tmux() {
    let _iso = door::isolate();
    let t = Pane::new("none");
    let (_, kid) = t.window("exec env -u TMUX_PANE -u TMUX sleep 600", "sleep");
    assert_eq!(
        tag(kid, "9d66c46d-bf88-4f99-877e-455555555555"),
        Outcome::NotInTmux
    );
    let mut loose = detached_sleep(None);
    let got = tag(loose.id(), "9d66c46d-bf88-4f99-877e-455555555555");
    let _ = loose.kill();
    let _ = loose.wait();
    assert_eq!(got, Outcome::Detached);
}

/// sid 不合法时**连环境都不读**（顺序也是判据的一部分：先 fail closed 再做 IO）。
#[test]
fn a_bad_sid_short_circuits_before_any_io() {
    let _iso = door::isolate();
    assert_eq!(tag(std::process::id(), "bad sid"), Outcome::RejectedSid);
}

/// ★★ **那次读的射程不许悄悄变大**。
///
/// `/proc/<pid>/environ` 里有用户全部的密钥类环境变量。打标读的那一个键住宿主口（`host/tmux/seat.rs`），必须是写死的常量，
/// 键名不许成为一维参数。多一处 `proc_env_var(pid, …)` ⇒ 红，来回答新那个键是什么；打标自己（`identity_tag.rs`）一处都不读。
#[test]
fn the_env_key_tagging_reads_is_one_named_constant() {
    let seat = guard_core::production_code(include_str!("../../../src/backend/host/tmux/seat.rs"));
    let tag_src =
        guard_core::production_code(include_str!("../../../src/backend/control/identity_tag.rs"));
    // 反空真地板：抽取塌了 ⇒ 下面几条都会零命中地绿。
    for (name, prod, floor) in [
        ("seat.rs", &seat, 600),
        ("identity_tag.rs", &tag_src, 2_000),
    ] {
        assert!(
            prod.len() > floor,
            "{name} 剥完只剩 {} 字节 —— 剥法坏了，本条此刻在空转",
            prod.len()
        );
    }
    let total = seat.matches("proc_env_var(pid, ").count();
    assert_eq!(
        total, 1,
        "seat.rs 生产段里 `proc_env_var(pid, …)` 有 {total} 处（登记 1 处）。多了 ⇒ 又读了一个环境变量；少了 ⇒ 读回路被摘掉了。"
    );
    assert_eq!(
        seat.matches("proc_env_var(pid, TMUX_PANE_ENV)").count(),
        1,
        "`proc_env_var(pid, TMUX_PANE_ENV)` 在 seat.rs 生产段里不见了 / 变形了"
    );
    assert_eq!(
        tag_src.matches("proc_env_var").count(),
        0,
        "identity_tag.rs 生产段里又自己读环境了（窗格那一格问 `host::tmux::seat::own_pane`）"
    );
}

/// 打标结局 → 容器：**七个结局逐格 == 手写表**。
///
/// 期望是手写的，不从实现生成。要守的方向是「不知道不许报成 `none`」：
/// 四个「不知道」的结局里任何一个被改成 `Some(None)`，界面就会对一条其实在 tmux 里的会话说
/// 「不在 tmux 会话里：程序退了只能 resume」。
#[test]
fn container_maps_every_tag_outcome_to_the_hand_written_table() {
    use crate::wire::SessionContainer;
    let hosted = |t: &str| {
        Some(SessionContainer::Tmux {
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
        (Outcome::Detached, None),
        (Outcome::NotOnPane, None),
        (Outcome::PaneUnknown, None),
        (Outcome::NoSuchPane, None),
        (Outcome::RejectedSid, None),
        (Outcome::Failed("x".into()), None),
    ];
    for (outcome, want) in &table {
        assert_eq!(outcome.container(), *want, "结局 {outcome:?}");
    }
}

/// ★ 打标没打上的那几形（没控制终端 · 不是窗格前台的那一个 · 环境读不到 · tmux 不认那个窗格 · sid 形状不对 · tmux 报错）：
/// 容器说不清 ⇒ 标签页那张脸上「在终端里打开」「看它的终端」都灰着、照实写说不清（这几形窗格上都没挂它的标，
/// 接回那一下认不到，见 `session_open_tests::every_session_the_face_lets_attach_is_found_by_look`）。活着 · 记录还没读完两段一样。
#[test]
fn an_untagged_session_cannot_be_attached_and_says_where_it_is_is_unclear() {
    use crate::wire::{Allow, CanView, Life, SessionFace, SessionFaceExt as _};
    let why = crate::common::cells::Words(copy_core::copy_text("beTerminal.why.unclear", &[]));
    for outcome in [
        Outcome::Detached,
        Outcome::NotOnPane,
        Outcome::PaneUnknown,
        Outcome::NoSuchPane,
        Outcome::RejectedSid,
        Outcome::Failed("x".into()),
    ] {
        let c = outcome.container();
        for life in [Life::live(None, c.as_ref()), Life::reading(c.as_ref())] {
            let f = SessionFace::of(life);
            assert_eq!(
                f.can.attach,
                Allow::No { why: why.clone() },
                "{outcome:?} · {life:?}"
            );
            assert_eq!(
                f.can.view,
                CanView::No { why: why.clone() },
                "{outcome:?} · {life:?}"
            );
        }
    }
}

// ═══ 打标失败说出来 ＋ `#[must_use]` ═══════════════════════════════
//
// 要求：「⇒ 失败那一形要说出来 ＋ `#[must_use]`（`§5.1 A2`；W5-VIS）」。

/// 一个会失败 / 会成功的假 tmux（**绝对路径**交给 `set_sid`，不碰进程级 `PATH`），经 `test_home::write_exe` 写：
/// 写完直接 exec 它也不撞 ETXTBSY。
#[cfg(unix)]
fn fake_tmux(tag: &str, script: &str) -> std::path::PathBuf {
    let p = test_home::scratch(&format!("w5vis-{tag}")).join("tmux");
    test_home::write_exe(&p, script);
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
        (Outcome::Detached, None),
        (Outcome::NotOnPane, None),
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
    match set_sid(
        crate::platform::child::Child::new(&bad),
        "%9".into(),
        "abc",
        "tmux-1-9".into(),
    ) {
        Outcome::Failed(why) => assert!(why.contains(said), "原因里没有 tmux 的原话：{why}"),
        other => panic!("假 tmux 退出 1，结局却是 {other:?}"),
    }
    let mute = fake_tmux("mute", "#!/bin/sh\nexit 1\n");
    match set_sid(
        crate::platform::child::Child::new(&mute),
        "%9".into(),
        "abc",
        "tmux-1-9".into(),
    ) {
        Outcome::Failed(why) => assert!(
            why.contains(copy_core::copy_static!("beLaunch.said.silent")),
            "{why}"
        ),
        other => panic!("{other:?}"),
    }
    // 正控：成功那一形是 `Tagged(终端句柄)`。
    let good = fake_tmux("good", "#!/bin/sh\nexit 0\n");
    assert_eq!(
        set_sid(
            crate::platform::child::Child::new(&good),
            "%9".into(),
            "abc",
            "tmux-1-9".into()
        ),
        Outcome::Tagged("tmux-1-9".into())
    );
    // 起不来（程序不存在）⇒ 仍是 `Failed`，原因说清。
    let gone = std::env::temp_dir().join("ccm-w5vis-s2-definitely-not-here/tmux");
    match set_sid(
        crate::platform::child::Child::new(&gone),
        "%9".into(),
        "abc",
        "tmux-1-9".into(),
    ) {
        Outcome::Failed(why) => assert!(why.contains("起不来 tmux"), "{why}"),
        other => panic!("{other:?}"),
    }
}

/// 接线：唯一调用点（`tag_seen`，watcher 经 `Reactions::tag` 调它）先经 `failure_note` 再取 `container`（不再是一条链 `tag(..).container()`）；
/// `Outcome` 带 `#[must_use]`。两处都是**剥注释后的生产文本**，各带一个正控（合成语料里的旧形必须被认出）。
#[test]
fn w5vis_s2_the_only_caller_says_the_failure_and_the_outcome_is_must_use() {
    fn caller_says_it(prod: &str) -> Result<(), String> {
        let n = prod.matches("= tag(").count();
        if n != 1 {
            return Err(format!(
                "`= tag(` 在 identity_tag 生产段里 {n} 处（要恰好 1）"
            ));
        }
        let at = prod.find("= tag(").unwrap();
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
    let tag_src =
        guard_core::production_code(include_str!("../../../src/backend/control/identity_tag.rs"));
    caller_says_it(&tag_src).unwrap_or_else(|e| panic!("{e}"));
    must_use_on_outcome(&tag_src).unwrap_or_else(|e| panic!("{e}"));
    // 正控：S2 之前的原形必须被认出（量具没瞎）。
    let old_caller = format!(
        "let container = tag(pid, &sid).container();\n{}\n",
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

// ═══ 只给窗格前台那一个进程打标 ═══════════════════════════════════
//
// 私有 server（`crate::tmux_sandbox`：`-L` ＋ 私有 `TMUX_TMPDIR`）上的真窗格；判据自己起的进程摘掉 `TMUX` · `CLAUDE_CONFIG_DIR`，
// 不起真 claude。打标经注入口落到这台私有 server 上（转接脚本经 `/bin/sh` 读，见 `identity_tag_door`）。

/// 一台私有 tmux ＋ 把注入口转到它上面的那份转接脚本。
#[cfg(unix)]
struct Pane {
    sbx: crate::tmux_sandbox::SandboxTmux,
    relay: std::path::PathBuf,
}

#[cfg(unix)]
impl Pane {
    fn new(tag: &str) -> Pane {
        let sbx = crate::tmux_sandbox::SandboxTmux::new(&format!("idtag-{tag}"));
        sbx.ok(&["set-environment", "-g", "-u", "CLAUDE_CONFIG_DIR"]);
        let relay = sbx.dir().join("relay");
        std::fs::write(
            &relay,
            format!(
                "#!/bin/sh\nexec env -u TMUX -u TMUX_PANE TMUX_TMPDIR='{}' tmux -L s -f /dev/null \"$@\"\n",
                sbx.dir().join("tmux").display()
            ),
        )
        .expect("写转接脚本");
        Pane { sbx, relay }
    }
    /// 这台 server 的 socket（`terminal-end` 那一口照 `-S` 问它）。
    fn socket(&self) -> String {
        self.sbx.out(&["display-message", "-p", "#{socket_path}"])
    }
    /// 起一个窗口跑 `cmd`（`/bin/sh -c`），等窗格进程换成 `comm` 且环境读得出（exec 换身那一刻 environ 是空的）；回（窗格 id，窗格进程）。
    fn window(&self, cmd: &str, comm: &str) -> (String, u32) {
        let out = self.sbx.out(&[
            "new-window",
            "-d",
            "-P",
            "-F",
            "#{pane_id} #{pane_pid}",
            cmd,
        ]);
        let (pane, pid) = out.split_once(' ').expect("窗格 id 与 pid");
        let pid: u32 = pid.parse().expect("pane_pid");
        assert!(settled(pid, comm), "窗格进程没换成 {comm}");
        (pane.to_string(), pid)
    }
    /// 这个窗格上此刻的 `@ccm_sid`。
    fn sid_on(&self, pane: &str) -> String {
        self.sbx
            .out(&["display-message", "-p", "-t", pane, "#{@ccm_sid}"])
    }
}

/// 等到 `pid` 的 `comm` 是 `comm`、环境读得出（墙钟只当兜底上限）。
#[cfg(unix)]
fn settled(pid: u32, comm: &str) -> bool {
    guard_core::wall::rounds().any(|_| {
        let c = std::fs::read_to_string(format!("/proc/{pid}/comm")).unwrap_or_default();
        let env = std::fs::read(format!("/proc/{pid}/environ")).unwrap_or_default();
        let hit = c.trim() == comm && !env.is_empty();
        if !hit {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        hit
    })
}

/// 等窗格里的壳把一个 pid 写进 `file`，读回它。
#[cfg(unix)]
fn pid_in(file: &std::path::Path) -> u32 {
    guard_core::wall::rounds()
        .find_map(|_| {
            let p = std::fs::read_to_string(file).ok()?.trim().parse().ok();
            if p.is_none() {
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            p
        })
        .expect("窗格里那个进程的 pid")
}

/// 在测试进程这边起一个 `setsid` 出来的 `sleep`（没控制终端），环境里带 `TMUX_PANE=pane`（`None` ⇒ 不带）。
#[cfg(unix)]
fn detached_sleep(pane: Option<&str>) -> std::process::Child {
    let mut c = std::process::Command::new("setsid");
    c.args(["sleep", "600"])
        .env_remove("TMUX")
        .env_remove("TMUX_PANE")
        .env_remove("CLAUDE_CONFIG_DIR")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    if let Some(p) = pane {
        c.env("TMUX_PANE", p);
    }
    let kid = c.spawn().expect("起不来 setsid sleep");
    assert!(settled(kid.id(), "sleep"), "setsid sleep 没起来");
    kid
}

const MAIN_SID: &str = "1f58e575-0000-4000-8000-000000000001";
const KID_SID: &str = "2c2cb902-0000-4000-8000-000000000002";

/// ★ 只有窗格前台那一个进程打得上标：带着同一个 `TMUX_PANE` 的 ① 没控制终端的进程（`claude -p` 那一形）
/// ② 另一个窗格前台、环境改成这个窗格的进程 ③ 这个窗格 tty 上的后台进程组 —— 都不改窗格标签、容器不知道。
#[cfg(unix)]
#[test]
fn only_the_panes_foreground_process_is_tagged() {
    let t = Pane::new("fg");
    let _iso = door::isolate_with(&t.relay);
    let (pane, main) = t.window("exec cat", "cat");
    let tagged = tag(main, MAIN_SID);
    assert!(
        matches!(tagged, Outcome::Tagged(_)),
        "前台那一个：{tagged:?}"
    );
    assert_eq!(t.sid_on(&pane), MAIN_SID);

    // ① 没控制终端。
    let mut kid = detached_sleep(Some(&pane));
    let got = tag(kid.id(), KID_SID);
    let _ = kid.kill();
    let _ = kid.wait();
    assert_eq!(got, Outcome::Detached, "没控制终端的进程");
    assert_eq!(got.container(), None);
    assert_eq!(t.sid_on(&pane), MAIN_SID, "① 把窗格的标顶掉了");

    // ② 另一个窗格的前台进程组里，环境却改写成这个窗格（窗格里的壳起的子进程）。
    let other_file = t.sbx.dir().join("other-pid");
    t.window(
        &format!(
            "TMUX_PANE={pane} sleep 600 & echo $! > '{}'; wait",
            other_file.display()
        ),
        "sh",
    );
    let other = pid_in(&other_file);
    assert!(settled(other, "sleep"), "另一个窗格里的 sleep 没起来");
    let got = tag(other, KID_SID);
    assert_eq!(got, Outcome::NotOnPane, "别的窗格前台的进程");
    assert_eq!(got.container(), None);
    assert_eq!(t.sid_on(&pane), MAIN_SID, "② 把窗格的标顶掉了");

    // ③ 窗格 tty 上的后台进程组（窗格里的壳开了作业控制，`sleep` 在后台、前台是 `cat`）：同一窗格先挂起一个 claude、再起一个那一形——
    // 前台那一个打上之后，重对一次标（探测到达 · resync）轮到后台那一个，不把前台的标顶掉。
    let bg_file = t.sbx.dir().join("bg-pid");
    let (bg_pane, bg_fg) = t.window(
        &format!(
            "set -m; sleep 600 & echo $! > '{}'; exec cat",
            bg_file.display()
        ),
        "cat",
    );
    let bg = pid_in(&bg_file);
    assert!(settled(bg, "sleep"), "后台 sleep 没起来");
    let got = tag(bg, KID_SID);
    assert_eq!(got, Outcome::NotOnPane, "窗格 tty 上的后台进程组");
    assert_eq!(got.container(), None);
    assert_eq!(t.sid_on(&bg_pane), "", "③ 打上了标");
    assert!(
        matches!(tag(bg_fg, MAIN_SID), Outcome::Tagged(_)),
        "③ 前台那一个"
    );
    assert_eq!(tag(bg, KID_SID), Outcome::NotOnPane, "③ 重对一次标");
    assert_eq!(t.sid_on(&bg_pane), MAIN_SID, "③ 后台那一个把前台的标顶掉了");
}

/// ★ 结束会话的门只认标：带着同一个 `TMUX_PANE`、没控制终端的进程打标被拒之后，对它的 sid「结束」认不到窗格、不碰它。
#[cfg(unix)]
#[test]
fn ending_a_detached_sid_never_touches_the_pane() {
    let t = Pane::new("end");
    let _iso = door::isolate_with(&t.relay);
    let (pane, main) = t.window("exec cat", "cat");
    assert!(matches!(tag(main, MAIN_SID), Outcome::Tagged(_)));
    let mut kid = detached_sleep(Some(&pane));
    let got = tag(kid.id(), KID_SID);
    let _ = kid.kill();
    let _ = kid.wait();
    assert_eq!(got, Outcome::Detached);
    let sock = t.socket();
    let ended = super::super::kill::end_terminal(
        crate::host::tmux::On {
            socket: Some(&sock),
        },
        None,
        &super::super::kill::TerminalEndReq {
            sid: Some(KID_SID.into()),
            ..Default::default()
        },
    );
    assert_eq!(
        ended.map_err(|f| f.code),
        Err(Some("not_known".to_string())),
        "对子进程的 sid 结束却认到了窗格"
    );
    assert_eq!(
        t.sbx
            .out(&["display-message", "-p", "-t", &pane, "#{pane_id}"]),
        pane,
        "窗格没了"
    );
    assert_eq!(t.sid_on(&pane), MAIN_SID);
}
