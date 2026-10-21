package com.woxlium.zombie

import android.content.Context
import android.content.SharedPreferences

/** Settings, lifetime stats and quest progress (SharedPreferences). */
class Prefs(context: Context) {
    private val sp: SharedPreferences = context.getSharedPreferences("woxlium", Context.MODE_PRIVATE)

    var sfxVolume: Int
        get() = sp.getInt("sfx", 80)
        set(v) = sp.edit().putInt("sfx", v).apply()
    var vibration: Boolean
        get() = sp.getBoolean("vib", true)
        set(v) = sp.edit().putBoolean("vib", v).apply()
    /** 0 = low (60 % render scale), 1 = medium (80 %), 2 = high (100 %) */
    var quality: Int
        get() = sp.getInt("quality", 1)
        set(v) = sp.edit().putInt("quality", v).apply()
    var fps60: Boolean
        get() = sp.getBoolean("fps60", true)
        set(v) = sp.edit().putBoolean("fps60", v).apply()
    var showFps: Boolean
        get() = sp.getBoolean("showfps", false)
        set(v) = sp.edit().putBoolean("showfps", v).apply()
    /** look sensitivity, percent */
    var sensitivity: Int
        get() = sp.getInt("sens", 100)
        set(v) = sp.edit().putInt("sens", v).apply()

    val renderScale: Float get() = when (quality) { 0 -> 0.6f; 1 -> 0.8f; else -> 1f }
    val shadows: Boolean get() = quality > 0

    // lifetime stats
    val totalKills get() = sp.getInt("kills", 0)
    val bestWave get() = sp.getInt("bestWave", 0)
    val bestScore get() = sp.getInt("bestScore", 0)
    val bestCombo get() = sp.getInt("bestCombo", 0)
    val storyWins get() = sp.getInt("storyWins", 0)
    val games get() = sp.getInt("games", 0)

    fun recordGame(kills: Int, wave: Int, score: Int, combo: Int, storyWin: Boolean, endless: Boolean) {
        sp.edit()
            .putInt("kills", totalKills + kills)
            .putInt("bestWave", if (endless) maxOf(bestWave, wave) else bestWave)
            .putInt("bestScore", maxOf(bestScore, score))
            .putInt("bestCombo", maxOf(bestCombo, combo))
            .putInt("storyWins", storyWins + if (storyWin) 1 else 0)
            .putInt("games", games + 1)
            .apply()
    }

    data class Quest(val title: String, val desc: String, val progress: Int, val goal: Int) {
        val done get() = progress >= goal
    }

    fun quests(): List<Quest> = listOf(
        Quest("İLK KAN", "1 zombi öldür", totalKills, 1),
        Quest("KASAP", "Toplam 100 zombi öldür", totalKills, 100),
        Quest("SOKAK SAVAŞÇISI", "Toplam 500 zombi öldür", totalKills, 500),
        Quest("REKORTMEN", "Tek oyunda 5000 puan topla", bestScore, 5000),
        Quest("KOMBO USTASI", "Tek seferde 15'li kombo yap", bestCombo, 15),
        Quest("HAYATTA KALAN", "Sonsuz Dalga'da 5. dalgaya ulaş", bestWave, 5),
        Quest("EFSANE", "Sonsuz Dalga'da 10. dalgaya ulaş", bestWave, 10),
        Quest("TAHLİYE", "Hikaye modunu tamamla", storyWins, 1),
    )
}
