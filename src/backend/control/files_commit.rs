//! 〔F7c · 第三波 · 2026-09-24〕**上传的提交** —— 把暂存区里一份传完的件挪进用户指定的目标。
//!
//! # 为什么有这一份（用户逐字两句）
//!
//! 「**保留SFTP. 思考怎么干净**」＋「**现在只允许后端的文件管理部分写文件**」。
//! ⇒ `设计/60 §13`：SFTP 缩成只做传输，而上传**不直接写到目标** —— SFTP 只写进我们自己的暂存区
//! （`~/.cc-monitor/staging/<key>.part`，断点续传的本钱也住在那儿），传完由**这里**提交。
//! ⇒ 真正落进用户目录的那一下，只有后端文件管理这一处。
//!
//! # 它是文件管理那一面的成员，住 `control/` 是**层**的归属
//!
//! 同 `control/files_write.rs` 那条理由：它会改变世界 ⇒ 住「会改变世界」那一层；
//! 它属于文件管理后端这个模块（`tests/backend/files/module_boundary_guard.rs` 的成员名单）。
//! ⚠ 刻意**不住 `files/`**：那一族的头注与 `设计/96 §2.9` 边界①逐字「整族纯读」——
//! 往那棵树里放一个会改名的模块，那句话就当场变假。
//!
//! # 它在 `readonly_guard` 第三层上（第二个登记的模块）
//!
//! 第三层的四条对本模块逐条成立：改动动词只用闭集里那几个 · 表外改动照旧禁 ·
//! **每个会改动的函数先过路径解析**（路径解析本体借 `files_write` 那一份，一个字节不抄）·
//! 后端生产树里引用得到本模块的**只有** `inbound.rs` 那一条命令。
//!
//! # 提交的两支（`overwrite` 由调用方**显式**给，不给默认值）
//!
//! | `overwrite` | 做法 | 为什么 |
//! |---|---|---|
//! | `false` | 先在目标上用 `O_EXCL` **占一个位**（已在 ⇒ 拒），再把暂存件改名上位、盖掉那个占位 | 「目标已经在了就拒」要**原子地**判：先看一眼再改名，中间有一个窗（`files_write::rename_entry` 头注自陈的那一格）；占位把那个窗关上了 —— 占位成功的那一刻，目标**确实**此前不存在 |
//! | `true` | 暂存件直接改名上位 | 同一个盘上的改名是原子的：读的人要么看见旧的一整份，要么看见新的一整份 |
//!
//! ⚠ 暂存件的路径**由本模块自己拼**（`$HOME` ＋ 那个固定的相对段 ＋ `key`），`key` 只收
//! [`KEY_LEN`] 位小写十六进制 ⇒ 调用方**指不到**暂存区之外的任何一个文件当「源」。
//!
//! # 买不到什么（逐条）
//!
//! - **跨盘的提交做不到**：暂存区与目标不在同一个文件系统上 ⇒ 改名回 `EXDEV`，原样带回。
//!   要做就得「复制 ＋ 删」，而复制在第三层禁表里 —— 新形状，要单独论证。
//! - **「SFTP 的起始目录 == 这里的 `$HOME`」是前提**：`ChrootDirectory` / `internal-sftp -d`
//!   那种机器上两边拼出来的暂存件不是同一份 ⇒ 本命令答「暂存件不在」，出声但做不成。
//! - **占位与改名之间仍有一个窗**：别的进程在那一瞬往占位里写了东西，改名会把它盖掉。
//!   那个窗里的东西是「一份刚出现、0 字节起步的文件」，不是用户既有数据 —— 如实登记。
//! - **Windows 远端**：改名覆盖的语义不同（目标在就失败），`overwrite: true` 那一支在那边会拒。没量过。

