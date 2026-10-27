# Third-Party Components

G'MIC Video's own source code is licensed under the GPL-3.0 (see [`LICENSE`](LICENSE)).
This repository and the released AppImage also bundle the following third-party software,
which keeps its own license:

| Component | Location | License |
| --- | --- | --- |
| [G'MIC](https://gmic.eu/) 4.0.6 (command-line interpreter) | `bin/gmic`, `bin/gmic.bin` | [CeCILL](https://cecill.info/licences/Licence_CeCILL_V2.1-en.html) (GPL-compatible) |
| `libgmic` 4.0.6 | `lib/gmic/libgmic.so*` | [CeCILL-C](https://cecill.info/licences/Licence_CeCILL-C_V1-en.html) (LGPL-like) |
| G'MIC filter definitions (`update406.gmic`) | `assets/gmic/update406.gmic` | CeCILL |
| Shared libraries shipped with G'MIC (e.g. OpenEXR / Imath) | `lib/gmic/` | Their respective upstream licenses (BSD-3-Clause for OpenEXR / Imath) |

G'MIC is developed by the IMAGE team of the GREYC laboratory (CNRS, ENSICAEN, Université de Caen).
Source code: <https://github.com/GreycLab/gmic>.

Runtime dependencies that are **not** bundled (installed from your distribution): Python 3, PySide6
(LGPL-3.0), KDE Kirigami (LGPL-2.0+), FFmpeg (LGPL-2.1+/GPL-2.0+). Rust crate dependencies are listed
in `Cargo.toml` / `Cargo.lock` and are under their own (mostly MIT / Apache-2.0) licenses.
