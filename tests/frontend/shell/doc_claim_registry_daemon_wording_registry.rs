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
    "src/frontend/shell/README.md",
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
    // 🔴 〔步 8 改名一刀 2026-09-19〕**六条例外在这一拍删掉了 —— 那是例外真的没有了，不是放宽。**
    //    它们当年的理由全都是「那一处在本轮写区之外 ⇒ 改不动」，而**步 8 是全仓冻结窗口**：
    //    写区就是全仓，那个前提当场消失。逐条：
    //      · `CONTRIBUTING.md` 的 markdown 锚点 —— 目标标题（`REMOTE-PHASE0-DEPLOY.md`）这一拍跟着改了，锚点同步改，链接没断；
    //      · `README.md` 里那串 CI job 名 —— `.github/workflows/ci.yml` 的 job 同拍改名；
    //      · `tests/e2e/README.md` 与 `src/frontend/shell/README.md` 的两处占位符 `<daemon>` —— 换成 `<backend>`；
    //      · `src/frontend/shell/README.md` 里「安装 / 卸载 daemon」两句 —— 界面按钮文案住 `src/frontend/ui/settings/machine-card.ts`，同拍改成「安装 / 卸载后端」。
    //    ⇒ **账不许挂空号**：这六条今天在盘上都命中 0 次，留着就是六条空转的放行。
    ("src/doc/INVARIANTS.md", "「**ccm做到必须走daemon**」",
     "用户 08-14 逐字裁定的原话 —— 引文改了就不是引文了"),
    ("src/doc/INVARIANTS.md", "**原措辞**：「daemon 对被观测文件系统必须只读，绝不写。」",
     "§41.6 的**原措辞留档**（2026-07-31 收窄前那句）—— 历史句，改它等于篡改沿革；而它旁边那句「现措辞」正是本轮改的那一处"),
    ("src/doc/INVARIANTS.md", "「daemonless 降级读取（无需 daemon）」",
     "已删掉的那个界面 checkbox 的**逐字标签**（`K-R59` 09-11 整格删除，这里是墓碑）"),
    ("src/doc/INVARIANTS.md", "「**daemon 结构上产不出它**：`control/launch.rs` 头注逐字",
     "`K-R106` 订正段里**逐字回抄的原文**（下一句就是「那句被用户当场推翻了一半」）"),
    ("src/doc/INVARIANTS.md", "不许再用「daemon」这个词把「远端常驻的那份」与「后端」压成一个",
     "`R61` 裁定三本身 —— 它说的就是这个词，把词换掉这句话就没有指称对象了"),
    ("src/doc/IPC-PROTOCOL.md", "= CC 2.1.x daemon 后台任务",
     "这一处的 `daemon` 指的是 **Claude Code 自己**那个 `--fork-session` 后台模式，不是本仓的后端 —— 换了词就把两个不同的东西压成一个（`R61` 治的正是这一形，反方向）"),
    ("src/doc/IPC-PROTOCOL.md", "（`daemon-协议-v1 §3`）",
     "与仓外 aterm **冻结在 2026-07-18** 的那份契约文档的**名字**，不是散文"),
];

/// 语料地板：低于这个字节数就判「散文没喂进来」，而不是「一处都没有」。
const CORPUS_FLOOR_BYTES: usize = 300_000;