//!
//! # 〔F9c · 第四波 · 2026-09-24〕第二种件：**存盘的块**（`files-stage-chunk` ＋ `files-commit-text`）
//!
//! 文件窗口存一份装不进一条请求行的文本（后端入方向一行 1 MiB，`inbound.rs::MAX_LINE_BYTES`）时，
//! 把它切成几块逐块送进暂存区（`<key>.<seq>.chunk`，**`O_EXCL` 新建**：一块只写一次），
//! 再由 `files-commit-text` 按块号读回、拼起来、核总长，交给写面那一份原地覆盖
//! （`files_write::overwrite_text`，**一字不抄**）。设计全文住 `调研/第四波记录/F9c.md`。
//!
//! ⚠ 为什么不是「改名上位」（上面那种件的提交）：存盘是**改一份已经在的文件**，原地覆盖保留
//! 权限位 / 属主 / 硬链接、链接跟过去写、跨盘照常；改名上位三样全换掉（`EXDEV` 还会失败）。
//! 同一份文件 1 MiB 上下存出两种结果不行 ⇒ 提交这一下与 `files-write-text` 走**同一个原语**。
//! ⚠ 为什么一块一份文件、不往一份上续写：续写、截断、「在就打开不在就建」那几种开法都在第三层禁表上，
//! 而且每开一个写句柄都得配一个 `O_EXCL` ⇒ 「一块一份、`O_EXCL` 新建」**一个动词不加**。
//! （护栏是不剥注释的子串扫描，这里刻意不写那几种开法的字面名字 —— 同 `files_write` 头注那条。）
//! ⚠ 块数没有单独的上限：每块至少 1 字节（空块拒）⇒ 块数 ≤ 总长 ≤ `files::READ_TEXT_MAX_BYTES`
//! （存得回的就读得回来：窗口的编辑上限与它对拍相等）。
//! ⚠ 清理：提交**不论成败**都删掉这一次的块（窗口手上还有全文，重存换新键）；中途断掉留下的块
//! 由 [`sweep_stale`] 按同一个期限收（它认得两种形状）。

use crate::control::files_write::{
    content_sha256, overwrite_text_expecting, resolve_in_root, sha256_expect_of, Answer,
    ManageCommand, WriteRefusal,
};
use std::path::{Path, PathBuf};

/// 暂存区相对 `$HOME` 的那一段。
///
/// 〔SR1b · 2026-09-24〕往里写的那一侧（SFTP 传输台）**搬进了本 crate**（`dial/sftp.rs::STAGING_ROOT` ·
/// `control/transfer.rs`）⇒ 从前「桥那一侧一份逐字副本 ＋ 相等断言」那一对收成**同一个 crate 里的两个名字**，
/// 判据直接比（`dial_sftp_tests::the_declared_write_roots_are_exactly_staging_and_bin`）。
pub const STAGING_DIR: &str = ".cc-monitor/staging";

/// 暂存件的键长（十六进制位数）。造键的那一侧（`control/transfer.rs::staging_key`）同一个数（直接引用）。
pub const KEY_LEN: usize = 32;

/// 暂存件的后缀。
pub const PART_SUFFIX: &str = ".part";

/// 〔F9c〕存盘块的后缀（`<key>.<seq>.chunk`）。与 [`PART_SUFFIX`] 刻意不同名：两种件的消耗方式不同
/// （上传件改名上位、块读回后删），混一个名字，提交与孤儿扫就得去猜它是哪一种。
pub const CHUNK_SUFFIX: &str = ".chunk";

/// 〔F9c〕第 `seq` 块的文件名。
pub fn chunk_name(key: &str, seq: u64) -> String {
    format!("{key}.{seq}{CHUNK_SUFFIX}")
}

/// 〔F9c〕这个名字是不是一块（回 `(键, 块号)`）。块号只认**规范**十进制（`0` 或不以 `0` 开头），
/// 否则 `a.01.chunk` 与 `a.1.chunk` 会是同一块的两个名字。
pub fn parse_chunk_name(name: &str) -> Option<(&str, u64)> {
    let (key, seq) = name.strip_suffix(CHUNK_SUFFIX)?.split_once('.')?;
    let canonical = !seq.is_empty()
        && seq.bytes().all(|b| b.is_ascii_digit())
        && (seq == "0" || !seq.starts_with('0'));
    if !is_key(key) || !canonical {
        return None;
    }
    Some((key, seq.parse().ok()?))
}

