#!/usr/bin/env python3
"""Time local read-only mail views without printing mail or authentication data."""

import argparse
import json
import math
import statistics
import subprocess
import time


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", default="mxr")
    parser.add_argument("--repeat", type=int, default=3)
    parser.add_argument("--budget-ms", type=float, default=250)
    parser.add_argument("--timeout", type=float, default=30)
    parser.add_argument("--case", action="append", choices=["status", "attention", "all", "mine", "theirs", "unread", "now", "rail", "todo", "updates", "reading", "archive", "search"])
    args = parser.parse_args()
    if args.repeat < 1 or args.timeout <= 0 or args.budget_ms <= 0:
        parser.error("repeat, timeout and budget must be positive")
    cases = {
        "status": ["status"],
        "attention": ["messages", "list"],
        "all": ["messages", "list", "--turn", "all"],
        "mine": ["messages", "list", "--turn", "mine"],
        "theirs": ["messages", "list", "--turn", "theirs"],
        "unread": ["messages", "list", "--unread"],
        "now": ["now"],
        "rail": ["modes", "rail"],
        "todo": ["todo", "list", "--state", "open"],
        "updates": ["updates"],
        "reading": ["reading", "edition", "--peek"],
        "archive": ["records", "list"],
        "search": ["search", "is:unread", "--limit", "50"],
    }
    passed = True
    for name, command in cases.items():
        if args.case and name not in args.case:
            continue
        samples = []
        failures = []
        for run in range(args.repeat):
            started = time.perf_counter()
            try:
                result = subprocess.run(
                    [args.binary, *command, "--format", "json"],
                    capture_output=True, timeout=args.timeout, check=False,
                )
                if result.returncode:
                    failures.append({"run": run + 1, "exit": result.returncode})
                else:
                    # Validate the structured reply, but never retain or emit its mail.
                    json.loads(result.stdout)
            except subprocess.TimeoutExpired:
                failures.append({"run": run + 1, "error": "timeout"})
            except (OSError, ValueError):
                failures.append({"run": run + 1, "error": "invalid command or JSON reply"})
            samples.append(round((time.perf_counter() - started) * 1000, 2))
        ordered = sorted(samples)
        within_budget = not failures and max(samples) <= args.budget_ms
        passed = passed and within_budget
        print(json.dumps({
            "case": name, "samples_ms": samples,
            "median_ms": statistics.median(samples),
            "p95_ms": ordered[math.ceil(len(ordered) * 0.95) - 1],
            "within_budget": within_budget, "failures": failures,
        }), flush=True)
    return 0 if passed else 1


if __name__ == "__main__":
    raise SystemExit(main())
