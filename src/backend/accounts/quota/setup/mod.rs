//! ★ 轮换的配置编辑层：管界面与手写的配置 —— 策略文件（[`script`] 读头 · 只重写头那一块 · [`shelf`] 目录 · 新建 / 复制 / 改名 / 删 · 时区）·
//! 设置声明的校验（[`form`]）· 会话载入哪份（副本存在基础层，载入只在本层）与新会话默认用哪份（[`uses`]）· 旧版账本的升级（[`migrate`] · [`upgrade`]）。
//! 本层可以问基础功能层（`quota/rules/` 等：一判 · 推演 · 事实），基础层不认本层：它只认会话里存的副本（新会话那一份经 [`uses::Setup`] 实现的 `Adoptions` 给）。

pub(crate) mod form;
pub(crate) mod migrate;
pub(crate) mod script;
pub(crate) mod shelf;
pub(crate) mod upgrade;
pub(crate) mod uses;

use crate::accounts::quota::rules::book::Copy;
use crate::accounts::quota::rules::Adopted;
use serde_json::Value;
use std::path::Path;

/// 策略头里的时区（缺 · 认不出 ⇒ UTC，界面另提示「时区不明」）。
pub(crate) fn tz_of(head: Option<&script::Head>) -> crate::Tz {
    head.and_then(|h| h.tz.as_deref())
        .and_then(crate::Tz::iana)
        .unwrap_or_default()
}

/// 一份读懂的策略文件 ⇒ 交给基础层判的那一份（设置页 · 试跑；`settings` 给了 ⇒ 照它，否则文件里的）。
pub(crate) fn adopted_of_file(
    id: &str,
    rev: &str,
    file: &script::File,
    settings: Option<&Value>,
) -> Adopted {
    Adopted {
        id: id.to_string(),
        rev: rev.to_string(),
        code: Ok(file.code.clone()),
        time_varying: file.head.time_varying,
        settings: settings.unwrap_or(&file.head.settings).clone(),
        tz: tz_of(Some(&file.head)),
    }
}

/// 一份读懂的策略文件此刻的副本（会话载入的就是它）。
pub(crate) fn copy_of_file(id: &str, rev: &str, file: &script::File) -> Copy {
    Copy {
        id: id.to_string(),
        rev: rev.to_string(),
        code: file.code_text.clone(),
        settings: file.head.settings.clone(),
        tz: file.head.tz.clone(),
        time_varying: file.head.time_varying,
        form: if file.head.form.is_empty() {
            Value::Null
        } else {
            serde_json::to_value(&file.head.form).unwrap_or(Value::Null)
        },
    }
}

/// 目录里那一份此刻的副本：没了 · 读不懂 ⇒ 缺（载入不了）。
pub(crate) fn copy_of(s: &shelf::Shelved) -> Option<Copy> {
    s.file.as_ref().ok().map(|f| copy_of_file(&s.id, &s.rev, f))
}

/// 策略目录 `dir` 里 id 那一份此刻的副本（`id` 空 · 没了 · 读不懂 ⇒ 缺）。
pub(crate) fn copy_at(dir: Option<&Path>, id: &str) -> Option<Copy> {
    if id.is_empty() {
        return None;
    }
    shelf::find(&shelf::all(dir), id).and_then(|s| copy_of(&s))
}

/// 副本里存的声明（存的时候就是读懂的；读不回 ⇒ 空）。
pub(crate) fn form_of(c: &Copy) -> Vec<form::Cell> {
    serde_json::from_value(c.form.clone()).unwrap_or_default()
}
