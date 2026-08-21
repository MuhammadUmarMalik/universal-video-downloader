#!/usr/bin/env python3
"""Assert the durable state produced by the native startup recovery coordinator."""

from __future__ import annotations

import argparse
import sqlite3
from pathlib import Path


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("database", type=Path)
    parser.add_argument("partial", type=Path)
    args = parser.parse_args()

    connection = sqlite3.connect(args.database)
    job = connection.execute(
        """
        SELECT status, downloaded_bytes, temp_path, error_code
        FROM download_jobs
        WHERE id = 'native-recovery-job'
        """
    ).fetchone()
    recovery_event = connection.execute(
        """
        SELECT 1
        FROM job_events
        WHERE job_id = 'native-recovery-job'
          AND event_type = 'recovery_queued'
        LIMIT 1
        """
    ).fetchone()
    connection.close()

    expected = ("queued", 5, str(args.partial), None)
    if job != expected:
        raise SystemExit(f"recovery assertion failed: expected {expected!r}, got {job!r}")
    if recovery_event is None:
        raise SystemExit("recovery assertion failed: no recovery_queued event found")
    if args.partial.read_bytes() != b"hello":
        raise SystemExit("recovery assertion failed: partial file contents changed")

    print("native recovery assertion passed")
    print(f"job={job!r}")
    print("event=recovery_queued")


if __name__ == "__main__":
    main()
