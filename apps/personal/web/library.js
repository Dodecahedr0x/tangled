import { el, field, button, empty, recordTitle, timeLabel, revealSelection } from './ui.js';

const MAX_FILE = 16 * 1024 * 1024;
const rows = value => Array.isArray(value) ? value : (value?.records || []);
const keyOf = record => record.rkey || record.uri?.split('/').pop();
const tagsOf = value => [...new Set(value.split(',').map(tag => tag.trim()).filter(Boolean))];
const tagsText = body => Array.isArray(body?.tags) ? body.tags.join(', ') : '';
const newKey = () => globalThis.crypto.randomUUID();
const input = (value = '', attrs = {}) => el('input', { type: 'text', value, ...attrs });
const textarea = (value = '', attrs = {}) => el('textarea', { rows: 7, value, ...attrs });
const heading = text => el('h2', { class: 'section-title', text });
const hint = text => el('p', { class: 'hint', text });
const replace = (node, ...children) => node.replaceChildren(...children.flat(Infinity).filter(child => child != null && child !== false));
const status = () => el('p', { class: 'inline-status', role: 'status', 'aria-live': 'polite' });
const select = (options, value = '') => el('select', {}, options.map(([key, label]) => el('option', { value: key, text: label, selected: key === value })));
const submit = label => el('button', { type: 'submit', class: 'button primary', text: label });
const humanCollection = collection => ({ 'pub.tangled.capture': 'Saved item', 'pub.tangled.note': 'Note', 'pub.tangled.annotation': 'Annotation', 'pub.tangled.feedItem': 'Feed item', 'pub.tangled.sourceItem': 'Connected item' })[collection] || collection || 'Record';
const plainBody = body => ['text', 'content', 'description', 'summary', 'excerpt', 'quote', 'caption', 'transcript', 'ocrText'].map(name => typeof body?.[name] === 'string' ? body[name] : '').filter(Boolean).join('\n\n');

function run(ctx, control, message, task) {
  return ctx.run(control, async () => {
    message.textContent = '';
    try { await task(); }
    catch (error) {
      message.textContent = `${error.message || error} Check the fields and try again.`;
      throw error;
    }
  });
}

function formWith(control, message, ctx, task, ...children) {
  return el('form', { class: 'stack', onSubmit: event => {
    event.preventDefault();
    if (!control.disabled) void run(ctx, control, message, task);
  } }, ...children, el('div', { class: 'actions' }, control), message);
}

function reference(record) {
  return el('details', { class: 'detail' }, el('summary', { text: 'Record reference' }),
    el('p', { class: 'meta', text: record.uri || '' }),
    record.cid ? el('p', { class: 'meta', text: `Revision: ${record.cid}` }) : null);
}

function externalLink(url) {
  try {
    const parsed = new URL(url);
    if (['http:', 'https:'].includes(parsed.protocol)) return el('a', { href: parsed.href, target: '_blank', rel: 'noopener noreferrer', text: parsed.href });
  } catch { /* Non-web source references remain inert text. */ }
  return url ? el('span', { class: 'meta', text: url }) : null;
}

function itemRow(record, selected, open, detail = '') {
  return el('li', { class: `item${selected ? ' selected' : ''}` },
    el('div', { class: 'item-main' }, button(recordTitle(record), open),
      detail ? el('p', { class: 'meta', text: detail }) : null));
}

async function fileBytes(file) {
  if (!file) throw new Error('Choose a file first.');
  if (file.size > MAX_FILE) throw new Error('Choose a file smaller than 16 MiB.');
  return Array.from(new Uint8Array(await file.arrayBuffer()));
}

function localDate(seconds) {
  if (seconds === undefined || seconds === null) return '';
  const date = new Date(seconds * 1000);
  if (!Number.isFinite(date.getTime())) return '';
  return new Date(date.getTime() - date.getTimezoneOffset() * 60000).toISOString().slice(0, 16);
}

function reminderValue(control) {
  if (!control.value) return null;
  const seconds = Math.floor(new Date(control.value).getTime() / 1000);
  if (!Number.isFinite(seconds) || seconds < 0) throw new Error('Choose a valid reminder date.');
  return seconds;
}

