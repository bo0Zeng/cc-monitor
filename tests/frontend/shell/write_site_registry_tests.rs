use std::path::{Path, PathBuf};

/// 写盘调用的**形态清单**。人群按「做了什么」取：这些是 Rust 侧真正落盘的动作。
///
/// ⚠ 只读的 `fs::read*` / `metadata` / `exists` **不在其中** —— 本条钉的是「写」。
const WRITE_CALLS: &[&str] = &[
    "fs::write(",
    "fs::create_dir_all(",
    "fs::create_dir(",
    "fs::copy(",
    "fs::remove_file(",
    "fs::remove_dir_all(",
    "fs::remove_dir(",
    // ⚠ 这一个**刻意不带左括号**。带上就成了 `::rename(`，而
    // `atomic_replace_registry` 正是按那个形态数「原子替换调用点」——
    // 本文件一加它当场红（实测）。往那张表里塞一条豁免是最省事的消法，
    // 但那会把它变成废纸；去掉括号后：我这边照样认得出真调用，
    // 而**哪天有人在本文件里真写了一次 `fs::rename(...)`，那张表仍会逮住它**。
    // 代价是这个 needle 稍宽（会命中 `fs::rename_xxx` 之类），今天全树无此形态。
    "fs::rename",
    "File::create(",
    // **补一根针**：`OpenOptions` 开写（`O_EXCL` 新建 / 追加）。
    //   现打时这张表漏了它 —— 本机分叉那一处（`history.rs` 在 `~/.claude/projects/` 下 `O_EXCL` 写新会话）
    //   就是经它落盘的，**两张写点登记表都没看见**。那一处已经交给后端（`--fork-session`），这根针防它换个名字回来。
    "fs::OpenOptions::new(",
];

