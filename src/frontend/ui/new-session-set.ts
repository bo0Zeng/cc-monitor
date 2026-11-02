/**
 * 新建会话框的「一组」页：组名 ＋［换一组］· 几行（目录 · 别名 · 个数 · 组名）·［＋ 加一行］· 预览 ·［仅保存］，主按钮「启动 N」。
 *
 * - 判定与那几句全在那台后端：预览现问 `set-plan {rows}`（改了停手 300ms 再问，只读不写）；每行已有几个 · 再起几个 · 组名缺省是什么 ·
 *   起不了的原因 · 合计 · 主按钮的字都照成品摆。「＋」能不能再点（一行上限 · 合计上限）与重行也是那台在预览里当场判的（`canMore` · `moreWhy` · `blocked`），存的时候那台照旧再判（拒了那一句落在页底）。
 * - 打开的是存好的那一组：组名格旁［删除］直接删（`removeSet`）＋ toast［撤销］（照读到的那一份 `saveSet` 写回）。
 * - 组名一格留空 ＝ 缺省（占位字是那台算的缺省组名）；写了才进那一行的 `tab-group`。
 * - ［仅保存］只写配置文件那一段（`profiles-write` 的 `saveSet`）；主按钮 ＝ 存 ＋ 照存下的补足起（`set-start.ts`，同栏顶那一项）。
 */
import { copyText } from "./copy-table";
import { el } from "./kit/el";
import { button, setDisabled } from "./kit/button";
import { openMenu } from "./kit/menu";
import { select, type SelectHandle } from "./kit/select";
import { toast, undoToast } from "./kit/toast";
import { detailOf, saidOf } from "./kit/detail";
import { readProfiles, writeProfiles } from "./profiles-reads";
import { planSet, type RowEdit, type SetPlan } from "./set-reads";
import { startSet } from "./set-start";
import type { Machine } from "./ipc/origin";
import type { SetOut } from "./generated/SetOut";
import s from "./new-session.module.css";

/** 预览停手多久再问。 */
const PREVIEW_IDLE_MS = 300;

/** 框里那一页交给宿主（`new-session.ts`）的几只手。 */
export interface SetPage {
  el: HTMLElement;
  /** 换了机器（或第一次打开）：读那台存好的几组与别名，`name` 给了就打开那一组。 */
  load(origin: Machine, name: string | null): Promise<void>;
  /** 主按钮为什么点不了（`null` ＝ 能点）。 */
  blocked(): string | null;
  /** 主按钮的字：那台写好的「启动 N」/「已齐」（还没问到 ⇒ 正在读）。 */
  action(): string;
  /** 存 ＋ 起；成了 ⇒ `null`（框关），不成 ⇒ 那一句（落在按钮行上方）。 */
  submit(): Promise<{ said: string; detail: string } | null>;
}

/** 一行在框里的样子（`tabGroup` 空串 ＝ 缺省）。 */
interface Draft {
  cwd: string;
  alias: string;
  n: number;
  tabGroup: string;
}

/** 一格空着没有（只看框里自己的字）。 */
const blank = (v: string): boolean => v.trim().length === 0;

/** 框里那几行 ⇒ 交给那台的那一形（组名空着 ＝ 缺省）。 */
export function rowsOf(drafts: readonly Draft[]): RowEdit[] {
  return drafts.map((d) => ({ cwd: d.cwd.trim(), alias: d.alias, n: d.n, tabGroup: blank(d.tabGroup) ? null : d.tabGroup.trim() }));
}

/**
 * 建一组页。`refresh` ＝ 叫框重判主按钮（字 · 能不能点）。
 */
