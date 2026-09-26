//! 〔步 23b · 2026-09-19〕**文件管理面的落盘原语** —— `设计/60 §6.5.2 A` 拍板的那个
//! 「**带围栏的**白名单模块」，`readonly_guard` 写盘白名单上的第二个洞口。
//!
//! # 🔴〔FN1 · 第四波 4C · 2026-09-25 · 用户 V119〕**文件管理面不再有任何数据围栏**
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
//! # 🔴〔波 5 ㈡ · 2026-09-23〕本模块从白名单层**搬到了第三层**，下面「它刻意不是什么」那一节是**历史**
//!
//! 用户逐字：「**现在只允许后端的文件管理部分写文件**」（收窄的是**主语**，不是动作）。
//! ⇒ `设计/60 §8.6` 第 3 步：新建目录 · 改名 · 删除 · 改权限 · 覆盖写 —— 它们要
//! **改动既有数据**，正是白名单层的判准（「不改既有数据」）所禁的。
//! ⇒ `readonly_guard` 长出**第三层**，判准换成「**改，但每一处都先过路径解析、
//! 且只从声明过的那一面来**」（〔FN1〕原话是「先过围栏」），本模块是那一层登记的模块之一。
//!
//! | 那一层钉的 | 怎么钉 |
//! |---|---|
//! | 能用哪几个改动动词 | **闭集**登记（`readonly_guard` 那张表）；表外的改动动词在本模块里照旧红 —— 包括那个「一条路径进、整棵树出」的一步递归删（它的遍历不经过围栏；〔FW5 · 第四波〕递归删由既有动词**逐条拼出**，见 [`delete_tree`]） |
//! | 〔FW5〕列举之后的改动**在列举之后再过一次路径解析** | 出现目录列举的函数恒等于登记的那几个；其中每一处改动之前、列举之后必须有一次路径解析调用（顶上判一次、底下整摞删 ⇒ 红） |
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
//! # 路径解析：两道，各治一种逃逸（〔FN1〕原标题「围栏」）
//!
//! `设计/60 §6.5.2 A` 逐字：「写点只许落在**用户指定的文件管理目标**下，
//! **不许**落进 Claude 那几棵树」。这一句拆成两道能分别单测的关：
//!
//! | 道 | 住址 | 它拦的是 | 它**拦不住**的是 |
//! |---|---|---|---|
//! | ① 词法 | [`lexical_in_root`] | 上跳段 · 绝对路径 · 盘符 · 空段（〔FN1〕「写点本身就是一份会话文件」那一格删了） | 盘上真实的 symlink —— 它根本不碰盘 |
//! | ② 现打 | [`resolve_parent_in_root`] | 目标根里藏一条指向别处的 symlink（**解完再判一次**） | 判定与落盘之间的时间窗（TOCTOU，见下） |
//!
//! 🔴 **〔波 5 ㈢ · 2026-09-23〕上面那句「不许落进 Claude 那几棵树」的射程被用户改窄了**
//! （〔FN1 · 09-25〕又被 V119 整个拿掉：下面那张表「今天」那一栏已经是历史，会话文件那一行今天也**放行**）。
//!
//! 用户 09-23 逐字：「文件管理器该不该能改 `~/.claude` 里的东西. **可以.**」
//! ⇒ `设计/60 §8.7` 那道「两道栅栏宽窄不同」的产品题按**丙**（统一成同一个判定）裁，
//! 统一到**窄的那一档**：只拦那几份具体的会话文件（`projects/<proj>/<sid>.jsonl` 恰 2 段 ·
//! `sessions/<x>.json` 恰 1 段），于是 skills · 配置 · 账号库**改得动**。
//!
//! | | 09-23 之前 | 今天 |
//! |---|---|---|
//! | 判定 | `is_inside_tree`（拒**整棵 `~/.claude*` 树**） | `is_session_record_path`（只拒那几份会话文件） |
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
//! 判定的住址是适配层里的 `is_session_record_path`，本模块只是**调用它** ——〔FN1〕今天只剩删会话那一处调用。
//!
//! # 🔴 诚实边界 —— 本模块买到的与**买不到**的
//!
//! 1. **TOCTOU 仍在**：路径解析② 解析父目录与真正落盘之间有一个时间窗，
//!    有人在这个窗里把父目录换成 symlink，解析② 看到的就是旧真相。
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

