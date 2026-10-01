use super::{
    EnvKeyClaim, Verdict, ENV_KEY_CLAIM_SITES, FALLS_SHORT_CEILING, MEASURE_CENSUS, STATUS_CELLS,
    THIRTY_THREE_B_QUESTIONS,
};
use std::path::PathBuf;

const INVARIANTS: &str = include_str!("../../../src/doc/INVARIANTS.md");

fn repo_root() -> PathBuf {
    // 住址唯一源：`crate::guard_support`（头注写着 24 份副本怎么一起漂的）。
    crate::guard_support::repo_root()
}

/// 本族判据的人群：仓里**跟踪着**的文件（`git ls-files` 口径，仓根相对、正斜杠）。
///
/// 走文件系统的话，工作树里没入库的东西也会被算进来 —— 各路施工在仓根 `.scratch/` 下放的临时脚本与读数、
/// 没提交的草稿 README；它们里的地址或数字一错，判据就替一份不在仓里的文件红（或替它绿）。
/// 工作树里删了而索引里还在的那几份也不算（没有正文可读）。
fn tracked_files() -> Vec<String> {
    let out = std::process::Command::new("git")
        .args(["ls-files", "-z"])
        .current_dir(repo_root())
        .output()
        .expect("跑不动 `git ls-files` —— 本族判据的人群口径就是它");
    assert!(
        out.status.success(),
        "`git ls-files` 非零退出：{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let root = repo_root();
    let v: Vec<String> = String::from_utf8_lossy(&out.stdout)
        .split('\0')
        .filter(|rel| !rel.is_empty() && root.join(rel).is_file())
        .map(str::to_string)
        .collect();
    // ★ 自检：清单太短 ⇒ 口径坏了，下面整族会零命中地绿。
    assert!(
        v.len() > 300,
        "`git ls-files` 只列出 {} 个文件 —— 口径坏了（本仓实测上千个）",
        v.len()
    );
    v
}

/// 跟踪着的、住在 `dir` 下（含子目录；按路径段比，不按字符串前缀）且后缀是 `ext` 的文件。
fn tracked_under(dir: &str, ext: &str) -> Vec<String> {
    tracked_files()
        .into_iter()
        .filter(|rel| {
            let p = std::path::Path::new(rel);
            p.starts_with(dir) && p.extension().is_some_and(|e| e == ext)
        })
        .collect()
}

/// `doc/` 下（含子目录）跟踪着的 `.md`。
fn doc_files() -> Vec<PathBuf> {
    let root = repo_root();
    let mut v: Vec<PathBuf> = tracked_under("src/doc", "md")
        .into_iter()
        .map(|rel| root.join(rel))
        .collect();
    v.sort();
    v
}

/// 入口 README 的形状：文件名 `README*.md`，且不在依赖 / 构建产物 / 第三方源码目录下。
fn is_entry_readme(rel: &str) -> bool {
    const SKIP: &[&str] = &["node_modules", "target", "dist", "vendor", ".git"];
    let name = rel.rsplit('/').next().unwrap_or(rel);
    name.starts_with("README")
        && name.ends_with(".md")
        && !rel.split('/').any(|seg| SKIP.contains(&seg))
}

/// 全仓的入口 `README*.md`（**派生，不是手写清单**）。
///
/// # 这张表原来是我手写的七条，而仓里有八份
///
/// 少的那一份是 **`README.en.md`** —— 于是英文入口文档里的符号引用与路径引用
/// **一处都没人守**。同一个文件在本会话里已经是第二次成为盲区
///（上一次是 `doc_copy_registry` 那边：它带着一个陈旧的 `13 tokens`，
/// 而那张表的锚点全是中文措辞，英文散文一条都对不上）。
///
/// ⇒ 病根与本会话反复量到的同一条：**手写清单描述人群**。
/// 改成从跟踪着的文件里挑出来（[`tracked_files`] · [`is_entry_readme`]）。
fn entry_readmes() -> Vec<PathBuf> {
    let root = repo_root();
    let mut v: Vec<PathBuf> = tracked_files()
        .into_iter()
        .filter(|rel| is_entry_readme(rel))
        .map(|rel| root.join(rel))
        .collect();
    v.sort();
    v
}

/// 正控：仓根 `.scratch/` 里放一份**会命中**的 README（形状对、里面指着一个不存在的符号与路径），
/// 本族的人群照旧不含它 —— 判据不受工作树里没入库的东西影响。
#[test]
fn an_untracked_scratch_file_never_enters_the_population() {
    struct Cleanup(PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let dir = repo_root().join(format!(".scratch/doc-claim-probe-{}", std::process::id()));
    let _cleanup = Cleanup(dir.clone());
    std::fs::create_dir_all(&dir).expect("建探针目录");
    let probe = dir.join("README.md");
    // 地址运行时拼：写成字面量的话，源码这一侧的地址判据会把本文件当成指错了的那一份。
    std::fs::write(
        &probe,
        format!(
            "见 `nowhere_probe.{0}::no_such_symbol_probe` 与 `src/no/such/probe.{0}`。\n",
            "rs"
        ),
    )
    .expect("写探针");
    let rel = probe
        .strip_prefix(repo_root())
        .expect("探针在仓里")
        .to_string_lossy()
        .replace('\\', "/");
    assert!(
        probe.is_file() && is_entry_readme(&rel),
        "探针没放成一份入口 README 的形状 —— 下面的「不在人群里」是空转"
    );
    assert!(!tracked_files().contains(&rel), "探针被当成了跟踪着的文件");
    assert!(
        !entry_readmes().contains(&probe),
        "仓根 `.scratch/` 里没入库的 README 进了入口 README 的人群"
    );
}

/// 一张「表头含状态」的表：`(文件名, 表头行号, 各行 = (件名, 首格原文, 状态格原文))`。
///
/// 件名剥掉 `*`/`✅` 便于对表；**首格原文留着**，因为本仓的约定是
/// 「✅ 打在件名上、日期写在状态列」—— 判「文档说完没完」要同时看这两处。
#[allow(clippy::type_complexity)]
fn status_tables() -> Vec<(String, usize, Vec<(String, String, String)>)> {
    let mut out = Vec::new();
    for p in doc_files() {
        let name = p.file_name().unwrap().to_string_lossy().to_string();
        let raw = std::fs::read_to_string(&p).unwrap_or_default();
        let lines: Vec<&str> = raw.lines().collect();
        let mut i = 0usize;
        while i < lines.len() {
            let is_head = lines[i].starts_with('|')
                && lines[i].contains("状态")
                && i + 1 < lines.len()
                && lines[i + 1].starts_with('|')
                && lines[i + 1]
                    .chars()
                    .all(|c| matches!(c, '|' | '-' | ':' | ' '));
            if !is_head {
                i += 1;
                continue;
            }
            let mut rows = Vec::new();
            let mut j = i + 2;
            while j < lines.len() && lines[j].starts_with('|') {
                let cells: Vec<&str> = lines[j].trim_matches('|').split('|').collect();
                let raw_first = cells.first().unwrap_or(&"").trim().to_string();
                let item = raw_first.replace(['*', '✅'], "").trim().to_string();
                let status = cells.last().unwrap_or(&"").trim().to_string();
                rows.push((item, raw_first, status));
                j += 1;
            }
            out.push((name.clone(), i + 1, rows));
            i = j;
        }
    }
    out
}

/// ★ 抽取器自检：扫描面没缩水。坏掉时下面几条会零命中零失败地绿。
#[test]
fn the_doc_scan_actually_reads_the_durable_docs() {
    let files = doc_files();
    // 〔2026-09-18 下调 11 → 10〕不是遍历坏了：`306c862e`（退役三份旧设计文档、
    // 设计与源头归并到）删掉了 `doc/账号用量-usage抓取方案.md`。
    // 现打 `src/doc/*.md` = 10，`git ls-files` 同为 10 ⇒ **没有文件丢，是地板没跟着改**。
    // 〔2026-09-18 二次下调 10 → 9〕又删了一篇：
    // `远端支持方案-agent查看器与代码全景图.md`（2026-07-20 的「设计草案，待用户定 / 未写码」，
    // 已由那一族取代）。现打 `src/doc/*.md` = 9，`git ls-files` 同为 9。
    // ⚠ 往下拧地板的合法理由**只有**「那些文件真的不在了」—— 这两次都是。
    assert!(
        files.len() >= 9,
        "`doc/` 只扫到 {} 个 .md —— 遍历坏了（2026-09-18 现打 9 个）",
        files.len()
    );
    let total: usize = files
        .iter()
        .map(|p| {
            std::fs::read_to_string(p)
                .map(|s| s.lines().count())
                .unwrap_or(0)
        })
        .sum();
    assert!(
        total >= 4000,
        "`doc/` 总共只剩 {total} 行 —— 路径或读法坏了（摸底实测 4158 行）"
    );
    let tables = status_tables();
    // 1 → 2：`src/doc/ARCHITECTURE.md` 新增「backend 四层落地」表。
    // 2 → **1**：那张表退役（架构文档不放进度），它的四格登记与量法同拍删；
    //   剩下的一张是 `INVARIANTS §33b` 那张（五格，照旧逐格量）。
    assert_eq!(
        tables.len(),
        1,
        "「表头含状态」的表张数变了（实得 {:?}）—— **这不是让你改数字**：\n\
             新出现一张就把它的每一格登记进 `STATUS_CELLS` 并配一条现场量法；\n\
             少了一张就说明表被删了或表头措辞变了（那本条会零命中地绿，所以它必须红）。",
        tables
            .iter()
            .map(|(f, l, r)| format!("{f}:{l}（{} 行）", r.len()))
            .collect::<Vec<_>>()
    );
}

/// ★ **两个方向**：文档里每一格都登记了；登记表里没有文档里已经不存在的件。
#[test]
fn every_status_cell_is_registered() {
    let mut in_doc: Vec<String> = status_tables()
        .into_iter()
        .flat_map(|(_, _, rows)| rows)
        .map(|(item, _, _)| item)
        .collect();
    in_doc.sort();
    let mut registered: Vec<String> = STATUS_CELLS.iter().map(|(k, _)| k.to_string()).collect();
    registered.sort();
    assert_eq!(
        in_doc, registered,
        "耐久文档的「状态列」与登记表对不上。\n\
             多出来的格请登记进 `STATUS_CELLS` 并写一条**现场量法**（不是抄它的状态，\n\
             是写「怎么从代码里量出这个状态还对不对」）；\n\
             登记表里多出来的件说明文档改了而这里没跟。"
    );
}

/// 生产段里 `.call("launch")` 的处数（**不数测试段、不数本仓的说明文字**）；人群是壳 `src/` 下跟踪着的 `.rs`。
fn production_launch_calls() -> usize {
    let root = repo_root();
    // 运行时拼，免得命中本文件自己。
    let verb = format!(".call(\"{}\"", "launch");
    tracked_under("src/frontend/shell/src", "rs")
        .into_iter()
        // `launch_wire.rs` 的头注里逐字写着那个串（F07 立的例外，沿用）。
        .filter(|rel| {
            std::path::Path::new(rel).file_name() != Some(std::ffi::OsStr::new("launch_wire.rs"))
        })
        .map(|rel| {
            guard_core::production_code(
                &std::fs::read_to_string(root.join(&rel)).unwrap_or_default(),
            )
            .matches(verb.as_str())
            .count()
        })
        .sum()
}

/// ★★ **核心手法：判据从文档里读那个数，不自己写一份。**
///
/// # 它抓的是什么
///
/// `INVARIANTS §33b` 有一格逐字写着「生产段 `.call("launch")` 今天 **N 处**」。
/// 本条把那个 N 抽出来，与现场数的比。⇒ 那个数**只有一个家（文档）**，
/// 而「有人加了第三处调用而文档还写着 2」变成一条会红的机检。
///
/// ⚠ **F07 漏掉的正是这一格。** 它订正了三问的答案 ①（11 行之后那一处），
/// 而这一行说着同一句话（「零生产调用方 —— 只有一处且在 `cfg(test)` 里」）**没被碰**。
/// **订正手头那一处，不等于订正那句话。**
#[test]
fn the_doc_number_for_production_launch_calls_matches_reality() {
    let marker = "〔机检〕生产段 `.call(\"launch\")` 处数：";
    // ⚠ **F12 扩扫描面**：第一版只读 `INVARIANTS.md` 一份 ⇒
    // `/full-audit` 逮到**第四份副本**住在 `src/doc/IPC-PROTOCOL.md:584`（「生产路径今天还没切过来」），
    // 而它**结构上永远不会红**。⇒ 先扫全 `src/doc/**`，任何一份里出现同一句旧断言都要红。
    {
        let stale = "生产路径今天还没切过来";
        let mut offenders: Vec<String> = Vec::new();
        for p in doc_files() {
            if std::fs::read_to_string(&p)
                .map(|t| t.contains(stale))
                .unwrap_or(false)
            {
                offenders.push(p.file_name().unwrap().to_string_lossy().to_string());
            }
        }
        assert!(
            offenders.is_empty(),
            "这些耐久文档还写着「{stale}」：{offenders:?} —— 那句话在 U8a-2c-1 之后就假了。\n\
                 ⚠ **它是同一句话的第四份副本**，而 F11 的机检只读 `INVARIANTS.md` ⇒ 它结构上够不着。\n\
                 这正是本模块头注那条纪律的反例：**订正一句假话时先把它的全部副本找出来。**"
        );
    }
    let at = INVARIANTS.find(marker).unwrap_or_else(|| {
        panic!(
            "`INVARIANTS.md` 里找不到锚点 {marker:?} —— 那句话被改写了。\n\
                 本条判据的整个价值就是「那个数只有一个家」；\n\
                 改措辞就把它变成零命中地绿，所以宁可让它红。"
        )
    });
    let tail = &INVARIANTS[at + marker.len()..];
    let digits: String = tail.chars().take_while(|c| c.is_ascii_digit()).collect();
    let want: usize = digits
        .parse()
        .unwrap_or_else(|_| panic!("锚点后面不是数字，实得 {:?}", &tail[..12.min(tail.len())]));
    let got = production_launch_calls();
    assert_eq!(
        want, got,
        "文档写着生产段有 {want} 处 `.call(\"launch\")`，实测 {got} 处。\n\
             ⚠ **这两个数只允许有一个家（文档那一处）** —— 别在代码里再写一份，\n\
             回去改文档那个数，并顺手想一想：多出来的那处是不是又切了一格？\n\
             （F04c 加的那处是 `send-keys`，**不是**又切了一格「起会话」——\n\
             那两件事很容易被混成一个数，`launch_wire` 那条判据就是为此改过度量的。）"
    );
}

/// ★ 「外层载荷那几个产出方」—— 逐个**按文档说的状态**复核。
///
/// 这条是「可数的实测断言」里第二条能钉的。⚠ 它**只钉住「在不在」**，
/// 钉不住「它们各自还是不是生产在跑」—— 那需要真远端/真安装包。
///
/// # 🔴 `K-R104`（09-13）：它从「四个都还在」变成「各自是不是文档说的那个状态」
///
/// 上一版逐字叫 `the_four_outer_layer_producers_are_all_still_there`，〔散文墓碑〕
/// 断的是**四条存在性**。而 `K-R104` 让其中一条**真的退役了** ——
/// `account_usage.rs::build_usage_probe_cmd`（用量探针那条 shell 串）
/// 随编排搬上后端帧面而整个不存在了。
///
/// ⇒ 按它自己报错文案里那句话办：「**这多半是好事** …… 回去把它和 `INVARIANTS §33b`
/// 那句『四个产出方，一个都没退役』一起重裁」。**重裁的结果不是删掉本条**，
/// 是把那一格从「必须在」翻成「**必须不在**」——
/// 退役了却又长回来（有人重新在 monitor 里拼一条 tmux 编排串）同样要红。
/// 剩下三条照旧钉存在性。
#[test]
fn the_outer_layer_producers_are_in_the_state_the_doc_claims() {
    let root = repo_root();
    let checks: &[(&str, bool)] = &[
        (
            // 🔴 **「必须在」翻成「必须不在」**（同 `K-R104` 那一格的处置）：
            //    TS 座按删了（外层三格只剩 `payload::render_tmux_outer` 一个家），
            //    `INVARIANTS §33b` 产出方表那一行同拍改记「已删」。长回来 ⇒ 红。
            "session-backend.ts（TS 座，LR2 已删 —— 必须不在）",
            !root.join("src/session-backend.ts").is_file(),
        ),
        (
            "control/launch.rs（backend argv）",
            root.join("src/backend/control/launch.rs").is_file(),
        ),
        (
            // 🔴 住址换了，**产出方本身没退役**：
            //    〔用@09-11 `K33`〕那个 bash 脚本删了，「用户终端那条路」今天由
            //    后端本体的一次性模式渲（`control::ccm::plan::render_container`）。
            //    ⇒ `INVARIANTS §33b` 那句「四个产出方，一个都没退役」**仍然成立**，
            //    只是第四个的住址从 `shared/ccm` 变成了 `control/ccm/plan.rs`。
            "control/ccm/plan.rs（用户终端那条路）",
            root.join("src/backend/control/ccm/plan.rs").is_file(),
        ),
    ];
    let missing: Vec<&str> = checks
        .iter()
        .filter(|(_, ok)| !ok)
        .map(|(n, _)| *n)
        .collect();
    assert!(
        missing.is_empty(),
        "「外层产出方」里这几格与文档说的状态对不上：{missing:?}\n\
             这几条**不在了** ⇒ **这多半是好事**：有产出方退役了 ⇒\n\
             `INVARIANTS §33b` 那张表过期了，回去把它和 U8c-3 的前置一起重裁。"
    );
}

/// 🔴🔴 **`K-R105` `KR105D1`：`§33b` 三问的答案，逐问与现场对拍。**
///
/// 立项理由与手法住 [`THIRTY_THREE_B_QUESTIONS`] 上方那一段（本条不复述）。
/// 一句话：**改一问所依赖的行为而不改那问的答案 ⇒ 当场红。**
///
/// # 三条量法，逐条写清它量的是什么
///
/// - **①「生产切到后端的 `launch` 了吗」** —— 量「哪几棵树的生产段真的发
///   `create-or-attach`」。**两棵树各算一格**：monitor 自己那条 `↗` 路
///   （`src/frontend/shell/src/**.rs`）与后端自带的 CLI 面（`control/ccm/`）。
///   🔴 08-14 那一版只量了前一棵 ⇒ `K-P2` `D3`（09-03）把后一棵翻正之后，
///   那个读数**在它自己的尺子上仍然是对的**，而它答的那一问已经不是原来那一问了。
///   ⇒ 本条把两棵树都收进来，`部分切` 与 `全切` 因此分得开。
/// - **②「attach 那条串归谁产」** —— 量「生产 TS 里还有没有人问座要 attach」。
///   backend **结构上不产 attach**（`control/launch.rs` 头注逐字「本模块不 attach，一次都不」，
///   `parse_request` 的错文案逐字「attach 是平面 ③，不归后端」）⇒
///   前端不产的那天，就是这一问有第二个答案的那天。
/// - **③「daemonless 的远端还要不要能起会话」** —— 量那一档的**三个载体**
///   （落盘字段 · 界面那个 input · 数据源那条轮询回落）。
///   刻意**不数 `daemonless` 这个词**：`remote-config.ts` 里还留着一处认旧配置的墓碑，
///   数名字会把它读成回潮（口径与 `launch_wire.rs` 那条同源）。
///
/// # ⚠ 剥法用的是哪一份
///
/// TS 用 [`guard_core::strip_comment_lines`]（块注释 + 整行 + 行尾），**不是**
/// `production_code` —— 后者按 Rust 的 `#[cfg(test)]` 写，喂 TS 会多剥或少剥。
/// Rust 侧照旧 `production_code`。⇒ 注释里怎么解释这三问都不算数，只看生产段。
#[test]
fn the_three_questions_in_33b_have_todays_answers() {
    let root = repo_root();
    let read = |rel: &str| std::fs::read_to_string(root.join(rel)).unwrap_or_default();
    let prod_rs = |rel: &str| guard_core::production_code(&read(rel));
    let prod_ts = |rel: &str| guard_core::strip_comment_lines(&read(rel));

    // ── 量法 ① ────────────────────────────────────────────────────────────
    // 运行时拼，免得命中本文件自己的说明。
    let mode = format!("\"create-or-{}\"", "attach");
    let word = format!("create-or-{}", "attach");
    let mut monitor_emits = false;
    let mut scanned_rs = 0usize;
    for (p, raw) in guard_core::scan_tree!(&root.join("src/frontend/shell/src"), &["rs"]) {
        // `launch_wire.rs` 的说明里逐字写着那个串（F07 立的例外，本条沿用同一条）。
        if p.file_name().is_some_and(|n| n == "launch_wire.rs") {
            continue;
        }
        scanned_rs += 1;
        if guard_core::production_code(&raw).contains(mode.as_str()) {
            monitor_emits = true;
        }
    }
    // ★ 抽取器自检：人群没缩水（否则 `monitor_emits` 恒 false ⇒ ① 永远读成「部分切」）。
    assert!(
        scanned_rs >= 50,
        "只扫到 {scanned_rs} 个 monitor 侧 `.rs` —— 遍历坏了，量法 ① 会零命中地绿"
    );
    let ccm: String = ["mod.rs", "argv.rs", "plan.rs"]
        .iter()
        .map(|f| prod_rs(&format!("src/backend/control/ccm/{f}")))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        ccm.lines().count() >= 300,
        "`control/ccm/` 三份的生产段只剩 {} 行 —— 读错了或剥法把代码也剥了，量法 ① 会零命中地绿",
        ccm.lines().count()
    );
    let ccm_emits = guard_core::contains_word(&ccm, &word);
    let a1 = match (monitor_emits, ccm_emits) {
        (false, false) => "〔现打①〕一格没切",
        (true, true) => "〔现打①〕全切",
        _ => "〔现打①〕部分切",
    };

    // ── 量法 ② ────────────────────────────────────────────────────────────
    // 座本身（`session-backend.ts`）删了；量法照旧数「生产 TS 里还有谁问座要 attach」——
    // 座长回来、又有人问它要，这一问的判词就翻回「前端仍产」。
    let seat_attach = format!("SESSION_BACKEND.{}", "attach");
    let mut askers: Vec<String> = Vec::new();
    let mut scanned_ts = 0usize;
    for (p, raw) in guard_core::scan_tree!(&root.join("src"), &["ts"]) {
        let name = p
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        // 座本身不算 —— 它是被问的那一层，不是问的人（今天它不在盘上，这一格留着给「长回来」那天）。
        if name.ends_with(".test.ts")
            || name.ends_with(".vitest.ts")
            || name == "session-backend.ts"
        {
            continue;
        }
        scanned_ts += 1;
        let code = guard_core::strip_comment_lines(&raw);
        // 整词，不是裸子串：`…attachFoo` 不算（与 `launch_wire` 那把尺子同口径）。
        if guard_core::contains_word(&code, seat_attach.as_str()) {
            askers.push(name);
        }
    }
    assert!(
        scanned_ts >= 100,
        "只扫到 {scanned_ts} 份生产 TS —— 遍历坏了，量法 ② 会零命中地绿"
    );
    let a2 = if askers.is_empty() {
        "〔现打②〕后端全产 attach"
    } else {
        "〔现打②〕前端仍产 attach"
    };
    // ★ 反向锚点：backend 那条「不 attach」的结构事实还在。它没了，② 的两档都说不清。
    // 🔴 **钉整行，不是子串**（`needle_anchor_registry` 治的那一族：匹配单位比事实小）。
    let launch_rs = read("src/backend/control/launch.rs");
    let no_attach = "//! 开不了你面前的窗）。所以本模块**不 attach**，一次都不。";
    assert!(
        guard_core::pin_line(&launch_rs, no_attach).is_ok(),
        "`control/launch.rs` 头注里那一行「本模块不 attach，一次都不」不在了 ——\n\
             ② 这一问的整个形状建立在它上面（后端结构上开不了你面前的窗）。\n\
             真要改，回 `INVARIANTS §33b` 与 `U8c-3` 重裁，别只改头注。"
    );

    // 🔴 〔条 80 「不要管旧配置」〕**量法 ③ 与第三问一起删了。**
    //    原先它量那一档的三个载体：`remote-config.ts` 的落盘字段 `"daemonless",` ·
    //    `machine-card.ts` 的 `daemonlessInput` · `ssh_source.rs` 的 `daemonless_stream_loop`。
    //    `K-R59`（09-11）早把前两个删了，条 80 又删掉最后那块墓碑
    //    （`LEGACY_NO_BACKEND_KEY`）⇒ **三格在盘上全部不存在，恒 false**
    //    ⇒ 判词恒为「已退役」⇒ 与文档**永远对得上** ⇒ 那是三条恒绿的判据。
    //    删的是判据，不是历史：`INVARIANTS.md §33b` 那一行原地留着并加了一句订正。
    // ── 逐问与文档对拍 ────────────────────────────────────────────────────
    let derived = [a1, a2];
    assert_eq!(
        THIRTY_THREE_B_QUESTIONS.len(),
        derived.len(),
        "三问表的行数与量法条数对不上 —— 加一问要同时加一条量法"
    );
    for ((q, verdicts), got) in THIRTY_THREE_B_QUESTIONS.iter().zip(derived) {
        assert!(
            verdicts.contains(&got),
            "`{q}` 量出来的判词 {got:?} 不在它自己的闭集里 —— 量法与表对不上"
        );
        let present: Vec<&&str> = verdicts
            .iter()
            .filter(|v| INVARIANTS.contains(**v))
            .collect();
        assert_eq!(
            present.len(),
            1,
            "`{q}`：`INVARIANTS.md` 里出现的判词是 {present:?} —— 必须**恰好一个**。\n\
                 · 一个都没有 ⇒ 那一问的答案没带判词（改措辞就把本条变成零命中地绿，\n\
                   所以宁可让它红）；\n\
                 · 出现两个以上 ⇒ 同一问在文档里有两份互相矛盾的答案。\n\
                 闭集：{verdicts:?}"
        );
        assert_eq!(
            *present[0], got,
            "🔴 **`{q}` 的答案过期了。**\n\
                 文档里写着 {:?}，现打是 {got:?}。\n\
                 · **事实前进了而答案没跟**（这一族在本节犯过至少三次：F07 · `K-R59` · `K-P2 D3`）\n\
                   ⇒ 改文档那一格，并同轮问一句：这一问挡着的那件事，今天还挡不挡得住？\n\
                 · **答案改了而事实没动** ⇒ 那是有人在文档里许了一个还没兑现的愿。\n\
                 ⚠ 现场读数：① monitor 树发 `create-or-…`={monitor_emits} · `control/ccm/` 发={ccm_emits}；\n\
                 ② 生产 TS 里还问座要 attach 的：{askers:?}。",
            present[0]
        );
    }
}

/// ★★ `K-R73` `KR73D3`：**普查表与 `STATUS_CELLS` 两个方向对拍**，外加那条递减棘轮。
///
/// # 没有这一条，那张普查表会跟本模块治的那些文档副本一样腐
///
/// 新加一格量法而普查表没跟 ⇒ 那一格**没人判过它量的是不是它声称的那件事**，
/// 而这正是 `R29` 裁定零那次的形状（`observe`/`platform`/`common` 三格是**跟着**
/// `control/` 那格一起写下去的，没有一格被单独问过「你量的是落地还是有个目录」）。
/// 反方向也要：普查表留着一行而 `STATUS_CELLS` 那边没了 ⇒ 那一行在描述一个不存在的量法。
#[test]
fn every_status_cell_measure_is_in_the_census() {
    let mut cells: Vec<&str> = STATUS_CELLS.iter().map(|(_, how)| *how).collect();
    cells.sort();
    cells.dedup();
    let mut census: Vec<&str> = MEASURE_CENSUS.iter().map(|(k, ..)| *k).collect();
    census.sort();
    // 同一个量法键不许在普查表里出现两次 —— 两行说法不一致时谁也不知道哪行算数。
    let mut uniq = census.clone();
    uniq.dedup();
    assert_eq!(
        census, uniq,
        "普查表里有重复的量法键 —— 一个事实恰好一个住址"
    );
    assert_eq!(
        cells, census,
        "`STATUS_CELLS` 的量法键与 `MEASURE_CENSUS` 对不上。\n\
             **`STATUS_CELLS` 多出来的**：新加了一格量法却没判过「它量的是不是它声称的那件事」——\n\
             那正是 `DECISIONS.md#R29` 裁定零那次的形状（三格裸 `is_dir()` 是跟着上一格一起写下去的）。\n\
             **普查表多出来的**：那一行在描述一个已经不存在的量法，摘掉它。"
    );
    // 分母自检：表空了上面那个等号会退化成「空 == 空」。
    // 地板 10 → **9**：`usage-probe-uses-the-kernel` 那一格
    // 随用量 ③ 轴整轴退役（它量的是「用量探针在不在调载荷内核」，探针没了）。
    // ⚠ **降地板要写清是哪一行、为什么** —— 这一条挡的是「偷偷删行」，
    // 而「那一格量的东西整块不存在了」是唯一正当的降法。
    // 地板 9 → **5**：`monitor-backend-*-landed` 那四行随 ARCHITECTURE 那张进度表整块退役
    //   （那四格量的东西在文档里不存在了 —— 同上面那条唯一正当的降法），剩 `INVARIANTS §33b` 的五格。
    assert!(
        MEASURE_CENSUS.len() >= 5,
        "普查表只剩 {} 行（P6 后现打 5 行）—— 少于分母说明有人在偷偷删行",
        MEASURE_CENSUS.len()
    );
    for (k, _, _, why) in MEASURE_CENSUS {
        assert!(
            why.trim().chars().count() >= 30,
            "`{k}` 那一行没写清「它到底量了什么、与那句话差在哪」（只有 {} 字）",
            why.trim().chars().count()
        );
    }
    // ★ 递减棘轮：**对不上**那一栏只许比今天少。
    let falls_short = MEASURE_CENSUS
        .iter()
        .filter(|(_, _, v, _)| *v == Verdict::FallsShort)
        .map(|(k, ..)| *k)
        .collect::<Vec<_>>();
    assert!(
        falls_short.len() <= FALLS_SHORT_CEILING,
        "「量法与它声称的性质对不上」涨到 {} 条了 > 棘轮上限 {FALLS_SHORT_CEILING}（09-12 现打 5）。\n\
             逐条：{falls_short:?}\n\
             ⚠ **不许把上限调上去让今天好过** —— 这是递减棘轮。",
        falls_short.len()
    );
}

// 这里原住着那四格「monitor 侧四层落地」的落地探针与它的反向自检：
//   `a_capability_line_has_landed` · `layer_has_landed_at` · `the_capability_line_landing_probe_actually_bites`〔散文墓碑〕。
//   它们只服务 ARCHITECTURE 那张进度表；那张表退役（架构文档不放进度，进度住），四格人群为空，
//   探针连同自检一起删 —— 留着就是一条没人喂的恒真判据。量法与普查行同拍删（`STATUS_CELLS` / `MEASURE_CENSUS`）。

/// ★★ 逐格跑「现场量法」：**文档里那一格**记的状态今天还对不对。
///
/// ⚠ 状态**从文档读**，不从登记表读 —— 见 `STATUS_CELLS` 的头注：
/// 第一版存了一份副本，E4 变异（只改文档里的状态）时五条判据**全绿**。
///
/// ⚠ 量法都是**结构性**的（文件/符号在不在、生产段有没有在调），**不是**跑功能。
/// 那是刻意的：这一族的失效形态是「事实变了而文档没跟」，
/// 而结构性事实恰好是变了就一定能看见的那种。
#[test]
fn each_registered_status_still_matches_reality() {
    let root = repo_root();
    let read = |rel: &str| std::fs::read_to_string(root.join(rel)).unwrap_or_default();
    let prod = |rel: &str| guard_core::production_code(&read(rel));
    // 文档那张表：件名 → (首格原文, 状态格原文)
    let cells: std::collections::BTreeMap<String, (String, String)> = status_tables()
        .into_iter()
        .flat_map(|(_, _, rows)| rows)
        .map(|(item, raw, status)| (item, (raw, status)))
        .collect();
    for (item, how) in STATUS_CELLS {
        let (delivered, why) = match *how {
            "payload-kernel-exists" => (
                // 内核搬进后端，量法跟着换住址。
                prod("src/backend/control/launch_render/payload.rs").contains("fn render_payload"),
                "载荷内核在后端 `control/launch_render/payload.rs`",
            ),
            // ⚠ **F12 订正**：第一版左支读的是 `src/frontend/shell/src/shell_quote.rs` —— **那个文件不存在**
            // ⇒ 左支恒 false，整条判据只靠右支撑着（`/full-audit` 逮到的）。
            // 这正是「判据自己会不会错」那一问要问的东西：**读一个不存在的文件不会报错，
            // 只会静默返回空串**，而 `||` 让它看起来像「两条都在查」。
            // ⇒ 改成读真正的家，并加一条「文件必须存在」的断言，杜绝同样的静默空转。
            "posix-quote-has-one-home" => (
                {
                    // 从前读 monitor `ssh_source.rs` 那层转调壳（`shell_quote`〔散文墓碑〕）转不转内核；那层壳零生产调用、删了
                    //   ⇒ 量法改读家本身（唯一性另由 `quote_singleton_guard` 守）。
                    let home = "src/common/shell-quote-core/src/lib.rs";
                    assert!(
                        repo_root().join(home).is_file(),
                        "量法读的 {home} 不存在 —— 读不到的文件只会静默返回空串"
                    );
                    prod(home).contains("pub fn posix_quote(")
                },
                "Rust 侧的 POSIX quote 收在共享 crate（另有 `quote_singleton_guard` 单点守卫）",
            ),
            "ccm-invocation-kernel-exists" => (
                prod("src/backend/control/launch_render/ccm_invocation.rs")
                    .contains("fn render_ccm_invocation"),
                "ccm 调用行内核在后端 `control/launch_render/ccm_invocation.rs`",
            ),
            "production-ts-calls-the-rust-renderers" => (
                // 两条渲染今天是那台后端的帧命令，主路经 `src/frontend/ui/launch-render.ts` 问。
                read("src/frontend/ui/launch-render.ts")
                    .contains("chan.call(origin, \"launch-render-cli\"")
                    && read("src/frontend/ui/launch-render.ts")
                        .contains("chan.call(origin, \"launch-render-payload\"")
                    && read("src/frontend/ui/remote-launch-run.ts").contains("renderCli(")
                    && read("src/frontend/ui/remote-launch-run.ts").contains("renderPayload("),
                "生产 TS 主路在问那台后端的两条渲染帧命令",
            ),
            // 「待做」那一格：**反向**量法 —— TS 渲染器还在，就说明确实还没删。
            "ts-renderer-still-there" => (
                !root.join("src/session-backend.ts").is_file(),
                "TS 渲染器已经删了",
            ),
            other => panic!("`{item}` 的量法键 {other:?} 没有实现 —— 登记表与实现漂了"),
        };
        let (raw_first, status) = cells
            .get(*item)
            .unwrap_or_else(|| panic!("`{item}` 不在文档那张表里 —— 另一条判据会先红"));
        // 本仓的约定：**✅ 打在件名上、日期写在状态列**；或者状态列直接写「已交付」。
        //
        // ⚠ **「先出现的那个词算数」**，不是「含哪个词」——状态格里常常带订正叙事
        // （`U8c-2c-2` 那格逐字写着「本列此前写『待做』」），两个词都在里面。
        // 第一版用 `contains` 判，当场被它红了；「先出现的算数」既能容纳叙事，
        // 又不至于把判定权交给措辞。
        let done_at = status.find("已交付");
        let todo_at = status.find("待做").or_else(|| status.find("未做"));
        let says_done = match (done_at, todo_at) {
            _ if raw_first.contains('✅') => true,
            (Some(d), Some(t)) => d < t,
            (Some(_), None) => true,
            (None, Some(_)) => false,
            (None, None) => panic!(
                "`{item}` 的状态格里既没有「已交付」也没有「待做」（首格 {raw_first:?} / \
                     状态 {status:?}）—— 改措辞就把本条变成零命中地绿，所以宁可让它红。"
            ),
        };
        assert_eq!(
            delivered, says_done,
            "`{item}`：文档那一格说「已交付 = {says_done}」（状态列原文 {status:?}），\n\
                 而现场量法说「{why}」= {delivered}。\n\
                 ⚠ 两种可能，都要动手：\n\
                 · 事实前进了而状态列没跟（**这一族最常见**，六轮 14 处都是它）⇒ 改文档；\n\
                 · 或者本条量法本身选错了标的（U8c-2a 就这么红过一次）⇒ 改量法 + 写清为什么。"
        );
    }
}

/// ★★ **F12 全局变异抽样抓到的缺口**：文档里**枚举的一组标识符**没人对拍。
///
/// # 它是怎么被发现的
///
/// Phase G 的全局变异抽样里，把 backend `inbound.rs` 的 `unknown_command`
/// **三处一起改名**成 `unknown_cmd` —— **backend 253 条全绿**。
/// 而 `src/doc/IPC-PROTOCOL.md` 逐条列着六个**协议级**错误码，语义是
/// 「客户端代码写错了，别重试」—— 那是**仓外可见的契约**（`resolve` 那条已经与 aterm 冻结）。
///
/// ⚠ F11 建这个登记表时扫的是「状态列」与「可数的实测断言」两族，
/// **漏了第三种形状：文档里枚举的一组标识符**。
/// 那不是「F11 做漏了」——是**摸底时的分族本身不完整**，
/// 而**只有跨模块的变异抽样能发现这种「整族缺口」**（本工作区自己的判据都在
/// 各自那件的范围里看，看不到「有一族根本没人管」）。
///
/// ⇒ 这条也是 skill 那句「**变异存活分布不会说谎**」在本工作区拿到的实货。
#[test]
fn the_protocol_level_error_codes_in_the_doc_are_the_ones_the_backend_uses() {
    const IPC: &str = include_str!("../../../src/doc/IPC-PROTOCOL.md");
    let marker = "**协议级**由 `inbound.rs` 独占 ——";
    let at = IPC.find(marker).unwrap_or_else(|| {
        panic!(
            "`IPC-PROTOCOL.md` 里找不到锚点 {marker:?} —— 那句话被改写了。\n\
                 本条判据的价值是「那份清单只有一个家（文档）」，改措辞就把它变成零命中地绿。"
        )
    });
    // ⚠ 收尾锚点是**清单本身的收尾**（`，语义是`），不是段落结束 ——
    // 第一版切到空行，把后面几句里的 `invalid_args`/`launch`/`resolve` 也收进来了
    // （抽到 9 个而真值 6 个）。**抽取器自检当场把它拦下来了**，这就是它的岗位。
    let seg = &IPC[at..];
    let end = seg
        .find("，语义是")
        .expect("找不到清单的收尾锚点「，语义是」—— 那句话被改写了");
    let seg = &seg[..end];
    let mut in_doc: Vec<&str> = Vec::new();
    let mut rest = seg;
    while let Some(a) = rest.find('`') {
        rest = &rest[a + 1..];
        let Some(b) = rest.find('`') else { break };
        let word = &rest[..b];
        rest = &rest[b + 1..];
        // 只收「像错误码」的词：全小写 + 下划线，且不是模块名。
        if !word.is_empty()
            && word.chars().all(|c| c.is_ascii_lowercase() || c == '_')
            && word != "inbound"
        {
            in_doc.push(word);
        }
    }
    in_doc.sort();
    in_doc.dedup();
    // ★ 抽取器自检：这一段里就该有六个码；抽不到就说明剥法坏了。
    assert_eq!(
        in_doc.len(),
        6,
        "从文档那一段只抽到 {} 个协议级错误码（{in_doc:?}）—— 剥法坏了，\n\
             下面那条会零命中地绿。文档实测是六个：bad_request · line_too_long ·\n\
             unknown_command · duplicate_id · handler_panicked · not_cancellable。",
        in_doc.len()
    );
    let backend = std::fs::read_to_string(repo_root().join("src/backend/stream/inbound.rs"))
        .expect("读不到后端的 inbound.rs");
    let prod = guard_core::production_code(&backend);
    let missing: Vec<&&str> = in_doc
        .iter()
        .filter(|c| !prod.contains(&format!("\"{c}\"")))
        .collect();
    assert!(
        missing.is_empty(),
        "`IPC-PROTOCOL.md` 列着这些协议级错误码，而后端生产段里**找不到**：{missing:?}\n\
             ⚠ 它是**仓外可见的契约**（`resolve` 那条已经与仓外 aterm 冻结）——\n\
             改名 = 静默毁约：对端拿到一个它不认识的码，而两侧的测试都不会红\n\
             （F12 的全局变异抽样就是这么把这个缺口逮出来的：三处一起改名，backend 253 条全绿）。\n\
             要改就两侧一起改，并想清楚仓外消费方。"
    );
}

/// **`doc/` 里点名的代码符号必须解析得到，且住在文档说的那个文件里。**
///
/// **为什么建它**：08-06 把本会话逮到的每一处文档/计划腐坏按机制归了族，主力是
/// **停滞式** —— 世界变了、文本一个字没动（改代码的那个提交**碰过**那份文件，
/// 却把已假的那句原样留着）。这一族**结构上救不了**靠「改法纪律」：留痕是「怎么改」的规矩，
/// 而这族的定义就是没人来改。能接住它的只有一样：**一条会红的判据读到那句散文**（E12 ①）。
///
/// `file.rs::symbol` 是 `doc/` 里**最可机检**的一族散文：改名 / 删除 / 搬家都让它变假，
/// 而**没有任何东西会红**。建判据当天实测 **73 处引用、真腐 0**——
/// ⚠ **「今天全对」正是建它的理由，不是不建的理由**：干净是纪律攒出来的，
/// 而纪律不在门禁里就只是运气，`doc/` 这四个文件此前**一条判据都没读过**。
///
/// **口径**（比「符号存在」严一档，建时实测不误红）：文档写 `a.rs::foo`，
/// 就要求 `foo` 的声明**出现在 `a.rs` 里** —— 只对符号名会放过「搬到别的文件」，
/// 而搬家恰恰是本仓重构的常见形态。
///
/// ⚠ ~~抽取器摘除调用者自己（`scan_tree!` 按构造如此 —— 而那一刀今天**不生效**）
/// ⇒ 只在 `doc/` 引用本文件里的符号时才会误红~~ —— **08-06 当天就误红了一次**
/// （`DEVELOPMENT.md` 指向本文件里的一条判据）。
/// 现在本文件自己也进扫描面（见下方 `srcs.push`）。**「只在极少数情况下会错」不是边界，是欠账。**
///
/// ⚠ 上面被划掉那句话的**前提**也是假的，一并记下来：
/// `scan_tree!` 那一刀在这一处不生效（判据由 `#[path]` 挂载 ⇒ `file!()` 是折返路径
/// ⇒ 后缀比不命中）。也就是说 08-06 那次误红**不是**「摘除生效带来的已知例外」，
/// 而是当年判据与 `doc_claim_registry.rs` 同住一份文件、`file!()` 真的命中过。
#[test]
fn every_code_symbol_named_in_the_docs_still_resolves() {
    /// 例外表：**每条都写清「为什么它解析不到却是对的」**。
    /// 下面有一条自检把「已经不需要的例外」揪出来 —— 例外表自己也会腐。
    const EXCEPTIONS: &[(&str, &str)] = &[
        (
            "render_local_attach",
            "历史句：原 monitor Tauri 命令（本机接回那一句），搬成本机后端 `launch-local` 的接回那一格。\
             `INVARIANTS §33b` ② 那一格的沿革与 `CONTRIBUTING` 那张表逐字点着它，那是「attach 归谁产」怎么一步步落地的线索",
        ),
        (
            "setup",
            "tauri 的 `.setup(move |app| …)` 钩子闭包 —— 是真东西，但不是一处声明",
        ),
        // 「示例占位符」那一行（CONTRIBUTING 教人照着加一条 Tauri 命令的那个假名字）摘了：CONTRIBUTING 按 4.0.0 重写，加命令的做法改成讲帧命令与 `MONITOR_OWN`，那个示例占位符不在了。
        (
            "run_tmux_reconcile_poller",
            "`INVARIANTS.md` 那句逐字写着它**已删**（audit-fixes F03.2）—— 历史句，\
                 删掉反而丢掉「为什么今天没有 poller」的解释",
        ),
        // 下面四条：`INVARIANTS.md` §33b 的沿革段逐字点着它们 —— 它们是 TS 兜底一族的
        //   「存续理由」判据与两把尺子，随那一族（`launch-render-fallback.ts` · `session-backend.ts` · 五个 builder）
        //   按删了。那几句是「那一族当年靠什么站着」的解释，改写会丢线索（同 `build_usage_probe_cmd`）；
        //   「那一族长回来」由 `tests/frontend/ui/launch-no-shell-in-ts.vitest.ts` 挡着。
        (
            "the_ts_fallback_renderer_now_stands_on_its_own_consumers",
            "历史句：TS 兜底渲染器的「存续理由」判据，那一族删了",
        ),
        (
            "the_retired_premise_left_a_tombstone_that_is_still_on_the_board",
            "历史句：上一条的看守，同一拍删了",
        ),
        (
            "TS_FALLBACK_KEEPERS",
            "历史句：尺子A（兜底一族的消费者处数表），同一拍删了",
        ),
        (
            "TS_FALLBACK_REACH",
            "历史句：尺子B（兜底一族有没有生产调用方），同一拍删了",
        ),
        (
            "build_usage_probe_cmd",
            "★`INVARIANTS.md` §33b 那两处逐字写着它**已退役** \
                 —— 用量探针的整条编排搬上后端帧面之后，monitor 一个 shell 字符都不渲染。\
                 那两句正是「外层四个产出方里退役了哪一个、为什么」的解释，\
                 **删掉这个地址反而丢掉线索**（同上面 `run_tmux_reconcile_poller` 那条）。\
                 ⚠ 它**今天是无人看管的**：原先由 \
                 `the_outer_layer_producers_are_in_the_state_the_doc_claims` 翻面钉着\
                 （「这个函数要是回来了就红」），而用量 ②③ 两轴整轴退役之后那一格已随\
                 `account_usage.rs` 整删 —— **如实登记为射程边界**：\
                 挡「它回来」的今天只有「整个功能不存在」这个事实，没有判据。",
        ),
    ];
    const KW: &[&str] = &[
        "fn", "struct", "enum", "const", "static", "trait", "mod", "type",
    ];

    // ── 收全仓声明：符号名 → 它出现在哪些文件名里
    let mut srcs: Vec<(PathBuf, String)> = Vec::new();
    // 🔴 〔搬树 2026-09-18 ·  纪律 3〕**第四棵：`tests`。**
    //    `doc/` 点名的符号里有一整批是**判据名**（`INVARIANTS.md` 那几行逐字
    //    「由某某 `every_…` 那条判据钉着」，写成住址形），
    //    而判据剖分之后整个住进了 `<repo>/tests/`。
    //    ⚠ 这段解释里**刻意不写出那个住址形**（`文件·rs` ＋ 两个冒号 ＋ 符号名）——
    //      写了它自己就成了一处地址，而 `structural_scan` 那条判据会去核它。
    //    少这一棵 ⇒ 那些符号被读成「全仓找不到 —— 改名或删了」，
    //    而它们一个都没改名、也没删，只是搬了家。
    //    ⚠ 四棵根互不包含（`§5.4b` 纪律 1）。
    for root in [
        "src/frontend/shell/src",
        "src/common",
        "src/backend",
        "tests",
    ] {
        srcs.extend(guard_core::scan_tree!(&repo_root().join(root), &["rs"]));
    }
    // 〔08-06 第二次补扫描面〕**把 `doc_claim_registry.rs` 也收进来**。
    // ⚠ 先前这里的理由是「`scan_tree!` 按构造摘除调用者」——
    //   那一刀**在这一处不生效**（判据由 `#[path]` 挂载 ⇒ 折返路径 ⇒ 后缀比不命中），
    //   而且上面四棵根里逐字含 `"tests"` ⇒ **本判据文件本来就在语料里**。
    //   ⇒ 下面这一 `push` 今天是冗余的（`src/frontend/shell/src` 那棵已经收过同一份），
    //   而它**刻意不删**：这里收的是**声明**不是语料，多一份只会让名字更像活的，
    //   方向安全，而且它把「那一份一定在」钉成一件不依赖根清单的事。
    // ⚠ 这不是假设：头注原本写着「只在这种情况下才会误红」，而 08-06 当天就发生了
    // （`DEVELOPMENT.md` 指向本文件的 `the_backend_test_command_in_the_docs_matches_ci`）。
    // ⇒ 把「已知的例外」变成「已修的缺陷」，头注那句警告随之删掉。
    srcs.push((
        PathBuf::from("doc_claim_registry.rs"),
        include_str!("../../../src/frontend/shell/src/doc_claim_registry.rs").to_string(),
    ));
    // `build.rs` 是单文件、不在任何被扫的目录下 —— 第一版就漏了它，
    // 于是 `build.rs::emit_backend_build_id` 被当成「腐了」。**抽取器的扫描面要自己说清楚。**
    let br = repo_root().join("src/frontend/shell/build.rs");
    let br_src = std::fs::read_to_string(&br).expect("读不到 src/frontend/shell/build.rs");
    srcs.push((br, br_src));

    let mut decl: std::collections::BTreeMap<String, std::collections::BTreeSet<String>> =
        std::collections::BTreeMap::new();
    for (p, raw) in &srcs {
        let fname = p
            .file_name()
            .expect("源文件名")
            .to_string_lossy()
            .to_string();
        // 注释里的 `fn foo` 不算声明 —— 否则「注掉一个函数」这种变异会被判据放过。
        let stripped = guard_core::strip_comment_lines(raw);
        for line in stripped.lines() {
            let mut it = line.split_whitespace().peekable();
            while let Some(tok) = it.next() {
                if !KW.contains(&tok) {
                    continue;
                }
                let Some(next) = it.peek() else { continue };
                let ident: String = next
                    .chars()
                    .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                    .collect();
                if !ident.is_empty() {
                    decl.entry(ident).or_default().insert(fname.clone());
                }
            }
        }
    }
    // ★ 抽取器自检 1：收不到足够多的声明 ⇒ 遍历坏了，下面整条会零命中地绿。
    assert!(
        decl.len() > 2000,
        "全仓只抽到 {} 个声明符号 —— 遍历或剥法坏了（建判据当天实测 3073 个 / 133 个源文件）",
        decl.len()
    );

    // ── 收 doc/ 里的 `file.rs::symbol`
    let mut refs: Vec<(String, usize, String, String)> = Vec::new();
    // 〔08-06 扩面〕**不止 `doc/`**：各目录的 `README.md` 同样在教人「去看哪条判据」，
    // 而它们此前不在扫描面里 —— 我当天就在 `tests/e2e/README.md` 里写下一个指针，
    // 于是那个指针**没有任何东西守着**。⇒ 把入口 README 一并收进来。
    // 实测扩面当日：这些 README 里共 11 处这种引用，**解析不到 0 处**（不误红）。
    let mut targets: Vec<PathBuf> = doc_files();
    let base = targets.len();
    targets.extend(entry_readmes());
    // ★ 扩面自检：入口 README 收不到 ⇒ 路径写错了，扩面等于没做。
    assert!(
        targets.len() >= base + 5,
        "入口 README 只收到 {} 个（doc/ 之外）—— 路径写错了，扩面是空转的",
        targets.len() - base
    );
    for p in targets {
        // ⚠ 用**仓相对路径**而不是裸文件名：扩面后有七个 `README.md`，
        // 裸名会让诊断把 `tests/e2e/README.md` 打印成 `doc/README.md` —— 指错地方的诊断
        // 比没有诊断更费时间（本会话反复吃过「读诊断」的亏）。
        let fname = p
            .strip_prefix(repo_root())
            .unwrap_or(&p)
            .to_string_lossy()
            .to_string();
        let text = std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("读 {p:?} 失败: {e}"));
        for (i, line) in text.lines().enumerate() {
            let b = line.as_bytes();
            let mut from = 0usize;
            while let Some(k) = line[from..].find(".rs::") {
                let at = from + k;
                // 往前收路径：只走 ASCII 路径字符 ⇒ 遇到中文（多字节）自然停在字符边界上。
                let mut s = at;
                while s > 0 && {
                    let c = b[s - 1] as char;
                    c.is_ascii_alphanumeric() || c == '_' || c == '.' || c == '/' || c == '-'
                } {
                    s -= 1;
                }
                let base = line[s..at + 3]
                    .rsplit('/')
                    .next()
                    .unwrap_or_default()
                    .to_string();
                let mut e = at + 5;
                while e < b.len() && {
                    let c = b[e] as char;
                    c.is_ascii_alphanumeric() || c == '_'
                } {
                    e += 1;
                }
                let sym = line[at + 5..e].to_string();
                if !sym.is_empty() && !base.is_empty() {
                    refs.push((fname.clone(), i + 1, base, sym));
                }
                from = at + 5;
            }
        }
    }
    // ★ 抽取器自检 2：`doc/` 里本来就有几十处 —— 抽到个位数就是剥法坏了。
    assert!(
        refs.len() >= 70,
        "只抽到 {} 处 `file.rs::symbol` —— 剥法坏了（`doc/` 当日 73 处，扩面后另加各 README 11 处）",
        refs.len()
    );

    // ★ 自检 3：例外表保鲜。例外是**欠账**，不是免检章。
    for (sym, why) in EXCEPTIONS {
        let used: Vec<&(String, usize, String, String)> =
            refs.iter().filter(|r| r.3 == *sym).collect();
        assert!(
            !used.is_empty(),
            "例外表里的 `{sym}` 在 `doc/` 里已经没人写了 —— 删掉这一行。\n\
                 （它当初的理由：{why}）"
        );
        let still_needed = used
            .iter()
            .any(|r| !matches!(decl.get(&r.3), Some(fs) if fs.contains(&r.2)));
        assert!(
            still_needed,
            "例外 `{sym}` 现在**解析得到了** —— 删掉这条例外，别让例外表替真判据挡枪。\n\
                 （它当初的理由：{why}）"
        );
    }

    let bad: Vec<String> = refs
        .iter()
        .filter(|r| !EXCEPTIONS.iter().any(|(s, _)| *s == r.3))
        .filter_map(|(f, ln, base, sym)| match decl.get(sym) {
            None => Some(format!(
                "{f}:{ln}  `{base}::{sym}` —— **全仓找不到这个符号**（改名或删了）"
            )),
            Some(fs) if !fs.contains(base) => Some(format!(
                "{f}:{ln}  `{base}::{sym}` —— 符号还在，但**搬家了**：现住 {:?}",
                fs.iter().collect::<Vec<_>>()
            )),
            _ => None,
        })
        .collect();
    assert!(
        bad.is_empty(),
        "`doc/` 点名了这些代码符号，而它们今天对不上：\n{}\n\n\
             ⚠ 这是**停滞式腐坏**的典型形态：改代码的人不会回来改文档，而在本判据之前
             **没有任何东西会因此变红**。两条修法（E12）：① 把文档改对；\
             ② 那句话若只是历史，就写清「已删 / 已改名」并进本条的例外表（带理由）。",
        bad.join("\n")
    );
}

