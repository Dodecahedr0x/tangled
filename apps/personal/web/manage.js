import { el, field, button, empty, recordTitle, timeLabel } from './ui.js';

const p = text => el('p', { text });
const input = (type = 'text', attrs = {}) => el('input', { type, ...attrs });
const section = (title, ...children) => el('section', { class: 'pane stack' }, el('h2', { text: title }), ...children);
const check = (control, text) => el('label', { class: 'check' }, control, text);
const status = () => el('p', { class: 'inline-status', role: 'status', 'aria-live': 'polite' });
const formButton = text => el('button', { type: 'submit', class: 'button primary', text });
function message(node, text, error = false) {
  node.textContent = text;
  node.className = `inline-status${error ? ' danger-text' : ''}`;
  node.setAttribute('role', error ? 'alert' : 'status');
}
async function perform(ctx, control, notice, task) {
  await ctx.run(control, async () => {
    message(notice, 'Working…');
    try { await task(); }
    catch (error) { message(notice, error.message || 'Could not finish. Try again.', true); throw error; }
  });
}
function metadata(pairs) {
  return el('dl', {}, pairs.filter(([, value]) => value !== undefined && value !== null && value !== '').map(([label, value]) => [el('dt', { class: 'muted', text: label }), el('dd', { text: value })]));
}
function readable(value, depth = 0) {
  if (value == null) return p('None');
  if (typeof value !== 'object') return p(String(value));
  if (depth > 5) return p('Additional nested details retained in the signed record.');
  if (Array.isArray(value)) return el('ul', {}, value.map(item => el('li', {}, readable(item, depth + 1))));
  return el('dl', {}, Object.entries(value).map(([key, item]) => [el('dt', { class: 'muted', text: key }), el('dd', {}, readable(item, depth + 1))]));
}
function citationList(citations = []) {
  return el('ul', {}, citations.map(item => el('li', {}, el('span', { text: item.uri || 'Record' }), el('p', { class: 'muted', text: item.cid || '' }))));
}

