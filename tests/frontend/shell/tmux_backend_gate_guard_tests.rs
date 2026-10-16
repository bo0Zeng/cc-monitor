use std::fs;

/// 远端 tmux 命令里**只读**的动词。不在这张表里的一律按「有破坏性」处理。
///
/// ⚠ **默认拒绝是本条的全部要点**：加一个新动词（`respawn-pane` / `kill-pane` /
/// `paste-buffer` / `set-option` …）不需要谁想起来把它登记成危险的 ——
/// 它天然就落在网里，除非有人**明确**把它写进这张只读表并为此负责。
const READ_ONLY_VERBS: &[&str] = &[
    "ls",
    "list-sessions",
    "list-windows",
    "capture-pane",
    "display-message",
    "show-option",
    "has-session",
];

/// 走 Gate 的唯一入口（Gate 2 远端半支与被守护的命令拼成**一条原子命令**）。
const GATE_BUILDER: &str = "build_guarded_tmux_cmd";

/// 取 `at` 所在的那个**顶层函数**（名字，函数体）。
fn enclosing_fn(src: &str, at: usize) -> (String, String) {
    let head = &src[..at];
    let start = [
        head.rfind("\nfn "),
        head.rfind("\npub fn "),
        head.rfind("\npub async fn "),
        head.rfind("\nasync fn "),
        head.rfind("\npub(crate) fn "),
    ]
    .into_iter()
    .flatten()
    .max()
    .unwrap_or(0);
    let rest = &src[start..];
    let end = rest.find("\n}\n").map(|k| k + 3).unwrap_or(rest.len());
    let body = rest[..end].to_string();
    let name = body
        .split_once("fn ")
        .map(|(_, t)| {
            t.chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                .collect::<String>()
        })
        .unwrap_or_default();
    (name, body)
}

/// ★ **正题（新增）**：monitor 侧发出的每一条远端 tmux 命令，
/// 要么动词是只读的，要么它所在的函数**走 Gate**。
///
/// # 它补的是哪个洞
///
/// 本模块原来只看住**两个写死的签名**（`GUARDED_COMMANDS`，随那两条命令迁到界面一起删了）。
/// 实测：往 `tmux.rs` 追加一条
/// `pub async fn tmux_respawn_pane(..)`，里面直接 `format!("tmux respawn-pane -k -t {..}")`
/// 再 `connect_and_exec_cmd` —— **`respawn-pane -k` 会杀掉 pane 里正在跑的进程**，
/// 是彻头彻尾的破坏性操作，而且**完全不经 §34 的任何一道 Gate**。
/// 全量 `cargo test --workspace`：**985 条全绿，一条都没响。**
///
/// ⇒ 病根还是那一个：**用手写清单描述人群**。看住的是「今天这两个函数」，
/// 不是「所有会对远端下命令的地方」。加第三个就出圈，而且没有任何信号。
///
/// # 人群与判准（先量后定，量到的都写在这）
///
/// 人群 = monitor 生产段里每一处 `tmux <动词>` 字符串（从前只量 `tmux.rs` 一份，那份文件删了）。
///
/// # ★★ `K-R72`（09-12）：判准**收紧了一格**，而且是**变强**不是变弱
///
/// 原判准是「动词 ∈ [`READ_ONLY_VERBS`] **或**所在函数含 `build_guarded_tmux_cmd`」。
/// 今天 monitor 侧那条会拼破坏性 tmux 命令的路整个删了（送键与杀会话只走后端）
/// ⇒ 那个「或」的右半**没有成员了**，而**留着一个空的或分支是危险的**：
/// 它给「重新长出一个受托者、然后把破坏性动词挂上去」留着一条合法路。
/// ⇒ 判准收成一条：**monitor 侧发出的每一条远端 tmux 命令都必须是只读动词。**
/// 这比原来严格 —— 原来允许「走 Gate 的破坏性动词」，今天一个都不允许。
///
/// ⚠ **人群里仍可能混进错误消息串**（`format!("tmux …: {..}")` 这种）。
/// 刻意不去区分「命令串」与「消息串」—— 文本上分不干净。⚠ 但**代价随判准收紧变了**：
/// 原先多算没有代价（那些函数本来就走 Gate），今天多算一处含破坏性动词的**消息串**
/// 就是一次**误红**。⇒ 现打：monitor 生产段里这样的消息串**零处**
/// （那两处 `tmux kill-session: {..}` / `tmux send-keys: {..}` 随回落一起走了）。
/// 哪天又长出来，正确处置是把那句消息改得不含裸动词，**不是**把动词塞进只读表。
/// `tmux ` 之后的动词：先跳过插值占位（`{UTF8_CLIENT_FLAG}`）与 `-x` 形状的全局旗标。
///
/// 原来只看紧跟 `tmux ` 的小写字母 ⇒ 真正发出去的那条
/// `tmux {UTF8_CLIENT_FLAG} ls -F …` 一直读成空动词、**被跳过**；本条的地板 1 其实是一句报错消息
/// （`CCM_TMUX_UNPARSABLE … tmux ls …`）撑着的。那句消息抽进文案表以后人群归零、地板红了，
/// 才看见这个洞：`tmux {旗标} respawn-pane` 这种形状同样会被跳过。⇒ 改成跳过占位与旗标再读动词，
/// 人群回到真命令本身。
fn verb_after_tmux(rest: &str) -> String {
    let mut s = rest;
    loop {
        s = s.trim_start_matches(' ');
        if s.starts_with('{') {
            match s.find('}') {
                Some(k) => s = &s[k + 1..],
                None => return String::new(),
            }
        } else if s.starts_with('-') {
            s = s.trim_start_matches(|c: char| !c.is_whitespace());
        } else {
            break;
        }
    }
    s.chars()
        .take_while(|c| c.is_ascii_lowercase() || *c == '-')
        .collect()
}

