//! Alone in its binary so the shared catalog is untouched when the test starts.

use snipdeck_symbols::{Catalog, catalog_init_count};

#[test]
fn nothing_is_parsed_before_first_use() {
    assert_eq!(
        catalog_init_count(),
        0,
        "nothing may build the catalog at start-up"
    );
    let first = Catalog::get();
    assert!(!first.is_empty());
    assert_eq!(catalog_init_count(), 1);
    let second = Catalog::get();
    assert!(std::ptr::eq(first, second));
    assert_eq!(catalog_init_count(), 1, "built once");
}
