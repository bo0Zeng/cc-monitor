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
    // 地板 170：08-05 实测 **190** 个
    // （`generated/` 73 · `src/` 本层 58 · `settings/` 23 · `views/` 14 · 其余 22）。
    // ⚠ 原文写「实测应约 90+」、地板 `>= 60` —— 数字腐了一倍多而判据照样绿
    // （地板式判据在「数字变大」这个方向上不会红，定框 E12 第二个陷阱的又一例）。
    //
    // **余量 20 意味着什么**：它**装不下 `settings/`（23）** ⇒ 少掉 `settings/` 或任何
    // 更大的子目录（`generated/` 73 · 本层 58）都会被这条抓住。
    // ⚠ **`views/`（14）单独消失这条抓不住** —— 那一层由 `SCHEDULING_SITES` 的反向检查
    // 兜着：分类账里有 5 个 `views/` 下的文件，它们一起消失那条会红。
    // 写清这一点是因为**「地板护住了整个扫描面」是一句很容易顺手写下的假话**。
    assert!(
        ts.len() >= 170,
        "只扫到 {} 个前端 .ts（08-05 实测 190）—— 遍历器坏了",
        ts.len()
    );
    assert!(
        fs::read_to_string(root.join("src/backend/control/ccm/plan.rs"))
            .map(|s| s.len())
            .unwrap_or(0)
            > 10_000,
        "`control/ccm/plan.rs` 读不到或太短 —— 路径变了？"
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

/// **全部调度调用点的分类账**：`(相对仓根的路径, API, 处数, 这几处是什么)`。
///
/// # 为什么要有这张表，而不是继续只认周期形态
///
/// `is_periodic` 认的是「一行里像不像周期唤醒」（`setInterval` / 名字带 `poll` 的
/// `setTimeout` / `sleep `）。它的漏网在本模块头注里**早就自陈过**，本轮实测了后果：
/// `views/history.ts` 那条「索引构建中 → 1 秒后自动重试」的递归 `setTimeout` **零命中**，
/// `tabs.ts` 的 rIC 物化队列、`session-viewer.ts` 的 rAF 补料链等**五处自链全部逃逸**。
/// ⇒ 「登记齐了」这句话建立在一个看不见它们的扫描面上。
///
/// 修法不是把形态认得更聪明（自链在语法上确实与一次性延时难分），而是**换失守方向**：
/// 从「认出来的才要登记」改成「**每一个调度调用点都必须被分类**」。
/// 新增一处没登记的调度点 ⇒ 本条红。**逃逸变成失败即红。**
///
/// ⚠ **不含行号，只含处数** —— 行号会腐：`00-核实台账` 记的 `views/history.ts:752`
/// 在 F14 第一刀改过那个文件之后已经是 `:761`。处数变了才是该重新分类的时刻。
const SCHEDULING_SITES: &[(&str, &str, usize, &str)] = &[
    ("src/frontend/ui/branch-fold.ts", "requestAnimationFrame", 1, "★ F15：live 模式主线重算的**帧末合批**（`scheduleLiveRecompute`）。排一次位（`liveScheduled`）⇒ **不是自链**：回调里不再排下一次，只有新记录到达才会再排。原来这里是逐条同步跑 `computeMainBranch`（扫全部 records 的 Kahn 拓扑）⇒ N 条记录 O(N²)。"),
    ("src/frontend/ui/branch-fold.ts", "setTimeout", 1, "★ F15：上面那条的**无 rAF 兜底**（`typeof requestAnimationFrame !== \"function\"` 时）。0ms，一次性。"),
    ("src/frontend/ui/front-pop.ts", "setTimeout", 1, "↗ 切过去了：1 秒后把对勾换回 ↗。一次性 UI 反馈。"),
    // P2s（补审 A4）：**有退出条件的自链**，不是 data-poll。
    ("src/frontend/ui/settings/backend-section.ts", "setTimeout", 1,
     "起/停一台机之后轮询状态到落定。**上限 30 次 × 100ms**、由用户动作触发、\
          落定即停 ⇒ 有退出条件的自链，不进 `REGISTERED`。\
          ⚠ 不轮询的后果很具体：两个命令都是「发出去就返回」（`backend_start` 只 spawn 了监护线程、\
          `backend_stop` 只发 SIGKILL），命令一返回就画等于**每次操作后都显示操作前的状态**。"),
    ("src/frontend/ui/e2e-probe.ts", "requestAnimationFrame", 2, "★ **rAF 自链**：`sample` 每帧重排自己（起点 1 处 + 链内 1 处）。退出条件是 `stopReplayJitterProbe` 显式 `cancelAnimationFrame`。只在 e2e 探针里启用，不在正常路径上。"),
    // 通用组件（`kit/`）：都是一次性 UI 延时，不取数、不自链。
    ("src/frontend/ui/find-strip.ts", "setTimeout", 1, "停 300ms 自己找（防抖）：每次输入清掉上一个再排；找过 / 收起时清掉。一次性。"),
    ("src/frontend/ui/kit/toast.ts", "setTimeout", 1, "到点收起这一条（纯告知 4s · 带动作 8s；悬停 / 焦点时清掉、离开后按剩下的时间重排）。一次性。"),
    ("src/frontend/ui/kit/tooltip.ts", "setTimeout", 2, "悬停 500ms 才出提示 · 卡式离开宿主与卡 120ms 才收；离开 / 移进卡即 `clearTimeout`。两处都一次性。"),
    ("src/frontend/ui/kit/block.ts", "setTimeout", 2, "① 加载超过 300ms 才画骨架 ② 超过 10s 才写正在做什么；换态时都 `clearTimeout`。一次性。"),
    ("src/frontend/ui/kit/interrupts.ts", "setTimeout", 1, "问后端「会打断什么」的 2s 上限：到点当有东西在跑；答到了 `clearTimeout`。一次性。"),
    ("src/frontend/ui/kit/menu.ts", "setTimeout", 3, "① ② 子菜单悬停 150ms 开 / 250ms 关（关菜单时统一清）③ 右键开的菜单下一拍挂「点外面」监听。一次性。"),
    ("src/frontend/ui/launch-slot.ts", "setTimeout", 1, "起新会话之后的占位标签页：每个一个 20 s 的点，到点只把样子换成「未报到」、问一次那台那个 tmux 会话在不在与画面（不重试、不轮询）；报到了 / 关掉时 `clearTimeout`。一次性。"),
    ("src/frontend/ui/launch-arrival.ts", "setTimeout", 2, "① 起会话之后等那台报出它的**预算**（`ARRIVAL_BUDGET_MS`）：每件预期一个、到点只说一次「没看到会话起来」，见到了当场 `clearTimeout`。② `awaitArrival` 发起方自己的上界（预算 ＋ 15 s：主窗口不回话也不挂着），回话一到就 `clearTimeout`。都是一次性，不重试、不取数。"),
    ("src/frontend/ui/events.ts", "setTimeout", 3, "① `scheduleBatchEnd` 的 batch-end 哨兵（每次重排前 `clearTimeout`，且有 `BATCH_HOLD_MAX_MS` 5min 防呆上限）② `setTimeout(drain, 0)` —— **队列 drain 自链**，退出条件是 `queue.length === 0`，由 `scheduled` 标志防重入。不是节拍器：没有队列就不会再排。原 ③（`makeYieldToMain` 的兜底）搬进 `yield-to-main.ts`（3 = 2 ＋ 1）。④（2 → 3）一台机器的会话流看不见了之后等 `UNSEEN_SAY_MS`（20 s）：还没看见才说一句是哪台、能做什么；又看见了当场 `clearTimeout`。每台每次看不见至多一个，一次性，不重试、不取数。"),
    // `src/frontend/ui/session-accounts-poll.ts` 的 `setInterval` ×1 这一行出去了（10s 账号轮询改事件驱动，理由见 `REGISTERED` 头上那段）。
    // 〔三入口拆分〕原先 `main.ts` 一行 3 处；代码块「复制」那段全局代理
    //   （② ③ 两处）搬进了主窗与 viewer 窗共用的 `entry-render-common.ts`（viewer 窗不再加载
    //   `main.ts`，而它也要这段代理；设置窗没有代码块，不加载它）。
    //   **一处都没多、一处都没少，只是换了文件**：3 = 1 ＋ 2。
    ("src/frontend/ui/entry-render-common.ts", "setTimeout", 2, "① ② 1.2s 后把「已复制」/「失败」还原成「复制」。一次性 UI 反馈。"),
    ("src/frontend/ui/settings/accounts-section.ts", "setTimeout", 1, "账号页收到后端推来的 `accounts-changed` / `quota-changed`：300ms 内的几帧合成一次重读（每来一帧重排一次，不自链、不取数）。一次性合批，不是 data-poll。"),
    ("src/frontend/ui/settings/panel.ts", "setTimeout", 1, "带目的地打开：1.5s 后撤掉那一节的高亮。一次性 UI 反馈。"),
    // `cc_integration.ts` 并进 `machine-aliases.ts`（终端集成成了 PowerShell 那一侧的别名块）⇒ 那一处跟着换文件：一处没多一处没少。
    // 那一处（「重新扫描」后 500ms 撤掉状态徽章的高亮描边）**删了**：别名块的现状今天随读回口的候选一起到，
    //   「重新读一遍」重读的是整份候选，不再闪一下徽章 ⇒ `machine-aliases.ts` 这一行整行走（少一处，不是换文件）。
    // 〔拆 `tabs.ts` 子步 9〕实时流视图搬进 `tab-stream-view.ts` ⇒ 原 `tabs.ts` 的 rAF ① · rIC ×1 · setTimeout ① 三处跟着走（下三行）：
    //   rAF 4 = 3 ＋ 1 · rIC 1 = 0 ＋ 1（`tabs.ts` 那一行因此整行删掉）· setTimeout 3 = 2 ＋ 1。一处没多一处没少。
    ("src/frontend/ui/tab-stream-view.ts", "requestAnimationFrame", 1, "① `fillAbove` 批末复检（间接自链，有队列型守卫）：补完一批下一帧再看一眼，仍在触发区 / 仍不可滚且账本有余就再补；切走了（`activeId` 守卫）或账尽即停。"),
    ("src/frontend/ui/tab-stream-view.ts", "requestIdleCallback", 1, "★ **空闲物化队列的自链**：`run` 处理一个后台 tab 后再排自己。退出条件是队列空。"),
    ("src/frontend/ui/tab-stream-view.ts", "setTimeout", 1, "① `setTimeout(run, 200)` —— 上面那条 rIC 队列在 `requestIdleCallback` 缺失时的兜底，同一条自链。"),
    ("src/frontend/ui/tabs.ts", "requestAnimationFrame", 2, "原 ① `fillAbove` 批末复检搬去了 `tab-stream-view.ts`（上面那条），编号沿用原号。② 切 Tab 后把面板整表 re-render 推到下一帧，入口处 `this.activeId !== sessionId` 早返。原 ③（`scheduleTabBarRefresh` 帧末合批）随 tab 栏视图搬去了 `tab-bar-view.ts`。④ ★ 步 3（2026-09-18）：`switchTo` 贴底的**第二帧**（对齐 `session-viewer.ts` 已有的同一修法）。⚠ 它**不是只读校正** —— `scrollToBottom()` 会把 `stickToBottom` 重新置真，所以第二帧与第一帧一样是强制贴底。可接受的理由只有一条：两帧之间只隔 ~16ms，人滚不出意图；切走了有 `activeId` 守卫挡着。**不是自链**（回调里不再排下一次）。"),
    // 〔拆 `tabs.ts` 子步 11〕拖拽状态机搬进 `tab-bar-drag.ts` ⇒ 原 ⑪ 停留计时器跟着走（下一行）：`tabs.ts` setTimeout 2 = 1 ＋ 1。
    ("src/frontend/ui/tab-bar-drag.ts", "setTimeout", 1, "⑪ ★ `updateDwell` 的**停留计时器**（`DWELL_MS` = 250ms）：拖动时压住某个 tab 满 250ms ⇒ 落点从 `before` 切成 `onto`（与它成组）。**一次性、非取数**：每次换目标 / 抖动超 4px 都先 `clearTimeout` 再重排，`teardownDrag` 收尾时无条件清（判据 `tests/frontend/ui/tabs.vitest.ts` 「步 17·D ⑤」那组用 `vi.getTimerCount()` 数在飞的定时器，死值验刀 21 钉着）。⚠ 它**非有不可**：指针停住之后 `mousemove` 就不再来了，靠事件驱动的话「停留」永远攒不满。"),
    // 〔拆 `tabs.ts` 子步 12〕tab 栏视图搬进 `tab-bar-view.ts` ⇒ 原 rAF ③ 与 setTimeout ⑩（同一个 `scheduleTabBarRefresh` 的两支）跟着走（下两行）：
    //   `tabs.ts` rAF 3 = 2 ＋ 1 · setTimeout 1 = 0 ＋ 1（`tabs.ts` 的 setTimeout 那一行因此整行删掉）。
    ("src/frontend/ui/tab-bar-view.ts", "requestAnimationFrame", 1, "③ ★ F15：`scheduleRefresh`（原 `TabManager.scheduleTabBarRefresh`） —— live 路上后台 tab 的 unread 徽标**帧末合批**（原来每来一行整刷一次 bar）。排一次位，不是自链。⚠ 只合批这一处，用户动作触发的十几个调用点仍是同步的（合批对它们无收益，反而把「点完立刻看到」变成「下一帧」）。"),
    ("src/frontend/ui/tab-bar-view.ts", "setTimeout", 2, "⑩ ★ F15：`scheduleRefresh`（原 `TabManager.scheduleTabBarRefresh`）的**无 rAF 兜底**，0ms、一次性。⑪ 「需要你」悬停 500ms 才开菜单：一次性，移开就清，不自链。"),
    // 〔拆 `tabs.ts` 子步 5〕会话动作搬进 `tab-session-actions.ts` ⇒ 原 ② ③ ④ ⑧ ⑨ 五处跟着走（下一行）：8 = 3 ＋ 5。
    // 5 → 3：`awaitExitFor` 的 ③ `stop(false)` 上限与 ④ 1s 轮询随它一起删了（换号重启直接 kill，不再等退出）。
    // 2 → 4：↗ 的「进行中」两处（`frontOnce`：超过 300ms 才进 · 进了至少停 400ms），都是一次性。
    ("src/frontend/ui/terminal-page.ts", "setTimeout", 2, "① 底部抽屉终端页：送字送键之后 0.5 · 1.5 · 3 秒各再抓一屏（一次动作三次、换会话 / 收起即清，不自链、开着不轮询）② 「已送达」2 秒后收。一次性。"),
    ("src/frontend/ui/tab-session-actions.ts", "setTimeout", 3, "⑧ `shellFront`：↗ 壳那一跳的期限（到点落成「无应答」，本机 / 远端两条共用这一处）。一次性，不是周期取数。编号沿用 `tabs.ts` 那一行拆开之前的原号。⑩ ⑪ `frontOnce`：↗ 在飞超过 300ms 才把按钮换成「进行中」· 进了之后至少停 400ms 再收（防闪），一次性。"),
    ("src/frontend/ui/views/grid-monitor.ts", "setInterval", 1, "1s 重绘 —— 按格差量（没变的一拍零 DOM 写），不再整表重建。**ui-clock，不取数**，见 `REGISTERED` 那条。"),
    // 历史页照稿重做：旧页那一处 rAF（展开 / 收起后合并重画）随旧页删了。
    ("src/frontend/ui/views/history.ts", "setTimeout", 3, "① 敲字之后停 150 ms 才问清单（`queryTimer`，再敲就重来）② 方向键走行时停 200 ms 才读右边（`previewTimer`，快速划过不读）③ 焦点离开列表那一下推到下一拍再看焦点去了哪（`focusout` 时 `activeElement` 还没换）。都是一次性，不取数、不是节拍器。"),
    ("src/frontend/ui/views/session-viewer.ts", "requestAnimationFrame", 5, "① ② 两处 `maybeFillAbove` —— **向上补料的 rAF 链**，五道守卫在 `:418-426`（世代 / 已到顶 / 在途 等）③ 渲染批前先让状态文绘一帧 ④ ⑤ 双 rAF 后重发 `scrollIntoView`（等 content-visibility 材料化）。"),
    ("src/frontend/ui/views/session-viewer.ts", "setTimeout", 1, "1.5s 后移除搜索命中的闪烁 class。一次性。原 ①（`setTimeout(r, 0)` 让出主线程、等晚到的 Channel 块）随那条命令改走通道删了：页在同一个 Promise 链里交完。"),
    ("src/frontend/ui/yield-to-main.ts", "setTimeout", 1, "`makeYieldToMain` 探不到 `MessageChannel` 时的兜底 `setTimeout(run, 0)` —— 让出一跳，由调用方自链（重放 drain · 长回复分片渲染），退出条件在调用方：队列空 / 片渲完。不是节拍器。"),
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

/// 扫描面：**全部**调度调用点（不只是「看起来像周期」的那些）。
fn scan_all_scheduling_sites() -> Vec<(String, String, usize)> {
    const APIS: [&str; 4] = [
        "setInterval",
        "setTimeout",
        "requestAnimationFrame",
        "requestIdleCallback",
    ];
    let root = repo_root();
    let mut files: Vec<PathBuf> = Vec::new();
    collect_ts(&root.join("src"), &mut files);
    files.sort();
    let mut out: Vec<(String, String, usize)> = Vec::new();
    for f in files {
        let rel = f
            .strip_prefix(&root)
            .unwrap_or(&f)
            .to_string_lossy()
            .replace('\\', "/");
        let src = guard_core::strip_comment_lines(&fs::read_to_string(&f).unwrap_or_default());
        for api in APIS {
            let n = count_calls(&src, api);
            if n > 0 {
                out.push((rel.clone(), api.to_string(), n));
            }
        }
    }
    out.sort();
    out
}

/// ★ **本轮的正题**：每一个调度调用点都必须被分类过。
///
/// 与 `every_periodic_wake_is_registered_with_an_owner` 的分工：
/// 那一条钉「**认出来的**周期唤醒有没有主人」，本条钉「**有没有认漏**」。
/// 两条都要 —— 只有前者时，一处新写的 rAF 自链可以一声不响地进仓。
#[test]
fn every_scheduling_call_site_is_classified() {
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
    let found = scan_all_scheduling_sites();
    // 抽取器自检：扫不到东西时下面的对拍会两边都空、静默变绿。
    let total: usize = found.iter().map(|(_, _, n)| *n).sum();
    assert!(
        total >= 30,
        "全仓只扫到 {total} 个调度调用点（实测应为 40+）—— 抽取器坏了，\
             下面的对拍会在两边都空的情况下变绿"
    );

    let mut missing: Vec<String> = Vec::new();
    let mut drifted: Vec<String> = Vec::new();
    for (f, api, n) in &found {
        match SCHEDULING_SITES
            .iter()
            .find(|(g, a, _, _)| g == f && a == api)
        {
            None => missing.push(format!("  {f}  [{api}]  {n} 处")),
            Some((_, _, want, _)) if want != n => {
                drifted.push(format!("  {f}  [{api}]  表里 {want} 处，实测 {n} 处"))
            }
            Some(_) => {}
        }
    }
    assert!(
        missing.is_empty(),
        "有调度调用点没被分类过。**这张表不是豁免清单** —— 请写清这几处各是什么：\n{}\n\
             （一次性 UI 反馈 / 有退出条件的自链 / 真 data-poll；是 data-poll 的还要进 `REGISTERED`）",
        missing.join("\n")
    );
    assert!(
        drifted.is_empty(),
        "调度调用点的处数变了 —— **那正是该重新分类的时刻**，别只改数字：\n{}",
        drifted.join("\n")
    );
    // 反向：表里的条目必须**真的还在**（搬走/删掉了就该删条目，别留僵尸账）。
    for (f, api, want, _) in SCHEDULING_SITES {
        assert!(
            found.iter().any(|(g, a, _)| g == f && a == api),
            "分类账里的 `{f} [{api}]`（{want} 处）已经一处都不剩了 —— 删掉这条"
        );
    }
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
