//! F07（出口③ 早已交付）：**远端起会话主路的决策已经在 backend 渲染** —— 把它钉住。
//!
//! # 摸底结论
//!
//! F07 的题目是「远端起会话主路走 backend」。逐段量下来**决策那半已经切完了**：
//!
//! | 段 | 今天在哪 |
//! |---|---|
//! | 会话名 | F13 的铸名口（`mintTmuxName`，避让不可分离） |
//! | §34 三道门 | F03 + F04a 已搬进 daemon `control/` |
//! | 内层载荷 | `backend::control::payload`（P4b） |
//! | ccm 调用行 | `backend::control::ccm_invocation`（P4b） |
//! | **生产切换** | ✅ `remote-launch-run.ts` 三处在调 `render_ccm_launch` / `render_launch_payload` |
//!
//! 剩下的**只有「删 TS 那两个渲染器」**，而那是 U8c-3 的题目、不是 F07 的
//! —— F07 要的是「走 backend」，不是「删旧的」。
//!
//! # ⚠ 摸底在 `src/doc/INVARIANTS.md §33b` 里抓到**两处过期陈述**
//!
//! **过期一**：那张表把 **U8c-2c-2 写成「待做」** —— 实测已交付
//! （两条 tauri 命令注册 + 生产 TS 三处在调 + `parity_ledger` 两条能力）。
//!
//! **过期二**：三问的答案① 写「**否** —— 全仓 `.call("launch")` 只有一处且在 `cfg(test)` 里」，
//! 实测**生产段有一处**（`daemon_launch.rs`，U8a-2c-1 的 `daemon_send_into`）⇒ 应为「**部分是**」。
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
//! | ① 生产切到 daemon 的 `launch` 了吗 | 部分是（`send-into` 一格） | **仍是「部分是」**：生产段 `create-or-attach` **0 处**（下面那条判据在量），`attach` 结构上不归 daemon（`control/launch.rs` 头注「本模块**不 attach**」） |
//! | ② attach 那条串归谁产 | 一半有答案 | **仍挡着，而且不止 attach**：`renderFallback` 的三格（tmux `create` / `send-into` / `attach`）全在 TS。⇒「只剩 attach 那一格」是把阻碍读窄了 |
//! | ③ daemonless 的远端还要不要能起会话 | **未决**（U12 待做） | **已决：要。** `U12` 那个**件**被 `C7` 关掉了，但 `C7` 裁的是「**本机**也要有后端进程」；而 `daemonless` 今天是**每台远端主机的用户开关**（`src/settings/machine-card.ts` 那个 checkbox「daemonless 降级读取（无需 daemon）」→ `RemoteHostConfig.daemonless`，生产段 7 个文件 31 处）⇒ 那种主机**存在**、且它的 `↗` 走纯 SSH（`launch_remote_terminal` 不经 daemon）⇒ 起会话只能靠 monitor 自己渲染整串 |
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
//! `the_two_reasons_u8c3_cannot_delete_the_ts_renderer_still_hold` 的头注「第四次」那一节。

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
    "src/launch-render-fallback.ts",
    "src/remote-config.ts",
    "src/settings/machine-card.ts",
    // `K-R59`：那条 TS 兜底路的消费者，逐处登记在 `TS_FALLBACK_KEEPERS`。
    "src/remote-launch.ts",
    "src/launch-payload-golden.ts",
];

/// 🔴 **那条 TS 兜底路今天靠谁站着** —— `U8c-3` 的**新**存续理由，逐处点名。
///
/// `(被引的符号, 消费者文件, 生产段处数, 它是什么, 它什么时候能走)`
///
/// 现打于 `4d298c4`（`src` 下，去掉 `*.test.ts` / `*.vitest.ts`）。
/// ⚠ **处数由判据从源码派生再逐格比对** —— 散文里那个数会腐烂，这张表不行。
/// ⚠ 尺子的射程如实写：它数的是**剥完注释的源码里那个标识符出现几次**
/// （`import` 那一行算一处）。别名 import、动态取属性它都数不到，**不声称堵住**。
///
/// # 🔴 `K-R105`（09-13）：**这张表只是尺子A，而散文抄的是它、读的却是尺子B**
///
/// 本表数的是「**盘上还有谁提到它**」（标识符出现处数）。而「**这条路今天还站不站在
/// 生产上**」是另一个问题，两把尺子的读数差着一个数量级：09-13 现打，尺子A 说
/// `renderFallback` 有三个消费者文件，尺子B 说**有生产调用方的只有一个**
/// （`remote-launch.ts` 那五个 builder 的生产调用方是 0；`launch-payload-golden.ts`
/// 只被 `npm run gen:payload-golden` 那条链走到）。
///
/// ⚠ 那句「N 个生产消费者」在**尺子A 上一直是对的**，所以 08-14 到 09-13 之间
/// 没有人去核它 —— 而它被读成了尺子B 的读数，还传抄了好几份。
/// **一个数不写清它的尺子，就是半句假话**（`K-R89` 的 PM 审计逐字纠过同一处：
/// 「那 5 个 builder 是生产消费者」—— 生产调用方 0）。
/// ⇒ 尺子B 落成 [`TS_FALLBACK_REACH`]，同样**从源码派生**；散文里两个数都不许再写。
const TS_FALLBACK_KEEPERS: &[(&str, &str, usize, &str, &str)] = &[
    (
        "renderFallback",
        "src/launch-payload-golden.ts",
        2,
        "金样本发生器：`import` 一处 + `payload: renderFallback(planOf(c))` 一处。\
             它产的是 `backend/control/fixtures/payload-golden.json` —— Rust 侧那条\
             「两边逐字节同构」的对拍拿它当**左边**。",
        "Rust 侧不再拿 TS 的输出当金样本的那天（那要先有另一个真相源）。",
    ),
    (
        "renderFallback",
        "src/remote-launch.ts",
        6,
        "五个 builder（resume 直起 / resume 进 tmux / 送进已有 tmux / 起 launcher / attach）\
             各调一次 + `import` 一处。",
        "这五条各自都改走后端渲染的那天。",
    ),
    (
        "renderFallback",
        "src/remote-launch-run.ts",
        2,
        "生产主路的**回落**：`renderCliViaBackend` 不成时兜底（`import` 一处 + 调用一处）。",
        "`renderCliViaBackend` 覆盖到全部三格、回落变成死代码的那天。",
    ),
    (
        "SESSION_BACKEND",
        "src/launch-render-fallback.ts",
        4,
        "座本身：`import` 一处 + `attach` / `createRunAttach` / `runInExistingAttach` 各一处。",
        "外层 tmux 命令改由后端产出的那天（`control/launch.rs` 头注逐字「本模块**不 attach**」）。",
    ),
    // 🔴 〔`K-R109` 09-13〕**这里原来有第 6 行，走了。**
    // 原文：`("SESSION_BACKEND", "src/remote-launch-run.ts", 2, "`import` 一处 +
    // `attachCmd` 那一处（把 `↗` 交给用户自己的终端那一跳）", …)`，解锁条件写着
    // 「`attach` 那一跳由 `K-R59` `§0c` 明写**不做** —— 后端在远端开不了你面前的窗」。
    // ⚠ **那句话没有错，它的射程被读宽了**：`control/launch.rs` 那条「本模块不 attach」
    // 讲的是**远端**（`R61` 裁定三之后不许再拿「daemon」把两侧压成一个）。
    // 这一处是**本机**就地 resume，后端就在用户面前那台机器上 ⇒ 它产得出，也接过去了
    //（`history.rs::render_local_attach` ⇒ `commands.render_local_attach`）。
    // ⇒ 这一行**不是「登记漏了」，是消费者真的少了一个**：生产处数 2 → 0。
    // 反向闭合由下面 ⑤ 那一格守着（少一个不登记会红，**多一个也会红**）。
];

