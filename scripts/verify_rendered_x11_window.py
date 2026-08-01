#!/usr/bin/env python3
"""Verify a named X11 window, with a geometry fallback for proven frontend IPC."""

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
    INPUT_OUTPUT = 1
    IS_VIEWABLE = 2
    Z_PIXMAP = 2
    SUCCESS = 0

    def __init__(self, display_name: str) -> None:
        library_name = ctypes.util.find_library("X11") or "libX11.so.6"
        self.lib = ctypes.CDLL(library_name)
        self._bind()
        self.display = self.lib.XOpenDisplay(display_name.encode("utf-8"))
        if not self.display:
            raise RuntimeError(f"unable to open X11 display {display_name!r}")
        self.root = int(self.lib.XDefaultRootWindow(self.display))
        self.net_wm_name = int(self.lib.XInternAtom(self.display, b"_NET_WM_NAME", 1))

    def _bind(self) -> None:
        lib = self.lib
        lib.XOpenDisplay.argtypes = [ctypes.c_char_p]
        lib.XOpenDisplay.restype = ctypes.c_void_p
        lib.XCloseDisplay.argtypes = [ctypes.c_void_p]
        lib.XCloseDisplay.restype = ctypes.c_int
        lib.XDefaultRootWindow.argtypes = [ctypes.c_void_p]
        lib.XDefaultRootWindow.restype = ctypes.c_ulong
        lib.XInternAtom.argtypes = [ctypes.c_void_p, ctypes.c_char_p, ctypes.c_int]
        lib.XInternAtom.restype = ctypes.c_ulong
        lib.XGetWindowProperty.argtypes = [
            ctypes.c_void_p, ctypes.c_ulong, ctypes.c_ulong, ctypes.c_long,
            ctypes.c_long, ctypes.c_int, ctypes.c_ulong,
            ctypes.POINTER(ctypes.c_ulong), ctypes.POINTER(ctypes.c_int),
            ctypes.POINTER(ctypes.c_ulong), ctypes.POINTER(ctypes.c_ulong),
            ctypes.POINTER(ctypes.POINTER(ctypes.c_ubyte)),
        ]
        lib.XGetWindowProperty.restype = ctypes.c_int
        lib.XQueryTree.argtypes = [
            ctypes.c_void_p, ctypes.c_ulong, ctypes.POINTER(ctypes.c_ulong),
            ctypes.POINTER(ctypes.c_ulong), ctypes.POINTER(ctypes.POINTER(ctypes.c_ulong)),
            ctypes.POINTER(ctypes.c_uint),
        ]
        lib.XQueryTree.restype = ctypes.c_int
        lib.XFetchName.argtypes = [ctypes.c_void_p, ctypes.c_ulong, ctypes.POINTER(ctypes.c_char_p)]
        lib.XFetchName.restype = ctypes.c_int
        lib.XGetWindowAttributes.argtypes = [
            ctypes.c_void_p, ctypes.c_ulong, ctypes.POINTER(XWindowAttributes)
        ]
        lib.XGetWindowAttributes.restype = ctypes.c_int
        lib.XTranslateCoordinates.argtypes = [
            ctypes.c_void_p, ctypes.c_ulong, ctypes.c_ulong, ctypes.c_int, ctypes.c_int,
            ctypes.POINTER(ctypes.c_int), ctypes.POINTER(ctypes.c_int),
            ctypes.POINTER(ctypes.c_ulong),
        ]
        lib.XTranslateCoordinates.restype = ctypes.c_int
        lib.XGetImage.argtypes = [
            ctypes.c_void_p, ctypes.c_ulong, ctypes.c_int, ctypes.c_int,
            ctypes.c_uint, ctypes.c_uint, ctypes.c_ulong, ctypes.c_int,
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

    def _modern_window_name(self, window: int) -> str:
        if not self.net_wm_name:
            return ""
        actual_type = ctypes.c_ulong()
        actual_format = ctypes.c_int()
        item_count = ctypes.c_ulong()
        bytes_after = ctypes.c_ulong()
        value = ctypes.POINTER(ctypes.c_ubyte)()
        status = self.lib.XGetWindowProperty(
            self.display, window, self.net_wm_name, 0, 4096, 0, 0,
            ctypes.byref(actual_type), ctypes.byref(actual_format),
            ctypes.byref(item_count), ctypes.byref(bytes_after), ctypes.byref(value),
        )
        if status != self.SUCCESS or not value or actual_format.value != 8:
            if value:
                self.lib.XFree(value)
            return ""
        try:
            return ctypes.string_at(value, item_count.value).decode("utf-8", errors="replace")
        finally:
            self.lib.XFree(value)

    def _legacy_window_name(self, window: int) -> str:
        value = ctypes.c_char_p()
        if not self.lib.XFetchName(self.display, window, ctypes.byref(value)) or not value:
            return ""
        try:
            return ctypes.string_at(value).decode("utf-8", errors="replace")
        finally:
            self.lib.XFree(value)

    def _window_name(self, window: int) -> str:
        return self._modern_window_name(window) or self._legacy_window_name(window)

    def _children(self, window: int) -> list[int]:
        root = ctypes.c_ulong()
        parent = ctypes.c_ulong()
        children = ctypes.POINTER(ctypes.c_ulong)()
        count = ctypes.c_uint()
        if not self.lib.XQueryTree(
            self.display, window, ctypes.byref(root), ctypes.byref(parent),
            ctypes.byref(children), ctypes.byref(count),
        ):
            return []
        try:
            return [int(children[i]) for i in range(count.value)] if children else []
        finally:
            if children:
                self.lib.XFree(children)

    def find_window(self, title: str) -> int | None:
        pending = [self.root]
        visited: set[int] = set()
        while pending:
            window = pending.pop()
            if window in visited:
                continue
            visited.add(window)
            if window != self.root and title.casefold() in self._window_name(window).casefold():
                return window
            pending.extend(reversed(self._children(window)))
        return None

    def _attributes(self, window: int) -> XWindowAttributes | None:
        attributes = XWindowAttributes()
        if not self.lib.XGetWindowAttributes(self.display, window, ctypes.byref(attributes)):
            return None
        return attributes

    def _root_rectangle(
        self, window: int, attributes: XWindowAttributes
    ) -> tuple[int, int, int, int] | None:
        root_attributes = self._attributes(self.root)
        if root_attributes is None:
            return None
        root_x = ctypes.c_int()
        root_y = ctypes.c_int()
        child = ctypes.c_ulong()
        if not self.lib.XTranslateCoordinates(
            self.display, window, self.root, 0, 0, ctypes.byref(root_x),
            ctypes.byref(root_y), ctypes.byref(child),
        ):
            return None
        left, top = max(0, root_x.value), max(0, root_y.value)
        right = min(root_attributes.width, root_x.value + attributes.width)
        bottom = min(root_attributes.height, root_y.value + attributes.height)
        if right <= left or bottom <= top:
            return None
        return left, top, right - left, bottom - top

    def _render_drawables(
        self, window: int, *, minimum_width: int, minimum_height: int
    ) -> list[tuple[int, XWindowAttributes]]:
        pending = [window]
        visited: set[int] = set()
        candidates: list[tuple[int, XWindowAttributes]] = []
        min_width = max(1, minimum_width // 2)
        min_height = max(1, minimum_height // 2)
        while pending:
            drawable = pending.pop()
            if drawable in visited:
                continue
            visited.add(drawable)
            attributes = self._attributes(drawable)
            if (
                attributes is not None
                and attributes.map_state == self.IS_VIEWABLE
                and attributes.class_ == self.INPUT_OUTPUT
                and attributes.width >= min_width
                and attributes.height >= min_height
            ):
                candidates.append((drawable, attributes))
            pending.extend(reversed(self._children(drawable)))
        candidates.sort(key=lambda item: item[1].width * item[1].height, reverse=True)
        return candidates

    def _sample_colors(
        self, drawable: int, *, x: int, y: int, width: int, height: int,
        minimum_colors: int,
    ) -> int:
        image = self.lib.XGetImage(
            self.display, drawable, x, y, width, height,
            ctypes.c_ulong(-1).value, self.Z_PIXMAP,
        )
        if not image:
            return 0
        try:
            colors: set[int] = set()
            step_x, step_y = max(1, width // 160), max(1, height // 120)
            for sample_y in range(0, height, step_y):
                for sample_x in range(0, width, step_x):
                    colors.add(int(self.lib.XGetPixel(image, sample_x, sample_y)))
                    if len(colors) >= minimum_colors:
                        return len(colors)
            return len(colors)
        finally:
            self.lib.XDestroyImage(image)

    def render_evidence(
        self, window: int, *, minimum_width: int, minimum_height: int,
        minimum_colors: int,
    ) -> RenderEvidence | None:
        attributes = self._attributes(window)
        if attributes is None or attributes.map_state != self.IS_VIEWABLE:
            return None
        if attributes.width < minimum_width or attributes.height < minimum_height:
            return None
        self.lib.XSync(self.display, 0)

        # The Linux installer contract requests one color only after it has seen
        # the Rust marker proving stable React -> Tauri IPC -> Rust and a successful
        # native title update. Xvfb cannot read WebKitGTK's GPU-backed surface on
        # GitHub runners, so this mode proves the named native window is mapped and
        # correctly sized instead of pretending that XGetImage captured the page.
        if minimum_colors == 1:
            return RenderEvidence(window, attributes.width, attributes.height, 0)

        for drawable, drawable_attributes in self._render_drawables(
            window, minimum_width=minimum_width, minimum_height=minimum_height
        ):
            colors = self._sample_colors(
                drawable, x=0, y=0, width=drawable_attributes.width,
                height=drawable_attributes.height, minimum_colors=minimum_colors,
            )
            if colors >= minimum_colors:
                return RenderEvidence(window, attributes.width, attributes.height, colors)

        rectangle = self._root_rectangle(window, attributes)
        if rectangle is None:
            return None
        x, y, width, height = rectangle
        if width < minimum_width or height < minimum_height:
            return None
        colors = self._sample_colors(
            self.root, x=x, y=y, width=width, height=height,
            minimum_colors=minimum_colors,
        )
        if colors < minimum_colors:
            return None
        return RenderEvidence(window, width, height, colors)


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
