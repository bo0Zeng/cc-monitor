use super::REQUIRED_FACTS;
use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    // 住址唯一源：`crate::guard_support`（头注写着 24 份副本怎么一起漂的）。
    crate::guard_support::repo_root()
}

fn arch_doc() -> String {
    let p = repo_root().join("src/doc/ARCHITECTURE.md");
    assert!(
        p.is_file(),
        "读不到 {} —— 读不到的文件只会静默返回空串，那会让本模块零命中地绿",
        p.display()
    );
    std::fs::read_to_string(&p).expect("读 ARCHITECTURE.md")
}

/// 按 `## N.` 标题切段；返回**剥掉指定那一节之后**的全文。
///
/// 用于反向断言：承载事实的那一节被剥掉，存在钉必须失守。
fn without_section(src: &str, head_prefix: &str) -> String {
    let lines: Vec<&str> = src.lines().collect();
    let a = lines
        .iter()
        .position(|l| l.starts_with(head_prefix))
        .unwrap_or_else(|| panic!("找不到以 {head_prefix:?} 开头的小节 —— 标题措辞变了"));
    let b = lines[a + 1..]
        .iter()
        .position(|l| l.starts_with("## "))
        .map(|k| a + 1 + k)
        .unwrap_or(lines.len());
    let mut kept: Vec<&str> = Vec::new();
    kept.extend_from_slice(&lines[..a]);
    kept.extend_from_slice(&lines[b..]);
    kept.join("\n")
}

/// 某份文本里**没被覆盖**的事实清单：`(事实, 缺哪个探针)`。
fn uncovered(src: &str) -> Vec<(&'static str, &'static str)> {
    let mut out = Vec::new();
    for (fact, probes, _) in REQUIRED_FACTS {
        for probe in probes.iter() {
            if !src.contains(probe) {
                out.push((*fact, *probe));
                break;
            }
        }
    }
    out
}

/// ★★ 存在钉：顶层文档仍然覆盖每一条结构性事实。
#[test]
fn the_top_level_doc_still_carries_every_structural_fact() {
    let missing = uncovered(&arch_doc());
    assert!(
        missing.is_empty(),
        "`src/doc/ARCHITECTURE.md` 丢了结构性事实：{missing:?}\n\
             ⚠ **别改这张表来求绿** —— 表里每条都写着「缺了会怎样」，\n\
             那几句是从重写前那份文档的实测病灶来的（`backend` 1 次 / 「轮询」0 次）。\n\
             要么把事实写回文档，要么先证明那条事实已经不成立了。"
    );
}

/// ★★ **反向断言：把承载事实的那一节剥掉，上面那条必须失守。**
///
/// ⚠ **订正（F19 实施中被自己的变异逮到）**：本条头注原写「没有这一条，
/// 存在钉就可能是恒真的」—— **那句是误导**。M3 变异（把四条事实的探针全换成泛词
/// `backend` / `事件` / `crate`）时**本条照绿**，因为那些泛词**也**主要长在那一节里。
///
/// ⇒ 本条真正测的是「**事实长没长在该长的那一节**」（剥了节就失守），
/// **不测「探针够不够有辨别力」** —— 后者由
/// `no_fact_may_rest_on_a_probe_that_is_everywhere` 管。两件事，别混说。
#[test]
fn the_facts_really_live_in_the_sections_that_are_supposed_to_carry_them() {
    let src = arch_doc();
    assert!(uncovered(&src).is_empty(), "基线就没绿，先修上一条");

    // 层边界那一节承载「backend 边界 / 零轮询 / 搬不动的边界 / 不进 workspace」四条。
    let no_layers = without_section(&src, "## 2. ");
    let lost = uncovered(&no_layers);
    assert!(
        lost.len() >= 3,
        "剥掉「层边界」那一节之后，仍有 {} 条事实被判成「覆盖了」（只丢了 {lost:?}）——\n\
             说明探针命中的是别处的零碎词句，而不是那一节。这条判据是**恒真的**，重挑探针。",
        REQUIRED_FACTS.len() - lost.len()
    );

    // 数据流那一节承载「远端那条链」。
    let no_flow = without_section(&src, "## 1. ");
    assert!(
        uncovered(&no_flow)
            .iter()
            .any(|(f, _)| f.contains("远端那条数据流链")),
        "剥掉「数据流」那一节之后，「远端那条链」仍被判成覆盖了 —— \
             那条链的探针没长在它该长的地方"
    );
}

