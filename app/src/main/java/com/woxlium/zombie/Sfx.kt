package com.woxlium.zombie

import android.content.Context
import android.media.AudioAttributes
import android.media.SoundPool
import android.os.Build
import android.os.SystemClock
import android.os.VibrationEffect
import android.os.Vibrator
import java.io.File
import java.io.FileOutputStream
import java.nio.ByteBuffer
import java.nio.ByteOrder
import kotlin.math.PI
import kotlin.math.exp
import kotlin.math.sin
import kotlin.random.Random

/**
 * Procedurally synthesised sound effects (no audio assets ship with the APK)
 * plus haptics. Safe to call from the GL thread.
 */
class Sfx(context: Context, private val prefs: Prefs) {
    private val pool: SoundPool = SoundPool.Builder()
        .setMaxStreams(12)
        .setAudioAttributes(
            AudioAttributes.Builder()
                .setUsage(AudioAttributes.USAGE_GAME)
                .setContentType(AudioAttributes.CONTENT_TYPE_SONIFICATION)
                .build()
        ).build()
    private val ids = HashMap<String, Int>()
    private val dir = File(context.cacheDir, "sfx_v1").apply { mkdirs() }
    @Volatile private var ready = false
    private var ambienceStream = 0
    private var lastGrowl = 0L
    private val rng = Random(7)
    @Suppress("DEPRECATION")
    private val vibrator = context.getSystemService(Context.VIBRATOR_SERVICE) as? Vibrator

    init {
        Thread({
            for ((name, gen) in sounds) {
                val f = File(dir, "$name.wav")
                if (!f.exists() || f.length() < 64) writeWav(f, gen())
                ids[name] = pool.load(f.absolutePath, 1)
            }
            ready = true
        }, "sfx-synth").start()
    }

    private val volume get() = prefs.sfxVolume / 100f

    fun play(name: String, vol: Float = 1f, rate: Float = 1f) {
        if (!ready) return
        val id = ids[name] ?: return
        val v = (vol * volume).coerceIn(0f, 1f)
        if (v > 0f) pool.play(id, v, v, 1, 0, rate.coerceIn(0.5f, 2f))
    }

    fun click() = play("click", 0.6f)

    fun startAmbience() {
        if (!ready) return
        stopAmbience()
        val id = ids["wind"] ?: return
        ambienceStream = pool.play(id, 0.25f * volume, 0.25f * volume, 0, -1, 1f)
    }

    fun stopAmbience() {
        if (ambienceStream != 0) pool.stop(ambienceStream)
        ambienceStream = 0
    }

    fun handle(ev: Int) {
        if (ev == 0) return
        val r = 0.92f + rng.nextFloat() * 0.16f
        if (ev and NativeBridge.EV_SWING != 0) play("swing", 0.5f, r)
        if (ev and NativeBridge.EV_HIT != 0) { play("punch", 1f, r); buzz(18) }
        if (ev and NativeBridge.EV_KICK_HIT != 0) { play("kick", 1f, r); buzz(30) }
        if (ev and NativeBridge.EV_CRIT != 0) { play("crit", 0.9f); buzz(45) }
        if (ev and NativeBridge.EV_ZOMBIE_DIE != 0) play("zdie", 0.8f, r)
        if (ev and NativeBridge.EV_PLAYER_HURT != 0) { play("hurt", 0.9f, r); buzz(60) }
        if (ev and NativeBridge.EV_BLOCK != 0) { play("block", 0.8f, r); buzz(15) }
        if (ev and NativeBridge.EV_DODGE != 0) play("dodge", 0.7f, r)
        if (ev and NativeBridge.EV_HEAL != 0) play("heal", 0.8f)
        if (ev and NativeBridge.EV_WAVE_START != 0) play("horn", 0.7f)
        if (ev and NativeBridge.EV_WAVE_CLEAR != 0) play("clear", 0.7f)
        if (ev and NativeBridge.EV_PICKUP != 0) play("pickup", 0.8f)
        if (ev and NativeBridge.EV_NO_STAMINA != 0) play("tired", 0.6f)
        if (ev and NativeBridge.EV_EXPLODE != 0) { play("boom", 1f); buzz(80) }
        if (ev and NativeBridge.EV_GAME_OVER != 0) { play("gameover", 1f); buzz(250) }
        if (ev and NativeBridge.EV_VICTORY != 0) play("clear", 1f, 0.8f)
        if (ev and NativeBridge.EV_GROWL != 0) {
            val now = SystemClock.uptimeMillis()
            if (now - lastGrowl > 1400) {
                lastGrowl = now
                play("growl", 0.45f, 0.8f + rng.nextFloat() * 0.4f)
            }
        }
    }

