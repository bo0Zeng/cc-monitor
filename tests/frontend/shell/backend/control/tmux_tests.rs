// 〔MIG-1 续〕这里原来住着单一落点宏 `backend_watcher_src`〔散文墓碑〕（`include_str!` 读后端 `watcher.rs` 那条 `TMUX_LS_FMT` 双写点）；
//   格式串只剩后端一个家、那条对拍随之删了 ⇒ 宏没有消费者，一起删。

// 〔MIG-1〕monitor 这一侧的 tmux 观测分类（`classify_tmux_observation`〔散文墓碑〕）与它那条 `OBS_*` 双写点判据删了：
//   收割搬进后端会话账本（`tests/backend/observe/session_ledger_tests.rs` 钉「不可观测不收割」）。

use super::*;

/// A5：send-keys 目标白名单——只认本工具的 cc-* 会话名，拒用户别的 tmux。
#[test]
fn ccm_tmux_name_whitelist() {
    assert!(is_ccm_tmux_name("cc-abc12345"));
    // ★ S4b-3b：新命名 `<X>-cc`（撞名时 `<X>-cc-<N>`）也要本地命中，
    // 否则每次 kill/send-keys 都要多跑一趟远端去核 `@ccm_sid`。
    assert!(is_ccm_tmux_name("abc12345-cc"));
    assert!(is_ccm_tmux_name("abc12345-cc-2"));
    assert!(is_ccm_tmux_name("my-proj-cc"));
    // **老前缀必须继续命中** —— 用户机器上正跑着的会话就是这个形状，
    // 不认它们等于把它们变成 issue #76 那种「失管会话」。
    assert!(is_ccm_tmux_name("cc-proj"));
    // 退化名不该命中：`-cc` 前面得有东西。
    assert!(!is_ccm_tmux_name("-cc"));
    // 名字里恰好含 `-cc` 但不是以它结尾、也不是 `-cc-<数字>` ⇒ 不认
    //（那多半是别人的会话，误认会让我们跳过远端核验就去 kill）。
    assert!(!is_ccm_tmux_name("foo-ccx"));
    assert!(!is_ccm_tmux_name("foo-cc-bar"));
    assert!(is_ccm_tmux_name("cc-abc12345-2")); // pickFreshTmuxName 的 -N 变体
    assert!(!is_ccm_tmux_name("cc-")); // 只前缀无体
    assert!(!is_ccm_tmux_name("web")); // 用户自己的会话
    assert!(!is_ccm_tmux_name("mycc-x")); // 非前缀
    assert!(!is_ccm_tmux_name("cc-a b")); // 空格（注入面）
    assert!(!is_ccm_tmux_name("cc-a;rm")); // 分号
    assert!(!is_ccm_tmux_name("cc-a$x")); // 元字符
}

