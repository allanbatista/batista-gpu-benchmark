# batista-gpu-benchmark

Cross-platform GPU benchmark built on [Bevy](https://bevy.org) 0.19 / wgpu.
Renders a deterministic PBR scene — a central GLB model on a circular floor,
orbited by seeded colored lights and a moving camera — and captures per-frame
timing into reproducible reports.

Runs on **Linux** (Vulkan), **Windows** (DirectX 12, Vulkan) and **macOS**
(Metal). The OpenGL/GLES option is listed but blocked with a clear error:
Bevy 0.19 cannot create a GL surface (engine limitation, not a benchmark one) —
it will return when the engine regains native GL support.

## Quick start

```bash
./scripts/fetch-model.sh     # downloads the default model (once)
cargo run --release          # interactive mode with settings UI
```

Benchmark from the command line:

```bash
cargo run --release -- \
  --benchmark --preset high --backend vulkan \
  --resolution 1920x1080 --vsync off \
  --warmup 10 --duration 60 --runs 3 \
  --output ./results --exit-after-benchmark
```

## CLI

| Flag | Description |
|---|---|
| `--benchmark` | start the benchmark automatically on launch |
| `--preset <low\|medium\|high\|extreme\|custom>` | quality preset (versioned workload, e.g. `high-v1`) |
| `--backend <auto\|vulkan\|dx12\|metal\|gl>` | graphics API (validated against the OS before startup; `gl` currently errors — engine limitation) |
| `--adapter <index-or-name>` | GPU selection: index from `--list-adapters` or a name substring |
| `--resolution <W>x<H>` | window resolution |
| `--render-scale <0..1]` | internal 3D render scale (downscale only; 1.0 = native) |
| `--window-mode <windowed\|borderless\|fullscreen>` | window mode (exclusive fullscreen is best-effort on Wayland) |
| `--vsync <on\|off>` | present mode |
| `--warmup <s>` / `--duration <s>` / `--runs <n>` | timing plan (defaults 10 / 60 / 3) |
| `--seed <n>` | scene seed (default 42) |
| `--model <path>` | custom GLB replacing the bundled model |
| `--output <dir>` | results directory (default `./results`) |
| `--exit-after-benchmark` | exit when done (code 0 = ok, 1 = runtime/report failure, 2 = invalid args) |
| `--list-adapters` | print all GPU adapters and exit |

Precedence: defaults < `config/settings.toml` < CLI flags.

## How a run works

```
Idle → Loading → Prewarming → Warmup → Running → Finalizing → (Cooldown → Warmup …) → Completed
```

- **Prewarming** waits for assets *and* for the render-pipeline queue to stay
  empty for 60 consecutive frames, so shader compilation never pollutes results.
- **Warmup** (default 10 s) runs the scene without sampling.
- **Running** captures one sample per frame into preallocated memory — no disk
  IO, no allocation on the hot path. The internal benchmark clock resets each
  run, so every run (and every machine) replays the identical trajectory.
- `Esc` cancels. Losing window focus or occlusion during capture marks the run
  as non-comparable.

## Results

Each session writes `results/<YYYY-MM-DD_HH-MM-SS>/`:

- `summary.json` — system, adapter (features/limits), settings snapshot,
  per-run and consolidated metrics, versioned criteria, official/custom flag
  with the exact deviation list.
- `frames.csv` — `run_index, frame_index, elapsed_seconds, frame_time_ms, fps,
  gpu_time_ms`. `elapsed_seconds` is benchmark-clock time (starts at each run's
  warmup), so rows correlate with scene state. `gpu_time_ms` is empty when GPU
  timing is unavailable.
- `settings.toml` — the exact configuration used.
- `screenshot.png` — final frame (includes the overlay; optional).

### Metrics (versioned)

- `avg_fps = 1000·n / Σft` (harmonic mean)
- percentiles: nearest-rank over frame times (P50/P90/P95/P99)
- 1% / 0.1% lows: average of the worst k frames expressed as FPS (`low-agg-v1`)
- stutter: `ft > max(50 ms, 3·median)` (`stutter-v1`)
- `score = avg_fps × clamp(low1/avg_fps, 0, 1)` (`gpu-benchmark-score-v1`) —
  algebraically `min(low1_fps, avg_fps)`, i.e. a stability-bounded FPS
- consolidation: mean/median/best/worst of per-run scores;
  `variation% = 100·(best−worst)/mean`, flagged unstable above 5%

Reports with different criteria/preset versions must not be compared.

## Official comparable profile

A run is marked `official: true` only with: an unmodified versioned preset,
vsync **off**, FPS limit **off**, render scale 1.0, seed 42, the bundled model,
10 s warmup / 60 s / 3 runs, no focus loss, release build. Anything else is
reported as `custom` with the deviation list — still useful, just not
cross-machine comparable.

## Anti-aliasing

Exclusive per-camera selector: **Off / MSAA 2×·4×·8× / FXAA / SMAA / TAA /
DLSS / FSR 1.0**. MSAA levels are pinned by presets; any other mode marks the
run custom. DLSS and FSR 1.0 are upscalers — they change the internal
resolution and are never comparable with native rendering.

- **FSR 1.0** (any GPU): FSR1-style spatial upscale — the 3D pass renders at
  the official FSR quality factors (1.3×/1.5×/1.7×/2.0×) and is upscaled with
  AMD FidelityFX CAS sharpening on top. (Full EASU edge reconstruction is a
  possible future upgrade.)
- **DLSS** (NVIDIA RTX + Vulkan): native Bevy integration; requires building
  with `--features dlss` and the NVIDIA DLSS SDK (`DLSS_SDK` + `VULKAN_SDK`
  env vars, clang). CI never builds it; untested until run on RTX hardware.

## Ray tracing (experimental)

Build with `--features rt-experimental` to enable an experimental ray-traced
lighting mode (bevy_solari — ReSTIR DI/GI). Select it under *Renderer → Render
mode* (restart required). Requirements are detected at runtime (Vulkan +
ray-query support; e.g. RDNA2+/RTX); the option stays disabled otherwise and
the PBR benchmark is never affected.

Notes:
- Solari only samples **directional lights and emissive meshes**, so in RT mode
  the orbiting lights become emissive spheres (real ray-traced light sources)
  and punctual lights are silenced. Shadow mapping is replaced by ray-traced
  visibility.
- Image is noisier than raster (no denoiser without DLSS Ray Reconstruction) —
  this mode benchmarks RT throughput, not image quality.
- Reports are always marked `experimental` + `custom`; results are not
  comparable across systems or with the PBR mode (spec §4.2).

## Replacing the default model

The bundled `assets/models/benchmark.glb` is the Khronos **Damaged Helmet**
sample (CC BY 4.0 — see `assets/models/ATTRIBUTION.md`), used as a placeholder.
Replace the file with any GLB, or pass `--model <path>`. The model is
auto-scaled and grounded via its bounding box; models with PBR textures,
metallic and non-metallic parts and normal maps make the most representative
workload. Runs with a non-bundled model are marked custom.

## Building

Stable Rust (see `rust-version` implied by Bevy 0.19). Linux needs
`libwayland-dev`-era basics only — audio/gamepad system deps are not used.
Release builds are strongly recommended for benchmarking (debug builds are
CPU-bound and marked as a deviation in reports).
