//! **每一套 e2e 要么进门禁，要么登记为什么不进**〔audit-0805 08-08，Phase G 第 73 件〕。
//!
//! # 这个病已经发生过一次，而仍然没人守着下一次
//!
//! `ci.yml` 自己逐字记着（G-C 那段）：
//!
//! > Phase G 审计：G2 新增的这套**只有 npm 脚本、没进任何门禁**。它已具备入网条件
//! > （…），只是当时**忘了接线**。对比 `graylight-suite` 的排除在 `e2e/README.md` 里
//! > 是有论证的，这两套什么都没写 —— 那正是**「新东西入网漏一拍」的形状**。
//!
//! 那一次是**人**发现的。今天守着这件事的是 `ci.yml` 里那条
//! `[ "$N" -ge 19 ]` 计数地板 + 一张手写的 19 对 `(套名, 地板)` 反向自检 ——
//! 它挡得住「已登记的那 19 套被改回裸 `npm run` 或被删」，
//! **挡不住「第 20 套根本没登记」**：`N` 还是 19，19 对也全对得上，CI 照旧绿。
//!
//! ⇒ 人群改从 **`package.json` 派生**（哪些 `test:*` 真的在跑 `e2e/`），默认拒绝。
//!
//! # 三条出路，第三条要写理由
//!
//! 1. `ci.yml` 里有 `assert-pass-floor.sh <套名> <地板>`（19 套走这条）；
//! 2. `ci.yml` 里有步骤**直接跑那个脚本路径**（`exec-bits` 走这条 —— 它是个 guard，
//!    不打印「合计 PASS=」，没有断言数可言）；
//! 3. 登记在 [`EXEMPT`]，**并写清为什么**。
//!
//! ★ **先核（E1）救了一次**：`graylight-suite` 与 `f40-suite` 看起来是两个洞，
//! 而它们的排除**在 `e2e/README.md` 里是有论证的**（要 Xvfb 上跑着 `npx tauri dev`
//! 的真 app、断言源是运行中 app 写的日志、不打印「合计 PASS=」）——理由成立。
//! 它们不是「忘了接线」，是**结构上进不了无头 CI**。⇒ 登记，不是「顺手补进去」。
//!
//! # 顺带钉住那个「19」的四份副本
//!
//! 同一个数今天在 `ci.yml` 里存了**四份**（注释里的散文 · 步骤名 · `-ge` 那行 · 收尾 echo），
//! 而那段注释自己就记着这四份漂过：「本行与下面 `:313` 原写「8」，是 E82 订正下方散文时
//! 漏跟的两处」「套数也从 8 长到了 15」。⇒ 本模块要求**四份都等于派生出来的真值**：
//! 加一套就必须四处一起改，而诊断会直接给出该写的数（E3 的退而求其次形态 ——
//! 副本消不掉时，至少让它们对拍）。

#[cfg(test)]
mod tests {
    /// **刻意不进 CI 门禁的 e2e 套**（`test:` 后缀, 为什么）。
    ///
    /// 默认拒绝：不在这里、又没有地板行、又没被直接跑的套，正题判据会点名。
    const EXEMPT: &[(&str, &str)] = &[
        (
            "graylight",
            "全链级：断言源是 Xvfb 上**正在跑的 dev app**（`npx tauri dev`）写的 monitor 日志，\
             无头 CI 里没有那个 app；论证写在 `e2e/README.md`「`graylight-suite`（全链级）不在上表那些套件里」",
        ),
        (
            "f40",
            "渲染/滚动管线级：同 `graylight-suite` 的规格（要跑着的 dev app + Xvfb），\
             且它**不打印「合计 PASS=」**、断言数随环境分支变 ⇒ 连 `assert-pass-floor` 的度量口径都不成立；\
             论证写在 `e2e/README.md`（U0 2026-08-01 补写那两段）",
        ),
    ];

