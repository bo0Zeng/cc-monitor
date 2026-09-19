use super::*;
use std::collections::BTreeSet;

/// `KR60D1` ③ 用的抽取器 —— **直接用盘上已有那一把**，不自己再写一个。
///
/// 〔`K-R60` 09-11 自抓〕本条第一版手写了一个同形的抠取器（找 `.rs::`、向前吃路径、
/// 向后吃符号），写完才发现 `structural_scan::symbol_addresses` 逐字就是这件事，
/// 而且它那一侧**更强**：还会去全仓解析那个符号今天在不在、有没有搬家。
/// 抄一份被测逻辑正是本仓反复判过的那族病 ⇒ 删掉自己那份，改调它。
fn addresses_in(why: &str) -> Vec<(usize, String, String, bool)> {
    crate::structural_scan::symbol_addresses(why)
}

/// `KR60D1` ①：**清单是一个闭集，而且只有一份。**
///
/// 两半（`TOOLS` 派生的 + [`UNMANAGED_ENV`] 手写的）**不许重叠**，id 不许重名 ——
/// 重了就等于同一个环境项有两个住址，而那正是本件要治的病。
#[test]
fn the_environment_is_one_closed_list() {
    let env = environment();
    let ids: BTreeSet<&str> = env.iter().map(|e| e.id).collect();
    assert_eq!(
        ids.len(),
        env.len(),
        "闭集里有重名的 id —— 同一个环境项两个住址，实得 {:?}",
        env.iter().map(|e| e.id).collect::<Vec<_>>()
    );
    assert_eq!(
        env.len(),
        TOOLS.len() + UNMANAGED_ENV.len(),
        "闭集的人数 ≠ 两半之和 —— environment() 漏了一半还是加了第三份"
    );
    for u in UNMANAGED_ENV {
        assert!(
            !TOOLS.iter().any(|t| t.id == u.id),
            "`{}` 同时在 TOOLS 与 UNMANAGED_ENV 里 —— 有 ToolSpec 的不许再手写一条",
            u.id
        );
    }
}

/// 🔴 `KR60D1` ②：**每一档都必须有人 —— 一档不许靠「没列出来」表示。**
///
/// 这一条就是 `K-R60` 那件正题的门禁：把手写那一半从 [`UNMANAGED_ENV`]
/// 里删光（回到那件之前「不写进去就算另一档」的盘面）⇒ 本条红。
///
/// 🔴 〔`K-R65` 09-11〕**本条自己的报错逐字兑现过一次**：`K38` 把 10 项从
/// 「app 假设它在」搬空之后那一档空了，而报错逐字写着「要么给它一个成员，
/// **要么把这一档从 EnvTier 里删掉**」⇒ 删掉了那一档，档数从三变四（新增两档）。
/// 一条判据把自己的两条出路都写出来，走的是哪一条**有记录**，这就是那次记录。
///
/// 分母现算（`EnvTier::ALL`），不写死一个基数〔`13b`：报一个基数也是复述〕。
#[test]
fn every_tier_has_members_so_absence_never_encodes_a_judgement() {
    let env = environment();
    for tier in EnvTier::ALL {
        let n = env.iter().filter(|e| e.tier == *tier).count();
        assert!(
            n > 0,
            "「{}」这一档在闭集里一个成员都没有（共 {} 档 · 闭集 {} 项）——\n\
                 空的那一档等于**用缺席表达一个判断**，而读者分不出「没有这种东西」\n\
                 与「有人忘了写」。要么给它一个成员，要么把这一档从 EnvTier 里删掉。",
            tier.label(),
            EnvTier::ALL.len(),
            env.len()
        );
    }
}

