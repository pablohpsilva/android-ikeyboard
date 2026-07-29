#!/usr/bin/env python3
"""Order the bundled lexicons by real word frequency — and gate that they stay so.

**Why this exists.** The shell hands `assets/lexicons/<tag>.txt` to the Rust core
(`Lexicons.load` -> `KeyboardCore.open`), and `build_packs` records each word's
*input position* as its bundled rank (0 = commonest) before byte-sorting a copy
for the `fst`. Line order therefore **is** the frequency signal: it feeds
correction candidate ranking, the suggestion strip's `dict_rank`, and the
accent-variant order.

`Dictionary::from_sorted_words` once *required* byte-sorted input, so these files
were authored alphabetically. Wave 4 relaxed that and re-purposed input order as
the frequency carrier — but the assets were never re-ordered, so every consumer
was reading alphabetical position as if it were commonness. This tool re-orders
them from the frequency lists already shipped alongside (`assets/freq/<tag>.txt`)
and, with `--check`, fails the build if they ever drift back.

    python3 core/tools/order_lexicons.py            # regenerate
    python3 core/tools/order_lexicons.py --check    # gate: exit 1 + report if stale

**Re-order, never re-derive.** A lexicon's word *set* is a curated decision made
elsewhere; only its order is generated here. The tool refuses to write a file
whose word set would change, so a malformed frequency list can never quietly add
or drop vocabulary.

Stdlib only, no network — like every other tool in this directory.
"""

import argparse
import os
import sys

_HERE = os.path.dirname(os.path.abspath(__file__))

#: The Android asset root, resolved from this file rather than the working
#: directory: `ci.yml` runs the gate with `working-directory: core`, while the
#: regeneration command is typically run from the repository root.
DEFAULT_ASSETS = os.path.normpath(
    os.path.join(_HERE, os.pardir, os.pardir,
                 "apps", "android", "ime-service", "src", "main", "assets")
)

#: Sort key for a word carrying no frequency rank — it trails every ranked word.
UNRANKED = float("inf")


def read_words(path):
    """The non-empty, stripped lines of `path`, in file order."""
    with open(path, encoding="utf-8") as fh:
        return [line.strip() for line in fh if line.strip()]


def freq_positions(assets, tag):
    """`{word: position}` from `freq/<tag>.txt`, or `None` if it does not ship.

    Position 0 is the commonest word. The shipped frequency lists hold no
    duplicates; should one appear, the earliest (most frequent) position wins.
    """
    path = os.path.join(assets, "freq", f"{tag}.txt")
    if not os.path.isfile(path):
        return None
    positions = {}
    for index, word in enumerate(read_words(path)):
        positions.setdefault(word, index)
    return positions


def ordered(words, positions):
    """`words` commonest-first; those with no rank trail, in lexicographic order."""
    return sorted(words, key=lambda w: (positions.get(w, UNRANKED), w))


def lexicon_tags(assets):
    """Every language tag with a bundled lexicon, in a stable order."""
    directory = os.path.join(assets, "lexicons")
    if not os.path.isdir(directory):
        return []
    return sorted(
        name[: -len(".txt")] for name in os.listdir(directory) if name.endswith(".txt")
    )


def desired(assets, tag):
    """The correctly ordered words for `tag`, or `None` when it has no freq list."""
    positions = freq_positions(assets, tag)
    if positions is None:
        return None
    return ordered(read_words(os.path.join(assets, "lexicons", f"{tag}.txt")), positions)


def stale_lexicons(assets):
    """`[(tag, reason)]` for every lexicon not in frequency order."""
    stale = []
    for tag in lexicon_tags(assets):
        want = desired(assets, tag)
        if want is None:
            continue
        current = read_words(os.path.join(assets, "lexicons", f"{tag}.txt"))
        if current != want:
            first = next(
                (i for i, (a, b) in enumerate(zip(current, want)) if a != b), 0
            )
            stale.append(
                (tag, f"line {first + 1}: has {current[first]!r}, expected {want[first]!r}")
            )
    return stale


def write_lexicon(assets, tag, words):
    """Rewrite `lexicons/<tag>.txt` as `words`, one per line, LF, trailing newline."""
    path = os.path.join(assets, "lexicons", f"{tag}.txt")
    with open(path, "w", encoding="utf-8", newline="\n") as fh:
        fh.write("".join(word + "\n" for word in words))


def regenerate(assets):
    """Re-order every lexicon in place. Returns the tags actually rewritten."""
    changed = []
    for tag in lexicon_tags(assets):
        path = os.path.join(assets, "lexicons", f"{tag}.txt")
        current = read_words(path)
        want = desired(assets, tag)
        if want is None:
            print(f"order_lexicons: {tag}: no freq/{tag}.txt — left untouched")
            continue
        # The set is curated elsewhere; this tool only reorders. Refuse to write
        # anything that would add or drop a word.
        if set(want) != set(current):
            raise AssertionError(
                f"{tag}: word set would change ({len(set(current))} -> {len(set(want))})"
            )
        if current != want:
            write_lexicon(assets, tag, want)
            changed.append(tag)
    return changed


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument(
        "--check", action="store_true",
        help="exit 1 if any lexicon is not in frequency order (CI gate)",
    )
    parser.add_argument(
        "--assets", default=DEFAULT_ASSETS,
        help="Android asset root (defaults to the one in this repository)",
    )
    args = parser.parse_args(argv)

    if args.check:
        stale = stale_lexicons(args.assets)
        if stale:
            print("order_lexicons: lexicons are not in frequency order:")
            for tag, reason in stale:
                print(f"  lexicons/{tag}.txt — {reason}")
            print("  fix: python3 core/tools/order_lexicons.py")
            return 1
        print("order_lexicons: every bundled lexicon is in frequency order")
        return 0

    changed = regenerate(args.assets)
    print(
        f"order_lexicons: reordered {', '.join(changed)}"
        if changed
        else "order_lexicons: already in frequency order — nothing to do"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
