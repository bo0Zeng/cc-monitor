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
     "把内嵌的后端二进制释放到 `~/.cc-monitor/bin/cc-monitor-local-<build_id>`，\
          供 exe 旁没有本机后端时起进程。写的是 monitor 自己的目录，\
          不碰用户既有环境、不注册到任何用户配置里；按 build_id 命名 ⇒ 幂等、不覆盖别的版本。\
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
    // ── 🔴 `K-R69`：**本机那条 `ccm` 入口**。这是安装动作（`ccm` 这个工具的本机那一半）。
    ("local_backend.rs", "install_local_ccm_entry", Some("ccm"),
     "把**后端二进制自己的改名副本**放到 `~/.cc-monitor/bin/ccm`（Windows 上带 `.exe`，\
          名字的唯一真相源是 `local_backend::local_ccm_entry_name`）。\
          ⚠ **不是第二份实现**：`control::ccm::intercept` 认 `argv[0]` 的 basename ⇒ \
          改个名字就是那条入口，零新增 argv 解析（`K33`：所有命令只许有一处）。\
          🔴 **落点刻意不是 `~/.local/bin/ccm`** —— 那是用户那份旧 `ccm` 住的地方，\
          `K34` 逐字「原本的配置**要手动删除**」、`K31`「不许动用户机器」\
          ⇒ 产品一个字节都不动它，只在自己的目录里放一份，并**说得出**\
          「你 PATH 上那个不是我们装的这一份」（`ccm_probe::classify_path_ccm`）。\
          写法与 `extract_embedded_to` 同一套（`.partial` + 置可执行位 + `rename`），\
          唯一调用点是 `local_backend::resolve_or_extract` ⇒ 两条生产路共用这一处。"),
    // ── PS1：把内嵌的 cc-bus 装到 `<claude_dir>/skills/cc-bus/`。**这是安装动作**。
    ("cc_bus_deploy.rs", "deploy_into", Some("cc-bus"),
     "写 `<claude_dir>/skills/cc-bus/` 的 17 个文件（内嵌自 `src/shared/cc-bus/`）。\
          ⚠ 这是本仓**第一处往 `<claude_dir>` 写的地方** —— 只读铁律原本禁它，\
          `U10b`〔用@08-13〕裁「开」之后写成 `INVARIANTS` 的**第 7 条例外**，四个配套一条不省：\
          ① **用户显式动作**（只由设置页按钮调，绝不在启动/后台路径上跑）；\
          ② **独立 realpath 白名单**（`fenced_dest`：canonicalize 后必须仍在 claude_dir 下，\
             挡「skills 是指向别处的软链」）；\
          ③ **幂等**（逐文件比内容，一致就一个字节都不写、也不留备份）；\
          ④ **可撤销**（覆盖前把旧目录整个 rename 成 `cc-bus.bak-<ts>`）。",
    ),
    ("cc_bus_deploy.rs", "fenced_dest", None,
     "`mkdir -p <claude_dir>/skills` —— 只为**建出围栏要归一的那一层**（`canonicalize` 需要\
          路径真实存在）。**不是安装动作**：它一个内容文件都不写；真正的安装在 `deploy_into`。\
          ⚠ 它同时是那道围栏本身：归一之后必须仍在 `claude_dir` 下，否则**拒收**。"),
    ("cc_bus_deploy.rs", "backup_existing", None,
     "把已装的 `cc-bus` 整个 `rename` 成 `cc-bus.bak-<ts>`。**不是安装动作**，是它的\
          **可撤销**那一格（`U10b` 第 7 条例外的四个配套之一）。\
          ⚠ 只在**内容确有变化**时才做：全一致时不备份，否则每点一次就多一份垃圾备份。"),
    // ── 构建期写盘：**不碰用户既有环境**，只往 `OUT_DIR` 放构建产物。
    // 单列在这里是因为它此前**整个在扫描面之外**（08-08 并入），
    // 而它确实在开发者机器上写文件 —— 「不是安装动作」得由人说出来，不是靠没人看见。
    ("build.rs", "embed_backends", None,
     "把 `embedded-backends/cc-monitor-backend-<arch>` 复制进 `OUT_DIR`，\
          供 `include_bytes!` 内嵌。写的是 cargo 自己的构建目录，不碰用户环境；\
          ⚠ 〔`K-R70` 09-12 订正本行后半句〕它**不再读旁边那份 `.build_id` 清单** —— \
          身份改从二进制字节里扫（`CC_MONITOR_BUILD_STAMP`），\
          由 `sftp_tests.rs::the_embedded_identity_comes_from_the_bytes_not_from_a_label` 守着"),
    // ── devbench F03：skill 接入面的收件箱写入。**不是安装动作**。
    ("skill_host.rs", "write_skill_file", None,
     "写用户**自己项目里**的 `.claude/planned-build/INBOX.txt`（planned-build skill 的\
          「结构化注入」进件口）。**不碰 Claude 的数据、不碰用户环境、不装任何东西。**\n\
          分界沿用 `src/doc/INVARIANTS.md` `§1` 那条 F47 澄清的口径：用户亲自驱动、\
          每次写都是面板内一次直接手势、写的不是 Claude 的 jsonl/pidfile。\n\
          三道围栏（`skill_host::resolve_editable`）：① 路径 `canonicalize` **之后**\
          做集合判定（集合来自声明表的 `editable`，不是一串 if）② 过\
          `claude_data_fence::is_protected_claude_data_path` 纵深〔步 H2 09-21：\
          这道判定已从 `sftp_pool` 搬成独立一族，F47 与 F03b 两段澄清共用它这一个〕\
          ③ 目标必须**已存在**\
          （本功能是「编辑收件箱」不是「创建任意文件」）。\n\
          写本身走 `verified_write::verify_and_rollback`（备份 → 写 → 读回逐字节比对 →\
          不符即回滚），**没有自造第四份写入实现** —— 那个模块头注记着本仓曾有 4 处\
          独立实现且校验强度不一致（两处只比长度）。"),
    // ── `K-R49`：加了账号就把 `alphacc` / `betacc` 那条命令落下来。**两个落点性质完全不同，分两行记。**
    ("account_aliases.rs", "write_alias_file", None,
     "整份重写 `~/.cc-monitor/account-aliases.sh`。**不是安装动作** —— 写的是 monitor \
          自己的目录（与 `local_backend` 的 `bin/`、`local_backend_host` 的 `listen-token` 同一个），\
          用户的 shell 配置一个字节都不碰。\
          ★ 为什么是「整份重写」而不是往 `~/.bashrc` 追加：追加那条路上，加三个账号就追三次、\
          删了账号那一行还留着指向一个不存在的号，而**弄坏的代价是 shell 起不来**。\
          整份重写换来三条：幂等（内容一致时一个字节都不写）· 删了账号它那条当场消失 · \
          删掉整份文件也只是少几个命令。落盘借 `profile_installer::atomic_write_string`，\
          写完回读逐字比对，不符就把这份**我们自己的**文件删掉（半截的它比没有它更坏）。"),
    ("account_aliases.rs", "ensure_rc_source_line", None,
     "往用户**自己指定**的那份 rc 里装**一行** `source`（BEGIN/END 围栏内）。\
          ⚠ 它确实写用户既有的环境，但**不是安装动作**：它不装任何 `TOOLS` 里的工具，\
          只是让上面那份生成文件被 source 到 —— 真正的内容一个字节都不在这里。\
          🔴 四道：① 路径过 `profile_installer::fence_profile_path`（只许落在 home 之内，\
          而且那份 rc 由界面上的人**选**，代码不猜）；② 已经 source 过就一个字节都不写（幂等）；\
          ③ 围栏损坏（有 BEGIN 没 END）**中止**，绝不用后面那个 END 去配对吃掉用户代码；\
          ④ 先 `fs::copy` 备份、写完回读逐字比对、不符从备份回滚。\
          ★ 多数人根本走不到这一行：`src/shared/ccm-aliases.sh` 自带那行 `[ -r … ] && . …`，\
          装过 ccm 别名块的人加账号之后什么都不用做。"),
    // ── 安装动作：写的是**用户既有的环境/配置**，且对应声明表里的一个工具
    ("profile_installer.rs", "install_to_profile", Some("ccm"),
     "往用户 shell profile 的 BEGIN/END 块里装 ccm 启动器（写前先备份）"),
    ("profile_installer.rs", "uninstall_from_profile", Some("ccm"),
     "从 profile 里摘掉那个块（同样先备份）"),
    ("profile_installer.rs", "atomic_write_string", Some("ccm"),
     "上面两个动作唯一的落盘漏斗：临时文件 + rename"),
    ("profile_installer.rs", "atomic_replace_path", Some("ccm"),
     "跨设备回退的 rename。⚠ 这是**四份平台原语副本之一**，四份都已登记在 `atomic_replace_registry`（承接 C10）——本条不重复判它，只记它是个写点"),
    ("mcp.rs", "write_json_atomic", Some("project-mcp"),
     "写项目级 MCP 服务器配置（`TOOLS` 里 `project-mcp` 那条的真落点）"),
    // ── 不是安装动作：写的是 monitor 自己的东西
    ("bind.rs", "spawn", None, "monitor 自己的运行时目录/落地文件"),
    ("bind.rs", "process_await_file", None, "monitor 自己的等待文件"),
    ("bind.rs", "cleanup_dead", None, "清理 monitor 自己留下的死文件"),
    ("config.rs", "save_config", None, "monitor 自己的配置文件"),
    // K-H2a：第三方 API key 那份文件。**monitor 自己的文件**（不是用户的、也不是某个工具的安装动作）。
    // ⚠ 它与人手编是同一份文件的两个写者 ⇒ 写的那一刻才读盘、未知键一个不吃、
    //   字段顺序按名字排、原子替换（复用 `config::atomic_replace`）、写完立刻收窄成只给本人。
    ("creds_store.rs", "write_key_at", None, "monitor 自己的凭据文件（中转那把第三方 API key）"),
    ("config.rs", "atomic_replace", None, "原子替换原语的本地副本（同上，归 `atomic_replace_registry` 判）"),
    ("lib.rs", "open_log_dir", None, "打开日志目录前确保它存在"),
    ("logging.rs", "build_rolling_appender", None, "monitor 自己的滚动日志"),
    ("logging.rs", "write_diagnostics_to_config", None, "把诊断信息写进 monitor 自己的配置"),
    ("logging.rs", "atomic_replace", None, "原子替换原语的本地副本（头注自陈是从 config.rs 复制的）"),
    ("session_map.rs", "run_watcher", None, "monitor 自己的会话映射状态"),
    // 🔴〔订正 2026-09-21〕这一行原先逐字写着「把远端文件落到**本地缓存**；
    //    写的**不是用户既有环境**」—— **两句都是假的**：`local_path` 由用户在保存
    //    对话框（老面板）或那个落点框（原生窗口）里给，**用户指哪写哪**，那按定义
    //    就是用户既有环境，不是缓存。
    //    而它是一条**推诿链的末端**：`remote_write_registry` 的两张表各自把「本机落点
    //    那一半」指到本表，本表这一行又说「不是用户既有环境」⇒ 没人守，洞活着。
    //    ⇒ 围栏已补在 `sftp_pool::sftp_download` 第一行（`guard_write(&local_path)`）。
    //    ⚠ 本表仍然只是**申报**，不是守卫 —— 别再把它当挡箭牌。
    ("sftp_pool.rs", "download_inner", None, "把远端文件落到**用户选的本机路径**（保存对话框 / 落点框）；围栏在入口 `sftp_download` 第一行"),
    ("utils.rs", "atomic_write_json", None, "通用原子写原语，调用方各自申报"),
    ("utils.rs", "atomic_replace_path", None, "同上，原语的本地副本"),
    // ── `K-P1`：常驻那条路要写两样东西。**都不是安装动作** —— 写的是 monitor 自己的目录。
    ("local_backend_host.rs", "ensure_listen_token", None,
     "写 `~/.cc-monitor/listen-token`（**`0600`**，`create_new` 只创建一次）。\
          ★ 它买的是**权限位**：回环 TCP 上同机任何进程（含别的用户）都连得上，\
          Unix socket 有权限位而它没有，收窄只能靠一个 token；而 **backend 只读铁律不许它自己写文件**\
          ⇒ token 只能由宿主生成、当 env 传进去。**这一格是一条真裁决，不是实现细节。**\
          幂等：已存在就读回（重写会让上一个宿主留下的那个后端当场变成接不上的孤儿）"),
    ("local_backend_host.rs", "write_listen_pid", None,
     "写 `~/.cc-monitor/listen-<port>.pid` —— 「谁在听那个口」。\
          它**不是**真相源（真相源永远是「那个口连不连得上」），只在**停**那一步用，\
          且用之前还要过一道 `/proc/<pid>/exe` 的身份核对。\
          没有它，接管来的那个实例按不动「停」——那时按钮就成了一句骗人的话"),
    // ── 既不是安装、也不是「monitor 自己的」：**删用户数据**
    ("history.rs", "delete_history_session", None,
     "★ 删的是用户 `~/.claude/projects/**` 下的会话文件（用户主动发起）。\
          它不是安装动作，但也不是 monitor 自己的东西。⚠ **08-07 订正**：本行原写「围栏由 \
          `validate_delete_target` 与它自己的判据守着」—— 那句只对一半：围栏函数有五条穿越 \
          防护判据，但**没有任何东西钉住那条路真的过了围栏**（实测跳过围栏，全仓 978 条不红）。 \
          现由 `history_tests.rs::the_delete_entry_point_actually_goes_through_the_fence` 端到端钉住"),
];

