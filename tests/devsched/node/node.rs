//! 编译节点端到端：在私有临时目录（`/tmp/dn<4 位>`）当根起节点二进制，经标准输入 / 输出收发帧。
//! 作业都是 `sh -c 'sleep …'` 这类小东西；测完按 PID 收干净（节点 · 作业印出来的进程号）。

include!("common.rs");
#[test]
fn job_finds_root_toolchain_programs_by_name() {
    let r = TestRoot::new();
    let bin = r.0.join("toolchains/cargo/bin");
    std::fs::create_dir_all(&bin).unwrap();
    let tool = bin.join("dsn-fake-tool");
    write_exe(&tool, "#!/bin/sh\necho fake-tool-ran\n");
    let mut n = Node::start(&r.0, &[]);
    n.hello();
    n.job(1, "dsn-fake-tool");
    let (out, e) = n.collect(1, WALL_CEILING);
    assert_eq!(
        (e.stop, e.code),
        (StopReason::Done, Some(0)),
        "根下工具链的程序按名字找得到：{out}"
    );
    assert_eq!(out.trim(), "fake-tool-ran");
}

#[test]
fn job_env_is_frame_env_plus_node_vars_only() {
    let r = TestRoot::new();
    let mut n = Node::start(&r.0, &[]);
    n.hello();
    n.conn
        .send(&Frame::Job(devsched_wire::Job {
            job: 1,
            project: "p".into(),
            tree: "w".into(),
            rel: String::new(),
            argv: vec![
                "sh".into(),
                "-c".into(),
                "echo ${DSN_NODE_ONLY-unset} ${FROM_FRAME-unset}".into(),
            ],
            env: [("FROM_FRAME".to_string(), "frame".to_string())]
                .into_iter()
                .collect(),
            stop: Default::default(),
            nice: None,
        }))
        .unwrap();
    let (out, e) = n.collect(1, WALL_CEILING);
    assert_eq!(e.stop, StopReason::Done);
    assert_eq!(out.trim(), "unset frame", "节点自己的环境不带给作业");
}

#[test]
fn job_output_exit_code_and_skipped() {
    let r = TestRoot::new();
    let mut n = Node::start(&r.0, &[]);
    let h = n.hello();
    assert_eq!(h.proto, devsched_wire::proto());
    assert_eq!(Path::new(&h.root), r.0.as_path());
    n.job(1, "echo hi; echo oops >&2; echo \"$TMPDIR\"; echo mobile >> \"$DEVSCHED_SKIPPED\"; echo windows >> \"$DEVSCHED_SKIPPED\"; exit 3");
    let (out, e) = n.collect(1, WALL_CEILING);
    let mut lines = out.lines();
    assert_eq!(lines.next(), Some("hi"));
    let tmp = lines.next().unwrap();
    assert!(
        Path::new(tmp).starts_with(r.0.join("t")),
        "TMPDIR 在 <根>/t 下：{tmp}"
    );
    assert!(!Path::new(tmp).exists(), "收尾删掉这趟的临时目录");
    assert_eq!(
        (e.code, e.stop, e.skipped.clone()),
        (
            Some(3),
            StopReason::Done,
            vec!["mobile".to_string(), "windows".to_string()]
        )
    );
    assert!(zombies_of(n.pid()).is_empty(), "节点名下不留僵尸");
}

#[test]
fn stop_frame_stops_whole_group() {
    let r = TestRoot::new();
    let mut n = Node::start(&r.0, &[]);
    n.hello();
    n.job(5, "sleep 300 & echo $!; sleep 300 & echo $!; wait");
    let kids = n.read_pids(5, 2);
    n.conn
        .send(&Frame::Stop(devsched_wire::Stop { job: 5 }))
        .unwrap();
    let (_, e) = n.collect(5, WALL_CEILING);
    assert_eq!((e.stop, e.code), (StopReason::Stopped, None));
    assert!(wait_gone(&kids, WALL_CEILING), "整组停掉");
    assert!(zombies_of(n.pid()).is_empty());
}

