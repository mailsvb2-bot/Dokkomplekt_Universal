#!/usr/bin/env python3
"""Find a named X11 window and prove that it has rendered non-blank pixels."""

from __future__ import annotations

import argparse
import ctypes
import ctypes.util
import os
import sys
from dataclasses import dataclass


class XWindowAttributes(ctypes.Structure):
    _fields_ = [
        ("x", ctypes.c_int),
        ("y", ctypes.c_int),
        ("width", ctypes.c_int),
        ("height", ctypes.c_int),
        ("border_width", ctypes.c_int),
        ("depth", ctypes.c_int),
        ("visual", ctypes.c_void_p),
        ("root", ctypes.c_ulong),
        ("class_", ctypes.c_int),
        ("bit_gravity", ctypes.c_int),
        ("win_gravity", ctypes.c_int),
        ("backing_store", ctypes.c_int),
        ("backing_planes", ctypes.c_ulong),
        ("backing_pixel", ctypes.c_ulong),
        ("save_under", ctypes.c_int),
        ("colormap", ctypes.c_ulong),
        ("map_installed", ctypes.c_int),
        ("map_state", ctypes.c_int),
        ("all_event_masks", ctypes.c_long),
        ("your_event_mask", ctypes.c_long),
        ("do_not_propagate_mask", ctypes.c_long),
        ("override_redirect", ctypes.c_int),
        ("screen", ctypes.c_void_p),
    ]


@dataclass(frozen=True)
class RenderEvidence:
    window_id: int
    width: int
    height: int
    colors: int