export function setPage(refresh: () => void): SetPage {
  let origin: Machine = "";
  let sets: SetOut[] = [];
  let aliases: string[] = [];
  let rev: string | null = null;
  /** 打开的是存好的哪一组（`null` ＝ 新建）：存的时候按它改名 / 新建。 */
  let was: string | null = null;
  let drafts: Draft[] = [];
  let plan: SetPlan | null = null;
  let planFailed: string | null = null;
  let readFailed: string | null = null;
  let seq = 0;
  let idle: ReturnType<typeof setTimeout> | null = null;

  const root = el("div", s.nsPage);
  root.dataset.role = "set-page";
  const nameInput = el("input", s.nsInput);
  nameInput.type = "text";
  nameInput.spellcheck = false;
  nameInput.dataset.role = "set-name";
  nameInput.setAttribute("aria-label", copyText("newSessionSet.label.name"));
  const switchBtn = button({ label: copyText("newSessionSet.set.switch"), kind: "ghost", size: "compact", icon: "caretDown", iconAfter: true });
  switchBtn.dataset.role = "set-switch";
  const nameBox = el("div", s.nsBox);
  nameBox.append(nameInput, switchBtn);
  // 打开的是存好的那一组才有：直接删 ＋ toast［撤销］（撤得回不确认）。
  const deleteBtn = button({ label: copyText("newSessionSet.set.delete"), kind: "icon", size: "compact", icon: "close" });
  deleteBtn.dataset.role = "set-delete";
  const nameCell = el("div", s.nsSetName);
  nameCell.append(nameBox, deleteBtn);
  const rowsBox = el("div", s.nsSetRows);
  const addBtn = button({ label: copyText("newSessionSet.row.add"), kind: "ghost", size: "compact", icon: "plus" });
  addBtn.dataset.role = "set-add";
  const preview = el("div", s.nsSetPreview);
  preview.dataset.role = "set-preview";
  const saveBtn = button({ label: copyText("newSessionSet.action.save"), size: "compact" });
  saveBtn.dataset.role = "set-save";
  const note = el("div", s.nsNote);
  note.hidden = true;
  const labeled = (label: string, control: HTMLElement): HTMLElement => {
    const r = el("div", s.nsRow);
    r.append(el("div", s.nsLabel, label), control);
    return r;
  };
  root.append(
    labeled(copyText("newSessionSet.label.name"), nameCell),
    labeled(copyText("newSessionSet.label.rows"), (() => {
      const c = el("div", s.nsCell);
      c.append(rowsBox, addBtn);
      return c;
    })()),
    labeled(copyText("newSessionSet.label.preview"), preview),
    note,
    saveBtn,
  );

  const say = (text: string): void => {
    note.textContent = text;
    note.hidden = text === "";
    note.dataset.intent = "error";
  };

  /** 预览：每行「目录 组名 已有 n」＋ 右侧再起几个 / 起不了的原因，末行合计（那几句都是那台写好的）。 */
  const paintPreview = (): void => {
    preview.replaceChildren();
    if (planFailed !== null) {
      preview.append(el("div", s.nsNote, planFailed));
      return;
    }
    if (plan === null) return;
    for (const r of plan.rows) {
      const line = el("div", s.nsSetLine);
      line.append(
        el("span", s.nsSetCwd, r.cwd),
        el("span", "", r.group),
        el("span", s.nsPlaceNote, r.haveSaid),
        el("span", s.nsSetMore, r.moreSaid),
      );
      if (r.blocked !== null) line.dataset.intent = "error";
      preview.append(line);
    }
    preview.append(el("div", s.nsPlaceNote, plan.totalSaid));
  };

  const ask = async (): Promise<void> => {
    const my = ++seq;
    const rows = rowsOf(drafts).filter((r) => r.cwd !== "" && r.alias !== "");
    if (rows.length === 0) {
      plan = null;
      planFailed = null;
      paintPreview();
      refresh();
      return;
    }
    try {
      const p = await planSet(origin, { rows });
      if (my !== seq) return;
      plan = p;
      planFailed = null;
    } catch (e) {
      if (my !== seq) return;
      plan = null;
      planFailed = saidOf(e);
    }
    paintPreview();
    paintRows();
    refresh();
  };

  /** 改了一格：停手 300ms 再问预览。 */
  const changed = (): void => {
    say("");
    if (idle !== null) clearTimeout(idle);
    // 调度：一次性 —— 停手 300ms 再问一次预览（每次改都重计）
    idle = setTimeout(() => {
      idle = null;
      void ask();
    }, PREVIEW_IDLE_MS);
    refresh();
  };

  const aliasSel = (d: Draft): SelectHandle =>
    select({
      label: copyText("newSession.label.alias"),
      options: aliases.map((a) => ({ value: a, label: a })),
      value: d.alias,
      onChange: (v) => {
        d.alias = v;
        changed();
      },
    });

  const paintRows = (): void => {
    rowsBox.replaceChildren();
    drafts.forEach((d, i) => {
      const line = el("div", s.nsSetRow);
      line.dataset.role = "set-row";
      const cwd = el("input", s.nsInput);
      cwd.type = "text";
      cwd.spellcheck = false;
      cwd.value = d.cwd;
      cwd.dataset.role = "set-cwd";
      cwd.setAttribute("aria-label", copyText("newSession.label.cwd"));
      cwd.addEventListener("input", () => {
        d.cwd = cwd.value;
        changed();
      });
      const cwdBox = el("div", s.nsBox);
      cwdBox.append(cwd);
      const minus = button({ label: copyText("newSessionSet.row.less"), kind: "icon", size: "compact", icon: "minimize" });
      minus.dataset.role = "set-less";
      minus.disabled = d.n <= 1;
      minus.addEventListener("click", () => {
        d.n = Math.max(1, d.n - 1);
        paintRows();
        changed();
      });
      const count = el("span", s.nsSetCount, String(d.n));
      count.dataset.role = "set-n";
      const plus = button({ label: copyText("newSessionSet.row.more"), kind: "icon", size: "compact", icon: "plus" });
      plus.dataset.role = "set-more";
      // 能不能再加一是那台当场判的（一行上限 · 一组合计上限）；还没问到 ⇒ 照常可点，问回来再灰。
      const fit = plan?.rows[i];
      setDisabled(plus, fit !== undefined && !fit.canMore ? (fit.moreWhy ?? "") : null);
      plus.addEventListener("click", () => {
        if (plus.getAttribute("aria-disabled") === "true") return;
        d.n += 1;
        paintRows();
        changed();
      });
      const drop = button({ label: copyText("newSessionSet.row.drop"), kind: "icon", size: "compact", icon: "close" });
      drop.dataset.role = "set-drop";
      drop.disabled = drafts.length <= 1;
      drop.addEventListener("click", () => {
        drafts.splice(i, 1);
        paintRows();
        changed();
      });
      const group = el("input", s.nsInput);
      group.type = "text";
      group.spellcheck = false;
      group.value = d.tabGroup;
      group.dataset.role = "set-group";
      group.setAttribute("aria-label", copyText("newSessionSet.row.group"));
      // 留空 ＝ 缺省：占位字是那台算的缺省组名（还没问到 ⇒ 「缺省」）。
      group.placeholder = plan?.rows[i]?.group ?? copyText("newSessionSet.row.groupDefault");
      group.addEventListener("input", () => {
        d.tabGroup = group.value;
        changed();
      });
      const groupBox = el("div", s.nsBox);
      groupBox.append(el("span", s.nsPlaceNote, copyText("newSessionSet.row.group")), group);
      line.append(cwdBox, aliasSel(d).el, minus, count, plus, drop, groupBox);
      rowsBox.append(line);
      // 那台说这一行起不了（重行 · 别名不在 · 目录不在）⇒ 淡掉，下面一行写那台那一句。
      const why = fit?.blocked ?? null;
      line.dataset.blocked = String(why !== null);
      if (why !== null) {
        const note = el("div", s.nsSetRowWhy, why);
        note.dataset.role = "set-row-why";
        rowsBox.append(note);
      }
    });
  };

  /** 打开一组：存好的那一组 ⇒ 照它的行；新建 ⇒ 一行空的（别名取这台第一条）。 */
  const open = (x: SetOut | null): void => {
    was = x?.name ?? null;
    nameInput.value = x?.name ?? "";
    // kit 按钮自带 display，`hidden` 切不动它 ⇒ 收起走 style。
    deleteBtn.style.display = was === null ? "none" : "";
    drafts = x ? x.rows.map((r) => ({ cwd: r.cwd, alias: r.alias, n: r.n, tabGroup: r.tabGroup ?? "" })) : [{ cwd: "", alias: aliases[0] ?? "", n: 1, tabGroup: "" }];
    plan = null;
    planFailed = null;
    say("");
    paintRows();
    paintPreview();
    void ask();
  };

  switchBtn.addEventListener("click", () => {
    openMenu(
      { el: switchBtn, align: "end" },
      [
        ...sets.map((x) => ({ label: x.name, checked: x.name === was, onClick: () => open(x) })),
        ...(sets.length === 0 ? [] : [{ divider: true, label: "" }]),
        { label: copyText("setMenu.list.create"), onClick: () => open(null) },
      ],
      { label: copyText("newSessionSet.set.switch") },
    );
  });
  nameInput.addEventListener("input", () => {
    say("");
    refresh();
  });
  addBtn.addEventListener("click", () => {
    // 新行带上一行的目录与别名。
    const last = drafts.at(-1);
    drafts.push({ cwd: last?.cwd ?? "", alias: last?.alias ?? aliases[0] ?? "", n: 1, tabGroup: "" });
    paintRows();
    changed();
  });

  /** 删打开的那一组（`removeSet`）：成了 ⇒ 页换新建、toast［撤销］（撤销 ＝ 照读到的那一份 `saveSet` 写回）；不成 ⇒ 页不换、页底那一句。 */
  deleteBtn.addEventListener("click", () => {
    const gone = sets.find((x) => x.name === was);
    if (!gone) return;
    void (async () => {
      try {
        rev = (await writeProfiles(origin, [{ op: "removeSet", name: gone.name }], rev)).rev;
      } catch (e) {
        say(saidOf(e));
        return;
      }
      sets = sets.filter((x) => x !== gone);
      open(null);
      const rows = rowsOf(gone.rows.map((r) => ({ cwd: r.cwd, alias: r.alias, n: r.n, tabGroup: r.tabGroup ?? "" })));
      const undo = async (): Promise<void> => {
        try {
          rev = (await writeProfiles(origin, [{ op: "saveSet", was: null, set: { name: gone.name, rows } }], rev)).rev;
          sets = [...sets, gone];
        } catch (e) {
          toast(copyText("newSessionSet.delete.undoFailed"), saidOf(e));
        }
      };
      undoToast(copyText("newSessionSet.delete.done"), () => void undo(), () => {});
    })();
  });

  /** 存这一组（改名照打开时的名字）；成了记下新版本与名字。不成 ⇒ 那一句。 */
  const save = async (): Promise<{ said: string; detail: string } | null> => {
    const name = nameInput.value.trim();
    try {
      const done = await writeProfiles(origin, [{ op: "saveSet", was, set: { name, rows: rowsOf(drafts) } }], rev);
      rev = done.rev;
      was = name;
      return null;
    } catch (e) {
      return { said: saidOf(e), detail: detailOf(e) };
    }
  };

  saveBtn.addEventListener("click", () => {
    if (page.blocked() !== null) return;
    void save().then((bad) => {
      if (bad === null) toast(copyText("newSessionSet.save.done"), nameInput.value.trim(), { level: "info" });
      else say(bad.said);
    });
  });

  const page: SetPage = {
    el: root,
    load: async (o, name) => {
      origin = o;
      readFailed = null;
      try {
        const book = await readProfiles(o);
        sets = book.sets;
        rev = book.rev;
        aliases = book.profiles.filter((p) => p.usable).map((p) => p.name);
      } catch (e) {
        sets = [];
        aliases = [];
        readFailed = saidOf(e);
      }
      open(sets.find((x) => x.name === name) ?? null);
      if (readFailed !== null) say(readFailed);
    },
    blocked: () => {
      if (readFailed !== null) return readFailed;
      if (blank(nameInput.value)) return copyText("newSessionSet.name.empty");
      if (drafts.some((d) => blank(d.cwd))) return copyText("newSessionSet.cwd.empty");
      if (aliases.length === 0) return copyText("newSessionSet.alias.none");
      if (plan === null) return copyText("kit.state.reading");
      // 都够了：主按钮灰着，为什么就是那台写好的「已齐」。
      if (plan.more === 0) return plan.startSaid;
      return null;
    },
    action: () => plan?.startSaid ?? copyText("kit.state.reading"),
    submit: async () => {
      const bad = await save();
      if (bad !== null) return bad;
      void startSet(origin, nameInput.value.trim());
      return null;
    },
  };
  return page;
}

/** 页签那一条：「单个 · 一组」，点了换页。 */
export function pageTabs(onPick: (onSet: boolean) => void): { el: HTMLElement; set(onSet: boolean): void } {
  const bar = el("div", s.nsTabs);
  bar.setAttribute("role", "tablist");
  const mk = (onSet: boolean, page: string, label: string): HTMLButtonElement => {
    const b = el("button", s.nsTab, label);
    b.type = "button";
    b.setAttribute("role", "tab");
    b.dataset.page = page;
    b.addEventListener("click", () => onPick(onSet));
    return b;
  };
  const single = mk(false, "single", copyText("newSession.page.single"));
  const group = mk(true, "set", copyText("newSession.page.set"));
  bar.append(single, group);
  return {
    el: bar,
    set: (onSet) => {
      single.setAttribute("aria-selected", String(!onSet));
      group.setAttribute("aria-selected", String(onSet));
    },
  };
}
