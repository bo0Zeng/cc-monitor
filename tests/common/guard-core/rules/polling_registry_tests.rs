use std::fs;
use std::path::PathBuf;

/// 周期唤醒的**登记表**：`(相对仓根的路径, 类别, 为什么 + 谁退役它)`。
///
/// 多一处没登记的 ⇒ 下面那条红。**登记表不是豁免清单**，是「这些我看过、
/// 而且知道它归谁」的账。
const REGISTERED: &[(&str, &str, &str)] = &[
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
    // ⚠ 第四类 `one-shot`：shell 那条针是宽的（`contains("sleep ")`），一次性 sleep 也会命中。
    // **刻意不收窄针**（宁可宽松让人判断，也不要用严格的错误引入噪声）
    // —— 收窄会漏掉「新加一个 `sleep 1` 在循环里」那种真轮询。代价是一次性的也要登记一行，
    // 而那正是「默认拒绝」想要的：新加一处就得回答它是哪一类。
];

/// ★ **前提触发器**：本模块头注说「Rust 那半由 `src/frontend/clippy.toml` 的定时器那一组挡」——
/// 那句话只在名单里真有那一组、壳与文件窗口两个包真把名单开成 deny 时成立。
/// 一句指向空处的指针读起来像「这里有人想过了」，下一个人就不会去查 ⇒ 指针要有判据看着。
#[test]
fn the_other_half_of_the_sweep_still_has_a_home() {
    let root = repo_root();
    let toml = fs::read_to_string(root.join("src/frontend/clippy.toml"))
        .expect("读不到 src/frontend/clippy.toml");
    // 名单里那一组：会让线程 / 任务自己醒来的构件。少了一个 ⇒ 那一种周期唤醒在壳里写得出、没人问。
    for path in [
        "std::thread::sleep",
        "std::thread::park_timeout",
        "std::sync::mpsc::Receiver::recv_timeout",
        "tokio::time::sleep",
        "tokio::time::interval",
    ] {
        let entry = format!("path = \"{path}\"");
        assert!(
            toml.contains(&entry),
            "`src/frontend/clippy.toml` 的定时器那一组里没有 `{path}` —— 本模块头注说 Rust 那半由它挡，此刻是假的。"
        );
    }
    // 名单只在开成 deny 的包上起作用。
    for manifest in [
        "src/frontend/shell/Cargo.toml",
        "src/frontend/filewin/Cargo.toml",
    ] {
        let body = fs::read_to_string(root.join(manifest)).expect("读不到包清单");
        let lints = body
            .split("[lints.clippy]")
            .nth(1)
            .and_then(|s| s.split("\n[").next())
            .unwrap_or_default();
        assert!(
            guard_core::pin_line(lints, "disallowed_methods = \"deny\"").is_ok(),
            "`{manifest}` 的 `[lints.clippy]` 没把 `disallowed_methods` 开成 deny ⇒ 名单在那个包上不起作用。"
        );
    }
    let me = fs::read_to_string(root.join("tests/common/guard-core/rules/polling_registry.rs"))
        .expect("读不到本文件");
    // 只看头注那半（`production_source` 剥掉 `#[cfg(test)]` 段）：拿整份文件找，本条自己的代码里就写着这个名字。
    let head_note = guard_core::production_source(&me);
    assert!(
        head_note.contains("src/frontend/clippy.toml"),
        "本模块头注里找不到 `src/frontend/clippy.toml` —— 指针被删了而本条还绿着，\n\
             说明本条钉的是「那半有人挡」而不是「这里指着它」。两件都要。"
    );
}

/// **明令不许有周期唤醒**的文件（把两处散文纪律变成机检）。
const NO_PERIODIC_WAKE: &[(&str, &str)] = &[(
    "src/frontend/ui/views/cc-bus-section.ts",
    "头注写着「本文件里不得出现 setInterval / setTimeout 轮询 / 后台定时任务」",
)];

fn repo_root() -> PathBuf {
    // 住址唯一源：`guard_core::addr`。
    guard_core::addr::repo_root()
}

