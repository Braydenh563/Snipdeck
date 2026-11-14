# backend-2
Done: B6, B5, B12 (1)(2)(3). All checks green (fmt, clippy -D warnings, tests, doc) for store/portable/core.
Next: nothing in scope.
Notes for the app (not edited):
- B6: snipdeck-app still calls the unchecked `replace_key_record` (controller.rs trait + lock_ctl.rs 513/534/550). Switch to `replace_key_record_checked(&record, &key)` with the key from `KeyRing::rekey`, then drop the unchecked one. `change_master_password` in the store stays on the unchecked path (record made in-process).
- B12(3): `licence::verify(token, key)` lost `now`. `verify_embedded(token, _now)` keeps its parameter only because snipdeck-app (settings_ctl.rs:333, licence_store.rs:46) passes it; drop it when the app is updated.
- B5: `ItemRepo::plain_formats` has a default impl so app's `NoCount` wrapper compiles.