/// 🔴 **尺子B**〔`K-R105` 09-13〕：[`TS_FALLBACK_KEEPERS`] 里那个消费者文件
/// **今天还站不站在生产路上**。
///
/// 与尺子A（标识符出现处数）**问的不是同一件事**，读数也不同 ——
/// 这张表存在的全部理由就是让两把尺子各有一个家，别再有人拿其中一个的数
/// 去说另一个的话（那正是 08-14 那句散文的形状）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Reach {
    /// **有生产调用方**：它导出的那几个符号，在 `src/**` 的生产段里（本文件之外）
    /// 至少被引一次 ⇒ 这条路真的走得到。
    OnProductionPath,
    /// **生产调用方 0**：只剩测试 / e2e / `npm run …` 那条链在调。
    /// ⚠ 这**不等于**「可以删」—— 第四列写清它今天靠谁跑。
    OffProductionPath,
}

/// `(消费者文件, 把这条路接出去的导出符号, 判定, 它今天靠谁跑)`。
///
/// # 为什么导出符号是手写的，而判定是派生的
///
/// 「这个文件的哪几个导出把兜底那条路接出去」是**语义选择**，机器判不了：
/// `remote-launch.ts` 同时还导出 `mintTmuxName` / `deriveTmuxName` /
/// `buildOpenTerminalCmd` 三个**真在生产跑**的符号，照「文件有没有生产调用方」量，
/// 它会被读成 `OnProductionPath` —— 而那不是本表要答的问题。
/// ⇒ 符号由人挑（挑错了 `7u` 会看见），**判定由机器从源码数出来**。
///
/// ⚠ 尺子B 自己的射程也如实写，三条：
///
/// 1. 它数的是「剥完注释的生产段里那个标识符**以整词形态**出现过没有」
///    （[`guard_core::contains_word`]，与尺子A 同一份剥法 [`production_ts`]）
///    ⇒ **别名 import 与动态取属性它数不到**。
///    🔴 **整词是必需的，不是讲究**：建这张表当天用裸子串量过一趟，
///    `launch-cli-golden.ts` 的 `CLI_GOLDEN_CASES` 把 `GOLDEN_CASES` 命中了
///    ⇒ 一个夹具发生器被读成「生产调用方」，判定当场从 `Off` 翻成 `On`。
/// 2. **只走一跳，但会跳过已登记为 `Off` 的那几家**：`launch-render-fallback.ts`
///    被三个文件引，其中两个自己就是 `Off` ⇒ 真正撑着它的只有 `remote-launch-run.ts`。
///    不做这一步的话，「一群互相引用的死代码」会集体读成 `On`。
/// 3. ⚠ 而它**仍然不是可达性分析**：若两个 `Off` 文件互相引用之外还有第三条边，
///    或者某个 `On` 的家其实自己走不到生产入口，本条看不出来。
///    ⇒ 它买的是「**这条路少了/多了一个真正的生产家，会有东西说话**」，
///    不是「这几行代码今天真的在跑」。后者要真机，不在本条射程。
const TS_FALLBACK_REACH: &[(&str, &[&str], Reach, &str)] = &[
    (
        "src/launch-payload-golden.ts",
        &["renderGoldenFixture", "GOLDEN_CASES"],
        Reach::OffProductionPath,
        "`npm run gen:payload-golden`（`tests/e2e/launch-payload-golden-emit.mts`）\
             与它自己那份 vitest。**生产运行时零调用**，但它产的入库夹具是 Rust 那条\
             逐字节对拍的**左边** ⇒ 删它要先给那条对拍换一个真相源。",
    ),
    (
        "src/remote-launch.ts",
        &[
            "buildResumeDirectCmd",
            "buildResumeTmuxCmd",
            "buildResumeIntoExistingTmuxCmd",
            "buildLauncherCmd",
            "buildAttachCmd",
        ],
        Reach::OffProductionPath,
        "只有 `remote-launch.test.ts` · `tests/e2e/resume-cmd-driver.ts` · \
             `tests/e2e/tmux-target-emit.mts` 在调（`K-R89` 的 PM 审计现打核过：生产调用方 0）。\
             ⚠ 同一份文件里 `mintTmuxName` / `deriveTmuxName` / `buildOpenTerminalCmd` \
             **是生产在跑的** —— 本行判的是上面那五个 builder，不是这个文件。",
    ),
    (
        "src/remote-launch-run.ts",
        &[
            "runRemoteResume",
            "runRemoteResumeTmux",
            "runRemoteLauncher",
        ],
        Reach::OnProductionPath,
        "`tabs.ts` / `views/history.ts` / `account-restart.ts` 那条 `↗` 主路。\
             **这是兜底渲染器今天唯一的生产入口。**",
    ),
    (
        "src/launch-render-fallback.ts",
        &["renderFallback"],
        Reach::OnProductionPath,
        "被上面那个生产入口引（`remote-launch-run.ts` 的回落那一行）。",
    ),
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
/// 一旦有人把它改回「TS 自己渲染」，本条红 —— 而那种回退**功能不变砖**
/// （TS 兜底渲染器还在），门禁也不会因为别的原因红。
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
                 远端起会话主路不再走 backend 渲染了。\n\
                 ⚠ 这种回退**功能不变砖**（TS 兜底渲染器还在），所以除了本条没人会红。"
        );
    }
}

