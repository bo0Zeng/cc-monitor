use super::*;

/// ★ 有期限命令时：它当**前缀**，秒数紧随其后，插件与参数原样跟在后面。
#[test]
fn the_deadline_command_becomes_the_prefix() {
    let t = PathBuf::from("/usr/bin/timeout");
    let bin = PathBuf::from("/opt/p/tool");
    let (prog, argv) = argv_for(&bin, &["--", "a b", "c"], 7, Some(&t));
    assert_eq!(prog, t, "程序应当是那条期限命令");
    assert_eq!(
        argv,
        vec![
            "7".to_string(),
            "/opt/p/tool".to_string(),
            "--".to_string(),
            "a b".to_string(),
            "c".to_string()
        ],
        "秒数没排在插件前面，或参数被改了形状"
    );
}

/// ★★ **降级那条路**：找不到期限命令就裸跑 —— 这一格此前只有一句头注。
///
/// 钉两件事：① 起的就是插件自己；② 秒数**一个字都不许出现在 argv 里**
///（不然会变成插件自己的第一个参数，那是货真价实的错传）。
#[test]
fn without_a_deadline_command_it_runs_bare_and_says_so_in_the_argv() {
    let bin = PathBuf::from("/opt/p/tool");
    let (prog, argv) = argv_for(&bin, &["x"], 7, None);
    assert_eq!(prog, bin, "裸跑时起的应当是插件自己");
    assert_eq!(argv, vec!["x".to_string()], "裸跑时不该有任何前缀参数");
    assert!(
        !argv.contains(&"7".to_string()),
        "秒数漏进了插件的 argv —— 那会被它当成一个真参数：{argv:?}"
    );
}

/// 没有参数的调用，两种形态都不该多出空串。
#[test]
fn an_empty_arg_list_stays_empty() {
    let bin = PathBuf::from("/opt/p/tool");
    let (_, bare) = argv_for(&bin, &[], 3, None);
    assert!(bare.is_empty(), "{bare:?}");
    let t = PathBuf::from("/usr/bin/timeout");
    let (_, pre) = argv_for(&bin, &[], 3, Some(&t));
    assert_eq!(pre, vec!["3".to_string(), "/opt/p/tool".to_string()]);
}

/// 「被信号打断」与「退出码是几」是**两件事**，骨架必须分得开。
#[test]
fn a_signal_death_is_not_an_exit_code() {
    let killed = Done {
        code: None,
        stdout: Vec::new(),
        stderr: Vec::new(),
    };
    assert!(!killed.timed_out(), "码是 None 不该被算成超时");
    let expired = Done {
        code: Some(TIMED_OUT_CODE),
        stdout: Vec::new(),
        stderr: Vec::new(),
    };
    assert!(expired.timed_out());
    let ok = Done {
        code: Some(0),
        stdout: Vec::new(),
        stderr: Vec::new(),
    };
    assert!(!ok.timed_out());
}

/// 诊断取法：**stderr 优先**，空了才退回 stdout；两条都空就是空串。
#[test]
fn the_diagnosis_prefers_stderr_and_falls_back_to_stdout() {
    let d = Done {
        code: Some(1),
        stdout: b"out-1\nout-2\n".to_vec(),
        stderr: b"\n  \nerr-1\nerr-2\n".to_vec(),
    };
    assert_eq!(d.diagnosis(), "err-1", "stderr 里的第一行非空内容没被取到");
    let d2 = Done {
        code: Some(1),
        stdout: b"out-1\n".to_vec(),
        stderr: b"   \n".to_vec(),
    };
    assert_eq!(d2.diagnosis(), "out-1", "stderr 全空白时没退回 stdout");
    let d3 = Done {
        code: Some(1),
        stdout: Vec::new(),
        stderr: Vec::new(),
    };
    assert_eq!(d3.diagnosis(), "");
}

/// 非 UTF-8 的输出不许让取诊断这一步炸掉。
#[test]
fn invalid_utf8_output_still_yields_a_line() {
    assert_eq!(first_line(&[0xff, 0xfe, b'\n', b'x']), "\u{fffd}\u{fffd}");
}

// ── 可打断的那一形：`run_abortable` ─────────────────────────────────────

