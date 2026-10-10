use std::path::Path;

fn main() {
    emit_target_triple();
    // 每一份真内嵌进来的后端字节自报的 id（哪一份 · id）。
    let mut carried: Vec<(String, String)> = Vec::new();
    embed_backends(&mut carried);
    embed_native_backend(&mut carried);
    emit_embedded_id(&carried);
    embed_native_filewin();
    manifest_for_every_artifact();
    // 清单不交给 Tauri 编（它那份资源只进 `[[bin]]`）；图标与版本信息仍由它编。出错照 `tauri_build::build()` 原样说、退出。
    let tauri_attrs = tauri_build::Attributes::new()
        .windows_attributes(tauri_build::WindowsAttributes::new_without_app_manifest());
    if let Err(e) = tauri_build::try_build(tauri_attrs) {
        println!("{e:#}");
        std::process::exit(1);
    }
}

/// Windows 进程清单（`windows/app.manifest`）编成资源、交给本包的**每一个**链接产物 —— 两个程序，**也包括测试程序**。
///
/// Tauri 自己编的那份资源只链进 `[[bin]]`（`embed-resource` 的 `rustc-link-arg-bins`），lib 的测试程序于是没有清单；
/// 而 Tauri 的 Windows 运行时按名字引入 comctl32 的 `TaskDialogIndirect` · `SetWindowSubclass` 一族（只有通用控件 v6 按名字导出），
/// 测试一碰到它（`chan::host::plan_open` 经 `raise_main` 拉主窗那一下，测试走生产句柄 `InboundBackends` 就够得到），
/// 链接器就把它留进导入表，那份测试程序在 Windows 上第一行代码之前就死（`0xc0000139`）。清单只有这一份，程序与测试程序同一份。
/// 判据：`tests/frontend/shell/lib_app_manifest_tests.rs`（测试程序在 Windows 上读回自己的清单）。
fn manifest_for_every_artifact() {
    let dir =
        std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let manifest = dir.join("windows").join("app.manifest");
    println!("cargo:rerun-if-changed={}", manifest.display());
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let out = std::path::PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR"));
    let rc = out.join("app-manifest.rc");
    // `1` ＝ CREATEPROCESS_MANIFEST_RESOURCE_ID，`24` ＝ RT_MANIFEST。rc 的字符串里反斜杠要成对。
    let path = manifest.display().to_string().replace('\\', "\\\\");
    std::fs::write(&rc, format!("1 24 \"{path}\"\n")).expect("写不出清单的 .rc");
    if let Err(e) =
        embed_resource::compile_for_everything(&rc, embed_resource::NONE).manifest_required()
    {
        println!("cargo:warning=清单没编成资源：{e}");
        std::process::exit(1);
    }
}

/// 🔴 **本文件不读后端源码。** monitor 的「我这一版」只有一个值：手上那份内嵌后端字节自报的 id
/// （[`emit_embedded_id`] 交出去，`byte_table::my_backend_id` 读）。身份戳界标与扫法住契约 crate
/// `deploy-contract`（`[build-dependencies]`）；「字节 id == 后端源码 `BUILD_ID`」那条半 bump 核对
/// 住发版那一侧（`release.yml` 的内嵌校验 · [`REEMBED_CMD`] 的 `--check`）—— 那里读源码是对的，它就是判据。
/// 开发中「源码 bump 了、字节还没重铺」不让 monitor 编不过：monitor 按手上那份旧字节说话。
///
/// 出事之后怎么办只有这一个住址：一条命令（`tests/scripts/re-embed.sh`），配方与 `release.yml` 的
/// `Cross-compile backend for both musl targets` 逐字同源（`tests/evidence/K-R124-ruler.py` ⑬b 每趟两向对拍）；
/// 本机重编为什么必须用 zigbuild、不能换回 `rust-lld`，读数写在那条命令的头注里。
const REEMBED_CMD: &str = "bash tests/scripts/re-embed.sh";

