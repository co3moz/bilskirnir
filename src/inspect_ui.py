"""Dump the AT-SPI accessibility tree of applications on the desktop, as the desktop user."""
import json
import sys

try:
    import pyatspi
except ImportError:
    print("AT-SPI support is not installed in this desktop image; rebuild the profile image", file=sys.stderr)
    sys.exit(1)

pid_filter, name_filter, max_depth, max_nodes = sys.argv[1], sys.argv[2], int(sys.argv[3]), int(sys.argv[4])
INTERESTING = (pyatspi.STATE_VISIBLE, pyatspi.STATE_SHOWING, pyatspi.STATE_FOCUSED, pyatspi.STATE_ENABLED,
               pyatspi.STATE_CHECKED, pyatspi.STATE_SELECTED, pyatspi.STATE_EDITABLE, pyatspi.STATE_ACTIVE,
               pyatspi.STATE_SENSITIVE, pyatspi.STATE_EXPANDED, pyatspi.STATE_PRESSED)
count = 0


def node(accessible, depth):
    global count
    if count >= max_nodes:
        return None
    count += 1
    item = {"role": accessible.getRoleName(), "name": accessible.name or ""}
    try:
        states = accessible.getState().getStates()
        item["states"] = [pyatspi.stateToString(state) for state in states if state in INTERESTING]
    except Exception:
        pass
    try:
        x, y, width, height = accessible.queryComponent().getExtents(pyatspi.DESKTOP_COORDS)
        item.update({"x": x, "y": y, "width": width, "height": height})
    except Exception:
        pass
    try:
        text = accessible.queryText()
        item["text"] = text.getText(0, min(text.characterCount, 200))
    except Exception:
        pass
    try:
        actions = accessible.queryAction()
        item["actions"] = [actions.getName(index) for index in range(actions.nActions)]
    except Exception:
        pass
    if depth < max_depth:
        children = []
        for index in range(min(accessible.childCount, 200)):
            try:
                child = accessible.getChildAtIndex(index)
            except Exception:
                continue
            if child is None:
                continue
            described = node(child, depth + 1)
            if described:
                children.append(described)
        if children:
            item["children"] = children
    return item


desktop = pyatspi.Registry.getDesktop(0)
applications = []
for index in range(desktop.childCount):
    application = desktop.getChildAtIndex(index)
    if application is None:
        continue
    try:
        pid = application.get_process_id()
    except Exception:
        pid = None
    if pid_filter and str(pid) != pid_filter:
        continue
    if name_filter and name_filter.lower() not in (application.name or "").lower():
        continue
    tree = node(application, 0)
    if tree:
        tree["pid"] = pid
        applications.append(tree)
print(json.dumps({"applications": applications, "nodes": count, "truncated": count >= max_nodes}))