/// F04 Gate 1：**空 target** 恒被拒——`=:` 会解析成「当前会话」，是唯一真正危险的默认值。
/// 〔DUP3 · 主会话 09-26 裁〕Gate 1 并进 `gate-core` 的 tmux 名那一族：判定是「已有会话」那一条
/// `gate_core::existing_tmux_name_issue`（空 · 控制符 · 视觉欺骗字符），先前本侧与界面各一份的「只拒空」是它的真子集。
///
/// # ⚠ `K-R72`（09-12）：**人群没缩，只是换了住址**
///
/// 送键与杀会话那两条桌面侧回落删掉之后 `build_kill_session_cmd` /  〔散文墓碑〕
/// `build_send_keys_remote_cmd` 不在了 —— 但 Gate 1 **不是那两条回落的东西**：  〔散文墓碑〕
/// 它守的是「任何拿 target 去做事的入口，都得先把空目标拒掉」。⇒ 本条改打
/// **今天三条路各自真正的入口**，一条都没少：
/// ① 谓词本体 [`gate1_admit_target`]（今天只剩 [`exact_target`] 这个跨轨锚点在用）；
/// ②③④〔C4e · 第四波 4C〕三条路（抓屏 · 送键 · 杀会话）的生产入口原本也在这里真跑一遍、断言在任何 IO 之前就地拒；
///    三条整条迁到界面之后（`src/frontend/ui/tmux-control.ts`），那一格随入口搬过去：`tests/frontend/ui/tmux-control.vitest.ts`
///    「空目标就地拒，一个字节都不发」三个入口各一条（Tauri 命令 `tmux_send_keys` / `kill_remote_tmux`〔散文墓碑〕删了）。
///
/// 含 glob/元字符但非空的 target **不**在这一层被拒（`shell_quote` 已安全引号化，
/// 字符集收紧是〔DUP2〕`gate-core` 那两条（`new_tmux_name_issue` / `existing_tmux_name_issue`）的职责，
/// 见 `gate1_admit_target` 头注）。
///
/// 〔IV1 · V121〕要求住址：`INVARIANTS §47`（外部值拼进 shell / 交给对端之前本侧先过放行判定）；②形（attach 目标只拒空）。
#[test]
fn gate1_admits_an_existing_target_by_the_one_gate_core_rule() {
    // ① 谓词本体（正反各一格 —— 只钉「坏的被拒」的话，把它焊死成恒拒也能绿）
    assert!(
        gate1_admit_target("").is_err(),
        "空 target 应被 Gate 1 拒绝（谓词本体）"
    );
    assert!(
        gate1_admit_target("cc-a b").is_ok(),
        "非空 target 不该被 Gate 1 拒绝（谓词本体）"
    );
    // 非空、含元字符/glob 的已有会话名不被 Gate 1 拒（收紧字符集是新建那一条的职责，不许在 Gate 1 顺手做）。
    for safe_nonempty in ["cc-a b", "cc-a;rm", "cc-a$x", "si*", "a'b", "a=b", "会话"] {
        assert!(
            gate1_admit_target(safe_nonempty).is_ok(),
            "非空 target {safe_nonempty:?} 不该被 Gate 1 拒绝"
        );
    }
    // 〔DUP3〕并进 gate-core 那一条之后多拒的两类：控制符 · 视觉欺骗字符（V131 ② 的拒绝集），那一句说出是哪个码位。
    for (bad, cp) in [
        ("a\nb", "U+000A"),
        ("a\u{1b}b", "U+001B"),
        ("a\u{200b}b", "U+200B"),
        ("a\u{202e}b", "U+202E"),
    ] {
        let e = gate1_admit_target(bad).expect_err(bad);
        assert!(e.contains(cp), "{bad:?}：那一句没说出是哪个字符：{e}");
    }
    // 判定真是 gate-core 那一条（不是本侧又写了一份）：两者对同一批样本逐个同答。
    for v in [
        "",
        "x",
        "si*",
        "a\u{0}b",
        "a\u{2060}b",
        "-lead",
        "a:b",
        "a.b",
    ] {
        assert_eq!(
            gate1_admit_target(v).is_ok(),
            gate_core::existing_tmux_name_issue(v).is_none(),
            "{v:?}：Gate 1 与 gate-core 已有会话那一条答得不一样"
        );
    }
}

// 〔SH1〕`the_surviving_cross_ssh_tmux_read_asks_for_a_utf8_client_before_the_subcommand`〔散文墓碑〕 那条退役：它守的那条跨 SSH `tmux ls` 串不在了（改问那台后端 `tmux-list`）。

/// backend 侧那个「一个口径一个家」的家（相对**仓根**）—— 跨仓对拍的被读对象。
///
/// 单一落点：路径写死在这里一处，backend 再搬家只改这一行。
const BACKEND_KOU_JING_HOME: &str = "src/backend/common/tmux_utf8.rs";

