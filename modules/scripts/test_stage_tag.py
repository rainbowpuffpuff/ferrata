#!/usr/bin/env python3
"""Offline checks of stage.py's LEZ tag selection: `python3 -I modules/scripts/test_stage_tag.py`.

stage.py is imported, not run, so these tests stage no crates and use no network.
"""
import importlib.util
import itertools
import os
from pathlib import Path
import sys
import unittest
from unittest import mock

sys.dont_write_bytecode = True
HERE = Path(__file__).resolve().parent
REPO = HERE.parent.parent
STAGE = HERE / "stage.py"
SERIAL = itertools.count()


def load_stage(tag=None, lez=None):
    """Import a fresh stage.py with only the requested FERRATA_LEZ_* variables set."""
    with mock.patch.dict(os.environ, clear=False):
        os.environ.pop("FERRATA_LEZ_TAG", None)
        os.environ.pop("FERRATA_LEZ", None)
        if tag is not None:
            os.environ["FERRATA_LEZ_TAG"] = tag
        if lez is not None:
            os.environ["FERRATA_LEZ"] = str(lez)
        spec = importlib.util.spec_from_file_location(f"_ferrata_stage_tag_{next(SERIAL)}", STAGE)
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        return module


class TestStageTag(unittest.TestCase):
    def test_default_is_rc1(self):
        stage = load_stage()
        self.assertEqual(stage.LEZ_TAG, "v0.4.0-rc1")
        self.assertEqual(stage.LEZ_DIR, "upstream/lez-v0.4.0-rc1")
        self.assertEqual(stage.LEZ, (REPO.parent / "upstream" / "lez-v0.4.0-rc1").resolve())

    def test_v040_selects_the_v040_source_and_staging_directory(self):
        stage = load_stage(tag="v0.4.0")
        self.assertEqual(stage.LEZ_TAG, "v0.4.0")
        self.assertEqual(stage.LEZ_DIR, "upstream/lez-v0.4.0")
        self.assertEqual(stage.LEZ, (REPO.parent / "upstream" / "lez-v0.4.0").resolve())

    def test_explicit_lez_source_still_wins(self):
        source = REPO / ".einfra-tmp" / "explicit-lez-source"
        stage = load_stage(tag="v0.4.0", lez=source)
        self.assertEqual(stage.LEZ, source.resolve())
        self.assertEqual(stage.LEZ_DIR, "upstream/lez-v0.4.0")


if __name__ == "__main__":
    unittest.main()
