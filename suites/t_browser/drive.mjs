// T-BROWSER — drive the corpus with a real browser.
//
// The suite's other runtime tests drive Ashlar with an HTTP client this repo
// wrote, which is exactly the client that cannot find the things a browser
// finds: a page with no title, a favicon nobody answers, a socket that
// reconnects on reload, an editor merging two people's real keystrokes.
// Every finding this file asserts was a defect when it was written.
//
// Not in CI, and not a workspace dependency (G1): it needs a browser and
// node, which `cargo test` must never need. Same shape as T-A3 — a gate run
// by hand, with its results recorded under results/.
//
//   node <repo>/suites/t_browser/drive.mjs <ashlar-binary> --root <repo> [--shots DIR]
import { spawn } from 'node:child_process';
import { mkdirSync } from 'node:fs';
import { createRequire } from 'node:module';

// Resolved from the OPERATOR's directory, not this file's. Playwright is a
// tool the gate is run with, never a dependency of this repo (G1) — install
// it wherever you like and run the gate from there.
const require = createRequire(`${process.cwd()}/`);
let chromium;
try {
  ({ chromium } = require('playwright'));
} catch {
  console.error(
    'T-BROWSER needs playwright resolvable from the current directory.\n' +
    'See suites/t_browser/PROTOCOL.md.'
  );
  process.exit(2);
}

const BIN = process.argv[2];
const flag = (name, fallback = null) => {
  const i = process.argv.indexOf(name);
  return i > -1 ? process.argv[i + 1] : fallback;
};
const SHOTS = flag('--shots');
// The repo whose examples are served. Given explicitly because the gate is
// run from wherever playwright is installed, which is not this repo.
const ROOT = flag('--root', process.cwd());
if (SHOTS) mkdirSync(SHOTS, { recursive: true });

const checks = [];
const check = (name, pass, detail = '') => {
  checks.push({ name, pass, detail });
  console.log(`${pass ? 'PASS' : 'FAIL'}  ${name}${detail ? `  — ${detail}` : ''}`);
};

const sleep = ms => new Promise(r => setTimeout(r, ms));

async function serve(example, port) {
  const p = spawn(BIN, ['run', `examples/${example}`, '--port', String(port)], {
    cwd: ROOT, stdio: 'ignore',
  });
  for (let i = 0; i < 60; i++) {
    try {
      const r = await fetch(`http://127.0.0.1:${port}/`);
      if (r.ok) return p;
    } catch {}
    await sleep(250);
  }
  p.kill();
  throw new Error(`${example} did not come up on ${port}`);
}

const browser = await chromium.launch({
  executablePath: process.env.ASHLAR_CHROMIUM || undefined,
});

// ---- counter: titles, console cleanliness, live patch, reload ------------
{
  const srv = await serve('counter', 8401);
  const ctx = await browser.newContext();
  const page = await ctx.newPage();
  const errors = [];
  page.on('console', m => m.type() === 'error' && errors.push(m.text()));
  page.on('pageerror', e => errors.push(`pageerror: ${e.message}`));
  await page.goto('http://127.0.0.1:8401/', { waitUntil: 'load' });
  await sleep(900);

  const btn = page.locator('button').first();
  const before = await btn.textContent();
  await btn.click(); await sleep(400);
  const after = await btn.textContent();
  check('counter: a click patches the view in place, server-side',
    before === 'clicks: 0' && after === 'clicks: 1', `${before} -> ${after}`);

  await page.reload({ waitUntil: 'load' }); await sleep(700);
  const reloaded = await page.locator('button').first().textContent();
  check('counter: a view instance state belongs to its page (G3)',
    reloaded === 'clicks: 0', `after reload: ${reloaded}`);

  if (SHOTS) await page.screenshot({ path: `${SHOTS}/counter.png` });
  check('counter: the browser console is clean', errors.length === 0, errors.join(' | '));
  await ctx.close(); srv.kill();
}

