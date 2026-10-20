"""Build pre-rename release aliases: legacy-assets.py <dist> <release-tag>."""

import hashlib
import re
import shutil
import sys
import zipfile
from pathlib import Path

dist, tag = Path(sys.argv[1]), sys.argv[2]
if not re.fullmatch(r"v[0-9]+\.[0-9]+\.[0-9]+(?:[-+][0-9A-Za-z.+-]+)?", tag):
    raise ValueError("expected a release tag such as v0.10.5")

for os in ("windows", "macos", "linux"):
    source = dist / f"tacit-{os}.zip"
    target = dist / f"typescript-{os}.zip"
    if os == "windows":
        # Old updaters pass -LocalSource; let the official installer fetch the program and packs.
        installer = f"""param([switch]$LocalSource)
$ErrorActionPreference = 'Stop'
& ([scriptblock]::Create((Invoke-RestMethod 'https://github.com/iamallyouneed/Tacit/releases/download/{tag}/install.ps1'))) -Version '{tag}' -Languages 'en,ko'
"""
        with zipfile.ZipFile(source) as original:
            original.getinfo("install.ps1")
            with zipfile.ZipFile(target, "w") as legacy:
                for entry in original.infolist():
                    data = installer.encode("ascii") if entry.filename == "install.ps1" else original.read(entry)
                    legacy.writestr(entry, data)
    else:
        shutil.copyfile(source, target)
    checksum = hashlib.sha256(target.read_bytes()).hexdigest()
    target.with_suffix(".zip.sha256").write_text(f"{checksum}  {target.name}\n", encoding="ascii")
