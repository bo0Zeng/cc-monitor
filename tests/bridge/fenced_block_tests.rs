use super::*;

const B: &str = "# === cc-monitor BEGIN";
const E: &str = "# === cc-monitor END";

#[test]
fn no_begin_means_append() {
    assert_eq!(find_pair("a\nb\n", B, E, "x").unwrap(), None);
    assert_eq!(find_pair("", B, E, "x").unwrap(), None);
}

#[test]
fn paired_block_is_found_by_line_index() {
    let t = "a\n# === cc-monitor BEGIN v1 ===\nbody\n# === cc-monitor END ===\nz\n";
    assert_eq!(find_pair(t, B, E, "x").unwrap(), Some((1, 3)));
}

/// **这一条是本模块存在的理由。** 本机侧原先在这种输入上返回 `None` → 追加 →
/// 第二次安装时损坏的 BEGIN 与新块的 END 配对 → **吃掉两者之间的用户代码**。
#[test]
fn begin_without_end_is_an_error_not_an_append() {
    let t = "# my stuff\n# === cc-monitor BEGIN v1 ===\nfunction cc { }\n";
    let e = find_pair(t, B, E, "PowerShell profile").unwrap_err();
    assert!(e.contains("第 2 行"), "要报出是哪一行：{e}");
    assert!(e.contains("找不到配对的 END"), "{e}");
    assert!(e.contains("已中止"), "措辞要让用户知道我们没动文件：{e}");
    assert!(e.contains("PowerShell profile"), "要说清是哪个文件：{e}");
    // 用户可见文案不许带 markdown 星号（前端 toast 是纯文本渲染）
    assert!(!e.contains("**"), "文案里有字面星号：{e}");
}

/// **只找 BEGIN 之后的 END**：BEGIN 前面的 END 不算（独立 `find` 会误配它）。
#[test]
fn end_before_begin_does_not_pair() {
    let t = "# === cc-monitor END ===\nuser stuff\n# === cc-monitor BEGIN v1 ===\nbody\n";
    assert!(
        find_pair(t, B, E, "x").is_err(),
        "前面那个 END 不该被误配成配对"
    );
}

/// 缩进的标记也要认（两侧都用 `trim_start`）。
#[test]
fn indented_markers_are_recognised() {
    let t = "a\n  # === cc-monitor BEGIN v1 ===\nb\n\t# === cc-monitor END ===\n";
    assert_eq!(find_pair(t, B, E, "x").unwrap(), Some((1, 3)));
}

/// 只取**第一个** BEGIN（幂等：重复安装不会因为多个 BEGIN 而漂移）。
#[test]
fn first_begin_wins() {
    let t = "# === cc-monitor BEGIN v1 ===\nx\n# === cc-monitor BEGIN v2 ===\ny\n# === cc-monitor END ===\n";
    assert_eq!(find_pair(t, B, E, "x").unwrap(), Some((0, 4)));
}

// ═══════════════════════════════════════════════════════════════════════
// `KR62D3`：那张账得**判得动**，不然它就只是一段更长的注释
// ═══════════════════════════════════════════════════════════════════════

/// ★★ 每一行都指得出**代码住址**。
///
/// 只判「**有没有**住址」；那个住址今天解析不解析得到，由 `structural_scan` 里
/// 那条扫全仓代码住址的判据管（它会报「找不到这个符号 / 符号搬家了」）——
/// 与 `tool_registry::every_unmanaged_entry_names_a_code_address` 同一套分工。
#[test]
fn every_fence_shape_names_code_addresses() {
    // 反向自检：抽取器不是恒真的（散文里抠不出住址，真住址抠得出）。
    assert!(
        crate::structural_scan::symbol_addresses("装在用户的配置里").is_empty(),
        "抽取器把散文当住址了 —— 本条此刻无效"
    );
    assert!(
        !crate::structural_scan::symbol_addresses("fenced_block.rs::find_pair").is_empty(),
        "抽取器连一个真住址都抠不出来 —— 先查抽取器，别改断言"
    );
    assert!(
        FENCE_SHAPES.len() >= 3,
        "账里少于 3 行 —— `§0c` 数出来就是三套"
    );
    let mut ids: Vec<&str> = FENCE_SHAPES.iter().map(|s| s.id).collect();
    let n = ids.len();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), n, "账里有重名的 id —— 同一个形状两个住址");
    for s in FENCE_SHAPES {
        for (col, addr) in [
            ("install_site", Some(s.install_site)),
            ("pairing", Some(s.pairing)),
            ("uninstall_site", s.uninstall_site),
        ] {
            let Some(addr) = addr else { continue };
            assert!(
                !crate::structural_scan::symbol_addresses(addr).is_empty(),
                "`{}` 的 `{col}` 不是 `<文件>.rs::<符号>` 形态的住址，实得 {addr:?} —— \
                     一条没有住址的账，读的人无从核对，而它会安静地过期",
                s.id
            );
        }
        assert!(!s.differs_in.is_empty(), "`{}` 没写它差在哪", s.id);
        // 这两格是给人读的，但空着等于这张账少了一半 —— 也顺带让它们真的**被读**。
        assert!(!s.host.is_empty(), "`{}` 没写它在哪台机器上", s.id);
        assert!(!s.what_goes_in.is_empty(), "`{}` 没写往里放什么", s.id);
    }
}

