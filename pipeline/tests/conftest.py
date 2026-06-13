"""Shared fixtures. The full build is ~45s, so build it ONCE per test session and let the
backbone/lexeme probe modules open their own read-only connections to it."""
import sqlite3

import pytest

from kanzipipe.build import build


@pytest.fixture(scope="session")
def built_db(tmp_path_factory):
    out = tmp_path_factory.mktemp("kanzi_db") / "kanzi.sqlite"
    build(out)
    return out


@pytest.fixture()
def db(built_db):
    c = sqlite3.connect(built_db)
    yield c
    c.close()