/// 远端那两份 musl 字节的落点目录名。与 [`NATIVE_BACKEND_DIR`] 对称，两个都是
/// **机检过的落点**：`tests/evidence/K-R124-ruler.py` ⑬d 把这两个常量与
/// `src/frontend/shell/.gitignore` 里带 `⇐ 内嵌落点` 锚的那几行**两向集合相等**。
const EMBEDDED_BACKENDS_DIR: &str = "embedded-backends";

/// 本机后端的文件名是 `<stem>-<target-triple>`（Tauri `externalBin` 的规矩），而 std 里没有
/// 「当前 target triple」这个常量 —— 只有 build script 拿得到 `TARGET`。
fn emit_target_triple() {
    println!(
        "cargo:rustc-env=CCM_TARGET_TRIPLE={}",
        std::env::var("TARGET").unwrap_or_else(|_| "unknown-target".into())
    );
    // 本构建脚本自己的路径：判据（`byte_table_tests`）拿铺好的假字节真跑这一份，看「几份内嵌字节不同版」时它是否当场失败。
    if let Ok(me) = std::env::current_exe() {
        println!("cargo:rustc-env=CCM_BUILD_SCRIPT_EXE={}", me.display());
    }
}

/// 🔴 `K-R70`：**读一份二进制并问它是谁** —— 不看它旁边任何文件。
///
/// 扫法只有一份：`deploy_contract::identity_of_bytes`（界标 `STAMP_OPEN` / `STAMP_CLOSE`，恰好一个戳才是身份；
/// 运行期推字节之前的见证、后端看落点那一份都是它）。后端那段 `CC_MONITOR_BUILD_STAMP` 是一个 `#[used]` 的
/// `static [u8; N]`：有地址、进 `.rodata`、字节按定义连续，拆不成立即数 ⇒ 扫字节是结构性成立的事，不是启发式。
/// 读不到 / 0 个 / 多个都给 `None`（调用方各自决定怎么说）。
fn file_build_id(p: &Path) -> Option<String> {
    let bytes = std::fs::read(p).ok()?;
    match deploy_contract::identity_of_bytes(&bytes, STAMP_MARKS) {
        deploy_contract::RemoteIdentity::Stamp(id) => Some(id),
        _ => None,
    }
}

/// 身份戳的两个界标（契约 crate 那一对）。
const STAMP_MARKS: deploy_contract::Marks<'static> = deploy_contract::Marks {
    open: deploy_contract::STAMP_OPEN,
    close: deploy_contract::STAMP_CLOSE,
};

/// **「我这一版」的唯一来源**：几份内嵌字节自报的 id 必须彼此相等，那个共同的 id 以 `BACKEND_EMBEDDED_ID`
/// 交给 monitor（`option_env!` 读）。一份都没内嵌 ⇒ 不发这个 env（monitor 那侧是 `None`：手上没有可放的字节，
/// 不判那台旧、不发起换装）。
///
/// 不等 ⇒ 构建当场失败：同一个 exe 里带着两版后端，「我这一版」就不是一个值 —— 推到远端的是一版、
/// 本机自释放的是另一版，版本判定按哪个比都有一侧是错的。
fn emit_embedded_id(carried: &[(String, String)]) {
    let Some((_, first)) = carried.first() else {
        return;
    };
    if carried.iter().any(|(_, id)| id != first) {
        let listed: Vec<String> = carried
            .iter()
            .map(|(what, id)| format!("{what} = `{id}`"))
            .collect();
        panic!(
            "内嵌的几份后端字节不是同一版：{}。\n\
             同一个 exe 里带着两版后端 ⇒ 「我这一版」不是一个值（推到远端的与本机自释放的各说各的）。\n\
             出路二选一，**两条都是同一条命令**：\n\
             ① `{REEMBED_CMD}`（两个 musl arch）／`{REEMBED_CMD} --native`（本机那一份）—— 从同一份源码重编重铺；\n\
             ② `{REEMBED_CMD} --clean` —— 删掉落点，自动部署与自释放诚实关闭，编译立刻恢复。",
            listed.join(" · ")
        );
    }
    println!("cargo:rustc-env=BACKEND_EMBEDDED_ID={first}");
}

