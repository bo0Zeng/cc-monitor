//! 〔MIG-3a · 子步 3 · `设计/01 §6.7a` 规矩 1 · 主会话 09-27 裁 ⑯〕**把这台二进制带着的 cc-bus 装到这台的 `<skills 根>/cc-bus/`**。
//!
//! 从前是 monitor 的 `cc_bus_deploy.rs`（`deploy_local_cc_bus` / `cc_bus_install_state` 两条 Tauri 命令）：monitor 读盘判三态、
//! 算好经本机后端写。「资产的装不算部署」—— 往那台放 cc-bus 归**后端代管的资产**（`01 §3.4`）：判 · 写 · 记都在那台后端，
//! 写经它自己的文件管理面（[`crate::assets::door`]），装卸账**复用 skill 装记录那一份**（`skill_ledger`，同形：`name = "cc-bus"`，
//! 目录由记录模块按 skills 根算，不另立第二份账）。
//!
//! # `INVARIANTS` 第 7 条例外的四个配套要求（一条都不许省，逐条对应到下面）
//!
//! - **用户显式动作**：只由设置页那颗按钮经通道说 `cc-bus-install`，启动 / 后台路径一处都不调。
//! - **独立 realpath 白名单**：[`fenced_root`] —— `<skills 根>` 若已在，解到底必须仍在它的上一层（claude 目录）底下。
//! - **幂等**：逐文件比内容，全一致就一个字节不写、不备份。
//! - **可撤销**：覆盖前把整个目录改名成 `cc-bus.bak-<秒>`（`files-rename`，原子、不留半份）。
//!
//! 源是**内嵌**的（装了的二进制身边没有仓）：单一事实源仍是 `src/shared/cc-bus/`（两棵树都不属于），编译期固化。

use copy_core::copy_text;
use serde_json::{json, Map, Value};
use std::path::{Path, PathBuf};

use crate::assets::door::{self, Door};

