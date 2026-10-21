# JNI entry points are resolved by name from libwoxlium.so
-keepclasseswithmembernames class com.woxlium.zombie.NativeBridge {
    native <methods>;
}
