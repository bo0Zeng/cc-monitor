//! **文件管理面的落盘原语**：路径解析（造 [`Resolved`] 的那几处）与经 [`Face`] 的每一件写。
//!
//! # 边界在哪
//!
//! `root` 由请求给，可以是任意绝对路径（文件管理器能改这个用户能改的任何东西，会话文件也一样）。**谁能发这条命令才是边界**：
//! 本机是 0700 目录里的套接字 ＋ 对端 uid，远端是 SSH。路径解析保的是正确性 —— 改的就是 `root ＋ rel` 那一格，
//! 半路一条链接不许把它换到根外：[`lexical_in_root`]（逐段只许普通段）→ [`walk_in_root`]（解成真路径、判在根下、
//! 从根句柄逐段不跟链接地开到那一层；写只经那个目录句柄）。
//! 唯一问会话形状的是删历史会话那一条（[`delete_session`]，只收 sid，要删的必须**是**一份会话）。
//!
//! # 写只能经这里：类型
//!
//! - 写只经 [`crate::platform::fs::Face`]：它的写方法只收 [`Resolved`]，而 `Resolved` 只由本模块的路径解析担保
//!   （外加删会话那一道 [`fenced_session_file`]）；
//! - `Face` 只有分派那一层造，按命令表文件管理那一族递进来 ⇒ 别的命令写不出来；
//! - 动词闭集就是 `Face` 的方法表；开文件一律不跟链接。递归删由删文件 ＋ 删空目录逐条拼（[`delete_tree`]），复制由新建 ＋ 改名拼（[`copy_entry`]）。
//!
//! # 还开着的
//!
//! unix 上每一格都从根句柄逐段不跟链接地开到那一层、写只经那个目录句柄（`*at` 一族）：判与动之间半路一层被换成链接，
//! 动的仍是判过的那一层，落不到根外；递归删 / 复制每一条都这样走。还开着：CAS 与换名之间（窗里被别人改了会被盖掉）；
//! Windows 臂仍按路径字符串动手（那几形在 Windows 上没闭合、没在真 Windows 上跑过）。判据全在本机临时目录上跑，没在真远端上跑过。

use crate::platform::fs::{Face, Perms, Resolved, Ro};
use cmd_wire::files as wire;
use copy_core::{copy_text, io_reason};
use std::path::{Component, Path, PathBuf};

// 路径解析住 `common/root_path.rs`：传输台（原生那一块）写本机落点也过它，两块各引各的。
pub use crate::common::root_path::{lexical_in_root, walk_in_root, Last};

/// 一次落盘没成，**是谁拦的**。
///
/// # 🔴 为什么要分这两档（不是分类癖，是线上那一面分得出码才有意义）
///
/// 接命令面时逼出来的：参数被拒与盘上出错是**两件对调用方意义完全不同**的事 ——
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
    /// （`Door::put` 的 `expect` 对不上）⇒ 一个字节没写。调用方该**重读重算**，不是重试同一份。
    Stale(String),
    /// 同 [`WriteRefusal::Io`]，句子只说原因词、下层原话另带（进复制详情，不上句子）。线上码同 `io_failed`。
    IoSaid { said: String, raw: String },
    /// 落点上**已经有一项**（新建 / 建目录 / 改名的目标名被占了），一个字节没动。线上码 [`EXISTS`]。
    /// 与 [`WriteRefusal::Io`] 分开：调用方的下一步是**换一个名字**，不是重试（重试一万次也一样）。
    /// 只有 `files-create` · `files-mkdir` · `files-rename` 回它，也只有它们声明。
    Exists(String),
}

/// 命令级码：**落点上已经有一项**（`files-create` · `files-mkdir` · `files-rename` 声明）。
pub const EXISTS: &str = "exists";

/// 命令级码：**这个名字在这台不能用**（保留名 · 不许的字符 · 结尾点或空格；`single` 时不止一段）。
/// `files-create` · `files-mkdir` · `files-rename` 声明。
pub const BAD_NAME: &str = "bad_name";

/// 命令级码：**这个平台没有 unix 权限位**。只有 `files-chmod` 声明它。
///
/// ⚠ 这是这个字面量在后端的**第二份**（另一份是 `lib.rs::NO_UNIX_MODE`，填 `hello.unavailable` 用），
/// 但它**不是第二个源头** —— 真相是 `inbound::REGISTRY` 里 `files-chmod` 自己登记的 `codes`。
/// 刻意不去借 `lib.rs` 那一份：文件管理后端往外够的边是登记过的闭集（`files/module_boundary_guard`，
/// 「只许依赖 platform / common / 围栏」），为一个码多长一条边不值。
/// 两份逐字相等由 `target_parity_guard::every_target_has_the_same_commands_except_the_registered_gaps` 钉着。
pub const NO_UNIX_MODE: &str = "no_unix_mode";

