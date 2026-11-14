//! Search suggestions: the dropdown under the Panel, Picker and Main search fields that
//! completes the word being typed (v1.0.1 slice 8).
//!
//! Pure: no window, no store. [`Index`] holds the names a suggestion can be made of, and
//! [`suggest`] answers for a query and the caret in it. Suggestions are tag names, list names
//! and the words of item titles (a prefix of the word at the caret, case-folded, at most
//! [`MAX`]), never items themselves: the results already show items. A Sensitive item's title
//! is never indexed. [`apply`] replaces only the word at the caret.

use slint::{ModelRc, VecModel};
use snipdeck_core::Item;
use snipdeck_ui::SuggestRow;

/// The most rows the dropdown shows.
pub const MAX: usize = 6;

/// Title words shorter than this are not suggested.
const MIN_WORD_CHARS: usize = 3;

/// What a suggestion names, for its glyph and spoken label.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// A tag name.
    Tag,
    /// A list name.
    List,
    /// A word of an item's title.
    Word,
}

/// One row of the dropdown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Suggestion {
    /// The text that replaces the word at the caret.
    pub text: String,
    /// What the text names.
    pub kind: Kind,
    /// The byte range of the query's word that the text replaces.
    pub replaces: (usize, usize),
}

/// The names suggestions are made of: built once with the search index, read per keystroke.
#[derive(Debug, Default, Clone)]
pub struct Index {
    tags: Vec<String>,
    lists: Vec<String>,
    words: Vec<String>,
}

fn fold(text: &str) -> String {
    text.to_lowercase()
}

fn sorted_unique(names: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut names: Vec<String> = names.into_iter().collect();
    names.sort_by_cached_key(|name| (fold(name), name.clone()));
    names.dedup_by(|a, b| fold(a) == fold(b));
    names
}

impl Index {
    /// An index over `items` (live ones; a Sensitive item contributes no title word), the
    /// `tags` and the `lists`.
    pub fn build<'a>(
        items: impl IntoIterator<Item = &'a Item>,
        tags: impl IntoIterator<Item = &'a str>,
        lists: impl IntoIterator<Item = &'a str>,
    ) -> Self {
        let words = items
            .into_iter()
            .filter(|item| !item.sensitive)
            .flat_map(|item| {
                item.title
                    .split(|c: char| !c.is_alphanumeric())
                    .filter(|w| w.chars().count() >= MIN_WORD_CHARS)
                    .map(str::to_owned)
                    .collect::<Vec<_>>()
            });
        Self {
            tags: sorted_unique(tags.into_iter().map(str::to_owned)),
            lists: sorted_unique(lists.into_iter().map(str::to_owned)),
            // Title words are shown in lower case unless they are written in capitals.
            words: sorted_unique(words.map(|w| {
                if w.chars().any(char::is_lowercase) {
                    fold(&w)
                } else {
                    w
                }
            })),
        }
    }
}

/// The byte range of the word around `caret` (a byte offset, clamped to the query and to a
/// character boundary), and where the caret sits in it. A word is a run of letters and digits,
/// the same split the title words use, so search syntax around it (`#tag`, `^start`, `"quote`,
/// `-not`) stays where it is: only the run is replaced.
fn word_at(query: &str, caret: usize) -> (usize, usize, usize) {
    let mut caret = caret.min(query.len());
    while !query.is_char_boundary(caret) {
        caret -= 1;
    }
    let start = query[..caret]
        .char_indices()
        .rev()
        .take_while(|(_, c)| c.is_alphanumeric())
        .last()
        .map_or(caret, |(at, _)| at);
    let end = query[caret..]
        .char_indices()
        .find(|(_, c)| !c.is_alphanumeric())
        .map_or(query.len(), |(at, _)| caret + at);
    (start, end, caret)
}

/// Whether `name` (folded) starts with `prefix`, or one of its words does: `rep` finds
/// "Email replies".
fn matches_prefix(folded: &str, prefix: &str) -> bool {
    folded
        .split(|c: char| !c.is_alphanumeric())
        .any(|word| word.starts_with(prefix))
}

