package com.woxlium.zombie

import android.annotation.SuppressLint
import android.opengl.GLSurfaceView
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.os.SystemClock
import android.view.Gravity
import android.view.MotionEvent
import android.view.View
import android.widget.FrameLayout
import android.widget.ImageView
import android.widget.LinearLayout
import android.widget.TextView
import com.woxlium.zombie.Ui.dp
import javax.microedition.khronos.egl.EGLConfig
import javax.microedition.khronos.opengles.GL10
import kotlin.random.Random

/** First-person game screen: GLES3 view driven by the Rust core + touch HUD. */
class GameActivity : BaseActivity() {
    companion object {
        const val EXTRA_MODE = "mode"
    }

    private lateinit var gl: GLSurfaceView
    private lateinit var hpBar: BarView
    private lateinit var staminaBar: BarView
    private lateinit var waveText: TextView
    private lateinit var objective: TextView
    private lateinit var scoreText: TextView
    private lateinit var comboText: TextView
    private lateinit var banner: TextView
    private lateinit var fpsText: TextView
    private lateinit var minimap: MinimapTouch
    private lateinit var hud: FrameLayout
    private var mapCloser: View? = null

    private val ui = Handler(Looper.getMainLooper())
    private var mode = NativeBridge.MODE_STORY
    private var lastWave = -1
    private var lastCombo = 0
    private var extractionShown = false
    private var ended = false
    private var paused = false
    private var loading: View? = null
    @Volatile private var renderScale = 1f
    @Volatile private var fps60 = true

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        NativeBridge.nativeInit(assets)
        mode = intent.getIntExtra(EXTRA_MODE, NativeBridge.MODE_STORY)
        val p = Game.prefs
        renderScale = p.renderScale
        fps60 = p.fps60
        NativeBridge.nativeSetShadows(p.shadows)
        NativeBridge.nativeStart(mode, Random.nextInt())