#[test]
fn stop_line_breach_after_two_hits() {
    let r = TestRoot::new();
    let mut n = Node::start(&r.0, &[]);
    n.hello();
    n.conn
        .send(&Frame::Job(devsched_wire::Job {
            job: 2,
            project: "p".into(),
            tree: "w".into(),
            rel: String::new(),
            argv: vec!["sh".into(), "-c".into(), "sleep 300 & echo $!; wait".into()],
            env: BTreeMap::new(),
            stop: devsched_wire::StopLines {
                mem: Some(u64::MAX / (1 << 20)),
                psi: None,
                disk: None,
            },
            nice: None,
        }))
        .unwrap();
    let kids = n.read_pids(2, 1);
    let t = Instant::now();
    let (_, e) = n.collect(2, WALL_CEILING);
    assert!(t.elapsed() >= Duration::from_secs(1), "一次撞线不该就停");
    assert_eq!(
        (e.stop, e.breach),
        (StopReason::Breach, Some(devsched_wire::Breach::Mem))
    );
    assert!(wait_gone(&kids, WALL_CEILING));
}

#[test]
fn killing_the_node_takes_the_job_group_with_it() {
    let r = TestRoot::new();
    let mut n = Node::start(&r.0, &[]);
    n.hello();
    n.job(1, "sleep 300 & echo $!; sleep 300 & echo $!; wait");
    let kids = n.read_pids(1, 2);
    unsafe { libc::kill(n.pid(), libc::SIGKILL) };
    assert!(n.wait_exit(WALL_CEILING).is_some());
    assert!(wait_gone(&kids, WALL_CEILING), "节点被杀，作业组跟着没");
}

#[test]
fn startup_clears_last_residue() {
    let r = TestRoot::new();
    // 上次留下的：一组还活着的进程（记录在 state/jobs/）与 t/ 下的临时目录
    std::fs::create_dir_all(r.0.join("state/jobs")).unwrap();
    std::fs::create_dir_all(r.0.join("t/zz99/deep")).unwrap();
    // 不是我们记着的（共用这个根的别的东西正在用）：不许删
    std::fs::create_dir_all(r.0.join("t/othr/in-use")).unwrap();
    let tag = r.0.join("t/zz99/skipped");
    let mut c = Command::new("sh");
    c.arg("-c")
        .arg("sleep 300 & echo $!; wait")
        .env("DEVSCHED_SKIPPED", &tag)
        .stdout(Stdio::piped())
        .stdin(Stdio::null());
    unsafe {
        use std::os::unix::process::CommandExt;
        c.pre_exec(|| {
            libc::setsid();
            Ok(())
        });
    }
    let mut leader = c.spawn().unwrap();
    let mut line = String::new();
    std::io::BufRead::read_line(
        &mut std::io::BufReader::new(leader.stdout.take().unwrap()),
        &mut line,
    )
    .unwrap();
    let sleeper: i32 = line.trim().parse().unwrap();
    let lp = leader.id() as i32;
    procs::write_record(&r.0.join("state/jobs"), "zz99", lp, tag.as_os_str()).unwrap();

    let mut n = Node::start(&r.0, &[]);
    n.pids.extend([lp, sleeper]);
    n.hello();
    let cleared = wait_gone(&[lp, sleeper], WALL_CEILING);
    let _ = leader.kill();
    let _ = leader.wait();
    assert!(cleared, "上次的组该停掉");
    assert!(!r.0.join("t/zz99").exists(), "t/ 下上次的临时目录该删掉");
    assert!(
        r.0.join("t/othr/in-use").is_dir(),
        "没记录的不是我们的，不删"
    );
    assert_eq!(entries_of(&r.0.join("state/jobs")).len(), 0);
}

