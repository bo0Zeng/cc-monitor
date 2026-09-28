use super::*;

// ═══════════════════════════════════════════════════════════════════════════
// `KR62D3`：**同一件事今天有几套形状 —— 一条有住址的账**
// ═══════════════════════════════════════════════════════════════════════════

/// 「把 cc-monitor 的一块东西装进一份 shell 配置」这件事的**一个形状**。
///
/// # 为什么要有这张表（而不是把它写进某段注释里）
///
/// `K-R62 §0c` 现打到的那条：同一件事按宿主分了三套形状，判定那一半 `fenced_block`
/// 已经收了（三套全走 [`find_pair`]），**装与卸那一半没收**。
/// 那段话本来只活在件文件的正文里 —— 而 `K-R60` / `K-R61` 两件已经连着证明：
/// **写在注释里而字段 / 判据看不见，等于没写**（一句真话摆错了格，和假话一样是假举证，`K29`）。
///
/// ⇒ `K-R62` **不收敛它们**（那是另一个量级，见件文件 `§0e`），只把它**登记成一条有住址的账**，
/// 然后由 PM 裁收不收、什么时候收。这张表买到三件下面各有判据看着的东西：
///   ① 每一行都指得出**代码住址**（`<文件>.rs::<符号>`），`structural_scan` 会去验它解析得到；
///   ② 围栏标记**指**各自那个常量，不在这里抄字面量（抄一份就是第二个住址）；
///   ③ 「判定已收 / 装卸未收」这句话是**数出来的**，不是形容出来的。
pub(crate) struct FenceShape {
    /// 稳定 id。
    pub id: &'static str,
    /// 哪台机器上的哪份文件。
    pub host: &'static str,
    /// 往里放什么。
    pub what_goes_in: &'static str,
    /// 这一套认哪一对围栏的 BEGIN。**指常量，不抄字面量。**
    pub begin_marker: &'static str,
    /// 装那一半住哪（`<文件>.rs::<符号>`）。
    pub install_site: &'static str,
    /// 卸那一半住哪；`None` = **今天没有卸口**（那本身就是一条账）。
    pub uninstall_site: Option<&'static str>,
    /// 配对判定走哪一份。三套今天是同一份 —— 这一格就是「已经收了哪一半」的读数。
    pub pairing: &'static str,
    /// 它与别的形状**差在哪**。
    pub differs_in: &'static str,
}

/// 🔴 **那三套形状 + `K-R62` 新加的那一条，唯一一份账。**

///
/// ⚠ 它**不判对错**，只记「今天是什么样」。要不要收敛由 PM 裁。
pub(crate) const FENCE_SHAPES: &[FenceShape] = &[
    FenceShape {
        id: "remote-posix-block",
        host: "远端 POSIX 的 ~/<用户选的那份 rc>",
        what_goes_in: "整块别名 snippet（src/shared/ccm-aliases.sh）",
        begin_marker: super::super::block::CCM_PROFILE_BEGIN,
        // 〔AL2 · 第四波 4D〕远端那两条命令并进 `aliases_block_*`（带 `origin`）⇒ 装口与本机同一处。
        install_site: "block.rs::install_to_profile",
        uninstall_site: Some("block.rs::uninstall_from_profile"),
        pairing: "fence.rs::find_pair",
        differs_in: "与本机那一套同一个装口，只差门的 origin（经那台远端后端 files-put 写）",
    },
    FenceShape {
        id: "local-windows-ps",
        host: "本机 Windows 的 PowerShell profile",
        what_goes_in: "整块 PowerShell 代码（src/shared/cc.ps1.tpl 渲染）",
        begin_marker: super::super::block::BEGIN_MARKER,
        install_site: "block.rs::install_to_profile",
        uninstall_site: Some("block.rs::uninstall_from_profile"),
        pairing: "fence.rs::find_pair",
        differs_in: "内容是**现渲**的（命令名与要不要带 cc 函数由界面给），另两套写的是仓里那份文件本身；\
                     而且它要保住 CRLF（fence.rs::detect_eol，`Layout::PowerShell` 那一臂）",
    },
    // 〔TL1 · 4C〕墓碑：这里从前还有一行 `local-posix-source-line`（本机 rc 里包着「一行 source」的那一对围栏，
    //   唯一**没有卸口**的一套）。那一步退役了（`设计/71 §6.1`「source 那一行只许一处装」：接上别名文件的那一行只住别名块里，
    //   选了 rc 只查不装）⇒ 这一套不再存在，账降一行；「装得进去卸不掉」那一格随之清零。盘上已有的那一块不读不删。
    // ★★ 〔`K-R62` 09-11〕**本件新加的那条路，就是这一行。**
    FenceShape {
        id: "local-posix-block",
        host: "本机 POSIX 的 ~/<用户选的那份 rc>",
        what_goes_in: "整块别名 snippet —— **与 `remote-posix-block` 同一个常量**（block.rs::CCM_WRAPPER_SNIPPET）",
        // 与远端那一套**同一个常量**：本机与远端装进 rc 的是同一个东西（`K15` / `K36`）。
        begin_marker: super::super::block::CCM_PROFILE_BEGIN,
        // 🔴 **落盘那一跳与 `local-windows-ps` 是同一处** —— 这一行的 `install_site`
        //    与它逐字相同，不是笔误：补这一格没有多出第四台安装器，多出来的只是
        //    那一台安装器的第二种方言（分岔在 block.rs::plan_install）。
        install_site: "block.rs::install_to_profile",
        uninstall_site: Some("block.rs::uninstall_from_profile"),
        pairing: "fence.rs::find_pair",
        differs_in: "与 `remote-posix-block` **只差落盘那一跳**（本地原子替换 vs SFTP）：\
                     内容、围栏、合块与剥块的实现全共用；与 `local-windows-ps` 只差**方言**\
                     （分岔在 block.rs::plan_install / \
                     block.rs::plan_uninstall，落盘与备份回滚那一整套共用）。\
                     ⇒ 补这一格没有把三套变成四套。判据 \
                     block_tests.rs::the_local_posix_port_is_byte_for_byte_the_remote_one",
    },
];