/// The suggestions for the word at `caret` in `query`: names that start with the part of the
/// word before the caret (ignoring case; for a name of several words, any of its words), tags first, then lists, then title words, at most
/// [`MAX`]. None for an empty word, and none that would only repeat the word as typed.
#[must_use]
pub fn suggest(query: &str, caret: usize, index: &Index) -> Vec<Suggestion> {
    let (start, end, caret) = word_at(query, caret);
    let prefix = fold(&query[start..caret]);
    if prefix.is_empty() {
        return Vec::new();
    }
    let whole = fold(&query[start..end]);
    let groups = [
        (Kind::Tag, &index.tags),
        (Kind::List, &index.lists),
        (Kind::Word, &index.words),
    ];
    let mut found: Vec<Suggestion> = Vec::new();
    for (kind, names) in groups {
        for name in names {
            let folded = fold(name);
            let taken = found.iter().any(|s| fold(&s.text) == folded);
            if matches_prefix(&folded, &prefix) && folded != whole && !taken {
                found.push(Suggestion {
                    text: name.clone(),
                    kind,
                    replaces: (start, end),
                });
            }
        }
    }
    found.truncate(MAX);
    found
}

/// `query` with the word `suggestion` was made for replaced by its text, and the caret (a byte
/// offset) just after it. Nothing else in the query changes.
#[must_use]
pub fn apply(query: &str, suggestion: &Suggestion) -> (String, usize) {
    let (start, end) = suggestion.replaces;
    let start = start.min(query.len());
    let end = end.clamp(start, query.len());
    let mut out = String::with_capacity(query.len() + suggestion.text.len());
    out.push_str(&query[..start]);
    out.push_str(&suggestion.text);
    let caret = out.len();
    out.push_str(&query[end..]);
    (out, caret)
}

/// Wires a window's search suggestions to a controller: `$find` is the controller method that
/// answers `(query, caret)`. The window needs `on_suggest_requested`, `on_suggest_chosen`,
/// `on_suggest_dismissed`, `set_suggestions` and `invoke_apply_suggestion`.
#[macro_export]
macro_rules! wire_suggestions {
    ($window:expr, $controller:expr, $find:ident) => {{
        let dropdown = ::std::rc::Rc::new(::std::cell::RefCell::new(
            $crate::search_suggest::Dropdown::default(),
        ));
        let weak = $window.as_weak();
        {
            let (dropdown, weak) = (::std::rc::Rc::clone(&dropdown), weak.clone());
            let controller = ::std::rc::Rc::clone(&$controller);
            $window.on_suggest_requested(move |query, caret| {
                let found = controller
                    .borrow()
                    .$find(query.as_str(), usize::try_from(caret).unwrap_or(0));
                let rows = dropdown.borrow_mut().show(
                    query.as_str(),
                    usize::try_from(caret).unwrap_or(0),
                    found,
                );
                if let Some(window) = weak.upgrade() {
                    window.set_suggest_query(query);
                    window.set_suggestions(rows);
                }
            });
        }
        {
            let (dropdown, weak) = (::std::rc::Rc::clone(&dropdown), weak.clone());
            let controller = ::std::rc::Rc::clone(&$controller);
            $window.on_suggest_chosen(move |row| {
                let Some(window) = weak.upgrade() else {
                    return;
                };
                let (query, caret) = dropdown.borrow().asked();
                let fresh = controller.borrow().$find(&query, caret);
                let chosen = dropdown.borrow_mut().choose(
                    usize::try_from(row).unwrap_or(usize::MAX),
                    window.get_query().as_str(),
                    &fresh,
                );
                let rows = dropdown.borrow_mut().clear();
                window.set_suggestions(rows);
                if let Some((text, caret)) = chosen {
                    window.invoke_apply_suggestion(text.into(), i32::try_from(caret).unwrap_or(0));
                }
            });
        }
        $window.on_suggest_dismissed(move || {
            let rows = dropdown.borrow_mut().clear();
            if let Some(window) = weak.upgrade() {
                window.set_suggestions(rows);
            }
        });
    }};
}

/// What one window's dropdown currently offers: kept by its binding between the request (an
/// edit) and the choice (a click or Enter), so the choice replaces the word it was made for.
#[derive(Debug, Default)]
pub struct Dropdown {
    query: String,
    caret: usize,
    found: Vec<Suggestion>,
}

