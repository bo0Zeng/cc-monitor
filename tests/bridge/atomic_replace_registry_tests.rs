use std::fs;
use std::path::{Path, PathBuf};

/// 选择规则 —— 判据红的时候原样打给写代码的人。
const RULE: &str = "\
选哪一套（`src/doc/INVARIANTS.md §4`）：\n\
  · 写**用户的**文件（PowerShell profile / MCP 配置 / auto-launch.json …）\n\
    ⇒ `ReplaceFileW(dst, tmp, NULL, REPLACEFILE_WRITE_THROUGH, …)`\n\
      理由：它**保留 dst 原有的 ACL/ADS/创建时间**。`MoveFileExW` 会把 tmp 的\n\
      继承 ACL 覆盖上去 ⇒ 用户 explicit ACE 丢失 ⇒ Documents 重定向到非默认盘的\n\
      用户读不了自己的 profile（v1.7.10 真踩过）。\n\
  · 写 **monitor 自己的**文件（config.json / 日志轮转）\n\
    ⇒ `MoveFileExW(tmp, dst, MOVEFILE_REPLACE_EXISTING)` 就够 —— 文件是我们自己的，\n\
      tmp 的 ACL 覆盖上去没有受害者。§4 的要求**只限定在用户文件**。\n\
拿不准就当成用户文件（两种错判的代价不对称：多保留一次 ACL 无害，丢一次 ACL 用户读不了文件）。";

