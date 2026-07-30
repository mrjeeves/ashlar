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
import { mkdirSync, writeFileSync } from 'node:fs';
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
    before === 'this window: 0' && after === 'this window: 1', `${before} -> ${after}`);

  await page.reload({ waitUntil: 'load' }); await sleep(700);
  const reloaded = await page.locator('button').first().textContent();
  check('counter: a view instance state belongs to its page (G3)',
    reloaded === 'this window: 0', `after reload: ${reloaded}`);

  // The second button is the same keyword on a singleton, so it is the
  // program's one value. A real second tab is the only way to see that the
  // two scopes differ — which is the whole reason the page carries both.
  const other = await ctx.newPage();
  await other.goto('http://127.0.0.1:8401/', { waitUntil: 'load' });
  await sleep(700);
  await page.locator('button.count').first().click();   // this tab's own
  await page.locator('button.all').click();             // everybody's
  await sleep(500);
  const mine = await page.locator('button.count').first().textContent();
  const theirs = await other.locator('button.all').textContent();
  const theirsOwn = await other.locator('button.count').first().textContent();
  check('counter: shared state crosses tabs, per-instance state does not',
    theirs === 'everyone: 1' && mine === 'this window: 1' && theirsOwn === 'this window: 0',
    `${mine} / ${theirs} / other's own ${theirsOwn}`);
  await other.close();

  if (SHOTS) await page.screenshot({ path: `${SHOTS}/counter.png` });
  check('counter: the browser console is clean', errors.length === 0, errors.join(' | '));
  await ctx.close(); srv.kill();
}

// ---- enclave: a file picker and a drop zone, both real -------------------
//
// This is the check the enclave was rewritten for. Sharing a file was
// `/share <path>` typed into the conversation — a command line wearing a
// chat's clothes — on the excuse that a browser cannot hand a server a path
// it can open. True, and beside the point: a picker hands over the FILE. What
// was actually missing was `multipart/form-data` in the runtime, which fell
// through to the JSON arm and handed the program `none`.
//
// Neither half can be checked without a browser. The picker needs a real file
// dialog; the drop needs a real DataTransfer. The mesh node is absent here and
// that is fine — what is under test is that the bytes leave the page, which is
// the shim's and the runtime's business, not the mesh's.
{
  const srv = await serve('enclave', 8405);
  const ctx = await browser.newContext();
  const page = await ctx.newPage();
  const posts = [];
  page.on('request', r => {
    if (r.method() === 'POST') {
      posts.push({ url: r.url(), type: r.headers()['content-type'] || '' });
    }
  });
  await page.goto('http://127.0.0.1:8405/', { waitUntil: 'load' });
  await sleep(900);

  const picker = page.locator('.room-add input[type=file]');
  check('enclave: the shelf carries a real file picker',
    await picker.count() === 1, `${await picker.count()} file input(s)`);

  const tmp = `${SHOTS || '/tmp'}/ashlar-picked.txt`;
  writeFileSync(tmp, 'the bytes a person picked\n');
  await picker.setInputFiles(tmp);
  await page.locator('.room-add button[type=submit]').click();
  await sleep(900);
  const picked = posts.find(p => p.url.endsWith('/mesh/share'));
  check('enclave: picking a file posts it as multipart, with no client code',
    !!picked && picked.type.startsWith('multipart/form-data'),
    picked ? picked.type.split(';')[0] : 'no POST');

  // And a drop does the same, without touching the picker or the button. The
  // shim treats a drop on a form that has a file input as choosing one, so
  // this costs the program nothing and needs no new attribute.
  posts.length = 0;
  await page.goto('http://127.0.0.1:8405/', { waitUntil: 'load' });
  await sleep(900);
  // Read the hover state back from the SAME call that set it: the drop below
  // submits the form, and a navigation takes any page global with it.
  const dragging = await page.evaluate(() => {
    const form = document.querySelector('.room-add');
    const dt = new DataTransfer();
    dt.items.add(new File(['x'], 'hover.txt', { type: 'text/plain' }));
    form.dispatchEvent(new DragEvent('dragover', { bubbles: true, dataTransfer: dt }));
    return form.hasAttribute('data-ash-dropping');
  });
  check('enclave: dragging over the form says so, for the stylesheet', dragging);
  await page.evaluate(() => {
    const form = document.querySelector('.room-add');
    const dt = new DataTransfer();
    dt.items.add(new File(['dropped bytes'], 'dropped.txt', { type: 'text/plain' }));
    form.dispatchEvent(new DragEvent('drop', { bubbles: true, dataTransfer: dt }));
  });
  await sleep(1200);
  // The drop navigated; come back to a live page before measuring it.
  await page.goto('http://127.0.0.1:8405/', { waitUntil: 'load' });
  await sleep(700);
  const dropped = posts.find(p => p.url.endsWith('/mesh/share'));
  check('enclave: dropping a file submits it, with nothing clicked',
    !!dropped && dropped.type.startsWith('multipart/form-data'),
    dropped ? dropped.type.split(';')[0] : 'no POST');

  // The conversation scrolls inside its pane rather than growing the page —
  // the composer stays where it is put. `min-height: 0`, measured.
  const geometry = await page.evaluate(() => {
    const de = document.documentElement;
    const speak = document.querySelector('.speak').getBoundingClientRect();
    return { grows: de.scrollHeight > de.clientHeight + 1, bottom: Math.round(speak.bottom), h: window.innerHeight };
  });
  check('enclave: the page does not grow and the composer stays put',
    !geometry.grows && Math.abs(geometry.bottom - geometry.h) <= 1,
    `page ${geometry.grows ? 'grows' : 'fits'}, composer at ${geometry.bottom} of ${geometry.h}`);

  if (SHOTS) await page.screenshot({ path: `${SHOTS}/enclave.png` });
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
  // Poll rather than sleep a guessed interval. Reconnect lands in about two
  // seconds, but a fixed window turns machine load into a red check, and a
  // check that fails for reasons unrelated to the claim teaches people to
  // re-run instead of to look.
  let recovered = true;
  for (let i = 0; i < 40; i++) {
    await sleep(500);
    recovered = await a.evaluate(() => document.documentElement.hasAttribute('data-ash-offline'));
    if (!recovered) break;
  }
  check('slate: it reconnects when the network returns', recovered === false, String(recovered));

  await ctx.close(); srv.kill();
}

