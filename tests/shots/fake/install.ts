/**
 * 截图工具的假后端入口：在产品入口模块之前装好 `window.__TAURI_INTERNALS__`，
 * 产品代码照常 `invoke` / `listen` / 订通道，答它的是场景里的合成数据。
 *
 * 只由截图工具的 vite 插件插进页面（`tests/shots/serve.mjs`）；产品构建里没有它。
 */
import { mockIPC, mockWindows } from "@tauri-apps/api/mocks";
import { emit } from "@tauri-apps/api/event";
import { FakeBackend } from "./backend";
import { DEFAULT_STORAGE, sceneById } from "../scenes";
import type { ShotsHandle } from "./types";
import { __setHostOsForTests } from "../../../src/frontend/ui/settings/host-os";

const params = new URLSearchParams(location.search);
const scene = sceneById(params.get("scene") ?? "");
const label = location.pathname.startsWith("/settings")
  ? "settings"
  : location.pathname.startsWith("/viewer")
    ? `viewer-${params.get("viewer") ?? "x"}`
    : "main";

const backend = new FakeBackend(scene.world());
if (scene.hostOs) __setHostOsForTests(scene.hostOs);
mockWindows(label);
mockIPC((cmd, args) => backend.invoke(cmd, (args ?? {}) as Record<string, unknown>), { shouldMockEvents: true });
backend.attachEmitter((event, payload) => emit(event, payload));

// 产品代码会把「上次停在哪个 tab」之类的记忆写进 localStorage；每张图从同一个起点开始。
try {
  localStorage.clear();
  sessionStorage.clear();
  for (const [k, v] of Object.entries(scene.storage ?? DEFAULT_STORAGE)) localStorage.setItem(k, v);
} catch {
  // 读不了存储就当它是空的
}

const handle: ShotsHandle = {
  state: "booting",
  error: null,
  unhandled: backend.unhandled,
};
window.__shots = handle;

window.addEventListener("DOMContentLoaded", () => {
  void (async () => {
    try {
      await scene.act({ backend });
      handle.state = "done";
    } catch (e) {
      handle.error = e instanceof Error ? `${e.message}\n${e.stack ?? ""}` : String(e);
      handle.state = "failed";
    }
  })();
});