/// 每个落点的申报：`(文件, 函数, 属于哪个已声明工具, 说法)`。
///
/// 第三列 `Some(id)` = **这是那个工具的安装/卸载动作**，`id` 必须在
/// `tool_registry::TOOLS` 里真实存在（下面有对拍）；`None` = **不是安装动作**，
/// 第四列要写清它写的是什么。
///
/// ⚠ **默认拒绝**：人群从源码派生，没在这张表里的落点当场红。
/// 「谁进人群」由机器定，「它是不是安装动作」才是人的答案。
#[allow(clippy::type_complexity)]
pub(crate) const WRITE_SITES: &[(&str, &str, Option<&str>, &str)] = &[
    // ── P2z：单 exe 自释放内嵌后端。**不是安装动作** —— 它写的是 monitor 自己的缓存。
    ("local_backend.rs", "extract_embedded_to", None,
     "把后端字节放到 `~/.cc-monitor/bin/ccm`（它就是后端本身，本机常驻后端跑的与终端里敲的是同一个文件）。\
          写的是 monitor 自己的目录，不碰用户既有环境、不注册到任何用户配置里；换版照 HX2 D-b「盘上的比我旧才换」。\
          ⚠ **唯一调用点是 `local_backend::resolve_or_extract`**〔`K-R43` 改：本行原先写\
          「供 `start_or_extract` …」，那时它是唯一调用点；今天 `start_or_extract` 与\
          `local_backend_host::resolve_backend_bin` **都经那一份共用的解析**走到这里 ⇒ 写盘这一跳\
          仍然只有一处，而**吃它的路从一条变成两条**〕"),
    // ── P2t：收掉自己留下的 `.partial` 残骸。**不是安装动作** —— 它只**删**，且只删自己那套命名。
    ("local_backend.rs", "sweep_stale_partials", None,
     "删 `~/.cc-monitor/bin/.<释放名>.<pid>.partial` 里**够老**（≥24h）的残骸 —— \
          那是 `extract_embedded_to` 崩在 rename 之前留下的自家垃圾。\
          ⚠ 三重收窄，因为这个目录**与远端自部署共用**：① 只认自己那套命名（`.<释放名>.<pid>.partial`）；\
          ② 只删够老的（正常释放是毫秒级的顺序写，24h 给了五个数量级余量 —— \
          宁可多留一天垃圾，也不要把**正在写的**那份删掉，那等于自己制造半截文件）；\
          ③ 删不掉就算了，**清扫失败绝不挡住释放**。\
          ★ 为什么会有残骸：临时名从固定名改成**带 pid**（防两个 monitor 写同一个 `.partial`）之后，\
          崩掉的那些不会再被下一次覆盖 ⇒ 得自己收。"),
    // ── `extract_embedded_to` 自己那一份暂存件的收尾（问完 / 失败 / 不放，任何结局都删）。**不是安装动作** —— 它只**删**自己刚写的那一份。
    ("local_backend.rs", "drop_partial", None,
     "删 `extract_embedded_to` 这一趟刚写的 `~/.cc-monitor/bin/.<ccm 名>.<pid>.partial`（问手上那份字节放不放用的暂存件）：\
          放了（换名上位后它已不在）· 不放 · 它说「不」· 问不成，四种结局都走这里；清不掉出声，一天后由 `sweep_stale_partials` 收。"),
    // ── 本机那条 `ccm` 入口（逐字节副本 `install_local_ccm_entry`〔散文墓碑〕）删了：落点就是后端本身（`extract_embedded_to`）。
    ("local_backend.rs", "sweep_moved_aside", None,
     "删 `~/.cc-monitor/bin/.<ccm 名>.<pid>.old` —— Windows 上换版时正在跑的那份旧 `ccm` 只能改名挪开，下一次放置时收；\
          只认自己那套命名，删不掉就下次再说。"),
    ("local_backend.rs", "sweep_legacy_extracts", None,
     "删旧版本机释放的 `~/.cc-monitor/bin/cc-monitor-backend-<build_id>` —— 身份戳恰一个（是我们编的）才删，\
          认不出的不动、删不掉（正在跑）不管。"),
    // ── **本机那一份文件窗口程序**。不是安装动作 —— 我们自己的部署物，放在我们自己的目录里。
    ("local_backend.rs", "place_local_program", None,
     "把这一份产物带着的、没有身份戳的本机程序放到 `~/.cc-monitor/bin/<file>`：文件窗口程序 \
          `cc-monitor-filewin[.exe]`（monitor 旁边没有它时）。\
          写法与 `extract_embedded_to` 同一套（`.partial` + 置可执行位 + 上位走 `rename_into_place`）；盘上那份逐字节相等就零写。\
          调用点：`filewin::proc::resolve_window_bin`（开窗时）。\
          不碰用户既有环境、不注册到任何用户配置里。"),
    // ── 往 `~/.cc-monitor/bin` 放程序的两条路共用的上位那一步（旧的正在跑 ⇒ 先挪开）。不是安装动作。
    ("local_backend.rs", "rename_into_place", None,
     "`~/.cc-monitor/bin/.<名>.<pid>.partial` → `<名>`；Windows 上旧的那份正在跑换不掉 ⇒ 先把它改名成 `.<名>.<pid>.old` 再上位\
          （挪开的那份由 `sweep_moved_aside` 下次收）。写的只是我们自己刚放的那一份与我们自己的旧版。"),
    // ── 这里原来有 `cc_bus_deploy.rs` 的三行（`deploy_into` 装 17 个文件 ·
    //    `fenced_dest` 先 `mkdir -p skills` · `backup_existing` 整目录改名成 `.bak-<ts>`，`U10b` 第 7 条例外那四个配套的落点）。
    //    只有后端的文件管理部分写文件，本机也算 ⇒ 三件都改经本机后端（`files-put` 带 `parents` /
    //    `files-rename` / `files-chmod`），本进程一个字节不落 ⇒ 三行摘掉。四个配套一条没省：
    //    显式动作（只由那个按钮调）· realpath 围栏（`fenced_dest` 只读判 ＋ 后端围栏再判）· 幂等（逐文件经后端读回比）·
    //    可撤销（整目录改名，经后端）。
    // ── 构建期写盘：**不碰用户既有环境**，只往 `OUT_DIR` 放构建产物。
    // 单列在这里是因为它此前**整个在扫描面之外**（08-08 并入），
    // 而它确实在开发者机器上写文件 —— 「不是安装动作」得由人说出来，不是靠没人看见。
    ("build.rs", "embed_backends", None,
     "把 `embedded-backends/cc-monitor-backend-<arch>` 复制进 `OUT_DIR`，\
          供 `include_bytes!` 内嵌。写的是 cargo 自己的构建目录，不碰用户环境；\
          ⚠ 〔`K-R70` 09-12 订正本行后半句〕它**不再读旁边那份 `.build_id` 清单** —— \
          身份改从二进制字节里扫（`CC_MONITOR_BUILD_STAMP`），\
          由 `sftp_tests.rs::the_embedded_identity_comes_from_the_bytes_not_from_a_label` 守着"),
    // ── 这里原来有收件箱写（`write_skill_file`〔散文墓碑〕）一行（项目里的
    //    `.claude/planned-build/INBOX.txt`，本进程 `fs::write` ＋ `verified_write` 回读回滚）。要求「远端（和本机，
    //    同一条路）的 `INBOX.txt` 能编辑、经那台机器后端的文件管理那一面写」⇒ 读写都改经后端（`files-peek` /
    //    `files-put`，带 CAS 期望），本进程一个字节不落 ⇒ 摘行。三道围栏也进了那台后端；收件箱编辑面整块删了，那三道随之没了。
    // ── 🔴 这里原来有三行 `fenced_block.rs` 的本机原语
    //    （`put_atomic` / `save_backup` / `delete_created`，那时是本机 rc · `$PROFILE` · 别名文件 · rc 里那一行
    //    source 的唯一落盘漏斗）。只有后端的文件管理部分写文件，**本机也算** ⇒ 那几件改经本机后端
    //    （`user_files::edit` → `files-peek` / `files-put`），本进程**一个字节都不落** ⇒ 三行随原语一起走了。
    //    写的规则（备份 · 原子替换 · 回读 · 回滚）从此只住后端 `control/files_write.rs::put_text`。
    // ── 这里原来还有三行：`profile_installer.rs` 的 `atomic_write_string`〔散文墓碑〕 /
    //    `atomic_replace_path`（本机用户文件的原子写原语，申报成 `ccm` 的安装动作）与 `mcp.rs`（整份删了）的
    //    本机原子写（`project-mcp` 那条的真落点）。`$PROFILE` / rc / 项目 `.mcp.json` 全改经后端写
    //    （`user_files` → `files-put`），三件零调用方、删了 ⇒ 三行随之走。那两个工具的装 / 卸动作今天在后端落盘，
    //    本表（monitor 进程的写盘人群）里**不再有它们** —— 那正是用户那一裁要的形状。
    // ── 不是安装动作：写的是 monitor 自己的东西
    ("bind.rs", "spawn", None, "monitor 自己的运行时目录/落地文件"),
    ("bind.rs", "process_await_file", None, "monitor 自己的等待文件"),
    ("bind.rs", "cleanup_dead", None, "清理 monitor 自己留下的死文件"),
    ("config.rs", "patch_config_at", None, "monitor 自己的配置文件（唯一写口：进程级锁内现读 ＋ 按键补丁；整份替换的 `save_config` 删了）"), // 〔散文墓碑〕
    // ── 这里原来有一行 `creds_store.rs` 的凭据写口（K-H2a：账号的第三方 API key 那份文件）。
    //    「每台机器上这份文件的程序写者恰好一个 ＝ 那台的后端」⇒ 本机那一份也交本机常驻后端写
    //    （`apikey-key-set` → `src/backend/accounts/upstream_select/file_face.rs`，第四层后端自有状态），本进程一个字节不落 ⇒ 摘行。
    ("fs.rs", "atomic_replace", None, "原子替换原语的本地副本（同上，归 `atomic_replace_registry` 判；原住 `config.rs`，住壳的平台层）"),
    ("lib.rs", "open_log_dir", None, "打开日志目录前确保它存在"),
    ("logging.rs", "build_rolling_appender", None, "monitor 自己的滚动日志"),
    // `logging.rs` 那两行（`write_diagnostics_to_config` · `atomic_replace` 副本）摘了：诊断写口改经
    //   `config::patch_config_at`，本文件零写盘。
    // `session_map.rs` 那条 watcher 线程那一行摘了：monitor 自己那份判活（连同它写的会话映射状态）删了。
    // 「下载落到用户选的本机路径」那一行摘了（连同它上面那段 09-21 的订正：「本地缓存」那句是假的、
    //    围栏补在开单那一刻）—— 落地那一下随传输台搬进了本机常驻后端（第三层文件管理写面 `control/transfer.rs`），
    //    monitor 这一侧零写盘。上一版还说「开单时那道本机落点围栏照旧在中继里先判一次
    //    （`sftp_pool.rs::transfer_call`）」—— FN1 把那一判删了（后端那道也没了，本地那道是它的出声早副本）。
    // 原 `utils.rs`：随两个前端共用搬进 `host-core`（`src/common/host-core/src/atomic.rs`）。
    ("atomic.rs", "atomic_write_json", None, "通用原子写原语，调用方各自申报"),
    // 文件窗口的书签：锁旁件（空文件，只拿来上锁）＋ 它所在的目录。
    //   书签文件本身走上面那条原子写原语（`filewin/bookmarks.rs::mutate`）。
    ("bookmarks.rs", "lock_store", None,
     "monitor 自己的状态：文件窗口书签的锁旁件（`<数据目录>/filewin-bookmarks.json.lock`，空文件）"),
    ("atomic.rs", "atomic_replace_path", None, "同上，原语的本地副本"),
    // 钥匙文件那一行摘了：monitor 只交路径、只读，常驻后端绑上口之后自己换一把写进去（`control/resident.rs::rotate_token`）。
    // 「谁在听」那一行摘了：monitor 不再写那份记录，由常驻后端绑上口之后自己记（`control/resident.rs::record_owner`，本机远端同一个写者）。
    // ── 起脱离那条载体之前建好后端 stderr 诊断文件那一层目录。**不是安装动作**。
    ("local_backend_host.rs", "spawn_detached", None,
     "建 `<monitor 数据目录>/logs/backend/`（`create_dir_all`，只建目录）—— 脱离常驻的本机后端把自己的 stderr 落在\
          这一层里（后端只 `O_EXCL` 新建文件、不建目录，`src/backend/stderr_log.rs`）。写的是 monitor 自己的日志目录"),
    // ── 这里原来有 `history.rs` 的 `delete_history_session` 一行（**删用户数据**：〔散文墓碑〕
    //    本进程 `fs::remove_file` 删 `~/.claude/projects/**` 下的会话文件）。只有后端的文件管理部分写文件
    //    也管本机 ⇒ 删历史会话改成后端一条只收 sid 的命令（`files-delete-session`；当时说「会话文件围栏唯一的例外」，
    // FN1 之后写面已无那道围栏），
    //    本进程一个字节不删 ⇒ 摘行。那条「入口真的过了围栏」的端到端判据随之换成后端那一族与本侧的一致性闸判据。
];

