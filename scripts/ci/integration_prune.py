"""Full-tier pruning test against Cargo itself, run after a job's build filled its registry."""

from pathlib import Path
import unittest

import prune

ROOT = Path(__file__).resolve().parents[2]


class OwnedNamesTests(unittest.TestCase):
    def test_owned_names_need_no_registry(self):
        # A job holds only the crates it built; dependency metadata would need every one.
        libraries, executables = prune.owned_names(ROOT, ROOT / "devices/sdr/lsdr/nostd-check")
        self.assertLessEqual({"bitsandbytes", "bitsandbytes_macros", "dns", "crypto"}, libraries)
        # crypto's oracle tests share third-party crate names: executables only, never libraries.
        self.assertIn("chacha20", executables - libraries)
        self.assertIn("lsdr_nostd_check", libraries)


if __name__ == "__main__":
    unittest.main()
