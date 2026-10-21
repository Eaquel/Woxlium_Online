package com.woxlium.zombie

import android.content.Intent
import android.graphics.drawable.GradientDrawable
import android.os.Bundle
import android.view.Gravity
import android.view.View
import android.widget.FrameLayout
import android.widget.ImageView
import android.widget.LinearLayout
import android.widget.TextView
import com.woxlium.zombie.Ui.dp

/** Lobby: main menu built from the provided button artwork. */
class MainActivity : BaseActivity() {
    private lateinit var record: TextView

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        root.addView(ImageView(this).apply {
            setImageResource(R.drawable.menu_bg)
            scaleType = ImageView.ScaleType.CENTER_CROP
        })
        root.addView(View(this).apply {
            background = GradientDrawable(GradientDrawable.Orientation.LEFT_RIGHT, intArrayOf(0xF0060806.toInt(), 0x80060806.toInt(), 0xE0060806.toInt()))
        })
        // zombie looming on the right
        root.addView(ImageView(this).apply {
            setImageResource(R.drawable.portrait_zombie)
            alpha = 0.55f
            scaleType = ImageView.ScaleType.FIT_END
        }, FrameLayout.LayoutParams(dp(260f), FrameLayout.LayoutParams.MATCH_PARENT, Gravity.END or Gravity.BOTTOM))

        val content = LinearLayout(this).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
            setPadding(dp(28f), dp(16f), dp(28f), dp(16f))
        }
        val left = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            gravity = Gravity.START
            addView(Ui.label(context, "WOXLIUM", 54f, Ui.ACCENT).apply { letterSpacing = 0.12f })
            addView(Ui.label(context, "ZOMBİ YAKIN DÖVÜŞ • HAYATTA KAL", 15f, Ui.DIM))
            record = Ui.label(context, "", 13f, Ui.TEXT)
            addView(record)
            addView(menuButton(R.drawable.btn_play, 108f) { startGame(NativeBridge.MODE_STORY) },
                LinearLayout.LayoutParams(LinearLayout.LayoutParams.WRAP_CONTENT, dp(108f)).apply { topMargin = dp(12f) })
        }
        val right = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            gravity = Gravity.CENTER_HORIZONTAL
            addView(menuButton(R.drawable.btn_story, 70f) { showOverlay(Panels.story(context, { closeOverlay(); startGame(NativeBridge.MODE_STORY) }, { closeOverlay() })) })
            addView(menuButton(R.drawable.btn_endless, 52f) { startGame(NativeBridge.MODE_ENDLESS) })
            addView(menuButton(R.drawable.btn_character, 52f) { showOverlay(Panels.character(context) { closeOverlay(); refresh() }) })
            addView(menuButton(R.drawable.btn_quests, 50f) { showOverlay(Panels.quests(context) { closeOverlay() }) })
            addView(menuButton(R.drawable.btn_settings, 48f) { showOverlay(Panels.settings(context) { closeOverlay() }) })
            addView(menuButton(R.drawable.btn_exit, 48f) { finishAffinity() })
        }
        content.addView(left, LinearLayout.LayoutParams(0, LinearLayout.LayoutParams.WRAP_CONTENT, 1.1f))
        content.addView(right, LinearLayout.LayoutParams(0, LinearLayout.LayoutParams.WRAP_CONTENT, 1f))
        root.addView(content, FrameLayout.LayoutParams(FrameLayout.LayoutParams.MATCH_PARENT, FrameLayout.LayoutParams.MATCH_PARENT))
        refresh()
    }

    private fun menuButton(res: Int, heightDp: Float, onClick: () -> Unit): View =
        Ui.imageButton(this, res, onClick).apply {
            layoutParams = LinearLayout.LayoutParams(LinearLayout.LayoutParams.WRAP_CONTENT, dp(heightDp)).apply {
                topMargin = dp(4f); bottomMargin = dp(4f)
            }
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
