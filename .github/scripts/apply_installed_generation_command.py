from __future__ import annotations

import base64
import gzip
import hashlib
import os
from pathlib import Path
import subprocess
import sys

EXPECTED_PARENT = "eaf00795e1f56a0c79b87eae40e2fa714f7ad4ad"
EXPECTED_TRIGGER = ".github/ci-sync/installed-generation.trigger"
BRANCH_WORKFLOW = ".github/workflows/apply-installed-generation-command.yml"
PATCH_SHA256 = "27eaac06015a74eb0c053a9e6be4550dea4f827787ef0d01821d626dc42274ef"
GZIP_SHA256 = "ae304ff06b162635044db2e262070d4f051b6d7f7760347a566b855eb04c3a04"
MAIN_BLOB = "1b7472e35b2afa6da6f75da82079a963472e73e2"
SUBSYSTEM_BLOB = "b4058ecb48298dcf699de8b6423f1780ecadeca2"
TEST_BLOB = "de07875786c7979b9a8596d4288c74a757962546"
MANIFEST_BLOB = "78b510a2e195cb2943fc22817bf49658eca6ac01"
PATCH_GZIP_B64 = """
H4sIAAAAAAAAA61Yy27bRhTd6yumKhBQsEhLsiLbTJzmUSHNIqlru90EwWBEXskTkTPMzNCWmxjol7Qf0E1RoJsW6CJf4P5R7/Ah
UU87tumF6OG9574fZMiHQ+K6I24I29YqcA1LFbd32zHjwlOaDFaf17gIYUK6sNvuPOx6Xnuw293tAGm3Wr1ut+a67jrE2tbW1lrU
p0+J22n19ps9spX97hE84iKI0hC+cuo6HegLbSDW24mSAWhNB1EKieLCaASoNx7VSG01A0uNjJnhUlCVCsNjmDJsrWQ4ZSo8Zwoo
dErSrRoZCmK1dRrkY43YKwL0nhppckC0CX0fxJnv2wOn4QUyiiAwvv/4Jwge0ydPHCuv5BqwYDxSMhUhPWcmOEUEy+dxAwqZmbhw
PuHBJ3tKDg5I3XVnLG7GYpXKnbbXyp2212m2W9ZrKxVFUyic8RBEADRhppSZU9irkF45GGKsacySmTKeNoonNFEw5BMH1UJYt4Q9
qDc8S32I6M/Toe8PlYwb1nlVJUYgQDEDlCF4Fj+q4EMK2ix4YUH6xxzGXusUKaFdhHYzaFRpxpbZtKxgTnCZJ1CpKSYKqqXTyGqV
pavvP095FILy/RCGDJ/M+Spmgo3AeZYkxwZVqBA18ji1O90sTu2dVrPdWRmnxety+Xjh6HLeOj7MlD+WMTiFU7NYN9CIzb73mKZo
GwwzbeZA7WXdwYU2DLM6LGFsQeUgiO08OEXQCJpkTvAyVBkFCko5Wd1w6ft9pSR6VppTUI1vHi2z5egeTLhxWo0VBApMqgT5fuw4
jcXnC16zPtI2SjbjksTTecQel8GbFWt5xVmVsuBDyrEtZJ6wdRTJYDwzfLGqG8vhu87wPFV29trY/7Y6O/s7RUnHMiQGnaqrqRIw
IQUPWETfawzF4AIpmiRIlQKMyQUwRVMTNAnXdCjVgIdYpTRJBxEPaCjPRSRZSE+lNs0Z5kZanjRJdiMVDRTYREIPoPyQGUbHcNEk
QqqYRfxnzJKEB2NQVKYmSasiEqY0UA3xGagmScJhkUQajOFihCYkERM0YgOIbDa9xz6KyZQZZRRDLs2iZs0t8TQbAk2T0Koz5BFQ
wWKMhuYjYbWwWEaiDBmmFgj/b5Iz1DFjyESzQZSzVlCnFAVyqlCZc4W9CZVItS2cRCrTxGJLVQCHSp6BsEnRJCf2+VH2+IUUBiZo
/dYdtJ0xT5XaUItF+W2wcRnuOhtfleJeTqUdWtB+B44KcTPUGzmE9Dt9+rL/pn/07KRPD49evTmhR/0ffuwfn9DjF9/1Xz8rMuay
qMRUAxaYhl4Xa0aMuADCNKG2aWc102vl7bXX3mv2umtrpizsSzv87Q024cwNN3Aptkb3yQ18UW2gX0adJUhwCjHzr3eQhzDSaTTn
2XGBwaQx+Yz3SX3bxMl2KMdjiecwNtslgRfKYFJfDZLX7FqIcoyEmzDmlo2NKNMFwrN9bA0cBgm95eebVrFdoTM1juUTBfCaJfk4
d94ujwanLnFjsIU2BSf1q1/tH/n8+9Vv//1y9e/Vn1d/4e/fV398/qekWtRhhsWFqEDt7rbanZ3uw97uZk70VhpjG/NstVX4Wz2v
ted1Wp3eOv53iwdZYmJ3tUahc7+d+ZWc2KQ6zJ+vcWaYIumEYoVYZs0t57o4Ysu98MkbKaqdo9g78Ce/+fqtrbR305K6vpgoThq7
w2GTs8vIkPGIBpHUEM5vIHZWZwWKs/rmhfroVj3TeZARN3DLSDC9nHouuHwdKDeb/H1gTsM4Ndg4pRjRvHxvq2wVw5tC1Stl400R
3Rlivuy65S531ipjWYFmWoMyXzlf5I+qPg0PQ2Z3l8Zq+xVgW+FnODuKDnNbJywBeXNNzTpkZRO7q7FLcq+zOBXZNM+75W2tnQPx
Kp3X2rmh9cIE7svwORWuM3qA+1/ZsG9rcxXDW3wrXTZ6Smom5r5srqpwfZzHAndgXJ8gunUfmgPx8nFWfjnYOM+c+X5cZ8Ywu1l7
BeKavl3PRKx42Hh3DwlTseXahMEddkyLgXXrjKmCeNXpZzPGmntfeVEVdJNayGfpXSohR/AqQ9naVH61CtyYjXhwn3mfS7qJcXb4
38U0y+8VINlXif39e7HAQs7rb+HsOpLhlutI9g+uIzlCIcVCsiCAxGhabmQ6W0Lyd01Ni36I7ykgtC3K+U802fchKc3cdz87M2jI
7Yer95ILZ2hfhNG02v8yQ29VaxUAAA==
"""