/// 🔴 **本面的命令表**（形状借写面那一个类型，理由同它：线上契约的数据形态）。
pub const COMMIT_COMMANDS: &[ManageCommand] = &[
    ManageCommand {
        name: "files-commit-upload",
        what:
            "把暂存区里一份传完的上传件挪进用户指定的目标（先过路径解析；不覆盖时 `O_EXCL` 占位再改名上位）",
        args: &["key", "overwrite", "rel", "root"],
        fields: &["bytes", "path"],
        codes: &["bad_args", "bad_path", "io_failed", "refused"],
    },
    // ── 〔F9c · 第四波〕存盘装不进一行时的两步（头注「第二种件」一节）──────────────
    ManageCommand {
        name: "files-stage-chunk",
        what: "把存盘的一块写进暂存区 `<key>.<seq>.chunk`（`O_EXCL` 新建：同一块写第二次就拒；暂存区不在就建）",
        args: &["content", "key", "seq"],
        fields: &["bytes"],
        codes: &["bad_args", "io_failed", "refused"],
    },
    ManageCommand {
        name: "files-commit-text",
        what: "按块号读回 `0..chunks` 块、拼起来、总长必须等于 `bytes`，再原地覆盖目标（与 \
               `files-write-text` 同一个原语：跟链接、只收已在的普通文件）；〔FW1〕`expect: {sha256}` 必给，\
               盘上那份对不上 ⇒ `stale`、一个字节不写；不论成败都删掉这些块",
        args: &["bytes", "chunks", "expect", "key", "rel", "root"],
        fields: &["bytes", "path", "sha256"],
        codes: &["bad_args", "bad_path", "io_failed", "refused", "stale"],
    },
];

/// 本面命令名（给 `readonly_guard` 第三层那条「门里够得到的命令」对拍用）。
pub fn commit_command_names() -> Vec<&'static str> {
    COMMIT_COMMANDS.iter().map(|c| c.name).collect()
}

/// `key` 合不合法：恰好 [`KEY_LEN`] 位、只有 `0-9a-f`。
pub fn is_key(key: &str) -> bool {
    key.len() == KEY_LEN
        && key
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// 暂存件的绝对路径。`key` 不合法 ⇒ 拒（**不**猜、不清洗）。
pub fn staged_path(home: &Path, key: &str) -> Result<PathBuf, WriteRefusal> {
    if !is_key(key) {
        return Err(WriteRefusal::Refused(format!(
            "refuse write: 暂存件的键只收 {KEY_LEN} 位小写十六进制（给的是 {key:?}）"
        )));
    }
    Ok(home.join(STAGING_DIR).join(format!("{key}{PART_SUFFIX}")))
}

/// **提交**：暂存件 → 目标。成功回 `(落点, 字节数)`。
///
/// 第一件事是过路径解析（第三层 ③ 逐函数扫这个顺序）。
pub fn commit_upload(
    home: &Path,
    key: &str,
    root: &Path,
    rel: &str,
    overwrite: bool,
) -> Result<(PathBuf, u64), WriteRefusal> {
    let dest = resolve_in_root(root, rel).map_err(WriteRefusal::Refused)?;
    let staged = staged_path(home, key)?;
    // 不跟链接地看暂存件一眼：它必须是一份普通文件（一条链接当「源」＝ 挪走链接指向之外的东西，不许）。
    let meta = std::fs::symlink_metadata(&staged).map_err(|e| {
        WriteRefusal::Io(format!(
            "refuse write: 暂存件不在（{}：{e}）—— 传输没跑完，或者 SFTP 的起始目录不是这台后端的 home",
            staged.display()
        ))
    })?;
    if !meta.file_type().is_file() {
        return Err(WriteRefusal::Refused(format!(
            "refuse write: 暂存件不是一份普通文件（{}）",
            staged.display()
        )));
    }
    let bytes = meta.len();
    if overwrite {
        std::fs::rename(&staged, &dest).map_err(|e| {
            WriteRefusal::Io(format!(
                "refuse write: 暂存件改名上位到 {} 失败：{e}",
                dest.display()
            ))
        })?;
        return Ok((dest, bytes));
    }
    // 不覆盖：先占位（`O_EXCL`，目标已在 ⇒ 当场失败），再改名上位盖掉自己那个占位。
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&dest)
        .map_err(|e| {
            WriteRefusal::Io(format!(
                "refuse write: 目标已经在了、或者占不了位（{}：{e}）—— 不覆盖，先删掉它或换个名字",
                dest.display()
            ))
        })?;
    if let Err(e) = std::fs::rename(&staged, &dest) {
        // 撤掉自己那个 0 字节的占位（它是这一次刚建的，不是用户既有数据）。
        let _ = std::fs::remove_file(&dest);
        return Err(WriteRefusal::Io(format!(
            "refuse write: 暂存件改名上位到 {} 失败：{e}",
            dest.display()
        )));
    }
    Ok((dest, bytes))
}

