//! Chapter extraction.

use std::collections::HashMap;
use std::ffi::CStr;

use ff_format::Rational;
use ff_format::chapter::ChapterInfo;

use super::mapping::pts_to_duration;

/// Extracts all chapters from an `AVFormatContext`.
///
/// # Safety
///
/// The `ctx` pointer must be valid and `avformat_find_stream_info` must have been called.
pub(super) unsafe fn extract_chapters(ctx: *mut ff_sys::AVFormatContext) -> Vec<ChapterInfo> {
    // SAFETY: Caller guarantees ctx is valid
    unsafe {
        let nb_chapters = (*ctx).nb_chapters;
        let chapters_ptr = (*ctx).chapters;

        if chapters_ptr.is_null() || nb_chapters == 0 {
            return Vec::new();
        }

        let mut chapters = Vec::with_capacity(nb_chapters as usize);

        for i in 0..nb_chapters {
            // SAFETY: i < nb_chapters, so this is within bounds
            let chapter = *chapters_ptr.add(i as usize);
            if chapter.is_null() {
                continue;
            }

            chapters.push(extract_single_chapter(chapter));
        }

        chapters
    }
}

/// Extracts information from a single `AVChapter`.
///
/// # Safety
///
/// The `chapter` pointer must be valid.
unsafe fn extract_single_chapter(chapter: *mut ff_sys::AVChapter) -> ChapterInfo {
    // SAFETY: Caller guarantees chapter is valid
    unsafe {
        let id = (*chapter).id;

        let av_tb = (*chapter).time_base;
        let time_base = if av_tb.den != 0 {
            Some(Rational::new(av_tb.num, av_tb.den))
        } else {
            log::warn!(
                "chapter time_base has zero denominator, treating as unknown \
                 chapter_id={id} time_base_num={num} time_base_den=0",
                num = av_tb.num
            );
            None
        };

        let (start, end) = if let Some(tb) = time_base {
            (
                pts_to_duration((*chapter).start, tb),
                pts_to_duration((*chapter).end, tb),
            )
        } else {
            (std::time::Duration::ZERO, std::time::Duration::ZERO)
        };

        let title = extract_chapter_title((*chapter).metadata);
        let metadata = extract_chapter_metadata((*chapter).metadata);

        let mut builder = ChapterInfo::builder().id(id).start(start).end(end);

        if let Some(t) = title {
            builder = builder.title(t);
        }
        if let Some(tb) = time_base {
            builder = builder.time_base(tb);
        }
        if let Some(m) = metadata {
            builder = builder.metadata(m);
        }

        builder.build()
    }
}

/// Extracts the "title" metadata tag from a chapter's `AVDictionary`.
///
/// Returns `None` if the dict is null or the tag is absent.
///
/// # Safety
///
/// `dict` may be null (returns `None`) or a valid `AVDictionary` pointer.
unsafe fn extract_chapter_title(dict: *mut ff_sys::AVDictionary) -> Option<String> {
    // SAFETY: av_dict_get handles null dict by returning null
    unsafe {
        if dict.is_null() {
            return None;
        }
        let entry = ff_sys::av_dict_get(dict, c"title".as_ptr(), std::ptr::null(), 0);
        if entry.is_null() {
            return None;
        }
        let value_ptr = (*entry).value;
        if value_ptr.is_null() {
            return None;
        }
        Some(CStr::from_ptr(value_ptr).to_string_lossy().into_owned())
    }
}

/// Extracts all metadata tags except "title" from a chapter's `AVDictionary`.
///
/// Returns `None` if the dict is null or all tags are filtered out.
///
/// # Safety
///
/// `dict` may be null (returns `None`) or a valid `AVDictionary` pointer.
unsafe fn extract_chapter_metadata(
    dict: *mut ff_sys::AVDictionary,
) -> Option<HashMap<String, String>> {
    // SAFETY: av_dict_get handles null dict by returning null
    unsafe {
        if dict.is_null() {
            return None;
        }

        let mut map = HashMap::new();
        let mut entry: *const ff_sys::AVDictionaryEntry = std::ptr::null();
        let flags = ff_sys::AV_DICT_IGNORE_SUFFIX.cast_signed();

        loop {
            entry = ff_sys::av_dict_get(dict, c"".as_ptr(), entry, flags);
            if entry.is_null() {
                break;
            }

            let key_ptr = (*entry).key;
            let value_ptr = (*entry).value;

            if key_ptr.is_null() || value_ptr.is_null() {
                continue;
            }

            let key = CStr::from_ptr(key_ptr).to_string_lossy().into_owned();
            if key == "title" {
                continue;
            }
            let value = CStr::from_ptr(value_ptr).to_string_lossy().into_owned();
            map.insert(key, value);
        }

        if map.is_empty() { None } else { Some(map) }
    }
}
