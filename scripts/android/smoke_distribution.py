"""Build and verify release artifacts with a disposable test key; never publish them."""
from __future__ import annotations
import json
import os
from pathlib import Path
import subprocess
import tempfile
from configure_distribution import configure
from verify_distribution import signing_certificate, verify

def main() -> None:
    project=Path('src-tauri/gen/android')
    output=Path('dist/android-distribution-smoke')
    with tempfile.TemporaryDirectory(prefix='vibe-signing-smoke-') as temporary:
        env={**os.environ,'ANDROID_KEYSTORE_PATH':str(Path(temporary)/'test-only.jks'),'ANDROID_STORE_PASSWORD':'disposable-fixture-store','ANDROID_KEY_ALIAS':'test-only','ANDROID_KEY_PASSWORD':'disposable-fixture-key'}
        subprocess.run(['keytool','-genkeypair','-keystore',env['ANDROID_KEYSTORE_PATH'],'-alias','test-only','-storetype','JKS','-keyalg','RSA','-keysize','2048','-validity','10000','-dname','CN=Disposable CI Fixture','-storepass:env','ANDROID_STORE_PASSWORD','-keypass:env','ANDROID_KEY_PASSWORD','-noprompt'],env=env,check=True,capture_output=True)
        prior={k:os.environ.get(k) for k in env if k.startswith('ANDROID_')}
        os.environ.update({k:v for k,v in env.items() if k.startswith('ANDROID_')})
        try:
            cert=signing_certificate()
            configure(project,output,'links.example.com',[cert],True)
            subprocess.run(['cargo','tauri','android','build','--ci','--apk','--aab','--split-per-abi','--target','aarch64'],env=env,check=True)
            apk=next((project/'app/build/outputs/apk').rglob('*release*.apk'))
            aab=next((project/'app/build/outputs/bundle').rglob('*release*.aab'))
            tools=Path(os.environ['ANDROID_HOME'])/'build-tools'
            signers=sorted(tools.glob('*/apksigner'),key=lambda p:tuple(int(x) for x in p.parent.name.split('.') if x.isdigit()))
            if not signers:raise RuntimeError('apksigner missing')
            receipt=verify(apk,aab,signers[-1],cert,json.loads(Path("src-tauri/tauri.conf.json").read_text(encoding="utf-8"))["version"])
            receipt.update({'identity':'disposable CI fixture, never a production upgrade key','production_signed':False,'website_verified':False})
            (output/'verification.json').write_text(json.dumps(receipt,indent=2)+'\n',encoding='utf-8')
            print('Disposable release APK/AAB signature smoke passed; production identity and device checks remain separate')
        finally:
            configure(project,output)
            for k,v in prior.items():
                if v is None:os.environ.pop(k,None)
                else:os.environ[k]=v

if __name__=='__main__':main()