/// 〔MIG-3a〕从 monitor 搬来时，住址抽取器（`structural_scan::symbol_addresses` / `fn_names_starting_with`）留在 monitor ——
/// 这里是本族够用的两把小尺（`<文件>.rs::<符号>` 形态 · 生产段里 `fn <前缀>…` 的名字）。
fn symbol_addresses(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for w in text.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '.' || c == ':'))
    {
        if let Some((file, sym)) = w.split_once(".rs::") {
            let ok_file =
                !file.is_empty() && file.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
            let ok_sym = sym
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
                && sym.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
            if ok_file && ok_sym {
                out.push(w.to_string());
            }
        }
    }
    out
}

fn fn_names_starting_with(src: &str, prefixes: &[&str]) -> Vec<String> {
    let code = guard_core::production_code(src);
    code.split("fn ")
        .skip(1)
        .filter_map(|rest| {
            let name: String = rest
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                .collect();
            prefixes.iter().any(|p| name.starts_with(p)).then_some(name)
        })
        .collect()
}

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
        symbol_addresses("装在用户的配置里").is_empty(),
        "抽取器把散文当住址了 —— 本条此刻无效"
    );
    assert!(
        !symbol_addresses("fence.rs::find_pair").is_empty(),
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
                !symbol_addresses(addr).is_empty(),
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
/// （〔TL1〕从前 `account_aliases` 那对标记刻意不同前缀；那一对随代装 rc 那一行退役了）。
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
    // 〔TL1 · 4C〕从前还有 `local-posix-source-line` 那一家（rc 里包一行 source 的第三对围栏），那一步退役、账降一行。
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
    // ② 各**家族**的标记必须两两不同（同族内共用不算）。
    let families = [remote.begin_marker, windows.begin_marker];
    for (i, a) in families.iter().enumerate() {
        for b in families.iter().skip(i + 1) {
            assert_ne!(
                a, b,
                "两个家族共用了同一对围栏 —— 装一个会把另一个整块替换掉"
            );
        }
    }
    // ③ 标记确实是那几个常量本身（指过去，不是抄一份长得一样的）。
    assert_eq!(remote.begin_marker, super::super::block::CCM_PROFILE_BEGIN);
    assert_eq!(windows.begin_marker, super::super::block::BEGIN_MARKER);
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
        vec!["fence.rs::find_pair"],
        "配对判定不再只有一份 —— 本模块存在的全部理由就是它只有一份"
    );

    let mut installs: Vec<&str> = FENCE_SHAPES.iter().map(|s| s.install_site).collect();
    installs.sort_unstable();
    installs.dedup();
    // 〔TL1 · 4C〕3 → 2：「本机 POSIX 那一行 source」那一套退役（`71 §6.1`：接上别名文件的那一行只住别名块里）。
    // 〔AL2 · 第四波 4D〕2 → 1：远端那一套并进同一个装口（`install_to_profile`，门按 `origin` 取）—— 收敛了。
    assert_eq!(
        installs.len(),
        1,
        "「装」那一半今天有 {} 处独立实现（远端 SFTP · 本机 PowerShell；`K-R62` 新加的那条路**借的是本机 \
             PowerShell 那一台安装器**，所以不是另一处；原先的第三处「本机 POSIX 那一行 source」〔TL1〕退役了）。\
             这个数变了就来改它 —— 变小 = 有人收敛了（好事，顺手降账）；变大 = 又长出一套（`KR62D1` 的失效方向）。实得：{installs:?}",
        installs.len()
    );

    // 「有装口没卸口」的那几套，逐条点得出名字 —— 〔TL1 · 4C〕这一格今天**零条**（原先唯一那条
    //   `local-posix-source-line` 随那一步退役了）。再长出一条 ⇒ 红。
    let no_uninstall: Vec<&str> = FENCE_SHAPES
        .iter()
        .filter(|s| s.uninstall_site.is_none())
        .map(|s| s.id)
        .collect();
    assert_eq!(
        no_uninstall,
        Vec::<&str>::new(),
        "「装得进去卸不掉」的那一批变了 —— 它是这张账里最贵的一格，别让它静默增减"
    );
}