def run(*args: str, capture: bool = False) -> str:
    result = subprocess.run(args, check=True, text=True, capture_output=capture)
    return result.stdout.strip() if capture else ""


def require(condition: bool, message: str) -> None:
    if not condition:
        raise SystemExit(message)


def blob_bytes(sha: str) -> bytes:
    return subprocess.run(["git", "cat-file", "blob", sha], check=True, capture_output=True).stdout


def main() -> None:
    require(run("git", "rev-parse", "HEAD^", capture=True) == EXPECTED_PARENT, "unexpected applicator parent")
    changed = run("git", "diff", "--name-only", "HEAD^", "HEAD", capture=True).splitlines()
    require(changed == [EXPECTED_TRIGGER], f"unexpected trigger commit paths: {changed}")
    require(Path(EXPECTED_TRIGGER).is_file(), "trusted trigger is missing")

    gzip_bytes = base64.b64decode("".join(PATCH_GZIP_B64.split()), validate=True)
    require(hashlib.sha256(gzip_bytes).hexdigest() == GZIP_SHA256, "embedded gzip SHA-256 mismatch")
    patch_bytes = gzip.decompress(gzip_bytes)
    require(hashlib.sha256(patch_bytes).hexdigest() == PATCH_SHA256, "embedded patch SHA-256 mismatch")
    patch_path = Path(os.environ.get("RUNNER_TEMP", "/tmp")) / "installed-generation-main.patch"
    patch_path.write_bytes(patch_bytes)
    run("git", "apply", "--check", "--whitespace=error-all", str(patch_path))
    run("git", "apply", "--whitespace=error-all", str(patch_path))

    subsystem = Path("src-tauri/src/subsystems/hardware_e2e.rs")
    subsystem.parent.mkdir(parents=True, exist_ok=True)
    subsystem.write_bytes(blob_bytes(SUBSYSTEM_BLOB))
    test_path = Path("tests/test_v18_4_6_installed_app_generation_hardware.py")
    test_path.write_bytes(blob_bytes(TEST_BLOB))
    Path(EXPECTED_TRIGGER).unlink()
    require(Path(BRANCH_WORKFLOW).is_file(), "temporary branch workflow is missing")
    Path(BRANCH_WORKFLOW).unlink()

    candidate = Path(os.environ.get("RUNNER_TEMP", "/tmp")) / "SOURCE_MANIFEST_SHA256.txt"
    subprocess.run([sys.executable, "scripts/verify_source_manifest.py", "--candidate", str(candidate)], check=False)
    require(candidate.is_file() and candidate.stat().st_size > 0, "source manifest candidate was not created")
    Path("SOURCE_MANIFEST_SHA256.txt").write_bytes(candidate.read_bytes())

    actual = {
        "src-tauri/src/main.rs": run("git", "hash-object", "src-tauri/src/main.rs", capture=True),
        str(subsystem): run("git", "hash-object", str(subsystem), capture=True),
        str(test_path): run("git", "hash-object", str(test_path), capture=True),
        "SOURCE_MANIFEST_SHA256.txt": run("git", "hash-object", "SOURCE_MANIFEST_SHA256.txt", capture=True),
    }
    expected = {
        "src-tauri/src/main.rs": MAIN_BLOB,
        str(subsystem): SUBSYSTEM_BLOB,
        str(test_path): TEST_BLOB,
        "SOURCE_MANIFEST_SHA256.txt": MANIFEST_BLOB,
    }
    require(actual == expected, f"verified product blob mismatch: {actual}")

    run(sys.executable, "-m", "pytest", "-q", "tests")
    run(sys.executable, "scripts/static_quality_gate.py", "--source-only")
    run(sys.executable, "scripts/verify_source_manifest.py")
    run("git", "diff", "--check")


if __name__ == "__main__":
    main()
