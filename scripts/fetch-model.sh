#!/usr/bin/env sh
# Downloads the default (placeholder) benchmark model into assets/models/benchmark.glb.
set -eu
cd "$(dirname "$0")/.."
mkdir -p assets/models

URL="https://raw.githubusercontent.com/KhronosGroup/glTF-Sample-Assets/main/Models/DamagedHelmet/glTF-Binary/DamagedHelmet.glb"
curl -fL --retry 3 -o assets/models/benchmark.glb "$URL"

cat > assets/models/ATTRIBUTION.md <<'EOF'
# Default model attribution

`benchmark.glb` is the Khronos glTF sample model **"Damaged Helmet"**, used here as a
placeholder benchmark model.

- Source: https://github.com/KhronosGroup/glTF-Sample-Assets/tree/main/Models/DamagedHelmet
- Model: "Battle Damaged Sci-fi Helmet - PBR" by theblueturtle_ (2016), glTF rebuild by ctxwing (2018)
- License: CC BY 4.0 (https://creativecommons.org/licenses/by/4.0/)

To use your own model, replace `assets/models/benchmark.glb` with any GLB file,
or pass `--model <path>` on the command line.
EOF

echo "OK: assets/models/benchmark.glb ($(wc -c < assets/models/benchmark.glb) bytes)"
