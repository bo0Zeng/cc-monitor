/**
 * 「待办」各类的数排成几枚小标签（「文件与数据」顶上 · 每台段头 · 别名页页首指路条同一套）：
 * 要做（含过期）· 待定 · 要装 · 可选（含要装 · 可选）· 已做；0 的不出。类与态都是那台后端给的，这里只数、只排。
 */
import { tag } from "../kit/badge";
import { copyText } from "../copy-table";
import type { Chore } from "./data-reads";

/** 各类的数：要做（含过期）· 待定 · 要装 · 可选（含要装 · 可选）· 已做；0 的不出。 */
export function countTags(list: readonly Chore[], only?: readonly ("must" | "decide" | "install" | "optional" | "done")[]): HTMLElement[] {
  const open = list.filter((c) => c.state !== "done" && c.state !== "declined");
  const n = {
    must: open.filter((c) => c.kind === "must" || c.state === "expired").length,
    decide: open.filter((c) => c.kind === "decide").length,
    install: open.filter((c) => c.kind === "install").length,
    optional: open.filter((c) => (c.kind === "optional" || c.kind === "installOptional") && c.state !== "expired").length,
    done: list.filter((c) => c.state === "done").length,
  };
  const said: Record<keyof typeof n, (n: number) => string> = {
    must: (x) => copyText("dataPage.chores.countMust", { n: x }),
    decide: (x) => copyText("dataPage.chores.countDecide", { n: x }),
    install: (x) => copyText("dataPage.chores.countInstall", { n: x }),
    optional: (x) => copyText("dataPage.chores.countOptional", { n: x }),
    done: (x) => copyText("dataPage.chores.countDone", { n: x }),
  };
  const out: HTMLElement[] = [];
  for (const k of only ?? (["must", "decide", "install", "optional", "done"] as const)) {
    if (n[k] === 0) continue;
    const t = tag(said[k](n[k]));
    t.dataset.kind = k;
    out.push(t);
  }
  return out;
}

