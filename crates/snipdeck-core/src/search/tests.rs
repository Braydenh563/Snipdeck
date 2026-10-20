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

fn titles(index: &SearchIndex, items: &[Item], query: &str, limit: usize) -> Vec<String> {
    index
        .query(query, limit)
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
    let hit = &index.query("", 1)[0];
    assert_eq!((hit.score, hit.title_indices.is_empty()), (0, true));
}

#[test]
fn a_blank_query_counts_as_empty() {
    let items = sample();
    let index = SearchIndex::new(&items);
    assert_eq!(index.query("  \t ", 10).len(), 4);
}

#[test]
fn limit_zero_and_no_match_give_nothing() {
    let items = sample();
    let index = SearchIndex::new(&items);
    assert_eq!(index.query("", 0).len(), 0);
    assert_eq!(index.query("sig", 0).len(), 0);
    assert_eq!(index.query("qqqqzzzz", 10).len(), 0);
    assert_eq!(SearchIndex::default().query("a", 10).len(), 0);
}

#[test]
fn title_matches_report_character_indices_for_highlighting() {
    let items = sample();
    let index = SearchIndex::new(&items);
    let hits = index.query("signature", 10);
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].id, items[0].id);
    assert_eq!(hits[0].title_indices, (6..15).collect::<Vec<u32>>());
}

#[test]
fn indices_count_characters_and_matching_ignores_accents_and_lowercase() {
    let items = sample();
    let index = SearchIndex::new(&items);
    let hits = index.query("cafe", 10);
    assert_eq!(hits[0].id, items[3].id);
    assert_eq!(hits[0].title_indices, vec![0, 1, 2, 3]);
    // Smart case: a capital in the query makes that word case-sensitive.
    assert_eq!(index.query("email", 10)[0].id, items[0].id);
    assert_eq!(index.query("Email", 10)[0].id, items[0].id);
    assert_eq!(index.query("EMAIL", 10).len(), 0);
}

#[test]
fn every_word_of_the_query_must_match() {
    let items = sample();
    let index = SearchIndex::new(&items);
    assert_eq!(titles(&index, &items, "mail sig", 10), ["Email signature"]);
    assert_eq!(index.query("mail zzzz", 10).len(), 0);
}

#[test]
fn title_outranks_tag_outranks_content() {
    let content = item("Zeta", "invoice reminder text");
    let tagged = item("Alpha", "nothing here");
    let titled = item("Invoice", "");
    let mut index = SearchIndex::default();
    index.upsert(&content);
    index.upsert_with_tags(&tagged, ["billing", "invoice"]);
    index.upsert(&titled);
    let ids: Vec<_> = index.query("invoice", 10).iter().map(|h| h.id).collect();
    assert_eq!(ids, [titled.id, tagged.id, content.id]);
}

#[test]
fn a_weak_title_match_still_beats_a_perfect_tag_match() {
    let weak = item("International voice note", "");
    let tagged = item("Other", "");
    let mut index = SearchIndex::default();
    index.upsert_with_tags(&tagged, ["invoice"]);
    index.upsert(&weak);
    let ids: Vec<_> = index.query("invoice", 10).iter().map(|h| h.id).collect();
    assert_eq!(ids, [weak.id, tagged.id]);
}

#[test]
fn tag_hits_have_no_title_indices() {
    let tagged = item("Alpha", "");
    let mut index = SearchIndex::default();
    index.upsert_with_tags(&tagged, ["urgent"]);
    let hits = index.query("urgent", 10);
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].title_indices.len(), 0);
}

#[test]
fn equal_scores_keep_the_given_order() {
    let items: Vec<Item> = (0..5).map(|_| item("Reply", "")).collect();
    let index = SearchIndex::new(&items);
    let ids: Vec<_> = index.query("reply", 10).iter().map(|h| h.id).collect();
    assert_eq!(ids, items.iter().map(|i| i.id).collect::<Vec<_>>());
}

#[test]
fn content_matching_covers_only_the_first_64_characters() {
    let inside = item("A", &format!("{}needle", "x".repeat(58)));
    let outside = item("B", &format!("{}needle", "x".repeat(59)));
    let accents = item("C", &format!("{}zzzz", "é".repeat(60)));
    let index = SearchIndex::new([&inside, &outside, &accents]);
    let ids: Vec<_> = index.query("needle", 10).iter().map(|h| h.id).collect();
    assert_eq!(ids, [inside.id]);
    let ids: Vec<_> = index.query("zzzz", 10).iter().map(|h| h.id).collect();
    assert_eq!(ids, [accents.id]);
}

#[test]
fn upsert_replaces_in_place() {
    let mut items = sample();
    let mut index = SearchIndex::new(&items);
    items[1].title = "Standup link".to_owned();
    index.upsert(&items[1]);
    assert_eq!(index.len(), 4);
    assert_eq!(index.query("meeting", 10).len(), 0);
    assert_eq!(titles(&index, &items, "", 10)[1], "Standup link");
}

