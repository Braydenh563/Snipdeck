#![allow(
    clippy::single_range_in_vec_init,
    reason = "a mark is a list of ranges; one range is the common answer"
)]

use std::time::{Duration, Instant};

use proptest::prelude::*;

use super::*;
use crate::{ItemKind, ListId};

fn item(title: &str, preview: &str) -> Item {
    let now = "2026-10-17T14:05:09Z".parse().unwrap();
    let mut item = Item::new(ListId::generate(), title.to_owned(), ItemKind::Text, now);
    item.preview = preview.to_owned();
    item
}

fn tags<'a>(names: &'a [&'a str]) -> Names<'a> {
    Names {
        tags: names,
        ..Names::default()
    }
}

/// The index's answer for typed text, with no full-text answers.
fn ask(index: &SearchIndex, query: &str, limit: usize) -> Vec<Hit> {
    index.query(&Query::parse(query), &BodyMatches::default(), limit)
}

fn titles(index: &SearchIndex, items: &[Item], query: &str, limit: usize) -> Vec<String> {
    ask(index, query, limit)
        .iter()
        .map(|hit| items.iter().find(|i| i.id == hit.id).unwrap().title.clone())
        .collect()
}

fn sample() -> Vec<Item> {
    vec![
        item("Email signature", "Best regards,\nAlex"),
        item("Meeting link", "https://meet.example.com/standup"),
        item(
            "Invoice reminder",
            "Hi, a friendly reminder that invoice 1042 is due",
        ),
        item("Café menu", "Flat white, cortado"),
    ]
}

#[test]
fn empty_query_returns_items_in_given_order_up_to_the_limit() {
    let items = sample();
    let index = SearchIndex::new(&items);
    assert_eq!(index.len(), 4);
    let all = titles(&index, &items, "", 100);
    assert_eq!(
        all,
        [
            "Email signature",
            "Meeting link",
            "Invoice reminder",
            "Café menu"
        ]
    );
    assert_eq!(titles(&index, &items, "", 2), all[..2]);
    let hit = &ask(&index, "", 1)[0];
    assert_eq!((hit.score, hit.tag.is_none()), (0, true));
}

#[test]
fn a_blank_query_counts_as_empty() {
    let items = sample();
    let index = SearchIndex::new(&items);
    assert_eq!(ask(&index, "  \t ", 10).len(), 4);
}

#[test]
fn limit_zero_and_no_match_give_nothing() {
    let items = sample();
    let index = SearchIndex::new(&items);
    assert_eq!(ask(&index, "", 0).len(), 0);
    assert_eq!(ask(&index, "sig", 0).len(), 0);
    assert_eq!(ask(&index, "qqqqzzzz", 10).len(), 0);
    assert_eq!(ask(&SearchIndex::default(), "a", 10).len(), 0);
}

#[test]
fn marks_come_from_the_same_matcher_in_character_positions() {
    let q = |text: &str| Query::parse(text);
    assert_eq!(locate("Email signature", &q("signature"), true), [6..15]);
    // LIBRARY-6: multi-word, subsequence and folded matches are all marked.
    assert_eq!(locate("Email signature", &q("emsig"), true), [0..2, 6..9]);
    assert_eq!(locate("Email signature", &q("emsig"), false), []);
    assert_eq!(locate("Café menu", &q("cafe"), true), [0..4]);
    assert_eq!(locate("STRASSE", &q("straße"), true), [0..7]);
    assert_eq!(locate("Straße 5", &q("strasse"), true), [0..6]);
    assert_eq!(
        locate("Invoice for work", &q("work invoice"), true),
        [0..7, 12..16]
    );
    assert_eq!(locate("ｃａｆｅ au lait", &q("CAFE"), true), [0..4]);
    assert_eq!(
        locate("これは日本語のテキストです", &q("テキスト"), false),
        [7..11]
    );
    assert_eq!(
        locate("abc", &q("-abc"), true),
        [],
        "a negated word marks nothing"
    );
}

#[test]
fn matching_ignores_case_accents_and_width() {
    let items = vec![item("Café résumé", ""), item("Email signature", "")];
    let index = SearchIndex::new(&items);
    // LIBRARY-51: case never hides an item.
    for q in ["cafe resume", "RESUME", "Café", "EMAIL", "eMaIl SiG"] {
        assert_eq!(ask(&index, q, 10).len(), 1, "{q}");
    }
    // LIBRARY-52: full-width input from an IME, and a decomposed accent.
    assert_eq!(ask(&index, "ｃａｆｅ", 10)[0].id, items[0].id);
    assert_eq!(ask(&index, "cafe\u{301}", 10)[0].id, items[0].id);
}

#[test]
fn every_word_of_the_query_must_match() {
    let items = sample();
    let index = SearchIndex::new(&items);
    assert_eq!(titles(&index, &items, "mail sig", 10), ["Email signature"]);
    assert_eq!(ask(&index, "mail zzzz", 10).len(), 0);
}

#[test]
fn title_outranks_tag_outranks_content() {
    let content = item("Zeta", "invoice reminder text");
    let tagged = item("Alpha", "nothing here");
    let titled = item("Invoice", "");
    let mut index = SearchIndex::default();
    index.upsert(&content, Names::default());
    index.upsert(&tagged, tags(&["billing", "invoice"]));
    index.upsert(&titled, Names::default());
    let ids: Vec<_> = ask(&index, "invoice", 10).iter().map(|h| h.id).collect();
    assert_eq!(ids, [titled.id, tagged.id, content.id]);
}

/// LIBRARY-5: match quality first, field second. A scattered subsequence in a title loses to an
/// exact tag and to content that starts with the word.
#[test]
fn match_quality_outranks_the_field() {
    let scattered = item("Shipping instructions guide", "");
    let tagged = item("Email footer", "");
    let content = item("Other", "sig block for replies");
    let mut index = SearchIndex::default();
    index.upsert(&scattered, Names::default());
    index.upsert(&tagged, tags(&["sig"]));
    index.upsert(&content, Names::default());
    let ids: Vec<_> = ask(&index, "sig", 10).iter().map(|h| h.id).collect();
    assert_eq!(ids, [tagged.id, content.id, scattered.id]);
}

/// LIBRARY-9: the trigger is a field, and an exact trigger is the first hit.
#[test]
fn an_exact_trigger_is_the_first_hit() {
    let titled = item(";sig", "");
    let triggered = item("Email signature", "Best regards");
    let mut index = SearchIndex::default();
    index.upsert(&titled, Names::default());
    index.upsert(
        &triggered,
        Names {
            trigger: Some(";sig"),
            ..Names::default()
        },
    );
    let ids: Vec<_> = ask(&index, ";sig", 10).iter().map(|h| h.id).collect();
    assert_eq!(ids, [triggered.id, titled.id]);
    assert_eq!(
        ask(&index, ";SI", 10).len(),
        2,
        "a prefix of a trigger matches too"
    );
}

/// LIBRARY-4 and LIBRARY-1: each word may match a different field, lists included.
#[test]
fn each_word_matches_in_any_field() {
    let a = item("Invoice", "Dear customer, please find attached");
    let b = item("Thanks", "");
    let mut index = SearchIndex::default();
    index.upsert(
        &a,
        Names {
            tags: &["work"],
            list: "Billing",
            trigger: None,
        },
    );
    index.upsert(
        &b,
        Names {
            list: "Email replies",
            ..Names::default()
        },
    );
    for q in ["invoice work", "invoice customer", "work customer billing"] {
        let ids: Vec<_> = ask(&index, q, 10).iter().map(|h| h.id).collect();
        assert_eq!(ids, [a.id], "{q}");
    }
    assert_eq!(ask(&index, "email replies", 10)[0].id, b.id);
    assert_eq!(
        ask(&index, "invoice work", 10)[0].tag.as_deref(),
        Some("work")
    );
}

/// LIBRARY-3: the full-text answer counts as a match of that word, in the weakest field, and
/// negation and filters still apply to it.
#[test]
fn body_matches_from_the_store_join_the_in_memory_fields() {
    let a = item("Invoice", "short preview");
    let b = item("Other", "");
    let mut index = SearchIndex::default();
    index.upsert(&a, tags(&["work"]));
    index.upsert(&b, Names::default());
    let query = Query::parse("invoice attached -zzz");
    let body = BodyMatches::new(vec![
        None,
        Some([a.id, b.id].into_iter().collect()),
        Some(HashSet::new()),
    ]);
    let hits = index.query(&query, &body, 10);
    assert_eq!(hits.iter().map(|h| h.id).collect::<Vec<_>>(), [a.id]);
    let body = BodyMatches::new(vec![
        None,
        Some([a.id].into_iter().collect()),
        Some([a.id].into_iter().collect()),
    ]);
    assert!(
        index.query(&query, &body, 10).is_empty(),
        "the negated word is in the body"
    );
}

/// LIBRARY-8: phrases, negation and filters.
#[test]
fn operators_filter_the_results() {
    let mut sig = item("Signature", "Kind regards, Alex");
    sig.pinned = true;
    let mut link = item("Docs", "https://example.com/kind");
    link.kind = ItemKind::Link;
    let plus = item("C++ tips", "use std::vector");
    let cats = item("Cats", "");
    let mut index = SearchIndex::default();
    index.upsert(
        &sig,
        Names {
            tags: &["Work mail"],
            list: "Email replies",
            trigger: Some(";sig"),
        },
    );
    index.upsert(&link, tags(&["old"]));
    index.upsert(&plus, Names::default());
    index.upsert(&cats, Names::default());
    let ids = |q: &str| -> Vec<ItemId> { ask(&index, q, 10).iter().map(|h| h.id).collect() };
    assert_eq!(ids("\"kind regards\""), [sig.id]);
    assert_eq!(ids("\"regards kind\""), []);
    assert_eq!(ids("kind -regards"), [link.id]);
    assert_eq!(
        ids("c++"),
        [plus.id],
        "punctuation is searched, not dropped"
    );
    assert_eq!(ids("std::vector"), [plus.id]);
    assert_eq!(ids("tag:work"), [sig.id]);
    assert_eq!(ids("tag:\"work mail\""), [sig.id]);
    assert_eq!(ids("-tag:old kind"), [sig.id]);
    assert_eq!(ids("list:email"), [sig.id]);
    assert_eq!(ids("kind:link"), [link.id]);
    assert_eq!(ids("is:pinned"), [sig.id]);
    assert_eq!(ids("-is:pinned kind"), [link.id]);
    assert_eq!(ids("tag:"), ids(""), "a filter being typed changes nothing");
}

#[test]
fn a_tag_hit_names_the_matching_tag_and_other_hits_name_none() {
    let tagged = item("Alpha", "");
    let titled = item("Urgent", "");
    let mut index = SearchIndex::default();
    index.upsert(&tagged, tags(&["billing", "Urgent"]));
    index.upsert(&titled, tags(&["urgentish"]));
    let hits = ask(&index, "urgent", 10);
    let tag_of = |id| hits.iter().find(|h| h.id == id).map(|h| h.tag.clone());
    assert_eq!(
        tag_of(tagged.id),
        Some(Some("Urgent".to_owned())),
        "as written"
    );
    assert_eq!(
        tag_of(titled.id),
        Some(None),
        "the exact title explains the word better than the tag"
    );
    assert!(ask(&index, "", 10).iter().all(|h| h.tag.is_none()));
}

#[test]
fn equal_scores_keep_the_given_order() {
    let items: Vec<Item> = (0..5).map(|_| item("Reply", "")).collect();
    let index = SearchIndex::new(&items);
    let ids: Vec<_> = ask(&index, "reply", 10).iter().map(|h| h.id).collect();
    assert_eq!(ids, items.iter().map(|i| i.id).collect::<Vec<_>>());
}

fn used(mut item: Item, count: u32, last: Option<&str>) -> Item {
    item.use_count = count;
    item.last_used_at = last.map(|at| at.parse().unwrap());
    item
}

#[test]
fn equal_scores_go_to_the_more_used_item_then_to_the_one_used_last() {
    let items = vec![
        item("Reply", ""),
        used(item("Reply", ""), 2, Some("2026-10-01T09:00:00Z")),
        used(item("Reply", ""), 9, Some("2026-09-01T09:00:00Z")),
        used(item("Reply", ""), 2, Some("2026-10-10T09:00:00Z")),
        item("Reply", ""),
    ];
    let index = SearchIndex::new(&items);
    let ids: Vec<_> = ask(&index, "reply", 10).iter().map(|h| h.id).collect();
    // Nine uses first; the two with two uses by last use; the unused two in the user's order.
    let want = [2, 3, 1, 0, 4].map(|i| items[i].id);
    assert_eq!(ids, want);
}

#[test]
fn use_never_beats_a_better_match_and_decides_a_cut_at_the_limit() {
    let items = vec![
        used(
            item("Reply to customer", ""),
            50,
            Some("2026-10-10T09:00:00Z"),
        ),
        item("Reply", ""),
        used(item("Reply", ""), 3, None),
        item("Reply", ""),
    ];
    let index = SearchIndex::new(&items);
    let hits = ask(&index, "reply", 10);
    let exact_first = hits.iter().position(|h| h.id == items[0].id).unwrap();
    assert!(
        hits[..exact_first]
            .iter()
            .all(|h| h.score > hits[exact_first].score),
        "a heavily used worse match stays behind every better one"
    );
    // Cut to two: the used one of the three equal titles survives the cut.
    let two: Vec<_> = ask(&index, "reply", 2).iter().map(|h| h.id).collect();
    assert!(two.contains(&items[2].id), "{two:?}");
}

#[test]
fn a_counted_use_reorders_a_tie_without_rebuilding_the_index() {
    let mut items: Vec<Item> = (0..3).map(|_| item("Reply", "")).collect();
    let mut index = SearchIndex::new(&items);
    assert_eq!(ask(&index, "reply", 1)[0].id, items[0].id);
    items[2] = used(items[2].clone(), 1, Some("2026-10-18T09:00:00Z"));
    assert!(index.refresh_usage(&items));
    assert!(!index.refresh_usage(&items), "nothing more to change");
    assert_eq!(ask(&index, "reply", 1)[0].id, items[2].id);
    assert_eq!(
        ask(&index, "", 1)[0].id,
        items[0].id,
        "a blank query keeps the list order"
    );
}

#[test]
fn content_matching_covers_only_the_first_64_characters() {
    let inside = item("A", &format!("{}needle", "x".repeat(58)));
    let outside = item("B", &format!("{}needle", "x".repeat(59)));
    let accents = item("C", &format!("{}zzzz", "é".repeat(60)));
    let index = SearchIndex::new([&inside, &outside, &accents]);
    let ids: Vec<_> = ask(&index, "needle", 10).iter().map(|h| h.id).collect();
    assert_eq!(ids, [inside.id]);
    let ids: Vec<_> = ask(&index, "zzzz", 10).iter().map(|h| h.id).collect();
    assert_eq!(ids, [accents.id]);
}

#[test]
fn upsert_replaces_in_place() {
    let mut items = sample();
    let mut index = SearchIndex::new(&items);
    items[1].title = "Standup link".to_owned();
    index.upsert(&items[1], Names::default());
    assert_eq!(index.len(), 4);
    assert_eq!(ask(&index, "meeting", 10).len(), 0);
    assert_eq!(titles(&index, &items, "", 10)[1], "Standup link");
}

#[test]
fn upsert_replaces_tags_too() {
    let a = item("Alpha", "");
    let mut index = SearchIndex::default();
    index.upsert(&a, tags(&["urgent"]));
    index.upsert(&a, tags(&["billing"]));
    assert_eq!(ask(&index, "urgent", 10).len(), 0);
    assert_eq!(ask(&index, "billing", 10).len(), 1);
    index.upsert(&a, Names::default());
    assert_eq!(ask(&index, "billing", 10).len(), 0);
}

#[test]
fn remove_drops_the_item_and_keeps_the_rest_in_order() {
    let items = sample();
    let mut index = SearchIndex::new(&items);
    assert!(index.remove(items[1].id));
    assert!(!index.remove(items[1].id));
    assert!(!index.contains(items[1].id) && index.contains(items[2].id));
    assert_eq!(index.len(), 3);
    assert_eq!(
        titles(&index, &items, "", 10),
        ["Email signature", "Invoice reminder", "Café menu"]
    );
    assert_eq!(titles(&index, &items, "reminder", 10), ["Invoice reminder"]);
}

#[test]
fn new_items_are_appended_after_existing_ones() {
    let first = item("Reply one", "");
    let second = item("Reply two", "");
    let mut index = SearchIndex::default();
    index.upsert(&first, Names::default());
    index.upsert(&second, Names::default());
    index.remove(first.id);
    index.upsert(&first, Names::default());
    let ids: Vec<_> = ask(&index, "", 10).iter().map(|h| h.id).collect();
    assert_eq!(ids, [second.id, first.id]);
}

#[test]
fn very_long_titles_do_not_break_matching() {
    let long = item(&"ab ".repeat(50_000), "");
    let index = SearchIndex::new([&long]);
    assert_eq!(ask(&index, "ab", 5).len(), 1);
}

fn corpus(n: usize) -> Vec<Item> {
    const WORDS: [&str; 24] = [
        "invoice",
        "reminder",
        "meeting",
        "standup",
        "signature",
        "address",
        "password",
        "template",
        "customer",
        "refund",
        "shipping",
        "thanks",
        "regards",
        "follow",
        "update",
        "report",
        "weekly",
        "budget",
        "contract",
        "review",
        "policy",
        "support",
        "ticket",
        "note",
    ];
    let mut state = 0x2545_F491_4F6C_DD1D_u64;
    let mut next = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        usize::try_from(state >> 33).unwrap()
    };
    (0..n)
        .map(|_| {
            let title = format!(
                "{} {} {}",
                WORDS[next() % 24],
                WORDS[next() % 24],
                next() % 1000
            );
            let body: Vec<&str> = (0..30).map(|_| WORDS[next() % 24]).collect();
            item(&title, &Item::preview_of(&body.join(" "), false))
        })
        .collect()
}