// ═══════════════ 〔F9c · 第四波〕存盘的块：写一块 · 读回拼起来提交 · 删掉 ═══════════════

/// 暂存区在盘上的位置；不在就建（两层：`~/.cc-monitor` 与它底下的 `staging`，已在不算错）。
///
/// 每一层**先过路径解析**（以上一层为根）再建 —— 第三层 ③ 逐函数扫这个顺序。
/// ⚠ `~/.cc-monitor` 若是用户自己放的一条链接，跟过去（后端自己的家，与第四层 `exit_policy` 同一个家）。
fn ensure_staging(home: &Path) -> Result<PathBuf, WriteRefusal> {
    let mut at = home.to_path_buf();
    for seg in STAGING_DIR.split('/') {
        let next = resolve_in_root(&at, seg).map_err(WriteRefusal::Refused)?;
        if let Err(e) = std::fs::create_dir(&next) {
            if e.kind() != std::io::ErrorKind::AlreadyExists {
                return Err(WriteRefusal::Io(format!(
                    "refuse write: 建暂存区 {} 失败：{e}",
                    next.display()
                )));
            }
        }
        if !std::fs::metadata(&next).is_ok_and(|m| m.is_dir()) {
            return Err(WriteRefusal::Refused(format!(
                "refuse write: {} 在，但不是一个目录 —— 暂存区放不进去",
                next.display()
            )));
        }
        at = next;
    }
    Ok(at)
}

/// **写一块**：`<key>.<seq>.chunk`，`O_EXCL` 新建。回写进去的字节数。
///
/// 同一块已经在（重发 / 两次存盘撞了键）⇒ 拒，一个字节不盖。写到一半失败 ⇒ 删掉自己刚建的那一份。
pub fn stage_chunk(home: &Path, key: &str, seq: u64, bytes: &[u8]) -> Result<u64, WriteRefusal> {
    use std::io::Write as _;
    if !is_key(key) {
        return Err(WriteRefusal::Refused(format!(
            "refuse write: 暂存件的键只收 {KEY_LEN} 位小写十六进制（给的是 {key:?}）"
        )));
    }
    let dir = ensure_staging(home)?;
    let at = resolve_in_root(&dir, &chunk_name(key, seq)).map_err(WriteRefusal::Refused)?;
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&at)
        .map_err(|e| {
            WriteRefusal::Io(format!(
                "refuse write: 第 {seq} 块建不了（{}：{e}）—— 同一块只写一次，已经在就是重发了",
                at.display()
            ))
        })?;
    if let Err(e) = f.write_all(bytes) {
        drop(f);
        let _ = std::fs::remove_file(&at);
        return Err(WriteRefusal::Io(format!(
            "refuse write: 第 {seq} 块写到一半断了（{}：{e}）",
            at.display()
        )));
    }
    Ok(bytes.len() as u64)
}

