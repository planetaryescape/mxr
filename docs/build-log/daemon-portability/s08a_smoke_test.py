#!/usr/bin/env python3
"""Small regression checks for the supervised-stop smoke log filter."""

from __future__ import annotations

import importlib.util
import signal
import unittest
from pathlib import Path

spec = importlib.util.spec_from_file_location("s08a_smoke", Path(__file__).with_name("s08a_smoke.py"))
assert spec is not None and spec.loader is not None
smoke = importlib.util.module_from_spec(spec)
spec.loader.exec_module(smoke)


class ShutdownLogFilterTests(unittest.TestCase):
    def test_sigterm_allows_only_relationship_analytics_cancellation(self) -> None:
        log = (
            '\x1b[33mWARN service="relationship" service shutdown signal timed out\x1b[0m\n'
            'WARN task=relationship_worker runtime task drain timed out\n'
        )
        self.assertEqual(
            smoke.validate_shutdown_log(log, signal.SIGTERM),
            [
                ("service timeout", "relationship"),
                ("runtime task timeout", "relationship_worker"),
            ],
        )

    def test_sigint_rejects_any_timeout(self) -> None:
        with self.assertRaisesRegex(RuntimeError, "SIGINT"):
            smoke.validate_shutdown_log(
                'WARN service="relationship" service shutdown signal timed out',
                signal.SIGINT,
            )

    def test_sigterm_rejects_other_or_unidentified_timeouts(self) -> None:
        lines = (
            'WARN service="semantic" service shutdown signal timed out',
            "WARN task=search_worker runtime task drain timed out",
            "WARN service shutdown signal timed out",
            "WARN client connection drain timed out",
        )
        for line in lines:
            with self.subTest(line=line), self.assertRaisesRegex(RuntimeError, "SIGTERM"):
                smoke.validate_shutdown_log(line, signal.SIGTERM)

    def test_service_failures_and_duplicate_cancellations_are_fatal(self) -> None:
        lines = (
            'WARN service="relationship" service shutdown signal failed',
            'WARN service="relationship" service shutdown signal timed out\n'
            'WARN service="relationship" service shutdown signal timed out',
        )
        for log in lines:
            with self.subTest(log=log), self.assertRaisesRegex(RuntimeError, "SIGTERM"):
                smoke.validate_shutdown_log(log, signal.SIGTERM)


if __name__ == "__main__":
    unittest.main()
