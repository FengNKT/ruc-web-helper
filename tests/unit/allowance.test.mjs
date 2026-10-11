import test from 'node:test';
import assert from 'node:assert/strict';
import {calculateAllowance,formatAllowance} from '../../src/allowance.js';
const GB=1e9;
test('monthly free allowance and whole paid GB do not double-charge existing usage',()=>{
  const a=calculateAllowance(30*GB,12.34);
  assert.deepEqual([a.used,a.free,a.paid,a.remaining],[30*GB,70*GB,24*GB,94*GB]);
  assert.ok(Math.abs(a.shares.reduce((sum,v)=>sum+v,0)-100)<1e-10);
  assert.deepEqual(calculateAllowance(110*GB,1).shares,[110/112*100,0,2/112*100]);
  for(const [balance,paid] of [[0,0],[0.49,0],[0.5,1],[0.99,1],[1,2],[12.34,24],[-1,0]])assert.equal(calculateAllowance(0,balance).paid,paid*GB);
  assert.equal(calculateAllowance(100*GB,0).remaining,0);
  assert.equal(formatAllowance(0),'0.00 GB');
});
test('missing readings remain unknown instead of becoming free or paid quota',()=>{
  assert.equal(calculateAllowance(null,1).free,null);
  assert.equal(calculateAllowance(-999999,1).remaining,null);
  assert.equal(calculateAllowance(0,null).paid,null);
  assert.equal(calculateAllowance(0,NaN).shares,null);
  assert.equal(formatAllowance(null),'—');
});
