# Woxlium — 3D Zombi Yakın Dövüş (MVP)

Rust (oyun çekirdeği + OpenGL ES 2.0 renderer) ve Kotlin (Android arayüzü, ses, dokunmatik kontroller) ile yazılmış 3D zombi hayatta kalma oyunu.

* **Android 7.0 (API 24) → Android 17 (API 37)**, `minSdk 24`, `targetSdk 36`
* **32 + 64 bit**: `armeabi-v7a`, `arm64-v8a` (+ emülatör/Chromebook için `x86_64`)
* 16 KB sayfa hizalı native kütüphane (Android 15+ / Play şartı)
* Tek APK ≈ **2.6 MB** (3 ABI dahil), sıfır harici Rust bağımlılığı, AndroidX yok

## Oynanış
* **Sol yarı**: kayan joystick (parmağın değdiği yerde belirir)
* **Yumruk**: 3'lü kombo (3. vuruş güçlü / kritik + şok dalgası), otomatik hedefleme, vuruş duraksaması (hit-stop) ve ekran sarsıntısı
* **Tekme**: geniş açı, kalabalığı geri iter · **Blok** (basılı tut): hasarı %80 azaltır, kalkan efekti · **Kaçınma**: dokunulmaz atılma · **İlk yardım**: +45 can
* Dayanıklılık sistemi: azaldığında ter, bittiğinde duman efekti
* Zombiler: Yürüyen, Koşucu, Atlayan (uzaktan sıçrar), Şişko (ölünce zehirli patlar) — duvarların etrafından akış alanı (flow-field) ile yol bulur
* **Hikaye modu**: 5 dalga + tahliye noktasına koşu · **Sonsuz dalga**: rekor için
* **Sağ üst mini harita** (yollar, duvarlar, zombiler, ilk yardım, tahliye); **dokununca büyük haritaya büyür**, tekrar dokununca kapanır
* Tüm butonlar basıldıkça büyür; titreşim ve prosedürel üretilmiş ses efektleri
* Lobi: OYNA, Hikaye Modu, Sonsuz Dalga, Karakter & Envanter (2 kıyafet), Görevler (8 başarım), Ayarlar (ses, titreşim, grafik kalitesi = render ölçeği, 30/60 FPS, FPS göstergesi, kamera yakınlığı), Çıkış
* Seviye kasma yok: tüm yetenekler baştan açık

## Proje yapısı
```
art/source/                 verilen konsept sayfaları (kaynak)
tools/build_assets.py       yeşil perde temizleme, kare kesme, atlas paketleme, UI kesimi
app/src/main/assets/        üretilen sprites.webp (2048² atlas) + 256² zemin/duvar dokuları
app/src/main/res/drawable-nodpi/  üretilen butonlar ve ikonlar
rust/woxlium_core/          Rust çekirdeği (game.rs, world.rs, render.rs, engine.rs, jni.rs)
rust/build_android.sh       tüm ABI'ler için çapraz derleme (Gradle preBuild'den çağrılır)
app/src/main/java/...       Kotlin: lobi, oyun ekranı, HUD, ses, ayarlar
```

## Derleme
Gereksinimler: JDK 17+, Android SDK (platform 36, NDK 28.2.13676358), Rust + hedefler:
```bash
rustup target add aarch64-linux-android armv7-linux-androideabi x86_64-linux-android
./gradlew assembleRelease        # app/build/outputs/apk/release/app-release.apk
```
Release APK şimdilik debug anahtarıyla imzalanır — yayınlamadan önce `app/build.gradle.kts` içinde kendi keystore'unuzu tanımlayın.

Assetleri yeniden üretmek için (Pillow, numpy, scipy): `python3 tools/build_assets.py`

## Test
```bash
cd rust/woxlium_core && cargo test --release     # oyun mantığı / dünya / matematik testleri
```
Masaüstü görsel doğrulama (ANGLE/SwiftShader ile ekran dışı render, senaryolu oynanış + ekran görüntüleri):
```bash
A=/path/to/chromium   # libEGL.so, libGLESv2.so, vk_swiftshader_icd.json içeren dizin
RUSTFLAGS="-L $A" cargo build --release --features preview --bin preview
VK_ICD_FILENAMES=$A/vk_swiftshader_icd.json LD_LIBRARY_PATH=$A ./target/release/preview out ../../build/preview_assets
```
