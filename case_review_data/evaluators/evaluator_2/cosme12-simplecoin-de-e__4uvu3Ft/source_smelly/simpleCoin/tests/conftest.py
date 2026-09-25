"""Shared pytest configuration for the SimpleCoin test suite.

The production modules (miner, wallet, miner_config) live directly inside the
simpleCoin package directory, so tests import them as plain top-level modules.
pytest is normally started from the repository root
(`python -m pytest simpleCoin/tests -q`), which means the simpleCoin directory
itself is not on sys.path; add it here so the imports keep working no matter
where pytest was started from.
"""

import os
import sys

PACKAGE_DIR = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

if PACKAGE_DIR not in sys.path:
    sys.path.insert(0, PACKAGE_DIR)
