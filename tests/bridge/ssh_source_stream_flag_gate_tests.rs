use super::decide_stream_flags;

fn caps(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| s.to_string()).collect()
}

/// F66 DoD：能力门控矩阵——空集（旧 backend/未确认）恒全 false；
/// 声明 bg+tail-only 后 tail_only 恒开、with_bg 随 showBgSessions；
/// 部分声明只开对应位；未知 token 忽略。
/// ★ U-CC1：`KNOWN_CAPABILITY_TOKENS` 与 `decide_stream_flags` 必须是同一份事实。
///
/// 漂开的后果是**漂移记账说谎**：backend 声明了一个我们其实认识的 token，诊断面却把它
/// 报成「不认识」；或者反过来，真的新 token 被当成已知、一声不吭。
#[test]
fn known_capability_tokens_match_decide_stream_flags() {
    for t in super::KNOWN_CAPABILITY_TOKENS {
        let one = vec![t.to_string()];
        assert_ne!(
            super::decide_stream_flags(&one, true),
            (false, false, false),
            "`{t}` 在名单里，但 decide_stream_flags 根本不看它 —— 两份事实漂了"
        );
    }
    for junk in ["nope", "future-thing", ""] {
        assert_eq!(
            super::decide_stream_flags(&[junk.to_string()], true),
            (false, false, false),
            "`{junk}` 不在名单里却影响了 flag —— 名单漏登记了一个真能力"
        );
    }
    // 🔴 〔`设计/80 §8.7` 步 3〕地板从 2 抬到 3。步 2 那一路的交接逐字：后端已经在
    // hello 里声明 `rbind-token`，而本名单当时还是两个 ⇒ 每次握手都往 drift_ledger
    // 记一条 `UnknownBackendToken`。抬这个数是为了**下一次漏登记时本条会响**。
    assert!(
        super::KNOWN_CAPABILITY_TOKENS.len() >= 3,
        "名单只剩 {} 个 —— 维护坏了，本断言在空转",
        super::KNOWN_CAPABILITY_TOKENS.len()
    );
    // ★ 名单里必须**有** `rbind-token` 那一条：只靠上面那个循环的话，
    //   「把它从名单里摘掉」这个变异会让循环少跑一圈而**照样绿**（分母变小的方向瞎）。
    assert!(
        super::KNOWN_CAPABILITY_TOKENS.contains(&"rbind-token"),
        "`rbind-token` 不在 monitor 认识的名单里 —— 每次握手都会记一条假的漂移账：{:?}",
        super::KNOWN_CAPABILITY_TOKENS
    );
}

