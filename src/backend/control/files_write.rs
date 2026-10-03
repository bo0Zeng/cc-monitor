//! **文件管理面的落盘原语** —— 那个
//! 「**带围栏的**白名单模块」，`readonly_guard` 写盘白名单上的第二个洞口。
//!
//! # 🔴〔用户〕**文件管理面不再有任何数据围栏**
//!
//! 用户原话：「**文件管理器全部都可以改. 不需要任何围栏**」。⇒ 会话文件（`projects/<proj>/<sid>.jsonl` ·
//! `sessions/<x>.json`）、项目目录、subagent 记录、tasks 都能新建 · 改名 · 删 · 改权限 · 覆盖 · 复制 · 读改写，
//! 递归删也不再因为树里藏着一份会话文件就整趟拒。
//! **拿掉的**是「不许改这些东西」那一问（从前每一关都问一次适配层的会话形状判定）。
//! **留着的**是路径解析的正确性 —— 它不限制改什么，只保证改的就是 `root ＋ rel` 说的那一格：
//! [`lexical_in_root`]（逐段只许普通段，拼出来仍在根下）· [`resolve_parent_in_root`]（父目录解链接后仍在根下）·
//! [`resolve_existing_in_root`]（跟链接的动词：解到底仍在根下）。三个旧名是 `fence_lexical` / `fence_resolved` /
//! `fenced_existing`，串起来那个旧名 `fenced_target`，今天叫 [`resolve_in_root`]。
//! 唯一还在问会话形状的是**删历史会话**那一条（[`delete_session`]，只收 sid）—— 那不是文件管理器，是历史浏览器的
//! 「删会话」，它问的是「要删的必须**是**一份会话」，方向与从前那道围栏相反，本刀一个字节没动它。
//! 下面凡说到「围栏拦会话文件」的段落都是**历史**，读现状看这一节。
//!
//! # 🔴〔波 5 ㈡〕本模块从白名单层**搬到了第三层**，下面「它刻意不是什么」那一节是**历史**
//!
//! 用户逐字：「**现在只允许后端的文件管理部分写文件**」（收窄的是**主语**，不是动作）。
//! ⇒：新建目录 · 改名 · 删除 · 改权限 · 覆盖写 —— 它们要
//! **改动既有数据**，正是白名单层的判准（「不改既有数据」）所禁的。
//! ⇒ `readonly_guard` 长出**第三层**，判准换成「**改，但每一处都先过路径解析、
//! 且只从声明过的那一面来**」（原话是「先过围栏」），本模块是那一层登记的模块之一。
//!
//! | 那一层钉的 | 怎么钉 |
//! |---|---|
//! | 能用哪几个改动动词 | **闭集**登记（`readonly_guard` 那张表）；表外的改动动词在本模块里照旧红 —— 包括那个「一条路径进、整棵树出」的一步递归删（它的遍历不经过围栏；递归删由既有动词**逐条拼出**，见 [`delete_tree`]） |
//! | 列举之后的改动**在列举之后再过一次路径解析** | 出现目录列举的函数恒等于登记的那几个；其中每一处改动之前、列举之后必须有一次路径解析调用（顶上判一次、底下整摞删 ⇒ 红） |
//! | 每一处改动都先过路径解析 | 本模块每个含改动动词的函数里，路径解析调用必须出现在**第一个改动动词之前** |
//! | 只从文件管理那一面来 | 后端生产树里引用得到本模块的文件，集合**恒等于**登记的那一扇门（`inbound.rs`）；其余任何一面伸手 ⇒ 红 |
//! | `O_EXCL` 那一处没变质 | 开句柄的调用与 `O_EXCL` 仍然逐一配对（新建那条路一个字节没松） |
//!
//! ⚠ 下面原来那几段（「它只会新建……」「它不删、不改名……」）写的是 09-19 那一版的射程，
//! **今天不再成立**；留着是因为「它当初为什么只许新建」那条推理今天仍然是第三层的底子
//! （O_EXCL 那条路**仍然**只新建）。读现状看上面那张表。
//!
//! # 它是什么，以及它**刻意不是**什么
//!
//! 裁过一次：同一轮对话里先说的「把不能写文件的规矩去掉」，
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
//! # 路径解析：两道，各治一种逃逸（原标题「围栏」）
//!
//! 「写点只许落在**用户指定的文件管理目标**下，
//! **不许**落进 Claude 那几棵树」。这一句拆成两道能分别单测的关：
//!
//! | 道 | 住址 | 它拦的是 | 它**拦不住**的是 |
//! |---|---|---|---|
//! | ① 词法 | [`lexical_in_root`] | 上跳段 · 绝对路径 · 盘符 · 空段（「写点本身就是一份会话文件」那一格删了） | 盘上真实的 symlink —— 它根本不碰盘 |
//! | ② 现打 | [`resolve_parent_in_root`] | 目标根里藏一条指向别处的 symlink（**解完再判一次**） | 判定与落盘之间的时间窗（TOCTOU，见下） |
//!
//! 🔴 上面那句的后半「不许落进 Claude 那几棵树」**今天不成立**：写面没有会话数据围栏，skills · 配置 ·
//! 账号库 · 会话记录与别的文件走同一条路（用户原话「文件管理器全部都可以改. 不需要任何围栏」）。
//! 判定是纯结构的，不问配置根在哪，`CLAUDE_CONFIG_DIR` 指到哪都判得一样。
//!
//! ⚠ **「哪几份文件算 Claude 的会话数据」这条知识不在本模块**：`control/` 是通用层，
//! 它不该知道那个目录叫什么（`agent_locality_guard` 的针就钉在这上面）。
//! 判定的住址是适配层里的 `is_session_record_path`；本模块连它的名字都不引，由门经 [`SessionPort`] 递进来 ——今天只剩删会话那一处要它。
//!
//! # 🔴 诚实边界 —— 本模块买到的与**买不到**的
//!
//! 1. **TOCTOU 仍在**：路径解析② 解析父目录与真正落盘之间有一个时间窗，
//!    有人在这个窗里把父目录换成 symlink，解析② 看到的就是旧真相。
//!    **兜底的是 `O_EXCL` 本身** —— 最后那一段若已存在（含它是一条 symlink），
//!    开文件这一步直接失败，不会跟随过去写。⇒ 窗里能被利用的只剩「父目录整个被换掉」
//!    这一形，而那需要对目标根有写权限的本地攻击者。**如实登记为未闭合。**
//! 能用原子原语闭合的已闭合：改名不覆盖（[`rename_no_clobber`]；盘不认那个旗 ⇒ 普通文件 `link ＋ unlink`、目录拒）· 复制与读改写新建先写旁名再不覆盖上位 ·
//!    开文件全程不跟链接（[`opener`]）。仍开着：父目录被整个换掉（要逐段 `openat` 一族）· 递归删 / 复制逐条的窗 · CAS 与换名之间 · 改权限跟链接。
//! 2. ✅〔波 5 ㈢ · **本条已假，留原话当墓碑**〕
//!    原话是「**只认得当前这一个配置根**：账号隔离（cc-acct-iso）靠切那个环境变量，
//!    盘上可以同时有好几个账号目录，而配置根解析只答得出**此刻这一个**。
//!    另外那几个靠「路径里有一段以那个名字开头」这条形状兜，**换个目录名就兜不住**」。
//!    换成结构判定之后，写侧围栏**根本不问配置根在哪** ⇒ 那一维不存在了。
//! 3. **没有真远端**：本模块整个是本机文件系统上的路径算术 ＋ 一次落盘，
//!    判据也全在临时目录上跑。「在一台真远端机器上跑过」这件事**本轮买不到**，
//!    判据头注里逐条写着哪几格是判不了的。
//! 4. **它今天没有调用方**：本轮只落「模块 ＋ 围栏 ＋ 判据」这三样（射程）。
//!    把它接到命令面上要加子命令 ⇒ 要 bump `BUILD_ID` ⇒ 要同拍 re-embed（条 19c），
//!    那几处全在本轮写区之外。**「能力在、还没接线」这件事不许被读成「已经能用了」。**

use copy_core::copy_text;
use std::path::{Component, Path, PathBuf};

/// 路径解析①（词法）：**纯路径算术，不碰盘**。过了就返回「打算写到哪」。
///
/// `rel` 是**相对**目标根的那一段。拒绝的形状逐条：
///
/// - 空串 / 全是空白 —— 没有目标就不该有写。
/// - 绝对路径、Windows 盘符 —— 那不是「根底下的一段」。
/// - 上跳段与当前目录段 —— 上跳是逃出目标根的第一条路；当前目录段本身无害，
///   但留着它就等于承认「这里做路径规范化」，而规范化与安全判定混在一起正是
///   本仓反复踩的那种坑 ⇒ **一律拒，让调用方送干净的段进来。**
///
/// 🔴 这里原来还有一关「写点自己是不是一份会话文件」（再早是「目标根在不在
/// `~/.claude*` 那几棵树里」）。用户原话「**文件管理器全部都可以改. 不需要任何围栏**」⇒ 那一关删了：
/// 本函数只答「这一段拼到根上之后还在不在根底下」，**不问落点是谁的数据**。
///
/// 入参从 `&str` 换成了 `impl AsRef<Path>`：段判定走的是 `Path::components`，
/// 它本来就**不看编码** ⇒ 非 UTF-8 的名字（乱码文件名）照样逐段判；递归删的子项名字
/// 也不一定是 UTF-8，逐条目过路径解析要的正是这一格。判定一个字没变。
pub fn lexical_in_root(root: &Path, rel: impl AsRef<Path>) -> Result<PathBuf, String> {
    let rel = rel.as_ref();
    if rel.to_string_lossy().trim().is_empty() {
        return Err(copy_text("beFilesWrite.path.empty", &[]));
    }
    let shown = rel.display();
    for c in rel.components() {
        match c {
            Component::Normal(_) => {}
            Component::ParentDir => {
                return Err(copy_text(
                    "beFilesWrite.path.parentStep",
                    &[("path", &shown.to_string())],
                ));
            }
            Component::CurDir => {
                return Err(copy_text(
                    "beFilesWrite.path.dotStep",
                    &[("path", &shown.to_string())],
                ));
            }
            Component::RootDir | Component::Prefix(_) => {
                return Err(copy_text(
                    "beFilesWrite.path.absolute",
                    &[("path", &shown.to_string())],
                ));
            }
        }
    }
    let target = root.join(rel);
    // 上面已经把非 Normal 的段全拒了 ⇒ 这一条恒真。**留着它是自证**：
    // 哪天有人往上面那个 match 里加一档放行，这里会当场把后果说出来。
    if !target.starts_with(root) {
        return Err(copy_text(
            "beFilesWrite.path.outsideRoot",
            &[
                ("path", &target.display().to_string()),
                ("root", &root.display().to_string()),
            ],
        ));
    }
    Ok(target)
}

/// 路径解析②（现打）：把**父目录**解析成真路径（解 symlink）之后再判一次落点。
///
/// 解析① 是纯字符串算术，它**看不见盘上的 symlink**：目标根里放一条
/// `docs -> /别处`，`root/docs/x.md` 在词法上完全干净，落盘却落到了根外面。
///
/// 处置：解父目录（不解最后那一段 —— 作用在链接本身上的动词不跟它），
/// 然后在真路径上判一次「还在不在目标根底下」。
/// `root` 自己也解一次：两边都解完再比，才比得对。
///
/// 🔴 这里原来在真路径上还判一次「是不是那几份会话文件」，删了（理由同 [`lexical_in_root`]）。
/// 留下的这一判**不是**「不许改什么」，是「这一次到底改的是哪一份」的正确性：`root ＋ rel` 说的是根底下那一格，
/// 一条藏在半路的链接不许把它换成根外面的另一格。
///
/// 🔴 〔波 5 ㈢〕本函数此前还收一个 `claude_home` 并把它也解一次 —— 那是
/// 「整棵树」那一档的需要。换成结构判定之后**配置根这个入参整个不需要了**：
/// 判定只看路径的段形状，`CLAUDE_CONFIG_DIR` 指到哪都判得一样。
/// ⇒ 少一个入参不是整理，是**少一条会错的依赖**。
///
/// ⚠ 父目录**必须已经在盘上**。本模块不建目录（那是白名单层明令禁止的），
/// 所以「父目录不在」是一条正常的拒绝理由，不是内部错误。
pub fn resolve_parent_in_root(root: &Path, target: &Path) -> Result<PathBuf, String> {
    let real_root = std::fs::canonicalize(root).map_err(|e| {
        copy_text(
            "beFilesWrite.path.unresolved",
            &[("path", &root.display().to_string()), ("e", &e.to_string())],
        )
    })?;
    let parent = target.parent().ok_or_else(|| {
        copy_text(
            "beFilesWrite.path.noParent",
            &[("path", &target.display().to_string())],
        )
    })?;
    let name = target.file_name().ok_or_else(|| {
        copy_text(
            "beFilesWrite.path.noName",
            &[("path", &target.display().to_string())],
        )
    })?;
    let real_parent = std::fs::canonicalize(parent).map_err(|e| {
        copy_text(
            "beFilesWrite.path.unresolved",
            &[
                ("path", &parent.display().to_string()),
                ("e", &e.to_string()),
            ],
        )
    })?;
    let resolved = real_parent.join(name);
    if !real_parent.starts_with(&real_root) {
        return Err(copy_text(
            "beFilesWrite.path.escaped",
            &[
                ("path", &real_parent.display().to_string()),
                ("root", &real_root.display().to_string()),
            ],
        ));
    }
    Ok(resolved)
}

/// 两道路径解析串起来跑一遍，返回最终落点。**不碰盘上的内容，只解路径。**
///
/// 抽成单独一个函数，是为了让「两道真的被串起来了」这件事有一个可直接喂参数的入口
/// —— 判据不必为了验它而每次都真写一份文件。
///
/// 旧名 `fenced_target`：它今天**不拦任何数据**（用户「不需要任何围栏」），
/// 只做词法不越根 ＋ 父目录解链接后仍在根下 ⇒ 改成说它真在做的那件事。
pub fn resolve_in_root(root: &Path, rel: impl AsRef<Path>) -> Result<PathBuf, String> {
    let lexical = lexical_in_root(root, rel)?;
    resolve_parent_in_root(root, &lexical)
}

/// 一次落盘没成，**是谁拦的**。
///
/// # 🔴 为什么要分这两档（不是分类癖，是线上那一面分得出码才有意义）
///
/// 接命令面那一拍逼出来的：参数被拒与盘上出错是**两件对调用方意义完全不同**的事 ——
/// 前者是「这条路径本来就不许写」（换条路径才有意义），后者是「路径没问题，
/// 这一次没写成」（重试才有意义）。此前两者都压成一个 `String`，
/// 线上那一面只能靠**猜字符串前缀**去分它们，而那是会漂的。
///
/// ⚠ 分档**不放宽任何东西**：两档都是 `Err`，两档都不落盘。
#[derive(Debug)]
pub enum WriteRefusal {
    /// 这一次的参数本身不成立（路径解析拒：越根 · 上跳 · 父目录不在 · 解完链接跑出根；
    /// 或者形状不对：不是普通文件 · 权限位越界）。线上码 `refused`。
    /// 旧名 `Fenced`：它今天不代表任何数据围栏，名字跟着线上码走。
    Refused(String),
    /// 参数成立，盘上这一步没成（目标已存在 · 父目录不可写 · 盘满 …）。
    Io(String),
    /// **这个平台上没有这件事**（非 unix 上改 unix 权限位）。
    ///
    /// 与 [`WriteRefusal::Io`] 分开是因为调用方的下一步不同：`io_failed` 说「重试才有意义」，
    /// 而这一档**重试一万次也一样** —— 压成 `io_failed` 就是在说假话。
    /// 线上码 [`NO_UNIX_MODE`] 同时是 `files-chmod` 声明过的命令级码，`lib.rs` 那条
    /// target 轴的现推读的就是它（待拍 3）。
    Unsupported(String),
    /// 读改写的**写那一半**发现：盘上那份已经不是调用方读到的那一份了
    /// （`files-put` 的 `expect` 对不上）⇒ 一个字节没写。调用方该**重读重算**，不是重试同一份。
    Stale(String),
}

