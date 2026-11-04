/**
 * 文件窗口（egui）那一半：私有 Xvfb ＋ `workspace_tests.rs` 末尾那一格截图测试，一张图一个进程
 * （winit 一个进程只许建一个事件循环）。测试二进制先 `cargo test --no-run` 编出来，再逐场景按名字拉起。
 */
import { spawn, spawnSync } from "node:child_process";
import { existsSync, mkdirSync, rmSync, utimesSync, writeFileSync } from "node:fs";
import path from "node:path";
import { WALL_CEILING_MS } from "./real/pool.mjs";

/** 起私有 Xvfb 的那一份起法（仓根相对；文件窗口台架同用它）。 */
const XVFB_LAUNCHER = "tests/scripts/xvfb-free.sh";

/** 截图测试的全名（`cargo test` 的过滤串；改名要同拍改这里）。 */
const TEST = "workspace::tests::screenshot_for_the_shots_tool";

export const FILEWIN_SCENES = [
  { id: "filewin-main", scene: "main", title: "文件窗口 · 单栏 ＋ 预览", desc: "远端 devbox 上的一个项目目录，选中 main.rs、右侧预览（代码高亮）；左栏书签。左栏「家目录」灰着、预览头上是沙箱路径：都是合成后端的痕迹" },
  { id: "filewin-split", scene: "split", title: "文件窗口 · 双栏", desc: "开了双栏：两栏同一个目录" },
  { id: "filewin-empty", scene: "empty", title: "文件窗口 · 空目录", desc: "进到一个空目录" },
  { id: "filewin-missing", scene: "missing", title: "文件窗口 · 目录不存在", desc: "进到一个不存在的目录：列表那里报错" },
  { id: "filewin-pull", scene: "pull", title: "文件窗口 · 下载在路上", desc: "一趟下载在路上：「进度」表收着，状态栏右端「进度 1」带一小条合计进度（点开是表里那一行）" },
  { id: "filewin-mkdir-error", scene: "mkdir-error", title: "文件窗口 · 新建文件夹名字填错", desc: "就地新建文件夹那一格把名字清空了就回车：红边 ＋ 格子下面挂一句「名称为空」，那一格留着" },
  { id: "filewin-search", scene: "search", title: "文件窗口 · 按名字搜", desc: "工具条搜索框里输「retry」：全家目录里名字带它的文件。索引那一行的「扫描间隔 4242 秒」是合成后端故意给的怪数、命中路径是沙箱里的长路径" },
  { id: "filewin-progress", scene: "progress", title: "文件窗口 · 进度", desc: "窗口底部「进度」表：上传一摞与下载在跑（可停）· 删除 2 项在跑（带票，「停」按得下）· 上传失败（［重试失败的］）· 复制 3 项到另一台收尾（后端那一行 ＋ 没成的那一项 ＋［重试失败的］）· 复制 5 项断开（其余待核）；状态栏「进度 3」带红点" },
  { id: "filewin-stale", scene: "stale", title: "文件窗口 · 断线过期", desc: "和 devbox 断了：工具条下一条「devbox 离线 · 采样 13:40」＋［重新连接］；列表照常摆着上次那一屏；写类按钮灰（悬停「离线 · 只读」）" },
  { id: "filewin-unreadable", scene: "unreadable", title: "文件窗口 · 无权限目录清单", desc: "按名字搜，状态行「3 个目录无权限［查看］」点开：列后端交来的那两个目录（相对搜索起点）＋「另外 1 个」；点一行复制那条绝对路径（待认）" },
  { id: "filewin-edit-page", scene: "edit-page", title: "文件窗口 · 编辑页", desc: "目录页上开 main.rs ⇒ 同一栏一个编辑页（标签「✎ main.rs」、未保存点）：头条面包屑 · 未保存 · 查找 · 保存 · ⋯；行号槽 ＋ 正文在面里滚；查找条浮在右上；状态栏是底条（行 · 列 · UTF-8 · LF · 大小 / 上限）" },
  { id: "filewin-edit-stale", scene: "edit-stale", title: "文件窗口 · 编辑页 · 盘上被改过", desc: "保存时盘上那份在打开之后被改过：编辑面顶一条「main.rs 已在盘上被修改」［覆盖］［放弃改动并重开］，字一个不动" },
  { id: "filewin-edit-close", scene: "edit-close", title: "文件窗口 · 关编辑页 · 没保存", desc: "改了没存点 ×：「关闭 main.rs · 未保存」「不保存 = 丢弃改动」［取消］［不保存］［保存］（焦点在保存）" },
  { id: "filewin-close-window", scene: "close-window", title: "文件窗口 · 关窗那一问", desc: "有没保存的编辑页与一趟在下的：「关闭文件窗口 · devbox」按族列出（未保存 · 传输中）；［取消］［全部保存后关闭］［仍然关闭］" },
  { id: "filewin-delete-ask", scene: "delete-ask", title: "文件窗口 · 删除那一问", desc: "选中一个文件夹 ＋ 两个文件按 Delete：「删除 3 项」· 一块清单（图标 ＋ 名字 ｜ 右端「文件夹」/ 大小）· 不可恢复 · 取消（焦点）· 删除 3 项（危险）" },
  { id: "filewin-rename-denied", scene: "rename-denied", title: "文件窗口 · 就地改名 · 被那台拒", desc: "main.rs 改成 retry.rs ⇒ 那台回「改名失败 · 无权限」（带复制详情）：红字后面跟［复制详情］，点它不算点别处、不再提交" },
  { id: "filewin-rename-error", scene: "rename-error", title: "文件窗口 · 就地改名 · 填错", desc: "main.rs 上 F2：名字那一格成了输入框，改成 Cargo.toml 回车 ⇒ 后端回「已存在」：红边 ＋ 格子下面挂一句「Cargo.toml 已存在」，那一格留着" },
  { id: "filewin-new-folder", scene: "new-folder", title: "文件窗口 · 就地新建文件夹 ＋ 改名回执", desc: "「新建 ▾ → 文件夹」：文件夹那一段之后冒出一行「新建文件夹」、名字整个选中；右下角是上一次改名的回执「已改名为 retry.rs［撤销］」" },
  { id: "filewin-chmod", scene: "chmod", title: "文件窗口 · 改权限", desc: "「⋯ → 权限」：「改权限 · main.rs」·「当前 644」· 3 × 3 勾（勾了所有者 / 同组 / 其他人的执行）·「数字写法 755」两边联动 ·［取消］［改权限］" },
  { id: "filewin-upload-clash", scene: "upload-clash", title: "文件窗口 · 上传遇到同名", desc: "上传三个文件、两个同名：整组一条开单，那台后端探了回同名清单 ⇒ 同一个冲突框「上传 3 项到 …」·「同名 2 项」·清单（要放过去的 → 已有的，时刻是后端写好的字）·［逐项选］·［取消］［跳过同名］（默认）［保留两份］［覆盖］（危险）" },
  { id: "filewin-cross-copy", scene: "cross-copy", title: "文件窗口 · 复制到另一台", desc: "report.pdf「复制到另一台」：「复制到」下拉 gpu-01 ·「放到」那台的面包屑 ＋ 文件夹（选中 inbox）· 右上「新建文件夹」·「目标 gpu-01:…/inbox」·［取消］［复制到 gpu-01］" },
  { id: "filewin-drag-in", scene: "drag-in", title: "文件窗口 · 从桌面拖进来", desc: "拖着两个文件经过列表：列表区一层淡强调色底 ＋ 虚线框「松开上传 → orders-service（2 个文件）」" },
  { id: "filewin-narrow-drawer", scene: "narrow-drawer", width: 720, title: "文件窗口 · 窄档 720 · 左栏抽屉", desc: "窗宽 720：左栏收成盖在列表上的抽屉（不挤列表），开关打开着" },
  { id: "filewin-peek", scene: "peek", width: 720, title: "文件窗口 · 窄档 · 空格看一眼", desc: "窗宽 720、main.rs 上按空格：窗口正中一块「看一眼」浮层（代码高亮）；Esc / 空格 / 点别处收" },
  { id: "filewin-plan", scene: "plan", title: "文件窗口 · 计划反查", desc: "目录落在一片的仓库里：列表多一列「节点」（状态图标 ＋「编号 标题」；被几格声明 ⇒ 标题后面一段灰字「共用 · 也在 A3-2 重试策略」）；选中 main.rs，预览头多一块「归属」（标题 ·［在计划里看］· 仓库 · 权力范围 · 同一段灰字）。回包是合成的（真后端 `plan-files` 判）" },
  { id: "filewin-plan-menu", scene: "plan-menu", title: "文件窗口 · 右键 · 在计划里看", desc: "main.rs 上右键：删除之前多一项「在计划里看 · 下单入口」；不归哪一格的文件这一项不出" },
  { id: "filewin-view", scene: "view", title: "文件窗口 · 图片查看页", desc: "docs 里回车 architecture.png ⇒ 同一栏开一个查看页（标签图片图标 ＋ 名字）：头条面包屑 ·「‹ 1 / 2 ›」· 适应窗口（亮）· 原尺寸 · − 百分比 + · 下载；画面里整张放进来、透明处棋盘格；状态栏「1600 × 1000 · 大小 · 时间」" },
  { id: "filewin-view-menu", scene: "view-menu", title: "文件窗口 · 右键查看 ＋ 预览头查看", desc: "docs 里选中 architecture.png、右键：第一项「查看」（图片不给「编辑」）；预览头上「查看」「下载」，预览里摆的是与查看页共用的那一份" },
  { id: "filewin-multi-menu", scene: "multi-menu", title: "文件窗口 · 多选 ＋ 右键菜单", desc: "Ctrl 点选 docs · main.rs · release.tar.gz、在压缩包上右键：多选那一种只给「算大小」「改权限…」｜「删除 3 项…」（红、带 Delete）；每一项图标 ＋ 字 ＋ 右端快捷键，不画框。还该有的「下载 3 项」「复制到另一台…」「复制到另一栏」多选还做不了，没有" },
  { id: "filewin-blank-menu", scene: "blank-menu", title: "文件窗口 · 空白处右键", desc: "列表最后一行下面右键：选中清掉，菜单对这个目录说话 ——「文件夹」（Ctrl+Shift+N）·「空文件」·「上传」（Ctrl+U）｜「终端」（照命令栏「这个目录」那一组）" },
  { id: "filewin-focus", scene: "focus", title: "文件窗口 · 键盘焦点环", desc: "按 Tab 走到搜索框：2px 强调色外环（鼠标点出来的不画）" },
  { id: "filewin-many-menu", scene: "many-menu", title: "文件窗口 · 多选右键菜单", desc: "（按两栏摆菜单）选中一个文件夹 ＋ 两个文件右键：「下载 3 项」灰着（有目录，悬停说哪几项）·「复制 3 项到另一台…」·「复制 3 项到另一栏」｜ 算大小 · 改权限 ｜ 删除 3 项" },
  { id: "filewin-batch-ask", scene: "batch-ask", title: "文件窗口 · 同名冲突框", desc: "复制 3 项到另一栏，后端回同名 3 项：清单（要放过去的 → 已有的；目录那一行写不合并）·［逐项选］·［取消］［跳过同名］（默认）［保留两份］［覆盖］（危险）" },
  { id: "filewin-batch-end", scene: "batch-end", title: "文件窗口 · 结局行两颗", desc: "「进度」表：上传一摞停了 ⇒［继续］（没传的那几项再开单，续传先对尾块）· 复制到另一台部分失败 ⇒［重试失败的］（只重没成的那一项，另一张票）；状态栏「已选 3 项 · 大小」（都是文件才写大小）" },
  { id: "filewin-batch-done", scene: "batch-done", title: "文件窗口 · 一组全成", desc: "一组复制全成 ⇒ 右下角回执「已复制 2 项 → ~/work/b」（后端那一行原样）；「进度」表收着" },
  { id: "filewin-batch-each", scene: "batch-each", title: "文件窗口 · 同名冲突框 · 逐项选", desc: "逐项选展开：每行一个三段（跳过｜保留两份｜覆盖；目录只给前两段）·［取消］［确定］" },
  { id: "filewin-big-preview", scene: "big-preview", title: "文件窗口 · 预览 · 文本太大", desc: "notes.txt 换成 300 K：预览那一栏摆后端那道预览门写好的那一句（预览上限 64 K · 可点「编辑」打开），不读" },
  { id: "filewin-big-image", scene: "big-image", title: "文件窗口 · 预览 · 图太大", desc: "logo.png 换成 6 M：预览那一栏摆后端那一句（超过图片预览上限 4 M · 不预览 · 可下载），头上照样给「查看」「下载」" },
  { id: "filewin-big-edit", scene: "big-edit", title: "文件窗口 · 编辑 · 超编辑上限", desc: "notes.txt 换成 9 M、回车编辑：编辑页正中摆后端那道编辑门写好的那一句（多多少字节 · 不截断），没发读" },
  { id: "filewin-big-view", scene: "big-view", title: "文件窗口 · 查看页 · 图太大", desc: "docs 里 architecture.png 换成 40 M、查看：画面正中摆后端那道查看门写好的那一句（超过查看上限 32 M · 可下载），没读" },
].map((s) => ({ dir: "文件窗口", width: 1280, height: 800, ...s }));

