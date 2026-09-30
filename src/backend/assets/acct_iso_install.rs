//! 〔MIG-3a · 子步 3 · `设计/01 §6.7a` 规矩 1 · 主会话 09-27 裁 ⑯ · 09-28 裁 2 · 09-28 预裁〕**把这台二进制带着的 cc-acct-iso 装到这台**：
//! 字节落 `~/.cc-monitor/bin/cc-acct-iso/` ＋ `~/.local/bin` 链接 ＋ 配置目录。
//!
//! 从前分两半：monitor 的 `deploy_remote_acct_iso`〔散文墓碑〕把它自己内嵌的那份 vendored 脚本经 SFTP 推过来、写身份标记，
//! 再（更早）经 ssh 跑 `cc-acct-iso-install.sh`。主会话 09-28 预裁「部署载荷只一种走法」：字节照 cc-bus 那样**随后端二进制走**
//! （[`FILES`]，单一事实源是 `src/shared/cc-acct-iso/`（两棵树都不属于，同 `src/shared/cc-bus/`；〔09-28〕从 `src/frontend/shell/vendor/` 挪来），编译期固化），落盘全经**这台后端的文件管理面**：
//! 字节 `files-put`（CAS：读到哪份就对哪份写）· 可执行位 `files-chmod` · 链接 `files-link`（写面闭集里 FILES2 那一个动词，不另起原语）；
//! 装卸账记进 **skill 装记录那一份**（`name = "acct-iso"`，`at = "home"`：键是家目录、路径相对它；同形，不另立第二份账）。
//!
//! 与那份安装脚本逐项对得上的地方（`src/shared/cc-acct-iso/scripts/cc-acct-iso-install.sh`）：
//! - 链接 `~/.local/bin/cc-acct-iso` → `<装在哪>/scripts/cc-acct-iso`；⚠ 已经有东西在那儿 ⇒ **不动它**、如实说（脚本是 `ln -sfn` 盖掉；
//!   写面的「建链接」对已在的一律拒，这里不先删再建 —— 那是替用户删一个他可能自己放的东西）。
//! - `~/.cc-acct-iso/`（`0700`）＋ `config`（从随包那份 `examples/config` 拷，**已有不覆盖**）。
//! - 脚本「刻意不改 rc、不动账号 / 凭据」—— 这里同样一个都不碰。

use copy_core::copy_text;
use serde_json::{json, Map, Value};

use crate::assets::door::{self, Door};