/// ★★ **探针必须有辨别力** —— M3 变异（探针换泛词）从这条判据的缺口里活着走了出去。
///
/// # 三条量法，都是量出来的
///
/// 实测今天全部探针在 `ARCHITECTURE.md` 里的频次：**每条事实都至少有一个「只出现 1 次」
/// 的锚**（`backend = 读` / `零轮询` / `ssh_source.rs` / `exec` / `原生构建` 各 1 次），
/// 而 M3 用的泛词是 `backend` **12** 次 · `事件` **8** 次 · `crate` **6** 次。
/// 最高的现役探针是 `control/` **15** 次 —— 它靠同组的稀有锚兜着，本身不承重。
///
/// ⇒ ① 每条事实**至少 3 个探针**（单探针的事实经不起一个措辞改动）；
/// ② 每条至少有**一个频次 ≤ 2 的稀有锚**；③ 没有探针频次 > 20。
#[test]
fn no_fact_may_rest_on_a_probe_that_is_everywhere() {
    let src = arch_doc();
    let times = |probe: &str| src.matches(probe).count();
    for (fact, probes, why) in REQUIRED_FACTS {
        assert!(
            probes.len() >= 3,
            "事实「{fact}」只有 {} 个探针 —— 单探针撑不住一条结构性主张，\
                 一次措辞改动就能让它零命中地绿。它的份量见「缺了会怎样」：{why}",
            probes.len()
        );
        let rare = probes.iter().filter(|p| times(p) <= 2).count();
        assert!(
            rare >= 1,
            "事实「{fact}」的探针全是常见词（频次 {:?}）—— 没有稀有锚就是**恒真断言**：\
                 换掉整节内容它照样绿。实测每条现役事实都有一个频次 1 的锚。",
            probes.iter().map(|p| (*p, times(p))).collect::<Vec<_>>()
        );
        for p in probes.iter() {
            let n = times(p);
            assert!(
                n <= 20,
                "事实「{fact}」的探针 {p:?} 在文档里出现 {n} 次 —— 太常见，\
                     它承不了「这条事实讲过了」这个判断（今天最高的现役探针是 15 次）"
            );
        }
    }
}

/// ★ 形状钉之二：**State 注册表的摘要不许长回来。**
///
/// 〔F19〕原 §3 有一张 7 行的 State 表，而 `src/doc/STATE-MATRIX.md` 里有**严格更全的同一张**
/// （多三列 + 逐命令 consumer），且原文自己就写着「详细矩阵 → STATE-MATRIX.md」——
/// **自己承认家在那边、又存了一份摘要**。⇒ 摘要删除、只留指针，本条钉住它不再长回来。
///
/// ⚠ 钉的是**表的形状**（`Arc<X>` 行）而不是「不许提 State」：
/// §3 现在仍然要讲「为什么漏一次 `app.manage()` 编译器抓不住」——那是架构性的。
/// 实测：删表后 `Arc<` **0 行**，`manage` 只剩那句理由里的 **1 处**。
#[test]
fn the_state_matrix_summary_never_grows_back() {
    let src = arch_doc();
    let rows: Vec<&str> = src
        .lines()
        .filter(|l| l.starts_with('|') && l.contains("Arc<"))
        .collect();
    assert!(
        rows.is_empty(),
        "`src/doc/ARCHITECTURE.md` 里又出现了 {} 行 State 表（{rows:?}）——\
             那张表的唯一的家是 `src/doc/STATE-MATRIX.md`，这里只该有指针。",
        rows.len()
    );
    assert!(
        src.contains("STATE-MATRIX.md"),
        "指针也没了 —— 那比存一份副本更糟：撤 State 的人根本找不到那份强制 checklist"
    );
}

/// 行里反引号包住的「代码单元」token（路径 / 模块名 / 文件名）。
fn code_units(line: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut rest = line;
    while let Some(i) = rest.find('`') {
        let after = &rest[i + 1..];
        let Some(j) = after.find('`') else { break };
        let tok = &after[..j];
        rest = &after[j + 1..];
        let shaped = tok.len() > 3
            && tok
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '/' | '-' | '.'))
            && (tok.contains('/')
                || tok.contains('_')
                || tok.ends_with(".rs")
                || tok.ends_with(".ts"));
        if shaped {
            out.push(tok);
        }
    }
    out
}

