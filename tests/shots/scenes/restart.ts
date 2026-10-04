/**
 * 换号重启：确认框 · 进行中 · 三种结局（起成了 · 起了没见报出 · 旧的停了没起来）。
 * 那台的 `session-restart` 由假后端答；起成了之后接回终端那一跳也由假后端答（开窗不出声）。
 */
import type { Scene } from "./index";
import type { World } from "../fake/types";
import { Refuse } from "../fake/types";
import { defaultWorld } from "../fake/world";
import { byText, click, mainReady, rightClick, sleep, waitFor } from "./helpers";

const ALL_TABS = 7;
/** devbox 上在跑的那一个（tab 栏第 4 个）。 */
const DEVBOX_TAB = 3;

function shot(id: string, title: string, desc: string, act: Scene["act"], world: () => World): Scene {
  return { id, page: "index", dir: "主窗口-换号重启", title, desc, width: 1280, height: 800, world, act };
}

/** `answer` = 那台对 `session-restart` 怎么答（不给 ⇒ 一直不回：进行中那一张）。 */
function restartWorld(answer?: () => unknown): () => World {
  return () => {
    const w = defaultWorld();
    w.ops["session-restart"] = () => {
      if (!answer) return new Promise(() => {});
      return answer();
    };
    w.ops["launch-render-cli"] = () => ({ cmd: "ccm -- --tmux-attach", account: null });
    w.ops["terminal-ssh"] = () => ({ command: "ssh -t devbox -- ccm" });
    w.commands.terminal_dial = () => ({ machine: "devbox" });
    w.commands.open_terminal_window = () => null;
    return w;
  };
}

const reply = (over: Record<string, unknown>) => () => ({
  compact: "done",
  started: "arrived",
  terminal: "cc-billing",
  account: { name: "work", configDir: "/home/user/.cc-monitor/accounts/work", model: null },
  ...over,
});

/** 右键 devbox 那个 tab → 换号重启 → work → 先压缩上下文再重启 ⇒ 确认框。 */
async function openConfirm(): Promise<void> {
  await mainReady(ALL_TABS);
  await rightClick(document.querySelectorAll<HTMLElement>("#tab-bar .tab")[DEVBOX_TAB]);
  await click(await byText(".tab-context-menu-item", "换号重启"));
  await click(await byText(".tab-context-menu-item", "work"));
  await click(await byText(".tab-context-menu-item", "先压缩上下文再重启"));
  await waitFor("[role='dialog']");
  await sleep(400);
}

async function confirmAndWait(): Promise<void> {
  await openConfirm();
  const btns = document.querySelectorAll<HTMLButtonElement>("[role='dialog'] button");
  await click(btns[btns.length - 1]);
  await waitFor(".ccm-toast");
  await sleep(900);
}

export const RESTART_SCENES: Scene[] = [
  shot("restart-confirm", "换号重启 · 确认框", "devbox 上在跑的会话右键 → 换号重启 → work → 先压缩：确认框（危险样式，默认焦点在取消）", openConfirm, restartWorld()),
  shot("restart-running", "换号重启 · 进行中", "确认之后：那台在做（先压缩、等摘要、停旧、起新），界面只说一句「重启切换中」", confirmAndWait, restartWorld()),
  shot("restart-done", "换号重启 · 起成了", "那台答起成了、新进程报出：开终端接上（不另说话），最后说一次「重启切换 ✓」", confirmAndWait, restartWorld(reply({}))),
  shot("restart-missed", "换号重启 · 起了没见报出", "那台用新号起了，但期限内没见新进程报出：照样接上，说「未见会话报出」", confirmAndWait, restartWorld(reply({ compact: "timed_out", started: "missed" }))),
  shot(
    "restart-start-failed",
    "换号重启 · 旧的停了没起来",
    "那台停了旧会话、没能用新号起来：说清是哪个终端，点这句 ⇒ 用新号在 tmux 里再起一次",
    confirmAndWait,
    restartWorld(() => {
      throw new Refuse("start_failed", "ccm: launcher exited 1", { terminal: "cc-billing", why: "start_failed" });
    }),
  ),
];
