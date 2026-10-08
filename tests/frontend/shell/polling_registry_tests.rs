use std::fs;
use std::path::{Path, PathBuf};

/// 周期唤醒的**登记表**：`(相对仓根的路径, 类别, 为什么 + 谁退役它)`。
///
/// 多一处没登记的 ⇒ 下面那条红。**登记表不是豁免清单**，是「这些我看过、
/// 而且知道它归谁」的账。
const REGISTERED: &[(&str, &str, &str)] = &[
    // 🔴 **这里原来的第一条出去了**：`src/frontend/ui/session-accounts-poll.ts` 那个
    //    10s `refreshSessionAccounts`（data-poll）。它自己的说法栏写着退役条件 ——「先有一种账号事件
    //    （新帧或文件事件）」—— 这一拍兑现的是两件事：两条查询搬上了已有长连接（`accounts-sessions` /
    //    `accounts-list`，不再每拍握一次手），而「会话 ↔ 账号」只在会话起停时变、**起停本来就有帧**
    //    （`session_added` / `session_removed` ⇒ 远端 `live` 格 / `ended` 格），
    //    再加一个握手完成事件 `remote-backend-ready`（后来改经通道订 `accounts-changed`，那一格 `seen`）。⇒ 刷新改由事件驱动（`createEventRefresher`，零定时器），
    //    `setInterval` 删了。留着这一行，下面那条反向检查（「登记了却已经没有周期唤醒」）会当场红。
    //    买不到的一格写在 `session-accounts-poll.ts` 头注：别处改了默认账号、而这台上没有会话起停时，
    //    账号清单要等下一次握手 / 起停 / 本 UI 操作才刷新。
    (
        "src/frontend/ui/views/agent-window.ts",
        "ui-clock",
        "每分钟重画一次 agent 窗口的标题区（已跑 · 最近 · 等了多久都只写到分钟）。**不取数** —— 只按已有的运行表重画；\
             运行表走会话流的帧。关窗即清。",
    ),
    (
        "src/frontend/ui/views/grid-monitor.ts",
        "ui-clock",
        "1s 重绘一次网格。**不取数** —— 只把已有状态（相对时间等）重画；\
             取数走事件（`events.ts` 的帧）。这一类是后端那条护栏头注说的「正当周期行为」。",
    ),
    // `tab-session-actions.ts` 那条 data-poll（`awaitExitFor`：等 claude 退出的 1s 轮询）退役：
    //   换号重启不再键入 `/exit` 等它自己退，直接 kill ⇒ 没有要等的事，行删。
    // `control/ccm/plan.rs` 那一行（容器路收尾「等信任框、替用户按 Enter」的六轮 `sleep 0.5`）退役：
    //   信任由用户在会话里自己答，那段等待整条删了 ⇒ 行删（那份文件照旧在扫描面里，见 `scan()`）。
    // ★★ **08-10（devbench F07）扩面后逮到的一族**：`src/shared/cc-bus/scripts/`。
    // 本表原来的人群是「`src/**/*.ts` + 写死的 `shared/ccm` 一个文件名」⇒ 这棵树整个在账外。
    // ⚠ 第四类 `one-shot` 是这次新加的：shell 那条针是宽的（`contains("sleep ")`），
    // 一次性 sleep 也会命中。**刻意不收窄针**（宁可宽松让人判断，也不要用严格的错误引入噪声）
    // —— 收窄会漏掉「新加一个 `sleep 1` 在循环里」那种真轮询。代价是一次性的也要登记一行，
    // 而那正是「默认拒绝」想要的：新加一处就得回答它是哪一类。
    (
        "src/shared/cc-bus/scripts/cc-busd",
        "data-poll",
        "★ **本表扩面当天逮到的唯一真轮询**：长驻 broker 进程，`while [ \"$running\" = 1 ]` \
             里每 0.5s 醒一次扫队列目录（`:101` `sleep \"$POLL\"`；`:99` 是「一整轮没进展」的退避）。\
             它**自己的注释就承认了**：`:20` 逐字「队列空时轮询间隔秒(**本实现恒轮询,未用 inotify**)」。\
             **事件源**：`$BUS/queue` 目录的 inotify —— 队列是文件系统目录、天然可 watch，\
             与后端侧看 pidfile 的做法同构。\
             **退役归**未排期（cc-bus 增强属 issue #77/#78 那一族，用户 08-10 明确「后面再增强」）。\
             如实记未排期，不编一个假 owner 让它看起来有人管。\
             ⚠ 同文件 `:115` 另有 `for _i in $(seq 1 10); do sleep 0.3; backend_running && break; done` \
             —— 那是 wait-for-condition（上限 ~3s，注释自陈「轮询确认最多 ~3s」），不是节拍器。",
    ),
    (
        "src/shared/cc-bus/scripts/cc-bus-lib.sh",
        "one-shot",
        "`:238` 的 `sleep 0.3` 夹在 `tmux send-keys <文本>` 与 `tmux send-keys Enter` 之间 —— \
             **键入节奏**，一次性。不是周期唤醒：它不在任何循环的每轮上，前后是一对 send-keys。",
    ),
    (
        "src/shared/cc-bus/scripts/cc-kill",
        "one-shot",
        "`:18` `kill $procs; sleep 0.3; kill -9 $procs` —— **优雅退出与强杀之间的宽限**，一次性。",
    ),
    (
        "src/shared/cc-bus/scripts/cc-spawn",
        "one-shot",
        "`:146` `sleep 1.5` —— **启动让路**（等被 spawn 的 agent 把自己登记上来）。一次性；\
             同文件 `:145` 注释逐字「显式保留而非默默删掉——原先 pretrusted 成功路径上就是 \
             sleep 1.5」，说明它是刻意留的既有行为。",
    ),
];