/// **`doc/` 里点名的仓内文件路径必须解析得到。**
///
/// 与上一条（`file.rs::symbol`）同族、更宽一档：符号那条只看得见 `.rs`，
/// 而 `doc/` 里点名的还有 `.ts` / `.sh` / `.mjs` / `.json` / `.yml`。
/// 建判据当天实测 **119 处**带目录的路径引用，逐条核完**真腐 1 处**：
/// `INVARIANTS.md` 里的 `src/backend/observe/accounts_query.rs`
/// —— 那个文件早已搬进 `observe/`，而**没有任何东西会因此变红**（本条即为此建）。
///
/// ⚠ **解析口径用 `git ls-files` 而不是磁盘**：磁盘会把「本机生成、CI 里还不存在」的
/// 生成物也算成解析得到（`src/frontend/shell/gen/schemas/**` 就是），那样判据在两个环境里结论不同 ——
/// 而**结论随环境变的判据比没有判据更坏**。生成物走例外表，理由写明。
///
/// ⚠ 匹配用**后缀**：文档常按「隐含根」写（`control/gate.rs` 指的是
/// `src/backend/control/gate.rs`）。第一版用全路径相等，
/// 一口气误报 36 处 —— 又一次**匹配单位比事实小**。
#[test]
fn every_repo_path_named_in_the_docs_still_resolves() {
    /// 例外：**解析不到却是对的**。三种形状，每种都在本仓真实出现过。
    const EXCEPTIONS: &[(&str, &str)] = &[
        (
            "src/frontend/shell/gen/schemas/acl-manifests.json",
            "tauri 构建生成物 + gitignore：磁盘上有、`git ls-files` 里没有，且不同环境有无不定",
        ),
        (
            "shared/ccm-wrapper.sh",
            "**历史句**：原文逐字写着「取代已删除的 …」——删掉它反而丢掉「今天为什么没有 wrapper」",
        ),
        (
            "tests/e2e/tmux-guarded-acceptance.sh",
            "**历史句**：`INVARIANTS §34` 那一段逐字在说「这套 e2e 的输入源是\
                 那个已被删掉的 builder ⇒ 整套删了」——它点这个路径正是为了说清**哪一套没了**；\
                 删掉这句话，读的人只会看见「三道门少了一层真机验收」而不知道为什么",
        ),
        (
            "src/launch-render-cli.ts",
            "**历史句**：`INVARIANTS §33` 背景段逐字讲「F03 当时有两个渲染器、\
                 各住哪」—— TS 那份 CLI 渲染器已删，§33 末尾的 LR1 更新段给了今天的住址\
                 （`ccm_invocation.rs`）。改写背景段会丢掉「这条铁律当初是对着哪一份立的」",
        ),
        (
            "tests/launch-render-cli.test.ts",
            "**历史句**：`INVARIANTS §33` 三处「验证」逐字记着当时那几刀下在\
                 哪套测试上（R04① · #76 防线）。套件随 TS 渲染器删了，今天的验证住址逐条写在\
                 §33 末尾的 LR1 更新段",
        ),
        // 下面四条：TS 兜底一族（座 · 兜底渲染器）与它们的两份套件按删了。
        //   `INVARIANTS §31 / §31a / §33 / §33b` 与 `CONTRIBUTING` 那一节点它们的句子都是**沿革**
        //   （「阶段①问前端座」「门禁腐过一次」「四处同源」「双渲染器」），每处旁边都补了那句今天的住址；
        //   删掉路径，读的人就不知道今天那条规矩当初是对着哪一份立的。
        (
            "src/session-backend.ts",
            "**历史句**：TS 座，已删（外层三格今天只在 `payload.rs`）",
        ),
        (
            "src/launch-render-fallback.ts",
            "**历史句**：TS 兜底渲染器，已删",
        ),
        (
            "tests/session-backend-gate.vitest.ts",
            "**历史句**：`§31` 第①条的旧机检，已由 `tests/frontend/ui/launch-no-shell-in-ts.vitest.ts` 接替",
        ),
        // `tests/session-backend.test.ts` 那一行摘了：唯一点它的 CONTRIBUTING 沿革段随重写删了。
        // `src/frontend/ui/cards/memory-recall.ts` 那一行摘了：CONTRIBUTING 的示例改成「写在 `cards/` 下」，不再点那个占位文件名。
        (
            "code-picture/doc/agents/claude-code.md",
            "**跨仓引用**：另一个仓的语料，本仓解析不到是正常的",
        ),
        ("agents/claude-code.md", "同上（同一句里的简写形）"),
        // `account-ux/` · `unify-launch/` 两行摘了：`doc/` 里点它们的出处删了之后没人再这样写。
        // 账号切换那份计划仓设计稿那一行摘了：INVARIANTS 那一段改写成现状之后 `doc/` 里没人再指它。
        // `/.mcp.json` 那一行摘了：INVARIANTS 那两处改写之后 `doc/` 里没人再这样写。
    ];
    const EXTS: &[&str] = &["rs", "ts", "sh", "mjs", "json", "yml", "toml", "md", "py"];

    // 解析口径：跟踪着的文件（自检在 [`tracked_files`] 里：清单太短 ⇒ 口径坏了）。
    let tracked: Vec<String> = tracked_files();

    // ── 抽 `doc/` 里反引号包着、**带目录**的路径
    let mut refs: Vec<(String, usize, String)> = Vec::new();
    for p in doc_files() {
        let fname = p
            .file_name()
            .expect("doc 文件名")
            .to_string_lossy()
            .to_string();
        let text = std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("读 {p:?} 失败: {e}"));
        for (i, line) in text.lines().enumerate() {
            for chunk in line.split('`').skip(1).step_by(2) {
                let c = chunk.trim();
                if !c.contains('/') || c.contains(' ') || c.contains("::") {
                    continue;
                }
                let Some(ext) = c.rsplit('.').next() else {
                    continue;
                };
                if !EXTS.contains(&ext) || c.starts_with("http") {
                    continue;
                }
                if !c
                    .chars()
                    .all(|ch| ch.is_ascii_alphanumeric() || "_./-".contains(ch))
                {
                    continue;
                }
                refs.push((fname.clone(), i + 1, c.to_string()));
            }
        }
    }
    // ★ 自检 2：数量地板 **+ 锚点**。
    //
    // ⚠ 只有数量地板是**不够**的，这一条是变异当场量出来的：把「带目录才算」那个条件反过来
    // （于是收的是 `lib.rs` 这类**不带目录**的名字），`refs.len()` 照样过 90 ——
    // **地板对「收的是不是同一类东西」完全是瞎的**，它只数个数。
    // 补一个必须在场的锚点，人群换了就当场红。
    assert!(
        refs.len() >= 90,
        "`doc/` 里只抽到 {} 处带目录的路径引用 —— 剥法坏了（建判据当天实测 119 处）",
        refs.len()
    );
    const CANARY: &str = "src/session-backend.ts";
    assert!(
        refs.iter().any(|(_, _, c)| c == CANARY),
        "抽到了 {} 条，但**锚点 `{CANARY}` 不在里面** —— 收的多半不是「带目录的仓内路径」这一类了。\n\
             （数量地板只数个数，换一群东西照样能喂饱它。）",
        refs.len()
    );

    // `doc/` 在仓根下一层 ⇒ 文中的 `../../README.md` 说的就是仓根的 `src/README.md`。
    // 不归一化就会把五处**完全正确**的相对写法判成腐 —— 判据误报比漏报更快被人关掉。
    let resolves = |c: &str| {
        let c = c.trim_start_matches("../");
        tracked
            .iter()
            .any(|t| t == c || t.ends_with(&format!("/{c}")))
    };

    // ★ 自检 3：例外表保鲜 —— 例外是欠账不是免检章。
    for (path, why) in EXCEPTIONS {
        assert!(
            refs.iter().any(|(_, _, c)| c == path),
            "例外表里的 `{path}` 在 `doc/` 里已经没人写了 —— 删掉这一行。（当初的理由：{why}）"
        );
        assert!(
            !resolves(path),
            "例外 `{path}` 现在**解析得到了** —— 删掉这条例外，别让例外表替真判据挡枪。\n\
                 （当初的理由：{why}）"
        );
    }

    let bad: Vec<String> = refs
        .iter()
        .filter(|(_, _, c)| !EXCEPTIONS.iter().any(|(e, _)| e == c))
        .filter(|(_, _, c)| !resolves(c))
        .map(|(f, ln, c)| format!("  doc/{f}:{ln}  `{c}`"))
        .collect();
    assert!(
        bad.is_empty(),
        "`doc/` 点名了这些仓内路径，而 `git ls-files` 里找不到（含后缀匹配）：\n{}\n\n\
             ⚠ 同 `file.rs::symbol` 那条：**搬家 / 改名 / 删除都让它变假，而此前没有东西会红**。\n\
             修法（E12）：① 把路径改对；② 若那句只是历史或示例，写清楚并进例外表（带理由）。",
        bad.join("\n")
    );
}

