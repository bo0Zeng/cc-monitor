/**
 * **起一组**（栏顶组菜单那一项 · 新建框一组页的主按钮同一个起法）：`set-launch` 照存好的那一组补足、立刻起；
 * 起成的交占位标签页等报到；起完说那台写好的那一句（有没成的带逐条原因与［重试］：再点一次 ＝ 只起还缺的）。
 */
import { copyText } from "./copy-table";
import { toast } from "./kit/toast";
import { detailOf, saidOf } from "./kit/detail";
import { launchSet, type SetLaunched } from "./set-reads";
import { awaitArrival } from "./session-open";
import type { Machine } from "./ipc/origin";

/** 在飞的那几组：连点不重发（补足本来幂等，省一趟）。 */
const inFlight = new Set<string>();

/** 照存好的那一组补足、立刻起。 */
export async function startSet(origin: Machine, name: string): Promise<void> {
  const key = `${origin}\u0000${name}`;
  if (inFlight.has(key)) return;
  inFlight.add(key);
  let out: SetLaunched;
  try {
    out = await launchSet(origin, name);
  } catch (e) {
    toast(saidOf(e), "", { detail: detailOf(e) });
    return;
  } finally {
    inFlight.delete(key);
  }
  for (const r of out.rows) {
    for (const o of r.opened) {
      if (o.arrival !== null) awaitArrival(origin, o.arrival, o.cwd, copyText("sessionOpen.arrived.title"));
    }
  }
  // 标题是组名（数据原样），正文是那台写好的那一句；没成的逐条原因进明细（「消息」里展开看）。
  if (out.failed === 0) toast(name, out.said, { level: "info" });
  else toast(name, out.said, { more: [out.detail], action: { label: copyText("setMenu.launched.retry"), run: () => void startSet(origin, name) } });
}
