#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(mut tmp) = tempfile::NamedTempFile::new() else {
        return;
    };
    use std::io::Write as _;
    let _ = tmp.write_all(data);
    let _ = tmp.flush();
    // Must not panic or abort — only return Err.
    let _ = ff_decode::VideoDecoder::open(tmp.path()).build();
});