/// **发版版本号六处必须一致**（`package.json` 是权威源，其余对拍）。
///
/// **为什么建它**：`src/doc/RELEASING.md` 自己逐字记着 ——
/// 「v3.1→v3.4 **连续四次**发版漏改 README，于是 README 的『当前版本』长期落后一个大版本；
/// BACKLOG 早把『checklist 里没有 README 这一条』点名为**机制性根因**，
/// 而根因没修 ⇒ 第四次照样复发」。
///
/// 那次的修法是**在 checklist 里加一行散文**。散文接不住它：第四次复发时 checklist 已经在了。
/// ⇒ 这正是 E12 ①「送进一条会红的判据」该管的形状，也是 E3 的标准解
/// （一个事实六个副本 ⇒ 定权威源 + 其余对拍）。
///
/// ⚠ 本条**故意在发版中途也会红**：改了 `package.json` 而 README 还没跟上时它就红 ——
/// 那不是误报，那是它的岗位（`RELEASING` 的 checklist 要求这几处一起改）。
///
/// ⚠ 建判据当天六处全部是 `3.6.0`，**一处不差** —— 又是「今天干净、但没人守着」：
/// 这个位置**已经腐过四次**，靠的是人记得，不是机制。
///
/// # 与 `release.yml` 那道 guard 的关系（E3：别造第二个权威源）
///
/// 建完本条的**第二天**才查到：`release.yml` 里早有一道
/// 「Verify version consistency with tag」，查**四处**（`package.json` / `tauri.conf.json` /
/// `Cargo.toml` / **`Cargo.lock`**），CHANGELOG 记着它是「防 v2.4.2 漂移事故复发」加的。
///
/// 两者**不是一个事实两个权威源**，因为比的东西不同：
/// 那道 guard 的权威是 **git tag**（发版时「代码里的版本 == tag」），
/// 本条的权威是 `package.json`（任何时候「所有副本彼此一致」）。
///
/// ★ 但它给出一条更要紧的教训：**「已经有 guard」不等于「有信号」**。
/// `release.yml` 逐字写着 `on: push: tags: ['v*']` —— 而〔用 08-05〕已裁定不再 push
/// ⇒ 那道 guard **今天结构上一次都不会跑**。README 那四次复发也正是发生在
/// 「代码侧四处有人查、README 没人查」的缝里。
///
/// # 诚实边界：`Cargo.lock` **钉不进本条**（试过，是恒绿的）
///
/// 先把它当第七处加了进来，然后按 E11 造变异（把 lock 里 monitor 包改成 `3.5.0`）——
/// **测试照样绿**。查下去才明白：`cargo test` 启动时会先解析依赖，
/// **把 `Cargo.lock` 自动改回与 `Cargo.toml` 一致**（实测：变异后 `3.5.0` → 跑完 `3.6.0`，exit 0）。
/// ⇒ 任何住在 `cargo test` 里的判据都**看不见** lock 漂移：它在被观察之前就被治好了。
///
/// ⚠ 也不能改成读 `git show HEAD:` 那一份：那样「bump 版本」这个提交本身会被自己挡住
/// （提交前跑测试 → HEAD 里还是旧版本 → 红 → 提交不了），**造出一个解不开的死结**。
///
/// ⇒ 结论如实记着：lock 这一处**本地钉不住**，唯一能查它的是 `release.yml` 那道
/// PowerShell 步骤（它不经 cargo 读文件），而那道今天不会跑。这是一条**真的诚实边界**，
/// 不是「以后补」—— 记进。
///
/// ★ 它差一点就成了本仓最讨厌的那种东西：**一条永远不会红的判据**。
/// 逮住它的不是「测试失败」，是**变异之后诊断栏一个字都没有** —— 只看 exit code 会当它绿了。
/// **那道版本 guard 被 Linux job「继承」这件事，压在一条 `needs:` 边上**。
///
/// `release.yml` 的 `build-linux` 头上逐字写着为什么它串在 Windows 之后：
///
/// > `build-windows` 里那道**四处版本号与 tag 一致**的检查因此**被继承** —— 版本漂了
/// > 先失败，本 job 根本不会起。**不重复实现那道检查**（重复 = 又一个会漂的副本）。
///
/// 那是一条**正确的 E3 决定**（别造第二个权威源），而它的正确性**整个压在
/// `needs: [build-backends, build-windows]` 这一行上**。谁为了「发版快一点」把
/// `build-windows` 从 needs 里摘掉，两件事同时发生，且都不会有人说话：
///
/// 1. **`.deb` 的版本再没人查** —— 那道 guard 正是「防 v2.4.2 漂移事故复发」加的；
/// 2. 两个 job 会**同时** `action-gh-release`，竞争同一个 release（那正是当初串起来的理由 ①）。
///
/// ⇒ 本条钉两件：那条边还在 · 那道 guard**仍然只有一处**（没被人「顺手也加到 Linux」，
/// 那会变成第二个会漂的副本，正是上面那段论证要避免的）。
///
/// ⚠ 与上一条的分工：上一条比的是**六处副本彼此一致**（权威是 `package.json`），
/// 本条不看版本号，只看**那道以 tag 为权威的检查还罩不罩得住 Linux 产物**。
#[test]
fn the_linux_job_still_inherits_the_version_guard() {
    let rel = guard_core::strip_hash_comment_lines(
        &std::fs::read_to_string(
            crate::guard_support::repo_root().join(".github/workflows/release.yml"),
        )
        .expect("读不到 release.yml"),
    );
    let lines: Vec<&str> = rel.lines().collect();
    let at = lines
        .iter()
        .position(|l| l.trim_end() == "  build-linux:")
        .unwrap_or_else(|| panic!("`release.yml` 里找不到 `build-linux:` job —— job 名变了或它被删了，本条会零命中地绿"));
    // `needs:` 必须在这个 job 的头部（`steps:` 之前）——不然读到的是别人的。
    let head_end = lines[at..]
        .iter()
        .position(|l| l.trim() == "steps:")
        .unwrap_or_else(|| panic!("`build-linux` 里找不到 `steps:` —— 段界读法坏了"));
    let head = lines[at..at + head_end].join("\n");
    // ⚠ 不用裸 `contains`：`needle_anchor` 棘轮当场把本条判为「语料上的裸匹配」（33→35），
    // 而它是对的 —— 「文件里某处有 `needs:`、某处有 `build-windows`」和
    // 「**那条 needs 上有 build-windows**」是两回事（前者被两行毫不相干的字就满足了）。
    // 改成：先取出那一条 `needs:` 行，再在**那一行**里按词匹配。
    let needs_line = lines[at..at + head_end]
        .iter()
        .find(|l| l.trim_start().starts_with("needs:"))
        .copied()
        .unwrap_or("");
    assert!(
        guard_core::contains_word(needs_line, "build-windows"),
        "`build-linux` 不再依赖 `build-windows` 了。它的头部现在是：\n{head}\n\n\
             ★ 两件事同时发生，且都不会有人说话：\n\
             1. **`.deb` 的版本再没人查** —— 那道「四处版本号与 tag 一致」的检查只住在 \n\
                `build-windows` 里，而 `build-linux` 头注逐字写着「因此被继承 …… \n\
                **不重复实现那道检查**（重复 = 又一个会漂的副本）」。那道 guard 是\n\
                「防 v2.4.2 漂移事故复发」加的。\n\
             2. 两个 job 会**同时** `action-gh-release`，竞争同一个 release —— \n\
                那正是当初把它们串起来的理由 ①。\n\
             ⇒ 真要并行，就得先解决这两件（比如把版本检查提成独立 job 让两边都 needs 它），\n\
             而不是只删这条边。"
    );

    // 那道 guard 仍然**只有一处**：既没被删，也没被「顺手也加到 Linux」。
    let guard_steps = lines
        .iter()
        .filter(|l| guard_core::contains_word(l, "Verify version consistency with tag"))
        .count();
    assert_eq!(
        guard_steps, 1,
        "「Verify version consistency with tag」这道步骤在 `release.yml` 里出现 {guard_steps} 次（应为 1）。\n\
             0 次 = 它被删了（那道 guard 是防 v2.4.2 漂移事故复发的，删之前先说清谁接）；\n\
             ≥2 次 = 有人在 Linux 那边**重复实现**了它 —— 那正是 `build-linux` 头注逐字反对的\n\
             「又一个会漂的副本」（E3）。真要两边都查，就把它提成一个独立 job。"
    );
}

