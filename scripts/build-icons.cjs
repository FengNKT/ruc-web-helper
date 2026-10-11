// Render the editable vector master; produce Windows icons with true alpha.
const { chromium } = require('playwright');
const { readFileSync, copyFileSync } = require('node:fs');
const { resolve } = require('node:path');
const { spawnSync } = require('node:child_process');
const artifact = require('./lib/artifacts.cjs');
(async () => {
  const directory = resolve(__dirname, '../src-tauri/icons');
  const preview = artifact('ruc-web-helper-1024.png','icons');
  copyFileSync(resolve(directory,'ruc-web-helper.svg'),resolve(__dirname,'../public/app-icon.svg'));
  const browser = await chromium.launch({channel: 'msedge', headless: true});
  try {
    const page = await browser.newPage({viewport: {width: 1024, height: 1024}, deviceScaleFactor: 1});
    const svg = readFileSync(resolve(directory, 'ruc-web-helper.svg'), 'utf8');
    await page.setContent(`<style>html,body{margin:0;background:transparent}svg{width:1024px;height:1024px;display:block}</style>${svg}`);
    await page.screenshot({path: preview, omitBackground: true});
  } finally {
    await browser.close();
  }
  const result = spawnSync('python', ['-c', `
from pathlib import Path
from PIL import Image
p = Path(${JSON.stringify(directory)})
preview = Path(${JSON.stringify(preview)})
source = Image.open(preview).convert('RGBA')
source.resize((512,512), Image.Resampling.LANCZOS).save(p / 'icon.png')
source.save(p / 'icon.ico', sizes=[(16,16),(20,20),(24,24),(32,32),(40,40),(48,48),(64,64),(128,128),(256,256)])
for size in [16,32,48,128,256]:
    source.resize((size,size), Image.Resampling.LANCZOS).save(preview.parent / f'icon-{size}.png')
ico = Image.open(p / 'icon.ico')
assert len(ico.ico.sizes()) == 9
assert source.getpixel((0,0))[3] == 0
print('PASS: transparent PNG and nine Windows icon sizes')
`], {stdio: 'inherit'});
  if (result.error) throw result.error;
  process.exitCode = result.status;
})().catch(error => {console.error(error); process.exitCode = 1;});
