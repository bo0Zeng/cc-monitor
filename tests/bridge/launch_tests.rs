use super::*;

fn cfg(host: &str, user: &str, port: u16, key: Option<&str>) -> RemoteConfig {
    RemoteConfig {
        host: host.into(),
        label: "t".into(),
        port,
        user: user.into(),
        key_path: key.map(String::from),
        daemon_path: "d".into(),
        host_key_fingerprint: None,
        addresses: Vec::new(),
        jump: None,
    }
}

/// ★ U8b：**POSIX 上「不开终端窗口」是既定设计，文案不许暗示「以后会支持」。**
///
/// 原文案「拉起终端窗口仅支持 Windows（v1）」里那个 `(v1)` 在撒谎 —— L1 早就裁决过
/// 反方向（`launch_local_posix` 头注：开窗要先猜终端模拟器，是平白引入一个会在别人
/// 机器上错的决定）。用户在 Linux 上每次点 ↗ 都会读到那句话。
/// ★★ **F06b-1d（C9）：backend 开的每一个终端窗口都必须带上 daemon 路径。**
///
/// 判据形态：**零命中守卫**（跑法：单测扫生产源码 · 钉的性质：**生产接线** ——
/// 两维分开写，见 `ROADMAP §4` 登记的计量缺陷）。
///
/// # 它防的是什么
///
/// 「给窗口设 env」这件事**没有集中落点**：本文件有三个各自 spawn 的开窗点
/// （POSIX 一个、Windows 的 `wt.exe` 与 conhost 兜底各一个）。**新加第四个而忘了带 env**，
/// 后果是那条路上的 `ccm resume` **静默地**永远走本地 —— 与名字打错同一族的静默失败。
/// ⇒ 用「`Command::new(` 的总数」当触发器：多一个就红，逼人回来看这条。
///
/// ⚠ **别手搓剥测试的尺子**：用 `guard_core::production_code`（仓里已有那把）。
/// 不剥的话，下面那两条**测试里的字符串字面量** `"Command::new(\"gnome-terminal\")"`
/// 会被数进去 —— 实测裸数是 6，生产里只有 4。
#[test]
fn every_terminal_window_backend_opens_carries_the_daemon_path() {
    let src = include_str!("../../src/bridge/src/launch.rs");
    let prod = guard_core::production_code(src);
    let spawns = prod.matches("Command::new(").count();
    // ⚠ 钉**真正的动作** `.env(k, v)`，不是 helper 的调用次数：
    //    Windows 那个函数**只调一次 helper**，把结果给两个 spawn 点共用
    //    ⇒ 按 helper 数写地板会写成 3，实测 2（第一版就这么错的，被本条自己逮住）。
    let helper = prod.matches("daemon_bin_env_for_window(").count();
    let carried = prod.matches(".env(k, v)").count();
    // 抽取器自检：剥完必须还看得见东西，且**确实剥掉了**测试里那两条字面量。
    assert!(
        prod.len() > 5_000,
        "剥完只剩 {} 字节 —— 剥过头了，本条会零命中地绿",
        prod.len()
    );
    assert!(
        !prod.contains("gnome-terminal"),
        "测试段没剥干净（`gnome-terminal` 只出现在测试的字符串字面量里）"
    );
    // 生产里 4 个：三个开窗点 + 一个 `where.exe` 探测（它不是开窗，不需要 env）。
    // 🔴 〔`K-H2b` `D6` 第九拍，08-29〕**5 → 4**：POSIX 那条路上先前是两个 `Command::new(`
    //    （开窗一个 · 无窗口回落一个），本轮把「谁被 spawn · 带哪些参数」抽成纯函数
    //    `build_local_posix_spawn` 之后合并成**一个** —— 开窗点的**人数没变**，
    //    变的是它们从两处 `Command::new(` 收成一处。
    //    ⚠ 这不是「把上限调上去让今天好过」：这个数是**等号**、方向是**减少**，
    //    三条断言（`carried == 3` · `helper == 2` · `where.exe` 那条身份）**一字未动**。
    //    🔴 **订正〔`D7 阻-5`③，08-29〕：这里先前逐字写着「它守的那条不变式
    //    （`spawns - carried == 1`，那 1 是 `where.exe` 探测）**逐字仍然成立**」——
    //    那句话描述的是一个没有发生过的世界。** 现打 `26f53da`：`spawns == 5` ·
    //    `carried == 3` ⇒ **差是 2，不是 1**（那时不带 env 的有两个：`where.exe` 探测
    //    **+ POSIX 无窗口回落**）。⇒ `spawns - carried == 1` 是**这一改之后才第一次成立**的，
    //    不是「仍然」。结论（收紧）没变，错的是那句话。
    assert_eq!(
        spawns, 4,
        "`launch.rs` 生产代码里的 `Command::new(` 从 4 变成了 {spawns}。\n\
             若新增的是**开终端窗口**，它必须也带上 `daemon_bin_env_for_window(...)` 的 env，\n\
             否则那条路上的 `ccm resume` 会**静默地**永远走本地（与名字打错同一族的静默失败）；\n\
             若新增的只是探测进程（如 `where.exe`），把本条的数字与这句说明一起更新。"
    );
    assert_eq!(
        carried, 3,
        "真的把 env 交给窗口的 spawn 点从 3 变成了 {carried} —— 有开窗点漏了，或有人删了它"
    );
    assert_eq!(
        helper, 2,
        "解析 sidecar 路径的调用点从 2 变成了 {helper}（POSIX 一次 · Windows 一次给两个 spawn 共用）"
    );
    // 那个不带 env 的必须是探测，不是开窗：钉住它的身份，别让「探测」变成豁免借口。
    assert!(
        prod.contains("Command::new(\"where.exe\")"),
        "唯一允许不带 daemon env 的 `Command::new` 是 `where.exe` 探测；它不见了 ⇒ \n\
             要么被改名，要么 4-3=1 这个差额现在对应的是一个**真开窗点**"
    );
}

