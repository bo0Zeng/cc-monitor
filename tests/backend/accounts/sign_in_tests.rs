//! 登录作业走一遍：假的登录子命令（临时目录里一份 shell 脚本，放在 `PATH` 最前）照 Claude 实测的那几行说话 ——
//! 打印链接 · 逐行读 `码#state` · 码不对说 `Invalid code` 接着等 · 对了写一份**假**凭据 ＋ 一份假的 `.claude.json`、退 0 · 换码被拒退 1。
//! 作业表是进程里一份：各条判据用各自的号名（同一个号新开一份会替掉旧的）。
//! 链接 · 码 · 凭据都是写死的假串，配置目录在临时目录里；没碰真家目录，也没拿真号登（真号那一格没核）。
#![cfg(unix)]

use super::*;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

const FAKE_URL: &str = "https://claude.com/cai/oauth/authorize?code=true&client_id=fake&state=st8";

/// 认链接 · 判粘的 · 认「码不对」都照 Claude 那一份（[`crate::agents::claudecode::sign_in::FACE`]），程序换成临时目录里那份脚本。
fn face() -> SignInFace {
    crate::agents::claudecode::sign_in::FACE
}

struct Rig {
    root: PathBuf,
}

impl Drop for Rig {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

impl Rig {
    /// `script` ＝ 读到链接之后那一段（`$CLAUDE_CONFIG_DIR` 由作业交）。
    fn new(tag: &str, script: &str) -> Rig {
        let root = std::env::temp_dir().join(format!(
            "login-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|x| x.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(root.join("bin")).unwrap();
        std::fs::create_dir_all(root.join("acct")).unwrap();
        let p = root.join("bin").join("claude");
        let body = if script.starts_with("#!") {
            format!("{script}\n")
        } else {
            format!("#!/bin/sh\n{script}\n")
        };
        // 本进程只写一份旁稿，`bin/claude` 由 `cp` 那个子进程写：本进程里从没有过 `bin/claude` 的写 fd。
        // 在本进程里直接写 ⇒ 并行测试里别的线程正在 fork 时，子进程在 fork 与 exec 之间还攥着那份写 fd，
        // 这时 exec 它 ＝ `ETXTBSY`（`Text file busy (os error 26)`，机器忙时撞得到）。登录作业照 `PATH` 找 `claude`，不能改经 `/bin/sh` 起。
        let draft = root.join("claude.draft");
        std::fs::write(&draft, body).unwrap();
        let cp = std::process::Command::new("cp")
            .arg(&draft)
            .arg(&p)
            .status()
            .unwrap();
        assert!(cp.success(), "cp 没写成：{cp:?}");
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
        Rig { root }
    }

    fn dir(&self) -> String {
        self.root.join("acct").display().to_string()
    }

    fn path(&self) -> String {
        format!("{}:/usr/bin:/bin", self.root.join("bin").display()).into()
    }

    fn ask(&self, account: &'static str, conn: u64) -> (SignInAsk<'static>, String) {
        let dir = self.dir();
        (
            SignInAsk {
                face: face(),
                target: account,
                dir: None,
                then: &[],
                conn,
                cwd: Some(self.root.clone()),
                path: Some(self.path()),
            },
            dir,
        )
    }

    fn start(&self, account: &'static str, conn: u64, ended: &Log) -> Result<Started, Said> {
        let (mut a, dir) = self.ask(account, conn);
        let dir: &'static str = Box::leak(dir.into_boxed_str());
        a.dir = Some(dir);
        start(&a, on_end(ended))
    }
}

/// 发起作业的连接号：跟生产同一个发号器取（[`crate::control::asks::Desk`]，读循环开一条连接就从它拿一个号）。
/// 不写死：同一进程里读循环那几条判据也从 1 起发号，它们的连接一收场就收掉那个号上的登录作业（`Desk` 的 `Drop`）——
/// 写死 1…6 时，机器一忙、两边的时段叠上，这边的作业会被那边收走（`它没自己退: Disconnected` · `本该被拒：Err(NoJob)`）。
fn conn() -> u64 {
    crate::control::asks::Desk::new().conn()
}

type Log = Arc<Mutex<Vec<(String, SignInEnd)>>>;

fn on_end(log: &Log) -> OnEnd {
    let log = Arc::clone(log);
    Box::new(move |job, e| log.lock().unwrap().push((job.to_string(), e.clone())))
}

/// 照 Claude 实测那几行说话；`good#st8` 登成、`bad#st8` 换码被拒、别的说码不对接着等（夹具 `tests/__fixtures__/fake-claude-auth-login.sh`）。
const CLAUDE_LIKE: &str = include_str!("../../__fixtures__/fake-claude-auth-login.sh");

fn read(p: &Path) -> String {
    std::fs::read_to_string(p).unwrap_or_default()
}

/// 起 ⇒ 交回那一家打印的链接；粘对了 ⇒ 那一家自己写凭据、退 0 ⇒ 成；作业收了，「结束了」回调收到一次「成」。
/// 首启引导那一格（假的核法）：核心不改那一家自己的 `.claude.json` —— 登完它就是假登录子命令写的那一份，没多出 `hasCompletedOnboarding`。
#[test]
fn a_pasted_code_signs_the_account_in_and_the_core_writes_nothing_itself() {
    let rig = Rig::new("ok", CLAUDE_LIKE);
    let log: Log = Default::default();
    let s = rig.start("acct-ok", conn(), &log).expect("起得来");
    assert_eq!(s.url, FAKE_URL);
    assert_eq!(finish(&s.job, " good#st8\n"), Ok("acct-ok".to_string()));
    let acct = rig.root.join("acct");
    assert_eq!(
        read(&acct.join(".credentials.json")),
        r#"{"claudeAiOauth":{"accessToken":"fake"}}"#
    );
    assert_eq!(
        read(&acct.join(".claude.json")),
        r#"{"oauthAccount":{"emailAddress":"a@example.test"}}"#
    );
    assert_eq!(
        finish(&s.job, "good#st8"),
        Err(FinishFail::NoJob),
        "作业登完就收了"
    );
    assert_eq!(
        log.lock().unwrap().as_slice(),
        &[(s.job.clone(), SignInEnd::Done)]
    );
}

/// 粘的 state 对不上这一趟的链接 ⇒ 不写进去、作业还在；那一家说码不对 ⇒ 作业还在；再粘对的 ⇒ 成。
#[test]
fn a_bad_paste_keeps_the_job() {
    let rig = Rig::new("bad", CLAUDE_LIKE);
    let log: Log = Default::default();
    let s = rig.start("acct-bad", conn(), &log).unwrap();
    assert_eq!(finish(&s.job, "good#other"), Err(FinishFail::BadPaste));
    assert_eq!(finish(&s.job, "no-hash"), Err(FinishFail::BadPaste));
    assert_eq!(finish(&s.job, "wrong#st8"), Err(FinishFail::BadPaste));
    assert_eq!(finish(&s.job, "good#st8"), Ok("acct-bad".to_string()));
}

/// 换码被拒 ⇒ `Refused`，原话里是它最后说的那一行；作业收了。
#[test]
fn a_refused_exchange_says_why_and_ends_the_job() {
    let rig = Rig::new("refused", CLAUDE_LIKE);
    let log: Log = Default::default();
    let s = rig.start("acct-refused", conn(), &log).unwrap();
    match finish(&s.job, "bad#st8") {
        Err(FinishFail::Refused(said)) => {
            assert!(
                said.raw
                    .as_deref()
                    .unwrap_or("")
                    .contains("Login failed: fake refusal"),
                "{said:?}"
            )
        }
        other => panic!("本该被拒：{other:?}"),
    }
    assert_eq!(finish(&s.job, "good#st8"), Err(FinishFail::NoJob));
    assert!(matches!(
        log.lock().unwrap().as_slice(),
        [(_, SignInEnd::Failed(_))]
    ));
}

/// 取消 · 被同一个号的新作业替掉 · 发起它的连接断了：三种都收作业、杀那一家的子命令，「结束了」回调不出声。
#[test]
fn cancel_replace_and_a_dropped_connection_end_the_job_quietly() {
    let rig = Rig::new("end", CLAUDE_LIKE);
    let log: Log = Default::default();
    let k = conn();
    let a = rig.start("acct-end", k, &log).unwrap();
    assert!(cancel(&a.job));
    assert!(!cancel(&a.job));
    assert_eq!(finish(&a.job, "good#st8"), Err(FinishFail::NoJob));
    let b = rig.start("acct-end", k, &log).unwrap();
    let c = rig.start("acct-end", k, &log).unwrap();
    assert_eq!(
        finish(&b.job, "good#st8"),
        Err(FinishFail::NoJob),
        "被替掉了"
    );
    drop(Desk::of(k));
    assert_eq!(
        finish(&c.job, "good#st8"),
        Err(FinishFail::NoJob),
        "连接断了"
    );
    assert!(log.lock().unwrap().is_empty());
}

/// 本机浏览器回调到那一家自己开的端口、它自己登完退 0（这里用「打印链接后自己退 0」装）⇒ 「结束了」回调收到「成」。
#[test]
fn a_login_that_completes_by_itself_is_reported() {
    let rig = Rig::new(
        "auto",
        // 等判据放行（`start` 已经回了）才自己退：这一条看的是「`start` 回了以后它才退」那个次序；
        //   反过来（它在 `start` 醒来前就退了）是 `a_link_then_an_instant_exit_still_starts`。工作目录是这份临时目录；目录没了（判据中途红了）也退。
        &format!(
            "echo \"visit: {FAKE_URL}\"\nwhile [ ! -e go ] && [ -d bin ]; do sleep 0.02; done\necho \"Login successful.\""
        ),
    );
    let (tx, rx) = std::sync::mpsc::channel::<(String, SignInEnd)>();
    let tx = Mutex::new(tx);
    let (mut a, dir) = rig.ask("acct-auto", conn());
    a.dir = Some(Box::leak(dir.into_boxed_str()));
    let s = start(
        &a,
        Box::new(move |job, e| {
            let _ = tx.lock().unwrap().send((job.to_string(), e.clone()));
        }),
    )
    .unwrap();
    std::fs::write(rig.root.join("go"), "").unwrap();
    let job = s.job.clone();
    // 等那一件事（它自己退了 ⇒ 回调来一次），兜底是那个统一的上限。
    let got = rx
        .recv_timeout(guard_core::wall::CEILING)
        .expect("它没自己退");
    assert_eq!(got, (job.clone(), SignInEnd::Done));
    assert_eq!(
        finish(&job, "good#st8"),
        Err(FinishFail::NoJob),
        "它自己退了，作业也收了"
    );
}

/// 没给出链接就退了 ⇒ 起不成，原话里是它说的那一行。
#[test]
fn no_link_before_it_exits_is_a_failed_start() {
    let rig = Rig::new("nolink", "echo 'boom: no network' >&2\nexit 1");
    let log: Log = Default::default();
    let e = rig.start("acct-nolink", conn(), &log).expect_err("起不成");
    assert!(
        e.raw.as_deref().unwrap_or("").contains("boom: no network"),
        "{e:?}"
    );
}

/// 给了链接、马上自己退了（`start` 那条线程醒来之前作业可能已经收了）⇒ `start` 照收到的输出回链接，不回「没给出链接就退了」；
/// 「结束了」回调同一份作业报「成」：两句对得上。
/// 谁先醒不该要紧 ⇒ 作业那几条线程（`start` 这条 · 读线程 · 子进程的读线程，都从这条线程起、继承它的亲和）挤在一个核上，
/// 读线程多半在 `start` 醒来之前就看到它退了、收了作业；连起几份，每份都得这样。只钉判据自己起的那条线程，不动别的判据。
#[cfg(target_os = "linux")]
#[test]
fn a_link_then_an_instant_exit_still_starts() {
    std::thread::spawn(|| {
        // SAFETY: cpu_set_t 是纯位图，全零即空集；pid 0 ＝ 调用它的这条线程。
        unsafe {
            let mut one: libc::cpu_set_t = std::mem::zeroed();
            let cpu = usize::try_from(libc::sched_getcpu()).unwrap_or(0);
            libc::CPU_SET(cpu, &mut one);
            assert_eq!(
                libc::sched_setaffinity(0, std::mem::size_of::<libc::cpu_set_t>(), &one),
                0,
                "钉不上核"
            );
        }
        link_then_instant_exit();
    })
    .join()
    .unwrap_or_else(|e| std::panic::resume_unwind(e));
}

#[cfg(target_os = "linux")]
fn link_then_instant_exit() {
    let rig = Rig::new("instant", &format!("echo \"visit: {FAKE_URL}\"\nexit 0"));
    for i in 0..40 {
        let target: &'static str = Box::leak(format!("acct-instant-{i}").into_boxed_str());
        let (tx, rx) = std::sync::mpsc::channel::<(String, SignInEnd)>();
        let tx = Mutex::new(tx);
        let (mut a, dir) = rig.ask(target, conn());
        a.dir = Some(Box::leak(dir.into_boxed_str()));
        let s = start(
            &a,
            Box::new(move |job, e| {
                let _ = tx.lock().unwrap().send((job.to_string(), e.clone()));
            }),
        )
        .unwrap_or_else(|e| panic!("第 {i} 份：给了链接却回起不成：{e:?}"));
        assert_eq!(s.url, FAKE_URL);
        let got = rx
            .recv_timeout(guard_core::wall::CEILING)
            .expect("它没自己退");
        assert_eq!(got, (s.job.clone(), SignInEnd::Done), "第 {i} 份");
    }
}

/// 缺陷（10-19）：登录子命令照**起会话那个 shell** 的 `PATH` 找、子进程也拿那一份（npm 装的那一形要靠它找 `node`）；
/// 那一份上没有 ⇒ 起不成，说的是查过哪儿，不是「未装」（它可能装了、只是不在后端自己的 `PATH` 上）。
#[test]
fn the_login_runs_with_the_session_shell_path_and_says_where_it_looked() {
    let rig = Rig::new(
        "spath",
        &format!(
            "case \":$PATH:\" in *\":{}:\"*) ;; *) echo \"PATH=$PATH\" >&2; exit 3;; esac\necho \"visit: {FAKE_URL}\"\nsleep 5",
            rig_bin_marker()
        ),
    );
    let log: Log = Default::default();
    let (mut a, _) = rig.ask("acct-spath", conn());
    let session = format!(
        "{}:{}:/usr/bin:/bin",
        rig.root.join("bin").display(),
        rig_bin_marker()
    );
    a.path = Some(session.into());
    let s = start(&a, on_end(&log))
        .expect("起会话那个 shell 的 PATH 上有它 ⇒ 起得来，且子进程拿的是那一份");
    assert_eq!(s.url, FAKE_URL);
    cancel(&s.job);

    let not_installed = copy_core::copy_text("reason.spawn.notInstalled", &[]);
    let (mut a, _) = rig.ask("acct-spath-none", conn());
    a.path = Some(format!("{}:/usr/bin:/bin", rig.root.join("acct").display()).into());
    let e = start(&a, on_end(&log)).expect_err("那一份 PATH 上没有 ⇒ 起不成");
    assert!(e.said.contains("×3"), "要说查过几个目录：{e:?}");
    assert!(
        !e.said.contains(&not_installed),
        "不许说「{not_installed}」：{e:?}"
    );
}

/// 只出现在「起会话那个 shell」那一份 `PATH` 里的一个目录（不必存在）。
fn rig_bin_marker() -> &'static str {
    "/nonexistent/session-only"
}
