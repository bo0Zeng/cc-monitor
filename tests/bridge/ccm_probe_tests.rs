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
    };
    // 用户机器上那份 2026-07-27 的旧 bash：答得出 `--ccm-probe`（所以「在不在」判不了它），
    // 但版本与能力集都是上一代。
    let legacy = CcmProbeResult {
        installed: true,
        version: Some("4".into()),
        capabilities: vec!["new".into(), "resume".into()],
        build: None,
    };
    let v = classify_path_ccm(&ours, &legacy);
    assert_eq!(v, PathCcmVerdict::NotOurs, "旧的没被认出来");
    let hint = render_path_ccm_hint(v, &ours, &legacy, Some("$HOME/.cc-monitor/bin/ccm"));
    assert!(
        hint.contains("不是") && hint.contains("version=4") && hint.contains("version=5"),
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
    };
    let b = CcmProbeResult {
        installed: true,
        version: Some("5".into()),
        capabilities: vec!["new".into(), "detach".into()],
        build: None,
    };
    // 同版本、能力集差一项 ⇒ 仍判「不是我们那一份」。**路径在这里根本不存在。**
    assert_eq!(classify_path_ccm(&a, &b), PathCcmVerdict::NotOurs);
    // 反向：能力集顺序不同不算不同（那是名片的写法，不是身份）。
    let b2 = CcmProbeResult {
        installed: true,
        version: Some("5".into()),
        capabilities: vec!["detach".into(), "new".into()],
        build: None,
    };
    let a2 = CcmProbeResult {
        installed: true,
        version: Some("5".into()),
        capabilities: vec!["new".into(), "detach".into()],
        build: None,
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
    let prod = guard_core::production_code(include_str!("../../src/bridge/src/ccm_probe.rs"));
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
