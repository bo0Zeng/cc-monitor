//! CLI 独有子命令的说明：协议参考 `IPC-COMMANDS.md` 的 CLI 那一节由它生成（文档料，不是对外文案）。

/// CLI 独有的子命令（`SUBCOMMANDS` 里不从 `inbound::REGISTRY` 派生的那几条）：`(子命令名（不带打头的 `--`）, 用法, 一句话)`。
/// 协议参考 `IPC-COMMANDS.md` 的 CLI 那一节由它生成；漏一条、多一条生成器都当场失败。
pub const CLI_ONLY_DOCS: &[(&str, &str, &str)] = &[
    ("backend-probe", "", "能力探测：回 `{proto, buildId, commands}`，`commands` 是这台真能派发的 CLI 控制面子命令；不读 stdin"),
    ("resident-attach", "", "小中继：连这台家里常驻后端的套接字，stdin → 套接字、套接字 → stdout 原样对拷，任一边断就退；断开 ⇒ stdout 一行 `{attach:\"refused\", reason: absent|unreachable}`、退出 2"),
    ("resident-ensure", "[--replace]", "确保这台的常驻后端在听：已在 ⇒ `{pid}`（在听那一位，读不到记录 ⇒ `null`）；没在 ⇒ 起一个脱离的自己、回 `{pid}`；`--replace` 先停掉在听那一位再起"),
    ("resident-stop", "[--grace <秒>]", "停这台的常驻后端：核身份 → SIGTERM → 宽限（缺省 35 秒）→ SIGKILL；回 `{stopped: graceful|killed|not_running, pid}`"),
    ("tmux-notify", "<backend_pid> <backend_starttime>", "tmux 钩子用：核对身份后叫正在跑的后端立刻重扫 tmux；身份对不上静默退出 0；不碰文件系统"),
];