impl Dropdown {
    /// Remembers `found` for `query` with the caret at `caret` and returns the dropdown's rows.
    pub fn show(
        &mut self,
        query: &str,
        caret: usize,
        found: Vec<Suggestion>,
    ) -> ModelRc<SuggestRow> {
        query.clone_into(&mut self.query);
        self.caret = caret;
        self.found = found;
        self.rows()
    }

    /// The query and caret the rows were made for, to ask the controller again at choice time.
    #[must_use]
    pub fn asked(&self) -> (String, usize) {
        (self.query.clone(), self.caret)
    }

    /// Forgets the suggestions and returns the empty rows (Esc, or a choice made).
    pub fn clear(&mut self) -> ModelRc<SuggestRow> {
        self.found.clear();
        self.rows()
    }

    /// The query with row `index` applied and the caret after it; the dropdown is then empty.
    /// `None` (and nothing applied) when the rows are stale: the search text is no longer the
    /// one they were made for (`current_query`), or `fresh`, the suggestions asked again now,
    /// no longer holds that row (the library changed).
    pub fn choose(
        &mut self,
        index: usize,
        current_query: &str,
        fresh: &[Suggestion],
    ) -> Option<(String, usize)> {
        let chosen = self
            .found
            .get(index)
            .filter(|s| self.query == current_query && fresh.contains(s))
            .map(|s| apply(&self.query, s));
        self.found.clear();
        chosen
    }