/// `KR60D1` ③：**「标了档」与「随手填的档」要分得开。**
///
/// 手写那一半的每一条，`why` 里**必须有**一个 `<路径>.rs::<符号>` 形态的代码住址 ——
/// 「这东西该由谁装」是一个**设计判断**，判断得指得出它长在哪段代码上；
/// 一句形容词（「常用工具」「一般都有」）过不去这一格。
///
/// **分工写清，别让人以为这一条买到了两件事**：
/// - 本条只判「**有没有**住址」（缺席这件事只有本条看得见 —— 下面那一条对
///   「一个住址都没写」的条目是**静默放过**的）；
/// - 「那个住址今天**解析不解析得到**」由 `structural_scan` 里那条扫全仓代码住址的
///   判据管，它会报「找不到这个符号 / 符号搬家了」。本轮它真的逮到过一条
///   （第一版把 `tmux` 那条指到了一个枚举**变体**上）。
///
/// ⚠ **诚实边界**：两条加起来买到的是「这条住址指得到一处真代码」，
/// **判不了「这处代码真的就是这一项该指的那处」** —— 那要读语义，机器读不了。
#[test]
fn every_unmanaged_entry_names_a_code_address() {
    // 反向自检（要件 3）：抽取器不是恒真的
    assert!(
        addresses_in("常用工具，一般机器上都有").is_empty(),
        "抽取器把散文当住址了"
    );
    assert!(
        !addresses_in("launch.rs::TERMINAL_EXITS").is_empty(),
        "抽取器连一个真住址都抠不出来 —— 先查抽取器，别改断言"
    );

    let mut checked = 0usize;
    for u in UNMANAGED_ENV {
        let addrs = addresses_in(u.why);
        assert!(
            !addrs.is_empty(),
            "`{}` 的 why 里没有 `<路径>.rs::<符号>` 形态的住址 —— \
                 「这东西该由谁装」是一个**设计判断**（不是「这台机器上恰好有」这个读数），\
                 判断必须指得出它长在哪段代码上。\n实得：{}",
            u.id,
            u.why
        );
        checked += 1;
    }
    assert!(
        checked >= 5 && checked == UNMANAGED_ENV.len(),
        "计数自检：扫到 {checked} 条，而表里 {} 条",
        UNMANAGED_ENV.len()
    );
}

/// ★★ `KR62D1` 的第二条死值验：**`posix-rc-aliases` 升到了第一档，而且是真升。**
///
/// 「档没升 = 活没做完」这句话本身可验 —— 这一条就是它。三格一起断，缺一格都能装样子：
///   ① 它**不在** [`UNMANAGED_ENV`] 里了（留在那儿就没有 `ToolSpec`，也就永远升不到第一档）；
///   ② 它在 [`TOOLS`] 里且 `installable` / `uninstallable` **都为真**（装得了也卸得掉）；
///   ③ [`environment`] 把它算成 [`EnvTier::AppInstalls`]（档是**派生**出来的，不是手填的）。
///
/// **死值验**：把 `installable` 翻回 `false` ⇒ ②③ 双双红；
/// 把这一条搬回 `UNMANAGED_ENV` ⇒ ① 红。
#[test]
fn posix_rc_aliases_sits_in_the_first_tier_now() {
    const ID: &str = "posix-rc-aliases";
    assert!(
        !UNMANAGED_ENV.iter().any(|u| u.id == ID),
        "`{ID}` 还留在 UNMANAGED_ENV 里 —— `K-R62` 之后它有装口也有卸口了，\
             留在手写那一半（没有 ToolSpec ⇒ 永远算作「今天没有装口」）就是盘上写着一句假话"
    );
    let t = TOOLS
        .iter()
        .find(|t| t.id == ID)
        .unwrap_or_else(|| panic!("`{ID}` 不在 TOOLS 里 —— 本机 POSIX 那一格没人申报"));
    assert!(
        t.installable,
        "`{ID}` 申报成装不了 —— 那 `environment()` 会把它算进「app 只查」"
    );
    assert!(t.uninstallable, "`{ID}` 申报成卸不掉 —— 有围栏就必须卸得掉");
    assert!(t.touches().next().is_some(), "`{ID}` 装得了却没申报落点");
    let tier = environment()
        .into_iter()
        .find(|e| e.id == ID)
        .map(|e| e.tier)
        .expect("闭集里找不到它");
    assert_eq!(
        tier,
        EnvTier::AppInstalls,
        "`{ID}` 在闭集里的档不是「{}」—— `K-R62` 那一格没做完",
        EnvTier::AppInstalls.label()
    );
}

