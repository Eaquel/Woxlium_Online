#!/usr/bin/env bash
# Downloads the third-party source assets (not committed) into $DL (default build/dl).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DL="${DL:-$ROOT/build/dl}"
mkdir -p "$DL/ph" "$DL/tex" "$DL/ual"

# Microsoft Rocketbox (MIT): sparse checkout of the avatars/motions we use
if [ ! -d "$DL/rocketbox/.git" ]; then
  GIT_LFS_SKIP_SMUDGE=1 git clone --depth 1 --filter=blob:none --no-checkout https://github.com/microsoft/Microsoft-Rocketbox "$DL/rocketbox"
  git -C "$DL/rocketbox" sparse-checkout init --no-cone
  {
    for n in 04 05 07 12 18; do echo "/Assets/Avatars/Adults/Male_Adult_$n/"; done
    for a in m_run_injured m_walk_bruised m_walk_drunk; do echo "/Assets/Animations/all_animations_max_motextr_xy/$a*"; done
    echo "/Assets/Animations/all_animations_max_motextr_static/m_idle_drunk_01*"
    echo "/LICENSE*"
  } > "$DL/rocketbox/.git/info/sparse-checkout"
  git -C "$DL/rocketbox" checkout -q HEAD
fi

# Quaternius Universal Animation Library, standard (CC0)
if [ ! -f "$DL/ual/ual.zip" ]; then
  curl -sSL -o "$DL/ual/ual.zip" https://opengameart.org/sites/default/files/universal_animation_librarystandard.zip
  (cd "$DL/ual" && unzip -oq ual.zip)
fi

# Poly Haven (CC0) props, facade kit, surfaces and sky
(cd "$DL/ph" && python3 "$ROOT/tools/fetch_polyhaven_models.py" concrete_road_barrier covered_car street_lamp_01 old_tyre trashbag \
  metal_trash_can wooden_crate_01 medical_box modular_urban_apartments_facade utility_box_01 fire_hydrant dead_tree_trunk Barrel_01)
(cd "$DL/tex" && python3 "$ROOT/tools/fetch_polyhaven_textures.py" road_damaged cracked_concrete_wall rubble concrete_wall_008 \
  muddy_tracks sparse_grass concrete_pavement worn_cracked_plaster)
U=$(curl -s https://api.polyhaven.com/files/qwantani_night_puresky | python3 -c "import json,sys;print(json.load(sys.stdin)['tonemapped']['url'])")
curl -sSL -A Mozilla/5.0 -o "$DL/tex/sky_full.jpg" "$U"
python3 -c "
from PIL import Image; Image.MAX_IMAGE_PIXELS=None
Image.open('$DL/tex/sky_full.jpg').resize((2048,1024), Image.LANCZOS).save('$DL/tex/sky.jpg', quality=90)"
rm -f "$DL/tex/sky_full.jpg"
echo "assets in $DL"
