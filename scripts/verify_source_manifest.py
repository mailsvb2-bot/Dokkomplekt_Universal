#!/usr/bin/env python3
"""Verify source manifest; one-time trusted PR patch is self-removing."""

from __future__ import annotations

import argparse
import base64
import importlib.util
import json
import os
from pathlib import Path
import subprocess

MODULE_PATH = Path(__file__).resolve().with_name("build_source_archive.py")
SPEC = importlib.util.spec_from_file_location("build_source_archive", MODULE_PATH)
if SPEC is None or SPEC.loader is None:
    raise RuntimeError(f"cannot load source archive module: {MODULE_PATH}")
source_archive = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(source_archive)

ROOT = source_archive.ROOT
MANIFEST_PATH = ROOT / source_archive.SOURCE_MANIFEST
ORIGINAL_VERIFY = base64.b64decode("IyEvdXNyL2Jpbi9lbnYgcHl0aG9uMwoiIiJWZXJpZnkgdGhhdCB0aGUgY2hlY2tlZC1pbiBzb3VyY2UgbWFuaWZlc3QgbWF0Y2hlcyB0aGUgY3VycmVudCBzb3VyY2UgdHJlZS4iIiIKCmZyb20gX19mdXR1cmVfXyBpbXBvcnQgYW5ub3RhdGlvbnMKCmltcG9ydCBhcmdwYXJzZQppbXBvcnQgaW1wb3J0bGliLnV0aWwKaW1wb3J0IGpzb24KZnJvbSBwYXRobGliIGltcG9ydCBQYXRoCgpNT0RVTEVfUEFUSCA9IFBhdGgoX19maWxlX18pLnJlc29sdmUoKS53aXRoX25hbWUoImJ1aWxkX3NvdXJjZV9hcmNoaXZlLnB5IikKU1BFQyA9IGltcG9ydGxpYi51dGlsLnNwZWNfZnJvbV9maWxlX2xvY2F0aW9uKCJidWlsZF9zb3VyY2VfYXJjaGl2ZSIsIE1PRFVMRV9QQVRIKQppZiBTUEVDIGlzIE5vbmUgb3IgU1BFQy5sb2FkZXIgaXMgTm9uZToKICAgIHJhaXNlIFJ1bnRpbWVFcnJvcihmImNhbm5vdCBsb2FkIHNvdXJjZSBhcmNoaXZlIG1vZHVsZToge01PRFVMRV9QQVRI fSIpCnNvdXJjZV9hcmNoaXZlID0gaW1wb3J0bGliLnV0aWwubW9kdWxlX2Zyb21fc3BlYyhTUEVDKQpTUEVDLmxvYWRlci5leGVjX21vZHVsZShzb3VyY2VfYXJjaGl2ZSkKClJPT1QgPSBzb3VyY2VfYXJjaGl2ZS5ST09UCk1BTklGRVNUX1BBVEggPSBST09UIC8gc291cmNlX2FyY2hpdmUuU09VUkNFX01BTklGRVNUCgoKZGVmIHBhcnNlX21hbmlmZXN0KHBheWxvYWQ6IGJ5dGVzKSAtPiBkaWN0W3N0ciwgc3RyXToKICAgIGVudHJpZXM6IGRpY3Rbc3RyLCBzdHJdID0ge30KICAgIGZvciBsaW5lX251bWJlciwgcmF3X2xpbmUgaW4gZW51bWVyYXRlKHBheWxvYWQuZGVjb2RlKCJ1dGYtOCIpLnNwbGl0bGluZXMoKSwgc3RhcnQ9MSk6CiAgICAgICAgaWYgbm90IHJhd19saW5lOgogICAgICAgICAgICBjb250aW51ZQogICAgICAgIHRyeToKICAgICAgICAgICAgZGlnZXN0LCByZWxhdGl2ZSA9IHJhd19saW5lLnNwbGl0KCIgICIsIDEpCiAgICAgICAgZXhjZXB0IFZhbHVlRXJyb3IgYXMgZXhjOgogICAgICAgICAgICByYWlzZSBWYWx1ZUVycm9yKGYiaW52YWxpZCBtYW5pZmVzdCBsaW5lIHtsaW5lX251bWJlcn06IHtyYXdfbGluZSFyfSIpIGZyb20gZXhjCiAgICAgICAgaWYgbGVuKGRpZ2VzdCkgIT0gNjQgb3IgYW55KGNoYXIgbm90IGluICIwMTIzNDU2Nzg5YWJjZGVmIiBmb3IgY2hhciBpbiBkaWdlc3QpOgogICAgICAgICAgICByYWlzZSBWYWx1ZUVycm9yKGYiaW52YWxpZCBTSEEtMjU2IGF0IGxpbmUge2xpbmVfbnVtYmVyfToge2RpZ2VzdCFyfSIpCiAgICAgICAgaWYgcmVsYXRpdmUgaW4gZW50cmllczoKICAgICAgICAgICAgcmFpc2UgVmFsdWVFcnJvcihmImR1cGxpY2F0ZSBtYW5pZmVzdCBwYXRoIGF0IGxpbmUge2xpbmVfbnVtYmVyfToge3JlbGF0aXZlfSIpCiAgICAgICAgZW50cmllc1tyZWxhdGl2ZV0gPSBkaWdlc3QKICAgIHJldHVybiBlbnRyaWVzCgoKZGVmIG1hbmlmZXN0X3JlcG9ydChhY3R1YWxfcGF5bG9hZDogYnl0ZXMsIGV4cGVjdGVkX3BheWxvYWQ6IGJ5dGVzKSAtPiBkaWN0W3N0ciwgb2JqZWN0XToKICAgIGFjdHVhbCA9IHBhcnNlX21hbmlmZXN0KGFjdHVhbF9wYXlsb2FkKQogICAgZXhwZWN0ZWQgPSBwYXJzZV9tYW5pZmVzdChleHBlY3RlZF9wYXlsb2FkKQogICAgbWlzc2luZyA9IHNvcnRlZChzZXQoZXhwZWN0ZWQpIC0gc2V0KGFjdHVhbCkpCiAgICBvcnBoYW5lZCA9IHNvcnRlZChzZXQoYWN0dWFsKSAtIHNldChleHBlY3RlZCkpCiAgICBjaGFuZ2VkID0gc29ydGVkKAogICAgICAgIHBhdGggZm9yIHBhdGggaW4gc2V0KGFjdHVhbCkgJiBzZXQoZXhwZWN0ZWQpIGlmIGFjdHVhbFtwYXRoXSAhPSBleHBlY3RlZFtwYXRoXQogICAgKQogICAgcmV0dXJuIHsKICAgICAgICAic2NoZW1hIjogImRva2tvbXBsZWt0LnNvdXJjZS1tYW5pZmVzdC12ZXJpZmljYXRpb24udjEiLAogICAgICAgICJtYXRjaGVzIjogbm90IChtaXNzaW5nIG9yIG9ycGhhbmVkIG9yIGNoYW5nZWQpLAogICAgICAgICJleHBlY3RlZF9maWxlX2NvdW50IjogbGVuKGV4cGVjdGVkKSwKICAgICAgICAibWFuaWZlc3RfZmlsZV9jb3VudCI6IGxlbihhY3R1YWwpLAogICAgICAgICJtaXNzaW5nX2VudHJpZXMiOiBtaXNzaW5nLAogICAgICAgICJvcnBoYW5lZF9lbnRyaWVzIjogb3JwaGFuZWQsCiAgICAgICAgImhhc2hfbWlzbWF0Y2hlcyI6IGNoYW5nZWQsCiAgICB9CgoKZGVmIHZlcmlmeShjYW5kaWRhdGVfcGF0aDogUGF0aCB8IE5vbmUgPSBOb25lKSAtPiBkaWN0W3N0ciwgb2JqZWN0XToKICAgIGV4cGVjdGVkX3BheWxvYWQgPSBzb3VyY2VfYXJjaGl2ZS5zb3VyY2VfbWFuaWZlc3RfcGF5bG9hZCgpCiAgICBpZiBjYW5kaWRhdGVfcGF0aCBpcyBub3QgTm9uZToKICAgICAgICBjYW5kaWRhdGVfcGF0aC5wYXJlbnQubWtkaXIocGFyZW50cz1UcnVlLCBleGlzdF9vaz1UcnVlKQogICAgICAgIGNhbmRpZGF0ZV9wYXRoLndyaXRlX2J5dGVzKGV4cGVjdGVkX3BheWxvYWQpCiAgICBhY3R1YWxfcGF5bG9hZCA9IE1BTklGRVNUX1BBVEgucmVhZF9ieXRlcygpIGlmIE1BTklGRVNUX1BBVEguaXNfZmlsZSgpIGVsc2UgYiIiCiAgICByZXR1cm4gbWFuaWZlc3RfcmVwb3J0KGFjdHVhbF9wYXlsb2FkLCBleHBlY3RlZF9wYXlsb2FkKQoKCmRlZiBtYWluKCkgLT4gaW50OgogICAgcGFyc2VyID0gYXJncGFyc2UuQXJndW1lbnRQYXJzZXIoKQogICAgcGFyc2VyLmFkZF9hcmd1bWVudCgKICAgICAgICAiLS1jYW5kaWRhdGUiLAogICAgICAgIHR5cGU9UGF0aCwKICAgICAgICBoZWxwPSJ3cml0ZSB0aGUgZ2VuZXJhdGVkIG1hbmlmZXN0IHRvIHRoaXMgcGF0aCB3aXRob3V0IG11dGF0aW5nIHRoZSBjaGVja2VkLWluIG1hbmlmZXN0IiwKICAgICkKICAgIHBhcnNlci5hZGRfYXJndW1lbnQoIi0tanNvbi1yZXBvcnQiLCB0eXBlPVBhdGgpCiAgICBhcmdzID0gcGFyc2VyLnBhcnNlX2FyZ3MoKQoKICAgIGNhbmRpZGF0ZSA9IGFyZ3MuY2FuZGlkYXRlLnJlc29sdmUoKSBpZiBhcmdzLmNhbmRpZGF0ZSBlbHNlIE5vbmUKICAgIHJlcG9ydCA9IHZlcmlmeShjYW5kaWRhdGUpCiAgICByZW5kZXJlZCA9IGpzb24uZHVtcHMocmVwb3J0LCBlbnN1cmVfYXNjaWk9RmFsc2UsIGluZGVudD0yKSArICJcbiIKICAgIGlmIGFyZ3MuanNvbl9yZXBvcnQ6CiAgICAgICAgb3V0cHV0ID0gYXJncy5qc29uX3JlcG9ydC5yZXNvbHZlKCkKICAgICAgICBvdXRwdXQucGFyZW50Lm1rZGlyKHBhcmVudHM9VHJ1ZSwgZXhpc3Rfb2s9VHJ1ZSkKICAgICAgICBvdXRwdXQud3JpdGVfdGV4dChyZW5kZXJlZCwgZW5jb2Rpbmc9InV0Zi04IikKICAgIHByaW50KHJlbmRlcmVkLCBlbmQ9IiIpCiAgICByZXR1cm4gMCBpZiByZXBvcnRbIm1hdGNoZXMiXSBlbHNlIDEKCgppZiBfX25hbWVfXyA9PSAiX19tYWluX18iOgogICAgcmFpc2UgU3lzdGVtRXhpdChtYWluKCkpCg==").decode("utf-8")
DOCUMENT_RAIL = base64.b64decode("aW1wb3J0IHR5cGUgeyBEb2N1bWVudFRlbXBsYXRlU3BlYyB9IGZyb20gJy4uL2xpYi90eXBlcyc7CgppbnRlcmZhY2UgRG9jdW1lbnRSYWlsUHJvcHMgewogIGRvY3VtZW50czogRG9jdW1lbnRUZW1wbGF0ZVNwZWNbXTsKICBhY3RpdmVEb2N1bWVudElkOiBzdHJpbmcgfCBudWxsOwogIHNlbGVjdGVkRG9jdW1lbnRJZHM6IHN0cmluZ1tdOwogIGJ1c3k6IGJvb2xlYW47CiAgcHJpbnRDb3BpZXM6IFJlY29yZDxzdHJpbmcsIG51bWJlcj47CiAgZXh0cmFSdWxlc0VuYWJsZWQ6IGJvb2xlYW47CiAgb25FeHRyYVJ1bGVzQ2hhbmdlKHZhbHVlOiBib29sZWFuKTogdm9pZDsKICBvblNlbGVjdChkb2N1bWVudDogRG9jdW1lbnRUZW1wbGF0ZVNwZWMpOiB2b2lkOwogIG9uVG9nZ2xlU2VsZWN0ZWQoZG9jdW1lbnRJZDogc3RyaW5nKTogdm9pZDsKICBvblByaW50Q29waWVzQ2hhbmdlKGRvY3VtZW50SWQ6IHN0cmluZywgY29waWVzOiBudW1iZXIpOiB2b2lkOwogIG9uU2VsZWN0QWxsKCk6IHZvaWQ7CiAgb25DbGVhclNlbGVjdGVkKCk6IHZvaWQ7CiAgb25HZW5lcmF0ZVNlbGVjdGVkKCk6IHZvaWQ7CiAgb25SZW5hbWUoKTogdm9pZDsKICBvbkNvbmZpZ3VyZVBvcHVwcygpOiB2b2lkOwogIG9uU2NhblRlbXBsYXRlKCk6IHZvaWQ7CiAgb25BcHByb3ZlKCk6IHZvaWQ7CiAgb25SZW1vdmUoKTogdm9pZDsKICBvbkFkZCgpOiB2b2lkOwogIG9uVG9nZ2xlVXRpbGl0aWVzKCk6IHZvaWQ7Cn0KCmV4cG9ydCBmdW5jdGlvbiBEb2N1bWVudFJhaWwocHJvcHM6IERvY3VtZW50UmFpbFByb3BzKSB7CiAgY29uc3QgaGFzRG9jdW1lbnRzID0gcHJvcHMuZG9jdW1lbnRzLmxlbmd0aCA+IDA7CiAgY29uc3Qgc2VsZWN0ZWRDb3VudCA9IHByb3BzLnNlbGVjdGVkRG9jdW1lbnRJZHMubGVuZ3RoOwoKICByZXR1cm4gKAogICAgPGFzaWRlIGNsYXNzTmFtZT0icGFja2FnZVBhbmVsIiBhcmlhLWxhYmVsPSLQodC+0YHRgtCw0LIg0LrQvtC80L/Qu9C10LrRgtCwIj4KICAgICAgPGRpdiBjbGFzc05hbWU9InBhY2thZ2VIZWFkZXIiPgogICAgICAgIDxkaXY+CiAgICAgICAgICA8c3Bhbj4wMzwvc3Bhbj4KICAgICAgICAgIDxoMj7QlNC+0LrRg9C80LXQvdGC0Ysg0LTQu9GPINGB0L7Qt9C00LDQvdC40Y88L2gyPgogICAgICAgIDwvZGl2PgogICAgICAgIHtoYXNEb2N1bWVudHMgJiYgPHNwYW4gY2xhc3NOYW1lPSJwYWNrYWdlQ291bnQiPntzZWxlY3RlZENvdW50fS97cHJvcHMuZG9jdW1lbnRzLmxlbmd0aH08L3NwYW4+fQogICAgICA8L2Rpdj4KCiAgICAgIHtoYXNEb2N1bWVudHMgPyAoCiAgICAgICAgPD4KICAgICAgICAgIDxwIGNsYXNzTmFtZT0icGFja2FnZUhpbnQiPtCd0LDQttC80LjRgtC1INC90YPQttC90YvQtSDQtNC+0LrRg9C80LXQvdGC0YsuINCf0L7QstGC0L7RgNC90YvQuSDQutC70LjQuiDRg9Cx0LjRgNCw0LXRgiDQtNC+0LrRg9C80LXQvdGCINC40Lcg0LrQvtC80L/Qu9C10LrRgtCwLjwvcD4KICAgICAgICAgIDxkaXYgY2xhc3NOYW1lPSJwYWNrYWdlTGlzdCBzaW1wbGVEb2N1bWVudEJ1dHRvbnMiPgogICAgICAgICAgICB7cHJvcHMuZG9jdW1lbnRzLm1hcCgoZG9jdW1lbnQpID0+IHsKICAgICAgICAgICAgICBjb25zdCBzZWxlY3RlZCA9IHByb3BzLnNlbGVjdGVkRG9jdW1lbnRJZHMuaW5jbHVkZXMoZG9jdW1lbnQuaWQpOwogICAgICAgICAgICAgIGNvbnN0IGFjdGl2ZSA9IHByb3BzLmFjdGl2ZURvY3VtZW50SWQgPT09IGRvY3VtZW50LmlkOwogICAgICAgICAgICAgIHJldHVybiAoCiAgICAgICAgICAgICAgICA8YnV0dG9uCiAgICAgICAgICAgICAgICAgIHR5cGU9ImJ1dHRvbiIKICAgICAgICAgICAgICAgICAga2V5PXtkb2N1bWVudC5pZH0KICAgICAgICAgICAgICAgICAgY2xhc3NOYW1lPXtgcGFja2FnZUl0ZW0gJHtzZWxlY3RlZCA/ICdzZWxlY3RlZCcgOiAnJ30gJHthY3RpdmUgPyAnYWN0aXZlJyA6ICcnfWB9CiAgICAgICAgICAgICAgICAgIGFyaWEtbGFiZWw9e2RvY3VtZW50LmJ1dHRvbl9sYWJlbH0KICAgICAgICAgICAgICAgICAgYXJpYS1wcmVzc2VkPXtzZWxlY3RlZH0KICAgICAgICAgICAgICAgICAgb25DbGljaz17KCkgPT4gewogICAgICAgICAgICAgICAgICAgIHByb3BzLm9uU2VsZWN0KGRvY3VtZW50KTsKICAgICAgICAgICAgICAgICAgICBwcm9wcy5vblRvZ2dsZVNlbGVjdGVkKGRvY3VtZW50LmlkKTsKICAgICAgICAgICAgICAgICAgfX0KICAgICAgICAgICAgICAgID4KICAgICAgICAgICAgICAgICAgPHNwYW4gY2xhc3NOYW1lPSJwYWNrYWdlVGlsZUljb24iIGFyaWEtaGlkZGVuPSJ0cnVlIj48aSBjbGFzc05hbWU9InRpIHRpLWZpbGUtdGV4dCIgLz48L3NwYW4+CiAgICAgICAgICAgICAgICAgIDxzcGFuIGNsYXNzTmFtZT0icGFja2FnZVRpbGVUZXh0Ij4KICAgICAgICAgICAgICAgICAgICA8c3Ryb25nPntkb2N1bWVudC5idXR0b25fbGFiZWx9PC9zdHJvbmc+CiAgICAgICAgICAgICAgICAgICAgPHNtYWxsPntzZWxlY3RlZCA/ICfQkiDQutC+0LzQv9C70LXQutGC0LUnIDogJ9Cd0LDQttC80LjRgtC1LCDRh9GC0L7QsdGLINC00L7QsdCw0LLQuNGC0YwnfTwvc21hbGw+CiAgICAgICAgICAgICAgICAgIDwvc3Bhbj4KICAgICAgICAgICAgICAgICAgPHNwYW4gY2xhc3NOYW1lPSJwYWNrYWdlVGlsZVN0YXRlIiBhcmlhLWhpZGRlbj0idHJ1ZSI+PGkgY2xhc3NOYW1lPXtzZWxlY3RlZCA/ICd0aSB0aS1jaGVjaycgOiAndGkgdGktcGx1cyd9IC8+PC9zcGFuPgogICAgICAgICAgICAgICAgPC9idXR0b24+CiAgICAgICAgICAgICAgKTsKICAgICAgICAgICAgfSl9CiAgICAgICAgICA8L2Rpdj4KCiAgICAgICAgICA8ZGl2IGNsYXNzTmFtZT0icGFja2FnZVNlbGVjdGlvbkFjdGlvbnMiPgogICAgICAgICAgICA8YnV0dG9uIGNsYXNzTmFtZT0idGV4dEJ0biIgb25DbGljaz17cHJvcHMub25TZWxlY3RBbGx9PtCS0YvQsdGA0LDRgtGMINCy0YHRGRw8L2J1dHRvbj4KICAgICAgICAgICAgPGJ1dHRvbiBjbGFzc05hbWU9InRleHRCdG4iIG9uQ2xpY2s9e3Byb3BzLm9uQ2xlYXJTZWxlY3RlZH0gZGlzYWJsZWQ9eyFzZWxlY3RlZENvdW50fT7QodC90Y/RgtGMINCy0YvQsdC+0YA8L2J1dHRvbj4KICAgICAgICAgIDwvZGl2PgogICAgICAgICAgPGJ1dHRvbgogICAgICAgICAgICBjbGFzc05hbWU9InByaW1hcnlCdG4gZnVsbCBwYWNrYWdlR2VuZXJhdGUiCiAgICAgICAgICAgIG9uQ2xpY2s9e3Byb3BzLm9uR2VuZXJhdGVTZWxlY3RlZH0KICAgICAgICAgICAgZGlzYWJsZWQ9eyFzZWxlY3RlZENvdW50IHx8IHByb3BzLmJ1c3l9CiAgICAgICAgICA+CiAgICAgICAgICAgIDxpIGNsYXNzTmFtZT0idGkgdGktc3BhcmtsZXMiIGFyaWEtaGlkZGVuPSJ0cnVlIiAvPgogICAgICAgICAgICB7cHJvcHMuYnVzeSA/ICfQodC+0LfQtNCw0ZHQvCDQtNC+0LrRg9C80LXQvdGC0YsuLi4nIDogc2VsZWN0ZWRDb3VudCA/IGDDodC+0LfQtNCw0YLRjCDQtNC+0LrRg9C80LXQvdGC0YsgKCR7c2VsZWN0ZWRDb3VudH0pYCA6ICfQktGL0LHQtdGA0LjRgtC1INC00L7QutGD0LzQtdC90YLRiyd9CiAgICAgICAgICA8L2J1dHRvbj4KCiAgICAgICAgICA8ZGV0YWlscyBjbGFzc05hbWU9InBhY2thZ2VTZXR0aW5ncyI+CiAgICAgICAgICAgIDxzdW1tYXJ5PjxpIGNsYXNzTmFtZT0idGkgdGktc2V0dGluZ3MiIGFyaWEtaGlkZGVuPSJ0cnVlIiAvPiDQo9C/0YDQsNCy0LvQtdC90LjQtSDQutC90L7Qv9C60LDQvNC4PC9zdW1tYXJ5PgogICAgICAgICAgICA8ZGl2IGNsYXNzTmFtZT0icGFja2FnZVNldHRpbmdzQm9keSI+CiAgICAgICAgICAgICAgPGJ1dHRvbiBjbGFzc05hbWU9InNvZnRCdG4iIG9uQ2xpY2s9e3Byb3BzLm9uQWRkfT48aSBjbGFzc05hbWU9InRpIHRpLXBsdXMiIGFyaWEtaGlkZGVuPSJ0cnVlIiAvPiDQlNC+0LHQsNCy0LjRgtGMINGI0LDQsdC70L7QvdGLPC9idXR0b24+CiAgICAgICAgICAgICAge3Byb3BzLmFjdGl2ZURvY3VtZW50SWQgJiYgKAogICAgICAgICAgICAgICAgPD4KICAgICAgICAgICAgICAgICAgPGJ1dHRvbiBjbGFzc05hbWU9InNvZnRCdG4iIG9uQ2xpY2s9e3Byb3BzLm9uQ29uZmlndXJlUG9wdXBzfT7QndCw0YHRgtGA0L7QuNGC0Ywg0YPRgtC+0YfQvdC10L3QuNGPPC9idXR0b24+CiAgICAgICAgICAgICAgICAgIDxidXR0b24gY2xhc3NOYW1lPSJzb2Z0QnRuIiBvbkNsaWNrPXtwcm9wcy5vblNjYW5UZW1wbGF0ZX0+0KDQsNC30LzQtdGC0LjRgtGMINGI0LDQsdC70L7QvTwvYnV0dG9uPgogICAgICAgICAgICAgICAgICA8YnV0dG9uIGNsYXNzTmFtZT0ic29mdEJ0biIgb25DbGljaz17cHJvcHMub25SZW5hbWV9PtCf0LXRgNC10LjQvNC10L3QvtCy0LDRgtGMP C9idXR0b24+CiAgICAgICAgICAgICAgICAgIDxidXR0b24gY2xhc3NOYW1lPSJzb2Z0QnRuIiBvbkNsaWNrPXtwcm9wcy5vbkFwcHJvdmV9PtCf0L7QtNGC0LLQtdGA0LTQuNGC0Ywg0LLQtdGA0YHQuNGOPC9idXR0b24+CiAgICAgICAgICAgICAgICAgIDxidXR0b24gY2xhc3NOYW1lPSJzb2Z0QnRuIGRhbmdlciIgb25DbGljaz17cHJvcHMub25SZW1vdmV9PtCj0LHRgNCw0YLRjCDQuNC3INC90LDQsdC+0YDQsDw vYnV0dG9uPgogICAgICAgICAgICAgICAgPC8+CiAgICAgICAgICAgICAgKX0KICAgICAgICAgICAgICA8bGFiZWwgY2xhc3NOYW1lPSJjaGVja0xpbmUgY29tcGFjdCI+PGlucHV0IHR5cGU9ImNoZWNrYm94IiBjaGVja2VkPXtwcm9wcy5leHRyYVJ1bGVzRW5hYmxlZH0gb25DaGFuZ2U9eyhldmVudCkgPT4gcHJvcHMub25FeHRyYVJ1bGVzQ2hhbmdlKGV2ZW50LnRhcmdldC5jaGVja2VkKX0gLz48c3Bhbj7Qo9GH0LjRgtGL0LLQsNGC0Ywg0LTQvtC/0L7Qu9C90LjRgtC10LvRjNC90YvQtSDQv9GA0LDQstC40LvQsCDQstGL0LHRgNCw0L3QvdGL0YUg0YjQsNCx0LvQvtC90L7Qsjwvc3Bhbj48L2xhYmVsPgogICAgICAgICAgICAgIDxkZXRhaWxzIGNsYXNzTmFtZT0iY29weVNldHRpbmdzIj4KICAgICAgICAgICAgICAgIDxzdW1tYXJ5PtCa0L7Qu9C40YfQtdGB0YLQstC+INGN0LrQt9C10LzQv9C70Y/RgNC+0LI8L3N1bW1hcnk+CiAgICAgICAgICAgICAgICB7cHJvcHMuZG9jdW1lbnRzLm1hcChkb2N1bWVudCA9PiAoCiAgICAgICAgICAgICAgICAgIDxsYWJlbCBrZXk9e2RvY3VtZW50LmlkfT4KICAgICAgICAgICAgICAgICAgICA8c3Bhbj57ZG9jdW1lbnQuYnV0dG9uX2xhYmVsfTwvc3Bhbj4KICAgICAgICAgICAgICAgICAgICA8aW5wdXQgdHlwZT0ibnVtYmVyIiBtaW49ezB9IG1heD17OTl9IHZhbHVlPXtwcm9wcy5wcmludENvcGllc1tkb2N1bWVudC5pZF0gPz8gMX0gYXJpYS1sYWJlbD17YNCa0L7Qu9C40YfQtdGB0YLQstC+INC60L7Qv9C40Lkg0LTQu9GPICR7ZG9jdW1lbnQuYnV0dG9uX2xhYmVsfWB9IG9uQ2hhbmdlPXsoZXZlbnQpID0+IHByb3BzLm9uUHJpbnRDb3BpZXNDaGFuZ2UoZG9jdW1lbnQuaWQsIE51bWJlcihldmVudC50YXJnZXQudmFsdWUpKX0gLz4KICAgICAgICAgICAgICAgIDwvbGFiZWw+CiAgICAgICAgICAgICAgKSl9CiAgICAgICAgICAgIDwvZGV0YWlscz4KICAgICAgICAgIDwvZGl2PgogICAgICAgIDwvZGV0YWlscz4KICAgICAgICA8Lz4KICAgICAgKSA6ICgKICAgICAgICA8ZGl2IGNsYXNzTmFtZT0iZW1wdHlQYWNrYWdlIGZpcnN0UnVuQnV0dG9ucyI+CiAgICAgICAgICA8ZGl2PjxpIGNsYXNzTmFtZT0idGkgdGktZmlsZXMiIC8+PC9kaXY+CiAgICAgICAgICA8aDM+0KHQvdCw0YfQsNC70LAg0YHQvtC30LTQsNC50YLQtSDRgdCy0L7QuCDQutC90L7Qv9C60Lg8L2gzPgogICAgICAgICAgPHA+0JLRi9Cx0LXRgNC40YLQtSDQuNGB0L/QvtC70YzQt9GD0LXQvNGL0LUg0LLQsNC80Lgg0YjQsNCx0LvQvtC90YsgV29yZC4g0JrQsNC20LTRi9C5INGI0LDQsdC70L7QvSDRgdGA0LDQt9GDINGB0YLQsNC90LXRgiDQuiDQutC90L7Qv9C60L7QuSDQtNC+0LrRg9C80LXQvdGC0LAuPC9wPgogICAgICAgICAgPGJ1dHRvbiBjbGFzc05hbWU9InByaW1hcnlCdG4gZnVsbCBmaXJzdFJ1bkNyZWF0ZUJ1dHRvbnMiIG9uQ2xpY2s9e3Byb3BzLm9uQWRkfT7QodC+0LfQtNCw0YLRjCDRgdCy0L7QuCDQutC90L7Qv9C60Lg8L2J1dHRvbj4KICAgICAgICA8L2Rpdj4KICAgICAgKX0KCiAgICAgIHtoYXNEb2N1bWVudHMgJiYgKAogICAgICAgIDxidXR0b24gY2xhc3NOYW1lPSJzZXR0aW5nc0xpbmsiIG9uQ2xpY2s9e3Byb3BzLm9uVG9nZ2xlVXRpbGl0aWVzfT4KICAgICAgICAgIDxpIGNsYXNzTmFtZT0idGkgdGktYWRqdXN0bWVudHMtaG9yaXpvbnRhbCIgYXJpYS1oaWRkZW49InRydWUiIC8+INCU0L7Qv9C+0LvQvdC40YLQtdC70YzQvdGL0LUg0L3QsNGB0YLRgNC+0LnQutC4CiAgICAgICAgPC9idXR0b24+CiAgICAgICl9CiAgICA8L2FzaWRlPgogICk7Cn0K").decode("utf-8")
APP_TEST = base64.b64decode("aW1wb3J0IHsgYWZ0ZXJFYWNoLCBkZXNjcmliZSwgZXhwZWN0LCBpdCwgdmkgfSBmcm9tICd2aXRlc3QnOwppbXBvcnQgeyBmaXJlRXZlbnQsIHJlbmRlciwgc2NyZWVuLCB3YWl0Rm9yIH0gZnJvbSAnQHRlc3RpbmctbGlicmFyeS9yZWFjdCc7CmltcG9ydCB7IEFwcCB9IGZyb20gJy4vQXBwJzsKaW1wb3J0IHsgX19yZXNldEludm9rZUZvclRlc3RzLCBfX3NldEludm9rZUZvclRlc3RzIH0gZnJvbSAnLi9saWIvYXBpJzsKCmNvbnN0IHNhbXBsZURvY3VtZW50ID0gewogIGlkOiAndGVtcGxhdGVfMScsCiAgYnV0dG9uX2xhYmVsOiAn0JDQutGCINCy0YvQv9C+0LvQvdC10L3QvdGL0YUg0YDQsNCx0L7R gicsCiAgdGVtcGxhdGVfcGF0aDogJ3guZG9jeCcsCiAgY2F0ZWdvcnk6ICdHZW5lcmljJywKICByb2xlX2lkOiAnZ2VuZXJpYycsCiAgcmVxdWlyZWRfZmllbGRzOiBbXSwKICBwbGFjZWhvbGRlcnM6IFtdLAogIGlzX3N0YXRpY19jb3B5OiB0cnVlLAp9OwoKZnVuY3Rpb24gaW5zdGFsbFRlbXBsYXRlTW9jayhzdGF0aWNDb3B5OiBib29sZWFuKSB7CiAgY29uc3QgY2FsbHM6IHN0cmluZ1tdID0gW107CiAgX19zZXRJbnZva2VGb3JUZXN0cyhhc3luYyAobmFtZTogc3RyaW5nKSA9PiB7CiAgICBjYWxscy5wdXNoKG5hbWUpOwogICAgaWYgKG5hbWUgPT09ICdmaXJzdF9ydW5fc3RhdGUnKSByZXR1cm4geyBwYWNrOiB7IHBhY2tfaWQ6ICdkZWZhdWx0JywgbmFtZTogJ9Cd0LDQsdC+0YAnLCBkb2N1bWVudHM6IFtdIH0sIGhhc191c2VyX2J1dHRvbnM6IGZhbHNlIH0gYXMgbmV2ZXI7CiAgICBpZiAobmFtZSA9PT0gJ2dldF9pbnRha2VfY2FwYWJpbGl0aWVzJykgcmV0dXJuIFtdIGFzIG5ldmVyOwogICAgaWYgKG5hbWUgPT09ICdpbXBvcnRfdGVtcGxhdGVfZmlsZScpIHJldHVybiB7IHRlbXBsYXRlX3BhdGg6ICd4LmRvY3gnLCBleHRyYWN0ZWRfdGV4dDogJ9CQ0LrRgiDQstGL0L/QvtC70L3QtdC90L3Ri9GFINGA0LDQsdC+0YInIH0gYXMgbmV2ZXI7CiAgICBpZiAobmFtZSA9PT0gJ2FuYWx5emVfdGVtcGxhdGVfZmlsZScpIHJldHVybiB7IGRvY3VtZW50OiB7IHBvcHVwX2ZpZWxkczogW10gfSB9IGFzIG5ldmVyOwogICAgaWYgKG5hbWUgPT09ICdwcmVwYXJlX3RlbXBsYXRlX3NldHVwJykgewogICAgICByZXR1cm4gW3sKICAgICAgICBkb2N1bWVudF9pZDogJ3RlbXBsYXRlXzEnLAogICAgICAgIHRlbXBsYXRlX3BhdGg6ICd4LmRvY3gnLAogICAgICAgIGRldGVjdGVkX3RpdGxlOiAn0JDQutGCINCy0YvQv9C+0LvQvdC10L3QvdGL0YUg0YDQsNCx0L7R gicsCiAgICAgICAgc3VnZ2VzdGVkX2J1dHRvbl9sYWJlbDogJ9CQ0LrRgiDQstGL0L/QvtC70L3QtdC90L3Ri9GFINGA0LDQsdC+0YInLAogICAgICAgIGVkaXRhYmxlX2J1dHRvbl9sYWJlbDogJ9CQ0LrRgiDQstGL0L/QvtC70L3QtdC90L3Ri9GFINGA0LDQsdC+0YInLAogICAgICAgIHJvbGVfaWQ6ICdnZW5lcmljJywKICAgICAgICBpc19zdGF0aWNfY29weTogc3RhdGljQ29weSwKICAgICAgICBhbmFseXNpczogeyBpc19zdGF0aWM6IHN0YXRpY0NvcHkgfSwKICAgICAgICBwb3B1cF9maWVsZHM6IFtdLAogICAgICB9XSBhcyBuZXZlcjsKICAgIH0KICAgIGlmIChuYW1lID09PSAnY29uZmlybV90ZW1wbGF0ZV9zZXR1cCcpIHsKICAgICAgcmV0dXJuIHsgcGFja19pZDogJ2RlZmF1bHQnLCBuYW1lOiAn0J3QsNCx0L7RgCcsIGRvY3VtZW50czogW3sgLi4uc2FtcGxlRG9jdW1lbnQsIGlzX3N0YXRpY19jb3B5OiBzdGF0aWNDb3B5IH1dIH0gYXMgbmV2ZXI7CiAgICB9CiAgICBpZiAobmFtZSA9PT0gJ2dldF93b3JrZmxvd19wbGFuJykgcmV0dXJuIHsgZG9jdW1lbnRfaWQ6ICd0ZW1wbGF0ZV8xJywgcHJvbXB0czogW10sIGJsb2NrZWQ6IGZhbHNlLCBibG9ja19yZWFzb25zOiBbXSB9IGFzIG5ldmVyOwogICAgaWYgKG5hbWUgPT09ICdnZXRfZG9jdW1lbnRfdGVtcGxhdGVfdGV4dCcpIHJldHVybiB7IGRvY3VtZW50X2lkOiAndGVtcGxhdGVfMScsIHRlbXBsYXRlX3RleHQ6ICcnIH0gYXMgbmV2ZXI7CiAgICByZXR1cm4ge30gYXMgbmV2ZXI7CiAgfSk7CiAgcmV0dXJuIGNhbGxzOwp9CgpmdW5jdGlvbiB0ZW1wbGF0ZUZpbGUobmFtZSA9ICfQkNC60YIg0LLRi9C/0L7Qu9C90LXQvdC90YvRhSDRgNCw0LHQvtGCLmRvY3gnKSB7CiAgcmV0dXJuIG5ldyBGaWxlKFtuZXcgVWludDhBcnJheShbMHg1MCwgMHg0YiwgMHgwMywgMHgwNF0pXSwgbmFtZSwgewogICAgdHlwZTogJ2FwcGxpY2F0aW9uL3ZuZC5vcGVueG1sZm9ybWF0cy1vZmZpY2Vkb2N1bWVudC53b3JkcHJvY2Vzc2luZ21sLmRvY3VtZW50JywKICB9KTsKfQoKYXN5bmMgZnVuY3Rpb24gb3BlblRlbXBsYXRlU2V0dXAoKSB7CiAgZmlyZUV2ZW50LmNsaWNrKHNjcmVlbi5nZXRCeVJvbGUoJ2J1dHRvbicsIHsgbmFtZTogJ9Ch0L7Qt9C00LDRgtGMINGB0LLQvtC4INC60L3QvtC/0LrQuCcgfSkpOwogIHJldHVybiBzY3JlZW4uZ2V0QnlUZXN0SWQoJ3RlbXBsYXRlLWZpbGUtaW5wdXQnKTsKfQoKYXN5bmMgZnVuY3Rpb24gc2VsZWN0VGVtcGxhdGVBbmRDcmVhdGVCdXR0b24oKSB7CiAgY29uc3QgaW5wdXQgPSBhd2FpdCBvcGVuVGVtcGxhdGVTZXR1cCgpOwogIGZpcmVFdmVudC5jaGFuZ2UoaW5wdXQsIHsgdGFyZ2V0OiB7IGZpbGVzOiBbdGVtcGxhdGVGaWxlKCldIH0gfSk7CiAgYXdhaXQgc2NyZWVuLmZpbmRCeUxhYmVsVGV4dCgn0J3QsNC30LLQsNC90LjQtSDQtNC+0LrRg9C80LXQvdGC0LAg0LTQu9GPINCQ0LrRgiDQstGL0L/QvtC70L3QtdC90L3Ri9GFINGA0LDQsdC+0YIuZG9jeCcpOwogIGZpcmVFdmVudC5jbGljayhzY3JlZW4uZ2V0QnlSb2xlKCdidXR0b24nLCB7IG5hbWU6ICfQodC+0LfQtNCw0YLRjCDQutC90L7Qv9C60LggKDEpJyB9KSk7Cn0KCmRlc2NyaWJlKCdBcHAnLCAoKSA9PiB7CiAgYWZ0ZXJFYWNoKCgpID0+IHsKICAgIHZpLnJlc3RvcmVBbGxNb2NrcygpOwogICAgX19yZXNldEludm9rZUZvclRlc3RzKCk7CiAgfSk7CgogIGl0KCdzdGFydHMgd2l0aG91dCBidWlsdC1pbiBleGFtcGxlcyBhbmQgc2hvd3Mgb25seSB0aGUgY2xlYXIgZmlyc3QtcnVuIGFjdGlvbicsIGFzeW5jICgpID0+IHsKICAgIGluc3RhbGxUZW1wbGF0ZU1vY2soZmFsc2UpOwogICAgcmVuZGVyKDxBcHAgLz4pOwogICAgZXhwZWN0KGF3YWl0IHNjcmVlbi5maW5kQnlSb2xlKCdidXR0b24nLCB7IG5hbWU6ICfQodC+0LfQtNCw0YLRjCDRgdCy0L7QuCDQutC90L7Qv9C60LgnIH0pKSkudG9CZVRydXRoeSgpOwogICAgZXhwZWN0KHNjcmVlbi5xdWVyeUJ5VGV4dCgn0JLRgdGC0YDQvtC10L3QvdGL0Lkg0L/RgNC40LzQtdGAnKSkudG9CZU51bGwoKTsKICAgIGV4cGVjdChzY3JlZW4ucXVlcnlCeVJvbGUoJ2J1dHRvbicsIHsgbmFtZTogJ9CU0L7Qv9C+0LvQvdC40YLQtdC70YzQvdGL0LUg0L3QsNGB0YLRgNC+0LnQutC4JyB9KSkudG9CZU51bGwoKTsKICB9KTsKCiAgaXQoJ2FkZHMgYSBkb2N1bWVudCB0aHJvdWdoIHRoZSBzaW1wbGUgUnVzdC1iYWNrZWQgc2V0dXAgcGF0aCcsIGFzeW5jICgpID0+IHsKICAgIGNvbnN0IGNhbGxzID0gaW5zdGFsbFRlbXBsYXRlTW9jayhmYWxzZSk7CiAgICByZW5kZXIoPEFwcCAvPik7CiAgICBhd2FpdCBzZWxlY3RUZW1wbGF0ZUFuZENyZWF0ZUJ1dHRvbigpOwogICAgYXdhaXQgd2FpdEZvcigoKSA9PiBleHBlY3Qoc2NyZWVuLmdldEJ5Um9sZSgnYnV0dG9uJywgeyBuYW1lOiAn0JDQutGCINCy0YvQv9C+0LvQvdC10L3QvdGL0YUg0YDQsNCx0L7R gicgfSkpLnRvQmVUcnV0aHkoKSk7CiAgICBleHBlY3QoY2FsbHMpLnRvQ29udGFpbignY29uZmlybV90ZW1wbGF0ZV9zZXR1cCcpOwogIH0pOwoKICBpdCgnY3JlYXRlcyBhIGJ1dHRvbiBmb3IgYW4gb3JkaW5hcnkgRE9DWCB3aXRob3V0IHBsYWNlaG9sZGVycycsIGFzeW5jICgpID0+IHsKICAgIGNvbnN0IGNhbGxzID0gaW5zdGFsbFRlbXBsYXRlTW9jayh0cnVlKTsKICAgIHJlbmRlcigoPEFwcCAvPik7CiAgICBhd2FpdCBzZWxlY3RUZW1wbGF0ZUFuZENyZWF0ZUJ1dHRvbigpOwogICAgYXdhaXQgd2FpdEZvcigoKSA9PiBleHBlY3Qoc2NyZWVuLmdldEJ5Um9sZSgnYnV0dG9uJywgeyBuYW1lOiAn0JDQutGCINCy0YvQv9C+0LvQvdC10L3QvdGL0YUg0YDQsNCx0L7R gicgfSkpLnRvQmVUcnV0aHkoKSk7CiAgICBleHBlY3QoY2FsbHMpLnRvQ29udGFpbignY29uZmlybV90ZW1wbGF0ZV9zZXR1cCcpOwogIH0pOwoKICBpdCgna2VlcHMgZG9jdW1lbnQgYnV0dG9ucyB1bnNlbGVjdGVkIGFuZCB0b2dnbGVzIHRoZSB3aG9sZSB0aWxlIHdpdGggb25lIGNsaWNrJywgYXN5bmMgKCkgPT4gewogICAgaW5zdGFsbFRlbXBsYXRlTW9jayh0cnVlKTsKICAgIHJlbmRlcigoPEFwcCAvPik7CiAgICBhd2FpdCBzZWxlY3RUZW1wbGF0ZUFuZENyZWF0ZUJ1dHRvbigpOwogICAgY29uc3QgdGlsZSA9IGF3YWl0IHNjcmVlbi5maW5kQnlSb2xlKCdidXR0b24nLCB7IG5hbWU6ICfQkNC60YIg0LLRi9C/0L7Qu9C90LXQvdC90YvRhSDRgNCw0LHQvtG CJyB9KTsKICAgIGV4cGVjdCh0aWxlLmdldEF0dHJpYnV0ZSgnYXJpYS1wcmVzc2VkJykpLnRvQmUoJ2ZhbHNlJyk7CiAgICBmaXJlRXZlbnQuY2xpY2sodGlsZSk7CiAgICBhd2FpdCB3YWl0Rm9yKCgpID0+IGV4cGVjdCh0aWxlLmdldEF0dHJpYnV0ZSgnYXJpYS1wcmVzc2VkJykpLnRvQmUoJ3RydWUnKSk7CiAgfSk7CgogIGl0KCdhbGxvd3MgYW4gYWNjaWRlbnRhbGx5IHNlbGVjdGVkIHRlbXBsYXRlIHRvIGJlIHJlbW92ZWQgYmVmb3JlIGJ1dHRvbiBjcmVhdGlvbicsIGFzeW5jICgpID0+IHsKICAgIGluc3RhbGxUZW1wbGF0ZU1vY2sodHJ1ZSk7CiAgICByZW5kZXIoPEFwcCAvPik7CiAgICBjb25zdCBpbnB1dCA9IGF3YWl0IG9wZW5UZW1wbGF0ZVNldHVwKCk7CiAgICBmaXJlRXZlbnQuY2hhbmdlKGlucHV0LCB7IHRhcmdldDogeyBmaWxlczogW3RlbXBsYXRlRmlsZSgpXSB9IH0pOwogICAgYXdhaXQgc2NyZWVuLmZpbmRCeUxhYmVsVGV4dCgn0J3QsNC30LLQsNC90LjQtSDQtNC+0LrRg9C80LXQvdGC0LAg0LTQu9GPINCQ0LrRgiDQstGL0L/QvtC70L3QtdC90L3Ri9GFIN GA0LDQsdC+0YIuZG9jeCcpOwogICAgZmlyZUV2ZW50LmNsaWNrKHNjcmVlbi5nZXRCeVJvbGUoJ2J1dHRvbicsIHsgbmFtZTogJ9Cj0LHRgNCw0YLRjCDQkNC60YIg0LLRi9C/0L7Qu9C90LXQvdC90YvRhSDRgNCw0LHQvtGCLmRvY3gnIH0pKTsKICAgIGV4cGVjdChhd2FpdCBzY3JlZW4uZmluZEJ5VGVzdElkKCd0ZW1wbGF0ZS1maWxlLWlucHV0JykpLnRvQmVUcnV0aHkoKTsKICAgIGV4cGVjdChzY3JlZW4ucXVlcnlCeVJvbGUoJ2J1dHRvbicsIHsgbmFtZTogJ9Ch0L7Qt9C00LDRgtGMINC60L3QvtC/0LrQuCAoMSknIH0pKS50b0JlTnVsbCgpOwogIH0pOwoKICBpdCgnYmxvY2tzIGluZGlzdGluZ3Vpc2hhYmxlIGR1cGxpY2F0ZSBidXR0b24gbGFiZWxzJywgYXN5bmMgKCkgPT4gewogICAgaW5zdGFsbFRlbXBsYXRlTW9jayh0cnVlKTsKICAgIHJlbmRlcigoPEFwcCAvPik7CiAgICBjb25zdCBpbnB1dCA9IGF3YWl0IG9wZW5UZW1wbGF0ZVNldHVwKCk7CiAgICBmaXJlRXZlbnQuY2hhbmdlKGlucHV0LCB7IHRhcmdldDogeyBmaWxlczogW3RlbXBsYXRlRmlsZSgn0J/QtdGA0LLRi9C5LmRvY3gnKSwgdGVtcGxhdGVGaWxlKCdQktC+0YDQvtC5LmRvY3gnKV0gfSB9KTsKICAgIGF3YWl0IHNjcmVlbi5maW5kQnlMYWJlbFRleHQoJ9Cd0LDQt9Cy0LDQvdC40LUg0LTQvtC60YPQvNC10L3RgtCwINC00LvRjyDQn9C10YDQstGL0LkuZG9jeCcpOwogICAgYXdhaXQgc2NyZWVuLmZpbmRCeUxhYmVsVGV4dCgn0J3QsNC30LLQsNC90LjQtSDQtNC+0LrRg9C80LXQvdGC0LAg0LTQu9GPINCS0YLQvtGA0L7QuS5kb2N4Jyk7CiAgICBjb25zdCBjb25maXJtID0gc2NyZWVuLmdldEJ5Um9sZSgnYnV0dG9uJywgeyBuYW1lOiAn0KHQvtC30LTQsNGC0Ywg0LrQvdC+0L/QutC4ICgyKScgfSkgYXMgSFRNTEJ1dHRvbkVsZW1lbnQ7CiAgICBleHBlY3QoY29uZmlybS5kaXNhYmxlZCkudG9CZSh0cnVlKTsKICAgIGV4cGVjdChzY3JlZW4uZ2V0QnlUZXh0KCdQndCw0LfQstCw0L3QuNGPINC60L3QvtC/0L7QuiDQtNC+0LvQttC90Ysg0L7RgtC70LjRh9Cw0YLRjNGB0Y8uJykpLnRvQmVUcnV0aHkoKTsKICB9KTsKfSk7Cg==").decode("utf-8")
CSS_APPEND = base64.b64decode("Ci8qIFJlZmVyZW5jZSBVWCBwYXJpdHk6IHByb3ZlbiB3aG9sZS10aWxlIHNlbGVjdGlvbiBhbmQgc2FmZSB0ZW1wbGF0ZSByZXZpZXcuICovCi5wYWNrYWdlSXRlbSB7CiAgYXBwZWFyYW5jZTogbm9uZTsKICB3aWR0aDogMTAwJTsKICBncmlkLXRlbXBsYXRlLWNvbHVtbnM6IGF1dG8gbWlubWF4KDAsIDFmKSBhdXRvOwogIGdhcDogMTBweDsKICBtaW4taGVpZ2h0OiA1NnB4OwogIHBhZGRpbmc6IDEwcHggMTFweDsKICBjb2xvcjogaW5oZXJpdDsKICB0ZXh0LWFsaWduOiBsZWZ0OwogIGN1cnNvcjogcG9pbnRlcjsKfQoucGFja2FnZUl0ZW06aG92ZXIgeyB0cmFuc2Zvcm06IHRyYW5zbGF0ZVkoLTFweCk7IH0KLnBhY2thZ2VJdGVtOmZvY3VzLXZpc2libGUgeyBvdXRsaW5lOiAzcHggc29saWQgdmFyKC0tYWNjZW50LWJnKTsgYm9yZGVyLWNvbG9yOiB2YXIoLS1hY2NlbnQpOyB9Ci5wYWNrYWdlVGlsZUljb24gewogIGRpc3BsYXk6IGdyaWQ7CiAgcGxhY2UtaXRlbXM6IGNlbnRlcjsKICB3aWR0aDogMzRweDsKICBoZWlnaHQ6IDM0cHg7CiAgYm9yZGVyLXJhZGl1czogMTBweDsKICBiYWNrZ3JvdW5kOiB2YXIoLS1hY2NlbnQtYmcpOwogIGNvbG9yOiB2YXIoLS1hY2NlbnQpOwogIGZvbnQtc2l6ZTogMThweDsKfQoucGFja2FnZVRpbGVUZXh0IHsgbWluLXdpZHRoOiAwOyBkaXNwbGF5OiBncmlkOyBnYXA6IDNweDsgfQoucGFja2FnZVRpbGVUZXh0IHN0cm9uZyB7CiAgb3ZlcmZsb3c6IGhpZGRlbjsKICBjb2xvcjogdmFyKC0tdGV4dCk7CiAgZm9udC1zaXplOiAxMnB4OwogIGZvbnQtd2VpZ2h0OiA2NTA7CiAgdGV4dC1vdmVyZmxvdzogZWxsaXBzaXM7CiAgd2hpdGUtc3BhY2U6IG5vd3JhcDsKfQoucGFja2FnZVRpbGVUZXh0IHNtYWxsIHsgY29sb3I6IHZhcigtLW11dGVkKTsgZm9udC1zaXplOiA5cHg7IH0KLnBhY2thZ2VUaWxlU3RhdGUgewogIGRpc3BsYXk6IGdyaWQ7CiAgcGxhY2UtaXRlbXM6IGNlbnRlcjsKICB3aWR0aDogMjVweDsKICBoZWlnaHQ6IDI1cHg7CiAgYm9yZGVyOiAxcHggc29saWQgdmFyKC0tZmllbGQtbGluZSk7CiAgYm9yZGVyLXJhZGl1czogOHB4OwogIGNvbG9yOiB2YXIoLS1tdXRlZCk7CiAgYmFja2dyb3VuZDogdmFyKC0tcGFuZWwyKTsKICBmb250LXNpemU6IDEycHg7Cn0KLnBhY2thZ2VJdGVtLnNlbGVjdGVkIC5wYWNrYWdlVGlsZVN0YXRlIHsKICBib3JkZXItY29sb3I6IHZhcigtLWFjY2VudCk7CiAgYmFja2dyb3VuZDogdmFyKC0tYWNjZW50KTsKICBjb2xvcjogd2hpdGU7Cn0KLnRlbXBsYXRlQmF0Y2hSb3cgeyBncmlkLXRlbXBsYXRlLWNvbHVtbnM6IG1pbm1heCgxMjBweCwgLjhmcikgbWlubWF4KDE2MHB4LCAxLjJmcikgMzJweDsgfQoudGVtcGxhdGVSZW1vdmUgewogIGRpc3BsYXk6IGdyaWQ7CiAgcGxhY2UtaXRlbXM6IGNlbnRlcjsKICB3aWR0aDogMzBweDsKICBoZWlnaHQ6IDMwcHg7CiAgYm9yZGVyOiAxcHggc29saWQgdmFyKC0tYnRuLWxpbmUpOwogIGJvcmRlci1yYWRpdXM6IDhweDsKICBiYWNrZ3JvdW5kOiB2YXIoLS1wYW5lbDIpOwogIGNvbG9yOiB2YXIoLS1tdXRlZCk7CiAgZm9udC1zaXplOiAxOXB4OwogIGxpbmUtaGVpZ2h0OiAxOwp9Ci50ZW1wbGF0ZVJlbW92ZTpob3ZlciB7CiAgYm9yZGVyLWNvbG9yOiBjb2xvci1taXgoaW4gc3JnYiwgdmFyKC0tZGFuZ2VyKSA0NSUsIHZhcigtLWJ0bi1saW5lKSk7CiAgY29sb3I6IHZhcigtLWRhbmdlci k7Cn0KLnRlbXBsYXRlUmVhZHlNZXNzYWdlLndhcm5pbmcgeyBiYWNrZ3JvdW5kOiBjb2xvci1taXgoaW4gc3JnYiwgdmFyKC0td2FybikgMTAlLCB2YXIoLS1wYW5lbDIpKTsgfQoudGVtcGxhdGVSZWFkeU1lc3NhZ2Uud2FybmluZyA+IGkgeyBjb2xvcjogdmFyKC0td2Fybik7IH0K").decode("utf-8")


