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
//! 触发点在界面（〔MIG-3b 续〕`src/frontend/ui/panorama/api.ts::askOrPlace`，经 [`panorama_place`]）：那台 `panorama` 回「没装 / 太旧」才推（V108「只传给开过远端全景的机器」），
//! 不随后端部署顺手推。推法与 F08 部署后端同一条路：〔THIN〕「那台要哪一格」问本机常驻后端（帧命令 `deploy-slot`：它问 `uname`、
//! 按表 A / 表 B 判、看这一版带没带 —— 不承诺 / 这一版没带 ⇒ 写第一个字节之前就拒，那句话它说；`设计/00 §1.2` 判定只在后端）→
//! 按答里那一格从 `byte_table::pick` 取字节 → 本机常驻后端那条 `files` 链路
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

/// 〔THIN〕本机常驻后端那条命令的名字（与 `src/backend/stream/inbound.rs::REGISTRY` 同名）。
pub(crate) const SLOT_CMD: &str = "deploy-slot";

/// 问那一趟的上限：远端一次 `uname`（一两个往返），本机纯判定。
const SLOT_BUDGET: std::time::Duration = std::time::Duration::from_secs(60);

/// 〔THIN〕`deploy-slot` 的应答 → 那一格（**严格收**：恰 `{os, arch, label, ack}`，键认不出 ⇒ 两侧漂了）。
pub(crate) fn decode_slot(v: &serde_json::Value) -> Result<crate::byte_table::Key, String> {
    let bad = || copy_text("rsPanoramaBytes.slot.unreadable", &[]);
    let obj = v.as_object().ok_or_else(bad)?;
    let mut keys: Vec<&str> = obj.keys().map(String::as_str).collect();
    keys.sort_unstable();
    if keys != ["ack", "arch", "label", "os"] {
        return Err(bad());
    }
    let s = |k: &str| obj.get(k).and_then(serde_json::Value::as_str).unwrap_or("");
    deploy_core::key_of(s("os"), s("arch")).map_err(|_| bad())
}

/// 〔THIN〕问本机常驻后端「那台要哪一格全景字节」（`cfg` = 那台远端；`None` = 本机）。入参只有事实：怎么够到那台 · 这一版带着哪几格。
/// 拒绝的那句话是后端说的（`Refusal::say`，同一份文案），原样带回。
async fn ask_slot(
    cfg: Option<&crate::ssh_source::RemoteConfig>,
    machine: &str,
) -> Result<crate::byte_table::Key, String> {
    use crate::backend::control::backend_route::{route_call_error, Routed};
    use crate::byte_table::{carried, Product};
    let carried: Vec<serde_json::Value> = carried(Product::Panorama)
        .iter()
        .map(|k| serde_json::json!({ "os": k.os.label(), "arch": k.arch.label() }))
        .collect();
    let mut args = serde_json::json!({ "product": Product::Panorama.wire(), "machine": machine, "carried": carried });
    let dial = cfg.map(crate::dial_host::transfer_dial).transpose()?;
    if let Some(d) = &dial {
        args["dial"] = d.clone();
    }
    let client = crate::dial_host::local_backend_accepting(SLOT_CMD).await?;
    let data = client
        .call(SLOT_CMD, args, SLOT_BUDGET)
        .await
        .map_err(
            |e| match route_call_error(&e, |_code, message| message.to_string()) {
                Routed::NoChannel(s) | Routed::Refused(s) => s,
                Routed::Done => copy_text("rsPanoramaBytes.slot.unreadable", &[]),
            },
        )?
        .unwrap_or_default();
    let key = decode_slot(&data)?;
    // 第一次连一台没钉过指纹的机器就在这一跳 ⇒ 照 monitor 自己开链路那几条同一个判定固化（同部署计划那一条）。
    if let (Some(c), Some(d)) = (cfg, &dial) {
        let ack: crate::ssh_link::Ack = serde_json::from_value(data["ack"].clone())
            .map_err(|_| copy_text("rsPanoramaBytes.slot.unreadable", &[]))?;
        crate::dial_host::settle_host_key(c, d, &ack);
    }
    Ok(key)
}

/// 按后端答的那一格取这一版带着的全景字节。取不到 ⇒ 两侧漂了（后端只回 `carried` 里的格）。
fn slot_bytes(key: crate::byte_table::Key) -> Result<&'static [u8], String> {
    crate::byte_table::pick(crate::byte_table::Product::Panorama, key)
        .map(|p| p.bytes)
        .ok_or_else(|| copy_text("rsPanoramaBytes.slot.unreadable", &[]))
}