/// ★ **前提触发器**：本模块头注说「Rust 那半的家是 `rust_timer_registry`」——
/// 那句话只在**它真的还在**时成立。
///
/// # 为什么这条值得单独存在
///
/// 这段指针的**上一版**是「Rust 侧刻意不在范围内，如实登记为未做，不假装覆盖了」。
/// 那句话在写下时是真的，F09 把那半做掉之后它就烂了 —— 而**没有任何东西会因此变红**，
/// 直到 audit-0805 F14 §4 复核时才发现。
///
/// ★ **一句自称诚实的话烂掉，比一个错数字更贵**：错数字会被人核对，
/// 而「我如实记了未做」读起来像「这里有人想过了」，下一个人就不会去查。
/// ⇒ 指针必须有判据看着，跟散文数字一样（定框 E12）。
#[test]
fn the_other_half_of_the_sweep_still_has_a_home() {
    let root = repo_root();
    let other = root.join("src/frontend/shell/src/rust_timer_registry.rs");
    let body = fs::read_to_string(&other).unwrap_or_default();
    // ⚠ `contains_word` 不是 `contains`：变异实测把 `REGISTERED` 改名成 `REGISTERED_X`，
    // 裸 `contains` **照样绿**（前缀）。那正是 F24 那一族 —— 而它在这条**新写的**判据里
    // 又复发了一次，说明「知道有这个坑」不等于不踩。原语在手就别手写匹配。
    assert!(
        guard_core::contains_word(&body, "REGISTERED"),
        "`rust_timer_registry` 不见了（或不再是登记表）。\n\
             本模块头注逐字说着「Rust 那半的家是它」—— 那句话此刻是假的。\n\
             ★ 要么把那半的新家写进头注，要么把头注改回「未做」；\n\
             **不许留着一句指向空处的指针** —— 那比没有注释更坏（skill 铁律 14）。"
    );
    let me = fs::read_to_string(root.join("src/frontend/shell/src/polling_registry.rs"))
        .expect("读不到本文件");
    // ⚠ **只看头注那半**（`production_source` 把 `#[cfg(test)]` 段剥掉）。
    // 变异实测：拿整份文件 `contains` 时，**本条自己的代码里就写着这个名字**
    // （上面那个路径 join、下面那个 `mod` 断言）⇒ 散文里的指针被删光了它照样绿。
    // 那是 F23 那一族「判据匹配到自己」—— 在这条**新写的**判据里又复发了一次。
    let head_note = guard_core::production_source(&me);
    // 反向：头注真的指过去了才算。只留判据不改散文，读的人还是被那句旧话骗。
    assert!(
        guard_core::contains_word(&head_note, "rust_timer_registry"),
        "本模块头注里找不到 `rust_timer_registry` —— 指针被删了而本条还绿着，\n\
             说明本条钉的是「那半存在」而不是「这里指着它」。两件都要。"
    );
    // 那半必须真的被编进来（`mod` 声明），否则它是一份没人跑的死代码。
    let lib =
        fs::read_to_string(root.join("src/frontend/shell/src/lib.rs")).expect("读不到 lib.rs");
    // `find_pinned`：恰好一处 + 两侧有边界。裸 `contains` 会被
    // `mod rust_timer_registry_v2` 之类喂饱，而那时「那半有人管」已经不成立了。
    assert!(
        guard_core::find_pinned(&lib, "mod rust_timer_registry;").is_ok(),
        "`rust_timer_registry` 没有在 `lib.rs` 里声明 ⇒ 它根本不参与编译与测试，\n\
             「那半有人管」这句话就成了空头支票。"
    );
}

/// ★ **前提触发器**：上面两条「今天不能退役」的理由，前提是
/// **backend 只装那三条 hook**（`session-created` / `session-closed` / `session-renamed`）。
///
/// hook 覆盖面一变（多一条、少一条、换名字）⇒ 本条**主动红**，逼人回来重新裁定
/// 「哪些轮询现在可以退役了」。这是好事：多一条 hook 往往正好解锁一处轮询。
///
/// ⚠ 它挡不住「hook 装上了但 tmux 那个事件本身覆盖面变了」（tmux 版本差异）——
/// 那属于外部世界，本仓钉不了。**比没有强，别读成证明。**
#[test]
fn the_hook_coverage_that_these_reasons_rest_on_has_not_changed() {
    const BACKEND_HOOKS: &str = include_str!("../../../src/backend/control/tmux_hook.rs");
    let prod = guard_core::production_code(BACKEND_HOOKS);
    // 判据串运行时拼，免得命中本文件自己上面那两段说明。
    let want: Vec<String> = ["created", "closed", "renamed"]
        .iter()
        .map(|e| format!("session-{e}"))
        .collect();
    for w in &want {
        assert!(
            prod.contains(w.as_str()),
            "backend 的 hook 里找不到 `{w}` —— 覆盖面缩小了。\n\
                 上面 `src/frontend/ui/tabs.ts` / `src/frontend/ui/main.ts` 两条「今天不能退役」的理由建立在\
                 「只有这三条 hook」之上，覆盖面一变就要重新裁定。"
        );
    }
    // 反向：**不许多**。多一条就可能解锁一处轮询 ⇒ 主动红提醒。
    let found = prod.matches("session-").count()
        + prod.matches("window-").count()
        + prod.matches("pane-").count();
    assert!(
        found > 0,
        "一条 hook 名都没扫到 —— 剥法或路径坏了，本条会零命中地绿"
    );
    let hook_events = prod
        .split("HOOK_EVENTS")
        .nth(1)
        .unwrap_or("")
        .split("];")
        .next()
        .unwrap_or("");
    assert!(
        !hook_events.is_empty(),
        "抽不到 `HOOK_EVENTS` 数组 —— 抽取器坏了"
    );
    let n = hook_events.matches("session-").count()
        + hook_events.matches("window-").count()
        + hook_events.matches("pane-").count()
        + hook_events.matches("client-").count();
    assert_eq!(
        n, 3,
        "backend 装的 hook 从 3 条变成了 {n} 条 —— **这多半是好事**，\n\
             但它意味着上面两条「今天不能退役」的理由前提变了：\n\
             请回 F02 重新裁定哪些轮询可以改等帧了（多一条 hook 常常正好解锁一处）。\n\
             抽到的数组：{hook_events}"
    );
}

