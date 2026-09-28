//! 〔RW1 · 第四波 · 2026-09-24〕**monitor 进程够用户文件的唯一开口 —— 它一个字节都不自己落盘。**
//!
//! 用户逐字：「现在只允许后端的文件管理部分写文件」；追问后裁「**只管用户的文件**」「**也管本机**」。
//! ⇒ rc 里的别名块 · PowerShell `$PROFILE` · 项目 `.mcp.json` · skill 收件箱 · `~/.claude/skills/cc-bus/` ·
//! 删历史会话，这些改动从此都经**那台机器上的后端**（`files-peek` / `files-put` / `files-rename` /
//! `files-chmod` / `files-delete-session`），本机与远端**同一条路**，只差 origin。
//! 〔RM1d〕代码全景的批注 / 文档关联（V110「引擎只算、文件管理来写」）也经这扇门：计划由全景小程序算
//! （`panorama_call.rs::edit_via`），落盘是这里的 `put` / `delete`（后者 = `files-delete`）。
//!
//! # 分工
//!
//! | 在哪 | 做什么 |
//! |---|---|
//! | 这里（monitor） | 读（经后端）· **算**新内容（各调用方的纯规划函数：`fenced_block::splice_in` 等）· 交给后端 |
//! | 后端 `control/files_write.rs::put_text` | CAS · 相同不写 · 备份 · 暂存旁名换名上位 · 回读比对 · 回滚 —— **写的规则只有那一份** |
//!
//! 读与写之间隔着一次往返 ⇒ 交出去的时候**必须**带「我读到的是哪一份」（`expect`）；
//! 盘上那份在这中间被别人改了 ⇒ 后端回 `stale`、一个字节不写，这里**重读重算**（有上限）。
//!
//! 🔴 `D11`：那台机器的后端没连上 ⇒ **明确报错，不回落**到直写。

use crate::backend::control::backend_route::{no_channel, route_call_error, Routed};
use crate::backend::control::inbound_client::client_for;
use crate::copy_table::copy_text;

/// 一次 `files-peek` 读回来的。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Peeked {
    /// 读的是哪一份（后端解过链接的那一个，给人看）。
    pub path: String,
    /// `None` = 确定不存在（「读不出来」是 `Err`，不是这一形）。
    pub text: Option<String>,
}

/// 一次 `files-put` 真写了之后的回执。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Landed {
    pub path: String,
    pub changed: bool,
    pub created: bool,
    pub backup: Option<String>,
}

/// 一次后端写没成：**`stale` 与别的分得开**（前者该重读重算，后者原话交给用户）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Refused {
    Stale(String),
    /// 对端说了话、给了一个码（`stale` 之外的那些：`refused` / `unreadable` / …）。
    Peer {
        code: String,
        said: String,
    },
    Other(String),
}

impl Refused {
    pub(crate) fn said(self) -> String {
        match self {
            Refused::Stale(s) | Refused::Other(s) | Refused::Peer { said: s, .. } => s,
        }
    }
}

impl From<String> for Refused {
    fn from(s: String) -> Self {
        Refused::Other(s)
    }
}

/// 「那台机器上的文件管理那一面」。生产只有 [`BackendDoor`] 一个实现；判据用内存 / 临时目录的替身。
pub(crate) trait Door {
    /// 这台机器在话里怎么称呼（「本机」/ 远端名）。
    fn machine(&self) -> String;
    /// 后端这个进程的 home（绝对路径）。
    async fn home(&self) -> Result<String, String>;
    async fn peek(&self, root: &str, rel: &str) -> Result<Peeked, String>;
    async fn put(
        &self,
        root: &str,
        rel: &str,
        content: &str,
        expect: Option<&str>,
        backup: bool,
        parents: bool,
    ) -> Result<Landed, Refused>;
    async fn rename(&self, root: &str, from: &str, to: &str) -> Result<(), String>;
    /// 〔RM1d〕删**一个文件**（`files-delete`，非递归；落点同一道围栏）。今天唯一的用户：全景删批注侧车。
    /// 〔RM1e〕带 CAS：`expect` = 读到的那一份，盘上逐字节等于它才删；不等 / 已经不在 ⇒ [`Refused::Stale`]，一个字节不动。
    /// 〔墓碑 —— RM1d 那一版这里写着「没有 CAS（后端这条命令不收 `expect`），调用方先 `peek` 核一遍」。〕
    async fn delete(&self, root: &str, rel: &str, expect: &str) -> Result<(), Refused>;
    // 〔MIG-3a〕「只删一个空目录」那一形（〔FW1〕`delete_empty_dir`〔散文墓碑〕）随卸 skill 进后端删了：收空目录今天在那台后端里（`assets/skill_flow.rs`）。
    async fn chmod(&self, root: &str, rel: &str, mode: u32) -> Result<(), String>;
    async fn delete_session(&self, sid: &str) -> Result<String, String>;
    /// 一个路径**在不在**（`files-stat`）：在 ⇒ `Some(kind)`；对端答「读不到」⇒ `None`。
    /// ⚠ 「读不到」与「不存在」在这一问上分不开（权限不够也是 `None`）—— 只给「在不在场」那种展示用。
    async fn stat_kind(&self, path: &str) -> Result<Option<String>, String>;
    /// 列一个目录（`files-ls`）：`(名字, 是不是目录)`。
    async fn list_dir(&self, path: &str) -> Result<Vec<(String, bool)>, String>;
}

