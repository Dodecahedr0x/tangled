import { el, button, empty, recordTitle, timeLabel } from './ui.js';

const LIMIT = 10;
const rows = value => Array.isArray(value) ? value : value.records;
const keyOf = record => record.rkey || record.uri.split('/').pop();
const message = () => el('p', { class: 'inline-status', role: 'status', 'aria-live': 'polite' });
const meta = text => el('p', { class: 'meta', text });

export async function mountToday(root, ctx) {
  const sections = [];
  const definitions = [
    {
      title: 'Due reminders', feature: 'notes', request: { action: 'due' }, view: 'notes', open: 'Open Notes',
      emptyTitle: 'No reminders due', emptyText: 'Add a reminder to a note when something needs your attention later. Reminders are checked when you open or refresh this view.',
      detail: record => `Due: ${timeLabel(record.body.reminderAt * 1000)} · ${record.body.state === 'inbox' ? 'Self inbox' : record.body.state === 'draft' ? 'Draft' : 'Note'}`,
      action: 'Clear reminder', undo: 'Undo clear reminder', success: 'Reminder cleared',
      change: record => ({ action: 'edit', rkey: keyOf(record), patch: { reminderAt: null } }),
      restore: record => ({ action: 'edit', rkey: keyOf(record), patch: { reminderAt: record.body.reminderAt } }),
    },
    {
      title: 'Self inbox', feature: 'notes', request: { action: 'list', state: 'inbox' }, view: 'notes', open: 'Open Notes',
      emptyTitle: 'Your self inbox is clear', emptyText: 'Capture a thought to your self inbox, then file it to Notes when you are ready.',
      detail: record => record.body.text?.slice(0, 140) || 'Open this note to add text.',
      action: 'File to Notes', undo: 'Undo file to Notes', success: 'Filed to Notes',
      change: record => ({ action: 'edit', rkey: keyOf(record), patch: { state: 'note' } }),
      restore: record => ({ action: 'edit', rkey: keyOf(record), patch: { state: 'inbox' } }),
    },
    {
      title: 'Unread saved items', feature: 'capture', request: { action: 'list', read: false }, view: 'capture', open: 'Open Capture',
      emptyTitle: 'No unread saved items', emptyText: 'Save a link, an article, a file, or a passing thought in Capture. You can mark items unread there to return to them.',
      detail: record => [record.body.kind || 'Saved item', record.body.url].filter(Boolean).join(' · '),
      action: 'Mark read', undo: 'Undo mark saved item read', success: 'Saved item marked read',
      change: record => ({ action: 'mark-read', uri: record.uri }),
      restore: record => ({ action: 'mark-unread', uri: record.uri }),
    },
    {
      title: 'Unread feed articles', feature: 'feeds', request: { action: 'queue', unread: true, limit: LIMIT + 1 }, view: 'feeds', open: 'Open Reading',
      emptyTitle: 'No unread feed articles', emptyText: 'Add a subscription and import an RSS or Atom file in Reading. Feed articles here come from your supplied files.',
      detail: record => record.body.sourceUrl || 'Imported feed article',
      action: 'Mark read', undo: 'Undo mark feed article read', success: 'Feed article marked read',
      change: record => ({ action: 'mark', item: record.uri, read: true }),
      restore: record => ({ action: 'mark', item: record.uri, read: false }),
    },
  ];

  function makeSection(definition) {
    const list = el('ul', { class: 'list' }), summary = meta(''), loadingStatus = message(), actionStatus = message();
    const changes = el('div', { class: 'stack' });
    const state = { definition, records: null, generation: 0, mutating: false, undo: [] };
    const refresh = button(`Refresh ${definition.title.toLowerCase()}`, () => ctx.run(refresh, load));

    function renderList() {
      if (!state.records) return;
      const visible = state.records.slice(0, LIMIT);
      summary.textContent = state.records.length > LIMIT
        ? `Showing first ${LIMIT}${definition.feature === 'feeds' ? '; more are available in Reading.' : ` of ${state.records.length}. Open ${definition.view === 'notes' ? 'Notes' : 'Capture'} to see all.`}`
        : `${state.records.length} ${state.records.length === 1 ? 'item' : 'items'}`;
      if (!visible.length) {
        list.replaceChildren(el('li', {}, empty(definition.emptyTitle, definition.emptyText)));
        return;
      }
      list.replaceChildren(...visible.map(record => {
        const open = button(recordTitle(record), () => ctx.navigate(definition.view, { uri: record.uri }));
        open.className += ' item-title';
        const action = button(definition.action, () => change(action, record));
        action.disabled = state.mutating;
        return el('li', { class: 'item' },
          el('div', { class: 'item-main' }, open, meta(definition.detail(record))),
          el('div', { class: 'actions' }, action));
      }));
    }

    function renderChanges() {
      changes.replaceChildren(...state.undo.map(change => {
        const undo = button(definition.undo, () => perform(undo, async () => {
          await ctx.api(definition.feature, definition.restore(change.record));
          state.undo = state.undo.filter(entry => entry !== change);
          renderChanges();
          await refreshRelated();
          actionStatus.textContent = `Change undone: ${recordTitle(change.record)}.`;
          ctx.notify('Change undone.');
        }));
        undo.disabled = state.mutating;
        return el('div', { class: 'item' },
          el('div', { class: 'item-main' }, meta(`${definition.success}: ${recordTitle(change.record)}`)),
          el('div', { class: 'actions' }, undo));
      }));
    }

    async function perform(control, task) {
      if (state.mutating) return;
      return ctx.run(control, async () => {
        state.mutating = true;
        actionStatus.textContent = '';
        try { await task(); }
        catch (error) {
          actionStatus.textContent = `${error.message || error} Try the action again.`;
          throw error;
        } finally {
          state.mutating = false;
          renderList(); renderChanges();
        }
      });
    }

    async function change(control, record) {
      return perform(control, async () => {
        await ctx.api(definition.feature, definition.change(record));
        // Only changed standard fields are sent back; extension data stays in the repository.
        state.undo.push({ record });
        state.records = state.records.filter(item => item.uri !== record.uri);
        renderList(); renderChanges();
        await refreshRelated();
        actionStatus.textContent = `${definition.success}. You can undo this change below while Today remains open.`;
        ctx.notify(`${definition.success}.`);
      });
    }

    async function refreshRelated() {
      await Promise.all(sections.filter(section => section.definition.feature === definition.feature).map(section => section.load()));
    }

    async function load() {
      const generation = ++state.generation;
      loadingStatus.textContent = 'Loading…';
      try {
        const result = await ctx.api(definition.feature, definition.request);
        if (generation !== state.generation) return;
        const records = definition.feature === 'feeds' ? result.items : rows(result);
        if (!Array.isArray(records)) throw new Error('The repository returned an unreadable list.');
        state.records = records;
        renderList();
        loadingStatus.textContent = '';
        refresh.textContent = `Refresh ${definition.title.toLowerCase()}`;
      } catch (error) {
        if (generation !== state.generation) return;
        loadingStatus.textContent = `Could not load ${definition.title.toLowerCase()}. ${error.message || error}${state.records ? ' Previously loaded items are shown.' : ''}`;
        refresh.textContent = `Retry ${definition.title.toLowerCase()}`;
      }
    }

    state.load = load;
    state.node = el('section', { class: 'stack today-section', 'aria-label': definition.title },
      el('div', { class: 'toolbar' }, el('h2', { class: 'section-title', text: definition.title }),
        button(definition.open, () => ctx.navigate(definition.view)), refresh),
      summary, loadingStatus, list, actionStatus, changes);
    sections.push(state);
    return state.node;
  }

  root.replaceChildren(el('div', { class: 'stack today' },
    el('div', { class: 'toolbar' },
      button('New note', () => ctx.navigate('notes', { mode: 'note' }), 'primary'),
      button('Capture to self inbox', () => ctx.navigate('notes', { mode: 'inbox' })),
      button('Save an item', () => ctx.navigate('capture'))),
    meta('A short list from your shared repository. Open Notes, Capture, or Reading for the complete lists. Refresh to check for changes.'),
    ...definitions.map(makeSection)));
  await Promise.all(sections.map(section => section.load()));
}
