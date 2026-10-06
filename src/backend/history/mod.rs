//! 历史清单与注解（模块地图那一行）。

pub mod history_annotations; // 历史注解（星标 / 改名 / 隐藏 / 上次账号）：帧面 `history-annotate` / `history-forget` / `history-last-accounts`（第四层；文件就是 monitor 从前那一份，路径由它交）
pub mod history_list; // 历史页的平铺会话清单：帧面 `history-list`（每行带状态与「能做什么」· 按项目的分组 · 搜标题 / 第一句 / 项目名；远端问那台 `raw`）