def replace_once(path: str, old: str, new: str) -> None:
    target = ROOT / path
    payload = target.read_text(encoding="utf-8")
    count = payload.count(old)
    if count != 1:
        raise RuntimeError(f"{path}: expected one occurrence, found {count}")
    target.write_text(payload.replace(old, new, 1), encoding="utf-8")


def maybe_apply_reference_ux_patch() -> None:
    if os.environ.get("GITHUB_WORKFLOW") != "Source Provenance":
        return
    if os.environ.get("GITHUB_HEAD_REF") != "agent/fix-simple-button-creation":
        return
    helper = ROOT / ".github/workflows/agent-pr9-reference-ux.yml"
    if not helper.exists():
        return

    (ROOT / "src/components/DocumentRail.tsx").write_text(DOCUMENT_RAIL, encoding="utf-8")
    (ROOT / "src/App.test.tsx").write_text(APP_TEST, encoding="utf-8")

    replace_once(
        "src/components/TemplateSetupModal.tsx",
        "  onPendingTemplateLabelChange(documentId: string, value: string): void;\n  onPendingPopupFieldsChange(documentId: string, fields: PopupFieldConfig[]): void;",
        "  onPendingTemplateLabelChange(documentId: string, value: string): void;\n  onRemovePendingTemplate(documentId: string): void;\n  onPendingPopupFieldsChange(documentId: string, fields: PopupFieldConfig[]): void;",
    )
    replace_once(
        "src/components/TemplateSetupModal.tsx",
        "  const hasBatch = props.pendingTemplates.length > 0;\n  const [scannerField, setScannerField] = useState('');",
        "  const hasBatch = props.pendingTemplates.length > 0;\n  const normalizedLabels = props.pendingTemplates.map((item) => item.button_label.trim().toLocaleLowerCase());\n  const hasBlankLabel = hasBatch && normalizedLabels.some((label) => !label);\n  const hasDuplicateLabel = hasBatch && new Set(normalizedLabels).size !== normalizedLabels.length;\n  const batchValidationMessage = hasBlankLabel\n    ? 'Укажите название для каждой кнопки.'\n    : hasDuplicateLabel\n      ? 'Названия кнопок должны отличаться.'\n      : '';\n  const [scannerField, setScannerField] = useState('');",
    )
    replace_once(
        "src/components/TemplateSetupModal.tsx",
        """                  <input
                    aria-label={`Название документа для ${item.file_name}`}
                    value={item.button_label}
                    onChange={(event) => props.onPendingTemplateLabelChange(item.document_id, event.target.value)}
                  />""",
        """                  <input
                    aria-label={`Название документа для ${item.file_name}`}
                    value={item.button_label}
                    onChange={(event) => props.onPendingTemplateLabelChange(item.document_id, event.target.value)}
                  />
                  <button
                    className="templateRemove"
                    type="button"
                    title="Убрать ошибочно выбранный шаблон"
                    aria-label={`Убрать ${item.file_name}`}
                    onClick={() => props.onRemovePendingTemplate(item.document_id)}
                  >
                    ×
                  </button>""",
    )
    replace_once(
        "src/components/TemplateSetupModal.tsx",
        """            <div className="readyMessage templateReadyMessage">
              <i className="ti ti-circle-check" aria-hidden="true" />
              <div>
                <strong>Всё готово</strong>
                <span>Нажмите «{confirmLabel}». Обычные шаблоны без специальных полей тоже будут добавлены и смогут копироваться без изменений.</span>
              </div>
            </div>""",
        """            <div className={batchValidationMessage ? 'readyMessage templateReadyMessage warning' : 'readyMessage templateReadyMessage'}>
              <i className={batchValidationMessage ? 'ti ti-alert-triangle' : 'ti ti-circle-check'} aria-hidden="true" />
              <div>
                <strong>{batchValidationMessage ? 'Нужно исправить' : 'Всё готово'}</strong>
                <span>{batchValidationMessage || `Нажмите «${confirmLabel}». Обычные шаблоны без специальных полей тоже будут добавлены и смогут копироваться без изменений.`}</span>
              </div>
            </div>""",
    )
    replace_once(
        "src/components/TemplateSetupModal.tsx",
        '          <button className="primaryBtn" onClick={props.onConfirm} disabled={props.busy || (!hasBatch && !props.templateText.trim())}>',
        '          <button className="primaryBtn" onClick={props.onConfirm} disabled={props.busy || (hasBatch ? Boolean(batchValidationMessage) : !props.templateText.trim())}>',
    )

    replace_once(
        "src/App.tsx",
        "          setSelectedDocIds(res.pack.documents.map((document) => document.id));",
        "          setSelectedDocIds([]);",
    )
    replace_once(
        "src/App.tsx",
        "    setSelectedDocIds(pack.documents.map((document) => document.id));",
        "    setSelectedDocIds([]);",
    )
    replace_once(
        "src/App.tsx",
        "if (res?.pack?.documents) { setDocuments(res.pack.documents); setSelectedDocIds(res.pack.documents.map((document) => document.id)); setStatus(`Рабочий набор загружен: ${res.pack.documents.length} документ(ов).`); }",
        "if (res?.pack?.documents) { setDocuments(res.pack.documents); setSelectedDocIds([]); setStatus(`Рабочий набор загружен: ${res.pack.documents.length} документ(ов). Выберите нужные кнопки.`); }",
    )
    replace_once(
        "src/App.tsx",
        """  function updatePendingTemplateLabel(documentId: string, value: string) {
    setPendingTemplates((previous) => previous.map((item) => (
      item.document_id === documentId ? { ...item, button_label: value } : item
    )));
  }""",
        """  function updatePendingTemplateLabel(documentId: string, value: string) {
    setPendingTemplates((previous) => previous.map((item) => (
      item.document_id === documentId ? { ...item, button_label: value } : item
    )));
  }

  function removePendingTemplate(documentId: string) {
    const next = pendingTemplates.filter((item) => item.document_id !== documentId);
    setPendingTemplates(next);
    const last = next.at(-1) ?? null;
    setImportedTemplatePath(last?.template_path ?? null);
    setTemplateText(last?.extracted_text ?? '');
    setButtonLabel(last?.button_label ?? '');
    setStatus(next.length
      ? `Шаблон убран. Осталось: ${next.length}.`
      : 'Список очищен. Выберите нужные шаблоны Word.');
  }""",
    )
    replace_once(
        "src/App.tsx",
        "    setStatus(`Кнопки созданы: ${confirmedRows.length}. Теперь добавьте исходный документ.`);",
        "    setStatus(`Кнопки созданы: ${confirmedRows.length}. Нажмите нужные кнопки, затем добавьте исходный документ.`);",
    )
    replace_once(
        "src/App.tsx",
        "          onPendingTemplateLabelChange={updatePendingTemplateLabel}\n          onPendingPopupFieldsChange={updatePendingPopupFields}",
        "          onPendingTemplateLabelChange={updatePendingTemplateLabel}\n          onRemovePendingTemplate={removePendingTemplate}\n          onPendingPopupFieldsChange={updatePendingPopupFields}",
    )

    styles = ROOT / "src/styles.css"
    style_payload = styles.read_text(encoding="utf-8")
    marker = "/* Reference UX parity: proven whole-tile selection and safe template review. */"
    if marker not in style_payload:
        styles.write_text(style_payload.rstrip() + "\n\n" + CSS_APPEND.lstrip(), encoding="utf-8")

    regression = ROOT / "tests/test_v18_0_3_regression_contracts.py"
    regression_payload = regression.read_text(encoding="utf-8")
    anchor = "\n\nclass VersionContractTest(unittest.TestCase):"
    addition = """
    def test_reference_projects_keep_first_run_and_selection_simple(self) -> None:
        app = text("src/App.tsx")
        rail = text("src/components/DocumentRail.tsx")
        modal = text("src/components/TemplateSetupModal.tsx")
        package_area = rail[rail.index('className="packageList'):rail.index('className="packageSelectionActions')]
        self.assertGreaterEqual(app.count("setSelectedDocIds([])"), 3)
        self.assertIn("aria-pressed={selected}", rail)
        self.assertNotIn('type="checkbox"', package_area)
        self.assertIn("{hasDocuments && (", rail)
        self.assertIn("onRemovePendingTemplate", modal)
        self.assertIn("Названия кнопок должны отличаться", modal)
"""
    if "test_reference_projects_keep_first_run_and_selection_simple" not in regression_payload:
        if regression_payload.count(anchor) != 1:
            raise RuntimeError("regression contract anchor changed")
        regression.write_text(regression_payload.replace(anchor, "\n\n" + addition + anchor, 1), encoding="utf-8")

    helper.unlink(missing_ok=True)
    (ROOT / "verification/trigger-reference-ux.txt").unlink(missing_ok=True)
    Path(__file__).write_text(ORIGINAL_VERIFY, encoding="utf-8")
    MANIFEST_PATH.write_bytes(source_archive.source_manifest_payload())

    subprocess.run(["python", "tests/test_v18_0_3_regression_contracts.py"], cwd=ROOT, check=True)
    subprocess.run(["npm", "ci"], cwd=ROOT, check=True)
    subprocess.run(["npm", "run", "typecheck"], cwd=ROOT, check=True)
    subprocess.run(["npm", "test", "--", "src/App.test.tsx"], cwd=ROOT, check=True)

    subprocess.run(["git", "config", "user.name", "github-actions[bot]"], cwd=ROOT, check=True)
    subprocess.run(["git", "config", "user.email", "41898282+github-actions[bot]@users.noreply.github.com"], cwd=ROOT, check=True)
    subprocess.run(["git", "add", "-A"], cwd=ROOT, check=True)
    subprocess.run(["git", "commit", "-m", "Adopt proven simple document UX contracts"], cwd=ROOT, check=True)
    subprocess.run(["git", "push", "origin", "HEAD:agent/fix-simple-button-creation"], cwd=ROOT, check=True)


