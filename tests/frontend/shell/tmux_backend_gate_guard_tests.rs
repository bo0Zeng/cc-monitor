use std::fs;
use std::path::PathBuf;

/// backend 侧「有身份守卫」的标志。F03 之后**必须**出现。
///
/// 选这几个是因为它们是 monitor 侧那道门的**产物名**（拒绝码 / 拒绝文案）——
/// backend 复现 Gate 2 最自然的形态就是回一个同族的拒绝码，F03 正是这么做的
/// （`control/gate.rs::admit` 回 `wrong_owner` + `CCM_GUARD_REJECTED …`）。
// 拒绝那句话进了文案表（不再逐字带 `CCM_GUARD_REJECTED`）⇒ 标志换成真做判定的那一下 `gate2`。
// `gate-core` 收成后端模块 `control/gate_rules.rs` ⇒ 那一下的写法跟着换。
const BACKEND_GATE_MARKERS: &[&str] = &["gate_rules::gate2", "wrong_owner"];

/// Gate 3（`windows==1`，只约束破坏性动作）在后端侧的形状。
/// **F04a 起：必须存在**（此前是「一个都不该有」）。
const BACKEND_GATE3_MARKERS: &[&str] = &["session_windows", "kill-session"];

// 这里原来还有两张表：`GUARDED_COMMANDS`（要看住的两个命令：`tmux_send_keys` / `kill_remote_tmux`〔散文墓碑〕）
//   与 `BACKEND_CHANNEL_MARKERS`（它们走后端的标志 `backend_route::Routed`）。两条命令迁到界面之后，被看住的不再是
//   「monitor 里那两个函数体」，而是「monitor 里有没有这两件事的路」＋「界面经谁说」—— 见文末两条。

// `MONITOR_TMUX`〔散文墓碑〕（`include_str!` monitor 的 `tmux.rs`）删：那份文件随 monitor 侧的 Gate 残留删了，
//   下面那条改量整棵 monitor 生产段（`monitor_production_corpus`）。

fn backend_control_dir() -> PathBuf {
    // 住址唯一源：`crate::guard_support`。原来这里自己爬一级（`src/frontend/shell` 的上级只到
    // `<repo>/src`）⇒ 扫到 0 个 `.rs`，而本模块下面三条会**零命中地绿**。
    crate::guard_support::backend_src_root().join("control")
}

/// backend `control/` 下**全部** `.rs` 的生产段。
///
/// ⚠ **递归遍历，不是硬编码文件表** —— 本模块头注记着为什么：
/// 硬编码的两文件表让 F03 新增的 `gate.rs` 整个逃出了扫描面。
fn backend_control_production() -> Vec<(String, String)> {
    let dir = backend_control_dir();
    let mut out = Vec::new();
    let mut stack = vec![dir.clone()];
    while let Some(d) = stack.pop() {
        let Ok(rd) = fs::read_dir(&d) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
                continue;
            }
            if p.extension().and_then(|x| x.to_str()) != Some("rs") {
                continue;
            }
            let rel = p
                .strip_prefix(&dir)
                .unwrap_or(&p)
                .to_string_lossy()
                .replace('\\', "/");
            let raw = fs::read_to_string(&p).unwrap_or_default();
            out.push((rel, guard_core::production_code(&raw)));
        }
    }
    out.sort();
    out
}

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
    //    而**归零之后本条就是空真**（`bad` 恒空）—— 到那一拍该做的不是把地板改成 0，
    //    是给它换一份**会漂的活体语料**（同本文件 `every_target_placeholder_comes_from_exact_target`
    //    今天的做法：人群 0 + 一份合成坏语料承重）。
    (seen_read_only, bad)
}