#[test]
fn second_node_does_not_run_alongside_but_forwards() {
    let r = TestRoot::new();
    let mut a = Node::start(&r.0, &[]);
    a.hello();
    let mut b = Node::start(&r.0, &[]);
    let hb = b.hello();
    assert_eq!(Path::new(&hb.root), r.0.as_path());
    // 经 b 交的作业由 a 起：垫片（组长）的父进程是 a
    b.job(1, "echo $PPID; sleep 300 & echo $!; wait");
    let v = b.read_pids(1, 2);
    let shim = v[0];
    assert_eq!(
        procs::stat(shim).map(|s| s.ppid),
        Some(a.pid()),
        "作业该由主节点起"
    );
    // b 读到结尾 ⇒ 主节点停掉 b 那条连接上的作业，b 自己退出；a 照常
    drop(b.child.stdin.take());
    b.conn.close();
    assert!(b.wait_exit(WALL_CEILING).is_some(), "转发的那一个该退出");
    assert!(wait_gone(&v, WALL_CEILING));
    assert!(a.child.try_wait().unwrap().is_none(), "主节点照常活着");
}

#[test]
fn heartbeat_loss_stops_jobs_and_node_exits() {
    let r = TestRoot::new();
    let mut n = Node::start(&r.0, &["--heartbeat-timeout", "2"]);
    n.hello();
    n.job(1, "sleep 300 & echo $!; wait");
    let kids = n.read_pids(1, 1);
    // 不发心跳
    let (_, e) = n.collect(1, WALL_CEILING);
    assert_eq!(e.stop, StopReason::Heartbeat);
    assert!(wait_gone(&kids, WALL_CEILING));
    assert!(n.wait_exit(WALL_CEILING).is_some(), "心跳断了节点自己退出");
}

#[test]
fn stdin_eof_stops_jobs_and_node_exits() {
    let r = TestRoot::new();
    let mut n = Node::start(&r.0, &["--heartbeat-timeout", "2"]);
    n.hello();
    n.job(1, "sleep 300 & echo $!; wait");
    let kids = n.read_pids(1, 1);
    n.conn.close();
    assert!(wait_gone(&kids, WALL_CEILING), "标准输入到结尾 ⇒ 停作业");
    let st = n.wait_exit(WALL_CEILING);
    assert!(st.is_some(), "标准输入到结尾 ⇒ 节点退出");
}

#[test]
fn beat_is_answered_with_facts() {
    let r = TestRoot::new();
    let mut n = Node::start(&r.0, &[]);
    n.hello();
    n.until(WALL_CEILING, |f| matches!(f, Frame::Facts(_)));
    n.conn.send(&Frame::Beat).unwrap();
    match n.until(WALL_CEILING, |f| matches!(f, Frame::Facts(_))) {
        Frame::Facts(f) => {
            assert!(f.threads.is_some() && f.mem.is_some() && f.ours.is_some_and(|o| o >= 1));
        }
        _ => unreachable!(),
    }
}

#[test]
fn sccache_server_outlives_jobs_and_goes_with_the_node() {
    let r = TestRoot::new();
    let bin = r.0.join("toolchains/cargo/bin");
    std::fs::create_dir_all(&bin).unwrap();
    // 假的 sccache：记下自己的进程号与收到的端口，一直睡（真的 server 前台跑也是这样）
    let fake = bin.join("sccache");
    write_exe(
        &fake,
        "#!/bin/sh\nmkdir -p \"$SCCACHE_DIR\"\necho \"$$ $SCCACHE_SERVER_PORT $SCCACHE_START_SERVER\" > \"$SCCACHE_DIR/fake.tmp\"\nmv \"$SCCACHE_DIR/fake.tmp\" \"$SCCACHE_DIR/fake\"\nexec sleep 300\n",
    );
    let mut n = Node::start(&r.0, &["--heartbeat-timeout", "3"]);
    n.hello();
    let script = "f=\"$SCCACHE_DIR/fake\"; i=0; while [ ! -s \"$f\" ] && [ $i -lt 100 ]; do sleep 0.1; i=$((i+1)); done; echo \"${SCCACHE_SERVER_PORT-unset} $(cat \"$f\" 2>/dev/null)\"";
    n.job(1, script);
    let (out1, e1) = n.collect(1, WALL_CEILING);
    assert_eq!(e1.stop, StopReason::Done, "{out1}");
    let f1: Vec<&str> = out1.split_whitespace().collect();
    assert_eq!(f1.len(), 4, "作业看得到端口、server 起过：{out1}");
    let server: i32 = f1[1].parse().unwrap();
    n.pids.push(server);
    assert_eq!(f1[0], f1[2], "作业连的端口就是 server 的：{out1}");
    assert_eq!(f1[3], "1", "前台起 server：{out1}");
    assert!(!gone(server), "第一趟收尾没把 server 扫掉");
    n.job(2, script);
    let (out2, e2) = n.collect(2, WALL_CEILING);
    assert_eq!(e2.stop, StopReason::Done, "{out2}");
    assert_eq!(out1, out2, "第二趟连的是同一个 server");
    n.conn.close();
    assert!(
        n.wait_exit(WALL_CEILING).is_some(),
        "标准输入到结尾 ⇒ 节点退出"
    );
    assert!(wait_gone(&[server], WALL_CEILING), "节点退出 ⇒ server 停掉");
}

