//! 进后端的口（最上面那一层）：命令表 · 帧面的分派与取消 · CLI 面的分派 · 对后端其余部分开的门 · 总期限 · 出口声明 · tee 的消费侧（活卡 `live_card` 帧）。
//! 引所有功能；除 `lib.rs` 以外谁都不引它（`layering_guard` 钉着）。各功能要的协议零件（帧 · typed 命令 · 登记形状 · 复制详情 …）住 [`crate::wire`]。
//! 本机后端问远端后端的那一跳（跨机问答原语）不在这里，住 `dial/remote_ask.rs`（池里那条 SSH 上问一次）。

pub mod catalog; // 列全部命令与用途的那个口（`ccm -- --commands`）：协议参考与 skill 说明书的命令段都照它现打
pub mod claim; // 起进程那一下：选载体 · 抢门牌，抢到了才接 stderr 落盘（`main` 调它一次）
pub(crate) mod entry; // 进程入口：起来那一下的装配 · 分流 · 两条载体（库面对外只有它的 `run`，`main.rs` 调一次）
pub mod cli; // 一次性 CLI 面：与帧面共用命令表里同一个 `run`（原 `control/stream::cli.rs`）
pub mod inbound; // U6b-1：流连接上的入方向（信封 / 分派 / 取消）＋ 命令表
pub(crate) mod live; // 活卡成品（整台一份）：对账 · 缺口 · 记录到了整轮覆盖 · 子运行归位 · 截头 · 每条连接的发件箱
pub mod oneshot; // 手写的一次性子命令：`(子命令, 实现)` 一张表，`main` 查它分派
pub(crate) mod resync_face; // `resync`（手动对齐）：回包那一栏 `unavailable` 是命令表派生的 ⇒ 住命令表这一层
pub(crate) mod run_route; // 流归位：每段流折成归一事件、定它归哪个运行（只用适配层给的那几格）
pub mod tap; // tee 的消费侧（后端这一半）：进程级 tap 口 → 活卡 → 每条流连接最低优先那一路
pub mod topic_body; // 帧里的小成品现算（照主题表的重问那条命令跑它自己的处理器；只由 main 装进 `wire::topic_hook`）
pub mod writer; // 一条流的写者：应答 · 出方向 · tap 三路排空写进那一端（两条载体共用）
