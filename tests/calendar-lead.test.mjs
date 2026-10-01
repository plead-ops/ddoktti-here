import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import ts from '../apps/desktop/node_modules/typescript/lib/typescript.js';
const source=fs.readFileSync(new URL('../apps/desktop/src/calendar-lead.ts',import.meta.url),'utf8');
const code=ts.transpileModule(source,{compilerOptions:{module:ts.ModuleKind.ESNext}}).outputText;
const {leadTitle,spanText,timeRange}=await import('data:text/javascript;base64,'+Buffer.from(code).toString('base64'));
test('lead wording states the exact lead and keeps counting down',()=>{
 const now=1_700_000_000;
 assert.equal(leadTitle(now+3600,now),'1시간 뒤 일정이 시작돼요');
 assert.equal(leadTitle(now+5400,now),'1시간 30분 뒤 일정이 시작돼요');
 assert.equal(leadTitle(now+320,now),'5분 뒤 일정이 시작돼요');
 assert.equal(leadTitle(now+30,now),'지금 일정이 시작돼요');
 assert.equal(leadTitle(now-600,now),'10분 전에 일정이 시작됐어요');
 assert.equal(spanText(59),'1분');
});
test('time range uses local clock and drops a missing end',()=>{
 const start=new Date(2026,9,2,14,0).getTime()/1000,end=new Date(2026,9,2,14,30).getTime()/1000;
 assert.equal(timeRange(start,end),'14:00–14:30');
 assert.equal(timeRange(start),'14:00');
 assert.equal(timeRange(start,start),'14:00');
});