/// 把交叉编译好的 musl backend 二进制
/// （`src/frontend/shell/embedded-backends/cc-monitor-backend-<arch>`）复制进 OUT_DIR 并置
/// `embedded_backends` cfg；任一缺失则不置 cfg（`byte_table::pick` 对 Linux 两格返回 None → 部署那一步说「这一版没带」）。
/// 两份都在 ⇒ 各自自报的 id 记进 `carried`（[`emit_embedded_id`] 核它们与本机那一份同一版）。
///
/// 🔴 〔步 `19c`〕**这两份字节从哪来，只有两个地方**：发版那趟是 `release.yml` 的
/// `Cross-compile backend for both musl targets`，本机那趟是 [`REEMBED_CMD`]，
/// 两者**逐字同一条配方**（`K-R124` ⑬b 两向对拍）。
fn embed_backends(carried: &mut Vec<(String, String)>) {
    // 允许自定义 cfg（Rust 1.80+ unexpected_cfgs 检查）。
    println!("cargo:rustc-check-cfg=cfg(embedded_backends)");
    let out = std::env::var("OUT_DIR").expect("OUT_DIR");
    let dir = Path::new(EMBEDDED_BACKENDS_DIR);
    let mut found: Vec<(String, String)> = Vec::new();
    for arch in ["x86_64", "aarch64"] {
        let src = dir.join(format!("cc-monitor-backend-{arch}"));
        println!("cargo:rerun-if-changed={}", src.display());
        if !src.exists() {
            // U-1：缺二进制就静默不置 cfg 正是 v2.19–v2.22 那批安装包的事故形状 ⇒ 至少说一句。
            println!(
                "cargo:warning=缺少内嵌 backend {arch}（{}）——远端自动部署将关闭（embedded_backends cfg 不置）。要它就跑 `{REEMBED_CMD}`",
                src.display()
            );
            continue;
        }
        // 🔴 `K-R70`：身份从这份字节自己里读，旁边任何文件都没有话语权。
        // 问不出身份不能只是 warning —— 有人手工塞了个陈旧 / 别处来的二进制，恰好就是这一形。
        let Some(embedded_id) = file_build_id(&src) else {
            panic!(
                "内嵌 backend {arch}（src/frontend/shell/{}）**问不出身份** —— \
                 在它的字节里找不到恰好一个 `{}…{}` 身份戳。\n\
                 可能是：① 它不是这套源码编出来的（太旧 —— `p2f-build-stamp` 之前的\
                 backend 根本没有戳）；② 它被改过 / 截断了。\n\
                 🔴 **别去写一个 `.build_id` 旁文件来糊它** —— 没有任何东西读那个文件，写一份只是把标签换个地方抄。\n\
                 出路二选一，**两条都是同一条命令**（步 `19c`）：\n\
                 ① `{REEMBED_CMD}` —— 重编两个 musl arch 并铺回落点；\n\
                 ② `{REEMBED_CMD} --clean` —— 删掉落点，自动部署诚实关闭，编译立刻恢复。",
                src.display(),
                deploy_contract::STAMP_OPEN,
                deploy_contract::STAMP_CLOSE,
            );
        };
        let dst = Path::new(&out).join(format!("backend-{arch}"));
        std::fs::copy(&src, &dst).expect("copy embedded backend binary");
        found.push((
            format!("embedded-backends/cc-monitor-backend-{arch}"),
            embedded_id,
        ));
    }
    // 两份齐了才置 cfg（`byte_table` 那两槽同进同出）；只铺了一份的那份不进 exe，也就不算「手上的字节」。
    if found.len() == 2 {
        println!("cargo:rustc-cfg=embedded_backends");
        carried.extend(found);
    }
}

/// 目标平台的可执行后缀。
///
/// 🔴 **由 `TARGET` 算，不许用 `std::env::consts::EXE_SUFFIX`** —— build script 跑在
/// **构建机**上（交叉编译 Windows 产物时那台是 Linux）⇒ 那个常量给的是**宿主**的后缀，
/// 而这里要的是**目标**的。同一个坑 `emit_target_triple` 里那条 `CCM_TARGET_TRIPLE`
/// 已经踩过一次（「std 里没有『当前 target triple』这个常量，只有 build script 拿得到 `TARGET`」）。
fn target_exe_suffix(target: &str) -> &'static str {
    if target.contains("windows") {
        ".exe"
    } else {
        ""
    }
}

