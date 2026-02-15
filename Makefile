.PHONY: ci-smoke latency-report

ci-smoke:
	cargo run --release --bin benchmark -- --mode cpu --ci-smoke --frames 12 --w 96 --h 54 --quality fast --max-ms 20

latency-report:
	cargo run --release --bin latency_report -- --wav assets/test/latency_pulse_120bpm.wav --fail-over-ms 120