/// 读改写一次最多重来几趟（`stale` 才重来：盘上那份在读与写之间被别人改了）。
pub(crate) const EDIT_ATTEMPTS: usize = 3;

/// 一次 [`edit`] 的结局。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Edited {
    /// 规划说「没事可做」，或算出来与盘上逐字相同 ⇒ 一个字节没写。
    Unchanged,
    Written(Landed),
}

/// 🔴 **读 → 算 → 交**（写的规则不在这里，在后端）。
///
/// `plan` 拿到读到的全文（`None` = 不存在），回 `Some(新全文)` 或 `None`（没事可做）。
/// 后端回 `stale` ⇒ 重读重算，最多 [`EDIT_ATTEMPTS`] 趟；`plan` 因此是 `FnMut`（每趟对新读到的那一份再算一遍）。
pub(crate) async fn edit<D: Door>(
    door: &D,
    root: &str,
    rel: &str,
    backup: bool,
    parents: bool,
    mut plan: impl FnMut(Option<&str>) -> Result<Option<String>, String>,
) -> Result<Edited, String> {
    let mut last = String::new();
    for _ in 0..EDIT_ATTEMPTS {
        let got = door.peek(root, rel).await?;
        let Some(next) = plan(got.text.as_deref())? else {
            return Ok(Edited::Unchanged);
        };
        if got.text.as_deref() == Some(next.as_str()) {
            return Ok(Edited::Unchanged);
        }
        match door
            .put(root, rel, &next, got.text.as_deref(), backup, parents)
            .await
        {
            Ok(landed) if landed.changed => return Ok(Edited::Written(landed)),
            Ok(_) => return Ok(Edited::Unchanged),
            Err(Refused::Stale(s)) => last = s,
            Err(e @ (Refused::Other(_) | Refused::Peer { .. })) => return Err(e.said()),
        }
    }
    Err(copy_text(
        "rsUserFiles.edit.gaveUp",
        &[
            ("last", &last.to_string()),
            ("attempts", &EDIT_ATTEMPTS.to_string()),
        ],
    ))
}

/// `abs` 在 `home` 底下的那一段（给 `root = home` 的那几处用：rc / profile）。
///
/// ⚠ 两边都是**字符串**：本机与远端同一种算法（远端路径在这台机器上没法 `canonicalize`）。
/// 不在 home 底下 ⇒ 拒（与 `profile_installer::fence_lexical` 的「只能落在 home 之内」同一句承诺）。
pub(crate) fn rel_under(home: &str, abs: &str) -> Result<String, String> {
    let norm = |s: &str| s.replace('\\', "/");
    let (h, a) = (norm(home), norm(abs));
    let h = h.trim_end_matches('/');
    let rest = a
        .strip_prefix(h)
        .and_then(|r| r.strip_prefix('/'))
        .filter(|r| !r.is_empty())
        .ok_or_else(|| {
            copy_text(
                "rsUserFiles.rel.outsideHome",
                &[("abs", &abs.to_string()), ("home", &home.to_string())],
            )
        })?;
    Ok(rest.to_string())
}

/// 〔AL2 · 第四波 4D〕[`rel_under`] 的反方向：`home` 底下的相对段 `rel`（`/` 分隔，交给后端的那一形）→ 那台机器上的绝对路径。
///
/// ⚠ 同样是**字符串**、本机与远端同一种算法：分隔符跟 `home` 自己的写法走（`home` 里有 `\` 而没有 `/` ⇒ `\`，否则 `/`）。
/// 不用 `std::path::Path::join` —— 那是**本机**的分隔符：Windows 上的 monitor 拿它拼远端 `/home/zbl` 会得到
/// `/home/zbl\.bashrc`；反过来 Windows 的 home 整串拼 `/` 分隔的 `rel` 又是 WIN1 F8 那一形（`C:\Users\zbl\.cc-monitor/aliases.ps1`）。
pub(crate) fn join_under(home: &str, rel: &str) -> String {
    let sep = if home.contains('\\') && !home.contains('/') {
        '\\'
    } else {
        '/'
    };
    let mut out = home.trim_end_matches(['/', '\\']).to_string();
    for seg in rel.split(['/', '\\']).filter(|s| !s.is_empty()) {
        out.push(sep);
        out.push_str(seg);
    }
    out
}