/// 本机内嵌后端的落点 —— `release.yml` 那一步按同一条路径铺，
/// `byte_table.rs`（全仓唯一的取字节口）用**同一条路径的字面量** `include_bytes!` 它。
///
/// # 🔴 为什么是**定死的名字**，而不是像 `embedded-backends/` 那样把 triple 编进文件名
///
/// 因为消费侧那个 `include_bytes!` 的参数**必须是字面量**：
/// `cross_half_edge_registry::every_non_literal_include_is_registered_with_a_reason` 默认拒绝
/// 非字面量的 `include_*!`，而它的登记表**不在本件写区**。
/// ⇒ 名字定死、**用一个旁挂清单把 triple 记下来再核**（`.target`，下面那条硬校验）。
/// 这一换其实更强：文件名是**没人核的约定**，清单是**每次构建都核的断言**。
///
/// # 🔴 为什么不铺进已有的 `embedded-backends/`
///
/// 那个目录名是 `local_backend_host_tests.rs::every_test_that_starts_the_real_backend_demands_a_private_tmux`
/// 认「谁会起真后端」的**来历串之一**。字面量路径写进 `local_backend.rs` 的生产段，
/// 会把整段生产代码拖进那条判据的人群（实测：人群里多出一条 `local_backend.rs::default`，
/// 而那条连测试都不是；同时 `the_local_backend_host_really_registers_an_inbound_client`
/// 因为切块规则一起掉出人群，那条判据的地板当场红）。
/// ⇒ 换一个目录，两条判据都不被误伤，而且它本来也不是同一类东西
/// （那边是**远端部署产物**，这边是**本机原生**）。
/// ✅ **那笔欠账 09-10 当天就还了**：`src/frontend/shell/.gitignore` 里已经有 `/native-backend/`。
/// 〔墓碑 —— 本行原话逐字：「⚠ **一条如实记着的欠账**：`src/frontend/shell/.gitignore` 里还没有这一行，
///  而它**不在本件写区** —— 见 `K-R42` 件文件的上报口。」它写下时是真的，
///  补上那一行的那一拍（`track/alarm`）当场把它变成了假话，**而那一拍也点名了它**。〕
/// ★ 记这一笔的形状：**造了债的那一拍自己报出来，并分清它是新债还是存量** ——
/// 这条正是**它自己造的新债**，不是从别处继承来的。
const NATIVE_BACKEND_DIR: &str = "native-backend";
const NATIVE_BACKEND_FILE: &str = "cc-monitor-native";