/// **明令不许有周期唤醒**的文件（把两处散文纪律变成机检）。
const NO_PERIODIC_WAKE: &[(&str, &str)] = &[(
    "src/frontend/ui/settings/cc-bus-section.ts",
    "头注写着「本文件里不得出现 setInterval / setTimeout 轮询 / 后台定时任务」",
)];

fn repo_root() -> PathBuf {
    // 住址唯一源：`crate::guard_support`（头注写着 24 份副本怎么一起漂的）。
    crate::guard_support::repo_root()
}

/// 剥掉整行注释（`//` / `*` / `/*`）。
///
/// ⚠ 必须剥：`cc-bus-section.ts` 的头注里**就写着**
/// `setInterval` 这个词（写的是「不许有」）。不剥的话它们会被自己的纪律说明命中 ——
/// 与 `launch-cli-wire.vitest.ts` 那次「文档注释里就写着 `deny_unknown_fields`」同一个坑。
/// 一行里有没有周期唤醒的形态。
/// `shared/` 下的 shell 脚本全集：`.sh` 后缀 **或** 首行有 shebang（`cc-busd`/`cc-send`
/// 这些没有后缀）。**按内容认，不按后缀认** —— 后缀是可选的，shebang 才是「它是脚本」的证据。
fn collect_shell(dir: &std::path::Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(rd) = fs::read_dir(dir) else {
        return out;
    };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            out.extend(collect_shell(&p));
            continue;
        }
        let is_sh = p.extension().and_then(|x| x.to_str()) == Some("sh");
        let has_shebang = fs::read_to_string(&p)
            .ok()
            .and_then(|t| t.lines().next().map(|l| l.starts_with("#!")))
            .unwrap_or(false);
        if is_sh || has_shebang {
            out.push(p);
        }
    }
    out
}

fn is_periodic(line: &str, is_shell: bool) -> bool {
    if is_shell {
        return line.contains("sleep ");
    }
    if line.contains("setInterval") {
        return true;
    }
    // `poll` 命名的递归 setTimeout —— 见模块头注的诚实边界。
    line.contains("setTimeout") && line.to_lowercase().contains("poll")
}

/// 扫描面：`src/**/*.ts`（排除测试）+ **`shared/` 下所有 shell 脚本**。
///
/// ⚠ **08-10（devbench F07）扩面**：原来这里是 `files.push(root.join("shared/ccm"))`
/// —— **一个写死的文件名**。头注当时写的范围「TS 与 `shared/ccm`」在写下时是对的，
/// 而 `src/shared/cc-bus/` 进仓之后就成了一个没人管的角落：`src/shared/cc-bus/scripts/cc-busd`
/// 是个**长驻 broker 进程**，每 0.5s 醒一次扫队列目录，它自己的注释逐字承认
/// 「队列空时轮询间隔秒(**本实现恒轮询,未用 inotify**)」—— 而本表看不见它。
///
/// ★ 这不是「另一个仓不该管」：同仓的 `shell_lint_registry`（扫描面含
/// `src/shared/cc-bus/scripts/*`）与 `session_name_registry`（点名 `cc-spawn`）**都**已经
/// 把那棵树算进人群了，**只有轮询这张表没跟上**。⇒ 人群改成**遍历**，
/// 加一个脚本自动进人群，不用谁记得回来 push 一行。
fn scan() -> Vec<(String, usize)> {
    let root = repo_root();
    let mut files: Vec<PathBuf> = Vec::new();
    collect_ts(&root.join("src"), &mut files);
    files.sort();
    let mut shells = collect_shell(&root.join("src/shared"));
    shells.sort();
    files.extend(shells);
    // 🔴 **容器路那段 shell 由 Rust 渲出来**（`shared/ccm` 删了，产出方换了）⇒ 收进人群，
    //    渲出去的 shell 里再长出 `sleep` 就当场要登记。
    //    ⚠ 它按 `is_shell` 那条针认（`contains("sleep ")`）—— 那正对：
    //    本表要认的是**那段 shell 里的 sleep**，不是 Rust 自己的节拍。
    files.push(root.join("src/backend/control/ccm/plan.rs"));
    let mut out = Vec::new();
    for f in files {
        let rel = f
            .strip_prefix(&root)
            .unwrap_or(&f)
            .to_string_lossy()
            .replace('\\', "/");
        let is_shell = !rel.ends_with(".ts");
        let src = guard_core::strip_comment_lines(&fs::read_to_string(&f).unwrap_or_default());
        let n = src.lines().filter(|l| is_periodic(l, is_shell)).count();
        if n > 0 {
            out.push((rel, n));
        }
    }
    out
}

