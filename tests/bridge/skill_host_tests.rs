use super::*;

/// 本模块源码（剥掉测试段）—— 三条钉法都在它上面取样。
fn host_src() -> String {
    guard_core::production_code(include_str!("../../src/bridge/src/skill_host.rs"))
}

/// 从生产段里切出一个具名函数的函数体。切不到就 panic（**抽取器自检**：
/// 切歪了下面几条会零命中地绿）。
fn fn_body(src: &str, sig: &str) -> String {
    let at = src
        .find(sig)
        .unwrap_or_else(|| panic!("找不到 `{sig}` —— 抽取器坏了，本条会零命中地绿"));
    let rest = &src[at..];
    let end = rest
        .find("\n}\n")
        .unwrap_or_else(|| panic!("找不到 `{sig}` 的结尾 —— 抽取器坏了"));
    rest[..end].to_string()
}

/// ★★ **钉法 1（C3 的正题）：宿主逻辑里不许出现任何具体 skill 名。**
///
/// # 人群从声明表派生，不是手写禁词表
///
/// 这是本仓反复吃亏的那个病根（「判据的人群总是从**已经有名字**的那批派生」）的**对治**：
/// 禁词表写死 `["planned-build","cc-bus"]` 的话，加第三个 skill 就漏了；
/// 遍历 `SKILLS` 则**加一份声明自动进人群**。
#[test]
fn every_host_fn_is_blind_to_skill_names() {
    let src = host_src();
    // 抽取器自检：表本身必须非空，否则下面整条空转。
    assert!(
        SKILLS.len() >= 2,
        "声明表少于 2 份 —— 本件的 DoD 要求用两份声明验 schema"
    );
    let bodies = [
        fn_body(&src, "pub fn discover("),
        fn_body(&src, "pub fn instances("),
        fn_body(&src, "pub fn editable_paths("),
    ];
    for spec in SKILLS {
        for (i, body) in bodies.iter().enumerate() {
            assert!(
                !body.contains(spec.id),
                "宿主的第 {} 个函数体里出现了 skill 名 `{}` —— \n\
                     宿主必须对具体 skill 一无所知（定框 C3）。要按 skill 分叉的行为，\n\
                     应该变成声明里的一个字段，而不是宿主里的一个 `if`。",
                i + 1,
                spec.id
            );
        }
    }
}

/// ★ **钉法 3（C6）：`Missing` 必须带身份 —— 阴性对照式。**
///
/// 把 probe 指向一个不存在的路径，断言输出里**能看到是哪个 skill 的哪条前提**。
/// ⚠ 只断言「返回了 Missing」是不够的：那不能区分「带身份」与「只说不可用」。
#[test]
fn a_missing_skill_says_which_one_and_which_path() {
    let nowhere = Path::new("/nonexistent-claude-dir-for-tests");
    let cwd = Path::new("/nonexistent-cwd-for-tests");
    for spec in SKILLS {
        let got = discover(spec, nowhere, cwd);
        let msg = got.describe();
        assert!(
            msg.contains(spec.id),
            "缺席信息里没有 skill 身份：{msg:?} —— C6 要求说清是哪一条前提没满足"
        );
        match got {
            Presence::Missing { expected, .. } => {
                let p = expected.to_string_lossy();
                assert!(
                    p.contains("nonexistent-claude-dir-for-tests")
                        || p.contains("nonexistent-cwd-for-tests"),
                    "期望路径没落在传入的根下：{p} —— discover 可能读了进程环境而不是入参"
                );
                assert!(msg.contains(&*p), "缺席信息里没有那条期望路径：{msg}");
            }
            Presence::Found => {
                panic!("在一个不存在的 claude_dir 下竟然报 Found —— probe 形同虚设")
            }
        }
    }
}

/// ★ **钉法 2（写面）：`editable_paths` 是集合，且恰好来自声明。**
///
/// ⚠ 本条只验「集合算得对」。**「写必须过这个集合」要等 F03** ——
/// 如实登记为诚实边界，不假装本件已经把写面围住了。
#[test]
fn editable_paths_come_from_the_spec_and_nowhere_else() {
    let cwd = Path::new("/tmp/x");
    for spec in SKILLS {
        let got = editable_paths(spec, cwd);
        assert_eq!(
            got.len(),
            spec.editable.len(),
            "`{}` 的可编辑集合大小与声明不符 —— 宿主凭空加了或漏了路径",
            spec.id
        );
        let root = cwd.join(spec.artifacts.root);
        for (p, f) in got.iter().zip(spec.editable) {
            assert_eq!(
                p,
                &root.join(f),
                "`{}` 的可编辑路径不是「artifacts.root + 声明里的文件名」",
                spec.id
            );
        }
    }
}

