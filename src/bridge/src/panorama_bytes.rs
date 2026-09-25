//! 〔RM1c · 第四波〕**全景小程序的字节从哪来** —— 按那台机器的 (OS, arch) 选内嵌的那一份。
//!
//! 只装代码全景引擎的独立小程序 `cc-monitor-panorama`（`src/panorama-engine`，用户 09-24 V108 选 B）
//! 随后端部署、**只传给开过远端全景的机器**。字节从哪来：`build.rs::embed_panoramas`
//! 把两个 musl arch 的字节放进 `OUT_DIR`、置 `embedded_panoramas` cfg；〔DP1〕`include_bytes!` 它们的那两槽住 `byte_table.rs`。
//!
//! # 〔RM1e〕推上去（[`push_to`]）
//!
//! 〔墓碑 —— RM1c 那一版这里写着「推上去不在这里」：F08 部署路那时正被 SR1b 搬进本机常驻后端，
//!  本模块零生产调用方。SR1b 已合（部署经 `dial_host::RemoteFs`）。〕
//! 触发点在 `panorama_call.rs`：远端 `panorama` 回「没装 / 太旧」才推（V108「只传给开过远端全景的机器」），
//! 不随后端部署顺手推。推法与 F08 部署后端同一条路：`uname -s -m` 选字节 → 本机常驻后端那条 `files` 链路
//! （写只许 `~/.cc-monitor/bin/` 与暂存区）→ 建目录 → 原子上传 ＋ 后端读回逐字节比对（`sftp::upload_verified`）。
//! 落点 == 后端 `control/panorama.rs::fixed_candidates` 的第二个候选（判据读后端源码对拍）。
//!
//! # 为什么按 (OS, arch) 而不是只按 arch
//!
//! 〔墓碑 —— RM1c 那一版这里写着「后端那两份按 arch 选（`sftp.rs` 里那个只认 arch 的函数），因为远端今天只有 Linux」。〕
//! 〔DP1〕今天后端与全景小程序都经 `byte_table` 按 (OS, arch) 取：远端是 Windows 的那天，这里答「没有」，
//! 而不是把一份 Linux ELF 推过去（`build.rs::embed_native_backend` 头注那条真机读数就是这个形状）。

use crate::copy_table::copy_text;

/// 〔RM1f〕本机那一份在盘上的文件名：[`PROGRAM_NAME`] ＋ **目标平台**的可执行后缀
/// （`build.rs` 按 `TARGET` 算好的 `CCM_TARGET_EXE_SUFFIX`，同本机后端释放名那条来路）
/// == 本机后端 `control/panorama.rs::program_file_name()`（后端就跑在这台上，它的后缀就是这台的；判据对拍）。
pub(crate) fn local_file_name() -> String {
    format!("{PROGRAM_NAME}{}", env!("CCM_TARGET_EXE_SUFFIX"))
}

/// 〔RM1f〕**本机**要放下来的那一份（本机不经推送：本机后端在 `~/.cc-monitor/bin/` 找它，`local_backend::place_local_panorama` 放下来）。
///
/// 〔DP1 · 第四波〕字节从 `byte_table` 按**这台机器自己**的 (OS, arch) 取（`设计/01 §6.7a` 规矩 4：本机只是「目标机器恰好是自己」）。
/// 〔墓碑 —— RM1f 那一版这里自己 `include_bytes!` 按 `TARGET` 内嵌的原生小程序（`build.rs::embed_native_panorama`），
///  没有就退到本机是 Linux 时 musl 那两份里对得上 arch 的一份；那一槽搬进了 `byte_table.rs`，次序原样（原生先、musl 后，见那边 `pick`）。〕
pub(crate) fn local_panorama_binary() -> Option<&'static [u8]> {
    let key = crate::byte_table::Key::this_machine().ok()?;
    crate::byte_table::pick(crate::byte_table::Product::Panorama, key).map(|p| p.bytes)
}

/// 按那台机器的 `uname -s` / `uname -m`（原样）选字节。**只认得出、且这一版带着的组合才给**，其余 `None`。
///
/// 〔DP1 · 第四波〕只改函数体、签名不动：字节与 (OS, arch) → 键的解析都住 `byte_table`（全仓唯一的取字节口，
/// `设计/96 §7.1.1b`）。〔墓碑 —— 从前这里自己 `include_bytes!` 两份 musl，并经一个只认 Linux 的 arch 归一函数选。〕
pub(crate) fn panorama_binary(os: &str, arch: &str) -> Option<&'static [u8]> {
    let key = crate::byte_table::key_of(os, arch).ok()?;
    crate::byte_table::pick(crate::byte_table::Product::Panorama, key).map(|p| p.bytes)
}

/// 〔RM1e〕推到 home 底下哪个目录（相对段）。== 后端 `exit_policy::DIR_NAME` ＋ `bin`，
/// 也在 SR1b 那两个远端写根之内（判据读两处后端源码）。
pub(crate) const PUSH_DIR: &str = ".cc-monitor/bin";

/// 〔RM1e〕盘上那个可执行文件的名字 == 后端 `control/panorama.rs::PLUGIN_NAME`（判据读后端源码）。
pub(crate) const PROGRAM_NAME: &str = "cc-monitor-panorama";

/// 〔RM1e〕推上去的权限位：后端经插件口 exec 它（同一个用户）。
pub(crate) const PUSH_MODE: u32 = 0o755;

/// 〔RM1e〕问那台机器是什么 OS / arch 的那条命令（一次性 exec，`exec_site_registry` 登记）。
const UNAME_CMD: &str = "uname -s -m";

