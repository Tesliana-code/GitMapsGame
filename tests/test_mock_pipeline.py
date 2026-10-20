import sys
import unittest
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))
from mock_pipeline import SCENARIOS, evaluate

class ReleaseContractTests(unittest.TestCase):
    def test_every_synthetic_scenario(self):
        for name, item in SCENARIOS.items():
            with self.subTest(name=name):
                self.assertEqual(evaluate(item["ci"], item["approved"], item["same_sha"]), item["expected"])
    def test_missing_check_denies(self):
        for args in [(False, False, False), (True, False, False), (False, True, True)]:
            with self.subTest(args=args):
                self.assertNotEqual(evaluate(*args), "MOCK_RELEASE_VERIFIED")
    def test_success_requires_all_checks(self):
        self.assertEqual(evaluate(True, True, True), "MOCK_RELEASE_VERIFIED")
if __name__ == "__main__":
    unittest.main()
