/**
 * 「会打断什么」一问：关窗 · 退出 · 重启切换 · 停某台的 cc-monitor 之前，先问后端这么做会打断什么。
 *
 * - 清单由后端出（按族：在跑的回合 · 在跑的 agent · 后台任务 · 在传的文件 · 没存的编辑，各带名字），界面只画。
 * - 什么都打断不了（也没有非说不可的提醒）⇒ 直接做，不问。有 ⇒ 一个框：标题是动作、正文 `中断` / `保留` 两段、按钮 取消（默认焦点）· 动作（红）。
 * - 后端 2 秒没答（或答不上来）⇒ 按「有东西在跑」处理，框里一行 `无应答 · 在跑的任务未确认`。
 * - 同一件事重复按：并进同一个框，不叠第二个。
 * 判据：`tests/frontend/ui/kit/interrupts.vitest.ts`。
 */
import { confirmDialog, type ConfirmFn } from "./dialog";
import { copyText } from "../copy-table";

export type InterruptFamily = "turn" | "agent" | "task" | "transfer" | "edit";

export interface Interrupts {
  /** 按族，每族带显示名（列出来的族就是会被打断的；`turn` 那一族没有名字）。 */
  families: { family: InterruptFamily; names: string[] }[];
}

/** 问后端的那一下（调用方给：哪台、哪件事）。 */
export type AskInterrupts = () => Promise<Interrupts>;

export interface InterruptSpec {
  /** 标题（动宾）：`重启切换 orders → work`。 */
  title: string;
  /** 动作键（红）：`仍然重启`。 */
  action: string;
  ask: AskInterrupts;
  /** 保留的（`会话记录 · 可恢复`）；不给就不列这一段。 */
  keep?: string[];
  /** 一句非说不可的提醒（例：那个号还没信任这个目录）：有它就照问，即使什么都打断不了。 */
  note?: string;
  /** 问话框（测试注入；缺省是全产品那个对话框）。 */
  confirm?: ConfirmFn;
}

export const INTERRUPTS_WITHIN_MS = 2000;
/** 一族最多列几个名字，再多写 `×n`。 */
const NAMES_MAX = 3;

function familyLabel(family: InterruptFamily): string {
  switch (family) {
    case "turn":
      return copyText("kit.interrupts.turn");
    case "agent":
      return copyText("kit.interrupts.agent");
    case "task":
      return copyText("kit.interrupts.task");
    case "transfer":
      return copyText("kit.interrupts.transfer");
    case "edit":
      return copyText("kit.interrupts.edit");
  }
}

/** 一族那一项的字：`当前轮次` / `后台任务 build · lint` / `在跑 agent ×5`。 */
export function familyLine(family: InterruptFamily, names: string[]): string {
  const label = familyLabel(family);
  if (names.length === 0) return label;
  return names.length > NAMES_MAX
    ? copyText("kit.interrupts.many", { label, n: names.length })
    : copyText("kit.interrupts.named", { label, names: names.join(copyText("kit.text.sep")) });
}

/** 后端那一问，限时：到点没答 ⇒ `null`（当有东西在跑）。 */
export async function askWithin(ask: AskInterrupts, ms = INTERRUPTS_WITHIN_MS): Promise<Interrupts | null> {
  let timer: ReturnType<typeof setTimeout> | null = null;
  const late = new Promise<null>((resolve) => {
    // 调度：一次性 —— 问「会打断什么」的 2s 上限，答到了就清
    timer = setTimeout(() => resolve(null), ms);
  });
  try {
    return await Promise.race([ask().catch(() => null), late]);
  } finally {
    if (timer !== null) clearTimeout(timer);
  }
}

const pending = new Map<string, Promise<boolean>>();

/** 问完、该问才问：`true` ＝ 去做（什么都打断不了，或用户点了动作）；`false` ＝ 取消。 */
export function confirmInterrupts(spec: InterruptSpec): Promise<boolean> {
  const same = pending.get(spec.title);
  if (same) return same;
  const p = (async (): Promise<boolean> => {
    const got = await askWithin(spec.ask);
    const items = got?.families.map((f) => familyLine(f.family, f.names)) ?? [];
    if (got !== null && items.length === 0 && !spec.note) return true;
    if (got === null) items.push(copyText("kit.interrupts.noAnswer"));
    return (spec.confirm ?? confirmDialog)({
      title: spec.title,
      action: spec.action,
      danger: true,
      body: spec.note,
      rows: [
        { label: copyText("kit.interrupts.cut"), items },
        { label: copyText("kit.interrupts.keep"), items: spec.keep ?? [] },
      ],
    });
  })().finally(() => pending.delete(spec.title));
  pending.set(spec.title, p);
  return p;
}
