//! 把这台二进制带着的 cc-bus 装到这台的 `<skills 根>/cc-bus/`。资产的装不算部署：判 · 写 · 记都在那台后端，
//! 写经它自己的文件管理面（[`crate::common::door`]），装卸账复用 skill 装记录那一份（`skill_ledger`，`name = "cc-bus"`）。
//!
//! # `INVARIANTS` 第 7 条例外的四个配套要求
//!
//! - 用户显式动作：只由扩展页 cc-bus 那一行的确认卡经枢纽说 `cc-bus-install`（`assets/hub.rs`），启动 / 后台路径一处都不调。
//! - 独立 realpath 白名单：[`fenced_root`] —— `<skills 根>` 若已在，解到底必须仍在它的上一层（claude 目录）底下。
//! - 幂等：逐文件比内容，全一致就一个字节不写、不备份。
//! - 可撤销：覆盖前把整个目录改名成 `cc-bus.bak-<秒>`（`files-rename`，原子、不留半份）。
//!
//! 源是内嵌的（装了的二进制身边没有仓）：单一事实源是 `src/shared/cc-bus/`，编译期固化。

use crate::platform::fs::Ro;
use copy_core::copy_text;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

use crate::assets::hub_wire::{CcBusInstalled, CcBusState};
use crate::common::door::{self, Door};
use crate::common::said::IntoNote as _;

pub(crate) use super::skill_ledger::Record;
use super::skill_ledger::{RecordReq, Recorded};

/// 装记录里这一件叫什么（记录按 `skills 根 / name` 算目录 ⇒ 就是 `<skills 根>/cc-bus`）。
pub(crate) const NAME: &str = "cc-bus";

/// 内嵌的那些文件（条数以 `FILES.len()` 为准，判据对拍，不在散文里写数）。
///
/// ⚠ 加文件要同时加到这里 —— 判据 `the_embedded_file_list_matches_the_repo` 会对拍，少一个当场红。
const FILES: &[(&str, &[u8])] = &[
    ("SKILL.md", include_bytes!("../../shared/cc-bus/SKILL.md")),
    // 唯一的入口（PATH 上只有它）；各动词的本体是同目录的 `cc-bus-<动词>.sh`。
    (
        "scripts/cc-bus",
        include_bytes!("../../shared/cc-bus/scripts/cc-bus"),
    ),
    (
        "scripts/cc-bus-agent-claude.sh",
        include_bytes!("../../shared/cc-bus/scripts/cc-bus-agent-claude.sh"),
    ),
    (
        "scripts/cc-bus-agent-codex.sh",
        include_bytes!("../../shared/cc-bus/scripts/cc-bus-agent-codex.sh"),
    ),
    (
        "scripts/cc-bus-commit.sh",
        include_bytes!("../../shared/cc-bus/scripts/cc-bus-commit.sh"),
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
        "scripts/cc-bus-list.sh",
        include_bytes!("../../shared/cc-bus/scripts/cc-bus-list.sh"),
    ),
    (
        "scripts/cc-bus-log.sh",
        include_bytes!("../../shared/cc-bus/scripts/cc-bus-log.sh"),
    ),
    (
        "scripts/cc-bus-peek.sh",
        include_bytes!("../../shared/cc-bus/scripts/cc-bus-peek.sh"),
    ),
    (
        "scripts/cc-bus-send.sh",
        include_bytes!("../../shared/cc-bus/scripts/cc-bus-send.sh"),
    ),
    (
        "scripts/cc-bus-stop-hook.sh",
        include_bytes!("../../shared/cc-bus/scripts/cc-bus-stop-hook.sh"),
    ),
    (
        "scripts/cc-bus-unread.sh",
        include_bytes!("../../shared/cc-bus/scripts/cc-bus-unread.sh"),
    ),
    (
        "scripts/cc-bus-whoami.sh",
        include_bytes!("../../shared/cc-bus/scripts/cc-bus-whoami.sh"),
    ),
];

