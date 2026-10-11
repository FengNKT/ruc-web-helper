export function applyTheme(theme = 'system') {
  const value = ['light', 'dark'].includes(theme) ? theme : 'system';
  document.documentElement.dataset.colorMode = value;
  document.documentElement.style.colorScheme = value === 'system' ? 'light dark' : value;
  try { localStorage.setItem('ruc-theme', value); } catch {}
}
