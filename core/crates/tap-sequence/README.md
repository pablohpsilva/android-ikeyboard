# featherkey-tap-sequence

**Its one job:** decide which real words a *sequence* of ambiguous taps explains.

`input-decoder` answers "which key was this tap?" — one tap, one answer. This
crate answers the question that only makes sense across a whole word: given that
every tap was a *distribution* over nearby keys, which dictionary words do those
taps jointly explain, and how well? That is what lets a first tap committed as
`r` be revised once `h` and `e` arrive, so `rhe` can still reach `the` — a repair
no per-tap decision and no edit-distance-1 corrector can make.

It runs a bounded beam search: expand each surviving prefix by the tap's most
likely keys, prune every prefix no real word continues, keep the best `BEAM`, and
complete the survivors.

**What it deliberately does not know:** word frequency, the user's learned words,
next-word context, or language momentum. Those live in `prediction`,
`personalization`, `context` and `candidate-ranker`; duplicating them here would
be a second ranking model. This crate reports *spatial fit only* — how well the
taps explain a word — and the caller combines that with everything else.

It reaches the lexicon through the `Lexicon` trait, so it depends on no
dictionary implementation and is testable against a `BTreeSet`.