/// 一行 `use ... fs ...` 该放行还是该拦。`Ok(())` = 放行。
///
/// # 为什么要有这道
///
/// 本模块的人群靠 **`fs::` 这个前缀**认写盘调用（`WRITE_CALLS` 每一条都带它），
/// `hooks_diag::this_module_never_writes` 的白名单也是扫 `fs::`（那个模块进了后端，本条照旧守 `config_surface`）〔散文墓碑〕。
/// 两条判据因此**共享同一个前提**：`std::fs` 只能以带前缀的形态出现。
///
/// 08-07 实测这个前提没人守：往 `hooks_diag.rs` 里加
/// `use std::fs as sysio;` + `sysio::write(p, s)`（一个名叫
/// 「绝不写盘」的模块里真写一次盘），全仓 **974 条判据一条不红** ——
/// 连本模块上一轮刚建的写点人群都漏掉了。⚠ 头一次变异我把别名取成 `ffs`，
/// 而 `ffs::write(` 里**含有** `fs::write(` 子串 ⇒ 两条判据都红了，
/// 差点被我读成「有人守着」。**变异要造得像，巧合的红比不红更骗人。**
///
/// 修法照后端侧 `readonly_guard` 的先例：**堵逃生口**，别去追那些改写形态。
fn fs_import_verdict(line: &str) -> Result<(), String> {
    let s = line.trim();
    if !s.starts_with("use ") || !s.contains("fs") {
        return Ok(());
    }
    // 放行两种，两种今天都真实存在（各自的理由写在这里，不是「看着眼熟」）：
    // · `use std::fs;`      —— 调用处必然写成 `fs::xxx(`，前缀还在
    // · `use std::fs::File;` —— File 的写入口 `File::create(` 自己就在 `WRITE_CALLS` 里
    if s == "use std::fs;" || s == "use std::fs::File;" {
        return Ok(());
    }
    if s.contains("std::fs as ") || s.contains("fs as ") {
        return Err("别名导入：调用处不再带 `fs::` 前缀，两条判据同时瞎掉".into());
    }
    if s.contains("std::fs::*") {
        return Err("glob 导入：写函数变成裸名字，前缀扫描扫不到".into());
    }
    if s.contains("std::fs::") {
        return Err("条目导入：`use std::fs::write;` 之后 `write(..)` 不带前缀".into());
    }
    Ok(())
}

