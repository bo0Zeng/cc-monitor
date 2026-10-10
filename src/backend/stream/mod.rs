//! 进后端的口 ① 帧面：帧定义 · 入方向命令信封 · 常驻监听口 · 本机后端问远端后端的那一跳（跨机问答原语）· tee 的消费侧（`tap` 帧）。

pub(crate) mod detail; // 失败应答的「复制详情」那一格（时刻 · 机器 · 命令 · 码 · 原话）
pub mod inbound; // U6b-1：流连接上的入方向（信封 / 分派 / 取消）
pub mod listen; // K-P1：常驻监听口 —— 脱离宿主之后还能被找到 / 被问到 / 被接上（纯判定住这里，接受循环住 main.rs）
pub(crate) mod run_route; // 流归位：每段流折成归一事件、定它归哪个运行（只用适配层给的那几格）
pub(crate) mod said; // 那几条命令被拒时给人看的那一句（界面那五张「码 → 句」表搬来）
pub mod tap; // tee 的消费侧（后端这一半）：进程级 tap 口 → 当前那条流连接的 `tap` 帧
pub mod topic; // 「X 变了 ⇒ 重读」那一种推送（`changed`）的主题表（只此一处：名字 · 可丢性 · 小成品上限）
pub mod topic_body; // 帧里的小成品现算（照主题表的重问那条命令跑它自己的处理器；只由 main 装进 topic_hook）
pub mod topic_hook; // 帧里的小成品怎么现算：进程里装的那一个函数（发端经它调）
pub mod wire;
