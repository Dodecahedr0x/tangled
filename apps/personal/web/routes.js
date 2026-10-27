export const viewNames = Object.freeze(['today', 'library', 'capture', 'notes', 'feeds', 'connections', 'agents', 'devices', 'publish']);
const views = new Set(viewNames);
const modes = new Set(['note', 'draft', 'inbox']);
const recordPattern = /^tangled:\/\/did:sol:(?:[a-z0-9]+:)?[1-9A-HJ-NP-Za-km-z]{32,44}\/[A-Za-z0-9._-]{1,256}\/[A-Za-z0-9._~:!-]{1,256}$/;
const validRecord = uri => typeof uri === 'string' && recordPattern.test(uri);

export function parseRoute(hash = '') {
  const source = String(hash).replace(/^#/, '');
  const index = source.indexOf('?');
  const name = index < 0 ? source : source.slice(0, index);
  const view = views.has(name) ? name : 'today';
  const result = { view, selection: {}, error: '' };
  if (!views.has(name) || index < 0) return result;
  if (source.length > 4096) return { ...result, error: 'This record link is too long. Open the item from your library.' };
  const params = new URLSearchParams(source.slice(index + 1));
  if (params.has('item')) {
    const uri = params.get('item');
    if (params.getAll('item').length !== 1 || !validRecord(uri)) result.error = 'This record link is invalid. Open the item from your library.';
    else result.selection.uri = uri;
  } else if (view === 'notes' && modes.has(params.get('mode'))) result.selection.mode = params.get('mode');
  return result;
}

export function formatRoute(view, selection = {}) {
  if (!views.has(view)) throw new Error('Choose a known app view.');
  const params = new URLSearchParams();
  if (selection.uri) {
    if (!validRecord(selection.uri)) throw new Error('This record reference cannot be linked.');
    params.set('item', selection.uri);
  } else if (view === 'notes' && modes.has(selection.mode)) params.set('mode', selection.mode);
  return `#${view}${params.size ? '?' + params.toString() : ''}`;
}

export function recordLink(base, view, uri) {
  const url = new URL(base);
  if (!['http:', 'https:'].includes(url.protocol) || url.username || url.password) throw new Error('Use an app origin without URL credentials.');
  url.search = '';
  url.hash = formatRoute(view, { uri });
  return url.href;
}
