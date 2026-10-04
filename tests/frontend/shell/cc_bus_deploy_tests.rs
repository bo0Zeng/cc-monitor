//! 装 / 三态那几条判据随装 cc-bus 进了后端（`tests/backend/assets/cc_bus_install_tests.rs`）；这里只剩装前那道本机 `ccm` 预检。
use super::*;

/// ★★ 装前那道能力预检**列的东西必须与 `cc-spawn` 真正协商的一致**。
///
/// 两边是**同一个事实的两份表达**（Rust 的 `CC_SPAWN_NEEDS` 与 shell 里那行 `for _c in …`）。
/// 抽不成一份（一个是编译进 monitor 的常量、一个是要部署出去的 shell）⇒ 只能钉一致。
/// ⚠ 不一致的后果是**预检说没事、装完就坏**：漏列一条，缺那条能力的机器上
/// 装完 `cc-spawn` 直接 `exit 2`，而部署那步一声不吭地成功了。
#[test]
fn the_deploy_precheck_lists_what_cc_spawn_negotiates() {
    let spawn = include_str!("../../../src/shared/cc-bus/scripts/cc-spawn");
    let line = spawn
        .lines()
        .find(|l| l.trim_start().starts_with("for _c in "))
        .expect("`cc-spawn` 里那行能力协商不见了 —— 它是这条判据的另一半");
    // 形如：`for _c in detach tmux-size tmux-base bus-register; do`
    let listed: Vec<&str> = line
        .trim()
        .trim_start_matches("for _c in ")
        .split(';')
        .next()
        .unwrap_or("")
        .split_whitespace()
        .collect();
    assert!(
        !listed.is_empty(),
        "解析出空清单 —— 抽取器坏了（本条此刻是空转的）"
    );
    assert_eq!(
        listed, CC_SPAWN_NEEDS,
        "装前预检的清单与 `cc-spawn` 真正协商的对不上。\n             \
             左=cc-spawn 实际要的，右=Rust 侧 `CC_SPAWN_NEEDS`。\n             \
             漏列一条 ⇒ 预检说没事、装完 `cc-spawn` 直接 exit 2，而部署那步一声不吭地成功了。"
    );
}

