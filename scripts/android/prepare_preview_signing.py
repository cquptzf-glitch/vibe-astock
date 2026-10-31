"""Preserve the preview upgrade identity in CI cache; never store its private key in Git."""
import argparse
import base64
import binascii
import tempfile
import hashlib
import json
import os
from pathlib import Path
import subprocess
from configure_distribution import certificate, validate_package

def key_certificate(key: Path) -> str:
    env = {**os.environ, 'VIBE_DEBUG_STORE_PASS': 'android'}
    result = subprocess.run(['keytool', '-exportcert', '-keystore', str(key), '-alias', 'androiddebugkey', '-storepass:env', 'VIBE_DEBUG_STORE_PASS'], env=env, capture_output=True, check=False)
    if result.returncode or not result.stdout:
        raise SystemExit('Preview signing key cannot be read')
    return certificate(hashlib.sha256(result.stdout).hexdigest())


def prepare(expected_file: Path, key: Path, output: Path, secret_base64: str = '', package_name: str = 'com.vibeastock.mobile'):
    package_name = validate_package(package_name)
    expected = json.loads(expected_file.read_text(encoding='utf-8')) if expected_file.exists() else None
    if expected is not None:
        if not isinstance(expected, dict) or not isinstance(expected.get('certificate_sha256'), str):
            raise SystemExit('Pinned preview identity is malformed')
        try:
            certificate(expected['certificate_sha256'])
        except ValueError:
            raise SystemExit('Pinned preview certificate is malformed') from None
    if expected and expected.get('package', package_name) != package_name:
        raise SystemExit('Pinned preview package does not match the selected build profile')
    if secret_base64:
        if not expected:
            raise SystemExit('A restored key requires a pinned public preview identity')
        try:
            raw = base64.b64decode(secret_base64, validate=True)
        except (ValueError, binascii.Error):
            raise SystemExit('Preview keystore secret is not valid base64') from None
        if not 0 < len(raw) <= 1024 * 1024:
            raise SystemExit('Preview keystore secret has an invalid size')
        key.parent.mkdir(parents=True, exist_ok=True)
        candidate = None
        try:
            with tempfile.NamedTemporaryFile(dir=key.parent, prefix='.vibe-key-', delete=False) as f:
                candidate = Path(f.name)
                f.write(raw)
            candidate.chmod(0o600)
            if key_certificate(candidate) != certificate(expected['certificate_sha256']):
                raise SystemExit('Restored preview key differs from the pinned identity')
            os.replace(candidate, key)
        finally:
            if candidate is not None:
                candidate.unlink(missing_ok=True)
    if expected and not key.exists():
        raise SystemExit('The pinned preview key is missing. Restore VIBE_PREVIEW_KEYSTORE_BASE64; never replace an existing upgrade identity.')
    env = {**os.environ, 'VIBE_DEBUG_STORE_PASS': 'android', 'VIBE_DEBUG_KEY_PASS': 'android'}
    if not key.exists():
        key.parent.mkdir(parents=True, exist_ok=True)
        subprocess.run(['keytool', '-genkeypair', '-keystore', str(key), '-alias', 'androiddebugkey', '-storetype', 'JKS', '-keyalg', 'RSA', '-keysize', '2048', '-validity', '10000', '-dname', 'CN=Android Debug,O=Android,C=US', '-storepass:env', 'VIBE_DEBUG_STORE_PASS', '-keypass:env', 'VIBE_DEBUG_KEY_PASS', '-noprompt'], env=env, check=True, capture_output=True)
    key.chmod(0o600)
    actual = key_certificate(key)
    if expected and actual != certificate(expected['certificate_sha256']):
        raise SystemExit('Preview certificate differs from the pinned identity; refusing an incompatible upgrade')
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps({'certificate_sha256': actual, 'package': package_name, 'debug_signed': True, 'device_upgrade_verified': False}, indent=2) + '\n', encoding='utf-8')
    print('Preview signing certificate SHA256: ' + actual)


def configure_preview(project: Path) -> None:
    gradle = project / 'app/build.gradle.kts'
    source = gradle.read_text(encoding='utf-8')
    import re
    source = re.sub(r'// BEGIN VIBE PREVIEW SIGNING.*?// END VIBE PREVIEW SIGNING\n?', '', source, flags=re.S)
    source = source.rstrip() + '\n\n' + PREVIEW_SIGNING
    gradle.write_text(source, encoding='utf-8')

PREVIEW_SIGNING = '''// BEGIN VIBE PREVIEW SIGNING
android {
    signingConfigs.getByName("debug") {
        storeFile = file(System.getenv("VIBE_PREVIEW_KEYSTORE_PATH") ?: error("VIBE_PREVIEW_KEYSTORE_PATH required"))
        storePassword = "android"
        keyAlias = "androiddebugkey"
        keyPassword = "android"
    }
    buildTypes.getByName("debug") {
        signingConfig = signingConfigs.getByName("debug")
    }
}
// END VIBE PREVIEW SIGNING
'''

if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--parallel', action='store_true')
    args = parser.parse_args()
    package = 'com.vibeastock.preview' if args.parallel else 'com.vibeastock.mobile'
    pin = Path('docs/android/preview-signing-parallel.json' if args.parallel else 'docs/android/preview-signing.json')
    key = Path.home() / '.android' / ('vibe-preview.keystore' if args.parallel else 'debug.keystore')
    if not pin.is_file():
        raise SystemExit('The selected preview profile has no pinned signing identity')
    prepare(pin, key, Path('dist/preview-signing.json'), os.environ.get('VIBE_PREVIEW_KEYSTORE_BASE64', ''), package)
    configure_preview(Path('src-tauri/gen/android'))
    if not os.environ.get('GITHUB_ENV'):
        raise SystemExit('Set VIBE_PREVIEW_KEYSTORE_PATH to the guarded key before building')
    with open(os.environ['GITHUB_ENV'], 'a', encoding='utf-8') as env_file:
        env_file.write('VIBE_PREVIEW_KEYSTORE_PATH=' + str(key.resolve()) + '\n')
