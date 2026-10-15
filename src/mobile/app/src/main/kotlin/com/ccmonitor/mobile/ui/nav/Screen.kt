package com.ccmonitor.mobile.ui.nav

import java.net.URLEncoder

/** 全局导航路由。 */
sealed class Screen(
    val route: String,
) {
    /**
     * 启动落地判定屏（`startDestination`）。
     *
     * 打开即进聊天屏要等一个异步事实：上次那台机器（核对过还在）；
     * 而 `startDestination` 建图时就得给出字符串。所以起点是这个不画东西的判定屏，
     * 读到答案后用 `popUpTo(inclusive = true)` 替换自己，返回键不会退回它。
     * 没有上次那台时去 [Hosts]。
     */
    data object Launch : Screen("launch")

    data object Hosts : Screen("hosts")

    data object HostEdit : Screen("host/{id}/edit")

    data object Identities : Screen("identities")

    // 按 tab key 路由，hostId/cd/launcherId 由 SessionTabManager.get(key) 提供；VM 经 per-key store 保活。
    data object Session : Screen("session/{key}?open={open}") {
        /** 文件 tab 进来时要打开的那个路径（可缺）。 */
        const val ARG_OPEN = "open"

        fun of(
            key: String,
            open: String,
        ): String = "session/$key?$ARG_OPEN=" + URLEncoder.encode(open, "UTF-8").replace("+", "%20")
    }

    // 终端 tab 总览。注意：与 [Conversations]（Claude 对话总览）是两件事，
    // 后者的路由刻意不用 `sessions*` 前缀以免混淆。
    data object SessionsOverview : Screen("sessions")

    /** Claude 对话总览：历史 / 正在跑 / 需手动 三分区。对用户说「对话」，不说「会话」。 */
    data object Conversations : Screen("conversations/{hostId}") {
        fun of(hostId: String): String = "conversations/$hostId"
    }

    /**
     * 一个对话。由「主机 + 对话编号」共同确定，只用编号的话两台主机上同号的对话会撞（见 `chatKey`）。
     */
    data object Chat : Screen("chat/{hostId}/{sid}?new={new}&cwd={cwd}") {
        /**
         * [isNew] = 对话刚开，远端还不存在。`ChatRoute` 据此决定加不加 `--resume <编号>`：
         * 对远端没见过的编号 resume 会直接失败（`--resume` 还按目录划分作用域）。
         * 缺省 `false`：从列表点进来的对话确实已存在。
         */
        fun of(
            hostId: String,
            sid: String,
            isNew: Boolean = false,
            /** 新建会话从哪进来的那个目录（见 `ChatRoute.draftCwd`）。 */
            cwd: String? = null,
        ): String = "chat/$hostId/$sid?new=$isNew" + (cwd?.let { "&cwd=" + URLEncoder.encode(it, "UTF-8").replace("+", "%20") } ?: "")

        const val ARG_NEW = "new"
        const val ARG_CWD = "cwd"
    }

    // 「新建会话 · 选主机」：复用 HostListScreen，但连接一律 forceNew=true；
    // 选中即 popUpTo 弹出本页，返回键即取消，不留 force-new 状态。
    data object NewSessionHostPicker : Screen("sessions/new")

    data object Settings : Screen("settings")

    /** 编辑按键（终端屏 ⋯「编辑按键」· 绑了「编辑按键」的触发进来；只有这一页）。 */
    data object Keys : Screen("keys")

    /** 轮换屏：一条会话的（[sid]）· 这台新会话用的那份（[sid] 缺）。 */
    data object Rotation : Screen("rotation/{hostId}?sid={sid}") {
        fun of(
            hostId: String,
            sid: String? = null,
        ): String = "rotation/$hostId" + (sid?.let { "?sid=$it" } ?: "")

        const val ARG_SID = "sid"
    }

    /** 设置 → 机器与账号 → 轮换：当前服务器（上次那台）新会话用的那份。 */
    data object RotationHere : Screen("settings/rotation")
}
