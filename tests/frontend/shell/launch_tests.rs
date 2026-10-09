use super::*;
// 开窗的两个平台臂搬进 `platform/terminal.rs`：本文件那几条照旧判它们。
#[allow(unused_imports)]
use crate::platform::terminal::*;

/// 开终端这一族今天住两份：`launch.rs`（跑什么 · 两条 Tauri 命令）＋ `platform/terminal.rs`（开窗的两个平台臂）。
/// 按源码判的那几条读两份的合订本（射程与搬家前那一份 `launch.rs` 相同）。
const LAUNCH_SRC: &str = concat!(
    include_str!("../../../src/frontend/shell/src/launch.rs"),
    "\n",
    include_str!("../../../src/frontend/shell/src/platform/terminal.rs")
);

/// ★★ **F06b-1d（C9）：backend 开的每一个终端窗口都必须带上后端路径。**
///
/// 判据形态：**零命中守卫**（跑法：单测扫生产源码 · 钉的性质：**生产接线** ——
/// 两维分开写，登记的计量缺陷）。
///
/// # 它防的是什么
///
/// 「给窗口设 env」这件事**没有集中落点**：本文件有三个各自 spawn 的开窗点
/// （POSIX 一个、Windows 的 `wt.exe` 与 conhost 兜底各一个）。**新加第四个而忘了带 env**，
/// 后果是那条路上的 `ccm resume` **静默地**永远走本地 —— 与名字打错同一族的静默失败。
/// ⇒ 用「`Command::new(` 的总数」当触发器：多一个就红，逼人回来看这条。
///
/// ⚠ **别手搓剥测试的尺子**：用 `guard_core::production_code`（仓里已有那把）。
#[test]
fn every_terminal_window_backend_opens_carries_the_backend_path() {
    let src = LAUNCH_SRC;
    let prod = guard_core::production_code(src);
    let spawns = prod.matches("Command::new(").count();
    // ⚠ 钉**真正的动作** `.env(k, v)`，不是 helper 的调用次数：
    //    Windows 那个函数**只调一次 helper**，把结果给两个 spawn 点共用
    //    ⇒ 按 helper 数写地板会写成 3，实测 2（第一版就这么错的，被本条自己逮住）。
    let helper = prod.matches("backend_bin_env_for_window(").count();
    let carried = prod.matches(".env(k, v)").count();
    // 抽取器自检：剥完必须还看得见东西，且**确实剥掉了**测试里那两条字面量。
    assert!(
        prod.len() > 5_000,
        "剥完只剩 {} 字节 —— 剥过头了，本条会零命中地绿",
        prod.len()
    );
    // 生产里 3 个，全是开窗点（POSIX 一个 · Windows `wt.exe` 与 conhost 兜底各一个），每个都带 env。
    // 先前多一个不带 env 的 `where.exe` 探测；找 ssh 客户端挪进本机后端（只查文件）之后它没了 ⇒ 4 → 3，差额归零。
    assert_eq!(
        spawns, 3,
        "`launch.rs` 生产代码里的 `Command::new(` 从 3 变成了 {spawns}。\n\
             若新增的是**开终端窗口**，它必须也带上 `backend_bin_env_for_window(...)` 的 env，\n\
             否则那条路上的 `ccm resume` 会**静默地**永远走本地（与名字打错同一族的静默失败）。"
    );
    assert_eq!(
        carried, spawns,
        "真的把 env 交给窗口的 spawn 点 {carried} 个，开窗点 {spawns} 个 —— 有开窗点漏了 env"
    );
    assert_eq!(
        helper, 2,
        "解析本机后端路径的调用点从 2 变成了 {helper}（POSIX 一次 · Windows 一次给两个 spawn 共用）"
    );
}

