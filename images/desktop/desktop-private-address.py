#!/usr/bin/env python3
"""Print the desktop's IPv4 address in its private network, whose subnet is the only argument.

The manager passes that subnet as DESKTOP_PRIVATE_SUBNET, and desktop-session listens for VNC on
this address alone: the private network is the one the desktop shares with the manager only, while
the internet network, and any network connected later, reach the desktop's other interfaces.
Exits 1 without output when no interface has an address in the subnet, 2 for an invalid subnet.
"""
import ipaddress
import socket
import struct
import sys

SIOCGIFADDR = 0x8915


def addresses():
    """The IPv4 address of every interface that has one."""
    import fcntl

    found = []
    with socket.socket(socket.AF_INET, socket.SOCK_DGRAM) as probe:
        for _, name in socket.if_nameindex():
            request = struct.pack("256s", name.encode()[:15])
            try:
                reply = fcntl.ioctl(probe.fileno(), SIOCGIFADDR, request)
            except OSError:
                continue
            # struct ifreq: the 16-byte name, then a sockaddr_in whose address starts at byte 4.
            found.append(ipaddress.IPv4Address(reply[20:24]))
    return found


def pick(subnet, candidates):
    """The first of `candidates` in `subnet`, or None. An invalid or IPv6 subnet raises ValueError."""
    network = ipaddress.IPv4Network(subnet, strict=False)
    return next((address for address in candidates if address in network), None)


def main(argv):
    if len(argv) != 2:
        print("usage: desktop-private-address SUBNET", file=sys.stderr)
        return 2
    try:
        address = pick(argv[1], addresses())
    except ValueError as error:
        print(f"desktop-private-address: {error}", file=sys.stderr)
        return 2
    if address is None:
        return 1
    print(address)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
