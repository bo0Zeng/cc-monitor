//! 〔步 23b · 2026-09-19〕**文件管理面的落盘原语** —— `设计/60 §6.5.2 A` 拍板的那个
//! 「**带围栏的**白名单模块」，`readonly_guard` 写盘白名单上的第二个洞口。
//!
//! # 它是什么，以及它**刻意不是**什么
//!
//! `设计/60 §6.5.1` 逐字裁过一次：同一轮对话里先说的「把不能写文件的规矩去掉」，
//! 被后说的、更具体的「ABC 都按建议」**覆盖**了；而 A 的建议原文是
//! 「**能**，但新模块必须带上 Claude 数据源围栏 ＋ 判据」。
//! ⇒ **规矩留着，加一个模块。** 本模块就是那个模块，而 `readonly_guard`
//! 两层判据**一个字都没放宽**：
//!
//! - 它只会**新建一份此前不存在的文件**（`O_EXCL`）；目标已经在了就直接失败。
//! - 它**不**删、**不**改名、**不**复制、**不**建硬链软链、**不**建目录，
//!   也**不**用截断或追加的方式去开一个既有文件（那两样都能改到既有数据）。
//! - 上面这几条不是本模块的自律，是 `readonly_guard` 白名单层**逐条扫源码**扫出来的：
//!   换一种写法，那一层当场红。
//!
//! ⚠ **上面几条刻意不写出那些函数的字面名字** —— 护栏是子串扫描、**不剥注释**，
//! 把那些词原样写进注释，会让本模块被自己的文档判成违规
//! （`control/fork_write.rs` 头注记着本仓为这件事栽过四次）。
//! **要改就改措辞，别去放宽护栏。**
//!
//! # 围栏：两道，各治一种逃逸
//!
//! `设计/60 §6.5.2 A` 逐字：「写点只许落在**用户指定的文件管理目标**下，
//! **不许**落进 Claude 那几棵树」。这一句拆成两道能分别单测的关：
//!
//! | 道 | 住址 | 它拦的是 | 它**拦不住**的是 |
//! |---|---|---|---|
//! | ① 词法 | [`fence_lexical`] | 上跳段 · 绝对路径 · 盘符 · 空段 · 落进那几棵树 | 盘上真实的 symlink —— 它根本不碰盘 |
//! | ② 现打 | [`fence_resolved`] | 目标根里藏一条指向别处的 symlink（**解完再判一次**） | 判定与落盘之间的时间窗（TOCTOU，见下） |
//!
//! ⚠ **「哪几棵树算 Claude 的」这条知识不在本模块**：`control/` 是通用层，
//! 它不该知道那个目录叫什么（`agent_locality_guard` 的针就钉在这上面）。
//! 判定的唯一住址是适配层里的 `is_inside_tree`，本模块只是**调用它**。
//!
//! # 🔴 诚实边界 —— 本模块买到的与**买不到**的
//!
//! 1. **TOCTOU 仍在**：围栏② 解析父目录与真正落盘之间有一个时间窗，
//!    有人在这个窗里把父目录换成 symlink，围栏② 看到的就是旧真相。
//!    **兜底的是 `O_EXCL` 本身** —— 最后那一段若已存在（含它是一条 symlink），
//!    开文件这一步直接失败，不会跟随过去写。⇒ 窗里能被利用的只剩「父目录整个被换掉」
//!    这一形，而那需要对目标根有写权限的本地攻击者。**如实登记为未闭合。**
//! 2. **只认得当前这一个配置根**：账号隔离（cc-acct-iso）靠切那个环境变量，
//!    盘上可以同时有好几个账号目录，而配置根解析只答得出**此刻这一个**。
//!    另外那几个靠「路径里有一段以那个名字开头」这条形状兜，**换个目录名就兜不住**。
//!    如实登记（边界逐条写在那条判定自己的头注里）。
//! 3. **没有真远端**：本模块整个是本机文件系统上的路径算术 ＋ 一次落盘，
//!    判据也全在临时目录上跑。「在一台真远端机器上跑过」这件事**本轮买不到**，
//!    判据头注里逐条写着哪几格是判不了的。
//! 4. **它今天没有调用方**：本轮只落「模块 ＋ 围栏 ＋ 判据」这三样（`99 §4` 步 23b 的射程）。
//!    把它接到命令面上要加子命令 ⇒ 要 bump `BUILD_ID` ⇒ 要同拍 re-embed（`99 §4` 条 19c），
//!    那几处全在本轮写区之外。**「能力在、还没接线」这件事不许被读成「已经能用了」。**

use crate::agents::claudecode::paths::{is_inside_tree, resolve_home};
use std::path::{Component, Path, PathBuf};

