/**
 * 「开始用」卡：一个会话都没有时主窗口空态底下那一块。一步一行（圈 · 名字 · 一句 · 按钮），
 * 每步的字、按钮上的字与按钮开哪一处（`go`）都是本机后端 `first-run` 写好的，这里不按步骤挑字。
 * 点过「跳过」（交后端记下，跨重启还在）或每步都打了勾 ⇒ 不出。
 */
import { copyText } from "./copy-table";
import { button } from "./kit/button";
import { icon } from "./kit/icon";
import type { FirstRunGo } from "./generated/FirstRunGo";
import type { Readiness } from "./settings/readiness-reads";
import s from "./start-card.module.css";

export interface StartCardHost {
  /** 按钮开哪一处（后端给的闭集）。 */
  open(go: FirstRunGo): void;
  /** 点了「跳过」。 */
  skip(): void;
}

/** 这一份要不要出卡：问不到 · 跳过过 · 都做完 ⇒ 不出。 */
export function wantsCard(r: Readiness | null): r is Readiness {
  return r !== null && !r.skipped && r.steps.some((x) => !x.done);
}

export function startCard(r: Readiness, host: StartCardHost): HTMLElement {
  const card = document.createElement("section");
  card.className = s.startCard;
  card.dataset.role = "start-card";
  const head = document.createElement("div");
  head.className = s.startHead;
  const title = document.createElement("span");
  title.className = s.startTitle;
  title.textContent = copyText("firstRun.head.title");
  const skip = button({ label: copyText("rsFilewinBatch.pick.skip"), kind: "ghost", size: "compact", onClick: () => host.skip() });
  skip.dataset.action = "skip";
  head.append(title, skip);
  card.appendChild(head);
  r.steps.forEach((st, i) => {
    const row = document.createElement("div");
    row.className = s.startRow;
    row.dataset.step = st.id;
    row.dataset.done = String(st.done);
    const mark = document.createElement("span");
    mark.className = s.startMark;
    if (st.done) mark.appendChild(icon("check", "compact"));
    const text = document.createElement("div");
    text.className = s.startText;
    const name = document.createElement("div");
    name.className = s.startName;
    name.textContent = st.name;
    const hint = document.createElement("div");
    hint.className = s.startHint;
    hint.textContent = st.hint;
    text.append(name, hint);
    row.append(mark, text);
    if (!st.done) {
      const b = button({ label: st.act.label, kind: i === 0 ? "primary" : "secondary", size: "compact", onClick: () => host.open(st.act.go) });
      b.dataset.go = st.act.go;
      row.appendChild(b);
    }
    card.appendChild(row);
  });
  return card;
}
