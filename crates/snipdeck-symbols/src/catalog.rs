//! The symbol catalog: built once, on first use, from the embedded data.

use std::collections::HashMap;
use std::ops::Range;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::art::ArtPack;
use crate::data::{self, DataError};
use crate::fold::{fold, fold_german, words};

/// English names of the emoji groups, in the order of `Symbol::group` for emoji.
pub const EMOJI_GROUPS: [&str; 9] = [
    "Smileys & Emotion",
    "People & Body",
    "Animals & Nature",
    "Food & Drink",
    "Travel & Places",
    "Activities",
    "Objects",
    "Symbols",
    "Flags",
];

/// English names of the kaomoji groups, in the order of `Symbol::group` for kaomoji.
pub const KAOMOJI_GROUPS: [&str; 10] = [
    "joy", "love", "sad", "angry", "surprise", "shrug", "greet", "sorry", "animals", "actions",
];

/// English names of the character groups (`Symbol::group` for characters); the same list the data
/// generator writes to the manifest.
pub const CHARACTER_GROUPS: [&str; 10] = [
    "Punctuation",
    "Arrows",
    "Maths",
    "Currency",
    "Letters and numbers",
    "Super and subscripts",
    "Technical",
    "Box drawing",
    "Shapes",
    "Symbols",
];

/// The four kinds of symbol. The order is the order of the catalog and of the tabs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Kind {
    /// A Unicode emoji sequence.
    Emoji,
    /// A Japanese emoticon.
    Kaomoji,
    /// A Phosphor icon (not text: it is copied as SVG or PNG).
    Icon,
    /// A Unicode symbol character (arrow, maths, currency, punctuation, box drawing), pasted as text.
    Character,
}

/// A stable, persisted identity: the emoji's lowercase hex sequence, the kaomoji id, the icon name.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SymbolId {
    /// For example `1f469-200d-1f4bb`.
    Emoji(String),
    /// For example `actions-table-flip`.
    Kaomoji(String),
    /// For example `arrow-clockwise`.
    Icon(String),
    /// The lowercase hex code point, for example `2192`.
    Char(String),
}

impl SymbolId {
    /// Which kind this id names.
    #[must_use]
    pub fn kind(&self) -> Kind {
        match self {
            Self::Emoji(_) => Kind::Emoji,
            Self::Kaomoji(_) => Kind::Kaomoji,
            Self::Icon(_) => Kind::Icon,
            Self::Char(_) => Kind::Character,
        }
    }

    /// The persisted form: `emoji:1f600`, `kaomoji:joy-smile`, `icon:arrow-clockwise`.
    #[must_use]
    pub fn key(&self) -> String {
        match self {
            Self::Emoji(s) => format!("emoji:{s}"),
            Self::Kaomoji(s) => format!("kaomoji:{s}"),
            Self::Icon(s) => format!("icon:{s}"),
            Self::Char(s) => format!("char:{s}"),
        }
    }

    /// Parses [`SymbolId::key`]; `None` for anything else.
    #[must_use]
    pub fn from_key(key: &str) -> Option<Self> {
        let (kind, rest) = key.split_once(':')?;
        if rest.is_empty() {
            return None;
        }
        match kind {
            "emoji" => Some(Self::Emoji(rest.to_owned())),
            "kaomoji" => Some(Self::Kaomoji(rest.to_owned())),
            "icon" => Some(Self::Icon(rest.to_owned())),
            "char" => Some(Self::Char(rest.to_owned())),
            _ => None,
        }
    }
}

/// The languages whose emoji names are bundled. English is always searched as well.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Lang {
    /// English (the base data).
    En,
    /// German.
    De,
    /// Spanish.
    Es,
    /// French.
    Fr,
    /// Japanese.
    Ja,
    /// Portuguese.
    Pt,
    /// Simplified Chinese.
    Zh,
}

impl Lang {
    /// The language of a tag such as `de`, `pt-BR` or `zh-Hans`; English when unknown.
    #[must_use]
    pub fn from_tag(tag: &str) -> Self {
        match tag
            .split(['-', '_'])
            .next()
            .unwrap_or("")
            .to_ascii_lowercase()
            .as_str()
        {
            "de" => Self::De,
            "es" => Self::Es,
            "fr" => Self::Fr,
            "ja" => Self::Ja,
            "pt" => Self::Pt,
            "zh" => Self::Zh,
            _ => Self::En,
        }
    }