type Answer = Result<Value, (&'static str, String)>;
/// 装记录的写口（`skill_ledger::answer_record`）—— 由门（`inbound.rs`）递进来（第四层判据 ④）。
pub(crate) type Record<'a> = &'a dyn Fn(&Value) -> Result<Value, (&'static str, String)>;

/// 装记录里这一件叫什么。
pub(crate) const NAME: &str = "acct-iso";
/// 链接落在家目录底下的哪儿（同安装脚本的缺省 `BIN_DIR`）。
pub(crate) const LINK_REL: &str = ".local/bin/cc-acct-iso";
/// 配置落在哪（同安装脚本的缺省 `CC_ACCT_ISO_HOME`）。
pub(crate) const CONFIG_DIR_REL: &str = ".cc-acct-iso";
const CONFIG_REL: &str = ".cc-acct-iso/config";
/// 字节落在家目录底下的哪儿（部署那一根，`~/.cc-monitor/bin/` 底下；名字只住这里）。
pub(crate) const DEST_REL: &str = relay_route_core::ACCT_ISO_REL;
/// 身份标记（vendored 那份的内容指纹，[`vendor_id`]）落在 `DEST_REL` 底下的哪儿。
const MARKER_REL: &str = ".vendor_id";

/// 内嵌的那几份（相对 `DEST_REL`，可执行与否）。⚠ 与 vendored 目录两向对拍（`the_embedded_files_match_the_vendored_tree`）。
pub(crate) const FILES: &[(&str, &[u8], bool)] = &[
    (
        "scripts/cc-acct-iso",
        include_bytes!("../../shared/cc-acct-iso/scripts/cc-acct-iso"),
        true,
    ),
    (
        "scripts/lib.sh",
        include_bytes!("../../shared/cc-acct-iso/scripts/lib.sh"),
        true,
    ),
    (
        "scripts/cc-acct-iso-install.sh",
        include_bytes!("../../shared/cc-acct-iso/scripts/cc-acct-iso-install.sh"),
        true,
    ),
    (
        "scripts/test/run-tests.sh",
        include_bytes!("../../shared/cc-acct-iso/scripts/test/run-tests.sh"),
        true,
    ),
    (
        "SKILL.md",
        include_bytes!("../../shared/cc-acct-iso/SKILL.md"),
        false,
    ),
    (
        "examples/config",
        include_bytes!("../../shared/cc-acct-iso/examples/config"),
        false,
    ),
];

const VENDOR_ID_RAW: &str = include_str!("../../shared/cc-acct-iso/.vendor_id");

/// 内嵌那份的内容指纹（`.vendor_id`，vendored 目录里 `VENDOR.md` 那份菜谱算的；monitor 构建期自洽校验）。
pub(crate) fn vendor_id() -> &'static str {
    VENDOR_ID_RAW.trim()
}

fn refused(m: String) -> (&'static str, String) {
    ("refused", m)
}

/// 缺的那几层目录逐级建（在就算了）。
fn ensure_dirs(d: &dyn Door, home: &str, rel_dir: &str) -> Result<(), String> {
    let mut at = String::new();
    for seg in rel_dir.split('/').filter(|s| !s.is_empty()) {
        if !at.is_empty() {
            at.push('/');
        }
        at.push_str(seg);
        if door::stat_kind(d, &door::join_under(home, &at))?.is_none() {
            door::mkdir(d, home, &at)?;
        }
    }
    Ok(())
}

/// 一份字节落到 `rel`（相对家目录）：盘上已是这一份 ⇒ 不写（`None`）；否则按读到的那一份 CAS 写（`expect`），要可执行就补位。
/// 回 `Some(原来没有)`。
fn land(
    d: &dyn Door,
    home: &str,
    rel: &str,
    text: &str,
    exec: bool,
) -> Result<Option<bool>, (&'static str, String)> {
    // 父目录都还不在时 `files-peek` 解析不了那条路径 ⇒ 先问在不在，不在就是「原来没有」。
    let now = if door::stat_kind(d, &door::join_under(home, rel))
        .map_err(refused)?
        .is_some()
    {
        door::peek(d, home, rel).map_err(refused)?.text
    } else {
        None
    };
    if now.as_deref() == Some(text) {
        return Ok(None);
    }
    door::put(d, home, rel, text, now.as_deref(), false, true).map_err(|e| refused(e.said()))?;
    if exec && crate::platform::paths::has_exec_bits() {
        door::chmod(d, home, rel, 0o755).map_err(refused)?;
    }
    Ok(Some(now.is_none()))
}

/// `acct-iso-install {}` → `{dest, version, written, link, linked, config, configWritten, recordFailed}`。
/// 落点由这台后端按自己的家目录算（[`DEST_REL`]），不收调用方给的路径。
pub(crate) fn answer_install(d: &dyn Door, record: Record) -> Answer {
    let home = door::home(d).map_err(refused)?;
    let dest_abs = door::join_under(&home, DEST_REL);
    // ⓪ 字节：逐份比，一致就不写（幂等）；标记最后写（字节齐了才算这一版）。
    let mut written = 0usize;
    let mut main_text = None;
    let mut files = Map::new();
    for (rel, bytes, exec) in FILES {
        let text = std::str::from_utf8(bytes).map_err(|_| {
            (
                "bad_file",
                copy_text("beAcctIsoInstall.embed.notUtf8", &[("rel", rel)]),
            )
        })?;
        let at = format!("{DEST_REL}/{rel}");
        if let Some(created) = land(d, &home, &at, text, *exec)? {
            written += 1;
            files.insert(
                at,
                json!({ "digest": crate::assets::skill_ledger::digest_of(text), "created": created }),
            );
        }
        if *rel == "scripts/cc-acct-iso" {
            main_text = Some(text);
        }
    }
    let marker = format!("{DEST_REL}/{MARKER_REL}");
    let stamp = format!("{}\n", vendor_id());
    if let Some(created) = land(d, &home, &marker, &stamp, false)? {
        files.insert(
            marker,
            json!({ "digest": crate::assets::skill_ledger::digest_of(&stamp), "created": created }),
        );
    }
    let main_text = main_text.unwrap_or_default();
    let main_abs = door::join_under(&home, &format!("{DEST_REL}/scripts/cc-acct-iso"));
    // ① 链接：已在（任何东西）⇒ 不动它。
    let link_abs = door::join_under(&home, LINK_REL);
    let linked = if door::stat_kind(d, &link_abs).map_err(refused)?.is_some() {
        false
    } else {
        let parent = LINK_REL.rsplit_once('/').map_or("", |(p, _)| p);
        ensure_dirs(d, &home, parent).map_err(refused)?;
        door::link(d, &home, LINK_REL, &main_abs).map_err(refused)?;
        files.insert(
            LINK_REL.to_string(),
            json!({ "digest": crate::assets::skill_ledger::digest_of(&main_text), "created": true }),
        );
        true
    };
    // ② 配置：已有不覆盖；从随包那份拷。
    let config_abs = door::join_under(&home, CONFIG_REL);
    let config_written = if door::stat_kind(d, &config_abs).map_err(refused)?.is_some() {
        false
    } else {
        let sample = FILES
            .iter()
            .find(|(rel, _, _)| *rel == "examples/config")
            .and_then(|(_, b, _)| std::str::from_utf8(b).ok())
            .unwrap_or_default();
        door::put(d, &home, CONFIG_REL, sample, None, false, true)
            .map_err(|e| refused(e.said()))?;
        if crate::platform::paths::has_exec_bits() {
            door::chmod(d, &home, CONFIG_DIR_REL, 0o700).map_err(refused)?;
        }
        files.insert(
            CONFIG_REL.to_string(),
            json!({ "digest": crate::assets::skill_ledger::digest_of(sample), "created": true }),
        );
        true
    };
    // ③ 记账：这一趟真写了的那几个（一个没写 ⇒ 不记）。
    let record_failed = if files.is_empty() {
        None
    } else {
        record(&json!({ "op": "add", "name": NAME, "at": "home", "files": files }))
            .err()
            .map(|(_, e)| copy_text("beAcctIsoInstall.record.failed", &[("e", &e)]))
    };
    Ok(json!({
        "dest": dest_abs,
        "version": vendor_id(),
        "written": written,
        "link": link_abs,
        "linked": linked,
        "config": config_abs,
        "configWritten": config_written,
        "recordFailed": record_failed,
    }))
}

#[cfg(test)]
#[path = "../../../tests/backend/assets/acct_iso_install_tests.rs"]
mod tests;
