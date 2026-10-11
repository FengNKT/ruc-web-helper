import {aggregateTraffic,monthlySessions} from './traffic.js';
import {formatBytes,formatDate,formatDuration} from './format.js';

export function setupTraffic(root, call, renameDevice, showDevice, acceptDevices) {
  const q=s=>root.querySelector(s),qa=s=>[...root.querySelectorAll(s)];
  const el=(tag,cls,text)=>{const e=document.createElement(tag);if(cls)e.className=cls;if(text!==undefined)e.textContent=text;return e;};
  const colors=Array.from({length:8},(_,i)=>`var(--ruc-chart-${i+1})`);
  let request=0,model=null,payload=null,selected='all',day=1,account='',active='traffic',sessionPage=1;
  const colorMap=new Map();
  const beijing=seconds=>new Date(seconds*1000).toLocaleString('zh-CN',{timeZone:'Asia/Shanghai',hour12:false});
  const color=key=>{if(!colorMap.has(key))colorMap.set(key,colors[colorMap.size%colors.length]);return colorMap.get(key);};
  function clear(text){q('[data-traffic-device]').disabled=true;q('[data-traffic-day]').disabled=true;model=null;payload=null;q('[data-traffic-content]').hidden=true;q('[data-traffic-status]').hidden=false;q('[data-traffic-status]').textContent=text;q('[data-traffic-chart]').replaceChildren();q('[data-traffic-ranking]').replaceChildren();renderSessions(text);}
  async function load(){
    const token=++request;const [year,month]=q('[data-month]').value.split('-').map(Number);
    const previousMonth=model?`${model.year}-${model.month}`:'';
    sessionPage=1;clear('正在读取完整会话记录…');q('[data-traffic-refresh]').disabled=true;
    try {
      const result=await call('get_traffic_data',{year,month});if(token!==request)return;
      if(account!==result.username){colorMap.clear();if(account)selected='all';account=result.username;}
      payload=result;model=aggregateTraffic(result.items,result.devices,year,month,undefined,result.deviceNames);
      acceptDevices(result);
      const choices=new Map(model.devices.map(d=>[d.key,d.name]));
      const missing=selected!=='all'&&!choices.has(selected);
      if(missing)selected='all';
      day=Math.max(1,Math.min(model.days.length,previousMonth===`${year}-${month}`?day:model.elapsedDays||1));
      const select=q('[data-traffic-device]');select.replaceChildren(new Option('全部设备','all'));
      choices.forEach((name,key)=>{select.append(new Option(name,key));color(key)});select.value=selected;select.disabled=false;
      q('[data-traffic-day]').replaceChildren(...model.days.map(d=>new Option(`${month} 月 ${d.day} 日`,String(d.day))));
      q('[data-traffic-day]').value=String(day);q('[data-traffic-day]').disabled=false;
      q('[data-traffic-status]').hidden=!missing;if(missing)q('[data-traffic-status]').textContent='该设备在所选月份暂无记录，已切换为全部设备。';q('[data-traffic-content]').hidden=false;
      q('[data-traffic-coverage]').textContent=`记录查询范围：${result.queryFrom} 至 ${result.queryUntil}。所选月份涉及 ${model.sessionCount} 次结束会话，其中 ${model.crossDaySessions} 次跨天。${model.invalid?`另有 ${model.invalid} 条无效记录未计入。`:''}${result.warnings?.length?`设备信息提示：${result.warnings.join('；')}`:''}`;
      q('[data-traffic-official]').textContent=`当前账期已用 ${formatBytes(result.usedBytes)}，当前可用 ${formatBytes(result.remainBytes)}；结算日 ${formatDate(result.checkoutDate,true)}。这些是当前账号数据，与所选历史月份及估算合计口径不同。`;
      render();
    } catch(error){if(token===request)clear(typeof error==='string'?error:error.message||'统计读取失败，请重试');}
    finally{if(token===request)q('[data-traffic-refresh]').disabled=false;}
  }
  const value=d=>selected==='all'?d.total:(d.values.get(selected)||0);
  const visible=()=>model.devices.filter(d=>selected==='all'||d.key===selected);
  function render(){
    if(!model)return;
    const total=model.days.reduce((sum,d)=>sum+value(d),0),elapsed=model.elapsedDays;
    const peak=model.days.slice(0,elapsed).reduce((a,b)=>!a||value(b)>value(a)?b:a,null);
    q('[data-traffic-total]').textContent=visible().length?formatBytes(total):'—';
    q('[data-traffic-total-label]').textContent=selected==='all'?'所选月份估算用量':'所选设备估算用量';
    q('[data-traffic-average]').textContent=elapsed&&visible().length?formatBytes(total/elapsed):'—';
    q('[data-traffic-peak]').textContent=peak&&value(peak)>0?`${model.month} 月 ${peak.day} 日 · ${formatBytes(value(peak))}`:'—';
    const monthPie=q('[data-month-pie]');monthPie.replaceChildren();
    if(total>0)monthPie.append(pie(visible().map(device=>({...device,bytes:device.total})),total,`${model.month}月设备估算流量占比`,'monthPie'));
    else monthPie.append(el('p','ruc-traffic-note','暂无可展示的月度流量占比'));
    const ranking=q('[data-traffic-ranking]');ranking.replaceChildren();
    visible().forEach(d=>{
      const row=el('button','ruc-ranking-row');row.type='button';row.setAttribute('aria-label',`查看${d.name}，估算用量 ${formatBytes(d.total)}`);
      const line=el('span','ruc-ranking-label');const name=el('span','ruc-ranking-name');const swatch=el('i','ruc-device-swatch');swatch.style.background=color(d.key);name.append(swatch,document.createTextNode(d.name));line.append(name,el('span','',`${formatBytes(d.total)} · ${total?(d.total/total*100).toFixed(1):'0.0'}%`));
      const track=el('span','ruc-ranking-track'),bar=el('span','');bar.style.background=color(d.key);bar.style.width=`${total?d.total/total*100:0}%`;track.append(bar);row.append(line,track);
      row.addEventListener('click',()=>{selected=d.key;sessionPage=1;q('[data-traffic-device]').value=selected;render()});
      const wrapper=el('div','ruc-ranking-item');wrapper.append(row);
      if(d.key!=='unknown'){const edit=el('button','ruc-text-button','命名');edit.type='button';edit.setAttribute('aria-label',`命名${d.name}`);edit.addEventListener('click',()=>renameDevice({mac:d.key,name:d.name,customName:payload.deviceNames?.[d.key]||'',username:payload.username}));wrapper.append(edit)}
      if(d.key!=='unknown'){const device=el('button','ruc-text-button','设备详情');device.type='button';device.addEventListener('click',()=>showDevice(d.key));wrapper.append(device);}
      ranking.append(wrapper);
    });
    if(!visible().length)ranking.append(el('p','ruc-traffic-note','暂无已结束会话记录，不能据此判断实际用量为零'));
    draw();renderDay();renderSessions();
  }
  function renderSessions(status=''){
    const target=q('[data-record-body]');target.replaceChildren();
    const records=model?monthlySessions(model,selected):[];
    const pages=Math.max(1,Math.ceil(records.length/10));sessionPage=Math.min(sessionPage,pages);
    const names=new Map(model?.devices.map(d=>[d.key,d.name])||[]);
    records.slice((sessionPage-1)*10,sessionPage*10).forEach(record=>{
      const row=el('tr');[names.get(record.key)||'未归属设备',formatDate(record.start),record.ip,formatDuration(record.end-record.start),formatBytes(record.allocated),formatBytes(record.bytes)].forEach(value=>row.append(el('td','',value)));target.append(row);
    });
    if(!records.length){const row=el('tr'),cell=el('td','',status||'当前月份与设备范围暂无结束会话');cell.colSpan=6;row.append(cell);target.append(row);}
    q('[data-record-info]').textContent=model?`第 ${sessionPage} 页 · 共 ${records.length} 条`:status;
    q('[data-record-prev]').disabled=!model||sessionPage<=1;q('[data-record-next]').disabled=!model||sessionPage>=pages;
  }
  function pie(entries,total,label,kind){
      const svg=document.createElementNS('http://www.w3.org/2000/svg','svg');
      svg.setAttribute('viewBox','0 0 200 200');svg.setAttribute('class','ruc-statistics-pie');svg.dataset[kind]='';svg.setAttribute('role','img');
      svg.setAttribute('aria-label',label);
      let angle=-Math.PI/2;
      const positive=entries.filter(device=>device.bytes>0);
      positive.forEach(device=>{
        const fraction=device.bytes/total,end=angle+fraction*Math.PI*2;
        const mark=document.createElementNS(svg.namespaceURI,positive.length===1?'circle':'path');
        if(positive.length===1){mark.setAttribute('cx','100');mark.setAttribute('cy','100');mark.setAttribute('r','94');}
        else mark.setAttribute('d',`M 100 100 L ${100+94*Math.cos(angle)} ${100+94*Math.sin(angle)} A 94 94 0 ${fraction>0.5?1:0} 1 ${100+94*Math.cos(end)} ${100+94*Math.sin(end)} Z`);
        mark.setAttribute('fill',color(device.key));mark.setAttribute('stroke','var(--ruc-bg)');mark.setAttribute('stroke-width','2');
        const title=document.createElementNS(svg.namespaceURI,'title');title.textContent=`${device.name} · ${formatBytes(device.bytes)} · ${(fraction*100).toFixed(1)}%`;mark.append(title);svg.append(mark);angle=end;
      });
      return svg;
  }
  function renderDay(){
    if(!model)return;
    const target=q('[data-traffic-day-detail]'),d=model.days[day-1];target.replaceChildren();
    const records=d.records.filter(r=>selected==='all'||r.key===selected);
    const summary=el('div','ruc-traffic-summary ruc-daily-summary');const usage=el('div');usage.append(el('span','',`${model.month} 月 ${day} 日估算用量`),el('strong','',records.length?formatBytes(value(d)):'—'));const sessions=el('div');sessions.append(el('span','','涉及结束会话'),el('strong','',`${records.length} 次`));summary.append(usage,sessions);target.append(summary);target.append(el('h2','ruc-day-chart-title','当日设备分布'));
    if(d.start>Date.now()/1000){target.append(el('p','ruc-traffic-note','该日期尚未到来'));return;}
    if(!records.length){target.append(el('p','ruc-traffic-note','暂无可分配的结束会话，不能据此判断当天没有使用流量。'));return;}
    const entries=visible().filter(device=>d.values.has(device.key)).map(device=>({...device,bytes:d.values.get(device.key)}));
    const total=entries.reduce((sum,device)=>sum+device.bytes,0);
    const distribution=el('div','ruc-day-distribution');
    if(total>0)distribution.append(pie(entries,total,`${model.month}月${day}日${selected==='all'?'各设备':'所选设备'}估算流量占比`,'dayPie'));
    else distribution.append(el('p','ruc-traffic-note','当日估算合计为 0 B，暂无可展示的流量占比。'));
    const breakdown=el('div','ruc-day-breakdown');
    breakdown.append(el('p','ruc-day-distribution-title',selected==='all'?'当日设备用量占比':'当日所选设备用量占比'));
    entries.forEach(device=>{
      const row=el('div','ruc-day-device');const swatch=el('i','');swatch.style.background=color(device.key);
      row.append(swatch,el('span','ruc-day-device-name',device.name),el('span','ruc-day-device-value',`${formatBytes(device.bytes)} · ${total?(device.bytes/total*100).toFixed(1)+'%':'—'}`));breakdown.append(row);
    });distribution.append(breakdown);target.append(distribution);
    const cross=records.filter(r=>r.crossDay).length;
    if(cross)target.append(el('p','ruc-traffic-note',`包含 ${cross} 次跨天会话，按各日在线时长分配。`));
    const details=el('details','ruc-day-sessions');details.append(el('summary','','查看会话分配明细'));
    records.forEach(r=>{const device=model.devices.find(d=>d.key===r.key);const row=el('div','ruc-session-detail');row.append(el('strong','',device.name),el('span','',`${beijing(r.start)} → ${beijing(r.end)}（${formatDuration(r.end-r.start)}）`),el('span','',`会话总量 ${formatBytes(r.bytes)} · 当日分配 ${formatBytes(r.allocated)}${r.crossDay?' · 跨天估算':''}`));details.append(row)});target.append(details);
  }
  function draw(){
    if(!model||q('#month-panel').hidden||q('#traffic-panel').hidden||q('[data-panel="records"]').hidden)return;
    const chart=q('[data-traffic-chart]'),width=chart.clientWidth;if(width<100)return;
    const height=260,left=48,right=12,top=27,bottom=39,w=width-left-right,h=height-top-bottom;
    const svg=document.createElementNS('http://www.w3.org/2000/svg','svg');svg.setAttribute('viewBox',`0 0 ${width} ${height}`);svg.setAttribute('class','ruc-traffic-svg');svg.setAttribute('role','img');svg.setAttribute('aria-label',`${model.year}年${model.month}月每日估算流量堆叠图，点击柱图或切换日统计查看详情`);
    const add=(tag,attrs,text)=>{const n=document.createElementNS(svg.namespaceURI,tag);Object.entries(attrs).forEach(([k,v])=>n.setAttribute(k,v));if(text!==undefined)n.textContent=text;svg.append(n);return n;};
    const maximum=Math.max(1,...model.days.map(value));
    const unit=maximum>=1e9?1e9:maximum>=1e6?1e6:maximum>=1e3?1e3:1,label=unit===1e9?'GB':unit===1e6?'MB':unit===1e3?'KB':'B';
    const raw=maximum/unit,step=10**Math.floor(Math.log10(raw));const ceiling=Math.ceil(raw/step)*step;
    for(let i=0;i<=4;i++){const y=top+h-h*i/4;add('line',{x1:left,x2:width-right,y1:y,y2:y,stroke:'var(--ruc-line)'});add('text',{x:left-7,y:y+4,'text-anchor':'end'},Number((ceiling*i/4).toPrecision(3)).toString());}
    add('text',{x:left,y:15},`流量（${label}）`);
    const bw=w/model.days.length;
    model.days.forEach((d,i)=>{let base=0;visible().forEach(device=>{const bytes=d.values.get(device.key)||0;if(!bytes)return;const bh=bytes/unit/ceiling*h;const mark=add('rect',{x:left+i*bw+bw*.14,y:top+h-base-bh,width:bw*.72,height:bh,fill:color(device.key)});const title=document.createElementNS(svg.namespaceURI,'title');title.textContent=`${model.month}月${d.day}日 · ${device.name} · 估算 ${formatBytes(bytes)}`;mark.append(title);base+=bh;});});
    const ticks=width<420?[1,10,20,model.days.length]:[1,5,10,15,20,25,model.days.length];
    [...new Set(ticks)].forEach(n=>add('text',{x:left+(n-.5)*bw,y:height-18,'text-anchor':'middle'},String(n)));
    add('text',{x:width-right,y:height-2,'text-anchor':'end'},'日期（日）');
    add('line',{x1:left+(day-.5)*bw,x2:left+(day-.5)*bw,y1:top,y2:top+h,stroke:'var(--ruc-muted)','stroke-dasharray':'3 3','pointer-events':'none'});
    svg.addEventListener('click',event=>{const rect=svg.getBoundingClientRect(),x=(event.clientX-rect.left)*width/rect.width;day=Math.min(model.days.length,Math.max(1,Math.floor((x-left)/bw)+1));q('[data-traffic-day]').value=String(day);tab('day');renderDay();q('.ruc-main').scrollTop=0});
    chart.replaceChildren(svg);
  }
  function tab(view){
    active=view;qa('[data-record-tab]').forEach(b=>b.setAttribute('aria-selected',String(b.dataset.recordTab===view)));
    q('#traffic-panel').hidden=view==='history';q('#history-panel').hidden=view!=='history';
    q('#month-panel').hidden=view!=='traffic';q('#day-panel').hidden=view!=='day';
    q('[data-statistics-day-filter]').hidden=view!=='day';
    draw();
  }
  qa('[data-record-tab]').forEach(b=>b.addEventListener('click',()=>tab(b.dataset.recordTab)));
  q('[data-traffic-device]').addEventListener('change',event=>{selected=event.target.value;sessionPage=1;render()});
  q('[data-traffic-day]').addEventListener('change',event=>{day=Number(event.target.value);draw();renderDay()});
  q('[data-record-prev]').addEventListener('click',()=>{sessionPage--;renderSessions()});
  q('[data-record-next]').addEventListener('click',()=>{sessionPage++;renderSessions()});
  new ResizeObserver(draw).observe(q('[data-traffic-chart]'));
  return {load,show:()=>{tab(active);draw()},selectDevice:key=>{selected=key;active='traffic';sessionPage=1;},invalidate:(preserve=false)=>{++request;if(!preserve){account='';selected='all';colorMap.clear();}clear('请刷新统计');q('[data-traffic-refresh]').disabled=false;}};
}
