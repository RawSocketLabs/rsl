pub fn bytes<'a>(pointer: *const u8, length: usize) -> &'a [u8] {
    // SAFETY: The caller supplied a pointer and length.
    unsafe { std::slice::from_raw_parts(pointer, length) }
}
