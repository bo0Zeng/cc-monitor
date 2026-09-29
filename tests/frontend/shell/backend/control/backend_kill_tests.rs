//! 〔C4e · 第四波 4C〕**「谁在建 tmux 会话 ↔ 后端 kill 的形状门」那一族判据**。
//!
//! 守的要求：`INVARIANTS §34`（三道门：名字形状 · 身份 · 窗口）在创建那一侧的反面 ——
//! 「建得出来、主路杀不掉」的名字不许被铸出来（F04b / F15 立的那一条，`INVARIANTS §33b` 三问里的创建路径）。
//!
//! 本文件原本挂在杀会话的 monitor 发送端 `backend_kill.rs`〔散文墓碑〕下面，一起住着那个发送端自己的几条单元判据
//! （读 `killed` 那一格 · 两条后端命令的拒绝文案同形）。C4e 把杀会话迁到界面（`src/frontend/ui/tmux-control.ts::killSession`
//! 经通道直接说后端的 `kill`），发送端删了：
//! - 它自己的那几条随它退役 —— 「读 `killed` 不许猜」搬到 TS `decodeKilled`，「拒绝码逐码一句」搬到 TS 那张表
//!   （`tests/frontend/ui/tmux-control.vitest.ts`，码集合取自跨语言金样 `tests/__fixtures__/tmux-control.golden.json`）；
//! - **本文件剩下的两条不跟着走**（它们守的与 monitor 里有没有发送端无关），文件原地不动，改挂在
//!   `backend/control/mod.rs` 的测试段（`kill_name_tests`）。