export async function shootFilewin({ repo, sandbox, out, scenes, env, buildEnv, children }) {
  const results = [];
  const problems = [];
  const crate = path.join(repo, "src/frontend/filewin");
  console.log("文件窗口：编测试二进制（第一次要几分钟）…");
  const build = spawnSync("cargo", ["test", "--lib", "--no-run", "--message-format=json"], {
    cwd: crate,
    // 编译要真 HOME（rustup / cargo 的家），别的照摘
    env: { ...buildEnv, CARGO_BUILD_JOBS: process.env.CARGO_BUILD_JOBS ?? "3" },
    encoding: "utf8",
    maxBuffer: 256 * 1024 * 1024,
    stdio: ["ignore", "pipe", "inherit"],
  });
  const exe = build.stdout
    .split("\n")
    .filter((l) => l.startsWith("{"))
    .map((l) => JSON.parse(l))
    .find((m) => m.reason === "compiler-artifact" && m.profile?.test && m.target?.name === "cc_monitor_filewin" && m.executable)?.executable;
  if (build.status !== 0 || !exe) {
    problems.push(`文件窗口：测试二进制没编出来（cargo 退出码 ${build.status}）`);
    return { results, problems };
  }

  const x = await privateXvfb({ repo, env, children });
  if (x === null) {
    problems.push("文件窗口：Xvfb 没报号就退了（起不来）");
    return { results, problems };
  }
  const { display } = x;

  for (const s of scenes) {
    const file = path.join(out, s.dir, `${s.id}.png`);
    mkdirSync(path.dirname(file), { recursive: true });
    // 合成的家目录：地址栏末几段读起来像一台真机器（…/home/user/work/orders-service）
    const home = path.join(sandbox, "filewin", s.scene, "home", "user");
    layFixture(home);
    const runEnv = {
      ...env,
      DISPLAY: `:${display}`,
      CCM_SHOTS_FILEWIN_SCENE: s.scene,
      CCM_SHOTS_FILEWIN_OUT: file,
      CCM_SHOTS_FILEWIN_DIR: home,
      CCM_SHOTS_FILEWIN_PREVIEW: CODE,
      CCM_SHOTS_FILEWIN_W: String(s.width),
    };
    delete runEnv.WAYLAND_DISPLAY;
    const r = spawnSync(exe, [TEST, "--exact", "--ignored", "--nocapture", "--test-threads=1"], { env: runEnv, encoding: "utf8", timeout: WALL_CEILING_MS });
    const ok = r.status === 0 && existsSync(file);
    if (!ok) {
      const why = `${r.stderr ?? ""}\n${r.stdout ?? ""}`.split("\n").filter((l) => /panicked|error/i.test(l)).slice(0, 2).join(" | ");
      problems.push(`${s.id}：没截成（退出码 ${r.status}）${why}`);
    }
    results.push({ ...s, file: path.relative(out, file), ok });
    console.log(`${ok ? "✓" : "✗"} ${s.dir}/${s.id}`);
  }
  await x.stop();
  return { results, problems };
}