#[test]
fn command_killed_by_signal_exits_128_plus_signal() {
    let r = TestRoot::new();
    let mut n = Node::start(&r.0, &[]);
    n.hello();
    n.job(1, "kill -SEGV $$");
    let (_, e) = n.collect(1, WALL_CEILING);
    assert_eq!(
        (e.stop, e.code),
        (StopReason::Done, Some(139)),
        "跟本机一样折成 128 ＋ 信号"
    );
}

#[test]
fn sccache_server_is_under_the_thread_line_and_counted_as_ours() {
    let r = TestRoot::new();
    let bin = r.0.join("toolchains/cargo/bin");
    std::fs::create_dir_all(&bin).unwrap();
    // 假的 sccache：自己再起两个（像 server 起的编译器），记下进程号
    write_exe(
        &bin.join("sccache"),
        "#!/bin/sh\nmkdir -p \"$SCCACHE_DIR\"\nsleep 300 & a=$!\nsleep 300 & b=$!\necho \"$$ $a $b\" > \"$SCCACHE_DIR/fake.tmp\"\nmv \"$SCCACHE_DIR/fake.tmp\" \"$SCCACHE_DIR/fake\"\nwait\n",
    );
    let mut n = Node::start(&r.0, &["--nproc-limit", "100000", "--user-margin", "80"]);
    n.hello();
    // hello 之后那一份 facts 收掉（那时 server 的两个子进程可能还没起），下面等的才是 beat 的回话
    n.until(WALL_CEILING, |f| matches!(f, Frame::Facts(_)));
    let f = r.0.join("cache/sccache/fake");
    let t = Instant::now();
    while !f.exists() && t.elapsed() < WALL_CEILING {
        std::thread::sleep(Duration::from_millis(50));
    }
    let pids: Vec<i32> = std::fs::read_to_string(&f)
        .unwrap_or_else(|e| {
            panic!(
                "节点没起假 sccache（{e}）；节点日志：\n{}",
                std::fs::read_to_string(r.0.join("log/node.log")).unwrap_or_default()
            )
        })
        .split_whitespace()
        .map(|x| x.parse().unwrap())
        .collect();
    n.pids.extend(&pids);
    let limits = std::fs::read_to_string(format!("/proc/{}/limits", pids[0])).unwrap();
    let soft: Option<u64> = limits
        .lines()
        .find(|l| l.starts_with("Max processes"))
        .and_then(|l| l.split_whitespace().nth(2))
        .and_then(|x| x.parse().ok());
    let line = devsched_node::limits::Limits {
        cap: Some(100000),
        margin: 80,
        max_procs: 256,
    }
    .line(limits::rlimit_nproc());
    assert_eq!(soft, Some(line), "server 的线程软限压在线上：{limits}");
    n.conn.send(&Frame::Beat).unwrap();
    let ours = match n.until(WALL_CEILING, |f| matches!(f, Frame::Facts(_))) {
        Frame::Facts(f) => f.ours.unwrap_or(0),
        _ => unreachable!(),
    };
    let me = procs::stat(n.pid()).map(|s| s.threads).unwrap_or(1);
    assert!(
        ours >= me + 3,
        "server 那一组（它自己 ＋ 两个）算进我们的：ours = {ours}，节点自己 {me}"
    );
}

