//! 信任级别：主机表那一行写了 `trust=own` 才算自家机器。

/// 一台机器的信任级别。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trust {
    /// 自家机器（可信）。
    Own,
    /// 别人有 root（不可信）：只回输出与退出码。
    Untrusted,
}

impl Trust {
    /// 照主机表那一行的选项串定级：按空白拆，有一项恰是 `trust=own` 才是 `Own`。
    pub fn from_host_opts(opts: &str) -> Trust {
        if opts.split_whitespace().any(|o| o == "trust=own") {
            Trust::Own
        } else {
            Trust::Untrusted
        }
    }
}

/// 这一趟派不派得到这台：要 own 的只派可信机器。
pub fn may_dispatch(trust: Trust, need_own: bool) -> bool {
    !need_own || trust == Trust::Own
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_exact_trust_own_item_is_own() {
        assert_eq!(Trust::from_host_opts("trust=own"), Trust::Own);
        assert_eq!(
            Trust::from_host_opts("mem=6/2 caps=gui,node trust=own locks=a,b"),
            Trust::Own
        );
        assert_eq!(Trust::from_host_opts("\ttrust=own\n"), Trust::Own);
        for opts in [
            "",
            "mem=24/2",
            "trust=ownx",
            "notrust=own",
            "trust=own,x",
            "caps=trust=own",
            "trust=Own",
            "trust=",
        ] {
            assert_eq!(Trust::from_host_opts(opts), Trust::Untrusted, "{opts:?}");
        }
    }

    #[test]
    fn need_own_only_goes_to_own() {
        assert!(may_dispatch(Trust::Own, true));
        assert!(may_dispatch(Trust::Own, false));
        assert!(may_dispatch(Trust::Untrusted, false));
        assert!(!may_dispatch(Trust::Untrusted, true));
    }
}