/// 一行 `use ... fs ...` 该放行还是该拦。`Ok(())` = 放行。
///
/// # 为什么要有这道
///
/// 本模块的人群靠 **`fs::` 这个前缀**认写盘调用（`WRITE_CALLS` 每一条都带它），
/// `hooks_diag::this_module_never_writes` 的白名单也是扫 `fs::`。
/// 两条判据因此**共享同一个前提**：`std::fs` 只能以带前缀的形态出现。
///
/// 08-07 实测这个前提没人守：往 `hooks_diag.rs` 里加
/// `use std::fs as sysio;` + `sysio::write(p, s)`（一个名叫
/// `this_module_never_writes` 的模块里真写一次盘），全仓 **974 条判据一条不红** ——
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

/// ★★ **没有一种导入形态能让写盘调用丢掉 `fs::` 前缀**〔audit-0805 08-07〕。
///
/// 这是上面那条人群、以及 `hooks_diag::this_module_never_writes` 白名单的**共同前提**。
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
             · `hooks_diag::this_module_never_writes` 的白名单（它扫的就是 `fs::`）。\n\
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

/// 语料 = `src/` 整棵树 **+ `build.rs`**〔audit-0805 08-08〕。
///
/// ★ 为什么非把 `build.rs` 并进来：本表问的是「**谁能碰这台机器**」，
/// 而构建脚本每次 `cargo build`／`cargo check` 都在开发者机器上真跑
/// （它起 `sh` 与 `git`、往 `OUT_DIR` 复制内嵌后端）。
/// 08-08 实测：全仓所有登记表/守卫的扫描根都是 `src/bridge/src` · `src/backend`
/// · `src/bridge/crates` · `src` · `doc` —— **`src/bridge/build.rs` 一张表都没扫到**，
/// 它是这些扫描面共同的盲点（与 F65「三张表共享同一个没写下来的前提」同族）。
fn corpus() -> Vec<(PathBuf, String)> {
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
             ⚠ `ROADMAP §5 4b` 记的正是这个缺口：`tool_registry` 只守声明表自洽，\n\
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
    let table = guard_core::production_code(include_str!("../../src/bridge/src/tool_registry.rs"));
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
    assert!(
        checked >= 3,
        "申报表里只有 {checked} 条「安装动作」（08-07 实测 5）—— \
             要么真收口了（那很好，把这个数调下来），要么有人把它们改成了 `None` 绕过对拍。"
    );
}