/// 按块号读回 `0..chunks`，拼起来。总长必须**恰好**等于 `bytes`，多出第 `chunks` 块也拒。
/// **纯读**（不跟链接地看每一块一眼：它必须是一份普通文件）。
fn gather_chunks(home: &Path, key: &str, chunks: u64, bytes: u64) -> Result<Vec<u8>, WriteRefusal> {
    if !is_key(key) {
        return Err(WriteRefusal::Refused(format!(
            "refuse write: 暂存件的键只收 {KEY_LEN} 位小写十六进制（给的是 {key:?}）"
        )));
    }
    let dir = home.join(STAGING_DIR);
    let want = usize::try_from(bytes).unwrap_or(usize::MAX);
    let mut out: Vec<u8> = Vec::with_capacity(want.min(crate::files::READ_TEXT_MAX_BYTES));
    for seq in 0..chunks {
        let p = dir.join(chunk_name(key, seq));
        let meta = std::fs::symlink_metadata(&p).map_err(|e| {
            WriteRefusal::Io(format!(
                "refuse write: 第 {seq} 块不在（共 {chunks} 块；{}：{e}）—— 没送到，或者已经被提交过",
                p.display()
            ))
        })?;
        if !meta.file_type().is_file() {
            return Err(WriteRefusal::Refused(format!(
                "refuse write: 第 {seq} 块不是一份普通文件（{}）",
                p.display()
            )));
        }
        if out.len() as u64 + meta.len() > bytes {
            return Err(WriteRefusal::Io(format!(
                "refuse write: 读到第 {seq} 块就超过了说好的 {bytes} 字节 —— 块对不上，一个字节没写"
            )));
        }
        let got = std::fs::read(&p).map_err(|e| {
            WriteRefusal::Io(format!(
                "refuse write: 第 {seq} 块读不出来（{}：{e}）",
                p.display()
            ))
        })?;
        out.extend_from_slice(&got);
    }
    if out.len() as u64 != bytes {
        return Err(WriteRefusal::Io(format!(
            "refuse write: {chunks} 块拼回来 {} 字节，说好的是 {bytes} —— 块对不上，一个字节没写",
            out.len()
        )));
    }
    if std::fs::symlink_metadata(dir.join(chunk_name(key, chunks))).is_ok() {
        return Err(WriteRefusal::Io(format!(
            "refuse write: 说好 {chunks} 块，暂存区里还有第 {chunks} 块 —— 块对不上，一个字节没写"
        )));
    }
    Ok(out)
}

/// 删掉这个键的**全部**块（列暂存区、按名字认 —— 不按块号数：说错块数、中间缺一块时照样删干净）。
/// 尽力而为，删不掉不挡调用方（孤儿扫会收）。每一处删之前先过以暂存区为根的路径解析；链接不删
/// （那不是我们放的一块）。
fn drop_chunks(home: &Path, key: &str) {
    let dir = home.join(STAGING_DIR);
    let Ok(rd) = std::fs::read_dir(&dir) else {
        return;
    };
    for entry in rd.flatten() {
        let Some(name) = entry.file_name().to_str().map(str::to_string) else {
            continue;
        };
        if parse_chunk_name(&name).is_none_or(|(k, _)| k != key) {
            continue;
        }
        let Ok(at) = resolve_in_root(&dir, &name) else {
            continue;
        };
        if std::fs::symlink_metadata(&at).is_ok_and(|m| m.file_type().is_file()) {
            let _ = std::fs::remove_file(&at);
        }
    }
}

/// **提交存盘**：读回 `0..chunks` 块 ⇒ 原地覆盖 `root/rel`（写面那一份原语，路径解析在它里面）。
/// 回 `(落点, 字节数)`。**不论成败**都删掉这一次的块。
pub fn commit_text(
    home: &Path,
    key: &str,
    chunks: u64,
    bytes: u64,
    root: &Path,
    rel: &str,
    expect_sha256: &str,
) -> Result<(PathBuf, u64, String), WriteRefusal> {
    // 〔FW1〕与 `files-write-text` 同一道 CAS（`overwrite_text_expecting`）：两支存盘同一种结果，含「盘上被改过 ⇒ stale」。
    let r = gather_chunks(home, key, chunks, bytes).and_then(|body| {
        overwrite_text_expecting(root, rel, &body, expect_sha256)
            .map(|at| (at, body.len() as u64, content_sha256(&body)))
    });
    drop_chunks(home, key);
    r
}

/// 多久没动过的暂存件算**孤儿**（秒）。
///
/// 孤儿只有两种来路：上传失败之后再没重试的（失败**刻意留着**给续传）· 传完了窗口却没来得及提交的。
/// 一趟正在传的件每写一块修改时间就刷新一次，永远不会被判成孤儿。
/// ⚠ 7 天不是量出来的，是一个约定：一周没人回来续的件，那次续传已经不值得等了。
///
/// 🔴 **这个值住后端，不住传输那一侧**：它是一个期限的**值**（`设计/05 §3.3.2`「值归后端」），
/// 而且「修改时间」与「此刻」必须取**同一台机器的钟** —— 放在 monitor 那边比，
/// 就是拿 monitor 的钟去比远端的时间戳（两台机器差几个小时，判出来的「老」就差几个小时）。
pub const STAGING_STALE_SECS: u64 = 7 * 24 * 3600;

