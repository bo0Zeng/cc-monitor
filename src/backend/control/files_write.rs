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
pub const MANAGE_COMMANDS: &[ManageCommand] = &[ManageCommand {
    name: "files-create",
    what: "在用户指定的文件管理目标根底下，新建一份**此前不存在**的文件（`O_EXCL`）",
    args: &["content", "rel", "root"],
    fields: &["bytes", "path"],
    codes: &["bad_args", "bad_path", "io_failed", "refused"],
}];

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

/// 这一面的**唯一入口**（形状照 `files::answer_wire`）。
///
/// 🔴 分派写成一个对 [`MANAGE_COMMANDS`] 的 `match`，而「表里有、分派没有」
/// 那种静默的不可用由判据钉住（本仓 `p1t-removal-cause` 那次真 bug 就是这一形）。
pub fn answer_wire(wire_name: &str, args: &serde_json::Value) -> Answer {
    match wire_name {
        "files-create" => answer_create(args),
        other => Err(("bad_args", format!("`{other}` 不是文件管理写面的命令"))),
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/control/files_write_tests.rs"]
mod tests;