impl WriteRefusal {
    /// 线上错误码。**闭集四个**，与各条错误枚举（`stream/inbound/files_write_face.rs`）的码对得上
    /// （第三个只有 `files-chmod` 会回，也只有它声明；第四个 `stale` 只有 `Door::put` 会回
    /// ——`files-delete` 带 `expect` 那一形也回它，也声明了）。
    pub fn code(&self) -> &'static str {
        match self {
            WriteRefusal::Refused(_) => "refused",
            WriteRefusal::Io(_) | WriteRefusal::IoSaid { .. } => "io_failed",
            WriteRefusal::Unsupported(_) => NO_UNIX_MODE,
            WriteRefusal::Stale(_) => "stale",
            WriteRefusal::Exists(_) => EXISTS,
        }
    }

    /// 给人看的那句话（原样来自路径解析／系统，本层不改写）。
    pub fn message(&self) -> &str {
        match self {
            WriteRefusal::Refused(m)
            | WriteRefusal::Io(m)
            | WriteRefusal::Unsupported(m)
            | WriteRefusal::Stale(m)
            | WriteRefusal::Exists(m)
            | WriteRefusal::IoSaid { said: m, .. } => m.as_str(),
        }
    }

    /// 盘上这一步没成：句子（已带原因词）＋ 系统原话（进复制详情，不上句子）。线上码 `io_failed`。
    pub fn io(said: String, e: &std::io::Error) -> WriteRefusal {
        WriteRefusal::IoSaid {
            said,
            raw: e.to_string(),
        }
    }

    /// 整棵那几条停在半路：换成外层那一句（`said` 里已经接着这一条的说法），拒绝仍是拒绝、
    /// 其余都是 `io_failed`；这一条带着的原话跟过去（不在外层丢掉）。
    pub fn restated(self, said: String) -> WriteRefusal {
        match self {
            WriteRefusal::Refused(_) => WriteRefusal::Refused(said),
            WriteRefusal::IoSaid { raw, .. } => WriteRefusal::IoSaid { said, raw },
            WriteRefusal::Io(_)
            | WriteRefusal::Unsupported(_)
            | WriteRefusal::Stale(_)
            | WriteRefusal::Exists(_) => WriteRefusal::Io(said),
        }
    }

    /// 下层原话（只有 [`WriteRefusal::IoSaid`] 带；交给应答的复制详情）。
    pub fn raw(&self) -> Option<&str> {
        match self {
            WriteRefusal::IoSaid { raw, .. } => Some(raw.as_str()),
            _ => None,
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
    face: &Face,
    root: &Path,
    rel: impl AsRef<Path>,
    bytes: &[u8],
) -> Result<PathBuf, WriteRefusal> {
    use std::io::Write as _;
    let target = walk_in_root(Last::Link, root, rel).map_err(WriteRefusal::Refused)?;
    let mut f = face.create_new(&target).map_err(|e| {
        let said = copy_text(
            "beFilesWrite.create.failed",
            &[
                ("path", &target.display().to_string()),
                ("why", &io_reason(e.kind())),
            ],
        );
        if e.kind() == std::io::ErrorKind::AlreadyExists {
            WriteRefusal::Exists(said)
        } else {
            WriteRefusal::io(said, &e)
        }
    })?;
    if let Some(p) = own_mode(&target, false) {
        f.set_permissions(p).map_err(|e| {
            WriteRefusal::io(
                copy_text(
                    "beFilesWrite.chmod.failed",
                    &[
                        ("path", &target.display().to_string()),
                        ("why", &io_reason(e.kind())),
                    ],
                ),
                &e,
            )
        })?;
    }
    // 写失败（盘满等）也要把原因带回去 —— 静默的半截文件比报错糟得多。
    f.write_all(bytes).map_err(|e| {
        WriteRefusal::io(
            copy_text(
                "reason.fs.writeFailed",
                &[
                    ("path", &target.display().to_string()),
                    ("why", &io_reason(e.kind())),
                ],
            ),
            &e,
        )
    })?;
    Ok(target.into_path_buf())
}

// ══════════════════════════════════════════════════════════════════════════
// 改动既有数据的那五件
// ══════════════════════════════════════════════════════════════════════════
//
// 🔴 **这一节才花掉用户那句话**：「现在只允许后端的文件管理部分写文件」。
//   它们每一件都会改动盘上已经在的东西。
//
// 🔴 **每个函数的第一件事是过路径解析** —— 那不是风格，是类型：`Face` 的写方法只收路径解析交回的 `Resolved`，
//   先动手、后判那一形编不过。
//
// 路径解析（[`walk_in_root`]）差别只在最后那一段解不解：`Last::Link` 给作用在链接本身上的动词（新建 · 建目录 · 改名 · 删除）；
// `Last::Follow` 给跟链接的（改权限 · 换内容）：不解的话，根里一条指向根外的链接就能把根外那一份改掉。

/// **不覆盖地改名** `root ＋ from` → `root ＋ to`：两条各过路径解析，然后一次
/// `platform::fs::rename_noreplace` —— 目标已在（含一条链接）⇒ `AlreadyExists`、一个字节不动，没有先看后改的窗。
/// 回 `(源, 目标, 那一下的结局)`；路径解析拒 ⇒ `Err`。
///
/// 那块盘不认这个旗（NFS · 部分 FUSE · 老内核 · 别的 unix）⇒ 不退回先看后改：
/// 普通文件（含链接本身）走 `platform::fs::rename_by_link`（`link` 在目标已在时原子失败，NFS 上也成立）；
/// 目录没有这条路 ⇒ 拒、出声（`Err`，说「这个盘不支持不覆盖改名目录」）。
/// `between` 是判据插竞争用的口（解析之后、动手之前；生产传空）；`force_link` 让判据走那块盘的那一支（生产传 `false`）。
fn rename_no_clobber(
    face: &Face,
    root: &Path,
    from: &Path,
    to: &Path,
    between: &mut dyn FnMut(),
    force_link: bool,
) -> Result<(PathBuf, PathBuf, std::io::Result<()>), String> {
    let src = walk_in_root(Last::Link, root, from)?;
    let dst = walk_in_root(Last::Link, root, to)?;
    between();
    if !force_link {
        match face.move_noreplace(&src, &dst) {
            Err(e) if crate::platform::fs::noreplace_unsupported(&e) => {}
            other => return Ok((src.into_path_buf(), dst.into_path_buf(), other)),
        }
    }
    if Ro::symlink_metadata(&src).is_ok_and(|m| m.is_dir()) {
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
    let done = face.move_by_link(&src, &dst);
    Ok((src.into_path_buf(), dst.into_path_buf(), done))
}

/// 新建一个目录。**只建最后那一段**：父目录不在 ⇒ 路径解析② 那一步就拒
/// （「顺手把中间几层补出来」是另一件事，没人裁过）。
pub fn make_dir(face: &Face, root: &Path, rel: impl AsRef<Path>) -> Result<PathBuf, WriteRefusal> {
    let target = walk_in_root(Last::Link, root, rel).map_err(WriteRefusal::Refused)?;
    face.make_dir(&target).map_err(|e| {
        let said = copy_text(
            "reason.fs.writeFailed",
            &[
                ("path", &target.display().to_string()),
                ("why", &io_reason(e.kind())),
            ],
        );
        if e.kind() == std::io::ErrorKind::AlreadyExists {
            WriteRefusal::Exists(said)
        } else {
            WriteRefusal::io(said, &e)
        }
    })?;
    if let Some(p) = own_mode(&target, true) {
        face.set_permissions(&target, p).map_err(|e| {
            WriteRefusal::io(
                copy_text(
                    "beFilesWrite.chmod.failed",
                    &[
                        ("path", &target.display().to_string()),
                        ("why", &io_reason(e.kind())),
                    ],
                ),
                &e,
            )
        })?;
    }
    Ok(target.into_path_buf())
}

/// 这台机器上后端自家目录的权限位：新建在 `~/.cc-monitor` 里的文件只给本人读写。
const OWN_FILE_MODE: u32 = 0o600;

/// **新建**在这台机器自家目录（`$HOME/.cc-monitor` 底下，那个目录是后端认的本人真目录：`platform::fs::inside_own_dir`）里的东西只给本人：
/// 文件 [`OWN_FILE_MODE`] · 目录 `own_dir::PRIVATE_DIR_MODE`（unix；别的平台照常建，那边按父目录继承 ACL）。只管新建：覆盖写沿用原权限位、复制照抄源的。
/// `at` 是过了路径解析的那一条（父目录已解开）。不在自家目录底下 ⇒ 不收紧。
fn own_mode(at: &Path, dir: bool) -> Option<Perms> {
    if !crate::platform::fs::inside_own_dir(at) {
        return None;
    }
    let mode = if dir {
        crate::common::own_dir::PRIVATE_DIR_MODE
    } else {
        OWN_FILE_MODE
    };
    Ro::perms_from_mode(mode)
}

/// 改名 / 同根内移动。
///
/// 🔴 **两个参数各过一遍路径解析** —— 只解 `from` 的话，`to` 那一侧半路一条链接就能把东西搬到根外面去。
/// 从前这一句的理由是「能把任意文件改名成一份会话文件的名字」—— 那一道拦截用户拿掉了。
///
/// 🔴 **目标已经在了就拒**：unix 上系统那一步**会静默顶掉**已有的目标文件 —— 那就是一次
/// 没人问过的覆盖。从「先看一眼、在就拒」换成一次原子的不覆盖改名（[`rename_no_clobber`]）：看与改之间那个窗没了。
pub fn rename_entry(
    face: &Face,
    root: &Path,
    from: impl AsRef<Path>,
    to: impl AsRef<Path>,
) -> Result<PathBuf, WriteRefusal> {
    rename_entry_racing(face, root, from.as_ref(), to.as_ref(), &mut || {})
}

/// [`rename_entry`] 的本体；`between` 见 [`rename_no_clobber`]。
fn rename_entry_racing(
    face: &Face,
    root: &Path,
    from: &Path,
    to: &Path,
    between: &mut dyn FnMut(),
) -> Result<PathBuf, WriteRefusal> {
    let (src, dst, done) =
        rename_no_clobber(face, root, from, to, between, false).map_err(WriteRefusal::Refused)?;
    match done {
        Ok(()) => Ok(dst),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            Err(WriteRefusal::Exists(copy_text(
                "beFilesWrite.rename.exists",
                &[("path", &dst.display().to_string())],
            )))
        }
        Err(e) => Err(WriteRefusal::io(
            copy_text(
                "beFilesWrite.rename.failed",
                &[
                    ("src", &src.display().to_string()),
                    ("dst", &dst.display().to_string()),
                    ("why", &io_reason(e.kind())),
                ],
            ),
            &e,
        )),
    }
}

/// 删一个文件或一个**空**目录。**删的是链接本身**（不跟过去）。
///
/// 🔴 **本函数不递归，刻意的**：路径解析的射程是**一条路径**，而递归删动的是一整棵子树 ——
/// 顶上那一条解得干净，底下一条链接或一个挂载点照样会被一起动到。非空目录 ⇒ 系统报错、原样带回。
/// 递归删是**另一个函数**（[`delete_tree`]），逐条目过路径解析；
/// 线上要显式带 `recursive: true` 才走它 —— 不带，本函数的射程一个字节不变。
pub fn delete_entry(
    face: &Face,
    root: &Path,
    rel: impl AsRef<Path>,
) -> Result<PathBuf, WriteRefusal> {
    let target = walk_in_root(Last::Link, root, rel).map_err(WriteRefusal::Refused)?;
    let is_dir = Ro::symlink_metadata(&target)
        .map(|m| m.is_dir())
        .map_err(|e| {
            WriteRefusal::io(
                copy_text(
                    "reason.fs.readFailed",
                    &[
                        ("path", &target.display().to_string()),
                        ("why", &io_reason(e.kind())),
                    ],
                ),
                &e,
            )
        })?;
    let done = if is_dir {
        face.delete_dir(&target)
    } else {
        face.delete_file(&target)
    };
    done.map_err(|e| {
        WriteRefusal::io(
            copy_text(
                "beFilesWrite.delete.failed",
                &[
                    ("path", &target.display().to_string()),
                    ("why", &io_reason(e.kind())),
                ],
            ),
            &e,
        )
    })?;
    Ok(target.into_path_buf())
}

/// **带 CAS 的删一份文件**：盘上那份逐字节 == `expect` 才删，否则一个字节不动。
///
/// 读改写那一族（`Door::put` 的 `expect`）缺的最后一格：调用方「读到的是这一份 ⇒ 删它」，
/// 从前只能先 `Door::peek` 核、再 `files-delete` 删，核与删之间整整一趟往返的窗。
/// 这里把「核」挪到删的同一个进程里、紧贴着删那一下（窗缩到本函数里那两行之间 —— TOCTOU 照旧如实登记，
/// 同本模块头注诚实边界第 1 条）。
///
/// 目标**必须是普通文件**（不跟链接地看）：是目录 / 链接 / 别的 ⇒ `Refused`（CAS 比的是一份文件的字节，
/// 链接的字节是它指向的那一份 —— 删的却是链接本身，两者对不上，不给这一形）。
/// 不在 ⇒ `Stale`（读的时候还在）；在但不等 ⇒ `Stale`。与 [`delete_entry`] 同一道路径解析、同一个删的动词。
pub fn delete_file_expecting(
    face: &Face,
    root: &Path,
    rel: impl AsRef<Path>,
    expect: &[u8],
) -> Result<PathBuf, WriteRefusal> {
    let target = walk_in_root(Last::Link, root, rel).map_err(WriteRefusal::Refused)?;
    let md = match Ro::symlink_metadata(&target) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(WriteRefusal::Stale(copy_text(
                "beFilesWrite.delete.gone",
                &[("path", &target.display().to_string())],
            )))
        }
        Err(e) => {
            return Err(WriteRefusal::io(
                copy_text(
                    "reason.fs.readFailed",
                    &[
                        ("path", &target.display().to_string()),
                        ("why", &io_reason(e.kind())),
                    ],
                ),
                &e,
            ))
        }
    };
    if !md.is_file() {
        return Err(WriteRefusal::Refused(copy_text(
            "beFilesWrite.delete.notRegular",
            &[("path", &target.display().to_string())],
        )));
    }
    let current = Ro::read_nofollow(&target).map_err(|e| {
        WriteRefusal::io(
            copy_text(
                "reason.fs.readFailed",
                &[
                    ("path", &target.display().to_string()),
                    ("why", &io_reason(e.kind())),
                ],
            ),
            &e,
        )
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
    face.delete_file(&target).map_err(|e| {
        WriteRefusal::io(
            copy_text(
                "beFilesWrite.delete.failed",
                &[
                    ("path", &target.display().to_string()),
                    ("why", &io_reason(e.kind())),
                ],
            ),
            &e,
        )
    })?;
    Ok(target.into_path_buf())
}

/// 〔SU1 问 2〕**只删一个空目录**（`files-delete` 带 `expect: {"empty_dir": true}`）：
/// 「我看到的是一个空目录，删它」—— 与带逐字节 `expect` 删文件那一形对称的 CAS。
///
/// - 目标（不跟链接地看）必须是一个**真目录**：是文件 / 链接 / 别的 ⇒ `Refused`（这一形只对目录）。
/// - 不在 ⇒ `Stale`（看的时候还在）；**不空 ⇒ `Stale`**：`remove_dir` 自己就是原子的「空才删」，系统拒非空、
///   本函数只把那一下翻成 `stale`（说的是「你看的时候它空，此刻不空了」）—— 没有先看后删的窗。
/// - 删的动词与 [`delete_entry`] 删空目录那一支同一个（闭集里的「删空目录」），路径解析同一道。
pub fn delete_empty_dir(
    face: &Face,
    root: &Path,
    rel: impl AsRef<Path>,
) -> Result<PathBuf, WriteRefusal> {
    let target = walk_in_root(Last::Link, root, rel).map_err(WriteRefusal::Refused)?;
    let md = match Ro::symlink_metadata(&target) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(WriteRefusal::Stale(copy_text(
                "beFilesWrite.rmdir.gone",
                &[("path", &target.display().to_string())],
            )))
        }
        Err(e) => {
            return Err(WriteRefusal::io(
                copy_text(
                    "reason.fs.readFailed",
                    &[
                        ("path", &target.display().to_string()),
                        ("why", &io_reason(e.kind())),
                    ],
                ),
                &e,
            ))
        }
    };
    if !md.is_dir() {
        return Err(WriteRefusal::Refused(copy_text(
            "beFilesWrite.rmdir.notDir",
            &[("path", &target.display().to_string())],
        )));
    }
    match face.delete_dir(&target) {
        Ok(()) => Ok(target.into_path_buf()),
        Err(e) if e.kind() == std::io::ErrorKind::DirectoryNotEmpty => {
            Err(WriteRefusal::Stale(copy_text(
                "beFilesWrite.rmdir.notEmpty",
                &[("path", &target.display().to_string())],
            )))
        }
        Err(e) => Err(WriteRefusal::io(
            copy_text(
                "beFilesWrite.delete.failed",
                &[
                    ("path", &target.display().to_string()),
                    ("why", &io_reason(e.kind())),
                ],
            ),
            &e,
        )),
    }
}

/// 线上那几个字（八进制写法）⇒ 权限位；两个界面都只交原文，解与拒只在这里。
/// 超出 `0o7777` 拒而不掩：敲了 `100644`（连文件类型位一起抄来的）掩成 `644` 就是改了别的东西。
pub fn mode_of_text(t: &str) -> Result<u32, WriteFail> {
    let t = t.trim();
    let said = if t.is_empty() {
        copy_text("beFilesWrite.mode.empty", &[])
    } else {
        match u32::from_str_radix(t, 8) {
            Ok(m) if m <= 0o7777 => return Ok(m),
            Ok(_) => copy_text("beFilesWrite.mode.outOfRange", &[]),
            Err(_) => copy_text("beFilesWrite.mode.notOctal", &[]),
        }
    };
    Err(WriteFail::from(("refused", said)))
}

