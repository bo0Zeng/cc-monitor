/// 本模块的**全部启动入口**。两条判据共用这一份人群
/// （`the_startup_path_really_calls_this_module` 与 `the_production_entry_hands_the_stdio_consumer_down`）
/// —— 各存一份迟早分叉：新增入口时只想得起改一处。
const ENTRIES: &[&str] = &["start_if_present", "start_or_extract"];

// ── `K-R28`：起后端那一跳的两件事，各自一条判据 ────────────────────────
// 下面四条按「哪一半」分：①判别 ②重试 ③上限 ④两句话。
// 名字里说得出它断的是哪一半，坏了一条就知道坏在哪儿。
// ⚠ 一条都**不起真进程**：`spawn_retrying_etxtbsy` 把 spawn 与 sleep 都做成了注入点，
//   正是为了这个。

/// **判别那一半**：认的是 `os error 26`（errno），不是 `Text file busy`（locale 翻译）。
///
/// 🔴 **含本半的反向自检**：判别器**不是恒真也不是恒假** —— 下面那个 `assert_ne!`
/// 就是那一格。没有它，把函数体换成 `true` 或 `false` 都能让上面几条里的一半照样绿。
#[test]
fn the_etxtbsy_verdict_reads_the_errno_half_not_the_localised_half() {
    // 英文 locale 下 glibc 打出来的那一份（`launch.rs` 那条判据实测到的逐字）。
    let english = "spawn 本地命令失败: Text file busy (os error 26)";
    // 中文 locale 下同一个 errno —— 后半句被翻译了，前半句没有。
    let chinese = "spawn 本地命令失败: 文本文件忙 (os error 26)";
    // 只有被翻译的那半句、没有 errno ⇒ **不许认**（那才是「隐式的 locale 前提」）。
    let only_the_prose = "spawn failed: Text file busy";
    // 另一个真失败：这台机器上它就是起不来。
    let real_failure = "spawn 本地命令失败: Permission denied (os error 13)";

    assert!(
        spawn_error_is_etxtbsy(english),
        "英文 locale 的 ETXTBSY 没认出来"
    );
    assert!(
        spawn_error_is_etxtbsy(chinese),
        "换个 locale 就认不出来了 ⇒ 判据挂着一条隐式的 locale 前提，正是本件在治的病"
    );
    assert!(
        !spawn_error_is_etxtbsy(only_the_prose),
        "认了被翻译的那半句 —— 那半句由 C 库按 LC_MESSAGES 打，不是判据该抓的东西"
    );
    assert!(
        !spawn_error_is_etxtbsy(real_failure),
        "一个真失败被读成了「会自己过去的竞态」⇒ 它会被重试，偶发红比今天更糟"
    );
    // 反向自检：两张脸必须真的不同答案。恒真 / 恒假在这一格上当场红。
    assert_ne!(
        spawn_error_is_etxtbsy(english),
        spawn_error_is_etxtbsy(real_failure),
        "判别器对两种性质相反的错给了同一个答案 ⇒ 它是恒答的，一件事都没分开"
    );
}

/// **重试那一半**：只对 `ETXTBSY` 重试；别的错**一次都不重试**。
///
/// 三腿：①一直撞 ⇒ 试满并读成「这一刻恰好撞上了」；②真失败 ⇒ **第一次就放弃**
/// （这条是 `acceptor 怎么失效` 点名要的那个非 `ETXTBSY` 对照）；③中途成功 ⇒ 就此打住。
#[test]
fn only_etxtbsy_is_retried_a_real_failure_gives_up_on_the_first_try() {
    use std::cell::Cell;

    // 腿①：一直撞 ETXTBSY。
    let tries_used = Cell::new(0u32);
    let busy: Result<(), SpawnFailure> = spawn_retrying_etxtbsy(
        5,
        || {
            tries_used.set(tries_used.get() + 1);
            Err("spawn 本地命令失败: Text file busy (os error 26)".to_string())
        },
        |_| {},
    );
    assert_eq!(tries_used.get(), 5, "上限是 5，却没试满");
    let Err(SpawnFailure::TransientBusy { tries, last }) = busy else {
        panic!("一直 ETXTBSY 却没读成「这一刻恰好撞上了」：{busy:?}");
    };
    assert_eq!(tries, 5);
    // 逐字相等，不是 `contains`：
    // ① 最后一次的原话必须带回来（不带回来，用户与日志都问不到成因）；
    // ② 🔴 **它只许装那一句原话** —— 本轮自查逮到的正是这一处：初版把
    //    「撞了几次」也揉进了 `last`，而那个数住 `tries` ⇒ 同一个事实两份表示，
    //    实打出来是「…最后一次逐字：撞了 8 次，最后一次逐字：Text file busy…」。
    //    **本件在治的那条病，长在治它的代码里。** 钉住它别回来。
    assert_eq!(
        last, "spawn 本地命令失败: Text file busy (os error 26)",
        "`last` 不是那一句原话 —— 要么没带回来，要么被加工了（比如又把次数揉了进来）"
    );

    // 腿②（**反侧对照**）：一个真失败 —— 一次都不许重试。
    let real_calls = Cell::new(0u32);
    let naps = Cell::new(0u32);
    let broken: Result<(), SpawnFailure> = spawn_retrying_etxtbsy(
        5,
        || {
            real_calls.set(real_calls.get() + 1);
            Err("spawn 本地命令失败: Permission denied (os error 13)".to_string())
        },
        |_| naps.set(naps.get() + 1),
    );
    assert_eq!(
        real_calls.get(),
        1,
        "一个真缺陷被重试了 —— 那是把它变成偶尔绿的偶发红，比今天更糟"
    );
    assert_eq!(naps.get(), 0, "真失败那一支还睡了一觉 —— 用户白等");
    assert!(
        matches!(broken, Err(SpawnFailure::Broken(_))),
        "真失败没读成「这台机器上它就是起不来」：{broken:?}"
    );

    // 腿③：第 3 次成功 ⇒ 就此打住，不把剩下的额度也用掉。
    let n = Cell::new(0u32);
    let ok: Result<u32, SpawnFailure> = spawn_retrying_etxtbsy(
        5,
        || {
            n.set(n.get() + 1);
            if n.get() < 3 {
                Err("Text file busy (os error 26)".to_string())
            } else {
                Ok(n.get())
            }
        },
        |_| {},
    );
    assert_eq!(ok, Ok(3), "中途成功却没就此返回：{ok:?}");
    assert_eq!(n.get(), 3);
}

/// **上限那一半**：生产段的额度必须够重试、又必须留在「用户还觉得是瞬间」那一档里。
///
/// 🔴 这条刻意**不去核那个数等于 8** —— 那样它只是把常量抄了第二遍
/// （闭集只许有一个住址，那个住址是 `SPAWN_ETXTBSY_TRIES` 自己）。
/// 它核的是那个数**买到的两条性质**：够不够重试、会不会退化成测试台那种「慢慢等」。
#[test]
fn the_retry_budget_is_big_enough_to_retry_and_small_enough_to_wait_on() {
    use std::cell::Cell;

    assert!(
        SPAWN_ETXTBSY_TRIES >= 2,
        "上限是 {SPAWN_ETXTBSY_TRIES} —— 小于 2 = 认得出但从不重试，这一半等于没做"
    );
    // 生产段每一次重试都是一趟真的 fork+execve（几百微秒量级）⇒ 额度不许放成测试台那种。
    // `launch.rs` 的 50 是**测试台**口径（它可以慢慢等），照抄进来就是让用户等。
    assert!(
        SPAWN_ETXTBSY_TRIES <= 16,
        "上限放到了 {SPAWN_ETXTBSY_TRIES} —— 生产段是用户在等一个窗口，\
             不是一条可以慢慢等的判据。`launch.rs` 那个 50 是测试台口径，别照抄进来"
    );

    // 最后一次失败之后不叫 `backoff` ⇒ 它恰好被调用 `tries - 1` 次。
    // （那个注入点今天在生产段是空的，理由见 `spawn_with_etxtbsy_retry` 头注；
    //   但「不在最后一次之后白等」这条性质要现在就钉住，接上去那天才不会带着一处白等。）
    let naps = Cell::new(0u32);
    let _: Result<(), SpawnFailure> = spawn_retrying_etxtbsy(
        4,
        || Err("Text file busy (os error 26)".to_string()),
        |_| naps.set(naps.get() + 1),
    );
    assert_eq!(
        naps.get(),
        3,
        "两次之间那一步被调了 {} 次而不是 3 次 —— 最后一次失败之后那一次没人会用到，\
             而它是用户真的在等的时间",
        naps.get()
    );
}

/// ★ `K-R30`（`KR30D3`，acceptor: 机检）：**那个数的出处钉**。
///
/// 上面那条判据核的是这个数**买到的两条性质**（够不够重试 · 会不会退化成慢慢等），
/// 而 `SPAWN_ETXTBSY_TRIES` **这个值本身**在 `K-R28` 交出来时没有任何读数撑着 ——
/// 那一节写的是**理由**。`K-R30` 在沙箱里真造出那个竞态量了一趟，把读数写进了它的头注。
/// 本条钉的是那段出处与那个数**同生共死**：出处删了 ⇒ 红；数改了而出处没改 ⇒ 也红。
///
/// 🔴 它**不**核「那个数等于 8」—— 那样又是把常量抄第二遍（上面那条的头注逐字写过
/// 为什么不许）。它核的是**出处段里逐字写着的那个数就是常量今天的值**：needle 由常量现拼。
#[test]
fn the_retry_budget_number_has_a_measured_origin_pinned_to_it() {
    let whole = include_str!("../../../../src/bridge/src/backend/control/local_backend.rs");
    // ⚠ 定位串**运行时拼**，而且名字取自 `stringify!` 而不是又抄一个字面量：
    //   ① 直接写全串会命中**本条自己**（初版实测「命中 2 处」当场红 —— F23 那一族，
    //      本文件的 `C12` 源码钉早就是这么写的）；② `stringify!` 让这里零字面量复述。
    let decl_needle = format!("pub const {}: u32 =", stringify!(SPAWN_ETXTBSY_TRIES));
    // ① 反向自检：先断言真取到了那段头注 —— 取不到就会在空串上恒真地全绿。
    let decl = guard_core::find_pinned(whole, &decl_needle)
        .expect("那个常量的定义不在了 —— 改了名的人请顺手改本条");
    let head = &whole[..decl];
    let at = guard_core::find_pinned(head, "〔出处·K-R30〕").unwrap_or_else(|e| {
        panic!(
            "这个数的**出处段**不在它头注里了（{e}）。\n\
                 `K-R30` 之前它只有理由、没有读数；出处一删，它当场退回「一个判断」。\n\
                 要换出处就连读数一起换（量具 tests/evidence/K-R30-etxtbsy-window.py）。"
        )
    });
    let origin = &head[at..];
    assert!(
        origin.len() > 300,
        "切出来的出处段只有 {} 字节 —— 切错了，下面两条会在一小截文本上恒假",
        origin.len()
    );
    // ② 那个数与出处**同生共死**：needle 由常量现拼，改了值这里当场找不着。
    let pinned = format!("{SPAWN_ETXTBSY_TRIES} 次立即重试打完");
    guard_core::find_pinned(origin, &pinned).unwrap_or_else(|e| {
        panic!(
            "出处段里没有一处逐字写着「{pinned}」（{e}）。\n\
                 ★ 改了 `SPAWN_ETXTBSY_TRIES` 就必须同改那一节 —— 一个没有出处的数，\
                 下一个人只能照抄，而抄一次漂一次。"
        )
    });
    // ③ 出处要给得出**住址**，否则它只是一句好听的话：读数得能重跑。
    guard_core::find_pinned(origin, "K-R30-etxtbsy-window.py").unwrap_or_else(|e| {
        panic!("出处段没给量具的住址（{e}）—— 给不出住址的读数，下一轮没人复得出来")
    });
}

/// ★★ `K-R31`（`KR31D2`，acceptor: 机检）：**撑着那个数的那条外部前提的判据**。
///
/// # 上面那条钉「有出处」，本条钉「那个出处**还成立**」
///
/// [`SPAWN_ETXTBSY_TRIES`] 那节读数量的是一个**特定形状的孩子**：`fork` 之后
/// **一步不干**、直接 `execve`。那一档窗口才停在几百微秒，上限才盖得住。
/// 孩子中间要干活时（量具的对照臂：一个 CPython 子进程），那个上限**当场盖不住** ——
/// 分子分母逐字写在那节出处里。
///
/// 🔴 而在本条之前，**加一处 `pre_exec` 编得过、跑得过、门禁一个数都不动** ——
/// 改它的人**不会知道自己动了什么**。⇒ 本条把那句话变成会红的东西。
///
/// ⚠ **它证的不是「那条前提永远成立」，是「它不成立的那一刻会有人知道」。**
/// 这两句不许压成一句。
///
/// # 🔴 单位：**行**。不是「处」，也不是「块」
///
/// 立本条时那句话在盘上是 **3 行**（同一段 `///` 里连着的三行）；按相邻块算是 **2**；
/// 按「一段文档注释」算是 **1** —— `K-R30` 头注写的「1 处」是第三种读法，
/// 而它**没写死是哪一种**。本条按**行**判，并把三个数一起印进失败文案，歧义到此为止。
///
/// # 它扫的是什么、扫不到什么
///
/// 扫描面 = `src/bridge/src` · `src/backend` · `src/bridge/crates` 三棵树的
/// `.rs`，逐份过 [`guard_core::production_code`]（剥测试段 + 剥块注释 + 剥整行与行尾 `//`）。
/// ⇒ 本条那张形态表住 `#[cfg(test)]` 里、那几段前提住 `///` 里，**按构造都进不了扫描面**
/// —— `scanning_guard_registry` 头注四类里的第二类（「剥生产段（构造性摘除）」）。
/// ⚠ 那份登记治的正是「**判据在自己的注释 / 登记表 / 常量里找到了自己 ⇒ 恒绿**」，
/// 而本条是那一族的第 N 个：不剥注释的话，**本文件上面那段头注自己就是第一处命中**
/// （`K-R30` 自查逮到过一次，提交 `494ad4c`）。
///
/// 🔴 `scan_tree!` 按构造**摘除调用者自己那一份**，而最可能长出这种写法的恰恰是本文件
/// （生产段起进程那一跳就在这儿）⇒ 本文件另走 `include_str!` **单独喂一遍**。
/// **别把那一份删了** —— 删了之后本条看起来和没删一模一样。
///
/// ⚠ **形态表是枚举，不是全称**：表外的写法（自己 `clone(2)` · 换一个装 fd 的 crate ·
/// 走 `nix`）**本条一个都看不见**。这条边界也写进失败文案，别读成「这一族已经封死」。
/// 立本条时逐条现打过一趟：除 `pre_exec` 那 3 行注释外，表里其余六种**全树零命中**
/// （分母 216 份 `.rs`，量具 `tests/evidence/K-R31-fork-exec-forms.py`）。
///
/// # 反向那半（没有它，本条会在空串上恒真地绿）
///
/// ① **阳性对照**：同一把探测器喂一段真写法 ⇒ 必须命中（形态表写错一个字母当场红）；
/// ② **阴性对照**：同一段包进行注释 / 块注释 ⇒ 必须**不**命中（剥注释那步真的在做）；
/// ③ **扫描面地板**：份数与字节数（遍历坏了 ⇒ 红，而不是静默变绿）；
/// ④ 本文件剥完**不许还残留测试属性**（剥法坏了 ⇒ 形态表自己就进了扫描面）。
#[test]
fn nothing_in_the_production_path_runs_code_between_fork_and_exec() {
    // 「在 fork 与 exec 之间插一段代码」的写法。**枚举，不是全称**（见头注）。
    const FORMS: &[&str] = &[
        "pre_exec",          // std `CommandExt::pre_exec`：闭包在孩子里、`execve` 之前跑
        "before_exec",       // 同一件事的旧名（已弃用，仍编得过）
        "libc::fork",        // 手写 fork+exec ⇒ 中间那段全归调用方
        "libc::vfork",       // 同上，且它连地址空间都不换
        "command_fds",       // 那个 crate 装 fd 用的就是 `pre_exec`
        "CommandFdExt",      // 同上，trait 名那一半
        "libc::posix_spawn", // 手写 file actions：同一段窗口，只是搬进了 libc
    ];

    /// 相邻的命中行合成一块 —— 「处」那个单位的一种**可判定**读法。
    fn count_blocks(h: &[(usize, String)]) -> usize {
        let mut n = 0usize;
        let mut prev: Option<usize> = None;
        for (i, _) in h {
            if !matches!(prev, Some(p) if *i == p + 1) {
                n += 1;
            }
            prev = Some(*i);
        }
        n
    }

    // 探测器：一份文本剥完之后仍命中的**行**。诊断带**逐字行内容**当校验位而不带行号 ——
    // `production_code` 会删整行注释，剥后的序号**不是原文行号**，写出来就是一个假住址。
    let probe = |rel: &str, src: &str| -> (usize, Vec<(usize, String)>) {
        let prod = guard_core::production_code(src);
        let mut out = Vec::new();
        for (i, l) in prod.lines().enumerate() {
            if let Some(f) = FORMS.iter().find(|f| l.contains(**f)) {
                out.push((i, format!("{rel}  〔{f}〕 {}", l.trim())));
            }
        }
        (prod.len(), out)
    };

    // ① 阳性对照：形态表 + 剥法**有牙**。没有它，把 `FORMS` 写错一个字母也照样全绿。
    let live = "    unsafe { cmd.pre_exec(|| Ok(())) };";
    assert_eq!(
        probe("〔阳性对照〕", live).1.len(),
        1,
        "阳性对照没被逮到 —— 形态表或剥法坏了，下面那条此刻是空转的"
    );
    // ② 阴性对照：注释里的同一段**不许**算命中。三种注释形态各一行
    //    （整行 `//` · 块注释 · **缩进过的** `///` —— 最后这种正是 `§0b` 记的那个洞：
    //    PM 立件时用的 `grep -v ':[0-9]*://'` 只剥顶格 `//`，缩进的剥不掉）。
    let commented = "// unsafe { cmd.pre_exec(|| Ok(())) };\n\
                         /* cmd.pre_exec(); */\n\
                         \x20   /// 那一档靠的是本仓一处 pre_exec 都没有\n";
    let in_comment = probe("〔阴性对照〕", commented).1;
    assert!(
        in_comment.is_empty(),
        "注释里的写法被数成了命中 —— 剥注释那一步没在做：{in_comment:?}\n\
             ⇒ 本条会被**它自己要守的那句话**喂饱（那段头注里逐字写着这些词），于是恒红或恒瞎。"
    );

    let root = crate::guard_support::repo_root();
    // 🔴 `scan_tree!` 摘掉调用者自己那份，而**最该被扫的就是本文件** ⇒ 单独喂一遍。
    let self_rel = "src/bridge/src/backend/control/local_backend.rs";
    let me = include_str!("../../../../src/bridge/src/backend/control/local_backend.rs");
    // ④ 剥法自检：剥完还残留测试属性 ⇒ 上面那张 `FORMS` 表自己就进了扫描面。
    guard_core::assert_no_test_code(self_rel, &guard_core::production_code(me));
    let mut corpus: Vec<(String, String)> = vec![(self_rel.to_string(), me.to_string())];
    for sub in ["src/bridge/src", "src/backend", "src/bridge/crates"] {
        for (f, src) in guard_core::scan_tree!(&root.join(sub), &["rs"]) {
            let rel = f
                .strip_prefix(&root)
                .unwrap_or(&f)
                .to_string_lossy()
                .replace('\\', "/");
            corpus.push((rel, src));
        }
    }

    let (mut files, mut bytes) = (0usize, 0usize);
    let (mut hit_files, mut hit_blocks) = (0usize, 0usize);
    let mut hit_lines: Vec<String> = Vec::new();
    for (rel, src) in &corpus {
        let (n, h) = probe(rel.as_str(), src.as_str());
        files += 1;
        bytes += n;
        if !h.is_empty() {
            hit_files += 1;
            hit_blocks += count_blocks(&h);
            hit_lines.extend(h.into_iter().map(|(_, d)| d));
        }
    }

    // ③ 扫描面地板：**先证明扫到了东西**，否则下面那条在空串上恒真。
    assert!(
        files >= 150 && bytes >= 600_000,
        "扫描面只有 {files} 份 `.rs` / 剥完 {bytes} 字节 —— **遍历或剥法坏了**，\n\
             本条此刻是在空串上恒真地绿（09-06 立本条时沙箱实测 **192 份 / 1137249 字节**，\n\
             那是一个**带日期的快照**，不是一条性质）。\n\
             地板刻意留了余量：它挡的是「**扫不到东西**」，不是「代码变少了」——\n\
             收得太紧的地板会在正常删代码时变红，那是在挡住进步。\n\
             ⚠ 变异实测（`R31M2`）：把扫描面的扩展名换成一个不存在的，本条当场红\n\
             （读到「1 份 / 21937 字节」）—— 那 1 份是本文件自己那一份，它另走 `include_str`。"
    );

    assert!(
        hit_lines.is_empty(),
        "生产段出现了「在 fork 与 exec 之间插一段代码」的写法 —— \
             **{} 行** · **{} 块**（剥后文本里相邻的命中行合成一块）· **{} 份文件**：\n  {}\n\n\
             🔴 **本条判的单位是「行」**：有一行就红。三个数一起印，是因为 `K-R30` 写的\n\
             「1 处」在盘上按行是 3、按相邻块是 2 —— 那个歧义在这里终结，别再用「处」。\n\n\
             ★ 后果不是风格问题，是**一个读数的出处当场不再成立**：\n\
               `{}` 那个上限的出处段量的是「孩子在 fork 与 execve 之间**不干活**」那一档；\n\
               孩子中间还要干活时那个上限**盖不住**（对照臂的分子分母逐字写在那节出处里）。\n\
             ⇒ 处置**二选一，没有第三条**：\n\
               ① 撤掉这处写法；或\n\
               ② **回去重量**（量具住址写在那节出处里），并把 `{}` 与那节出处**一起**改 ——\n\
                  `the_retry_budget_number_has_a_measured_origin_pinned_to_it` 会逼你同改。\n\n\
             （分母：本趟扫了 {} 份 `.rs`、剥完共 {} 字节，扫描面 = 三棵树的**生产段**。\n\
               形态表是**枚举不是全称**：{:?}\n\
               —— 表外的写法（自己 `clone(2)` · 别的装 fd 的 crate · `nix`）本条一个都看不见。）",
        hit_lines.len(),
        hit_blocks,
        hit_files,
        hit_lines.join("\n  "),
        stringify!(SPAWN_ETXTBSY_TRIES),
        stringify!(SPAWN_ETXTBSY_TRIES),
        files,
        bytes,
        FORMS,
    );
}