#[test]
fn node_exits_right_after_stdin_closes_when_idle() {
    let r = TestRoot::new();
    // 心跳时限给得比兜底期限还长：节点要是等心跳时限才退，就过了兜底期限、判红
    let mut n = Node::start(&r.0, &["--heartbeat-timeout", "100000"]);
    n.hello();
    n.job(1, "true");
    let (_, e) = n.collect(1, WALL_CEILING);
    assert_eq!(e.stop, StopReason::Done);
    n.conn.close();
    // 没连接没作业就当场退，ssh 跟着收，不用调度端去杀
    assert!(
        n.wait_exit(WALL_CEILING).is_some(),
        "关了输入没当场退（在等心跳时限）"
    );
}

#[test]
fn mem_breach_takes_the_sccache_server_down_and_next_job_gets_a_new_one() {
    let r = TestRoot::new();
    let bin = r.0.join("toolchains/cargo/bin");
    std::fs::create_dir_all(&bin).unwrap();
    write_exe(
        &bin.join("sccache"),
        "#!/bin/sh\nmkdir -p \"$SCCACHE_DIR\"\necho \"$$\" > \"$SCCACHE_DIR/fake.tmp\"\nmv \"$SCCACHE_DIR/fake.tmp\" \"$SCCACHE_DIR/fake\"\nexec sleep 300\n",
    );
    let mut n = Node::start(&r.0, &[]);
    n.hello();
    let f = r.0.join("cache/sccache/fake");
    let read_pid = |f: &Path| -> i32 {
        let t = Instant::now();
        while t.elapsed() < WALL_CEILING {
            if let Ok(s) = std::fs::read_to_string(f) {
                if let Ok(p) = s.trim().parse() {
                    return p;
                }
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        panic!("假 sccache 没起来");
    };
    let first = read_pid(&f);
    n.pids.push(first);
    // 一趟带必撞的内存停线
    n.conn
        .send(&Frame::Job(devsched_wire::Job {
            job: 1,
            project: "p".into(),
            tree: "w".into(),
            rel: String::new(),
            argv: vec!["sh".into(), "-c".into(), "sleep 30".into()],
            env: BTreeMap::new(),
            stop: devsched_wire::StopLines {
                mem: Some(u64::MAX / (1 << 21)),
                psi: None,
                disk: None,
            },
            nice: None,
        }))
        .unwrap();
    let (_, e) = n.collect(1, WALL_CEILING);
    assert_eq!(e.stop, StopReason::Breach, "{e:?}");
    assert!(
        wait_gone(&[first], WALL_CEILING),
        "撞内存停线 ⇒ server 那一组一起停"
    );
    let _ = std::fs::remove_file(&f);
    n.job(2, "echo ${SCCACHE_SERVER_PORT-unset}");
    let (out, e2) = n.collect(2, WALL_CEILING);
    assert_eq!(e2.stop, StopReason::Done);
    assert_ne!(out.trim(), "unset", "下一趟又有 server 可连");
    let second = read_pid(&f);
    n.pids.push(second);
    assert_ne!(first, second, "是新起的一份");
}

#[test]
fn second_node_started_as_first_is_leaving_still_gets_served() {
    let r = TestRoot::new();
    for i in 0..10 {
        let mut a = Node::start(&r.0, &[]);
        a.hello();
        // 主节点关了输入、正要退的同时，第二趟的节点起来
        a.conn.close();
        let mut b = Node::start(&r.0, &[]);
        b.hello();
        b.job(1, "echo hi");
        let (out, e) = b.collect(1, WALL_CEILING);
        assert_eq!((e.stop, out.trim()), (StopReason::Done, "hi"), "第 {i} 次");
        b.conn.close();
        assert!(a.wait_exit(WALL_CEILING).is_some());
        assert!(b.wait_exit(WALL_CEILING).is_some());
    }
}

#[test]
fn hello_reports_cargo_config_above_the_root() {
    let r = TestRoot::new();
    // 根放在一个带 .cargo/config.toml 的目录底下（像根在家目录下、家目录里有 cargo 配置）
    std::fs::create_dir_all(r.0.join(".cargo")).unwrap();
    std::fs::write(
        r.0.join(".cargo/config.toml"),
        "[build]\nincremental = false\n",
    )
    .unwrap();
    let root = r.0.join("rt");
    std::fs::create_dir_all(&root).unwrap();
    let mut n = Node::start(&root, &[]);
    let h = n.hello();
    let want = r.0.join(".cargo/config.toml").display().to_string();
    assert!(
        h.cargo_config.contains(&want),
        "hello 报出会生效的 cargo 配置：{:?}",
        h.cargo_config
    );
}

/// 假 srun：记下自己拿到的参数（一趟一行），跳过 srun 自己的参数、exec 后面那条命令（头一个绝对路径起）。
fn fake_srun(dir: &Path) -> (PathBuf, String) {
    let bin = dir.join("fakebin");
    std::fs::create_dir_all(&bin).unwrap();
    let log = dir.join("srun.log");
    let s = bin.join("srun");
    write_exe(
        &s,
        &format!(
            "#!/bin/sh\necho \"$*\" >> '{}'\nwhile [ $# -gt 0 ] && [ \"${{1#/}}\" = \"$1\" ]; do shift; done\nexec \"$@\"\n",
            log.display()
        ),
    );
    let path = format!(
        "{}:{}",
        bin.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    (log, path)
}

#[test]
fn slurm_main_node_execs_itself_under_srun_and_forwarders_do_not() {
    let r = TestRoot::new();
    let side = TestRoot::new();
    let (log, path) = fake_srun(&side.0);
    let env = [("PATH", path)];
    let mut a = Node::start_env(&r.0, &["--slurm", "-p,cpu,-c,4"], &env);
    a.hello();
    a.job(1, "echo ran-a");
    let (out, e) = a.collect(1, WALL_CEILING);
    assert_eq!(
        (e.stop, e.code, out.trim()),
        (StopReason::Done, Some(0), "ran-a")
    );
    let exe = std::fs::canonicalize(BIN).unwrap();
    assert_eq!(
        std::fs::read_to_string(&log).unwrap(),
        format!(
            "--ntasks=1 -p cpu -c 4 {} --root {}\n",
            exe.display(),
            r.0.display()
        ),
        "主节点经 srun 起、只一个任务（多起一份就是两个节点抢一条连接）、srun 后面是去掉 --slurm 的原参数"
    );
    // 主节点还在 ⇒ 第二个只当转发，不再调 srun
    let mut b = Node::start_env(&r.0, &["--slurm", "-p,cpu,-c,4"], &env);
    b.hello();
    b.job(2, "echo ran-b");
    let (out, e) = b.collect(2, WALL_CEILING);
    assert_eq!(
        (e.stop, e.code, out.trim()),
        (StopReason::Done, Some(0), "ran-b")
    );
    assert_eq!(
        std::fs::read_to_string(&log).unwrap().lines().count(),
        1,
        "转发不另起 srun"
    );
}

#[test]
fn clean_removes_the_tree_but_not_while_a_job_runs_in_it() {
    let r = TestRoot::new();
    let tree = r.0.join("projects/p/w");
    std::fs::write(tree.join("a.rs"), "x").unwrap();
    std::fs::create_dir_all(tree.join(".build/debug")).unwrap();
    let mut n = Node::start(&r.0, &[]);
    n.hello();
    n.job(1, "sleep 300 & echo $!; wait");
    n.read_pids(1, 1);
    let clean = Frame::Clean(devsched_wire::Clean {
        project: "p".into(),
        tree: "w".into(),
    });
    n.conn.send(&clean).unwrap();
    match n.until(WALL_CEILING, |f| matches!(f, Frame::Error(_))) {
        Frame::Error(e) => assert!(e.msg.contains("有作业在跑"), "{}", e.msg),
        _ => unreachable!(),
    }
    assert!(tree.join("a.rs").exists(), "有作业在那棵树里跑 ⇒ 不删");
    n.conn
        .send(&Frame::Stop(devsched_wire::Stop { job: 1 }))
        .unwrap();
    let (_, e) = n.collect(1, WALL_CEILING);
    assert_eq!(e.stop, StopReason::Stopped);
    n.conn.send(&clean).unwrap();
    // 帧照收到的先后处理：清理之后那一下心跳回了 facts ⇒ 清理已经做完
    n.conn.send(&Frame::Beat).unwrap();
    n.until(WALL_CEILING, |f| matches!(f, Frame::Facts(_)));
    assert!(!tree.exists(), "工作树连树里的产物目录一起删掉");
}

/// clean 当场只改名（另起线程删；删得慢不慢这里判不出）：回心跳那一下树已经不在原处，改了名的那份随后删光；
/// 上回没删完留下的 `.del-*`，节点起来就删。
#[test]
fn clean_moves_the_tree_aside_at_once_and_leaves_no_trash() {
    let r = TestRoot::new();
    let proj = r.0.join("projects/p");
    std::fs::create_dir_all(proj.join(".del-x.1/deep")).unwrap();
    for i in 0..200 {
        std::fs::write(proj.join(format!("w/f{i}")), "x").unwrap();
    }
    let mut n = Node::start(&r.0, &[]);
    n.hello();
    // hello 之后节点先发一次 facts：收掉它，下面等到的 facts 才是 beat 的回话（帧照先后处理，那时 clean 已做完）
    n.until(WALL_CEILING, |f| matches!(f, Frame::Facts(_)));
    n.conn
        .send(&Frame::Clean(devsched_wire::Clean {
            project: "p".into(),
            tree: "w".into(),
        }))
        .unwrap();
    n.conn.send(&Frame::Beat).unwrap();
    n.until(WALL_CEILING, |f| matches!(f, Frame::Facts(_)));
    assert!(!proj.join("w").exists(), "回心跳时树已不在原处");
    let t = Instant::now();
    let trash = || {
        entries_of(&proj)
            .into_iter()
            .filter(|p| {
                p.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with(".del-")
            })
            .count()
    };
    while trash() > 0 && t.elapsed() < WALL_CEILING {
        std::thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(trash(), 0, "改了名的与上回留下的 .del-* 都删光");
}

#[test]
fn clean_sees_a_running_job_when_the_root_is_given_through_a_link() {
    let r = TestRoot::new();
    let real = r.0.join("real");
    std::fs::create_dir_all(real.join("projects/p/w")).unwrap();
    std::fs::write(real.join("projects/p/w/a.rs"), "x").unwrap();
    let link = r.0.join("ln");
    std::os::unix::fs::symlink(&real, &link).unwrap();
    let mut n = Node::start(&link, &[]);
    n.hello();
    n.job(1, "sleep 300 & echo $!; wait");
    n.read_pids(1, 1);
    n.conn
        .send(&Frame::Clean(devsched_wire::Clean {
            project: "p".into(),
            tree: "w".into(),
        }))
        .unwrap();
    n.conn.send(&Frame::Beat).unwrap();
    let mut refused = false;
    // 帧照收到的先后处理：清理的错误（有的话）先到，心跳回的 facts 后到
    while let Frame::Error(e) = n.until(WALL_CEILING, |f| {
        matches!(f, Frame::Error(_) | Frame::Facts(_))
    }) {
        refused |= e.msg.contains("有作业在跑");
    }
    assert!(
        refused && real.join("projects/p/w/a.rs").exists(),
        "根经链接给：作业在树里跑着 ⇒ clean 回错误、不删"
    );
}

/// 作业帧的 nice：这个账号调得了就照帧设，调不了（往高优先级调要提权）作业照常跑、照原样。
/// 「照帧设」那一半要往下还有余地：本进程已在 19（编译机上的作业都是 nice 19）时只判得了「调不了照常跑」那一半。
#[test]
fn job_frame_nice_is_set_where_the_account_may_and_a_refused_one_still_runs() {
    // SAFETY：只读本进程的优先级。
    let cur = unsafe { libc::getpriority(libc::PRIO_PROCESS, 0) };
    // 往高调（cur − 1）这个账号调不调得了：照 coreutils `nice -n -1` 自己试出来的为准（调不了它照原样跑、印原值）
    let o = Command::new("nice")
        .args(["-n", "-1", "nice"])
        .stderr(Stdio::null())
        .output()
        .unwrap();
    let higher: i32 = String::from_utf8_lossy(&o.stdout).trim().parse().unwrap();
    let mut cases = vec![(cur - 1, higher)];
    if cur < 19 {
        cases.push((cur + 1, cur + 1));
    }
    let r = TestRoot::new();
    let mut n = Node::start(&r.0, &[]);
    n.hello();
    for (i, (want, got)) in cases.into_iter().enumerate() {
        let id = i as u64 + 1;
        let mut j = job_frame(id, "nice");
        j.nice = Some(want);
        n.conn.send(&Frame::Job(j)).unwrap();
        let (out, e) = n.collect(id, WALL_CEILING);
        assert_eq!(
            (e.stop, e.code, out.trim()),
            (StopReason::Done, Some(0), got.to_string().as_str()),
            "此刻 nice {cur}、帧里要 {want}"
        );
    }
    n.job(9, "nice");
    let (out, _) = n.collect(9, WALL_CEILING);
    assert_eq!(out.trim(), cur.to_string(), "帧里没带 nice ⇒ 照节点自己的");
}

#[test]
fn forwarder_exits_when_the_main_node_goes() {
    // 心跳时限拉到测不着：b 退出只能是因为主节点没了，不是自己当了主节点、等心跳超时退的
    let hb = ["--heartbeat-timeout", "100000"];
    let r = TestRoot::new();
    let mut a = Node::start(&r.0, &hb);
    a.hello();
    let mut b = Node::start(&r.0, &hb);
    b.hello();
    b.job(1, "echo $PPID; sleep 300 & echo $!; wait");
    let v = b.read_pids(1, 2);
    assert_eq!(
        procs::stat(v[0]).map(|s| s.ppid),
        Some(a.pid()),
        "b 是转发：作业由 a 起"
    );
    unsafe { libc::kill(a.pid(), libc::SIGKILL) };
    assert!(a.wait_exit(WALL_CEILING).is_some());
    assert!(
        b.wait_exit(WALL_CEILING).is_some(),
        "主节点没了 ⇒ 转发的那一个也退出，不挂着"
    );
    assert!(
        wait_gone(&v, WALL_CEILING),
        "主节点没了 ⇒ 它起的作业组跟着没"
    );
}

#[test]
fn tmp_path_too_long_refuses_the_job_with_its_stop() {
    let r = TestRoot::new();
    // 根 ≥ 21 字节 ⇒ `<根>/t/xxxx` 再接 80 字节到 108；套接字 `<根>/state/n.sock` 仍在 108 以内
    let root = r.0.join("a".repeat(24));
    std::fs::create_dir_all(root.join("projects/p/w")).unwrap();
    let mut n = Node::start(&root, &[]);
    n.hello();
    n.job(1, "echo should-not-run");
    let (out, e) = n.collect(1, WALL_CEILING);
    assert_eq!(
        (e.stop, e.code, out.as_str()),
        (StopReason::TmpPath, None, ""),
        "临时目录路径超长 ⇒ 不起、停因照实"
    );
}

#[test]
fn two_jobs_on_one_connection_run_at_the_same_time() {
    let r = TestRoot::new();
    let mut n = Node::start(&r.0, &[]);
    n.hello();
    // 各自留一个记号、等对面的记号：一趟跑完另一趟才起 ⇒ 等不到、退 1
    let d = r.0.join("meet");
    std::fs::create_dir_all(&d).unwrap();
    let meet = |me: &str, other: &str| {
        format!(
            "touch '{d}/{me}'; i=0; while [ ! -e '{d}/{other}' ] && [ $i -lt 600 ]; do sleep 0.05; i=$((i+1)); done; [ -e '{d}/{other}' ]",
            d = d.display()
        )
    };
    n.job(1, &meet("a", "b"));
    n.job(2, &meet("b", "a"));
    let mut codes = BTreeMap::new();
    let t = Instant::now();
    while codes.len() < 2 && t.elapsed() < WALL_CEILING {
        if let Some(Frame::Exit(e)) = n.recv(Duration::from_millis(200)) {
            codes.insert(e.job, (e.stop, e.code));
        }
    }
    let done = (StopReason::Done, Some(0));
    assert_eq!(
        codes,
        BTreeMap::from([(1, done), (2, done)]),
        "同一条连接上两趟同时跑"
    );
}
