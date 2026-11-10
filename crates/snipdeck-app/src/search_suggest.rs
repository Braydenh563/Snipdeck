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

/// The byte range of the whitespace-delimited word around `caret` (a byte offset, clamped to
/// the query and to a character boundary), and where the caret sits in it.
fn word_at(query: &str, caret: usize) -> (usize, usize, usize) {
    let mut caret = caret.min(query.len());
    while !query.is_char_boundary(caret) {
        caret -= 1;
    }
    let start = query[..caret].rfind(char::is_whitespace).map_or(0, |at| {
        at + query[at..].chars().next().map_or(1, char::len_utf8)
    });
    let end = query[caret..]
        .find(char::is_whitespace)
        .map_or(query.len(), |at| caret + at);
    (start, end, caret)
}

/// The suggestions for the word at `caret` in `query`: names that start with the part of the
/// word before the caret (ignoring case), tags first, then lists, then title words, at most
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
            if folded.starts_with(&prefix) && folded != whole && !taken {
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
                let rows = dropdown.borrow_mut().show(query.as_str(), found);
                if let Some(window) = weak.upgrade() {
                    window.set_suggestions(rows);
                }
            });
        }
        {
            let (dropdown, weak) = (::std::rc::Rc::clone(&dropdown), weak.clone());
            $window.on_suggest_chosen(move |row| {
                let chosen = dropdown
                    .borrow_mut()
                    .choose(usize::try_from(row).unwrap_or(usize::MAX));
                let rows = dropdown.borrow_mut().clear();
                if let Some(window) = weak.upgrade() {
                    window.set_suggestions(rows);
                    if let Some((text, caret)) = chosen {
                        window.invoke_apply_suggestion(
                            text.into(),
                            i32::try_from(caret).unwrap_or(0),
                        );
                    }
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
    found: Vec<Suggestion>,
}

impl Dropdown {
    /// Remembers `found` for `query` and returns the dropdown's rows.
    pub fn show(&mut self, query: &str, found: Vec<Suggestion>) -> ModelRc<SuggestRow> {
        query.clone_into(&mut self.query);
        self.found = found;
        self.rows()
    }

    /// Forgets the suggestions and returns the empty rows (Esc, or a choice made).
    pub fn clear(&mut self) -> ModelRc<SuggestRow> {
        self.found.clear();
        self.rows()
    }

    /// The query with row `index` applied and the caret after it; the dropdown is then empty.
    pub fn choose(&mut self, index: usize) -> Option<(String, usize)> {
        let chosen = self.found.get(index).map(|s| apply(&self.query, s));
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
        assert!(texts("", 0, &index).is_empty());
        assert!(texts("zzz", 3, &index).is_empty());
        let kinds: Vec<Kind> = suggest("ser", 3, &index).iter().map(|s| s.kind).collect();
        assert_eq!(kinds, [Kind::Tag, Kind::List, Kind::Word]);
    }

    #[test]
    fn never_suggests_sensitive_titles() {
        let index = index();
        assert!(!texts("pass", 4, &index).iter().any(|t| t == "password"));
        assert!(texts("pass", 4, &index).is_empty());
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
        let rows = dropdown.show(query, suggest(query, query.len(), &index));
        assert_eq!(slint::Model::row_count(&rows), 3);
        assert_eq!(dropdown.choose(1), Some(("mail Servers".to_owned(), 12)));
        assert_eq!(dropdown.choose(0), None, "a choice empties the dropdown");
        assert_eq!(slint::Model::row_count(&dropdown.clear()), 0);
    }

    #[test]
    fn a_caret_off_a_character_boundary_does_not_panic() {
        let index = index();
        let _ = suggest("é", 1, &index);
        let _ = suggest("ab é", 99, &index);
    }
}
