import types
import unittest

from verification.tools.capture import (
    resolve_arches,
    resolve_case_ids_and_profile_arches,
)


class TestCaptureCli(unittest.TestCase):
    def test_profile_quick_resolves_cases_and_profile_arches(self):
        args = types.SimpleNamespace(profile="quick", case=None)
        case_ids, profile_arches = resolve_case_ids_and_profile_arches(args)

        self.assertEqual(case_ids, ["curated/add_loop_001", "curated/fusion_jcc_001"])
        self.assertEqual(profile_arches, ["HSW", "SKL", "ICL"])

    def test_resolve_arches_prefers_cli(self):
        manifest = {"id": "curated/add_loop_001", "run": {"arches": ["HSW", "SKL"]}}

        arches = resolve_arches(["ICL"], manifest, ["HSW"])

        self.assertEqual(arches, ["ICL"])

    def test_requires_exactly_one_of_profile_or_case(self):
        with self.assertRaises(ValueError):
            resolve_case_ids_and_profile_arches(
                types.SimpleNamespace(profile=None, case=None)
            )

        with self.assertRaises(ValueError):
            resolve_case_ids_and_profile_arches(
                types.SimpleNamespace(profile="quick", case="curated/add_loop_001")
            )