/// 后端入方向一行的上限（`src/backend/inbound.rs::MAX_LINE_BYTES` 的本侧镜像；两个 crate 引不到对方 ⇒
/// `byte_cap_registry` 读两侧源码钉相等）。读改写的写那一半把新内容与读到的那一份装进同一行请求。
pub(crate) const REQUEST_LINE_CAP: usize = 1 << 20;

/// 后端那一侧一趟最多等多久。写面全是同步文件 I/O，秒级内完成；给足余量同时防卡死。
const DOOR_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

/// 生产那一扇门：**经这台机器的后端**（`inbound_client::client_for(origin)`）。本机与远端同一份。
pub(crate) struct BackendDoor {
    pub origin: crate::origin::Origin,
}

impl BackendDoor {
    pub(crate) fn new(origin: crate::origin::Origin) -> Self {
        Self { origin }
    }

    /// 发一条写面命令，拿它的 `data`。失败翻成人话；**对端回 `stale` 单列**。
    /// 〔E2〕`pub(crate)`：`ccm_probe::probe_ccm_cli` 经同一扇门问那台后端 `ccm-probe`（不另开一条通道）。
    pub(crate) async fn ask(
        &self,
        cmd: &str,
        args: serde_json::Value,
    ) -> Result<serde_json::Value, Refused> {
        let wire = self.origin.as_wire_str();
        let who = crate::backend::control::cc_bus::machine_label(wire);
        let Some(client) = client_for(wire) else {
            let why = match no_channel(wire) {
                Routed::NoChannel(s) | Routed::Refused(s) => s,
                Routed::Done => String::new(),
            };
            return Err(Refused::Other(copy_text(
                "rsUserFiles.ask.backendDown",
                &[("who", &who.to_string()), ("why", &why.to_string())],
            )));
        };
        if !client.accepts(cmd) {
            return Err(Refused::Other(
                crate::backend::control::cc_bus::describe_backend_too_old_for(
                    wire,
                    cmd,
                    &copy_text("rsUserFiles.ask.notDone", &[]),
                ),
            ));
        }
        // 请求一行装不装得下：后端一行上限 1 MiB（`inbound::MAX_LINE_BYTES`，本侧的镜像是
        // [`REQUEST_LINE_CAP`]）。装不下当场说清，不发 —— 发了只会换来一句 `line_too_long`。
        let line = crate::backend::control::inbound_client::encode_request("0", cmd, &args);
        if line.len() > REQUEST_LINE_CAP {
            return Err(Refused::Other(copy_text(
                "rsUserFiles.ask.tooBig",
                &[
                    ("bytes", &(line.len()).to_string()),
                    ("cap", &REQUEST_LINE_CAP.to_string()),
                ],
            )));
        }
        // 对端说了话时它给的那个码（分流器递回来的 `(code, message)` 里认，不自己 match 错误枚举）。
        let peer_code: std::cell::RefCell<Option<String>> = std::cell::RefCell::new(None);
        match client.call(cmd, args, DOOR_TIMEOUT).await {
            Ok(Some(v)) => Ok(v),
            Ok(None) => Err(Refused::Other(copy_text(
                "rsUserFiles.ask.emptyReply",
                &[("who", &who.to_string())],
            ))),
            Err(e) => {
                let said = match route_call_error(&e, |code, message| {
                    *peer_code.borrow_mut() = Some(code.to_string());
                    format!("{who}：{message}")
                }) {
                    Routed::NoChannel(s) | Routed::Refused(s) => s,
                    Routed::Done => String::new(),
                };
                Err(match peer_code.into_inner() {
                    Some(c) if c == "stale" => Refused::Stale(said),
                    Some(code) => Refused::Peer { code, said },
                    None => Refused::Other(said),
                })
            }
        }
    }
}

/// 后端交回来的路径（字符串或 `{"b16": …}`）→ 给人看的一行。
fn path_text(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => s.clone(),
        other => other
            .get("b16")
            .and_then(serde_json::Value::as_str)
            .and_then(|h| {
                (0..h.len())
                    .step_by(2)
                    .map(|i| u8::from_str_radix(h.get(i..i + 2)?, 16).ok())
                    .collect::<Option<Vec<u8>>>()
            })
            .map(|b| String::from_utf8_lossy(&b).into_owned())
            .unwrap_or_default(),
    }
}

impl BackendDoor {
    /// 门上**唯一**那一问 `files-delete`（〔RM1e〕判据钉它恰好一处、每次都带 `expect`：逐字节那一形）。
    async fn delete_expecting(
        &self,
        root: &str,
        rel: &str,
        expect: serde_json::Value,
    ) -> Result<(), Refused> {
        self.ask(
            "files-delete",
            serde_json::json!({ "root": root, "rel": rel, "expect": expect }),
        )
        .await
        .map(|_| ())
    }
}