// 🔴 〔`K-R65` 09-11〕〔散文墓碑〕**`a_hand_written_entry_is_never_app_installs` 删了。**
//
// 它逐字断言 `UNMANAGED_ENV` 每一条的 `tier != EnvTier::AppInstalls`，
// 理由是「装得了就该有一条 `ToolSpec`」。那时 `tier` 是**手填**的 ⇒ 它真有牙。
//
// 今天 `tier` 由 [`EnvTier::of`] 派生，而手写那一半的第二格在 [`environment`] 里
// **恒 `false`** ⇒ `EnvTier::of(_, false)` 一辈子返回不了 `AppInstalls`
// ⇒ 本条**在算术上不可能红**。留着就是一颗「永远不会红的钉子」——
// `K-R60` 09-11 刚以同一条理由删过一颗（那条断言「档 == `if installable {…}`」，
// 是拿实现自己核自己），本仓更早还删过一颗按 `destination` 推 locality 的。
// ⇒ 同一把尺子，删。
//
// ⚠ **它守的那件事没有丢，只是换了守法**：从「断言一个手填格不许是某个值」
// 变成「那个格子根本不存在」——`UnmanagedEnv` 上今天**没有** `tier` 字段可填。

/// 🔴 `KR65D2` ①：**「谁该装」与「今天装得了吗」不是同一个字段** —— 两格都有区分力，
/// 而且**互不函数**（知道一格答不出另一格）。
///
/// 形抄本仓已有的 `config_surface::host_is_not_a_function_of_destination`：
/// 两个字段合成一个的病，靠「找得到两对反例」证伪。
///
/// **死值验**：把 `who` 改成 `if has_installer { AppShips } else { UserProvides }`
/// 那种派生（也就是把两格又合回去）⇒ 下面两组反例必有一组空 ⇒ 红。
#[test]
fn who_should_install_is_not_a_function_of_whether_we_can_install_today() {
    let env = environment();
    // 「今天有没有装口」这一格在闭集里的读法：只有 `AppInstalls` 那一档是「有」。
    let has_installer = |e: &EnvEntry| e.tier == EnvTier::AppInstalls;

    // ① 同一个 `who`，两种「有没有装口」—— 否则 `who` 就是那一格的同义词。
    let ships: Vec<&EnvEntry> = env
        .iter()
        .filter(|e| e.who == Provisioning::AppShips)
        .collect();
    assert!(
        ships.iter().any(|e| has_installer(e)) && ships.iter().any(|e| !has_installer(e)),
        "「app 自带」这一群里，「今天有装口」与「今天没有装口」**没有同时出现** ——\n\
             那说明这两格今天是同一个字段的两个名字，而 `KR65D2` 的题面正是它们不是。\n\
             实得：{:?}",
        ships
            .iter()
            .map(|e| (e.id, e.tier.label()))
            .collect::<Vec<_>>()
    );

    // ② 同一种「没有装口」，两个不同的 `who` —— 否则「没装口」就唯一决定了「谁该装」，
    //    那正是本件之前的盘面（没装口 ⇒ 只能写成「不该我们装」）。
    let no_installer: BTreeSet<Provisioning> = env
        .iter()
        .filter(|e| !has_installer(e))
        .map(|e| e.who)
        .collect();
    assert!(
        no_installer.len() >= 2,
        "「今天没有装口」的那一群里，「谁该装」只有一个取值（{:?}）——\n\
             那就等于说「没装口」= 「不该我们装」，而 `K38` 裁的恰恰相反：\n\
             `cc-acct-iso-local` **该由 app 装**，只是实现还欠着。",
        no_installer.iter().map(|w| w.label()).collect::<Vec<_>>()
    );
}

