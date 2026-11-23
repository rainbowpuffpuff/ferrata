// Private accounts on LEZ v0.4: what the page can do in the browser and what proving needs.
export function mountPrivate(el, c) {
  el.innerHTML = '';
  const p = document.createElement('p');
  p.textContent = 'Private accounts: see below.';
  el.append(p);
}
