use super::{
    tmux_exe_in, tmux_in, tmux_present, unavailable_from, unavailable_here, TmuxPlatform, NO_TMUX,
};

/// ★ `K-P4` 红线之一〔NET2 真填〕：**生产 hello 填的就是这台机器的答案**（「`hello.unavailable` 真填」）。
///
/// 与 `wire_tests.rs::hello_unavailable_is_additive_present_and_absent` 是两半：那条钉两形的字节，
/// 本条钉「生产那一格恰好一处、给的是 `unavailable_here()`」—— 换回 `Vec::new()` 就红。
#[test]
fn production_hello_fills_unavailable_from_this_machine() {
    let prod = crate::guard_support::production_code(include_str!("../../src/backend/main.rs"));
    let sites: Vec<&str> = prod
        .lines()
        .map(str::trim)
        .filter(|l| l.starts_with("unavailable:"))
        .collect();
    assert_eq!(
        sites,
        ["unavailable: unavailable_here(),"],
        "`main.rs` 生产段里给 `unavailable` 赋值的地方应当恰好一处、填 `unavailable_here()`\n\
             （0 处 ⇒ 抽取坏了；≥2 处 ⇒ 有第二条路；`Vec::new()` ⇒ 退回了恒空）。实得：{sites:?}"
    );
}

/// unix 权限位那一维两向：没有权限位 ⇒ 恰好是声明了 `no_unix_mode` 的那几条（今天 `files-chmod`）、码是它；有 ⇒ 空。
/// 两侧异源：左边是 `unix_mode_unavailable` 的产出，右边是本条手写的名单。
#[test]
fn the_unix_mode_axis_lists_exactly_the_commands_that_declare_it() {
    let got: Vec<(String, String)> = super::unix_mode_unavailable(false)
        .into_iter()
        .map(|u| (u.command, u.code))
        .collect();
    assert_eq!(
        got,
        [("files-chmod".to_string(), super::NO_UNIX_MODE.to_string())]
    );
    assert!(super::unix_mode_unavailable(true).is_empty());
    // 这份二进制是 unix ⇒ 生产那一格里没有这一维。
    assert!(
        !unavailable_here()
            .iter()
            .any(|u| u.code == super::NO_UNIX_MODE),
        "unix 上的 hello 说了没有 unix 权限位"
    );
}

