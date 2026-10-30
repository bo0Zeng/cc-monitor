//! **契约类型的写字一侧**：`cmd-wire` 只放两端对上的线上类型；照这些类型写成给人看的成品字（取文案表）的住这里。
//! 壳 · 后端 · 手机核心链同一份。
//!
//! - [`front`]：↗ 切到终端的结局与浮层那一张（壳 · 本机后端 `session-window` 两个写家都调它，句子只在这一处取）。

pub mod front;

#[cfg(test)]
#[used]
static TEST_HOME: fn() = test_home::linked;
