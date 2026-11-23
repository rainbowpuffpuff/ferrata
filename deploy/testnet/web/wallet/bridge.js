// The bridge (RFP-022): deposit on Sepolia from the user's own browser wallet, claim privately on LEZ, burn, release.
export function mountBridge(el, c) {
  el.innerHTML = '';
  const p = document.createElement('p');
  p.textContent = c.cfg.bridge?.vault ? 'Loading…' : 'No bridge vault is configured on this network yet (config.json: bridge.vault).';
  el.append(p);
}
