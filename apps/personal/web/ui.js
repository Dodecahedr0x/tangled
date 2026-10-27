let nextId = 0;

export function el(tag, attrs = {}, ...children) {
  const node = document.createElement(tag);
  for (const [key, value] of Object.entries(attrs || {})) {
    if (value === undefined || value === null || value === false) continue;
    if (key === 'text') node.textContent = String(value);
    else if (key === 'class' || key === 'className') node.className = value;
    else if (key.startsWith('on') && typeof value === 'function') {
      node.addEventListener(key.slice(2).toLowerCase(), event => {
        try { Promise.resolve(value(event)).catch(report); } catch (error) { report(error); }
      });
    } else if (key === 'dataset') Object.assign(node.dataset, value);
    else if (key === 'style' && typeof value === 'object') Object.assign(node.style, value);
    else if (key in node && !key.startsWith('aria')) node[key] = value;
    else node.setAttribute(key, value === true ? '' : String(value));
  }
  const append = child => {
    if (child === undefined || child === null || child === false) return;
    if (Array.isArray(child)) child.forEach(append);
    else node.append(child instanceof Node ? child : document.createTextNode(String(child)));
  };
  children.forEach(append);
  return node;
}
function report(error) {
  window.dispatchEvent(new CustomEvent('personal-error', { detail: error?.message || String(error) }));
}
export function field(label, control, hint = '') {
  if (!control.id) control.id = `personal-field-${++nextId}`;
  const description = hint ? el('small', { class: 'hint', id: `${control.id}-hint`, text: hint }) : null;
  if (description) control.setAttribute('aria-describedby', description.id);
  return el('div', { class: 'field' }, el('label', { htmlFor: control.id, text: label }), control, description);
}
export function button(label, onClick, kind = 'secondary') {
  return el('button', { type: 'button', class: `button ${kind}`, onClick, text: label });
}
export function empty(title, description, action) {
  return el('section', { class: 'empty' }, el('h2', { text: title }), el('p', { text: description }), action);
}
export function recordTitle(record) {
  const body = record?.body || {};
  return body.title || body.name || body.text?.slice(0, 90) || body.imported?.object?.text?.slice(0, 90) || record?.collection || 'Untitled';
}
export function timeLabel(timestamp) {
  if (!timestamp) return '';
  const date = new Date(typeof timestamp === 'number' ? timestamp : String(timestamp));
  return Number.isNaN(date.valueOf()) ? String(timestamp) : new Intl.DateTimeFormat(undefined, { dateStyle: 'medium', timeStyle: 'short' }).format(date);
}
