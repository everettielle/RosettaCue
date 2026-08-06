Media tool staging directory
============================

RosettaCue shells out to three external executables for Blu-ray source analysis
and PGS extraction. They are NOT bundled in this repository. Place them here (or
anywhere on your PATH) before running source analysis or extraction.

Required names:
- macOS/Linux: bd_list_titles, bd_splice, ffmpeg
- Windows:     bd_list_titles.exe, bd_splice.exe, ffmpeg.exe

Where they come from:
- bd_list_titles, bd_splice  example utilities built from libbluray (LGPL-2.1)
                             https://www.videolan.org/developers/libbluray.html
- ffmpeg                     https://ffmpeg.org/ (LGPL or GPL, build dependent)

At runtime RosettaCue resolves tools from the directory named by the
ROSETTACUE_MEDIA_TOOLS_DIR environment variable, then from PATH. Packaged builds
copy this directory to <resources>/tools and set that variable automatically.
Verify what is being resolved with:

    cargo run -p rosettacue-cli -- doctor

Distribution
------------

The contents of this directory are gitignored. Do not commit third-party
binaries. If you redistribute a packaged RosettaCue build that contains these
executables, you must comply with their licenses and include their notices and
source/build provenance.
