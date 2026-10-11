import test from 'node:test';
import assert from 'node:assert/strict';
import {formatBytes,formatDate,formatDuration,isOnline,numericId,formatMac} from '../../src/format.js';

test('accounting uses decimal units and distinguishes unknown, zero and unlimited',()=>{
  assert.equal(formatBytes(null),'—');assert.equal(formatBytes(undefined),'—');
  assert.equal(formatBytes(0),'0 B');assert.equal(formatBytes(-999999),'不限量');
  assert.equal(formatBytes(1000000000),'1.00 GB');assert.equal(formatBytes(-1),'—');
  assert.equal(formatBytes(NaN),'—');
});
test('device flags and identifiers do not treat string zero as online',()=>{
  assert.equal(isOnline('0'),false);assert.equal(isOnline('1'),true);
  assert.equal(isOnline(false),false);assert.equal(isOnline(1),true);
  assert.equal(numericId(null),null);assert.equal(numericId(''),null);
  assert.equal(numericId('123'),123);assert.equal(numericId(0),null);
  assert.equal(numericId('1e25'),null);
});
test('invalid dates do not turn into an epoch date, and durations reject invalid values',()=>{
  assert.equal(formatDate(null),'—');assert.equal(formatDate('invalid'),'—');
  assert.equal(formatDuration(3665),'1 小时 1 分');assert.equal(formatDuration(-1),'—');
});
test('MAC addresses are displayed consistently and invalid addresses stay unknown',()=>{
  assert.equal(formatMac('aa-bb-cc-dd-ee-ff'),'AA:BB:CC:DD:EE:FF');
  assert.equal(formatMac('aabbccddeeff'),'AA:BB:CC:DD:EE:FF');
  assert.equal(formatMac(null),'—');assert.equal(formatMac('000000000000'),'—');
});
