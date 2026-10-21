# Woxlium — Birinci Şahıs 3D Zombi Dövüşü

Rust (oyun çekirdeği + OpenGL ES 3.0 renderer) ve Kotlin (Android arayüzü, ses, dokunmatik kontroller) ile yazılmış, birinci şahıs, gerçek 3D yakın dövüş zombi oyunu.

* **Android 7.0 (API 24) → Android 17 (API 37)**, `minSdk 24`, `targetSdk 36`, OpenGL ES 3.0
* **32 + 64 bit**: `armeabi-v7a`, `arm64-v8a` (+ emülatör/Chromebook için `x86_64`), 16 KB sayfa hizalı
* Release APK ≈ 17 MB (3 ABI, tüm modeller/dokular dahil)

## Görsel içerik (gerçek 3D)
* **Zombiler:** Microsoft Rocketbox'ın gerçekçi insan modelleri (7–9K üçgen, normal map'li). Dokular otomatik zombileştirilir: solgun ölü ten, damarlar, morluklar, buğulu gözler, kan lekeleri.
* **Animasyonlar:** Mocap animasyonları retarget edilir. Zombilerde sarhoş/yaralı yürüme ve koşu (kollar öne uzatılmış), saldırı, göğüs ve kafa darbesi tepkisi, ölüm var. FP kollarda boks guard, jab ve cross.
* **Çevre:** Poly Haven taranmış (photoscanned) proplar: araba, beton bariyer, sokak lambası, çöp kutusu, lastik, varil, yangın musluğu. Modüler kitten prosedürel birleştirilen 4 katlı apartman cepheleri, PBR asfalt/kaldırım/çamur dokuları ve Samanyolu'lu gece gökyüzü.
* **Renderer:**
  * GPU skinning ve normal mapping
  * Ay ışığı gölge haritası (PCF), oyuncunun el feneri (spot ışık), titreyen sokak lambaları
  * Üstel sis, ACES ton eşleme, ETC2 sıkıştırılmış dokular
  * Kan, kıvılcım ve şok dalgası efektleri; ölüm yerinde büyüyen kan gölü

## Oynanış
* **Kontroller:** Sol yarı joystick ile yürünür, sağ yarı sürüklenerek etrafa bakılır.
* **Yumruk:** Sol-sağ-güçlü üçlü kombo.
  * Vuruşta hit-stop ve kamera sarsıntısı olur, yakındaki zombiye hafifçe çekilirsin.
  * Yukarı bakarak vurulan kafa darbelerinin kritik olma şansı daha yüksek.
* **Blok:** Basılı tutulur, hasarı %85 azaltır.
* **Atılma:** Kısa süre hasar almadan kaçarsın.
* **Dayanıklılık:** Her hareket dayanıklılık harcar.
* **Zombi AI:** Akış alanı (flow field) ile engellerin etrafından yol bulur, koşucu zombiler 2. dalgadan itibaren gelir.
* **Modlar:** Hikaye modunda 5 dalganın ardından kırmızı fişeğe koşup tahliye olursun. Sonsuz dalgada rekor tutulur.
* **İlk yardım çantaları:** Yerde parlar, üstüne gelince +40 can verir.
* **Mini harita:** Sağ üstte, bakış yönüne göre döner; dokununca büyük haritaya açılır.
* **Lobi:** OYNA (hikaye), Sonsuz Dalga, Görevler (8 başarım), Ayarlar ve Çıkış.
  * Ayarlar: ses, titreşim, grafik kalitesi (gölge + çözünürlük), 30/60 FPS, FPS göstergesi, bakış hassasiyeti.
  * Paneller küçük ekranda kaydırılabilir; lobi ekran yüksekliğine göre ölçeklenir.

## Proje yapısı
```
rust/woxlium_core/src/
  game.rs     FP oyun mantığı (oyuncu, zombi AI, dalgalar, efektler)  — GL'siz, test edilebilir
  level.rs    şehir kavşağı yerleşimi, çarpışma, akış alanı
  anim.rs     klip örnekleme, cross-fade, skin matrisleri
  wxm.rs      .wxm model/iskelet/animasyon yükleyici
  render.rs   GLES3 renderer
  texture.rs  ETC2 PVR yükleyici · assets.rs  APK (AAssetManager) / dizin kaynağı
  engine.rs, jni.rs
tools/blender/wxm_export.py    Rocketbox avatar + UAL/Rocketbox klip retarget → .wxm
tools/blender/props_export.py  Poly Haven propları → .wxm
tools/blender/facade_build.py  modüler kitten apartman cephesi birleştirme
tools/build_assets.py          zombileştirme, ETC2 sıkıştırma, VFX atlası, UI kesimi, assets_gen.rs
tools/fetch_assets.sh          üçüncü parti kaynak assetleri indirir (repo'ya eklenmez)
app/src/main/java/...          Kotlin: lobi, oyun ekranı, HUD, ses, ayarlar
```
Lisanslar: `THIRD_PARTY_NOTICES.md` (Rocketbox MIT, Quaternius + Poly Haven CC0).

## Derleme
```bash
rustup target add aarch64-linux-android armv7-linux-androideabi x86_64-linux-android
./gradlew assembleRelease        # app/build/outputs/apk/release/app-release.apk
```
Üretilmiş assetler (`app/src/main/assets`) repoda olduğu için APK derlemek için Blender gerekmez.
İçeriği baştan üretmek için: `tools/fetch_assets.sh && BLENDER=/path/blender ETCPAK=/path/etcpak tools/rebuild_all.sh`

Release APK şimdilik debug anahtarıyla imzalanır; yayınlamadan önce kendi keystore'unuzu tanımlayın.

## Test
```bash
cd rust/woxlium_core && cargo test --release
```
Masaüstü görsel doğrulama (ANGLE/SwiftShader ile ekran dışı GLES3 render):
```bash
A=/path/to/chromium   # libEGL.so, libGLESv2.so, vk_swiftshader_icd.json
RUSTFLAGS="-L $A" cargo build --release --features preview --bin preview
VK_ICD_FILENAMES=$A/vk_swiftshader_icd.json LD_LIBRARY_PATH=$A ./target/release/preview out ../../build/preview_assets [fight|tour|story]
```
