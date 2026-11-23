// Prove an Ethereum fact on LEZ through Ferrata's attestation program (RFP-021). Filled in by prove-impl below.
export function mountProve(el, c) {
  el.innerHTML = '';
  const p = document.createElement('p');
  p.textContent = c.cfg.attestation?.program
    ? 'Loading…'
    : 'No attestation program is configured on this network yet (config.json: attestation.program).';
  el.append(p);
}