/// 每个生产调用点登记：`(相对 src/bridge/src 的路径, 符号, 处数, 写的是哪类文件, 为什么是这一套)`。
///
/// ⚠ **登记表不是豁免清单**：新增一处没登记的 ⇒ 下面那条红，并把 `RULE` 原样打出来。
const SITES: &[(&str, &str, usize, &str, &str)] = &[
    (
        "backend/control/local_backend.rs",
        "rename",
        // 〔E2 · V28〕3 → 4：逐字节副本那一处删（-1）；落点就是 `ccm` 之后，上位那一步多一条「旧的正在跑 ⇒ 先改名挪开、再上位」（+2）。
        //   仍是 monitor 自己目录里的部署物（`~/.cc-monitor/bin/ccm`），结论不变。
        4,
        "monitor 自己的缓存（自释放出来的后端二进制 · `K-R69` 起还有本机那条 `ccm` 入口 · 〔RM1f〕本机那一份代码全景小程序）",
        "P2z 的自释放：先写 `.partial` 再 rename，防的是**半截文件被当成可执行的后端起起来**。\
             §4 把 `ReplaceFileW` 的要求限定在**用户文件**（要保 ACL/ADS），这里写的是 monitor 自己\
             刚建的新文件、dst 通常压根不存在 ⇒ 没有要保留的 ACL，`rename` 的语义正合适。\
             🔴 〔`K-R69` 09-12 从 1 处涨到 2 处，而这张表逼着重判了一次语义 —— 结论不变，理由要写清〕\
             第二处是 `install_local_ccm_entry`（本机那条 `ccm` 入口 = 后端二进制的改名副本）。\
             它**同属 monitor 自己的文件**：落点是 `~/.cc-monitor/bin/`，不是用户的 `~/.local/bin/ccm`。\
             ⚠ **那个区别正是本件的要害**：往用户那份上写就要保 ACL、更要先问用户 —— 而 `K34` 逐字\
             「原本的配置**要手动删除**」⇒ 产品根本不往那儿写。所以这一处仍落 monitor 自己那一档。\
             🔴 〔RM1f 从 2 处涨到 3 处，重判一次 —— 结论不变〕第三处是 `place_local_panorama`（本机代码全景小程序），\
             落点同是 `~/.cc-monitor/bin/`，我们自己放、本机后端自己起 ⇒ 没有要保留的 ACL，`rename` 正合适。",
    ),
    (
        "cc_bus_deploy.rs",
        "rename",
        1,
        "**用户文件**（`<claude_dir>/skills/cc-bus`，覆盖前把它整个改名成 `cc-bus.bak-<ts>` 留底）",
        "`PS1` 的可撤销那一格（`U10b` 第 7 条例外的四个配套之一）。\
             用**改名**不是拷贝：改名原子、且不会在中途留下半份备份。\
             §4 的 `ReplaceFileW` 要求限定在**覆盖用户文件**（要保 ACL/ADS）——\
             这里是把整个目录**挪开**（dst 是个新名字、必不存在），不覆盖任何东西 ⇒ `rename` 正合适。\
             ⚠ 失败就**整条中止**（错误里逐字写着「没动原目录」），绝不带着半个备份继续写。",
    ),
    (
        "config.rs",
        "MoveFileExW",
        1,
        "monitor 自己的 config.json",
        "写的是我们自己的配置文件，tmp 的 ACL 覆盖到 dst 没有受害者。\
             §4 把 ReplaceFileW 的要求**限定在用户文件**，这里不在其中。",
    ),
    // 〔CFG1 · 4D〕`logging.rs [MoveFileExW]` 摘了：诊断写口改经 `config::patch_config_at`（config.json 唯一的写函数），
    //   它那份从 config.rs 复制来的 `atomic_replace` 随之删了。
    (
        "utils.rs",
        "ReplaceFileW",
        1,
        "用户文件（`atomic_write_json` 的所有调用方，如 auto-launch.json）",
        "必须保留 dst 原有 ACL。dst 不存在时 fallback 到 rename（首次写，新文件本来就继承父目录 ACL）。",
    ),
    // 〔RW1 · 第四波 09-24〕这里原来有 `profile_installer.rs` 的 `ReplaceFileW` 一行（PowerShell profile ·
    //   `.mcp.json` 的本机写）。用户文件改经后端写之后那份原语零调用方、删了 ⇒ 摘行。
    //   「替换保住 explicit ACE」这条性质住到了后端 `files_write.rs::swap_in` 的 `cfg(windows)` 那一支。
    // ── 〔audit-0805 08-06〕补上 `rename` 那一半（§4 规则原话里第一个被禁的写法）
    // 〔RW1 · 第四波 09-24〕`profile_installer.rs` 的 `rename` 两处（Windows 首装分支 ＋ POSIX 分支）随那份原语一起走了。
    (
        "utils.rs",
        "rename",
        2,
        "**用户文件**（`atomic_replace_path` 的第二份副本）",
        "与从前 profile_installer 那两处逐行同形（Windows 首装分支 + POSIX 分支；〔RW1〕那两处今天已删）。\
             ⚠ 副本是**刻意**的（模块头注论证过不建统一写入器：两类文件的正确行为本来就不同），\
             但刻意复制的代价就是**两处都得被看住** —— 这正是登记表存在的理由。",
    ),
    (
        "config.rs",
        "rename",
        1,
        "**monitor 自己的** config.json（POSIX 分支）",
        "同文件那条 `MoveFileExW` 的 `cfg(not(windows))` 对侧。写的是我们自己的配置，\
             POSIX 上 rename 即原子替换，无 ACL 顾虑。",
    ),
    // 〔CFG1 · 4D〕`logging.rs [rename]` 同上一起摘。
    // 〔SR1b · 2026-09-24〕`sftp.rs [rename] 2`（远端上传落地的两步：旧的改名 `.bak` · 临时件上位）摘了 ——
    //   那段原子上传随 SFTP 搬进本机常驻后端（`src/backend/dial/sftp.rs::put_atomic`，写只许两处）；本表只管 monitor。
    // 〔SR1b · 2026-09-24〕`sftp_pool.rs [rename]`（`download_inner`：下载先写 `<local>.part` 再 rename 落地）这一行摘了 ——
    //   下载的本机落点随传输台搬进了本机常驻后端（`control/transfer.rs`，第三层文件管理写面），monitor 这一侧零 rename。
];

fn src_root() -> PathBuf {
    // 住址唯一源：`crate::guard_support`（头注写着 24 份副本怎么一起漂的）。
    crate::guard_support::crate_src_root()
}

/// 剥掉整行注释 —— 本文件与 `mcp.rs` / `profile_installer.rs` 的**头注里就写着这两个符号**。
fn collect_rs(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            collect_rs(&p, out);
        } else if p.extension().is_some_and(|x| x == "rs") {
            out.push(p);
        }
    }
}