/// 剥掉整行注释（`//` / `*` / `/*`）。
///
/// ⚠ 必须剥：`cc-bus-section.ts` 的头注里**就写着**
/// `setInterval` 这个词（写的是「不许有」）。不剥的话它们会被自己的纪律说明命中 ——
/// 与 `launch-cli-wire.vitest.ts` 那次「文档注释里就写着 `deny_unknown_fields`」同一个坑。
/// 一行里有没有周期唤醒的形态。
/// `shared/` 下的 shell 脚本全集：`.sh` 后缀 **或** 首行有 shebang（`cc-bus send`/`cc-bus recv`
/// 这些没有后缀）。**按内容认，不按后缀认** —— 后缀是可选的，shebang 才是「它是脚本」的证据。
///
/// 认法是「`.sh` 或首行 `#!`」，比 `guard_core::shell_scripts`（shebang 里要带 `sh`）宽：
/// `src/shared` 里那份 python 脚本也在人群里，它要是醒着轮询一样要登记。
fn shared_scripts() -> Vec<PathBuf> {
    guard_core::scan_tree!(&repo_root().join("src/shared"), &[])
        .into_iter()
        .filter(|(p, text)| {
            p.extension().and_then(|x| x.to_str()) == Some("sh") || text.starts_with("#!")
        })
        .map(|(p, _)| p)
        .collect()
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
/// ⚠ shell 脚本那一半是**遍历**，不是写死的文件名：写死的名单看不见新进仓的那棵树
/// （一个每 0.5s 醒一次扫队列目录的长驻进程，就能在名单外面轮询而本表不红）。
/// 同仓的 `shell_lint_registry`（扫描面含 `src/shared/cc-bus/scripts/*`）与
/// `session_name_registry` 也是同一个人群 ⇒ 加一个脚本自动进人群，不用谁记得回来 push 一行。
fn scan() -> Vec<(String, usize)> {
    let root = repo_root();
    let mut files = frontend_ts();
    files.extend(shared_scripts());
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

/// `src/**/*.ts`，测试与声明文件除外（已排序）。
fn frontend_ts() -> Vec<PathBuf> {
    guard_core::scan_tree!(&repo_root().join("src"), &["ts"])
        .into_iter()
        .map(|(p, _)| p)
        .filter(|p| {
            let name = p.file_name().unwrap_or_default().to_string_lossy();
            !name.contains(".vitest.") && !name.contains(".test.") && !name.ends_with(".d.ts")
        })
        .collect()
}

/// ★ 抽取器自检：扫不到东西时下面几条会零命中变绿。
#[test]
fn the_scan_actually_reads_the_frontend_and_ccm() {
    let root = repo_root();
    let ts = frontend_ts();
    // 采集面与 git 跟踪着的前端 `.ts` 对拍（测试与声明文件不在人群里）。
    let seen: Vec<String> = ts.iter().map(|p| guard_core::addr::rel_of(p)).collect();
    let skip: Vec<String> = guard_core::addr::tracked_under("src", "ts")
        .into_iter()
        .filter(|r| r.contains(".vitest.") || r.contains(".test.") || r.ends_with(".d.ts"))
        .collect();
    let skip: Vec<&str> = skip.iter().map(String::as_str).collect();
    guard_core::addr::assert_scanned_every_tracked("前端周期唤醒", &seen, &["src"], "ts", &skip);
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
    // 地板 `>= 3` 换成相等：今天 data-poll **0** 条（`tab-session-actions.ts` 的 `awaitExitFor` 随换号重启直接 kill 退役，
    //   cc-bus 那条长驻 broker 随投递只一条路删了，2 → 0）。
    //   地板在「少了一条」这个方向上判不出是退役还是抽取坏了 —— 相等判得出，而且逼人写清是哪一条。
    assert_eq!(
        polls, 0,
        "data-poll 条数变了（今天 0）—— 多了请登记事件源与退役去处"
    );
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

/// ★ **每一个调度调用点都带着分类标记，类在闭集里**。
///
/// 与 `every_periodic_wake_is_registered_with_an_owner` 的分工：那一条钉「**认出来的**周期唤醒有没有主人」，
/// 本条钉「**有没有认漏**」。
#[test]
fn every_scheduling_call_site_carries_a_classification_mark() {
    // 匹配单位自检（两个方向都要钉）：
    // 放松的那一侧 —— 带空白的调用要数进来（洞就在这里）。
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
    let files = frontend_ts();
    let (mut sites, mut bad, mut clocks) = (0usize, Vec::new(), std::collections::BTreeSet::new());
    for f in files {
        let rel = f
            .strip_prefix(&root)
            .unwrap_or(&f)
            .to_string_lossy()
            .replace('\\', "/");
        let raw = fs::read_to_string(&f).unwrap_or_default();
        let marks = scheduling_marks(&raw);
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
}

/// ★ **身份 poller 不许回来** —— ccm 渲出去的 shell 里
/// **一条与会话同寿的循环都不许有**。
///
/// # 钉的是什么
///
/// 要求「**可以动ccm. 不要轮询**」＋「**ccm做到必须走backend**」。
/// 身份通道 B 住后端（`src/backend/control/identity_tag.rs`，由它已有的 pidfile inotify 驱动），
/// 不在 ccm 里上 inotify（破「纯 POSIX shell、零第三方」，而 ccm 要部署到任意远端）。
/// ⇒ 没有「每次醒来的代价」可量，本条钉的是「那条与会话同寿的循环真的没了」。
///
/// # 为什么不是零命中的空守卫
///
/// 它有一个真实的反向锚点：同两份文件里**仍然有**渲给 shell 的串（容器路键入载荷那句
/// `send-keys -t`）。该钉的是**那一种形态**：与会话同寿的循环。
/// 下面第二段断言正是靠它证明抽取器没有空转。
#[test]
fn the_identity_poller_is_gone_for_good() {
    // 🔴 **语料是 `control/ccm/{mod,plan}.rs`**：容器路的那段 shell（send-keys
    //    载荷与收尾）由这两份 Rust 渲出来 ⇒ 「与会话同寿的循环不许回来」
    //    这件事要盯的是**产出方**。剥注释用 Rust 那套。
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
                 身份通道 B 住后端（`control/identity_tag.rs`），\n\
                 要求：「不要轮询」「ccm 做到必须走后端」。\n\
                 ⚠ 别把它当成「加个 sleep 兜一下更稳」——那正是这里要根除的东西：\n\
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
        prod.contains("tmux -u send-keys -t {t}"),
        "连容器路键入载荷那句 `tmux send-keys` 都扫不到 —— 剥法或路径坏了，上面那几条是零命中地绿"
    );
}

// ⚠ **如实边界**：一次性模式在 tmux 内由谁去打 `@ccm_sid`，今天**没有判据盯着**。

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

// ═══════════════════════════════════════════════════════════════════════
// **monitor Rust 自己拼出来的 shell 周期唤醒**：clippy 的名单只认 Rust 的调用，认不出拼进命令行里的 `sleep`
// ═══════════════════════════════════════════════════════════════════════

fn root() -> PathBuf {
    guard_core::addr::shell_root()
}

/// 生产段：用 `guard_core` 剥（连测试段一起剥）。
fn production(raw: &str) -> String {
    guard_core::production_code(raw)
}

/// 递归遍历 `src/`，**不是硬编码文件名单**。
fn rust_files() -> Vec<(String, String)> {
    let src = root().join("src");
    // 人群 ＝ 本 crate 的 `src/` ＋ manifest 明写的兄弟包（窗口包 · 通道 · 宿主原语 · 开窗契约）：
    //   `scan_tree!` 自己收兄弟包；键照住在哪棵树拼，兄弟包的键带包名。
    let trees: Vec<(String, PathBuf)> = std::iter::once(("src".to_string(), src.clone()))
        .chain(guard_core::population_trees(&src))
        .collect();
    let mut out: Vec<(String, String)> = guard_core::scan_tree!(&src, &["rs"])
        .into_iter()
        .map(|(p, raw)| {
            let (label, rest) = trees
                .iter()
                .find_map(|(label, tree)| p.strip_prefix(tree).ok().map(|r| (label, r)))
                .unwrap_or_else(|| panic!("{} 不在任何一棵人群树里", p.display()));
            (
                format!("{label}/{}", rest.to_string_lossy().replace('\\', "/")),
                raw,
            )
        })
        .collect();
    out.sort();
    out
}

/// shell 形态的周期唤醒。**只收「只可能是 shell」的写法**。
fn shell_wake_hits(prod: &str) -> usize {
    // 判据串运行时拼，免得命中本文件自己的说明。
    let pats = [
        format!("while {}", "[ "),
        format!("while {}", "true"),
        format!("until{}", " "),
        format!("sleep{}", " "),
        format!("us{}", "leep"),
        format!("$(s{}", "eq "),
    ];
    prod.lines()
        .filter(|l| {
            let t = l.trim_start();
            !t.starts_with("//") && pats.iter().any(|p| l.contains(p.as_str()))
        })
        .count()
}

/// ★ 抽取器自检：模式面既没画小也没画大。
#[test]
fn the_shell_wake_scan_is_neither_too_narrow_nor_too_wide() {
    // ★ **画大了会怎样**：摸底时 `for i in` 误命中了 `find_ci` 里那个 Rust `for i in 0..n`。
    //   这条把那次教训钉住 —— 模式面不许收到 Rust 的循环写法。
    //
    // 标的按住址读：`find_ci` 住 `src/backend/history/search_rules.rs`；它搬走时本条要跟着改住址，
    //   不然会以「那个循环不见了」的形态红，报的方向是错的。
    let anchor = guard_core::addr::repo_src_root().join("backend/history/search_rules.rs");
    let search = production(&fs::read_to_string(&anchor).unwrap_or_default());
    assert!(
        search.contains(&format!("for i in 0{}n", "..")),
        "`src/backend/history/search_rules.rs` 里那个 Rust `for i in 0..n`（`find_ci`）不见了 \
             —— 下面那条反向断言失去了标的"
    );
    assert_eq!(
        shell_wake_hits(&search),
        0,
        "模式面把 `search_rules.rs` 的 **Rust** 循环也收进来了 —— 那是摸底时踩过的那个错法\n\
             （画大了 ⇒ 这张表被噪音填满 ⇒ 与画小了一样失去意义）"
    );
}

/// ★★ **两个方向**：每一处 shell 周期唤醒都登记了；登记表里没有已经不存在的。
///
/// ⚠ 发现机制是**遍历**（`rust_files()`），不是手写清单 ——
/// 「一组同类东西都必须满足 X」的判据，清单只能用来**表态**。
#[test]
fn every_shell_shaped_periodic_wake_is_registered() {
    // ★ 反向自检先跑：表空了之后，「扫不到」必须与「扫描器坏了」分得开。
    //   合成一条 shell 轮询喂给同一把尺子，它必须数出 ≥1。
    let synthetic = format!(
        "let s = format!(\"while {} $i -lt 3 ]; do {} 0.5; done\");",
        "[", "sleep"
    );
    assert!(
        shell_wake_hits(&synthetic) >= 1,
        "尺子认不出一条摆在面前的 shell 轮询 —— 下面那格是零命中地绿"
    );
    let got: Vec<(String, usize)> = rust_files()
        .into_iter()
        .filter_map(|(rel, raw)| {
            let n = shell_wake_hits(&production(&raw));
            (n > 0).then_some((rel, n))
        })
        .collect();
    assert!(
        got.is_empty(),
        "\nmonitor Rust 又**拼出了 shell 周期唤醒**：{got:?}\n\
             远端那一侧的等待归后端（它有内核事件源）；先问能不能改问后端。"
    );
}
