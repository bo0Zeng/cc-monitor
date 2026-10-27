/**
 * 新计划框（页头［新计划…］· 空态里那一颗）：机器 · 工作区 · 路径 · 领域 · 只读「执行」那一行 ·［新建］/［切到这一片］。
 *
 * 判定全在后端：能选哪几档、仓库落在哪、撞没撞上已有的一片、那一行怎么写都是 `plan-new` 的成品，改一格就重问一次（只认最后一问的答）；
 * 名字合不合规矩不在这里判，提交即 `plan-command repo`，pb 拒 ⇒ 它那一句落在路径下、框不关。
 */
import { banner } from "../kit/banner";
import { formDialog } from "../kit/dialog";
import { field } from "../kit/field";
import { select, type SelectOption } from "../kit/select";
import { segmented } from "../kit/tabs";
import { saidOf } from "../kit/detail";
import { copyText } from "../copy-table";
import { machineName } from "../control-said";
import type { Origin } from "../ipc/origin";
import type { PlanCmdReply, PlanNewForm } from "../plan-reads";
import s from "./plan-new.module.css";

/** 框要的几样（计划页给）：有哪几台 · 一台的目录（已有的工作区 · 会话的工作目录）· 问 · 敲 · 成了之后。 */
export interface NewPlanHost {
  machines: () => readonly Origin[];
  dirs: (origin: Origin) => { known: readonly string[]; cwds: readonly string[] };
  ask: (origin: Origin, dir: string, path: string, domain: string | null) => Promise<PlanNewForm>;
  create: (origin: Origin, dir: string, path: string, domain: string | null) => Promise<PlanCmdReply>;
  /** 成了：哪台 · 那一片落在的工作区根 · 片名 · toast 那一句。 */
  done: (origin: Origin, root: string, slice: string, said: string) => void;
}

/** 一行：标签在上 · 那一格 · 底下一句（可缺）。 */
function row(label: string, control: HTMLElement, under?: HTMLElement): HTMLElement {
  const r = document.createElement("div");
  r.className = s.pnRow;
  const l = document.createElement("div");
  l.className = s.pnLabel;
  l.textContent = label;
  r.append(l, control);
  if (under) r.appendChild(under);
  return r;
}

/** 灰字一句（空 ⇒ 不占行）。 */
function note(text = ""): HTMLElement {
  const e = document.createElement("div");
  e.className = s.pnNote;
  e.textContent = text;
  return e;
}

/** 一台的目录下拉：已有的工作区在前，会话的工作目录在后（同一个只出一次）。 */
function dirOptions(d: { known: readonly string[]; cwds: readonly string[] }, extra: string): SelectOption[] {
  const out: SelectOption[] = [];
  const seen = new Set<string>();
  const add = (v: string, note: string): void => {
    if (v === "" || seen.has(v)) return;
    seen.add(v);
    out.push({ value: v, label: v, note });
  };
  for (const k of d.known) add(k, copyText("plan.new.known"));
  for (const c of d.cwds) add(c, copyText("plan.new.cwd"));
  add(extra, "");
  return out;
}