/// ★★ **K-R12 下一拍（09-04）：「同一个口径只有一个家 + 另一侧引用它或有对拍」——
/// 本 const 走的是**对拍**那一支。**
///
/// # 这条钉的是**关系**，不是词表
///
/// 三件事一起断言，缺一件就只买到一角：
///
/// | # | 断的什么 | 缺了它会怎样 |
/// |---|---|---|
/// | ① | 本侧**只有一个**声明（本文件那一处，全 monitor 树无第二处） | 本侧自己先分了两份，对拍再准也没用 |
/// | ② | 本侧那个值与后端家里那一行**逐字相等**（值是从本侧 const **现取**的） | 两侧漂开而两边都不红 —— 正是本件治的那个形状 |
/// | ③ | backend 家里**两种表示都还在** | 有人把家「收口」成一种表示 ⇒ 另一类调用点静默失效 |
///
/// ②③ 都是**读两棵树**才验得了的性质：两个 crate 不共享源码树，共用 `const` 拿不到
/// （七个 `*-core` 的职责逐条都装不下，论据在 `UTF8_CLIENT_FLAG` 的头注里）。
///
/// # ⚠ 作用域，逐条说清（`brief` 12：报一个数就要说清尺子）
///
/// - **在哪跑**：monitor 那格 cargo（`cargo test --workspace --lib`）。
///   backend 自己那格看不见它 —— 但门禁两格都跑，所以任一侧漂开都会在门禁里红。
/// - **读了哪两棵树**：本侧 `include_str!("../../../../../src/frontend/shell/src/backend/control/tmux.rs")`（编译期，同一半）+
///   backend 侧 [`BACKEND_KOU_JING_HOME`]（**运行期** `read_to_string`）。
/// - 🔴 **为什么后端那一半刻意用运行期读、而不是 `include_str!`**：
///   `include_str!` 会新长出一条**跨半边的编译期边**，而那种边由
///   `cross_half_edge_registry::CROSS_EDGES` 逐条登记着（多一条就红），
///   **那个文件不在本拍写区**。运行期读在本仓是**既有做法**、不是绕道：
///   `cross_half_edge_registry` 自己就是运行期遍历后端那棵树的
///   （`both_halves()` 扫 `src/backend`），`scanning_guard_registry::PENDING`
///   里也直接列着后端的文件。而且它在该登记表关心的那一维上**更轻**：
///   backend 换布局时这里是一句说得清的运行期失败，不是 `cargo test` 编不过。
///   ⚠ 代价如实写下：这条边因此**不出现在** `CROSS_EDGES` 里。
///   PM 若要它以编译期形态登记，改法是**两处一起动、不许只动一处**：
///   ① 把下面那句运行期读换成编译期读（`include_str!` 配 `concat!` / `env!` 拼路径，
///      形状照本文件原有的 `backend_watcher_src`〔散文墓碑〕那个单一落点宏）；
///   ② 同轮在 `CROSS_EDGES` 里加一条 `monitor→backend` 的登记
///      （读者 `src/frontend/shell/src/backend/control/tmux.rs` · 被读 `src/backend/common/tmux_utf8.rs` ·
///      理由「跨轨对拍：口径的家在对面，本侧那一份必须与它逐字相等」）。
///   🔴 只动 ① 会让那张表的条数当场对不上 —— 它是**两个方向都查**的。
/// - **不管什么**：它不证明「那个旗真的被走到了」（「盘上有 ≠ 被走到」）。
///   行为那一半的死值在 `tests/evidence/K-R12-deathvalue.md`（真 tmux 3.4 私有 socket）。
///
/// 〔IV1 · V121〕要求住址：`INVARIANTS §49`（tmux 打印通道必须是 UTF-8，段数下溢出声）（跨仓对拍）。
#[test]
fn utf8_client_kou_jing_has_one_home_and_this_side_matches_it() {
    let prod = guard_core::production_code(include_str!(
        "../../../../../src/frontend/shell/src/backend/control/tmux.rs"
    ));
    guard_core::assert_no_test_code("tmux.rs", &prod);

    // ── ① 〔SH1〕本侧的旗随跨 SSH 那条读删了（monitor 零处跨 SSH tmux 读）；下面只对拍下溢谓词。
    let root = crate::guard_support::repo_root();
    // ── ② 与后端那个家逐字相等（值现取，不写死） ──────────────────
    let home_path = root.join(BACKEND_KOU_JING_HOME);
    let home = std::fs::read_to_string(&home_path).unwrap_or_else(|e| {
        panic!(
            "读不到后端侧那个家 {home_path:?}：{e}\n\
                 它是 K-R12 下一拍建的「一个口径一个家」。文件被搬了 ⇒ 改本文件那个常量；\
                 家被删了 ⇒ 那个口径退回三份靠人对齐，先回件文件。"
        )
    });
    assert!(
        home.len() > 2_000,
        "backend 那个家只有 {} 字节 —— 没读到内容，下面的对拍是空转的",
        home.len()
    );
    // 段数下溢那个谓词是同一族的第二个口径：比的是**函数体**，不是名字。
    // 〔MIG-1 续〕本侧那一份随解析搬进后端删了 ⇒ 改钉「只剩那个家」：家里有它、本侧没有第二份。
    let body = format!("{}.count() < expected", ".split('\\t')");
    assert!(
        !prod.contains(&body),
        "monitor 这一侧又长出了一份下溢谓词 —— 解析住后端（`observe/tmux_list.rs`），口径只许一个家"
    );
    assert!(
        home.contains(&body),
        "backend 那个家里找不到下溢谓词 `{body}` —— 口径的家变了，或被改成了 `!=`（会误伤合法内容）"
    );

    // ── ③ backend 家里两种表示都还在 ───────────────────────────────────
    // ⚠ 锚点只钉「那里有一个声明」（`const <名>:`），**不钉类型写法** ——
    //   带类型标注的锚点实测会被 `(&'static str, &'static str)` 这种合法写法误伤，
    //   而它印出来的话是「家里少了 env 形」：一句指向完全错误方向的诊断。
    //   ⚠ 同时**刻意收在 `:` 上**：收在标识符上时 `const <名>X:` 会被裸 `contains`
    //   当成命中（「匹配单位比事实小」那一族），于是「家改名了」这一形看不见。
    // 表里存**标识符**，锚点现拼 —— 反向自检那份「改了名」的夹具必须从标识符派生，
    // 从锚点文本派生的夹具会跟着锚点一起变松，于是「锚点变松了」这件事自己看不见
    // （backend 那侧的同职判据实测栽过这一形，头注里逐字记着）。
    let anchor = |ident: &str| format!("const {ident}:");
    for (label, ident) in [
        ("argv 形（旗）", "UTF8_CLIENT_FLAG"),
        ("env 形", "UTF8_CLIENT_ENV"),
    ] {
        let needle = anchor(ident);
        assert!(
            home.contains(&needle),
            "backend 那个家里少了**{label}**（找不到 `{needle}`）—— \
                 「一个口径两种表示」被收口成一种了，而两类调用点各需要一种：\
                 少了哪一种，那一类调用点就静默退回非 UTF-8 客户端。"
        );
        // 反向自检（③ 那一半的牙）：一个**改了名**的声明不许算命中。
        // 没有这一格，③ 就是「那个大文件里恰好有这个串」式的恒真。
        let renamed = anchor(&format!("{ident}X"));
        assert!(
            !format!("pub {renamed} (&str, &str) = (\"x\", \"y\");\n").contains(&needle),
            "③ 的锚点匹配单位比事实小：`{renamed}` 这样一个改了名的声明也算成 `{needle}` 在"
        );
    }
    // 本侧**不许**长出 env 形：跨 SSH 这一侧没有本地 `Command` 可挂 env，
    // 走 `request_env` 要赌对端 `AcceptEnv`（不认就静默拒绝）⇒ 拿一条静默失效治另一条。
    //
    // ⚠ **必须先剥注释再扫**：本文件的头注里逐字讨论过 `LC_ALL` 那条路为什么不走
    //   （那是**警告**，不是用法），不剥就当场误报 —— 本仓「判据数到注释」已栽过三次。
    let code = guard_core::strip_comment_lines(&prod);
    assert!(
        code.len() > 800, // 〔MIG-1 续〕5000 → 800：列会话那一族搬进后端，本文件生产段只剩 Gate 1 对拍锚点与转调壳（现打约 1.2k）
        "剥注释之后只剩 {} 字节 —— 剥过头了，下面那条在空转",
        code.len()
    );
    assert!(
        !code.contains("LC_ALL"),
        "monitor 这一侧长出了 env 形 —— 〔SH1〕它今天零处跨 SSH 的 tmux 读，更不该自带口径"
    );

    // ── 反向自检：上面那两条 `contains` 真的分得清 ────────────────────
    // 没有这一格，② 就可能是「随便什么串都在那个大文件里」式的恒真。
    let drifted = format!("{}: &str = \"-ux\";", "UTF8_CLIENT_FLAG");
    assert!(
        !home.contains(&drifted),
        "喂一个**漂了的**值居然也在后端那个家里命中（`{drifted}`）—— \
             ② 那条对拍此刻恒真，它什么都没在守"
    );
    assert!(
        !home.contains(&format!("{}.count() != expected", ".split('\\t')")),
        "backend 家里同时存在 `!=` 那一版下溢谓词 —— 口径不一致，且 `!=` 会误伤合法内容"
    );
}

