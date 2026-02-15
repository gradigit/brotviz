# Testing and Benchmarking

## 1) Run the test suite

```bash
cargo test
```

This includes:
- preset count/name sanity checks
- render smoke tests for every preset
- adaptive auto-mode switching behavior

## 2) Benchmark preset rendering

CPU only:

```bash
cargo run --release --bin benchmark -- --mode cpu --frames 180 --w 160 --h 88 --quality balanced
```

Metal only (macOS):

```bash
cargo run --release --bin benchmark -- --mode metal --frames 120 --w 160 --h 88 --quality balanced
```

Both:

```bash
cargo run --release --bin benchmark -- --mode both --frames 120 --w 160 --h 88 --quality balanced
```

Useful flags:
- `--frames N`
- `--w WIDTH`
- `--h HEIGHT`
- `--quality fast|balanced|high|ultra`
- `--scale N`
- `--safe true|false`
- `--ci-smoke` (fails if any preset renders black or exceeds `--max-ms`)
- `--max-ms N` (CI smoke threshold, default `20`)

CI smoke target (fast preset sweep intended for CI):

```bash
cargo bench-smoke
```

Equivalent:

```bash
make ci-smoke
```

## 3) Generate deterministic latency/functionality audio fixture

```bash
cargo run --release --bin gen_test_audio
```

Default output:

assets/test/latency_pulse_120bpm.wav

The file contains:
- startup silence
- timed pulse train (for latency checks)
- smooth section (for slow transitions)
- dense transient section (for jump cuts)
- chirp sweep (for treble/centroid reactivity)

## 4) Practical latency check flow

1. Start visualizer with latency HUD visible.
2. Play fixture file through system audio.
3. Observe HUD latency metrics (`Lat(ms n/a/p95)`) during pulse and dense sections.
4. Compare baseline across renderer/engine combinations.

Example:

```bash
cargo run --release -- --source system --engine metal --renderer kitty
afplay assets/test/latency_pulse_120bpm.wav
```

## 5) Automated latency report (fixture-driven)

Run an offline latency report against the generated fixture:

```bash
cargo latency-report
```

Equivalent:

```bash
make latency-report
```

Direct invocation with custom limits:

```bash
cargo run --release --bin latency_report -- --wav assets/test/latency_pulse_120bpm.wav --fail-over-ms 120
```

This reports:
- matched pulse count
- misses / false positives
- delta stats (`mean`, `p50`, `p95`, `min`, `max`) in milliseconds