/// 本文件自己要排除 —— `RULE` 里写着两个符号的**调用示例**，那是字符串不是注释，
/// 剥注释剥不掉。⚠ 第一次跑就是被这个咬红的（判据匹配到自己的文本，与 F12 那次同族）。
/// 改名了也不会静默失效：新名字会以「未登记」的身份出现在下面那条里。
/// 被扫的原子替换原语：`(显示名, 匹配串)`。
///
/// 〔audit-0805 08-06〕**补上 `rename`** —— `src/doc/INVARIANTS.md §4` 那条规则的原话是
/// 「不能用 `std::fs::rename` / `MoveFileExW` 直接覆盖用户文件」，
/// 而本登记表此前只扫两个 **Win32** 符号 ⇒ **规则里第一个被点名的写法根本不在人群里**。
/// 于是「把 §4 从散文变成机检」（定框 E12）这件事只做到了三分之二。
///
/// ⚠ 匹配串刻意是**调用形态**而不是裸 `rename(`：后者会命中函数**定义**
/// `pub async fn sftp_rename(` —— 摸底时实测多数出一处，正是本区反复记的 F24
/// （匹配单位比事实小/大）的镜像半。
const SCANNED: &[(&str, &str)] = &[
    ("MoveFileExW", "MoveFileExW("),
    ("ReplaceFileW", "ReplaceFileW("),
    ("rename", "::rename("),
    ("rename", ".rename("),
];

// 🔴 〔步 7c 剖分 2026-09-19〕**原来这里有一条 `const SELF` 自摘，已按判据自己给的出路删掉。**
//
// 它排掉的是 `src/bridge/src/atomic_replace_registry.rs`，理由是那份文件的
// **示例字面量**（`MoveFileExW(` / `ReplaceFileW(`）会被下面的扫描当成真调用点。
// 那些示例这一轮跟着测试段搬进了本文件，而 `call_sites()` 只扫 `src_root()`
// （生产树）⇒ 生产段那份里一个示例都不剩 ⇒ **排除成了死规则**。
// 判据自己的自检逐字报的就是这一句（「里已经没有那两个符号的示例了 —— 那条排除成了死规则，删掉它」）。
// ⇒ 照它说的删。删掉之后那份生产文件**进了人群**，而它今天一个命中都没有 —— 这是对的：
//   哪天有人真往那份文件里写一处原子替换，本条从此看得见（此前被那条排除永久挡着）。

/// 生产段（剥注释后）里**调用形态**的命中：`Symbol(`。`use` 那行没有括号，不算。
fn call_sites() -> Vec<(String, String, usize)> {
    let root = src_root();
    let mut files = Vec::new();
    collect_rs(&root, &mut files);
    files.sort();
    let mut out = Vec::new();
    for f in files {
        let rel = f
            .strip_prefix(&root)
            .unwrap_or(&f)
            .to_string_lossy()
            .replace('\\', "/");
        let src = guard_core::strip_comment_lines(&fs::read_to_string(&f).unwrap_or_default());
        for (name, pat) in SCANNED {
            let n = src.matches(pat).count();
            if n > 0 {
                match out.iter_mut().find(|(f, s, _)| f == &rel && s == name) {
                    Some((_, _, acc)) => *acc += n,
                    None => out.push((rel.clone(), (*name).to_string(), n)),
                }
            }
        }
    }
    out.sort();
    out
}

