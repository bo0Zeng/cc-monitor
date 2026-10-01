//! 进后端的口 ① 帧面（`00 §1.6.2`）：帧定义 · 入方向命令信封 · 常驻监听口 · 本机后端问远端后端的那一跳（跨机问答原语）· tee 的消费侧（`tap` 帧）。

pub mod inbound; // U6b-1：流连接上的入方向（信封 / 分派 / 取消）
pub mod listen; // K-P1：常驻监听口 —— 脱离宿主之后还能被找到 / 被问到 / 被接上（纯判定住这里，接受循环住 main.rs）
pub mod remote_ask; // 〔C4d · 第四波 4B〕本机后端问远端后端的那一跳（池里那条 SSH 上 capture 一次性子命令）＋ 可达表 —— 全后端只此一处；帧面 `remote-reach`
pub(crate) mod run_route; // 流归位：每段流折成归一事件、定它归哪个运行（只用适配层给的那几格）
pub mod tap; // 〔TAP · V124〕tee 的消费侧（后端这一半）：进程级 tap 口 → 当前那条流连接的 `tap` 帧（`设计/20 §8`）
pub mod wire;