/// **孤儿扫**：暂存区里修改时间早于 `now - STAGING_STALE_SECS` 的暂存件删掉。回删掉的名字。
///
/// - 只认**我们自己的形状**：`<32 位十六进制>.part` 与〔F9c〕`<32 位十六进制>.<块号>.chunk`；
///   别的名字一个不碰（那不是我们放的）。
/// - `keep` 那一个不碰（调用方此刻手上的那一份）。
/// - 每一处删之前**先过路径解析**（以暂存区为根的 [`resolve_in_root`]）—— 第三层 ③ 逐函数扫这个顺序；
///   它在这里拦的是「暂存区里被人放了一条指出去的链接」那一形（解父目录之后跑出了暂存区 ⇒ 不删）。
/// - 尽力而为：列不出、删不掉都不挡调用方（孤儿多留一轮不伤人）。
///
/// ⚠ **只在事件上跑**（提交成功那一刻，见 [`answer_commit`]），不是节拍器 —— 后端零定时器。
pub fn sweep_stale(home: &Path, now_secs: u64, keep: &str) -> Vec<String> {
    let dir = home.join(STAGING_DIR);
    let Ok(rd) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut gone = Vec::new();
    for entry in rd.flatten() {
        let Some(name) = entry.file_name().to_str().map(str::to_string) else {
            continue;
        };
        // 两种形状：上传件 `<key>.part` · 〔F9c〕存盘块 `<key>.<seq>.chunk`。
        let Some(key) = name
            .strip_suffix(PART_SUFFIX)
            .or_else(|| parse_chunk_name(&name).map(|(k, _)| k))
        else {
            continue;
        };
        if !is_key(key) || key == keep {
            continue;
        }
        let Ok(at) = resolve_in_root(&dir, &name) else {
            continue;
        };
        let Ok(meta) = std::fs::symlink_metadata(&at) else {
            continue;
        };
        if !meta.file_type().is_file() {
            continue;
        }
        let Some(mtime) = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        else {
            continue;
        };
        if now_secs.saturating_sub(mtime.as_secs()) > STAGING_STALE_SECS
            && std::fs::remove_file(&at).is_ok()
        {
            gone.push(name);
        }
    }
    gone
}

/// 取一个路径参数（字符串 或 `{"b16": …}`，与写面同一口径）。
fn path_arg(args: &serde_json::Value, key: &str) -> Result<PathBuf, (&'static str, String)> {
    let v = args.get(key).ok_or(("bad_path", format!("少了 `{key}`")))?;
    let raw = crate::files::raw::from_json(v).ok_or((
        "bad_path",
        format!("`{key}` 的形状不对 —— 只认字符串或 `{{\"b16\": \"<十六进制>\"}}`"),
    ))?;
    if raw.is_empty() {
        return Err(("bad_path", format!("`{key}` 是空的")));
    }
    Ok(crate::files::raw::to_path_buf(&raw))
}

/// 取一个字符串参数。
fn str_arg<'a>(args: &'a serde_json::Value, key: &str) -> Result<&'a str, (&'static str, String)> {
    args.get(key)
        .and_then(serde_json::Value::as_str)
        .ok_or(("bad_args", format!("少了 `{key}`，或者它不是一个字符串")))
}

fn answer_commit(args: &serde_json::Value) -> Answer {
    answer_commit_at(&home_dir()?, args)
}

/// [`answer_commit`] 去掉「家在哪」那一问之后的全部 —— 判据从这里进（不改进程的 `HOME`：
/// 那是全进程共享的，改了会波及同一进程里并发跑的别的判据）。
fn answer_commit_at(home: &Path, args: &serde_json::Value) -> Answer {
    let root = path_arg(args, "root")?;
    let rel = str_arg(args, "rel")?.to_string();
    let key = str_arg(args, "key")?.to_string();
    // 🔴 覆盖策略**必须显式给**：默认成哪一边都是替用户做了一个他没做的决定。
    let overwrite = args
        .get("overwrite")
        .and_then(serde_json::Value::as_bool)
        .ok_or((
            "bad_args",
            "少了 `overwrite`（true / false）—— 覆盖不覆盖不给默认值".to_string(),
        ))?;
    let (landed, bytes) = commit_upload(home, &key, &root, &rel, overwrite)
        .map_err(|e| (e.code(), e.message().to_string()))?;
    // 暂存区清理「孤儿」那一格的事件：一次提交成功（`设计/60 §13.2 ④`）。
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    sweep_stale(home, now, &key);
    Ok(serde_json::json!({
        "path": crate::files::raw::to_json(crate::files::raw::path_bytes(&landed)),
        "bytes": bytes,
    }))
}