/// 「创建路径」的登记表：`(路径, 判定, 理由)`。
///
/// # ⚠ 发现机制是**遍历**，这张表只用来**表态**
///
/// F04b 建这条判据时**只读了 TS 那一份**（`isValidNewTmuxName`）—— 于是 F12 的
/// `/full-audit` 逮到 `shared/ccm` 那条创建路径**也允许 `=`**：`--tmux=*` 的取值是
/// `${1#*=}`（剥到第一个 `=`）⇒ `ccm --tmux=proj=x` 建得出 `proj=x`，通道 B 还给它写真
/// `@ccm_sid` ⇒ Gate 2 通过、正常出现在列表里，而「结束会话」永远 `invalid_args`
/// ⇒ **那个会话在 UI 上杀不掉**。（改之前实测：`ccm new --tmux=proj=x --print` 产的就是
/// `new-session -d -s 'proj=x'`。）
///
/// ⇒ F15 把发现机制换成**遍历**：扫全仓生产段里真正产 `tmux new-session` 的文件
/// （摸底实测 **4 个**；收窄前 `new-session` 这个词还会命中测试夹具与 UI 动作 id ——
/// **扫描面画大了会被噪音填满，与画小了一样失去意义**）。
/// 每个都必须在下表里表态：要么**自己校验**禁字集，要么**名字来自已校验的上游**并说清是谁。
#[cfg(test)]
const CREATION_PATHS: &[(&str, CreationVerdict, &str)] = &[
    // 🔴 〔`K-R48` 第二拍 09-11〕原来这里第一条是 `shared/ccm`（bash 的 `case` 校验，
    //    `F15` 给它加的 `=`）。〔用@09-11 `K33`〕那个脚本删了 ⇒ **这条路没有第二个实现了**，
    //    它的原生副本就是下面那条 `control/ccm/plan.rs`。表从 5 条回到 4 条。
    (
        "src/backend/control/launch.rs",
        CreationVerdict::UpstreamValidated,
        "名字来自入方向 `parse_request`，它自己就拒 `:`/`=`（那正是本判据的字符集来源）",
    ),
    (
        // 〔`K-R48` 09-11〕**`ccm` 那条创建路径今天唯一的实现**：`ccm` 变成后端二进制
        // 自己的命令之后，`--print` 吐的那条 tmux 编排与真跑读的是同一个 `Plan`。
        //（第一拍它与 `shared/ccm` 并存、表是 5 条；第二拍脚本删了，回到 4 条。）
        "src/backend/control/ccm/plan.rs",
        // 〔DUP2 · J6〕规则搬进共享 crate 之后，这一条的禁字集字面量不住本文件了 ⇒ 从「自己校验」改记「过上游那一份」，
        //   上游 = 下面理由里点名的 `src/common/gate-core/src/lib.rs`（③b 靠这个点名把那条校验器接回人群）。
        CreationVerdict::UpstreamValidated,
        "显式 `--tmux=<名>` / `--tmux-base=<基名>` 两条都先过 `validate_tmux_name`，它调共享那一份 \
             `src/common/gate-core/src/lib.rs` 的 `new_tmux_name_issue`（禁字集 `NEW_TMUX_NAME_REFUSED` 逐字拒 `* ? . : =`，\
             另拒前导 `-` · 控制符 · 视觉欺骗字符 · 超过 128；〔DUP2〕规则全仓只剩这一份）；\
             派生名那条走 `derive_tmux_name`，它的字符集只放行 `[A-Za-z0-9_-]`，\
             **构造上产不出禁字**。三条入口都由 `control::ccm::plan::tests::a_session_name_that_would_confuse_tmux_is_refused` 钉住",
    ),
    // 🔴 **`K-R104`（09-13）：`src/frontend/shell/src/account_usage.rs` 这一行删了。**
    //    它原来的理由是「探针会话名是 `ccm-usage-<slug>`……且它自己 `kill-session` 收尾」。
    //    今天那两句都不成立了：编排搬上后端帧面之后，**monitor 不再建任何 tmux 会话**
    //    （会话由 `oneshot-session` 原语铸并建，收尾发帧面的 `kill`）。
    //    ⇒ 它不再是一个「创建路径」⇒ 留着就是幽灵条目，而本表的遍历会当场逮住。
    //    ★ 同 `K-R72` 那次逐字：这一改是**结构性强制的随动**，不是顺手删记录。
    // 🔴 〔LR2 2026-09-25〕**`src/session-backend.ts` 这一行删了** —— `设计/00 §2.5 ④` 收官那天删的
    //    就是它（下面那条 Rust 对侧的注释原来逐字预告了这一行）。它原来的理由是「只是渲染器：名字由上游
    //    `mintTmuxName` 产、由 `src/frontend/ui/shell-quote.ts::isValidNewTmuxName` 校验」；那条上游关系今天挂在
    //    下面那条 Rust 创建路径的理由里（③b 要求每条校验器都有创建路径点它的名）。
    (
        // 〔`设计/90 §4 E` 2026-09-19 建 · 步 22b·B 接上生产 · LR2 起是外层三格唯一的家〕
        // 〔MIG-2〕载荷内核搬进后端（`99 §2.1 ⑬`），住址跟着换；把禁字喂进它那一段也跟着搬进后端测试段（见下 ③c）。
        "src/backend/control/launch_render/payload.rs",
        CreationVerdict::ValidatesItselfByAllowlist,
        "〔DUP2 · J6〕界面那一道（TS 的两个会话名谓词）删了，名字的规则只有一份：`TmuxTarget::check` 对**新建**那一格\
             调 `src/common/gate-core/src/lib.rs` 的 `new_tmux_name_issue`（拒 `* ? . : =` · 前导 `-` · 控制符 · 视觉欺骗字符 · 超过 128），\
             对 attach / 送进已有会话调 `existing_tmux_name_issue`（拒绝集 ＋ 非空，V131 ②）；另对 `Raw` 那一支只放行 `[A-Za-z0-9_-]`\
             （裸拼的渲染前提，构造上产不出 `:` `=` `*` `?` `.` 与控制字符）；\
             `@ccm_sid` 另过 `shell_quote_core::session_id_ok`（它是**裸拼**的；〔DUP1〕原先那份 `ccm_sid_safe`〔散文墓碑〕收进共享那一条）。三条都由 \
             `launch_render::tmux_outer_parity::tests::the_rust_side_refuses_what_the_typescript_seat_would_have_concatenated` 钉住",
    ),
];