/// ★ **那一句无条件断言不许出现在散文里**〔audit-0805 F08 下半〕
///
/// ⚠ **08-06 把这条标题收窄了**：原来写的是「『容器一定是 tmux』这个**无条件说法**
/// 不许出现」—— 那比它实际做的宽。实测两个洞：
/// ① **同义改写不认**：往被扫文件里写「远端会话的容器一定是 tmux」，本条**不红**
///    （needle 是一个精确短语，不是「无条件说法」这个语义）；
/// ② **扫描面是洞**（已修）：把**原句**写进 `src/doc/DEVELOPMENT.md`（原先只扫三份）也**不红**。
///
/// ①**刻意不追**，理由是量过的：想把它翻成本仓偏好的枚举式白名单
/// （「每一行同时出现『容器』与 tmux 的散文都必须说清是哪条路」），
/// 实测 11 行里 **9 行会误红** —— 那 9 行大多是**订正句**（「那是假的」）与本守卫自己的代码。
/// 又一次「人群不同质」（同 `platform/fallback_guard` 那条）：
/// 关于同一话题的散文里混着**断言、订正、判据本身**三类，白名单分不开它们。
/// ⇒ 保留精确 needle，但**把话说准**：它挡的是那一句原文的回潮，不是一族说法。。
///
/// 它今天在三处散文里当**理由**用（解释 POSIX 为什么不开终端窗口），而代码说的相反：
/// POSIX 远端 `↺` 走 `runRemoteResume` → `planResumeDirect`，
/// 那里逐字写着 `container: { kind: "none" }`（`launch-requests.ts:45`，
/// 是全文件唯一一个 `none`，其余四个 plan 才是 tmux）。
///
/// F08 上半订正过**两条**同源假头注（`launch.rs:122` 与 `src/fork-start.ts`），
/// 但**漏了这三处** —— 因为它们在**另一条路**（远端）上，看起来像是另一件事。
/// ⇒ 复核时才发现（E1：台账是筛子不是免检章）。
///
/// ⚠ 这不是说「远端永远不进 tmux」：`planResumeTmux`（F52）那条**就是** tmux。
/// 假的是**无条件的那个说法**，以及拿它当「不开终端窗口」的理由。
/// 要说容器，就得说清是哪条路。
///
/// ⚠ needle **运行时拼**：写成字面量的话，本条会在**自己的注释里**找到它 ⇒ 恒红
/// （F23 那一族的镜像）。
#[test]
fn no_prose_claims_the_session_container_is_always_tmux() {
    let needle = format!("会话容器{}是 tmux", "本来就");
    let root = crate::guard_support::repo_root();
    // 〔08-06 扩面〕原来只扫三份。实测：把**原句**写进 `src/doc/DEVELOPMENT.md`
    // （不在那三份里）**不会红** —— 扫描面本身就是个洞。⇒ 改成「全 `doc/` + 两份 README + 本文件」。
    //
    // ⚠ 用 `scan_tree!` 而**不是裸 `read_dir`**：`scanning_guard_registry` 那条元判据
    // 当场把裸遍历拦下了（理由是「判据在自己那份里找到自己 ⇒ 恒绿」，实测五次）。
    // 这里扫的是 `doc/` 的 md、与本文件不同族，但**规矩就是规矩** —— 而且它本来就更省事。
    let mut files: Vec<(String, String)> = Vec::new();
    for (q, body) in guard_core::scan_tree!(&root.join("src/doc"), &["md"]) {
        files.push((
            format!("doc/{}", q.file_name().expect("文件名").to_string_lossy()),
            body,
        ));
    }
    for f in ["README.md", "README.en.md", "src/bridge/src/launch.rs"] {
        let body = std::fs::read_to_string(root.join(f))
            .unwrap_or_else(|e| panic!("{f} 读不到：{e} —— 文件搬了就把本条一起改"));
        files.push((f.to_string(), body));
    }
    // ★★ 〔P3b 08-12 第二次扩面〕**加 `tests/e2e/` 与 `src/`**。
    //
    // 08-06 那次（`c87d123`）的账是「扫描面本身是个洞」，扩到了 `doc/` + 两份 README。
    // 今天再量：**洞还在，只是挪了个位置** —— `tests/e2e/restart-cmd-driver.ts:6` 那句
    // 「GUI 全链在 Linux 结构性不可达（launch.rs 仅 Windows→回退剪贴板）」
    // 就躺在扫不到的地方，而 `launch.rs::launch_local_posix` 明明就在本文件里。
    //
    // ⇒ **这不是巧合**：扫描面按「想到哪扫哪」长出来，而假话按「写在哪就在哪」分布。
    // 两者的形状不一样，所以「上次扩过了」不等于「这次够了」。
    // 〔搬树 2026-09-18〕**这一格从 `"tests/e2e"` 换成整棵 `"tests"`** —— 与
    // `structural_scan` 那边同一个判断：前端测试此前住在 `src/` 里、被下面那个 `"src"`
    // 顺带收着；搬去 `tests/` 之后**这个语料面悄悄缩了一大块**，而本条的反空真检查
    // 只管「某一族够不够」，管不了「少了一棵树」⇒ 会安静地少扫，不会红。
    for (dir, exts) in [("tests", &["ts", "sh", "md"][..]), ("src", &["ts"][..])] {
        for (q, body) in guard_core::scan_tree!(&root.join(dir), exts) {
            // 🔴 〔2026-09-18〕**`evidence/` 要排掉。** 它是量具与记录，
            // 里面的审计表会**逐字引用探针自己的那句话**（实发一例：一份 deathvalue
            // 记录里有一行在复述本条的探针串）⇒ 收进来等于**把探针的串喂给探针**，
            // 当场一条假阳。`structural_scan` 那边排掉它是同一条理由。
            //
            // ⚠ 本注释**刻意不复述那句探针串、也不写出那边那个函数名**：
            // 写全了会被本条与死名那条各命中一次（`调研/设计/16 §5.5`：
            // 注释里引用旧形状时，不要写成能被同一条规则命中的完整形）。
            // 实测：第一版注释把两者都写全了，当场多出两条假阳。
            if q.to_string_lossy()
                .replace('\\', "/")
                .contains("/evidence/")
            {
                continue;
            }
            files.push((
                format!("{dir}/{}", q.file_name().expect("文件名").to_string_lossy()),
                body,
            ));
        }
    }
    // ★★ **扩面自检用「比例」不用「绝对下限」**〔D 阶段补审 08-12 自查改的〕。
    //
    // 第一版写 `>= 90`，而实测真实人群是 **365**（`doc` 11 + `e2e` 30 + `src` 321 + 固定 3）
    // ⇒ 可以**静默少扫 275 份**而本条照绿。
    //
    // ⚠ 这个仓**刚刚才教过我**：`shell_lint_registry` 的账逐字写着
    // 「`≥` 正是它落后三次的成因 —— 加脚本时它不响，于是没人回来棘」，
    // 而我在同一天写了同一个形状。⇒ 「读过那条教训」不等于「写代码时想得起来」。
    //
    // 不写成等号是因为**这个人群天天在变**（`src/*.ts` 加一个文件就变）——
    // 等号会天天假红，那正是铁律 18 说的「假阳会训练人绕过判据」。
    // 折中：**按目录分族各自要够**（少了哪一族就点名哪一族），
    // 总数只做一个「明显坏了」的兜底。人群不同质 ⇒ 自检也不该只有一个数。
    {
        let by = |pre: &str| files.iter().filter(|(f, _)| f.starts_with(pre)).count();
        // 〔2026-09-18 重设地板〕现打：`src/doc/*.md` 10 · `tests/**` 150 个 .ts ＋
        // `tests/e2e` 的 sh/md · `src/**.ts` 213。
        // `src/` 那格从 **250 下调到 200**：**不是扫描面塌了**，是 143 份前端测试
        // 合法搬到了 `tests/`（它们现在由上面那一族数着）。
        for (pre, floor) in [("doc/", 8usize), ("tests/", 100), ("src/", 200)] {
            let n = by(pre);
            assert!(
                n >= floor,
                "`{pre}` 只收到 {n} 份（地板 {floor}）—— 那一族的扫描面塌了，\n\
                     而总数可能因为别族还在而看起来正常。**分族数就是为了让这种塌方点名可见**。"
            );
        }
        assert!(
            files.len() >= 300,
            "总共只收到 {} 份散文 —— 明显坏了（08-12 实测 365）",
            files.len()
        );
    }
    let mut total = 0usize;
    let mut hits = Vec::new();
    for (f, body) in &files {
        total += body.len();
        let n = body.matches(needle.as_str()).count();
        if n > 0 {
            hits.push(format!("  {f}：{n} 处"));
        }
    }
    // 抽取器自检：全部散文都读到了才算数（读空了下面会零命中地绿）。
    assert!(
        total > 20_000,
        "三份散文只读到 {total} 字节 —— 抽取器坏了，本条此刻是空转的"
    );
    assert!(
        hits.is_empty(),
        "这些散文还在无条件断言「会话容器就是 tmux」，而代码说的相反：\n{}\n\n\
             POSIX 远端 `↺` 走 `planResumeDirect`，那里是 `container: {{ kind: \"none\" }}`\n\
             （`launch-requests.ts:45`，全文件唯一一个 `none`）。判据在\n\
             `launch-requests.vitest.ts` 的「远端 resume 的会话容器」那一组。\n\
             ★ 要说 tmux，就得说清**是哪条路**（`planResumeTmux` 那条才是）——\n\
             无条件的说法是假的，而它今天正被当成「POSIX 不开终端窗口」的理由。",
        hits.join("\n")
    );
}

#[test]
fn the_posix_message_states_a_decision_not_a_missing_feature() {
    let m = POSIX_NO_TERMINAL_WINDOW;
    assert!(
        !m.contains("v1") && !m.contains("v2"),
        "文案里带版本号会被读成「以后会支持」：{m}"
    );
    assert!(m.contains("刻意"), "没说清这是刻意的：{m}");
    assert!(
        m.contains("tmux"),
        "没说清会话容器是什么，用户不知道去哪找：{m}"
    );
    assert!(m.contains("既定设计"), "没有把「这不是没做完」说出来：{m}");
}

/// ★ U8b **跨轨对拍**：前端匹配的那个标记，必须真的在后端那句话里。
///
/// 前端据它把标题从「拉起失败」换成「本机不开终端窗口」。两边漂开的症状是
/// **静默退回**：用户又开始在 Linux 上每次点 ↗ 都读到「拉起失败」，而两边各自看都对。
#[test]
fn the_posix_marker_is_the_one_the_frontend_matches_on() {
    const RUNNER: &str = include_str!("../../src/remote-launch-run.ts");
    let key = "export const POSIX_NO_WINDOW_MARKER = \"";
    let at = RUNNER
        .find(key)
        .expect("前端找不到 POSIX_NO_WINDOW_MARKER —— 抽取坏了，本断言在空转");
    let rest = &RUNNER[at + key.len()..];
    let marker = &rest[..rest.find('"').expect("字面量没收尾")];
    assert!(
        marker.chars().count() >= 6,
        "抽到的标记太短（{marker:?}）—— 抽取坏了"
    );
    assert!(
        POSIX_NO_TERMINAL_WINDOW.contains(marker),
        "\n前端按 {marker:?} 判「这是既定设计」，但后端那句话里没有它：\n  {POSIX_NO_TERMINAL_WINDOW}\n\
             ⇒ 用户会退回去看到「拉起失败」。两边必须一起改。"
    );
    // 反面：标记不许宽到把**真失败**也软化掉。
    for real_failure in [
        "未找到远端配置: \"x\"",
        "refuse launch: 远端命令含控制字符",
        "spawn powershell failed: No such file",
    ] {
        assert!(
            !real_failure.contains(marker),
            "标记 {marker:?} 太宽，会把真失败 {real_failure:?} 也报成「既定设计」"
        );
    }
}

