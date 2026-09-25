use super::{death_events_available, self_healing_caveat, NO_DEATH_EVENTS_HERE};

/// ★ 本机这一格（**负例**）：Linux 上有那条腿 ⇒ 没有要声明的话。
#[cfg(target_os = "linux")]
#[test]
fn on_linux_the_leg_is_there_so_there_is_nothing_to_declare() {
    assert!(
        death_events_available(),
        "Linux 上这一格回了「没有」—— 而 `pidwatch::linux` 的 `pidfd_open` + \
             `poll(pidfd, POLLIN, -1)` 就在旁边、今天在门禁里绿着。\n\
             回「没有」会让上层在**唯一一个真有这条腿的平台**上也说自愈不成立。"
    );
    assert_eq!(
        self_healing_caveat(),
        None,
        "Linux 上多说了一句声明 —— 那句话是给**没有**那条腿的平台准备的。\n\
             这一格是本组的负例：它恒 `Some(..)` 的话，上面那条源码判据照样绿。"
    );
}

/// ★★ 非 Linux 那一支**写在源码里**，且它给的是「确证没有」。
#[test]
fn the_non_linux_arm_is_wired_into_the_source() {
    let prod = guard_core::production_code(include_str!(
        "../../../src/backend/platform/pidwatch/mod.rs"
    ));
    const SIG: &str = "pub(crate) const fn death_events_available() -> bool {";
    let lines: Vec<&str> = prod.lines().map(str::trim).collect();
    // 反空真①：剥法跑飞 / 文件被掏空时，下面几条会在一份空文本上恒真。
    assert!(
        lines.len() >= 20,
        "生产段只剩 {} 行 —— 剥法坏了或文件被掏空，本条此刻是空转的",
        lines.len()
    );
    // 反空真②：**恰好三条臂**〔WN1：Windows 有了自己的那条腿，是第三个真实的平台答案〕。
    //           多一条 = 多了一个平台答案（第二份真相）；
    //           少一条 = 有人把平台这一维塞回了一行 `cfg!()`，护栏的人群就够不着它了。
    assert_eq!(
        lines.iter().filter(|l| **l == SIG).count(),
        3,
        "`death_events_available` 的臂数不是 3 —— 平台这一维只许在这三条 `#[cfg]` 臂上成值。\n\
             写成 `const X: bool = cfg!(target_os = \"linux\");` 那种一行式**看着更干净**，\n\
             但那一行不带 `#[cfg]` 属性 ⇒ 整个掉出 `platform/fallback_guard.rs` 的人群，牙就没了。"
    );
    // 每条臂：往上找最近的那个 cfg 属性行，往下取**第一条非空行**当它的值。
    //
    // ⚠ 刻意不去配对花括号：这两条臂的体各只有一行（`true` / `false`），
    //   而「取到收尾大括号为止」要在测试里写一个裸的右大括号字面量 ——
    //   而本文件正被 `platform/fallback_guard.rs` 与本条自己当**数据**读，
    //   往里塞结构字符是给两条剥法各添一个陷阱。
    let mut arms: Vec<(&str, &str)> = Vec::new();
    for (i, l) in lines.iter().enumerate() {
        if *l != SIG {
            continue;
        }
        let cfg = lines[..i]
            .iter()
            .rev()
            .find(|p| p.starts_with("#[cfg("))
            .copied()
            .unwrap_or("<这条臂上没有 cfg 属性>");
        let body = lines[i + 1..]
            .iter()
            .find(|p| !p.is_empty())
            .copied()
            .unwrap_or("");
        arms.push((cfg, body));
    }
    // 反空真③：抽取器真的取到了三条，且体不是空的（取行跑飞会给空串）。
    assert_eq!(arms.len(), 3, "抽出来 {} 条臂 —— 抽取器跑飞了", arms.len());
    for (cfg, body) in &arms {
        assert!(
            !body.trim().is_empty(),
            "`{cfg}` 那条臂抽出来是空的 —— 取行跑飞了，下面的判断没有意义"
        );
    }
    let arm_of = |cfg: &str| -> String {
        arms.iter()
            .find(|(c, _)| *c == cfg)
            .map(|(_, b)| b.trim().to_string())
            .unwrap_or_else(|| panic!("找不到 `{cfg}` 那条臂 —— 两条臂的 cfg 被换过了：{arms:?}"))
    };
    assert_eq!(
        arm_of("#[cfg(not(any(target_os = \"linux\", windows)))]"),
        "false",
        "没有进程看守的那条臂（既非 Linux 也非 Windows）不再是「确证没有」。\n\
             `fallback::watch_pid_until_exit` 什么都不做、`on_dead` 永远不会被调用 ——\n\
             这里给一个乐观值就是替一条不存在的腿担保，而上层会拿它当「崩了会有人管」。\n\
             ⚠ 也不许改成「不知道」：那正是 `K-P4` 拆开的那条病（一个值装了两件事），\n\
             而「不知道」在这一格会被下游读成「也许有」。"
    );
    assert_eq!(
        arm_of("#[cfg(target_os = \"linux\")]"),
        "true",
        "Linux 那条臂不再说「有」—— 而那条腿（pidfd + poll）今天就在旁边绿着"
    );
    assert_eq!(
        arm_of("#[cfg(windows)]"),
        "win32::WAKES_ON_EXIT",
        "Windows 那条臂不再读 `win32.rs` 紧挨着实现的那个声明 —— 写成字面量会被 \
         `fallback_guard` 判成伪造成功（它说得对：Windows 块里的「有」要有东西背书），\
         写成别的就是第二份真相〔WN1：那条腿只到编得过 ＋ 源码对拍〕"
    );
    // ★ 编译期那一半也要真的写在那儿：本机编不到它，只能读源码文本。
    //   （`#[cfg(not(any(target_os = "linux", windows)))]` 在本文件里出现四处 —— `mod fallback;` ·
    //    那条 `use` · 上面那条臂 · 本断言 ⇒ 不用 `pin_line`，它要求整份文件里恰好一行。）
    guard_core::pin_line(&prod, "const _: () = assert!(").unwrap_or_else(|why| {
        panic!(
            "{why}\n\
                 ⇒ 那条**只在非 Linux 编译时开口**的编译期断言不见了。\n\
                 源码文本判据只证「那一支写在那儿」，证不了「那一支真的被编进去了」——\n\
                 两者证的不是同一件事，所以两条都要在\n\
                 （形状照 `main.rs` 那条 `#[cfg(windows)] const _`，`K-P4` 立的）。"
        )
    });
}