/// 本仓的**工作目录**（`cc-monitor/` 的上一级）—— `artifacts.root` 相对的就是它。
///
/// ⚠ 这是一个**假设**，写出来免得它变成隐含知识：计划目录住工作目录的 `.claude/`，
/// 而 `cc-monitor` 是工作目录下的子仓。生产路径上这个 cwd 由**活跃 tab** 决定
/// （同 panorama 的 `RepoInfoGetter`），测试里从**这个 crate 属于哪个仓**推出来。
///
/// # 它**问 git**，不从路径往上数〔`K-R13` 09-01〕
///
/// 原来的算式是 `CARGO_MANIFEST_DIR` 往上跳两级。
/// 主树上那是 `…/cc-monitor/src/bridge` ⇒ 跳两级恰好是工作目录，**算对了**；
/// git 工作树上那是 `…/worktrees/<名>/src/bridge` ⇒ 跳两级是 `…/worktrees`，
/// 那不是任何项目的工作目录，**算错了**。
///
/// 而 08-26 有人在那个错落点上补了一个同名的东西 ⇒ **错的算式指到了一个真实存在的
/// 目录**，靠它的两条判据从此在几十棵树上一起绿，一条也没出声
/// （09-01 现打：56/56 棵工作树的老落点是**同一个**目录）。
///
/// ⇒ 换成问**权威**：`git rev-parse --git-common-dir` 给的是主仓的 `.git`，
/// 在链接工作树里问也一样 ⇒ **主树与工作树同一个答案、同一个理由**，
/// 而不是一边靠算对、一边靠盘上碰巧有个同名的东西。
///
/// ⚠ **推不出来就 panic（fail-closed），不回落旧算式** —— 见 [`workspace_cwd_from`]。
fn workspace_cwd() -> PathBuf {
    workspace_cwd_from(Path::new(env!("CARGO_MANIFEST_DIR")))
        .unwrap_or_else(|why| panic!("推不出工作目录，本条判不了（不许当成绿）—— {why}"))
}

/// 问 git 要「本 crate 属于**哪个仓**」，再上跳两级 = 工作目录。
///
/// `rev-parse --git-common-dir` 给的是**主仓**的 `.git`（在链接工作树里问也一样）——
/// 这是本条唯一的权威。`<仓>/.git` → `<仓>` → 工作目录，恰好两级。
///
/// ⚠ **推不出来一律 `Err`，不猜、不回落**。回落到「往上跳两级」等于把今天这个 bug
/// 原样搬进 `unwrap_or` 的右边，而且从此连报错都没有。
///
/// 参数是目录而不是写死 `CARGO_MANIFEST_DIR`，为的是能**对着一棵不是工作树的目录跑一次**
/// （`the_workspace_cwd_fails_closed_when_git_cannot_answer` 就是那一格）。
fn workspace_cwd_from(dir: &Path) -> Result<PathBuf, String> {
    let common = git_common_dir(dir)?;
    let repo = common
        .parent()
        .ok_or_else(|| format!("git 给的 {} 没有上一级 —— 层级不够，不猜", common.display()))?;
    let ws = repo
        .parent()
        .ok_or_else(|| format!("仓根 {} 没有上一级 —— 层级不够，不猜", repo.display()))?;
    Ok(ws.to_path_buf())
}

/// 一次只读的 `git rev-parse`。**两种「问不到」都要判**，它们在类型上不是一回事：
/// 机器上没有 `git` 时 [`std::process::Command`] 给的是 `io::Error(NotFound)`，
/// **不是**一个非零退出码（09-01 现打，两侧都量过）。
fn git_common_dir(dir: &Path) -> Result<PathBuf, String> {
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["rev-parse", "--path-format=absolute", "--git-common-dir"])
        .output()
        .map_err(|e| {
            format!(
                "起不来 `git`（{e}）—— 本条靠 git 当权威，问不到就不许猜一个出来。\n\
                     ⚠ 这一支不是「git 说不知道」，是**进程都没起来**（PATH 里没有它）。"
            )
        })?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
        // 把 fail-closed 的两种来路分开 —— 它们在 git 的报错里长得一模一样，
        // 而处置完全不同：一种是这棵树自己的登记没了，一种是压根没在 git 树里。
        let why = if linked_worktree_pointer(dir).is_some() {
            "这棵树的 worktree 登记没了：它的 `.git` 还是那行指回主仓的指针，\
                 而主仓里对应的那份登记已经被 prune 掉了（盘上的树还在，版本控制里没有它了）。\n\
                 ⇒ 这不是本判据坏了，是这棵树本身已经不在版本控制里；\
                 它的门禁在更早的格子上就已经红了。"
        } else {
            "这里不在任何 git 树里（连指回主仓的那行指针都没有）。"
        };
        return Err(format!(
            "`git rev-parse` 在 {} 上退出码 {:?} —— {why}\ngit 自己说：{stderr}",
            dir.display(),
            out.status.code()
        ));
    }
    let raw = String::from_utf8_lossy(&out.stdout).trim().to_string();
    let p = PathBuf::from(&raw);
    if !p.is_absolute() {
        return Err(format!(
            "git 给的 common-dir 不是绝对路径（{raw}）—— `--path-format=absolute` 被拿掉了？\
                 相对路径在这里没有意义：它相对的是 git 进程的 cwd，不是我们问的那个目录。"
        ));
    }
    Ok(p)
}

/// 从 `from` 往上找：这棵树的 `.git` 是不是「一行指回主仓的指针」（链接工作树的形状）。
///
/// 先撞到目录形的 `.git` ⇒ 不是链接工作树，`None`。
fn linked_worktree_pointer(from: &Path) -> Option<PathBuf> {
    for d in from.ancestors() {
        let g = d.join(".git");
        if g.is_file() {
            return Some(g);
        }
        if g.is_dir() {
            return None;
        }
    }
    None
}