/// ★ **每一处 `-t {…}` 的目标都必须出自 `exact_target`**〔audit-0805 08-07〕。
///
/// 裸 `-t <名>` 是「精确 → 名字开头 → glob」三级解析。实测（tmux 3.6）只有 `sib-2`
/// 存在时 `kill-session -t sib` 杀掉 `sib-2` 且 **rc=0**、`send-keys -t sib` 投进
/// `sib-2`、`kill-session -t 'si*'` glob 命中。本仓必然踩
/// （`pickFreshTmuxName` 造 `<sid8>-cc-2/-3`、终端 `cct` 造 `<dir>_cc-2/-3`）。
///
/// # ⚠ `K-R72`（09-12）：人群从 4 处缩到 1 处，牙跟着换住址
///
/// 那 4 处里有 3 处（`display-message` / `kill-session` / `send-keys`）随两条回落走了 ——
/// 今天 monitor 侧**只有 `capture-pane` 还在把目标插进一条 tmux 命令串**。
/// 顺带走的还有原来那半「委托给 `build_guarded_tmux_cmd` 也算」的闭环逻辑：
/// **没有受托者了，判准就回到最简的那一条** —— 谁插目标谁调 `exact_target(`。
///
/// 🔴 **分母掉到 1 之后，「地板 ≥ N」这种抽取器自检就买不到东西了**（1 处也过、
/// 0 处才红，而 0 处那天本条本来就该重写）。⇒ 换成**喂一份合成的坏语料**：
/// 同一把尺子必须在坏语料上红。这样「人群只剩一个」不等于「判据变空转」。
#[test]
fn every_target_placeholder_comes_from_exact_target() {
    // 把「找出每一处 `-t {…}` 所在的函数」抽成纯函数 —— 于是同一把尺子既量真生产段，
    // 也量下面那份合成的坏语料。**尺子只有一份**是本条的要点。
    fn sites(src: &str) -> Vec<(String, String)> {
        let lines: Vec<&str> = src.lines().collect();
        let mut out: Vec<(String, String)> = Vec::new();
        for (i, line) in lines.iter().enumerate() {
            if !line.contains("-t {") {
                continue;
            }
            // 往回找最近的 `fn 名字`，再取它的体（到下一个顶格行；
            // `where` / `)` 顶格的是头的一部分 —— 这一族本仓已栽过三次）。
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
            let mut body = Vec::new();
            for (k, l) in lines[s..].iter().enumerate() {
                let cont = l.starts_with("where") || l.starts_with(')') || l.trim() == "{";
                if k > 0 && !l.is_empty() && !l.starts_with(char::is_whitespace) && !cont {
                    break;
                }
                body.push(*l);
            }
            out.push((name, body.join("\n")));
        }
        out
    }
    fn bad_of(src: &str) -> Vec<String> {
        sites(src)
            .into_iter()
            .filter(|(_, body)| !body.contains("exact_target("))
            .map(|(name, _)| name)
            .collect()
    }

    let prod = guard_core::production_code(include_str!(
        "../../../../../src/frontend/shell/src/backend/control/tmux.rs"
    ));
    let found = sites(&prod);
    // 🔴 **`K-R112`（09-13）：人群从 1 掉到 0 —— 这是这条棘轮的终态，不是它坏了。**
    //   `build_capture_pane_cmd`〔散文墓碑〕是最后一个把目标插进 tmux 命令串的地方，抓屏改走
    //   `capture-pane` 帧之后它整块删了 ⇒ monitor 侧**再没有一处**在拼 tmux 目标。
    //   ⚠ 分母 0 的判据是**空真** —— 所以这一条的牙从此**全部**压在下面那份合成语料上：
    //   同一把尺子在坏语料上必须红。两样一起断，缺一条本条就成了摆设。
    assert!(
        found.is_empty(),
        "生产段又出现了把目标插进 tmux 命令串的地方：{:?}\n\
             —— 那条路 `K-R72`/`K-R112` 已经收干净了（三条命令全走后端帧面）。\n\
             真要新增一处，`exact_target` 今天只住后端侧\n\
             （`src/backend/control/launch.rs`），别在这里重新长一份。",
        found.iter().map(|(n, _)| n).collect::<Vec<_>>()
    );
    // ★ 反向自检：同一把尺子在**合成的坏语料**上必须红。
    //   ⚠ 语料里刻意不出现 `exact_target`，断言也不取自夹具的名字（`6g` 那一族）。
    const SYNTHETIC_BAD: &str =
        "fn zzz_probe(x: &str) -> String {\n    format!(\"tmux kill-session -t {x} 2>&1\")\n}\n";
    assert_eq!(
        bad_of(SYNTHETIC_BAD),
        vec!["zzz_probe".to_string()],
        "本条的尺子在一份**明摆着裸目标**的语料上都不红 —— 它此刻什么都没在守"
    );
    // ★ 正向自检：同一把尺子在一份**调了 `exact_target` 的**语料上必须不红。
    //   只有反向那一格的话，一把「恒红」的坏尺子也能过 —— 那不是尺子，是常量。
    const SYNTHETIC_OK: &str =
        "fn zzz_ok(x: &str) -> String {\n    let t = exact_target(x);\n    format!(\"tmux kill-session -t {t} 2>&1\")\n}\n";
    assert!(
        bad_of(SYNTHETIC_OK).is_empty(),
        "本条的尺子把一份**调了 `exact_target` 的**语料也判红了 —— 它恒红，不是在守"
    );
}