/// ★★ **围栏标记指常量，不抄字面量** —— 而且「哪两套该共用、哪两套不许共用」判得出来。
///
/// 共用错了的后果不是难看，是**装一个把另一个整块替换掉**
/// （`account_aliases` 那对标记刻意不同前缀，理由逐字写在它头上）。
#[test]
fn the_markers_are_shared_exactly_where_they_should_be() {
    let by = |id: &str| {
        FENCE_SHAPES
            .iter()
            .find(|s| s.id == id)
            .unwrap_or_else(|| panic!("账里没有 `{id}` 这一行"))
    };
    let remote = by("remote-posix-block");
    let local_block = by("local-posix-block");
    let local_line = by("local-posix-source-line");
    let windows = by("local-windows-ps");

    // ① 两套「整块进 POSIX rc」必须是同一对围栏 —— 那是 `KR62D1`「不是第四套」的一半。
    assert_eq!(
        local_block.begin_marker, remote.begin_marker,
        "本机 POSIX 那一套换了自己的围栏 —— 那就真的成了第四套：\
             同一台机器既当本机又当远端时，装两次会在 rc 里留下两个块"
    );
    assert_eq!(
        local_block.pairing, remote.pairing,
        "本机 POSIX 那一套的配对判定不再与远端同一份"
    );
    // ② 三个**家族**的标记必须两两不同（同族内共用不算）。
    let families = [
        remote.begin_marker,
        windows.begin_marker,
        local_line.begin_marker,
    ];
    for (i, a) in families.iter().enumerate() {
        for b in families.iter().skip(i + 1) {
            assert_ne!(
                a, b,
                "两个家族共用了同一对围栏 —— 装一个会把另一个整块替换掉"
            );
        }
    }
    // ③ 标记确实是那几个常量本身（指过去，不是抄一份长得一样的）。
    assert_eq!(remote.begin_marker, crate::sftp::CCM_PROFILE_BEGIN);
    assert_eq!(windows.begin_marker, crate::profile_installer::BEGIN_MARKER);
    assert_eq!(local_line.begin_marker, crate::account_aliases::RC_BEGIN);
}

/// ★★ 「判定已经收了，装与卸还没收」这句话是**数出来的**。
///
/// ⚠ **它会在事情变好的那天红一次**，那是有意的摩擦（同 `config_surface` 那张
/// 钉死 host 的表）：真把装 / 卸收敛了，就来改这个数并顺手把这张账降几行 ——
/// 而不是让「已经收了」这句话在盘上悄悄变成过去时。
#[test]
fn the_pairing_half_is_converged_and_the_install_half_is_not() {
    let mut pairings: Vec<&str> = FENCE_SHAPES.iter().map(|s| s.pairing).collect();
    pairings.sort_unstable();
    pairings.dedup();
    assert_eq!(
        pairings,
        vec!["fenced_block.rs::find_pair"],
        "配对判定不再只有一份 —— 本模块存在的全部理由就是它只有一份"
    );

    let mut installs: Vec<&str> = FENCE_SHAPES.iter().map(|s| s.install_site).collect();
    installs.sort_unstable();
    installs.dedup();
    assert_eq!(
        installs.len(),
        3,
        "「装」那一半今天有 {} 处独立实现（`§0c` 那三套：远端 SFTP · 本机 PowerShell · \
             本机 POSIX 那一行 source；`K-R62` 新加的那条路**借的是本机 PowerShell 那一台安装器**，\
             所以没有变成第 4 处）。这个数变了就来改它 —— 变小 = 有人收敛了（好事，顺手降账）；\
             变大 = 又长出一套（`KR62D1` 的失效方向）。实得：{installs:?}",
        installs.len()
    );

    // 「有装口没卸口」的那几套，逐条点得出名字 —— 这一格今天恰好一条。
    let no_uninstall: Vec<&str> = FENCE_SHAPES
        .iter()
        .filter(|s| s.uninstall_site.is_none())
        .map(|s| s.id)
        .collect();
    assert_eq!(
        no_uninstall,
        vec!["local-posix-source-line"],
        "「装得进去卸不掉」的那一批变了 —— 它是这张账里最贵的一格，别让它静默增减"
    );
}

/// 每一份被 [`FENCE_SHAPES`] 点到名的源文件。覆盖由下面那条判据钉死。
const SHAPE_FILES: &[(&str, &str)] = &[
    ("sftp.rs", include_str!("../../src/bridge/src/sftp.rs")),
    (
        "profile_installer.rs",
        include_str!("../../src/bridge/src/profile_installer.rs"),
    ),
    (
        "account_aliases.rs",
        include_str!("../../src/bridge/src/account_aliases.rs"),
    ),
    (
        "fenced_block.rs",
        include_str!("../../src/bridge/src/fenced_block.rs"),
    ),
];

