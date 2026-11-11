# Segmenter dictionaries

ICU's word and line dictionaries for Thai, Lao, Khmer and Burmese
(`segmenter-southeast-asian.dict`) and for Chinese and Japanese word boundaries
(`segmenter-chinese-japanese.dict`). The page fetches one only when reading
text needs it, instead of compiling about 3.7 MB of dictionaries into the wasm;
until it arrives, those scripts break by the plain rules.

They are ICU4X's data (Unicode License v3, `LICENSE-Unicode.txt`) in Genet
parley's blob format, written from ICU's baked data by:

    cargo run -p genet-parley --features complex-scripts \
        --example export_segmenter_dictionaries -- <this directory>

from a Genet checkout at the revision Mere locks.