impl Door for BackendDoor {
    fn machine(&self) -> String {
        crate::backend::control::cc_bus::machine_label(self.origin.as_wire_str())
    }

    async fn home(&self) -> Result<String, String> {
        let v = self
            .ask("files-home", serde_json::json!({}))
            .await
            .map_err(Refused::said)?;
        let p = path_text(v.get("path").unwrap_or(&serde_json::Value::Null));
        if p.is_empty() {
            return Err(copy_text(
                "rsUserFiles.home.unknown",
                &[("machine", &(self.machine()).to_string())],
            ));
        }
        Ok(p)
    }

    async fn peek(&self, root: &str, rel: &str) -> Result<Peeked, String> {
        let v = self
            .ask(
                "files-peek",
                serde_json::json!({ "root": root, "rel": rel }),
            )
            .await
            .map_err(Refused::said)?;
        Ok(Peeked {
            path: path_text(v.get("path").unwrap_or(&serde_json::Value::Null)),
            text: v
                .get("text")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string),
        })
    }

    async fn put(
        &self,
        root: &str,
        rel: &str,
        content: &str,
        expect: Option<&str>,
        backup: bool,
        parents: bool,
    ) -> Result<Landed, Refused> {
        let v = self
            .ask(
                "files-put",
                serde_json::json!({
                    "root": root,
                    "rel": rel,
                    "content": content,
                    "expect": expect,
                    "backup": backup,
                    "parents": parents,
                }),
            )
            .await?;
        let flag = |k: &str| {
            v.get(k)
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false)
        };
        Ok(Landed {
            path: path_text(v.get("path").unwrap_or(&serde_json::Value::Null)),
            changed: flag("changed"),
            created: flag("created"),
            backup: v.get("backup").filter(|b| !b.is_null()).map(path_text),
        })
    }

    async fn rename(&self, root: &str, from: &str, to: &str) -> Result<(), String> {
        self.ask(
            "files-rename",
            serde_json::json!({ "root": root, "from": from, "to": to }),
        )
        .await
        .map(|_| ())
        .map_err(Refused::said)
    }

    async fn delete(&self, root: &str, rel: &str, expect: &str) -> Result<(), Refused> {
        self.delete_expecting(root, rel, serde_json::json!(expect))
            .await
    }

    async fn chmod(&self, root: &str, rel: &str, mode: u32) -> Result<(), String> {
        self.ask(
            "files-chmod",
            serde_json::json!({ "root": root, "rel": rel, "mode": mode }),
        )
        .await
        .map(|_| ())
        .map_err(Refused::said)
    }

    async fn delete_session(&self, sid: &str) -> Result<String, String> {
        let v = self
            .ask("files-delete-session", serde_json::json!({ "sid": sid }))
            .await
            .map_err(Refused::said)?;
        Ok(path_text(v.get("path").unwrap_or(&serde_json::Value::Null)))
    }

    async fn stat_kind(&self, path: &str) -> Result<Option<String>, String> {
        match self
            .ask("files-stat", serde_json::json!({ "path": path }))
            .await
        {
            Ok(v) => Ok(Some(
                v.get("kind")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("")
                    .to_string(),
            )),
            Err(Refused::Peer { code, .. }) if code == "unreadable" => Ok(None),
            Err(e) => Err(e.said()),
        }
    }

    async fn list_dir(&self, path: &str) -> Result<Vec<(String, bool)>, String> {
        let v = self
            .ask(
                "files-ls",
                serde_json::json!({ "path": path, "limit": LIST_LIMIT }),
            )
            .await
            .map_err(Refused::said)?;
        let entries = v
            .get("entries")
            .and_then(serde_json::Value::as_array)
            .ok_or_else(|| {
                copy_text(
                    "rsUserFiles.list.notArray",
                    &[("machine", &(self.machine()).to_string())],
                )
            })?;
        Ok(entries
            .iter()
            .map(|e| {
                let p = path_text(e.get("path").unwrap_or(&serde_json::Value::Null));
                let name = p.rsplit(['/', '\\']).next().unwrap_or("").to_string();
                let is_dir = e.get("kind").and_then(serde_json::Value::as_str) == Some("dir");
                (name, is_dir)
            })
            .filter(|(n, _)| !n.is_empty())
            .collect())
    }
}

/// [`Door::list_dir`] 一次最多要多少项（skill 的实例目录是几个到几十个，不是一个大目录）。
const LIST_LIMIT: usize = 1000;

#[cfg(test)]
#[path = "../../../tests/bridge/user_files_tests.rs"]
pub(crate) mod tests;
