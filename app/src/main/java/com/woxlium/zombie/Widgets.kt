package com.woxlium.zombie

import android.annotation.SuppressLint
import android.content.Context
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.Paint
import android.graphics.RectF
import android.view.MotionEvent
import android.view.View
import kotlin.math.hypot
import kotlin.math.min

/** Floating virtual joystick: appears wherever the thumb lands. */
class JoystickView(ctx: Context, private val onMove: (Float, Float) -> Unit) : View(ctx) {
    private val base = Paint(Paint.ANTI_ALIAS_FLAG).apply { color = 0x55101410; style = Paint.Style.FILL }
    private val ring = Paint(Paint.ANTI_ALIAS_FLAG).apply { color = 0x99B4F05A.toInt(); style = Paint.Style.STROKE; strokeWidth = 5f }
    private val knob = Paint(Paint.ANTI_ALIAS_FLAG).apply { color = 0xCCD8E8C8.toInt() }
    private var pointer = -1
    private var cx = 0f
    private var cy = 0f
    private var kx = 0f
    private var ky = 0f
    private val radius get() = resources.displayMetrics.density * 62f

    @SuppressLint("ClickableViewAccessibility")
    override fun onTouchEvent(e: MotionEvent): Boolean {
        when (e.actionMasked) {
            MotionEvent.ACTION_DOWN, MotionEvent.ACTION_POINTER_DOWN -> {
                if (pointer == -1) {
                    val i = e.actionIndex
                    pointer = e.getPointerId(i)
                    cx = e.getX(i); cy = e.getY(i); kx = cx; ky = cy
                    invalidate()
                }
            }
            MotionEvent.ACTION_MOVE -> {
                val i = e.findPointerIndex(pointer)
                if (i >= 0) {
                    var dx = e.getX(i) - cx
                    var dy = e.getY(i) - cy
                    val d = hypot(dx, dy)
                    val r = radius
                    if (d > r) {
                        // drag the base along so direction changes stay instant
                        cx += dx * (1 - r / d); cy += dy * (1 - r / d)
                        dx = e.getX(i) - cx; dy = e.getY(i) - cy
                    }
                    kx = cx + dx; ky = cy + dy
                    onMove(dx / r, dy / r)
                    invalidate()
                }
            }
            MotionEvent.ACTION_UP, MotionEvent.ACTION_POINTER_UP, MotionEvent.ACTION_CANCEL -> {
                if (e.actionMasked == MotionEvent.ACTION_CANCEL || e.getPointerId(e.actionIndex) == pointer) {
                    pointer = -1
                    onMove(0f, 0f)
                    invalidate()
                }
            }
        }
        return true
    }

    override fun onDraw(c: Canvas) {
        val r = radius
        if (pointer == -1) {
            // resting hint in the lower-left corner
            val hx = r * 1.6f
            val hy = height - r * 1.6f
            ring.alpha = 90
            c.drawCircle(hx, hy, r, base)
            c.drawCircle(hx, hy, r, ring)
            c.drawCircle(hx, hy, r * 0.42f, knob.apply { alpha = 90 })
            return
        }
        ring.alpha = 200
        c.drawCircle(cx, cy, r, base)
        c.drawCircle(cx, cy, r, ring)
        c.drawCircle(kx, ky, r * 0.42f, knob.apply { alpha = 220 })
    }
}

/** Rounded status bar (health / stamina) with a trailing "ghost" segment. */
class BarView(ctx: Context, private val fillColor: Int) : View(ctx) {
    private val bg = Paint(Paint.ANTI_ALIAS_FLAG).apply { color = 0xB0000000.toInt() }
    private val stroke = Paint(Paint.ANTI_ALIAS_FLAG).apply { color = 0xFF56664A.toInt(); style = Paint.Style.STROKE; strokeWidth = 3f }
    private val fill = Paint(Paint.ANTI_ALIAS_FLAG).apply { color = fillColor }
    private val ghost = Paint(Paint.ANTI_ALIAS_FLAG).apply { color = Color.WHITE; alpha = 120 }
    private val rect = RectF()
    private var value = 1f
    private var trail = 1f

    fun set(v: Float) {
        val nv = v.coerceIn(0f, 1f)
        if (nv != value) {
            if (nv > value) trail = nv
            value = nv
            invalidate()
        }
    }

    override fun onDraw(c: Canvas) {
        val r = height / 2f
        rect.set(0f, 0f, width.toFloat(), height.toFloat())
        c.drawRoundRect(rect, r, r, bg)
        if (trail > value) {
            trail = maxOf(value, trail - 0.015f)
            rect.set(0f, 0f, width * trail, height.toFloat())
            c.drawRoundRect(rect, r, r, ghost)
            postInvalidateOnAnimation()
        }
        rect.set(0f, 0f, width * value, height.toFloat())
        if (value > 0f) c.drawRoundRect(rect, r, r, fill)
        rect.set(1.5f, 1.5f, width - 1.5f, height - 1.5f)
        c.drawRoundRect(rect, r, r, stroke)
    }
}

/** Square hit area over the GL-drawn minimap; draws nothing itself. */
class MinimapTouch(ctx: Context) : View(ctx) {
    override fun onMeasure(w: Int, h: Int) {
        val s = min(MeasureSpec.getSize(w), MeasureSpec.getSize(h))
        setMeasuredDimension(s, s)
    }
}

/** Drag anywhere on it to look around (first-person camera). */
class LookPad(ctx: Context, private val onLook: (Float, Float) -> Unit) : View(ctx) {
    private var pointer = -1
    private var lx = 0f
    private var ly = 0f

    @SuppressLint("ClickableViewAccessibility")
    override fun onTouchEvent(e: MotionEvent): Boolean {
        when (e.actionMasked) {
            MotionEvent.ACTION_DOWN, MotionEvent.ACTION_POINTER_DOWN -> if (pointer == -1) {
                val i = e.actionIndex
                pointer = e.getPointerId(i)
                lx = e.getX(i); ly = e.getY(i)
            }
            MotionEvent.ACTION_MOVE -> {
                val i = e.findPointerIndex(pointer)
                if (i >= 0) {
                    onLook(e.getX(i) - lx, e.getY(i) - ly)
                    lx = e.getX(i); ly = e.getY(i)
                }
            }
            MotionEvent.ACTION_UP, MotionEvent.ACTION_POINTER_UP, MotionEvent.ACTION_CANCEL -> {
                if (e.actionMasked == MotionEvent.ACTION_CANCEL || e.getPointerId(e.actionIndex) == pointer) pointer = -1
            }
        }
        return true
    }
}
