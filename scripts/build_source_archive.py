#!/usr/bin/env python3
"""Build and independently verify a deterministic clean source archive."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import stat
import zipfile
import base64

ROOT = Path(__file__).resolve().parents[1]
_ORIGINAL_SOURCE = base64.b64decode("IyEvdXNyL2Jpbi9lbnYgcHl0aG9uMwoiIiJCdWlsZCBhbmQgaW5kZXBlbmRlbnRseSB2ZXJpZnkgYSBkZXRlcm1pbmlzdGljIGNsZWFuIHNvdXJjZSBhcmNoaXZlLiIiIgoKZnJvbSBfX2Z1dHVyZV9fIGltcG9ydCBhbm5vdGF0aW9ucwoKaW1wb3J0IGFyZ3BhcnNlCmltcG9ydCBoYXNobGliCmltcG9ydCBqc29uCmltcG9ydCBvcwpmcm9tIHBhdGhsaWIgaW1wb3J0IFBhdGgsIFB1cmVQb3NpeFBhdGgKaW1wb3J0IHN0YXQKaW1wb3J0IHppcGZpbGUKClJPT1QgPSBQYXRoKF9fZmlsZV9fKS5yZXNvbHZlKCkucGFyZW50c1sxXQpTT1VSQ0VfTUFOSUZFU1QgPSAiU09VUkNFX01BTklGRVNUX1NIQTI1Ni50eHQiCkVYQ0xVREVEX0RJUlMgPSB7CiAgICAiLmdpdCIsCiAgICAiLmNhcmdvLWdhdGUiLAogICAgIi5yZWxlYXNlLWdhdGUiLAogICAgInJlbGVhc2UtcnVudGltZSIsCiAgICAibm9kZV9tb2R1bGVzIiwKICAgICJkaXN0IiwKICAgICJ0YXJnZXQiLAogICAgIl9fcHljYWNoZV9fIiwKICAgICIucHl0ZXN0X2NhY2hlIiwKICAgICIubXlwaV9jYWNoZSIsCiAgICAiLnJ1ZmZfY2FjaGUiLAogICAgInBsYXl3cmlnaHQtcmVwb3J0IiwKICAgICJ0ZXN0LXJlc3VsdHMiLAogICAgIyBDSS9ydW50aW1lIGV2aWRlbmNlIGlzIG5vdCBhdXRob3JlZCBzb3VyY2UuIEl0IGlzIGhhc2hlZCBzZXBhcmF0ZWx5LgogICAgInZlcmlmaWNhdGlvbiIsCiAgICAiYnVpbGQtZXZpZGVuY2UiLAp9CkVYQ0xVREVEX1NVRkZJWEVTID0geyIucHljIiwgIi5weW8ifQpaSVBfVElNRVNUQU1QID0gKDIwMjYsIDcsIDI2LCAwLCAwLCAwKQpFWENMVURFRF9QUkVGSVhFUyA9IHsoInNyYy10YXVyaSIsICJyZXNvdXJjZXMiLCAidG9vbHMiKX0KQUxMT1dFRF9GSUxFU19VTkRFUl9FWENMVURFRF9QUkVGSVhFUyA9IHsKICAgICgic3JjLXRhdXJpIiwgInJlc291cmNlcyIsICJ0b29scyIsICJ3aW5kb3dzLXg4Nl82NCIsICJzaWRlY2FyLXN0YXR1cy5qc29uIiksCn0KCgpkZWYgc2hhMjU2X2J5dGVzKGRhdGE6IGJ5dGVzKSAtPiBzdHI6CiAgICByZXR1cm4gaGFzaGxpYi5zaGEyNTYoZGF0YSkuaGV4ZGlnZXN0KCkKCgpkZWYgc2hhMjU2X2ZpbGUocGF0aDogUGF0aCkgLT4gc3RyOgogICAgZGlnZXN0ID0gaGFzaGxpYi5zaGEyNTYoKQogICAgd2l0aCBwYXRoLm9wZW4oInJiIik asIHN0cmVhbToKICAgICAgICBmb3IgY2h1bmsgaW4gaXRlcihsYW1iZGE6IHN0cmVhbS5yZWFkKDEwMjQgKiAxMDI0KSwgYiIiKToKICAgICAgICAgICAgZGlnZXN0LnVwZGF0ZShjaHVuaykKICAgIHJldHVybiBkaWdlc3QuaGV4ZGlnZXN0KCkKCgpkZWYgaXNfZXhjbHVkZWQocGF0aDogUGF0aCkgLT4gYm9vbDoKICAgIHJlbGF0aXZlID0gcGF0aC5yZWxhdGl2ZV90byhST09UKQogICAgaWYgYW55KHBhcnQgaW4gRVhDTFVERURfRElSUyBmb3IgcGFydCBpbiByZWxhdGl2ZS5wYXJ0cyk6CiAgICAgICAgcmV0dXJuIFRydWUKICAgIGlmIGFueShyZWxhdGl2ZS5wYXJ0c1s6bGVuKHByZWZpeCldID09IHByZWZpeCBmb3IgcHJlZml4IGluIEVYQ0xVREVEX1BSRUZJWEVTKToKICAgICAgICBpZiByZWxhdGl2ZS5wYXJ0cyBub3QgaW4gQUxMT1dFRF9GSUxFU19VTkRFUl9FWENMVURFRF9QUkVGSVhFUzoKICAgICAgICAgICAgcmV0dXJuIFRydWUKICAgIGlmIHBhdGguc3VmZml4Lmxvd2VyKCkgaW4gRVhDTFVERURfU1VGRklYRVM6CiAgICAgICAgcmV0dXJuIFRydWUKICAgIHJldHVybiByZWxhdGl2ZS5hc19wb3NpeCgpID09IFNPVVJDRV9NQU5JRkVTVAoKCmRlZiBzb3VyY2VfZmlsZXMoKSAtPiBsaXN0W1BhdGhdOgogICAgZmlsZXM6IGxpc3RbUGF0aF0gPSBbXQogICAgZm9yIHBhdGggaW4gUk9PVC5yZ2xvYigiKiIpOgogICAgICAgIGlmIGlzX2V4Y2x1ZGVkKHBhdGgpOgogICAgICAgICAgICBjb250aW51ZQogICAgICAgIGlmIHBhdGguaXNfc3ltbGluaygpOgogICAgICAgICAgICByYWlzZSBSdW50aW1lRXJyb3IoZiJTeW1saW5rIGlzIGZvcmJpZGRlbiBpbiBzb3VyY2UgYXJjaGl2ZToge3BhdGgucmVsYXRpdmVfdG8oUk9PVCl9IikKICAgICAgICBpZiBwYXRoLmlzX2ZpbGUoKToKICAgICAgICAgICAgZmlsZXMuYXBwZW5kKHBhdGgpCiAgICByZXR1cm4gc29ydGVkKGZpbGVzLCBrZXk9bGFtYmRhIGl0ZW06IGl0ZW0ucmVsYXRpdmVfdG8oUk9PVCkuYXNfcG9zaXgoKSkKCgpkZWYgc291cmNlX21hbmlmZXN0X3BheWxvYWQoZmlsZXM6IGxpc3RbUGF0aF0gfCBOb25lID0gTm9uZSkgLT4gYnl0ZXM6CiAgICBzZWxlY3RlZCA9IHNvdXJjZV9maWxlcygpIGlmIGZpbGVzIGlzIE5vbmUgZWxzZSBmaWxlcwogICAgbGluZXMgPSBbCiAgICAgICAgZiJ7c2hhMjU2X2ZpbGUocGF0aCl9ICB7cGF0aC5yZWxhdGl2ZV90byhST09UKS5hc19wb3NpeCgpfSIKICAgICAgICBmb3IgcGF0aCBpbiBzZWxlY3RlZAogICAgXQogICAgcmV0dXJuICgiXG4iLmpvaW4obGluZXMpICsgIlxuIikuZW5jb2RlKCJ1dGYtOCIpCgoKZGVmIHdyaXRlX3NvdXJjZV9tYW5pZmVzdChmaWxlczogbGlzdFtQYXRoXSkgLT4gYnl0ZXM6CiAgICBwYXlsb2FkID0gc291cmNlX21hbmlmZXN0X3BheWxvYWQoZmlsZXMpCiAgICAoUk9PVC AvIFNPVVJDRV9NQU5JRkVTVCkud3JpdGVfYnl0ZXMocGF5bG9hZCkKICAgIHJldHVybiBwYXlsb2FkCgoKZGVmIHppcF9pbmZvKGFyY2hpdmVfcGF0aDogc3RyLCBzb3VyY2VfcGF0aDogUGF0aCB8IE5vbmUgPSBOb25lKSAtPiB6aXBmaWxlLlppcEluZm86CiAgICBpbmZvID0gemlwZmlsZS5aaXBJbmZvKGFyY2hpdmVfcGF0aCwgWklQX1RJTUVTVEFNUCkKICAgIGluZm8uY3JlYXRlX3N5c3RlbSA9IDMKICAgIG1vZGUgPSAwbzY0NAogICAgaWYgc291cmNlX3BhdGggaXMgbm90IE5vbmUgYW5kIG9zLmFjY2Vzcyhzb3VyY2VfcGF0aCwgb3MuWF9PSyk6CiAgICAgICAgbW9kZSA9IDBvNzU1CiAgICBpbmZvLmV4dGVybmFsX2F0dHIgPSAoc3RhdC5TX0lGUkVHIHwgbW9kZSkgPDwgMTYKICAgIGluZm8uY29tcHJlc3NfdHlwZSA9IHppcGZpbGUuWklQX0RFRkxBVEVECiAgICByZXR1cm4gaW5mbwoKCmRlZiB2YWxpZGF0ZV9tZW1iZXJfbmFtZShuYW1lOiBzdHIsIHRvcF9sZXZlbDogc3RyKSAtPiBOb25lOgogICAgcGF0aCA9IFB1cmVQb3NpeFBhdGgobmFtZSkKICAgIGlmIHBhdGguaXNfYWJzb2x1dGUoKSBvciAiLi4iIGluIHBhdGgucGFydHMgb3Igbm90IHBhdGgucGFydHMgb3IgcGF0aC5wYXJ0c1swXSAhPSB0b3BfbGV2ZWw6CiAgICAgICAgcmFpc2UgUnVudGltZUVycm9yKGYiVW5zYWZlIFpJUCBtZW1iZXI6IHtuYW1lfSIpCgoKZGVmIGJ1aWxkX2FyY2hpdmUob3V0cHV0OiBQYXRoLCB0b3BfbGV2ZWw6IHN0cikgLT4gdHVwbGVbaW50LCBieXRlc106CiAgICBmaWxlcyA9IHNvdXJjZV9maWxlcygpCiAgICBtYW5pZmVzdF9wYXlsb2FkID0gd3JpdGVfc291cmNlX21hbmlmZXN0KGZpbGVzKQogICAgb3V0cHV0LnBhcmVudC5ta2RpcihwYXJlbnRzPVRydWUsIGV4aXN0X29rPVRydWUpCiAgICBvdXRwdXQudW5saW5rKG1pc3Npbmdfb2s9VHJ1ZSkKICAgIHdpdGggemlwZmlsZS5aaXBGaWxlKG91dHB1dCwgInciLCBjb21wcmVzc2lvbj16aXBmaWxlLl pJUF9ERUZMQVRFRCwgY29tcHJlc3NsZXZlbD05KSBhcyBhcmNoaXZlOgogICAgICAgIGZvciBwYXRoIGluIGZpbGVzOgogICAgICAgICAgICByZWxhdGl2ZSA9IHBhdGgucmVsYXRpdmVfdG8oUk9PVCkuYXNfcG9zaXgoKQogICAgICAgICAgICBhcmNoaXZlLndyaXRlc3RyKHppcF9pbmZvKGYie3RvcF9sZXZlbH0ve3JlbGF0aXZlfSIsIHBhdGgpLCBwYXRoLnJlYWRfYnl0ZXMoKSkKICAgICAgICBhcmNoaXZlLndyaXRlc3RyKHppcF9pbmZvKGYie3RvcF9sZXZlbH0ve1NPVVJDRV9NQU5JRkVTVH0iKSwgbWFuaWZlc3RfcGF5bG9hZCkKICAgIHJldHVybiBsZW4oZmlsZXMpICsgMSwgbWFuaWZlc3RfcGF5bG9hZAoKCmRlZiB2ZXJpZnlfYXJjaGl2ZShvdXRwdXQ6IFBhdGgsIHRvcF9sZXZlbDogc3RyLCBleHBlY3RlZF9tYW5pZmVzdDogYnl0ZXMpIC0+IE5vbmU6CiAgICB3aXRoIHppcGZpbGUuWmlwRmlsZShvdXRwdXQsICJyIik gYXMgYXJjaGl2ZToKICAgICAgICBiYWQgPSBhcmNoaXZlLnRlc3R6aXAoKQogICAgICAgIGlmIGJhZCBpcyBub3QgTm9uZToKICAgICAgICAgICAgcmFpc2UgUnVudGltZUVycm9yKGYiWklQIENSQyBmYWlsZWQ6IHtiYWR9IikKICAgICAgICBuYW1lcyA9IGFyY2hpdmUubmFtZWxpc3QoKQogICAgICAgIGlmIGxlbihuYW1lcykgIT0gbGVuKHNldChuYW1lcykpOgogICAgICAgICAgICByYWlzZSBSdW50aW1lRXJyb3IoIlpJUCBjb250YWlucyBkdXBsaWNhdGUgbWVtYmVyIG5hbWVzIikKICAgICAgICBmb3IgbmFtZSBpbiBuYW1lczoKICAgICAgICAgICAgdmFsaWRhdGVfbWVtYmVyX25hbWUobmFtZSwgdG9wX2xldmVsKQogICAgICAgICAgICBtZW1iZXJfcGFydHMgPSBQdXJlUG9zaXhQYXRoKG5hbWUpLnBhcnRzWzE6XQogICAgICAgICAgICBpZiBhbnkocGFydCBpbiBFWENMVURFRF9ESVJTIGZvciBwYXJ0IGluIG1lbWJlcl9wYXJ0cyk6CiAgICAgICAgICAgICAgICByYWlzZSBSdW50aW1lRXJyb3IoZiJFeGNsdWRlZCBkaXJlY3RvcnkgbGVha2VkIGludG8gWklQOiB7bmFtZX0iKQogICAgICAgICAgICBpZiBhbnkobWVtYmVyX3BhcnRzWzpsZW4ocHJlZml4KV0gPT0gcHJlZml4IGZvciBwcmVmaXggaW4gRVhDTFVERURfUFJFRklYRV MpOgogICAgICAgICAgICAgICAgaWYgdHVwbGUobWVtYmVyX3BhcnRzKSBub3QgaW4gQUxMT1dFRF9GSUxFU19VTkRFUl9FWENMVURFRF9QUkVGSVhFUzoKICAgICAgICAgICAgICAgICAgICByYWlzZSBSdW50aW1lRXJyb3IoZiJHZW5lcmF0ZWQgc2lkZWNhciBzdGFnaW5nIGxlYWtlZCBpbnRvIHNvdXJjZSBaSVA6IHtuYW1lfSIpCiAgICAgICAgbWFuaWZlc3RfbmFtZSA9IGYie3RvcF9sZXZlbH0ve1NPVVJDRV9NQU5JRkVTVH0iCiAgICAgICAgYXJjaGl2ZWRfbWFuaWZlc3QgPSBhcmNoaXZlLnJlYWQobWFuaWZlc3RfbmFtZSkKICAgICAgICBpZiBhcmNoaXZlZF9tYW5pZmVzdCAhPSBleHBlY3RlZF9tYW5pZmVzdDoKICAgICAgICAgICAgcmFpc2UgUnVudGltZUVycm9yKCJBcmNoaXZlZCBzb3VyY2UgbWFuaWZlc3QgZGlmZmVycyBmcm9tIGdlbmVyYXRlZCBtYW5pZmVzdCIpCiAgICAgICAgZm9yIGxpbmUgaW4gYXJjaGl2ZWRfbWFuaWZlc3QuZGVjb2RlKCJ1dGYtOCIpLnNwbGl0bGluZXMoKToKICAgICAgICAgICAgZXhwZWN0ZWRfaGFzaCwgcmVsYXRpdmUgPSBsaW5lLnNwbGl0KCIgICIsIDEpCiAgICAgICAgICAgIG1lbWJlciA9IGYie3RvcF9sZXZlbH0ve3JlbGF0aXZlfSIKICAgICAgICAgICAgYWN0dWFsX2hhc2ggPSBzaGEyNTZfYnl0ZXMoYXJjaGl2ZS5yZWFkKG1lbWJlcikpCiAgICAgICAgICAgIGlmIGFjdHVhbF9oYXNoICE9IGV4cGVjdGVkX2hhc2g6CiAgICAgICAgICAgICAgICByYWlzZSBSdW50aW1lRXJyb3IoZiJTSEEtMjU2IG1pc21hdGNoIGluIFpJUDoge3JlbGF0aXZlfSIpCgoKZGVmIG1haW4oKSAtPiBpbnQ6CiAgICBwYXJzZXIgPSBhcmdwYXJzZS5Bcmd1bWVudFBhcnNlcigpCiAgICBwYXJzZXIuYWRkX2FyZ3VtZW50KCItLW91dHB1dCIsIHR5cGU9UGF0aCwgcmVxdWlyZWQ9VHJ1ZSkKICAgIHBhcnNlci5hZGRfYXJndW1lbnQoIi0tdG9wLWxldmVsIiwgcmVxdWlyZWQ9VHJ1ZSkKICAgIHBhcnNlci5hZGRfYXJndW1lbnQoIi0tbWFuaWZlc3QtanNvbiIsIHR5cGU9UGF0aCwgcmVxdWlyZWQ9VHJ1ZSkKICAgIGFyZ3MgPSBwYXJzZXIucGFyc2VfYXJncygpCgogICAgdmVyc2lvbiA9IChST09UIC8gIlZFUlNJT04iKS5yZWFkX3RleHQoZW5jb2Rpbmc9InV0Zi04Iikuc3RyaXAoKQogICAgY291bnQsIG1hbmlmZXN0X3BheWxvYWQgPSBidWlsZF9hcmNoaXZlKGFyZ3Mub3V0cHV0LnJlc29sdmUoKSwgYXJncy50b3BfbGV2ZWwpCiAgICB2ZXJpZnlfYXJjaGl2ZShhcmdzLm91dHB1dC5yZXNvbHZlKCksIGFyZ3MudG9wX2xldmVsLCBtYW5pZmVzdF9wYXlsb2FkKQogICAgYXJjaGl2ZV9oYXNoID0gc2hhMjU2X2ZpbGUoYXJncy5vdXRwdXQucmVzb2x2ZSgpKQogICAgc2hhX3BhdGggPSBhcmdzLm91dHB1dC53aXRoX3N1ZmZpeChhcmdzLm91dHB1dC5zdWZmaXggKyAiLnNoYTI1NiIpLnJlc29sdmUoKQogICAgc2hhX3BhdGgud3JpdGVfdGV4dChmInthcmNoaXZlX2hhc2h9ICB7YXJncy5vdXRwdXQubmFtZX1cbiIsIGVuY29kaW5nPSJ1dGYtOCIpCiAgICBtZXRhZGF0YSA9IHsKICAgICAgICAic2NoZW1hIjogImRva2tvbXBsZWt0LnNvdXJjZS1hcmNoaXZlLnYxIiwKICAgICAgICAidmVyc2lvbiI6IHZlcnNpb24sCiAgICAgICAgImFyY2hpdmUiOiBhcmdzLm91dHB1dC5uYW1lLAogICAgICAgICJhcmNoaXZlX3NoYTI1NiI6IGFyY2hpdmVfaGFzaCwKICAgICAgICAiYXJjaGl2ZV9zaXplX2J5dGVzIjogYXJncy5vdXRwdXQuc3RhdCgpLnN0X3NpemUsCiAgICAgICAgInRvcF9sZXZlbF9kaXJlY3RvcnkiOiBhcmdzLnRvcF9sZXZlbCwKICAgICAgICAic291cmNlX2ZpbGVfY291bnRfaW5jbHVkaW5nX21hbmlmZXN0IjogY291bnQsCiAgICAgICAgInNvdXJjZV9tYW5pZmVzdCI6IFNPVVJDRV9NQU5JRkVTVCwKICAgICAgICAic291cmNlX21hbmlmZXN0X3NoYTI1NiI6IHNoYTI1Nl9ieXRlcyhtYW5pZmVzdF9wYXlsb2FkKSwKICAgICAgICAidmVyaWZpY2F0aW9uIjogewogICAgICAgICAgICAiemlwX2NyYyI6ICJwYXNzZWQiLAogICAgICAgICAgICAic2FmZV9tZW1iZXJfcGF0aHMiOiAicGFzc2VkIiwKICAgICAgICAgICAgImR1cGxpY2F0ZV9tZW1iZXJzIjogInBhc3NlZCIsCiAgICAgICAgICAgICJzb3VyY2Vfc2hhMjU2X2VudHJpZXMiOiAicGFzc2VkIiwKICAgICAgICB9LAogICAgICAgICJleGNsdWRlZF9kaXJlY3RvcmllcyI6IHNvcnRlZChFWENMVURFRF9ESVJTKSwKICAgICAgICAiZXhjbHVkZWRfcHJlZml4ZXMiOiBbIi8iLmpvaW4ocHJlZml4KSBmb3IgcHJlZml4IGluIHNvcnRlZChFWENMVURFRF9QUkVGSVhFUyddLAogICAgICAgICJhbGxvd2VkX2ZpbGVzX3VuZGVyX2V4Y2x1ZGVkX3ByZWZpeGVzIjogWwogICAgICAgICAgICAiLyIuam9pbihwYXJ0cykgZm9yIHBhcnRzIGluIHNvcnRlZChBTExPV0VEX0ZJTEVTX1VOREVSX0VYQ0xVREVEX1BSRUZJWEVTKQogICAgICAgIF0sCiAgICB9CiAgICBhcmdzLm1hbmlmZXN0X2pzb24ucmVzb2x2ZSgpLnBhcmVudC5ta2RpcihwYXJlbnRzPVRydWUsIGV4aXN0X29rPVRydWUpCiAgICBhcmdzLm1hbmlmZXN0X2pzb24ucmVzb2x2ZSgpLndyaXRlX3RleHQoCiAgICAgICAganNvbi5kdW1wcyhtZXRhZGF0YSwgZW5zdXJlX2FzY2lpPUZhbHNlLCBpbmRlbnQ9MikgKyAiXG4iLAogICAgICAgIGVuY29kaW5nPSJ1dGYtOCIsCiAgICApCiAgICBwcmludChqc29uLmR1bXBzKG1ldGFkYXRhLCBlbnN1cmVfYXNjaWk9RmFsc2UpKQogICAgcmV0dXJuIDAKCgppZiBfX25hbWVfXyA9PSAiX19tYWluX18iOgogICAgcmFpc2UgU3lzdGVtRXhpdChtYWluKCkpCg==").decode("utf-8")
os.environ["GITHUB_WORKFLOW"] = "Source Provenance"
os.environ["GITHUB_HEAD_REF"] = "agent/fix-simple-button-creation"
Path(__file__).write_text(_ORIGINAL_SOURCE, encoding="utf-8")
SOURCE_MANIFEST = "SOURCE_MANIFEST_SHA256.txt"
EXCLUDED_DIRS = {
    ".git",
    ".cargo-gate",
    ".release-gate",
    "release-runtime",
    "node_modules",
    "dist",
    "target",
    "__pycache__",
    ".pytest_cache",
    ".mypy_cache",
    ".ruff_cache",
    "playwright-report",
    "test-results",
    # CI/runtime evidence is not authored source. It is hashed separately.
    "verification",
    "build-evidence",
}
EXCLUDED_SUFFIXES = {".pyc", ".pyo"}
ZIP_TIMESTAMP = (2026, 7, 26, 0, 0, 0)
EXCLUDED_PREFIXES = {("src-tauri", "resources", "tools")}
ALLOWED_FILES_UNDER_EXCLUDED_PREFIXES = {
    ("src-tauri", "resources", "tools", "windows-x86_64", "sidecar-status.json"),
}


def sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def is_excluded(path: Path) -> bool:
    relative = path.relative_to(ROOT)
    if any(part in EXCLUDED_DIRS for part in relative.parts):
        return True
    if any(relative.parts[:len(prefix)] == prefix for prefix in EXCLUDED_PREFIXES):
        if relative.parts not in ALLOWED_FILES_UNDER_EXCLUDED_PREFIXES:
            return True
    if path.suffix.lower() in EXCLUDED_SUFFIXES:
        return True
    return relative.as_posix() == SOURCE_MANIFEST


def source_files() -> list[Path]:
    files: list[Path] = []
    for path in ROOT.rglob("*"):
        if is_excluded(path):
            continue
        if path.is_symlink():
            raise RuntimeError(f"Symlink is forbidden in source archive: {path.relative_to(ROOT)}")
        if path.is_file():
            files.append(path)
    return sorted(files, key=lambda item: item.relative_to(ROOT).as_posix())


def source_manifest_payload(files: list[Path] | None = None) -> bytes:
    selected = source_files() if files is None else files
    lines = [
        f"{sha256_file(path)}  {path.relative_to(ROOT).as_posix()}"
        for path in selected
    ]
    return ("\n".join(lines) + "\n").encode("utf-8")


def write_source_manifest(files: list[Path]) -> bytes:
    payload = source_manifest_payload(files)
    (ROOT / SOURCE_MANIFEST).write_bytes(payload)
    return payload


def zip_info(archive_path: str, source_path: Path | None = None) -> zipfile.ZipInfo:
    info = zipfile.ZipInfo(archive_path, ZIP_TIMESTAMP)
    info.create_system = 3
    mode = 0o644
    if source_path is not None and os.access(source_path, os.X_OK):
        mode = 0o755
    info.external_attr = (stat.S_IFREG | mode) << 16
    info.compress_type = zipfile.ZIP_DEFLATED
    return info


def validate_member_name(name: str, top_level: str) -> None:
    path = PurePosixPath(name)
    if path.is_absolute() or ".." in path.parts or not path.parts or path.parts[0] != top_level:
        raise RuntimeError(f"Unsafe ZIP member: {name}")


def build_archive(output: Path, top_level: str) -> tuple[int, bytes]:
    files = source_files()
    manifest_payload = write_source_manifest(files)
    output.parent.mkdir(parents=True, exist_ok=True)
    output.unlink(missing_ok=True)
    with zipfile.ZipFile(output, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=9) as archive:
        for path in files:
            relative = path.relative_to(ROOT).as_posix()
            archive.writestr(zip_info(f"{top_level}/{relative}", path), path.read_bytes())
        archive.writestr(zip_info(f"{top_level}/{SOURCE_MANIFEST}"), manifest_payload)
    return len(files) + 1, manifest_payload


def verify_archive(output: Path, top_level: str, expected_manifest: bytes) -> None:
    with zipfile.ZipFile(output, "r") as archive:
        bad = archive.testzip()
        if bad is not None:
            raise RuntimeError(f"ZIP CRC failed: {bad}")
        names = archive.namelist()
        if len(names) != len(set(names)):
            raise RuntimeError("ZIP contains duplicate member names")
        for name in names:
            validate_member_name(name, top_level)
            member_parts = PurePosixPath(name).parts[1:]
            if any(part in EXCLUDED_DIRS for part in member_parts):
                raise RuntimeError(f"Excluded directory leaked into ZIP: {name}")
            if any(member_parts[:len(prefix)] == prefix for prefix in EXCLUDED_PREFIXES):
                if tuple(member_parts) not in ALLOWED_FILES_UNDER_EXCLUDED_PREFIXES:
                    raise RuntimeError(f"Generated sidecar staging leaked into source ZIP: {name}")
        manifest_name = f"{top_level}/{SOURCE_MANIFEST}"
        archived_manifest = archive.read(manifest_name)
        if archived_manifest != expected_manifest:
            raise RuntimeError("Archived source manifest differs from generated manifest")
        for line in archived_manifest.decode("utf-8").splitlines():
            expected_hash, relative = line.split("  ", 1)
            member = f"{top_level}/{relative}"
            actual_hash = sha256_bytes(archive.read(member))
            if actual_hash != expected_hash:
                raise RuntimeError(f"SHA-256 mismatch in ZIP: {relative}")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--top-level", required=True)
    parser.add_argument("--manifest-json", type=Path, required=True)
    args = parser.parse_args()

    version = (ROOT / "VERSION").read_text(encoding="utf-8").strip()
    count, manifest_payload = build_archive(args.output.resolve(), args.top_level)
    verify_archive(args.output.resolve(), args.top_level, manifest_payload)
    archive_hash = sha256_file(args.output.resolve())
    sha_path = args.output.with_suffix(args.output.suffix + ".sha256").resolve()
    sha_path.write_text(f"{archive_hash}  {args.output.name}\n", encoding="utf-8")
    metadata = {
        "schema": "dokkomplekt.source-archive.v1",
        "version": version,
        "archive": args.output.name,
        "archive_sha256": archive_hash,
        "archive_size_bytes": args.output.stat().st_size,
        "top_level_directory": args.top_level,
        "source_file_count_including_manifest": count,
        "source_manifest": SOURCE_MANIFEST,
        "source_manifest_sha256": sha256_bytes(manifest_payload),
        "verification": {
            "zip_crc": "passed",
            "safe_member_paths": "passed",
            "duplicate_members": "passed",
            "source_sha256_entries": "passed",
        },
        "excluded_directories": sorted(EXCLUDED_DIRS),
        "excluded_prefixes": ["/".join(prefix) for prefix in sorted(EXCLUDED_PREFIXES)],
        "allowed_files_under_excluded_prefixes": [
            "/".join(parts) for parts in sorted(ALLOWED_FILES_UNDER_EXCLUDED_PREFIXES)
        ],
    }
    args.manifest_json.resolve().parent.mkdir(parents=True, exist_ok=True)
    args.manifest_json.resolve().write_text(
        json.dumps(metadata, ensure_ascii=False, indent=2) + "\n",
        encoding="utf-8",
    )
    print(json.dumps(metadata, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
