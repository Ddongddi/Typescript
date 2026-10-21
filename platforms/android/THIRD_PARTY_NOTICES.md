# Android keyboard attribution

The top-right hint positioning in `DualKeyboardView.java` is adapted from
[HeliBoard v4.2 KeyboardView.java](https://github.com/HeliBorg/HeliBoard/blob/0c60dffb03a4e2e7a37cb82210f5299f93c03979/app/src/main/java/helium314/keyboard/keyboard/KeyboardView.java).
Upstream marks that file `Apache-2.0 AND GPL-3.0-only`:

Copyright (C) 2010 The Android Open Source Project; modified by HeliBoard contributors.
Tacit adapts the phone hint's measured glyph width and font-ascent positioning to
the framework KeyboardView and the existing Korean two-set legends.

License texts: [Apache License 2.0](licenses/Apache-2.0.txt),
[GNU GPL 3.0](licenses/GPL-3.0-only.txt).
Tacit's remaining code is covered by the repository's AGPL-3.0-only license.

HeliBoard's input engine, dictionaries, settings, and gesture libraries are not included.
The rounded key backgrounds and light/dark palettes are original Tacit resources.
