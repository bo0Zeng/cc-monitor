/// 〔audit-0805 08-06〕**防命令注入的那道校验，此前一条判据都没有。**
///
/// # 怎么找到的
///
/// 新先验：抽「只被一处调用」的生产函数。`has_bad_chars` 在全仓只出现两次
/// （定义 + 一处调用），顺着它找到唯一消费者 [`validate_config_dir_ps`] ——
/// 而它 5 处出现里**没有一处是测试**。
///
/// 它守的是「**拒绝拼入命令**：非法 CLAUDE_CONFIG_DIR」，也就是把一个用户可控的
/// 目录名塞进 PowerShell 命令串之前的最后一道闸。失效形态是**静默放行**：
/// 校验松掉不会让任何测试变红，而后果是命令串里多了一个 `;` 或 `$(...)`。
///
/// ⚠ 它 `#[cfg(any(windows, test))]` —— **Linux 的测试构建里是编译的**，
/// 所以这一族与 `ROADMAP §5` 的 3y（Windows-only 代码本机连编译都不碰）**不同**：
/// 这里没有平台借口，只是没人写。
///
/// # 用例挑的是「每一条拒绝理由各一发 + 两条不许误拒」
///
/// 不许误拒那两条是有来历的：头注逐字记着 Phase G 审计抓出的真 bug ——
/// 早先两边共用「必须 `/` 开头 + 禁 `\`」，于是真实的 Windows 账号目录
/// `C:\Users\z\.claude-alt\z` **必被拒**，「本机分叉时选具名账号」在主平台 100% 失败。
/// ⇒ 反向用例把那个回归钉住。
///
/// 〔IV1 · V121〕要求住址：`INVARIANTS §47`（外部值拼进 shell / 交给对端之前本侧先过放行判定）；②形（拒绝集 ＋ 形式判定）。
#[test]
fn the_config_dir_validator_rejects_every_injection_shape() {
    // ★ 先证明夹具走得通：两种平台的合法绝对路径都必须过。
    for ok in [
        "/home/z/.claude",
        "C:\\Users\\z\\.claude-alt\\z",
        "\\\\server\\share\\claude",
    ] {
        assert!(
            validate_config_dir_ps(ok).is_ok(),
            "合法路径被拒了：{ok:?} —— 这正是 Phase G 抓出的那个真 bug 的形状\n\
                 （早先禁 `\\` ⇒ 每个 Windows 账号目录都过不去，主平台 100% 失败）"
        );
    }

    // ① 非绝对 / 根 / `..` 穿越（两种分隔符、中间与结尾各一）
    for bad in [
        "relative/path",
        ".claude",
        "/",
        "/home/../etc",
        "/home/..",
        "C:\\a\\..\\b",
        "C:\\a\\..",
    ] {
        assert!(
            validate_config_dir_ps(bad).is_err(),
            "路径形态没被拒：{bad:?}"
        );
    }

    // ② 控制字符与 C1 段（`\u{85}` 在很多终端里不可见）
    for bad in [
        "/home/z\u{0}/x",
        "/home/z\n/x",
        "/home/z\u{85}/x",
        "/home/z\u{9f}/x",
    ] {
        assert!(
            validate_config_dir_ps(bad).is_err(),
            "控制字符没被拒：{bad:?}"
        );
    }

    // ③ shell 元字符 —— **逐个**过，不是抽一个代表。
    //    ★ 自检：集合非空，否则这个循环是空转的。
    assert!(
        !crate::backend::control::payload::SHELL_META_COMMON.is_empty(),
        "`SHELL_META_COMMON` 空了 —— 下面这轮是空转的"
    );
    for c in crate::backend::control::payload::SHELL_META_COMMON.chars() {
        let bad = format!("/home/z{c}/x");
        assert!(
            validate_config_dir_ps(&bad).is_err(),
            "shell 元字符 {c:?} 没被拒 —— 它会被原样拼进命令串"
        );
    }

    // ④ 同形欺骗字符（走 `acct_core::is_deceptive_char` 那条并集）
    //    先确认这个字符确实被那张表认得，否则用例本身可能选错了字。
    // ★ 这条自检当场救过一次：第一版选的是 `\u{2044}`（FRACTION SLASH，肉眼像 `/`），
    //   它**不在** `acct_core` 那张表里 —— 若没有这条自检，下面那条会因为别的原因红/绿，
    //   而我会以为「欺骗字符这一支验过了」。
    for deceptive in ['\u{200B}', '\u{202E}', '\u{FEFF}', '\u{00A0}'] {
        assert!(
            acct_core::is_deceptive_char(deceptive),
            "样本字符 {deceptive:?} 不在 `acct_core` 的欺骗字符表里 —— \
                 换一个，否则下面那条在测别的东西"
        );
        assert!(
            validate_config_dir_ps(&format!("/home/z{deceptive}etc")).is_err(),
            "同形/不可见字符 {deceptive:?} 没被拒 —— 它在终端里看不见，却会原样进命令串"
        );
    }
}

/// ★★ **把「本机 resume 到底跑什么」钉在真构造器上**〔audit-0805 F08 / 报告 B-2〕。
///
/// # 此前那条判据在替代码说好话
///
/// `launch_tests.rs::local_and_remote_share_the_same_payload` 用的是**手写夹具**
/// `"… && ccm --ccm-tmux claude --resume s1"`，而它**从不调用**真正的 payload 构造器。
/// 那个夹具里有 `--tmux`，生产里没有 —— **判据恰好体现了生产违反的那个假设**。
///
/// # 本条钉的是**现状**，不是理想
///
/// 它断言生产 payload 里**确实没有容器**（既无 `--tmux` 也无 `cct`）。
/// 这不是在祝福这个行为 —— 是让它**不能再悄悄变、也不能再被一条漂亮的夹具盖住**。
/// 真要改成进容器，改完这条会红，那时才是带着证据做决定的时刻。
///
/// ⚠ 功能后果（claude 在 `stdin=/dev/null` 下具体怎么表现）**红线内测不了**，
/// 本条只钉**命令串**这一层可判据的事实。
#[test]
fn the_local_resume_payload_has_no_session_container_today() {
    let choice = local_launch_choice(&LocalPsAction::Resume("s1".into()), None)
        .expect("resume s1 应该能构造出来");
    let rendered = match &choice {
        LocalLaunchChoice::Fixed(c) => c.clone(),
        LocalLaunchChoice::Probe {
            preferred,
            fallback,
            ..
        } => format!("{preferred} | {fallback}"),
    };
    assert!(
        rendered.contains("--resume") && rendered.contains("s1"),
        "抽取器自检：构造出来的串里连 `--resume s1` 都没有 —— 切错东西了：{rendered}"
    );
    assert!(
        !rendered.contains("--ccm-tmux") && !rendered.split_whitespace().any(|w| w == "cct"),
        "★★ **回落那条路**产出了会话容器（`--tmux` / `cct`）—— 本条不该再绿。\n\
             \n\
             ⚠ **P3t-Y3 翻面**：本条**测什么没变，自陈换了**。它量的从来只是\n\
             `local_launch_choice`，也就是 **P3t 之后的回落路**（渲染器拒了才走的那条）。\n\
             P3t 之前那等价于「本机 resume 没有容器」；**现在不等价了** ——\n\
             本机先过 `render_local_ccm`，渲得出来就带 `--tmux`。\n\
             ⇒ 本条现在钉的是「**回落路仍是无容器的那条**」：它是诚实降级的落点，\n\
             不是第二条并列的路（顺序由 `the_local_launch_tries_the_renderer_before_the_old_path` 钉）。\n\
             真要连回落也进容器，请连同 `launch.rs` 与 `src/fork-start.ts` 那两条头注一起改。\n\
             实得：{rendered}"
    );
}

/// ★★ **P3t-Y3 的翻面另一半 —— 不许翻成更弱的一条。**
///
/// 上面那条钉「回落路没有容器」。光有它，**整个 P3t 被回退掉也不会红**
/// （回落路本来就该没容器，回退之后它还是没容器）。
/// ⇒ 必须再钉正面事实：**渲染得出来的时候，那条串真的带 `--tmux`**。
///
/// 判据怎么失效（`P2s-Y3` / `P3 刀 0` 各栽过一次）：翻面时只留「旧事实不再成立」，
/// 丢掉「新事实成立」。所以这里同时钉容器名**就是传进去的那个**
/// —— 若它被换成 Rust 自己铸的名字，`U11` 那个撞名坑就回来了。
/// 判据自己给能力集 —— **不问这台机器**。
///
/// = `CLI_REQUIRED_CAPS`（每次调用都要的静态能力）**加上两条 §37 维度能力**
/// （`account` 恒真维度要 / `model` 条件式维度要）。第一版只给了前者，
/// 当场报「维度 account 需要远端 ccm 能力 account」—— 那正是 §37 把两类能力
/// 分开的意义：静态那张表**不是**全集，照它拼会漏。
///
/// 缺能力时该怎样，由 `ccm_invocation` 自己那两条（`MissingCap` / `DimensionNeedsCap`）钉；
/// 本文件只需要一个「能力齐」的输入。
#[cfg(not(windows))]
fn caps_of_a_current_ccm() -> std::collections::BTreeSet<String> {
    crate::backend::control::ccm_invocation::CLI_REQUIRED_CAPS
        .iter()
        .map(|c| (*c).to_string())
        .chain(["account".to_string(), "model".to_string()])
        .collect()
}

// ═══════════════════════════════════════════════════════════════════════
// `K-R106` `KR106D1`：**本机后端产得出 `attach` 那一句**，而且它落在刚建的那个会话上
// ═══════════════════════════════════════════════════════════════════════

/// 后端那份 `ccm` 的 **argv 解析**半。跨半边编译期边，登记住
/// `cross_half_edge_registry::CROSS_EDGES`。
#[cfg(not(windows))]
const CCM_ARGV_SRC: &str = include_str!("../../src/backend/control/ccm/argv.rs");

/// 后端那份 `ccm` 的 **计划 + 等价 shell 渲染**半。同上。
#[cfg(not(windows))]
const CCM_PLAN_SRC: &str = include_str!("../../src/backend/control/ccm/plan.rs");

/// 后端那份 `ccm` 把 `Plan::Attach` 渲成什么 —— **逐字**。
///
/// 钉整行而不是钉 `"tmux attach"` 四个字：`=名:` 那个**精确匹配形**是承重的
/// （裸 `-t <名>` 会打到兄弟会话上，`session-backend.ts::exactTarget` 头注记着实测）。
/// 只钉动词的话，把 `={name}:` 改成 `{name}` 照样绿，而那一刀的后果是接错会话。
#[cfg(not(windows))]
const CCM_ATTACH_RENDER: &str =
    r#"Plan::Attach { name } => format!("tmux attach -t {}", sq(&format!("={name}:")))"#;

/// 🔴🔴 `KR106D1`〔用@09-13「**归本机后端就好了啊**」〕：
/// **本机后端产得出 `attach` 那一句，而且那一句落在它刚建的那个会话上。**
///
/// # 它为什么不判「枚举里有 `Attach` 这个词」
///
/// 那是本件单子逐字点名的失效方向：**加个变体不接线照样绿**。
/// ⇒ 本条一个字都不读源码里的枚举，它**驱动生产渲染路**
/// （[`render_local_ccm_with`]，本机 `new`/`resume` 走的同一条），
/// 从**渲出来的那两串话本身**里把会话名读回来比。
///
/// # 四段各买什么（别读成一段）
///
/// | 段 | 它挡住的那一刀 |
/// |---|---|
/// | ① 名字形状 | `K-R87` 那次 `ccm-oneshot-` 两形都不命中 ⇒ 建出来的是**失管会话** |
/// | ② 建/接同名 | attach 那一臂渲成 `ccm attach <sid>` 或干脆掉进 `_ => new` 兜底 |
/// | ③ 跨半边 | 我们产的这一串，后端那份 `ccm` 真把它读成「接进这个名字」 |
/// | ④ fail-closed | 旧路 / spawn 那条路被要求 attach 时**拒**，不许凑一个出来 |
///
/// # ⚠ 它买不到什么（如实写）
///
/// - ③ 是**文本级的两侧同形**，不是真跑一次 `ccm`。真跑那一格归 e2e
///   （`ccm-print-parity` 里「attach 到 cc-p1」那条）。**本条不声称跑过。**
/// - 🔴 **`K-R109`（09-13）订正：前端在问它要了。** 原文写「前端今天还没在问它要：
///   `src/remote-launch-run.ts` 那条 `↗` 仍问 `SESSION_BACKEND.attach` 要」——
///   那四处接线本轮全部落地（属性 · `generate_handler!` · `LEDGER` · 包装层），
///   `runLocalResumeIntoExistingTmux` 现在 `await commands.render_local_attach(…)`。
///   ⚠ **本条钉的仍然只是「后端产得出」** —— 「有人在用」那一半由前端那一侧的判据钉
///   （`tests/remote-launch-run.vitest.ts` 的 `KR109D2` 两条），两处别混成一处。
#[test]
#[cfg(not(windows))]
fn the_local_backend_renders_an_attach_that_lands_on_the_session_it_just_created() {
    let caps = caps_of_a_current_ccm();
    // ① 名字不是随手起的：它要过 `gate_core` 那两形之一，否则起出来的会话主路认不出、
    //    杀不掉 —— `K-R87` 那次 `ccm-oneshot-<x>` 两形都不命中，就是这个坑。
    const NAME: &str = "s1abcdef-cc";
    assert!(
        gate_core::is_ccm_tmux_name(NAME),
        "本条自己用的名字就过不了 Gate 2 —— 那么下面量到的一切都在量一个失管会话"
    );
    // 阴性对照：`K-R87` 那个形状**必须**不过，否则上面那条是空真。
    let the_r87_shape = format!("ccm-oneshot-{}", "abcdef");
    assert!(
        !gate_core::is_ccm_tmux_name(&the_r87_shape),
        "{the_r87_shape:?} 居然过了 Gate 2 —— 上面那条断言此刻什么都没买到"
    );

    let acct = LaunchAccount::Base;
    // 建那一句（今天就产得出的）与接那一句（本轮加的）**走同一条渲染路、同一个名字**。
    let created = render_local_ccm_with(
        &LocalPsAction::New,
        None,
        Some(&acct),
        Some(NAME),
        &caps,
        true,
    )
    .expect("建那一句本来就渲染得出来 —— 渲不出说明本条的前提变了，回来重裁");
    let attach = render_local_ccm_with(
        &LocalPsAction::Attach,
        None,
        Some(&acct),
        Some(NAME),
        &caps,
        true,
    )
    .expect(
        "本机后端产不出 attach —— `KR106D1` 的正题就是这一句〔用@09-13「归本机后端就好了啊」〕",
    );

    // ② 会话名从**那两串话本身**里读回来，不是拿常量对常量。
    let created_target = created
        .split_whitespace()
        .find_map(|t| t.strip_prefix("--ccm-tmux="))
        .unwrap_or_else(|| {
            panic!("建那一句里没有 `--ccm-tmux=<名>`，它根本没建容器 —— 下面两条会空转：{created}")
        });
    let mut toks = attach.split_whitespace();
    assert_eq!(
        toks.next(),
        Some("ccm"),
        "attach 那一句不是在调后端的命令行入口（`K26`：`ccm` 就是它）：{attach}"
    );
    // V138：接回从位置动作 `attach <名>` 改成壳层选项 `--attach <名>`；〔V151〕它是 ccm 的，写在 `--` 右边。
    assert_eq!(toks.next(), Some("--"), "attach 那一句没有 `--`：{attach}");
    assert_eq!(
        toks.next(),
        Some("--attach"),
        "\n★ 本机后端渲出来的**动作不是 attach**（实得整串：{attach}）。\n\
             最可能的形状：`Attach` 那一臂掉进了 `render_ccm_invocation` 的 `_ => new` 兜底 ——\n\
             那一刀的后果不是「没接上」，是**另起一条 claude**，而用户以为回到了原会话。"
    );
    let attach_target = toks
        .next()
        .unwrap_or_else(|| panic!("attach 那一句没有目标会话名：{attach}"));
    assert_eq!(
        toks.next(),
        None,
        "`ccm --attach <名>` 不收任何修饰 flag（`ccm_invocation` 那一支早于维度循环 return），\
             多出来的东西说明它走了别的分支：{attach}"
    );
    assert_eq!(
        attach_target, created_target,
        "\n★★ **接的不是刚建的那个会话**：建的是 {created_target:?}，接的是 {attach_target:?}。\n\
             这两个名字在生产代码里本来就是同一个 `&str`（`render_local_ccm_with` 的 `name`，\n\
             同时喂给 `Action::Attach` 与 `Container::Tmux`）—— 它们不相等只有一种可能：\n\
             有人给 attach 那一臂另开了一个名字来源。"
    );
    assert!(
        gate_core::is_ccm_tmux_name(attach_target),
        "接进去的那个名字过不了 Gate 2（{attach_target:?}）—— 主路认不出它，杀不掉它"
    );

    // ③ 跨半边：我们产的这一串，后端那份 `ccm` 真把它读成「接进这个名字」。
    let argv_prod = guard_core::production_code(CCM_ARGV_SRC);
    let plan_prod = guard_core::production_code(CCM_PLAN_SRC);
    assert!(
        argv_prod.len() > 3_000 && plan_prod.len() > 5_000,
        "跨半边语料只读进来 {} / {} 字节 —— 下面三条此刻在空转",
        argv_prod.len(),
        plan_prod.len()
    );
    assert!(
        argv_prod.contains("pub(crate) const ATTACH: &str = \"--attach\";"),
        "后端那份 `ccm` 的旗标表里 `--attach` 不见了 —— 我们产的这一句它读不成 attach"
    );
    assert!(
        argv_prod.contains("flag::ATTACH => o.attach_name = val!(),"),
        "`ccm --attach <名>` 的值不再落进 `attach_name` —— \
             那么「接哪一个」这条信息在后端那半就断了"
    );
    assert!(
        plan_prod.contains(CCM_ATTACH_RENDER),
        "\n后端那份 `ccm` 把 `Plan::Attach` 渲成的那一行变了（本条钉的整行：\n  {CCM_ATTACH_RENDER}\n\
             ）。⚠ 承重的不只是 `tmux attach` 四个字，还有 `=名:` 那个**精确匹配形** ——\n\
             裸 `-t <名>` 会按「精确名 → 名字开头 → glob」解析，打到兄弟会话上\n\
             （`src/session-backend.ts::exactTarget` 头注有 tmux 3.6 的实测）。"
    );

    // ④ fail-closed：旧路与 spawn 那条路被要求 attach 时**拒**。
    let old = build_local_posix_command(&LocalPsAction::Attach, None, None);
    assert!(
        old.as_ref().is_err_and(|e| e.contains("接不回已有会话")),
        "\n★ 旧路（`build_local_posix_command`）居然给 attach 渲出了东西：{old:?}\n\
             它只会拼一个**拉起器** ⇒ 渲出来的是「另起一条 claude」。\n\
             **静默产出比拒绝坏得多**：用户以为接回了原会话，实际两条都在跑。"
    );
    let spawned = launch_now(&LocalPsAction::Attach, None, None, None, Some(NAME));
    assert!(
        spawned.as_ref().is_err_and(|e| e.contains("自己的终端")),
        "\n★ `launch_local` 收下了 attach：{spawned:?}\n\
             那条路把命令 spawn 出去、stdio 全 null ⇒ 一个接不上任何终端的 attach 进程，\n\
             而它**还会静默成功**。attach 的正题是把用户自己的终端接进去（`§1.3`）。"
    );

    // ⑤ **产出口真的走这条路**：本机后端那个出口喂一份确定的 ccm 事实进去，
    //    拿到的必须与上面那一句**逐字节相同**（不是「长得像」）。
    fn a_current_ccm() -> crate::ccm_probe::CcmProbeResult {
        crate::ccm_probe::CcmProbeResult {
            installed: true,
            version: Some("0.0.0-判据替身".to_string()),
            capabilities: caps_of_a_current_ccm().into_iter().collect(),
            build: None,
        }
    }
    let _probe = override_ccm_probe(CcmProbeSource(a_current_ccm));
    assert_eq!(
        render_local_attach(NAME.to_string()).expect("本机后端那个产出口渲不出来"),
        attach,
        "\n★ `render_local_attach` 交出去的那一串与渲染路现算的不是同一串 ——\n\
             那说明产出口自己又走了一条（两个决定点、两套判据，正是 #76 那条病的形状）。"
    );
}

#[test]
#[cfg(not(windows))]
fn the_rendered_local_command_really_carries_the_container() {
    let name = "s1abcdef-cc";
    let cmd = render_local_ccm_with(
        &LocalPsAction::Resume("s1abcdef".into()),
        None,
        Some(&LaunchAccount::Base),
        Some(name),
        &caps_of_a_current_ccm(),
        true,
    )
    .expect("账号 0 + 有名字 + 能力齐 ⇒ 必须渲染得出来");
    assert!(
        cmd.contains("--ccm-tmux"),
        "渲出来的本机命令里没有 `--tmux` —— 那就还是**无 tty、无 tmux** 的老样子，\n\
             用户敲进去的字会被脚本吃掉。实得：{cmd}"
    );
    assert!(
        cmd.contains(name),
        "容器名不是传进去的那个（`{name}`）—— 名字只许由前端 `mintTmuxName` 铸，\n\
             在 Rust 里另铸一个就是 F13 修掉的撞名坑（见 ROADMAP `U11`）。实得：{cmd}"
    );
}

/// ★★ **P3t-Y2 给上面那条补一句射程**（不改它测什么，只改它自称守什么）。
///
/// 上面那条量的是 `local_launch_choice` —— 也就是**回落那条路**的构造器。
/// P3t-Y2 之后本机拉起先过 CLI 渲染器（`render_local_ccm`），渲不出来才落到它。
/// ⇒ 「本机 resume 没有容器」这句话**从此不再等价于**「`local_launch_choice` 没有容器」：
/// 前者要看渲染器渲不渲得出来，后者只看回落。上面那条**照旧恒绿**，但它守的人群窄了。
///
/// 本条不是重复它，是把「窄了多少」写成可执行的：今天 `render_local_ccm` 的**三格纯逻辑拒绝**
/// 决定了生产上谁能进容器。三格全拒 ⇒ 生产行为与 P3t 之前逐字节相同（Y2 是零行为改动的接线）。
/// Y2b 前端接线之后，第一格会开，那时上面那条的自陈就该改了。
///
/// # 🔴 `K-R53` 09-11：**本条的名字今天已经比它测的东西宽了一格，别照名字读它**
///
/// 函数名逐字是「前端今天送得出的**每一形**都被拒」——**那句话现在是假的**：
/// 具名账号带上名字之后渲染得出来（那正是本件开的那一格）。本条测的仍然都成立，
/// 但它的人群已经缩到「**说不出名字的**那几形」：没有会话名 · 未表态账号 · 只有目录。
///
/// 🔴 **`K-R89` 09-13：人群又缩了一格，而且这一次连「都被拒」那个动词都不对了。**
/// 「未表态账号」那一形**今天渲染得出来**（`R28`：省略 `--account` 有确定语义）——
/// 本条的 ② 因此从「必拒」翻成「必渲染得出，且不许带 `--base`/`--account`」。
/// ⇒ 今天真正**被拒**的只剩**两形**：没有会话名 · 只有目录没有名字。
/// ⚠ **仍然刻意不改名**（同 `K-R53` 那一拍的理由：改判据名要同拍跑 `pb doc`，
/// 而本件写区里没有那份生成区）。**全人群那一条仍在继任者手里**
/// [`every_local_account_shape_gets_a_named_verdict_from_the_backend_path`]，
/// 而「六格今天各自是什么」在 [`THE_SIX_WAYS_THE_OLD_PATH_STILL_WINS`]。
///
/// ⚠ **刻意不改名**：改判据的名字要同拍跑 `pb doc`（生成区会连带打红），
/// 而本件的写区里没有那份生成区。⇒ 如实登记在这里，并把**全人群**那一条交给继任者
/// [`every_local_account_shape_gets_a_named_verdict_from_the_backend_path`]
/// （它逐格点名、加变体编译不过）。**两条一起读才是今天的分母。**
#[test]
#[cfg(not(windows))]
fn the_local_renderer_refuses_every_shape_the_front_end_can_send_today() {
    let base = LaunchAccount::Base;
    let named = LaunchAccount::Named {
        config_dir: "/home/u/.claude-alt/z".into(),
        name: None,
    };
    let act = LocalPsAction::Resume("s1".into());

    // ① 没名字 —— 名字只许 `mintTmuxName` 铸，Rust 这侧不许补默认值（F13 那个坑）。
    for no_name in [None, Some(""), Some("   ")].into_iter() {
        let no_name = no_name.filter(|n: &&str| !n.trim().is_empty());
        let r = render_local_ccm_with(
            &act,
            None,
            Some(&base),
            no_name,
            &caps_of_a_current_ccm(),
            true,
        );
        assert!(
            r.as_ref().is_err_and(|e| e.contains("缺少会话名")),
            "没有会话名时必须拒 —— 在 Rust 里铸一个名字就是 F13 修掉的撞名坑第三次。实得：{r:?}"
        );
    }

    // ② 未表态账号（`None`）—— 🔴 **`K-R89` 09-13：这一格从「必拒」翻成「渲染得出来」**。
    //    翻它的不是本判据的口味，是 `DECISIONS.md#R28`（用户 09-12）：省略 `--account`
    //    在 `ccm` 上有确定语义（`plan.rs::resolve_account` 的两支）。
    //    ⚠ **翻的只有「拒不拒」，没翻的那半必须原样守住**：渲染出来的那一串里
    //    **不许出现 `--base`** —— 那是把「继承环境」偷换成「显式清空」＝ #75 病灶。
    let r = render_local_ccm_with(
        &act,
        None,
        None,
        Some("s1abcdef-cc"),
        &caps_of_a_current_ccm(),
        true,
    );
    let cmd = r.as_ref().unwrap_or_else(|e| {
        panic!(
            "未表态账号今天必须渲染得出来（`R28` 之后省略有确定语义）。\n\
                 若它又回到短路，请先回 `DECISIONS.md#R28` 看那一裁是不是被推翻了，\n\
                 别在这里把闸悄悄加回来。实得降级理由：{e}"
        )
    });
    assert!(
        !cmd.contains("--base"),
        "未表态账号被渲染成了 `--base` —— 那是把「继承环境」偷换成「显式清空」，\n\
             正是 #75「resume 在错数据目录找不到会话」。实得：{cmd}"
    );
    assert!(
        !cmd.contains("--account"),
        "未表态账号被渲染成了 `--account <某个号>` —— 那是替用户挑了一个号。实得：{cmd}"
    );

    // ③ 具名账号 —— `LaunchAccount::Named` 只有 configDir、没有名字，而 CLI 只会
    //    `--account <名字>` ⇒ §35 短路。**理由必须是「说不出」，不是别的**：
    //    reason 是生产侧唯一的降级线索，换一个理由就是换一条诊断。
    let r = render_local_ccm_with(
        &act,
        None,
        Some(&named),
        Some("s1abcdef-cc"),
        &caps_of_a_current_ccm(),
        true,
    );
    let reason =
        r.expect_err("具名账号今天渲染不出来 —— 若它成功了，请先确认 `--account` 的名字是从哪来的");
    assert!(
        reason.contains("account"),
        "具名账号的降级理由该指向 account 维度（§35 短路），实得：{reason}"
    );
}

