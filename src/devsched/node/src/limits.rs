//! 线程上限：账号的 `RLIMIT_NPROC` 管的是这个账号的全部线程（ssh 登进来的 shell 也算）。
//! 我们（连节点自己）压在「上限 − 余量」以下，留余量给用户再开 shell；到线先拒新作业、再停在跑的。量不到的当最紧。

use std::fs;

use crate::procs;

/// 余量默认：留给用户的线程数（`--user-margin`）。
pub const DEFAULT_USER_MARGIN: u64 = 80;
/// 节点名下进程数的硬上限默认（`--max-procs`）。
pub const DEFAULT_MAX_PROCS: u64 = 256;

/// 此刻的 RLIMIT_NPROC 软限；无限 ⇒ u64::MAX；读不到 ⇒ None。
pub fn rlimit_nproc() -> Option<u64> {
    let mut rl = libc::rlimit {
        rlim_cur: 0,
        rlim_max: 0,
    };
    if unsafe { libc::getrlimit(libc::RLIMIT_NPROC, &mut rl) } != 0 {
        return None;
    }
    Some(if rl.rlim_cur == libc::RLIM_INFINITY {
        u64::MAX
    } else {
        rl.rlim_cur
    })
}

/// 账号此刻的线程数：`/proc` 下属本账号的各进程 `Threads:` 之和；一个都读不到 ⇒ None。
pub fn account_threads() -> Option<u64> {
    let mut total = 0u64;
    let mut seen = false;
    for p in procs::my_pids() {
        if let Some(st) = procs::stat(p) {
            seen = true;
            total += st.threads.max(1);
        } else if let Ok(s) = fs::read_to_string(format!("/proc/{p}/status")) {
            if let Some(n) = s
                .lines()
                .find_map(|l| l.strip_prefix("Threads:"))
                .and_then(|v| v.trim().parse::<u64>().ok())
            {
                seen = true;
                total += n;
            }
        }
    }
    seen.then_some(total)
}

/// 线程上限的设定：`cap` 是参数压低的上限（测试注入用），实际上限取它与 RLIMIT_NPROC 小的那个。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    pub cap: Option<u64>,
    pub margin: u64,
    pub max_procs: u64,
}

impl Default for Limits {
    fn default() -> Limits {
        Limits {
            cap: None,
            margin: DEFAULT_USER_MARGIN,
            max_procs: DEFAULT_MAX_PROCS,
        }
    }
}

impl Limits {
    /// 实际上限：量不到 ⇒ 0（最紧：不接活）。
    pub fn limit(&self, rlimit: Option<u64>) -> u64 {
        let r = rlimit.unwrap_or(0);
        self.cap.map_or(r, |c| c.min(r))
    }

    /// 线：上限 − 余量。
    pub fn line(&self, rlimit: Option<u64>) -> u64 {
        self.limit(rlimit).saturating_sub(self.margin)
    }

    /// 判一次。`account` 账号线程数（量不到 ⇒ None，当已越线）；`ours` 节点名下的进程数。
    pub fn judge(&self, rlimit: Option<u64>, account: Option<u64>, ours: u64) -> Verdict {
        let line = self.line(rlimit);
        let acc = account.unwrap_or(u64::MAX);
        if acc > line {
            return Verdict::Stop(Breach::Threads { account, line });
        }
        if ours > self.max_procs {
            return Verdict::Stop(Breach::Procs {
                ours,
                max: self.max_procs,
            });
        }
        if acc >= line {
            return Verdict::Refuse(Breach::Threads { account, line });
        }
        if ours >= self.max_procs {
            return Verdict::Refuse(Breach::Procs {
                ours,
                max: self.max_procs,
            });
        }
        Verdict::Ok
    }
}

/// 撞了哪一条。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Breach {
    Threads { account: Option<u64>, line: u64 },
    Procs { ours: u64, max: u64 },
}

