//! The Symbols tab in the Panel window: turns [`crate::symbols_panel::Grid`]
//! into the window's rows, keeps the selection and the picture cache. No controller here: the
//! binding passes in the recents and the language, and runs the uses itself.

use std::{cell::RefCell, collections::HashMap};

use slint::{Global, ModelRc, SharedString, VecModel};
use snipdeck_symbols::{Catalog, KAOMOJI_GROUPS, Kind, Lang, SymbolId};
use snipdeck_ui::{self as ui, symbol_nav};

use crate::symbols_panel::{self, Grid, Title};

/// Pixel size of the art asked of the catalog (its native size: no resampling).
const ART_PX: u32 = 48;
/// Most pictures kept before the cache starts over.
const ART_CACHE_MAX: usize = 1200;
/// The ink icons are drawn in before the theme colours them (`colorize` keeps the alpha).
const ICON_INK: [u8; 4] = [255, 255, 255, 255];

/// The text colour of the theme (`text.primary`), for an icon copied or saved as a picture.
#[must_use]
pub const fn ink(dark: bool) -> [u8; 4] {
    if dark {
        [0xEB, 0xE9, 0xE7, 0xFF]
    } else {
        [0x17, 0x16, 0x13, 0xFF]
    }
}

/// What the tab is showing: kind, query, selection and the grid they make.
#[derive(Default)]
pub struct Glue {
    inner: RefCell<Inner>,
    art: RefCell<HashMap<usize, slint::Image>>,
}

#[derive(Default)]
struct Inner {
    tab: i32,
    query: String,
    selected: usize,
    columns: usize,
    grid: Grid,
    top_ticks: i32,
    top_row: i32,
    lang: Option<Lang>,
}

impl Glue {
    /// The tab was opened: a clean query, the first cell selected.
    pub fn opened(&self, w: &ui::PanelWindow, recents: &[SymbolId], lang: Lang) {
        {
            let mut s = self.inner.borrow_mut();
            s.query.clear();
            s.selected = 0;
        }
        self.rebuild(w, recents, lang);
    }

    /// The search text changed.
    pub fn query(&self, w: &ui::PanelWindow, query: &str, recents: &[SymbolId], lang: Lang) {
        {
            let mut s = self.inner.borrow_mut();
            query.clone_into(&mut s.query);
            s.selected = 0;
        }
        self.rebuild(w, recents, lang);
    }

    /// A kind tab was chosen.
    pub fn kind(&self, w: &ui::PanelWindow, tab: i32, recents: &[SymbolId], lang: Lang) {
        {
            let mut s = self.inner.borrow_mut();
            s.tab = tab.clamp(0, 3);
            s.selected = 0;
        }
        self.rebuild(w, recents, lang);
    }

    /// The window now fits `columns` cells across.
    pub fn columns(&self, w: &ui::PanelWindow, columns: i32, recents: &[SymbolId], lang: Lang) {
        let columns = usize::try_from(columns).unwrap_or(1).max(1);
        if self.inner.borrow().columns == columns {
            return;
        }
        {
            let mut s = self.inner.borrow_mut();
            s.columns = columns;
            // The same symbol stays selected through the new layout.
            s.selected = s.selected.min(s.grid.items.len().saturating_sub(1));
        }
        self.rebuild(w, recents, lang);
    }

    /// An arrow, Home, End or page key. Up from the first row leaves the selection where it is.
    pub fn mv(&self, w: &ui::PanelWindow, code: i32, page: usize) {
        let Some(mv) = symbol_nav::Move::from_code(code) else {
            return;
        };
        {
            let mut s = self.inner.borrow_mut();
            let Some(next) =
                symbol_nav::navigate(&s.grid.nav, s.grid.items.len(), s.selected, mv, page)
            else {
                return;
            };
            s.selected = next;
        }
        self.push_selection(w);
    }

    /// A cell was chosen with the pointer.
    pub fn select(&self, w: &ui::PanelWindow, flat: i32) {
        let flat = usize::try_from(flat).unwrap_or(0);
        {
            let mut s = self.inner.borrow_mut();
            s.selected = flat.min(s.grid.items.len().saturating_sub(1));
        }
        self.push_selection(w);
    }

