/**
 * 设置 → 机器 →「轮换」栏里规则列表下面那条时间轴：这台全部号、没有本会话轨、行头多「在用 N 会话」。
 * 顶行 `可用 personal, team · 最早恢复 work ↻22:40`（额度账的此刻可用 · 最早回来，后端判）；右侧 `6h | 24h | 7d`（设置这一份单记）。
 * 号多过 8 个时多一个「只看在用的」。数据：`rotation-plan {machine: true, view}`；读不到 ⇒ 整块不画（列表照旧）。
 * 判据：`tests/frontend/ui/settings/rules-timeline.vitest.ts`。
 */
import { readPlan, readQuota, type PlanRead } from "../quota-reads";
import { accountLabel, type QuotaRead } from "../quota-lines";
import { timelineAxis, viewSwitch, type TlView } from "../rot-timeline";
import { checkbox } from "../kit/switch";
import { copyText } from "../copy-table";
import type { Origin } from "../ipc/origin";
import s from "./rules-section.module.css";

/** 泳道多过这么多条才出「只看在用的」。 */
const MANY = 8;

export class MachineTimeline {
  readonly element: HTMLElement;
  private view: TlView = "24h";
  private onlyUsed = false;
  /** 此刻在看的那台（还没读过 ⇒ 空）。 */
  private at: { origin: Origin } | null = null;
  private plan: PlanRead | null = null;
  private quota: QuotaRead | null = null;
  private seq = 0;

  constructor() {
    this.element = document.createElement("section");
    this.element.className = s.rulesTimeline;
    this.element.dataset.rulesTimeline = "";
  }

  /** 读这台（换了机器 / 规则推来了 / 换了视窗都重读；只认最后一趟）。 */
  load(origin: Origin): void {
    if (origin !== this.at?.origin) {
      this.plan = null;
      this.quota = null;
    }
    this.at = { origin };
    const my = ++this.seq;
    void Promise.allSettled([
      readPlan(origin, { machine: true, view: this.view }),
      readQuota(origin),
    ]).then(([p, q]) => {
      if (my !== this.seq) return;
      this.plan = p.status === "fulfilled" ? p.value : null;
      this.quota = q.status === "fulfilled" ? q.value : null;
      this.paint();
    });
  }

  private head(): string {
    const q = this.quota;
    if (!q) return "";
    const parts: string[] = [];
    if (q.usableNow.length > 0)
      parts.push(
        copyText("rot.tl.usable", {
          list: q.usableNow.map(accountLabel).join(", "),
        }),
      );
    if (q.earliestReturn)
      parts.push(
        copyText("rot.tl.earliest", {
          acct: accountLabel(q.earliestReturn.account),
          at: q.earliestReturn.atText ?? "",
        }),
      );
    if (parts.length === 0)
      return q.accounts.length === 0
        ? copyText("rot.tl.unseen")
        : copyText("rot.tl.allOk");
    return parts.join(copyText("kit.text.sep"));
  }

  private paint(): void {
    const p = this.plan;
    if (!p) {
      this.element.replaceChildren();
      return;
    }
    const top = document.createElement("div");
    top.className = s.rulesTlTop;
    const title = document.createElement("span");
    title.className = s.rulesTlTitle;
    title.textContent = copyText("acct.tl.title");
    const line = document.createElement("span");
    line.className = s.rulesTlHead;
    line.dataset.tlHead = "machine";
    line.textContent = this.head();
    top.append(title, line);
    if (p.lanes.length > MANY) {
      const only = checkbox(copyText("rot.tl.onlyUsed"), this.onlyUsed, (v) => {
        this.onlyUsed = v;
        this.paint();
      });
      only.dataset.tlOnlyUsed = "";
      top.appendChild(only);
    }
    top.appendChild(
      viewSwitch(this.view, (v) => {
        this.view = v;
        if (this.at !== null) this.load(this.at.origin);
      }),
    );
    const shown: PlanRead = this.onlyUsed
      ? { ...p, lanes: p.lanes.filter((l) => (l.usedBy ?? 0) > 0) }
      : p;
    this.element.replaceChildren(
      top,
      timelineAxis(shown, { track: null, usedBy: true }),
    );
  }
}
