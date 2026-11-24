// QR codes: the receive code drawn as a QR (optionally with an amount asked for), and a scanner that fills the Send
// form from a QR: the browser's own BarcodeDetector where it has one (Android's Chrome), otherwise paulmillr's decoder
// (vendor/qr.js) on camera frames, and the same decoder on a picture for devices without a camera for the page. Frames
// are read in this page and nothing is sent anywhere.
const NS = 'http://www.w3.org/2000/svg';
const B58 = '[1-9A-HJ-NP-Za-km-z]';
let lib = null;
const qrLib = () => (lib ??= import('./vendor/qr.js'));

/** `lez:<id>?pk=<hex>[&amount=<decimal>]`, a bare id, a public key or a name; null when it is none of these. */
export function parseScanned(text, decimals = 9) {
  const t = String(text ?? '').trim();
  const m = t.match(new RegExp(`^lez:(${B58}{32,44})(?:\\?(.*))?$`));
  if (m) {
    const p = new URLSearchParams(m[2] ?? '');
    const pk = p.get('pk'), amount = p.get('amount');
    if (pk !== null && !/^[0-9a-fA-F]{64}$/.test(pk)) return null;
    const okAmount = amount !== null && new RegExp(`^\\d{1,20}(\\.\\d{1,${decimals}})?$`).test(amount);
    return { to: pk ? `lez:${m[1]}?pk=${pk.toLowerCase()}` : m[1], amount: okAmount ? amount : null };
  }
  if (new RegExp(`^${B58}{32,44}$`).test(t) || /^(0x)?[0-9a-fA-F]{64}$/.test(t) || /^[a-zA-Z0-9-]{3,32}(\.lez)?$/.test(t)) return { to: t, amount: null };
  return null;
}

/** The receive code, with `&amount=` when one is asked for. */
export function receiveCode(id, pkHex, amount = '') {
  return `lez:${id}?pk=${pkHex}${amount ? '&amount=' + amount : ''}`;
}

/** Draws `text` as a QR (error correction M, 4-module quiet zone) into `el`, as one SVG path. */
export async function renderQR(el, text, label = 'QR code') {
  const { encodeQR } = await qrLib();
  const m = encodeQR(text, 'raw', { ecc: 'medium', border: 4 });
  const n = m.length;
  const svg = document.createElementNS(NS, 'svg');
  svg.setAttribute('viewBox', `0 0 ${n} ${n}`);
  svg.setAttribute('shape-rendering', 'crispEdges');
  svg.setAttribute('role', 'img');
  svg.setAttribute('aria-label', label);
  const bg = document.createElementNS(NS, 'rect');
  bg.setAttribute('width', n); bg.setAttribute('height', n); bg.setAttribute('fill', '#fff');
  let d = '';
  for (let y = 0; y < n; y++) for (let x = 0; x < n; x++) if (m[y][x]) d += `M${x} ${y}h1v1h-1z`;
  const path = document.createElementNS(NS, 'path');
  path.setAttribute('d', d); path.setAttribute('fill', '#000');
  svg.append(bg, path);
  el.replaceChildren(svg);
  el.dataset.text = text;
  return n;
}

let canvas = null;
/** Decodes a QR from a video frame, an image or an ImageBitmap; null when there is none. */
export async function decodeImage(src, w, h) {
  const { decodeQR } = await qrLib();
  const scale = Math.min(1, 1000 / Math.max(w, h));
  const cw = Math.max(1, Math.round(w * scale)), ch = Math.max(1, Math.round(h * scale));
  canvas ??= document.createElement('canvas');
  canvas.width = cw; canvas.height = ch;
  const ctx = canvas.getContext('2d', { willReadFrequently: true });
  ctx.drawImage(src, 0, 0, cw, ch);
  try { return decodeQR({ width: cw, height: ch, data: ctx.getImageData(0, 0, cw, ch).data }); } catch { return null; }
}

/**
 * The scanner: `button` opens the camera, `file` decodes a picture; `onText(text)` gets what was read.
 * Elements of the overlay: #scanner, #scan-video, #scan-cancel, #scan-hint.
 */
export function mountScanner({ button, file, onText, say }) {
  const overlay = document.getElementById('scanner'), video = document.getElementById('scan-video');
  let stream = null, timer = 0, detector = null, busy = false;
  const stop = () => {
    clearTimeout(timer); timer = 0;
    stream?.getTracks().forEach((t) => t.stop()); stream = null;
    video.srcObject = null; overlay.hidden = true;
  };
  document.getElementById('scan-cancel').onclick = stop;
  document.addEventListener('visibilitychange', () => { if (document.visibilityState === 'hidden' && stream) stop(); });
  async function tick() {
    if (!stream) return;
    if (!busy && video.readyState >= 2 && video.videoWidth) {
      busy = true;
      try {
        let text = null;
        if (detector) text = (await detector.detect(video))[0]?.rawValue ?? null;
        else text = await decodeImage(video, video.videoWidth, video.videoHeight);
        if (text) { stop(); onText(text); return; }
      } catch { /* the next frame */ } finally { busy = false; }
    }
    timer = setTimeout(tick, 120);
  }
  button.onclick = async () => {
    say('');
    if (!navigator.mediaDevices?.getUserMedia) { say('This browser gives the page no camera here: use “From a photo”.', 'warn'); return; }
    overlay.hidden = false;
    try {
      stream = await navigator.mediaDevices.getUserMedia({ audio: false, video: { facingMode: { ideal: 'environment' }, width: { ideal: 1280 }, height: { ideal: 720 } } });
    } catch (e) {
      stop();
      say(e.name === 'NotAllowedError' ? 'The camera was not allowed. Allow it in the browser\'s site settings, or use “From a photo”.' : `No camera: ${e.message}`, 'warn');
      return;
    }
    video.srcObject = stream;
    await video.play().catch(() => {});
    detector = null;
    if ('BarcodeDetector' in globalThis) {
      try { if ((await BarcodeDetector.getSupportedFormats()).includes('qr_code')) detector = new BarcodeDetector({ formats: ['qr_code'] }); } catch { /* the JS decoder */ }
    }
    tick();
  };
  file.onchange = async () => {
    const f = file.files?.[0]; file.value = '';
    if (!f) return;
    say('Reading the picture…');
    try {
      const bmp = await createImageBitmap(f);
      const text = await decodeImage(bmp, bmp.width, bmp.height);
      bmp.close?.();
      if (!text) { say('No QR code found in that picture.', 'warn'); return; }
      onText(text);
    } catch (e) { say(`Could not read that picture: ${e.message}`, 'bad'); }
  };
  return { stop };
}