/// ★ 正题：**每一处原子替换调用点都得登记它为什么选这一套语义**。
#[test]
fn every_atomic_replace_call_site_says_which_semantics_and_why() {
    let found = call_sites();
    let total: usize = found.iter().map(|(_, _, n)| *n).sum();
    // 抽取器自检：扫不到东西时下面的对拍会两边都空、静默变绿。
    assert!(
        total >= 4,
        "全仓只扫到 {total} 个原子替换调用点（08-05 实测 4）—— 抽取器坏了，\
             下面的对拍会在两边都空的情况下变绿"
    );

    // 🔴 〔步 7c 2026-09-19〕**这里原来有一条「排除项自检」，连同它自检的那条排除一起删了。**
    // 理由写在上面 `SELF` 原址那段注里：示例字面量随测试段搬走 ⇒ 排除成了死规则。
    // ⚠ 它换来的那一格保护**没有丢**：那条排除的风险是「排掉之后没人看那份文件」，
    //   而现在那份文件**在人群里**（不再被排除），下面的对拍直接看着它。
    // ⇒ 正控：往 `src/bridge/src/atomic_replace_registry.rs` 的生产段塞一句
    //   `MoveFileExW(x)`，本条会以「未登记的调用点」红（步 7c 死值验跑过）。

    let mut missing = Vec::new();
    let mut drifted = Vec::new();
    for (f, sym, n) in &found {
        match SITES.iter().find(|(g, s, _, _, _)| g == f && s == sym) {
            None => missing.push(format!("  {f}  [{sym}]  {n} 处")),
            Some((_, _, want, _, _)) if want != n => {
                drifted.push(format!("  {f}  [{sym}]  表里 {want} 处，实测 {n} 处"))
            }
            Some(_) => {}
        }
    }
    assert!(
        missing.is_empty(),
        "有原子替换调用点没登记：\n{}\n\n{RULE}",
        missing.join("\n")
    );
    assert!(
        drifted.is_empty(),
        "原子替换调用点的处数变了 —— 那正是该重新判定语义的时刻：\n{}\n\n{RULE}",
        drifted.join("\n")
    );
    // 反向：登记的还得真在（搬走/换实现了就该删条目，别留僵尸账）。
    for (f, sym, _, _, _) in SITES {
        assert!(
            found.iter().any(|(g, s, _)| g == f && s == sym),
            "登记表里的 `{f} [{sym}]` 已经不在了 —— 删掉这条"
        );
    }
}

/// 每条登记都得说清**写的是哪类文件**（那才是选语义的依据），不许只写「原子替换」。
#[test]
fn each_entry_names_the_file_class_it_writes() {
    for (f, sym, _, class, why) in SITES {
        assert!(
            class.contains("用户") || class.contains("monitor 自己"),
            "{f} [{sym}] 的文件类别写的是「{class}」—— 必须明说是**用户文件**还是\
                 **monitor 自己的文件**，那是 §4 里唯一的分界"
        );
        assert!(
            why.chars().count() > 20,
            "{f} [{sym}] 的理由太短，像是占位：「{why}」"
        );
    }
    // 两套都必须真的有人用 —— 只剩一套时这张表就退化成一句废话，该回来重读 §4。
    let mv = SITES
        .iter()
        .filter(|(_, s, ..)| *s == "MoveFileExW")
        .count();
    let rp = SITES
        .iter()
        .filter(|(_, s, ..)| *s == "ReplaceFileW")
        .count();
    assert!(
        mv > 0 && rp > 0,
        "登记表里只剩一套语义（MoveFileExW {mv} 条 / ReplaceFileW {rp} 条）—— \
             「两类文件两种语义」这条分工是否还成立？回去重读 `src/doc/INVARIANTS.md §4`"
    );
}

/// ★ 把 `INVARIANTS §4` 从散文变成机检（定框 **E12**）：
/// 它逐字要求的那三件（备份 / `ReplaceFileW` / 回读校验）必须在文档里仍然写着，
/// 且 `MoveFileExW` 仍被明令排除 —— 否则本模块整套论证的前提就没了。
#[test]
fn the_doc_rule_this_registry_rests_on_is_still_there() {
    let doc = fs::read_to_string(crate::guard_support::repo_root().join("src/doc/INVARIANTS.md"))
        .expect("src/doc/INVARIANTS.md 读不到 —— 路径变了就把这条一起改");
    let sec = doc
        .split("## 4. ")
        .nth(1)
        .expect("`src/doc/INVARIANTS.md` 里找不到 §4 —— 本注册表的全部依据都在那一节");
    let sec = sec.split("\n---").next().unwrap_or(sec);
    for want in ["ReplaceFileW", "MoveFileExW", "backup", "ACL"] {
        assert!(
            sec.contains(want),
            "`INVARIANTS §4` 里已经没有 `{want}` 了 —— 本注册表按它的分界登记语义，\
                 §4 一改就该回来重判。§4 现文：\n{sec}"
        );
    }
    assert!(
        sec.contains("用户文件"),
        "`INVARIANTS §4` 不再把要求限定在**用户文件** —— 那正是本表「两类文件两种语义」\
             的唯一依据，改了就该回来重判整张表。§4 现文：\n{sec}"
    );
}
