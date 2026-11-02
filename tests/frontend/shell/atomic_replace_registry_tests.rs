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
  · 写 **monitor 自己的**文件（config.json · 释放出来的程序）\n\
    ⇒ 临时件 `sync_all` 后 `std::fs::rename(tmp, dst)` 就够（Windows 上即覆盖已在的目标，长路径前缀标准库自己补；\n\
      不手写 `MoveFileExW`）—— 文件是我们自己的，tmp 的 ACL 覆盖上去没有受害者。§4 的要求**只限定在用户文件**。\n\
拿不准就当成用户文件（两种错判的代价不对称：多保留一次 ACL 无害，丢一次 ACL 用户读不了文件）。";

/// 每个生产调用点登记：`(相对 src/frontend/shell/src 的路径, 符号, 外层项, 写的是哪类文件, 为什么是这一套)`，按「文件 × 符号 × 外层项」认。
/// 本包 manifest 明写的兄弟源码树（`guard_core::population_trees`）里那几处写成 `<包名>/<相对它 src 的路径>`。
///
/// ⚠ **登记表不是豁免清单**：新增一处没登记的 ⇒ 下面那条红，并把 `RULE` 原样打出来。
const SITES: &[(&str, &str, &str, &str, &str)] = &[
    // 壳自己的部署物（自释放的后端 · 本机 `ccm` 入口 · 文件窗口程序）暂存件上位经 `platform/own_fs.rs` 的 `OwnBin`，
    //   `local_backend.rs` 不直接 rename；换名都归下面 `own_fs.rs` 那一行。
    // `cc_bus_deploy.rs [rename]` 那一行摘了：覆盖前整目录改名留底随装 cc-bus 进了本机后端（`assets/cc_bus_install.rs`，经 `files-rename`）。
    // `config.rs [rename]`（`patch_config_at`：config.json 临时件上位）同样摘了 —— 改经 `own_fs.rs` 的原子写。
    // `logging.rs [MoveFileExW]` 摘了：诊断写口改经 `config::patch_config_at`（config.json 唯一的写函数），
    //   它那份从 config.rs 复制来的 `atomic_replace` 随之删了。
    (
        "host-core/atomic.rs", // 原 `utils.rs`：原子写随两个前端共用搬进 `host-core`
        "ReplaceFileW",
"atomic_replace_path",
        "用户文件（`atomic_write_json` 的所有调用方，如 auto-launch.json）",
        "必须保留 dst 原有 ACL。dst 不存在时 fallback 到 rename（首次写，新文件本来就继承父目录 ACL）。",
    ),
    // 这里原来有 `profile_installer.rs` 的 `ReplaceFileW` 一行（PowerShell profile ·
    //   `.mcp.json` 的本机写）。用户文件改经后端写之后那份原语零调用方、删了 ⇒ 摘行。
    //   「替换保住 explicit ACE」这条性质住到了后端 `files/write.rs::replace_contents`（Windows 上就地写）。
    // ── 补上 `rename` 那一半（§4 规则原话里第一个被禁的写法）
    // `profile_installer.rs` 的 `rename` 两处（Windows 首装分支 ＋ POSIX 分支）随那份原语一起走了。
    (
        "host-core/atomic.rs", // 同上
        "rename",
"atomic_replace_path",
        "**用户文件**（`atomic_replace_path` 的第二份副本）",
        "Windows 首装分支 + POSIX 分支同一形。\
             ⚠ 副本是**刻意**的（模块头注论证过不建统一写入器：两类文件的正确行为本来就不同），\
             但刻意复制的代价就是**两处都得被看住** —— 这正是登记表存在的理由。",
    ),
    // `logging.rs [rename]` 同上一起摘。
    // `sftp.rs [rename] 2`（远端上传落地的两步：旧的改名 `.bak` · 临时件上位）摘了 ——
    //   那段原子上传随 SFTP 搬进本机常驻后端（`src/backend/ssh-dial/sftp.rs::put_atomic`，写只许两处）；本表只管 monitor。
    // 传输台那份（今 `transfer_relay.rs`）`[rename]`（`download_inner`：下载先写 `<local>.part` 再 rename 落地）这一行摘了 ——
    //   下载的本机落点随传输台搬进了本机常驻后端（`control/transfer.rs`，第三层文件管理写面），monitor 这一侧零 rename。
    (
        "platform/own_fs.rs",
        "rename",
        "rename",
        "monitor 自己的文件（数据目录底下的状态 · bin/ 底下的部署物）",
        "旁名先 sync_all 再换名上位；文件是我们自己的，tmp 的 ACL 覆盖上去没有受害者。用户文件不经这里。",
    ),
];