/// `(目录, 文件)`：推到 `<home>/.cc-monitor/bin/cc-monitor-panorama`。**纯函数**。
pub(crate) fn push_target(home: &str) -> (String, String) {
    let dir = format!("{}/{PUSH_DIR}", home.trim_end_matches('/'));
    let file = format!("{dir}/{PROGRAM_NAME}");
    (dir, file)
}

/// `uname -s -m` 的一行输出 → `(os, arch)`。恰两段才认（空 / 一段 / 多段 ⇒ 说清楚）。**纯函数**。
pub(crate) fn parse_uname(out: &str) -> Result<(String, String), String> {
    let parts: Vec<&str> = out.split_whitespace().collect();
    match parts.as_slice() {
        [os, arch] => Ok(((*os).to_string(), (*arch).to_string())),
        _ => Err(copy_text(
            "rsPanoramaBytes.uname.badShape",
            &[("out", &format!("{:?}", out.trim()))],
        )),
    }
}

/// 问那台机器的 `(os, arch)`：一次性 exec **收全**（stdout / stderr 各有上限、带退出码 ——
/// Windows 远端没有 `uname` 时退出码非零，那句 stderr 原样带回）。
async fn probe_uname(cfg: &crate::ssh_source::RemoteConfig) -> Result<(String, String), String> {
    let got = crate::ssh_source::connect_and_exec_capture(cfg, UNAME_CMD, None).await?;
    if got.exit_status != Some(0) {
        return Err(copy_text(
            "rsPanoramaBytes.uname.failed",
            &[
                ("code", &format!("{:?}", got.exit_status)),
                ("detail", &(got.stderr.trim()).to_string()),
            ],
        ));
    }
    parse_uname(&got.stdout)
}

/// 〔RM1e〕把这一版的全景小程序推到 `origin` 那台机器上（头注「推上去」）。
///
/// 那台的 (OS, arch) 没有内嵌字节 ⇒ 如实说、一个字节不推。
/// 〔RM1f〕本机那一台不经 SSH：[`place_local`] 把 [`local_panorama_binary`] 放进 `~/.cc-monitor/bin/`（本机后端的第二个候选）。
/// 〔墓碑 —— RM1e 那一版本机这一臂直接拒：「本机的代码全景组件不经推送 —— 它随本机后端一起放在本机后端旁边」。〕
pub(crate) async fn push_to(origin: &crate::origin::Origin) -> Result<(), String> {
    if origin.as_wire_str() == crate::backend::control::inbound_client::LOCAL_ORIGIN {
        return tokio::task::spawn_blocking(place_local)
            .await
            .map_err(|e| copy_text("rsPanoramaBytes.push.taskFailed", &[("e", &e.to_string())]))?;
    }
    let label = origin.as_wire_str();
    let cfg = crate::load_remote_config_by_label(label).ok_or_else(|| {
        copy_text(
            "rsPanoramaBytes.push.noConfig",
            &[("label", &label.to_string())],
        )
    })?;
    let (os, arch) = probe_uname(&cfg).await?;
    let bytes = panorama_binary(&os, &arch).ok_or_else(|| {
        copy_text(
            "rsPanoramaBytes.push.noBuild",
            &[("os", &os.to_string()), ("arch", &arch.to_string())],
        )
    })?;
    let fs = crate::dial_host::RemoteFs::open(&cfg).await?;
    let (dir, file) = push_target(fs.home());
    fs.mkdirs(&dir).await?;
    crate::sftp::upload_verified(&fs, &file, bytes, PUSH_MODE).await?;
    tracing::info!(
        "[{label}] 代码全景组件已推到 {file}（{os} / {arch}，{} 字节）",
        bytes.len()
    );
    Ok(())
}

/// 〔RM1f〕本机那一台：把这一份产物带着的小程序放到 `~/.cc-monitor/bin/<local_file_name>`。
///
/// # 为什么是「缺 / 旧时才放」，不是「随本机后端释放一起放」
///
/// 本机后端有两条来路：monitor 这一趟**起**它（走 `local_backend::resolve_or_extract`）、或者**接上**一个已经在跑的
/// 常驻后端（`local_backend_host::start_detached` 的 adopt 那一臂 —— 那一臂**根本不找二进制**）。只在前一条路上放，
/// 接上旧常驻后端的那一趟就永远没有小程序。⇒ 与远端同一个触发点：本机后端答「没装 / 装的太旧」时放一次、再问一次
/// （`panorama_call.rs::ask_or_push`）。**写的那一下住 `local_backend.rs`**（与释放本机后端同一套：暂存旁名 → 可执行位 → 换名）。
fn place_local() -> Result<(), String> {
    let bytes =
        local_panorama_binary().ok_or_else(|| copy_text("rsPanoramaBytes.local.noBuild", &[]))?;
    // 与推到远端同一个落点（[`PUSH_DIR`]，判据对拍后端的第二个候选）。
    let dir = dirs::home_dir()
        .map(|h| PUSH_DIR.split('/').fold(h, |p, seg| p.join(seg)))
        .ok_or_else(|| copy_text("rsPanoramaBytes.local.noHome", &[]))?;
    let placed = crate::backend::control::local_backend::place_local_panorama(
        &dir,
        &local_file_name(),
        bytes,
        &crate::platform_fs::make_executable,
    )?;
    tracing::info!(
        "本机代码全景组件已放到 {}（{} 字节）",
        placed.display(),
        bytes.len()
    );
    Ok(())
}

#[cfg(test)]
#[path = "../../../tests/bridge/panorama_bytes_tests.rs"]
mod tests;