/// ★ **前提触发器**：U8c-3（删 TS 渲染器）今天删不得的**两条依据**仍然成立。
///
/// 依据一：**兜底渲染器仍是生产渲染器** —— `remote-launch-run.ts` 的**生产段**
/// 仍在调 `renderFallback(`，而它产的三格（tmux `create` / `send-into` / `attach`）
/// 全要外层 tmux 命令，归 TS 的座 `session-backend.ts`
/// （daemon 的 `control/launch.rs` 头注逐字写着「本模块**不 attach**，一次都不」）。
/// 依据二：**`create-or-attach` 那格仍未切** —— 生产段一次都不发这个 mode。
///
/// ⚠ 〔08-14〕依据一的**量法换了**：原来量的是「文件里出现过 `session-backend` 这个词」
/// （含注释、且两份文件 `||`），那在它要报的方向上是瞎的 —— 见 `production_ts` 头注。
/// 现在量「**那个调用还在不在**」，注释里怎么写它都不算数。
///
/// ⚠ 如实登记本条量法的边界：它仍是**子串**。把 `SESSION_BACKEND` 换个本地别名
/// （`const seat = SESSION_BACKEND; seat.attach(…)`）能从缝里过去 ——
/// 但那种改动**不改变前提本身**（座还在 TS、外层命令还是 TS 产），
/// 而本条要报的是**前提没了**，所以这个缝不在它的射程里。
///
/// 任一条变了 ⇒ **主动红**：那时 U8c-3 的前置动了，回来重裁 F07 的剩余面。
///
/// # ⚠ F04c 订正了依据二的**度量方式**（结论没变）
///
/// 原来数的是「生产段 `.call("launch")` 的处数 == 1」。F04c 让它变成 **2** 而当场报红 ——
/// **它红得对**（前提确实动了，该回来重裁），**但重裁的结论是「依据二仍成立」**：
/// 新增那一处是 `daemon_send_keys` 发的 `send-into` / `send-keys-raw`，那是
/// **`tmux_send_keys` 这条命令**改走 daemon，**不是「起会话」又切了一格**。
///
/// ⇒ **`.call("launch")` 的处数是个过期的代理指标**：它把「有几条代码路径用 launch 命令」
/// 和「起会话有几格切到了 daemon」混成一个数。改成直接量后者 ——
/// **生产段发不发 `create-or-attach`**。
///
/// ★ 这是「**依据/度量过期而结论仍对**」在本工作区的**第三次**
/// （F01 四处「每 ~8s」· F07 `§33b` 两处 · 本条）。三次的处置都一样：
/// **把结论留住，把依据换成还量得准的那个**。
///
/// # ⚠⚠ 第四次：`K-P2` 08-29 —— 依据二的**扫描面**比它要报的事实小了一格
///
/// 上面那句「改成直接量后者」把**量法**修对了，却留下一个**面**的洞：
/// 扫的是 `env!("CARGO_MANIFEST_DIR")/src`，也就是**只有 `src/bridge/src/**.rs`**。
/// 而「起会话改走 daemon」有两条路，`K-P2 §0d`〔PM 08-29〕**裁的是后一条**：
///
/// | 路 | 接线落在哪 | 本条**看不看得见** |
/// |---|---|---|
/// | ㈡ 宿主自己发 `launch` | `src/bridge/src/**.rs` | 看得见 |
/// | ㈠ **`ccm` 直接问后端二进制**（`§0d` 裁定：`ccm <子命令>` = 后端以**一次性模式**跑） | `shared/ccm`（**shell**） | **看不见** |
///
/// ⇒ 走㈠ 的话，「起会话那格切到 daemon 了」这件事**做成了，而本条一个字都不说**
/// —— 那不是绿，是**零命中地绿**。`K-P2` 的 `KP2A` 逐字预言过这个形状：
/// 「它的扫描面只有 `src/bridge/src/**/*.rs` ⇒ **扫不到 `shared/ccm`** ……
///  这条判据**零命中地绿**，而事情做成了它一个字都不说」。
///
/// ⇒ **把 `shared/ccm` 的生产段加进同一个扫描面**（同一个结论、同一个 needle、
/// 同一条失败文案），并给它配自己的抽取器自检。
/// **结论仍然只有一条**：「起会话那格有没有切过去」——变的是它够得到哪几棵树。
///
/// ⚠ **本条不管「ccm 发了哪几条一次性子命令」**（那是 `ccm_cli_contract` 的
/// `ccm_reaches_the_backend_through_one_shot_subcommands` 〔散文墓碑〕（`K-R48` 第二拍随 `shared/ccm` 删），`K-P2 KP2A②`）：
/// 一条判据一件事。本条只回答 F07/U8c-3 要的那一句——**起会话那格切了没有**。
#[test]
fn the_two_reasons_u8c3_cannot_delete_the_ts_renderer_still_hold() {
    // 依据一 a：**生产主路仍在调兜底渲染器**（`container: tmux` 的三格都落它）。
    let run = production_ts(&read_ts("src/remote-launch-run.ts"));
    assert!(
        run.contains("renderFallback("),
        "`remote-launch-run.ts` 的**生产段**里再也没有 `renderFallback(` 了 ——\n\
             **这多半是好事**：兜底那支可能已经搬走了 ⇒ U8c-3 的依据一没了，回 F07/U8c-3 重裁。\n\
             ⚠ 别看注释怎么写 —— 本条只读生产段（`production_ts`），这正是 08-14 换掉的量法。"
    );
    // 依据一 b：**外层 tmux 命令仍归 TS 的座产**（兜底渲染器仍问 `SESSION_BACKEND` 要）。
    let fallback = production_ts(&read_ts("src/launch-render-fallback.ts"));
    assert!(
        fallback.contains("SESSION_BACKEND."),
        "`launch-render-fallback.ts` 的**生产段**不再问座要命令了 ——\n\
             **这多半是好事**：外层 tmux 命令（`new-session` / `send-keys` / `attach`）\n\
             可能已经不归 TS 产了 ⇒ U8c-3 的依据一没了，回 F07/U8c-3 重裁。"
    );
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
    //（旧 bash 的 `launch_via_daemon` 〔散文墓碑〕 发 `mode=create-or-attach`）⇒ **这一半当场红了，红得对**。
    //
    // ⇒ 翻成正向：`shared/ccm` 从「不许有」变成「**必须有**」。
    // **别把它删掉**：删掉之后「起会话又退回本机 tmux 直起」就没有任何东西会说话。
    //
    // 🔴 **而本条的结论不变，理由要写清楚**（这正是那句 assert 文案要求「回来重裁」的事）：
    // 三问的答案① 变了（起会话这一格 `shared/ccm` 已经切到 daemon），
    // **但上面那两条依据是独立的、且都还成立** ——
    // ① a：`remote-launch-run.ts` 的生产主路仍在调 `renderFallback(`；
    // ① b：`launch-render-fallback.ts` 仍问 `SESSION_BACKEND.` 要外层 tmux 命令。
    // 那两条说的是 **monitor 自己那条 `↗` 路**（TS 渲染 → `ssh -t bash -lic '<串>'`），
    // 它与「ccm 在远端自己起会话时问不问 daemon」**是两条路，别压成一句**。
    // 〔散文墓碑〕这里原来还写着「再加上 `the_daemonless_remote_still_needs_the_ts_fallback_renderer`
    // 那条**硬**障碍（daemonless 主机今天仍是产品提供的开关）」——
    // 🔴 `K-R59`（09-11，定框 `K35`）把那一档整格删了，那条判据也随之换人。
    // ⇒ **删 TS 渲染器的前置仍然不成立**，但今天的理由换成了
    //   `the_ts_fallback_renderer_now_stands_on_its_own_consumers`（消费者逐处见
    //   `TS_FALLBACK_KEEPERS`；**处数与「站不站在生产路上」都从源码派生，这里不写死**
    //   —— `K-R105` 09-13 把那两个字面量从全部散文副本里撤了，理由见那张表的头注）。
    // ⚠ 而 **Rust 那棵树仍然必须是零** —— monitor 侧那条 `.call("launch")` 至今只发
    //   `send-into` / `send-keys-raw`（`daemon_launch::the_only_mode_this_channel_can_speak_is_send_into`
    //   钉着它）。**两棵树本拍起口径不同，这不是疏漏，是两件不同的事。**
    assert!(
        hits.is_empty(),
        "**Rust 生产段**开始发 `create-or-attach` 了（{hits:?}）—— **这多半是好事**：\n\
             monitor 侧「起会话」那格可能也切到 daemon 了 ⇒ `INVARIANTS §33b` 三问的答案① 又变了，\n\
             回 F07/U8c-3 重裁「删 TS 渲染器」的前置。\n\
             ⚠ 同轮还要回 `K-P2`：`KP2A②` 的棘轮要抬、`KP2C` 的退路要登记、\n\
             `KP2D` 的通道 A/B 冲突必须已经解掉。\n\
             ⚠ `control/ccm/` **不在本断言的人群里**（它在下面单判，`K-P2` `D3` 已经切过去了）。"
    );
    assert!(
        guard_core::contains_word(&ccm, &word),
        "`control/ccm/` 的生产段**不再发** `create-or-attach` 了 —— 起会话退回本机 tmux 直起？\n\
             `K-P2` `D3`（09-03）把它接到了后端那条一次性口上；`K-R48`（09-11）之后\n\
             那条口住进了同一个进程（`control::launch::parse_request` 那道门）。\n\
             ⇒ 真要退回来，请连同 daemon 侧那条\n\
             `the_container_launch_goes_through_the_one_door_with_every_field_intact`\n\
             一起撤，并回 `K-P2` 说明为什么。"
    );
}