/// 命令级码：**这个平台没有 unix 权限位**。只有 `files-chmod` 声明它。
///
/// ⚠ 这是这个字面量在后端的**第二份**（另一份是 `lib.rs::NO_UNIX_MODE`，target 轴现推用），
/// 但它**不是第二个源头** —— 真相是 `inbound::REGISTRY` 里 `files-chmod` 自己登记的 `codes`。
/// 刻意不去借 `lib.rs` 那一份：文件管理后端往外够的边是登记过的闭集（`files/module_boundary_guard`，
/// 「只许依赖 platform / common / 围栏」），为一个码多长一条边不值。
/// 两份逐字相等由 `target_parity_guard::the_unix_mode_axis_agrees_with_what_this_binary_was_compiled_with` 钉着。
pub const NO_UNIX_MODE: &str = "no_unix_mode";

impl WriteRefusal {
    /// 线上错误码。**闭集四个**，与 [`MANAGE_COMMANDS`] 那一栏逐字对得上
    /// （第三个只有 `files-chmod` 会回，也只有它声明；第四个 `stale` 只有 `files-put` 会回
    /// ——`files-delete` 带 `expect` 那一形也回它，也声明了）。
    pub fn code(&self) -> &'static str {
        match self {
            WriteRefusal::Refused(_) => "refused",
            WriteRefusal::Io(_) => "io_failed",
            WriteRefusal::Unsupported(_) => NO_UNIX_MODE,
            WriteRefusal::Stale(_) => "stale",
        }
    }

    /// 给人看的那句话（原样来自路径解析／系统，本层不改写）。
    pub fn message(&self) -> &str {
        match self {
            WriteRefusal::Refused(m)
            | WriteRefusal::Io(m)
            | WriteRefusal::Unsupported(m)
            | WriteRefusal::Stale(m) => m.as_str(),
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
pub fn create_new_file(
    root: &Path,
    rel: impl AsRef<Path>,
    bytes: &[u8],
) -> Result<PathBuf, WriteRefusal> {
    use std::io::Write as _;
    let target = resolve_in_root(root, rel).map_err(WriteRefusal::Refused)?;
    let mut f = opener()
        .write(true)
        .create_new(true)
        .open(&target)
        .map_err(|e| {
            WriteRefusal::Io(copy_text(
                "beFilesWrite.create.failed",
                &[
                    ("path", &target.display().to_string()),
                    ("e", &e.to_string()),
                ],
            ))
        })?;
    if let Some(p) = own_mode(&target, false) {
        f.set_permissions(p).map_err(|e| {
            WriteRefusal::Io(copy_text(
                "beFilesWrite.chmod.failed",
                &[
                    ("path", &target.display().to_string()),
                    ("e", &e.to_string()),
                ],
            ))
        })?;
    }
    // 写失败（盘满等）也要把原因带回去 —— 静默的半截文件比报错糟得多。
    f.write_all(bytes).map_err(|e| {
        WriteRefusal::Io(copy_text(
            "beFilesWrite.write.failed",
            &[
                ("path", &target.display().to_string()),
                ("e", &e.to_string()),
            ],
        ))
    })?;
    Ok(target)
}

// ══════════════════════════════════════════════════════════════════════════
//  改动既有数据的那五件 ——〔波 5 ㈡〕 **第 3 步**
// ══════════════════════════════════════════════════════════════════════════
//
// 🔴 **这一节才花掉用户那句话**：「现在只允许后端的文件管理部分写文件」。
//   它们每一件都会改动盘上已经在的东西 ⇒ 本模块因此从白名单层搬到第三层。
//
// 🔴 **每个函数的第一件事是过路径解析** —— 那不是风格，是 `readonly_guard` 第三层
//   逐函数扫出来的：函数里第一个改动动词之前，必须已经出现一次路径解析调用。
//   换一种写法（先动手、后判），那一层当场红。
//
// ⚠ 路径解析分两个入口，差别只在**最后那一段解不解**：
//   · [`resolve_in_root`] —— 只解父目录。给「作用在**链接本身**上」的动词用
//     （新建 · 建目录 · 改名 · 删除 —— 它们都不跟最后那一段的链接）。
//   · [`resolve_existing_in_root`] —— 连最后那一段也解。给「**跟链接**」的动词用
//     （改权限 · 覆盖写）：不解的话，根里一条指向根外的链接就能把根外那一份改掉。
//
// ⚠ **TOCTOU 照旧在**（同本模块头注诚实边界第 1 条）：判定与动手之间有一个窗。
//   第三层钉的是「先判后动」这个**顺序**，钉不了「判完之后世界没变」。如实登记。

/// 路径解析③：对一个**已经在盘上**的东西做一次**会跟链接**的写之前，把它**解到底**再判一次。
///
/// 先走 [`resolve_in_root`]（词法 ＋ 父目录解开），再把**完整路径**解成真路径：
/// 解出来跑出了目标根 ⇒ 拒。返回解到底的那一个 —— 动手就动它，不再经过任何一条链接。
/// 旧名 `fenced_existing`；「解出来是一份会话文件 ⇒ 拒」那一判删了。
pub fn resolve_existing_in_root(root: &Path, rel: impl AsRef<Path>) -> Result<PathBuf, String> {
    let at = resolve_in_root(root, rel)?;
    let real = std::fs::canonicalize(&at).map_err(|e| {
        copy_text(
            "beFilesWrite.path.unresolved",
            &[("path", &at.display().to_string()), ("e", &e.to_string())],
        )
    })?;
    let real_root = std::fs::canonicalize(root).map_err(|e| {
        copy_text(
            "beFilesWrite.path.unresolved",
            &[("path", &root.display().to_string()), ("e", &e.to_string())],
        )
    })?;
    if !real.starts_with(&real_root) {
        return Err(copy_text(
            "beFilesWrite.path.escaped",
            &[
                ("path", &real.display().to_string()),
                ("root", &real_root.display().to_string()),
            ],
        ));
    }
    Ok(real)
}

/// 〔「全程 `O_NOFOLLOW`」〕第三层开文件**只有这一个出处**：unix 上带 `O_NOFOLLOW` ——
/// 最后一段是链接 ⇒ 开就失败，解析过的路径在开之前被换成一条链接也跟不过去。Windows 没有等价的开法（`platform::fs` 头注），照旧。
/// `readonly_guard` 第三层钉着：五个模块里 `OpenOptions::new()` 只在这里出现，每一次开都挂在它后面。
pub(crate) fn opener() -> std::fs::OpenOptions {
    #[allow(unused_mut)]
    let mut o = std::fs::OpenOptions::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        o.custom_flags(crate::platform::fs::NO_FOLLOW);
    }
    o
}

/// 读一整份，不跟最后那一段的链接（[`opener`]）。
pub(crate) fn read_nofollow(p: &Path) -> std::io::Result<Vec<u8>> {
    use std::io::Read as _;
    let mut buf = Vec::new();
    opener().read(true).open(p)?.read_to_end(&mut buf)?;
    Ok(buf)
}

/// **不覆盖地改名** `root ＋ from` → `root ＋ to`：两条各过路径解析，然后一次
/// `platform::fs::rename_noreplace` —— 目标已在（含一条链接）⇒ `AlreadyExists`、一个字节不动，没有先看后改的窗。
/// 回 `(源, 目标, 那一下的结局)`；路径解析拒 ⇒ `Err`。
///
/// 那块盘不认这个旗（NFS · 部分 FUSE · 老内核 · 别的 unix）⇒ 不退回先看后改：
/// 普通文件（含链接本身）走 `platform::fs::rename_by_link`（`link` 在目标已在时原子失败，NFS 上也成立）；
/// 目录没有这条路 ⇒ 拒、出声（`Err`，说「这个盘不支持不覆盖改名目录」）。
/// `between` 是判据插竞争用的口（解析之后、动手之前；生产传空）；`force_link` 让判据走那块盘的那一支（生产传 `false`）。
fn rename_no_clobber(
    root: &Path,
    from: &Path,
    to: &Path,
    between: &mut dyn FnMut(),
    force_link: bool,
) -> Result<(PathBuf, PathBuf, std::io::Result<()>), String> {
    let src = resolve_in_root(root, from)?;
    let dst = resolve_in_root(root, to)?;
    between();
    if !force_link {
        match crate::platform::fs::rename_noreplace(&src, &dst) {
            Err(e) if crate::platform::fs::noreplace_unsupported(&e) => {}
            other => return Ok((src, dst, other)),
        }
    }
    if std::fs::symlink_metadata(&src).is_ok_and(|m| m.is_dir()) {
        return Err(copy_text(
            "beFilesWrite.rename.dirNoNoreplace",
            &[("path", &src.display().to_string())],
        ));
    }
    tracing::warn!(
        "改名 {} → {}：这块盘不认「不覆盖改名」，普通文件改走 link ＋ unlink",
        src.display(),
        dst.display()
    );
    let done = crate::platform::fs::rename_by_link(&src, &dst);
    Ok((src, dst, done))
}

/// 新建一个目录。**只建最后那一段**：父目录不在 ⇒ 路径解析② 那一步就拒
/// （「顺手把中间几层补出来」是另一件事，没人裁过）。
pub fn make_dir(root: &Path, rel: impl AsRef<Path>) -> Result<PathBuf, WriteRefusal> {
    let target = resolve_in_root(root, rel).map_err(WriteRefusal::Refused)?;
    std::fs::create_dir(&target).map_err(|e| {
        WriteRefusal::Io(copy_text(
            "beFilesWrite.copyTree.mkdirFailed",
            &[
                ("path", &target.display().to_string()),
                ("e", &e.to_string()),
            ],
        ))
    })?;
    if let Some(p) = own_mode(&target, true) {
        std::fs::set_permissions(&target, p).map_err(|e| {
            WriteRefusal::Io(copy_text(
                "beFilesWrite.chmod.failed",
                &[
                    ("path", &target.display().to_string()),
                    ("e", &e.to_string()),
                ],
            ))
        })?;
    }
    Ok(target)
}

/// 这台机器上后端自家目录的权限位：新建在 `~/.cc-monitor` 里的文件只给本人读写。
const OWN_FILE_MODE: u32 = 0o600;

/// **新建**在这台机器自家目录（`$HOME/.cc-monitor`，含它本身）里的东西只给本人：文件 [`OWN_FILE_MODE`] · 目录
/// `own_dir::PRIVATE_DIR_MODE`（unix；别的平台照常建，那边按父目录继承 ACL）。只管新建：覆盖写沿用原权限位、复制照抄源的。
/// `at` 是过了路径解析的那一条（父目录已解开）。
/// 答不出家在哪 ⇒ 不收紧。
fn own_mode(at: &Path, dir: bool) -> Option<std::fs::Permissions> {
    let home = crate::platform::paths::home_dir()?;
    let home = std::fs::canonicalize(&home).unwrap_or(home);
    if !at.starts_with(home.join(crate::common::own_dir::DIR_NAME)) {
        return None;
    }
    let mode = if dir {
        crate::common::own_dir::PRIVATE_DIR_MODE
    } else {
        OWN_FILE_MODE
    };
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        Some(std::fs::Permissions::from_mode(mode))
    }
    #[cfg(not(unix))]
    {
        let _ = mode;
        None
    }
}

/// 改名 / 同根内移动。
///
/// 🔴 **两个参数各过一遍路径解析** —— 只解 `from` 的话，`to` 那一侧半路一条链接就能把东西搬到根外面去。
/// 从前这一句的理由是「能把任意文件改名成一份会话文件的名字」—— 那一道拦截用户拿掉了。
///
/// 🔴 **目标已经在了就拒**：unix 上系统那一步**会静默顶掉**已有的目标文件 —— 那就是一次
/// 没人问过的覆盖。从「先看一眼、在就拒」换成一次原子的不覆盖改名（[`rename_no_clobber`]）：看与改之间那个窗没了。
pub fn rename_entry(
    root: &Path,
    from: impl AsRef<Path>,
    to: impl AsRef<Path>,
) -> Result<PathBuf, WriteRefusal> {
    rename_entry_racing(root, from.as_ref(), to.as_ref(), &mut || {})
}

/// [`rename_entry`] 的本体；`between` 见 [`rename_no_clobber`]。
fn rename_entry_racing(
    root: &Path,
    from: &Path,
    to: &Path,
    between: &mut dyn FnMut(),
) -> Result<PathBuf, WriteRefusal> {
    let (src, dst, done) =
        rename_no_clobber(root, from, to, between, false).map_err(WriteRefusal::Refused)?;
    match done {
        Ok(()) => Ok(dst),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            Err(WriteRefusal::Io(copy_text(
                "beFilesWrite.rename.exists",
                &[("path", &dst.display().to_string())],
            )))
        }
        Err(e) => Err(WriteRefusal::Io(copy_text(
            "beFilesWrite.rename.failed",
            &[
                ("src", &src.display().to_string()),
                ("dst", &dst.display().to_string()),
                ("e", &e.to_string()),
            ],
        ))),
    }
}

/// 删一个文件或一个**空**目录。**删的是链接本身**（不跟过去）。
///
/// 🔴 **本函数不递归，刻意的**：路径解析的射程是**一条路径**，而递归删动的是一整棵子树 ——
/// 顶上那一条解得干净，底下一条链接或一个挂载点照样会被一起动到。非空目录 ⇒ 系统报错、原样带回。
/// 递归删是**另一个函数**（[`delete_tree`]），逐条目过路径解析；
/// 线上要显式带 `recursive: true` 才走它 —— 不带，本函数的射程一个字节不变。
pub fn delete_entry(root: &Path, rel: impl AsRef<Path>) -> Result<PathBuf, WriteRefusal> {
    let target = resolve_in_root(root, rel).map_err(WriteRefusal::Refused)?;
    let is_dir = std::fs::symlink_metadata(&target)
        .map(|m| m.is_dir())
        .map_err(|e| {
            WriteRefusal::Io(copy_text(
                "beFilesWrite.copyTree.unreadable",
                &[
                    ("path", &target.display().to_string()),
                    ("e", &e.to_string()),
                ],
            ))
        })?;
    let done = if is_dir {
        std::fs::remove_dir(&target)
    } else {
        std::fs::remove_file(&target)
    };
    done.map_err(|e| {
        WriteRefusal::Io(copy_text(
            "beFilesWrite.delete.failed",
            &[
                ("path", &target.display().to_string()),
                ("e", &e.to_string()),
            ],
        ))
    })?;
    Ok(target)
}

/// **带 CAS 的删一份文件**：盘上那份逐字节 == `expect` 才删，否则一个字节不动。
///
/// 读改写那一族（`files-put` 的 `expect`）缺的最后一格：调用方「读到的是这一份 ⇒ 删它」，
/// 从前只能先 `files-peek` 核、再 `files-delete` 删，核与删之间整整一趟往返的窗。
/// 这里把「核」挪到删的同一个进程里、紧贴着删那一下（窗缩到本函数里那两行之间 —— TOCTOU 照旧如实登记，
/// 同本模块头注诚实边界第 1 条）。
///
/// 目标**必须是普通文件**（不跟链接地看）：是目录 / 链接 / 别的 ⇒ `Refused`（CAS 比的是一份文件的字节，
/// 链接的字节是它指向的那一份 —— 删的却是链接本身，两者对不上，不给这一形）。
/// 不在 ⇒ `Stale`（读的时候还在）；在但不等 ⇒ `Stale`。与 [`delete_entry`] 同一道路径解析、同一个删的动词。
pub fn delete_file_expecting(
    root: &Path,
    rel: impl AsRef<Path>,
    expect: &[u8],
) -> Result<PathBuf, WriteRefusal> {
    let target = resolve_in_root(root, rel).map_err(WriteRefusal::Refused)?;
    let md = match std::fs::symlink_metadata(&target) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(WriteRefusal::Stale(copy_text(
                "beFilesWrite.delete.gone",
                &[("path", &target.display().to_string())],
            )))
        }
        Err(e) => {
            return Err(WriteRefusal::Io(copy_text(
                "beFilesWrite.copyTree.unreadable",
                &[
                    ("path", &target.display().to_string()),
                    ("e", &e.to_string()),
                ],
            )))
        }
    };
    if !md.is_file() {
        return Err(WriteRefusal::Refused(copy_text(
            "beFilesWrite.delete.notRegular",
            &[("path", &target.display().to_string())],
        )));
    }
    let current = read_nofollow(&target).map_err(|e| {
        WriteRefusal::Io(copy_text(
            "beFilesWrite.read.failed",
            &[
                ("path", &target.display().to_string()),
                ("e", &e.to_string()),
            ],
        ))
    })?;
    if current.is_empty() && md.len() > 0 {
        return Err(WriteRefusal::Io(format!(
            "refuse write: {}",
            hollow_read(&target, md.len())
        )));
    }
    if current != expect {
        return Err(WriteRefusal::Stale(copy_text(
            "beFilesWrite.delete.changed",
            &[("path", &target.display().to_string())],
        )));
    }
    std::fs::remove_file(&target).map_err(|e| {
        WriteRefusal::Io(copy_text(
            "beFilesWrite.delete.failed",
            &[
                ("path", &target.display().to_string()),
                ("e", &e.to_string()),
            ],
        ))
    })?;
    Ok(target)
}

