//! **手写的一次性子命令**：`(子命令, 实现)` 一张表，`main` 查它分派。
//!
//! 别的子命令都从 `inbound::REGISTRY` 派生（`stream::cli::handles`）。这几条的名字 · 用法 · 一句话住 `inbound::CLI_ONLY_DOCS`
//! （闸门 `stream::cli::is_subcommand` 与列全部命令的口照它认），本表与它两向相等由测试钉；表里一行没有实现编不过。
//! 本表单住一份文件：`stream::cli` 只认名字、不引用这里，帧面命令的处理器经它不连到这几条实现的依赖上。

/// 一次性子命令拿到的：剥完流旗标的 argv（打头是子命令本身）· 看的那一台的时区 · 启动入口造的那一份写句柄。
pub struct Call<'a> {
    pub args: &'a [String],
    pub tz: &'a crate::Tz,
    pub own: &'a crate::platform::Own,
}

/// 一条一次性子命令跑完的退出码。
pub type Ran<'a> = std::pin::Pin<Box<dyn std::future::Future<Output = i32> + 'a>>;

/// 一条手写的一次性子命令的实现。
pub type Oneshot = for<'a> fn(Call<'a>) -> Ran<'a>;

/// `(子命令, 实现)`。
pub const ONESHOTS: &[(&str, Oneshot)] = &[
    // hook 子进程走这条：校验身份后给后端发 SIGUSR1，不碰文件系统。
    ("--tmux-notify", |c| {
        Box::pin(async move { crate::control::tmux_hook::notify(c.args) })
    }),
    // 远端常驻后端的起 · 找 / 停（`control/resident.rs` 头注）。
    ("--resident-ensure", |c| {
        Box::pin(async move { crate::control::resident::ensure(&c.args[1..], c.own) })
    }),
    ("--resident-attach", |_| {
        Box::pin(crate::control::resident::attach())
    }),
    // 后端问后端：问这台常驻一条帧命令（只问常驻，不在这里起冷的顶上）。
    ("--resident-call", |c| {
        Box::pin(crate::control::resident::call(&c.args[1..], c.tz))
    }),
    // 会话级答题 hook 的客户端：经本机常驻后端问人、把答案交回那一家；没答案就空手退 0。
    // 判「终端那边答了」读活会话表（hook 在 control 层够不着 history，由这里交进去）。
    ("--answer-hook", |c| {
        Box::pin(crate::control::asks::hook::run(
            c.args,
            crate::history::live_table::states_in,
        ))
    }),
    ("--resident-stop", |c| {
        Box::pin(async move { crate::control::resident::run_stop(&c.args[1..], c.own) })
    }),
    // 部署那一趟：暂存区里新传上去的那一份自己核字节、换进落点。
    ("--place-self", |c| {
        Box::pin(async move { crate::control::place_self::run(c.own) })
    }),
    // 卸载那一趟：暂存区里传上去的那一份删掉落点那一份、再删自己。
    ("--unplace-self", |c| {
        Box::pin(async move { crate::control::place_self::run_unplace(c.own) })
    }),
    // 列全部命令与用途（帧命令 · 子命令 · 一句话）。
    ("--commands", |_| {
        Box::pin(async { crate::stream::catalog::run() })
    }),
];

/// 表里这一条子命令的实现。
pub fn oneshot(flag: &str) -> Option<Oneshot> {
    ONESHOTS.iter().find(|(t, _)| *t == flag).map(|(_, f)| *f)
}