    /// A group button of the emoji strip: the group goes to the top and its first cell is selected.
    pub fn group(&self, w: &ui::PanelWindow, group: i32) {
        {
            let mut s = self.inner.borrow_mut();
            let Some(Some((row, first))) = usize::try_from(group)
                .ok()
                .and_then(|g| s.grid.groups.get(g).copied())
            else {
                return;
            };
            s.selected = first;
            s.top_row = i32::try_from(row).unwrap_or(-1);
            s.top_ticks += 1;
        }
        self.push_selection(w);
        let s = self.inner.borrow();
        w.set_symbol_top_row(s.top_row);
        w.set_symbol_top_ticks(s.top_ticks);
    }

    /// The symbol in cell `flat`.
    #[must_use]
    pub fn id_at(&self, flat: i32) -> Option<SymbolId> {
        let flat = usize::try_from(flat).ok()?;
        let index = *self.inner.borrow().grid.items.get(flat)?;
        Some(Catalog::get().symbol(index)?.id.clone())
    }

    /// The picture of catalog entry `index` (an empty image when it has none).
    #[must_use]
    pub fn art(&self, index: i32) -> slint::Image {
        let Ok(index) = usize::try_from(index) else {
            return slint::Image::default();
        };
        if let Some(image) = self.art.borrow().get(&index) {
            return image.clone();
        }
        let image = load_art(index).unwrap_or_default();
        let mut cache = self.art.borrow_mut();
        if cache.len() >= ART_CACHE_MAX {
            cache.clear();
        }
        cache.insert(index, image.clone());
        image
    }

    fn rebuild(&self, w: &ui::PanelWindow, recents: &[SymbolId], lang: Lang) {
        let rows = {
            let mut s = self.inner.borrow_mut();
            if s.columns == 0 {
                s.columns = 7;
            }
            s.lang = Some(lang);
            s.grid =
                symbols_panel::build(Catalog::get(), s.tab, &s.query, lang, recents, s.columns);
            s.selected = s.selected.min(s.grid.items.len().saturating_sub(1));
            rows_of(&s.grid, &ui::SymbolTexts::get(w), lang)
        };
        let query = self.inner.borrow().query.clone();
        let tab = self.inner.borrow().tab;
        let t = ui::SymbolTexts::get(w);
        let empty = self.inner.borrow().grid.items.is_empty();
        w.set_symbol_rows(ModelRc::new(VecModel::from(rows)));
        w.set_symbol_kind(tab);
        if w.get_symbol_query() != query.as_str() {
            w.set_symbol_query(query.as_str().into());
        }
        w.set_symbol_empty_heading(if empty && !query.is_empty() {
            t.invoke_no_match(query.as_str().into())
        } else {
            SharedString::new()
        });
        w.set_symbol_empty_body(if empty && !query.is_empty() {
            t.invoke_no_match_body()
        } else {
            SharedString::new()
        });
        self.push_selection_with(w, lang);
    }

    fn push_selection(&self, w: &ui::PanelWindow) {
        let lang = self.inner.borrow().lang.unwrap_or(Lang::En);
        self.push_selection_with(w, lang);
    }

    fn push_selection_with(&self, w: &ui::PanelWindow, lang: Lang) {
        let s = self.inner.borrow();
        let row = s
            .grid
            .row_of(s.selected)
            .and_then(|r| i32::try_from(r).ok());
        w.set_symbol_selected(i32::try_from(s.selected).unwrap_or(0));
        w.set_symbol_selected_row(row.unwrap_or(-1));
        let name = s
            .grid
            .items
            .get(s.selected)
            .map(|&i| label_of(i, lang))
            .unwrap_or_default();
        w.set_symbol_selected_name(name.into());
    }
}

