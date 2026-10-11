// All day boundaries use Beijing time, independently of the computer's timezone.
export const DAY = 86400;
const OFFSET = 8 * 3600;
export function monthBounds(year, month) {
  return [Date.UTC(year, month - 1, 1) / 1000 - OFFSET, Date.UTC(year, month, 1) / 1000 - OFFSET];
}
export function normalizeMac(value) {
  const mac = String(value || '').replace(/[:-]/g, '').toLowerCase();
  return /^[0-9a-f]{12}$/.test(mac) && mac !== '000000000000' ? mac : 'unknown';
}
export function aggregateTraffic(items, devices, year, month, now = Date.now() / 1000, names = {}) {
  const [from, until] = monthBounds(year, month);
  const days = Array.from({length: Math.round((until - from) / DAY)}, (_, i) => ({
    day: i + 1, start: from + i * DAY, total: 0, values: new Map(), records: [], crossDay: 0,
  }));
  const known = new Map((devices || []).map(d => [normalizeMac(d.mac), d]));
  const groups = new Map(), seen = new Set();
  let invalid = 0, crossDaySessions = 0, sessionCount = 0;
  for (const [index, item] of (items || []).entries()) {
    const id = String(item.detail_id ?? '');
    if (id && seen.has(id)) continue;
    if (id) seen.add(id);
    const start = Number(item.add_time), end = Number(item.drop_time), bytes = Number(item.total_bytes);
    if (!Number.isFinite(start) || !Number.isFinite(end) || start <= 0 || end < start ||
        !Number.isSafeInteger(bytes) || bytes < 0 || item.total_bytes == null) { invalid++; continue; }
    // A zero-duration record belongs to its timestamp; an end exactly at midnight does not enter the next day.
    if (start === end ? start < from || start >= until : end <= from || start >= until) continue;
    const key = normalizeMac(item.user_mac);
    if (!groups.has(key)) {
      const device = known.get(key);
      groups.set(key, {key, name: key === 'unknown' ? '未归属设备' : names[key] || device?.name || `${device?.os || '设备'} · ${key.slice(-4).toUpperCase()}`,
        total: 0, sessions: 0, current: !!device?.current});
    }
    const group = groups.get(key); group.sessions++; sessionCount++;
    const isCrossDay = Math.floor((start + OFFSET) / DAY) !== Math.floor(((end > start ? end - 0.001 : end) + OFFSET) / DAY);
    if (isCrossDay) crossDaySessions++;
    for (const day of days) {
      const a = Math.max(start, day.start), b = Math.min(end, day.start + DAY);
      if (start === end ? start < day.start || start >= day.start + DAY : b <= a) continue;
      // Difference of cumulative rounded allocations conserves bytes across day/month boundaries.
      const allocated = end === start ? bytes : Math.round(bytes * ((b - start) / (end - start))) - Math.round(bytes * ((a - start) / (end - start)));
      day.total += allocated; group.total += allocated;
      day.values.set(key, (day.values.get(key) || 0) + allocated);
      day.records.push({id, sessionKey:id?`id:${id}`:`row:${index}`, key, start, end, bytes, allocated, ip:item.user_ip||'—', crossDay: isCrossDay});
      if (isCrossDay) day.crossDay++;
    }
  }
  return {year, month, from, until, days, devices: [...groups.values()].sort((a,b) => b.total - a.total || a.key.localeCompare(b.key)),
    total: days.reduce((sum, d) => sum + d.total, 0), invalid, crossDaySessions, sessionCount,
    elapsedDays: Math.max(0, Math.min(days.length, Math.floor((now - from) / DAY) + 1))};
}
export function monthlySessions(model, selected = 'all') {
  const sessions = new Map();
  for (const day of model.days) for (const record of day.records) {
    if (selected !== 'all' && record.key !== selected) continue;
    const existing = sessions.get(record.sessionKey);
    if (existing) existing.allocated += record.allocated;
    else sessions.set(record.sessionKey, {...record});
  }
  return [...sessions.values()].sort((a,b)=>b.start-a.start||a.sessionKey.localeCompare(b.sessionKey));
}
