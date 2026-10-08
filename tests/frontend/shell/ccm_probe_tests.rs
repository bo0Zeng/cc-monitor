use super::*;

/// 🔴 `KR69D2`：**「你 PATH 上那个是旧的」这句话真的说得出来**，而且判的不是路径。
///
/// # 死值验（`KR69D2` 逐字要的那一格）
///
/// 造一个「PATH 上有旧 `ccm`、而我们也装了一份」的场景 ⇒ **必须指名说出来**。
/// 把 [`render_path_ccm_hint`] 里 `NotOurs` 那一支的话掏空 ⇒ 本条红。
///
/// # 失效方向（本条专门盯着它）
///
/// **只比路径**。下面第三格喂的是「两份东西**路径可以完全一样**、身份不同」——
/// 本条一个路径字符串都不看，所以那条路走不通。
#[test]
fn a_stale_ccm_on_path_is_named_out_loud() {
    let ours = CcmProbeResult {
        installed: true,
        version: Some("5".into()),
        capabilities: vec!["new".into(), "resume".into(), "detach".into()],
        build: None,
        at: None,
    };
    // 用户机器上那份 2026-07-27 的旧 bash：答得出 `--ccm-probe`（所以「在不在」判不了它），
    // 但版本与能力集都是上一代。
    let legacy = CcmProbeResult {
        installed: true,
        version: Some("4".into()),
        capabilities: vec!["new".into(), "resume".into()],
        build: None,
        at: None,
    };
    let v = classify_path_ccm(&ours, &legacy);
    assert_eq!(v, PathCcmVerdict::NotOurs, "旧的没被认出来");
    let hint = render_path_ccm_hint(v, &ours, &legacy, Some("$HOME/.cc-monitor/bin/ccm"));
    assert!(
        // 名片那半句按文案键取（`rsCcmProbe.card.summary` 不再是 `version=` 日志行形），不钉原文。
        copy_core::copy_matches("rsCcmProbe.hint.notOurs", &hint)
            && hint.contains(&describe_card(&legacy))
            && hint.contains(&describe_card(&ours)),
        "那句话没把「它是谁 / 我们是谁」摆出来 —— 只说「不一样」等于没说：\n{hint}"
    );
    // 🔴 产品**不删**：措辞里不许出现祈使的「请删除」。
    assert!(
        !hint.contains("请删除"),
        "产品在催用户删他自己的东西 —— `K34` 逐字「原本的配置**要手动删除**」，\n\
             那是**用户的**动作；`K31` 更不许我们代劳。这一格只许指名。\n{hint}"
    );
    // ★ 同版本同能力 ⇒ 判 `Ours`，而且**没有话要说**（免得每次打开都吓人一跳）。
    assert_eq!(classify_path_ccm(&ours, &ours), PathCcmVerdict::Ours);
    assert_eq!(
        render_path_ccm_hint(PathCcmVerdict::Ours, &ours, &ours, None),
        ""
    );
    // ★ 我们自己那份没装 ⇒ **说不出**，不许说成「你 PATH 上那个是旧的」。
    let nothing = parse_probe_output("");
    assert_eq!(
        classify_path_ccm(&nothing, &legacy),
        PathCcmVerdict::Undetermined,
        "我们自己都没装，却对别人下了判词 —— 那就是替用户下一个他没做过的结论"
    );
    // ★ PATH 上什么都没有 ⇒ `Absent`，与「是旧的」分得开。
    assert_eq!(classify_path_ccm(&ours, &nothing), PathCcmVerdict::Absent);
}