// ════════════════════════════════════════════════════════════════════════
// `K-R112`（09-13）：抓屏改走 `capture-pane` 帧。〔C4e · 第四波 4C〕抓屏整条迁到界面。
// ════════════════════════════════════════════════════════════════════════
//
// 这里原来住着 `KR112D2` 的两刀机检：「抓屏这条路上没有命令串」（`the_capture_path_asks_the_backend_instead_of_composing_a_shell_line`〔散文墓碑〕，
// 带一个认命令串形态的谓词 `tmux_shell_line_markers` 与它的活体夹具〔散文墓碑〕）与「本机不再是死胡同」
// （`the_local_capture_is_no_longer_a_dead_end`〔散文墓碑〕）。抓屏改由界面经通道直接问那台机器的后端之后，
// monitor 里**那条路本身不在了**（`capture_remote_pane` / `capture_via_backend`〔散文墓碑〕都删了），两刀各自的去处：
// ① 「没有第二份实现」⇒ 下面这条零命中（整棵 monitor 生产段）＋ `frame_query_tests` 那条「已迁的零发送点」；
// ② 「本机与远端同一条路、通道不在时两句话不同」⇒ `tests/frontend/ui/tmux-control.vitest.ts`（`<local>` 照样经通道问 · 两句话不同）。

