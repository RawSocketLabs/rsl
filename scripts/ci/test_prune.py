"""Pruning tests: owned units leave the cache, third-party units stay, unreadable targets go."""

from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest import mock

import prune

HASH = "0123456789abcdef"


class PruneTests(unittest.TestCase):
    def tree(self, root):
        files = [f"debug/deps/libbitsandbytes-{HASH}.rlib", f"debug/deps/dns-{HASH}",
                 f"debug/deps/dns-{HASH}.d", f"debug/deps/libsyn-{HASH}.rlib", f"debug/deps/syn-{HASH}.d",
                 f"debug/deps/chacha20-{HASH}", f"debug/deps/libchacha20-{HASH}.rlib",
                 f"debug/.fingerprint/chacha20-{HASH}/lib", f"debug/.fingerprint/crypto-{HASH}/lib",
                 f"debug/.fingerprint/sdr-gearbox-{HASH}/lib", f"debug/.fingerprint/syn-{HASH}/lib",
                 f"debug/build/bitsandbytes-{HASH}/out", f"debug/build/proc-macro2-{HASH}/out",
                 "debug/examples/sniff", "debug/incremental/x", "debug/dns", "debug/libbitsandbytes.rlib",
                 f"x86_64-unknown-linux-gnu/release/deps/libbitsandbytes-{HASH}.rlib",
                 f"x86_64-unknown-linux-gnu/release/.fingerprint/syn-{HASH}/lib", "nextest/ci/junit.xml"]
        for name in files:
            (root / name).parent.mkdir(parents=True, exist_ok=True)
            (root / name).write_text("x")

    def test_owned_units_are_removed_and_third_party_units_kept(self):
        with tempfile.TemporaryDirectory() as directory:
            target = Path(directory)
            self.tree(target)
            libraries = {"bitsandbytes", "dns", "sdr_gearbox", "crypto"}
            # `chacha20` is also an owned test target: its executable goes, the crate stays.
            prune.prune(target, (libraries, libraries | {"chacha20"}))
            kept = sorted(str(path.relative_to(target)) for path in target.rglob("*") if path.is_file())
            self.assertEqual(kept, [f"debug/.fingerprint/chacha20-{HASH}/lib", f"debug/.fingerprint/syn-{HASH}/lib",
                                    f"debug/build/proc-macro2-{HASH}/out", f"debug/deps/libchacha20-{HASH}.rlib",
                                    f"debug/deps/libsyn-{HASH}.rlib", f"debug/deps/syn-{HASH}.d",
                                    f"x86_64-unknown-linux-gnu/release/.fingerprint/syn-{HASH}/lib"])

    def test_any_pruning_failure_leaves_the_target_out_of_the_cache_without_failing(self):
        patches = {"metadata": ("owned_names", subprocess.CalledProcessError(101, ["cargo", "metadata"])),
                   "unlink": ("prune", PermissionError("unlink"))}
        for name, (function, error) in patches.items():
            with self.subTest(name), tempfile.TemporaryDirectory() as directory:
                target = Path(directory) / "target"
                self.tree(target)
                with mock.patch.object(prune, "owned_names", return_value=(set(), set())), \
                        mock.patch.object(prune, function, side_effect=error), mock.patch("builtins.print"):
                    prune.main([str(target), str(Path(directory) / "absent")])  # returns: no raise
                self.assertFalse(target.exists())

if __name__ == "__main__":
    unittest.main()
