/// **刻意不进 CI 门禁的 e2e 套**（`test:` 后缀, 为什么）。
///
/// 默认拒绝：不在这里、又没有地板行、又没被直接跑的套，正题判据会点名。
const EXEMPT: &[(&str, &str)] = &[
    (
        "graylight",
        "全链级：断言源是 Xvfb 上**正在跑的 dev app**（`npx tauri dev`）写的 monitor 日志，\
             无头 CI 里没有那个 app；论证写在 `tests/e2e/README.md`「`graylight-suite`（全链级）不在上表那些套件里」",
    ),
    (
        "f40",
        "渲染/滚动管线级：同 `graylight-suite` 的规格（要跑着的 dev app + Xvfb），\
             且它**不打印「合计 PASS=」**、断言数随环境分支变 ⇒ 连 `assert-pass-floor` 的度量口径都不成立；\
             论证写在 `tests/e2e/README.md`（U0 2026-08-01 补写那两段）",
    ),
];

fn package_json() -> String {
    let p = crate::guard_support::repo_root().join("package.json");
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("读不到 {p:?}: {e}"))
}

/// 人群：`package.json` 里**真的在跑 `tests/e2e/` 的** `test:*` 脚本 → `(套名, 脚本路径)`。
///
/// ⚠ 按「这条 npm 脚本跑的是不是 e2e 套」取，不按名字前缀取：
/// `test:` 下面也有纯 node/vitest 的套，而 `gen:payload-golden` 这种**生成器**
/// 名字里没有 `test`、也确实不该有断言地板。
fn e2e_suites() -> Vec<(String, String)> {
    let mut out = Vec::new();
    for line in package_json().lines() {
        let t = line.trim();
        let Some(rest) = t.strip_prefix("\"test:") else {
            continue;
        };
        let Some((name, cmd)) = rest.split_once("\":") else {
            continue;
        };
        let cmd = cmd
            .trim()
            .trim_start_matches('"')
            .trim_end_matches(',')
            .trim_end_matches('"');
        if !cmd.contains("tests/e2e/") {
            continue;
        }
        let Some(path) = cmd.split_whitespace().find(|w| w.starts_with("tests/e2e/")) else {
            continue;
        };
        out.push((name.to_string(), path.to_string()));
    }
    out.sort();
    out
}

/// `ci.yml` 里带断言数地板的套名（从 `assert-pass-floor.sh <名> <数>` 的**调用行**取）。
///
/// ⚠ 只认 `run:` 那种真调用行 —— `ci.yml` 自己那段反向自检里也写着这个脚本名
/// （`grep -q "assert-pass-floor.sh $pair\b"`），把它算进来会数出一个假的更大的数。
/// 这一点 `ci.yml` 的注释里逐字警告过（「用宽模式会把它们算进来」），照它办。
fn floored() -> Vec<String> {
    // ⚠ 剔注释走 `ci_yaml::live_lines`，**不自己再写一遍**：
    // 第一版在这里内联了一个 `starts_with('#')` 过滤，被 `structural_scan` 的
    // 「剥注释转换器必须登记」当场逮住（默认拒绝该有的样子）。按它问的那句
    // 「共享原语为什么不够」查下去 —— `guard_core::strip_comment_lines` 认的是
    // `//` / `/*`，接不住 YAML 的 `#` ⇒ 正确答案不是登记第二份，是把
    // 原本住在 `shared_crate_registry::mod tests` 里的那份**搬出来共用**。
    let live = crate::shared_crate_registry::ci_yaml::live_lines();
    let mut out = Vec::new();
    for line in live.lines() {
        let t = line.trim();
        let Some(rest) = t.split_once("run: bash tests/e2e/assert-pass-floor.sh ") else {
            continue;
        };
        let mut it = rest.1.split_whitespace();
        let (Some(name), Some(floor)) = (it.next(), it.next()) else {
            continue;
        };
        if floor.chars().all(|c| c.is_ascii_digit()) {
            out.push(name.to_string());
        }
    }
    out.sort();
    out
}

/// ★ 正题：**每一套 e2e 要么进门禁，要么登记为什么不进**。
#[test]
fn every_e2e_suite_is_either_gated_or_registered_as_exempt() {
    let suites = e2e_suites();
    let floored = floored();
    let yml = crate::shared_crate_registry::ci_yaml::yml();
    // 抽取器自检：哪一头空了，下面那条对拍都会零命中地绿。
    assert!(
        suites.len() >= 15,
        "从 `package.json` 只抠到 {} 套跑 e2e 的 `test:*`（08-08 实测 22）—— \
             写法变了，本条会零命中地绿",
        suites.len()
    );
    assert!(
        floored.len() >= 15,
        "从 `ci.yml` 只抠到 {} 条 `assert-pass-floor` 调用行（08-08 实测 19）—— \
             抽取器坏了，本条会把**所有**套判成没进门禁",
        floored.len()
    );

    let ungated: Vec<String> = suites
        .iter()
        .filter(|(n, _)| !floored.contains(n))
        // 第二条出路：`ci.yml` 里有步骤直接跑那个脚本路径。
        .filter(|(_, path)| !yml.contains(&format!("run: bash {path}")))
        .filter(|(n, _)| !EXEMPT.iter().any(|(e, _)| e == n))
        .map(|(n, p)| format!("  test:{n}  ({p})"))
        .collect();
    assert!(
        ungated.is_empty(),
        "这些 e2e 套**没进任何门禁**，也没登记为什么不进：\n{}\n\n\
             ★ 这正是 `ci.yml` 自己记下的那个形状：「只有 npm 脚本、没进任何门禁 …… \n\
             只是当时忘了接线 …… 那正是『新东西入网漏一拍』的形状」。上一次是**人**发现的。\n\
             ⚠ 那条 `[ \"$N\" -ge 19 ]` 计数地板挡不住这件事：第 20 套没登记时 `N` 还是 19。\n\
             三条出路：① 在 `ci.yml` 加一行 `assert-pass-floor.sh <套名> <地板>`（记得同步那个数）；\n\
             ② 让某个步骤直接跑它（它若不打印「合计 PASS=」就走这条）；\n\
             ③ 登记进本文件的 `EXEMPT` **并写清结构性理由** —— \n\
             「暂时没接」不是理由，`graylight`/`f40` 那两条写的是「无头 CI 里没有那个 dev app」。",
        ungated.join("\n")
    );
}

