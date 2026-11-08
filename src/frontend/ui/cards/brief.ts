/**
 * **派活的那段话**（子运行记录里第一条：派出它的那一方交给它的活，记录成品里 `userText.speaker` 是 `agentTask`）。
 *
 * 不是用户说的 ⇒ 不画成右边的用户气泡：整宽、靠左、左边一条竖线、底色深一档；抬头写明是谁派的（主会话 / 某个 agent）＋ 派出的时刻；
 * 正文小一号、次一级颜色；后端给了折法（`who.parts[0].fold`）⇒ 先露它说的那几行、可展开，展开那一颗照后端那一句。界面不数行、不留阈值。
 */
import { copyText } from "../copy-table";
import { button } from "../kit/button";
import type { TextFold } from "../generated/TextFold";
import s from "./brief.module.css";

/**
 * @param text  那段话
 * @param fold  后端写好的折法；不折 ⇒ 缺
 * @param time  派出的时刻（记录的钟面，后端写好的 `timeText`）
 * @param from  派出它的那个 agent 的标签；主会话派的 ⇒ `null`
 */
export function buildBriefCard(text: string, fold: TextFold | undefined, time: string, from: string | null): HTMLElement {
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
  if (fold) {
    body.dataset.clamped = "1";
    body.style.setProperty("-webkit-line-clamp", String(fold.shown));
    const more = button({
      label: fold.more,
      kind: "ghost",
      size: "compact",
      onClick: () => {
        const open = body.dataset.clamped === "1";
        if (open) delete body.dataset.clamped;
        else body.dataset.clamped = "1";
        more.textContent = open ? copyText("kit.dock.close") : fold.more;
      },
    });
    more.classList.add(s.briefMore);
    card.appendChild(more);
  }
  return card;
}
