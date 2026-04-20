# Keep JNI entry points and foreground playback components stable under R8.
-keep class com.example.lan_audio_android_mvp.OpusNativeDecoder { *; }
-keep class com.example.lan_audio_android_mvp.OpusFrameDecoder { *; }
-keepclasseswithmembers class * {
    native <methods>;
}

# Flutter and Media3 use reflection around service/activity entry points.
-keep class com.example.lan_audio_android_mvp.MainActivity { *; }
-keep class com.example.lan_audio_android_mvp.PlaybackForegroundService { *; }