/// **两句话那一半**：两种结局说给用户听的话**不许是同一句**，也不许互相串。
#[test]
fn the_two_verdicts_hand_the_user_two_different_sentences() {
    // ⚠ 路径取中性名，且下面一条断言都不取自它 —— 免得「输出里含某句话」靠路径恒真。
    let bin = std::path::Path::new("/tmp/ccm-backend-under-test");

    let busy: Result<(), SpawnFailure> = spawn_retrying_etxtbsy(
        3,
        || Err("spawn 本地命令失败: Text file busy (os error 26)".to_string()),
        |_| {},
    );
    let Err(SpawnFailure::TransientBusy { tries, last }) = busy else {
        panic!("一直 ETXTBSY 却没读成「这一刻恰好撞上了」：{busy:?}");
    };
    let transient = etxtbsy_gave_up_reason(bin, tries, &last);
    assert!(
        transient.contains("再开一次多半就好"),
        "撞上竞态那句话没告诉用户「再开一次多半就好」，他仍然不知道该不该重开：{transient}"
    );
    assert!(
        transient.contains("连着 3 次"),
        "没说清试了几次 ⇒ 这句话没法与「一次都没试」区分开：{transient}"
    );

    let broken: Result<(), SpawnFailure> = spawn_retrying_etxtbsy(
        3,
        || Err("spawn 本地命令失败: Permission denied (os error 13)".to_string()),
        |_| {},
    );
    let Err(SpawnFailure::Broken(e)) = broken else {
        panic!("一个真失败被读成了竞态：{broken:?}");
    };
    // 「今天那句照旧」靠的就是这一条：真失败的逐字**原封不动**地留在值里，
    // 生产段那一支才有东西可以原样转给用户。
    assert_eq!(
        e, "spawn 本地命令失败: Permission denied (os error 13)",
        "真失败的逐字被加工了 ⇒ 「别的错今天那句照旧」这句话就不成立了"
    );
    assert!(
        !transient.contains(&e),
        "两句话串了：竞态那句里带上了真失败的逐字：{transient}"
    );
    assert!(
        !e.contains("再开一次"),
        "「这台机器上就是起不来」那句里混进了「会自己过去」的说法：{e}"
    );
}

/// `P2t` 摸底交付的那一刀：**两个进程不写同一个 `.partial`**。
///
/// # 它防的是什么
///
/// `.partial` + `rename` 存在的全部理由是「**半截文件不许被当成可执行的 daemon 起起来**」。
/// 而临时名原来是**固定的** ⇒ 两个同版本 monitor 同时释放会写同一个文件：
/// 一个写到一半、另一个 `rename` 走 —— 出来的正是这道防线要防的东西。
/// ⚠ 不是理论：`tauri_plugin_single_instance` **只在 `#[cfg(windows)]` 注册**
/// ⇒ Linux/macOS 上两个 monitor 天然并存。
///
/// # 🔴 `K-R69` 09-12：**人群从「恰好一处」改成「每一处」**，理由写清楚
///
/// 原来这一条是 `find_pinned(&prod, "std::process::id()")`（要求**恰好一处**）＋
/// 「那一处在 `.partial` 附近」。本轮新增了第二个走 `.partial` + `rename` 的落点
/// （[`install_local_ccm_entry`]）⇒ 它当场红了，报文逐字「命中 2 处，断言指不明是哪一处」。
///
/// **那不是「判据过严」，是它的人群一直写小了**：这条性质从头就该覆盖**所有**临时名落点，
/// 只是当时只有一处，于是「恰好一处」与「每一处」在读数上分不开。
/// ⇒ 改成覆盖式（人群现算），**不是**把 needle 撑大到只认第一处 ——
/// 后者正是本仓那条「匹配单位比事实小」的老病。
#[test]
fn two_processes_do_not_share_one_partial_file() {
    let prod = guard_core::production_code(include_str!(
        "../../../../src/bridge/src/backend/control/local_backend.rs"
    ));
    // 人群**现算**：每一处「`.partial` 结尾的格式串 + 后面跟着实参」。
    // ⚠ 针带逗号是刻意的：`name.ends_with(".partial")`（清扫那一处**读**它）
    //   长得像但不是构造点，带上逗号就分开了。
    assert!(
        prod.split(".partial\",").skip(1).count() >= 2,
        "生产段里构造 `.partial` 临时名的地方少于 2 处 —— 抽取器坏了、或落点改了写法，\n\
             本条此刻**无效**（零命中会让下面那个 for 循环空转变绿）"
    );
    for args in prod.split(".partial\",").skip(1) {
        assert!(
            args.chars()
                .take(160)
                .collect::<String>()
                .contains("std::process::id()"),
            "有一处 `.partial` 临时名里没有本进程 id —— 固定名会让两个 monitor 写同一个文件：\n\
                 一个写到一半、另一个 `rename` 走，出来的正是这道防线要防的**半截可执行文件**。\n\
                 ⚠ 不是理论：`tauri_plugin_single_instance` 只在 `#[cfg(windows)]` 注册 ⇒ \n\
                 Linux/macOS 上两个 monitor 天然并存。实参逐字：{}",
            args.chars().take(160).collect::<String>()
        );
    }
}

