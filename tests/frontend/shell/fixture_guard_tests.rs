use super::*;

/// 仓根。**只跳一级**（`src/frontend/shell` 的上级就是仓根，那是 cargo 给的事实）。
fn fg_repo_root() -> std::path::PathBuf {
    crate::guard_support::repo_root().to_path_buf()
}

/// **活引用语料面**：`src` ＋ `tests` 两棵互不包含的根，按
/// [`counts_as_live_reference_site`] 筛过之后剩下的 `(仓根相对路径, 原文)`。
///
/// # 🔴 为什么要**明写摘掉本条自己那两份文件**
///
/// 本模块与它的判词里都会写出夹具的基名（头注里那句「现打 10 处活引用全是 `.ts`」
/// 就点了那份夹具的名字），而下面 [`every_fixture_has_a_live_reference`] 的
/// 豁免登记表里也会写夹具路径。不摘的话：
///
/// | 不摘的后果 | 症状 |
/// |---|---|
/// | 头注点了某份夹具的名字 | 那份夹具**永远有活引用** ⇒ 它成不了孤儿 ⇒ 本条对它恒绿 |
/// | 豁免表里登记一份孤儿 | 登记这个动作**本身**把它变成「有引用」⇒ 它掉出孤儿集 ⇒ 保鲜自检当场红 ⇒ **豁免永远登不进来** |
///
/// 第二行尤其要紧：它会让「登记豁免」这条出路**结构上不可用**，
/// 于是下一个人只剩「把判据改松」这一条路 —— 那是本仓最怕的结局。
///
/// ⇒ 走 [`guard_core::scan_tree_excluding`] 的**明写名单**：它**摘不到就 panic**，
/// 所以这两份文件改名 / 搬家会当场出声，而不是安静地回到人群里。
/// （`file!()` 那条自摘的路在本仓**不生效** —— 那个原语的头注整段在讲这件事。）
fn live_reference_corpus() -> Vec<(String, String)> {
    let root = fg_repo_root();
    let mut out: Vec<(String, String)> = Vec::new();
    for (sub, excluded) in [
        ("src", &["shell/src/fixture_guard.rs"] as &[&str]),
        ("tests", &["shell/fixture_guard_tests.rs"]),
    ] {
        for (p, src) in guard_core::scan_tree_excluding(&root.join(sub), &[] as &[&str], excluded) {
            let rel = p
                .strip_prefix(&root)
                .unwrap_or(&p)
                .to_string_lossy()
                .replace('\\', "/");
            if !counts_as_live_reference_site(&rel) {
                continue;
            }
            out.push((rel, src));
        }
    }
    out
}

