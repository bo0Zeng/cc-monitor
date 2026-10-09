/**
 * 「配置里有 app 不认识的键」的常驻提示：用户手写的键（打错字 / 抄了过期文档 / 已退役的旧名字）各模块认不出就一个字都不说，
 * 用户只看到「我明明写了，它没生效」⇒ 这里指名道姓说出来。
 * 常驻、没有「知道了」（它一直为真到用户去改那个文件；状态性警告不得只活在悬停 / 点击之后，同 `restart-notice.ts`）；
 * `config.ts` 那句 `console.warn` 用户看不见，不算出声。
 * 射程：只在设置窗；只认顶层键（子对象的 schema 归各自的主人）；不判值对不对。
 */
import { loadConfig, unknownConfigKeys, unknownKeysIn } from "../config";
import { copyText } from "../copy-table";

/**
 * 提示条上那句话（纯函数，判据直接打在这里）；空列表 ⇒ `null`。文案受 `ui-copy-discipline.vitest.ts` 管
 * （`config.json` 是用户自己机器上的文件，那把尺子不把 `.json` 当源码住址）。
 */
export function unknownKeysMessage(keys: readonly string[]): string | null {
  if (keys.length === 0) return null;
  return (
    copyText("unknownKeysNotice.unknownKeysMessage.message", { keysCount: keys.length, keys: keys.join(copyText("unknownKeysNotice.unknownKeysMessage.listSep")) })
  );
}

/** 这个窗口里的那几条（各自的重画）。 */
const renders = new Set<(keys: readonly string[]) => void>();

/** 拿一份刚读回的配置重算一遍：设置窗每次打开都读一次配置，顺手交给它（改对了就消、新出现的就出）。 */
export function rerenderUnknownKeys(cfg: unknown): void {
  const keys = unknownKeysIn(cfg);
  for (const r of renders) r(keys);
}

/**
 * 常驻条，没有关闭按钮。读两次：先用 `config.ts` 上次读盘留下的快照（立刻有话说），同时再 `loadConfig()` 刷新
 * （开设置正是在外面改完文件之后的标准动作）。两条路更新同一句话。
 */
export function createUnknownKeysBar(): HTMLElement {
  const bar = document.createElement("div");
  bar.className = "settings-unknown-keys-bar";
  bar.setAttribute("role", "status");
  const render = (keys: readonly string[]): void => {
    const msg = unknownKeysMessage(keys);
    if (msg === null) {
      bar.hidden = true;
      bar.textContent = "";
      return;
    }
    bar.hidden = false;
    bar.textContent = msg;
  };
  render(unknownConfigKeys());
  renders.add(render);
  // 刷新失败不清空已显示的那句：读不到盘不等于那些键消失了。
  void loadConfig().then(
    (cfg) => render(unknownKeysIn(cfg)),
    () => {},
  );
  return bar;
}
