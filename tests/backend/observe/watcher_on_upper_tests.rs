//! 跟读判据里引到上层的那几条：控制面真打标那一手（`control::identity_tag::tag_seen`）· 帧面宿主 · 进后端的口。
//! 观测面是独立的包（`backend-observe`）、够不到上层 ⇒ 挂在主 crate（经 `cross/observe.rs`）；读者经 `watcher::test_support` 驱动。
//! 打标的口注入一个有状态的假 tmux 脚本（§48.3）；「claude」各自坐在私有 tmux server 的一个窗口里（[`Ttys`]），只为有个真终端。

use crate::observe::watcher::test_support::{kind_of, pidfile_of, write_pidfile, LiveEar, Reader};
use crate::observe::watcher::Reconciled;
use crate::wire::Frame;
use std::path::{Path, PathBuf};

/// 给每个「claude」一个真终端：私有 tmux server（`tmux_sandbox`：`-L` ＋ 私有 `TMUX_TMPDIR`）上一个窗口，前台就是它，
/// 它是那个窗格的根进程，环境里的 `TMUX_PANE` 改成假 tmux 那一头的窗格。
/// 打标只认窗格前台的进程（控制终端 · 前台进程组 · 终端首领写着这个窗格），没控制终端的 `sleep` 冒充不了窗格进程。
struct Ttys {
    sbx: crate::platform::tmux_sandbox::SandboxTmux,
}