#[test]
fn verb_after_tmux_skips_placeholders_and_global_flags() {
    assert_eq!(verb_after_tmux("{UTF8_CLIENT_FLAG} ls -F x"), "ls");
    assert_eq!(verb_after_tmux("-u respawn-pane -k"), "respawn-pane");
    assert_eq!(verb_after_tmux("kill-session -t x"), "kill-session");
    assert_eq!(verb_after_tmux(">/dev/null"), "");
}

#[test]
fn every_remote_tmux_verb_is_either_read_only_or_routed_through_the_gate() {
    // 人群从 `tmux.rs` 一份扩到整棵 monitor 生产段（那份文件删了；比原来大，不是小）。
    let (_, prod) = monitor_production_corpus();
    // ★ `K-R72`：受托者不许再出现。这一句就是「那个空的或分支」的回潮闸 ——
    //   有人重新拼一个 `build_guarded_tmux_cmd` 出来，本条当场红。
    assert!(
        !prod.contains(GATE_BUILDER),
        "monitor 生产段里又出现了 `{GATE_BUILDER}` —— 那是 monitor 自己拼\n\
             「原子 verify+act 远端 shell 串」的受托者，`K-R72` 把它连同它唯一的两个消费者\n\
             （kill / send-keys 的一次性 SSH 回落）一起删了。要恢复它先回 `K-R54` 重新裁定。"
    );
    let (seen_read_only, bad) = scan_verbs(&prod);
    // 人群归零：`list_remote_tmux` 改问那台后端的 `tmux-list`，monitor 侧**零处**远端 tmux 命令串。
    //   照 `K-R112` 那段留下的话办：不把地板改成 0 了事，而是**人群恒等 0 ＋ 一份合成坏语料承重**（正控，扫描器认得出违规）。
    assert_eq!(
        seen_read_only, 0,
        "monitor 侧又长出了远端 tmux 只读命令串 —— 列会话今天问那台后端 `tmux-list`"
    );
    let (_, caught) =
        scan_verbs("fn synthetic() { let c = format!(\"tmux {x} kill-session -t =a:\"); }");
    assert_eq!(
        caught.len(),
        1,
        "正控：扫描器认不出合成语料里那条改状态的 tmux 命令 —— 本条空转"
    );
    assert!(
        bad.is_empty(),
        "monitor 侧有**不是只读动词**的远端 tmux 命令：\n{}\n\
             `K-R72` 起 monitor 只许对远端 tmux 下**只读**命令 —— 改状态的一律走后端\n\
             （`C5` 逐字：任何改状态的 tmux 命令一律归 `control/`）。\n\
             ⚠ **别把动词加进 `READ_ONLY_VERBS` 来消红** —— 那张表只收真正不改变远端状态的动词。\n\
             正确动作：把这条命令搬进后端的 `control/`，让它过 `admit` / `admit_destructive`。",
        bad.join("\n")
    );
}

