//! The SHARK²-style path scorer — a verbatim port of Kotlin
//! `GestureDecoder.decode` / `resampleInto` / `normalizeInto` / `avgKeyStep`.
//! Every magic constant and the scoring formula are copied from the Kotlin so the
//! two stay bit-for-bit comparable (the bounded twin — see the crate README).

use std::collections::HashMap;

use featherkey_fold::fold_char;

use crate::GestureIndex;

/// One point of a swipe path (or a key centre), in the caller's coordinate space.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

// Constants copied verbatim from GestureDecoder.kt.
const SAMPLES: usize = 24;
const SHAPE_WEIGHT: f32 = 0.3;
const LEARNED_BOOST: f32 = 0.55;
const FREQ_MIN: f32 = 0.70;
const FREQ_SPAN: f32 = 8000.0;
const MAX_KEYS: usize = 48;

/// The frequency discount for a word at 0-based `rank` (lower discount = better).
/// `u32::MAX` marks an unranked word (no discount). Mirrors the Kotlin `when` arm.
pub(crate) fn freq_discount(rank: u32) -> f32 {
    if rank == u32::MAX {
        1.0
    } else {
        FREQ_MIN + (1.0 - FREQ_MIN) * (rank as f32 / FREQ_SPAN).min(1.0)
    }
}

/// Arc-length resample `points` to `SAMPLES` evenly spaced points. `None` for a
/// degenerate (fewer than two points, or zero-length) path. Mirrors `resampleInto`.
fn resample(points: &[Point]) -> Option<[Point; SAMPLES]> {
    let n = points.len();
    if n < 2 {
        return None;
    }
    let mut cum = vec![0.0f32; n];
    let mut total = 0.0;
    for i in 1..n {
        total += (points[i].x - points[i - 1].x).hypot(points[i].y - points[i - 1].y);
        cum[i] = total;
    }
    if total <= 1e-3 {
        return None;
    }
    let mut out = [Point { x: 0.0, y: 0.0 }; SAMPLES];
    out[0] = points[0];
    let step_len = total / (SAMPLES as f32 - 1.0);
    let mut seg = 1usize;
    for (k, out_pt) in out.iter_mut().enumerate().skip(1).take(SAMPLES - 2) {
        let target = step_len * k as f32;
        while seg < n - 1 && cum[seg] < target {
            seg += 1;
        }
        let (seg_start, seg_end) = (cum[seg - 1], cum[seg]);
        let t = if seg_end > seg_start {
            (target - seg_start) / (seg_end - seg_start)
        } else {
            0.0
        };
        *out_pt = Point {
            x: points[seg - 1].x + (points[seg].x - points[seg - 1].x) * t,
            y: points[seg - 1].y + (points[seg].y - points[seg - 1].y) * t,
        };
    }
    out[SAMPLES - 1] = points[n - 1];
    Some(out)
}

/// Centre and scale-normalise (subtract centroid, divide by RMS radius) so an offset
/// or larger/smaller path of the same shape matches. Mirrors `normalizeInto`.
fn normalize(pts: &[Point; SAMPLES]) -> [Point; SAMPLES] {
    let n = SAMPLES as f32;
    let (mut cx, mut cy) = (0.0, 0.0);
    for p in pts {
        cx += p.x;
        cy += p.y;
    }
    cx /= n;
    cy /= n;
    let mut rms = 0.0;
    for p in pts {
        let (dx, dy) = (p.x - cx, p.y - cy);
        rms += dx * dx + dy * dy;
    }
    rms = (rms / n).sqrt();
    if rms < 1e-3 {
        rms = 1.0;
    }
    let mut out = [Point { x: 0.0, y: 0.0 }; SAMPLES];
    for (o, p) in out.iter_mut().zip(pts.iter()) {
        *o = Point {
            x: (p.x - cx) / rms,
            y: (p.y - cy) / rms,
        };
    }
    out
}

/// Average nearest-neighbour distance between key centres (~one key pitch).
fn avg_key_step(centers: &HashMap<char, Point>) -> f32 {
    let list: Vec<Point> = centers.values().copied().collect();
    if list.len() < 2 {
        return 100.0;
    }
    // With ≥ 2 centres every point has a neighbour, so each `nearest` is a real
    // distance; average them for ~one key pitch.
    let mut sum = 0.0;
    for (i, a) in list.iter().enumerate() {
        let mut nearest = f32::MAX;
        for (j, b) in list.iter().enumerate() {
            if i != j {
                nearest = nearest.min((a.x - b.x).hypot(a.y - b.y));
            }
        }
        sum += nearest;
    }
    sum / list.len() as f32
}

