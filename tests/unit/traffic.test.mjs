import test from 'node:test';
import assert from 'node:assert/strict';
import {aggregateTraffic,monthlySessions,monthBounds,normalizeMac,DAY} from '../../src/traffic.js';
const t = value => Date.parse(value) / 1000;
const record = (id,start,end,bytes,mac='aa:bb:cc:dd:ee:ff') => ({detail_id:id,add_time:t(start),drop_time:t(end),total_bytes:bytes,user_mac:mac});
test('session statistics share monthly allocation and filter before pagination',()=>{
  const cross=record(1,'2026-09-30T23:00:00+08:00','2026-10-01T03:00:00+08:00',1000);
  const another=record(2,'2026-10-02T23:00:00+08:00','2026-10-03T03:00:00+08:00',2000,'112233445566');
  const duplicateWithoutId=record(undefined,'2026-10-04T00:00:00+08:00','2026-10-04T01:00:00+08:00',50);
  const model=aggregateTraffic([cross,another,duplicateWithoutId,duplicateWithoutId],[],2026,10);
  const sessions=monthlySessions(model);
  assert.equal(sessions.length,4);
  assert.equal(sessions.reduce((sum,r)=>sum+r.allocated,0),model.total);
  const selected=monthlySessions(model,'aabbccddeeff');
  assert.equal(selected.length,3);
  assert.equal(selected.find(r=>r.id==='1').allocated,750);
  assert.equal(selected.find(r=>r.id==='1').bytes,1000);
  assert.ok(selected.every(r=>r.key==='aabbccddeeff'));
  assert.equal(monthlySessions(model,'abcdef123456').length,0);
});
test('allocation follows time overlap, conserves bytes and uses Beijing midnight',()=>{
  const r=record(1,'2026-10-10T23:00:00+08:00','2026-10-11T03:00:00+08:00',1001);
  const m=aggregateTraffic([r,r],[{mac:'AA-BB-CC-DD-EE-FF',name:'电脑'}],2026,10);
  assert.equal(m.days[9].total,250);assert.equal(m.days[10].total,751);
  assert.equal(m.total,1001);assert.equal(m.sessionCount,1);assert.equal(m.crossDaySessions,1);
  assert.equal(m.devices[0].name,'电脑');assert.equal(m.devices[0].total,1001);
});
test('cross-month clipping preserves total with rounding; midnight end is exclusive',()=>{
  const r=record(1,'2026-09-30T23:00:00+08:00','2026-10-01T02:00:00+08:00',10);
  const a=aggregateTraffic([r],[],2026,9),b=aggregateTraffic([r],[],2026,10);
  assert.equal(a.total,3);assert.equal(b.total,7);assert.equal(a.total+b.total,10);
  const midnight=aggregateTraffic([record(2,'2026-10-01T23:00:00+08:00','2026-10-02T00:00:00+08:00',100)],[],2026,10);
  assert.equal(midnight.days[0].total,100);assert.equal(midnight.days[1].records.length,0);assert.equal(midnight.crossDaySessions,0);
});
test('invalid records, zero durations, unassigned devices and leap/year boundaries',()=>{
  const [a,b]=monthBounds(2024,2);assert.equal((b-a)/DAY,29);
  assert.equal(monthBounds(2026,12)[1],monthBounds(2027,1)[0]);
  const r=record(1,'2026-10-01T12:00:00+08:00','2026-10-01T12:00:00+08:00',9,'');
  const m=aggregateTraffic([r,{...r,detail_id:2,total_bytes:-1},{...r,detail_id:3,drop_time:0}],[],2026,10,t('2026-10-02T00:00:00+08:00'));
  assert.equal(m.total,9);assert.equal(m.invalid,2);assert.equal(m.devices[0].key,'unknown');assert.equal(m.elapsedDays,2);
  assert.equal(normalizeMac('bad'),'unknown');
});
test('custom names override official names and cover historical devices',()=>{
  const r=record(1,'2026-10-01T12:00:00+08:00','2026-10-01T13:00:00+08:00',100);
  const aliases={aabbccddeeff:'历史电脑'};
  assert.equal(aggregateTraffic([r],[],2026,10,undefined,aliases).devices[0].name,'历史电脑');
  assert.equal(aggregateTraffic([r],[{mac:r.user_mac,name:'学校设备名'}],2026,10,undefined,aliases).devices[0].name,'历史电脑');
});