export async function mountLibrary(root, ctx) {
  let searches = [], groups = [], annotations = [], current = null, results = [], total = 0, offset = 0, revision = 0;
  let selectedSearch = '', selectedGroup = null;
  const query = input('', { type: 'search', placeholder: 'Search your library' });
  const collection = input('', { placeholder: 'Any record type' });
  const source = input('', { placeholder: 'Any source' });
  const tag = input();
  const group = select([['', 'All collections']]);
  const searchStatus = status(), detailStatus = status(), savedStatus = status(), groupStatus = status();
  const searchButton = submit('Search');
  const resultList = el('ul', { class: 'list' });
  const resultSummary = el('p', { class: 'meta' });
  const pagination = el('div', { class: 'actions' });
  const reader = el('section', { class: 'pane reader', 'aria-label': 'Selected library record' }, empty('Choose a result', 'Open a record to read, annotate, or add it to a collection.'));
  const savedPicker = select([['', 'New saved search']]);
  const savedName = input('', { required: true, maxLength: 200 });
  const saveSearchButton = submit('Save search');
  const groupPicker = select([['', 'New collection']]);
  const groupName = input('', { required: true, maxLength: 200 });
  const groupDescription = textarea('', { rows: 2 });
  const groupTags = input();
  const groupItems = el('ul', { class: 'list' });
  const saveGroupButton = submit('Save collection');
  const groupDelete = button('Delete collection', () => void run(ctx, groupDelete, groupStatus, async () => {
    if (!selectedGroup) return;
    const key = keyOf(selectedGroup);
    await ctx.api('memory', { action: 'delete-collection', rkey: key });
    if (group.value === key) group.value = '';
    selectedGroup = null;
    groupName.value = groupDescription.value = groupTags.value = '';
    await loadOrganizers(); await search();
    ctx.notify('Collection deleted. Its records are still in your library.');
  }), 'danger');
  groupDelete.disabled = true;
  const filters = () => ({ query: query.value, collection: collection.value.trim(), source: source.value.trim(), tag: tag.value.trim(), group: group.value, limit: 30, offset });

  function applySearch(record) {
    const body = record.body;
    for (const [control, name] of [[query, 'query'], [collection, 'collection'], [source, 'source'], [tag, 'tag'], [group, 'group']]) control.value = body[name] || '';
    offset = 0;
    selectedSearch = keyOf(record);
    savedPicker.value = selectedSearch;
    savedName.value = body.name || '';
  }

  function fillGroup(record) {
    selectedGroup = record || null;
    groupPicker.value = record ? keyOf(record) : '';
    groupName.value = record?.body.name || '';
    groupDescription.value = record?.body.description || '';
    groupTags.value = tagsText(record?.body);
    groupDelete.disabled = !record;
    replace(groupItems, );
    if (!record) return;
    const items = record.body.items || [];
    if (!items.length) groupItems.append(el('li', { class: 'meta', text: 'No records yet. Open a search result and add it here.' }));
    items.forEach((item, index) => {
      const known = results.find(result => result.uri === item.uri);
      const remove = button('Remove', () => void run(ctx, remove, groupStatus, async () => {
        const remaining = (selectedGroup.body.items || []).filter((_, itemIndex) => itemIndex !== index);
        selectedGroup = await ctx.api('memory', { action: 'put-collection', rkey: keyOf(selectedGroup), items: remaining });
        await loadOrganizers();
        await search();
      }));
      groupItems.append(el('li', { class: 'item' }, el('div', { class: 'item-main' }, el('p', { text: known?.title || item.uri.split('/').slice(-2).join(' / ') }), reference(item)), remove));
    });
  }

  async function loadOrganizers() {
    const [saved, collections, notes] = await Promise.all([
      ctx.api('memory', { action: 'list-searches' }), ctx.api('memory', { action: 'list-collections' }),
      ctx.api('records', { action: 'list', collection: 'pub.tangled.annotation' })
    ]);
    searches = rows(saved); groups = rows(collections); annotations = rows(notes);
    const filterKey = group.value;
    replace(group, el('option', { value: '', text: 'All collections' }), ...groups.map(record => el('option', { value: keyOf(record), text: recordTitle(record) })));
    group.value = groups.some(record => keyOf(record) === filterKey) ? filterKey : '';
    replace(savedPicker, el('option', { value: '', text: 'New saved search' }), ...searches.map(record => el('option', { value: keyOf(record), text: recordTitle(record) })));
    savedPicker.value = searches.some(record => keyOf(record) === selectedSearch) ? selectedSearch : '';
    replace(groupPicker, el('option', { value: '', text: 'New collection' }), ...groups.map(record => el('option', { value: keyOf(record), text: recordTitle(record) })));
    fillGroup(groups.find(record => keyOf(record) === keyOf(selectedGroup || {})));
    if (current) renderReader(current);
  }

  async function openResult(hit, reveal = false) {
    const ticket = ++revision;
    replace(reader, hint('Opening record…'));
    detailStatus.textContent = '';
    try {
      const record = await ctx.api('records', { action: 'get', collection: hit.collection, rkey: keyOf(hit) });
      let text = plainBody(record.body);
      if (record.collection === 'pub.tangled.capture') text = (await ctx.api('capture', { action: 'read', uri: record.uri })).text;
      if (ticket !== revision) return;
      current = { record, text };
      renderReader(current);
      if (reveal) revealSelection(reader);
    } catch (error) {
      if (ticket !== revision) return;
      current = null;
      replace(reader, empty('Could not open this record', error.message || String(error), button('Try again', () => void openResult(hit))));
    }
  }

  function renderReader({ record, text }) {
    const body = record.body || {};
    const addGroup = select(groups.map(record => [keyOf(record), recordTitle(record)]));
    const add = button('Add to collection', () => void run(ctx, add, detailStatus, async () => {
      const chosen = groups.find(item => keyOf(item) === addGroup.value);
      if (!chosen) throw new Error('Create a collection first.');
      const items = chosen.body.items || [];
      if (items.some(item => item.uri === record.uri)) { ctx.notify('This record is already in that collection.', 'info'); return; }
      await ctx.api('memory', { action: 'put-collection', rkey: keyOf(chosen), items: [...items, { uri: record.uri, cid: record.cid }] });
      await loadOrganizers(); ctx.notify('Added to collection.');
    }));
    add.disabled = !groups.length;
    const annotationList = el('div', { class: 'stack' });
    const annotationText = textarea('', { required: true, maxLength: 32768 });
    const quote = textarea('', { rows: 2 });
    const annotationTags = input();
    const annotationStatus = status();
    const annotationSave = submit('Save annotation');
    const annotationTitle = heading('Add an annotation');
    let editing = null;
    const cancel = button('New annotation', () => {
      editing = null; annotationText.value = quote.value = annotationTags.value = '';
      annotationTitle.textContent = 'Add an annotation'; annotationText.focus();
    });
    for (const note of annotations.filter(item => item.body.target?.uri === record.uri)) {
      const edit = button('Edit annotation', () => {
        editing = note; annotationText.value = note.body.text || ''; quote.value = note.body.quote || '';
        annotationTags.value = tagsText(note.body); annotationTitle.textContent = 'Edit annotation'; annotationText.focus();
      });
      const remove = button('Delete annotation', () => void run(ctx, remove, annotationStatus, async () => {
        await ctx.api('memory', { action: 'delete-annotation', rkey: keyOf(note) });
        await loadOrganizers(); await search(); ctx.notify('Annotation deleted.');
      }), 'danger');
      annotationList.append(el('section', { class: 'detail' }, el('p', { text: note.body.text }),
        note.body.quote ? el('blockquote', { text: note.body.quote }) : null,
        tagsText(note.body) ? el('p', { class: 'meta', text: tagsText(note.body) }) : null, el('div', { class: 'actions' }, edit, remove)));
    }
    const annotationForm = formWith(annotationSave, annotationStatus, ctx, async () => {
      await ctx.api('memory', { action: 'annotate', rkey: editing ? keyOf(editing) : newKey(), text: annotationText.value,
        quote: quote.value, tags: tagsOf(annotationTags.value), target: editing?.body.target || { uri: record.uri, cid: record.cid } });
      await loadOrganizers(); await search(); ctx.notify('Annotation saved.');
    }, annotationTitle, field('Annotation', annotationText), field('Quoted passage (optional)', quote), field('Annotation tags', annotationTags, 'Separate tags with commas.'), cancel);
    replace(reader, heading(recordTitle(record)), el('p', { class: 'meta', text: humanCollection(record.collection) }),
      body.url ? externalLink(body.url) : null,
      el('div', { class: 'reader-body', text: text || 'This record has no readable text. Open its original workflow to manage attachments or structured content.' }),
      reference(record), el('div', { class: 'form-grid' }, field('Collection', addGroup), el('div', { class: 'actions' }, add)),
      !groups.length ? hint('Create a collection below to organize this record.') : null,
      detailStatus, annotationList, annotationForm);
  }

  async function search() {
    searchStatus.textContent = 'Searching…';
    try {
      const response = await ctx.api('memory', { action: 'search', ...filters() });
      results = response.results || []; total = response.total || 0;
      resultSummary.textContent = `${total} ${total === 1 ? 'record' : 'records'}${total ? ` · ${offset + 1}–${Math.min(offset + results.length, total)}` : ''}`;
      replace(resultList, ...(results.length ? results.map(hit => el('li', { class: `item${current?.record.uri === hit.uri ? ' selected' : ''}` },
        el('div', { class: 'item-main' }, button(hit.title || hit.snippet?.slice(0, 90) || humanCollection(hit.collection), () => void openResult(hit, true)),
          el('p', { class: 'meta', text: humanCollection(hit.collection) }), el('p', { text: hit.snippet || '' })))) : [empty('No matching records', 'Try fewer words or clear a filter. Save content in Capture or write a note to begin.')]));
      const previous = button('Previous', () => void run(ctx, previous, searchStatus, async () => { offset = Math.max(0, offset - 30); await search(); }));
      const next = button('Next', () => void run(ctx, next, searchStatus, async () => { offset += 30; await search(); }));
      previous.disabled = offset === 0; next.disabled = offset + results.length >= total;
      replace(pagination, ...(total > 30 ? [previous, next] : []));
      searchStatus.textContent = '';
    } catch (error) { searchStatus.textContent = `${error.message || error} Try searching again.`; throw error; }
  }

  savedPicker.addEventListener('change', () => {
    const saved = searches.find(record => keyOf(record) === savedPicker.value);
    if (saved) applySearch(saved);
    else { selectedSearch = ''; savedName.value = ''; }
  });
  groupPicker.addEventListener('change', () => fillGroup(groups.find(record => keyOf(record) === groupPicker.value)));
  const runSaved = button('Run saved search', () => void run(ctx, runSaved, savedStatus, async () => {
    const saved = searches.find(record => keyOf(record) === savedPicker.value);
    if (!saved) throw new Error('Choose a saved search first.');
    applySearch(saved); await search();
  }));
  const deleteSaved = button('Delete saved search', () => void run(ctx, deleteSaved, savedStatus, async () => {
    if (!selectedSearch) throw new Error('Choose a saved search first.');
    await ctx.api('memory', { action: 'delete-search', rkey: selectedSearch });
    selectedSearch = ''; savedName.value = ''; await loadOrganizers(); ctx.notify('Saved search deleted.');
  }), 'danger');
  replace(root, el('div', { class: 'stack' },
    formWith(searchButton, searchStatus, ctx, async () => { offset = 0; await search(); },
      field('Search your library', query), el('details', { class: 'detail' }, el('summary', { text: 'Search filters' }),
        el('div', { class: 'form-grid' }, field('Record type', collection, 'Exact collection name, for example pub.tangled.note. Leave blank for all types.'), field('Source', source, 'Exact source name, URL, or author.'), field('Tag', tag), field('Collection', group)))),
    el('div', { class: 'split' }, el('section', { class: 'pane', 'aria-label': 'Search results' }, resultSummary, resultList, pagination), reader),
    el('details', { class: 'detail' }, el('summary', { text: 'Saved searches' }),
      formWith(saveSearchButton, savedStatus, ctx, async () => {
        selectedSearch = selectedSearch || newKey();
        await ctx.api('memory', { action: 'save-search', rkey: selectedSearch, name: savedName.value.trim(), ...filters(), offset: 0 });
        await loadOrganizers(); ctx.notify('Search saved with the current filters.');
      }, field('Saved search', savedPicker), field('Search name', savedName), hint('Save the query and filters above. Results are recomputed when you run the search.'), el('div', { class: 'actions' }, runSaved, deleteSaved))),
    el('details', { class: 'detail' }, el('summary', { text: 'Manage collections' }),
      formWith(saveGroupButton, groupStatus, ctx, async () => {
        selectedGroup = await ctx.api('memory', { action: 'put-collection', rkey: selectedGroup ? keyOf(selectedGroup) : newKey(), name: groupName.value.trim(), description: groupDescription.value, tags: tagsOf(groupTags.value) });
        await loadOrganizers(); ctx.notify('Collection saved. Open a result to add records.');
      }, field('Collection to edit', groupPicker), field('Collection name', groupName), field('Description', groupDescription), field('Collection tags', groupTags, 'Separate tags with commas.'), groupItems, groupDelete))));
  searchStatus.textContent = 'Loading your library…';
  try { await loadOrganizers(); await search(); }
  catch (error) { searchStatus.textContent = `${error.message || error} Reload this view to try again.`; ctx.notify(error.message || String(error), 'error'); }
}

