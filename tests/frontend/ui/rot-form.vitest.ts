/**
 * 照策略声明生成的表单：一种类型一种格，字全取声明里后端写好的那几样；
 * 改一格只回 `{k, v}`（值原样交后端校验，界面不判范围）；改过的格一道琥珀线、`line` 旁写原值；后端回的错落在那一格。
 */
import { describe, it, expect } from "vitest";
import { cellErrorsFor, paramForm } from "../../../src/frontend/ui/rot-form";
import { copyText } from "../../../src/frontend/ui/copy-table";
import type { DeclShown } from "../../../src/frontend/ui/generated/DeclShown";

const D: Record<string, DeclShown> = {
  back: { k: "back", type: "bool", init: false, label: "L-back" },
  delay: { k: "delay", type: "minutes", init: 0, min: 0, max: 120, label: "L-delay", hint: "H-delay" },
  stint: { k: "stint", type: "points", init: null, min: 1, max: 100, label: "L-stint", hint: "H-stint" },
  mode: {
    k: "mode",
    type: "choice",
    init: "normal",
    of: ["normal", "first", "standby"],
    label: "L-mode",
    options: [
      { v: "normal", label: "O-normal" },
      { v: "first", label: "O-first", hint: "OH-first" },
      { v: "standby", label: "O-standby", hint: "OH-standby" },
    ],
  },
  allOut: { k: "all_out", type: "choice", init: "stop", of: ["stop", "go"], label: "L-allout", options: [{ v: "stop", label: "O-stop" }, { v: "go", label: "O-go" }] },
  line: { k: "line", type: "line", init: 90, label: "L-line" },
  slots: { k: "slots", type: "slots", init: [], label: "L-slots" },
};

function form(decls: DeclShown[], values: Record<string, unknown>, extra: Partial<Parameters<typeof paramForm>[0]> = {}) {
  const sets: [string, unknown][] = [];
  const root = paramForm({ decls, values, name: "t", onSet: (k, v) => sets.push([k, v]), ...extra });
  document.body.replaceChildren(root);
  return { root, sets };
}

const field = (root: HTMLElement, k: string): HTMLElement => root.querySelector<HTMLElement>(`[data-rot-field="${k}"]`)!;

function type(input: HTMLInputElement, text: string): void {
  input.value = text;
  input.dispatchEvent(new Event("change", { bubbles: true }));
}