/// **第二条权威路**：`git worktree list --porcelain` 的第一条 = 主工作树的住址。
///
/// ⚠ 刻意与 [`workspace_cwd_from`] 走**不同的查询** —— 把同一条查询抄两遍是自证，
/// 不是对拍：那样改一处 flag 两边一起变，判据一声不吭。
fn main_checkout_from_git(dir: &Path) -> Result<PathBuf, String> {
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["worktree", "list", "--porcelain"])
        .output()
        .map_err(|e| format!("起不来 `git`（{e}）—— 对拍的那一侧也问不到权威了"))?;
    if !out.status.success() {
        return Err(format!(
            "`git worktree list` 在 {} 上退出码 {:?}：{}",
            dir.display(),
            out.status.code(),
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    for l in text.lines() {
        // porcelain 格式：主工作树恒排第一条，字段名逐字是 `worktree <绝对路径>`。
        if let Some(rest) = l.strip_prefix("worktree ") {
            return Ok(PathBuf::from(rest));
        }
    }
    Err(
        "`git worktree list --porcelain` 里一条 `worktree ` 字段都没有 —— \
             抽取器坏了，本条会零命中地绿"
            .to_string(),
    )
}

/// ★★ **P3：`workspace_cwd()` 算得**对**不对 —— 直接断言，不再借「盘上有」。**
///
/// # 为什么非要另立这一格〔`K-R13` 09-01，本件第一条验收〕
///
/// 下面 [`the_editable_paths_point_at_real_files`] 的断言正文是
/// `p.canonicalize().is_ok()` —— **纯粹「盘上有」**。它同时透过这一个观测手段
/// 守着三件互相独立的事：
///
/// | | 性质 | 那两条判据守不守 |
/// |---|---|---|
/// | P1 | `editable` 的基准是 `artifacts.root` 不是实例目录（F02 那个原缺陷） | 守 |
/// | P2 | `artifacts.root` 的字面与真实盘上目录名对得上 | 🔴 **09-10 起不守了**，见下 |
/// | P3 | `workspace_cwd()` **推得对** | **不守** |
///
/// P3 出错之后盘上**仍然有**（08-26 有人在那个错落点上补了一个同名的东西）⇒
/// 观测手段照旧满足，两条判据照旧绿。**判据没有坏，是它从来没有 P3 那一格。**
/// 08-26 之前工作树里 P3 一错 P1 跟着红，那是**巧合的耦合**，不是有人在守。
///
/// 🔴 **09-10 订正 P2 那一行**：那两条判据的语料换成了 [`ArtifactFixture`]
/// （tempdir 夹具），理由是它们原先依赖**不在版本控制里**的产物树、
/// 因而永远只在一个人的机器上跑得起来。**P2 因此丢了，今天没有人接** ——
/// 逐条交代在 [`the_editable_paths_point_at_real_files`] 的头注里。
/// ⚠ 这一行别再读成「P2 有人守」：它现在和 P3 一样是**没人守**的一格，
/// 区别只是 P3 另立了一条（就是本条），而 P2 还没有。
///
/// ⇒ 换算法只是把今天这个答案改对；**只有这一格会在它下次算错时出声。**
///
/// # 它怎么判（两条**不同的** git 查询对拍）
///
/// 实现问的是 `rev-parse --git-common-dir`（本 crate 属于哪个仓）；
/// 本条问的是 `worktree list --porcelain` 的第一条（主工作树在哪）。
/// 少跳一级 / 换个 flag / 退回「往上跳两级」，本条都红。
///
/// ⚠ P3 **在词法上判不了**：`workspace_cwd()` 想要的「工作目录」是**项目**定义的
/// （生产路径上由活跃 tab 给），不是 crate 位置的函数，而它也不是任何一级祖先所独有的特征。
/// ⇒ 要判它只能问一个权威。这就是本条为什么起进程。
#[test]
fn the_workspace_cwd_is_derived_from_git_not_guessed_from_the_path() {
    let got = workspace_cwd();
    let main = main_checkout_from_git(Path::new(env!("CARGO_MANIFEST_DIR")))
        .unwrap_or_else(|why| panic!("问不到主工作树的住址，本条判不了 —— {why}"));
    let want = main
        .parent()
        .unwrap_or_else(|| panic!("主工作树 {} 没有上一级", main.display()))
        .to_path_buf();
    // 两边都 canonicalize：换个等价写法不该让本条红，**算错才该让它红**。
    let g = got.canonicalize().unwrap_or_else(|e| {
        panic!(
            "算出来的工作目录 {} 打不开（{e}）—— 算式推出了一个盘上没有的地方",
            got.display()
        )
    });
    let w = want
        .canonicalize()
        .unwrap_or_else(|e| panic!("权威给的 {} 打不开（{e}）", want.display()));
    assert_eq!(
        g,
        w,
        "工作目录**算错了**。\n\
             算出来 : {}\n\
             权威说 : {}\n\
             ⇒ 这一格判的是「算式推得对」，不是「算出来的地方盘上有东西」。\n\
             两者今天分得开：一个算错的路径完全可能指到一个真实存在的同名目录，\n\
             那时「盘上有」照样满足，而这一格会红。",
        g.display(),
        w.display()
    );
}

/// ★ **fail-closed 那一侧自己也要有一格** —— 「推不出来就不许猜」不许只写在注释里。
///
/// 夹具形状 = 一棵**登记被 prune 掉**的树：`.git` 还是那行指回主仓的指针，
/// 而它指向的 gitdir 不存在。09-01 现打，盘上真有 6 棵是这个形状。
///
/// ⚠ 断言的子串取自**我们自己的诊断**，不取自夹具的目录名 —— 后者会让这一格靠路径恒真。
#[test]
fn the_workspace_cwd_fails_closed_when_git_cannot_answer() {
    let base = std::env::temp_dir().join(format!("wc-probe-{}", std::process::id()));
    let deep = base.join("a").join("b");
    std::fs::create_dir_all(&deep).expect("造夹具失败 —— 本条会零命中地绿");
    std::fs::write(base.join(".git"), "gitdir: /no-such-gitdir-for-this-case\n")
        .expect("造夹具失败 —— 本条会零命中地绿");

    let err = workspace_cwd_from(&deep).expect_err(
        "git 答不上来，它竟然还给出了一个工作目录 —— 那正是本件治的那个形状：\
             推错了还猜一个看起来很合理的东西出来",
    );
    let says_prune = err.contains("登记没了");
    std::fs::remove_dir_all(&base).ok();
    assert!(
        says_prune,
        "fail-closed 的正文没说清是哪一种「问不到」。\n\
             这一格要的不是「红」，是**红得说得清**：一棵树登记被 prune（盘上还在、\
             版本控制里没了）与「压根不在 git 树里」在 git 自己的报错里长得一样，\n\
             而两者的处置完全不同。实际拿到的是：\n{err}"
    );
}

/// ★ **git 给的必须是绝对路径，否则拒收** —— `--path-format=absolute` 那一段是承重的。
///
/// # 为什么它值单独一格（而不是靠上面两条顺带守住）
///
/// 09-01 两侧现打：`rev-parse --git-common-dir` **在链接工作树里本来就回绝对路径**
/// （`…/cc-monitor/.git`），只有在**主工作树**里才回相对的 `../.git`。
/// ⇒ 把那段 flag 拿掉，**在工作树上一格都不红**，而在主树上「往上跳两级」
/// 会从一个相对路径起跳，跳到哪儿全看 git 进程的 cwd。
///
/// 那正是本仓最贵那族病的形状：**在我这棵树上量不到，换一棵树才炸**。
/// ⇒ 本条自己 `git init` 一棵**主工作树形状**的仓当夹具，
/// 于是这一刀在**任何**树上跑都逮得到，不再靠「碰巧跑在哪棵树上」。
#[test]
fn a_relative_answer_from_git_is_refused_not_patched_up() {
    let base = std::env::temp_dir().join(format!("wc-repo-{}", std::process::id()));
    std::fs::remove_dir_all(&base).ok();
    std::fs::create_dir_all(&base).expect("造夹具失败 —— 本条会零命中地绿");
    let init = std::process::Command::new("git")
        .arg("-C")
        .arg(&base)
        .args(["init", "-q"])
        .status()
        .expect("起不来 `git` —— 本条判不了，不许当成绿");
    assert!(init.success(), "夹具仓建不起来 —— 本条会零命中地绿");

    let got = git_common_dir(&base);
    std::fs::remove_dir_all(&base).ok();
    let p = got.unwrap_or_else(|why| {
        panic!(
            "在一棵刚建好的仓上都问不到 common-dir —— {why}\n\
                 ⇒ 多半是问法变了（`--path-format=absolute` 被拿掉，git 回了相对路径，\
                 而我们**拒收**相对路径）。拒收是对的：相对路径相对的是 git 进程的 cwd，\
                 拿它往上跳两级跳到哪儿没人说得准。"
        )
    });
    assert!(
        p.is_absolute(),
        "git 回了一个非绝对路径而它竟然被收下了：{}\n\
             ⇒ 那道 `is_absolute` 的闸被拆了。",
        p.display()
    );
}

/// 夹具里那个**实例目录**的名字。
///
/// 它不是装饰：P1 那一刀（把 `editable` 的基准写成「实例目录」）只有在盘上真有一个
/// 实例目录时才落得下来 —— 没有它，那一刀算出来的路径根本不成形，P1 就买不到。
const INSTANCE_DIR: &str = "some-workspace";

/// **在 tempdir 里搭一份同形的产物树**〔09-10〕。
///
/// # 🔴 布局用字面量写死，刻意**不**从 `SKILLS` 生成
///
/// 从声明生成的夹具是**循环**的：改了 `artifacts.root`，夹具跟着改，判据永远绿。
/// 写死之后它是**两份副本对拍** —— 声明单方面改了而这里没改，当场红。
/// 同一条纪律在本仓已有先例：`local_accounts.rs` 的 `write_manifest` 刻意写死
/// `"accounts.json"` 而不用 `MANIFEST_NAME`（常量是**实现**，文件名是**契约**，
/// 判据该钉契约）。
///
/// ⚠ 它买不到 P2（声明与**真实盘上**目录名对得上）—— 逐条交代写在
/// [`the_editable_paths_point_at_real_files`] 的头注里那张表。
struct ArtifactFixture(PathBuf);

impl ArtifactFixture {
    fn new(tag: &str) -> Self {
        let name = format!("ccm-skill-fx-{tag}-{}", std::process::id());
        let root = std::env::temp_dir().join(name);
        std::fs::remove_dir_all(&root).ok();

        // ── planned-build ────────────────────────────────────────────────
        // 收件箱住**计划目录根**，不住实例目录 —— F02 收工时错的正是这一格。
        let pb = root.join(".claude").join("planned-build");
        std::fs::create_dir_all(&pb).expect("造夹具失败 —— 本条会零命中地绿");
        std::fs::write(pb.join("INBOX.txt"), b"").expect("造收件箱失败");
        // 一个带标记的**实例目录**（见 `INSTANCE_DIR`）。
        let inst = pb.join(INSTANCE_DIR);
        std::fs::create_dir_all(&inst).expect("造实例目录失败");
        std::fs::write(inst.join("STATUS.md"), b"").expect("造实例标记失败");
        // 同目录下一个**真实存在、但不在白名单里**的文件（写面围栏第 ② 格的靶子）。
        std::fs::write(pb.join("README.md"), b"").expect("造旁邻文件失败");

        // ── cc-bus（`editable` 是空的，只把根搭出来，让两份声明都落得下来）──
        let bus = root
            .join("cc-monitor")
            .join("src")
            .join("shared")
            .join("cc-bus");
        std::fs::create_dir_all(&bus).expect("造 cc-bus 夹具失败");
        std::fs::write(bus.join("SKILL.md"), b"").expect("造 cc-bus 标记失败");

        // ── 逃逸靶子：住在**产物根外面**、`../..` 够得着（写面围栏第 ③ 格）──
        // 盘上真有它 ⇒ 那一格从「碰运气」变成**恒定跑得到**（先前写的是
        // `/etc/hostname`，Windows 上永远不在 ⇒ 整格静默跳过）。
        std::fs::write(root.join("outsider.txt"), b"").expect("造逃逸靶子失败");

        ArtifactFixture(root)
    }
}

impl Drop for ArtifactFixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// ★★ **接线层判据：算出来的路径必须真的指向存在的文件。**
///
/// # 它为什么存在（这条是被一次真缺陷逼出来的）
///
/// F02 收工时 `editable` 的基准是「实例目录」，算出 `<工作区>/INBOX.txt` —— 而那个
/// 文件根本不存在（收件箱是**项目级**的，住计划目录根）。**F02 的判据当时全绿**：
/// 它们只验「集合来自声明」「大小对得上」，**没有一条去看那些路径指得对不对**。
///
/// ⇒ 纯函数判据看不出「集合**整体**指错地方」。这条补的就是那一层：
/// 算一次，断言每条路径都能 `canonicalize`。
///
/// # 🔴 09-10：语料从**真实工作目录**换成 [`ArtifactFixture`]，买卖逐条交代
///
/// **换的理由不是「CI 上跑不了」，是比它更根本的一条**：那套产物树
/// （`.claude/planned-build/`）住在**仓的上一级**、**不在版本控制里**
/// （本仓 `git ls-files` 对 `.claude/` 零命中）⇒ 本条**永远只在一个人的机器上跑得起来**。
/// 而「只有一个人跑得起来」正是这个仓这两天在治的病：云端那条 Rust 门禁从 08-05 起
/// 卡在更靠前的步骤上，1300+ 条判据整整一个月**没人知道自己坏没坏**。
/// 再留一条「只有开发机跑得动」的判据，就是在同一个坑里再挖一铲。
///
/// | | 性质 | 换夹具之后 |
/// |---|---|---|
/// | P1 | `editable` 的基准是 `artifacts.root` 不是实例目录（F02 那个原缺陷） | **仍然守**：夹具里真有一个实例目录（`INSTANCE_DIR`），那一刀算出来的文件在夹具里不存在 ⇒ 照红 |
/// | P2 | 声明的字面与**真实盘上**目录名对得上 | 🔴 **丢了** |
/// | P2′ | 声明的字面与**判据里手写的那份副本**对得上 | 新买到的：夹具布局是字面量、不从声明生成 ⇒ 声明单方面改动当场红 |
/// | P3 | `workspace_cwd()` 推得对 | 从来没守过；另立在 [`the_workspace_cwd_is_derived_from_git_not_guessed_from_the_path`] |
///
/// ## 🔴 P2 丢了什么、今天谁来接
///
/// **今天没有人接。** 如实登记，别读成「换个说法还在」。
///
/// 丢的**恰恰是这一形**：planned-build 那个工具**在外面**把产物目录改了名，
/// 而 `SKILLS` 里的声明没跟 —— 换夹具之前本条会红，**现在不会**。
/// P2′ 只接得住「声明改了而夹具那份副本没改」，接不住「外面改了而两份副本都没改」。
///
/// ⚠ **接它的办法有，但不在本件**：另立一条**明确标注「本地专有」**的判据，
/// 对着真实工作目录跑同一组断言。⚠ 光加 `#[ignore]` **不算数** ——
/// 那是「靠人记得跑」，而本仓刚吃过「不跑伪装成跑了」的亏；
/// 要落就得同时把它接进一条**只在开发机跑**的门禁步骤里。那是另一件活。
///
/// 🔴 **不许**为了保住 P2 而在 CI 上现造一份产物树 —— 那会让 P2 变成**自证**
/// （我们造什么，它就验到什么）。
#[test]
fn the_editable_paths_point_at_real_files() {
    let fx = ArtifactFixture::new("editable");
    // 抽取器自检①：至少有一个 skill 声明了可编辑文件，否则整条空转。
    let total: usize = SKILLS.iter().map(|s| s.editable.len()).sum();
    assert!(
        total >= 1,
        "没有任何 skill 声明 editable —— 本条会零命中地绿（F03 的写面就没有对象了）"
    );
    let mut checked = 0usize;
    for spec in SKILLS {
        for p in editable_paths(spec, &fx.0) {
            checked += 1;
            assert!(
                p.canonicalize().is_ok(),
                "`{}` 声明的可编辑文件算出来是 {}，而夹具里没有它。\n\
                     ⇒ 两种来路，都要看：\n\
                     ① **P1**（本条的正题）：`editable` 的基准错了。F02 收工时正是这个错 ——\n\
                     　　基准写成「实例目录」，而收件箱是**项目级**的（planned-build 明写\n\
                     　　「住计划目录根，不住工作区」）。夹具里真有一个实例目录，那一刀在这里照红。\n\
                     ② **P2′**：声明（`artifacts.root` / `editable`）单方面改了，而夹具里\n\
                     　　那份**手写副本**没跟。两份副本对拍就是为了让这一形出声。\n\
                     ⚠ 本条红**不代表算式错了**：`workspace_cwd()` 推得对不对由\n\
                     `the_workspace_cwd_is_derived_from_git_not_guessed_from_the_path` 单独判，\n\
                     而 09-10 起本条根本不走那条算式（语料是夹具，不是真实工作目录）。",
                spec.id,
                p.display()
            );
        }
    }
    // 抽取器自检②〔09-10 补〕：上面那个循环真的跑够了 `total` 次。
    // `editable_paths` 回一个空 `Vec` 时循环体一次都不进 ⇒ 本条恒绿，
    // 而自检① 只数**声明**，数不到**产出**。两格差的正是这一刀。
    assert!(
        checked == total,
        "`editable_paths` 只产出了 {checked} 条路径，而声明里有 {total} 条 ——\n\
             上面那个循环在空转（少产出一条就少验一条，而本条照样绿）。"
    );
}

/// ★ **写面围栏：三道各自要能拦住东西。**
///
/// 🔴 09-10：语料同上一条，换成 [`ArtifactFixture`]。换的理由与买卖那张表写在
/// [`the_editable_paths_point_at_real_files`] 的头注里，**这里不复述**（复述就会漂）。
///
/// ⚠ 换夹具**顺带把两格从「碰运气」变成恒定跑得到**，那是白捡的：
/// ② 的旁邻文件与 ③ 的逃逸靶子先前都包在 `if …exists()` 里 —— 盘上没有就**整格静默跳过**，
/// 而 ③ 那个靶子写的是 `/etc/hostname`，**Windows 上永远跳过**。
/// 现在两个靶子都由夹具造出来，两格各加了一条「靶子在不在」的自检。
#[test]
fn the_write_fence_rejects_what_it_should() {
    let fx = ArtifactFixture::new("fence");
    let cwd = &fx.0;
    let spec = SKILLS
        .iter()
        .find(|s| !s.editable.is_empty())
        .expect("没有带 editable 的 skill —— 本条会零命中地绿");
    let root = cwd.join(spec.artifacts.root);

    // ① 白名单内的真实文件：放行。
    let ok_path = &editable_paths(spec, cwd)[0];
    let allowed = resolve_editable(spec, cwd, ok_path);
    assert!(
        allowed.is_ok(),
        "白名单里的真实文件被拒了 —— 围栏把该放的也拦了。\n\
             靶子 = {}，围栏说：{allowed:?}",
        ok_path.display()
    );

    // ② 同目录下**没在白名单里**的真实文件：拒。
    //    拿一个**不存在**的文件去试是没用的：那时拦住的是「不存在」而不是「不在白名单」。
    let sibling = root.join("README.md");
    assert!(
        sibling.is_file(),
        "夹具里那个旁邻文件不见了（{}）—— 这一格会零命中地绿",
        sibling.display()
    );
    let err =
        resolve_editable(spec, cwd, &sibling).expect_err("同目录下不在白名单的文件竟然被放行");
    assert!(
        err.contains("不在") && err.contains(spec.id),
        "拒绝理由没说清是「不在可编辑集合里」以及是哪个 skill：{err}"
    );

    // ③ 用 `..` 逃出去：拒。靶子住在**产物根外面**，由夹具造。
    let escape = root.join("..").join("..").join("outsider.txt");
    assert!(
        escape.canonicalize().is_ok(),
        "逃逸靶子打不开（{}）—— 这一格会零命中地绿",
        escape.display()
    );
    assert!(
        resolve_editable(spec, cwd, &escape).is_err(),
        "`..` 逃逸没被拦住"
    );

    // ④ ★ **等价写法必须被接受** —— 这条才是「解析后判定」与「判字符串」的真分界。
    //
    // ⚠ **写下这条的经过值得记**：我原本用 ③（`..` 逃逸）当那个分界的阴性对照，
    // 实测**变异没红** —— 把 `canonicalize` 全去掉、改成纯字符串比较，8 条判据照样绿。
    // 查清之后发现不是判据弱，是**我的用例选错了**：集合判定用的是**精确相等**
    // 而不是「以 root 开头」，所以 `..` 逃逸在字符串下**同样不在集合里**、同样被拒。
    //
    // ⇒ 顺带修正了我对这个围栏的理解，如实记下强度分布：
    //   · **集合精确相等** = 主防线（很强：白名单是具体文件名，不是目录前缀）
    //   · `canonicalize` = ① 让等价写法可用（本条验的就是它）
    //                      ② 让第三道 `is_protected_claude_data_path` 看到**符号链接的真实目标**
    //                         而不是链接名
    //   · 第三道 = 纵深（即使声明写歪也不许碰 Claude 数据）
    //
    // ⚠ 09-10：绕的那个目录先前是 `devbench`（真实工作目录里的一个实例），
    // 换夹具之后改成夹具自己那个实例目录 —— **形状一个字没变**：
    // 「进一个真实存在的子目录再 `..` 回来」，`canonicalize` 要求每一段都真的在盘上。
    let inst = root.join(INSTANCE_DIR);
    let equivalent = inst.join("..").join(spec.editable[0]);
    assert!(
        resolve_editable(spec, cwd, &equivalent).is_ok(),
        "等价写法 {} 被拒了 —— 围栏在判字符串而不是判解析后的真实路径",
        equivalent.display()
    );
}

/// ★ **两个集合的交集项，两边 id 必须逐字一致**〔devbench F06〕。
///
/// # 它们不是「同一张表的两个视图」
///
/// [`SKILLS`]（接入的 skill：cc-monitor 显示它的产物、编辑它的注入文件）与
/// `tool_registry::TOOLS`（受管工具：**装到别处**的东西）是**两个不同集合，有交集**。
/// 今天交集只有 `cc-bus` 一个：`planned-build` 在这边不在那边（它不由 cc-monitor 装），
/// 而 `ccm`/`cc-acct-iso`/`backend`（〔`K-R81` 09-12〕原先叫 `remote-daemon`）/
/// `project-mcp`/`powershell-profile` 在那边不在这边（它们不是 skill）。
///
/// ⚠ **反方向刻意不钉**（「TOOLS 里的每个工具都该是一个 skill」）—— 那句话是假的，
/// 钉它等于把一个错误的概念做成判据。devbench 的账本 L4 原写「同一张表的两个视图」
/// 就是这个错，F06 已订正。
#[test]
fn the_intersection_uses_the_same_id_on_both_sides() {
    let reg = guard_core::production_code(include_str!("../../src/bridge/src/tool_registry.rs"));
    let mut intersect = 0usize;
    for spec in SKILLS {
        if reg.contains(&format!("id: {:?}", spec.id)) {
            intersect += 1;
        }
    }
    // 抽取器自检：交集为 0 说明要么抽取坏了，要么两张表真的毫无关系
    // （那时 `Install::ManagedTool` 那条判据也会红，两条互相印证）。
    assert!(
        intersect >= 1,
        "`SKILLS` 与 `TOOLS` 交集为 0 —— 抽取器坏了，或 `cc-bus` 从某一边消失了。\n\
             本条会零命中地绿，所以它必须先红。"
    );
}

/// ★ **跨语言对拍：TS 那份手写类型的字段必须与本结构体一致。**
///
/// `SkillView` 在 TS 侧是**手写**的（不是 ts-rs 生成，照 `launch-cli-wire.ts` 的先例）
/// ⇒ 没有编译器管着它。这条判据读 TS 源码逐字段对拍，漏一个就红。
///
/// ⚠ 它**只钉字段名**，不钉类型 —— 那是本仓「名字钉死是普遍的、类型生成是按需的」
/// 那条成文规则的档位。如实记，别读成「类型也对上了」。
#[test]
fn the_ts_view_type_matches_this_struct() {
    let ts = std::fs::read_to_string(crate::guard_support::repo_src_root().join("ipc/commands.ts"))
        .expect("读不到 `src/ipc/commands.ts` —— 抽取器坏了，本条会零命中地绿");
    let at = ts
        .find("export interface SkillView {")
        .expect("TS 侧找不到 `SkillView` 接口 —— 它被改名或删了");
    let body = &ts[at..at + ts[at..].find('}').expect("接口没闭合")];

    // 人群从 Rust 这一侧派生：改结构体就自动进人群，不用记得回来加。
    for field in ["id", "label", "missing_reason", "instances", "editable"] {
        assert!(
            body.contains(field),
            "TS 的 `SkillView` 缺字段 `{field}`。\n\
                 它是手写类型（没有编译器管），Rust 侧 `skill_host::SkillView` 改了字段\n\
                 就必须来这里同步 —— 这条判据就是那个「必须」。"
        );
    }
    // 抽取器自检：真的切到了接口体，而不是切了个空串。
    assert!(
        body.len() > 60,
        "切出来的 TS 接口体只有 {} 字节 —— 切歪了，本条会零命中地绿",
        body.len()
    );
}

/// ★ **每一段都要语义非空** —— 「装得下」不等于「装得对」（F02 DoD Y21）。
///
/// 没有这条，我可以给第二份声明填一堆空串让它编过，然后声称 schema 验过了。
#[test]
fn every_spec_section_is_semantically_filled() {
    for spec in SKILLS {
        assert!(!spec.id.is_empty() && !spec.label.is_empty(), "id/label 空");
        match &spec.discover {
            Discover::ClaudeSkill { dir, probe_file } => {
                assert!(
                    !dir.is_empty() && !probe_file.is_empty(),
                    "{}: discover 空",
                    spec.id
                );
            }
            Discover::CwdPath { path } => {
                assert!(!path.is_empty(), "{}: discover 空", spec.id);
            }
        }
        assert!(
            !spec.artifacts.root.is_empty() && !spec.artifacts.instance_marker.is_empty(),
            "{}: artifacts 段有空字段 —— 空 root 会让 instances() 去列工作目录本身",
            spec.id
        );
        // ⚠ `editable` **允许为空**（cc-bus 就没有「人手写的注入文件」）——
        // 空数组是有意义的值。但每一项都不许是空串。
        for f in spec.editable {
            assert!(!f.is_empty(), "{}: editable 里有空文件名", spec.id);
        }
        match &spec.install {
            Install::ManagedTool(id) => {
                assert!(!id.is_empty(), "{}: install 指向空 id", spec.id)
            }
            Install::NotSupported(why) => assert!(
                why.len() > 20,
                "{}: install 标 NotSupported 但理由太短（{} 字节）—— \
                     如实登记要说清为什么、归谁",
                spec.id,
                why.len()
            ),
        }
    }
}

/// ★ **`install` 指向的必须是 `tool_registry` 里真实存在的 id。**
///
/// 这条把「两个视图」钉成「不是两份数据」（账本 L4）：指一个不存在的工具 ⇒ 当场红。
#[test]
fn managed_tool_ids_exist_in_the_tool_registry() {
    let reg = guard_core::production_code(include_str!("../../src/bridge/src/tool_registry.rs"));
    // 抽取器自检：那份源码里必须真的有 id 字段，否则下面恒绿。
    assert!(
        reg.contains("id:"),
        "`tool_registry.rs` 里读不到 `id:` —— 抽取器坏了，本条会零命中地绿"
    );
    for spec in SKILLS {
        if let Install::ManagedTool(tool_id) = &spec.install {
            let needle = format!("id: {tool_id:?}");
            assert!(
                reg.contains(&needle),
                "`{}` 的 install 指向 `tool_registry` 里不存在的 id `{tool_id}`\n\
                     （找的是字面 {needle}）—— 装法的住址只有一个，别在这里另立一份",
                spec.id
            );
        }
    }
}

/// ★ **`actions` 段不许回潮**〔用 08-10：cc-monitor 不调用任何 skill〕。
///
/// 本模块唯一碰外部世界的地方是存在性探测。一旦有人加回「跑 skill 命令」那条路，
/// 诚实边界 5a 那个口子（外部命令间接写盘）就回来了。
#[test]
fn the_host_never_spawns_anything() {
    let src = host_src();
    for forbidden in ["Command::new", "std::process", "spawn("] {
        assert!(
            !src.contains(forbidden),
            "宿主里出现了 `{forbidden}` —— cc-monitor **不调用任何 skill**（用户 08-10）：\n\
                 它只做装配台与产物编辑台，跑 skill 是 agent 的活。\n\
                 若真要加，先回定框 C4 加一行理由，并把 ROADMAP 的 5a 从「靶子转移」改回「活的」。"
        );
    }
}

/// `PS2-Y1`：「为什么没有装卸面」那段边界必须留在本文件上。
///
/// 它省的是**几天**：下一个被指派这件事的人若不知道两条 skill 一条都装不了，
/// 会先去写 UI、再在联调时撞上 `installable: false`，最后才找到只读铁律那堵墙。
///
/// ⚠ 读 `production_source`（**只剥测试段、保留注释**）—— 上一件 `P8b` 首跑就栽在
/// 用错剥法上：`production_code` 连 `//` 一起剥，而这类判据钉的**恰恰是注释**。
/// 剥测试段仍是必须的：否则本条自己这几个字面量会把自己喂绿（本会话第六次防同一个自伤）。
#[test]
fn why_there_is_no_install_ui_is_written_down_here() {
    let prod = guard_core::production_source(include_str!("../../src/bridge/src/skill_host.rs"));
    for needle in ["U10b", "U9", "installable: false", "恒灰"] {
        assert!(
            prod.contains(needle),
            "宿主头注里少了「{needle}」—— 那段边界是 `PS2` 唯一的交付物"
        );
    }
    // ★ 钉**说法本身**：把「不许装」写成「还没实现」是最可能的腐坏形态，
    // 而两者的处置完全不同（一个要裁定、一个要工时）。
    assert!(
        prod.contains("那是**不许装**，不是「还没写」"),
        "那句区分被改掉了 —— 它正是本件的正题"
    );
}

/// `PS2-Y2`：`Presence` 今天**恰好两态**。
///
/// ★★ 本条**不是禁止加第三态** —— 它是个**提问点**：加之前先答「装着的那份是哪个版本」
/// 从哪来（`U9` 第二问，实测两份差 167 行）。答了就把这条改掉，连同上面那段头注。
#[test]
fn presence_still_has_exactly_two_states() {
    // 用穷举 match 钉：加了变体**编译期**就红在这里，比数字符串可靠。
    let sample = Presence::Missing {
        skill: "x".into(),
        expected: PathBuf::from("/x"),
    };
    let n = match sample {
        Presence::Found => 1,
        Presence::Missing { .. } => 2,
    };
    assert_eq!(
        n, 2,
        "`Presence` 的变体变了。**不是不许加第三态**（「版本不符」正是 `PS2` 想要的），\
             但加之前先答：装着的那份是**哪个版本**、这个量从哪来？—— 那是 `U9` 第二问，\
             今天未裁，且实测仓内那份与 `~/.claude/skills/` 那份差 167 行。\
             答了就把这条判据与 `skill_host` 头注那段一起改掉。"
    );
}