    private fun buzz(ms: Long) {
        if (!prefs.vibration) return
        val v = vibrator ?: return
        if (Build.VERSION.SDK_INT >= 26) {
            v.vibrate(VibrationEffect.createOneShot(ms, VibrationEffect.DEFAULT_AMPLITUDE))
        } else {
            @Suppress("DEPRECATION")
            v.vibrate(ms)
        }
    }

    fun release() {
        stopAmbience()
        pool.release()
    }

    // ------------------------------------------------------------ synthesis
    private companion object {
        const val RATE = 22050

        fun buf(sec: Float) = FloatArray((sec * RATE).toInt())

        /** Fill with a generator f(t, i) shaped by an exponential decay envelope. */
        inline fun tone(sec: Float, decay: Float, attack: Float = 0.004f, f: (Float, Int) -> Float): FloatArray {
            val b = buf(sec)
            for (i in b.indices) {
                val t = i.toFloat() / RATE
                val env = (t / attack).coerceAtMost(1f) * exp(-t * decay)
                b[i] = f(t, i) * env
            }
            return b
        }

        fun noise(seed: Int): () -> Float {
            val r = Random(seed)
            return { r.nextFloat() * 2f - 1f }
        }

        /** one-pole low-pass, in place */
        fun lowpass(b: FloatArray, k: Float): FloatArray {
            var y = 0f
            for (i in b.indices) { y += k * (b[i] - y); b[i] = y }
            return b
        }

        fun mix(vararg parts: FloatArray): FloatArray {
            val out = FloatArray(parts.maxOf { it.size })
            for (p in parts) for (i in p.indices) out[i] += p[i]
            return out
        }

        fun sweep(sec: Float, f0: Float, f1: Float, decay: Float, shape: (Float) -> Float = { sin(it) }): FloatArray {
            var ph = 0f
            return tone(sec, decay) { t, _ ->
                val f = f0 + (f1 - f0) * (t / sec)
                ph += (2f * PI.toFloat() * f) / RATE
                shape(ph)
            }
        }

        val saw: (Float) -> Float = { p -> ((p / (2f * PI.toFloat())) % 1f) * 2f - 1f }

        val sounds: List<Pair<String, () -> FloatArray>> = listOf(
            "punch" to { val n = noise(1); mix(sweep(0.18f, 150f, 55f, 22f), lowpass(tone(0.08f, 45f) { _, _ -> n() }, 0.35f)) },
            "kick" to { val n = noise(2); mix(sweep(0.26f, 110f, 40f, 14f), lowpass(tone(0.12f, 30f) { _, _ -> n() * 0.8f }, 0.25f)) },
            "crit" to { mix(sweep(0.4f, 90f, 35f, 9f), tone(0.45f, 7f) { t, _ -> sin(2 * PI.toFloat() * 880f * t) * 0.25f + sin(2 * PI.toFloat() * 1320f * t) * 0.12f }) },
            "swing" to { val n = noise(3); lowpass(tone(0.16f, 18f, 0.04f) { _, _ -> n() * 0.6f }, 0.18f) },
            "dodge" to { val n = noise(4); lowpass(tone(0.28f, 10f, 0.06f) { _, _ -> n() * 0.7f }, 0.12f) },
            "zdie" to { val n = noise(5); mix(sweep(0.7f, 140f, 50f, 4f, saw).also { lowpass(it, 0.08f) }, lowpass(tone(0.25f, 12f) { _, _ -> n() * 0.6f }, 0.2f)) },
            "hurt" to { val n = noise(6); mix(sweep(0.3f, 220f, 120f, 9f, saw).also { lowpass(it, 0.1f) }, lowpass(tone(0.1f, 30f) { _, _ -> n() }, 0.4f)) },
            "block" to { tone(0.35f, 12f) { t, _ -> sin(2 * PI.toFloat() * 1180f * t) * 0.4f + sin(2 * PI.toFloat() * 1770f * t) * 0.25f + sin(2 * PI.toFloat() * 610f * t) * 0.3f } },
            "heal" to { mix(sweep(0.6f, 440f, 880f, 3.5f), sweep(0.6f, 660f, 1320f, 4f).also { for (i in it.indices) it[i] *= 0.4f }) },
            "pickup" to { mix(tone(0.12f, 18f) { t, _ -> sin(2 * PI.toFloat() * 660f * t) * 0.6f }, FloatArray(2400) + tone(0.15f, 14f) { t, _ -> sin(2 * PI.toFloat() * 990f * t) * 0.6f }) },
            "horn" to { sweep(1.1f, 98f, 92f, 1.6f, saw).also { lowpass(it, 0.06f) } },
            "clear" to { mix(tone(0.9f, 3f) { t, _ -> sin(2 * PI.toFloat() * 523f * t) * 0.35f }, FloatArray(4400) + tone(0.9f, 3f) { t, _ -> sin(2 * PI.toFloat() * 784f * t) * 0.35f }) },
            "tired" to { val n = noise(7); lowpass(tone(0.5f, 5f, 0.12f) { _, _ -> n() * 0.5f }, 0.05f) },
            "growl" to { val n = noise(8); lowpass(tone(1.0f, 2.5f, 0.15f) { t, _ -> n() * (0.6f + 0.4f * sin(2 * PI.toFloat() * 11f * t)) }, 0.03f).also { for (i in it.indices) it[i] *= 3f } },
            "boom" to { val n = noise(9); mix(sweep(0.9f, 80f, 25f, 4f), lowpass(tone(0.9f, 4.5f) { _, _ -> n() }, 0.08f)) },
            "gameover" to { sweep(1.6f, 196f, 65f, 1.4f, saw).also { lowpass(it, 0.05f) } },
            "click" to { tone(0.05f, 60f) { t, _ -> sin(2 * PI.toFloat() * 1500f * t) * 0.5f } },
            "wind" to {
                val n = noise(10)
                val b = lowpass(FloatArray(RATE * 4) { n() }, 0.02f)
                // seamless loop: crossfade tail into head, slow gusts
                for (i in b.indices) b[i] *= 2.2f * (0.6f + 0.4f * sin(2 * PI.toFloat() * i / b.size))
                b
            },
        )

        fun writeWav(f: File, data: FloatArray) {
            var peak = 1e-4f
            for (s in data) peak = maxOf(peak, kotlin.math.abs(s))
            val gain = if (peak > 0.95f) 0.95f / peak else 1f
            val bb = ByteBuffer.allocate(44 + data.size * 2).order(ByteOrder.LITTLE_ENDIAN)
            bb.put("RIFF".toByteArray()).putInt(36 + data.size * 2).put("WAVE".toByteArray())
            bb.put("fmt ".toByteArray()).putInt(16).putShort(1).putShort(1).putInt(RATE).putInt(RATE * 2).putShort(2).putShort(16)
            bb.put("data".toByteArray()).putInt(data.size * 2)
            for (s in data) bb.putShort((s * gain * 32767f).toInt().coerceIn(-32768, 32767).toShort())
            FileOutputStream(f).use { it.write(bb.array()) }
        }
    }
}
