# sshj + BouncyCastle 在 R8 full-mode 下要保留（反射加载 provider 与算法）。debug 不开 minify，只影响 release。
-keep class org.bouncycastle.** { *; }
-dontwarn org.bouncycastle.**
-keep class net.schmizz.sshj.** { *; }
-keep class com.hierynomus.** { *; }
-dontwarn net.schmizz.sshj.**
# net.i2p.crypto.eddsa 是 sshj 加载 Ed25519 / OpenSSH-v1 私钥的传递依赖。
# 注意：只写 -dontwarn 不写 -keep 的话，R8 full-mode 会删光整个包，release 下 loadKeys(ed25519) 抛
# NoClassDefFoundError，Ed25519 身份连不上。ECDSA / RSA 走 BC，不受影响。整包 keep。
-keep class net.i2p.crypto.eddsa.** { *; }
-dontwarn net.i2p.crypto.**
-dontwarn org.slf4j.**
# sshj 通过反射实例化这些 Factory.Named
-keep class * implements net.schmizz.sshj.common.Factory$Named { *; }
