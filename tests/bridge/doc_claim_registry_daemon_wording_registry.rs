use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    // 住址唯一源：`crate::guard_support`（头注写着 24 份副本怎么一起漂的）。
    crate::guard_support::repo_root()
}

/// 本闸的扫描面 —— `K-R116` 写区里那 9 份散文。**闭集，按住址点名。**
///
/// 🔴 表名起成 `SITES` 是 `scanning_guard_registry::TABLE_DECLS` 那条纪律要的
/// （「新写一条『扫描面 ＋ 常量表』型的判据，那张表要起成 `TABLE_DECLS` 里已有的名字之一」）。
const SITES: &[&str] = &[
    "src/doc/IPC-PROTOCOL.md",
    "src/doc/INVARIANTS.md",
    "src/doc/ARCHITECTURE.md",
    "src/doc/CONTRIBUTING.md",
    "README.md",
    "README.en.md",
    "tests/e2e/README.md",
    "src/bridge/README.md",
    "src/backend/README.md",
];

/// 写区里**裸着的 `daemon`，而它一个字都不许动** —— `(文件, 逐字片段, 理由)`。
///
/// 🔴 **这是本仓这个闭集的唯一住址**：量具 `tests/evidence/K-R116-ruler.py` 不抄一份，
/// 它**解析本表**（`--apply` 与本闸因此不可能对不上）。
///
/// 每条片段必须在那份文件里**恰好命中一次** —— 命中 0 次 = 那句话被改过了、这条例外
/// 此刻在空转；命中多次 = 片段太短，说不清点的是哪一处。两侧都由下面的判据断言。
const EXEMPT: &[(&str, &str, &str)] = &[
    ("src/doc/INVARIANTS.md", "「**ccm做到必须走daemon**」",
     "用户 08-14 逐字裁定的原话 —— 引文改了就不是引文了"),
    ("src/doc/IPC-PROTOCOL.md", "「ccm 做到必须走 daemon」",
     "同上，用户 08-14 逐字裁定在本文件里的第二处引用"),
    ("src/doc/INVARIANTS.md", "**原措辞**：「daemon 对被观测文件系统必须只读，绝不写。」",
     "§41.6 的**原措辞留档**（2026-07-31 收窄前那句）—— 历史句，改它等于篡改沿革；而它旁边那句「现措辞」正是本轮改的那一处"),
    ("src/doc/INVARIANTS.md", "「daemonless 降级读取（无需 daemon）」",
     "已删掉的那个界面 checkbox 的**逐字标签**（`K-R59` 09-11 整格删除，这里是墓碑）"),
    ("src/doc/INVARIANTS.md", "「**daemon 结构上产不出它**：`control/launch.rs` 头注逐字",
     "`K-R106` 订正段里**逐字回抄的原文**（下一句就是「那句被用户当场推翻了一半」）"),
    ("src/doc/INVARIANTS.md", "不许再用「daemon」这个词把「远端常驻的那份」与「后端」压成一个",
     "`R61` 裁定三本身 —— 它说的就是这个词，把词换掉这句话就没有指称对象了"),
    ("src/doc/CONTRIBUTING.md", "REMOTE-PHASE0-DEPLOY.md#发版构建交叉编译--内嵌-daemon-二进制f08b",
     "markdown **锚点**，指向 `src/doc/REMOTE-PHASE0-DEPLOY.md` 的标题；那份文件不在本轮写区 ⇒ 标题不动，锚点跟着不许动，否则链接当场断"),
    ("src/doc/IPC-PROTOCOL.md", "= CC 2.1.x daemon 后台任务",
     "这一处的 `daemon` 指的是 **Claude Code 自己**那个 `--fork-session` 后台模式，不是本仓的后端 —— 换了词就把两个不同的东西压成一个（`R61` 治的正是这一形，反方向）"),
    ("src/doc/IPC-PROTOCOL.md", "（`daemon-协议-v1 §3`）",
     "与仓外 aterm **冻结在 2026-07-18** 的那份契约文档的**名字**，不是散文"),
    ("README.md", "`rust` / `frontend` / `daemon` / `linux-app-build` / `e2e-smoke`",
     "`.github/workflows/ci.yml` 里的 **job 名**，改它 CI 就对不上"),
    ("tests/e2e/README.md", "<daemon>",
     "shell 命令里的**占位符** `<daemon>`（要替进去的是那个二进制的路径）"),
    ("src/bridge/README.md", "设置面板「安装 daemon」",
     "**逐字引用界面上那个按钮的文案** —— 文案住 `src/settings/machine-card.ts`（本轮写区之外）；只改文档不改界面，文档当场说假话。UI 文案那一档整体交回 PM 另派"),
    ("src/bridge/README.md", "设置面板「卸载 daemon」",
     "同上，另一个按钮的逐字文案"),
    ("src/bridge/README.md", "一次性 exec `<daemon> --list-projects/--list-sessions/",
     "同上，命令行占位符 `<daemon>`"),
];