/// 〔SU1 问 2〕**只删一个空目录**（`files-delete` 带 `expect: {"empty_dir": true}`）：
/// 「我看到的是一个空目录，删它」—— 与带逐字节 `expect` 删文件那一形对称的 CAS。
///
/// - 目标（不跟链接地看）必须是一个**真目录**：是文件 / 链接 / 别的 ⇒ `Refused`（这一形只对目录）。
/// - 不在 ⇒ `Stale`（看的时候还在）；**不空 ⇒ `Stale`**：`remove_dir` 自己就是原子的「空才删」，系统拒非空、
///   本函数只把那一下翻成 `stale`（说的是「你看的时候它空，此刻不空了」）—— 没有先看后删的窗。
/// - 删的动词与 [`delete_entry`] 删空目录那一支同一个（闭集里的「删空目录」），路径解析同一道。
pub fn delete_empty_dir(root: &Path, rel: impl AsRef<Path>) -> Result<PathBuf, WriteRefusal> {
    let target = resolve_in_root(root, rel).map_err(WriteRefusal::Refused)?;
    let md = match std::fs::symlink_metadata(&target) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(WriteRefusal::Stale(copy_text(
                "beFilesWrite.rmdir.gone",
                &[("path", &target.display().to_string())],
            )))
        }
        Err(e) => {
            return Err(WriteRefusal::Io(copy_text(
                "beFilesWrite.copyTree.unreadable",
                &[
                    ("path", &target.display().to_string()),
                    ("e", &e.to_string()),
                ],
            )))
        }
    };
    if !md.is_dir() {
        return Err(WriteRefusal::Refused(copy_text(
            "beFilesWrite.rmdir.notDir",
            &[("path", &target.display().to_string())],
        )));
    }
    match std::fs::remove_dir(&target) {
        Ok(()) => Ok(target),
        Err(e) if e.kind() == std::io::ErrorKind::DirectoryNotEmpty => {
            Err(WriteRefusal::Stale(copy_text(
                "beFilesWrite.rmdir.notEmpty",
                &[("path", &target.display().to_string())],
            )))
        }
        Err(e) => Err(WriteRefusal::Io(copy_text(
            "beFilesWrite.rmdir.failed",
            &[
                ("path", &target.display().to_string()),
                ("e", &e.to_string()),
            ],
        ))),
    }
}

/// `files-delete` 的 `expect` 那一格（给了的话）：逐字节那一形 · 「只删空目录」那一形。
enum DeleteExpect {
    Bytes(Vec<u8>),
    EmptyDir,
}

/// 改 unix 权限位（只收低 12 位）。**跟链接** ⇒ 走 [`resolve_existing_in_root`]。
///
/// ⚠ 非 unix 平台上**如实回失败**，不假装改成了（`Permissions` 在那边只有一个只读位）。
pub fn change_mode(root: &Path, rel: impl AsRef<Path>, mode: u32) -> Result<PathBuf, WriteRefusal> {
    let real = resolve_existing_in_root(root, rel).map_err(WriteRefusal::Refused)?;
    if mode > 0o7777 {
        return Err(WriteRefusal::Refused(copy_text(
            "beFilesWrite.chmod.badMode",
            &[("mode", &format!("{mode:o}"))],
        )));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(&real, std::fs::Permissions::from_mode(mode)).map_err(|e| {
            WriteRefusal::Io(copy_text(
                "beFilesWrite.chmod.failed",
                &[("path", &real.display().to_string()), ("e", &e.to_string())],
            ))
        })?;
        Ok(real)
    }
    #[cfg(not(unix))]
    {
        // 回 `no_unix_mode`，不回 `io_failed`（理由住 [`WriteRefusal::Unsupported`]）。
        Err(WriteRefusal::Unsupported(copy_text(
            "beFilesWrite.chmod.unsupported",
            &[("path", &real.display().to_string())],
        )))
    }
}

/// 覆盖写一份**已经在**的普通文件。**跟链接** ⇒ 走 [`resolve_existing_in_root`]。
///
/// ⚠ 只覆盖**普通文件**：目标是目录或不存在 ⇒ 拒。新建一份请走 [`create_new_file`]
/// （那条是 `O_EXCL`，两条路刻意分开 —— 「新建」与「改既有」是两件风险不同的事）。
///
/// 🔴**原子地换**：走 [`swap_in`]（同目录 `O_EXCL` 暂存旁名 → 写满 → 沿用原权限位 → 换名上位；
/// 最后一段是链接 ⇒ 解到底、改真文件）。覆盖写一律「临时件 ＋ rename」原子化：
/// 此前是就地先截断再写 —— 写到一半失败、或后端在写的中途被收掉，目标剩半份或 0 字节。
/// 今天那两形下目标原封不动（旁边可能剩一份 `.<名>.ccm-put-<pid>-<序>.part`，它不是用户数据）。
/// ⚠ 认下的代价（D-a 的代价， §2.1，「刻意不换语义」那句要随 D0 并入改）：
///   换名之后 inode 换了 ⇒ **硬链接**的另一个名字仍指旧内容 · 目标若属**别的用户**、只是给了我们写权限，换完属主变成我们 ·
///   Linux 上的 **xattr / ACL** 不跟过来。权限位沿用；跨盘不会（旁名与目标同目录）。
///   Windows 臂照旧就地写（`swap_in` 自己那一支，保 ACE —— 认过）。
/// 🔴上面前两格代价**不认**：目标 `nlink > 1`（有硬链接）或属主不是后端这个用户 ⇒
///   **退回就地写**（保住硬链接与属主），并记一行「这一次不是原子写，因为 …」（[`in_place_reason`]）。
///   xattr / ACL 不跟过来那一格仍是已知代价（记录 `HX1.md` §6）。
pub fn overwrite_text(
    root: &Path,
    rel: impl AsRef<Path>,
    bytes: &[u8],
) -> Result<PathBuf, WriteRefusal> {
    let rel = rel.as_ref();
    let real = resolve_existing_in_root(root, rel).map_err(WriteRefusal::Refused)?;
    let md = std::fs::metadata(&real).map_err(|e| {
        WriteRefusal::Io(copy_text(
            "beFilesWrite.copyTree.unreadable",
            &[("path", &real.display().to_string()), ("e", &e.to_string())],
        ))
    })?;
    if !md.is_file() {
        return Err(WriteRefusal::Refused(copy_text(
            "beFilesWrite.overwrite.notRegular",
            &[("path", &real.display().to_string())],
        )));
    }
    let links_owner = links_and_owner(&real);
    if let Some(why) = links_owner.and_then(|(links, owner)| {
        in_place_reason(links, owner, crate::platform::paths::current_uid())
    }) {
        // 就地写：先截断再写（写到一半失败 / 进程在写的中途被收掉 ⇒ 目标剩半份）—— 换来的是硬链接与属主不被拆开。
        tracing::warn!(
            "覆盖写 {}：这一次不是原子写，因为{why}（原子换会把它拆开）—— 改成就地写",
            real.display()
        );
        // 不跟链接地开（[`opener`]）、不顺手新建：解析完之后它被换成链接 / 被删了 ⇒ 开就失败，不写到别处去。
        use std::io::Write as _;
        let wrote = opener()
            .write(true)
            .truncate(true)
            .open(&real)
            .and_then(|mut f| f.write_all(bytes));
        wrote.map_err(|e| {
            WriteRefusal::Io(copy_text(
                "beFilesWrite.write.failed",
                &[("path", &real.display().to_string()), ("e", &e.to_string())],
            ))
        })?;
        return Ok(real);
    }
    swap_in(root, rel, bytes, Some(md.permissions()), false, &mut || {})
}

/// 一份文件的 `(硬链接数, 属主 uid)`（跟链接地看）。**非 unix 上没有这两个概念 ⇒ `None`**（那边照原子换 / Windows 臂办）。
/// 住本模块而不住 `platform/`：读元数据扩展（`MetadataExt`）在只读护栏上是第三层独有的词（同 `kind_and_device`）。
#[cfg(unix)]
fn links_and_owner(p: &Path) -> Option<(u64, u32)> {
    use std::os::unix::fs::MetadataExt as _;
    std::fs::metadata(p).ok().map(|m| (m.nlink(), m.uid()))
}

#[cfg(not(unix))]
fn links_and_owner(_p: &Path) -> Option<(u64, u32)> {
    None
}

/// 这一份目标**不该原子换**的原因（`None` = 该原子换）：换名上位会让 inode 换掉 ⇒
/// 有硬链接（`links > 1`）的另一个名字仍指旧内容；属主不是后端这个用户（只是给了写权限）⇒ 换完属主变成后端用户。
pub(crate) fn in_place_reason(links: u64, owner: u32, me: u32) -> Option<String> {
    let mut parts: Vec<String> = Vec::new();
    if links > 1 {
        parts.push(format!("它有 {links} 个硬链接"));
    }
    if owner != me {
        parts.push(format!(
            "它的属主（uid {owner}）不是后端这个用户（uid {me}）"
        ));
    }
    (!parts.is_empty()).then(|| parts.join("、"))
}

// ══════════════════════════════════════════════════════════════════════════
// CAS 的**摘要形**：`expect: {"sha256": "<64 位小写十六进制>"}`
// ══════════════════════════════════════════════════════════════════════════
//
// 编辑器存盘带 CAS（`expect`），比的是摘要：它**仍是 CAS**（比的是「我看的时候那一份」），
// 只是换了表示 —— 逐字节形（`files-put` / `files-delete`）留给小写。为什么编辑器要摘要形：存盘分块那一支本来就是
// 因为**一行装不下**（一行 1 MiB、编辑上限 8 MiB），逐字节的 `expect` 要么让装得进一行的门槛减半，
// 要么让原文再分块送一遍（大文件存盘流量翻倍）。摘要定长，两支同形。
//
// 🔴 摘要**只在后端算**（读的那一趟 `files-read-text` 交出 `sha256`、写成之后应答里交新的）——窗口当不透明令牌存着再交回来，
//   算法只有一个家：[`crate::files::content_sha256`]（住读族那一侧：读的那一趟要它，而读族不许伸手进写面 ——
//   `readonly_guard` 「写面只有一扇门」那条钉着）。

pub use crate::files::{content_sha256, SHA256_HEX_LEN};

/// 取**必给**的摘要形 `expect`：恰好 `{"sha256": "<64 位小写十六进制>"}`，多一个键、少一个键、串的形状不对都拒（不猜）。
pub fn sha256_expect_of(args: &serde_json::Value) -> Result<String, (&'static str, String)> {
    let v = args.get("expect").ok_or((
        "bad_args",
        crate::common::contract::malformed(
            "missing `expect` ({\"sha256\": ...} handed out by the read)",
        ),
    ))?;
    let bad = || {
        (
            "bad_args",
            format!(
                "`expect` 只收 `{{\"sha256\": \"<{SHA256_HEX_LEN} 位小写十六进制>\"}}`（给的是 {v}）"
            ),
        )
    };
    let obj = v.as_object().ok_or_else(bad)?;
    if obj.len() != 1 {
        return Err(bad());
    }
    let hex = obj
        .get("sha256")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(bad)?;
    if hex.len() != SHA256_HEX_LEN
        || !hex
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(bad());
    }
    Ok(hex.to_string())
}

/// **带 CAS 的覆盖写**：盘上那份的摘要 == `expect_sha256` 才交给 [`overwrite_text`]，否则一个字节不写。
///
/// - 目标不在 ⇒ `Stale`（看的时候还在）；不是普通文件 ⇒ `Refused`（同 [`overwrite_text`]）；
///   「盘上有字节、读出来却是空的」⇒ `Io`（同读改写那一句，继续走就是拿新内容盖掉一份没读到的原文）。
/// - 摘要不等 ⇒ `Stale`：调用方该让人决定（重开 / 仍然覆盖），不是重试同一份。
/// - 比对与写之间仍有窗（TOCTOU，同本模块头注诚实边界第 1 条）：CAS 缩小的是「窗口读 → 后端写」那一整趟往返的窗。
/// - **写法本身不在这里**：比完交给 [`overwrite_text`] 那一个原语（它怎么落盘由那一处定）。
pub fn overwrite_text_expecting(
    root: &Path,
    rel: impl AsRef<Path>,
    bytes: &[u8],
    expect_sha256: &str,
) -> Result<PathBuf, WriteRefusal> {
    let rel = rel.as_ref();
    let at = resolve_in_root(root, rel).map_err(WriteRefusal::Refused)?;
    match std::fs::symlink_metadata(&at) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(WriteRefusal::Stale(copy_text(
                "beFilesWrite.overwrite.gone",
                &[("path", &at.display().to_string())],
            )))
        }
        Err(e) => {
            return Err(WriteRefusal::Io(copy_text(
                "beFilesWrite.copyTree.unreadable",
                &[("path", &at.display().to_string()), ("e", &e.to_string())],
            )))
        }
        Ok(_) => {}
    }
    let real = resolve_existing_in_root(root, rel).map_err(WriteRefusal::Refused)?;
    let md = std::fs::metadata(&real).map_err(|e| {
        WriteRefusal::Io(copy_text(
            "beFilesWrite.copyTree.unreadable",
            &[("path", &real.display().to_string()), ("e", &e.to_string())],
        ))
    })?;
    if !md.is_file() {
        return Err(WriteRefusal::Refused(copy_text(
            "beFilesWrite.overwrite.notRegular",
            &[("path", &real.display().to_string())],
        )));
    }
    let current = read_nofollow(&real).map_err(|e| {
        WriteRefusal::Io(copy_text(
            "beFilesWrite.read.failed",
            &[("path", &real.display().to_string()), ("e", &e.to_string())],
        ))
    })?;
    if current.is_empty() && md.len() > 0 {
        return Err(WriteRefusal::Io(format!(
            "refuse write: {}",
            hollow_read(&real, md.len())
        )));
    }
    if content_sha256(&current) != expect_sha256 {
        return Err(WriteRefusal::Stale(copy_text(
            "beFilesWrite.overwrite.changed",
            &[("path", &real.display().to_string())],
        )));
    }
    overwrite_text(root, rel, bytes)
}

// ══════════════════════════════════════════════════════════════════════════
// **递归删：逐条目过路径解析**（原标题「逐条目过围栏」）
// ══════════════════════════════════════════════════════════════════════════
//
// 设计全文住第一节。三件承重的事：
//
// 1. **不用那个一步递归删的库函数**（它在第三层禁词表上，留在那儿）：它的遍历不经过我们的路径解析，
//    删的是「那一刻盘上的东西」而不是「判过的东西」。
// 2. **两趟**：计划趟（只读）逐条目过路径解析，任一条被拒 ⇒ 整趟拒、一个字节不动；
//    执行趟**只删计划里的**，每一条**当场再过一次**路径解析，只用「删文件」与「删空目录」两个既有动词
//    ⇒ 计划之外新长出来的东西绝不会被连带删掉：它所在的目录不空，删目录那一步失败 ⇒ 停。
// 3. **列举与改动分住两个函数**（[`plan_tree`] 只列不删；[`remove_planned`] 只删一条、先过它自己那一条的路径解析）。
//    `readonly_guard` 第三层有一条判据钉「列举之后的改动，在列举之后必须再过一次路径解析」，
//    顶上判一次、底下整摞删那一形当场红。
//
// ⚠ 每一条的「判」与「删」之间仍然有窗（TOCTOU，同本模块头注诚实边界第 1 条）；
//   两趟之间的窗靠「只删计划里的 ＋ 当场再判」收住。如实登记为未闭合。

