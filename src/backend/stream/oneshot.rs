//! **手写的一次性子命令**：`(子命令, 用法, 一句话, 实现)` 一张表。`main` 查它分派，闸门 `stream::cli::is_subcommand`
//! 与列全部命令的口（`stream::catalog`，行序就是本表的先后）照它认（文档料，不是对外文案）。
//!
//! 别的子命令都从 `inbound::REGISTRY` 派生（`stream::cli::handles`）。表里一行没有实现编不过。

/// 一次性子命令拿到的：剥完流旗标的 argv（打头是子命令本身）· 看的那一台的时区 · 启动入口造的那一份写句柄。
pub struct Call<'a> {
    pub args: &'a [String],
    pub tz: &'a crate::common::time::Tz,
    pub own: &'a crate::platform::Own,
}

/// 一条一次性子命令跑完的退出码。
pub type Ran<'a> = std::pin::Pin<Box<dyn std::future::Future<Output = i32> + 'a>>;

/// 一条手写的一次性子命令的实现。
pub type Oneshot = for<'a> fn(Call<'a>) -> Ran<'a>;

/// `(子命令, 用法（跟在子命令后面的词，没有 ⇒ 空串）, 一句话, 实现)`。
pub const ONESHOTS: &[(&str, &str, &str, Oneshot)] = &[
    (
        "--commands",
        "",
        "列全部命令与用途：stdout 一行 `{commands: [{cmd?, cli?, usage?, what, args?}]}`（`cmd` 帧命令名 · `cli` 子命令 · `what` 一句话 · `args` 收不收参数；只在流上的没有 `cli`，CLI 独有的没有 `cmd`）",
        |_| Box::pin(async { crate::stream::catalog::run() }),
    ),
    // 判「终端那边答了」读活会话表（hook 在 control 层够不着 history，由这里交进去）。
    (
        "--answer-hook",
        "<agent>",
        "会话级答题 hook 的客户端（那一家的 hook 设置里直接叫它，不经 shell）：读 hook 的 stdin，经本机常驻后端 `needs-ask` 挂上、等人答；拿到答案 ⇒ stdout 打那一家要的那一整份；常驻不在 · 断开 · 不是要人答的那一种 · 终端那边先答了 ⇒ 一个字节不打。恒退出 0",
        |c| {
            Box::pin(crate::control::asks::hook::run(
                c.args,
                crate::history::live_table::states_in,
            ))
        },
    ),
    (
        "--place-self",
        "",
        "部署那一趟：暂存区里新传上去的这一份读自己的字节，自报是这一版才 `rename(2)` 盖掉落点 `~/.cc-monitor/bin/ccm`；回 `{placed: true, build_id}`；不在暂存区 ⇒ `outside_staging`；自报不是这一版 ⇒ `not_self`、暂存件当场删",
        |c| Box::pin(async move { crate::control::place_self::run(c.own) }),
    ),
    (
        "--unplace-self",
        "",
        "卸载那一趟：暂存区里传上去的这一份删掉落点 `~/.cc-monitor/bin/ccm`（不认身份）、再删自己；回 `{removed: true|false}`；不在暂存区 ⇒ `outside_staging`",
        |c| Box::pin(async move { crate::control::place_self::run_unplace(c.own) }),
    ),
    // 远端常驻后端的起 · 找 · 问 · 停（`control/resident.rs` 头注）。
    (
        "--resident-attach",
        "",
        "小中继：连这台家里常驻后端的套接字，stdin → 套接字、套接字 → stdout 原样对拷，任一边断就退；断开 ⇒ stdout 一行 `{attach:\"refused\", reason: absent|unreachable}`、退出 2",
        |_| Box::pin(crate::control::resident::attach()),
    ),
    (
        "--resident-call",
        "[--stdin-line] --within-ms <毫秒> [--tz <IANA 名>]",
        "问这台的常驻后端一条帧命令（stdin 一行 `{cmd, args}`）：连套接字、握手（在看名单交空的）、发请求、等那一条应答，stdout 一行回包、退出 0；常驻答了失败 ⇒ stderr 一行它那一份失败信封、退出 2；没人在听 ⇒ `resident_absent`（不起冷进程顶上）；`--within-ms` 必给（等多久归发起方，缺 ⇒ `bad_args`），到期限没回 ⇒ `timed_out`",
        |c| Box::pin(crate::control::resident::call(&c.args[1..], c.tz)),
    ),
    (
        "--resident-ensure",
        "[--replace]",
        "确保这台的常驻后端在听：已在 ⇒ `{pid, replaced: false}`（pid 是套接字对端）；没在 ⇒ 起一个脱离的自己、回 `{pid, replaced}`；`--replace` 在听那一位比自己旧才停掉再起（只升不降）",
        |c| Box::pin(async move { crate::control::resident::ensure(&c.args[1..], c.own) }),
    ),
    (
        "--resident-stop",
        "[--grace <秒>]",
        "停这台的常驻后端：问套接字在听的是谁 → SIGTERM → 宽限（缺省 35 秒）→ SIGKILL；回 `{stopped: graceful|killed|not_running, pid}`",
        |c| Box::pin(async move { crate::control::resident::run_stop(&c.args[1..], c.own) }),
    ),
    (
        "--tmux-notify",
        "<backend_pid> <backend_starttime>",
        "tmux 钩子用：核对身份后叫正在跑的后端立刻重扫 tmux；身份对不上静默退出 0；不碰文件系统",
        |c| Box::pin(async move { crate::control::tmux_hook::notify(c.args) }),
    ),
];

/// 表里这一条子命令的实现。
pub fn oneshot(flag: &str) -> Option<Oneshot> {
    ONESHOTS.iter().find(|(t, ..)| *t == flag).map(|(.., f)| *f)
}