/// 🔴🔴 **`U8c-3` 的前提换人了 —— 这是那份换人手续。**〔`K-R59` `KR59D2` 09-11〕
///
/// # 被换掉的那条是什么
///
/// 〔散文墓碑〕这里此前住着 `the_daemonless_remote_still_needs_the_ts_fallback_renderer`（08-14 立）。
/// 它主张两件事：① `daemonless` 这个每机开关还在（字段 + 界面那一格，两处都要）；
/// ② **所以** `src/launch-render-fallback.ts` 与 `src/session-backend.ts` 删不得 ——
/// 没装 ccm 的 daemonless 远端，它的 `↗` 命令就是这两个文件产的。
///
/// 它自己逐字写着：「哪天 `daemonless` 这个开关真被取消了（那才是 `C7` 覆盖到远端的那一天），
/// **本条主动红**，提醒回来重裁 `U8c-3` —— 那时兜底渲染器少了一类必须服务的主机。」
///
/// **那一天就是 09-11**（用户定框 `K35`：「不要有 daemonless。没有没有后端的情况。
/// 前端应该就是去调用远程后端的。」）⇒ 那条判据**红了，而且红得对**。
///
/// # 🔴 而它红完之后的答案，不是它自己那句话
///
/// 它预写的结论是「那时兜底渲染器**少了一类必须服务的主机**」——
/// 听起来像「可以删了」。**现打不是这样**：那条路**另有消费者**，
/// 逐处与处数见 [`TS_FALLBACK_KEEPERS`]（**处数从源码派生，这里不写一个字面量**）。
///
/// 🔴 **`K-R105` 09-13：这一段原来在这儿写着两个数（「3 个」「2 个」），撤了。**
/// 那两个数是**尺子A**（标识符出现处数）的读数，而读它的人（连同散文里那几份副本）
/// 一律把它读成**尺子B**（有没有生产调用方）。两把尺子今天各有一个家：
/// 尺子A = [`TS_FALLBACK_KEEPERS`]，尺子B = [`TS_FALLBACK_REACH`]，**两张都从源码派生**。
///
/// ⇒ **前提退役，但那条路不退役 —— 它另有消费者。**
/// 08-14 那条把「daemonless 主机需要它」当成了「它删不得」的**理由**，
/// 而那不是唯一的理由。**换人手续办完了，不是把它抹掉。**
///
/// ⚠ **本条只答「那条路还有没有别的消费者」，不答「那几个消费者今天还该不该存在」** ——
/// 后者要读 `U8c-3` 的原意，不在 `K-R59` 射程（件文件 `§0b-3` 逐字）。
///
/// # 本条什么时候该红（新的触发条件，逐条写死）
///
/// - **那一档回潮**：`daemonless` 又变回一个用户开关（字段清单 / 界面 / 数据源分支任一处）
///   ⇒ 红。`K35` 是用户定框，回潮要先回去改定框，不是悄悄加回来。
/// - **消费者少一个**：[`TS_FALLBACK_KEEPERS`] 与源码对不上 ⇒ 红。**掉到 0 那天**
///   就是这两个文件真能删的那天 —— 那时回 `U8c-3`，而不是靠读注释判断。
#[test]
fn the_ts_fallback_renderer_now_stands_on_its_own_consumers() {
    // ① **前提确实退役了**（不是「名字没了」，是那一档没了）。
    //    🔴 刻意**不**用 `!contains("daemonless")` —— `remote-config.ts` 里还剩**一处**
    //    该词的字面量（`LEGACY_NO_BACKEND_KEY`，认旧配置用的墓碑），数名字会把它读成回潮。
    //    ⇒ 断的是**载体**：落盘字段清单里那一项 · 界面那个 input · 数据源那条分支。
    let cfg = production_ts(&read_ts("src/remote-config.ts"));
    assert!(
        !cfg.contains(r#""daemonless","#),
        "`REMOTE_HOST_FIELDS` 里又有 `daemonless` 了 —— 那一档回潮了。\n\
             `K35`〔用 09-11〕逐字：「不要有 daemonless。没有没有后端的情况。」\n\
             要加回来先回去改定框，并同轮重裁 `U8c-3`（本条的存续理由会跟着变）。"
    );
    let card = production_ts(&read_ts("src/settings/machine-card.ts"));
    assert!(
        !card.contains("daemonlessInput"),
        "机器卡片的生产段里又有那个开关了 —— 用户又能造出「不装后端」的主机。同上。"
    );
    let src = guard_core::production_code(include_str!("../../../../src/bridge/src/ssh_source.rs"));
    assert!(
        !src.contains("daemonless_stream_loop"),
        "`ssh_source.rs` 生产段里那条轮询回落又回来了 —— \n\
             **开关没了而路还在**，那是最坏的一种：没有任何界面造得出它，却仍有一条代码路等着。"
    );
    // ② **新的存续理由**：那条路今天靠自己的消费者站着，逐处点名、处数从源码派生。
    for (symbol, file, want, what, unlock) in TS_FALLBACK_KEEPERS {
        let code = production_ts(&read_ts(file));
        let got = code.matches(symbol).count();
        assert_eq!(
            got, *want,
            "`{file}` 里 `{symbol}` 的生产处数是 {got}，登记的是 {want}。\n\
                 那一处是什么：{what}\n\
                 **少一处** ⇒ 一个消费者走了，把登记拧下来；\n\
                 **掉到一个都不剩** ⇒ 那才是「这条路可以退役」，回 `U8c-3` 重裁，别自批。\n\
                 它什么时候能走：{unlock}"
        );
    }
    // ③ 🔴 **尺子B 也从源码派生**〔`K-R105` 09-13〕：每个消费者文件今天还站不站在生产路上。
    //    两向对拍：`TS_FALLBACK_KEEPERS` 里出现过的文件必须在 `TS_FALLBACK_REACH` 里恰好一行，
    //    反之亦然 —— 少一行，那个文件的「生产可达吗」就没人判过。
    {
        let mut keeper_files: Vec<&str> = TS_FALLBACK_KEEPERS.iter().map(|(_, f, ..)| *f).collect();
        // 座本身那一格：`SESSION_BACKEND` 的消费者里有 `launch-render-fallback.ts`，
        // 而 `renderFallback` 的定义也住在它里面 ⇒ 它两种身份都算，只登记一次。
        keeper_files.sort_unstable();
        keeper_files.dedup();
        let mut reach_files: Vec<&str> = TS_FALLBACK_REACH.iter().map(|(f, ..)| *f).collect();
        reach_files.sort_unstable();
        let mut reach_uniq = reach_files.clone();
        reach_uniq.dedup();
        assert_eq!(
            reach_files, reach_uniq,
            "`TS_FALLBACK_REACH` 里有重复的文件 —— 一个事实恰好一个住址"
        );
        assert_eq!(
            keeper_files, reach_files,
            "尺子A 的人群与尺子B 的人群对不上。\n\
                 **尺子A 多出来的** ⇒ 那个消费者文件没人判过「它今天还站不站在生产路上」；\n\
                 **尺子B 多出来的** ⇒ 那一行在描述一个已经不在尺子A 人群里的文件。\n\
                 ⚠ 两把尺子问的不是同一件事，所以**两张表都要有它** —— \
                 08-14 那句散文之所以能带着一个尺子A 的数说尺子B 的话，就是因为尺子B 没有家。"
        );
        // 射程第 2 条：已登记为 `Off` 的那几家不算「生产调用方」（见 `TS_FALLBACK_REACH` 头注）。
        let off_files: Vec<&str> = TS_FALLBACK_REACH
            .iter()
            .filter(|(_, _, r, _)| *r == Reach::OffProductionPath)
            .map(|(f, ..)| *f)
            .collect();
        // 生产 TS 语料**只取一次**（剥完注释），下面逐格复用。
        let mut corpus: Vec<(String, String)> = Vec::new();
        for (p, raw) in guard_core::scan_tree!(&repo_root().join("src"), &["ts"]) {
            let rel = p
                .strip_prefix(repo_root())
                .unwrap_or(&p)
                .to_string_lossy()
                .replace('\\', "/");
            if rel.ends_with(".test.ts") || rel.ends_with(".vitest.ts") {
                continue;
            }
            corpus.push((rel, production_ts(&raw)));
        }
        // ★ 抽取器自检：人群没缩水（否则 `OffProductionPath` 那几格零命中地绿）。
        assert!(
            corpus.len() >= 100,
            "只扫到 {} 份生产 TS —— 遍历坏了，下面那几条会零命中地绿",
            corpus.len()
        );
        for (file, exports, want, who) in TS_FALLBACK_REACH {
            assert!(!who.trim().is_empty(), "`{file}` 没写它今天靠谁跑");
            assert!(
                !exports.is_empty(),
                "`{file}` 一个导出符号都没挑 —— 下面那条会零命中地绿"
            );
            let mut callers: Vec<&str> = Vec::new();
            let mut off_callers: Vec<&str> = Vec::new();
            for (rel, code) in &corpus {
                if rel.as_str() == *file {
                    continue; // 定义处不算调用方
                }
                // 🔴 整词，不是裸子串 —— 理由（`CLI_GOLDEN_CASES` 那次）见本表头注。
                if !exports.iter().any(|s| guard_core::contains_word(code, s)) {
                    continue;
                }
                if off_files.contains(&rel.as_str()) {
                    off_callers.push(rel.as_str());
                } else {
                    callers.push(rel.as_str());
                }
            }
            let got = if callers.is_empty() {
                Reach::OffProductionPath
            } else {
                Reach::OnProductionPath
            };
            assert_eq!(
                got, *want,
                "`{file}` 的**尺子B** 读数是 {got:?}，登记的是 {want:?}\n\
                     （生产调用方：{callers:?}；已登记为 Off、因而不计入的调用方：{off_callers:?}）。\n\
                     **从 Off 变成 On** ⇒ 有人把它接进生产了 ⇒ 改登记，并回 `U8c-3` 想一想\
                     「删它」的代价是不是又涨了一格；\n\
                     **从 On 变成 Off** ⇒ 🔴 **这多半是好事**：那条路少了一个真正的生产入口，\
                     回 `U8c-3` 重裁 —— 别只改这张表。\n\
                     ⚠ 它今天靠谁跑：{who}\n\
                     ⚠ 本条**不做可达性分析**，只数「那几个导出符号在别的生产 TS 里出现过没有」。"
            );
        }

        // ⑤ 🔴🔴 〔`K-R109` 09-13〕**尺子A 反向闭合** —— 座今天有哪几个生产消费者，
        //    由**遍历**说了算，不由登记说了算。
        //
        // # 为什么非补不可（这是本轮现打出来的一个洞，不是顺手加的一格）
        //
        // 上面 ② 只对**登记在案**的 `(符号, 文件)` 对逐格比数 ⇒ 它逮得住「少一处」，
        // **逮不住「多一处」**：把 `SESSION_BACKEND.attach` 写回任意一个**没登记**的
        // 生产文件里，② 一格都不响（那个文件根本不在它的循环里）。
        // ⇒ 「座的生产消费者从 3 个文件收到 2 个」这句话，在本轮之前**没有任何东西钉着**：
        //    收窄它不会红（那是 ② 的活），而**反悔**同样不会红。
        //
        // # 它守什么、不守什么
        //
        // **守**：座的生产消费者集合 == 本表登记的那几份。少一份（有人接走了）红、
        // **多一份**（有人把语法又写回前端某处）也红 —— 后者正是 ② 的盲区。
        // **不守**：别名 import（`import { SESSION_BACKEND as X }`）与动态取属性 ——
        // 与尺子A 同一条射程，不声称堵住。座自己（`session-backend.ts`）不在人群里：
        // 它是**被问的那一层**，口径与 `doc_claim_registry` 量法 ② 逐字同源。
        {
            const SEAT: &str = "SESSION_BACKEND";
            let mut registered: Vec<&str> = TS_FALLBACK_KEEPERS
                .iter()
                .filter(|(sym, ..)| *sym == SEAT)
                .map(|(_, f, ..)| *f)
                .collect();
            registered.sort_unstable();
            registered.dedup();
            // 反空真：登记侧空了，下面那条相等就是 `[] == []`，把座删光都绿。
            assert!(
                !registered.is_empty(),
                "`TS_FALLBACK_KEEPERS` 里一条 `{SEAT}` 的登记都没有 —— \
                     本条此刻是空真。**掉到 0 那天**是 `U8c-3` 该重裁的那天，\
                     不是把这一格删掉的那天。"
            );
            let mut found: Vec<&str> = corpus
                .iter()
                .filter(|(rel, _)| rel != "src/session-backend.ts")
                .filter(|(_, code)| guard_core::contains_word(code, SEAT))
                .map(|(rel, _)| rel.as_str())
                .collect();
            found.sort_unstable();
            assert_eq!(
                found, registered,
                "座（`{SEAT}`）的**生产消费者文件集**与 `TS_FALLBACK_KEEPERS` 的登记对不上。\n\
                     遍历实得：{found:?}\n登记：{registered:?}\n\
                     **实得多出来的** ⇒ 有人把会话后端的语法又写回前端某处了 —— \
                     那是 §31 最终形态第①条禁的事，先问它能不能改成问后端要\
                     （本机那一句今天有：`history.rs::render_local_attach`）。\n\
                     **实得少一个** ⇒ 一个消费者接走了，把登记那一行删掉，\
                     并回 `U8c-3` 看看「删座」的代价是不是又低了一格。\n\
                     ⚠ 本条与 ② 是**两个方向**：② 逮「少」，本条逮「多」。\
                     `K-R109` 之前只有 ②，于是「消费者从 3 收到 2」这句话反悔不会红。"
            );
        }
    }
    // ④ 承接方那两个文件本身还在（`U8c-3` 的顺序是「先有承接方，再删旧的」）。
    for f in ["src/launch-render-fallback.ts", "src/session-backend.ts"] {
        assert!(
            repo_root().join(f).is_file(),
            "`{f}` 没了，而 ② 那几个消费者还在 —— **有人先删了承接方**。"
        );
    }
}

/// 🔴 **`KR59D2` 的死值验落点：那份换人手续不许被悄悄撕掉。**
///
/// # 它为什么是一条独立的判据
///
/// `KR59D2` 逐字要的是：「把那条判据整条删掉、别的都不动 ⇒ **必须有东西红**
/// （若没有，说明「前提没了」这件事在盘上真的没有任何痕迹 —— 那正是本条要治的）」。
///
/// 上面那条墓碑自己做不到这件事：删掉它，它就不再运行，也就不再说话。
/// ⇒ 由**本条**在旁边看着它。**两条一起删**才能静默 —— 而那已经不是「别的都不动」了。
///
/// # 它防的那个活体
///
/// `RELAY_KEEPS_THE_OLD_PATH` 犯过同形的病：退役条件悬空指向一个**已经被删掉的东西**
/// （`shared/ccm`），没人发现。本条断的正是「指着的那几样今天都还在盘上」。
#[test]
fn the_retired_premise_left_a_tombstone_that_is_still_on_the_board() {
    let me = include_str!("../../../../src/bridge/src/backend/control/launch_wire.rs");
    // 地板：切不到语料时下面几条会零命中地绿。
    assert!(
        me.len() > 20_000,
        "只读到 {} 字节的本文件 —— 本条在空转",
        me.len()
    );
    // 🔴🔴 **针一律现拼，一个都不许写成整串字面量 —— 这一条本轮实测栽过两次。**
    //
    // ① 第一版把墓碑那个函数名连着 `fn ` 前缀写成一整串字面量，
    //    而**那串字面量自己就住在本文件里** ⇒ `me.contains(needle)` **恒真**：
    //    死值验把那条判据整条改名，读数是「新红 0」——判据在自己身上空转。
    // ② 改成现拼之后，我又把那串完整字面量抄进了**解释它的注释**里，同一条当场又恒真一次。
    // ⇒ 所以这段解释里也不写完整串（要提就断开写：`fn the_ts_fallback_renderer_now_` ＋ 后半）。
    // 与 `6g` 那族「断言用的子串取自夹具自己的名字」是同一个病。
    let tomb_fn = format!(
        "fn the_ts_fallback_renderer_now_{}",
        "stands_on_its_own_consumers"
    );
    let keepers = format!("TS_FALLBACK_{}", "KEEPERS");
    // 🔴〔`K-R105` 09-13〕**尺子B 那张表也进人群** —— 它与 `KEEPERS` 是同一份手续的两半
    // （一把量「还有谁提到它」，一把量「还站不站在生产路上」），少了任一把，
    // 那句「N 个生产消费者」就又能带着一个尺子的数去说另一个尺子的话。
    let reach = format!("TS_FALLBACK_{}", "REACH");
    for needle in [tomb_fn.as_str(), keepers.as_str(), reach.as_str()] {
        // 🔴〔`K-R105` 09-13〕**整词，不是裸子串。** 本轮死值验实测：给那张表的名字
        // **加一个后缀**（= 把它从人群里拿走最省事的写法），裸 `contains` **照样命中**
        // —— needle 被撑大就溜过去了，正是 `needle_anchor_registry` 头注那张表里
        // F16 那一行的形状。⚠ 这段解释里**不写那个改过的名字**：写了它就成了本文件里
        // 一处「以 needle 打头的更长的串」，恰好把这一条重新变瞎（`6g` 那族）。
        assert!(
            guard_core::contains_word(me, needle),
            "`{needle}` 不在本文件里了 —— **`U8c-3` 的那份换人手续被撕掉了。**\n\
                 08-14 那条前提触发器（`daemonless` 主机需要兜底渲染器）在 09-11 `K-R59` 红了，\n\
                 而它红完之后留下的东西就是那条墓碑 + 那张消费者登记。\n\
                 删掉它们 = 「一个前提没了」这件事在盘上再没有任何痕迹，\n\
                 下一个人读到的会是「兜底渲染器没有存续理由」——**而那是假的**。\n\
                 真要删，先回 `U8c-3` 答出「那条路今天还删不删得」，再连本条一起删。"
        );
    }
    // 🔴 **手续在，不等于手续还在干活。**〔本轮死值验 `M10` 逼出来的：把那半的比对
    //    换成 `.iter().take(0)`，六格全绿 —— 那半当时没有任何东西看着。〕
    //    ⇒ 再断一句：墓碑那个函数体里**真的在整表迭代**那张登记。
    //    ⚠ 射程如实写：它认的是「整表迭代」这一个**形状**（`in <表> {`）。
    //      换一种写法（先 `collect` 再比、或换个循环变量顺序）它就认不出 —— **不声称堵住**。
    let at = me.find(tomb_fn.as_str()).expect("上面刚断过它在");
    let body_end = me[at..].find("\n    }\n").map_or(me.len() - at, |k| k + 6);
    let body = &me[at..at + body_end];
    assert!(
        body.len() > 800,
        "切出来的墓碑函数体只有 {} 字节 —— 本条在空转（「体里没有」与「压根没切出体」同形）",
        body.len()
    );
    let iterating = format!("in {keepers} {{");
    assert!(
        body.contains(iterating.as_str()),
        "墓碑还在，但它**不再逐条比对**那张消费者登记（找不到 `{iterating}`）——\n\
             表留着而比对掏空 = 「兜底渲染器还有 3 + 2 个消费者」这句话从此没人核。\n\
             真要换写法，请连本条一起改（并说明新的形状怎么认）。"
    );
    // 阴性对照：这把尺子不是恒真的。
    // ⚠ **针要现拼**：写成字面量的话它自己就在本文件里，这一条当场自相矛盾
    //   （本轮实测红过一次 —— 与 `daemon-section` 那条「别让路径混进断言」同族）。
    let absent = format!("fn {}", "a_judgement_that_was_never_written");
    assert!(
        !me.contains(absent.as_str()),
        "扫描器恒真 —— 上面几条在空转"
    );
}

/// ★ **F04c 补：`send-keys` 那两个 mode 只许从一个地方发出去。**
///
/// 上面那条不再数 `.call("launch")` 的处数了，于是「谁在发 launch」这件事少了一道账。
/// 本条把它补回来，但量的是**对的东西**：走 daemon 的 `send-keys` 语义
/// （`send-into` / `send-keys-raw`）在生产段只许有**一个**产出点
/// （`daemon_send_keys::mode_for`）—— 多一处就是「同一个决策两份实现」的起点，
/// 而这个决策错了的后果是**把「打断当前回合」变成「提交用户排队的文本」**。
#[test]
fn the_send_keys_mode_names_have_exactly_one_production_home() {
    let raw = format!("\"send-keys-{}\"", "raw");
    let mut homes: Vec<String> = Vec::new();
    let mut stack = vec![std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src")];
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
                continue;
            }
            let src = guard_core::production_code(&std::fs::read_to_string(&p).unwrap_or_default());
            if src.contains(raw.as_str()) {
                homes.push(p.file_name().unwrap().to_string_lossy().to_string());
            }
        }
    }
    assert_eq!(
        homes,
        vec!["daemon_send_keys.rs".to_string()],
        "`send-keys-raw` 这个 mode 名的生产段落点不止一个（或搬走了）：{homes:?}\n\
             它必须只有一个家（`daemon_send_keys::mode_for`）—— 两处就会漂，\n\
             而这个决策漂了的后果是把 `Escape`（打断当前回合）当成「键入并提交」。"
    );
}