/// 一趟递归删最多动多少条（含目标自己）。**超了整趟拒**，不做半截。
///
/// 理由：计划是内存里一张表，而命令在阻塞档（开跑之后取消不掉）⇒ 不给它无界。
/// 这个数不是量出来的最优值，是「一次手势删掉十万条以上」已经不该是文件管理器那一下的事。
pub const TREE_ENTRY_CAP: usize = 100_000;

/// 计划里的一条：相对目标根的那一段 ＋ 计划那一刻它是不是目录（不跟链接地看的）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Planned {
    pub rel: PathBuf,
    pub is_dir: bool,
}

/// 不跟链接地看一眼：`(是不是目录, 所在设备号)`。
///
/// 设备号只在 unix 上有意义（跨挂载点那一判）；别处回常数 0 ⇒ 那一判在那里**不开口**（如实登记）。
#[cfg(unix)]
fn kind_and_device(p: &Path) -> std::io::Result<(bool, u64)> {
    use std::os::unix::fs::MetadataExt as _;
    std::fs::symlink_metadata(p).map(|m| (m.is_dir(), m.dev()))
}

#[cfg(not(unix))]
fn kind_and_device(p: &Path) -> std::io::Result<(bool, u64)> {
    std::fs::symlink_metadata(p).map(|m| (m.is_dir(), 0))
}

/// **计划趟（只读）**：从 `rel` 起、不跟链接地走整棵树，**每一条目各过一次路径解析**。
///
/// 回先序表（每一条都排在它所有后代之前 ⇒ 倒过来就是能逐条删空的次序）。
/// 整趟拒的几形（都**一个字节不动**）：任一条目被路径解析拒（`refused`，话里点名是哪一条）·
/// 树里跨了挂载点（`refused`：不走进另一个文件系统去删）· 条目数超过 [`TREE_ENTRY_CAP`]（`refused`）·
/// 列不出某一层（`io_failed`）。
///
/// 🔴 **本函数里一个改动都没有** —— 列举与改动分住两个函数是第三层那条判据钉的。
pub fn plan_tree(root: &Path, rel: impl AsRef<Path>) -> Result<Vec<Planned>, WriteRefusal> {
    plan_tree_within(root, rel.as_ref(), TREE_ENTRY_CAP)
}

/// [`plan_tree`] 的本体，上限由调用方给（判据拿一个小上限验「超了整趟拒」，不必真铺十万条）。
pub fn plan_tree_within(root: &Path, rel: &Path, cap: usize) -> Result<Vec<Planned>, WriteRefusal> {
    let top = resolve_in_root(root, rel).map_err(WriteRefusal::Refused)?;
    let (top_is_dir, top_dev) = kind_and_device(&top).map_err(|e| {
        WriteRefusal::Io(copy_text(
            "beFilesWrite.copyTree.unreadable",
            &[("path", &top.display().to_string()), ("e", &e.to_string())],
        ))
    })?;
    let mut plan = vec![Planned {
        rel: rel.to_path_buf(),
        is_dir: top_is_dir,
    }];
    let mut pending: Vec<PathBuf> = if top_is_dir {
        vec![rel.to_path_buf()]
    } else {
        Vec::new()
    };
    while let Some(dir_rel) = pending.pop() {
        let dir_at = resolve_in_root(root, &dir_rel).map_err(WriteRefusal::Refused)?;
        let listing = std::fs::read_dir(&dir_at).map_err(|e| {
            WriteRefusal::Io(copy_text(
                "beFilesWrite.copyTree.unlistable",
                &[
                    ("path", &dir_at.display().to_string()),
                    ("e", &e.to_string()),
                ],
            ))
        })?;
        for item in listing {
            let item = item.map_err(|e| {
                WriteRefusal::Io(copy_text(
                    "beFilesWrite.copyTree.listBroke",
                    &[
                        ("path", &dir_at.display().to_string()),
                        ("e", &e.to_string()),
                    ],
                ))
            })?;
            let child_rel = dir_rel.join(item.file_name());
            // ★ **逐条目过路径解析**：这一条就是本函数存在的理由。
            let at = resolve_in_root(root, &child_rel).map_err(|m| {
                WriteRefusal::Refused(copy_text(
                    "beFilesWrite.deleteTree.entryRefused",
                    &[("why", &m)],
                ))
            })?;
            let (is_dir, dev) = kind_and_device(&at).map_err(|e| {
                WriteRefusal::Io(copy_text(
                    "beFilesWrite.copyTree.unreadable",
                    &[("path", &at.display().to_string()), ("e", &e.to_string())],
                ))
            })?;
            if dev != top_dev {
                return Err(WriteRefusal::Refused(copy_text(
                    "beFilesWrite.deleteTree.crossMount",
                    &[("path", &at.display().to_string())],
                )));
            }
            plan.push(Planned {
                rel: child_rel.clone(),
                is_dir,
            });
            if plan.len() > cap {
                return Err(WriteRefusal::Refused(copy_text(
                    "beFilesWrite.deleteTree.overCap",
                    &[
                        ("path", &top.display().to_string()),
                        ("cap", &cap.to_string()),
                    ],
                )));
            }
            if is_dir {
                pending.push(child_rel);
            }
        }
    }
    Ok(plan)
}

/// **执行趟的一条**：先过**它自己那一条**的路径解析，再核种类没变，才删。
///
/// 删的是链接本身（不跟过去）；目录只用「删空目录」—— 里面还有计划之外的东西 ⇒ 系统报错、停。
pub fn remove_planned(root: &Path, p: &Planned) -> Result<PathBuf, WriteRefusal> {
    let at = resolve_in_root(root, &p.rel).map_err(WriteRefusal::Refused)?;
    let is_dir = std::fs::symlink_metadata(&at)
        .map(|m| m.is_dir())
        .map_err(|e| {
            WriteRefusal::Io(copy_text(
                "beFilesWrite.copyTree.unreadable",
                &[("path", &at.display().to_string()), ("e", &e.to_string())],
            ))
        })?;
    if is_dir != p.is_dir {
        return Err(WriteRefusal::Io(copy_text(
            "beFilesWrite.deleteTree.kindChanged",
            &[("path", &at.display().to_string())],
        )));
    }
    let done = if is_dir {
        std::fs::remove_dir(&at)
    } else {
        std::fs::remove_file(&at)
    };
    done.map_err(|e| {
        WriteRefusal::Io(copy_text(
            "beFilesWrite.delete.failed",
            &[("path", &at.display().to_string()), ("e", &e.to_string())],
        ))
    })?;
    Ok(at)
}

/// **递归删**：计划（逐条目过路径解析）→ 按计划倒序逐条删（每条当场再过一次路径解析）。
///
/// 回 `(目标的落点, 真删掉了几条)`。执行趟中途停下 ⇒ 已删的删掉了（与任何递归删同形），
/// 话里带「删了几条之后停在哪一条」。计划趟被拒 ⇒ 一条都没删。
///
/// 🔴 本函数自己**不含任何改动动词**：删那一下住 [`remove_planned`]，列那一下住 [`plan_tree`]。
pub fn delete_tree(root: &Path, rel: impl AsRef<Path>) -> Result<(PathBuf, usize), WriteRefusal> {
    delete_tree_upto(root, rel, None).map(|(top, removed, _)| (top, removed))
}

/// 同 [`delete_tree`]，这一趟至多删 `limit` 条就停在两条之间（叶子先删，剩下的仍是一棵连着的树）：
/// 回 `(目标的落点, 这一趟删了几条, 还剩几条)`。一趟删多少由调用方定（后端不看钟）；`limit` 为 0 也删一条，一趟接一趟总能删完。
pub fn delete_tree_upto(
    root: &Path,
    rel: impl AsRef<Path>,
    limit: Option<usize>,
) -> Result<(PathBuf, usize, usize), WriteRefusal> {
    let rel = rel.as_ref();
    let top = resolve_in_root(root, rel).map_err(WriteRefusal::Refused)?;
    let plan = plan_tree(root, rel)?;
    let mut removed = 0usize;
    for p in plan.iter().rev() {
        if removed > 0 && limit.is_some_and(|n| removed >= n) {
            return Ok((top, removed, plan.len() - removed));
        }
        remove_planned(root, p).map_err(|e| {
            let said = copy_text(
                "beFilesWrite.deleteTree.stopped",
                &[
                    ("why", e.message()),
                    ("n", &removed.to_string()),
                    ("total", &plan.len().to_string()),
                ],
            );
            match e {
                WriteRefusal::Refused(_) => WriteRefusal::Refused(said),
                WriteRefusal::Io(_) | WriteRefusal::Unsupported(_) | WriteRefusal::Stale(_) => {
                    WriteRefusal::Io(said)
                }
            }
        })?;
        removed += 1;
    }
    Ok((top, removed, 0))
}

/// 复制时暂存旁名的序号（同一进程里两趟复制不撞名）。
static COPY_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// **同根内复制一份普通文件**。回 `(落点, 字节数)`。
///
/// # 🔴 它不给第三层添一个动词 —— 由已有的三个拼出来，理由是承重的
///
/// 标准库那个「一步复制」**仍然在第三层的禁词表上**，本函数刻意不用它，两条理由：
///
/// 1. 目标那一格若是一条链接，它**跟过去写**：根里一条指向根外的链接，
///    就能借它把根外那一份盖掉 —— 而路径解析判的是**链接本身**那条路径。
/// 2. 目标已在时它**就地截断重写**：写到一半失败，留下的是半份旧文件、半份新内容。
///
/// ⇒ 拼法（三个动词都早在闭集里，**闭集一个字没变**）：两支都先 `O_EXCL` 开一个同目录的暂存旁名 → 写满 → 按句柄抄权限位，再上位：
///
/// | 覆盖策略 | 怎么上位 | 目标已在 |
/// |---|---|---|
/// | `overwrite = false`（缺省） | 不覆盖改名（[`rename_no_clobber`]） | 上位那一下原子地失败（含它只是一条链接、含写的中途才冒出来的），**一个字节不动**，旁名删掉 |
/// | `overwrite = true`（**显式**） | 换名上位 | 换名那一下**原子地**顶掉（顶掉的是链接本身，不跟过去） |
///
/// 不覆盖那一支从前是 `O_EXCL` 直接开目标：写到一半时目标那一格已经露出半份。今天目标那一格只会整份出现。
/// 写失败 ⇒ 删掉**我们自己刚建的那一份**（暂存旁名），原样带回原因。
///
/// # 路径解析：三条路径各过一次
///
/// - `from` 走 [`resolve_existing_in_root`]（**解到底**）：复制是一次跟链接的读，
///   根里一条指向根外的链接不许借它把根外那一份复制进来。
/// 从前这里还拦「把正被 Claude 打开的 `jsonl` 复制走」，那一道用户拿掉了。
/// - `to` 与暂存旁名走 [`resolve_in_root`]（只解父目录）：它们都是**作用在链接本身**上的。
///
/// ⚠ 只收**普通文件**：目录递归复制没做（与删除不递归同一条理由：路径解析的射程是一条路径）。
/// 新文件的权限位**从源抄**（[`land_copy`]；此前是进程缺省、受 umask）；覆盖时目标换成源的权限位。
/// ⚠ TOCTOU 照旧在（同本模块头注诚实边界第 1 条）。
pub fn copy_entry(
    root: &Path,
    from: impl AsRef<Path>,
    to: impl AsRef<Path>,
    overwrite: bool,
) -> Result<(PathBuf, u64), WriteRefusal> {
    copy_entry_racing(root, from.as_ref(), to.as_ref(), overwrite, &mut || {})
}

/// [`copy_entry`] 的本体；`between` 见 [`land_copy`]。
fn copy_entry_racing(
    root: &Path,
    from: &Path,
    to: &Path,
    overwrite: bool,
    between: &mut dyn FnMut(),
) -> Result<(PathBuf, u64), WriteRefusal> {
    let src = resolve_existing_in_root(root, from).map_err(WriteRefusal::Refused)?;
    let dst = resolve_in_root(root, to).map_err(WriteRefusal::Refused)?;
    if src == dst {
        return Err(WriteRefusal::Refused(copy_text(
            "beFilesWrite.copy.same",
            &[("path", &dst.display().to_string())],
        )));
    }
    let src_md = std::fs::metadata(&src).map_err(|e| {
        WriteRefusal::Io(copy_text(
            "beFilesWrite.copyTree.unreadable",
            &[("path", &src.display().to_string()), ("e", &e.to_string())],
        ))
    })?;
    if !src_md.is_file() {
        return Err(WriteRefusal::Refused(copy_text(
            "beFilesWrite.copy.notRegular",
            &[("path", &src.display().to_string())],
        )));
    }
    land_copy(root, to, &src, src_md.permissions(), overwrite, between)
}

/// 〔抽出〕把 `src`（已经过了路径解析的真路径）的字节落进 `root ＋ dst_rel` 那一格：
/// 同目录暂存旁名 `O_EXCL` 新建 → 写满 → 抄源的权限位 → 上位（`overwrite` 换名顶掉；否则不覆盖改名）。
/// 任一步失败 ⇒ 删掉**我们自己刚建的那一份**（旁名），原样带回原因。回 `(落点, 字节数)`。
///
/// 🔴 **权限位从源抄**（此前是进程缺省、受 umask —— 那条被登记为开着的缺陷）。
/// 用的是闭集里已有的「改权限」，落在我们自己刚建的那一份的**句柄**上（没有路径可被换成链接）；`Permissions` 原样搬
/// （unix 是 mode 低 12 位，别处是只读位）⇒ 不需要平台分支。单文件复制（[`copy_entry`]）与复制目录共用这一段。
/// 源与旁名都经 [`opener`] 开（不跟链接）：源在解析之后被换成一条链接 ⇒ 开就失败，不把链接那头的东西抄进来。
/// `between`：旁名写满之后、上位之前（判据插竞争用；生产传空）。
fn land_copy(
    root: &Path,
    dst_rel: &Path,
    src: &Path,
    perms: std::fs::Permissions,
    overwrite: bool,
    between: &mut dyn FnMut(),
) -> Result<(PathBuf, u64), WriteRefusal> {
    let name = dst_rel.file_name().ok_or_else(|| {
        WriteRefusal::Refused(copy_text(
            "beFilesWrite.path.noName",
            &[("path", &dst_rel.display().to_string())],
        ))
    })?;
    let seq = COPY_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    // 旁名按**原始字节**拼（名字可以不是 UTF-8）：`.` ＋ 原名 ＋ 固定后缀。
    let mut side_name = std::ffi::OsString::from(".");
    side_name.push(name);
    side_name.push(format!(".ccm-copy-{}-{seq}.part", std::process::id()));
    let side_rel = dst_rel.with_file_name(side_name);
    let side = resolve_in_root(root, &side_rel).map_err(WriteRefusal::Refused)?;
    let mut reader = opener().read(true).open(src).map_err(|e| {
        WriteRefusal::Io(copy_text(
            "beFilesWrite.copy.openSrcFailed",
            &[("path", &src.display().to_string()), ("e", &e.to_string())],
        ))
    })?;
    let mut writer = opener()
        .write(true)
        .create_new(true)
        .open(&side)
        .map_err(|e| {
            WriteRefusal::Io(copy_text(
                "beFilesWrite.create.failed",
                &[("path", &side.display().to_string()), ("e", &e.to_string())],
            ))
        })?;
    let drop_side = |why: WriteRefusal| {
        // 只删**我们自己刚建的那一份**（`O_EXCL` 保证它此前不存在）。
        std::fs::remove_file(&side).ok();
        why
    };
    let n = match std::io::copy(&mut reader, &mut writer) {
        Ok(n) => n,
        Err(e) => {
            drop(writer);
            return Err(drop_side(WriteRefusal::Io(copy_text(
                "beFilesWrite.copy.broke",
                &[("path", &side.display().to_string()), ("e", &e.to_string())],
            ))));
        }
    };
    let moded = writer.set_permissions(perms);
    drop(writer);
    if let Err(e) = moded {
        return Err(drop_side(WriteRefusal::Io(copy_text(
            "beFilesWrite.copy.modeFailed",
            &[("path", &side.display().to_string()), ("e", &e.to_string())],
        ))));
    }
    if overwrite {
        let dst =
            resolve_in_root(root, dst_rel).map_err(|m| drop_side(WriteRefusal::Refused(m)))?;
        between();
        if let Err(e) = std::fs::rename(&side, &dst) {
            return Err(drop_side(WriteRefusal::Io(copy_text(
                "beFilesWrite.swap.failed",
                &[("path", &dst.display().to_string()), ("e", &e.to_string())],
            ))));
        }
        return Ok((dst, n));
    }
    let (_, dst, done) = rename_no_clobber(root, &side_rel, dst_rel, between, false)
        .map_err(|m| drop_side(WriteRefusal::Refused(m)))?;
    if let Err(e) = done {
        return Err(drop_side(WriteRefusal::Io(copy_text(
            "beFilesWrite.create.failed",
            &[("path", &dst.display().to_string()), ("e", &e.to_string())],
        ))));
    }
    Ok((dst, n))
}

