/**
 * 「待办」各类的数排成几枚小标签（「文件与数据」顶上 · 每台段头 · 别名页页首指路条同一套）：
 * 要做 · 待定 · 要装 · 可选 · 已做；0 的不出。数是后端数好的（一台的 `counts`，跨台的经本机 `chores-merge`），这里只排。
 */
import { tag } from "../kit/badge";
import { copyText } from "../copy-table";
import type { ChoreCounts } from "../generated/ChoreCounts";

type Kind = "must" | "decide" | "install" | "optional" | "done";

const SAID: Record<Kind, (n: number) => string> = {
  must: (n) => copyText("dataPage.chores.countMust", { n }),
  decide: (n) => copyText("dataPage.chores.countDecide", { n }),
  install: (n) => copyText("dataPage.chores.countInstall", { n }),
  optional: (n) => copyText("dataPage.chores.countOptional", { n }),
  done: (n) => copyText("dataPage.chores.countDone", { n }),
};

export function countTags(n: ChoreCounts, only: readonly Kind[] = ["must", "decide", "install", "optional", "done"]): HTMLElement[] {
  const out: HTMLElement[] = [];
  for (const k of only) {
    if (n[k] === 0) continue;
    const t = tag(SAID[k](n[k]));
    t.dataset.kind = k;
    out.push(t);
  }
  return out;
}