/// `K-R42`：**把「本机后端」也内嵌进 exe**，让裸 `cc-monitor.exe` 自己带得上一份。
///
/// # 它与 `embed_backends` 是两件事，别合并
///
/// | | `embed_backends`（远端那条） | 本函数（本机这条） |
/// |---|---|---|
/// | 内嵌什么 | `cc-monitor-backend-{x86_64,aarch64}`，**musl Linux**，按 **arch** 分派 | `cc-monitor-native-<target triple>`，**当前 TARGET 的原生二进制** |
/// | 给谁用 | SFTP 推到远端主机（远端就是 Linux ⇒ musl 是对的） | 本机自释放（`local_backend::start_or_extract`） |
/// | 认不认 OS | **不认**（只看 arch） | **由 TARGET 定死**，编译期就选好了 |
///
/// 🔴 **「不认 OS」正是 09-10 那个真机读数的根因**：`local_backend_host.rs` 里那道
/// `if cfg!(target_os = "linux")` 的闸（D 阶段补审 08-11 加的）挡的就是
/// 「往 Windows 上释放一个 Linux ELF、然后报告『已起』」——
/// 那道闸**是对的**，它挡住的是**没有 Windows 版可嵌**这件事，不是「自释放这条路不该走」。
/// ⇒ 本函数补的就是那个缺口：**给 Windows 一份能跑的字节**。
/// 缺口本身一年前就登记过（逐字：「要 release 流程产
/// Windows backend 并内嵌」）—— 今天才第一次有人在真机上撞到它。
///
/// # 它也是「我这一版」的一份
///
/// 内嵌了它 ⇒ 它自报的 id 记进 `carried`，[`emit_embedded_id`] 要求它与远端那两份同一版（不等当场失败）。
///
/// # 缺席 = 不置 cfg + **可见的** warning
///
/// 沿用 `embed_backends` 的 U-1 那条账：**静默不置 cfg 正是 v2.19–v2.22 那批安装包的事故形状**。
/// 缺席时 `byte_table::pick` 在这一格给不出本机原生那份（Linux 构建上还有 musl 那份可给），自释放那条路说「这一版没带」。
fn embed_native_backend(carried: &mut Vec<(String, String)>) {
    // 允许自定义 cfg（Rust 1.80+ unexpected_cfgs 检查）。
    println!("cargo:rustc-check-cfg=cfg(embedded_native_backend)");
    let target = std::env::var("TARGET").unwrap_or_else(|_| "unknown-target".into());
    // ⚠ **无条件 emit**：`local_backend::local_ccm_entry_name` 是**无条件**的生产代码，
    // 它 `env!` 这个名字 —— 只在某些分支 emit 会让别的分支编不过。
    println!(
        "cargo:rustc-env=CCM_TARGET_EXE_SUFFIX={}",
        target_exe_suffix(&target)
    );

    let dir = Path::new(NATIVE_BACKEND_DIR);
    let src = dir.join(NATIVE_BACKEND_FILE);
    let target_manifest = dir.join(format!("{NATIVE_BACKEND_FILE}.target"));
    for f in [&src, &target_manifest] {
        println!("cargo:rerun-if-changed={}", f.display());
    }
    // 🔴**缺席不再被说成「开发构建里的正常情况」。**
    //
    // 〔墓碑，本条 warning 原话逐字：「没有本机内嵌后端 ——**裸可执行文件起不了本机后端**，
    //  只有安装包那份带本机后端的能起。开发构建里这是正常的；……」。**后半句违反 `D11`**
    //  （用户逐字「不要退路 / 所有东西都不要假设后端没起来」）：它把「开发构建上窗口
    //  『走后端』那条主路一直在走退路」写成了常态，于是本机上验证任何后端功能都先天带洞。〕
    //
    // ⇒ 修在**构建这一侧**，运行期一个分支都没加：开发构建的路径是明写的、而且与发版
    //   **同一条** —— 那条命令的 `--native` 编出本机原生后端铺进本目录，本函数照常内嵌，
    //   运行期按原路自释放再起。「起不起得来」由它的 `--check-dev` **真起一趟**来判
    //   （读那份字节 stdout 的第一行，必须是一帧自报源码 `BUILD_ID` 的 hello；缺席即红）。
    // ⚠ 这里**仍然是 warning 不是 panic**，理由是硬的、不是手软：门禁与 CI 上每一格
    //   `cargo check/test/clippy` 都在**没铺字节**的树上跑（字节不进仓），panic 会把它们全拖红，
    //   而那几处不在本件写区。**判词住那条命令上，不住这句 warning 上。**
    if !src.exists() {
        println!(
            "cargo:warning=没有本机内嵌后端（src/frontend/shell/{}）⇒ **这一趟编出来的可执行文件起不了本机后端**\
             （D11：这不是开发构建的正常情况）。开发构建要它：`{REEMBED_CMD} --native`；\
             判它起不起得来：`{REEMBED_CMD} --check-dev`。\
             发版构建里出现这一行 = 那一版的裸 exe 又回到 09-10 那个读数（0 个本机后端进程）。",
            src.display()
        );
        return;
    }
    // ── ① 它是**给这个 target 编的**吗 ─────────────────────────────────────
    //
    // 名字定死（理由见 `NATIVE_BACKEND_DIR` 头注）⇒ 「这份字节属于哪个平台」**只能靠这个清单**。
    // 放它过去等于把一个别的平台的二进制内嵌进来，而那正是 08-11 补审逮到的那个阻塞级缺陷
    // （往 Windows 上释放 Linux ELF 再报「已起」）。
    // 清单写着别的 target ⇒ 这份是给那个 target 铺的（例：铺着本机 linux 那份去编 Windows 的测试程序），
    //   这一趟当它不在：不内嵌、不置 cfg，照缺席那样喊一句 warning。清单缺席 / 空 ⇒ 说不出给谁编的，当场失败。
    let staged_target = read_trimmed(&target_manifest);
    if !staged_target.is_empty() && staged_target != target {
        println!(
            "cargo:warning=本机内嵌后端（src/frontend/shell/{}）是给 `{staged_target}` 编的，这一趟的 TARGET 是 `{target}` \
             ⇒ 这一趟不内嵌它（编出来的可执行文件起不了本机后端）。要这个 target 的：`{REEMBED_CMD} --native`（在那个 target 上）。",
            src.display()
        );
        return;
    }
    if staged_target != target {
        panic!(
            "本机内嵌后端（src/frontend/shell/{}）是给 `{}` 编的，而这一趟的 TARGET 是 `{target}`。\n\
             （清单读作 `{staged_target}`；空串 = 根本没有 `{}.target` 这个文件。）\n\
             内嵌一个别的平台的二进制 = 释放到用户盘上再起，起不来 —— \
             而 `Resolved::Found` 会先撒一次谎（它只证明文件落地了）。\n\
             出路二选一，**两条都是同一条命令**（步 `19c`）：\
             ① `{REEMBED_CMD} --native` —— 为这一趟的 TARGET 重编并重铺那两个文件；\
             ② `{REEMBED_CMD} --clean` —— 删掉落点，自释放诚实关闭，编译立刻恢复。",
            src.display(),
            staged_target,
            NATIVE_BACKEND_FILE,
        );
    }
    // ── ② 它是谁：身份从这份字节自己里读（理由住 [`file_build_id`] 的头注）。
    //    ⚠ 旁边那个 `.target` 答的是「给哪个平台编的」，不是「你是谁」。
    let Some(embedded_id) = file_build_id(&src) else {
        panic!(
            "本机内嵌后端（src/frontend/shell/{}）**问不出身份** —— \
             在它的字节里找不到恰好一个 `{}…{}` 身份戳。\n\
             身份是它落到用户盘上时的文件名来源（`cc-monitor-backend-<build_id>`），\
             问不出就拼不出落点。\n\
             可能是：① 它不是这套源码编出来的（`p2f-build-stamp` 之前的后端没有戳）；\
             ② 被改过 / 截断。\n\
             🔴 **别去补一个 `.build_id` 旁文件** —— 没人读它，那只是把标签换个地方抄。\n\
             出路二选一，**两条都是同一条命令**（步 `19c`）：`{REEMBED_CMD} --native` 重编重铺，\
             或 `{REEMBED_CMD} --clean` 删掉落点（自释放诚实关闭，编译立刻恢复）。",
            src.display(),
            deploy_contract::STAMP_OPEN,
            deploy_contract::STAMP_CLOSE,
        );
    };
    carried.push((
        format!("{NATIVE_BACKEND_DIR}/{NATIVE_BACKEND_FILE}"),
        embedded_id,
    ));
    // ⚠ **不往 `OUT_DIR` 拷一份**（`embed_backends` 那条是拷的）——消费侧那个 `include_bytes!`
    // 直接按**字面量相对路径**读得到，而拷贝会让本函数变成一个**写盘落点**，
    // 那要在 `write_site_registry::WRITE_SITES` 里申报（`fs::copy(` 是它的 needle 之一），
    // 而那张表**不在本件写区**。少一次拷贝同时也少一条要申报的写点 —— 两头都更干净。
    println!("cargo:rustc-cfg=embedded_native_backend");
}