/// The candidate word's ideal path folded into `poly` (accents → base key,
/// non-key characters dropped). `None` if it overflows `MAX_KEYS` or has < 2 keys.
fn word_poly(word: &str, centers: &HashMap<char, Point>) -> Option<Vec<Point>> {
    let mut poly = Vec::with_capacity(MAX_KEYS);
    for ch in word.chars() {
        if let Some(c) = centers.get(&fold_char(ch)) {
            if poly.len() >= MAX_KEYS {
                return None;
            }
            poly.push(*c);
        }
    }
    if poly.len() < 2 {
        None
    } else {
        Some(poly)
    }
}

/// The blended location+shape distance of a candidate's ideal `poly` against the
/// already-resampled `gesture`. `None` if the poly resamples degenerate. This is the
/// SHARK² two-channel score before the frequency/learned discount.
fn geometric_base(
    gesture: &[Point; SAMPLES],
    n_gesture: &[Point; SAMPLES],
    poly: &[Point],
    step: f32,
) -> Option<f32> {
    let ideal = resample(poly)?;
    let n_ideal = normalize(&ideal);
    let (mut loc, mut shape) = (0.0f32, 0.0f32);
    for i in 0..SAMPLES {
        loc += (gesture[i].x - ideal[i].x).hypot(gesture[i].y - ideal[i].y);
        shape += (n_gesture[i].x - n_ideal[i].x).hypot(n_gesture[i].y - n_ideal[i].y);
    }
    Some(loc / SAMPLES as f32 + SHAPE_WEIGHT * step * (shape / SAMPLES as f32))
}

/// Best-matching words for `path`, most likely first (empty if not a gesture).
/// `path` and `centers` must be in the same coordinate space. `rank_of` returns a
/// word's 0-based frequency rank (`u32::MAX` = unranked); `learned` maps a word to
/// its learned frequency (presence alone applies `LEARNED_BOOST`).
pub fn decode(
    path: &[Point],
    centers: &HashMap<char, Point>,
    index: &GestureIndex,
    rank_of: impl Fn(&str) -> u32,
    learned: &HashMap<String, u32>,
    limit: usize,
) -> Vec<String> {
    if path.len() < 3 || centers.is_empty() {
        return Vec::new();
    }
    let step = avg_key_step(centers);
    let prune_r = step * 1.7;
    let gesture = match resample(path) {
        Some(g) => g,
        None => return Vec::new(),
    };
    let n_gesture = normalize(&gesture);
    let (start, end) = (gesture[0], gesture[SAMPLES - 1]);

    // Only words whose first key lies within the prune radius of the gesture start
    // can match, so scan just those buckets rather than every word.
    let mut scored: Vec<(String, f32)> = Vec::new();
    for (first_key, first_c) in centers {
        if (start.x - first_c.x).hypot(start.y - first_c.y) > prune_r {
            continue;
        }
        for entry in index.bucket(*first_key) {
            let last_c = match centers.get(&entry.last) {
                Some(c) => c,
                None => continue,
            };
            if (end.x - last_c.x).hypot(end.y - last_c.y) > prune_r {
                continue;
            }
            let poly = match word_poly(&entry.word, centers) {
                Some(p) => p,
                None => continue,
            };
            let base = match geometric_base(&gesture, &n_gesture, &poly, step) {
                Some(b) => b,
                None => continue,
            };
            let discount = if learned.contains_key(&entry.word) {
                LEARNED_BOOST
            } else {
                freq_discount(rank_of(&entry.word))
            };
            scored.push((entry.word.clone(), base * discount));
        }
    }
    top_unique(scored, limit)
}

