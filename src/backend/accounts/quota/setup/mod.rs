//! ★ 轮换的配置编辑层：管界面与手写的配置 —— 策略文件（[`script`] 读头 · 只重写头那一块 · [`shelf`] 目录 · 新建 / 复制 / 改名 / 删 · 时区）·
//! 设置声明的校验（[`form`]）· 会话改过的设置与新会话默认用哪份（[`uses`]）· 旧版账本的升级（[`migrate`] · [`upgrade`]）。
//! 本层可以问基础功能层（`quota/rules/` 等：一判 · 推演 · 事实），基础层不认本层：它只认本层交的 [`Adopted`]（经 [`uses::Setup`] 实现的 `Adoptions`）。

pub(crate) mod form;
pub(crate) mod migrate;
pub(crate) mod script;
pub(crate) mod shelf;
pub(crate) mod upgrade;
pub(crate) mod uses;

use crate::accounts::quota::rules::Adopted;
use crate::accounts::quota::strategy::{Fail, FailKind, FailWhy};
use serde_json::Value;
use std::path::Path;

/// 策略头里的时区（缺 · 认不出 ⇒ UTC，界面另提示「时区不明」）。
pub(crate) fn tz_of(head: Option<&script::Head>) -> crate::Tz {
    head.and_then(|h| h.tz.as_deref())
        .and_then(crate::Tz::iana)
        .unwrap_or_default()
}

/// 一份读懂的策略文件 ＋ 改过的设置（没改 ⇒ 文件里的）⇒ 交给基础层的那一份。
pub(crate) fn adopted_of_file(
    id: &str,
    rev: &str,
    file: &script::File,
    draft: Option<&Value>,
) -> Adopted {
    Adopted {
        id: id.to_string(),
        rev: rev.to_string(),
        code: Ok(file.code.clone()),
        time_varying: file.head.time_varying,
        settings: draft.unwrap_or(&file.head.settings).clone(),
        tz: tz_of(Some(&file.head)),
    }
}

/// 目录里的那一份（没了 ⇒ `None`）＋ 改过的设置 ⇒ 交给基础层的那一份；没了 · 读不懂 ⇒ 判的时候走兜底（带原因）。
pub(crate) fn adopted_in(
    id: &str,
    shelved: Option<&shelf::Shelved>,
    draft: Option<&Value>,
) -> Adopted {
    let fail = match shelved.map(|s| (s, s.file.as_ref())) {
        Some((s, Ok(f))) => return adopted_of_file(id, &s.rev, f, draft),
        Some((_, Err(e))) => e.clone(),
        None => Fail {
            kind: FailKind::Gone,
            line: None,
            col: None,
            msg: format!("strategy file `{id}` is not there"),
            why: FailWhy::Gone,
        },
    };
    Adopted {
        id: id.to_string(),
        rev: shelved.map(|s| s.rev.clone()).unwrap_or_default(),
        code: Err(fail),
        settings: draft
            .cloned()
            .unwrap_or_else(|| Value::Object(Default::default())),
        ..Adopted::none()
    }
}

/// 策略目录 `dir` 里 id 那一份 ＋ 改过的设置（`id` 空 ⇒ 不轮换）。
pub(crate) fn adopted_at(dir: Option<&Path>, id: &str, draft: Option<&Value>) -> Adopted {
    if id.is_empty() {
        return Adopted::none();
    }
    adopted_in(id, shelf::find(&shelf::all(dir), id).as_deref(), draft)
}
