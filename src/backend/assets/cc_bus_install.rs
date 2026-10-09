//! 把这台二进制带着的 cc-bus 装到这台的 `<skills 根>/cc-bus/`。资产的装不算部署：判 · 写 · 记都在那台后端，
//! 写经它自己的文件管理面（[`crate::assets::door`]），装卸账复用 skill 装记录那一份（`skill_ledger`，`name = "cc-bus"`）。
//!
//! # `INVARIANTS` 第 7 条例外的四个配套要求
//!
//! - 用户显式动作：只由扩展页 cc-bus 那一行的确认卡经枢纽说 `cc-bus-install`（`assets/hub.rs`），启动 / 后台路径一处都不调。
//! - 独立 realpath 白名单：[`fenced_root`] —— `<skills 根>` 若已在，解到底必须仍在它的上一层（claude 目录）底下。
//! - 幂等：逐文件比内容，全一致就一个字节不写、不备份。
//! - 可撤销：覆盖前把整个目录改名成 `cc-bus.bak-<秒>`（`files-rename`，原子、不留半份）。
//!
//! 源是内嵌的（装了的二进制身边没有仓）：单一事实源是 `src/shared/cc-bus/`，编译期固化。

use copy_core::copy_text;
use serde_json::{json, Map, Value};
use std::path::{Path, PathBuf};

use crate::assets::door::{self, Door};

type Answer = Result<Value, crate::stream::inbound::spec::Fail>;
/// 装记录的写口（`skill_ledger::answer_record`）—— **由门（`stream/inbound/`）递进来**，本模块不直呼它（第四层判据 ④）。
pub(crate) type Record<'a> =
    &'a dyn Fn(&Value) -> Result<Value, crate::stream::inbound::spec::Fail>;

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
    // 保活是 cc-bus 的调用方，所以住 `examples/`、不进 `scripts/`（那里的命令面条数有判据钉着）。
    (
        "examples/cc-keepalive",
        include_bytes!("../../shared/cc-bus/examples/cc-keepalive"),
    ),
    (
        "examples/config",
        include_bytes!("../../shared/cc-bus/examples/config"),
    ),
    // 带注释的默认 kinds 表随包落盘，落在 `<claude_dir>/skills/cc-bus/examples/`，不是 `~/.cc-bus/kinds.tsv`：只读第 7 条例外只放行 `skills/cc-bus` 这一个落点。
    // 脚本按「`~/.cc-bus/kinds.tsv` → 随包这一份 → 内置 msg」的顺序找 ⇒ 随包这份就是生效的默认；用户要改时复制到 `~/.cc-bus/` 再改。
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
    // 只读看收件箱尾巴（后端 `bus-inbox` 转调它）。
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

/// 内嵌那一份在资产目录里的摘要：与 `asset_catalog::skill_asset` 对一个装好的目录算出来的相同
/// （按目录走的顺序 —— 逐级按名字排 —— 逐个喂（相对路径, 内容））。扩展页拿它当 cc-bus 的「这一版」。
pub(crate) fn embedded_digest() -> String {
    let mut files: Vec<&(&str, &[u8])> = FILES.iter().collect();
    files.sort_by(|a, b| a.0.split('/').cmp(b.0.split('/')));
    let mut f = crate::assets::asset_catalog::Fnv::default();
    f.part(crate::assets::asset_catalog::KIND_SKILL.as_bytes());
    for (rel, bytes) in files {
        f.part(rel.as_bytes()).part(bytes);
    }
    f.hex()
}

/// 装之前看一眼（**只读**）：`{dest, writes, existing, version}` —— 内容会变的那几个（缺的 ＋ 不一样的）· 落点上有没有东西
/// （在而且要写 ⇒ 装的时候整个改名留作备份）· 内嵌那一份的摘要。确认卡由枢纽拿它拼。
pub(crate) fn state_at(skills: &Path) -> Value {
    let dest = skills.join(NAME);
    let writes: Vec<&str> = FILES
        .iter()
        .filter(|(rel, bytes)| {
            std::fs::read(dest.join(rel))
                .map(|got| got != *bytes)
                .unwrap_or(true)
        })
        .map(|(rel, _)| *rel)
        .collect();
    json!({
        "dest": dest.display().to_string(),
        "writes": writes,
        "existing": dest.exists(),
        "version": embedded_digest(),
    })
}

fn skills_root() -> Result<PathBuf, crate::stream::inbound::spec::Fail> {
    super::asset_kind()
        .and_then(crate::agents::skills_root)
        .ok_or_else(|| {
            crate::stream::inbound::spec::Fail::from((
                "refused",
                copy_text("beCcBusInstall.root.unknown", &[]),
            ))
        })
}

/// `cc-bus-install-state {}` → [`state_at`]。
pub(crate) fn answer_state() -> Answer {
    Ok(state_at(&skills_root()?))
}

/// `cc-bus-install {}` → `{dest, written, backup, recordFailed}`（`written` = 写了的那几个相对路径；全都一致 ⇒ 空、一个字节不动）。
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
        return Ok(json!({ "dest": dest_s, "written": [], "backup": null, "recordFailed": null }));
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
                    &[("dest", &dest_s), ("bak", &bak), ("why", &e)],
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
        .map(|f| copy_text("beCcBusInstall.record.failed", &[("why", &f.into_note())]));
    Ok(json!({
        "dest": dest_s,
        "written": FILES.iter().map(|(rel, _)| *rel).collect::<Vec<_>>(),
        "backup": backup,
        "recordFailed": record_failed,
    }))
}

#[cfg(test)]
#[path = "../../../tests/backend/assets/cc_bus_install_tests.rs"]
mod tests;