/// 🔴 ★ 〔`设计/80 §8.7` 步 3〕**跨进程双写点：monitor 发的每一条流模式 flag，
/// 后端都必须认得并剥离。**
///
/// 失效方向是本仓栽过的那一条（§26）：老后端把**不认识**的 `--flag` 当成一次性查询
/// ⇒ 处理完就退出 ⇒ 无 hello ⇒ monitor 重连 ⇒ **死循环**。
/// 而「声明了那条能力 ⟹ 会剥离对应 flag」是后端那侧的自证纪律 ——
/// 本条钉的是另一半：**monitor 真正拼进命令行的那几个字面量，一个不落地在
/// 后端的 `STREAM_FLAGS` 里**。
///
/// ⚠ 抠的是**后端源文件**（`include_str!`），不是本仓某个常量的副本 ——
/// 两侧同源的判据会恒真。
#[test]
fn the_stream_flags_monitor_sends_are_all_strippable() {
    let backend_lib = include_str!("../../src/backend/lib.rs");
    assert!(
        backend_lib.contains("pub const STREAM_FLAGS: &[&str]"),
        "后端侧 STREAM_FLAGS 不在预期文件里，双写点锚点已失效"
    );
    // 〔E2〕那张表 fmt 之后折成多行、第一项是引用 `STREAM_FLAG_EXPLICIT` ⇒ 取到 `];` 为止，再把那个常量的字面量补进来。
    let at = backend_lib
        .find("pub const STREAM_FLAGS: &[&str]")
        .expect("抠不到 STREAM_FLAGS");
    let table = &backend_lib[at..at + backend_lib[at..].find("];").expect("STREAM_FLAGS 没收尾")];
    let explicit = backend_lib
        .lines()
        .find(|l| l.contains("pub const STREAM_FLAG_EXPLICIT: &str"))
        .expect("抠不到 STREAM_FLAG_EXPLICIT");
    let line = format!("{table} {explicit}");

    // monitor 侧：从**生产函数体**里抠它真的 `push` 了哪几个串，不手抄一份清单 —— 手抄的那种漏一条不会红。
    // 〔DEL〕远端只剩常驻一形 ⇒ 旗标只经 attach 行（`remote_resident::attach_line`）交给那台，
    //   远端 `listen::attach_flags` 遇到不在 STREAM_FLAGS 里的词整条拒（不是静默忽略）。
    let rr = guard_core::production_code(include_str!("../../src/bridge/src/remote_resident.rs"));
    let at = rr
        .find("pub(crate) fn attach_line(")
        .expect("生产段里没有 `attach_line` —— 抽取器坏了，本条此刻无效");
    let body = &rr[at..at + rr[at..].find("\n}\n").expect("attach_line 没收尾")];
    let mut sent: Vec<&str> = Vec::new();
    for seg in body.split(r#"f.push(""#).skip(1) {
        if let Some(end) = seg.find('"') {
            sent.push(&seg[..end]);
        }
    }
    for f in &sent {
        assert!(
            line.contains(&format!("\"{f}\"")),
            "monitor 会发 `{f}`，而后端的 STREAM_FLAGS 里没有它 ⇒ \
             远端 `attach_flags` 整条拒 ⇒ 接不上那台的常驻后端。\n\
             后端那一行现打：{line}"
        );
    }
    // 〔E2 · V28〕流模式显式词：后端表里有它（名字是 `ccm` 时零参数是起会话）。
    // 〔MIG-1 续〕「测试连接探针那一发带它」那一格随探针搬进本机后端：那一发今天住后端 `dial/probe.rs`，
    //   由它自己拼（`crate::STREAM_FLAG_EXPLICIT`，与本表同一个常量），不再经 monitor。
    let word = crate::backend::control::local_backend::STREAM_WORD;
    assert!(
        line.contains(&format!("\"{word}\"")),
        "后端 STREAM_FLAGS 不认 `{word}`：{line}"
    );
    // 反向自检：抠出来的就是那三条（相等；防「抠出来是空的也全绿」）。
    assert_eq!(
        sent,
        ["--with-bg", "--tail-only", "--with-rbind-token"],
        "`attach_line` 发的旗标抠出来不是那三条 —— 抽取坏了或步 3 的接线断了"
    );
}

#[test]
fn capability_gate_matrix() {
    // 空集 = 旧 backend / 尚未收到 hello → 全降级
    assert_eq!(decide_stream_flags(&caps(&[]), true), (false, false, false));
    assert_eq!(
        decide_stream_flags(&caps(&[]), false),
        (false, false, false)
    );
    // 全能力声明
    assert_eq!(
        decide_stream_flags(&caps(&["bg", "tail-only", "rbind-token"]), true),
        (true, true, true)
    );
    assert_eq!(
        decide_stream_flags(&caps(&["bg", "tail-only", "rbind-token"]), false),
        (false, true, true),
        "关 showBgSessions 只关 with_bg，tail-only / rbind-token 照开"
    );
    // 部分声明：只有 tail-only → with_bg 恒 false（即便 show_bg）
    assert_eq!(
        decide_stream_flags(&caps(&["tail-only"]), true),
        (false, true, false),
        "backend 没声明 bg → 即便用户想看也不发 --with-bg"
    );
    // 部分声明：只有 bg
    assert_eq!(
        decide_stream_flags(&caps(&["bg"]), true),
        (true, false, false),
        "backend 没声明 tail-only → 不发 --tail-only（历史走全量推流）"
    );
    // 🔴 〔步 3〕只有 rbind-token：**这一位与另两位正交**，而且
    //    **没有用户开关**（它不是偏好，是「这台后端报不报得出令牌」）。
    assert_eq!(
        decide_stream_flags(&caps(&["rbind-token"]), true),
        (false, false, true),
        "只声明 rbind-token 时这一位没开 —— 令牌永远到不了 monitor"
    );
    assert_eq!(
        decide_stream_flags(&caps(&["rbind-token"]), false),
        (false, false, true),
        "showBgSessions 关了却把 rbind-token 这一位也一起关了 —— 那是把两件事绑在一起"
    );
    // 未知 token 忽略（加法式向前兼容：未来后端声明我们还不认识的能力）
    assert_eq!(
        decide_stream_flags(&caps(&["bg", "tail-only", "rbind-token", "future-x"]), true),
        (true, true, true),
        "未知能力 token 不影响已知门控"
    );
}