/// ★ **那个「19」的四份副本都要等于派生出来的真值**。
///
/// 副本消不掉（一份是给人读的散文、一份是步骤名、一份是真判定、一份是收尾回显），
/// 那就让它们**对拍**。这四份漂过 —— 那段注释自己记着「原写『8』…漏跟的两处」。
#[test]
fn the_suite_count_is_the_same_number_in_all_four_places() {
    let n = floored().len();
    let yml = crate::shared_crate_registry::ci_yaml::yml();
    // `(定位这一行的措辞, 这份副本是什么, 这行里必须出现的东西)`。
    //
    // ⚠ **两步走**：先按**不含数字**的措辞找到那一行（找不到 = 措辞变了，该修锚点），
    // 再看那行里有没有带着真值的那段字面。08-08 第一版图省事，直接找「含 `-ge` 的行」
    // —— 当场抓到的是**上一轮刚加的 shellcheck 覆盖面地板**（同一份文件里的另一条 `-ge`），
    // 报「写着 45」。**匹配单位比事实大**，F24 那一族，这次犯在四份副本的对拍上。
    type Expect = fn(usize) -> Vec<String>;
    let places: &[(&str, &str, Expect)] = &[
        (
            "套真机套件必须",
            "注释里的散文（给人读的那份）",
            |n| vec![format!("{n} 套真机套件必须")],
        ),
        (
            "覆盖面地板（",
            "步骤名（CI 日志里显示的那份）",
            |n| vec![format!("（{n} 套")],
        ),
        (
            "套带地板",
            "真正做判定的那一行（连同它自己的报错文案）",
            |n| vec![format!("-ge {n} "), format!(">= {n}）")],
        ),
        (
            "套逐个对上",
            "收尾回显（跑绿时打印的那份）",
            |n| vec![format!("\"{n} 套逐个对上")],
        ),
    ];
    let mut seen = 0usize;
    for (anchor, what, expect) in places {
        let line = yml
            .lines()
            .map(str::trim)
            .find(|l| l.contains(anchor))
            .unwrap_or_else(|| {
                panic!("`ci.yml` 里找不到「{what}」那一行（措辞锚点 `{anchor}`）—— 措辞变了，本条会零命中地绿")
            });
        for want in expect(n) {
            assert!(
                line.contains(&want),
                "「{what}」里找不到 `{want}`，而 `ci.yml` 真实带地板的套是 {n} 个。\n\
                     那一行现在是：{line}\n\
                     ⇒ 同一个数在 `ci.yml` 里存了四份，加一套就得**四处一起改**。\n\
                     ⚠ 别只改报错的那一处：这四份漂过一次，那段注释自己记着\n\
                     「本行原写『8』，是订正下方散文时漏跟的两处」。"
            );
        }
        seen += 1;
    }
    assert_eq!(
        seen, 4,
        "只核到 {seen} 处副本（应 4 处）—— 抽取器漏了，本条在空转"
    );
}

/// **豁免行不许变成死行**（反向锚点）。
#[test]
fn every_exemption_still_points_at_a_real_ungated_suite() {
    let suites = e2e_suites();
    let floored = floored();
    for (name, why) in EXEMPT {
        assert!(
            suites.iter().any(|(n, _)| n == name),
            "`EXEMPT` 里登记着 `test:{name}`，而 `package.json` 里已经没有这套了 ⇒ 删掉这一行"
        );
        assert!(
            !floored.contains(&name.to_string()),
            "`test:{name}` 登记着豁免，可 `ci.yml` 里**已经有它的地板行了** ⇒ 删掉这条豁免。\n\
                 留着的害处很具体：下一个人会以为这套没人跑，从而不敢依赖它的结论。"
        );
        assert!(
            why.contains("README") || why.len() >= 60,
            "`test:{name}` 的豁免理由太薄 —— 要么指向写着论证的那份文档，要么把结构性理由说清楚。\
                 「暂时没接」这种可克服的话不算理由。"
        );
    }
}

/// **自称「进的是本机门禁」的豁免，本机门禁里真得跑它** —— 两向集合相等。
///
/// 左 = `EXEMPT` 里理由写着 `本机门禁 \`tests/scripts/gate.sh\`` 的那几套；
/// 右 = `package.json` 里跑 e2e 的套中，`ci.yml` 没有地板行、而 `tests/scripts/gate.sh` 里
/// `run_e2e <套名> ` **恰好一处**的那几套。
/// 两侧**异源**：左边是本文件的登记，右边是门禁脚本的现物。
///
/// 它逮两形：① 有人把 `gate.sh` 那一行删了/改了名，豁免还挂着「进了本机门禁」⇒ 左多右少；
/// ② 有人往 `gate.sh` 里挂了一套 `ci.yml` 没地板的新 e2e，本文件没登记 ⇒ 右多左少
/// （那一形正题判据 `every_e2e_suite_is_either_gated_or_registered_as_exempt` 也会红，
/// 但它说的是「没进任何门禁」，而事实是「进了本机门禁、没说」）。
/// ⚠ 反空真：右边读不到 `gate.sh` ⇒ 右空、左 2 ⇒ 当场分叉；左边的两条是手写的，不从右边派生。
#[test]
fn an_exemption_that_claims_the_local_gate_is_really_run_there() {
    let gate =
        std::fs::read_to_string(crate::guard_support::repo_root().join("tests/scripts/gate.sh"))
            .expect("读不到 tests/scripts/gate.sh");
    let floored = floored();
    // 人群 = `package.json` 里跑 e2e 的套（`assert-pass-floor.sh` 只认 npm 脚本，
    // `gate.sh` 跑得动的一定在这里面）；判据 = `gate.sh` 里 `run_e2e <套名> ` 那一句**恰好一处**
    // （`find_pinned`：零处 / 两处都不算「在跑」）。
    let mut right: Vec<String> = e2e_suites()
        .into_iter()
        .map(|(n, _)| n)
        .filter(|n| !floored.contains(n))
        .filter(|n| guard_core::find_pinned(&gate, &format!("run_e2e {n} ")).is_ok())
        .collect();
    right.sort();
    let mut left: Vec<String> = EXEMPT
        .iter()
        .filter(|(_, why)| why.contains("本机门禁 `tests/scripts/gate.sh`"))
        .map(|(n, _)| n.to_string())
        .collect();
    left.sort();
    assert_eq!(
        left, right,
        "「自称进了本机门禁的豁免」与「`gate.sh` 真在跑、`ci.yml` 没地板的套」对不上。\n\
             左（本文件登记）：{left:?}\n右（`gate.sh` 现物）：{right:?}"
    );
}

/// `P0b`：全链台架的「没有孤儿后端」那一格，**人群要覆盖两族**。
///
/// # 它防的是一个实测出来的洞
///
/// 那一格原来只数 `.build/backend/...`，而 08-12 实测：盘上活着的 backend
/// 走的是**部署落点** `~/.cc-monitor/bin/cc-monitor-backend`（`sftp::ensure_backend_deployed`
/// 的落点）—— **那一族当时根本不在人群里**，计数器却会安心地报 0。
/// 这正是 `needle_anchor_registry` 那条：**匹配单位不许比事实小**。
///
/// ★★ 本条钉的是**那两条 `pgrep` 表达式本身**（`find_pinned`：恰好一处 + 两侧有边界），
/// 不是「字面量在文件里出现过」—— 首跑变异当场证明后者不成立：把 `pgrep` 那行删掉，
/// 同一个字面量在**我自己写的说明注释**里还在，判据照样绿。
///
/// ⚠ 本条钉「两族都数」，**不保证**盘上没有第三族。哪天后端又多一个落点，
/// 这条会因为「新落点不在表里」而**沉默**，不会报。如实登记。
#[test]
fn the_orphan_backend_gate_counts_both_families() {
    let raw = read_e2e("graylight-suite.sh");
    let src = strip_comments(&raw);
    for expr in [
        "pgrep -fc '.build/backend/[^ ]*/cc-monitor-backend'",
        "pgrep -fc '\\.cc-monitor/bin/cc-monitor-backend'",
    ] {
        assert!(
            guard_core::find_pinned(&src, expr).is_ok(),
            "孤儿后端那一格的**可执行行**里少了这条计数：`{expr}`。\
                 少数一族，计数器就会在真有残留时报 0（08-12 实测：部署落点那一族当时不在人群里）。\
                 ⚠ 写进注释不算数 —— 本条剥注释后才钉。"
        );
    }
}