/// ★ 形状钉之二：**换成「模块表」也不许长回来**〔audit-0805 08-06〕。
///
/// # 它补的洞
///
/// 隔壁那条按 `.rs` / `.ts` **后缀**数文件名提及。实测：往 `ARCHITECTURE.md` 追加一张
/// **30 行的「模块 | 职责」表**（`backend/control` / `observe/watcher` / `common/fs` …），
/// 文件名提及**一处没涨**、也没有 `├── ` 树枝 ⇒ **五条判据全绿**。
///
/// 而模块表与文件表是**同一个病**：本模块头注自己写着
/// 「一份顶层文档不该靠列文件来解释自己」—— 去掉后缀并不改变那句话。
/// ⇒ 判据锚在**怎么写**（后缀）上，而不是锚在**做了什么**（用表格枚举代码单元）上。
///
/// # 天花板量出来的
///
/// 今天 **12 行**（`backend/` 四行子目录表 · 两行守卫表 · 六行文件归属表 —— 都是正当的）。
/// 天花板 **25**：给正常增长两倍余量，而一张三十行的清单塞不进去。
/// 地板 **6**：抽取器坏掉时返回一个小得离谱的数，本条必须红而不是零命中地绿。
#[test]
fn the_top_level_doc_never_turns_into_a_table_of_code_units() {
    // 抽取器自检：喂人造行，两个方向都要对。
    assert_eq!(
        code_units("| `backend/control` | 控制面 |").len(),
        1,
        "表格行里的模块名没被认出来"
    );
    assert!(
        code_units("| 状态 | 说明 |").is_empty(),
        "没有代码单元的表格行被误认"
    );
    assert!(
        code_units("正文里提到 `observe/watcher` 不算表格行").len() == 1,
        "抽 token 这一步本身要认得它（是否算数由调用方按「是不是表格行」决定）"
    );

    let src = arch_doc();
    let rows: Vec<&str> = src
        .lines()
        .filter(|l| l.trim_start().starts_with('|') && !code_units(l).is_empty())
        .collect();
    assert!(
        rows.len() >= 6,
        "只数出 {} 行带代码单元的表格行（08-06 实测 12）—— 抽取器多半坏了，\
             本条会零命中地绿",
        rows.len()
    );
    assert!(
        rows.len() <= 25,
        "`src/doc/ARCHITECTURE.md` 里「用表格枚举代码单元」的行涨到 {} 行（天花板 25，实测 12）——\n\
             逐**模块**清单正在长回来。它与逐**文件**清单是同一个病：\n\
             顶层文档不该靠列代码单元来解释自己，那些清单的家是各目录 README 与 `BACKEND_FILES`。\n\
             ⚠ 别把这条的天花板调上去 —— 隔壁那条按后缀数的判据看不见模块表，\n\
             把它调松等于这一整类清单重新无人看守。",
        rows.len()
    );
}

/// ★ 形状钉：**逐文件模块表不许长回来。**
///
/// # 天花板是量出来的，不是猜的
///
/// 实测：重写**前**全文 `.rs`/`.ts` 文件名提及 **117 处**（其中那张 122 行的
/// 模块表独占 **82 处**、含 23 行 `├── ` 树枝）；重写**后 49 处**（§2 内只剩 13 处）。
/// ⇒ 天花板 **70**：既给正常增长留了空档，又容不下一张重新长出来的清单。
///
/// ⚠ 这里**不禁**文件名 —— §5「关键设计选择」必须指名道姓才讲得清「为什么不能用别的方案」。
/// 禁的是**规模**：一份顶层文档不该靠列文件来解释自己。
#[test]
fn the_per_file_module_tree_never_grows_back() {
    let src = arch_doc();
    let mut mentions = 0usize;
    for tok in
        src.split(|c: char| !(c.is_ascii_alphanumeric() || matches!(c, '_' | '/' | '-' | '.')))
    {
        if tok.len() > 3 && (tok.ends_with(".rs") || tok.ends_with(".ts")) {
            mentions += 1;
        }
    }
    assert!(
        mentions <= 70,
        "`src/doc/ARCHITECTURE.md` 里的文件名提及涨到 {mentions} 处（天花板 70，重写后实测 49）——\n\
             逐文件清单正在长回来。它的家是 `src/bridge/README.md` / `src/README.md` / \n\
             `backend/mod.rs` 的 `BACKEND_FILES`，顶层文档只该指过去。"
    );
    assert!(
        mentions >= 20,
        "只数出 {mentions} 处文件名 —— 抽取器多半坏了（重写后实测 49 处）。\
             返回一个小得离谱的数时，本条会零命中地绿，所以它必须红。"
    );
    let branches = src.matches("\u{251c}\u{2500}\u{2500} ").count();
    assert_eq!(
        branches, 0,
        "出现了 {branches} 行目录树树枝 —— 重写前那张模块表有 23 行。\
             §1 的 ASCII 数据流图不用这种树枝，所以这条不会误伤它。"
    );
}

