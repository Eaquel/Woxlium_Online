#!/usr/bin/env bash
# Full content rebuild: Blender export (characters, props, facades) + texture/asset packing.
# Needs: Blender 4.5 ($BLENDER), etcpak ($ETCPAK), Python with Pillow/numpy/scipy.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DL="${DL:-$ROOT/build/dl}"
BLENDER="${BLENDER:-blender}"
export ROCKETBOX="$DL/rocketbox/Assets"
export UAL_GLB="$DL/ual/Animation Library[Standard]/Godot/AnimationLibrary_Godot_Standard.glb"
export PH_TEX="$DL/tex"
cd "$ROOT"
"$BLENDER" -b --python tools/blender/wxm_export.py -- tools/blender/characters.json
"$BLENDER" -b --python tools/blender/props_export.py -- "$DL/ph" build/wxm_props concrete_road_barrier:1500 covered_car:4000 \
  street_lamp_01:1500 old_tyre:800 trashbag:800 metal_trash_can:1200 wooden_crate_01:600 medical_box:600 utility_box_01:800 \
  fire_hydrant:900 dead_tree_trunk:2000 Barrel_01:800
"$BLENDER" -b --python tools/blender/facade_build.py -- "$DL/ph/modular_urban_apartments_facade" build/wxm_props
python3 tools/build_assets.py