/// ★★〔C4e · 第四波 4C〕**monitor 里抓屏一条路都不剩**（零命中 ＋ 正控）。
///
/// 守的要求：`设计/05 §14.3` 逐字「迁到通道之后，业务解释是不是**只有一个家**」——
/// 抓屏的解释今天只住 `src/frontend/ui/tmux-control.ts`；monitor 里再长出一条拼 shell 串抓屏的路，就是同一件事的第二份实现
/// （`K-R112` 删掉的那一形：`command -v tmux` 门控 ＋ 两个哨兵）。
/// 帧命令名 `"capture-pane"` 那一格由 `frame_query_tests::the_channeled_ops_are_sent_only_through_the_channel` 管
/// （`CHANNELED_ELSEWHERE` 那一行：monitor 生产段零字面量），本条管**shell 串那几种形态**。
/// 正控：同一份语料上认得出今天真在的 `tmux.rs` 那个转调壳（〔MIG-1 续〕原来认的是列会话那条的格式串常量，那一族搬进了后端）。
#[test]
fn the_monitor_has_no_capture_path_any_more() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut corpus = String::new();
    let mut files = 0usize;
    for (_, one_file) in guard_core::scan_tree_excluding(&root, &["rs"], &[]) {
        files += 1;
        corpus.push_str(&guard_core::strip_comment_lines(
            &guard_core::production_code(&one_file),
        ));
        corpus.push('\n');
    }
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
    // 〔MIG-1 续〕正控换锚：列会话那条只读调用（格式串常量）随解析搬进了后端 ⇒ 认今天真在的 Gate 1 对拍锚点。
    assert!(
        guard_core::contains_word(&corpus, "is_ccm_tmux_name"),
        "正控失败：同一份语料里认不出 `tmux.rs` 那个 Gate 2 转调壳 —— 上面的零命中不可信"
    );
}

