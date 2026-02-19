#!/usr/bin/env python3
"""Benchmark script for measuring pipeline response latency."""

import argparse
import json
import statistics
import time
import urllib.request

TEST_SENTENCES = [
    # Polish - fillers
    ("yyy no więc jakby to jest ważne", "pl"),
    ("eee znaczy generalnie chciałem powiedzieć", "pl"),
    # Polish - word corrections (splits)
    ("napewno to jest na prawdę ważne", "pl"),
    ("wogóle nie wiem co powiedzieć narazie", "pl"),
    ("poprostu przedewszystkim trzeba to zrobić", "pl"),
    # Polish - word corrections (joins)
    ("dla tego po mimo wszystko udało się", "pl"),
    ("po nie waż to jest na przeciwko", "pl"),
    # Polish - ITN
    ("mam dwadzieścia trzy lata i mieszkam tu pięć lat", "pl"),
    # Polish - long input
    ("yyy no więc jakby generalnie chciałem powiedzieć że napewno wogóle poprostu to jest bardzo ważna sprawa i trzeba to koniecznie omówić na spotkaniu", "pl"),
    # Polish - short/clean
    ("dzień dobry", "pl"),
    ("proszę o pomoc w tej sprawie", "pl"),
    # English - fillers
    ("um like you know basically its fine", "en"),
    ("uh i mean sort of actually right", "en"),
    # English - word corrections (homophones)
    ("your going to loose alot", "en"),
    ("its going to effect the weather or not we go", "en"),
    ("there going to be better then us", "en"),
    ("i would of done it if its possible", "en"),
    # English - ITN
    ("i have twenty three cats and five dogs", "en"),
    # English - long input
    ("um like you know basically your going to loose alot of time if you dont figure out weather or not its going to work and there not helping", "en"),
    # English - short/clean
    ("hello world", "en"),
]


def send_request(url, text):
    """Send a chat completion request and return (response_text, elapsed_ms)."""
    payload = json.dumps({
        "model": "text-cleanup-pipeline",
        "messages": [{"role": "user", "content": text}],
    }).encode()

    req = urllib.request.Request(
        f"{url}/v1/chat/completions",
        data=payload,
        headers={"Content-Type": "application/json"},
    )

    start = time.perf_counter()
    with urllib.request.urlopen(req) as resp:
        resp.read()
    elapsed = (time.perf_counter() - start) * 1000
    return elapsed


def run_bench(url, rounds, warmup):
    total_requests = len(TEST_SENTENCES) * rounds
    print(f"Benchmark: {total_requests} requests ({len(TEST_SENTENCES)} sentences x {rounds} rounds), {warmup} warmup round(s)")
    print(f"Target: {url}/v1/chat/completions")
    print()

    # Warmup
    for w in range(warmup):
        for text, _ in TEST_SENTENCES:
            try:
                send_request(url, text)
            except Exception as e:
                print(f"Error during warmup: {e}")
                return

    # Measured rounds
    # timings[i] = list of latencies for sentence i across rounds
    timings = [[] for _ in TEST_SENTENCES]
    all_latencies = []
    wall_start = time.perf_counter()

    for r in range(rounds):
        for i, (text, _) in enumerate(TEST_SENTENCES):
            try:
                ms = send_request(url, text)
            except Exception as e:
                print(f"Error on round {r+1}, sentence {i+1}: {e}")
                return
            timings[i].append(ms)
            all_latencies.append(ms)

    wall_total = time.perf_counter() - wall_start

    # Per-sentence table
    trunc = 40
    header = f" {'Sentence (truncated)':<{trunc}}  Lang   Mean ms"
    sep = " " + "\u2500" * (trunc + 18)
    print(header)
    print(sep)
    for i, (text, lang) in enumerate(TEST_SENTENCES):
        display = text[:trunc].ljust(trunc)
        mean = statistics.mean(timings[i])
        print(f" {display}  {lang:<4}  {mean:>8.1f}")

    print()
    print(" Overall latency (ms)")
    print(" " + "\u2500" * 21)
    print(f" Min:   {min(all_latencies):>8.1f}")
    print(f" Max:   {max(all_latencies):>8.1f}")
    print(f" Mean:  {statistics.mean(all_latencies):>8.1f}")

    sorted_lat = sorted(all_latencies)
    n = len(sorted_lat)
    p50 = sorted_lat[int(n * 0.50)]
    p95 = sorted_lat[min(int(n * 0.95), n - 1)]
    p99 = sorted_lat[min(int(n * 0.99), n - 1)]
    print(f" P50:   {p50:>8.1f}")
    print(f" P95:   {p95:>8.1f}")
    print(f" P99:   {p99:>8.1f}")
    print(f" Total: {wall_total:>7.1f}s")


def main():
    parser = argparse.ArgumentParser(description="Benchmark the text-cleanup pipeline")
    parser.add_argument("--url", default="http://localhost:8787", help="Server base URL (default: http://localhost:8787)")
    parser.add_argument("--rounds", type=int, default=3, help="Number of measured rounds (default: 3)")
    parser.add_argument("--warmup", type=int, default=1, help="Number of warmup rounds to discard (default: 1)")
    args = parser.parse_args()

    run_bench(args.url.rstrip("/"), args.rounds, args.warmup)


if __name__ == "__main__":
    main()