/**
 * 起一台私有 Xvfb（文件窗口这一半与 WebKitGTK 那一份浏览器同用）。起法只住 tests/scripts/xvfb-free.sh 那一份：
 * 它挑空号、占住，号写在 Xvfb 标准输出第一行；收场（`stop`）照它的 release 删自己那把锁。起不来 ⇒ `null`。
 */
export async function privateXvfb({ repo, env, children }) {
  const xvfb = spawn("bash", [path.join(repo, XVFB_LAUNCHER)], { env, stdio: ["ignore", "pipe", "ignore"] });
  children.push(xvfb);
  const display = await displayOf(xvfb);
  if (display === null) return null;
  const stop = async () => {
    try {
      process.kill(xvfb.pid, "SIGTERM");
    } catch {
      // 已经退了
    }
    if (xvfb.exitCode === null && xvfb.signalCode === null) await new Promise((r) => xvfb.once("exit", r));
    spawnSync("bash", [path.join(repo, XVFB_LAUNCHER), "release", String(display), String(xvfb.pid)], { stdio: "ignore" });
  };
  return { display, stop };
}

const CODE = `//! 库存客户端：统一的重试与超时。
use std::time::Duration;

/// 一次调用最多重试几次、首次退避多久；之后每次翻倍。
pub struct Retry {
    pub times: u32,
    pub backoff: Duration,
}

impl Retry {
    pub fn delay(&self, attempt: u32) -> Duration {
        // 第 n 次重试前等 backoff × 2^n
        self.backoff * 2u32.pow(attempt)
    }
}

fn main() {
    let r = Retry { times: 3, backoff: Duration::from_millis(200) };
    for i in 0..r.times {
        println!("第 {} 次重试前等 {:?}", i + 1, r.delay(i));
    }
}
`;

