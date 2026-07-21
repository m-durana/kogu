from __future__ import annotations
import os, sqlite3, sys
from .db import DB_PATH
from .ingest import native_wikt
def main(argv):
    path = argv[0] if argv else os.environ.get("KOGU_DB", str(DB_PATH))
    print(f"applying native Wiktionary etymology to {path}")
    conn = sqlite3.connect(path); native_wikt.ingest(conn); conn.commit()
    n = conn.execute("SELECT count(*) FROM etymology WHERE source='wiktionary-native'").fetchone()[0]
    conn.close(); print(f"  native-wiktionary rows now: {n}"); return 0
if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