// ---- slate: page titles, absolute-path assets, real co-editing -----------
{
  const srv = await serve('slate', 8402);
  const ctx = await browser.newContext();
  const a = await ctx.newPage();
  const errors = [];
  a.on('console', m => m.type() === 'error' && errors.push(m.text()));
  a.on('pageerror', e => errors.push(`pageerror: ${e.message}`));

  await a.goto('http://127.0.0.1:8402/', { waitUntil: 'load' });
  await sleep(700);
  check('slate: the index names its own tab (§9.4)', (await a.title()) === 'slate',
    JSON.stringify(await a.title()));

  const robots = await fetch('http://127.0.0.1:8402/robots.txt');
  check('slate: an absolute path is answerable (§9.8)',
    robots.status === 200 && robots.headers.get('content-type') === 'text/plain',
    `${robots.status} ${robots.headers.get('content-type')}`);

  // A real form post: urlencoded, not JSON, and nothing wrote client code.
  await a.locator('form input').first().fill('Rims Not Wheels');
  await Promise.all([a.waitForNavigation({ waitUntil: 'load' }), a.locator('form button').first().click()]);
  await sleep(900);
  check('slate: a native form post makes a pad',
    a.url().endsWith('/p/rims-not-wheels'), a.url());
  check('slate: the pad names its tab after itself',
    (await a.title()) === 'Rims Not Wheels · slate', JSON.stringify(await a.title()));

  // Two real browsers, typing at the same time, no client-side merge code.
  const b = await ctx.newPage();
  await b.goto(a.url(), { waitUntil: 'load' });
  await sleep(900);
  const ta = a.locator('textarea, input[type=text]').first();
  const tb = b.locator('textarea, input[type=text]').first();
  await ta.click(); await ta.fill(''); await a.keyboard.type('line one', { delay: 30 });
  await sleep(800);
  check('slate: what one page types appears on the other',
    (await tb.inputValue()).includes('line one'), JSON.stringify(await tb.inputValue()));

  await Promise.all([
    a.keyboard.type(' AAA', { delay: 20 }),
    (async () => { await tb.click(); await b.keyboard.type(' BBB', { delay: 20 }); })(),
  ]);
  await sleep(1500);
  const truth = await (await fetch(`http://127.0.0.1:8402/api/pad/rims-not-wheels`)).json();
  check('slate: two people typing at once both survive the merge',
    truth.body.includes('AAA') && truth.body.includes('BBB'), JSON.stringify(truth.body));

  if (SHOTS) await a.screenshot({ path: `${SHOTS}/slate-pad.png` });
  check('slate: two pages present, one per tab',
    truth.here.length === 2, JSON.stringify(truth.here));
  await b.close(); await sleep(1200);
  const alone = await (await fetch(`http://127.0.0.1:8402/api/pad/rims-not-wheels`)).json();
  check('slate: presence departs when a tab closes',
    alone.here.length === 1, JSON.stringify(alone.here));

  check('slate: the browser console is clean', errors.length === 0, errors.join(' | '));

  // A socket can die with neither end told. The page must notice on its own,
  // because a stale page that looks live is the failure this refuses.
  const liveAtRest = await a.evaluate(() => document.documentElement.hasAttribute('data-ash-offline'));
  check('slate: a live page is not marked offline', liveAtRest === false, String(liveAtRest));
  await ctx.setOffline(true);
  await sleep(55000);
  const noticed = await a.evaluate(() => document.documentElement.hasAttribute('data-ash-offline'));
  check('slate: a page cut off notices and says so (§9.5)', noticed === true,
    noticed ? 'watchdog fired' : 'still looks live');
  await ctx.setOffline(false);
  await sleep(4000);
  const recovered = await a.evaluate(() => document.documentElement.hasAttribute('data-ash-offline'));
  check('slate: it reconnects when the network returns', recovered === false, String(recovered));

  await ctx.close(); srv.kill();
}

await browser.close();
const failed = checks.filter(c => !c.pass);
console.log(`\n${checks.length - failed.length}/${checks.length} checks passed`);
process.exit(failed.length ? 1 : 0);
