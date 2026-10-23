"""Writes synthetic byte-counter recordings for the measurement tests.

Shapes mimic what a real link does (slow start, plateau, noise, stalls).
Replace or extend with recorded runs from a real test; never include IPs.
"""
import json
import math
import random
from pathlib import Path

out = Path(__file__).resolve().parents[1] / "src-tauri" / "fixtures" / "throughput"
out.mkdir(parents=True, exist_ok=True)


def record(name, description, rate_at, seconds, cap_bytes=None, step_ms=100, seed=7):
    rng = random.Random(seed)
    readings, total, now = [], 0.0, 0
    while now < seconds * 1000:
        step = step_ms + rng.randint(-8, 8)
        mbps = max(0.0, rate_at(now / 1000, rng))
        total += mbps * 1_000_000 / 8 * step / 1000
        now += step
        if cap_bytes and total >= cap_bytes:
            readings.append([now, int(total)])
            break
        readings.append([now, int(total)])
    (out / f"{name}.json").write_text(json.dumps({"description": description, "synthetic": True, "readings": readings}, separators=(",", ":")) + "\n")


record("steady-300", "300 Mbps link, half-second ramp, ±3% noise", lambda t, r: 300 * (1 - math.exp(-t / 0.5)) * (1 + r.uniform(-0.03, 0.03)), 10)
record("slow-ramp-200", "Ramps linearly to 200 Mbps over 6 s, then holds", lambda t, r: min(200, 200 * t / 6) * (1 + r.uniform(-0.02, 0.02)), 10)
record("stall", "200 Mbps for 2.5 s, then nothing arrives", lambda t, r: 200 * (1 - math.exp(-t / 0.4)) if t < 2.5 else 0, 10)
record("cap-gigabit", "1 Gbps link that reaches the 380 MB cap", lambda t, r: 1000 * (1 - math.exp(-t / 0.4)) * (1 + r.uniform(-0.02, 0.02)), 10, cap_bytes=380_000_000)
record("spiky-100", "100 Mbps average with ±45% swings (busy Wi-Fi)", lambda t, r: 100 * (1 - math.exp(-t / 0.4)) * (1 + r.uniform(-0.45, 0.45)), 10, seed=11)
record("too-short", "Cut off after 1.6 s", lambda t, r: 150 * (1 - math.exp(-t / 0.4)), 1.6)