/// ★ 那句话必须真的**说出「没有」**，不是一句读不出结论的散文。
#[test]
fn the_caveat_says_out_loud_that_there_is_no_self_healing() {
    // 承重词**运行时拼** —— 写成整串会让本条命中本文件自己的散文。
    let no_leg = format!("没有{}", "「进程死了会有人被叫醒」这条腿");
    let structural = format!("结构性{}", "不成立");
    for needle in [no_leg.as_str(), structural.as_str()] {
        assert!(
            NO_DEATH_EVENTS_HERE.contains(needle),
            "那句话里没有 `{needle}` —— `KP3D` 买的是「说出口」，\
                 一句读不出「没有」的散文等于没说"
        );
    }
    // 反向：不许在这句话里承诺自愈。
    for forbidden in ["会自动重起", "会自己再起来"] {
        assert!(
            !NO_DEATH_EVENTS_HERE.contains(forbidden),
            "那句话里出现了 `{forbidden}` —— 它是给**没有**那条腿的平台说的，\
                 在那儿承诺自愈是一句做不到的话"
        );
    }
    // 反空真：那句话不许被掏空成一个占位串。
    assert!(
        NO_DEATH_EVENTS_HERE.chars().count() >= 40,
        "那句话只有 {} 个字 —— 掏空到这个长度，上面几条就靠一个占位串恒绿",
        NO_DEATH_EVENTS_HERE.chars().count()
    );
}
