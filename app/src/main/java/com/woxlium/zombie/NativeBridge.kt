package com.woxlium.zombie

import android.content.res.AssetManager

/** JNI surface of the Rust core (rust/woxlium_core/src/jni.rs). */
object NativeBridge {
    init {
        System.loadLibrary("woxlium")
    }

    const val MODE_ENDLESS = 0
    const val MODE_STORY = 1

    const val BTN_ATTACK = 0
    const val BTN_DODGE = 2
    const val BTN_BLOCK = 4

    // HUD indices (game.rs HUD_*); HUD_FPS is served by the engine
    const val HUD_HP = 0
    const val HUD_HP_MAX = 1
    const val HUD_STAMINA = 2
    const val HUD_STAMINA_MAX = 3
    const val HUD_WAVE = 4
    const val HUD_ENEMIES = 5
    const val HUD_KILLS = 6
    const val HUD_SCORE = 7
    const val HUD_COMBO = 8
    const val HUD_STATE = 9
    const val HUD_BREAK = 10
    const val HUD_OBJECTIVE = 11
    const val HUD_MAX_COMBO = 12
    const val HUD_TIME = 13
    const val HUD_EXTRACT_DIST = 14
    const val HUD_MODE = 15
    const val HUD_FPS = 16

    // event bits (game.rs EV_*)
    const val EV_SWING = 1 shl 0
    const val EV_HIT = 1 shl 1
    const val EV_HEAVY_HIT = 1 shl 2
    const val EV_CRIT = 1 shl 3
    const val EV_ZOMBIE_DIE = 1 shl 4
    const val EV_PLAYER_HURT = 1 shl 5
    const val EV_BLOCK = 1 shl 6
    const val EV_DODGE = 1 shl 7
    const val EV_HEAL = 1 shl 8
    const val EV_WAVE_START = 1 shl 9
    const val EV_WAVE_CLEAR = 1 shl 10
    const val EV_GROWL = 1 shl 11
    const val EV_NO_STAMINA = 1 shl 13
    const val EV_GAME_OVER = 1 shl 14
    const val EV_VICTORY = 1 shl 15
    const val EV_STEP = 1 shl 16

    @JvmStatic external fun nativeInit(assets: AssetManager)
    @JvmStatic external fun nativeStart(mode: Int, seed: Int)
    /** Loads meshes/textures on the GL thread; false if the device lacks GLES 3 features. */
    @JvmStatic external fun nativeSurfaceCreated(): Boolean
    @JvmStatic external fun nativeSurfaceChanged(width: Int, height: Int)
    /** Advances and draws one frame on the GL thread; returns EV_* bits. */
    @JvmStatic external fun nativeFrame(dt: Float): Int
    @JvmStatic external fun nativeMove(x: Float, y: Float)
    /** Look delta in radians (x = yaw, y = pitch). */
    @JvmStatic external fun nativeLook(dx: Float, dy: Float)
    @JvmStatic external fun nativeButton(id: Int, down: Boolean)
    @JvmStatic external fun nativePause(paused: Boolean)
    @JvmStatic external fun nativeHud(index: Int): Float
    @JvmStatic external fun nativeSetMinimap(x: Float, y: Float, size: Float)
    @JvmStatic external fun nativeSetMapExpanded(expanded: Boolean)
    @JvmStatic external fun nativeSetShadows(on: Boolean)
}
