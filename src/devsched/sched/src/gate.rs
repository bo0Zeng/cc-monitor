//! 门槛：派之前照主机表的放行线拿资源事实判；判不了当不放行。停线照主机表带进作业帧。
//! 主机表的单位：mem · tmp · disk 是 G（GiB），psi 是百分数，threads 是个数；帧里 mem · tmp · disk 是 MiB。

use crate::config::Host;
use devsched_wire::{Facts, StopLines};

const MIB_PER_G: f64 = 1024.0;

/// 过线没过：`Err(为什么)`。`slot` 是位序号（从 1 起；mem 只管第 2 个位起，见 CONFIG.md 主机表）。
pub fn gate(host: &Host, slot: u32, f: &Facts) -> Result<(), String> {
    let unknown = |what: &str| Err(format!("量不到{what}，判不了"));
    if let Some((pass, _)) = host.pair("mem") {
        if slot > 1 {
            match f.mem {
                None => return unknown("可用内存"),
                Some(m) if (m as f64) < pass * MIB_PER_G => {
                    return Err(format!("可用内存 {:.1}G < {pass}G", m as f64 / MIB_PER_G))
                }
                _ => {}
            }
        }
    }
    if let Some((pass, _)) = host.pair("psi") {
        match f.psi {
            None => return unknown("内存压力"),
            Some(p) if p >= pass => return Err(format!("内存压力 {p}% ≥ {pass}%")),
            _ => {}
        }
    }
    for (k, val, what) in [("tmp", f.tmp, "/tmp 剩余"), ("disk", f.disk, "盘剩余")] {
        if let Some((pass, _)) = host.pair(k) {
            match val {
                None => return unknown(what),
                Some(m) if (m as f64) < pass * MIB_PER_G => {
                    return Err(format!("{what} {:.1}G < {pass}G", m as f64 / MIB_PER_G))
                }
                _ => {}
            }
        }
    }
    if let Some((pass, _)) = host.pair("threads") {
        let (Some(now), Some(ours)) = (f.threads, f.ours) else {
            return unknown("账号线程数");
        };
        let peak = host.num("peak").unwrap_or(60).max(0) as f64;
        let want = now.saturating_sub(ours) as f64 + (f.jobs as f64 + 1.0) * peak;
        if want > pass {
            return Err(format!(
                "整账号线程 现 {now}（我们 {ours} · {} 趟）＋这趟 {peak} ⇒ {want} > {pass}",
                f.jobs
            ));
        }
    }
    Ok(())
}

/// 作业帧带的停线（主机表 `放行/停` 的后一半）。
pub fn stop_lines(host: &Host) -> StopLines {
    StopLines {
        mem: host.pair("mem").map(|(_, s)| (s * MIB_PER_G) as u64),
        psi: host.pair("psi").map(|(_, s)| s),
        disk: host.pair("disk").map(|(_, s)| (s * MIB_PER_G) as u64),
    }
}

/// 本机此刻的资源事实（本机那一支自己量）。
pub fn local_facts(dir_for_disk: &std::path::Path) -> Facts {
    Facts {
        mem: read_mem_avail_mib(),
        psi: read_psi_avg10(),
        disk: free_mib(dir_for_disk),
        tmp: free_mib(std::path::Path::new("/tmp")),
        threads: None,
        nproc: None,
        ours: None,
        jobs: 0,
    }
}

fn read_mem_avail_mib() -> Option<u64> {
    let s = std::fs::read_to_string("/proc/meminfo").ok()?;
    let l = s.lines().find(|l| l.starts_with("MemAvailable:"))?;
    let kb: u64 = l.split_whitespace().nth(1)?.parse().ok()?;
    Some(kb / 1024)
}

fn read_psi_avg10() -> Option<f64> {
    let s = std::fs::read_to_string("/proc/pressure/memory").ok()?;
    let l = s.lines().find(|l| l.starts_with("some"))?;
    l.split_whitespace()
        .find_map(|w| w.strip_prefix("avg10="))
        .and_then(|v| v.parse().ok())
}

fn free_mib(p: &std::path::Path) -> Option<u64> {
    use std::os::unix::ffi::OsStrExt;
    let c = std::ffi::CString::new(p.as_os_str().as_bytes()).ok()?;
    // SAFETY: statvfs 写进我们给的结构
    let mut st: libc::statvfs = unsafe { std::mem::zeroed() };
    let r = unsafe { libc::statvfs(c.as_ptr(), &mut st) };
    (r == 0).then(|| (st.f_bavail as u64).saturating_mul(st.f_frsize as u64) / (1024 * 1024))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::HostTable;

    fn h(line: &str) -> Host {
        HostTable::parse(line).hosts.remove(0)
    }

    fn facts() -> Facts {
        Facts {
            mem: Some(8 * 1024),
            psi: Some(1.0),
            disk: Some(600 * 1024),
            tmp: Some(4 * 1024),
            threads: Some(50),
            nproc: Some(250),
            ours: Some(10),
            jobs: 1,
        }
    }

    #[test]
    fn pass_lines() {
        let x = h("x 3 ssh:x mem=6/2 psi=25/60 disk=500/200 tmp=2/1");
        assert_eq!(gate(&x, 2, &facts()), Ok(()));
        let low = Facts {
            mem: Some(5 * 1024),
            ..facts()
        };
        assert!(gate(&x, 2, &low).is_err());
        assert_eq!(gate(&x, 1, &low), Ok(()), "mem 只管第 2 个位起");
        assert!(gate(
            &x,
            1,
            &Facts {
                psi: Some(25.0),
                ..facts()
            }
        )
        .is_err());
        assert!(gate(
            &x,
            1,
            &Facts {
                disk: Some(100 * 1024),
                ..facts()
            }
        )
        .is_err());
        assert!(gate(
            &x,
            1,
            &Facts {
                tmp: Some(1024),
                ..facts()
            }
        )
        .is_err());
    }

    #[test]
    fn threads_gate_counts_ours_and_peak() {
        let x = h("x 3 ssh:x threads=170/210 peak=80");
        assert!(
            gate(&x, 1, &Facts { jobs: 0, ..facts() }).is_ok(),
            "50 − 10 ＋ 1×80 = 120"
        );
        assert!(
            gate(&x, 1, &Facts { jobs: 1, ..facts() }).is_err(),
            "50 − 10 ＋ 2×80 = 200"
        );
    }

    #[test]
    fn unknown_facts_do_not_pass() {
        let x = h("x 3 ssh:x psi=25/60");
        assert!(gate(
            &x,
            1,
            &Facts {
                psi: None,
                ..facts()
            }
        )
        .is_err());
        let y = h("y 3 ssh:y");
        assert_eq!(gate(&y, 3, &Facts::default()), Ok(()), "没写门槛的不看");
        let z = h("z 3 ssh:z threads=170/210");
        assert!(gate(
            &z,
            1,
            &Facts {
                ours: None,
                ..facts()
            }
        )
        .is_err());
    }

    #[test]
    fn stop_lines_from_second_half() {
        let x = h("x 3 ssh:x mem=6/2 psi=25/60 disk=500/200");
        assert_eq!(
            stop_lines(&x),
            StopLines {
                mem: Some(2048),
                psi: Some(60.0),
                disk: Some(200 * 1024)
            }
        );
        assert_eq!(stop_lines(&h("y 1 ssh:y")), StopLines::default());
    }
}
