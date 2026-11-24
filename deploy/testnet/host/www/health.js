const $ = id => document.getElementById(id);
function fmtAge(s){ if(s==null) return ''; if(s<120) return s+' s ago'; if(s<7200) return Math.round(s/60)+' min ago'; return (s/3600).toFixed(1)+' h ago'; }
async function tick(){
  try{
    const s = await (await fetch('/status/status.json',{cache:'no-store'})).json();
    const l = s.lez||{}, b = s.bedrock||{}, at = s.attestation||{};
    // The updater's /health answers a list of instances (one here); an older one answered an object.
    const u = (Array.isArray(at.updater) ? at.updater[0] : at.updater) || {};
    $('utc').textContent = s.utc;
    $('head').textContent = l.head ?? '–'; $('headage').textContent = l.head_age_s!=null ? 'changed '+fmtAge(l.head_age_s) : (l.error||'');
    $('final').textContent = l.final ?? '–'; $('fage').textContent = fmtAge(l.final_age_s);
    $('nf').textContent = l.not_final ?? '–';
    $('bed').textContent = (b.slot ?? '–') + ' / ' + (b.height ?? '–');
    const fin = u.finalized_slot ?? (u.instance && u.instance.finalized_slot);
    $('sep').textContent = fin ?? '–';
    $('seplag').textContent = u.status || u.state || '';
    $('upd').textContent = u.last_applied_utc || u.last_update || '–';
    const a = s.alarms||[];
    // Built with textContent: an alarm's text is never parsed as HTML.
    const list = $('alarmlist'); list.replaceChildren();
    if (!a.length) { const n = document.createElement('span'); n.className = 'ok'; n.textContent = '● none'; list.append(n); }
    for (const x of a) {
      const row = document.createElement('div'); row.className = x.level === 'critical' ? 'bad' : 'warn';
      const what = document.createElement('code'); what.textContent = String(x.what ?? '');
      row.append(`● ${String(x.level ?? '')}: `, what, ` ${typeof x.detail === 'string' ? x.detail : ''}`);
      list.append(row);
    }
    $('raw').textContent = JSON.stringify(s, null, 2);
    const h = await (await fetch('/status/history.json',{cache:'no-store'})).json();
    const pts = h.filter(p=>p.final_age_s!=null); if(pts.length>1){
      const max = Math.max(60, ...pts.map(p=>p.final_age_s)); const n = pts.length;
      const d = pts.map((p,i)=>`${i?'L':'M'}${(i/(n-1)*958+1).toFixed(1)},${(118 - p.final_age_s/max*110).toFixed(1)}`).join(' ');
      $('spark').innerHTML = `<path d="${d}" fill="none" stroke="currentColor" stroke-width="1.5"/><text x="4" y="12" font-size="11" fill="currentColor">max ${max} s</text>`;
    }
  }catch(e){ $('raw').textContent = 'status not available yet: ' + e; }
}
tick(); setInterval(tick, 15000);
