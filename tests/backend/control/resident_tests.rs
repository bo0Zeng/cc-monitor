//! 〔HOST · V139〕远端常驻后端的起 · 找 · 停（`control/resident.rs`）。
//! 守的要求：`99 §1` V139「远端常驻、本机远端同形」；`INVARIANTS §48.1`「监听口要钥匙」（钥匙不进 env / argv）。

use super::*;

fn scratch(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("ccm-resident-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// H5：钥匙出生即只给本人、第二次读回同一把；子进程的环境里只有钥匙文件的**路径**，没有钥匙本身。
#[test]
fn the_token_is_private_stable_and_never_handed_through_the_environment() {
    let home = scratch("tok");
    let path = home.join(relay_route_core::LISTEN_TOKEN_FILE_REL);
    let t1 = ensure_token(&path).expect("铸不出钥匙");
    assert_eq!(t1.len(), 2 * TOKEN_BYTES);
    assert_eq!(
        ensure_token(&path).unwrap(),
        t1,
        "第二次没读回同一把 —— 已在跑的常驻后端会接不上"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "钥匙文件不是只给本人");
    }
    let env = child_env(
        49999,
        &path,
        &home,
        &[
            ("CCM_RELAY_PORT", "8788".into()),
            ("CCM_BACKEND_STDERR_LOG", format!("~/{STDERR_LOG_REL}")),
        ],
    );
    assert!(
        env.iter().all(|(_, v)| !v.contains(&t1)),
        "钥匙进了子进程的环境：{env:?}"
    );
    let names: Vec<&str> = env.iter().map(|(k, _)| k.as_str()).collect();
    assert_eq!(
        names,
        [
            crate::stream::listen::ENV_PORT,
            crate::stream::listen::ENV_TOKEN_FILE,
            "CCM_RELAY_PORT",
            "CCM_BACKEND_STDERR_LOG",
        ],
        "子进程环境那几格对不上（V139：中转口要交给它）"
    );
    assert_eq!(
        env[3].1,
        home.join(STDERR_LOG_REL).display().to_string(),
        "`~/` 没换成家目录"
    );
    let _ = std::fs::remove_dir_all(&home);
}

/// 停之前核身份：部署换过文件之后旧进程的 exe 带 ` (deleted)` 照样认；别的程序不认。
#[test]
fn the_owner_record_parses_and_the_identity_check_tolerates_a_replaced_file() {
    assert_eq!(
        parse_owner("123\n/h/.cc-monitor/bin/cc-monitor-backend\n"),
        Some((123, PathBuf::from("/h/.cc-monitor/bin/cc-monitor-backend")))
    );
    assert_eq!(parse_owner("0\n/x\n"), None);
    assert_eq!(parse_owner("123\n"), None);
    let rec = Path::new("/h/.cc-monitor/bin/cc-monitor-backend");
    assert!(exe_matches("/h/.cc-monitor/bin/cc-monitor-backend", rec));
    assert!(exe_matches(
        "/h/.cc-monitor/bin/cc-monitor-backend (deleted)",
        rec
    ));
    assert!(!exe_matches("/usr/bin/python3", rec));
}

fn env_of<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
    move |k: &str| {
        pairs
            .iter()
            .find(|(n, _)| *n == k)
            .map(|(_, v)| (*v).to_string())
    }
}

