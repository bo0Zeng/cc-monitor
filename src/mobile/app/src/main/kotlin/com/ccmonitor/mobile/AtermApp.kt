package com.ccmonitor.mobile

import android.app.Activity
import android.app.Application
import android.os.Bundle
import com.ccmonitor.mobile.core.data.di.dataModule
import com.ccmonitor.mobile.core.ssh.SshConnectionManager
import com.ccmonitor.mobile.core.ssh.di.sshModule
import com.ccmonitor.mobile.dev.DevSeeder
import com.ccmonitor.mobile.di.appModule
import com.ccmonitor.mobile.link.HostBackends
import com.ccmonitor.mobile.service.AppForeground
import com.ccmonitor.mobile.service.ForegroundTracker
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.launch
import org.koin.android.ext.koin.androidContext
import org.koin.android.ext.koin.androidLogger
import org.koin.core.context.startKoin

class AtermApp : Application() {
    override fun onCreate() {
        super.onCreate()
        val koin =
            startKoin {
                androidLogger()
                androidContext(this@AtermApp)
                modules(dataModule, sshModule, appModule)
            }.koin
        // 维护前台标志，并在回到前台时探活重连僵死会话。
        registerForegroundTracking(koin.get(), koin.get())

        // 只在 debug 构建播种演示主机和身份；dev key 资产也只在 src/debug。
        if (BuildConfig.DEBUG) {
            val seeder = koin.get<DevSeeder>()
            CoroutineScope(SupervisorJob() + Dispatchers.IO).launch { seeder.seedIfEmpty() }
        }
    }

    /**
     * 维护 [AppForeground]，并在 0→1（回到前台，含冷启动）时调 [SshConnectionManager.probeAndReconnectStale]，
     * 再让每台断着的常驻流当场重接（[HostBackends.onForeground]；SSH 那层重连过的那几台由流自己跟上）。
     * 网络恢复由 [com.ccmonitor.mobile.service.SshKeepAliveService] 触发探测；纯回前台（同网络下服务端 idle 超时、
     * NAT 重绑静默掐断）只能靠这里，否则首次交互要吃满 15s openShell 看门狗。
     * 探测可以在主线程调：临界区很短，网络往返在 reconnectScope 里。
     */
    private fun registerForegroundTracking(
        manager: SshConnectionManager,
        backends: HostBackends,
    ) {
        val tracker =
            ForegroundTracker(
                onEnterForeground = {
                    manager.probeAndReconnectStale()
                    backends.onForeground()
                },
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