#[test]
fn the_release_version_is_the_same_in_all_six_places() {
    /// 从 `hay` 里按 `needle` 抠出紧随其后的 `X.Y.Z`。
    /// **needle 必须恰好命中一次** —— 命中零次（那行被改写）或多次（抠错地方）都当场红，
    /// 否则这条判据会在「读不到东西」的时候安静地绿。
    fn pick(who: &str, hay: &str, needle: &str) -> String {
        let n = hay.matches(needle).count();
        assert_eq!(
            n, 1,
            "在 {who} 里，锚点 {needle:?} 命中 {n} 次（要求恰好 1 次）——\n\
                 那一行被改写或挪走了。**先修锚点再谈版本对不对**，否则本条会零命中地绿。"
        );
        let at = hay.find(needle).expect("上面已断言命中一次") + needle.len();
        let rest = &hay[at..];
        let end = rest
            .find(|c: char| !(c.is_ascii_digit() || c == '.'))
            .unwrap_or(rest.len());
        let v = &rest[..end];
        assert!(
            v.split('.').count() == 3 && v.split('.').all(|s| !s.is_empty()),
            "{who} 在锚点之后抠到的是 {v:?} —— 形状不像 `X.Y.Z`"
        );
        v.to_string()
    }

    let root = repo_root();
    let rd = |p: &str| {
        std::fs::read_to_string(root.join(p)).unwrap_or_else(|e| panic!("读 {p} 失败: {e}"))
    };
    let (pkg, cargo, conf, readme, readme_en) = (
        rd("package.json"),
        rd("src/frontend/shell/Cargo.toml"),
        rd("src/frontend/shell/tauri.conf.json"),
        rd("README.md"),
        rd("README.en.md"),
    );

    // 权威源（E3）：npm 包清单。其余五处都只是它的副本。
    let authority = pick("package.json", &pkg, "\n  \"version\": \"");
    let others: [(&str, String); 5] = [
        (
            "src/frontend/shell/Cargo.toml",
            pick("src/frontend/shell/Cargo.toml", &cargo, "\nversion = \""),
        ),
        (
            "src/frontend/shell/tauri.conf.json",
            pick(
                "src/frontend/shell/tauri.conf.json",
                &conf,
                "\n  \"version\": \"",
            ),
        ),
        (
            "README.md 抬头那行",
            pick("README.md 抬头那行", &readme, "当前版本: v"),
        ),
        (
            "README.md 「项目当前状态」块",
            pick("README.md 「项目当前状态」块", &readme, "- **版本**：v"),
        ),
        (
            "README.en.md 抬头那行",
            pick("README.en.md 抬头那行", &readme_en, "| Current: v"),
        ),
    ];

    let off: Vec<String> = others
        .iter()
        .filter(|(_, v)| *v != authority)
        .map(|(who, v)| format!("  {who}：{v}"))
        .collect();
    assert!(
        off.is_empty(),
        "版本号对不上。权威源 `package.json` = {authority}，而这几处是别的数：\n{}\n\n\
             ⚠ 这个位置**已经连续腐过四次**（v3.1→v3.4 每次发版都漏改 README，\n\
             `src/doc/RELEASING.md` 自己记着这件事）。当时的修法是往 checklist 里加一行散文，\n\
             而第四次复发时那行散文已经在了 —— 所以现在由本条判据接着。\n\
             修法：把落后的那几处改成 {authority}（`RELEASING.md § 1` 的 checklist 列了全部落点）。",
        off.join("\n")
    );
}