impl Ttys {
    fn new(tag: &str) -> Ttys {
        let sbx = crate::platform::tmux_sandbox::SandboxTmux::new(&format!("wup-{tag}"));
        sbx.ok(&["set-environment", "-g", "-u", "CLAUDE_CONFIG_DIR"]);
        Ttys { sbx }
    }
    /// 起一个前台在私有窗格里、环境里写着 `pane` 的「claude」（`sleep`），等它换好身、环境读得出；回它的 pid。
    fn claude(&self, pane: &str) -> u32 {
        let pid: u32 = self
            .sbx
            .out(&[
                "new-window",
                "-d",
                "-P",
                "-F",
                "#{pane_pid}",
                &format!("exec env -u TMUX TMUX_PANE={pane} sleep 600"),
            ])
            .parse()
            .expect("pane_pid");
        let up = guard_core::wall::rounds().any(|_| {
            let c = std::fs::read_to_string(format!("/proc/{pid}/comm")).unwrap_or_default();
            let env = std::fs::read(format!("/proc/{pid}/environ")).unwrap_or_default();
            let hit = c.trim() == "sleep" && !env.is_empty();
            if !hit {
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            hit
        });
        assert!(up, "窗格里的 sleep 没起来");
        pid
    }
    /// [`Ttys::claude`] ＋ 在 `sessions` 下写它的 pidfile（`sid` · `status`）。
    fn claude_with_pidfile(&self, sessions: &Path, pane: &str, sid: &str, status: &str) -> u32 {
        let pid = self.claude(pane);
        write_pidfile(sessions, pid, sid, status);
        pid
    }
    /// 结束那个「claude」（按 pid 发 TERM，等它从 `/proc` 消失：私有 server 替它收尸）。
    fn end(&self, pid: u32) {
        let _ = std::process::Command::new("kill")
            .arg(pid.to_string())
            .status();
        let gone = guard_core::wall::rounds().any(|_| {
            let alive = crate::platform::proc::pid_alive(pid);
            if alive {
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            !alive
        });
        assert!(gone, "pid {pid} 一直没走");
    }
}

/// 一个 tmux 会话里两个窗格各跑一个 claude：各自的标签打在各自的窗格上，对账一轮就稳 —— 之后每一轮一次都不写。
/// 标签若还是会话级单值，两个 claude 每一轮都把对方改回来，而每次「真写了」都会再起一次探测（探测 ↔ 打标死循环）。
///
/// 假 tmux 照真 tmux 的取值规则：窗格上有自己的值就取它，没有就往上取会话那一级的。
#[cfg(target_os = "linux")]
#[test]
fn two_claudes_in_one_tmux_session_settle_after_one_retag_round() {
    let root = std::env::temp_dir().join(format!("ccm-w5-two-in-one-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let opts = root.join("opts");
    let sessions = root.join("sessions");
    std::fs::create_dir_all(&opts).unwrap();
    std::fs::create_dir_all(&sessions).unwrap();
    let script = root.join("tmux");
    std::fs::write(
        &script,
        format!(
            r#"#!/bin/sh
D='{}'
if [ "$2" = display-message ]; then
  v=$(cat "$D/p$5" 2>/dev/null || cat "$D/s" 2>/dev/null)
  printf '%s\n' "$6" | sed -e 's/#{{session_id}}/$0/g' -e "s/#{{@ccm_sid}}/$v/g" -e 's/#{{session_windows}}/1/g' -e 's/#{{@[a-z_]*}}//g'
  exit 0
fi
if [ "$1" = set-option ]; then
  if [ "$2" = -p ]; then printf '%s' "$6" > "$D/p$4"; else printf '%s' "$5" > "$D/s"; fi
  exit 0
fi
exit 1
"#,
            opts.display()
        ),
    )
    .unwrap();
    let _iso = crate::control::identity_tag::door::isolate_with(&script);
    let ttys = Ttys::new("two-in-one");
    let kids: Vec<u32> = ["%1", "%2"].iter().map(|pane| ttys.claude(pane)).collect();
    let files: Vec<PathBuf> = kids
        .iter()
        .zip(["two-a", "two-b"])
        .map(|(k, sid)| pidfile_of(&sessions, *k, sid))
        .collect();
    // 这几条要真打标（落假 tmux）。
    let mut reader = Reader::new(
        root.join("projects"),
        crate::control::identity_tag::tag_seen,
    );
    let first: Vec<bool> = files.iter().map(|f| reader.session_added(f)).collect();
    let rounds = [reader.retag(None), reader.retag(None)];
    drop(ttys);
    std::fs::remove_dir_all(&root).ok();
    assert_eq!(
        first,
        vec![true, true],
        "两个窗格起初都没挂标签 ⇒ 各写一次（夹具自检）"
    );
    assert_eq!(
        rounds,
        [0, 0],
        "同一个 tmux 会话里的两个 claude 在互相改写标签 —— 每一轮对账都「真写了」⇒ 每份快照之后再探一次，不收敛"
    );
}

// ═══ 身份标签对账 ═══════════════════════════════════════════════
//
// 夹具：一个有状态的假 tmux（一个会话 `$7`，`@ccm_sid` 存在一份文件里；`display-message` 读它、`set-option` 写它）＋
// 私有 tmux 窗口里一个前台 `sleep` 当 claude（[`Ttys`]，pidfile 带它的真 procStart）；打标只经假 tmux（§48.3）。

/// 回（假 tmux 脚本，标签文件）。
fn fake_tmux_world(dir: &Path) -> (PathBuf, PathBuf) {
    std::fs::create_dir_all(dir).unwrap();
    let label = dir.join("label");
    let script = dir.join("tmux");
    std::fs::write(
        &script,
        format!(
            "#!/bin/sh\nst='{}'\ncase \"$*\" in\n  *display-message*) printf '$7\\t%s\\t1\\t\\n' \"$(cat \"$st\" 2>/dev/null)\";;\n  *set-option*) eval \"v=\\${{$#}}\"; printf '%s' \"$v\" > \"$st\";;\nesac\n",
            label.display()
        ),
    )
    .unwrap();
    (script, label)
}

/// **标签被外部改掉之后会被纠正**：pidfile 重写（sid 没变）那一支 ＋ 对在跟会话的整批对账（tmux 探测到达时调的那一个）。
/// 要求：「tmux 探测结果到达、pidfile 重写（sid 没变那一支）时顺手比一次标签」。
#[test]
fn an_externally_changed_identity_tag_is_put_back() {
    let dir = std::env::temp_dir().join(format!("ccm-resync-retag-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let (script, label) = fake_tmux_world(&dir.join("tmux"));
    let _iso = crate::control::identity_tag::door::isolate_with(&script);
    let sessions = dir.join("sessions");
    let ttys = Ttys::new("retag");
    let kid = ttys.claude_with_pidfile(&sessions, "%5", "resync-a", "busy");
    // 这几条要真打标（落假 tmux）。
    let mut reader = Reader::new(dir.join("projects"), crate::control::identity_tag::tag_seen);
    let pidfile = sessions.join(format!("{kid}.json"));
    reader.session_added(&pidfile);
    let read = || std::fs::read_to_string(&label).unwrap_or_default();
    assert_eq!(read(), "resync-a", "首次宣告就该打上");

    // ① 外部改掉 ⇒ pidfile 重写（sid 没变，只换了状态）⇒ 纠正。
    std::fs::write(&label, "bg-sid").unwrap();
    write_pidfile(&sessions, kid, "resync-a", "idle");
    reader.session_added(&pidfile);
    let after_rewrite = read();

    // ② 再改掉 ⇒ 整批对账纠正、回写了 1 个；再对一次 ⇒ 0 个（值一样不动）。
    std::fs::write(&label, "bg-sid").unwrap();
    let wrote = reader.retag(None);
    let after_batch = read();
    let wrote_again = reader.retag(None);
    drop(ttys);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(
        after_rewrite, "resync-a",
        "pidfile 重写没有纠正被改掉的标签"
    );
    assert_eq!(
        (after_batch.as_str(), wrote, wrote_again),
        ("resync-a", 1, 0)
    );
}

/// **对齐 = 拿盘上现实对后端的表，只对差异发帧**（与起步初扫同一个 `reconcile_sessions`）。
/// 表里：A（pidfile 删了）· B（进程死了、pidfile 还在）· C（活着、标签被外部改掉）；盘上多一个没跟的 D。
/// 整机一趟 ⇒ 移除 A、B · 宣告 D · 重打 C；只对 C 的一趟 ⇒ 只碰 C。期望全是手写的。
/// 要求：「pidfile 目录逐个重验（多的补 `session_added`，少的补移除）· 每个在跟的 pid 重判活 · …… · 只对差异发帧」。
#[test]
fn resync_reconciles_the_table_against_the_disk_and_emits_only_the_difference() {
    let dir = std::env::temp_dir().join(format!("ccm-resync-recon-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let (script, label) = fake_tmux_world(&dir.join("tmux"));
    let _iso = crate::control::identity_tag::door::isolate_with(&script);
    let sessions = dir.join("sessions");
    let ttys = Ttys::new("recon");
    let a = ttys.claude_with_pidfile(&sessions, "%1", "sid-a", "idle");
    let b = ttys.claude_with_pidfile(&sessions, "%2", "sid-b", "idle");
    let c = ttys.claude_with_pidfile(&sessions, "%5", "sid-c", "idle");
    // 这几条要真打标（落假 tmux）。
    let mut reader = Reader::new(dir.join("projects"), crate::control::identity_tag::tag_seen);
    for k in [a, b, c] {
        reader.session_added(&sessions.join(format!("{k}.json")));
    }
    reader.frames();
    std::fs::remove_file(sessions.join(format!("{a}.json"))).unwrap();
    ttys.end(b);
    std::fs::write(&label, "bg-sid").unwrap();
    ttys.claude_with_pidfile(&sessions, "%9", "sid-d", "idle");
    let frames = |reader: &mut Reader| {
        let mut v: Vec<String> = reader
            .frames()
            .into_iter()
            .map(|f| match f {
                Frame::SessionAdded(crate::wire::SessionAdded { sid, .. }) => {
                    format!("added {sid}")
                }
                Frame::SessionLeft(crate::wire::SessionLeft { sid, .. }) => {
                    format!("removed {sid}")
                }
                other => kind_of(&other),
            })
            .collect();
        v.sort();
        v
    };

    // 只对 C：D 不宣告、A/B 不移除，只重打 C。
    std::fs::write(&label, "bg-sid").unwrap();
    let one = reader.reconcile(&sessions, Some("sid-c"));
    let one_frames = frames(&mut reader);
    let one_label = std::fs::read_to_string(&label).unwrap_or_default();

    // 整机：再改一次标签。
    std::fs::write(&label, "bg-sid").unwrap();
    let all = reader.reconcile(&sessions, None);
    let all_frames = frames(&mut reader);
    drop(ttys);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(
        (one, one_frames, one_label.as_str()),
        (
            Reconciled {
                added: 0,
                removed: 0,
                retagged: 1,
                caught_up: 0
            },
            Vec::<String>::new(),
            "sid-c"
        ),
        "只对一个会话的那一趟碰了别的会话"
    );
    // D 的首次宣告也写一次标签（它住 %9，同一个假会话）⇒ 写入 2 处：C 的纠正 ＋ D 的首打。
    assert_eq!(
        (all, all_frames),
        (
            Reconciled {
                added: 1,
                removed: 2,
                retagged: 2,
                caught_up: 0
            },
            vec![
                "added sid-d".to_string(),
                "removed sid-a".to_string(),
                "removed sid-b".to_string(),
                "session_primed".to_string()
            ]
        ),
        "整机对齐的差异不对"
    );
}

/// 恢复问过记录（`session-open` · `session-say` 恢复那一支同一处）⇒ 在不在交到每一份在跑的 watcher（会话账本据此翻已结束 ⇄ 记录已不在）：
/// 记录不在 ⇒ 交「不在」、不恢复（回记录已不在）；记录在 ⇒ 交「在」、照记录起（哪一家 · 目录）。
#[test]
fn resuming_hands_whether_the_record_is_there_to_the_session_ledger() {
    let home = std::env::temp_dir().join(format!("ccm-resume-seen-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    let proj = home.join("projects").join("-w");
    std::fs::create_dir_all(&proj).unwrap();
    let sid = "7e1f0c2a-9b3d-4e5f-8a6b-0c1d2e3f4a5b";
    let ear = LiveEar::enter();
    let open = crate::faces::session_open_face::resume_in;

    assert!(open(&home, sid).is_none(), "记录不在 ⇒ 不恢复");
    assert_eq!(ear.records_seen(sid), [false]);

    std::fs::write(
        proj.join(format!("{sid}.jsonl")),
        format!("{{\"type\":\"user\",\"cwd\":\"/w\",\"sessionId\":\"{sid}\"}}\n"),
    )
    .unwrap();
    let r = open(&home, sid).expect("记录在 ⇒ 照起");
    assert_eq!(r.cwd, "/w");
    assert_eq!(ear.records_seen(sid), [true]);
    drop(ear);
    let _ = std::fs::remove_dir_all(&home);
}

/// 帧面 `resync` 的成品 == 跨语言金样的形状（键集合相等、每格是计数）；`sid` 给了却不是非空字符串 ⇒ `bad_args`。
/// 界面那一侧 `tests/frontend/ui/resync.vitest.ts` 按同一份金样解。
#[test]
fn resync_face_reply_matches_the_cross_language_golden() {
    /// 照分派那一层解入参（`typed::take`）再跑；回包投成线上那一份，失败只留码。
    fn answer(args: &serde_json::Value) -> Result<serde_json::Value, String> {
        use crate::wire::typed::{Codes as _, Handle as _};
        let req = crate::wire::typed::take(args)
            .map_err(|f| cmd_wire::failed::Failed::code_or_fallback(f.code.clone()))?;
        let cx = crate::wire::typed::Cx::default();
        let got = crate::wire::spec::finish(crate::stream::resync_face::ResyncCmd::run(req, cx))
            .map_err(|e| cmd_wire::failed::Failed::code_or_fallback(e.into_said().code.clone()))?;
        Ok(serde_json::to_value(got).expect("回包总能序列化"))
    }
    let golden: serde_json::Value =
        serde_json::from_str(include_str!("../../__fixtures__/resync.golden.json")).unwrap();
    let got = answer(&golden["request"]).expect("金样那份请求该答得出");
    let keys = |v: &serde_json::Value| -> Vec<String> {
        let mut k: Vec<String> = v.as_object().unwrap().keys().cloned().collect();
        k.sort();
        k
    };
    assert_eq!(keys(&got), keys(&golden["reply"]), "成品的键与金样不一致");
    // 两格计数 ＋ 那一句 ＋ 那一格能力事实（与 hello 同形：`[{command, code}]`）。
    assert!(got["said"].is_string(), "`said` 不是字：{got}");
    for k in ["added", "removed"] {
        assert!(got[k].as_u64().is_some(), "`{k}` 不是计数：{got}");
    }
    let facts = got["unavailable"]
        .as_array()
        .expect("`unavailable` 不是数组");
    assert!(
        facts
            .iter()
            .all(|e| e["command"].is_string() && e["code"].is_string()),
        "`unavailable` 的项不是 {{command, code}}：{got}"
    );
    for bad in [
        serde_json::json!({"sid": 5}),
        serde_json::json!({"sid": ""}),
        serde_json::json!({"sid": "s", "x": 1}),
    ] {
        assert_eq!(
            answer(&bad).map(|_| ()),
            Err("bad_args".to_string()),
            "{bad}"
        );
    }
}

/// `resync` 回包的 `said`：那一句由后端写好，界面照抄（不再拿几格计数自己拼）。
/// 没人在看 ⇒ 只那一句；否则「补读了几条」接「差异」，计数为 0 各换成那一形。
#[test]
fn resync_said_is_written_by_the_backend() {
    use crate::stream::resync_face::said;
    use copy_core::copy_text;
    assert_eq!(said(1, 2, 3, 4, 0), copy_text("resync.done.nobody", &[]));
    assert_eq!(
        said(0, 0, 0, 0, 1),
        format!(
            "{}{}",
            copy_text("resync.done.caughtNone", &[]),
            copy_text("resync.done.same", &[])
        )
    );
    assert_eq!(
        said(1, 2, 3, 4, 2),
        format!(
            "{}{}",
            copy_text("resync.done.caughtUp", &[("n", "4")]),
            copy_text(
                "resync.done.diff",
                &[("added", "1"), ("removed", "2"), ("retagged", "3")]
            )
        )
    );
}
