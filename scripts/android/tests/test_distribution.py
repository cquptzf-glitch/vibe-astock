import json
import hashlib
import os
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch
from types import SimpleNamespace
import xml.etree.ElementTree as ET
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from configure_distribution import configure, validate_host, certificate, attr
from verify_distribution import association_matches, verify_apk_certificate, verify_preview, expected_version_code
from prepare_preview_signing import prepare, configure_preview

class DistributionTest(unittest.TestCase):
    def setUp(self):
        self.directory=tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.root=Path(self.directory.name)
        self.project=self.root/'android'
        (self.project/'app/src/main').mkdir(parents=True)
        self.gradle=self.project/'app/build.gradle.kts'
        self.gradle.write_text('plugins { id("com.android.application") }\n',encoding='utf-8')
        self.manifest=self.project/'app/src/main/AndroidManifest.xml'
        self.manifest.write_text('<manifest xmlns:android="http://schemas.android.com/apk/res/android"><application><activity android:name=".MainActivity"><intent-filter><action android:name="android.intent.action.MAIN" /></intent-filter></activity></application></manifest>',encoding='utf-8')
        self.out=self.root/'output'
        self.fingerprint='12'*32
    def test_links_are_idempotent_and_can_be_disabled(self):
        for _ in range(2):configure(self.project,self.out,'links.example.com',[self.fingerprint])
        app=ET.parse(self.manifest).getroot().find('application')
        self.assertEqual(len(app.findall('activity/intent-filter')),4)
        self.assertEqual(app.find('meta-data').get(attr('value')),'links.example.com')
        pack=json.loads((self.out/'.well-known/assetlinks.json').read_text())
        self.assertTrue(association_matches(pack,[self.fingerprint]))
        self.assertFalse(association_matches(pack,['34'*32]))
        configure(self.project,self.out)
        app=ET.parse(self.manifest).getroot().find('application')
        self.assertEqual(len(app.findall('activity/intent-filter')),1)
        self.assertIsNone(app.find('meta-data'))
        self.assertFalse((self.out/'.well-known/assetlinks.json').exists())
    def test_invalid_configuration_does_not_modify_sources(self):
        before=self.manifest.read_bytes()
        for host in ['http://example.com','example.com/path','127.0.0.1','a.local','evil.com:443','example.com?x=1','x.test']:
            with self.assertRaises(ValueError):configure(self.project,self.out,host,[self.fingerprint])
        with self.assertRaises(ValueError):configure(self.project,self.out,'links.example.com',[])
        self.assertEqual(self.manifest.read_bytes(),before)
    def test_signing_uses_environment_only_and_missing_credentials_refuse(self):
        with patch.dict(os.environ,{},clear=True):
            with self.assertRaises(ValueError):configure(self.project,self.out,signing=True)
        key=self.root/'test-only-keystore';key.write_bytes(b'synthetic test bytes, not a real key')
        env={'ANDROID_KEYSTORE_PATH':str(key),'ANDROID_STORE_PASSWORD':'fixture-store-pass','ANDROID_KEY_ALIAS':'fixture-alias','ANDROID_KEY_PASSWORD':'fixture-key-pass'}
        with patch.dict(os.environ,env,clear=True):
            for _ in range(2):configure(self.project,self.out,signing=True)
        source=self.gradle.read_text()
        self.assertEqual(source.count('create("vibeRelease")'),1)
        for value in env.values():self.assertNotIn(value,source)
        self.assertIn('isDebuggable = false',source)
        configure(self.project,self.out)
        self.assertNotIn('vibeRelease',self.gradle.read_text())
    def test_apksigner_old_and_build_tools_37_output(self):
        digest = '12' * 32
        for label in ['Signer #1', 'V3.0Signer:', 'V2Signer:', 'V1Signer:', 'V3.0 Signer:', 'V3.0Signer #1:']:
            verify_apk_certificate(label + ' certificate SHA-256 digest: ' + digest + '\n', digest)
        for output in [
            'Source Stamp Signer: certificate SHA-256 digest: ' + digest,
            'Signer #1 public key SHA-256 digest: ' + digest,
            'Signer #1 certificate SHA-256 digest: ' + '34' * 32,
            'Signer #1 certificate SHA-256 digest: ' + digest + '\nSigner #2 certificate SHA-256 digest: ' + '34' * 32,
            'V3.0Signer: certificate SHA-256 digest: ' + digest + 'BAD',
        ]:
            with self.assertRaises(ValueError): verify_apk_certificate(output, digest)

    def test_malformed_associations_are_not_verified(self):
        for body in [None,{},[{'relation':'delegate_permission/common.handle_all_urls','target':{}}],[{'relation':['delegate_permission/common.handle_all_urls'],'target':{'namespace':'android_app','package_name':'com.vibeastock.mobile','sha256_cert_fingerprints':['bad']}}]]:
            self.assertFalse(association_matches(body,[self.fingerprint]))
        self.assertEqual(certificate(self.fingerprint),':'.join(['12']*32))
        self.assertEqual(validate_host('Links.Example.Com'),'links.example.com')
    def test_android_version_code_matches_tauri_contract(self):
        self.assertEqual(expected_version_code('0.1.38'),1038)
        self.assertEqual(expected_version_code('1.2.3'),1002003)
        for invalid in ['', '1.0', '0.0.0', '1.2.3-debug', '9999.0.0']:
            with self.assertRaises(ValueError): expected_version_code(invalid)

    def test_preview_gradle_binds_the_guarded_key_explicitly(self):
        for _ in range(2): configure_preview(self.project)
        source = self.gradle.read_text()
        self.assertEqual(source.count('// BEGIN VIBE PREVIEW SIGNING'), 1)
        self.assertIn('storeFile = file(System.getenv("VIBE_PREVIEW_KEYSTORE_PATH")', source)
        self.assertIn('signingConfig = signingConfigs.getByName("debug")', source)
        configure(self.project, self.out)
        self.assertIn('VIBE_PREVIEW_KEYSTORE_PATH', self.gradle.read_text())

    def test_preview_checks_built_apk_certificate_version_and_package(self):
        apk = self.root/'fixture.apk'; apk.write_bytes(b'synthetic APK fixture')
        signer = self.root/'apksigner'
        expected = {'package':'com.vibeastock.mobile','certificate_sha256':self.fingerprint,'debug_signed':True,'device_upgrade_verified':False}
        badging = "package: name='com.vibeastock.mobile' versionCode='1038' versionName='0.1.38'\napplication-debuggable\nnative-code: 'arm64-v8a'\n"
        def result(stdout, code=0): return SimpleNamespace(returncode=code, stdout=stdout)
        with patch('verify_distribution.subprocess.run',side_effect=[result(badging),result('V2 Signer: certificate SHA-256 digest: '+self.fingerprint)]) as run:
            receipt = verify_preview(apk,signer,expected,'0.1.38')
            self.assertTrue(receipt['signature_verified'])
            self.assertFalse(receipt['device_upgrade_verified'])
            self.assertEqual(receipt['apk']['sha256'],hashlib.sha256(apk.read_bytes()).hexdigest())
            self.assertEqual(run.call_count,2)
        for text in [badging.replace('com.vibeastock.mobile','other.package'),badging.replace('0.1.38','0.1.37'),badging.replace("versionCode='1038'","versionCode='1'"),badging.replace('arm64-v8a','x86'),badging.replace('application-debuggable','')]:
            with patch('verify_distribution.subprocess.run',return_value=result(text)):
                with self.assertRaises(ValueError): verify_preview(apk,signer,expected,'0.1.38')
        for signed in [result('V2 Signer: certificate SHA-256 digest: '+'34'*32), result('',1)]:
            with patch('verify_distribution.subprocess.run',side_effect=[result(badging),signed]):
                with self.assertRaises(ValueError): verify_preview(apk,signer,expected,'0.1.38')

    def test_pinned_preview_identity_refuses_missing_or_changed_keys(self):
        expected=self.root/'preview.json';key=self.root/'preview-test-key';output=self.root/'receipt.json'
        expected.write_text(json.dumps({'certificate_sha256':'12'*32}),encoding='utf-8')
        with patch('prepare_preview_signing.subprocess.run') as run:
            with self.assertRaises(SystemExit):prepare(expected,key,output)
            run.assert_not_called()
        key.write_bytes(b'synthetic fixture, not a key')
        with patch('prepare_preview_signing.subprocess.run') as run:
            run.return_value.returncode=0;run.return_value.stdout=b'synthetic certificate fixture'
            with self.assertRaises(SystemExit):prepare(expected,key,output)
            self.assertFalse(output.exists())
            expected.write_text(json.dumps({'certificate_sha256':hashlib.sha256(run.return_value.stdout).hexdigest()}),encoding='utf-8')
            prepare(expected,key,output)
            self.assertTrue(json.loads(output.read_text())['debug_signed'])

if __name__=='__main__':unittest.main()