export async function mountAgents(root, ctx) {
  const grantList = el('div', { class: 'list' });
  const activityList = el('div', { class: 'list' });
  const notice = status();
  const collectionOptions = el('datalist', { id: 'grant-collections' });
  const audience = input('text', { required: true, placeholder: 'did:sol:…', spellcheck: false });
  const collection = input('text', { required: true, placeholder: 'pub.tangled.note', spellcheck: false });
  collection.setAttribute('list', 'grant-collections');
  const access = el('select', {}, el('option', { value: 'read', text: 'Read this collection' }), el('option', { value: 'write', text: 'Write new records to this collection' }));
  const ttl = input('number', { required: true, value: '3600', min: 1, max: 86400, step: 1 });
  const issue = formButton('Issue grant');
  const issued = el('div', { class: 'stack' });
  async function load() {
    const [grants, activity, revocations, records] = await Promise.all([
      ctx.api('agents', { action: 'list' }), ctx.api('agents', { action: 'report' }),
      ctx.api('records', { action: 'list', collection: 'sys.revocation' }), ctx.api('records', { action: 'list' }),
    ]);
    collectionOptions.replaceChildren(...[...new Set((records.records || []).map(record => record.collection))].filter(name => name?.startsWith('pub.tangled.') && !name.startsWith('pub.tangled.agent.')).sort().map(name => el('option', { value: name })));
    const revoked = new Map((revocations.records || []).map(record => [record.body?.cid, record]));
    grantList.replaceChildren(...(grants.grants || []).map(record => {
      const body = record.body || {};
      const cap = body.capability || {};
      const revocation = revoked.get(body.cid);
      const state = revocation ? 'Revoked' : cap.exp && cap.exp * 1000 <= Date.now() ? 'Expired' : 'Active';
      const reason = input('text', { placeholder: 'Task finished', maxLength: 4096 });
      const revokeStatus = status();
      const revoke = button('Revoke grant', () => perform(ctx, revoke, revokeStatus, async () => {
        await ctx.api('agents', { action: 'revoke', cid: body.cid, reason: reason.value.trim() || 'owner revoked' });
        ctx.notify('Grant revoked. Future requests using this certificate will be blocked.'); await load();
      }), 'danger');
      const details = el('details', { class: 'detail' }, el('summary', { text: 'Certificate and revocation' }), metadata([
        ['Certificate', body.cid], ['Signed grant', record.uri], ['Issued', cap.iat ? timeLabel(cap.iat * 1000) : timeLabel(record.ts)],
        ['Revocation reason', revocation?.body?.reason],
      ]), button('Download certificate', () => ctx.download('agent-certificate.json', { cid: body.cid, capability: cap })));
      if (!revocation) details.append(field('Reason for revoking', reason), revoke, revokeStatus);
      return el('article', { class: 'item' }, el('div', { class: 'item-main' }, el('h3', { class: 'item-title', text: cap.aud || 'Agent grant' }),
        el('div', { class: 'meta' }, el('span', { class: 'badge', text: state }), el('span', { text: cap.cmd === 'repo/write' ? 'Write new records' : 'Read' })),
        p(cap.res || record.collection), p(`Expires ${cap.exp ? timeLabel(cap.exp * 1000) : 'at the certificate’s stated time'}`), details));
    }));
    if (!grantList.childNodes.length) grantList.append(empty('No app access yet', 'Issue a limited grant for an app or agent that has its own identity.'));
    activityList.replaceChildren(...(activity.activities || []).map(record => {
      const body = record.body || {};
      return el('article', { class: 'item' }, el('div', { class: 'item-main' }, el('h3', { class: 'item-title', text: body.action === 'write' ? 'Wrote a new record' : 'Searched a collection' }),
        el('p', { class: 'muted', text: `${body.actor || 'Agent'} · ${timeLabel(record.ts)}` }),
        metadata([['Collection', body.request?.collection], ['Search', body.request?.query]]),
        el('details', { class: 'detail' }, el('summary', { text: 'Request and cited results' }), readable(body.request || {}), citationList(body.citations),
          metadata([['Capability certificate', body.capability], ['Signed invocation', body.invocation], ['Signed receipt', record.uri], ['Receipt CID', record.cid]]))));
    }));
    if (!activityList.childNodes.length) activityList.append(empty('No delegated activity', 'Successful searches and writes will appear here with the records they used or created.'));
  }
  const form = el('form', { class: 'stack', onSubmit: event => {
    event.preventDefault();
    return perform(ctx, issue, notice, async () => {
      const result = await ctx.api('agents', { action: 'grant', audience: audience.value.trim(), collection: collection.value.trim(), access: access.value, ttl_seconds: Number(ttl.value) });
      issued.replaceChildren(p('Grant issued. Give this certificate to the intended app or agent.'), button('Download new certificate', () => ctx.download('agent-certificate.json', { cid: result.cid, capability: result.capability })));
      message(notice, 'Grant issued.'); ctx.notify('Scoped grant issued.'); await load();
    });
  } }, field('App or agent DID', audience, 'The app creates its own identity. Never give it your owner keys or data directory.'), field('Collection', collection, 'One collection per grant. Reading and writing require separate grants.'), collectionOptions,
  field('Access', access), field('Expires after (seconds)', ttl, 'Choose 1 to 86,400 seconds (up to 24 hours).'), issue, notice, issued);
  const refresh = button('Refresh access and activity', () => perform(ctx, refresh, notice, async () => { await load(); message(notice, 'Access and activity refreshed.'); }));
  root.replaceChildren(el('div', { class: 'toolbar' }, refresh), el('div', { class: 'split' }, section('Give limited access', form), el('div', { class: 'stack' }, section('Issued grants', grantList), section('Activity receipts', activityList))));
  try { await load(); } catch (error) { message(notice, `${error.message} Use Refresh access and activity to retry.`, true); }
}