/// 把「数远端 tmux 动词」那一段抽成函数：真生产段与合成正控语料共用同一把尺子。
fn scan_verbs(prod: &str) -> (usize, Vec<String>) {
    let mut seen_read_only = 0usize;
    let mut bad: Vec<String> = Vec::new();
    let mut from = 0usize;
    while let Some(rel) = prod[from..].find("tmux ") {
        let i = from + rel;
        from = i + "tmux ".len();
        let verb = verb_after_tmux(&prod[from..]);
        if verb.is_empty() {
            continue;
        }
        let (fname, _) = enclosing_fn(&prod, i);
        if READ_ONLY_VERBS.contains(&verb.as_str()) {
            seen_read_only += 1;
        } else {
            bad.push(format!("  {fname}() 里的 `tmux {verb}`"));
        }
    }
    // 抽取器自检：一处都没数到 ⇒ 剥生产段或扫描坏了，本条此刻量不到东西。
    // 🔴 `K-R112`（09-13）：地板 2 → **1**。`tmux capture-pane` 那一处随抓屏改走
    //    backend 帧面而不存在了 ⇒ monitor 侧只剩 `tmux … ls`（`list_remote_tmux`）一处。
    //    ⚠ **这一格快到头了**：`list_remote_tmux` 也改走后端的那天，这个人群会归零，
    //    而**归零之后本条就是空真**（`bad` 恒空）—— 到那时该做的不是把地板改成 0，
    //    是给它换一份**会漂的活体语料**（同本文件 `every_target_placeholder_comes_from_exact_target`
    //    今天的做法：人群 0 + 一份合成坏语料承重）。
    (seen_read_only, bad)
}

/// monitor 生产段（剥 `#[cfg(test)]` 与注释行）里的全部 `.rs`，拼成一份语料。
fn monitor_production_corpus() -> (usize, String) {
    let root = guard_core::addr::repo_root().join("src/frontend/shell/src");
    let mut corpus = String::new();
    let mut files = 0usize;
    for (_, one_file) in guard_core::scan_tree_excluding(&root, &["rs"], &[]) {
        files += 1;
        corpus.push_str(&guard_core::strip_comment_lines(
            &guard_core::production_code(&one_file),
        ));
        corpus.push('\n');
    }
    (files, corpus)
}

/// ★★**monitor 里没有杀会话的第二条路**（零命中 ＋ 正控）。
///
/// 守的要求：`INVARIANTS §34`（破坏性动作过三道门，门只住后端 `control/gate.rs`）与定框 `C5`
/// 「任何改状态的 tmux 命令一律归 `control/`」—— 杀会话今天只剩一条路：界面经通道说后端的 `kill`，
/// 后端先 `admit_destructive` 拿**句柄**再杀。monitor 里再长出一处自己拼 `kill-session` 的 shell 串，
/// 就是 `K-R72` 删掉的那条对**名字**下手的回落回来了（TOCTOU 窗口）。
/// 正控：同一识别器在后端 `control/kill.rs` 的生产段上认得出那个动词。
#[test]
fn the_monitor_has_no_second_path_that_kills_a_session() {
    let (files, corpus) = monitor_production_corpus();
    assert!(files > 100, "只扫到 {files} 份 monitor 源码 —— 遍历坏了");
    let verb = ["kill", "session"].join("-");
    assert!(
        !guard_core::contains_word(&corpus, &verb),
        "monitor 生产段里又出现了 `{verb}` —— 杀会话在 monitor 里长回了一条自己拼 shell 串的路。\n\
         杀会话只许走后端的 `kill`（先过身份门 ＋ 窗口门、对句柄下手）；界面经 `src/frontend/ui/tmux-control.ts::killSession` 说它。"
    );
    let kill_rs = guard_core::production_code(
        &fs::read_to_string(guard_core::addr::backend_src_root().join("control/kill.rs"))
            .expect("读后端 control/kill.rs"),
    );
    assert!(
        guard_core::contains_word(&kill_rs, &verb),
        "正控失败：后端 `control/kill.rs` 的生产段里认不出 `{verb}` —— 识别器瞎了，上面那个零命中不可信"
    );
}

