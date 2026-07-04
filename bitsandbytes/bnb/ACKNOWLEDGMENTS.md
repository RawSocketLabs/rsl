# Acknowledgments

`bnb` collapses the capabilities of several excellent crates into one. It shares no
code with any of them — it is a from-scratch implementation — but their designs
shaped it, and credit is due.

## binrw

`bnb`'s codec is modeled on [**binrw**](https://github.com/jam1garner/binrw) by
**jam1garner** and its contributors (MIT-licensed). binrw's declarative,
bidirectional `BinRead`/`BinWrite` model — and the ergonomics of its attribute
surface (`magic`, `args`, `map`, `calc`, `temp`, `count`, `if`, `parse_with`, …) —
set the bar this crate aims at. `#[bin]`'s `#[br]`/`#[bw]` attribute vocabulary
deliberately echoes binrw's, so where a spelling is reused it means what binrw means
and a binrw user is immediately at home.

The codec is an **independent, from-scratch implementation** — it copies no binrw
source — extended to do the one thing a byte-oriented `Read + Seek` codec cannot:
read and write fields at arbitrary *bit* offsets. binrw is an excellent crate, and
its design made this one much better. Thank you.

## The bit/int/enum crates

`bnb` also draws on the crates whose capabilities it folds together:

- [**arbitrary-int**](https://crates.io/crates/arbitrary-int) — the arbitrary-width
  integers (`u1`..`u127`).
- [**modular-bitfield**](https://crates.io/crates/modular-bitfield) /
  [**bitfield-struct**](https://crates.io/crates/bitfield-struct) /
  [**bitbybit**](https://crates.io/crates/bitbybit) — bitfield packing with named
  accessors and bit-range placement.
- [**num_enum**](https://crates.io/crates/num_enum) — the enum ⇄ integer mapping and
  the catch-all / `TryFrom` conventions.

Thanks to their authors and contributors.
