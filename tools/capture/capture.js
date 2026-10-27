// Installer and portable bookmarklet source. Reads only visible page text.
window.captureCurrentPage = function captureCurrentPage() {
  if (!/^https?:\/\//.test(window.location.href)) {
    alert('Open an HTTP(S) page to capture it.');
    return;
  }
  const selection = String(window.getSelection() || '').slice(0, 65536);
  const article = document.querySelector('article') || document.querySelector('main') || document.body;
  const text = String(article ? article.innerText || '' : '').slice(0, 1048576);
  const payload = {
    action: 'save',
    kind: text.trim() ? 'article' : 'bookmark',
    url: window.location.href,
    title: document.title.slice(0, 1024),
    source: { type: 'browser', selection, extractor: 'visible-text-v1' }
  };
  if (text.trim()) payload.text = text;
  const url = URL.createObjectURL(new Blob([JSON.stringify(payload, null, 2)], { type: 'application/json' }));
  const link = document.createElement('a');
  link.href = url;
  link.download = 'tangled-capture.json';
  document.body.appendChild(link);
  link.click();
  link.remove();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
};
const installerLink = document.getElementById('capture-bookmarklet');
if (installerLink) installerLink.href = 'javascript:(' + window.captureCurrentPage.toString() + ')()';