fn rows_of(grid: &Grid, t: &ui::SymbolTexts<'_>, lang: Lang) -> Vec<ui::SymbolRow> {
    grid.rows
        .iter()
        .map(|row| {
            let header = row
                .title
                .as_ref()
                .map_or_else(String::new, |title| title_text(title, t).to_uppercase());
            let link = row
                .link
                .map_or_else(String::new, |_| t.invoke_show_all().to_string());
            let cells: Vec<ui::SymbolCell> = row
                .cells
                .iter()
                .filter_map(|cell| {
                    let sym = Catalog::get().symbol(cell.index)?;
                    let kind = sym.id.kind();
                    Some(ui::SymbolCell {
                        art: if kind == Kind::Kaomoji {
                            -1
                        } else {
                            i32::try_from(cell.index).unwrap_or(-1)
                        },
                        text: sym.text.clone().unwrap_or_default().into(),
                        label: label_of(cell.index, lang).into(),
                        kind: match kind {
                            Kind::Emoji => 0,
                            Kind::Kaomoji => 1,
                            Kind::Icon => 2,
                        },
                        wide: cell.wide,
                        flat: i32::try_from(cell.flat).unwrap_or(0),
                    })
                })
                .collect();
            ui::SymbolRow {
                header: header.into(),
                link: link.into(),
                link_kind: row.link.map_or(-1, |k| match k {
                    Kind::Emoji => 1,
                    Kind::Kaomoji => 2,
                    Kind::Icon => 3,
                }),
                cells: ModelRc::new(VecModel::from(cells)),
                chips: row.chips,
                y: row.y,
                h: row.h,
            }
        })
        .collect()
}

/// What a step that rebuilds the grid is given: the glue, the window, the recents and the language.
pub type Step<'a> = &'a dyn Fn(&Glue, &ui::PanelWindow, &[SymbolId], Lang);

fn title_text(title: &Title, t: &ui::SymbolTexts<'_>) -> String {
    match title {
        Title::Recent => t.invoke_recently_used().to_string(),
        Title::Kind(k) => t
            .invoke_kind_name(match k {
                Kind::Emoji => 0,
                Kind::Kaomoji => 1,
                Kind::Icon => 2,
            })
            .to_string(),
        Title::EmojiGroup(g) => t
            .invoke_emoji_group(i32::try_from(*g).unwrap_or(0))
            .to_string(),
        Title::KaomojiGroup(g) if *g < KAOMOJI_GROUPS.len() => t
            .invoke_kaomoji_group(i32::try_from(*g).unwrap_or(0))
            .to_string(),
        Title::None | Title::KaomojiGroup(_) => String::new(),
        Title::IconCategory(name) => capitalise(name),
    }
}

fn capitalise(name: &str) -> String {
    let mut chars = name.chars();
    chars
        .next()
        .map_or_else(String::new, |c| c.to_uppercase().chain(chars).collect())
}

/// The accessible name of a symbol: the emoji's name in the UI language, a kaomoji's first tag,
/// an icon's name with spaces.
#[must_use]
pub fn label_of(index: usize, lang: Lang) -> String {
    let catalog = Catalog::get();
    let Some(sym) = catalog.symbol(index) else {
        return String::new();
    };
    match sym.id.kind() {
        Kind::Emoji => catalog.display_name(index, lang).to_owned(),
        Kind::Kaomoji => sym
            .keywords
            .first()
            .cloned()
            .unwrap_or_else(|| sym.name.clone()),
        Kind::Icon => sym.name.replace('-', " "),
    }
}

fn load_art(index: usize) -> Option<slint::Image> {
    let catalog = Catalog::get();
    let sym = catalog.symbol(index)?;
    let (rgba, px) = match sym.id.kind() {
        Kind::Emoji => (catalog.art(&sym.id, ART_PX)?, ART_PX),
        Kind::Icon => {
            let png = catalog.icon_png(&sym.id, ART_PX, ICON_INK)?;
            let mut reader = png::Decoder::new(std::io::Cursor::new(png.as_slice()))
                .read_info()
                .ok()?;
            let mut buf = vec![0; reader.output_buffer_size()?];
            let info = reader.next_frame(&mut buf).ok()?;
            buf.truncate(info.buffer_size());
            (buf, ART_PX)
        }
        Kind::Kaomoji => return None,
    };
    let buffer = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::clone_from_slice(&rgba, px, px);
    Some(slint::Image::from_rgba8(buffer))
}
