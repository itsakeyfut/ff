//! Container-level metadata extraction.

use std::collections::HashMap;
use std::ffi::CStr;

/// Extracts container-level metadata from an `AVFormatContext`.
///
/// This function reads all metadata entries from the container's `AVDictionary`,
/// including standard keys (title, artist, album, date, etc.) and custom metadata.
///
/// # Safety
///
/// The `ctx` pointer must be valid.
///
/// # Returns
///
/// Returns a `HashMap` containing all metadata key-value pairs.
/// If no metadata is present, returns an empty `HashMap`.
pub(super) unsafe fn extract_metadata(
    ctx: *mut ff_sys::AVFormatContext,
) -> HashMap<String, String> {
    let mut metadata = HashMap::new();

    // SAFETY: Caller guarantees ctx is valid
    unsafe {
        let dict = (*ctx).metadata;
        if dict.is_null() {
            return metadata;
        }

        // Iterate through all dictionary entries using av_dict_get with AV_DICT_IGNORE_SUFFIX
        // This iterates all entries when starting with an empty key and passing the previous entry
        let mut entry: *const ff_sys::AVDictionaryEntry = std::ptr::null();

        // AV_DICT_IGNORE_SUFFIX is a small constant (2) that safely fits in i32
        let flags = ff_sys::AV_DICT_IGNORE_SUFFIX.cast_signed();

        loop {
            // Get the next entry by passing the previous one
            // Using empty string as key and AV_DICT_IGNORE_SUFFIX to iterate all entries
            entry = ff_sys::av_dict_get(dict, c"".as_ptr(), entry, flags);

            if entry.is_null() {
                break;
            }

            // Extract key and value from the entry
            let key_ptr = (*entry).key;
            let value_ptr = (*entry).value;

            if key_ptr.is_null() || value_ptr.is_null() {
                continue;
            }

            // Convert C strings to Rust strings
            let key = CStr::from_ptr(key_ptr).to_string_lossy().into_owned();
            let value = CStr::from_ptr(value_ptr).to_string_lossy().into_owned();

            metadata.insert(key, value);
        }
    }

    metadata
}