/// ★★ **`tests/__fixtures__/` 里每一份夹具都得有人在用。**
///
/// 头注（病 · 死值验读数 · 买到什么 · 买不到什么 · 三条排除各自的理由）住生产侧模块
/// `fixture_guard.rs`。**这里不抄第二份。**
#[test]
fn every_fixture_has_a_live_reference() {
    /// **豁免登记**：`(仓根相对路径, 为什么它掉光了活引用还能留在盘上)`。
    ///
    /// 🔴 **今天是空的，而「空」在这里是承重的**：它逐字的意思是
    /// **盘上没有一份孤儿夹具**，不是「这一格没人看」。
    ///
    /// ⚠ 表名叫 `REGISTERED` 不是随手起的：`scanning_guard_registry` 那条纪律逐字
    /// 「新写一条『扫描面 ＋ 常量表』型的判据，那张表要起成 `TABLE_DECLS` 里已有的
    /// 名字之一」—— 起了别的名字，`every_registry_guard_keeps_its_reverse_half`
    /// **看不见本条**，而「它看过了、过了」与「它压根没去看」在输出上一模一样。
    ///
    /// ⚠ 往里加一行之前先问两遍：
    /// ① **它为什么还在盘上？** 被测面退役了就把夹具一起删掉 —— `α3` 那一刀的账
    ///    正是「清单腐了 ⇒ 该删的没删」，留着它就是把同一个坑再挖一次。
    /// ② **它的正文干净吗？** 本仓那条「测试夹具采结构不采内容」的纪律**今天没有机检**
    ///    （见模块头注），而 09-21 删掉的那一份正文里有真金额。
    const REGISTERED: &[(&str, &str)] = &[];

    let root = fg_repo_root();
    let fixtures = files_under(&root.join(FIXTURE_DIR), &root);

    // ★ 抽取器自检 1：人群空集 ⇒ 下面那条相等在「两边都空」上成立。
    //   它挡的是「目录路径写错 / 目录被搬走」这一形（那一形按构造只会让人群变空）。
    assert!(
        !fixtures.is_empty(),
        "`{FIXTURE_DIR}` 下一份文件都没枚举到 —— 目录搬走了 / 路径写错了 / 遍历器坏了。\n\
         这一格非红不可：人群空集时下面那条相等**零命中地成立**，\n\
         而它宣布的结论恰好是「没有孤儿夹具」。"
    );

    let corpus = live_reference_corpus();
    // ★ 抽取器自检 2：语料面塌了 ⇒ 每一份夹具都会被判成孤儿（**安全方向**，会红），
    //   但失败文案会指错修法。所以这一格单独放在前面，让它自己说话。
    assert!(
        corpus.len() >= 900,
        "活引用语料面只收到 {} 份文件 —— 它坏了（2026-09-22 现打 996 份：\n\
         `src` ＋ `tests` 两棵根下的 1413 份文本文件，减掉三条排除与本条自己那两份）。\n\
         下面那条相等此刻会把每一份夹具都报成孤儿，而那是量具坏了，不是盘面坏了。",
        corpus.len()
    );

    // ★ 抽取器自检 3（**阴性对照，跑在真语料上**）：谓词得会说「不」。
    //   `is_named_in` 若被改成恒真，上面两条地板都照样过，而孤儿集会恒空 ⇒ 恒绿。
    //   ⇒ 拿一个**按构造不可能存在**的基名去问真语料，必须一处都不命中。
    let impossible = format!("zz{u}no{u}such{u}fixture{u}anywhere.bin", u = "-");
    assert!(
        !corpus.iter().any(|(_, src)| is_named_in(&impossible, src)),
        "一个按构造不存在的基名在真语料里命中了 —— `is_named_in` 恒真，\n\
         那么下面的孤儿集恒空、本条恒绿。"
    );

    let orphans: Vec<String> = fixtures
        .iter()
        .filter(|rel| {
            let base = rel.rsplit('/').next().unwrap_or(rel);
            !corpus.iter().any(|(_, src)| is_named_in(base, src))
        })
        .cloned()
        .collect();

    let want: std::collections::BTreeSet<String> =
        REGISTERED.iter().map(|(p, _)| (*p).to_string()).collect();
    assert_eq!(
        want.len(),
        REGISTERED.len(),
        "`REGISTERED` 里有重复路径 —— 后一行会静默吃掉前一行"
    );
    let have: std::collections::BTreeSet<String> = orphans.iter().cloned().collect();

    // ① **没登记的孤儿** —— 正题那一半。
    let newly: Vec<&String> = have.difference(&want).collect();
    assert!(
        newly.is_empty(),
        "这几份夹具在活树里**一处引用都没有**：\n  {}\n\n\
         「活树」＝ `src` ＋ `tests`，减掉夹具目录自己 · `tests/evidence/`（量具与记录）\n\
         · 纯散文（`.md` / `.txt`）—— 三条排除各自的理由在 `counts_as_live_reference_site` 头注里。\n\
         \n\
         ⇒ 两条出路，按优先级：\n\
         ① **把夹具删掉** —— 它的被测面多半已经退役了。这正是 `α3` 那一刀该做而\n\
            因为清单腐了没做成的事（按旧住址删 ⇒ 删了个不存在的文件，真文件静默留在盘上，\n\
            而它的正文里有真金额）；\n\
         ② **接上引用** —— 真有判据要用它就把它用起来。\n\
         ⚠ **第三条路（往 `REGISTERED` 里加一行）不是省事的出口**：\n\
            那张表要求同行写清「它为什么还在盘上」，而这个问题多半没有好答案。\n\
         ⚠ 只被 `tests/evidence/` 点着**不算活的**，这一条是承重的：\n\
            09-21 那份孤儿夹具就是只被那一棵点着（3 处），把它算进来本条对它就是瞎的。",
        newly
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join("\n  ")
    );

    // ② **反向那半 / 保鲜自检**：登记了却不再是孤儿（或者盘上没有了）⇒ 把那行删掉。
    //   没有这一半，那张表会替真缺陷挡枪。
    let stale: Vec<&String> = want.difference(&have).collect();
    assert!(
        stale.is_empty(),
        "这几条豁免登记盘上已经不成立了 —— **把它们从 `REGISTERED` 里删掉**\n  {}\n\n\
         （两种情形：那份夹具被删了，或者有人给它接上了活引用 —— 两样都是好事。）\n\
         留着一条用不上的豁免，等于给下一份真孤儿预留一个免检章。",
        stale
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join("\n  ")
    );
}