/// 文件窗口那份程序的文件名（落点目录同 [`NATIVE_BACKEND_DIR`]，旁挂 `.target` 清单同那两份）。
/// 与 `filewin::proc::BIN_STEM`（monitor 包那个 `[[bin]]` 的名字）是同一个词；`byte_table.rs` 用**同一条路径的字面量**
/// `include_bytes!` 它；`re-embed.sh --native` 与 `release.yml` 两个 job 按同一个名字铺（判据对拍）。
const NATIVE_FILEWIN_FILE: &str = "cc-monitor-filewin";

/// **把文件窗口那份程序也内嵌进 exe**：单文件的 monitor 自己带着它，开窗时旁边没有就放到 `~/.cc-monitor/bin/` 再起
/// （`filewin::proc::resolve_window_bin`）。
///
/// # 形状与 [`embed_native_backend`] 同一套
///
/// 定死名字（消费侧 `include_bytes!` 要字面量）· 旁挂 `.target` 清单（名字里没有 triple）· 清单写着别的 target ⇒ 这一趟当它不在、清单缺席 ⇒ 当场 panic
/// （内嵌一个别的平台的程序 = 放到用户盘上起不来）· 缺席 ⇒ 不置 cfg ＋ **可见的** warning。
/// ⚠ 没有身份戳：它是哪一版由字节本身答（放的时候逐字节比，见 `local_backend::place_local_program`）。
/// ⚠ **它不能在同一趟 cargo 里现编现嵌**：它是本包的另一个 `[[bin]]`，编它要先编本包的库（本函数就跑在那一步里）
/// ⇒ 先单编它、铺进落点，再编 monitor（`re-embed.sh --native` 与 `release.yml` 都是这个次序）。
/// 没铺时开窗只剩「exe 旁边」那一支（开发树 `target/<档>/` 里两个二进制本来就挨着，安装包也把它装在主程序旁）。
fn embed_native_filewin() {
    println!("cargo:rustc-check-cfg=cfg(embedded_native_filewin)");
    let target = std::env::var("TARGET").unwrap_or_else(|_| "unknown-target".into());
    let dir = Path::new(NATIVE_BACKEND_DIR);
    let src = dir.join(NATIVE_FILEWIN_FILE);
    let target_manifest = dir.join(format!("{NATIVE_FILEWIN_FILE}.target"));
    for f in [&src, &target_manifest] {
        println!("cargo:rerun-if-changed={}", f.display());
    }
    if !src.exists() {
        println!(
            "cargo:warning=没有内嵌文件窗口程序（src/frontend/shell/{}）⇒ 这一趟编出来的可执行文件只在旁边有 \
             `{NATIVE_FILEWIN_FILE}` 时开得了文件窗口（开发树的 target 目录里两个挨着）。要它：`{REEMBED_CMD} --native`。",
            src.display()
        );
        return;
    }
    let staged_target = read_trimmed(&target_manifest);
    // 清单写着别的 target ⇒ 当它不在（理由同 [`embed_native_backend`] 那一步）；清单缺席 / 空 ⇒ 当场失败。
    if !staged_target.is_empty() && staged_target != target {
        println!(
            "cargo:warning=内嵌文件窗口程序（src/frontend/shell/{}）是给 `{staged_target}` 编的，这一趟的 TARGET 是 `{target}` \
             ⇒ 这一趟不内嵌它（只在旁边有 `{NATIVE_FILEWIN_FILE}` 时开得了文件窗口）。",
            src.display()
        );
        return;
    }
    if staged_target != target {
        panic!(
            "内嵌文件窗口程序（src/frontend/shell/{}）是给 `{}` 编的，而这一趟的 TARGET 是 `{target}`。\n\
             （清单读作 `{staged_target}`；空串 = 根本没有 `{}.target` 这个文件。）\n\
             内嵌一个别的平台的程序 = 放到用户盘上起不来。\
             出路二选一，**两条都是同一条命令**：① `{REEMBED_CMD} --native` 为这一趟的 TARGET 重编重铺；\
             ② `{REEMBED_CMD} --clean` 删掉落点（只剩「exe 旁边」那一支，编译立刻恢复）。",
            src.display(),
            staged_target,
            NATIVE_FILEWIN_FILE,
        );
    }
    println!("cargo:rustc-cfg=embedded_native_filewin");
}

/// 读一份单值清单，读不到就给空串（「没有这个文件」与「文件是空的」在这里同义：都不可信）。
fn read_trimmed(p: &Path) -> String {
    std::fs::read_to_string(p)
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}
