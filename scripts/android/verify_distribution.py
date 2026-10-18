"""Verify release certificate, bundle signature and optional owner website association."""
from __future__ import annotations
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import urllib.request
import zipfile
from configure_distribution import certificate, validate_host

def signing_certificate() -> str:
    args = ["keytool", "-exportcert", "-keystore", os.environ["ANDROID_KEYSTORE_PATH"], "-alias", os.environ["ANDROID_KEY_ALIAS"], "-storepass:env", "ANDROID_STORE_PASSWORD"]
    result = subprocess.run(args, capture_output=True, check=False)
    if result.returncode or not result.stdout:
        raise ValueError("Could not read configured signing certificate; check alias and store password")
    return certificate(hashlib.sha256(result.stdout).hexdigest())

def verify_apk_certificate(output: str, expected: str) -> None:
    # Build Tools 37 adds the verified scheme and a colon to signer labels.
    # Match application signers only; a source stamp cannot establish identity.
    digests = re.findall(
        r"^(?:V(?:1|2|3\.[012])\s*)?Signer(?: #\d+)?:? certificate SHA-256 digest:\s*([a-fA-F0-9]{64})\s*$",
        output, re.M,
    )
    if not digests:
        raise ValueError("Could not parse APK application signing certificate")
    actual = {certificate(digest) for digest in digests}
    if actual != {certificate(expected)}:
        raise ValueError("APK signing certificate differs from configured upgrade key: " + ", ".join(sorted(actual)))

def verify(apk: Path, aab: Path, apksigner: Path, expected: str) -> dict:
    expected = certificate(expected)
    badging = subprocess.run([str(apksigner.with_name("aapt")), "dump", "badging", str(apk)], capture_output=True, text=True)
    package = re.search(r"package: name='([^']+)' versionCode='([0-9]+)' versionName='([^']+)'", badging.stdout)
    if badging.returncode or not package or package[1] != "com.vibeastock.mobile" or "application-debuggable" in badging.stdout or "arm64-v8a" not in badging.stdout:
        raise ValueError("Release APK package, ABI or debuggability is incorrect")
    r = subprocess.run([str(apksigner), "verify", "--verbose", "--print-certs", str(apk)], capture_output=True, text=True)
    if r.returncode:
        raise ValueError("APK signature verification failed")
    verify_apk_certificate(r.stdout, expected)
    with zipfile.ZipFile(aab) as archive:
        if not any(name.startswith("base/lib/arm64-v8a/") and name.endswith(".so") for name in archive.namelist()):
            raise ValueError("AAB has no ARM64 native library")
        if not any(re.fullmatch(r"META-INF/[^/]+\.(RSA|DSA|EC)", name, re.I) for name in archive.namelist()):
            raise ValueError("AAB contains no signing block")
    r = subprocess.run(["jarsigner", "-verify", "-strict", str(aab)], capture_output=True, text=True)
    # A self-signed Android application certificate isn't a public CA certificate.
    # jarsigner returns 4 for that valid signature, so inspect the signature with keytool too.
    if r.returncode not in (0, 4) or "jar verified" not in (r.stdout + r.stderr).lower():
        raise ValueError("AAB JAR signature verification failed")
    exported = subprocess.run(["keytool", "-printcert", "-jarfile", str(aab), "-rfc"], capture_output=True, check=False)
    pem = re.search(rb"-----BEGIN CERTIFICATE-----\s*(.*?)\s*-----END CERTIFICATE-----", exported.stdout, re.S)
    if not pem:
        raise ValueError("AAB certificate missing")
    import base64
    actual = certificate(hashlib.sha256(base64.b64decode(re.sub(rb"\s", b"", pem[1]), validate=True)).hexdigest())
    if actual != expected:
        raise ValueError("AAB signing certificate differs from APK")
    return {"package":package[1],"version_code":int(package[2]),"version_name":package[3],"debuggable":False,"certificate_sha256": expected, "apk": {"name": apk.name, "bytes": apk.stat().st_size, "sha256": hashlib.sha256(apk.read_bytes()).hexdigest()}, "aab": {"name": aab.name, "bytes": aab.stat().st_size, "sha256": hashlib.sha256(aab.read_bytes()).hexdigest()}, "signature_verified": True, "device_upgrade_verified": False}