        gl = GLSurfaceView(this).apply {
            setEGLContextClientVersion(3)
            setEGLConfigChooser(8, 8, 8, 8, 24, 0)
            preserveEGLContextOnPause = true
            setRenderer(Renderer())
            renderMode = GLSurfaceView.RENDERMODE_CONTINUOUSLY
        }
        root.addView(gl)
        gl.addOnLayoutChangeListener { _, l, t, r, b, ol, ot, or_, ob ->
            if (r - l != or_ - ol || b - t != ob - ot) applyRenderScale()
        }
        buildHud()
        loading = Ui.label(this, "YÜKLENİYOR…", 26f, Ui.ACCENT, Gravity.CENTER).apply { setBackgroundColor(0xFF000000.toInt()) }
        root.addView(loading, FrameLayout.LayoutParams(FrameLayout.LayoutParams.MATCH_PARENT, FrameLayout.LayoutParams.MATCH_PARENT))
        ui.post(hudTick)
    }

    private fun applyRenderScale() {
        val w = gl.width
        val h = gl.height
        if (w <= 0 || h <= 0) return
        if (renderScale < 0.99f) gl.holder.setFixedSize((w * renderScale).toInt(), (h * renderScale).toInt())
        else gl.holder.setSizeFromLayout()
        pushMinimapRect()
    }

    private fun pushMinimapRect() {
        if (!::minimap.isInitialized || minimap.width == 0) return
        val loc = IntArray(2)
        val glLoc = IntArray(2)
        minimap.getLocationInWindow(loc)
        gl.getLocationInWindow(glLoc)
        val s = renderScale
        NativeBridge.nativeSetMinimap((loc[0] - glLoc[0]) * s, (loc[1] - glLoc[1]) * s, minimap.width * s)
    }

    // --------------------------------------------------------------- HUD
    private fun buildHud() {
        hud = FrameLayout(this)
        root.addView(hud, FrameLayout.LayoutParams(FrameLayout.LayoutParams.MATCH_PARENT, FrameLayout.LayoutParams.MATCH_PARENT))
        val w = resources.displayMetrics.widthPixels

        // right side: look around (under the buttons); left side: walk stick
        val sens = Game.prefs.sensitivity / 100f * 0.0042f
        hud.addView(LookPad(this) { dx, dy -> NativeBridge.nativeLook(dx * sens, dy * sens) },
            FrameLayout.LayoutParams(w * 58 / 100, FrameLayout.LayoutParams.MATCH_PARENT, Gravity.END))
        hud.addView(JoystickView(this) { x, y -> NativeBridge.nativeMove(x, y) },
            FrameLayout.LayoutParams(w * 42 / 100, FrameLayout.LayoutParams.MATCH_PARENT, Gravity.START))

        val m = dp(14f)
        val status = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            hpBar = BarView(context, 0xFFD23A2A.toInt())
            staminaBar = BarView(context, 0xFFE8C547.toInt())
            addView(Ui.label(context, "SAĞLIK", 11f, Ui.DIM))
            addView(hpBar, LinearLayout.LayoutParams(dp(210f), dp(15f)))
            addView(Ui.label(context, "DAYANIKLILIK", 11f, Ui.DIM))
            addView(staminaBar, LinearLayout.LayoutParams(dp(170f), dp(10f)))
            scoreText = Ui.label(context, "", 14f)
            addView(scoreText)
        }
        hud.addView(status, FrameLayout.LayoutParams(FrameLayout.LayoutParams.WRAP_CONTENT, FrameLayout.LayoutParams.WRAP_CONTENT, Gravity.TOP or Gravity.START).apply { setMargins(m, m, 0, 0) })

        val center = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            gravity = Gravity.CENTER_HORIZONTAL
            waveText = Ui.label(context, "", 17f, Ui.TEXT, Gravity.CENTER)
            objective = Ui.label(context, "", 13f, Ui.ACCENT, Gravity.CENTER)
            addView(waveText)
            addView(objective)
        }
        hud.addView(center, FrameLayout.LayoutParams(FrameLayout.LayoutParams.WRAP_CONTENT, FrameLayout.LayoutParams.WRAP_CONTENT, Gravity.TOP or Gravity.CENTER_HORIZONTAL).apply { topMargin = m })

        comboText = Ui.label(this, "", 28f, Ui.ACCENT, Gravity.CENTER).apply { alpha = 0f }
        hud.addView(comboText, FrameLayout.LayoutParams(FrameLayout.LayoutParams.WRAP_CONTENT, FrameLayout.LayoutParams.WRAP_CONTENT, Gravity.CENTER).apply { topMargin = -dp(90f) })

        banner = Ui.label(this, "", 44f, Ui.TEXT, Gravity.CENTER).apply { alpha = 0f; letterSpacing = 0.15f }
        hud.addView(banner, FrameLayout.LayoutParams(FrameLayout.LayoutParams.WRAP_CONTENT, FrameLayout.LayoutParams.WRAP_CONTENT, Gravity.CENTER).apply { bottomMargin = dp(120f) })

        fpsText = Ui.label(this, "", 11f, Ui.DIM)
        hud.addView(fpsText, FrameLayout.LayoutParams(FrameLayout.LayoutParams.WRAP_CONTENT, FrameLayout.LayoutParams.WRAP_CONTENT, Gravity.BOTTOM or Gravity.CENTER_HORIZONTAL).apply { bottomMargin = dp(4f) })

        // minimap (drawn by Rust) + pause
        minimap = MinimapTouch(this)
        Ui.pressGrow(minimap, { toggleMap() }, 1.04f)
        hud.addView(minimap, FrameLayout.LayoutParams(dp(124f), dp(124f), Gravity.TOP or Gravity.END).apply { setMargins(0, m, m, 0) })
        minimap.addOnLayoutChangeListener { _, _, _, _, _, _, _, _, _ -> pushMinimapRect() }
        hud.addView(Ui.imageButton(this, R.drawable.ic_pause) { pauseGame() },
            FrameLayout.LayoutParams(dp(44f), dp(44f), Gravity.TOP or Gravity.END).apply { setMargins(0, m, m + dp(134f), 0) })

        // action buttons (bottom-right, over the look pad)
        val cluster = FrameLayout(this)
        fun add(view: View, size: Int, right: Int, bottom: Int) =
            cluster.addView(view, FrameLayout.LayoutParams(size, size, Gravity.BOTTOM or Gravity.END).apply { setMargins(0, 0, right, bottom) })
        add(actionButton(R.drawable.ic_punch, NativeBridge.BTN_ATTACK), dp(98f), dp(20f), dp(24f))
        add(actionButton(R.drawable.ic_block, NativeBridge.BTN_BLOCK), dp(68f), dp(128f), dp(18f))
        add(actionButton(R.drawable.ic_dodge, NativeBridge.BTN_DODGE), dp(64f), dp(34f), dp(134f))
        hud.addView(cluster, FrameLayout.LayoutParams(dp(220f), dp(220f), Gravity.BOTTOM or Gravity.END))
    }

    /** HUD action: grows while held, sends down/up immediately. */
    @SuppressLint("ClickableViewAccessibility")
    private fun actionButton(res: Int, id: Int): View = ImageView(this).apply {
        setImageResource(res)
        scaleType = ImageView.ScaleType.FIT_CENTER
        alpha = 0.9f
        setOnTouchListener { v, e ->
            when (e.actionMasked) {
                MotionEvent.ACTION_DOWN -> {
                    v.animate().scaleX(1.18f).scaleY(1.18f).setDuration(50).start()
                    v.alpha = 1f
                    NativeBridge.nativeButton(id, true)
                }
                MotionEvent.ACTION_UP, MotionEvent.ACTION_CANCEL -> {
                    v.animate().scaleX(1f).scaleY(1f).setDuration(90).start()
                    v.alpha = 0.9f
                    NativeBridge.nativeButton(id, false)
                }
            }
            true
        }
    }

    private fun toggleMap() {
        val expanded = mapCloser == null
        NativeBridge.nativeSetMapExpanded(expanded)
        if (expanded) {
            mapCloser = View(this).apply {
                isClickable = true
                setOnClickListener { toggleMap() }
            }
            hud.addView(mapCloser, FrameLayout.LayoutParams(FrameLayout.LayoutParams.MATCH_PARENT, FrameLayout.LayoutParams.MATCH_PARENT))
        } else {
            hud.removeView(mapCloser)
            mapCloser = null
        }
    }

    private fun flashBanner(text: String, color: Int) {
        banner.text = text
        banner.setTextColor(color)
        banner.animate().cancel()
        banner.alpha = 0f
        banner.scaleX = 1.4f; banner.scaleY = 1.4f
        banner.animate().alpha(1f).scaleX(1f).scaleY(1f).setDuration(250).withEndAction {
            banner.animate().alpha(0f).setStartDelay(1300).setDuration(500).start()
        }.start()
    }

    private val hudTick = object : Runnable {
        @SuppressLint("SetTextI18n")
        override fun run() {
            val h = { i: Int -> NativeBridge.nativeHud(i) }
            val hpMax = h(NativeBridge.HUD_HP_MAX)
            if (hpMax > 0f) {
                loading?.let { root.removeView(it); loading = null; Game.sfx?.startAmbience() }
                hpBar.set(h(NativeBridge.HUD_HP) / hpMax)
                staminaBar.set(h(NativeBridge.HUD_STAMINA) / h(NativeBridge.HUD_STAMINA_MAX))
                val wave = h(NativeBridge.HUD_WAVE).toInt()
                val state = h(NativeBridge.HUD_STATE).toInt()
                val story = h(NativeBridge.HUD_MODE) > 0.5f
                scoreText.text = "SKOR ${h(NativeBridge.HUD_SCORE).toInt()}   •   LEŞ ${h(NativeBridge.HUD_KILLS).toInt()}"
                waveText.text = when {
                    wave == 0 -> "HAZIRLAN"
                    story -> "DALGA $wave/5   •   ${h(NativeBridge.HUD_ENEMIES).toInt()} ZOMBİ"
                    else -> "DALGA $wave   •   ${h(NativeBridge.HUD_ENEMIES).toInt()} ZOMBİ"
                }
                val extraction = h(NativeBridge.HUD_OBJECTIVE) > 0.5f
                objective.text = when {
                    extraction -> "► KIRMIZI FİŞEĞE KOŞ (${h(NativeBridge.HUD_EXTRACT_DIST).toInt()} m)"
                    state == 1 -> "Sonraki dalga: ${h(NativeBridge.HUD_BREAK).toInt() + 1} sn"
                    else -> ""
                }
                if (wave != lastWave && wave > 0) {
                    lastWave = wave
                    flashBanner(if (story && wave == 5) "SON DALGA" else "DALGA $wave", Ui.BLOOD)
                }
                if (extraction && !extractionShown) {
                    extractionShown = true
                    flashBanner("TAHLİYE!", Ui.ACCENT)
                }
                val combo = h(NativeBridge.HUD_COMBO).toInt()
                if (combo >= 2 && combo != lastCombo) {
                    comboText.text = "${combo}x KOMBO"
                    comboText.animate().cancel()
                    comboText.alpha = 1f
                    comboText.scaleX = 1.35f; comboText.scaleY = 1.35f
                    comboText.animate().scaleX(1f).scaleY(1f).setDuration(120).start()
                } else if (combo < 2 && lastCombo >= 2) {
                    comboText.animate().alpha(0f).setDuration(300).start()
                }
                lastCombo = combo
                fpsText.text = if (Game.prefs.showFps) "${h(NativeBridge.HUD_FPS).toInt()} FPS" else ""
                if (!ended && (state == 2 || state == 3)) endGame(state == 3)
            }
            ui.postDelayed(this, 100)
        }
    }

    // --------------------------------------------------------------- flow
    private fun endGame(victory: Boolean) {
        ended = true
        if (mapCloser != null) toggleMap()
        val h = { i: Int -> NativeBridge.nativeHud(i) }
        val kills = h(NativeBridge.HUD_KILLS).toInt()
        val score = h(NativeBridge.HUD_SCORE).toInt()
        val wave = h(NativeBridge.HUD_WAVE).toInt()
        val combo = h(NativeBridge.HUD_MAX_COMBO).toInt()
        val best = Game.prefs.bestScore
        Game.prefs.recordGame(kills, wave, score, combo, victory && mode == NativeBridge.MODE_STORY, mode == NativeBridge.MODE_ENDLESS)
        val secs = h(NativeBridge.HUD_TIME).toInt()
        val panel = Ui.panel(this).apply {
            addView(Ui.label(context, if (victory) "TAHLİYE EDİLDİN!" else "ÖLDÜN", 34f, if (victory) Ui.ACCENT else Ui.BLOOD, Gravity.CENTER))
            addView(Ui.label(context,
                "Dalga $wave   •   $kills leş   •   En iyi kombo ${combo}x\nSüre ${secs / 60}:${"%02d".format(secs % 60)}   •   Skor $score" +
                    if (score > best) "\nYENİ REKOR!" else "",
                16f, Ui.TEXT, Gravity.CENTER))
            addView(LinearLayout(context).apply {
                orientation = LinearLayout.HORIZONTAL
                gravity = Gravity.CENTER
                setPadding(0, dp(12f), 0, 0)
                val s = dp(62f)
                addView(Ui.imageButton(context, R.drawable.ic_restart) { restart() }, LinearLayout.LayoutParams(s, s).apply { marginEnd = dp(18f) })
                addView(Ui.imageButton(context, R.drawable.ic_lobby) { finish() }, LinearLayout.LayoutParams(s, s))
            })
        }
        showOverlay(panel)
    }

    private fun restart() {
        while (closeOverlay()) { /* drop all panels */ }
        ended = false
        lastWave = -1
        extractionShown = false
        paused = false
        NativeBridge.nativePause(false)
        NativeBridge.nativeStart(mode, Random.nextInt())
        Game.sfx?.startAmbience()
    }

    private fun pauseGame() {
        if (paused || ended) return
        paused = true
        NativeBridge.nativePause(true)
        NativeBridge.nativeMove(0f, 0f)
        NativeBridge.nativeButton(NativeBridge.BTN_BLOCK, false)
        Game.sfx?.stopAmbience()
        val panel = Ui.panel(this).apply {
            addView(Ui.label(context, "DURAKLATILDI", 30f, Ui.ACCENT, Gravity.CENTER))
            addView(LinearLayout(context).apply {
                orientation = LinearLayout.HORIZONTAL
                gravity = Gravity.CENTER
                setPadding(0, dp(12f), 0, 0)
                val s = dp(62f)
                val lp = { LinearLayout.LayoutParams(s, s).apply { marginEnd = dp(16f) } }
                addView(Ui.imageButton(context, R.drawable.ic_resume) { resumeGame() }, lp())
                addView(Ui.imageButton(context, R.drawable.ic_restart) { restart() }, lp())
                addView(Ui.imageButton(context, R.drawable.ic_gear) { showOverlay(Panels.settings(context) { closeOverlay() }) }, lp())
                addView(Ui.imageButton(context, R.drawable.ic_lobby) { finish() }, LinearLayout.LayoutParams(s, s))
            })
            addView(Ui.label(context, "Devam • Yeniden • Ayarlar • Lobi", 12f, Ui.DIM, Gravity.CENTER))
        }
        showOverlay(panel)
    }

    private fun resumeGame() {
        while (closeOverlay()) { /* close pause + settings */ }
        paused = false
        NativeBridge.nativePause(false)
        Game.sfx?.startAmbience()
    }

    override fun onBack() {
        when {
            mapCloser != null -> toggleMap()
            ended -> finish()
            paused && overlayOpen -> if (!closeOverlay() || !overlayOpen) resumeGame()
            else -> pauseGame()
        }
    }

    override fun onPause() {
        super.onPause()
        pauseGame()
        gl.onPause()
    }

    override fun onResume() {
        super.onResume()
        gl.onResume()
    }

    override fun onDestroy() {
        ui.removeCallbacksAndMessages(null)
        Game.sfx?.stopAmbience()
        super.onDestroy()
    }

    // --------------------------------------------------------------- GL thread
    private inner class Renderer : GLSurfaceView.Renderer {
        private var last = 0L

        override fun onSurfaceCreated(unused: GL10?, config: EGLConfig?) {
            if (!NativeBridge.nativeSurfaceCreated()) {
                ui.post { (loading as? TextView)?.text = "Bu cihaz OpenGL ES 3.0 gerektirir" }
            }
            last = System.nanoTime()
        }

        override fun onSurfaceChanged(unused: GL10?, width: Int, height: Int) {
            NativeBridge.nativeSurfaceChanged(width, height)
            ui.post { pushMinimapRect() }
        }

        override fun onDrawFrame(unused: GL10?) {
            if (!fps60) {
                val spent = (System.nanoTime() - last) / 1_000_000L
                if (spent < 32) SystemClock.sleep(32 - spent)
            }
            val now = System.nanoTime()
            val dt = ((now - last) / 1e9f).coerceIn(0f, 0.1f)
            last = now
            val ev = NativeBridge.nativeFrame(dt)
            if (ev != 0) Game.sfx?.handle(ev)
        }
    }
}
