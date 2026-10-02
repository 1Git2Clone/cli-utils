#!/usr/bin/env python3
"""Compute the layout string for a window after removing one pane.

Port of workmux's layout_after_sidebar_remove (layout_tree.rs): reads the
live #{window_layout}, parses tmux's layout grammar
(`WxH,X,Y{children}` / `[children]` / `,pane_id`), removes the target
leaf, and rescales the remaining siblings along the axis their parent
split was on so they fill the space the removed pane leaves behind --
keeping their relative proportions instead of handing everything to
whichever pane happens to be the removed pane's tree sibling (tmux's own
default when you just kill-pane).

The result is meant for `tmux select-layout -t <window> <result>`, run
after the kill. Prints nothing (and exits 0) if the layout can't be
parsed or the pane isn't found in it, so callers can fall back to doing
nothing rather than fail.
"""

import subprocess
import sys


class Leaf:
    __slots__ = ("w", "h", "x", "y", "pane_id")

    def __init__(self, w, h, x, y, pane_id):
        self.w, self.h, self.x, self.y, self.pane_id = w, h, x, y, pane_id


class Split:
    __slots__ = ("w", "h", "x", "y", "horizontal", "children")

    def __init__(self, w, h, x, y, horizontal, children):
        self.w, self.h, self.x, self.y = w, h, x, y
        self.horizontal = horizontal  # True: {}, side by side. False: [], stacked.
        self.children = children


class Parser:
    def __init__(self, body):
        self.s = body
        self.pos = 0

    def peek(self):
        return self.s[self.pos] if self.pos < len(self.s) else None

    def expect(self, ch):
        if self.peek() != ch:
            raise ValueError(f"expected {ch!r} at {self.pos} in {self.s!r}")
        self.pos += 1

    def parse_num(self):
        start = self.pos
        while True:
            c = self.peek()
            if c is None or not c.isdigit():
                break
            self.pos += 1
        if self.pos == start:
            raise ValueError(f"expected digits at {self.pos} in {self.s!r}")
        return int(self.s[start : self.pos])

    def parse_rect(self):
        w = self.parse_num()
        self.expect("x")
        h = self.parse_num()
        self.expect(",")
        x = self.parse_num()
        self.expect(",")
        y = self.parse_num()
        return w, h, x, y

    def parse_node(self):
        w, h, x, y = self.parse_rect()
        c = self.peek()
        if c == "{":
            self.pos += 1
            return Split(w, h, x, y, True, self.parse_children("}"))
        if c == "[":
            self.pos += 1
            return Split(w, h, x, y, False, self.parse_children("]"))
        if c == ",":
            self.pos += 1
            return Leaf(w, h, x, y, self.parse_num())
        raise ValueError(f"unexpected {c!r} at {self.pos} in {self.s!r}")

    def parse_children(self, close):
        children = [self.parse_node()]
        while self.peek() != close:
            self.expect(",")
            children.append(self.parse_node())
        self.expect(close)
        return children


def parse_layout(layout):
    # "CCCC,body" -- a 4-char hex checksum, then the comma this skips.
    if len(layout) < 6 or layout[4] != ",":
        return None
    parser = Parser(layout[5:])
    node = parser.parse_node()
    if parser.pos != len(parser.s):
        return None
    return node


def find_axis(node, target, horizontal=None):
    """Axis of the split that directly contains `target`: True if {},
    False if []. None if `target` isn't in this subtree."""
    if isinstance(node, Leaf):
        return horizontal if node.pane_id == target else None
    for child in node.children:
        if isinstance(child, Leaf) and child.pane_id == target:
            return node.horizontal
        found = find_axis(child, target)
        if found is not None:
            return found
    return None


def prune(node, target):
    """Remove the leaf with id `target`. Collapses a split left with one
    child. Returns None if nothing is left (never happens at the root
    here -- the caller only runs this when another pane still exists)."""
    if isinstance(node, Leaf):
        return None if node.pane_id == target else node
    children = [c for c in (prune(c, target) for c in node.children) if c is not None]
    if not children:
        return None
    if len(children) == 1:
        return children[0]
    node.children = children
    return node


def extent(node, horizontal):
    return node.w if horizontal else node.h


def proportional_lengths(old_lengths, available):
    old_total = sum(old_lengths)
    if old_total == 0 or not old_lengths:
        return [0] * len(old_lengths)
    remaining = available
    last = len(old_lengths) - 1
    result = []
    for i, old_len in enumerate(old_lengths):
        if i == last:
            result.append(remaining)
        else:
            scaled = int(old_len * available / old_total + 0.5)
            scaled = min(scaled, remaining)
            remaining = max(0, remaining - scaled)
            result.append(scaled)
    return result


def scale_axis(node, horizontal, new_len, new_pos):
    if horizontal:
        node.w, node.x = new_len, new_pos
    else:
        node.h, node.y = new_len, new_pos

    if isinstance(node, Leaf):
        return

    if node.horizontal == horizontal:
        # Same axis as this split: redistribute proportionally, in order.
        seps = len(node.children) - 1
        old_lengths = [extent(c, horizontal) for c in node.children]
        new_lengths = proportional_lengths(old_lengths, max(0, new_len - seps))
        pos = new_pos
        for child, child_len in zip(node.children, new_lengths):
            scale_axis(child, horizontal, child_len, pos)
            pos += child_len + 1
    else:
        # Cross axis: every child spans the same new length/position.
        for child in node.children:
            scale_axis(child, horizontal, new_len, new_pos)


def serialize_node(node):
    rect = f"{node.w}x{node.h},{node.x},{node.y}"
    if isinstance(node, Leaf):
        return f"{rect},{node.pane_id}"
    open_c, close_c = ("{", "}") if node.horizontal else ("[", "]")
    return rect + open_c + ",".join(serialize_node(c) for c in node.children) + close_c


def layout_checksum(body):
    csum = 0
    for b in body.encode():
        csum = (csum >> 1) | ((csum & 1) << 15)
        csum = (csum + b) & 0xFFFF
    return csum


def serialize_layout(root):
    body = serialize_node(root)
    return f"{layout_checksum(body):04x},{body}"


def main():
    if len(sys.argv) != 3:
        print(f"usage: {sys.argv[0]} <window_id> <pane_id>", file=sys.stderr)
        return 1
    window_id, pane_arg = sys.argv[1], sys.argv[2]
    pane_id = int(pane_arg[1:] if pane_arg.startswith("%") else pane_arg)

    layout_str = subprocess.run(
        ["tmux", "display-message", "-t", window_id, "-p", "#{window_layout}"],
        capture_output=True,
        text=True,
        check=False,
    ).stdout.strip()

    root = parse_layout(layout_str)
    if root is None:
        return 0

    axis = find_axis(root, pane_id)
    if axis is None:
        return 0

    content = prune(root, pane_id)
    if content is None:
        return 0

    scale_axis(content, axis, extent(root, axis), root.x if axis else root.y)
    print(serialize_layout(content))
    return 0


if __name__ == "__main__":
    sys.exit(main())