/// 读仓根的一份文本。
///
/// ⚠ **刻意包成函数，不在 `let` 右边直接写 `read_to_string`**：
/// `needle_anchor_registry::corpus_vars` 按「`let X = …read_to_string(…)`」播种
/// 「语料变量」，而它的传递闭包**按名字**跑一层 —— 在本文件里多播一个名字出去，
/// 会把同文件别处**早就存在**的匹配一起卷进人群，那条递减棘轮当场涨一格。
/// 〔与 `frozen_backend_census::read_frozen` 那条头注同源，09-14 实打过一次〕
fn read_repo_file(rel: &str) -> String {
    std::fs::read_to_string(repo_root().join(rel)).unwrap_or_else(|e| panic!("读 {rel} 失败：{e}"))
}

/// 本仓 breaking 段的**约定名** —— `CHANGELOG.md` 里标题带这个词的那个 `###` 就是它。
///
/// 〔现打于 `track/k-r120` 交回那一刻，量法
/// `grep -c '^### .*会改变已有行为' CHANGELOG.md` = **2**（`3.8.0` 与 `3.7.0` 各一处）。
/// **这个数不进判据** —— 下面只有一条 `≥ 1` 的地板，它挡的是
/// 「这个约定被整份抹掉、而『不许被埋』那条从此零命中地绿」。〕
const BREAKING_MARK: &str = "会改变已有行为";

/// 〔`K-R120` `KR120D2`，09-14〕**`CHANGELOG.md` 最上面那一节的版本号，
/// 必须就是这棵树此刻要发的那个版本号。**
///
/// # 它从哪来 —— 一条**现打出来**的缺口，不是设想
///
/// `K-R118` 的死值验第 ⑦ 刀（刀具住 `tests/evidence/K-R118-cut.py` 的 `d7`）：
/// 把版本号那六处 ＋ `src/frontend/shell/Cargo.lock` **一起** bump，而 `CHANGELOG.md`
/// 一个字不动 ⇒ 实测 `GATE: OK —— 16 格全绿，一条都没红`
/// （逐字读数住 `tests/evidence/K-R118-deathvalue.md#§E3`，本条不抄那份快照）。
/// ⇒ 「**版本号 bump 了而 CHANGELOG 没跟**」这一形当时**没有任何东西在守**。
/// 它的后果正是本区最贵的那一族：一次**静默的行为改变** —— 用户拿到的包只涨了小版本号，
/// 而里面有几条会让他原来的用法当场失效。
///
/// # 🔴 判的**不是**「文件里有没有出现这个版本号串」
///
/// 那种判法一个字都买不到：版本号写在这份文件的**任何地方**（一句散文、一条旧条目、
/// 甚至一段注释）都能骗过它。本条判的是**最上面那一节标题里的那个版本号**。
///
/// # 为什么只跟**一处**比，而不是把七处都读一遍
///
/// 「七处」今天已经各有各的家，本条只接最后那一段，**不再复述一份锚点表**
/// （`brief` 13b：闭集只许有一个住址）：
///
/// | 谁 ↔ 谁 | 由谁守 |
/// |---|---|
/// | 权威源 `package.json` ↔ 另外四处（`Cargo.toml` · `tauri.conf.json` · `README.md` ×2 · `README.en.md`） | 同模块的 [`the_release_version_is_the_same_in_all_six_places`] |
/// | `src/frontend/shell/Cargo.toml` ↔ `src/frontend/shell/Cargo.lock` | 门禁 `winchk` 那一格的 `cargo check --locked`；发版路上另有 `release.yml` 的 `Verify version consistency with tag`（四处对账） |
/// | **`CHANGELOG.md` 最上一节 ↔ `src/frontend/shell/Cargo.toml`** | **本条**（`env!("CARGO_PKG_VERSION")`，编译期注入，不抠锚点） |
///
/// ⇒ **三段接起来**才等于「最上一节 == 那七处」。少任何一段都不等于 ——
/// 上面那两行不是背景，是本条结论的**承重件**。
///
/// # 第二条判定：breaking 段不许被埋在列表里
///
/// `R77` 裁的是「既然不走 `4.0.0`，那几条破坏性变更**必须写成显眼的 breaking 段**」——
/// 而「显眼」在机器面上唯一判得动的那一半是**位置**：最上面那一节里若有 breaking 段，
/// 它必须是**第一个** `###`。「**藏在列表里**」正是那条裁定点名要避开的形状。
///
/// # ⚠ 诚实边界（三条，别读宽）
///
/// 1. 🔴 **「该不该有 breaking 段」本条判不了，也不判** —— 机器分不出「这一版真的没有
///    破坏性变更」与「有而没写」。硬要求每一节都有，只会把它变成谁都会写的一句空话，
///    而 `references/writing.md` 第三节逐字反对这一形（那种闸的真阳率压不住噪声）。
///    ⇒ **这一档登记为「不在射程」，不是「做到了」。**
/// 2. breaking 段靠 [`BREAKING_MARK`] 这个**约定词**认。换一种说法另起一节 ⇒ 本条静默。
///    挡这一形的是下面那条**地板**（全文至少一处），它只保证「这个约定没被整份抹掉」，
///    **不保证最上面那一节里那一处还在**。
/// 3. 本条只读 `CHANGELOG.md` 一份文件 ＋ 一个编译期常量。那一节里**写的内容对不对、
///    全不全**（六条是不是真的六条、有没有漏掉一条）一个字都不判 ——
///    那是人裁的，`K-R118` 交回时逐字写过「**用户可见**那一层给不出判别式」。
#[test]
fn the_changelog_top_section_is_the_version_we_ship() {
    /// 段界记号。**一律走具名常量** —— `needle_anchor_registry` 那条递减棘轮数的正是
    /// 「拿磁盘语料做裸字面量匹配」，本条一处都不往上加。
    const TOP_MARK: &str = "## ";
    /// 版本节的标题形状：`## [X.Y.Z] — 日期`。
    const SECTION_MARK: &str = "## [";
    /// 节内子标题。
    const SUB_MARK: &str = "### ";

    // 🔴 **要发的那个版本号从 `src/frontend/shell/Cargo.toml` 编译期注入**，本条不自己再抠一遍锚点
    //    —— 同一个值不许长出第二个住址（`brief` 13b）。
    let shipping = env!("CARGO_PKG_VERSION");
    let changelog = read_repo_file("CHANGELOG.md");

    let heading = changelog
        .lines()
        .find(|l| l.starts_with(SECTION_MARK))
        .unwrap_or_else(|| {
            panic!(
                "`CHANGELOG.md` 里一行 `{SECTION_MARK}…` 都找不到 —— 节标题的写法变了，\n\
                     本条从此零命中地绿。**先修段界读法，再谈版本号对不对。**"
            )
        });

    let top_version = heading
        .trim_start_matches('#')
        .trim()
        .strip_prefix('[')
        .and_then(|rest| rest.split_once(']'))
        .map(|(v, _)| v)
        .unwrap_or_else(|| panic!("最上面那一节的标题抠不出 `[…]`，它逐字是：{heading}"));

    assert_eq!(
        top_version, shipping,
        "`CHANGELOG.md` 最上面那一节写的是 `{top_version}`，而这棵树要发的是 `{shipping}`。\n\n\
             ★ 本条接的是 `K-R118` `d7` 那一刀现打出来的缺口：那一刀把版本号七处一起 bump、\n\
             `CHANGELOG.md` 一个字不动 ⇒ 当时 **16 格全绿，一条都没红**。\n\
             「小版本号 ＋ 没人说的破坏性变更」= 一次静默的行为改变，那是本区最贵的一族。\n\n\
             出路二选一（**不是**「把这一条放宽」）：\n\
             ① 版本号真的要 bump ⇒ 在 `CHANGELOG.md` 顶上补 `## [{shipping}] — <日期>` 那一节，\n\
                破坏性变更写在**最前**（`R77`）；\n\
             ② 版本号 bump 错了 ⇒ 改回去，七处一起\n\
                （另外五处由 `the_release_version_is_the_same_in_all_six_places` 看着，\n\
                 `Cargo.lock` 由 `cargo check --locked` 看着）。\n\n\
             ⚠ **这条前提本来就该变的时候去哪里重裁**：顶上挂一个 `## [Unreleased]` 会让本条红。\n\
             本仓至今没用过那种写法（所以这里没有那一档豁免，也就没有一条没夹具的分支）；\n\
             真要用，去 `src/doc/RELEASING.md` 把发版次序整个重裁一次 —— 别在这里加一行豁免。"
    );

    // ── 第二条判定：breaking 段不许被埋在列表里 ──────────────────────────
    let body: Vec<&str> = changelog
        .lines()
        .skip_while(|l| !l.starts_with(SECTION_MARK))
        .skip(1)
        .take_while(|l| !l.starts_with(TOP_MARK))
        .collect();
    // 抽取器自检：段界真的切到了东西，下面两条不是在空转。
    assert!(
        !body.is_empty(),
        "最上面那一节 `{heading}` 的正文是空的 —— 段界读法坏了，下面两条此刻在空转"
    );

    // 地板（反空真）：breaking 段那个约定词在整份 `CHANGELOG.md` 里至少还有一处。
    let mark_lines = changelog
        .lines()
        .filter(|l| l.starts_with(SUB_MARK) && l.contains(BREAKING_MARK))
        .count();
    assert!(
        mark_lines >= 1,
        "整份 `CHANGELOG.md` 里一条带 {BREAKING_MARK:?} 的 `{SUB_MARK}` 标题都没有 ——\n\
             breaking 段的**约定名**被换掉了，下面那条「不许被埋」从此零命中地绿。\n\
             换写法可以，但要同一拍把 `BREAKING_MARK` 改过来。"
    );

    let subs: Vec<&str> = body
        .iter()
        .copied()
        .filter(|l| l.starts_with(SUB_MARK))
        .collect();
    if let Some(at) = subs.iter().position(|l| l.contains(BREAKING_MARK)) {
        assert_eq!(
            at,
            0,
            "`{heading}` 这一节里，breaking 段排在第 {} 个 `{SUB_MARK}`，不是第一个。\n\n\
                 ★ `R77` 裁的是「既然不走大版本号，那几条破坏性变更**必须写成显眼的 breaking 段**」\n\
                 —— 而「小版本号 ＋ **藏在列表里**的破坏性变更」正是那条裁定点名要避开的形状。\n\
                 这一节现在的子标题顺序是：\n{}",
            at + 1,
            subs.iter()
                .enumerate()
                .map(|(i, l)| format!("  {}. {l}", i + 1))
                .collect::<Vec<_>>()
                .join("\n")
        );
    }
}

