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

export function diagPush(r: Omit<DiagRecord, "t">): void {
  buf.push({ t: Date.now(), ...r });
  if (buf.length > 50) buf.shift();
}

export function diagRecent(n = 20): DiagRecord[] {
  return buf.slice(-n);
}