// 〔TL1 · 4C〕墓碑：这里从前有一个按「那台答的系统 / 架构两个词」直接取字节的函数（DP1 那一拍只改了函数体、委托 `byte_table`）。
//   远端推字节改走 `byte_table::choose` 之后它零生产调用方 ⇒ 删；两个词 → 键的解析只剩 `byte_table::key_of` 一处。

/// 〔RM1e〕推到 home 底下哪个目录（相对段）。== 后端 `exit_policy::DIR_NAME` ＋ `bin`，
/// 也在 SR1b 那两个远端写根之内（判据读两处后端源码）。
pub(crate) const PUSH_DIR: &str = ".cc-monitor/bin";

/// 〔RM1e〕盘上那个可执行文件的名字 == 后端 `control/panorama.rs::PLUGIN_NAME`（判据读后端源码）。
pub(crate) const PROGRAM_NAME: &str = "cc-monitor-panorama";

/// 〔RM1e〕推上去的权限位：后端经插件口 exec 它（同一个用户）。
pub(crate) const PUSH_MODE: u32 = 0o755;

/// `(目录, 文件)`：推到 `<home>/.cc-monitor/bin/cc-monitor-panorama`。**纯函数**。
pub(crate) fn push_target(home: &str) -> (String, String) {
    let dir = format!("{}/{PUSH_DIR}", home.trim_end_matches('/'));
    let file = format!("{dir}/{PROGRAM_NAME}");
    (dir, file)
}

// 〔TL1 · 4C〕墓碑：这里从前有**第二份** `uname -s -m`（问那台 + 按两段切），与 `byte_table::probe_key` 并存
//   （`DP1.md` 报备 7「两份 uname -s -m」）。今天只剩 `byte_table` 那一份。