/// ★★★ `K-R53` `KR53D1`：**本机账号的每一形，后端那条路渲染得出来吗** —— 逐格点名。
///
/// # 它判的是**分母**，不是可达性
///
/// 上面那条 (`the_local_renderer_refuses_every_shape_the_front_end_can_send_today`)
/// 钉的是 P3t-Y2 那一刻的事实「**全拒**」。本条是它的继任者：把
/// [`LaunchAccount`] 的全部形状加上「参数缺席」逐格喂一次，
/// **每一格都要说得出自己该是 `Ok` 还是 `Err`、以及 `Err` 的理由指向哪**。
///
/// 失效方向逐字（`KR53D1`）：「再加一个入口而它复用了那个缺一态的旧函数」——
/// 加一个变体 ⇒ 下面这张表的 `match` 不穷尽 ⇒ **编译不过**，不是静默漏一格。
///
/// # 🔴 缺席那一格：**09-13 `K-R89` 之前是红的，今天是绿的** —— 翻它的是一条裁定，不是一次放宽
///
/// 「参数缺席」的语义是**继承环境**（旧路发空前缀）。这里此前逐字写着「三格里没有一格
/// 逐字等于『继承』⇒ 那是**产品决定** ＋ 改 `src/backend/control/ccm/plan.rs`」，
/// 并把这一格钉成 `Err`。**那段话在 09-12 就过期了，而它一直挂在盘上等一个已经到了的决定。**
///
/// 〔`DECISIONS.md#R28`，用户 09-12 逐字：「把调用方选中的号静默换掉 / **不要这么做** /
///  不是有选默认账号吗? **就用那个**」〕⇒ 那个产品决定做了，而且**落地了**：
/// `src/backend/control/ccm/plan.rs::resolve_account` 头注挂着 ✅，
/// 省略被拆成两支，**两支都是这一裁的一部分**：
///
/// | 目标 shell 里有没有 `CLAUDE_CONFIG_DIR` | 旧路（空前缀） | ccm 省略 `--account` | ccm `--base` |
/// |---|---|---|---|
/// | 有，= X | 用 X | **尊重 X**（`R08` 那道 `-z` 闸不触发）= 继承 ✅ | `unset` ⇒ 用 `~/.claude` ❌ |
/// | 没有 | 用 `~/.claude` | **落 manifest 默认号** ✅〔`R28`：「就用那个」〕 | 用 `~/.claude` |
///
/// ⚠ **第二行那一格从 ❌ 翻成 ✅ 的是「该不该」，不是「是什么」** —— 行为一个字节没动，
/// 动的是对它的判断（`R28` 裁定零逐字：「本裁改的不是行为，是『这是不是我们要的』」）。
/// ⚠ **别把这张表压成一句「省略就是继承」**：省略是**两支**，只有第一支叫继承。
///
/// ⚠ **本机这条路上第一支到底拿谁的环境**（现打 09-13）：送法是
/// `launch::build_local_posix_argv` ⇒ `bash -lic '<cmd>'`（login ＋ interactive）
/// ⇒ 用户 rc/profile 先跑 ⇒ `ccm` 看到的就是**用户 shell 里那一个**，与旧路同源。
///
/// ⇒ **本条今天钉的是**：这一格 `Ok`，且渲染出来的那一串里 `--base` 与 `--account`
/// **一个都不许有**。谁哪天把它映成 `--base`，这里当场红 —— 那一刀正是 `#75`
///（把继承偷换成显式清空）；谁把它映成某个具名号，也当场红（那是 `R28` 禁的静默换号）。
/// ⚠ **`ccm` 那一侧怎么解释省略，本条一个字都不管** —— 那半的唯一住址是
/// `plan.rs::resolve_account`，由 `plan::the_four_ways_an_account_gets_picked` 钉着。
#[test]
#[cfg(not(windows))]
fn every_local_account_shape_gets_a_named_verdict_from_the_backend_path() {
    let act = LocalPsAction::Resume("s1".into());
    let caps = caps_of_a_current_ccm();
    let named_with_name = LaunchAccount::Named {
        config_dir: "/home/u/.claude-alt/z".into(),
        name: Some("z".into()),
    };
    let named_dir_only = LaunchAccount::Named {
        config_dir: "/home/u/.claude-alt/z".into(),
        name: None,
    };
    let base = LaunchAccount::Base;

    // 分母 = `LaunchAccount` 的全部形状 + 「参数缺席」。
    //
    // ⚠ 标签**只有一处住址**（`label_of` 里那个 `match`）—— 本条第一版在它旁边另写了一份
    //   `denominator: [&str; 4]` 字面量，那正是 `brief` 第 13b 条禁的「闭集重抄一份」：
    //   两份字面量迟早漂开，而漂开的那天两边看起来都没错。⇒ 标签一律现算。
    //
    // **穷尽性由 `label_of` 那个 `match` 买**：加一个变体而不回来加一行 ⇒ 编译不过，
    // 不是静默漏一格。这正是 `KR53D1` 的失效方向逐字
    //（「再加一个入口而它复用了那个缺一态的旧函数」）在 Rust 这一侧的落点。
    fn label_of(acct: Option<&LaunchAccount>) -> &'static str {
        match acct {
            None => "缺席",
            Some(LaunchAccount::Base) => "Base",
            Some(LaunchAccount::Named { name: Some(_), .. }) => "Named{有名字}",
            Some(LaunchAccount::Named { name: None, .. }) => "Named{只有目录}",
        }
    }
    let shapes: [Option<&LaunchAccount>; 4] = [
        None,
        Some(&base),
        Some(&named_with_name),
        Some(&named_dir_only),
    ];
    // 反重复：四格必须**互不相同**，否则「四格都喂过了」是假的
    //（例：两格都是 `Named{只有目录}` ⇒ 有一种形状根本没被喂过，而条数照样是 4）。
    let labels: Vec<&str> = shapes.iter().map(|a| label_of(*a)).collect();
    let mut uniq = labels.clone();
    uniq.sort_unstable();
    uniq.dedup();
    assert_eq!(
        uniq.len(),
        labels.len(),
        "分母这张表里有两格是同一种形状 ⇒ 有一种形状没被喂过。实得：{labels:?}"
    );

    // 结果**按标签取**，不按下标取 —— 下标取法在 `shapes` 顺序一变时会悄悄换一格来断，
    // 那是一次静默的假读数（本区最贵的那族）。取不到就 `panic`，不会空转。
    let verdicts: Vec<(&str, Result<String, String>)> = shapes
        .iter()
        .map(|acct| {
            (
                label_of(*acct),
                render_local_ccm_with(&act, None, *acct, Some("s1abcdef-cc"), &caps, true),
            )
        })
        .collect();
    let verdict = |label: &str| -> &Result<String, String> {
        &verdicts
            .iter()
            .find(|(l, _)| *l == label)
            .unwrap_or_else(|| panic!("分母里没有 `{label}` 这一格 —— 下面那条断言在空转"))
            .1
    };

    // ① 缺席 —— 🔴 **`K-R89` 09-13：这一格翻面了。**
    //    它此前是 `Err` 且理由点着「继承」，依据是头注那张三说法对照表的最后一栏
    //    「省略 `--account` ⇒ 落 manifest 默认号 ⇒ 同样是静默换号」。
    //    **那一栏今天不是「病灶」了** —— 用户 09-12 `R28` 逐字裁「不是有选默认账号吗?
    //    **就用那个**」，并且 `plan.rs::resolve_account` 已经按两支落地（`-z` 闸 ＋ 默认号）。
    //    ⇒ 缺席这一格现在必须 **`Ok`**，且渲染出来的那一串里**两个账号 flag 都不许有**。
    let r = verdict("缺席");
    let cmd = r.as_ref().unwrap_or_else(|e| {
        panic!(
            "「参数缺席」= 继承环境，`R28` 之后它渲染得出来（省略 = `plan.rs::resolve_account`\n\
                 的两支：有继承态就继承 · 裸终端落 manifest 默认号，两支都是那一裁要的）。\n\
                 若它又短路了，先回 `DECISIONS.md#R28` 确认那一裁是不是被推翻，别在这里加闸。\n\
                 实得降级理由：{e}"
        )
    });
    assert!(
        !cmd.contains("--base"),
        "缺席被渲染成 `--base` —— 那是把「继承」偷换成「显式清空」（#75）。实得：{cmd}"
    );
    assert!(
        !cmd.contains("--account"),
        "缺席被渲染成 `--account <某号>` —— 那是替调用方挑了一个号，\n\
             与 `R28` 逐字「不许把调用方选中的号静默换掉」反向。实得：{cmd}"
    );

    // ② Base —— `Ok`，而且渲染出来的那条真的带 `--base`。
    let r = verdict("Base");
    let cmd = r.as_ref().expect("账号 0 是本机一直渲染得出来的那一格");
    assert!(
        cmd.contains("--base"),
        "账号 0 必须显式 `--base`，实得：{cmd}"
    );

    // ③ Named{有名字} —— **本件要开的就是这一格**：`Ok`，且带 `--account z`。
    let r = verdict("Named{有名字}");
    let cmd = r.as_ref().unwrap_or_else(|e| {
        panic!(
            "具名账号**说得出名字**时必须渲染得出来 —— 说不出来就意味着盘上四个本机拉起入口里\n\
                 那三个（`src/tabs.ts` · `src/views/history.ts` 两处）在类型上到不了后端那条路。\n\
                 实得降级理由：{e}"
        )
    });
    assert!(
        cmd.contains("--account z"),
        "具名账号该渲染成 `--account <名字>`，实得：{cmd}"
    );

    // ④ Named{只有目录} —— 仍然 `Err`：**不许从目录名推一个 `--account` 出来**。
    //    推错的失效方向是 `ccm` 当场 `die`（退出码 2）= 一次能起的会话变成报错，
    //    与 `apikey_account_id_of_dir` 那条「推错就回落」的保守方向**相反**。
    let r = verdict("Named{只有目录}");
    assert!(
        r.as_ref().is_err_and(|e| e.contains("account")),
        "只有目录没有名字时必须诚实短路（§35），**不许拿目录名当 `--account`**。实得：{r:?}"
    );
}

// ═══════════════════════════════════════════════════════════════════════
// `K-R89` `KR89D1`：**六格的今天版** —— 一张由行为驱动的表，不是一段散文
// ═══════════════════════════════════════════════════════════════════════

/// 一格今天是什么。**三值，别加第四个而不同时给它一条驱动**（下面那个 `match` 会逼你）。
#[cfg(not(windows))]
#[derive(PartialEq, Eq, Debug, Clone, Copy)]
pub(crate) enum CellToday {
    /// 后端那条路今天**渲染得出来** —— 这一格关了。
    Closed,
    /// 今天仍落回旧路，而**挡路的那样东西说得出名字**（第四列就是它）。
    StillFallsBack,
    /// **结构性** —— 不是欠账。翻它要先翻一条定框，不是补一段代码。
    Structural,
}

/// 🔴🔴 **六格今天版。`parity_ledger.rs` 的 `launch.render-payload` 那一行点的就是这六格。**
///
/// `(格名, 今天是什么, 现打的说法, 缺什么 / 谁能关它)`
///
/// # 它与账本那一行的分工（别读成两份清单）
///
/// 账本那一行是**散文**，它自己记过两次「改了行为没回来改理由」的前科。
/// 本表是**同一件事的可执行版**：下面那条判据把每一格**真去驱动一遍**，
/// 观测到的状态与本表第二列不符 ⇒ **当场红**。
/// ⇒ 「改一格的行为而不改它的说法」在这里做不到 —— 那正是本表存在的理由。
///
/// # ⚠ 本表买不到什么（诚实边界，别读大）
///
/// - **第三、四列（那两段话）真不真，机器判不了。** 本表钉的是「第二列 == 现打」，
///   以及「有人改了行为就必须回来动这张表」。**一段读着有道理的假理由照样过得去。**
/// - **它不是全部降级面**：它只装 `parity_ledger` 那一行点名的这六格。别的降级理由
///   （例：`--base` 那条逃生口、`send-into` 的 #76 防线）不在本表人群里。
/// - **Windows 那一格在本树上量不到运行时行为**（本模块整个挂 `#[cfg(not(windows))]`）
///   ⇒ 它的观测是**源码级**的，如实写在驱动里。
///
/// # 🔴 `K-R89` 09-13 改了哪一格、为什么（这一段是本轮唯一的行为改动）
///
/// 「账号未表态（继承）」从 `StillFallsBack` 翻成 `Closed`。翻它的**不是本件的判断**，
/// 是 `DECISIONS.md#R28`（用户 09-12 亲裁）＋ 它在
/// `src/backend/control/ccm/plan.rs::resolve_account` 上的落地。
/// 盘上原来有一句陈账逐字写着「③ 那一格要动的是 ccm 省略时的默认语义（**产品决定**
/// ＋ `plan.rs`）」—— **它在等一个 09-12 就到了的决定**，本轮一并撤掉。
#[cfg(not(windows))]
pub(crate) const THE_SIX_WAYS_THE_OLD_PATH_STILL_WINS: &[(&str, CellToday, &str, &str)] = &[
    (
        "账号未表态（继承）",
        CellToday::Closed,
        "🔴 `K-R89` 09-13 关掉的就是这一格。`CliAccount::Inherit` 渲染成「一个账号 flag 都不加」；\
             省略在 `ccm` 上有确定语义（`plan.rs::resolve_account` 两支：`CLAUDE_CONFIG_DIR` 非空 ⇒ \
             保留不覆盖〔`R08` 的 `-z` 闸〕· 裸终端 ⇒ 落 manifest `isDefault`），两支都是 `R28` 要的。",
        "已关。⚠ **只关了本机那半** —— 远端是 ssh 过去、那台机器上的继承态不是 monitor 的环境\
             （`R28` 裁定四逐字）⇒ `WireAccount` 刻意没有对应变体，远端那半归 `K-R90`。",
    ),
    (
        "只说得出目录没名字",
        CellToday::StillFallsBack,
        "`LaunchAccount::Named{name: None}` ⇒ `CliAccount::Named{name: None}` ⇒ 账号维度\
             `cli_flags` 回 `None` ⇒ §35 整条降级。理由是「说不出」，不是「不想说」。",
        "缺的是**名字这条信息本身**，不是 CLI 语法 —— 上游（`accounts.ts` 那个取值口）\
             说得出名字的那天它自己就关了。🔴 **不许从目录名推一个 `--account` 出来**：\
             推错的失效方向是 `ccm` 当场 `die`（rc=2），一次能起的会话变成报错。",
    ),
    (
        "没有 tmux 名",
        CellToday::StillFallsBack,
        "`NO_TMUX_NAME` —— 名字只许 `remote-launch.ts::mintTmuxName` 铸（F13 那个撞名坑），\
             Rust 这侧不许补默认值。⚠ **这一格今天是半开的**（现打 09-13）：resume 那条\
             前端已接线（〔FE1〕本机 resume 只剩一份编排 `local-resume.ts::resumeLocalSession`，名字经 `tmux-name-mint.ts` 铸，\
             人群由 `tests/ipc/commands.vitest.ts` 那条「每处 `resume_history_session` 都带 `tmuxName`」钉着）；\
             而 `new_local_session` 的 Rust 签名里**根本没有 `tmux_name` 这一格** ⇒ 起新会话恒短路。",
        "给 `new_local_session` 加一个名字参数 ＋ 前端在那条路上也过一次铸造口。\
             ⚠ 那要动 `src/ipc/commands.ts` 与两个调用点，**不在 `K-R89` 的写区里**。",
    ),
    (
        "这个号走中转",
        CellToday::StillFallsBack,
        "`launch_local` 里那行 `relay.is_empty()` **显式**保住的互斥 —— 不是渲染器拒的\
             （渲染器单独看已经不再互斥，`K-R53` 开的那一格）。常量是 `RELAY_KEEPS_THE_OLD_PATH`。",
        "退役条件 `K-R61` 已经收成**一行 Rust**（把 `relay.is_empty()` 换成「探到 \
             `base-url-across-tmux` 才放行」），但它的前置是「有人守住『用户机器上跑的 `ccm` \
             就是 app 自己推的那一份』」—— 那一格今天没人守。⚠ 互斥这条性质本身由邻居\
             那条判据钉，本行只记「这一格今天关没关」。",
    ),
    (
        "这台机没装 ccm",
        CellToday::StillFallsBack,
        "`render_ccm_invocation` 的第一行 `if !installed { NotInstalled }`。\
             探测走 `CcmProbeSource` 那条缝（本判据喂确定值，**不问跑它的这台机器**）。",
        "**部署面，不是渲染器的欠账**（`K27`/`K34`：部署是产品的一部分，由客户端做）。\
             远端那条装法 `sftp::deploy_remote_backend`（〔MC1〕部署后端连同 `ccm` 入口）今天就在盘上；本机那条归部署向导。",
    ),
    (
        "Windows",
        CellToday::Structural,
        "`render_local_ccm` / `render_local_ccm_with` 整段挂 `#[cfg(not(windows))]` ⇒ \
             Windows 上那条路**在编译期就不存在**；`launch_local` 的 `#[cfg(windows)]` 那一支\
             连 `tmux_name` 都不读（读了就是给「Windows 也进容器」开口子）。",
        "**不是欠账**：定框 `C12`〔用 08-12〕逐字「windows不要tmux」。要翻它先回去翻定框。",
    ),
];

/// 🔴🔴 `KR89D1`：**六格逐格现打，观测到的与表上写的不一样就红。**
///
/// # 每一格怎么观测的（写在这里，别让读的人去猜）
///
/// 五格靠**真去驱动生产函数**（`render_local_ccm_with` / `launch_local`），
/// 第六格（Windows）在本树上跑不到运行时，观测是**源码级**的 —— 逐条写在 `observe` 里。
///
/// # ⚠ 与邻居 [`a_launch_that_goes_through_the_relay_still_cannot_get_a_tmux_container`] 的分工
///
/// 「走中转」那一格两处都会驱动一次 `launch_local`，而**它们量的不是两把尺子**：
/// 同一个观测口（[`LaunchSink`] 那条缝上真正交出去的那一串）、同一个判定
/// （串里有没有 `--ccm-tmux=`）。差别在**结论**：邻居主张的是「两个集合不相交」（互斥），
/// 本条只记「这一格今天关没关」。⇒ 谁哪天把中转那一行翻掉，**两条一起红**，
/// 而它们要求的后续动作不同（邻居要重裁互斥，本条要改表）。
#[test]
#[cfg(not(windows))]
fn every_one_of_the_six_cells_is_measured_not_narrated() {
    // 反空真 ①：表得有六行，且**格名互不相同**（重名 ⇒ 有一格根本没被观测过，而条数照样对）。
    assert_eq!(
        THE_SIX_WAYS_THE_OLD_PATH_STILL_WINS.len(),
        6,
        "账本 `launch.render-payload` 那一行点的是六格。加/删一格 ⇒ 同拍改账本那一行，\
             并回来给新格写一条驱动。"
    );
    let mut names: Vec<&str> = THE_SIX_WAYS_THE_OLD_PATH_STILL_WINS
        .iter()
        .map(|(n, ..)| *n)
        .collect();
    let n_all = names.len();
    names.sort_unstable();
    names.dedup();
    assert_eq!(
        names.len(),
        n_all,
        "表里有两行是同一个格名 ⇒ 有一格没被观测过"
    );

    // 反空真 ②：三值**至少两值有人占**。全是同一个值时，下面那条相等断言退化成
    // 「所有格都一样」——那时把某一格的行为翻掉、再把整列一起改，读起来仍然全绿。
    let mut kinds: Vec<CellToday> = THE_SIX_WAYS_THE_OLD_PATH_STILL_WINS
        .iter()
        .map(|(_, v, ..)| *v)
        .collect();
    kinds.sort_by_key(|k| format!("{k:?}"));
    kinds.dedup();
    assert!(
        kinds.len() >= 2,
        "六格今天是同一个状态（{kinds:?}）—— 先确认这是真的；\
             真是真的话，本条那条相等断言此刻买不到「逐格」，请改形状。"
    );

    for (name, want, say, need) in THE_SIX_WAYS_THE_OLD_PATH_STILL_WINS {
        let got = observe_one_cell(name);
        assert_eq!(
            got, *want,
            "\n★ 六格表第「{name}」格：**现打是 {got:?}，表上写的是 {want:?}**。\n\
                 ⇒ 有人改了这一格的行为，而没有回来改它的说法 —— 那正是这张表存在的理由。\n\
                 表上今天写着：{say}\n\
                 表上今天说缺什么：{need}\n\
                 ⚠ 改表的同时把 `parity_ledger.rs` 的 `launch.render-payload` 那一行一起读一遍：\
                 两处说的是同一件事。"
        );
    }
}

/// 六格各自的观测口。**一个 `match`，认不出的格名当场 `panic`** ——
/// 加一格却不给它驱动时，上面那条判据不会静默少测一格。
#[cfg(not(windows))]
fn observe_one_cell(name: &str) -> CellToday {
    let act = LocalPsAction::Resume("s1".into());
    let caps = caps_of_a_current_ccm();
    const TMUX: &str = "s1abcdef-cc";
    let dir_only = LaunchAccount::Named {
        config_dir: "/home/u/.claude-alt/z".into(),
        name: None,
    };
    // 纯函数半的观测：渲染得出来 = 这一格关了。
    let pure = |acct: Option<&LaunchAccount>, tmux: Option<&str>, installed: bool| {
        if render_local_ccm_with(&act, None, acct, tmux, &caps, installed).is_ok() {
            CellToday::Closed
        } else {
            CellToday::StillFallsBack
        }
    };
    match name {
        // ① 未表态 —— `R28` 之后渲染得出来。
        "账号未表态（继承）" => pure(None, Some(TMUX), true),
        // ② 只有目录 —— §35 短路。
        "只说得出目录没名字" => pure(Some(&dir_only), Some(TMUX), true),
        // ③ 没有 tmux 名 —— 名字只许铸造口产，Rust 侧不补默认值。
        //    ⚠ 喂 `Base`（一个**确定渲染得出来**的账号形状）⇒ 这一格观测到的
        //    「拒」只可能是名字那一维造成的，不会与账号那一维混在一起。
        "没有 tmux 名" => pure(Some(&LaunchAccount::Base), None, true),
        // ④ 没装 ccm —— 探测结果由参数喂，**不问跑它的这台机器**。
        "这台机没装 ccm" => pure(Some(&LaunchAccount::Base), Some(TMUX), false),
        // ⑤ 走中转 —— 这一格不在纯函数半里（闸在 `launch_local` 体内那行
        //    `relay.is_empty()`）⇒ 必须真跑一趟拉起，量**交出去的那一串**。
        "这个号走中转" => {
            let acct = LaunchAccount::Named {
                config_dir: "/home/u/.claude-alt/acct-a".into(),
                name: Some("acct-a".into()),
            };
            fn not_win() -> bool {
                false
            }
            fake_table(&["acct-a"], true);
            let _facts = override_inject_facts(InjectFactSources {
                endpoint: fake_ep,
                windows: not_win,
                all_sessions: all_sessions_off,
            });
            fn a_current_ccm() -> crate::ccm_probe::CcmProbeResult {
                crate::ccm_probe::CcmProbeResult {
                    installed: true,
                    version: Some("0.0.0-判据替身".to_string()),
                    capabilities: caps_of_a_current_ccm().into_iter().collect(),
                    build: None,
                }
            }
            let _probe = override_ccm_probe(CcmProbeSource(a_current_ccm));
            thread_local! {
                static SEEN: std::cell::RefCell<Vec<String>> =
                    const { std::cell::RefCell::new(Vec::new()) };
            }
            fn recorder(cmd: &str, _cwd: Option<&str>) -> Result<(), String> {
                SEEN.with(|v| v.borrow_mut().push(cmd.to_string()));
                Ok(())
            }
            let _sink = override_launch_sink(LaunchSink(recorder));
            launch_now(&act, None, None, Some(&acct), Some(TMUX))
                .expect("走中转这一趟拉起本身不该失败");
            let sent = SEEN.with(|v| {
                v.borrow()
                    .last()
                    .cloned()
                    .expect("这一趟什么都没送出去 —— 观测口坏了，读数作废")
            });
            // 自检：这一趟**真的**走了中转（否则下面那个判定量的是另一件事）。
            assert!(
                sent.contains("ANTHROPIC_BASE_URL"),
                "这一趟没拿到中转前缀 —— 替身没生效，本格此刻在量别的东西。实得：{sent}"
            );
            if sent.contains("--ccm-tmux=") {
                CellToday::Closed
            } else {
                CellToday::StillFallsBack
            }
        }
        // ⑥ Windows —— 本模块整个挂 `#[cfg(not(windows))]`，跑不到那一支的运行时。
        //    ⇒ 观测是**源码级**的，如实写清它量的是什么：
        //      · `launch_local` 的 `#[cfg(windows)]` 那一支里有 `let _ = tmux_name;`
        //        （逐字：连读都不读，读了就是给「Windows 也进容器」开口子）；
        //      · 渲染器那一半挂着 `#[cfg(not(windows))]`（编译期就不给 Windows）。
        //    两条**都**成立才算「结构性」；少一条就说明有人开了口子。
        "Windows" => {
            let prod = guard_core::production_code(include_str!("../../src/bridge/src/history.rs"));
            let at = guard_core::find_pinned(&prod, "fn launch_local(").unwrap_or_else(|e| {
                panic!("`fn launch_local(` 不是恰好一处 —— 锚点坏了，本格读数作废：{e}")
            });
            let win_arm_keeps_out = prod[at..]
                .split_once("#[cfg(not(windows))]")
                .map(|(head, _)| head.contains("let _ = tmux_name;"))
                .unwrap_or(false);
            // ⚠ 带 `fn ` 前缀才认得出**定义**那一处 —— 不带的话第一处命中的是
            //   `render_local_ccm` 体内那次**调用**，而调用点上没有 cfg 属性。
            let renderer_is_posix_only = prod
                .find("fn render_local_ccm_with(")
                .map(|i| prod[..i].trim_end().ends_with("#[cfg(not(windows))]"))
                .unwrap_or(false);
            if win_arm_keeps_out && renderer_is_posix_only {
                CellToday::Structural
            } else {
                CellToday::Closed
            }
        }
        other => panic!(
            "六格表里多了一格「{other}」而没有人给它写观测口 —— \
                 加格与加驱动必须同一拍，否则那一格是**登记了但没量过**。"
        ),
    }
}

