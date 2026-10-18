"""Configure generated Android sources. Credentials remain in environment variables."""
from __future__ import annotations
import argparse
import ipaddress
import json
import os
from pathlib import Path
import re
import xml.etree.ElementTree as ET

ANDROID = "http://schemas.android.com/apk/res/android"
ET.register_namespace("android", ANDROID)
def attr(name: str) -> str:
    return "{" + ANDROID + "}" + name

def validate_host(host: str) -> str:
    host = host.strip().lower()
    if len(host) > 253 or not re.fullmatch(r"(?:[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?\.)+[a-z][a-z0-9-]{1,62}", host):
        raise ValueError("App Links host must be a public DNS name without scheme, path or port")
    try:
        ipaddress.ip_address(host)
    except ValueError:
        pass
    else:
        raise ValueError("App Links host must not be an IP address")
    if host.endswith((".localhost", ".local", ".internal", ".test", ".invalid")):
        raise ValueError("App Links requires the owner's public HTTPS domain")
    return host

def certificate(value: str) -> str:
    value = value.replace(":", "").strip().upper()
    if not re.fullmatch(r"[A-F0-9]{64}", value):
        raise ValueError("A signing certificate needs exactly 32 SHA256 bytes")
    return ":".join(value[i:i+2] for i in range(0,64,2))

SIGNING = '''// BEGIN VIBE DISTRIBUTION SIGNING
android {
    signingConfigs {
        create("vibeRelease") {
            storeFile = file(System.getenv("ANDROID_KEYSTORE_PATH") ?: error("ANDROID_KEYSTORE_PATH required"))
            storePassword = System.getenv("ANDROID_STORE_PASSWORD") ?: error("ANDROID_STORE_PASSWORD required")
            keyAlias = System.getenv("ANDROID_KEY_ALIAS") ?: error("ANDROID_KEY_ALIAS required")
            keyPassword = System.getenv("ANDROID_KEY_PASSWORD") ?: error("ANDROID_KEY_PASSWORD required")
        }
    }
    buildTypes.getByName("release") {
        signingConfig = signingConfigs.getByName("vibeRelease")
        isDebuggable = false
        isJniDebuggable = false
    }
}
// END VIBE DISTRIBUTION SIGNING
'''

def configure(project: Path, output: Path, host: str = "", fingerprints: list[str] | None = None, signing: bool = False) -> dict:
    fingerprints = [certificate(f) for f in (fingerprints or [])]
    if host:
        host = validate_host(host)
        if not fingerprints:
            raise ValueError("Enabled App Links requires the installed app's certificate fingerprint")
    elif fingerprints:
        raise ValueError("Certificate fingerprints without an App Links host are ambiguous")
    gradle = project / "app/build.gradle.kts"
    manifest = project / "app/src/main/AndroidManifest.xml"
    if not gradle.is_file() or not manifest.is_file():
        raise ValueError("Run cargo tauri android init --ci before distribution configuration")
    source = gradle.read_text(encoding="utf-8")
    source = re.sub(r"// BEGIN VIBE DISTRIBUTION SIGNING.*?// END VIBE DISTRIBUTION SIGNING\n?", "", source, flags=re.S)
    if signing:
        for key in ("ANDROID_KEYSTORE_PATH", "ANDROID_STORE_PASSWORD", "ANDROID_KEY_ALIAS", "ANDROID_KEY_PASSWORD"):
            if not os.environ.get(key):
                raise ValueError(f"Missing signing configuration: {key}")
        if not Path(os.environ["ANDROID_KEYSTORE_PATH"]).is_file():
            raise ValueError("Signing keystore file is absent")
        source = source.rstrip() + "\n\n" + SIGNING
    tree = ET.parse(manifest)
    app = tree.getroot().find("application")
    if app is None:
        raise ValueError("Generated manifest has no application")
    activity = next((a for a in app.findall("activity") if a.get(attr("name")) in (".MainActivity", "com.vibeastock.mobile.MainActivity")), None)
    if activity is None:
        raise ValueError("Generated manifest has no expected MainActivity")
    metadata_key = "com.vibeastock.APP_LINK_HOST"
    for meta in list(app.findall("meta-data")):
        if meta.get(attr("name")) == metadata_key:
            app.remove(meta)
    for f in list(activity.findall("intent-filter")):
        if f.get(attr("label")) == "vibe-owned-app-links":
            activity.remove(f)
    if host:
        ET.SubElement(app, "meta-data", {attr("name"): metadata_key, attr("value"): host})
        for path, prefix in (("/stock/", True), ("/calendar", False), ("/research", False)):
            f = ET.SubElement(activity, "intent-filter", {attr("autoVerify"): "true", attr("label"): "vibe-owned-app-links"})
            ET.SubElement(f, "action", {attr("name"): "android.intent.action.VIEW"})
            for category in ("DEFAULT", "BROWSABLE"):
                ET.SubElement(f, "category", {attr("name"): "android.intent.category." + category})
            ET.SubElement(f, "data", {attr("scheme"): "https", attr("host"): host, attr("pathPrefix" if prefix else "path"): path})
    # Validate everything before modifying either generated file.
    gradle.write_text(source, encoding="utf-8")
    ET.indent(tree, space="    ")
    tree.write(manifest, encoding="utf-8", xml_declaration=True)
    receipt = {"package": "com.vibeastock.mobile", "app_links_host": host or None, "signing_configured": signing, "website_verified": False}
    output.mkdir(parents=True, exist_ok=True)
    if host:
        association = [{"relation": ["delegate_permission/common.handle_all_urls"], "target": {"namespace": "android_app", "package_name": "com.vibeastock.mobile", "sha256_cert_fingerprints": fingerprints}}]
        website = output / ".well-known"
        website.mkdir(exist_ok=True)
        (website / "assetlinks.json").write_text(json.dumps(association, indent=2) + "\n", encoding="utf-8")
    else:
        (output / ".well-known/assetlinks.json").unlink(missing_ok=True)
    (output / "configuration.json").write_text(json.dumps(receipt, indent=2) + "\n", encoding="utf-8")
    return receipt

if __name__ == "__main__":
    p = argparse.ArgumentParser()
    p.add_argument("--project", type=Path, default=Path("src-tauri/gen/android"))
    p.add_argument("--output", type=Path, default=Path("dist/android-distribution"))
    p.add_argument("--host", default="")
    p.add_argument("--fingerprint", action="append", default=[])
    p.add_argument("--signing", action="store_true")
    a = p.parse_args()
    print(json.dumps(configure(a.project, a.output, a.host, a.fingerprint, a.signing)))