/// ★ P5L-Y1/Y3：**候选表有序，且第一个存在的胜出**。
///
/// ⚠⚠ **补门的代价：本条从此在 Windows 上 0 次执行**〔win-compile 09-09〕。
/// 被测的 `TERMINAL_EXITS` 与 `pick_terminal_exit_from` 都带 `#[cfg(not(windows))]`，
/// 而本条**漏了对应的门** ⇒ 云端（windows-latest，本仓**唯一**跑 `cargo test` 的平台）
/// 上 `--all-targets` 直接**编译失败**（E0425 ×7），不是警告。
/// 补门只买回「编得过」，**买不回覆盖**：它今天只在开发者的 POSIX 盘上跑，
/// CI 上没有任何东西证明它是绿的。**别把「CI 全绿」读成「这条跑过了」。**
#[cfg(not(windows))]
#[test]
fn the_terminal_exit_is_picked_in_declared_order() {
    // 都在 ⇒ 取第一个（`xdg-terminal-exec` 优先，见 `TERMINAL_EXITS` 头注）。
    assert_eq!(
        pick_terminal_exit_from(TERMINAL_EXITS, &|_| true),
        Some("xdg-terminal-exec")
    );
    // 只有第二个在 ⇒ 取第二个。
    // 只有它不在 ⇒ `None`（表里今天只有一个，见 `TERMINAL_EXITS` 的 D 阶段补审）。
    assert_eq!(
        pick_terminal_exit_from(TERMINAL_EXITS, &|c| c == "x-terminal-emulator"),
        None
    );
    // 一个都不在 ⇒ `None`，调用方据此诚实降级（`P5L-Y2`）。
    assert_eq!(pick_terminal_exit_from(TERMINAL_EXITS, &|_| false), None);
    // 表本身：**只准放规范化出口**，一个具名终端都不许有。
    // ⚠ **每加一个出口都要先核它的参数约定**（D 阶段补审：`--` 只对 `xdg-terminal-exec`
    // 与 ptyxis 核实过；`x-terminal-emulator` 在别的机器上可能是 `-e`）。
    assert_eq!(TERMINAL_EXITS, &["xdg-terminal-exec"]);
}

