//! 〔步 23b · 2026-09-19〕**文件管理面的落盘原语** —— `设计/60 §6.5.2 A` 拍板的那个
//! 「**带围栏的**白名单模块」，`readonly_guard` 写盘白名单上的第二个洞口。
//!
//! # 🔴〔波 5 ㈡ · 2026-09-23〕本模块从白名单层**搬到了第三层**，下面「它刻意不是什么」那一节是**历史**
//!
//! 用户逐字：「**现在只允许后端的文件管理部分写文件**」（收窄的是**主语**，不是动作）。
//! ⇒ `设计/60 §8.6` 第 3 步：新建目录 · 改名 · 删除 · 改权限 · 覆盖写 —— 它们要
//! **改动既有数据**，正是白名单层的判准（「不改既有数据」）所禁的。
//! ⇒ `readonly_guard` 长出**第三层**，判准换成「**改，但每一处都先过围栏、
//! 且只从声明过的那一面来**」，本模块是那一层**唯一**登记的模块。
//!
//! | 那一层钉的 | 怎么钉 |
//! |---|---|
//! | 能用哪几个改动动词 | **闭集**登记（`readonly_guard` 那张表）；表外的改动动词在本模块里照旧红 —— 包括**递归删**（没签字，理由见 [`delete_entry`]） |
//! | 每一处改动都先过围栏 | 本模块每个含改动动词的函数里，围栏调用必须出现在**第一个改动动词之前** |
//! | 只从文件管理那一面来 | 后端生产树里引用得到本模块的文件，集合**恒等于**登记的那一扇门（`inbound.rs`）；其余任何一面伸手 ⇒ 红 |
//! | `O_EXCL` 那一处没变质 | 开句柄的调用与 `O_EXCL` 仍然逐一配对（新建那条路一个字节没松） |
//!
//! ⚠ 下面原来那几段（「它只会新建……」「它不删、不改名……」）写的是 09-19 那一版的射程，
//! **今天不再成立**；留着是因为「它当初为什么只许新建」那条推理今天仍然是第三层的底子
//! （O_EXCL 那条路**仍然**只新建）。读现状看上面那张表。
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
//! | ① 词法 | [`fence_lexical`] | 上跳段 · 绝对路径 · 盘符 · 空段 · 写点本身就是一份会话文件 | 盘上真实的 symlink —— 它根本不碰盘 |
//! | ② 现打 | [`fence_resolved`] | 目标根里藏一条指向别处的 symlink（**解完再判一次**） | 判定与落盘之间的时间窗（TOCTOU，见下） |
//!
//! 🔴 **〔波 5 ㈢ · 2026-09-23〕上面那句「不许落进 Claude 那几棵树」的射程被用户改窄了。**
//!
//! 用户 09-23 逐字：「文件管理器该不该能改 `~/.claude` 里的东西. **可以.**」
//! ⇒ `设计/60 §8.7` 那道「两道栅栏宽窄不同」的产品题按**丙**（统一成同一个判定）裁，
//! 统一到**窄的那一档**：只拦那几份具体的会话文件（`projects/<proj>/<sid>.jsonl` 恰 2 段 ·
//! `sessions/<x>.json` 恰 1 段），于是 skills · 配置 · 账号库**改得动**。
//!
//! | | 09-23 之前 | 今天 |
//! |---|---|---|
//! | 判定 | `is_inside_tree`（拒**整棵 `~/.claude*` 树**） | `is_protected_session_path`（只拒那几份会话文件） |
//! | `~/.claude/skills/x.md` | **拒** | **放行** |
//! | `~/.claude/settings.json` | **拒** | **放行**（与桥那一侧逐字同口径） |
//! | `~/.claude/projects/-x/s.jsonl` | 拒 | **照旧拒** |
//! | 依赖「此刻选中的是哪个账号」 | **是**（`resolve_home` 现打解析配置根） | **不是** —— 判定是纯结构的 |
//!
//! ⚠ 最后那一行顺带结掉了本模块头注下面「诚实边界」第 2 条的一半：
//! 写侧围栏从此**不问**配置根在哪，`CLAUDE_CONFIG_DIR` 指到哪都判得一样。
//!
//! ⚠ **「哪几份文件算 Claude 的会话数据」这条知识不在本模块**：`control/` 是通用层，
//! 它不该知道那个目录叫什么（`agent_locality_guard` 的针就钉在这上面）。
//! 判定的住址是适配层里的 `is_protected_session_path`，本模块只是**调用它**。
//!
//! # 🔴 诚实边界 —— 本模块买到的与**买不到**的
//!
//! 1. **TOCTOU 仍在**：围栏② 解析父目录与真正落盘之间有一个时间窗，
//!    有人在这个窗里把父目录换成 symlink，围栏② 看到的就是旧真相。
//!    **兜底的是 `O_EXCL` 本身** —— 最后那一段若已存在（含它是一条 symlink），
//!    开文件这一步直接失败，不会跟随过去写。⇒ 窗里能被利用的只剩「父目录整个被换掉」
//!    这一形，而那需要对目标根有写权限的本地攻击者。**如实登记为未闭合。**
//! 2. ✅〔波 5 ㈢ · 2026-09-23 · **本条已假，留原话当墓碑**〕
//!    原话是「**只认得当前这一个配置根**：账号隔离（cc-acct-iso）靠切那个环境变量，
//!    盘上可以同时有好几个账号目录，而配置根解析只答得出**此刻这一个**。
//!    另外那几个靠「路径里有一段以那个名字开头」这条形状兜，**换个目录名就兜不住**」。
//!    换成结构判定之后，写侧围栏**根本不问配置根在哪** ⇒ 那一维不存在了。
//!    🔴 **但别把它读成「围栏变强了」**：它同时变窄了很多（见上面那张表）——
//!    结掉的是「判定依赖一个会变的全局状态」这条缺陷，付出的是「整棵树」那一档的拦截面，
//!    而那一格是**用户裁的**，不是这一刀省下来的。
//! 3. **没有真远端**：本模块整个是本机文件系统上的路径算术 ＋ 一次落盘，
//!    判据也全在临时目录上跑。「在一台真远端机器上跑过」这件事**本轮买不到**，
//!    判据头注里逐条写着哪几格是判不了的。
//! 4. **它今天没有调用方**：本轮只落「模块 ＋ 围栏 ＋ 判据」这三样（`99 §4` 步 23b 的射程）。
//!    把它接到命令面上要加子命令 ⇒ 要 bump `BUILD_ID` ⇒ 要同拍 re-embed（`99 §4` 条 19c），
//!    那几处全在本轮写区之外。**「能力在、还没接线」这件事不许被读成「已经能用了」。**

