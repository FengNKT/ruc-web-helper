export function formatBytes(value){
  if(value===-999999)return '不限量';
  if(typeof value!=='number'||!Number.isFinite(value)||value<0)return '—';
  if(value<1000)return `${value.toFixed(0)} B`;
  const units=['KB','MB','GB','TB'];let result=value/1000,index=0;
  while(result>=1000&&index<units.length-1){result/=1000;index++;}
  return `${result.toFixed(2)} ${units[index]}`;
}
export function formatDate(value,dateOnly=false){
  if(value==null||value==='')return '—';
  const number=Number(value);const date=new Date(Number.isFinite(number)?(number<1e12?number*1000:number):value);
  if(!Number.isFinite(date.getTime()))return '—';
  return dateOnly?date.toLocaleDateString('zh-CN'):date.toLocaleString('zh-CN',{hour12:false});
}
export function formatDuration(value){
  const seconds=Number(value);if(!Number.isFinite(seconds)||seconds<0)return '—';
  if(seconds<60)return `${Math.floor(seconds)} 秒`;
  return `${Math.floor(seconds/3600)} 小时 ${Math.floor(seconds%3600/60)} 分`;
}
export function isOnline(value){return value===true||value===1||value==='1';}
export function numericId(value){if(value===null||value===''||value===undefined)return null;const number=Number(value);return Number.isSafeInteger(number)&&number>0?number:null;}
export function formatMac(value){const mac=String(value||'').replace(/[:-]/g,'');return /^[0-9a-f]{12}$/i.test(mac)&&mac!=='000000000000'?mac.toUpperCase().match(/../g).join(':'):'—';}