/// ★★ **活树里每一处夹具地址都得真有那份文件。**〔方向乙，甲的镜像〕
///
/// 它收的是另一半：**代码里点着一份盘上没有的夹具**。
/// 顺带给本族一个**非空的第二人群** —— 方向甲那张豁免表今天是空的，
/// 而一条只有空表的判据说不出「反向那半还在不在」。
#[test]
fn every_fixture_address_in_the_live_tree_still_exists() {
    /// **悬空登记**：`(仓根相对路径, 它点的基名, 为什么这条悬空地址刻意留着)`。
    ///
    /// 🔴 今天是空的。⚠ 这不等于全仓没有悬空地址：`tests/evidence/` 里现打有
    /// **2 处**指着 09-21 删掉那份夹具（一份剪切脚本 ＋ 一份死值验记录）——
    /// 那两处是**历史记录**，地址本该是历史，按构造不在本条人群里
    /// （`tests/evidence/` 不算活树，理由在 `counts_as_live_reference_site` 头注）。
    /// **这一句是射程说明，不是欠账。**
    const SITES: &[(&str, &str, &str)] = &[];

    let root = fg_repo_root();
    let on_disk: std::collections::BTreeSet<String> = files_under(&root.join(FIXTURE_DIR), &root)
        .iter()
        .map(|rel| rel.rsplit('/').next().unwrap_or(rel).to_string())
        .collect();
    let corpus = live_reference_corpus();

    let mut addressed: Vec<(String, String)> = Vec::new();
    for (rel, src) in &corpus {
        for base in fixture_addresses(src) {
            addressed.push((rel.clone(), base));
        }
    }
    addressed.sort();
    addressed.dedup();

    // ★ 抽取器自检：人群塌了 ⇒ 下面那条相等在空集上成立。
    //   现打 2026-09-22：活树里 **5 份文件 / 5 条**（去重后；同一份文件点同一个基名算一条）。
    assert!(
        addressed.len() >= 4,
        "活树里只抽到 {} 条夹具地址 —— 抽取器坏了（2026-09-22 现打 5 条 / 5 份文件）。\n\
         针是从 `fixture_dir_leaf()` 运行期拆的，拆坏了这一格就整条空转。",
        addressed.len()
    );

    let want: std::collections::BTreeSet<(String, String)> = SITES
        .iter()
        .map(|(p, b, _)| ((*p).to_string(), (*b).to_string()))
        .collect();
    let have: std::collections::BTreeSet<(String, String)> = addressed
        .iter()
        .filter(|(_, base)| !on_disk.contains(base))
        .cloned()
        .collect();

    let newly: Vec<String> = have
        .difference(&want)
        .map(|(p, b)| format!("  {p}  点着 `{b}`，而 `{FIXTURE_DIR}` 下没有这份文件"))
        .collect();
    assert!(
        newly.is_empty(),
        "活树里这几处夹具地址**悬空**了：\n{}\n\n\
         ⇒ 两条出路：把那份夹具补回来，或者把点它的那一行改对 / 删掉。\n\
         ⚠ 这一形正是 `α3` 那一刀的病根的镜像：那一次是**清单**里的住址腐了\n\
         （写 `src/__fixtures__/`、真住址是 `tests/__fixtures__/`）⇒ 按旧住址删，\n\
         删了个不存在的文件，而真文件静默留在盘上。地址腐掉不会自己出声。",
        newly.join("\n")
    );

    let stale: Vec<String> = want
        .difference(&have)
        .map(|(p, b)| format!("  {p}  `{b}`"))
        .collect();
    assert_eq!(
        stale,
        Vec::<String>::new(),
        "这几条悬空登记盘上已经不成立了 —— 把它们从 `SITES` 里删掉（多半是有人把地址改对了）"
    );
}