use super::should_upgrade_reconnect as up;

/// F66 ★ 防无限重连：`should_upgrade_reconnect` 只在「下一轮严格增开一个本轮关着的
/// flag」时才 true——保证收敛。这条测试是收 hello 自愈升级那段的回归护栏
/// （审计阻塞：那段防死循环逻辑此前零测试；抽成纯函数后在此穷举）。
///
/// 🔴 〔`设计/80 §8.7` 步 3〕元组 2 → 3 位。收敛上界随之 2 → 3 轮。
/// **本条改成真穷举**（2³ × 2³ = 64 格全跑）：三位之后手挑边角会漏，
/// 而漏掉的那一格的症状是**无限重连**（本仓 v2.22.1 栽过一次）。
#[test]
fn upgrade_reconnect_converges() {
    let all: Vec<(bool, bool, bool)> = (0..8)
        .map(|m| (m & 1 != 0, m & 2 != 0, m & 4 != 0))
        .collect();
    // ① 定义：恰好在「某一位 next 开着而 cur 关着」时为真。
    for &cur in &all {
        for &next in &all {
            let strictly_more = (next.0 && !cur.0) || (next.1 && !cur.1) || (next.2 && !cur.2);
            assert_eq!(
                up(cur, next),
                strictly_more,
                "cur={cur:?} next={next:?}：升级判定与「严格增开某一位」不等价"
            );
        }
    }
    // ② ★ 关键收敛点（记账之后 `caps` 就是声明集 ⇒ `next == cur`）：**恒不再重连**。
    //    这一条是「绝不无限重连」的全部内容，单独写出来是为了让它红的时候说得清。
    for &cur in &all {
        assert!(!up(cur, cur), "next==cur 时还要重连 ⇒ 无限重连：{cur:?}");
    }
    // ③ 下一轮更弱（不该发生，但函数必须安全）→ 不重连。
    assert!(!up((true, true, true), (false, false, false)));
    // ④ 点名钉住新那一位：本轮 bg+tail 已开、后端又声明了 rbind-token ⇒ 该升级。
    //    （只有 ① 那个循环的话，把第三项整个删掉会让 `strictly_more` 跟着变 ——
    //     那是**两侧同源恒真**。这一条写字面量，删第三项它会红。）
    assert!(
        up((true, true, false), (true, true, true)),
        "本轮没开 rbind-token、后端声明了，却不升级 ⇒ 令牌永远到不了 monitor"
    );
    assert!(!up((true, true, true), (true, true, false)));
}

/// F66：确认 `build.rs::emit_backend_capabilities` 那条单源管道真的通（非空、含当前
/// token）——否则乐观路径静默退化成「第一轮降级 + hello 自愈」（仍正确，只慢一轮）。
/// 用 `contains` 而非精确相等：backend 将来加 token 时本测试仍过，不误红。
///
/// 〔`K-R19` 订正 09-03〕这一句原先点的是 `EMBEDDED_BACKEND_CAPABILITIES`，**全仓零定义**
/// ——管道上三个真名依次是：`build.rs::emit_backend_capabilities` → 编译期 env
/// `BACKEND_CAPABILITIES` → `ssh_source.rs::embedded_backend_capabilities`。
#[test]
fn embedded_capabilities_single_source_wired() {
    let caps = super::embedded_backend_capabilities();
    assert!(caps.contains(&"bg".to_string()), "单源应含 bg：{caps:?}");
    assert!(
        caps.contains(&"tail-only".to_string()),
        "单源应含 tail-only：{caps:?}"
    );
}