/// `KR69D2` 的**失效方向那一格**：判词**不看路径**。
///
/// 两张名片身份不同，而「它们在哪」这件事本条压根问不到 ——
/// [`classify_path_ccm`] 的签名里没有路径。这条断的是**签名**买到的性质：
/// 有人想把它改成比路径，得先改签名，那已经是明知故犯了。
#[test]
fn the_verdict_cannot_be_reached_by_comparing_paths() {
    let a = CcmProbeResult {
        installed: true,
        version: Some("5".into()),
        capabilities: vec!["new".into()],
        build: None,
        at: None,
    };
    let b = CcmProbeResult {
        installed: true,
        version: Some("5".into()),
        capabilities: vec!["new".into(), "detach".into()],
        build: None,
        at: None,
    };
    // 同版本、能力集差一项 ⇒ 仍判「不是我们那一份」。**路径在这里根本不存在。**
    assert_eq!(classify_path_ccm(&a, &b), PathCcmVerdict::NotOurs);
    // 反向：能力集顺序不同不算不同（那是名片的写法，不是身份）。
    let b2 = CcmProbeResult {
        installed: true,
        version: Some("5".into()),
        capabilities: vec!["detach".into(), "new".into()],
        build: None,
        at: None,
    };
    let a2 = CcmProbeResult {
        installed: true,
        version: Some("5".into()),
        capabilities: vec!["new".into(), "detach".into()],
        build: None,
        at: None,
    };
    assert_eq!(
        classify_path_ccm(&a2, &b2),
        PathCcmVerdict::Ours,
        "同一套能力换个顺序被判成两个东西 —— 那会让用户每次打开都看到一句假警报"
    );
}

#[test]
fn parses_real_probe_output() {
    let out = "name=ccm\nversion=1\nself=/x/ccm\ncapabilities=new,resume,attach,tmux,account,cwd,agent,launcher,ccm-sid,print\nagents=claude,codex\n";
    let r = parse_probe_output(out);
    assert!(r.installed);
    assert_eq!(r.version.as_deref(), Some("1"));
    assert!(r.capabilities.contains(&"tmux".to_string()));
    assert!(r.capabilities.contains(&"ccm-sid".to_string()));
}

/// ★★ 🔴 `KR70D1`（09-12）：**产品从「那个进程自己」手里拿到构建身份。**
///
/// # 它买的是哪一格
///
/// `K-R68` 摸底的结论逐字：后端二进制的身份今天只能去读它**旁边**那个 `.build_id`
/// 文本文件，而那是 `release.yml` 从源码常量抠出来写的一张标签 ——
/// 三个载体的标签恒等 ⇒ 一格证据都不提供（`DECISIONS.md#R26` 裁定零）。
/// [`probe_binary_uncached`] 这条路是**直接问那个二进制**（`<bin> --ccm-probe`，
/// 不经 shell、不读它旁边任何文件），本条钉住那条握手**答得出身份**。
///
/// # 三格
///
/// ① 有 `build=` ⇒ 拿得到；② 旧后端（没有那一行）⇒ `None`，**不许猜**；
/// ③ 它**不参与** [`classify_path_ccm`] 的判词（理由住 `CcmProbeResult::build` 的头注：
/// 同一份后端的两个构建仍然是「我们这一份」）。
#[test]
fn the_probe_carries_the_build_identity_of_the_binary_itself() {
    let with = parse_probe_output(
        "name=ccm\nversion=5\nself=/x/ccm\ncapabilities=new\nagents=claude\nbuild=p9-sample\n",
    );
    assert_eq!(
        with.build.as_deref(),
        Some("p9-sample"),
        "对面自报了身份而我们没接住 —— 「问一份二进制它是谁」这条路断在解析这一跳"
    );
    // ② 旧后端不吐这一行 ⇒ 只许答「不知道」，不许拿别处的值顶上。
    let without =
        parse_probe_output("name=ccm\nversion=5\nself=/x/ccm\ncapabilities=new\nagents=claude\n");
    assert_eq!(
        without.build, None,
        "对面没说，我们替它编了一个 —— 那正是「把失败面换成假答案」那一族"
    );
    // ③ 身份不参与「是不是我们那一份」的判词。
    let a = CcmProbeResult {
        installed: true,
        version: Some("5".into()),
        capabilities: vec!["new".into()],
        build: Some("p9-old".into()),
        at: None,
    };
    let b = CcmProbeResult {
        build: Some("p9-new".into()),
        ..a.clone()
    };
    assert_eq!(
        classify_path_ccm(&a, &b),
        PathCcmVerdict::Ours,
        "两个构建的同一份后端被判成「不是我们那一份」—— \n\
             那会让用户每次打开都看到一句假警报（`CcmProbeResult::build` 头注逐字写着这条边界）"
    );
}