// ---- slate: collaborator cursors, at the granularity the merge uses -----
{
  const srv = await serve('slate', 8403);
  const ctx = await browser.newContext();
  const a = await ctx.newPage();
  const ctx2 = await browser.newContext();
  const b2 = await ctx2.newPage();
  await a.goto('http://127.0.0.1:8403/p/welcome', { waitUntil: 'load' });
  await b2.goto('http://127.0.0.1:8403/p/welcome', { waitUntil: 'load' });
  await sleep(1200);
  const ta = p => p.locator('textarea').first();
  const share = async p => (await p.locator('.sharing').first().textContent().catch(() => '')).trim();

  await ta(a).click(); await ta(a).fill('');
  await a.keyboard.type('line one\nline two\nline three', { delay: 25 });
  await sleep(1200);
  await ta(b2).click(); await b2.keyboard.press('Control+Home');
  await b2.keyboard.type('X', { delay: 25 });
  await sleep(1200);
  check('slate: different lines, nobody is warned',
    (await share(a)) === '' && (await share(b2)) === '',
    `${JSON.stringify(await share(a))} / ${JSON.stringify(await share(b2))}`);

  await ta(b2).click(); await b2.keyboard.press('Control+End');
  await b2.keyboard.type('!', { delay: 25 });
  await sleep(1400);
  const sa = await share(a), sb = await share(b2);
  check('slate: two carets on one line, each page names the other',
    sa.includes('on your line') && sb.includes('on your line'), `${JSON.stringify(sa)} / ${JSON.stringify(sb)}`);

  // And the check that would have caught the leak: close the page and see
  // whether its caret leaves with it. The first version of these checks
  // never closed anything, so they passed 17/17 with a departed
  // collaborator still reported as being on your line.
  await ctx2.close();
  await sleep(1500);
  const afterClose = await share(a);
  check('slate: a departed page takes its caret with it',
    afterClose === '', JSON.stringify(afterClose));

  await ctx.close(); srv.kill();
}

await browser.close();
const failed = checks.filter(c => !c.pass);
console.log(`\n${checks.length - failed.length}/${checks.length} checks passed`);
process.exit(failed.length ? 1 : 0);