/// **校验器**登记表：`(路径, 它是谁)`。
///
/// # ⚠ 为什么是两张表
///
/// 第一版我把 `src/frontend/ui/shell-quote.ts` 塞进了 `CREATION_PATHS` —— 而**它不产 `new-session`**，
/// 它是**校验器**。判据当场红（遍历只找到 4 个产出方，登记表却有 5 条）。
/// ⇒ 两张表各司其职：
///
/// - `CREATION_PATHS`：**谁在创建**（发现机制 = 遍历 `tmux new-session`）；
/// - `VALIDATORS`：**谁在校验**（这些文件必须真的拒后端拒的每个字符）。
///
/// ★ 一般化：**「一张表混装两种角色」是它自己会红的那种错** ——
/// 因为两种角色的**发现机制不同**（一个能遍历，一个不能），混在一张表里必然对不上。
/// `(路径, **禁字集表达式的字面量**, 它是谁)`。
///
/// # ⚠ 第二列不是装饰 —— 没有它这条判据是恒真的
///
/// 第一版我写的是 `src.contains('=')`（整个文件里有没有那个字符）。
/// **变异 P1（把 `=` 从 `shared/ccm` 的禁字集里拿掉）当场存活** ——
/// 因为一个 shell 脚本里到处都是 `=`（变量赋值、`--tmux=*`…）⇒ 那个断言**恒真**。
///
/// ★ 「判据自己会不会错」那一问的教科书形态：**它匹配到了别处**。
/// ⇒ 改成钉**禁字集表达式本身**：字面量必须逐字出现在文件里，且它必须含后端拒的每个字符。
/// 两个方向都活：拿掉 `=` ⇒ 字面量不再出现 ⇒ 红；backend 新增禁字 ⇒ 字面量缺它 ⇒ 红。
#[cfg(test)]
const VALIDATORS: &[(&str, &str, &str)] = &[
    // 〔DUP2 · 主会话 09-26 裁 J6〕原来这里两行：`src/frontend/ui/shell-quote.ts`（`[*?=]`，TS 的新建谓词）与 `src/backend/control/ccm/plan.rs`
    //    （`"*?.:="`，后端 `validate_tmux_name` 自己那一份）。规则收成一份进共享 crate 之后只剩下面这一行 ——
    //    禁字集字面量住它，`plan.rs` 与 `payload.rs` 两条创建路径的理由各点它的名（③b 那条边）。
    (
        "src/common/gate-core/src/lib.rs",
        "\"*?.:=\"",
        "`NEW_TMUX_NAME_REFUSED` —— 新建会话名的禁字集，全仓唯一一份（`new_tmux_name_issue` 用它）；\
             刻意写成一个字符串字面量而不是 `matches!(c, '*' | '?' | …)`，就是为了让本判据的第二列钉得住它\
             （钉表达式本身、不钉「文件里有没有那个字符」）",
    ),
];

#[cfg(test)]
#[derive(PartialEq, Eq, Debug)]
enum CreationVerdict {
    /// 这条路径**自己**校验禁字集。⇒ 必须在 [`VALIDATORS`] 里，且那张表的第二列
    /// （禁字集表达式的字面量）要逐字出现在它的源码里。
    ValidatesItself,
    /// 🔴 〔`设计/90 §4 E` 09-19〕**它自己校验，但用的是放行集、不是禁字集。**
    ///
    /// 放行集比禁字集**严格更强**（`[A-Za-z0-9_-]` 在构造上就产不出那几个禁字，
    /// 连控制符和视觉欺骗字符一起挡了），**但它在盘上没有一个禁字集字面量可钉**
    /// ⇒ [`VALIDATORS`] 那张表的第二列对它是空的，硬塞进去只能写一个假字面量。
    ///
    /// ⇒ 换一种钉法：**把那几个禁字真的喂进去，看它拒不拒**（行为对拍，比文本对拍更硬 ——
    /// 文本那条挡的是「表达式被改了」，这条挡的是「它真的放过了某个字符」）。
    /// 落点在本判据 ③c 那一段，带正控。
    ValidatesItselfByAllowlist,
    /// 名字来自已校验的上游 ⇒ 本路径不必再校验，但**必须说清上游是谁**。
    UpstreamValidated,
}