fn collect_ts(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            collect_ts(&p, out);
            continue;
        }
        let name = p
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        if name.ends_with(".ts")
            && !name.contains(".vitest.")
            && !name.contains(".test.")
            && !name.ends_with(".d.ts")
        {
            out.push(p);
        }
    }
}

/// ★ 抽取器自检：扫不到东西时下面几条会零命中变绿。
#[test]
fn the_scan_actually_reads_the_frontend_and_ccm() {
    let root = repo_root();
    let mut ts = Vec::new();
    collect_ts(&root.join("src"), &mut ts);
    // 采集面与 git 跟踪着的前端 `.ts` 对拍（测试与声明文件不在人群里）。
    let seen: Vec<String> = ts.iter().map(|p| crate::guard_support::rel_of(p)).collect();
    let skip: Vec<String> = crate::guard_support::tracked_under("src", "ts")
        .into_iter()
        .filter(|r| r.contains(".vitest.") || r.contains(".test.") || r.ends_with(".d.ts"))
        .collect();
    let skip: Vec<&str> = skip.iter().map(String::as_str).collect();
    crate::guard_support::assert_scanned_every_tracked(
        "前端周期唤醒",
        &seen,
        &["src"],
        "ts",
        &skip,
    );
    // 剥注释不能把整份文件剥空。
    //
    // ★上一版是 `strip_comment_lines(&ccm).len() * 2 > ccm.len()`。
    //   那条线量的**不是剥法坏没坏**，是「ccm 里以 `//` / `*` / `/*` 开头的行占多少」——
    //   一个随写作漂移的量。现打（量于 `018b134`）：抹掉 545 字节 / 9 行，余量 90209 字节。
    //   它今天离翻红很远（不像下面那条 `* 4` 只剩 97 字节），但**形状同族** ⇒ 一起换掉，
    //   免得下一轮谁把 ccm 的形态改了，同一个病在这里再演一遍。
    // ⚠ 诚实边界：这个剥法是 Rust/TS 那套（`//` / `*` / `/*`），对 shell 的 `#` 一个字
    //   都不剥 ⇒ 这里**不许**断言「它剥掉了东西」（那不是它对 shell 语料的契约）。
    //   今天被抹掉的 9 行全是 shell 的 `*)` case 分支 —— 那是 `scan()` 在 ccm 上的
    //   真实行为，是另一件事，不是本条该钉的性质。
    let ccm = fs::read_to_string(root.join("src/backend/control/ccm/plan.rs")).unwrap_or_default();
    assert!(
        !guard_core::strip_comment_lines(&ccm).trim().is_empty(),
        "剥注释后 `control/ccm/plan.rs` 一个非空白字节都不剩（原文 {} 字节）。两个可能的真因：\n\
             ① 剥法太狠（`strip_comment_lines` 坏了）⇒ `scan()` 此刻在扫空字符串；\n\
             ② 那份文件整份都成了注释行 —— 真发生了就说明文件换了个东西，本条要重新裁定。",
        ccm.len()
    );
}

/// ★ 正题：**每一处周期唤醒都得在登记表里**。
#[test]
fn every_periodic_wake_is_registered_with_an_owner() {
    let found = scan();
    let registered: Vec<&str> = REGISTERED.iter().map(|(f, _, _)| *f).collect();
    let mut unregistered: Vec<String> = Vec::new();
    for (f, n) in &found {
        if !registered.contains(&f.as_str()) {
            unregistered.push(format!("  {f}（{n} 处）"));
        }
    }
    assert!(
        unregistered.is_empty(),
        "有周期唤醒没登记。**登记表不是豁免清单** —— 请写明它是哪一类\
             （ui-clock / data-poll / wait-for-condition），\
             `data-poll` 还要写明事件源在哪、谁退役它：\n{}",
        unregistered.join("\n")
    );
    // 反向：登记表里的文件必须**真的还有**周期唤醒（搬走/退役了就该删条目）。
    for (f, _, _) in REGISTERED {
        assert!(
            found.iter().any(|(g, _)| g == f),
            "登记表里的 `{f}` 已经没有周期唤醒了 —— 退役了就把这条删掉（别留成僵尸账）"
        );
    }
}