/// ★★ **没有一种导入形态能让写盘调用丢掉 `fs::` 前缀**。
///
/// 这是上面那条人群、以及只读模块白名单（当年是 `hooks_diag::this_module_never_writes`〔散文墓碑〕）的**共同前提**。
/// 前提没人守的时候，两条判据会**同时**瞎掉且都保持绿色 —— 08-07 实测过（见
/// [`fs_import_verdict`] 的头注）。
#[test]
fn no_alias_or_item_import_can_hide_a_write_call() {
    // ① 常驻正反例：真代码里今天**没有**违规样本，那一支平时没人行使 ——
    //    本仓已连着五次栽在「新分支平时没人走」上，所以样本写死在这里。
    for ok in [
        "use std::fs;",
        "use std::fs::File;",
        "use std::path::Path;",
        "let x = 1;",
    ] {
        assert!(
            fs_import_verdict(ok).is_ok(),
            "{ok:?} 该放行却被拦了 —— 分类器过严会逼人去改合法代码"
        );
    }
    for bad in [
        "use std::fs as sysio;",
        "use std::fs as f;",
        "use std::fs::*;",
        "use std::fs::write;",
        "use std::fs::{self, write};",
    ] {
        assert!(
            fs_import_verdict(bad).is_err(),
            "{bad:?} 该被拦却放行了 —— 这正是 08-07 那次真写盘溜过去的形态"
        );
    }

    // ② 全树扫描。
    let files = corpus();
    let mut checked = 0usize;
    let mut offenders = Vec::new();
    for (path, src) in &files {
        let prod = guard_core::production_code(src);
        for line in prod.lines() {
            if line.trim().starts_with("use ") && line.contains("fs") {
                checked += 1;
                if let Err(why) = fs_import_verdict(line) {
                    offenders.push(format!("  {}: {} —— {why}", path.display(), line.trim()));
                }
            }
        }
    }
    // 抽取器自检：一条 `use ... fs ...` 都没扫到 ⇒ 上面整段在空转。
    assert!(
        checked >= 3,
        "全树只扫到 {checked} 行含 fs 的 `use`（08-07 实测 5：`use std::fs;` ×1 + `use std::fs::File;` ×4）\
             —— 剥法或扫描面坏了，本条此刻无效"
    );
    assert!(
        offenders.is_empty(),
        "这些导入会让写盘调用**丢掉 `fs::` 前缀**：\n{}\n\n\
             ⚠ 前缀一没，两条判据同时瞎掉且都保持绿色：\n\
             · 本模块的写点人群（`WRITE_CALLS` 每一条都带 `fs::`）；\n\
             · 只读模块的 `fs::` 白名单（`config_surface`）。\n\
             08-07 实测：`use std::fs as sysio;` + `sysio::write(p, s)` 放进 `hooks_diag.rs`，\n\
             974 条判据一条不红 —— 而那个模块判据的名字逐字写着「never writes」。\n\
             修法：用 `use std::fs;` 走全前缀，或直接写 `std::fs::xxx(`。别改本条去迁就它。",
        offenders.join("\n")
    );
}

