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
    // ★ 自检：口径坏了 ⇒ 下面整族会零命中地绿。正控：本文件自己在人群里。
    assert!(
        v.iter()
            .any(|r| r == "tests/frontend/shell/doc_claim_registry_tests.rs"),
        "`git ls-files` 列出的清单里没有本文件 —— 口径坏了"
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
/// **一处都没人守**。
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

/// ★ 抽取器自检：扫描面没缩水。坏掉时下面几条会零命中零失败地绿。
#[test]
fn the_doc_scan_actually_reads_the_durable_docs() {
    let files = doc_files();
    // 正控：本模块 `include_str!` 的那一份必在人群里，且读得出正文。
    let inv = repo_root().join("src/doc/INVARIANTS.md");
    assert!(
        files.contains(&inv) && !INVARIANTS.trim().is_empty(),
        "`doc/` 的人群里没有 `INVARIANTS.md` —— 遍历坏了（扫到 {} 份）",
        files.len()
    );
}

// 这里原住着那四格「monitor 侧四层落地」的落地探针与它的反向自检：
//   `a_capability_line_has_landed` · `layer_has_landed_at` · `the_capability_line_landing_probe_actually_bites`〔散文墓碑〕。
//   它们只服务 ARCHITECTURE 那张进度表；那张表退役（架构文档不放进度，进度住），四格人群为空，
//   探针连同自检一起删 —— 留着就是一条没人喂的恒真判据。量法与普查行同拍删（`STATUS_CELLS` / `MEASURE_CENSUS`）。

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
    const EXCEPTIONS: &[(&str, &str)] = &[(
        "run_tmux_reconcile_poller",
        "`INVARIANTS.md` 那句逐字写着它**已删**（audit-fixes F03.2）—— 历史句，\
                 删掉反而丢掉「为什么今天没有 poller」的解释",
    )];
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
    // 于是 `build.rs` 里定义的函数（如 `build.rs::emit_embedded_id`）被当成「腐了」。**抽取器的扫描面要自己说清楚。**
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
    // ★ 抽取器自检 1：本文件自己声明的 `repo_root` 必须被收到 ⇒ 否则遍历坏了，下面整条会零命中地绿。
    assert!(
        decl.contains_key("repo_root"),
        "声明表里没有 `repo_root` —— 遍历或剥法坏了（抽到 {} 个）",
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
    // ★ 抽取器自检 2：锚点 —— `INVARIANTS.md` 点名的那条起会话判据必须被收到，否则剥法坏了。
    const REF_CANARY: &str = "every_value_is_judged_before_it_becomes_a_ccm_argument";
    assert!(
        refs.iter().any(|(.., sym)| sym == REF_CANARY),
        "抽到 {} 处 `file.rs::symbol`，锚点 `{REF_CANARY}` 不在里面 —— 剥法坏了",
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
        // 下面五条：载荷那一层（裸载荷 · 外层 tmux 三格）与前端那份 IR 随「起会话只交一行 `ccm …`」删了。
        //   `INVARIANTS §33 / §33b / §39` 与只绑 Windows 的 §36 点它们的句子都是**沿革**（当时那条规矩对着哪一份立、当时由哪套测试钉），
        //   每处旁边都补了今天的住址（§33 末尾「今天」那一段 · 只绑 Windows 的 §36 末尾「今天」那一段）。
        (
            "src/frontend/ui/launch-plan.ts",
            "**历史句**：`LaunchPlan` IR 当年的家（§33 背景 · §39 · 本机走 IR 那一节），已删",
        ),
        (
            "src/frontend/ui/launch-dimensions.ts",
            "**历史句**：维度注册表当年的家（§33 背景），已删",
        ),
        (
            "tests/test-support/launch-payload-golden.ts",
            "**历史句**：载荷那一份夹具的用例表（§33 LR2 更新 · U8c-3 后一半），已删",
        ),
        (
            "tests/test-support/launch-tmux-outer-golden.ts",
            "**历史句**：外层 tmux 三格那一份夹具的用例表（§33 LR2 更新 · U8c-3 后一半），已删",
        ),
        (
            "tests/frontend/ui/launch-requests.vitest.ts",
            "**历史句**：只绑 Windows 的 §36 当时那几格的验证住址（维度注册表在本机下的行为），随 IR 删了；今天的验证写在那一节末尾「今天」那一段",
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
    // ★ 自检 2：锚点必须在场，人群换了就当场红。
    const CANARY: &str = "src/session-backend.ts";
    assert!(
        refs.iter().any(|(_, _, c)| c == CANARY),
        "抽到了 {} 条，但**锚点 `{CANARY}` 不在里面** —— 收的多半不是「带目录的仓内路径」这一类了。\n\
",
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
/// `K-R118` 的死值验第 ⑦ 刀：
/// 把版本号那六处 ＋ `src/frontend/shell/Cargo.lock` **一起** bump，而 `CHANGELOG.md`
/// 一个字不动 ⇒ 实测 `GATE: OK —— 16 格全绿，一条都没红`
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
/// 的**顶层两处**仍是 `3.7.0`。
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
        total > 0,
        "一处 `INVARIANTS §N` 引用都没扫到 —— 根没对上，本条在空转"
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