// ══════════════════════════════════════════════════════════════════════════
// **复制目录：逐条目过路径解析**（照递归删的形状）
// ══════════════════════════════════════════════════════════════════════════
//
// 设计住 §2.2。与递归删（[`delete_tree`]）同一个形状：
//
// 1. **计划趟只读**（[`plan_copy_within`]）：源解到底一次，然后不跟链接地走整棵，每一条目过一次 [`resolve_in_root`]；
//    链接 / 设备 / 管道 / 套接字 ⇒ 整趟拒（闭集里没有「建链接」这个动词，`readonly_guard` 第三层 ②）；
//    跨挂载点 ⇒ 整趟拒；条目数 > [`TREE_ENTRY_CAP`] ⇒ 整趟拒。**一个改动都没有。**
// 2. **执行趟**（[`copy_planned`]）先序逐条：源与目标**当场再过一次**路径解析、不跟链接地核源的种类没变；
//    目录 ⇒ 建目录（目标已在 ⇒ 系统报错 ⇒ 停：目录复制不合并）；文件 ⇒ [`land_copy`]（`O_EXCL` 新建 · 写满 · 抄权限位）。
//    全部落完后目录的权限位**倒序**抄（先抄的话一个只读目录里就建不出下一层）。
// 3. **中途失败 ⇒ 回滚**（[`undo_made`]）：倒序删掉这一趟**自己建的**，各过一次路径解析；撤不干净就说哪一条。
//
// ⚠ 不添动词：建目录 · `O_EXCL` 新建 · 写 · 改权限 · 删文件 · 删空目录，全在闭集里。
// ⚠ TOCTOU 照旧在（同本模块头注诚实边界第 1 条）；源在计划与执行之间被换了种类 ⇒ 停、回滚。

/// 复制计划里的一条：相对**源顶**的那一段（空 ＝ 源顶自己）＋ 计划那一刻它是不是目录（否则是普通文件）。
/// `link_to` 有值 ＝ 它是一条符号链接、目标文本就是这个（复制链接本身，`cp -R` 缺省的 `-P`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CopyPlanned {
    pub tail: PathBuf,
    pub is_dir: bool,
    pub link_to: Option<PathBuf>,
}

/// 一次复制目录的计划。
#[derive(Debug)]
pub struct CopyPlan {
    /// 目标根解开之后的真路径（计划与执行都以它为根）。
    pub root: PathBuf,
    /// 源顶在真根下的相对段（源解到底之后换算回来的）。
    pub src: PathBuf,
    /// 先序表（每一条排在它所有后代之前）。
    pub entries: Vec<CopyPlanned>,
}

fn join_tail(base: &Path, tail: &Path) -> PathBuf {
    if tail.as_os_str().is_empty() {
        base.to_path_buf()
    } else {
        base.join(tail)
    }
}

/// 不跟链接地看一眼：`(种类, 所在设备号)`。种类：`Ok(true)` 目录 · `Ok(false)` 普通文件 · `Err(说法)` 别的（链接 / 设备 …）。
fn copy_kind(p: &Path) -> std::io::Result<(Result<bool, &'static str>, u64)> {
    let (is_dir, dev) = kind_and_device(p)?;
    let ft = std::fs::symlink_metadata(p)?.file_type();
    let kind = if is_dir {
        Ok(true)
    } else if ft.is_file() {
        Ok(false)
    } else if ft.is_symlink() {
        Err("link")
    } else {
        Err("other")
    };
    Ok((kind, dev))
}

/// [`plan_copy_within`] 取默认上限。
pub fn plan_copy(root: &Path, from: impl AsRef<Path>) -> Result<CopyPlan, WriteRefusal> {
    plan_copy_within(root, from.as_ref(), TREE_ENTRY_CAP)
}

/// [`copy_kind`] 那两档「复制不了的种类」说给人听的那几个字（文案表）。
fn kind_words(what: &str) -> String {
    match what {
        "link" => copy_text("beFilesWrite.copyTree.kindLink", &[]),
        _ => copy_text("beFilesWrite.copyTree.kindOther", &[]),
    }
}

/// 复制目录那几句话里「这一条」的路径。
fn shown(p: &Path) -> String {
    p.display().to_string()
}

/// **复制目录的计划趟（只读）**。整趟拒的几形都**一个字节不动**（本函数一个改动都没有）。
pub fn plan_copy_within(root: &Path, from: &Path, cap: usize) -> Result<CopyPlan, WriteRefusal> {
    let real = resolve_existing_in_root(root, from).map_err(WriteRefusal::Refused)?;
    let real_root = std::fs::canonicalize(root).map_err(|e| {
        WriteRefusal::Refused(copy_text(
            "beFilesWrite.copyTree.rootUnresolved",
            &[("path", &shown(root)), ("e", &e.to_string())],
        ))
    })?;
    let src = real
        .strip_prefix(&real_root)
        .map(Path::to_path_buf)
        .unwrap_or_default();
    if src.as_os_str().is_empty() {
        return Err(WriteRefusal::Refused(copy_text(
            "beFilesWrite.copyTree.srcIsRoot",
            &[("path", &shown(&real))],
        )));
    }
    let (top_kind, top_dev) = copy_kind(&real).map_err(|e| {
        WriteRefusal::Io(copy_text(
            "beFilesWrite.copyTree.unreadable",
            &[("path", &shown(&real)), ("e", &e.to_string())],
        ))
    })?;
    let top_is_dir = top_kind.map_err(|what| {
        WriteRefusal::Refused(copy_text(
            "beFilesWrite.copyTree.cannotCopy",
            &[("path", &shown(&real)), ("what", &kind_words(what))],
        ))
    })?;
    let mut entries = vec![CopyPlanned {
        tail: PathBuf::new(),
        is_dir: top_is_dir,
        link_to: None,
    }];
    let mut pending: Vec<PathBuf> = if top_is_dir {
        vec![PathBuf::new()]
    } else {
        Vec::new()
    };
    while let Some(tail) = pending.pop() {
        let dir_at =
            resolve_in_root(&real_root, join_tail(&src, &tail)).map_err(WriteRefusal::Refused)?;
        let listing = std::fs::read_dir(&dir_at).map_err(|e| {
            WriteRefusal::Io(copy_text(
                "beFilesWrite.copyTree.unlistable",
                &[("path", &shown(&dir_at)), ("e", &e.to_string())],
            ))
        })?;
        for item in listing {
            let item = item.map_err(|e| {
                WriteRefusal::Io(copy_text(
                    "beFilesWrite.copyTree.listBroke",
                    &[("path", &shown(&dir_at)), ("e", &e.to_string())],
                ))
            })?;
            let child_tail = tail.join(item.file_name());
            // ★ **逐条目过路径解析**。
            let at = resolve_in_root(&real_root, src.join(&child_tail)).map_err(|m| {
                WriteRefusal::Refused(copy_text(
                    "beFilesWrite.copyTree.entryRefused",
                    &[("why", &m)],
                ))
            })?;
            let (kind, dev) = copy_kind(&at).map_err(|e| {
                WriteRefusal::Io(copy_text(
                    "beFilesWrite.copyTree.unreadable",
                    &[("path", &shown(&at)), ("e", &e.to_string())],
                ))
            })?;
            // 链接 ⇒ 记下它的目标文本（原样，不跟进去）；平台建不了链接时照旧整趟拒。
            let link_to = match kind {
                Err("link") if super::files_extract::LINKS_SUPPORTED => {
                    Some(std::fs::read_link(&at).map_err(|e| {
                        WriteRefusal::Io(copy_text(
                            "beFilesWrite.copyTree.unreadable",
                            &[("path", &shown(&at)), ("e", &e.to_string())],
                        ))
                    })?)
                }
                _ => None,
            };
            let is_dir = if link_to.is_some() {
                false
            } else {
                kind.map_err(|what| {
                    WriteRefusal::Refused(copy_text(
                        "beFilesWrite.copyTree.entryUncopyable",
                        &[("path", &shown(&at)), ("what", &kind_words(what))],
                    ))
                })?
            };
            if dev != top_dev {
                return Err(WriteRefusal::Refused(copy_text(
                    "beFilesWrite.copyTree.crossMount",
                    &[("path", &shown(&at))],
                )));
            }
            entries.push(CopyPlanned {
                tail: child_tail.clone(),
                is_dir,
                link_to,
            });
            if entries.len() > cap {
                return Err(WriteRefusal::Refused(copy_text(
                    "beFilesWrite.copyTree.overCap",
                    &[("path", &shown(&real)), ("cap", &cap.to_string())],
                )));
            }
            if is_dir {
                pending.push(child_tail);
            }
        }
    }
    Ok(CopyPlan {
        root: real_root,
        src,
        entries,
    })
}

/// **执行趟的一条**：源与目标各**当场再过一次**路径解析，不跟链接地核源的种类与计划相同，才动手。回复制了几个字节。
pub fn copy_planned(plan: &CopyPlan, p: &CopyPlanned, dst_rel: &Path) -> Result<u64, WriteRefusal> {
    let src = resolve_in_root(&plan.root, join_tail(&plan.src, &p.tail))
        .map_err(WriteRefusal::Refused)?;
    let dst = resolve_in_root(&plan.root, dst_rel).map_err(WriteRefusal::Refused)?;
    let unreadable = |e: std::io::Error| {
        WriteRefusal::Io(copy_text(
            "beFilesWrite.copyTree.unreadable",
            &[("path", &shown(&src)), ("e", &e.to_string())],
        ))
    };
    let (kind, _) = copy_kind(&src).map_err(unreadable)?;
    // 链接：当场再核「仍是链接、目标文本没变」，再建一条同文本的链接（住 `files_extract::land_link`）。
    if let Some(to) = &p.link_to {
        let now = std::fs::read_link(&src).map_err(unreadable)?;
        if kind != Err("link") || &now != to {
            return Err(WriteRefusal::Io(copy_text(
                "beFilesWrite.copyTree.kindChanged",
                &[("path", &shown(&src))],
            )));
        }
        super::files_extract::land_link(&plan.root, dst_rel, to)?;
        return Ok(0);
    }
    if kind != Ok(p.is_dir) {
        return Err(WriteRefusal::Io(copy_text(
            "beFilesWrite.copyTree.kindChanged",
            &[("path", &shown(&src))],
        )));
    }
    if p.is_dir {
        std::fs::create_dir(&dst).map_err(|e| {
            WriteRefusal::Io(copy_text(
                "beFilesWrite.copyTree.mkdirFailed",
                &[("path", &shown(&dst)), ("e", &e.to_string())],
            ))
        })?;
        return Ok(0);
    }
    let perms = std::fs::metadata(&src).map_err(unreadable)?.permissions();
    land_copy(&plan.root, dst_rel, &src, perms, false, &mut || {}).map(|(_, n)| n)
}

/// 目录的权限位抄过去（全部落完之后倒序调）。两边各过一次路径解析。
fn copy_dir_mode(plan: &CopyPlan, p: &CopyPlanned, dst_rel: &Path) -> Result<(), WriteRefusal> {
    let src = resolve_in_root(&plan.root, join_tail(&plan.src, &p.tail))
        .map_err(WriteRefusal::Refused)?;
    let dst = resolve_in_root(&plan.root, dst_rel).map_err(WriteRefusal::Refused)?;
    let perms = std::fs::metadata(&src)
        .map_err(|e| {
            WriteRefusal::Io(copy_text(
                "beFilesWrite.copyTree.unreadable",
                &[("path", &shown(&src)), ("e", &e.to_string())],
            ))
        })?
        .permissions();
    std::fs::set_permissions(&dst, perms).map_err(|e| {
        WriteRefusal::Io(copy_text(
            "beFilesWrite.copy.modeFailed",
            &[("path", &shown(&dst)), ("e", &e.to_string())],
        ))
    })
}

/// 回滚：倒序删掉这一趟**自己建的**（`made` 是按建出来的先后记的目标相对段 ＋ 是不是目录）。
/// 回 `(撤掉了几条, 撤不掉的那一条的说法)`。
fn undo_made(root: &Path, made: &[(PathBuf, bool)]) -> (usize, Option<String>) {
    let mut undone = 0usize;
    for (rel, is_dir) in made.iter().rev() {
        let at = match resolve_in_root(root, rel) {
            Ok(a) => a,
            Err(m) => return (undone, Some(m)),
        };
        let done = if *is_dir {
            std::fs::remove_dir(&at)
        } else {
            std::fs::remove_file(&at)
        };
        if let Err(e) = done {
            return (undone, Some(format!("{}：{e}", at.display())));
        }
        undone += 1;
    }
    (undone, None)
}

/// 一趟复制目录做了什么。
#[derive(Debug, PartialEq, Eq)]
pub struct TreeCopied {
    pub path: PathBuf,
    pub bytes: u64,
    pub files: usize,
    pub dirs: usize,
    /// 照原样复制的链接条数（不算进 `files`）。
    pub links: usize,
}

/// **复制目录**：计划（逐条目过路径解析）→ 先序逐条建（每条当场再过一次）→ 目录权限位倒序抄；中途失败回滚自己建的。
///
/// 🔴 本函数自己**不含任何改动动词**：建那一下住 [`copy_planned`]，列那一下住 [`plan_copy_within`]，撤那一下住 [`undo_made`]。
pub fn copy_tree(
    root: &Path,
    from: impl AsRef<Path>,
    to: impl AsRef<Path>,
) -> Result<TreeCopied, WriteRefusal> {
    copy_tree_with(root, from.as_ref(), to.as_ref(), TREE_ENTRY_CAP)
}

