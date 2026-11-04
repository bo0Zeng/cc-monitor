// 任务面板每一行的状态字照抄后端写好的 `statusText`，界面不从 `status` 原值自己映。
// 夹具故意让状态字与原值对不上（completed 配一个不在表里的字）：界面若还在自己映，这条就红。夹具只造结构。
import { describe, it, expect } from "vitest";
import { TasksPanel, type TaskEntry } from "../../../src/frontend/ui/tasks-panel";

const task = (id: string, status: string, statusText: string): TaskEntry => ({ id, subject: `任务 ${id}`, status, statusText, blocks: [], blockedBy: [] });

describe("任务面板：状态字只摆后端那一句", () => {
  it("★ 行上的读屏字 ＝ statusText", () => {
    const p = new TasksPanel();
    document.body.append(p.pageElement);
    p.setSession("s", [task("1", "completed", "甲状态"), task("2", "weird", "乙状态")]);
    const labels = [...p.pageElement.querySelectorAll("li [role=img]")].map((e) => e.getAttribute("aria-label"));
    expect(labels).toEqual(["甲状态", "乙状态"]);
  });
});
