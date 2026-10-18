# ARIB encoding map

`../../src/arib_text/encoding.bin` contains the 6,647 Unicode ranges from
TSDuck's `src/libtsduck/dtv/charset/tsARIBCharsetEncoding.cpp`, commit
`f3e026e927c5b054649ff4e5d5a6ba2eabc1ce8c` (BSD-2-Clause).
Source: https://github.com/tsduck/tsduck/blob/f3e026e927c5b054649ff4e5d5a6ba2eabc1ce8c/src/libtsduck/dtv/charset/tsARIBCharsetEncoding.cpp

Each eight-byte record stores the Unicode range start and the original packed
encoding entry as two big-endian unsigned 32-bit integers. The packed entry
contains the character-set selector, double-byte flag, row, first cell, and
range length. No broadcast captures or programme text are included.
The adjacent `tsduck-LICENSE` retains the upstream license.

Regenerate offline from a checkout containing the pinned commit:

```sh
python3 tools/generate-arib-encoding.py /path/to/tsduck
```

The Rust encoder selects the additional-symbol character set for mapped rows
0x75 and above, emits self-contained G0 designations, and handles text splitting.
It normalizes fullwidth tilde and hyphen and expands the modern programme marks
U+1F19B through U+1F1AC to KonomiTV's textual labels before encoding.
