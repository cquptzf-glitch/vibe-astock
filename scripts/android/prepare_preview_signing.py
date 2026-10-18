"""Preserve the preview upgrade identity in CI cache; never store its private key in Git."""
import hashlib
import json
import os
from pathlib import Path
import subprocess
from configure_distribution import certificate

def prepare(expected_file:Path,key:Path,output:Path):
    expected=json.loads(expected_file.read_text(encoding='utf-8')) if expected_file.exists() else None
    if expected and not key.exists():
        raise SystemExit('The existing preview signing key is missing from CI cache. Restore that key before publishing; do not silently replace the upgrade identity.')
    env={**os.environ,'VIBE_DEBUG_STORE_PASS':'android','VIBE_DEBUG_KEY_PASS':'android'}
    if not key.exists():
        key.parent.mkdir(parents=True,exist_ok=True)
        subprocess.run(['keytool','-genkeypair','-keystore',str(key),'-alias','androiddebugkey','-storetype','JKS','-keyalg','RSA','-keysize','2048','-validity','10000','-dname','CN=Android Debug,O=Android,C=US','-storepass:env','VIBE_DEBUG_STORE_PASS','-keypass:env','VIBE_DEBUG_KEY_PASS','-noprompt'],env=env,check=True,capture_output=True)
        key.chmod(0o600)
    r=subprocess.run(['keytool','-exportcert','-keystore',str(key),'-alias','androiddebugkey','-storepass:env','VIBE_DEBUG_STORE_PASS'],env=env,capture_output=True,check=False)
    if r.returncode or not r.stdout:raise SystemExit('Preview signing key cannot be read')
    actual=certificate(hashlib.sha256(r.stdout).hexdigest())
    if expected and actual!=certificate(expected['certificate_sha256']):
        raise SystemExit('Preview certificate differs from the pinned identity; refusing to publish an incompatible upgrade')
    output.parent.mkdir(parents=True,exist_ok=True)
    output.write_text(json.dumps({'certificate_sha256':actual,'package':'com.vibeastock.mobile','debug_signed':True,'device_upgrade_verified':False},indent=2)+'\n',encoding='utf-8')
    print('Preview signing certificate SHA256: '+actual)

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

if __name__=='__main__':
    key = Path.home()/'.android/debug.keystore'
    prepare(Path('docs/android/preview-signing.json'),key,Path('dist/preview-signing.json'))
    configure_preview(Path('src-tauri/gen/android'))
    # Explicitly bind Gradle to the key checked above. Its default user-home
    # resolution can otherwise select a different Android debug keystore.
    if not os.environ.get('GITHUB_ENV'):
        raise SystemExit('Set VIBE_PREVIEW_KEYSTORE_PATH to the guarded key before building')
    with open(os.environ['GITHUB_ENV'],'a',encoding='utf-8') as env_file:
        env_file.write('VIBE_PREVIEW_KEYSTORE_PATH='+str(key.resolve())+'\n')