fn src_root() -> PathBuf {
    // 住址唯一源：`guard_core::addr`。
    guard_core::addr::shell_src_root()
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
/// **补上 `rename`** —— `src/doc/INVARIANTS.md §4` 那条规则的原话是
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

// 🔴 **原来这里有一条 `const SELF` 自摘，已按判据自己给的出路删掉。**
//
// 它排掉的是 `tests/frontend/shell/atomic_replace_registry.rs`，理由是那份文件的
// **示例字面量**（`MoveFileExW(` / `ReplaceFileW(`）会被下面的扫描当成真调用点。
// 那些示例这一轮跟着测试段搬进了本文件，而 `call_sites()` 只扫 `src_root()`
// （生产树）⇒ 生产段那份里一个示例都不剩 ⇒ **排除成了死规则**。
// 判据自己的自检逐字报的就是这一句（「里已经没有那两个符号的示例了 —— 那条排除成了死规则，删掉它」）。
// ⇒ 照它说的删。删掉之后那份生产文件**进了人群**，而它今天一个命中都没有 —— 这是对的：
//   哪天有人真往那份文件里写一处原子替换，本条从此看得见（此前被那条排除永久挡着）。

/// 生产段（剥注释后）里**调用形态**的命中：`Symbol(`。`use` 那行没有括号，不算。
fn call_sites() -> Vec<(String, String, String)> {
    let root = src_root();
    let mut files = Vec::new();
    collect_rs(&root, &mut files);
    // 本包 manifest 明写的兄弟源码树（`guard_core::population_trees`）：键带上包名（`host-core/atomic.rs`），与壳里的相对路径分开。
    let others: Vec<(PathBuf, String)> = guard_core::population_trees(&root)
        .into_iter()
        .map(|(name, r)| (r, name))
        .collect();
    for (r, _) in &others {
        collect_rs(r, &mut files);
    }
    files.sort();
    let mut out = Vec::new();
    for f in files {
        let rel = match others.iter().find(|(r, _)| f.starts_with(r)) {
            Some((r, name)) => format!(
                "{name}/{}",
                f.strip_prefix(r)
                    .unwrap_or(&f)
                    .to_string_lossy()
                    .replace('\\', "/")
            ),
            None => f
                .strip_prefix(&root)
                .unwrap_or(&f)
                .to_string_lossy()
                .replace('\\', "/"),
        };
        let src = guard_core::strip_comment_lines(&fs::read_to_string(&f).unwrap_or_default());
        let lines: Vec<&str> = src.lines().collect();
        for (i, l) in lines.iter().enumerate() {
            for (name, pat) in SCANNED {
                if l.contains(pat) {
                    let func = guard_core::addr::enclosing_fn(&lines, i);
                    out.push((rel.clone(), (*name).to_string(), func));
                }
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

/// ★ 正题：**每一处原子替换调用点都得登记它为什么选这一套语义**。
#[test]
fn every_atomic_replace_call_site_says_which_semantics_and_why() {
    let found = call_sites();
    let missing: Vec<String> = found
        .iter()
        .filter(|(f, sym, func)| {
            !SITES
                .iter()
                .any(|(g, s, h, _, _)| g == f && s == sym && h == func)
        })
        .map(|(f, sym, func)| format!("  {f}  [{sym}]  在 `{func}` 里"))
        .collect();
    assert!(
        missing.is_empty(),
        "有原子替换调用点没登记：\n{}\n\n{RULE}",
        missing.join("\n")
    );
    // 反向：登记的还得真在（搬走/换实现了就该删条目，别留僵尸账）。
    for (f, sym, func, _, _) in SITES {
        assert!(
            found
                .iter()
                .any(|(g, s, h)| g == f && s == sym && h == func),
            "登记表里的 `{f} [{sym}]`（`{func}`）已经不在了 —— 删掉这条"
        );
    }
}

/// 每条登记都得说清**写的是哪类文件**（那才是选语义的依据），不许只写「原子替换」。
#[test]
fn each_entry_names_the_file_class_it_writes() {
    for (f, sym, _, class, _) in SITES {
        assert!(
            class.contains("用户") || class.contains("monitor 自己"),
            "{f} [{sym}] 的文件类别写的是「{class}」—— 必须明说是**用户文件**还是\
                 **monitor 自己的文件**，那是 §4 里唯一的分界"
        );
    }
    // 两套都必须真的有人用 —— 只剩一套时这张表就退化成一句废话，该回来重读 §4。
    let own = SITES
        .iter()
        .filter(|(_, s, _, class, _)| *s == "rename" && class.contains("monitor 自己"))
        .count();
    let rp = SITES
        .iter()
        .filter(|(_, s, ..)| *s == "ReplaceFileW")
        .count();
    assert!(
        own > 0 && rp > 0,
        "登记表里只剩一套语义（monitor 自己的 rename {own} 条 / ReplaceFileW {rp} 条）—— \
             「两类文件两种语义」这条分工是否还成立？回去重读 `src/doc/INVARIANTS.md §4`"
    );
}

/// ★ 把 `INVARIANTS §4` 从散文变成机检（定框 **E12**）：
/// 它逐字要求的那三件（备份 / `ReplaceFileW` / 回读校验）必须在文档里仍然写着，
/// 且 `MoveFileExW` 仍被明令排除 —— 否则本模块整套论证的前提就没了。
#[test]
fn the_doc_rule_this_registry_rests_on_is_still_there() {
    let doc = fs::read_to_string(guard_core::addr::repo_root().join("src/doc/INVARIANTS.md"))
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
