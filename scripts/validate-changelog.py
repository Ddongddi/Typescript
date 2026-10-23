"""Check both changelogs before CI and release publication. Standard library only."""
import argparse
from datetime import date
from pathlib import Path
import re


SECTIONS = {
    "ko": {"추가", "변경", "수정", "제거", "보안", "패키지", "사용 중단"},
    "en": {"Added", "Changed", "Fixed", "Removed", "Security", "Packaging", "Deprecated"},
}


def entries(text, language):
    prose = re.sub(r"`[^`\n]*`", "", text)
    if "\ufffd" in text or re.search(r"(?<!\S)\?{2,}(?!\S)", prose):
        raise ValueError(f"{language}: broken characters in changelog")
    versions = []
    content = []
    current = None
    for line in text.splitlines():
        if line.startswith("## "):
            if current and current != "Unreleased" and not any(s.startswith("- ") for s in content):
                raise ValueError(f"{language}: empty release {current}")
            match = re.fullmatch(r"## \[(Unreleased|\d+\.\d+\.\d+)\](?: - (\d{4}-\d{2}-\d{2}))?|## (0\.1\.0)", line)
            if not match:
                raise ValueError(f"{language}: invalid version heading {line}")
            current = match[1] or match[3]
            stamp = match[2]
            if current not in ("Unreleased", "0.1.0"):
                if not stamp:
                    raise ValueError(f"{language}: missing release date for {current}")
                date.fromisoformat(stamp)
            versions.append((current, stamp))
            content = []
        elif line.startswith("### ") and line[4:] not in SECTIONS[language]:
            raise ValueError(f"{language}: invalid section heading {line}")
        elif current:
            content.append(line)
    if current and current != "Unreleased" and not any(s.startswith("- ") for s in content):
        raise ValueError(f"{language}: empty release {current}")
    if not versions or versions[0][0] != "Unreleased":
        raise ValueError(f"{language}: missing Unreleased heading")
    if len({v for v, _ in versions}) != len(versions):
        raise ValueError(f"{language}: duplicate version")
    released = [tuple(map(int, v.split("."))) for v, _ in versions if v != "Unreleased"]
    if released != sorted(released, reverse=True):
        raise ValueError(f"{language}: versions must be newest first")
    return versions


def validate(korean, english, tag=None):
    ko, en = entries(korean, "ko"), entries(english, "en")
    if ko != en:
        raise ValueError("Korean and English version lists or dates differ")
    if tag and (len(ko) < 2 or ko[1][0] != tag.removeprefix("v")):
        raise ValueError(f"{tag}: tag must match the newest changelog release")


def selftest():
    ko = "## [Unreleased]\n\n## [1.0.0] - 2026-10-22\n### 수정\n- 고침\n"
    en = "## [Unreleased]\n\n## [1.0.0] - 2026-10-22\n### Fixed\n- Fixed\n"
    validate(ko, en, "v1.0.0")
    validate(ko.replace("고침", "`value ?? fallback`"), en)
    for bad_ko, bad_en, tag in [
        (ko.replace("수정", "??"), en, None),
        (ko.replace("고침", "??"), en, None),
        (ko.replace("고침", "\ufffd"), en, None),
        (ko.replace("- 고침", ""), en, None),
        (ko, en.replace("1.0.0", "1.0.1"), None),
        (ko, en, "v9.0.0"),
        (ko.replace("### 수정", "### broken"), en, None),
    ]:
        try:
            validate(bad_ko, bad_en, tag)
        except ValueError:
            continue
        raise AssertionError("broken changelog passed validation")
    print("Changelog self-tests passed.")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--tag")
    parser.add_argument("--selftest", action="store_true")
    args = parser.parse_args()
    if args.selftest:
        selftest()
    root = Path(__file__).resolve().parent.parent
    try:
        validate((root / "CHANGELOG.md").read_text(encoding="utf-8"),
                 (root / "CHANGELOG.en.md").read_text(encoding="utf-8"), args.tag)
    except (ValueError, UnicodeError) as error:
        parser.exit(1, f"Changelog validation failed: {error}\n")
    print("Changelogs are valid and match.")