// ═══════════════════════════════════════════════════════════════════════
// `K-R89` `KR89D4`：**那条逐字节对拍不许变成自洽夹具，也不许被连量具一起砍**
// ═══════════════════════════════════════════════════════════════════════

/// 对拍那条判据的源码。**编译期嵌进来** —— 文件被删/改名 ⇒ **编译失败**，
/// 不是运行时静默跳过。（同 `launch_payload_parity.rs` 自己对夹具与 TS 那一半的做法。）
const PARITY_SRC: &str =
    include_str!("../../../../src/bridge/src/backend/control/launch_payload_parity.rs");

/// 对拍**左边**那个真相源的源码。同上，编译期嵌。
const GOLDEN_SRC: &str = include_str!("../../../../src/launch-payload-golden.ts");

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
                     逐字禁令住 `src/launch-payload-golden.ts` 的头注。\n\
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
    let end = rest.find("\n    }\n").map(|k| k + 6).unwrap_or(rest.len());
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
        "            if got != want {",
        "            let want = got.clone();\n            if got != want {",
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
/// `src/launch-payload-golden.ts` 是 Rust 那条「两边逐字节同构」对拍的**左边**：
/// 它调**真的** `renderFallback` 产 `fixtures/payload-golden.json`，
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
///   这与 [`the_retired_premise_left_a_tombstone_that_is_still_on_the_board`] 是同一形，
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
                 那是「让 Rust 侧去调 TS 现场生成」的形状，而 `src/launch-payload-golden.ts`\n\
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

    // ⑤ **左边那个真相源本身**：它必须仍然调**真的**生产渲染器，
    //    而不是在 TS 里另抄一份「应该长这样」的字面量。
    assert!(
        GOLDEN_SRC.contains("renderFallback(planOf(c))"),
        "`launch-payload-golden.ts` 不再用真的 `renderFallback` 产黄金串了 ——\n\
             那时左边就从「另一种语言的独立实现」退化成「一份手抄的期望值」。"
    );
    // ⑥ 那条逐字禁令本身还在盘上（`K-R89` 的题面逐字点名它）。
    assert!(
        GOLDEN_SRC.contains("不能让 Rust 侧去调 TS 现场生成"),
        "`launch-payload-golden.ts` 头注里那句逐字禁令被删了 ——\n\
             `KR89D4` 逐字：删了它，下一个人不会知道这条捷径为什么不许走。"
    );
}