/// 〔`K-R120` 收窗口补，09-14〕**两份 README 里「这份文档此刻自称的版本」那一处，
/// 必须就是这棵树此刻要发的那个版本号。**
///
/// # 它从哪来 —— 收窗口现打逮到的漏，而漏在**人群**上，不在实现上
///
/// 本件按 `KR120D1` 把「七处」一次改齐到 `3.8.0` 之后，收窗口现打逮到
/// `README.md` 与 `README.en.md` 的发布沿革段抬头仍写着 `当前发布 **v3.7.0**` /
/// `current release **v3.7.0**` —— **这棵树会带着「当前发布 v3.7.0」把 3.8.0 发出去。**
///
/// 🔴 **成因不是「有人改漏了」，是「人群里根本没有它」**：那一件的「七处」＝
/// [`the_release_version_is_the_same_in_all_six_places`] 数得到的那几处 ＋ `Cargo.lock`，
/// 而现打 `grep -c '当前发布\|current release' src/frontend/shell/src/doc_claim_registry.rs`
/// **零命中** —— **判据在，而它的人群不含这一处**。
/// ★ 这与 `R73` 第五节登记的「判据在、执行面没有」是**同一族的镜像**：执行面在，人群不够。
/// 而它的默认结局一样：**静默的绿**。
///
/// # 🔴 射程刻意很窄：判「此刻自称的版本」，不判「文档里出现过的所有版本号」
///
/// 那两行是**发布沿革**段 —— 现打各含 **11** 个形如 `vX.Y.Z` 的串
/// （`v2.19.0` `v2.19.1` `v2.20.0` `v2.21.0` `v2.22.0` `v2.22.2` `v3.3.0` `v3.4.0`
/// `v3.5.0` `v3.6.0` ＋ 自称的那一个），其中 **10 个是历史沿革，本来就该停在旧号上**。
/// ⇒ 本条只钉**紧跟在那两个锚点之后**的那一个。
/// **把整段收进人群 = 下次发版红一片**，那不是守，是拆（`testing.md` 判据硬规则 4：
/// 扫描面按**语义**划，不按「碰巧只有它长这样」划）。
/// 同理，`README.md` 与 `README.en.md` 里那两句「v3.6.0 与 v3.7.0 的实际产物是 `en-US`」
/// 是 09-10 落的**历史订正**，**刻意不在射程里**。
///
/// # 跟谁比
///
/// 与 [`the_changelog_top_section_is_the_version_we_ship`] 同一条路：
/// `env!("CARGO_PKG_VERSION")`（＝ `src/frontend/shell/Cargo.toml` 的 `version`，编译期注入）——
/// **不抠第二份权威源锚点**（`brief` 13b）。它与另外五处的一致由
/// [`the_release_version_is_the_same_in_all_six_places`] 守，与 `Cargo.lock` 的一致由
/// 门禁 `winchk` 那一格的 `cargo check --locked` 守。
///
/// # ⚠ 诚实边界（三条）
///
/// 1. 人群是**两处，按锚点点名**。README 里别处再长出第三句「当前发布 …」，本条看不见 ——
///    它判的是**这两个锚点**，不是「所有自称」。
/// 2. 锚点里带着 markdown 的 `**` ⇒ 排版一改（比如去掉加粗），本条**当场红在
///    「锚点命中 0 次」上**，而不是静默地绿。这是有意的，与那条「六处一致」的 `pick`
///    同一条纪律：命中必须恰好 1 次。
/// 3. 本条**不判那一整段散文对不对**（沿革列得全不全、里面的话有没有过期），一个字都不判。
#[test]
fn the_docs_self_reported_release_is_the_version_we_ship() {
    let shipping = env!("CARGO_PKG_VERSION");

    // 人群：**两处，按锚点点名**。
    // ⚠ 刻意写成函数体里的 `let`，不是模块级 `const …: &[…]` ——
    //   后者要起 `scanning_guard_registry::TABLE_DECLS` 里**已有的**名字（那条元判据
    //   按名字认表），而往那张闭集里加一个新名字必须同拍改 `MUST_BE_RECOGNISED`，
    //   两处都不在本件写区。同形先例就在上面那条「六处一致」里（它的 `others` 也是
    //   函数体里的 `let`）。
    let places: [(&str, &str, &str); 2] = [
        ("README.md 发布沿革段抬头", "README.md", "当前发布 **v"),
        (
            "README.en.md 发布沿革段抬头",
            "README.en.md",
            "current release **v",
        ),
    ];

    let mut off: Vec<String> = Vec::new();
    for (who, file, needle) in places {
        let doc = read_repo_file(file);
        let hits = doc.matches(needle).count();
        assert_eq!(
            hits, 1,
            "在 {who}（`{file}`）里，锚点 {needle:?} 命中 {hits} 次（要求恰好 1 次）——\n\
                 那一行被改写、被挪走，或者排版变了（锚点里带着 markdown 的 `**`）。\n\
                 **先修锚点再谈版本号对不对**，否则本条会零命中地绿。"
        );
        let at = doc.find(needle).expect("上面已断言命中一次") + needle.len();
        let rest = &doc[at..];
        let end = rest
            .find(|c: char| !(c.is_ascii_digit() || c == '.'))
            .unwrap_or(rest.len());
        let got = &rest[..end];
        assert!(
            got.split('.').count() == 3 && got.split('.').all(|s| !s.is_empty()),
            "{who} 在锚点之后抠到的是 {got:?} —— 形状不像 `X.Y.Z`"
        );
        if got != shipping {
            off.push(format!("  {who}（`{file}`）：{got}"));
        }
    }

    assert!(
        off.is_empty(),
        "这棵树要发的是 `{shipping}`，而这几处文档**自称**的是别的号：\n{}\n\n\
             ★ 本条是 `K-R120` 收窗口现打逮到的那个漏的处置：那一拍七处都已经是新号，\n\
             而这两处**不在任何判据的人群里** ⇒ 这棵树会带着「当前发布 <旧号>」把新版发出去。\n\
             「判据在、而它的人群不含这一处」与「判据在、执行面没有」是同一族，\n\
             两边的默认结局都是**静默的绿**。\n\n\
             修法：把上面点名的那几处改成 {shipping}。\n\
             ⚠ **只改紧跟锚点的那一个** —— 同一段里另外十个 `vX.Y.Z` 是**历史沿革**，\n\
             它们本来就该停在旧号上，跟着改就是把沿革改成假的。",
        off.join("\n")
    );
}

/// 〔`K-R122`（09-14）`KR122D3`〕**`package-lock.json` 自称的那个版本，
/// 必须就是这棵树此刻要发的那个版本号。**
///
/// # 它从哪来 —— `K-R119` 推 tag 之前现打逮到的第十、十一处旧号
///
/// `K-R119` 在推 `v3.8.0` 之前逐处 grep 了一遍（不是「判据绿了」，是真去看那几行），
/// 七处版本号 ＋ `R79` 那两处 README 自称全是 `3.8.0`，而 `package-lock.json`
/// 的**顶层两处**仍是 `3.7.0`。读数住 `tests/evidence/K-R119-发版读数.md § 四`。
///
/// 🔴 **成因与 `K-R120` 那两处 README 同源，不是「有人改漏了」**：
/// 那两处**不在任何判据的人群里** —— 现打 `grep -c 'package-lock' src/frontend/shell/src/` 在本条
/// 落地之前是 **0**。`src/doc/RELEASING.md § 1` 自己逐字记着这一条「**没有任何东西卡它**」。
/// ⇒ 「判据在、而它的人群不含这一处」，默认结局是**静默的绿**。
///
/// # 🔴 射程刻意很窄：只钉**顶层那两处**，不钉几百个依赖的 `version`
///
/// 这份文件里 `"version": "` 这个串现打有 **790** 处 —— 其中 **788** 处是**依赖自己的
/// 版本**，它们跟本包的版本号一点关系都没有，跟着改就是把 lockfile 改成假的。
/// 本条只钉 npm 自己写的那两处「**这个包是谁、什么版本**」：
///   ① 文件顶层的 `version`（紧跟顶层 `name` 那一个）；
///   ② `packages` 里 `""` 这个键（npm 用它表示**根包自己**）底下的 `version`。
/// ⇒ 锚点按**语义**划，不按「碰巧只有它长这样」划（`references/testing.md` 判据硬规则 4）。
///
/// # 跟谁比
///
/// 与 [`the_changelog_top_section_is_the_version_we_ship`] 和
/// [`the_docs_self_reported_release_is_the_version_we_ship`] 同一条路：
/// `env!("CARGO_PKG_VERSION")`（＝ `src/frontend/shell/Cargo.toml` 的 `version`，编译期注入），
/// **不抠第二份权威源锚点**（`brief` 13b）。它与 `package.json` 那个权威源的一致由
/// [`the_release_version_is_the_same_in_all_six_places`] 守。
/// ⇒ **三段接起来**才等于「lockfile == `package.json`」；少任何一段都不等于。
///
/// # ⚠ 诚实边界（四条，别读宽）
///
/// 1. **不判 lockfile 的其余任何一个字节** —— 依赖树对不对、`integrity` 对不对、
///    与 `package.json` 的依赖区间合不合，本条一个字都不问（那是 `npm ci` 的事）。
/// 2. 人群是**两处，按锚点点名**。npm 换一种排版（缩进变了 / 键序变了）⇒ 本条**当场红在
///    「锚点命中 0 次」上**，而不是静默地绿。这是有意的，与本模块另外两条 `pick` 同一条纪律。
/// 3. 锚点 ② 里带着包名 `cc-monitor`。改包名 ⇒ 本条红在命中 0 次上，
///    **那正是该有人看一眼的时刻**（改包名要同拍改 `package.json`）。
/// 4. 🔴 **它不会让构建红，这正是它当初漏掉的原因** —— `K-R119` 那趟演练里
///    `npm ci` 与 `npm install` 两步都 success（读数同上）。lockfile 里这个号是
///    「**这棵树自称的版本**」的一处，不是构建的输入 ⇒ 没有第二个机制会替它出声。
#[test]
fn the_npm_lockfile_claims_the_version_we_ship() {
    let shipping = env!("CARGO_PKG_VERSION");
    let lock = read_repo_file("package-lock.json");

    // 人群：**两处，按锚点点名**。刻意写成函数体里的 `let`（理由同上一条：
    // 模块级 `const …: &[…]` 要进 `scanning_guard_registry::TABLE_DECLS` 那张闭集）。
    let places: [(&str, &str); 2] = [
        ("package-lock.json 顶层的 version", "\n  \"version\": \""),
        (
            "package-lock.json 的 packages[\"\"]（npm 用它表示根包自己）",
            "\n    \"\": {\n      \"name\": \"cc-monitor\",\n      \"version\": \"",
        ),
    ];

    let mut off: Vec<String> = Vec::new();
    for (who, needle) in places {
        let hits = lock.matches(needle).count();
        assert_eq!(
            hits, 1,
            "在 {who} 里，锚点 {needle:?} 命中 {hits} 次（要求恰好 1 次）——\n\
                 npm 换了排版、或者包名改了。**先修锚点再谈版本号对不对**，\n\
                 否则本条会零命中地绿（这份文件里另外那几百个 `version` 字段是依赖的，\n\
                 锚点一松就会抠到它们身上）。"
        );
        let at = lock.find(needle).expect("上面已断言命中一次") + needle.len();
        let rest = &lock[at..];
        let end = rest
            .find(|c: char| !(c.is_ascii_digit() || c == '.'))
            .unwrap_or(rest.len());
        let got = &rest[..end];
        assert!(
            got.split('.').count() == 3 && got.split('.').all(|s| !s.is_empty()),
            "{who} 在锚点之后抠到的是 {got:?} —— 形状不像 `X.Y.Z`"
        );
        if got != shipping {
            off.push(format!("  {who}：{got}"));
        }
    }

    assert!(
        off.is_empty(),
        "这棵树要发的是 `{shipping}`，而 `package-lock.json` **自称**的是别的号：\n{}\n\n\
             ★ 本条是 `K-R119` 推 tag 之前现打逮到的那个漏的处置：那一拍七处版本号 ＋ 两处 README\n\
             自称都已经是新号，而这两处**不在任何判据的人群里** ⇒ 这棵树会带着一个旧号的 lockfile 发版。\n\
             ⚠ 它**不会**让 `npm ci` 红（`K-R119` 演练实测两步都 success）—— 所以没有第二个机制\n\
             会替它出声，只有本条。\n\n\
             修法：把 `package-lock.json` **顶层那两处**改成 {shipping}。\n\
             ⚠ **只改那两处** —— 同一份文件里另外几百个 `version` 是**依赖自己的版本**，\n\
             跟着改就是把 lockfile 改成假的。",
        off.join("\n")
    );
}