/// U-1（2026-08-01）：**`build_id` 那半单源管道一直没有等价断言。**
///
/// 〔墓碑 —— 本段原话逐字：「`build.rs::emit_backend_build_id` 抠不到就
///  `unwrap_or_else(|| "unknown")` —— **静默退化**。一旦 backend crate 改名 /
///  `BUILD_ID` 挪出 `main.rs` / `const` 写法换行，`EXPECTED_BACKEND_BUILD_ID` 会变成
///  `"unknown"`，而**编译通过、测试全绿**，运行期把每台远端后端都判成 `StaleBuild`
///  → 无限重装。」**那段描述在 `19b`（09-19）之前逐字为真。**〕
///
/// 🔴 **今天那条兜底没了**：`build.rs::backend_source_build_id()` 抠不到就**当场 panic**
/// （`设计/96 §7.2.5`）⇒ 这一形在**所有**构建形态下都编不过，轮不到测试来发现。
/// ⇒ 本条断言的**人群因此变小了**：它今天只逮得住「有人在 `lib.rs` 里把 `BUILD_ID`
/// 真的写成 `"unknown"`」这一种（那是一次故意的手滑，不是路径漂）。
/// ⚠ **留着它不是留一条恒真断言** —— 下面那两条形状检查（非空 / ≤64 / 字符集）
/// 是它今天真正在买的东西；`"unknown"` 这一条降级为**便宜的第二道**。
/// ⚠ 「路径漂」那一维今天由两处接住，都不在这里：`build.rs` 那条 panic（构建期）与
/// `tests/evidence/K-R124-ruler.py` 的 ⑩（`release.yml` 里每一处抽取住址实打指得到真东西）。
///
/// capabilities 那半有 `embedded_capabilities_single_source_wired` 兜着，这半没有。
/// U13 的仓库级重命名**必须**先有这条，否则那次重命名是静默失败。
///
/// 判据刻意宽松（不写死具体 id）：只要不是兜底值、且长得像一个 build id 就行 ——
/// 写死 id 会让每次正常 bump 都误红，那种守卫最后会被人删掉。
#[test]
fn embedded_build_id_single_source_wired() {
    let id = super::EXPECTED_BACKEND_BUILD_ID;
    assert_ne!(
        id, "unknown",
        "`build.rs::emit_backend_build_id` 没抠到后端的 `const BUILD_ID` —— \
             多半是路径失效（crate 改名 / 文件搬家）或 `const` 写法变了。\
             它是**静默退化**：不修的话每台远端都会被判 StaleBuild 并无限重装。"
    );
    assert!(
        !id.is_empty() && id.len() <= 64,
        "build_id 形状可疑：{id:?}"
    );
    assert!(
        id.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.'),
        "build_id 含意外字符（多半是抠错了行）：{id:?}"
    );
}

