# rsl-error-correction — design

Error correction is not an ordinary reversible byte transform. Encoding adds redundancy and
decoding may return clean data, corrected data with a count, or an uncorrectable error. The public
contract preserves those outcomes rather than flattening them into `bytes -> bytes`.