/// 语料地板：低于这个字节数就判「散文没喂进来」，而不是「一处都没有」。
const CORPUS_FLOOR_BYTES: usize = 300_000;

/// 例外表覆盖的处数 —— **恒等**，不是地板。
///
/// 少一处 = 有条例外空转了（那句话被改过）；多一处 = 有人往例外表里塞了新的放行，
/// 而放行必须是**有意的一拍**。⚠ 这个数与 [`EXEMPT`] 的条数今天恰好相等（15），
/// 但两者不是同一件事：一条片段可以盖住同一句里的两处裸词。
/// 〔`设计/50` 09-18〕**15 → 14**：`src/bridge/README.md` 那条
/// 「各配置远端 exec `<daemon> --usage`」的放行随那一行文档一起删了
/// （用量 ② 轴整轴退役）。**这是例外表变短，不是放宽。**
const EXEMPT_HITS: usize = 14;

/// ASCII 标识符字符 —— **汉字不算**，这一条就是「两个数」的分水岭。
fn is_ident(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_'
}

/// 把 `[a, b)` 这处命中扩成它所属的 ASCII token，返回 `(起, 止)`。
///
/// ⚠ 按**字节**走：`-` `.` `/` `:` 只在它另一侧紧跟 ASCII 标识符字符时才吃 ——
/// 于是 `ccm做到必须走daemon` 切出裸词（汉字挡住了扩张），
/// 而 `src/backend` / `daemon_send_keys.rs` 切出整条。
fn token_at(s: &[u8], mut a: usize, mut b: usize) -> (usize, usize) {
    while a > 0
        && (is_ident(s[a - 1])
            || (matches!(s[a - 1], b'-' | b'.' | b'/' | b':') && a >= 2 && is_ident(s[a - 2])))
    {
        a -= 1;
    }
    while b < s.len()
        && (is_ident(s[b])
            || (matches!(s[b], b'-' | b'.' | b'/' | b':') && b + 1 < s.len() && is_ident(s[b + 1])))
    {
        b += 1;
    }
    (a, b)
}

/// 一份文本里 `daemon`（不分大小写）的全部命中起点。
///
/// ⚠ 刻意**不写** `.contains("…")` / `.find("…")` 那一形：
/// `needle_anchor_registry` 的递减棘轮按「拿磁盘语料做裸字面量匹配」计数，
/// 而本条的针是**变量**（下面 `NEEDLE`），不进那个人群。
fn hits(haystack: &str) -> Vec<usize> {
    const NEEDLE: &str = "daemon";
    // ⚠ 局部变量**刻意起长名**：`needle_anchor_registry::corpus_vars` 的传递闭包
    //   **按名字**跑（不看类型），而本文件里 `text` / `b` / `lower` 这几个短名
    //   早就被别的判据用着 —— 在这里复用一个，就会把同文件里
    //   `name.starts_with("README")` 那一族**早已存在**的匹配一起卷进它的人群，
    //   那条递减棘轮当场 33 → 34。〔09-14 实打逮到过一次，读数在 `tests/evidence/K-R116-deathvalue.md`〕
    let folded_haystack = haystack.to_ascii_lowercase();
    let folded_bytes = folded_haystack.as_bytes();
    let width = NEEDLE.len();
    let mut out = Vec::new();
    let mut cursor = 0usize;
    while cursor + width <= folded_bytes.len() {
        if &folded_bytes[cursor..cursor + width] == NEEDLE.as_bytes() {
            out.push(cursor);
            cursor += width;
        } else {
            cursor += 1;
        }
    }
    out
}

fn read(rel: &str) -> String {
    std::fs::read_to_string(repo_root().join(rel))
        .unwrap_or_else(|e| panic!("{rel} 读不到：{e} —— 文件搬了就把本条一起改"))
}