/// ★★ Windows 那条预检**真探了**，而且五种情形的话**互相分得开**。
///
/// 🪦 上一版这里是 `windows_says_the_precheck_did_not_happen_instead_of_staying_silent`〔散文墓碑〕
/// （`#[cfg(windows)]`，断言那句话里有「没做预检」）。那一版的前提 ——「monitor 在 Windows 上
/// 没有任何 ccm 探测形态」—— 在 `K-R69`（`probe_binary_uncached`）之后不成立了，
/// 于是那句话从「诚实」变成了「过期」。⇒ 本条判的是**纯函数** `windows_ccm_precheck`，
/// 在哪台机器上都跑（上一版只在云端 windows-latest 上跑）。
///
/// 钉的是五件事：
/// 1. 五种情形**逐对不同**（没装 · 答不出 · 不是这一版 · 缺能力 · 全对）—— 合并任意两种都是骗人；
/// 2. 「全对」那一档**仍然说话**，而且说清查的是**那一份**、不是 PATH 上那个（`None` 的含义是
///    「探过了 PATH 上那个」，Windows 上从来没探过它）；
/// 3. 「不是这一版」那一档把**两边的 build 都说出来**（只说「不一致」用户不知道该往哪边对）；
/// 4. 没装 / 答不出两档仍然把 [`CC_SPAWN_NEEDS`] 四条说全（清单现取，不抄字面量）；
/// 5. 没有一档再说「没做预检」—— 那句话今天是假的。
#[test]
fn windows_precheck_really_probes_and_its_five_answers_are_distinguishable() {
    use crate::ccm_probe::CcmProbeResult;
    let want = "p9z-this-build";
    let card = |installed: bool, caps: &[&str], build: Option<&str>| CcmProbeResult {
        installed,
        version: Some("5".into()),
        capabilities: caps.iter().map(|c| c.to_string()).collect(),
        build: build.map(str::to_string),
        at: None,
    };
    let at = "$HOME/.cc-monitor/bin/ccm.exe";
    let full = card(true, CC_SPAWN_NEEDS, Some(want));
    let dead = card(false, &[], None);
    let old = card(true, CC_SPAWN_NEEDS, Some("p1a-older"));
    let lacking = card(true, &["detach"], Some(want));
    let answers = [
        ("没装", windows_ccm_precheck(None, Some(want))),
        (
            "答不出",
            windows_ccm_precheck(Some((at, &dead)), Some(want)),
        ),
        (
            "不是这一版",
            windows_ccm_precheck(Some((at, &old)), Some(want)),
        ),
        (
            "缺能力",
            windows_ccm_precheck(Some((at, &lacking)), Some(want)),
        ),
        ("全对", windows_ccm_precheck(Some((at, &full)), Some(want))),
    ];
    for (i, (a, x)) in answers.iter().enumerate() {
        assert!(!x.is_empty(), "「{a}」那一档回了空话");
        assert!(
            !x.contains("没做预检"),
            "「{a}」那一档还在说「没做预检」—— 今天它真探了：{x}"
        );
        for (b, y) in answers.iter().skip(i + 1) {
            assert_ne!(x, y, "「{a}」与「{b}」说的是同一句话 —— 用户分不开");
        }
    }
    let get = |k: &str| answers.iter().find(|(n, _)| *n == k).unwrap().1.clone();
    for k in ["没装", "答不出"] {
        for c in CC_SPAWN_NEEDS.iter().copied() {
            assert!(
                get(k).contains(c),
                "「{k}」那一档没提能力 {c:?}：{}",
                get(k)
            );
        }
        assert!(
            get(k).contains("查不了"),
            "「{k}」没把「查不了」说出来：{}",
            get(k)
        );
    }
    let stale = get("不是这一版");
    assert!(
        stale.contains("p1a-older") && stale.contains(want),
        "「不是这一版」要把两边的 build 都说出来：{stale}"
    );
    assert!(
        get("缺能力").contains("tmux-size"),
        "「缺能力」要点名缺的是哪几条：{}",
        get("缺能力")
    );
    let fine = get("全对");
    assert!(
        fine.contains("PATH") && fine.contains("那一份"),
        "「全对」那一档必须说清查的是 cc-monitor 那一份、不是 PATH 上那个：{fine}"
    );
}

/// 手上没带后端字节（「我这一版」是 `None`）：不说「不是这一版」也不说「是这一版」，只说版本不可比；能力照查。
#[test]
fn windows_precheck_without_own_bytes_says_incomparable() {
    use crate::ccm_probe::CcmProbeResult;
    let card = |caps: &[&str]| CcmProbeResult {
        installed: true,
        version: Some("5".into()),
        capabilities: caps.iter().map(|c| c.to_string()).collect(),
        build: Some("p1a-older".into()),
        at: None,
    };
    let at = "$HOME/.cc-monitor/bin/ccm.exe";
    let full = windows_ccm_precheck(Some((at, &card(CC_SPAWN_NEEDS))), None);
    let caveat = crate::copy_table::copy_text("rsCcBusDeploy.win.pathCaveat", &[]);
    assert_eq!(
        full,
        format!("cc-monitor 装的 ccm（{at}）p1a-older · 未带后端字节 · 版本不可比\n{caveat}")
    );
    let lacking = windows_ccm_precheck(Some((at, &card(&["detach"]))), None);
    assert!(
        lacking.contains("版本不可比") && lacking.contains("· 缺 tmux-size"),
        "没带字节时缺的能力照样要点名：{lacking}"
    );
    for said in [&full, &lacking] {
        assert!(
            !said.contains("不是这一版") && !said.contains("是这一版"),
            "没有对照物却判了版本：{said}"
        );
    }
}

/// Windows 上那条生产路径**真的接到了**纯函数上：回的是一句话（`None` 会把「查不了 PATH 上那个」
/// 读成「没问题」），且那句话出自五档之一（不再是那句固定的「没做预检」）。
#[cfg(windows)]
#[test]
fn windows_precheck_is_wired_to_the_real_probe() {
    let w = local_ccm_too_old_warning().unwrap_or_default();
    assert!(
        !w.is_empty(),
        "Windows 上回了 `None` —— 那是把「查不了」读成「没问题」"
    );
    assert!(!w.contains("没做预检"), "生产路径还在说那句过期的话：{w}");
    assert!(w.contains("cc-monitor 装的"), "话里没说清查的是哪一份：{w}");
}