/// ★★ **超时那条路真的会返回**〔D 阶段补审 08-12〕。
///
/// 这条**必须真起一个挂住的子进程**，不能拿构造好的字符串喂 `parse_probe_output` ——
/// 本件要防的东西恰恰不在解析里，在「等」这一步：没有上限时用户点一次「恢复」
/// 就是永远转圈，而那个形态**任何纯函数判据都看不见**。
///
/// 用 50ms 的上限去等一个睡 30s 的 `bash`：
/// ① 必须在远早于 30s 的时间内返回（否则超时根本没生效）；
/// ② 结论必须是 `installed: false`（超时当没装 ⇒ 调用方降级回旧路，而不是拿半截当真）。
///
/// 射程：够得到「会返回、且返回什么」；**够不到**「子进程真被收干净了」——
/// 那要看 `/proc`，而本条不去做那件事（`kill` + `wait` 已在生产段，收尸由它负责）。
/// ★ 生产段喂给 [`probe_with`] 的命令**只有那个常量**。
///
/// 上一条判据为了能测「挂住」把命令做成了参数。那就开了一个口：
/// 谁都能从生产段传别的串进去，而**行为判据看不见这件事**（传什么它都照跑）。
/// ⇒ 这条按源码钉：生产段（剥掉测试段）里 `probe_with(` 只许出现一次，且实参是 `CCM_PROBE_CMD`。
#[test]
fn the_only_production_probe_command_is_the_constant() {
    let prod =
        guard_core::production_code(include_str!("../../../src/frontend/shell/src/ccm_probe.rs"));
    // ⚠ 排掉**定义行**：`fn probe_with(` 自己也含这个串。
    // 「判据被自己要钉的那个名字命中」本会话第五次 —— 它不是偶发，是按名字取样的固有形态。
    // ⚠ 取的是**整行**，不是「从命中处到行尾」——第一版取后者，于是定义行切出来的是
    // `probe_with(timeout: …)`，`fn ` 被切在了前面，滤不掉。
    // 「判据被自己要钉的那个名字命中」本会话第五次，而这次连**补的那道滤网也切错了**。
    let calls: Vec<&str> = prod
        .lines()
        .filter(|l| l.contains("probe_with(") && !l.contains("fn probe_with("))
        .collect();
    assert_eq!(
        calls.len(),
        1,
        "生产段里 `probe_with(` 出现 {} 次 —— 不是恰好一次，那个「命令是参数」的口就管不住了：{calls:?}",
        calls.len()
    );
    assert!(
        calls[0].contains("CCM_PROBE_CMD"),
        "生产段调 `probe_with` 时喂的不是 `CCM_PROBE_CMD` —— \n\
             那个参数只为判据而开（要一个「一定挂住」的串），生产侧不许借它跑别的东西。实得：{}",
        calls[0]
    );
}

#[test]
#[cfg(not(windows))]
fn a_hanging_shell_does_not_hang_the_resume_button() {
    let t0 = std::time::Instant::now();
    // ⚠ 上限要**跨过 rc 加载**（本机 `bash -lic` 实测 ~130ms）：给 50ms 的话子进程
    // 还没来得及吐字就被杀了，「半截输出」那一格永远构造不出来 —— 第三版夹具卡在这儿。
    // 800ms 足够它吐完首行 + 填充，又远小于下面 5s 那道断言。
    let r = probe_local_ccm_uncached_for_test_sleep(std::time::Duration::from_millis(800));
    let took = t0.elapsed();
    assert!(
        took < std::time::Duration::from_secs(5),
        "等了 {took:?} —— 超时没生效。生产上这意味着「恢复」按钮永远转圈，\n\
             而且锁一握不放之后**每一次**本机 resume 都堵在这里（D 阶段补审那两条的合体）"
    );
    assert!(
        !r.installed,
        "超时之后回了 `installed: true` —— 那会拿一段没读完的输出当能力集，\n\
             半截的首行恰好可能是 `name=ccm` 而 `capabilities=` 还没来 ⇒ 被读成「装了但没能力」，\n\
             那是个比「没装」更难查的假象"
    );
}

