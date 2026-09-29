//! 历史跨机 join 与注解，以及本机后端问远端后端的那一跳（`00 §2.1` 模块地图那一行）。

pub mod history_annotations; // 〔C4d · 第四波 4B〕历史注解（星标 / 改名 / 隐藏 / 上次账号）：帧面 `history-annotate` / `history-forget` / `history-last-accounts`（第四层；文件就是 monitor 从前那一份，路径由它交）
pub mod history_join; // 〔C4d · 第四波 4B〕历史跨机 join 的唯一的家：帧面 `history-projects` / `history-sessions` 出成品（这台 ＋ 可达表里的远端，并注解 ＋ 判活）
pub mod remote_ask; // 〔C4d · 第四波 4B〕本机后端问远端后端的那一跳（池里那条 SSH 上 capture 一次性子命令）＋ 可达表 —— 全后端只此一处；帧面 `remote-reach`