/// ★ **白名单（独立 realpath 围栏）**：`<skills 根>` 若已在，解到底必须仍在它的上一层底下（挡「skills 是个指向别处的软链」）。
/// 还不在 ⇒ 写的时候逐级建（`Door::put` 的 `parents`，每一级各过写口那道围栏）。回 `(写的根, 落点)`：根 = skills 根的上一层。
fn fenced_root(skills: &Path) -> Result<(PathBuf, PathBuf), crate::common::said::Said> {
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
        // 解不到底：那一句只带路径与原因词，系统原话进复制详情。
        let unresolved = |at: &Path, e: std::io::Error| {
            crate::common::said::Said::with_raw(
                copy_text(
                    "reason.fs.readFailed",
                    &[
                        ("path", &at.display().to_string()),
                        ("why", &copy_core::io_reason(e.kind())),
                    ],
                ),
                &e,
            )
        };
        let real = skills.canonicalize().map_err(|e| unresolved(skills, e))?;
        let top_real = top.canonicalize().map_err(|e| unresolved(&top, e))?;
        if !real.starts_with(&top_real) {
            return Err(copy_text(
                "beCcBusInstall.fence.escapes",
                &[
                    ("real", &real.display().to_string()),
                    ("top", &top_real.display().to_string()),
                ],
            )
            .into());
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
    let mut f = crate::common::digest::Fnv::default();
    f.part(
        crate::assets::asset_catalog::ExtKind::Skill
            .as_str()
            .as_bytes(),
    );
    for (rel, bytes) in files {
        f.part(rel.as_bytes()).part(bytes);
    }
    f.hex()
}

/// 装之前看一眼（**只读**）：`{dest, writes, existing, version}` —— 内容会变的那几个（缺的 ＋ 不一样的）· 落点上有没有东西
/// （在而且要写 ⇒ 装的时候整个改名留作备份）· 内嵌那一份的摘要。确认卡由枢纽拿它拼。
pub(crate) fn state_at(skills: &Path) -> CcBusState {
    let dest = skills.join(NAME);
    let writes: Vec<String> = FILES
        .iter()
        .filter(|(rel, bytes)| {
            Ro::read(dest.join(rel))
                .map(|got| got != *bytes)
                .unwrap_or(true)
        })
        .map(|(rel, _)| (*rel).to_string())
        .collect();
    CcBusState {
        dest: dest.display().to_string(),
        writes,
        existing: dest.exists(),
        version: embedded_digest(),
    }
}

fn skills_root() -> Result<PathBuf, crate::common::said::Said> {
    crate::agents::asset_kind()
        .and_then(crate::agents::skills_root)
        .ok_or_else(|| {
            crate::wire::spec::fail("refused", copy_text("beCcBusInstall.root.unknown", &[]))
        })
}

/// `cc-bus-install-state {}` → [`state_at`]。
pub(crate) fn answer_state() -> Result<CcBusState, crate::common::said::Said> {
    Ok(state_at(&skills_root()?))
}

/// `cc-bus-install {}` → `{dest, written, backup, recordFailed}`（`written` = 写了的那几个相对路径；全都一致 ⇒ 空、一个字节不动）。
pub(crate) fn answer_install(
    d: &dyn Door,
    record: Record,
) -> Result<CcBusInstalled, crate::common::said::Said> {
    install_at(d, &skills_root()?, record)
}

/// 装到给定的 skills 根（可注入，判据拿临时目录跑）。
pub(crate) fn install_at(
    d: &dyn Door,
    skills: &Path,
    record: Record,
) -> Result<CcBusInstalled, crate::common::said::Said> {
    let (top, dest) = fenced_root(skills).map_err(|e| crate::wire::spec::fail("refused", e))?;
    let root = top.display().to_string();
    let dest_s = dest.display().to_string();
    // 记：每个内嵌文件一行、摘要是内嵌那一份的；`created` = 这一趟是不是新建的（已记的取第一次的，写口管）。
    //   全都一致那一形也记：盘上被别处换成了这一版之后，记录里不能还留着旧摘要（卸时会误判「装完改过」）；记录没变写口不写盘。
    let record_all = |created: bool| -> Result<Option<String>, crate::common::said::Said> {
        let mut files = std::collections::BTreeMap::new();
        for (rel, bytes) in FILES {
            let text = std::str::from_utf8(bytes).map_err(|_| {
                crate::common::said::Said::new(
                    "bad_file",
                    copy_text("beCcBusInstall.embed.notUtf8", &[("rel", rel)]),
                )
            })?;
            files.insert(
                (*rel).to_string(),
                Recorded {
                    digest: crate::common::digest::fnv_digest(text),
                    created,
                },
            );
        }
        Ok(record(&RecordReq::Add {
            name: NAME.to_string(),
            files,
            project: None,
        })
        .err()
        .map(|f| copy_text("beCcBusInstall.record.failed", &[("why", &f.into_note())])))
    };
    // 先算幂等：全都一致就一个字节不写、不备份（只记）。
    let mut all_same = door::stat_kind(d, &dest_s)
        .map_err(|e| crate::common::said::Said::new("refused", e))?
        == Some("dir".into());
    if all_same {
        for (rel, bytes) in FILES {
            let got = door::peek(d, &root, &rel_of(skills, rel))
                .map_err(|e| crate::common::said::Said::new("refused", e))?;
            if got.text.as_deref().map(str::as_bytes) != Some(*bytes) {
                all_same = false;
                break;
            }
        }
    }
    if all_same {
        return Ok(CcBusInstalled {
            dest: dest_s,
            written: Vec::new(),
            backup: None,
            record_failed: record_all(false)?,
        });
    }
    // ★ 可撤销：整个目录改名（原子）。已经一致时走不到这里 ⇒ 不攒垃圾备份。
    let backup = if door::stat_kind(d, &dest_s)
        .map_err(|e| crate::common::said::Said::new("refused", e))?
        .is_some()
    {
        let ts = crate::common::time::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|x| x.as_secs())
            .unwrap_or(0);
        let bak = format!("{NAME}.bak-{ts}");
        let top_name = skills_name(skills);
        let (from, to) = (format!("{top_name}/{NAME}"), format!("{top_name}/{bak}"));
        door::rename(d, &root, &from, &to).map_err(|e| {
            crate::common::said::Said::new(
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
    for (rel, bytes) in FILES {
        let text = std::str::from_utf8(bytes).map_err(|_| {
            crate::common::said::Said::new(
                "bad_file",
                copy_text("beCcBusInstall.embed.notUtf8", &[("rel", rel)]),
            )
        })?;
        let at = rel_of(skills, rel);
        door::put(d, &root, &at, text, None, false, true)
            .map_err(|e| crate::common::said::Said::new("refused", e.said()))?;
        // `scripts/` 下的与带 shebang 的都要可执行（装完不能跑等于没装）。Windows 上没有可执行位：写口那边如实回失败 ⇒ 只在说 POSIX 的那台上发。
        if crate::platform::paths::has_exec_bits()
            && (rel.starts_with("scripts/") || bytes.starts_with(b"#!"))
        {
            door::chmod(d, &root, &at, 0o755)
                .map_err(|e| crate::common::said::Said::new("refused", e))?;
        }
    }
    // 原有那一份已整个改名走 ⇒ 这一趟每个都是新建的。
    let record_failed = record_all(true)?;
    Ok(CcBusInstalled {
        dest: dest_s,
        written: FILES.iter().map(|(rel, _)| (*rel).to_string()).collect(),
        backup,
        record_failed,
    })
}

#[cfg(test)]
#[path = "../../../tests/backend/assets/cc_bus_install_tests.rs"]
mod tests;
