package com.woxlium.zombie

import android.content.Intent
import android.graphics.drawable.GradientDrawable
import android.os.Bundle
import android.view.Gravity
import android.view.View
import android.widget.FrameLayout
import android.widget.LinearLayout
import android.widget.TextView
import com.woxlium.zombie.Ui.dp

/** Lobby built from the provided button artwork; sized to fit any landscape screen. */
class MainActivity : BaseActivity() {
    private lateinit var record: TextView

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        NativeBridge.nativeInit(assets)
        root.background = GradientDrawable(GradientDrawable.Orientation.TL_BR, intArrayOf(0xFF10140F.toInt(), 0xFF050605.toInt()))

        val h = resources.displayMetrics.heightPixels / resources.displayMetrics.density
        // button height adapts so the whole menu fits on short phones
        val unit = ((h - 40f) / 7.2f).coerceIn(38f, 58f)

        val content = LinearLayout(this).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
            setPadding(dp(28f), dp(10f), dp(28f), dp(10f))
        }
        val left = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            addView(Ui.label(context, "WOXLIUM", unit * 0.9f, Ui.ACCENT).apply { letterSpacing = 0.12f })
            addView(Ui.label(context, "BİRİNCİ ŞAHIS ZOMBİ DÖVÜŞÜ", 14f, Ui.DIM))
            record = Ui.label(context, "", 13f, Ui.TEXT)
            addView(record)
            addView(menuButton(R.drawable.btn_play, unit * 1.8f) { showStory() },
                LinearLayout.LayoutParams(LinearLayout.LayoutParams.WRAP_CONTENT, dp(unit * 1.8f)).apply { topMargin = dp(10f) })
        }
        val right = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            gravity = Gravity.CENTER_HORIZONTAL
            addView(menuButton(R.drawable.btn_endless, unit) { startGame(NativeBridge.MODE_ENDLESS) })
            addView(menuButton(R.drawable.btn_quests, unit) { showOverlay(Panels.quests(context) { closeOverlay() }) })
            addView(menuButton(R.drawable.btn_settings, unit) { showOverlay(Panels.settings(context) { closeOverlay() }) })
            addView(menuButton(R.drawable.btn_exit, unit) { finishAffinity() })
        }
        content.addView(left, LinearLayout.LayoutParams(0, LinearLayout.LayoutParams.WRAP_CONTENT, 1.1f))
        content.addView(right, LinearLayout.LayoutParams(0, LinearLayout.LayoutParams.WRAP_CONTENT, 1f))
        root.addView(content, FrameLayout.LayoutParams(FrameLayout.LayoutParams.MATCH_PARENT, FrameLayout.LayoutParams.MATCH_PARENT))
        root.addView(Ui.label(this, "Modeller: Microsoft Rocketbox (MIT) • Animasyon: Quaternius UAL (CC0) • Çevre: Poly Haven (CC0)", 10f, Ui.DIM, Gravity.CENTER),
            FrameLayout.LayoutParams(FrameLayout.LayoutParams.MATCH_PARENT, FrameLayout.LayoutParams.WRAP_CONTENT, Gravity.BOTTOM).apply { bottomMargin = dp(4f) })
        refresh()
    }

    private fun menuButton(res: Int, heightDp: Float, onClick: () -> Unit): View =
        Ui.imageButton(this, res, onClick).apply {
            layoutParams = LinearLayout.LayoutParams(LinearLayout.LayoutParams.WRAP_CONTENT, dp(heightDp)).apply {
                topMargin = dp(4f); bottomMargin = dp(4f)
            }
        }

    private fun showStory() {
        showOverlay(Panels.story(this, { closeOverlay(); startGame(NativeBridge.MODE_STORY) }, { closeOverlay() }))
    }

    private fun refresh() {
        val p = Game.prefs
        record.text = "Rekor: ${p.bestScore} puan  •  En iyi dalga: ${p.bestWave}  •  Görev: ${p.quests().count { it.done }}/${p.quests().size}"
    }

    override fun onResume() {
        super.onResume()
        refresh()
    }

    private fun startGame(mode: Int) {
        startActivity(Intent(this, GameActivity::class.java).putExtra(GameActivity.EXTRA_MODE, mode))
        @Suppress("DEPRECATION")
        overridePendingTransition(android.R.anim.fade_in, android.R.anim.fade_out)
    }

    override fun onBack() {
        if (!closeOverlay()) finish()
    }
}
