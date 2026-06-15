"""Database helpers: create a fresh kogu DB from schema.sql, and common paths."""
from __future__ import annotations

import sqlite3
from pathlib import Path

# pipeline/kogupipe/db.py -> pipeline/ -> kogu/
PIPELINE_DIR = Path(__file__).resolve().parent.parent
PROJECT_DIR = PIPELINE_DIR.parent
SCHEMA_PATH = PIPELINE_DIR / "schema.sql"
SOURCES_DIR = PIPELINE_DIR / "sources"
DATA_DIR = PROJECT_DIR / "data"
DB_PATH = DATA_DIR / "kogu.sqlite"


def connect(path: str | Path) -> sqlite3.Connection:
    """Open a connection with foreign keys enforced."""
    conn = sqlite3.connect(str(path))
    conn.execute("PRAGMA foreign_keys = ON")
    return conn


def create_db(path: str | Path, *, schema: str | Path | None = None) -> sqlite3.Connection:
    """Create (overwriting) a fresh DB at ``path`` from the schema and return the connection."""
    path = Path(path)
    if path.exists():
        path.unlink()
    path.parent.mkdir(parents=True, exist_ok=True)
    schema_sql = Path(schema or SCHEMA_PATH).read_text(encoding="utf-8")
    conn = connect(path)
    conn.executescript(schema_sql)
    conn.commit()
    return conn