/// 例外表覆盖的处数 —— **恒等**，不是地板。
///
/// 少一处 = 有条例外空转了（那句话被改过）；多一处 = 有人往例外表里塞了新的放行，
/// 而放行必须是**有意的一拍**。⚠ 这个数与 [`EXEMPT`] 的条数今天恰好相等（15），
/// 但两者不是同一件事：一条片段可以盖住同一句里的两处裸词。
/// **15 → 14**：`src/frontend/shell/README.md` 那条
/// 「各配置远端 exec `<daemon> --usage`」的放行随那一行文档一起删了
/// （用量 ② 轴整轴退役）。**这是例外表变短，不是放宽。**
/// 〔步 8 改名一刀 2026-09-19〕**14 → 8**：六条例外的前提（「那一处在本轮写区之外」）
/// 被全仓冻结窗口整个取消了 ⇒ 那六处真的改成「后端」了，例外随之作废。
/// 逐条理由见 [`EXEMPT`] 表头那段注释。**又一次是例外表变短，不是放宽。**
// 8 → 7：`IPC-PROTOCOL.md` §11 按点 ↗ 时现查重写，那一处引文随旧链路的沿革一起删了。
const EXEMPT_HITS: usize = 7;

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
        // 🔴 〔2026-09-18 修复 · 步 8 2026-09-19 **再修一次**〕这几格是**故意构造的夹具**：
        //   `daemon` 出现在更长的 token 里 ⇒ 切出来不是裸词。
        //   09-18 那次把它们从 `remote-daemon-proto` 换成了当时活着的四个标识符
        //   （`embedded-daemons/` · `daemon_send_keys.rs` · `--daemon-probe` · `backendPath`），
        //   而**步 8 把那四个全改名了** —— 夹具又一次指向不存在的东西。
        //   ⇒ 这一次换成**本仓明写保护、不会再改的那一档**（`tests/evidence/w8-rename.py` 的 `PROTECTED`）：
        //   `backendPath` —— **用户盘上 `config.json` 里那个现役的键**（步 8 刻意摘出去，
        //   改它要配一次迁移，见）；
        //   `DaemonTransport` —— **仓外 aterm 自己的类型名**（冻结在 2026-07-18 那份契约里）。
        //   两个都不是我们的名字，两个都不会再动。
        //   ⚠ 教训与同一条：**机械替换会砸坏刻意构造的夹具**；
        //   而夹具指着**活体**时每一轮改名都会再砸一次 ⇒ 指向「不许改的那一档」才是稳的。
        // 🔴 **刻意不用旧闭集 id / 旧 crate 目录名那两个拼写当夹具**（第一版就是那样，当场被逮）：
        //   它们正是 `tool_registry::old_name_counts` 的针，写在这儿会让
        //   `every_place_that_still_says_the_old_name_is_registered_and_only_shrinks`
        //   把本文件读成「又一处还在说旧名字的地方」（实发 `OldId × 2`）。
        // 🔴 〔第三次被砸〕上一版这一格是 `daemonPath`，理由逐字写在上面那段
        //   注释里：「**用户盘上 `config.json` 里那个现役的键**（步 8 刻意摘出去，改它要配
        //   一次迁移）」。**条 80 把那条迁移要求整个裁掉了**（用户逐字「新版本要完全抛弃旧的」），
        //   于是 `A1` 当天就把它改成了 `backendPath` —— 夹具里再没有 `daemon` 可找，本条当场红。
        //
        // 🔴 **第三次了，说明「挑一个活着的、看起来不会改的名字」这条路本身是错的。**
        //   每一轮都在赌「这个名字这次不会动」，而三轮都赌输了。
        //   ⇒ 换做法：**现拼一个**。`concat!` 是编译期拼接，`daemonSuffix` 这个字面量
        //   **在源码里根本不存在** ⇒ 任何机械替换（按整词、按子串、按标识符）都砸不到它，
        //   任何「还有谁在说旧名字」的账本也数不到它。
        //   ⚠ 它**不需要**指向真东西：`hits()` 只在 probe 串**自身**里找（见它的实现），
        //     这一格买的是「`daemon` 嵌在更长 token 里时不算裸词」，与盘上有没有这个名字无关。
        (concat!("daemon", "Suffix"), false),
        // 仓外 aterm 自己的类型名，冻结在 2026-07-18 那份契约里 —— 不是我们的名字，我们改不动。
        ("DaemonTransport", false),
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
    // 🔴 〔步 8 改名一刀 2026-09-19〕**139 → 25，地板 100 → 20。**
    //    这不是「把地板调下去让今天好过」：那 139 处里绝大多数是 `daemon_*` / `--daemon-probe`
    //    / `embedded-daemons/` 这类**代码标识符**，而步 8 把它们全改名了 ⇒ 盘上真的没有了。
    //    今天剩下的 25 处全部来自两个**明写保护、不许改**的拼写
    //    （`backendPath` 那个现役配置键 · 旧闭集 id 与旧 crate 目录名那两个拼写 ·
    //      仓外 aterm 的类型名）。
    //    ⚠ 这一格本来就不是承重的那半：「切 token 坏掉」的两个方向分别由上面的合成串自检
    //    与下面的 `offenders` 接着；本条只答「真语料确实喂进来、而且里面确实有非裸词」。
    // 〔2026-09-29 README 按 4.0.0 重写，25 → 19，地板 20 → 15〕旧 README 里点名的那几处标识符随整段沿革删了，盘上真的没有了。
    assert!(
        idents >= 15,
        "只数出 {idents} 处代码标识符（README 重写后现打 19；步 8 后 25；改名前 139）—— 本条在真语料上没跑起来"
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