#[test]
fn parallel_and_serial_scoring_agree() {
    let items = corpus(2_500);
    let mut index = SearchIndex::new(&items);
    for (n, item) in items.iter().enumerate().take(200) {
        index.upsert(item, tags(&[if n % 2 == 0 { "billing" } else { "chat" }]));
    }
    for query in [
        "inv",
        "bil",
        "rem meet",
        "weekly 7",
        "zzz",
        "e",
        "-inv tag:bil",
    ] {
        let parsed = Query::parse(query);
        let none = BodyMatches::default();
        let serial = index.run(&parsed, &none, 50, usize::MAX, 1);
        for threads in [2, 3, 8] {
            assert_eq!(
                serial,
                index.run(&parsed, &none, 50, 0, threads),
                "{query} x{threads}"
            );
        }
        assert_eq!(serial, ask(&index, query, 50), "{query}");
    }
}

#[test]
fn results_are_sorted_by_score_then_given_order() {
    let items = corpus(300);
    let index = SearchIndex::new(&items);
    let position = |id| items.iter().position(|i| i.id == id).unwrap();
    let hits = ask(&index, "re", 1_000);
    assert!(hits.len() > 10);
    for pair in hits.windows(2) {
        assert!(
            pair[0].score > pair[1].score
                || (pair[0].score == pair[1].score && position(pair[0].id) < position(pair[1].id))
        );
    }
}

