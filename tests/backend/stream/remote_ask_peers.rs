//! 判据用的「那几台」（替身）：跨台合并那几条（`remote_ask::ask_each`）照它问别的台的常驻。
//! 机器名 ⇒ 那台常驻答回来的那一行（缺 ⇒ 常驻不在）；记下每一问的命令行与 stdin 那一行。

use super::{insert, Remote, Said, Table};
use serde_json::{json, Value};
use std::future::Future;
use std::pin::Pin;
use std::sync::Mutex;

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

    /// 这几台登进可达表（拨号里只记机器名，替身按它认是哪台）。
    pub(crate) fn table(names: &[&str]) -> Table {
        let t = Table::default();
        for n in names {
            insert(&t, n, json!({ "host": n })).unwrap();
        }
        t
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
                .ok_or(Said {
                    code: Some(crate::wire::listen::CALL_ABSENT.into()),
                    message: "absent".into(),
                })
        })
    }
}

/// 发起方给的截止时刻：从现在起 `ms` 毫秒。
pub(crate) fn until(ms: u64) -> Option<crate::platform::child::Until> {
    crate::platform::child::Until::after(crate::platform::child::Deadline::millis(ms))
}
