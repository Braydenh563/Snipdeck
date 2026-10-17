//! Scenario: nucleo fuzzy search over 10k items of ~200 chars (no UI).

use crate::gen;
use crate::harness::{rss_mb, stats, time, Report};
use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32Str, Utf32String};
use std::sync::Arc;
use std::time::Instant;

pub const QUERIES: &[&str] = &["s", "sn", "snip", "json", "deploy rel", "kbrnts", "zzqx"];

/// Haystack for one item: title + body, as the app would index it.
pub fn haystack(it: &gen::Item) -> String {
    format!("{} {}", it.title, it.body)
}

/// Scores every haystack and returns (index, score) sorted best-first.
pub fn rank(pattern: &Pattern, hay: &[Utf32String], matcher: &mut Matcher) -> Vec<(u32, u32)> {
    let mut out: Vec<(u32, u32)> = hay
        .iter()
        .enumerate()
        .filter_map(|(i, h)| {
            pattern
                .score(h.slice(..), matcher)
                .map(|s| (i as u32, s))
        })
        .collect();
    out.sort_unstable_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    out
}

/// Same as `rank`, split over `threads` scoped threads (one Matcher each).
pub fn rank_par(q: &str, hay: &[Utf32String], threads: usize) -> Vec<(u32, u32)> {
    let chunk = hay.len().div_ceil(threads);
    let mut out: Vec<(u32, u32)> = std::thread::scope(|sc| {
        let handles: Vec<_> = hay
            .chunks(chunk)
            .enumerate()
            .map(|(c, part)| {
                sc.spawn(move || {
                    let mut m = Matcher::new(Config::DEFAULT);
                    let p = Pattern::parse(q, CaseMatching::Smart, Normalization::Smart);
                    part.iter()
                        .enumerate()
                        .filter_map(|(i, h)| {
                            p.score(h.slice(..), &mut m)
                                .map(|s| ((c * chunk + i) as u32, s))
                        })
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        handles
            .into_iter()
            .flat_map(|h| h.join().unwrap())
            .collect()
    });
    out.sort_unstable_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    out
}

fn bench(iters: usize, mut f: impl FnMut() -> usize) -> (crate::harness::Stats, usize) {
    let mut v = Vec::new();
    let mut hits = 0;
    for _ in 0..iters {
        let (h, d) = time(&mut f);
        hits = h;
        v.push(d);
    }
    (stats(&v), hits)
}

pub fn run(n: usize) {
    let items = gen::items(n, 7);
    let strs: Vec<String> = items.iter().map(haystack).collect();
    let avg = strs.iter().map(|s| s.len()).sum::<usize>() as f64 / n as f64;
    let (rss0, _) = rss_mb();
    let (hay, t_conv) = time(|| {
        strs.iter()
            .map(|s| Utf32String::from(s.as_str()))
            .collect::<Vec<_>>()
    });
    let (rss1, _) = rss_mb();
    let short: Vec<Utf32String> = items
        .iter()
        .map(|it| {
            let body: String = it.body.chars().take(64).collect();
            Utf32String::from(format!("{} {}", it.title, body).as_str())
        })
        .collect();
    let mut matcher = Matcher::new(Config::DEFAULT);
    let iters = 40;

    for q in QUERIES {
        let mut r = Report::new("search", format!("{n} items, avg {avg:.0} chars, q=\"{q}\""));
        // (a) cached Utf32String haystacks, single thread, score + full sort
        let mut a = Vec::new();
        let mut hits = 0;
        for _ in 0..iters {
            let (res, d) = time(|| {
                let p = Pattern::parse(q, CaseMatching::Smart, Normalization::Smart);
                rank(&p, &hay, &mut matcher)
            });
            hits = res.len();
            a.push(d);
        }
        let sa = stats(&a);
        r.f("hits", hits as f64);
        r.f("cached_med", sa.median);
        r.f("cached_p95", sa.p95);
        r.f("cached_max", sa.max);
        // (b) converting &str on the fly (no cache), single thread
        let mut b = Vec::new();
        let mut buf = Vec::new();
        for _ in 0..iters {
            let ((), d) = time(|| {
                let p = Pattern::parse(q, CaseMatching::Smart, Normalization::Smart);
                let mut out: Vec<(usize, u32)> = strs
                    .iter()
                    .enumerate()
                    .filter_map(|(i, s)| {
                        p.score(Utf32Str::new(s, &mut buf), &mut matcher)
                            .map(|sc| (i, sc))
                    })
                    .collect();
                out.sort_unstable_by(|x, y| y.1.cmp(&x.1));
                std::hint::black_box(out);
            });
            b.push(d);
        }
        let sb = stats(&b);
        r.f("onthefly_med", sb.median);
        r.f("onthefly_max", sb.max);
        // (c) 4 scoped threads, full haystack
        let (sp, _) = bench(iters, || rank_par(q, &hay, 4).len());
        r.f("par4_med", sp.median);
        r.f("par4_max", sp.max);
        // (d) truncated haystack (title + first 64 chars of body), single thread
        let (st, th) = bench(iters, || {
            let p = Pattern::parse(q, CaseMatching::Smart, Normalization::Smart);
            rank(&p, &short, &mut matcher).len()
        });
        r.f("trunc_hits", th as f64);
        r.f("trunc_med", st.median);
        r.f("trunc_max", st.max);
        // (e) truncated + 4 threads
        let (spt, _) = bench(iters, || rank_par(q, &short, 4).len());
        r.f("trunc_par4_med", spt.median);
        r.f("trunc_par4_max", spt.max);
        r.emit();
    }

    // (c) nucleo::Nucleo worker pool (as in helix): time from reparse to finished snapshot,
    //     typing "deploy" one char at a time with append=true.
    let mut nuc: nucleo::Nucleo<u32> =
        nucleo::Nucleo::new(nucleo::Config::DEFAULT, Arc::new(|| {}), None, 1);
    let inj = nuc.injector();
    let ((), t_inject) = time(|| {
        for (i, s) in strs.iter().enumerate() {
            inj.push(i as u32, |_, cols| cols[0] = s.as_str().into());
        }
    });
    // let the pool pick up the items
    while nuc.tick(10).running {}
    let mut per_key = Vec::new();
    let typed = "deploy";
    for k in 1..=typed.len() {
        let t = Instant::now();
        nuc.pattern.reparse(
            0,
            &typed[..k],
            nucleo::pattern::CaseMatching::Smart,
            nucleo::pattern::Normalization::Smart,
            true,
        );
        while nuc.tick(10).running {}
        let cnt = nuc.snapshot().matched_item_count();
        std::hint::black_box(cnt);
        per_key.push(t.elapsed());
    }
    let s = stats(&per_key);
    let mut r = Report::new("search", format!("{n} items, nucleo::Nucleo pool, typing \"deploy\""));
    r.d("inject_10k", t_inject);
    r.f("per_key_med", s.median);
    r.f("per_key_max", s.max);
    r.f("threads", std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1) as f64);
    r.emit();

    let mut r = Report::new("search", format!("{n} items: index cost"));
    r.d("utf32_convert", t_conv);
    r.f("utf32_rss_delta_mb", rss1 - rss0);
    r.emit();
}