#[test]
fn upsert_replaces_tags_too() {
    let a = item("Alpha", "");
    let mut index = SearchIndex::default();
    index.upsert_with_tags(&a, ["urgent"]);
    index.upsert_with_tags(&a, ["billing"]);
    assert_eq!(index.query("urgent", 10).len(), 0);
    assert_eq!(index.query("billing", 10).len(), 1);
    index.upsert(&a);
    assert_eq!(index.query("billing", 10).len(), 0);
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
    index.upsert(&first);
    index.upsert(&second);
    index.remove(first.id);
    index.upsert(&first);
    let ids: Vec<_> = index.query("", 10).iter().map(|h| h.id).collect();
    assert_eq!(ids, [second.id, first.id]);
}

#[test]
fn very_long_titles_do_not_break_matching() {
    let long = item(&"ab ".repeat(50_000), "");
    let index = SearchIndex::new([&long]);
    assert_eq!(index.query("ab", 5).len(), 1);
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
        index.upsert_with_tags(item, [if n % 2 == 0 { "billing" } else { "chat" }]);
    }
    for query in ["inv", "bil", "rem meet", "weekly 7", "zzz", "e"] {
        let serial = index.run(query, 50, usize::MAX, 1);
        for threads in [2, 3, 8] {
            assert_eq!(
                serial,
                index.run(query, 50, 0, threads),
                "{query} x{threads}"
            );
        }
        assert_eq!(serial, index.query(query, 50), "{query}");
    }
}

#[test]
fn results_are_sorted_by_score_then_given_order() {
    let items = corpus(300);
    let index = SearchIndex::new(&items);
    let position = |id| items.iter().position(|i| i.id == id).unwrap();
    let hits = index.query("re", 1_000);
    assert!(hits.len() > 10);
    for pair in hits.windows(2) {
        assert!(
            pair[0].score > pair[1].score
                || (pair[0].score == pair[1].score && position(pair[0].id) < position(pair[1].id))
        );
    }
}

/// Found by the property test below (seed in `proptest-regressions/search/tests.txt`): the
/// highlight indices must be positions in the title's characters, whatever the matcher counts.
#[test]
fn highlight_indices_stay_inside_the_title() {
    let items = [item("0\u{11CA9}$", "0\u{11CA9}$")];
    let index = SearchIndex::new(&items);
    let hits = index.query("$", 1);
    let count = u32::try_from(items[0].title.chars().count()).unwrap();
    assert_eq!(hits.len(), 1);
    assert!(
        hits[0].title_indices.iter().all(|&i| i < count),
        "{:?}",
        hits[0].title_indices
    );
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
        let hits = index.query(&query, limit);
        prop_assert!(hits.len() <= limit);
        for hit in &hits {
            let title = &items.iter().find(|i| i.id == hit.id).unwrap().title;
            let count = u32::try_from(title.chars().count()).unwrap();
            prop_assert!(hit.title_indices.iter().all(|&i| i < count));
            prop_assert!(hit.title_indices.windows(2).all(|w| w[0] < w[1]));
        }
    }
}

/// SPEC §9: searching 10k items (title + first 64 characters, 4 threads) takes under 16 ms.
const SPEC_BUDGET: Duration = Duration::from_millis(16);

/// Timing check for SPEC §9 (10k items, title + 64 chars, < 16 ms). The median of 21 runs
/// must meet the real budget: release builds measured 0.6 to 3.6 ms here (4 cores, medians,
/// on a machine busy with other builds; worst single run 9 ms), so a regression to the budget
/// is a regression worth failing on, while ordinary noise keeps a 4x margin.
///
/// A release build needs a licence key (see `docs/licensing.md`); any valid public key will
/// do for timing:
/// `SNIPDECK_ALLOW_TEST_LICENCE_KEY=1 SNIPDECK_LICENCE_PUBLIC_KEY=Zr5+Myx6RTMyvZ0Kf32wVfXF7xoGraZtmLOftoEMRzo= cargo test --release -p snipdeck-core --lib search_10k -- --ignored --nocapture`.
#[test]
#[ignore = "timing test: run with --release -- --ignored"]
#[allow(
    clippy::print_stdout,
    reason = "the measurement is the point of this test"
)]
fn search_10k_items_stays_within_budget() {
    let items = corpus(10_000);
    let build = Instant::now();
    let index = SearchIndex::new(&items);
    let build_time = build.elapsed();
    let avg_len = items
        .iter()
        .map(|i| i.title.len() + i.preview.len())
        .sum::<usize>()
        / items.len();
    assert!(avg_len > 150, "corpus too small: {avg_len}");

    for query in ["e", "inv", "weekly rep", "refund 42", "zzzz"] {
        let mut runs: Vec<Duration> = (0..21)
            .map(|_| {
                let start = Instant::now();
                let hits = index.query(query, 50);
                std::hint::black_box(&hits);
                start.elapsed()
            })
            .collect();
        runs.sort();
        println!(
            "search 10k `{query}`: median {:?}, min {:?}, max {:?} (index build {build_time:?}, avg chars {avg_len})",
            runs[10], runs[0], runs[20]
        );
        assert!(
            runs[10] < SPEC_BUDGET,
            "{query}: median {:?} is over the SPEC budget {SPEC_BUDGET:?}",
            runs[10]
        );
    }
}