/// ★★**界面说这条控制类帧命令只经一处**：`kill` 的 `callTyped` 只住 `src/frontend/ui/tmux-control.ts`；
/// `launch` 那一条界面不再发（送键 · 就地 resume 那两个入口删了）⇒ 全界面零处。
///
/// 守的要求：「迁到通道之后，业务解释是不是**只有一个家**」—— 空目标先拒（Gate 1 本地那一格）、
/// 按形状收、`killed` / `typed` 不为真不当成功、就地 resume 能不能回落（F14），这几件只写在那一份里；
/// 别处直接 `chan.call(…, "kill", …)` 就是绕过它们的第二条路。
/// 两向相等：出现这几个字面量的文件集合 == `{src/tmux-control.ts}`；正控：那一份里各自恰好几处。
#[test]
fn the_front_end_speaks_the_tmux_control_ops_only_through_one_module() {
    let root = guard_core::addr::repo_root();
    let mut homes: std::collections::BTreeMap<String, Vec<String>> = Default::default();
    let mut scanned = 0usize;
    let needles = ["callTyped(origin, \"kill\""];
    let gone = "chan.call(origin, \"launch\"";
    let mut gone_hits: Vec<String> = Vec::new();
    let mut counts: std::collections::BTreeMap<&str, usize> = Default::default();
    for (p, text) in guard_core::scan_tree_excluding(&root.join("src"), &["ts"], &[]) {
        let rel = p
            .strip_prefix(&root)
            .unwrap_or(&p)
            .to_string_lossy()
            .replace('\\', "/");
        scanned += 1;
        let prod = guard_core::strip_comment_lines(&text);
        if prod.contains(gone) {
            gone_hits.push(rel.clone());
        }
        for n in needles {
            let c = prod.matches(n).count();
            if c > 0 {
                homes.entry(n.to_string()).or_default().push(rel.clone());
                if rel == "src/frontend/ui/tmux-control.ts" {
                    *counts.entry(n).or_default() += c;
                }
            }
        }
    }
    assert!(scanned > 100, "只扫到 {scanned} 份前端源码 —— 遍历坏了");
    for n in needles {
        assert_eq!(
            homes.get(n).cloned().unwrap_or_default(),
            vec!["src/frontend/ui/tmux-control.ts".to_string()],
            "`{n}` 出现在 `src/frontend/ui/tmux-control.ts` 之外（或那一份里没有了）—— 界面说这条控制类帧命令的家不止一个"
        );
    }
    assert!(gone_hits.is_empty(), "界面又发起了 `launch`：{gone_hits:?}");
    // 正控 ＋ 恒等：结束 1（送键与就地 resume 那两处随入口删了；「打断」那个 mode 名 → 删；抓屏改走 `terminal-preview` → 删）。
    assert_eq!(
        counts.into_iter().collect::<Vec<_>>(),
        vec![("callTyped(origin, \"kill\"", 1)],
        "`src/frontend/ui/tmux-control.ts` 里这几条的处数变了 —— 多一处是长出了第二个调用点，少一处是那条路没了"
    );
}

/// ★★**monitor 里抓屏一条路都不剩**（零命中 ＋ 正控）。
///
/// 守的要求：「迁到通道之后，业务解释是不是**只有一个家**」——
/// 抓屏的解释今天只住 `src/frontend/ui/tmux-control.ts`；monitor 里再长出一条拼 shell 串抓屏的路，就是同一件事的第二份实现。
/// 正控：同一份语料里认得出 monitor 那个唯一的起子进程口（原来认的是 `tmux.rs` 的转调壳，那份文件删了）。
#[test]
fn the_monitor_has_no_capture_path_any_more() {
    let (files, corpus) = monitor_production_corpus();
    assert!(files > 100, "只扫到 {files} 份 monitor 源码 —— 遍历坏了");
    // 形态现拼，免得本文件自己被别的扫描收进人群。
    let shapes = [
        format!("tmux {}", ["capture", "pane"].join("-")),
        ["NO", "PANE"].join("_"),
        ["capture", "via", "backend"].join("_"),
    ];
    for shape in &shapes {
        assert!(
            !corpus.contains(shape.as_str()),
            "monitor 生产段里又出现了 `{shape}` —— 抓屏在 monitor 里长回了一条路；\
             它只许住界面一处（`src/frontend/ui/tmux-control.ts::capturePane`）"
        );
    }
    assert!(
        guard_core::contains_word(&corpus, "ConsolePolicy"),
        "正控失败：同一份语料里认不出 `spawn_managed` 的 `ConsolePolicy` —— 上面的零命中不可信"
    );
}