/// **文档里写成 `CONST = 数` 的，代码里那个常量必须真是这个数。**
///
/// **这是定框 E12 自己点名的洞**：E12 的 ⚠ 逐字写着「那四个准确的细节数
/// （`CHUNK_SIZE=600` 等）**一个都不在它的扫描面里**」—— 本模块此前只管
/// 「状态列」与几种极窄形态，`CONST = 数` 这一族**没人读**。
///
/// **变异实证（先红后信的反面：它当时是绿的）**：把后端生产常量
/// `REPLY_BURST` 从 8 改成 3 —— backend 全套 + monitor 全套**都绿**，
/// 而 `IPC-PROTOCOL.md` 逐字写着「`main.rs::REPLY_BURST = 8`，连发 8 条后强制让位一次」。
/// 也就是说：**协议文档里的一个行为常量，代码改了不会有任何东西红。**
///
/// 手法沿用本模块的核心做法：**判据不自己写那个数**，从文档里抽出来再与代码比 ——
/// 于是那个数只有一个家（文档），改代码不改文档就红，改文档不改代码也红。
#[test]
fn every_constant_value_quoted_in_the_docs_matches_the_code() {
    /// 长得像常量、其实不是 Rust 常量的。逐条写清它是什么。
    const EXCEPTIONS: &[(&str, &str)] = &[
        (
            "CLAUDECODE",
            "**环境变量**（`CLAUDECODE=1`），不是 Rust 常量",
        ),
        ("CLAUDE_CODE_CHILD_SESSION", "同上，环境变量"),
    ];

    /// 从一行里抽 `NAME = 数`（大写标识符 ≥4 字符）。反引号可有可无。
    fn scan_line(line: &str) -> Vec<(String, String)> {
        let b: Vec<char> = line.chars().collect();
        let mut out = Vec::new();
        let mut i = 0usize;
        while i < b.len() {
            if !(b[i].is_ascii_uppercase()) {
                i += 1;
                continue;
            }
            let s = i;
            while i < b.len() && (b[i].is_ascii_uppercase() || b[i].is_ascii_digit() || b[i] == '_')
            {
                i += 1;
            }
            let name: String = b[s..i].iter().collect();
            if name.len() < 4 {
                continue;
            }
            let mut j = i;
            while j < b.len() && (b[j] == '`' || b[j] == ' ') {
                j += 1;
            }
            if j >= b.len() || (b[j] != '=' && b[j] != '：' && b[j] != ':') {
                continue;
            }
            j += 1;
            while j < b.len() && (b[j] == '`' || b[j] == ' ') {
                j += 1;
            }
            let vs = j;
            while j < b.len() && (b[j].is_ascii_digit() || b[j] == '_') {
                j += 1;
            }
            if j > vs {
                let v: String = b[vs..j].iter().filter(|c| **c != '_').collect();
                out.push((name, v));
            }
            i = j;
        }
        out
    }

    let mut claims: Vec<(String, usize, String, String)> = Vec::new();
    for p in doc_files() {
        let fname = p
            .file_name()
            .expect("doc 文件名")
            .to_string_lossy()
            .to_string();
        let text = std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("读 {p:?} 失败: {e}"));
        for (i, line) in text.lines().enumerate() {
            for (n, v) in scan_line(line) {
                claims.push((fname.clone(), i + 1, n, v));
            }
        }
    }
    // ★ 自检 1 + 锚点：只有数量地板不够（本会话实测过「人群被换掉、地板照样过」）。
    assert!(
        claims.len() >= 5,
        "`doc/` 里只抽到 {} 处 `CONST = 数` —— 剥法坏了（建判据当天实测 6 处）",
        claims.len()
    );
    // ⚠ 锚点**只认名字、不认值** —— 第一版把 `&& v == "8"` 也写进来了，
    // 于是判据自己成了那个数的第二份副本（正是本模块头注警告的形态）：
    // 合法地把常量改成别的数时，红的会是锚点而不是对拍，诊断指错方向。
    assert!(
        claims.iter().any(|(_, _, n, _)| n == "REPLY_BURST"),
        "锚点 `REPLY_BURST` 不在抽到的清单里 —— 收的多半不是这一类了。\n\
             （它是本条的立项样本：改代码不改文档时，全仓一条都不会红。）"
    );

    // ── 代码侧：常量名 → 它被定义成的那些值
    let mut defined: std::collections::BTreeMap<String, std::collections::BTreeSet<String>> =
        std::collections::BTreeMap::new();
    let mut srcs: Vec<(PathBuf, String)> = Vec::new();
    for root in ["src/frontend/shell/src", "src/common", "src/backend"] {
        srcs.extend(guard_core::scan_tree!(&repo_root().join(root), &["rs"]));
    }
    for (_, raw) in &srcs {
        for line in guard_core::strip_comment_lines(raw).lines() {
            let t = line.trim();
            let rest = match t
                .strip_prefix("const ")
                .or_else(|| t.strip_prefix("static "))
            {
                Some(r) => r,
                None => match t
                    .strip_prefix("pub const ")
                    .or_else(|| t.strip_prefix("pub static "))
                {
                    Some(r) => r,
                    None => continue,
                },
            };
            let Some((name, tail)) = rest.split_once(':') else {
                continue;
            };
            let name = name.trim();
            if name.is_empty()
                || !name
                    .chars()
                    .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
            {
                continue;
            }
            let Some((_, val)) = tail.split_once('=') else {
                continue;
            };
            let v: String = val
                .trim()
                .chars()
                .take_while(|c| c.is_ascii_digit() || *c == '_')
                .filter(|c| *c != '_')
                .collect();
            if !v.is_empty() {
                defined.entry(name.to_string()).or_default().insert(v);
            }
        }
    }
    // ★ 自检 2：代码侧一个常量都收不到 ⇒ 下面会把每条都判成「代码里没有」。
    assert!(
        defined.len() >= 20,
        "全仓只收到 {} 个数值常量定义 —— 剥法坏了",
        defined.len()
    );

    // ★ 自检 3：例外保鲜。
    for (name, why) in EXCEPTIONS {
        assert!(
            claims.iter().any(|(_, _, n, _)| n == name),
            "例外 `{name}` 在 `doc/` 里已经没人写了 —— 删掉这一行。（当初的理由：{why}）"
        );
        assert!(
            !defined.contains_key(*name),
            "例外 `{name}` 现在**真是一个 Rust 常量了** —— 删掉这条例外，让它进对拍。（当初的理由：{why}）"
        );
    }

    let bad: Vec<String> = claims
        .iter()
        .filter(|(_, _, n, _)| !EXCEPTIONS.iter().any(|(e, _)| e == n))
        .filter_map(|(f, ln, n, v)| match defined.get(n) {
            None => None, // 代码里没有同名常量：可能是别的语言/外部约定，不在本条管辖内
            Some(vs) if !vs.contains(v) => Some(format!(
                "  doc/{f}:{ln}  文档说 `{n} = {v}`，代码里实为 {:?}",
                vs.iter().collect::<Vec<_>>()
            )),
            _ => None,
        })
        .collect();
    assert!(
        bad.is_empty(),
        "文档写死的常量值与代码对不上：\n{}\n\n\
             ⚠ 立项样本就是这么溜掉的：`REPLY_BURST` 8→3，backend 与 monitor **两套全绿**。\n\
             修法（E12）：① 把文档改对；② 或者那句话本就不该写死数字 —— 改成指常量名。",
        bad.join("\n")
    );
}

/// **`scripts/` 里的每个文件都要在它自己的 README 里登记。**
///
/// # 逮到的是一条「找不到」的缺陷
///
/// 08-06 实测：`scripts/` 有三个脚本，而 `tests/scripts/README.md` 的表**只列了 `run.ps1`**。
/// 漏掉的两个里有 `verify-committed-state.sh` —— 它的头注逐字写着
/// 「本仓不 push ⇒ CI 从来没见过这些 commit，**所以这道门必须在本机跑**」。
/// ⇒ 一个「必须本机跑」的门，**照目录 README 是找不到的**；
/// 本会话是靠 grep 撞见它的，而那不是别人会走的路。
///
/// 这类缺陷不会让任何测试变红，也不会让任何人报错 —— 它只是让**下一个人找不到**。
/// 本条把「找得到」变成机检。
///
/// ⚠ 只钉**存在性**，不钉描述内容：描述会随脚本演进，钉了就是下一个假陈述
/// （本模块头注记着 `STATUS_CELLS` 那次教训）。要读细节去看脚本自己的头注。
#[test]
fn every_script_in_the_directory_is_listed_in_its_readme() {
    let dir = repo_root().join("tests/scripts");
    let readme =
        std::fs::read_to_string(dir.join("README.md")).expect("读不到 tests/scripts/README.md");
    // 人群：那个目录下跟踪着的文件（直接住在里面的那一层）。
    let mut files: Vec<String> = tracked_files()
        .into_iter()
        .filter_map(|rel| rel.strip_prefix("tests/scripts/").map(str::to_string))
        .filter(|n| !n.contains('/') && n != "README.md")
        .collect();
    files.sort();
    // ★ 抽取器自检：目录空了或读法坏了 ⇒ 下面会零命中地绿。
    assert!(
        files.len() >= 3,
        "`scripts/` 只扫到 {} 个文件（README 之外）—— 遍历坏了（08-06 实测 3 个）",
        files.len()
    );
    let missing: Vec<&String> = files
        .iter()
        .filter(|n| !readme.lines().any(|l| l.contains(n.as_str())))
        .collect();
    assert!(
        missing.is_empty(),
        "`scripts/` 里这些文件在 `tests/scripts/README.md` 里查不到：{missing:?}\n\n\
             ⚠ 这不会让任何测试变红，也不会让任何人报错 —— 它只是让**下一个人找不到**。\n\
             08-06 实测：那张表当时只有 `run.ps1`，于是照它找不到 `verify-committed-state.sh`，\n\
             而那是全仓**唯一量「提交状态」且必须在本机跑**的门。\n\
             ⇒ 加一行就行；细节写在脚本自己的头注里，别在 README 里抄第二份。"
    );
}

/// **开发者入口文档里的后端测试命令，必须与 `ci.yml` 逐字相同。**
///
/// # 逮到的是「照它做会少测」
///
/// `src/doc/DEVELOPMENT.md` 的「跑测试」节此前逐字写着
/// `cargo test --lib          # 全部单元测试` —— 而 `--lib` **只覆盖根包**，
/// 六个共享 crate 一条都不跑。新人照入口文档做，得到的是一个**少测**的读数，
/// 而它长得和全量读数一模一样（都是「ok. N passed」）。
///
/// 这一条属于本会话新命名的那类缺陷：**产物没错，通往它的路是错的**
/// —— 没有任何测试会因为文档里写错命令而变红。
///
/// # 手法：不在判据里写那个命令
///
/// 命令的**唯一的家是 `ci.yml`**。本条从两边各抽一次再比 ——
/// 于是改 CI 而不改文档会红，改文档而不改 CI 也会红，
/// 而判据自己**不持有第三份副本**（`STATUS_CELLS` 那次的教训，见本模块头注）。
#[test]
fn the_backend_test_command_in_the_docs_matches_ci() {
    let ci = std::fs::read_to_string(repo_root().join(".github/workflows/ci.yml"))
        .expect("读不到 ci.yml");
    // `rust` job 里那条 `cargo test …` —— 剔注释，只认真会跑的行。
    let cmd = ci
        .lines()
        .map(str::trim)
        .filter(|l| !l.starts_with('#'))
        .find_map(|l| l.strip_prefix("run: "))
        .into_iter()
        .chain(
            ci.lines()
                .map(str::trim)
                .filter(|l| !l.starts_with('#'))
                .filter_map(|l| l.strip_prefix("run: ")),
        )
        .find(|c| c.starts_with("cargo test --workspace"))
        .unwrap_or_else(|| {
            panic!(
                "`ci.yml` 里找不到 `cargo test --workspace …` 那一步 —— \n\
                     命令的家变了，本条的读法要跟着改（否则它会零命中地绿）。"
            )
        })
        .to_string();

    let dev = std::fs::read_to_string(repo_root().join("src/doc/DEVELOPMENT.md"))
        .expect("读不到 src/doc/DEVELOPMENT.md");
    assert!(
        dev.lines().any(|l| l.contains(cmd.as_str())),
        "`src/doc/DEVELOPMENT.md` 的「跑测试」节里没有 CI 那条命令：\n  {cmd}\n\n\
             ⚠ 它此前写的是 `cargo test --lib` 并标成「全部单元测试」——\n\
             而 `--lib` **只覆盖根包**，六个共享 crate 一条都不跑。\n\
             新人照入口文档做会得到一个**少测**的读数，而它长得和全量读数一模一样。\n\
             ⇒ 命令的唯一的家是 `ci.yml`，文档要与它逐字一致（本条不持有第三份副本）。"
    );
    // ★ 反向，且**扫全 `doc/` 而不只是这一份**。
    //
    // 第一刀只查了 `DEVELOPMENT.md`，而同一条少测命令在 `CONTRIBUTING.md` 里**还有三处**
    // （删完跑 / 发版前 checklist / 新 IPC 命令后的检查）。
    // ⇒ 那正是本仓 F07 记过的「**订正手头那一处，不等于订正那句话**」，我又犯一次。
    //
    // 判法：**裸的 `cargo test --lib`**（后面既没有过滤串也没有 `--`）在 `doc/` 里一处都不许有。
    // 带过滤（`cargo test --lib parser`）与带 `--`（`-- --nocapture` / `-- --ignored`）是
    // 合法的部分跑法，不误伤 —— 建判据当日实测：合法的四处、裸的三处，分得干净。
    //
    // ⚠ 顺带记：第一版的针（同一行出现 `--lib` 与「全部」）**当场命中了我自己的订正句**，
    // 这一版的针（裸命令）**又一次命中它** —— F23「更正时引用旧措辞」在本仓已第四次。
    // 处置沿用仓里既有的那条：**改写订正句，别让它复现原命令**（已改成「`--lib` 那种跑法不是全量」）。
    let mut bare: Vec<String> = Vec::new();
    for p in doc_files() {
        let name = p
            .file_name()
            .expect("doc 文件名")
            .to_string_lossy()
            .to_string();
        let text = std::fs::read_to_string(&p).unwrap_or_default();
        for (i, l) in text.lines().enumerate() {
            let Some((_, after)) = l.split_once("cargo test --lib") else {
                continue;
            };
            let next = after
                .trim_start()
                .split(|c: char| c.is_whitespace())
                .next()
                .unwrap_or("");
            let is_partial = !next.is_empty()
                && !next.starts_with('`')
                && !next.starts_with('|')
                && !next.starts_with('+')
                && !next.starts_with('）')
                && !next.starts_with(')');
            if !is_partial {
                bare.push(format!("  doc/{name}:{}  {}", i + 1, l.trim()));
            }
        }
    }
    assert!(
        bare.is_empty(),
        "`doc/` 里这些地方把**裸的** `cargo test --lib` 当成全量跑法：\n{}\n\n\
             ⚠ 它只覆盖**根包**，六个共享 crate 一条都不跑，而读数长得和全量一模一样\n\
             （都是「ok. N passed」）—— 照文档做的人不会察觉自己少测了。\n\
             ⇒ 换成 `ci.yml` 里那条 `--workspace --exclude …`；\n\
             真要跑部分，请带过滤串（`cargo test --lib <模块>`）或 `--`（`-- --nocapture`）。",
        bare.join("\n")
    );
}

// ═════════════════════════════════════════════════════════════════════════
// `K-P5g` `KP5GD3`：**「读几个」那句话的每一份副本，都被登记住**
//
// 分工（照 `STATUS_CELLS` 那一族的形状）：
//   · 人群 **扫出来**（`env_key_claim_lines`）—— 手写清单描述人群，是本仓最贵的病之一；
//   · 登记表 **只说「这一份是哪一类」**，不存那个数；
//   · 那个数从 **生产代码** 数出来（`env_keys_actually_read`）—— 它只有一个家。
// ⇒ 多写一份副本 ⇒ `every_copy_of_that_sentence_is_registered` 红；
//   哪份副本上的数与现场对不上 ⇒ `every_registered_copy_says_the_number_we_actually_read` 红。
// ═════════════════════════════════════════════════════════════════════════

/// 扫描器的针，**拆成两半、分行写**。
///
/// 🔴 合起来写在同一行上，本文件立刻成为它自己所描述的人群的一员
/// （登记表会开始登记自己）—— `the_registry_file_itself_stays_out_of_that_population`
/// 钉着这条性质，**别把这两行并回一行**，也别在本文件里把这两个词写在同一行上。
const NEEDLE_HEAD: &str = "只抠";
const NEEDLE_TAIL: &str = "键";