    /// Index into the bundled names tables; `None` for English.
    fn slot(self) -> Option<usize> {
        match self {
            Self::En => None,
            Self::De => Some(0),
            Self::Es => Some(1),
            Self::Fr => Some(2),
            Self::Ja => Some(3),
            Self::Pt => Some(4),
            Self::Zh => Some(5),
        }
    }
}

/// One symbol. Emoji and kaomoji carry the text that is pasted; icons carry none.
#[derive(Clone, Debug)]
pub struct Symbol {
    /// The stable identity.
    pub id: SymbolId,
    /// Emoji, kaomoji or icon.
    pub kind: Kind,
    /// What is pasted. `None` for icons.
    pub text: Option<String>,
    /// English display name: CLDR short name, a kaomoji's first tag, an icon's humanised name.
    pub name: String,
    /// Group index: [`EMOJI_GROUPS`], [`KAOMOJI_GROUPS`] or [`Catalog::icon_categories`].
    pub group: u8,
    /// English keywords (kaomoji tags, icon tags).
    pub keywords: Vec<String>,
    /// A kaomoji longer than ten characters spans both columns of the grid.
    pub wide: bool,
    icon_markup: Option<String>,
}

impl Symbol {
    /// The icon's stored markup (path data or raw inner svg); `None` for text symbols.
    pub(crate) fn icon_markup(&self) -> Option<&str> {
        self.icon_markup.as_deref()
    }
}

/// Folded name and keywords, ready for matching.
#[derive(Clone, Debug, Default)]
pub(crate) struct View {
    pub(crate) name: String,
    /// The name and keywords without spaces: "thumbsup" finds "thumbs up".
    pub(crate) name_compact: String,
    pub(crate) keywords_compact: Vec<String>,
    pub(crate) keywords: Vec<String>,
    pub(crate) name_words: Vec<String>,
    pub(crate) words: Vec<String>,
}

impl View {
    pub(crate) fn new(name: &str, keywords: &[String]) -> Self {
        let folded = fold(name);
        let mut kws: Vec<String> = keywords.iter().map(|k| fold(k)).collect();
        // The German spelling of an umlaut (laecheln for lächeln) is an alternate of the name and
        // of every keyword that has one.
        for raw in std::iter::once(name).chain(keywords.iter().map(String::as_str)) {
            let alt = fold_german(raw);
            if !kws.contains(&alt) && alt != folded {
                kws.push(alt);
            }
        }
        let name_words: Vec<String> = words(&folded).map(str::to_owned).collect();
        let mut all = name_words.clone();
        all.extend(kws.iter().flat_map(|k| words(k).map(str::to_owned)));
        let compact = |s: &str| s.chars().filter(|c| !c.is_whitespace()).collect::<String>();
        Self {
            name_compact: compact(&folded),
            keywords_compact: kws.iter().map(|k| compact(k)).collect(),
            name: folded,
            keywords: kws,
            name_words,
            words: all,
        }
    }
}

/// A translated emoji name with its folded view.
pub(crate) struct LangEntry {
    pub(crate) name: String,
    pub(crate) view: View,
}

/// The names of every symbol that has a translation in one language, indexed like the catalog.
pub(crate) struct LangNames {
    pub(crate) entries: Vec<Option<LangEntry>>,
    /// English term (kaomoji group, tag or name; icon category, tag or name) to its translation.
    pub(crate) terms: HashMap<String, String>,
}

/// Every symbol, in browse order: emoji, then kaomoji, then icons.
pub struct Catalog {
    symbols: Vec<Symbol>,
    views: Vec<View>,
    by_id: HashMap<SymbolId, usize>,
    emoji_count: usize,
    kaomoji_count: usize,
    icon_count: usize,
    icon_categories: Vec<String>,
    /// Emoji catalog index to its five uniform skin-tone variants (light to dark).
    tones: HashMap<usize, [String; 5]>,
    names: [OnceLock<LangNames>; 6],
    art: ArtPack,
}

static CATALOG: OnceLock<Catalog> = OnceLock::new();
static INITS: AtomicUsize = AtomicUsize::new(0);