/// ★ **monitor 里没有一处把目标插进 tmux 命令串**（`-t {…}`，零命中 ＋ 合成语料两向承重）。
///
/// 裸 `-t <名>` 是「精确 → 名字开头 → glob」三级解析（F01，tmux 3.6 实测：只有 `sib-2` 时 `kill-session -t sib` 杀掉 `sib-2`）。
/// 精确匹配形 `=名:` 只住后端 `control/launch.rs::exact_target`；monitor 这一侧连那份跨轨锚点也删了 ⇒ 人群恒等 0，
/// 牙压在两份合成语料上（同一把尺子在坏语料上红、在调了 `exact_target` 的语料上不红）。
#[test]
fn every_target_placeholder_comes_from_exact_target() {
    fn bad_of(src: &str) -> Vec<String> {
        let lines: Vec<&str> = src.lines().collect();
        let mut out = Vec::new();
        for (i, line) in lines.iter().enumerate() {
            if !line.contains("-t {") {
                continue;
            }
            let mut s = i;
            while s > 0 && !lines[s].contains("fn ") {
                s -= 1;
            }
            let name: String = lines[s]
                .split(" fn ")
                .nth(1)
                .or_else(|| lines[s].strip_prefix("fn "))
                .unwrap_or("")
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            let body: String = lines[s..=i].join("\n");
            if !body.contains("exact_target(") {
                out.push(name);
            }
        }
        out
    }
    let (_, corpus) = monitor_production_corpus();
    assert_eq!(
        bad_of(&corpus),
        Vec::<String>::new(),
        "monitor 生产段又出现了把目标插进 tmux 命令串的地方 —— 三条 tmux 控制命令全走后端帧面，精确匹配形只住后端"
    );
    assert!(
        !corpus.contains("-t {"),
        "monitor 生产段里出现了 `-t {{…}}` —— 那条路已经收干净了"
    );
    const SYNTHETIC_BAD: &str =
        "fn zzz_probe(x: &str) -> String {\n    format!(\"tmux kill-session -t {x} 2>&1\")\n}\n";
    assert_eq!(
        bad_of(SYNTHETIC_BAD),
        vec!["zzz_probe".to_string()],
        "本条的尺子在一份**明摆着裸目标**的语料上都不红 —— 它此刻什么都没在守"
    );
    const SYNTHETIC_OK: &str =
        "fn zzz_ok(x: &str) -> String {\n    let t = exact_target(x);\n    format!(\"tmux kill-session -t {t} 2>&1\")\n}\n";
    assert!(
        bad_of(SYNTHETIC_OK).is_empty(),
        "本条的尺子把一份**调了 `exact_target` 的**语料也判红了 —— 它恒红，不是在守"
    );
}

/// ★★ K-R12（09-04）「同一个口径只有一个家」—— tmux 打印通道的 UTF-8 口径（`INVARIANTS §49`）只住后端
/// `src/backend/common/tmux_utf8.rs`，monitor 这一侧零份（monitor 零处跨 SSH 的 tmux 读）。
///
/// 要求住址：`INVARIANTS §49`（tmux 打印通道必须是 UTF-8，段数下溢出声）。
/// 三件：① 家在、有内容 · ② 家里有下溢谓词、monitor 生产段没有第二份 · ③ 家里两种表示（argv 旗 · env）都在、monitor 不许长出 env 形。
#[test]
fn utf8_client_kou_jing_has_one_home_and_this_side_has_none() {
    let home_path = guard_core::addr::repo_root().join("src/backend/common/tmux_utf8.rs");
    let home = std::fs::read_to_string(&home_path)
        .unwrap_or_else(|e| panic!("读不到后端侧那个家 {home_path:?}：{e}"));
    assert!(
        home.len() > 2_000,
        "backend 那个家只有 {} 字节 —— 没读到内容",
        home.len()
    );
    let (_, corpus) = monitor_production_corpus();
    let body = format!("{}.count() < expected", ".split('\\t')");
    assert!(
        !corpus.contains(&body),
        "monitor 这一侧又长出了一份下溢谓词 —— 解析住后端（`observe/tmux_list.rs`），口径只许一个家"
    );
    assert!(
        home.contains(&body),
        "backend 那个家里找不到下溢谓词 `{body}`"
    );
    for ident in ["UTF8_CLIENT_FLAG", "UTF8_CLIENT_ENV"] {
        let needle = format!("const {ident}:");
        assert!(home.contains(&needle), "backend 那个家里少了 `{needle}`");
        let renamed = format!("const {ident}X:");
        assert!(
            !format!("pub {renamed} (&str, &str) = (\"x\", \"y\");\n").contains(&needle),
            "锚点匹配单位比事实小：`{renamed}` 也算成 `{needle}` 在"
        );
    }
    assert!(
        !corpus.contains("LC_ALL"),
        "monitor 这一侧长出了 env 形 —— 它今天零处跨 SSH 的 tmux 读，更不该自带口径"
    );
}
