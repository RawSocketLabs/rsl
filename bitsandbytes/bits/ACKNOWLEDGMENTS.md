# Acknowledgments

## binrw

`bits` is built on, and deeply inspired by,
[**binrw**](https://github.com/jam1garner/binrw) by **jam1garner** and its
contributors (MIT-licensed). binrw's declarative, bidirectional `BinRead`/
`BinWrite` model — and the ergonomics of its attribute surface (`magic`, `args`,
`map`, `calc`, `temp`, `count`, `if`, `parse_with`, …) — set the bar this crate
aims at, and `bits` uses binrw directly as its byte-stream codec (the default
`binrw` feature; the `#[wire]` macro lowers to `#[binrw]`).

The `bitstream` module (a bit-level cursor codec) is an **independent,
from-scratch implementation** — it copies no binrw source — written to cover the
one thing binrw's byte-oriented `Read + Seek` model cannot: fields at arbitrary
*bit* offsets. Its API and attribute design intentionally echo binrw's so the two
feel like one toolkit. If any binrw source is ever vendored in, its MIT copyright
and license text will be retained alongside it.

binrw is an excellent crate; this work would not exist without it. Thank you.
