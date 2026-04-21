import unittest

from verification.tools.canonicalize import canonicalize_result


class TestCanonicalize(unittest.TestCase):
    def test_sorts_event_arrays(self):
        raw = {
            "cycles": [
                {
                    "cycle": 0,
                    "executed": [
                        {"rnd": 1, "instrID": 2, "uopID": 1},
                        {"rnd": 0, "instrID": 2, "uopID": 0},
                    ],
                }
            ]
        }

        out = canonicalize_result(raw)

        self.assertEqual(out["cycles"][0]["executed"][0]["rnd"], 0)

    def test_sorts_keys_recursively(self):
        out = canonicalize_result({"b": 1, "a": {"d": 1, "c": 2}})

        self.assertEqual(list(out.keys()), ["a", "b"])
        self.assertEqual(list(out["a"].keys()), ["c", "d"])