/// 〔HOST〕远端 `--resident-ensure` 那一形：钥匙在文件里（env 只有路径）⇒ 读出来就是常驻；文件空 / 读不动 ⇒ 拒（fail closed）。
#[test]
fn a_token_file_gives_listen_mode_and_an_empty_one_is_refused() {
    let dir = std::env::temp_dir().join(format!("ccm-listen-tokfile-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let f = dir.join("listen-token");
    std::fs::write(&f, "abc123\n").unwrap();
    let path = f.to_string_lossy().into_owned();
    let m = crate::stream::listen::mode_from(&env_of(&[
        (crate::stream::listen::ENV_PORT, "51000"),
        (crate::stream::listen::ENV_TOKEN_FILE, &path),
    ]))
    .unwrap();
    assert_eq!(
        crate::stream::listen::resolve(m).unwrap(),
        Some((51000, "abc123".to_string()))
    );
    std::fs::write(&f, " \n").unwrap();
    let m = crate::stream::listen::mode_from(&env_of(&[
        (crate::stream::listen::ENV_PORT, "51000"),
        (crate::stream::listen::ENV_TOKEN_FILE, &path),
    ]))
    .unwrap();
    assert!(
        crate::stream::listen::resolve(m).is_err(),
        "空钥匙文件也起了一个口"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// 〔TAIL · HOST 余项〕远端起的常驻后端也带上数据目录那两格（与那台自己的 monitor 交的同一对值）⇒ 那台 monitor 收养它、HX2 不拒。
/// 守的要求：主会话裁 HOST 待拍 1「后端自己按默认推出这两格路径（谁起都一样）」（`_施工/paused-state.md`）。期望路径手写。
#[test]
fn the_resident_derives_the_same_data_dir_pair_whoever_starts_it() {
    let names = ("CCM_APIKEY_CREDENTIALS", "CCM_HISTORY_METADATA");
    let got = data_dir_envs(&env_of(&[]), Path::new("/h"), names.0, names.1);
    assert_eq!(
        got,
        vec![
            (
                names.0,
                "/h/.claude/work/apikey-credentials.json".to_string()
            ),
            (
                names.1,
                "/h/.claude/work/history-metadata.json".to_string()
            ),
        ]
    );
    // `CCM_DATA_DIR` 跟着走；本进程已有的那一格不覆盖；相对路径 ⇒ 两格都缺席（不退回真 profile）。
    let iso = data_dir_envs(
        &env_of(&[("CCM_DATA_DIR", "/iso"), (names.0, "/x/c.json")]),
        Path::new("/h"),
        names.0,
        names.1,
    );
    assert_eq!(
        iso,
        vec![(names.1, "/iso/history-metadata.json".to_string())]
    );
    assert!(data_dir_envs(
        &env_of(&[("CCM_DATA_DIR", "rel")]),
        Path::new("/h"),
        names.0,
        names.1
    )
    .is_empty());
}

/// ★ 〔GAP1 · `设计/15 §4.7 S1`〕远端常驻后端的诊断文件落 `~/.cc-monitor/logs/backend/stderr.log`（与本机同一层级），
/// 那几层目录由起它的那一步逐层建好（`stderr_log` 只建文件不建目录），每层只给本人。
#[test]
fn the_resident_log_lives_under_logs_backend_and_its_dirs_are_made() {
    assert_eq!(STDERR_LOG_REL, ".cc-monitor/logs/backend/stderr.log");
    let home = std::env::temp_dir().join(format!("ccm-gap1-logdir-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(&home).unwrap();
    log_dir_chain(&home).expect("逐层建");
    for rel in [
        ".cc-monitor",
        ".cc-monitor/logs",
        ".cc-monitor/logs/backend",
    ] {
        let d = home.join(rel);
        assert!(d.is_dir(), "{rel} 没建出来");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            let mode = std::fs::metadata(&d).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o700, "{rel} 不是只给本人");
        }
    }
    let _ = std::fs::remove_dir_all(&home);
}

/// 〔STOP〕T1 **宽限期 > 退出排空上限**：默认值与 `--grace` 的下限两格都比 `inbound::DRAIN_DEADLINE` 长。
/// 守的要求：`4d-lanes.md` `### STOP`（主会话裁）逐字「常驻后端自己收 SIGTERM 排空在飞写的上限（HX1）必须 < 宽限期（判据钉两者关系）」。
/// 等得比排空短 ⇒ 后端那句「哪几条没做完」永远被 SIGKILL 截断，而且不会报错。
#[test]
fn the_grace_period_outlasts_the_backend_drain_cap() {
    let drain_ms = crate::stream::inbound::DRAIN_DEADLINE.as_millis();
    assert!(
        u128::from(STOP_GRACE_MS) > drain_ms,
        "默认宽限期 {STOP_GRACE_MS}ms 不比排空上限 {drain_ms}ms 长"
    );
    let drain_secs = crate::stream::inbound::DRAIN_DEADLINE.as_secs();
    let args = |v: &str| vec!["--grace".to_string(), v.to_string()];
    assert_eq!(parse_grace(&[]), Ok(STOP_GRACE_MS));
    assert!(
        parse_grace(&args(&drain_secs.to_string())).is_err(),
        "等于排空上限的宽限期被收下了"
    );
    assert_eq!(
        parse_grace(&args(&(drain_secs + 1).to_string())),
        Ok(u32::try_from((drain_secs + 1) * 1000).unwrap())
    );
    for bad in ["", "x", "-5", "3601"] {
        assert!(parse_grace(&args(bad)).is_err(), "{bad:?}");
    }
    assert!(parse_grace(&["--grace".to_string()]).is_err(), "缺值");
}

/// 起一个 `sh`，`trap` 决定它怎么对 SIGTERM；装好 trap 才往 stdout 写一行（事件，不是睡）。
#[cfg(target_os = "linux")]
fn sh_child(script: &str) -> std::process::Child {
    use std::io::BufRead as _;
    let mut c = std::process::Command::new("sh")
        .arg("-c")
        .arg(script)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("起 sh");
    let mut line = String::new();
    std::io::BufReader::new(c.stdout.take().expect("stdout"))
        .read_line(&mut line)
        .expect("读就绪行");
    assert_eq!(line.trim(), "ready");
    c
}

/// 判据红了（panic）也收掉那个 `sh`（纪律 21）。
#[cfg(target_os = "linux")]
struct Reap(std::process::Child);

#[cfg(target_os = "linux")]
impl Drop for Reap {
    fn drop(&mut self) {
        if matches!(self.0.try_wait(), Ok(None)) {
            let _ = self.0.kill();
        }
        let _ = self.0.wait();
    }
}

/// 〔STOP〕T2 **同机监督者三态，真进程 ＋ 真信号**：听话的（收 SIGTERM 就退）⇒ `graceful`；聋的（忽略 SIGTERM）⇒ 宽限期满强杀、
/// `killed`、它真没了（收尸后 `/proc` 里没有）；不在的 ⇒ `not_running`；记下的程序对不上 ⇒ 拒、不发信号。
/// 守的要求：`4d-lanes.md` `### STOP` 逐字「读 pid → SIGTERM → 在本机按 pidfd 等到退出或宽限期到 → 到点 SIGKILL → 回结局」。
#[test]
#[cfg(target_os = "linux")]
fn the_one_shot_supervisor_stops_politely_then_by_force() {
    let exe = |pid: u32| PathBuf::from(crate::platform::proc::exe_of(pid).expect("exe"));
    // 听话的。
    let mut c = Reap(sh_child(
        "trap 'exit 0' TERM; echo ready; while :; do sleep 0.05; done",
    ));
    let pid = c.0.id();
    assert_eq!(
        stop_pid(pid, &exe(pid), 5_000, 1_000),
        Ok(Stopped::Graceful(pid))
    );
    assert_eq!(
        c.0.wait().expect("wait").code(),
        Some(0),
        "它自己给的退出码"
    );
    // 聋的：宽限期 200ms 满 ⇒ 强杀。
    let mut c = Reap(sh_child(
        "trap '' TERM; echo ready; while :; do sleep 0.05; done",
    ));
    let pid = c.0.id();
    assert_eq!(
        stop_pid(pid, &exe(pid), 200, 2_000),
        Ok(Stopped::Killed(pid))
    );
    use std::os::unix::process::ExitStatusExt as _;
    assert_eq!(
        c.0.wait().expect("wait").signal(),
        Some(9),
        "应当是被强杀的"
    );
    assert!(
        !std::path::Path::new(&format!("/proc/{pid}")).exists(),
        "进程表里还有 {pid}"
    );
    // 已经不在的（刚收过尸的那个 pid）⇒ not_running。
    assert_eq!(
        stop_pid(pid, &exe(std::process::id()), 200, 200),
        Ok(Stopped::NotRunning)
    );
    // 记下的程序对不上 ⇒ 拒，且它照样活着（没发任何信号）。
    let mut c = Reap(sh_child(
        "trap 'exit 0' TERM; echo ready; while :; do sleep 0.05; done",
    ));
    let pid = c.0.id();
    let err = stop_pid(pid, Path::new("/nonexistent/cc-monitor-backend"), 200, 200).unwrap_err();
    assert!(err.contains(&pid.to_string()), "{err}");
    assert!(matches!(c.0.try_wait(), Ok(None)), "对不上身份的也被停了");
    // 线上三个词。
    let words: Vec<&str> = [
        Stopped::Graceful(1),
        Stopped::Killed(1),
        Stopped::NotRunning,
    ]
    .iter()
    .map(|s| s.word())
    .collect();
    assert_eq!(words, ["graceful", "killed", "not_running"]);
}
