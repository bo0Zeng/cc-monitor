//! F07（出口③ 早已交付）：**远端起会话主路的决策已经在 backend 渲染** —— 把它钉住。
//!
//! # 摸底结论
//!
//! F07 的题目是「远端起会话主路走 backend」。逐段量下来**决策那半已经切完了**：
//!
//! | 段 | 今天在哪 |
//! |---|---|
//! | 会话名 | F13 的铸名口（`mintTmuxName`，避让不可分离） |
//! | §34 三道门 | F03 + F04a 已搬进 backend `control/` |
//! | 内层载荷 | `backend::control::payload`（P4b） |
//! | ccm 调用行 | `backend::control::ccm_invocation`（P4b） |
//! | **生产切换** | ✅ `remote-launch-run.ts` 三处在调 `render_ccm_launch` / `render_launch_payload` |
//!
//! 剩下的**只有「删 TS 那两个渲染器」**，而那是 U8c-3 的题目、不是 F07 的
//! —— F07 要的是「走 backend」，不是「删旧的」。
//! 〔LR1 · U8c-3 前一半〕`ccm …` 调用行那一份（`launch-render-cli.ts`）已删，它的夹具换成
//! 「生产请求 ＋ 手写期望」（`src/launch-cli-golden.ts` 头注）。
//! 〔LR2 · U8c-3 后一半〕兜底那一族（`launch-render-fallback.ts` · `session-backend.ts` ·
//! `remote-launch.ts` 五个 builder）也删了：零生产调用，两份夹具（`payload-golden.json` ·
//! `tmux-outer-golden.json`）的左边同样换成手写期望。本文件原来管它的那几张表
//! （尺子A 处数表 · 尺子B 生产可达表 · 那份换人手续）随之删；「这一族不许回来」由
//! `tests/launch-no-shell-in-ts.vitest.ts` 管（`设计/90 §3` 条 1）。`设计/00 §2.5 ④` 的「只留 Rust 两份」到了。
//!
//! # ⚠ 摸底在 `src/doc/INVARIANTS.md §33b` 里抓到**两处过期陈述**
//!
//! **过期一**：那张表把 **U8c-2c-2 写成「待做」** —— 实测已交付
//! （两条 tauri 命令注册 + 生产 TS 三处在调 + `parity_ledger` 两条能力）。
//!
//! **过期二**：三问的答案① 写「**否** —— 全仓 `.call("launch")` 只有一处且在 `cfg(test)` 里」，
//! 实测**生产段有一处**（`backend_launch.rs`，U8a-2c-1 的 `backend_send_into`）⇒ 应为「**部分是**」。
//!
//! ⚠ **结论仍然对**（U8c-3 今天删不得：③ U12 未决 + attach 那格仍在 TS），**但依据过期了**。
//! 这是本工作区「**理由过期而结论仍对**」的第二次（F01 那次是四处「每 ~8s」）——
//! 最难发现的一类，因为**结论对，所以没人会去查理由**。
//!
//! # ★★ U8c-3〔08-14 复裁〕：结论第三次不变，而这次**依据变硬了一条**
//!
//! 08-14 逐条重量三问（读数在 `unified-backend/features/U8c-3-r2-…md`）：
//!
//! | 问 | 08-04 的答 | 08-14 实测 |
//! |---|---|---|
//! | ① 生产切到后端的 `launch` 了吗 | 部分是（`send-into` 一格） | **仍是「部分是」**：生产段 `create-or-attach` **0 处**（下面那条判据在量），`attach` 结构上不归后端（`control/launch.rs` 头注「本模块**不 attach**」） |
//! | ② attach 那条串归谁产 | 一半有答案 | **仍挡着，而且不止 attach**：`renderFallback` 的三格（tmux `create` / `send-into` / `attach`）全在 TS。⇒「只剩 attach 那一格」是把阻碍读窄了 |
//! | ③ daemonless 的远端还要不要能起会话 | **未决**（U12 待做） | **已决：要。** `U12` 那个**件**被 `C7` 关掉了，但 `C7` 裁的是「**本机**也要有后端进程」；而 `daemonless` 今天是**每台远端主机的用户开关**（`src/settings/machine-card.ts` 那个 checkbox「daemonless 降级读取（无需后端）」→ `RemoteHostConfig.daemonless`，生产段 7 个文件 31 处）⇒ 那种主机**存在**、且它的 `↗` 走纯 SSH（`launch_remote_terminal` 不经后端）⇒ 起会话只能靠 monitor 自己渲染整串 |
//!
//! ⇒ ③ 从**软障碍（未决，所以不敢删）变成硬障碍（已决为「要」，所以确定不能删）**。
//! **「件关掉了」不等于「约束消失了」** —— 这是本节第三次栽在同一形状上，
//! 前两次是「结论对所以没人查理由」，这次是「**件关了所以没人查约束**」。
//!
//! ⚠ 而 08-04 立的那条前提触发器**在它被造出来要报的那个方向上是瞎的**，
//! 见 `production_ts` 的头注 —— 量具本身是本轮真正修掉的东西。
//!
//! ⚠⚠ 上表 ① 那个「0 处」的**分母 08-29 换过一次**〔`K-P2` C 阶段第二拍〕：
//! 原来只数 `src/bridge/src` 那棵树，现在**同时数 `shared/ccm` 的生产段** ——
//! 因为 `K-P2 §0d`〔PM 08-29〕把接线路裁成了「`ccm` 直接问后端二进制」，
//! 而那条路整条落在那份 shell 脚本里，旧扫描面**够不到它**。
//! **读数仍然是 0，变的是分母。** 逐字理由在
//! `the_create_or_attach_mode_is_sent_only_by_the_ccm_container_path` 头注里那段量法沿革（〔LR2〕原判据改名改写，逐字的「第四次」那一节见 git 历史）。

fn repo_root() -> std::path::PathBuf {
    // 住址唯一源：`crate::guard_support`（头注写着 24 份副本怎么一起漂的）。
    crate::guard_support::repo_root()
}

fn read_ts(rel: &str) -> String {
    std::fs::read_to_string(repo_root().join(rel)).unwrap_or_else(|e| panic!("读不到 {rel}: {e}"))
}