/// 上一条的夹具：**先吐半截、再挂住**，其余与生产逐字同一条路径。
///
/// ⚠⚠ 第一版这里是光秃秃的 `sleep 30`（零输出），于是「超时那次不采信半截输出」
/// 那条分支**判据根本够不到** —— 变异「超时后照样解析」当场**绿**。
/// 零输出时「采信」与「不采信」的结果恰好相同，那不是它被守住了，是它没被测。
/// ⇒ 夹具必须真吐出 `name=ccm` 再挂住：那正是生产上最难查的那一格，
///   首行成立而 `capabilities=` 还没来 ⇒ 被读成「装了但一个能力都没有」。
#[cfg(not(windows))]
fn probe_local_ccm_uncached_for_test_sleep(timeout: std::time::Duration) -> CcmProbeResult {
    // ⚠ 光 `printf` 冲不出来：`bash` 的 stdout 是管道 ⇒ **块缓冲**，
    // 杀掉时那半截还在它自己的缓冲区里，读到的是空 —— 第二版夹具就是在这儿又绿了一次。
    // 补一段够大的填充把缓冲挤过去，首行才真的到得了读端。
    probe_with(
        timeout,
        "printf 'name=ccm\\n'; head -c 200000 /dev/zero | tr '\\0' x; sleep 30",
    )
}

#[test]
fn no_ccm_sentinel_not_installed() {
    let r = parse_probe_output("NO_CCM\n");
    assert!(!r.installed);
    assert_eq!(r.capabilities.len(), 0);
}

#[test]
fn unrelated_same_name_binary_not_installed() {
    // PATH 上恰好有个不相关的同名 `ccm` 脚本——首行不是 "name=ccm" 就必须判定未装，
    // 不能因为 rc=0 就当已装（防止把用户自己的脚本误当成本工具的 CLI）。
    let r = parse_probe_output("some unrelated program output\n");
    assert!(!r.installed);
}

#[test]
fn empty_output_not_installed() {
    let r = parse_probe_output("");
    assert!(!r.installed);
}