class X11Probe:
    def __init__(self, display_name: str) -> None:
        library_name = ctypes.util.find_library("X11") or "libX11.so.6"
        self.lib = ctypes.CDLL(library_name)
        self._bind()
        encoded = display_name.encode("utf-8")
        self.display = self.lib.XOpenDisplay(encoded)
        if not self.display:
            raise RuntimeError(f"unable to open X11 display {display_name!r}")

    def _bind(self) -> None:
        lib = self.lib
        lib.XOpenDisplay.argtypes = [ctypes.c_char_p]
        lib.XOpenDisplay.restype = ctypes.c_void_p
        lib.XCloseDisplay.argtypes = [ctypes.c_void_p]
        lib.XCloseDisplay.restype = ctypes.c_int
        lib.XDefaultRootWindow.argtypes = [ctypes.c_void_p]
        lib.XDefaultRootWindow.restype = ctypes.c_ulong
        lib.XQueryTree.argtypes = [
            ctypes.c_void_p,
            ctypes.c_ulong,
            ctypes.POINTER(ctypes.c_ulong),
            ctypes.POINTER(ctypes.c_ulong),
            ctypes.POINTER(ctypes.POINTER(ctypes.c_ulong)),
            ctypes.POINTER(ctypes.c_uint),
        ]
        lib.XQueryTree.restype = ctypes.c_int
        lib.XFetchName.argtypes = [
            ctypes.c_void_p,
            ctypes.c_ulong,
            ctypes.POINTER(ctypes.c_char_p),
        ]
        lib.XFetchName.restype = ctypes.c_int
        lib.XGetWindowAttributes.argtypes = [
            ctypes.c_void_p,
            ctypes.c_ulong,
            ctypes.POINTER(XWindowAttributes),
        ]
        lib.XGetWindowAttributes.restype = ctypes.c_int
        lib.XGetImage.argtypes = [
            ctypes.c_void_p,
            ctypes.c_ulong,
            ctypes.c_int,
            ctypes.c_int,
            ctypes.c_uint,
            ctypes.c_uint,
            ctypes.c_ulong,
            ctypes.c_int,
        ]
        lib.XGetImage.restype = ctypes.c_void_p
        lib.XGetPixel.argtypes = [ctypes.c_void_p, ctypes.c_int, ctypes.c_int]
        lib.XGetPixel.restype = ctypes.c_ulong
        lib.XDestroyImage.argtypes = [ctypes.c_void_p]
        lib.XDestroyImage.restype = ctypes.c_int
        lib.XFree.argtypes = [ctypes.c_void_p]
        lib.XFree.restype = ctypes.c_int
        lib.XSync.argtypes = [ctypes.c_void_p, ctypes.c_int]
        lib.XSync.restype = ctypes.c_int

    def close(self) -> None:
        if self.display:
            self.lib.XCloseDisplay(self.display)
            self.display = None

    def __enter__(self) -> "X11Probe":
        return self

    def __exit__(self, *_: object) -> None:
        self.close()

    def _window_name(self, window: int) -> str:
        value = ctypes.c_char_p()
        if not self.lib.XFetchName(self.display, window, ctypes.byref(value)) or not value:
            return ""
        try:
            return ctypes.string_at(value).decode("utf-8", errors="replace")
        finally:
            self.lib.XFree(value)

    def _children(self, window: int) -> list[int]:
        root = ctypes.c_ulong()
        parent = ctypes.c_ulong()
        children = ctypes.POINTER(ctypes.c_ulong)()
        count = ctypes.c_uint()
        if not self.lib.XQueryTree(
            self.display,
            window,
            ctypes.byref(root),
            ctypes.byref(parent),
            ctypes.byref(children),
            ctypes.byref(count),
        ):
            return []
        try:
            return [int(children[index]) for index in range(count.value)] if children else []
        finally:
            if children:
                self.lib.XFree(children)

    def find_window(self, title: str) -> int | None:
        root = int(self.lib.XDefaultRootWindow(self.display))
        pending = [root]
        visited: set[int] = set()
        while pending:
            window = pending.pop()
            if window in visited:
                continue
            visited.add(window)
            if window != root and title in self._window_name(window):
                return window
            pending.extend(reversed(self._children(window)))
        return None

    def render_evidence(
        self,
        window: int,
        *,
        minimum_width: int,
        minimum_height: int,
        minimum_colors: int,
    ) -> RenderEvidence | None:
        attributes = XWindowAttributes()
        if not self.lib.XGetWindowAttributes(self.display, window, ctypes.byref(attributes)):
            return None
        if attributes.map_state != 2:  # IsViewable
            return None
        width = int(attributes.width)
        height = int(attributes.height)
        if width < minimum_width or height < minimum_height:
            return None

        image = self.lib.XGetImage(
            self.display,
            window,
            0,
            0,
            width,
            height,
            ctypes.c_ulong(-1).value,
            2,  # ZPixmap
        )
        if not image:
            return None
        try:
            colors: set[int] = set()
            step_x = max(1, width // 160)
            step_y = max(1, height // 120)
            for y in range(0, height, step_y):
                for x in range(0, width, step_x):
                    colors.add(int(self.lib.XGetPixel(image, x, y)))
                    if len(colors) >= minimum_colors:
                        return RenderEvidence(window, width, height, len(colors))
            return None
        finally:
            self.lib.XDestroyImage(image)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--display", default=os.environ.get("DISPLAY", ""))
    parser.add_argument("--title", required=True)
    parser.add_argument("--min-width", type=int, default=800)
    parser.add_argument("--min-height", type=int, default=500)
    parser.add_argument("--min-colors", type=int, default=64)
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    if not args.display:
        print("DISPLAY is required", file=sys.stderr)
        return 2
    try:
        with X11Probe(args.display) as probe:
            window = probe.find_window(args.title)
            if window is None:
                return 1
            evidence = probe.render_evidence(
                window,
                minimum_width=args.min_width,
                minimum_height=args.min_height,
                minimum_colors=args.min_colors,
            )
    except (OSError, RuntimeError) as error:
        print(str(error), file=sys.stderr)
        return 2
    if evidence is None:
        return 1
    print(f"{evidence.window_id} {evidence.width} {evidence.height} {evidence.colors}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
