"""kogu offline ingestion pipeline.

Build-time only. Compiles open upstream sources into one normalised SQLite DB
(``data/kogu.sqlite``) served read-only by the Rust backend. Nothing here runs in
the serving path.
"""