/// 〔RM1e〕把这一版的全景小程序推到 `origin` 那台机器上（头注「推上去」）。
///
/// 那台的 (OS, arch) 在表 A / 表 B 上过不去（问不出 · 没有产线 · 不承诺 · 这一版没带）⇒ 那句拒绝的话、一个字节不推。
/// 〔RM1f〕本机那一台不经 SSH：[`place_local`] 把后端答的那一格（〔THIN〕`deploy-slot`，本机那一行）的字节放进 `~/.cc-monitor/bin/`（本机后端的第二个候选）。
/// 〔墓碑 —— RM1e 那一版本机这一臂直接拒：「本机的代码全景组件不经推送 —— 它随本机后端一起放在本机后端旁边」。〕
pub(crate) async fn push_to(origin: &crate::origin::Origin) -> Result<(), String> {
    if origin.as_wire_str() == crate::backend::control::inbound_client::LOCAL_ORIGIN {
        let key = ask_slot(None, &copy_text("rsPanoramaBytes.local.machine", &[])).await?;
        let bytes = slot_bytes(key)?;
        return tokio::task::spawn_blocking(move || place_local(bytes))
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
    // 〔THIN〕「那台要哪一格」问本机常驻后端（拒绝的话它说）；这里只按答取字节。
    let key = ask_slot(Some(&cfg), label).await?;
    let machine = key.label();
    let bytes = slot_bytes(key)?;
    let fs = crate::dial_host::RemoteFs::open(&cfg).await?;
    let (dir, file) = push_target(fs.home());
    fs.mkdirs(&dir).await?;
    crate::sftp::upload_verified(&fs, &file, bytes, PUSH_MODE).await?;
    tracing::info!(
        "[{label}] 代码全景组件已推到 {file}（{machine}，{} 字节）",
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
/// （〔MIG-3b 续〕界面 `askOrPlace` → [`panorama_place`]）。**写的那一下住 `local_backend.rs`**（与释放本机后端同一套：暂存旁名 → 可执行位 → 换名）。
fn place_local(bytes: &'static [u8]) -> Result<(), String> {
    // 与推到远端同一个落点（[`PUSH_DIR`]，判据对拍后端的第二个候选）。
    let dir = dirs::home_dir()
        .map(|h| PUSH_DIR.split('/').fold(h, |p, seg| p.join(seg)))
        .ok_or_else(|| copy_text("rsPanoramaBytes.local.noHome", &[]))?;
    let placed = crate::backend::control::local_backend::place_local_panorama(
        &dir,
        &local_file_name(),
        bytes,
        &crate::platform::fs::make_executable,
        &crate::platform::fs::ensure_private_dir,
    )?;
    tracing::info!(
        "本机代码全景组件已放到 {}（{} 字节）",
        placed.display(),
        bytes.len()
    );
    Ok(())
}

// ═══ 〔MIG-3b 续 · 主会话 09-28 裁〕放字节那一条命令 ═══════════════════════════════════════════
//
// 界面直问那台后端 `panorama` / `panorama-edit`；那台回「没装 / 太旧」（`not_installed` / `unsupported`）⇒ 界面请这里**放字节**、
// 再问一次（`src/frontend/ui/panorama/api.ts::askOrPlace`）。原 `panorama_call.rs`〔散文墓碑〕那一跳（问 · 转 · 撤）整份删了，放字节那一半搬来这里：
// 每台一把锁 · 真要放之前在远端健康通道上说一声 · 本机远端同一个口（[`push_to`]）。

/// 每台机器一把「正在放」的锁 ＋ 放成过几次（同一台同时几问都撞上「没装」时只放一份：排队的那一问进锁时发现
/// 计数变了 ⇒ 前一个刚放完，不再放）。
fn place_state(origin: &str) -> std::sync::Arc<tokio::sync::Mutex<u64>> {
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex, OnceLock};
    static LOCKS: OnceLock<Mutex<HashMap<String, Arc<tokio::sync::Mutex<u64>>>>> = OnceLock::new();
    let mut m = LOCKS
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    m.entry(origin.to_string()).or_default().clone()
}

/// 放一次（排队的那一问若发现前一个刚放完就不放）。`seen` = 进来那一刻读到的「放成过几次」；`announce` 恰在真放之前、恰一次。
/// 抽出来是为了判据：锁 · 计数 · 说一声的次序不靠真 SSH 就验得动（`push` / `announce` 替身数次数）。
pub(crate) async fn place_once<N, P, F>(
    origin: &str,
    seen: u64,
    announce: N,
    push: P,
) -> Result<(), String>
where
    N: FnOnce(),
    P: FnOnce() -> F,
    F: std::future::Future<Output = Result<(), String>>,
{
    let state = place_state(origin);
    let mut placed = state.lock().await;
    if *placed != seen {
        return Ok(());
    }
    announce();
    push().await?;
    *placed += 1;
    Ok(())
}

/// 此刻「放成过几次」（进锁之前读，交给 [`place_once`]）。
pub(crate) fn placed_count(origin: &str) -> u64 {
    place_state(origin)
        .try_lock()
        .map(|g| *g)
        .unwrap_or(u64::MAX)
}

/// 〔RM1f〕远端健康通道上那一句的 `kind`（前端 `remote-health.ts` 的标题表认它）。
pub(crate) const INSTALL_NOTICE_KIND: &str = "panorama-install";

/// 〔RM1f〕那一句说什么（纯函数，判据直接比）。
pub(crate) fn install_notice(origin: &str) -> crate::ui_contract::RemoteHealthPayload {
    let message = if origin == crate::backend::control::inbound_client::LOCAL_ORIGIN {
        // 〔RM1f〕本机那一台不经网络：放到 `~/.cc-monitor/bin/`，一两秒的事。
        copy_text("rsPanoramaCall.install.local", &[])
    } else {
        let who = crate::backend::control::cc_bus::machine_label(origin);
        copy_text("rsPanoramaCall.install.remote", &[("who", &who)])
    };
    crate::ui_contract::RemoteHealthPayload {
        origin: origin.to_string(),
        kind: INSTALL_NOTICE_KIND.to_string(),
        message,
    }
}

/// 〔MIG-3b 续〕放字节：那台（远端推 · 本机放）装上这一版带着的全景小程序。界面听到 `not_installed` / `unsupported` 才调。
#[tauri::command]
pub async fn panorama_place(
    app: tauri::AppHandle,
    origin: crate::origin::Origin,
) -> Result<(), String> {
    let _ = origin.route("panorama_place")?;
    let wire = origin.as_wire_str().to_string();
    // 进锁之前读计数：排队等锁期间前一个放成了 ⇒ 这一问不再放。
    let seen = placed_count(&wire);
    place_once(
        &wire,
        seen,
        || {
            use tauri::Emitter;
            if let Err(e) = app.emit(
                crate::ui_contract::events::REMOTE_HEALTH,
                install_notice(&wire),
            ) {
                tracing::warn!("[{wire}] 「正在装代码全景组件」那一句没发出去：{e}");
            }
        },
        || push_to(&origin),
    )
    .await
}

#[cfg(test)]
#[path = "../../../../tests/frontend/shell/panorama_bytes_tests.rs"]
mod tests;
