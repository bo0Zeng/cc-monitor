package com.ccmonitor.mobile

import android.app.Activity
import android.app.Application
import android.net.ConnectivityManager
import android.net.Network
import android.os.Bundle
import com.ccmonitor.mobile.core.data.db.STATE_DB_FILE
import com.ccmonitor.mobile.core.data.db.StateDbAge
import com.ccmonitor.mobile.core.data.db.stateDbAge
import com.ccmonitor.mobile.core.data.db.stateDbVersion
import com.ccmonitor.mobile.core.data.di.dataModule
import com.ccmonitor.mobile.core.ssh.di.sshModule
import com.ccmonitor.mobile.core.ssh.installRustCore
import com.ccmonitor.mobile.dev.DevSeeder
import com.ccmonitor.mobile.di.appModule
import com.ccmonitor.mobile.link.HostBackends
import com.ccmonitor.mobile.service.AppForeground
import com.ccmonitor.mobile.service.ForegroundTracker
import com.ccmonitor.mobile.service.KeepAliveWork
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.launch
import org.koin.android.ext.koin.androidContext
import org.koin.android.ext.koin.androidLogger
import org.koin.core.context.startKoin
import java.io.File

class CcMonitorApp : Application() {
    /** 盘上那份状态库比还收的最老那一版还老 ⇒ 那一版（App 什么都不起，[MainActivity] 整屏说一句）；开得了 ⇒ `null`。 */
    var refused: StateDbAge.TooOld? = null
        private set

    override fun onCreate() {
        super.onCreate()
        // 库先看一眼：太老的不开、不迁（开了 Room 就照缺的那几跳抛），什么都不起，库一字不动。
        refused = stateDbAge(stateDbVersion(getDatabasePath(STATE_DB_FILE))) as? StateDbAge.TooOld
        if (refused != null) return
        val koin =
            startKoin {
                androidLogger()
                androidContext(this@CcMonitorApp)
                modules(dataModule, sshModule, appModule)
            }.koin
        // 核心先装（主机钥匙那份库在 App 私有目录，会话列表副本在缓存目录）：之后任何一次拨号都只认那份库。
        installRustCore(File(filesDir, "known_hosts"), File(cacheDir, "lists"))
        // 维护前台标志，并在回到前台时交给每台的常驻流（共用那条重连循环）。
        val backends = koin.get<HostBackends>()
        registerForegroundTracking(backends)
        // 网回来 · 网变了：App 一起来就挂上、直接交给那条重连循环，不等保活服务（它要连上过才起：冷启动时断网就没人重拨）。
        registerNetwork(backends)
        // 保活服务在「有活」那一下起（终端连着 · 在飞轮次 · 核心里在传的），不挂在哪一屏上；没活时服务自停。
        koin.get<KeepAliveWork>().startOnWork(CoroutineScope(SupervisorJob() + Dispatchers.Main), this)

        // 只在 debug 构建播种演示主机和身份；dev key 资产也只在 src/debug。
        if (BuildConfig.DEBUG) {
            val seeder = koin.get<DevSeeder>()
            CoroutineScope(SupervisorJob() + Dispatchers.IO).launch { seeder.seedIfEmpty() }
        }
    }

    /**
     * 维护 [AppForeground]，并在 0→1（回到前台，含冷启动）时把每台的常驻流交给核心（[HostBackends.wake]：连着的当场问活、
     * 断着的当场重接；SSH 那层跟着它换连接）。网络恢复由 [com.ccmonitor.mobile.service.SshKeepAliveService] 交同一个口；
     * 纯回前台（同网络下服务端 idle 超时、NAT 重绑静默掐断）只能靠这里。可以在主线程调：只是转一句话。
     */
    private fun registerNetwork(backends: HostBackends) {
        val cm = getSystemService(ConnectivityManager::class.java) ?: return
        runCatching {
            cm.registerDefaultNetworkCallback(
                object : ConnectivityManager.NetworkCallback() {
                    override fun onAvailable(network: Network) = backends.wake()
                },
            )
        }
    }

    private fun registerForegroundTracking(backends: HostBackends) {
        val tracker =
            ForegroundTracker(
                onEnterForeground = { backends.wake() },
                setForeground = { AppForeground.isForeground = it },
            )
        registerActivityLifecycleCallbacks(
            object : ActivityLifecycleCallbacks {
                override fun onActivityStarted(activity: Activity) = tracker.onActivityStarted()

                override fun onActivityStopped(activity: Activity) = tracker.onActivityStopped()

                override fun onActivityCreated(
                    activity: Activity,
                    savedInstanceState: Bundle?,
                ) = Unit

                override fun onActivityResumed(activity: Activity) = Unit

                override fun onActivityPaused(activity: Activity) = Unit

                override fun onActivitySaveInstanceState(
                    activity: Activity,
                    outState: Bundle,
                ) = Unit

                override fun onActivityDestroyed(activity: Activity) = Unit
            },
        )
    }
}