fn src_root() -> PathBuf {
    // 住址唯一源：`crate::guard_support`（头注写着 24 份副本怎么一起漂的）。
    crate::guard_support::crate_src_root()
}

/// 语料 = `src/` 整棵树 **+ `build.rs`**。
///
/// ★ 为什么非把 `build.rs` 并进来：本表问的是「**谁能碰这台机器**」，
/// 而构建脚本每次 `cargo build`／`cargo check` 都在开发者机器上真跑
/// （它起 `sh` 与 `git`、往 `OUT_DIR` 复制内嵌后端）。
/// 08-08 实测：全仓所有登记表/守卫的扫描根都是 `src/frontend/shell/src` · `src/backend`
/// · `src/common` · `src` · `doc` —— **`src/frontend/shell/build.rs` 一张表都没扫到**，
/// 它是这些扫描面共同的盲点（与 F65「三张表共享同一个没写下来的前提」同族）。
fn corpus() -> Vec<(PathBuf, String)> {
    // 人群含本包 manifest 明写的兄弟源码树（`host-core` · 文件窗口 …，`guard_core::population_trees`）：它们也跑在这台机器的前端进程里。
    let mut files = guard_core::scan_tree!(&src_root(), &["rs"]);
    let bs = Path::new(env!("CARGO_MANIFEST_DIR")).join("build.rs");
    let src = std::fs::read_to_string(&bs).expect("读不到 build.rs —— 它是本表的一部分");
    files.push((bs, src));
    files
}

/// 从一份源码的**生产段**里抠出「有写盘调用的 (函数名)」。
///
/// ⚠ 归属按「上一处 `fn 名字`」判 —— 粗，但**只会把落点归给更靠前的函数**，
/// 归错了会让申报表里出现一个对不上的名字，那是**会红**的方向（不是静默变绿）。
fn write_fns(src: &str) -> Vec<String> {
    let prod = guard_core::production_code(src);
    let mut cur = String::new();
    let mut out: Vec<String> = Vec::new();
    for line in prod.lines() {
        if let Some(rest) = line
            .split(" fn ")
            .nth(1)
            .or_else(|| line.strip_prefix("fn "))
        {
            let name: String = rest
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            if !name.is_empty() {
                cur = name;
            }
        }
        if WRITE_CALLS.iter().any(|c| line.contains(c)) && !cur.is_empty() && !out.contains(&cur) {
            out.push(cur.clone());
        }
    }
    out
}

/// ★ 正题：**每个写盘落点都得申报**，安装动作那一类还要点名真实存在的工具。
#[test]
fn every_write_site_is_declared_and_installers_name_a_real_tool() {
    let files = corpus();
    let mut found: Vec<(String, String)> = Vec::new();
    for (path, src) in &files {
        let stem = path
            .file_name()
            .and_then(|s| s.to_str())
            .expect("文件名")
            .to_string();
        for f in write_fns(src) {
            found.push((stem.clone(), f));
        }
    }
    // ★ 抽取器自检：扫不到东西时下面整条会零命中地绿。
    assert!(
        found.len() >= 15,
        "全树只找到 {} 个写盘落点（08-07 实测 19 个「文件::函数」）—— 抽取器坏了，本条此刻无效",
        found.len()
    );

    let missing: Vec<String> = found
        .iter()
        .filter(|(f, n)| !WRITE_SITES.iter().any(|(sf, sn, _, _)| sf == f && sn == n))
        .map(|(f, n)| format!("  {f}::{n}"))
        .collect();
    assert!(
        missing.is_empty(),
        "这些地方**会往盘上写东西，但没人申报它是不是安装动作**：\n{}\n\n\
             ⚠ 记的正是这个缺口：`tool_registry` 只守声明表自洽，\n\
             人群是「声明了的工具」而不是「真实发生的安装动作」——\n\
             于是新增一个写点，声明表可以一直不知道。\n\
             登记进 `WRITE_SITES`：是某个工具的安装动作就点名它的 `id`（要在 `TOOLS` 里真实存在），\n\
             不是就写清它写的是什么（例如「monitor 自己的缓存」）。",
        missing.join("\n")
    );

    // ★ 反向锚点：申报了一个已经不存在的落点 ⇒ 它在替真判据挡枪。
    let stale: Vec<String> = WRITE_SITES
        .iter()
        .filter(|(f, n, ..)| !found.iter().any(|(ff, nn)| ff == f && nn == n))
        .map(|(f, n, ..)| format!("  {f}::{n}"))
        .collect();
    assert!(
        stale.is_empty(),
        "申报表里这些落点**已经不写盘了**（改名、收口或删掉了）：\n{}\n\
             改名也要红 —— 名字变了就该有人重新回答一次「它是不是安装动作」。",
        stale.join("\n")
    );

    // ★★ 4b 要的那条连线：安装动作必须点名 `TOOLS` 里真实存在的 id。
    // 申报表进了后端：本表 ＋ 落在 Claude 布局里的那一半（适配层）。
    const HOME: &str = include_str!("../../../src/backend/footprint/registry.rs");
    const AGENT: &str = include_str!("../../../src/backend/agents/claudecode/footprint.rs");
    let table = guard_core::production_code(HOME) + &guard_core::production_code(AGENT);
    let mut checked = 0usize;
    for (f, n, tool, _) in WRITE_SITES {
        let Some(id) = tool else { continue };
        checked += 1;
        assert!(
            table.contains(&format!("id: \"{id}\"")),
            "`{f}::{n}` 申报成工具 `{id}` 的安装动作，但 `tool_registry::TOOLS` 里没有这个 id。\n\
                 要么 id 写错了，要么那个工具被删了而真实的安装动作还留着 —— 后者更值得查。"
        );
    }
    // 常驻自检：一条安装动作都没有时，上面那个循环空转，而它看起来照样绿。
    // 5 → 1（地板改成相等）：`ccm` 的三行（远端入口那一份落点原语与
    //   `profile_installer.rs` 两个原语）· `project-mcp` 的一行 · `cc-bus` 的一行（`deploy_into`〔散文墓碑〕，装 cc-bus 今天在本机后端）
    //   随「用户文件改经后端写」走了，剩本机那条 `ccm` 入口（写的是我们自己的目录）。
    // 1 → 0：本机 `ccm` 不再是「装的一份副本」，就是后端本身（`extract_embedded_to`，`None` 那一档：monitor 自己的部署物）。
    //   ⇒ 这张表今天没有安装动作；循环空转由上面「每条都在 `SITE_CLASS` 里」那两向相等兜着。
    assert_eq!(
        checked, 0,
        "申报表里的「安装动作」条数变了（实得 {checked}）—— \
             要么真收口了（那很好，把这个数调下来），要么有人把它们改成了 `None` 绕过对拍。"
    );
}

