import { createCipheriv, createDecipheriv, randomBytes } from "node:crypto";
import { loadConfig } from "../config.js";

/**
 * user token at-rest 암호화 (AES-256-GCM). PRD §13.3.
 * TOKEN_ENC_KEY 형식: "base64:<32바이트 base64>".
 * 주의: 키가 서버와 같은 환경이면 서버 침해 시 무력 — 가능하면 외부 KMS로 분리.
 */
function key(): Buffer {
  const raw = loadConfig().TOKEN_ENC_KEY;
  const b64 = raw.startsWith("base64:") ? raw.slice("base64:".length) : raw;
  const buf = Buffer.from(b64, "base64");
  if (buf.length !== 32) {
    throw new Error("TOKEN_ENC_KEY must decode to 32 bytes");
  }
  return buf;
}

/** 반환 형식: base64(iv).base64(tag).base64(ciphertext) */
export function encryptToken(plaintext: string): string {
  const iv = randomBytes(12);
  const cipher = createCipheriv("aes-256-gcm", key(), iv);
  const ct = Buffer.concat([cipher.update(plaintext, "utf8"), cipher.final()]);
  const tag = cipher.getAuthTag();
  return [iv.toString("base64"), tag.toString("base64"), ct.toString("base64")].join(".");
}

export function decryptToken(packed: string): string {
  const [ivB64, tagB64, ctB64] = packed.split(".");
  if (!ivB64 || !tagB64 || !ctB64) throw new Error("malformed ciphertext");
  const decipher = createDecipheriv("aes-256-gcm", key(), Buffer.from(ivB64, "base64"));
  decipher.setAuthTag(Buffer.from(tagB64, "base64"));
  return Buffer.concat([decipher.update(Buffer.from(ctB64, "base64")), decipher.final()]).toString(
    "utf8",
  );
}
