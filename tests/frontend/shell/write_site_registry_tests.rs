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

/// 每个落点的申报：`(文件, 函数, 写的落在哪一类地方, 写的是什么)`。
///
/// ⚠ **默认拒绝**：人群从源码派生，没在这张表里的落点当场红。第三列是闭集 [`Lands`]，
/// 闭集里**没有「用户文件」这一档**：用户文件一律经那台机器的后端写（`user_files` → `files-put` …），
/// 新落点要在这里登记，就得先说清它落在哪一类不是用户文件的地方。
#[allow(clippy::type_complexity)]
const WRITE_SITES: &[(&str, &str, Lands, &str)] = &[
    // ── P2z：单 exe 自释放内嵌后端。**不是安装动作** —— 它写的是 monitor 自己的缓存。
    ("local_backend.rs", "extract_embedded_to", Lands::OwnDeployment,
     "把后端字节放到 `~/.cc-monitor/bin/ccm`（它就是后端本身，本机常驻后端跑的与终端里敲的是同一个文件）。\
          写的是 monitor 自己的目录，不碰用户既有环境、不注册到任何用户配置里；换版照 HX2 D-b「盘上的比我旧才换」。\
          ⚠ **唯一调用点是 `local_backend::resolve_or_extract`**〔`K-R43` 改：本行原先写\
          「供 `start_or_extract` …」，那时它是唯一调用点；今天 `start_or_extract` 与\
          `local_backend_host::resolve_backend_bin` **都经那一份共用的解析**走到这里 ⇒ 写盘这一跳\
          仍然只有一处，而**吃它的路从一条变成两条**〕"),
    // ── P2t：收掉自己留下的 `.partial` 残骸。**不是安装动作** —— 它只**删**，且只删自己那套命名。
    ("local_backend.rs", "sweep_stale_partials", Lands::OwnDeployment,
     "删 `~/.cc-monitor/bin/.<释放名>.<pid>.partial` 里**够老**（≥24h）的残骸 —— \
          那是 `extract_embedded_to` 崩在 rename 之前留下的自家垃圾。\
          ⚠ 三重收窄，因为这个目录**与远端自部署共用**：① 只认自己那套命名（`.<释放名>.<pid>.partial`）；\
          ② 只删够老的（正常释放是毫秒级的顺序写，24h 给了五个数量级余量 —— \
          宁可多留一天垃圾，也不要把**正在写的**那份删掉，那等于自己制造半截文件）；\
          ③ 删不掉就算了，**清扫失败绝不挡住释放**。\
          ★ 为什么会有残骸：临时名从固定名改成**带 pid**（防两个 monitor 写同一个 `.partial`）之后，\
          崩掉的那些不会再被下一次覆盖 ⇒ 得自己收。"),
    // ── `extract_embedded_to` 自己那一份暂存件的收尾（问完 / 失败 / 不放，任何结局都删）。**不是安装动作** —— 它只**删**自己刚写的那一份。
    ("local_backend.rs", "drop_partial", Lands::OwnDeployment,
     "删 `extract_embedded_to` 这一趟刚写的 `~/.cc-monitor/bin/.<ccm 名>.<pid>.partial`（问手上那份字节放不放用的暂存件）：\
          放了（换名上位后它已不在）· 不放 · 它说「不」· 问不成，四种结局都走这里；清不掉出声，一天后由 `sweep_stale_partials` 收。"),
    ("local_backend.rs", "sweep_moved_aside", Lands::OwnDeployment,
     "删 `~/.cc-monitor/bin/.<ccm 名>.<pid>.old` —— Windows 上换版时正在跑的那份旧 `ccm` 只能改名挪开，下一次放置时收；\
          只认自己那套命名，删不掉就下次再说。"),
    // ── **本机那一份文件窗口程序**。不是安装动作 —— 我们自己的部署物，放在我们自己的目录里。
    ("local_backend.rs", "place_local_program", Lands::OwnDeployment,
     "把这一份产物带着的、没有身份戳的本机程序放到 `~/.cc-monitor/bin/<file>`：文件窗口程序 \
          `cc-monitor-filewin[.exe]`（开文件窗口都走这一份；安装包里不另放）。\
          写法与 `extract_embedded_to` 同一套（`.partial` + 置可执行位 + 上位走 `rename_into_place`）；盘上那份逐字节相等就零写。\
          调用点：`filewin::proc::resolve_window_bin`（开窗时）。\
          不碰用户既有环境、不注册到任何用户配置里。"),
    // ── 往 `~/.cc-monitor/bin` 放程序的两条路共用的上位那一步（旧的正在跑 ⇒ 先挪开）。不是安装动作。
    ("local_backend.rs", "rename_into_place", Lands::OwnDeployment,
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
    ("build.rs", "embed_backends", Lands::BuildOutput,
     "把 `embedded-backends/cc-monitor-backend-<arch>` 复制进 `OUT_DIR`，\
          供 `include_bytes!` 内嵌。写的是 cargo 自己的构建目录，不碰用户环境；\
          ⚠ 〔`K-R70` 09-12 订正本行后半句〕它**不再读旁边那份 `.build_id` 清单** —— \
          身份改从二进制字节里扫（`CC_MONITOR_BUILD_STAMP`），\
          由 `deploy_tests.rs::the_embedded_identity_comes_from_the_bytes_not_from_a_label` 守着"),
    // 进程清单的编法搬进 `src/common/win-manifest`（两个前端包的 `build.rs` 都调它）；它在人群里（`population`），文件名是 `lib.rs`。
    ("lib.rs", "embed_for_every_artifact", Lands::BuildOutput,
     "构建期：往 `OUT_DIR` 写一行 `.rc`（指向 `win-manifest/app.manifest`），编成 Windows 进程清单资源，\
          链进调它的那个包的每一个产物（测试程序也要）。写的是 cargo 自己的构建目录，不碰用户环境"),
    // 同一处编法的第二件：文件窗口程序的图标（size2 10-11 起它不经 `tauri-build`）。
    ("lib.rs", "embed_icon_for_bins", Lands::BuildOutput,
     "构建期：往 `OUT_DIR` 写一行 `.rc`（指向 monitor 那份 `icon.ico`），编成 Windows 图标资源，\
          只链进调它的那个包的程序（文件窗口）。写的是 cargo 自己的构建目录，不碰用户环境"),
    // ── 🔴 这里原来有三行 `fenced_block.rs` 的本机原语
    //    （`put_atomic` / `save_backup` / `delete_created`，那时是本机 rc · `$PROFILE` · 别名文件 · rc 里那一行
    //    source 的唯一落盘漏斗）。只有后端的文件管理部分写文件，**本机也算** ⇒ 那几件改经本机后端
    //    （`user_files::edit` → `files-peek` / `files-put`），本进程**一个字节都不落** ⇒ 三行随原语一起走了。
    //    写的规则（备份 · 原子替换 · 回读 · 回滚）从此只住后端 `control/files_write.rs::put_text`。
    // ── 不是安装动作：写的是 monitor 自己的东西
    ("bind.rs", "spawn", Lands::OwnState, "monitor 自己的运行时目录/落地文件"),
    ("bind.rs", "process_tty_file", Lands::OwnState, "monitor 自己的握手表（bash / zsh 那一份认上了写一条登记）· 删用完的那份等待记录"),
    ("console.rs", "tty_title", Lands::TerminalDevice, "不是盘上的文件：往本机 bash / zsh 接入块记下的那个终端设备写改标题的控制序列（终端程序据此改窗口标题）"),
    ("bind.rs", "forget", Lands::OwnState, "shell 退了：删 monitor 自己的握手表里那一条"),
    ("config.rs", "patch_config_at", Lands::OwnState, "monitor 自己的配置文件（唯一写口：进程级锁内现读 ＋ 按键补丁）"),
    ("bookmarks_migration.rs", "settled_at", Lands::OwnState, "书签迁移（下一版删）：monitor 自己的旧书签文件 `filewin-bookmarks.json` 在锁旁件里删一格 / 删空了连旁件删整份"),
    // ── 这里原来有一行 `creds_store.rs` 的凭据写口（K-H2a：账号的第三方 API key 那份文件）。
    //    「每台机器上这份文件的程序写者恰好一个 ＝ 那台的后端」⇒ 本机那一份也交本机常驻后端写
    //    （`apikey-key-set` → `src/backend/accounts/upstream_select/file_face.rs`，后端自有状态，写经 `Own`），本进程一个字节不落 ⇒ 摘行。
    ("lib.rs", "open_log_dir", Lands::OwnState, "打开日志目录前确保它存在"),
    ("logging.rs", "build_rolling_appender", Lands::OwnState, "monitor 自己的滚动日志"),
    // `logging.rs` 那两行（`write_diagnostics_to_config` · `atomic_replace` 副本）摘了：诊断写口改经
    //   `config::patch_config_at`，本文件零写盘。
    // `session_map.rs` 那条 watcher 线程那一行摘了：monitor 自己那份判活（连同它写的会话映射状态）删了。
    // 「下载落到用户选的本机路径」那一行摘了（连同它上面那段 09-21 的订正：「本地缓存」那句是假的、
    //    围栏补在开单那一刻）—— 落地那一下随传输台搬进了本机常驻后端（`control/transfer.rs`，写经帧面交来的 `Face`；传输台 layer11 起归原生那一块），
    //    monitor 这一侧零写盘。上一版还说「开单时那道本机落点围栏照旧在中继里先判一次
    //    （`transfer_relay.rs::transfer_call`）」—— FN1 把那一判删了（后端那道也没了，本地那道是它的出声早副本）。
    // 原 `utils.rs`：随两个前端共用搬进 `host-core`（`src/common/host-core/src/atomic.rs`）。
    ("atomic.rs", "atomic_write_json", Lands::OwnState, "通用原子写原语，调用方各自申报"),
    ("atomic.rs", "atomic_replace_path", Lands::OwnState, "同上，原语的本地副本"),
    // 文件窗口的帧日志：只在设了 `CCM_FILEWIN_FRAME_LOG=<文件>` 时开，往那份文件追加（收集「一闪一闪」的证据）。
    ("frame_log.rs", "to_file", Lands::OwnState,
     "文件窗口自己的诊断日志：用户经环境变量点名的那一份文件，只追加；不设就不碰盘"),
    // ── 起脱离那条载体之前建好后端 stderr 诊断文件那一层目录。**不是安装动作**。
    ("local_backend_host.rs", "spawn_detached", Lands::OwnState,
     "建 `<monitor 数据目录>/logs/backend/`（`create_dir_all`，只建目录）—— 脱离常驻的本机后端把自己的 stderr 落在\
          这一层里（后端只 `O_EXCL` 新建文件、不建目录，`src/backend/stderr_log.rs`）。写的是 monitor 自己的日志目录"),
    // 删历史会话是后端一条只收 sid 的命令（`files-delete-session`），本进程一个字节不删 ⇒ 不在本表。
];

/// 一行 `use ... fs ...` 该放行还是该拦。`Ok(())` = 放行。
///
/// # 为什么要有这道
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
    // 住址唯一源：`guard_core::addr`。
    guard_core::addr::shell_src_root()
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

/// ★ 正题：**每个写盘落点都得申报**，两向相等。
#[test]
fn every_write_site_is_declared() {
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
    // 抽取器坏了扫不到东西 ⇒ 下面反向那一条（申报的落点必须真在）红。

    let missing: Vec<String> = found
        .iter()
        .filter(|(f, n)| !WRITE_SITES.iter().any(|(sf, sn, _, _)| sf == f && sn == n))
        .map(|(f, n)| format!("  {f}::{n}"))
        .collect();
    assert!(
        missing.is_empty(),
        "这些地方**会往盘上写东西，但没人申报**：\n{}\n\n\
             登记进 `WRITE_SITES`：写清它落在哪一类地方（`Lands`，没有「用户文件」这一档）、写的是什么。\n\
             写的是用户文件 ⇒ 改经那台机器的后端写（`user_files`）。",
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
             改名也要红 —— 名字变了就该有人重新回答一次「它写的是什么」。",
        stale.join("\n")
    );
}

// ══════════════════════════════════════════════════════════════════════════
// 🔴 **monitor 进程不直接写用户文件**：由类型挡
// ══════════════════════════════════════════════════════════════════════════
//
// 用户逐字：「现在只允许后端的文件管理部分写文件」；追问后裁「**只管用户的文件**」「**也管本机**」。
// ⇒ monitor 进程里剩下的每一个写盘落点，都落在**不是用户文件**的那几类里；
//   用户文件（rc · `$PROFILE` · 项目 `.mcp.json` · skill 收件箱 · 会话记录）一律经那台机器的后端
//   （`user_files` → `files-peek` / `files-put` / `files-rename` / `files-chmod` / `files-delete-session`，本机分叉 `--fork-session`）。
//   `WRITE_SITES` 第三列是 [`Lands`]，闭集里没有「用户文件」这一档，每个落点都要在表里（上一条两向相等）。

/// 一个写盘落点写的是什么。**闭集，而且刻意没有「用户文件」这一档。**
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Lands {
    /// monitor 自己的状态 / 缓存 / 日志 / 凭据（`~/.cc-monitor/`、monitor 数据目录）。
    OwnState,
    /// 我们自己的部署物（放在我们自己的目录里的二进制 / 入口）。
    OwnDeployment,
    /// cargo 的构建目录。
    BuildOutput,
    /// 不是盘上的文件：终端设备（`/dev/pts/*`），写的是改标题的控制序列。
    TerminalDevice,
}