/// 改 unix 权限位（只收低 12 位），另交**改之前**的权限位（低 12 位；读不到 ⇒ `None`）：调用方据它撤销。
/// **跟链接** ⇒ 走 `Last::Follow`。非 unix 平台上**如实回失败**，不假装改成了（`Permissions` 在那边只有一个只读位）。
pub fn change_mode_reporting(
    face: &Face,
    root: &Path,
    rel: impl AsRef<Path>,
    mode: u32,
) -> Result<(PathBuf, Option<u32>), WriteRefusal> {
    let real = walk_in_root(Last::Follow, root, rel).map_err(WriteRefusal::Refused)?;
    #[cfg(unix)]
    {
        let before = Ro::metadata(&real)
            .ok()
            .and_then(|m| Ro::mode(&m))
            .map(|m| m & 0o7777);
        let perms = Ro::perms_from_mode(mode).expect("unix 上恒有权限位");
        face.set_permissions(&real, perms).map_err(|e| {
            WriteRefusal::io(
                copy_text(
                    "beFilesWrite.chmod.failed",
                    &[
                        ("path", &real.display().to_string()),
                        ("why", &io_reason(e.kind())),
                    ],
                ),
                &e,
            )
        })?;
        Ok((real.into_path_buf(), before))
    }
    #[cfg(not(unix))]
    {
        let _ = face;
        // 回 `no_unix_mode`，不回 `io_failed`（理由住 [`WriteRefusal::Unsupported`]）。
        Err(WriteRefusal::Unsupported(copy_text(
            "beFilesWrite.chmod.unsupported",
            &[("path", &real.display().to_string())],
        )))
    }
}

/// 覆盖写一份**已经在**的普通文件（跟链接：改真文件）。不在 ⇒ `Stale`；不是普通文件 ⇒ `Refused`。
/// 新建一份请走 [`create_new_file`]（那条是 `O_EXCL`：「新建」与「改既有」是两件风险不同的事）。怎么换住 [`replace_contents`]。
pub fn overwrite_text(
    face: &Face,
    root: &Path,
    rel: impl AsRef<Path>,
    bytes: &[u8],
) -> Result<PathBuf, WriteRefusal> {
    replace_contents(
        face,
        root,
        rel.as_ref(),
        Content::Bytes(bytes),
        None,
        Was::Old,
        &mut || {},
    )
    .map(|(p, _)| p)
}

/// 一份文件的 `(硬链接数, 属主 uid)`（跟链接地看）。**非 unix 上没有这两个概念 ⇒ `None`**（那边照原子换 / Windows 臂办）。
/// 硬链接数与属主经 `Ro::nlink` · `Ro::uid` 读（同 `kind_and_device` 读设备号）。
#[cfg(unix)]
fn links_and_owner(p: &Path) -> Option<(u64, u32)> {
    let m = Ro::metadata(p).ok()?;
    Some((Ro::nlink(&m)?, Ro::uid(&m)?))
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
// 只是换了表示 —— 逐字节形（`Door::put` / `files-delete`）留给小写。为什么编辑器要摘要形：存盘分块那一支本来就是
// 因为**一行装不下**（一行 1 MiB、编辑上限 8 MiB），逐字节的 `expect` 要么让装得进一行的门槛减半，
// 要么让原文再分块送一遍（大文件存盘流量翻倍）。摘要定长，两支同形。
//
// 🔴 摘要**只在后端算**（读的那一趟 `files-read-text` 交出 `sha256`、写成之后应答里交新的）——窗口当不透明令牌存着再交回来，
//   算法只有一个家：[`crate::files::content_sha256`]（住读族那一侧：读的那一趟要它，而读族不许伸手进写面 ——
//   `readonly_guard` 「写面只有一扇门」那条钉着）。

pub use crate::files::{content_sha256, SHA256_HEX_LEN};

/// **带 CAS 的覆盖写**：盘上那份的摘要 == `expect_sha256` 才交给 [`overwrite_text`]，否则一个字节不写。
///
/// - 目标不在 ⇒ `Stale`（看的时候还在）；不是普通文件 ⇒ `Refused`（同 [`overwrite_text`]）；
///   「盘上有字节、读出来却是空的」⇒ `Io`（同读改写那一句，继续走就是拿新内容盖掉一份没读到的原文）。
/// - 摘要不等 ⇒ `Stale`：调用方该让人决定（重开 / 仍然覆盖），不是重试同一份。
/// - 比对与写之间仍有窗（TOCTOU，同本模块头注诚实边界第 1 条）：CAS 缩小的是「窗口读 → 后端写」那一整趟往返的窗。
/// - **写法本身不在这里**：比完交给 [`overwrite_text`] 那一个原语（它怎么落盘由那一处定）。
pub fn overwrite_text_expecting(
    face: &Face,
    root: &Path,
    rel: impl AsRef<Path>,
    bytes: &[u8],
    expect_sha256: &str,
) -> Result<PathBuf, WriteRefusal> {
    let rel = rel.as_ref();
    let at = walk_in_root(Last::Link, root, rel).map_err(WriteRefusal::Refused)?;
    match Ro::symlink_metadata(&at) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(WriteRefusal::Stale(copy_text(
                "beFilesWrite.overwrite.gone",
                &[("path", &at.display().to_string())],
            )))
        }
        Err(e) => {
            return Err(WriteRefusal::io(
                copy_text(
                    "reason.fs.readFailed",
                    &[
                        ("path", &at.display().to_string()),
                        ("why", &io_reason(e.kind())),
                    ],
                ),
                &e,
            ))
        }
        Ok(_) => {}
    }
    let real = walk_in_root(Last::Follow, root, rel).map_err(WriteRefusal::Refused)?;
    let md = Ro::metadata(&real).map_err(|e| {
        WriteRefusal::io(
            copy_text(
                "reason.fs.readFailed",
                &[
                    ("path", &real.display().to_string()),
                    ("why", &io_reason(e.kind())),
                ],
            ),
            &e,
        )
    })?;
    if !md.is_file() {
        return Err(WriteRefusal::Refused(copy_text(
            "beFilesWrite.overwrite.notRegular",
            &[("path", &real.display().to_string())],
        )));
    }
    let current = crate::files::file_sha256(&real).map_err(|e| {
        WriteRefusal::io(
            copy_text(
                "reason.fs.readFailed",
                &[
                    ("path", &real.display().to_string()),
                    ("why", &io_reason(e.kind())),
                ],
            ),
            &e,
        )
    })?;
    if md.len() > 0 && current == content_sha256(b"") {
        return Err(WriteRefusal::Io(format!(
            "refuse write: {}",
            hollow_read(&real, md.len())
        )));
    }
    if current != expect_sha256 {
        return Err(WriteRefusal::Stale(copy_text(
            "beFilesWrite.overwrite.changed",
            &[("path", &real.display().to_string())],
        )));
    }
    overwrite_text(face, root, rel, bytes)
}

// ══════════════════════════════════════════════════════════════════════════
// **递归删：逐条目过路径解析**（原标题「逐条目过围栏」）
// ══════════════════════════════════════════════════════════════════════════
//
// 设计全文住第一节。三件承重的事：
//
// 1. **不用那个一步递归删的库函数**（`Face` 上没有它）：它的遍历不经过我们的路径解析，
//    删的是「那一刻盘上的东西」而不是「判过的东西」。
// 2. **两趟**：计划趟（只读）逐条目过路径解析，任一条被拒 ⇒ 整趟拒、一个字节不动；
//    执行趟**只删计划里的**，每一条**当场再过一次**路径解析，只用「删文件」与「删空目录」两个既有动词
//    ⇒ 计划之外新长出来的东西绝不会被连带删掉：它所在的目录不空，删目录那一步失败 ⇒ 停。
// 3. **列举与改动分住两个函数**（[`plan_tree`] 只列不删；[`remove_planned`] 只删一条、先过它自己那一条的路径解析）。
//    执行趟删的每一条都是当场再从根句柄走一遍交回的 `Resolved`：半路一层被换成链接 ⇒ 走不下去、停，删不到根外。

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
    Ro::symlink_metadata(p).map(|m| (m.is_dir(), Ro::dev(&m).unwrap_or(0)))
}

#[cfg(not(unix))]
fn kind_and_device(p: &Path) -> std::io::Result<(bool, u64)> {
    Ro::symlink_metadata(p).map(|m| (m.is_dir(), 0))
}

/// **计划趟（只读）**：从 `rel` 起、不跟链接地走整棵树，**每一条目各过一次路径解析**。
///
/// 回先序表（每一条都排在它所有后代之前 ⇒ 倒过来就是能逐条删空的次序）。
/// 整趟拒的几形（都**一个字节不动**）：任一条目被路径解析拒（`refused`，话里点名是哪一条）·
/// 树里跨了挂载点（`refused`：不走进另一个文件系统去删）· 条目数超过 [`TREE_ENTRY_CAP`]（`refused`）·
/// 列不出某一层（`io_failed`）。
///
/// 🔴 **本函数里一个改动都没有**（不收 `Face`）—— 列举与改动分住两个函数。
pub fn plan_tree(root: &Path, rel: impl AsRef<Path>) -> Result<Vec<Planned>, WriteRefusal> {
    plan_tree_within(root, rel.as_ref(), TREE_ENTRY_CAP)
}