/// ★ `data-poll` 这一类**必须**写明事件源与退役去处 —— 那是它与「正当周期行为」的分界。
#[test]
fn every_data_poll_names_its_event_source_and_owner() {
    let mut polls = 0usize;
    for (f, kind, why) in REGISTERED {
        assert!(
            matches!(
                *kind,
                "ui-clock" | "data-poll" | "wait-for-condition" | "one-shot"
            ),
            "{f} 的类别 {kind:?} 不在四类里"
        );
        if *kind == "data-poll" || why.contains("data-poll") {
            polls += 1;
            assert!(
                why.contains("事件源"),
                "{f} 记成 data-poll 却没说事件源在哪"
            );
            assert!(why.contains("退役归"), "{f} 记成 data-poll 却没说谁退役它");
        }
    }
    // 地板 `>= 3` 换成相等：今天 data-poll 恰好 **1** 条（`cc-busd`；`tab-session-actions.ts` 的
    //   `awaitExitFor` 那条随换号重启直接 kill 退役，2 → 1）。`C1` 那一拍少的是 `session-accounts-poll.ts` 的 10s 账号轮询（改事件驱动，见 `REGISTERED`）。
    //   地板在「少了一条」这个方向上判不出是退役还是抽取坏了 —— 相等判得出，而且逼人写清是哪一条。
    assert_eq!(polls, 1, "data-poll 条数变了（今天 1：cc-busd）—— 多了请登记事件源与退役去处，少了请写清退役的是哪条");
}

/// **调度调用点的分类住在调用点旁边**：每个 `setInterval` / `setTimeout` / `requestAnimationFrame` /
/// `requestIdleCallback` 调用上方（或同一行）一行 `// 调度：<类> —— <理由>`，类取闭集 [`SCHEDULING_KINDS`]。
///
/// 为什么要逐个调用点分类，而不是只认周期形态：`is_periodic` 认的是「一行里像不像周期唤醒」，
/// 自链（递归 `setTimeout` · rAF 补料链 · rIC 物化队列）在语法上与一次性延时难分 ⇒ 改成
/// 「**每一个调度调用点都必须被分类**」，新写一处没标的当场红。真轮询（取数的周期唤醒）还要进 `REGISTERED`。
const SCHEDULING_KINDS: &[&str] = &[
    "一次性", // 一次性延时：UI 反馈 · 防抖 · 悬停延迟 · 期限
    "自链",   // 有退出条件的自链（队列空 / 守卫不满足即停）
    "合批",   // 帧末 / 短窗合批：排一次位，回调里不再排
    "钟",     // ui-clock：只重画、不取数（文件要在 `REGISTERED` 里记成 `ui-clock`）
];

/// 标记的写法（行注释开头）。
const SCHEDULING_MARK: &str = "// 调度：";

/// **还没就地标记的**那几份（别的路正在改它们，合了再补标记、删行）：`(路径, API, 处数, 这几处是什么)`。
/// 处数变了就是该重新分类的时刻；那份文件标上了 ⇒ 删那一行。
const SCHEDULING_SITES: &[(&str, &str, usize, &str)] = &[
    ("src/frontend/ui/tabs.ts", "requestAnimationFrame", 2, "原 ① `fillAbove` 批末复检搬去了 `tab-stream-view.ts`（上面那条），编号沿用原号。② 切 Tab 后把面板整表 re-render 推到下一帧，入口处 `this.activeId !== sessionId` 早返。原 ③（`scheduleTabBarRefresh` 帧末合批）随 tab 栏视图搬去了 `tab-bar-view.ts`。④ ★ 步 3（2026-09-18）：`switchTo` 贴底的**第二帧**（对齐 `session-viewer.ts` 已有的同一修法）。⚠ 它**不是只读校正** —— `scrollToBottom()` 会把 `stickToBottom` 重新置真，所以第二帧与第一帧一样是强制贴底。可接受的理由只有一条：两帧之间只隔 ~16ms，人滚不出意图；切走了有 `activeId` 守卫挡着。**不是自链**（回调里不再排下一次）。"),
    ("src/frontend/ui/tab-session-actions.ts", "setTimeout", 3, "⑧ `shellFront`：↗ 壳那一跳的期限（到点落成「无应答」，本机 / 远端两条共用这一处）。一次性，不是周期取数。编号沿用 `tabs.ts` 那一行拆开之前的原号。⑩ ⑪ `frontOnce`：↗ 在飞超过 300ms 才把按钮换成「进行中」· 进了之后至少停 400ms 再收（防闪），一次性。"),
    ("src/frontend/ui/views/grid-monitor.ts", "setInterval", 1, "1s 重绘 —— 按格差量（没变的一拍零 DOM 写），不再整表重建。**ui-clock，不取数**，见 `REGISTERED` 那条。"),
];

