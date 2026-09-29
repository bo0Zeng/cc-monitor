//! 〔RW1 · 第四波 · 2026-09-24〕**monitor 进程够用户文件的唯一开口 —— 它一个字节都不自己落盘。**
//!
//! 用户逐字：「现在只允许后端的文件管理部分写文件」；追问后裁「**只管用户的文件**」「**也管本机**」。
//! ⇒ rc 里的别名块 · PowerShell `$PROFILE` · 项目 `.mcp.json` · skill 收件箱 · `~/.claude/skills/cc-bus/`，
//! 这些改动从此都经**那台机器上的后端**（`files-peek` / `files-put` / `files-rename` /
//! `files-chmod`），本机与远端**同一条路**，只差 origin。〔MIG-3b〕删历史会话不经这扇门了：界面经通道直说那台后端。
//! 〔RM1d → MIG-3b 续〕代码全景的批注 / 文档关联不经这扇门了：算与写都在那台后端（`panorama-edit`，界面经通道直问）。
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
    // 〔MIG-3b 续〕交写那一形（`put`，最后一个用户是全景的批注 / 文档关联）随全景写进那台后端（`panorama-edit`）删了。
    // 〔MIG-3a · 子步 3〕改名那一形（`rename`〔散文墓碑〕，唯一用户是 cc-bus 装前的整目录备份）随装 cc-bus 进后端删了。
    /// 〔RM1d〕删**一个文件**（`files-delete`，非递归；落点同一道围栏）。〔MIG-3b 续〕今天唯一的用户：`ccm_legacy.rs` 删旧入口（全景删批注那一处随全景写进了那台后端）。
    /// 〔RM1e〕带 CAS：`expect` = 读到的那一份，盘上逐字节等于它才删；不等 / 已经不在 ⇒ [`Refused::Stale`]，一个字节不动。
    /// 〔墓碑 —— RM1d 那一版这里写着「没有 CAS（后端这条命令不收 `expect`），调用方先 `peek` 核一遍」。〕
    async fn delete(&self, root: &str, rel: &str, expect: &str) -> Result<(), Refused>;
    // 〔MIG-3a〕「只删一个空目录」那一形（〔FW1〕`delete_empty_dir`〔散文墓碑〕）随卸 skill 进后端删了：收空目录今天在那台后端里（`assets/skill_flow.rs`）。
    // 〔MIG-3b 续〕改权限那一形（`chmod`，唯一用户是公钥推送）随推送进本机后端删了。
    // 〔MIG-3b〕`delete_session` 那一问走了：删会话由界面经通道直说那台后端（`src/frontend/ui/session-writes.ts`），门不再转交。
    // 〔MIG-3a〕「一个路径在不在」那一形（`stat_kind`〔散文墓碑〕）的用户（别名读回 · cc-bus 装）都进了后端 ⇒ 删。
    // 〔MIG-3a〕「列一个目录」那一形（`list_dir`〔散文墓碑〕）随收件箱进后端删了：列 skill 实例今天在那台后端里（`agents/claudecode/skill_host.rs`）。
}

// 〔MIG-3b 续〕读 → 算 → 交那一环（`edit`〔散文墓碑〕 与它的结局 `Edited`〔散文墓碑〕、重来趟数）删了：最后一个用户（公钥推送）进了本机后端，
//   那一环在后端还有一份（`src/backend/assets/door.rs` 的 `edit`，本机后端代管资产与 `authorized-keys-add` 都走它）。

// 〔MIG-3a〕`rel_under` / `join_under`〔散文墓碑〕两个字符串拼法随别名那一族进了那台后端（`src/backend/assets/door.rs` 那一份），monitor 零调用方 ⇒ 删。

/// 后端入方向一行的上限（`src/backend/stream/inbound.rs::MAX_LINE_BYTES` 的本侧镜像；两个 crate 引不到对方 ⇒
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
    /// 〔E2〕`pub(crate)`：`ccm_probe::probe_ccm_cli` 经同一扇门问那台后端 `ccm-probe`（不另开一条通道）。〔散文墓碑〕
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

    async fn delete(&self, root: &str, rel: &str, expect: &str) -> Result<(), Refused> {
        self.delete_expecting(root, rel, serde_json::json!(expect))
            .await
    }
}

#[cfg(test)]
#[path = "../../../../tests/frontend/shell/user_files_tests.rs"]
pub(crate) mod tests;