/// [`plan_tree`] 的本体，上限由调用方给（判据拿一个小上限验「超了整趟拒」，不必真铺十万条）。
pub fn plan_tree_within(root: &Path, rel: &Path, cap: usize) -> Result<Vec<Planned>, WriteRefusal> {
    let top = walk_in_root(Last::Link, root, rel).map_err(WriteRefusal::Refused)?;
    let (top_is_dir, top_dev) = kind_and_device(&top).map_err(|e| {
        WriteRefusal::io(
            copy_text(
                "reason.fs.readFailed",
                &[
                    ("path", &top.display().to_string()),
                    ("why", &io_reason(e.kind())),
                ],
            ),
            &e,
        )
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
        let dir_at = walk_in_root(Last::Link, root, &dir_rel).map_err(WriteRefusal::Refused)?;
        let listing = Ro::read_dir(&dir_at).map_err(|e| {
            WriteRefusal::io(
                copy_text(
                    "reason.fs.readFailed",
                    &[
                        ("path", &dir_at.display().to_string()),
                        ("why", &io_reason(e.kind())),
                    ],
                ),
                &e,
            )
        })?;
        for item in listing {
            let item = item.map_err(|e| {
                WriteRefusal::io(
                    copy_text(
                        "reason.fs.readFailed",
                        &[
                            ("path", &dir_at.display().to_string()),
                            ("why", &io_reason(e.kind())),
                        ],
                    ),
                    &e,
                )
            })?;
            let child_rel = dir_rel.join(item.file_name());
            // ★ **逐条目过路径解析**：这一条就是本函数存在的理由。
            let at = walk_in_root(Last::Link, root, &child_rel).map_err(|m| {
                WriteRefusal::Refused(copy_text(
                    "beFilesWrite.deleteTree.entryRefused",
                    &[("why", &m)],
                ))
            })?;
            let (is_dir, dev) = kind_and_device(&at).map_err(|e| {
                WriteRefusal::io(
                    copy_text(
                        "reason.fs.readFailed",
                        &[
                            ("path", &at.display().to_string()),
                            ("why", &io_reason(e.kind())),
                        ],
                    ),
                    &e,
                )
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
pub fn remove_planned(face: &Face, root: &Path, p: &Planned) -> Result<PathBuf, WriteRefusal> {
    let at = walk_in_root(Last::Link, root, &p.rel).map_err(WriteRefusal::Refused)?;
    let is_dir = Ro::symlink_metadata(&at).map(|m| m.is_dir()).map_err(|e| {
        WriteRefusal::io(
            copy_text(
                "reason.fs.readFailed",
                &[
                    ("path", &at.display().to_string()),
                    ("why", &io_reason(e.kind())),
                ],
            ),
            &e,
        )
    })?;
    if is_dir != p.is_dir {
        return Err(WriteRefusal::Io(copy_text(
            "beFilesWrite.delete.changed",
            &[("path", &at.display().to_string())],
        )));
    }
    let done = if is_dir {
        face.delete_dir(&at)
    } else {
        face.delete_file(&at)
    };
    done.map_err(|e| {
        WriteRefusal::io(
            copy_text(
                "beFilesWrite.delete.failed",
                &[
                    ("path", &at.display().to_string()),
                    ("why", &io_reason(e.kind())),
                ],
            ),
            &e,
        )
    })?;
    Ok(at.into_path_buf())
}

/// **递归删**：计划（逐条目过路径解析）→ 按计划倒序逐条删（每条当场再过一次路径解析）。
///
/// 回 `(目标的落点, 真删掉了几条)`。执行趟中途停下 ⇒ 已删的删掉了（与任何递归删同形），
/// 话里带「删了几条之后停在哪一条」。计划趟被拒 ⇒ 一条都没删。
///
/// 🔴 本函数自己**不含任何改动动词**：删那一下住 [`remove_planned`]，列那一下住 [`plan_tree`]。
pub fn delete_tree(
    face: &Face,
    root: &Path,
    rel: impl AsRef<Path>,
) -> Result<(PathBuf, usize), WriteRefusal> {
    delete_tree_upto(face, root, rel, None).map(|(top, removed, _)| (top, removed))
}

/// 同 [`delete_tree`]，这一趟至多删 `limit` 条就停在两条之间（叶子先删，剩下的仍是一棵连着的树）：
/// 回 `(目标的落点, 这一趟删了几条, 还剩几条)`。一趟删多少由调用方定（后端不看钟）；`limit` 为 0 也删一条，一趟接一趟总能删完。
pub fn delete_tree_upto(
    face: &Face,
    root: &Path,
    rel: impl AsRef<Path>,
    limit: Option<usize>,
) -> Result<(PathBuf, usize, usize), WriteRefusal> {
    let rel = rel.as_ref();
    let top = walk_in_root(Last::Link, root, rel).map_err(WriteRefusal::Refused)?;
    let plan = plan_tree(root, rel)?;
    let mut removed = 0usize;
    for p in plan.iter().rev() {
        if removed > 0 && limit.is_some_and(|n| removed >= n) {
            return Ok((top.into_path_buf(), removed, plan.len() - removed));
        }
        remove_planned(face, root, p).map_err(|e| {
            let said = copy_text(
                "beFilesWrite.deleteTree.stopped",
                &[
                    ("why", e.message()),
                    ("n", &removed.to_string()),
                    ("total", &plan.len().to_string()),
                ],
            );
            e.restated(said)
        })?;
        removed += 1;
    }
    Ok((top.into_path_buf(), removed, 0))
}

/// 复制一份普通文件 `from` → `to`（`overwrite` 只对目标已在时有意义）。源解到底（根里一条指向根外的链接不许借它把根外那一份复制进来）；
/// 落法全交 [`replace_contents`]：新建权限位从源抄、上位不覆盖（目标已在 ⇒ `exists`，一个字节不动）；
/// 盖掉一份已在的 ⇒ 解到底改真文件、权限位沿用目标（同 `cp`）。目录整棵见 [`copy_tree`]。
pub fn copy_entry(
    face: &Face,
    root: &Path,
    from: impl AsRef<Path>,
    to: impl AsRef<Path>,
    overwrite: bool,
) -> Result<(PathBuf, u64), WriteRefusal> {
    copy_entry_racing(
        face,
        root,
        from.as_ref(),
        to.as_ref(),
        overwrite,
        &mut || {},
    )
}

/// [`copy_entry`] 的本体；`between` 见 [`replace_contents`]。
fn copy_entry_racing(
    face: &Face,
    root: &Path,
    from: &Path,
    to: &Path,
    overwrite: bool,
    between: &mut dyn FnMut(),
) -> Result<(PathBuf, u64), WriteRefusal> {
    let src = walk_in_root(Last::Follow, root, from).map_err(WriteRefusal::Refused)?;
    let dst = walk_in_root(Last::Link, root, to).map_err(WriteRefusal::Refused)?;
    if src == dst {
        return Err(WriteRefusal::Refused(copy_text(
            "beFilesWrite.copy.same",
            &[("path", &dst.display().to_string())],
        )));
    }
    let src_md = Ro::metadata(&src).map_err(|e| {
        WriteRefusal::io(
            copy_text(
                "reason.fs.readFailed",
                &[
                    ("path", &src.display().to_string()),
                    ("why", &io_reason(e.kind())),
                ],
            ),
            &e,
        )
    })?;
    if !src_md.is_file() {
        return Err(WriteRefusal::Refused(copy_text(
            "beFilesWrite.copy.notRegular",
            &[("path", &src.display().to_string())],
        )));
    }
    let was = if overwrite { Was::Either } else { Was::New };
    replace_contents(
        face,
        root,
        to,
        Content::File(&src),
        Some(src_md.permissions()),
        was,
        between,
    )
}

// ══════════════════════════════════════════════════════════════════════════
// **复制目录：逐条目过路径解析**（照递归删的形状）
// ══════════════════════════════════════════════════════════════════════════
//
// 设计住 §2.2。与递归删（[`delete_tree`]）同一个形状：
//
// 1. **计划趟只读**（[`plan_copy_within`]）：源解到底一次，然后不跟链接地走整棵，每一条目过一次 `Last::Link`；
//    链接 / 设备 / 管道 / 套接字 ⇒ 整趟拒（这一趟不建链接）；
//    跨挂载点 ⇒ 整趟拒；条目数 > [`TREE_ENTRY_CAP`] ⇒ 整趟拒。**一个改动都没有。**
// 2. **执行趟**（[`copy_planned`]）先序逐条：源与目标**当场再过一次**路径解析、不跟链接地核源的种类没变；
//    目录 ⇒ 建目录（目标已在 ⇒ 系统报错 ⇒ 停：目录复制不合并）；文件 ⇒ [`replace_contents`]（新建 · 抄权限位）。
//    全部落完后目录的权限位**倒序**抄（先抄的话一个只读目录里就建不出下一层）。
// 3. **中途失败 ⇒ 回滚**（[`undo_made`]）：倒序删掉这一趟**自己建的**，各过一次路径解析；撤不干净就说哪一条。
//
// ⚠ 不添动词：建目录 · `O_EXCL` 新建 · 写 · 改权限 · 删文件 · 删空目录，全在闭集里。
// ⚠ 源在计划与执行之间被换了种类 ⇒ 停、回滚。

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
    let ft = Ro::symlink_metadata(p)?.file_type();
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

/// [`copy_kind`] 那两档「复制不了的种类」说给人听的那几个字（文案表）。
fn kind_words(what: &str) -> String {
    match what {
        "link" => copy_text("beFilesWrite.copyTree.kindLink", &[]),
        _ => copy_text("beFilesWrite.copyTree.kindOther", &[]),
    }
}

/// 复制目录那几句话里「这一条」的路径。
fn shown(p: &Path) -> String {
    win_path_core::shown(p)
}

/// **复制目录的计划趟（只读）**。整趟拒的几形都**一个字节不动**（本函数一个改动都没有）。
pub fn plan_copy_within(root: &Path, from: &Path, cap: usize) -> Result<CopyPlan, WriteRefusal> {
    let real = walk_in_root(Last::Follow, root, from).map_err(WriteRefusal::Refused)?;
    let real_root = Ro::canonicalize(root).map_err(|e| {
        WriteRefusal::Refused(copy_text(
            "beFilesWrite.copyTree.rootUnresolved",
            &[("path", &shown(root)), ("why", &io_reason(e.kind()))],
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
        WriteRefusal::io(
            copy_text(
                "reason.fs.readFailed",
                &[("path", &shown(&real)), ("why", &io_reason(e.kind()))],
            ),
            &e,
        )
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
        let dir_at = walk_in_root(Last::Link, &real_root, join_tail(&src, &tail))
            .map_err(WriteRefusal::Refused)?;
        let listing = Ro::read_dir(&dir_at).map_err(|e| {
            WriteRefusal::io(
                copy_text(
                    "reason.fs.readFailed",
                    &[("path", &shown(&dir_at)), ("why", &io_reason(e.kind()))],
                ),
                &e,
            )
        })?;
        for item in listing {
            let item = item.map_err(|e| {
                WriteRefusal::io(
                    copy_text(
                        "reason.fs.readFailed",
                        &[("path", &shown(&dir_at)), ("why", &io_reason(e.kind()))],
                    ),
                    &e,
                )
            })?;
            let child_tail = tail.join(item.file_name());
            // ★ **逐条目过路径解析**。
            let at = walk_in_root(Last::Link, &real_root, src.join(&child_tail)).map_err(|m| {
                WriteRefusal::Refused(copy_text(
                    "beFilesWrite.copyTree.entryRefused",
                    &[("why", &m)],
                ))
            })?;
            let (kind, dev) = copy_kind(&at).map_err(|e| {
                WriteRefusal::io(
                    copy_text(
                        "reason.fs.readFailed",
                        &[("path", &shown(&at)), ("why", &io_reason(e.kind()))],
                    ),
                    &e,
                )
            })?;
            // 链接 ⇒ 记下它的目标文本（原样，不跟进去）；平台建不了链接时照旧整趟拒。
            let link_to = match kind {
                Err("link") if super::files_extract::LINKS_SUPPORTED => {
                    Some(Ro::read_link(&at).map_err(|e| {
                        WriteRefusal::io(
                            copy_text(
                                "reason.fs.readFailed",
                                &[("path", &shown(&at)), ("why", &io_reason(e.kind()))],
                            ),
                            &e,
                        )
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
pub fn copy_planned(
    face: &Face,
    plan: &CopyPlan,
    p: &CopyPlanned,
    dst_rel: &Path,
) -> Result<u64, WriteRefusal> {
    let src = walk_in_root(Last::Link, &plan.root, join_tail(&plan.src, &p.tail))
        .map_err(WriteRefusal::Refused)?;
    let dst = walk_in_root(Last::Link, &plan.root, dst_rel).map_err(WriteRefusal::Refused)?;
    let unreadable = |e: std::io::Error| {
        WriteRefusal::io(
            copy_text(
                "reason.fs.readFailed",
                &[("path", &shown(&src)), ("why", &io_reason(e.kind()))],
            ),
            &e,
        )
    };
    let (kind, _) = copy_kind(&src).map_err(unreadable)?;
    // 链接：当场再核「仍是链接、目标文本没变」，再建一条同文本的链接（住 `files_extract::land_link`）。
    if let Some(to) = &p.link_to {
        let now = Ro::read_link(&src).map_err(unreadable)?;
        if kind != Err("link") || &now != to {
            return Err(WriteRefusal::Io(copy_text(
                "beFilesWrite.copyTree.kindChanged",
                &[("path", &shown(&src))],
            )));
        }
        super::files_extract::land_link(face, &plan.root, dst_rel, to)?;
        return Ok(0);
    }
    if kind != Ok(p.is_dir) {
        return Err(WriteRefusal::Io(copy_text(
            "beFilesWrite.copyTree.kindChanged",
            &[("path", &shown(&src))],
        )));
    }
    if p.is_dir {
        face.make_dir(&dst).map_err(|e| {
            WriteRefusal::io(
                copy_text(
                    "reason.fs.writeFailed",
                    &[("path", &shown(&dst)), ("why", &io_reason(e.kind()))],
                ),
                &e,
            )
        })?;
        return Ok(0);
    }
    let perms = Ro::metadata(&src).map_err(unreadable)?.permissions();
    replace_contents(
        face,
        &plan.root,
        dst_rel,
        Content::File(&src),
        Some(perms),
        Was::New,
        &mut || {},
    )
    .map(|(_, n)| n)
}

/// 目录的权限位抄过去（全部落完之后倒序调）。两边各过一次路径解析。
fn copy_dir_mode(
    face: &Face,
    plan: &CopyPlan,
    p: &CopyPlanned,
    dst_rel: &Path,
) -> Result<(), WriteRefusal> {
    let src = walk_in_root(Last::Link, &plan.root, join_tail(&plan.src, &p.tail))
        .map_err(WriteRefusal::Refused)?;
    let dst = walk_in_root(Last::Link, &plan.root, dst_rel).map_err(WriteRefusal::Refused)?;
    let perms = Ro::metadata(&src)
        .map_err(|e| {
            WriteRefusal::io(
                copy_text(
                    "reason.fs.readFailed",
                    &[("path", &shown(&src)), ("why", &io_reason(e.kind()))],
                ),
                &e,
            )
        })?
        .permissions();
    face.set_permissions(&dst, perms).map_err(|e| {
        WriteRefusal::io(
            copy_text(
                "beFilesWrite.copy.modeFailed",
                &[("path", &shown(&dst)), ("why", &io_reason(e.kind()))],
            ),
            &e,
        )
    })
}

/// 回滚：倒序删掉这一趟**自己建的**（`made` 是按建出来的先后记的目标相对段 ＋ 是不是目录）。
/// 回 `(撤掉了几条, 撤不掉的那一条的说法)`。
fn undo_made(face: &Face, root: &Path, made: &[(PathBuf, bool)]) -> (usize, Option<String>) {
    let mut undone = 0usize;
    for (rel, is_dir) in made.iter().rev() {
        let at = match walk_in_root(Last::Link, root, rel) {
            Ok(a) => a,
            Err(m) => return (undone, Some(m)),
        };
        let done = if *is_dir {
            face.delete_dir(&at)
        } else {
            face.delete_file(&at)
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
    face: &Face,
    root: &Path,
    from: impl AsRef<Path>,
    to: impl AsRef<Path>,
) -> Result<TreeCopied, WriteRefusal> {
    copy_tree_with(face, root, from.as_ref(), to.as_ref(), TREE_ENTRY_CAP)
}

/// [`copy_tree`] 的本体，上限由调用方给（判据拿小上限验「超了整趟拒」）。
pub fn copy_tree_with(
    face: &Face,
    root: &Path,
    from: &Path,
    to: &Path,
    cap: usize,
) -> Result<TreeCopied, WriteRefusal> {
    let dst_top = walk_in_root(Last::Link, root, to).map_err(WriteRefusal::Refused)?;
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
        match copy_planned(face, &plan, p, &d) {
            Ok(n) => {
                bytes += n;
                made.push((d, p.is_dir));
            }
            Err(e) => {
                let (undone, stuck) = undo_made(face, &plan.root, &made);
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
                return Err(e.restated(said));
            }
        }
    }
    for p in plan.entries.iter().rev().filter(|p| p.is_dir) {
        let d = join_tail(&dst_rel, &p.tail);
        copy_dir_mode(face, &plan, p, &d).map_err(|e| {
            let said = copy_text(
                "beFilesWrite.copyTree.modeNotCopied",
                &[("why", e.message())],
            );
            match e.raw() {
                Some(raw) => WriteRefusal::IoSaid {
                    said,
                    raw: raw.to_string(),
                },
                None => WriteRefusal::Io(said),
            }
        })?;
    }
    let dirs = plan.entries.iter().filter(|p| p.is_dir).count();
    let links = plan.entries.iter().filter(|p| p.link_to.is_some()).count();
    Ok(TreeCopied {
        path: dst_top.into_path_buf(),
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
//   删会话那一条的 `Resolved` 由 `fenced_session_file` 担保，判据钉它在生产树里恰好被调用一处。
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

/// `Door::peek` 一趟肯交的上限。
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
pub fn peek_text(root: &Path, rel: impl AsRef<Path>) -> Result<Peeked, WriteFail> {
    let rel = rel.as_ref();
    let at = walk_in_root(Last::Link, root, rel).map_err(|m| WriteFail::from(("refused", m)))?;
    match Ro::symlink_metadata(&at) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok(Peeked {
                path: at.into_path_buf(),
                text: None,
            })
        }
        Err(e) => {
            return Err(WriteFail::io(
                copy_text(
                    "reason.fs.readFailed",
                    &[
                        ("path", &at.display().to_string()),
                        ("why", &io_reason(e.kind())),
                    ],
                ),
                &e,
            ))
        }
        Ok(_) => {}
    }
    let real =
        walk_in_root(Last::Follow, root, rel).map_err(|m| WriteFail::from(("refused", m)))?;
    let md = Ro::metadata(&real).map_err(|e| {
        WriteFail::io(
            copy_text(
                "reason.fs.readFailed",
                &[
                    ("path", &real.display().to_string()),
                    ("why", &io_reason(e.kind())),
                ],
            ),
            &e,
        )
    })?;
    if !md.is_file() {
        return Err(WriteFail::from((
            "refused",
            copy_text(
                "beFilesWrite.peek.notRegular",
                &[("path", &real.display().to_string())],
            ),
        )));
    }
    if md.len() > PEEK_MAX_BYTES as u64 {
        return Err(WriteFail::from((
            "too_large",
            copy_text(
                "beFilesWrite.peek.tooBig",
                &[
                    ("path", &real.display().to_string()),
                    ("size", &md.len().to_string()),
                    ("max", &PEEK_MAX_BYTES.to_string()),
                ],
            ),
        )));
    }
    let bytes = Ro::read_nofollow(&real).map_err(|e| {
        WriteFail::io(
            copy_text(
                "reason.fs.readFailed",
                &[
                    ("path", &real.display().to_string()),
                    ("why", &io_reason(e.kind())),
                ],
            ),
            &e,
        )
    })?;
    if bytes.is_empty() && md.len() > 0 {
        return Err(WriteFail::from(("io_failed", hollow_read(&real, md.len()))));
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
        path: real.into_path_buf(),
        text: Some(text),
    })
}

/// 「盘上有字节、读出来却是空的」那一句（v1.7.9 那次事故：OneDrive 占位 / 杀毒软件锁着）。
///
/// 🔴 继续走的后果是拿「空 ＋ 新内容」整份盖掉原文 —— 读改写的读那一半与写那一半都在这里停。
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

/// 🔴 **用户文件整份替换的那一个序列**（从前 monitor 的 `fenced_block::apply`，今天只有这一份）：
///
/// ① 需要时逐级补父目录 → ② 过路径解析 → ③ **CAS**：盘上那份必须逐字节等于 `expect`（`None` = 必须不存在）
/// → ④ 与新内容逐字节相同 ⇒ 不写 → ⑤ 要备份就 `O_EXCL` 另存原文 → ⑥ 同目录暂存旁名写满、换名上位
/// → ⑦ 回读逐字节比对 → 不符就回滚（原来在 ⇒ 把原文换回去；原来不在 ⇒ 删掉刚建的）。
///
/// ⚠ TOCTOU 照旧在（同本模块头注诚实边界第 1 条）：③ 与 ⑥ 之间有一个窗。窗里被别人改了，
///    ⑥ 会把那一次改动盖掉 —— CAS 缩小的是「monitor 读 → 后端写」那一整趟往返的窗，不是这一个。
pub fn put_text(
    face: &Face,
    root: &Path,
    rel: impl AsRef<Path>,
    bytes: &[u8],
    expect: Option<&[u8]>,
    keep_backup: bool,
    parents: bool,
) -> Result<Put, WriteRefusal> {
    put_text_racing(
        face,
        root,
        rel.as_ref(),
        bytes,
        expect,
        keep_backup,
        parents,
        &mut || {},
    )
}

/// [`put_text`] 的本体；`between` 交给 [`replace_contents`]（CAS 判完、旁名写满之后，换名上位之前）。
fn put_text_racing(
    face: &Face,
    root: &Path,
    rel: &Path,
    bytes: &[u8],
    expect: Option<&[u8]>,
    keep_backup: bool,
    parents: bool,
    between: &mut dyn FnMut(),
) -> Result<Put, WriteRefusal> {
    if parents {
        make_parents(face, root, rel)?;
    }
    let at = walk_in_root(Last::Link, root, rel).map_err(WriteRefusal::Refused)?;
    let existed = match Ro::symlink_metadata(&at) {
        Ok(_) => true,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => false,
        Err(e) => {
            return Err(WriteRefusal::io(
                copy_text(
                    "reason.fs.readFailed",
                    &[
                        ("path", &at.display().to_string()),
                        ("why", &io_reason(e.kind())),
                    ],
                ),
                &e,
            ))
        }
    };
    let (dst, current, perms) = if existed {
        let real = walk_in_root(Last::Follow, root, rel).map_err(WriteRefusal::Refused)?;
        let md = Ro::metadata(&real).map_err(|e| {
            WriteRefusal::io(
                copy_text(
                    "reason.fs.readFailed",
                    &[
                        ("path", &real.display().to_string()),
                        ("why", &io_reason(e.kind())),
                    ],
                ),
                &e,
            )
        })?;
        if !md.is_file() {
            return Err(WriteRefusal::Refused(copy_text(
                "beFilesWrite.overwrite.notRegular",
                &[("path", &real.display().to_string())],
            )));
        }
        let cur = Ro::read_nofollow(&real).map_err(|e| {
            WriteRefusal::io(
                copy_text(
                    "reason.fs.readFailed",
                    &[
                        ("path", &real.display().to_string()),
                        ("why", &io_reason(e.kind())),
                    ],
                ),
                &e,
            )
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
            path: dst.into_path_buf(),
            bytes: bytes.len(),
            changed: false,
            created: false,
            backup: None,
        });
    }
    let backup = match current.as_deref() {
        Some(orig) if keep_backup && !orig.is_empty() => {
            Some(land_backup(face, root, rel, orig, perms.clone())?)
        }
        _ => None,
    };
    // 原来不在（`expect: null`）⇒ 上位不许顶掉任何东西：CAS 判完「不在」之后冒出来的那一份照样不盖，回 `stale`。
    let was = if current.is_none() {
        Was::New
    } else {
        Was::Old
    };
    match replace_contents(face, root, rel, Content::Bytes(bytes), None, was, between) {
        Err(WriteRefusal::Exists(_)) => {
            return Err(WriteRefusal::Stale(copy_text(
                "beFilesWrite.put.appeared",
                &[("path", &dst.display().to_string())],
            )))
        }
        other => other?,
    };
    let back = Ro::read_nofollow(&dst);
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
                &[("why", &io_reason(e.kind()))],
            ),
        };
        let raw = back.as_ref().err().map(ToString::to_string);
        let undone = match current.as_deref() {
            Some(orig) => {
                let put_back = Content::Bytes(orig);
                replace_contents(face, root, rel, put_back, None, Was::Old, &mut || {}).is_ok()
            }
            None => remove_created(face, root, rel).is_ok(),
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
        let said = copy_text(
            "beFilesWrite.put.verifyFailed",
            &[
                ("path", &dst.display().to_string()),
                ("why", &why),
                ("note", &note),
            ],
        );
        return Err(match raw {
            Some(raw) => WriteRefusal::IoSaid { said, raw },
            None => WriteRefusal::Io(said),
        });
    }
    Ok(Put {
        path: dst.into_path_buf(),
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
fn make_parents(face: &Face, root: &Path, rel: &Path) -> Result<(), WriteRefusal> {
    let Some(parent) = rel.parent() else {
        return Ok(());
    };
    let mut acc = PathBuf::new();
    for comp in parent.components() {
        acc.push(comp);
        let at = walk_in_root(Last::Link, root, &acc).map_err(WriteRefusal::Refused)?;
        match Ro::metadata(&at) {
            Ok(m) if m.is_dir() => continue,
            Ok(_) => {
                return Err(WriteRefusal::Refused(copy_text(
                    "beFilesWrite.mkdirs.notDir",
                    &[("path", &at.display().to_string())],
                )))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                face.make_dir(&at).map_err(|e| {
                    WriteRefusal::io(
                        copy_text(
                            "reason.fs.writeFailed",
                            &[
                                ("path", &at.display().to_string()),
                                ("why", &io_reason(e.kind())),
                            ],
                        ),
                        &e,
                    )
                })?;
                if let Some(p) = own_mode(&at, true) {
                    face.set_permissions(&at, p).map_err(|e| {
                        WriteRefusal::io(
                            copy_text(
                                "beFilesWrite.chmod.failed",
                                &[
                                    ("path", &at.display().to_string()),
                                    ("why", &io_reason(e.kind())),
                                ],
                            ),
                            &e,
                        )
                    })?;
                }
            }
            Err(e) => {
                return Err(WriteRefusal::io(
                    copy_text(
                        "reason.fs.readFailed",
                        &[
                            ("path", &at.display().to_string()),
                            ("why", &io_reason(e.kind())),
                        ],
                    ),
                    &e,
                ))
            }
        }
    }
    Ok(())
}

/// [`replace_contents`] 换上去的内容从哪来。
pub(crate) enum Content<'a> {
    Bytes(&'a [u8]),
    /// 一份已在盘上的普通文件（复制的源），不跟链接地读。
    File(&'a Path),
    /// 上传的暂存件：同盘时它自己改名上位（不复制），跨盘退回复制；成了之后它就没了。
    Staged(&'a Resolved),
}

/// 那一格原来在不在：调用方判过的那一面；对不上 ⇒ 不动（`New` 而已在 ⇒ `Exists`；`Old` 而不在 ⇒ `Stale`）。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Was {
    New,
    Old,
    Either,
}

/// 旁名序号（同一进程连写两次也换一个旁名）。
static SWAP_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// **换掉一份文件的内容**（不在就新建）：写面覆盖写 · 读改写 · 复制 · 上传提交都只经这里。回 `(落点, 字节数)`。
///
/// - 已在：解到底改真文件（经链接跑出根 ⇒ 拒）；权限位沿用它；有硬链接 / 属主不是后端这个用户 / Windows（换名会换掉 ACL）⇒
///   就地写（截断再写 ＋ 落盘，记一行「这一次不是原子写」）。xattr 不跟过来是已知代价。
/// - 不在：权限位取 `fresh`，缺 ⇒ 落在后端自家目录里的收紧、别处按 umask。上位走不覆盖改名。
/// - 原子那一支：同目录 `O_EXCL` 旁名 → 先定权限位 → 写满 → 落盘 → 上位 → 目录落盘；失败删掉自己的旁名，目标不动。
///   暂存件同盘时省掉旁名（定权限位 ＋ 落盘之后它自己上位）。
/// `between`：写满之后、上位之前（判据插竞争用；生产传空）。
pub(crate) fn replace_contents(
    face: &Face,
    root: &Path,
    rel: &Path,
    content: Content<'_>,
    fresh: Option<Perms>,
    was: Was,
    between: &mut dyn FnMut(),
) -> Result<(PathBuf, u64), WriteRefusal> {
    let unread = |at: &Path, e: std::io::Error| {
        let at = at.display().to_string();
        let said = copy_text(
            "reason.fs.readFailed",
            &[("path", &at), ("why", &io_reason(e.kind()))],
        );
        WriteRefusal::io(said, &e)
    };
    let unwritten = |at: &Path, e: std::io::Error| {
        let at = at.display().to_string();
        let said = copy_text(
            "reason.fs.writeFailed",
            &[("path", &at), ("why", &io_reason(e.kind()))],
        );
        WriteRefusal::io(said, &e)
    };
    let at = walk_in_root(Last::Link, root, rel).map_err(WriteRefusal::Refused)?;
    let exists = match Ro::symlink_metadata(&at) {
        Ok(_) => true,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => false,
        Err(e) => return Err(unread(&at, e)),
    };
    match (was, exists) {
        (Was::New, true) => {
            return Err(WriteRefusal::Exists(copy_text(
                "beFilesWrite.swap.exists",
                &[("path", &at.display().to_string())],
            )))
        }
        (Was::Old, false) => {
            return Err(WriteRefusal::Stale(copy_text(
                "beFilesWrite.overwrite.gone",
                &[("path", &at.display().to_string())],
            )))
        }
        _ => {}
    }
    let (dst, perms, in_place) = if exists {
        let real = walk_in_root(Last::Follow, root, rel).map_err(WriteRefusal::Refused)?;
        let md = Ro::metadata(&real).map_err(|e| unread(&real, e))?;
        if !md.is_file() {
            return Err(WriteRefusal::Refused(copy_text(
                "beFilesWrite.overwrite.notRegular",
                &[("path", &real.display().to_string())],
            )));
        }
        let why = links_and_owner(&real).and_then(|(links, owner)| {
            in_place_reason(links, owner, crate::platform::paths::current_uid())
        });
        if cfg!(windows) {
            tracing::warn!(
                "换内容 {}：Windows 上换名会换掉 ACL —— 就地写",
                real.display()
            );
        } else if let Some(why) = &why {
            tracing::warn!(
                "换内容 {}：这一次不是原子写，因为{why} —— 改成就地写",
                real.display()
            );
        }
        let in_place = cfg!(windows) || why.is_some();
        (real, Some(md.permissions()), in_place)
    } else {
        let perms = fresh.or_else(|| own_mode(&at, false));
        (at, perms, false)
    };
    let staged_gone = |staged: &Resolved| {
        // 内容已经落进目标：暂存件删不掉不算失败（孤儿扫按期限收）。
        face.delete_file(staged).ok();
    };
    if in_place {
        let mut f = face.overwrite(&dst).map_err(|e| unwritten(&dst, e))?;
        let n = pour(&content, &mut f)
            .and_then(|n| f.sync_all().map(|()| n))
            .map_err(|e| unwritten(&dst, e))?;
        if let Content::Staged(staged) = content {
            staged_gone(staged);
        }
        return Ok((dst.into_path_buf(), n));
    }
    if let Content::Staged(staged) = &content {
        let n = Ro::symlink_metadata(staged)
            .map_err(|e| unread(staged, e))?
            .len();
        let ready = perms
            .clone()
            .map_or(Ok(()), |p| face.set_permissions(staged, p))
            .and_then(|()| face.sync(staged));
        if let Err(e) = ready {
            return Err(unwritten(staged, e));
        }
        between();
        match place(face, staged, &dst, was == Was::New) {
            Ok(()) => {
                face.sync_dir(&dst).ok();
                return Ok((dst.into_path_buf(), n));
            }
            Err(e) if e.kind() != std::io::ErrorKind::CrossesDevices => {
                return Err(placed_err(&dst, e))
            }
            Err(_) => {}
        }
    }
    let name = dst.file_name().ok_or_else(|| {
        WriteRefusal::Refused(copy_text(
            "beFilesWrite.path.noName",
            &[("path", &rel.display().to_string())],
        ))
    })?;
    let seq = SWAP_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    // 名字可以不是 UTF-8 ⇒ 旁名按 `OsString` 拼。
    let mut side_name = std::ffi::OsString::from(".");
    side_name.push(name);
    side_name.push(format!(".ccm-swap-{}-{seq}.part", std::process::id()));
    let side = dst.sibling(&side_name).ok_or_else(|| {
        WriteRefusal::Refused(copy_text(
            "beFilesWrite.path.noName",
            &[("path", &rel.display().to_string())],
        ))
    })?;
    let mut f = face.create_new(&side).map_err(|e| unwritten(&side, e))?;
    // 权限位在写进内容之前就定：旁名上也不留一刻宽的。
    let written = perms
        .map_or(Ok(()), |p| f.set_permissions(p))
        .and_then(|()| pour(&content, &mut f))
        .and_then(|n| f.sync_all().map(|()| n));
    drop(f);
    let n = match written {
        Ok(n) => n,
        Err(e) => {
            face.delete_file(&side).ok();
            return Err(unwritten(&side, e));
        }
    };
    between();
    if let Err(e) = place(face, &side, &dst, was == Was::New) {
        face.delete_file(&side).ok();
        return Err(placed_err(&dst, e));
    }
    face.sync_dir(&dst).ok();
    if let Content::Staged(staged) = content {
        staged_gone(staged);
    }
    Ok((dst.into_path_buf(), n))
}

/// 把内容倒进一份已开的文件，回字节数。
fn pour(content: &Content<'_>, f: &mut crate::platform::fs::FaceFile) -> std::io::Result<u64> {
    use std::io::Write as _;
    match content {
        Content::Bytes(b) => f.write_all(b).map(|()| b.len() as u64),
        Content::File(p) => std::io::copy(&mut Ro::open_nofollow(p)?, f),
        Content::Staged(s) => std::io::copy(&mut Ro::open_nofollow(s)?, f),
    }
}

/// 上位：`new` ⇒ 不覆盖改名（盘不认那个旗 ⇒ 普通文件走 `link` ＋ `unlink`），那一格已有东西 ⇒ `AlreadyExists`；否则换名顶掉。
fn place(face: &Face, from: &Resolved, to: &Resolved, new: bool) -> std::io::Result<()> {
    if !new {
        return face.move_file(from, to);
    }
    match face.move_noreplace(from, to) {
        Err(e) if crate::platform::fs::noreplace_unsupported(&e) => face.move_by_link(from, to),
        other => other,
    }
}

fn placed_err(dst: &Path, e: std::io::Error) -> WriteRefusal {
    if e.kind() == std::io::ErrorKind::AlreadyExists {
        return WriteRefusal::Exists(copy_text(
            "beFilesWrite.swap.exists",
            &[("path", &dst.display().to_string())],
        ));
    }
    WriteRefusal::io(
        copy_text(
            "reason.fs.writeFailed",
            &[
                ("path", &dst.display().to_string()),
                ("why", &io_reason(e.kind())),
            ],
        ),
        &e,
    )
}

/// 另存一份原文：`<名>.ccm-backup-<毫秒>-<序号>`，落在 `rel` 旁边（`O_EXCL`，沿用原权限位）。
fn land_backup(
    face: &Face,
    root: &Path,
    rel: &Path,
    original: &[u8],
    perms: Option<Perms>,
) -> Result<PathBuf, WriteRefusal> {
    use std::io::Write as _;
    let name = rel.file_name().ok_or_else(|| {
        WriteRefusal::Refused(copy_text(
            "beFilesWrite.path.noName",
            &[("path", &rel.display().to_string())],
        ))
    })?;
    let ms = crate::common::time::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let seq = SWAP_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let mut bak_name = name.to_os_string();
    bak_name.push(format!(".ccm-backup-{ms}-{seq}"));
    let bak_rel = rel.with_file_name(bak_name);
    let bak = walk_in_root(Last::Link, root, &bak_rel).map_err(WriteRefusal::Refused)?;
    let mut f = face.create_new(&bak).map_err(|e| {
        WriteRefusal::io(
            copy_text(
                "beFilesWrite.backup.createFailed",
                &[
                    ("path", &bak.display().to_string()),
                    ("why", &io_reason(e.kind())),
                ],
            ),
            &e,
        )
    })?;
    if let Err(e) = f.write_all(original) {
        drop(f);
        face.delete_file(&bak).ok();
        return Err(WriteRefusal::io(
            copy_text(
                "beFilesWrite.backup.createFailed",
                &[
                    ("path", &bak.display().to_string()),
                    ("why", &io_reason(e.kind())),
                ],
            ),
            &e,
        ));
    }
    if let Some(p) = perms {
        // 按句柄改（没有路径可被换成链接）；删那一下仍按路径（删的是链接本身，不跟）。
        keep_mode(
            &bak,
            p,
            |_, p| f.set_permissions(p),
            |_| face.delete_file(&bak),
        )?;
    }
    drop(f);
    Ok(bak.into_path_buf())
}

/// 〔E 吞错普查点名〕备份**沿用原文件的权限位**；沿用不上 ⇒ 删掉这份备份、整趟拒（此刻原文件还没动）。
///
/// 原先一个 `.ok()` 吞掉：一份 0600 的原文件（钥匙、令牌一类）可能留下一份按 umask 建出来的 0644 备份，
/// 而一句话都没有。两个动作都由调用方交进来（生产 = `set_permissions` / `remove_file`，写在 `land_backup` 里 ——
/// 那里先过了路径解析，交给 `Face` 的是 `Resolved`）；判据交一个会失败的 chmod。
fn keep_mode(
    bak: &Path,
    perms: Perms,
    chmod: impl FnOnce(&Path, Perms) -> std::io::Result<()>,
    remove: impl FnOnce(&Path) -> std::io::Result<()>,
) -> Result<(), WriteRefusal> {
    let Err(e) = chmod(bak, perms) else {
        return Ok(());
    };
    // 两句系统原话（沿用不上 · 删不掉）都进复制详情，各占一行。
    let (gone, raw) = match remove(bak) {
        Ok(()) => (copy_text("beFilesWrite.backup.removed", &[]), e.to_string()),
        Err(re) => (
            copy_text(
                "beFilesWrite.backup.notRemoved",
                &[("why", &io_reason(re.kind()))],
            ),
            format!("{e}\n{re}"),
        ),
    };
    Err(WriteRefusal::IoSaid {
        said: copy_text(
            "beFilesWrite.backup.modeFailed",
            &[
                ("path", &bak.display().to_string()),
                ("why", &io_reason(e.kind())),
                ("gone", &gone),
            ],
        ),
        raw,
    })
}

/// 回滚那一支：删掉**这一趟自己刚建出来**的那一份（只在「原来不存在」时调）。
fn remove_created(face: &Face, root: &Path, rel: &Path) -> Result<(), WriteRefusal> {
    let at = walk_in_root(Last::Link, root, rel).map_err(WriteRefusal::Refused)?;
    face.delete_file(&at).map_err(|e| {
        WriteRefusal::io(
            copy_text(
                "beFilesWrite.delete.failed",
                &[
                    ("path", &at.display().to_string()),
                    ("why", &io_reason(e.kind())),
                ],
            ),
            &e,
        )
    })
}

/// 🔴 **删一份历史会话 —— 只收 sid。**
///
/// 落点由适配层按 sid 在本机记录树里找（门递进来的 [`SessionPort::locate`]：找 → 解到底 → 恰是
/// `<项目>/<sid>.jsonl` → 必须是会话文件的形状）。这一条是「用户在历史浏览器里明确点了删」那一件事。
/// 从前这里写「别的写一律不许碰会话文件，这一条是唯一的例外」—— 文件管理面今天什么都能改，
/// 这一条的独特之处只剩「只收 sid、只删恰是那一形的那一份」。
pub fn delete_session(
    face: &Face,
    sid: &str,
    sessions: &SessionPort,
) -> Result<PathBuf, WriteRefusal> {
    delete_session_with(
        face,
        sid,
        sessions.locate,
        sessions.is_record,
        sessions.file_name,
    )
}

/// 删会话那一条要问的两件事（落点 · 形状）是 agent 记录布局的知识，
/// 本模块一家都不认 ⇒ 由门（命令注册那一处）从原生那一侧的窄口（`agents::locate_session_for_delete` ·
/// `agents::is_session_record`）取来递进 [`delete_session_of`]。
#[derive(Clone, Copy)]
pub struct SessionPort {
    /// sid ⇒ 要删的那一份。
    pub locate: fn(&str) -> Result<PathBuf, String>,
    /// 这一份是不是会话记录。
    pub is_record: fn(&Path) -> bool,
    /// 这一份记录归的那一家给 sid 起的文件名（没有 ⇒ 那一家不按 sid 命名会话文件）。
    pub file_name: fn(&Path, &str) -> Option<String>,
}

/// [`delete_session`] 的本体。`locate` / `is_record` 由调用方给（生产侧 = 门递进来的窄口；判据拿临时目录当 home），
/// 删之前**必须**先过 [`fenced_session_file`]。
pub fn delete_session_with(
    face: &Face,
    sid: &str,
    locate: impl FnOnce(&str) -> Result<PathBuf, String>,
    is_record: impl FnOnce(&Path) -> bool,
    file_name: impl FnOnce(&Path, &str) -> Option<String>,
) -> Result<PathBuf, WriteRefusal> {
    let target =
        fenced_session_file(sid, locate, is_record, file_name).map_err(WriteRefusal::Refused)?;
    face.delete_file(&target).map_err(|e| {
        WriteRefusal::io(
            copy_text(
                "beFilesWrite.session.deleteFailed",
                &[
                    ("path", &target.display().to_string()),
                    ("why", &io_reason(e.kind())),
                ],
            ),
            &e,
        )
    })?;
    Ok(target.into_path_buf())
}

/// 删会话那一条**自己的**围栏：落点只能是 `locate(sid)` 找到的那一份，而且**它必须是**
/// 会话文件的形状、文件名恰是那一家给这个 sid 起的名字（Claude：`<sid>.jsonl`；`locate` 换成什么都骗不过这两问）。
fn fenced_session_file(
    sid: &str,
    locate: impl FnOnce(&str) -> Result<PathBuf, String>,
    is_record: impl FnOnce(&Path) -> bool,
    file_name: impl FnOnce(&Path, &str) -> Option<String>,
) -> Result<Resolved, String> {
    let p = locate(sid)?;
    let want = file_name(&p, sid).unwrap_or_default();
    if want.is_empty() || p.file_name() != Some(std::ffi::OsStr::new(&want)) {
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
    // 这一份就是会话那一条的落点：`locate` 按 sid 找到、名字与形状都对上了（删会话不过根内路径解析，过的是这道围栏）。
    Resolved::vouch(p.clone()).map_err(|e| {
        copy_text(
            "beFilesWrite.path.unresolved",
            &[
                ("path", &p.display().to_string()),
                ("why", &io_reason(e.kind())),
            ],
        )
    })
}

// ══════════════════════════════════════════════════════════════════════════
// 命令面
// ══════════════════════════════════════════════════════════════════════════
//
// 🔴 **这一步不花用户那句「允许」**，`§8.6` 第 2 步逐字：
//   「它的射程：`O_EXCL` 新建一份**此前不存在**的文件；不删、不改名、不覆盖、不建目录
//    🔴 **`readonly_guard` 一行不动** —— 它那时早就在写盘白名单上（09-19 那一版；今天写经 `Face`）
//    ⇒ 这一步只是把一个做好了没接的东西接上」。
//   本节兑现的就是那一句：上面那些函数**一个字节没改射程**，
//   下面加的全是「把参数收进来、把答案交出去」。
//
// 🔴 **「窄」不是靠命令面窄 —— 这一条是现打改过来的，原来那句话是假的。**
//   本节第一版写的是「只上帧面、不上 CLI 面」。**做不到**：
//   `stream::cli::cli_exposed` 逐字是 `!spec.run.on_frame_arm()` ——
//   一条命令进了 `inbound::REGISTRY` 且不是那个硬臂，CLI 面就**自动**认得它
//  （现打：加完之后 `every_cli_exposed_command_is_in_the_query_mode_gate` 当场红，
//   逐字点名 `["--files-create"]`）。⇒ 两个宿主共用同一个 `run`，
//   那正是说的「一份代码两种宿主」。
//   ⇒ **窄在别的地方**：写要 `Face`，而 `Face` 只递给命令表文件管理那一族（typed 的 `Facing` 那一档），
//   不是「谁发得出命令」。别把两者混起来读。
//
// ⚠ 本节**买不到**什么：接上的是「命令面够得到这份原语」这一跳。
//   「真远端那台机器上跑过」**没有**（同本模块头注第 3 条），
//   本机跑得到的是本机文件系统上的那一趟。

/// 写面一条命令没成：码 ＋ 给人看的那一句 ＋ 下层原话（可缺；不上句子，进应答的复制详情）。
/// 参数那几道（`(码, 话)`）与盘上那几步（[`WriteRefusal`]）都经 `?` 换成它。
#[derive(Debug, Clone, PartialEq)]
pub struct WriteFail {
    pub code: &'static str,
    pub said: String,
    pub raw: Option<String>,
}

impl From<(&'static str, String)> for WriteFail {
    fn from((code, said): (&'static str, String)) -> Self {
        WriteFail {
            code,
            said,
            raw: None,
        }
    }
}

impl WriteFail {
    /// 盘上这一步没成（`io_failed`）：句子已带原因词，系统原话进复制详情。
    pub fn io(said: String, e: &std::io::Error) -> WriteFail {
        WriteFail {
            code: "io_failed",
            said,
            raw: Some(e.to_string()),
        }
    }
}

impl From<WriteRefusal> for WriteFail {
    /// 码由那个枚举自己答（理由住 [`WriteRefusal`]）。
    fn from(e: WriteRefusal) -> Self {
        WriteFail {
            code: e.code(),
            said: e.message().to_string(),
            raw: e.raw().map(str::to_string),
        }
    }
}

/// 文件管理**写**面的一条线上命令：只当「线上契约的数据形态」用（判据按它对拍 `inbound::REGISTRY` 与 `IPC-PROTOCOL.md §10`）。
pub struct ManageCommand {
    /// 线上命令名（连字符那一套，与 `inbound::REGISTRY` 逐字相同）。
    pub name: &'static str,
    /// 它做什么（登记散文，只给判据读、不上界面 ⇒ 不叫 `what`：那个字段名会被普查当成文案出口）。
    pub purpose: &'static str,
}

/// 🔴 **文件管理写面的唯一住址。**
///
/// `readonly_guard` 那条「拿得到 `Face` 的命令 == 写面登记的命令」判的就是这张表（与上传提交 · 解压两张的并）——
/// 表里没有的名字，`inbound::REGISTRY` 上也不许有对应的一条。
pub const MANAGE_COMMANDS: &[ManageCommand] = &[
    ManageCommand {
        name: "files-create",
        purpose: "在用户指定的文件管理目标根底下，新建一份**此前不存在**的文件（`O_EXCL`）",
    },
    // ── **改动既有数据**的那五件 ──────────
    ManageCommand {
        name: "files-mkdir",
        purpose: "新建一个目录（只建最后那一段；父目录不在就失败，不顺手补）",
    },
    ManageCommand {
        name: "files-rename",
        purpose: "改名 / 同根内移动；**两个参数各过一遍路径解析**，目标已存在就拒（不覆盖）",
    },
    ManageCommand {
        name: "files-delete",
        purpose: "删一组（`control/files_batch.rs`）：每项一个文件或一个**空**目录（删的是链接本身，不跟过去）；显式 `recursive: true` \
               才删整棵树 —— 逐条目过路径解析，任一条被拒那一项不动（`delete_tree`）、按段删，段与段之间看撤的旗；给了 `expect` \
               ⇒ 只删一份普通文件、且盘上逐字节等于它才删（否则那一项 `stale`，一个字节不动）；`expect: {\"empty_dir\": true}` \
               ⇒ 只删一个空目录；逐项结局 ＋ 结局那一行；带票 ⇒ 断开照常做完、再问回同一份",
    },
    ManageCommand {
        name: "files-chmod",
        purpose: "一组改成同一个 unix 权限位（低 12 位，`control/files_batch.rs`）；**跟链接**，所以落点解到底再判一次；\
没有 unix 权限位的平台上那一项回 `no_unix_mode`；每项带改之前的那一份（撤销用）",
    },
    // ── 窗口的「复制」换走通道 ──────────────────
    ManageCommand {
        name: "files-copy",
        purpose: "同根内复制一组（`control/files_batch.rs`）：先探整组落点，同名而没答 ⇒ 一项不做、回 `exists` 与同名清单；\
               答法 `overwrite` · `skip` · `keep_both`（另起 `名 (2).ext`，撞上往后试）；目录整棵复制、同名不合并；\
               **三条路径各过一遍路径解析**；不覆盖的那几项 `O_EXCL`，探完之后冒出来的同名不盖（那一项 `exists`）；\
               覆盖经暂存旁名原子顶掉；权限位从源抄；树里的链接复制**链接本身**",
    },
    ManageCommand {
        name: "files-write-text",
        purpose: "覆盖写一份**已经在**的普通文件；**跟链接**，所以落点解到底再判一次；\
`expect: {sha256}` **必给**：盘上那份的摘要对得上才写（否则 `stale`，一个字节不动），应答交新摘要",
    },
    // ── 删历史会话 ──
    ManageCommand {
        name: "files-delete-session",
        purpose: "删一份历史会话 —— **只收 sid**，落点由后端按 sid 在记录树里找（历史浏览器的删会话，不是文件管理器）",
    },
];
/// 一条写命令的入参类型就选中了那条命令的本体（同读那一族的 `files::Ask`）：命令那一层只经 [`answer`] 这一扇门进来。
pub trait Act {
    type Reply;
    fn act(self, face: &Face) -> Result<Self::Reply, WriteFail>;
}

/// 文件管理写的那几条（写面 · 上传提交 · 解压）的**入口**：写经分派递进来的 `face`。
/// 删会话那一条另走 [`delete_session_of`]（它还要门递进来的那两问）。
pub fn answer<A: Act>(req: A, face: &Face) -> Result<A::Reply, WriteFail> {
    req.act(face)
}

macro_rules! acts {
    ($($req:ident => $reply:ident, |$r:ident, $f:ident| $body:expr;)*) => {
        $(impl Act for wire::$req {
            type Reply = wire::$reply;
            #[allow(unused_variables)]
            fn act(self, $f: &Face) -> Result<wire::$reply, WriteFail> {
                let $r = self;
                $body
            }
        })*
    };
}
pub(crate) use acts;

acts! {
    FilesCreateReq => FilesPathBytesReply, |r, f| create(f, r);
    FilesMkdirReq => FilesPathReply, |r, f| mkdir(f, r);
    FilesRenameReq => FilesPathReply, |r, f| rename(f, r);
    FilesWriteTextReq => FilesWrittenReply, |r, f| write_text(f, r);
    FilesPeekReq => FilesPeekReply, |r, f| peek(r);
    FilesPutReq => FilesPutReply, |r, f| put(f, r);
}

/// 目标根（或别的绝对路径那一格）⇒ `PathBuf`。空的 ⇒ `bad_path`（缺 · 形状不对在解入参那一步就回了 `bad_args`）。
pub fn root_of(p: &wire::WirePath, key: &str) -> Result<PathBuf, WriteFail> {
    if p.0.is_empty() {
        return Err(WriteFail::from((
            "bad_path",
            crate::common::contract::malformed(&format!("`{key}` is empty")),
        )));
    }
    Ok(crate::files::raw::to_path_buf(&p.0))
}

/// 相对段 ⇒ `PathBuf`。⚠ **空串不在这里拒**：交给路径解析①（它拒空段、码是 `refused`）。
pub fn rel_of(p: &wire::WirePath) -> PathBuf {
    crate::files::raw::to_path_buf(&p.0)
}

/// 一个落点交回线上（原始字节形）。
pub fn wire_of(p: &Path) -> wire::WirePath {
    wire::WirePath(crate::files::raw::path_bytes(p).to_vec())
}

/// `single`（缺 ⇒ `false`）：给了 ⇒ 这一格只许是一段名字（界面「新建 / 改名」那一格敲的就是一个名字）。
/// 每一段都过这台的名字规则（`platform::paths::name_problem`）。不合 ⇒ 码 [`BAD_NAME`]。
pub(crate) fn names_ok(single: bool, rel: &Path) -> Result<(), WriteFail> {
    let shown = rel.to_string_lossy();
    let parts: Vec<Component> = rel.components().collect();
    if single && parts.is_empty() {
        return Err(WriteFail::from((
            BAD_NAME,
            copy_text("beFilesWrite.name.empty", &[]),
        )));
    }
    if single && !matches!(parts.as_slice(), [Component::Normal(_)]) {
        return Err(WriteFail::from((
            BAD_NAME,
            copy_text("beFilesWrite.name.notOne", &[("name", &shown)]),
        )));
    }
    for part in parts {
        let Component::Normal(seg) = part else {
            continue;
        };
        let name = seg.to_string_lossy();
        let said = match crate::platform::paths::name_problem(seg) {
            None => continue,
            Some(crate::platform::paths::NameProblem::Reserved) => {
                copy_text("beFilesWrite.name.reserved", &[("name", &name)])
            }
            Some(crate::platform::paths::NameProblem::BadChar(c)) => copy_text(
                "beFilesWrite.name.badChar",
                &[("name", &name), ("char", &c.escape_debug().to_string())],
            ),
            Some(crate::platform::paths::NameProblem::Trailing) => {
                copy_text("beFilesWrite.name.trailing", &[("name", &name)])
            }
        };
        return Err(WriteFail::from((BAD_NAME, said)));
    }
    Ok(())
}

/// 一次拒绝交回线上：码由那个枚举自己答（理由住 [`WriteRefusal`]）。
fn refusal(e: WriteRefusal) -> WriteFail {
    WriteFail::from(e)
}

fn n(x: usize) -> u64 {
    x as u64
}

/// `files-create` —— 把两道路径解析与那一处 `O_EXCL` 落盘接到线上。不给 `content` ⇒ 新建一份**空文件**。
fn create(face: &Face, req: wire::FilesCreateReq) -> Result<wire::FilesPathBytesReply, WriteFail> {
    let root = root_of(&req.root, "root")?;
    let rel = rel_of(&req.rel);
    names_ok(req.single.unwrap_or(false), &rel)?;
    let bytes = req.content.map(|c| c.0).unwrap_or_default();
    // 🔴 码由那个枚举自己答，**不在这里猜字符串前缀** —— 理由住 [`WriteRefusal`]。
    let landed = create_new_file(face, &root, &rel, &bytes).map_err(refusal)?;
    Ok(wire::FilesPathBytesReply {
        path: wire_of(&landed),
        bytes: n(bytes.len()),
    })
}

/// `files-mkdir`。
fn mkdir(face: &Face, req: wire::FilesMkdirReq) -> Result<wire::FilesPathReply, WriteFail> {
    let root = root_of(&req.root, "root")?;
    let rel = rel_of(&req.rel);
    names_ok(req.single.unwrap_or(false), &rel)?;
    let done = make_dir(face, &root, &rel).map_err(refusal)?;
    Ok(wire::FilesPathReply {
        path: wire_of(&done),
    })
}

/// `files-rename`。
fn rename(face: &Face, req: wire::FilesRenameReq) -> Result<wire::FilesPathReply, WriteFail> {
    let root = root_of(&req.root, "root")?;
    let (from, to) = (rel_of(&req.from), rel_of(&req.to));
    names_ok(req.single.unwrap_or(false), &to)?;
    let done = rename_entry(face, &root, &from, &to).map_err(refusal)?;
    Ok(wire::FilesPathReply {
        path: wire_of(&done),
    })
}

/// `files-write-text`：`content` · `expect`（摘要形 CAS）都**必给**：没有「不问就盖」这一形（同 `Door::put`）。
fn write_text(
    face: &Face,
    req: wire::FilesWriteTextReq,
) -> Result<wire::FilesWrittenReply, WriteFail> {
    let root = root_of(&req.root, "root")?;
    let rel = rel_of(&req.rel);
    let bytes = req.content.0;
    let done = overwrite_text_expecting(face, &root, &rel, &bytes, &req.expect.sha256.0)
        .map_err(refusal)?;
    Ok(wire::FilesWrittenReply {
        path: wire_of(&done),
        bytes: n(bytes.len()),
        sha256: wire::Sha256Hex(content_sha256(&bytes)),
    })
}

/// `Door::peek`：读改写的读那一半（不写，不要句柄）。
fn peek(req: wire::FilesPeekReq) -> Result<wire::FilesPeekReply, WriteFail> {
    let root = root_of(&req.root, "root")?;
    let rel = rel_of(&req.rel);
    let got = peek_text(&root, &rel)?;
    Ok(wire::FilesPeekReply {
        path: wire_of(&got.path),
        exists: got.text.is_some(),
        text: got.text,
    })
}

/// `Door::put`：🔴 `expect` **必给**：`null` = 「我读的时候它不在」；字符串 / b16 = 「我读到的就是这一份」（缺席在解入参那一步就拒了）。
fn put(face: &Face, req: wire::FilesPutReq) -> Result<wire::FilesPutReply, WriteFail> {
    let root = root_of(&req.root, "root")?;
    let rel = rel_of(&req.rel);
    let expect = req.expect.map(|e| e.0);
    let done = put_text(
        face,
        &root,
        &rel,
        &req.content.0,
        expect.as_deref(),
        req.backup.unwrap_or(false),
        req.parents.unwrap_or(false),
    )
    .map_err(refusal)?;
    Ok(wire::FilesPutReply {
        path: wire_of(&done.path),
        bytes: n(done.bytes),
        changed: done.changed,
        created: done.created,
        backup: done.backup.as_deref().map(wire_of),
    })
}

/// `files-delete-session`：🔴 **只收 sid**（多给任何一个键在解入参那一步就是 `bad_args`）——
/// 这一条只按 sid 找那一份、从不收路径，「顺手也收一个路径」那一形连表达的机会都不给。
pub fn delete_session_of(
    face: &Face,
    req: wire::FilesDeleteSessionReq,
    sessions: &SessionPort,
) -> Result<wire::FilesPathReply, WriteFail> {
    let done = delete_session(face, &req.sid, sessions).map_err(refusal)?;
    Ok(wire::FilesPathReply {
        path: wire_of(&done),
    })
}

#[cfg(test)]
#[path = "../../../tests/backend/control/files_write_tests.rs"]
pub(crate) mod tests;

// 覆盖写原子化的判据（写到一半被收掉 · 权限位 / 链接 / 旁名 · 生产段 `fs::write(` 只剩 Windows 臂）。
#[cfg(test)]
#[path = "../../../tests/backend/control/overwrite_atomic_tests.rs"]
mod overwrite_atomic_tests;

// TOCTOU 那几条闭合的竞争判据。
#[cfg(test)]
#[path = "../../../tests/backend/control/files_toctou_tests.rs"]
mod toctou_tests;