/// How many times the shared catalog has been built: 0 until first use, then 1.
#[must_use]
pub fn catalog_init_count() -> usize {
    INITS.load(Ordering::SeqCst)
}

fn bad(file: &'static str, what: &'static str) -> DataError {
    DataError { file, what }
}

fn humanise(name: &str) -> String {
    name.replace('-', " ")
}

fn split_list(s: &str) -> Vec<String> {
    s.split('|')
        .filter(|k| !k.is_empty())
        .map(str::to_owned)
        .collect()
}

impl Catalog {
    /// The shared catalog, built on the first call (inflating and parsing about 3,000 rows).
    #[must_use]
    pub fn get() -> &'static Catalog {
        CATALOG.get_or_init(|| {
            INITS.fetch_add(1, Ordering::SeqCst);
            // The data is embedded and checked by `scripts/symbols.py --check` and a test, so an
            // error here means a broken build; an empty catalog beats a crash in the Panel.
            Catalog::build().unwrap_or_else(|_| Catalog::empty())
        })
    }

    fn empty() -> Self {
        Self {
            symbols: Vec::new(),
            views: Vec::new(),
            by_id: HashMap::new(),
            emoji_count: 0,
            kaomoji_count: 0,
            icon_count: 0,
            icon_categories: Vec::new(),
            tones: HashMap::new(),
            names: Default::default(),
            art: ArtPack::none(),
        }
    }

    /// Builds a fresh catalog from the embedded data. Prefer [`Catalog::get`].
    ///
    /// # Errors
    /// When a data file is damaged.
    #[allow(clippy::too_many_lines, reason = "one linear load of every data file")]
    pub fn build() -> Result<Self, DataError> {
        let mut symbols = Vec::new();
        for line in data::rows("emoji.tsv.z", data::EMOJI)? {
            let c: Vec<&str> = line.split('\t').collect();
            let [seq, group, _subgroup, name, kws] = c[..] else {
                return Err(bad("emoji.tsv.z", "row without 5 columns"));
            };
            symbols.push(Symbol {
                id: SymbolId::Emoji(seq.to_owned()),
                kind: Kind::Emoji,
                text: Some(data::seq_to_string(seq).ok_or(bad("emoji.tsv.z", "bad sequence"))?),
                name: name.to_owned(),
                group: group.parse().map_err(|_| bad("emoji.tsv.z", "bad group"))?,
                keywords: split_list(kws),
                wide: false,
                icon_markup: None,
            });
        }
        let emoji_count = symbols.len();
        for line in data::rows("kaomoji.tsv.z", data::KAOMOJI)? {
            let c: Vec<&str> = line.split('\t').collect();
            let [id, text, group, tags, wide] = c[..] else {
                return Err(bad("kaomoji.tsv.z", "row without 5 columns"));
            };
            let group: u8 = group
                .parse()
                .map_err(|_| bad("kaomoji.tsv.z", "bad group"))?;
            let mut keywords = split_list(tags);
            let name = keywords.first().cloned().unwrap_or_default();
            if let Some(g) = KAOMOJI_GROUPS.get(usize::from(group)) {
                keywords.push((*g).to_owned());
            }
            symbols.push(Symbol {
                id: SymbolId::Kaomoji(id.to_owned()),
                kind: Kind::Kaomoji,
                text: Some(text.to_owned()),
                name,
                group,
                keywords,
                wide: wide == "1",
                icon_markup: None,
            });
        }
        let kaomoji_count = symbols.len() - emoji_count;
        let mut icon_categories: Vec<String> = Vec::new();
        for line in data::rows("icons.bin.z", data::ICONS)? {
            let c: Vec<&str> = line.split('\t').collect();
            let [name, tags, category, markup] = c[..] else {
                return Err(bad("icons.bin.z", "row without 4 columns"));
            };
            let group = if let Some(i) = icon_categories.iter().position(|x| x == category) {
                i
            } else {
                icon_categories.push(category.to_owned());
                icon_categories.len() - 1
            };
            let mut keywords = split_list(tags);
            if !category.is_empty() {
                keywords.push(category.to_owned());
            }
            symbols.push(Symbol {
                id: SymbolId::Icon(name.to_owned()),
                kind: Kind::Icon,
                text: None,
                name: humanise(name),
                group: u8::try_from(group)
                    .map_err(|_| bad("icons.bin.z", "too many categories"))?,
                keywords,
                wide: false,
                icon_markup: Some(markup.to_owned()),
            });
        }
        let icon_count = symbols.len() - emoji_count - kaomoji_count;
        for line in data::rows("characters.tsv.z", data::CHARACTERS)? {
            let c: Vec<&str> = line.split('\t').collect();
            let [hex, group, name, kws] = c[..] else {
                return Err(bad("characters.tsv.z", "row without 4 columns"));
            };
            let ch = u32::from_str_radix(hex, 16)
                .ok()
                .and_then(char::from_u32)
                .ok_or(bad("characters.tsv.z", "bad code point"))?;
            symbols.push(Symbol {
                id: SymbolId::Char(hex.to_owned()),
                kind: Kind::Character,
                text: Some(ch.to_string()),
                name: name.to_owned(),
                group: group
                    .parse()
                    .map_err(|_| bad("characters.tsv.z", "bad group"))?,
                keywords: split_list(kws),
                wide: false,
                icon_markup: None,
            });
        }
        let views = symbols
            .iter()
            .map(|s| View::new(&s.name, &s.keywords))
            .collect();
        let by_id = symbols
            .iter()
            .enumerate()
            .map(|(i, s)| (s.id.clone(), i))
            .collect();
        let mut catalog = Self {
            symbols,
            views,
            by_id,
            emoji_count,
            kaomoji_count,
            icon_count,
            icon_categories,
            tones: HashMap::new(),
            names: Default::default(),
            art: ArtPack::parse(data::ART)?,
        };
        for line in data::rows("tones.tsv.z", data::TONES)? {
            let (base, variants) = line
                .split_once('\t')
                .ok_or(bad("tones.tsv.z", "row without 2 columns"))?;
            let texts: Vec<String> = variants
                .split('|')
                .filter_map(data::seq_to_string)
                .collect();
            let texts: [String; 5] = texts
                .try_into()
                .map_err(|_| bad("tones.tsv.z", "a row needs five variants"))?;
            let i = catalog
                .index_of(&SymbolId::Emoji(base.to_owned()))
                .ok_or(bad("tones.tsv.z", "unknown base emoji"))?;
            catalog.tones.insert(i, texts);
        }
        Ok(catalog)
    }

    /// Number of symbols of all kinds.
    #[must_use]
    pub fn len(&self) -> usize {
        self.symbols.len()
    }

    /// True for the (broken-build) empty catalog.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.symbols.is_empty()
    }

    /// Every symbol in browse order.
    #[must_use]
    pub fn symbols(&self) -> &[Symbol] {
        &self.symbols
    }

    /// The symbol at a catalog index (the `index` of a [`crate::Hit`]).
    #[must_use]
    pub fn symbol(&self, index: usize) -> Option<&Symbol> {
        self.symbols.get(index)
    }

    /// The catalog index of an id; `None` when the id is not in this data version.
    #[must_use]
    pub fn index_of(&self, id: &SymbolId) -> Option<usize> {
        self.by_id.get(id).copied()
    }

    /// The symbol of an id.
    #[must_use]
    pub fn get_symbol(&self, id: &SymbolId) -> Option<&Symbol> {
        self.index_of(id).and_then(|i| self.symbols.get(i))
    }

    /// The catalog indices of one kind (kinds are contiguous).
    #[must_use]
    pub fn range(&self, kind: Kind) -> Range<usize> {
        match kind {
            Kind::Emoji => 0..self.emoji_count,
            Kind::Kaomoji => self.emoji_count..self.emoji_count + self.kaomoji_count,
            Kind::Icon => {
                self.emoji_count + self.kaomoji_count
                    ..self.emoji_count + self.kaomoji_count + self.icon_count
            }
            Kind::Character => {
                self.emoji_count + self.kaomoji_count + self.icon_count..self.symbols.len()
            }
        }
    }

    /// English icon category names, indexed by an icon's `group`.
    #[must_use]
    pub fn icon_categories(&self) -> &[String] {
        &self.icon_categories
    }

    pub(crate) fn view(&self, index: usize) -> Option<&View> {
        self.views.get(index)
    }

    pub(crate) fn art_pack(&self) -> &ArtPack {
        &self.art
    }

    /// The translated names of every emoji in `lang`, loaded on first use.
    pub(crate) fn lang_names(&self, lang: Lang) -> Option<&LangNames> {
        let slot = lang.slot()?;
        Some(self.names[slot].get_or_init(|| self.load_names(slot)))
    }

    fn load_names(&self, slot: usize) -> LangNames {
        let mut entries: Vec<Option<LangEntry>> = Vec::new();
        entries.resize_with(self.symbols.len(), || None);
        let mut terms: HashMap<String, String> = HashMap::new();
        let mut readings: HashMap<String, String> = HashMap::new();
        let (file, blob) = data::NAMES[slot];
        // A damaged file leaves the language empty: names fall back to English.
        for line in data::rows(file, blob).unwrap_or_default() {
            let c: Vec<&str> = line.split('\t').collect();
            let [key, name, kws] = c[..] else { continue };
            if let Some(term) = key.strip_prefix("t:") {
                terms.insert(term.to_owned(), name.to_owned());
                if !kws.is_empty() {
                    readings.insert(term.to_owned(), kws.to_owned());
                }
                continue;
            }
            let id = key.strip_prefix("c:").map_or_else(
                || SymbolId::Emoji(key.to_owned()),
                |h| SymbolId::Char(h.to_owned()),
            );
            let Some(i) = self.index_of(&id) else {
                continue;
            };
            if let Some(slot) = entries.get_mut(i) {
                let kws = split_list(kws);
                *slot = Some(LangEntry {
                    name: name.to_owned(),
                    view: View::new(name, &kws),
                });
            }
        }
        // Kaomoji and icons: the name and tags are English terms; translate those that have a
        // translation and keep the reading (kana, pinyin) of each as one more keyword.
        for kind in [Kind::Kaomoji, Kind::Icon] {
            for i in self.range(kind) {
                let sym = &self.symbols[i];
                let mut kws: Vec<String> = Vec::new();
                for term in sym.keywords.iter().chain(std::iter::once(&sym.name)) {
                    if let Some(t) = terms.get(term) {
                        kws.push(t.clone());
                        kws.extend(readings.get(term).cloned());
                    }
                }
                if let Some(t) = terms.get(&sym.name) {
                    entries[i] = Some(LangEntry {
                        name: t.clone(),
                        view: View::new(t, &kws),
                    });
                } else if !kws.is_empty() {
                    entries[i] = Some(LangEntry {
                        name: sym.name.clone(),
                        view: View::new(&sym.name, &kws),
                    });
                }
            }
        }
        LangNames { entries, terms }
    }

    /// True when the emoji at `index` has bundled Fluent or flag art; `false` for the ones the
    /// system emoji font draws ("system style") and for every other kind.
    #[must_use]
    pub fn has_art(&self, index: usize) -> bool {
        index < self.emoji_count && self.art.png(index).is_some()
    }

    /// The five uniform skin-tone variants (light to dark) of the emoji at `index`, as text.
    #[must_use]
    pub fn tone_variants(&self, index: usize) -> Option<&[String; 5]> {
        self.tones.get(&index)
    }

    /// The label of a group in `lang`: the emoji group, kaomoji group, icon category or character
    /// group named by `group`. English when the language has no translation.
    #[must_use]
    pub fn group_name(&self, kind: Kind, group: u8, lang: Lang) -> String {
        let english = match kind {
            Kind::Emoji => EMOJI_GROUPS.get(usize::from(group)).copied(),
            Kind::Kaomoji => KAOMOJI_GROUPS.get(usize::from(group)).copied(),
            Kind::Icon => self
                .icon_categories
                .get(usize::from(group))
                .map(String::as_str),
            Kind::Character => CHARACTER_GROUPS.get(usize::from(group)).copied(),
        }
        .unwrap_or("");
        self.lang_names(lang)
            .and_then(|n| n.terms.get(english))
            .map_or_else(|| english.to_owned(), Clone::clone)
    }

    /// The display name of a symbol in `lang` (English when the language has none).
    #[must_use]
    pub fn display_name(&self, index: usize, lang: Lang) -> &str {
        let Some(sym) = self.symbols.get(index) else {
            return "";
        };
        self.lang_names(lang)
            .and_then(|n| n.entries.get(index))
            .and_then(Option::as_ref)
            .map_or(sym.name.as_str(), |e| e.name.as_str())
    }
}