describe("rot-form · 照声明一项一格", () => {
  it("格的先后 == 声明的先后；字取声明里的 label / hint", () => {
    const { root } = form([D.delay, D.back, D.mode], { delay: 0, back: false, mode: "normal" });
    expect([...root.querySelectorAll<HTMLElement>("[data-rot-field]")].map((e) => e.dataset.rotField)).toEqual(["delay", "back", "mode"]);
    expect(field(root, "delay").textContent).toContain("L-delay");
    expect(field(root, "delay").textContent).toContain("H-delay");
    expect(field(root, "mode").textContent).toContain("OH-first");
  });

  it("bool ⇒ 勾选框；勾上回 true", () => {
    const { root, sets } = form([D.back], { back: false });
    const box = field(root, "back").querySelector<HTMLInputElement>("input[type=checkbox]")!;
    expect(box.checked).toBe(false);
    box.click();
    expect(sets).toEqual([["back", true]]);
  });

  it("数 ⇒ 数字格：填数回数；可空的清空回 null；填不成数的原样交后端（界面不判）", () => {
    const { root, sets } = form([D.delay, D.stint], { delay: 10, stint: null });
    const delay = field(root, "delay").querySelector<HTMLInputElement>("input")!;
    expect(delay.value).toBe("10");
    type(delay, "15");
    type(delay, "999");
    type(delay, "abc");
    const stint = field(root, "stint").querySelector<HTMLInputElement>("input")!;
    expect(stint.value).toBe("");
    expect(stint.placeholder).toBe(copyText("rot.form.empty"));
    type(stint, "12");
    type(stint, " ");
    expect(sets).toEqual([
      ["delay", 15],
      ["delay", 999],
      ["delay", "abc"],
      ["stint", 12],
      ["stint", null],
    ]);
  });

  it("choice ⇒ 单选（两个以内横排）；点一项回它的值", () => {
    const { root, sets } = form([D.mode, D.allOut], { mode: "standby", all_out: "stop" });
    const radios = [...field(root, "mode").querySelectorAll<HTMLInputElement>("input[type=radio]")];
    expect(radios.length).toBe(3);
    expect(radios[2].checked).toBe(true);
    radios[1].click();
    expect(sets).toEqual([["mode", "first"]]);
    expect(field(root, "all_out").dataset.inline).toBe("true");
    expect(field(root, "mode").dataset.inline).toBeUndefined();
  });

  it("choice 选项多过 4 个 ⇒ 下拉", () => {
    const many: DeclShown = { ...D.mode, of: ["a", "b", "c", "d", "e"], options: ["a", "b", "c", "d", "e"].map((v) => ({ v, label: `O-${v}` })) };
    const { root } = form([many], { mode: "c" });
    expect(field(root, "mode").querySelector("input[type=radio]")).toBeNull();
    expect(field(root, "mode").querySelector("button")?.textContent).toContain("O-c");
  });

  it("line ⇒ 用到 [n] % · 用满才换 · 5h、7d 分开…", () => {
    const { root, sets } = form([D.line], { line: 90 });
    const f = field(root, "line");
    const [at, full] = [...f.querySelectorAll<HTMLInputElement>("input[type=radio]")];
    expect(at.checked).toBe(true);
    const n = f.querySelector<HTMLInputElement>("input[data-line='all']")!;
    expect(n.value).toBe("90");
    type(n, "95");
    full.click();
    expect(sets).toEqual([
      ["line", 95],
      ["line", null],
    ]);
    expect(f.textContent).toContain(copyText("rot.form.full"));
    expect(f.textContent).toContain(copyText("rot.form.split"));
  });

  it("line 分开写的值 ⇒ 5h / 7d 两格；改一格回整个表", () => {
    const { root, sets } = form([D.line], { line: { "5h": 80, "7d": 95 } });
    const f = field(root, "line");
    const w5 = f.querySelector<HTMLInputElement>("input[data-line='5h']")!;
    const w7 = f.querySelector<HTMLInputElement>("input[data-line='7d']")!;
    expect([w5.value, w7.value]).toEqual(["80", "95"]);
    type(w7, "90");
    expect(sets).toEqual([["line", { "5h": 80, "7d": 90 }]]);
  });

  it("点「5h、7d 分开…」⇒ 两格都照此刻的数", () => {
    const { root, sets } = form([D.line], { line: 90 });
    field(root, "line").querySelector<HTMLButtonElement>("[data-line-split]")!.click();
    expect(sets).toEqual([["line", { "5h": 90, "7d": 90 }]]);
  });

  it("slots ⇒ 每段一行（起 · 止 · 线）＋ ＋ 时段；删一段 · 改一格回整张表；空 ⇒ 无", () => {
    const { root, sets } = form([D.slots], { slots: [{ at: "17:00-02:00", line: 80 }] });
    const f = field(root, "slots");
    const row = f.querySelector<HTMLElement>("[data-slot='0']")!;
    const [from, to, line] = [...row.querySelectorAll<HTMLInputElement>("input")];
    expect([from.value, to.value, line.value]).toEqual(["17:00", "02:00", "80"]);
    type(to, "03:00");
    type(line, "0");
    row.querySelector<HTMLButtonElement>("[data-slot-drop]")!.click();
    f.querySelector<HTMLButtonElement>("[data-slot-add]")!.click();
    expect(sets).toEqual([
      ["slots", [{ at: "17:00-03:00", line: 80 }]],
      ["slots", [{ at: "17:00-02:00", line: 0 }]],
      ["slots", []],
      ["slots", [{ at: "17:00-02:00", line: 80 }, { at: "00:00-06:00", line: 80 }]],
    ]);
    expect(f.textContent).toContain(copyText("rot.form.slotRest"));
    const empty = form([D.slots], { slots: [] });
    expect(field(empty.root, "slots").textContent).toContain(copyText("rot.form.slotNone"));
  });

  it("按量号 ⇒ line 那格只写「按量号没有窗口」、slots 不出", () => {
    const { root } = form([D.line, D.slots, D.mode], { line: 90, slots: [], mode: "standby" }, { metered: true });
    expect(field(root, "line").textContent).toContain(copyText("rot.form.metered"));
    expect(field(root, "line").querySelector("input")).toBeNull();
    expect(field(root, "slots")).toBeNull();
  });

  it("改过的格一道琥珀线；line 旁写原值；skip 掉的不画", () => {
    const { root } = form([D.line, D.mode, D.slots], { line: 95, mode: "normal", slots: [] }, { changed: new Set(["line"]), was: { line: 99 }, skip: ["slots"] });
    expect(field(root, "line").dataset.changed).toBe("true");
    expect(field(root, "mode").dataset.changed).toBeUndefined();
    expect(field(root, "line").textContent).toContain(copyText("rot.form.was", { n: 99 }));
    expect(field(root, "slots")).toBeNull();
  });

  it("后端回的错落在那一格（cell 前缀对上的才算）", () => {
    const errs = cellErrorsFor(
      [
        { cell: "params.delay", code: "range", said: "S-delay" },
        { cell: "accounts[2].line.5h", code: "range", said: "S-line" },
        { cell: "accounts[1].line", code: "range", said: "S-other" },
      ],
      "accounts[2]",
    );
    expect(errs).toEqual([{ k: "line", said: "S-line" }]);
    const { root } = form([D.line, D.delay], { line: 120, delay: 0 }, { errors: errs });
    expect(field(root, "line").dataset.error).toBe("true");
    expect(field(root, "line").textContent).toContain("S-line");
    expect(field(root, "delay").dataset.error).toBeUndefined();
  });
});