export async function mountDevices(root, ctx) {
  const summary = el('div');
  const notice = status();
  const backupPassword = input('password', { required: true, autoComplete: 'new-password', minLength: 12, maxLength: 1024 });
  const confirmation = input('password', { required: true, autoComplete: 'new-password', minLength: 12, maxLength: 1024 });
  const openPassword = input('password', { required: true, autoComplete: 'off', maxLength: 1024 });
  const file = input('file', { accept: '.json,application/json', required: true });
  const label = input('text', { value: 'Recovered library', maxLength: 80 });
  const restoreConfirm = input('checkbox');
  const syncConfirm = input('checkbox');
  const clearPasswords = () => { backupPassword.value = ''; confirmation.value = ''; openPassword.value = ''; };
  async function refresh() {
    const current = await ctx.api('devices', { action: 'status' });
    summary.replaceChildren(metadata([['Owner', current.owner], ['Signed records', current.records], ['Local attachments and schemas', current.blobs], ['Current signed head', current.head || 'No saved records yet']]));
  }
  function passphrase(control) {
    const bytes = new TextEncoder().encode(control.value).length;
    if (bytes < 12 || bytes > 1024) throw new Error('Use a recovery passphrase of 12–1,024 UTF-8 bytes. Enter it again to retry.');
    return control.value;
  }
  async function readBackup() {
    const chosen = file.files?.[0];
    if (!chosen) throw new Error('Choose an encrypted recovery JSON file first.');
    if (chosen.size > 128 * 1024 * 1024 + 4096) throw new Error('This file is too large. Choose a recovery file up to 128 MiB plus 4 KiB.');
    let backup;
    try { backup = JSON.parse(await chosen.text()); } catch { throw new Error('Could not read this recovery JSON file. Choose the original encrypted backup and retry.'); }
    if (!backup || Array.isArray(backup) || typeof backup !== 'object') throw new Error('Choose an encrypted recovery JSON object.');
    return backup;
  }
  const create = formButton('Download encrypted recovery copy');
  const createForm = el('form', { class: 'stack', onSubmit: event => {
    event.preventDefault();
    if (importBusy) return;
    importBusy = true;
    return perform(ctx, create, notice, async () => {
      controls.filter(item => item !== create).forEach(item => { item.disabled = true; });
      try {
        const secret = passphrase(backupPassword);
        if (secret !== confirmation.value) throw new Error('The passphrases do not match. Enter both again.');
        const result = await ctx.request('backups/export', { passphrase: secret });
        ctx.download(result.filename || 'personal-recovery.json', result.backup);
        message(notice, 'Encrypted recovery copy downloaded. Keep the file and passphrase separately.'); ctx.notify('Encrypted recovery copy downloaded.');
      } finally { clearPasswords(); controls.filter(item => item !== create).forEach(item => { item.disabled = false; }); }
    }).finally(() => { importBusy = false; });
  } }, p('This copy contains your full owner keys, signed library and local attachment bytes. Anyone who unlocks it can act as you. Keep it somewhere private, separately from its passphrase.'),
  field('New recovery passphrase', backupPassword, 'Use a separate strong passphrase, at least 12 characters. It cannot be recovered for you.'), field('Confirm recovery passphrase', confirmation), create);
  const importNotice = status();
  let importBusy = false;
  const controls = [];
  async function importAction(action, control) {
    if (importBusy) return;
    importBusy = true;
    try {
      await perform(ctx, control, importNotice, async () => {
        controls.filter(item => item !== control).forEach(item => { item.disabled = true; });
        try {
          if (action === 'restore' && !restoreConfirm.checked) throw new Error('Confirm that you want a fresh full-owner copy before restoring.');
          if (action === 'sync' && !syncConfirm.checked) throw new Error('Confirm that you want to import this newer signed history before syncing.');
          if (action === 'restore' && !label.value.trim()) throw new Error('Name the fresh recovered library.');
          const secret = passphrase(openPassword);
          const backup = await readBackup();
          const result = await ctx.request(`backups/${action}`, { passphrase: secret, backup, ...(action === 'restore' ? { label: label.value.trim() } : {}) });
          clearPasswords();
          if (action === 'restore') {
            await ctx.refreshSession();
            const location = result.session?.data || result.restored?.destination || result.restored?.data || result.restored?.path || ctx.session?.data;
            ctx.notify(`Fresh owner copy restored${location ? ` at ${location}` : ''}. This service now uses it. On restart, launch Personal with this recovered directory to continue there. Your previous library was not overwritten.`, 'info');
            ctx.navigate('library');
          } else {
            message(importNotice, action === 'verify' ? 'Recovery copy verified: decryption, owner identity, signed history and attachments are valid. No library was changed.' : 'Newer owner copy imported. The library now has the verified signed history and attachments.');
            if (action === 'sync') { await refresh(); ctx.notify('Newer recovery copy synced.'); }
          }
        } finally { clearPasswords(); controls.filter(item => item !== control).forEach(item => { item.disabled = false; }); }
      });
    } finally { importBusy = false; }
  }
  const verify = button('Verify recovery copy', () => importAction('verify', verify));
  const restore = button('Restore fresh library', () => importAction('restore', restore), 'primary');
  const sync = button('Sync newer copy', () => importAction('sync', sync));
  controls.push(verify, restore, sync, create);
  const refreshButton = button('Refresh recovery status', () => perform(ctx, refreshButton, notice, async () => { await refresh(); message(notice, 'Recovery status refreshed.'); }));
  root.replaceChildren(el('div', { class: 'stack' }, section('This owner library', summary, refreshButton, notice),
    el('div', { class: 'split' }, section('Create a recovery copy', createForm), section('Open an encrypted recovery copy',
      p('Choose a file from this computer. Verification leaves your library unchanged. Each restored installation holds the same full owner keys; individual copies cannot be revoked.'),
      field('Encrypted recovery file', file), field('Recovery passphrase', openPassword, 'Enter the passphrase again for each action. Password fields are cleared after every attempt.'), verify,
      el('div', { class: 'detail stack' }, el('h3', { text: 'Restore into a fresh library' }), p('Creates a separate private repository and switches this running service to it after verification. Existing libraries are never overwritten.'), field('Recovered library name', label), check(restoreConfirm, 'Create a fresh full-owner copy and switch to it'), restore),
      el('div', { class: 'detail stack' }, el('h3', { text: 'Bring this library up to date' }), p('Imports a newer copy of this same owner. Stale, divergent or different-owner history is rejected. Write on one installation at a time; sync before writing on another.'), check(syncConfirm, 'Import this copy’s verified newer history into the current library'), sync), importNotice))));
  try { await refresh(); } catch (error) { message(notice, `${error.message} Use Refresh recovery status to retry.`, true); }
}

