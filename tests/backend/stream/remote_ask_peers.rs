//! 判据用的「那几台」（替身）：跨台合并那几条（`remote_ask::ask_each`）照它问别的台的常驻。
//! 机器名 ⇒ 那台常驻答回来的那一行（缺 ⇒ 常驻不在）；记下每一问的命令行与 stdin 那一行。

use super::{Remote, Table};
use crate::common::said::Said;
use serde_json::Value;
use std::collections::BTreeMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Mutex;

impl Table {
    /// 一张固定的（机器表由调用方给）：各处判据经它造，生产只有 `REACH` 那一张。
    pub(crate) fn of(hosts: Vec<crate::dial::Host>) -> Self {
        Table {
            fixed: Mutex::new(Some(hosts)),
            peers: Mutex::new(BTreeMap::new()),
        }
    }
}

pub(crate) struct Peers {
    answers: Vec<(String, String)>,
    pub(crate) asked: Mutex<Vec<(String, Value)>>,
}

impl Peers {
    /// `answers`：那几台各自答回来的那一行（不在里头的那台 ⇒ 常驻不在）。
    pub(crate) fn new(answers: &[(&str, String)]) -> Peers {
        Peers {
            answers: answers
                .iter()
                .map(|(m, a)| (m.to_string(), a.clone()))
                .collect(),
            asked: Mutex::new(Vec::new()),
        }
    }

    /// 问过的每一问：(命令行, stdin 那一行)。
    pub(crate) fn asked(&self) -> Vec<(String, Value)> {
        self.asked.lock().unwrap().clone()
    }

    /// 这几台进机器表（组成的拨号请求里 `host` 就是机器名，替身按它认是哪台）。
    pub(crate) fn table(names: &[&str]) -> Table {
        named(names)
    }

    /// 命令行里交给那台的 `--within-ms` 那个数。
    pub(crate) fn within_ms(command: &str) -> u64 {
        command
            .split_whitespace()
            .skip_while(|w| !w.contains("--within-ms"))
            .nth(1)
            .and_then(|w| w.trim_matches('\'').parse().ok())
            .expect("命令行带期限")
    }
}

impl Remote for Peers {
    fn run<'a>(
        &'a self,
        _dial: &'a Value,
        _command: String,
        _stdin: Option<String>,
    ) -> Pin<Box<dyn Future<Output = Result<String, String>> + Send + 'a>> {
        unreachable!("问常驻走 run_coded")
    }

    fn run_coded<'a>(
        &'a self,
        dial: &'a Value,
        command: String,
        stdin: Option<String>,
    ) -> Pin<Box<dyn Future<Output = Result<String, Said>> + Send + 'a>> {
        Box::pin(async move {
            let line: Value = serde_json::from_str(stdin.as_deref().unwrap_or("")).unwrap();
            self.asked.lock().unwrap().push((command, line));
            let host = dial["host"].as_str().unwrap_or_default();
            self.answers
                .iter()
                .find(|(h, _)| h == host)
                .map(|(_, a)| a.clone())
                .ok_or_else(|| Said::new(crate::wire::listen::CALL_ABSENT, "absent".to_string()))
        })
    }
}

/// 发起方给的截止时刻：从现在起 `ms` 毫秒。
pub(crate) fn until(ms: u64) -> Option<crate::platform::child::Until> {
    crate::platform::child::Until::after(crate::platform::child::Deadline::millis(ms))
}

/// 判据用的一台：名字 ＝ 地址 ＝ `name`，用户 `u`（组成的请求里 `host` 就是它，替身按它认是哪台）。
pub(crate) fn host(name: &str) -> crate::dial::Host {
    crate::dial::machine::loose(&serde_json::json!({"label": name, "host": name, "user": "u"}))
        .expect("判据那一台")
}

/// 这几台的机器表（同 [`host`] 那一形，照给的先后）。
pub(crate) fn named(names: &[&str]) -> Table {
    Table::of(names.iter().map(|n| host(n)).collect())
}

/// 空的机器表。
pub(crate) fn empty() -> Table {
    Table::of(Vec::new())
}

/// 表尾再加一台。
pub(crate) fn add(t: &Table, h: crate::dial::Host) {
    t.fixed
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get_or_insert_with(Vec::new)
        .push(h);
}