/// Found by the property test below (seed in `proptest-regressions/search/tests.txt`): marks
/// must be positions in the text's characters, whatever the folding does to them.
#[test]
fn marks_stay_inside_the_text() {
    for (text, q) in [("0\u{11CA9}$", "$"), ("ß", "ss"), ("ﬁ", "i"), ("Ǆ", "dz")] {
        let count = text.chars().count();
        let ranges = locate(text, &Query::parse(q), true);
        assert_ne!(ranges.len(), 0, "{text} {q}");
        assert!(
            ranges.iter().all(|r| r.start < r.end && r.end <= count),
            "{ranges:?}"
        );
    }
}

proptest! {
    #[test]
    fn queries_never_panic_and_respect_the_limit(
        titles in proptest::collection::vec(any::<String>(), 0..12),
        query in prop_oneof![any::<String>(), "[a-zA-Z0-9 \\\\^$!'é]{0,8}"],
        limit in 0usize..20,
    ) {
        let items: Vec<Item> = titles.iter().map(|t| item(t, t)).collect();
        let index = SearchIndex::new(&items);
        let hits = ask(&index, &query, limit);
        prop_assert!(hits.len() <= limit);
        let parsed = Query::parse(&query);
        for hit in &hits {
            let title = &items.iter().find(|i| i.id == hit.id).unwrap().title;
            let count = title.chars().count();
            let ranges = locate(title, &parsed, true);
            prop_assert!(ranges.iter().all(|r| r.start < r.end && r.end <= count));
            prop_assert!(ranges.windows(2).all(|w| w[0].end < w[1].start));
        }
    }
}