/// 私有临时目录（进程号 ＋ 标签 ＋ 序号）。
#[cfg(unix)]
fn scratch(tag: &str) -> PathBuf {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static N: AtomicUsize = AtomicUsize::new(0);
    let p = std::env::temp_dir().join(format!(
        "ccm-be-invoke-{tag}-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    p
}

/// 一段 sh 写成的替身插件（可执行位置上）。
#[cfg(unix)]
fn script(dir: &Path, body: &str) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let f = dir.join("plugin-under-test");
    std::fs::write(&f, format!("#!/bin/sh\n{body}\n")).unwrap();
    std::fs::set_permissions(&f, std::fs::Permissions::from_mode(0o755)).unwrap();
    f
}

/// 这个 pid 还是一个活进程吗（僵尸不算活：它已经死了，只是还没被收尸）。
#[cfg(unix)]
fn alive(pid: u32) -> bool {
    match std::fs::read_to_string(format!("/proc/{pid}/stat")) {
        // 第三段是状态字母；进程名在括号里、可能带空格 ⇒ 从最后一个 `)` 之后切。
        Ok(s) => s
            .rsplit_once(')')
            .and_then(|(_, rest)| rest.split_whitespace().next())
            .is_some_and(|st| st != "Z" && st != "X"),
        Err(_) => false,
    }
}

/// ★**被丢 ⇒ 整组都没了**，不只是直接子进程。
///
/// 替身插件自己再起一个长睡的孙进程、把两个 pid 写进文件（**pid 从它写的文件里读** ——
/// 不是我们记下的那一个，异源），然后等着。宿主那一侧有 `timeout(1)` 前缀时，
/// 直接子进程是那条期限命令 ⇒ 只杀它（`kill_on_drop`）的话，插件与它的孙进程都成孤儿、照跑。
/// 断言：丢掉 future 之后，插件本身与孙进程**都**不再是活进程。
#[cfg(unix)]
#[test]
fn dropping_the_wait_kills_the_whole_group_not_just_the_direct_child() {
    let dir = scratch("group");
    let pids = dir.join("pids");
    let bin = script(
        &dir,
        &format!(
            "sleep 300 &\necho $! > '{p}.tmp'\necho $$ >> '{p}.tmp'\nmv '{p}.tmp' '{p}'\nwait",
            p = pids.display()
        ),
    );
    // 前提：这台机器上有 `timeout(1)` —— 否则本条只验得到「裸跑」那一形（直接子进程就是插件）。
    assert!(
        deadline_bin().is_some(),
        "这台机器的 PATH 上没有 `timeout` —— 本条要验的正是「有前缀时孙进程也得死」那一形，判不了"
    );
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap();
    let (grandchild, plugin) = rt.block_on(async {
        let b = bin.clone();
        let task = tokio::spawn(async move { run_abortable(&b, &[], 300, &[], 1024).await });
        // 等替身把两个 pid 写出来（它写完才 `mv`，读到就是完整的两行）。
        let mut got = None;
        for _ in 0..200 {
            if let Ok(s) = std::fs::read_to_string(&pids) {
                let v: Vec<u32> = s.lines().filter_map(|l| l.trim().parse().ok()).collect();
                if v.len() == 2 {
                    got = Some((v[0], v[1]));
                    break;
                }
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
        let (g, p) = got.expect("替身插件 10 秒内没写出两个 pid —— 它根本没起来，本条判不了");
        assert!(alive(g) && alive(p), "丢之前两个都该活着（正控）");
        task.abort();
        let joined = task.await;
        assert!(
            joined.as_ref().is_err_and(|e| e.is_cancelled()),
            "任务没有被撤掉：{:?}",
            joined.map(|r| r.is_ok())
        );
        (g, p)
    });
    // 信号是异步送达的：给它一点时间死透（被收尸由 init / 子收割者负责，僵尸已算死）。
    let mut dead = false;
    for _ in 0..100 {
        if !alive(grandchild) && !alive(plugin) {
            dead = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    // 收尾：不管判据成不成立，别把长睡进程留在机器上。
    for pid in [grandchild, plugin] {
        let _ = std::process::Command::new("kill")
            .args(["-9", &pid.to_string()])
            .stderr(std::process::Stdio::null())
            .status();
    }
    assert!(
        dead,
        "future 被丢掉 5 秒后，插件（{plugin} 活着={}）或它起的孙进程（{grandchild} 活着={}）还在跑 —— \
         只杀了直接子进程（那条期限命令），干活的那一组成了孤儿：取消等于没取消",
        alive(plugin),
        alive(grandchild)
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// ★**没被丢 ⇒ 与同步那一形同形同果**（码 · 两条流逐字节相等）。
#[cfg(unix)]
#[test]
fn an_undisturbed_abortable_run_ends_exactly_like_the_blocking_one() {
    let dir = scratch("same");
    let bin = script(
        &dir,
        "printf 'out:%s\\n' \"$@\"\nprintf 'err line\\n' >&2\nexit 5",
    );
    let args = ["a b", "--x"];
    let sync = match run(&bin, &args, 30, &[("K", "v")]) {
        Ok(d) => d,
        Err(_) => panic!("同步那一形没起来"),
    };
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let asynced = match rt.block_on(run_abortable(&bin, &args, 30, &[("K", "v")], 1024)) {
        Ok(d) => d,
        Err(_) => panic!("异步那一形没起来"),
    };
    assert_eq!(sync.code, Some(5), "正控：替身的退出码");
    assert_eq!(asynced.code, sync.code, "退出码不同");
    assert_eq!(asynced.stdout, sync.stdout, "stdout 不同");
    assert_eq!(asynced.stderr, sync.stderr, "stderr 不同");
    assert_eq!(sync.stdout, b"out:a b\nout:--x\n", "正控：argv 原样直传");
    // 找不到的插件：两形给同一个结局（有期限前缀时是那条期限命令报的码，裸跑时是 `NotRun`）。
    let missing = dir.join("no-such-plugin");
    let shape = |r: Result<Done, NotRun>| match r {
        Ok(d) => format!("done {:?}", d.code),
        Err(NotRun::Failed(_)) => "failed".to_string(),
        Err(NotRun::ArgListTooLong) => "too-long".to_string(),
    };
    assert_eq!(
        shape(rt.block_on(run_abortable(&missing, &[], 1, &[], 1024))),
        shape(run(&missing, &[], 1, &[]))
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// ★**每条流最多留 `keep ＋ 1` 字节，其余照读照丢**：多写的那个子进程照样正常退出
/// （不读的话它写满管道就卡住），调用方看 `len() > keep` 就知道超了。
#[cfg(unix)]
#[test]
fn an_oversized_stream_is_kept_to_one_past_the_cap_and_the_rest_is_drained() {
    let dir = scratch("keep");
    // 远超一个管道缓冲（64 KiB）：不照读照丢的话它会卡在写上。
    let bin = script(
        &dir,
        "head -c 300000 /dev/zero\nhead -c 300000 /dev/zero >&2\nexit 0",
    );
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let done = match rt.block_on(run_abortable(&bin, &[], 30, &[], 100)) {
        Ok(d) => d,
        Err(_) => panic!("没起来"),
    };
    assert_eq!(
        done.code,
        Some(0),
        "多写的那个子进程没能正常退出（被管道卡住了？）"
    );
    assert_eq!(done.stdout.len(), 101, "stdout 留的不是 keep＋1 字节");
    assert_eq!(done.stderr.len(), 101, "stderr 留的不是 keep＋1 字节");
    // 正控：没超的照原样全留。
    let small = match rt.block_on(run_abortable(&bin, &[], 30, &[], 1_000_000)) {
        Ok(d) => d,
        Err(_) => panic!("没起来"),
    };
    assert_eq!(small.stdout.len(), 300_000);
    let _ = std::fs::remove_dir_all(&dir);
}

/// 要求：「要上游给的」④「小程序写成进度行 → 插件口转订阅流」。
///
/// ★给了回调 ⇒ stderr 上**整行**的 `progress=` 行交回调（前缀后面那段、去行尾），不进诊断，别的照旧留；
/// 没写完的半行（没有换行）不算进度。没给回调 ⇒ 同一串字节原样全留（进度行就是普通 stderr）。
#[cfg(unix)]
#[test]
fn progress_lines_on_stderr_go_to_the_callback_and_stay_out_of_the_diagnosis() {
    let dir = scratch("progress");
    let bin = script(
        &dir,
        r#"printf 'progress={"n":1}\n' >&2
printf 'boom\n' >&2
printf 'progress={"n":2}\r\n' >&2
printf 'progress=half' >&2
printf 'out\n'
exit 3"#,
    );
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let mut got: Vec<String> = Vec::new();
    let mut on = |cell: &str| got.push(cell.to_string());
    let done = match rt.block_on(run_abortable_reporting(&bin, &[], 30, &[], 1024, &mut on)) {
        Ok(d) => d,
        Err(_) => panic!("没起来"),
    };
    assert_eq!(got, [r#"{"n":1}"#, r#"{"n":2}"#]);
    assert_eq!(done.code, Some(3));
    assert_eq!(String::from_utf8_lossy(&done.stderr), "boom\nprogress=half");
    assert_eq!(done.diagnosis(), "boom", "诊断被进度行抢了");
    assert_eq!(done.stdout, b"out\n");
    // 没给回调：一个字节都不分拣。
    let plain = match rt.block_on(run_abortable(&bin, &[], 30, &[], 1024)) {
        Ok(d) => d,
        Err(_) => panic!("没起来"),
    };
    assert_eq!(
        String::from_utf8_lossy(&plain.stderr),
        "progress={\"n\":1}\nboom\nprogress={\"n\":2}\r\nprogress=half"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