/**
 * 铺这一张要的真盘那几份：家目录本身（书签那份文件落在这里）＋ 「上传遇到同名」那一张本机那一侧的三份（两份与目录里同名）。
 * 远端那棵树不在这里：截图那一格自己铺在合成后端那台机器的盘上（`workspace_tests.rs` 的 `lay_shots_tree`，内存里，不落盘）。
 */
function layFixture(home) {
  rmSync(home, { recursive: true, force: true });
  mkdirSync(home, { recursive: true });
  const at = (iso) => new Date(iso);
  const local = path.join(home, "..", "..", "local");
  mkdirSync(local, { recursive: true });
  for (const [name, n, t] of [["main.rs", 612, "2026-10-01T13:50:00"], ["README.md", 34, "2026-09-12T10:00:00"], ["new.txt", 9, "2026-10-01T13:50:00"]]) {
    writeFileSync(path.join(local, name), Buffer.alloc(n, 0x61));
    utimesSync(path.join(local, name), at(t), at(t));
  }
}

/** Xvfb 报的号（标准输出第一行）；没报就退了、或 10 秒内没报 ⇒ `null`。 */
function displayOf(child) {
  return new Promise((resolve) => {
    let buf = "";
    const done = (v) => {
      clearTimeout(timer);
      child.stdout.removeAllListeners("data");
      resolve(v);
    };
    const timer = setTimeout(() => done(null), 10_000);
    child.stdout.on("data", (b) => {
      buf += b.toString("utf8");
      const nl = buf.indexOf("\n");
      if (nl >= 0) {
        const n = Number.parseInt(buf.slice(0, nl).trim(), 10);
        done(Number.isInteger(n) ? n : null);
      }
    });
    child.on("exit", () => done(null));
  });
}
