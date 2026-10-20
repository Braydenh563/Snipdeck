# backend-2
Done: B6 (replace_key_record_checked + previous_key_record, schema v6).
Next: B5, B12.
Blocked/notes: snipdeck-app still calls unchecked replace_key_record (lock_ctl.rs 513/534/550, controller.rs trait); migrate to replace_key_record_checked(&record, &key) using the key from rekey, then drop the unchecked one.
