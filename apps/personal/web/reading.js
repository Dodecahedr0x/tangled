import { el, field, button, empty, recordTitle, timeLabel, revealSelection } from './ui.js';

const MiB = 1024 * 1024;
const tagsFrom = value => [...new Set(value.split(',').map(tag => tag.trim()).filter(Boolean))];
const input = (value = '', attrs = {}) => el('input', { type: 'text', value, ...attrs });
const check = (checked = false) => el('input', { type: 'checkbox', checked });
const status = () => el('p', { class: 'inline-status', role: 'status', 'aria-live': 'polite' });
const say = (node, text) => node.replaceChildren(text);
const meta = text => el('p', { class: 'meta', text });
const heading = text => el('h3', { class: 'section-title', text });
const section = (title, ...children) => el('section', { class: 'stack' }, heading(title), ...children);
const options = (select, values, placeholder) => {
  const previous = select.value;
  select.replaceChildren(el('option', { value: '', text: placeholder }), ...values.map(([value, label]) => el('option', { value, text: label })));
  if (values.some(([value]) => value === previous)) select.value = previous;
};
const details = (title, ...children) => el('details', {}, el('summary', { text: title }), el('div', { class: 'stack' }, ...children));
const form = (children, submit) => el('form', { class: 'stack', onSubmit: event => { event.preventDefault(); submit(); } }, ...children);
const nativeOf = item => item.body?.imported || {};
const highlight = (list, key) => [...list.children].forEach(row => row.classList?.toggle('selected', row.getAttribute?.('data-item') === key));
const sourceTitle = item => nativeOf(item).object?.text?.slice(0, 100) || `${nativeOf(item).object?.kind || 'Source item'} ${nativeOf(item).object?.id || ''}`;
async function fileText(file, limit, label) {
  if (!file) throw new Error(`Choose ${label} first.`);
  if (file.size > limit) throw new Error(`${label} must be ${limit / MiB} MiB or smaller. Choose a smaller file.`);
  try { return new TextDecoder('utf-8', { fatal: true }).decode(await file.arrayBuffer()); }
  catch { throw new Error(`Could not read ${label} as UTF-8. Choose a UTF-8 export and try again.`); }
}
function safeLink(value) {
  try {
    const url = new URL(value);
    if (!['http:', 'https:'].includes(url.protocol) || url.username || url.password || /\s/.test(value)) return null;
    return el('a', { href: url.href, target: '_blank', rel: 'noopener noreferrer', text: 'Open source article' });
  } catch { return null; }
}

