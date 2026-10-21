package com.woxlium.zombie

/** JNI surface of the Rust core (rust/woxlium_core/src/jni.rs). */
object NativeBridge {
    init {
        System.loadLibrary("woxlium")
    }

    const val MODE_ENDLESS = 0
    const val MODE_STORY = 1

    const val BTN_ATTACK = 0
    const val BTN_KICK = 1
    const val BTN_DODGE = 2
    const val BTN_HEAL = 3
    const val BTN_BLOCK = 4

    // texture slots, in the order the renderer expects them
    val TEXTURES = arrayOf(
        "sprites.webp", "tex_asphalt.webp", "tex_grass.webp", "tex_mud.webp", "tex_dirt.webp",
        "tex_concrete.webp", "tex_concrete_cracked.webp", "tex_concrete_holes.webp",
    )

    // HUD indices (game.rs HUD_*)
    const val HUD_HP = 0
    const val HUD_HP_MAX = 1
    const val HUD_STAMINA = 2
    const val HUD_STAMINA_MAX = 3
    const val HUD_MEDKITS = 4
    const val HUD_WAVE = 5
    const val HUD_ENEMIES = 6
    const val HUD_KILLS = 7
    const val HUD_SCORE = 8
    const val HUD_COMBO = 9
    const val HUD_STATE = 10
    const val HUD_BREAK = 11
    const val HUD_OBJECTIVE = 12
    const val HUD_KICK_KILLS = 13
    const val HUD_MAX_COMBO = 14
    const val HUD_TIME = 15
    const val HUD_EXTRACT_DIST = 16
    const val HUD_STORY_WAVES = 17
    const val HUD_FPS = 18
    const val HUD_MODE = 19

    // event bits (game.rs EV_*)
    const val EV_SWING = 1 shl 0
    const val EV_HIT = 1 shl 1
    const val EV_KICK_HIT = 1 shl 2
    const val EV_CRIT = 1 shl 3
    const val EV_ZOMBIE_DIE = 1 shl 4
    const val EV_PLAYER_HURT = 1 shl 5
    const val EV_BLOCK = 1 shl 6
    const val EV_DODGE = 1 shl 7
    const val EV_HEAL = 1 shl 8
    const val EV_WAVE_START = 1 shl 9
    const val EV_WAVE_CLEAR = 1 shl 10
    const val EV_GROWL = 1 shl 11
    const val EV_PICKUP = 1 shl 12
    const val EV_NO_STAMINA = 1 shl 13
    const val EV_GAME_OVER = 1 shl 14
    const val EV_VICTORY = 1 shl 15
    const val EV_EXPLODE = 1 shl 16

    @JvmStatic external fun nativeStart(mode: Int, skin: Int, seed: Int)
    @JvmStatic external fun nativeSetTexture(slot: Int, glId: Int)
    @JvmStatic external fun nativeSurfaceCreated()
    @JvmStatic external fun nativeSurfaceChanged(width: Int, height: Int)
    /** Advances and draws one frame on the GL thread; returns EV_* bits. */
    @JvmStatic external fun nativeFrame(dt: Float): Int
    @JvmStatic external fun nativeMove(x: Float, y: Float)
    @JvmStatic external fun nativeButton(id: Int, down: Boolean)
    @JvmStatic external fun nativePause(paused: Boolean)
    @JvmStatic external fun nativeHud(index: Int): Float
    @JvmStatic external fun nativeSetZoom(zoom: Float)
    @JvmStatic external fun nativeSetMinimap(x: Float, y: Float, size: Float)
    @JvmStatic external fun nativeSetMapExpanded(expanded: Boolean)
}
