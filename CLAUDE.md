# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Commands

```bash
# Frontend dev server only (no Tauri)
npm run dev

# Full Tauri app in dev mode (hot-reload frontend + Rust recompile on save)
npm run tauri dev

# Build frontend for production
npm run build

# Build full Tauri app (macOS .app / .dmg)
npm run tauri build

# Check Rust compilation without linking
cd src-tauri && cargo check

# Run Rust tests
cd src-tauri && cargo test
```

There is no linter configured. No test suite exists on the frontend side.

## Architecture

This is a **Tauri v2** desktop app (Vue 3 frontend + Rust backend) for audio noise reduction.

### Data flow

1. User picks an audio file → `invoke('analyze_audio')` → Rust decodes with **symphonia**, returns metadata
2. User clicks "Enhance" → `invoke('process_audio')` → Rust pipeline:
   - Decode with symphonia → deinterleave channels → resample to 48 kHz → denoise with **nnnoiseless** (RNNoise, single pass) → resample back → optional peak normalize to -1 dBFS → write WAV/MP3
3. User can apply post-processing gain via `invoke('boost_volume')` — reads and rewrites the already-processed file without re-denoising
4. Cancellation: `invoke('cancel_process')` sets an `Arc<AtomicBool>` in Tauri state; the denoising loop checks it every frame

Progress events flow Rust → frontend via `app.emit("audio-progress", AudioProgressPayload)`. The frontend listener filters by `task` field (`"analyze"` or `"process"`).

### Rust backend (`src-tauri/src/`)

- **`lib.rs`** — Tauri command definitions and app setup. All commands are `async` + `spawn_blocking` so the UI stays responsive. Manages `CancelFlag` state.
- **`audio.rs`** — All audio processing logic: decode, resample (linear interpolation), denoise (`nnnoiseless`), peak normalize, WAV write, MP3 transcode via `ffmpeg` CLI.
- **`models.rs`** — Serde structs for IPC: `AnalyzeAudioRequest/Response`, `ProcessAudioRequest/Response`, `AudioProgressPayload`. `ProcessAudioRequest` has `normalize: bool` (default false) for peak normalization.

Key constants in `audio.rs`: `DENOISE_SAMPLE_RATE = 48_000`, `FRAME_SIZE = DenoiseState::FRAME_SIZE` (480 samples). nnnoiseless expects samples scaled to i16 range (`× 32768`) on input, divide back on output.

### Frontend (`src/`)

- **`App.vue`** — Single-page app. All state lives here. `isWorking = busy || analyzing || boosting` gates all controls during any async operation.
- **`components/WaveformPlayer.vue`** — WaveSurfer.js wrapper. Key fix: `waveRoot` ref is always in the DOM when `filePath` is set (not inside `v-if="!isLoading"`), with spinner overlaid via `position: absolute`. Uses `await nextTick()` before initialising WaveSurfer.
- **`i18n/en.js` + `i18n/it.js`** — All UI strings. Add new keys to both files when adding UI text.
- **`styles.css`** — Tailwind base + any global overrides.

### Window configuration

`titleBarStyle: "Transparent"` + `hiddenTitle: true` in `tauri.conf.json` removes the macOS gray title bar while keeping native drag and traffic light buttons. Header has `pt-7` to clear the native title bar area.

### Asset protocol

`tauri.conf.json` enables `assetProtocol` with scope `["$HOME/**", "/tmp/**"]` — required for WaveSurfer to load local audio files via `convertFileSrc()`.

### MP3 export

Requires `ffmpeg` on `$PATH`. The Rust code runs `ffmpeg` as a child process; if not found, returns a user-facing error.