/// 围栏①（词法）：**纯路径算术，不碰盘**。过了就返回「打算写到哪」。
///
/// `rel` 是**相对**目标根的那一段。拒绝的形状逐条：
///
/// - 空串 / 全是空白 —— 没有目标就不该有写。
/// - 绝对路径、Windows 盘符 —— 那不是「根底下的一段」。
/// - 上跳段与当前目录段 —— 上跳是逃出目标根的第一条路；当前目录段本身无害，
///   但留着它就等于承认「这里做路径规范化」，而规范化与安全判定混在一起正是
///   本仓反复踩的那种坑 ⇒ **一律拒，让调用方送干净的段进来。**
/// - 过了上面几关之后，再判一次那几棵树 —— 目标根**自己**可能就在里面
///   （有人把「文件管理目标」指到配置根底下），那种情况下相对段再干净也不行。
pub fn fence_lexical(claude_home: &Path, root: &Path, rel: &str) -> Result<PathBuf, String> {
    if rel.trim().is_empty() {
        return Err("refuse write: 相对路径是空的".to_string());
    }
    for c in Path::new(rel).components() {
        match c {
            Component::Normal(_) => {}
            Component::ParentDir => {
                return Err(format!("refuse write: 相对路径里有上跳段（{rel}）"));
            }
            Component::CurDir => {
                return Err(format!("refuse write: 相对路径里有当前目录段（{rel}）"));
            }
            Component::RootDir | Component::Prefix(_) => {
                return Err(format!("refuse write: 只收相对段，给的是绝对路径（{rel}）"));
            }
        }
    }
    let target = root.join(rel);
    // 上面已经把非 Normal 的段全拒了 ⇒ 这一条恒真。**留着它是自证**：
    // 哪天有人往上面那个 match 里加一档放行，这里会当场把后果说出来。
    if !target.starts_with(root) {
        return Err(format!(
            "refuse write: 拼出来的路径不在目标根底下（{} 不在 {} 里）",
            target.display(),
            root.display()
        ));
    }
    if is_inside_tree(claude_home, &target) {
        return Err(format!(
            "refuse write: 写点落在 Claude 数据源那几棵树里（{}）——\
             管理会话文件请走历史浏览器，不走文件管理面",
            target.display()
        ));
    }
    Ok(target)
}

/// 围栏②（现打）：把**父目录**解析成真路径（解 symlink）之后再判一次。
///
/// 围栏① 是纯字符串算术，它**看不见盘上的 symlink**：目标根里放一条
/// `docs -> <配置根>`，`root/docs/x.md` 在词法上完全干净，落盘却落进了那棵树。
///
/// 处置：解父目录（不解最后那一段 —— 它本来就不该存在，`O_EXCL` 会兜），
/// 然后把「在不在目标根底下」与「是不是那几棵树」**在真路径上各判一次**。
/// `claude_home` 与 `root` 自己也解一次：两边都解完再比，才比得对
/// （配置根自己是条 symlink 的情况，只解一边会漏）。
///
/// ⚠ 父目录**必须已经在盘上**。本模块不建目录（那是白名单层明令禁止的），
/// 所以「父目录不在」是一条正常的拒绝理由，不是内部错误。
pub fn fence_resolved(claude_home: &Path, root: &Path, target: &Path) -> Result<PathBuf, String> {
    // 解不开就用原路径：配置根在很多机器上根本不存在，那是正常的，不该在这里失败。
    let real_home =
        std::fs::canonicalize(claude_home).unwrap_or_else(|_| claude_home.to_path_buf());
    let real_root = std::fs::canonicalize(root)
        .map_err(|e| format!("refuse write: 目标根解析不了（{}：{e}）", root.display()))?;
    let parent = target
        .parent()
        .ok_or_else(|| format!("refuse write: 目标没有父目录（{}）", target.display()))?;
    let name = target
        .file_name()
        .ok_or_else(|| format!("refuse write: 目标没有文件名（{}）", target.display()))?;
    let real_parent = std::fs::canonicalize(parent)
        .map_err(|e| format!("refuse write: 父目录解析不了（{}：{e}）", parent.display()))?;
    let resolved = real_parent.join(name);
    // ★ **顺序是承重的**：先问那几棵树，再问有没有跑出目标根。
    //   一条 symlink 常常同时犯两样（指出去、而且指进那几棵树），两条判定谁先答，
    //   决定了用户看到的是哪一句。**「碰了 Claude 数据源」这句更要紧**，
    //   它说的是这条围栏立在这里的**全部理由**；「跑出目标根」只是越界。
    //   ⚠ 反过来排也仍然会拒 —— 但诊断会把最要紧的那件事盖掉。
    if is_inside_tree(&real_home, &resolved) {
        return Err(format!(
            "refuse write: 解完 symlink 之后写点落进了 Claude 数据源那几棵树（{}）",
            resolved.display()
        ));
    }
    if !real_parent.starts_with(&real_root) {
        return Err(format!(
            "refuse write: 解完 symlink 之后写点跑出了目标根（{} 不在 {} 里）",
            real_parent.display(),
            real_root.display()
        ));
    }
    Ok(resolved)
}

/// 两道围栏串起来跑一遍，返回最终落点。**不碰盘上的内容，只解路径。**
///
/// 抽成单独一个函数，是为了让「围栏真的被串起来了」这件事有一个可直接喂参数的入口
/// —— 判据不必为了验围栏而每次都真写一份文件。
pub fn fenced_target(root: &Path, rel: &str) -> Result<PathBuf, String> {
    let home = resolve_home();
    let lexical = fence_lexical(&home, root, rel)?;
    fence_resolved(&home, root, &lexical)
}

/// **唯一的写盘处**：在用户指定的目标根底下，新建一份此前不存在的文件。
///
/// `create_new(true)` = `O_EXCL`：目标已经在了（哪怕它只是一条 symlink）就失败，
/// 绝不跟随、绝不覆盖。这正是 `D1` 收窄后那条铁律的误差项 ——
/// **新增一份此前不存在的文件，不算「改动用户既有数据」。**
///
/// 返回真正落盘的那个绝对路径（解完 symlink 的）。
pub fn create_new_file(root: &Path, rel: &str, bytes: &[u8]) -> Result<PathBuf, String> {
    use std::io::Write as _;
    let target = fenced_target(root, rel)?;
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&target)
        .map_err(|e| format!("refuse write: 新建 {} 失败：{e}", target.display()))?;
    // 写失败（盘满等）也要把原因带回去 —— 静默的半截文件比报错糟得多。
    f.write_all(bytes)
        .map_err(|e| format!("refuse write: 写 {} 失败：{e}", target.display()))?;
    Ok(target)
}

#[cfg(test)]
#[path = "../../../tests/backend/control/files_write_tests.rs"]
mod tests;
