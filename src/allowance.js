const GB=1_000_000_000;
export function calculateAllowance(usedBytes,balance){
  const used=typeof usedBytes==='number'&&Number.isFinite(usedBytes)&&usedBytes>=0?usedBytes:null;
  const free=used===null?null:Math.max(0,100*GB-used);
  const paid=typeof balance==='number'&&Number.isFinite(balance)?Math.floor(Math.max(0,balance)/0.5)*GB:null;
  const remaining=free===null||paid===null?null:free+paid;
  const total=used===null||remaining===null?null:used+remaining;
  return {used,free,paid,remaining,shares:total>0?[used,free,paid].map(value=>value/total*100):null};
}
export function formatAllowance(value){
  return typeof value==='number'&&Number.isFinite(value)&&value>=0?`${(value/GB).toFixed(2)} GB`:'—';
}