def verify_preview(apk: Path, apksigner: Path, expected: dict, version: str) -> dict:
    if expected.get("package") != "com.vibeastock.mobile" or not version:
        raise ValueError("Preview identity or version missing")
    digest = certificate(expected["certificate_sha256"])
    badging = subprocess.run([str(apksigner.with_name("aapt")), "dump", "badging", str(apk)], capture_output=True, text=True)
    package = re.search(r"package: name='([^']+)' versionCode='([0-9]+)' versionName='([^']+)'", badging.stdout)
    if badging.returncode or not package or package[1] != expected["package"] or package[3] != version or int(package[2]) <= 0 or "application-debuggable" not in badging.stdout or "arm64-v8a" not in badging.stdout:
        raise ValueError("Preview APK package, version, ABI or debuggability is incorrect")
    result = subprocess.run([str(apksigner), "verify", "--verbose", "--print-certs", str(apk)], capture_output=True, text=True)
    if result.returncode:
        raise ValueError("Preview APK signature verification failed")
    verify_apk_certificate(result.stdout, digest)
    return {**expected, "version_name": version, "version_code": int(package[2]), "signature_verified": True, "device_upgrade_verified": False, "apk": {"name": apk.name, "bytes": apk.stat().st_size, "sha256": hashlib.sha256(apk.read_bytes()).hexdigest()}}

def association_matches(body: object, fingerprints: list[str]) -> bool:
    wanted = {certificate(f) for f in fingerprints}
    if not isinstance(body, list) or not wanted:
        return False
    for entry in body:
        if not isinstance(entry, dict) or not isinstance(entry.get("relation"), list) or "delegate_permission/common.handle_all_urls" not in entry["relation"]:
            continue
        target = entry.get("target")
        if not isinstance(target, dict) or target.get("namespace") != "android_app" or target.get("package_name") != "com.vibeastock.mobile" or not isinstance(target.get("sha256_cert_fingerprints"), list):
            continue
        try:
            if wanted.issubset({certificate(f) for f in target["sha256_cert_fingerprints"] if isinstance(f, str)}):
                return True
        except ValueError:
            continue
    return False

def verify_website(host: str, fingerprints: list[str]) -> None:
    class NoRedirect(urllib.request.HTTPRedirectHandler):
        def redirect_request(self, *args, **kwargs):
            return None
    url = "https://" + validate_host(host) + "/.well-known/assetlinks.json"
    with urllib.request.build_opener(NoRedirect()).open(url, timeout=12) as response:
        if response.status != 200 or response.headers.get_content_type() != "application/json":
            raise ValueError("assetlinks.json needs HTTP 200 and application/json without redirects")
        payload = response.read(256 * 1024 + 1)
        if len(payload) > 256 * 1024 or not association_matches(json.loads(payload), fingerprints):
            raise ValueError("Website association does not match package and installed certificate")

if __name__ == "__main__":
    p = argparse.ArgumentParser()
    p.add_argument("--certificate", action="store_true")
    p.add_argument("--preview-receipt", type=Path)
    p.add_argument("--version", default="")
    p.add_argument("--output", type=Path)
    p.add_argument("--apk", type=Path)
    p.add_argument("--aab", type=Path)
    p.add_argument("--apksigner", type=Path)
    p.add_argument("--expected", default="")
    p.add_argument("--check-host", default="")
    a = p.parse_args()
    if a.certificate:
        print(signing_certificate())
    elif a.preview_receipt:
        if not all((a.apk, a.apksigner, a.version, a.output)):
            p.error("Preview verification requires --apk, --apksigner, --version and --output")
        result = verify_preview(a.apk, a.apksigner, json.loads(a.preview_receipt.read_text(encoding="utf-8")), a.version)
        a.output.write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
        print("Preview APK matched the guarded upgrade certificate: " + result["certificate_sha256"])
    elif a.check_host:
        verify_website(a.check_host, [a.expected])
        print("Website association matched; device verification is still required")
    else:
        if not all((a.apk, a.aab, a.apksigner, a.expected)):
            p.error("--apk, --aab, --apksigner and --expected are required")
        print(json.dumps(verify(a.apk, a.aab, a.apksigner, a.expected), indent=2))