/// ★ P5L-Y1：**载荷原样进终端的参数位** —— 本件只加「怎么开窗」，不碰「开什么」。
///
/// # 🔴 前三条断言从**源码文本**换成了**行为**〔`K-H2b` `D6` 第九拍，08-29〕
///
/// 先前这三条钉的是三段源码字面（`b.arg("--").args(&argv);` ·
/// `Command::new(&argv[0])` · `match term {`）。**实打证明那不够**：
/// 在 `build_local_posix_argv` **之后**、拼 `Command` **之前**插一段剥掉
/// `export ANTHROPIC_BASE_URL=…; ` 的映射 ⇒ **三段文本一处不少、`1229 passed` 全绿**，
/// 而真正跑起来的 `bash -lic` 里没有中转注入。
/// ⇒ 那一跳抽成了纯函数 [`build_local_posix_spawn`]，本条改成**读它产出来的东西**。
/// 「分流真的看 `term`」那一格也跟着变成行为：两个入参各喂一次，答案必须不同。
///
/// ⚠⚠ **补门的代价：本条从此在 Windows 上 0 次执行**〔win-compile 09-09〕。
/// [`build_local_posix_spawn`] 带 `#[cfg(not(windows))]` 而本条**漏了对应的门**
/// ⇒ 云端（windows-latest，本仓**唯一**跑 `cargo test` 的平台）上编译失败（E0425 ×4）。
/// ⚠ 连坐的还有本条**末尾那一格源码守卫**（「降级的说明得是一条真日志」）——
/// 它本身与平台无关，却跟着这道门一起在 Windows 上不跑了。
#[cfg(not(windows))]
#[test]
fn opening_a_window_does_not_touch_the_payload() {
    let prod = guard_core::production_code(include_str!("../../src/bridge/src/launch.rs"));
    let argv: Vec<String> = ["bash", "-lic", "unset X; claude --resume s1"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    // ★ 开窗那条：`argv` **整个原样**进参数位，且前面隔着一个 `--`
    //   （否则终端会把 `bash` 之后的东西当成自己的选项解析）。
    let (program, args) = build_local_posix_spawn(Some("xdg-terminal-exec"), &argv);
    assert_eq!(program, "xdg-terminal-exec");
    assert_eq!(
        args,
        {
            let mut w = vec!["--".to_string()];
            w.extend(argv.iter().cloned());
            w
        },
        "开窗那条路没有把 `argv` **整个原样**交出去 —— \n\
             本件的全部承诺是「只加怎么开窗，不碰开什么」，这一格就是那句话本身。"
    );
    // 无窗口那条回落必须还在（`P5L-Y2`）：它是「一个出口都没有」时的唯一去处。
    let (fallback_program, fallback_args) = build_local_posix_spawn(None, &argv);
    assert_eq!(
        (fallback_program.as_str(), &fallback_args[..]),
        (argv[0].as_str(), &argv[1..]),
        "无窗口回落不再原样起 `argv[0]` —— 那样在没有规范化出口的机器上会变成「点了没反应」"
    );
    // ⚠ **分流必须真的看 `term`**〔变异 M2 逼出来的〕：先前这一格钉的是源码里有 `match term {`，
    // 而那只证明「那段代码在」，不证明「它走得到」。⇒ 换成行为：两个入参答案必须不同。
    assert_ne!(
        build_local_posix_spawn(Some("xdg-terminal-exec"), &argv),
        build_local_posix_spawn(None, &argv),
        "开窗那条分流不再按入参 `term` 走 —— 那段代码可能还在，但已经**走不到**了"
    );
    // P5L-Y2：降级必须**说清**，不是静默。
    //
    // ⚠ **钉「它是一条真日志」，不只是钉那句话在**〔变异 M4 逼出来的〕：
    // 把 `tracing::warn!` 换成 `format!`，文案原样留着，判据照样绿 ——
    // 而那时那句话**谁也看不到**。⇒ 按位置钉：文案前面不远处必须有 `tracing::warn!`。
    let at = prod
        .find("回落到无窗口直起")
        .expect("降级那条路不再说明原因 —— 用户会看到「点了没反应」而无从归因");
    let before = &prod[at.saturating_sub(200)..at];
    assert!(
        before.contains("tracing::warn!"),
        "降级的说明不是一条真日志（前 200 字节里没有 `tracing::warn!`）——\n\
             文案留着而日志没了，等于那句话谁也看不到。"
    );
}

/// ★ P5L-Y3：候选表的**理由**必须写在源码里（必需词守卫，同 `P4b-Y3` / `PS1`）。
#[test]
fn the_terminal_exit_table_says_why_it_refuses_to_pick() {
    let me = include_str!("../../src/bridge/src/launch.rs");
    // ⚠ 数次数而不是 `contains` —— 本判据自己的字面量也在这个文件里
    //（本会话第四次栽在「判据被自己要钉的名字命中」上）。
    for must in ["绝不在这里列", "交还给桌面"] {
        assert!(
            me.matches(must).count() >= 2,
            "`TERMINAL_EXITS` 的头注里少了 {must:?}。\n\
                 那段话记的是「为什么不探测具体终端」——用户逐字「暂时不考虑其他终端」。\n\
                 删掉它，下一个人就会顺手加一行 `gnome-terminal`。"
        );
    }
}

/// ★ U8b：**`launch.rs` 的生产段不许出现任何具名终端模拟器。**
///
/// 零命中型判据，钉的是 L1 那条裁决。它挡的是很自然的一个「顺手改进」：
/// 有人看到 Linux 上开不了窗，加一段 `gnome-terminal` / `alacritty` 探测。
/// 那不是清理，是**产品决定** —— 要做就先答「探测顺序是什么、找不到怎么办」，
/// 而不是静默挑一个（挑错了用户会看到一个空白窗口或什么都没有，且极难归因）。
///
/// # ★★ P5L（08-12）：那两问**答了**，于是本条放行两个**规范化出口**
///
/// `U14`〔用 08-12〕已裁「**要做，功能必须一样**」，而本条头注自己写的开锁条件
/// （「先答探测顺序 / 找不到怎么办」）逐条答完：
/// · **顺序**：`TERMINAL_EXITS` 是一张具名的有序常量（`xdg-terminal-exec` → `x-terminal-emulator`）；
/// · **找不到**：诚实降级回无窗口那条，并 `warn` 说清（`P5L-Y2`）。
///
/// ⇒ 放行的是**规范化出口**，不是终端本身：这两个都把「用哪个终端」交还给桌面/发行版配置
/// （用户逐字「**纯 bash 的意思是暂时不考虑其他终端**」反对的是终端**专属集成**）。
/// **具名终端一个都不放行** —— 那才是「挑」，也正是本条原本要挡的东西。
#[test]
fn no_terminal_emulator_is_ever_spawned_from_this_file() {
    // 运行时拼，避免命中本行自己。
    let emulators: Vec<String> = [
        "gnome-termin",
        "konsol",
        "xterm",
        "alacritt",
        "kitt",
        "wezter",
        "foot",
        "Terminal.ap",
        "iTerm",
    ]
    .iter()
    .map(|s| format!("{s}{}", ""))
    .collect();
    // 匹配器自检：独立手写的样本必须被这份名单命中（防名单写坏了导致零命中恒绿）。
    for sample in [
        "Command::new(\"gnome-terminal\")",
        "Command::new(\"alacritty\")",
        "Command::new(\"kitty\")",
    ] {
        assert!(
            emulators.iter().any(|e| sample.contains(e.as_str())),
            "匹配器漏了这种写法：{sample} —— 下面那句「零命中」对它毫无意义"
        );
    }
    let prod = guard_core::production_code(include_str!("../../src/bridge/src/launch.rs"));
    let hits: Vec<&String> = emulators
        .iter()
        .filter(|e| prod.contains(e.as_str()))
        .collect();
    assert!(
        hits.is_empty(),
        "`launch.rs` 的生产段出现了终端模拟器（{hits:?}）。\n\
             L1 裁决过：POSIX 上没有「唯一的终端」，挑一个是平白引入一个会在别人机器上错的决定。\n\
             真要做就先答「探测顺序 / 找不到怎么办」，并当成产品决定走一遍计划 —— 别静默挑一个。\n\
             ⚠ P5L 已按这条开锁条件放行了**规范化出口**（`TERMINAL_EXITS`：xdg-terminal-exec / \
             x-terminal-emulator）——它们把选择权交还给桌面，与「挑一个具名终端」是两回事。"
    );
}

/// ★★★ **第八层，第九拍自查当轮逮到并堵掉**〔`K-H2b` `D6` 回修，08-29〕。
///
/// # 它是怎么长出来的
///
/// `D6 阻-1` 那一轮把「中转前缀真的拼上去了」从**文本**换成了**行为**：
/// `history::LaunchSink` 那条缝让判据看得见 `launch_local` **真正交出去的那一串**。
/// 收工前按铁律 15 问「第八层会怎么绕过这一轮」时，最像的一条是
/// **在缝的下游动手** —— 本函数正是那条缝的**下游第一跳**。
///
/// **实打（刀 `Z1`）**：在 [`build_local_posix_argv`] 里把
/// `export ANTHROPIC_BASE_URL=…; ` 这一段从命令串里剥掉（**只剥这一段，别的一字不动**）
/// ⇒ **`1228 passed; 0 failed` 全绿。**
/// 上游那条缝的判据照绿（它量的是**交给**本函数的那一串），
/// 而真正跑起来的 `bash -lic` 里**没有中转注入** —— 与本件没做完全一样。
///
/// # 为什么隔壁那条判据挡不住
///
/// `local_and_remote_share_the_same_payload` 断的正是「逐字节透传」，
/// 而它**只喂一个 payload，且那个 payload 里没有中转前缀**
/// ⇒ 一把**只针对前缀**的剪刀从它下面走过去。
/// ★ 这与 `D6 阻-2` 是**同一族病**：**输入域 = 1**（「形状对、恒答其中一张脸」）。
///
/// ⇒ 本条把输入域打开：**三个 payload，两个带中转前缀、两个不同的账号**。
///
/// # ⚠ 它买不到什么（射程边缘，如实写）
///
/// 本条量的是**纯函数这一跳**。`launch_local_posix_via` 拿到 `argv` **之后**再动手
/// （在 `Command` 上改参数）本条看不见 —— 刀 `Z1c` / `L10` 打的正是那 3 行。
/// 🔴 **订正〔`D7 阻-1`/`阻-2`，08-29〕：这里先前逐字写着「那一跳要真开窗才观测得到」
/// —— 假话。** 那 3 行今天由**两条**判据看着，一支一条：
/// `the_spawned_process_really_gets_the_relay_prefix_without_a_terminal`（`term = None`）
/// 与 `the_terminal_we_hand_the_command_to_really_gets_the_relay_prefix`（`term = Some(假终端)`），
/// **两条都不开窗**。
///
/// ⚠⚠ **补门的代价 —— 这一条最贵，逐字写清**〔win-compile 09-09〕。
/// 本条自述钉的是**第八层**那一整跳（命令串 → 真正要 spawn 的 `(program, args)`），
/// 而它用的 `local_posix_spawn_plan` 带 `#[cfg(not(windows))]`、本条**漏了对应的门**
/// ⇒ 云端（windows-latest，本仓**唯一**跑 `cargo test` 的平台）上编译失败（E0425 ×2）。
/// 补门之后**本条在 CI 上 0 次执行** —— 也就是说「第八层被堵住了」这句话，
/// 今天**只有开发者的 POSIX 盘证明得了**，云端一次都没证明过。
/// ★ 这不是新缺陷（Windows 上本来就没有 POSIX 那条送法），但**它也不是没有代价**：
///   把「补门」读成「修好了」是错的，红消掉了、覆盖没回来。
#[cfg(not(windows))]
#[test]
fn the_local_argv_hands_the_command_through_byte_for_byte_prefix_and_all() {
    let payloads = [
        // ① 不带中转前缀的那一形（隔壁那条判据今天只喂这一形）。
        "unset CLAUDE_CONFIG_DIR; ccm --tmux claude --resume s1",
        // ② 带中转前缀 —— 第八层就长在「没人喂这一形」上。
        "export ANTHROPIC_BASE_URL='http://127.0.0.1:8788/s/claude-code/acct-a/sid-1'; \
             export CLAUDE_CONFIG_DIR='/h/.claude-accts/acct-a'; claude --resume s1",
        // ③ **另一个号** —— 「哪个号」这一维也不许在这一跳上被抹平。
        "export ANTHROPIC_BASE_URL='http://127.0.0.1:8788/s/claude-code/acct-b/sid-2'; \
             export CLAUDE_CONFIG_DIR='/h/.claude-accts/acct-b'; claude --resume s2",
    ];
    // 反空真：这三形本来就该是三个不同的串（否则下面三条里有两条是同一条）。
    assert_eq!(
        payloads
            .iter()
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        3,
        "三个 payload 里有重复 —— 本条的输入域没有真的打开"
    );
    for payload in payloads {
        let argv = build_local_posix_argv(payload).expect("这三形都该通过校验");
        assert_eq!(
            argv,
            vec!["bash".to_string(), "-lic".to_string(), payload.to_string()],
            "\n★★ **送出去的那一串在这一跳上被改了** —— 这是第八层的形状：\n\
                 上游那条缝（`history::LaunchSink`）的判据量的是**交给本函数的那一串**，\n\
                 本函数再剥掉一段，两边都不红，而真正跑起来的 `bash -lic` 里没有中转注入。\n\
                 实得 argv：{argv:?}"
        );
        // ★★ **整跳钉住**：命令串 → 真正要 spawn 的 `(program, args)`。
        //    先前 `launch_local_posix_via` 里 `build_local_posix_argv` 与拼 `Command`
        //    之间隔着一跳**没有任何判据** ⇒ 在那里剥前缀实测全绿（第八层的下游版本）。
        let (program, args) =
            local_posix_spawn_plan(payload, Some("xdg-terminal-exec")).expect("该通过校验");
        assert_eq!(program, "xdg-terminal-exec");
        assert_eq!(
            args,
            vec![
                "--".to_string(),
                "bash".to_string(),
                "-lic".to_string(),
                payload.to_string()
            ],
            "开窗那条路上载荷被改了 —— `--` 之后必须是 argv **逐字节原样**"
        );
        // 无窗口回落那一支：同一份载荷，只是少了终端那一层包装。
        let (program, args) = local_posix_spawn_plan(payload, None).expect("该通过校验");
        assert_eq!(program, "bash");
        assert_eq!(
            args,
            vec!["-lic".to_string(), payload.to_string()],
            "无窗口回落那条路上载荷被改了"
        );
    }
}

/// ★ L1 的验收判据（主计划 §2「关键判断」第 1 条逐字）：
/// **给同一个 plan 换 transport，除 ssh 包装外输出逐字节相同。**
///
/// 这条测试就是那句话的机器版：把远端命令体里的 ssh 包装层层剥掉，
/// 剩下的必须与本地 argv **逐字节**相等。任何一侧偷偷加/减修饰都会红。
#[test]
fn local_and_remote_share_the_same_payload() {
    let payload = "unset CLAUDE_CONFIG_DIR; cd '/home/z/p' && ccm --tmux claude --resume s1";
    let local = build_local_posix_argv(payload).unwrap();
    assert_eq!(
        local,
        vec!["bash", "-lic", payload],
        "本地：直接 exec，无 ssh 包"
    );

    let c = cfg("h", "u", 22, None);
    let remote = build_remote_ssh_ps_command(&c, payload).unwrap();
    // 剥 ssh 层 → 剥 PS 单引号层 → 得到与本地同构的 `bash -lic <quoted>`
    let after_dashdash = remote.rsplit_once("-- ").unwrap().1;
    let inner = after_dashdash
        .strip_prefix('\'')
        .and_then(|s| s.strip_suffix('\''))
        .expect("ps quoted")
        .replace("''", "'");
    assert_eq!(
        inner,
        format!("bash -lic {}", posix_quote(payload)),
        "远端：同一个 payload，只多了 ssh + PS 两层包装"
    );
    // ★ 逐字节：把远端**每一层包装都反解**之后，得到的必须就是本地那一串。
    //（不能写成 `inner.contains(payload)` —— payload 里有单引号，`posix_quote`
    //  会把它变成 `'\''`；那条会误报，而它误报说明的恰恰是「包装确实存在」。）
    let unwrapped = inner
        .strip_prefix("bash -lic '")
        .and_then(|s| s.strip_suffix('\''))
        .expect("bash -lic 层")
        .replace(r"'\''", "'");
    assert_eq!(
        unwrapped, local[2],
        "剥净包装后，两条路送的是同一串（逐字节）"
    );
}

/// 与传输无关的三条校验，本地那条路**同样**生效（不是只有远端才验）。
#[test]
fn local_argv_shares_the_transport_agnostic_validation() {
    assert!(build_local_posix_argv("   ").is_err(), "空命令");
    assert!(
        build_local_posix_argv(&"x".repeat(MAX_REMOTE_CMD + 1)).is_err(),
        "超长"
    );
    assert!(build_local_posix_argv("a\nb").is_err(), "控制字符");
    assert!(build_local_posix_argv("claude --resume s1").is_ok());
}

/// ★ 「拒绝双引号」是 **PowerShell 5.1 的怪癖**，不是命令本身的性质
/// ⇒ 它只该拦远端那条路，**不该**跟着搬到 POSIX 本地。
///
/// 判据落在性质上，不落在表面特征上：把一个 Windows 传参畸变套到 Linux 上，
/// 会让本地路径无端拒绝一批合法命令。
#[test]
fn double_quote_rejection_is_powershell_only() {
    let with_dq = r#"claude --append-system-prompt "be brief""#;
    assert!(
        build_remote_ssh_ps_command(&cfg("h", "u", 22, None), with_dq).is_err(),
        "远端（走 PowerShell）应拒"
    );
    assert!(
        build_local_posix_argv(with_dq).is_ok(),
        "POSIX 本地不经 PowerShell，不该拦"
    );
}

/// ★ L1：`launch_local_posix` 的 **spawn 那半**真的会跑起来（此前无覆盖）。
///
/// 用一条无害命令写一个标记文件来观测。**刻意不起任何 agent**。
/// 它不是 hermetic 的（`bash -lic` 会 source 用户 rc）—— 但要验的正是
/// 「按我们给的 argv 真的 exec 了」，而 rc 的存在恰恰是生产形态的一部分。
#[cfg(not(windows))]
#[test]
fn local_posix_spawn_actually_runs_the_command() {
    let dir = std::env::temp_dir().join(format!("l1-spawn-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("mkdir");
    let marker = dir.join("ran");
    let cmd = format!("printf ok > {}", marker.display());
    // ★ P5L：本条走 `None`（无窗口）那条 —— **别在这里喂一个真出口**，
    // 否则它会在开发者桌面上**弹出一个真终端窗口**（实测跑过一次，本机
    // `xdg-terminal-exec` → `ptyxis`）。
    // ⚠ 订正〔`D7 阻-2`〕：这一行先前跟着一句「开窗那条路只有形状判据」—— 今天是假话。
    // 开窗那一支的 spawn 由 `the_terminal_we_hand_the_command_to_really_gets_the_relay_prefix`
    // 用一个**假终端脚本**买到了（不弹窗）；这里传 `None` 只是因为**本条**要的是回落那一支。
    launch_local_posix_via(&cmd, dir.to_str(), None).expect("spawn 应成功");
    // 轮询等它落地（spawn 是异步的；上限宽松，判的是「跑没跑」不是快慢）。
    let mut seen = false;
    for _ in 0..100 {
        if marker.is_file() {
            seen = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    let content = if seen {
        std::fs::read_to_string(&marker).unwrap_or_default()
    } else {
        String::new()
    };
    let _ = std::fs::remove_dir_all(&dir);
    assert!(seen, "5s 内没看到标记文件——spawn 那半没真跑");
    assert_eq!(content, "ok", "命令跑了但内容不对");
}

// ═════════════════════════════════════════════════════════════════════════
// `D7 阻-1` / `D7 阻-2`：**这条链的分叉点是 `term`，两支都要买**
// ═════════════════════════════════════════════════════════════════════════
//
// 病史十层，每一层的形状都是同一个：**在这条链上找一跳，那一跳没有判据看着**。
// 链：`history::launch_local` → 拼前缀 → `launch_local_posix` → `launch_local_posix_via`
//   → `local_posix_spawn_plan`（纯函数）→ **`Command::new(program).args(args).spawn()`**。
// 前九层堵到了纯函数那一跳为止；第九层（刀 `Z1c`）与第十层（刀 `L10`）都长在
// **纯函数返回之后那 3 行**上，差别只在 `L10` **只在 `term.is_some()` 那一支动手**。
//
// 🔴 **`L10` 之所以比 `Z1c` 更坏**：它活过「加一条走 `term = None` 的真起判据」这个
//    最便宜的修法，而**生产上装了规范化终端出口的机器（= 有桌面的真实用户）走的正是
//    `term = Some(...)` 那一支**。
//
// ⇒ 下面**两条**判据，一支一条，都在**真正被 spawn 出去的那个进程**上观测：
//    ㈠ `term = None` ⇒ 观测点 = 起出去的那个 `bash` **自己看到的 `ANTHROPIC_BASE_URL`**；
//    ㈡ `term = Some(<假终端>)` ⇒ 观测点 = 那个终端**自己收到的 argv**。
//
// ⚠ **不起真终端、不弹任何窗口**：`term` 是入参 ⇒ 判据喂一个自己造的
//   `#!/bin/sh` 脚本（把 `"$@"` 写进文件），`Command::new(program)` 拿绝对路径直接 exec 它。
//   这与「真开一个窗口再去看进程 argv」是两回事 —— 先前头注里写的「要真开窗才买得到」
//   只对**那一种**判据成立，`D7` 实测打穿过（`§A2`）。
// ⚠ 两条都**不是 hermetic** 的（`bash -lic` 会 source 用户 rc）——
//   而这笔钱仓里今天已经在付（`local_posix_spawn_actually_runs_the_command` 就这么写的，
//   且它不在 `10 ignored` 里）⇒ **不是一笔新代价**。
// ⚠ **刻意不起任何 agent**：载荷只有一条 `printf`。

/// 造一份只属于这一条判据的临时目录（名字取中性名，**不进任何断言**）。
#[cfg(not(windows))]
fn scratch_dir(tag: &str) -> std::path::PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let dir = std::env::temp_dir().join(format!("l1-{tag}-{}-{nanos}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("mkdir");
    dir
}

/// 轮询等一个文件落地（spawn 是异步的；上限宽松，判的是「有没有」不是快慢）。
#[cfg(not(windows))]
fn wait_for(path: &std::path::Path) -> bool {
    for _ in 0..100 {
        if path.is_file() {
            // 再等一拍，避免读到写了一半的内容。
            std::thread::sleep(std::time::Duration::from_millis(20));
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    false
}

/// 本件的中转前缀 —— 由**生产那一处**渲染器产出，判据不自己抄一份字面量。
#[cfg(not(windows))]
fn relay_probe_prefix(url: &str) -> String {
    crate::backend::control::payload::relay_env_prefix_posix(url)
}

// ─────────────────────────────────────────────────────────────────────────
// ★★ `K-R24`：**exec 一个自己刚写出来的文件，前提是「这一刻没人握着它的写 fd」**
//    —— 而这条前提，先前**没人建立、也没人检查**。
//
// # 病历（PM 09-04 夜实打，同一棵树同一个提交两趟）
//
// 第一趟开窗那条判据红，报文逐字
// `spawn 假终端应成功: "spawn 本地命令失败: Text file busy (os error 26)"`；
// 第二趟全绿。⇒ 一个读数装着**两件事**：
//   ① 开窗那一支的 spawn 真的坏了（真缺陷）；
//   ② 这一趟 exec 撞上了 `ETXTBSY`（环境时序，与被测那一半无关）。
// ★ **这正是 `K-R24` 的正题（一个值装了几件事），长在我自己这条判据上。**
//
// # 机制：为什么会有别人握着写 fd
//
// `ETXTBSY` 的定义就是「execve 的目标文件此刻正被某个进程打开着写」。
// 本进程自己那把写句柄在 `install_fake_terminal` 里**确定性地关掉了**（见那里）。
// 剩下的唯一来路是**别的线程**：这个测试二进制的生产段里有 40+ 处真起进程的调用点
//（`write_site_registry` 那张 `SPAWNS` 表逐条登记着），起进程要么 fork 要么 posix-spawn，
// 两者都把父进程此刻打开的 fd **复制一份给子进程**，而 close-on-exec 要到子进程
// **exec 那一刻**才生效 ⇒ 「fork 之后、exec 之前」那段窗口里，那个子进程就是
// **一个握着我们这个文件写 fd 的进程**。我们此刻 execve 它 = `ETXTBSY`。
// ★ 同一个成因在别的工具链上是有名的：go 与 cargo 都是靠**对 `ETXTBSY` 重试**收的。
//
// # 本段把那一个读数拆成三个（这才是本件的正题）
//
// | 坏法 | 红的是哪一句 |
// |---|---|
// | ① 前提没建立（上限内一直 `ETXTBSY`） | 「前提不成立：exec 那一刻有人握着…… 这条今天判不了」 |
// | ② spawn 那一半真坏了（别的错） | 「开窗那一支的 spawn 真的失败了（不是 ETXTBSY）」 |
// | ③ 夹具没装好（没有执行位） | 「假终端没有执行位 —— 红的是夹具，不是被测那一半」 |
// ─────────────────────────────────────────────────────────────────────────

/// 允许撞几次 `ETXTBSY` 才算「这条前提建不起来」。
///
/// 🔴 **它不是「睡够久就当没事」**：上限到了**照样红**，只是红的那句话换成
/// 「前提不成立」。50 × 20ms ≈ 1s，而它要等的那个窗口（别人的 fork 到 exec）
/// 在实测里是亚毫秒级 —— 这个上限是**宽的**，不是**紧的**。
#[cfg(not(windows))]
const FAKE_TERM_ETXTBSY_TRIES: u32 = 50;

/// 起那个假终端时，「没人握着它的写 fd」这条前提的三种结局。
#[cfg(not(windows))]
#[derive(Debug)]
enum FakeTermSpawn {
    /// 起成功了 —— 前提成立。
    Ok,
    /// 上限内一直 `ETXTBSY` ⇒ **前提没建立**，这条今天判不了。
    PremiseUnmet(String),
    /// 别的错 ⇒ **spawn 那一半真的坏了**。
    Broken(String),
}

/// 一条 spawn 错误是不是 `ETXTBSY`。
///
/// 🔴 **认的是 `os error 26` 那一半，不是 `Text file busy` 那一半**：
/// 后半句由 C 库按 `LC_MESSAGES` 打（glibc 有中文翻译），拿它当判据等于给这条判据
/// 再挂一条**隐式的 locale 前提** —— 而那正是本件在治的病。前半句是 errno 的十进制，
/// 与 locale 无关。
#[cfg(not(windows))]
fn spawn_error_is_etxtbsy(err: &str) -> bool {
    err.contains("os error 26")
}

/// 写一个「只记录、不执行」的假终端脚本，并**把前提真的建立起来**。
///
/// 两半都在这里：
/// - **写句柄**：具名句柄 + `sync_all` + 显式 drop。先前这里是 `std::fs::write`，
///   它也会关，但那是**实现细节** —— 读的人看不出「关掉写 fd」是这条判据的前提之一。
///   本件要买的正是「**让前提看得见**」。
/// - **执行位**：加完当场读回来核一遍（它是确定性的，所以直接断言，不重试）。
#[cfg(not(windows))]
fn install_fake_terminal(path: &std::path::Path, body: &str) {
    use std::io::Write as _;
    use std::os::unix::fs::PermissionsExt;
    let mut fh = std::fs::File::create(path).expect("建假终端脚本");
    fh.write_all(body.as_bytes()).expect("写假终端脚本");
    fh.sync_all().expect("把假终端脚本落到盘上");
    drop(fh);
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))
        .expect("给假终端脚本加执行位");
    let mode = std::fs::metadata(path)
        .expect("读回假终端脚本的权限")
        .permissions()
        .mode();
    assert!(
        mode & 0o111 != 0,
        "假终端没有执行位（mode={mode:o}）—— 红的是夹具，不是被测那一半"
    );
}

/// 起那个假终端，并把「前提没建立」与「spawn 坏了」**分成两个读数**。
///
/// `tries` = 允许撞几次 `ETXTBSY`。⚠ 只对 `ETXTBSY` 重试；**别的错一次都不重试**
/// （重试一个真缺陷 = 把它变成偶尔绿的偶发红，那比今天更糟）。
#[cfg(not(windows))]
fn spawn_fake_terminal(
    payload: &str,
    cwd: Option<&str>,
    term: Option<&str>,
    tries: u32,
) -> FakeTermSpawn {
    let mut last = String::new();
    for i in 0..tries.max(1) {
        match launch_local_posix_via(payload, cwd, term) {
            Ok(()) => return FakeTermSpawn::Ok,
            Err(e) if spawn_error_is_etxtbsy(&e) => {
                // 🔴 **出声**（本波派工单逐字要的那一格）：重试**不许静默** ——
                //    静默的重试会让「这条前提今天被破了几次」变成一个**没人量得到的数**，
                //    而那正是本件在治的病换个地方长。这一行同时是量具的读数来源
                //   （`tests/evidence/K-R24-D7-load-axis-stress.py` 数的就是它）。
                eprintln!(
                    "[K-R24] 前提被破了一次：exec 假终端撞上 ETXTBSY（第 {} 次），\
                         上限 {tries} 次内重试；逐字：{e}",
                    i + 1
                );
                last = format!("撞了 {} 次，最后一次逐字：{e}", i + 1);
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            Err(e) => return FakeTermSpawn::Broken(e),
        }
    }
    FakeTermSpawn::PremiseUnmet(last)
}

/// ★★★ `D7 阻-1`（第九层，刀 `Z1c`）：**`term = None` 那一支上，
/// 真正被 spawn 出去的那个进程拿到了中转注入。**
///
/// # 它买的是哪一格
///
/// `local_posix_spawn_plan` 返回之后还剩 3 行（`Command::new` · `.args(&args)` ·
/// 那几个 `cwd`/`env`/`stdio`/`process_group` 设置）。在那 3 行里再剥一次前缀，
/// **纯函数一字不动、全仓锚点一处不少**，而 `D7` 实打 `1229 passed` 全绿。
/// ⇒ 本条把观测点挪到**进程自己**：起出去的那条命令 `printf "$ANTHROPIC_BASE_URL"`，
/// 它写下来的那一串必须**逐字节等于**我们注入的那个 URL。
///
/// # ⚠ 它买不到什么
///
/// - **开窗那一支**（`term = Some(...)`）本条一格都不走 —— 那一支由下面那条买
///   （刀 `L10` 只在开窗支动手时本条**不红**，实测如此，别把两条读成一条）。
/// - **claude 拿到这个变量之后的真实行为** —— 红线「绝不起真 claude」，原样在「判不了」里。
#[cfg(not(windows))]
#[test]
fn the_spawned_process_really_gets_the_relay_prefix_without_a_terminal() {
    let dir = scratch_dir("nowin");
    let seen = dir.join("what-the-process-saw");
    let url = "http://127.0.0.1:8788/s/claude-code/acct-a/sid-1";
    let prefix = relay_probe_prefix(url);
    // 反空真①：前缀本来就该非空且真的是一段 env 注入，否则下面整条是「空 == 空」。
    assert!(
        prefix.contains("ANTHROPIC_BASE_URL") && !prefix.is_empty(),
        "生产那一处渲染器没渲出中转注入 —— 本条按红处理：{prefix:?}"
    );
    let payload = format!(
        "{prefix}printf '%s' \"$ANTHROPIC_BASE_URL\" > {}",
        seen.display()
    );
    // 反空真②：观测点在**进程那一侧**，起手它必须不存在。
    assert!(!seen.exists(), "起手观测文件就在了 —— 本条会读到上一趟的痕");

    launch_local_posix_via(&payload, dir.to_str(), None).expect("spawn 应成功");

    let landed = wait_for(&seen);
    let got = if landed {
        std::fs::read_to_string(&seen).unwrap_or_default()
    } else {
        String::new()
    };
    let _ = std::fs::remove_dir_all(&dir);
    assert!(landed, "5s 内那个进程什么都没写下来 —— spawn 那半没真跑");
    assert_eq!(
        got, url,
        "\n★★ **起出去的那个进程没拿到中转注入** —— 这是第九层（刀 `Z1c`）的形状：\n\
             `local_posix_spawn_plan` 返回之后那 3 行里把 `export ANTHROPIC_BASE_URL=…; `\n\
             从 argv 里剥掉，纯函数一字不动、全仓锚点一处不少，而上一版全绿。\n\
             生产后果：claude 直连官方端点，第三方 key 用不上，而门禁四个数一格不动。\n\
             实得 = {got:?} · 期望 = {url:?}"
    );
}

/// ★★★ `D7 阻-2`（第十层，刀 `L10`）：**`term = Some(...)` 那一支上，
/// 我们真正交给终端的那份 argv 带着中转注入。**
///
/// # 🔴 为什么这一支非买不可（它比第九层更坏一格）
///
/// 刀 `L10` = 把剥前缀那一手**只放在 `term.is_some()` 那一支**：
/// ⇒ `D7` 实打 `1229 passed; 0 failed` + `GATE: OK`，四个数与干净树逐字相同，
/// **而且它活过第九层最便宜的修法**（装上那条走 `term = None` 的真起判据之后仍然绿）。
/// **生产上这台机器走的正是这一支**（`pick_terminal_exit()` 在 `PATH` 里找
/// `xdg-terminal-exec`，现打它在 `/usr/bin/` 里）⇒ `L10` 的后果是
/// **在有桌面的真实用户机器上中转注入被整个剥掉**，而判据今天唯一走过的那条
/// （无窗口回落）恰恰是那些机器上走不到的。
///
/// # 怎么在**不开窗**的前提下观测它
///
/// `term` 是入参（`P5L` 做的），⇒ 喂一个**自己造的假终端**：临时目录里一个
/// `#!/bin/sh` 脚本，把 `"$@"` 逐行写进一个文件。`Command::new(program)` 拿到的是
/// 它的绝对路径 ⇒ 直接 exec，**没有任何窗口会弹出来**。
/// 断言它收到的 argv **逐格等于** `["--", "bash", "-lic", <带前缀的命令串>]`。
///
/// ⚠ 那个假终端**只记录、不执行** ⇒ 这一趟里载荷一个字都没跑（更不会起 agent）。
///
/// # ⚠ 它买不到什么
///
/// - **真终端拿到 argv 之后会不会照着跑** —— 那是 `xdg-terminal-exec` 自己的约定，
///   本条只买「我们交出去的那一份是对的」。真机验收归 `auto-e2e`。
/// - **无窗口回落那一支**由上面那条买（刀 `Z1c` 只在 `term = None` 支动手时本条不红）。
#[cfg(not(windows))]
#[test]
fn the_terminal_we_hand_the_command_to_really_gets_the_relay_prefix() {
    let dir = scratch_dir("win");
    let recorded = dir.join("what-the-terminal-got");
    let fake_term = dir.join("record-and-exit");
    install_fake_terminal(
        &fake_term,
        &format!(
            "#!/bin/sh\nfor a in \"$@\"; do printf '%s\\n' \"$a\"; done > '{}'\n",
            recorded.display()
        ),
    );

    let url = "http://127.0.0.1:8788/s/claude-code/acct-b/sid-2";
    let prefix = relay_probe_prefix(url);
    // 反空真①：同上，前缀本来就该非空。
    assert!(
        prefix.contains("ANTHROPIC_BASE_URL") && !prefix.is_empty(),
        "生产那一处渲染器没渲出中转注入 —— 本条按红处理：{prefix:?}"
    );
    let payload = format!("{prefix}printf ok");
    // 反空真②：观测点起手必须不存在。
    assert!(
        !recorded.exists(),
        "起手观测文件就在了 —— 本条会读到上一趟的痕"
    );

    // ★★ `K-R24`：这一处先前是 `.expect("spawn 假终端应成功")` —— 一个读数装着
    //    「spawn 坏了」与「exec 那一刻撞上 `ETXTBSY`」两件事。现在两件各有各的话。
    match spawn_fake_terminal(
        &payload,
        dir.to_str(),
        fake_term.to_str(),
        FAKE_TERM_ETXTBSY_TRIES,
    ) {
        FakeTermSpawn::Ok => {}
        FakeTermSpawn::PremiseUnmet(m) => {
            let _ = std::fs::remove_dir_all(&dir);
            panic!(
                "\n前提不成立：**exec 那一刻一直有人握着这个假终端的写 fd**（`ETXTBSY`）——\n\
                     成因是这个测试二进制里别的线程正卡在「起进程」的 fork 与 exec 之间，\n\
                     它继承了本判据写这个脚本时那把写 fd。\n\
                     ⇒ **这条今天判不了**，它**不是**「开窗那一支的 spawn 坏了」。\n\
                     {m}"
            );
        }
        FakeTermSpawn::Broken(m) => {
            let _ = std::fs::remove_dir_all(&dir);
            panic!(
                "\n★★ **开窗那一支的 spawn 真的失败了**（不是 `ETXTBSY`，所以不是前提问题）：\n\
                     {m}"
            );
        }
    }

    let landed = wait_for(&recorded);
    let got: Vec<String> = if landed {
        std::fs::read_to_string(&recorded)
            .unwrap_or_default()
            .lines()
            .map(str::to_string)
            .collect()
    } else {
        Vec::new()
    };
    let _ = std::fs::remove_dir_all(&dir);
    assert!(
        landed,
        "5s 内那个终端什么都没写下来 —— 开窗那一支的 spawn 没真跑"
    );
    assert_eq!(
        got,
        vec![
            "--".to_string(),
            "bash".to_string(),
            "-lic".to_string(),
            payload.clone(),
        ],
        "\n★★ **交给终端的那份 argv 在最后 3 行里被改了** —— 这是第十层（刀 `L10`）的形状：\n\
             把剥前缀那一手**只放在 `term.is_some()` 那一支**，\n\
             ⇒ 走 `term = None` 的判据一格不动、全量门禁四个数与干净树逐字相同，\n\
             而**生产上装了规范化终端出口的机器走的正是这一支**。\n\
             实得 = {got:?}"
    );
}

/// ★★★ `K-R24`：**「前提没建立」与「spawn 坏了」真的是两个读数** —— 就地断言出来。
///
/// # 它买的是哪一格
///
/// 上面那条开窗判据在 `ETXTBSY` 上偶发红过（病历见 `spawn_fake_terminal` 上方那一段）。
/// 修法是「只对 `ETXTBSY` 重试、上限到了换一句话红」，而**这个修法自己也需要一条判据**：
/// 若哪天有人把分类器写宽（比如认成 `Broken` 一律重试、或反过来把真缺陷认成前提问题），
/// **今天没有任何东西会红**。本条就是那条。
///
/// # 台子怎么搭的（不靠猜，靠 `ETXTBSY` 的定义）
///
/// `ETXTBSY` 的定义是「execve 的目标此刻被某个进程打开着写」。⇒ 本条**自己攥一把写句柄**
/// 不放，那一刻 execve 必然是它。三条腿，**两侧都堵**：
/// - **反空真 / 非空对照**：不攥的时候它**必须起得来** —— 否则下面那个「认成前提不成立」
///   可能只是「这条路根本起不来」的恒答，那是空真。
/// - **正题（这一侧）**：攥着的时候，得到的必须是 `PremiseUnmet` 且成因逐字带着 errno，
///   **不是** `Broken`（那就又把两件事塞回一个读数了）。
/// - **反侧**：一个**不是** `ETXTBSY` 的真失败（拿一个没有执行位的文件当终端 ⇒ 权限被拒）
///   必须读成 `Broken`。🔴 少了这一条，一个「一律认成前提问题」的退化分类器**照样全绿**，
///   而它的后果是**把真缺陷说成「今天判不了」并且重试它** —— 比今天更糟。
///
/// # ⚠ 它买不到什么
///
/// - **它复现的不是真实那条时序**：真实成因是别的线程 fork 出来的子进程**短暂**继承了写 fd，
///   本条是**自己长时间攥着**。两者对 execve 是同一件事（都是「有人开着写」），
///   但本条**不证明**那条 fork 竞态真的发生过 —— 那一格由病历里那两趟读数与
///   `tests/evidence/K-R24-D7-load-axis-stress.py` 那份量具承重，如实登记。
/// - **不证明重试上限选得对**：上限是宽的，那是取舍，不是判据。
#[cfg(not(windows))]
#[test]
fn an_open_write_handle_reads_as_an_unmet_premise_not_as_a_broken_spawn() {
    let dir = scratch_dir("etxtbsy");
    let fake_term = dir.join("record-and-exit");
    install_fake_terminal(&fake_term, "#!/bin/sh\nexit 0\n");
    let payload = "printf ok";

    // 腿①（非空对照）：没人攥写句柄时，这条路本来就该起得来。
    let clean = spawn_fake_terminal(
        payload,
        dir.to_str(),
        fake_term.to_str(),
        FAKE_TERM_ETXTBSY_TRIES,
    );
    assert!(
        matches!(clean, FakeTermSpawn::Ok),
        "干净时就起不来 ⇒ 下面那一格是空真（它会恒答「前提不成立」）：{clean:?}"
    );

    // 腿②（正题）：把前提**主动破掉** —— 攥一把写句柄不放。
    let held = std::fs::OpenOptions::new()
        .write(true)
        .open(&fake_term)
        .expect("攥住假终端的写句柄");
    let got = spawn_fake_terminal(payload, dir.to_str(), fake_term.to_str(), 2);
    drop(held);

    // 腿③（反侧）：一个**不是** `ETXTBSY` 的真失败必须读成 `Broken`。
    //
    // 🔴 台子取的是**一个根本不存在的路径**，而不是「一个存在但没有执行位的文件」——
    //    〔本轮自查，第 15 条：拿本轮的病理回头打自己的代码〕第一版正是后者，
    //    而后者**自己就带着本件在治的那条前提**：那个文件也是这一趟刚写出来的，
    //    它也可能被别的线程 fork 出来的子进程握着写 fd ⇒ 那一趟拿到的会是 errno 26
    //    而不是权限被拒 ⇒ **这条腿自己变成偶发红**。
    //    不存在的路径**不可能被谁开着写** ⇒ 这一腿的前提是恒成立的，不需要建立也不需要检查。
    let missing = dir.join("no-such-terminal-here");
    assert!(
        !missing.exists(),
        "这条腿要一个**不存在**的路径，它却在：{missing:?}"
    );
    let refused = spawn_fake_terminal(payload, dir.to_str(), missing.to_str(), 2);

    let _ = std::fs::remove_dir_all(&dir);

    match got {
        FakeTermSpawn::PremiseUnmet(m) => assert!(
            m.contains("os error 26"),
            "认成了「前提不成立」，可成因不是 `ETXTBSY` ⇒ 分类器认的是别的东西：{m}"
        ),
        other => panic!(
            "\n★★ 有人攥着写 fd 时 execve 必是 `ETXTBSY`，而这条判据把它读成了 {other:?}\n\
                 ⇒ 「前提没建立」又和别的坏法共用一个读数了 —— 那正是 `K-R24` 的正题。"
        ),
    }
    assert!(
        matches!(refused, FakeTermSpawn::Broken(_)),
        "\n★★ 一个**没有执行位**的终端是**真失败**，不是「前提不成立」，而这条判据把它读成了 {refused:?}\n\
             ⇒ 分类器退化成了「一律认成前提问题」：真缺陷会被说成「今天判不了」并被重试。"
    );
}

/// ★★★ **这条链上今天最后一跳**：`launch_local_posix` 那一行包装〔第八轮自查，08-29〕。
///
/// # 它是我这一轮自己找出来的第十一层（先量了，再堵）
///
/// 上面那两条判据（两支各一条）驱动的是 [`launch_local_posix_via`]，
/// 而生产的入口是它外面那一行包装 [`launch_local_posix`]：
/// ```text
/// launch_local_posix_via(cmd, cwd, pick_terminal_exit())
/// ```
/// 🔴 **在那一行之前把前缀剥掉**（`let cmd = …strip…;`），实打
/// **`1232 passed; 0 failed` 全绿** —— 两条新判据都直调 `_via`，看不见它；
/// 而 `history` 那条缝量的是**交给送法之前**那一串，也看不见它。
/// 锚点 `launch_local_posix_via(cmd, cwd, pick_terminal_exit())` 全仓 **1**，切前切后都是 1。
/// **生产后果**：**两条支路上中转注入一起没了**（比 `L10` 还宽一格）。
///
/// # 为什么它只能由一条**文本**判据看着（如实登记，不假装钉住了）
///
/// 要行为地驱动它就得调 [`launch_local_posix`] 本身，而它里面那个
/// [`pick_terminal_exit`] 会在**装了规范化终端出口的机器上真的弹出一个窗口**
///（本机 `/usr/bin/xdg-terminal-exec` 现打存在）—— 红线逐字禁这一形。
/// ⇒ 本条钉的是**形状**：那个函数体里**只许有那一行**。
///
/// **绕过形态**（写清楚，别读宽）：
/// - 把剥前缀塞进 [`pick_terminal_exit`] 是**没用的**（它不碰 `cmd`）；
/// - 但把剥前缀塞进 [`build_local_posix_argv`] / `build_local_posix_spawn` 的**上游调用方**
///   （比如 `history::launch_local` 与本函数之间新长出来的一跳）**本条看不见** ——
///   那一跳今天不存在，一旦长出来，`PRODUCTION_LAUNCH_SINK` 的地址对拍会先红。
/// - **本条自己的绕过形态**：把那一行改写成一个语义相同、字面不同的表达式
///   （例如换行、加类型标注）⇒ 本条**假红**，不是假绿 —— 方向是 fail-closed。
///
/// **重新裁定的落点**：本栏。要把这一跳也变成行为，得把「挑终端出口」也做成入参
///（再套一层 `_using` 形），而那只会把同一个问题往外挪一层 —— 值不值得由 PM 裁。
#[cfg(not(windows))]
#[test]
fn the_thin_wrapper_hands_the_command_straight_through_to_the_via_form() {
    let prod = guard_core::production_code(include_str!("../../src/bridge/src/launch.rs"));
    // 锚点必须**唯一**：Windows 那一份的形参带下划线前缀（`_cmd` / `_cwd`）⇒ 签名不同。
    let head = "pub fn launch_local_posix(cmd: &str, cwd: Option<&str>) -> Result<(), String> {";
    let at = guard_core::find_pinned(&prod, head)
        .unwrap_or_else(|e| panic!("`launch_local_posix` 的签名不是恰好一处 —— 先修锚点：{e}"));
    // 花括号配平切体（本文件唯一一处切块，刻意不另造第二种切法）。
    let bytes = prod.as_bytes();
    let open = at + head.len() - 1;
    let mut depth = 0i32;
    let mut end = bytes.len();
    for i in open..bytes.len() {
        if bytes[i] == b'{' {
            depth += 1;
        } else if bytes[i] == b'}' {
            depth -= 1;
            if depth == 0 {
                end = i;
                break;
            }
        }
    }
    let body = &prod[open + 1..end];
    // 反空真：切到了东西（切错了就红，别在一个空串上绿着）。
    assert!(
        (10..400).contains(&body.len()),
        "切出来的体只有 {} 字节 —— 配平切错了，本条会零命中地绿",
        body.len()
    );
    let stmts: Vec<&str> = body
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    assert_eq!(
        stmts,
        vec!["launch_local_posix_via(cmd, cwd, pick_terminal_exit())"],
        "\n★★ **这条链上最后那一行包装长胖了。**\n\
             实打过的第十一层就长在这里：在这一行**之前**把 `export ANTHROPIC_BASE_URL=…; ` 剥掉，\n\
             ⇒ `cargo test -p monitor --lib` **1232 全绿** —— 上面那两条判据都直调 `_via`，看不见它；\n\
             `history::LaunchSink` 那条缝量的是**交给送法之前**那一串，也看不见它。\n\
             **生产后果比刀 `L10` 还宽一格：两条支路上中转注入一起没了。**\n\
             ⇒ 这个函数体只许有那一行。要在这里加事情，先把「挑终端出口」也做成入参，\n\
             让判据驱动得到它（见本条头注「重新裁定的落点」）。\n\
             实得：{stmts:?}"
    );
}

#[test]
fn build_basic_agent_auth() {
    let c = cfg("pi.local", "pi", 22, None);
    let remote = "unset X; cd '/home/pi' && claude --resume s1";
    let got = build_remote_ssh_ps_command(&c, remote).unwrap();
    assert!(
        got.starts_with("& ssh -t -p 22 pi@pi.local -- "),
        "基本形态（agent 无 -i）: {got}"
    );
    // 解码 PS 单引号层（'' → '，全量双写故非重叠替换可逆）应还原传输包装形态。
    let payload = got.rsplit_once("-- ").unwrap().1;
    let inner = payload
        .strip_prefix('\'')
        .and_then(|s| s.strip_suffix('\''))
        .expect("ps quoted");
    assert_eq!(
        inner.replace("''", "'"),
        format!("bash -lic {}", posix_quote(remote))
    );
}

#[test]
fn build_key_and_port() {
    let c = cfg("10.0.0.2", "u", 2222, Some(r"C:\Users\z's\id_ed25519"));
    let got = build_remote_ssh_ps_command(&c, "claude --resume s1").unwrap();
    assert!(got.starts_with("& ssh -t -p 2222 -i 'C:\\Users\\z''s\\id_ed25519' u@10.0.0.2 -- "));
    // PS 单引号层把 ' 双写：bash -lic 'claude…' → ''claude…''
    assert!(got.contains("bash -lic ''claude --resume s1''"), "{got}");
}

#[test]
fn build_ipv6_host_ok() {
    let c = cfg("[::1]", "u", 22, None);
    assert!(build_remote_ssh_ps_command(&c, "claude --resume s1").is_ok());
}

#[test]
fn build_jump_arg_variants() {
    // F56：默认 port 省 :port,非默认带;非法 user/host 拒。
    assert_eq!(
        build_jump_arg("pi", "jump.local", 22).unwrap(),
        " -J pi@jump.local"
    );
    assert_eq!(
        build_jump_arg("u", "10.0.0.1", 2222).unwrap(),
        " -J u@10.0.0.1:2222"
    );
    assert!(build_jump_arg("bad user", "h", 22).is_err(), "非法 user 拒");
    assert!(build_jump_arg("u", "h;rm -rf", 22).is_err(), "非法 host 拒");
}

#[test]
fn reject_bad_inputs() {
    let c = cfg("h", "u", 22, None);
    assert!(build_remote_ssh_ps_command(&c, "").is_err(), "空命令拒");
    assert!(
        build_remote_ssh_ps_command(&c, "a\nb").is_err(),
        "控制字符拒"
    );
    assert!(
        build_remote_ssh_ps_command(&c, "cc --x \"y\"").is_err(),
        "双引号拒（PS native 畸变面）"
    );
    assert!(
        build_remote_ssh_ps_command(&c, &"a".repeat(5000)).is_err(),
        "超长拒"
    );
    let bad_user = cfg("h", "u ser", 22, None);
    assert!(
        build_remote_ssh_ps_command(&bad_user, "x").is_err(),
        "user 空格拒"
    );
    let empty_user = cfg("h", "", 22, None);
    assert!(
        build_remote_ssh_ps_command(&empty_user, "x").is_err(),
        "user 空拒"
    );
    let bad_host = cfg("h; rm", "u", 22, None);
    assert!(
        build_remote_ssh_ps_command(&bad_host, "x").is_err(),
        "host 注入拒"
    );
}

#[test]
fn remote_cmd_single_quotes_survive_both_layers() {
    // cwd 带单引号：前端 posixQuote 产出 '\'' 序列，PS 层再双写——验证嵌套后形态可逆。
    let c = cfg("h", "u", 22, None);
    let remote = r"cd '/a'\''b' && claude --resume s1";
    let got = build_remote_ssh_ps_command(&c, remote).unwrap();
    // PS 单引号字面量内：每个 ' 变 ''。解码（'' → '）应还原出 bash -lic 'POSIX(remote)'。
    let ps_payload = got.rsplit_once("-- ").map(|(_, p)| p).expect("has payload");
    let inner = ps_payload
        .strip_prefix('\'')
        .and_then(|s| s.strip_suffix('\''))
        .expect("ps quoted");
    let decoded = inner.replace("''", "'");
    assert_eq!(decoded, format!("bash -lic {}", posix_quote(remote)));
}
