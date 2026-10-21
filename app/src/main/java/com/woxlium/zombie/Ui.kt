package com.woxlium.zombie

import android.annotation.SuppressLint
import android.app.Activity
import android.content.Context
import android.graphics.Color
import android.graphics.Typeface
import android.graphics.drawable.GradientDrawable
import android.os.Build
import android.view.Gravity
import android.view.MotionEvent
import android.view.View
import android.view.WindowInsets
import android.view.WindowInsetsController
import android.view.WindowManager
import android.widget.ImageView
import android.widget.LinearLayout
import android.widget.TextView

/** Shared look & feel: fonts, colours, panels and press-to-grow buttons. */
object Ui {
    val font: Typeface = Typeface.create("sans-serif-condensed", Typeface.BOLD)
    const val ACCENT = 0xFFB4F05A.toInt()
    const val TEXT = 0xFFE8EDE0.toInt()
    const val DIM = 0xFF9AA392.toInt()
    const val BLOOD = 0xFFD23A2A.toInt()
    const val PANEL = 0xE6141A14.toInt()

    fun Context.dp(v: Float): Int = (v * resources.displayMetrics.density + 0.5f).toInt()
    fun Context.dpf(v: Float): Float = v * resources.displayMetrics.density

    fun label(ctx: Context, text: String, sp: Float, color: Int = TEXT, gravity: Int = Gravity.START): TextView =
        TextView(ctx).apply {
            this.text = text
            textSize = sp
            setTextColor(color)
            typeface = font
            this.gravity = gravity
            setShadowLayer(ctx.dpf(3f), 0f, ctx.dpf(1.5f), Color.BLACK)
            letterSpacing = 0.04f
        }

    fun panelBackground(ctx: Context, stroke: Int = 0xFF56664A.toInt()) = GradientDrawable().apply {
        setColor(PANEL)
        cornerRadius = ctx.dpf(10f)
        setStroke(ctx.dp(2f), stroke)
    }

    fun panel(ctx: Context): LinearLayout = LinearLayout(ctx).apply {
        orientation = LinearLayout.VERTICAL
        background = panelBackground(ctx)
        val p = ctx.dp(18f)
        setPadding(p, p, p, p)
        gravity = Gravity.CENTER_HORIZONTAL
    }

    fun textButton(ctx: Context, text: String, onClick: () -> Unit): TextView = label(ctx, text, 18f, TEXT, Gravity.CENTER).apply {
        background = GradientDrawable().apply {
            setColor(0xFF2A3326.toInt())
            cornerRadius = ctx.dpf(8f)
            setStroke(ctx.dp(2f), 0xFF7E9460.toInt())
        }
        val p = ctx.dp(10f)
        setPadding(p * 2, p, p * 2, p)
        pressGrow(this, onClick)
    }

    fun imageButton(ctx: Context, res: Int, onClick: () -> Unit): ImageView = ImageView(ctx).apply {
        setImageResource(res)
        scaleType = ImageView.ScaleType.FIT_CENTER
        adjustViewBounds = true
        pressGrow(this, onClick)
    }

    /** Buttons grow while pressed and fire on release inside the view. */
    @SuppressLint("ClickableViewAccessibility")
    fun pressGrow(v: View, onClick: (() -> Unit)?, grow: Float = 1.12f) {
        v.isClickable = true
        v.setOnTouchListener { view, e ->
            when (e.actionMasked) {
                MotionEvent.ACTION_DOWN -> view.animate().scaleX(grow).scaleY(grow).setDuration(70).start()
                MotionEvent.ACTION_UP -> {
                    view.animate().scaleX(1f).scaleY(1f).setDuration(110).start()
                    if (e.x >= 0 && e.y >= 0 && e.x <= view.width && e.y <= view.height) {
                        Game.sfx?.click()
                        onClick?.invoke()
                    }
                }
                MotionEvent.ACTION_CANCEL -> view.animate().scaleX(1f).scaleY(1f).setDuration(110).start()
            }
            true
        }
    }

    fun immersive(a: Activity) {
        a.window.addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
        if (Build.VERSION.SDK_INT >= 28) {
            a.window.attributes = a.window.attributes.apply {
                layoutInDisplayCutoutMode = WindowManager.LayoutParams.LAYOUT_IN_DISPLAY_CUTOUT_MODE_SHORT_EDGES
            }
        }
        if (Build.VERSION.SDK_INT >= 30) {
            a.window.setDecorFitsSystemWindows(false)
            a.window.insetsController?.let {
                it.hide(WindowInsets.Type.systemBars())
                it.systemBarsBehavior = WindowInsetsController.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE
            }
        } else {
            @Suppress("DEPRECATION")
            a.window.decorView.systemUiVisibility = (View.SYSTEM_UI_FLAG_IMMERSIVE_STICKY
                or View.SYSTEM_UI_FLAG_FULLSCREEN
                or View.SYSTEM_UI_FLAG_HIDE_NAVIGATION
                or View.SYSTEM_UI_FLAG_LAYOUT_STABLE
                or View.SYSTEM_UI_FLAG_LAYOUT_HIDE_NAVIGATION
                or View.SYSTEM_UI_FLAG_LAYOUT_FULLSCREEN)
        }
    }
}

/** Process-wide singletons shared by the activities. */
object Game {
    var sfx: Sfx? = null
    lateinit var prefs: Prefs

    fun init(ctx: Context) {
        if (!::prefs.isInitialized) prefs = Prefs(ctx.applicationContext)
        if (sfx == null) sfx = Sfx(ctx.applicationContext, prefs)
    }
}