/// 要求：「顺序定死：先读字节定身份，再决定要不要跑它」（本机 `ccm` 那一跳点名未收）。
/// 行为：落点上那份字节自报身份戳恰一个才算「我们的」；一个会自称 `name=ccm` 的脚本**不跑就不认**。
/// 接线：`local_ccm_entry_status` 里起进程那一臂恰一处、挂在字节认身份的守卫后面。
#[test]
fn our_own_ccm_is_identified_by_its_bytes_before_it_is_run() {
    let d = std::env::temp_dir().join(format!("ccm-e2-probe-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    let ours = d.join("ours");
    let mut b = b"\x7fELF....".to_vec();
    b.extend_from_slice(deploy_contract::STAMP_OPEN.as_bytes());
    b.extend_from_slice(b"p5a-e2");
    b.extend_from_slice(deploy_contract::STAMP_CLOSE.as_bytes());
    std::fs::write(&ours, &b).unwrap();
    let liar = d.join("liar");
    std::fs::write(&liar, "#!/bin/sh\nprintf 'name=ccm\\nversion=6\\n'\n").unwrap();
    assert!(ours_by_bytes(&ours), "带戳的后端字节没认出来");
    assert!(
        !ours_by_bytes(&liar),
        "一个只会自称 name=ccm 的脚本被当成了我们的"
    );
    assert!(
        !ours_by_bytes(&d.join("absent")),
        "不在的文件被当成了我们的"
    );
    let _ = std::fs::remove_dir_all(&d);
    let prod =
        guard_core::production_code(include_str!("../../../src/frontend/shell/src/ccm_probe.rs"));
    let at =
        guard_core::find_pinned(&prod, "pub(crate) fn local_ccm_entry_now(").expect("入口不在了");
    let body = &prod[at..];
    guard_core::find_pinned(
        body,
        "let ours_bytes = installed.is_some_and(|p| ours_by_bytes(p));",
    )
    .expect("「是不是我们的字节」那一格不在了");
    guard_core::find_pinned(
        body,
        "Some(p) if ours_bytes => probe_binary_uncached(p, OURS_PROBE_TIMEOUT),",
    )
    .expect("起 `--ccm-probe` 那一臂不在「先认字节」的守卫后面");
}

/// 「本机 ccm 那一格两件都报：我们那份装下来了 ＋ 登录 shell 里敲 ccm 走到的是不是它 —— 走到别处（如旧 shim）就明说是哪一份、怎么清，不代清」。
///
/// 三件：① 探针补的 `at=` 行摘得出（名片认不得也摘）；② 落在哪按**文件**判（软链解开 · 旧入口认得出 · 同名片的旧 shim 也判「不是它」）；
/// ③ 那一格两件都说、`ok` 只在两件都成时为真，说不清不写。期望值全是手写字面量。
#[cfg(unix)]
#[test]
fn the_local_ccm_cell_reports_both_halves_and_names_where_ccm_really_goes() {
    // ①
    let r = parse_probe_output("name=ccm\nversion=5\ncapabilities=new\n\nat=/h/.local/bin/ccm\n");
    assert!(r.installed);
    assert_eq!(r.at.as_deref(), Some("/h/.local/bin/ccm"));
    let broken = parse_probe_output("\nat=/h/.local/bin/ccm\n");
    assert!(!broken.installed, "答不出名片的旧入口不算「装了」");
    assert_eq!(
        broken.at.as_deref(),
        Some("/h/.local/bin/ccm"),
        "答不出名片也要说得出住址"
    );
    assert_eq!(parse_probe_output("NO_CCM\n").at, None);

    // ②
    let d = std::env::temp_dir().join(format!("ccm-fix3-reach-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    let landing = d.join("landing-ccm");
    std::fs::write(&landing, b"\x7fELF").unwrap();
    let link = d.join("link-ccm");
    std::os::unix::fs::symlink(&landing, &link).unwrap();
    let shim = d.join("shim-ccm");
    std::fs::write(
        &shim,
        "#!/bin/sh\n# cc-monitor: ccm = 后端本体的一次性模式（K33：所有命令只许有一处）\nexec x ccm \"$@\"\n",
    )
    .unwrap();
    let mine = d.join("mine-ccm");
    std::fs::write(&mine, "#!/bin/sh\necho hi\n").unwrap();
    let s = |p: &std::path::Path| p.to_string_lossy().into_owned();
    assert_eq!(reach_of(None, Some(&landing)), Reach::Nothing);
    assert_eq!(
        reach_of(Some(&s(&link)), Some(&landing)),
        Reach::Landing,
        "软链到落点就是它"
    );
    assert_eq!(
        reach_of(Some(&s(&shim)), Some(&landing)),
        Reach::OtherFile { path: s(&shim) }
    );
    assert_eq!(
        reach_of(Some(&s(&mine)), Some(&landing)),
        Reach::OtherFile { path: s(&mine) }
    );
    assert_eq!(
        reach_of(Some("ccm"), Some(&landing)),
        Reach::NotAFile,
        "函数 / 别名不是文件"
    );
    let card = CcmProbeResult {
        installed: true,
        version: Some("5".into()),
        capabilities: vec!["new".into()],
        build: None,
        at: Some(s(&shim)),
    };
    let other = Reach::OtherFile { path: s(&shim) };
    assert_eq!(
        judge_path_ccm(&card, &card, &other),
        PathCcmVerdict::NotOurs,
        "名片一样、落在另一个文件（旧 shim 转给同版本后端）仍不是它"
    );
    assert_eq!(
        judge_path_ccm(&card, &card, &Reach::NotAFile),
        PathCcmVerdict::Ours
    );
    let _ = std::fs::remove_dir_all(&d);

    // ③
    assert_eq!(
        local_ccm_cell(true, true, PathCcmVerdict::Ours, &card),
        (
            Some(true),
            copy_core::copy_static!("rsCcmProbe.cell.ours").to_string()
        )
    );
    assert_eq!(
        local_ccm_cell(true, true, PathCcmVerdict::NotOurs, &card),
        (
            Some(false),
            copy_core::copy_text("rsCcmProbe.cell.elsewhere", &[("at", &s(&shim))])
        )
    );
    assert_eq!(
        local_ccm_cell(true, true, PathCcmVerdict::Absent, &card),
        (
            Some(false),
            copy_core::copy_static!("rsCcmProbe.cell.absent").to_string()
        )
    );
    assert_eq!(
        local_ccm_cell(false, false, PathCcmVerdict::Undetermined, &card),
        (
            Some(false),
            copy_core::copy_static!("rsCcmProbe.cell.notLanded").to_string()
        )
    );
    assert_eq!(
        local_ccm_cell(true, true, PathCcmVerdict::Undetermined, &card),
        (None, String::new()),
        "字节是我们的却问不出名片 ⇒ 说不清，不写账本"
    );
}

/// 读数，**不在门禁**（要一个 PowerShell；`CCM_PWSH=<程序> cargo test -- --ignored`，那个程序收一个 `.ps1` 路径去跑）。
/// 要求：「本机 ccm 那一格两件都报 ——「我们那份装下来了」＋「登录 shell 里敲 `ccm` 走到的是不是它」」。
///
/// Windows 那一形的探测串（`CCM_PROBE_PS`）在真 PowerShell 里跑三种情形，输出交给同一个 [`parse_probe_output`]：
/// PATH 上有一个 `ccm` 程序 ⇒ 名片读得出、住址是它的路径；只有同名函数 ⇒ 名片读得出、住址是名字（判词回退比名片）；都没有 ⇒ 没装、无住址。
/// 买不到：`powershell.exe` 5.1 本身 · 真加载用户 profile · 现拼 PATH 那一跳（`FRESH_PATH_PS` 读注册表，只有 Windows 有）。
#[test]
#[ignore = "要一个 PowerShell：CCM_PWSH=<收 .ps1 路径的程序> cargo test -- --ignored"]
fn wf1_the_windows_probe_script_reports_card_and_where_ccm_resolves() {
    let Ok(pwsh) = std::env::var("CCM_PWSH") else {
        panic!("没给 CCM_PWSH");
    };
    let card = "name=ccm\nversion=5\ncapabilities=new,resume\nbuild=b1\n";
    let dir = std::env::temp_dir().join(format!("wf1-ccm-probe-{}", std::process::id()));
    let bin = dir.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let ccm = bin.join("ccm");
    std::fs::write(
        &ccm,
        format!(
            "#!/bin/sh\n[ \"$1 $2\" = '-- --ccm-probe' ] && printf '{}'\n",
            card.replace('\n', "\\n")
        ),
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(&ccm, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let run = |tag: &str, setup: &str| -> CcmProbeResult {
        let f = dir.join(format!("{tag}.ps1"));
        std::fs::write(&f, format!("{setup}\n{CCM_PROBE_PS}")).unwrap();
        let out = std::process::Command::new(&pwsh)
            .arg(&f)
            .output()
            .expect("起 CCM_PWSH");
        assert!(
            out.status.success(),
            "{tag}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        parse_probe_output(&String::from_utf8_lossy(&out.stdout))
    };
    let on_path = run(
        "app",
        "$env:PATH = \"$PSScriptRoot/bin\" + [IO.Path]::PathSeparator + $env:PATH",
    );
    let func = run(
        "func",
        &format!(
            "$env:PATH = '/usr/bin:/bin'\nfunction ccm {{ '{}' }}",
            card.trim_end().replace('\n', "'; '")
        ),
    );
    let none = run("none", "$env:PATH = '/usr/bin:/bin'");
    let _ = std::fs::remove_dir_all(&dir);
    let want_card = |r: &CcmProbeResult| {
        (
            r.installed,
            r.version.clone(),
            r.capabilities.clone(),
            r.build.clone(),
        )
    };
    let full = (
        true,
        Some("5".to_string()),
        vec!["new".to_string(), "resume".to_string()],
        Some("b1".to_string()),
    );
    assert_eq!(want_card(&on_path), full, "程序：{on_path:?}");
    assert!(
        on_path
            .at
            .as_deref()
            .is_some_and(|a| a.ends_with("/bin/ccm")),
        "程序的住址该是它的路径：{on_path:?}"
    );
    assert_eq!(want_card(&func), full, "函数：{func:?}");
    assert_eq!(
        func.at.as_deref(),
        Some("ccm"),
        "函数的住址该是名字：{func:?}"
    );
    assert_eq!(
        (none.installed, none.at.as_deref()),
        (false, None),
        "没有：{none:?}"
    );
}

/// 〔判定只在后端〕[`ask_once`] 读 CLI 面的信封（`control/cli_control.rs` 头注那三条）：
/// 入参从 stdin 交到（替身原样回显）· exit 0 ⇒ stdout 那行 JSON · exit 2 ⇒ `{code, message}` 成 `Refused` ·
/// 别的退出码 / 不成 JSON ⇒ `Unreadable` · 上限内不退 ⇒ `TimedOut` · 起不来 ⇒ `Spawn`。替身是一份 sh 脚本（不是后端），
/// 判的是问法；判词本身住后端（`deploy_plan_tests`）。
#[cfg(unix)]
#[test]
fn ask_once_reads_the_cli_envelope_both_ways() {
    use std::os::unix::fs::PermissionsExt;
    let d = std::env::temp_dir().join(format!("ccm-p1-once-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    let script = |name: &str, body: &str| {
        let p = d.join(name);
        std::fs::write(&p, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
        p
    };
    let t = std::time::Duration::from_secs(5);
    let args = serde_json::json!({ "text": "#!/bin/sh\n", "n": 1 });
    // 刚写好的替身脚本，exec 那一刻可能撞上别的测试线程 fork 出来、还没 exec 的子进程握着它的写 fd
    // ⇒ `ETXTBSY`（`os error 26`，认 errno 不认 locale 文字）。只对这一种起不来重试，上限 50 × 20ms；
    // 别的结果原样交回判（同 `launch_tests` 起假终端那一处）。
    let once = |prog: &std::path::Path, cmd: &str, timeout: std::time::Duration| {
        let mut r = ask_once(prog, cmd, &args, timeout);
        for _ in 0..50 {
            match &r {
                Err(OnceErr::Spawn(e)) if e.contains("os error 26") => {
                    std::thread::sleep(std::time::Duration::from_millis(20));
                    r = ask_once(prog, cmd, &args, timeout);
                }
                _ => break,
            }
        }
        r
    };
    let echo = script(
        "echo",
        r#"[ "$1" = "--" ] && [ "$2" = "--deploy-retired" ] || exit 9; cat"#,
    );
    assert_eq!(once(&echo, "deploy-retired", t), Ok(args.clone()));
    let no = script(
        "no",
        r#"echo '{"code":"refused","message":"不放"}' >&2; exit 2"#,
    );
    assert_eq!(
        once(&no, "x", t),
        Err(OnceErr::Refused {
            code: "refused".into(),
            message: "不放".into()
        })
    );
    for (name, body) in [
        ("garbage", "echo not-json"),
        ("exit1", "echo '{}'; exit 1"),
        ("bad-envelope", "echo '{\"code\":1}' >&2; exit 2"),
    ] {
        let p = script(name, body);
        assert!(
            matches!(once(&p, "x", t), Err(OnceErr::Unreadable(_))),
            "{name}"
        );
    }
    let hang = script("hang", "exec sleep 30");
    assert_eq!(
        once(&hang, "x", std::time::Duration::from_millis(300)),
        Err(OnceErr::TimedOut)
    );
    assert!(matches!(
        ask_once(&d.join("absent"), "x", &args, t),
        Err(OnceErr::Spawn(_))
    ));
    let _ = std::fs::remove_dir_all(&d);
}

/// 报备「monitor 让 PowerShell 吐 UTF-8 统一到「探针直写 UTF-8 字节」一形（`ccm_probe.rs` 那处 P1 跟）」。
/// 同 P2 `profile_installer::render_user_path_probe_command` 那一形。
/// Windows 那两段探针（现拼 PATH · 问 `ccm`）各**恰好一处**把整段编成 UTF-8 字节直写标准输出流，且**零处**再去改控制台代码页。
/// 买不到：真 `powershell.exe` 5.1 上的读数（本机没有 PowerShell；`wf1_the_windows_probe_script_reports_card_and_where_ccm_resolves` 那台架要 `CCM_PWSH`）。
#[test]
fn the_windows_probe_scripts_write_utf8_bytes_themselves() {
    for (name, ps) in [
        ("FRESH_PATH_PS", FRESH_PATH_PS),
        ("CCM_PROBE_PS", CCM_PROBE_PS),
    ] {
        for (needle, want) in [
            ("[Text.Encoding]::UTF8.GetBytes(", 1),
            ("$o = [Console]::OpenStandardOutput()", 1),
            ("$o.Write($b, 0, $b.Length)", 1),
            ("OutputEncoding", 0),
        ] {
            assert_eq!(ps.matches(needle).count(), want, "{name}：`{needle}`");
        }
    }
}

/// 「cc-monitor 这一份在哪」那一句：Windows 上写绝对路径（不写 `$HOME/…`）；POSIX 照旧写 `$HOME/…`（与生成进命令里的那一形同）。
#[test]
fn where_our_ccm_lives_is_said_in_the_platform_own_way() {
    use crate::platform::login_shell::LoginShell;
    let abs = std::path::Path::new(r"C:\Users\u\.cc-monitor\bin\ccm.exe");
    let home_form = "$HOME/.cc-monitor/bin/ccm.exe";
    assert_eq!(
        LoginShell::PowerShell.entry_shown(abs, home_form),
        r"C:\Users\u\.cc-monitor\bin\ccm.exe"
    );
    assert_eq!(LoginShell::Posix.entry_shown(abs, home_form), home_form);
}