// 〔C4e · 第四波 4C〕这里原来住着「本机杀会话 / 送键不许回落到 SSH」两条（`the_local_kill_never_falls_back_to_ssh`〔散文墓碑〕 /
//   `the_local_send_keys_never_falls_back_to_ssh`〔散文墓碑〕，P3 刀 2 · K-R56 · K-R72）：回潮闸（生产段里不许再有
//   `connect_and_exec_cmd`）＋「说真实原因」（对 `<local>` 不报「未找到远端配置」、本机与远端两句话不同）。
//   两条命令整条迁到界面之后：
//   ① 回潮闸 ⇒ `tmux_backend_gate_guard` 那两条改钉「monitor 生产段里一处破坏性 tmux 动词都没有」（界面那一侧结构上没有 SSH）；
//   ② 说真实原因 ⇒ `tests/frontend/ui/tmux-control.vitest.ts`「通道不在：本机与远端两句话不同」（结束会话 · 发按键各一遍）。

/// F01 回归：tmux `-t` 目标**必须**精确匹配（`'=<名>:'`），绝不留裸目标。
///
/// 删掉这条性质会让换号重启把 `/exit` 敲进**兄弟会话里还活着的 claude** 并 kill 它，
/// 而 UI 报告「已重启」。**尾冒号不能省**：`send-keys`/`capture-pane` 收 target-pane，
/// `=名`（无冒号）在那条路径上 rc=1 完全失效。
///
/// # 🔴 `K-R112`（09-13）：**牙换住址 —— 两侧各一份变一份，而这一条跟过去数**
///
/// `K-R72` 把产物侧人群从三个构造器缩到 `capture-pane` 一个；本件把最后那一个也走掉了，
/// 连同它的产地 `exact_target`。⇒ **monitor 侧今天没有这条性质可断**。
///
/// ⚠ 这时有两种写法，只有一种是诚实的：
/// · 把本条删掉 ⇒ 「精确匹配」从此**无人在数**（而它仍然承重）；
/// · 让本条**跟到新住址去数** ⇒ 就是下面这样。
/// 判的是后端那棵树（读文件，不跨 crate 调用）—— 同 `tmux_backend_gate_guard`
/// 那几条两树对拍的做法。
///
/// ⚠ **它买不到什么**：只证明那两处**调了** `exact_target`，不证明 `exact_target`
/// 自己产的形状对 —— 那由后端那棵树自己的判据钉（本条够不着它的运行期）。
#[test]
fn tmux_targets_use_exact_match() {
    // ① monitor 侧：一处裸目标都不许再有（本件之后这一侧连命令串都没有了）。
    let mine = guard_core::production_code(include_str!(
        "../../../../../src/frontend/shell/src/backend/control/tmux.rs"
    ));
    assert!(
        !mine.contains("-t {"),
        "monitor 的 `tmux.rs` 生产段又出现了 `-t {{…}}` —— 那条路已经收干净了"
    );
    // ①b [`exact_target`] 自己产的形状 —— 它今天零生产调用方，但**是后端那条
    //     跨轨对拍的锚点**（见它的头注），所以这几格照旧断。
    assert_eq!(exact_target("cc-x").unwrap(), "'=cc-x:'");
    assert_eq!(exact_target("proj_cc-2").unwrap(), "'=proj_cc-2:'");
    // glob 名即便漏进来也被引号原样包住（不脱出成 shell glob）。
    assert_eq!(exact_target("si*").unwrap(), "'=si*:'");
    // 含单引号的名字仍被正确转义（`shell_quote` 的 `'\''` 形态）。
    assert!(exact_target("a'b").unwrap().starts_with("'=a"));
    assert!(exact_target("a'b").unwrap().ends_with("b:'"));
    // Gate 1：空 target 必须被拒（`=:` 会被 tmux 解析成「当前会话」）。
    assert!(exact_target("").is_err(), "空 target 必须被 Gate 1 拒绝");
    // ② 那条性质的新住址：backend 侧抓屏与杀会话**都**过 `exact_target`。
    // 〔搬树 2026-09-17〕后端树从 `remote-daemon-proto/src/` 搬到 `<repo>/src/backend/`
    // ⇒ **中间那层 `src` 没了**。原来是 `.join("src/backend").join("src").join("control")`。
    let backend = crate::guard_support::backend_src_root().join("control");
    let mut checked = 0usize;
    for (file, why) in [("capture_pane.rs", "抓屏"), ("kill.rs", "杀会话")] {
        let p = backend.join(file);
        let raw = std::fs::read_to_string(&p)
            .unwrap_or_else(|e| panic!("读不到 {p:?}：{e} —— 本条的被测对象没了，它此刻在空转"));
        let prod = guard_core::production_code(&raw);
        assert!(
            prod.contains("exact_target("),
            "backend 的 `control/{file}`（{why}）生产段里没有 `exact_target(` —— \n\
                 「`-t <名>` 必须是 `=<名>:` 精确形态」这条性质**今天两棵树上都没人守了**：\n\
                 monitor 侧 `K-R112` 已把它走掉（那一处的墓碑在本文件里），\n\
                 而这一处正是它唯一的新住址。裸目标会走 tmux 的\n\
                 「精确→名字开头→glob」三级解析 —— `cc-abc12345` 会命中 `cc-abc12345-2`。"
        );
        checked += 1;
    }
    assert_eq!(checked, 2, "只核到 {checked} 处 —— 本断言在空转");
}

// 〔C4e · 第四波 4C〕这里原来住着「抓不到的五档分得开、认不出的码不许猜」（`the_five_capture_refusals_stay_apart`〔散文墓碑〕，
//   驱动 monitor 的 `describe_capture_refusal`〔散文墓碑〕）。那一份说法随抓屏迁到界面：同一条性质住
//   `tests/frontend/ui/tmux-control.vitest.ts`（码集合取自跨语言金样 —— 与后端 `REGISTRY` 那一块对拍过的同一份，不是手抄）。

// 〔MIG-1 续 · `99 §2.1 ⑬`〕列会话那一族的判据（解析四条 · 格式串真 TAB · `TMUX_LS_FMT` 双写点 · `tmux-list` 成品严格收 ·
//   K-R12 `J1` 下溢死值）随解析搬进后端：`tests/backend/observe/tmux_list_tests.rs`（格式串只剩后端一个家，双写点那条随之无所对拍）。