/// ★★ 正题：**那 9 份散文里不许再有人读的 `daemon`**。
#[test]
fn no_prose_in_the_wording_sites_still_says_daemon() {
    let bodies: Vec<(&str, String)> = SITES.iter().map(|r| (*r, read(r))).collect();

    // ── 抽取器自检①：语料真喂进来了（读空了下面每一条都会零命中地绿）──
    let total: usize = bodies.iter().map(|(_, t)| t.len()).sum();
    assert!(
        total >= CORPUS_FLOOR_BYTES,
        "{} 份散文只读到 {total} 字节（地板 {CORPUS_FLOOR_BYTES}）—— 抽取器坏了，本条在空转",
        bodies.len()
    );

    // ── 抽取器自检②：切 token 那一步两个方向都要对 ──
    //
    // 用**合成串**喂，不碰真语料：真树上「采到了它、而它过了」与「压根没扫到」
    // 在输出上一模一样，那正是 `scanning_guard_registry` 头注治的那一形。
    for (probe, want_bare) in [
        // ⚠ 刻意**不写**那个「远端 ＋ 旧词」连写的形：`tool_registry::SITES` 那张**旧名字存量账**
        //   按整串数它（`Why::Wording`），本文件写一处就得往那张账上加一行 ——
        //   而那张账数的是「还没改的措辞」，一处**自检夹具**混进去会把它读成一笔真债。
        ("常驻 daemon 的 stdin", true),
        ("ccm做到必须走daemon", true),
        // 🔴 〔2026-09-18 修复〕这一格原是 `("remote-daemon-proto", false)` ——
        //   一个**故意构造的夹具**：`daemon` 出现在更长的 token 里。
        //   重组的机械改名把它换成了 `"src/backend"`，而那串里**一个 `daemon` 都没有**
        //   ⇒ 夹具失去意义、本条当场红。这正是 `调研/设计/16 §5.3` 记的那类假阳性：
        //   **机械替换会砸坏刻意构造的测试夹具**。
        //   换成 `embedded-daemons/`（活的目录名，同样是「非裸词」那一形）。
        ("embedded-daemons/", false),
        ("daemon_send_keys.rs", false),
        ("--daemon-probe", false),
        ("daemonPath", false),
    ] {
        let h = hits(probe);
        assert_eq!(h.len(), 1, "自检串 {probe:?} 里应当恰好一处命中");
        let (a, b) = token_at(probe.as_bytes(), h[0], h[0] + 6);
        let bare = probe[a..b].eq_ignore_ascii_case("daemon");
        assert_eq!(
            bare,
            want_bare,
            "切 token 判错了：{probe:?} 切出 {:?}，期望「裸词={want_bare}」",
            &probe[a..b]
        );
    }

    // ── 例外表：每条恰好命中一次，逐条求出它盖住的区间 ──
    let mut spans: Vec<(&str, usize, usize)> = Vec::new();
    for (f, frag, why) in EXEMPT {
        let site_text = &bodies
            .iter()
            .find(|(r, _)| r == f)
            .unwrap_or_else(|| panic!("例外表点的 {f} 不在 SITES 里 —— 两张表对不上"))
            .1;
        let n = site_text.matches(frag).count();
        assert_eq!(
            n, 1,
            "例外片段在 {f} 里命中 {n} 次（要求恰好 1 次）：{frag}\n\
                 · 0 次 = 那句话被改过了，这条例外此刻在空转（理由：{why}）\n\
                 · 多次 = 片段太短，说不清点的是哪一处"
        );
        let at = site_text.find(frag).expect("上面刚断言过命中一次");
        spans.push((f, at, at + frag.len()));
    }

    // ── 正题 ──
    let mut offenders: Vec<String> = Vec::new();
    let mut exempted = 0usize;
    let mut idents = 0usize;
    for (rel, site_text) in &bodies {
        let raw = site_text.as_bytes();
        for h in hits(site_text) {
            let (a, b) = token_at(raw, h, h + 6);
            if !site_text[a..b].eq_ignore_ascii_case("daemon") {
                idents += 1;
                continue;
            }
            if spans.iter().any(|(f, x, y)| f == rel && *x <= a && a < *y) {
                exempted += 1;
                continue;
            }
            let at = site_text[..a].matches('\n').count() + 1;
            let from = site_text[..a].rfind('\n').map(|i| i + 1).unwrap_or(0);
            let upto = site_text[b..]
                .find('\n')
                .map(|i| b + i)
                .unwrap_or(site_text.len());
            let ctx: String = site_text[from..upto].chars().take(90).collect();
            offenders.push(format!("  {rel}:{at}  {ctx}"));
        }
    }

    // ── 抽取器自检③：标识符那一档必须真的数到东西 ──
    //
    // 数不到 = 切 token 那一步在真语料上根本没跑（合成串过了不代表真树上跑到了）。
    assert!(
        idents >= 100,
        "只数出 {idents} 处代码标识符（09-14 现打 139）—— 本条在真语料上没跑起来"
    );

    // ── 抽取器自检④：例外表不许空转（**恒等**，不是地板）──
    assert_eq!(
        exempted, EXEMPT_HITS,
        "例外表今天盖住 {exempted} 处（登记 {EXEMPT_HITS}）——\n\
             少了 = 有条例外空转；多了 = 有人往表里塞了新的放行。\n\
             放行必须是有意的一拍：改这个数的同一拍要在 EXEMPT 里写清是哪条、为什么。"
    );

    assert!(
        offenders.is_empty(),
        "这些散文里又写了人读的 `daemon`（`R61`：不要有 daemon 这个说法了）：\n{}\n\n\
             ★ 出路两条：① 把它改成「后端」（英文那份是 `backend`）；\n\
             ② 它**真的**不该改（用户逐字引用 · 历史原措辞留档 · markdown 锚点 ·\n\
             命令行占位符 · CI job 名 · 界面按钮的逐字文案 · 指的是 Claude Code 自己那个\n\
             daemon）⇒ 往 `EXEMPT` 加一行**并写清理由**，同一拍把 `EXEMPT_HITS` 调上去。\n\
             ⚠ **代码标识符本来就不该红**（`src/backend` · `daemon_*` · `--daemon-probe`）——\n\
             它红了说明 token 切法出问题了，先看上面那几条自检。",
        offenders.join("\n")
    );
}
