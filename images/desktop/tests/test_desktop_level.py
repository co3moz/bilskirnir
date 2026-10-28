"""The desktop level both images carry in their thoric.desktop-level label, which a manager compares
with the levels it handles before it starts anything from them. A change to the images that raises
the level must raise DESKTOP_LEVEL in the manager with it, or the manager built with these images
would refuse them."""

import re
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[3]
DOCKERFILE = ROOT / "images/desktop/Dockerfile"
CATALOG = ROOT / "core/manager/src/catalog.rs"


def stage_levels():
    """The level each final stage of the Dockerfile labels its image with."""
    levels, stage = {}, None
    for line in DOCKERFILE.read_text().splitlines():
        if match := re.match(r"FROM \S+ AS (\S+)$", line):
            stage = match.group(1)
        elif match := re.match(r"LABEL thoric\.desktop-level=(\S+)$", line):
            levels[stage] = match.group(1)
    return levels


class DesktopLevelTest(unittest.TestCase):
    def test_both_images_carry_the_level_the_manager_handles(self):
        manager = re.search(r"^pub const DESKTOP_LEVEL: u32 = (\d+);$", CATALOG.read_text(), re.M)
        self.assertIsNotNone(manager, "DESKTOP_LEVEL is missing from catalog.rs")
        self.assertEqual(stage_levels(), {"xfce": manager.group(1), "gnome": manager.group(1)})


if __name__ == "__main__":
    unittest.main()