/// 每一份被 [`FENCE_SHAPES`] 点到名的源文件。覆盖由下面那条判据钉死。
const SHAPE_FILES: &[(&str, &str)] = &[
    (
        "block.rs",
        include_str!("../../../../src/backend/assets/aliases/block.rs"),
    ),
    (
        "fence.rs",
        include_str!("../../../../src/backend/assets/aliases/fence.rs"),
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
/// （`sftp.rs::uninstall_remote_backend` 是设置面板上的按钮，而字段写着卸不掉）。
///
/// # 🔴 本条同时是 `K-R63 §0c-2` 要的那个**射程读数**
///
/// 件文件写着：`K-R63` 落地之后要回头看 `local-posix-source-line`
/// （唯一 `uninstall_site: None` 的一套）红没红，**没红先查射程是不是漏了它**。
/// ⇒ 本条就是那道射程：它**逐行**走 [`FENCE_SHAPES`]，那一行进了分母
/// （下面的计数自检钉死「一行都不许跳过」）。
///
/// 它今天**绿**，而绿的理由写清楚：本条判的是「**申报 ↔ 现实一致**」，
/// 〔TL1 · 4C 补〕那一行后来整行退役了（`71 §6.1`），今天账上没有 `None` 那一格 —— 下面 `None` 那一臂是给下一条长出来的留的。
/// 而那一行的申报（`None`）与现实（`account_aliases.rs` 生产段里
/// 一个 `fn uninstall… / remove… / strip… / purge…` 都没有）**是一致的** ——
/// 它是**缺实现**，不是**假申报**。要它红需要的是另一条性质
/// （「装得进去就必须卸得掉」，即 `tool_registry_tests.rs::fenced_block_implies_uninstallable`
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
        fn_names_starting_with(file_of("block.rs"), &["uninstall"])
            .contains(&"uninstall_from_profile".to_string()),
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
                    fn_names_starting_with(file_of(addr), &[sym]).contains(&sym.to_string()),
                    "`{}` 申报卸口住 {addr:?}，而那份文件的生产段里没有这个 `fn`",
                    s.id
                );
            }
            // 申报「今天没有卸口」⇒ 它家里不许躺着一个没人认领的同族实现。
            // ⚠ 动词表的分母如实写在这里：**登记过的就这四个**，不是穷举。
            None => {
                let home = s.install_site;
                let stray: Vec<String> = fn_names_starting_with(
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

// ═══════════════════════════════════════════════════════════════════════
// 〔AL1 · 2026-09-24〕`设计/71 §12.5`：**规则只有一份** —— 拼接与落盘序列的判据
// ═══════════════════════════════════════════════════════════════════════

// 〔W5-ALIAS · 第五波先行〕这里原来是落盘序列 `apply`〔散文墓碑〕的五条判据（相同不写 · 备份只给非空用户文件 ·
//   读回不对就撤 · 撤的措辞只说真发生的事 · 读不出就不写）与它的内存落点。那一个序列删了（零调用方：用户文件经后端
//   `files-put` 写，序列与这几条性质住后端 `control/files_write.rs::put_text` 与它的判据；远端 `ccm` 入口改走 `sftp::upload_verified`）。

/// 两种排版都幂等；POSIX 块外一个字节都不动，PowerShell 保住 CRLF。
#[test]
fn splicing_is_idempotent_and_keeps_every_user_line() {
    let (b, e) = ("# === x BEGIN ===", "# === x END ===");
    let block = format!("{b}\nbody\n{e}\n");
    for layout in [Layout::Posix, Layout::PowerShell] {
        for existing in ["", "user\n", "user", "a\r\nb\r\n"] {
            let once = splice_in(existing, b, e, &block, "t", layout).unwrap();
            let twice = splice_in(&once, b, e, &block, "t", layout).unwrap();
            assert_eq!(once, twice, "{layout:?} {existing:?}");
            let out = splice_out(&once, b, e, "t", layout).unwrap();
            assert!(!out.contains("body"), "{layout:?} {existing:?} → {out:?}");
            for l in existing.lines() {
                assert!(out.contains(l), "{layout:?} 丢了用户那行 {l:?}：{out:?}");
            }
        }
    }
    let crlf = splice_in("a\r\nb\r\n", b, e, &block, "t", Layout::PowerShell).unwrap();
    assert!(
        !crlf.replace("\r\n", "").contains('\n'),
        "混进了裸 LF：{crlf:?}"
    );
    // 悬空 BEGIN：两种排版都中止。
    for layout in [Layout::Posix, Layout::PowerShell] {
        assert!(splice_in(&format!("{b}\nuser\n"), b, e, &block, "t", layout).is_err());
        assert!(splice_out(&format!("{b}\nuser\n"), b, e, "t", layout).is_err());
    }
}

/// 🔴 **规则只有一个住址**：两家（别名文件 `account_aliases.rs` · 别名块 `profile_installer.rs`，〔W5-ALIAS〕远端 rc 那一份
/// 也搬进了后者）的生产段里，「配对 ＋ 读回比对」的原语**零命中**。
/// 〔W5-ALIAS · 第五波先行〕正控换了：从前是本模块 `apply`〔散文墓碑〕里恰好一处 `verify_readback(`；那个序列删了
/// （零调用方），「备份 · 原子替换 · 回读 · 回滚」今天只住后端 `control/files_write.rs::put_text` —— 正控钉它在、且恰好一处定义；
/// 本模块生产段里**没有**任何异步落盘序列（`async fn` 零处）。别名块「写口只有一个」那条住
/// `block_tests.rs::the_alias_block_is_written_through_exactly_one_door`。
///
/// 死值验：往 `account_aliases.rs` 的写别名文件那一跳里放回一行
/// `verify_and_rollback(`（一个回滚写入器的调用形）⇒ 本条红在第一个断言。
/// 〔TL1 · 4C〕原句点的是代装 rc 那一行的那一跳（`ensure_rc_source_line`〔散文墓碑〕），那一跳退役了。
#[test]
fn the_write_rule_has_exactly_one_home() {
    let root = crate::guard_support::repo_root().join("src/backend/assets/aliases");
    let read = |f: &str| {
        let raw = std::fs::read_to_string(root.join(f)).unwrap();
        let prod = guard_core::production_code(&raw);
        guard_core::assert_no_test_code(f, &prod);
        prod
    };
    let needles = [
        "verify_and_rollback(",
        "verify_readback(",
        "find_pair(",
        "split_inclusive(",
    ];
    let mut hits: Vec<String> = Vec::new();
    for f in ["mod.rs", "block.rs"] {
        let prod = read(f);
        for n in needles {
            if prod.contains(n) {
                hits.push(format!("{f}: {n}"));
            }
        }
    }
    assert!(hits.is_empty(), "规则长出了第二个住址：{hits:?}");
    // 本模块只剩纯规划：一个异步落盘序列都不许长回来。
    let home = read("fence.rs");
    assert!(
        !guard_core::contains_word(&home, "put(") && !guard_core::contains_word(&home, "door"),
        "`fence.rs` 生产段里够到了写口 —— 落盘序列长到规划这一层了（它只住 `files-put`）"
    );
    // 正控：序列的那一个住址真的在（后端），而且恰好一处定义。
    let backend = guard_core::production_code(
        &std::fs::read_to_string(
            crate::guard_support::repo_root().join("src/backend/control/files_write.rs"),
        )
        .unwrap(),
    );
    guard_core::find_pinned(&backend, "pub fn put_text(")
        .expect("后端那一个写序列 `files_write.rs::put_text` 不见了（或长成了两处）");
}