/// SPEC §9: searching 10k items (in-memory fields, several threads) takes under 16 ms.
const SPEC_BUDGET: Duration = Duration::from_millis(16);

/// The bound this build is held to. Release builds must meet the SPEC budget itself; the
/// unoptimised test profile (what `cargo test` runs in CI) is several times slower, so it is
/// held to four times the budget: loose enough for a busy CI machine, tight enough that the
/// regressions the audit measured (116 ms for a three-word query, 3.7 s for a pasted
/// paragraph, 2 ms per removal) fail.
fn budget() -> Duration {
    if cfg!(debug_assertions) {
        SPEC_BUDGET * 4
    } else {
        SPEC_BUDGET
    }
}

/// Timed runs per query: the fastest counts. A loaded CI runner only ever makes a run slower, so
/// the best of several runs (after one untimed warm-up) measures the code, not the neighbours; a
/// real regression is slow in all of them and still fails.
const TIMED_RUNS: usize = 9;

fn best(runs: &[Duration]) -> Duration {
    assert!(runs.len() >= 5, "best-of-N needs N >= 5");
    runs.iter().copied().min().unwrap_or_default()
}

/// LIBRARY-50 and LIBRARY-53: timing checks that run in every `cargo test` (not ignored): one
/// warm-up, then the best of [`TIMED_RUNS`] runs against [`budget`]. `--release` holds them to the
/// SPEC budget itself; the debug-profile allowance is [`budget`]'s four times.
#[test]
#[allow(
    clippy::print_stdout,
    reason = "the measurement is the point of this test"
)]
fn search_10k_items_stays_within_budget() {
    let items = corpus(10_000);
    let build = Instant::now();
    let mut index = SearchIndex::default();
    for (n, item) in items.iter().enumerate() {
        index.upsert(
            item,
            Names {
                tags: &[["billing", "chat"][n % 2], ["work", "home"][n % 3 % 2]],
                list: ["Email replies", "Snippets"][n % 2],
                trigger: None,
            },
        );
    }
    let build_time = build.elapsed();
    let avg_len = items
        .iter()
        .map(|i| i.title.len() + i.preview.len())
        .sum::<usize>()
        / items.len();
    assert!(avg_len > 60, "corpus too small: {avg_len}");
    let paragraph = "dear customer thank you ".repeat(250);
    let profile = if cfg!(debug_assertions) {
        "test (unoptimised)"
    } else {
        "release"
    };
    for query in [
        "e",
        "inv",
        "weekly rep",
        "refund 42",
        "zzzz",
        "dear customer thank",
        "snippet 9999",
        "tag:bil -work inv",
        paragraph.as_str(),
    ] {
        let parsed = Query::parse(query);
        let none = BodyMatches::default();
        std::hint::black_box(index.query(&parsed, &none, 50)); // warm-up: caches, allocator
        let runs: Vec<Duration> = (0..TIMED_RUNS)
            .map(|_| {
                let start = Instant::now();
                let hits = index.query(&parsed, &none, 50);
                std::hint::black_box(&hits);
                start.elapsed()
            })
            .collect();
        let took = best(&runs);
        let shown: String = query.chars().take(24).collect();
        println!(
            "search 10k `{shown}`: best of {TIMED_RUNS} {took:?} ({profile}, index build {build_time:?})"
        );
        assert!(
            took < budget(),
            "{shown}: best of {TIMED_RUNS} {took:?} is over {:?} ({profile})",
            budget()
        );
    }

    // LIBRARY-54: removing is constant time.
    // 200 removes in five chunks of 40: the fastest chunk, scaled to 200, is held to the budget.
    let doomed: Vec<_> = items.iter().step_by(50).map(|i| i.id).collect();
    assert_eq!(doomed.len(), 200);
    let chunks: Vec<Duration> = doomed
        .chunks(40)
        .map(|chunk| {
            let start = Instant::now();
            for id in chunk {
                assert!(index.remove(*id));
            }
            start.elapsed()
        })
        .collect();
    let took = best(&chunks) * 5;
    println!("remove 200 of 10k: {took:?} ({profile})");
    assert!(took < budget(), "200 removes took {took:?} ({profile})");
    assert_eq!(index.len(), 9_800);
    assert!(
        items
            .iter()
            .skip(1)
            .step_by(50)
            .all(|i| index.contains(i.id))
    );
}

#[test]
fn a_cloned_index_shares_its_entries_and_a_patch_copies_one() {
    let items = sample();
    let index = SearchIndex::new(&items);
    let mut patched = index.clone();
    let mut edited = items[1].clone();
    edited.title = "Renamed link".to_owned();
    patched.upsert(&edited, Names::default());
    // Three of four entries are the very same allocation; only the edited one was replaced.
    let shared = (0..items.len())
        .filter(|&i| Arc::ptr_eq(&index.entries[i], &patched.entries[i]))
        .count();
    assert_eq!(shared, 3);
    assert!(
        Arc::ptr_eq(&index.positions, &patched.positions),
        "positions untouched"
    );
    assert_eq!(
        titles(&index, &items, "Meeting", 5),
        ["Meeting link"],
        "the original is intact"
    );
    // A use count moves one entry only.
    let mut used = items[0].clone();
    used.use_count = 3;
    let mut counted = index.clone();
    assert!(counted.refresh_usage([&used]));
    assert_eq!(
        (0..items.len())
            .filter(|&i| Arc::ptr_eq(&index.entries[i], &counted.entries[i]))
            .count(),
        3
    );
}
