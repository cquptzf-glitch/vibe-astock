import assert from "node:assert/strict";
import test from "node:test";
import { checkPassphrase, decryptBackup, encryptBackup, ENCRYPTED_FORMAT, fromB64, isEncryptedBackup, toB64 } from "../src/mobile/backupCrypto.ts";

const plain = JSON.stringify({ format: "vibe_astock_mobile_backup", data: { watchlist: [{ symbol: "600519", name: "贵州茅台" }] } });

test("加密后外壳是明文 JSON、里面不含原文;同一口令能还原,每次加密的盐和 IV 都不同", async () => {
  const a = await encryptBackup(plain, "correct horse battery");
  const b = await encryptBackup(plain, "correct horse battery");
  const env = JSON.parse(a);
  assert.equal(env.format, ENCRYPTED_FORMAT);
  assert.equal(env.kdf.name, "PBKDF2");
  assert.ok(env.kdf.iterations >= 300_000);
  assert.ok(!a.includes("贵州茅台") && !a.includes("600519") && !a.includes("watchlist"));
  assert.notEqual(JSON.parse(b).kdf.salt, env.kdf.salt);
  assert.notEqual(JSON.parse(b).cipher.iv, env.cipher.iv);
  assert.equal(isEncryptedBackup(a), true);
  assert.equal(isEncryptedBackup(plain), false);
  assert.equal(isEncryptedBackup("不是 JSON"), false);
  assert.equal(await decryptBackup(a, "correct horse battery"), plain);
  assert.equal(await decryptBackup(b, "correct horse battery"), plain);
});

test("口令错、密文或外壳被改、参数被改成离谱的值,都拒绝且说法一致;口令太短不让加密", async () => {
  const sealed = await encryptBackup(plain, "correct horse battery");
  await assert.rejects(decryptBackup(sealed, "wrong passphrase!"), /口令不对,或者这份备份文件被改过/);
  const env = JSON.parse(sealed);
  const flipped = fromB64(env.data);
  flipped[flipped.length - 1] ^= 1;
  await assert.rejects(decryptBackup(JSON.stringify({ ...env, data: toB64(flipped) }), "correct horse battery"), /口令不对,或者这份备份文件被改过/);
  await assert.rejects(decryptBackup(JSON.stringify({ ...env, kdf: { ...env.kdf, salt: toB64(new Uint8Array(16)) } }), "correct horse battery"), /口令不对/);
  await assert.rejects(decryptBackup(JSON.stringify({ ...env, kdf: { ...env.kdf, iterations: 50_000_000 } }), "correct horse battery"), /参数不合理/);
  await assert.rejects(decryptBackup(JSON.stringify({ ...env, kdf: { ...env.kdf, iterations: 1 } }), "correct horse battery"), /参数不合理/);
  await assert.rejects(decryptBackup(JSON.stringify({ ...env, version: 2 }), "x"), /不认识的加密备份格式/);
  await assert.rejects(decryptBackup("坏的", "x"), /不是有效的备份文件/);
  await assert.rejects(encryptBackup(plain, "短口令"), /至少 8 个字符/);
  assert.equal(checkPassphrase("12345678"), null);
  assert.match(checkPassphrase("1234567") ?? "", /现在 7 个/);
  assert.equal(checkPassphrase("八个汉字的口令呀"), null, "按字符数算,不按字节数");
});
