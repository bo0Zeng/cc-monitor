package com.ccmonitor.mobile.ui.nav

/** 全局导航路由。 */
sealed class Screen(
    val route: String,
) {
    /**
     * 启动落地判定屏（`startDestination`）。
     *
     * 打开即进聊天屏要等三个异步事实：新界面开关、上次那台机器、那台机器上次的对话；
     * 而 `startDestination` 建图时就得给出字符串。所以起点是这个不画东西的判定屏，
     * 读到答案后用 `popUpTo(inclusive = true)` 替换自己，返回键不会退回它。
     * 开关关着时去 [Hosts]。
     */
    data object Launch : Screen("launch")

    data object Hosts : Screen("hosts")

    data object HostEdit : Screen("host/{id}/edit")

    data object Identities : Screen("identities")

    // 按 tab key 路由，hostId/cd/launcherId 由 SessionTabManager.get(key) 提供；VM 经 per-key store 保活。
    data object Session : Screen("session/{key}")

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
    data object Chat : Screen("chat/{hostId}/{sid}?new={new}") {
        /**
         * [isNew] = 对话刚开，远端还不存在。`ChatRoute` 据此决定加不加 `--resume <编号>`：
         * 对远端没见过的编号 resume 会直接失败（`--resume` 还按目录划分作用域）。
         * 缺省 `false`：从列表点进来的对话确实已存在。
         */
        fun of(
            hostId: String,
            sid: String,
            isNew: Boolean = false,
        ): String = "chat/$hostId/$sid?new=$isNew"

        const val ARG_NEW = "new"
    }

    // 「新建会话 · 选主机」：复用 HostListScreen，但连接一律 forceNew=true；
    // 选中即 popUpTo 弹出本页，返回键即取消，不留 force-new 状态。
    data object NewSessionHostPicker : Screen("sessions/new")

    data object Settings : Screen("settings")

    // 自定义按钮编辑器：全局按钮从设置进，主机专属按钮从主机编辑器进。
    data object ButtonSettings : Screen("settings/buttons")

    data object HostButtons : Screen("host/{id}/buttons")
}
