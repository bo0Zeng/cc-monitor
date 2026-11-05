//! **契约类型的写字一侧**：`cmd-wire` 只放两端对上的线上类型；照这些类型写成给人看的成品字（取文案表）、
//! 折成两端照画的整份的住这里。壳 · 后端 · 手机核心链同一份。
//!
//! - [`front`]：↗ 切到终端的结局与浮层那一张（壳 · 本机后端 `session-window` 两个写家都调它，句子只在这一处取）。
//! - [`history_asks`]：历史那几行上按哪台换说法的两格（「打开目录」出不出 · 删除确认的正文）。
//! - [`record`]：认不出那一条的句子与判法（后端适配层认不出的那一行 · 手机核心解不开的那一条同一份）。
//! - [`groups`]：几台的组合成一份，连组头那几句（本机后端问完各台 · 手机核心逐台问完调同一份）。
//! - [`search`]：几台的全文搜索结果合成一份，连结果头那一句与头底下那几句（本机后端扇出完 · 手机核心逐台问完调同一份）。
//! - [`book`]：一台的会话簿（会话帧折成整份 · 那台看不见时那一张脸；两端只调这一份折法）。
//! - [`live`]：一台的活卡板（`live_card` 帧折成某会话某运行整份的卡；两端只调这一份折法）。
//! - [`listing_copy`]：远端那几台的会话清单副本（副本表 · 照答换 · 落盘那一形；本机后端与手机核心同一份）。
//! - [`needs`]：几台的需手动合成一份 ＋ 总数（本机后端 `sessions-needs-merge` · 手机核心逐台问完调同一份）。
//! - [`column`]：最近优先那一个排法，几台各一列并成一列、分叉挂父、手机会话屏切段（本机后端历史页 · 全文搜索合并 · 手机会话屏同一份）。

pub mod book;
pub mod column;
pub mod front;
pub mod groups;
pub mod history_asks;
pub mod listing_copy;
pub mod live;
pub mod needs;
pub mod record;
pub mod search;

#[cfg(test)]
#[used]
static TEST_HOME: fn() = test_home::linked;