/// ★★★ `D4 阻-3`：**「走中转」与「有 tmux 容器」今天仍然互斥** —— 把这个事实钉住。
///
/// # 它为什么存在：盘上写着「已消掉」，而其实没消掉
///
/// 第一拍报过一条代价「走中转的号拿不到 tmux 容器」（当时的成因：外侧那句 export
/// 在 tmux 边界被吃掉）。第二拍照 `R08` 在那份已删的 bash `ccm` 里加了一条转发，于是件文件
/// 与 [`launch_local`] 的头注都写上了**「不再互斥」**。
/// 🔴 `D4` 现打证伪：**代价原样还在，只是成因换了。**
///
/// # 🔴🔴 `K-R53` 09-11 **重新裁定**：成因**第二次**换了，而互斥仍然成立
///
/// `D4` 那一拍的成因是「具名账号根本进不了 ccm」（`Named` 只有目录、说不出 `--account`）。
/// **本件把那一格开了**（[`LaunchAccount::Named::name`]）⇒ **那个成因今天不成立了**：
/// 下面第 ⓪ 格现打断言的正是这件事 —— 渲染器**单独看已经不再互斥**。
///
/// 今天互斥是由 [`launch_local`] 里那一行 `relay.is_empty()` **显式保住**的
/// （理由与退役条件住 [`RELAY_KEEPS_THE_OLD_PATH`]）。
///
/// ⚠ 〔`K-R61` 09-11〕这一段先前逐字写着「`capabilities=`（第 624 行，18 个 token）里
/// 没有任何 token 声明它 ⇒ 放行会让**装旧 ccm 的机器**静默吃掉」——
/// **那个住址与那个理由今天都不成立了**，重裁后的两句都住
/// [`RELAY_KEEPS_THE_OLD_PATH`] 的头注，本条不复述第二份。
///
/// ⇒ **这一条从「成因是说不出名字」改成「成因是那一行还没改成探那个能力」。**
/// 前者是结构性的（只能等改 `LaunchAccount`），后者**有可执行的退役条件**。
///
/// # 🔴🔴🔴 `K-R55` 09-11：**上一版自己抄了一份被测逻辑** —— 换成量真正送出去的那一串
///
/// 上一版的循环体逐字是：
/// ```text
/// let prefix = relay_prefix_for_launch(&act, acct).expect(…);
/// if !prefix.is_empty() { relayed.push(label); }
/// if prefix.is_empty() && renders(acct) { containered.push(label); }
/// ```
/// —— 那个 `prefix.is_empty() &&` **就是 [`launch_local`] 里那道闸的一份拷贝**。
/// ⇒ 它证的是自己那份拷贝，生产那一行翻不翻它都不知道。
/// PM 09-11 现打：把生产那行换成 `if true`，**点名单跑 1 passed**；
/// 实现方 09-11 在沙箱里复打了同一刀，**全量 `cargo --lib` 1472 passed / 0 failed**
/// （量于 `07e4e72` + 那一刀，镜像 `ccmon-devbox:latest`，`CARGO_TARGET_DIR=pm-targets/k-r55`）
/// —— 全仓**没有任何一条**判据对那一刀出声。
///
/// ⇒ 本条现在**真的驱动 [`launch_local`]**，两个集合都从
/// **[`LaunchSink`] 那条缝上收到的那个字符串**里读出来，一个字节的判断逻辑都不自带：
/// - 「走中转」= 那一串里有 `ANTHROPIC_BASE_URL`（中转前缀唯一的形状）；
/// - 「有容器」= 那一串里有 `--ccm-tmux=`（`render_ccm_invocation` 唯一产出它的地方；
///   回落路 `build_local_posix_command` 从不说 tmux）。
///
/// 「装没装 ccm」由 [`CcmProbeSource`] 那条缝喂进来（**不问跑判据的这台机器** ——
/// 沙箱里没装 ccm，不喂的话四格会一起落到回落路，那时本条又变成空真）。
///
/// # 今天的成因（本条逐格量出来，不是推的）
///
/// 分母 = [`LaunchAccount`] 的**全部形状**加上「参数缺席」，并且**具名那一格喂两个号**
/// （一个在apikey 表里、一个不在 —— 只喂一个的话「中转在不在场」这一维的取值域是 1，
/// 那正是 `D6 阻-2` 逮到过的形状）：
///
/// | 形状 | 送出去那一串带不带中转前缀 | 带不带 `--ccm-tmux=`（= [`launch_local`] 的判据） |
/// |---|---|---|
/// | 缺席（`None`） | 不带（不走中转） | **是**〔🔴 `K-R89` 09-13 翻的：`R28` 之后省略有确定语义，渲染器说得出「继承」了〕 |
/// | `Base`（账号 0） | 不带（不走中转） | **是** |
/// | `Named{acct-a}`（**在apikey 表里**） | 带 | 否 —— `relay.is_empty()` 那一行挡住 |
/// | `Named{acct-b}`（不在表里） | 不带 | **是**（`K-R53` 开的就是这一格） |
///
/// # ⚠ 它连带说明了一件别处的事（别让那条判据被读宽）
///
/// 我们自己这份 `ccm` 的容器路那条 `ANTHROPIC_BASE_URL` 转发（连同钉它的
/// `payload::tests::the_ccm_container_path_forwards_the_relay_base_url_across_the_tmux_boundary`
/// 与件文件里的 `M12`/`M12b`）量的是一条**在本机中转这条路上今天生产不可达**的路：
/// 那段 shell 真的会转发，而**没有任何生产输入能同时走到中转与容器**。
/// 它不是假的，它买不到本件要的那一格。**那条判据的头注里也写了这句话，两处别只改一处。**
///
/// # 🔴 这条前提**还会再变一次** —— 变的那天去哪里重新裁定（`testing.md` 三.11 要的那一栏）
///
/// 〔`K-R61` 09-11 **重裁**〕上一版这里写的是「退役条件今天是**一行 shell**」，
/// 点的是那份 bash `ccm` 的第 624 行 —— 而它 `07e4e72` 就删了。
/// 今天的前提是：**转发做到了、也声明了**（`base-url-across-tmux` 已在
/// `src/backend/control/ccm/mod.rs` 的 `CAPABILITIES` 里，
/// 由那棵树的 `the_base_url_token_is_declared_because_the_tmux_path_really_forwards_it`
/// 真去驱动一遍），**差的只是 [`launch_local`] 那一行还没改成探它**。
///
/// ⇒ 退役条件收成**一行 Rust**：[`launch_local`] 里那句 `relay.is_empty()`
/// 换成「探到 `base-url-across-tmux` 才放行」。真做那一天，**同一拍**这几样：
///   ① 本条会红 —— **在这里重新裁定**；
///   ② [`launch_local`] 的头注与 [`RELAY_KEEPS_THE_OLD_PATH`] 跟着改；
///   ③ 件计划 `K-H2b §4` 那条登记跟着改
///      〔`K-R53` 09-11 报回 PM，**`K-R61` 仍未做**：`K-R61 §0e` 逐字裁「不碰它」〕；
///   ④ `CAPABILITIES` 那个 token —— **`K-R61` 已做**，这一样从此不再是待办。
#[test]
#[cfg(not(windows))]
fn a_launch_that_goes_through_the_relay_still_cannot_get_a_tmux_container() {
    let act = LocalPsAction::Resume("s1".into());
    let base = LaunchAccount::Base;
    // 在apikey 表里的那个号 —— 名字说得出（本件之后前端就是这么传的）。
    let acct_a = LaunchAccount::Named {
        config_dir: "/home/u/.claude-alt/acct-a".into(),
        name: Some("acct-a".into()),
    };
    // 不在apikey 表里的那个号 —— 「哪个号」这一维的取值域因此是 2，不是 1（`D6 阻-2`）。
    let acct_b = LaunchAccount::Named {
        config_dir: "/home/u/.claude-alt/acct-b".into(),
        name: Some("acct-b".into()),
    };

    // 中转事实由替身给：那台后端的表里只有 acct-a、中转在听、不是 Windows。
    fn not_windows() -> bool {
        false
    }
    fake_table(&["acct-a"], true);
    let _guard = override_inject_facts(InjectFactSources {
        endpoint: fake_ep,
        windows: not_windows,
        all_sessions: all_sessions_off,
    });

    // 「这台机器装没装 ccm」也由替身给 —— 本条**不问跑它的那台机器**
    //（沙箱里没装，不喂的话四格一起落到回落路 ⇒ 本条变成空真）。
    fn a_current_ccm() -> crate::ccm_probe::CcmProbeResult {
        crate::ccm_probe::CcmProbeResult {
            installed: true,
            version: Some("0.0.0-判据替身".to_string()),
            capabilities: caps_of_a_current_ccm().into_iter().collect(),
            build: None,
        }
    }
    let _probe = override_ccm_probe(CcmProbeSource(a_current_ccm));

    // 送法替身：本条量的是 [`launch_local`] **真正交出去的那一串**，不是源码、
    // 也不是本条自己再算一遍的什么东西。
    thread_local! {
        static SENT: std::cell::RefCell<Vec<String>> =
            const { std::cell::RefCell::new(Vec::new()) };
    }
    fn recorder(cmd: &str, _cwd: Option<&str>) -> Result<(), String> {
        SENT.with(|v| v.borrow_mut().push(cmd.to_string()));
        Ok(())
    }
    let _sink = override_launch_sink(LaunchSink(recorder));

    // 容器名由调用方给（`mintTmuxName` 那一侧的事），这里只要一个合法的名字。
    const TMUX: &str = "s1abcdef-cc";

    // ⓪ **重新裁定的那一格**：渲染器**单独看**已经不再互斥了 ——
    //    在apikey 表里的那个号，渲染器今天渲得出来。互斥不再由它保。
    //    （这一格红 = `K-R53` 那一刀被退掉了，那时下面几格的理由也就不成立。）
    //    ⚠ 走的是**生产那个渲染器** [`render_local_ccm`]（探测经缝喂），
    //    不是它的纯函数半 —— 后者会把「生产上探测这一跳还在不在」漏在射程外。
    assert!(
        render_local_ccm(&act, None, Some(&acct_a), Some(TMUX)).is_ok(),
        "渲染器又对具名账号短路了 —— 那是 `K-R53` 之前的形状，\n\
             本条头注里那段「成因换成探不到能力」就不再成立，回去重新裁定。"
    );

    let mut relayed = Vec::new();
    let mut containered = Vec::new();
    let shapes: [(&str, Option<&LaunchAccount>); 4] = [
        ("缺席", None),
        ("Base", Some(&base)),
        ("Named{acct-a·在表里}", Some(&acct_a)),
        ("Named{acct-b·不在表里}", Some(&acct_b)),
    ];
    for (label, acct) in shapes {
        // 🔴 **真去走生产那条路** —— 上一版在这里自己抄了一份 `launch_local` 的闸
        //    （见头注 `K-R55` 那一节），于是把生产那一行翻掉一个字都不响。
        launch_now(&act, None, None, acct, Some(TMUX))
            .unwrap_or_else(|e| panic!("形状 {label} 这一趟拉起本身就失败了：{e}"));
        let sent = SENT.with(|v| {
            v.borrow()
                .last()
                .cloned()
                .unwrap_or_else(|| panic!("形状 {label} 这一趟什么都没送出去"))
        });
        // 两个判定都只看**那一串**：`ANTHROPIC_BASE_URL` 只可能来自中转前缀，
        // `--ccm-tmux=` 只可能来自 `render_ccm_invocation`（回落路从不说 tmux）。
        if sent.contains("ANTHROPIC_BASE_URL") {
            relayed.push(label);
        }
        if sent.contains("--ccm-tmux=") {
            containered.push(label);
        }
    }
    // 反空真：两边**都非空**（都空的话下面那条不相交是空真）。
    assert_eq!(
        relayed,
        ["Named{acct-a·在表里}"],
        "真拿到中转前缀的形状变了 —— 本条的结论要重新裁定（见头注最后一节）"
    );
    assert_eq!(
        containered,
        ["缺席", "Base", "Named{acct-b·不在表里}"],
        "能走进 ccm 容器的形状变了 —— 本条的结论要重新裁定（见头注最后一节）。\n\
             〔`K-R89` 09-13：「缺席」是本轮新进来的一格 —— `R28` 之后省略 `--account` \
             有确定语义，渲染器不再对它短路。**互斥那条结论没变**，变的是分母。〕"
    );
    // 正题：两个集合不相交 ⇒ 今天没有任何一次本机拉起同时拿到中转前缀与 tmux 容器。
    assert!(
        relayed.iter().all(|l| !containered.contains(l)),
        "「走中转」与「有 tmux 容器」不再互斥了 —— 那是**好事**，但盘上有三处话要跟着改：\n\
             ① 本条（重新裁定）② `launch_local` 头注与 `RELAY_KEEPS_THE_OLD_PATH`\n\
             ③ 件计划 `K-H2b §4` 那条登记。\n\
             （第四样 —— `src/backend/control/ccm/mod.rs` 的 `CAPABILITIES` 加\n\
             `base-url-across-tmux` —— `K-R61` 已经做了：转发做到了、也声明了。）\n\
             实得：走中转的 {relayed:?} · 有容器的 {containered:?}"
    );
}

// ═══════════════════════════════════════════════════════════════════════
// `K-R61`：退役条件那句话 —— **几处说的是同一件事**，而且**它点名的住址真的在盘上**
// ═══════════════════════════════════════════════════════════════════════

/// 本组判据的**自剪线**。见 [`r61_hay`]。
///
/// ⚠ 这个串在本文件里**必须只出现在这一行**（下面两条判据都靠它切被测面）。
const R61_SELF_CUT: &str = "〔K-R61 判据组自剪线〕";

/// 本组的被测面 = `history.rs` 全文**截到自剪线为止**。
///
/// 🔴 为什么要剪：本组的锚点是**逐字串**，而它们在下面两条判据里各有一份字面量副本
/// （判据自带清单，与 `ccm_invocation.rs` 那两处「刻意的重复」同一个理由）。
/// 不剪的话 [`guard_core::find_pinned`] 会看到两处、当场报「指不明是哪一处」——
/// 那是**量具把自己也算进了被测面**。
///
/// ⚠ **它买不到的**：自剪线**之后**的文本一律不进射程。有人把同一段话复制到本文件
/// 更后面去，本组看不见。射程边界就写在这里，别读宽。
///
/// # 🔴 〔搬树 2026-09-18 · `设计/16 §5.4b` 纪律 3〕**被测面是两份文件，不是一份**
///
/// 那句退役条件今天**跨在剖分线两侧**：住址表 `r61_sites()` 的 ①②③ 住生产段
/// （`src/bridge/src/history.rs`），而 ④⑤ 是**测试段里的两段话**
/// （互斥判据的头注 · 它末尾那条 `assert!` 的诊断文案），剖分之后住本文件。
/// ⇒ 只喂生产段那一份的话，④⑤ 两个锚点一个都找不到。
///
/// ⚠ **自剪线仍然非留不可**，只是它剪的对象换了半边：`r61_sites()` 里每个锚点都有
/// 一份**字面量副本**，而那张表今天住本文件 ⇒ 不剪的话 `find_pinned` 会看到两处。
/// 本文件里自剪线**之前**没有任何一份锚点副本（现打：①②③ 只在表里出现，
/// ④⑤ 各只在它们真正那一处出现）。
fn r61_hay() -> &'static str {
    static HAY: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    HAY.get_or_init(|| {
        let prod = include_str!("../../src/bridge/src/history.rs");
        // ★ 反空真：生产段那一份真读到了（空串会让 ①②③ 三个锚点一起「找不到」，
        //   而那与「那几段话被删了」在输出上一模一样）。
        assert!(
            prod.len() > 10_000,
            "只读到 {} 字节的 `history.rs` —— 本组判据在空转",
            prod.len()
        );
        let mine = include_str!("history_tests.rs");
        let cut = mine
            .find(R61_SELF_CUT)
            .expect("自剪线不见了 —— 本组判据此刻在量它自己，读数作废");
        format!("{prod}\n{}", &mine[..cut])
    })
}

/// 「退役条件那句话」在本文件里的**住址表 —— 只有这一处**。
///
/// 每一处给一对**逐字锚点**（起 / 止）。两个锚点都由 [`guard_core::find_pinned`]
/// 断言**恰好命中一次**，取「起 → 止」之间那一段 ⇒ 窗口**不可能跨到下一条**：
/// 止锚点就是紧挨着它的下一个结构物本身。
///
/// ⚠ `K-R61 §0a` 那张表登记的是**四处**；本轮现打**五处**。多出来的两处是
/// ③（[`launch_local`] 体内那段「为什么要显式保住」）与
/// ⑤（互斥判据末尾那条 `assert!` 的诊断文案）—— 它们也在说同一件事，`§0a` 漏了。
/// 数字与名单同住这里（纪律 ⑭）：分母 = 本表的长度，成员 = 本表逐行。
fn r61_sites() -> Vec<(&'static str, &'static str, &'static str)> {
    vec![
        (
            "① 常量本体 RELAY_KEEPS_THE_OLD_PATH",
            "const RELAY_KEEPS_THE_OLD_PATH: &str =",
            "/// ⚠ **`Err` 那一支不回 token**",
        ),
        (
            "② 常量头注的『退役条件』一节",
            "# 🔴 退役条件〔`K-R61` 09-11 重裁",
            "const RELAY_KEEPS_THE_OLD_PATH: &str =",
        ),
        (
            "③ launch_local 体内『为什么要显式保住』",
            "🔴🔴🔴 **三次订正（`K-R61` 09-11）",
            "let rendered = if relay.is_empty() {",
        ),
        (
            "④ 互斥判据头注『这条前提还会再变一次』",
            "# 🔴 这条前提**还会再变一次**",
            "fn a_launch_that_goes_through_the_relay_still_cannot_get_a_tmux_container() {",
        ),
        (
            "⑤ 互斥判据末尾那条 assert! 的诊断文案",
            "「走中转」与「有 tmux 容器」不再互斥了",
            "实得：走中转的 {relayed:?}",
        ),
    ]
}

/// 按住址表切出那几段。锚点唯一性在这里当场核（切之前，不是切之后）。
fn r61_segments() -> Vec<(&'static str, &'static str)> {
    let hay = r61_hay();
    r61_sites()
        .into_iter()
        .map(|(name, start, end)| {
            let a = guard_core::find_pinned(hay, start).unwrap_or_else(|e| {
                panic!("{name}：起锚点不是恰好一处 —— 形状变了，先修锚点：{e}")
            });
            let b = guard_core::find_pinned(hay, end).unwrap_or_else(|e| {
                panic!("{name}：止锚点不是恰好一处 —— 形状变了，先修锚点：{e}")
            });
            assert!(
                a < b && b - a < 4000,
                "{name}：切出来的窗口不成形（起 {a} 止 {b}）—— 两个锚点的相对位置变了，\n\
                     照原样切会切到别人身上，本条此刻的读数一律作废。"
            );
            (name, &hay[a..b])
        })
        .collect()
}

/// 从一段文本里抽出「反引号括起来、像**仓内路径**的那些串」。
///
/// 🔴 **刻意不要求后缀**：`K-R61 §0d` 那把尺子的 `EXT` 白名单正是把**无后缀**的那一形
/// 整个滤掉了，于是它一处都数不到本件正在治的那个样本 ——「量一个人群之前，
/// 先确认尺子逮得到那个已知的样本」。这里只要求：带 `/` · 不是 URL · 不带空格 ·
/// 只由路径字符组成 · 不是绝对路径。行号后缀（`:123` / `:12-34`）当场剥掉。
fn r61_paths_in(seg: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = seg;
    while let Some(a) = rest.find('`') {
        let after = &rest[a + 1..];
        let Some(b) = after.find('`') else { break };
        let raw = after[..b].trim();
        rest = &after[b + 1..];
        let tok = match raw.rsplit_once(':') {
            Some((head, tail))
                if !tail.is_empty() && tail.chars().all(|c| c.is_ascii_digit() || c == '-') =>
            {
                head
            }
            _ => raw,
        };
        if !tok.contains('/') || tok.contains("://") || tok.contains(' ') {
            continue;
        }
        if tok.starts_with('/') || tok.starts_with('~') || tok.starts_with("./") {
            continue;
        }
        if !tok
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "._/+-".contains(c))
        {
            continue;
        }
        out.push(tok.to_string());
    }
    out
}

/// `KR61D1`：**那几处说的是同一件事** —— 同一个前提、同一个住址、同一个 token。
///
/// # 它为什么存在
///
/// 上一版这几处逐字点着一份 `07e4e72` 就删掉的 bash 脚本，而互斥那条判据一直是绿的
/// ⇒ **判据活着、前提死了，中间没有任何东西会响**。本条就是那个「会响的东西」。
///
/// # 判的是什么（三条，缺一不可）
///
/// 1. **同一个前提**：每一处都要有那句承重话（`转发做到了、也声明了`）。
///    ⇒ 只把住址换新、把理由留在旧版本上（`K-R61 KR61D1` 逐字点名的失效方向
///    「**换地址不换前提**」）在这里当场红。
/// 2. **同一个住址**：每一处点的都是 [`R61_ADDR`]，而它**在盘上真的存在**
///    （存在性那一半由 [`every_address_the_retirement_condition_names_is_still_on_disk`] 守）。
/// 3. **旧住址一处都不许留**：那份已删的 bash 脚本的旧路径，五段里出现一次就红。
///    ⇒ `KR61D1` 那条死值验（「四处中任意一处改回旧住址 ⇒ 必须红」）由这一条兑现。
///
/// # ⚠ 边界（别读宽）
///
/// - 本条判的是**这几段文本互相一致**，**不判**「这段话是真的」。
///   「那个文件里真的有那个 token」由后端那棵树的
///   `control::ccm::tests::the_base_url_token_is_declared_because_the_tmux_path_really_forwards_it` 守。
/// - 住址表本身（哪几处算「同职」）是**人写的**。有人在别处再写一段同职的话而不登记，
///   本条看不见 —— 那正是 `§0a` 漏掉 ③⑤ 两处的形状。
#[test]
fn the_retirement_condition_says_the_same_thing_in_every_place_that_states_it() {
    // 判据自带清单（**不复用生产常量**）：复用的话，谁把生产那一份改了，
    // 循环跟着改，两边一起漂而没有一格红。
    const PREMISE: &str = "转发做到了、也声明了";
    const TOKEN: &str = "base-url-across-tmux";

    let segs = r61_segments();
    assert_eq!(
        segs.len(),
        5,
        "住址表的长度变了 —— 分母变了就要重新裁定，别让它悄悄变"
    );
    // 旧住址：**现搭**，不写成字面量。写成字面量的话本文件里就又多了一处
    // 「那个已删文件的路径」，而本条自己就是来消灭它的。
    let retired = format!("shared/{}", "ccm");

    for (name, seg) in &segs {
        assert!(
            seg.contains(PREMISE),
            "{name} 里没有那句承重话「{PREMISE}」。\n\
                 ⇒ 这正是 `KR61D1` 点名的失效方向：**换地址不换前提**。\n\
                 今天的前提是「我们自己这份 ccm 转发得了、也声明了，差的只是那一行还没改成探它」，\n\
                 不是旧话「对面可能是装了别的 ccm 的机器」（`K34`/`K35` 之后那类机器正在退场）。\n\
                 实得这一段：\n{seg}"
        );
        assert!(
            seg.contains(R61_ADDR),
            "{name} 点的住址不是 `{R61_ADDR}` —— 几处不再指同一个地方。\n\
                 实得这一段：\n{seg}"
        );
        assert!(
            seg.contains(TOKEN),
            "{name} 里没点名那个 token `{TOKEN}` —— 退役条件说不清要探什么。\n\
                 实得这一段：\n{seg}"
        );
        // 旧住址一处都不许留（`-aliases.sh` 那个**还在盘上**，不算）。
        let stale = seg
            .match_indices(retired.as_str())
            .filter(|(i, _)| {
                seg[i + retired.len()..]
                    .chars()
                    .next()
                    .is_none_or(|c| c != '-')
            })
            .count();
        assert_eq!(
            stale, 0,
            "{name} 里还点着那份已删脚本的旧住址（{stale} 处）—— 那是 `K-R61` 要治的病本身：\n\
                 它 `07e4e72` 就删了，指着它的话不会有任何东西出声。\n\
                 实得这一段：\n{seg}"
        );
    }

    // 整份文件那一格：不只这五段，全文都不许再点那个旧住址。
    // （少了这一格，把旧住址挪出这五段的窗口就能躲过去。）
    let whole = include_str!("../../src/bridge/src/history.rs");
    let left: Vec<&str> = whole
        .lines()
        .filter(|l| {
            l.match_indices(retired.as_str()).any(|(i, _)| {
                l[i + retired.len()..]
                    .chars()
                    .next()
                    .is_none_or(|c| c != '-')
            })
        })
        .collect();
    assert!(
        left.is_empty(),
        "本文件里还有 {} 行点着那份已删脚本：\n  {}",
        left.len(),
        left.join("\n  ")
    );
}

/// 退役条件点名的那个住址 —— **本文件里的字面量只有这一处**（`brief` 13b）。
const R61_ADDR: &str = "src/backend/control/ccm/mod.rs";

/// `KR61D2`：**退役条件点名的仓内住址，不在了就得响。**
///
/// 存在性由本条负责，**不由谁记得**。这一条不是给某一个旧名字写的补丁：
/// 它把那几段里**每一个**看起来像仓内路径的串都拿去盘上核一次 ⇒
/// 下一个被删掉的文件同样会当场红。
///
/// # ⚠⚠ 反向本条**不主张**（这一句是 `KR61D2` 点名要写进头注的）
///
/// 把住址改成一个**存在但不相干**的文件（比如把 `…/ccm/mod.rs` 换成 `…/ccm/argv.rs`），
/// **本条逮不到** —— 它只判「在不在」，不判「这个住址讲的是不是那件事」。
/// 别把它读成「住址对不对有人管」。
///
/// 那半格今天由**别的东西**兜，而且兜得不全，如实写清：
/// - [`the_retirement_condition_says_the_same_thing_in_every_place_that_states_it`]
///   钉着 [`R61_ADDR`] 这**一个**串 ⇒ 换成 `argv.rs` 它会红。但那是**钉死一个字面量**，
///   只护得住这一个住址，护不住下一条退役条件点的下一个住址。
/// - 「那个文件里真的有那个 token」由后端那棵树的
///   `the_base_url_token_is_declared_because_the_tmux_path_really_forwards_it` 守。
/// - 「这段话说的是不是真的」**没有任何东西守**。
///
/// # ⚠ 射程
///
/// 只到 [`r61_sites`] 登记的那几段。**本条不是全仓 doc-link 检查器**
/// （`K-R61 §0e` 逐字禁的就是顺手做那个）—— 全仓那个人群多大，读数住件文件 `§8`。
#[test]
fn every_address_the_retirement_condition_names_is_still_on_disk() {
    let root = crate::guard_support::repo_root().to_path_buf();

    let mut checked: Vec<String> = Vec::new();
    let mut missing: Vec<String> = Vec::new();
    for (name, seg) in r61_segments() {
        for p in r61_paths_in(seg) {
            checked.push(format!("{name} → {p}"));
            if !root.join(&p).exists() {
                missing.push(format!("{name} → `{p}`"));
            }
        }
    }
    // 反空真：抽不到路径的话下面那条 `is_empty()` 是白过的。
    assert!(
        checked.len() >= 4,
        "只从那几段里抽到 {} 个住址 —— 抽取器坏了，本条此刻在空转。抽到的：{checked:?}",
        checked.len()
    );
    assert!(
        missing.is_empty(),
        "退役条件点着 {} 个**盘上没有**的住址：\n  {}\n\
             ⇒ 这就是 `K-R61` 立件的那个形状：判据活着、前提指着一个已经被删掉的文件，\n\
             中间没有任何东西会响。**改住址的同时把那句理由也重读一遍** ——\n\
             `K-R61` 那一轮变的不是住址，是前提本身。\n\
             本轮核过的全部住址（分母 {}）：{checked:?}",
        missing.len(),
        missing.join("\n  "),
        checked.len()
    );
}