/// 数一个调度 API 在源码里的**调用**次数（散文里提到名字不算）。
///
/// # 原来是 `matches("{api}(")`，而它旁边的注释写着「允许 `api  (`」
///
/// **代码不允许，注释说允许** —— 两者对不上，而对不上的那一边正是漏洞：
/// 把 `requestAnimationFrame (tick)`（**自链**，正是 E6 禁的连续唤醒）写进
/// 一个已退役的视图文件（`views/usage-view.ts`），本条与 `every_periodic_wake_is_registered_with_an_owner`
/// **两条都不响**（后者的 `is_periodic` 根本不看 rAF，只看 `setInterval` 与带 `poll` 的
/// `setTimeout`）⇒ 一个空格就能把「全部调度调用点」这条枚举式白名单的人群缩小。
///
/// ⚠ 对照：同一轮里 `setInterval (…)` **被抓住了**，但那是隔壁那条判据的裸 `contains`
/// 顺手接住的，不是本条的功劳 —— **纵深防御会掩盖单条判据的洞**，
/// 所以变异要看「是谁红的」，不能只看有没有红。
///
/// 现在的口径：名字必须是**完整的一个词**（`myRequestAnimationFrame` 不算，
/// `window.setInterval` 算），其后允许任意空白，然后必须是 `(`。
fn count_calls(src: &str, api: &str) -> usize {
    let mut n = 0usize;
    let mut from = 0usize;
    while let Some(rel) = src[from..].find(api) {
        let i = from + rel;
        from = i + api.len();
        let starts_word = !src[..i]
            .chars()
            .next_back()
            .is_some_and(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$');
        if starts_word && src[from..].trim_start().starts_with('(') {
            n += 1;
        }
    }
    n
}

/// 一份 TS 源码里，每一处调度调用（剥注释后按行认）是哪一行 · 哪个 API · 标的是哪一类（没标 ⇒ `None`）。
/// 标记认法：调用那一行自己带着，或它**紧上方那一串行注释**里有一行是 [`SCHEDULING_MARK`]。
fn scheduling_marks(raw: &str) -> Vec<(usize, &'static str, Option<String>)> {
    const APIS: [&str; 4] = [
        "setInterval",
        "setTimeout",
        "requestAnimationFrame",
        "requestIdleCallback",
    ];
    let code: Vec<String> = guard_core::strip_comment_lines(raw)
        .lines()
        .map(str::to_string)
        .collect();
    let raw_lines: Vec<&str> = raw.lines().collect();
    let kind_in = |l: &str| -> Option<String> {
        let at = l.find(SCHEDULING_MARK)?;
        let rest = &l[at + SCHEDULING_MARK.len()..];
        Some(rest.split_whitespace().next().unwrap_or("").to_string())
    };
    let mut out = Vec::new();
    for (i, line) in code.iter().enumerate() {
        for api in APIS {
            for _ in 0..count_calls(line, api) {
                let mut mark = raw_lines.get(i).and_then(|l| kind_in(l));
                // 往上走那一串注释行：剥注释（共享原语）之后空了、原文不空的就是注释行。
                let mut j = i;
                while mark.is_none() && j > 0 {
                    j -= 1;
                    let up = raw_lines.get(j).copied().unwrap_or("");
                    if !(code[j].trim().is_empty() && !up.trim().is_empty()) {
                        break;
                    }
                    mark = kind_in(up);
                }
                out.push((i + 1, api, mark));
            }
        }
    }
    out
}

/// ★ **每一个调度调用点都带着分类标记，类在闭集里**（还没标的那几份见 [`SCHEDULING_SITES`]，处数对得上）。
///
/// 与 `every_periodic_wake_is_registered_with_an_owner` 的分工：那一条钉「**认出来的**周期唤醒有没有主人」，
/// 本条钉「**有没有认漏**」。
#[test]
fn every_scheduling_call_site_carries_a_classification_mark() {
    // 匹配单位自检（两个方向都要钉）：
    // 放松的那一侧 —— 带空白的调用要数进来（本轮的洞就在这里）。
    assert_eq!(
        count_calls("requestAnimationFrame (tick);", "requestAnimationFrame"),
        1,
        "带空格的调用没被数进来 —— 一个空格就能把这条判据的人群缩小"
    );
    assert_eq!(count_calls("setInterval\n  (f, 9);", "setInterval"), 1);
    // 收紧的那一侧 —— 别把不是调用的东西也数进来，否则新口径会误红好代码。
    // ⚠ 这条第一版写的是 `mySetInterval(` —— 里面是大写 `S`，**根本不含 `setInterval`**，
    // 于是把词边界整段删掉它照样绿（变异实测）。**负向断言最容易写成恒绿的**：
    // 它要求你先造出「真的会被误命中」的输入，而那一步很容易糊弄过去。
    assert_eq!(
        count_calls("my_setInterval(f, 9);", "setInterval"),
        0,
        "`my_setInterval` 被当成了 `setInterval` —— 词边界没守住"
    );
    assert_eq!(
        count_calls("const h = setInterval; use(h);", "setInterval"),
        0,
        "只是提到名字、没有调用，不该计数"
    );
    assert_eq!(
        count_calls("window.setInterval(f, 9);", "setInterval"),
        1,
        "`window.setInterval(` 是调用，必须数"
    );
    // 标记认法的正反控（合成源码）：没标 · 标了闭集外的类 · 标在上方注释串里 · 同一行。
    let probe = "a();\nsetTimeout(f, 1);\n// 调度：随便 —— x\nsetTimeout(f, 1);\n// 说明\n// 调度：一次性 —— y\nrequestAnimationFrame(g);\nsetInterval(h, 9); // 调度：钟 —— z\n";
    assert_eq!(
        scheduling_marks(probe),
        vec![
            (2, "setTimeout", None),
            (4, "setTimeout", Some("随便".to_string())),
            (7, "requestAnimationFrame", Some("一次性".to_string())),
            (8, "setInterval", Some("钟".to_string())),
        ],
        "标记认法认错了"
    );
    let root = repo_root();
    let mut files: Vec<PathBuf> = Vec::new();
    collect_ts(&root.join("src"), &mut files);
    files.sort();
    let pending: std::collections::BTreeSet<&str> =
        SCHEDULING_SITES.iter().map(|(f, _, _, _)| *f).collect();
    let (mut sites, mut bad, mut clocks) = (0usize, Vec::new(), std::collections::BTreeSet::new());
    let mut pending_found: Vec<(String, String, usize)> = Vec::new();
    for f in files {
        let rel = f
            .strip_prefix(&root)
            .unwrap_or(&f)
            .to_string_lossy()
            .replace('\\', "/");
        let raw = fs::read_to_string(&f).unwrap_or_default();
        let marks = scheduling_marks(&raw);
        if pending.contains(rel.as_str()) {
            let mut by: std::collections::BTreeMap<&str, usize> = Default::default();
            for (_, api, _) in &marks {
                *by.entry(api).or_insert(0) += 1;
            }
            for (api, n) in by {
                pending_found.push((rel.clone(), api.to_string(), n));
            }
            assert!(
                marks.iter().any(|(_, _, m)| m.is_none()),
                "`{rel}` 的调度点都标上了 —— 把它从 `SCHEDULING_SITES` 里删掉"
            );
            continue;
        }
        for (ln, api, mark) in marks {
            sites += 1;
            match mark.as_deref() {
                Some(k) if SCHEDULING_KINDS.contains(&k) => {
                    if k == "钟" {
                        clocks.insert(rel.clone());
                    }
                }
                Some(k) => bad.push(format!(
                    "  {rel}:{ln}  [{api}]  标的「{k}」不在闭集 {SCHEDULING_KINDS:?} 里"
                )),
                None => bad.push(format!("  {rel}:{ln}  [{api}]  没有标记")),
            }
        }
    }
    // 抽取器坏了扫不到东西 ⇒ 下面那条会零命中地绿。
    assert!(
        sites > 0,
        "`src/` 下一个调度调用点都没扫到 —— 遍历或认法坏了"
    );
    assert!(
        bad.is_empty(),
        "这几个调度调用点没分类（或类不在闭集里）。在调用上方加一行 `{SCHEDULING_MARK}<类> —— <理由>`：\n{}\n\
         （一次性 UI 反馈 / 有退出条件的自链 / 合批 / 只重画的钟；取数的周期唤醒还要进 `REGISTERED`）",
        bad.join("\n")
    );
    // 「钟」那几份都得在 `REGISTERED` 里记成 ui-clock（只重画、不取数，由那张表写清）。
    for f in &clocks {
        assert!(
            REGISTERED
                .iter()
                .any(|(g, k, _)| g == f && *k == "ui-clock"),
            "`{f}` 标了「钟」却不在 `REGISTERED` 里记成 ui-clock"
        );
    }
    // 还没标的那几份：处数两向对得上（多了 / 少了 ⇒ 重新分类；表里那一格盘上没有了 ⇒ 删行）。
    let mut want: Vec<(String, String, usize)> = SCHEDULING_SITES
        .iter()
        .map(|(f, a, n, _)| (f.to_string(), a.to_string(), *n))
        .collect();
    want.sort();
    pending_found.sort();
    assert_eq!(
        pending_found, want,
        "还没标记的那几份，调度点处数与 `SCHEDULING_SITES` 对不上 —— 那正是该重新分类的时刻；\
         趁这一拍就地标上、把那一行删掉最好"
    );
}

/// ★ **身份 poller 不许回来** —— `U-NP④`（2026-08-14）之后 `shared/ccm` 里
/// **一条与会话同寿的循环都不许有**。
///
/// # 这条判据取代了谁
///
/// 它的**上一版**叫 `the_per_second_identity_poller_spawns_nothing_per_tick`，
/// 钉的是「那条每秒循环每次醒来不许起外部进程」（audit-0805 F14 第六刀：管道形态
/// 每 tick 7 次 clone/execve，纯 builtin 0 次）。那一版的头注**自己写着**：
/// 「本条钉的是**每次醒来的代价**，不是醒不醒；『别每秒醒』要 inotify，
/// 得动 ccm 的进程模型 —— 如实登记为未做」。
///
/// 用户原话「**可以动ccm. 不要轮询**」＋「**ccm做到必须走backend**」
/// ⇒ 那件「未做」被做掉了，做法不是给 ccm 上 inotify（破「纯 POSIX shell、零第三方」，
/// 而 ccm 要经 `include_str!` 部署到任意远端），而是**把通道 B 整条搬去 backend**
///（`src/backend/control/identity_tag.rs`，由它已有的 pidfile inotify 驱动）。
/// ⇒ 「醒来的代价」这个量**不再存在**，钉它的判据必须换成钉「它真的没了」。
///
/// # 为什么不是零命中的空守卫
///
/// 它有一个真实的反向锚点：同两份文件里**仍然有**渲给 shell 的串（容器路键入载荷那句
/// `send-keys -t`）。该钉的是**那一种形态**：与会话同寿的循环。
/// 下面第二段断言正是靠它证明抽取器没有空转。
#[test]
fn the_identity_poller_is_gone_for_good() {
    // 🔴 **语料换了：`shared/ccm` → `control/ccm/{mod,plan}.rs`。**
    //    〔用@09-11 `K33`〕那个 bash 脚本删了，而它渲出来的那段 shell（容器路的 send-keys
    //    载荷与收尾）今天由这两份 Rust 产出 ⇒ 「与会话同寿的循环不许回来」
    //    这件事要盯的是**产出方**。剥注释也跟着换成 Rust 那套（`strip_comment_lines`）。
    let root = repo_root();
    let raw: String = ["mod.rs", "plan.rs"]
        .iter()
        .map(|f| {
            fs::read_to_string(root.join("src/backend/control/ccm").join(f))
                .unwrap_or_else(|e| panic!("control/ccm/{f} 读不到 —— 路径变了就把这条一起改：{e}"))
        })
        .collect::<Vec<_>>()
        .join("\n");
    // ⚠ 剥法用 `production_code`（Rust 那套：`//` 行 + 文档注释），**不是**
    //    `strip_comment_lines` —— 后者把注释行换成空行、行数不变，
    //    于是「剥掉了没有」这条自检按行数比会恒假（第一版就是这么红的，自己逮到）。
    let prod = guard_core::production_code(&raw);
    // ★ 反空真自检 —— 两个失效方向各一条，**都不随注释占比漂移**（口径沿用上一版）。
    assert!(
        prod.len() < raw.len(),
        "剥法一个字节都没剥掉（进 {} 字节、出 {} 字节）。两个可能的真因，**别只查第一个**：\n\
             ① `production_code` 没生效 ⇒ 下面几条会被那两份文件的散文喂饱；\n\
             ② `control/ccm/` 里一行注释都没有了 ⇒ 剥这一步已经没有意义，\n\
                该连着下面几条一起重新裁定。",
        raw.len(),
        prod.len()
    );
    assert!(
        !prod.trim().is_empty(),
        "剥完 `control/ccm/` 一个非空白字节都不剩（原文 {} 字节）。两个可能的真因：\n\
             ① 剥法把整份文件吃掉了 ⇒ 下面几条此刻在扫空字符串，会零命中地绿；\n\
             ② 那两份文件整份都成了注释与空行 ⇒ 已经不是原来那个东西，本组判据要重新裁定。",
        raw.len()
    );
    // ★ 与会话同寿的循环，唯一写得出的形态就是「盯着一个 PID 活不活」。
    //   ⚠ 它今天要在**渲出去的 shell 串**里找 —— 产出方换了语言，那条循环的形态没变。
    for shape in ["while kill -0", "until kill -0", "while ! kill -0"] {
        assert!(
            !prod.contains(shape),
            "`control/ccm/` 渲出去的 shell 里又出现了 `{shape}` —— 那是一条**与会话同寿**的循环。\n\
                 `U-NP④` 把身份通道 B 整条搬去了后端（`control/identity_tag.rs`），\n\
                 用户原话：「不要轮询」「ccm 做到必须走后端」。\n\
                 ⚠ 别把它当成「加个 sleep 兜一下更稳」——那正是本件要根除的东西：\n\
                 每会话一条、跑在**远端**机器上、与会话同寿。\n\
                 真需要一个新的等待，先回答「它的内核事件源是什么、为什么后端接不了」。"
        );
    }
    // 反向锚点 ①：那条 poller 用的解析器也一起没了（留着就是死代码）。
    assert!(
        !prod.contains("_ccm_sid_from_file"),
        "`_ccm_sid_from_file` 还在 —— 它只服务那条已删的 poller，留着就是死代码"
    );
    // 反向锚点 ②：**抽取器没有空转** —— 渲给 shell 的那句键入载荷必须还看得见。
    assert!(
        prod.contains("tmux send-keys -t {t}"),
        "连容器路键入载荷那句 `tmux send-keys` 都扫不到 —— 剥法或路径坏了，上面那几条是零命中地绿"
    );
}

// 🔴 **这里原来有 `ccm_fails_loudly_when_no_backend_can_be_found` 〔散文墓碑〕，
//   随 `shared/ccm` 一起删了 —— 而且它是「被测对象消失」，不是「判据放宽」。**
//
//   它钉的是 `U-NP④`（08-14）那条：**在 tmux 里找不到 backend ⇒ 响亮失败（rc=2）**，
//   形状面四格（身份分支真去查 backend · 找不到就 `die` · 逃生口会说话 · 前置检查排在
//   任何 `tmux` 调用之前）。〔用@09-11 `K33`〕「后端只有一个」之后，
//   **「找不到后端」这个概念不存在了**：敲的那个命令就是后端。
//   `tests/evidence/K-R48-356-verdicts.tsv` 第 67–81 行那 15 条 e2e 判的是同一件事，判词同为 `N`。
//
// ⚠ **如实边界，别读成「这条风险没了」**：`K-R48` 第一拍逐字登记着一格**没裁**的 ——
//   「一次性模式在 tmux 内由谁去打 `@ccm_sid`」。今天一次性模式**一个字都不说**
//   （那 356 条里唯一两条判 `K` 的就是它，见 verdicts 第 77–78 行）。
//   ⇒ 「没人打身份」这件事今天**没有任何判据盯着**，归 `K-R48` 下一拍。

/// ★ 把两处**散文纪律**变成机检：这两个文件里一处周期唤醒都不许有。
#[test]
fn the_files_that_forbid_polling_really_have_none() {
    let root = repo_root();
    for (f, why) in NO_PERIODIC_WAKE {
        let raw = fs::read_to_string(root.join(f))
            .unwrap_or_else(|e| panic!("{f} 读不到：{e} —— 文件搬了就把这条一起改"));
        assert!(raw.len() > 500, "{f} 只有 {} 字节，像是抽错了", raw.len());
        let prod = guard_core::strip_comment_lines(&raw);
        let hits: Vec<&str> = prod
            .lines()
            .filter(|l| is_periodic(l, false))
            .map(|l| l.trim())
            .collect();
        assert!(
            hits.is_empty(),
            "`{f}` 的纪律是「不许有周期唤醒」（{why}），却出现了：{hits:?}"
        );
    }
}
