/* Account lifecycle controls: no source credentials or file content in browser storage. */
(() => {
  const uploadKey = 'tangled-resumable-upload-v1';
  let controller = null, stopped = false, canceled = false, activeUpload = null, pairing = null, pairingTimer = null, qrUrl = null;
  const fileDigest = async bytes => Array.from(new Uint8Array(await crypto.subtle.digest('SHA-256', bytes)), b => b.toString(16).padStart(2, '0')).join('');
  const saveUpload = value => { try { value ? localStorage.setItem(uploadKey, JSON.stringify(value)) : localStorage.removeItem(uploadKey); } catch {} };
  const savedUpload = () => { try { return JSON.parse(localStorage.getItem(uploadKey) || 'null'); } catch { return null; } };
  const uploadProgress = text => { $('#upload-progress').textContent = text; };
  function progress(status) {
    uploadProgress(`${status.received.toLocaleString()} / ${status.total.toLocaleString()} bytes · ${Math.floor(status.received * 100 / status.total)}%`);
  }
  const pending = savedUpload();
  if (pending) { activeUpload = pending.id; $('#upload-button').textContent = 'Resume upload'; $('#cancel-upload').hidden = false; uploadProgress('Choose the same export file to resume its saved upload.'); }
  $('#upload').onsubmit = async event => {
    event.preventDefault();
    const form = event.target, file = form.elements.file.files[0];
    if (!file) return;
    const button = $('#upload-button'); button.disabled = true; stopped = false; canceled = false;
    $('#pause-upload').hidden = false; $('#cancel-upload').hidden = false;
    try {
      if (!crypto.subtle) throw new Error('Use HTTPS or localhost to upload with integrity verification.');
      if (file.size > 256 * 1024 * 1024) throw new Error('This export exceeds 256 MiB. Export smaller batches.');
      uploadProgress('Checking the file before resuming…');
      const digest = await fileDigest(await file.arrayBuffer());
      if (canceled) throw new DOMException('Upload canceled', 'AbortError');
      let status, prior = savedUpload();
      if (prior && prior.sha256 === digest && prior.total === file.size) {
        try { status = await request(`/api/imports/uploads/${encodeURIComponent(prior.id)}`); }
        catch { saveUpload(null); activeUpload = null; }
        if (status && (status.source !== form.elements.source.value || status.account !== form.elements.account.value)) throw new Error('Choose the same source and account used to start this upload, or cancel it.');
      } else if (prior) throw new Error('A different export is waiting to resume. Select its original file or cancel the saved upload.');
      if (!status) status = await post('/api/imports/uploads', { source: form.elements.source.value, account: form.elements.account.value, filename: file.name, total: file.size, sha256: digest });
      if (canceled) { await post(`/api/imports/uploads/${encodeURIComponent(status.id)}/cancel`, {}); saveUpload(null); return; }
      activeUpload = status.id; saveUpload({ id: status.id, sha256: digest, total: file.size });
      progress(status);
      while (status.received < status.total && !stopped) {
        const offset = status.received;
        const chunk = await file.slice(offset, Math.min(file.size, offset + status.chunk_size)).arrayBuffer();
        const checksum = await fileDigest(chunk);
        controller = new AbortController();
        const response = await fetch(`/api/imports/uploads/${encodeURIComponent(status.id)}/chunks?offset=${offset}`, {
          method: 'PUT', credentials: 'same-origin', signal: controller.signal,
          headers: { 'Content-Type': 'application/octet-stream', 'X-Chunk-SHA256': checksum }, body: chunk
        });
        const next = await response.json();
        if (!response.ok) throw new Error(next.error || 'Chunk upload failed. Resume to retry.');
        status = next; progress(status);
      }
      if (stopped) { uploadProgress('Paused. Resume with the same file when ready.'); return; }
      uploadProgress('Upload complete. Reading and importing the verified export…'); $('#pause-upload').hidden = true;
      const complete = await post(`/api/imports/uploads/${encodeURIComponent(status.id)}/complete`, {});
      saveUpload(null); activeUpload = null; $('#cancel-upload').hidden = true;
      const r = complete.receipt;
      uploadProgress('Imported successfully.'); notice(`${r.inserted} items added, ${r.updated} updated, ${r.unchanged} unchanged. Review coverage notes in import history.`);
      await refresh();
    } catch (error) {
      if (canceled) uploadProgress('Canceled. Staged file bytes were removed.');
      else if (error.name === 'AbortError' || stopped) uploadProgress('Paused. Your acknowledged chunks remain saved.');
      else notice(`${error.message} A saved upload can be resumed with the same file.`, true);
    } finally {
      controller = null; button.disabled = false; button.textContent = activeUpload ? 'Resume upload' : 'Import export'; $('#pause-upload').hidden = true;
    }
  };
  $('#pause-upload').onclick = () => { stopped = true; controller?.abort(); };
  $('#cancel-upload').onclick = async () => {
    canceled = true; stopped = true; controller?.abort();
    const id = activeUpload || savedUpload()?.id;
    try { if (id) await post(`/api/imports/uploads/${encodeURIComponent(id)}/cancel`, {}); saveUpload(null); activeUpload = null; $('#cancel-upload').hidden = true; $('#upload-button').textContent = 'Import export'; uploadProgress('Canceled. Staged file bytes were removed.'); }
    catch (error) { notice(error.message, true); }
  };

  let availability = null;
  async function loadAvailability() {
    try { availability = await request('/api/imports/pairing'); updateAvailability(); }
    catch (error) { $('#pairing-availability').textContent = error.message; }
  }
  function updateAvailability() {
    const source = $('#hosted-pairing').elements.source.value;
    const item = availability?.sources?.find(s => s.source === source);
    const available = availability?.available && item?.available;
    $('#pairing-start').disabled = !available;
    $('#pairing-availability').textContent = available ? 'Sign in here. Your host will keep the connection running and store its source session privately.' : item?.reason || availability?.reason || 'Hosted pairing is not configured on this host. External connectors remain available.';
  }
  $('#hosted-pairing').elements.source.onchange = updateAvailability;
  function clearPairing() { clearTimeout(pairingTimer); pairingTimer = null; if (qrUrl) URL.revokeObjectURL(qrUrl); qrUrl = null; }
  function renderPairing(data) {
    const preserveInput = pairing?.id === data.id && pairing?.status === data.status && ['phone', 'code', 'password'].includes(data.status) && $('#pairing-steps').querySelector('form');
    pairing = data; const box = $('#pairing-steps');
    if (preserveInput) { schedulePairing(); return; }
    box.replaceChildren();
    $('#pairing-state').textContent = data.error || ({ phone: 'Enter your Telegram phone number.', code: 'Enter the login code from Telegram.', password: 'Enter your Telegram two-step verification password.', qr: 'Scan this QR in WhatsApp → Linked devices.', redirect: 'Authorize read access at X to continue.', authorized: 'Creating your account connection…', connected: 'Connected. New activity will be imported by this host.', expired: 'This sign-in expired. Start again.', failed: 'Connection failed. Start again.' }[data.status] || data.status);
    if (data.status === 'qr' && data.qr_svg) {
      if (qrUrl) URL.revokeObjectURL(qrUrl);
      qrUrl = URL.createObjectURL(new Blob([data.qr_svg], { type: 'image/svg+xml' }));
      const image = element('img'); image.src = qrUrl; image.alt = 'WhatsApp linked-device pairing QR code'; box.append(image);
    }
    if (data.status === 'redirect' && data.authorize_url) {
      const target = new URL(data.authorize_url);
      if (['https://x.com', 'https://twitter.com'].includes(target.origin)) { const link = element('a', 'Authorize at X'); link.href = target.href; box.append(link); }
    }
    if (['phone', 'code', 'password'].includes(data.status)) {
      const form = element('form'), label = element('label', data.status === 'phone' ? 'Phone number including country code' : data.status === 'code' ? 'Telegram login code' : 'Two-step verification password');
      const input = element('input'); input.name = data.status; input.type = data.status === 'phone' ? 'tel' : 'password'; input.autocomplete = 'off'; input.required = true; label.append(input);
      const submit = element('button', 'Continue'); form.append(label, submit);
      form.onsubmit = async event => { event.preventDefault(); submit.disabled = true; try { const value = input.value; input.value = ''; renderPairing(await post(`/api/imports/pairing/${encodeURIComponent(data.id)}/step`, { [data.status]: value })); } catch (error) { notice(error.message, true); } finally { submit.disabled = false; } };
      box.append(form);
    }
    if (!['connected', 'expired', 'failed'].includes(data.status)) {
      const cancel = element('button', 'Cancel sign-in', 'secondary'); cancel.type = 'button';
      cancel.onclick = async () => { try { await post('/api/imports/pairing/disconnect', { id: data.id }); clearPairing(); pairing = null; box.replaceChildren(); $('#pairing-state').textContent = 'Sign-in canceled and its retained authorization removed.'; } catch (error) { notice(error.message, true); } };
      box.append(cancel); schedulePairing();
    } else { clearPairing(); if (data.status === 'connected') refresh().catch(error => notice(error.message, true)); }
  }
  function schedulePairing() { clearTimeout(pairingTimer); pairingTimer = setTimeout(async () => { if (!pairing) return; try { renderPairing(await request(`/api/imports/pairing/${encodeURIComponent(pairing.id)}`)); } catch (error) { $('#pairing-state').textContent = error.message; } }, 2500); }
  $('#hosted-pairing').onsubmit = async event => { event.preventDefault(); clearPairing(); $('#pairing-start').disabled = true; try { renderPairing(await post('/api/imports/pairing/start', { source: event.target.elements.source.value })); } catch (error) { notice(error.message, true); } finally { updateAvailability(); } };
  loadAvailability();

  $('#open-erasure').onclick = async () => {
    try {
      const data = await request('/api/imports'), scope = $('#erase-scope'); scope.replaceChildren();
      const all = element('option', 'Entire imported archive'); all.value = 'all'; scope.append(all);
      const seen = new Set();
      for (const source of [...data.receipts, ...data.connections]) {
        const value = JSON.stringify({ source: source.source, account: source.account });
        if (seen.has(value)) continue; seen.add(value);
        const option = element('option', `${source.source} · ${source.account}`); option.value = value; scope.append(option);
      }
      $('#erase-confirmation').value = ''; $('#erase-submit').disabled = true; $('#erase-status').textContent = '';
      $('#erase-dialog').showModal(); $('#erase-confirmation').focus();
    } catch (error) { notice(error.message, true); }
  };
  $('#erase-dismiss').onclick = () => $('#erase-dialog').close();
  $('#erase-confirmation').oninput = event => { $('#erase-submit').disabled = event.target.value !== 'ERASE'; };
  $('#erase-form').onsubmit = async event => {
    event.preventDefault(); const button = $('#erase-submit'); button.disabled = true;
    $('#erase-status').textContent = 'Stopping hosted source sessions and permanently removing retained data…';
    try {
      const selection = $('#erase-scope').value, scope = selection === 'all' ? {} : JSON.parse(selection);
      const receipt = await post('/api/imports/erase', { ...scope, confirmation: $('#erase-confirmation').value });
      saveUpload(null); activeUpload = null; canceled = true; stopped = true; controller?.abort(); clearPairing(); pairing = null;
      $('#pairing-steps').replaceChildren(); $('#pairing-state').textContent = ''; $('#cancel-upload').hidden = true; $('#upload-button').textContent = 'Import export'; uploadProgress('');
      $('#erase-status').textContent = 'Erased from this host. Exported backups and source-app copies are unaffected.';
      notice('Selected imported data and retained hosted authorization were erased.');
      await refresh(); $('#files').replaceChildren(); $('#erase-dialog').close();
    } catch (error) { $('#erase-status').textContent = error.message; button.disabled = false; }
  };
})();