/// ★★ **P3t-Y2 的顺序判据**：渲染器在前，`build_local_posix_command` 在后。
///
/// 「两条路都在文件里」证明不了任何事 —— 本件的全部内容就是**谁先谁后**：
/// 旧路必须是「渲染器拒了才走」的回落，不是并列的第二条路。并列意味着
/// 「本机进不进 tmux」由谁先被写下来决定，那不是一个能守住的性质。
///
/// 形状抄 `local_backend::the_exit_path_really_stops_the_local_backend`：
/// 锚点当场核唯一性 + 按花括号配平切体 + 切出来的体自检大小（否则会零命中地绿）。
#[test]
fn the_local_launch_tries_the_renderer_before_the_old_path() {
    let prod = guard_core::production_code(include_str!("../../src/bridge/src/history.rs"));
    // 锚点唯一性：`fn launch_local(` 全树恰好一处（`find_pinned` 自带边界检查）。
    let at = guard_core::find_pinned(&prod, "fn launch_local(")
        .unwrap_or_else(|e| panic!("`fn launch_local(` 不是恰好一处 —— 形状变了，先修锚点：{e}"));
    let body = {
        let b = prod.as_bytes();
        let open = (at..b.len())
            .find(|&i| b[i] == b'{')
            .expect("`fn launch_local(` 之后找不到块起点");
        let (mut depth, mut end) = (0i32, b.len());
        for i in open..b.len() {
            if b[i] == b'{' {
                depth += 1;
            } else if b[i] == b'}' {
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
        body.len() > 200 && body.len() < 3000,
        "切出来的 `launch_local` 体只有 {} 字节 —— 配平切错了，本条会零命中地绿",
        body.len()
    );

    let r = body
        .find("render_local_ccm(")
        .expect("`launch_local` 体里找不到 `render_local_ccm(` —— 渲染器没接上，本机还是走旧路");
    let old = body.find("build_local_posix_command(").expect(
        "`launch_local` 体里找不到 `build_local_posix_command(` —— 回落没了，渲染器拒了就无路可走",
    );
    assert!(
        r < old,
        "★ 顺序反了：`build_local_posix_command` 出现在 `render_local_ccm` **之前**。\n\
             那样旧路就成了并列的第一条路，渲染器变成够不着的死代码 —— 本件等于没做。"
    );
    // 各恰好一处：两处渲染器调用意味着有一条分支绕过了顺序。
    for (needle, n) in [
        (
            "render_local_ccm(",
            body.matches("render_local_ccm(").count(),
        ),
        (
            "build_local_posix_command(",
            body.matches("build_local_posix_command(").count(),
        ),
    ] {
        assert_eq!(
            n, 1,
            "`launch_local` 体里 `{needle}` 出现 {n} 次 —— 不是恰好一处，顺序就管不住了"
        );
    }
    // 回落必须真的住在 `Err` 那条臂里，而不是顺序碰巧靠后。
    let err_arm = body
        .find("Err(why)")
        .expect("`launch_local` 体里找不到 `Err(why)` —— 回落不在降级臂里了");
    assert!(
        err_arm < old,
        "`build_local_posix_command` 不在 `Err(why)` 臂内 —— 它只是碰巧写在后面，\n\
             那不叫「渲染器拒了才走」。"
    );
}
use super::*;

/// 每个测试独占的临时目录（仓库约定不引 `tempfile`，用 pid + 计数器保唯一）。
/// **绝不碰用户真实的 `~/.claude`** —— 全部在 `std::env::temp_dir()` 下。
struct TmpDir(PathBuf);
static TMP_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
impl TmpDir {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!(
            "hist-{}-{}",
            std::process::id(),
            TMP_SEQ.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
        ));
        std::fs::create_dir_all(&p).expect("mkdir");
        TmpDir(p)
    }
    fn write(&self, name: &str, body: &str) -> PathBuf {
        let f = self.0.join(name);
        std::fs::write(&f, body).expect("write");
        f
    }
}
impl Drop for TmpDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

// 〔RW1 · 第四波 · 2026-09-24〕这里原来是本机分叉读 jsonl 那一格（`read_jsonl_values`〔散文墓碑〕：剥 BOM · 静默丢坏行）
// 的判据。本机分叉交给后端之后那个函数零调用方、删了；后端那份读法的判据住 `fork_write_tests.rs`。

/// ★★〔`K-R97` 09-12 后继形态〕**「从 jsonl 头部抠 cwd」这件事，全仓只剩一处了。**
///
/// # 原形是什么、为什么换
///
/// 原形叫「两个提取器仍旧照登记的样子不一致」〔audit-0805 08-06〕：同一个问题两处实现 ——
/// monitor 窗口 **30** 行且只认 `JsonlRecord::User`，后端窗口 **40** 行且认任何带非空 cwd
/// 的记录。后果具体：首个带 cwd 的记录落在第 31–40 行时，**两边给两个答案**。
/// 那一版**只钉不改**（走档①：登记 + 钉住），因为「取 30 还是 40」是会改行为的设计决定。
///
/// `K-R97` 把本机项目列表改走后端那条 `--list-projects` ⇒ monitor 那一份**连同它唯一的
/// 调用点一起没了**。⚠ **这不是「对齐到 40」**，是那个设计决定**不再需要有人做** ——
/// 问题只剩一个实现，也就无从不一致。
///
/// ⇒ 本条换成后继形态：**钉住 monitor 侧不许再长出第二份**，并核后端那一份还在。
/// ⚠ **这不是降强度**：原形钉的是两个数的差（谁改了都红），后继钉的是「只剩一处」
/// （谁把第二份写回来都红），而后者恰恰是 `K33`「所有命令只许有一处」的形状。
#[test]
fn extracting_cwd_from_a_jsonl_head_now_lives_in_exactly_one_place() {
    // ① monitor 生产段：**一个头部窗口读法都不许有**。
    let own = guard_core::production_code(include_str!("../../src/bridge/src/history.rs"));
    let local: Vec<&str> = own
        .lines()
        .filter(|l| l.contains("reader.lines().map_while(Result::ok).take("))
        .collect();
    assert!(
        local.is_empty(),
        "monitor 侧又长出了一份 jsonl 头部读法：\n{}\n\n\
             ⇒ `K-R97` 之后这件事的家在后端（`observe/history_query.rs`）。\n\
             真要在 monitor 侧读，先回答「为什么这条路问不了后端」，再连同本条一起改。",
        local.join("\n")
    );

    // ② 后端那一份还在，且窗口是个说得出的数 —— 否则上面那条会零命中地绿
    //    （「两边都没有」与「只剩一处」在断言上长得一样，这一格就是分开它们的那个）。
    let backend_src = std::fs::read_to_string(
        crate::guard_support::repo_root().join("src/backend/observe/history_query.rs"),
    )
    .expect("读不到后端的 history_query.rs");
    let remote: Vec<usize> = guard_core::production_code(&backend_src)
        .lines()
        .filter(|l| l.contains("reader.lines().map_while(Result::ok).take("))
        .filter_map(|l| l.split(".take(").nth(1))
        .filter_map(|s| s.split(')').next())
        .filter_map(|s| s.trim().parse::<usize>().ok())
        .collect();
    assert_eq!(
        remote,
        vec![40],
        "后端那一份不是「恰好一处、窗口 40 行」了（实得 {remote:?}）。\n\
             ① 变成 0 处 ⇒ 那件事没人做了，而 monitor 这侧已经不做了；\n\
             ② 变成 2 处 ⇒ 两份实现在后端里面又长了一次。"
    );
}

// 〔C4d · 第四波 4B〕历史清单与注解搬进本机常驻后端（主会话 09-25 裁：join 只一个家、注解读写者换成本机后端）——
//   这里原先驱动 monitor 那一份实现的几组判据随被测函数一起退役，它们钉的性质各自在新家有判据（逐条对应）：
//   - `K-R97` 本机项目清单来自后端那一行 · 行里的「不知道」不被压平 · 本机判活答真值 · 一次列举只问一次
//     （`the_local_project_list_is_whatever_the_backend_said` 那一组〔散文墓碑〕）⇒ 后端 `tests/backend/history_join_tests.rs`
//     （`local_liveness_answers_true_and_false_and_unknown_is_its_own_bucket` · `a_local_listing_joins_the_record_tree_and_the_synthesized_history` ·
//     `one_remote_is_asked_exactly_once_with_the_old_subcommands` · `remote_projects_carry_the_annotation_counts_and_say_unknown_honestly`）；
//     「本机后端不在 ≠ 一个项目都没有」⇒ 通道的失败层级（`src/history-reads.ts` 抛、界面说「加载失败」），判据 `tests/history-reads.vitest.ts`；
//   - `K-R92` 线上那几格分得开「不知道」与「真的是 0」· 「不知道」自成一档排序（`the_three_counts_can_say_i_do_not_know` 那两条〔散文墓碑〕）
//     ⇒ 后端 `history_join_tests.rs`（`unreadable_annotations_are_unknown_not_zero` · `projects_sort_unknown_between_known_true_and_known_false`）；
//   - 两个线上形状的驼峰契约（`history_project_camel_case_contract` 那两条〔散文墓碑〕）⇒ 跨语言金样 `tests/__fixtures__/history-products.golden.json`
//     （后端产 · TS 解码器逐键收）；
//   - Codex 分组 · 首条真用户话去注入（`codex_projects_group_by_cwd` 那两条〔散文墓碑〕）⇒ 后端 `tests/backend/agents/codex/history_tests.rs`
//     ＋ `history_join_tests.rs::synthesized_history_groups_by_cwd_under_the_kind_prefix`；
//   - 上次账号的 serde 与 patch 三态 · 只含真有的那几条（`last_account_serde_and_patch_semantics` 那两条〔散文墓碑〕）⇒ 后端
//     `tests/backend/history_annotations_tests.rs`（`patch_semantics_match_what_the_monitor_did` · `last_accounts_are_only_the_entries_that_have_one`）；
//   - 摘录按字符截断（`truncate_chars_unicode` 那三条〔散文墓碑〕）⇒ `search-core` 的 `truncate_excerpt`（后端会话行改用它）；
//   - 「迁移前」旧读者读注解夹具 == 金样（`c4d_the_old_reader_reads_the_annotation_fixture_as_the_golden`〔散文墓碑〕，子步 4 那一拍对过）
//     ⇒ 金样 `tests/__fixtures__/history-metadata.readout.golden.json` 留作「迁移前」的冻结读数，后端新读者照旧对它。

/// Phase 2 F1a-3：Codex 会话按 cwd 分组成合成 HistoryProject（count/max-mtime/name/键/has_live）。
/// 测试用：把一个 configDir 包成具名账号。
fn named(d: &str) -> LaunchAccount {
    LaunchAccount::Named {
        config_dir: d.to_string(),
        name: None,
    }
}

// ── G3b：本地拉起的账号注入（`CLAUDE_CONFIG_DIR`）─────────────────────────
//
// 既有那批**逐字节钉死输出**的测试全部传 `None` 后原样通过 ——
// 它们因此就是「**账号 0 = 一个字都不注入 = 旧行为**」的守卫，不用再写一条。

#[test]
fn account_zero_injects_nothing_byte_for_byte() {
    // 三种「没有账号」的表达（None / 空串 / 纯空白）产出必须完全相同。
    // ① 参数缺席 = 调用方没表态 ⇒ 一个字都不注入（既有调用点逐字节等价旧行为）
    let a = build_local_posix_command(&LocalPsAction::New, None, None).unwrap();
    assert!(
        !a.contains("CLAUDE_CONFIG_DIR"),
        "没表态时不该出现这个变量名"
    );

    // ② ★★ 显式账号 0 = **unset**，不是「什么都不加」。Phase G 审计抓出的静默串号：
    //    本地拉起故意加载 rc，而 rc 里很可能有 `export CLAUDE_CONFIG_DIR=<默认账号>`
    //    ⇒ 「什么都不加」会落到别的号上，而弹窗上写着「不注入」。
    let base =
        build_local_posix_command(&LocalPsAction::New, None, Some(&LaunchAccount::Base)).unwrap();
    assert!(
        base.starts_with("unset CLAUDE_CONFIG_DIR; "),
        "账号 0 必须显式 unset：{base}"
    );
    assert!(base.ends_with(&a), "前缀不该改动命令本体");
    let base_ps =
        build_local_ps_command(&LocalPsAction::New, None, Some(&LaunchAccount::Base)).unwrap();
    assert!(
        base_ps.starts_with("$env:CLAUDE_CONFIG_DIR=$null; "),
        "PS 侧的账号 0 同样要显式清掉：{base_ps}"
    );

    // ③ 具名账号但 configDir 是空串 ⇒ **坏数据，报错**（空值 ≠ 未设）
    assert!(build_local_posix_command(&LocalPsAction::New, None, Some(&named(""))).is_err());
    assert!(build_local_posix_command(&LocalPsAction::New, None, Some(&named("   "))).is_err());
}

#[test]
fn account_prefix_is_prepended_posix_and_ps() {
    let dir = "/home/u/.claude-alt/z";
    let px = build_local_posix_command(&LocalPsAction::New, None, Some(&named(dir))).unwrap();
    assert!(
        px.starts_with(&format!("export CLAUDE_CONFIG_DIR='{dir}'; ")),
        "POSIX 前缀必须在最前面（要先于拉起命令生效）：{px}"
    );
    // 前缀之后仍是原来那条命令，逐字节
    let bare = build_local_posix_command(&LocalPsAction::New, None, None).unwrap();
    assert!(px.ends_with(&bare), "前缀不该改动命令本体");

    let ps = build_local_ps_command(&LocalPsAction::New, None, Some(&named(dir))).unwrap();
    assert!(
        ps.starts_with(&format!("$env:CLAUDE_CONFIG_DIR='{dir}'; ")),
        "{ps}"
    );
}

/// ★★ Phase G 审计抓出的真 bug：**Windows 上的账号目录必须被接受**。
///
/// 原来 POSIX 与 PS 共用一条「必须 `/` 开头 + 禁 `\`」的校验 ⇒
/// `C:\Users\z\.claude-alt\z` 恒被拒 ⇒ 「本机分叉时选一个具名账号」在**主平台**上
/// 100% 失败（`fork-flow.ts` 是全仓唯一给 `resume_history_session` 传 `configDir` 的
/// 调用点，所以这个洞是分叉专属的、别处测不到）。
///
/// 判据照抄当年的 `local_accounts::looks_absolute`〔散文墓碑〕（〔C4d〕已删；同一课今天住后端 `is_safe_config_dir`）。
/// 而**旧测试全喂 POSIX 路径**（`/home/u/.claude-alt/z`），所以它们测不出来。
#[test]
fn windows_account_dirs_are_accepted_by_the_ps_side() {
    for d in [
        "C:\\Users\\z\\.claude-alt\\z",
        "D:/Users/z/.claude-alt/b",
        "\\\\server\\share\\accts\\z",
    ] {
        assert!(
            build_local_ps_command(&LocalPsAction::New, None, Some(&named(d))).is_ok(),
            "Windows 账号目录被拒了：{d:?}"
        );
    }
    // POSIX 那条**仍然**只收 POSIX 路径（各自平台各自判据，别互相放宽）
    assert!(
        build_local_posix_command(&LocalPsAction::New, None, Some(&named("C:\\Users\\z"))).is_err(),
        "POSIX 侧不该接受 Windows 路径"
    );
    // 反斜杠形态的 `..` 也要挡住
    for d in ["C:\\Users\\..\\evil", "C:\\Users\\z\\.."] {
        assert!(
            build_local_ps_command(&LocalPsAction::New, None, Some(&named(d))).is_err(),
            "PS 侧漏了反斜杠 `..`：{d:?}"
        );
    }
    // 引号仍然禁（单引号能提前闭合 PS 的字面量串）
    assert!(
        build_local_ps_command(&LocalPsAction::New, None, Some(&named("C:\\a'; rm x; '"))).is_err()
    );
}

/// ★ 非法 configDir **绝不拼进命令** —— 这条产物会进 shell，宽容一格就是注入面。
/// 判据与载荷渲染同一道闸（`payload.rs::config_dir_command_safe`），不重新发明。〔DUP1〕原先写的是「照抄 TS 侧 `isValidConfigDir`」〔散文墓碑〕，TS 那份删了。
#[test]
fn illegal_config_dir_is_refused_not_sanitized() {
    let bad = [
        "relative/path",              // 非绝对
        "/",                          // 根
        "/home/u/../../etc",          // 含 /../
        "/home/u/..",                 // 以 /.. 结尾
        "/home/u'; rm -rf /; echo '", // 单引号闭合 + 注入
        "/home/u`whoami`",            // 反引号
        "/home/u$HOME",               // 变量展开
        "/home/u;id",                 // 分号
        "/home/u|id",                 // 管道
        "/home/u\n/x",                // 控制符（字面反斜杠 n 不算，见下面真控制符）
        "/home/u\u{0000}x",
        "/home/u\u{200b}x", // 零宽
        "/home/u\u{feff}x", // BOM
        "/home/u\u{00a0}x", // NBSP
    ];
    for d in bad {
        assert!(
            build_local_posix_command(&LocalPsAction::New, None, Some(&named(d))).is_err(),
            "非法 configDir 竟被接受：{d:?}"
        );
        assert!(
            build_local_ps_command(&LocalPsAction::New, None, Some(&named(d))).is_err(),
            "非法 configDir 竟被接受（PS）：{d:?}"
        );
    }
    // 反向自检：正常路径必须通过，否则上面全是空转
    assert!(build_local_posix_command(
        &LocalPsAction::New,
        None,
        Some(&named("/home/u/.claude-alt/z"))
    )
    .is_ok());
}

/// ★ U8c-1：POSIX 校验改调内核（P4b 起 `backend::control::payload`）之后**多拒**的那六段码位。
///
/// 这不是纯重构 —— 本文件原先用的 `SPOOFABLE` 是 **U7-3 之前**的旧集合，
/// 而内核建立在 `acct_core::is_deceptive_char` 的并集上。这条测试点名那六段，
/// 变异（把内核换回旧表）时会逐个报出来。
///
/// ⚠ **PS 那条路刻意还用旧表**（Windows 平台特化，见 `SPOOFABLE` 头注），
/// 所以这里只断言 POSIX 侧 —— 断言 PS 侧会当场红，那才是假装做完了。
#[test]
fn posix_config_dir_now_rejects_the_code_points_u7_3_added() {
    for (name, c) in [
        ("U+1680 Ogham space", '\u{1680}'),
        ("U+2000 en quad", '\u{2000}'),
        ("U+200A hair space", '\u{200a}'),
        ("U+202F narrow NBSP", '\u{202f}'),
        ("U+205F medium math space", '\u{205f}'),
        ("U+2060 word joiner", '\u{2060}'),
        ("U+3000 ideographic space", '\u{3000}'),
    ] {
        let dir = format!("/home/u/.claude-alt/{c}z");
        assert!(
            build_local_posix_command(&LocalPsAction::New, None, Some(&named(&dir))).is_err(),
            "{name} 应被 POSIX 侧拒掉（acct-core 并集里有它，history.rs 旧表没有）"
        );
    }
}

/// ★ U8c-1：合法输入的产物**逐字节不变** —— 搬内核不许改一个字节。
#[test]
fn posix_account_prefix_is_byte_identical_after_moving_to_the_kernel() {
    assert_eq!(
        config_dir_prefix_posix(None).unwrap(),
        "",
        "参数缺席仍是空串（既有调用点逐字节等价旧行为）"
    );
    assert_eq!(
        config_dir_prefix_posix(Some(&LaunchAccount::Base)).unwrap(),
        "unset CLAUDE_CONFIG_DIR; ",
        "账号 0 的逐字节形态被 e2e 探针 grep 着"
    );
    assert_eq!(
        config_dir_prefix_posix(Some(&named("/home/u/.claude-alt/z"))).unwrap(),
        "export CLAUDE_CONFIG_DIR='/home/u/.claude-alt/z'; "
    );
}

// 〔RW1 · 第四波 · 2026-09-24〕这里原来是「建分支入口真的过了围栏吗」（喂一个界外的绝对路径给本机那份
// `branch_impl`〔散文墓碑〕，要求形状闸在任何 IO 之前拒）。本机分叉改成 exec 本机后端 `--fork-session` 之后，
// 本进程里没有那份实现了；形状闸今天住 `remote_branch::validate_fork_id`（两侧共用，判据在 `remote_branch_tests.rs`）〔散文墓碑〕
// 与后端 `branch_core::find_session_file` 的 `is_plain_sid`（判据在后端 `fork_write_tests.rs`）。

// 〔RW1 · 第四波 · 2026-09-24〕这里原来是删除那条路的两组判据：端到端的「删除入口真的过了围栏吗」
// （子进程里造一份自己的 claude 目录、喂一个 projects 之外的文件，要求入口拒、文件还在）与
// `validate_delete_target`〔散文墓碑〕的四条穿越防护（`..` · symlink 出界 · 非 jsonl · 不存在）。
// 删历史会话改成后端一条只收 sid 的命令（`files-delete-session`）之后：
//   · 「只收 sid、落点由后端按 sid 找、解到底必须恰是 `<项目>/<sid>.jsonl`、symlink 出界不跟」
//     那几格住后端判据（`tests/backend/control/files_write_tests.rs` 的删会话那一族 ＋
//     `readonly_guard::the_session_file_exception_lives_in_exactly_one_place`）；
//   · 本侧只剩**一致性闸**（界面给的 sid 必须恰是那份文件名的 stem）与「交出去的只有 sid」两格，判据在下面。

// 〔MIG-3b〕删会话那道 stem 一致性闸的两条判据（只交 sid · 对不上一个请求都不发）与
//   分叉结果形状那一条（驼峰键）随 monitor 那两条命令删了：界面经通道直说那台后端（`src/session-writes.ts`），
//   分叉成品由金样 `tests/__fixtures__/session-fork.golden.json` 钉（`tests/session-writes.vitest.ts` 读同一份）；
//   stem 闸是恒真的（会话行的 `sessionId` 由后端按文件名 stem 出），后端删之前自己判「落点恰是 `<sid>.jsonl`」。

// 〔RW1 · 第四波 · 2026-09-24〕这里原来是本机分叉那份实现的四条 IO 判据（`O_EXCL` 不覆盖 · 软链逃逸按 sid 找不到 ·
// 源零改动 ＋ 新文件原生格式 · 以及它们共用的最小会话夹具）。本机分叉改成 exec 本机后端 `--fork-session` 之后，
// 同一组性质由后端那份实现（`src/backend/control/fork_write.rs`）的同形判据守着（`fork_write_tests.rs`），
// 本侧只剩「结果怎么解释」（`remote_branch_tests.rs`：本机那一趟的三态折成与远端同形的结果）。

/// 后端那棵树上某个文件的**生产段**（运行时读，不是 `include_str!`）。
///
/// ⚠ 刻意**不用** `include_str!`：那会长出一条**编译期**的跨半边，
/// 而 `cross_half_edge_registry` 的头注逐字讲过那条边的代价
/// （后端在目标机上 `cargo build` 就咬住旁边这棵树了）。运行时读没有这个代价 ——
/// 同 `K-R97` 那条 `extracting_cwd_from_a_jsonl_head_now_lives_in_exactly_one_place`。
fn r88_backend_production(rel: &str) -> String {
    let p = crate::guard_support::repo_root().join(rel);
    let raw = std::fs::read_to_string(&p)
        .unwrap_or_else(|e| panic!("读不到后端的 {rel}：{e} —— 先修住址，别绕过本条"));
    guard_core::production_code(&raw)
}

/// ★★ `KR88D1`：**「按 sid 找那份会话文件」这件事，全仓只剩一份实现，两侧都调它。**
///
/// # 它买什么
///
/// 收之前两侧各有一份、而且**入参形状都不一样**（这边收路径、那边收 sid）。
/// 那不是「重复」这么简单：**「查不到怎么办」两边可以各答各的**，
/// 而没有任何东西会因此变红。
///
/// # 🔴 它刻意**不**判什么（`KR88D1` 点名的失效方向）
///
/// **不判「两边源码文本一样」** —— 那是判写法，而且很容易恒绿
/// （两边都没有那段文本时它照样通过）。本条判的是**同一份实现**：
/// 唯一那份的**声明只有一处**，两侧各有**恰好一处**调用，
/// 且两条分叉路径上**一处目录枚举都不许有**（有 = 有人又自己找了一遍）。
///
/// 「改那一份一处、两边行为都跟着变」那一刀是**死值验**，读数落在件文件 `§3-1`：
/// 判据不可能替代它 —— 那一刀要真的改一次再看两边红不红。
#[test]
fn finding_a_session_file_by_sid_now_lives_in_exactly_one_place() {
    const CALL: &str = "branch_core::find_session_file(";

    // ① 唯一那份：声明只有一处，且住在共享 crate 里。
    let core = guard_core::production_code(include_str!(
        "../../src/bridge/crates/branch-core/src/lib.rs"
    ));
    let decls = core.matches("pub fn find_session_file").count();
    assert_eq!(
        decls, 1,
        "共享 crate 里 `find_session_file` 的声明有 {decls} 处（该是 1）。\n\
             0 ⇒ 它被搬走/删了，下面两条会零命中地绿；2 ⇒ 唯一那份自己裂了。"
    );

    // ② 〔RW1 · 第四波 09-24〕**monitor 那一侧零处、后端那一侧恰好一处**。
    //    从前两侧各有一处（本机分叉在 monitor 进程里找、写）；本机分叉改成 exec 本机后端的
    //    `--fork-session` 之后，「按 sid 找那份」只剩后端那一处在问 —— monitor 这一侧再出现一处，
    //    就是有人又在本进程里做分叉了（那正是用户裁掉的那一形：monitor 不直接写用户文件）。
    let mine = guard_core::production_code(include_str!("../../src/bridge/src/history.rs"));
    // 〔MIG-3b〕monitor 分叉那一侧的模块删了（界面经通道直说 `session-fork`），人群只剩 `history.rs` 与后端。
    let theirs = r88_backend_production("src/backend/control/fork_write.rs");
    for (who, src, want) in [
        ("monitor `history.rs`", &mine, 0usize),
        ("后端 `fork_write.rs`", &theirs, 1),
    ] {
        let n = src.matches(CALL).count();
        assert_eq!(
            n, want,
            "{who} 的生产段里 `{CALL}` 有 {n} 处（该是 {want}）——\n\
                 monitor 那一侧 ≠ 0 ⇒ 又在本进程里找 / 写会话了（`RW1`：分叉两侧都交给后端）；\n\
                 后端那一侧 ≠ 1 ⇒ 不走共享那份了，或一条路上问了两遍。"
        );
    }
    // 〔MIG-3b〕「只有一处发送点」那一格挪到界面：前端 `chan.call` 的 `session-fork` 只在 `src/session-writes.ts`
    //   （`frame_query_tests::the_channeled_ops_are_sent_only_through_the_channel` 数 monitor 零字面量）。

    // ③ 两条分叉路径上**一处目录枚举都没有** —— 「自己又找了一遍」的形状。
    //
    // ⚠ 人群按**那几个函数**切，不是整份 `history.rs`：这个文件别处本来就有遍历
    //（历史列表那一族），拿整份文件当分母的话本条恒红。
    // ⚠ 针**运行时拼**：本文件的测试段自己落在 `scanning_guard_registry` 的扫描面里，
    //   把那两个词写成字面量会让本条被算进「裸遍历」的人群（09-13 现打，它当场逮到了）。
    let needles = [format!("read{}dir(", "_"), format!("Walk{}", "Dir")];
    let scan = |src: &str| -> Vec<String> {
        src.lines()
            .filter(|l| needles.iter().any(|n| l.contains(n.as_str())))
            .map(|l| l.trim().to_string())
            .collect()
    };
    // 〔MIG-3b〕monitor 那一侧没有分叉那条路了（界面直说后端），人群只剩后端那一份。
    // 反向自检：尺子够得着 —— 把针塞进一份副本，量具必须数得出来。
    let poisoned = format!("{theirs}\n  let _ = std::fs::read{}dir(root);\n", "_");
    assert_eq!(
        scan(&poisoned).len(),
        1,
        "阳性对照没过 —— 量具此刻无效，下面那条断言是空真"
    );
    for (who, src) in [("后端 `fork_write.rs`", theirs.as_str())] {
        let hits = scan(src);
        assert!(
            hits.is_empty(),
            "{who}上又长出了目录枚举：\n{}\n\n\
                 ⇒ 「按 sid 找那份会话文件」的家在 `branch_core::find_session_file`。\n\
                 真有第二种找法要立，先回答「为什么这一侧不能问那一份」，再连本条一起改。",
            hits.join("\n")
        );
    }
}

