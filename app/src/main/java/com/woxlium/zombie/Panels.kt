package com.woxlium.zombie

import android.content.Context
import android.view.Gravity
import android.view.View
import android.widget.ImageView
import android.widget.LinearLayout
import android.widget.ScrollView
import android.widget.SeekBar
import android.widget.TextView
import com.woxlium.zombie.Ui.dp

/** Menu panels shared by the lobby and the in-game pause menu. */
object Panels {
    private fun row(ctx: Context, title: String, control: View): LinearLayout = LinearLayout(ctx).apply {
        orientation = LinearLayout.HORIZONTAL
        gravity = Gravity.CENTER_VERTICAL
        setPadding(0, ctx.dp(5f), 0, ctx.dp(5f))
        addView(Ui.label(ctx, title, 16f), LinearLayout.LayoutParams(ctx.dp(190f), LinearLayout.LayoutParams.WRAP_CONTENT))
        addView(control, LinearLayout.LayoutParams(ctx.dp(230f), LinearLayout.LayoutParams.WRAP_CONTENT))
    }

    private fun toggle(ctx: Context, options: List<String>, initial: Int, onChange: (Int) -> Unit): TextView {
        var idx = initial
        lateinit var tv: TextView
        tv = Ui.textButton(ctx, options[idx]) {
            idx = (idx + 1) % options.size
            tv.text = options[idx]
            onChange(idx)
        }
        tv.textSize = 15f
        return tv
    }

    private fun slider(ctx: Context, min: Int, max: Int, value: Int, onChange: (Int) -> Unit): SeekBar = SeekBar(ctx).apply {
        this.max = max - min
        progress = value - min
        setOnSeekBarChangeListener(object : SeekBar.OnSeekBarChangeListener {
            override fun onProgressChanged(s: SeekBar?, p: Int, fromUser: Boolean) { if (fromUser) onChange(p + min) }
            override fun onStartTrackingTouch(s: SeekBar?) {}
            override fun onStopTrackingTouch(s: SeekBar?) {}
        })
    }

    fun settings(ctx: Context, onClose: () -> Unit): View {
        val p = Game.prefs
        return Ui.panel(ctx).apply {
            addView(Ui.label(ctx, "AYARLAR", 26f, Ui.ACCENT, Gravity.CENTER))
            addView(row(ctx, "Ses efektleri", slider(ctx, 0, 100, p.sfxVolume) { p.sfxVolume = it }))
            addView(row(ctx, "Titreşim", toggle(ctx, listOf("KAPALI", "AÇIK"), if (p.vibration) 1 else 0) { p.vibration = it == 1 }))
            addView(row(ctx, "Grafik kalitesi", toggle(ctx, listOf("DÜŞÜK (hızlı)", "ORTA", "YÜKSEK"), p.quality) { p.quality = it }))
            addView(row(ctx, "Kare hızı", toggle(ctx, listOf("30 FPS (pil)", "60 FPS"), if (p.fps60) 1 else 0) { p.fps60 = it == 1 }))
            addView(row(ctx, "FPS göstergesi", toggle(ctx, listOf("KAPALI", "AÇIK"), if (p.showFps) 1 else 0) { p.showFps = it == 1 }))
            addView(row(ctx, "Kamera (yakın ↔ uzak)", slider(ctx, 75, 135, p.zoom) {
                p.zoom = it
                runCatching { NativeBridge.nativeSetZoom(it / 100f) }
            }))
            addView(Ui.label(ctx, "Grafik ayarı bir sonraki oyunda uygulanır.", 12f, Ui.DIM, Gravity.CENTER))
            addView(closeButton(ctx, onClose))
        }
    }

    fun closeButton(ctx: Context, onClose: () -> Unit): View =
        Ui.imageButton(ctx, R.drawable.ic_check, onClose).apply {
            layoutParams = LinearLayout.LayoutParams(ctx.dp(58f), ctx.dp(58f)).apply { topMargin = ctx.dp(10f); gravity = Gravity.CENTER_HORIZONTAL }
        }

    fun quests(ctx: Context, onClose: () -> Unit): View {
        val list = LinearLayout(ctx).apply { orientation = LinearLayout.VERTICAL }
        for (q in Game.prefs.quests()) {
            list.addView(LinearLayout(ctx).apply {
                orientation = LinearLayout.HORIZONTAL
                gravity = Gravity.CENTER_VERTICAL
                setPadding(0, ctx.dp(4f), 0, ctx.dp(4f))
                addView(ImageView(ctx).apply {
                    setImageResource(if (q.done) R.drawable.ic_check else R.drawable.ic_scroll)
                    alpha = if (q.done) 1f else 0.6f
                }, LinearLayout.LayoutParams(ctx.dp(40f), ctx.dp(40f)))
                addView(LinearLayout(ctx).apply {
                    orientation = LinearLayout.VERTICAL
                    setPadding(ctx.dp(12f), 0, 0, 0)
                    addView(Ui.label(ctx, q.title, 17f, if (q.done) Ui.ACCENT else Ui.TEXT))
                    addView(Ui.label(ctx, q.desc, 13f, Ui.DIM))
                }, LinearLayout.LayoutParams(ctx.dp(300f), LinearLayout.LayoutParams.WRAP_CONTENT))
                addView(Ui.label(ctx, "${minOf(q.progress, q.goal)}/${q.goal}", 16f, if (q.done) Ui.ACCENT else Ui.TEXT, Gravity.END),
                    LinearLayout.LayoutParams(ctx.dp(80f), LinearLayout.LayoutParams.WRAP_CONTENT))
            })
        }
        return Ui.panel(ctx).apply {
            addView(Ui.label(ctx, "GÖREVLER", 26f, Ui.ACCENT, Gravity.CENTER))
            addView(ScrollView(ctx).apply { addView(list) }, LinearLayout.LayoutParams(LinearLayout.LayoutParams.WRAP_CONTENT, ctx.dp(220f)))
            addView(closeButton(ctx, onClose))
        }
    }