/// ★★ **创建路径不许铸出主路杀不掉的名字** —— 发现机制是遍历，不是手写清单。
#[test]
fn no_creation_path_can_mint_a_name_the_main_path_cannot_kill() {
    let root = crate::guard_support::repo_root();

    // ── ① 反向锚点：backend 那条形状门还在（它没了本判据就在空转）──────────
    let kill_prod =
        guard_core::production_code(include_str!("../../../../../src/backend/control/kill.rs"));
    // 〔TAIL · DUP3 §5 ③〕`=` 不再拒（`=a=b:` 精确命中名叫 `a=b` 的会话）⇒ 字符集只剩 `:`。
    let forbidden: Vec<char> = [':']
        .into_iter()
        .filter(|c| kill_prod.contains(&format!("name.contains('{c}')")))
        .collect();
    assert_eq!(
        forbidden,
        vec![':'],
        "backend 的 `parse_name` 不再拒 `:` 了 —— 本判据的字符集来源变了，回来重裁"
    );

    // ── ② 遍历：谁在生产段真正产 `tmux new-session` ────────────────────────
    // ⚠ 模式收窄到 `tmux new-session` 与 argv 形态；**只写 `new-session` 会命中
    //   测试夹具与 UI 动作 id**（摸底实测：宽模式 10 个文件，收窄后 4 个）。
    let verb = format!("new-{}", "session");
    let wide = format!("tmux {verb}");
    let argv = format!("\"{verb}\", \"-d\"");
    let mut found: Vec<String> = Vec::new();
    let mut scanned = 0usize;
    let mut stack: Vec<std::path::PathBuf> =
        // 〔搬树 2026-09-17〕**这里没有 `"src/backend"`，不是漏了**：后端树搬到
        // `<repo>/src/backend` 之后它已经是 `"src"` 的**子目录**，两个都列会把
        // 后端的每个文件数两遍（搬家前 `src/backend` 与 `src` 是互斥的）。
        // 〔搬 src-tauri 2026-09-17〕**这里没有 `"src/frontend/shell/src"`，不是漏了**：
        // 它已经是 `"src"` 的子目录，两个都列会把每个文件数两遍。
        ["src", "shared"]
            .iter()
            .map(|d| root.join(d))
            .collect();
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
            let name = p
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            if name.contains(".test.") || name.contains(".vitest.") {
                continue;
            }
            let ext = p.extension().and_then(|x| x.to_str()).unwrap_or("");
            if !matches!(ext, "rs" | "ts" | "sh" | "") {
                continue;
            }
            let Ok(raw) = std::fs::read_to_string(&p) else {
                continue;
            };
            scanned += 1;
            let body = if ext == "rs" {
                guard_core::production_code(&raw)
            } else {
                raw
            };
            let hit = super::creation_detect::creates_a_session(&body);
            if hit {
                found.push(
                    p.strip_prefix(&root)
                        .unwrap_or(&p)
                        .to_string_lossy()
                        .replace('\\', "/"),
                );
            }
        }
    }
    found.sort();
    // ★ 抽取器自检：遍历坏了下面几条会零命中地绿。
    assert!(
        scanned >= 300,
        "只扫到 {scanned} 个文件 —— 遍历坏了（四个根目录实测远超 300）"
    );
    let mut registered: Vec<String> = CREATION_PATHS
        .iter()
        .map(|(f, _, _)| (*f).to_string())
        .collect();
    registered.sort();
    // ★★ **「少一处」有两种成因，而它们的处置相反** 〔audit-0805 08-07〕。
    //
    // 发现口径是 `"new-session", "-d"` 这个 argv 形态 —— 它把 `-d` 这个**可选旗标**
    // 当成了识别特征。08-07 实测：把 `launch.rs` 的 `-d` 去掉，本条当场红，
    // 而诊断说的是「那条路没了 ⇒ 删登记」。**照它做就是把一条真实创建路径移出人群**，
    // 于是判据回绿、盲区永久化 —— 一个讲错成因的红灯，比不红更坏。
    // ⇒ 登记在册却掉出人群时，先看它**是不是还在产 new-session**，再给处置。
    let verb_only = registered
        .iter()
        .filter(|r| !found.contains(r))
        .filter(|r| {
            // ⚠ **必须与发现口径读同一段源码**：第一版读整份文件，于是测试夹具里的
            // `new-session` 让「已经改名、真的不再创建」的文件被判成「还在产」——
            // 一条讲错成因的诊断，被它自己的变异当场逮出来（08-07）。
            std::fs::read_to_string(root.join(r))
                .map(|s| {
                    let body = if r.ends_with(".rs") {
                        guard_core::production_code(&s)
                    } else {
                        s
                    };
                    body.contains(&verb)
                })
                .unwrap_or(false)
        })
        .cloned()
        .collect::<Vec<_>>();
    assert!(
        verb_only.is_empty(),
        "\n这些文件**还在产 `{verb}`，只是不再匹配发现口径** `{argv}`：{verb_only:?}\n\
             ⚠ **别删登记** —— 那条路还在。多半是 argv 形态被改了（例如 `-d` 被去掉，\n\
             而 `-d` 正是「后台建会话」本身：没有它 tmux 会去 attach 当前终端，\n\
             backend 那条路上根本没有终端）。\n\
             先确认那个改动是不是本意；是本意就同步改这里的发现口径，不是就改回去。"
    );
    assert_eq!(
        found, registered,
        "\n真正产 `tmux new-session` 的文件与创建路径登记表对不上。\n\
             **多一处** = 新增了一条创建路径没表态 ⇒ 要么让它自己校验禁字集，\n\
             要么写清「名字来自哪个已校验的上游」。\n\
             **少一处** = 那条路没了 ⇒ 删登记。⚠ 但先读上面那条：**还在产 `{verb}` 的**\n\
             属于「形态变了」不是「路没了」，处置相反。\n\
             ⚠ F04b 那版是**手写两个文件名**，于是 `shared/ccm` 整条路径逃出了扫描面（F12 逮到）。"
    );

    // ── ③ 每条创建路径都要有非空理由；自己校验的那条必须在 `VALIDATORS` 里 ──
    for (f, verdict, why) in CREATION_PATHS {
        assert!(!why.trim().is_empty(), "{f} 的理由是空的");
        if *verdict == CreationVerdict::ValidatesItself {
            assert!(
                VALIDATORS.iter().any(|(v, _, _)| v == f),
                "`{f}` 记成「自己校验」，却不在 `VALIDATORS` 表里 —— \
                     那下面那条「真的拒了那些字符」就不会查它"
            );
        }
    }

    // ── ③c 🔴 **放行集那一族：把禁字真的喂进去** 〔`设计/90 §4 E` 09-19〕───────
    //
    // 上面 ③ 要求「自己校验」的必须在 `VALIDATORS` 里，而那张表钉的是**禁字集字面量**。
    // 放行集写法在盘上根本没有那样一个字面量（它说的是「只放行这些」，不是「拒这些」），
    // 硬塞一行进去只能编一个假字面量 —— 那是**为了让尺子读得到而改被测物**，方向反了。
    //
    // ⇒ 这一族改成**行为对拍**：逐个禁字喂进去，必须拒；再喂一个合法名字，必须过。
    // 正控是必需的 —— 只测「该拒的拒了」，渲染器整个坏掉（永远 `Err`）时它也全绿。
    let allowlist_rows: Vec<&str> = CREATION_PATHS
        .iter()
        .filter(|(_, v, _)| *v == CreationVerdict::ValidatesItselfByAllowlist)
        .map(|(f, ..)| *f)
        .collect();
    assert_eq!(
        allowlist_rows,
        vec!["src/backend/control/launch_render/payload.rs"],
        "\n放行集那一族的成员变了。本段是**按人群逐个手接**的（喂字符要拿到那个入口函数），\n         多一个成员就得在这里给它接上一段 —— 否则它会**静默地一条都不被喂**。"
    );
    // 〔MIG-2〕那条放行集的入口（`render_tmux_outer`）搬进了后端 crate，monitor 够不着 ⇒ 「逐个禁字喂进去必须拒 ＋ 合法名字必须过」
    //   那一段搬到它旁边：`tests/backend/control/launch_render/payload_tests.rs::the_tmux_outer_refuses_every_name_the_kill_gate_refuses`
    //   （字符集同样从后端 `kill.rs` 的形状门现抠）。这里核它还在。
    // 运行时读（不是编译期嵌入）：只核那段在不在，不值得一条跨半边的编译期边。
    let backend_side = std::fs::read_to_string(
        crate::guard_support::repo_root()
            .join("tests/backend/control/launch_render/payload_tests.rs"),
    )
    .expect("读不到后端那份 payload_tests.rs");
    assert!(
        backend_side.contains("fn the_tmux_outer_refuses_every_name_the_kill_gate_refuses"),
        "喂禁字那一段在后端测试段里不见了 —— 放行集那条创建路径从此没人喂"
    );

    // ── ③b 🔴 **反方向**：每条校验器都得有一条创建路径指着它 ────────────
    //
    // 〔`K-R105` 09-13，`KR105D4`〕**删创建路径要连它的校验器一起看。**
    // 上面 ③ 走的是「创建路径 → 校验器」那一向：记成「自己校验」就必须在下表里。
    // **反过来没人查** —— 而这两张表的**发现机制不对称**正是漏洞所在：
    // `CREATION_PATHS` 是遍历出来的（少一条会红），`VALIDATORS` 是**手写**的，
    // 少一条不红、**多一条也不红**。
    //
    // ⇒ 活体形状（当年就摆在盘上）：`src/frontend/ui/shell-quote.ts` 这一行当时靠
    // TS 座那条创建路径的理由把它引进来。〔LR2 2026-09-25〕那一天到了：座删了，本条当场按设计要求
    // 「同一拍把这条校验器一起处置」—— 它**还有人在用**（生产 TS 的 `planLauncher` 过它），
    // 于是把 `payload.rs` 那条创建路径的理由写全、点了它的名。原先的推演留档如下：`CREATION_PATHS` 少一行、
    // 遍历那条断言照样绿，而 `VALIDATORS` 里 `shell-quote.ts` 这一行
    // **变成一条守着「没有任何创建路径在用的校验器」的判据** ——
    // 它仍然会逐字检查那个禁字集，仍然全绿，**而它守的东西已经不在人群里了**。
    // 那不是红，是**一格静默的空真**：正是本文件头注那句「一张表混装两种角色」的反面
    //（两张表分了角色，却没人看着它们的**边**）。
    //
    // 量法：一条校验器合法，当且仅当 —— 它**自己就是**一条创建路径（`ValidatesItself`），
    // 或者**某条创建路径的理由里点了它的名**（`UpstreamValidated` 那一族）。
    // ⚠ 「点名」= 那条理由里出现它的仓相对路径。这要求写理由的人把住址写全，
    //   而那本来就是 ③ 那句「必须说清上游是谁」的字面要求。
    for (v, _, who) in VALIDATORS {
        let is_path = CREATION_PATHS.iter().any(|(f, ..)| f == v);
        let named_by: Vec<&str> = CREATION_PATHS
            .iter()
            .filter(|(_, _, why)| why.contains(*v))
            .map(|(f, ..)| *f)
            .collect();
        assert!(
            is_path || !named_by.is_empty(),
            "\n校验器 `{v}` **今天没有任何创建路径指着它**（它是谁：{who}）。\n\
                 ⇒ 这一行还会跑、还会绿，而它守的那条路已经不在 `CREATION_PATHS` 里了 ——\n\
                 **一条守着不存在的人群的判据，比没有判据更坏**（它让人以为这一格有人看着）。\n\
                 · 是**刚删掉一条创建路径**？那就同一拍把这条校验器一起处置：\n\
                   还有别人在用 ⇒ 把那个用它的创建路径的理由写全（点它的名）；\n\
                   没人用了 ⇒ 连这一行一起撤，并回 `INVARIANTS §33b` / `U8c-3` 说明。\n\
                 · 是**新加了一条校验器**？那它守的创建路径是哪一条 —— 先把那条登记上。\n\
                 ⚠ 本条**不**替你判「该留还是该删」，它只保证那个决定**必须被做一次**。"
        );
    }

    // ── ④ 校验器必须真的拒后端拒的每个字符 ───────────────────────────
    assert!(
        !VALIDATORS.is_empty(),
        "校验器表空了 —— 下面这段会零命中地绿"
    );
    for (f, class_expr, who) in VALIDATORS {
        assert!(!who.trim().is_empty(), "{f} 没说它是谁");
        let src = std::fs::read_to_string(root.join(f))
            .unwrap_or_else(|_| panic!("读不到 {f} —— 读不到的文件只会静默返回空串"));
        // ★ 钉**禁字集表达式本身**，不是「文件里有没有那个字符」——
        //   后者对 shell 脚本恒真（变异 P1 当场存活，见 `VALIDATORS` 头注）。
        assert!(
            src.contains(class_expr),
            "校验器 `{f}` 里找不到禁字集表达式 `{class_expr}` ——\n\
                 要么它被改了（那就同时改这张表，并想清新表达式还拒不拒 {forbidden:?}），\n\
                 要么**某个禁字被拿掉了** ⇒ 这条路径能铸出一个**建得出来、主路杀不掉**的名字\n\
                 （backend 的 kill 形状门拒它，且**按设计不回落**）。"
        );
        for c in &forbidden {
            assert!(
                class_expr.contains(*c),
                "`{f}` 的禁字集表达式 `{class_expr}` 里没有 `{c}` —— \
                     backend 的 kill 形状门拒它，而这条创建路径放它进来"
            );
        }
    }
}