export async function mountCapture(root, ctx) {
  let records = [], selected = null, currentText = '', readerError = '', readerTicket = 0;
  const title = input('', { maxLength: 4096 });
  const kind = select([['bookmark', 'Web link'], ['article', 'Article text'], ['text', 'Text'], ['file', 'File'], ['screenshot', 'Screenshot']]);
  const url = input('', { type: 'url', placeholder: 'https://example.org/article' });
  const text = textarea('', { rows: 8 });
  const tags = input();
  const file = input('', { type: 'file' });
  const urlField = field('Original URL', url), textField = field('Saved text', text), fileField = field('Choose a file', file, 'Up to 16 MiB. Files stay private; original bytes are available to download.');
  const saveStatus = status(), listStatus = status(), readerStatus = status();
  const save = submit('Save item');
  const readFilter = select([['', 'All reading states'], ['unread', 'Unread'], ['read', 'Read']]);
  const starFilter = select([['', 'All items'], ['starred', 'Starred only']]);
  const tagFilter = input();
  const list = el('ul', { class: 'list' });
  const count = el('p', { class: 'meta' });
  const reader = el('section', { class: 'pane reader', 'aria-label': 'Saved item reader' }, empty('Choose a saved item', 'Your saved text is available offline. Web links open the original page.'));

  function updateKind() {
    const isFile = ['file', 'screenshot'].includes(kind.value);
    urlField.hidden = !['bookmark', 'article'].includes(kind.value);
    textField.hidden = !['text', 'article'].includes(kind.value);
    fileField.hidden = !isFile;
    url.required = kind.value === 'bookmark';
    text.required = !textField.hidden;
    file.required = isFile;
    file.accept = kind.value === 'screenshot' ? 'image/*' : '';
  }
  kind.addEventListener('change', updateKind); updateKind();

  async function loadList() {
    listStatus.textContent = 'Loading saved items…';
    const filters = { action: 'list' };
    if (readFilter.value) filters.read = readFilter.value === 'read';
    if (starFilter.value) filters.starred = true;
    if (tagFilter.value.trim()) filters.tag = tagFilter.value.trim();
    records = rows(await ctx.api('capture', filters));
    count.textContent = `${records.length} saved ${records.length === 1 ? 'item' : 'items'}`;
    replace(list, ...(records.length ? records.map(record => itemRow(record, selected?.uri === record.uri, () => void open(record.uri, true),
      [record.body.kind, record.body.read ? 'Read' : 'Unread', record.body.starred ? 'Starred' : '', tagsText(record.body)].filter(Boolean).join(' · '))) : [empty('No saved items', 'Save a web link, supplied article text, a file, or a screenshot above. Clear filters to see everything.')]));
    listStatus.textContent = '';
  }

  async function open(uri, reveal = false) {
    const ticket = ++readerTicket;
    replace(reader, hint('Opening saved item…'));
    try {
      const [recordResult, textResult] = await Promise.allSettled([ctx.api('capture', { action: 'get', uri }), ctx.api('capture', { action: 'read', uri })]);
      if (ticket !== readerTicket) return;
      if (recordResult.status === 'rejected') throw recordResult.reason;
      selected = recordResult.value;
      currentText = textResult.status === 'fulfilled' ? textResult.value.text || '' : '';
      readerError = textResult.status === 'rejected' ? `Saved text could not be displayed: ${textResult.reason.message || textResult.reason}. Download the original to open it.` : '';
      renderReader();
      for (const row of list.children) row.classList.remove('selected');
      const index = records.findIndex(record => record.uri === uri);
      if (index >= 0) list.children[index]?.classList.add('selected');
      if (reveal) revealSelection(reader);
    } catch (error) {
      if (ticket === readerTicket) replace(reader, empty('Could not read this saved item', error.message || String(error), button('Try again', () => void open(uri))));
    }
  }

  function renderReader() {
    const record = selected, body = record.body;
    const read = button(body.read ? 'Mark unread' : 'Mark read', () => void run(ctx, read, readerStatus, async () => {
      await ctx.api('capture', { action: body.read ? 'mark-unread' : 'mark-read', uri: record.uri });
      await loadList(); await open(record.uri);
    }));
    const star = button(body.starred ? 'Remove star' : 'Star item', () => void run(ctx, star, readerStatus, async () => {
      await ctx.api('capture', { action: 'star', uri: record.uri, starred: !body.starred });
      await loadList(); await open(record.uri);
    }));
    const download = button('Download original', () => void run(ctx, download, readerStatus, async () => {
      const data = await ctx.api('capture', { action: 'download', uri: record.uri });
      const fallback = body.kind === 'screenshot' ? 'screenshot' : body.kind === 'file' ? 'attachment' : 'saved-item.txt';
      ctx.download(body.title || fallback, new Uint8Array(data.bytes), data.mime || 'application/octet-stream');
    }));
    const editTitle = input(body.title || '', { maxLength: 4096 });
    const editTags = input(tagsText(body));
    const editable = ['text', 'article'].includes(body.kind);
    const editText = textarea(editable ? currentText : '');
    const editStatus = status(), editSave = submit('Save changes');
    const remove = button('Delete saved item', () => void run(ctx, remove, readerStatus, async () => {
      await ctx.api('capture', { action: 'delete', uri: record.uri });
      selected = null; ++readerTicket;
      replace(reader, empty('Saved item deleted', 'It no longer appears in your saved list. Earlier signed revisions may remain in your repository.'));
      await loadList(); ctx.notify('Saved item deleted.');
    }), 'danger');
    readerStatus.textContent = '';
    replace(reader, heading(recordTitle(record)), el('p', { class: 'meta', text: [body.kind, body.read ? 'Read' : 'Unread', tagsText(body)].filter(Boolean).join(' · ') }),
      body.url ? externalLink(body.url) : null,
      el('div', { class: 'actions' }, read, star, download), readerStatus,
      el('div', { class: 'reader-body', text: readerError || currentText || (body.kind === 'bookmark' ? 'This is a saved link. Open the original URL to read the page.' : 'Download the original file to open it in a suitable application.') }),
      el('details', { class: 'detail' }, el('summary', { text: 'Edit saved item' }),
        formWith(editSave, editStatus, ctx, async () => {
          const patch = { title: editTitle.value, tags: tagsOf(editTags.value) };
          if (editable) patch.text = editText.value;
          await ctx.api('capture', { action: 'update', uri: record.uri, patch });
          await loadList(); await open(record.uri); ctx.notify('Saved item updated.');
        }, field('Title', editTitle), field('Tags', editTags, 'Separate tags with commas.'), editable ? field('Saved text', editText) : null)),
      reference(record), el('div', { class: 'actions' }, remove));
  }

  const filterButton = submit('Apply filters');
  const importFile = input('', { type: 'file', accept: 'application/json,.json' });
  const importStatus = status(), importButton = submit('Import browser capture');
  replace(root, el('div', { class: 'stack' },
    el('details', { open: true, class: 'detail' }, el('summary', { text: 'Save something' }),
      formWith(save, saveStatus, ctx, async () => {
        const payload = { action: 'save', kind: kind.value, ...(title.value ? { title: title.value } : {}), tags: tagsOf(tags.value) };
        if (!urlField.hidden && url.value) payload.url = url.value.trim();
        if (!textField.hidden) payload.text = text.value;
        if (!fileField.hidden) {
          const selectedFile = file.files?.[0];
          payload.bytes = await fileBytes(selectedFile);
          payload.mime = selectedFile.type || 'application/octet-stream';
          if (!payload.title) payload.title = selectedFile.name;
          if (kind.value === 'screenshot' && !payload.mime.startsWith('image/')) throw new Error('Choose an image file for a screenshot.');
        }
        const record = await ctx.api('capture', payload);
        title.value = url.value = text.value = tags.value = file.value = '';
        await loadList(); await open(record.uri); ctx.notify('Item saved privately.');
      }, hint('Save supplied content privately. Article text must be pasted here; saving a URL does not fetch its page.'),
        el('div', { class: 'form-grid' }, field('What are you saving?', kind), field('Title (optional)', title)), urlField, textField, fileField, field('Tags', tags, 'Separate tags with commas.'))),
    el('details', { class: 'detail' }, el('summary', { text: 'Import a browser capture' }),
      formWith(importButton, importStatus, ctx, async () => {
        const chosen = importFile.files?.[0];
        if (!chosen) throw new Error('Choose a capture file from the browser helper.');
        if (chosen.size > 24 * 1024 * 1024) throw new Error('This capture file is too large.');
        let payload;
        try { payload = JSON.parse(await chosen.text()); } catch { throw new Error('This is not a valid browser capture file.'); }
        if (!payload || Array.isArray(payload) || typeof payload !== 'object') throw new Error('Choose a browser capture object.');
        const record = await ctx.api('capture', { ...payload, action: 'save' });
        importFile.value = ''; await loadList(); await open(record.uri); ctx.notify('Browser capture imported.');
      }, field('Browser capture file', importFile), hint('Use a file downloaded by the Save to Tangled browser helper.'))),
    formWith(filterButton, listStatus, ctx, loadList, el('div', { class: 'form-grid' }, field('Reading state', readFilter), field('Stars', starFilter), field('Tag', tagFilter))),
    el('div', { class: 'split' }, el('section', { class: 'pane', 'aria-label': 'Saved items' }, count, list), reader)));
  try { await loadList(); }
  catch (error) { listStatus.textContent = `${error.message || error} Use Apply filters to try again.`; ctx.notify(error.message || String(error), 'error'); }
}