/// ★★ 〔`K-R63` 09-11〕**这张账的「卸」那一格也是一句申报，而申报要对得上现实。**
///
/// # 它补的是哪半边
///
/// `uninstall_site` 先前只有 `Some` 那半边被守：形状（`every_fence_shape_names_code_addresses`）
/// ＋ 全仓那条符号地址判据（改名 / 删了会红）。
/// **`None` 那半边一条判据都没有** —— 「今天没有卸口」这句话，盘上真长出一个卸口
/// 也不会有人回来改它。那正是 `tool_registry.rs::TOOLS` 上 `remote-daemon` 栽的坑
/// （`sftp.rs::uninstall_remote_daemon` 是设置面板上的按钮，而字段写着卸不掉）。
///
/// # 🔴 本条同时是 `K-R63 §0c-2` 要的那个**射程读数**
///
/// 件文件写着：`K-R63` 落地之后要回头看 `local-posix-source-line`
/// （唯一 `uninstall_site: None` 的一套）红没红，**没红先查射程是不是漏了它**。
/// ⇒ 本条就是那道射程：它**逐行**走 [`FENCE_SHAPES`]，那一行进了分母
/// （下面的计数自检钉死「一行都不许跳过」）。
///
/// 它今天**绿**，而绿的理由写清楚：本条判的是「**申报 ↔ 现实一致**」，
/// 而那一行的申报（`None`）与现实（`account_aliases.rs` 生产段里
/// 一个 `fn uninstall… / remove… / strip… / purge…` 都没有）**是一致的** ——
/// 它是**缺实现**，不是**假申报**。要它红需要的是另一条性质
/// （「装得进去就必须卸得掉」，即 `tool_registry.rs::fenced_block_implies_uninstallable`
/// 在这张账上的对应物），而那一条今天立起来就是一道**永远红**的闸
/// （补卸口是 `§0d` 明写本件不做的事）—— 立不立由 PM 裁，本条不替它裁。
#[test]
fn a_shape_that_declares_no_uninstall_really_has_none() {
    // ① 语料覆盖：账里点到的每一份文件都在 SHAPE_FILES 里（少一份 ⇒ 那一行悄悄出局）。
    let file_of = |addr: &str| -> &'static str {
        let base = addr.split("::").next().unwrap_or(addr);
        SHAPE_FILES
            .iter()
            .find(|(n, _)| *n == base)
            .unwrap_or_else(|| panic!("`{base}` 不在 SHAPE_FILES 里 —— 账里点了它，而本条读不到它"))
            .1
    };
    // ①b 覆盖是**逐列**查的，不是等用到才查：三列住址点到的每一份文件都要读得到。
    for s in FENCE_SHAPES {
        for addr in [Some(s.install_site), Some(s.pairing), s.uninstall_site]
            .into_iter()
            .flatten()
        {
            let _ = file_of(addr);
        }
    }
    // ② 反向自检：扫描器在真树上认得出一个真的卸载实现（零命中 ⇒ 下面全是空真）。
    assert!(
        crate::structural_scan::fn_names_starting_with(file_of("sftp.rs"), &["uninstall"])
            .contains(&"uninstall_remote_ccm_helper".to_string()),
        "扫描器在真树上零命中 —— 本条此刻无效，先查剥法别改断言"
    );

    // 认领集：账上已经被某一行认走的卸载符号。
    let claimed: Vec<&str> = FENCE_SHAPES
        .iter()
        .filter_map(|s| s.uninstall_site)
        .filter_map(|a| a.split("::").nth(1))
        .collect();

    let mut checked = 0usize;
    for s in FENCE_SHAPES {
        checked += 1;
        match s.uninstall_site {
            // 申报「有卸口」⇒ 那个符号必须真在它说的那份文件里。
            Some(addr) => {
                let sym = addr.split("::").nth(1).unwrap_or_default();
                assert!(
                    crate::structural_scan::fn_names_starting_with(file_of(addr), &[sym])
                        .contains(&sym.to_string()),
                    "`{}` 申报卸口住 {addr:?}，而那份文件的生产段里没有这个 `fn`",
                    s.id
                );
            }
            // 申报「今天没有卸口」⇒ 它家里不许躺着一个没人认领的同族实现。
            // ⚠ 动词表的分母如实写在这里：**登记过的就这四个**，不是穷举。
            None => {
                let home = s.install_site;
                let stray: Vec<String> = crate::structural_scan::fn_names_starting_with(
                    file_of(home),
                    &["uninstall", "remove", "strip", "purge"],
                )
                .into_iter()
                .filter(|n| !claimed.contains(&n.as_str()))
                .collect();
                assert!(
                    stray.is_empty(),
                    "`{}` 登记着「今天没有卸口」，而 {home} 那份文件里躺着没人认领的 \
                         {stray:?} —— 要么它就是卸口（那就把 `uninstall_site` 填上），\
                         要么它不是（那就说清它是什么）",
                    s.id
                );
            }
        }
    }
    // ③ 计数自检：一行都没跳过（`§0c-2` 要的那个「射程有没有漏掉它」的读数就是它）。
    assert_eq!(
        checked,
        FENCE_SHAPES.len(),
        "只走了 {checked} 行，而账上有 {} 行",
        FENCE_SHAPES.len()
    );
}
