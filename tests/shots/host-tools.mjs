/**
 * 截图台架用到的、住在登录用户家目录里的两样：Rust 工具链（编真后端 · 无头壳 · 文件窗口）与浏览器（Playwright 装的 Chrome for Testing）。
 *
 * 台架常在换过 HOME 的环境里跑（经 `tests/scripts/session-scrub.sh` 指到沙箱）：那时 `$HOME/.cargo`、`$HOME/.cache/ms-playwright` 都是空的。
 * 外面给了位置就照用；没给 ⇒ 按登录用户的家目录（口令表里那一格，`os.userInfo().homedir`，不看 HOME）找默认位置。
 */
import { existsSync, readdirSync, statSync } from "node:fs";
import os from "node:os";
import path from "node:path";

/** 登录用户的家目录（不随 HOME 变）。 */
export function loginHome() {
  return os.userInfo().homedir;
}

/** 编译用的工具链位置：`CARGO_HOME` · `RUSTUP_HOME`，外面给了照用，没给落在 `home` 底下的默认位置。 */
export function toolchainEnv(env, home = loginHome()) {
  return {
    CARGO_HOME: env.CARGO_HOME || path.join(home, ".cargo"),
    RUSTUP_HOME: env.RUSTUP_HOME || path.join(home, ".rustup"),
  };
}

/**
 * 浏览器：`CCM_SHOTS_CHROME` 给了照用；否则在 Playwright 缓存里找 `chromium-<号>/chrome-linux64/chrome`，取最新的那一份。
 * 缓存找这几处：`PLAYWRIGHT_BROWSERS_PATH` · `$HOME/.cache/ms-playwright` · `<home>/.cache/ms-playwright`。
 */
export function findChrome(env, home = loginHome()) {
  if (env.CCM_SHOTS_CHROME) return env.CCM_SHOTS_CHROME;
  const caches = [env.PLAYWRIGHT_BROWSERS_PATH, env.HOME && path.join(env.HOME, ".cache/ms-playwright"), path.join(home, ".cache/ms-playwright")].filter(
    (d, i, all) => d && all.indexOf(d) === i,
  );
  const found = caches
    .filter((d) => existsSync(d))
    .flatMap((d) =>
      readdirSync(d)
        .filter((b) => /^chromium-\d+$/.test(b))
        .map((b) => path.join(d, b, "chrome-linux64/chrome")),
    )
    .filter((p) => existsSync(p))
    .sort((a, b) => statSync(b).mtimeMs - statSync(a).mtimeMs);
  if (found.length === 0) {
    throw new Error(`找不到 Chromium（找过 ${caches.join(" · ")}）：设 CCM_SHOTS_CHROME 指向一个 chrome 可执行文件，或 PLAYWRIGHT_BROWSERS_PATH 指向 Playwright 的浏览器目录`);
  }
  return found[0];
}