/// ★★ `K-P4` 红线之二，也是本拍的核心交付：**这个字段不是编译期常量。**
///
/// # 少了本条会怎样
///
/// 只有上一条的话，「`unavailable` 恒空」与「backend 根本答不出这个问题」在判据眼里
/// **一模一样** —— 那样这一拍就只是在 wire 上多挂了一个永远为空的字段，
/// 也就是握手帧上多了一句谁都不会读的话。**「事前协商」一格都没买到，而没有任何东西会说。**
///
/// # 它证的到底是什么
///
/// 同一份二进制，**换一台机器就换一个答案**。所以两半都要证：
/// ① **判定**那一半（`unavailable_from`）—— 三种世界，两种答案，且「判不出来」不倒向「做不到」；
/// ② **读世界**那一半（`tmux_in`）—— 真去文件系统上看，看得见和看不见给不同的答案。
/// 只证 ① 的话，一个 `fn tmux_in(_) -> Option<bool> { Some(true) }` 的退化实现照样绿。
#[test]
fn the_answer_is_a_function_of_the_machine_not_of_the_build() {
    // ── ① 判定那一半：三种"世界"，两种答案 ──────────────────────────────
    assert!(
        unavailable_from(Some(true)).is_empty(),
        "有 tmux 还报做不到 ⇒ 界面会灰掉一个能用的按钮"
    );
    assert!(
        unavailable_from(None).is_empty(),
        "🔴 **「判不出来」被压成了「做不到」** —— 这是本字段最贵的那个错：\n\
             能用的功能会从界面上消失，而这种消失没有任何回音（用户只会以为它不支持）。\n\
             没把握时必须退回今天的行为（照发、点了看命令级 code），那一侧是安全的。"
    );
    let missing = unavailable_from(Some(false));
    let names: Vec<&str> = missing.iter().map(|u| u.command.as_str()).collect();
    // 🔴 `K-R104`（09-13）：2 → **4**。`capture-pane` / `oneshot-session` 上帧面时
    //    各自登记了 `no_tmux`（它们都要起 tmux），**这张表是从 `codes` 派生的**
    //    ⇒ 它们自动进表。这正是本条报错文案里逐字预言的那一形：
    //    「本条红未必是错……那就把这里的期望值补上」。
    //    🔴 〔删用量〕**4 → 3**：`oneshot-session` 随用量 ③ 轴整轴退役
    //    （`control/oneshot_session.rs` 整删、`inbound::REGISTRY` 11 → 10）。
    //    `capture-pane` **留着**（拉屏预览在用），别把两条一起读成退役。
    //    ⚠ 顺序按 `REGISTRY` 的排列，不是字典序。
    assert_eq!(
        names,
        vec!["capture-pane", "kill", "launch"],
        "没有 tmux 的那台机器上，做不到的恰好是 `REGISTRY` 里登记了 `{NO_TMUX}` 的那几条。\n\
             ⚠ 本条红**未必是错**：你要是新加了一条会回 `{NO_TMUX}` 的命令，它已经自动进表了\n\
             （这张表是从 `codes` 派生的，不是手写的）—— 那就把这里的期望值补上。\n\
             实得：{names:?}"
    );
    assert!(
        missing.iter().all(|u| u.code == NO_TMUX),
        "表里出现了不是 `{NO_TMUX}` 的原因：{missing:?}"
    );

    // ── ② 读世界那一半：真去文件系统上看 ────────────────────────────────
    //
    // 夹具**不依赖这台机器上装没装 tmux**（那是世界的事实，不是代码的），
    // 所以两个答案都能精确断言。同 `wire.rs` 那条 `homes` 夹具的纪律。
    assert_eq!(
        tmux_in(None),
        None,
        "`PATH` 没设 ⇒ **无处可查** ⇒ 「没找到」这句话说不出口，只能是「判不出来」"
    );

    let root = std::env::temp_dir().join(format!("ccm-kp4-tmux-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let with = root.join("with");
    let without = root.join("without");
    std::fs::create_dir_all(&with).expect("建夹具目录");
    std::fs::create_dir_all(&without).expect("建夹具目录");

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let fake = with.join("tmux");
        std::fs::write(&fake, b"#!/bin/sh\nexit 0\n").expect("写合成 tmux");
        let mut perm = std::fs::metadata(&fake).expect("读夹具权限").permissions();
        perm.set_mode(0o755);
        std::fs::set_permissions(&fake, perm).expect("给合成 tmux 上执行位");

        assert_eq!(
            tmux_in(Some(with.as_os_str())),
            Some(true),
            "`PATH` 上摆着一个可执行的 `tmux` 却没看见 ⇒ 读世界那一半是瞎的"
        );
        assert_eq!(
            tmux_in(Some(without.as_os_str())),
            Some(false),
            "空目录当 `PATH` 却报「有」⇒ 读世界那一半在撒谎（退化成常量了）"
        );

        // ★ 合起来：**同一份代码，两台不同的机器，两个不同的答案。**
        //   这一步才是「不是编译期常量」的正面证据 —— 上面两组各自都只证了一半。
        // 🔴 `K-R104`：`2` → **4**（`capture-pane` / `oneshot-session` 也登记了
        //    `no_tmux`，这张表从 `codes` 派生 ⇒ 自动进表）。
        // 🔴 **4 → 3**：`oneshot-session` 随用量 ③ 轴退役。
        //    ⚠ 这个数**不许写成地板** —— 「有 tmux 的机器上一条都不报」那一半是
        //    `is_empty()`，而这一半要的是「恰好是登记了 `no_tmux` 的那几条」。
        let today = unavailable_from(Some(false)).len();
        assert!(
            unavailable_from(tmux_in(Some(without.as_os_str()))).len() == today
                && unavailable_from(tmux_in(Some(with.as_os_str()))).is_empty(),
            "端到端：没有 tmux 的机器上要报出 {today} 条做不到，有 tmux 的机器上一条都不报"
        );
    }
    #[cfg(not(unix))]
    {
        // 🔴 这一格断的是**探针**，不是**答案**（下一拍把两者拆开了）：
        //    「扫 `PATH` 找无后缀 `tmux`」这个判准在非 unix 上不等价于 `execvp`
        //    ⇒ 这个**函数**必须不开口。而那台机器上的**答案**由 `TmuxPlatform` 给，
        //    走 `tmux_exe_in`，见 `the_windows_answer_is_confirmed_absent_not_unknown`。
        assert_eq!(
            tmux_in(Some(with.as_os_str())),
            None,
            "非 unix 上这个探针必须不开口 ——`Command::new(\"tmux\")` 在那儿还会看进程自身\
                 目录与当前目录，而且真装了也叫 `tmux.exe`，这个扫描对不上它"
        );
    }
    let _ = std::fs::remove_dir_all(&root);

    // ── ③ 生产入口跑得通（真填那天换过去的就是它）────────────────────────
    //   只断言**与世界无关**的性质：不断言条数 —— 那会变成「跑测试这台机器上装没装 tmux」。
    for u in &unavailable_here() {
        assert!(
            crate::stream::inbound::COMMANDS.contains(&u.command.as_str()),
            "声明做不到的 `{}` 根本不在 `commands` 里 —— 本字段说的是「接得下但做不到」，\
                 「根本不接」那一格由不在 `commands` 里表达",
            u.command
        );
    }
}

/// ★★ `K-P4` 下一拍的正题：**Windows 上那张表不许是空的。**
///
/// # 上一拍在这一格上明确没买到，而它恰好是动机平台
///
/// 上一版 `tmux_in` 首行 `if !cfg!(unix) { return None; }` ⇒ Windows 上恒「判不出来」
/// ⇒ `unavailable_from(None)` 不列 ⇒ **表恒空** ⇒「没有 tmux 却宣称我认 `kill`/`launch`」
/// 在 Windows 上一格没治。病灶是**一个值装了两件事**：探针不工作（真）＋答案未知（假）。
///
/// # 🔴 这个读数是怎么取的，以及它**证不到什么**（本机没有 Windows）
///
/// 走的是**纯函数入参**：`tmux_present` 自己**不碰 `cfg!`**，平台是它的第一个参数。
/// ⇒ 下面每一条都是**在这台 Linux 上真跑出来的**，不是「合成样本上大概会这样」。
/// **它守得住**：那两维怎么合成答案（包括「Windows 那一档必须给确证的 `Some(false)`」）。
/// **它守不住**：Windows 上编出来的二进制**真的选了**那一档 —— 那是 `TMUX_PLATFORM`
/// 那一行的事，由 `the_windows_arm_is_wired_into_the_source`（源码文本）与
/// `#[cfg(windows)] const _`（只在 Windows 编译时开口）各守一半。
/// ⚠ **别把本条读成「Windows 上成立」** —— 本条成立的是「给定平台入参时成立」。
#[test]
fn the_windows_answer_is_confirmed_absent_not_unknown() {
    let root = std::env::temp_dir().join(format!("ccm-kp4-win-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let bare = root.join("bare"); // 什么都没有的一格 PATH
    let msys = root.join("msys"); // 有人把 MSYS2 的 tmux.exe 放上来了
    std::fs::create_dir_all(&bare).expect("建夹具目录");
    std::fs::create_dir_all(&msys).expect("建夹具目录");
    std::fs::write(msys.join("tmux.exe"), b"pretend this is msys2 tmux").expect("写合成 exe");

    // ── ① 正题：没有 tmux.exe 的那台 Windows ⇒ **确证的「没有」**，不是「不知道」──
    assert_eq!(
        tmux_present(TmuxPlatform::AbsentUnlessExeOnPath, Some(bare.as_os_str())),
        Some(false),
        "Windows 那一档又变回「不知道」了 —— 那正是上一拍的病：\n\
             tmux 结构上不存在于 Windows，「没有」这句话在编译期就成立，不需要探针去证。"
    );
    let names: Vec<String> = unavailable_from(tmux_present(
        TmuxPlatform::AbsentUnlessExeOnPath,
        Some(bare.as_os_str()),
    ))
    .iter()
    .map(|u| u.command.clone())
    .collect();
    // 🔴 `K-R104`：同上一条，2 → **4**（`capture-pane` / `oneshot-session` 自动进表）。
    // 🔴 **4 → 3**：`oneshot-session` 随用量 ③ 轴退役。
    assert_eq!(
        names,
        vec![
            "capture-pane".to_string(),
            "kill".to_string(),
            "launch".to_string()
        ],
        "🔴 **Windows 上这张表又空了** —— 这一格就是本拍的正题。\n\
             握手帧第四条面在动机平台上不说话 = 这一拍什么都没买到。\n\
             ⚠ 本条红未必是错：新加了一条会回 `no_tmux` 的命令，它会自动进表 —— 那就补期望值。\n\
             实得：{names:?}"
    );

    // ── ② `PATH` 读不到，Windows 上**仍然**是确证的「没有」──────────────────
    //    这一档的默认值来自**平台**，不来自探针 ⇒ 探针无话可说不影响它。
    //    （unix 那一档正相反：没有默认值，探针不开口就只能是 `None`。）
    assert_eq!(
        tmux_present(TmuxPlatform::AbsentUnlessExeOnPath, None),
        Some(false),
        "Windows 上「PATH 读不到」被读成了「答案未知」—— 又把两件事压回一个值了"
    );

    // ── ③ 探针只能把它**抬成「有」**，不会把它压回「不知道」────────────────
    assert_eq!(
        tmux_present(TmuxPlatform::AbsentUnlessExeOnPath, Some(msys.as_os_str())),
        Some(true),
        "`PATH` 上摆着 `tmux.exe` 却仍报「没有」⇒ 会把一个真能用的按钮灰掉\n\
             （MSYS2 / Cygwin 那台机器上 `Command::new(\"tmux\")` 是真能起来的）"
    );
    assert!(
        unavailable_from(tmux_present(
            TmuxPlatform::AbsentUnlessExeOnPath,
            Some(msys.as_os_str())
        ))
        .is_empty(),
        "有 tmux.exe 还报做不到 ⇒ 界面会灰掉一个能用的按钮"
    );

    // ── ④ 🔴 Linux 那一侧一格都没破：三档原样 ──────────────────────────────
    assert_eq!(
        tmux_present(TmuxPlatform::AskThePath, None),
        None,
        "unix 上 `PATH` 无处可查仍然必须是「判不出来」—— 这一档不许被 Windows 那一档带跑"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let with = root.join("with");
        std::fs::create_dir_all(&with).expect("建夹具目录");
        let fake = with.join("tmux");
        std::fs::write(&fake, b"#!/bin/sh\nexit 0\n").expect("写合成 tmux");
        let mut perm = std::fs::metadata(&fake).expect("读夹具权限").permissions();
        perm.set_mode(0o755);
        std::fs::set_permissions(&fake, perm).expect("给合成 tmux 上执行位");
        assert_eq!(
            tmux_present(TmuxPlatform::AskThePath, Some(with.as_os_str())),
            Some(true),
            "unix 那一档必须仍然走探针 —— 摆着一个可执行的 `tmux` 却说没有"
        );
        assert_eq!(
            tmux_present(TmuxPlatform::AskThePath, Some(bare.as_os_str())),
            Some(false),
            "unix 上 `tmux` 不在 `PATH` 上仍然必须是**确证没有**（不是「不知道」）"
        );
        // ★ 两个平台形状的判准**不许互串**：`tmux.exe` 不是 unix 上的 tmux，
        //   无后缀 `tmux`（且没有执行位）也不是 Windows 认的那个。
        assert_eq!(
            tmux_present(TmuxPlatform::AskThePath, Some(msys.as_os_str())),
            Some(false),
            "unix 那一档把 `tmux.exe` 当成 tmux 了 —— 两个平台的判准串了线"
        );
        assert!(
            !tmux_exe_in(Some(with.as_os_str())),
            "Windows 那个判准把无后缀的 `tmux` 当成 `tmux.exe` 了"
        );
    }

    // ── ⑤ 第三档还在：既不是 unix 也不是 windows ⇒ 仍然老实说「不知道」──────
    assert_eq!(
        tmux_present(TmuxPlatform::NoOpinion, Some(msys.as_os_str())),
        None,
        "「真不知道」这一档被合并掉了 —— 拆的是平台那一维，不是三态处置那条规则"
    );
    assert!(
        unavailable_from(tmux_present(
            TmuxPlatform::NoOpinion,
            Some(bare.as_os_str())
        ))
        .is_empty(),
        "「不知道」被压成了「做不到」—— 能用的功能会从界面上无声消失"
    );

    let _ = std::fs::remove_dir_all(&root);
}

/// ★ `K-P4` 下一拍第二条：**windows 那一支真的写在生产源码里，且只写了一处。**
///
/// # 为什么单有上面那条不够
///
/// 上面那条把平台当**入参**，于是它在 Linux 上跑得出真读数；
/// 代价是：**没有任何东西说生产那一行怎么选这个入参**。
/// 把 `TMUX_PLATFORM` 的 windows 那一支改回 `NoOpinion`，上面那条**照样全绿** ——
/// 表在 Windows 上重新恒空，而没有人会说话。本条钉的就是那一行。
///
/// # 🔴 它守不住什么（本机没有 Windows，这一句必须写在这儿）
///
/// 它读的是**磁盘上的源码文本**，证的是「那一支写在那儿、而且只有一处」；
/// 它**证不了**「Windows 上编出来的二进制真的走了那一支」——
/// 那由 `#[cfg(windows)] const _` 那条编译期断言守，而**那一条在本仓门禁上不存在**。
#[test]
fn the_windows_arm_is_wired_into_the_source() {
    // `TMUX_PLATFORM` 与 tmux 可用性那一族已搬进 `lib.rs` ⇒ 扫两份的全集。
    let prod = crate::guard_support::production_code(&crate::guard_support::backend_root_source());
    let decl = // 搬进 `lib.rs` 时提了权 ⇒ 逐字锚跟着改成 `pub const`。
        // ⚠ 这一条**本该红，也真的红了**：它报的是「有一行包含它但不等于它」并把那一行
        //   原样印了出来 —— 逐字锚的正确失效方式。
        "pub const TMUX_PLATFORM: TmuxPlatform = if cfg!(windows) {";
    let at = guard_core::pin_line(&prod, decl)
        .unwrap_or_else(|why| panic!("生产段里钉不住那一行：{why}\n（找的是 `{decl}`）"));
    let body: Vec<&str> = prod
        .lines()
        .skip(at)
        .take_while(|l| l.trim() != "};")
        .collect();
    // 反空真：抽取跑飞了（没收住尾）时下面几条会在整份文件上恒真。
    assert!(
        (2..=12).contains(&body.len()),
        "抽出来 {} 行 —— 那不是一个三档选择器，抽取器跑飞了",
        body.len()
    );
    let body = body.join("\n");
    let win = body
        .find("AbsentUnlessExeOnPath")
        .expect("windows 那一支不见了 —— 表会在 Windows 上重新恒空");
    let unix = body
        .find("cfg!(unix)")
        .expect("unix 那一支不见了 —— Linux 上会不再走探针");
    assert!(
        win < unix,
        "`cfg!(windows)` 那一支给的不是「确证没有」——两支的次序被换过了：\n{body}"
    );
    assert!(
        body.contains("NoOpinion"),
        "第三档没了 —— 「真不知道」被合并进别的档里了：\n{body}"
    );
    // 「只许一处」：全生产段里把平台翻成值的地方**恰好一个**。
    assert_eq!(
        prod.matches("cfg!(windows)").count(),
        1,
        "生产段里有 {} 处 `cfg!(windows)` —— 平台这一维只许在 `TMUX_PLATFORM` 一处成值，\n\
             第二处就是第二份真相（本工作区最贵的那一类病）。",
        prod.matches("cfg!(windows)").count()
    );
    // windows 那一档给出的必须是一个**确定的布尔**，不是 `None`。
    guard_core::pin_line(
        &prod,
        "TmuxPlatform::AbsentUnlessExeOnPath => Some(tmux_exe_in(path)),",
    )
    .unwrap_or_else(|why| {
        panic!("windows 那一档不再给确定答案了：{why}\n它一旦回 `None`，表在 Windows 上就又空了。")
    });
}

/// ★ `K-P4` 红线之三：**声明用的 code，必须是那条命令自己登记过的 code。**
///
/// 事前那句话与事后那句话要是各说各的词，客户端就得维护**两张**「这句话怎么翻成人话」
/// 的表，而 monitor 侧那张已经写好了（`backend_launch.rs` 等三处逐字「远端未安装 tmux」）。
///
/// # 它真正逮的是什么（不是同义反复）
///
/// 这张表从 `REGISTRY.codes` 派生，但**常量 `NO_TMUX` 那个字面量是第二份拷贝**。
/// 有人把 `REGISTRY` 里的 `no_tmux` 改名（比如收窄成 `tmux_missing`），
/// 派生出来的表会**静默变空** —— 所有测试照绿，而第四条面从此永远不说话。
/// 本条把那次改名变成一次红。
#[test]
fn the_declared_code_is_one_the_registry_already_declares() {
    let owners: Vec<&str> = crate::stream::inbound::REGISTRY
        .iter()
        .filter(|s| s.codes.contains(&NO_TMUX))
        .map(|s| s.name)
        .collect();
    assert!(
        !owners.is_empty(),
        "`REGISTRY` 里没有任何一条命令登记 `{NO_TMUX}` —— 要么那个 code 被改名了、\n\
             要么依赖 tmux 的命令都没了。无论哪种，握手帧第四条面此刻**永远为空**，\n\
             而它自己不会喊疼。（本常量只是拿去查表，`REGISTRY` 才是源头。）"
    );
    for u in unavailable_from(Some(false)) {
        let spec = crate::stream::inbound::REGISTRY
            .iter()
            .find(|s| s.name == u.command)
            .unwrap_or_else(|| panic!("声明了一条 `REGISTRY` 里没有的命令：{}", u.command));
        assert!(
            spec.codes.contains(&u.code.as_str()),
            "给 `{}` 声明的原因 `{}` 不在它自己登记的 codes {:?} 里 ——\n\
                 事前说的和事后回的不是同一句话，客户端得为此维护第二张翻译表。",
            u.command,
            u.code,
            spec.codes
        );
    }
}
