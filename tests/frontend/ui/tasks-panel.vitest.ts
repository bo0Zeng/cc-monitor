// 任务面板只摆后端写好的句子：每一行的状态字照抄 `statusText`、状态栏那一枚照抄整份的 `chip`，界面不从 `status` 原值自己映、不自己数。
// 夹具故意让句子与原值对不上（completed 配一个不在表里的字 · chip 写一句跟条数无关的）：界面若还在自己映 / 自己数，这几条就红。夹具只造结构。
import { describe, it, expect } from "vitest";
import { TasksPanel, type TaskEntry } from "../../../src/frontend/ui/tasks-panel";
import type { TasksList } from "../../../src/frontend/ui/generated/TasksList";

const task = (id: string, status: string, statusText: string): TaskEntry => ({ id, subject: `任务 ${id}`, status, statusText, blocks: [], blockedBy: [] });
const list = (tasks: TaskEntry[], chip = "", hint = ""): TasksList => ({ tasks, chip, hint });

describe("任务面板：只摆后端那几句", () => {
  it("★ 行上的读屏字 ＝ statusText", () => {
    const p = new TasksPanel();
    document.body.append(p.pageElement);
    p.setSession("s", list([task("1", "completed", "甲状态"), task("2", "weird", "乙状态")], "甲枚"));
    const labels = [...p.pageElement.querySelectorAll("li [role=img]")].map((e) => e.getAttribute("aria-label"));
    expect(labels).toEqual(["甲状态", "乙状态"]);
  });

  it("★ 状态栏那一枚 ＝ chip，页签跟它同字；没有任务 ⇒ 不渲染", () => {
    const p = new TasksPanel();
    p.setSession("s", list([task("1", "completed", "x")], "甲枚 9/9", "乙分项"));
    expect([p.summaryElement.textContent, p.label, p.summaryElement.style.display]).toEqual(["甲枚 9/9", "甲枚 9/9", ""]);
    p.setSession("s", list([]));
    expect(p.summaryElement.style.display).toBe("none");
  });
});