// 〔RW1 · 第四波 · 2026-09-24〕这里原来是 `KR88D2` 的 monitor 那一侧（查不到的 sid ⇒ 报错，不静默挑第一个）。
// monitor 进程里不再有分叉的实现，那条性质只住后端（`fork_write·rs::an_unknown_session_id_is_refused_not_silently_substituted`）。

// 〔MIG-3b〕`KR88D2`「两侧入参都收 sid、都不收路径」那一条随
//   monitor 那两条分叉命令一起退役：今天只有一处发出分叉（界面 `session-writes.ts::forkSession`，请求体恰好 `{sid, uuid}`，
//   由金样 `session-fork.golden.json` 的 `request` 钉），后端入口只认这两格。

// P3 归并：iso_parse_* 测试已搬到 utils::tests（函数本身搬到 utils）。

#[test]
fn resume_cmd_prefers_cc_with_claude_fallback() {
    let sid = "01998f2a-1234-7abc-9def-0123456789ab";
    let cmd = build_resume_ps_command(sid, None).unwrap();
    // 优先 cc、回退 claude，两者都带正确 sid
    assert!(cmd.contains("Get-Command cc"));
    assert!(cmd.contains(&format!("cc --resume {sid}")));
    assert!(cmd.contains(&format!("claude --resume {sid}")));
}

#[test]
fn resume_cmd_rejects_injection() {
    // 含 shell 元字符的 session_id 必须被拒（防命令注入）
    for bad in [
        "a; rm -rf /",
        "a && calc",
        "a`whoami`",
        "a$(id)",
        "a b",
        "a\"b",
        "",
        "a/../b",
    ] {
        assert!(
            build_resume_ps_command(bad, None).is_err(),
            "应拒绝危险 session_id: {bad:?}"
        );
    }
}

/// F34：自定义 launcher——合法形态放行、注入面拒绝、空视为未设置。
/// 〔DUP3 · `INVARIANTS §47` ③〕判定是全仓那一张命令片段白名单（`shell_quote_core::launcher_refused_char`）：
/// 路径与前导 `~/` 从此放行（先前本机这一份不许 `/`）；拒的那一句说出是哪个字符。正反各一格。
#[test]
fn a_custom_launcher_goes_through_the_one_command_fragment_whitelist() {
    assert_eq!(checked_launcher(None).unwrap(), None);
    assert_eq!(checked_launcher(Some("")).unwrap(), None);
    assert_eq!(checked_launcher(Some("   ")).unwrap(), None);
    assert_eq!(
        checked_launcher(Some("cct")).unwrap().as_deref(),
        Some("cct")
    );
    assert_eq!(
        checked_launcher(Some(" cc -p 8 ")).unwrap().as_deref(),
        Some("cc -p 8")
    );
    for good in ["/usr/local/bin/claude", "~/bin/claude --x", "ccr code"] {
        assert_eq!(
            checked_launcher(Some(good)).unwrap().as_deref(),
            Some(good),
            "真实启动器被拒了：{good:?}"
        );
    }
    for (bad, c) in [
        ("cc; calc", ';'),
        ("cc|id", '|'),
        ("cc$(id)", '$'),
        ("cc`id`", '`'),
        ("cc&&x", '&'),
        ("cc\"x", '"'),
        ("cc (x)", '('),
        ("a~b", '~'),
    ] {
        let e = checked_launcher(Some(bad)).expect_err(bad);
        assert!(
            e.contains(&format!("{c:?}")),
            "{bad:?}：那一句没说出是哪个字符：{e}"
        );
    }
}

#[test]
fn resume_cmd_custom_launcher_used_verbatim() {
    let sid = "abc-123";
    let cmd = build_resume_ps_command(sid, Some("cct")).unwrap();
    assert_eq!(cmd, "cct --resume abc-123");
    // 设了自定义命令就不再出现 cc 自动检测
    assert!(!cmd.contains("Get-Command"));
}

/// F96：本地起新会话命令——同 cc 优先/回退逻辑，但**不带 resume flag / sid**。
#[test]
fn new_session_cmd_prefers_cc_no_resume_flag() {
    let cmd = build_new_session_ps_command(None).unwrap();
    assert!(cmd.contains("Get-Command cc"));
    assert!(cmd.contains("{ cc }"), "cc 分支: {cmd}");
    assert!(cmd.contains("{ claude }"), "回退分支: {cmd}");
    // 起新会话不是 resume：绝不带 --resume / sid
    assert!(
        !cmd.contains("--resume"),
        "起新会话不应带 resume flag: {cmd}"
    );
}

#[test]
fn new_session_cmd_custom_launcher_verbatim() {
    let cmd = build_new_session_ps_command(Some("cct")).unwrap();
    assert_eq!(cmd, "cct");
    assert!(!cmd.contains("Get-Command"));
    assert!(!cmd.contains("--resume"));
}

#[test]
fn new_session_cmd_rejects_injection_launcher() {
    for bad in ["cc; calc", "cc|id", "cc$(id)", "cc`id`", "cc&&x"] {
        assert!(
            build_new_session_ps_command(Some(bad)).is_err(),
            "应拒绝注入 launcher: {bad:?}"
        );
    }
}

/// ★ L1：POSIX 渲染器的形状 —— 与 PowerShell 那条是**同一个决策**的另一种写法。
///
/// `Get-Command` 的等价物是 `command -v`（它同样找得到 shell **函数**，
/// 而 `ccm` 的 `cc` 集成正是一个函数；命令跑在 `bash -lic` 里、rc 已加载）。
#[test]
fn posix_renderer_mirrors_the_powershell_one() {
    let sid = "01998f2a-1234-7abc-9def-0123456789ab";
    assert_eq!(
        build_local_posix_command(&LocalPsAction::Resume(sid.to_string()), None, None).unwrap(),
        format!(
            "if command -v cc >/dev/null 2>&1; then cc --resume {sid}; \
                 else claude --resume {sid}; fi"
        )
    );
    assert_eq!(
        build_local_posix_command(&LocalPsAction::New, None, None).unwrap(),
        "if command -v cc >/dev/null 2>&1; then cc; else claude; fi"
    );
    // F34 自定义命令：两边都不做别名探测，**逐字节相同**（这一支没有平台差异）。
    for action in [LocalPsAction::New, LocalPsAction::Resume(sid.to_string())] {
        assert_eq!(
            build_local_posix_command(&action, Some("cct"), None).unwrap(),
            build_local_ps_command(&action, Some("cct"), None).unwrap(),
            "显式指定命令时两个渲染器不该有任何差异"
        );
    }
}