type Answer = Result<Value, (&'static str, String)>;
/// 装记录的写口（`skill_ledger::answer_record`）—— **由门（`inbound.rs`）递进来**，本模块不直呼它（第四层判据 ④）。
pub(crate) type Record<'a> = &'a dyn Fn(&Value) -> Result<Value, (&'static str, String)>;

/// 装记录里这一件叫什么（记录按 `skills 根 / name` 算目录 ⇒ 就是 `<skills 根>/cc-bus`）。
pub(crate) const NAME: &str = "cc-bus";

/// 内嵌的那些文件（条数以 `FILES.len()` 为准，判据对拍，不在散文里写数）。
///
/// ⚠ 加文件要同时加到这里 —— 判据 `the_embedded_file_list_matches_the_repo` 会对拍，少一个当场红。
const FILES: &[(&str, &[u8])] = &[
    ("SKILL.md", include_bytes!("../../shared/cc-bus/SKILL.md")),
    (
        "examples/cc-busd.service",
        include_bytes!("../../shared/cc-bus/examples/cc-busd.service"),
    ),
    // 〔保活 09-24〕设计 95 §3bis：保活是 cc-bus 的**调用方**，所以它住 `examples/`，
    // 不进 `scripts/`（那里的命令面条数有判据钉着，而「保活不是 cc-bus 的功能」本来就该在结构上看得见）。
    (
        "examples/cc-keepalive",
        include_bytes!("../../shared/cc-bus/examples/cc-keepalive"),
    ),
    (
        "examples/config",
        include_bytes!("../../shared/cc-bus/examples/config"),
    ),
    // 〔kinds 09-24〕设计 95 §2.2「部署要跟上」：**带注释的默认 kinds 表**随包落盘。
    // ⚠ 它落在 `<claude_dir>/skills/cc-bus/examples/`，**不是** `~/.cc-bus/kinds.tsv` ——
    //   只读铁律第 7 条例外只放行 `skills/cc-bus` 这一个落点（`fenced_dest`）。
    //   脚本按「`~/.cc-bus/kinds.tsv` → 随包这一份 → 内置 msg」的顺序找，所以随包这份**就是生效的默认**，
    //   用户要改时复制到 `~/.cc-bus/` 再改（`cc-bus-install.sh` 顺手放一份 `.example`）。
    (
        "examples/kinds.tsv",
        include_bytes!("../../shared/cc-bus/examples/kinds.tsv"),
    ),
    (
        "examples/policy.tsv",
        include_bytes!("../../shared/cc-bus/examples/policy.tsv"),
    ),
    (
        "scripts/cc-agents",
        include_bytes!("../../shared/cc-bus/scripts/cc-agents"),
    ),
    (
        "scripts/cc-broadcast",
        include_bytes!("../../shared/cc-bus/scripts/cc-broadcast"),
    ),
    (
        "scripts/cc-bus-adapt.sh",
        include_bytes!("../../shared/cc-bus/scripts/cc-bus-adapt.sh"),
    ),
    (
        "scripts/cc-bus-adapt-posix.sh",
        include_bytes!("../../shared/cc-bus/scripts/cc-bus-adapt-posix.sh"),
    ),
    (
        "scripts/cc-bus-adapt-windows.sh",
        include_bytes!("../../shared/cc-bus/scripts/cc-bus-adapt-windows.sh"),
    ),
    (
        "scripts/cc-bus-agent-claude.sh",
        include_bytes!("../../shared/cc-bus/scripts/cc-bus-agent-claude.sh"),
    ),
    (
        "scripts/cc-commit",
        include_bytes!("../../shared/cc-bus/scripts/cc-commit"),
    ),
    (
        "scripts/cc-peek",
        include_bytes!("../../shared/cc-bus/scripts/cc-peek"),
    ),
    (
        "scripts/cc-bus-install.sh",
        include_bytes!("../../shared/cc-bus/scripts/cc-bus-install.sh"),
    ),
    (
        "scripts/cc-bus-lib.sh",
        include_bytes!("../../shared/cc-bus/scripts/cc-bus-lib.sh"),
    ),
    (
        "scripts/cc-bus-stop-hook",
        include_bytes!("../../shared/cc-bus/scripts/cc-bus-stop-hook"),
    ),
    (
        "scripts/cc-busd",
        include_bytes!("../../shared/cc-bus/scripts/cc-busd"),
    ),
    (
        "scripts/cc-kill",
        include_bytes!("../../shared/cc-bus/scripts/cc-kill"),
    ),
    (
        "scripts/cc-list",
        include_bytes!("../../shared/cc-bus/scripts/cc-list"),
    ),
    // 〔SH1 · V136〕只读看收件箱尾巴（后端 `bus-inbox` 转调它）。
    (
        "scripts/cc-log",
        include_bytes!("../../shared/cc-bus/scripts/cc-log"),
    ),
    (
        "scripts/cc-recv",
        include_bytes!("../../shared/cc-bus/scripts/cc-recv"),
    ),
    (
        "scripts/cc-register",
        include_bytes!("../../shared/cc-bus/scripts/cc-register"),
    ),
    (
        "scripts/cc-send",
        include_bytes!("../../shared/cc-bus/scripts/cc-send"),
    ),
    (
        "scripts/cc-spawned-record",
        include_bytes!("../../shared/cc-bus/scripts/cc-spawned-record"),
    ),
    (
        "scripts/cc-spawn",
        include_bytes!("../../shared/cc-bus/scripts/cc-spawn"),
    ),
    (
        "scripts/cc-whoami",
        include_bytes!("../../shared/cc-bus/scripts/cc-whoami"),
    ),
];

/// ★ **白名单（独立 realpath 围栏）**：`<skills 根>` 若已在，解到底必须仍在它的上一层底下（挡「skills 是个指向别处的软链」）。
/// 还不在 ⇒ 写的时候逐级建（`files-put` 的 `parents`，每一级各过写口那道围栏）。回 `(写的根, 落点)`：根 = skills 根的上一层。
fn fenced_root(skills: &Path) -> Result<(PathBuf, PathBuf), String> {
    let top = skills
        .parent()
        .ok_or_else(|| {
            copy_text(
                "beCcBusInstall.fence.noParent",
                &[("skills", &skills.display().to_string())],
            )
        })?
        .to_path_buf();
    if skills.exists() {
        let real = skills.canonicalize().map_err(|e| {
            copy_text(
                "beCcBusInstall.fence.resolveFailed",
                &[
                    ("path", &skills.display().to_string()),
                    ("e", &e.to_string()),
                ],
            )
        })?;
        let top_real = top.canonicalize().map_err(|e| {
            copy_text(
                "beCcBusInstall.fence.resolveFailed",
                &[("path", &top.display().to_string()), ("e", &e.to_string())],
            )
        })?;
        if !real.starts_with(&top_real) {
            return Err(copy_text(
                "beCcBusInstall.fence.escapes",
                &[
                    ("real", &real.display().to_string()),
                    ("top", &top_real.display().to_string()),
                ],
            ));
        }
    }
    Ok((top, skills.join(NAME)))
}

/// skills 根自己在写的根底下叫什么（写的根 = 它的上一层）。
fn skills_name(skills: &Path) -> String {
    skills
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// 落点相对写的根的那一段。
fn rel_of(skills: &Path, rel: &str) -> String {
    format!("{}/{NAME}/{rel}", skills_name(skills))
}

/// 查这台装的是哪一版（**只读**）：`{state: "not_installed" | "up_to_date" | "drifted", differing?, missing?}`。
/// 三态刻意不合并：「没装」并进「不是最新」⇒ 以为点一下是更新；「不是最新」并进「已装」⇒ 装着旧的没人去点。
pub(crate) fn state_at(skills: &Path) -> Value {
    let dest = skills.join(NAME);
    if !dest.is_dir() {
        return json!({ "state": "not_installed" });
    }
    let (mut differing, mut missing) = (0u32, 0u32);
    for (rel, bytes) in FILES {
        let p = dest.join(rel);
        if !p.exists() {
            missing += 1;
        } else if std::fs::read(&p).map(|got| got != *bytes).unwrap_or(true) {
            differing += 1;
        }
    }
    if differing == 0 && missing == 0 {
        json!({ "state": "up_to_date" })
    } else if missing as usize == FILES.len() {
        // 目录在、一个内嵌文件都没有 ⇒ 那不是「装了个旧版」，是根本没装。
        json!({ "state": "not_installed" })
    } else {
        json!({ "state": "drifted", "differing": differing, "missing": missing })
    }
}

fn skills_root() -> Result<PathBuf, (&'static str, String)> {
    crate::agents::skills_root().ok_or(("refused", copy_text("beCcBusInstall.root.unknown", &[])))
}

/// `cc-bus-install-state {}` → 三态。
pub(crate) fn answer_state() -> Answer {
    Ok(state_at(&skills_root()?))
}

/// `cc-bus-install {}` → `{dest, written, unchanged, backup, recordFailed}`。
pub(crate) fn answer_install(d: &dyn Door, record: Record) -> Answer {
    install_at(d, &skills_root()?, record)
}

/// 装到给定的 skills 根（可注入，判据拿临时目录跑）。
pub(crate) fn install_at(d: &dyn Door, skills: &Path, record: Record) -> Answer {
    let (top, dest) = fenced_root(skills).map_err(|e| ("refused", e))?;
    let root = top.display().to_string();
    let dest_s = dest.display().to_string();
    // 先算幂等：全都一致就什么都不做（不备份、不写、不记）。
    let mut all_same =
        door::stat_kind(d, &dest_s).map_err(|e| ("refused", e))? == Some("dir".into());
    if all_same {
        for (rel, bytes) in FILES {
            let got = door::peek(d, &root, &rel_of(skills, rel)).map_err(|e| ("refused", e))?;
            if got.text.as_deref().map(str::as_bytes) != Some(*bytes) {
                all_same = false;
                break;
            }
        }
    }
    if all_same {
        return Ok(
            json!({ "dest": dest_s, "written": 0, "unchanged": FILES.len(), "backup": null, "recordFailed": null }),
        );
    }
    // ★ 可撤销：整个目录改名（原子）。已经一致时走不到这里 ⇒ 不攒垃圾备份。
    let backup = if door::stat_kind(d, &dest_s)
        .map_err(|e| ("refused", e))?
        .is_some()
    {
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|x| x.as_secs())
            .unwrap_or(0);
        let bak = format!("{NAME}.bak-{ts}");
        let top_name = skills_name(skills);
        let (from, to) = (format!("{top_name}/{NAME}"), format!("{top_name}/{bak}"));
        door::rename(d, &root, &from, &to).map_err(|e| {
            (
                "refused",
                copy_text(
                    "beCcBusInstall.backup.failed",
                    &[("dest", &dest_s), ("bak", &bak), ("e", &e)],
                ),
            )
        })?;
        Some(dest.with_file_name(bak).display().to_string())
    } else {
        None
    };
    let mut files = Map::new();
    for (rel, bytes) in FILES {
        let text = std::str::from_utf8(bytes).map_err(|_| {
            (
                "bad_file",
                copy_text("beCcBusInstall.embed.notUtf8", &[("rel", rel)]),
            )
        })?;
        let at = rel_of(skills, rel);
        door::put(d, &root, &at, text, None, false, true).map_err(|e| ("refused", e.said()))?;
        // `scripts/` 下的与带 shebang 的都要可执行（装完不能跑等于没装）。Windows 上没有可执行位：写口那边如实回失败 ⇒ 只在说 POSIX 的那台上发。
        if crate::platform::paths::has_exec_bits()
            && (rel.starts_with("scripts/") || bytes.starts_with(b"#!"))
        {
            door::chmod(d, &root, &at, 0o755).map_err(|e| ("refused", e))?;
        }
        files.insert(
            (*rel).to_string(),
            json!({ "digest": crate::assets::skill_ledger::digest_of(text), "created": true }),
        );
    }
    let record_failed = record(&json!({ "op": "add", "name": NAME, "files": files }))
        .err()
        .map(|(_, e)| copy_text("beCcBusInstall.record.failed", &[("e", &e)]));
    Ok(json!({
        "dest": dest_s,
        "written": FILES.len(),
        "unchanged": 0,
        "backup": backup,
        "recordFailed": record_failed,
    }))
}

#[cfg(test)]
#[path = "../../../tests/backend/assets/cc_bus_install_tests.rs"]
mod tests;