/** 开框（`at` ＝ 预填的那台与目录）。 */
export function openNewPlan(host: NewPlanHost, at: { origin: Origin; dir: string }): void {
  let origin = at.origin;
  let dir = at.dir;
  let domain: string | null = null;
  let f: PlanNewForm | null = null;
  let seq = 0;

  const body = document.createElement("div");
  body.className = s.pnForm;
  const machineSel = select({
    label: copyText("plan.new.machine"),
    options: host.machines().map((m) => ({ value: m, label: machineName(m) })),
    value: origin,
    onChange: (v) => {
      origin = v;
      const opts = dirOptions(host.dirs(origin), "");
      dir = opts[0]?.value ?? "";
      dirSel.setOptions(opts, dir);
      void ask();
    },
  });
  const dirSel = select({
    label: copyText("plan.new.workspace"),
    options: dirOptions(host.dirs(origin), dir),
    value: dir,
    onChange: (v) => {
      dir = v;
      void ask();
    },
  });
  const pair = document.createElement("div");
  pair.className = s.pnPair;
  pair.append(row(copyText("plan.new.machine"), machineSel.el), row(copyText("plan.new.workspace"), dirSel.el, note(copyText("plan.new.workspaceHint"))));

  const path = field({ label: copyText("plan.new.path"), help: copyText("plan.new.pathHint") });
  const where = note();
  path.root.appendChild(where);
  path.input.addEventListener("input", () => {
    path.setError(null);
    void ask();
  });

  const domainCell = document.createElement("div");
  domainCell.className = s.pnRow;
  const domainNote = note();
  const domainRow = row(copyText("plan.new.domain"), domainCell, domainNote);

  const warn = document.createElement("div");
  const line = document.createElement("code");
  line.className = s.pnLine;
  body.append(pair, path.root, domainRow, warn, row(copyText("plan.new.line"), line, note(copyText("plan.new.lineHint"))));

  const pathNow = (): string => path.input.value.trim();

  const paint = (): void => {
    const g = f;
    const names = (g?.domains ?? []).filter((d) => d.error === null && d.name !== null).map((d) => d.name as string);
    const ex = g?.existing ?? null;
    const cur = ex ? ex.domain : domain;
    const seg = segmented({
      label: copyText("plan.new.domain"),
      items: names.map((n) => ({ key: n, label: n })),
      current: cur !== null && names.includes(cur) ? cur : null,
      onChange: (k) => {
        domain = k;
        void ask();
      },
    });
    // 撞上已有的一片 ⇒ 领域建了不改：那一排锁住变淡。
    seg.inert = ex !== null;
    if (ex) domainCell.dataset.locked = "true";
    else delete domainCell.dataset.locked;
    domainCell.replaceChildren(seg);
    domainNote.textContent = ex ? "" : copyText("plan.new.domainHint");
    where.textContent = g?.note ?? "";
    // pb 不在 · 太旧 ⇒ 带着那一句；起 pb 没成 ⇒ `failed`。两样都是后端写好的那一句。
    const bad = g?.pb.said ?? g?.failed ?? null;
    warn.replaceChildren(...(bad ? [banner("error", bad)] : []));
    line.textContent = g?.line ?? "";
    handle.setAction(ex ? copyText("plan.new.switch") : copyText("plan.new.create"));
    handle.refresh();
  };

  const ask = async (): Promise<void> => {
    const mine = ++seq;
    const got = await host.ask(origin, dir, pathNow(), domain).catch((e: unknown) => ({ ...(f ?? emptyForm(dir)), failed: saidOf(e) }));
    if (mine !== seq) return;
    f = got;
    paint();
  };

  const handle = formDialog({
    title: copyText("plan.new.title"),
    action: copyText("plan.new.create"),
    body,
    first: () => path.input,
    blocked: () => {
      if (!f) return copyText("plan.new.needPath");
      const bad = f.pb.said ?? f.failed;
      if (bad !== null) return bad;
      if (!pathNow() || !f.line) return copyText("plan.new.needPath");
      if (!f.existing && domain === null) return copyText("plan.new.needDomain");
      return null;
    },
    dirty: () => pathNow().length > 0,
    submit: async () => {
      const g = f;
      if (!g) return false;
      const ex = g.existing;
      let said: string | null;
      try {
        said = (await host.create(origin, dir, pathNow(), ex ? null : domain)).said;
      } catch (e) {
        path.setError(saidOf(e));
        path.input.focus();
        return false;
      }
      host.done(origin, g.root, ex?.slice ?? g.slice ?? "", said ?? copyText("plan.new.created"));
      return null;
    },
  });
  paint();
  void ask();
}

/** 问的那一下抛了（那台不在 · 后端没答）：照空的那一份画，那一句进红线。 */
function emptyForm(dir: string): PlanNewForm {
  return { pb: { state: "ok", said: null }, workspace: null, root: dir, slice: null, domains: [], existing: null, repoDir: null, note: null, line: null, failed: null };
}
