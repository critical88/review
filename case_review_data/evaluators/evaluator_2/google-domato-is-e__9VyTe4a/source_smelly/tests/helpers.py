#   Shared helpers for the repository's pytest suite.
#   --------------------------------------
#
#   Licensed under the Apache License, Version 2.0 (the "License");
#   you may not use this file except in compliance with the License.
#   You may obtain a copy of the License at
#
#       http://www.apache.org/licenses/LICENSE-2.0

import importlib.util
import os

REPO_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

_DRIVER_MODULES = {
    'canvas': os.path.join(REPO_ROOT, 'canvas', 'generator.py'),
    'jscript': os.path.join(REPO_ROOT, 'jscript', 'generator.py'),
    'php': os.path.join(REPO_ROOT, 'php', 'generator.py'),
    'vbscript': os.path.join(REPO_ROOT, 'vbscript', 'generator.py'),
    'webgl': os.path.join(REPO_ROOT, 'webgl', 'generator.py'),
    'webgpu': os.path.join(REPO_ROOT, 'webgpu', 'generator.py'),
}


def load_driver(name):
    """Loads one of the bundled per-language driver scripts.

    The bundled drivers live next to their own grammar files, so they are
    imported from their on-disk location instead of the repository root.
    """
    path = _DRIVER_MODULES[name]
    spec = importlib.util.spec_from_file_location('domato_%s_driver' % name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


ALL_DRIVERS = {
    'domato': os.path.join(REPO_ROOT, 'generator.py'),
    'canvas': _DRIVER_MODULES['canvas'],
    'jscript': _DRIVER_MODULES['jscript'],
    'php': _DRIVER_MODULES['php'],
    'vbscript': _DRIVER_MODULES['vbscript'],
    'webgl': _DRIVER_MODULES['webgl'],
    'webgpu': _DRIVER_MODULES['webgpu'],
}