async function localSelectionKey(ctx, uri, collection) {
  const parts = typeof uri === 'string' && /^tangled:\/\/([^/\s?#]+)\/([^/\s?#]+)\/([^/\s?#%]+)$/.exec(uri);
  if (!parts || parts[2] !== collection || ['.', '..'].includes(parts[3])) throw new Error('This link does not identify a record for this reader. Open the correct record link.');
  const info = await ctx.api('info', {});
  if (parts[1] !== info.did) throw new Error('This record belongs to another owner. Open it in that owner’s Personal library.');
  return parts[3];
}
function requireLinkedRecord(item, uri, collection) {
  if (!item || item.uri !== uri || item.collection !== collection || item.rkey !== uri.split('/').pop() || item.deleted || item.tombstone || !item.body) {
    throw new Error('This record is unavailable or was deleted. Choose another item or check the link.');
  }
}
function copyLink(ctx, view, uri) {
  const copy = button('Copy link', () => ctx.run(copy, () => ctx.copyRecordLink(view, uri)));
  return copy;
}

// Bounded text extraction for a reader, not an HTML sanitizer. Never parsed into a live DOM.
function feedPlainText(html) {
  const bytes = new TextEncoder().encode(String(html).slice(0, 256 * 1024));
  const source = new TextDecoder().decode(bytes.subarray(0, 256 * 1024), { stream: true }), pieces = [];
  const paragraphs = new Set(['address', 'article', 'aside', 'blockquote', 'div', 'dl', 'fieldset', 'figcaption', 'figure', 'footer', 'form', 'h1', 'h2', 'h3', 'h4', 'h5', 'h6', 'header', 'hr', 'main', 'nav', 'ol', 'p', 'pre', 'section', 'table', 'ul']);
  const lines = new Set(['br', 'dt', 'dd', 'li', 'tr']);
  let index = 0, suppressed = '';
  while (index < source.length) {
    if (suppressed) {
      const close = new RegExp(`</${suppressed}(?=[\\s/>])`, 'ig'); close.lastIndex = index;
      const match = close.exec(source);
      if (!match) break;
      index = match.index; suppressed = '';
    }
    if (source[index] !== '<') {
      const end = source.indexOf('<', index);
      pieces.push(source.slice(index, end < 0 ? source.length : end).replace(/\s+/g, ' '));
      index = end < 0 ? source.length : end; continue;
    }
    if (source.startsWith('<!--', index)) {
      const end = source.indexOf('-->', index + 4);
      index = end < 0 ? source.length : end + 3; continue;
    }
    const tag = /^<\/?([a-z][a-z0-9:-]*)(?=[\s/>]|$)/i.exec(source.slice(index, index + 128));
    const declaration = /^<!|^<\?/.test(source.slice(index, index + 2));
    if (!tag && !declaration) { pieces.push('<'); index++; continue; }
    let end = index + (tag?.[0].length || 2), quote = '';
    for (; end < source.length; end++) {
      const char = source[end];
      if (quote) { if (char === quote) quote = ''; }
      else if (char === '"' || char === "'") quote = char;
      else if (char === '>') break;
    }
    if (end === source.length) break; // An unfinished tag/attribute cannot expose hidden trailing text.
    const name = tag?.[1].toLowerCase(), closing = source[index + 1] === '/';
    if (!closing && (name === 'script' || name === 'style')) suppressed = name;
    if (paragraphs.has(name)) pieces.push('\n\n');
    else if (lines.has(name)) pieces.push('\n');
    else if (name === 'td' || name === 'th') pieces.push(' ');
    index = end + 1;
  }
  const entities = { amp: '&', lt: '<', gt: '>', quot: '"', apos: "'", nbsp: ' ', copy: '©', reg: '®', trade: '™', ndash: '–', mdash: '—', hellip: '…', lsquo: '‘', rsquo: '’', ldquo: '“', rdquo: '”', bull: '•', middot: '·', euro: '€', pound: '£', yen: '¥', cent: '¢', times: '×', divide: '÷' };
  const latinNames = 'nbsp iexcl cent pound curren yen brvbar sect uml copy ordf laquo not shy reg macr deg plusmn sup2 sup3 acute micro para middot cedil sup1 ordm raquo frac14 frac12 frac34 iquest Agrave Aacute Acirc Atilde Auml Aring AElig Ccedil Egrave Eacute Ecirc Euml Igrave Iacute Icirc Iuml ETH Ntilde Ograve Oacute Ocirc Otilde Ouml times Oslash Ugrave Uacute Ucirc Uuml Yacute THORN szlig agrave aacute acirc atilde auml aring aelig ccedil egrave eacute ecirc euml igrave iacute icirc iuml eth ntilde ograve oacute ocirc otilde ouml divide oslash ugrave uacute ucirc uuml yacute thorn yuml';
  latinNames.split(' ').forEach((name, offset) => { entities[name] = String.fromCodePoint(160 + offset); });
  Object.assign(entities, { AMP: '&', LT: '<', GT: '>', QUOT: '"' });
  return pieces.join('').replace(/&(#x[0-9a-f]{1,6}|#[0-9]{1,7}|[a-z][a-z0-9]{1,31});/gi, (whole, entity) => {
    if (entity[0] !== '#') return Object.hasOwn(entities, entity) ? entities[entity] : whole;
    const hex = entity[1]?.toLowerCase() === 'x', point = Number.parseInt(entity.slice(hex ? 2 : 1), hex ? 16 : 10);
    return point > 0 && point <= 0x10ffff && !(point >= 0xd800 && point <= 0xdfff) ? String.fromCodePoint(point) : '\ufffd';
  }).replace(/[^\S\n]+/g, ' ').replace(/ *\n */g, '\n').replace(/\n{3,}/g, '\n\n').trim();
}

export async function mountFeeds(root, ctx) {
  const feedback = status(), subscriptions = el('div', { class: 'list' }), queue = el('div', { class: 'list' });
  const reader = el('article', { class: 'reader', 'aria-label': 'Feed reader' });
  const url = input('', { type: 'url', required: true }), title = input(), subTags = input();
  let editing = false, selected = null, queueGeneration = 0, readerGeneration = 0;
  const importSource = el('select'), file = input('', { type: 'file', accept: '.xml,.rss,.atom,application/xml,text/xml' });
  const source = el('select'), unread = check(true), starred = check(), filterTags = input();
  const queueStatus = status();
  const filters = () => ({ unread: unread.checked, ...(starred.checked ? { starred: true } : {}),
    ...(source.value ? { url: source.value } : {}), ...(filterTags.value.trim() ? { tags: tagsFrom(filterTags.value) } : {}), limit: 1000 });
  const clearEditor = () => { editing = false; url.disabled = false; url.value = ''; title.value = ''; subTags.value = ''; saveSub.textContent = 'Subscribe'; };
  const saveSubscription = async () => ctx.run(saveSub, async () => {
    if (!url.value.trim()) throw new Error('Enter the RSS or Atom subscription URL.');
    const result = await ctx.api('feeds', { action: 'subscribe', url: url.value.trim(), title: title.value.trim() || url.value.trim(), tags: tagsFrom(subTags.value) });
    clearEditor(); await loadSubscriptions(); importSource.value = result.body.url;
    await loadQueue(); ctx.notify('Subscription saved. Import a feed file to add articles.');
  });
  const saveSub = button('Subscribe', saveSubscription, 'primary');
  const importFile = button('Import feed file', () => ctx.run(importFile, async () => {
    if (!importSource.value) throw new Error('Choose a subscription for this feed file, or add one first.');
    const xml = await fileText(file.files?.[0], 2 * MiB, 'RSS or Atom file');
    const result = await ctx.api('feeds', { action: 'ingest', url: importSource.value, xml });
    await loadQueue(); ctx.notify(`Imported ${result.imported} articles; ${result.skipped} already present or repeated.`);
  }), 'primary');
  const exportOpml = button('Download subscriptions (OPML)', () => ctx.run(exportOpml, async () => {
    const result = await ctx.api('feeds', { action: 'opml' }); ctx.download('subscriptions.opml', result.opml, 'application/xml');
    ctx.notify('Subscription file downloaded.');
  }));
  const digest = button('Download reading digest', () => ctx.run(digest, async () => {
    const result = await ctx.api('feeds', { action: 'digest', ...filters() });
    ctx.download('reading-digest.txt', result.text, 'text/plain;charset=utf-8'); ctx.notify(`Downloaded a digest of ${result.count} articles.`);
  }));
  function renderReader(item) {
    readerGeneration++;
    selected = item;
    if (!item) { reader.replaceChildren(empty('Choose an article', 'Open an article from the reading queue.')); return; }
    highlight(queue, item.uri);
    const body = item.body, reading = item.reading || {};
    const itemTags = input((reading.tags || []).join(', '));
    const mark = async (node, patch) => ctx.run(node, async () => {
      await ctx.api('feeds', { action: 'mark', item: item.uri, ...patch });
      item.reading = { ...reading, ...patch }; renderReader(item); await loadQueue(); ctx.notify('Reading state saved.');
    });
    const read = button(reading.read ? 'Mark unread' : 'Mark read', () => mark(read, { read: !reading.read }));
    const star = button(reading.starred ? 'Unstar article' : 'Star article', () => mark(star, { starred: !reading.starred }));
    const saveTags = button('Save article tags', () => mark(saveTags, { tags: tagsFrom(itemTags.value) }));
    const link = safeLink(body.link);
    reader.replaceChildren(...[el('h2', { text: recordTitle(item) }), meta(body.sourceUrl),
      meta(`Published: ${body.publishedAt || 'Not supplied'}${body.updatedAt ? ` · Updated: ${body.updatedAt}` : ''}`),
      meta(`Imported locally: ${timeLabel(item.ts)}`),
      el('div', { class: 'actions' }, read, star, copyLink(ctx, 'feeds', item.uri), link),
      field('Article tags', itemTags, 'Comma-separated. Subscription tags also apply.'), saveTags,
      body.bodyType === 'html' ? meta('Plain-text reading view. The original XML remains preserved with the imported entry.') : null,
      el('div', { class: 'reader-body', text: (body.bodyType === 'html' ? feedPlainText(body.body || '') : body.body) || 'This source entry has no article text.' })].filter(Boolean));
  }
  async function openLinkedItem(uri) {
    const generation = ++readerGeneration;
    try {
      const rkey = await localSelectionKey(ctx, uri, 'pub.tangled.feedItem');
      if (generation !== readerGeneration) return;
      const [item, states] = await Promise.all([
        ctx.api('records', { action: 'get', collection: 'pub.tangled.feedItem', rkey }),
        ctx.api('records', { action: 'list', collection: 'pub.tangled.readingState' }),
      ]);
      if (generation !== readerGeneration) return;
      requireLinkedRecord(item, uri, 'pub.tangled.feedItem');
      const reading = states.records.find(row => row.body?.item === uri)?.body || {};
      renderReader({ ...item, reading });
    } catch (error) {
      if (generation !== readerGeneration) return;
      reader.replaceChildren(empty('Could not open linked article', `${error.message} Choose an article from the queue or reopen the link to retry.`));
      ctx.notify(error.message, 'error');
    }
  }
  async function loadQueue() {
    const generation = ++queueGeneration;
    say(queueStatus, 'Loading reading queue…');
    try {
      const result = await ctx.api('feeds', { action: 'queue', ...filters() });
      if (generation !== queueGeneration) return;
      queue.replaceChildren(...result.items.map(item => {
        const open = button(recordTitle(item), () => { renderReader(item); ctx.updateRecordLink?.('feeds', item.uri); revealSelection(reader); }); open.classList?.add('item-title');
        return el('div', { class: `item${selected?.uri === item.uri ? ' selected' : ''}`, 'data-item': item.uri },
          el('div', { class: 'item-main' }, open, meta(`${item.reading?.read ? 'Read' : 'Unread'}${item.reading?.starred ? ' · Starred' : ''} · ${item.body.sourceUrl}`),
            item.effectiveTags?.length ? meta(item.effectiveTags.join(', ')) : null));
      }));
      if (!result.items.length) queue.replaceChildren(empty('No articles in this queue', 'Import an RSS or Atom file, or change the reading filters.'));
      say(queueStatus, `${result.items.length} articles${result.items.length === 1000 ? ' (first 1,000; narrow the filters for more)' : ''}.`);
    } catch (error) { if (generation === queueGeneration) say(queueStatus, `Could not load the reading queue. ${error.message} Use Apply filters to retry.`); throw error; }
  }
  async function loadSubscriptions() {
    const result = await ctx.api('feeds', { action: 'list' });
    const rows = result.subscriptions, choices = rows.map(row => [row.body.url, `${row.body.title || row.body.url} — ${row.body.url}`]);
    options(importSource, choices, 'Choose subscription'); options(source, choices, 'All subscriptions');
    importFile.disabled = !rows.length;
    subscriptions.replaceChildren(...rows.map(row => {
      const body = row.body;
      const edit = button('Edit subscription', () => {
        editing = true; url.value = body.url; url.disabled = true; title.value = body.title || ''; subTags.value = (body.tags || []).join(', '); saveSub.textContent = 'Save subscription'; title.focus();
      });
      const remove = button('Unsubscribe', () => ctx.run(remove, async () => {
        await ctx.api('feeds', { action: 'unsubscribe', url: body.url });
        if (editing && url.value === body.url) clearEditor();
        await loadSubscriptions(); await loadQueue(); ctx.notify('Unsubscribed. Imported articles and reading history are retained.');
      }));
      return el('div', { class: 'item' }, el('div', { class: 'item-main' }, el('strong', { text: body.title || body.url }), meta(body.url), body.tags?.length ? meta(body.tags.join(', ')) : null), el('div', { class: 'actions' }, edit, remove));
    }));
    if (!rows.length) subscriptions.replaceChildren(empty('No subscriptions yet', 'Add a feed URL, then import its saved RSS or Atom XML file.'));
  }
  const apply = button('Apply filters', () => ctx.run(apply, loadQueue));
  const retry = button('Retry loading feeds', () => ctx.run(retry, load));
  async function load() {
    say(feedback, 'Loading feeds…');
    try { await loadSubscriptions(); await loadQueue(); say(feedback, ''); }
    catch (error) { say(feedback, `Could not load feeds. ${error.message}`); throw error; }
  }
  root.replaceChildren(el('div', { class: 'stack' },
    meta('Private reading from supplied RSS or Atom files. Feed URLs are identifiers; this app does not fetch or refresh them.'),
    feedback, details('Subscriptions and feed imports', subscriptions,
      form([field('Feed URL', url), field('Subscription title', title), field('Subscription tags', subTags, 'Comma-separated.'),
        el('div', { class: 'actions' }, saveSub, button('Clear subscription form', clearEditor))], saveSubscription),
      section('Import articles', field('Import into subscription', importSource, 'Choose the subscription whose URL matches the saved feed.'),
        field('RSS or Atom file', file, 'UTF-8 XML, up to 2 MiB and 1,000 entries.'), importFile), exportOpml),
    form([el('div', { class: 'form-grid' }, field('Subscription filter', source), field('Filter article tags', filterTags, 'Comma-separated; all tags must match.')),
      el('div', { class: 'actions' }, field('Unread only', unread), field('Starred only', starred), apply, digest)], () => ctx.run(apply, loadQueue)),
    queueStatus, el('div', { class: 'split' }, el('section', { class: 'pane', 'aria-label': 'Reading queue' }, queue), reader), retry));
  renderReader(null);
  await Promise.all([
    load().catch(error => ctx.notify(error.message, 'error')),
    ctx.selection?.uri ? openLinkedItem(ctx.selection.uri) : Promise.resolve(),
  ]);
}

export async function mountConnections(root, ctx) {
  const feedback = status(), listStatus = status(), list = el('div', { class: 'list' });
  const reader = el('article', { class: 'reader', 'aria-label': 'Connected source reader' });
  const activityFile = input('', { type: 'file', accept: '.json,application/json' }), importPreview = status();
  const query = input(), source = input(), account = input(), conversation = input(), tag = input(), bookmarked = check(), deleted = check();
  let activity = null, contacts = [], selectedKey = null, listGeneration = 0, readerGeneration = 0;
  let invalidateReply = () => {};
  const importButton = button('Import activity', () => ctx.run(importButton, async () => {
    if (!activity) throw new Error('Choose and review an activity.json file first.');
    const result = await ctx.api('connections', { action: 'ingest', activity });
    await loadList(); invalidateReply(); ctx.notify(`Imported or retained ${result.items.length} source items. No source account was contacted.`);
  }), 'primary'); importButton.disabled = true;
  activityFile.addEventListener('change', () => ctx.run(activityFile, async () => {
    activity = null; importButton.disabled = true; say(importPreview, 'Reading activity file…');
    try {
      const text = await fileText(activityFile.files?.[0], 8 * MiB, 'activity.json file');
      let parsed; try { parsed = JSON.parse(text); } catch { throw new Error('This file is not valid JSON. Choose the recovered activity.json file.'); }
      if (!Array.isArray(parsed) || parsed.length > 1000) throw new Error('Choose an activity.json array with no more than 1,000 entries.');
      if (new TextEncoder().encode(JSON.stringify(parsed)).length > 8 * MiB) throw new Error('Activity data must be 8 MiB or smaller. Split the export into smaller batches.');
      if (parsed.some(row => !row || typeof row.source !== 'string' || typeof row.account !== 'string' || !row.object || typeof row.object !== 'object')) throw new Error('This is not a recovery activity export. Choose activity.json, without recovery keys or account credentials.');
      const counts = new Map(); for (const row of parsed) counts.set(row.source, (counts.get(row.source) || 0) + 1);
      activity = parsed;
      say(importPreview, `${activityFile.files[0].name}: ${parsed.length} entries. Sources: ${[...counts].map(([name, count]) => `${name} (${count})`).join(', ') || 'none'}. Ready to import.`);
    } catch (error) { say(importPreview, error.message); throw error; }
    finally { importButton.disabled = !activity; }
  }));
  const filters = () => ({ action: query.value.trim() ? 'search' : 'list',
    ...Object.fromEntries([['query', query], ['source', source], ['account', account], ['conversation', conversation], ['tag', tag]].filter(([, node]) => node.value.trim()).map(([key, node]) => [key, node.value.trim()])),
    ...(bookmarked.checked ? { bookmarked: true } : {}), ...(deleted.checked ? { includeDeleted: true } : {}) });
  async function loadList() {
    const generation = ++listGeneration; say(listStatus, 'Loading source items…');
    try {
      const rows = await ctx.api('connections', filters()); if (generation !== listGeneration) return;
      list.replaceChildren(...rows.map(item => {
        const imported = nativeOf(item), object = imported.object || {};
        const open = button(sourceTitle(item), () => ctx.run(open, async () => { if (await openItem(item.rkey)) { ctx.updateRecordLink?.('connections', item.uri); revealSelection(reader); } }));
        return el('div', { class: `item${item.rkey === selectedKey ? ' selected' : ''}`, 'data-item': item.rkey }, el('div', { class: 'item-main' }, open,
          meta(`${imported.source} · ${object.sender || 'Sender not supplied'} · ${object.timestamp || 'Time not supplied'}`),
          meta(`Account: ${imported.account}${object.conversation ? ` · Conversation: ${object.conversation}` : ''}${item.body.deleted ? ' · Deleted at source' : ''}${item.overlay?.body?.bookmarked ? ' · Bookmarked' : ''}`)));
      }));
      if (!rows.length) list.replaceChildren(empty('No source items found', 'Import a recovery activity.json file, or clear the search filters.'));
      say(listStatus, `${rows.length} source items. Source timestamps are preserved; list order is local storage order.`);
    } catch (error) { if (generation === listGeneration) say(listStatus, `Could not load source items. ${error.message} Use Search items to retry.`); throw error; }
  }
  async function openItem(rkey, expectedUri, generation = ++readerGeneration) {
    const [item, history] = await Promise.all([ctx.api('connections', { action: 'get', rkey }), ctx.api('connections', { action: 'history', rkey })]);
    if (generation !== readerGeneration) return;
    if (expectedUri) requireLinkedRecord(item, expectedUri, 'pub.tangled.sourceItem');
    invalidateReply(); selectedKey = rkey;
    // `get` returns the native record only; retrieve the separate overlay through list.
    const rows = await ctx.api('connections', { action: 'list', includeDeleted: true });
    if (generation !== readerGeneration) return;
    const overlay = rows.find(row => row.rkey === rkey)?.overlay;
    const imported = nativeOf(item);
    const nativeContacts = imported.object?.sender ? await ctx.api('connections', { action: 'contacts', source: imported.source, account: imported.account, sender: imported.object.sender }) : [];
    if (generation !== readerGeneration) return;
    renderReader(item, overlay, history, nativeContacts[0]); highlight(list, item.rkey); return true;
  }
  async function openLinkedItem(uri) {
    const generation = ++readerGeneration;
    try {
      const rkey = await localSelectionKey(ctx, uri, 'pub.tangled.sourceItem');
      if (generation !== readerGeneration) return;
      await openItem(rkey, uri, generation);
    } catch (error) {
      if (generation !== readerGeneration) return;
      reader.replaceChildren(empty('Could not open linked source item', `${error.message} Search items or reopen the link to retry.`));
      ctx.notify(error.message, 'error');
    }
  }
  function renderReader(item, overlay, history, nativeContact) {
    const imported = nativeOf(item), object = imported.object || {}, state = overlay?.body || {};
    const note = el('textarea', { value: state.note || '', rows: 4 }), itemTags = input((state.tags || []).join(', ')), bookmark = check(!!state.bookmarked);
    const save = button('Save local organization', () => ctx.run(save, async () => {
      await ctx.api('connections', { action: 'overlay', rkey: item.rkey, patch: { bookmarked: bookmark.checked, tags: tagsFrom(itemTags.value), note: note.value } });
      invalidateReply(); await loadList(); ctx.notify('Local bookmark, tags and note saved. Source content is unchanged.');
    }));
    const revision = el('select', {}, history.map((entry, index) => el('option', { value: String(index), text: `Revision ${index + 1}${index === item.body.currentRevision ? ' (current)' : ''} · ${entry.imported?.object?.timestamp || 'Time not supplied'}${entry.imported?.object?.deleted ? ' · Deleted' : ''}` })));
    revision.value = String(item.body.currentRevision || 0);
    const selectRevision = button('Use selected source revision', () => ctx.run(selectRevision, async () => {
      await ctx.api('connections', { action: 'select-revision', rkey: item.rkey, index: Number(revision.value) });
      invalidateReply(); await loadList(); await openItem(item.rkey); ctx.notify('Source view updated to the selected revision. Other revisions are retained.');
    }));
    const downloadOriginal = (sourceOriginal, node) => ctx.run(node, async () => {
      const result = await ctx.api('connections', { action: 'original', rkey: item.rkey, index: Number(revision.value), ...(sourceOriginal ? { sourceOriginal: true } : {}) });
      ctx.download(sourceOriginal ? 'source-original.bin' : 'source-entry.json', new Uint8Array(result.bytes), result.mime || 'application/octet-stream'); ctx.notify('Selected revision original downloaded.');
    });
    const original = button('Download original entry', () => downloadOriginal(false, original));
    const sourceOriginal = button('Download original source bytes', () => downloadOriginal(true, sourceOriginal));
    const updateOriginal = () => { sourceOriginal.disabled = !history[Number(revision.value)]?.sourceOriginalBlob; };
    revision.addEventListener('change', updateOriginal); updateOriginal();
    const contact = nativeContact;
    const alias = input(contact?.body.alias || ''), groups = input((contact?.body.groups || []).join(', '));
    const saveContact = button('Save contact alias and groups', () => ctx.run(saveContact, async () => {
      const result = await ctx.api('connections', { action: 'contact', source: imported.source, account: imported.account, sender: object.sender, patch: { alias: alias.value.trim(), groups: tagsFrom(groups.value) } });
      contacts = [...contacts.filter(row => row.uri !== result.uri), result]; renderContacts(); ctx.notify('Local contact saved for this source, account and sender.');
    }));
    const replyText = el('textarea', { rows: 5, value: state.reply?.text || '' }), replyPreview = el('div', { class: 'stack' });
    let preview = null, draftGeneration = 0;
    const approve = check(), exportReply = button('Export approved reply', () => ctx.run(exportReply, async () => {
      if (!preview || !approve.checked || replyText.value !== preview.body.reply.text) throw new Error('Preview and approve the current reply text first.');
      const result = await ctx.api('connections', { action: 'reply-export', rkey: item.rkey, previewCid: preview.cid, confirmed: true });
      ctx.download('approved-reply.json', result); ctx.notify('Approved reply intent downloaded. Delivery: not-sent.');
    }), 'primary'); exportReply.disabled = true;
    const invalidate = () => { draftGeneration++; preview = null; approve.checked = false; exportReply.disabled = true; replyPreview.replaceChildren(meta('Review a new preview before approving an export.'), exportReply); };
    invalidateReply = invalidate;
    replyText.addEventListener('input', invalidate);
    approve.addEventListener('change', () => { exportReply.disabled = !preview || !approve.checked; });
    const previewButton = button('Preview reply', () => ctx.run(previewButton, async () => {
      if (!replyText.value.trim()) throw new Error('Write reply text before previewing.');
      invalidate(); const generation = draftGeneration;
      const result = await ctx.api('connections', { action: 'reply-preview', rkey: item.rkey, account: imported.account, recipient: object.sender, text: replyText.value });
      if (generation !== draftGeneration || selectedKey !== item.rkey) return;
      preview = result; const reply = result.body.reply;
      replyPreview.replaceChildren(heading('Review this reply'), meta(`Source: ${reply.source} · Sending account: ${reply.account} · Recipient: ${reply.recipient}`),
        meta(`Conversation: ${reply.conversation ?? 'None'} · In reply to: ${reply.inReplyTo} · Kind: ${reply.kind}`),
        el('div', { class: 'reader-body', text: reply.text }),
        details('Approval evidence', meta(`Source item: ${reply.targetUri}`), meta(`Source version: ${reply.targetCid}`), meta(`Preview version: ${result.cid}`)),
        field('I approve this exact reply for export', approve), meta('Export downloads a reply intent. It does not send a message.'), exportReply);
      ctx.notify('Reply preview ready for review. No message has been sent.');
    }));
    invalidate();
    reader.replaceChildren(...[el('h2', { text: sourceTitle(item) }), copyLink(ctx, 'connections', item.uri),
      meta(`Native sender: ${object.sender || 'Not supplied'} · Source: ${imported.source}`),
      meta(`Account: ${imported.account} · Conversation: ${object.conversation ?? 'None'}`),
      meta(`Source time: ${object.timestamp || 'Not supplied'} · Imported locally: ${timeLabel(item.ts)}`),
      item.body.deleted ? el('p', { class: 'danger-text', text: 'The selected source revision reports this item deleted.' }) : null,
      el('div', { class: 'reader-body', text: object.text || 'No source text supplied.' }),
      object.attachments?.length ? details('Source attachment references', meta('References are preserved as text; no attachment is downloaded.'), el('ul', {}, object.attachments.map(value => el('li', { text: value })))) : null,
      section('Local organization', field('Bookmark', bookmark), field('Item tags', itemTags, 'Comma-separated.'), field('Local note', note), save),
      object.sender ? details('Contact alias and groups', meta(`Native identity: ${imported.source} / ${imported.account} / ${object.sender}`), field('Contact alias', alias), field('Contact groups', groups, 'Comma-separated. Aliases do not merge source identities.'), saveContact) : null,
      details('Source revision history', field('Source revision', revision), el('div', { class: 'actions' }, selectRevision, original, sourceOriginal)),
      object.sender && !item.body.deleted ? section('Reply intent', meta(`From account ${imported.account} to native sender ${object.sender}. Nothing is sent by this app.`), field('Reply text', replyText), previewButton, replyPreview) : meta('Reply preview requires a native sender and a source item that is not deleted.')].filter(Boolean));
  }
  const contactList = el('div', { class: 'list' }), groupFilter = input();
  const findContacts = button('Filter contacts', () => ctx.run(findContacts, async () => {
    contacts = await ctx.api('connections', { action: 'contacts', ...(groupFilter.value.trim() ? { group: groupFilter.value.trim() } : {}) }); renderContacts();
  }));
  function renderContacts() {
    contactList.replaceChildren(...contacts.map(contact => {
      const body = contact.body, identity = body.identity;
      const showItems = button('Show contact items', () => ctx.run(showItems, async () => {
        query.value = ''; source.value = identity.source; account.value = identity.account; conversation.value = ''; tag.value = ''; bookmarked.checked = false; deleted.checked = false;
        const rows = await ctx.api('connections', { action: 'list', contact: contact.uri });
        list.replaceChildren(...rows.map(item => { const open = button(sourceTitle(item), () => ctx.run(open, async () => { if (await openItem(item.rkey)) { ctx.updateRecordLink?.('connections', item.uri); revealSelection(reader); } })); return el('div', { class: 'item', 'data-item': item.rkey }, open); }));
        say(listStatus, `${rows.length} items from ${body.alias || identity.sender}. Use Search items to return to filters.`);
      }));
      return el('div', { class: 'item' }, el('div', { class: 'item-main' }, el('strong', { text: body.alias || identity.sender }), meta(`${identity.source} · ${identity.account} · ${identity.sender}`), meta((body.groups || []).join(', '))), showItems);
    }));
    if (!contacts.length) contactList.replaceChildren(empty('No matching contacts', 'Open a source item to assign its sender a local alias and groups.'));
  }
  const search = button('Search items', () => ctx.run(search, loadList));
  const retry = button('Retry loading connections', () => ctx.run(retry, load));
  async function load() {
    say(feedback, 'Loading connections…');
    try { await loadList(); contacts = await ctx.api('connections', { action: 'contacts' }); renderContacts(); say(feedback, ''); }
    catch (error) { say(feedback, `Could not load connections. ${error.message}`); throw error; }
  }
  root.replaceChildren(el('div', { class: 'stack' },
    meta('Private organization of supplied source history. Native senders, source accounts and source times are retained. This app never sends to a connected source.'), feedback,
    details('Import recovery activity', field('Recovery activity.json file', activityFile, 'Choose activity.json only, without recovery keys or account credentials. Up to 8 MiB and 1,000 entries.'), importPreview, importButton),
    form([el('div', { class: 'form-grid' }, field('Search source text and local notes', query), field('Source filter', source), field('Account filter', account), field('Conversation filter', conversation), field('Item tag filter', tag)),
      el('div', { class: 'actions' }, field('Bookmarked only', bookmarked), field('Include source-deleted items', deleted), search)], () => ctx.run(search, loadList)),
    listStatus, el('div', { class: 'split' }, el('section', { class: 'pane', 'aria-label': 'Source items' }, list), reader),
    details('Contacts', form([field('Contact group filter', groupFilter), findContacts], () => ctx.run(findContacts, async () => { contacts = await ctx.api('connections', { action: 'contacts', ...(groupFilter.value.trim() ? { group: groupFilter.value.trim() } : {}) }); renderContacts(); })), contactList), retry));
  reader.replaceChildren(empty('Choose a source item', 'Read its native context, add local organization, or prepare a reply intent.'));
  await Promise.all([
    load().catch(error => ctx.notify(error.message, 'error')),
    ctx.selection?.uri ? openLinkedItem(ctx.selection.uri) : Promise.resolve(),
  ]);
}