    fn package_json() -> String {
        let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("仓根")
            .join("package.json");
        std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("读不到 {p:?}: {e}"))
    }

    /// 人群：`package.json` 里**真的在跑 `e2e/` 的** `test:*` 脚本 → `(套名, 脚本路径)`。
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
            if !cmd.contains("e2e/") {
                continue;
            }
            let Some(path) = cmd.split_whitespace().find(|w| w.starts_with("e2e/")) else {
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
            let Some(rest) = t.split_once("run: bash e2e/assert-pass-floor.sh ") else {
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

    /// `P0b`：全链台架的「没有孤儿 daemon」那一格，**人群要覆盖两族**。
    ///
    /// # 它防的是一个实测出来的洞
    ///
    /// 那一格原来只数 `remote-daemon-proto/target/...`，而 08-12 实测：盘上活着的 daemon
    /// 走的是**部署落点** `~/.cc-monitor/bin/cc-monitor-remote`（`sftp::ensure_daemon_deployed`
    /// 的落点）—— **那一族当时根本不在人群里**，计数器却会安心地报 0。
    /// 这正是 `needle_anchor_registry` 那条：**匹配单位不许比事实小**。
    ///
    /// ★★ 本条钉的是**那两条 `pgrep` 表达式本身**（`find_pinned`：恰好一处 + 两侧有边界），
    /// 不是「字面量在文件里出现过」—— 首跑变异当场证明后者不成立：把 `pgrep` 那行删掉，
    /// 同一个字面量在**我自己写的说明注释**里还在，判据照样绿。
    ///
    /// ⚠ 本条钉「两族都数」，**不保证**盘上没有第三族。哪天 daemon 又多一个落点，
    /// 这条会因为「新落点不在表里」而**沉默**，不会报。如实登记。
    #[test]
    fn the_orphan_daemon_gate_counts_both_families() {
        let raw = read_e2e("graylight-suite.sh");
        let src = strip_comments(&raw);
        for expr in [
            "pgrep -fc 'remote-daemon-proto/target/[^ ]*/cc-monitor-remote'",
            "pgrep -fc '\\.cc-monitor/bin/cc-monitor-remote'",
        ] {
            assert!(
                guard_core::find_pinned(&src, expr).is_ok(),
                "孤儿 daemon 那一格的**可执行行**里少了这条计数：`{expr}`。\
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
            guard_core::find_pinned(&src, "reap-orphan-daemons.sh").is_ok(),
            "得给出替代品，否则被 ABORT 拦住的人只会自己去敲 pkill"
        );
        // 那把刀本身必须在，且**默认不杀**、按父进程判孤儿。
        let reaper = read_e2e("reap-orphan-daemons.sh");
        // 钉**那条判定表达式本身**，不是「文件里有 yes 这三个字母」。
        assert!(
            guard_core::find_pinned(&reaper, r#"[ "${1:-}" = "--yes" ] && YES=1"#).is_ok(),
            "一把会杀进程的刀必须要求显式确认（`--yes`），且那一判定要**恰好一处**"
        );
        assert!(
            guard_core::find_pinned(&reaper, r#"ppid="$(ps -o ppid= -p "$pid""#).is_ok(),
            "孤儿判据靠的就是**读父进程**（`ps -o ppid=`）—— 没有它就退回成模式杀"
        );
    }

    /// ★★ `C7i` 红线的**存量债棘轮**〔`P0e` 08-12 实测〕：
    /// **靠 `TMUX_TMPDIR` 做隔离的 e2e 脚本里，裸 `tmux` 调用只许变少。**
    ///
    /// # 红线逐字
    ///
    /// 「tmux 命令**一律带 socket 选择器**（`-S <绝对路径>` 或 `-L <名>`），
    /// `kill-server` 不许没有选择器，**禁止靠 `TMUX_TMPDIR`/`unset TMUX` 做隔离**」。
    ///
    /// 它不是洁癖：08-11 一条探针写了 `TMUX_TMPDIR=… tmux kill-server`，
    /// `TMUX` 被外层压过 ⇒ 命令打到用户**真实**的 server 上，**9 个真实会话没了**。
    ///
    /// # 为什么是棘轮而不是当场修光
    ///
    /// `P0d` 把**三套**换成了 `-L` shim，但它的人群是「那三套 + 三套已合规」——
    /// 今天实测**还有 4 套在人群外**（`inbound-daemon-frames` / `restart-suite` /
    /// `resume-daemon-frames` / `resume-suite`，合计 **15 处**裸调）。
    /// ⚠ 而 `P0d` 的 acceptor 是**一次性实测** ⇒ 既拦不住回潮，也发现不了人群外的。
    ///
    /// 一次修光要**真跑这 4 套 e2e**（它们会起真 tmux）—— 而那正是 08-11 事故的形态。
    /// ⇒ 先钉住「不许再长」，修的时候连同真跑一起做（那需要一个能盯着的窗口）。
    ///
    /// ⚠ **失效模式如实登记**：命令位的判定是启发式（行首 / `;` / `|` / `$(` / `&&` 之后）。
    /// 写成 `eval "tm$x new-session"` 这类绕得过去；`echo` 里提到 tmux 会**误计**
    /// （宁可多算：多算只会让棘轮更紧，少算才会漏）。
    #[test]
    fn bare_tmux_in_tmpdir_isolated_suites_only_goes_down() {
        /// 08-12 全树实测：4 个脚本 / 15 处。**只许降。**
        const CEILING: usize = 15;

        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("e2e");
        let mut total = 0usize;
        let mut detail: Vec<String> = Vec::new();
        // ⚠ 走 `guard_core::scan_tree!` 而不是自己 `read_dir` —— `scanning_guard_registry`
        // 逐字要求：裸遍历的判据「在自己的登记表/注释/常量里找到自己 ⇒ **恒绿**，
        // audit-0805 实测五次，**五次都不是被判据变红发现的**」。
        // ★ 本条扫的是 `.sh`、自己是 `.rs`，**按构造读不到自己** —— 但仍走共享原语：
        //   规矩是「用那把尺」，不是「证明这次不会自伤」（下一个抄这段的人可能就扫 `.rs` 了）。
        for (f, src) in guard_core::scan_tree!(&dir, &["sh"]) {
            let exec: Vec<&str> = src
                .lines()
                .filter(|l| !l.trim_start().starts_with('#'))
                .collect();
            // 人群：**自己**设 `TMUX_TMPDIR` 的（= 拿它当隔离），不是提一句的。
            if !exec.iter().any(|l| l.contains("TMUX_TMPDIR=")) {
                continue;
            }
            let n = exec.iter().filter(|l| bare_tmux_call(l)).count();
            if n > 0 {
                total += n;
                detail.push(format!("  {} : {n}", f.file_name().unwrap_or_default().to_string_lossy()));
            }
        }
        assert!(
            total <= CEILING,
            "靠 `TMUX_TMPDIR` 隔离的套件里，裸 `tmux` 调用有 {total} 处 > 棘轮上限 {CEILING}。\n\
             ★ `C7i` 红线逐字：**禁止靠 `TMUX_TMPDIR`/`unset TMUX` 做隔离** —— \
             08-11 那条探针就是这么把用户 **9 个真实会话**打没的。\n\
             正确形状抄 `P0d`：一个 `$BIN/tmux` shim（`exec` 真 tmux + 强插 `-L <私有名>`）进 PATH 最前，\
             调用点一个字都不用改（`graylight-suite.sh` 是现成的样板）。\n\
             ⚠ **不许把上限调上去让今天好过** —— 这是递减棘轮。\n\
             当前分布：\n{}",
            detail.join("\n")
        );
        // 抽取器自检：一处都数不出来 ⇒ 人群或判定坏了，上面那条就是空绿。
        assert!(
            total >= 10,
            "只数出 {total} 处 —— 判定坏了（08-12 实测 15 处），\
             那样「没超上限」是零命中得来的，不是真的"
        );
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
            let sub_cmd = after
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_lowercase());
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
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .unwrap()
                .join("e2e")
                .join(name),
        )
        .unwrap_or_else(|e| panic!("读不到 e2e/{name}：{e}"))
    }

    /// shell 的「剥生产段」：只留可执行行。
    /// 与 Rust 侧 `guard_core::production_code` 同一个用意 —— 判据不该被**解释它自己的散文**喂饱。
    fn strip_comments(src: &str) -> String {
        src.lines()
            .filter(|l| !l.trim_start().starts_with('#'))
            .collect::<Vec<_>>()
            .join("\n")
    }
}