/// [`copy_tree`] 的本体，上限由调用方给（判据拿小上限验「超了整趟拒」）。
pub fn copy_tree_with(
    root: &Path,
    from: &Path,
    to: &Path,
    cap: usize,
) -> Result<TreeCopied, WriteRefusal> {
    let dst_top = resolve_in_root(root, to).map_err(WriteRefusal::Refused)?;
    let plan = plan_copy_within(root, from, cap)?;
    let src_top = plan.root.join(&plan.src);
    if dst_top.starts_with(&src_top) {
        return Err(WriteRefusal::Refused(copy_text(
            "beFilesWrite.copyTree.intoItself",
            &[("dst", &shown(&dst_top)), ("src", &shown(&src_top))],
        )));
    }
    let dst_rel = dst_top
        .strip_prefix(&plan.root)
        .map(Path::to_path_buf)
        .map_err(|_| {
            WriteRefusal::Refused(copy_text(
                "beFilesWrite.copyTree.outsideRoot",
                &[("dst", &shown(&dst_top)), ("root", &shown(&plan.root))],
            ))
        })?;
    let total = plan.entries.len();
    let mut made: Vec<(PathBuf, bool)> = Vec::new();
    let mut bytes = 0u64;
    for (i, p) in plan.entries.iter().enumerate() {
        let d = join_tail(&dst_rel, &p.tail);
        match copy_planned(&plan, p, &d) {
            Ok(n) => {
                bytes += n;
                made.push((d, p.is_dir));
            }
            Err(e) => {
                let (undone, stuck) = undo_made(&plan.root, &made);
                let tail = match stuck {
                    None => copy_text(
                        "beFilesWrite.copyTree.undoneAll",
                        &[("n", &undone.to_string())],
                    ),
                    Some(what) => copy_text(
                        "beFilesWrite.copyTree.undoneSome",
                        &[
                            ("n", &undone.to_string()),
                            ("m", &made.len().to_string()),
                            ("what", &what),
                        ],
                    ),
                };
                let said = copy_text(
                    "beFilesWrite.copyTree.stopped",
                    &[
                        ("why", e.message()),
                        ("i", &(i + 1).to_string()),
                        ("total", &total.to_string()),
                        ("tail", &tail),
                    ],
                );
                return Err(match e {
                    WriteRefusal::Refused(_) => WriteRefusal::Refused(said),
                    _ => WriteRefusal::Io(said),
                });
            }
        }
    }
    for p in plan.entries.iter().rev().filter(|p| p.is_dir) {
        let d = join_tail(&dst_rel, &p.tail);
        copy_dir_mode(&plan, p, &d).map_err(|e| {
            WriteRefusal::Io(copy_text(
                "beFilesWrite.copyTree.modeNotCopied",
                &[("why", e.message())],
            ))
        })?;
    }
    let dirs = plan.entries.iter().filter(|p| p.is_dir).count();
    let links = plan.entries.iter().filter(|p| p.link_to.is_some()).count();
    Ok(TreeCopied {
        path: dst_top,
        bytes,
        files: total - dirs - links,
        dirs,
        links,
    })
}

// ══════════════════════════════════════════════════════════════════════════
// 用户文件的读改写 ＋ 删历史会话
// ══════════════════════════════════════════════════════════════════════════
//
// 🔴 只有后端的文件管理部分写**用户的文件**，本机也算：
//   monitor 进程从此不直接写用户文件（rc 里的别名块 · PowerShell `$PROFILE` ·
//   项目 `.mcp.json` · skill 收件箱 · `~/.claude/skills/cc-bus/` · cc-bus 收件箱 · 删历史会话），
//   本机与远端**同一条路**：经通道问那台机器上的后端（`call(origin, …)`），落盘只在这里。
//
// 🔴 **写的规则只有这一份**：monitor 那一侧从前的 `fenced_block::apply`（读 → 计划 → 相同不写 →
//   备份 → 原子替换 → 回读比对 → 回滚）搬到了 [`put_text`]；monitor 那一侧只剩「读 · 算 · 交」。
//   读与写之间隔着一次往返 ⇒ 写那一半**必须**带「我读到的是哪一份」（`expect`），对不上就一个字节不写
//   （[`WriteRefusal::Stale`]）。没有「不问就盖」这一形。
//
// 🔴 **闭集一个动词没加**：替换 = `O_EXCL` 新建暂存旁名 ＋ 写满 ＋ 换名上位（与 [`copy_entry`]
//   覆盖那一支同一个拼法）；备份 = `O_EXCL` 新建一份写进内存里的原文（一步复制那个动词仍在禁表上）；
//   沿用原权限位 = 改权限；补父目录 = 逐级建目录（每一级各过一遍路径解析）。
//
// 🔴 **删会话**（[`delete_session`]）：它**只收 sid**，落点由适配层按 sid 找
//   （门经 [`SessionPort`] 递进来的那一个窄口），不收路径 ⇒ 调用方表达不出「另一份文件」。
//   `readonly_guard` 第三层的针因此多一根 `fenced_session_file(`，判据钉它在生产树里恰好被调用一处。
// 它从前的说法是「会话文件围栏唯一的例外」—— 文件管理面的会话文件围栏拿掉之后没有「例外」可言了；
//   它自己那道（「删的必须**是**一份会话、名字恰是 `<sid>.jsonl`」）一个字节没动。

/// 读改写里**读那一半**交回去的东西。
#[derive(Debug, PartialEq, Eq)]
pub struct Peeked {
    /// 读的是哪一份（最后一段是链接时是解到底的那一份）。
    pub path: PathBuf,
    /// `None` = **确定不存在**（与「读不出来」分得开：后者是 `Err`）。
    pub text: Option<String>,
}

/// `files-peek` 一趟肯交的上限。
///
/// ⚠ 为什么不是 `files-read-text` 那个 8 MiB：读改写的写那一半要把**新内容 ＋ 读到的那一份**
/// 装进同一行请求，而后端一行上限是 `inbound::MAX_LINE_BYTES`（1 MiB）。
/// 两份各 256 KiB、再给 JSON 转义留出余量 ⇒ 读得回来的，写得回去。
/// 更大的文件 ⇒ `too_large`，说清楚，不截断。
pub const PEEK_MAX_BYTES: usize = 256 * 1024;

/// 一次读改写的**读那一半**：与写**同一道路径解析**（词法 ＋ 父目录解开；最后一段在盘上就解到底），
/// 于是「读的那一份」与「写的那一份」是同一个落点。
///
/// 不在 ⇒ `Ok(text: None)`；在但不是普通文件 / 不是 UTF-8 / 超上限 ⇒ 拒（码见 [`answer_peek`]）。
pub fn peek_text(root: &Path, rel: impl AsRef<Path>) -> Result<Peeked, (&'static str, String)> {
    let rel = rel.as_ref();
    let at = resolve_in_root(root, rel).map_err(|m| ("refused", m))?;
    match std::fs::symlink_metadata(&at) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok(Peeked {
                path: at,
                text: None,
            })
        }
        Err(e) => {
            return Err((
                "io_failed",
                copy_text(
                    "beFilesWrite.copyTree.unreadable",
                    &[("path", &at.display().to_string()), ("e", &e.to_string())],
                ),
            ))
        }
        Ok(_) => {}
    }
    let real = resolve_existing_in_root(root, rel).map_err(|m| ("refused", m))?;
    let md = std::fs::metadata(&real).map_err(|e| {
        (
            "io_failed",
            copy_text(
                "beFilesWrite.copyTree.unreadable",
                &[("path", &real.display().to_string()), ("e", &e.to_string())],
            ),
        )
    })?;
    if !md.is_file() {
        return Err((
            "refused",
            copy_text(
                "beFilesWrite.peek.notRegular",
                &[("path", &real.display().to_string())],
            ),
        ));
    }
    if md.len() > PEEK_MAX_BYTES as u64 {
        return Err((
            "too_large",
            copy_text(
                "beFilesWrite.peek.tooBig",
                &[
                    ("path", &real.display().to_string()),
                    ("size", &md.len().to_string()),
                    ("max", &PEEK_MAX_BYTES.to_string()),
                ],
            ),
        ));
    }
    let bytes = read_nofollow(&real).map_err(|e| {
        (
            "io_failed",
            copy_text(
                "beFilesWrite.read.failed",
                &[("path", &real.display().to_string()), ("e", &e.to_string())],
            ),
        )
    })?;
    if bytes.is_empty() && md.len() > 0 {
        return Err(("io_failed", hollow_read(&real, md.len())));
    }
    let text = String::from_utf8(bytes).map_err(|_| {
        (
            "not_text",
            copy_text(
                "beFilesWrite.peek.notUtf8",
                &[("path", &real.display().to_string())],
            ),
        )
    })?;
    Ok(Peeked {
        path: real,
        text: Some(text),
    })
}

/// 「盘上有字节、读出来却是空的」那一句（v1.7.9 那次事故：OneDrive 占位 / 杀毒软件锁着）。
///
/// 🔴 继续走的后果是拿「空 ＋ 新内容」整份盖掉原文 —— 读改写的读那一半与写那一半都在这里停。
/// 从前住 monitor 的 `fenced_block::LocalFile::read`〔散文墓碑〕，随写规则一起搬到后端。
fn hollow_read(p: &Path, on_disk: u64) -> String {
    copy_text(
        "beFilesWrite.put.hollow",
        &[
            ("path", &p.display().to_string()),
            ("size", &on_disk.to_string()),
        ],
    )
}

/// 一次 [`put_text`] 做了什么。
#[derive(Debug, PartialEq, Eq)]
pub struct Put {
    /// 落点（最后一段是链接时是解到底的那一份 —— **改的是真文件，不把用户的链接换成一份普通文件**）。
    pub path: PathBuf,
    /// 写进去几个字节（没写时是新内容的长度）。
    pub bytes: usize,
    /// 真的写了吗。算出来的内容与盘上逐字节相同 ⇒ `false`，一个字节不动。
    pub changed: bool,
    /// 这份文件是这一次新建的。
    pub created: bool,
    /// 原文另存在哪（没要备份 / 原来不存在 / 原来是空的 ⇒ `None`）。
    pub backup: Option<PathBuf>,
}

/// 暂存旁名 / 备份名的序号（同一进程里两趟不撞名）。
static PUT_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// 🔴 **用户文件整份替换的那一个序列**（从前 monitor 的 `fenced_block::apply`，今天只有这一份）：
///
/// ① 需要时逐级补父目录 → ② 过路径解析 → ③ **CAS**：盘上那份必须逐字节等于 `expect`（`None` = 必须不存在）
/// → ④ 与新内容逐字节相同 ⇒ 不写 → ⑤ 要备份就 `O_EXCL` 另存原文 → ⑥ 同目录暂存旁名写满、换名上位
/// → ⑦ 回读逐字节比对 → 不符就回滚（原来在 ⇒ 把原文换回去；原来不在 ⇒ 删掉刚建的）。
///
/// ⚠ TOCTOU 照旧在（同本模块头注诚实边界第 1 条）：③ 与 ⑥ 之间有一个窗。窗里被别人改了，
///    ⑥ 会把那一次改动盖掉 —— CAS 缩小的是「monitor 读 → 后端写」那一整趟往返的窗，不是这一个。
pub fn put_text(
    root: &Path,
    rel: impl AsRef<Path>,
    bytes: &[u8],
    expect: Option<&[u8]>,
    keep_backup: bool,
    parents: bool,
) -> Result<Put, WriteRefusal> {
    put_text_racing(
        root,
        rel.as_ref(),
        bytes,
        expect,
        keep_backup,
        parents,
        &mut || {},
    )
}

/// [`put_text`] 的本体；`between` 交给 [`swap_in`]（CAS 判完、旁名写满之后，换名上位之前）。
fn put_text_racing(
    root: &Path,
    rel: &Path,
    bytes: &[u8],
    expect: Option<&[u8]>,
    keep_backup: bool,
    parents: bool,
    between: &mut dyn FnMut(),
) -> Result<Put, WriteRefusal> {
    if parents {
        make_parents(root, rel)?;
    }
    let at = resolve_in_root(root, rel).map_err(WriteRefusal::Refused)?;
    let existed = match std::fs::symlink_metadata(&at) {
        Ok(_) => true,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => false,
        Err(e) => {
            return Err(WriteRefusal::Io(copy_text(
                "beFilesWrite.copyTree.unreadable",
                &[("path", &at.display().to_string()), ("e", &e.to_string())],
            )))
        }
    };
    let (dst, current, perms) = if existed {
        let real = resolve_existing_in_root(root, rel).map_err(WriteRefusal::Refused)?;
        let md = std::fs::metadata(&real).map_err(|e| {
            WriteRefusal::Io(copy_text(
                "beFilesWrite.copyTree.unreadable",
                &[("path", &real.display().to_string()), ("e", &e.to_string())],
            ))
        })?;
        if !md.is_file() {
            return Err(WriteRefusal::Refused(copy_text(
                "beFilesWrite.overwrite.notRegular",
                &[("path", &real.display().to_string())],
            )));
        }
        let cur = read_nofollow(&real).map_err(|e| {
            WriteRefusal::Io(copy_text(
                "beFilesWrite.read.failed",
                &[("path", &real.display().to_string()), ("e", &e.to_string())],
            ))
        })?;
        if cur.is_empty() && md.len() > 0 {
            return Err(WriteRefusal::Io(format!(
                "refuse write: {}",
                hollow_read(&real, md.len())
            )));
        }
        (real, Some(cur), Some(md.permissions()))
    } else {
        (at, None, None)
    };
    if current.as_deref() != expect {
        return Err(WriteRefusal::Stale(match (&current, expect) {
            (None, Some(_)) => copy_text(
                "beFilesWrite.put.gone",
                &[("path", &dst.display().to_string())],
            ),
            (Some(_), None) => copy_text(
                "beFilesWrite.put.appeared",
                &[("path", &dst.display().to_string())],
            ),
            _ => copy_text(
                "beFilesWrite.put.changed",
                &[("path", &dst.display().to_string())],
            ),
        }));
    }
    if current.as_deref() == Some(bytes) {
        return Ok(Put {
            path: dst,
            bytes: bytes.len(),
            changed: false,
            created: false,
            backup: None,
        });
    }
    let backup = match current.as_deref() {
        Some(orig) if keep_backup && !orig.is_empty() => {
            Some(land_backup(root, rel, orig, perms.clone())?)
        }
        _ => None,
    };
    // 原来不在（`expect: null`）⇒ 换名上位不许顶掉任何东西：CAS 判完「不在」之后冒出来的那一份照样不盖，回 `stale`。
    match swap_in(root, rel, bytes, perms.clone(), current.is_none(), between) {
        Err(WriteRefusal::Stale(_)) => {
            return Err(WriteRefusal::Stale(copy_text(
                "beFilesWrite.put.appeared",
                &[("path", &dst.display().to_string())],
            )))
        }
        other => other?,
    };
    let back = read_nofollow(&dst);
    if back.as_deref().ok() != Some(bytes) {
        let why = match &back {
            Ok(b) => copy_text(
                "beFilesWrite.put.verifyMismatch",
                &[
                    ("got", &b.len().to_string()),
                    ("want", &bytes.len().to_string()),
                ],
            ),
            Err(e) => copy_text(
                "beFilesWrite.put.verifyUnreadable",
                &[("e", &e.to_string())],
            ),
        };
        let undone = match current.as_deref() {
            Some(orig) => swap_in(root, rel, orig, perms, false, &mut || {}).is_ok(),
            None => remove_created(root, rel).is_ok(),
        };
        let note = match (existed, undone, &backup) {
            (true, true, _) => copy_text("beFilesWrite.put.restored", &[]),
            (false, true, _) => copy_text("beFilesWrite.put.createdRemoved", &[]),
            (true, false, Some(b)) => copy_text(
                "beFilesWrite.put.restoreFailedBackup",
                &[("path", &b.display().to_string())],
            ),
            (true, false, None) => copy_text("beFilesWrite.put.restoreFailed", &[]),
            (false, false, _) => copy_text("beFilesWrite.put.createdKept", &[]),
        };
        return Err(WriteRefusal::Io(copy_text(
            "beFilesWrite.put.verifyFailed",
            &[
                ("path", &dst.display().to_string()),
                ("why", &why),
                ("note", &note),
            ],
        )));
    }
    Ok(Put {
        path: dst,
        bytes: bytes.len(),
        changed: true,
        created: !existed,
        backup,
    })
}

