//! 命令表 · 连接本身：`cancel` · `ping` · 链路四条 · 传输四条（`Run::Builtin` 那几条的本体是 `dispatch` 的硬臂）。

use crate::stream::inbound::spec::{CommandSpec, Run};

pub(super) const SPECS: &[CommandSpec] = &[
    CommandSpec {
        name: "cancel",
        doc_anchor: None,
        codes: &[],
        fields: &["target"],
        takes_input: true,
        run: Run::Builtin,
    },
    // **链路四条** —— 本机只常驻一个后端，
    // 到各远端的 SSH 连接由它持有、按拨号身份复用（`dial/pool.rs`）；monitor 经这条流开「链路」，
    // 链路上的字节与 C2 那个 `--dial` 子进程的 stdout 逐字节同形（`dial/mod.rs` 头注）。
    // 四条都是 `Run::Builtin`：要碰本连接的链路表 ⇒ **只在帧面**，CLI 面不派生（一次性进程没有「连接」可言）。
    CommandSpec {
        name: "link-open",
        doc_anchor: Some("#### `link-open`"),
        codes: &[
            "invalid_args",
            "unsupported_use",
            "duplicate_link",
            "too_many_links",
        ],
        fields: &["dial", "link", "window"],
        takes_input: true,
        run: Run::Builtin,
    },
    CommandSpec {
        name: "link-data",
        doc_anchor: Some("#### `link-data`"),
        codes: &["invalid_args", "no_such_link", "link_busy", "link_closed"],
        fields: &["data", "link"],
        takes_input: true,
        run: Run::Builtin,
    },
    CommandSpec {
        name: "link-credit",
        doc_anchor: Some("#### `link-credit`"),
        codes: &["invalid_args", "no_such_link"],
        fields: &["bytes", "link"],
        takes_input: true,
        run: Run::Builtin,
    },
    CommandSpec {
        name: "link-close",
        doc_anchor: Some("#### `link-close`"),
        codes: &["invalid_args"],
        fields: &["link"],
        takes_input: true,
        run: Run::Builtin,
    },
    // **传输四条** —— 用户「SFTP 进本机常驻后端，只写暂存区」：传输台从 monitor 搬进
    // 本机常驻后端。四条都是 `Run::Builtin`：要碰本连接的票表与应答通道 ⇒ **只在帧面**。
    CommandSpec {
        name: "transfer-upload",
        doc_anchor: Some("#### `transfer-upload`"),
        codes: &["bad_args", "io_failed", "busy", "too_many_transfers"],
        fields: &["dial", "id", "key", "local_path"],
        takes_input: true,
        run: Run::Builtin,
    },
    CommandSpec {
        name: "transfer-download",
        doc_anchor: Some("#### `transfer-download`"),
        codes: &["bad_args", "refused", "too_many_transfers"],
        fields: &["dial", "id", "local_path", "remote_path"],
        takes_input: true,
        run: Run::Builtin,
    },
    CommandSpec {
        name: "transfer-start",
        doc_anchor: Some("#### `transfer-start`"),
        codes: &["bad_args", "no_such_transfer", "already_started"],
        fields: &["id"],
        takes_input: true,
        run: Run::Builtin,
    },
    CommandSpec {
        name: "transfer-stop",
        doc_anchor: Some("#### `transfer-stop`"),
        codes: &["bad_args"],
        fields: &["id"],
        takes_input: true,
        run: Run::Builtin,
    },
    CommandSpec {
        name: "ping",
        doc_anchor: None,
        codes: &[],
        fields: &[],
        takes_input: false,
        run: Run::Async(|_r| Box::pin(async move { Ok(None) })),
    },
];
