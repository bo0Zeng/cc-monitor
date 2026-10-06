/**
 * 场景清单：每张图一个场景（哪扇窗 · 多大 · 放进哪个目录 · 标题 · 一句「这是什么状态」· 世界 · 开页之后做什么）。
 */
import type { SceneCtx, World } from "../fake/types";
import { defaultWorld } from "../fake/world";
import { MAIN_SCENES } from "./main";
import { PANEL_SCENES } from "./panels";
import { SETTINGS_SCENES } from "./settings";
import { HISTORY_SCENES } from "./history";
import { RESTART_SCENES } from "./restart";
import { KIT_SCENES } from "./kit";
import { ACCT_SCENES } from "./acct";
import { DRAWER_SCENES } from "./drawer";

export interface Scene {
  id: string;
  /** 哪扇窗：主窗口 / 设置窗 / 查看窗。 */
  page: "index" | "settings" | "viewer" | "tests/shots/kit";
  /** 图集里的目录（按界面分）。 */
  dir: string;
  title: string;
  desc: string;
  width: number;
  height: number;
  world: () => World;
  /** 页地址上另加的参数（查看窗要 `viewer=<sid>`）。 */
  query?: string;
  /** 开页之前写进 localStorage 的（tab 栏宽、提示看过没有 …）；不给 ⇒ 用 [`DEFAULT_STORAGE`]。 */
  storage?: Record<string, string>;
  /** 开页之后：等界面画好、点开要截的那一块。返回即可截。 */
  act: (ctx: SceneCtx) => Promise<void>;
}

/** 默认：tab 栏放宽到能读出标题、命令面板的首次高亮已经看过。 */
export const DEFAULT_STORAGE: Record<string, string> = {
  "cc-monitor.tab-bar-w": "260",
  "cc-monitor.cmdk-hint.seen": "1",
};

export const SCENES: Scene[] = [...MAIN_SCENES, ...PANEL_SCENES, ...RESTART_SCENES, ...HISTORY_SCENES, ...SETTINGS_SCENES, ...KIT_SCENES, ...ACCT_SCENES, ...DRAWER_SCENES];

export function sceneById(id: string): Scene {
  return (
    SCENES.find((s) => s.id === id) ?? {
      id: "",
      page: "index",
      dir: "",
      title: "",
      desc: "",
      width: 1280,
      height: 800,
      world: defaultWorld,
      act: async () => {},
    }
  );
}