/// 🔴 `KR65D2` ②：**「app 该自带、而今天还没有装口」那一格数得出来，且不是空的。**
///
/// 死值验的两侧（件计划 `KR65D2` 逐字）：
///   · 把 `cc-acct-iso-local` 标成「app 该装」而不给实现 ⇒ **能被数出来**（就是本条）；
///   · 把它标成「不该我们装」⇒ **必须红**（那一格由
///     `everything_the_charter_named_as_ours_is_in_the_shipped_population` 判）。
///
/// ⚠ 本条**不判「这一格里该有几项」** —— 那要读语义。它判的是这一格**存在、非空、
/// 且每一项都答得出「欠的是什么」**（`why` 里那个代码住址由另一条判据管）。
#[test]
fn the_tier_for_owed_installers_is_countable_and_not_empty() {
    let env = environment();
    let owed: Vec<&EnvEntry> = env
        .iter()
        .filter(|e| e.tier == EnvTier::AppShipsNoInstallerYet)
        .collect();
    assert!(
        !owed.is_empty(),
        "「{}」这一档一个成员都没有 —— 要么本件的活退回去了（`cc-acct-iso-local` \
             又被写成「不该我们装」），要么装口真补上了（那它该升到「{}」，\
             同轮把这一条改成新的下界）",
        EnvTier::AppShipsNoInstallerYet.label(),
        EnvTier::AppInstalls.label()
    );
    for e in &owed {
        assert_eq!(
            e.who,
            Provisioning::AppShips,
            "`{}` 落在「欠装口」那一档，而它的 `who` 不是「{}」—— 派生坏了",
            e.id,
            Provisioning::AppShips.label()
        );
        // 「有名字、看得见」：档名本身必须说清是**欠的实现**，不是「不该我们装」。
        assert!(
            e.tier.label().contains("该自带") && e.tier.label().contains("还没有装口"),
            "这一档的档名读不出「该我们装、而今天还没有装口」两半，实得 {:?} —— \
                 用户会把它读成「不该我们装」",
            e.tier.label()
        );
    }
}

/// 🔴 `KR65D3`：**「app 自带的是哪几样」是一个数出来的人群，不是写死的一张单子。**
///
/// # 人群住哪儿
///
/// 唯一住址是 [`Provisioning::AppShips`]，**现算**：`environment()` 里 `who` 是它的那几项。
/// `TOOLS` 那一半由 `destination` 派生，手写那一半显式声明 —— 两侧都不在这条判据里。
///
/// # 下面这张 `NAMED_AS_OURS` **不是自带清单**，别读错
///
/// 它收的是**定框与用户逐字点过名的那几样**，用途是**下界对拍**：
/// 点了名的，派生出来的人群里必须有。它不会变成人群的第二个住址 ——
/// 下面第 ④ 条自检就是钉这件事的：**人群必须严格大于这张表**，
/// 否则「派生」这句话是假的。
///
/// ⚠ 〔来历，别删 —— 这张表自己就是「点名清单不等于人群」的证据〕
/// - `pb skill` 曾经在这张表上（`K38` 初版逐字点了它的名）——**用户当拍改口撤掉了**：
///   它不自带，它在 `skill_host.rs::SKILLS`（app 的 skill 仓库）那条线上。
///   ⇒ 那条线本件不碰，两张表**本来就是不同人群**，不许拿相等去守。
/// - `code-picture-sidecar` **不在 `K38` 的举例里**，是用户当拍凭记忆追问出来的
///   （PM 拟这条 dod 时只数出两样）⇒ **点名清单本身就会漏**，
///   这正是「人群必须是数出来的」那句话的来历。
///   ⚠ 〔条 67 · 09-18〕那一项**已随 `sidecars/` 整棵删除而摘掉** —— 留这段话是因为
///   「点名清单本身就会漏」这条教训与它在不在表上无关。
///
/// **死值验**：把 `cc-bus` 从「自带」里摘掉（例如在 [`environment`] 里给它硬写
/// `Provisioning::NotAnInstall`）⇒ 本条红。
/// ⚠ 〔条 67 · 09-18〕**第二条死值验没有了** —— 它钉的是 `sidecars/` 那一层，
/// 而那一层整棵删了。⇒ 今天这条判据**只剩一条死值验**（`cc-bus` 那条）。
/// 如实记：射程比 09-11 那一拍窄了一格，不是"一样强"。
#[test]
fn everything_the_charter_named_as_ours_is_in_the_shipped_population() {
    /// `(闭集里的 id, 谁在什么时候点的名)`。**只收逐字点过名的**，不收推断出来的。
    const NAMED_AS_OURS: &[(&str, &str)] = &[
        ("cc-bus", "K38 逐字：「cc-bus」"),
        ("cc-acct-iso", "K38 逐字：「account」—— 远端那半"),
        (
            "cc-acct-iso-local",
            "K38 逐字：「account」—— 本机那半（件计划 §0c 明裁它是 app 独有的）",
        ),
    ];
    let env = environment();
    // ① 人群**现算**，不抄名单。
    let shipped: BTreeSet<&str> = env
        .iter()
        .filter(|e| e.who == Provisioning::AppShips)
        .map(|e| e.id)
        .collect();
    assert!(
        !shipped.is_empty(),
        "「app 自带」这个人群是空的 —— 先查 `Provisioning::of_tool` 与 `environment`，别改断言"
    );

    // ② 保鲜自检：定框点过名的 id 必须还在闭集里（改了名 / 删了 ⇒ 本表当场腐）。
    let all: BTreeSet<&str> = env.iter().map(|e| e.id).collect();
    for (id, src) in NAMED_AS_OURS {
        assert!(
            all.contains(id),
            "`{id}` 在闭集里已经找不到了（它当初的来历：{src}）—— \
                 要么它改名了（这张表跟着改），要么它真没了（那 `K38` 那一格要重裁）"
        );
    }

    // ③ 下界对拍：定框点了名的，必须在派生出来的人群里。
    let missing: Vec<&str> = NAMED_AS_OURS
        .iter()
        .filter(|(id, _)| !shipped.contains(id))
        .map(|(id, _)| *id)
        .collect();
    assert!(
        missing.is_empty(),
        "`K38` 逐字点名要自带的东西，在「app 自带」这个人群里**找不到**：{missing:?}\n\
             人群现算于 `environment()` 的 `who == AppShips`，今天是 {shipped:?}。\n\
             ⇒ 要么那几项的申报错了（改申报），要么 `K38` 那一格要重裁（改定框，不是改这里）。"
    );

    // ④ **人群不许退化成这张表** —— 严格大于，才说明它是派生出来的。
    assert!(
        shipped.len() > NAMED_AS_OURS.len(),
        "「app 自带」这个人群（{} 项）没有比定框举的例子（{} 项）更大 —— \
             那说明它其实是照这张表抄的，而不是数出来的。人群实得 {shipped:?}",
        shipped.len(),
        NAMED_AS_OURS.len()
    );
}