impl std::fmt::Display for Breach {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Breach::Threads {
                account: Some(a),
                line,
            } => write!(f, "账号线程 {a} 到了线 {line}（上限 − 余量）"),
            Breach::Threads {
                account: None,
                line,
            } => write!(f, "量不到账号线程数，当已到线 {line}"),
            Breach::Procs { ours, max } => write!(f, "节点名下进程 {ours} 到了上限 {max}"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Ok,
    /// 不接新作业，在跑的照跑。
    Refuse(Breach),
    /// 停掉在跑的。
    Stop(Breach),
}

impl Verdict {
    pub fn accepts_new(&self) -> bool {
        matches!(self, Verdict::Ok)
    }
}

/// 跑着的时候一次次判：账号线程贴在线上（Refuse）连着这么多次 ⇒ 升成停在跑的（进程数那一条不升：到了上限只是不接新的）。
/// 作业的软限压在线上，内核把账号卡在线上、过不了线，所以「过了线才停」那一支轮不到我们自己的作业；贴线不走就要停。
pub const STOP_AFTER_AT_LINE: u32 = 2;

#[derive(Debug, Default)]
pub struct LineWatch {
    at_line: u32,
}

impl LineWatch {
    pub fn observe(&mut self, v: Verdict) -> Verdict {
        match v {
            Verdict::Refuse(b @ Breach::Threads { .. }) => {
                self.at_line += 1;
                if self.at_line >= STOP_AFTER_AT_LINE {
                    self.at_line = 0;
                    Verdict::Stop(b)
                } else {
                    v
                }
            }
            _ => {
                self.at_line = 0;
                v
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unmeasurable_is_tightest() {
        let l = Limits::default();
        assert_eq!(l.limit(None), 0);
        assert!(matches!(l.judge(None, Some(1), 0), Verdict::Stop(_)));
        assert!(matches!(l.judge(Some(1000), None, 0), Verdict::Stop(_)));
    }

    #[test]
    fn line_is_limit_minus_margin_and_cap_lowers() {
        let l = Limits {
            cap: Some(250),
            margin: 80,
            max_procs: 256,
        };
        assert_eq!(l.line(Some(u64::MAX)), 170);
        assert_eq!(l.line(Some(200)), 120);
        assert_eq!(l.judge(Some(u64::MAX), Some(100), 0), Verdict::Ok);
        assert!(matches!(
            l.judge(Some(u64::MAX), Some(170), 0),
            Verdict::Refuse(Breach::Threads { .. })
        ));
        assert!(matches!(
            l.judge(Some(u64::MAX), Some(171), 0),
            Verdict::Stop(Breach::Threads { .. })
        ));
    }

    #[test]
    fn staying_at_line_escalates_to_stop() {
        let b = Breach::Threads {
            account: Some(10),
            line: 10,
        };
        let mut w = LineWatch::default();
        assert_eq!(w.observe(Verdict::Refuse(b)), Verdict::Refuse(b));
        assert_eq!(w.observe(Verdict::Ok), Verdict::Ok);
        assert_eq!(
            w.observe(Verdict::Refuse(b)),
            Verdict::Refuse(b),
            "中间离了线就从头数"
        );
        assert_eq!(w.observe(Verdict::Refuse(b)), Verdict::Stop(b));
        assert_eq!(
            w.observe(Verdict::Refuse(b)),
            Verdict::Refuse(b),
            "停过一趟从头数"
        );
        let p = Breach::Procs { ours: 4, max: 4 };
        let mut w = LineWatch::default();
        assert_eq!(w.observe(Verdict::Refuse(p)), Verdict::Refuse(p));
        assert_eq!(
            w.observe(Verdict::Refuse(p)),
            Verdict::Refuse(p),
            "进程数到上限只是不接新的"
        );
    }

    #[test]
    fn our_proc_cap() {
        let l = Limits {
            cap: None,
            margin: 0,
            max_procs: 4,
        };
        assert_eq!(l.judge(Some(u64::MAX), Some(10), 3), Verdict::Ok);
        assert!(matches!(
            l.judge(Some(u64::MAX), Some(10), 4),
            Verdict::Refuse(Breach::Procs { .. })
        ));
        assert!(matches!(
            l.judge(Some(u64::MAX), Some(10), 5),
            Verdict::Stop(Breach::Procs { .. })
        ));
    }

    #[test]
    fn measures_this_account() {
        let n = account_threads().expect("读得到 /proc");
        // 测试进程自己至少两条线程（主线程 ＋ 跑这条测试的）
        assert!(n >= 2, "{n}");
        assert!(rlimit_nproc().is_some());
    }
}
