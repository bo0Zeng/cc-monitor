// 中立的纯 JVM 叶子模块：只放一次性远端 exec 的抽象 RemoteExecutor 与 ConnectionDeadException（core-ssh 实现）。
plugins {
    alias(libs.plugins.kotlin.jvm)
}

java {
    sourceCompatibility = JavaVersion.VERSION_11
    targetCompatibility = JavaVersion.VERSION_11
}