/// 剥 TS 的生产段：整行 `//` / `*` / `/*` 注释 + 行尾 `//`。
///
/// ★★ **本轮真正修掉的东西是量具，不是结论。**
///
/// 08-04 立的依据一原式是
/// `fallback.contains("session-backend") || run.contains("session-backend")` ——
/// **整份文件的子串，含注释，而且是 `||`**。而 `remote-launch-run.ts` 的注释里
/// 逐字提了 4 次 `session-backend.ts`（那些注释干的正是「解释这一格为什么还在 TS」这件事）
/// ⇒ **把生产 import 与调用点删干净，那条依然绿**。
///
/// 它是**前提触发器**：整个价值就是「前提没了要主动红」。而它在那个方向上是瞎的 ——
/// 一条只会在「什么都没变」时说话的判据，与没有判据是同一件东西。
///
/// ⇒ 改成量**生产段里那个调用还在不在**。剥法与
/// `the_remote_launch_main_path_really_calls_the_backend_renderers` 共用一份
/// （原来那份是就地写的，两处各写一份就会漂）。
///
/// # 为什么不是直接调 `guard_core::strip_comment_lines`（`structural_scan` 那张表要的回答）
///
/// **整行那半就是它**（本函数只是转调）。多出来的只有**行尾 `//` 截断**一步 ——
/// 共享原语的头注逐字说明它**刻意不剥行尾**，理由是会砍坏 `"http://host"` 这类字面量。
/// 而本组判据必须剥行尾：F10 那次的教训逐字是「**行尾注释里的提及不算数**」，
/// 08-04 那份就地剥法正是为此才截的。
///
/// ⚠ `tool_registry.rs` 那条登记逐字写着「本文件今天没有 `://` 字面量所以没事，
/// **但那是运气不是设计**」。⇒ 这里不留运气：`the_ts_comment_stripper_actually_strips`
/// 对本组的**四份语料**逐个断言「不含 `://`」，运气变成读数。
fn production_ts(src: &str) -> String {
    guard_core::strip_comment_lines(src)
        .lines()
        .map(|l| match l.find("//") {
            Some(i) => &l[..i],
            None => l,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// 本组判据读的全部 TS 语料 —— `production_ts` 的适用面就是这张表。
const TS_CORPUS: &[&str] = &[
    "src/remote-launch-run.ts",
    "src/remote-config.ts",
    "src/settings/machine-card.ts",
    "src/remote-launch.ts",
    // 〔LR2〕原来这里的注释讲的是「TS 兜底路的消费者各自登记在哪」；那一族删了，
    // 这几份留在语料里只为一件事：`production_ts` 的行尾截断在它们上面安全（不含 `://`）。
    // 两份夹具用例表挪出了 `src/`（`设计/90 §3` 条 1：`src/**` 零 shell 串），住址跟着改。
    "tests/test-support/launch-payload-golden.ts",
    "tests/test-support/launch-tmux-outer-golden.ts",
];

/// ★ 量具自检：`production_ts` 真的在剥，而不是原样返回；且行尾截断在本组语料上安全。
///
/// **不用「剥完更短」当自检** —— guard-core 的头注逐字记着那条为什么不够
/// （光靠剥注释就能满足，与剥对没剥对无关）。这里直接喂一段字面量看输出。
#[test]
fn the_ts_comment_stripper_actually_strips() {
    assert_eq!(production_ts("// a\n * b\n/* c\nd // e\nf"), "\n\n\nd \nf");
    // 反向：没有注释的文本一个字都不许动。
    assert_eq!(
        production_ts("let x = 1;\nlet y = 2;"),
        "let x = 1;\nlet y = 2;"
    );
    // ★ 行尾截断的安全前提**量出来**，不靠运气：本组语料一条 `://` 都没有。
    let mark = format!(":{}", "//");
    for f in TS_CORPUS {
        let src = read_ts(f);
        assert!(
            src.len() > 3000,
            "{f} 只有 {} 字节 —— 语料读错了",
            src.len()
        );
        assert!(
            !src.contains(mark.as_str()),
            "{f} 里出现了 `{mark}` 字面量 —— 行尾 `//` 截断会把它砍成半行、造出假阴性。\n\
                 ⇒ 要么把那个字面量挪走，要么给本组换一份不截行尾的剥法（那时上面两条断言也要改）。"
        );
    }
}

/// ★ **生产接线钉**：主路真的调那两条 backend 渲染命令。
///
/// 一旦有人把它改回「TS 自己渲染」，本条红。〔LR2〕TS 那份兜底渲染器已删，回退要先把它写回来 ——
/// 那一步由 `tests/launch-no-shell-in-ts.vitest.ts`（`设计/90 §3` 条 1）挡着；本条管的是接线这一头。
#[test]
fn the_remote_launch_main_path_really_calls_the_backend_renderers() {
    let ts = read_ts("src/remote-launch-run.ts");
    assert!(
        ts.len() > 5000,
        "remote-launch-run.ts 只有 {} 字节，抽错了？",
        ts.len()
    );
    // 剥整行注释 + 行尾注释（F10 那次学到的：行尾注释里的提及不算数）。
    let prod = production_ts(&ts);
    for needle in [
        "commands.render_ccm_launch(",
        "commands.render_launch_payload(",
    ] {
        assert!(
            prod.contains(needle),
            "`remote-launch-run.ts` 的生产段里找不到 `{needle}` ——\n\
                 远端起会话主路不再走 backend 渲染了。"
        );
    }
}

/// ★ **`create-or-attach` 那一格两棵树各自的口径**（原 U8c-3「前提触发器」剩下的那一半）。
///
/// 〔LR2〕本条原名 `the_two_reasons_u8c3_cannot_delete_the_ts_renderer_still_hold`〔散文墓碑〕，
/// 立它时要回答「U8c-3（删 TS 渲染器）为什么今天删不得」，依据有两条：
/// 依据一（生产主路仍调 TS 兜底 · 兜底仍问 TS 座要外层 tmux 命令 · 两个夹具发生器还在调它）
/// 与依据二（**起会话那格**有没有切到后端）。依据一在步 22b·B 翻成回潮闸、在 LR2 随那一族删掉而整条没了
/// （「不许回来」今天住 `tests/launch-no-shell-in-ts.vitest.ts`）；依据二与删不删 TS 无关，一个字没动：
///
/// - **Rust 生产段**（`src/bridge/src/**.rs`）**不许**发 `create-or-attach` —— monitor 侧 `launch` 那条
///   `.call` 只发 `send-into`；
/// - **`src/backend/control/ccm/`** **必须**发 —— `ccm` 容器路接在后端那条一次性口上（`K-P2` `D3`）。
///
/// 两棵树口径相反不是疏漏，是两件不同的事（历次量法订正见 git 历史：`K-P2` 08-29 补第二棵树、
/// `K-R48` 09-11 换住址、F04c 把 `.call("launch")` 处数这个过期代理换成直接量 mode）。
#[test]
fn the_create_or_attach_mode_is_sent_only_by_the_ccm_container_path() {
    // 依据二：**起会话那格**有没有切过去 —— 直接量「生产段发不发 `create-or-attach`」。
    // **运行时拼，免得命中本文件自己的说明。**
    let mode = format!("\"create-or-{}\"", "attach");
    let mut hits: Vec<String> = Vec::new();
    let mut stack = vec![std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src")];
    let mut scanned = 0usize;
    while let Some(d) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else {
            continue;
        };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
                continue;
            }
            if p.extension().and_then(|x| x.to_str()) != Some("rs") {
                continue;
            }
            if p.file_name().is_some_and(|n| n == "launch_wire.rs") {
                continue; // 本文件的说明里逐字写着那个串
            }
            scanned += 1;
            let src = guard_core::production_code(&std::fs::read_to_string(&p).unwrap_or_default());
            if src.contains(mode.as_str()) {
                hits.push(p.file_name().unwrap().to_string_lossy().to_string());
            }
        }
    }
    // ★ 抽取器自检：扫描面没缩水（否则下面那条零命中地绿）。
    assert!(
        scanned >= 50,
        "只扫到 {scanned} 个 .rs —— 遍历坏了，下面那条断言会零命中地绿"
    );
    // ★★ 第二棵树〔`K-P2` 08-29 立，`K-R48` 第二拍 09-11 换住址〕。
    //    它原来是 `shared/ccm`（那份 bash 脚本），理由是「`§0d` 裁定的接线路落在那里，
    //    而上面那棵树够不到它」。〔用@09-11 `K33`〕脚本删了 ⇒ 换成
    //    `src/backend/control/ccm/`：**接线路还在那条边上，只是换了语言**。
    //    ⚠ needle 仍用**带边界的词**（不是 Rust 那边的带引号字面量）：
    //    它在两侧可能长在字面量里、也可能长在 JSON 串里，带边界两种形态都收得到。
    let word = format!("create-or-{}", "attach");
    let ccm_dir = crate::guard_support::repo_root().join("src/backend/control/ccm");
    let ccm: String = ["mod.rs", "argv.rs", "plan.rs"]
        .iter()
        .map(|f| {
            guard_core::production_code(
                &std::fs::read_to_string(ccm_dir.join(f))
                    .unwrap_or_else(|e| panic!("读不到 control/ccm/{f}：{e}")),
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    // ★ 抽取器自检（这一棵树自己的）：剥注释器没把代码一起剥掉。
    //   地板 300 行是从 `shared/ccm` 那一版逐字沿用的（那边生产段 536 行 / 全文 1258）；
    //   现打这三份剥完远在其上。
    assert!(
        ccm.lines().count() >= 300,
        "`control/ccm/` 三份的生产段只剩 {} 行 —— 剥注释器把代码也剥了？下面那条会零命中地绿",
        ccm.lines().count()
    );
    // ★★ 〔`K-P2` `D` 阶段第三拍 09-03〕**这一半本拍翻了面。**
    //
    // 它原来与 Rust 那棵树同判：「**两棵树都不许**出现 `create-or-attach`」。
    // `K-P2` `D3` 把 `shared/ccm` 的 `--tmux` 接到了后端那条一次性口上
    //（旧 bash 的 `launch_via_backend` 〔散文墓碑〕 发 `mode=create-or-attach`）⇒ **这一半当场红了，红得对**。
    //
    // ⇒ 翻成正向：`shared/ccm` 从「不许有」变成「**必须有**」。
    // **别把它删掉**：删掉之后「起会话又退回本机 tmux 直起」就没有任何东西会说话。
    //
    // 〔LR2〕这里原来接着写「本条的结论不变：删 TS 渲染器的前置仍然不成立」，理由是另两条依据
    //   （生产主路调 TS 兜底 · 兜底问 TS 座要外层命令）与后来的「消费者表」。那几条前提先后没了
    //   （步 22b·B 生产切走 · LR2 把那一族删了），本条只剩下面这一件：「起会话那格」在两棵树上各自什么样。
    // ⚠ 而 **Rust 那棵树仍然必须是零** —— 〔C4e〕monitor 侧那条 `.call("launch")` 连同它的发送端迁到界面删了
    //   （界面经 `src/tmux-control.ts` 只发 `send-into`，`tmux_backend_gate_guard` 那条「只经一处」钉着）。
    //   **两棵树本拍起口径不同，这不是疏漏，是两件不同的事。**
    assert!(
        hits.is_empty(),
        "**Rust 生产段**开始发 `create-or-attach` 了（{hits:?}）—— **这多半是好事**：\n\
             monitor 侧「起会话」那格可能也切到后端了 ⇒ `INVARIANTS §33b` 三问的答案① 又变了，\n\
             回 `INVARIANTS §33b` 把那一问改过来。\n\
             ⚠ 同轮还要回 `K-P2`：`KP2A②` 的棘轮要抬、`KP2C` 的退路要登记、\n\
             `KP2D` 的通道 A/B 冲突必须已经解掉。\n\
             ⚠ `control/ccm/` **不在本断言的人群里**（它在下面单判，`K-P2` `D3` 已经切过去了）。"
    );
    assert!(
        guard_core::contains_word(&ccm, &word),
        "`control/ccm/` 的生产段**不再发** `create-or-attach` 了 —— 起会话退回本机 tmux 直起？\n\
             `K-P2` `D3`（09-03）把它接到了后端那条一次性口上；`K-R48`（09-11）之后\n\
             那条口住进了同一个进程（`control::launch::parse_request` 那道门）。\n\
             ⇒ 真要退回来，请连同后端侧那条\n\
             `the_container_launch_goes_through_the_one_door_with_every_field_intact`\n\
             一起撤，并回 `K-P2` 说明为什么。"
    );
}

// ═══════════════════════════════════════════════════════════════════════
// `K-R89` `KR89D4`：**那条逐字节对拍不许变成自洽夹具，也不许被连量具一起砍**
// ═══════════════════════════════════════════════════════════════════════

/// 对拍那条判据的源码。**编译期嵌进来** —— 文件被删/改名 ⇒ **编译失败**，
/// 不是运行时静默跳过。（同 `launch_payload_parity.rs` 自己对夹具与 TS 那一半的做法。）
/// 🔴 〔搬树 2026-09-18 · `设计/16 §5.4b` 纪律 3〕**语料跟着判据搬**：
/// 那条逐字节对拍是一条 `#[test]`，剖分把它从
/// `src/bridge/src/backend/control/launch_payload_parity.rs` 搬到了
/// `tests/bridge/backend/control/launch_payload_parity_tests.rs`。
/// 指着生产段那一份的话，下面三条量的是一个**已经不含那条判据**的文件 ——
/// 而它们的反空真（「对拍那条判据不在了」）**按设计当场响了**，没有零命中地绿。
/// ⚠ **两半拼起来**：剖分把那条 `#[test]` 搬去了 `_tests.rs`，而它依赖的
/// 那份入库夹具（`fixtures/payload-golden.json`）与 `TS_HALF` 仍然住生产段
/// ⇒ 本条第 ①②③④ 格要的东西**跨在剖分线两侧**，少喂一边就有一半在空转。
const PARITY_SRC: &str = concat!(
    include_str!("../../../../src/bridge/src/backend/control/launch_payload_parity.rs"),
    include_str!("launch_payload_parity_tests.rs")
);

/// 对拍**左边**那个真相源的源码。同上，编译期嵌。
const GOLDEN_SRC: &str = include_str!("../../../test-support/launch-payload-golden.ts");

/// 对拍那条判据**被绕过**的三种形状。**用变体，不用一句话** ——
/// 棘轮要断言「这一刀触发的是**哪一格**」，按格认不按文字认
/// （`brief` 第 12b 条：有些诊断里嵌着现算的数，按字面比会把同一格算成找不到）。
#[derive(Debug, PartialEq, Eq)]
enum ParityBypass {
    /// **`K-R89` 那一形（写死）**：左边不再取自入库夹具的 `payload` 字段。
    LeftNoLongerFromTheFixture,
    /// **`K-R105` 那一形（同名遮蔽）**：原行一字不动，前面**再绑一次**同名的
    /// ⇒ 比较退化成 `got != got`。事实是「一边只许被绑**一次**」，
    /// 所以这一格的判定单位必须是**计数**。
    ASideIsBoundMoreThanOnce { side: &'static str, times: usize },
    /// 右边不再跑生产命令，改成自己重搭一份 spec 去比。
    RightNoLongerRunsTheProductionCommand,
}

impl ParityBypass {
    /// 判据红的时候给人看的那段话。**只在这里写一份**（判据与棘轮共用判定，
    /// 诊断也就只该有一个家）。
    fn why(&self) -> String {
        match self {
            ParityBypass::LeftNoLongerFromTheFixture => {
                "对拍的**左边**不再取自入库夹具的 `payload` 字段了 ——\n\
                     若它改成了「Rust 现场再渲染一次」，这条对拍就成了自洽夹具（`U7-4` 的病根），\n\
                     逐字禁令住 `tests/test-support/launch-payload-golden.ts` 的头注。\n\
                     ★ 这是 `K-R89` 09-13 逮到的那一形（`want` 被写死成 `got`）。"
                    .to_string()
            }
            ParityBypass::ASideIsBoundMoreThanOnce { side, times } => format!(
                "对拍函数体里 `{side}` 出现了 {times} 次（只许 1 次）。\n\
                     **两次 = 有人用同名遮蔽把这条对拍的一边换掉了** —— 「那一行还在」\n\
                     那一格对这一形是瞎的（原来那一行还在，而它已经不参与比较了）。\n\
                     ★ 这是 `K-R105` 09-13 逮到的那一形，当时 13 格全绿一声不吭。\n\
                     ⚠ 真要重构变量名，把本条一起改；但先想清楚：改完之后，\n\
                     「左边取自入库夹具、右边跑生产命令」这两件事还有谁在看着。"
            ),
            ParityBypass::RightNoLongerRunsTheProductionCommand => {
                "对拍的**右边**不再跑生产命令 `render_launch_payload` 了 ——\n\
                     自己重搭一份 spec 来比，比的就不是上线那条路（复盘实测过：那时\n\
                     「清空 `nested_env`」那个变异全绿）。"
                    .to_string()
            }
        }
    }
}

/// 对拍函数体的**切法**（带长度地板）—— 判据与棘轮共用，两处各切一遍就会漂。
fn the_parity_fn_body(src: &str, at: usize) -> Result<&str, String> {
    let rest = &src[at..];
    // 🔴 〔搬树 2026-09-18〕收尾针从 `"\n    }\n"` 改成 `"\n}\n"`：那条对拍判据搬出
    //    `mod tests {}` 之后是**文件顶层**的 `fn`，缩进整整少了一级。
    //    按缩进认边界的针会随搬树静默失配（`设计/16 §5.4b`）—— 旧针会切在 `for` 循环
    //    那个 `    }` 上，把函数体截短一大截；下面那条 `<= 300` 的地板是它的反空真。
    //
    // 🔴 〔步 7c 2026-09-19〕**再往前一步：不用字符串针，按「行等于 `}`」认。**
    //    `"\n}\n"` 是对的，但它仍然是一根**位置针**（它假设收尾 `}` 正好在列 0），
    //    而「剖分退一层缩进」这件事在本仓已经发生过两轮。按行比不携带列假设。
    //    ⚠ 顺带一条真实收益：本轮 `needle_anchor_registry` 把两棵测试树收进语料之后，
    //      本文件的裸 `.find("` 才第一次被数到（此前整棵 `tests/bridge/backend/` 不在
    //      任何扫描面里）—— 那条递减棘轮因此凭空多了一格余量。改掉针 = 把余量还回去，
    //      而不是把上限从 8 调到 9（那条判据的文案逐字禁止后者）。
    let end = {
        let mut e = rest.len();
        let mut off = 0usize;
        for (i, l) in rest.split('\n').enumerate() {
            if i > 0 && l == "}" {
                e = off + 1; // 含那一行那个 `}`
                break;
            }
            off += l.len() + 1;
        }
        e
    };
    let body = &rest[..end];
    if body.len() <= 300 {
        return Err(format!(
            "取到的对拍函数体只有 {} 字节 —— 切法坏了，判定会零命中地绿",
            body.len()
        ));
    }
    Ok(body)
}

/// 🔴🔴 **「那条逐字节对拍的两条边还独立吗」的判定本体**〔`K-R106` 09-13〕。
///
/// # 它为什么被抽出来（这是本轮加的唯一一件事，理由值得写清楚）
///
/// `K-R89`（左边被**写死**成右边）与 `K-R105`（**同名遮蔽**，原行一字不动）
/// 连着两次栽在同一件事上：**对拍死了，而门禁全绿。**
/// `K-R105` 的解药是把匹配单位从「有没有」换成「**有几处**」——
/// 🔴 **而那副解药本身今天没有任何判据在守**（`K-R106` 实测：把那段计数整块拿掉，
/// 全量 `cargo` 一条都不红）。⇒ 判定抽成这一份，
/// 让 [`the_parity_guard_counts_bindings_it_does_not_merely_look_for_them`]
/// 拿**活体变异语料**去驱动它：谁把 ② 那一格退回子串存在性，那条棘轮当场红。
///
/// # 三格各钉什么（顺序即优先级：先看左边在不在，再看它被绑了几次）
///
/// | 格 | 钉的事实 | 判定单位 |
/// |---|---|---|
/// | ① | 左边取自**入库夹具**的 `payload` 字段 | 那一行在不在（`K-R89` 那一形是**替换**，行会消失）|
/// | ② | 每一边**只许被绑一次** | 🔴 **计数**（`K-R105` 那一形原行不动，只看「在不在」是瞎的）|
/// | ③ | 右边跑**生产命令本体** | 那一处调用在不在 |
///
/// # ⚠ 它买不到什么（如实写）
///
/// - **射程收到了函数体内**（`K-R106` 前 ① ③ 读的是整份 `PARITY_SRC`）。
///   这是**收紧**：文件别处提一句同样的话不再能替它兑现。
/// - **判定仍是文本**。它防的是**顺手**：让一条挡路的对拍过去，最省事的两种写法
///   （写死 · 遮蔽）现在都会红。换一套等价写法（改变量名、把比较搬进 helper）躲得过 ——
///   **那是决心，不是顺手**，本条不声称挡得住。
/// - **有人把判据里那句调用删掉、就地再写一个更弱的检查**，本条看不见
///   （棘轮驱动的是这个函数，不是那条判据的调用点）。⇒ 那一步会让本函数变成死代码
///   （编译器会喊 `dead_code`），但**没有判据会红**。登记，不假装钉住了。
fn the_two_sides_are_still_independent(body: &str) -> Result<(), ParityBypass> {
    // ① 左边那一行还在（`K-R89` 那一形是替换 ⇒ 行会消失）。
    if !body.contains("let want = c.payload") {
        return Err(ParityBypass::LeftNoLongerFromTheFixture);
    }
    // ② 🔴 **计数，不是存在性。** 这一格就是 `K-R105` 那副解药，棘轮守的正是它。
    for side in ["let want", "let got"] {
        let times = body.matches(side).count();
        if times != 1 {
            return Err(ParityBypass::ASideIsBoundMoreThanOnce { side, times });
        }
    }
    // ③ 右边仍然跑生产命令本体。
    if !body.contains("render_launch_payload(c.req)") {
        return Err(ParityBypass::RightNoLongerRunsTheProductionCommand);
    }
    Ok(())
}

/// 🔴🔴🔴 **反向棘轮**〔`K-R106` `KR106D4` ①，PM 09-13 裁「先问能不能把闸补上」〕：
/// **那条对拍守卫的匹配单位必须是「有几处」，不许退回「有没有」。**
///
/// # 它为什么非有不可（这是一条量出来的缺口，不是补全癖）
///
/// `K-R89`（写死）与 `K-R105`（同名遮蔽）连着两次栽在「对拍死了而门禁全绿」上。
/// `K-R105` 开的药是把 [`the_two_sides_are_still_independent`] 的 ② 那一格
/// 从存在性换成计数。**`K-R106` 现打：把那段计数整块拿掉，全量 `cargo` 一条都不红**
/// —— **解药本身没人守**。本条就是那个守。
///
/// # 它怎么钉的：**不看源码里有没有某个词，而是拿活体变异去驱动判定本体**
///
/// 那种「文件里出现过 `.count()` 吗」的写法，正是本族（`needle_anchor_registry`：
/// **匹配单位比事实小**）自己要治的病的同形 —— 换个等价写法就绿，而且它证不出行为。
/// ⇒ 本条**从真语料现造三份变异体**（不是手写夹具：手写的会与真语料漂开），
/// 逐份断言判定本体**指名点姓地**报出哪一格：
///
/// | 变异体 | 复刻的是哪一刀 | 必须报 |
/// |---|---|---|
/// | 左边被写死成右边 | `K-R89` `D4-selffix` | ① `LeftNoLongerFromTheFixture` |
/// | 原行不动、前面再绑一次 | `K-R105` `D4-selffix` | ② `ASideIsBoundMoreThanOnce`（**只有计数看得见**）|
/// | 右边改成自己重搭 | 复盘那一刀 | ③ `RightNoLongerRunsTheProductionCommand` |
///
/// ⇒ 谁把 ② 退回 `body.contains("let want")`，第二份变异体当场变成 `Ok`，**本条红**。
///
/// # ⚠ 诚实边界
///
/// - 三份变异体都**从真语料现造**，造之前逐处断言锚点**恰好命中一次**（fail-closed）——
///   锚点漂了就当场炸，不许「零命中地绿」。
/// - 本条守的是**判定本体**，不是那条判据的**调用点**：有人把调用删掉、就地写一个更弱的，
///   本条看不见（那会让判定本体变成死代码，编译器喊 `dead_code`，但没有判据红）。
///   **登记在案**，同 [`the_two_sides_are_still_independent`] 头注最后一条。
#[test]
fn the_parity_guard_counts_bindings_it_does_not_merely_look_for_them() {
    let parity_fn = format!(
        "fn rust_payload_rendering_matches_the_{}",
        "typescript_golden_byte_for_byte"
    );
    let at = PARITY_SRC
        .find(parity_fn.as_str())
        .expect("对拍那条判据不在了 —— 本条此刻在量一个不存在的东西");
    let clean = the_parity_fn_body(PARITY_SRC, at).unwrap_or_else(|e| panic!("{e}"));

    // ── 反空真：干净语料**必须过**。它要是本来就不过，下面三条一律作废 ──────
    assert_eq!(
        the_two_sides_are_still_independent(clean),
        Ok(()),
        "盘上那份对拍语料自己就过不了判定 —— 那么下面三份变异体的「红」证明不了任何事"
    );

    // ── 现造三份变异体。造之前逐处 fail-closed 断言锚点恰好命中一次 ──────────
    let mutate = |anchor: &str, into: &str| -> String {
        let n = clean.matches(anchor).count();
        assert_eq!(
            n, 1,
            "造变异体的锚点 {anchor:?} 在真语料里命中 {n} 次（要 1 次）——\n\
                 锚点漂了，本条**不许**继续跑：一个打不中的变异会让下面那条断言\n\
                 变成「判定对一份没变的语料说 Err」，那是假读数。"
        );
        clean.replace(anchor, into)
    };

    // ① `K-R89` 那一刀：把左边**写死**成右边。
    let hardcoded = mutate("let want = c.payload.clone();", "let want = got.clone();");
    assert_eq!(
        the_two_sides_are_still_independent(&hardcoded),
        Err(ParityBypass::LeftNoLongerFromTheFixture),
        "\n★ 判定没认出 `K-R89` 那一形（左边被写死成右边）——\n\
             那一刀之后对拍在比 `x == x`，而它照样全绿。"
    );

    // ② 🔴 `K-R105` 那一刀：**原行一字不动**，前面再绑一次同名的。
    //    这一份就是本棘轮的正题：**只有「计数」看得见它。**
    let shadowed = mutate(
        // 〔搬树 2026-09-18〕锚点缩进少一级：那条判据搬出 `mod tests {}` 之后
        // 函数体整体左移 4 格。**锚点内容一个字没变。**
        "        if got != want {",
        "        let want = got.clone();\n        if got != want {",
    );
    assert!(
        shadowed.contains("let want = c.payload.clone();"),
        "变异体没造对：`K-R105` 那一形的要害是**原行留着**，留不住就退化成 ① 那一形了"
    );
    assert_eq!(
        the_two_sides_are_still_independent(&shadowed),
        Err(ParityBypass::ASideIsBoundMoreThanOnce {
            side: "let want",
            times: 2
        }),
        "\n★★★ **匹配单位被退回「子串存在性」了。**\n\
             这一份变异体的形状是：`if got != want {{` 前面插一行 `let want = got.clone();`，\n\
             **原来那一行一个字节没动** ⇒ 对拍变成 `got != got`。\n\
             「那一行还在吗」对它是瞎的 —— 事实是「一边只许被绑**一次**」，\n\
             所以 `the_two_sides_are_still_independent` 的 ② 那一格**必须数，不许只看有没有**。\n\
             ⚠ `K-R105` 09-13 实测过这一刀：加固之前它让 **13 格全绿一声不吭**。\n\
             ⚠ 这条棘轮是 `KR106D4` ① 的落点（PM 09-13 裁「先问能不能把闸补上」）——\n\
             **不许把它调宽让今天好过**；真要改判定，先回来说明谁来接这一格。"
    );

    // ③ 右边改成自己重搭一份，不跑生产命令。
    let selfmade = mutate(
        "render_launch_payload(c.req)",
        "payload_rebuilt_by_hand(c.req)",
    );
    assert_eq!(
        the_two_sides_are_still_independent(&selfmade),
        Err(ParityBypass::RightNoLongerRunsTheProductionCommand),
        "\n★ 判定没认出「右边不跑生产命令」那一形 —— 那时比的不是上线那条路。"
    );
}

/// 🔴🔴 `KR89D4`：**删得动前端渲染器的那一天，别把量它的尺子一起删了。**
///
/// # 它守的是什么形状
///
/// `tests/test-support/launch-payload-golden.ts` 是 Rust 那条逐字节对拍的**左边**：
/// 〔LR2〕它原来调真的 TS 兜底渲染器产 `fixtures/payload-golden.json`，那份渲染器删了之后
/// 落盘的是用例表里的**手写期望**，
/// Rust 侧 [`super::super::launch_payload_parity`] 拿自己渲染的结果与**入库的那份**比。
/// 两侧都不在运行时去调对方 —— 那正是 `U7-4` 那种**自洽夹具**（夹具由被测代码现场产出、
/// 永远自己对自己）被挡住的地方。
///
/// `K-R89` 的题目是「删前端那条渲染路」。**删它最省事的做法恰好是最坏的那个**：
/// 把对拍一起删掉，或者把左边换成「Rust 自己再算一遍」—— 那时读数照样全绿，
/// 而全绿的原因是没有人再在比了。⇒ 本条把这三件事各钉一条。
///
/// # ⚠ 诚实边界（两侧都写出来）
///
/// - **本条与被守的那份住在两个文件里** ⇒ **两个一起删仍然静默**。
///   （〔LR2〕原来这里点着另一条同形的判据 —— 那份「换人手续」的看守，随 TS 兜底一族删了。）
///   买到的是「别的都不动、只删对拍」那一刀会红，不是「谁也删不掉」。
/// - **本条按文本判**（`include_str!` 进来的源码）⇒ 换个等价写法躲得过。
///   它防的是**顺手**（删一条挡路的判据），不防**决心**。
///   ⚠ 〔`K-R105` 09-13〕**这句诚实边界曾经把一形放错了边**：它写着「防顺手不防决心」，
///   而**同名遮蔽恰恰是顺手**（让一条挡路的对拍过去，最省事的写法就是它）。
///   ⇒ 那一格的判定单位换成了**计数**。
/// - **本条不判夹具的内容对不对** —— 那是对拍自己那三条的事
///   （用例数 `EXPECT_CASES` · 逐条字节比 · `nestedEnvKeys` 集合相等）。
///
/// # 🔴 `K-R106` 09-13：**判定本体搬出去了，而且它自己有人守了**
///
/// ② 那一格（左边取自夹具 · 两边各只绑一次 · 右边跑生产命令）此前**就地写在本函数体里**
/// ⇒ 它自己不可被驱动、也没有任何判据在守。现打实测：把那段计数整块拿掉，
/// 全量 `cargo` **一条都不红** —— `K-R105` 那副解药本身是裸的。
/// ⇒ 判定搬进 [`the_two_sides_are_still_independent`]（**本条与棘轮共用那一份**），
/// 反向棘轮 [`the_parity_guard_counts_bindings_it_does_not_merely_look_for_them`]
/// 拿**从真语料现造的三份变异体**驱动它：
/// 谁把「有几处」退回「有没有」，那条棘轮当场红。
#[test]
fn the_byte_for_byte_parity_still_has_two_independent_sides() {
    // 反空真：语料真的读进来了（`include_str!` 空文件也能编过）。
    assert!(
        PARITY_SRC.len() > 3_000 && GOLDEN_SRC.len() > 2_000,
        "对拍语料读进来只有 {} / {} 字节 —— 本条此刻在空转",
        PARITY_SRC.len(),
        GOLDEN_SRC.len()
    );

    // ① **对拍那条判据还在，而且还挂着 `#[test]`**。
    //    needle 现拼：本文件里不留完整串（`6g` 那族：断言用的子串别取自被断言物的名字）。
    //
    //    ⚠ **两半都要**（这一条是死值验逼出来的）：只钉函数名时，
    //    「把 `#[test]` 摘成 `#[allow(dead_code)]`」那一刀**活了下来** ——
    //    函数原样在盘上、名字也在，而它再也不跑了。⇒ 加钉「紧挨着它的上一行是 `#[test]`」。
    let parity_fn = format!(
        "fn rust_payload_rendering_matches_the_{}",
        "typescript_golden_byte_for_byte"
    );
    let at = PARITY_SRC.find(parity_fn.as_str()).unwrap_or_else(|| {
        panic!(
            "`launch_payload_parity.rs` 里那条逐字节对拍没了 ——\n\
                 **删前端渲染器最省事的做法就是把量它的尺子一起删掉**（纪律 ⑱）。\n\
                 真要退役它，先回 `K-R89`/`U8c-3` 说明「谁来证明两侧一致」，别自批。"
        )
    });
    let test_attr = format!("#[{}]", "test");
    assert!(
        PARITY_SRC[..at].trim_end().ends_with(test_attr.as_str()),
        "那条逐字节对拍还在盘上，但它**不再是一条会跑的判据**了（紧挨着它的上一行不是 \
             `{test_attr}`）——\n\
             「留着函数、摘掉注册」与「删掉它」在读数上一模一样，而前者更难被发现。"
    );

    // ② **左边仍然是「入库的那份」，右边仍然是「生产命令本体」，而且两边各只被绑一次。**
    //    判定本体不在这里 —— 它住 [`the_two_sides_are_still_independent`]，
    //    **判据与棘轮共用那一份**（`K-R106`；理由住那个函数的头注）。
    let body = the_parity_fn_body(PARITY_SRC, at).unwrap_or_else(|e| panic!("{e}"));
    if let Err(bypass) = the_two_sides_are_still_independent(body) {
        panic!("{}", bypass.why());
    }
    // ③ **左边不许在运行时去调 TS**（头注逐字禁掉的那条捷径的机械形态）。
    for forbidden in ["Command::new", "process::Command"] {
        assert!(
            !PARITY_SRC.contains(forbidden),
            "`launch_payload_parity.rs` 里出现了 `{forbidden}` ——\n\
                 那是「让 Rust 侧去调 TS 现场生成」的形状，而 `tests/test-support/launch-payload-golden.ts`\n\
                 的头注逐字禁掉它：「不能让 Rust 侧去调 TS 现场生成（那就成了自洽夹具）」。\n\
                 夹具必须**入库**，两侧各自与它比。"
        );
    }
    // ④ 夹具仍然是**入库的一份文件**（编译期嵌），不是运行时算出来的。
    // ⚠ 前缀现拼：写成整串字面量的话，本文件自己就多出一处
    //   `cross_half_edge_registry` 抽不出路径的 `include_*!` 调用
    //   （它按「动词 + `(`」计数，而这里下一个字符是转义引号）⇒ 那条登记判据当场红。
    //   〔09-13 实打过一次：`every_non_literal_include_is_registered_with_a_reason` FAILED〕
    let fixture_include = format!("{}_str!(\"fixtures/payload-golden.json\")", "include");
    assert!(
        PARITY_SRC.contains(fixture_include.as_str()),
        "对拍不再 `include_str!` 那份入库夹具了 —— 夹具一旦不入库，\n\
             「夹具陈旧」与「两侧一致」就再也分不开。"
    );

    // ⑤ **左边那个真相源本身**〔LR2 翻面〕：原来要求它「调真的 TS 兜底渲染器」（另一种语言的独立实现）。
    //    那份渲染器零生产调用、按 `设计/00 §2.5 ④` 删了 ⇒ 左边换成用例表里的**手写期望**（同 LR1 对
    //    `cli-golden.json` 的做法）。今天要钉的是：落盘的 `payload` 取自用例表的字面量，
    //    **不是**任何渲染器现算的（那样左右两侧就同源了 —— 恒等两侧同源会恒真）。
    assert!(
        GOLDEN_SRC.contains("payload: c.payload,"),
        "`launch-payload-golden.ts` 落盘的 `payload` 不再取自用例表的手写期望了 ——\n\
             若它改成了现场调某个渲染器，左右两侧就可能同源（尤其是调回 Rust 那一份）。"
    );
    // 只看生产段（剥注释）：头注里点名那条 Rust 命令是在解释对拍，不是在调它。
    let golden_code = production_ts(GOLDEN_SRC);
    for renderer_import in ["launch-render-", "render_launch_payload", "renderLaunch"] {
        assert!(
            !golden_code.contains(renderer_import),
            "`launch-payload-golden.ts` 里出现了 `{renderer_import}` —— 夹具发生器又接上了某个渲染器，\n\
                 左边就不再是手写期望。"
        );
    }
    // ⑥ 那条逐字禁令本身还在盘上（`K-R89` 的题面逐字点名它）。
    assert!(
        GOLDEN_SRC.contains("不能让 Rust 侧去调 TS 现场生成"),
        "`launch-payload-golden.ts` 头注里那句逐字禁令被删了 ——\n\
             `KR89D4` 逐字：删了它，下一个人不会知道这条捷径为什么不许走。"
    );
}

// ═════════════════════════════════════════════════════════════════════════════
// `设计/00 §2.5 ④`：**盘上还剩几份渲染实现** —— 给那个数一个住址
// ═════════════════════════════════════════════════════════════════════════════
//
// # 那个数本来没有家
//
// `设计/00 §2.5 ④` 逐字「**5 个渲染实现 → 2 个（Rust CLI + Rust 载荷）**」——
// 而「5」在盘上**一处都没有**：它是散文里的一个数，没有判据钉着，
// 也没有写清它是按什么口径数的。⇒ 与 `K-R105` 那两把尺子被混读是同一个形状的病，
// 只是这次连尺子都还没有。本节把它落成：**口径写死 + 逐份点名 + 恒等 + 反向闭合。**
//
// # 口径（不写清口径的数就是半句假话）
//
// 数的是 **monitor 这一侧**（`src/*.ts` ＋ `src/bridge/src/**.rs`）
// 「能产出**起一个会话的那条 shell 串**（或它的一整层）」的实现。
//
// ⚠ **`src/backend/` 那棵树不在人群里**，这是口径不是遗漏：那是**后端二进制自己**的
// CLI 面（`control/ccm/`，跑在远端那台机器上的 `ccm` 命令），它是这条路的**被调方**，
// 不是 monitor 侧的第 6 个副本。把它数进来，「5 → 2」这个目标本身就无从谈起。
//
// ⚠ 〔LR2〕原来这里还有一句口径注脚：`remote-launch.ts` 那 5 个 builder 不算 5 份（它们只是调同一个
// TS 兜底渲染器的 5 个调用点）。那一族连同它们调的那一份一起删了，注脚随之没了用处。

/// `(仓相对路径, 这一份的入口符号, 它是什么, 它今天站在哪)`。
///
/// 🔴 **条数写成恒等**（[`the_launch_renderers_on_disk_are_exactly_these`]）：
/// 地板在「变少」方向是瞎的，而本表整个存在的理由就是**看着它变少**。
/// 加一份、删一份都必须回来改这张表 —— 改的时候人会看见 `LAUNCH_RENDERER_TARGET`。
// 🔴 〔LR1 · U8c-3 前一半〕**5 → 4**：原第一行 `("src/launch-render-cli.ts", "tryRenderCli",
// "TS · ccm 调用行", …)` 删了 —— 零生产调用，最后只剩「产 `cli-golden.json` 的 `out`」一个用途，
// 那一格换成了用例表里的手写期望（`src/launch-cli-golden.ts` 头注写了为什么夹具本身不删）。
// 🔴 〔LR2 · U8c-3 后一半〕**4 → 2，到了 `LAUNCH_RENDERER_TARGET`**：TS 兜底渲染器（`renderFallback`）
// 与它的座（`TMUX_BACKEND`）两行删了 —— 零生产调用，最后只剩「两份夹具的左边」一个用途，
// 那一格同样换成了用例表里的手写期望（`tests/test-support/launch-payload-golden.ts` / `launch-tmux-outer-golden.ts` 头注）。
const LAUNCH_RENDERERS: &[(&str, &str, &str, &str)] = &[
    (
        "src/bridge/src/backend/control/ccm_invocation.rs",
        "render_ccm_invocation",
        "Rust · ccm 调用行",
        "✅ **目标态那两份之一**。生产在跑（`render_ccm_launch` ⇒ 装了 ccm 的远端）。",
    ),
    (
        "src/bridge/src/backend/control/payload.rs",
        "render_payload",
        "Rust · 载荷（`设计/90 §4 E` 起同时管外层那三格）",
        "✅ **目标态那两份之一**。🔴 **〔步 22b·B 2026-09-20〕内层与外层三格今天都在生产上跑** \
         —— 同一条命令 `render_launch_payload`（`outer` 缺席 = `container:\"none\"` 那一格，\
         带 `outer` = tmux 那三格）。这里原来逐字写着「外层三格 `render_tmux_outer` 本拍刚补出来，\
         **生产调用方 0**，如实登记」，那是 22b·A 的读数。\
         生产路上的那道闸由 `tests/remote-launch-run.vitest.ts` 的 `W22B` 组钉着。",
    ),
];

/// `设计/00 §2.5 ④` 承诺的终点。**只许降到它，不许从它往上爬。**
const LAUNCH_RENDERER_TARGET: usize = 2;

/// ★ 逐份点名 + 恒等 + 每一份**实打指得到真东西**。
#[test]
fn the_launch_renderers_on_disk_are_exactly_these() {
    // ① 条数恒等，而且**就是目标值**。〔LR2〕5 → 4（LR1）→ 2：`设计/00 §2.5 ④` 结账，
    //    本条从「还在路上」改成「恰好是那两份」（原来那句「份数已经降到目标 —— 那一天到了」的断言，
    //    今天按它自己的吩咐翻了面）。
    assert_eq!(
        LAUNCH_RENDERERS.len(),
        LAUNCH_RENDERER_TARGET,
        "盘上的渲染实现份数不是目标 {LAUNCH_RENDERER_TARGET} 了 ——\n\
         · **多了一份** ⇒ 先问「为什么同一件事要有第二个家」（`设计/00 §2.5 ④` 的整个要点就是消灭副本）；\n\
         · **少了一份** ⇒ 起会话有一格说不出命令了，先查是哪一份没了。"
    );

    // ② 每一份的住址在盘上，且入口符号真的在它的**生产段**里。
    //    没有这一步，上面那个 4 只是一个数字 —— 把四行路径全改成 `a.ts` 它照样绿。
    for (path, symbol, what, _) in LAUNCH_RENDERERS {
        let full = repo_root().join(path);
        assert!(full.is_file(), "登记的渲染实现 {path}（{what}）不在盘上");
        let raw = std::fs::read_to_string(&full).expect("读不到");
        let prod = if path.ends_with(".ts") {
            production_ts(&raw)
        } else {
            guard_core::production_code(&raw)
        };
        assert!(
            guard_core::contains_word(&prod, symbol),
            "{path} 的生产段里找不到入口符号 `{symbol}` ——\n\
             要么它改名了（回来改这张表），要么这一份已经不是渲染实现了（那就该删行）。"
        );
    }

    // ③ 目标态那两份**今天真的是 Rust**（不是把两份 TS 标个勾就算达标）。
    let rust: Vec<&str> = LAUNCH_RENDERERS
        .iter()
        .filter(|(p, ..)| p.ends_with(".rs"))
        .map(|(p, ..)| *p)
        .collect();
    assert_eq!(
        rust.len(),
        LAUNCH_RENDERER_TARGET,
        "Rust 侧的份数不是 {LAUNCH_RENDERER_TARGET} —— 目标态是「Rust CLI ＋ Rust 载荷」两份，\
         多出来的那份说明外层又被单开了一个家（`设计/90 §4 E` 刻意把它并进载荷那份，理由在 `payload.rs` 头注）"
    );
}

/// ★ **反向闭合（外层 tmux 那一层）** —— 从源码派生，多一个家就红。
///
/// 上面那张表是**人写的**：少登记一份它看不见。这一条补的正是那个方向 ——
/// 「`tmux new-session -d -s ` 这条命令在 monitor 的 Rust 那一侧有几个家」由机器数出来，
/// 与期望的那一份**两向集合相等**（前端那一侧见函数体里 〔LR2〕 那段）。
///
/// 🔴 **为什么盯这一条字面量**：`设计/90 §4 E` 搬的就是它，
/// 而它是**要落进用户 shell 去执行的字节** —— 第三个家出现的那一刻，
/// 「两份实现、逐字节对拍」这个结构就已经不成立了，而**别的判据一条都不会响**
/// （各自的夹具只管自己那一份）。
///
/// ⚠ 射程如实写：它数的是**剥完注释的生产段里那个字面量出现过没有**。
/// 把命令拆成几段拼（`"tmux new-" + "session"`）能从缝里过去 —— 本条不声称堵住那个。
/// 它买的是「**照抄一份**」这种最常见的形状会有东西说话。
#[test]
fn the_outer_tmux_command_has_exactly_one_home() {
    let needle = format!("tmux new-{} -d -s ", "session");
    let mut homes: Vec<String> = Vec::new();
    let mut scanned = 0usize;

    let root = repo_root();
    for (p, raw) in guard_core::scan_tree!(&root.join("src/bridge/src"), &["rs"]) {
        scanned += 1;
        if guard_core::production_code(&raw).contains(needle.as_str()) {
            homes.push(format!(
                "src/bridge/src/{}",
                p.strip_prefix(root.join("src/bridge/src"))
                    .unwrap_or(&p)
                    .to_string_lossy()
            ));
        }
    }
    // 〔LR2〕原来这里接着扫 `src/**/*.ts`（那时 TS 座 `session-backend.ts` 是第二个家）。座删了之后
    //   前端那一侧改由 `tests/launch-no-shell-in-ts.vitest.ts` 管（`设计/90 §3` 条 1：三个入口的 import 闭包里
    //   零 `tmux <动词> -` / `&&` 字面量）—— 它按执行链取人群，把 `src/` 里两份夹具用例表的手写期望
    //   （它们逐字就是这条命令）排在前端之外；这里再扫一遍 TS 就得另抄一份那个排除，两份会漂。
    // ★ 抽取器自检：人群没缩水（否则 `homes` 恒空 ⇒ 集合相等会在两边都空时假绿）。
    assert!(
        scanned >= 50,
        "只扫到 {scanned} 个 monitor 侧 `.rs` —— 遍历坏了"
    );

    homes.sort();
    // 〔LR2〕两个家 → **一个**：TS 那个座（`src/session-backend.ts`）删了，`设计/90 §4 E` 收官。
    let want = vec!["src/bridge/src/backend/control/payload.rs".to_string()];
    assert_eq!(
        homes, want,
        "\n★ 外层 tmux 命令的家变了。两向集合相等，所以多一个少一个都在这儿说话：\n\
         · **多一个** ⇒ 有人又照抄了一份 `tmux new-session …`。`设计/00 §2.5 ④` 的整个\n\
           要点是消灭副本；前端那一侧另有 `tests/launch-no-shell-in-ts.vitest.ts`（`设计/90 §3` 条 1）。\n\
         · **少一个** ⇒ 起会话那三格没人产得出了（`payload.rs::render_tmux_outer`）。\n"
    );
}
