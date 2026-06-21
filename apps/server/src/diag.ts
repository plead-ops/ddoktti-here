/** 임시 진단용 인메모리 링버퍼 — 최근 메시지 이벤트 처리 결과 (디버깅 후 제거 예정). */
interface DiagRecord {
  t: number;
  channelType?: string;
  channel: string;
  bot: boolean;
  subtype: string | null;
  candidates: number;
  results: Array<{ userId: string; outcome: string; trigger?: string | null }>;
}

const buf: DiagRecord[] = [];
let total = 0; // 부팅 후 받은 message 이벤트 총수(타입 무관)
let slackConnected = false; // Socket Mode 연결 성공 여부

export function setSlackConnected(b: boolean): void {
  slackConnected = b;
}

export function diagPush(r: Omit<DiagRecord, "t">): void {
  total += 1;
  buf.push({ t: Date.now(), ...r });
  if (buf.length > 50) buf.shift();
}

export function diagSummary(): { slackConnected: boolean; total: number; recent: DiagRecord[] } {
  return { slackConnected, total, recent: buf.slice(-20) };
}