/// 逐级补出 `rel` 的父目录。**每一级各过一遍路径解析**；已经在（目录，或指向目录的链接）就跳过。
///
/// ⚠ 只给 [`put_text`] 用、且只在调用方**显式**要了（`parents: true`）时跑 ——
/// 「顺手把中间几层补出来」不是缺省行为（`files-mkdir` 照旧只建最后那一段）。
fn make_parents(root: &Path, rel: &Path) -> Result<(), WriteRefusal> {
    let Some(parent) = rel.parent() else {
        return Ok(());
    };
    let mut acc = PathBuf::new();
    for comp in parent.components() {
        acc.push(comp);
        let at = resolve_in_root(root, &acc).map_err(WriteRefusal::Refused)?;
        match std::fs::metadata(&at) {
            Ok(m) if m.is_dir() => continue,
            Ok(_) => {
                return Err(WriteRefusal::Refused(copy_text(
                    "beFilesWrite.mkdirs.notDir",
                    &[("path", &at.display().to_string())],
                )))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                std::fs::create_dir(&at).map_err(|e| {
                    WriteRefusal::Io(copy_text(
                        "beFilesWrite.copyTree.mkdirFailed",
                        &[("path", &at.display().to_string()), ("e", &e.to_string())],
                    ))
                })?;
                if let Some(p) = own_mode(&at, true) {
                    std::fs::set_permissions(&at, p).map_err(|e| {
                        WriteRefusal::Io(copy_text(
                            "beFilesWrite.chmod.failed",
                            &[("path", &at.display().to_string()), ("e", &e.to_string())],
                        ))
                    })?;
                }
            }
            Err(e) => {
                return Err(WriteRefusal::Io(copy_text(
                    "beFilesWrite.copyTree.unreadable",
                    &[("path", &at.display().to_string()), ("e", &e.to_string())],
                )))
            }
        }
    }
    Ok(())
}

/// 把 `bytes` **原子地**换上 `rel` 那一格：同目录 `O_EXCL` 暂存旁名 → 写满 → （沿用原权限位，按句柄）→ 换名上位。
/// 最后一段在盘上就解到底（改真文件，不换掉链接）；失败删掉自己的暂存旁名。
/// `create`（调用方判过「原来不在」）⇒ 上位走不覆盖改名（[`rename_no_clobber`]）：那一格此刻已有东西 ⇒ `Stale`、旁名删掉。
/// `between`：旁名写满之后、上位之前（判据插竞争用；生产传空）。
fn swap_in(
    root: &Path,
    rel: &Path,
    bytes: &[u8],
    perms: Option<std::fs::Permissions>,
    create: bool,
    between: &mut dyn FnMut(),
) -> Result<PathBuf, WriteRefusal> {
    use std::io::Write as _;
    let at = resolve_in_root(root, rel).map_err(WriteRefusal::Refused)?;
    let exists = std::fs::symlink_metadata(&at).is_ok();
    if create && exists {
        return Err(WriteRefusal::Stale(copy_text(
            "beFilesWrite.put.appeared",
            &[("path", &at.display().to_string())],
        )));
    }
    let dst = if exists {
        resolve_existing_in_root(root, rel).map_err(WriteRefusal::Refused)?
    } else {
        at
    };
    // 🔴 Windows：换名上位会让目标**换成暂存旁名的 ACL**（`MoveFileExW` 语义）—— v1.7.9 那次事故
    //   （用户读不了自己的 `$PROFILE`）正是这一形，从前 monitor 那一侧靠 `ReplaceFileW` 保住。
    //   后端没有那条平台原语 ⇒ 已在的目标在 Windows 上**就地覆盖写**（ACL / ADS / 创建时间都留着）。
    //   ⚠ 代价如实写：这一支**不是原子的**（写到一半断电会留半份），兜底是调用方要的备份 ＋ 回读比对 ＋ 回滚。
    #[cfg(windows)]
    if std::fs::symlink_metadata(&dst).is_ok() {
        let _ = &perms;
        std::fs::write(&dst, bytes).map_err(|e| {
            WriteRefusal::Io(copy_text(
                "beFilesWrite.write.failed",
                &[("path", &dst.display().to_string()), ("e", &e.to_string())],
            ))
        })?;
        return Ok(dst);
    }
    let name = dst.file_name().ok_or_else(|| {
        WriteRefusal::Refused(copy_text(
            "beFilesWrite.path.noName",
            &[("path", &rel.display().to_string())],
        ))
    })?;
    let seq = PUT_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    // 〔FW5 之后〕名字可以不是 UTF-8 ⇒ 旁名按 `OsString` 拼，不经 `str`。
    let mut side_name = std::ffi::OsString::from(".");
    side_name.push(name);
    side_name.push(format!(".ccm-put-{}-{seq}.part", std::process::id()));
    let side = dst.with_file_name(&side_name);
    let mut f = opener()
        .write(true)
        .create_new(true)
        .open(&side)
        .map_err(|e| {
            WriteRefusal::Io(copy_text(
                "beFilesWrite.swap.sideCreateFailed",
                &[("path", &side.display().to_string()), ("e", &e.to_string())],
            ))
        })?;
    // 权限位在写进内容之前就定（新建在自家目录里的那一份出生就只给本人，旁名上也不留一刻宽的）。
    let perms = perms.or_else(|| own_mode(&dst, false));
    if let Err(e) = perms.map_or(Ok(()), |p| f.set_permissions(p)) {
        drop(f);
        std::fs::remove_file(&side).ok();
        return Err(WriteRefusal::Io(copy_text(
            "beFilesWrite.swap.sideModeFailed",
            &[("path", &side.display().to_string()), ("e", &e.to_string())],
        )));
    }
    if let Err(e) = f.write_all(bytes) {
        drop(f);
        std::fs::remove_file(&side).ok();
        return Err(WriteRefusal::Io(copy_text(
            "beFilesWrite.swap.sideWriteFailed",
            &[("path", &side.display().to_string()), ("e", &e.to_string())],
        )));
    }
    drop(f);
    if create {
        let why =
            match rename_no_clobber(root, &rel.with_file_name(&side_name), rel, between, false) {
                Ok((_, _, Ok(()))) => return Ok(dst),
                Err(m) => WriteRefusal::Refused(m),
                Ok((_, _, Err(e))) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                    WriteRefusal::Stale(copy_text(
                        "beFilesWrite.put.appeared",
                        &[("path", &dst.display().to_string())],
                    ))
                }
                Ok((_, _, Err(e))) => WriteRefusal::Io(copy_text(
                    "beFilesWrite.swap.failed",
                    &[("path", &dst.display().to_string()), ("e", &e.to_string())],
                )),
            };
        std::fs::remove_file(&side).ok();
        return Err(why);
    }
    between();
    if let Err(e) = std::fs::rename(&side, &dst) {
        std::fs::remove_file(&side).ok();
        return Err(WriteRefusal::Io(copy_text(
            "beFilesWrite.swap.failed",
            &[("path", &dst.display().to_string()), ("e", &e.to_string())],
        )));
    }
    Ok(dst)
}

/// 另存一份原文：`<名>.ccm-backup-<毫秒>-<序号>`，落在 `rel` 旁边（`O_EXCL`，沿用原权限位）。
fn land_backup(
    root: &Path,
    rel: &Path,
    original: &[u8],
    perms: Option<std::fs::Permissions>,
) -> Result<PathBuf, WriteRefusal> {
    use std::io::Write as _;
    let name = rel.file_name().ok_or_else(|| {
        WriteRefusal::Refused(copy_text(
            "beFilesWrite.path.noName",
            &[("path", &rel.display().to_string())],
        ))
    })?;
    let ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let seq = PUT_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let mut bak_name = name.to_os_string();
    bak_name.push(format!(".ccm-backup-{ms}-{seq}"));
    let bak_rel = rel.with_file_name(bak_name);
    let bak = resolve_in_root(root, &bak_rel).map_err(WriteRefusal::Refused)?;
    let mut f = opener()
        .write(true)
        .create_new(true)
        .open(&bak)
        .map_err(|e| {
            WriteRefusal::Io(copy_text(
                "beFilesWrite.backup.createFailed",
                &[("path", &bak.display().to_string()), ("e", &e.to_string())],
            ))
        })?;
    if let Err(e) = f.write_all(original) {
        drop(f);
        std::fs::remove_file(&bak).ok();
        return Err(WriteRefusal::Io(copy_text(
            "beFilesWrite.backup.writeFailed",
            &[("path", &bak.display().to_string()), ("e", &e.to_string())],
        )));
    }
    if let Some(p) = perms {
        // 按句柄改（没有路径可被换成链接）；删那一下仍按路径（删的是链接本身，不跟）。
        keep_mode(
            &bak,
            p,
            |_, p| f.set_permissions(p),
            |b| std::fs::remove_file(b),
        )?;
    }
    drop(f);
    Ok(bak)
}

/// 〔E 吞错普查点名〕备份**沿用原文件的权限位**；沿用不上 ⇒ 删掉这份备份、整趟拒（此刻原文件还没动）。
///
/// 原先一个 `.ok()` 吞掉：一份 0600 的原文件（钥匙、令牌一类）可能留下一份按 umask 建出来的 0644 备份，
/// 而一句话都没有。两个动作都由调用方交进来（生产 = `set_permissions` / `remove_file`，写在 `land_backup` 里 ——
/// 那里先过了路径解析，`readonly_guard` 第三层的判准就落在那个函数上）；判据交一个会失败的 chmod。
fn keep_mode(
    bak: &Path,
    perms: std::fs::Permissions,
    chmod: impl FnOnce(&Path, std::fs::Permissions) -> std::io::Result<()>,
    remove: impl FnOnce(&Path) -> std::io::Result<()>,
) -> Result<(), WriteRefusal> {
    let Err(e) = chmod(bak, perms) else {
        return Ok(());
    };
    let gone = match remove(bak) {
        Ok(()) => copy_text("beFilesWrite.backup.removed", &[]),
        Err(re) => copy_text("beFilesWrite.backup.notRemoved", &[("e", &re.to_string())]),
    };
    Err(WriteRefusal::Io(copy_text(
        "beFilesWrite.backup.modeFailed",
        &[
            ("path", &bak.display().to_string()),
            ("e", &e.to_string()),
            ("gone", &gone),
        ],
    )))
}

/// 回滚那一支：删掉**这一趟自己刚建出来**的那一份（只在「原来不存在」时调）。
fn remove_created(root: &Path, rel: &Path) -> Result<(), WriteRefusal> {
    let at = resolve_in_root(root, rel).map_err(WriteRefusal::Refused)?;
    std::fs::remove_file(&at).map_err(|e| {
        WriteRefusal::Io(copy_text(
            "beFilesWrite.delete.failed",
            &[("path", &at.display().to_string()), ("e", &e.to_string())],
        ))
    })
}

/// 🔴 **删一份历史会话 —— 只收 sid。**
///
/// 落点由适配层按 sid 在本机记录树里找（门递进来的 [`SessionPort::locate`]：找 → 解到底 → 恰是
/// `<项目>/<sid>.jsonl` → 必须是会话文件的形状）。这一条是「用户在历史浏览器里明确点了删」那一件事。
/// 从前这里写「别的写一律不许碰会话文件，这一条是唯一的例外」—— 文件管理面今天什么都能改，
/// 这一条的独特之处只剩「只收 sid、只删恰是那一形的那一份」。
pub fn delete_session(sid: &str, sessions: &SessionPort) -> Result<PathBuf, WriteRefusal> {
    delete_session_with(sid, sessions.locate, sessions.is_record)
}

/// 删会话那一条要问的两件事（落点 · 形状）是 agent 记录布局的知识，
/// 本模块一家都不认 ⇒ 由门（命令注册那一处）从原生那一侧的窄口（`agents::locate_session_for_delete` ·
/// `agents::is_session_record`）取来递进 [`answer_wire`]。
#[derive(Clone, Copy)]
pub struct SessionPort {
    /// sid ⇒ 要删的那一份。
    pub locate: fn(&str) -> Result<PathBuf, String>,
    /// 这一份是不是会话记录。
    pub is_record: fn(&Path) -> bool,
}

/// [`delete_session`] 的本体。`locate` / `is_record` 由调用方给（生产侧 = 门递进来的窄口；判据拿临时目录当 home），
/// 删之前**必须**先过 [`fenced_session_file`]。
pub fn delete_session_with(
    sid: &str,
    locate: impl FnOnce(&str) -> Result<PathBuf, String>,
    is_record: impl FnOnce(&Path) -> bool,
) -> Result<PathBuf, WriteRefusal> {
    let target = fenced_session_file(sid, locate, is_record).map_err(WriteRefusal::Refused)?;
    std::fs::remove_file(&target).map_err(|e| {
        WriteRefusal::Io(copy_text(
            "beFilesWrite.session.deleteFailed",
            &[
                ("path", &target.display().to_string()),
                ("e", &e.to_string()),
            ],
        ))
    })?;
    Ok(target)
}

/// 删会话那一条**自己的**围栏：落点只能是 `locate(sid)` 找到的那一份，而且**它必须是**
/// 会话文件的形状、文件名恰是 `<sid>.jsonl`（`locate` 换成什么都骗不过这两问）。
fn fenced_session_file(
    sid: &str,
    locate: impl FnOnce(&str) -> Result<PathBuf, String>,
    is_record: impl FnOnce(&Path) -> bool,
) -> Result<PathBuf, String> {
    let p = locate(sid)?;
    let want = format!("{sid}.jsonl");
    if p.file_name() != Some(std::ffi::OsStr::new(&want)) {
        return Err(copy_text(
            "beFilesWrite.session.nameMismatch",
            &[
                ("path", &p.display().to_string()),
                ("want", &want.to_string()),
            ],
        ));
    }
    if !is_record(&p) {
        return Err(copy_text(
            "beFilesWrite.session.notRecord",
            &[("path", &p.display().to_string())],
        ));
    }
    Ok(p)
}