/// ★ **前提触发器：耐久文档里那句「过渡期回落」不许比代码活得久。**
///
/// # 为什么专门给一句文档配一条判据
///
/// F07 顺出的一般化：**「状态列」与「实测答案」是耐久文档里最易腐的两种字段** ——
/// 它们描述**当下**，而文档寿命比「当下」长。F04b 自己就撞到四处：
/// `IPC-PROTOCOL` 说 kill 的 shell 路是主路（已降为回落）·
/// `INVARIANTS §A5` 说 kill「无此白名单」（**自 F04 起就假了**）·
/// `INVARIANTS §34` 说三道门住 `tmux.rs`（主路那份已在后端）·
/// 用量方案文档说 kill「backend 不参与」。
///
/// 处置不是「以后记得更新」，是**配一条触发器**：本条把那句话与
/// 「回落这段代码到底还在不在」绑在一起。F11 删回落时它会主动红，
/// 逼人回来把那句话一起改掉。
///
/// 🔴 **`K-R72`（09-12）：它真的响了，而且响得对。**
/// 那一刀删掉 `kill_remote_tmux` 的一次性 SSH 回落，本条**当场红**，
/// 逼着把 `src/doc/IPC-PROTOCOL.md` 那两处「过渡期」的说法一起改成「已删」。
/// ⇒ 今天两侧都是 `false`：代码里没有回落，文档里也不再说有。
/// **本条不因此作废** —— 它两个方向都咬：谁把回落加回来不改文档、
/// 或谁把那句话写回文档而代码里没有，都会红。
///
/// # 🔴🔴 `K-R106`（09-13）`KR106D3`：**人群从一份文档扩到整棵 `doc/`**
///
/// 本条此前只 `include_str!` **一份** `src/doc/IPC-PROTOCOL.md` ——
/// 而同一句话当时在盘上还有**另外三份副本**，它们**结构上够不着**：
/// `src/doc/CONTRIBUTING.md`（正文 ＋ 同节表格两处）· `src/doc/ARCHITECTURE.md` ·
/// `doc/账号用量-usage抓取方案.md`。`K-R72` 那次「响得对」只响到了它看得见的那一份，
/// 于是它逼人改的也只有那一份 —— **一条判据挡住的，只有它人群里的那些**。
///
/// ⇒ 发现机制从**一个 `include_str!`** 换成**遍历 `doc/`**（同本文件
/// `CREATION_PATHS` 那条的做法：人群靠遍历发现，不靠手写清单）。
/// ⚠ 这是**扩扫描面 = 更严**，不是放宽闸：两个方向都还咬，只是够得着的人多了。
///
/// # ⚠ 诚实边界（两侧都写出来，别读大）
///
/// - **人群是 `doc/` 这棵树**，按「耐久文档的家」这条语义划，不是「碰巧只有它们长这样」。
///   仓根那几份 `.md`（`README*` / `CHANGELOG` / 复盘报告）与 `evidence/` **不在人群里**：
///   前者不是耐久设计文档；后者是**死值验留档**，逐字记着历史上那一刀砍的是什么，
///   它**本来就该**提到那句话（现打 09-13：`tests/evidence/K-R72-deathvalue.md` 正是这一形）。
///   ⇒ 把它们扫进来买到的不是更严，是一条必然误报的闸。
/// - **它按整串 `contains` 判** ⇒ 想在耐久文档里给这句话立一块**墓碑**（「历史上有过、
///   已经删了」）就会被它拦下。今天的出路是**换一种说法**（本轮三份副本都是这么改的）。
///   这是它已知的代价，不是没看见。
/// - **它不判那三份副本说得对不对** —— 只判「那句话在不在」与「代码里那条路在不在」一致。
#[test]
fn the_doc_sentence_about_the_transitional_fallback_cannot_outlive_the_code() {
    // 〔C4e · 第四波 4C〕「那条过渡期回落还在不在」原来读的是 `tmux.rs` 里杀会话那条 Tauri 命令的函数体
    //   （体里有没有 `connect_and_exec_cmd`）。那条命令整个迁到界面删了 ⇒ 问题换成**整棵 monitor 生产段**里
    //   还有没有一处自己拼杀会话的 shell 串（`kill-session`）—— 界面那一侧结构上没有 SSH，回落只可能长回 monitor。
    //   正控：同一识别器在后端 `control/kill.rs` 的生产段上认得出那个动词（那是真杀会话的那一处）。
    let verb = ["kill", "session"].join("-");
    let mut monitor_prod = String::new();
    for (_, one_file) in guard_core::scan_tree_excluding(
        &crate::guard_support::repo_root().join("src/frontend/shell/src"),
        &["rs"],
        &[],
    ) {
        monitor_prod.push_str(&guard_core::strip_comment_lines(
            &guard_core::production_code(&one_file),
        ));
    }
    let fallback_alive = guard_core::contains_word(&monitor_prod, &verb);
    assert!(
        guard_core::contains_word(
            &guard_core::production_code(include_str!("../../../../../src/backend/control/kill.rs")),
            &verb
        ),
        "正控失败：后端 `control/kill.rs` 的生产段里认不出 `{verb}` —— 识别器瞎了，上面那个「不在」不可信"
    );

    // ── 人群：遍历 `doc/`（递归），**不是**一张手写清单 ────────────────
    let root = crate::guard_support::repo_root();
    let needle = format!("过渡期{}", "回落");
    let mut scanned: Vec<String> = Vec::new();
    let mut said: Vec<String> = Vec::new();
    let mut stack = vec![root.join("src/doc")];
    while let Some(d) = stack.pop() {
        let rd = std::fs::read_dir(&d)
            .unwrap_or_else(|e| panic!("读不到 {} —— 人群空了本条会零命中地绿：{e}", d.display()));
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
                continue;
            }
            if p.extension().and_then(|x| x.to_str()) != Some("md") {
                continue;
            }
            let rel = p
                .strip_prefix(&root)
                .unwrap_or(&p)
                .to_string_lossy()
                .replace('\\', "/");
            let Ok(text) = std::fs::read_to_string(&p) else {
                panic!("{rel} 读不出来 —— 读不到的文件只会静默返回空串");
            };
            scanned.push(rel.clone());
            if text.contains(needle.as_str()) {
                said.push(rel);
            }
        }
    }
    // ★ 抽取器自检：人群塌成 0 时，下面那条相等断言会**空真地**绿。
    assert!(
        scanned.len() >= 8,
        "只扫到 {} 份耐久文档（`src/doc/**/*.md`）—— 遍历坏了，本条此刻在空转：{scanned:?}",
        scanned.len()
    );
    // ★ 地板的第二半：人群里必须**真的有**那份 `K-R72` 逼着改过的文档，
    //   否则「扫到 8 份」也可能扫的是另外八份。
    assert!(
        scanned.iter().any(|f| f.ends_with("IPC-PROTOCOL.md")),
        "人群里没有 `src/doc/IPC-PROTOCOL.md` —— 本条原来唯一看得见的那一份掉出去了：{scanned:?}"
    );
    said.sort();
    assert_eq!(
        !said.is_empty(),
        fallback_alive,
        "代码与耐久文档对不上了：\n\
             · monitor 生产段里还有自己拼杀会话 shell 串的第二条路吗 = {fallback_alive}\n\
             · `doc/` 里还写着那句话的（分母 = 遍历到的 {} 份 `.md`）= {said:?}\n\
             ⚠ 如果是**删掉了那条路**：那句话要一起改，否则下一个读者会以为\n\
             「没有后端的远端」还有一条路可走 —— 而那正是 C7 说的过渡期已经结束。\n\
             ⚠ 如果是**改了措辞**：本条判据跟着改（它钉的是两者一致，不是某个字面量）。\n\
             ⚠ 〔`K-R106` 09-13〕人群是**整棵 `doc/`**，不再只有 `IPC-PROTOCOL.md` ——\n\
             在**任何一份**耐久文档里把那句话写回来，本条都会红。",
        scanned.len()
    );
}

// 〔C4e · 第四波 4C〕这里原来住着「两条后端命令的拒绝文案说同一件事」（`the_refusal_wording_matches_the_sibling_command`〔散文墓碑〕，
//   对照 monitor 里杀会话与送键两个发送端各自那份 `refusal_text`）。两个发送端都迁到界面删了，
//   拒绝码 → 一句话从此只有 `src/frontend/ui/tmux-control.ts` 一份（按动作分表：结束会话 · 发按键），
//   「逐码一句、两两不同、带会话名与后端原话」由 `tests/frontend/ui/tmux-control.vitest.ts` 钉着。