/// ★★ **P3t-Y0：Windows 那条路本件一个字不动 —— 而钉的是「该活下来的性质」，不是字节。**
///
/// `C12` 逐字「windows不要tmux」。用内容哈希钉「一个字没改」看着更严，其实更坏：
/// 将来任何一次正当的 Windows 改动都会让它假红，而假红久了就会被人加豁免 ——
/// 那时它连性质都不守了。⇒ 钉性质：**Windows 本机的渲染器自己永远不产会话容器**。
///
/// ⚠ `launcher` 必须传 `None`。用户显式指定 `cct`（F34）时输出里当然会有 `cct`，
/// 那是**用户自己要的**，不是本工具替他加的 —— `posix_renderer_mirrors_the_powershell_one`
/// 正是拿 `Some("cct")` 在对拍。人群划错这一格，本条会变成「禁止用户用 cct」。
///
/// # 射程（`reach`）
///
/// 够得到：Windows 渲染器的**输出里没有容器**。
/// **够不到**：Windows 那条路今天还跑不跑得起来 —— 没有 Windows 机器，
/// 「没改」证明不了「还能跑」。那一格归 `auto-e2e`（本件 §4 已登记）。
#[test]
fn the_windows_local_path_never_grows_a_session_container() {
    let sid = "01998f2a-1234-7abc-9def-0123456789ab";
    let named = LaunchAccount::Named {
        config_dir: "C:\\Users\\z\\.claude-alt\\z".into(),
        name: None,
    };
    let accounts: [Option<&LaunchAccount>; 3] = [None, Some(&LaunchAccount::Base), Some(&named)];
    let mut checked = 0usize;
    for action in [LocalPsAction::New, LocalPsAction::Resume(sid.to_string())] {
        for acct in accounts {
            let cmd =
                build_local_ps_command(&action, None, acct).expect("这几组形状都该渲染得出来");
            checked += 1;
            assert!(
                !cmd.contains("--ccm-tmux"),
                "Windows 渲染器吐了 `--tmux` —— `C12` 逐字「windows不要tmux」。实得：{cmd}"
            );
            assert!(
                !cmd.split_whitespace().any(|w| w == "cct"),
                "Windows 渲染器自己挑了带 tmux 的别名 `cct`（用户没指定）。实得：{cmd}"
            );
        }
    }
    // 完备性自检：人群空掉时「全过」与「没测」长得一模一样。
    assert_eq!(checked, 6, "只渲了 {checked} 组，人群跑偏了");

    // ★ 结构半：`launch_local` 的 Windows 那支**不许调渲染器**。
    // 光有上面的行为半不够 —— 渲染器可以在 `launch_local` 里被调、把容器加在
    // `build_local_ps_command` **之外**，那样上面六组照样全绿。
    let prod = guard_core::production_code(include_str!("../../src/bridge/src/history.rs"));
    // ⚠ 锚点从裸 `#[cfg(windows)]` 扩到「它 + 它门着的那一行」〔`D6` 回修，08-29〕：
    //   本轮 `PRODUCTION_LAUNCH_SINK` 也按平台分了两支 ⇒ 裸锚点从 1 处变成 2 处，
    //   本条当场红（报文逐字「断言指不明是哪一处」）。**它逮到的是真的**：
    //   锚点不唯一时下面切出来的臂可能是别人的。⇒ 按 `F19` 那条纪律
    //   「把 needle 扩到能唯一确定那个事实的大小」，而**不是**把断言放宽。
    let at = guard_core::find_pinned(&prod, "#[cfg(windows)]\n    let base = {")
        .unwrap_or_else(|e| panic!("`launch_local` 的 Windows 臂锚点不是恰好一处，先修锚点：{e}"));
    let arm = {
        let b = prod.as_bytes();
        let open = (at..b.len()).find(|&i| b[i] == b'{').expect("找不到块起点");
        let (mut depth, mut end) = (0i32, b.len());
        for i in open..b.len() {
            if b[i] == b'{' {
                depth += 1;
            } else if b[i] == b'}' {
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
        arm.len() > 60 && arm.len() < 1500,
        "切出来的 Windows 臂只有 {} 字节 —— 配平切错了，本条会零命中地绿",
        arm.len()
    );
    assert!(
        !arm.contains("render_local_ccm"),
        "`launch_local` 的 Windows 臂调了 CLI 渲染器 —— 那条路会带 `--tmux`。实得：{arm}"
    );
    assert!(
        arm.contains("build_local_ps_command"),
        "`launch_local` 的 Windows 臂不再走 `build_local_ps_command` —— 换路了。实得：{arm}"
    );
}

/// ★★ **P3t-Y5 的出口**：把生产渲染器的**真输出**吐给 e2e。
///
/// e2e 要证「这条命令在真 tmux 上干了什么」。若脚本里手抄一份命令串，证的就是手抄那份 ——
/// 渲染器改了、脚本没改，实测照样绿。⇒ 串必须从**这里**出去。
///
/// `#[ignore]` 是因为它不是判据（不断言任何事），只是个数据出口；
/// 跑法：`cargo test --lib emit_local_launch_command_for_e2e -- --ignored --nocapture`。
#[test]
#[ignore]
#[cfg(not(windows))]
fn emit_local_launch_command_for_e2e() {
    let sid = std::env::var("P3T_E2E_SID").unwrap_or_else(|_| "s1abcdef".into());
    let name = std::env::var("P3T_E2E_TMUX").unwrap_or_else(|_| "s1abcdef-cc".into());
    // ★★ launcher 由 e2e 指定，而且必须是个**独一无二的名字**（实测逼出来的）。
    //
    // 第一版让 e2e 拿 PATH shim 顶掉 `claude`。**那在生产送法下不成立**：
    // `launch_local_posix` 用的是 `bash -lic`，**登录 shell 会重跑 profile 并把
    // `~/.local/bin` 重排到 PATH 最前** ⇒ shim 被顶掉、解析到的是用户**真实的 claude**
    // （C7d 逐字禁的那件事，实测真起了两次）。
    // 用一个只在隔离目录里存在的名字，PATH 谁在前都盖不住它 —— 这是结构保证，不是纪律。
    let launcher = std::env::var("P3T_E2E_LAUNCHER").ok();
    let cmd = render_local_ccm_with(
        &LocalPsAction::Resume(sid),
        launcher.as_deref(),
        Some(&LaunchAccount::Base),
        Some(&name),
        &caps_of_a_current_ccm(),
        true,
    )
    .expect("渲染不出来 —— e2e 无对象可跑");
    println!("P3T_CMD<<<{cmd}>>>");
}

/// ★ L1：**sid 校验与注入防线在 POSIX 那条路上同样生效**。
///
/// 主计划点名这道校验「要保留——那是一道独立防线，不是重复」。
/// L1 把它抽进了共享决策 `local_launch_choice`，本测试钉住抽完之后两条路都还有。
#[test]
fn posix_renderer_keeps_sid_and_launcher_defenses() {
    for bad in ["", "../etc", "a b", "x;id", "sid$(id)"] {
        assert!(
            build_local_posix_command(&LocalPsAction::Resume(bad.to_string()), None, None).is_err(),
            "POSIX 渲染器应拒绝非法 sid: {bad:?}"
        );
    }
    for bad in ["cc; calc", "cc|id", "cc$(id)", "cc`id`", "cc&&x"] {
        assert!(
            build_local_posix_command(&LocalPsAction::New, Some(bad), None).is_err(),
            "POSIX 渲染器应拒绝注入 launcher: {bad:?}"
        );
    }
}

/// F06：`build_resume_ps_command`/`build_new_session_ps_command` 收拢成
/// `build_local_ps_command` 后必须逐字节保持——把重构前两个函数曾经产出的具体字符串
/// 内联成期望值（而非依赖上面 6 条测试的"包含子串"断言，那些不足以证明完全同构）。
#[test]
fn unified_builder_byte_identical_to_pre_f06_resume_output() {
    let sid = "01998f2a-1234-7abc-9def-0123456789ab";
    let expected = "if (Get-Command cc -ErrorAction SilentlyContinue) \
             { cc --resume 01998f2a-1234-7abc-9def-0123456789ab } \
             else { claude --resume 01998f2a-1234-7abc-9def-0123456789ab }";
    assert_eq!(build_resume_ps_command(sid, None).unwrap(), expected);
    assert_eq!(
        build_local_ps_command(&LocalPsAction::Resume(sid.to_string()), None, None).unwrap(),
        expected
    );
}

#[test]
fn unified_builder_byte_identical_to_pre_f06_new_session_output() {
    let expected = "if (Get-Command cc -ErrorAction SilentlyContinue) { cc } else { claude }";
    assert_eq!(build_new_session_ps_command(None).unwrap(), expected);
    assert_eq!(
        build_local_ps_command(&LocalPsAction::New, None, None).unwrap(),
        expected
    );
}

// ═════════════════════════════════════════════════════════════════════
// `K-H2b` · 〔US1〕接上注入点 —— 起会话那一侧：问那台后端要成品、照成品执行
// ═════════════════════════════════════════════════════════════════════

/// ★★★ `KH2B5`（`§0e` 裁一）**渲染那一半**：成品说不注入 ⇒ 前缀是**空串**（命令逐字节与本件之前相同）；
/// 给了地址 ⇒ POSIX / PowerShell 两形（钥匙段读文件，RK1）。期望手写。
#[test]
fn the_relay_prefix_is_rendered_from_the_answer_by_platform() {
    let url = "http://127.0.0.1:8788/s/claude-code/acct-a";
    assert_eq!(relay_prefix_for(None, false), "");
    assert_eq!(relay_prefix_for(None, true), "");
    assert_eq!(
        relay_prefix_for(Some(url), false),
        // 〔E2 · V146〕pane 里先看用户自己设没设 `ANTHROPIC_BASE_URL`：设了 ⇒ 说一行、不注入；没设 ⇒ 注入。
        "[ -n \"${ANTHROPIC_BASE_URL:-}\" ] && printf '%s\\n' 'cc-monitor：这个会话用你自己设的端点（ANTHROPIC_BASE_URL），不走中转，拿不到流式' || export ANTHROPIC_BASE_URL='http://127.0.0.1:8788/'\"$(cat \"$HOME/.cc-monitor/relay-key\")\"'/s/claude-code/acct-a'; ",
    );
    assert_eq!(
        relay_prefix_for(Some(url), true),
        "if ($env:ANTHROPIC_BASE_URL) { Write-Host 'cc-monitor：这个会话用你自己设的端点（ANTHROPIC_BASE_URL），不走中转，拿不到流式' } else { $env:ANTHROPIC_BASE_URL='http://127.0.0.1:8788/' + (Get-Content -Raw -LiteralPath (Join-Path $HOME '.cc-monitor/relay-key')).Trim() + '/s/claude-code/acct-a' }; "
    );
}

/// ★★★ `D5 阻-1` + `D6 阻-2/3` + `D7 阻-4`（〔US1〕换了被问的对象）：**那次拉起真的问了那台后端，而且真的用了答案。**
///
/// 病史（别退回去）：先前量「源码里有没有那段文本」⇒ 文本留住、行为摘掉照绿（第五层）。今天替身在缝上
/// （[`InjectFactSources::endpoint`]）记下**每一次被问、问的入参**，承重的是第二半：**前缀必须随替身的答案变** ——
/// 有行 / 没行 / 中转不在 / 平台 / 哪个号 / 哪一次拉起（resume 的 sid · 新开的 nonce）各自落到不同的脸上。
/// 把任何一维写死成常量，这几格里至少一格当场不同。
///
/// 买不到：后端那张决策表答得对不对（`endpoint_tests`）；生产上缝里插的是不是真的那一跳（下一条按函数地址对拍）。
#[test]
fn the_launch_side_really_asks_the_backend_and_uses_its_answer() {
    use std::cell::Cell;
    thread_local! {
        static WINDOWS_ANSWER: Cell<bool> = const { Cell::new(false) };
    }
    fn spy_windows() -> bool {
        WINDOWS_ANSWER.with(Cell::get)
    }
    let acct_a = LaunchAccount::Named {
        config_dir: "/h/.claude-alt/acct-a".to_string(),
        name: None,
    };
    let acct_b = LaunchAccount::Named {
        config_dir: "/h/.claude-alt/acct-b".to_string(),
        name: None,
    };
    let action = LocalPsAction::Resume("sid-1".to_string());
    let _guard = override_inject_facts(InjectFactSources {
        endpoint: fake_ep,
        windows: spy_windows,
        all_sessions: all_sessions_off,
    });
    let s_url = |id: &str| format!("http://127.0.0.1:8788/s/claude-code/{id}");

    // ① 有行 ＋ 在听 ⇒ 前缀 == 纯函数拿那个地址渲出来的；问了恰好一次、问的是本机、入参逐格对。
    fake_table(&["acct-a", "acct-b"], true);
    let _ = fake_asks();
    let got = prefix_now(&action, Some(&acct_a)).expect("这一档不该报错");
    let want = relay_prefix_for(Some(&s_url("acct-a")), false);
    assert!(!want.is_empty(), "期望值是空串 —— 下面那条相等断言是空真");
    assert_eq!(got, want, "问是问了，**答案没被用上**（`D5` 那一刀的形状）");
    let asks = fake_asks();
    assert_eq!(
        asks,
        vec![(
            None,
            serde_json::json!({"agent":"claude-code",
                "account":{"kind":"named","configDir":"/h/.claude-alt/acct-a","name":null},
                "allSessions":false})
        )],
        "这次拉起没问那台后端 / 问了不止一次 / 问的入参不对"
    );

    // ①b 只换一个号 ⇒ 路由键的账号段跟着变（`D6 阻-2`：acct-b 的会话不许拿 acct-a 的 key）。
    let got_b = prefix_now(&action, Some(&acct_b)).expect("这一档不该报错");
    assert!(
        got.contains("/acct-a'") && got_b.contains("/acct-b'"),
        "{got:?} · {got_b:?}"
    );

    // ② 只把「表里有没有这一行」翻过来 ⇒ 空串（不注入，逐字节旧路）。
    fake_table(&["someone-else"], true);
    assert_eq!(prefix_now(&action, Some(&acct_a)).unwrap(), "");

    // ③ 只把「中转在不在」翻过来 ⇒ `Err`，话里说得出是哪个号、为什么（`KH2B2`②：不许静默）。
    fake_table(&["acct-a"], false);
    let e = prefix_now(&action, Some(&acct_a)).expect_err("中转不在却照旧起出去了");
    assert!(e.contains("acct-a") && e.contains("中转没在听"), "{e}");

    // ④ 只把「这台机是不是 Windows」翻过来 ⇒ PowerShell 形态（`D6 阻-3`）。
    fake_table(&["acct-a"], true);
    WINDOWS_ANSWER.with(|c| c.set(true));
    let ps = prefix_now(&action, Some(&acct_a)).unwrap();
    assert_eq!(ps, relay_prefix_for(Some(&s_url("acct-a")), true));
    WINDOWS_ANSWER.with(|c| c.set(false));
    assert_ne!(prefix_now(&action, Some(&acct_a)).unwrap(), ps);

    // ⑤⑥ 〔V141〕resume 与新开**都真的问、都真的注入**（刀 `S1` `S2`），而问的入参与拿到的前缀逐字相同：
    //   地址不随会话变、入参里零会话身份（会话 id 归 claude，中转从它的请求头认）。
    let _ = fake_asks();
    let r9 = prefix_now(&LocalPsAction::Resume("sid-9".to_string()), Some(&acct_a)).unwrap();
    let n1 = prefix_now(&LocalPsAction::New, Some(&acct_a)).unwrap();
    assert_eq!(r9, relay_prefix_for(Some(&s_url("acct-a")), false));
    assert_eq!(n1, r9, "新开与 resume 的地址不同 ⇒ 地址里带了会话身份");
    let asks = fake_asks();
    assert_eq!(asks.len(), 2, "两次拉起问了 {} 次", asks.len());
    assert_eq!(
        asks[0], asks[1],
        "两次拉起问的入参不同 ⇒ 入参里带了会话身份"
    );
    assert!(!r9.contains("sid-9"), "resume 的 sid 进了地址：{r9}");
}

/// 〔US1〕D11：**问不到那台后端 ⇒ 拒绝起会话并说清**（不退回「自己读文件」、不猜成「不注入」）。
/// 期望手写；正控：同一个号、替身答得了 ⇒ 不报错。
#[test]
fn a_launch_whose_backend_cannot_be_asked_is_refused_not_guessed() {
    fn not_win() -> bool {
        false
    }
    let acct = LaunchAccount::Named {
        config_dir: "/h/.claude-alt/acct-a".to_string(),
        name: None,
    };
    let action = LocalPsAction::Resume("sid-1".to_string());
    {
        let _g = override_inject_facts(InjectFactSources {
            endpoint: fake_ep_unreachable,
            windows: not_win,
            all_sessions: all_sessions_off,
        });
        let e = prefix_now(&action, Some(&acct)).expect_err("问不到却起出去了");
        assert!(
            e.contains("问不到本机后端") && e.contains("没有控制通道"),
            "{e}"
        );
    }
    let _g = override_inject_facts(InjectFactSources {
        endpoint: fake_ep,
        windows: not_win,
        all_sessions: all_sessions_off,
    });
    fake_table(&[], true);
    assert_eq!(prefix_now(&action, Some(&acct)).unwrap(), "");
}

/// ★★★ `D5 阻-1` 的第三格：**生产上插进那条缝的，就是那几个真取值口**（按函数地址，两跳都拍：
/// `const` 与没装替身时 `inject_facts()` 交出来的那一跳 —— 只拍 `const` 时，把 `inject_facts()` 改成「不装替身也回一份写死的」就绕过去了）。
#[test]
fn the_production_relay_facts_are_those_take_points() {
    fn not_it() -> bool {
        true
    }
    assert!(
        !std::ptr::fn_addr_eq(PRODUCTION_INJECT_FACTS.windows, not_it as fn() -> bool),
        "这把尺子对任何同型函数都说「是」—— 它恒真，本条按红处理"
    );
    let live = inject_facts();
    // 〔US1〕「那台后端答的成品」那一格：生产恒指 `ask_launch_endpoint`（经那条长连接问 `launch-endpoint`）。
    assert!(
        std::ptr::fn_addr_eq(live.endpoint, ask_launch_endpoint as EndpointAsk)
            && std::ptr::fn_addr_eq(PRODUCTION_INJECT_FACTS.endpoint, ask_launch_endpoint as EndpointAsk),
        "生产上「问那台后端要成品」不再是 `ask_launch_endpoint` —— 换成一份写死的答案，谁都不走中转 / 谁都走，行为判据看不见"
    );
    assert!(
        !std::ptr::fn_addr_eq(live.endpoint, fake_ep as EndpointAsk),
        "反空真：替身与生产那一跳分得开"
    );
    assert!(
        std::ptr::fn_addr_eq(live.windows, platform_is_windows as fn() -> bool)
            && std::ptr::fn_addr_eq(
                PRODUCTION_INJECT_FACTS.windows,
                platform_is_windows as fn() -> bool
            ),
        "生产上「这台机是不是 Windows」不再由 `platform_is_windows` 答"
    );
    assert!(
        std::ptr::fn_addr_eq(live.all_sessions, relay_all_sessions_switch as fn() -> bool)
            && std::ptr::fn_addr_eq(
                PRODUCTION_INJECT_FACTS.all_sessions,
                relay_all_sessions_switch as fn() -> bool
            ),
        "生产上「全量注入开关开没开」不再由 `relay_all_sessions_switch` 答"
    );
    // ★★ `D6 阻-1`：**送出去**那条缝同样按地址对拍。
    let sink = launch_sink();
    #[cfg(not(windows))]
    let production_sink =
        crate::launch::launch_local_posix as fn(&str, Option<&str>) -> Result<(), String>;
    #[cfg(windows)]
    let production_sink =
        crate::launch::launch_powershell_window as fn(&str, Option<&str>) -> Result<(), String>;
    assert!(
        std::ptr::fn_addr_eq(sink.0, production_sink)
            && std::ptr::fn_addr_eq(PRODUCTION_LAUNCH_SINK.0, production_sink),
        "没装替身时最后送出去的那一步不是 `launch::launch_local_posix` / `launch::launch_powershell_window`"
    );
}

/// ★★★ `K-R55`（09-11）：**本机 ccm 探测那条新缝，生产上插的就是那个真取值口。**
///
/// 上一条对拍的是中转那三格与送法；这一条是同一个形状的第四处 ——
/// [`CcmProbeSource`] 是本拍为了让
/// `a_launch_that_goes_through_the_relay_still_cannot_get_a_tmux_container`
/// 真去走生产那条路才开的，而**开一条缝就欠一条地址对拍**：
/// 缝一旦在生产上也指着一份写死的答案（比如「恒装着、能力全有」），
/// 那条行为判据**照绿**（它本来就装替身），而生产上「没装 ccm 要诚实降级」整条没了。
///
/// 两跳都拍，理由与上一条第二半逐字同一条：只拍 `const` 时，
/// 有人把 [`ccm_probe_source`] 改成「不装替身也回一份写死的」就绕过去了。
#[test]
#[cfg(not(windows))]
fn the_local_launch_really_asks_the_production_ccm_probe() {
    // 反空真排最前：这把尺子分得出「不是那个函数」，否则下面两条恒真。
    fn not_it() -> crate::ccm_probe::CcmProbeResult {
        crate::ccm_probe::CcmProbeResult {
            installed: false,
            version: None,
            capabilities: vec![],
            build: None,
        }
    }
    assert!(
        !std::ptr::fn_addr_eq(
            PRODUCTION_CCM_PROBE.0,
            not_it as fn() -> crate::ccm_probe::CcmProbeResult
        ),
        "这把尺子对任何同型函数都说「是」—— 它恒真，本条按红处理"
    );
    let production = crate::ccm_probe::probe_local_ccm as fn() -> crate::ccm_probe::CcmProbeResult;
    // ⚠ **刻意不装替身**（替身住 thread-local ⇒ 别的判据装的那份影响不到这里）。
    assert!(
        std::ptr::fn_addr_eq(ccm_probe_source().0, production),
        "没装替身时 `ccm_probe_source()` 交出来的不是 `ccm_probe::probe_local_ccm` ——\n\
             生产那一跳被换掉了，而只对拍那个 `const` 的判据看不见（第六层的形状）"
    );
    assert!(
        std::ptr::fn_addr_eq(PRODUCTION_CCM_PROBE.0, production),
        "生产上「这台机装没装 ccm、有哪些能力」不再由 `ccm_probe::probe_local_ccm` 答 ——\n\
             换成一份写死的「装着且能力全有」，没装 ccm 的机器会渲出一条带未知 flag 的命令，\n\
             而那时回落分支已经不在了（`render_local_ccm` 头注里那个 fail-open）"
    );
}

/// ★★ 〔C4c · US1〕**「configDir → 表里的 id」与「哪几个号在表里有行」两条规则只有一个家：`acct-core`**。
///
/// 要求住址：`设计/01 §5` D1「**一个判定只有一个家**」。〔US1〕monitor 那一个绑定（`history::apikey_routed_subset`〔散文墓碑〕）
/// 随界面那一问搬进后端一起删了 ⇒ 两棵生产树里两条规则的**定义**都零处；反空真：同一个识别器在 `acct-core` 上各数得出恰好 1。
#[test]
fn the_two_apikey_rules_are_defined_only_in_acct_core() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let defs = ["fn apikey_account_id_of_dir", "fn apikey_routed_subset"];
    let mut found = Vec::new();
    for dir in [root.join("src"), root.join("../backend")] {
        for (p, rule_src) in guard_core::scan_tree!(&dir, &["rs"]) {
            let rule_prod = guard_core::production_code(&rule_src);
            for d in defs {
                if guard_core::contains_word(&rule_prod, d) {
                    found.push((p.clone(), d));
                }
            }
        }
    }
    assert!(
        found.is_empty(),
        "两条 apikey 规则在两半里又长出了定义：{found:?}"
    );
    let core_prod = guard_core::production_code(
        &std::fs::read_to_string(root.join("crates/acct-core/src/lib.rs")).unwrap(),
    );
    for d in defs {
        guard_core::find_pinned(&core_prod, d)
            .unwrap_or_else(|e| panic!("acct-core 里 `{d}` 不是恰好一处：{e}"));
    }
}

/// ★★ `KH2B2`②在这一层（〔US1〕照成品执行）：`/s/`（`whenDown: refuse`）而中转不在 ⇒ **当场说话**；
/// `/t/`（`direct`）而中转不在 ⇒ **照旧直连**（空串）—— 两种降级不许写成一样（`设计/20 §3.2` ⚠）。
#[test]
fn a_launch_that_needs_the_relay_is_refused_when_the_relay_is_not_running() {
    fn not_win() -> bool {
        false
    }
    fn on() -> bool {
        true
    }
    let action = LocalPsAction::Resume("sid-1".to_string());
    let acct_a = LaunchAccount::Named {
        config_dir: "/h/.claude-alt/acct-a".to_string(),
        name: None,
    };
    let _g = override_inject_facts(InjectFactSources {
        endpoint: fake_ep,
        windows: not_win,
        all_sessions: on,
    });
    fake_table(&["acct-a"], false);
    let e = prefix_now(&action, Some(&acct_a)).expect_err("中转没起来却照旧渲染");
    assert!(
        e.contains("中转没在听") && e.contains(RELAY_NOT_LISTENING.as_str()),
        "错误得说出真正的原因：{e}"
    );
    // `/t/` 那一格（账号 0、开关开）同样不在 ⇒ 直连，不拒。
    assert_eq!(prefix_now(&action, Some(&LaunchAccount::Base)).unwrap(), "");
    // 非空对照：只把「中转在不在」翻过来，两格都注入。
    fake_table(&["acct-a"], true);
    assert!(prefix_now(&action, Some(&acct_a)).unwrap().contains("/s/"));
    assert!(prefix_now(&action, Some(&LaunchAccount::Base))
        .unwrap()
        .contains("/t/claude-code/0'"));
}

/// ★★★ **接线判据**：`launch_local` **真正交出去的那一串**以中转前缀打头。
///
/// # 🔴🔴🔴 它先前是一条文本判据，而 `D6` 的刀 `Y1` 把它打穿了（第七层）
///
/// 第一版量的是「从 `fn launch_local(` 起切 3600 字节，窗口里**有没有**这三样文本」：
/// ① `relay_prefix_for_launch(action, account)?` ② `relay + &` 恰好 2 处
/// ③ `let cmd = relay + &base;` 与另外两处的先后序。
/// 刀 `Y1` 在拼装那一行加了一句
/// `let relay = if relay.is_empty() { relay } else { String::new() };`
/// ⇒ 三样文本**一处不少**（三个锚点数与干净树逐字相同）
/// ⇒ **`1227 passed; 0 failed`、`GATE: OK`、四个数与干净树逐字相同**，
/// **而前缀算出来了没拼上去 —— 本件的正题整个被摘掉。**
/// ⚠ 而**上一版这段头注里逐字写着它要防的正是这件事**（「算出来却没拼上去，行为上与本件
/// 没做完全一样」）—— 威胁模型写对了，买的东西是文本。
/// ⚠ 它还带着 `D4 阻-1` 那一形：判据住 `history.rs::tests`，而它 `include_str!("../../src/bridge/src/history.rs")`
/// 扫的就是本文件 ⇒ `relay + &` 全仓 4 = 生产 2 + 本条的针 1 + 报文 1，按本仓变异纪律
/// 「锚点全改」会把针一起带走。
///
/// # ⇒ 换成量**真正送出去的那一串**
///
/// [`LaunchSink`] 那条缝让判据能装一个记账替身，于是本条量的不再是源码，是
/// **`launch_local` 最后交给送法的那个字符串**。三格，每格都能被一刀翻掉：
///
/// | 格 | 断的是什么 | 翻掉它的形状 |
/// |---|---|---|
/// | ① | 表里没这个号 ⇒ 那一串里**一个 `ANTHROPIC_BASE_URL` 都没有** | 无条件注入 |
/// | ② | 表里有 ⇒ 那一串**逐字节等于**「前缀 + ① 那一串」 | 刀 `Y1`（算了没拼）· 掏空注入点 |
/// | ③ | 换一个号 ⇒ 前缀里的 `<account>` 段跟着变 | 刀 `E6`（账号段写死成常量） |
/// | ⑤ | **新开会话**那一支也带前缀 | 刀 `S2`（`New => Ok(String::new())`） |
/// | ⑥ | 换一个 sid ⇒ `<key>` 段跟着变 | 刀 `S1`（`Resume(_sid) => Some("sid-1")`） |
///
/// 🔴 ⑤⑥ 是 `D7 阻-4` 补的：先前 ①–④ **全部走 `Resume("sid-1")`**
/// ⇒ `action` 这一维的输入域是 1，而**新开会话是 `KH2B6` 逐字写着「现在生产可达」的一条路**。
///
/// # ⚠ 它买不到什么（如实写）
///
/// - **走 ccm 容器那一支**：本条喂 `tmux_name = None` ⇒ 走的是回落那条路。
///   而按 `a_launch_that_goes_through_the_relay_still_cannot_get_a_tmux_container`，
///   **带中转前缀的拉起今天必然落到回落路** ⇒ 本条驱动的正是那条生产可达的路。
///   ccm 那一支上「前缀有没有拼」由同一行代码管（合流之后**只有一处**拼接）。
/// - **送法自己拿到串之后干了什么**：那是 `launch::launch_local_posix` 自己的判据面；
///   「生产上插进这条缝的就是它」由 `the_production_relay_facts_are_those_take_points`
///   末尾那一格按**函数地址**对拍。
/// - **谁绕开这条缝直接调送法**：由 `payload.rs` 那道人群闸数着（零调用点）。
#[test]
fn the_relay_prefix_is_really_prepended_to_the_command_that_gets_launched() {
    use std::cell::RefCell;
    thread_local! {
        static SENT: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    }
    fn recorder(cmd: &str, _cwd: Option<&str>) -> Result<(), String> {
        SENT.with(|v| v.borrow_mut().push(cmd.to_string()));
        Ok(())
    }
    fn answer(rows: &[&str], running: bool) {
        fake_table(rows, running);
    }
    fn last_sent() -> String {
        SENT.with(|v| v.borrow().last().cloned().expect("这一趟什么都没送出去"))
    }

    let _sink = override_launch_sink(LaunchSink(recorder));
    let _facts = override_inject_facts(InjectFactSources {
        endpoint: fake_ep,
        // 平台那一格照生产那个取值口（本条不翻它 —— 翻它的是上面那条判据的第 ④ 格）。
        windows: platform_is_windows,
        all_sessions: all_sessions_off,
    });

    let action = LocalPsAction::Resume("sid-1".to_string());
    let accounts = [
        ("acct-a", "/h/.claude-alt/acct-a"),
        // 🔴 `D6 阻-2`：**两个号**，不是一个 —— 「这次拉起是哪个号」也要能翻。
        ("acct-b", "/h/.claude-alt/acct-b"),
    ];

    let mut with_relay = Vec::new();
    for (id, dir) in accounts {
        let account = LaunchAccount::Named {
            config_dir: dir.to_string(),
            name: None,
        };
        // ① 表里没有这个号 ⇒ 逐字节旧路。这一趟同时是下面那条相等断言的**基准串**。
        answer(&[], true);
        launch_now(&action, None, None, Some(&account), None).expect("不走中转这一趟不该失败");
        let bare = last_sent();
        assert!(
            !bare.is_empty() && bare.contains(dir),
            "基准串不像一条本机拉起命令（连这个号的 configDir 都没有）：{bare:?}"
        );
        assert!(
            !bare.contains("ANTHROPIC_BASE_URL"),
            "表里没有这个号，送出去的那一串却带着中转注入 —— \
                 `KH2B5`「没配第三方 key 的号一个字节都不受影响」当场破了：{bare:?}"
        );

        // ② 表里有这个号 ⇒ 送出去的那一串**逐字节等于**「前缀 + 基准串」。
        answer(&["acct-a", "acct-b"], true);
        // 〔US1〕期望的前缀 = 纯函数拿「那台后端该答的那个地址」渲出来的（地址手写，不由被测函数现算）。
        let prefix = relay_prefix_for(
            Some(&format!("http://127.0.0.1:8788/s/claude-code/{id}")),
            cfg!(windows),
        );
        // 反空真：期望的前缀本来就该是非空的，否则下面那条相等断言是「x == x」。
        assert!(!prefix.is_empty(), "期望前缀是空串 —— 本条按红处理");
        launch_now(&action, None, None, Some(&account), None).expect("走中转这一趟不该失败");
        let routed = last_sent();
        assert_eq!(
            routed,
            format!("{prefix}{bare}"),
            "\n★★ **算出来了没拼上去** —— 这正是 `D6` 刀 `Y1` 的形状：\n\
                 `relay_prefix_for_launch` 照样被调、照样答对，而拼装那一行把它扔了\n\
                 ⇒ 起会话的命令串里没有 `ANTHROPIC_BASE_URL`，本件的正题整个被摘掉，\n\
                 **而上一版那条数三样文本的判据照绿。**\n\
                 号 = {id} · 实得 = {routed:?} · 期望 = {:?}",
            format!("{prefix}{bare}")
        );
        with_relay.push((id, routed));
    }

    // ③ 🔴 `D6 阻-2`：**两个号送出去的前缀必须不一样**，而且各自带自己的账号段。
    let (id_a, cmd_a) = &with_relay[0];
    let (id_b, cmd_b) = &with_relay[1];
    assert!(
        cmd_a.contains(&format!("/{id_a}'")) && cmd_b.contains(&format!("/{id_b}'")),
        "\n路由键里的账号段不是这次拉起的那个号 —— 刀 `E6` 的形状：\n\
             把 `apikey_account_id` 的答案 `.map(|_| \"acct-a\")` 写死，\n\
             生产后果是 **acct-b 的会话拿着 acct-a 的那把 key 发请求，两边都显示成功**。\n\
             实得：{cmd_a:?} · {cmd_b:?}"
    );
    assert_ne!(
        cmd_a.split("; ").next(),
        cmd_b.split("; ").next(),
        "两个号送出去的第一段（中转前缀）逐字节相同 —— 「哪个号」这一维成了常量"
    );

    // ④ 中转没在跑 ⇒ **当场拒，而且一个字节都没送出去**（`KH2B2`②：不许静默）。
    let before = SENT.with(|v| v.borrow().len());
    answer(&["acct-a"], false);
    let account = LaunchAccount::Named {
        config_dir: "/h/.claude-alt/acct-a".to_string(),
        name: None,
    };
    assert!(
        launch_now(&action, None, None, Some(&account), None).is_err(),
        "中转没在跑却照旧起出去了"
    );
    assert_eq!(
        SENT.with(|v| v.borrow().len()),
        before,
        "拒了却还是往外送了一条命令 —— 那条「当场拒」只拒在返回值上"
    );

    // ═══════════════════════════════════════════════════════════════════
    // ⑤⑥ 🔴 `D7 阻-4`：**「哪一次拉起」这一维，在送出去的那一串上也要翻得动**
    // ═══════════════════════════════════════════════════════════════════
    //
    // 上面 ①–④ 全部走 `Resume("sid-1")` ⇒ `action` 这一维的输入域 = 1。
    // 刀 `S2`（`New => return Ok(String::new())`）在**这条判据上也全绿**，
    // 而**新开会话是 `KH2B6` 逐字写着「现在生产可达」的一条路**。
    answer(&["acct-a"], true);
    let acct = LaunchAccount::Named {
        config_dir: "/h/.claude-alt/acct-a".to_string(),
        name: None,
    };

    // ⑤ **`New` 那一支**：送出去的那一串必须也带中转注入，且账号段是这次的号。
    launch_now(&LocalPsAction::New, None, None, Some(&acct), None)
        .expect("新开会话走中转这一趟不该失败");
    let new_sent = last_sent();
    assert!(
        new_sent.contains("ANTHROPIC_BASE_URL") && new_sent.contains("/acct-a'"),
        "\n★★ **新开会话送出去的那一串里没有中转注入** —— 刀 `S2` 的形状：\n\
             `LocalPsAction::New => return Ok(String::new())`。\n\
             生产后果：一个 api-key 号**新开**一个会话 ⇒ claude 直连官方端点、\n\
             第三方 key 用不上，而全量门禁四个数一格不动。实得 = {new_sent:?}"
    );

    // ⑥ **`Resume` 那一支的 sid 不是写死的**：换一个 sid，送出去的那一串跟着变。
    launch_now(
        &LocalPsAction::Resume("sid-9".to_string()),
        None,
        None,
        Some(&acct),
        None,
    )
    .expect("这一趟不该失败");
    let resumed_9 = last_sent();
    // 〔V141〕sid 只在命令体里（`--resume`），不在中转地址里。
    let (relay_part, body_part) = resumed_9.split_once("; ").unwrap_or_default();
    assert!(
        body_part.contains("sid-9") && !relay_part.contains("sid-9"),
        "\n换一个会话 id，送出去的那一串没跟着变（刀 `S1` 的形状：`Resume(_sid) => Some(\"sid-1\")`），\n\
             或 sid 进了中转地址（V141）。实得 = {resumed_9:?}"
    );
    // 反空真：这两趟本来就该是两条不同的串（否则上面两条里有一条在数同一份东西）。
    assert_ne!(
        new_sent, resumed_9,
        "新开与 resume 送出去的是同一串 —— 「哪一次拉起」这一维成了常量"
    );
}

// ═════════════════════════════════════════════════════════════════════════
// 🔴 `K-P5b` `KP5BD1`：**起会话方把身份塞进了下一跳的进程环境**
// ═════════════════════════════════════════════════════════════════════════

/// 从送出去的那一串里，把身份那一段（到第一个 `; ` 为止）抠出来。
///
/// ⚠ 用**变量名**定位，不用位置定位：位置是会变的（今天身份那一段前面还有中转前缀），
/// 而「哪一段是身份」这件事只有变量名说得准。
fn identity_segment(cmd: &str) -> Option<&str> {
    let i = cmd.find(LAUNCH_ID_VAR)?;
    let seg = &cmd[i..];
    Some(&seg[..seg.find("; ").unwrap_or(seg.len())])
}

/// ★★★ `KP5BD1`：`launch_local` **真正交出去的那一串**里，身份被塞进了进程环境。
///
/// # 它量的是行为，不是文本（这条纪律是 `D6` 刀 `Y1` 花一整轮买回来的）
///
/// 量文本那一形已经在本文件里被打穿过一次：`relay_prefix_for_launch` 照样被调、
/// 答案照样对，而拼装那一行把它扔了 ⇒ 三个文本锚点一处不少、全量门禁四个数与干净树逐字相同。
/// ⇒ 本条一个字节的源码都不扫，只看 [`LaunchSink`] 那条缝上**真正交出去的那个字符串**。
///
/// # 五格，每格能被哪一刀翻掉
///
/// | 格 | 断的是什么 | 翻掉它的形状 |
/// |---|---|---|
/// | ① | 送出去的那一串里**有** `CCM_LAUNCH_ID=<sid>` 这一句 | 把 `+ &identity.prefix` 从拼装那一行删掉（刀 `Y1` 同形） |
/// | ② | 换一个 sid ⇒ 那一段跟着变 | `Resume(_) => Some("sid-1")`（身份写死） |
/// | ③ | **新开**那一支也有身份，且两趟 token 不同 | `New => String::new()`（只给 resume 落身份 —— 而 `K-P5 §3 三` 现打的正是「新开那一支没有 sid」，它才是本件的正主） |
/// | ④ | **铸法是共用那一份**：喂一个过不了白名单的 sid ⇒ token **不是**那个 sid | 在本文件里另写一份 `match { Resume(s) => s.clone(), … }`（第二份铸法，白名单回落那一格丢了） |
/// | ⑤ | 平台那一维翻得动：`windows = true` ⇒ 渲成 `$env:` 形态 | 把 `windows` 那一格写死（`D6` 刀 `Xb` 同形，生产后果是 Windows 上塞出一句 POSIX `export`） |
///
/// # ⚠ 它买不到什么（如实写，别读宽）
///
/// - **走 ccm 容器那一支身份到不到得了 agent 进程**：到不了。本条喂 `tmux_name = None`
///   ⇒ 走的是回落那条路（渲染器早退，见 [`NO_TMUX_NAME`]）。容器那一支上外侧这句 `export`
///   会在 tmux 边界被吃掉（与 `K-H2b` 给 `ANTHROPIC_BASE_URL` 踩过的**同一个坑**，
///   那一次的修法是在容器载荷内侧补一句转发）—— 当时那份 bash `ccm` 是红线文件，
///   那一句没补 ⇒ 这一格当时是个洞，登记在 `launcher_identity_registry` 的 `L1` 那一行里。
///   🔴 〔`K-R61` 09-11 现打〕`src/backend/control/ccm/plan.rs` 的容器路
///   **今天有** `export CCM_LAUNCH_ID=…` 那一句 ⇒ **那个洞的成因很可能已经不在了**。
///   但「洞补没补上」的落点是 `launcher_identity_registry` 的 `L1`，**不在 `K-R61` 写区**，
///   本轮**没有**去重裁它 —— 已报回 PM。在有人重裁之前，别把这一段读成「已经全覆盖」。
/// - **读的那一侧**：backend 从 `/proc/<pid>/environ` 读回来、经 wire 帧发出去 —— 本拍**没有做**
///   （面在 `src/backend/`，不在本拍写区）。⇒ 今天这个变量**有人写、没人读**。
/// - **Windows 上的运行时行为**：一行都没量（这台机器是 Linux）。⑤ 买到的只是
///   「平台那一格翻得动、渲出来的形态跟着变」，不是「PowerShell 里真的设上了」。
#[test]
fn the_launcher_plants_the_session_identity_into_the_process_environment() {
    use std::cell::{Cell, RefCell};
    thread_local! {
        static SENT: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
        static WINDOWS_ANSWER: Cell<bool> = const { Cell::new(false) };
    }
    fn recorder(cmd: &str, _cwd: Option<&str>) -> Result<(), String> {
        SENT.with(|v| v.borrow_mut().push(cmd.to_string()));
        Ok(())
    }
    fn spy_windows() -> bool {
        WINDOWS_ANSWER.with(Cell::get)
    }
    fn last_sent() -> String {
        SENT.with(|v| v.borrow().last().cloned().expect("这一趟什么都没送出去"))
    }

    let _sink = override_launch_sink(LaunchSink(recorder));
    // 表里一行都没有 ⇒ 中转前缀恒空 ⇒ 本条量到的只有身份那一段（两件事分开量）。
    fake_table(&[], true);
    let _facts = override_inject_facts(InjectFactSources {
        endpoint: fake_ep,
        windows: spy_windows,
        all_sessions: all_sessions_off,
    });
    let account = LaunchAccount::Named {
        config_dir: "/h/.claude-alt/acct-a".to_string(),
        name: None,
    };

    // ① resume：身份就是这条会话的 sid，逐字节。
    let sid = "0198f0d2-1111-4222-8333-444455556666";
    launch_now(
        &LocalPsAction::Resume(sid.to_string()),
        None,
        None,
        Some(&account),
        None,
    )
    .expect("这一趟不该失败");
    let first = last_sent();
    assert!(
        !first.contains("ANTHROPIC_BASE_URL"),
        "表里一行都没有，却混进了中转前缀 —— 本条的替身没装上，下面几格量的不是身份：{first:?}"
    );
    let want_first = format!("{LAUNCH_ID_VAR}='{sid}'");
    assert_eq!(
        identity_segment(&first),
        Some(want_first.as_str()),
        "\n★★ **起会话方没把身份塞进进程环境** —— 刀的形状是把\n\
             `+ &identity.prefix` 从拼装那一行删掉（`D6` 刀 `Y1` 同形：\n\
             token 照样铸得出来，只是没拼上去）。\n\
             生产后果：起出来的那条会话**在环境里说不出自己是谁**，\n\
             读的那一侧只能退回去扫窗口标题 —— 那正是本件要消灭的东西。\n\
             实得整串 = {first:?}"
    );

    // ② 换一个 sid ⇒ 身份那一段跟着变（不是常量）。
    let sid9 = "0198f0d2-9999-4222-8333-444455556666";
    launch_now(
        &LocalPsAction::Resume(sid9.to_string()),
        None,
        None,
        Some(&account),
        None,
    )
    .expect("这一趟不该失败");
    let second = last_sent();
    let want_second = format!("{LAUNCH_ID_VAR}='{sid9}'");
    assert_eq!(
        identity_segment(&second),
        Some(want_second.as_str()),
        "换一条会话，环境里的身份没跟着变 —— 「这条会话是谁」成了常量：{second:?}"
    );

    // ③ **新开**那一支也落身份，而且两趟拿到的是两个不同的 nonce。
    //   `K-P5 §3 三` 现打：5 个起会话方**没有一处**在起新会话时知道 sid
    //   ⇒ 新开这一支才是本件的正主，它落不落身份不能靠 resume 那一支代言。
    launch_now(&LocalPsAction::New, None, None, Some(&account), None).expect("新开这一趟不该失败");
    let new_a = identity_segment(&last_sent())
        .expect("新开那一支送出去的串里没有身份 —— `New => String::new()` 那一刀的形状")
        .to_string();
    launch_now(&LocalPsAction::New, None, None, Some(&account), None).expect("新开这一趟不该失败");
    let new_b = identity_segment(&last_sent()).expect("同上").to_string();
    assert_ne!(
        new_a, new_b,
        "两次新开拿到同一个身份 —— nonce 成了常量，两条会话在环境里说自己是同一个人"
    );
    assert!(
        !new_a.contains(sid) && !new_a.contains(sid9),
        "新开那一支把上一条 resume 的 sid 当成了自己的身份：{new_a:?}"
    );

    // ④ **铸法是共用那一份** —— 〔DUP1〕这一格原来喂一个「过得了本机拉起那道 sid 校验、过不了中转段白名单」的 sid
    //   （129 字节纯字母：旧校验无长度上限），看身份会不会回落到 nonce（共用那份铸法 `payload::route_key_for_session` 会，
    //   第二份实现不会）。sid 规则收进 `shell_quote_core::session_id_ok`（`设计/90 §3` 判据 2 · `INVARIANTS §47` ①，1..=64）之后，
    //   **本机拉起收得进来的 sid 全都过得了中转段白名单** ⇒ 那一支回落在这条路上不可达，这个行为判别器没了。
    //   「铸法只有一份」今天由 `launcher_identity_registry_tests.rs` 数 `route_key_for_session(` 的调用点那条结构判据守着；
    //   这里改钉两件事：那个 129 字节的 sid **在拉起那一关就被拒**（新规则确实更严），以及「收得进来 ⇒ 中转段安全」这个包含关系。
    let bad = "a".repeat(129);
    assert!(
        launch_now(
            &LocalPsAction::Resume(bad.clone()),
            None,
            None,
            Some(&account),
            None,
        )
        .is_err(),
        "129 字节的 sid 过了本机拉起那一关 —— sid 规则没接到共享那一份"
    );
    for ok in [
        sid,
        sid9,
        "a",
        &"a".repeat(shell_quote_core::SESSION_ID_MAX),
    ] {
        assert!(
            shell_quote_core::session_id_ok(ok),
            "样本自己不合法：{ok:?}"
        );
        assert!(
            relay_route_core::segment_is_safe(ok),
            "拉起收得进来的 sid {ok:?} 过不了中转段白名单 —— 包含关系破了，④ 原来那一格又变得可达（该把它加回来）"
        );
    }

    // ⑤ 平台那一维翻得动：`windows = true` ⇒ 渲成 PowerShell 形态。
    //   写死那一格的生产后果是 Windows 上往 PowerShell 串里塞一句 POSIX `export`
    //   ⇒ 身份注入整个失效（`D6` 刀 `Xb` 在中转那一格上的同一形）。
    WINDOWS_ANSWER.with(|c| c.set(true));
    launch_now(
        &LocalPsAction::Resume(sid.to_string()),
        None,
        None,
        Some(&account),
        None,
    )
    .expect("这一趟不该失败");
    let ps = last_sent();
    assert!(
        ps.contains(&format!("$env:{LAUNCH_ID_VAR}='{sid}'; ")),
        "\n把「这台机是不是 Windows」翻成 true，身份那一句还是 POSIX 形态 ——\n\
             那一格是个常量，Windows 上会往 PowerShell 串里塞一句 `export`。实得 = {ps:?}"
    );
    // 反空真：POSIX 那一趟本来就该拿不到这个形状（否则上面那条恒真）。
    //
    // ⚠ **09-09 订正（云端 windows-latest 首跑逮到）**：这里原写
    //   `!first.contains("$env:")` —— 断的是**整串**。而 `launch_local` 里 `base`
    //   那一格是 `#[cfg(windows)]` 选的（它**不走**这条缝），Windows 上它渲出
    //   `$env:CLAUDE_CONFIG_DIR='…'; ` ⇒ 原断言在 Windows 上**恒假**，
    //   量的根本不是身份那一段。
    //   ⇒ 收窄到**只盯身份那一段**，并补一条**正**的：POSIX 那一趟必须真的渲出
    //   `export <变量名>=`。两条合起来比原来那一条**更严** ——
    //   原写法在「身份那一段整个不见了」时是绿的。
    assert!(
        first.contains(&format!("export {LAUNCH_ID_VAR}=")),
        "POSIX 那一趟没渲出 `export {LAUNCH_ID_VAR}=` —— 上面那条断言是恒真的：{first:?}"
    );
    assert!(
        !first.contains(&format!("$env:{LAUNCH_ID_VAR}=")),
        "POSIX 那一趟把身份渲成了 `$env:` —— 上面那条断言是恒真的：{first:?}"
    );
}

// ═════════════════════════════════════════════════════════════════════════
// 🔴🔴 `K-P5h` `KP5HD1`：**铸出来的那个 token 真的交到了调用方手上**
// ═════════════════════════════════════════════════════════════════════════

/// ★★★ `KP5HD1`：[`launch_local`] / [`new_local_session`] 回的那个串，
/// **就是塞进那次拉起进程环境里的同一个 token**；而拼出来的命令串**一个字节没变**。
///
/// # 它与旁边那条老判据的分工（两条都要，别合并）
///
/// [`the_launcher_plants_the_session_identity_into_the_process_environment`] 买的是
/// 「**塞进去了**」；本条买的是「**交出来了**」。`K-P5g` 交回时现打的卡点逐字是
/// 「写侧把 token 铸完就扔」—— 那一天上面那条老判据**全绿**，
/// 因为塞进去这件事一直是对的，缺的是**没有任何调用方手上有那个 token**。
/// ⇒ 两件事各自要有自己的牙。
///
/// # 五格，每格能被哪一刀翻掉
///
/// | 格 | 断的是什么 | 翻掉它的形状 |
/// |---|---|---|
/// | ① | 回的那个串**逐字节**是命令里 `CCM_LAUNCH_ID=` 后面那个值 | `Ok("x".into())`（回一个常量）· `Ok(identity.prefix)`（回错那一半） |
/// | ② | 两趟**新开**回的是两个不同的 token | 同上那个常量刀（`KP5HD1` 的死值验逐字点名的就是它） |
/// | ③ | resume 那一支回的是 sid 本身 | 把 `New`/`Resume` 两支的返回值接反 |
/// | ④ | **additive**：交出来这件事没改动命令串 —— 送出去的那一串逐字节等于 `前缀 + 基准串` | 在拼装那一行顺手动一下（多拼 / 少拼 / 换序，`D6` 刀 `Y1` 那一族） |
/// | ⑤ | 拉起**失败**时不回 token | 把 `?` 换成忽略错误（那会让调用方去等一条不存在的会话） |
///
/// # ⚠ 它买不到什么（如实写）
///
/// - **拿这个 token 真能反查出 sid**：那要一条真的跑起来的会话 + 一个真后端。
///   本条只买到「token 到了调用方手上」，反查那一跳的判据在前端
///   （`tests/accounts.vitest.ts` 的 `K-P5h` 那一组，`KP5HD2`）。
/// - **走 ccm 容器那一支**：与老判据同一个洞（喂 `tmux_name = None` ⇒ 走回落那条路），
///   登记在 `launcher_identity_registry` 的 `L1` 那一行里。
/// - **Windows 上的运行时行为**：④ 那一格按平台各自取基准串，但这台机器是 Linux，
///   PowerShell 那一侧一行都没真跑过。
#[test]
fn the_minted_identity_token_is_handed_back_to_the_caller() {
    use std::cell::RefCell;
    thread_local! {
        static SENT2: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    }
    fn recorder(cmd: &str, _cwd: Option<&str>) -> Result<(), String> {
        SENT2.with(|v| v.borrow_mut().push(cmd.to_string()));
        Ok(())
    }
    fn boom(_cmd: &str, _cwd: Option<&str>) -> Result<(), String> {
        Err("拉起失败（判据夹具）".to_string())
    }
    fn not_windows() -> bool {
        false
    }
    fn last_sent() -> String {
        SENT2.with(|v| v.borrow().last().cloned().expect("这一趟什么都没送出去"))
    }

    let _sink = override_launch_sink(LaunchSink(recorder));
    // 表里一行都没有 ⇒ 中转前缀恒空 ⇒ ④ 那一格量的是「身份 + 基准串」这两段，
    // 中转那一段由它自己那条判据管（两件事分开量）。
    fake_table(&[], true);
    let _facts = override_inject_facts(InjectFactSources {
        endpoint: fake_ep,
        windows: not_windows,
        all_sessions: all_sessions_off,
    });
    let account = LaunchAccount::Named {
        config_dir: "/h/.claude-alt/acct-a".to_string(),
        name: None,
    };

    // ① **新开**那一支：回的那个串就是命令里那个值。
    //    ⚠ 这里刻意**不**拿 `identity_segment` 的整段去比 —— 那样只要回的是
    //    「`CCM_LAUNCH_ID=…` 这一整句」就绿了，而本条要的是**值本身**。
    let token_a = launch_now(&LocalPsAction::New, None, None, Some(&account), None)
        .expect("新开这一趟不该失败");
    assert!(
        !token_a.trim().is_empty(),
        "新开那一趟回了个空串 —— 「交出来」这一步等于没做"
    );
    let sent_a = last_sent();
    let want_a = format!("{LAUNCH_ID_VAR}='{token_a}'");
    assert_eq!(
        identity_segment(&sent_a),
        Some(want_a.as_str()),
        "\n★★ **交回来的 token 不是塞进环境里的那一个。**\n\
             刀的形状：`Ok(\"x\".into())`（回一个常量）或 `Ok(identity.prefix)`（回错那一半）。\n\
             生产后果：起会话方拿着一个**谁也不认识**的串去反查 sid ⇒ 永远查不到，\n\
             而「查不到就不猜」会让整条回填静默失效 —— 与本件没做完全一样。\n\
             交回来的 = {token_a:?}，送出去的整串 = {sent_a:?}"
    );

    // ② 两趟新开 ⇒ 两个**不同**的 token（常量刀在这里也红一次，两格互为纵深）。
    let token_b = launch_now(&LocalPsAction::New, None, None, Some(&account), None)
        .expect("新开这一趟不该失败");
    assert_ne!(
        token_a, token_b,
        "两趟新开交回来的是同一个 token —— 「交出来」那一步回的是常量，\n\
             而 `KP5HD1` 的死值验逐字点名的就是这一刀"
    );

    // ③ resume 那一支：token 就是 sid 本身（`route_key_for_session(Some(sid))` 原样返回）。
    //    ⚠ 这一格**不是**本件的正主（`K-P5g` 现打过它会退化成布尔谓词），
    //    写在这里只为钉住「两支没接反」。
    let sid = "0198f0d2-1111-4222-8333-444455556666";
    let token_r = launch_now(
        &LocalPsAction::Resume(sid.to_string()),
        None,
        None,
        Some(&account),
        None,
    )
    .expect("这一趟不该失败");
    assert_eq!(
        token_r, sid,
        "resume 那一支交回来的不是 sid —— 两支的返回值接反了"
    );

    // ④ ★★ **additive**：送出去的那一串逐字节 = 「身份那一句 + 基准串」。
    //    两边都由**生产函数现算**，本条不抄第二份拼装规则 ——
    //    抄一份的话，改了生产那一行、判据跟着抄错，两边一起错还全绿。
    {
        let act = LocalPsAction::Resume(sid.to_string());
        #[cfg(windows)]
        let base = build_local_ps_command(&act, None, Some(&account))
            .expect("基准串算不出来，④ 这一格是空真");
        #[cfg(not(windows))]
        let base = build_local_posix_command(&act, None, Some(&account))
            .expect("基准串算不出来，④ 这一格是空真");
        let want = format!("{}{base}", launch_identity_env_prefix(&token_r, false));
        let got = last_sent();
        // 反空真：基准串不是空的（空的话下面那条就退化成「送出去的等于身份那一句」）。
        assert!(
            base.len() > 10,
            "基准串只有 {} 字节 —— ④ 这一格在拿一个空壳对拍",
            base.len()
        );
        assert_eq!(
            got, want,
            "\n★★ **「把 token 交出来」这一拍改动了拼出来的命令串** —— \
                 而 `KP5HD1` 逐字要求「拼出来的命令串一个字节没变」。\n\
                 additive 的全部含义就是这一行；两边都是生产函数现算的，\n\
                 对不上说明拼装那一行被动过（多拼 / 少拼 / 换序，`D6` 刀 `Y1` 那一族）。"
        );
    }

    // ⑤ 拉起**失败**时不回 token —— 回了会让调用方去等一条根本不存在的会话。
    let _boom = override_launch_sink(LaunchSink(boom));
    let failed = launch_now(&LocalPsAction::New, None, None, Some(&account), None);
    assert!(
        failed.is_err(),
        "拉起失败了却回了 `Ok` —— 失败被吞掉，调用方会去等一条不存在的会话：{failed:?}"
    );
}

// ═════════════════════════════════════════════════════════════════════════
// 🔴🔴 `D8 阻-1`：**账号传递链那三跳** —— 生产主路，第九轮之前一格判据都没有
// ═════════════════════════════════════════════════════════════════════════
//
// # 病是怎么长出来的（`D8 §4` 第 2 条，别只读结论）
//
// 本件所有承重的行为判据（[`the_relay_prefix_is_really_prepended_to_the_command_that_gets_launched`]
// / [`the_launch_side_really_asks_the_backend_and_uses_its_answer`]）的
// **驱动入口都是 [`launch_local`] 或更下游**，而**生产入口在它上面两跳**：
//
// ```text
// #[tauri::command] resume_history_session  →  resume_impl  →  launch_local
// #[tauri::command] new_local_session       ─────────────────→  launch_local
// ```
//
// ⇒ **判据的射程上界正好卡在 `launch_local`，而九轮买的那些牙全长在它的下游。**
// `D8` 三刀实打（三处调用点各写一次 `account.filter(|_| false)`）：
// **全量门禁四个数一格不动（`1328 / 一致 / 493 / 1512`）、`GATE: OK`，而中转前缀恒空。**
//
// 🔴 **而看起来在守它的那把尺子，作用域对不上事实**：`tests/ipc/commands.vitest.ts:402`
//「每一处起本机会话的调用都带 `account`」守的是 **TS 那一侧**（`D8` 的 `E4` 实测：
// 在前端调用点上下同一形状的刀，`npm` 那道门当场红）。
// **同一根链的 Rust 这一侧三跳，一格都没守。** —— 「两条路只修了一条」。
//
// # 为什么是**三条**判据而不是一条（`K22` 的口径）
//
// **N 支信号要 N 个只由这一支挡住的探针。** 三跳各自能独立答错，所以三刀的**红名单
// 必须两两不同**（本轮实打的三张红名单在件文件 `§12` 的变异表里逐行给了）：
//
// | 刀 | 落在哪一处 | 红名单 |
// |---|---|---|
// | ① | `resume_impl` 里那一处 `account` | 探针 ① **与** ② 一起红 |
// | ② | `resume_history_session` 里那一处 `account.as_ref()` | **只有**探针 ② 红 |
// | ③ | `new_local_session` 里那一处 `account.as_ref()` | **只有**探针 ③ 红 |
//
// ⚠ 探针 ② 在刀 ① 上也红，是**链的包含关系**（②的驱动路径经过①），不是重复计数：
// 三张红名单两两不同 ⇒ 三刀可区分 ⇒ **三格**。反过来只写一条探针 ② 的话，
// ①②两刀的红名单会相同 ⇒ 那才是「N 支只买了一格」。
//
// # ⚠ 它们**买不到**什么（如实写）
//
// - **前端到底传没传 `account`**：那是 `commands.vitest.ts:402` 那把尺子的面，本族只管
//   「传进来之后 Rust 这一侧有没有原样送到拼前缀那一行」。**两把尺子各守一侧，别只改一处。**
// - **`launch_local` 以下的任何一格**：那是上面那条 `…_is_really_prepended_…` 的面。
//   本族刻意**不**重复买它 —— 三条探针的反空真只断「不走中转那一趟长得像一条真拉起」。
// - **Windows 那条腿**：`PRODUCTION_LAUNCH_SINK` 在 Windows 上是另一个送法，
//   而本族喂的是记账替身 ⇒ **送法**那一格三条探针一格都驱动不到（登记，不假装）。
//   〔09-09 订正：这里原写「而门禁跑在 Linux ⇒ 本族与本文件其余判据同样只驱动
//    POSIX 那一支」—— **那半句今天是假话**。云端 `Rust lint + test` 跑在
//    windows-latest，而 `launch_local` 里 `base` 那一格是 `#[cfg(windows)]` 选的
//    ⇒ 本族在 CI 上驱动的是 **PowerShell** 那一支，在开发机上才是 POSIX 那一支。〕

/// `D8 阻-1` 三支探针共用的记账台：这一趟真正交出去的 `(命令串, cwd)`。
///
/// 〔TL3 · 🔴-2〕**跨线程**（先前是线程局部）：三条入口今天是 `async` 命令，同步那一截在 `spawn_blocking`
/// 的线程上把串交给送法 ⇒ 记账台得在那条线程上也记得到。三条探针共用它 ⇒ 由 [`ENTRY_TURN`] 排队、一次一条。
static ENTRY_SENT: std::sync::Mutex<Vec<(String, Option<String>)>> =
    std::sync::Mutex::new(Vec::new());
/// 三条探针轮流用记账台（`cargo test` 多线程并行跑；上一条炸了也不连坐 ⇒ 毒化照取）。
static ENTRY_TURN: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn entry_sent() -> std::sync::MutexGuard<'static, Vec<(String, Option<String>)>> {
    ENTRY_SENT
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn entry_recorder(cmd: &str, cwd: Option<&str>) -> Result<(), String> {
    entry_sent().push((cmd.to_string(), cwd.map(str::to_string)));
    Ok(())
}

/// 装台：一条会记账的送法 + 一组答案由探针写死的取值口 ＋ 记账台的这一轮。三个守卫掉出作用域自动还原 / 放手。
fn entry_stage() -> (
    LaunchSinkGuard,
    InjectFactsGuard,
    std::sync::MutexGuard<'static, ()>,
) {
    let turn = ENTRY_TURN
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    entry_sent().clear();
    fake_table(&[], true);
    (
        override_launch_sink(LaunchSink(entry_recorder)),
        override_inject_facts(InjectFactSources {
            endpoint: fake_ep,
            // 平台那一格照生产那个取值口（翻它的是别处那条判据的第 ④ 格）。
            windows: platform_is_windows,
            all_sessions: all_sessions_off,
        }),
        turn,
    )
}
fn entry_answer(rows: &[&str]) {
    fake_table(rows, true);
}
fn entry_last() -> (String, Option<String>) {
    entry_sent().last().cloned().expect("这一趟什么都没送出去")
}

/// ★★ **探针 ①**〔`D8 阻-1`，`KH2B1`〕：`resume_impl` 这一跳把 `account` / `session_id` / `cwd`
/// **原样**交给 [`launch_local`]。
///
/// 刀 `D8P32`（`resume_impl` 里那一行 `account,` 写成 `account.filter(|_| false)`）
/// 在本条落地之前是**全量门禁四个数一格不动**的。
/// ⚠ 行号带尖号（`D8 阻-5` 的纪律）：`c02d954` 上是 `:1826`，本尖上是 `:1856`；
/// **锚点用文本别用行号** —— `^        account,$` 在本文件全文恰好 **1** 处。
#[test]
fn the_resume_hop_above_launch_local_carries_the_account_and_the_sid_through() {
    let (_sink, _facts, _turn) = entry_stage();
    let dir = "/h/.claude-alt/acct-r1";
    let account = LaunchAccount::Named {
        config_dir: dir.to_string(),
        name: None,
    };
    // 中性名（`brief` 12：断言用的子串不许取自夹具名字里带含义的那半）。
    let cwd = "/p/one";

    // ① 反空真 —— 表里没有这个号 ⇒ 这条入口本来就该送出一条**不带中转注入**的命令，
    //    而且它得像一条真的本机拉起（这个号的 configDir 在里面）。
    //    没有这一格，下面那条「有前缀」的断言在「整条链恒空」时会读成假红/假绿。
    entry_answer(&[]);
    tauri::async_runtime::block_on(resume_impl(
        "sid-r1".to_string(),
        cwd.to_string(),
        None,
        Some(account.clone()),
        None,
    ))
    .expect("不走中转这一趟不该失败");
    let (bare, bare_cwd) = entry_last();
    assert!(
        bare.contains(dir),
        "基准串里连这个号的 configDir 都没有 —— 这条入口根本没把账号送下去：{bare:?}"
    );
    assert!(
        !bare.contains("ANTHROPIC_BASE_URL"),
        "表里没有这个号，送出去的那一串却带着中转注入：{bare:?}"
    );
    assert_eq!(
        bare_cwd.as_deref(),
        Some(cwd),
        "这一跳把 `cwd` 弄丢了 —— 会话会起在默认目录上，而 toast 照报成功"
    );

    // ② 表里有这个号 ⇒ 送出去的那一串带中转注入，账号段与 sid 段都是**这一发**的。
    entry_answer(&["acct-r1"]);
    tauri::async_runtime::block_on(resume_impl(
        "sid-r1".to_string(),
        cwd.to_string(),
        None,
        Some(account.clone()),
        None,
    ))
    .expect("走中转这一趟不该失败");
    let (routed, _) = entry_last();
    assert!(
        routed.contains("ANTHROPIC_BASE_URL"),
        "\n★★ **`resume_impl` 这一跳把账号扔了** —— 刀 `D8P32` 的形状：\n\
             `launch_local(…, account.filter(|_| false), …)`。\n\
             生产后果：历史页 resume 一个 api-key 号 ⇒ claude 直连官方端点、第三方 key 用不上，\n\
             而在 `D8` 实测里**全量门禁四个数一格不动**。实得 = {routed:?}"
    );
    assert!(
        routed.contains(&format!("/{}'", "acct-r1")),
        "路由键里的账号段不是这一发的号：{routed:?}"
    );
    assert!(
        !routed
            .split("; ")
            .next()
            .unwrap_or_default()
            .contains("sid-r1"),
        "〔V141〕resume 的 sid 进了中转地址（会话 id 归 claude，地址不随会话变）：{routed:?}"
    );
    // 逐字节：前缀 + 基准串。剥掉第一段之后剩下的**必须**逐字节等于 ① 那趟的基准串
    // —— 「多注入一个前缀」与「顺手把命令体也换了」在只断 `contains` 的判据上同形。
    let (head, tail) = routed.split_once("; ").expect("走中转那一趟没有前缀段");
    // ⚠ **09-09 订正（云端 windows-latest 首跑逮到）**：`entry_stage` 的平台那一格
    //   照**生产取值口**（真答案），所以在 Windows 上中转前缀**真的**渲成 PS 形态
    //   `$env:ANTHROPIC_BASE_URL='…'`，而这里原来把 POSIX 那一种写死了。
    //   ⇒ 按**这台机器**算出该有的那一种，各断各的。
    //   🔴 **不是「两种都放行」** —— 那会把「渲错了平台形态」这一刀松掉
    //   （`D6` 刀 `Xb` 的正主）。现在两个平台各自只放行一种：
    //   Linux 上渲成 `$env:` 照样红，Windows 上渲成 `export` 也照样红。
    let want_head = if platform_is_windows() {
        "$env:ANTHROPIC_BASE_URL="
    } else {
        "export ANTHROPIC_BASE_URL="
    };
    // 〔E2 · V146〕第一段先判用户自己设没设端点，注入在它的「没设」那一臂里 ⇒ 认「含」不认「打头」。
    assert!(
        head.contains(want_head),
        "第一段不是中转注入（这台机器该渲成 {want_head:?}）：{head:?}"
    );
    assert_eq!(
        tail, bare,
        "剥掉中转前缀之后的命令体与不走中转那一趟不一样 —— 这一跳除了账号还动了别的"
    );
}

/// ★★ **探针 ②**〔`D8 阻-1`，`KH2B1`〕：**前端真正调的那条命令**
/// [`resume_history_session`]（`#[tauri::command]`）把五个入参原样交给 [`resume_impl`]。
///
/// 刀 `D8P32b`（[`resume_history_session`] 里那一行 `account.as_ref(),` 写成
/// `account.as_ref().filter(|_| false)`）在本条落地之前是**全量门禁四个数一格不动**的。
/// ⚠ 行号带尖号（`D8 阻-5`）：`c02d954` 与本尖上都是 `:892`（本轮加的行都在它下面）。
///
/// ⚠ 第 ② 格是**对拍**（同一组输入喂两条入口，两串必须逐字节相同）——
/// 它买的是「这一跳一个入参都没被换掉」，比只断「有前缀」宽一格：
/// 换掉 `launcher` / `tmux_name` / `cwd` 中任何一个，这一格也红。
#[test]
fn the_resume_command_the_frontend_calls_hands_all_five_arguments_down_unchanged() {
    let (_sink, _facts, _turn) = entry_stage();
    let dir = "/h/.claude-alt/acct-r2";
    let cwd = "/p/two";
    entry_answer(&["acct-r2"]);

    // ① 从**那条 `#[tauri::command]`** 进去。
    tauri::async_runtime::block_on(resume_history_session(
        "sid-r2".to_string(),
        cwd.to_string(),
        Some("cc".to_string()),
        Some(LaunchAccount::Named {
            config_dir: dir.to_string(),
            name: None,
        }),
        None,
    ))
    .expect("这一趟不该失败");
    let (from_cmd, cmd_cwd) = entry_last();
    assert!(
        from_cmd.contains("ANTHROPIC_BASE_URL") && from_cmd.contains("/acct-r2'"),
        "\n★★ **那条 `#[tauri::command]` 把账号扔了** —— 刀 `D8P32b` 的形状：\n\
             `resume_impl(…, account.as_ref().filter(|_| false), …)`。\n\
             这一跳就是**历史页 resume 那个按钮真正调的那条命令**，\n\
             而在 `D8` 实测里一刀下去**全量门禁四个数一格不动**。实得 = {from_cmd:?}"
    );
    assert!(
        !from_cmd
            .split("; ")
            .next()
            .unwrap_or_default()
            .contains("sid-r2"),
        "〔V141〕resume 的 sid 进了中转地址：{from_cmd:?}"
    );

    // ② 对拍：同一组输入直接喂下一跳，两串必须**逐字节相同**。
    //    ⇒ 这一跳换掉五个入参里的**任何一个**（不只是 `account`），本格都红。
    let account = LaunchAccount::Named {
        config_dir: dir.to_string(),
        name: None,
    };
    tauri::async_runtime::block_on(resume_impl(
        "sid-r2".to_string(),
        cwd.to_string(),
        Some("cc".to_string()),
        Some(account),
        None,
    ))
    .expect("这一趟不该失败");
    let (from_impl, impl_cwd) = entry_last();
    assert_eq!(
        from_cmd, from_impl,
        "\n那条 `#[tauri::command]` 与它下一跳送出去的不是同一串 —— \
             这一跳换掉了某个入参（不一定是 `account`）"
    );
    assert_eq!(cmd_cwd, impl_cwd, "`cwd` 在这一跳被换掉了");
}

/// ★★ **探针 ③**〔`D8 阻-1`，`KH2B1`〕：**「在该目录起新会话」那条命令**
/// [`new_local_session`]（`#[tauri::command]`）把 `account` / `cwd` 原样交给 [`launch_local`]。
///
/// 刀 `D8P33`（[`new_local_session`] 里那一行 `account.as_ref(),` 写成
/// `account.as_ref().filter(|_| false)`）在本条落地之前是**全量门禁四个数一格不动**的。
/// ⚠ 行号带尖号（`D8 阻-5`）：`c02d954` 上是 `:1871`，本尖上是 `:1901`。
///
/// ⚠ 这条路**没有 sid**（`<key>` 段走 nonce，见 `payload::relay_key_for` 的表），
/// 所以本条只钉账号段与 `cwd`；`<key>` 那一维归 `payload` 那一侧的判据。
#[test]
fn the_new_session_command_the_frontend_calls_carries_the_account_and_the_cwd_through() {
    let (_sink, _facts, _turn) = entry_stage();
    let tmp = TmpDir::new(); // `new_local_session` 会先核 `cwd` 是不是现存目录
    let cwd = tmp.0.to_string_lossy().into_owned();
    let dir = "/h/.claude-alt/acct-r3";

    // ① 反空真：表里没有这个号 ⇒ 不带中转注入，但 configDir 与 cwd 都得走到。
    entry_answer(&[]);
    tauri::async_runtime::block_on(new_local_session(
        cwd.clone(),
        None,
        Some(LaunchAccount::Named {
            config_dir: dir.to_string(),
            name: None,
        }),
    ))
    .expect("不走中转这一趟不该失败");
    let (bare, bare_cwd) = entry_last();
    assert!(
        bare.contains(dir),
        "基准串里连这个号的 configDir 都没有：{bare:?}"
    );
    assert!(
        !bare.contains("ANTHROPIC_BASE_URL"),
        "表里没有这个号却带着中转注入：{bare:?}"
    );
    assert_eq!(
        bare_cwd.as_deref(),
        Some(cwd.as_str()),
        "这一跳把 `cwd` 弄丢了 —— 「在该目录起新会话」会起到别的目录去"
    );

    // ② 表里有这个号 ⇒ 带中转注入，且账号段是这一发的号。
    entry_answer(&["acct-r3"]);
    tauri::async_runtime::block_on(new_local_session(
        cwd.clone(),
        None,
        Some(LaunchAccount::Named {
            config_dir: dir.to_string(),
            name: None,
        }),
    ))
    .expect("走中转这一趟不该失败");
    let (routed, routed_cwd) = entry_last();
    assert!(
        routed.contains("ANTHROPIC_BASE_URL") && routed.contains("/acct-r3'"),
        "\n★★ **「在该目录起新会话」那条命令把账号扔了** —— 刀 `D8P33` 的形状：\n\
             `launch_local(…, account.as_ref().filter(|_| false), …)`。\n\
             生产后果：一个 api-key 号**新开**会话 ⇒ 直连官方端点，\n\
             而在 `D8` 实测里一刀下去**全量门禁四个数一格不动**。实得 = {routed:?}"
    );
    assert_eq!(
        routed_cwd.as_deref(),
        Some(cwd.as_str()),
        "走中转这一趟把 `cwd` 弄丢了"
    );
    assert_ne!(
        bare, routed,
        "两趟送出去的是同一串 —— 「表里有没有这一行」这一维在这条入口上成了常量"
    );
}

// ── P28：这条负对照**建不起来**，读数如实留在这里 ──
//
// 本文件对 `history.rs` 的判据全部走 `guard_core::production_code`，而它的针
// （`fn launch_local(` 1820 · `fn render_local_ccm_with(` 1620 · `render_local_ccm(` 1592/1909/1917/2632 ·
//  `build_local_posix_command(` 1486/1909/1928 · `let _ = tmux_name;` 1834/2636）
// **一个都不满足负对照的前提**：现打逐针比对 `production_code` 的产物与便宜近似
// （`split("\n#[cfg(test)]").next()`）的产物，上面每一个针**两边都在**。
//
// 🔴 原因值得写下来，它订正了一条容易记错的口径：
// 便宜近似切的是**逐字**的 `\n#[cfg(test)]`。`history.rs` 最靠前的测试期属性在 **1558** 行，
// 但它写的是 `#[cfg(all(test, not(windows)))]` —— 便宜近似**匹配不上**
// （`guard-core` 头注记的「坑 1 的变种」）。它真正的切点是第一个**逐字**列 0 的
// `#[cfg(test)]`＝**2196** 行 ⇒ 便宜近似留下 123_670 字节（原文 159_846、生产段 52_035，
// 现打），本文件的针全在那 123_670 里。
// ⇒ **这一份在 `P28` 那个口径下不是高危**：换成便宜近似，本文件一条判据都不会静默变瞎。
//
// ⚠ 别把这段读成「这份文件安全」。它只是说**便宜近似**这一形够不着它；
// 「剥法过剥」的别的走法（词法掩码认错、区间收尾挪晚）本条一个字都不管。
// ⚠ 也别为了让这一格有东西就换一批「好过的针」—— 那是拿一条恒真的判据冒充覆盖，
// `assert_stripper_keeps` 正是为此在第一版上当场红的（读数见 `payload_tests` 那条的注释）。

/// 全量注入开关**关着**（旋钮设成 `0` 那一形；生产默认开，RL2）的替身。缝上那一格要一个函数指针。
fn all_sessions_off() -> bool {
    false
}

// ── 〔US1 · 第四波 4D〕替身：「那台后端答的成品」（缝上 `endpoint` 那一格） ─────────────────────────
//
// 决策表今天住后端（`accounts/upstream/endpoint.rs::decide_launch`，它自己的判据在 `endpoint_tests`）。
// 这边量的是**起会话那一侧**：真的问了、问的入参对、照成品执行（拼上 / 不拼 / 拒）。替身照后端那张表的
// 几格作答（表的行与「中转在不在」由 [`fake_table`] 设），**它不是被测对象**，只为让这一侧有真答案可用。

thread_local! {
    static FAKE_ROWS: std::cell::RefCell<Vec<String>> = const { std::cell::RefCell::new(Vec::new()) };
    static FAKE_LISTENING: std::cell::Cell<bool> = const { std::cell::Cell::new(true) };
    static FAKE_ASKS: std::cell::RefCell<Vec<(Option<String>, serde_json::Value)>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

/// 替身那张表：哪几行 · 那台的中转在不在听。
fn fake_table(rows: &[&str], listening: bool) {
    FAKE_ROWS.with(|v| *v.borrow_mut() = rows.iter().map(|s| s.to_string()).collect());
    FAKE_LISTENING.with(|c| c.set(listening));
}

/// 替身被问过的每一次：`(主机 | None = 本机, 入参)`，取走即清。
fn fake_asks() -> Vec<(Option<String>, serde_json::Value)> {
    FAKE_ASKS.with(|v| std::mem::take(&mut *v.borrow_mut()))
}

type EpFut =
    std::pin::Pin<Box<dyn std::future::Future<Output = Result<serde_json::Value, String>> + Send>>;

/// 替身本体：照后端决策表的几格作答（`/s/` 有行 ⇒ refuse；开关开且登记 ⇒ `/t/` direct；撞名 ⇒ 不注入）。
fn fake_ep(host: Option<String>, a: serde_json::Value) -> EpFut {
    FAKE_ASKS.with(|v| v.borrow_mut().push((host, a.clone())));
    let rows = FAKE_ROWS.with(|v| v.borrow().clone());
    let listening = FAKE_LISTENING.with(std::cell::Cell::get);
    let agent = a["agent"].as_str().unwrap_or_default().to_string();
    let id = a["account"]["configDir"]
        .as_str()
        .and_then(apikey_account_id_of_dir);
    let has = |x: &str| agent == "claude-code" && rows.iter().any(|r| r == x);
    let none = serde_json::json!({"baseUrl":null,"listening":false,"whenDown":null,"account":null});
    let answer = if let Some(id) = id.as_deref().filter(|i| has(i)) {
        serde_json::json!({"baseUrl": format!("http://127.0.0.1:8788/s/{agent}/{id}"),
            "listening": listening, "whenDown": "refuse", "account": id})
    } else if a["allSessions"] == serde_json::json!(true) && agent == "claude-code" {
        let label = match a["account"]["kind"].as_str() {
            Some("named") => id.clone(),
            Some("base") => Some("0".to_string()),
            _ => Some("_".to_string()),
        };
        match label.filter(|l| !has(l)) {
            Some(l) => {
                serde_json::json!({"baseUrl": format!("http://127.0.0.1:8788/t/{agent}/{l}"),
                "listening": listening, "whenDown": "direct", "account": null})
            }
            None => none,
        }
    } else {
        none
    };
    Box::pin(async move { Ok(answer) })
}

/// 替身：那台后端问不到。
fn fake_ep_unreachable(host: Option<String>, a: serde_json::Value) -> EpFut {
    FAKE_ASKS.with(|v| v.borrow_mut().push((host, a)));
    Box::pin(async move { Err("没有控制通道".to_string()) })
}

/// 〔TL3 · 审计 F 🔴-2〕[`relay_prefix_for_launch`] 今天是 `async`（本机起会话链路零 `block_on`，`INVARIANTS §10`）；
/// 判据在**测试线程**上等它一趟（替身都是线程局部的，`block_on` 在调用线程上轮询那个 future）。
fn prefix_now(action: &LocalPsAction, account: Option<&LaunchAccount>) -> Result<String, String> {
    tauri::async_runtime::block_on(relay_prefix_for_launch(action, account))
}

/// 〔TL3〕「问后端 ＋ 拉起」在测试线程上走一趟：前缀由生产那个取值口现算（[`prefix_now`]），再交给同步那一截
/// [`launch_local`]；问不到 / 该拒 ⇒ 当场 `Err`、一个字节都不送（与生产 [`launch_local_asking_backend`] 同序）。
/// 与生产那一趟的差别只有一格：同步那一截不换线程（本文件各条的记账替身多是线程局部的）。
/// 「换了线程也照样拼上」由命令级那几条探针真走 `spawn_blocking` 钉（`the_resume_command_…` · `the_new_session_command_…`）。
fn launch_now(
    action: &LocalPsAction,
    launcher: Option<&str>,
    cwd: Option<&str>,
    account: Option<&LaunchAccount>,
    tmux_name: Option<&str>,
) -> Result<String, String> {
    let relay = prefix_now(action, account)?;
    launch_local(action, launcher, cwd, account, tmux_name, relay)
}

/// ★★★ 〔`设计/20 §7` 步 4 · RL2（V135 真跑过一次）〕**开关默认开，环境变量恰好是 `0` 才关。**
///
/// 本条只量取值口自己（缝上那一格由上面按地址对拍）。⚠ 它**不去改进程环境**：
/// `cargo test` 多线程跑，改 `std::env` 会串到别的判据。⇒ 只断「今天这个进程里没设它 ⇒ 开」
/// 这一向，并把「设成 0 ⇒ 关」那一向交给**名字与比较值**的字面量对拍（取值口只有一行）。
#[test]
fn the_all_sessions_switch_is_on_unless_that_one_variable_says_0() {
    assert_eq!(RELAY_ALL_SESSIONS_ENV, "CCM_RELAY_ALL_SESSIONS");
    if std::env::var_os(RELAY_ALL_SESSIONS_ENV).is_none() {
        assert!(
            relay_all_sessions_switch(),
            "环境里没有这个变量，开关却是关的 —— 默认开那一格破了（RL2 · V135）"
        );
    } else {
        // 跑判据的人自己设了它：这一向判不了，照实说出来（不是「过了」）。
        println!(
            "〔读数〕本进程环境里设了 {RELAY_ALL_SESSIONS_ENV} ⇒ 「没设就开」这一向本趟判不了"
        );
    }
}

/// ★★★ 〔`设计/20 §7` 步 4〕**起会话那一侧真的问了开关、并且把它的答案交给了那台后端**（接线判据）。
///
/// 〔US1〕开关之后那几格（`/t/` · 账号 0 的 `0` · 没表态的 `_`）是后端决策表的事（`endpoint_tests`）；这一侧量的是：
/// 同一个号、同一个「中转在听」，只把开关翻过来 ⇒ 交出去的 `allSessions` 跟着翻、前缀跟着变（关 ⇒ 空串）。
/// 谁把 `(facts.all_sessions)()` 换成一个常量，两格里必有一格红。
#[test]
fn the_launch_side_asks_the_all_sessions_switch_and_uses_its_answer() {
    fn not_win() -> bool {
        false
    }
    fn on() -> bool {
        true
    }
    let action = LocalPsAction::Resume("sid-1".to_string());
    let named = LaunchAccount::Named {
        config_dir: "/h/.claude-alt/acct-sub".to_string(),
        name: None,
    };
    fake_table(&[], true);
    {
        let _f = override_inject_facts(InjectFactSources {
            endpoint: fake_ep,
            windows: not_win,
            all_sessions: all_sessions_off,
        });
        let _ = fake_asks();
        assert_eq!(prefix_now(&action, Some(&named)).unwrap(), "");
        assert_eq!(fake_asks()[0].1["allSessions"], serde_json::json!(false));
    }
    let _f = override_inject_facts(InjectFactSources {
        endpoint: fake_ep,
        windows: not_win,
        all_sessions: on,
    });
    assert_eq!(
        prefix_now(&action, Some(&named)).unwrap(),
        "[ -n \"${ANTHROPIC_BASE_URL:-}\" ] && printf '%s\\n' 'cc-monitor：这个会话用你自己设的端点（ANTHROPIC_BASE_URL），不走中转，拿不到流式' || export ANTHROPIC_BASE_URL='http://127.0.0.1:8788/'\"$(cat \"$HOME/.cc-monitor/relay-key\")\"'/t/claude-code/acct-sub'; ",
        "开关开着，订阅号该走 `/t/`"
    );
    assert_eq!(fake_asks()[0].1["allSessions"], serde_json::json!(true));
    // F5：没表态也交出去（`account: null`），由后端答 `_` 那一格。
    let _ = prefix_now(&action, None).unwrap();
    assert_eq!(fake_asks()[0].1["account"], serde_json::Value::Null);
}

// ═════════════════════════════════════════════════════════════════════════════
// 〔RL1 · US1 · DEL〕R1：`relay_endpoint_on` —— 问那台后端要成品（`launch-endpoint`），照成品执行（远端不另起中转）
// ═════════════════════════════════════════════════════════════════════════════
//
// 台架：一条内存管道两头 —— monitor 这头是**真的** `InboundClient`（`park → into_client`），登记在一个
// 假主机名下；对面是一个**照脚本答**的假后端（记下每条请求的 `cmd`，按脚本回 `data`）。
// 缝上那一格装的是**生产那一跳**（`PRODUCTION_INJECT_FACTS.endpoint`）⇒ 转交那一截也在射程里。
// 异源在哪：期望的地址手写；「发了哪几条命令、什么顺序」数的是假后端那一侧收到的（不是被测函数自己说的）。

mod relay_endpoint_rig {
    use crate::backend::control::inbound_client::{
        park, register, unregister, BackendHello, InboundClient,
    };
    use crate::ssh_source::{parse_frame, InboundFrame};
    use serde_json::Value;
    use std::collections::VecDeque;
    use std::sync::{Arc, Mutex};
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt};

    pub(super) struct Rig {
        pub(super) host: String,
        pub(super) seen: Arc<Mutex<Vec<String>>>,
        client: Arc<InboundClient>,
    }

    impl Drop for Rig {
        fn drop(&mut self) {
            unregister(&self.host, &self.client);
        }
    }

    /// `script`：每条命令按顺序答的 `data`（答完了还来 ⇒ 回 `ok:false`，让多发的那一条当场现形）。
    pub(super) fn rig(host: &str, script: Vec<(&'static str, Value)>) -> Rig {
        let (mon_w, be_r) = tokio::io::duplex(1 << 16);
        let (be_w, mon_r) = tokio::io::duplex(1 << 16);
        let hello = InboundFrame::Hello {
            v: 1,
            build_id: "t".into(),
            host_arch: "x86_64".into(),
            claude_dir: "/tmp".into(),
            homes: vec![],
            capabilities: vec![],
            commands: ["launch-endpoint"].iter().map(|s| s.to_string()).collect(),
            unavailable: vec![],
            uncancellable: vec![],
        };
        let client =
            park(mon_w).into_client(BackendHello::from_hello_frame(&hello).expect("hello"));
        let c2 = Arc::clone(&client);
        tauri::async_runtime::spawn(async move {
            let mut lines = tokio::io::BufReader::new(mon_r).lines();
            while let Ok(Some(l)) = lines.next_line().await {
                if let Some(f) = parse_frame(&l) {
                    crate::backend::control::local_backend::absorb_local_frame(f, Some(&c2));
                }
            }
        });
        let seen = Arc::new(Mutex::new(Vec::<String>::new()));
        let seen2 = Arc::clone(&seen);
        let mut plan: VecDeque<(&'static str, Value)> = script.into();
        tauri::async_runtime::spawn(async move {
            let mut w = be_w;
            let mut lines = tokio::io::BufReader::new(be_r).lines();
            while let Ok(Some(l)) = lines.next_line().await {
                let v: Value = serde_json::from_str(&l).expect("请求不是 JSON");
                let id = v["id"].as_str().unwrap_or_default().to_string();
                let cmd = v["cmd"].as_str().unwrap_or_default().to_string();
                seen2.lock().expect("lock").push(cmd.clone());
                let line = match plan.front() {
                    Some((c, _)) if *c == cmd => {
                        let (_, data) = plan.pop_front().expect("刚看过");
                        serde_json::json!({"kind":"reply","id":id,"ok":true,"data":data})
                            .to_string()
                    }
                    _ => serde_json::json!({"kind":"reply","id":id,"ok":false,
                        "error":{"code":"unscripted","message":format!("脚本里下一条不是 {cmd}")}})
                    .to_string(),
                };
                if w.write_all(format!("{line}\n").as_bytes()).await.is_err() {
                    break;
                }
            }
        });
        register(host, Arc::clone(&client));
        Rig {
            host: host.to_string(),
            seen,
            client,
        }
    }
}

/// 〔US1〕那台后端答的成品（`launch-endpoint`）。
fn rl1_answer(
    url: Option<&str>,
    listening: bool,
    when_down: Option<&str>,
    account: Option<&str>,
) -> serde_json::Value {
    serde_json::json!({"baseUrl": url, "listening": listening, "whenDown": when_down, "account": account})
}

const RL1_S_URL: &str = "http://127.0.0.1:8788/s/claude-code/acct-a";
const RL1_T_URL: &str = "http://127.0.0.1:8788/t/claude-code/0";

fn rl1_named(id: &str) -> LaunchAccount {
    LaunchAccount::Named {
        config_dir: format!("/home/u/.claude-alt/{id}"),
        name: None,
    }
}

fn rl1_facts(all_sessions: fn() -> bool) -> InjectFactsGuard {
    fn not_win() -> bool {
        false
    }
    override_inject_facts(InjectFactSources {
        endpoint: PRODUCTION_INJECT_FACTS.endpoint,
        windows: not_win,
        all_sessions,
    })
}

fn rl1_off() -> bool {
    false
}

/// R1 ①：**不注入 ⇒ 一条中转命令都不发**（用到才起）：假后端只收到 `launch-endpoint` 一条。
#[test]
fn a_launch_that_needs_no_relay_sends_no_relay_command() {
    let _f = rl1_facts(rl1_off);
    let rig = relay_endpoint_rig::rig(
        "rl1-host-none",
        vec![("launch-endpoint", rl1_answer(None, false, None, None))],
    );
    let got = tauri::async_runtime::block_on(relay_endpoint_on(
        &crate::origin::Origin(rig.host.clone()),
        Some(&rl1_named("acct-a")),
    ));
    assert_eq!(got, Ok(None));
    assert_eq!(
        *rig.seen.lock().expect("lock"),
        vec!["launch-endpoint"],
        "不注入却发了中转命令"
    );
}

/// R1 ②：成品说「注入 `/s/`、那台的中转在听」⇒ 原样交出地址，只问这一条。
#[test]
fn an_apikey_row_on_a_listening_machine_gets_the_substitute_url_without_an_ensure() {
    let _f = rl1_facts(rl1_off);
    let rig = relay_endpoint_rig::rig(
        "rl1-host-up",
        vec![(
            "launch-endpoint",
            rl1_answer(Some(RL1_S_URL), true, Some("refuse"), Some("acct-a")),
        )],
    );
    let got = tauri::async_runtime::block_on(relay_endpoint_on(
        &crate::origin::Origin(rig.host.clone()),
        Some(&rl1_named("acct-a")),
    ))
    .expect("不该拒");
    assert_eq!(got.as_deref(), Some(RL1_S_URL));
    assert_eq!(*rig.seen.lock().expect("lock"), vec!["launch-endpoint"]);
}

/// 〔DEL〕D3 R1 ③：远端成品说 `refuse`、那台的中转不在 ⇒ **不另起一个**（假后端只收到 `launch-endpoint` 一条）、
/// **拒绝起会话**，说法与本机那一格逐字同一句（只差哪台机器）。
/// 守的要求：`4d-lanes.md` `## DEL` 逐字「脱离的独立 `--relay` 进程一族（`relay-ensure` / `relay-status` / not_ours 探针）」删；
/// `设计/20 §3.3`「远端中转住远端常驻后端里 …… 走的就是上面本机那一段」。
#[test]
fn a_remote_apikey_row_whose_relay_is_down_is_refused_without_starting_one() {
    let _f = rl1_facts(rl1_off);
    let rig = relay_endpoint_rig::rig(
        "rl1-host-dead",
        vec![(
            "launch-endpoint",
            rl1_answer(Some(RL1_S_URL), false, Some("refuse"), Some("acct-a")),
        )],
    );
    let err = tauri::async_runtime::block_on(relay_endpoint_on(
        &crate::origin::Origin(rig.host.clone()),
        Some(&rl1_named("acct-a")),
    ))
    .expect_err("中转不在还放行了一个 apikey 号");
    assert_eq!(
        err,
        relay_down_refusal("[rl1-host-dead] ", Some("acct-a"), &RELAY_NOT_LISTENING)
    );
    assert_eq!(
        *rig.seen.lock().expect("lock"),
        vec!["launch-endpoint"],
        "中转不在时又去那台起了一个"
    );
}

/// R1 ④：成品说 `direct`（`/t/`）、中转不在 ⇒ **照旧直连**（`None`），不拒 —— 「有它更好」；同样只问一条。
#[test]
fn a_passthrough_launch_whose_relay_is_down_goes_direct_instead_of_failing() {
    let _f = rl1_facts(rl1_off);
    let rig = relay_endpoint_rig::rig(
        "rl1-host-t",
        vec![(
            "launch-endpoint",
            rl1_answer(Some(RL1_T_URL), false, Some("direct"), None),
        )],
    );
    let got = tauri::async_runtime::block_on(relay_endpoint_on(
        &crate::origin::Origin(rig.host.clone()),
        Some(&LaunchAccount::Base),
    ));
    assert_eq!(got, Ok(None));
    assert_eq!(*rig.seen.lock().expect("lock"), vec!["launch-endpoint"]);
}

// ── 〔LOC1b · 第四波 4D〕冷读本机远端同一条路 ──────────────────────────────────────────
//
// 要求住址：`INVARIANTS §40` 逐字「我的目的就是把本地当成不走 ssh 的远端」· `设计/00 §2.5 ①` 逐字
// 「历史 / 账号 / tmux / MCP 四个面，本机与远端走同一条代码路径」。
// 夹具只有结构、没有会话正文（`test-fixtures-no-real-transcript`）。

const LOC1B_CLAUDE_PAGE: &str = concat!(
    "{\"type\":\"permission-mode\",\"permissionMode\":\"default\"}\n",
    "\n",
    "{\"type\":\"user\",\"uuid\":\"u1\",\"timestamp\":\"2026-01-01T00:00:00Z\",\"cwd\":\"/w\",\"message\":{\"role\":\"user\",\"content\":\"x\"}}\n",
    "{\"type\":\"assistant\",\"uuid\":\"a1\",\"timestamp\":\"2026-01-01T00:00:01Z\",\"message\":{\"role\":\"assistant\",\"content\":[{\"type\":\"text\",\"text\":\"y\"}]}}\n",
);

/// 同一页喂本机与远端两个 pager：记录、seq、cwd、sid 逐格相同，差别**只在**载荷上的 `origin`（本机省略、远端是机器名）。
#[test]
fn loc1b_one_pager_serves_both_sides_and_only_the_payload_origin_differs() {
    let path = "/h/.claude/projects/p/0b8f7a4e-0000-4000-8000-000000000001.jsonl";
    let mut local = SessionPager::new(&crate::origin::Origin::local(), path);
    let mut remote = SessionPager::new(&crate::origin::Origin("devbox".into()), path);
    let (l, r) = (
        local.page(LOC1B_CLAUDE_PAGE),
        remote.page(LOC1B_CLAUDE_PAGE),
    );
    // permission-mode 不可显示但占号（seq 0），空行不占号 ⇒ user=1、assistant=2。
    assert_eq!(l.iter().map(|p| p.seq).collect::<Vec<_>>(), vec![1, 2]);
    assert_eq!(r.iter().map(|p| p.seq).collect::<Vec<_>>(), vec![1, 2]);
    for (a, b) in l.iter().zip(&r) {
        assert_eq!(a.session_id, "0b8f7a4e-0000-4000-8000-000000000001");
        assert_eq!(
            (&a.session_id, &a.cwd, &a.path),
            (&b.session_id, &b.cwd, &b.path)
        );
        assert_eq!(a.cwd.as_deref(), Some("/w"), "cwd 从首条 user 记录起往后带");
        assert_eq!(
            serde_json::to_value(&a.message).unwrap(),
            serde_json::to_value(&b.message).unwrap()
        );
    }
    assert!(
        l.iter().all(|p| p.origin.is_none()),
        "本机载荷不带 origin（前端视为本机）"
    );
    assert!(r.iter().all(|p| p.origin.as_deref() == Some("devbox")));
}

/// 页边界不重置行号：第二页接着第一页的号往下数（分页是 transport 的事，seq 口径不许跟着变）。
#[test]
fn loc1b_numbering_continues_across_pages() {
    let path = "/h/.claude/projects/p/s.jsonl";
    let mut whole = SessionPager::new(&crate::origin::Origin::local(), path);
    let one: Vec<u64> = whole
        .page(LOC1B_CLAUDE_PAGE)
        .iter()
        .map(|p| p.seq)
        .collect();
    let (head, tail) =
        LOC1B_CLAUDE_PAGE.split_at(LOC1B_CLAUDE_PAGE.find("{\"type\":\"assistant\"").unwrap());
    let mut paged = SessionPager::new(&crate::origin::Origin::local(), path);
    let mut two: Vec<u64> = paged.page(head).iter().map(|p| p.seq).collect();
    two.extend(paged.page(tail).iter().map(|p| p.seq));
    assert_eq!(one, two);
}

/// 种类按文件名形态判：远端的 Codex rollout 路径（本机没有那一家的根）也走 Codex 解析、sid 取末尾 UUID；
/// 同一行放进 Claude 名字的文件 ⇒ 按 Claude 解（抢救成 `Unrecognized`），两形确实分叉（正反控）。
#[test]
fn loc1b_the_agent_kind_comes_from_the_file_name_on_either_side() {
    let uuid = "0b8f7a4e-0000-4000-8000-000000000002";
    let codex_path =
        format!("/far/away/.codex/sessions/2026/01/01/rollout-2026-01-01T00-00-00-{uuid}.jsonl");
    let line = "{\"timestamp\":\"2026-01-01T00:00:00Z\",\"type\":\"response_item\",\"payload\":{\"type\":\"message\",\"role\":\"user\",\"content\":[{\"type\":\"input_text\",\"text\":\"x\"}]}}\n";
    let mut codex = SessionPager::new(&crate::origin::Origin("devbox".into()), &codex_path);
    let got = codex.page(line);
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].session_id, uuid);
    assert!(
        matches!(got[0].message, crate::messages::JsonlRecord::User { .. }),
        "Codex 的 user 消息要映射成 User 记录：{:?}",
        got[0].message
    );
    let mut claude = SessionPager::new(
        &crate::origin::Origin("devbox".into()),
        "/far/away/.claude/projects/p/s.jsonl",
    );
    let as_claude = claude.page(line);
    assert!(
        !as_claude
            .iter()
            .any(|p| matches!(p.message, crate::messages::JsonlRecord::User { .. })),
        "同一行按 Claude 解不该成 User —— 否则上一条没证明种类真的按名字分了"
    );
}

/// 读一整份会话的那条 Tauri 命令**只有一条路**：经那台后端的 `history-read` 分页，体里零处按本机分叉、零处自己开文件。
/// 正控：针在一段写着本机分支的合成串里认得出来（否则零命中是空真）。
#[test]
fn loc1b_the_cold_read_command_has_no_local_branch_and_opens_no_file() {
    let prod = guard_core::production_code(include_str!("../../src/bridge/src/history.rs"));
    let start = prod
        .find("pub async fn stream_read_session_jsonl(")
        .expect("找不到那条命令 —— 本条空转");
    let end = prod[start..]
        .find("\n}\n")
        .map(|k| start + k)
        .expect("找不到函数尾");
    let body = &prod[start..end];
    let needles = [
        "Route::Local",
        "Route::Remote",
        "is_local()",
        "File::open",
        "remote_history::",
    ];
    let hits = |s: &str| needles.iter().filter(|n| s.contains(*n)).count();
    assert_eq!(
        hits(body),
        0,
        "冷读命令体里又长出了按本机 / 远端分叉或自己开文件的那一形：\n{body}"
    );
    assert_eq!(
        body.matches("frame_query::read_page(").count(),
        1,
        "取原文只经那台后端这一处"
    );
    // 正控：旧形状（本机分支自己开文件）会被认出来。
    let old = "if let crate::origin::Route::Remote(host) = origin.route(\"x\")? { return crate::remote_history::f(host).await; } let file = File::open(&target);";
    assert_eq!(hits(old), 3);
}

/// R1 ⑥〔US1〕：成品形状对不上（多一格 · 缺一格 · `whenDown` 认不出 · `baseUrl` 与 `whenDown` 不同有无）⇒ 抛「两端契约对不上」，
/// 不猜成「不注入」。期望手写；正控：四个键恰好那一形照收。
#[test]
fn a_launch_endpoint_answer_of_the_wrong_shape_is_refused_not_guessed() {
    let ok = rl1_answer(Some(RL1_T_URL), true, Some("direct"), None);
    assert!(launch_endpoint_from_wire("本机", &ok).is_ok());
    let mut extra = ok.clone();
    extra["rows"] = serde_json::json!([]);
    let mut missing = ok.clone();
    missing.as_object_mut().unwrap().remove("listening");
    for bad in [
        extra,
        missing,
        rl1_answer(Some(RL1_T_URL), true, Some("maybe"), None),
        rl1_answer(Some(RL1_T_URL), true, None, None),
        rl1_answer(None, false, Some("refuse"), None),
        serde_json::json!("x"),
    ] {
        let e = launch_endpoint_from_wire("本机", &bad).expect_err("坏形被收了");
        assert!(e.contains("两端版本对不上"), "{bad} ⇒ {e}");
    }
}