// ══════════════════════════════════════════════════════════════════════════
// 🔴 **monitor 进程不直接写用户文件** —— 零命中，带正控
// ══════════════════════════════════════════════════════════════════════════
//
// 用户逐字：「现在只允许后端的文件管理部分写文件」；追问后裁「**只管用户的文件**」「**也管本机**」。
// ⇒ monitor 进程（本表的人群）里剩下的每一个写盘落点，都必须落在**不是用户文件**的那几类里；
//   用户文件（rc · `$PROFILE` · 项目 `.mcp.json` · skill 收件箱 · `~/.claude/skills/cc-bus` · 会话记录）
//   一律经那台机器的后端（`user_files` → `files-peek` / `files-put` / `files-rename` / `files-chmod` /
//   `files-delete-session`，本机分叉 `--fork-session`）。
//
// 两道，各治一形：
//   ① **分类闭集**（人的答案）：`WRITE_SITES` 里每一个落点在 [`SITE_CLASS`] 里恰好一行，类别是闭集，
//      **闭集里没有「用户文件」这一档** —— 新落点要么证明它不是用户文件，要么指名谁来收（待收）。两向相等。
//   ② **零命中**（机器的答案）：搬走写盘的那几份文件，生产段里一个写原语都不许再有（带正控：往一份副本里塞一处，必须数得出来）。