/// 取一个非负整数参数。
fn u64_arg(args: &serde_json::Value, key: &str) -> Result<u64, (&'static str, String)> {
    args.get(key)
        .and_then(serde_json::Value::as_u64)
        .ok_or(("bad_args", format!("少了 `{key}`，或者它不是一个非负整数")))
}

/// 这台后端的家（暂存区拼在它底下）。
fn home_dir() -> Result<PathBuf, (&'static str, String)> {
    std::env::var_os("HOME").map(PathBuf::from).ok_or((
        "io_failed",
        "这台后端不知道自己的 home（没有 `HOME`）—— 暂存区拼不出来".to_string(),
    ))
}

/// 〔F9c〕`files-stage-chunk`。
fn answer_stage_at(home: &Path, args: &serde_json::Value) -> Answer {
    let key = str_arg(args, "key")?.to_string();
    let seq = u64_arg(args, "seq")?;
    let v = args
        .get("content")
        .ok_or(("bad_args", "少了 `content` —— 一块至少 1 字节".to_string()))?;
    let bytes = crate::files::raw::from_json(v).ok_or((
        "bad_args",
        "`content` 的形状不对 —— 只认字符串或 `{\"b16\": \"<十六进制>\"}`".to_string(),
    ))?;
    if bytes.is_empty() {
        return Err((
            "bad_args",
            "空块 —— 每块至少 1 字节（块数的上界就靠这一条：块数 ≤ 总长）".to_string(),
        ));
    }
    let n =
        stage_chunk(home, &key, seq, &bytes).map_err(|e| (e.code(), e.message().to_string()))?;
    Ok(serde_json::json!({ "bytes": n }))
}

/// 〔F9c〕`files-commit-text`。
fn answer_commit_text_at(home: &Path, args: &serde_json::Value) -> Answer {
    let root = path_arg(args, "root")?;
    let rel = str_arg(args, "rel")?.to_string();
    let key = str_arg(args, "key")?.to_string();
    let chunks = u64_arg(args, "chunks")?;
    let bytes = u64_arg(args, "bytes")?;
    let cap = crate::files::READ_TEXT_MAX_BYTES as u64;
    if bytes > cap {
        return Err((
            "bad_args",
            format!(
                "`bytes` 是 {bytes}，超过 {cap}（`files-read-text` 一趟的天花板：存得回的要读得回来），多了 {} 字节",
                bytes - cap
            ),
        ));
    }
    if chunks == 0 || chunks > bytes {
        return Err((
            "bad_args",
            format!("`chunks` 是 {chunks}、`bytes` 是 {bytes} —— 每块至少 1 字节，块数只能在 1..=总长 之间"),
        ));
    }
    let expect = sha256_expect_of(args)?;
    let (landed, n, sha) = commit_text(home, &key, chunks, bytes, &root, &rel, &expect)
        .map_err(|e| (e.code(), e.message().to_string()))?;
    // 提交成功 ⇒ 顺手扫孤儿（同上传那一条的事件）。
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    sweep_stale(home, now, &key);
    Ok(serde_json::json!({
        "path": crate::files::raw::to_json(crate::files::raw::path_bytes(&landed)),
        "bytes": n,
        "sha256": sha,
    }))
}

/// 这一面的**唯一入口**（形状照写面那一个）。
pub fn answer_wire(wire_name: &str, args: &serde_json::Value) -> Answer {
    match wire_name {
        "files-commit-upload" => answer_commit(args),
        "files-stage-chunk" => answer_stage_at(&home_dir()?, args),
        "files-commit-text" => answer_commit_text_at(&home_dir()?, args),
        other => Err(("bad_args", format!("`{other}` 不是上传提交那一面的命令"))),
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/control/files_commit_tests.rs"]
mod tests;
