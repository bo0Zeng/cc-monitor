//! 停线判定：节点（远端作业）与调度端本机那一支用这同一份。单位跟帧一样：内存 · 盘 MiB，压力百分比。

use crate::{Breach, Facts, StopLines};

/// 连着撞几次才停（看门狗 2 秒量一次）。
pub const STOP_AFTER_HITS: u32 = 2;

/// 判的次序：内存 · 压力 · 盘（`Breach::ALL`）。
const ORDER: &[Breach] = Breach::ALL;

impl StopLines {
    /// 此刻撞了这一条没有。没给这条停线、或量不到那一格，都不算撞（量不到由门槛 · 线程线兜底）。
    pub fn hit(&self, b: Breach, f: &Facts) -> bool {
        match b {
            Breach::Mem => matches!((self.mem, f.mem), (Some(s), Some(m)) if m < s),
            Breach::Psi => matches!((self.psi, f.psi), (Some(s), Some(p)) if p >= s),
            Breach::Disk => matches!((self.disk, f.disk), (Some(s), Some(d)) if d < s),
        }
    }

    /// 此刻撞的第一条（按 内存 · 压力 · 盘 的次序）。
    pub fn first_hit(&self, f: &Facts) -> Option<Breach> {
        ORDER.iter().copied().find(|b| self.hit(*b, f))
    }
}

/// 一趟的停线：同一条连着撞 [`STOP_AFTER_HITS`] 次才算撞停线，中间一次没撞就从头数。
#[derive(Debug, Clone, Default)]
pub struct StopWatch {
    lines: StopLines,
    hits: [u32; Breach::ALL.len()],
}

impl StopWatch {
    pub fn new(lines: StopLines) -> StopWatch {
        StopWatch {
            lines,
            hits: [0; Breach::ALL.len()],
        }
    }

    /// 量一次；撞够次数的那一条返回出来。
    pub fn observe(&mut self, f: &Facts) -> Option<Breach> {
        let mut out = None;
        for (i, b) in ORDER.iter().copied().enumerate() {
            if self.lines.hit(b, f) {
                self.hits[i] += 1;
                if self.hits[i] >= STOP_AFTER_HITS && out.is_none() {
                    out = Some(b);
                }
            } else {
                self.hits[i] = 0;
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn needs_two_consecutive_hits_of_the_same_line() {
        let mut w = StopWatch::new(StopLines {
            mem: Some(u64::MAX),
            psi: None,
            disk: Some(1),
        });
        let hit = Facts {
            mem: Some(1),
            disk: Some(10),
            ..Facts::default()
        };
        let calm = Facts {
            mem: Some(u64::MAX),
            disk: Some(10),
            ..Facts::default()
        };
        assert_eq!(w.observe(&hit), None, "第一次不报");
        assert_eq!(w.observe(&calm), None);
        assert_eq!(w.observe(&hit), None, "中间没撞就从头数");
        assert_eq!(w.observe(&hit), Some(Breach::Mem));
    }

    #[test]
    fn unmeasured_or_unset_does_not_hit() {
        let l = StopLines {
            mem: None,
            psi: Some(0.0),
            disk: Some(100),
        };
        assert_eq!(l.first_hit(&Facts::default()), None);
        assert_eq!(
            l.first_hit(&Facts {
                psi: Some(0.0),
                ..Facts::default()
            }),
            Some(Breach::Psi)
        );
        assert_eq!(
            l.first_hit(&Facts {
                disk: Some(99),
                ..Facts::default()
            }),
            Some(Breach::Disk)
        );
        assert_eq!(
            l.first_hit(&Facts {
                mem: Some(0),
                ..Facts::default()
            }),
            None,
            "没给内存停线"
        );
    }
}