// 🔴 **〔条 67 · 2026-09-18〕那条「我们随产品分发二进制的那一层」跨半判据删了。**
// 它钉的是「`sidecars/` 那一层还在盘上」∧「闭集里申报了它」同时成立。
// 那一层删了、那条申报也摘了 ⇒ 判据的两边都不在，**留着它就是一条恒红或恒空真的尺子**。
// ⚠ 它当初买的那个道理**别丢**：「光在闭集里加一行是**申报**，申报会在那一层被掏空之后
//   照样绿着 —— 那正是 `remote-daemon` 的 `uninstallable: false` 假申报活了一个月那一族。」
//   ⇒ 下一次往闭集里加「app 自带」的条目时，仍然要去**钉真源码**，不许只申报。

/// 「谁该装」三值**都真有人用** —— 一个只有一个取值的字段没有区分力。
/// 形抄 `config_surface::all_host_scopes_are_really_used`。
#[test]
fn every_provisioning_value_is_really_used() {
    let env = environment();
    for w in Provisioning::ALL {
        assert!(
            env.iter().any(|e| e.who == *w),
            "「{}」这个取值在闭集里一项都没有（共 {} 值 · 闭集 {} 项）—— \
                 没人用的取值要么删掉，要么它就是漏了",
            w.label(),
            Provisioning::ALL.len(),
            env.len()
        );
    }
}

/// **「不该我们装」的东西不许有装口** —— [`EnvTier::of`] 里那两支 `_` 会把这种
/// 自相矛盾**吸收成一个正常档**，所以矛盾本身要在这里单独判红，不能靠那个 `match`。
#[test]
fn nobody_declares_an_installer_for_something_we_should_not_install() {
    for t in TOOLS {
        let who = Provisioning::of_tool(t);
        if who != Provisioning::AppShips {
            assert!(
                !t.installable,
                "`{}` 的落点说它「{}」，而 `installable: true` 说我们装得了 —— \
                     两句话有一句是假的",
                t.id,
                who.label()
            );
        }
    }
}