    fn rows(&self) -> ModelRc<SuggestRow> {
        let rows: Vec<SuggestRow> = self
            .found
            .iter()
            .map(|s| SuggestRow {
                text: s.text.as_str().into(),
                kind: match s.kind {
                    Kind::Tag => 0,
                    Kind::List => 1,
                    Kind::Word => 2,
                },
            })
            .collect();
        ModelRc::new(VecModel::from(rows))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use snipdeck_core::{ItemKind, ListId, Timestamp};

    fn at() -> Timestamp {
        Timestamp::from_second(1_800_000_000).expect("a valid time")
    }

    fn item(title: &str, sensitive: bool) -> Item {
        let mut item = Item::new(ListId::generate(), title.to_owned(), ItemKind::Text, at());
        item.sensitive = sensitive;
        item
    }

    fn index() -> Index {
        let items = [
            item("Email signature", false),
            item("Server password", true),
            item("Servo reset", false),
        ];
        Index::build(&items, ["Server", "work"], ["Servers", "Personal"])
    }

    fn texts(query: &str, caret: usize, index: &Index) -> Vec<String> {
        suggest(query, caret, index)
            .into_iter()
            .map(|s| s.text)
            .collect()
    }

    #[test]
    fn suggests_tags_lists_and_title_words_by_prefix() {
        let index = index();
        assert_eq!(texts("ser", 3, &index), ["Server", "Servers", "servo"]);
        assert_eq!(texts("SIG", 3, &index), ["signature"], "case is folded");
        assert_eq!(texts("wo", 2, &index), ["work"]);
        assert_eq!(texts("", 0, &index).len(), 0);
        assert_eq!(texts("zzz", 3, &index).len(), 0);
        let kinds: Vec<Kind> = suggest("ser", 3, &index).iter().map(|s| s.kind).collect();
        assert_eq!(kinds, [Kind::Tag, Kind::List, Kind::Word]);
    }

    #[test]
    fn never_suggests_sensitive_titles() {
        let index = index();
        assert!(!texts("pass", 4, &index).iter().any(|t| t == "password"));
        assert_eq!(texts("pass", 4, &index).len(), 0);
        assert!(texts("emai", 4, &index).contains(&"email".to_owned()));
    }

    #[test]
    fn caps_at_six() {
        let items: Vec<Item> = (0..10).map(|n| item(&format!("alpha{n}"), false)).collect();
        let index = Index::build(&items, [], []);
        assert_eq!(suggest("alp", 3, &index).len(), MAX);
    }

    #[test]
    fn replaces_only_the_word_at_the_caret() {
        let index = index();
        let query = "mail ser tail";
        let found = suggest(query, 8, &index);
        assert_eq!(found[0].text, "Server");
        let (out, caret) = apply(query, &found[0]);
        assert_eq!(out, "mail Server tail");
        assert_eq!(&out[..caret], "mail Server");
        // The caret inside a word completes from the text before it and swaps the whole word.
        let (out, _) = apply("sersomething", &suggest("sersomething", 3, &index)[0]);
        assert_eq!(out, "Server");
    }

    #[test]
    fn a_word_already_typed_in_full_is_not_offered_again() {
        let index = index();
        assert!(!texts("servo", 5, &index).contains(&"servo".to_owned()));
    }

    #[test]
    fn choosing_a_row_applies_it_to_the_query_it_was_made_for_and_empties_the_dropdown() {
        let index = index();
        let mut dropdown = Dropdown::default();
        let query = "mail ser";
        let found = suggest(query, query.len(), &index);
        let rows = dropdown.show(query, query.len(), found.clone());
        assert_eq!(slint::Model::row_count(&rows), 3);
        assert_eq!(
            dropdown.choose(1, query, &found),
            Some(("mail Servers".to_owned(), 12))
        );
        assert_eq!(
            dropdown.choose(0, query, &found),
            None,
            "a choice empties the dropdown"
        );
        assert_eq!(slint::Model::row_count(&dropdown.clear()), 0);
    }

    #[test]
    fn stale_rows_choose_nothing() {
        let index = index();
        let query = "ser";
        let found = suggest(query, 3, &index);
        let mut dropdown = Dropdown::default();
        dropdown.show(query, 3, found.clone());
        assert_eq!(
            dropdown.choose(0, "", &found),
            None,
            "the search was cleared or set by the program"
        );
        dropdown.show(query, 3, found.clone());
        assert_eq!(
            dropdown.choose(0, "sero", &found),
            None,
            "the text moved on"
        );
        dropdown.show(query, 3, found.clone());
        let after_a_rename = suggest(query, 3, &Index::build(&[], ["Other"], []));
        assert_eq!(
            dropdown.choose(0, query, &after_a_rename),
            None,
            "the index changed and no longer offers the row"
        );
        dropdown.show(query, 3, found.clone());
        assert!(dropdown.choose(0, query, &found).is_some());
    }

    #[test]
    fn words_split_like_title_words_and_keep_search_syntax() {
        let items = [
            item("Café menu", false),
            item("日本語 メモ", false),
            item("naïve", false),
        ];
        let index = Index::build(&items, ["Email replies", "#x"], ["Work notes"]);
        // A tag typed with a leading `#` keeps it; only the run after it is replaced.
        let found = suggest("#rep", 4, &index);
        assert_eq!(
            found[0].text, "Email replies",
            "a later word of the name matches"
        );
        assert_eq!(apply("#rep", &found[0]), ("#Email replies".to_owned(), 14));
        // Other syntax around the word stays too.
        let found = suggest("-\"not", 5, &index);
        assert_eq!(apply("-\"not", &found[0]), ("-\"Work notes".to_owned(), 12));
        // Accents: the caret is a byte offset after the é.
        let found = suggest("caf", 3, &index);
        assert_eq!(found[0].text, "café");
        assert_eq!(apply("caf", &found[0]), ("café".to_owned(), 5));
        assert!(
            suggest("ça café", 3, &index).is_empty(),
            "the word at the caret is `ça`"
        );
        // CJK words are words.
        let found = suggest("日本", 6, &index);
        assert_eq!(found[0].text, "日本語");
        assert_eq!(apply("日本 x", &found[0]), ("日本語 x".to_owned(), 9));
        // An emoji ends a word, and the caret after one has no word.
        assert_eq!(suggest("🙂", 4, &index).len(), 0);
        let found = suggest("🙂caf", 7, &index);
        assert_eq!(apply("🙂caf", &found[0]), ("🙂café".to_owned(), 9));
        // Mid-word caret: the whole run is replaced.
        let found = suggest("naixx", 3, &index);
        assert_eq!(found.len(), 0);
        let found = suggest("naïxx", 4, &index);
        assert_eq!(apply("naïxx", &found[0]), ("naïve".to_owned(), 6));
    }

    #[test]
    fn a_caret_off_a_character_boundary_does_not_panic() {
        let index = index();
        let _ = suggest("é", 1, &index);
        let _ = suggest("ab é", 99, &index);
    }
}