/// ★ 上面两条的**活体夹具** —— 它们在真树上今天都是绿的，而「今天该是空集」
/// 那种格是**空真**：抽取器死了照样是空集。
/// ⇒ 这里把四个抽取器逐格切开，**正反各喂一遍**，并把
/// `files_under` 头注里那条「为什么不能走共享原语」钉成机检。
#[test]
fn the_fixture_scanner_can_tell_an_orphan_from_none() {
    let root = fg_repo_root();

    // ① 🔴 **为什么非要自带遍历器**：同一个真目录，本函数看得见二进制，
    //    共享原语（只收文本语料）看不见 ⇒ 两个份数**必须不相等**。
    //    哪天原语补了字节档，这一格会红，那时 `files_under` 就该退役。
    let icons = root.join("src/frontend/shell/icons");
    let by_path = files_under(&icons, &root);
    let by_text = guard_core::scan_tree_excluding(&icons, &[] as &[&str], &[]);
    assert!(
        by_path.len() > by_text.len(),
        "路径遍历（{}）没比文本语料遍历（{}）多 —— 要么那个目录里已经没有二进制文件了，\n\
         要么共享原语补了字节档。两种情形都要回去读 `files_under` 头注：\n\
         它存在的**唯一**理由就是「二进制孤儿夹具不许按构造看不见」。",
        by_path.len(),
        by_text.len()
    );

    // ② 遍历真的递归（夹具目录哪天长出子目录，里面的孤儿也得进人群）。
    let nested = files_under(&root.join("tests/frontend/shell"), &root);
    assert!(
        nested.iter().any(|p| p.matches('/').count() >= 3),
        "遍历没递归进子目录 —— 夹具目录一旦分层，里面的孤儿就整批掉出人群"
    );

    // ③ 「活引用来源」的三条排除，逐条正反各一。
    assert!(counts_as_live_reference_site(
        "tests/frontend/ui/scale2-height-corpus.ts"
    ));
    assert!(counts_as_live_reference_site(
        "src/frontend/shell/src/lib.rs"
    ));
    assert!(
        !counts_as_live_reference_site(&format!("{FIXTURE_DIR}/whatever.jsonl")),
        "夹具目录自己被算成了活引用来源 —— 夹具互相点名会让整族恒绿"
    );
    assert!(
        !counts_as_live_reference_site("tests/evidence/CP-copy-judges.py"),
        "`tests/evidence/` 被算成了活引用来源 —— 09-21 那份孤儿夹具就是只被它点着的"
    );
    for e in PROSE_EXTENSIONS {
        assert!(
            !counts_as_live_reference_site(&format!("tests/evidence-like/a{e}")),
            "纯散文 `{e}` 被算成了活引用来源 —— 一句话读不了一份夹具"
        );
    }

    // ④ `is_named_in`：命中与不命中都要成立（恒真 / 恒假都会让上面两条瞎掉）。
    let base = format!("zz{u}probe{u}corpus.jsonl", u = "-");
    assert!(is_named_in(&base, &format!("const F = \"__x__/{base}\";")));
    assert!(!is_named_in(&base, "const F = \"__x__/other.jsonl\";"));

    // ⑤ `fixture_addresses`：三形都收得到，而「指目录」那一形**不收**。
    let leaf = fixture_dir_leaf();
    let one = format!("resolve(__dirname, \"{leaf}/{base}\")");
    assert_eq!(fixture_addresses(&one), vec![base.clone()], "紧挨形没收到");
    let two = format!("import x from \"../{leaf}/{base}?raw\";");
    assert_eq!(fixture_addresses(&two), vec![base.clone()], "相对形没收到");
    let three = format!("语料住 `{FIXTURE_DIR}/{base}`（69 条）");
    assert_eq!(
        fixture_addresses(&three),
        vec![base.clone()],
        "写全形没收到"
    );
    let dir_only = format!("mkdirSync(resolve(ROOT, \"{FIXTURE_DIR}\"))");
    assert_eq!(
        fixture_addresses(&dir_only),
        Vec::<String>::new(),
        "「指这个目录」被误采成了一处夹具地址 —— 那一形一份文件都指不出来"
    );
    let slash_only = format!("re = r\"/{leaf}/\"");
    assert_eq!(
        fixture_addresses(&slash_only),
        Vec::<String>::new(),
        "空 token 被收进来了 —— 真树上会多出 4 处假悬空"
    );
}
