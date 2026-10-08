/**
 * **派活的那段话**（子运行记录里第一条：派出它的那一方交给它的活，记录成品里 `userText.speaker` 是 `agentTask`）。
 *
 * 不是用户说的 ⇒ 不画成右边的用户气泡：整宽、靠左、左边一条竖线、底色深一档；抬头写明是谁派的（主会话 / 某个 agent）＋ 派出的时刻；
 * 正文小一号、次一级颜色，超过四行收起、可展开。
 */
import { copyText } from "../copy-table";
import { button } from "../kit/button";
import s from "./brief.module.css";

/** 正文超过几行收起。 */
export const BRIEF_LINES = 4;

/**
 * @param text  那段话
 * @param time  派出的时刻（记录的钟面，后端写好的 `timeText`）
 * @param from  派出它的那个 agent 的标签；主会话派的 ⇒ `null`
 */
export function buildBriefCard(text: string, time: string, from: string | null): HTMLElement {
  const card = document.createElement("div");
  card.className = s.brief;
  card.dataset.role = "brief";
  const head = document.createElement("div");
  head.className = s.briefHead;
  const who = document.createElement("span");
  who.className = s.briefWho;
  who.dataset.role = "brief-from";
  who.textContent = from === null ? copyText("agentWindow.brief.fromMain") : copyText("agentWindow.brief.fromAgent", { parent: from });
  const when = document.createElement("span");
  when.className = s.briefWhen;
  when.textContent = time;
  head.append(who, when);
  const body = document.createElement("div");
  body.className = s.briefBody;
  body.textContent = text;
  card.append(head, body);
  if (text.split("\n").length > BRIEF_LINES) {
    body.dataset.clamped = "1";
    const more = button({
      label: copyText("agentWindow.brief.more"),
      kind: "ghost",
      size: "compact",
      onClick: () => {
        const open = body.dataset.clamped === "1";
        if (open) delete body.dataset.clamped;
        else body.dataset.clamped = "1";
        more.textContent = open ? copyText("agentWindow.brief.less") : copyText("agentWindow.brief.more");
      },
    });
    more.classList.add(s.briefMore);
    card.appendChild(more);
  }
  return card;
}