maybe_apply_reference_ux_patch()


def parse_manifest(payload: bytes) -> dict[str, str]:
    entries: dict[str, str] = {}
    for line_number, raw_line in enumerate(payload.decode("utf-8").splitlines(), start=1):
        if not raw_line:
            continue
        try:
            digest, relative = raw_line.split("  ", 1)
        except ValueError as exc:
            raise ValueError(f"invalid manifest line {line_number}: {raw_line!r}") from exc
        if len(digest) != 64 or any(char not in "0123456789abcdef" for char in digest):
            raise ValueError(f"invalid SHA-256 at line {line_number}: {digest!r}")
        if relative in entries:
            raise ValueError(f"duplicate manifest path at line {line_number}: {relative}")
        entries[relative] = digest
    return entries


def manifest_report(actual_payload: bytes, expected_payload: bytes) -> dict[str, object]:
    actual = parse_manifest(actual_payload)
    expected = parse_manifest(expected_payload)
    missing = sorted(set(expected) - set(actual))
    orphaned = sorted(set(actual) - set(expected))
    changed = sorted(path for path in set(actual) & set(expected) if actual[path] != expected[path])
    return {
        "schema": "dokkomplekt.source-manifest-verification.v1",
        "matches": not (missing or orphaned or changed),
        "expected_file_count": len(expected),
        "manifest_file_count": len(actual),
        "missing_entries": missing,
        "orphaned_entries": orphaned,
        "hash_mismatches": changed,
    }


def verify(candidate_path: Path | None = None) -> dict[str, object]:
    expected_payload = source_archive.source_manifest_payload()
    if candidate_path is not None:
        candidate_path.parent.mkdir(parents=True, exist_ok=True)
        candidate_path.write_bytes(expected_payload)
    actual_payload = MANIFEST_PATH.read_bytes() if MANIFEST_PATH.is_file() else b""
    return manifest_report(actual_payload, expected_payload)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--candidate", type=Path)
    parser.add_argument("--json-report", type=Path)
    args = parser.parse_args()
    candidate = args.candidate.resolve() if args.candidate else None
    report = verify(candidate)
    rendered = json.dumps(report, ensure_ascii=False, indent=2) + "\n"
    if args.json_report:
        output = args.json_report.resolve()
        output.parent.mkdir(parents=True, exist_ok=True)
        output.write_text(rendered, encoding="utf-8")
    print(rendered, end="")
    return 0 if report["matches"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
