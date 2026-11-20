"""Extract the MediaX test fixture from the SHA-256-pinned KANALI 2.7.0 installer."""
import lzma
from pathlib import Path
import sys

installer, target = map(Path, sys.argv[1:])
target.mkdir(parents=True, exist_ok=True)
# Offsets and compressed lengths of the x64 codec blocks in this installer.
# Extract only the converter dependencies. No device service or driver is needed.
blocks = [
    ("MediaX.dll", 124809979, 324084),
    ("avcodec-62.dll", 11828889, 26109835),
    ("avfilter-11.dll", 38344299, 9493622),
    ("avformat-62.dll", 55054647, 7552807),
    ("avutil-60.dll", 62970332, 846589),
    ("swresample-6.dll", 144713141, 263633),
    ("swscale-9.dll", 145120553, 230573),
]
with installer.open("rb") as source:
    for name, offset, length in blocks:
        source.seek(offset)
        decoder = lzma.LZMADecompressor(
            format=lzma.FORMAT_RAW,
            filters=[{"id": lzma.FILTER_LZMA1, "dict_size": 1 << 23}],
        )
        data = decoder.decompress(source.read(length))
        if not data.startswith(b"MZ"):
            raise ValueError(f"Invalid KANALI fixture: {name}")
        (target / name).write_bytes(data)
