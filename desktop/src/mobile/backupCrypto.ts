/**
 * 可选的备份加密:用口令 + PBKDF2-SHA256(31 万次)派生 256 位密钥,AES-GCM 加密整份备份 JSON。
 * 口令只在这次导出 / 导入时用,不保存、不上传;忘了口令就解不开,没有找回办法。
 * 加密备份的外壳是明文 JSON({format, version, kdf, cipher, data}),里面的数据是密文;解密失败(口令错或文件被改)一律报同一句话。
 */
export const ENCRYPTED_FORMAT = "vibe_astock_mobile_backup_encrypted";
export const MIN_PASSPHRASE_CHARS = 8;
const ITERATIONS = 310_000;
const SALT_BYTES = 16;
const IV_BYTES = 12;
const WRONG = "口令不对,或者这份备份文件被改过";

export interface EncryptedEnvelope {
  format: typeof ENCRYPTED_FORMAT;
  version: 1;
  kdf: { name: "PBKDF2"; hash: "SHA-256"; iterations: number; salt: string };
  cipher: { name: "AES-GCM"; iv: string };
  data: string;
}

function subtle(): SubtleCrypto {
  const s = globalThis.crypto?.subtle;
  if (!s) throw new Error("当前环境不支持加密(缺少 WebCrypto),请导出不加密的备份");
  return s;
}

export function toB64(bytes: Uint8Array): string {
  let text = "";
  for (let i = 0; i < bytes.length; i += 0x8000) text += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
  return btoa(text);
}
export function fromB64(text: string): Uint8Array {
  const raw = atob(text);
  const out = new Uint8Array(raw.length);
  for (let i = 0; i < raw.length; i++) out[i] = raw.charCodeAt(i);
  return out;
}

async function deriveKey(passphrase: string, salt: Uint8Array, iterations: number): Promise<CryptoKey> {
  const material = await subtle().importKey("raw", new TextEncoder().encode(passphrase), "PBKDF2", false, ["deriveKey"]);
  return subtle().deriveKey({ name: "PBKDF2", hash: "SHA-256", salt: salt as BufferSource, iterations }, material, { name: "AES-GCM", length: 256 }, false, ["encrypt", "decrypt"]);
}

export function checkPassphrase(passphrase: string): string | null {
  const chars = [...passphrase].length;
  return chars < MIN_PASSPHRASE_CHARS ? `口令至少 ${MIN_PASSPHRASE_CHARS} 个字符(现在 ${chars} 个)` : null;
}

export async function encryptBackup(plain: string, passphrase: string): Promise<string> {
  const problem = checkPassphrase(passphrase);
  if (problem) throw new Error(problem);
  const salt = globalThis.crypto.getRandomValues(new Uint8Array(SALT_BYTES));
  const iv = globalThis.crypto.getRandomValues(new Uint8Array(IV_BYTES));
  const key = await deriveKey(passphrase, salt, ITERATIONS);
  const data = new Uint8Array(await subtle().encrypt({ name: "AES-GCM", iv: iv as BufferSource, additionalData: new TextEncoder().encode(ENCRYPTED_FORMAT) as BufferSource }, key, new TextEncoder().encode(plain)));
  const envelope: EncryptedEnvelope = {
    format: ENCRYPTED_FORMAT, version: 1,
    kdf: { name: "PBKDF2", hash: "SHA-256", iterations: ITERATIONS, salt: toB64(salt) },
    cipher: { name: "AES-GCM", iv: toB64(iv) }, data: toB64(data),
  };
  return JSON.stringify(envelope);
}

export function isEncryptedBackup(text: string): boolean {
  try {
    const value = JSON.parse(text) as { format?: unknown };
    return value?.format === ENCRYPTED_FORMAT;
  } catch { return false; }
}

export async function decryptBackup(text: string, passphrase: string): Promise<string> {
  let env: EncryptedEnvelope;
  try { env = JSON.parse(text) as EncryptedEnvelope; } catch { throw new Error("这不是有效的备份文件"); }
  if (env?.format !== ENCRYPTED_FORMAT || env.version !== 1 || env.kdf?.name !== "PBKDF2" || env.kdf.hash !== "SHA-256" || env.cipher?.name !== "AES-GCM") throw new Error("不认识的加密备份格式");
  // 迭代次数来自文件:限一个合理范围,免得被改成巨大的数把手机卡死
  if (!Number.isInteger(env.kdf.iterations) || env.kdf.iterations < 100_000 || env.kdf.iterations > 2_000_000) throw new Error("加密备份的参数不合理");
  try {
    const key = await deriveKey(passphrase, fromB64(env.kdf.salt), env.kdf.iterations);
    const plain = await subtle().decrypt({ name: "AES-GCM", iv: fromB64(env.cipher.iv) as BufferSource, additionalData: new TextEncoder().encode(ENCRYPTED_FORMAT) as BufferSource }, key, fromB64(env.data) as BufferSource);
    return new TextDecoder().decode(plain);
  } catch (error) {
    if (error instanceof Error && error.message.includes("WebCrypto")) throw error;
    throw new Error(WRONG);
  }
}