use crate::agents::claudecode::paths::is_protected_session_path;
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
/// - 过了上面几关之后，再判一次**写点自己是不是一份会话文件**。
///   ⚠ 〔波 5 ㈢〕这一关此前判的是「目标根自己有没有落在那几棵树里」——
///   用户 09-23 那一裁之后那一问**不再是拒绝理由**（把文件管理目标指到 `~/.claude`
///   底下是合法的），换成的是「拼出来的那条路径是不是那几份具体的会话文件」。
pub fn fence_lexical(root: &Path, rel: &str) -> Result<PathBuf, String> {
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
    if is_protected_session_path(&target) {
        return Err(format!(
            "refuse write: 写点就是一份 Claude 会话数据文件（{}）——\
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
/// 然后把「在不在目标根底下」与「是不是那几份会话文件」**在真路径上各判一次**。
/// `root` 自己也解一次：两边都解完再比，才比得对。
///
/// 🔴 〔波 5 ㈢〕本函数此前还收一个 `claude_home` 并把它也解一次 —— 那是
/// 「整棵树」那一档的需要。换成结构判定之后**配置根这个入参整个不需要了**：
/// 判定只看路径的段形状，`CLAUDE_CONFIG_DIR` 指到哪都判得一样。
/// ⇒ 少一个入参不是整理，是**少一条会错的依赖**。
///
/// ⚠ 父目录**必须已经在盘上**。本模块不建目录（那是白名单层明令禁止的），
/// 所以「父目录不在」是一条正常的拒绝理由，不是内部错误。
pub fn fence_resolved(root: &Path, target: &Path) -> Result<PathBuf, String> {
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
    // ★ **顺序是承重的**：先问是不是会话文件，再问有没有跑出目标根。
    //   一条 symlink 常常同时犯两样（指出去、而且指到一份会话文件上），两条判定谁先答，
    //   决定了用户看到的是哪一句。**「碰了 Claude 会话数据」这句更要紧**，
    //   它说的是这条围栏立在这里的**全部理由**；「跑出目标根」只是越界。
    //   ⚠ 反过来排也仍然会拒 —— 但诊断会把最要紧的那件事盖掉。
    if is_protected_session_path(&resolved) {
        return Err(format!(
            "refuse write: 解完 symlink 之后写点落到了一份 Claude 会话数据文件上（{}）",
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
    let lexical = fence_lexical(root, rel)?;
    fence_resolved(root, &lexical)
}

/// 一次落盘没成，**是谁拦的**。
///
/// # 🔴 为什么要分这两档（不是分类癖，是线上那一面分得出码才有意义）
///
/// 接命令面那一拍逼出来的：围栏拒绝与盘上出错是**两件对调用方意义完全不同**的事 ——
/// 前者是「这条路径本来就不许写」（换条路径才有意义），后者是「路径没问题，
/// 这一次没写成」（重试才有意义）。此前两者都压成一个 `String`，
/// 线上那一面只能靠**猜字符串前缀**去分它们，而那是会漂的。
///
/// ⚠ 分档**不放宽任何东西**：两档都是 `Err`，两档都不落盘。
#[derive(Debug)]
pub enum WriteRefusal {
    /// 围栏拦的（词法那道 或 解完 symlink 那道）。
    Fenced(String),
    /// 围栏放行了，盘上这一步没成（目标已存在 · 父目录不可写 · 盘满 …）。
    Io(String),
}

impl WriteRefusal {
    /// 线上错误码。**闭集两个**，与 [`MANAGE_COMMANDS`] 那一栏逐字对得上。
    pub fn code(&self) -> &'static str {
        match self {
            WriteRefusal::Fenced(_) => "refused",
            WriteRefusal::Io(_) => "io_failed",
        }
    }

    /// 给人看的那句话（原样来自围栏／系统，本层不改写）。
    pub fn message(&self) -> &str {
        match self {
            WriteRefusal::Fenced(m) | WriteRefusal::Io(m) => m.as_str(),
        }
    }
}

/// **唯一的写盘处**：在用户指定的目标根底下，新建一份此前不存在的文件。
///
/// `create_new(true)` = `O_EXCL`：目标已经在了（哪怕它只是一条 symlink）就失败，
/// 绝不跟随、绝不覆盖。这正是 `D1` 收窄后那条铁律的误差项 ——
/// **新增一份此前不存在的文件，不算「改动用户既有数据」。**
///
/// 返回真正落盘的那个绝对路径（解完 symlink 的）。
pub fn create_new_file(root: &Path, rel: &str, bytes: &[u8]) -> Result<PathBuf, WriteRefusal> {
    use std::io::Write as _;
    let target = fenced_target(root, rel).map_err(WriteRefusal::Fenced)?;
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&target)
        .map_err(|e| {
            WriteRefusal::Io(format!("refuse write: 新建 {} 失败：{e}", target.display()))
        })?;
    // 写失败（盘满等）也要把原因带回去 —— 静默的半截文件比报错糟得多。
    f.write_all(bytes).map_err(|e| {
        WriteRefusal::Io(format!("refuse write: 写 {} 失败：{e}", target.display()))
    })?;
    Ok(target)
}

// ══════════════════════════════════════════════════════════════════════════
//  改动既有数据的那五件 ——〔波 5 ㈡ · 2026-09-23〕`设计/60 §8.6` **第 3 步**
// ══════════════════════════════════════════════════════════════════════════
//
// 🔴 **这一节才花掉用户那句话**：「现在只允许后端的文件管理部分写文件」。
//   它们每一件都会改动盘上已经在的东西 ⇒ 本模块因此从白名单层搬到第三层。
//
// 🔴 **每个函数的第一件事是过围栏** —— 那不是风格，是 `readonly_guard` 第三层
//   逐函数扫出来的：函数里第一个改动动词之前，必须已经出现一次围栏调用。
//   换一种写法（先动手、后判），那一层当场红。
//
// ⚠ 围栏分两个入口，差别只在**最后那一段解不解**：
//   · [`fenced_target`] —— 只解父目录。给「作用在**链接本身**上」的动词用
//     （新建 · 建目录 · 改名 · 删除 —— 它们都不跟最后那一段的链接）。
//   · [`fenced_existing`] —— 连最后那一段也解。给「**跟链接**」的动词用
//     （改权限 · 覆盖写）：不解的话，根里一条指向会话文件的链接就能把那份文件改坏。
//
// ⚠ **TOCTOU 照旧在**（同本模块头注诚实边界第 1 条）：判定与动手之间有一个窗。
//   第三层钉的是「先判后动」这个**顺序**，钉不了「判完之后世界没变」。如实登记。

/// 围栏③：对一个**已经在盘上**的东西做一次**会跟链接**的写之前，把它**解到底**再判一次。
///
/// 先走 [`fenced_target`]（词法 ＋ 父目录解开），再把**完整路径**解成真路径：
/// 解出来是一份会话文件 ⇒ 拒；解出来跑出了目标根 ⇒ 拒。返回解到底的那一个 ——
/// 动手就动它，不再经过任何一条链接。
pub fn fenced_existing(root: &Path, rel: &str) -> Result<PathBuf, String> {
    let at = fenced_target(root, rel)?;
    let real = std::fs::canonicalize(&at)
        .map_err(|e| format!("refuse write: 目标解析不了（{}：{e}）", at.display()))?;
    if is_protected_session_path(&real) {
        return Err(format!(
            "refuse write: 解到底之后落到了一份 Claude 会话数据文件上（{}）",
            real.display()
        ));
    }
    let real_root = std::fs::canonicalize(root)
        .map_err(|e| format!("refuse write: 目标根解析不了（{}：{e}）", root.display()))?;
    if !real.starts_with(&real_root) {
        return Err(format!(
            "refuse write: 解到底之后跑出了目标根（{} 不在 {} 里）",
            real.display(),
            real_root.display()
        ));
    }
    Ok(real)
}

/// 新建一个目录。**只建最后那一段**：父目录不在 ⇒ 围栏② 那一步就拒
/// （「顺手把中间几层补出来」是另一件事，没人裁过）。
pub fn make_dir(root: &Path, rel: &str) -> Result<PathBuf, WriteRefusal> {
    let target = fenced_target(root, rel).map_err(WriteRefusal::Fenced)?;
    std::fs::create_dir(&target).map_err(|e| {
        WriteRefusal::Io(format!(
            "refuse write: 建目录 {} 失败：{e}",
            target.display()
        ))
    })?;
    Ok(target)
}

/// 改名 / 同根内移动。
///
/// 🔴 **两个参数各过一遍围栏** —— 只判 `from` 的话，能把任意文件**改名成**一份会话文件的名字，
/// 盖掉正在跑的那一场（桥那一侧 `sftp_rename` 当初就是少了这一道被逮住的）。
///
/// 🔴 **目标已经在了就拒**：unix 上系统那一步**会静默顶掉**已有的目标文件 —— 那就是一次
/// 没人问过的覆盖。⇒ 先看一眼（不跟链接地看），在就拒。
/// ⚠ 看与改之间有一个窗（TOCTOU）；原子的「不许顶掉」要平台专有的调用，本刀没做，如实登记。
pub fn rename_entry(root: &Path, from: &str, to: &str) -> Result<PathBuf, WriteRefusal> {
    let src = fenced_target(root, from).map_err(WriteRefusal::Fenced)?;
    let dst = fenced_target(root, to).map_err(WriteRefusal::Fenced)?;
    if std::fs::symlink_metadata(&dst).is_ok() {
        return Err(WriteRefusal::Io(format!(
            "refuse write: 目标已经在了（{}）—— 改名不覆盖，先删掉它或换个名字",
            dst.display()
        )));
    }
    std::fs::rename(&src, &dst).map_err(|e| {
        WriteRefusal::Io(format!(
            "refuse write: 改名 {} → {} 失败：{e}",
            src.display(),
            dst.display()
        ))
    })?;
    Ok(dst)
}

/// 删一个文件或一个**空**目录。**删的是链接本身**（不跟过去）。
///
/// 🔴 **递归删刻意没做，不是漏了**：围栏的射程是**一条路径**，而递归删动的是一整棵子树 ——
/// 根里一个普通目录底下可以藏着一份会话文件（有人把 `projects/` 拷进了文件管理目标），
/// 顶上那一条路径过得了围栏，底下那一份却会被一起删掉。
/// 要做就得逐条目过一遍围栏，而那一遍与真删之间的窗是**整趟遍历**那么长。
/// ⇒ 那是一个新形状，要单独论证；本模块今天删非空目录 ⇒ 系统报错、原样带回。
pub fn delete_entry(root: &Path, rel: &str) -> Result<PathBuf, WriteRefusal> {
    let target = fenced_target(root, rel).map_err(WriteRefusal::Fenced)?;
    let is_dir = std::fs::symlink_metadata(&target)
        .map(|m| m.is_dir())
        .map_err(|e| WriteRefusal::Io(format!("refuse write: 读不到 {}：{e}", target.display())))?;
    let done = if is_dir {
        std::fs::remove_dir(&target)
    } else {
        std::fs::remove_file(&target)
    };
    done.map_err(|e| WriteRefusal::Io(format!("refuse write: 删 {} 失败：{e}", target.display())))?;
    Ok(target)
}

/// 改 unix 权限位（只收低 12 位）。**跟链接** ⇒ 走 [`fenced_existing`]。
///
/// ⚠ 非 unix 平台上**如实回失败**，不假装改成了（`Permissions` 在那边只有一个只读位）。
pub fn change_mode(root: &Path, rel: &str, mode: u32) -> Result<PathBuf, WriteRefusal> {
    let real = fenced_existing(root, rel).map_err(WriteRefusal::Fenced)?;
    if mode > 0o7777 {
        return Err(WriteRefusal::Fenced(format!(
            "refuse write: 权限位只收低 12 位（给的是 {mode:o}）"
        )));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(&real, std::fs::Permissions::from_mode(mode)).map_err(|e| {
            WriteRefusal::Io(format!("refuse write: 改权限 {} 失败：{e}", real.display()))
        })?;
        Ok(real)
    }
    #[cfg(not(unix))]
    {
        Err(WriteRefusal::Io(format!(
            "refuse write: 这个平台没有 unix 权限位，{} 一个字节没动",
            real.display()
        )))
    }
}

/// 覆盖写一份**已经在**的普通文件。**跟链接** ⇒ 走 [`fenced_existing`]。
///
/// ⚠ 只覆盖**普通文件**：目标是目录或不存在 ⇒ 拒。新建一份请走 [`create_new_file`]
/// （那条是 `O_EXCL`，两条路刻意分开 —— 「新建」与「改既有」是两件风险不同的事）。
pub fn overwrite_text(root: &Path, rel: &str, bytes: &[u8]) -> Result<PathBuf, WriteRefusal> {
    let real = fenced_existing(root, rel).map_err(WriteRefusal::Fenced)?;
    let is_file = std::fs::metadata(&real)
        .map(|m| m.is_file())
        .map_err(|e| WriteRefusal::Io(format!("refuse write: 读不到 {}：{e}", real.display())))?;
    if !is_file {
        return Err(WriteRefusal::Fenced(format!(
            "refuse write: {} 不是一份普通文件 —— 覆盖写只收普通文件",
            real.display()
        )));
    }
    std::fs::write(&real, bytes)
        .map_err(|e| WriteRefusal::Io(format!("refuse write: 写 {} 失败：{e}", real.display())))?;
    Ok(real)
}

// ══════════════════════════════════════════════════════════════════════════
//  命令面 ——〔波 5 ㈠ · 2026-09-23〕`设计/60 §8.6` **第 2 步**
// ══════════════════════════════════════════════════════════════════════════
//
// 🔴 **这一步不花用户那句「允许」**，`§8.6` 第 2 步逐字：
//   「它的射程：`O_EXCL` 新建一份**此前不存在**的文件；不删、不改名、不覆盖、不建目录
//    🔴 **`readonly_guard` 一行不动** —— 它早就在白名单上
//    ⇒ 这一步只是把一个做好了没接的东西接上」。
//   本节兑现的就是那一句：上面那些函数**一个字节没改射程**，
//   下面加的全是「把参数收进来、把答案交出去」。
//
// 🔴 **「窄」不是靠命令面窄 —— 这一条是现打改过来的，原来那句话是假的。**
//   本节第一版写的是「只上帧面、不上 CLI 面」。**做不到**：
//   `cli_control::cli_exposed` 逐字是 `!matches!(spec.run, Run::Builtin)` ——
//   一条命令进了 `inbound::REGISTRY` 且不是那个硬臂，CLI 面就**自动**认得它
//  （现打：加完之后 `every_cli_exposed_command_is_in_the_query_mode_gate` 当场红，
//   逐字点名 `["--files-create"]`）。⇒ 两个宿主共用同一个 `run`，
//   那正是 `设计/60 §8.1` 说的「一份代码两种宿主」。
//   ⇒ **窄在别的地方**：`readonly_guard` 第三层判的是「**谁引用得到这个模块**」
//   （= 只有这一面的登记入口），不是「谁发得出命令」。别把两者混起来读。
//
// ⚠ 本节**买不到**什么：接上的是「命令面够得到这份原语」这一跳。
//   「真远端那台机器上跑过」本轮**没有**（同本模块头注第 3 条），
//   本机跑得到的是本机文件系统上的那一趟。

/// 这一面回一条什么：成功交 JSON，失败交 `(code, message)`。
///
/// ⚠ 与 `inbound::CmdResult` / `files::Answer` **逐字同形**（一进一出、
/// `(码, 话)` 的错误信封）——接线那一拍才不用改形状。
pub type Answer = Result<serde_json::Value, (&'static str, String)>;

/// 文件管理**写**面的一条线上命令。
///
/// # ⚠ 它**刻意不叫** `Capability`，也刻意不进 `lib.rs::CAPABILITY_FACES`
///
/// `设计/96 §2` 那份汇总清单是**产品面**的登记（`CapabilityKind` 两类、
/// 逐 target 的对等断言），加一个面要动 `lib.rs::CAPABILITY_FACES` ——
/// 那处**在本刀写区之外**，已如实报备。
/// ⇒ 本表今天只当「线上契约的数据形态」用（判据按它对拍 `inbound::REGISTRY`
/// 与 `IPC-PROTOCOL.md §10`），**别把它读成「这一面已经进了能力清单」**。
pub struct ManageCommand {
    /// 线上命令名（连字符那一套，与 `inbound::REGISTRY` 逐字相同）。
    pub name: &'static str,
    /// 它做什么。
    pub what: &'static str,
    /// 入方向参数名。
    pub args: &'static [&'static str],
    /// 出方向字段名。
    pub fields: &'static [&'static str],
    /// 本命令自己可能回的 code。
    pub codes: &'static [&'static str],
}

/// 🔴 **文件管理写面的唯一住址。**
///
/// `readonly_guard` 第三层那条「只从声明过的那一面来」判的就是这张表 ——
/// 表里没有的名字，`inbound::REGISTRY` 上也不许有对应的一条。
pub const MANAGE_COMMANDS: &[ManageCommand] = &[
    ManageCommand {
        name: "files-create",
        what: "在用户指定的文件管理目标根底下，新建一份**此前不存在**的文件（`O_EXCL`）",
        args: &["content", "rel", "root"],
        fields: &["bytes", "path"],
        codes: &["bad_args", "bad_path", "io_failed", "refused"],
    },
    // ── 〔波 5 ㈡ 09-23〕`设计/60 §8.6` 第 3 步：**改动既有数据**的那五件 ──────────
    ManageCommand {
        name: "files-mkdir",
        what: "新建一个目录（只建最后那一段；父目录不在就失败，不顺手补）",
        args: &["rel", "root"],
        fields: &["path"],
        codes: &["bad_args", "bad_path", "io_failed", "refused"],
    },
    ManageCommand {
        name: "files-rename",
        what: "改名 / 同根内移动；**两个参数各过一遍围栏**，目标已存在就拒（不覆盖）",
        args: &["from", "root", "to"],
        fields: &["path"],
        codes: &["bad_args", "bad_path", "io_failed", "refused"],
    },
    ManageCommand {
        name: "files-delete",
        what: "删一个文件或一个**空**目录（不递归；删的是链接本身，不跟过去）",
        args: &["rel", "root"],
        fields: &["path"],
        codes: &["bad_args", "bad_path", "io_failed", "refused"],
    },
    ManageCommand {
        name: "files-chmod",
        what: "改 unix 权限位（低 12 位）；**跟链接**，所以落点解到底再判一次",
        args: &["mode", "rel", "root"],
        fields: &["mode", "path"],
        codes: &["bad_args", "bad_path", "io_failed", "refused"],
    },
    ManageCommand {
        name: "files-write-text",
        what: "覆盖写一份**已经在**的普通文件；**跟链接**，所以落点解到底再判一次",
        args: &["content", "rel", "root"],
        fields: &["bytes", "path"],
        codes: &["bad_args", "bad_path", "io_failed", "refused"],
    },
];

/// 本面声明的线上命令名。
pub fn manage_command_names() -> Vec<&'static str> {
    MANAGE_COMMANDS.iter().map(|c| c.name).collect()
}

/// 取一个**路径**参数（字符串 或 `{"b16": …}`，与 `files-read` 那一族同一口径）。
fn path_of(
    args: &serde_json::Value,
    key: &str,
) -> Result<std::path::PathBuf, (&'static str, String)> {
    let v = args.get(key).ok_or((
        "bad_path",
        format!("少了 `{key}` —— 它要么是一个字符串，要么是 `{{\"b16\": \"<十六进制>\"}}`"),
    ))?;
    let bytes = crate::files::raw::from_json(v).ok_or((
        "bad_path",
        format!(
            "`{key}` 的形状不对 —— 只认字符串或 `{{\"b16\": \"<十六进制>\"}}`；\
             这里刻意不「尽力而为」地猜，猜错一个字节就是往另一个地方落盘"
        ),
    ))?;
    if bytes.is_empty() {
        return Err(("bad_path", format!("`{key}` 是空的")));
    }
    Ok(crate::files::raw::to_path_buf(&bytes))
}

/// `files-create` —— 把两道围栏与那一处 `O_EXCL` 落盘接到线上。
///
/// ⚠ `rel` **只收 UTF-8 字符串**，这是一条真实的局限而不是疏忽：
/// [`fence_lexical`] 的入参就是 `&str`（它要逐段判上跳 / 盘符 / 空段），
/// 把它改成收裸字节是**动围栏本体**，不是接线该顺手做的事。⇒ 如实登记。
/// 目标**根**那一侧没有这个限制（它走 `b16` 那条路）。
fn answer_create(args: &serde_json::Value) -> Answer {
    let root = path_of(args, "root")?;
    let rel = args
        .get("rel")
        .and_then(serde_json::Value::as_str)
        .ok_or((
            "bad_args",
            "少了 `rel`，或者它不是一个字符串 —— 这一格**只收 UTF-8**（围栏本体按段判，\
             入参就是 `&str`）。非 UTF-8 的名字本面今天做不到，如实说，不猜"
                .to_string(),
        ))?
        .to_string();
    // 不给 `content` ⇒ 新建一份**空文件**（那正是「新建空文件」这件事的形状）。
    let bytes = match args.get("content") {
        None => Vec::new(),
        Some(v) => crate::files::raw::from_json(v).ok_or((
            "bad_args",
            "`content` 的形状不对 —— 只认字符串或 `{\"b16\": \"<十六进制>\"}`".to_string(),
        ))?,
    };
    let landed = create_new_file(&root, &rel, &bytes).map_err(|e| {
        // 🔴 码由那个枚举自己答，**不在这里猜字符串前缀** —— 理由住 [`WriteRefusal`]。
        (e.code(), e.message().to_string())
    })?;
    Ok(serde_json::json!({
        "path": crate::files::raw::to_json(crate::files::raw::path_bytes(&landed)),
        "bytes": bytes.len(),
    }))
}

/// 取一个**相对段**参数（只收 UTF-8，理由同 [`answer_create`] 头注）。
fn rel_of(args: &serde_json::Value, key: &str) -> Result<String, (&'static str, String)> {
    args.get(key)
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
        .ok_or((
            "bad_args",
            format!("少了 `{key}`，或者它不是一个字符串 —— 这一格只收 UTF-8（围栏本体按段判）"),
        ))
}

/// 把一个落点交回去（原始字节形）。
fn path_json(p: &Path) -> serde_json::Value {
    crate::files::raw::to_json(crate::files::raw::path_bytes(p))
}

/// 一次拒绝交回线上：码由那个枚举自己答（理由住 [`WriteRefusal`]）。
fn refusal(e: WriteRefusal) -> (&'static str, String) {
    (e.code(), e.message().to_string())
}

fn answer_mkdir(args: &serde_json::Value) -> Answer {
    let root = path_of(args, "root")?;
    let rel = rel_of(args, "rel")?;
    let done = make_dir(&root, &rel).map_err(refusal)?;
    Ok(serde_json::json!({ "path": path_json(&done) }))
}

fn answer_rename(args: &serde_json::Value) -> Answer {
    let root = path_of(args, "root")?;
    let from = rel_of(args, "from")?;
    let to = rel_of(args, "to")?;
    let done = rename_entry(&root, &from, &to).map_err(refusal)?;
    Ok(serde_json::json!({ "path": path_json(&done) }))
}

fn answer_delete(args: &serde_json::Value) -> Answer {
    let root = path_of(args, "root")?;
    let rel = rel_of(args, "rel")?;
    let done = delete_entry(&root, &rel).map_err(refusal)?;
    Ok(serde_json::json!({ "path": path_json(&done) }))
}

fn answer_chmod(args: &serde_json::Value) -> Answer {
    let root = path_of(args, "root")?;
    let rel = rel_of(args, "rel")?;
    let mode = args
        .get("mode")
        .and_then(serde_json::Value::as_u64)
        .and_then(|m| u32::try_from(m).ok())
        .ok_or((
            "bad_args",
            "少了 `mode`，或者它不是一个非负整数（十进制数值，例如 420 = 0o644）".to_string(),
        ))?;
    let done = change_mode(&root, &rel, mode).map_err(refusal)?;
    Ok(serde_json::json!({ "path": path_json(&done), "mode": mode }))
}

fn answer_write_text(args: &serde_json::Value) -> Answer {
    let root = path_of(args, "root")?;
    let rel = rel_of(args, "rel")?;
    // 🔴 这里**必须给** `content`：不给就把一份既有文件写成空的，那不是一个该有默认值的动作。
    let v = args.get("content").ok_or((
        "bad_args",
        "少了 `content` —— 覆盖写不给默认值（默认成空等于把那份文件清空）".to_string(),
    ))?;
    let bytes = crate::files::raw::from_json(v).ok_or((
        "bad_args",
        "`content` 的形状不对 —— 只认字符串或 `{\"b16\": \"<十六进制>\"}`".to_string(),
    ))?;
    let done = overwrite_text(&root, &rel, &bytes).map_err(refusal)?;
    Ok(serde_json::json!({ "path": path_json(&done), "bytes": bytes.len() }))
}

/// 这一面的**唯一入口**（形状照 `files::answer_wire`）。
///
/// 🔴 分派写成一个对 [`MANAGE_COMMANDS`] 的 `match`，而「表里有、分派没有」
/// 那种静默的不可用由判据钉住（本仓 `p1t-removal-cause` 那次真 bug 就是这一形）。
pub fn answer_wire(wire_name: &str, args: &serde_json::Value) -> Answer {
    match wire_name {
        "files-create" => answer_create(args),
        "files-mkdir" => answer_mkdir(args),
        "files-rename" => answer_rename(args),
        "files-delete" => answer_delete(args),
        "files-chmod" => answer_chmod(args),
        "files-write-text" => answer_write_text(args),
        other => Err(("bad_args", format!("`{other}` 不是文件管理写面的命令"))),
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/control/files_write_tests.rs"]
mod tests;
