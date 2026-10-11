//! 命令表 · 连接本身：`cancel` · `ping` · 链路四条 · 传输四条（`Run::Builtin` 那几条的本体是 `dispatch` 的硬臂）。

use crate::stream::inbound::spec::{arg, out, CommandSpec, Run};

pub(super) const SPECS: &[CommandSpec] = &[
    CommandSpec {
        name: "cancel",
        summary: "撤掉一条在跑的命令（`target` = 它的 `id`）；撤不存在的 id 也回 `ok`",
        codes: &[],
        fields: &[arg("target", "要撤的那条命令的 `id`")],
        takes_input: true,
        run: Run::Builtin,
    },
    // 流只发要看的会话：本连接那份 watcher 的名单 ⇒ **只在帧面**。
    CommandSpec {
        name: "stream-watch",
        summary: "这条流此刻在看哪几个会话（整份换；从没报过 ＝ 全看）：不在名单里的会话，它的行 · 运行表 · 主线外清单不上这条流，宣告 · 状态 · 轮次边沿照发；回刚进名单的那几个从第几行起上流",
        codes: &["bad_args"],
        fields: &[arg("sids", "在看的会话 id（串数组；空 ＝ 一个都不看）"), out("from", "刚进名单的会话每份记录一格 `{sid, path, seq}`（先前就在名单里的不回）"), out("path", "`from[]`：那份记录在那台机器上的绝对路径"), out("seq", "`from[]`：这条流从这一行起发它（与行帧同一个行号空间）；`[0, seq)` 按骨架补"), out("sid", "`from[]`：会话 id")],
        takes_input: true,
        run: Run::Builtin,
    },
    // **链路四条** —— 本机只常驻一个后端，
    // 到各远端的 SSH 连接由它持有、按拨号身份复用（`dial/pool.rs`）；monitor 经这条流开「链路」，
    // 链路上的字节与 C2 那个 `--dial` 子进程的 stdout 逐字节同形（`dial/mod.rs` 头注）。
    // 四条都是 `Run::Builtin`：要碰本连接的链路表 ⇒ **只在帧面**，CLI 面不派生（一次性进程没有「连接」可言）。
    CommandSpec {
        name: "link-open",
        summary: "开一条链路",
        codes: &[
            "bad_args",
            "unsupported_use",
            "duplicate_link",
            "too_many_links",
        ],
        fields: &[arg("dial", "拨号请求（`host` · `port` · `user` · `key_path` · `use` …，蛇形键）；`use:\"files\"` 开 sftp 一问一答"), arg("link", "链路 id：客户端给、客户端负责唯一"), arg("window", "初始下行信用（字节），在 [32 KiB, 16 MiB] 之内")],
        takes_input: true,
        run: Run::Builtin,
    },
    CommandSpec {
        name: "link-data",
        summary: "往链路里送一块上行字节",
        codes: &["bad_args", "no_such_link", "link_busy", "link_closed"],
        fields: &[arg("data", "无"), arg("link", "链路 id")],
        takes_input: true,
        run: Run::Builtin,
    },
    CommandSpec {
        name: "link-credit",
        summary: "还下行信用",
        codes: &["bad_args", "no_such_link"],
        fields: &[arg("bytes", "客户端读走了多少字节（累计信用不超过 16 MiB）"), arg("link", "链路 id")],
        takes_input: true,
        run: Run::Builtin,
    },
    CommandSpec {
        name: "link-close",
        summary: "关一条链路",
        codes: &["bad_args"],
        fields: &[arg("link", "链路 id；关不存在的也回 `ok`")],
        takes_input: true,
        run: Run::Builtin,
    },
    // **传输四条** —— 用户「SFTP 进本机常驻后端，只写暂存区」：传输台从 monitor 搬进
    // 本机常驻后端。四条都是 `Run::Builtin`：要碰本连接的票表与应答通道 ⇒ **只在帧面**。
    CommandSpec {
        name: "transfer-upload",
        summary: "开一张上传单",
        codes: &["bad_args", "io_failed", "busy", "too_many_transfers"],
        fields: &[arg("dial", "拨号请求，同 `link-open`"), arg("id", "传输单 id（`xfer-<n>`）"), arg("key", "暂存件的键（32 位十六进制）：同一份文件重拖同一个键 ⇒ 续传；提交时交给远端"), arg("local_path", "本机一份普通文件的路径")],
        takes_input: true,
        run: Run::Builtin,
    },
    CommandSpec {
        name: "transfer-download",
        summary: "开一张下载单",
        codes: &["bad_args", "refused", "too_many_transfers"],
        fields: &[arg("dial", "拨号请求，同 `link-open`"), arg("id", "传输单 id"), arg("local_path", "本机落点（绝对路径；也收 `{\"b16\":…}`）"), arg("remote_path", "远端路径")],
        takes_input: true,
        run: Run::Builtin,
    },
    CommandSpec {
        name: "transfer-start",
        summary: "传输单起跑",
        codes: &["bad_args", "no_such_transfer", "already_started"],
        fields: &[arg("id", "传输单 id；之后进度与终局走 `transfer` 帧")],
        takes_input: true,
        run: Run::Builtin,
    },
    CommandSpec {
        name: "transfer-stop",
        summary: "撤一张传输单",
        codes: &["bad_args"],
        fields: &[arg("id", "传输单 id；撤不在册的也回 `ok`")],
        takes_input: true,
        run: Run::Builtin,
    },
    CommandSpec {
        name: "ping",
        summary: "问活：零载荷，回 `ok`",
        codes: &[],
        fields: &[],
        takes_input: false,
        run: Run::Async(|_r| Box::pin(async move { Ok(None) })),
    },
];