/// `P0b`：台架**不许再教人裸 `pkill -f`**。
///
/// # 为什么这条值得单独钉
///
/// 模式杀**没有「只杀我起的那些」这个概念**。本仓吃过一次：`P5L` 那拍跑
/// `pkill -f xdg-terminal-exec`，**把我自己的 shell 打死了**（模式命中了自己的命令行）。
/// 而这条建议住在一个**出错时才会被读到**的地方（ABORT 文案），
/// 读它的人正处在「台架坏了、想赶紧清干净」的状态 —— 最容易照着敲。
#[test]
fn the_harness_no_longer_teaches_a_bare_pattern_kill() {
    let src = strip_comments(&read_e2e("graylight-suite.sh"));
    assert!(
        !guard_core::contains_word(&src, "pkill"),
        "台架的可执行段又出现 `pkill` —— 那是模式杀，打到什么由命令行长相决定"
    );
    assert!(
        guard_core::find_pinned(&src, "reap-orphan-backends.sh").is_ok(),
        "得给出替代品，否则被 ABORT 拦住的人只会自己去敲 pkill"
    );
    // 那把刀本身必须在，且**默认不杀**、按父进程判孤儿。
    let reaper = read_e2e("reap-orphan-backends.sh");
    // 钉**那条判定表达式本身**，不是「文件里有 yes 这三个字母」。
    // ⚠ 形状 08-13 变过一次：原来是 `[ "${1:-}" = "--yes" ] && YES=1`（只吃一个参数），
    //   `--fixture` 加进来之后改成 `while`+`case` 的解析循环 ⇒ 老针失配，本条当场红。
    //   **它报得对**：要钉的事实是「显式确认恰好一处」，而不是某一种写法。
    //   ⇒ 针跟着形状走（这与 `P0b §1r-4` 记的「针跟不上实现」是同一族，那次是变异存活，
    //   这次是判据直接红 —— 后者好得多，因为它逼人当场对齐）。
    assert!(
        guard_core::find_pinned(&reaper, r#"--yes)     YES=1 ;;"#).is_ok(),
        "一把会杀进程的刀必须要求显式确认（`--yes`），且那一判定要**恰好一处**"
    );
    assert!(
        guard_core::find_pinned(&reaper, r#"ppid="$(ps -o ppid= -p "$pid""#).is_ok(),
        "孤儿判据靠的就是**读父进程**（`ps -o ppid=`）—— 没有它就退回成模式杀"
    );
}

// E1（收孤儿那把刀认得出独立中转进程、只列不收）随那一形删了：盘上再没有独立的中转进程。

/// ★★ `C7i` 红线：**没有任何 e2e 套件靠 `TMUX_TMPDIR` 做隔离**。
///
/// # 红线逐字
///
/// 「tmux 命令**一律带 socket 选择器**（`-S <绝对路径>` 或 `-L <名>`），
/// `kill-server` 不许没有选择器，**禁止靠 `TMUX_TMPDIR`/`unset TMUX` 做隔离**」。
///
/// 它不是洁癖：08-11 一条探针写了 `TMUX_TMPDIR=… tmux kill-server`，`TMUX` 被外层压过
/// ⇒ 命令打到用户**真实**的 server 上，**9 个真实会话没了**。
///
/// # 这条判据的形状变过一次，值得记
///
/// 首版是**递减棘轮**（08-12 实测 4 个脚本 / 15 处，只许降）—— 因为当时以为
/// 「一次修光要真跑那 4 套会起 tmux 的 e2e」。而实际做下来发现：
/// **转 shim 是机械替换，且它的机制能用假 tmux 验证**（喂一个只打印 argv 的假 `tmux`，
/// 看每条命令是不是都被强插了 `-L`）⇒ 债当天就还清了，棘轮随之退役成**零容忍**。
///
/// ⚠ 而首版的抽取器自检（`total >= 10`）**在债还清时会误报** ——
/// 它拿「真实存量」当自检，存量归零就分不清「判定坏了」与「真的没有了」。
/// ⇒ 换成**夹具自检**（下面那三条 `assert!`）：判定对不对，用合成输入问，
/// 不靠「盘上还欠着东西」。
///
/// 要求住址：`INVARIANTS §48.3`（测试里起真后端必须 fail-closed 地隔离用户 tmux）；shell 套件那一侧。
#[test]
fn no_e2e_suite_isolates_with_tmux_tmpdir() {
    // ── 夹具自检：判定本身对不对（不依赖盘上有没有存量）
    assert!(
        bare_tmux_call("  tmux new-session -d -s x"),
        "命令位上的裸调没认出来"
    );
    assert!(
        bare_tmux_call("n=$(tmux list-sessions | wc -l)"),
        "`$(` 之后的裸调没认出来"
    );
    assert!(
        !bare_tmux_call("tmux -L e2eX new-session -d"),
        "带 `-L` 的被误判成裸调"
    );
    assert!(
        !bare_tmux_call("/usr/bin/tmux -S /tmp/s kill-server"),
        "带路径+选择器的被误判"
    );
    assert!(
        !bare_tmux_call(r#"echo "跑一下 tmux ls 看看""#),
        "`echo` 里的文字被误判成调用"
    );

    let dir = crate::guard_support::repo_root().join("tests").join("e2e");
    let mut scanned = 0usize;
    let mut bad: Vec<String> = Vec::new();
    // ⚠ 走 `guard_core::scan_tree!` 而不是自己 `read_dir`（`scanning_guard_registry` 的规矩：
    //   裸遍历的判据会在自己的登记表/注释里找到自己 ⇒ 恒绿）。
    for (f, src) in guard_core::scan_tree!(&dir, &["sh"]) {
        scanned += 1;
        let exec: Vec<&str> = src
            .lines()
            .filter(|l| !l.trim_start().starts_with('#'))
            .collect();
        // ★★ **零例外**。最后一条例外是 `local-backend-supervise.sh`
        //   —— 它把私有目录**喂给被监护的 backend**，所以换 shim 要连 Rust 那侧一起改。
        //   已改：传的不再是 `TMUX_TMPDIR`，而是**带 shim 的 PATH**
        //   （`CCM_E2E_TMUX_SHIM_BIN` → backend 的 `PATH` 前缀）⇒ backend shell out 的
        //   tmux 也被强插 `-L`，而**显式选择器压得过 `$TMUX`**（后者正是 08-11 的机制）。
        let name = f
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        if exec.iter().any(|l| l.contains("TMUX_TMPDIR=")) {
            bad.push(format!("  {name} 自己设 TMUX_TMPDIR 当隔离"));
        }
        // ★★〔08-13 事故补的〕**裸调也要拦**。
        //
        // 本条原来只查 `TMUX_TMPDIR=` —— 而 `bare_tmux_call` 这个判定明明就在上面，
        // 只用在夹具自检里，**从没接到真扫描上**。缝就在这儿：一个套件既不设
        // `TMUX_TMPDIR`、也不挂 shim、直接裸调 `tmux new-session`，它照样全绿，
        // 而那些会话建在**用户的默认 socket** 上。
        //
        // 08-13 实测发生了：`backend-cc-bus.sh` 的头注写着「本套件不用 tmux」，
        // 我后来往里加了 tmux 用例、**照着那句过期的注释省掉了 shim** ⇒
        // 两个 fixture 会话落到了用户的默认 socket 上（事后按精确名字收回）。
        //
        // ⇒ 规则：**要么挂 shim（共享原语或自建），要么每一处都自带选择器**。
        // `--names-only` 只取名字、不装 shim ⇒ 不算挂了共享原语（那样的套件要么自建 shim，要么每处自带选择器）。
        let uses_shared_shim = exec
            .iter()
            .any(|l| l.contains("tmux-shim.sh") && !l.contains("--names-only"));
        // 自建 shim 的形状 = **两件事同时成立**：
        // ① 往一个**名为 `tmux` 的文件**里写；② 那个文件里 `exec` 时带选择器。
        //
        // ⚠ 第一版只找「同一行里有 `exec` + `tmux` + `-L`」，**漏了 `cc-spawn-uplift`**
        //   —— 它写的是 `exec "$REALTMUX" -L $SOCK "$@"`，那一行里**没有字面量 `tmux`**
        //   （真 tmux 的路径在变量里），而且「写文件」与「exec」分在两行（heredoc）。
        //   ⇒ 判定要按**形状**认，别按某一行的字面量。
        let writes_a_tmux_file = exec
            .iter()
            .any(|l| l.contains("/tmux\"") || l.contains("/tmux'") || l.contains("/tmux <<"));
        let execs_with_selector = exec
            .iter()
            .any(|l| l.contains("exec ") && (l.contains(" -L ") || l.contains(" -S ")));
        let installs_own_shim = writes_a_tmux_file && execs_with_selector;
        /// **允许裸调的**（文件名, 为什么）。默认拒绝，例外要写清楚。
        const BARE_TMUX_OK: &[(&str, &str)] = &[
            (
                "ccm-cli.test.sh",
                "那处 `tmux` 在**被断言的字符串**里 —— 它是 `ccm --print` 生成的启动串的一部分\
                     （`if [ -n \"$TMUX\" ]; then …$(tmux display-message …)`），不是本套件在调 tmux。\
                     ⚠ 判定是文本扫描，分不出「引号里的写法」与「真调用」；\
                     与其把判定做成半个 shell 解析器，不如在这里登记一句话。",
            ),
            (
                "gen-idle-tmux.sh",
                "**夹具生成器**，不是套件：它被别的套件调用，隔离由**调用方**的 shim 提供\
                     （调用方 PATH 上有 shim ⇒ 这里的裸 `tmux` 一样被强插 `-L`）。\
                     ⚠ 直接手跑它会打到默认 socket —— 那是它作为「手动造夹具」工具的固有形态，\
                     文件头注已写明用途。",
            ),
        ];
        let exempt = BARE_TMUX_OK.iter().any(|(f, _)| *f == name);
        if !uses_shared_shim && !installs_own_shim && !exempt {
            for l in &exec {
                if bare_tmux_call(l) {
                    bad.push(format!("  {name} 裸调 tmux 且没挂 shim：{}", l.trim()));
                }
            }
        }
    }
    // 抽取器自检：扫到的文件数量级对不上 ⇒ 遍历坏了，上面那条就是零命中得来的。
    assert!(scanned >= 15, "只扫到 {scanned} 个 e2e 脚本 —— 遍历坏了");
    assert!(
        bad.is_empty(),
        "这些套件的 tmux 隔离不合 `C7i`：\n{}\n\
             ⇒ 改用共享原语 `tests/e2e/tmux-shim.sh`（`. tmux-shim.sh <前缀>` 进来），\
             它把 shim 放进 PATH 最前并**强插 `-L`** —— 调用点一个字都不用改。\n\
             ⚠ 裸调那几条同理：**要么挂 shim，要么每一处自带 `-L`/`-S`**。\n\
             08-13 实测过后果：fixture 会话建到了用户的默认 socket 上。",
        bad.join("\n")
    );
}

/// `C7i` 的隔离原语**只许有一份实现**。
///
/// 抽出来之前它在三个套件里**各抄了一份** —— 红线的落地有三份实现，
/// 改一处漏两处正是本仓一路在收的那一族。
///
/// 要求住址：`INVARIANTS §48.3`（测试里起真后端必须 fail-closed 地隔离用户 tmux）；shell 套件那一侧。
#[test]
fn the_tmux_shim_primitive_has_exactly_one_home() {
    let shim = read_e2e("tmux-shim.sh");
    // 它必须真的强插选择器 —— 这条钉的是**机制**，不是「文件在」。
    assert!(
        // ⚠ 针的边界踩过两次坑，如实记：`exec %s -L %s` 左边紧挨着 `\n` 里那个 `n`
        //   ⇒ `find_pinned` 判它没有左边界。⇒ 把左边界含进针里（`'#!/bin/sh` 那段）。
        guard_core::find_pinned(&shim, r#"'#!/bin/sh\nexec %s -L %s"#).is_ok(),
        "shim 没有强插 `-L` —— 那它就只是个包装，隔离仍然靠环境变量"
    );
    assert!(
        // ⚠ 不能只钉 `kill-server`：那个词在头注里也出现（「绝不裸 `kill-server`」）
        //   ⇒ `find_pinned` 要求恰好一处，会拒收。钉**整条带选择器的表达式**。
        guard_core::find_pinned(&shim, r#"-L "$TMUX_SHIM_SOCK" kill-server"#).is_ok(),
        "收尾必须自己带选择器收自己那台（`C7i`：`kill-server` 不许没有选择器）"
    );
}

/// 要求：两趟 e2e 同时跑（两棵工作树的门禁，或同一棵树的两次调用）互不打架 —— 私有 tmux socket、
/// docker 网络与容器的名字一趟一个，只经共享原语 `tests/e2e/tmux-shim.sh` 的 `e2e_run_name` 取。
///
/// 写死的名字下，一边收尾的 `kill-server` / `docker rm` 打掉另一边正在跑的那趟，PASS 数随并发漂；
/// 弱网台架的开跑残留检查还会因别棵树留下的同名容器红。
///
/// 判定（剥整行注释后的可执行行，`tests/e2e/` 整棵树，命中集 == ∅；行尾注释不剥，写进去的字面量照样算）：
/// ① tmux 选择器后面（`-L` / `-S`）是字面量 —— 只放过 `-L default`：canary 问的正是用户那台默认 server；
/// ② docker 的 `--name` / `--network` / `network create|rm` 后面是字面量 —— docker 自带的网络模式
///    （`host` / `none` / `bridge`）不是私有名字，放过；
/// ③ 名字变量（①② 位置上引用过的变量，加上名字里带 `sock` 的）被赋成字面量，
///    或带字面量缺省值（`${X:-lit}` / `${X:=lit}`）—— 值必须从 `$` 起（原语、或别的名字变量派生）；
///    名字带 `sock` 却赋成绝对路径的（X11 的 socket 文件）不是私有名字，放过；
/// ④ JS 里 `"-L", "<字面量>"` 这一形；
/// ⑤ `e2e_run_name()` 只定义在 `tmux-shim.sh` 一处，调了它的文件都 `.` 了那个原语。
/// 正控：下面那几段合成语料，坏的每一段都必须被点名、好的一段都不许被点名。
///
/// 买不到：拼在运行期的名字（`$(echo lit)`、`printf` 拼串）——这条防手滑，不防故意绕。
/// 射程只到 `tests/e2e/`；`tests/evidence/` 下的一次性量具自带名字，不在人群里。
#[test]
fn every_private_e2e_name_comes_from_the_one_primitive() {
    let bad_corpus: &[(&str, &str)] = &[
        ("t1.sh", "tmux -L e2eFoo ls\n"),
        ("t2.sh", "\"$REALTMUX\" -S /tmp/x kill-server\n"),
        ("t3.sh", "SOCK=p3tY5\n"),
        ("t4.sh", "_GC_SOCK=\"e2eGray\"\n"),
        ("t5.sh", "SOCK=(-L e2e-rbind)\n"),
        ("t6.sh", ": \"${CCM_E2E_TMUX_SOCK:=e2eGray}\"\n"),
        (
            "t7.sh",
            "TMUX_SHIM_SOCK=e2eGate2\n. \"$HERE/tmux-shim.sh\"\n",
        ),
        (
            "t8.sh",
            "NET=\"ccmon-weaknet-net\"\ndocker network create \"$NET\"\n",
        ),
        ("t9.sh", "docker run -d --name ccmon-x img sleep 1\n"),
        ("t10.sh", "docker network create --internal ccmon-n\n"),
        (
            "t11.mjs",
            "spawnSync(\"tmux\", [\"-L\", \"e2eX\", \"ls\"]);\n",
        ),
        ("t12.sh", "S=\"sockX\"\n\"$TMUX_BIN\" -L \"$S\" ls\n"),
        (
            "t13.sh",
            "NET=\"${WEAKNET_NET:-ccmon-weaknet-net}\"\ndocker run --network \"$NET\" img\n",
        ),
        ("t14.sh", "local F=\"$W\" sock=lateSrv\n"),
        ("t15.sh", "SOCK=\"$(e2e_run_name p3t)\"\n"),
    ];
    // 每段坏语料都配一份带定义的原语 ⇒ 点名只能来自那段语料自己（⑤ 不会替它们恒红）。
    let shim_def = "e2e_run_name() {\n  printf x\n}\n";
    for (name, src) in bad_corpus {
        let hits = hardcoded_private_names(&[
            (name.to_string(), src.to_string()),
            ("tmux-shim.sh".to_string(), shim_def.to_string()),
        ]);
        assert!(!hits.is_empty(), "判定漏了一形（合成语料 {name}）：\n{src}");
    }
    let good = "\
# tmux -L e2eGray 这是注释\n\
# shellcheck source=tests/e2e/tmux-shim.sh\n\
. \"$HERE/tmux-shim.sh\" --names-only\n\
SOCK=\"$(e2e_run_name p3tY5)\" || exit 2\n\
SOCK8=\"${SOCK}b\"\n\
local F=\"$W/$label\" sock=\"$SOCK$label\"\n\
_GC_SOCK=\"${CCM_E2E_TMUX_SOCK:-$(e2e_run_name e2eGray)}\"\n\
[ -n \"${FAKE_BACKEND_TMUX_SOCK:-}\" ] || echo \"需要 FAKE_BACKEND_TMUX_SOCK=<私有 socket 名>\"\n\
\"$REALTMUX\" -L default has-session -t \"=x\"\n\
\"$REALTMUX\" -L \"$SOCK\" kill-server\n\
printf '#!/bin/sh\\nexec %s -L %s \"$@\"\\n' \"$R\" \"$SOCK\" > \"$B/tmux\"\n\
rsh 'dpkg -L cc-monitor | wc -l'\n\
local sock=\"/tmp/.X11-unix/X${DISP#:}\"\n\
IMG=\"${WEAKNET_IMAGE:-ccmon-weaknet:latest}\"\n\
NET_PRE=ccmon-weaknet-net\n\
NET=\"$(e2e_run_name \"$NET_PRE\")\" || exit 2\n\
docker network create --internal \"$NET\"\n\
docker run -d --name \"$CA\" --network \"$NET\" --cap-add=NET_ADMIN \"$IMG\" sleep infinity\n\
CA=\"$(e2e_run_name ccmon-weaknet-a)\"\n\
tmux select-pane -L -t x; tmux capture-pane -p -S -50\n";
    let hits = hardcoded_private_names(&[
        ("good.sh".to_string(), good.to_string()),
        ("tmux-shim.sh".to_string(), shim_def.to_string()),
    ]);
    assert!(hits.is_empty(), "好语料被误点名：\n{}", hits.join("\n"));

    let dir = crate::guard_support::repo_root().join("tests").join("e2e");
    let files: Vec<(String, String)> = guard_core::scan_tree!(&dir, &["sh", "mjs", "mts", "ts"])
        .into_iter()
        .map(|(p, s)| (p.to_string_lossy().replace('\\', "/"), s))
        .collect();
    assert!(
        files.iter().filter(|(p, _)| p.ends_with(".sh")).count() >= 30,
        "只扫到 {} 份 `tests/e2e/**/*.sh` —— 扫描坏了（建判据当天 44 份）",
        files.iter().filter(|(p, _)| p.ends_with(".sh")).count()
    );
    let hits = hardcoded_private_names(&files);
    assert!(
        hits.is_empty(),
        "`tests/e2e/` 下有写死的私有名字（tmux socket / docker 网络 / 容器）—— 两趟同时跑会互相打掉：\n{}\n\
         ⇒ 只给前缀，名字经共享原语取：`. tmux-shim.sh <前缀>`（装 shim）或 \
         `. tmux-shim.sh --names-only` 后 `X=\"$(e2e_run_name <前缀>)\"`。",
        hits.join("\n")
    );
}

/// 一个参数位上的值是不是「引用」（变量 / 命令替换 / printf 占位），不是字面量。
fn name_arg_is_ref(tok: &str) -> bool {
    let t = tok.trim_start_matches(['"', '\'']);
    t.starts_with('$') || t.starts_with("%s")
}

/// `"$X"` / `${X}` / `"$X$y"` / `"${X}b"` → `X`。
fn referenced_var(tok: &str) -> Option<String> {
    let t = tok.trim_start_matches(['"', '\'']);
    let t = t.strip_prefix('$')?;
    let t = t.strip_prefix('{').unwrap_or(t);
    let v: String = t
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
        .collect();
    (!v.is_empty() && !v.starts_with(|c: char| c.is_ascii_digit())).then_some(v)
}

/// `rest` 开头那个参数（到空白或 `)` 为止）。
fn first_arg(rest: &str) -> &str {
    rest.trim_start()
        .split(|c: char| c.is_whitespace() || c == ')')
        .next()
        .unwrap_or("")
}

/// 一行里所有「名字参数位」：tmux 的 `-L`/`-S` 与 docker 的 `--name`/`--network`/`network create|rm`。
/// 交出 `(位置说明, 那个参数)`；`-L default` 不交（canary 问的是用户那台默认 server）。
fn name_arg_slots(line: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let bytes = line.as_bytes();
    for flag in ["-L", "-S"] {
        let mut i = 0usize;
        while let Some(off) = line[i..].find(flag) {
            let at = i + off;
            i = at + flag.len();
            let left_ok = at == 0 || matches!(bytes[at - 1], b' ' | b'\t' | b'(' | b'"');
            let right_ok = bytes.get(at + flag.len()).is_some_and(|b| *b == b' ');
            if !left_ok || !right_ok {
                continue;
            }
            let seg = line[..at].rsplit([';', '|', '&']).next().unwrap_or("");
            if !(seg.to_ascii_lowercase().contains("tmux") || seg.contains("exec %s")) {
                continue;
            }
            let arg = first_arg(&line[i..]);
            // 子命令自己的 `-L` / `-S`（`select-pane -L`、`capture-pane -S -50`）后面不是名字。
            if arg.is_empty() || arg.starts_with('-') || arg.bytes().all(|b| b.is_ascii_digit()) {
                continue;
            }
            if flag == "-L" && arg == "default" {
                continue;
            }
            out.push((format!("tmux {flag}"), arg.to_string()));
        }
    }
    if line.contains("docker") {
        for flag in ["--name", "--network"] {
            let mut i = 0usize;
            while let Some(off) = line[i..].find(flag) {
                let at = i + off;
                i = at + flag.len();
                let rest = &line[i..];
                let rest = match rest.as_bytes().first() {
                    Some(b' ') | Some(b'=') => &rest[1..],
                    _ => continue,
                };
                let arg = first_arg(rest);
                // docker 自带的网络模式（`docker build --network host` 装包那一步）不是私有名字；
                // 运行面用不用宿主网络归 `guard-run-netns.sh` 管。
                if flag == "--network" && matches!(arg, "host" | "none" | "bridge" | "default") {
                    continue;
                }
                out.push((format!("docker {flag}"), arg.to_string()));
            }
        }
        for sub in ["network create", "network rm"] {
            if let Some(at) = line.find(sub) {
                let name = line[at + sub.len()..]
                    .split_whitespace()
                    .find(|t| !t.starts_with('-'))
                    .unwrap_or("");
                let name = name
                    .split(|c: char| c == ')' || c == ';')
                    .next()
                    .unwrap_or("");
                out.push((format!("docker {sub}"), name.to_string()));
            }
        }
    }
    out
}

/// 一行打头那串赋值（`export` / `local` / `readonly` / `declare` 之后、命令之前）：`(变量名, 值)`。
/// 值按 shell 的引号与 `$(…)` 配对切 —— 不在命令位上的 `X=…`（比如 `echo` 文案里那种）不算赋值。
fn leading_assignments(line: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut s = line.trim_start();
    for kw in ["export ", "local ", "readonly ", "declare "] {
        if let Some(r) = s.strip_prefix(kw) {
            s = r.trim_start();
            while let Some(r) = s.strip_prefix('-') {
                s = r
                    .trim_start_matches(|c: char| c.is_ascii_alphabetic())
                    .trim_start();
            }
            break;
        }
    }
    loop {
        let name: String = s
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
            .collect();
        if name.is_empty() || name.starts_with(|c: char| c.is_ascii_digit()) {
            break;
        }
        let Some(rest) = s[name.len()..].strip_prefix('=') else {
            break;
        };
        let (mut depth, mut quote, mut end) = (0i32, None::<char>, rest.len());
        let chars: Vec<(usize, char)> = rest.char_indices().collect();
        let mut k = 0usize;
        while k < chars.len() {
            let (pos, c) = chars[k];
            match (quote, c) {
                (Some(q), c) if c == q && (q == '\'' || depth == 0) => quote = None,
                (Some('"'), '\\') => k += 1,
                (None, '\\') => k += 1,
                (None, '"') | (None, '\'') => quote = Some(c),
                (_, '(') if pos > 0 && rest.as_bytes()[pos - 1] == b'$' => depth += 1,
                (None, '(') => depth += 1,
                (_, ')') if depth > 0 => depth -= 1,
                (None, c) if c.is_whitespace() && depth == 0 => {
                    end = pos;
                    break;
                }
                _ => {}
            }
            k += 1;
        }
        out.push((name, rest[..end].to_string()));
        s = rest[end..].trim_start();
    }
    out
}

/// 值是不是「从 `$` 起、且不带字面量缺省值」。
fn assigned_from_ref(value: &str) -> bool {
    let v = value.trim_start_matches(['"', '\'']);
    if !v.starts_with('$') {
        return false;
    }
    if let Some(inner) = v.strip_prefix("${") {
        for op in [":-", ":="] {
            if let Some(at) = inner.find(op) {
                let dflt = &inner[at + op.len()..];
                return dflt.starts_with('$') || dflt.starts_with('}');
            }
        }
    }
    true
}

/// `${X:-lit}` / `${X:=lit}` 里的 `(X, lit 的头一个字)`，行里任意位置。
fn parameter_defaults(line: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let b = line.as_bytes();
    for open in 0..b.len().saturating_sub(1) {
        if b[open] != b'$' || b[open + 1] != b'{' {
            continue;
        }
        let at = open + 2;
        let name: String = line[at..]
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
            .collect();
        let rest = &line[at + name.len()..];
        if name.is_empty() {
            continue;
        }
        for op in [":-", ":="] {
            if let Some(d) = rest.strip_prefix(op) {
                out.push((name.clone(), d.to_string()));
            }
        }
    }
    out
}

/// 这份路径是不是共享原语本身（按路径最后一段整段比，不按后缀）。
fn is_the_primitive(path: &str) -> bool {
    std::path::Path::new(path).file_name() == Some(std::ffi::OsStr::new("tmux-shim.sh"))
}

/// 判定本体：`(路径, 源码)` → 命中（`路径: 说明`）。每份文件先收自己的「名字变量」，再查它们的赋值；
/// 带 `sock` 的变量名跨文件也认（`TMUX_SHIM_SOCK` 在套件里赋、在原语里用）。
fn hardcoded_private_names(files: &[(String, String)]) -> Vec<String> {
    let is_js = |p: &str| p.ends_with(".mjs") || p.ends_with(".mts") || p.ends_with(".ts");
    let mut hits = Vec::new();
    let mut defined_in: Vec<&str> = Vec::new();
    for (path, src) in files {
        if is_js(path) {
            for flag in ["\"-L\"", "'-L'", "\"-S\"", "'-S'"] {
                let mut i = 0usize;
                while let Some(off) = src[i..].find(flag) {
                    let at = i + off + flag.len();
                    i = at;
                    let next = src[at..].trim_start().trim_start_matches(',').trim_start();
                    let lit = next.starts_with('"')
                        || next.starts_with('\'')
                        || (next.starts_with('`') && !next[1..].starts_with("${"));
                    if lit {
                        hits.push(format!("{path}: tmux {flag} 后面是字面量"));
                    }
                }
            }
            continue;
        }
        let code = strip_comments(src);
        let lines: Vec<&str> = code.lines().collect();
        let mut name_vars: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        for line in &lines {
            for (slot, arg) in name_arg_slots(line) {
                if name_arg_is_ref(&arg) {
                    if let Some(v) = referenced_var(&arg) {
                        name_vars.insert(v);
                    }
                } else {
                    hits.push(format!(
                        "{path}: {slot} 后面是字面量 `{arg}`：{}",
                        line.trim()
                    ));
                }
            }
        }
        let watched = |v: &str| name_vars.contains(v) || v.to_ascii_lowercase().contains("sock");
        if lines
            .iter()
            .any(|l| l.trim_start().starts_with("e2e_run_name()"))
        {
            defined_in.push(path);
        }
        let calls = lines
            .iter()
            .any(|l| l.contains("e2e_run_name ") && !l.contains("e2e_run_name()"));
        let sources = lines.iter().any(|l| l.contains("tmux-shim.sh"));
        if calls && !sources && !is_the_primitive(path) {
            hits.push(format!(
                "{path}: 调了 e2e_run_name 却没 `.` 共享原语 tmux-shim.sh"
            ));
        }
        for line in &lines {
            for (var, value) in leading_assignments(line) {
                if !watched(&var) {
                    continue;
                }
                let path_like = !name_vars.contains(&var)
                    && value.trim_start_matches(['"', '\'']).starts_with('/');
                if !assigned_from_ref(&value) && !path_like {
                    hits.push(format!(
                        "{path}: 名字变量 {var} 赋成了字面量：{}",
                        line.trim()
                    ));
                }
            }
            for (var, dflt) in parameter_defaults(line) {
                if watched(&var) && !(dflt.starts_with('$') || dflt.starts_with('}')) {
                    hits.push(format!(
                        "{path}: 名字变量 {var} 带字面量缺省值：{}",
                        line.trim()
                    ));
                }
            }
        }
    }
    if defined_in.len() != 1 || !is_the_primitive(defined_in[0]) {
        hits.push(format!(
            "e2e_run_name() 应当恰好定义在 tests/e2e/tmux-shim.sh 一处，实得：{defined_in:?}"
        ));
    }
    hits
}

/// 命令位上的裸 `tmux`（没有 `-L`/`-S` 紧跟）。
///
/// ⚠ 判定要**两条都满足**，缺一条就会数出别的东西：
/// ① `tmux` 前面（去空白后）是**行首**或 `;` `|` `&` `(` —— 即它在命令位；
/// ② 后面紧跟的是**小写子命令**（`new-session` / `ls` / …）。
/// 少了②，`echo "… tmux ls …"` 这类文字会被算进来 —— 首版就是这么数出 28 的
/// （另一把尺数 15）。**两把尺不一致就不能立棘轮**，先把尺修对。
fn bare_tmux_call(line: &str) -> bool {
    let b = line.as_bytes();
    let mut i = 0usize;
    while let Some(off) = line[i..].find("tmux ") {
        let at = i + off;
        let before = line[..at].trim_end();
        let cmd_pos = before.is_empty()
            || before.ends_with(';')
            || before.ends_with('|')
            || before.ends_with('&')
            || before.ends_with('(');
        let pathish = at > 0 && b[at - 1] == b'/';
        let after = line[at + 5..].trim_start();
        let sub_cmd = after.chars().next().is_some_and(|c| c.is_ascii_lowercase());
        let selected = after.starts_with("-L ") || after.starts_with("-S ");
        if cmd_pos && sub_cmd && !selected && !pathish {
            return true;
        }
        i = at + 5;
    }
    false
}

fn read_e2e(name: &str) -> String {
    std::fs::read_to_string(
        crate::guard_support::repo_root()
            .join("tests")
            .join("e2e")
            .join(name),
    )
    .unwrap_or_else(|e| panic!("读不到 tests/e2e/{name}：{e}"))
}

/// shell 的「剥生产段」：只留可执行行。
/// 与 Rust 侧 `guard_core::production_code` 同一个用意 —— 判据不该被**解释它自己的散文**喂饱。
fn strip_comments(src: &str) -> String {
    src.lines()
        .filter(|l| !l.trim_start().starts_with('#'))
        .collect::<Vec<_>>()
        .join("\n")
}

/// `backend-wrapper.sh` 是夹具：**在仓里直接跑就拒**，一个字节都不交给后端。
///
/// 起因（09-27 实发）：分流不看 argv0 之后，没有打头 `--` 的调用就是「起 claude」⇒ 在仓里直接跑它
/// 起了一次 PATH 上**真的** claude。合法的叫法只有台架那一形（`tier2-rig.sh` 拷进台架目录、旁边放
/// `backend-path`，由 app 经 SSH 执行）—— SSH exec 不带 env，所以认的是那个文件。
/// 两向：① 仓里那份直接跑 ⇒ 退 2、说清是夹具、替身后端一次都没被叫到、tap 一个字节没写；
/// ①b 台架目录缺 `tmux-sock` ⇒ 同样退 2（没有私有 socket 名就没有隔离可给后端）；
/// ② 正控：台架那一形 ⇒ 真转到替身（否则 ① 可以靠「这脚本什么都转不过去」绿）。
/// PATH 只放它要的那几个工具、**不放 tmux** ⇒ 正控那一趟不在 `/tmp` 下建 tmux 垫片目录。
#[cfg(unix)]
#[test]
fn the_backend_wrapper_fixture_refuses_to_run_outside_a_rig() {
    use std::os::unix::fs::PermissionsExt;
    let d = std::env::temp_dir().join(format!("ccm-wrapper-guard-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    let bin = d.join("bin");
    std::fs::create_dir_all(&bin).expect("建临时目录");
    let path = std::env::var_os("PATH").unwrap_or_default();
    for tool in [
        "sh", "cat", "dirname", "date", "head", "stdbuf", "tee", "env",
    ] {
        let found = std::env::split_paths(&path)
            .map(|p| p.join(tool))
            .find(|p| p.is_file())
            .unwrap_or_else(|| panic!("这台机器上没有 {tool}"));
        std::os::unix::fs::symlink(found, bin.join(tool)).expect("链工具");
    }
    let reached = d.join("reached");
    let stub = d.join("stub-backend");
    std::fs::write(
        &stub,
        format!(
            "#!/bin/sh\nprintf '%s\\n' \"$*\" > '{}'\n",
            reached.display()
        ),
    )
    .expect("写替身后端");
    std::fs::set_permissions(&stub, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    let tap = d.join("tap");
    let run = |script: &std::path::Path| {
        std::process::Command::new(bin.join("sh"))
            .arg(script)
            .args(["--", "--ping"])
            .env_clear()
            .env("PATH", &bin)
            .env("HOME", &d)
            .env("CCM_E2E_BACKEND", &stub)
            .env("CCM_E2E_FRAME_TAP", &tap)
            .env("CCM_E2E_CLAUDE_DIR", d.join("claude"))
            .output()
            .expect("起 sh")
    };
    let wrapper = crate::guard_support::repo_root()
        .join("tests")
        .join("e2e")
        .join("backend-wrapper.sh");

    // ① 仓里那份直接跑。
    let out = run(&wrapper);
    let err = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(2), "直接跑没被拒：{err}");
    assert!(
        err.contains("是夹具") && err.contains("tier-2 台架"),
        "拒了但没说清是谁的夹具：{err}"
    );
    assert!(!reached.exists(), "拒之前已经把参数交给后端了");
    assert!(
        !d.join("tap.err").exists(),
        "拒之前已经写了 tap（副作用在防呆前面）"
    );

    // ①b 台架目录里缺 `tmux-sock`（台架那份私有 tmux socket 名）⇒ 没有隔离可给后端，同样拒。
    let rig = d.join("rig");
    std::fs::create_dir_all(&rig).expect("建台架目录");
    std::fs::copy(&wrapper, rig.join("backend-wrapper.sh")).expect("拷夹具");
    std::fs::write(rig.join("backend-path"), stub.to_string_lossy().as_bytes())
        .expect("写 backend-path");
    let out = run(&rig.join("backend-wrapper.sh"));
    assert_eq!(
        out.status.code(),
        Some(2),
        "缺 tmux-sock 没被拒：{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(!reached.exists(), "缺 tmux-sock 时已经把参数交给后端了");

    // ② 正控：台架那一形（两份文件都在）。
    std::fs::write(rig.join("tmux-sock"), b"e2eWrapperProbe").expect("写 tmux-sock");
    let out = run(&rig.join("backend-wrapper.sh"));
    assert!(
        out.status.success(),
        "台架那一形没转过去：{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        std::fs::read_to_string(&reached).ok().as_deref(),
        Some("-- --ping\n"),
        "台架那一形没把参数原样交给后端"
    );
    let _ = std::fs::remove_dir_all(&d);
}

/// `tests/e2e/` 底下**一处模式杀都不许有**（`pkill` / `killall`，剥注释后）。
///
/// 模式杀没有「只杀我起的那些」这个概念：打到什么由命令行长相决定。本仓第三次栽在这上面是
/// `local-backend-supervise.sh` 收尾的 `pkill -f "$BACKEND"` —— 它打死了**跑这套件的那条调用方 shell**，
/// 而且替一条漏网的判据（`KPY2` 第一个宿主起的后端没人收）兜了一个月。收尸一律按自己起的 pid /
/// 进程表里的事实（`/proc/<pid>/exe` ＋ 环境）认。上面 `the_harness_no_longer_teaches_a_bare_pattern_kill`
/// 只钉一份文件；这一条钉整棵树。
#[test]
fn no_e2e_script_kills_by_pattern() {
    let root = crate::guard_support::repo_root().join("tests").join("e2e");
    let files: Vec<(std::path::PathBuf, String)> = guard_core::scan_tree!(&root, &["sh"]);
    assert!(
        files.len() >= 30,
        "只扫到 {} 份 `tests/e2e/**/*.sh` —— 扫描坏了（建判据当天 40 份）",
        files.len()
    );
    let bad: Vec<String> = files
        .iter()
        .filter(|(_, src)| {
            let code = strip_comments(src);
            guard_core::contains_word(&code, "pkill") || guard_core::contains_word(&code, "killall")
        })
        .map(|(p, _)| p.display().to_string())
        .collect();
    assert!(
        bad.is_empty(),
        "这几份 e2e 的可执行段里有模式杀（`pkill` / `killall`）—— 改成按自己起的 pid 收：{bad:?}"
    );
}

/// 写路径之前「继承开发机默认值」的那几个变量：开发者的 shell rc 里常 export 着它们、指向真家目录 / 真账号目录 / 真 cc-bus /
/// 真凭据表 / 真数据目录。`${VAR:-沙箱}` 这种写法在那台机器上会照用真值、再往里写测试数据（10-01 实发过一次：
/// 当时另指账号库位置的那个变量指着真清单，真账号清单被改写成两个测试号；那个变量今天整个删了，账号库只跟着家走）。
const INHERITED_PATH_VARS: &[&str] = &[
    "HOME",
    "CLAUDE_CONFIG_DIR",
    "CC_BUS_HOME",
    "CCM_APIKEY_CREDENTIALS",
    "CCM_DATA_DIR",
];

/// 留着这种写法的那几处（`文件 · 变量 · 为什么不是往真路径里写`）。每一条都要是**只读**，或者另有一道闸挡住写。
const INHERITED_PATH_ALLOWED: &[(&str, &str, &str)] = &[
    (
        "tests/e2e/fake-claude",
        "CLAUDE_CONFIG_DIR",
        "假启动器扮的就是 claude：账号目录由起它的那一方交（ccm 按账号设）；没交才落 /tmp，交来的不在 /tmp 下就拒写",
    ),
    (
        "tests/e2e/gen-idle-tmux.sh",
        "CLAUDE_CONFIG_DIR",
        "只读：把调用方交来的那个目录原样转给 pane 里的假启动器（落点由假启动器那道 /tmp 闸兜着）",
    ),
    (
        "tests/e2e/p3t-local-tmux.sh",
        "CLAUDE_CONFIG_DIR",
        "只读：假启动器把自己拿到的那个值记进本趟日志，不往那里写",
    ),
    (
        "tests/e2e/cc-bus-queue-drain.sh",
        "CC_BUS_HOME",
        "只读：按 cc-send 同一个取法算出脚本眼里的 bus，断言它落在沙箱里（落不进 ⇒ 拒跑）",
    ),
];

/// 一段脚本（剥掉整行注释）里 `${VAR:-` 形的命中：`(变量, 次数)`。`\${VAR:-`（写进另一份脚本里的）同样算。
fn inherited_path_defaults(text: &str) -> Vec<(String, usize)> {
    let code = guard_core::strip_comment_lines(&guard_core::strip_hash_comment_lines(text));
    INHERITED_PATH_VARS
        .iter()
        .filter_map(|v| {
            let n = code.matches(&format!("${{{v}:-")).count();
            (n > 0).then(|| (v.to_string(), n))
        })
        .collect()
}

/// ★★ **`tests/e2e/` 下不许写「先继承开发机的值、没有才用沙箱」的路径**（`${HOME:-` · `${CLAUDE_CONFIG_DIR:-` ·
/// `${CC_BUS_HOME:-` · …，见 [`INHERITED_PATH_VARS`]）：沙箱路径一律无条件给。只读透传的那几处登记理由（[`INHERITED_PATH_ALLOWED`]），两向相等。
#[test]
fn no_e2e_script_inherits_a_dev_machine_path_it_may_write_to() {
    // 正控：现造的语料里该中的中（含写进另一份脚本里的 `\${`），注释行不中，无条件的写法不中。
    let corpus = "export HOME=\"${HOME:-/tmp/x}\"\n\
                  # ${CLAUDE_CONFIG_DIR:-只在注释里}\n\
                  printf '%s' \"\\${CC_BUS_HOME:-}\"\n\
                  export CLAUDE_CONFIG_DIR=\"$SBX/claude\"\n";
    assert_eq!(
        inherited_path_defaults(corpus),
        [("HOME".to_string(), 1), ("CC_BUS_HOME".to_string(), 1)],
        "针在现造语料上失准"
    );
    let root = crate::guard_support::repo_root();
    let dir = root.join("tests").join("e2e");
    let files = guard_core::files_under(&dir);
    assert!(
        files.len() > 30,
        "tests/e2e/ 下只看到 {} 份 —— 扫描面塌了",
        files.len()
    );
    let mut found: Vec<(String, String)> = Vec::new();
    for f in &files {
        let Ok(text) = std::fs::read_to_string(dir.join(f)) else {
            continue; // 非文本（二进制夹具）
        };
        for (v, _) in inherited_path_defaults(&text) {
            found.push((format!("tests/e2e/{f}"), v));
        }
    }
    found.sort();
    let mut want: Vec<(String, String)> = INHERITED_PATH_ALLOWED
        .iter()
        .map(|(f, v, _)| (f.to_string(), v.to_string()))
        .collect();
    want.sort();
    assert_eq!(
        found, want,
        "tests/e2e/ 里「先继承开发机的值」的路径写法与登记表对不上。多出来的 ⇒ 改成无条件指向沙箱（开发机 rc 里的真值会被照用、再被写坏）；\
         少了的 ⇒ 那一处改掉了，同拍把登记摘掉"
    );
    for (f, _, why) in INHERITED_PATH_ALLOWED {
        assert!(
            why.chars().count() >= 15,
            "`{f}` 没写清为什么它不是往真路径里写"
        );
    }
}