/// 一个写盘落点写的是什么。**闭集，而且刻意没有「用户文件」这一档。**
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Lands {
    /// monitor 自己的状态 / 缓存 / 日志 / 凭据（`~/.cc-monitor/`、monitor 数据目录）。
    OwnState,
    /// 我们自己的部署物（放在我们自己的目录里的二进制 / 入口）。
    OwnDeployment,
    /// cargo 的构建目录。
    BuildOutput,
    /// **是**用户文件，但有主、在别的路收：`(谁来收, 为什么不在本路)`。
    /// 今天**零成员**（唯一那一格 —— 下载落到用户选的本机路径 —— 被 SR1b 收了，落地搬进后端 `control/transfer.rs`）；
    /// 这一档留着：它是「一时搬不走、指名谁来收」这个形态本身，下一格要落时有地方落（同 `backend_route` 那条
    /// 「别因暂时没人用删判据形态」）。
    #[allow(dead_code)]
    Pending(&'static str),
}

/// ① 的人的答案：`(文件, 函数, 写的是什么)`。与 `WRITE_SITES` 的键**两向相等**。
const SITE_CLASS: &[(&str, &str, Lands)] = &[
    (
        "local_backend.rs",
        "extract_embedded_to",
        Lands::OwnDeployment,
    ),
    (
        "local_backend.rs",
        "sweep_stale_partials",
        Lands::OwnDeployment,
    ),
    ("local_backend.rs", "drop_partial", Lands::OwnDeployment),
    (
        "local_backend.rs",
        "sweep_moved_aside",
        Lands::OwnDeployment,
    ),
    (
        "local_backend.rs",
        "sweep_legacy_extracts",
        Lands::OwnDeployment,
    ),
    // 本机那一份文件窗口程序：同上，我们自己目录里的部署物。
    (
        "local_backend.rs",
        "place_local_program",
        Lands::OwnDeployment,
    ),
    (
        "local_backend.rs",
        "rename_into_place",
        Lands::OwnDeployment,
    ),
    ("build.rs", "embed_backends", Lands::BuildOutput),
    ("bind.rs", "spawn", Lands::OwnState),
    ("bind.rs", "process_await_file", Lands::OwnState),
    ("bind.rs", "cleanup_dead", Lands::OwnState),
    ("config.rs", "patch_config_at", Lands::OwnState),
    // `creds_store.rs` 的凭据写口那一行摘了（理由同上一张表）。
    ("fs.rs", "atomic_replace", Lands::OwnState), // 原住 `config.rs`
    ("lib.rs", "open_log_dir", Lands::OwnState),
    ("logging.rs", "build_rolling_appender", Lands::OwnState),
    // `logging.rs` 两行随写盘一起摘（见 `WRITE_SITES` 同处）。
    // `session_map.rs` 那条 watcher 线程那一行摘了：monitor 自己那份判活（连同它写的会话映射状态）删了。
    // 「下载落到用户选的本机路径」那一行（指名 SR1b 的待收例外）**收了**：
    //    下载的落地随传输台搬进本机常驻后端（`control/transfer.rs`，第三层文件管理写面；当时写「先过会话文件围栏」，
    //    FN1 之后只过路径解析）。
    // 通用原语：它自己不定落点，调用方各自申报（今天的调用方全是 monitor 自己的状态文件，
    // 下面第 ② 道把「搬走写盘的那几份文件」里调它也算成一处写）。
    ("atomic.rs", "atomic_write_json", Lands::OwnState),
    ("atomic.rs", "atomic_replace_path", Lands::OwnState),
    // monitor 自己的日志目录下那一层（后端 stderr 诊断文件住那里）。
    ("local_backend_host.rs", "spawn_detached", Lands::OwnState),
    // 文件窗口书签的锁旁件（`<monitor 数据目录>/filewin-bookmarks.json.lock`）——
    //   书签是 monitor 自己的状态（FW34 头注逐字「不是用户文件 ⇒ 不走后端写面」）。
    ("bookmarks.rs", "lock_store", Lands::OwnState),
];

/// ① 的判定（抽出来好喂正控）：两边的键两向对不上的那几条。
fn class_mismatch(
    sites: &[(&str, &str)],
    classes: &[(&str, &str, Lands)],
) -> (Vec<String>, Vec<String>) {
    let unclassified = sites
        .iter()
        .filter(|(f, n)| !classes.iter().any(|(cf, cn, _)| cf == f && cn == n))
        .map(|(f, n)| format!("{f}::{n}"))
        .collect();
    let ghosts = classes
        .iter()
        .filter(|(cf, cn, _)| !sites.iter().any(|(f, n)| f == cf && n == cn))
        .map(|(f, n, _)| format!("{f}::{n}"))
        .collect();
    (unclassified, ghosts)
}

#[test]
fn every_monitor_write_site_lands_outside_the_users_files() {
    let sites: Vec<(&str, &str)> = WRITE_SITES.iter().map(|(f, n, _, _)| (*f, *n)).collect();
    assert!(!sites.is_empty(), "写点表空了 —— 下面那条相等在空集上成立");
    let (unclassified, ghosts) = class_mismatch(&sites, SITE_CLASS);
    assert!(
        unclassified.is_empty() && ghosts.is_empty(),
        "monitor 的写盘落点与「它写的是什么」那张分类表对不上。\n  \
         没分类的（🔴 新落点先回答它是不是用户文件）：{unclassified:?}\n  \
         分类表里的幽灵（落点没了，同轮摘行）：{ghosts:?}\n\n\
         用户逐字「现在只允许后端的文件管理部分写文件」「只管用户的文件」「也管本机」。\n\
         ⇒ 分类是闭集，**没有「用户文件」这一档**：用户文件经那台机器的后端写（`user_files`）；\n\
         真是用户文件又一时搬不走的，记 `Pending` 并指名谁来收。"
    );
    for (f, n, c) in SITE_CLASS {
        if let Lands::Pending(owner) = c {
            assert!(
                owner.chars().count() >= 10,
                "`{f}::{n}` 记成待收，却没写清谁来收、为什么不在本路"
            );
        }
    }
    // 正控：多一处没分类的落点 ⇒ 这把尺子真的数得出来。
    let mut poisoned = sites.clone();
    // 靶子换成后端那一份（别名文件那一写随别名进了那台后端）——只拿它当「一处没分类的落点」的样本。
    poisoned.push(("mod.rs", "write_alias_file"));
    let (u, _) = class_mismatch(&poisoned, SITE_CLASS);
    assert_eq!(
        u,
        vec!["mod.rs::write_alias_file".to_string()],
        "正控没过 —— 本条空转"
    );
}

/// ② 的人群：用户文件的写从这些文件里搬走了（`RW1` 的七个子步逐份交给后端）。
const MOVED_OUT: &[&str] = &[
    // `account_aliases.rs` · `fenced_block.rs` 整份搬进了那台后端（`assets/aliases/`）⇒ 出名单；
    //   `profile_installer.rs` 只剩用户级 PATH 那一格，留在名单里。
    "profile_installer.rs",
    // `mcp.rs` 整份删了（MCP 读写进了那台后端）⇒ 出名单。
    // `skill_host.rs` 整份删了（收件箱那一面进了后端）⇒ 出名单。
    "cc_bus_deploy.rs",
    "history.rs",
    // `remote_history.rs` 整份删了（最后一个函数随子 agent 那条命令退役）⇒ 出名单。
    "user_files.rs",
];

/// ② 的针：本机写原语（`WRITE_CALLS`）＋ 委托出去的写（通用原子写 / 旧的回读回滚写入器）＋ SFTP 上传原语。
/// **运行时拼**（写成字面量会被别的判据当成靶子，见 `remote_write_registry_tests::write_prims` 头注）。
/// ⚠ SFTP 会话上的 `.rename(` / `.remove_file(` 那一族**不在这里**：持 SFTP 会话的文件由
///   `remote_write_registry` 逐处申报（它的人群就是「拿得到会话的那几份」），本条再数一遍只会与
///   `Door::rename` 这种经后端的调用撞名。
fn moved_out_needles() -> Vec<String> {
    let mut v: Vec<String> = WRITE_CALLS.iter().map(|s| s.to_string()).collect();
    // ⚠ 名字也**运行时拼**：其中两个是已删的旧名（挂着墓碑），写成字面量会让死名普查把它们当成活名。
    for (a, b) in [
        ("atomic_write_", "json"),
        ("atomic_write_", "string"),
        ("upload_", "atomic"),
        ("verify_and_", "rollback"),
    ] {
        v.push(format!("{a}{b}("));
    }
    v
}

/// ② 里**逐行登记的例外**：搬走写盘的那几份文件里，还在写 **monitor 自己的**状态文件的那几行。
/// `(文件, 那一行逐字, 写的是什么)`。整行相等，不是子串；每一行必须恰好出现一次（幽灵检查）。
/// **今天是空表**：唯一那一行（`history.rs` 写 `history-metadata.json` —— 标星 / 改名 / 隐藏这些注解）摘了：
/// 注解的读写者换成本机常驻后端（`src/backend/history/history_annotations.rs`，第四层；文件原地不动）⇒
/// 搬走写盘的这几份文件里，连 monitor 自己的状态也一行都不写了。表留着：新长一处「写自己的状态」时第一个要表态的地方。
const OWN_STATE_LINES: &[(&str, &str, &str)] = &[];

fn moved_out_hits(file: &str, prod: &str) -> Vec<String> {
    let needles = moved_out_needles();
    prod.lines()
        .map(str::trim)
        .filter(|l| needles.iter().any(|n| l.contains(n.as_str())))
        .filter(|l| {
            !OWN_STATE_LINES
                .iter()
                .any(|(f, line, _)| *f == file && l == line)
        })
        .map(str::to_string)
        .collect()
}

#[test]
fn the_files_that_used_to_write_users_files_write_nothing_now() {
    let root = src_root();
    let mut scanned = 0usize;
    let mut offenders: Vec<String> = Vec::new();
    for f in MOVED_OUT {
        let raw = std::fs::read_to_string(root.join(f))
            .unwrap_or_else(|e| panic!("读不到 {f}：{e} —— 搬家了就把名单一起改"));
        let prod = guard_core::production_code(&raw);
        guard_core::assert_no_test_code(f, &prod);
        scanned += 1;
        for hit in moved_out_hits(f, &prod) {
            offenders.push(format!("  {f}: {hit}"));
        }
        for (lf, line, what) in OWN_STATE_LINES {
            if lf == f {
                assert_eq!(
                    prod.lines().filter(|l| l.trim() == *line).count(),
                    1,
                    "登记的自有状态写（{what}）在 `{f}` 里不是恰好一行 —— 改了就同轮改登记"
                );
            }
        }
    }
    assert_eq!(scanned, MOVED_OUT.len());
    assert!(
        offenders.is_empty(),
        "这些文件的写已经交给那台机器的后端了，生产段却又长出了写原语：\n{}\n\n\
         用户逐字「现在只允许后端的文件管理部分写文件」（也管本机）。\n\
         ⇒ 改用 `user_files::edit` / `Door`（`files-peek` / `files-put` …），别在 monitor 进程里落盘。",
        offenders.join("\n")
    );
    // 正控：往一份真源码的副本里塞一处直写，必须被数出来（不许在空人群上恒绿）。
    let raw = std::fs::read_to_string(root.join("profile_installer.rs")).expect("读");
    let poisoned = format!(
        "{}\nfn sneaky(p: &std::path::Path) {{ let _ = std::fs::write(p, b\"x\"); }}\n",
        guard_core::production_code(&raw)
    );
    assert_eq!(
        moved_out_hits("profile_installer.rs", &poisoned).len(),
        1,
        "正控没过 —— 针或剥法坏了，本条空转"
    );
}
