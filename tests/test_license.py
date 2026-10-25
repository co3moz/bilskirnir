"""Bilskirnir's own license, GPL-3.0-or-later: that every place that states it states the same
license, and that every image and package a release distributes carries its notice.
"""

import json
from pathlib import Path
import re
import unittest

ROOT = Path(__file__).resolve().parents[1]
SPDX = "GPL-3.0-or-later"
OR_LATER = "either version 3 of the License, or (at your option) any later version"


def read(path):
    return (ROOT / path).read_text(encoding="utf-8")


def stages(dockerfile):
    """Each stage of a Dockerfile by its name, with its instructions."""
    found = {}
    for block in re.split(r"^FROM ", read(dockerfile), flags=re.M)[1:]:
        name = re.match(r"\S+(?:\s+AS\s+(\S+))?", block, re.I).group(1)
        found[name or ""] = block
    return found


class LicenseTest(unittest.TestCase):
    def test_the_license_file_is_the_gplv3(self):
        text = read("LICENSE")
        self.assertIn("GNU GENERAL PUBLIC LICENSE", text.splitlines()[0])
        self.assertIn("Version 3, 29 June 2007", text.splitlines()[1])
        self.assertIn("END OF TERMS AND CONDITIONS", text)

    def test_every_license_field_names_the_same_license(self):
        self.assertRegex(read("Cargo.toml"), rf'(?m)^license = "{re.escape(SPDX)}"$')
        self.assertEqual(json.loads(read("web/package.json"))["license"], SPDX)
        self.assertIn(f'org.opencontainers.image.licenses="{SPDX}"', read("Dockerfile"))

    def test_every_notice_names_version_3_or_later(self):
        for path in ("README.md", "desktop/copyright"):
            self.assertIn(OR_LATER, " ".join(read(path).split()), path)
        self.assertIn("version 3 or later", read("THIRD_PARTY_NOTICES.md"))
        # The launcher is attached to releases on its own, away from the repository's LICENSE.
        self.assertIn("GNU General Public\n# License, version 3 or later", read("windows/bilskirnir.ps1").replace("\r\n", "\n"))

    def test_every_image_and_package_carries_the_notice(self):
        self.assertIn("COPY LICENSE /usr/share/doc/bilskirnir/LICENSE", read("Dockerfile"))
        self.assertIn("COPY --chmod=0644 LICENSE /usr/share/doc/bilskirnir/LICENSE", read("wsl/Dockerfile"))
        desktop = stages("desktop/Dockerfile")
        self.assertIn("COPY copyright /usr/share/doc/bilskirnir/copyright", desktop["scripts"])
        for image in ("xfce", "gnome"):
            self.assertIn("COPY --from=scripts /usr/share/doc/bilskirnir/ /usr/share/doc/bilskirnir/", desktop[image], image)


if __name__ == "__main__":
    unittest.main()
