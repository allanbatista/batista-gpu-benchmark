# batista-gpu-benchmark

[![build](https://github.com/allanbatista/batista-gpu-benchmark/actions/workflows/build.yml/badge.svg)](https://github.com/allanbatista/batista-gpu-benchmark/actions/workflows/build.yml)

Cross-platform GPU benchmark built on [Bevy](https://bevy.org) 0.19 / wgpu.
Renders a deterministic PBR scene — an astronaut inside a circular futuristic
spaceship room, orbited by seeded colored lights and a moving camera — and
captures per-frame timing into reproducible reports.

Runs on **Linux** (Vulkan), **Windows** (DirectX 12, Vulkan) and **macOS**
(Metal). The OpenGL/GLES option is listed but blocked with a clear error:
Bevy 0.19 cannot create a GL surface (engine limitation) — it will return when
the engine regains native GL support.

![build status](https://img.shields.io/badge/rust-stable-orange) ![license](https://img.shields.io/badge/license-MIT-blue)

## Installation

Grab a package from the [releases page](https://github.com/allanbatista/batista-gpu-benchmark/releases)
(also produced as artifacts by every CI run on `main`):

| Format | Install |
|---|---|
| **Debian/Ubuntu** (`.deb`) | `sudo apt install ./batista-gpu-benchmark_*.deb` |
| **Fedora/RHEL** (`.rpm`, dnf/yum) | `sudo dnf install ./batista-gpu-benchmark-*.rpm` |
| **AppImage** | `chmod +x batista-gpu-benchmark-x86_64.AppImage && ./batista-gpu-benchmark-x86_64.AppImage` |
| **Snap** | `sudo snap install --dangerous ./batista-gpu-benchmark_*.snap` |
| **Flatpak** | `flatpak install ./batista-gpu-benchmark.flatpak` then `flatpak run com.allanbatista.GpuBenchmark` |
| **Windows / macOS** | portable binaries attached to releases |

Installed apps store settings in the user config dir
(`~/.config/batista-gpu-benchmark/settings.toml`) and write results to
`./results` relative to where they are launched (use `--output <dir>` to pick).

## Building from source

Stable Rust. On Linux no exotic system deps are required (audio/gamepad
subsystems are compiled out).

```bash
make build        # cargo build --release
make test         # cargo test --release
sudo make install # installs to /usr/local (PREFIX=... DESTDIR=... supported)
```

Package targets (each builds the release binary first):

```bash
make deb          # needs: cargo install cargo-deb
make rpm          # needs: cargo install cargo-generate-rpm
make appimage     # downloads appimagetool on first run
make snap         # needs: snapcraft
make flatpak      # needs: flatpak-builder + org.freedesktop runtime 24.08
```

## Quick start

```bash
cargo run --release          # interactive mode with settings UI
```

Benchmark from the command line:

```bash
batista-gpu-benchmark \
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

Precedence: defaults < settings file < CLI flags.

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
  per-run and consolidated metrics, GPU pass timings, optional telemetry
  (CPU/RAM %, and on Linux amdgpu: GPU busy %, VRAM, temperature, power),
  versioned criteria, official/custom flag with the exact deviation list.
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

## The benchmark scene

The bundled `assets/models/benchmark.glb` is the official workload model
("Astronaut in a White Suit" — see `assets/models/ATTRIBUTION.md`). Comparable
runs must use it (same model, same preset version, same seed). You can still
load any GLB with `--model <path>` — the model is auto-scaled and grounded via
its bounding box — but such runs are reported as `custom`.

The environment (`assets/models/environment.glb`) is a circular futuristic
spaceship room — white walls with black ribs/trim and emissive ring lighting —
generated procedurally by `scripts/build_environment.py` (Blender ≥4.x,
headless):

```bash
blender --background --python scripts/build_environment.py -- "$PWD/assets/models/environment.glb"
```

It provides the scene's floor, is part of the official workload, and its
emissive rings become real ray-traced light sources in the experimental RT
mode.

## Anti-aliasing & upscaling

Two independent axes:

**Anti-aliasing** — native-resolution AA: **Off / MSAA 2×·4×·8× / FXAA /
SMAA / TAA**. MSAA levels are pinned by presets; a different AA marks the run
custom.

**Upscaler** — renders the 3D pass at a lower internal resolution and
reconstructs: **Off (native) / FSR 1.0 / DLSS**. Any upscaler marks the run
non-comparable (the internal resolution changes).

- **FSR 1.0** (any GPU): FSR1-style spatial upscale at the official quality
  factors (1.3×/1.5×/1.7×/2.0×) plus AMD FidelityFX CAS sharpening. Being
  spatial, it **composes with any AA mode** (e.g. FSR 1.0 + MSAA 4×). Full
  EASU edge reconstruction is a possible future upgrade.
- **DLSS** (NVIDIA RTX + Vulkan): temporal upscaler — it **replaces
  anti-aliasing entirely** (the AA selector is disabled while DLSS is active).
  Requires building with `--features dlss` and the NVIDIA DLSS SDK
  (`DLSS_SDK` + `VULKAN_SDK` env vars, clang). CI never builds it; untested
  until run on RTX hardware.

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
  visibility. The room's emissive rings also become RT light sources.
- Image is noisier than raster (no denoiser without DLSS Ray Reconstruction) —
  this mode benchmarks RT throughput, not image quality.
- Reports are always marked `experimental` + `custom`; results are not
  comparable across systems or with the PBR mode.

## License

MIT — see [LICENSE](LICENSE). The bundled astronaut model was generated with
Meshy AI by the project author; the environment is generated procedurally from
this repository.