/// `P2t`：清扫只收**够老**的残骸，绝不碰新鲜的。
///
/// ★ 这条是行为判据（真的建文件、真的调），不是形状判据 ——
/// 「按年龄判」这种阈值逻辑最容易写反（`>=` 写成 `<=` 一个字符的事）。
#[test]
fn the_sweep_only_takes_the_old_ones() {
    let dir = std::env::temp_dir().join(format!(
        "p2t-sweep-{}-{:?}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let name = "cc-monitor-remote-testbuild";
    // 把 mtime 拨老 48h。⚠ 这一步是**判据的一部分**：首跑时我只放了「新鲜的别人的文件」，
    // 于是把命名法过滤整个删掉**照样绿** —— 年龄检查替它挡了。
    // **两道过滤各自的作用，必须各有一个只有它能挡住的夹具。**
    let age_back = |p: &std::path::Path| {
        let f = std::fs::File::options().write(true).open(p).unwrap();
        let old = std::time::SystemTime::now() - std::time::Duration::from_secs(48 * 3600);
        f.set_times(std::fs::FileTimes::new().set_modified(old))
            .unwrap();
    };

    let fresh = dir.join(format!(".{name}.4242.partial")); // 我们的、新鲜 ⇒ 留
    let stale = dir.join(format!(".{name}.9999.partial")); // 我们的、够老 ⇒ 收
    let other_fresh = dir.join("someone-elses-file"); // 别人的、新鲜 ⇒ 留
    let other_stale = dir.join("someone-elses-old-file"); // 别人的、够老 ⇒ **仍然留**
    for p in [&fresh, &stale, &other_fresh, &other_stale] {
        std::fs::write(p, b"x").unwrap();
    }
    age_back(&stale);
    age_back(&other_stale);

    sweep_stale_partials(&dir, name);

    assert!(
        fresh.exists(),
        "刚写的 `.partial` 被删了 —— 那正是这道防线要防的事：\
             把**正在写的**那份删掉，等于自己制造半截文件"
    );
    assert!(
        !stale.exists(),
        "够老的残骸没被收 —— 那清扫就是个摆设（带 pid 之后它们不会再被覆盖掉）"
    );
    assert!(
        other_fresh.exists(),
        "碰了不属于自己命名法的文件（这个目录与远端自部署共用）"
    );
    assert!(
        other_stale.exists(),
        "把**别人的**老文件也收了 —— 这个目录与远端自部署共用，\
             只有「够老」不构成删的理由，还得是**我们自己命名法**的"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// `P3-Y1` 的**兑现证据**〔08-12 补〕：帧 → 账本这一跳，**不起 daemon、不起 tmux**。
///
/// # 它补的是什么洞
///
/// 下面那条 `the_local_tmux_frames_really_land_in_the_ledger` 是 `P3-Y1` 原本唯一的
/// 证据，而它 `#[ignore]`（08-11 误伤用户 9 个真实会话之后的处置），且件的 `§0h` 逐字
/// 承认「**就算解开它也验不出来**」——它的客户端与 daemon 用的**不是同一个 socket**。
/// ⇒ 这条 DoD 一直挂着「没有兑现证据」。
///
/// 本条走**另一条路**：既然「帧 → 账本」是 `absorb_local_frame` 一个函数，就直接喂它
/// 一条**真实形状**的帧，再从**账本那一侧**（`snapshot_tmux_by_origin`，也就是 emitter
/// 判 idle/archived 时读的那一份）读回来。
///
/// ⚠ **射程**：本条钉的是「收到帧之后账本里有」。**daemon 真的会发**那个帧
/// 归 daemon 侧 `EMITS "tmux_sessions"`（逐字「登记 = 承诺真发」）；那一跳这里够不到，
/// 所以那条真 tmux 的实测**留着**，不是被本条替掉了。
#[test]
fn a_local_tmux_frame_lands_in_the_ledger_without_any_daemon() {
    // 用一个本条专属的 raw，避免与别的测试抢同一个 origin 的那一格。
    let raw = "p3y1-proof-cc: 1 windows (created Tue Aug 12 20:00:00 2026)";
    let line = format!(r#"{{"kind":"tmux_sessions","raw":{raw:?}}}"#);
    let frame = crate::ssh_source::parse_frame(&line).expect("这是真实帧形状，必须解析得出");
    assert!(
        matches!(frame, crate::ssh_source::InboundFrame::TmuxSessions { .. }),
        "解析出来的不是 TmuxSessions —— 后面的断言就没有意义了"
    );

    absorb_local_frame(&frame);

    let snap = crate::ssh_source::snapshot_tmux_by_origin();
    let got = snap
        .get(crate::inbound_client::LOCAL_ORIGIN)
        .map(String::as_str);
    assert_eq!(
        got,
        Some(raw),
        "帧收到了，但**账本里没有** —— DoD 自陈的失效方式逐字：\
             「『消费者收到了』不等于『账本里有』」。\
             账本这一份正是 emitter 判 idle/archived 时读的那一份。"
    );
}

/// `P3-Y1` 的**第二半**：读行循环**真的调**那个吸收点。
///
/// ★ 上面那条只证明「函数管用」。把循环里那一行删掉，它**照样绿**，
/// 而那时本机 tmux 会话又对 monitor 不可见了 —— 与 `P2` 当初「把非 hello 帧全丢了」
/// 是同一个形状的退化。⇒ 位置性质要单独钉（`find_pinned`：恰好一处、有边界）。
#[test]
fn the_read_loop_really_calls_the_absorb_point() {
    let prod = guard_core::production_code(include_str!(
        "../../../../src/bridge/src/backend/control/local_backend.rs"
    ));
    let at = guard_core::find_pinned(&prod, "absorb_local_frame(&frame);")
        .expect("读行循环里必须恰好有一处 `absorb_local_frame(&frame);`");
    let before = &prod[..at];
    assert!(
        before.contains("parse_frame(&line)"),
        "吸收点必须排在**解析出帧之后** —— 顺序反了就是拿没解析的东西去收"
    );
}

/// P3-Y1（acceptor: **实测**）：**本机 daemon 的 tmux 帧真的进了账本**。
///
/// # 为什么必须从账本那一侧读
///
/// DoD 自陈的失效方式逐字：「**「消费者收到了」不等于「账本里有」**」。
/// 消费者里加一行 `tracing::info!` 也能让人以为通了。
/// ⇒ 本条读的是 `snapshot_tmux_by_origin()`，即 emitter 判 idle/archived 时读的那一份。
///
/// # 隔离：私有 `TMUX_TMPDIR` + 跑前跑后比对（`C7e`）
///
/// daemon 跑 `tmux ls`。给它一个**私有的 `TMUX_TMPDIR`** ⇒ 它只看得见本条自己建的那台
/// tmux server，碰不到用户真实的那台。
/// ⚠ **比对本身就是判据的一部分，不是附带步骤** —— 隔离若没做对（比如忘了私有目录），
/// 测试会连进用户的 server 而**照样通过**：通过与否与隔离无关。
/// ⚠⚠ **默认不跑（`#[ignore]`）—— 08-11 事故之后的处置。**
///
/// 我在写这条时，配套的 shell 探针漏了 `unset TMUX`，`TMUX_TMPDIR` 被压过，
/// 命令打到了用户真实的 tmux server 上，**9 个真实会话没了**。
///
/// 代码这一侧的坑已经堵掉（`-S` 显式 socket · 不用 `kill-server` · `supervise` 一律清 `TMUX`），
/// 但「在一台跑着真实会话的机器上，让自动化去起 / 杀 tmux」这件事本身值得先停下来。
/// ⇒ 本条改成显式触发：`cargo test -- --ignored the_local_tmux_frames_really_land_in_the_ledger`。
///
/// **这是降级不是放弃**：P3-Y1 因此今天**没有实测证据**，如实登记在件的 §0h / 12e-1，
/// 不拿「判据绿」冒充「验过了」。
///
/// # ★ `K-R7`（08-31）：隔离原语从**自己手搓一份**换成**共享那一份**
///
/// 本条原来在测试体里现场造 shim（`exec 真tmux -S <私有 sock>`）——形态是对的，
/// 但那是 `C7i` 那条红线原语在 Rust 侧的**第二份实现**。
/// `tests/e2e/tmux-shim.sh` 的头注逐字写过为什么它要被抽成共享文件：
/// 「红线的落地**不该有三份实现**：改一处漏两处」。
/// ⇒ 改成问 `CCM_E2E_TMUX_SHIM_BIN` 要那一份（`$BIN/tmux` 强插 `-L`），
/// **同时**让本条的人群判据与那两条同族测试**变成同一条**（见
/// `every_test_that_starts_the_real_daemon_demands_a_private_tmux`）。
///
/// ⚠ 跑前/跑后那两次 `tmux ls`（`user_tmux()`）**刻意绕开 shim、按绝对路径问** ——
/// 它们要问的正是**用户那台真 server**「你变了没有」。走了 shim 就问到自己那台上去了，
/// 那条比对会变成一句恒真的空话。**而它此前正是那样**（见函数体里那段实打记录）。
#[cfg(all(embedded_daemons, target_os = "linux", target_arch = "x86_64"))]
#[test]
#[ignore = "会起真 tmux；08-11 出过误伤用户会话的事故，改成显式触发"]
fn the_local_tmux_frames_really_land_in_the_ledger() {
    use std::time::Duration;

    // ★★ **fail closed，排在一切之前**（`K-R7`）。
    let shim = crate::local_daemon::tests::demand_tmux_shim("本条起真 daemon 且起真 tmux");
    let _guard = crate::inbound_client::local_origin_test_lock();

    // ★★★ 「用户真实的 tmux」这一问**必须绕开 PATH 上的 shim**〔`K-R7` 08-31，实测逼出来的〕。
    //
    // 本条由 `tests/e2e/local-backend-supervise.sh` 驱动，而那个脚本把 shim 目录挂在
    // **本测试进程自己的 `PATH`** 最前面（`tmux-shim.sh` 里那句 `export PATH=…`）
    // ⇒ 裸 `Command::new("tmux")` 解析到的是 **shim**，问到的是 e2e 自己那台 server，
    // **根本不是用户那台**。
    //
    // 〔实打 08-31：本条原来把自己的会话建在**另一个** socket 上，于是 before/after
    //   两侧都在问 shim 那台空 server、两侧恒为空串 ⇒ **这条比对是空真**：
    //   隔离坏掉时它照样绿，而它的诊断文案写着「用户真实的 tmux 变了」。
    //   把会话搬到 shim 那台之后它**当场红** —— 而那次红正好证明了它此前问错了对象。〕
    // ⇒ 从 `PATH` 里把 shim 那一段剔掉再找 `tmux`，按**绝对路径**问。
    let real_tmux: Option<std::path::PathBuf> = {
        let shim_dir = std::path::Path::new(&shim);
        std::env::var("PATH")
            .unwrap_or_default()
            .split(':')
            .filter(|p| !p.is_empty() && std::path::Path::new(p) != shim_dir)
            .map(|p| std::path::Path::new(p).join("tmux"))
            .find(|c| c.is_file())
    };
    // 反空真：找不到 shim 之外的真 tmux ⇒ 下面那条比对又会退化成「空 == 空」。
    let real_tmux = real_tmux.expect(
        "`PATH` 上除了 shim 之外找不到第二个 `tmux` —— \
             那么下面「用户真实的 tmux 变了没有」那条比对会退化成空真（空 == 空），\
             隔离坏掉时它照样绿。",
    );
    let user_tmux = || -> String {
        std::process::Command::new(&real_tmux)
            .arg("ls")
            .env_remove("TMUX")
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
            .unwrap_or_default()
    };
    let user_tmux_before = user_tmux();

    let bin = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("embedded-daemons")
        .join("cc-monitor-remote-x86_64");
    let base = std::env::temp_dir().join(format!("p3-tmux-{}", std::process::id()));
    let home = base.join("home");
    let cfg_dir = home.join(".claude");
    std::fs::create_dir_all(cfg_dir.join("projects")).expect("建沙箱 HOME");

    let sess = format!("ccm-p3-{}", std::process::id());
    // ★★ **socket 选择器写死在 shim 里**〔事故订正 08-11 · `K-R7` 08-31 换成共享原语〕。
    //
    // 原来只给 `TMUX_TMPDIR` + `env_remove("TMUX")`。那样**只要漏掉后者**，
    // `TMUX` 就会压过 `TMUX_TMPDIR`，命令直接打到用户真实的 server 上 ——
    // 我在一次 shell 探针里正是漏了它，用户 9 个会话没了。
    // ⇒ 客户端这一侧**也按绝对路径调 shim**（不是裸 `tmux`）：
    //   shim 与 daemon 走的是**同一个** `-L`，而「同一台 server」正是本条的全部要害。
    let shim_tmux = std::path::Path::new(&shim).join("tmux");
    assert!(
        shim_tmux.exists(),
        "`CCM_E2E_TMUX_SHIM_BIN` 指的目录里没有 `tmux` —— \
             那不是 `tests/e2e/tmux-shim.sh` 造出来的那份，隔离无从谈起：{shim_tmux:?}"
    );
    let tmux = |args: &[&str]| {
        let mut c = std::process::Command::new(&shim_tmux);
        c.args(args).env_remove("TMUX").output()
    };

    let made = tmux(&["new-session", "-d", "-s", &sess]).expect("起私有 tmux 失败");
    assert!(
        made.status.success(),
        "私有 tmux 起不来：{}",
        String::from_utf8_lossy(&made.stderr)
    );

    let cleanup = |h: Option<&SuperviseHandle>| {
        if let Some(h) = h {
            h.stop();
        }
        // ⚠ **不用 `kill-server`** —— 那是个打整台 server 的大锤；
        // 一旦 socket 解析出偏差，它毁掉的是用户的全部会话（08-11 就是这么出的事）。
        // `kill-session -t <本条自己建的名字>` 最坏情况也只影响一个同名会话。
        let _ = tmux(&["kill-session", "-t", &sess]);
        let _ = std::fs::remove_dir_all(&base);
    };

    let h = supervise_with_stdio(
        bin,
        vec!["--tail-only".into()],
        vec![
            ("HOME".into(), home.display().to_string()),
            ("CLAUDE_CONFIG_DIR".into(), cfg_dir.display().to_string()),
            // ★★ 〔`P0e` 08-13〕**这条以前验不到帧，病根就写在原注释里**：
            // 「daemon 内部用默认 socket 名……上面客户端用另一个 socket。
            //  **两者不是同一个 socket**」⇒ `seen` 恒 false（`P3 §0h` 如实登记过）。
            //
            // 修法与 e2e 那边同一手：给 daemon 一条**前面挂着 shim 的 PATH**，
            // shim `exec` 真 tmux 并强插选择器 ⇒ 两边落在同一台 server 上。
            // ⚠ 〔`K-R7` 08-31〕客户端那一侧现在**按绝对路径调同一个 shim**，
            //   所以「同一台 server」不再靠两处各自写对一个 socket 名去对齐 ——
            //   它由**同一个 shim 文件**保证。选择器是 `-L` 还是 `-S` 归 `tmux-shim.sh` 管，
            //   本条一个字都不用知道（那正是把原语收成一处买到的东西）。
            (
                "PATH".into(),
                format!("{shim}:{}", std::env::var("PATH").unwrap_or_default()),
            ),
        ],
        CrashLimits::default(),
        Arc::new(|| {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0)
        }),
        Arc::new(|e| println!("[P3 实测] {e:?}")),
        Some(Arc::new(local_stdio_consumer_guarded)),
        crate::spawn_managed::local_backend_supervised(),
    );

    let mut seen = false;
    for _ in 0..150 {
        let snap = crate::ssh_source::snapshot_tmux_by_origin();
        if snap
            .get(crate::inbound_client::LOCAL_ORIGIN)
            .is_some_and(|raw| raw.contains(sess.as_str()))
        {
            seen = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }

    let user_tmux_after = user_tmux();
    cleanup(Some(&h));
    // 摘掉本条写进去的那一份，别留给同批别的用例。
    crate::ssh_source::record_tmux_raw(crate::inbound_client::LOCAL_ORIGIN, String::new());

    assert_eq!(
        user_tmux_before, user_tmux_after,
        "★ `C7e`：用户真实的 tmux 变了。\n\
             隔离没做对（shim 没挂上 / 客户端绕过了 shim）⇒ 本条刚才操作的是用户的 server。\n\
             ⚠ 这条比对**不是附带步骤**：隔离坏掉时下面那条断言**照样会过**。"
    );
    println!("E2E-OK P3 跑前跑后用户真实 tmux 一个字没变（隔离是断言，不是假设）");
    assert!(
        seen,
        "15s 内账本里没出现 `{sess}` —— 本机 tmux 帧没进 `snapshot_tmux_by_origin()`。\n\
             ★ 「消费者收到了」不算：消费者里加一行日志也能让人以为通了。\n\
             本条读的是 emitter 真正会读的那一份。"
    );
    println!("E2E-OK P3 本机 daemon 的 tmux 帧真的进了账本（`{sess}` 出现在快照里）");
}

/// ★★ **本机读帧不许被一个坏字节杀死，也不许无界**〔D 阶段补审 08-11 新增〕。
///
/// 补审在同一个读循环上逮到两条：
///
/// | # | 原版 | 后果 |
/// |---|---|---|
/// | B1 | `BufReader::lines()`（**UTF-8 严格**）+ `let Ok(line) = line else { break }` | 一个坏字节 ⇒ `InvalidData` 与 EOF 同路 ⇒ 消费者返回 = 判死 ⇒ daemon 因 EPIPE 自杀 ⇒ 记一次「崩溃」，三次后**整个进程周期不再起来**，日志写「崩了 3 次」——**一个错误的诊断** |
/// | B2 | `read_line` 语义 ⇒ **完全无界** | 远端有 `read_capped_line`（64 MiB 上限，头注记着 daemon 侧「512 MiB 无换行流 ⇒ RSS 6→518 MiB」的实测）。同一个对端、同一种失效模式，只有本机这侧没上限 |
///
/// 本条钉三件：① 不许再出现 `.lines()` 那条严格路 ② 必须走 `read_capped_line_sync`
/// ③ **上限必须取远端那个常量**（两边各写一份机制，但值不许漂）。
#[test]
fn the_local_frame_reader_is_bounded_and_lossy() {
    let src = include_str!("../../../../src/bridge/src/backend/control/local_backend.rs");
    let prod = guard_core::production_code(src);
    let at = guard_core::find_pinned(&prod, "fn local_stdio_consumer(")
        .expect("消费者不在了 —— 改了名就来改本条");
    let body: String = prod[at..]
        .lines()
        .skip(1)
        .take_while(|l| *l != "\u{7d}")
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        body.len() > 400,
        "切出来的体只有 {} 字节 —— 切错了",
        body.len()
    );

    assert!(
        !body.contains(concat!(".li", "nes()")),
        "本机读帧又用回了按行迭代器 —— 那是 **UTF-8 严格**的，\n\
             一个坏字节会被当成 EOF ⇒ 把 daemon 读死，还记成一次「崩溃」，三次后永久放弃。\n\
             远端那条路早就取了相反的取舍（`from_utf8_lossy`，注释逐字「非 UTF-8 不该让整条连接死掉」）。"
    );
    guard_core::find_pinned(&body, "read_capped_line_sync(").unwrap_or_else(|e| {
        panic!("本机读帧没走有界读行（{e}）—— 无界读遇一条永不结束的行就是无界堆分配")
    });
    guard_core::find_pinned(&body, "DAEMON_FRAME_LINE_CAP").unwrap_or_else(|e| {
        panic!(
            "本机读帧的上限不是远端那个常量（{e}）。\n\
                 两边机制各写一份（sync/async 跨不过去），但**值不许漂** —— 漂了就没人知道哪边先炸。"
        )
    });
}

/// P2-Y1b（acceptor: 机检）：**生产入口真的把消费者传下去了**。
///
/// 上面那条实测直接调 `supervise_with_stdio` 并自己传 `Some(local_stdio_consumer)`。
/// 而生产走的是 `start_or_extract`。两者之间那根线**没有任何东西守着** ——
/// 有人把它改回 `supervise(…)`（少一个参数、默认 `None`），实测照样全绿，
/// 线上却一条入方向通道都不会登记。本条钉的就是那根线。
///
/// ⚠ 它是**文本判据**，只证明「那行代码长这样」，不证明运行时真跑到。
/// 运行时那半由上面的实测证；两条合起来才闭合，单独任何一条都不够。
#[test]
fn the_production_entry_hands_the_stdio_consumer_down() {
    let src = include_str!("../../../../src/bridge/src/backend/control/local_backend.rs");
    // 人群 = 本模块的**全部启动入口**，与 `the_startup_path_really_calls_this_module`
    // 那条用的是同一个清单。只钉「今天 lib.rs 在调的那一个」= 给下一个用另一个入口的人留坑。
    for name in ENTRIES {
        // 按**行**取函数体：从 `pub fn <name>(` 那行起，到第一行**恰好是 `}`** 为止。
        // 不用 `src.find("\n}\n")` —— 那是语料上的裸 `.find`，`needle_anchor_registry`
        // 的递减棘轮不许再长；而且逐行判「整行等于 `}`」本来就比子串匹配更贴事实。
        let head = format!("pub fn {name}(");
        let mut lines = src.lines().skip_while(|l| !l.starts_with(&head)).peekable();
        assert!(lines.peek().is_some(), "人群塌了：入口 `{name}` 不在了");
        // ⚠ 变量名**刻意不叫 `body`**：`needle_anchor_registry` 按**名字**认语料变量，
        // 而本文件另一条判据早就有 `body.contains(".stop()")`。叫 `body` 会让那处
        // 被追认成「语料上的裸 contains」，把递减棘轮顶红 —— 明明我一行匹配都没加。
        let entry_src: String = lines
            // ⚠ 收尾行**不写字面量右花括号** —— 本仓有判据用「花括号配平」剥测试段
            // （剥法的唯一住址是 `guard_core::production_source`），源码里多一个孤立的右花括号会让它**提前闭合**（`b'…'` 的字符字面量也算，我第一次「修」时就还带着一个），
            // 测试段整段泄漏进「生产段」⇒ 别的判据当场误报（08-11 实测：单写者守卫红了）。
            .take_while(|l| *l != "\u{7d}")
            .collect::<Vec<_>>()
            .join("\n");
        let entry_src = entry_src.as_str();
        // 用 `find_pinned`（恰好一处 + 两侧有边界）而不是裸 `contains`：
        // 后者在 `needle_anchor_registry` 的递减棘轮里，而且它的病正是「needle 被撑大时照样绿」。
        guard_core::find_pinned(entry_src, "supervise_with_stdio(").unwrap_or_else(|e| {
            panic!(
                "`{name}` 不再恰好调一次 `supervise_with_stdio`（{e}）——\n\
                     十有八九被改回了 `supervise(`，那一支的 `stdio` 恒为 `None`\n\
                     ⇒ 本机 stdin 又变回 `Stdio::null()`（正是 C4 量出的那个缺口）。"
            )
        });
        guard_core::find_pinned(entry_src, "Some(Arc::new(local_stdio_consumer_guarded))").unwrap_or_else(
            |e| {
                panic!(
                    "`{name}` 没把**兜底版**消费者 `local_stdio_consumer_guarded` 传下去（{e}）。\n\
                         ⚠ 传裸的 `local_stdio_consumer` 也不行：它体内一次 panic 会 unwind 出 supervise 线程，\n\
                         留下「daemon 活着但没人读它 stdout」的**全绿死锁态**（补审 B3）。\n\
                         管子接出来了却没人读 ⇒ hello 帧没人解 ⇒ `client_for(<local>)` 恒 None，\n\
                         且不会报任何错。"
                )
            },
        );
    }
}

/// P2-Y1 + P2-Y3（acceptor: **实测**）：起**真的** daemon 二进制，
/// 看入方向通道是不是真的登记上了；再关写端，看 daemon 是不是**还活着**。
///
/// # 为什么不起 GUI
///
/// 要验的性质是「本机后端起来 ⇒ `client_for(LOCAL_ORIGIN)` 拿得到通道」。
/// 这条链的全部零件都在本模块 + `inbound_client` 里，GUI 一个字都不参与。
/// 起整个 app 只会把「哪一步坏了」这个信息埋掉。
///
/// # 沙箱 HOME 只给子进程
///
/// daemon 会 tail `$HOME/.claude/projects`。用 `envs` 参数给**子进程**单独设 `HOME`
/// ⇒ 测试进程自己的 env 一个字不动（`std::env::set_var` 是进程全局的，
/// cargo 又是多线程跑测试 ⇒ 那样会污染同批别的用例）。
///
/// # ⚠ 这条测试在缺内嵌二进制时**不存在**（诚实边界 10c）
///
/// `cfg(embedded_daemons)` 由 `build.rs` 在 `embedded-daemons/` 齐全时才置，而那个目录是
/// gitignore 的 ⇒ 干净 clone 上本条**不编译进去**，`cargo test` 照样全绿。
/// 不做成「缺了就 red」是因为那会让干净 clone 无法跑测试；缺失不是静默的 ——
/// `build.rs` 那处 `cargo:warning=缺少内嵌 daemon` 会喊（U-1 那次事故之后加的）。
///
/// # ★★ `K-R7`（08-31）：**这一条此前没有人点过名，而它与那条正题完全同形**
///
/// `K-R7` 的件文件 `§0` / `§2` 与风险 `6p` 讲的都只是
/// `local_daemon::tests::the_local_daemon_can_be_stopped_and_started_again`。
/// 而 `D3` 的全表现打之后，**本条是同一族的第二条**：普通 `#[test]`、同一个 `cfg`、
/// 起同一个真 daemon 二进制（还起了两次：探针一次 + `supervise_with_stdio` 一次），
/// 而 daemon 一上来就**无条件**往它连得到的 tmux server 装三条**全局** hook（槽位 `[50]`）。
///
/// ⚠⚠ **本条原来那句 `.env_remove("TMUX")` 读起来像隔离，其实不是**：
/// `TMUX` 一空，tmux 客户端就**回落到默认 socket** `/tmp/tmux-$UID/default` ——
/// 那正是用户那台 server。清一个变量买不到隔离，**只有显式选择器**（shim 强插 `-L`/`-S`）能。
/// 那句注释说的是另一件对的事（不继承「测试进程恰好在哪个 tmux 里」），别把它读成隔离。
///
/// ⇒ 与那条正题同样三道锁：`#[ignore]` + `CCM_E2E_TMUX_SHIM_BIN` fail-closed（排在
/// **任何 spawn 之前**）+ 那个 shim **真的挂进 daemon 的 `PATH` 最前面**（探针与被监护进程都要）。
///
/// ⚠⚠ **第三道锁的射程要分两格看**〔`D1` 审计 08-31 查实，阻塞 4 的另一半〕——
/// 与 `local_daemon_tests.rs::the_local_daemon_can_be_stopped_and_started_again` 头注里那张表**同一份**：
/// 走 `bash tests/e2e/local-backend-supervise.sh` 时 `tmux-shim.sh` 已经把 shim 挂进
/// **测试进程自己的 `PATH`**，而 `supervise_with_stdio` **从不 `env_clear()`**
/// ⇒ 第三道锁在**那条跑法上是冗余的**；它真正买的是「**手工 `cargo test -- --ignored`、
/// 变量设上但 shim 不在自己 `PATH` 上**」那一格。别把两格混着读。
/// 看着它的判据是 `local_daemon.rs` 那条
/// `every_test_that_starts_the_real_daemon_demands_a_private_tmux` 的 ㈡ 与 ㈢ **两格**
/// ——㈡ 判**写法**（`"PATH"` 与 `"{<绑定名>}:` 同行，插值紧跟开引号），
/// ㈢ 判**处数**（`PATH` 这个 env 键在本体里恰好写一次）。
/// ⚠ ㈢ 是 `D2` 复审 09-01 逼出来的〔阻塞 1 形 ②〕：本条的 `envs` 里**再追加一条**
/// `("PATH", …)` 就能把上面那条带 shim 的整个盖掉（`supervise_with_stdio` 是
/// `for (k, v) in &envs { cmd.env(k, v); }`，**后写的赢**，见 `:336`-`:337`），
/// 而在 ㈢ 落地之前那一刀**全量门禁新红 0**。两格的代价不是一种，判据也刻意分开（`K13`）。
///
/// # 🔴 复跑纪律：**换了 `embedded-daemons/` 的有无之后，必须 `touch src/bridge/build.rs`**
///
/// 〔`D2 §G-1` 的陈账，09-01 收 —— 此前只写在件文件里，**被守对象这一侧一个字都没有**。〕
/// 本条由 `#[cfg(embedded_daemons)]` 门着。**实测的现象**（`D2` 复审 09-01，我没重打，
/// 住址 `audits/K-R7-D2.md#§G-1`）：同一个 `CARGO_TARGET_DIR` 里把
/// `src/bridge/embedded-daemons/` 从「无」加成「有」，**`build.rs` 不重跑** ⇒
/// 读出的是「无 emb」那一档的数（`1208/0/10`），`touch build.rs` 之后才是 `1211/0/11`。
/// ⚠⚠ **两档的输出面长得一模一样，这个坑看不出来。**
/// ⇒ 换 emb 状态之后 `touch src/bridge/build.rs`，或**直接换一个全新的 target 目录名**。
/// ⚠ 机制**我没有实验证明**；`local_daemon.rs` 那条姊妹测试的同名小节里记着两条候选，
/// 其中「`rerun-if-changed` 只在文件存在时登记」那条**现打对不上源码**（那一行是无条件的）。
#[cfg(all(embedded_daemons, target_os = "linux", target_arch = "x86_64"))]
#[test]
#[ignore = "K-R7：起真 daemon ⇒ 会装全局 tmux hook。走 tests/e2e/local-backend-supervise.sh 那条带 shim 的路"]
fn the_local_daemon_really_registers_an_inbound_client() {
    // ★★ **fail closed，而且排在一切之前** —— 见上面头注第三段。
    let shim = crate::local_daemon::tests::demand_tmux_shim(
        "本条起真 daemon，而 daemon 一上来就往它连得到的 tmux server 装全局 hook",
    );
    let _guard = crate::inbound_client::local_origin_test_lock();
    let bin = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("embedded-daemons")
        .join("cc-monitor-remote-x86_64");
    assert!(
        bin.exists(),
        "`cfg(embedded_daemons)` 置了但 {bin:?} 不在 —— build.rs 与磁盘不一致"
    );

    let home = std::env::temp_dir().join(format!("p2-local-inbound-{}", std::process::id()));
    let cfg_dir = home.join(".claude");
    std::fs::create_dir_all(cfg_dir.join("projects")).expect("建沙箱 HOME");
    // ⚠⚠ **只设 `HOME` 不够，而且这条是实测逼出来的**（P2s 摸底 08-11）：
    // daemon 的 `resolve_claude_dir()` 逐字「`$CLAUDE_CONFIG_DIR` if set, else `$HOME/.claude`」
    // ⇒ 继承来的 `CLAUDE_CONFIG_DIR` **压过** `HOME`。本条第一版只设 HOME，
    // 那一跑 daemon 读的其实是**真实**的配置目录（只读 tail，没有写，但隔离是假的）。
    let envs = vec![
        ("HOME".to_string(), home.display().to_string()),
        (
            "CLAUDE_CONFIG_DIR".to_string(),
            cfg_dir.display().to_string(),
        ),
        // ★ `K-R7`：隔离**真的用上**。探针那一跳走 `.envs(envs…)`，被监护那一跳走
        //   `supervise_with_stdio(.., envs, ..)` ⇒ 写在这里两跳都盖得到。
        (
            "PATH".to_string(),
            format!("{shim}:{}", std::env::var("PATH").unwrap_or_default()),
        ),
    ];

    // 隔离**必须是断言，不能是假设**：起一趟一次性的，从 hello 帧里把 daemon 自陈的
    // `claude_dir` 读回来对一遍。上面那次教训就是「我以为设了 HOME 就隔离了」。
    {
        use std::io::BufRead;
        let mut probe = std::process::Command::new(&bin)
            .arg("--tail-only")
            .envs(envs.iter().map(|(k, v)| (k.clone(), v.clone())))
            .env_remove("TMUX") // 同上：不许继承「测试进程恰好在哪个 tmux 里」
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .spawn()
            .expect("起探针失败");
        let mut line = String::new();
        std::io::BufReader::new(probe.stdout.take().expect("有 stdout"))
            .read_line(&mut line)
            .expect("读 hello 失败");
        let _ = probe.kill();
        let _ = probe.wait();
        let frame = crate::ssh_source::parse_frame(&line).expect("首帧该是 hello");
        let crate::ssh_source::InboundFrame::Hello { claude_dir, .. } = &frame else {
            panic!("首帧不是 hello：{line}");
        };
        assert_eq!(
            Path::new(claude_dir),
            cfg_dir,
            "daemon 自陈的 claude_dir 不在沙箱里 —— 这一跑读的是**真实**配置目录。\n\
                 `resolve_claude_dir()` 是 `$CLAUDE_CONFIG_DIR` 优先、`$HOME/.claude` 兜底，\n\
                 两个都要设。（只设 HOME 那版跑起来一切正常，隔离却是假的。）"
        );
        // ★ `K-R7`：本条改成 `#[ignore]` 之后由 `tests/e2e/local-backend-supervise.sh` 驱动，
        //   而那个脚本的收尾自检是「标记数 < 跑成的测试数 ⇒ 有测试提前退出」。
        println!("E2E-OK P2 daemon 自陈的 claude_dir 就在沙箱里（隔离是断言，不是假设）");
    }

    let h = supervise_with_stdio(
        bin,
        vec!["--tail-only".into()],
        envs,
        CrashLimits::default(),
        Arc::new(|| {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0)
        }),
        Arc::new(|e| println!("[P2 实测] {e:?}")),
        Some(Arc::new(local_stdio_consumer_guarded)),
        crate::spawn_managed::local_backend_supervised(),
    );

    // 轮询而不是睡死：进程起来 + 发 hello 的耗时不确定，睡固定值要么慢要么飘。
    let mut client = None;
    for _ in 0..100 {
        if let Some(c) = crate::inbound_client::client_for(crate::inbound_client::LOCAL_ORIGIN) {
            client = Some(c);
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    let cleanup = |h: &SuperviseHandle| {
        h.stop();
        let _ = std::fs::remove_dir_all(&home);
    };
    let Some(client) = client else {
        cleanup(&h);
        panic!(
            "5s 内 `client_for(<local>)` 仍是 None —— 本机入方向通道没登记上。\n\
                 ⚠ 「没报错」不算数：远端那条路的头注记着一次审计变异 ——\n\
                 把注册整段删掉（写半边永不解冻）`cargo test` 照样全绿。所以这里必须**看到通道**。"
        );
    };

    // P2-Y1：登记了还得是**能用的** —— daemon 声明的入方向命令要在。
    assert!(
        client.accepts("launch") && client.accepts("kill"),
        "通道登记了，但 daemon 没声明接受 launch/kill ⇒ P3 接过来也发不出去"
    );
    println!("E2E-OK P2 本机入方向通道登记上了，而且声明接受 launch/kill");

    // P2-Y3：关写端**不许**把 daemon 带走（C8 裁定「默认不 kill、daemon 继续跑」）。
    // 单独观测，不顺带看一眼 —— 件里 §0d 把这条从「推断」改成了实测，就是这个意思。
    let pid = h.current_pid().expect("已经收到 hello 了，进程必然在");
    client.close_write();
    std::thread::sleep(std::time::Duration::from_millis(500));
    let alive = Path::new(&format!("/proc/{pid}")).exists();
    crate::inbound_client::unregister(crate::inbound_client::LOCAL_ORIGIN, &client);
    cleanup(&h);
    assert!(
        alive,
        "关掉 stdin 写端之后 daemon（pid={pid}）没了。\n\
             这与 C8「默认不 kill」直接冲突，也推翻了 `local_backend.rs::stop` 头注那句\n\
             「它的入方向对『写端关闭』是刻意不敏感的」——那句注释得改，不是这条测试得改。"
    );
    println!("E2E-OK P2 关掉 stdin 写端之后 daemon（pid={pid}）还活着（C8「默认不 kill」）");
}

use super::*;

fn never(_: &Path) -> bool {
    false
}

/// P2z-Y2（自批 D1）：**本机释放点与远端部署点结构上不许撞**。
///
/// 钉的是**构造方式**不是两个字面量不相等 —— 后者一改配置就绕过去了
/// （远端落点由 `cfg.daemon_path` 给，是**运行期**的值，编译期比不了）。
/// 所以断言：那条路径必须由 `build_id` 拼出来。
///
/// 病史：实测 08-11 本机 `~/.cc-monitor/bin/.build_id` = `p1r-event-liveness`
/// （别的 monitor 把这台当远端连时装的），而本机源码是 `p1x-overflow-identity`
/// ⇒ 同名会让两个 monitor 互判 stale、互相覆盖 ⇒ **无限重装循环**。
#[test]
fn the_local_extract_path_is_build_id_scoped() {
    // ★ 用**生产函数**，不是判据自己抄一份 `format!`（那就成了「测自己的副本」）。
    let a = local_extract_name("p1x-overflow-identity");
    let b = local_extract_name("p1r-event-liveness");
    assert_ne!(
        a, b,
        "两个不同 build_id 竟然产出同一个文件名 —— 那就等于回到「同路径互相覆盖」"
    );
    for (id, name) in [("p1x-overflow-identity", &a), ("p1r-event-liveness", &b)] {
        assert!(
            name.contains(id),
            "本机释放文件名 `{name}` 里没有 build_id `{id}`。\n\
                 ⚠ 这条钉的是**构造方式**：远端自部署落点同为 `~/.cc-monitor/bin/`，\n\
                 只有把 build_id 拼进文件名才能让两条路**结构上**撞不上。\n\
                 改成固定名 = 把「无限重装循环」装回来（见 `extract_embedded_to` 头注 D1 段）。"
        );
    }
    assert!(
        !a.contains("cc-monitor-remote"),
        "本机释放名不许长成远端那个名字（`cc-monitor-remote`）—— 那正是要避开的那个文件"
    );
}

// ═══════════════════════════════════════════════════════════════════════
// 🔴 `K-R69` / `KR69D1`：**本机那条 `ccm` 入口，与远端那条同源**
// ═══════════════════════════════════════════════════════════════════════

/// `KR69D1` 的**同源那一半（内容侧）**：本机那条落点**就是后端二进制本身**。
///
/// # 它是这一格的死值验第二向
///
/// `KR69D1` 逐字：「让本机那条改用**另一份**二进制 / 另一套 argv 解析 ⇒ **必须红**」。
/// 本条断的正是这件事，而且断得比「路径对不对」硬：**逐字节相同**。
/// 谁把这里改成写一段 shim 文本、或改成从别处取字节，当场红。
///
/// # 为什么不能拿「文件在不在」当判据
///
/// 「在」只说明有个叫 `ccm` 的文件，说不出它是谁 —— 而**本机上恰好有另一个叫 `ccm`
/// 的东西**正是这一整件的题面（用户 `~/.local/bin/ccm` 那份旧 bash）。
/// 「同名不同物」认不出来的判据，在这一件上等于没有。
#[test]
fn the_local_ccm_entry_is_a_copy_of_the_backend_itself() {
    let base = std::env::temp_dir().join(format!("ccm-kr69-copy-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(&base).expect("建夹具目录");
    // 夹具名**中性**、内容**唯一**：断言比的是这串字节，不是任何路径 / 目录名
    //〔固定项 12 那条 `6g`：断言用的子串不许取自夹具的名字〕。
    let bytes: Vec<u8> = (0u8..=255).cycle().take(4096).collect();
    let backend = base.join("some-backend-binary");
    std::fs::write(&backend, &bytes).expect("写夹具后端");

    let marked = std::sync::Mutex::new(Vec::<PathBuf>::new());
    let mark = |p: &Path| {
        marked.lock().unwrap().push(p.to_path_buf());
        Ok(())
    };
    let dest = install_local_ccm_entry(&base, &backend, &mark).expect("放本机 ccm 入口");

    assert_eq!(
        dest.file_name().and_then(|s| s.to_str()),
        Some(local_ccm_entry_name().as_str()),
        "放下去的名字不是 `local_ccm_entry_name()` 说的那个 —— 名字有了第二个来源"
    );
    assert_eq!(
        std::fs::read(&dest).expect("读回本机 ccm 入口"),
        bytes,
        "本机那条 `ccm` 落点的内容**不是后端那份字节** —— \n\
             那它就是第二份东西了，而 `K33` 逐字「所有命令只许有一处」。\n\
             `KR69D1` 的失效方向逐字：「在本机再写一个 `ccm` 壳 ⇒ 不算兑现」。"
    );
    assert_eq!(
        marked.lock().unwrap().len(),
        1,
        "置可执行位那一步没走（或走了不止一次）—— 放下去一个起不来的文件\n\
             比不放更坏：`--ccm-probe` 探它会失败，而失败长得像「没装」"
    );
    // 幂等：再放一次不重写（长度相同、且不比后端旧）。
    let before = std::fs::metadata(&dest).and_then(|m| m.modified()).ok();
    let again = install_local_ccm_entry(&base, &backend, &mark).expect("第二趟");
    assert_eq!(again, dest);
    assert_eq!(
        std::fs::metadata(&dest).and_then(|m| m.modified()).ok(),
        before,
        "第二趟重写了 —— 每次起 app 都白付一次几 MB 的顺序写"
    );
    // 反向：后端换了一版（字节变了、时间更新）⇒ 这一份必须跟着换。
    let newer: Vec<u8> = (0u8..=255).cycle().skip(7).take(4096).collect();
    assert_ne!(newer, bytes, "夹具自己塌了：两版字节竟然相同");
    std::fs::write(&backend, &newer).expect("换一版后端");
    filetime_bump(&backend);
    install_local_ccm_entry(&base, &backend, &mark).expect("第三趟");
    assert_eq!(
        std::fs::read(&dest).expect("读回"),
        newer,
        "后端换了一版，本机那条 `ccm` 入口还停在上一版 —— \n\
             用户终端里那条命令与 app 里跑的后端**不是同一份**，而两边都不会出声"
    );
    let _ = std::fs::remove_dir_all(&base);
}

/// 把一个文件的 mtime 往后推一点。**不引新依赖**：重写一次内容之后
/// 有些文件系统的时间戳粒度是秒级 ⇒ 直接改内容不一定改得动 `modified()`。
/// 这里显式把**目标**那份的时间戳往回拨（比推源那份便宜，也不碰时钟）。
fn filetime_bump(newer_than_this: &Path) {
    let dir = newer_than_this.parent().expect("有父目录");
    let stale = dir.join(local_ccm_entry_name());
    // 把目标那份的 mtime 设成 1970 —— 于是它一定「比后端旧」。
    // ⚠ **必须以可写方式打开**：只读句柄在有些平台上 `set_modified` 直接 `EBADF`，
    //   而那时上面第三条断言会**因为夹具没生效**而红 —— 那是假红，比不测还坏。
    //   ⇒ 这里不吞错：夹具塌了就当场说出来。
    std::fs::OpenOptions::new()
        .write(true)
        .open(&stale)
        .expect("夹具：打不开本机 ccm 入口")
        .set_modified(std::time::UNIX_EPOCH)
        .expect("夹具：改不动 mtime —— 下面那条「换版必刷新」判的就不是它了");
}

/// `KR69D1` 的**防空转**：那条入口真的有生产调用点，而且就在解析出后端之后。
///
/// # 没有这一条会怎样
///
/// [`install_local_ccm_entry`] 是个自足的函数，上面那条判据喂它一份夹具就能全绿 ——
/// 而**只要没人在生产段里调它，本机就还是一条 `ccm` 都没有**，
/// 也就是 `K-R69` 立件时那个读数原封不动。本仓这一形有名字（`K-R28` 那条「防空转」）。
///
/// # ⚠ 它守什么、**不守什么**〔`K-R69` 收窗口前订正 —— 原话不完整，PM 一刀切中〕
///
/// **约定型守卫**（查源码形态）。原话只写「挡得住『接线被删掉』，挡不住『换个名字继续错』」，
/// 而 PM 09-12 的**刀 T** 实测出第三样它当时也挡不住：**接线还在、名字没改、
/// 而调用点喂进去的可以不是后端本体** —— 把第二个实参从 `bin` 换成
/// `&extract_dir.join("<随便一个不存在的名字>")`，别处一字不动，
/// 判定行逐字 `test result: ok. 1393 passed; 0 failed` —— **一条都没红**。
/// 后果不是编译错，是**静默放下一份不是后端的文件**，而 `--ccm-probe` 探它会失败，
/// **失败长得像「没装」**（那句话正是本模块另一条判据自己写的）。
///
/// ⇒ 本条今天多钉一格：**那唯一一处调用喂进去的，就是 `Resolved::Found` 解开的那个绑定**。
/// ⚠ 这一格仍是**形态**（换个变量名照样能骗过它）—— 真正不看拼法的那一半在
/// [`tests::the_resolution_path_hands_the_ccm_entry_the_backend_it_just_resolved`]：
/// 它真跑一趟 `resolve_or_extract`，比的是**落下来那份的字节**。
/// **两条合起来才是那道缝，单独任何一条都不够。**
#[test]
fn the_resolution_path_really_puts_the_local_ccm_entry_down() {
    let sect = shared_resolution_body();
    let put = guard_core::find_pinned(&sect, "install_local_ccm_entry(").unwrap_or_else(|e| {
        panic!(
            "那份共用的解析体内 `install_local_ccm_entry(` 不是恰好一处（{e}）——\n\
                 一处都没有 ⇒ 本机那条 `ccm` 入口**没有任何生产调用点**，\n\
                 盘上回到 `K-R69` 立件时那个读数（本机 0 条），而上面那条判据照样绿。\n\
                 多于一处 ⇒ 有第二条放法，下面那条顺序断言说不清它断的是哪一处。\n逐字：{sect}"
        )
    });
    let found = guard_core::find_pinned(&sect, "extract_embedded_to(")
        .expect("那份共用的解析体内 `extract_embedded_to(` 不是恰好一处 —— 切歪了");
    assert!(
        found < put,
        "「放本机 ccm 入口」排到了「把后端释放出来」前面 —— 顺序反了：\n\
             那时它复制的是一份还不存在（或还是上一版）的二进制。"
    );
    // 🔴 **PM 刀 T 逼出来的那一格**：喂进去的**是解析出来的那份后端**，不是别的什么路径。
    // ⚠ 钉的是**这一对**，不是两个孤立的串：一头是 `Resolved::Found` 解开的那个绑定，
    //   另一头是那唯一一处调用的实参表。分开钉的话，把绑定留着、实参换掉照样过。
    guard_core::find_pinned(&sect, "if let Resolved::Found(bin) = &resolved {").unwrap_or_else(
        |e| {
            panic!(
                "那份共用的解析体内「解开 `Resolved::Found`」不是恰好一处（{e}）——\n\
                     下面那条断言指不明它说的是哪一个 `bin`，本条此刻无效。\n逐字：{sect}"
            )
        },
    );
    guard_core::find_pinned(&sect, "install_local_ccm_entry(extract_dir, bin, make_executable)")
        .unwrap_or_else(|e| {
            panic!(
                "那一处调用喂进去的不是解析出来的那份后端（{e}）。\n\
                     🔴 这正是 PM 09-12 刀 T 切中的那道缝：接线还在、函数没改，\n\
                     而实参换成别的路径 ⇒ **静默放下一份不是后端的文件**，\n\
                     `--ccm-probe` 探它会失败，而**失败长得像「没装」**。\n\
                     期望逐字：`install_local_ccm_entry(extract_dir, bin, make_executable)`\n逐字：{sect}"
            )
        });
}

/// 🔴 **刀 T 那道缝的另一半，而且这一半不看拼法**：真跑一趟 `resolve_or_extract`，
/// 断言落下来那条 `ccm` 入口**与它刚解析出来的那份后端逐字节相同**。
///
/// # 为什么非要行为判据不可
///
/// 上面那条是**形态**判据：换个变量名、换个等价写法都能绕过去。
/// 本条不读一个字源码 —— 它只问「盘上那两份字节一样吗」，
/// 于是「喂进去的不是后端本体」这一形**不论怎么拼**都会在这里现原形。
///
/// # 夹具怎么保证走的是我要盯的那一支
///
/// `target_triple` 取一个**盘上必不存在**的中性串 ⇒ `resolve_beside_this_exe` 必回
/// `Missing` ⇒ 走「释放内嵌那份」那一支，而本条盯的正是那一支之后那一跳。
/// 三条反向自检（缺一条这里就会零命中地绿）：① 真的 `Found` 了；
/// ② 释放出来那份就是我喂进去的字节；③ 那两个文件**不是同一个**
/// （否则「两份相同」靠自反恒真）。
#[test]
fn the_resolution_path_hands_the_ccm_entry_the_backend_it_just_resolved() {
    let base = std::env::temp_dir().join(format!("ccm-kr69-wire-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(&base).expect("建夹具目录");
    // 内容唯一、夹具名中性：断言比的是这串字节，不是任何路径 / 目录名〔固定项 12 的 `6g`〕。
    let bytes: Vec<u8> = (0u8..=255).cycle().skip(3).take(3072).collect();
    let mark = |_: &Path| Ok(());

    let r = resolve_or_extract(
        "no-such-target-triple",
        &base,
        Some(("kr69wire", &bytes)),
        &mark,
    );
    // ① 反向自检：真的走到了「释放出来并 Found」那一支。
    let Resolved::Found(bin) = r.clone() else {
        panic!("夹具塌了：内嵌那份没释放出来 ⇒ 本条此刻无效。实得 {r:?}");
    };
    // ② 反向自检：释放出来那份就是我喂进去的字节。
    assert_eq!(
        std::fs::read(&bin).expect("读回释放出来那份"),
        bytes,
        "夹具塌了：释放出来那份不是我喂进去的字节"
    );
    let entry = base.join(local_ccm_entry_name());
    // ③ 反向自检：两个文件不是同一个（否则下面那条比较靠自反恒真）。
    assert_ne!(
        entry, bin,
        "夹具塌了：本机 `ccm` 入口与释放出来那份是同一个文件 ⇒ 下面那条比较恒真"
    );
    assert!(
        entry.is_file(),
        "解析出后端之后，本机那条 `ccm` 入口**没被放下来**：{}\n\
             要么那一跳没接上，要么它拿到的路径根本不存在（PM 刀 T 那一形）。",
        entry.display()
    );
    assert_eq!(
        std::fs::read(&entry).expect("读回本机 ccm 入口"),
        bytes,
        "本机那条 `ccm` 入口的字节**不是刚解析出来的那份后端** ——\n\
             🔴 静默放下一份不是后端的文件，而 `--ccm-probe` 探它会失败，\n\
             **失败长得像「没装」**：用户看到的是「你没装」，而真相是「我们放错了东西」。"
    );
    let _ = std::fs::remove_dir_all(&base);
}

/// `KR69D1` 的**同源那一半（拼写侧）**：两条落点里那个 `ccm`，取自**同一处**。
///
/// # 它买的是什么
///
/// 本机那条靠**文件名**进 `intercept` 的入口①（`argv[0]` 的 basename），
/// 远端那条靠 shim 里的**子命令词**进入口②（`<bin> ccm …`）。
/// 两处要是各写一个字面量，改一个漏一个的后果是**静默的**：
/// 本机那份改了名字之后，它就不再被后端认成 `ccm`，而是当普通流模式起来 ——
/// 用户敲下去看到的是一个不动的进程，没有任何一条判据会红。
/// ⇒ 闭集只许有一个住址（`13b`），那个住址是 [`CCM_ENTRY_WORD`]。
#[test]
fn both_ccm_entries_spell_the_word_from_the_same_place() {
    // ① 本机：文件名以那个词打头（后面只许跟目标平台的可执行后缀）。
    let name = local_ccm_entry_name();
    assert_eq!(
        name.strip_prefix(CCM_ENTRY_WORD).map(str::to_string),
        Some(env!("CCM_TARGET_EXE_SUFFIX").to_string()),
        "本机那条入口的文件名不是「那个词 + 目标平台后缀」：{name:?}"
    );
    // ② 远端：shim 把 argv 交给的就是那个子命令词。
    let shim = ccm_entry_shim("/x/cc-monitor-remote");
    let handoff = format!(" {CCM_ENTRY_WORD} \"$@\"");
    assert!(
        shim.contains(&handoff),
        "远端那条 shim 交给后端的不是 `{CCM_ENTRY_WORD}` 子命令：\n{shim}"
    );
    // ③ 反向自检：那个词不许是空串 / 空白 —— 否则上面两条都会**空真**。
    assert!(
        !CCM_ENTRY_WORD.trim().is_empty()
            && CCM_ENTRY_WORD.chars().all(|c| c.is_ascii_alphanumeric()),
        "`CCM_ENTRY_WORD` 变成了 {CCM_ENTRY_WORD:?} —— 上面两条会零命中地绿"
    );
}

/// 远端 shim 必须把**入口名**传下去（`CCM_SELF`）——否则容器路的内层命令缺子命令词。
///
/// # 这条钉的是 09-15 真机逮到的那一格
///
/// `ccm::plan` 的内层载荷以 `self_path` 开头，取自 `CCM_SELF` → 兜底 `argv[0]`。
/// 入口①（改名副本）的 `argv[0]` basename 本来就是 `ccm`；入口②（本 shim）`exec` 的是
/// 二进制真身 ⇒ 不传 `CCM_SELF` 的话内层命令是 `<bin> --cwd …`，**没有 `ccm`**，
/// 被当 daemon 直连口解析 ⇒ `unknown argument: --cwd`。
///
/// ⚠ **为什么这条判据必须存在**：那个失败**不红在任何现有判据上** ——
/// tmux 会话建得出来、`@ccm_agent` 打得上、`--print` 吐的是同一条坏命令，
/// 只有真去读窗格才看得见。上面那条 `both_ccm_entries_…` 只看 ` ccm "$@"` 这个子串，
/// 它在坏版本里**照样命中**（坏的恰恰是 `exec` 之前少了一段）。
#[test]
fn remote_shim_carries_the_entry_name_for_the_container_path() {
    let shim = ccm_entry_shim("/x/cc-monitor-remote");
    assert!(
        shim.contains("CCM_SELF="),
        "远端 shim 没有把入口名传下去 ⇒ 容器路（--tmux）的内层命令会缺 `{CCM_ENTRY_WORD}` 子命令：\n{shim}"
    );
    // 必须取自 `$0`（「我是被当作什么叫的」），而不是硬编码某个路径。
    assert!(
        shim.contains("$0"),
        "`CCM_SELF` 不是取自 `$0` —— 换个落点就指错入口：\n{shim}"
    );
    // 外部已设时不许覆盖（`${CCM_SELF:-$0}` 这一形）。
    assert!(
        shim.contains("${CCM_SELF:-$0}"),
        "`CCM_SELF` 覆盖了调用方已经设好的值：\n{shim}"
    );
    // 反向自检：赋值必须在 `exec` **之前**，否则它进不了被 exec 的那个进程的环境。
    let (assign, exec_part) = shim
        .split_once("exec ")
        .expect("shim 里没有 `exec ` —— 这条判据的前提没了");
    assert!(
        assign.contains("CCM_SELF=") && !exec_part.contains("CCM_SELF="),
        "`CCM_SELF` 没有落在 `exec` 之前 ⇒ 传不进后端进程：\n{shim}"
    );
}

/// P2z-Y3：**本机那条路不许自己写版本比较** —— 复用 `sftp::deploy_decision`（纯函数）。
///
/// 它会失效的地方（如实写）：`deploy_decision` 只回答「要不要装」，
/// **不回答「装完对不对」**。本条只挡「另写一套比较逻辑」，不是完整校验。
#[test]
fn the_local_path_does_not_hand_roll_version_comparison() {
    let src = guard_core::production_code(include_str!(
        "../../../../src/bridge/src/backend/control/local_backend.rs"
    ));
    // 判据串运行时拼，免得命中本文件自己的头注。
    let bad = format!("{}_id !=", "build");
    assert!(
        !src.contains(&bad),
        "生产段出现了手写的 build_id 比较（`{bad}`）。\n\
             版本比对只有一个真相源：`sftp::deploy_decision`（纯函数，可单测）。\n\
             另写一套 ⇒ 两处判「要不要装」的逻辑迟早分叉，而分叉的后果是无限重装。"
    );
}

#[test]
fn missing_sidecar_is_an_honest_degrade_that_lists_every_path_it_tried() {
    let r = resolve_with(
        Path::new("/opt/app"),
        "x86_64-unknown-linux-gnu",
        "",
        &never,
    );
    let Resolved::Missing { looked_at, .. } = r else {
        panic!("应当是 Missing");
    };
    assert_eq!(looked_at.len(), 2, "两个候选都要列出来：{looked_at:?}");
    assert!(
        looked_at
            .iter()
            .any(|p| p.to_string_lossy().contains("x86_64-unknown-linux-gnu")),
        "triple 形态那个候选丢了：{looked_at:?}"
    );
}

/// 「像内部件号」的形状：一个 ASCII 大写字母紧跟两位数字（`F05b` · `K-R30` 的 `R30` 都算）。
/// 三个字节**全是 ASCII** 才算命中 ⇒ 切片必落在字符边界上，中文正文不会误伤。
fn internal_item_id_in(s: &str) -> Option<String> {
    let b = s.as_bytes();
    (0..b.len().saturating_sub(2)).find_map(|i| {
        (b[i].is_ascii_uppercase() && b[i + 1].is_ascii_digit() && b[i + 2].is_ascii_digit())
            .then(|| String::from_utf8_lossy(&b[i..i + 3]).into_owned())
    })
}

/// **用户拿得到的下一步**的标记。任一命中即算数 —— 钉的是「有没有下一步」，
/// **不是「必须这么措辞」**：换个说法重写诊断不该把判据弄红。
///
/// ⚠ 表里**刻意不放光秃秃的「安装包」三个字**：09-10 之前那版旧诊断第一句就是
/// 「安装包里没有本机后端 sidecar」—— 放进来这条判据对**那句假话恒绿**，
/// 那就又成了一条替假话背书的判据。表里这几个都是**用户点得动 / 做得出的东西**。
const A_STEP_THE_USER_CAN_TAKE: &[&str] = &["-setup.exe", ".msi", "装一次安装包", "装上安装包"];

/// 🔴 **这条判据换过一次靶子（09-10）** —— 换靶的理由比判据本身更该记下来。
///
/// # 原来钉的是什么，为什么必须换
///
/// 原文逐字：`assert!(reason.contains("F05b"), "诊断要指出「谁负责补上它」：{reason}")`。
/// 它**写的时候是对的**：那天 sidecar 确实还没做，诊断指出「谁来补」是有用的。
/// 而 09-10 F05b 随 v3.7.0 发出去之后，**没有人需要「补上它」了** ⇒
/// 这条判据就变成了钉住一个**目的已经过期**的字符串，
/// 让「安装包里还没有 `externalBin`」那句已经变假的话看起来**像有 judge 守着**。
/// **这正是本仓要治的那一形：一条判据替一句已经变假的话背书。**
///
/// # 现在钉的是什么
///
/// **诊断必须给读它的人一条做得到的下一步**（正向，主判据）——
/// 用户拿一个内部件号什么也做不了，他要的是「那我该干嘛」。
/// 外加一条负向：**不许再把内部件号甩到用户脸上**。
/// ⚠ 负向那条**单独不算数**：把件号删干净只是让判据闭嘴，消息可以照样没用 ⇒
/// 正向那条才是本条的立身之本，顺序上也先断它。
///
/// # 它什么时候会红（09-10 造了三刀，**三刀都真红了**，逐字读数见交回件）
///
/// · 刀 1：诊断被改写成只说「找不到」、不说怎么办（最像的一次退化）⇒ 正向那条红；
/// · 刀 2：诊断被改回 09-10 之前那一版（含 `F05b`）⇒ **正向那条先红** ——
///   ⚠ `assert!` 按顺序短路，**负向那条根本没轮到说话**，所以不许写成「两条都红」；
/// · 刀 3：诊断留着「装一次安装包」但把件号加回去 ⇒ **负向那条红**（打出 `F05`）。
///   刀 3 存在的唯一理由就是：没有它，负向那条**从没被证明过会红**。
///
/// # 诚实边界（写死，别读宽）
///
/// 它买的是「消息里有一条**看起来是**下一步的东西」，**买不到**「那句话是真的」，
/// 更买不到「读者真看懂了」—— 把标记词拼进一句废话里，它照样绿。
/// 射程也只有 [`resolve_with`] 这一条 `reason`：
/// [`resolve_beside_this_exe`] 拿不到自身路径那条、[`resolve_or_extract`] 释放失败那条，
/// **本条一条都没盖**。头注 / `DECISIONS` / 模块注释里写件号**不受本条管**，
/// 本条只管**发到用户面前的那一串**。
///
/// # 🔴 `K-R42`（09-10 同日）：**换完靶之后一天，被测对象自己变了 —— 本条现在守什么**
///
/// 本件给自释放那条路接上了「产物自己带着的那份」（[`native_embedded_daemon`]），
/// 〔那条路 `K-R42` 时住 [`start_or_extract`] 体内，`K-R43` 抽进了 [`resolve_or_extract`]〕
/// 于是要先回答一句：**「找不到 sidecar」这一形还存不存在？**
///
/// **存在，而且一点没少。** [`resolve_with`] 的职责一个字没改 —— 它仍然只回答
/// 「**exe 旁边**有没有」，而裸 exe 旁边**仍然没有**（本件不往 exe 旁边放东西）。
/// 变的是**这个答案之后发生什么**：以前它就是终点，现在它是自释放那一支的**入口**。
/// ⇒ 本条守的东西**逐字未变**（那一串给不给得出下一步 · 有没有把件号甩给用户），
/// 而且它守的那一串**仍然到得了用户眼前** —— `resolve_or_extract` 在两个来源都空时
/// 把 `beside` 的 `reason` **原样**交回去，那条路今天照走。
///
/// ⚠ **真正过期的是那一串里的两句话，本件同拍改掉了**（不改就又成了「判据替假话背书」）：
/// ① 「没有它通常意味着跑的是裸 `monitor.exe`」——今天裸 exe 会自己释放一份，这句不再成立；
/// ② 「本机后端今天**未启动**」——那是 `resolve_with` **答不了**的问题，它只看得见旁边。
/// 改完之后本条**照旧命中**（`装一次安装包` 仍在 [`A_STEP_THE_USER_CAN_TAKE`] 里）——
/// 🔴 而这正是要写下来的那一格：**本条没有变红，所以它也没有替本件的改动作证**。
/// 「释放失败」那一形由本条**射程之外**的
/// [`the_extraction_refusal_is_a_different_sentence_from_having_no_backend_at_all`] 接住，
/// 那条正好落在上面那句「本条一条都没盖」点名的三格之一。
#[test]
fn the_missing_sidecar_diagnosis_hands_the_user_a_next_step() {
    let r = resolve_with(
        Path::new("/opt/app"),
        "x86_64-unknown-linux-gnu",
        "",
        &never,
    );
    let Resolved::Missing { reason, .. } = r else {
        panic!("应当是 Missing");
    };
    assert!(
        A_STEP_THE_USER_CAN_TAKE.iter().any(|m| reason.contains(m)),
        "诊断没给读它的人任何一条做得到的下一步（找过 {A_STEP_THE_USER_CAN_TAKE:?}）。\n\
             「本机后端没有」本身**不是**下一步 —— 用户要的是「那我该干嘛」。\n\
             逐字：{reason}"
    );
    let id = internal_item_id_in(&reason);
    assert!(
        id.is_none(),
        "诊断里出现了内部件号 `{}` —— 用户拿它什么也做不了，而且件号会过期\n\
             （`F05b` 就是这么过期的，见本条头注）。件号写头注，别写给用户。\n\
             逐字：{reason}",
        id.clone().unwrap_or_default()
    );
}

/// 🔴 **`K-R42` 硬要求①的判据**：「带了但放不下来」与「压根没带」必须是**两句分得开的话**。
///
/// # 它为什么值一条判据
///
/// 两件事今天共用同一个返回形状（[`Resolved::Missing`]）⇒ **调用方分不开，只有那串字分得开**。
/// 而把后者说成前者，用户会去装一次安装包（没用 —— 他早就有后端了，是目录写不进去）。
/// 09-10 一整天治的就是这一形：读面把「读不到」说成「你没有」。
///
/// # 反向锚点：这条断言**不是靠路径恒真的**
///
/// 固定项 12 那条 `6g` 逐字：诊断常把路径原样印进输出，于是「输出里含某句话」会
/// **靠路径恒真**，把那一支实现整个换掉都不红。⇒ 夹具目录取**中性名**，
/// 并且**当场断言**那个名字里不含标记串（下面第一个 `assert!`）——
/// 少了它，哪天有人把夹具改成 `/tmp/放不下来/` 这条判据就变成了空判据。
///
/// # 诚实边界（写死别读宽）
///
/// · 它买的是「两句话**结构上**分得开」+「后者给得出下一步」，
///   **买不到**「那句话是真的」，更买不到「读者看懂了」（同上一条的边界）。
/// · 底层错误串是**调用方给的**，本条只验它被**原样带出来**，
///   不管那串字本身长什么样（真机上它来自 OS，本条够不着）。
#[test]
fn the_extraction_refusal_is_a_different_sentence_from_having_no_backend_at_all() {
    // 中性名：不含下面任何一个断言用的子串（`6g`）。
    let dir = Path::new("/tmp/ccm-fixture-7/bin");
    let os_err = "Permission denied (os error 13)";
    let refused = extraction_failure_reason(dir, os_err);

    // ── 反向锚点：标记串不许来自夹具的名字 ───────────────────────────
    assert!(
        !dir.to_string_lossy().contains(EXTRACTION_REFUSED_MARKER),
        "夹具目录名里含着标记串 ⇒ 下面那条 `contains` 会靠路径恒真，本条当场作废"
    );

    // ── ① 两句话分得开 ──────────────────────────────────────────────
    let Resolved::Missing { reason: absent, .. } = resolve_with(
        Path::new("/opt/app"),
        "x86_64-unknown-linux-gnu",
        "",
        &never,
    ) else {
        panic!("应当是 Missing");
    };
    assert!(
        refused.contains(EXTRACTION_REFUSED_MARKER),
        "「放不下来」那一句丢了它的标记 ⇒ 调用方与判据都再也分不出它和「没带后端」。\n逐字：{refused}"
    );
    assert!(
        !absent.contains(EXTRACTION_REFUSED_MARKER),
        "「压根没带」那一句也带上了标记 ⇒ 标记不再区分任何东西，两句话又合成一句。\n逐字：{absent}"
    );

    // ── ② 说得出「在哪儿」与「为什么」——否则「响亮」只是嗓门大 ────────
    assert!(
        refused.contains("/tmp/ccm-fixture-7/bin"),
        "没说清写不进去的是**哪个目录** —— 用户拿它没法去改权限。\n逐字：{refused}"
    );
    assert!(
        refused.contains(os_err),
        "底层错误串没被原样带出来 —— `os error 13`（权限）与磁盘满是完全不同的下一步。\n逐字：{refused}"
    );

    // ── ③ 与兄弟那条同职：给得出下一步 · 不许甩件号 ──────────────────
    //    〔铁律 15「我治的是这一处，还是所有同职的地方」：这两格是那一条判据
    //      已经买过的性质，而它的射程逐字写着**盖不到本函数** ⇒ 在这里补齐。〕
    assert!(
        A_STEP_THE_USER_CAN_TAKE.iter().any(|m| refused.contains(m)),
        "「放不下来」那一句没给读它的人任何一条做得到的下一步（找过 {A_STEP_THE_USER_CAN_TAKE:?}）。\n逐字：{refused}"
    );
    let id = internal_item_id_in(&refused);
    assert!(
        id.is_none(),
        "诊断里出现了内部件号 `{}` —— 用户拿它什么也做不了，而且件号会过期。\n逐字：{refused}",
        id.clone().unwrap_or_default()
    );
}

/// 🔴 `K-R42` 硬要求①的**真文件系统那一半**：目标目录建不出来时，
/// [`extract_embedded_to`] **真的**走 `Err`，而那条 `Err` 被
/// [`extraction_failure_reason`] 变成一句与「没带后端」分得开的话。
///
/// # 为什么不用 `chmod 0o555` 造这个失败
///
/// 两条：① `chmod` 要 `std::os::unix`，那是平台原语（本模块的例外额度已经占满）；
/// ② **权限位对 root 不成立** —— 沙箱里跑的是哪个 uid 会改变读数，
/// 而「同一条判据在不同机器上给不同答案」正是本仓要躲的东西。
/// ⇒ 改用**一个普通文件占住父路径**：`create_dir_all` 在任何平台、任何 uid 下都必然失败。
///
/// # 诚实边界（写死别读宽）
///
/// 它证的是「**真有一条 `Err` 走得通，且那条 `Err` 被原样带进了那句话**」。
/// 它**不是**「Windows 上权限不足时的真机行为」—— 那一格今天**判不了**，
/// 要一台真 Windows 机（`%USERPROFILE%` 只读 / 杀毒软件挡写 exe）。别把这一条读成那一条。
#[test]
fn a_directory_it_cannot_create_really_takes_the_loud_path() {
    let base = std::env::temp_dir().join(format!("ccm-kr42-loud-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(&base).expect("建夹具目录");
    // 用一个**普通文件**占住父路径 —— 下面那个 `create_dir_all` 必然失败。
    let blocker = base.join("occupied");
    std::fs::write(&blocker, b"x").expect("写占位文件");
    let dir = blocker.join("bin");

    let err = extract_embedded_to(&dir, "p2e-dial", b"not-a-real-daemon", &|_| Ok(()))
        .expect_err("目标目录的父路径是个普通文件，它居然报了成功");
    let reason = extraction_failure_reason(&dir, &err);
    assert!(
        reason.contains(EXTRACTION_REFUSED_MARKER),
        "真失败走出来的那句话没有标记 ⇒ 它与「这份产物没带后端」又分不开了。\n逐字：{reason}"
    );
    assert!(
        reason.contains(&err),
        "底层失败原文没被原样带出来 —— 「建不出目录」与「盘满」是不同的下一步。\n\
             实得 err：{err}\n逐字：{reason}"
    );
    let _ = std::fs::remove_dir_all(&base);
    println!("KR42-OK 真写不进去时走的是 Err，逐字：{err}");
}

/// 🔴 **`K-R42` 防空转**：自释放那条路**真的问了**「这份产物自己带没带」。
///
/// # 没有这一条会怎样
///
/// [`native_embedded_daemon`] 是纯的、[`extraction_failure_reason`] 是纯的 ——
/// 两条都测得漂漂亮亮，而**只要没人在生产段里接上它们，整件事就是死代码**，
/// 上面那几格照样全绿。本仓这一形有名字（`K-R28` 那条「防空转」逐字记着同一件事）。
///
/// # 🔴 `K-R43`：**靶搬了家，它守的是什么、还守不守得住**
///
/// 〔`K-R42` 立本条时，那三问住 [`start_or_extract`] 体内，本条切的就是那个体。
///  `K-R43` 把那三问整段抽进 [`resolve_or_extract`] ⇒ 靶跟着搬到共用那份上。〕
///
/// **它守的东西逐字未变**：自释放那条路上，「问产物自己带没带」与「释放失败说那句分得开的话」
/// 必须**真的有生产调用点**，而且「问产物」必须排在「找 exe 旁边」之后。
///
/// **还守得住，而且盖的面变大了**：`K-R43` 之后**只有一条自释放路**，
/// 两个生产入口（[`start_or_extract`] · `local_daemon.rs::resolve_daemon_bin`）都走它
/// ⇒ 同一条断言从盖 1 处变成盖 **2** 处。
/// ⚠ **但「都走了它」这一半不在本条射程里** —— 靶搬家之后，谁把 `start_or_extract`
/// 里那行调用删掉、自己再写一遍，本条**照样绿**（它只看共用那份的体）。
/// 那一半由 `local_daemon::the_two_resolution_paths_still_agree_on_the_order` 钉
/// （`K-R43` 同拍换了它的机制，理由住那条的头注）。**两条合起来才是原来那一格，单条都不够。**
///
/// # 顺序那一半
///
/// 「先找 exe 旁边、再动内嵌那份」是本模块头注花一整段论证过的取舍
/// （dev 构建里旁边那个更新）。`K-R42` 新加的那个来源**同属「内嵌那一档」**
/// ⇒ 它也必须排在「找旁边」之后。
///
/// ⚠ **诚实边界**：这是**约定型守卫**（查源码形态，同 `the_backend_layer_stays_host_agnostic`
/// 那一族）—— 它挡得住「接线被删掉 / 顺序被写反」，挡不住「换个名字继续错」。
#[test]
fn the_self_extract_path_really_asks_the_product_whether_it_carries_one() {
    let body = shared_resolution_body();
    // 反向自检：切出来的确实是那个函数体（切歪了下面几条会在一段不相干的文本上恒答）。
    // ⚠ 三处都走 `find_pinned`（**恰好一处** + 两侧有边界），不用裸 `contains` ——
    //   `needle_anchor_registry` 那条递减棘轮治的就是「匹配单位比事实小」。
    guard_core::find_pinned(&body, "extract_embedded_to(").unwrap_or_else(|e| {
        panic!("切出来的这段里 `extract_embedded_to(` 不是恰好一处（{e}）—— 切歪了，本条此刻无效。\n逐字：{body}")
    });

    // ★ **防空转的第二半**〔本轮 `7u` 那一刀逼出来的，逐字记下理由〕：
    //   把实现整个退掉之后，`the_extraction_refusal_…` 那条**仍然绿** ——
    //   它测的是一个**纯函数**，而纯函数不接调用点也照样对。
    //   ⇒ 「那句响亮的话真的被用上了」得在这里钉，不能指望那一条。
    guard_core::find_pinned(&body, "extraction_failure_reason(").unwrap_or_else(|e| {
        panic!(
            "那份共用的解析体内 `extraction_failure_reason(` 不是恰好一处（{e}）——\n\
                 一处都没有 ⇒ 释放失败又退回那句**与「没带后端」分不开**的话，\n\
                 而 `the_extraction_refusal_…` 这类纯函数判据**照样全绿**（本轮实测过）。\n逐字：{body}"
        )
    });
    let asked = guard_core::find_pinned(&body, "native_embedded_daemon").unwrap_or_else(|e| {
        panic!(
            "那份共用的解析体内 `native_embedded_daemon` 不是恰好一处（{e}）——\n\
                 一处都没有 ⇒「产物自己带着的那份」没有任何生产调用点：\n\
                 裸 exe 回到 09-10 那个读数（0 个本机后端进程），而本模块每一条判据照样绿。\n\
                 多于一处 ⇒ 有第二条取法，下面那条顺序断言就说不清它断的是哪一处。\n逐字：{body}"
        )
    });
    let beside = guard_core::find_pinned(&body, "resolve_beside_this_exe(")
        .expect("那份共用的解析体内 `resolve_beside_this_exe(` 不是恰好一处");
    assert!(
        beside < asked,
        "「问产物自己带没带」排到了「找 exe 旁边」前面 —— 顺序反了。\n\
             dev 构建里旁边那个是**更新**的，内嵌那份是打包时的快照；\
             顺序一反，同一台机上两条路会找到不同的二进制。"
    );
}

/// `K-R42`：释放出来那个文件名带**目标平台**的可执行后缀。
///
/// # 两个断言各自在哪个平台上非空
///
/// | | Linux / macOS（后缀 = 空串） | Windows（后缀 = `.exe`） |
/// |---|---|---|
/// | 「带后缀」那条 | **空真**（`ends_with("")` 恒真） | 真 |
/// | 「不带后缀时逐字不变」那条 | **真**（本件不许让 Linux 那条退化） | 空真 |
///
/// ⇒ 两条**各有一个平台上是空真**，而本仓 CI 的 `rust` job 跑在 **windows-latest**、
/// 门禁跑在 Linux ⇒ **两侧合起来才有牙，单侧都不够**。这一格如实写在这里，别读成「验过了」。
/// 「编得过」那一半由门禁的 `winchk`（`cargo check --target x86_64-pc-windows-gnu`）买。
/// `K-R42`：内嵌那份的**落点路径**，两侧拼法必须一致。
///
/// # 为什么会有两处
///
/// 闭集本该只有一个住址（把名字 emit 成编译期 env 就够了）——**`include_bytes!` 的语法
/// 不许**：它只吃字面量，`concat!(env!(..), ..)` 那种写法会撞
/// `cross_half_edge_registry` 那条「非字面量 include 必须登记」的默认拒绝，
/// 而那张登记表不在本件写区。⇒ 两处字面量是**被语法逼出来的**，不是懒。
/// 逼出来的重复由**判据**补：这一条现读 `build.rs` 里那两个常量的**值**，
/// 再去生产段里找拼出来的那条路径 —— 任一侧改名，这里当场红。
///
/// ⚠ 诚实边界：它对的是**拼法**，不是「那个文件真在那儿」——
/// 真不在时 `build.rs` 不置 cfg，`include_bytes!` 整个不参与编译（那一格由构建本身守）。
#[test]
fn the_native_daemon_path_is_spelled_the_same_on_both_sides() {
    // 🔴 **必须是 `include_str!`，不许 `std::fs::read_to_string`** —— 后者是
    //    `needle_anchor_registry::CORPUS_SEEDS` 的**种子**：写下它，这个函数里的局部
    //    （`line` / `s` / `e` …）会被那条棘轮的传递闭包一路认成「语料变量」，
    //    而单字母名字与本文件别处的局部**重名** ⇒ 一处**与本件毫无关系**的既有
    //    `e.contains("再开一次")` 当场被算进欠账，棘轮 33 → 34 变红（实测）。
    //    ⚠ 这正是那条棘轮自己头注里记着的「判据自己跑飞」，只是这次是我喂的种子。
    //    局部也一并改成长名字，别再给传递闭包留同名的落脚点。
    let build_rs = include_str!("../../../../src/bridge/build.rs");
    let spelled = |konst: &str| -> String {
        let decl = build_rs
            .lines()
            .find(|l| l.contains(konst) && l.contains("&str ="))
            .unwrap_or_else(|| panic!("`build.rs` 里找不到 `{konst}` 的声明 —— 改名了？"));
        let open = decl.find('"').expect("常量声明里没有字面量") + 1;
        let close = decl[open..].find('"').expect("字面量没闭合");
        decl[open..open + close].to_string()
    };
    let dir = spelled("NATIVE_DAEMON_DIR");
    let file = spelled("NATIVE_DAEMON_FILE");
    let prod = guard_core::production_code(include_str!(
        "../../../../src/bridge/src/backend/control/local_backend.rs"
    ));
    // 🔴 〔2026-09-18 修笔误〕原来是 `"\"../../..{dir}/{file}\""` —— **`../../..` 与
    // `{dir}` 之间少一个 `/`**，拼出来是 `"../../..native-daemon/…"`，而代码里是
    // `"../../../native-daemon/…"` ⇒ `contains` 永远不成立。
    // ⚠ 如实说：**我解释不了它以前怎么过的** —— 常量近 6 个提交都是 `"native-daemon"`
    // （无前导斜杠），本测试也没有平台门控，而 CI 的 Windows job 会跑它。
    // 可能是那条 job 有一段时间没绿过；没有证据就不编一个说法。
    let want = format!("\"../../../{dir}/{file}\"");
    assert!(
        prod.contains(&want),
        "`build.rs` 铺的是 `src/bridge/{dir}/{file}`，而本文件的 `include_bytes!` \
             没有一处拼成 {want} ⇒ 两侧对不上。\n\
             对不上的表现**不是编译错**：`build.rs` 那侧照样置 cfg，而这一侧 include 到\
             另一个路径 —— 要么编不过（好），要么嵌进一份别的东西（坏）。"
    );
}

#[test]
fn the_extracted_name_carries_the_target_exe_suffix() {
    let suffix = env!("CCM_TARGET_EXE_SUFFIX");
    let name = local_extract_name("p2e-dial");
    assert!(
        name.ends_with(suffix),
        "释放名 `{name}` 没带目标平台的可执行后缀 `{suffix}` —— \
             在把扩展名当身份的平台上，那个文件起不起得来是碰运气"
    );
    assert!(
        !suffix.is_empty() || name.ends_with("p2e-dial"),
        "后缀是空串，而释放名 `{name}` 却不再以 build_id 收尾 —— \
             本件在没有后缀的平台上**必须逐字不变**（盘上已有的那份要照旧命中）"
    );
    // 后缀是**编译期常量**、不是现算的平台原语 —— 现算要 `env::consts::`，
    // 而本文件在 `PLATFORM_EXCEPTIONS` 里的例外额度已经占满（那张表挂着递减棘轮）。
    // ⚠ 用 `contains_word`（两侧有边界）而不是裸 `contains` —— 后者被
    //   `needle_anchor_registry` 那条递减棘轮数着，而这里也确实不该用子串匹配。
    //   **不用 `find_pinned`**：这个名字在本文件里还出现在头注里，本条要的是
    //   「生产段还引着它」，不是「只出现一次」。
    let prod = guard_core::production_code(include_str!(
        "../../../../src/bridge/src/backend/control/local_backend.rs"
    ));
    assert!(
        guard_core::contains_word(&prod, "CCM_TARGET_EXE_SUFFIX"),
        "生产段里不再引用那个编译期常量 —— 后缀要么被写死成空串（Windows 上退化），\
             要么被改成现算的平台原语（当场撞 `the_backend_half_stays_platform_agnostic`）"
    );
}

#[test]
fn the_triple_suffixed_candidate_wins_over_the_bare_one() {
    // Tauri 打包出来的就是带 triple 的那个名字；裸名只是兜底。
    let triple = "aarch64-apple-darwin";
    let want = PathBuf::from("/opt/app").join(format!("{SIDECAR_STEM}-{triple}"));
    let w = want.clone();
    let exists = move |p: &Path| p == w.as_path() || p.ends_with(SIDECAR_STEM);
    assert_eq!(
        resolve_with(Path::new("/opt/app"), triple, "", &exists),
        Resolved::Found(want)
    );
}

#[test]
fn windows_exe_suffix_is_carried_into_both_candidates() {
    let c = sidecar_candidates(Path::new("C:/app"), "x86_64-pc-windows-msvc", ".exe");
    assert!(
        c.iter().all(|p| p.to_string_lossy().ends_with(".exe")),
        "Windows 上两个候选都得带 .exe：{c:?}"
    );
}

#[test]
fn a_single_crash_restarts() {
    let l = CrashLimits {
        max_crashes: 3,
        window_ms: 10_000,
    };
    assert_eq!(decide(&[1_000], 1_000, l), Decision::Restart);
    assert_eq!(decide(&[1_000, 2_000], 2_000, l), Decision::Restart);
}

#[test]
fn hitting_the_cap_gives_up_and_says_why() {
    let l = CrashLimits {
        max_crashes: 3,
        window_ms: 10_000,
    };
    // ★ 边界：第 3 次就该放弃（`>=`，不是 `>`）。
    let Decision::GiveUp { reason } = decide(&[1_000, 2_000, 3_000], 3_000, l) else {
        panic!("第 3 次崩溃就该放弃 —— 差一位的错本仓出现过");
    };
    assert!(reason.contains("崩了 3 次"), "诊断要带实际次数：{reason}");
    assert!(
        reason.contains("远端功能不受影响"),
        "放弃了要说清影响面：{reason}"
    );
}

#[test]
fn crashes_outside_the_window_do_not_count() {
    let l = CrashLimits {
        max_crashes: 3,
        window_ms: 10_000,
    };
    // 两次很久以前 + 一次刚刚 ⇒ 窗口内只有 1 次 ⇒ 重起。
    assert_eq!(
        decide(&[1_000, 2_000, 100_000], 100_000, l),
        Decision::Restart
    );
    // 窗口**下沿是闭区间**：正好在 now-window 上的那次算进来。
    let Decision::GiveUp { .. } = decide(&[90_000, 95_000, 100_000], 100_000, l) else {
        panic!("窗口内 3 次就该放弃");
    };
}

#[test]
fn the_window_floor_does_not_underflow_near_zero() {
    // `now_ms` 比 window 还小时 `saturating_sub` 兜住；不兜的话是 panic 而不是判错。
    let l = CrashLimits {
        max_crashes: 2,
        window_ms: 10_000,
    };
    assert_eq!(decide(&[0], 5, l), Decision::Restart);
    let Decision::GiveUp { .. } = decide(&[0, 1], 5, l) else {
        panic!("窗口下沿被钳到 0 之后，两次都该算进来");
    };
}

/// ★ **零定时器的编译期/源码钉**（C12）。
///
/// 两条一起：`Decision` 里不许出现「隔多久再来」这种字段；
/// 生产段里不许出现 `sleep`、也不许出现 `try_wait`（那是轮询，本模块的替代方案见头注）。
#[test]
fn nothing_in_the_production_path_wakes_itself_up() {
    match Decision::Restart {
        Decision::Restart => {}
        Decision::GiveUp { .. } => {}
    }
    let src = guard_core::production_code(include_str!(
        "../../../../src/bridge/src/backend/control/local_backend.rs"
    ));
    // 判据串运行时拼，免得命中本文件自己的头注（那里逐字讨论过这两个词）。
    for bad in [format!("thread::{}", "sleep"), format!("try_{}", "wait()")] {
        assert!(
            !src.contains(&bad),
            "生产段出现了 `{bad}` —— 它是「自己醒过来」的构件（C12）。\n\
                 等子进程死请读它 stdout 到 EOF（见模块头注的三条路对比）。"
        );
    }
}

/// ★ 生产接线的**前提钉**：本模块今天**不许**被接成「扫仓库 dev 产物」。
///
/// 摸底量到 daemon 一启动就无条件往 tmux server 装全局 hook 且没有开关 ⇒
/// 扫到 dev 产物就起它会去改用户真实 tmux 的状态。这条钉住那个前提：
/// 候选路径里**只能有 exe 同目录**，出现任何 `target`/`debug`/仓库相对路径就红。
/// ★★ **F06b-1d：给窗口的那份 env —— 名字从唯一的家来，sidecar 不在就不设。**
///
/// 判据形态：**纯函数**（跑法：单测 · 钉的性质：wire/边界映射 —— 两维分开写，见 `ROADMAP §4`
/// 登记的计量缺陷）。它钉两件：
/// ① `Found` ⇒ 键**必须**是 [`super::DAEMON_BIN_ENV`]（不是另抄一个字面量），值是那条真路径；
/// ② `Missing` ⇒ **`None`，不是 `Some((名, ""))`** —— 导一个指向空处的路径不会让 ccm 更聪明
///    （它那边 `[ -x ]` 一样过不了），只会给「这台机有没有本机后端」多一个假阳性来源。
#[test]
fn the_window_env_uses_the_one_home_and_stays_silent_without_a_sidecar() {
    let found = super::env_from_resolved(super::Resolved::Found("/tmp/x/ccm-remote".into()));
    let (k, v) = found.expect("Found 必须给出一对 env");
    assert_eq!(
        k,
        super::DAEMON_BIN_ENV,
        "给窗口的 env 名没走唯一的家 —— 有人另抄了一个字面量"
    );
    assert_eq!(v, "/tmp/x/ccm-remote", "值必须是解析出来的那条真路径");

    let missing = super::env_from_resolved(super::Resolved::Missing {
        reason: "测试".into(),
        looked_at: vec![],
    });
    assert!(
        missing.is_none(),
        "sidecar 不在时必须**什么都不设**，而不是设一个空值 —— \n\
             空值 ≠ 未设（Z01 那条支点）：ccm 那边 `[ -x ]` 照样过不了，\n\
             却给「这台机有没有本机后端」多造了一个假阳性来源。"
    );
}

/// ★★ **那个 env 名只有一个家**〔F06b-1 立，F06b-1c 起转为实断言〕。
///
/// # 🔴 `K-R48` 第二拍（09-11）：**射程砍掉一半，写清楚砍的是哪一半**
///
/// 原来它有两条腿：① 本文件生产段里那个字面量只许出现 1 次；
/// ② `shared/ccm` 里出现的每一处「像那个名字」的拼写都必须与 Rust 侧**逐字相同**
///（名字打错的后果是「ccm 永远读不到 ⇒ 永远走本地那条」，而那看起来完全正常）。
///
/// 〔用@09-11 `K33`〕「**不要有什么 bash 脚本**」⇒ `shared/ccm` 删了，
/// **第②条腿没有被测对象了**：今天不存在「另一个进程按名字去读这个 env」这回事，
/// 敲的那个命令就是后端。⇒ 只留①。
///
/// ⚠ **如实边界**：①**买不到**②买的那件事（「两处拼写一致」）。今天那个风险不存在，
/// 是因为**对侧没了**，不是因为有判据盯着 —— 哪天再出现一个按名字读它的消费者，
/// 这条判据**不会**替你盯着它。
#[test]
fn the_daemon_bin_env_name_has_exactly_one_home() {
    let me = guard_core::production_code(include_str!(
        "../../../../src/bridge/src/backend/control/local_backend.rs"
    ));
    let lit = format!("\"{}\"", super::DAEMON_BIN_ENV);
    let n_lit = me.matches(lit.as_str()).count();
    assert_eq!(
        n_lit, 1,
        "`local_backend.rs` 的生产代码里字面量 {lit} 出现了 {n_lit} 次 —— \n\
             唯一的家是那个 `const`，其余一律引用它。\n\
             ⚠ 值相等的断言**看不见副本**（副本的值按定义就相等），只有数源码才看得见。"
    );
}

/// ★★ **F05b 接线钉：每一个打包 job 都必须给 sidecar 备好料。**
///
/// # 为什么这条是「遍历发现」而不是「数一遍」
///
/// `externalBin` 一旦注入，`tauri build` 就要求**当前 target** 的那份二进制存在，
/// 少了它整个 job 以 `resource path ... doesn't exist` 失败（本机实测过）。
/// ⇒ 发版流水线里**每一处** `tauri build` 都得配三件：原生编 daemon · 按 triple 命名放好 ·
/// `--config` 注入补丁。少任何一件那个 job 就红，而**发版红是最贵的红**（tag 已经打出去了）。
///
/// 所以发现机制是**遍历 workflow 里所有 `tauri build` 调用**，不是手写「有两个 job」。
///
/// ⚠ 中间量自检：先断言真的找到了 `tauri build` 调用（找不到 = 抽取器坏了，本条零命中地绿）。
#[test]
fn every_bundle_job_stages_the_sidecar_before_building() {
    let root = crate::guard_support::repo_root();
    let wf = root.join(".github/workflows/release.yml");
    let src = std::fs::read_to_string(&wf).expect("读不到 release.yml");
    // 运行时拼，免得命中本文件自己的说明文字。
    let verb = format!("{} build", "tauri");
    let calls: Vec<&str> = src
        .lines()
        .filter(|l| l.contains(&verb) && l.trim_start().starts_with("run:"))
        .collect();
    assert!(
        calls.len() >= 2,
        "在 release.yml 里只找到 {} 处 `{verb}` 调用 —— 抽取器坏了或流水线变形了\n\
             （实测两处：Windows 的 nsis+msi 与 Linux 的 deb）。本条会零命中地绿。",
        calls.len()
    );
    for c in &calls {
        assert!(
            c.contains("tauri.sidecar.conf.json"),
            "这处打包没有注入 sidecar 补丁配置：{}\n\
                 ⇒ 安装包里不会带本机后端（C7），而 `local_backend` 会恒走诚实降级。",
            c.trim()
        );
    }
    // 三件里的另外两件：原生编 + 按 triple 命名。逐个 job 数得太脆，
    // 这里钉「整份 workflow 里这两件各至少与打包调用数一样多」。
    let native = src.matches("Build local backend sidecar").count();
    let staged = src.matches("Stage sidecar for externalBin").count();
    assert!(
        native >= calls.len() && staged >= calls.len(),
        "打包调用 {} 处，而「原生编 daemon」{native} 处、「按 triple 放好」{staged} 处 —— \n\
             有 job 会以 `resource path ... doesn't exist` 失败，而那是**发版时**才炸。",
        calls.len()
    );
    // sidecar 的名字只有一个家：这里不抄它，从 Rust 侧读。
    assert!(
        src.contains(super::SIDECAR_STEM),
        "release.yml 里没有出现 `{}` —— 拷过去的名字与消费侧对不上",
        super::SIDECAR_STEM
    );
}

/// ★★ 🔴 `KR70D3`（09-12）：**载体①与载体③是同一次构建 —— 这件事从此有人守。**
///
/// # 题面（`K-R68` 现打，`DECISIONS.md#R26` 裁定一）
///
/// 发版流水线里同一个文件被拷了两次：
/// `.build/backend/release/cc-monitor-remote.exe`
/// → `src/bridge/binaries/…`（载体③，Tauri `externalBin`，装机那份）
/// → `src/bridge/native-daemon/cc-monitor-native`（载体①，自释放那份）。
/// 两步之间**一条 `cargo` 都没有** ⇒ 它们逐字节相同，是最强的那种同源。
/// 🔴 **而这件事此前没有任何断言守着**：中间插一条 `cargo build`（换个 feature、
/// 换个 profile、甚至只是重编一次）就不再是同一份字节，**没人会红**。
/// 而两份字节不同、身份戳却相同（同一个 `BUILD_ID`）时，产品**分不出它们** ——
/// 那正是 `K-R70` 这一件的题面本身。
///
/// # ⚠ 本条**不主张**「载体②也要同字节」
///
/// ② 是另一个 job（ubuntu / `cargo zigbuild` / musl），**结构上不可能**逐字节相同 ——
/// 那正是 `K25` 裁的形状（「一份代码、每个平台编出自己那一份原生二进制」），不是缺陷。
/// ⇒ 本条的射程只到「同一个 job 里、拷同一个路径的那两步」。
///
/// # 量法与它的边界
///
/// 人群 = `release.yml` 里**拷贝那个原生产物**的所有行（锚是那条产物路径，不是步骤名 ——
/// 步骤名是散文，路径是事实），**按 job 分组**。断言：**同一个 job 内**，第一处与
/// 最后一处拷贝之间没有非注释的构建命令。
///
/// ⚠ **为什么按 job 分组，而不是拿全仓那几处一起比** —— 这一格是本条第一次跑就
/// 逮出来的（写它的时候以为是 2 处，实得 **3** 处）：第三处在 `build-linux` 里，
/// 它同样把那个原生产物拷成载体③，而 **Linux 那条路根本没有载体①**
/// （`K-R68` 现打：`native-daemon` 那三个字只出现在 `build-windows` 一个 job 里 ——
/// 已立成待决 `KU26` 问用户）。跨 job 比是**分母错**：两个 job 各自 `checkout` 各自编，
/// 它们之间当然有构建动作，那不是缺陷。
///
/// ⚠ 它**看不见**「构建命令写在别的文件里、由这里 `run:` 一个脚本触发」那一形；
/// 今天 `release.yml` 里这一段没有那种写法（人群里每一行都是内联的 `Copy-Item`/`cp`），
/// 真出现那一形要另加一条判据。**这是登记的射程，不是穷举过的全称。**
#[test]
fn the_two_carriers_are_copies_of_one_build_with_nothing_rebuilt_between() {
    let root = crate::guard_support::repo_root();
    let wf = root.join(".github/workflows/release.yml");
    let src = std::fs::read_to_string(&wf).expect("读不到 release.yml");
    let lines: Vec<&str> = src.lines().collect();
    // 锚：那个**原生产物**的路径。运行时拼，免得命中本条自己的说明文字。
    let artifact = format!(".build/backend/release/{}", super::SIDECAR_STEM);
    // job 边界：`jobs:` 下**两空格缩进**的那一层键。
    let job_at = |i: usize| -> &str {
        lines[..=i]
            .iter()
            .rev()
            .find(|l| {
                l.len() > 2
                    && l.starts_with("  ")
                    && !l.starts_with("   ")
                    && l.trim_end().ends_with(':')
            })
            .map(|l| l.trim().trim_end_matches(':'))
            .unwrap_or("<不在任何 job 里>")
    };
    let is_copy = |l: &str| {
        let t = l.trim_start();
        !t.starts_with('#')
            && l.contains(&artifact)
            && (t.starts_with("Copy-Item") || t.starts_with("cp "))
    };
    let copies: Vec<usize> = lines
        .iter()
        .enumerate()
        .filter(|(_, l)| is_copy(l))
        .map(|(i, _)| i)
        .collect();
    // 中间量自检：找不到 = 抽取器坏了 / 流水线变形，本条会零命中地绿。
    assert!(
        copies.len() >= 2,
        "在 release.yml 里只找到 {} 处「拷 `{artifact}`」（09-12 实测 3：\n\
             `build-windows` 两处（载体③ ＋ 载体①）· `build-linux` 一处（只有载体③））—— \n\
             抽取器坏了或那几步被改名/挪走，**本条此刻是无效的**。",
        copies.len()
    );
    // 词表如实登记：它是「已经栽过 + 想得到」的那几个，不是全称。
    const BUILD_VERBS: [&str; 3] = ["cargo build", "cargo zigbuild", "cargo run"];
    // 🔴 `rustc` 要分「编」与「问」—— **这一格是本条第一次跑就被它自己逮到的**：
    //    `Stage native daemon for self-extract` 头一行是 `rustc -vV | …` 取 target triple，
    //    那是一次**查询**，一个字节都没编。把它读成构建就是一条自信的假报警
    //    （`C17`：诊断不许比它证得出的说得更死）。
    const RUSTC_QUERIES: [&str; 4] = ["rustc -vV", "rustc -V", "rustc --version", "rustc --print"];
    let is_build_action = |t: &str| -> bool {
        if t.starts_with('#') {
            return false;
        }
        if BUILD_VERBS.iter().any(|v| t.contains(v)) {
            return true;
        }
        t.contains("rustc ") && !RUSTC_QUERIES.iter().any(|q| t.contains(q))
    };
    let mut jobs_with_a_pair = 0usize;
    for job in {
        let mut js: Vec<&str> = copies.iter().map(|i| job_at(*i)).collect();
        js.dedup();
        js
    } {
        let mine: Vec<usize> = copies
            .iter()
            .copied()
            .filter(|i| job_at(*i) == job)
            .collect();
        let (a, b) = (mine[0], *mine.last().expect("非空"));
        if b <= a + 1 {
            continue; // 这个 job 只拷一次（或两次挨着）⇒ 没有「之间」可查
        }
        jobs_with_a_pair += 1;
        let offenders: Vec<String> = lines[a + 1..b]
            .iter()
            .enumerate()
            .filter(|(_, l)| is_build_action(l.trim_start()))
            .map(|(k, l)| format!("release.yml:{} 逐字 `{}`", a + 2 + k, l.trim()))
            .collect();
        assert!(
            offenders.is_empty(),
            "job `{job}` 里，那个原生产物被拷去两个载体，而**两次拷贝之间出现了构建动作**：\n  {}\n\
                 ⇒ 它们**不再是同一份字节**，而两份字节会带着**同一个 `BUILD_ID`** 出货\n\
                 （身份戳取自源码常量）⇒ 产品分不出它们，`K33`「后端只有一个」在身份这一维当场破。\n\
                 真要在中间重编，得先答一个问题：**那两份字节凭什么还叫同一个身份？**\n\
                 （`DECISIONS.md#R26` 裁定一 · 本条的射程与边界见头注 —— 载体②不在里面，\n\
                  它是另一个 job / musl，结构上不可能同字节，那是 `K25` 裁的形状。）",
            offenders.join("\n  ")
        );
    }
    // 反空真：至少有一个 job 真的拷了两次，否则上面整段是空转的。
    assert!(
        jobs_with_a_pair >= 1,
        "没有任何一个 job 把那个原生产物拷去两个载体 —— 上面那段检查此刻**一格都没跑**。\n\
             （09-12 实测 `build-windows` 是那一个：`Stage sidecar for externalBin` → 载体③、\n\
              `Stage native daemon for self-extract` → 载体①。）"
    );
}

#[test]
fn candidates_never_point_into_a_build_tree() {
    let c = sidecar_candidates(Path::new("/opt/app"), "x86_64-unknown-linux-gnu", "");
    for p in &c {
        let s = p.to_string_lossy();
        for forbidden in ["target", "debug", "release", ".."] {
            assert!(
                !s.contains(forbidden),
                "候选路径含 `{forbidden}`：{s}\n\
                     扫 dev 产物就起 daemon = 去改用户真实 tmux server 的状态（它无条件装全局 hook，\
                     而且没有开关）。只许在 exe 同目录找。"
            );
        }
        assert!(p.starts_with("/opt/app"), "候选跑出了 exe 目录：{s}");
    }
}

/// ★ **生产接线钉**：`lib.rs` 的启动路径**真的**调了本模块的生产入口。
///
/// 这条是 F03 教训的直接产物：「模块存在 ≠ 模块被调用」。
/// 本模块写得再全，只要 `lib.rs` 里没那一行，本机后端就永远不会被起 ——
/// 而上面那些单测**全都照样绿**。
///
/// ⚠ **P2z 改过一次口径，记下为什么不是「改弱」**：原来钉的是字面量
/// `local_backend::start_if_present`。P2z 把接线换成了 `start_or_extract`
/// （exe 旁边没有就释放内嵌那份），那条字面量当场红 —— **它在做它的岗位**。
/// 改法不是把它删掉、也不是换成两个名字任选其一（那会让「一个都没接」漏网），
/// 而是钉「**至少接了一个已知生产入口，且那个入口确实存在于本模块**」。
/// ⇒ 将来再改入口名，这条仍会红，除非同时在这张清单里登记 —— 那正是要的。
#[test]
fn the_startup_path_really_calls_this_module() {
    // ⚠ **P2s 搬过一次家**：接线原来住 `lib.rs` 的 `run()` 里，P2s 把它抽进
    // `local_daemon.rs`（理由是结构性的：`#[tauri::command]` 不能与 `generate_handler!`
    // 同模块）。⇒ 语料从「一个文件」变成「启动路径这两个文件」。
    // **这不是把判据放宽**：仍然要求「至少一个已知入口被接上」，只是接线可以住这两处之一；
    // 两个文件都不接，照样红（M8 变异实测）。
    let prod = format!(
        "{}\n{}",
        guard_core::production_code(include_str!("../../../../src/bridge/src/lib.rs")),
        guard_core::production_code(include_str!("../../../../src/bridge/src/local_daemon.rs")),
    );
    let prod = prod.as_str();
    let me = guard_core::production_code(include_str!(
        "../../../../src/bridge/src/backend/control/local_backend.rs"
    ));
    // 本模块今天对外的生产入口清单。加入口 = 往这里加一条（**不许**留空清单）。
    assert!(
        !ENTRIES.is_empty(),
        "抽取器自检：入口清单空了 ⇒ 下面两条断言都会零命中地绿"
    );
    // ★★ **完备性自检**〔D 阶段补审 08-11 新增〕：`ENTRIES` 是**手写白名单**，
    // 原来只校验「清单里的名字存在」与「清单非空」，**没有任何一条校验它是完备的**。
    // ⇒ 新增第三个启动入口（不走 `supervise_with_stdio` / 不传消费者）时，
    // `the_production_entry_hands_the_stdio_consumer_down` **逮不到**——它只遍历清单。
    //
    // 完备性怎么判：本模块里**每一个调了 `supervise`（含 `_with_stdio`）的 `pub fn`**
    // 都必须在清单里。`supervise` / `supervise_with_stdio` 自己除外（它们是被调的那一方）。
    {
        let mut missing: Vec<String> = Vec::new();
        let mut seen = 0usize;
        for (i, l) in me.lines().enumerate() {
            let t = l.trim_start();
            if !t.starts_with("pub fn ") {
                continue;
            }
            let name: String = t["pub fn ".len()..]
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            if name == "supervise" || name == "supervise_with_stdio" {
                continue;
            }
            let body: String = me
                .lines()
                .skip(i + 1)
                .take_while(|l| *l != "\u{7d}")
                .collect::<Vec<_>>()
                .join("\n");
            if !body.contains("supervise") {
                continue;
            }
            seen += 1;
            if !ENTRIES.contains(&name.as_str()) {
                missing.push(name);
            }
        }
        assert!(
            seen >= 2,
            "只扫到 {seen} 个「会起进程的 pub fn」—— 抽取面坏了，完备性自检在空转"
        );
        assert!(
            missing.is_empty(),
            "这些 `pub fn` 会起被监护的进程，却**不在 `ENTRIES` 清单里**：{missing:?}\n\
                 ⇒ 手写白名单漏了它 ⇒ `the_production_entry_hands_the_stdio_consumer_down`\n\
                 只遍历清单，**逮不到这条新入口没接消费者**。清单要跟着实际入口走。"
        );
    }
    for e in ENTRIES {
        assert!(
            me.contains(&format!("pub fn {e}(")),
            "入口清单里的 `{e}` 在本模块生产段里不存在 —— 清单腐了。\n\
                 （这条防的是「把清单写宽以求绿」：写一个不存在的名字进去也不许过。）"
        );
    }
    let wired: Vec<&str> = ENTRIES
        .iter()
        .copied()
        .filter(|e| prod.contains(&format!("local_backend::{e}")))
        .collect();
    assert!(
        !wired.is_empty(),
        "`lib.rs` 的生产段里一个本模块生产入口都没有（找过 {ENTRIES:?}）—— \n\
             本机后端没有被接上启动路径。判据全绿而功能从不运行，\n\
             正是「模块存在 ≠ 模块被调用」那个坑。"
    );
    assert!(
        prod.contains("CCM_TARGET_TRIPLE"),
        "`lib.rs` 生产段里找不到 `CCM_TARGET_TRIPLE` —— 接线缺了 target triple 这个宿主知识"
    );
    // 句柄必须被**存下来**：不存就没人能 `stop()`。
    //
    // ⚠ **原版是恒绿的**〔D 阶段补审 08-11 逮到〕：它断言 `prod.contains("LOCAL_BACKEND")`，
    // 而 `prod` = `lib.rs` + `local_daemon.rs` 的生产段，**那个静态量的声明就在后者里**
    //（`pub static LOCAL_BACKEND: …`）。把「存进去」那一句删掉，字面量照样在（声明处 + 三个读者）
    // ⇒ 绿。要让它红只能删掉静态量本身，而那会先编译错。
    // ⇒ 改成钉**赋值那一句**（`*g = Some(h)`），那才是「存下来」这件事。
    guard_core::find_pinned(&prod, "*g = Some(h);").unwrap_or_else(|e| {
        panic!(
            "监护句柄没被**存**进 `LOCAL_BACKEND`（{e}）—— 退出时无法 `stop()`。\n\
                 ★ 别把「文件里提到过 LOCAL_BACKEND」当成证据：声明本身就提到它。"
        )
    });
}

/// ★ **接线钉之二：退出路径真的收尸。**
///
/// ⚠ 这条是 **F+02 回看抓出来的洞**，而不是 F05a 自己想到的：
/// F05a 当时把退出钩子整段删掉做变异，**815 条测试全绿**，CI 也不会红
/// （本仓 clippy 只报警告、不带 `-D warnings`）。
/// 也就是说「不 `stop()` 就成游魂进程」这条性质当时**只被 clippy 的 `dead_code` 偶然覆盖**
/// —— 一旦别处也用到 `stop()`，那条告警就消失，这条性质彻底失守。
///
/// **「靠一条告警守着」等于没守。** 上一条接线钉只钉了「入口被调用」，
/// 这条钉「出口也被接上」——同一件事的两半。
///
/// # ★★ P2s 翻面（08-11）：原来钉「必须 `stop()`」，现在钉「必须**按策略**决定 stop」
///
/// `C8` 裁定「每台机一个开关，管随 monitor 退出是否 kill，**默认不 kill**」
/// ⇒ 无条件 `stop()` 与定框直接冲突，这条判据必须翻面。
///
/// ⚠ **翻面最容易翻成一条更弱的判据**（比如只钉「体内提到了策略这个词」）。
/// 所以本条保留了它原来要挡的那个洞，并额外挡住翻面自身可能引入的两个新洞：
///
/// | 洞 | 谁挡 |
/// |---|---|
/// | 整段退出钩子被删掉（原洞，实测「815 条测试全绿」） | `RunEvent::Exit` 那条 needle |
/// | 退回无条件 `stop()`（默认被违反，用户没设也被杀） | 「`.stop()` 必须在策略读之后、且中间有条件」 |
/// | 读了策略却永远不 stop（开关拨到「杀」也没反应） | 「体内必须有 `.stop()`」 |
///
/// ★ **原理由（「不 stop 就成游魂进程」）实测是错的**〔08-11，P2s §0a〕：
/// daemon 是纯 stdio 子进程，monitor 一退读端就断，它 **153 毫秒**内自己 broken-pipe 退出。
/// ⇒ 不杀**不会**留游魂。那条理由曾是这条判据存在的全部依据，现在它的依据换成了
/// 「开关必须真的起作用，两个方向都要」。**依据换了就写出来，不假装它没变。**
#[test]
fn the_exit_path_really_stops_the_local_backend() {
    let prod = guard_core::production_code(include_str!("../../../../src/bridge/src/lib.rs"));
    for needle in ["RunEvent::Exit", "LOCAL_BACKEND", ".stop()"] {
        assert!(
            prod.contains(needle),
            "`lib.rs` 的生产段里找不到 `{needle}` —— 退出路径没接上。\n\
                 ⚠ 这条洞**不会被别的判据抓到**（实测：删掉整段钩子，815 条测试全绿），\
                 而 clippy 的 dead_code 只是偶然覆盖。"
        );
    }
    // ⚠⚠ **08-08 订正：原来这里比的是「文件里最后一个 `.stop()` 在不在 Exit 之后」。**
    //
    // 实测：把退出臂里的 `h.stop()` 拿掉、在文件别处留一处，本条**照样绿** ——
    // 而它自陈要挡的正是「出口没接上 ⇒ 游魂进程」，头注还写着「删掉整段钩子，
    // 815 条测试全绿」。`rfind` 取的是**任意一处**，不是**这一处**。
    // ⇒ 改成把退出臂的**体**切出来，`.stop()` 必须在**体内**。
    let arm_at = prod.find("RunEvent::Exit").expect("上面已断言存在");
    let body = {
        // 从锚点往后找第一个左花括号，再按配平切到它的收尾。
        let bytes = prod.as_bytes();
        let open = (arm_at..bytes.len())
            .find(|&i| bytes[i] == b'{')
            .expect("`RunEvent::Exit` 之后找不到块起点 —— 形状变了，先修锚点");
        let mut depth = 0i32;
        let mut end = bytes.len();
        for i in open..bytes.len() {
            if bytes[i] == b'{' {
                depth += 1;
            } else if bytes[i] == b'}' {
                depth -= 1;
                if depth == 0 {
                    end = i + 1;
                    break;
                }
            }
        }
        &prod[open..end]
    };
    assert!(
        body.len() > 40 && body.len() < 4000,
        "切出来的退出臂只有 {} 字节 —— 配平切错了，本条会零命中地绿",
        body.len()
    );
    // ⚠ 锚点必须**当场核唯一性**（`every_position_comparison_over_source_pins_and_bounds_its_anchors`
    // 的第三条纪律）：本文件自己就是那条判据头注里的活样本 ——
    // 旧版用的是**反向查找**（那个原语名字里带 r 的），命中「任意一处」，对不对全靠排序运气。
    // ⚠ 这行不许把那个原语连着左括号写出来 —— 检测器扫的就是那个字面量，
    // 写在散文里也会被算成「本判据用了它」（本条第一版就是这么被误判的）。
    // `find_pinned` = 恰好一处 + 两侧有边界，位置比较才站得住。
    let stop_at = guard_core::find_pinned(body, ".stop()").unwrap_or_else(|e| {
        panic!(
            "退出臂里没有 `.stop()`。\n\
                 ★ 「文件里某处有一个 `.stop()`」不算 —— 本条要的是**这一处**。\n\
                 没有它，开关拨到「monitor 退出时结束 daemon」也**不会有任何反应**。\n\
                 ⚠ 08-08 实测：把这一句拿掉、在别处留一个 `.stop()`，旧版本条照样绿。\n\
                 （锚点诊断：{e}）"
        )
    });
    // ── P2s 翻面新增的两条 ──────────────────────────────────────────
    let policy_at = guard_core::find_pinned(body, "kill_on_exit(").unwrap_or_else(|e| {
        panic!(
            "退出臂里没有读 `kill_on_exit(` —— 它在**无条件**收本机后端。\n\
                 那与 `C8`③「默认不 kill」直接冲突：用户什么都没设就被杀 daemon，\n\
                 而开关默认是关着的。（锚点诊断：{e}）"
        )
    });
    assert!(
        policy_at < stop_at,
        "退出臂里 `.stop()` 出现在读策略**之前** —— 那就不是「按策略决定」，\n\
             而是「先杀了再查开关」。（读策略那句写在后面也可能只是打日志用。）"
    );
    // ⚠ **原版只要求「两者之间有个 `if `」**〔D 阶段补审 08-11 逮到〕：
    // `let kill = kill_on_exit(…); if h.current_pid().is_some() { h.stop(); }`
    // **照样绿，而开关彻底失效**。三样东西都在，语义却不是那回事。
    // ⇒ 改成：从 `let <名> = …kill_on_exit(` 反推出绑定名，再要求 `if <名>` ——
    // 钉的是「**那个 if 判的就是策略值**」，而不是「有个 if」。
    // （从绑定名反推而不是写死 `if kill`：改变量名不该假红。）
    let bind = {
        let line = body[..policy_at]
            .lines()
            .last()
            .expect("策略那一行之前总有内容");
        let t = line.trim_start();
        let rest = t
            .strip_prefix("let ")
            .unwrap_or_else(|| panic!("读策略那一行不是 `let <名> = …` 的形状：{t:?}"));
        rest.chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect::<String>()
    };
    assert!(
        !bind.is_empty(),
        "从读策略那一行抠不出绑定名 —— 抽取面画错了，本条在空转"
    );
    let between = &body[policy_at..stop_at];
    assert!(
        between.contains(&format!("if {bind}")),
        "读了策略、也有 `.stop()`，但中间那个条件**判的不是策略值** ——\n\
             绑定名是 `{bind}`，而这一段里没有 `if {bind}`：\n{between}\n\
             ★ 补审给的骗法：`let kill = kill_on_exit(…); if h.current_pid().is_some() {{ h.stop(); }}`\n\
             三样东西（读策略 / if / stop）都在，开关却彻底失效。"
    );
}

// ── 真进程（`#[ignore]`，由 tests/e2e/local-backend-supervise.sh 驱动）──────────

/// ★★ **起真 daemon 的 e2e 必须 fail-closed 地要一个私有 tmux 目录**
/// 〔audit-0805 08-08，Phase G 第 51 件〕。
///
/// 本模块头注逐字写着「**绝不让被监护的 daemon 碰用户真实的 tmux server**」。
/// 那道保护今天全靠下面那句 `.expect("要 CCM_E2E_TMUX_TMPDIR")` ——
/// 裸跑 `cargo test -- --ignored` 会 panic 而不是去连真 server，形态是对的。
///
/// **但没人钉它**：08-08 实测把那句换成 `.unwrap_or_default()`，
/// **全仓 982 条判据一条不红**；此后任何一次 `--ignored` 都会把真 daemon
/// 接到用户的 tmux server 上。`#[ignore]` 测试平时不跑 ⇒ 它坏了也没人知道，
/// 正是「新分支平时没人走」那一族（本仓已栽过六次）。
///
/// 人群**从源码派生**：本文件里 `#[ignore]` 且体内出现 `CCM_E2E_DAEMON`
/// （= 真的要一个 daemon 二进制）的测试。用假二进制的那条不在其中。
///
/// # ⚠⚠ 射程订正〔`K-R7-D2`，08-31〕：**人群画在两个属性上，都够不着最危险的那一形**
///
/// 本条的人群有两个条件，**两个都是「怎么标记的」**：`#[ignore]` · 提到 `CCM_E2E_DAEMON`。
/// 而本文件里 `the_local_daemon_really_registers_an_inbound_client` 是**普通 `#[test]`**、
/// 用的是**内嵌**那份二进制（不经 `CCM_E2E_DAEMON`）⇒ **两个条件各差一个**，
/// 于是它起着真 daemon 而本条一声不吭。`local_daemon.rs` 那条姊妹判据同理够不着。
/// ⇒ 正题已经搬到 `local_daemon.rs` 的
/// `every_test_that_starts_the_real_daemon_demands_a_private_tmux`：
/// 人群按「**那个二进制哪来的**」派生（内嵌目录 / `CCM_E2E_DAEMON` / `E2eSandbox::demand`），
/// **两个文件一起扫**，不看任何属性。
/// **本条留着**（它守的是一格更窄但仍然真的性质），但别把它读成「起真 daemon 有人守了」。
#[test]
fn every_real_daemon_e2e_demands_a_private_tmux_dir() {
    const REAL: &str = "CCM_E2E_DAEMON";
    // ⚠⚠ 〔`P0e` 08-12〕这个名字换过一次，**换的是机制不是名字**：
    //   原来是 `CCM_E2E_TMUX_TMPDIR`（把私有目录传给 daemon）—— 而 `$TMUX` 一有值
    //   就会压过它，那正是 08-11 打没用户 9 个真实会话的机制，`C7i` 因此逐字禁止
    //   「靠 `TMUX_TMPDIR` 做隔离」。
    //   现在传的是**带 shim 的 PATH**：daemon shell out 的 tmux 会被强插 `-L`，
    //   **显式选择器压得过 `$TMUX`**。本条钉的性质一个字没变：
    //   **起真 daemon 的 e2e 必须 fail-closed 地要一个私有 tmux 隔离**。
    const PRIVATE_TMUX: &str = "CCM_E2E_TMUX_SHIM_BIN";
    // 取 shim 的**唯一入口**〔`K-R7` 09-01〕：住 `local_daemon::tests::demand_tmux_shim`。
    const GATE: &str = "demand_tmux_shim(";
    // 🔴 〔搬树 2026-09-18 · `设计/16 §5.4b` 纪律 3〕**语料跟着测试搬。**
    //
    // 本条数的是「**本文件里**带 `#[ignore]` 的真 daemon e2e」。剖分把本模块的测试段
    // 整个搬来了 `tests/bridge/backend/control/local_backend_tests.rs`（就是本文件），
    // 而 `src/bridge/src/backend/control/local_backend.rs` 今天**一个 `#[test]` 都没有**
    // ⇒ 老语料一条都抓不到，那条「抽取器自检」按设计响了。
    // ⚠ 下面那句「本条的文档注释里就写着 `#[ignore]` 与 `CCM_E2E_DAEMON`」正是
    //   **因为语料是本文件**才成立的纪律 —— 两件事必须同改，别只改一半。
    let src = include_str!("local_backend_tests.rs");
    // ⚠ **不在语料串上做裸 `split`**：`needle_anchor_registry` 的递减棘轮把它
    //   判为「匹配单位比事实小」的一族，且**不许调上限**（本条第一版就栽在这）。
    //   改成按行扫、遇到下一处 `#[test]` 收尾 —— 边界是「行」，比子串确定。
    let mut chunks: Vec<String> = Vec::new();
    let mut cur: Vec<&str> = Vec::new();
    for line in src.lines() {
        if line.trim() == concat!("#[te", "st]") {
            if !cur.is_empty() {
                chunks.push(cur.join("\n"));
                cur.clear();
            }
            continue;
        }
        cur.push(line);
    }
    chunks.push(cur.join("\n"));
    // ⚠ 认属性要**整行相等**，不能 `contains` —— 本条的**文档注释里**就写着
    //   `#[ignore]` 与 `CCM_E2E_DAEMON`，第一版因此把自己也算进了人群，
    //   然后拿自己的 `const` 行去判 fail-closed，当场自红。
    //   （F24 那一族：匹配单位比事实大；这次事实是「一条属性」，而我匹配了「提到过」。）
    let is_ignored = |c: &String| c.lines().any(|l| l.trim() == concat!("#[ig", "nore]"));
    let real_e2e: Vec<&String> = chunks
        .iter()
        .filter(|c| is_ignored(c) && c.contains(REAL))
        .collect();
    // 抽取器自检：一条都没抓到 ⇒ 下面整条空转。
    assert!(
        !real_e2e.is_empty(),
        "本文件里找不到「`#[ignore]` 且要 {REAL}」的测试（08-08 实测 1 条）—— \
             抽取器坏了或那条 e2e 被删了，本条此刻无效"
    );
    // ⚠⚠ **剥注释这一步是 09-01（`C` 第六拍）补的**〔`D5` 阻塞 2；PM `§0s` 五 ①〕。
    //
    //   下面那两格原来判在 `c.lines()`（**没剥注释的原始行**）上，而下游
    //   —— `local_daemon.rs` 那条正题的 ㈡（`shim_first_on_path` 取绑定名）——
    //   **当时**读的是它自己那份 `code_only`（**剥掉 `//` 打头的行**）。**两条判在不同的文本上。**
    //   `D5-M4` 实证：在合法落点上方加**一行纯注释**提到那个口
    //   ⇒ **本条当场红（新红 1）、主守卫一声不吭**，而本条的报文却说
    //   「因为下游要从这一行上取绑定名」——**那句归因在唯一分岔的场合恰好是假的**。
    //
    //   🔴 **这个坑本仓在这条守卫上已经栽过一次**：主守卫 `code_only` 上方那段注释逐字
    //   写着「判人群要看**代码**，不能看文档注释 —— `local_backend.rs` 那条同族守卫在
    //   这上面**自红过一次**」，主守卫为此加了 `code_only`，
    //   **而 `C` 第五拍给本条新加的那一格没有跟着剥**
    //   ⇒ 那正是「**只修一半 / 同族当场复发**」，只换了个动词（从「量词」换成
    //   「判在剥没剥注释的哪一份文本上」）。
    //
    //   ⇒ 现在**先剥再判**，与主守卫**同一把尺子**。
    //   ⚠ 只让本条**更松**（少几行可看），不放水：被判的那一行本来就得是代码。
    //
    //   ⚠⚠ **09-01（`C` 第七拍）两处一起收口成共享原语**〔`D6` `B2`〕：
    //   本条与主守卫原来各写了一份**私有副本**（只剥 `//` 打头的整行），现在都直接调
    //   `guard_core::strip_comment_lines`。仓规逐字「剥注释只许有一个权威实现」
    //   （`structural_scan.rs:425`）；**先量再选**的读数（`*` 打头的解引用行会被多剥掉，
    //   而今天判决不同 0 处）写在 `local_daemon.rs` 那条正题守卫的同名那一段里，不在这里抄一遍。
    //   🔴 **它剥不掉块注释里那些自己不以 `//` / `*` / `/*` 打头的内层行** ——
    //   `/*` 与 `*/` 各独占一行、内层行以 `let ` 打头时，那几行原样留着。
    //   ⚠ **别把上一句读成全称**〔`C8` 09-01 收窄，`D7` `B1`〕：内层行写成 ` * let …`
    //   （块注释的 `*` 对齐续行写法）今天**剥得掉** ⇒ 两条守卫一起红（实打，新红 2）。
    //   多行式 / 一行式**两个方向**今天都实打过：**多行式是一次静默的假绿**（`D6` `M-D6-3`，全量门禁 `GATE: OK`）、
    //   **一行式在旧剥法下是一次假阳**（`D6-M1`，换共享原语之后不再假阳）。
    //   逐条登记在 `local_daemon.rs` 头注「诚实边界 9」，并归跟进件 `K-R8`。
    for c in real_e2e {
        let code = guard_core::strip_comment_lines(c);
        let name = code
            .lines()
            .find_map(|l| l.trim().strip_prefix("fn "))
            .and_then(|r| r.split_once('('))
            .map(|(n, _)| n)
            .unwrap_or("<未知>");
        // ⚠⚠ **这一格 09-01 换过一次判法**〔`K-R7` `§0q` 裁一 · 出路乙，`D4` 阻塞 1〕。
        //   原来是两句：先找「含 `CCM_E2E_TMUX_SHIM_BIN` 的那一行」，再判那行含 `.expect(`。
        //   `D4` 那一刀（`var(SHIM).unwrap_or_else(|_| var(<真 PATH>).expect(..))`）
        //   **同一行上三样东西都还在** ⇒ 旧判据说合规，而 fail-closed 没了。
        //   🔴 **本条是那一族的第 4 处，而 `§0q` 只点了 `local_daemon.rs` 的 3 处** ——
        //     09-01 实测：只改本条人群里那条 e2e（`e2e_the_supervisor_restarts_…`）的取值，
        //     **全量门禁新红 0**，本条一声不吭。⇒ 一起换。
        //   现在判的是「**走没走取 shim 的那个唯一入口**」；
        //   「**那个口关不关得上**」由 `local_daemon.rs` 的
        //   `the_one_shim_gate_really_fails_closed` 在**默认门禁里真跑一遍**（不是文本钉）。
        //   ⚠ **判在剥过注释的 `code` 上**（见上面那段）—— 09-01 之前判的是原始行。
        let line = code
            .lines()
            .find(|l| l.contains(GATE))
            .unwrap_or_else(|| {
                panic!(
                    "`{name}` 会起一个**真** daemon，却没走取 shim 的那个唯一入口 \
                         （`{GATE}`）—— 它会连上用户真实的 tmux server。\n\
                         本模块头注写的是「绝不」。⚠ 那个变量叫 `{PRIVATE_TMUX}`，\
                         而**取它只许从那一个口取**（`local_daemon::tests::demand_tmux_shim`）：\
                         口里那句 fail-closed 是被一条真跑的测试钉住的，\
                         自己现取就退回到「谁也没在守」。\n\
                         ⚠ 判的是**剥掉整行注释之后**的本体 —— 剥法是 \
                         `guard_core::strip_comment_lines`：`trim_start()` 之后以 `//` 或 `*` \
                         或 `/*` 打头的**整行**换成空行。写在**整行注释**里提一句不算数，\
                         也不再让本条假红（`D5-M4`）。\n\
                         ★ **09-04（`K-R9`）：块注释那个洞已经关上，下面这段原文是病历不是现状。**\
                         原文逐字：「多行块注释的内层行**只要自己不以 `//` / `*` / `/*` 打头就剥不掉** \
                         —— `/*` 与 `*/` 各独占一行、内层行以 `let ` 打头时……照旧算数，\
                         而且**一声不吭**（**静默的假绿**），归跟进件 `K-R8`。」\n\
                         今天 `guard_core::strip_comment_lines` 先过 \
                         `guard-core/src/lib.rs::try_strip_block_comments`：带**开合状态与深度**的\
                         真词法（不再是按行前缀），块注释内容抹成**等长空格** ⇒ 行数与字节数都不变，\
                         而内层行三种写法**一律**剥掉。\n\
                         ⚠ 边界**换了地方、没有消失**：那份剥法词法与某文件对不上时\
                         （`depth` 不收口 / 停在字符串里）**一个字都不剥**（兜底，宁可留洞不许造假红），\
                         那份文件上这个洞会重开 ⇒ 由 `local_daemon.rs` 的 \
                         `no_monitor_file_falls_back_to_leaving_block_comments_in` 看着\
                         （现打：105 份 `.rs`，走兜底 0 份）。"
                )
            });
        // 反空真：光找到那一行不够 —— 它得真是**赋值**给某个绑定的。
        assert!(
            line.trim_start().starts_with("let "),
            "`{name}` 里出现 `{GATE}` 的那一行（**剥掉「`//` / `*` / `/*` 打头的整行」之后**）\
                 不是一条绑定：\n  {}\n\
                 ⇒ 本条只认「`let <名字> = …` 一行到底」这一种写法，因为下游\
                 （`local_daemon.rs` 那条正题的 ㈡）要从**同一把尺子剥出来的这一行**上取绑定名。\n\
                 ⚠ **三条已知的边界（分母 = 下面逐条列出的 ①②③），\
                 写在这里免得下一个人以为自己写错了**：\n\
                 ① **假阳**：把这条调用按 rustfmt 在 `=` 后断行（`let shim =` 换行再写调用）\
                 ⇒ 含口的那一行不再以 `let ` 打头，本条与下游 ㈡ **一起红**，\
                 而那是一条语义逐字不变的合法写法（`D5-M5` 实证）。**今天要的是单行写法。**\n\
                 ② 本条 09-01 起判在**剥掉「`//` / `*` / `/*` 打头的整行」之后**的文本上\
                 （剥法 `guard_core::strip_comment_lines`，与下游 ㈡ 同一把）——\
                 `D5-M4`：一行纯注释曾让本条假红而下游一声不吭，**两条判在不同文本上的日子结束了**。\n\
                 ③ ★ **09-04（`K-R9`）关上了 —— 原文「今天开着的」那句已经不是现状**。\
                 原文逐字：「**块注释的内层行，只要自己不以 `//` / `*` / `/*` 打头就剥不掉**\
                 ……本条与下游 ㈡ 都会把它当真代码读」，并按内层行的写法分了三形：\
                 ㋐ 以 `let ` 打头 ⇒ **一次静默的假绿**（`D6` `M-D6-3`：一条起真 daemon 的 e2e，\
                 `PATH` 里一点 shim 都没有，全量门禁 `GATE: OK`、新红 0）；\
                 ㋑ 既不以 `let ` 也不以那三种前缀打头 ⇒ 落到 `MISS_NO_BINDING`；\
                 ㋒ 以 `*` 打头 ⇒ 那时就剥得掉、两条守卫一起红（`C8` 09-01 实打）。\n\
                 今天剥法换成了**带开合状态与深度的真词法**\
                 （`guard-core/src/lib.rs::try_strip_block_comments`，等长抹空格 ⇒ 行位不变）\
                 ⇒ **㋐㋑㋒ 三形归一**：块注释内容一律剥掉，都落回上面那句 `panic!`\
                 （「没走那个唯一入口」）与下游 `MISS_LOCK2`。\
                 ㋐ 那个「静默假绿」`K-R9` `D3` 在本工作树上重打过两个读数：\
                 旧剥法 monitor `1275 passed / 0 failed`，新剥法当场红。\n\
                 ⚠ **今天真正的边界是那条静默兜底**：词法与某份文件对不上时一个字都不剥，\
                 那份文件上这个洞会重开 ⇒ 由 `local_daemon.rs` 的 \
                 `no_monitor_file_falls_back_to_leaving_block_comments_in` 看着\
                 （现打：105 份 `.rs`，走兜底 0 份）。\
                 **「块注释这一族还能怎么走」这个分母仍然给不出** —— 变的是\
                 「已知走法逐个实打过」，不是「证明了没有别的走法」。\n\
                 ⚠ **人群分母**：本条今天 **1** 条、下游 **6** 条（两个文件切出 **58** 块；\
                 量于 09-01，量具 `scratchpad/kr7-c7-starline.py`，被测对象 = 工作树尖 `aee8b9f`）。",
            line.trim()
        );
    }
}

/// ★ 起真 daemon → 杀它 → 看它自己回来。
#[test]
#[ignore]
fn e2e_the_supervisor_restarts_a_real_daemon_after_it_is_killed() {
    let bin = std::env::var("CCM_E2E_DAEMON").expect("要 CCM_E2E_DAEMON");
    let shim = crate::local_daemon::tests::demand_tmux_shim("本条起真 daemon");
    let claude = std::env::var("CCM_E2E_CLAUDE_DIR").expect("要 CCM_E2E_CLAUDE_DIR");
    let events: Arc<Mutex<Vec<SuperviseEvent>>> = Arc::new(Mutex::new(Vec::new()));
    let ev = events.clone();
    let h = supervise(
        PathBuf::from(&bin),
        vec!["--tail-only".into()],
        vec![
            // `C7i`：给 daemon 一条**前面挂着 shim** 的 PATH —— 它 shell out 的 tmux
            // 会被强插 `-L`。比传 `TMUX_TMPDIR` 硬：`$TMUX` 压不过显式选择器。
            (
                "PATH".into(),
                format!("{shim}:{}", std::env::var("PATH").unwrap_or_default()),
            ),
            ("CLAUDE_CONFIG_DIR".into(), claude),
        ],
        CrashLimits::default(),
        Arc::new(|| 0),
        Arc::new(move |e| ev.lock().expect("ev").push(e)),
        crate::spawn_managed::local_backend_supervised(),
    );
    let first = spin(|| h.current_pid()).expect("10s 内没起来");
    println!("E2E-OK 真 daemon 起来了 pid={first}");
    kill_for_test(first);
    let second = spin(|| h.current_pid().filter(|p| *p != first)).expect("10s 内没重起");
    assert_ne!(first, second, "pid 没变 ⇒ 没有真的重起");
    println!(
        "E2E-OK 被杀之后自己回来了 pid={second}（attempts={}）",
        h.attempts()
    );
    h.stop();
    // stop() 必须真的把它收掉 —— 不收就是游魂进程。
    assert!(
        spin(|| h.current_pid().is_none().then_some(())).is_some(),
        "stop() 之后当前 pid 还在"
    );
    println!("E2E-OK stop() 把当前子进程收掉了");
    let got = events.lock().expect("ev").clone();
    assert!(
        got.iter()
            .filter(|e| matches!(e, SuperviseEvent::Started { .. }))
            .count()
            >= 2,
        "Started 事件少于 2 次 ⇒ 事件面没有如实报告重起：{got:?}"
    );
    println!("E2E-OK 事件面报告了 {} 条", got.len());
}

/// ★ 一个**必崩**的二进制要在上限内被判死，而不是无限自旋。
#[test]
#[ignore]
fn e2e_a_binary_that_always_dies_is_given_up_on_within_the_cap() {
    let dir = std::env::var("CCM_E2E_WORK").expect("要 CCM_E2E_WORK");
    let bin = PathBuf::from(&dir).join("always-dies.sh");
    std::fs::write(&bin, "#!/bin/sh\nexit 7\n").expect("写不出脚本");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    }
    let events: Arc<Mutex<Vec<SuperviseEvent>>> = Arc::new(Mutex::new(Vec::new()));
    let ev = events.clone();
    let h = supervise(
        bin,
        vec![],
        vec![],
        CrashLimits {
            max_crashes: 3,
            window_ms: 60_000,
        },
        Arc::new(|| 1),
        Arc::new(move |e| ev.lock().expect("ev").push(e)),
        crate::spawn_managed::local_backend_supervised(),
    );
    let gave_up = spin(|| {
        events.lock().expect("ev").iter().find_map(|e| match e {
            SuperviseEvent::GaveUp { reason } => Some(reason.clone()),
            _ => None,
        })
    })
    .expect("10s 内没放弃 —— 它在无限自旋？");
    assert!(gave_up.contains("崩了 3 次"), "放弃理由不对：{gave_up}");
    let n = h.attempts();
    assert_eq!(n, 3, "应当正好起 3 次就放弃，实得 {n}");
    println!("E2E-OK 必崩二进制在 3 次内被判死，没有自旋");
}

/// ★ 真文件系统上「没有 sidecar」⇒ 诚实降级（这条不起任何进程）。
#[test]
#[ignore]
fn e2e_a_missing_sidecar_degrades_honestly_against_the_real_filesystem() {
    let dir = std::env::var("CCM_E2E_WORK").expect("要 CCM_E2E_WORK");
    let r = resolve_with(
        Path::new(&dir),
        "x86_64-unknown-linux-gnu",
        "",
        &|p: &Path| p.exists(),
    );
    let Resolved::Missing { looked_at, .. } = r else {
        panic!("空目录里居然找到了 sidecar");
    };
    assert_eq!(looked_at.len(), 2);
    println!("E2E-OK 缺 sidecar 时诚实降级，且列出了 2 条找过的路径");
}

// ── 测试侧助手。**只在测试里**，生产段没有任何轮询 ─────────────────────
fn spin<T>(mut f: impl FnMut() -> Option<T>) -> Option<T> {
    for _ in 0..200 {
        if let Some(v) = f() {
            return Some(v);
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    None
}

/// 测试侧杀进程：起一个 `kill`。**生产段不做这件事**（`stop()` 走 `Child::kill`）。
fn kill_for_test(pid: u32) {
    #[cfg(unix)]
    let _ = std::process::Command::new("kill")
        .args(["-TERM", &pid.to_string()])
        .status();
    #[cfg(windows)]
    let _ = std::process::Command::new("taskkill")
        .args(["/PID", &pid.to_string(), "/T", "/F"])
        .status();
}

// ═══════════════════════════════════════════════════════════════════════
// F16：三处失败模式（都是 F05a 我自己写的代码，F12 的 `/full-audit` 逐行核出来的）
// ═══════════════════════════════════════════════════════════════════════

/// [`resolve_or_extract`] 的**生产段函数体** —— 那份**共用的**「找那个二进制」。
///
/// 〔`K-R43` 改：原来切的是 [`start_or_extract`]（当时那三问就住在它体内）。
///  今天三问住共用那份，`start_or_extract` 体内只剩「调它 + 监护」⇒ 靶跟着搬。
///  **切法一个字没动**，动的只是切哪个函数。
///  搬完之后这把尺子买到的比原来多一格：`local_daemon.rs` 那条路今天也走这一份，
///  ⇒ 同一条断言同时盖住**两个**生产落点（「都走了它」那半由
///  `local_daemon::the_two_resolution_paths_still_agree_on_the_order` 钉）。〕
///
/// ⚠ 切法照下面那个 `wait_section()`（`find` 一个锚点再往后取），**但多一个右界**：
/// `wait_section` 取到文件尾，那对「顺序」类断言没关系，对「体内有没有某个名字」
/// 却会把**后面所有函数**都算进来 ⇒ 恒真。右界取**列 0 的那个 `}`**
/// （函数体内的右花括号都是缩进的）。取歪了由调用方那条锚点自检当场逮住
/// （体里必须有 `extract_embedded_to(`）。
///
/// 🔴 **它必须住在这里，不许挪到 `never()` 旁边** —— 那一块是
/// `e2e_a_missing_sidecar_degrades_honestly_against_the_real_filesystem` 与它之前那几个
/// e2e 块的地界，而 `local_daemon_tests.rs::every_test_that_starts_the_real_daemon_demands_a_private_tmux`
/// 按 `#[te st]` 行切块、并把**含 `include_st r!(` 的块当守卫整块跳过**。
/// 一挪过去，`the_local_daemon_really_registers_an_inbound_client` 会**静默掉出那条判据的人群**
/// （实测：那条判据的地板当场红，报文逐字「它起真 daemon 的来历不见了」）。
fn shared_resolution_body() -> String {
    let prod = guard_core::production_code(include_str!(
        "../../../../src/bridge/src/backend/control/local_backend.rs"
    ));
    let at = prod
        .find("pub fn resolve_or_extract(")
        .expect("找不到 `resolve_or_extract` —— 改名了就把引它的判据一起改");
    let rest = &prod[at..];
    let end = rest.find("\n}\n").map(|i| i + 2).unwrap_or(rest.len());
    // ⚠ **整行 `//` 注释剥掉再交出去**：调用方用 `find_pinned`（恰好一处），
    // 而函数体里那几段注释**逐字提到了它要找的那几个名字** ⇒ 不剥就恒判「多于一处」。
    // 〔同族病历：`needle_anchor_registry` 那条棘轮逐字记着「判据把自己留下的病历当成了病」。〕
    rest[..end]
        .lines()
        .filter(|l| !l.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// 监护线程「等它死 + 收尸」那一段的生产源码。
fn wait_section() -> String {
    let prod = guard_core::production_code(include_str!(
        "../../../../src/bridge/src/backend/control/local_backend.rs"
    ));
    let at = prod
        .find("pub fn supervise(")
        .expect("找不到 `supervise` —— 改名了就把下面三条一起改");
    prod[at..].to_string()
}

/// ★ ①「等它死」不许把 stdout 攒起来。
///
/// 原来是 `read_to_end(&mut Vec::new())` —— 被监护的 daemon **持续产帧**
/// （那正是头注用来论证「它不会关 stdout」的理由）⇒ 那个 `Vec` 单调增长且没有消费者。
/// ⚠ 今天不咬人只因为 `resolve_beside_this_exe` 恒 `Missing` —— **离生效只差一个配置项**。
#[test]
fn waiting_for_death_never_accumulates_the_child_stdout() {
    let sec = wait_section();
    // 运行时拼，免得命中本文件自己的说明。
    let bad = format!("read_to{}", "_end");
    assert!(
        !sec.contains(bad.as_str()),
        "`supervise` 里又出现了把 stdout 读进内存的写法 —— 被监护对象是**持续产帧**的，\n\
             那个缓冲区会随本机使用时长单调增长、且没有任何消费者。\n\
             ⇒ 用 `io::copy` 到 `io::sink()`：**EOF 语义完全不变**，但一个字节都不留。"
    );
    assert!(
        sec.contains("std::io::copy(") && sec.contains("std::io::sink()"),
        "找不到 `io::copy(… , io::sink())` —— 那是本条要求的那个形态"
    );
}

/// ★★ ②`wait()` 不许在持锁的情况下调。
///
/// 「子进程关掉 stdout 但继续活着」是本模块**已登记的诚实边界**；那时 `wait()` 会久等，
/// 而 `stop()` 第一件事就是 `self.child.lock()` 且它跑在**主线程**（`RunEvent::Exit`）
/// ⇒ 持锁 `wait()` 会把「误判它死了」升级成**应用退不出去**。
#[test]
fn reaping_never_holds_the_child_lock_while_it_waits() {
    let sec = wait_section();
    // 先把子进程从锁里 `take()` 出来，再在锁外 `wait()`。
    // ⚠ **锚点从「字面形状」改成「性质」**〔D 阶段补审 08-11，B4〕。
    //
    // 原来钉的是逐字的 `child.lock().ok().and_then(|mut g| g.take())`。
    // B4 要在 take 之前按 `ConsumerExit` 补一刀（早退时子进程还活着），
    // 那一句必然变形 ⇒ 判据当场红。**但它守的性质一个字没变**：
    // 「持锁的那个块里不许有 `wait()`」。⇒ 改成钉那句话本身。
    //
    // ★ 这是「判据挡路 ≠ 把判据放宽」的又一例：形状换了，性质原样，
    // 而且新写法**更硬** —— 原版认一串特定链式调用，现版认「块内不许 wait」。
    let take_at = sec.find("let mut reaped = {").expect(
        "收尸段不再是「持锁块里 take 出来、块外 wait」那个形状 ——\n\
             那意味着 `wait()` 可能又回到了锁里面。后果不是「误判它死了」，\n\
             是 `stop()` 在主线程永久阻塞、窗口关了进程退不出去。",
    );
    let held_end = {
        let blk = &sec[take_at..];
        let open = blk.find('{').expect("上面刚 pin 过");
        let (mut depth, mut end) = (0i32, open);
        for (i, c) in blk.char_indices().skip(open) {
            match c {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        end = i;
                        break;
                    }
                }
                _ => {}
            }
        }
        let held = &blk[open..=end];
        assert!(
            held.len() > 30 && held.len() < 2000,
            "切出来的持锁块 {} 字节 —— 配平切错了，本条在空转",
            held.len()
        );
        assert!(
            !held.contains("wait("),
            "持锁块里出现了 `wait(`：\n{held}\n\
                 ⇒ `stop()` 第一件事就是拿同一把锁，而它跑在**主线程**（`RunEvent::Exit`）\n\
                 ⇒ 持锁 `wait()` 会把「误判它死了」升级成**应用退不出去，只能 kill -9**。"
        );
        // ★★ **B4 的正题**：早退时必须补一刀 —— 变异实测「摘掉它全套件照样绿」，
        // 说明我修的那件事本来没有判据。补在这里而不是新开一条：
        // 它与「wait 不许在锁里」是**同一段代码的两条性质**，分开写会各自漂。
        assert!(
            held.contains("ConsumerExit::Early") && held.contains("kill()"),
            "持锁块里没有「早退就补一刀」那一支：\n{held}\n\
                 ⇒ 消费者因**读错误或 panic** 返回时子进程还活着，而它已被从共享锁里摘走\n\
                 ⇒ 并发的 `stop()` 看到 `None`，**一个字节的 kill 都不发**，却仍返回「已停」。\n\
                 ⚠ 不许改成无条件 kill：那会把正常退出码换成信号死，而 `decide()` 按退出码\n\
                 分崩溃/正常（风险 5d）。也不许去探子进程 —— 本模块禁那类构件。"
        );
        take_at + end
    };
    // ⚠ **不能只写 `sec.find(".wait()")`** —— 那会命中**关窗块**里那个
    //   `c.kill(); c.wait();`（它在 `take()` 之前，且它是对的：那处本来就持锁、
    //   而子进程刚被 kill、不会久等）。第一版就是这么写的，**判据自己当场红了**。
    //   ★ 又一次「锚点指到了第一处同名的东西」而不是那一处。
    //   ⇒ 钉的是「**被 wait 的那个东西是从锁里 take 出来的**」：`reaped` 之后紧跟 `.wait()`。
    // ⚠ 窗口起点从持锁块**结束处**算，不是从 `take_at` 算 ——
    // B4 让那个块变长了，固定 260 字符的窗口当场不够（判据自己红了一次）。
    let after_take = &sec[held_end..(held_end + 260).min(sec.len())];
    assert!(
        after_take.contains("reaped") && after_take.contains(".wait()"),
        "`take()` 之后没紧跟着对取出来的那个 `Child` 调 `wait()` —— 实得这一段：{after_take:?}"
    );
    // ★ 反向：`wait()` 那一行不许再出现在 `lock()` 的链式调用里。
    for l in sec.lines() {
        let t = l.trim_start();
        if t.starts_with("//") {
            continue;
        }
        assert!(
            !(l.contains(".lock()") && l.contains(".wait()")),
            "这一行同时有 `.lock()` 与 `.wait()` ⇒ 又变成持锁等了：{l}"
        );
    }
}

/// ★★ ③`spawn` 与「登记进锁」之间那个窗口必须关上。
///
/// 原来 `stopping` 只在循环顶部与 EOF 之后检查 ⇒ 刚过顶部检查就 `spawn` 时，
/// 一个并发的 `stop()` 会看到锁里还是 `None`、**一个字节的 kill 都没发**，
/// 而线程接着把子进程存进锁并进 `io::copy` 永久阻塞
/// ⇒ **monitor 退了、daemon 还在跑且没人能 kill 它** —— `stop()` 头注说的「游魂进程」。
#[test]
fn the_window_between_spawn_and_registration_is_closed() {
    let sec = wait_section();
    let reg_at = sec
        .find("*g = Some(spawned);")
        .expect("找不到「把子进程存进锁」那一行");
    let after = &sec[reg_at..];
    // 存完之后、发 `Started` 之前，必须再读一次 `stopping`。
    let started_at = after
        .find("SuperviseEvent::Started")
        .expect("找不到 `Started` 事件");
    let window = &after[..started_at];
    assert!(
        window.contains("stopping.load("),
        "登记子进程之后没有复查 `stopping` —— 那个窗口还开着：\n\
             `stop()` 落在里面就是一个**没人能 kill 的游魂 daemon**。\n\
             实得这一段：{window:?}"
    );
    assert!(
        window.contains("kill()"),
        "复查到 `stopping` 之后没有就地 kill —— 只 return 的话子进程留下来了"
    );
}

/// ★ **反向断言（「让它发生」那一半）**：改成 `io::copy` 之后，
/// 「子进程写了远超任何缓冲区的量再退出」这条路**仍然**能被检测到死亡。
///
/// ⚠ 只断言「没攒内存」是不够的 —— 那与「机制根本没跑」区分不开（F14 的 e2e 差点空绿）。
/// 本条让它**真的发生一次**：8 MiB stdout + 正常退出 ⇒ `Exited` 必须来。
#[test]
fn a_child_that_floods_stdout_and_exits_is_still_detected_as_dead() {
    let (tx, rx) = std::sync::mpsc::channel::<SuperviseEvent>();
    let h = supervise(
        PathBuf::from("sh"),
        vec![
            "-c".into(),
            // 8 MiB 到 stdout，然后正常退出。
            "dd if=/dev/zero bs=1024 count=8192 2>/dev/null; exit 3".into(),
        ],
        vec![],
        CrashLimits {
            max_crashes: 1,
            window_ms: 60_000,
        },
        Arc::new(|| 0),
        Arc::new(move |e| {
            let _ = tx.send(e);
        }),
        crate::spawn_managed::local_backend_supervised(),
    );
    let mut saw_exit = None;
    for _ in 0..6 {
        match rx.recv_timeout(std::time::Duration::from_secs(20)) {
            // ⚠ `K-P3b`：新字段**逐个点名**，不用 `..` 把它们静默掉 ——
            //   那正是「加了一维而没人回来看一眼」的入口。
            //   这一跑的形状：缺省那支（没有消费者）⇒ `witness` 必是 `NoConsumer`；
            //   `sh -c '…; exit 3'` 正常退出 ⇒ `status` 有值且退出码是 3。
            Ok(SuperviseEvent::Exited {
                code,
                attempt: _,
                status,
                witness,
            }) => {
                assert_eq!(
                    witness,
                    StreamWitness::NoConsumer,
                    "这一跑没接消费者，`witness` 却不是 `NoConsumer` —— \
                         那意味着有人替一条**没有观测者**的路填了两维证据"
                );
                assert_eq!(
                    status.and_then(|s| s.code()),
                    code,
                    "原样交上来的 `status` 与 `code` 对不上 —— 那两个字段说的该是同一件事"
                );
                saw_exit = Some(code);
                break;
            }
            Ok(_) => continue,
            Err(e) => panic!("20s 内没等到 `Exited` —— EOF 语义被改坏了：{e}"),
        }
    }
    h.stop();
    assert_eq!(
        saw_exit,
        Some(Some(3)),
        "写了 8 MiB 之后退出的子进程没被正确收尸（或退出码丢了）"
    );
}

/// ★★ **反向断言**：子进程**关掉 stdout 但继续活着**时，`stop()` 必须**及时返回**。
///
/// 这条是 ② 那个死锁链的行为面：持锁 `wait()` 会让这里永久卡住。
/// ⚠ 测试里用 `recv_timeout`/带上限的等待是允许的 —— C12 的「零定时器」管的是
/// **backend 生产代码**里不许有自己醒过来的构件，不是测试的等待上限。
#[test]
fn stop_returns_promptly_even_if_the_child_closed_stdout_but_lives_on() {
    let (tx, rx) = std::sync::mpsc::channel::<SuperviseEvent>();
    let h = supervise(
        PathBuf::from("sh"),
        // 关掉 stdout（制造 EOF）但继续活着 —— 那正是已登记的那个诚实边界。
        vec!["-c".into(), "exec 1>&-; sleep 30".into()],
        vec![],
        CrashLimits {
            max_crashes: 1,
            window_ms: 60_000,
        },
        Arc::new(|| 0),
        Arc::new(move |e| {
            let _ = tx.send(e);
        }),
        crate::spawn_managed::local_backend_supervised(),
    );
    // 等它真的起来（否则我们可能在 spawn 之前就 stop，测不到那条链）。
    match rx.recv_timeout(std::time::Duration::from_secs(20)) {
        Ok(SuperviseEvent::Started { .. }) => {}
        other => panic!("没等到 `Started`：{other:?}"),
    }
    // ★ `stop()` 在另一个线程上跑，主线程带上限地等它回来。
    let (done_tx, done_rx) = std::sync::mpsc::channel::<()>();
    let hh = std::sync::Arc::new(h);
    let h2 = hh.clone();
    std::thread::spawn(move || {
        h2.stop();
        let _ = done_tx.send(());
    });
    done_rx
        .recv_timeout(std::time::Duration::from_secs(10))
        .expect(
            "`stop()` 10s 没回来 —— 监护线程正持着 `child` 锁等一个还活着的子进程。\n\
                 生产上它跑在**主线程**（`RunEvent::Exit`）⇒ 窗口关了、进程退不出去，只能 kill -9。",
        );
}

/// ★★ `K-P3b KP3W2`：**消费者交上去的那两维是观测出来的，不是默认值。**
///
/// 两处观测点各钉一行：
/// - 「读端怎么结束的」：读错误那一支把**那句错原样**装进 `ReaderEnd::Broken`；
///   喂 `CleanEof` 会让「读坏了」在账上变成「崩了」（`B1` 那条错误诊断的全部内容：
///   `InvalidData` 与 EOF 走同一条路 ⇒ 记一次崩溃 ⇒ 三次之后整个进程周期不再起来）。
/// - 「它说过话没有」：读的是 `registered`，而那个值**只在**
///   `DaemonHello::from_hello_frame` 给出见证之后才变成 `Some` ——
///   写成常量就等于把 2026-07-09 的判别式换成一句猜测。
///
/// # ⚠ 射程：这是**源码判据**，如实登记
///
/// 「读端出错」那一支要一次**真的 IO 错误**才走得到（`read_capped_line_sync` 的 `Err`），
/// 在一根真管道上造不出来 ⇒ 这一格只证「那一行写在那儿」。
/// 行为那一半在 `local_daemon_tests.rs::three_fake_daemons_land_in_three_different_cells`：
/// 那里 ①② 两格走的是**真的**这个消费者（hello 之后 `exit 3` / 一个字节不说就 `exit 2`），
/// ③ 那一格喂的是注入的消费者 —— 各自的射程写在那条判据自己的头注里。
#[test]
fn the_consumer_reports_what_it_observed_not_a_default() {
    let prod = guard_core::production_code(include_str!(
        "../../../../src/bridge/src/backend/control/local_backend.rs"
    ));
    assert!(
        prod.len() > 10_000,
        "剥完只剩 {} 字节 —— 本条在空转",
        prod.len()
    );
    for l in [
        "reader_end = crate::daemon_policy::ReaderEnd::Broken(e.to_string());",
        "let handshake = if registered.is_some() {",
    ] {
        guard_core::pin_line(&prod, l).unwrap_or_else(|why| {
            panic!(
                "{why}\n\
                     ⇒ 消费者交上去的那一维不再是**观测**来的。\n\
                     ★ 那两维是宿主层判「崩了 / 被拒了 / 读坏了」的全部输入，\n\
                     填一个看起来合理的值 ⇒ 判出来的那一格是编的，而它**不会报错**。"
            )
        });
    }
}
