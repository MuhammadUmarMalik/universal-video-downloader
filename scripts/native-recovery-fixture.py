#!/usr/bin/env python3
"""Seed one interrupted public download into an initialized UMD SQLite database."""

from __future__ import annotations

import argparse
import os
import sqlite3
from pathlib import Path


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("database", type=Path)
    parser.add_argument("destination", type=Path)
    args = parser.parse_args()

    args.destination.mkdir(parents=True, exist_ok=True)
    timestamp = "2026-08-21T00:00:00Z"
    partial = args.destination / "native-recovery.mp4.part"

    connection = sqlite3.connect(args.database)
    connection.execute("PRAGMA foreign_keys = ON")
    connection.execute(
        """
        INSERT OR IGNORE INTO platforms
          (id, slug, name, enabled, adapter_version, created_at, updated_at)
        VALUES ('native-platform', 'generic', 'Generic', 1, 'fixture', ?, ?)
        """,
        (timestamp, timestamp),
    )
    connection.execute(
        """
        INSERT OR IGNORE INTO media_sources
          (id, platform_id, source_url, normalized_url, source_type, discovered_at)
        VALUES (
          'native-source', 'native-platform',
          'https://example.test/native-source',
          'https://example.test/native-source', 'single', ?
        )
        """,
        (timestamp,),
    )
    connection.execute(
        """
        INSERT OR IGNORE INTO media_items
          (id, source_id, canonical_url, title, first_seen_at, last_seen_at)
        VALUES (
          'native-item', 'native-source', 'https://example.test/native-item',
          'Native recovery fixture', ?, ?
        )
        """,
        (timestamp, timestamp),
    )
    connection.execute(
        """
        INSERT OR IGNORE INTO media_formats
          (id, media_item_id, container, file_size_bytes, is_video, is_audio,
           is_progressive, metadata_json, created_at)
        VALUES (
          'native-format', 'native-item', 'mp4', 5, 1, 1, 1,
          '{"public_url":"https://v.redd.it/native-recovery/video.mp4"}', ?
        )
        """,
        (timestamp,),
    )
    connection.execute(
        """
        INSERT OR REPLACE INTO download_jobs
          (id, media_item_id, format_id, status, priority, destination_path,
           temp_path, filename, total_bytes, downloaded_bytes, retry_count,
           max_retries, created_at, updated_at)
        VALUES (?, ?, ?, 'downloading', 0, ?, ?, ?, 5, 5, 0, 3, ?, ?)
        """,
        (
            "native-recovery-job",
            "native-item",
            "native-format",
            str(args.destination),
            str(partial),
            "native-recovery.mp4",
            timestamp,
            timestamp,
        ),
    )
    connection.execute(
        """
        INSERT OR REPLACE INTO job_events
          (id, job_id, event_type, payload_json, created_at)
        VALUES ('native-recovery-fixture-event', 'native-recovery-job',
                'downloading', '{"fixture":true}', ?)
        """,
        (timestamp,),
    )
    connection.commit()
    connection.close()
    partial.write_bytes(b"hello")
    print(f"seeded database: {args.database}")
    print(f"seeded partial:   {partial}")


if __name__ == "__main__":
    main()