/// ★★ **P3t-Y4 的机检半**：引一段文字证明不了「今天的代码就是那个意思」，
/// 所以裁定必须配一条**机器能重跑**的检查（DoD 逐字：「那句注释不许再写成泛指的『本机』」）。
///
/// # 裁的是什么
///
/// `src/doc/INVARIANTS.md` §36 的标题后半句就是它的全部内容 ——
/// 「嵌套 env 污染保护已在进程启动期做完，**别在本地渲染器里重复实现**」，
/// 铁律那段逐字禁的是「给本地渲染器补一段读 `plan.env`、把 `unset` 翻成 PowerShell
/// `Remove-Item Env:\X` 的代码」；整节的论证（`config_dir_prefix_ps` /
/// `validate_config_dir_ps` / 「`\` 与盘符」）全是 **Windows**。
/// ⇒ 它**不是**「本机不许用 CLI 渲染器」。把它当成那个用，是**把一条窄铁律读宽了**。
///
/// # 钉法
///
/// 人群 = 全树里每一个引 §36 的**注释块 / 字面量行**（自动派生，不是白名单）。
/// 规则：一个块里同时出现 §36 与「本机 / 本地」时，它就是在用 §36 划范围
/// ⇒ **必须同时出现 `Windows`**，把范围写准。
///
/// # 它守什么、不守什么
///
/// **守**：下一个人（包括我）再拿 §36 当「一律拒本机」的依据时当场红。
/// **不守**：① 一句**不引 §36** 却照样写「本机不走 CLI 渲染器」的注释 ——
///   本条按引用取样，够不着它（这是本条的射程边界，写在这里而不是假装没有）；
/// ② 「块里有 Windows」不等于「那句话说对了」—— 机检管得住范围词在不在，
///   管不住论证对不对，那一半靠上面的裁定文字与评审。
#[test]
fn every_citation_of_invariant_36_says_which_platform_it_binds() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    // ⚠ **不剥 `#[cfg(test)]`**（第一版剥了，人群当场变 0 —— 完备性自检逮住的）。
    // `parity_ledger` 那张平价账本整个住在测试段里：它是**登记表**，不是运行时代码。
    // 拿「生产段」当人群 = 把本条最主要的目标全摘掉。
    //
    // 🔴 〔步 7c 剖分 2026-09-19 · `设计/16 §6.2` B 类 ＋ `§5.4b` 纪律 3、4〕
    //    **上面那句话正是这一轮失效的那一句 —— 两件一起改。**
    //
    // · **B 类**：那张账本这一轮从 `src/bridge/src/parity_ledger.rs` 搬进了
    //   `tests/bridge/parity_ledger_tests.rs` ⇒ 只给 `<bridge>/src` 一棵根时它**掉出人群**。
    //   现打：完备性自检之后那条点名断言当场红（实得三份，`parity_ledger_tests.rs` 不在里面）。
    //   ⇒ 补上 `<repo>/tests/bridge` 这一棵。两棵互不包含（`§5.4b` 纪律 1）。
    // · **A 类**：摘除原来靠 `scan_tree_excluding_self(.., file!())`。本文件被 `#[path]`
    //   引进来 ⇒ `file!()` 是折返路径 ⇒ 摘除恒空转；而语料现在**含本文件自己那棵树**
    //   ⇒ 空转不再无害（本文件头注里 §36 出现十几次，会把自己喂饱）。
    //   ⇒ 改成 `scan_tree_excluding` 的明写名单，摘不到就 panic。
    //
    // ⚠ **射程边界如实写**：`src/backend` / `tests/backend` 这一轮**没有**加进来 ——
    //   它们在本条立条时就不在射程里，加它们是扩面（另一件事），不是这一轮的搬树补齐。
    let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("从 src/bridge 上去两级该是仓根")
        .to_path_buf();
    let mut files_src = guard_core::scan_tree_excluding(&root, &["rs"], &[]);
    files_src.extend(guard_core::scan_tree_excluding(
        &repo.join("tests/bridge"),
        &["rs"],
        &["arch_doc_shape_guard_tests.rs"],
    ));
    let mut blocks: Vec<(String, String)> = Vec::new(); // (文件, 块)
    for (path, src) in &files_src {
        let name = path
            .strip_prefix(&root)
            .or_else(|_| path.strip_prefix(repo.join("tests/bridge")))
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");
        // ★★ **切到句，不是切到块**（08-11 变异逼出来的第二版）。
        //
        // 第一版把连续注释行并成一整块。实测：把 `launch_wire` 那句读宽了的引用
        // **原样写回去**，本条**照样绿** —— 因为那句坏话与紧跟其后的订正文字同块，
        // 而订正文字里有 `Windows`。⇒ 那一版对它自称要抓的那次回归**真阳率为零**。
        //
        // 现在的单位是「一行里被句号切开的一段」：`§36` 与 `Windows` 必须落在**同一段**。
        for line in src.lines() {
            for seg in line.split('\u{3002}') {
                blocks.push((name.clone(), seg.to_string()));
            }
        }
    }

    // ★★ **钉切法本身**（变异 M3 逼出来的）。
    //
    // 实测：把切法退回「连续注释行并成一块」，再把那句读宽了的引用原样写回去 ——
    // 本条**照样绿**。也就是说粒度一退，判据就**静默失去牙齿**，而失去牙齿的样子
    // 与守得好好的样子在输出上完全一样。⇒ 粒度不是实现细节，是本条的射程本身，
    // 必须自己钉住：切出来的单位里不许再有句号。
    assert!(
        blocks.iter().all(|(_, b)| !b.contains('\u{3002}')),
        "切出来的单位里还有句号 —— 粒度退回块了。\n\
             那样一句坏话只要与任何一句提到 Windows 的订正文字同块就能蒙混过关，\n\
             本条会像第一版那样对 `launch_wire` 那次回归**零真阳**。"
    );

    let citing: Vec<&(String, String)> = blocks.iter().filter(|(_, b)| b.contains("§36")).collect();

    // ★ 完备性自检（`ENTRIES` 那条的教训：人群为空时「全过」与「没测」长得一模一样）。
    assert!(
        citing.len() >= 4,
        "全树只找到 {} 处引 §36 —— 少于开工时的 4 处。\n\
             要么切法把人群切没了（第一版剥生产段就剥成了 0），要么引用被删光了；\n\
             无论哪种，本条都会零命中地绿。",
        citing.len()
    );
    let files: std::collections::BTreeSet<&str> = citing.iter().map(|(f, _)| f.as_str()).collect();
    // 🔴 〔步 7c 剖分 2026-09-19 · C 类〕`parity_ledger.rs` → `parity_ledger_tests.rs`：
    // 那张平价账本（上面头注逐字说它「整个住在测试段里」）这一轮搬进了 `tests/bridge/`。
    for expect in [
        "parity_ledger_tests.rs",
        "backend/control/ccm_invocation.rs",
    ] {
        assert!(
            files.contains(expect),
            "`{expect}` 里找不到 §36 引用 —— 人群跑偏了（实得：{files:?}）"
        );
    }

    let mut bad = Vec::new();
    for (f, b) in citing {
        // **无条件要求**：引 §36 就得在同一句里写出它只绑 Windows。
        //
        // 第一版的规则是「这一段提到本机/本地时才要求」——那条件看着周到，其实是
        // 给「读宽了」留了后门：一句「§36 禁了这条路」不提本机也照样把范围读宽。
        // 而 §36 的病灶从来只有一个：**它的适用平台在转述里被丢掉**。
        // ⇒ 直接钉平台词在不在，不去猜这句话是在主张还是在讨论。
        if !b.contains("Windows") {
            bad.push(format!("{f}：{}", b.trim()));
        }
    }
    assert!(
        bad.is_empty(),
        "★ 这些句子引了 §36，却没在**同一句**里写出它只绑 Windows：\n  {}\n\n\
             §36 逐字讲的是 Windows 分支（`config_dir_prefix_ps` / `validate_config_dir_ps`），\
             禁的是「本地渲染器读 `plan.env`」——**不是**「本机不许用 CLI 渲染器」。\n\
             P3t 之前整条本机路就是被这句读宽了的引用挡着的，而挡出来的后果是\
             一个**无 tty、无 tmux** 的进程（用户敲进去的字会被脚本吃掉）。",
        bad.join("\n  ")
    );
}
