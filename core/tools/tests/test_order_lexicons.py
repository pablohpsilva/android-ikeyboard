"""Unit tests for tools/order_lexicons.py — the bundled-lexicon frequency order.

The lexicons the shell hands to the Rust core carry their frequency in their
*line order*: `build_packs` records each word's input position as its bundled
rank. These tests pin the generator that produces that order, and the `--check`
gate that stops it drifting back to alphabetical (as it silently had).

Every test builds its own temporary asset tree — the real assets are never read
or written here.
"""

import os
import subprocess
import sys
import tempfile
import unittest

TOOLS = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, TOOLS)

import order_lexicons  # noqa: E402


def write(path, lines):
    """Write one word per line, LF-terminated, like the shipped assets."""
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "w", encoding="utf-8", newline="\n") as fh:
        fh.write("".join(w + "\n" for w in lines))


def read(path):
    with open(path, encoding="utf-8") as fh:
        return fh.read()


class OrderLexiconsTest(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.assets = os.path.join(self.tmp.name, "assets")
        self.addCleanup(self.tmp.cleanup)

    def lexicon(self, tag):
        return os.path.join(self.assets, "lexicons", f"{tag}.txt")

    def fixture(self, tag, lexicon, freq):
        write(self.lexicon(tag), lexicon)
        write(os.path.join(self.assets, "freq", f"{tag}.txt"), freq)

    def words(self, tag):
        return read(self.lexicon(tag)).split("\n")[:-1]

    def test_orders_a_lexicon_by_the_frequency_list(self):
        self.fixture("en", ["apple", "the", "zoo"], ["the", "zoo", "apple"])
        self.assertEqual(order_lexicons.main(["--assets", self.assets]), 0)
        self.assertEqual(self.words("en"), ["the", "zoo", "apple"])

    def test_preserves_the_word_set_exactly(self):
        lexicon = ["alpha", "beta", "gamma", "delta"]
        self.fixture("en", lexicon, ["delta", "alpha", "gamma", "beta"])
        order_lexicons.main(["--assets", self.assets])
        self.assertEqual(sorted(self.words("en")), sorted(lexicon))
        self.assertEqual(len(self.words("en")), len(lexicon))

    def test_appends_words_with_no_frequency_rank_lexicographically(self):
        # "zeta" and "eta" are in no freq list: they trail every ranked word,
        # in lexicographic order among themselves.
        self.fixture("en", ["zeta", "the", "eta"], ["the"])
        order_lexicons.main(["--assets", self.assets])
        self.assertEqual(self.words("en"), ["the", "eta", "zeta"])

    def test_does_not_assume_the_lexicon_is_a_prefix_of_the_freq_list(self):
        # The real `lb` shape: the lexicon's words sit deep in the freq list,
        # far past its own length. Taking "the first N freq lines" would return
        # a different word set entirely.
        freq = [f"w{i:02d}" for i in range(50)]
        self.fixture("lb", ["w40", "w05", "w22"], freq)
        order_lexicons.main(["--assets", self.assets])
        self.assertEqual(self.words("lb"), ["w05", "w22", "w40"])

    def test_check_fails_on_an_alphabetical_lexicon(self):
        self.fixture("en", ["apple", "the", "zoo"], ["the", "zoo", "apple"])
        self.assertEqual(order_lexicons.main(["--check", "--assets", self.assets]), 1)
        # …and left the file untouched.
        self.assertEqual(self.words("en"), ["apple", "the", "zoo"])

    def test_check_names_the_offending_file(self):
        self.fixture("en", ["apple", "the", "zoo"], ["the", "zoo", "apple"])
        stale = order_lexicons.stale_lexicons(self.assets)
        self.assertEqual([tag for tag, _ in stale], ["en"])

    def test_check_passes_on_an_ordered_lexicon(self):
        self.fixture("en", ["the", "zoo", "apple"], ["the", "zoo", "apple"])
        self.assertEqual(order_lexicons.main(["--check", "--assets", self.assets]), 0)

    def test_is_idempotent(self):
        self.fixture("en", ["apple", "the", "zoo"], ["the", "zoo", "apple"])
        order_lexicons.main(["--assets", self.assets])
        once = read(self.lexicon("en"))
        order_lexicons.main(["--assets", self.assets])
        self.assertEqual(read(self.lexicon("en")), once)

    def test_preserves_trailing_newline_and_lf_endings(self):
        self.fixture("en", ["apple", "the"], ["the", "apple"])
        order_lexicons.main(["--assets", self.assets])
        out = read(self.lexicon("en"))
        self.assertTrue(out.endswith("\n"))
        self.assertFalse(out.endswith("\n\n"))
        self.assertNotIn("\r", out)

    def test_a_lexicon_with_no_freq_list_is_left_untouched_and_reported(self):
        write(self.lexicon("ru"), ["мир", "да"])
        self.assertEqual(order_lexicons.main(["--assets", self.assets]), 0)
        self.assertEqual(self.words("ru"), ["мир", "да"])

    def test_resolves_assets_independently_of_cwd(self):
        # ci.yml runs the gate with working-directory: core, while the
        # regeneration command is run from the repo root. Both must reach the
        # same assets, so the default path is derived from the tool's location.
        expected = os.path.join(
            os.path.dirname(TOOLS), os.pardir,
            "apps", "android", "ime-service", "src", "main", "assets",
        )
        self.assertEqual(
            os.path.realpath(order_lexicons.DEFAULT_ASSETS),
            os.path.realpath(expected),
        )
        # Same verdict and same report from both working directories. Asserting
        # equality rather than exit 0 keeps this test independent of whether the
        # committed assets happen to be ordered right now.
        runs = [
            subprocess.run(
                [sys.executable, os.path.join(TOOLS, "order_lexicons.py"), "--check"],
                cwd=cwd, capture_output=True, text=True,
            )
            for cwd in (os.path.dirname(TOOLS), os.path.dirname(os.path.dirname(TOOLS)))
        ]
        self.assertEqual(runs[0].returncode, runs[1].returncode)
        self.assertEqual(runs[0].stdout, runs[1].stdout)


if __name__ == "__main__":
    unittest.main()
