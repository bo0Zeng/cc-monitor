//! 〔MIG-3a · 子步 3 · `设计/01 §6.7a` 规矩 1 · 主会话 09-27 裁 ⑯ · 09-28 裁〕**cc-acct-iso 落进用户目录**：`~/.local/bin` 链接 ＋ 配置目录。
//!
//! 字节照走部署那一条（monitor 把 vendored 的脚本推进 `~/.cc-monitor/bin/cc-acct-iso/`）；**落进用户目录**这一步 ——
//! 从前是 monitor 经 ssh 起 `cc-acct-iso-install.sh`（`ln -sfn` ＋ `mkdir` ＋ `cp`）—— 今天由**那台后端的文件管理面**提交：
//! 链接走 `files-link`（写面闭集里 FILES2 那一个动词 `files_extract::land_link`，不另起原语）、配置走 `files-put` / `files-chmod`，
//! 装卸账记进 **skill 装记录那一份**（`name = "acct-iso"`，`at = "home"`：键是家目录、路径相对它；同形，不另立第二份账）。
//!
//! 与那份安装脚本逐项对得上的地方（`src/bridge/vendor/cc-acct-iso/scripts/cc-acct-iso-install.sh`）：
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
/// 装在哪只许是这一格底下（部署那一条的落点：`~/.cc-monitor/bin/`）。
const DEPLOY_PREFIX: &str = ".cc-monitor/bin/";

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

/// `acct-iso-install {dir}` → `{link, linked, config, configWritten, recordFailed}`。
/// `dir` = 部署那一条推到的地方（必须在 `~/.cc-monitor/bin/` 底下、主脚本已在）。
pub(crate) fn answer_install(d: &dyn Door, record: Record, args: &Value) -> Answer {
    let dir = args.get("dir").and_then(Value::as_str).ok_or((
        "bad_args",
        crate::common::contract::malformed("missing `dir`"),
    ))?;
    let home = door::home(d).map_err(refused)?;
    let rel_dir = door::rel_under(&home, dir).map_err(refused)?;
    if !rel_dir.starts_with(DEPLOY_PREFIX) || rel_dir.split('/').any(|s| s == "..") {
        return Err(refused(copy_text(
            "beAcctIsoInstall.dir.outside",
            &[("dir", dir)],
        )));
    }
    let main_rel = format!("{rel_dir}/scripts/cc-acct-iso");
    let main_abs = door::join_under(&home, &main_rel);
    let main = door::peek(d, &home, &main_rel).map_err(refused)?;
    let Some(main_text) = main.text else {
        return Err(refused(copy_text(
            "beAcctIsoInstall.main.missing",
            &[("path", &main_abs)],
        )));
    };
    let mut files = Map::new();
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
            json!({ "digest": crate::skill_ledger::digest_of(&main_text), "created": true }),
        );
        true
    };
    // ② 配置：已有不覆盖；从随包那份拷。
    let config_abs = door::join_under(&home, CONFIG_REL);
    let config_written = if door::stat_kind(d, &config_abs).map_err(refused)?.is_some() {
        false
    } else {
        let sample = door::peek(d, &home, &format!("{rel_dir}/examples/config"))
            .map_err(refused)?
            .text
            .ok_or_else(|| {
                refused(copy_text(
                    "beAcctIsoInstall.config.noSample",
                    &[("dir", dir)],
                ))
            })?;
        door::put(d, &home, CONFIG_REL, &sample, None, false, true)
            .map_err(|e| refused(e.said()))?;
        if crate::platform::paths::has_exec_bits() {
            door::chmod(d, &home, CONFIG_DIR_REL, 0o700).map_err(refused)?;
        }
        files.insert(
            CONFIG_REL.to_string(),
            json!({ "digest": crate::skill_ledger::digest_of(&sample), "created": true }),
        );
        true
    };
    // ③ 记账：真建出来的那几个（一个没建 ⇒ 不记）。
    let record_failed = if files.is_empty() {
        None
    } else {
        record(&json!({ "op": "add", "name": NAME, "at": "home", "files": files }))
            .err()
            .map(|(_, e)| copy_text("beAcctIsoInstall.record.failed", &[("e", &e)]))
    };
    Ok(json!({
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