/// `KR65D1` 的申报侧：**「你自己装」那一档的每一项都要说得出「怎么查」** ——
/// 而且**不许整档都是「查不动」**（那就等于把「不查」换了个名字，正是失效方向）。
///
/// ⚠ 行为那一半（真去查、缺了显示成 `Absent` 而不是 `Undetermined`）**不在这里** ——
/// 在 `config_surface::the_prompt_tier_really_looks_before_it_speaks`。
/// 本条只判申报，别把两条读成一条。
#[test]
fn the_prompt_tier_declares_how_it_will_look() {
    let env = environment();
    let mut probeable = 0usize;
    let mut blind = 0usize;
    for e in env
        .iter()
        .filter(|e| e.tier == EnvTier::UserInstallsWePrompt)
    {
        let EnvBacking::Named { probe, .. } = e.backing else {
            panic!(
                "`{}` 在「{}」这一档，却有 ToolSpec —— 那一档今天只该住手写那一半",
                e.id,
                EnvTier::UserInstallsWePrompt.label()
            )
        };
        match probe {
            EnvProbe::OnPath | EnvProbe::HomePath => probeable += 1,
            EnvProbe::CannotProbe { why } => {
                blind += 1;
                assert!(
                    why.len() > 30,
                    "`{}` 申报「查不动」而理由只有 {} 字节 —— \
                         「查不动」与「懒得查」在表上长得一模一样，理由是唯一分得开的东西",
                    e.id,
                    why.len()
                );
            }
        }
    }
    assert!(
        probeable + blind > 0,
        "「{}」这一档一个成员都没有 —— 那 9 项没搬过来",
        EnvTier::UserInstallsWePrompt.label()
    );
    assert!(
        probeable > blind,
        "「{}」这一档里查得动的 {probeable} 项、查不动的 {blind} 项 —— \
             查不动的过半就等于这一档只是「app 假设它在」换了个名字，\
             而那正是 `KR65D1` 写死的失效方向",
        EnvTier::UserInstallsWePrompt.label()
    );
}

/// 派生那一半：**`TOOLS` 的每一条都必须进闭集**，一条都不许漏。
///
/// 🔴 **本条上一版还断言了「档 == `if installable {…} else {…}`」，那是同义反复，已删。**
/// 〔`K-R60` 09-11 自抓，是本轮 `7u` 那一刀逼出来的：把实现整个退掉之后它**仍然绿**——
/// 因为 `environment()` 的档就是那个表达式算的，判据再算一遍等于拿它自己核它自己。〕
/// 本仓删过一颗同形的钉子（`config_surface` 那条按 `destination` 推 locality 的），
/// 理由逐字：「不留永远不会红的钉子」。
/// ⇒ 留下的是**够得着的那一半**：覆盖。漏掉一条 `TOOLS` ⇒ 本条红。
/// 而「字段填错了」那一格**不由本条守**，由读字段的那条（它右边钉的是实现，不是同一个表达式）。
#[test]
fn every_managed_tool_reaches_the_closed_set() {
    let env = environment();
    for t in TOOLS {
        let n = env.iter().filter(|e| e.id == t.id).count();
        assert_eq!(
            n, 1,
            "`{}` 在 TOOLS 里，而闭集里出现 {n} 次（应为 1）——\
                 闭集漏了它，这一页上就看不见它",
            t.id
        );
    }
}

// 🔴 〔`K-R63` 09-11〕**`KR60D3` 那条判据从这里搬走了，而不是删掉。**
//
// 它做的事（左边读 `cc-bus` 的 `installable` 字段、右边用 `pin_definition` 钉
// `cc_bus_deploy.rs` 里那个部署函数的签名、两边 `assert_eq!`）今天由
// `every_tool_declares_install_and_uninstall_as_the_implementations_really_are`
// 做，而那一条**对全表每一条的两格都做**。
//
// 为什么非搬不可（`KR63D2` 的正题）：那一条的**名字里带工具名** ——
// 读的人会以为「申报与实现对不对得上」这一格有人守，而它只守 `cc-bus` 一个工具。
// `K-R60` 收窗口时 PM 的刀 γ 就现打过同一件事的另一半；`K-R63` 的刀 C 更直接：
// 〔PM 09-11 现打，量于 `cd26954`〕翻 `cc-acct-iso` 的 `uninstallable`
// ⇒ 全表 1379 条一条没红。
// ⇒ **别再在这里加第二颗专名钉子**；要加就加进那张对拍表。