use crate::agents::claudecode::paths::{is_session_record_path, session_file_for_delete};
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
/// 🔴 〔FN1 · V119〕这里原来还有一关「写点自己是不是一份会话文件」（再早是「目标根在不在
/// `~/.claude*` 那几棵树里」）。用户原话「**文件管理器全部都可以改. 不需要任何围栏**」⇒ 那一关删了：
/// 本函数只答「这一段拼到根上之后还在不在根底下」，**不问落点是谁的数据**。
///
/// 〔FW5 · 第四波〕入参从 `&str` 换成了 `impl AsRef<Path>`：段判定走的是 `Path::components`，
/// 它本来就**不看编码** ⇒ 非 UTF-8 的名字（乱码文件名）照样逐段判；递归删的子项名字
/// 也不一定是 UTF-8，逐条目过路径解析要的正是这一格。判定一个字没变。
pub fn lexical_in_root(root: &Path, rel: impl AsRef<Path>) -> Result<PathBuf, String> {
    let rel = rel.as_ref();
    if rel.to_string_lossy().trim().is_empty() {
        return Err("refuse write: 相对路径是空的".to_string());
    }
    let shown = rel.display();
    for c in rel.components() {
        match c {
            Component::Normal(_) => {}
            Component::ParentDir => {
                return Err(format!("refuse write: 相对路径里有上跳段（{shown}）"));
            }
            Component::CurDir => {
                return Err(format!("refuse write: 相对路径里有当前目录段（{shown}）"));
            }
            Component::RootDir | Component::Prefix(_) => {
                return Err(format!(
                    "refuse write: 只收相对段，给的是绝对路径（{shown}）"
                ));
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
/// 🔴 〔FN1 · V119〕这里原来在真路径上还判一次「是不是那几份会话文件」，删了（理由同 [`lexical_in_root`]）。
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
    if !real_parent.starts_with(&real_root) {
        return Err(format!(
            "refuse write: 解完 symlink 之后写点跑出了目标根（{} 不在 {} 里）",
            real_parent.display(),
            real_root.display()
        ));
    }
    Ok(resolved)
}

/// 两道路径解析串起来跑一遍，返回最终落点。**不碰盘上的内容，只解路径。**
///
/// 抽成单独一个函数，是为了让「两道真的被串起来了」这件事有一个可直接喂参数的入口
/// —— 判据不必为了验它而每次都真写一份文件。
///
/// 〔FN1 · V119〕旧名 `fenced_target`：它今天**不拦任何数据**（用户「不需要任何围栏」），
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
    /// 〔FN1〕旧名 `Fenced`：它今天不代表任何数据围栏，名字跟着线上码走。
    Refused(String),
    /// 参数成立，盘上这一步没成（目标已存在 · 父目录不可写 · 盘满 …）。
    Io(String),
    /// 〔FW5 · 第四波〕**这个平台上没有这件事**（非 unix 上改 unix 权限位）。
    ///
    /// 与 [`WriteRefusal::Io`] 分开是因为调用方的下一步不同：`io_failed` 说「重试才有意义」，
    /// 而这一档**重试一万次也一样** —— 压成 `io_failed` 就是在说假话。
    /// 线上码 [`NO_UNIX_MODE`] 同时是 `files-chmod` 声明过的命令级码，`lib.rs` 那条
    /// target 轴的现推读的就是它（`设计/96 §8.5` 待拍 3）。
    Unsupported(String),
    /// 〔RW1 · 第四波〕读改写的**写那一半**发现：盘上那份已经不是调用方读到的那一份了
    /// （`files-put` 的 `expect` 对不上）⇒ 一个字节没写。调用方该**重读重算**，不是重试同一份。
    Stale(String),
}

/// 〔FW5〕命令级码：**这个平台没有 unix 权限位**。只有 `files-chmod` 声明它。
///
/// ⚠ 这是这个字面量在后端的**第二份**（另一份是 `lib.rs::NO_UNIX_MODE`，target 轴现推用），
/// 但它**不是第二个真相源** —— 真相是 `inbound::REGISTRY` 里 `files-chmod` 自己登记的 `codes`。
/// 刻意不去借 `lib.rs` 那一份：文件管理后端往外够的边是登记过的闭集（`files/module_boundary_guard`，
/// 「只许依赖 platform / common / 围栏」），为一个码多长一条边不值。
/// 两份逐字相等由 `target_parity_guard::the_unix_mode_axis_agrees_with_what_this_binary_was_compiled_with` 钉着。
pub const NO_UNIX_MODE: &str = "no_unix_mode";

impl WriteRefusal {
    /// 线上错误码。**闭集四个**，与 [`MANAGE_COMMANDS`] 那一栏逐字对得上
    /// （〔FW5〕第三个只有 `files-chmod` 会回，也只有它声明；〔RW1〕第四个 `stale` 只有 `files-put` 会回
    /// ——〔RM1e〕`files-delete` 带 `expect` 那一形也回它，也声明了）。
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
/// 〔FN1 · V119〕旧名 `fenced_existing`；「解出来是一份会话文件 ⇒ 拒」那一判删了。
pub fn resolve_existing_in_root(root: &Path, rel: impl AsRef<Path>) -> Result<PathBuf, String> {
    let at = resolve_in_root(root, rel)?;
    let real = std::fs::canonicalize(&at)
        .map_err(|e| format!("refuse write: 目标解析不了（{}：{e}）", at.display()))?;
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

/// 新建一个目录。**只建最后那一段**：父目录不在 ⇒ 路径解析② 那一步就拒
/// （「顺手把中间几层补出来」是另一件事，没人裁过）。
pub fn make_dir(root: &Path, rel: impl AsRef<Path>) -> Result<PathBuf, WriteRefusal> {
    let target = resolve_in_root(root, rel).map_err(WriteRefusal::Refused)?;
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
/// 🔴 **两个参数各过一遍路径解析** —— 只解 `from` 的话，`to` 那一侧半路一条链接就能把东西搬到根外面去。
/// 〔FN1 · V119〕从前这一句的理由是「能把任意文件改名成一份会话文件的名字」—— 那一道拦截用户拿掉了。
///
/// 🔴 **目标已经在了就拒**：unix 上系统那一步**会静默顶掉**已有的目标文件 —— 那就是一次
/// 没人问过的覆盖。⇒ 先看一眼（不跟链接地看），在就拒。
/// ⚠ 看与改之间有一个窗（TOCTOU）；原子的「不许顶掉」要平台专有的调用，本刀没做，如实登记。
pub fn rename_entry(
    root: &Path,
    from: impl AsRef<Path>,
    to: impl AsRef<Path>,
) -> Result<PathBuf, WriteRefusal> {
    let src = resolve_in_root(root, from).map_err(WriteRefusal::Refused)?;
    let dst = resolve_in_root(root, to).map_err(WriteRefusal::Refused)?;
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
/// 🔴 **本函数不递归，刻意的**：路径解析的射程是**一条路径**，而递归删动的是一整棵子树 ——
/// 顶上那一条解得干净，底下一条链接或一个挂载点照样会被一起动到。非空目录 ⇒ 系统报错、原样带回。
/// 〔FN1〕从前这里的例子是「底下藏着一份会话文件」，那一道拦截 V119 拿掉了；「每一条自己解一次」这条理由没变。
/// 〔FW5 · 第四波〕递归删是**另一个函数**（[`delete_tree`]），逐条目过路径解析；
/// 线上要显式带 `recursive: true` 才走它 —— 不带，本函数的射程一个字节不变。
pub fn delete_entry(root: &Path, rel: impl AsRef<Path>) -> Result<PathBuf, WriteRefusal> {
    let target = resolve_in_root(root, rel).map_err(WriteRefusal::Refused)?;
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

/// 〔RM1e · 第四波〕**带 CAS 的删一份文件**：盘上那份逐字节 == `expect` 才删，否则一个字节不动。
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
            return Err(WriteRefusal::Stale(format!(
                "refuse delete: {} 已经不在了（读的时候还在）—— 什么都没删，重读再来",
                target.display()
            )))
        }
        Err(e) => {
            return Err(WriteRefusal::Io(format!(
                "refuse write: 读不到 {}：{e}",
                target.display()
            )))
        }
    };
    if !md.is_file() {
        return Err(WriteRefusal::Refused(format!(
            "refuse delete: {} 不是一份普通文件 —— 带 `expect` 的删只收普通文件（不收目录、不收链接）",
            target.display()
        )));
    }
    let current = std::fs::read(&target)
        .map_err(|e| WriteRefusal::Io(format!("refuse write: 读不出 {}：{e}", target.display())))?;
    if current.is_empty() && md.len() > 0 {
        return Err(WriteRefusal::Io(format!(
            "refuse write: {}",
            hollow_read(&target, md.len())
        )));
    }
    if current != expect {
        return Err(WriteRefusal::Stale(format!(
            "refuse delete: {} 在你读过之后被改过了 —— 一个字节没删，重读再来",
            target.display()
        )));
    }
    std::fs::remove_file(&target).map_err(|e| {
        WriteRefusal::Io(format!("refuse write: 删 {} 失败：{e}", target.display()))
    })?;
    Ok(target)
}

/// 改 unix 权限位（只收低 12 位）。**跟链接** ⇒ 走 [`resolve_existing_in_root`]。
///
/// ⚠ 非 unix 平台上**如实回失败**，不假装改成了（`Permissions` 在那边只有一个只读位）。
pub fn change_mode(root: &Path, rel: impl AsRef<Path>, mode: u32) -> Result<PathBuf, WriteRefusal> {
    let real = resolve_existing_in_root(root, rel).map_err(WriteRefusal::Refused)?;
    if mode > 0o7777 {
        return Err(WriteRefusal::Refused(format!(
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
        // 〔FW5〕回 `no_unix_mode`，不回 `io_failed`（理由住 [`WriteRefusal::Unsupported`]）。
        Err(WriteRefusal::Unsupported(format!(
            "refuse write: 这个平台没有 unix 权限位，{} 一个字节没动",
            real.display()
        )))
    }
}

/// 覆盖写一份**已经在**的普通文件。**跟链接** ⇒ 走 [`resolve_existing_in_root`]。
///
/// ⚠ 只覆盖**普通文件**：目标是目录或不存在 ⇒ 拒。新建一份请走 [`create_new_file`]
/// （那条是 `O_EXCL`，两条路刻意分开 —— 「新建」与「改既有」是两件风险不同的事）。
///
/// 🔴〔HX1 · 4D〕**原子地换**：走 [`swap_in`]（同目录 `O_EXCL` 暂存旁名 → 写满 → 沿用原权限位 → 换名上位；
/// 最后一段是链接 ⇒ 解到底、改真文件）。主会话 D-a 裁「覆盖写一律『临时件 ＋ rename』原子化」，出处 E §E2：
/// 此前是就地先截断再写 —— 写到一半失败、或后端在写的中途被收掉，目标剩半份或 0 字节。
/// 今天那两形下目标原封不动（旁边可能剩一份 `.<名>.ccm-put-<pid>-<序>.part`，它不是用户数据）。
/// ⚠ 认下的代价（D-a 的代价，`调研/第四波记录/HX1.md` §2.1，`设计/60 §5.5`「刻意不换语义」那句要随 D0 并入改）：
///   换名之后 inode 换了 ⇒ **硬链接**的另一个名字仍指旧内容 · 目标若属**别的用户**、只是给了我们写权限，换完属主变成我们 ·
///   Linux 上的 **xattr / ACL** 不跟过来。权限位沿用；跨盘不会（旁名与目标同目录）。
///   Windows 臂照旧就地写（`swap_in` 自己那一支，保 ACE —— `设计/60 §3.3` 认过）。
/// 🔴〔HX1 · 主会话裁拍板项 2〕上面前两格代价**不认**：目标 `nlink > 1`（有硬链接）或属主不是后端这个用户 ⇒
///   **退回就地写**（保住硬链接与属主），并记一行「这一次不是原子写，因为 …」（[`in_place_reason`]）。
///   xattr / ACL 不跟过来那一格仍是已知代价（记录 `HX1.md` §6）。
pub fn overwrite_text(
    root: &Path,
    rel: impl AsRef<Path>,
    bytes: &[u8],
) -> Result<PathBuf, WriteRefusal> {
    let rel = rel.as_ref();
    let real = resolve_existing_in_root(root, rel).map_err(WriteRefusal::Refused)?;
    let md = std::fs::metadata(&real)
        .map_err(|e| WriteRefusal::Io(format!("refuse write: 读不到 {}：{e}", real.display())))?;
    if !md.is_file() {
        return Err(WriteRefusal::Refused(format!(
            "refuse write: {} 不是一份普通文件 —— 覆盖写只收普通文件",
            real.display()
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
        std::fs::write(&real, bytes).map_err(|e| {
            WriteRefusal::Io(format!("refuse write: 写 {} 失败：{e}", real.display()))
        })?;
        return Ok(real);
    }
    swap_in(root, rel, bytes, Some(md.permissions()))
}

/// 〔HX1〕一份文件的 `(硬链接数, 属主 uid)`（跟链接地看）。**非 unix 上没有这两个概念 ⇒ `None`**（那边照原子换 / Windows 臂办）。
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

/// 〔HX1 · 主会话裁拍板项 2〕这一份目标**不该原子换**的原因（`None` = 该原子换）：换名上位会让 inode 换掉 ⇒
/// 有硬链接（`links > 1`）的另一个名字仍指旧内容；属主不是后端这个用户（只是给了写权限）⇒ 换完属主变成后端用户。
pub(crate) fn in_place_reason(links: u64, owner: u32, me: u32) -> Option<String> {
    let mut why: Vec<String> = Vec::new();
    if links > 1 {
        why.push(format!("它有 {links} 个硬链接"));
    }
    if owner != me {
        why.push(format!(
            "它的属主（uid {owner}）不是后端这个用户（uid {me}）"
        ));
    }
    (!why.is_empty()).then(|| why.join("、"))
}

// ══════════════════════════════════════════════════════════════════════════
//  〔FW5 · 第四波 · 2026-09-24〕**递归删：逐条目过路径解析**（〔FN1〕原标题「逐条目过围栏」）
// ══════════════════════════════════════════════════════════════════════════
//
// 设计全文住 `调研/第四波记录/FW5.md` 第一节。三件承重的事：
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
    let (top_is_dir, top_dev) = kind_and_device(&top)
        .map_err(|e| WriteRefusal::Io(format!("refuse write: 读不到 {}：{e}", top.display())))?;
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
            WriteRefusal::Io(format!("refuse write: 列不出 {}：{e}", dir_at.display()))
        })?;
        for item in listing {
            let item = item.map_err(|e| {
                WriteRefusal::Io(format!("refuse write: 列 {} 时断了：{e}", dir_at.display()))
            })?;
            let child_rel = dir_rel.join(item.file_name());
            // ★ **逐条目过路径解析**：这一条就是本函数存在的理由。
            let at = resolve_in_root(root, &child_rel).map_err(|m| {
                WriteRefusal::Refused(format!(
                    "{m}\n—— 递归删整趟拒：这棵树里有一条路径解析不过，一个字节都没动"
                ))
            })?;
            let (is_dir, dev) = kind_and_device(&at).map_err(|e| {
                WriteRefusal::Io(format!("refuse write: 读不到 {}：{e}", at.display()))
            })?;
            if dev != top_dev {
                return Err(WriteRefusal::Refused(format!(
                    "refuse write: {} 在另一个文件系统上（挂载点）—— 递归删不走进去，整趟拒、一个字节没动",
                    at.display()
                )));
            }
            plan.push(Planned {
                rel: child_rel.clone(),
                is_dir,
            });
            if plan.len() > cap {
                return Err(WriteRefusal::Refused(format!(
                    "refuse write: {} 底下超过 {cap} 条 —— 一次手势不删这么多，整趟拒、一个字节没动",
                    top.display()
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
        .map_err(|e| WriteRefusal::Io(format!("refuse write: 读不到 {}：{e}", at.display())))?;
    if is_dir != p.is_dir {
        return Err(WriteRefusal::Io(format!(
            "refuse write: {} 在计划与动手之间换了种类（目录 ↔ 非目录）—— 不删",
            at.display()
        )));
    }
    let done = if is_dir {
        std::fs::remove_dir(&at)
    } else {
        std::fs::remove_file(&at)
    };
    done.map_err(|e| WriteRefusal::Io(format!("refuse write: 删 {} 失败：{e}", at.display())))?;
    Ok(at)
}

/// **递归删**：计划（逐条目过路径解析）→ 按计划倒序逐条删（每条当场再过一次路径解析）。
///
/// 回 `(目标的落点, 真删掉了几条)`。执行趟中途停下 ⇒ 已删的删掉了（与任何递归删同形），
/// 话里带「删了几条之后停在哪一条」。计划趟被拒 ⇒ 一条都没删。
///
/// 🔴 本函数自己**不含任何改动动词**：删那一下住 [`remove_planned`]，列那一下住 [`plan_tree`]。
pub fn delete_tree(root: &Path, rel: impl AsRef<Path>) -> Result<(PathBuf, usize), WriteRefusal> {
    let rel = rel.as_ref();
    let top = resolve_in_root(root, rel).map_err(WriteRefusal::Refused)?;
    let plan = plan_tree(root, rel)?;
    let mut removed = 0usize;
    for p in plan.iter().rev() {
        remove_planned(root, p).map_err(|e| {
            let said = format!(
                "{}\n—— 递归删删了 {removed} 条之后停在这一条（共计划 {} 条）",
                e.message(),
                plan.len()
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
    Ok((top, removed))
}

/// 复制时暂存旁名的序号（同一进程里两趟复制不撞名）。
static COPY_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// 〔F7a · 第三波 · 2026-09-24〕**同根内复制一份普通文件**。回 `(落点, 字节数)`。
///
/// # 🔴 它不给第三层添一个动词 —— 由已有的三个拼出来，理由是承重的
///
/// 标准库那个「一步复制」**仍然在第三层的禁词表上**，本函数刻意不用它，两条理由：
///
/// 1. 目标那一格若是一条链接，它**跟过去写**：根里一条指向根外的链接，
///    就能借它把根外那一份盖掉 —— 而路径解析判的是**链接本身**那条路径。
/// 2. 目标已在时它**就地截断重写**：写到一半失败，留下的是半份旧文件、半份新内容。
///
/// ⇒ 拼法（三个动词都早在闭集里，**闭集一个字没变**）：
///
/// | 覆盖策略 | 怎么落 | 目标已在 |
/// |---|---|---|
/// | `overwrite = false`（缺省） | `O_EXCL` 直接开目标 | 开那一步就失败（含它只是一条链接），**一个字节不动** |
/// | `overwrite = true`（**显式**） | `O_EXCL` 开一个同目录的暂存旁名 → 写满 → 换名上位 | 换名那一下**原子地**顶掉（顶掉的是链接本身，不跟过去） |
///
/// 写失败 ⇒ 删掉**我们自己刚建的那一份**（暂存旁名或新目标），原样带回原因。
///
/// # 路径解析：三条路径各过一次
///
/// - `from` 走 [`resolve_existing_in_root`]（**解到底**）：复制是一次跟链接的读，
///   根里一条指向根外的链接不许借它把根外那一份复制进来。
///   〔FN1 · V119〕从前这里还拦「把正被 Claude 打开的 `jsonl` 复制走」，那一道用户拿掉了。
/// - `to` 与暂存旁名走 [`resolve_in_root`]（只解父目录）：它们都是**作用在链接本身**上的。
///
/// ⚠ 只收**普通文件**：目录递归复制没做（与删除不递归同一条理由：路径解析的射程是一条路径）。
/// ⚠ 新文件的权限位是进程缺省（受 umask），**不从源那里抄**；覆盖时旧目标的权限位也随它一起换掉。
/// ⚠ TOCTOU 照旧在（同本模块头注诚实边界第 1 条）。
pub fn copy_entry(
    root: &Path,
    from: impl AsRef<Path>,
    to: impl AsRef<Path>,
    overwrite: bool,
) -> Result<(PathBuf, u64), WriteRefusal> {
    let to = to.as_ref();
    let src = resolve_existing_in_root(root, from).map_err(WriteRefusal::Refused)?;
    let dst = resolve_in_root(root, to).map_err(WriteRefusal::Refused)?;
    if src == dst {
        return Err(WriteRefusal::Refused(format!(
            "refuse write: 复制的源与目标是同一份（{}）",
            dst.display()
        )));
    }
    let is_file = std::fs::metadata(&src)
        .map(|m| m.is_file())
        .map_err(|e| WriteRefusal::Io(format!("refuse write: 读不到 {}：{e}", src.display())))?;
    if !is_file {
        return Err(WriteRefusal::Refused(format!(
            "refuse write: {} 不是一份普通文件 —— 只复制普通文件（目录复制没做）",
            src.display()
        )));
    }
    // 落在哪：不覆盖 ⇒ 直接落目标；显式覆盖 ⇒ 先落同目录的暂存旁名（它自己也过一遍路径解析）。
    let land = if overwrite {
        let name = to.file_name().ok_or_else(|| {
            WriteRefusal::Refused(format!("refuse write: `{}` 没有文件名", to.display()))
        })?;
        let seq = COPY_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        // 〔FW5〕旁名按**原始字节**拼（名字可以不是 UTF-8）：`.` ＋ 原名 ＋ 固定后缀。
        let mut side = std::ffi::OsString::from(".");
        side.push(name);
        side.push(format!(".ccm-copy-{}-{seq}.part", std::process::id()));
        resolve_in_root(root, to.with_file_name(side)).map_err(WriteRefusal::Refused)?
    } else {
        dst.clone()
    };
    let mut reader = std::fs::File::open(&src)
        .map_err(|e| WriteRefusal::Io(format!("refuse write: 打不开源 {}：{e}", src.display())))?;
    let mut writer = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&land)
        .map_err(|e| {
            WriteRefusal::Io(format!(
                "refuse write: 新建 {} 失败（不覆盖时目标已在就停在这一步）：{e}",
                land.display()
            ))
        })?;
    let n = match std::io::copy(&mut reader, &mut writer) {
        Ok(n) => n,
        Err(e) => {
            drop(writer);
            // 只删**我们自己刚建的那一份**（`O_EXCL` 保证它此前不存在）。
            std::fs::remove_file(&land).ok();
            return Err(WriteRefusal::Io(format!(
                "refuse write: 复制到一半断了（{}）：{e}",
                land.display()
            )));
        }
    };
    drop(writer);
    if overwrite {
        if let Err(e) = std::fs::rename(&land, &dst) {
            std::fs::remove_file(&land).ok();
            return Err(WriteRefusal::Io(format!(
                "refuse write: 换名上位 {} 失败：{e}",
                dst.display()
            )));
        }
    }
    Ok((dst, n))
}

// ══════════════════════════════════════════════════════════════════════════
//  〔RW1 · 第四波 · 2026-09-24〕用户文件的读改写 ＋ 删历史会话
// ══════════════════════════════════════════════════════════════════════════
//
// 🔴 用户裁「只允许后端的文件管理部分写文件」**只管用户的文件、本机也管**：
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
//   （`agents::claudecode::paths::session_file_for_delete`），不收路径 ⇒ 调用方表达不出「另一份文件」。
//   `readonly_guard` 第三层的针因此多一根 `fenced_session_file(`，判据钉它在生产树里恰好被调用一处。
//   〔FN1 · V119〕它从前的说法是「会话文件围栏唯一的例外」—— 文件管理面的会话文件围栏拿掉之后没有「例外」可言了；
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
        Err(e) => return Err(("io_failed", format!("读不到 {}：{e}", at.display()))),
        Ok(_) => {}
    }
    let real = resolve_existing_in_root(root, rel).map_err(|m| ("refused", m))?;
    let md = std::fs::metadata(&real)
        .map_err(|e| ("io_failed", format!("读不到 {}：{e}", real.display())))?;
    if !md.is_file() {
        return Err((
            "refused",
            format!("{} 不是一份普通文件 —— 读改写只收普通文件", real.display()),
        ));
    }
    if md.len() > PEEK_MAX_BYTES as u64 {
        return Err((
            "too_large",
            format!(
                "{} 有 {} 字节，超过读改写一趟的上限 {PEEK_MAX_BYTES} —— 不截断（截断的那一份写回去就是把尾巴删了）",
                real.display(),
                md.len()
            ),
        ));
    }
    let bytes = std::fs::read(&real)
        .map_err(|e| ("io_failed", format!("读不出 {}：{e}", real.display())))?;
    if bytes.is_empty() && md.len() > 0 {
        return Err(("io_failed", hollow_read(&real, md.len())));
    }
    let text = String::from_utf8(bytes).map_err(|_| {
        (
            "not_text",
            format!("{} 不是 UTF-8 文本 —— 读改写只收文本", real.display()),
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
/// 从前住 monitor 的 `fenced_block::LocalFile::read`〔散文墓碑〕，〔RW1〕随写规则一起搬到后端。
fn hollow_read(p: &Path, on_disk: u64) -> String {
    format!(
        "{} 在盘上有 {on_disk} 字节，但读出来是空的（可能被 OneDrive 或杀毒软件锁着）。已取消，没改任何东西。",
        p.display()
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
    let rel = rel.as_ref();
    if parents {
        make_parents(root, rel)?;
    }
    let at = resolve_in_root(root, rel).map_err(WriteRefusal::Refused)?;
    let existed = match std::fs::symlink_metadata(&at) {
        Ok(_) => true,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => false,
        Err(e) => {
            return Err(WriteRefusal::Io(format!(
                "refuse write: 读不到 {}：{e}",
                at.display()
            )))
        }
    };
    let (dst, current, perms) = if existed {
        let real = resolve_existing_in_root(root, rel).map_err(WriteRefusal::Refused)?;
        let md = std::fs::metadata(&real).map_err(|e| {
            WriteRefusal::Io(format!("refuse write: 读不到 {}：{e}", real.display()))
        })?;
        if !md.is_file() {
            return Err(WriteRefusal::Refused(format!(
                "refuse write: {} 不是一份普通文件 —— 整份替换只收普通文件",
                real.display()
            )));
        }
        let cur = std::fs::read(&real).map_err(|e| {
            WriteRefusal::Io(format!("refuse write: 读不出 {}：{e}", real.display()))
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
            (None, Some(_)) => format!(
                "refuse write: {} 已经不在了（读的时候还在）—— 一个字节没写，重读再来",
                dst.display()
            ),
            (Some(_), None) => format!(
                "refuse write: {} 现在已经在了（读的时候还不在）—— 一个字节没写，重读再来",
                dst.display()
            ),
            _ => format!(
                "refuse write: {} 在你读过之后被改过了 —— 一个字节没写，重读再来",
                dst.display()
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
    swap_in(root, rel, bytes, perms.clone())?;
    let back = std::fs::read(&dst);
    if back.as_deref().ok() != Some(bytes) {
        let why = match &back {
            Ok(b) => format!(
                "回读 {} 字节，与写进去的 {} 字节不一致",
                b.len(),
                bytes.len()
            ),
            Err(e) => format!("写完读不回来（{e}）"),
        };
        let undone = match current.as_deref() {
            Some(orig) => swap_in(root, rel, orig, perms).is_ok(),
            None => remove_created(root, rel).is_ok(),
        };
        let note = match (existed, undone, &backup) {
            (true, true, _) => "原文件已恢复。".to_string(),
            (false, true, _) => "刚建出来的那份已删掉。".to_string(),
            (true, false, Some(b)) => format!("恢复原文件也失败了，原文备份在 {}。", b.display()),
            (true, false, None) => "恢复原文件也失败了，请打开它看一眼。".to_string(),
            (false, false, _) => "刚建出来的那份没删掉，请手动删掉它。".to_string(),
        };
        return Err(WriteRefusal::Io(format!(
            "refuse write: 写后校验失败（{}）：{why}。{note}",
            dst.display()
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
                return Err(WriteRefusal::Refused(format!(
                    "refuse write: {} 已经在了、但不是目录",
                    at.display()
                )))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                std::fs::create_dir(&at).map_err(|e| {
                    WriteRefusal::Io(format!("refuse write: 建目录 {} 失败：{e}", at.display()))
                })?;
            }
            Err(e) => {
                return Err(WriteRefusal::Io(format!(
                    "refuse write: 读不到 {}：{e}",
                    at.display()
                )))
            }
        }
    }
    Ok(())
}

/// 把 `bytes` **原子地**换上 `rel` 那一格：同目录 `O_EXCL` 暂存旁名 → 写满 → （沿用原权限位）→ 换名上位。
/// 最后一段在盘上就解到底（改真文件，不换掉链接）；失败删掉自己的暂存旁名。
fn swap_in(
    root: &Path,
    rel: &Path,
    bytes: &[u8],
    perms: Option<std::fs::Permissions>,
) -> Result<PathBuf, WriteRefusal> {
    use std::io::Write as _;
    let at = resolve_in_root(root, rel).map_err(WriteRefusal::Refused)?;
    let dst = if std::fs::symlink_metadata(&at).is_ok() {
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
            WriteRefusal::Io(format!("refuse write: 写 {} 失败：{e}", dst.display()))
        })?;
        return Ok(dst);
    }
    let name = dst.file_name().ok_or_else(|| {
        WriteRefusal::Refused(format!("refuse write: `{}` 没有文件名", rel.display()))
    })?;
    let seq = PUT_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    // 〔FW5 之后〕名字可以不是 UTF-8 ⇒ 旁名按 `OsString` 拼，不经 `str`。
    let mut side_name = std::ffi::OsString::from(".");
    side_name.push(name);
    side_name.push(format!(".ccm-put-{}-{seq}.part", std::process::id()));
    let side = dst.with_file_name(side_name);
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&side)
        .map_err(|e| {
            WriteRefusal::Io(format!(
                "refuse write: 新建暂存旁名 {} 失败：{e}",
                side.display()
            ))
        })?;
    if let Err(e) = f.write_all(bytes) {
        drop(f);
        std::fs::remove_file(&side).ok();
        return Err(WriteRefusal::Io(format!(
            "refuse write: 写暂存旁名 {} 失败：{e}",
            side.display()
        )));
    }
    drop(f);
    if let Some(p) = perms {
        if let Err(e) = std::fs::set_permissions(&side, p) {
            std::fs::remove_file(&side).ok();
            return Err(WriteRefusal::Io(format!(
                "refuse write: 给暂存旁名沿用原权限位失败（{}）：{e}",
                side.display()
            )));
        }
    }
    if let Err(e) = std::fs::rename(&side, &dst) {
        std::fs::remove_file(&side).ok();
        return Err(WriteRefusal::Io(format!(
            "refuse write: 换名上位 {} 失败：{e}",
            dst.display()
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
        WriteRefusal::Refused(format!("refuse write: `{}` 没有文件名", rel.display()))
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
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&bak)
        .map_err(|e| {
            WriteRefusal::Io(format!(
                "refuse write: 备份 {} 没建成，原文件没动：{e}",
                bak.display()
            ))
        })?;
    if let Err(e) = f.write_all(original) {
        drop(f);
        std::fs::remove_file(&bak).ok();
        return Err(WriteRefusal::Io(format!(
            "refuse write: 写备份 {} 失败，原文件没动：{e}",
            bak.display()
        )));
    }
    drop(f);
    if let Some(p) = perms {
        std::fs::set_permissions(&bak, p).ok();
    }
    Ok(bak)
}

/// 回滚那一支：删掉**这一趟自己刚建出来**的那一份（只在「原来不存在」时调）。
fn remove_created(root: &Path, rel: &Path) -> Result<(), WriteRefusal> {
    let at = resolve_in_root(root, rel).map_err(WriteRefusal::Refused)?;
    std::fs::remove_file(&at)
        .map_err(|e| WriteRefusal::Io(format!("refuse write: 删 {} 失败：{e}", at.display())))
}

/// 🔴 **删一份历史会话 —— 只收 sid。**
///
/// 落点由适配层按 sid 在本机记录树里找（[`session_file_for_delete`]：找 → 解到底 → 恰是
/// `<项目>/<sid>.jsonl` → 必须是会话文件的形状）。这一条是「用户在历史浏览器里明确点了删」那一件事。
/// 〔FN1 · V119〕从前这里写「别的写一律不许碰会话文件，这一条是唯一的例外」—— 文件管理面今天什么都能改，
/// 这一条的独特之处只剩「只收 sid、只删恰是那一形的那一份」。
pub fn delete_session(sid: &str) -> Result<PathBuf, WriteRefusal> {
    delete_session_with(sid, session_file_for_delete)
}

/// [`delete_session`] 的本体。`locate` 由调用方给（生产侧 = 适配层那一份；判据拿临时目录当 home），
/// 删之前**必须**先过 [`fenced_session_file`]。
pub fn delete_session_with(
    sid: &str,
    locate: impl FnOnce(&str) -> Result<PathBuf, String>,
) -> Result<PathBuf, WriteRefusal> {
    let target = fenced_session_file(sid, locate).map_err(WriteRefusal::Refused)?;
    std::fs::remove_file(&target).map_err(|e| {
        WriteRefusal::Io(format!(
            "refuse write: 删会话 {} 失败：{e}",
            target.display()
        ))
    })?;
    Ok(target)
}

/// 删会话那一条**自己的**围栏：落点只能是 `locate(sid)` 找到的那一份，而且**它必须是**
/// 会话文件的形状、文件名恰是 `<sid>.jsonl`（`locate` 换成什么都骗不过这两问）。
fn fenced_session_file(
    sid: &str,
    locate: impl FnOnce(&str) -> Result<PathBuf, String>,
) -> Result<PathBuf, String> {
    let p = locate(sid)?;
    let want = format!("{sid}.jsonl");
    if p.file_name() != Some(std::ffi::OsStr::new(&want)) {
        return Err(format!(
            "refuse delete: 找到的那一份（{}）不叫 {want}",
            p.display()
        ));
    }
    if !is_session_record_path(&p) {
        return Err(format!(
            "refuse delete: {} 不是一份会话记录 —— 这一条只删会话",
            p.display()
        ));
    }
    Ok(p)
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
        what: "改名 / 同根内移动；**两个参数各过一遍路径解析**，目标已存在就拒（不覆盖）",
        args: &["from", "root", "to"],
        fields: &["path"],
        codes: &["bad_args", "bad_path", "io_failed", "refused"],
    },
    ManageCommand {
        name: "files-delete",
        what:
            "删一个文件或一个**空**目录（删的是链接本身，不跟过去）；〔FW5〕显式 `recursive: true` \
               才删整棵树 —— 逐条目过路径解析，任一条被拒整趟不动（`delete_tree`）；〔RM1e〕给了 `expect` \
               ⇒ 只删一份普通文件、且盘上逐字节等于它才删（否则 `stale`，一个字节不动）",
        args: &["expect", "recursive", "rel", "root"],
        fields: &["path", "removed"],
        codes: &["bad_args", "bad_path", "io_failed", "refused", "stale"],
    },
    ManageCommand {
        name: "files-chmod",
        what: "改 unix 权限位（低 12 位）；**跟链接**，所以落点解到底再判一次；\
               〔FW5〕没有 unix 权限位的平台上回 `no_unix_mode`",
        args: &["mode", "rel", "root"],
        fields: &["mode", "path"],
        codes: &["bad_args", "bad_path", "io_failed", NO_UNIX_MODE, "refused"],
    },
    // ── 〔F7a · 第三波 09-24〕`设计/60 §13`：窗口的「复制」换走通道 ──────────────────
    ManageCommand {
        name: "files-copy",
        what: "同根内复制一份普通文件；**三条路径各过一遍路径解析**；缺省不覆盖（`O_EXCL`），\
               显式 `overwrite` 才经暂存旁名原子顶掉",
        args: &["from", "overwrite", "root", "to"],
        fields: &["bytes", "path"],
        codes: &["bad_args", "bad_path", "io_failed", "refused"],
    },
    ManageCommand {
        name: "files-write-text",
        what: "覆盖写一份**已经在**的普通文件；**跟链接**，所以落点解到底再判一次",
        args: &["content", "rel", "root"],
        fields: &["bytes", "path"],
        codes: &["bad_args", "bad_path", "io_failed", "refused"],
    },
    // ── 〔RW1 · 第四波 09-24〕用户文件的读改写 ＋ 删历史会话（用户裁「只管用户的文件、本机也管」）──
    ManageCommand {
        name: "files-peek",
        what: "读改写的**读那一半**：与写同一道路径解析；不在 ⇒ `exists: false`（与「读不出来」分得开）",
        args: &["rel", "root"],
        fields: &["exists", "path", "text"],
        codes: &["bad_args", "bad_path", "io_failed", "not_text", "refused", "too_large"],
    },
    ManageCommand {
        name: "files-put",
        what: "整份替换一份文本文件：**CAS（`expect` 必给）→ 相同不写 → 备份 → 暂存旁名换名上位 → \
               回读比对 → 不符回滚** —— 用户文件的写规则只有这一份",
        args: &["backup", "content", "expect", "parents", "rel", "root"],
        fields: &["backup", "bytes", "changed", "created", "path"],
        codes: &["bad_args", "bad_path", "io_failed", "refused", "stale"],
    },
    ManageCommand {
        name: "files-delete-session",
        what: "删一份历史会话 —— **只收 sid**，落点由后端按 sid 在记录树里找（历史浏览器的删会话，不是文件管理器）",
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

/// `files-create` —— 把两道路径解析与那一处 `O_EXCL` 落盘接到线上。
///
/// 〔散文墓碑〕〔FW5 · 第四波〕这里原来写着「`rel` **只收 UTF-8 字符串**」—— 那时 [`lexical_in_root`]
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

/// 取一个**相对段**参数：字符串 或 `{"b16": …}`（与 `root` 同一口径）。
///
/// 〔FW5 · 第四波〕此前只收 UTF-8 字符串 —— 非 UTF-8 的名字（乱码文件名）在写面上**一件都做不了**。
/// 路径解析按 `Path` 逐段判之后这条限制没有理由了。⚠ **空串不在这里拒**：交给路径解析①
/// （它拒空段、码是 `refused`），与此前的行为逐字相同。
/// 码：缺了 / 形状不对 ⇒ `bad_args`（相对段这一格历来是这个码，与 `root` 的 `bad_path` 刻意不同）。
fn rel_of(args: &serde_json::Value, key: &str) -> Result<PathBuf, (&'static str, String)> {
    let v = args.get(key).ok_or((
        "bad_args",
        format!("少了 `{key}` —— 它要么是一个字符串，要么是 `{{\"b16\": \"<十六进制>\"}}`"),
    ))?;
    let bytes = crate::files::raw::from_json(v).ok_or((
        "bad_args",
        format!(
            "`{key}` 的形状不对 —— 只认字符串或 `{{\"b16\": \"<十六进制>\"}}`；\
             猜错一个字节就是对另一个名字动手"
        ),
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
    // 〔FW5〕递归**显式**：不给 ⇒ 不递归（射程与此前一个字节不差）；给了就必须是布尔，不猜。
    let recursive = match args.get("recursive") {
        None => false,
        Some(v) => v.as_bool().ok_or((
            "bad_args",
            "`recursive` 只收布尔 —— 删整棵树是一件要说清的事，这里不猜".to_string(),
        ))?,
    };
    // 〔RM1e〕CAS **显式**：不给 ⇒ 射程与此前一个字节不差。给了 ⇒ 只删一份普通文件、盘上逐字节等于它才删。
    //   `null` 拒（删的前提就是它在 —— 「我读的时候它不在」没有可删的东西）；与 `recursive` 同给拒（CAS 只对一份文件）。
    let expect = match args.get("expect") {
        None => None,
        Some(serde_json::Value::Null) => {
            return Err((
                "bad_args",
                "`expect` 不收 `null` —— 带 `expect` 的删说的是「读到的是这一份，删它」，不在就没有可删的".to_string(),
            ))
        }
        Some(v) => Some(bytes_of(v, "expect")?),
    };
    if recursive && expect.is_some() {
        return Err((
            "bad_args",
            "`expect` 与 `recursive` 不能同给 —— CAS 比的是一份文件的字节，整棵树没有「那一份」"
                .to_string(),
        ));
    }
    let (done, removed) = if recursive {
        delete_tree(&root, &rel).map_err(refusal)?
    } else if let Some(want) = expect.as_deref() {
        (
            delete_file_expecting(&root, &rel, want).map_err(refusal)?,
            1,
        )
    } else {
        (delete_entry(&root, &rel).map_err(refusal)?, 1)
    };
    Ok(serde_json::json!({ "path": path_json(&done), "removed": removed }))
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

fn answer_copy(args: &serde_json::Value) -> Answer {
    let root = path_of(args, "root")?;
    let from = rel_of(args, "from")?;
    let to = rel_of(args, "to")?;
    // 覆盖策略**显式**：不给 ⇒ 不覆盖（`O_EXCL`）；给了就必须是布尔，不猜「1」「"yes"」。
    let overwrite = match args.get("overwrite") {
        None => false,
        Some(v) => v.as_bool().ok_or((
            "bad_args",
            "`overwrite` 只收布尔 —— 覆盖是一件要说清的事，这里不猜".to_string(),
        ))?,
    };
    let (done, n) = copy_entry(&root, &from, &to, overwrite).map_err(refusal)?;
    Ok(serde_json::json!({ "path": path_json(&done), "bytes": n }))
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

/// 取一个**正文**参数（字符串或 `{"b16": …}`）。
fn bytes_of(v: &serde_json::Value, key: &str) -> Result<Vec<u8>, (&'static str, String)> {
    crate::files::raw::from_json(v).ok_or((
        "bad_args",
        format!("`{key}` 的形状不对 —— 只认字符串或 `{{\"b16\": \"<十六进制>\"}}`"),
    ))
}

/// 取一个**可缺席的布尔**：不给 ⇒ `false`；给了就必须是布尔（不猜 `1` / `"yes"`）。
fn flag_of(args: &serde_json::Value, key: &str) -> Result<bool, (&'static str, String)> {
    match args.get(key) {
        None => Ok(false),
        Some(v) => v.as_bool().ok_or(("bad_args", format!("`{key}` 只收布尔"))),
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
        "少了 `content` —— 整份替换不给默认值（默认成空等于把那份文件清空）".to_string(),
    ))?;
    let content = bytes_of(content, "content")?;
    // 🔴 `expect` **必给**：`null` = 「我读的时候它不在」；字符串 / b16 = 「我读到的就是这一份」。
    //    缺席 ⇒ 拒 —— 没有「不问就盖」这一形（理由住本节头注）。
    let expect = match args.get("expect") {
        None => return Err((
            "bad_args",
            "少了 `expect` —— 读改写的写那一半必须说清读到的是哪一份（`null` = 读的时候不存在）"
                .to_string(),
        )),
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

fn answer_delete_session(args: &serde_json::Value) -> Answer {
    // 🔴 **只收 sid**：多给任何一个键都拒 —— 这一条只按 sid 找那一份、从不收路径，
    //    「顺手也收一个路径」那一形连表达的机会都不给。
    if let Some(obj) = args.as_object() {
        if let Some(extra) = obj.keys().find(|k| k.as_str() != "sid") {
            return Err((
                "bad_args",
                format!("`files-delete-session` 只收 `sid`，多给了 `{extra}`"),
            ));
        }
    }
    let sid = args
        .get("sid")
        .and_then(serde_json::Value::as_str)
        .ok_or(("bad_args", "少了 `sid`，或者它不是一个字符串".to_string()))?;
    let done = delete_session(sid).map_err(refusal)?;
    Ok(serde_json::json!({ "path": path_json(&done) }))
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
        "files-copy" => answer_copy(args),
        "files-peek" => answer_peek(args),
        "files-put" => answer_put(args),
        "files-delete-session" => answer_delete_session(args),
        other => Err(("bad_args", format!("`{other}` 不是文件管理写面的命令"))),
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/control/files_write_tests.rs"]
mod tests;

// 〔HX1〕覆盖写原子化的判据（写到一半被收掉 · 权限位 / 链接 / 旁名 · 生产段 `fs::write(` 只剩 Windows 臂）。
#[cfg(test)]
#[path = "../../../tests/backend/control/overwrite_atomic_tests.rs"]
mod overwrite_atomic_tests;
