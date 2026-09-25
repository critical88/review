#   Additional test scaffolding for the repository's pytest suite.
#   Makes the repository root importable so the generator modules can be
#   imported by name, exactly like the driver scripts do with their own
#   sys.path manipulation.

import os
import sys

REPO_ROOT = os.path.dirname(os.path.abspath(__file__))

if REPO_ROOT not in sys.path:
    sys.path.insert(0, REPO_ROOT)