    fun character(ctx: Context, onClose: () -> Unit): View {
        val p = Game.prefs
        val cards = ArrayList<View>()
        fun refresh() = cards.forEachIndexed { i, v -> v.background = Ui.panelBackground(ctx, if (i == p.skin) Ui.ACCENT else 0xFF3A4436.toInt()) }
        val skins = LinearLayout(ctx).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER
            for ((i, pair) in listOf(R.drawable.portrait_soldier to "ASKER CEKETİ", R.drawable.portrait_leather to "DERİ CEKET").withIndex()) {
                val card = LinearLayout(ctx).apply {
                    orientation = LinearLayout.VERTICAL
                    gravity = Gravity.CENTER
                    setPadding(ctx.dp(10f), ctx.dp(8f), ctx.dp(10f), ctx.dp(8f))
                    addView(ImageView(ctx).apply { setImageResource(pair.first) }, LinearLayout.LayoutParams(ctx.dp(90f), ctx.dp(150f)))
                    addView(Ui.label(ctx, pair.second, 14f, Ui.TEXT, Gravity.CENTER))
                }
                Ui.pressGrow(card, { p.skin = i; refresh() }, 1.06f)
                cards.add(card)
                addView(card, LinearLayout.LayoutParams(LinearLayout.LayoutParams.WRAP_CONTENT, LinearLayout.LayoutParams.WRAP_CONTENT).apply { marginEnd = ctx.dp(10f) })
            }
            addView(LinearLayout(ctx).apply {
                orientation = LinearLayout.VERTICAL
                setPadding(ctx.dp(10f), 0, 0, 0)
                addView(Ui.label(ctx, "ENVANTER", 18f, Ui.ACCENT))
                addView(Ui.label(ctx, "• Yumruk kombosu (3 vuruş)\n• Güçlü tekme (geri iter)\n• Blok kalkanı (basılı tut)\n• Kaçınma atılması\n• 2x İlk yardım çantası", 14f))
                addView(Ui.label(ctx, "\nTEHDİTLER", 18f, Ui.BLOOD))
                addView(Ui.label(ctx, "Yürüyen • Koşucu (hızlı)\nAtlayan (uzaktan sıçrar)\nŞişko (ölünce patlar!)", 14f))
            })
        }
        refresh()
        return Ui.panel(ctx).apply {
            addView(Ui.label(ctx, "KARAKTER & ENVANTER", 24f, Ui.ACCENT, Gravity.CENTER))
            addView(skins)
            addView(Ui.label(ctx, "Toplam leş: ${p.totalKills}   •   En iyi dalga: ${p.bestWave}   •   Rekor skor: ${p.bestScore}", 13f, Ui.DIM, Gravity.CENTER))
            addView(closeButton(ctx, onClose))
        }
    }

    fun story(ctx: Context, onStart: () -> Unit, onClose: () -> Unit): View = Ui.panel(ctx).apply {
        addView(Ui.label(ctx, "HİKAYE: SON TAHLİYE", 24f, Ui.ACCENT, Gravity.CENTER))
        addView(Ui.label(ctx,
            "Şehir düştü. Silahın yok, mermin yok — sadece yumrukların var.\n" +
            "Kuzeydoğudaki tahliye noktasına ulaşmadan önce 5 dalga\n" +
            "enfekte sürüsünü geri püskürt. Sonra işaretli bölgeye koş.",
            15f, Ui.TEXT, Gravity.CENTER).apply { setPadding(0, context.dp(8f), 0, context.dp(8f)) })
        addView(Ui.label(ctx, "İpucu: 3. yumruk kritik vurur • Tekme kalabalığı dağıtır • Şişkolardan uzak dur", 13f, Ui.DIM, Gravity.CENTER))
        addView(LinearLayout(ctx).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER
            setPadding(0, context.dp(12f), 0, 0)
            addView(Ui.textButton(ctx, "SAVAŞA BAŞLA", onStart))
            addView(View(ctx), LinearLayout.LayoutParams(context.dp(16f), 1))
            addView(Ui.textButton(ctx, "GERİ", onClose))
        })
    }
}
