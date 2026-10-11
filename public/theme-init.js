// A local display cache avoids a bright flash before settings.json is loaded.
try {
  const theme = localStorage.getItem('ruc-theme');
  document.documentElement.style.colorScheme = theme === 'light' || theme === 'dark' ? theme : 'light dark';
} catch {}
