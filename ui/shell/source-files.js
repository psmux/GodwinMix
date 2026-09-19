import { el } from './dom.js';
import { errorToast } from './toast.js';

export const MEDIA_ACCEPT = 'video/*,audio/*,image/*,.mkv,.ts,.mov,.webm,.wav,.flac,.m4a';

/** Unique ASCII names avoid replacing footage already held by the mixer. */
export function uploadName(file) {
  const suffix = /\.([a-z0-9]{1,8})$/i.exec(file.name || '');
  const extensions = { 'image/png': 'png', 'image/jpeg': 'jpg', 'image/webp': 'webp', 'audio/mpeg': 'mp3', 'audio/wav': 'wav', 'video/mp4': 'mp4' };
  const extension = suffix?.[1].toLowerCase() || extensions[file.type];
  if (!extension) throw new Error('This file has no media extension. Give it a video, audio or image extension and choose it again.');
  const stem = (file.name || 'media').replace(/\.[^.]*$/, '').normalize('NFKD').replace(/[^a-z0-9]+/gi, '-').replace(/^-+|-+$/g, '').slice(0, 80) || 'media';
  const random = [...crypto.getRandomValues(new Uint32Array(2))].map(x => x.toString(36)).join('');
  return `${stem}-${Date.now().toString(36)}-${random}.${extension}`;
}

/**
 * A real browser picker. onAdded receives the public source.add result and
 * should add that source to the scene captured when the chooser opened.
 * Closing the surface leaves an already requested upload batch running.
 */
export function sourceFiles(client, options = {}) {
  const input = el('input', { type: 'file', accept: options.accept || MEDIA_ACCEPT, multiple: options.multiple !== false, hidden: true });
  const button = el('button.btn', { text: options.label || 'Browse files', onclick: () => browse() });
  const status = el('p.sm.dim', { role: 'status', hidden: true });
  const progress = el('progress', { max: 1, value: 0, hidden: true, 'aria-label': 'Upload progress', style: { width: '100%' } });
  const results = el('div.col.sm', { role: 'log', 'aria-label': 'File source results', 'aria-live': 'polite' });
  const node = el('div.col.source-file-picker', {}, [button, input, status, progress, results]);
  let pending = null, disposed = false;
  const setStatus = text => { if (!disposed) { status.hidden = false; status.textContent = text; } };

  function browse() {
    if (!pending && !disposed) input.click();
  }
  async function run(files) {
    const answers = [];
    if (!files.length) return answers;
    button.disabled = true;
    progress.hidden = false;
    results.replaceChildren();
    for (let index = 0; index < files.length; index++) {
      const file = files[index];
      let source = null, phase = 'upload';
      try {
        const name = uploadName(file);
        setStatus(`Uploading ${file.name} (${index + 1} of ${files.length})`);
        const uploaded = await client.upload(name, file, fraction => {
          if (!disposed) progress.value = (index + Math.max(0, Math.min(1, Number(fraction) || 0))) / files.length;
        });
        if (typeof uploaded?.path !== 'string' || !uploaded.path.trim()) {
          throw new Error('The mixer did not return the uploaded path. Open Media to find the file before adding it as a source.');
        }
        phase = 'source';
        setStatus(`Adding ${file.name}`);
        source = await client.call('source.add', { uri: uploaded.path, name: file.name });
        phase = 'scene';
        if (options.onAdded) await options.onAdded(source);
        answers.push({ file, source });
        if (!disposed) results.append(el('div', { text: `${file.name}: added.` }));
      } catch (cause) {
        const detail = cause?.message || String(cause);
        const next = phase === 'scene' ? 'The source exists. Open Add sources to add it to the scene.'
          : phase === 'source' ? 'The file is uploaded. Open Media to retry adding it as a source.' : 'Choose the file again to retry.';
        const error = new Error(`${file.name}: ${detail} ${next}`);
        answers.push({ file, source, error, phase });
        if (!disposed) results.append(el('div', { text: error.message, role: 'alert' }));
        try {
          if (options.onError) await options.onError(error, { file, source, phase });
          else errorToast(error, 'Add file');
        } catch (reportError) { errorToast(reportError, 'Report file error'); }
      }
      if (!disposed) progress.value = (index + 1) / files.length;
    }
    const added = answers.filter(x => !x.error).length;
    setStatus(`${added} of ${files.length} files added.`);
    return answers;
  }
  function upload(files) {
    if (disposed && !pending) return Promise.resolve([]);
    if (pending) return pending;
    pending = run([...files]).finally(() => {
      pending = null;
      button.disabled = false;
      progress.hidden = true;
      input.value = '';
    });
    return pending;
  }
  input.addEventListener('change', () => { void upload(input.files || []); });
  return { node, input, button, status, progress, browse, upload,
    destroy() { disposed = true; node.remove(); },
  };
}