// ══════════════════════════════════════════════════════════════════════════
//  命令面 ——〔波 5 ㈠〕 **第 2 步**
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
//   那正是说的「一份代码两种宿主」。
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
/// 那份汇总清单是**产品面**的登记（`CapabilityKind` 两类、
/// 逐 target 的对等断言），加一个面要动 `lib.rs::CAPABILITY_FACES` ——
/// 那处**在本刀写区之外**，已如实报备。
/// ⇒ 本表今天只当「线上契约的数据形态」用（判据按它对拍 `inbound::REGISTRY`
/// 与 `IPC-PROTOCOL.md §10`），**别把它读成「这一面已经进了能力清单」**。
pub struct ManageCommand {
    /// 线上命令名（连字符那一套，与 `inbound::REGISTRY` 逐字相同）。
    pub name: &'static str,
    /// 它做什么（登记散文，只给判据读、不上界面 ⇒ 不叫 `what`：那个字段名会被普查当成文案出口）。
    pub purpose: &'static str,
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
        purpose: "在用户指定的文件管理目标根底下，新建一份**此前不存在**的文件（`O_EXCL`）",
        args: &["content", "rel", "root"],
        fields: &["bytes", "path"],
        codes: &["bad_args", "bad_path", "io_failed", "refused"],
    },
    // ── 〔波 5 ㈡ 09-23〕：**改动既有数据**的那五件 ──────────
    ManageCommand {
        name: "files-mkdir",
        purpose: "新建一个目录（只建最后那一段；父目录不在就失败，不顺手补）",
        args: &["rel", "root"],
        fields: &["path"],
        codes: &["bad_args", "bad_path", "io_failed", "refused"],
    },
    ManageCommand {
        name: "files-rename",
        purpose: "改名 / 同根内移动；**两个参数各过一遍路径解析**，目标已存在就拒（不覆盖）",
        args: &["from", "root", "to"],
        fields: &["path"],
        codes: &["bad_args", "bad_path", "io_failed", "refused"],
    },
    ManageCommand {
        name: "files-delete",
        purpose:
            "删一个文件或一个**空**目录（删的是链接本身，不跟过去）；显式 `recursive: true` \
               才删整棵树 —— 逐条目过路径解析，任一条被拒整趟不动（`delete_tree`）；给了 `expect` \
               ⇒ 只删一份普通文件、且盘上逐字节等于它才删（否则 `stale`，一个字节不动）；`expect: {\"empty_dir\": true}` \
               ⇒ 只删一个空目录（不空 / 不在 ⇒ `stale`；不是目录 ⇒ `refused`）；递归删可带 `limit`：\
               这一趟至多删几条、停在两条之间，`remaining` 回还剩几条（再发一趟接着删）",
        args: &["expect", "limit", "recursive", "rel", "root"],
        fields: &["path", "remaining", "removed"],
        codes: &["bad_args", "bad_path", "io_failed", "refused", "stale"],
    },
    ManageCommand {
        name: "files-chmod",
        purpose: "改 unix 权限位（低 12 位）；**跟链接**，所以落点解到底再判一次；\
没有 unix 权限位的平台上回 `no_unix_mode`",
        args: &["mode", "rel", "root"],
        fields: &["mode", "path"],
        codes: &["bad_args", "bad_path", "io_failed", NO_UNIX_MODE, "refused"],
    },
    // ──：窗口的「复制」换走通道 ──────────────────
    ManageCommand {
        name: "files-copy",
        purpose: "同根内复制一份普通文件；**三条路径各过一遍路径解析**；缺省不覆盖（`O_EXCL`），\
               显式 `overwrite` 才经暂存旁名原子顶掉；权限位从源抄；显式 `recursive: true` \
               才复制目录 —— 逐条目过路径解析，跨挂载点 / 超上限整趟拒，中途失败回滚自己建的（`copy_tree`）；\
树里的链接复制**链接本身**（目标文本原样，`links` 计数）",
        args: &["from", "overwrite", "recursive", "root", "to"],
        fields: &["bytes", "dirs", "files", "links", "path"],
        codes: &["bad_args", "bad_path", "io_failed", "refused"],
    },
    ManageCommand {
        name: "files-write-text",
        purpose: "覆盖写一份**已经在**的普通文件；**跟链接**，所以落点解到底再判一次；\
`expect: {sha256}` **必给**：盘上那份的摘要对得上才写（否则 `stale`，一个字节不动），应答交新摘要",
        args: &["content", "expect", "rel", "root"],
        fields: &["bytes", "path", "sha256"],
        codes: &["bad_args", "bad_path", "io_failed", "refused", "stale"],
    },
    // ── 用户文件的读改写 ＋ 删历史会话（只管用户的文件、本机也管）──
    ManageCommand {
        name: "files-peek",
        purpose: "读改写的**读那一半**：与写同一道路径解析；不在 ⇒ `exists: false`（与「读不出来」分得开）",
        args: &["rel", "root"],
        fields: &["exists", "path", "text"],
        codes: &["bad_args", "bad_path", "io_failed", "not_text", "refused", "too_large"],
    },
    ManageCommand {
        name: "files-put",
        purpose: "整份替换一份文本文件：**CAS（`expect` 必给）→ 相同不写 → 备份 → 暂存旁名换名上位 → \
               回读比对 → 不符回滚** —— 用户文件的写规则只有这一份",
        args: &["backup", "content", "expect", "parents", "rel", "root"],
        fields: &["backup", "bytes", "changed", "created", "path"],
        codes: &["bad_args", "bad_path", "io_failed", "refused", "stale"],
    },
    ManageCommand {
        name: "files-delete-session",
        purpose: "删一份历史会话 —— **只收 sid**，落点由后端按 sid 在记录树里找（历史浏览器的删会话，不是文件管理器）",
        args: &["sid"],
        fields: &["path"],
        codes: &["bad_args", "io_failed", "refused"],
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
        crate::common::contract::malformed(&format!(
            "missing `{key}` (a string or {{\"b16\": \"<hex>\"}})"
        )),
    ))?;
    let bytes = crate::files::raw::from_json(v).ok_or((
        "bad_path",
        crate::common::contract::malformed(&format!(
            "`{key}` must be a string or {{\"b16\": \"<hex>\"}}"
        )),
    ))?;
    if bytes.is_empty() {
        return Err((
            "bad_path",
            crate::common::contract::malformed(&format!("`{key}` is empty")),
        ));
    }
    Ok(crate::files::raw::to_path_buf(&bytes))
}

/// `files-create` —— 把两道路径解析与那一处 `O_EXCL` 落盘接到线上。
///
/// 〔散文墓碑〕这里原来写着「`rel` **只收 UTF-8 字符串**」—— 那时 [`lexical_in_root`]
/// 的入参是 `&str`。路径解析换成按 `Path` 逐段判之后，相对段与 `root` 同形（字符串或 `{"b16": …}`），
/// 取法住 [`rel_of`]。
fn answer_create(args: &serde_json::Value) -> Answer {
    let root = path_of(args, "root")?;
    let rel = rel_of(args, "rel")?;
    // 不给 `content` ⇒ 新建一份**空文件**（那正是「新建空文件」这件事的形状）。
    let bytes = match args.get("content") {
        None => Vec::new(),
        Some(v) => crate::files::raw::from_json(v).ok_or((
            "bad_args",
            crate::common::contract::malformed(
                "`content` must be a string or {\"b16\": \"<hex>\"}",
            ),
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

/// 取一个**相对段**参数：字符串 或 `{"b16": …}`（与 `root` 同一口径）。
///
/// 此前只收 UTF-8 字符串 —— 非 UTF-8 的名字（乱码文件名）在写面上**一件都做不了**。
/// 路径解析按 `Path` 逐段判之后这条限制没有理由了。⚠ **空串不在这里拒**：交给路径解析①
/// （它拒空段、码是 `refused`），与此前的行为逐字相同。
/// 码：缺了 / 形状不对 ⇒ `bad_args`（相对段这一格历来是这个码，与 `root` 的 `bad_path` 刻意不同）。
fn rel_of(args: &serde_json::Value, key: &str) -> Result<PathBuf, (&'static str, String)> {
    let v = args.get(key).ok_or((
        "bad_args",
        crate::common::contract::malformed(&format!(
            "missing `{key}` (a string or {{\"b16\": \"<hex>\"}})"
        )),
    ))?;
    let bytes = crate::files::raw::from_json(v).ok_or((
        "bad_args",
        crate::common::contract::malformed(&format!(
            "`{key}` must be a string or {{\"b16\": \"<hex>\"}}"
        )),
    ))?;
    Ok(crate::files::raw::to_path_buf(&bytes))
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
    // 递归**显式**：不给 ⇒ 不递归（射程与此前一个字节不差）；给了就必须是布尔，不猜。
    let recursive = match args.get("recursive") {
        None => false,
        Some(v) => v.as_bool().ok_or((
            "bad_args",
            crate::common::contract::malformed("`recursive` must be a boolean"),
        ))?,
    };
    // CAS **显式**：不给 ⇒ 射程与此前一个字节不差。给了 ⇒ 只删一份普通文件、盘上逐字节等于它才删。
    //   `null` 拒（删的前提就是它在 —— 「我读的时候它不在」没有可删的东西）；与 `recursive` 同给拒（CAS 只对一份文件）。
    let expect = match args.get("expect") {
        None => None,
        Some(serde_json::Value::Null) => {
            return Err((
                "bad_args",
                crate::common::contract::malformed("`expect` must not be null here"),
            ))
        }
        // 〔SU1 问 2〕恰好 `{"empty_dir": true}` ⇒ 只删空目录；别的对象形照旧按逐字节那一形取（`{"b16": …}`，认不出 ⇒ `bad_args`）。
        Some(serde_json::Value::Object(o))
            if o.len() == 1 && o.get("empty_dir") == Some(&serde_json::Value::Bool(true)) =>
        {
            Some(DeleteExpect::EmptyDir)
        }
        Some(v) => Some(DeleteExpect::Bytes(bytes_of(v, "expect")?)),
    };
    if recursive && expect.is_some() {
        return Err((
            "bad_args",
            crate::common::contract::malformed("`expect` and `recursive` are mutually exclusive"),
        ));
    }
    // 递归删可带 `limit`：这一趟至多删几条，停在两条之间、回 `remaining`（还剩几条），调用方再发一趟接着删。
    let limit = match args.get("limit") {
        None => None,
        Some(v) => Some(v.as_u64().map(|n| n as usize).ok_or((
            "bad_args",
            crate::common::contract::malformed("`limit` must be a non-negative integer"),
        ))?),
    };
    let (done, removed, remaining) = if recursive {
        delete_tree_upto(&root, &rel, limit).map_err(refusal)?
    } else if let Some(want) = expect {
        let done = match want {
            DeleteExpect::Bytes(b) => delete_file_expecting(&root, &rel, &b),
            DeleteExpect::EmptyDir => delete_empty_dir(&root, &rel),
        };
        (done.map_err(refusal)?, 1, 0)
    } else {
        (delete_entry(&root, &rel).map_err(refusal)?, 1, 0)
    };
    Ok(serde_json::json!({ "path": path_json(&done), "removed": removed, "remaining": remaining }))
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
            crate::common::contract::malformed(
                "missing `mode` or not a non-negative integer (decimal, e.g. 420 = 0o644)",
            ),
        ))?;
    let done = change_mode(&root, &rel, mode).map_err(refusal)?;
    Ok(serde_json::json!({ "path": path_json(&done), "mode": mode }))
}

fn answer_copy(args: &serde_json::Value) -> Answer {
    let root = path_of(args, "root")?;
    let from = rel_of(args, "from")?;
    let to = rel_of(args, "to")?;
    // 覆盖策略**显式**：不给 ⇒ 不覆盖（`O_EXCL`）；给了就必须是布尔，不猜「1」「"yes"」。
    let overwrite = match args.get("overwrite") {
        None => false,
        Some(v) => v.as_bool().ok_or((
            "bad_args",
            crate::common::contract::malformed("`overwrite` must be a boolean"),
        ))?,
    };
    // 复制目录**显式**：不给 ⇒ 射程与此前一个字节不差；与 `overwrite: true` 同给 ⇒ 拒（目录复制不合并、不覆盖）。
    if flag_of(args, "recursive")? {
        if overwrite {
            return Err((
                "bad_args",
                copy_text("beFilesWrite.copy.recursiveOverwrite", &[]),
            ));
        }
        let t = copy_tree(&root, &from, &to).map_err(refusal)?;
        return Ok(serde_json::json!({
            "path": path_json(&t.path),
            "bytes": t.bytes,
            "files": t.files,
            "dirs": t.dirs,
            "links": t.links,
        }));
    }
    let (done, n) = copy_entry(&root, &from, &to, overwrite).map_err(refusal)?;
    // 两形同一张应答表（`files` / `dirs` 恒在）：调用方不必按问法猜回来的键。
    Ok(
        serde_json::json!({ "path": path_json(&done), "bytes": n, "files": 1, "dirs": 0, "links": 0 }),
    )
}

fn answer_write_text(args: &serde_json::Value) -> Answer {
    let root = path_of(args, "root")?;
    let rel = rel_of(args, "rel")?;
    // 🔴 这里**必须给** `content`：不给就把一份既有文件写成空的，那不是一个该有默认值的动作。
    let v = args.get("content").ok_or((
        "bad_args",
        crate::common::contract::malformed("missing `content` (no default)"),
    ))?;
    let bytes = crate::files::raw::from_json(v).ok_or((
        "bad_args",
        crate::common::contract::malformed("`content` must be a string or {\"b16\": \"<hex>\"}"),
    ))?;
    // CAS **必给**：没有「不问就盖」这一形（同 `files-put`）。形状先判、再碰盘。
    let expect = sha256_expect_of(args)?;
    let done = overwrite_text_expecting(&root, &rel, &bytes, &expect).map_err(refusal)?;
    Ok(serde_json::json!({
        "path": path_json(&done),
        "bytes": bytes.len(),
        "sha256": content_sha256(&bytes),
    }))
}

/// 取一个**正文**参数（字符串或 `{"b16": …}`）。
fn bytes_of(v: &serde_json::Value, key: &str) -> Result<Vec<u8>, (&'static str, String)> {
    crate::files::raw::from_json(v).ok_or((
        "bad_args",
        crate::common::contract::malformed(&format!(
            "`{key}` must be a string or {{\"b16\": \"<hex>\"}}"
        )),
    ))
}

/// 取一个**可缺席的布尔**：不给 ⇒ `false`；给了就必须是布尔（不猜 `1` / `"yes"`）。
fn flag_of(args: &serde_json::Value, key: &str) -> Result<bool, (&'static str, String)> {
    match args.get(key) {
        None => Ok(false),
        Some(v) => v.as_bool().ok_or((
            "bad_args",
            crate::common::contract::malformed(&format!("`{key}` must be a boolean")),
        )),
    }
}

fn answer_peek(args: &serde_json::Value) -> Answer {
    let root = path_of(args, "root")?;
    let rel = rel_of(args, "rel")?;
    let got = peek_text(&root, &rel)?;
    Ok(serde_json::json!({
        "path": path_json(&got.path),
        "exists": got.text.is_some(),
        "text": got.text,
    }))
}

fn answer_put(args: &serde_json::Value) -> Answer {
    let root = path_of(args, "root")?;
    let rel = rel_of(args, "rel")?;
    let content = args.get("content").ok_or((
        "bad_args",
        crate::common::contract::malformed("missing `content` (no default)"),
    ))?;
    let content = bytes_of(content, "content")?;
    // 🔴 `expect` **必给**：`null` = 「我读的时候它不在」；字符串 / b16 = 「我读到的就是这一份」。
    //    缺席 ⇒ 拒 —— 没有「不问就盖」这一形（理由住本节头注）。
    let expect = match args.get("expect") {
        None => {
            return Err((
                "bad_args",
                crate::common::contract::malformed(
                    "missing `expect` (null means absent at read time)",
                ),
            ))
        }
        Some(serde_json::Value::Null) => None,
        Some(v) => Some(bytes_of(v, "expect")?),
    };
    let backup = flag_of(args, "backup")?;
    let parents = flag_of(args, "parents")?;
    let done =
        put_text(&root, &rel, &content, expect.as_deref(), backup, parents).map_err(refusal)?;
    Ok(serde_json::json!({
        "path": path_json(&done.path),
        "bytes": done.bytes,
        "changed": done.changed,
        "created": done.created,
        "backup": done.backup.as_deref().map(path_json),
    }))
}

fn answer_delete_session(args: &serde_json::Value, sessions: &SessionPort) -> Answer {
    // 🔴 **只收 sid**：多给任何一个键都拒 —— 这一条只按 sid 找那一份、从不收路径，
    //    「顺手也收一个路径」那一形连表达的机会都不给。
    if let Some(obj) = args.as_object() {
        if let Some(extra) = obj.keys().find(|k| k.as_str() != "sid") {
            return Err((
                "bad_args",
                crate::common::contract::malformed(&format!(
                    "`files-delete-session` takes only `sid`, got `{extra}`"
                )),
            ));
        }
    }
    let sid = args.get("sid").and_then(serde_json::Value::as_str).ok_or((
        "bad_args",
        crate::common::contract::malformed("missing `sid` or not a string"),
    ))?;
    let done = delete_session(sid, sessions).map_err(refusal)?;
    Ok(serde_json::json!({ "path": path_json(&done) }))
}

/// 这一面的**唯一入口**（形状照 `files::answer_wire`）。
///
/// 🔴 分派写成一个对 [`MANAGE_COMMANDS`] 的 `match`，而「表里有、分派没有」
/// 那种静默的不可用由判据钉住（本仓 `p1t-removal-cause` 那次真 bug 就是这一形）。
pub fn answer_wire(wire_name: &str, args: &serde_json::Value, sessions: &SessionPort) -> Answer {
    match wire_name {
        "files-create" => answer_create(args),
        "files-mkdir" => answer_mkdir(args),
        "files-rename" => answer_rename(args),
        "files-delete" => answer_delete(args),
        "files-chmod" => answer_chmod(args),
        "files-write-text" => answer_write_text(args),
        "files-copy" => answer_copy(args),
        "files-peek" => answer_peek(args),
        "files-put" => answer_put(args),
        "files-delete-session" => answer_delete_session(args, sessions),
        other => Err((
            "bad_args",
            crate::common::contract::malformed(&format!("unknown write command `{other}`")),
        )),
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/control/files_write_tests.rs"]
mod tests;

// 覆盖写原子化的判据（写到一半被收掉 · 权限位 / 链接 / 旁名 · 生产段 `fs::write(` 只剩 Windows 臂）。
#[cfg(test)]
#[path = "../../../tests/backend/control/overwrite_atomic_tests.rs"]
mod overwrite_atomic_tests;

// TOCTOU 那几条闭合的竞争判据。
#[cfg(test)]
#[path = "../../../tests/backend/control/files_toctou_tests.rs"]
mod toctou_tests;
