//! **契约类型的写字一侧**：`cmd-wire` 只放两端对上的线上类型；照这些类型写成给人看的成品字（取文案表）的住这里。
//! 壳 · 后端 · 手机核心链同一份。
//!
//! - [`front`]：↗ 切到终端的结局与浮层那一张（壳 · 本机后端 `session-window` 两个写家都调它，句子只在这一处取）。
//! - [`history_asks`]：历史那几行上按哪台换说法的两格（「打开目录」出不出 · 删除确认的正文）。
//! - [`record`]：认不出那一条的句子与判法（后端适配层认不出的那一行 · 手机核心解不开的那一条同一份）。

pub mod front;
pub mod history_asks;
pub mod record;

#[cfg(test)]
#[used]
static TEST_HOME: fn() = test_home::linked;
