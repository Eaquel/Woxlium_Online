# Third-party assets

| Asset | Source | License |
|---|---|---|
| Realistic human avatars (zombies, first-person arms): Male_Adult_04/05/07/12/18 | [Microsoft Rocketbox](https://github.com/microsoft/Microsoft-Rocketbox) | MIT (below) |
| Locomotion/idle motions (`m_walk_drunk`, `m_walk_bruised`, `m_run_injured`, `m_idle_drunk_01`) | Microsoft Rocketbox | MIT |
| Combat motions (Punch_Enter/Jab/Cross, Hit_Chest/Head, Death01, Pistol_Aim_Neutral arms) | [Quaternius Universal Animation Library](https://quaternius.com/packs/universalanimationlibrary.html) | CC0 |
| Props (car, barriers, street lamp, bins, bags, tyre, crate, barrel, hydrant, utility box, tree trunk, medical box) and the modular apartment facade kit | [Poly Haven](https://polyhaven.com) | CC0 |
| Surfaces (road_damaged, concrete_pavement, muddy_tracks, rubble, sparse_grass, cracked_concrete_wall, concrete_wall_008, worn_cracked_plaster) and the qwantani_night_puresky HDRI | Poly Haven | CC0 |
| VFX / UI art (`art/source`) | provided by the project owner | — |

The Rocketbox textures are modified (zombification, blood decals) by `tools/build_assets.py`;
animations are retargeted by `tools/blender/wxm_export.py`.

## Microsoft Rocketbox — MIT License

Copyright (c) 2020 Microsoft

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
