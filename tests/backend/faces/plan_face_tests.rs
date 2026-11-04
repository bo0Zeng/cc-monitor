//! 计划读面的帧面宿主：本子只起一次 pb · 交出去的那几句（一格的 agent 视角点到才起 pb：`dump_tests` 里真起假 pb 判）。
//! 走登记表问的参数形状那几条住 `on_upper_tests`（读命令表 ⇒ 挂主 crate）。

use super::*;
use serde_json::json;

/// `plan-list` 那一条照 [`reads_in`] 读、再拼成清单：判据换成替身数起了几次 pb。
fn list_in(
    b: &book::Book,
    dirs: Vec<String>,
    fresh: bool,
    read: &dyn Fn(&Path) -> Result<wire::PlanRead, book::Miss>,
    tz: &crate::common::time::Tz,
) -> wire::PlanList {
    let (pb, docs) = reads_in(b, dirs, fresh, read, tz);
    list_of(pb, &docs)
}

/// ★ 不重问时（`plan-list {}` · `sessions-needs` 计划那一截）同一个目录只起一次 pb：认出是工作区 · 不是工作区 · 读不成（pb 失败 · 不认得）
/// 都记在本子里，连问两次第二次照记下的那个结局；读不成的那一种照旧说 pb 不认得。`fresh` ⇒ 重起。
#[test]
fn an_unfresh_list_runs_pb_once_per_dir_whatever_it_said() {
    use crate::plan::dump::Ran;
    let tz = crate::common::time::Tz::default();
    type Case = (&'static str, fn() -> Ran);
    let cases: [Case; 3] = [
        ("不是工作区", || Ran::NotWorkspace("x".into())),
        ("读不成", || Ran::Failed {
            said: "坏了".into(),
            raw: None,
        }),
        ("不认得", || Ran::Unsupported("老 pb".into())),
    ];
    for (name, ran) in cases {
        let b = crate::plan::book::Book::default();
        let runs = std::cell::Cell::new(0);
        let read = |dir: &Path| {
            runs.set(runs.get() + 1);
            b.ingest(ran(), dir, &crate::plan::fixture::who, 1)
        };
        let first = list_in(&b, vec!["/d".into()], false, &read, &tz);
        let again = list_in(&b, vec!["/d".into()], false, &read, &tz);
        assert_eq!(runs.get(), 1, "{name}：同一个目录连问两次只起一次 pb");
        assert_eq!(
            serde_json::to_value(&first).unwrap(),
            serde_json::to_value(&again).unwrap(),
            "{name}：第二次照记下的结局"
        );
        list_in(&b, vec!["/d".into()], true, &read, &tz);
        assert_eq!(runs.get(), 2, "{name}：fresh ⇒ 重起");
    }
}

/// 交出去的那一份：随数据变的那几句后端照文案表写好（`said`），两端不再拼。
mod said {
    use super::*;
    use crate::plan::fixture::{dump, dump_broken, who, MAIN, STRANGER};
    use crate::plan::product::make;

    fn out(d: &serde_json::Value, stale: bool) -> wire::PlanRead {
        let mut doc = make(d, &who, "r0".into(), 0);
        if stale {
            doc.stale = Some(wire::PlanStale {
                said: Some("pb 那一句".into()),
                raw: None,
                since: 0,
                since_text: None,
            });
        }
        annotated(doc, &Default::default())
    }

    fn t(key: &str, args: &[(&str, &str)]) -> String {
        copy_text(key, args)
    }

    fn sep() -> String {
        t("kit.text.sep", &[])
    }

    #[test]
    fn the_slice_lines_are_written_from_its_numbers_and_time() {
        let doc = out(&dump("/w"), false);
        let sl = &doc.slices[0];
        let p = sl.progress.as_ref().expect("有收的值");
        let (d, o) = (p.done.to_string(), p.of.to_string());
        assert_eq!(
            sl.said.top.as_deref(),
            Some(t("plan.overview.top", &[("done", &d), ("of", &o)]).as_str())
        );
        assert_eq!(
            sl.said.root.as_deref(),
            Some(t("plan.outline.rootProgress", &[("done", &d), ("of", &o)]).as_str())
        );
        let at = doc.read_at_text.clone().expect("时刻的字");
        assert_eq!(
            sl.said.read_at.as_deref(),
            Some(t("plan.overview.readAt", &[("time", &at)]).as_str())
        );
        assert_eq!(sl.said.strip, None);
        assert_eq!(sl.said.bare, None);
    }

    #[test]
    fn a_stale_read_says_unreadable_then_since_when_in_one_line() {
        let doc = out(&dump("/w"), true);
        let since = doc
            .stale
            .as_ref()
            .and_then(|s| s.since_text.clone())
            .expect("时刻的字");
        let want = [
            t("plan.strip.sliceUnreadable", &[("reason", "pb 那一句")]),
            t("plan.strip.stale", &[("time", &since)]),
        ]
        .join(&sep());
        assert_eq!(doc.slices[0].said.strip.as_deref(), Some(want.as_str()));
        let broken = out(&dump_broken("/w"), false);
        let sl = &broken.slices[0];
        let e = sl.error.clone().expect("读不成");
        assert_eq!(
            sl.said.bare.as_deref(),
            Some(t("plan.strip.sliceUnreadable", &[("reason", &e)]).as_str())
        );
    }

    #[test]
    fn blocks_cells_and_sessions_carry_their_written_lines() {
        let doc = out(&dump("/w"), false);
        let sl = &doc.slices[0];
        let a2 = sl.blocks.iter().find(|b| b.id == "A2").expect("A2 那一块");
        let k = a2.inside.as_ref().expect("底下有算活的");
        assert_eq!(
            a2.said.inside.as_deref(),
            Some(
                t(
                    "plan.whyShort.inside",
                    &[("done", &k.done.to_string()), ("of", &k.of.to_string())]
                )
                .as_str()
            )
        );
        let cell = |id: &str| sl.cells.iter().find(|c| c.id == id).expect("那一格");
        assert_eq!(
            Some(a2.said.name.as_str()),
            cell("A2").title.as_deref(),
            "块名是根节点的标题"
        );
        assert_eq!(
            sl.blocks[0].said.name,
            t("plan.head.top", &[]),
            "最上面那一块"
        );
        assert_eq!(
            cell("A2").said.block_root.as_deref(),
            Some(t("plan.cell.blockRoot", &[("block", &sl.labels["A2"])]).as_str()),
            "插的是「编号 标题」"
        );
        assert_eq!(cell("A2").said.trail, None, "顶层没有上面几层");
        let kid = sl
            .cells
            .iter()
            .find(|c| c.parent.as_deref() == Some("A2"))
            .expect("A2 底下一格");
        assert_eq!(kid.said.trail.as_deref(), Some(sl.labels["A2"].as_str()));
        assert_eq!(kid.said.block_root, None);
        let mine = &doc.by_session[MAIN].said;
        assert_eq!(mine.name, t("plan.head.top", &[]));
        assert_eq!(
            mine.mark,
            [
                "alpha".to_string(),
                t("plan.head.top", &[]),
                "开发".to_string()
            ]
            .join(&sep())
        );
        let mut d = dump("/w");
        d["repos"][0]["scopes"][0]["assignee"] = json!(STRANGER);
        let doc = out(&d, false);
        let owner = doc.slices[0].blocks[0].owner.as_ref().expect("负责人");
        assert_eq!(
            owner.label.as_deref(),
            Some(t("plan.who.unknown", &[("id", "999999…0000")]).as_str())
        );
    }

    /// 列表那一行的 `said` 照抄那一片成品上的那一份（不另写一遍）。
    #[test]
    fn a_list_row_copies_the_slices_said() {
        let doc = out(&dump("/w"), false);
        let row = crate::plan::product::list_row(&doc);
        assert_eq!(row.slices[0].said, doc.slices[0].said);
        assert_eq!(row.by_session[MAIN].said, doc.by_session[MAIN].said);
    }
}