/// Sort by ascending score (best first, ties broken by word for determinism), then
/// take the first `limit` distinct words.
fn top_unique(mut scored: Vec<(String, f32)>, limit: usize) -> Vec<String> {
    scored.sort_by(|a, b| {
        a.1.partial_cmp(&b.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.0.cmp(&b.0))
    });
    let mut out: Vec<String> = Vec::with_capacity(limit);
    for (word, _) in scored {
        if !out.contains(&word) {
            out.push(word);
        }
        if out.len() >= limit {
            break;
        }
    }
    out
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::key_path;

    /// A QWERTY a–z grid (unit spacing, rows staggered) — enough for path fixtures.
    fn qwerty() -> HashMap<char, Point> {
        let rows = [
            ("qwertyuiop", 0.0f32, 0.0f32),
            ("asdfghjkl", 0.5, 1.0),
            ("zxcvbnm", 1.5, 2.0),
        ];
        let mut m = HashMap::new();
        for (letters, x0, y) in rows {
            for (i, ch) in letters.chars().enumerate() {
                m.insert(
                    ch,
                    Point {
                        x: x0 + i as f32,
                        y,
                    },
                );
            }
        }
        m
    }

    /// The polyline through a word's key centres (the ideal path of a perfect swipe).
    fn trace(centers: &HashMap<char, Point>, word: &str) -> Vec<Point> {
        key_path(word, |c| centers.contains_key(&c))
            .into_iter()
            .filter_map(|c| centers.get(&c).copied())
            .collect()
    }

    /// A symmetric b/(a,e) layout: a and e mirror across x=0, so a gesture straight
    /// down from b is geometrically equidistant to "ba" and "be" — a clean tie.
    fn symmetric() -> HashMap<char, Point> {
        HashMap::from([
            ('b', Point { x: 0.0, y: 0.0 }),
            ('a', Point { x: -1.0, y: 1.0 }),
            ('e', Point { x: 1.0, y: 1.0 }),
        ])
    }

    fn straight_down() -> Vec<Point> {
        vec![
            Point { x: 0.0, y: 0.0 },
            Point { x: 0.0, y: 0.5 },
            Point { x: 0.0, y: 1.0 },
        ]
    }

    #[test]
    fn a_swipe_over_the_letters_decodes_to_that_word() {
        let centers = qwerty();
        let idx = GestureIndex::build(&["hello", "help", "hero", "world"]);
        let path = trace(&centers, "hello");
        let out = decode(&path, &centers, &idx, |_| u32::MAX, &HashMap::new(), 4);
        assert_eq!(out.first().map(String::as_str), Some("hello"));
    }

    #[test]
    fn too_few_points_is_not_a_gesture() {
        let centers = qwerty();
        let idx = GestureIndex::build(&["hello"]);
        let two = vec![Point { x: 0.0, y: 0.0 }, Point { x: 1.0, y: 0.0 }];
        assert!(decode(&two, &centers, &idx, |_| u32::MAX, &HashMap::new(), 4).is_empty());
    }

    #[test]
    fn no_centers_is_not_a_gesture() {
        let idx = GestureIndex::build(&["hello"]);
        let path = vec![
            Point { x: 0.0, y: 0.0 },
            Point { x: 1.0, y: 0.0 },
            Point { x: 2.0, y: 0.0 },
        ];
        assert!(decode(
            &path,
            &HashMap::new(),
            &idx,
            |_| u32::MAX,
            &HashMap::new(),
            4
        )
        .is_empty());
    }

    #[test]
    fn results_are_deduped_and_capped_at_the_limit() {
        let centers = qwerty();
        let idx = GestureIndex::build(&["hello", "hello", "help", "hero"]);
        let path = trace(&centers, "hello");
        let out = decode(&path, &centers, &idx, |_| u32::MAX, &HashMap::new(), 2);
        assert_eq!(out.len(), 2);
        assert_eq!(out.first().map(String::as_str), Some("hello"));
        assert_eq!(out.iter().filter(|w| *w == "hello").count(), 1); // deduped
    }

    #[test]
    fn a_learned_word_wins_a_geometric_tie() {
        let centers = symmetric();
        let idx = GestureIndex::build(&["ba", "be"]);
        let path = straight_down();
        let none = HashMap::new();
        // No signal → deterministic word order breaks the tie ("ba" < "be").
        assert_eq!(
            decode(&path, &centers, &idx, |_| u32::MAX, &none, 2),
            vec!["ba".to_string(), "be".to_string()]
        );
        // "be" learned → it wins the tie.
        let be_learned = HashMap::from([("be".to_string(), 1u32)]);
        assert_eq!(
            decode(&path, &centers, &idx, |_| u32::MAX, &be_learned, 1),
            vec!["be".to_string()]
        );
        // "ba" learned → it wins instead.
        let ba_learned = HashMap::from([("ba".to_string(), 1u32)]);
        assert_eq!(
            decode(&path, &centers, &idx, |_| u32::MAX, &ba_learned, 1),
            vec!["ba".to_string()]
        );
    }

    #[test]
    fn a_more_frequent_word_wins_a_geometric_tie() {
        let centers = symmetric();
        let idx = GestureIndex::build(&["ba", "be"]);
        let path = straight_down();
        let none = HashMap::new();
        // "be" ranks best (0 → 0.70 discount), "ba" unranked (1.0) → "be" wins.
        let rank = |w: &str| if w == "be" { 0 } else { u32::MAX };
        assert_eq!(
            decode(&path, &centers, &idx, rank, &none, 1),
            vec!["be".to_string()]
        );
    }

    #[test]
    fn freq_discount_matches_the_kotlin_curve() {
        assert!((freq_discount(0) - 0.70).abs() < 1e-6); // most common
        assert!((freq_discount(4000) - 0.85).abs() < 1e-6); // FREQ_MIN + 0.30*0.5
        assert!((freq_discount(8000) - 1.0).abs() < 1e-6); // clamped at the span
        assert!((freq_discount(20000) - 1.0).abs() < 1e-6); // clamp holds past span
        assert!((freq_discount(u32::MAX) - 1.0).abs() < 1e-6); // unranked
    }
}
