// 中立的纯 JVM 叶子模块：只放通用远端 exec 抽象 RemoteCommandChannel 与 ConnectionDeadException。
// core-ssh（实现）与 core-claude（消费）都依赖它，而彼此不依赖；core-claude 因此保持纯 JVM。
plugins {
    alias(libs.plugins.kotlin.jvm)
}

java {
    sourceCompatibility = JavaVersion.VERSION_11
    targetCompatibility = JavaVersion.VERSION_11
}

dependencies {
    // RemoteCommandChannel.exec 返回 Flow<ByteArray>——用 -core（纯 JVM artifact），不引 Android 耦合。
    implementation(libs.kotlinx.coroutines.core)

    // shellQuote 是全仓唯一的实现，它的测试在这里。
    testImplementation(libs.junit)
}