/// 🔴 ★ 〔`设计/80 §8.7` 步 3〕**升级判定不许再被 `if !tail_only` 包住。**
///
/// 两位的世界里那道外层 guard 等价于「本轮跑在降级模式」；三位之后它当场为假 ——
/// `tail_only` 已开、`rbind-token` 这一位没开是真实可达的状态。那时 guard 会把升级整个
/// 跳过 ⇒ `--with-rbind-token` 永远发不出去 ⇒ 令牌字段恒缺席 = 一个**合法值** ⇒ 极安静。
///
/// 纯函数那两条（`upgrade_reconnect_converges` / `capability_gate_matrix`）**结构上看不见
/// 调用点**，这一条按源文本钉：`if should_upgrade_reconnect(` 那一行的缩进，必须与同一个
/// hello 分支里 `if build_id == EXPECTED_BACKEND_BUILD_ID {` 那一行**相同**
/// （被任何一层 `if` 包住，缩进就会深一格）。
/// ⚠ 买不到：「没包住但改成了别的等价短路」（例：`!tail_only && should_upgrade…`）——
///   下面第二条断言只挡了最直白的那一形。
#[test]
fn the_upgrade_check_is_not_hidden_behind_the_tail_only_guard() {
    let prod = guard_core::production_code(include_str!("../../src/bridge/src/ssh_source.rs"));
    let indent = |needle: &str| -> usize {
        let hits: Vec<&str> = prod.lines().filter(|l| l.contains(needle)).collect();
        assert_eq!(
            hits.len(),
            1,
            "`{needle}` 在生产段里不是恰好 1 处：{hits:#?}"
        );
        hits[0].len() - hits[0].trim_start().len()
    };
    let call = indent("if should_upgrade_reconnect(");
    let anchor = indent("if build_id == EXPECTED_BACKEND_BUILD_ID {");
    assert!(
        anchor >= 8,
        "锚点缩进只有 {anchor} —— 抽错了行，本条此刻无效"
    );
    assert_eq!(
        call, anchor,
        "升级判定被包进了某个 `if` 里（缩进 {call} ≠ 锚点 {anchor}）—— \
         若那是 `if !tail_only`，`--with-rbind-token` 在 tail_only 已开的连接上永远发不出去"
    );
    let line = prod
        .lines()
        .find(|l| l.contains("if should_upgrade_reconnect("))
        .unwrap();
    assert!(
        !line.contains("tail_only &&"),
        "升级判定前面被短路了：{line}"
    );
}

/// 〔ST3〕★ 接缝：hello 里不认识的能力 token 记在**那台远端**名下；认识的一个都不记。
#[test]
fn unknown_capabilities_are_booked_under_that_remote() {
    use crate::drift_ledger::{snapshot, DriftFace};
    let devbox = crate::origin::Origin("st3-hello-probe".into());
    super::note_unknown_capabilities(&devbox, &caps(&["bg", "st3-cap-probe"]), "b1");
    let keys = |o: &crate::origin::Origin| -> Vec<String> {
        snapshot(o)
            .into_iter()
            .filter(|f| f.face == DriftFace::UnknownBackendToken)
            .flat_map(|f| f.entries.into_iter().map(|e| e.key))
            .collect()
    };
    assert_eq!(
        keys(&devbox),
        vec!["capabilities:st3-cap-probe".to_string()],
        "那台名下该恰好是那个不认识的 token"
    );
    assert!(
        !keys(&crate::origin::Origin::local()).contains(&"capabilities:st3-cap-probe".to_string()),
        "远端 hello 的 token 记进了本机那一本"
    );
}

// ─── 〔CF1 · 第四波 09-24〕F5：本机后端的起参也是「monitor 发、后端剥」的那一族 ─────────────────
// 与上面那条（远端 attach 行的旗标）同一族「monitor 发、后端认」；本机起参那一形后端不认的 `--flag` 会被当成一次性查询、
// 跑完就退（§26）。本机那两条载体的起参是 `local_backend::LOCAL_STREAM_ARGS` 一份常量；住这里是因为
// 「读后端 `lib.rs` 的源码」这条跨半边已经为本文件登记过了（`cross_half_edge_registry`），不另开一条。
// 判据总表住 `local_lines_tests.rs` 头注（F1–F8）。

/// 后端 `lib.rs::STREAM_FLAGS` 的字面量（从后端源码摘，异源）。
fn backend_stream_flags_cf1() -> std::collections::BTreeSet<String> {
    let src = include_str!("../../src/backend/lib.rs");
    let at = src
        .find("pub const STREAM_FLAGS: &[&str] = &[")
        .expect("后端 lib.rs 里找不到 `STREAM_FLAGS` 的定义");
    let rest = &src[at..];
    let body = &rest[..rest.find("];").expect("STREAM_FLAGS 没有收尾")];
    let mut out: std::collections::BTreeSet<String> = body
        .split('"')
        .skip(1)
        .step_by(2)
        .map(str::to_string)
        .collect();
    // 〔E2〕表里第一项引用 `STREAM_FLAG_EXPLICIT` ⇒ 从后端源码补它的字面量。
    if body.contains("STREAM_FLAG_EXPLICIT") {
        let l = src
            .lines()
            .find(|l| l.contains("pub const STREAM_FLAG_EXPLICIT: &str"))
            .expect("抠不到 STREAM_FLAG_EXPLICIT");
        out.insert(
            l.split('"')
                .nth(1)
                .expect("STREAM_FLAG_EXPLICIT 没有字面量")
                .to_string(),
        );
    }
    out.into_iter().collect()
}