export async function mountNotes(root, ctx) {
  let records = [], selected = null, sources = [];
  const stateFilter = select([['', 'All notes'], ['note', 'Notes'], ['draft', 'Drafts'], ['inbox', 'Self inbox'], ['due', 'Due reminders']]);
  const query = input('', { type: 'search', placeholder: 'Find a note' });
  const list = el('ul', { class: 'list' }), count = el('p', { class: 'meta' }), listStatus = status();
  const editor = el('section', { class: 'pane reader', 'aria-label': 'Note editor' });
  const convertArea = el('div', { class: 'stack' });
  const loadButton = submit('Apply filters');

  function renderList() {
    const needle = query.value.trim().toLocaleLowerCase();
    const filtered = records.filter(record => `${recordTitle(record)} ${record.body.text || ''}`.toLocaleLowerCase().includes(needle));
    count.textContent = `${filtered.length} ${filtered.length === 1 ? 'note' : 'notes'}`;
    replace(list, ...(filtered.length ? filtered.map(record => itemRow(record, selected?.uri === record.uri, () => { selected = record; renderEditor(record); renderList(); revealSelection(editor); },
      [record.body.state, record.body.reminderAt != null ? `Reminder: ${timeLabel(record.body.reminderAt * 1000)}` : '', record.body.attachments?.length ? `${record.body.attachments.length} attachments` : ''].filter(Boolean).join(' · '))) : [empty('No notes here yet', 'Write a note, keep a draft, or capture an idea in your self inbox. Clear filters to see other notes.')]));
  }

  async function loadList() {
    listStatus.textContent = 'Loading notes…';
    const payload = { action: stateFilter.value === 'due' ? 'due' : 'list' };
    if (stateFilter.value && stateFilter.value !== 'due') payload.state = stateFilter.value;
    records = rows(await ctx.api('notes', payload));
    renderList(); listStatus.textContent = '';
  }

  function renderEditor(record = null, initialState = 'note', draft = null) {
    selected = record;
    const body = record?.body || {};
    const title = input(draft?.title ?? body.title ?? '', { maxLength: 512 });
    const text = textarea(draft?.text ?? body.text ?? '', { rows: 12 });
    const state = select([['note', 'Note'], ['draft', 'Draft'], ['inbox', 'Self inbox']], draft?.state || body.state || initialState);
    const reminder = input(draft?.reminder ?? localDate(body.reminderAt), { type: 'datetime-local' });
    const snapshot = () => ({ title: title.value, text: text.value, state: state.value, reminder: reminder.value });
    const preserveDraft = updated => { const pending = snapshot(); renderEditor(updated, 'note', pending); renderList(); };
    const noteStatus = status(), save = submit(record ? 'Save note' : 'Create note');
    const attachments = el('div', { class: 'stack' });
    const attachFile = input('', { type: 'file' });
    const attachStatus = status(), attach = submit('Attach file');
    const appendText = textarea('', { rows: 3, required: true });
    const appendStatus = status(), append = submit('Append to note');
    const clearReminder = button('Clear reminder', () => { reminder.value = ''; reminder.focus(); });
    const remove = button('Delete note', () => void run(ctx, remove, noteStatus, async () => {
      await ctx.api('notes', { action: 'delete', rkey: keyOf(record) });
      selected = null; await loadList(); renderEditor(); ctx.notify('Note deleted. Earlier signed revisions may remain in the repository.');
    }), 'danger');
    for (const [index, attachment] of (body.attachments || []).entries()) {
      const download = button('Download', () => void run(ctx, download, attachStatus, async () => {
        const data = await ctx.api('notes', { action: 'attachment', rkey: keyOf(record), index });
        ctx.download(data.name || 'attachment', new Uint8Array(data.bytes), data.mime || 'application/octet-stream');
      }));
      const removeAttachment = button('Remove attachment', () => void run(ctx, removeAttachment, attachStatus, async () => {
        const updated = await ctx.api('notes', { action: 'edit', rkey: keyOf(record), patch: { attachments: body.attachments.filter((_, itemIndex) => itemIndex !== index) } });
        await loadList(); preserveDraft(updated); ctx.notify('Attachment removed from this note.');
      }));
      attachments.append(el('div', { class: 'item' }, el('div', { class: 'item-main' }, el('p', { text: attachment.name || 'Attachment' }), el('p', { class: 'meta', text: attachment.mime })), el('div', { class: 'actions' }, download, removeAttachment)));
    }
    replace(editor, heading(record ? 'Edit note' : 'New note'),
      formWith(save, noteStatus, ctx, async () => {
        const patch = { text: text.value, title: title.value || null, state: state.value, reminderAt: reminderValue(reminder) };
        const createBody = Object.fromEntries(Object.entries(patch).filter(([, value]) => value !== null));
        const payload = record ? { action: 'edit', rkey: keyOf(record), patch } : { action: state.value === 'inbox' ? 'capture' : 'create', body: createBody };
        const updated = await ctx.api('notes', payload);
        await loadList(); renderEditor(updated); renderList(); ctx.notify(record ? 'Note saved.' : 'Note created privately.');
      }, field('Title (optional)', title), field('Text', text), el('div', { class: 'form-grid' }, field('Keep as', state), field('Reminder', reminder, 'Reminders appear in Due reminders when this app is opened; no background notification is sent.')), clearReminder),
      record ? el('details', { class: 'detail' }, el('summary', { text: 'Append text' }), formWith(append, appendStatus, ctx, async () => {
        const pending = snapshot();
        const updated = await ctx.api('notes', { action: 'append', rkey: keyOf(record), text: appendText.value });
        if (pending.text !== body.text) pending.text = `${pending.text}${pending.text ? '\n' : ''}${appendText.value}`;
        else pending.text = updated.body.text;
        await loadList(); renderEditor(updated, 'note', pending); renderList(); ctx.notify('Text appended.');
      }, field('Text to append', appendText, 'Added after the existing text on a new line.'))) : null,
      record ? el('details', { class: 'detail' }, el('summary', { text: `Attachments (${body.attachments?.length || 0})` }), attachments,
        formWith(attach, attachStatus, ctx, async () => {
          const chosen = attachFile.files?.[0];
          const bytes = await fileBytes(chosen);
          const updated = await ctx.api('notes', { action: 'attach', rkey: keyOf(record), name: chosen.name, mime: chosen.type || 'application/octet-stream', bytes });
          await loadList(); preserveDraft(updated); ctx.notify('File attached privately.');
        }, field('File to attach', attachFile, 'Up to 16 MiB. Removing an attachment reference does not erase earlier signed revisions.'))) : hint('Save this note to attach files or append text.'),
      body.provenance ? el('details', { class: 'detail' }, el('summary', { text: 'Original source' }), reference(body.provenance), hint('This note preserves the source revision used when it was created.')) : null,
      record ? reference(record) : null, record ? el('div', { class: 'actions' }, remove) : null);
  }

  async function showConversion() {
    const response = await ctx.api('records', { action: 'list' });
    sources = rows(response).filter(record => record.collection !== 'pub.tangled.note' && record.body && typeof record.body === 'object');
    if (!sources.length) { replace(convertArea, empty('No source items yet', 'Save an item in Capture or import a source before turning it into a note.')); return; }
    const picker = select([['', 'Choose a saved item'], ...sources.map(record => [record.uri, `${recordTitle(record)} · ${humanCollection(record.collection)}`])]);
    const title = input('', { maxLength: 512 });
    const state = select([['note', 'Note'], ['draft', 'Draft'], ['inbox', 'Self inbox']], 'inbox');
    const text = textarea('', { rows: 6 });
    const conversionStatus = status(), convert = submit('Create note from source');
    let sourceTicket = 0, sourceReady = false;
    picker.addEventListener('change', async () => {
      const ticket = ++sourceTicket;
      const record = sources.find(record => record.uri === picker.value);
      sourceReady = false; convert.disabled = false;
      if (!record) { title.value = text.value = ''; return; }
      title.value = recordTitle(record);
      text.value = typeof record.body.text === 'string' ? record.body.text : (typeof record.body.excerpt === 'string' ? record.body.excerpt : '');
      conversionStatus.textContent = '';
      if (record.collection === 'pub.tangled.capture') {
        conversionStatus.textContent = 'Reading source text…'; convert.disabled = true;
        try {
          const result = await ctx.api('capture', { action: 'read', uri: record.uri });
          if (ticket !== sourceTicket) return;
          text.value = result.text || ''; sourceReady = true; conversionStatus.textContent = '';
        } catch (error) { if (ticket === sourceTicket) conversionStatus.textContent = `${error.message || error} Choose the source again to retry.`; }
        finally { if (ticket === sourceTicket) convert.disabled = false; }
      } else sourceReady = true;
    });
    replace(convertArea, formWith(convert, conversionStatus, ctx, async () => {
      const record = sources.find(record => record.uri === picker.value);
      if (!record || !sourceReady) throw new Error('Choose an available source item first.');
      const updated = await ctx.api('notes', { action: 'convert', source: { collection: record.collection, rkey: keyOf(record) }, body: { ...(title.value ? { title: title.value } : {}), state: state.value, text: text.value } });
      stateFilter.value = ''; query.value = ''; await loadList(); renderEditor(updated); renderList();
      replace(convertArea, ); ctx.notify('Note created with its original source reference.');
    }, field('Source item', picker), field('Note title', title), field('Keep as', state), field('Note text', text, 'Edit the copied text before creating the note. The original item stays unchanged.'),
    button('Cancel conversion', () => { ++sourceTicket; replace(convertArea, ); })));
  }
  const conversionStatus = status();
  const fromSource = button('Create from saved item', () => void run(ctx, fromSource, conversionStatus, showConversion));
  replace(root, el('div', { class: 'stack' },
    el('div', { class: 'toolbar' }, button('New note', () => { renderEditor(); renderList(); }), button('New draft', () => { renderEditor(null, 'draft'); renderList(); }), button('Capture to self inbox', () => { renderEditor(null, 'inbox'); renderList(); }), fromSource),
    conversionStatus, convertArea,
    formWith(loadButton, listStatus, ctx, loadList, el('div', { class: 'form-grid' }, field('Show', stateFilter), field('Find in notes', query))),
    el('div', { class: 'split' }, el('section', { class: 'pane', 'aria-label': 'Your notes' }, count, list), editor)));
  query.addEventListener('input', renderList);
  renderEditor();
  try { await loadList(); }
  catch (error) { listStatus.textContent = `${error.message || error} Use Apply filters to try again.`; ctx.notify(error.message || String(error), 'error'); }
}