// 〔`K-R72` 09-12 留档〕上一版判准的另一半住在这里 —— **刻意只留话，不留代码**。
//
// 原来那半逐字是：「直接含 `build_guarded_tmux_cmd` 的函数算走 Gate；调用了这样一个
// 函数的也算」（求闭包，因为 `kill_remote_tmux` → `build_kill_session_cmd` → Gate  〔散文墓碑〕
// 中间隔了一层；第一版没求闭包，那两处当场误红）。那段派生逻辑今天**没有被测对象**。
// 留一段跑不到的代码在这儿，就是本件 `KR72D1` 逐字禁止的那件事的测试侧变体：
// **盘上留着一条走不到的路。**⇒ 删干净，理由写在这里。

/// ★ 抽取器自检 B：backend `control/` 的**递归**扫描面没缩水。
///
/// 这条就是 F03 补上的那一条 —— 上一版没有它，扫描面从 5 个文件缩到 2 个也不会红。
#[test]
fn the_backend_control_scan_surface_is_not_a_hardcoded_short_list() {
    let files = backend_control_production();
    let names: Vec<&str> = files.iter().map(|(n, _)| n.as_str()).collect();
    assert!(
        files.len() >= 5,
        "backend control/ 只扫到 {} 个 .rs（{names:?}）—— 递归遍历坏了，\
             本模块下面几条会零命中地绿",
        files.len()
    );
    // 门住在这个文件里；它不在扫描面 = 反向锚点是空的。
    assert!(
        names.contains(&"gate.rs"),
        "扫描面里没有 `gate.rs`（实得 {names:?}）—— 那正是 F03 那次没红的形状"
    );
    let total: usize = files.iter().map(|(_, s)| s.len()).sum();
    assert!(
        total > 20_000,
        "backend control/ 生产段总共只剩 {total} 字节 —— 剥法或路径坏了"
    );
}