#[test]
fn both_carriers_start_the_backend_with_the_same_stream_flags_the_backend_strips() {
    use crate::backend::control::local_backend::LOCAL_STREAM_ARGS;
    let backend = backend_stream_flags_cf1();
    assert!(
        backend.len() >= 2,
        "后端 STREAM_FLAGS 只摘到 {backend:?} —— 抽取坏了"
    );
    // 〔V151〕打头的 `--` 是分隔（让 `ccm` 当后端用），不是流模式旗标。
    assert_eq!(
        LOCAL_STREAM_ARGS.first(),
        Some(&"--"),
        "本机后端起参没以 `--` 打头"
    );
    for a in &LOCAL_STREAM_ARGS[1..] {
        assert!(
            backend.contains(*a),
            "本机后端起参 `{a}` 不在后端 `STREAM_FLAGS`（{backend:?}）里 —— 后端不剥它 ⇒ 当成一次性查询跑完就退（§26）"
        );
    }
    assert!(
        LOCAL_STREAM_ARGS.contains(&"--with-bg"),
        "少了 `--with-bg` ⇒ bg 会话的内容从此静默没了（`showBgSessions` 缺省是开的）"
    );
    assert_eq!(
        LOCAL_STREAM_ARGS.contains(&"--tail-only"),
        crate::ssh_source::LOCAL_STREAM_TAIL_ONLY,
        "起参里有没有 `--tail-only` 与本机消费者认定的「这条流是 tail-only」对不上 —— \
         要么历史整份重放两遍，要么历史一行都没有"
    );
    // 两条载体用的是**这一份**，不是各写一份字面量。
    let stdio = guard_core::production_code(include_str!(
        "../../src/bridge/src/backend/control/local_backend.rs"
    ));
    let host =
        guard_core::production_code(include_str!("../../src/bridge/src/local_backend_host.rs"));
    assert_eq!(
        stdio.matches("LOCAL_STREAM_ARGS.iter()").count(),
        2,
        "stdio 载体的两个起法（`start_if_present` · `start_or_extract`）都要用 LOCAL_STREAM_ARGS"
    );
    guard_core::find_pinned(&host, "cmd.args(local_backend::LOCAL_STREAM_ARGS)")
        .unwrap_or_else(|e| panic!("常驻载体没用 LOCAL_STREAM_ARGS：{e}"));
    assert_eq!(
        stdio.matches("\"--tail-only\"").count(),
        1,
        "local_backend·rs 里 `\"--tail-only\"` 字面量只许住在 LOCAL_STREAM_ARGS 那一处"
    );
    assert_eq!(
        host.matches("\"--tail-only\"").count(),
        0,
        "local_backend_host·rs 里不许再有自己的 `\"--tail-only\"` 字面量"
    );
}

/// 〔LOC1b · 第四波 4D〕本机起参要带 `--with-rbind-token`：`session_added.pid` 跟令牌同一道闸
/// （后端 `wire::Frame::SessionAdded::pid`），本机 ↗ 绑窗口只能从那一格拿 pid（monitor 不再自己读 pidfile）。
/// 异源：旗标字面量从后端 `STREAM_FLAGS` 源码里摘，确认后端真剥它。
#[test]
fn loc1b_the_local_stream_asks_for_the_binding_material() {
    use crate::backend::control::local_backend::LOCAL_STREAM_ARGS;
    assert!(
        LOCAL_STREAM_ARGS.contains(&"--with-rbind-token"),
        "本机起参少了 `--with-rbind-token` ⇒ 本机 `session_added` 不带 pid ⇒ 本机 ↗ 按 PowerShell 父进程绑窗口那一跳没有 pid"
    );
    assert!(backend_stream_flags_cf1().contains("--with-rbind-token"));
}