function sourceHints(body) {
  const found = [];
  function walk(value, path, depth) {
    if (depth > 4 || found.length >= 30) return;
    if (typeof value === 'string' && (/blob|cid|source|url|link/i.test(path))) found.push([path, value]);
    else if (value && typeof value === 'object') Object.entries(value).forEach(([key, item]) => walk(item, path ? `${path}.${key}` : key, depth + 1));
  }
  walk(body, '', 0); return found;
}

export async function mountPublish(root, ctx) {
  const list = el('div', { class: 'list' });
  const notice = status();
  const preview = el('div', { class: 'reader' }, empty('Choose content to preview', 'Select individual records, then confirm the selected material can be shared.'));
  const filter = input('search', { placeholder: 'Find by title or collection' });
  const collectionFilter = el('select', {}, el('option', { value: '', text: 'All collections' }));
  const title = input('text', { value: 'Selected reading', maxLength: 4096 });
  const description = el('textarea', { rows: 3, maxLength: 16384 });
  const siteUrl = input('url', { placeholder: 'https://example.org/reading/' });
  const share = input('checkbox');
  const key = input('text', { required: true, placeholder: 'reading-list', maxLength: 256, spellcheck: false });
  const finalConfirm = input('checkbox', { disabled: true });
  const count = status();
  const signedResult = el('div');
  const publish = button('Create or update public publication', () => perform(ctx, publish, notice, async () => {
    if (!snapshot || !finalConfirm.checked) throw new Error('Preview your current selection, then confirm the separate public copy.');
    if (!key.value.trim()) throw new Error('Enter a stable publication name.');
    const result = await ctx.api('publish', { ...snapshot, action: 'publish', key: key.value.trim(), confirmPublish: true });
    finalConfirm.checked = false;
    const record = result.publication;
    signedResult.replaceChildren(p('Public copy saved. Source records remain private according to their original access.'), metadata([['Publication', record?.uri], ['Publication CID', record?.cid]]));
    message(notice, 'Public publication saved locally. Download the generated files to share or host them yourself.'); ctx.notify('Public copy saved locally.');
  }).finally(() => { publish.disabled = !snapshot || !finalConfirm.checked; }), 'primary');
  publish.disabled = true;
  let records = [], selected = new Map(), options = new Map(), snapshot = null, revision = 0;
  function invalidate() {
    revision++;
    snapshot = null; finalConfirm.checked = false; finalConfirm.disabled = true; publish.disabled = true; signedResult.replaceChildren();
    preview.replaceChildren(empty('Preview needed', 'Your selection or sharing details changed. Generate a new preview before publishing.'));
    count.textContent = `${selected.size} selected (maximum 256)`;
  }
  function recordOptions(record) {
    if (options.has(record.uri)) return options.get(record.uri);
    const link = input('url', { placeholder: 'https://…' });
    const textBlob = input('text', { placeholder: 'Content CID', spellcheck: false });
    const cid = input('text', { placeholder: 'Attachment CID', spellcheck: false });
    const name = input('text', { placeholder: 'Original attachment', maxLength: 4096 });
    const attachments = [], attachmentList = el('div', { class: 'stack' });
    const state = { link, textBlob, attachments };
    const add = button('Add this attachment', () => {
      if (!cid.value.trim()) { ctx.notify('Enter an attachment CID to add it.', 'error'); return; }
      if (attachments.length >= 32) { ctx.notify('Each item can include at most 32 attachments.', 'error'); return; }
      const item = { cid: cid.value.trim(), name: name.value.trim() || 'Attachment' };
      attachments.push(item); cid.value = ''; name.value = ''; invalidate();
      const row = el('div', { class: 'item' }, el('div', { class: 'item-main' }, p(item.name), el('p', { class: 'muted', text: item.cid })), button('Remove attachment', () => { attachments.splice(attachments.indexOf(item), 1); row.remove(); invalidate(); }));
      attachmentList.append(row);
    });
    link.addEventListener('input', invalidate); textBlob.addEventListener('input', invalidate);
    const hints = sourceHints(record.body || {});
    state.node = el('details', { class: 'detail' }, el('summary', { text: 'Optional links, text and attachments' }),
      p('Only the plain title, text and description are copied by default. Original source fields below are private unless you explicitly add a link, text blob or attachment.'),
      hints.length ? metadata(hints) : p('No source or attachment fields found. You can still supply a known local attachment CID.'),
      field('Include this web link', link, 'Optional HTTP or HTTPS link. No network request is made.'),
      field('Use this UTF-8 text blob', textBlob, 'Optional. Replaces inline text with these exact local bytes.'),
      field('Attachment CID to include', cid), field('Attachment download name', name), add, attachmentList);
    options.set(record.uri, state); return state;
  }
  function renderList() {
    const query = filter.value.trim().toLowerCase();
    const shown = records.filter(record => (!collectionFilter.value || record.collection === collectionFilter.value) && `${recordTitle(record)} ${record.collection}`.toLowerCase().includes(query));
    list.replaceChildren(...shown.map(record => {
      const chosen = input('checkbox', { checked: selected.has(record.uri), 'aria-label': `Select ${recordTitle(record)} from ${record.collection}` });
      chosen.addEventListener('change', () => {
        if (chosen.checked && selected.size >= 256) { chosen.checked = false; ctx.notify('Choose at most 256 records for one publication.', 'error'); return; }
        if (chosen.checked) selected.set(record.uri, record); else selected.delete(record.uri);
        invalidate();
      });
      const details = recordOptions(record);
      return el('article', { class: 'item' }, chosen, el('div', { class: 'item-main' }, el('h3', { class: 'item-title', text: recordTitle(record) }),
        el('div', { class: 'meta' }, el('span', { text: record.collection }), el('span', { text: ['public', 'pub'].includes(record.vis) ? 'Public source' : 'Private source' })),
        el('p', { text: typeof record.body?.text === 'string' ? record.body.text.slice(0, 220) : typeof record.body?.description === 'string' ? record.body.description.slice(0, 220) : 'Open optional details to inspect source fields.' }), details.node));
    }));
    if (!shown.length) list.append(empty(records.length ? 'No matching records' : 'No records to share', records.length ? 'Try another title or collection.' : 'Save content to your library before creating a selected public copy.', !records.length ? button('Open library', () => ctx.navigate('library')) : null));
    count.textContent = `${selected.size} selected (maximum 256)`;
  }
  const render = formButton('Preview selected content');
  const previewForm = el('form', { class: 'stack', onSubmit: event => {
    event.preventDefault();
    return perform(ctx, render, notice, async () => {
      if (!selected.size) throw new Error('Select at least one record from the list.');
      if (!share.checked) throw new Error('Check “Share these selected items” before generating a sharing preview.');
      const request = { action: 'render', title: title.value, description: description.value, ...(siteUrl.value.trim() ? { siteUrl: siteUrl.value.trim() } : {}), confirmPublish: true,
        records: [...selected.values()].map(record => {
          const extra = options.get(record.uri);
          return { collection: record.collection, rkey: record.rkey, ...(extra?.link.value.trim() ? { link: extra.link.value.trim() } : {}), ...(extra?.textBlob.value.trim() ? { textBlob: extra.textBlob.value.trim() } : {}), ...(extra?.attachments.length ? { attachments: extra.attachments.map(item => ({ ...item })) } : {}) };
        }) };
      const requestRevision = revision;
      const result = await ctx.api('publish', request);
      // An input may change while rendering. Require a new preview for that change.
      if (revision !== requestRevision) { message(notice, 'Selection changed during preview. Generate the preview again.'); return; }
      snapshot = request;
      finalConfirm.checked = false; finalConfirm.disabled = false; publish.disabled = true;
      const downloads = el('div', { class: 'actions' });
      const files = result.files?.length ? result.files : [{ path: 'index.html', mediaType: 'text/html', text: result.html }, { path: 'feed.xml', mediaType: 'application/rss+xml', text: result.rss }];
      files.forEach(file => downloads.append(button(`Download ${file.path}`, () => ctx.download(file.path.split('/').pop(), file.bytes ? new Uint8Array(file.bytes) : file.text || '', file.mediaType || 'application/octet-stream'))));
      preview.replaceChildren(el('h2', { text: 'Sharing preview' }), p(`${result.count ?? selected.size} selected items. The files below contain the copied content and selected attachments.`),
        el('iframe', { title: 'Public sharing preview', sandbox: '', referrerPolicy: 'no-referrer', srcdoc: result.html || '' }), downloads);
      message(notice, 'Preview ready. Review it before downloading or creating a public publication.');
    });
  } }, field('Sharing title', title), field('Description', description), field('Hosted site URL', siteUrl, 'Optional. Gives RSS readers your website address when you host these files.'),
    check(share, 'Share these selected items'), p('This preview and its downloads copy selected private text and attachments into shareable files. Copies already shared cannot be recalled.'), render);
  [title, description, siteUrl].forEach(control => control.addEventListener('input', invalidate));
  share.addEventListener('change', invalidate);
  filter.addEventListener('input', renderList); collectionFilter.addEventListener('change', renderList);
  key.addEventListener('input', () => { finalConfirm.checked = false; publish.disabled = true; });
  finalConfirm.addEventListener('change', () => { publish.disabled = !snapshot || !finalConfirm.checked; });
  async function load() {
    const result = await ctx.api('records', { action: 'list' });
    records = (result.records || []).filter(record => !record.deleted && !record.tombstone);
    selected.clear(); options.clear(); revision++; invalidate();
    collectionFilter.replaceChildren(el('option', { value: '', text: 'All collections' }), ...[...new Set(records.map(record => record.collection))].sort().map(value => el('option', { value, text: value })));
    renderList(); message(notice, 'Choose individual records below. Nothing is shared until you explicitly create sharing files or a public copy.');
  }
  const refresh = button('Refresh records', () => perform(ctx, refresh, notice, load));
  root.replaceChildren(el('div', { class: 'stack' }, el('div', { class: 'toolbar' }, field('Find records', filter), field('Collection', collectionFilter), refresh), count,
    el('div', { class: 'split' }, section('Choose records', list), el('div', { class: 'stack' }, section('Prepare sharing files', previewForm, notice), preview,
      section('Save a public signed copy', p('A stable name creates or updates one public publication in this local repository. Your sources keep their original access. This does not host a website or send files.'), field('Stable publication name', key),
        check(finalConfirm, 'I reviewed this preview. Create or update a separate public copy under this name.'), publish, signedResult)))));
  try { await load(); } catch (error) { message(notice, `${error.message} Use Refresh records to retry.`, true); }
}