/// ★ **反向锚点**（F03 起）：backend 侧的身份门**必须还在**。
///
/// 前提触发器翻了个面：U10 时钉「不许出现」（backend 还没有门），
/// F03 装上之后钉「不许消失」。删掉 Gate 2 而门禁全绿，正是这条要挡的。
#[test]
fn the_backend_identity_gate_is_still_there() {
    let files = backend_control_production();
    let all: String = files
        .iter()
        .map(|(_, s)| s.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    let missing: Vec<&str> = BACKEND_GATE_MARKERS
        .iter()
        .copied()
        .filter(|m| !all.contains(m))
        .collect();
    assert!(
        missing.is_empty(),
        "backend 的 control 面**找不到**身份守卫的标志 {missing:?} ——\n\
             §34 的 Gate 2 在后端侧没了（F03 把它装在 `control/gate.rs::admit`）。\n\
             这道门挡的是「往一个不是本工具管理的 tmux 会话里打字」。\n\
             真要撤，先回定框 C6 重新裁定，别让它在一次重构里悄悄蒸发。"
    );
    // 门必须在**生产段**、且在 `gate.rs` 里 —— 只在测试里出现等于没有门。
    let gate_rs = files
        .iter()
        .find(|(n, _)| n == "gate.rs")
        .map(|(_, s)| s.as_str())
        .unwrap_or("");
    assert!(
        gate_rs.contains("gate_rules::gate2"),
        "`gate.rs` 的生产段没有调 `gate_rules::gate2` —— 判定要么被就地重写了一份\
             （那就成了第二个家），要么这道门只剩个壳"
    );
    // ★ **门必须在路上，不只是在仓里。**
    //
    // ⚠ 这一条是本轮变异复验补的：M2「把 `launch.rs` 的 `gate::admit` 拆掉、退回
    // `has-session` + 裸 `type_payload`」时，上面两条**照样全绿** —— 因为 `gate.rs`
    // 文件还在、标志串还在。抓到它的是后端自己那两条接线测试，而本模块
    // （monitor 侧那条禁令的**前提**）却认为「门还在」，前提就成了假的。
    // 「模块存在 ≠ 模块被调用」是判据缺陷的又一种形状：**扫到了东西，但扫的不是那件事。**
    let launch_rs = files
        .iter()
        .find(|(n, _)| n == "launch.rs")
        .map(|(_, s)| s.as_str())
        .unwrap_or("");
    assert!(
        launch_rs.contains("gate::admit"),
        "backend 的 `control/launch.rs` 生产段没有调 `gate::admit` ——\n\
             门还在仓里，但**不在路上**：`send-into` 会绕过 §34 的 Gate 2 直接键入。\n\
             monitor 侧那条「不许改走后端」的禁令，其前提正是「backend 的门是通的」。"
    );
}

/// ★ **F04a 起翻面：backend 的 Gate 3 必须还在**（此前钉的是「不许出现」）。
///
/// # 这条触发器完整走过了一遍它设计的生命周期
///
/// U10 立它时钉「不许出现」——因为那时后端没有 Gate 3，下面那条路由禁令
/// 靠的就是这个前提。F04a 把 Gate 3 搬进来，它**如设计般红了一次**：
/// `出现了 Gate 3 / kill 的标志 ["session_windows", "kill-session"] —— 这多半是好事`。
///
/// ⚠ 那时正确的处置**不是删掉它**（铁律 13：删判据前先证明它恒绿），
/// 而是**改写**：前提变了 ⇒ 换成钉新前提。现在它钉「Gate 3 不许消失」。
#[test]
fn the_backend_now_has_gate3() {
    let files = backend_control_production();
    let all: String = files
        .iter()
        .map(|(_, s)| s.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    let missing: Vec<&str> = BACKEND_GATE3_MARKERS
        .iter()
        .copied()
        .filter(|m| !all.contains(m))
        .collect();
    assert!(
        missing.is_empty(),
        "backend 的 control 面**找不到** Gate 3 的标志 {missing:?} ——\n\
             §34 的第三道门（`windows == 1`，防误杀多窗口会话）在后端侧没了。\n\
             F04a 把它装在 `control/gate.rs::admit_destructive`；`control/kill.rs` 走它。"
    );
    // 门必须**在路上**（F+02 的教训：模块存在 ≠ 模块被调用）。
    let kill_rs = files
        .iter()
        .find(|(n, _)| n == "kill.rs")
        .map(|(_, s)| s.as_str())
        .unwrap_or("");
    assert!(
        kill_rs.contains("admit_destructive"),
        "`control/kill.rs` 的生产段没有调 `admit_destructive` ——\n\
             门还在仓里但不在路上：kill 会绕过 Gate 2/3 直接杀。"
    );
    // Gate 3 **只给破坏性动作**：非破坏性的 `admit` 不许看窗口数。
    let gate_rs = files
        .iter()
        .find(|(n, _)| n == "gate.rs")
        .map(|(_, s)| s.as_str())
        .unwrap_or("");
    let plain = gate_rs
        .split("pub(crate) fn admit(")
        .nth(1)
        .and_then(|t| t.split("pub(crate) fn admit_destructive").next())
        .unwrap_or("");
    assert!(
        !plain.is_empty(),
        "抽不到非破坏性 `admit` 的函数体 —— 抽取器坏了，下面那条会零命中地绿"
    );
    assert!(
        !plain.contains("windows != 1") && !plain.contains("p.windows"),
        "非破坏性的 `admit` 里出现了窗口数判断 —— `send-keys` 不删除任何东西，\n\
             给它加 Gate 3 会让「往一个多窗口会话里打字」被误拒（monitor 侧 F04 Phase D 审计修过这个错法）。"
    );
}

// 这里原来住着四条：「`kill` 必须走后端通道」（`kill_now_routes_through_the_backend`〔散文墓碑〕）·
//   「过门被拒绝绝不回落」（`a_gate_rejection_is_never_laundered_into_the_ssh_fallback`〔散文墓碑〕）·
//   「`send-keys` 也必须走后端通道」（`send_keys_now_routes_through_the_backend`〔散文墓碑〕）·「两条命令走同一个分流器」
//   （`both_commands_branch_on_the_same_three_way_verdict`〔散文墓碑〕），外加抽取器自检 A。它们钉的都是 monitor 里
//   `kill_remote_tmux` / `tmux_send_keys` 那两个函数体（主路走后端 · 回潮闸 · 三态不许压成两态 · `enter` 真传过去）。
//   两条命令整条迁到界面（`src/frontend/ui/tmux-control.ts`）之后，那两个函数体不在了，每一格的去处：
//   · 主路走后端 ＋ 回潮闸 ⇒ 下面第一条：monitor 生产段里**一处**杀会话的 shell 串都没有（界面那一侧结构上没有 SSH）；
//   · 界面只经一处说这几条 ⇒ 下面第二条；
//   · 三态不许压成两态（「门拒绝」与「通道不在」两句话不同）⇒ `tests/frontend/ui/tmux-control.vitest.ts`（身份门那一句 ≠ 通道不在那一句）；
//   · `enter` 那一格随裸键 mode `send-keys-raw` 删了（之后无调用者；送键只剩 `send-into`）。
//   后端那两道门（身份 · 窗口）还在路上 —— 上面两条反向锚点不动，界面从此**只**靠它们。

/// monitor 生产段（剥 `#[cfg(test)]` 与注释行）里的全部 `.rs`，拼成一份语料。
fn monitor_production_corpus() -> (usize, String) {
    let root = crate::guard_support::repo_root().join("src/frontend/shell/src");
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
    let kill_rs = backend_control_production()
        .into_iter()
        .find(|(n, _)| n == "kill.rs")
        .map(|(_, s)| s)
        .unwrap_or_default();
    assert!(
        guard_core::contains_word(&kill_rs, &verb),
        "正控失败：后端 `control/kill.rs` 的生产段里认不出 `{verb}` —— 识别器瞎了，上面那个零命中不可信"
    );
}

/// ★★**界面说这条控制类帧命令只经一处**：`kill` 的 `chan.call` 只住 `src/frontend/ui/tmux-control.ts`；
/// `launch` 那一条界面不再发（送键 · 就地 resume 那两个入口删了）⇒ 全界面零处。
///
/// 守的要求：「迁到通道之后，业务解释是不是**只有一个家**」—— 空目标先拒（Gate 1 本地那一格）、
/// 按形状收、`killed` / `typed` 不为真不当成功、就地 resume 能不能回落（F14），这几件只写在那一份里；
/// 别处直接 `chan.call(…, "kill", …)` 就是绕过它们的第二条路。
/// 两向相等：出现这几个字面量的文件集合 == `{src/tmux-control.ts}`；正控：那一份里各自恰好几处。
#[test]
fn the_front_end_speaks_the_tmux_control_ops_only_through_one_module() {
    let root = crate::guard_support::repo_root();
    let mut homes: std::collections::BTreeMap<String, Vec<String>> = Default::default();
    let mut scanned = 0usize;
    let needles = ["chan.call(origin, \"kill\""];
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
        vec![("chan.call(origin, \"kill\"", 1)],
        "`src/frontend/ui/tmux-control.ts` 里这几条的处数变了 —— 多一处是长出了第二个调用点，少一处是那条路没了"
    );
}

// ════════════════════════════════════════════════════════════════════════
// 从 `tests/frontend/shell/tmux_tests.rs`〔散文墓碑〕挪来的三条：那份测试的挂载点 `tmux.rs`
// 随 monitor 侧的 Gate 残留删了；三条守的都是「monitor 里没有某一种 tmux 的路」，人群从一份文件扩到整棵 monitor 生产段。
// ════════════════════════════════════════════════════════════════════════

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
    let home_path = crate::guard_support::repo_root().join("src/backend/common/tmux_utf8.rs");
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