/// 全仓（`git ls-files` 口径）扫出那句话的每一份副本：`(仓相对路径, 行号, 行文)`。
fn env_key_claim_lines() -> Vec<(String, usize, String)> {
    const EXTS: &[&str] = &[
        "rs", "ts", "tsx", "sh", "mjs", "json", "yml", "toml", "md", "py",
    ];
    // 人群：跟踪着的文件（自检在 [`tracked_files`] 里：清单太短 ⇒ 整组会零命中地绿）。
    let files: Vec<String> = tracked_files();
    let mut hits: Vec<(String, usize, String)> = Vec::new();
    for rel in files {
        let Some(ext) = rel.rsplit('.').next() else {
            continue;
        };
        if !EXTS.contains(&ext) {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(repo_root().join(&rel)) else {
            continue;
        };
        for (i, l) in text.lines().enumerate() {
            if l.contains(NEEDLE_HEAD) && l.contains(NEEDLE_TAIL) {
                hits.push((rel.clone(), i + 1, l.to_string()));
            }
        }
    }
    hits
}

/// backend **今天真的**从 `/proc/<pid>/environ` 里读几个环境变量 —— 从生产代码数出来。
///
/// ★ 这个数**只有一个家**（那段生产代码）：不是本文件里的常量，也不是文档里那个词。
/// 这正是本模块头注那条手法：「判据不自己写那个数 —— 它把数抽出来，再与现场量的比」。
fn env_keys_actually_read() -> usize {
    let p = repo_root().join("src/backend/observe/accounts_query.rs");
    let raw = std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("读不到 {p:?}：{e}"));
    assert!(
        raw.len() > 20_000,
        "只读到 {} 字节的 `accounts_query.rs` —— 没读到真文件，本组在空转",
        raw.len()
    );
    // ⚠ 剥生产段**走 `guard_core` 那一份**（与全树共用同一个区间判定），本文件不另写一条。
    let prod = guard_core::production_code(&raw);
    guard_core::assert_no_test_code("accounts_query.rs", &prod);
    // ⚠ 锚点用**带边界的钉法**，不写裸 `contains`
    //（`needle_anchor_registry` 那条递减棘轮：语料变量上的裸 `contains` 只许比今天少）。
    guard_core::find_pinned(&prod, "fn session_accounts(agent_home: &Path").unwrap_or_else(|e| {
        panic!("`accounts_query.rs` 生产段的锚点挪了 —— 下面两条会零命中地绿：{e}")
    });
    let n = prod.matches("proc_env_var(pid, ").count();
    assert!(
        n > 0,
        "生产段里一处 `proc_env_var(pid, …)` 都没有 —— 抽取器坏了，本组在空转"
    );
    n
}

/// N → 中文里写这个数**允许**用的那几个字。读到第三个变量时来这儿补一行。
fn count_words_for(n: usize) -> &'static [char] {
    match n {
        1 => &['一'],
        2 => &['两', '二'],
        3 => &['三'],
        4 => &['四'],
        _ => panic!("现在读 {n} 个了 —— 来 `count_words_for` 补上这个数中文怎么写"),
    }
}

/// 从一份副本里抠出它写着的那个计数词。`None` = 那处没写数（写的是「几」「N」之类）。
fn count_word_in(line: &str) -> Option<char> {
    let at = line.find(NEEDLE_HEAD)? + NEEDLE_HEAD.len();
    let rest = &line[at..];
    let k = rest.find('个')?;
    rest[..k]
        .chars()
        .next_back()
        .filter(|c| "一二两三四五六七八九十".contains(*c))
}

/// ★ 抽取器自检：扫不到东西 / 只扫到一个文件 ⇒ 下面三条会零命中地绿。
#[test]
fn the_environ_key_claim_scan_is_not_zero_hit() {
    let hits = env_key_claim_lines();
    assert!(
        hits.len() >= 8,
        "只扫到 {} 份副本（`K-P5g` 建判据当天实测 10 份 / 4 个文件）—— 针或人群坏了",
        hits.len()
    );
    let files: std::collections::BTreeSet<&str> = hits.iter().map(|(f, _, _)| f.as_str()).collect();
    assert!(
        files.len() >= 3,
        "只扫到 {} 个文件 —— 人群塌了：{files:?}",
        files.len()
    );
    // 锚点：数量地板对「收的是不是同一类东西」是瞎的（本模块另一条判据现打过这一课）。
    // 🔴 用 `K-P5f` 漏掉的那一处当锚点 —— 它不在场就说明这条判据没在看该看的地方。
    const CANARY: &str = "src/doc/INVARIANTS.md";
    assert!(
        hits.iter().any(|(f, _, _)| f == CANARY),
        "扫到了 {} 份，但**锚点 `{CANARY}` 不在里面** —— 收的多半不是那句话了",
        hits.len()
    );
    // 生产段那个数抽得出来，否则下面那条恒绿。
    assert!(env_keys_actually_read() >= 1);
}

/// ★★ **多一份副本、少一份副本，都红。**
///
/// # 这条买的是什么（`KP5GD3` 的正题）
///
/// `K-P5f` 那一拍订正了三处、漏了第四处，**而没有任何东西因此变红** ——
/// 因为那句话根本不在任何一张表里。本条把「这句话散在哪几处」变成一件
/// **有闸看着**的事：再添一份副本，作者必须来这里说清它是哪一类。
///
/// ⚠ 它买不到「那句话说得对不对」（那是评审的活），只买「每一份都在册」。
#[test]
fn every_copy_of_that_sentence_is_registered() {
    let hits = env_key_claim_lines();
    let matches_site = |rel: &str, line: &str, site: &(&str, &str, EnvKeyClaim)| {
        rel == site.0 && line.contains(site.1)
    };

    // ① 扫到的每一份都得在册，且**只对上一条**（锚点不许含糊）。
    let mut orphans: Vec<String> = Vec::new();
    for (rel, ln, line) in &hits {
        let n = ENV_KEY_CLAIM_SITES
            .iter()
            .filter(|s| matches_site(rel, line, s))
            .count();
        if n != 1 {
            orphans.push(format!(
                "  {rel}:{ln}  对上 {n} 条登记（该是 1）\n      {}",
                line.trim()
            ));
        }
    }
    assert!(
        orphans.is_empty(),
        "\n★★ 这几份副本没在册（或锚点含糊）：\n{}\n\n\
             ⚠ 这句话说的是「backend 从 `/proc/<pid>/environ` 读几个环境变量」，\
             它在盘上**散着好几份**。`K-P5f` 那一拍改了三处、漏了第四处 ——\n\
             **订正手头那一处，不等于订正那句话**（本模块头注对同一个病记过两次）。\n\
             ⇒ 新写一份副本，就来 `ENV_KEY_CLAIM_SITES` 登记它是哪一类：\n\
             `Asserts`（在断言当下，要过计数词对拍）/ `Quotes`（在引述那句话本身）/\n\
             `OtherSubject`（同句式但主语不是进程环境）。",
        orphans.join("\n")
    );

    // ② 反向：登记表自己也会腐 —— 在册却扫不到，说明那处已经改写/删了。
    let stale: Vec<String> = ENV_KEY_CLAIM_SITES
        .iter()
        .filter(|s| {
            hits.iter()
                .filter(|(rel, _, line)| matches_site(rel, line, s))
                .count()
                != 1
        })
        .map(|(f, a, c)| format!("  {f}  锚点 {a:?}（登记为 {c:?}）"))
        .collect();
    assert!(
        stale.is_empty(),
        "\n这几条登记在盘上对不到**恰好一处**（改写了 / 删了 / 锚点现在能对上多处）：\n{}\n\n\
             ⇒ 那处真没了就删掉这一行；只是挪了就换锚点。**别让登记表替真判据挡枪。**",
        stale.join("\n")
    );
}

/// ★★ **每一份「在断言当下」的副本，写的数必须等于生产代码今天真读的那个数。**
///
/// 这条就是 `K-P5f` 漏掉第四处时**本该变红**的那条。
#[test]
fn every_registered_copy_says_the_number_we_actually_read() {
    let n = env_keys_actually_read();
    let want = count_words_for(n);
    let hits = env_key_claim_lines();
    let find = |site: &(&str, &str, EnvKeyClaim)| {
        hits.iter()
            .find(|(rel, _, line)| rel == site.0 && line.contains(site.1))
            .cloned()
    };

    let mut bad: Vec<String> = Vec::new();
    for site in ENV_KEY_CLAIM_SITES {
        let Some((rel, ln, line)) = find(site) else {
            continue; // 上一条判据专管「在册却扫不到」，这里不重复报
        };
        let got = count_word_in(&line);
        match site.2 {
            EnvKeyClaim::Asserts => {
                let ok = got.is_some_and(|c| want.contains(&c));
                if !ok {
                    bad.push(format!(
                        "  {rel}:{ln}  写的是 {got:?}，而现场是 {n}（该写 {want:?}）\n      {}",
                        line.trim()
                    ));
                }
            }
            // 🔴 **反洗白**：引述那一类不许悄悄装着一句「正好也对」的断言 ——
            // 否则把一处真断言登记成 `Quotes` 就能绕开上面那格。
            EnvKeyClaim::Quotes | EnvKeyClaim::OtherSubject => {
                if got.is_some_and(|c| want.contains(&c)) {
                    bad.push(format!(
                        "  {rel}:{ln}  登记成 {:?}，可它写的数（{got:?}）与现场一致\n      \
                             ⇒ 它其实是在**断言当下**，改登记成 `Asserts`。\n      {}",
                        site.2,
                        line.trim()
                    ));
                }
            }
        }
    }
    assert!(
        bad.is_empty(),
        "\n★★ 「backend 从 `/proc/<pid>/environ` 读几个环境变量」这句话，\
             盘上这几份与现场对不上：\n{}\n\n\
             现场那个数从生产代码数出来（`accounts_query.rs` 生产段里 `proc_env_var(pid, …)` 的处数 = {n}），\n\
             ⇒ 要么是代码改了而这几份没跟着改（**盘上留了假话**），\n\
             要么是抽取器坏了。⚠ 改的时候**每一份都要改** ——\n\
             `K-P5f` 就是改了三处漏了第四处，而当时没有任何东西会红。",
        bad.join("\n")
    );
}

/// 本文件**自己不许进那个人群**：登记表登记自己会变成一条自指的死循环。
///
/// 手法是把针拆成两半分行写（见 `NEEDLE_HEAD` / `NEEDLE_TAIL` 头注）。
/// 这条钉住那条性质 —— 有人把它们并回一行时当场红，而不是等到人群悄悄多出几条。
#[test]
fn the_registry_file_itself_stays_out_of_that_population() {
    let me = include_str!("../../../src/frontend/shell/src/doc_claim_registry.rs");
    assert!(
        me.len() > 20_000,
        "include_str! 只读到 {} 字节 —— 没读到自己，本条在空转",
        me.len()
    );
    let self_hits: Vec<usize> = me
        .lines()
        .enumerate()
        .filter(|(_, l)| l.contains(NEEDLE_HEAD) && l.contains(NEEDLE_TAIL))
        .map(|(i, _)| i + 1)
        .collect();
    assert!(
        self_hits.is_empty(),
        "本文件第 {self_hits:?} 行把针的两半写到了同一行上 —— \
             登记表于是成了它自己所描述的人群的一员。拆开写。"
    );
    // 非空对照：针本身是有效的（同一把尺子在别处确实抓得到东西）。
    assert!(!env_key_claim_lines().is_empty());
}

// ════════════════════════════════════════════════════════════════════════
// 〔IV1 余〕代码里点到的 `INVARIANTS §N` 必须真有那一节
// ════════════════════════════════════════════════════════════════════════

/// `INVARIANTS.md` 里的节号 → 那一节的正文（到下一个同级或更高级标题为止）。
/// 节号认 `## N.` / `### N.M` / `## 24bis.` / `### 17a.` 这几形标题行。
fn invariant_sections() -> std::collections::BTreeMap<String, String> {
    let mut out: std::collections::BTreeMap<String, String> = std::collections::BTreeMap::new();
    let mut cur: Option<(String, usize)> = None;
    for line in INVARIANTS.lines() {
        let level = line.chars().take_while(|&c| c == '#').count();
        let id = if (2..=4).contains(&level) && line[level..].starts_with(' ') {
            let rest = line[level..].trim_start();
            let id = section_id_prefix(rest);
            (!id.is_empty() && rest[id.len()..].starts_with(['.', ' '])).then_some(id)
        } else {
            None
        };
        match id {
            Some(id) => {
                out.entry(id.clone()).or_default();
                cur = Some((id, level));
            }
            None => {
                if level >= 2 {
                    if cur.as_ref().is_some_and(|(_, l)| level <= *l) {
                        cur = None;
                    }
                } else if let Some((id, _)) = &cur {
                    let body = out.get_mut(id).unwrap();
                    body.push_str(line);
                    body.push('\n');
                }
            }
        }
    }
    out
}

/// 从串首取一个节号：数字 ＋ 可选小写字母（`a` / `bis`）＋ 可选 `.数字`。取不到给空串。
fn section_id_prefix(s: &str) -> String {
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() && b[i].is_ascii_digit() {
        i += 1;
    }
    if i == 0 {
        return String::new();
    }
    while i < b.len() && b[i].is_ascii_lowercase() {
        i += 1;
    }
    if i + 1 < b.len() && b[i] == b'.' && b[i + 1].is_ascii_digit() {
        i += 1;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
    }
    s[..i].to_string()
}

/// 一段文本里 `INVARIANTS[.md][`] [§] N` 形的引用（节号原样）。
fn invariant_refs_in(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for (at, _) in text.match_indices("INVARIANTS") {
        // `INVARIANTS` 与 `§` 之间只许有 `.md` · 反引号 · 空格这几样（至多 5 个字符）。
        let rest = &text[at + "INVARIANTS".len()..];
        let Some(sect) = rest
            .char_indices()
            .take(6)
            .find(|&(_, c)| c == '§')
            .map(|(i, _)| i)
        else {
            continue;
        };
        if !rest[..sect].chars().all(|c| ".md` ".contains(c)) {
            continue;
        }
        let r = rest[sect + '§'.len_utf8()..].trim_start_matches(' ');
        let id = section_id_prefix(r);
        if !id.is_empty() {
            out.push(id);
        }
    }
    out
}

/// 一个引用落不落得到：`§N`（含 `Na` / `Nbis`）必须是一个标题；`§N.M` 要么是标题，
/// 要么是 `§N` 正文里行首的 `M.` 编号条（例：`§21.3` = 第 21 节的第 3 条，那一节没有子标题）。
fn invariant_ref_resolves(secs: &std::collections::BTreeMap<String, String>, id: &str) -> bool {
    if secs.contains_key(id) {
        return true;
    }
    let Some((major, minor)) = id.split_once('.') else {
        return false;
    };
    secs.get(major).is_some_and(|body| {
        body.lines()
            .any(|l| l.trim_start().starts_with(&format!("{minor}. ")))
    })
}

/// 要求住址：`INVARIANTS`「修改本文档」第 2 条（生产模块头注要引 `§ N`）
/// 「每个判据族点得出它守的是哪条要求」· 共用纪律 19（判据头注写明它守的要求住址，`INVARIANTS §N` 是三种之一）。
///
/// IV1 交上来的缺口逐字：「判据 → 条这一向**没有机检**：`INVARIANTS §N` 被判据头注点到时，没有东西核那一节存在」。
/// 人群 = 两棵树（`src/` ＋ `tests/`）全部代码文件里的引用（盘上全集，不按判据族挑）；每一条都得落到一节上。
/// ⚠ 它核「那一节存在」，**不**核「那一节说的是这件事」（那一向判不动，如实写在这里）。
#[test]
fn every_invariants_section_cited_in_code_exists() {
    let secs = invariant_sections();
    // 正控：节号表读得出，且三种形状各在（否则认法坏了）。
    for id in ["2", "2.1", "24bis", "17a", "41.6", "49"] {
        assert!(
            secs.contains_key(id),
            "INVARIANTS 的节号表里没有 `{id}` —— 标题认法坏了（读到 {} 节）",
            secs.len()
        );
    }
    let root = repo_root();
    let mut total = 0usize;
    let mut bad = Vec::new();
    let exts = ["rs", "ts", "mts", "js", "py", "sh"];
    let files = guard_core::scan_tree_excluding(&root.join("src"), &exts, &[])
        .into_iter()
        .chain(guard_core::scan_tree_excluding(
            &root.join("tests"),
            &exts,
            &[],
        ));
    for (p, text) in files {
        let rel = p
            .strip_prefix(&root)
            .unwrap_or(&p)
            .to_string_lossy()
            .replace('\\', "/");
        if rel.contains("/vendor/")
            || rel.contains("/node_modules/")
            || rel.contains("/__fixtures__/")
        {
            continue;
        }
        for id in invariant_refs_in(&text) {
            total += 1;
            if !invariant_ref_resolves(&secs, &id) {
                bad.push(format!("{rel}  §{id}"));
            }
        }
    }
    assert!(
        total > 100,
        "只扫到 {total} 处 `INVARIANTS §N` 引用 —— 根没对上，本条在空转"
    );
    // 反向自检：一条合成的悬空引用必须被判出（针用 format! 拼，别让本文件自己进人群）。
    let ghost = format!("见 INVARIANTS {}999", '§');
    let ids = invariant_refs_in(&ghost);
    assert_eq!(ids, vec!["999".to_string()], "引用认法坏了");
    assert!(!invariant_ref_resolves(&secs, &ids[0]), "悬空引用没被判出");
    assert!(
        bad.is_empty(),
        "这些地方点了一个 `INVARIANTS` 里不存在的节（共扫 {total} 处）：\n  {}\n\
         ⇒ 改号 / 删节之后引用没跟上；或节号写错。改成真在的那一节。",
        bad.join("\n  ")
    );
}