/// ★ **那一句无条件断言不许出现在散文里**
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
/// F08 上半订正过**两条**同源假头注（`launch.rs:122` 与 `src/frontend/ui/fork-start.ts`），
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
    for f in [
        "README.md",
        "README.en.md",
        "src/frontend/shell/src/launch.rs",
        "src/frontend/shell/src/platform/terminal.rs", // 开窗的平台臂连同头注搬到这里
    ] {
        let body = std::fs::read_to_string(root.join(f))
            .unwrap_or_else(|e| panic!("{f} 读不到：{e} —— 文件搬了就把本条一起改"));
        files.push((f.to_string(), body));
    }
    // ★★ 〔P3b 08-12 第二次扩面〕**加 `tests/e2e/` 与 `src/`**。
    //
    // 08-06 那次（`c87d123`）的账是「扫描面本身是个洞」，扩到了 `doc/` + 两份 README。
    // 今天再量：**洞还在，只是挪了个位置** —— `tests/e2e/restart-cmd-driver.ts:6` 那句
    // 「GUI 全链在 Linux 结构性不可达（launch.rs 仅 Windows→回退剪贴板）」
    // 就躺在扫不到的地方，而 `launch_local_posix` 明明就在本文件扫的那几份里（今天住 `platform/terminal.rs`）。
    //
    // ⇒ **这不是巧合**：扫描面按「想到哪扫哪」长出来，而假话按「写在哪就在哪」分布。
    // 两者的形状不一样，所以「上次扩过了」不等于「这次够了」。
    // 〔搬树 2026-09-18〕**这一格从 `"tests/e2e"` 换成整棵 `"tests"`** —— 与
    // `structural_scan` 那边同一个判断：前端测试此前住在 `src/` 里、被下面那个 `"src"`
    // 顺带收着；搬去 `tests/` 之后**这个语料面悄悄缩了一大块**，而本条的反空真检查
    // 只管「某一族够不够」，管不了「少了一棵树」⇒ 会安静地少扫，不会红。
    for (dir, exts) in [("tests", &["ts", "sh", "md"][..]), ("src", &["ts"][..])] {
        for (q, body) in guard_core::scan_tree!(&root.join(dir), exts) {
            // 🔴 **`evidence/` 要排掉。** 它是量具与记录，
            // 里面的审计表会**逐字引用探针自己的那句话**（实发一例：一份 deathvalue
            // 记录里有一行在复述本条的探针串）⇒ 收进来等于**把探针的串喂给探针**，
            // 当场一条假阳。`structural_scan` 那边排掉它是同一条理由。
            //
            // ⚠ 本注释**刻意不复述那句探针串、也不写出那边那个函数名**：
            // 写全了会被本条与死名那条各命中一次（
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

/// 挑终端：设置里指定的 ⇒ 它（不在就照实说，不退成别的）；没设 ⇒ 两个系统出口 → 常见终端逐个探；KDE 上 konsole 先于别的具名终端；
/// 一个都没探到 ⇒ `None`（开窗那一问回「找不到终端」那个结局）。
#[cfg(not(windows))]
#[test]
fn the_terminal_is_picked_by_setting_then_system_exits_then_common_ones() {
    let only = |names: &'static [&'static str]| move |c: &str| names.contains(&c);
    let v = |xs: &[&str]| Ok(Some(xs.iter().map(|s| s.to_string()).collect::<Vec<_>>()));
    // 自动：系统出口在前，各家照自己的约定垫参数。
    assert_eq!(
        pick_terminal_from(None, "GNOME", &|_| true),
        v(&["xdg-terminal-exec", "--"])
    );
    assert_eq!(
        pick_terminal_from(
            None,
            "ubuntu:GNOME",
            &only(&["x-terminal-emulator", "xterm"])
        ),
        v(&["x-terminal-emulator", "-e"])
    );
    assert_eq!(
        pick_terminal_from(None, "GNOME", &only(&["wezterm", "xterm"])),
        v(&["wezterm", "start", "--"])
    );
    // KDE：konsole 提到具名终端最前；别的桌面照原序。
    assert_eq!(
        pick_terminal_from(Some(""), "KDE", &only(&["gnome-terminal", "konsole"])),
        v(&["konsole", "-e"])
    );
    assert_eq!(
        pick_terminal_from(Some("  "), "GNOME", &only(&["gnome-terminal", "konsole"])),
        v(&["gnome-terminal", "--"])
    );
    // 一个都没有 ⇒ `None`（不是一句报错：前端按结局说「未找到终端」并给设置入口）。
    assert_eq!(pick_terminal_from(None, "", &|_| false), Ok(None));
    // 设置里指定表里的名字 ⇒ 照它的约定；自定义前缀 ⇒ 按空白切开原样用。
    assert_eq!(
        pick_terminal_from(
            Some("alacritty"),
            "",
            &only(&["alacritty", "xdg-terminal-exec"])
        ),
        v(&["alacritty", "-e"])
    );
    assert_eq!(
        pick_terminal_from(Some(" tilix  -e "), "", &only(&["tilix"])),
        v(&["tilix", "-e"])
    );
    // 指定的那个不在 ⇒ 说是哪个（结局 `setMissing`，前端给［设置］），不退成自动挑到的别的终端。
    assert_eq!(
        pick_terminal_from(Some("kitty"), "", &only(&["xdg-terminal-exec"])),
        Err(SetMissing("kitty".to_string()))
    );
}

/// 「这台找不到终端」是一个**结局**（[`TerminalOpen::NoWindow`]，线上 `"noWindow"`），不是一句话里的标记：
/// 结局按码说、前端按码判 —— 改了那条文案，判断照样对；前端源码里不许按哪句话找字。
#[test]
fn the_no_window_outcome_is_a_code_not_a_phrase_the_frontend_greps_for() {
    assert_eq!(
        serde_json::to_value(TerminalOpen::NoWindow).unwrap(),
        serde_json::json!("noWindow")
    );
    assert_eq!(
        serde_json::to_value(TerminalOpen::Opened).unwrap(),
        serde_json::json!("opened")
    );
    assert_eq!(
        serde_json::to_value(TerminalOpen::SetMissing).unwrap(),
        serde_json::json!("setMissing")
    );
    const RUNNER: &str = include_str!("../../../src/frontend/ui/remote-launch-run.ts");
    const LOCAL: &str = include_str!("../../../src/frontend/ui/local-resume.ts");
    const LOGIN: &str = include_str!("../../../src/frontend/ui/settings/account-login.ts");
    const OPEN: &str = include_str!("../../../src/frontend/ui/terminal-open.ts");
    for (name, src) in [
        ("remote-launch-run.ts", RUNNER),
        ("local-resume.ts", LOCAL),
        ("account-login.ts", LOGIN),
    ] {
        assert!(
            src.contains("instanceof NoTerminalWindow") && !src.contains(".includes("),
            "{name} 没按结局判「找不到终端」，或在按那句话找字"
        );
    }
    assert!(
        OPEN.contains("\"noWindow\""),
        "terminal-open.ts 不认壳回的 noWindow 那个结局"
    );
}

/// 「终端」那一行只在 Linux 上出：Windows 上开终端恒是 PowerShell 窗口、没得选 ⇒ 壳答 `applies: false`，前端整行不画
/// （`terminal-row.vitest.ts` 那一格钉前端那一半）。Windows 那一臂在本机编不进来 ⇒ 钉它的正文；POSIX 那一臂行为地钉。
#[test]
fn the_terminal_row_only_applies_where_there_is_a_choice() {
    let prod = guard_core::production_code(LAUNCH_SRC);
    let at = guard_core::find_pinned(&prod, "pub fn terminal_choices() -> TerminalChoices {")
        .unwrap_or_else(|e| panic!("terminal_choices 的签名不是恰好一处：{e}"));
    let body = &prod[at..at
        + prod[at..]
            .find("\n}\n")
            .expect("切不出 terminal_choices 的体")];
    let win = guard_core::find_pinned(body, "#[cfg(windows)]")
        .unwrap_or_else(|e| panic!("terminal_choices 的 Windows 那一臂不是恰好一处：{e}"));
    assert!(
        guard_core::pin_line(&body[win..], "applies: false,").is_ok(),
        "Windows 那一臂不再答 applies: false：{}",
        &body[win..]
    );
    #[cfg(not(windows))]
    assert!(
        terminal_choices_from(None, "", &|_| false).applies,
        "Linux 上那一行该出（哪怕一个都没探到）"
    );
}

/// 设置页那一行的事实与开窗时挑的是同一份判定：自动那一项 ＝ 开窗时自动会挑的那个；探到的按同一顺序。
#[cfg(not(windows))]
#[test]
fn the_settings_row_shows_what_the_picker_would_pick() {
    let has = |c: &str| ["ptyxis", "xterm", "x-terminal-emulator"].contains(&c);
    let c = terminal_choices_from(Some(" ptyxis "), "GNOME", &has);
    assert!(c.applies);
    assert_eq!(c.found, vec!["x-terminal-emulator", "ptyxis", "xterm"]);
    assert_eq!(c.setting, "ptyxis");
    let auto = pick_terminal_from(None, "GNOME", &has).unwrap().unwrap();
    assert_eq!(c.auto.as_deref(), Some(auto[0].as_str()));
    let none = terminal_choices_from(None, "", &|_| false);
    assert_eq!(
        (none.auto, none.found.len(), none.setting.as_str()),
        (None, 0, "")
    );
}

/// 每个终端「开一个窗口跑这条命令」的写法各一格（照各家用法串核过）：前缀之后接 `bash -lic <留窗脚本> bash <命令>`。
/// 改了哪一家的写法 ⇒ 这一格红（它在别人机器上就会开出一个空窗口、命令没跑）。
#[cfg(not(windows))]
#[test]
fn each_terminal_gets_its_own_way_of_running_a_command() {
    let expect: &[(&str, &[&str])] = &[
        ("xdg-terminal-exec", &["--"]),
        ("x-terminal-emulator", &["-e"]),
        ("ptyxis", &["--"]),
        ("kgx", &["--"]),
        ("gnome-terminal", &["--"]),
        ("konsole", &["-e"]),
        ("xfce4-terminal", &["-x"]),
        ("mate-terminal", &["-x"]),
        ("qterminal", &["-e"]),
        ("tilix", &["-e"]),
        ("ghostty", &["-e"]),
        ("kitty", &[]),
        ("alacritty", &["-e"]),
        ("wezterm", &["start", "--"]),
        ("foot", &[]),
        ("xterm", &["-e"]),
    ];
    assert_eq!(
        EMULATORS.iter().map(|(n, _)| *n).collect::<Vec<_>>(),
        expect.iter().map(|(n, _)| *n).collect::<Vec<_>>(),
        "探测清单变了 —— 这张表跟着改，并给新来的那家写清它的用法"
    );
    let argv: Vec<String> = ["bash", "-lic", "claude --resume s1"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    for (name, before) in expect {
        let term = pick_terminal_from(Some(name), "", &|c| c == *name)
            .unwrap()
            .unwrap();
        let (program, args) = build_local_posix_spawn(&term, &argv);
        assert_eq!(program, *name);
        assert_eq!(&args[..before.len()], *before, "{name} 的前缀不对");
        let rest = &args[before.len()..];
        if ONE_STRING.contains(name) {
            assert_eq!(rest.len(), 1, "{name} 只收一整串");
        } else {
            assert_eq!(
                rest,
                ["bash", "-lic", KEEP_OPEN, "bash", "claude --resume s1"],
                "{name} 命令那一截不对"
            );
        }
    }
}

/// 自动挑的顺序：两个系统出口 → 这个桌面的默认 → 其余；桌面名按 `XDG_CURRENT_DESKTOP` 冒号分段、大小写不论。
#[cfg(not(windows))]
#[test]
fn auto_order_puts_the_desktops_own_terminal_first_after_the_system_exits() {
    let head = |d: &str| auto_order(d).into_iter().take(5).collect::<Vec<_>>();
    assert_eq!(
        head("ubuntu:GNOME"),
        [
            "xdg-terminal-exec",
            "x-terminal-emulator",
            "ptyxis",
            "kgx",
            "gnome-terminal"
        ]
    );
    assert_eq!(head("KDE")[2], "konsole");
    assert_eq!(head("XFCE")[2], "xfce4-terminal");
    assert_eq!(head("xfce")[2], "xfce4-terminal");
    assert_eq!(head("MATE")[2], "mate-terminal");
    assert_eq!(head("LXQt")[2], "qterminal");
    // 认不出的桌面：照清单原序。
    assert_eq!(
        auto_order("sway"),
        EMULATORS.iter().map(|(n, _)| *n).collect::<Vec<_>>()
    );
    // 每个名字恰好一次。
    let all = auto_order("GNOME:KDE");
    let mut uniq = all.clone();
    uniq.sort();
    uniq.dedup();
    assert_eq!(all.len(), uniq.len());
    assert_eq!(all.len(), EMULATORS.len());
    // GNOME 上只装了 kgx 与 xterm ⇒ 挑 kgx。
    assert_eq!(
        pick_terminal_from(None, "GNOME", &|c| c == "kgx" || c == "xterm"),
        Ok(Some(vec!["kgx".to_string(), "--".to_string()]))
    );
}

/// 收一整串的那几家（Tilix）：拼成的那一串交 shell 词法切回去，逐个参数与原来相等 —— 带单引号 · `$` · 反斜杠 · 双引号的载荷都不走样。
#[cfg(not(windows))]
#[test]
fn the_one_string_form_splits_back_into_the_same_arguments() {
    let payload = r#"printf '%s' "it's" '$HOME' \ "q"x""#;
    let argv: Vec<String> = ["bash", "-lic", payload]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let term = vec!["/usr/bin/tilix".to_string(), "-e".to_string()];
    let (_, args) = build_local_posix_spawn(&term, &argv);
    assert_eq!(args.len(), 2);
    let out = std::process::Command::new("sh")
        .arg("-c")
        .arg(format!(
            "eval \"set -- $1\"; for a in \"$@\"; do printf '%s\\0' \"$a\"; done"
        ))
        .arg("sh")
        .arg(&args[1])
        .output()
        .unwrap();
    let got: Vec<String> = String::from_utf8(out.stdout)
        .unwrap()
        .split('\0')
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect();
    assert_eq!(got, ["bash", "-lic", KEEP_OPEN, "bash", payload]);
}

/// 开窗那一跳只加「怎么开窗」：终端前缀 ＋ `bash -lic <留窗脚本> bash <命令>`，命令逐字节在最后一格。
#[cfg(not(windows))]
#[test]
fn opening_a_window_does_not_touch_the_payload() {
    let argv: Vec<String> = ["bash", "-lic", "unset X; claude --resume s1"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let term = vec!["wezterm".to_string(), "start".to_string(), "--".to_string()];
    let (program, args) = build_local_posix_spawn(&term, &argv);
    assert_eq!(program, "wezterm");
    assert_eq!(
        args,
        vec![
            "start",
            "--",
            "bash",
            "-lic",
            KEEP_OPEN,
            "bash",
            "unset X; claude --resume s1"
        ],
        "开窗那条路没有把命令原样交出去"
    );
}

/// 留窗脚本真的先跑命令（`$1` 原样、不经二次拼接），跑完换成交互 shell（同 Windows 那一形的 `-NoExit`）。
#[cfg(not(windows))]
#[test]
fn the_keep_open_script_runs_the_command_verbatim_then_hands_over_a_shell() {
    let dir = scratch_dir("keepopen");
    let out = dir.join("seen");
    let cmd = format!("printf '%s|' \"it's\" '$HOME' > '{}'", out.display());
    // 假 `$SHELL`：被换上来时记下自己收到的参数。
    let handed = dir.join("handed");
    let shell = dir.join("fake-shell");
    install_fake_terminal(
        &shell,
        &format!("#!/bin/sh\nprintf '%s' \"$*\" > '{}'\n", handed.display()),
    );
    let st = std::process::Command::new("bash")
        .args(["-c", KEEP_OPEN, "bash", &cmd])
        .env("SHELL", &shell)
        .stdin(std::process::Stdio::null())
        .status()
        .expect("跑 bash");
    let got = std::fs::read_to_string(&out).unwrap_or_default();
    let then = std::fs::read_to_string(&handed).ok();
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(got, "it's|$HOME|");
    assert!(st.success());
    assert_eq!(
        then.as_deref(),
        Some("-l"),
        "命令跑完没有换成 $SHELL -l（窗口会闪退）"
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
/// `local_posix_sends_the_payload_itself_without_an_ssh_wrap` 断的正是「逐字节透传」，
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
/// 那 3 行由 `the_spawned_process_really_gets_the_relay_prefix_through_the_terminal` 与
/// `the_terminal_we_hand_the_command_to_really_gets_the_relay_prefix` 看着，两条都不开窗。
///
/// ⚠⚠ **补门的代价 —— 这一条最贵，逐字写清**。
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
        "export ANTHROPIC_BASE_URL='http://127.0.0.1:8788/s/claude-code/acct-a'; \
             export CLAUDE_CONFIG_DIR='/h/.claude-alt/acct-a'; claude --resume s1",
        // ③ **另一个号** —— 「哪个号」这一维也不许在这一跳上被抹平。
        "export ANTHROPIC_BASE_URL='http://127.0.0.1:8788/s/claude-code/acct-b'; \
             export CLAUDE_CONFIG_DIR='/h/.claude-alt/acct-b'; claude --resume s2",
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
        let term = vec!["xdg-terminal-exec".to_string(), "--".to_string()];
        let (program, args) = local_posix_spawn_plan(payload, &term).expect("该通过校验");
        assert_eq!(program, "xdg-terminal-exec");
        assert_eq!(
            args,
            vec!["--", "bash", "-lic", KEEP_OPEN, "bash", payload],
            "开窗那条路上载荷被改了 —— 命令必须逐字节原样在最后一格"
        );
    }
}

/// ★ L1 的验收判据（「关键判断」第 1 条逐字）：
/// **给同一个 plan 换 transport，除 ssh 包装外输出逐字节相同。**
///
/// 远端那一半（ssh 外壳 ＋ PS 单引号层剥净之后就是同一串）随渲染搬进本机后端：
/// `tests/backend/dial_terminal_tests.rs::the_basic_shape_goes_through_the_agent_and_wraps_the_payload_only_twice`。
/// 这里留本地那一半：直接 exec `bash -lic <载荷>`，载荷逐字节不动、没有 ssh 包。
#[test]
fn local_posix_sends_the_payload_itself_without_an_ssh_wrap() {
    let payload = "unset CLAUDE_CONFIG_DIR; cd '/home/z/p' && ccm --tmux claude --resume s1";
    let local = build_local_posix_argv(payload).unwrap();
    assert_eq!(
        local,
        vec!["bash", "-lic", payload],
        "本地：直接 exec，无 ssh 包"
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
/// 远端（走 PowerShell）那一半的「应拒」随渲染搬进本机后端（`dial_terminal_tests.rs::bad_inputs_are_refused_and_say_which_cell`）；
/// 这里留本地那一半。
#[test]
fn double_quote_rejection_is_powershell_only() {
    let with_dq = r#"claude --append-system-prompt "be brief""#;
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
    // 喂一个只 `exec "$@"` 的假终端（不弹真窗口），命令照生产那一形在它里面跑。
    let term = exec_terminal(&dir);
    launch_local_posix_via(&cmd, dir.to_str(), &term).expect("spawn 应成功");
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
// 开窗那一跳之后的几行（`Command::new` · `.args` · cwd / env / stdio）：两条判据都在真被 spawn 出去的进程上观测，
// 终端是入参 ⇒ 喂自己造的 `#!/bin/sh` 假终端，不弹任何窗口；载荷只有一条 `printf`，不起任何 agent。
// ═════════════════════════════════════════════════════════════════════════

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

/// 一个只 `exec "$@"` 的假终端（不开窗，命令照生产那一形跑）⇒ 它的 argv 前缀。
#[cfg(not(windows))]
fn exec_terminal(dir: &std::path::Path) -> Vec<String> {
    let p = dir.join("exec-terminal");
    install_fake_terminal(&p, "#!/bin/sh\nexec \"$@\"\n");
    vec![p.to_string_lossy().into_owned()]
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

/// 本件的中转前缀 —— 照后端生产那一形手写（monitor 这棵树够不着后端渲染器，见下面那条注释）。
#[cfg(not(windows))]
fn relay_probe_prefix(url: &str) -> String {
    // 照那一形手写一份（钥匙段读 `$HOME` 下那个文件的命令替换，与 `ccm` 非得经 shell 那一趟同形，
    //   `control/ccm/plan.rs::relay_export`）—— 本条验的是开窗那一跳把整串原样交给了进程，不是前缀怎么渲。
    let (origin, path) = url.split_at(url.find("/s/").expect("夹具 URL 带 /s/"));
    format!(
        "export ANTHROPIC_BASE_URL={}$(cat ~/{}){}; ",
        shell_quote_core::posix_quote(&format!("{origin}/")),
        relay_route_core::KEY_FILE_REL,
        shell_quote_core::posix_quote(path)
    )
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
    let term: Vec<String> = term.into_iter().map(String::from).collect();
    let mut last = String::new();
    for i in 0..tries.max(1) {
        match launch_local_posix_via(payload, cwd, &term) {
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

/// 起出去的那个进程（经一个只 `exec "$@"` 的假终端）真的拿到了中转注入：观测点是进程自己写下的 `ANTHROPIC_BASE_URL`。
/// 量的是 `local_posix_spawn_plan` 返回之后那几行（`Command::new` · `.args` · cwd / env / stdio）没把载荷改掉。
/// 买不到：claude 拿到这个变量之后的真实行为（不起真 claude）。
#[cfg(not(windows))]
#[test]
fn the_spawned_process_really_gets_the_relay_prefix_through_the_terminal() {
    let dir = scratch_dir("nowin");
    let seen = dir.join("what-the-process-saw");
    let url = "http://127.0.0.1:8788/s/claude-code/acct-a";
    let prefix = relay_probe_prefix(url);
    // 反空真①：前缀本来就该非空且真的是一段 env 注入，否则下面整条是「空 == 空」。
    assert!(
        prefix.contains("ANTHROPIC_BASE_URL") && !prefix.is_empty(),
        "生产那一处渲染器没渲出中转注入 —— 本条按红处理：{prefix:?}"
    );
    // 注入的 URL 里钥匙那一段是「读 `$HOME/.cc-monitor/relay-key`」的命令替换 ⇒ 给这一趟一个夹具家目录、
    //   放一把夹具钥匙，期望值是**展开之后**那条带钥匙的 URL（不碰用户真实的家目录）。
    let fixture_key = "5eed".repeat(16);
    std::fs::create_dir_all(dir.join(".cc-monitor")).expect("夹具 .cc-monitor");
    std::fs::write(dir.join(relay_route_core::KEY_FILE_REL), &fixture_key).expect("夹具钥匙");
    let expanded = url.replacen("8788/", &format!("8788/{fixture_key}/"), 1);
    let payload = format!(
        "HOME='{}'; {prefix}printf '%s' \"$ANTHROPIC_BASE_URL\" > {}",
        dir.display(),
        seen.display()
    );
    // 反空真②：观测点在**进程那一侧**，起手它必须不存在。
    assert!(!seen.exists(), "起手观测文件就在了 —— 本条会读到上一趟的痕");

    let term = exec_terminal(&dir);
    launch_local_posix_via(&payload, dir.to_str(), &term).expect("spawn 应成功");

    let landed = wait_for(&seen);
    let got = if landed {
        std::fs::read_to_string(&seen).unwrap_or_default()
    } else {
        String::new()
    };
    let _ = std::fs::remove_dir_all(&dir);
    assert!(landed, "5s 内那个进程什么都没写下来 —— spawn 那半没真跑");
    assert_eq!(
        got, expanded,
        "\n★★ **起出去的那个进程没拿到中转注入** —— 这是第九层（刀 `Z1c`）的形状：\n\
             `local_posix_spawn_plan` 返回之后那 3 行里把 `export ANTHROPIC_BASE_URL=…; `\n\
             从 argv 里剥掉，纯函数一字不动、全仓锚点一处不少，而上一版全绿。\n\
             生产后果：claude 直连官方端点，第三方 key 用不上，而门禁四个数一格不动。\n\
             实得 = {got:?} · 期望 = {expanded:?}"
    );
}

/// 交给终端的那份 argv 逐格等于 `bash -lic <留窗脚本> bash <带前缀的命令>`：喂一个只记录、不执行的假终端（不开窗）。
/// 买不到：真终端拿到 argv 之后会不会照着跑 —— 那一格由私有显示上的真终端读数承重。
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

    let url = "http://127.0.0.1:8788/s/claude-code/acct-b";
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
            "bash".to_string(),
            "-lic".to_string(),
            KEEP_OPEN.to_string(),
            "bash".to_string(),
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

/// 生产入口 [`launch_local_posix`] 只有一句：挑终端 ＋ 交给 [`open_window_via`]（命令原样；挑到了它再交 [`launch_local_posix_via`]）。
/// 它里面挑终端会在有桌面的机器上真开窗 ⇒ 不能行为地驱动，钉形状：在这里剥掉中转前缀，两条走 `_via` 的判据都看不见。
#[cfg(not(windows))]
#[test]
fn the_thin_wrapper_hands_the_command_straight_through_to_the_via_form() {
    let prod = guard_core::production_code(LAUNCH_SRC);
    // 锚点必须**唯一**：Windows 那一份的形参带下划线前缀（`_cmd` / `_cwd`）⇒ 签名不同。
    let head =
        "pub fn launch_local_posix(cmd: &str, cwd: Option<&str>) -> Result<TerminalOpen, String> {";
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
        vec![
            "match pick_terminal() {",
            "Ok(term) => open_window_via(cmd, cwd, term.as_deref()),",
            "Err(SetMissing(name)) => {",
            "tracing::info!(\"launch: 设置里指定的终端 {name} 不在\");",
            "Ok(TerminalOpen::SetMissing)",
            "}",
            "}",
        ],
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

// 这里原来六条钉 `build_remote_ssh_ps_command`〔散文墓碑〕与 `build_jump_arg`〔散文墓碑〕（基本形态 · 钥匙与口 ·
// IPv6 · 跳板参数 · 坏输入 · 单引号过两层）：ssh 外壳搬进本机后端（帧命令 `terminal-ssh`），期望原样搬进
// `tests/backend/dial_terminal_tests.rs`（被测对象搬了家，一个期望没改；跳板那一格改经 `machine::resolve`）。

/// 开终端窗口那一问只答一处（`open_local`）：POSIX 一个终端都没探到 ⇒ 回「找不到终端」那个结局（前端据此说「未找到终端」并给设置入口，
/// 账号登录另给「在 tmux 里登录」），**不**回落到无窗口直起（要人交互的那一行跑在看不见的地方 = 没跑）。
/// 有出口那一支交 `launch_local_posix_via`（它的 spawn 由假终端那条判据买到）。
#[cfg(not(windows))]
#[test]
fn without_a_terminal_exit_the_window_open_says_so_instead_of_running_headless() {
    let dir = std::env::temp_dir().join(format!("ccm-open-window-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let ran = dir.join("ran");
    let cmd = format!("touch '{}'", ran.display());
    let got = open_window_via(&cmd, None, None);
    std::thread::sleep(std::time::Duration::from_millis(500));
    let ran_headless = ran.exists();
    std::fs::remove_dir_all(&dir).ok();
    assert_eq!(
        got,
        Ok(TerminalOpen::NoWindow),
        "找不到终端时该回「找不到终端」那个结局"
    );
    assert!(!ran_headless, "找不到终端时那一行被无窗口直起了");
}

/// Linux 上的系统通知：全进程一条会话总线长连接（`platform/notify.rs::linux::POOL`），不按条新开 ——
/// GNOME 见发信人的总线名没了、又认得出是哪个有窗口的程序，就当场把通知收掉（L2 · 10-08 真窗口现打）；
/// 「发 N 条只连一次」的行为由 `platform/notify_tests.rs` 拿假总线钉。发通知只有一个家，壳里别处不许再经插件发。
#[test]
fn linux_notifications_share_one_long_lived_bus_connection() {
    const NOTIFY: &str = include_str!("../../../src/frontend/shell/src/platform/notify.rs");
    let prod = guard_core::production_code(NOTIFY);
    assert!(
        guard_core::find_pinned(&prod, "linux::POOL.send(app, title, body)").is_ok(),
        "Linux 那一臂不再经全进程那一条连接发"
    );
    const LIB: &str = include_str!("../../../src/frontend/shell/src/lib.rs");
    assert!(
        !guard_core::contains_word(&guard_core::production_code(LIB), "NotificationExt"),
        "lib.rs 还在直接经插件发通知"
    );
}
