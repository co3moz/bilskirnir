"""desktop-private-address, which tells desktop-session the one address VNC listens on: the
desktop's address in the private network the manager names, never one of the internet network
or another interface, and nothing when the private network no longer has an address."""

import contextlib
import importlib.machinery
import importlib.util
import io
import ipaddress
from pathlib import Path
import sys
import unittest
from unittest import mock

ROOT = Path(__file__).resolve().parents[3]
DESKTOP = ROOT / "images/desktop"


def load(name, path):
    loader = importlib.machinery.SourceFileLoader(name, str(path))
    spec = importlib.util.spec_from_loader(name, loader)
    module = importlib.util.module_from_spec(spec)
    loader.exec_module(module)
    return module


private_address = load("desktop_private_address", DESKTOP / "desktop-private-address.py")

LOOPBACK, PRIVATE, EGRESS = map(ipaddress.IPv4Address, ("127.0.0.1", "172.30.0.3", "172.31.0.2"))


def run(*argv, found=(LOOPBACK, EGRESS, PRIVATE)):
    out, err = io.StringIO(), io.StringIO()
    with mock.patch.object(private_address, "addresses", return_value=list(found)), \
            contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
        code = private_address.main(["desktop-private-address", *argv])
    return code, out.getvalue(), err.getvalue()


class PrivateAddressTest(unittest.TestCase):
    def test_the_address_in_the_private_subnet_is_printed_whatever_the_interface_order(self):
        self.assertEqual(run("172.30.0.0/16"), (0, "172.30.0.3\n", ""))
        self.assertEqual(run("172.30.0.0/16", found=(PRIVATE, EGRESS)), (0, "172.30.0.3\n", ""))

    def test_no_address_in_the_subnet_prints_nothing(self):
        self.assertEqual(run("10.99.0.0/16"), (1, "", ""))
        self.assertEqual(run("172.30.0.0/16", found=(LOOPBACK, EGRESS)), (1, "", ""))

    def test_an_invalid_or_ipv6_subnet_is_refused(self):
        for subnet in ("not-a-subnet", "fd00:7468::/64"):
            code, out, err = run(subnet)
            self.assertEqual((code, out), (2, ""), subnet)
            self.assertIn("desktop-private-address", err)
        self.assertEqual(run()[0], 2)

    @unittest.skipUnless(sys.platform.startswith("linux"), "reads Linux interfaces")
    def test_the_interfaces_are_read_from_the_kernel(self):
        self.assertIn(LOOPBACK, private_address.addresses())


if __name__ == "__main__":
    unittest.main()
