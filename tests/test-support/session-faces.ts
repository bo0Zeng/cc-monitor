/**
 * 后端那份整份脸的金样（`tests/__fixtures__/session-face.golden.json`，`SessionFace::of` 对每种 `Life` 写的）读成测试里的帧。
 * 界面测试的脸只从这里取，不手写一份（两端对同一份金样画点）。
 */
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import type { SessionFace } from "../../src/frontend/ui/generated/SessionFace";
import type { WireSessionActivity } from "../../src/frontend/ui/generated/WireSessionActivity";
import type { TabManager } from "../../src/frontend/ui/tabs";
import type { TabStore } from "../../src/frontend/ui/tab-store";
import { REPO_ROOT } from "./repo-root";

export type FaceKey =
  | "ended"
  | "gone"
  | "live-background"
  | "live-idle"
  | "live-needs-you-tmux"
  | "live-unclear"
  | "live-working-plain"
  | "reading"
  | "reconnectable"
  | "unclear";

const GOLDEN = JSON.parse(readFileSync(resolve(REPO_ROOT, "tests/__fixtures__/session-face.golden.json"), "utf8")) as Record<FaceKey, SessionFace>;

/** 金样里那一张（每次一份新的，改了不串）。 */
export function goldenFace(key: FaceKey): SessionFace {
  return structuredClone(GOLDEN[key]);
}

/** 金样里每一张。 */
export function allFaces(): Array<[FaceKey, SessionFace]> {
  return (Object.keys(GOLDEN) as FaceKey[]).map((k) => [k, goldenFace(k)]);
}

const LIVE_KEY: Record<WireSessionActivity, FaceKey> = {
  working: "live-working-plain",
  needs_you: "live-needs-you-tmux",
  idle: "live-idle",
  background_work: "live-background",
};

/** 活着、在干那一态的那一张（`null` ＝ 判不出在干什么）；`tmux` 给了 ⇒ 终端那一格照它。 */
export function liveFace(activity: WireSessionActivity | null, tmux?: boolean): SessionFace {
  const f = goldenFace(activity === null ? "live-unclear" : LIVE_KEY[activity]);
  if (tmux !== undefined) f.can.term = { can: "yes", tmux };
  return f;
}

/**
 * 判据里的 `TabManager`：建 tab 那一刻那台还没给过脸的，当它刚宣告过（活着、在 tmux 里干活那一张）——
 * 生产里宣告与行同一条流、宣告那一帧在先（脸当场暂存，建 tab 时落实）。固定条复活那一趟不经它（复活出来的本来就没有脸），
 * 测「还没收到脸」的那几条也不经它。
 */
export function announced<T extends TabManager>(tm: T): T {
  const { store, prefs } = tm as unknown as { store: TabStore; prefs: { loadPinned(): Promise<void> } };
  let restoring = false;
  const load = prefs.loadPinned.bind(prefs);
  prefs.loadPinned = async () => {
    restoring = true;
    try {
      await load();
    } finally {
      restoring = false;
    }
  };
  const build = tm.ensureTab.bind(tm);
  tm.ensureTab = (sid, ...rest) => {
    if (!restoring && !store.tabs.has(sid) && !store.pendingFace.has(sid)) store.pendingFace.set(sid, liveFace("working", true));
    return build(sid, ...rest);
  };
  return tm;
}
