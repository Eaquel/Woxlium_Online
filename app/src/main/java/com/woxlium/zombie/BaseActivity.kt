package com.woxlium.zombie

import android.annotation.SuppressLint
import android.app.Activity
import android.os.Build
import android.os.Bundle
import android.view.View
import android.widget.FrameLayout
import android.window.OnBackInvokedDispatcher

/** Fullscreen activity with back handling that works from API 24 to 37. */
abstract class BaseActivity : Activity() {
    protected lateinit var root: FrameLayout
    private val overlays = ArrayList<View>()

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        Game.init(this)
        root = FrameLayout(this)
        setContentView(root)
        if (Build.VERSION.SDK_INT >= 33) {
            onBackInvokedDispatcher.registerOnBackInvokedCallback(OnBackInvokedDispatcher.PRIORITY_DEFAULT) { onBack() }
        }
    }

    override fun onWindowFocusChanged(hasFocus: Boolean) {
        super.onWindowFocusChanged(hasFocus)
        if (hasFocus) Ui.immersive(this)
    }

    override fun onResume() {
        super.onResume()
        Ui.immersive(this)
    }

    // API < 33 only; API 33+ is routed through OnBackInvokedDispatcher above
    @SuppressLint("GestureBackNavigation")
    @Deprecated("Pre-API-33 back handling")
    override fun onBackPressed() {
        onBack()
    }

    /** Closes the top overlay; returns false when none was open. */
    protected fun closeOverlay(): Boolean {
        val top = overlays.removeLastOrNull() ?: return false
        root.removeView(top)
        return true
    }

    protected fun showOverlay(content: View, dim: Boolean = true): View {
        val shade = FrameLayout(this).apply {
            setBackgroundColor(if (dim) 0xAA000000.toInt() else 0)
            isClickable = true
            addView(content, FrameLayout.LayoutParams(FrameLayout.LayoutParams.WRAP_CONTENT, FrameLayout.LayoutParams.WRAP_CONTENT, android.view.Gravity.CENTER))
        }
        content.scaleX = 0.85f; content.scaleY = 0.85f; content.alpha = 0f
        content.animate().scaleX(1f).scaleY(1f).alpha(1f).setDuration(160).start()
        root.addView(shade, FrameLayout.LayoutParams(FrameLayout.LayoutParams.MATCH_PARENT, FrameLayout.LayoutParams.MATCH_PARENT))
        overlays.add(shade)
        return shade
    }

    protected val overlayOpen get() = overlays.isNotEmpty()

    protected abstract fun onBack()
}
